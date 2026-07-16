use assert_cmd::Command;
use predicates::str::contains;
use rusqlite::Connection;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

#[test]
fn domain_and_protocol_boundaries_do_not_write_to_process_streams() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/domain.rs",
        "src/storage.rs",
        "src/application.rs",
        "src/app/memory.rs",
        "src/app/graph_store.rs",
        "src/protocol.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            !source.contains("println!(") && !source.contains("eprintln!("),
            "lower layer {relative} must return data/errors instead of writing process streams"
        );
    }

    let http = std::fs::read_to_string(root.join("src/app/http_server.rs")).unwrap();
    let mcp = std::fs::read_to_string(root.join("src/app/mcp_transport.rs")).unwrap();
    assert!(http.contains("dukememory::protocol::http_content_length"));
    assert!(mcp.contains("dukememory::protocol"));
}

fn cmd(db: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("dukememory").unwrap();
    command.arg("--db").arg(db);
    command.env("DUKEMEMORY_EMBED_PROVIDER", "mock");
    command.env("DUKEMEMORY_GEN_PROVIDER", "mock");
    command
}

fn stdout(command: &mut Command) -> String {
    String::from_utf8(command.assert().success().get_output().stdout.clone()).unwrap()
}

fn http_once(db: &std::path::Path, request: &str) -> String {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
        .env("DUKEMEMORY_EMBED_PROVIDER", "mock")
        .env("DUKEMEMORY_GEN_PROVIDER", "mock")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut url = String::new();
    reader.read_line(&mut url).unwrap();
    let port = url
        .trim()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(stream, "{request}").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(child.wait().unwrap().success());
    response
}

#[test]
fn http_memory_mutations_enforce_domain_invariants() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let operations = http_once(
        &db,
        "GET /operations HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(operations.starts_with("HTTP/1.1 200 OK"));
    assert!(operations.contains("X-Content-Type-Options: nosniff"));
    assert!(operations.contains("X-Frame-Options: DENY"));
    assert!(operations.contains("Content-Security-Policy: default-src 'none'"));
    assert!(operations.contains("script-src 'self'; script-src-attr 'none'"));
    assert!(!operations.contains("unsafe-inline"));
    assert!(operations.contains("Cache-Control: no-store"));
    assert!(operations.contains(r#""id":"memory.create""#));
    assert!(operations.contains(r#""memory_add""#));
    assert!(operations.contains(r#""/remember""#));
    let post = |path: &str, body: &str| {
        http_once(
            &db,
            &format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
    };

    for (path, body) in [
        (
            "/remember",
            r#"{"text":"valid text","type":"unknown_type"}"#,
        ),
        ("/remember", r#"{"text":"valid text","scope":"workspace"}"#),
        (
            "/remember",
            r#"{"text":"api_key: secret-value","scope":"project"}"#,
        ),
        ("/memory/status", r#"{"id":"missing","status":"deleted"}"#),
        ("/memory/update", r#"{"id":"missing","scope":"workspace"}"#),
    ] {
        let response = post(path, body);
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "unexpected response for {path}: {response}"
        );
        assert!(response.contains(r#""code":"bad_request""#));
    }

    let missing = post("/memory/status", r#"{"id":"missing","status":"active"}"#);
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"));

    let conn = Connection::open(&db).unwrap();
    let stored: i64 = conn
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap();
    assert_eq!(stored, 0);
}

#[test]
fn inferred_memory_edges_are_atomic_idempotent_and_bidirectional_for_graph_rag() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Upstream zinnia evidence")
            .arg("The upstream evidence explicitly references zzzz9999.")
            .arg("--id")
            .arg("aaaa1111"),
    );
    stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Terminal orchid anchor")
            .arg("Terminal orchid anchor is the selected graph seed.")
            .arg("--id")
            .arg("zzzz9999"),
    );

    let applied: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("memory-graph-links")
            .arg("--root")
            .arg(dir.path())
            .arg("--apply")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(applied["applied_count"], 1);

    let conn = Connection::open(&db).unwrap();
    let edge: (String, String, String, f64, String) = conn
        .query_row(
            "SELECT source_id, target_id, kind, confidence, provenance FROM memory_edges",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(edge.0, "aaaa1111");
    assert_eq!(edge.1, "zzzz9999");
    assert_eq!(edge.2, "relates_to");
    assert!(edge.3 >= 0.92);
    assert!(edge.4.contains("explicit_memory_id_mention"));
    drop(conn);

    let repeated: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("memory-graph-links")
            .arg("--root")
            .arg(dir.path())
            .arg("--apply")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(repeated["applied_count"], 0);

    let graph: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("graph-rag")
            .arg("terminal orchid anchor")
            .arg("--provider")
            .arg("mock")
            .arg("--json"),
    ))
    .unwrap();
    assert!(
        graph["relevant_nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == "aaaa1111")
    );

    let rollback_dir = tempdir().unwrap();
    let rollback_db = rollback_dir.path().join("memory.db");
    stdout(
        cmd(&rollback_db)
            .arg("add")
            .arg("note")
            .arg("First rollback node")
            .arg("This node references rollback02.")
            .arg("--id")
            .arg("rollback01"),
    );
    stdout(
        cmd(&rollback_db)
            .arg("add")
            .arg("note")
            .arg("Second rollback node")
            .arg("A distinct target for atomic graph writes.")
            .arg("--id")
            .arg("rollback02"),
    );
    let conn = Connection::open(&rollback_db).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_graph_audit BEFORE INSERT ON memory_events \
         WHEN NEW.event_type = 'memory_graph_links' \
         BEGIN SELECT RAISE(ABORT, 'forced graph audit failure'); END;",
    )
    .unwrap();
    drop(conn);
    cmd(&rollback_db)
        .arg("memory-graph-links")
        .arg("--root")
        .arg(rollback_dir.path())
        .arg("--apply")
        .arg("--json")
        .assert()
        .failure()
        .stderr(contains("forced graph audit failure"));
    let conn = Connection::open(&rollback_db).unwrap();
    let stored_edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_edges", [], |row| row.get(0))
        .unwrap();
    assert_eq!(stored_edges, 0);
}

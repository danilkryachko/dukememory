use assert_cmd::Command;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

const CORE_OPERATION_IDS: &[&str] = &[
    "memory.create",
    "memory.get",
    "memory.search",
    "memory.update",
    "memory.status",
    "memory.delete",
];

fn command(db: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("dukememory"));
    command
        .arg("--db")
        .arg(db)
        .env("DUKEMEMORY_EMBED_PROVIDER", "mock")
        .env("DUKEMEMORY_GEN_PROVIDER", "mock");
    command
}

fn stdout(command: &mut Command) -> String {
    String::from_utf8(command.assert().success().get_output().stdout.clone()).unwrap()
}

fn http_json(db: &std::path::Path, method: &str, path: &str, body: Option<&Value>) -> Value {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin!("dukememory"))
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
    let stdout_pipe = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout_pipe);
    let mut url = String::new();
    reader.read_line(&mut url).unwrap();
    let port = url
        .trim()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let encoded = body.map(Value::to_string).unwrap_or_default();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{encoded}",
        encoded.len()
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "unexpected HTTP response: {response}"
    );
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn mcp_tool_names(db: &std::path::Path) -> Vec<String> {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin!("dukememory"))
        .arg("--db")
        .arg(db)
        .arg("serve-mcp")
        .env("DUKEMEMORY_EMBED_PROVIDER", "mock")
        .env("DUKEMEMORY_GEN_PROVIDER", "mock")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})
    )
    .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(ToOwned::to_owned))
        .collect()
}

#[test]
fn schema_v21_migrates_then_survives_verified_backup_restore() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("legacy-v21.db");
    let restored = directory.path().join("restored.db");
    let backups = directory.path().join("backups");
    let rollback_dir = directory.path().join("restore-rollbacks");
    let journal_dir = directory.path().join("restore-journal");

    let first_id = stdout(
        command(&db)
            .arg("add")
            .arg("decision")
            .arg("Migration source")
            .arg("This v21 record must survive migration and restore."),
    )
    .trim()
    .to_string();
    let second_id = stdout(
        command(&db)
            .arg("add")
            .arg("design_note")
            .arg("Migration target")
            .arg("This related record must preserve its graph edge."),
    )
    .trim()
    .to_string();

    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;\
             DROP TABLE memory_edges;\
             DELETE FROM schema_versions WHERE version = 22;",
        )
        .unwrap();
    drop(connection);

    command(&db).arg("schema").arg("verify").assert().success();
    let connection = Connection::open(&db).unwrap();
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_versions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(version, 22);
    let (source_id, target_id) = if first_id < second_id {
        (&first_id, &second_id)
    } else {
        (&second_id, &first_id)
    };
    connection
        .execute(
            "INSERT INTO memory_edges(source_id, target_id, kind, confidence, provenance, created_at) VALUES (?1, ?2, 'relates_to', 0.95, ?3, 1)",
            params![source_id, target_id, r#"{"source":"compatibility_gate"}"#],
        )
        .unwrap();
    drop(connection);

    let backup_report: Value = serde_json::from_str(&stdout(
        command(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(backup_report["verified"], true);
    assert_eq!(backup_report["backup_integrity_ok"], true);
    let backup = std::path::PathBuf::from(backup_report["created"].as_str().unwrap());

    command(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--no-rollback")
        .arg("--rollback-dir")
        .arg(&rollback_dir)
        .arg("--journal-dir")
        .arg(&journal_dir)
        .assert()
        .success();
    command(&restored)
        .arg("schema")
        .arg("verify")
        .assert()
        .success();

    let restored_connection = Connection::open(&restored).unwrap();
    let memory_count: i64 = restored_connection
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap();
    let edge_count: i64 = restored_connection
        .query_row("SELECT COUNT(*) FROM memory_edges", [], |row| row.get(0))
        .unwrap();
    assert_eq!(memory_count, 2);
    assert_eq!(edge_count, 1);
}

#[test]
fn legacy_read_events_gain_session_link_before_session_index_creation() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("legacy-read-events.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE memory_read_events (\
                id INTEGER PRIMARY KEY AUTOINCREMENT, command TEXT NOT NULL, query TEXT NOT NULL, \
                memory_ids TEXT NOT NULL DEFAULT '', semantic_used INTEGER NOT NULL DEFAULT 0, \
                result_count INTEGER NOT NULL DEFAULT 0, budget INTEGER NOT NULL DEFAULT 0, \
                elapsed_ms INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL\
            );",
        )
        .unwrap();
    drop(connection);

    command(&db).arg("schema").arg("verify").assert().success();
    let connection = Connection::open(&db).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(memory_read_events)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(columns.contains(&"session_id".to_string()));
    let schema: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_versions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(schema, 22);
}

#[test]
fn legacy_agent_sessions_gain_leases_and_monotonic_event_sequences() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("legacy-agent-sessions.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE agent_sessions (\
                id TEXT PRIMARY KEY, task TEXT NOT NULL, target TEXT, scope TEXT NOT NULL DEFAULT 'project', \
                runner_profile TEXT, status TEXT NOT NULL DEFAULT 'active', outcome TEXT, summary TEXT, \
                changed_files TEXT NOT NULL DEFAULT '[]', validation_commands TEXT NOT NULL DEFAULT '[]', \
                commit_hash TEXT, memory_ids TEXT NOT NULL DEFAULT '[]', feedback_written INTEGER NOT NULL DEFAULT 0, \
                started_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, finished_at INTEGER\
            );\
            CREATE TABLE agent_session_events (\
                id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, event_type TEXT NOT NULL, \
                detail TEXT NOT NULL, created_at INTEGER NOT NULL\
            );\
            INSERT INTO agent_sessions (id, task, started_at, updated_at) \
                VALUES ('legacy-session', 'migrate legacy session', 100, 200);\
            INSERT INTO agent_session_events (session_id, event_type, detail, created_at) \
                VALUES ('legacy-session', 'started', '{}', 100);\
            INSERT INTO agent_session_events (session_id, event_type, detail, created_at) \
                VALUES ('legacy-session', 'context_loaded', '{}', 150);",
        )
        .unwrap();
    drop(connection);

    command(&db).arg("schema").arg("verify").assert().success();
    let connection = Connection::open(&db).unwrap();
    let session_columns = connection
        .prepare("PRAGMA table_info(agent_sessions)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for expected in [
        "lease_owner",
        "lease_token",
        "current_attempt_id",
        "lease_expires_at",
        "attempt_count",
        "last_event_sequence",
        "last_heartbeat_at",
    ] {
        assert!(session_columns.contains(&expected.to_string()));
    }
    let event_columns = connection
        .prepare("PRAGMA table_info(agent_session_events)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for expected in ["event_id", "sequence", "attempt_id"] {
        assert!(event_columns.contains(&expected.to_string()));
    }
    let sequences = connection
        .prepare(
            "SELECT sequence FROM agent_session_events WHERE session_id = 'legacy-session' ORDER BY sequence",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(sequences, vec![1, 2]);
    let last_sequence: i64 = connection
        .query_row(
            "SELECT last_event_sequence FROM agent_sessions WHERE id = 'legacy-session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(last_sequence, 2);
}

#[test]
fn core_cli_mcp_and_http_contracts_remain_callable() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("contracts.db");

    let help = stdout(command(&db).arg("--help"));
    for command_name in [
        "operations",
        "add",
        "remember",
        "get",
        "search",
        "update",
        "status",
        "delete",
    ] {
        assert!(
            help.contains(command_name),
            "missing CLI command {command_name}"
        );
    }

    let mcp_tools = mcp_tool_names(&db);
    for tool in [
        "memory_operations",
        "memory_add",
        "memory_remember",
        "memory_get",
        "memory_search",
    ] {
        assert!(
            mcp_tools.iter().any(|name| name == tool),
            "missing MCP tool {tool}"
        );
    }

    let catalog = http_json(&db, "GET", "/operations", None);
    let operation_ids = catalog["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|operation| operation["id"].as_str())
        .collect::<Vec<_>>();
    assert!(operation_ids.len() >= 25);
    for operation_id in CORE_OPERATION_IDS {
        assert!(
            operation_ids.contains(operation_id),
            "missing core operation {operation_id}"
        );
    }
    let cli_catalog: Value =
        serde_json::from_str(&stdout(command(&db).arg("operations").arg("--json"))).unwrap();
    assert_eq!(cli_catalog, catalog["operations"]);

    let remembered = http_json(
        &db,
        "POST",
        "/remember",
        Some(&serde_json::json!({
            "text": "HTTP compatibility contract memory",
            "type": "decision"
        })),
    );
    let id = remembered["id"].as_str().unwrap();
    let listed = http_json(&db, "GET", "/memory?q=compatibility", None);
    assert!(listed.to_string().contains(id));

    let updated = http_json(
        &db,
        "POST",
        "/memory/update",
        Some(&serde_json::json!({"id": id, "title": "Updated compatibility contract"})),
    );
    assert_eq!(updated["ok"], true);
    let status = http_json(
        &db,
        "POST",
        "/memory/status",
        Some(&serde_json::json!({"id": id, "status": "uncertain"})),
    );
    assert_eq!(status["status"], "uncertain");
    let search = http_json(
        &db,
        "POST",
        "/search",
        Some(&serde_json::json!({"query": "compatibility contract"})),
    );
    assert!(search.to_string().contains(id));
    let deleted = http_json(
        &db,
        "POST",
        "/memory/delete",
        Some(&serde_json::json!({"id": id})),
    );
    assert_eq!(deleted["ok"], true);
}

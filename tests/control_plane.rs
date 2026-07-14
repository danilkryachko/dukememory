use assert_cmd::Command;
use rusqlite::{Connection, params};
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

fn cmd(db: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("dukememory").unwrap();
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

struct Server {
    child: std::process::Child,
    port: u16,
}

impl Server {
    fn start(db: &std::path::Path) -> Self {
        let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
            .arg("--db")
            .arg(db)
            .arg("serve-http")
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg("0")
            .env("DUKEMEMORY_EMBED_PROVIDER", "mock")
            .env("DUKEMEMORY_GEN_PROVIDER", "mock")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut url = String::new();
        reader.read_line(&mut url).unwrap();
        let port = url.trim().rsplit(':').next().unwrap().parse().unwrap();
        std::thread::spawn(move || {
            let mut sink = std::io::sink();
            let _ = std::io::copy(&mut reader, &mut sink);
        });
        Self { child, port }
    }

    fn request(&self, path: &str) -> Value {
        request_json(self.port, path)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn request_json(port: u16, path: &str) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(60)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn stable_control_snapshot_is_cached_and_concurrency_safe() {
    let dir = tempdir().unwrap();
    let agent = dir.path().join(".agent");
    fs::create_dir_all(&agent).unwrap();
    let db = agent.join("memory.db");
    cmd(&db).arg("schema").arg("verify").assert().success();

    let server = Server::start(&db);
    let first = server.request("/web-control-center?since_days=7");
    assert_eq!(first["version"], 1);
    assert_eq!(first["current_version"], "stable-v1");
    assert_eq!(first["cache"]["hit"], false);
    assert_eq!(first["request_budget"]["initial_requests"], 1);
    assert_eq!(first["request_budget"]["legacy_fanout"], false);
    assert_eq!(
        first["compatibility"]["canonical_endpoint"],
        "/web-control-center"
    );
    let panel_names = first["panels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|panel| panel["name"].as_str())
        .collect::<Vec<_>>();
    assert!(panel_names.contains(&"rag_eval"));
    assert!(panel_names.contains(&"diff_impact"));
    assert!(
        first["summary"]["rag"]["near_miss_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        first["summary"]["diff_impact"]["write_ready_count"]
            .as_u64()
            .is_some()
    );

    let second = server.request("/web-control-center?since_days=7");
    assert_eq!(second["cache"]["hit"], true);
    assert_eq!(second["revision"], first["revision"]);
    assert!(second["cache"]["age_ms"].as_u64().unwrap() <= 3_000);

    let handles = (0..4)
        .map(|_| {
            let port = server.port;
            std::thread::spawn(move || request_json(port, "/web-control-center?since_days=7"))
        })
        .collect::<Vec<_>>();
    for handle in handles {
        let value = handle.join().unwrap();
        assert_eq!(value["revision"], first["revision"]);
        assert_eq!(value["cache"]["hit"], true);
    }

    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("cache revision")
        .arg("A durable database mutation invalidates the stable snapshot cache.")
        .assert()
        .success();
    let invalidated = server.request("/web-control-center?since_days=7");
    assert_eq!(invalidated["cache"]["hit"], false);
    assert_ne!(invalidated["revision"], first["revision"]);

    let details = server.request("/web-control-center?view=details&since_days=7");
    assert!(details["control_v12"].is_object());
    assert_eq!(details["current_version"], "stable-v1");
    let legacy = server.request("/web-control-center-v12?since_days=7");
    assert_eq!(legacy["deprecated"], true);
    assert_eq!(
        legacy["canonical_endpoint"],
        "/web-control-center?view=details"
    );
}

#[test]
fn session_pages_and_policy_cleanup_cover_terminal_states() {
    let dir = tempdir().unwrap();
    let agent = dir.path().join(".agent");
    fs::create_dir_all(&agent).unwrap();
    let db = agent.join("memory.db");
    let config = agent.join("config.toml");
    cmd(&db)
        .arg("init")
        .arg("--config")
        .arg(&config)
        .assert()
        .success();
    let config_text = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        config_text.replace("default_page_size = 20", "default_page_size = 1"),
    )
    .unwrap();

    let mut ids = Vec::new();
    for (task, outcome) in [
        ("completed session", "success"),
        ("failed session", "failed"),
        ("abandoned session", "abandoned"),
    ] {
        let started: Value = serde_json::from_str(&stdout(
            cmd(&db)
                .arg("--config")
                .arg(&config)
                .arg("agent-session")
                .arg("start")
                .arg(task)
                .arg("--json"),
        ))
        .unwrap();
        let id = started["id"].as_str().unwrap().to_string();
        let finished: Value = serde_json::from_str(&stdout(
            cmd(&db)
                .arg("--config")
                .arg(&config)
                .arg("agent-session")
                .arg("finish")
                .arg(&id)
                .arg("--outcome")
                .arg(outcome)
                .arg("--summary")
                .arg(format!("{task} finished"))
                .arg("--json"),
        ))
        .unwrap();
        assert_eq!(
            finished["session"]["attempt_state"],
            finished["session"]["status"]
        );
        ids.push((id, outcome.to_string()));
    }

    let old = 1_700_000_000_000i64;
    let conn = Connection::open(&db).unwrap();
    for (id, _) in &ids {
        conn.execute(
            "UPDATE agent_sessions SET updated_at = ?1, finished_at = ?1 WHERE id = ?2",
            params![old, id],
        )
        .unwrap();
    }

    let page: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("--config")
            .arg(&config)
            .arg("agent-session")
            .arg("status")
            .arg("--status")
            .arg("failed")
            .arg("--status")
            .arg("abandoned")
            .arg("--page")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["limit"], 1);
    assert_eq!(page["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(page["has_more"], true);
    assert_eq!(
        page["sessions"][0]["attempt_state"],
        page["sessions"][0]["status"]
    );

    let server = Server::start(&db);
    let http_page = server.request("/agent-sessions?status=failed,abandoned&limit=1&offset=0");
    assert_eq!(http_page["pagination"]["total"], 2);
    assert_eq!(http_page["pagination"]["has_more"], true);
    assert_eq!(http_page["sessions"].as_array().unwrap().len(), 1);
    drop(server);

    let preview: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("--config")
            .arg(&config)
            .arg("agent-session")
            .arg("cleanup")
            .arg("--status")
            .arg("failed")
            .arg("--status")
            .arg("abandoned")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(preview["version"], 2);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["candidate_count"], 2);
    assert_eq!(preview["status_counts"]["failed"], 1);
    assert_eq!(preview["status_counts"]["abandoned"], 1);

    let applied: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("--config")
            .arg(&config)
            .arg("agent-session")
            .arg("cleanup")
            .arg("--status")
            .arg("failed")
            .arg("--status")
            .arg("abandoned")
            .arg("--apply")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(applied["deleted_sessions"], 2);
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM agent_sessions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(remaining, 1);
}

#[test]
fn session_attempt_states_are_explicit() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let started: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("agent-session")
            .arg("start")
            .arg("attempt state session")
            .arg("--json"),
    ))
    .unwrap();
    let id = started["id"].as_str().unwrap();
    assert_eq!(started["attempt_state"], "idle");

    let claimed: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("agent-session")
            .arg("claim")
            .arg(id)
            .arg("--owner")
            .arg("state-worker")
            .arg("--json"),
    ))
    .unwrap();
    let token = claimed["lease_token"].as_str().unwrap();
    assert_eq!(claimed["session"]["attempt_state"], "leased");

    let released: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("agent-session")
            .arg("release")
            .arg(id)
            .arg("--owner")
            .arg("state-worker")
            .arg("--lease-token")
            .arg(token)
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(released["attempt_state"], "released");

    let claimed_again: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("agent-session")
            .arg("claim")
            .arg(id)
            .arg("--owner")
            .arg("state-worker-2")
            .arg("--lease-secs")
            .arg("5")
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(claimed_again["session"]["attempt_state"], "leased");
    Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE agent_sessions SET lease_expires_at = 0 WHERE id = ?1",
            params![id],
        )
        .unwrap();
    let status: Value = serde_json::from_str(&stdout(
        cmd(&db)
            .arg("agent-session")
            .arg("status")
            .arg(id)
            .arg("--json"),
    ))
    .unwrap();
    assert_eq!(status[0]["attempt_state"], "stale");
}

use assert_cmd::Command;
use predicates::str::contains;
use rusqlite::{Connection, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

fn cmd(db: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("dukememory").unwrap();
    command.arg("--db").arg(db);
    command
}

fn stdout(command: &mut Command) -> String {
    String::from_utf8(command.assert().success().get_output().stdout.clone()).unwrap()
}

fn json_section_ids(value: &Value, sections: &[&str]) -> Vec<String> {
    let mut ids = Vec::new();
    for section in sections {
        let Some(items) = value.get(*section).and_then(Value::as_array) else {
            continue;
        };
        ids.extend(items.iter().filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        }));
    }
    ids.sort();
    ids.dedup();
    ids
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn memory_content_hash(
    memory_type: &str,
    scope: &str,
    title: &str,
    body: &str,
    status: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{memory_type}\n{scope}\n{title}\n{body}\n{status}").as_bytes());
    format!("{:x}", hasher.finalize())
}

fn insert_empty_read_event(db: &std::path::Path, command: &str, query: &str) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at) \
         VALUES (?1, ?2, '', 1, 0, 1200, 1, ?3)",
        params![command, query, now_ms()],
    )
    .unwrap();
}

fn insert_read_event(db: &std::path::Path, command: &str, query: &str, semantic_used: bool) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at) \
         VALUES (?1, ?2, '', ?3, 0, 1200, 1, ?4)",
        params![command, query, if semantic_used { 1 } else { 0 }, now_ms()],
    )
    .unwrap();
}

fn insert_read_event_with_ids(db: &std::path::Path, command: &str, query: &str, ids: &[&str]) {
    let conn = Connection::open(db).unwrap();
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at) \
         VALUES (?1, ?2, ?3, 1, ?4, 1200, 1, ?5)",
        params![command, query, ids.join(","), ids.len(), now_ms()],
    )
    .unwrap();
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
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(stream, "{request}").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(child.wait().unwrap().success());
    response
}

#[test]
fn add_search_context_pack_and_status() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let first_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("MVP auth without SSO")
            .arg("Use email and password for the first version. SSO is deferred.")
            .arg("--scope")
            .arg("project")
            .arg("--source")
            .arg("test"),
    )
    .trim()
    .to_string();
    assert_eq!(first_id.len(), 12);

    let search = stdout(cmd(&db).arg("search").arg("email password").arg("--json"));
    let rows: Value = serde_json::from_str(&search).unwrap();
    assert_eq!(rows[0]["id"], first_id);
    assert_eq!(rows[0]["status"], "active");

    cmd(&db)
        .arg("context-pack")
        .arg("make login easier")
        .arg("--max-chars")
        .arg("1000")
        .assert()
        .success()
        .stdout(contains("MVP auth without SSO"));

    let second_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("MVP auth with magic links")
            .arg("Use passwordless magic links instead of email/password.")
            .arg("--supersedes")
            .arg(&first_id),
    )
    .trim()
    .to_string();
    assert_ne!(second_id, first_id);

    let active = stdout(
        cmd(&db)
            .arg("list")
            .arg("--status")
            .arg("active")
            .arg("--json"),
    );
    let active_rows: Value = serde_json::from_str(&active).unwrap();
    assert_eq!(active_rows[0]["id"], second_id);

    let superseded = stdout(
        cmd(&db)
            .arg("list")
            .arg("--status")
            .arg("superseded")
            .arg("--json"),
    );
    let superseded_rows: Value = serde_json::from_str(&superseded).unwrap();
    assert_eq!(superseded_rows[0]["id"], first_id);
}

#[test]
fn links_and_stats() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("known_issue")
        .arg("PDF export requires Chrome")
        .arg("The print pipeline is stable only in Chrome.")
        .arg("--link")
        .arg("file:src/export.ts")
        .arg("--link")
        .arg("symbol:exportPdf")
        .assert()
        .success();

    cmd(&db)
        .arg("stats")
        .assert()
        .success()
        .stdout(contains("total: 1"))
        .stdout(contains("known_issue: 1"));
}

#[test]
fn search_filters_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "checkout search token budget";
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Noisy search memory")
            .arg("checkout search token budget noisy card should be suppressed from search"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful search memory")
        .arg("checkout search token budget useful card should remain in search")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("search")
        .arg("--query")
        .arg(query)
        .assert()
        .success();

    let search = stdout(cmd(&db).arg("search").arg(query).arg("--json"));
    let rows: Value = serde_json::from_str(&search).unwrap();
    let titles = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!titles.contains(&"Noisy search memory"));
    assert!(titles.contains(&"Useful search memory"));
}

#[test]
fn budget_plan_uses_missing_feedback_without_overexpanding() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let baseline = stdout(
        cmd(&db)
            .arg("budget-plan")
            .arg("checkout validation")
            .arg("--json"),
    );
    let baseline_json: Value = serde_json::from_str(&baseline).unwrap();
    assert_eq!(baseline_json["profile"], "tiny");

    cmd(&db)
        .arg("feedback")
        .arg("--rating")
        .arg("missing")
        .arg("--command")
        .arg("brief")
        .arg("--query")
        .arg("checkout validation memory gap")
        .assert()
        .success();

    let planned = stdout(
        cmd(&db)
            .arg("budget-plan")
            .arg("checkout validation")
            .arg("--json"),
    );
    let planned_json: Value = serde_json::from_str(&planned).unwrap();
    assert_eq!(planned_json["profile"], "normal");
    assert_eq!(planned_json["max_chars"], 3000);
    assert!(
        planned_json["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str().unwrap().contains("missing feedback"))
    );

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Checkout validation memory gap resolved")
        .arg("This active card resolves the checkout validation memory gap.")
        .assert()
        .success();
    let resolved = stdout(
        cmd(&db)
            .arg("budget-plan")
            .arg("checkout validation")
            .arg("--json"),
    );
    let resolved_json: Value = serde_json::from_str(&resolved).unwrap();
    assert_eq!(resolved_json["profile"], "tiny");
    assert!(
        resolved_json["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .all(|reason| !reason.as_str().unwrap().contains("missing feedback"))
    );
}

#[test]
fn live_eval_resolves_empty_reads_by_memory_links() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Linked implementation coverage")
        .arg("This card covers the implementation hook through metadata.")
        .arg("--link")
        .arg("symbol:auth::rate_limit")
        .assert()
        .success();
    insert_empty_read_event(&db, "brief", "auth::rate_limit");
    insert_empty_read_event(&db, "brief", "missing checkout policy");

    let eval_live = stdout(cmd(&db).arg("eval").arg("live").arg("--json"));
    let eval_live_json: Value = serde_json::from_str(&eval_live).unwrap();
    assert_eq!(eval_live_json["inferred_missing"].as_u64().unwrap(), 1);
    assert!(
        eval_live_json["inferred_missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "missing checkout policy")
    );
    assert!(
        !eval_live_json["inferred_missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "auth::rate_limit")
    );
}

#[test]
fn live_eval_ignores_code_identifier_empty_reads() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Initialize schema")
        .arg("Create the memory database before recording read events.")
        .assert()
        .success();
    insert_empty_read_event(&db, "brief", "live_eval_report");
    insert_empty_read_event(&db, "brief", "src/app/diagnostics.rs");
    insert_empty_read_event(&db, "brief", "missing checkout policy");

    let eval_live = stdout(cmd(&db).arg("eval").arg("live").arg("--json"));
    let eval_live_json: Value = serde_json::from_str(&eval_live).unwrap();
    assert_eq!(eval_live_json["inferred_missing"].as_u64().unwrap(), 1);
    assert_eq!(
        eval_live_json["semantic_empty_missing"].as_u64().unwrap(),
        1
    );
    let queries = eval_live_json["inferred_missing_queries"]
        .as_array()
        .unwrap()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        queries
            .iter()
            .any(|item| *item == "missing checkout policy")
    );
    assert!(
        eval_live_json["semantic_empty_missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "missing checkout policy")
    );
    assert!(!queries.iter().any(|item| *item == "live_eval_report"));
    assert!(!queries.iter().any(|item| *item == "src/app/diagnostics.rs"));
}

#[test]
fn memory_qa_reports_only_actionable_missing_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Initialize schema")
        .arg("Create the memory database before recording feedback.")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--rating")
        .arg("missing")
        .arg("--command")
        .arg("brief")
        .arg("--query")
        .arg("live_eval_report")
        .assert()
        .success();

    let code_only_qa = stdout(cmd(&db).arg("memory-qa").arg("--json"));
    let code_only_json: Value = serde_json::from_str(&code_only_qa).unwrap();
    assert!(
        code_only_json["issues"]
            .as_array()
            .unwrap()
            .iter()
            .all(|issue| !issue.as_str().unwrap().contains("missing feedback"))
    );

    cmd(&db)
        .arg("feedback")
        .arg("--rating")
        .arg("missing")
        .arg("--command")
        .arg("brief")
        .arg("--query")
        .arg("missing checkout policy")
        .assert()
        .success();
    let actionable_qa = stdout(cmd(&db).arg("memory-qa").arg("--json"));
    let actionable_json: Value = serde_json::from_str(&actionable_qa).unwrap();
    assert!(
        actionable_json["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue.as_str().unwrap() == "1 unresolved missing feedback query(s)")
    );
}

#[test]
fn memory_qa_reports_semantic_empty_result_health() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Initialize schema")
        .arg("Create the memory database before recording semantic read events.")
        .assert()
        .success();

    insert_read_event(&db, "brief", "checkout policy memory", true);
    insert_read_event(&db, "search", "checkout policy memory", true);
    insert_read_event(&db, "impact", "payment retry policy", true);

    let qa = stdout(
        cmd(&db)
            .arg("memory-qa")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let qa_json: Value = serde_json::from_str(&qa).unwrap();
    assert_eq!(qa_json["semantic_read_rate"].as_f64().unwrap(), 1.0);
    assert_eq!(qa_json["semantic_result_rate"].as_f64().unwrap(), 0.0);
    assert_eq!(qa_json["semantic_empty_read_count"], 3);
    assert_eq!(qa_json["semantic_avg_results"].as_f64().unwrap(), 0.0);
    assert_eq!(
        qa_json["semantic_eligible_result_rate"].as_f64().unwrap(),
        0.0
    );
    assert_eq!(qa_json["semantic_eligible_empty_read_count"], 3);
    assert!(
        qa_json["semantic_empty_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|query| query == "checkout policy memory")
    );
    assert!(qa_json["issues"].as_array().unwrap().iter().any(|issue| {
        issue
            .as_str()
            .unwrap()
            .contains("semantic recall returns results")
    }));
}

#[test]
fn init_update_get_delete_and_privacy_guard() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let config = dir.path().join("config.toml");

    cmd(&db)
        .arg("init")
        .arg("--config")
        .arg(&config)
        .assert()
        .success()
        .stdout(contains("config:"));
    assert!(config.exists());
    let agents = config.parent().unwrap().join("AGENTS.md");
    assert!(agents.exists());
    assert!(
        fs::read_to_string(&agents)
            .unwrap()
            .contains("<!-- DUKEMEMORY_START -->")
    );

    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("secret")
        .arg("api_key: sk-test")
        .assert()
        .failure()
        .stderr(contains("may contain a secret"));

    let id = stdout(
        cmd(&db)
            .arg("add")
            .arg("note")
            .arg("Original title")
            .arg("Original body")
            .arg("--confidence")
            .arg("0.5"),
    )
    .trim()
    .to_string();

    cmd(&db)
        .arg("update")
        .arg(&id)
        .arg("--title")
        .arg("Updated title")
        .arg("--body")
        .arg("Updated body")
        .arg("--confidence")
        .arg("0.9")
        .arg("--replace-links")
        .arg("--link")
        .arg("file:src/main.rs")
        .assert()
        .success();

    let got = stdout(cmd(&db).arg("get").arg(&id).arg("--json"));
    let value: Value = serde_json::from_str(&got).unwrap();
    assert_eq!(value["title"], "Updated title");
    assert_eq!(value["confidence"], 0.9);
    assert_eq!(value["links"][0]["target"], "src/main.rs");

    cmd(&db).arg("delete").arg(&id).assert().success();
    cmd(&db).arg("get").arg(&id).assert().failure();
}

#[test]
fn export_import_backup_and_restore() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let export_path = dir.path().join("export.json");
    let imported_db = dir.path().join("imported.db");
    let backup_db = dir.path().join("backup.db");
    let restored_db = dir.path().join("restored.db");

    let id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Exported decision")
            .arg("This card should survive export and import.")
            .arg("--link")
            .arg("symbol:ExportedDecision"),
    )
    .trim()
    .to_string();

    cmd(&db)
        .arg("export")
        .arg("--output")
        .arg(&export_path)
        .assert()
        .success();
    let raw = fs::read_to_string(&export_path).unwrap();
    assert!(raw.contains(&id));

    cmd(&imported_db)
        .arg("import")
        .arg(&export_path)
        .assert()
        .success()
        .stdout(contains("imported: 1"));
    cmd(&imported_db)
        .arg("get")
        .arg(&id)
        .assert()
        .success()
        .stdout(contains("Exported decision"));

    cmd(&db).arg("backup").arg(&backup_db).assert().success();
    assert!(backup_db.exists());

    cmd(&restored_db)
        .arg("restore")
        .arg(&backup_db)
        .arg("--force")
        .assert()
        .success();
    cmd(&restored_db)
        .arg("get")
        .arg(&id)
        .assert()
        .success()
        .stdout(contains("Exported decision"));
}

#[test]
fn review_conflicts_links_session_and_vec_status() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Same title")
        .arg("First active decision.")
        .arg("--link")
        .arg("file:src/main.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Same title")
        .arg("Second active decision.")
        .arg("--confidence")
        .arg("0.4")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("Maybe")
        .arg("Needs confirmation.")
        .arg("--status")
        .arg("uncertain")
        .assert()
        .success();

    cmd(&db)
        .arg("conflicts")
        .assert()
        .success()
        .stdout(contains("possible_conflict"));
    cmd(&db)
        .arg("review")
        .assert()
        .success()
        .stdout(contains("low_confidence"))
        .stdout(contains("uncertain"));
    cmd(&db)
        .arg("links")
        .arg("--root")
        .arg(root)
        .assert()
        .success()
        .stdout(contains("ok  file:src/main.rs"));
    cmd(&db)
        .arg("session-close")
        .arg("--title")
        .arg("Session summary")
        .arg("--summary")
        .arg("Implemented v2 commands.")
        .arg("--next")
        .arg("Run release build")
        .assert()
        .success();
    cmd(&db)
        .arg("vec-status")
        .assert()
        .success()
        .stdout(contains("sqlite-vec feature:"))
        .stdout(contains("http://192.168.0.13:11434"))
        .stdout(contains("bge-m3:latest"));
}

#[test]
fn serve_mcp_handles_tools_list_and_context_pack() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("MCP decision")
        .arg("MCP can retrieve this card.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("MCP uncertain review")
        .arg("MCP review should report this uncertain memory.")
        .arg("--status")
        .arg("uncertain")
        .assert()
        .success();
    let long_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("MCP long search card")
            .arg(format!(
                "{} needle mcp exact detail should be visible {}",
                "mcp prefix noise ".repeat(80),
                "mcp tail noise ".repeat(80)
            )),
    )
    .trim()
    .to_string();
    let transcript = dir.path().join("mcp-transcript.md");
    fs::write(
        &transcript,
        format!(
            "We decided {} needle inbox exact detail should be visible {}\nTODO approve MCP inbox items.\n",
            "inbox prefix noise ".repeat(80),
            "inbox tail noise ".repeat(80)
        ),
    )
    .unwrap();
    cmd(&db)
        .arg("ingest-transcript")
        .arg(&transcript)
        .assert()
        .success();
    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg(format!("Recent unrelated snapshot {index}"))
            .arg("billing export unrelated recent card should not hide query snapshot memory")
            .assert()
            .success();
    }

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_context_pack","arguments":{"task":"retrieve mcp","max_chars":1000}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"needle mcp","max_chars":1000}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_get","arguments":{"id":long_id.clone(),"query":"needle mcp","max_chars":1000}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_get","arguments":{"id":long_id.clone(),"include_body":true}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"memory_snapshot","arguments":{"query":"needle mcp","max_chars":800}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"memory_review","arguments":{"max_chars":800}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"memory_inbox_list","arguments":{"query":"needle inbox","max_chars":1000}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"memory_inbox_list","arguments":{"include_body":true}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"needle mcp","budget":900,"max_chars":900}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"memory_impact","arguments":{"target":"needle mcp","budget":900,"max_chars":900}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"memory_drift","arguments":{"max_chars":900}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"memory_doctor","arguments":{"max_chars":900}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"memory_auto_ingest","arguments":{"input":transcript.display().to_string(),"dry_run":true,"max_chars":900}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"memory_budget_plan","arguments":{"task":"needle mcp","max_chars":600}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"needle mcp","max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":17,"method":"tools/call","params":{"name":"memory_get","arguments":{"id":long_id.clone(),"query":"needle mcp","max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"memory_evidence","arguments":{"id":long_id.clone(),"query":"needle mcp","max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"memory_doctrine","arguments":{"max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"memory_budget_plan","arguments":{"task":"needle mcp","max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"memory_doctor","arguments":{"max_chars":80}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"memory_feedback","arguments":{"id":long_id,"rating":"useful","command":"mcp-test","query":"needle mcp"}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"memory_feedback","arguments":{"rating":"missing","command":"mcp-test","query":"missing mcp card"}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("memory_brief"));
    assert!(stdout.contains("memory_impact"));
    assert!(stdout.contains("memory_budget_plan"));
    assert!(stdout.contains("memory_feedback"));
    assert!(stdout.contains("memory_drift"));
    assert!(stdout.contains("memory_context_pack"));
    assert!(stdout.find("memory_brief") < stdout.find("memory_context_pack"));
    assert!(stdout.contains("MCP decision"));
    assert!(stdout.contains("needle mcp exact detail"));
    let compact_search = stdout
        .lines()
        .find(|line| line.contains("\"id\":3"))
        .unwrap();
    assert!(!compact_search.contains(&"mcp prefix noise ".repeat(10)));
    let compact_get = stdout
        .lines()
        .find(|line| line.contains("\"id\":4"))
        .unwrap();
    assert!(compact_get.contains("summary"));
    assert!(!compact_get.contains("body"));
    assert!(!compact_get.contains(&"mcp prefix noise ".repeat(10)));
    let full_get = stdout
        .lines()
        .find(|line| line.contains("\"id\":5"))
        .unwrap();
    assert!(full_get.contains("body"));
    let snapshot = stdout
        .lines()
        .find(|line| line.contains("\"id\":6"))
        .unwrap();
    assert!(snapshot.contains("Relevant Memory"));
    assert!(!snapshot.contains(&"mcp prefix noise ".repeat(10)));
    let review = stdout
        .lines()
        .find(|line| line.contains("\"id\":7"))
        .unwrap();
    assert!(review.contains("total"));
    assert!(review.contains("issues"));
    assert!(review.contains("MCP uncertain review"));
    let compact_inbox = stdout
        .lines()
        .find(|line| line.contains("\"id\":8"))
        .unwrap();
    assert!(compact_inbox.contains("summary"));
    assert!(!compact_inbox.contains("body"));
    assert!(compact_inbox.contains("needle inbox exact detail"));
    assert!(!compact_inbox.contains(&"inbox prefix noise ".repeat(10)));
    let full_inbox = stdout
        .lines()
        .find(|line| line.contains("\"id\":9"))
        .unwrap();
    assert!(full_inbox.contains("body"));
    let brief = stdout
        .lines()
        .find(|line| line.contains("\"id\":10"))
        .unwrap();
    assert!(brief.contains("budget"));
    assert!(!brief.contains(&"mcp prefix noise ".repeat(10)));
    let impact = stdout
        .lines()
        .find(|line| line.contains("\"id\":11"))
        .unwrap();
    assert!(impact.contains("target"));
    assert!(!impact.contains(&"mcp prefix noise ".repeat(10)));
    let drift = stdout
        .lines()
        .find(|line| line.contains("\"id\":12"))
        .unwrap();
    assert!(drift.contains("counts"));
    assert!(drift.contains("missing_links"));
    let doctor = stdout
        .lines()
        .find(|line| line.contains("\"id\":13"))
        .unwrap();
    assert!(doctor.contains("review_issues"));
    assert!(doctor.contains("pending_inbox"));
    let auto_ingest = stdout
        .lines()
        .find(|line| line.contains("\"id\":14"))
        .unwrap();
    assert!(auto_ingest.contains("scanned"));
    assert!(auto_ingest.contains("returned_files"));
    let budget_plan = stdout
        .lines()
        .find(|line| line.contains("\"id\":15"))
        .unwrap();
    assert!(budget_plan.contains("profile"));
    assert!(budget_plan.contains("max_chars"));
    assert!(budget_plan.contains("reasons"));
    let mcp_text = |id: i64| {
        let line = stdout
            .lines()
            .find(|line| line.contains(&format!("\"id\":{id}")))
            .unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        value["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let feedback = mcp_text(22);
    let feedback_json: Value = serde_json::from_str(&feedback).unwrap();
    assert_eq!(feedback_json["ok"], true);
    assert_eq!(feedback_json["rating"], "useful");
    assert_eq!(feedback_json["summary"]["positive"], 1);
    let missing_feedback = mcp_text(23);
    let missing_feedback_json: Value = serde_json::from_str(&missing_feedback).unwrap();
    assert_eq!(missing_feedback_json["ok"], true);
    assert_eq!(missing_feedback_json["rating"], "missing");
    assert_eq!(missing_feedback_json["summary"]["missing"], 1);
    for id in 16..=23 {
        let text = mcp_text(id);
        serde_json::from_str::<Value>(&text)
            .unwrap_or_else(|err| panic!("MCP id {id} returned invalid JSON text: {err}: {text}"));
    }
}

#[test]
fn mcp_memory_search_filters_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "mcp search token budget";
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Noisy MCP search memory")
            .arg("mcp search token budget noisy card should be suppressed from MCP search"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful MCP search memory")
        .arg("mcp search token budget useful card should remain in MCP search")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("memory_search")
        .arg("--query")
        .arg(query)
        .assert()
        .success();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":query,"max_chars":1000}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("Noisy MCP search memory"));
    assert!(text.contains("Useful MCP search memory"));
    serde_json::from_str::<Value>(text).unwrap();
}

#[test]
fn mcp_snapshot_filters_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "snapshot token budget";
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("Noisy snapshot memory")
            .arg("snapshot token budget noisy card should be suppressed from MCP snapshot"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Useful snapshot memory")
        .arg("snapshot token budget useful card should remain in MCP snapshot")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("memory_snapshot")
        .arg("--query")
        .arg(query)
        .assert()
        .success();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_snapshot","arguments":{"query":query,"limit":4,"max_chars":1000}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("Noisy snapshot memory"));
    assert!(text.contains("Useful snapshot memory"));
    assert!(!text.contains("Recent unrelated snapshot"));
}

#[test]
fn mcp_snapshot_filters_noisy_top_candidates() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "snapshot overfetch token budget";

    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Useful snapshot overfetch memory")
        .arg("snapshot overfetch token budget useful card should remain in MCP snapshot")
        .assert()
        .success();
    let mut noisy_ids = Vec::new();
    for index in 0..10 {
        let id = stdout(
            cmd(&db)
                .arg("add")
                .arg("task_state")
                .arg(format!("Noisy snapshot overfetch memory {index}"))
                .arg("snapshot overfetch token budget noisy card should be suppressed from MCP snapshot"),
        )
        .trim()
        .to_string();
        noisy_ids.push(id);
    }
    for id in noisy_ids {
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(id)
            .arg("--rating")
            .arg("useless")
            .arg("--command")
            .arg("memory_snapshot")
            .arg("--query")
            .arg(query)
            .assert()
            .success();
    }

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_snapshot","arguments":{"query":query,"limit":4,"max_chars":1000}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Useful snapshot overfetch memory"));
    assert!(!text.contains("Noisy snapshot overfetch memory"));
}

#[test]
fn http_search_filters_noisy_top_candidates() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "http search token budget";

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful HTTP search memory")
        .arg("http search token budget useful card should remain in HTTP search")
        .assert()
        .success();

    let mut noisy_ids = Vec::new();
    for index in 0..10 {
        let noisy_id = stdout(
            cmd(&db)
                .arg("add")
                .arg("design_note")
                .arg(format!("Noisy HTTP search memory {index}"))
                .arg("http search token budget noisy card should be suppressed from HTTP search"),
        )
        .trim()
        .to_string();
        noisy_ids.push(noisy_id);
    }
    for noisy_id in noisy_ids {
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(&noisy_id)
            .arg("--rating")
            .arg("useless")
            .arg("--command")
            .arg("search")
            .arg("--query")
            .arg(query)
            .assert()
            .success();
    }

    let body = serde_json::json!({"query": query, "limit": 10}).to_string();
    let response = http_once(
        &db,
        &format!(
            "POST /search HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );
    assert!(response.contains("200 OK"));
    assert!(response.contains("Useful HTTP search memory"));
    assert!(!response.contains("Noisy HTTP search memory"));
}

#[test]
fn mcp_tool_calls_can_select_project_db_by_root_or_path_scope() {
    let dir = tempdir().unwrap();
    let default_root = dir.path().join("default");
    let selected_root = dir.path().join("selected");
    let default_db = default_root.join(".agent/memory.db");
    let selected_db = selected_root.join(".agent/memory.db");

    cmd(&default_db)
        .arg("add")
        .arg("decision")
        .arg("Default project decision")
        .arg("MCP should not read this card when a root override is supplied.")
        .assert()
        .success();
    cmd(&selected_db)
        .arg("add")
        .arg("decision")
        .arg("Selected project decision")
        .arg("MCP should read this card through the root override.")
        .assert()
        .success();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&default_db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"selected project","root":selected_root}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"selected project","scope":selected_root}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Selected project decision"));
    assert!(!stdout.contains("Default project decision"));
}

#[test]
fn v3_project_intelligence_rhai_suggest_compact_and_lifecycle() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let transcript = dir.path().join("transcript.md");
    let rules = dir.path().join("rules.rhai");

    fs::write(
        &rules,
        r#"
        fn score_memory(type, status, scope, title, body, task, confidence) {
            if type == "decision" { 5.0 } else { 0.0 }
        }
        "#,
    )
    .unwrap();
    cmd(&db)
        .arg("rhai-check")
        .arg(&rules)
        .assert()
        .success()
        .stdout(contains("ok score="));

    cmd(&db)
        .arg("add")
        .arg("product_goal")
        .arg("Build local memory")
        .arg("The product goal is fast local agent memory.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Use Rust Rhai")
        .arg("We decided to use Rust with Rhai for local rules.")
        .assert()
        .success();
    cmd(&db)
        .arg("session-close")
        .arg("--title")
        .arg("V3 session")
        .arg("--summary")
        .arg("Implemented v3 project intelligence.")
        .arg("--next")
        .arg("Run final verification")
        .arg("--scope")
        .arg("project")
        .assert()
        .success();

    cmd(&db)
        .arg("project-summary")
        .arg("--max-chars")
        .arg("2000")
        .assert()
        .success()
        .stdout(contains("Build local memory"))
        .stdout(contains("Use Rust Rhai"));
    cmd(&db)
        .arg("decisions")
        .assert()
        .success()
        .stdout(contains("Use Rust Rhai"));
    cmd(&db)
        .arg("next-actions")
        .assert()
        .success()
        .stdout(contains("Run final verification"));
    cmd(&db)
        .arg("context-pack")
        .arg("rules")
        .arg("--rules")
        .arg(&rules)
        .arg("--with-codegraph")
        .assert()
        .success()
        .stdout(contains("CodeGraph Hints"));

    fs::write(
        &transcript,
        "We decided to keep everything local.\nTODO run release packaging.\n",
    )
    .unwrap();
    cmd(&db)
        .arg("suggest")
        .arg(&transcript)
        .assert()
        .success()
        .stdout(contains("decision"))
        .stdout(contains("task_state"));

    cmd(&db)
        .arg("compact")
        .arg("--scope")
        .arg("project")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("Compacted task state"));

    cmd(&db)
        .arg("lifecycle")
        .arg("--stale-days")
        .arg("0")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("would_mark_uncertain"));

    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("intentional secret")
        .arg("api_key: sk-test-secret")
        .arg("--allow-sensitive")
        .assert()
        .success();
    cmd(&db)
        .arg("scan-secrets")
        .assert()
        .success()
        .stdout(contains("assignment_secret"));
}

#[test]
fn v4_inbox_mock_embeddings_redaction_and_provider_registry() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let transcript = dir.path().join("transcript.md");
    let export_path = dir.path().join("redacted.json");

    fs::write(
        &transcript,
        "We decided to use local mock embeddings for tests.\nTODO approve memory inbox items.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("ingest-transcript")
        .arg(&transcript)
        .assert()
        .success()
        .stdout(contains("inbox_added: 2"));

    let inbox = stdout(cmd(&db).arg("inbox-list").arg("--json"));
    let rows: Value = serde_json::from_str(&inbox).unwrap();
    let inbox_id = rows[0]["id"].as_str().unwrap();

    let memory_id = stdout(cmd(&db).arg("inbox-approve").arg(inbox_id))
        .trim()
        .to_string();
    assert_eq!(memory_id.len(), 12);

    cmd(&db)
        .arg("provider-list")
        .arg("--provider")
        .arg("mock")
        .assert()
        .success()
        .stdout(contains("mock-embedding"));

    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains("\"indexed\": 1"));

    cmd(&db)
        .arg("embed-search")
        .arg("local embeddings")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains(&memory_id));

    cmd(&db)
        .arg("vector-bench")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains("vectors: 1"));

    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("secret")
        .arg("token = abcdef12345")
        .arg("--allow-sensitive")
        .assert()
        .success();
    cmd(&db)
        .arg("scan-secrets")
        .arg("--fix-redact")
        .assert()
        .success()
        .stdout(contains("redacted: 1"));
    cmd(&db)
        .arg("export")
        .arg("--redact")
        .arg("--output")
        .arg(&export_path)
        .assert()
        .success();
    assert!(
        !fs::read_to_string(export_path)
            .unwrap()
            .contains("abcdef12345")
    );
}

#[test]
fn v5_agent_native_commands_snapshot_doctor_and_packaging() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let id = stdout(
        cmd(&db)
            .arg("remember")
            .arg("We decided the product should keep memory local and fast.")
            .arg("--type")
            .arg("decision"),
    )
    .trim()
    .to_string();
    assert_eq!(id.len(), 12);

    cmd(&db)
        .arg("what-do-we-know")
        .arg("local memory")
        .assert()
        .success()
        .stdout(contains("local and fast"));

    cmd(&db)
        .arg("context")
        .arg("continue memory implementation")
        .arg("--mode")
        .arg("fast")
        .assert()
        .success()
        .stdout(contains("Agent Context"))
        .stdout(contains("local and fast"));

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Snapshot JSON long body")
        .arg(format!(
            "snapshot json compact summary should stay readable {}",
            "snapshot json tail noise ".repeat(80)
        ))
        .arg("--link")
        .arg("file:src/snapshot.rs")
        .assert()
        .success();

    cmd(&db)
        .arg("snapshot")
        .assert()
        .success()
        .stdout(contains("Project Snapshot"));

    let snapshot_json = stdout(
        cmd(&db)
            .arg("snapshot")
            .arg("--max-chars")
            .arg("1200")
            .arg("--json"),
    );
    let snapshot_value: Value = serde_json::from_str(&snapshot_json).unwrap();
    let snapshot_items = snapshot_value.as_array().unwrap();
    assert!(!snapshot_items.is_empty());
    let long_item = snapshot_items
        .iter()
        .find(|item| item["title"] == "Snapshot JSON long body")
        .unwrap();
    assert!(long_item.get("body").is_none());
    assert!(
        long_item["summary"]
            .as_str()
            .unwrap()
            .contains("snapshot json compact summary")
    );
    assert_eq!(long_item["links"][0]["target"], "src/snapshot.rs");
    assert!(!snapshot_json.contains(&"snapshot json tail noise ".repeat(20)));

    cmd(&db)
        .arg("doctor")
        .assert()
        .success()
        .stdout(contains("memory_quality"))
        .stdout(contains("embeddings"));

    cmd(&db)
        .arg("embed-status")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains("eligible: 2"))
        .stdout(contains("missing: 2"));

    cmd(&db)
        .arg("embed-watch")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--once")
        .assert()
        .success()
        .stdout(contains("indexed=2"));

    cmd(&db)
        .arg("forget")
        .arg("local fast")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("would_reject"));

    cmd(&db)
        .arg("completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(contains("complete -F _dukememory dukememory"));

    cmd(&db)
        .arg("man")
        .assert()
        .success()
        .stdout(contains("dukememory(1)"));
}

#[test]
fn v6_content_length_mcp_and_rhai_policy_hooks() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let rules = dir.path().join("policy.rhai");

    fs::write(
        &rules,
        r#"
        fn score_memory(type, status, scope, title, body, task, confidence) { 1.0 }
        fn should_include(type, status, scope, title, body, task, confidence) {
            !title.contains("Reject")
        }
        fn should_redact(type, status, scope, title, body, confidence) {
            body.contains("token =")
        }
        "#,
    )
    .unwrap();

    cmd(&db)
        .arg("policy-check")
        .arg(&rules)
        .assert()
        .success()
        .stdout(contains("include=true"))
        .stdout(contains("redact=true"));

    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("Reject this")
        .arg("token = abcdef12345")
        .arg("--allow-sensitive")
        .assert()
        .success();
    cmd(&db)
        .arg("policy-apply")
        .arg(&rules)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("would_redact"))
        .stdout(contains("would_reject"));

    let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}});
    let body = request.to_string();
    let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .arg("--content-length")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(frame.as_bytes())
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Content-Length:"));
    assert!(stdout.contains("memory_agent_context"));
}

#[test]
fn v7_audit_workspace_init_and_bundle() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let bundle = dir.path().join("bundle.json");

    let id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Audited decision")
            .arg("This should create an audit event."),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("status")
        .arg(&id)
        .arg("uncertain")
        .assert()
        .success();

    cmd(&db)
        .arg("audit")
        .assert()
        .success()
        .stdout(contains("memory_status"))
        .stdout(contains("memory_added"));

    cmd(&db)
        .arg("workspace-init")
        .arg("--root")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(contains(".agent/rules.rhai"));
    assert!(dir.path().join(".agent/rules.rhai").exists());
    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("dukememory."));
    assert!(agents.contains("dukememory brief"));

    cmd(&db)
        .arg("bundle")
        .arg(&bundle)
        .arg("--redact")
        .assert()
        .success();
    let raw = fs::read_to_string(bundle).unwrap();
    assert!(raw.contains("\"version\""));
    assert!(raw.contains("Audited decision"));
}

#[test]
fn v8_daemon_http_merge_profiles_and_sync() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sync_path = dir.path().join("sync.json");
    let imported_db = dir.path().join("imported.db");

    let first = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Use local memory")
            .arg("First decision body."),
    )
    .trim()
    .to_string();
    let second = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Use local memory now")
            .arg("Second decision body."),
    )
    .trim()
    .to_string();

    cmd(&db)
        .arg("merge-candidates")
        .assert()
        .success()
        .stdout(contains("similar type/scope/title"));
    cmd(&db)
        .arg("merge-apply")
        .arg(&first)
        .arg(&second)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("would_merge"));

    cmd(&db)
        .arg("daemon")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--once")
        .assert()
        .success()
        .stdout(contains("daemon_tick"));

    cmd(&db)
        .arg("vec-migrate")
        .arg("--backend")
        .arg("json")
        .assert()
        .success()
        .stdout(contains("json fallback"));

    cmd(&db)
        .arg("profile")
        .arg("use")
        .arg("dukegraph")
        .arg("--dir")
        .arg(dir.path().join("profiles"))
        .assert()
        .success()
        .stdout(contains("dukegraph"));
    cmd(&db)
        .arg("profile")
        .arg("list")
        .arg("--dir")
        .arg(dir.path().join("profiles"))
        .assert()
        .success()
        .stdout(contains("dukegraph"));

    cmd(&db)
        .arg("sync")
        .arg("export")
        .arg(&sync_path)
        .assert()
        .success();
    cmd(&imported_db)
        .arg("sync")
        .arg("import")
        .arg(&sync_path)
        .assert()
        .success()
        .stdout(contains("imported: 2"));

    cmd(&db)
        .arg("maintain")
        .assert()
        .success()
        .stdout(contains("Maintenance Suggestions"));

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("\"ok\":true"));
    assert!(child.wait().unwrap().success());
}

#[test]
fn merge_candidates_ignore_different_release_versions() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let release_20 = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("dukememory 0.14.20 autonomous gap inbox released")
            .arg("Released 0.14.20 with autonomous gap inbox suggestions."),
    )
    .trim()
    .to_string();
    let release_21 = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("dukememory 0.14.21 autonomous quality inbox released")
            .arg("Released 0.14.21 with autonomous quality inbox suggestions."),
    )
    .trim()
    .to_string();
    let similar = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("dukememory autonomous quality inbox released")
            .arg("Released with autonomous quality inbox suggestions."),
    )
    .trim()
    .to_string();

    let merge_json = stdout(cmd(&db).arg("merge-candidates").arg("--json"));
    let merge_items: Value = serde_json::from_str(&merge_json).unwrap();
    assert!(!merge_items.as_array().unwrap().iter().any(|item| {
        (item["primary_id"] == release_20 && item["duplicate_id"] == release_21)
            || (item["primary_id"] == release_21 && item["duplicate_id"] == release_20)
    }));
    assert!(
        merge_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["primary_id"] == similar || item["duplicate_id"] == similar })
    );
}

#[test]
fn v9_schema_retrieve_eval_compact_and_http_metrics() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("schema")
        .arg("status")
        .assert()
        .success()
        .stdout(contains("expected: 15"));
    cmd(&db)
        .arg("schema")
        .arg("verify")
        .assert()
        .success()
        .stdout(contains("schema: ok"));

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Hybrid retrieve works")
        .arg("The retrieval layer should find expected memory.")
        .assert()
        .success();
    cmd(&db)
        .arg("session-close")
        .arg("--title")
        .arg("Old task state")
        .arg("--summary")
        .arg("Temporary operational note.")
        .arg("--scope")
        .arg("project")
        .assert()
        .success();

    cmd(&db)
        .arg("retrieve")
        .arg("expected memory")
        .arg("--strategy")
        .arg("fts")
        .arg("--format")
        .arg("agent")
        .assert()
        .success()
        .stdout(contains("Retrieved Memory"))
        .stdout(contains("constraints"));

    cmd(&db)
        .arg("eval")
        .arg("add-case")
        .arg("retrieve expected")
        .arg("expected memory")
        .arg("Hybrid retrieve")
        .assert()
        .success();
    cmd(&db)
        .arg("eval")
        .arg("run")
        .assert()
        .success()
        .stdout(contains("pass"));

    cmd(&db)
        .arg("compact-v2")
        .arg("--scope")
        .arg("project")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("Compacted operational memory"));

    cmd(&db)
        .arg("lock")
        .arg("status")
        .assert()
        .success()
        .stdout(contains("locks: none"));
    cmd(&db)
        .arg("build-info")
        .assert()
        .success()
        .stdout(contains("version:"))
        .stdout(contains("schema: 15"));

    let install_dir = dir.path().join("install");
    let target = install_dir.join("dukememory");
    let backup_dir = dir.path().join("install-backups");
    fs::create_dir_all(&install_dir).unwrap();
    fs::write(&target, b"old installed binary").unwrap();
    let source = assert_cmd::cargo::cargo_bin("dukememory");

    let dry_run = stdout(
        cmd(&db)
            .arg("update-install")
            .arg("--from")
            .arg(&source)
            .arg("--to")
            .arg(&target)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--dry-run")
            .arg("--json"),
    );
    let dry_run_json: Value = serde_json::from_str(&dry_run).unwrap();
    assert_eq!(dry_run_json["changed"], true);
    assert_eq!(dry_run_json["dry_run"], true);
    assert_eq!(dry_run_json["backup_keep"], 3);
    assert_eq!(fs::read(&target).unwrap(), b"old installed binary");
    assert!(!backup_dir.exists());

    let updated = stdout(
        cmd(&db)
            .arg("update-install")
            .arg("--from")
            .arg(&source)
            .arg("--to")
            .arg(&target)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--json"),
    );
    let updated_json: Value = serde_json::from_str(&updated).unwrap();
    assert_eq!(updated_json["changed"], true);
    assert_eq!(updated_json["dry_run"], false);
    assert_eq!(updated_json["backup_keep"], 3);
    let backup = updated_json["backup"].as_str().unwrap();
    assert_eq!(fs::read(backup).unwrap(), b"old installed binary");
    for idx in 0..4 {
        fs::write(
            backup_dir.join(format!("dukememory-old-{idx}.bak")),
            format!("old backup {idx}"),
        )
        .unwrap();
    }
    let version_output = StdCommand::new(&target).arg("--version").output().unwrap();
    assert!(version_output.status.success());
    assert!(
        String::from_utf8(version_output.stdout)
            .unwrap()
            .contains(env!("CARGO_PKG_VERSION"))
    );

    let current = stdout(
        cmd(&db)
            .arg("update-install")
            .arg("--from")
            .arg(&source)
            .arg("--to")
            .arg(&target)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--backup-keep")
            .arg("2")
            .arg("--json"),
    );
    let current_json: Value = serde_json::from_str(&current).unwrap();
    assert_eq!(current_json["changed"], false);
    assert_eq!(current_json["backup"], Value::Null);
    assert_eq!(current_json["backup_keep"], 2);
    assert_eq!(current_json["kept_backups"].as_array().unwrap().len(), 2);
    assert!(current_json["pruned_backups"].as_array().unwrap().len() >= 3);
    let install_backup_count = fs::read_dir(&backup_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("dukememory-")
        })
        .count();
    assert_eq!(install_backup_count, 2);

    let skills = dir.path().join("skills");
    cmd(&db)
        .arg("install-skill")
        .arg("--path")
        .arg(&skills)
        .assert()
        .success()
        .stdout(contains("dukememory-use"));
    let skill_md = fs::read_to_string(skills.join("dukememory-use/SKILL.md")).unwrap();
    assert!(skill_md.contains("memory_brief"));
    assert!(skill_md.contains("dukememory brief"));
    let skill_yaml = fs::read_to_string(skills.join("dukememory-use/agents/openai.yaml")).unwrap();
    assert!(skill_yaml.contains("$dukememory-use"));

    let home = dir.path().join("home");
    let install_to = home.join(".local/bin");
    cmd(&db)
        .env("HOME", &home)
        .arg("install")
        .arg("--to")
        .arg(&install_to)
        .arg("--force")
        .assert()
        .success()
        .stdout(contains("dukememory"));
    assert!(install_to.join("dukememory").exists());
    assert!(home.join(".codex/skills/dukememory-use/SKILL.md").exists());

    cmd(&db)
        .arg("doctor")
        .arg("--self-check")
        .assert()
        .success()
        .stdout(contains("self"));

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("\"memories\""));
    assert!(response.contains("\"schema\""));
    assert!(child.wait().unwrap().success());
}

#[test]
fn v10_runtime_config_and_http_error_statuses() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let config = dir.path().join("config.toml");

    cmd(&db)
        .arg("init")
        .arg("--config")
        .arg(&config)
        .assert()
        .success();
    let mut raw = fs::read_to_string(&config).unwrap();
    raw = raw.replace("provider = \"ollama\"", "provider = \"mock\"");
    raw = raw.replace(
        "endpoint = \"http://192.168.0.13:11434\"",
        "endpoint = \"local\"",
    );
    raw = raw.replace("model = \"bge-m3:latest\"", "model = \"mock-small\"");
    fs::write(&config, raw).unwrap();

    cmd(&db)
        .arg("--config")
        .arg(&config)
        .arg("add")
        .arg("decision")
        .arg("Config driven context")
        .arg("Runtime config should provide embedding defaults.")
        .assert()
        .success();
    cmd(&db)
        .arg("--config")
        .arg(&config)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();
    cmd(&db)
        .arg("--config")
        .arg(&config)
        .arg("context")
        .arg("embedding defaults")
        .arg("--mode")
        .arg("agent")
        .assert()
        .success()
        .stdout(contains("Config driven context"));

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /context HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("400 Bad Request"));
    assert!(response.contains("missing task"));
    assert!(child.wait().unwrap().success());

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET /missing HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("404 Not Found"));
    assert!(child.wait().unwrap().success());
}

#[test]
fn v11_auto_ingest_and_decision_doctrine() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("session.md"),
        "We decided to preserve active doctrine decisions.\nTODO approve auto ingest items.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("auto-ingest")
        .arg("--input")
        .arg(&sessions)
        .assert()
        .success()
        .stdout(contains(
            "auto_ingest scanned=1 ingested=1 skipped=0 inbox_added=2",
        ));
    cmd(&db)
        .arg("auto-ingest")
        .arg("--input")
        .arg(&sessions)
        .assert()
        .success()
        .stdout(contains(
            "auto_ingest scanned=1 ingested=0 skipped=1 inbox_added=0",
        ));
    cmd(&db)
        .arg("inbox-list")
        .assert()
        .success()
        .stdout(contains("preserve active doctrine decisions"))
        .stdout(contains("approve auto ingest items"));

    let old = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Use scattered decisions")
            .arg("Old decision body."),
    )
    .trim()
    .to_string();
    let new = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Use decision doctrine")
            .arg("Doctrine should be the source of active decisions.")
            .arg("--supersedes")
            .arg(&old),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Use decision doctrine now")
        .arg("This similar active decision should appear as a conflict candidate.")
        .assert()
        .success();

    cmd(&db)
        .arg("doctrine")
        .assert()
        .success()
        .stdout(contains("Decision Doctrine"))
        .stdout(contains("Active Decisions"))
        .stdout(contains(&new))
        .stdout(contains("supersedes:"))
        .stdout(contains(&old))
        .stdout(contains("Potential Conflicts"));

    let raw = stdout(cmd(&db).arg("doctrine").arg("--json"));
    let value: Value = serde_json::from_str(&raw).unwrap();
    assert!(value["active"].as_array().unwrap().len() >= 2);
    assert!(!value["superseded"].as_array().unwrap().is_empty());

    fs::write(
        sessions.join("daemon.log"),
        "TODO daemon should auto ingest session facts.\n",
    )
    .unwrap();
    cmd(&db)
        .arg("daemon")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--auto-ingest")
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--once")
        .assert()
        .success()
        .stdout(contains("auto_inbox_added=1"));

    fs::write(
        sessions.join("http.txt"),
        "We decided HTTP auto ingest should be exposed.\n",
    )
    .unwrap();
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let body = serde_json::json!({"input": sessions.display().to_string(), "scope": "project"})
        .to_string();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /auto-ingest HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));
    assert!(response.contains("\"inbox_added\":1"));
    assert!(child.wait().unwrap().success());

    fs::write(
        sessions.join("mcp.md"),
        "TODO MCP auto ingest should be exposed.\n",
    )
    .unwrap();
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_doctrine","arguments":{}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_auto_ingest","arguments":{"input":sessions.display().to_string(),"dry_run":true}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Use decision doctrine"));
    assert!(stdout.contains("would_ingest"));
}

#[test]
fn v11_release_bundle_bench_and_self_host() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let bundle = dir.path().join("release");

    cmd(&db)
        .arg("self-host")
        .arg("--force")
        .assert()
        .success()
        .stdout(contains("self_hosted: added=4 skipped=0 total=4"));
    cmd(&db)
        .arg("search")
        .arg("local-first")
        .assert()
        .success()
        .stdout(contains("dukememory. local-first constraint"));

    let bench = stdout(cmd(&db).arg("bench").arg("--json"));
    let bench_json: Value = serde_json::from_str(&bench).unwrap();
    assert_eq!(bench_json["schema"], 15);
    assert_eq!(bench_json["memory_count"], 4);
    assert!(bench_json["db_bytes"].as_u64().unwrap() > 0);

    cmd(&db)
        .arg("release-bundle")
        .arg(&bundle)
        .assert()
        .success()
        .stdout(contains(bundle.to_string_lossy().as_ref()));

    assert!(bundle.join("dukememory").exists());
    assert!(bundle.join("dukememory.toml").exists());
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(bundle.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["schema"], 15);
    assert_eq!(manifest["memory_stats"]["total"], 4);
    assert_eq!(manifest["binary_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn v12_always_on_operations() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let root = dir.path().join("workspace");
    let backups = root.join(".agent/backups");
    let sessions = root.join(".agent/sessions");
    let plist = dir.path().join("com.dukememory.daemon.plist");
    fs::create_dir_all(&sessions).unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Persistent memory")
        .arg("Production v12 should support always-on operation.")
        .assert()
        .success();

    let health = stdout(
        cmd(&db)
            .arg("health")
            .arg("--root")
            .arg(&root)
            .arg("--endpoint")
            .arg("mock")
            .arg("--json"),
    );
    let health_json: Value = serde_json::from_str(&health).unwrap();
    assert_eq!(health_json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(health_json["schema"], 15);
    assert_eq!(health_json["endpoint_ok"], true);

    for _ in 0..3 {
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .assert()
            .success()
            .stdout(contains("backup:"));
    }
    let backup_count = fs::read_dir(&backups)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|value| value.to_str())
                == Some("db")
        })
        .count();
    assert_eq!(backup_count, 2);

    cmd(&db)
        .arg("cleanup")
        .arg("--audit-keep")
        .arg("1")
        .arg("--dry-run")
        .arg("--json")
        .assert()
        .success()
        .stdout(contains("\"dry_run\": true"));

    cmd(&db)
        .arg("daemon-install")
        .arg("--output")
        .arg(&plist)
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--interval-secs")
        .arg("5")
        .assert()
        .success()
        .stdout(contains(plist.to_string_lossy().as_ref()));
    let plist_text = fs::read_to_string(&plist).unwrap();
    assert!(plist_text.contains("com.dukememory.daemon"));
    assert!(plist_text.contains("--auto-ingest"));
    assert!(plist_text.contains("memory.db"));
}

#[test]
fn v13_stabilization_integrity_optimize_and_large_http_request() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let root = dir.path().join("workspace");
    fs::create_dir_all(root.join(".agent/sessions")).unwrap();
    fs::create_dir_all(root.join(".agent/backups")).unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Optimized recall")
        .arg("Production v13 should keep the local SQLite memory optimized and verifiable.")
        .assert()
        .success();

    let integrity = stdout(cmd(&db).arg("integrity").arg("--json"));
    let integrity_json: Value = serde_json::from_str(&integrity).unwrap();
    assert_eq!(integrity_json["ok"], true);
    assert_eq!(integrity_json["schema"], 15);
    assert_eq!(integrity_json["integrity_check"], "ok");

    let optimized = stdout(cmd(&db).arg("optimize").arg("--vacuum").arg("--json"));
    let optimized_json: Value = serde_json::from_str(&optimized).unwrap();
    assert_eq!(optimized_json["analyzed"], true);
    assert_eq!(optimized_json["fts_optimized"], true);
    assert_eq!(optimized_json["vacuumed"], true);

    let health = stdout(
        cmd(&db)
            .arg("health")
            .arg("--root")
            .arg(&root)
            .arg("--endpoint")
            .arg("mock")
            .arg("--json"),
    );
    let health_json: Value = serde_json::from_str(&health).unwrap();
    assert_eq!(health_json["ok"], true);
    assert_eq!(health_json["integrity_ok"], true);

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let large_task = format!("optimized recall {}", "memory ".repeat(20_000));
    let body = serde_json::json!({"task": large_task}).to_string();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /context HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));
    assert!(response.contains("Optimized recall"));
    assert!(child.wait().unwrap().success());
}

#[test]
fn v13_1_backup_checksums_and_deeper_diagnostics() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Checksum backups")
        .arg("Production v13.1 should verify backup integrity.")
        .assert()
        .success();

    let first = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("1")
            .arg("--json"),
    );
    let first_json: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first_json["verified"], true);
    assert_eq!(first_json["backup_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(first_json["backup_integrity_ok"], true);
    assert_eq!(
        first_json["source_memory_count"],
        first_json["backup_memory_count"]
    );
    assert!(std::path::Path::new(first_json["checksum_file"].as_str().unwrap()).exists());
    assert!(std::path::Path::new(first_json["manifest_file"].as_str().unwrap()).exists());

    let second = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("1")
            .arg("--json"),
    );
    let second_json: Value = serde_json::from_str(&second).unwrap();
    assert_eq!(second_json["verified"], true);
    let backup_dbs = fs::read_dir(&backups)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .and_then(|value| value.to_str())
                == Some("db")
        })
        .count();
    assert_eq!(backup_dbs, 1);

    let integrity = stdout(cmd(&db).arg("integrity").arg("--json"));
    let integrity_json: Value = serde_json::from_str(&integrity).unwrap();
    assert_eq!(integrity_json["quick_check"], "ok");
    assert!(integrity_json["page_count"].as_i64().unwrap() > 0);

    let optimized = stdout(cmd(&db).arg("optimize").arg("--json"));
    let optimized_json: Value = serde_json::from_str(&optimized).unwrap();
    assert_eq!(optimized_json["wal_checkpointed"], true);
    assert!(optimized_json["page_count"].as_i64().unwrap() > 0);
}

#[test]
fn v13_2_consistent_sqlite_backup_and_verified_restore() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let restored = dir.path().join("restored.db");
    let backups = dir.path().join("backups");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Consistent sqlite backup")
        .arg("Production v13.2 should backup WAL-mode databases consistently.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("Restore checksum")
        .arg("Restore should verify the sidecar checksum before copying.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("3")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    assert_eq!(report_json["backup_integrity_ok"], true);
    assert_eq!(report_json["source_memory_count"], 2);
    assert_eq!(report_json["backup_memory_count"], 2);
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    let checksum = std::path::PathBuf::from(report_json["checksum_file"].as_str().unwrap());
    let manifest = std::path::PathBuf::from(report_json["manifest_file"].as_str().unwrap());
    assert!(backup.exists());
    assert!(checksum.exists());
    assert!(manifest.exists());

    cmd(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .assert()
        .success()
        .stdout(contains(restored.to_string_lossy().as_ref()));
    cmd(&restored)
        .arg("search")
        .arg("WAL-mode")
        .assert()
        .success()
        .stdout(contains("Consistent sqlite backup"));

    let direct_backup = dir.path().join("direct.db");
    cmd(&db)
        .arg("backup")
        .arg(&direct_backup)
        .assert()
        .success()
        .stdout(contains(direct_backup.to_string_lossy().as_ref()));
    cmd(&direct_backup)
        .arg("integrity")
        .arg("--json")
        .assert()
        .success()
        .stdout(contains("\"ok\": true"));
}

#[test]
fn v13_3_backup_verify_and_restore_rejects_bad_checksum() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    let restored = dir.path().join("restored.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Verified backup")
        .arg("Production v13.3 should verify backups before restore.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("Backup table counts")
        .arg("Backup verification should report table counts.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    assert_eq!(report_json["table_counts_match"], true);
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    let checksum = std::path::PathBuf::from(report_json["checksum_file"].as_str().unwrap());

    let verify = stdout(cmd(&db).arg("backup-verify").arg(&backup).arg("--json"));
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
    assert_eq!(verify_json["checksum_ok"], true);
    assert_eq!(verify_json["manifest_ok"], true);
    assert_eq!(verify_json["integrity_ok"], true);
    let memory_count = verify_json["table_counts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["table"] == "memories")
        .unwrap()["count"]
        .as_i64()
        .unwrap();
    assert_eq!(memory_count, 2);

    fs::write(
        &checksum,
        "0000000000000000000000000000000000000000000000000000000000000000  backup.db\n",
    )
    .unwrap();
    let verify_bad = stdout(cmd(&db).arg("backup-verify").arg(&backup).arg("--json"));
    let verify_bad_json: Value = serde_json::from_str(&verify_bad).unwrap();
    assert_eq!(verify_bad_json["verified"], false);
    assert_eq!(verify_bad_json["checksum_ok"], false);

    cmd(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .assert()
        .failure()
        .stderr(contains("backup verification failed"));
}

#[test]
fn v13_4_backup_manifest_and_atomic_restore_preflight() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    let restored = dir.path().join("restored.db");
    let bad_restored = dir.path().join("bad-restored.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Manifest verified backup")
        .arg("Production v13.4 should verify backup manifests before restore.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    let manifest = std::path::PathBuf::from(report_json["manifest_file"].as_str().unwrap());
    assert!(manifest.exists());

    let verify = stdout(cmd(&db).arg("backup-verify").arg(&backup).arg("--json"));
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
    assert_eq!(verify_json["manifest_present"], true);
    assert_eq!(verify_json["manifest_ok"], true);

    cmd(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .assert()
        .success();
    cmd(&restored)
        .arg("search")
        .arg("backup manifests")
        .assert()
        .success()
        .stdout(contains("Manifest verified backup"));

    let mut raw_manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    raw_manifest["backup_sha256"] = Value::String(
        "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    );
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&raw_manifest).unwrap(),
    )
    .unwrap();

    let bad_verify = stdout(cmd(&db).arg("backup-verify").arg(&backup).arg("--json"));
    let bad_verify_json: Value = serde_json::from_str(&bad_verify).unwrap();
    assert_eq!(bad_verify_json["verified"], false);
    assert_eq!(bad_verify_json["manifest_ok"], false);

    cmd(&bad_restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .assert()
        .failure()
        .stderr(contains("backup verification failed"));
    assert!(!bad_restored.exists());
}

#[test]
fn v13_5_strict_backup_verify_and_restore_dry_run() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    let restored = dir.path().join("restored.db");
    let direct = dir.path().join("direct.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Strict restore preflight")
        .arg("Production v13.5 should support strict backup verification and dry-run restore.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());

    let strict_verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let strict_verify_json: Value = serde_json::from_str(&strict_verify).unwrap();
    assert_eq!(strict_verify_json["verified"], true);
    assert_eq!(strict_verify_json["strict"], true);
    assert_eq!(strict_verify_json["strict_ok"], true);

    cmd(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("restore: verified"));
    assert!(!restored.exists());

    cmd(&restored)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .assert()
        .success();
    cmd(&restored)
        .arg("search")
        .arg("dry-run restore")
        .assert()
        .success()
        .stdout(contains("Strict restore preflight"));

    cmd(&db).arg("backup").arg(&direct).assert().success();
    let direct_verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&direct)
            .arg("--strict")
            .arg("--json"),
    );
    let direct_verify_json: Value = serde_json::from_str(&direct_verify).unwrap();
    assert_eq!(direct_verify_json["verified"], false);
    assert_eq!(direct_verify_json["strict_ok"], false);
}

#[test]
fn v13_6_restore_creates_rollback_backup() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.db");
    let target = dir.path().join("target.db");
    let backups = dir.path().join("backups");
    let rollbacks = dir.path().join("rollbacks");
    let no_rollback_target = dir.path().join("no-rollback-target.db");
    let no_rollback_dir = dir.path().join("no-rollback-dir");

    cmd(&source)
        .arg("add")
        .arg("decision")
        .arg("Incoming restore")
        .arg("Production v13.6 should preserve the replaced target database.")
        .assert()
        .success();
    let report = stdout(
        cmd(&source)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());

    cmd(&target)
        .arg("add")
        .arg("decision")
        .arg("Original target")
        .arg("This target memory should be saved in rollback before restore.")
        .assert()
        .success();

    cmd(&target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--dry-run")
        .arg("--rollback-dir")
        .arg(&rollbacks)
        .assert()
        .success()
        .stdout(contains("rollback:"));
    assert!(!rollbacks.exists());

    cmd(&target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--rollback-dir")
        .arg(&rollbacks)
        .assert()
        .success()
        .stdout(contains("rollback:"));

    let rollback_files = fs::read_dir(&rollbacks)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("db"))
        .collect::<Vec<_>>();
    assert_eq!(rollback_files.len(), 1);
    assert!(rollback_files[0].with_extension("db.sha256").exists());
    assert!(
        rollback_files[0]
            .with_extension("db.manifest.json")
            .exists()
    );
    cmd(&rollback_files[0])
        .arg("search")
        .arg("Original target")
        .assert()
        .success()
        .stdout(contains("Original target"));
    cmd(&target)
        .arg("search")
        .arg("Incoming restore")
        .assert()
        .success()
        .stdout(contains("Incoming restore"));

    cmd(&no_rollback_target)
        .arg("add")
        .arg("decision")
        .arg("No rollback target")
        .arg("This target should be replaced without rollback when explicitly requested.")
        .assert()
        .success();
    cmd(&no_rollback_target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--rollback-dir")
        .arg(&no_rollback_dir)
        .arg("--no-rollback")
        .assert()
        .success();
    assert!(!no_rollback_dir.exists());
}

#[test]
fn v13_7_restore_rollback_is_strictly_verifiable() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.db");
    let target = dir.path().join("target.db");
    let backups = dir.path().join("backups");
    let rollbacks = dir.path().join("rollbacks");

    cmd(&source)
        .arg("add")
        .arg("decision")
        .arg("Incoming restore v13.7")
        .arg("Production v13.7 should restore from strictly verified backups.")
        .assert()
        .success();
    let report = stdout(
        cmd(&source)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());

    cmd(&target)
        .arg("add")
        .arg("decision")
        .arg("Rollback v13.7 target")
        .arg("This target should become a strictly verifiable rollback backup.")
        .assert()
        .success();

    cmd(&target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--rollback-dir")
        .arg(&rollbacks)
        .assert()
        .success()
        .stdout(contains("rollback:"));

    let rollback = fs::read_dir(&rollbacks)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("db"))
        .unwrap();
    let verify = stdout(
        cmd(&target)
            .arg("backup-verify")
            .arg(&rollback)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
    assert_eq!(verify_json["strict"], true);
    assert_eq!(verify_json["strict_ok"], true);
    assert_eq!(verify_json["checksum_ok"], true);
    assert_eq!(verify_json["manifest_ok"], true);
}

#[test]
fn v13_8_backup_metadata_is_atomically_written_and_verified() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Atomic metadata v13.8")
        .arg("Production v13.8 should publish only verified backup metadata sidecars.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    assert!(backup.exists());
    assert!(backup.with_extension("db.sha256").exists());
    assert!(backup.with_extension("db.manifest.json").exists());

    let temp_sidecars = fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.contains(".tmp-"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    assert!(temp_sidecars.is_empty());

    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
    assert_eq!(verify_json["strict_ok"], true);
    assert_eq!(verify_json["checksum_ok"], true);
    assert_eq!(verify_json["manifest_ok"], true);
}

#[test]
fn v13_9_backup_policy_prunes_orphan_temp_files() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    fs::create_dir_all(&backups).unwrap();
    let orphan_db_tmp = backups.join("dukememory-1.db.tmp");
    let orphan_checksum_tmp = backups.join("dukememory-1.db.sha256.tmp-123");
    let orphan_manifest_tmp = backups.join("dukememory-1.db.manifest.tmp-123");
    let unrelated_tmp = backups.join("notes.tmp-123");
    fs::write(&orphan_db_tmp, b"db tmp").unwrap();
    fs::write(&orphan_checksum_tmp, b"checksum tmp").unwrap();
    fs::write(&orphan_manifest_tmp, b"manifest tmp").unwrap();
    fs::write(&unrelated_tmp, b"keep").unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Temp cleanup v13.9")
        .arg("Production v13.9 should prune orphan backup temp files safely.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    assert_eq!(report_json["temp_pruned"].as_array().unwrap().len(), 3);
    assert!(!orphan_db_tmp.exists());
    assert!(!orphan_checksum_tmp.exists());
    assert!(!orphan_manifest_tmp.exists());
    assert!(unrelated_tmp.exists());

    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
}

#[test]
fn v13_10_backup_policy_prunes_orphan_sidecars() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    fs::create_dir_all(&backups).unwrap();
    let orphan_checksum = backups.join("dukememory-orphan.db.sha256");
    let orphan_manifest = backups.join("dukememory-orphan.db.manifest.json");
    let unrelated_checksum = backups.join("manual.db.sha256");
    fs::write(&orphan_checksum, b"orphan checksum").unwrap();
    fs::write(&orphan_manifest, b"{}").unwrap();
    fs::write(&unrelated_checksum, b"keep").unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Sidecar cleanup v13.10")
        .arg("Production v13.10 should prune sidecars whose backup DB is gone.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    assert_eq!(report_json["sidecar_pruned"].as_array().unwrap().len(), 2);
    assert!(!orphan_checksum.exists());
    assert!(!orphan_manifest.exists());
    assert!(unrelated_checksum.exists());

    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    assert!(backup.with_extension("db.sha256").exists());
    assert!(backup.with_extension("db.manifest.json").exists());
    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
}

#[test]
fn v13_11_backup_manifest_rejects_source_count_drift() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Manifest source counts v13.11")
        .arg("Production v13.11 should verify source counts recorded in backup manifests.")
        .assert()
        .success();

    let report = stdout(
        cmd(&db)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["verified"], true);
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());
    let manifest = std::path::PathBuf::from(report_json["manifest_file"].as_str().unwrap());

    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);
    assert_eq!(verify_json["manifest_ok"], true);

    let mut raw_manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    raw_manifest["source_table_counts"][0]["count"] = Value::from(9999);
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&raw_manifest).unwrap(),
    )
    .unwrap();

    let bad_verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let bad_verify_json: Value = serde_json::from_str(&bad_verify).unwrap();
    assert_eq!(bad_verify_json["verified"], false);
    assert_eq!(bad_verify_json["manifest_ok"], false);
    assert_eq!(bad_verify_json["strict_ok"], false);
    assert!(
        bad_verify_json["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "manifest_source_table_counts_mismatch")
    );
}

#[test]
fn v13_12_backup_verify_reasons_and_restore_journal() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.db");
    let target = dir.path().join("target.db");
    let backups = dir.path().join("backups");
    let rollbacks = dir.path().join("rollbacks");
    let journal_dir = dir.path().join("restore-journal");

    cmd(&source)
        .arg("add")
        .arg("decision")
        .arg("Restore journal v13.12")
        .arg("Production v13.12 should write restore journals.")
        .assert()
        .success();
    let report = stdout(
        cmd(&source)
            .arg("backup-policy")
            .arg("--output-dir")
            .arg(&backups)
            .arg("--keep")
            .arg("2")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    let backup = std::path::PathBuf::from(report_json["created"].as_str().unwrap());

    cmd(&target)
        .arg("add")
        .arg("decision")
        .arg("Journal target")
        .arg("This target should be preserved by rollback and recorded in a journal.")
        .assert()
        .success();

    cmd(&target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--dry-run")
        .arg("--rollback-dir")
        .arg(&rollbacks)
        .arg("--journal-dir")
        .arg(&journal_dir)
        .assert()
        .success()
        .stdout(contains("restore: verified"));
    assert!(!journal_dir.exists());

    cmd(&target)
        .arg("restore")
        .arg(&backup)
        .arg("--force")
        .arg("--strict")
        .arg("--rollback-dir")
        .arg(&rollbacks)
        .arg("--journal-dir")
        .arg(&journal_dir)
        .assert()
        .success()
        .stdout(contains("journal:"));

    let journal_files = fs::read_dir(&journal_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(journal_files.len(), 1);
    let journal: Value =
        serde_json::from_str(&fs::read_to_string(&journal_files[0]).unwrap()).unwrap();
    assert_eq!(journal["status"], "success");
    assert_eq!(journal["source"], backup.display().to_string());
    assert_eq!(journal["target"], target.display().to_string());
    assert_eq!(journal["strict"], true);
    assert_eq!(journal["dry_run"], false);
    assert_eq!(journal["rollback_enabled"], true);
    assert_eq!(journal["rollback_verified"], true);
    assert!(std::path::Path::new(journal["rollback"].as_str().unwrap()).exists());
    assert!(journal["error"].is_null());

    let manifest = std::path::PathBuf::from(report_json["manifest_file"].as_str().unwrap());
    let mut raw_manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    raw_manifest["backup_sha256"] = Value::String(
        "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
    );
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&raw_manifest).unwrap(),
    )
    .unwrap();
    let bad_verify = stdout(
        cmd(&target)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let bad_verify_json: Value = serde_json::from_str(&bad_verify).unwrap();
    assert_eq!(bad_verify_json["verified"], false);
    assert!(
        bad_verify_json["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "manifest_backup_sha256_mismatch")
    );
}

#[test]
fn v14_retrieve_v2_context_pack_v2_and_rhai_ranking() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let rules = dir.path().join("retrieve_rules.rhai");

    fs::write(
        &rules,
        r#"
        fn score_memory(type, status, scope, title, body, task, confidence) {
            if type == "known_issue" { 100.0 } else { 0.0 }
        }
        fn should_include(type, status, scope, title, body, task, confidence) {
            status == "active"
        }
        "#,
    )
    .unwrap();

    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Local fast memory")
        .arg("Memory retrieval constraints must stay local, fast, and easy for the coding agent.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Use grouped retrieval")
        .arg("Group retrieved memory into decisions, constraints, current facts, risks, and recent work.")
        .arg("--link")
        .arg("file:src/app.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("known_issue")
        .arg("Ranking risk")
        .arg("Bad ranking can hide constraints when the agent needs exact project memory.")
        .assert()
        .success();
    cmd(&db)
        .arg("session-close")
        .arg("--title")
        .arg("V14 retrieve work")
        .arg("--summary")
        .arg("Implemented retrieve v2 and context-pack v2.")
        .arg("--scope")
        .arg("project")
        .assert()
        .success();

    let fts_fallback = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("src app grouped retrieval constraints")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("deep")
            .arg("--scope")
            .arg("project"),
    );
    let fts_fallback_json: Value = serde_json::from_str(&fts_fallback).unwrap();
    assert_eq!(fts_fallback_json["semantic_used"], false);
    assert!(
        fts_fallback_json["semantic_error"]
            .as_str()
            .unwrap()
            .contains("semantic index not ready")
    );

    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieve = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("src app grouped retrieval constraints")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("deep")
            .arg("--scope")
            .arg("project"),
    );
    let retrieve_json: Value = serde_json::from_str(&retrieve).unwrap();
    assert_eq!(retrieve_json["version"], 14);
    assert_eq!(retrieve_json["semantic_used"], true);
    assert!(retrieve_json["hits"].as_array().unwrap().len() >= 3);
    assert!(retrieve_json["hits"][0]["score"].as_f64().unwrap() > 0.0);
    assert!(retrieve_json["hits"][0]["utility_score"].as_f64().unwrap() > 0.0);
    let all_reasons = retrieve_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|hit| hit["reasons"].as_array().unwrap().iter())
        .collect::<Vec<_>>();
    assert!(
        all_reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("semantic:"))
    );
    assert!(
        all_reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("link_match:"))
    );
    let useful_id = retrieve_json["hits"][0]["memory"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    stdout(
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(&useful_id)
            .arg("--rating")
            .arg("useful")
            .arg("--command")
            .arg("retrieve")
            .arg("--query")
            .arg("src app grouped retrieval constraints"),
    );
    let quality_ranked = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("src app grouped retrieval constraints")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--scope")
            .arg("project"),
    );
    let quality_ranked_json: Value = serde_json::from_str(&quality_ranked).unwrap();
    assert!(
        quality_ranked_json["hits"].as_array().unwrap().len() <= 5,
        "tiny retrieval should keep a compact hit list"
    );
    let quality_hits = quality_ranked_json["hits"].as_array().unwrap();
    if quality_hits.len() > 2 {
        let top_score = quality_hits[0]["score"].as_f64().unwrap();
        let relevance_floor = (top_score - 18.0).max(8.0);
        assert!(
            quality_hits
                .iter()
                .skip(2)
                .all(|hit| hit["score"].as_f64().unwrap() >= relevance_floor),
            "tiny retrieval should drop weak low-relevance tails"
        );
    }
    let quality_reasons = quality_ranked_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|hit| hit["reasons"].as_array().unwrap().iter())
        .collect::<Vec<_>>();
    assert!(
        quality_reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("recent_reads:"))
    );
    assert!(
        quality_reasons
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("useful_feedback:"))
    );
    let tiny = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("src app grouped retrieval constraints")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(tiny.len() <= 1200);
    assert!(tiny.contains("Relevant Memory:"));

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Focused snippet card")
        .arg(format!(
            "{} needle relevance floor exact detail should be visible {}",
            "prefix noise ".repeat(80),
            "tail noise ".repeat(80)
        ))
        .assert()
        .success();
    let focused_snippet = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("needle relevance floor")
            .arg("--strategy")
            .arg("fts")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(focused_snippet.len() <= 1200);
    assert!(focused_snippet.contains("Focused snippet card"));
    assert!(focused_snippet.contains("needle relevance floor exact detail"));
    assert!(!focused_snippet.contains(&"prefix noise ".repeat(20)));

    let boosted = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("ranking risk grouped")
            .arg("--format")
            .arg("json")
            .arg("--rules")
            .arg(&rules),
    );
    let boosted_json: Value = serde_json::from_str(&boosted).unwrap();
    assert_eq!(boosted_json["hits"][0]["memory"]["type"], "known_issue");
    assert!(
        boosted_json["hits"][0]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("rhai_score:"))
    );

    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Recent linked work")
        .arg("fresh linked update should enter through relevant recent context")
        .arg("--link")
        .arg("file:src/constraints/ranking.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Recent unrelated context")
        .arg("fresh billing export update should not enter an unrelated context pack")
        .assert()
        .success();
    let context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("constraints memory")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--max-chars")
            .arg("3000"),
    );
    assert!(context_pack.contains("Decisions:"));
    assert!(context_pack.contains("Constraints:"));
    assert!(context_pack.contains("Risks:"));
    assert!(!context_pack.contains("Recent unrelated context"));

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Context focused long card")
        .arg(format!(
            "{} constraints memory exact context detail should be visible {}",
            "context prefix noise ".repeat(80),
            "context tail noise ".repeat(80)
        ))
        .assert()
        .success();
    let focused_context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("constraints memory")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--max-chars")
            .arg("3000"),
    );
    assert!(focused_context_pack.contains("constraints memory exact context detail"));
    assert!(!focused_context_pack.contains(&"context prefix noise ".repeat(10)));

    let recent_context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("constraints ranking")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--max-chars")
            .arg("3000"),
    );
    assert!(recent_context_pack.contains("Recent Work:"));
    assert!(recent_context_pack.contains("Recent linked work"));
    assert!(!recent_context_pack.contains("Recent unrelated context"));

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Generic scoring terms")
        .arg("memory agent project retrieval token quality context recall brief semantic")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Recent unrelated fallback")
        .arg("fresh unrelated note should not appear for generic-only memory requests")
        .assert()
        .success();
    let generic_scoring = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("memory agent project retrieval token quality context recall brief semantic")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let generic_scoring_json: Value = serde_json::from_str(&generic_scoring).unwrap();
    assert_eq!(generic_scoring_json["semantic_used"], false);
    assert_eq!(generic_scoring_json["semantic_skipped"], true);
    assert_eq!(
        generic_scoring_json["semantic_skip_reason"],
        "generic_query"
    );
    assert!(generic_scoring_json["semantic_error"].is_null());
    assert!(
        generic_scoring_json["receipt"]
            .as_str()
            .unwrap()
            .contains("semantic search skipped")
    );
    assert!(
        !generic_scoring_json["receipt"]
            .as_str()
            .unwrap()
            .contains("semantic search fallback")
    );
    assert!(
        generic_scoring_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["memory"]["title"] != "Recent unrelated fallback"),
        "generic-only retrieval should not add unrelated recent fallback cards"
    );
    assert!(
        generic_scoring_json["hits"].as_array().unwrap().len() <= 2,
        "generic-only tiny retrieval should stay extra compact"
    );
    assert!(
        generic_scoring_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|hit| hit["reasons"].as_array().unwrap().iter())
            .all(|reason| !reason.as_str().unwrap().starts_with("text_match:"))
    );
    let generic_empty_plain = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("agents memories projects contexts")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(generic_empty_plain.contains("Relevant Memory:"));
    assert!(generic_empty_plain.contains("none (generic query; semantic search skipped)"));
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Single anchor relevant")
        .arg("singleanchor detail should be the only one-term retrieval match")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Recent unrelated one term fallback")
        .arg("fresh unrelated task state should not appear for weak one-term retrieval")
        .assert()
        .success();
    let one_term = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("singleanchor")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let one_term_json: Value = serde_json::from_str(&one_term).unwrap();
    assert_eq!(one_term_json["semantic_used"], false);
    assert_eq!(one_term_json["semantic_skipped"], true);
    assert_eq!(one_term_json["semantic_skip_reason"], "weak_query");
    let one_term_titles = one_term_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["memory"]["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(one_term_titles.contains(&"Single anchor relevant"));
    assert!(
        !one_term_titles.contains(&"Recent unrelated one term fallback"),
        "weak one-term retrieval should not add unrelated recent fallback cards"
    );
    let one_term_plain = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("unmatchedanchor")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(one_term_plain.contains("none (weak query; semantic search skipped)"));

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Diversity note {index}"))
            .arg(
                "Diversity budget target should not let one memory type fill every retrieval slot.",
            )
            .assert()
            .success();
    }
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Diversity command")
        .arg("Run diversity budget target check before release.")
        .assert()
        .success();
    let diverse = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("diversity budget target")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--limit")
            .arg("3"),
    );
    let diverse_json: Value = serde_json::from_str(&diverse).unwrap();
    let diverse_types = diverse_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["memory"]["type"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert!(diverse_types.len() >= 2);

    for index in 0..3 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Duplicate quality note {index}"))
            .arg("Duplicate quality budget target should appear once in a tiny retrieval pack.")
            .assert()
            .success();
    }
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Duplicate quality command")
        .arg("Run duplicate quality budget target validation command.")
        .assert()
        .success();
    let deduped = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("duplicate quality budget target")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--limit")
            .arg("5")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let deduped_json: Value = serde_json::from_str(&deduped).unwrap();
    let duplicate_notes = deduped_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|hit| {
            hit["memory"]["title"]
                .as_str()
                .unwrap()
                .starts_with("Duplicate quality note")
        })
        .count();
    assert_eq!(
        duplicate_notes, 1,
        "tiny retrieval should keep only one near-duplicate card"
    );
}

#[test]
fn retrieve_skips_semantic_when_embedding_provider_is_unreachable() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let title = "Unreachable embedding provider";
    let body = "unreachable provider fallback vector health should stay fast";
    let endpoint = "http://127.0.0.1:9";
    let model = "bge-m3:latest";

    let memory_id = stdout(cmd(&db).arg("add").arg("design_note").arg(title).arg(body))
        .trim()
        .to_string();
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO memory_embeddings \
         (memory_id, model, endpoint, dimensions, embedding, content_hash, updated_at) \
         VALUES (?1, ?2, ?3, 3, '[0.1,0.2,0.3]', ?4, ?5)",
        params![
            memory_id,
            model,
            format!("ollama:{endpoint}"),
            memory_content_hash("design_note", "project", title, body, "active"),
            now_ms(),
        ],
    )
    .unwrap();
    drop(conn);

    let started = std::time::Instant::now();
    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("unreachable provider fallback vector health")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg(endpoint)
            .arg("--model")
            .arg(model)
            .arg("--budget-profile")
            .arg("deep"),
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "unreachable embedding provider should fall back before the embedding request timeout"
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], false);
    assert!(
        retrieved_json["semantic_error"]
            .as_str()
            .unwrap()
            .contains("embedding provider is not reachable")
    );
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["memory"]["title"] == title)
    );
}

#[test]
fn embed_status_caches_unreachable_provider_health_between_cli_runs() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(stream);
        }
    });

    let first_started = std::time::Instant::now();
    let first = stdout(
        cmd(&db)
            .arg("embed-status")
            .arg("--json")
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg(&endpoint)
            .arg("--model")
            .arg("bge-m3:latest"),
    );
    assert!(
        first_started.elapsed() >= std::time::Duration::from_millis(900),
        "first health check should exercise the slow provider path"
    );
    let first_json: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first_json["provider_reachable"], false);

    let second_started = std::time::Instant::now();
    let second = stdout(
        cmd(&db)
            .arg("embed-status")
            .arg("--json")
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg(&endpoint)
            .arg("--model")
            .arg("bge-m3:latest"),
    );
    assert!(
        second_started.elapsed() < std::time::Duration::from_millis(500),
        "second health check should reuse SQLite cooldown"
    );
    let second_json: Value = serde_json::from_str(&second).unwrap();
    assert_eq!(second_json["provider_reachable"], false);
    assert_eq!(second_json["provider_error"], first_json["provider_error"]);

    server.join().unwrap();
}

#[test]
fn embed_index_fails_fast_when_embedding_provider_is_unreachable() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(stream);
        }
    });
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Embed index fail fast")
        .arg("Embedding index should not hang when provider health is down.")
        .assert()
        .success();

    let started = std::time::Instant::now();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("ollama")
        .arg("--endpoint")
        .arg(&endpoint)
        .arg("--model")
        .arg("bge-m3:latest")
        .assert()
        .failure()
        .stderr(contains(
            "embedding provider is not reachable; skipping embed-index",
        ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "embed-index should fail before the long embedding request timeout"
    );

    server.join().unwrap();
}

#[test]
fn autonomous_run_skips_embed_index_when_provider_is_unreachable() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let status_file = dir.path().join(".agent/autonomous-status.json");
    let rollback_dir = dir.path().join(".agent/autonomous-rollbacks");
    let backup_dir = dir.path().join(".agent/backups");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(stream);
        }
    });
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Autonomous down provider")
        .arg("Autonomous maintenance should continue when embedding provider is down.")
        .assert()
        .success();

    let started = std::time::Instant::now();
    let run = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg(&endpoint)
            .arg("--model")
            .arg("bge-m3:latest")
            .arg("--json"),
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(6),
        "autonomous maintenance should skip embeddings before the long embedding timeout"
    );
    let run_json: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run_json["ok"], true);
    let actions = run_json["actions"].as_array().unwrap();
    assert!(actions.iter().any(|item| {
        item["kind"] == "embed_index"
            && item["status"] == "skipped"
            && item["detail"]
                .as_str()
                .unwrap()
                .contains("embedding provider is not reachable")
    }));
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "optimize_storage" && item["status"] == "ok")
    );

    server.join().unwrap();
}

#[test]
fn v14_retrieve_filters_weak_semantic_candidates() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Strong auth rate")
        .arg("auth rate limit local fast verified release")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic distractor")
        .arg("invoice sqlite browser")
        .assert()
        .success();
    let weak_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Weak billing cache")
            .arg("billing invoice cache export csv unrelated payment"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("update")
        .arg(&weak_id)
        .arg("--status")
        .arg("uncertain")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("auth rate limit")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], true);
    let titles = retrieved_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["memory"]["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(titles.contains(&"Strong auth rate"));
    assert!(
        !titles.contains(&"Semantic distractor"),
        "tiny hybrid retrieval should filter medium-score semantic-only candidates without lexical anchors"
    );
    assert!(
        !titles.contains(&"Weak billing cache"),
        "tiny hybrid retrieval should filter weak semantic-only candidates"
    );
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|hit| hit["semantic_score"].as_f64())
            .all(|score| score >= 0.18)
    );

    let agent_context = stdout(
        cmd(&db)
            .arg("context")
            .arg("auth rate limit")
            .arg("--mode")
            .arg("agent")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--embed-provider")
            .arg("mock")
            .arg("--embed-endpoint")
            .arg("local")
            .arg("--embed-model")
            .arg("mock-small"),
    );
    let agent_context_json: Value = serde_json::from_str(&agent_context).unwrap();
    let agent_titles = agent_context_json["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(agent_titles.contains(&"Strong auth rate"));
    assert!(
        !agent_titles.contains(&"Semantic distractor"),
        "tiny agent context should filter medium-score semantic-only candidates without lexical anchors"
    );
    assert!(
        !agent_titles.contains(&"Weak billing cache"),
        "tiny agent context should filter weak semantic-only candidates"
    );

    let semantic_context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("auth rate limit")
            .arg("--semantic")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--embed-provider")
            .arg("mock")
            .arg("--embed-endpoint")
            .arg("local")
            .arg("--embed-model")
            .arg("mock-small"),
    );
    let semantic_context_pack_json: Value = serde_json::from_str(&semantic_context_pack).unwrap();
    let context_pack_titles = semantic_context_pack_json
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(context_pack_titles.contains(&"Strong auth rate"));
    assert!(
        !context_pack_titles.contains(&"Semantic distractor"),
        "tiny context-pack semantic should filter medium-score semantic-only candidates without lexical anchors"
    );
    assert!(
        !context_pack_titles.contains(&"Weak billing cache"),
        "tiny context-pack semantic should filter weak semantic-only candidates"
    );
}

#[test]
fn v14_tiny_retrieval_applies_relative_semantic_score_floor() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Exact embedding target")
        .arg("orchard vector needle alpha beta gamma delta")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Loose embedding tail")
        .arg("orchard vector cache invoice export dashboard beta")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Far embedding tail")
        .arg("invoice sqlite browser payment export csv")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("orchard vector needle alpha")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], true);
    let hits = retrieved_json["hits"].as_array().unwrap();
    let exact = hits
        .iter()
        .find(|hit| hit["memory"]["title"] == "Exact embedding target")
        .expect("exact semantic target should be retained");
    assert!(
        exact["semantic_score"].as_f64().unwrap() >= 0.36,
        "best semantic target should keep its embedding score"
    );
    if let Some(loose) = hits
        .iter()
        .find(|hit| hit["memory"]["title"] == "Loose embedding tail")
    {
        assert!(
            loose["semantic_score"].is_null(),
            "weak semantic tail can remain only as lexical retrieval, not semantic"
        );
    }
}

#[test]
fn v14_tiny_retrieval_deduplicates_semantic_overlap() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic overlap alpha one")
        .arg("anchor beta gamma delta copper ivory cobalt")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic overlap alpha two")
        .arg("anchor beta gamma epsilon copper topaz ruby")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Different command target")
        .arg("memory qa recall policy install release")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("semantic overlap alpha anchor beta gamma")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], true);
    let overlap_hits = retrieved_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|hit| {
            hit["memory"]["title"]
                .as_str()
                .unwrap()
                .starts_with("Semantic overlap alpha")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        overlap_hits.len(),
        1,
        "tiny retrieval should keep the strongest semantic card from a redundant cluster"
    );
    assert!(overlap_hits[0]["semantic_score"].as_f64().unwrap() > 0.7);
}

#[test]
fn v14_tiny_relevance_floor_can_keep_one_strong_card() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Needlefloor exact strong")
        .arg("needlefloor exact release guard must remain verified local fast and deterministic")
        .arg("--confidence")
        .arg("1.0")
        .arg("--link")
        .arg("file:needlefloor/exact.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("note")
        .arg("Needlefloor exact weak")
        .arg("needlefloor exact")
        .arg("--status")
        .arg("uncertain")
        .arg("--confidence")
        .arg("0.1")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("needlefloor exact")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    let hits = retrieved_json["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["memory"]["title"], "Needlefloor exact strong");
}

#[test]
fn v14_tiny_retrieval_drops_partial_lexical_noise() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Auth rollout exact")
        .arg("auth rollout validation must stay local fast deterministic and verified")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Auth partial noisy")
        .arg("auth platform ownership belongs to a different operational area")
        .arg("--confidence")
        .arg("1.0")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Recent unrelated noisy")
        .arg("billing export preferences belong to a different operational area")
        .arg("--confidence")
        .arg("1.0")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("auth rollout")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    let titles = retrieved_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["memory"]["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(titles, vec!["Auth rollout exact"]);
}

#[test]
fn v14_tiny_retrieval_filters_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "checkout validation token budget";
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Noisy checkout memory")
            .arg("checkout validation token budget noisy card should be suppressed after same-query feedback"),
    )
    .trim()
    .to_string();
    let useful_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Useful checkout memory")
            .arg("checkout validation token budget useful card should remain in tiny retrieval"),
    )
    .trim()
    .to_string();
    let reusable_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Reusable checkout memory")
            .arg("checkout validation token budget reusable card should not be globally banned"),
    )
    .trim()
    .to_string();

    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("retrieve")
        .arg("--query")
        .arg(query)
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&reusable_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("retrieve")
        .arg("--query")
        .arg("billing invoice export")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg(query)
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    let ids = retrieved_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["memory"]["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        !ids.contains(&noisy_id.as_str()),
        "same-query useless feedback should suppress noisy tiny hits"
    );
    assert!(ids.contains(&useful_id.as_str()));
    assert!(
        ids.contains(&reusable_id.as_str()),
        "unrelated useless feedback should not globally ban a memory card"
    );
}

#[test]
fn retrieval_learns_intent_type_weights_from_feedback_events() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let issue_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("known_issue")
            .arg("Checkout failure issue")
            .arg("checkout failure debug marker exact should be preferred after useful feedback"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Checkout failure command")
        .arg("checkout failure debug marker exact validation command")
        .assert()
        .success();

    for _ in 0..2 {
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(&issue_id)
            .arg("--rating")
            .arg("useful")
            .arg("--command")
            .arg("retrieve")
            .arg("--query")
            .arg("fix checkout failure")
            .assert()
            .success();
    }

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("fix checkout failure")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    let issue_hit = retrieved_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["memory"]["id"] == issue_id)
        .unwrap();
    assert!(
        issue_hit["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason
                .as_str()
                .unwrap()
                .starts_with("intent_feedback:debug:known_issue:")),
        "known_issue should carry learned debug intent/type feedback"
    );
}

#[test]
fn tiny_retrieval_suppresses_compacted_history_unless_requested() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Budget target policy")
        .arg("Budget target policy keeps recall precise for the current agent task.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Autonomous compacted project release history")
        .arg("Autonomously compacted release history: budget target policy appeared in an old release note.")
        .assert()
        .success();

    let focused = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("budget target policy")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let focused_json: Value = serde_json::from_str(&focused).unwrap();
    assert!(
        focused_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["memory"]["title"] != "Autonomous compacted project release history"),
        "tiny task recall should not include compacted release history unless requested"
    );

    let history = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("release history budget target")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let history_json: Value = serde_json::from_str(&history).unwrap();
    assert!(
        history_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["memory"]["title"] == "Autonomous compacted project release history"),
        "explicit release-history queries should still reach compacted history cards"
    );
}

#[test]
fn tiny_retrieval_caps_broad_release_task_state_tail() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for (title, body) in [
        (
            "Budget target first release",
            "Released dukememory with budget target alpha planner window storage guard.",
        ),
        (
            "Budget target second release",
            "Released dukememory with budget target beta scheduler cache telemetry guard.",
        ),
    ] {
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg(title)
            .arg(body)
            .assert()
            .success();
    }
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Budget target evergreen design")
        .arg("Budget target policy retrieval behavior should prefer durable design notes.")
        .assert()
        .success();

    let focused = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("budget target policy")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let focused_json: Value = serde_json::from_str(&focused).unwrap();
    let focused_hits = focused_json["hits"].as_array().unwrap();
    let focused_release_task_states = focused_hits
        .iter()
        .filter(|hit| {
            hit["memory"]["type"] == "task_state"
                && hit["memory"]["title"].as_str().unwrap().contains("release")
        })
        .count();
    assert_eq!(
        focused_release_task_states, 1,
        "ordinary tiny recall should keep at most one broad release task_state"
    );
    assert!(
        focused_hits
            .iter()
            .any(|hit| hit["memory"]["title"] == "Budget target evergreen design")
    );

    let release = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("budget target release")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let release_json: Value = serde_json::from_str(&release).unwrap();
    let release_task_states = release_json["hits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|hit| {
            hit["memory"]["type"] == "task_state"
                && hit["memory"]["title"].as_str().unwrap().contains("release")
        })
        .count();
    assert!(
        release_task_states >= 2,
        "explicit release queries should not cap release task_state cards"
    );
}

#[test]
fn tiny_retrieval_suppresses_contract_cards_unless_requested() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let contract_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Project memory contract")
            .arg("# dukememory. Project Contract\nRules: summary budget exact must keep recall small."),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Summary budget exact implementation")
        .arg("summary budget exact implementation should be selected for ordinary recall work.")
        .assert()
        .success();
    for _ in 0..16 {
        insert_read_event_with_ids(&db, "brief", "summary budget exact", &[&contract_id]);
    }

    let focused = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("summary budget exact")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let focused_json: Value = serde_json::from_str(&focused).unwrap();
    assert!(
        focused_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["memory"]["title"] != "Project memory contract"),
        "ordinary tiny recall should not include project memory contract cards"
    );
    assert!(
        focused_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["memory"]["title"] == "Summary budget exact implementation")
    );

    let contract = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("memory contract")
            .arg("--strategy")
            .arg("fts")
            .arg("--format")
            .arg("json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let contract_json: Value = serde_json::from_str(&contract).unwrap();
    assert!(
        contract_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["memory"]["title"] == "Project memory contract"),
        "explicit memory contract queries should still retrieve contract cards"
    );
}

#[test]
fn quality_report_caps_broad_history_read_boost() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let history_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("Autonomous compacted project release history")
            .arg("Autonomously compacted release history: old release notes are useful only for explicit history lookup.")
            .arg("--link")
            .arg("file:src/app/autonomous.rs"),
    )
    .trim()
    .to_string();
    for _ in 0..16 {
        insert_read_event_with_ids(
            &db,
            "brief",
            "memory quality release history",
            &[&history_id],
        );
    }

    let quality = stdout(
        cmd(&db)
            .arg("quality-report")
            .arg("--json")
            .arg("--limit")
            .arg("20"),
    );
    let quality_json: Value = serde_json::from_str(&quality).unwrap();
    let history = quality_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == history_id)
        .unwrap();
    assert!(
        history["score"].as_f64().unwrap() < 90.0,
        "broad history cards should not reach top quality solely from frequent reads"
    );
    assert!(history["reasons"].as_array().unwrap().iter().any(|reason| {
        reason
            .as_str()
            .unwrap()
            .contains("frequent reads are capped")
    }));
}

#[test]
fn v14_context_and_impact_filter_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "checkout validation token budget";
    let mut noisy_ids: Vec<String> = Vec::new();
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Noisy context memory")
            .arg("checkout validation token budget noisy context card should be suppressed")
            .arg("--link")
            .arg("file:src/checkout.rs"),
    )
    .trim()
    .to_string();
    noisy_ids.push(noisy_id);
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful context memory")
        .arg("checkout validation token budget useful context card should remain")
        .arg("--link")
        .arg("file:src/checkout.rs")
        .assert()
        .success();
    for index in 0..4 {
        let noisy_id = stdout(
            cmd(&db)
                .arg("add")
                .arg("design_note")
                .arg(format!("Noisy context memory {index}"))
                .arg("checkout validation token budget noisy context card should be suppressed")
                .arg("--link")
                .arg("file:src/checkout.rs"),
        )
        .trim()
        .to_string();
        noisy_ids.push(noisy_id);
    }

    for noisy_id in noisy_ids {
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(&noisy_id)
            .arg("--rating")
            .arg("useless")
            .arg("--command")
            .arg("context-pack")
            .arg("--query")
            .arg(query)
            .assert()
            .success();
    }

    let context = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg(query)
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(!context.contains("Noisy context memory"));
    assert!(!context.contains("Noisy context memory 0"));
    assert!(context.contains("Useful context memory"));

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg(query)
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(!impact.contains("Noisy context memory"));
    assert!(impact.contains("Useful context memory"));
}

#[test]
fn v14_semantic_context_filters_query_useless_feedback() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let query = "auth rate limit";
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Noisy semantic context")
            .arg(
                "auth rate limit noisy semantic context card should not return through embeddings",
            ),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful semantic context")
        .arg("auth rate limit useful semantic context card should remain")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("context-pack")
        .arg("--query")
        .arg(query)
        .assert()
        .success();

    let semantic_context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg(query)
            .arg("--semantic")
            .arg("--json")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--embed-provider")
            .arg("mock")
            .arg("--embed-endpoint")
            .arg("local")
            .arg("--embed-model")
            .arg("mock-small"),
    );
    let semantic_context_pack_json: Value = serde_json::from_str(&semantic_context_pack).unwrap();
    let titles = semantic_context_pack_json
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        !titles.contains(&"Noisy semantic context"),
        "semantic additions should not re-add same-query useless context"
    );
    assert!(titles.contains(&"Useful semantic context"));
}

#[test]
fn v14_recall_uses_query_focused_summaries() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Recall focused long note")
        .arg(format!(
            "{} needle exact useful detail should be visible {}",
            "prefix noise ".repeat(80),
            "tail noise ".repeat(80)
        ))
        .assert()
        .success();

    let recall = stdout(
        cmd(&db)
            .arg("recall")
            .arg("needle exact")
            .arg("--max-chars")
            .arg("800")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let recall_json: Value = serde_json::from_str(&recall).unwrap();
    let summary = recall_json["items"][0]["summary"].as_str().unwrap();
    assert!(summary.contains("needle exact useful detail"));
    assert!(!summary.contains(&"prefix noise ".repeat(8)));
}

#[test]
fn recall_json_respects_tight_max_chars() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Recall json budget {index}"))
            .arg(format!(
                "recall json budget exact useful detail variant{index} {}",
                "tail noise ".repeat(30)
            ))
            .assert()
            .success();
    }

    let recall = stdout(
        cmd(&db)
            .arg("recall")
            .arg("recall json budget")
            .arg("--max-chars")
            .arg("360")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    assert!(
        recall.len() <= 360,
        "recall JSON exceeded budget: {}",
        recall.len()
    );
    let recall_json: Value = serde_json::from_str(&recall).unwrap();
    assert_eq!(recall_json["max_chars"], 360);
    assert!(recall_json["items"].as_array().unwrap().len() <= 1);
    let item_count = recall_json["items"].as_array().unwrap().len();
    let expected_receipt_count = match item_count {
        1 => "matched 1 card".to_string(),
        count => format!("matched {count} cards"),
    };
    assert!(
        recall_json["receipt"]
            .as_str()
            .unwrap()
            .contains(&expected_receipt_count),
        "tight recall receipt should match the rendered item count"
    );
    if let Some(item) = recall_json["items"].as_array().unwrap().first() {
        assert!(item["summary"].as_str().unwrap().len() <= 90);
    }

    let long_query_recall = stdout(
        cmd(&db)
            .arg("recall")
            .arg(format!(
                "recall json budget {}",
                "memory quality embedding ".repeat(20)
            ))
            .arg("--max-chars")
            .arg("360")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    assert!(
        long_query_recall.len() <= 360,
        "long-query recall JSON exceeded budget: {}",
        long_query_recall.len()
    );
    let long_query_json: Value = serde_json::from_str(&long_query_recall).unwrap();
    let long_query_item_count = long_query_json["items"].as_array().unwrap().len();
    let expected_receipt_count = match long_query_item_count {
        1 => "matched 1 card".to_string(),
        count => format!("matched {count} cards"),
    };
    assert!(
        long_query_json["receipt"]
            .as_str()
            .unwrap()
            .contains(&expected_receipt_count),
        "recall receipt should match the rendered item count"
    );
}

#[test]
fn recall_items_are_budget_aware() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..8 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Recall budget note {index}"))
            .arg(format!(
                "recall budget signal useful detail variant{index} area{index} path{index}"
            ))
            .assert()
            .success();
    }

    let tiny = stdout(
        cmd(&db)
            .arg("recall")
            .arg("recall budget signal")
            .arg("--max-chars")
            .arg("1200")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    let tiny_json: Value = serde_json::from_str(&tiny).unwrap();
    let tiny_len = tiny_json["items"].as_array().unwrap().len();
    assert!(tiny_len <= 3);

    let normal = stdout(
        cmd(&db)
            .arg("recall")
            .arg("recall budget signal")
            .arg("--max-chars")
            .arg("3000")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    let normal_json: Value = serde_json::from_str(&normal).unwrap();
    let normal_len = normal_json["items"].as_array().unwrap().len();
    assert!(normal_len <= 5);
    assert!(normal_len >= tiny_len);
}

#[test]
fn v14_agent_context_filters_next_actions() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Checkout validation rule")
        .arg("checkout validation must stay fast and deterministic")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Checkout validation next action")
        .arg("checkout validation follow-up should be visible in next actions")
        .assert()
        .success();
    let noisy_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("Checkout validation noisy action")
            .arg("checkout validation noisy follow-up should be filtered from next actions"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Billing export unrelated action")
        .arg("billing export follow-up should not enter checkout context")
        .assert()
        .success();
    cmd(&db)
        .arg("feedback")
        .arg("--id")
        .arg(&noisy_id)
        .arg("--rating")
        .arg("useless")
        .arg("--command")
        .arg("context")
        .arg("--query")
        .arg("checkout validation")
        .assert()
        .success();

    let context = stdout(
        cmd(&db)
            .arg("context")
            .arg("checkout validation")
            .arg("--mode")
            .arg("agent")
            .arg("--max-chars")
            .arg("1200"),
    );
    assert!(context.len() <= 1200);
    assert!(context.contains("Next Actions:"));
    assert!(context.contains("- Checkout validation next action"));
    assert!(!context.contains("Checkout validation noisy action"));
    assert!(!context.contains("Billing export unrelated action"));
}

#[test]
fn context_rows_are_budget_aware() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..10 {
        let memory_type = match index % 5 {
            0 => "decision",
            1 => "constraint",
            2 => "known_issue",
            3 => "task_state",
            _ => "design_note",
        };
        cmd(&db)
            .arg("add")
            .arg(memory_type)
            .arg(format!("Context budget card {index}"))
            .arg(format!(
                "context budget signal useful detail variant{index} area{index}"
            ))
            .assert()
            .success();
    }

    let tiny_context = stdout(
        cmd(&db)
            .arg("context")
            .arg("context budget signal")
            .arg("--mode")
            .arg("fast")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--limit")
            .arg("10")
            .arg("--json"),
    );
    let tiny_context_json: Value = serde_json::from_str(&tiny_context).unwrap();
    let tiny_context_len = tiny_context_json["memories"].as_array().unwrap().len();
    assert!(tiny_context_len <= 4);

    let normal_context = stdout(
        cmd(&db)
            .arg("context")
            .arg("context budget signal")
            .arg("--mode")
            .arg("fast")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--limit")
            .arg("10")
            .arg("--json"),
    );
    let normal_context_json: Value = serde_json::from_str(&normal_context).unwrap();
    let normal_context_len = normal_context_json["memories"].as_array().unwrap().len();
    assert!(normal_context_len <= 8);
    assert!(normal_context_len >= tiny_context_len);

    let tiny_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("context budget signal")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--limit")
            .arg("10")
            .arg("--json"),
    );
    let tiny_pack_json: Value = serde_json::from_str(&tiny_pack).unwrap();
    let tiny_pack_len = tiny_pack_json.as_array().unwrap().len();
    assert!(tiny_pack_len <= 4);

    let normal_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("context budget signal")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--limit")
            .arg("10")
            .arg("--json"),
    );
    let normal_pack_json: Value = serde_json::from_str(&normal_pack).unwrap();
    let normal_pack_len = normal_pack_json.as_array().unwrap().len();
    assert!(normal_pack_len <= 8);
    assert!(normal_pack_len >= tiny_pack_len);
}

#[test]
fn context_pack_json_is_compact_and_query_focused() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Context pack JSON long body")
        .arg(format!(
            "{} context pack exact useful detail should be visible {}",
            "context pack prefix noise ".repeat(80),
            "context pack tail noise ".repeat(80)
        ))
        .arg("--link")
        .arg("file:src/context_pack.rs")
        .assert()
        .success();

    let context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("context pack exact")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--json"),
    );
    let context_json: Value = serde_json::from_str(&context_pack).unwrap();
    let memory = &context_json.as_array().unwrap()[0];
    assert!(memory.get("body").is_none());
    assert!(
        memory["summary"]
            .as_str()
            .unwrap()
            .contains("exact useful detail")
    );
    assert_eq!(memory["links"][0]["target"], "src/context_pack.rs");
    assert!(!context_pack.contains(&"context pack prefix noise ".repeat(20)));
}

#[test]
fn context_pack_json_respects_tight_max_chars_and_usage() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Context pack budget {index}"))
            .arg(format!(
                "context pack budget exact useful detail variant{index} {}",
                "tail noise ".repeat(50)
            ))
            .assert()
            .success();
    }

    let context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("context pack budget")
            .arg("--max-chars")
            .arg("360")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    assert!(
        context_pack.len() <= 360,
        "context-pack JSON exceeded budget: {}",
        context_pack.len()
    );
    let context_json: Value = serde_json::from_str(&context_pack).unwrap();
    let rendered_count = context_json.as_array().unwrap().len();

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    let read = &usage_json["recent_reads"][0];
    assert_eq!(read["command"], "context-pack");
    assert_eq!(
        read["result_count"].as_u64().unwrap(),
        rendered_count as u64
    );
    assert_eq!(read["memory_ids"].as_array().unwrap().len(), rendered_count);
}

#[test]
fn search_falls_back_to_bounded_overlap_for_long_queries() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Context pack machine readable path")
        .arg(
            "Context-pack JSON full body removal uses compact summaries and token budget behavior.",
        )
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Unrelated token budget note")
        .arg("Token budget alone is not enough to satisfy a focused request.")
        .assert()
        .success();

    let search = stdout(
        cmd(&db)
            .arg("search")
            .arg("context-pack json full body compact summary token budget")
            .arg("--json"),
    );
    let search_json: Value = serde_json::from_str(&search).unwrap();
    let titles = search_json
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(titles.contains(&"Context pack machine readable path"));
    assert!(!titles.contains(&"Unrelated token budget note"));

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg("context-pack json full body compact summary token budget")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(impact.contains("Context pack machine readable path"));
    assert!(!impact.contains("Unrelated token budget note"));
}

#[test]
fn impact_uses_semantic_fallback_for_underfilled_natural_queries() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic impact fallback")
        .arg("semantic impact useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg("semantic impact fallback orchard")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let impact_json: Value = serde_json::from_str(&impact).unwrap();
    assert_eq!(impact_json["semantic_used"], true);
    let related_titles = impact_json["related"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(related_titles.contains(&"Semantic impact fallback"));
}

#[test]
fn search_and_mcp_search_use_semantic_fallback_for_underfilled_queries() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic search fallback")
        .arg("semantic search useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let search = stdout(
        cmd(&db)
            .arg("search")
            .arg("semantic search fallback orchard")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let search_json: Value = serde_json::from_str(&search).unwrap();
    assert!(
        search_json
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["title"] == "Semantic search fallback")
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    assert_eq!(usage_json["recent_reads"][0]["command"], "search");
    assert_eq!(usage_json["recent_reads"][0]["semantic_used"], true);

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"semantic search fallback orchard","max_chars":1000,"provider":"mock","endpoint":"local","model":"mock-small"}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Semantic search fallback"));
}

#[test]
fn mcp_context_surfaces_use_semantic_supplement() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic context fallback")
        .arg("semantic context fallback useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        for (id, name) in [(1, "memory_context_pack"), (2, "memory_agent_context")] {
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":{"task":"semantic context fallback orchard","max_chars":1000,"provider":"mock","endpoint":"local","model":"mock-small"}}})
            )
            .unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mcp_stdout = String::from_utf8(output.stdout).unwrap();
    let values = mcp_stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    for value in values {
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Semantic context fallback"));
    }

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    for command in ["memory_context_pack", "memory_agent_context"] {
        assert!(
            usage_json["recent_reads"]
                .as_array()
                .unwrap()
                .iter()
                .any(|read| read["command"] == command && read["semantic_used"] == true),
            "expected semantic read event for {command}"
        );
    }
}

#[test]
fn mcp_context_surfaces_log_only_rendered_memories() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("MCP tight context {index}"))
            .arg(format!(
                "mcp tight context exact useful detail variant{index} {}",
                "tail noise ".repeat(50)
            ))
            .assert()
            .success();
    }

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        for (id, name) in [(1, "memory_context_pack"), (2, "memory_agent_context")] {
            writeln!(
                stdin,
                "{}",
                serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":{"task":"mcp tight context","max_chars":80,"limit":8}}})
            )
            .unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mcp_stdout = String::from_utf8(output.stdout).unwrap();
    for line in mcp_stdout.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.len() <= 80,
            "MCP context text exceeded budget: {}",
            text.len()
        );
        assert_eq!(
            text.lines().filter(|line| line.starts_with("- ")).count(),
            0,
            "tight MCP context should not render memory cards that do not fit"
        );
    }

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    for command in ["memory_context_pack", "memory_agent_context"] {
        let read = usage_json["recent_reads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|read| read["command"] == command)
            .unwrap();
        assert_eq!(read["result_count"], 0);
        assert_eq!(read["memory_ids"].as_array().unwrap().len(), 0);
    }
}

#[test]
fn mcp_memory_search_logs_only_rendered_items() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("MCP search budget {index}"))
            .arg(format!(
                "mcp search budget exact useful detail variant{index} {}",
                "tail noise ".repeat(40)
            ))
            .assert()
            .success();
    }

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"mcp search budget","max_chars":80,"limit":8}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mcp_stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(mcp_stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    let rendered_items: Value = serde_json::from_str(text).unwrap();
    assert_eq!(rendered_items.as_array().unwrap().len(), 0);

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    let read = &usage_json["recent_reads"][0];
    assert_eq!(read["command"], "memory_search");
    assert_eq!(read["result_count"], 0);
    assert_eq!(read["memory_ids"].as_array().unwrap().len(), 0);
}

#[test]
fn mcp_brief_and_impact_log_only_budgeted_json_items() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("MCP rendered JSON {index}"))
            .arg(format!(
                "mcp rendered json exact useful detail variant{index} {}",
                "tail noise ".repeat(50)
            ))
            .arg("--link")
            .arg("file:src/mcp_rendered.rs")
            .assert()
            .success();
    }

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"mcp rendered json","budget":1200,"max_chars":1000}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_impact","arguments":{"target":"src/mcp_rendered.rs","budget":1200,"max_chars":1000}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mcp_stdout = String::from_utf8(output.stdout).unwrap();
    let mut rendered_by_id = HashMap::new();
    for line in mcp_stdout.lines() {
        let value: Value = serde_json::from_str(line).unwrap();
        let id = value["id"].as_i64().unwrap();
        let text = value["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.len() <= 1000,
            "MCP memory JSON exceeded budget: {}",
            text.len()
        );
        rendered_by_id.insert(id, serde_json::from_str::<Value>(text).unwrap());
    }

    let brief_json = rendered_by_id.get(&1).unwrap();
    let impact_json = rendered_by_id.get(&2).unwrap();
    let brief_ids = json_section_ids(brief_json, &["must_follow", "relevant", "risks"]);
    let impact_ids = json_section_ids(
        impact_json,
        &["decisions", "constraints", "risks", "related"],
    );
    assert!(brief_ids.len() < 4);
    assert!(impact_ids.len() < 4);
    assert!(
        brief_json["receipt"]
            .as_str()
            .unwrap()
            .contains(&format!("matched {} card", brief_ids.len()))
    );
    assert!(
        impact_json["receipt"]
            .as_str()
            .unwrap()
            .contains(&format!("matched {} card", impact_ids.len()))
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    for (command, ids) in [("brief", brief_ids), ("impact", impact_ids)] {
        let read = usage_json["recent_reads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|read| read["command"] == command)
            .unwrap();
        let mut logged_ids = read["memory_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let mut rendered_ids = ids;
        logged_ids.sort();
        rendered_ids.sort();
        assert_eq!(
            read["result_count"].as_u64().unwrap(),
            rendered_ids.len() as u64
        );
        assert_eq!(logged_ids, rendered_ids);
        assert_eq!(read["budget"], 1000);
    }
}

#[test]
fn mcp_snapshot_query_uses_semantic_supplement() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic snapshot fallback")
        .arg("semantic snapshot fallback useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_snapshot","arguments":{"query":"semantic snapshot fallback orchard","max_chars":1000,"provider":"mock","endpoint":"local","model":"mock-small"}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let mcp_stdout = String::from_utf8(output.stdout).unwrap();
    let value: Value = serde_json::from_str(mcp_stdout.lines().next().unwrap()).unwrap();
    let text = value["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Semantic snapshot fallback"));

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    assert!(
        usage_json["recent_reads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|read| read["command"] == "memory_snapshot" && read["semantic_used"] == true)
    );
}

#[test]
fn context_pack_uses_semantic_supplement_by_default() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic context fallback")
        .arg("semantic context fallback useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let context_pack = stdout(
        cmd(&db)
            .arg("context-pack")
            .arg("semantic context fallback orchard")
            .arg("--json")
            .arg("--embed-provider")
            .arg("mock")
            .arg("--embed-endpoint")
            .arg("local")
            .arg("--embed-model")
            .arg("mock-small"),
    );
    let context_json: Value = serde_json::from_str(&context_pack).unwrap();
    assert!(
        context_json
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["title"] == "Semantic context fallback")
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    assert_eq!(usage_json["recent_reads"][0]["command"], "context-pack");
    assert_eq!(usage_json["recent_reads"][0]["semantic_used"], true);
}

#[test]
fn agent_context_logs_semantic_usage() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Semantic agent context")
        .arg("semantic agent context useful memory should be found through embeddings")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let context = stdout(
        cmd(&db)
            .arg("context")
            .arg("agent context fallback orchard")
            .arg("--json")
            .arg("--embed-provider")
            .arg("mock")
            .arg("--embed-endpoint")
            .arg("local")
            .arg("--embed-model")
            .arg("mock-small"),
    );
    let context_json: Value = serde_json::from_str(&context).unwrap();
    assert!(
        context_json["memories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["title"] == "Semantic agent context")
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    assert_eq!(usage_json["recent_reads"][0]["command"], "context");
    assert_eq!(usage_json["recent_reads"][0]["semantic_used"], true);
}

#[test]
fn agent_context_json_is_compact_and_query_focused() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Agent context long body")
        .arg(format!(
            "{} agent context exact useful detail should be visible {}",
            "agent json prefix noise ".repeat(80),
            "agent json tail noise ".repeat(80)
        ))
        .arg("--link")
        .arg("file:src/agent/context.rs")
        .assert()
        .success();

    let context = stdout(
        cmd(&db)
            .arg("context")
            .arg("agent context exact")
            .arg("--mode")
            .arg("fast")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--json"),
    );
    let context_json: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context_json["compact"], true);
    assert_eq!(context_json["max_chars"], 1200);
    let memory = &context_json["memories"].as_array().unwrap()[0];
    assert!(memory.get("body").is_none());
    assert!(
        memory["summary"]
            .as_str()
            .unwrap()
            .contains("exact useful detail")
    );
    assert!(!context.contains(&"agent json prefix noise ".repeat(20)));
    assert_eq!(memory["links"][0]["target"], "src/agent/context.rs");
}

#[test]
fn agent_context_json_respects_tight_max_chars_and_usage() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..4 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Context json budget {index}"))
            .arg(format!(
                "context json budget exact useful detail variant{index} {}",
                "tail noise ".repeat(50)
            ))
            .assert()
            .success();
    }

    let context = stdout(
        cmd(&db)
            .arg("context")
            .arg("context json budget")
            .arg("--mode")
            .arg("fast")
            .arg("--max-chars")
            .arg("360")
            .arg("--limit")
            .arg("8")
            .arg("--json"),
    );
    assert!(
        context.len() <= 360,
        "agent context JSON exceeded budget: {}",
        context.len()
    );
    let context_json: Value = serde_json::from_str(&context).unwrap();
    let rendered_count = context_json["memories"].as_array().unwrap().len();

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    let read = &usage_json["recent_reads"][0];
    assert_eq!(read["command"], "context");
    assert_eq!(
        read["result_count"].as_u64().unwrap(),
        rendered_count as u64,
        "usage should log only rendered context memories"
    );
    assert_eq!(
        read["memory_ids"].as_array().unwrap().len(),
        rendered_count,
        "logged memory ids should match rendered context memories"
    );
}

#[test]
fn v14_agent_context_recent_fallback_requires_task_overlap() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("task_state")
        .arg("Billing export recent")
        .arg("billing export follow-up should not enter an unrelated checkout task")
        .assert()
        .success();

    let unrelated_context = stdout(
        cmd(&db)
            .arg("context")
            .arg("checkout validation")
            .arg("--mode")
            .arg("fast")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let unrelated_json: Value = serde_json::from_str(&unrelated_context).unwrap();
    assert!(
        unrelated_json["memories"].as_array().unwrap().is_empty(),
        "agent context should not bootstrap unrelated recent cards when direct matches are empty"
    );

    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Linked checkout command")
        .arg("Run the focused validation command before changing this area.")
        .arg("--link")
        .arg("file:src/checkout/validation.rs")
        .assert()
        .success();

    let linked_context = stdout(
        cmd(&db)
            .arg("context")
            .arg("checkout validation")
            .arg("--mode")
            .arg("fast")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let linked_json: Value = serde_json::from_str(&linked_context).unwrap();
    let titles = linked_json["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|memory| memory["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(titles, vec!["Linked checkout command"]);
}

#[test]
fn v14_retrieve_does_not_count_duplicate_fts_as_saturated() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..5 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Saturated lexical {index}"))
            .arg("saturated lexical match keeps tiny retrieval local and fast")
            .assert()
            .success();
    }
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("saturated lexical")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], true);
    assert_eq!(retrieved_json["semantic_skipped"], false);
    assert!(retrieved_json["semantic_skip_reason"].is_null());
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["semantic_score"].as_f64().is_some())
    );
}

#[test]
fn v14_retrieve_skips_semantic_when_tiny_fts_is_saturated() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for (title, body) in [
        (
            "Saturated lexical api",
            "saturated lexical api gateway routing auth headers circuit breaker",
        ),
        (
            "Saturated lexical cache",
            "saturated lexical cache eviction redis ttl invalidation warmup",
        ),
        (
            "Saturated lexical schema",
            "saturated lexical schema migration columns indexes constraints rollback",
        ),
        (
            "Saturated lexical deploy",
            "saturated lexical deploy release healthcheck rollback service monitor",
        ),
        (
            "Saturated lexical worker",
            "saturated lexical worker queue retry backoff jobs throughput",
        ),
    ] {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(title)
            .arg(body)
            .assert()
            .success();
    }
    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Recent unrelated fallback guard")
        .arg("fresh unrelated fallback should not be added when direct evidence is enough")
        .arg("--confidence")
        .arg("1.0")
        .assert()
        .success();
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("saturated lexical")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_used"], false);
    assert_eq!(retrieved_json["semantic_skipped"], true);
    assert_eq!(retrieved_json["semantic_skip_reason"], "lexical_saturated");
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["memory"]["title"] != "Recent unrelated fallback guard")
    );
    assert!(!retrieved_json["hits"].as_array().unwrap().is_empty());
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|hit| hit["semantic_score"].is_null())
    );
    assert!(
        retrieved_json["receipt"]
            .as_str()
            .unwrap()
            .contains("semantic search skipped")
    );
}

#[test]
fn v14_retrieve_does_not_count_uncertain_fts_as_saturated() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for (title, body) in [
        (
            "Uncertain lexical api",
            "uncertain lexical api gateway routing auth headers circuit breaker",
        ),
        (
            "Uncertain lexical cache",
            "uncertain lexical cache eviction redis ttl invalidation warmup",
        ),
        (
            "Uncertain lexical schema",
            "uncertain lexical schema migration columns indexes constraints rollback",
        ),
        (
            "Uncertain lexical deploy",
            "uncertain lexical deploy release healthcheck rollback service monitor",
        ),
        (
            "Uncertain lexical worker",
            "uncertain lexical worker queue retry backoff jobs throughput",
        ),
    ] {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(title)
            .arg(body)
            .arg("--status")
            .arg("uncertain")
            .assert()
            .success();
    }
    cmd(&db)
        .arg("embed-index")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success();

    let retrieved = stdout(
        cmd(&db)
            .arg("retrieve")
            .arg("uncertain lexical")
            .arg("--strategy")
            .arg("hybrid")
            .arg("--format")
            .arg("json")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let retrieved_json: Value = serde_json::from_str(&retrieved).unwrap();
    assert_eq!(retrieved_json["semantic_skipped"], false);
    assert!(retrieved_json["semantic_skip_reason"].is_null());
    assert!(
        retrieved_json["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hit| hit["semantic_score"].as_f64().is_some())
    );
}

#[test]
fn v14_5_brief_and_evidence_surfaces_are_budgeted_and_structured() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    let decision_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Auth rate limit doctrine")
            .arg("Auth rate limit must stay local, fast, and verified before release.")
            .arg("--source")
            .arg("test")
            .arg("--link")
            .arg("file:src/auth.rs"),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("add")
        .arg("known_issue")
        .arg("Auth rate limit risk")
        .arg("Auth rate limit bugs can slow development when stale memory is overused.")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Auth rate limit check")
        .arg("cargo test auth_rate_limit")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Auth focused long note")
        .arg(format!(
            "{} auth rate limit focused summary keeps the useful detail {}",
            "intro noise ".repeat(80),
            "tail noise ".repeat(80)
        ))
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Generic linked memory")
        .arg("memory agent project retrieval token quality context recall brief semantic")
        .arg("--link")
        .arg("file:src/noisy.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("command")
        .arg("Generic command")
        .arg("cargo test generic_memory_noise")
        .assert()
        .success();

    let generic_brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("memory agent project retrieval token quality context recall brief semantic")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(!generic_brief.contains("Files:"));
    assert!(!generic_brief.contains("Checks:"));
    let generic_empty_brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("agents memories projects contexts")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(
        generic_empty_brief.contains("Relevant: none (generic query; semantic search skipped)")
    );
    let weak_empty_brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("singleanchor")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(weak_empty_brief.contains("Relevant: none (weak query; semantic search skipped)"));

    let brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("auth rate limit")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(brief.len() <= 1200);
    assert!(brief.contains("Brief: auth rate limit"));
    assert!(brief.contains("Memory: read brief"));
    assert!(brief.contains("Must Follow:"));
    assert!(brief.contains("Risks:"));
    assert!(brief.contains("Files:"));
    assert!(brief.contains("Checks:"));
    assert!(brief.contains("auth rate limit focused summary keeps the useful detail"));
    assert!(!brief.contains(&"intro noise ".repeat(20)));

    let brief_json = stdout(cmd(&db).arg("brief").arg("auth rate limit").arg("--json"));
    let brief_value: Value = serde_json::from_str(&brief_json).unwrap();
    assert_eq!(brief_value["version"], 1);
    assert_eq!(brief_value["budget"], 1200);
    assert!(
        brief_value["receipt"]
            .as_str()
            .unwrap()
            .contains("Memory: read brief")
    );
    assert!(!brief_value["must_follow"].as_array().unwrap().is_empty());
    assert!(
        brief_value["semantic_error"]
            .as_str()
            .unwrap()
            .contains("semantic index not ready")
    );
    let relevant_summaries = brief_value["relevant"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["summary"].as_str())
        .collect::<Vec<_>>();
    assert!(
        relevant_summaries
            .iter()
            .any(|summary| summary.contains("auth rate limit focused summary"))
    );
    assert!(
        !relevant_summaries
            .iter()
            .any(|summary| summary.contains(&"intro noise ".repeat(20)))
    );

    cmd(&db)
        .arg("evidence")
        .arg(&decision_id)
        .assert()
        .success()
        .stdout(contains("Evidence:"))
        .stdout(contains("Memory: read evidence"))
        .stdout(contains("source: test"))
        .stdout(contains("link: file:src/auth.rs"))
        .stdout(contains("memory_added"));

    let evidence_json = stdout(cmd(&db).arg("evidence").arg(&decision_id).arg("--json"));
    let evidence_value: Value = serde_json::from_str(&evidence_json).unwrap();
    assert_eq!(evidence_value["memory"]["id"], decision_id);
    assert_eq!(evidence_value["source"], "test");
    assert!(
        evidence_value["receipt"]
            .as_str()
            .unwrap()
            .contains("Memory: read evidence")
    );
    assert!(
        !evidence_value["audit_events"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--since-days").arg("1"));
    assert!(usage.contains("Memory Usage Report"));
    assert!(usage.contains("brief:"));
    assert!(usage.contains("evidence:"));
    assert!(usage.contains("unique_memory_ids:"));
    assert!(usage.contains("semantic_results:"));
    assert!(usage.contains("semantic_eligible_reads:"));
    assert!(usage.contains("semantic_eligible_results:"));
    assert!(usage.contains("nonsemantic_reads:"));

    let usage_json = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_value: Value = serde_json::from_str(&usage_json).unwrap();
    assert!(usage_value["read_count"].as_u64().unwrap() >= 2);
    assert!(usage_value["unique_memory_ids"].as_u64().unwrap() >= 1);
    assert!(
        usage_value["semantic_reads_with_results"]
            .as_u64()
            .is_some()
    );
    assert!(usage_value["semantic_empty_read_count"].as_u64().is_some());
    assert!(usage_value["semantic_result_rate"].as_f64().is_some());
    assert!(usage_value["semantic_avg_results"].as_f64().is_some());
    assert!(usage_value["semantic_eligible_total"].as_u64().is_some());
    assert!(
        usage_value["semantic_eligible_read_rate"]
            .as_f64()
            .is_some()
    );
    assert!(
        usage_value["semantic_eligible_reads_with_results"]
            .as_u64()
            .is_some()
    );
    assert!(
        usage_value["semantic_eligible_empty_read_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        usage_value["semantic_eligible_result_rate"]
            .as_f64()
            .is_some()
    );
    assert!(usage_value["nonsemantic_read_count"].as_u64().is_some());

    let usefulness = stdout(cmd(&db).arg("usefulness-report").arg("--json"));
    let usefulness_value: Value = serde_json::from_str(&usefulness).unwrap();
    assert!(usefulness_value["total_active"].as_u64().unwrap() >= 3);
    assert!(
        !usefulness_value["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"auth rate limit","budget":1200}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"memory_evidence","arguments":{"id":decision_id}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_evidence","arguments":{"id":decision_id,"include_body":true}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_doctrine","arguments":{"max_chars":700}}})
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"memory_doctrine","arguments":{"include_body":true}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Auth rate limit doctrine"));
    assert!(stdout.contains("memory_added"));
    let compact_evidence = stdout
        .lines()
        .find(|line| line.contains("\"id\":2"))
        .unwrap();
    assert!(compact_evidence.contains("summary"));
    assert!(compact_evidence.contains("audit_event_count"));
    assert!(!compact_evidence.contains("body"));
    let full_evidence = stdout
        .lines()
        .find(|line| line.contains("\"id\":3"))
        .unwrap();
    assert!(full_evidence.contains("body"));
    let compact_doctrine = stdout
        .lines()
        .find(|line| line.contains("\"id\":4"))
        .unwrap();
    assert!(compact_doctrine.contains("summary"));
    assert!(!compact_doctrine.contains("body"));
    let full_doctrine = stdout
        .lines()
        .find(|line| line.contains("\"id\":5"))
        .unwrap();
    assert!(full_doctrine.contains("body"));

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let body = serde_json::json!({"task":"auth rate limit","budget":1200}).to_string();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /brief HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));
    assert!(response.contains("Auth rate limit doctrine"));
    assert!(child.wait().unwrap().success());

    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
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
    let body = serde_json::json!({"id":decision_id}).to_string();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "POST /evidence HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));
    assert!(response.contains("memory_added"));
    assert!(child.wait().unwrap().success());
}

#[test]
fn tiny_brief_keeps_artifact_hints_compact() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..6 {
        cmd(&db)
            .arg("add")
            .arg("command")
            .arg(format!("Artifact budget command {index}"))
            .arg(format!(
                "cargo test artifact_budget_{index} # artifact budget query focused check"
            ))
            .arg("--link")
            .arg(format!("file:src/artifact_budget_{index}.rs"))
            .assert()
            .success();
    }

    let brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("artifact budget query")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let brief_json: Value = serde_json::from_str(&brief).unwrap();
    assert!(brief_json["files"].as_array().unwrap().len() <= 4);
    assert!(brief_json["checks"].as_array().unwrap().len() <= 2);

    let normal = stdout(
        cmd(&db)
            .arg("brief")
            .arg("artifact budget query")
            .arg("--json")
            .arg("--budget-profile")
            .arg("normal"),
    );
    let normal_json: Value = serde_json::from_str(&normal).unwrap();
    assert!(
        normal_json["files"].as_array().unwrap().len()
            >= brief_json["files"].as_array().unwrap().len()
    );
    assert!(
        normal_json["checks"].as_array().unwrap().len()
            >= brief_json["checks"].as_array().unwrap().len()
    );
}

#[test]
fn tiny_brief_uses_shorter_query_focused_summaries() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Summary budget note")
        .arg(format!(
            "{} summary budget exact useful detail should remain visible while the surrounding noise is shortened {}",
            "prefix noise ".repeat(40),
            "tail noise ".repeat(40)
        ))
        .assert()
        .success();

    let tiny = stdout(
        cmd(&db)
            .arg("brief")
            .arg("summary budget exact")
            .arg("--json")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    let tiny_json: Value = serde_json::from_str(&tiny).unwrap();
    let tiny_summary = tiny_json["relevant"][0]["summary"].as_str().unwrap();
    assert!(tiny_summary.len() <= 120);
    assert!(tiny_summary.contains("summary budget exact"));

    let deep = stdout(
        cmd(&db)
            .arg("brief")
            .arg("summary budget exact")
            .arg("--json")
            .arg("--budget-profile")
            .arg("deep"),
    );
    let deep_json: Value = serde_json::from_str(&deep).unwrap();
    let deep_summary = deep_json["relevant"][0]["summary"].as_str().unwrap();
    assert!(deep_summary.len() >= tiny_summary.len());
}

#[test]
fn usage_report_counts_semantic_eligible_reads_only() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Initialize schema")
        .arg("Create db.")
        .assert()
        .success();

    insert_read_event_with_ids(&db, "brief", "checkout policy memory", &["abc111"]);
    insert_read_event(&db, "search", "checkout policy memory", true);
    insert_read_event(&db, "impact", "payment retry policy", false);
    insert_read_event(&db, "evidence", "abc123", false);
    insert_read_event(&db, "impact", "src/app.rs", false);

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    assert_eq!(usage_json["read_count"], 5);
    assert_eq!(usage_json["semantic_read_count"], 2);
    assert_eq!(usage_json["fallback_read_count"], 3);
    assert_eq!(usage_json["semantic_eligible_total"], 3);
    assert_eq!(usage_json["semantic_eligible_read_count"], 2);
    assert_eq!(usage_json["nonsemantic_read_count"], 2);
    assert_eq!(usage_json["semantic_reads_with_results"], 1);
    assert_eq!(usage_json["semantic_empty_read_count"], 1);
    assert_eq!(
        usage_json["semantic_empty_queries"][0],
        "checkout policy memory"
    );
    assert_eq!(usage_json["semantic_result_rate"].as_f64().unwrap(), 0.5);
    assert_eq!(usage_json["semantic_avg_results"].as_f64().unwrap(), 0.5);
    assert_eq!(usage_json["semantic_eligible_reads_with_results"], 1);
    assert_eq!(usage_json["semantic_eligible_empty_read_count"], 1);
    assert_eq!(
        usage_json["semantic_eligible_result_rate"]
            .as_f64()
            .unwrap(),
        0.5
    );
    assert_eq!(
        usage_json["semantic_eligible_read_rate"].as_f64().unwrap(),
        2.0 / 3.0
    );
}

#[test]
fn dashboard_reports_semantic_empty_result_attention() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Initialize schema")
        .arg("Create db.")
        .assert()
        .success();

    insert_read_event(&db, "brief", "checkout policy memory", true);
    insert_read_event(&db, "search", "checkout policy memory", true);
    insert_read_event(&db, "impact", "payment retry policy", true);

    let dashboard = stdout(cmd(&db).arg("dashboard").arg("--json"));
    let dashboard_json: Value = serde_json::from_str(&dashboard).unwrap();
    assert_eq!(dashboard_json["semantic_empty_projects"], 1);
    assert_eq!(dashboard_json["semantic_empty_read_count"], 3);
    assert_eq!(dashboard_json["semantic_result_warn_projects"], 1);
    assert_eq!(dashboard_json["semantic_empty_gap_projects"], 1);
    assert_eq!(dashboard_json["semantic_empty_gap_count"], 2);
    let project = &dashboard_json["projects"][0];
    assert_eq!(
        project["semantic_eligible_result_rate"].as_f64().unwrap(),
        0.0
    );
    assert_eq!(project["semantic_eligible_empty_read_count"], 3);
    assert_eq!(project["autonomous_semantic_empty_missing"], 2);
    assert!(
        project["semantic_empty_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|query| query == "checkout policy memory")
    );
    assert!(
        project["autonomous_semantic_empty_missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|query| query == "checkout policy memory")
    );
    assert!(
        project["attention_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "semantic_empty_results")
    );
    assert!(
        project["repair_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["code"] == "embed_index" && action["safe_auto"] == true)
    );
}

#[test]
fn dashboard_repair_refreshes_daemon_embedding_skip() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let agent_dir = dir.path().join(".agent");
    fs::create_dir_all(&agent_dir).unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Daemon embedding repair")
        .arg("Dashboard repair should refresh embeddings after daemon skipped maintenance.")
        .assert()
        .success();

    fs::write(
        agent_dir.join("daemon-status.json"),
        serde_json::json!({
            "tick_ok": true,
            "embedding_skipped": true,
            "embedding_error": "embedding provider was unavailable during daemon tick"
        })
        .to_string(),
    )
    .unwrap();

    let dashboard = stdout(cmd(&db).arg("dashboard").arg("--json"));
    let dashboard_json: Value = serde_json::from_str(&dashboard).unwrap();
    assert_eq!(dashboard_json["daemon_embedding_skipped_projects"], 1);
    let project = &dashboard_json["projects"][0];
    assert_eq!(project["daemon_embedding_skipped"], true);
    assert!(
        project["attention_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "daemon_embedding_skipped")
    );
    assert!(
        project["repair_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["code"] == "daemon_embed_index"
                && action["reason"] == "daemon_embedding_skipped"
                && action["safe_auto"] == true)
    );

    let repair = stdout(
        cmd(&db)
            .arg("dashboard-repair")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let repair_json: Value = serde_json::from_str(&repair).unwrap();
    assert!(
        repair_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .any(|action| action["code"] == "daemon_embed_index"
                && action["skipped"] == true
                && action["detail"] == "dry run")
    );

    let repair_apply = stdout(
        cmd(&db)
            .arg("dashboard-repair")
            .arg("--apply")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let repair_apply_json: Value = serde_json::from_str(&repair_apply).unwrap();
    assert_eq!(repair_apply_json["ok"], true);
    assert!(
        repair_apply_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .any(|action| action["code"] == "daemon_embed_index"
                && action["applied"] == true
                && action["detail"].as_str().unwrap().contains("indexed=")
                && action["detail"]
                    .as_str()
                    .unwrap()
                    .contains("daemon_status=cleared"))
    );
    let repaired_status: Value =
        serde_json::from_str(&fs::read_to_string(agent_dir.join("daemon-status.json")).unwrap())
            .unwrap();
    assert_eq!(repaired_status["embedding_skipped"], false);
    assert_eq!(repaired_status["embedding_error"], Value::Null);
    assert!(repaired_status["embedding_repaired_at"].as_i64().is_some());
    assert_eq!(
        repaired_status["embedding_repair_source"],
        "dashboard_repair"
    );

    let repaired_dashboard = stdout(cmd(&db).arg("dashboard").arg("--json"));
    let repaired_dashboard_json: Value = serde_json::from_str(&repaired_dashboard).unwrap();
    assert_eq!(
        repaired_dashboard_json["daemon_embedding_skipped_projects"],
        0
    );
    assert_eq!(
        repaired_dashboard_json["daemon_embedding_repaired_projects"],
        1
    );
    assert_eq!(
        repaired_dashboard_json["projects"][0]["daemon_embedding_skipped"],
        false
    );
    assert!(
        repaired_dashboard_json["projects"][0]["daemon_embedding_repaired_at"]
            .as_i64()
            .is_some()
    );
    assert_eq!(
        repaired_dashboard_json["projects"][0]["daemon_embedding_repair_source"],
        "dashboard_repair"
    );

    let history = stdout(cmd(&db).arg("dashboard-repair-history").arg("--json"));
    let history_json: Value = serde_json::from_str(&history).unwrap();
    assert!(
        history_json["actions_by_code"]["daemon_embed_index"]
            .as_u64()
            .unwrap()
            >= 1
    );
}

#[test]
fn brief_sections_are_budget_aware() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..6 {
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg(format!("Checkout budget decision {index}"))
            .arg("checkout budget signal must stay compact and relevant")
            .assert()
            .success();
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Checkout budget note {index}"))
            .arg("checkout budget signal implementation detail for focused recall")
            .assert()
            .success();
        cmd(&db)
            .arg("add")
            .arg("known_issue")
            .arg(format!("Checkout budget risk {index}"))
            .arg("checkout budget signal risk should be visible but bounded")
            .assert()
            .success();
    }

    let tiny = stdout(
        cmd(&db)
            .arg("brief")
            .arg("checkout budget signal")
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--limit")
            .arg("18")
            .arg("--json"),
    );
    let tiny_json: Value = serde_json::from_str(&tiny).unwrap();
    assert!(tiny_json["must_follow"].as_array().unwrap().len() <= 3);
    assert!(tiny_json["relevant"].as_array().unwrap().len() <= 3);
    assert!(tiny_json["risks"].as_array().unwrap().len() <= 2);

    let normal = stdout(
        cmd(&db)
            .arg("brief")
            .arg("checkout budget signal")
            .arg("--budget-profile")
            .arg("normal")
            .arg("--limit")
            .arg("18")
            .arg("--json"),
    );
    let normal_json: Value = serde_json::from_str(&normal).unwrap();
    assert!(normal_json["must_follow"].as_array().unwrap().len() >= 4);
    assert!(normal_json["relevant"].as_array().unwrap().len() >= 4);
    assert!(normal_json["risks"].as_array().unwrap().len() >= 3);
}

#[test]
fn brief_ultra_tight_budget_logs_no_unrendered_items() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..6 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Brief tight {index}"))
            .arg(format!(
                "brief tight exact useful detail variant{index} {}",
                "tail noise ".repeat(40)
            ))
            .assert()
            .success();
    }

    let brief = stdout(
        cmd(&db)
            .arg("brief")
            .arg("brief tight")
            .arg("--budget")
            .arg("240"),
    );
    assert!(brief.len() <= 240, "brief exceeded budget: {}", brief.len());
    assert_eq!(
        brief.lines().filter(|line| line.starts_with("- ")).count(),
        0
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    let read = &usage_json["recent_reads"][0];
    assert_eq!(read["command"], "brief");
    assert_eq!(read["result_count"], 0);
    assert_eq!(read["memory_ids"].as_array().unwrap().len(), 0);
    assert_eq!(read["budget"], 240);
}

#[test]
fn impact_sections_are_budget_aware() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let target = "src/checkout.rs";

    for index in 0..6 {
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg(format!("Checkout impact decision {index}"))
            .arg("checkout impact target must stay compact and relevant")
            .arg("--link")
            .arg(format!("file:{target}"))
            .assert()
            .success();
        cmd(&db)
            .arg("add")
            .arg("constraint")
            .arg(format!("Checkout impact constraint {index}"))
            .arg("checkout impact target must avoid slow unrelated work")
            .arg("--link")
            .arg(format!("file:{target}"))
            .assert()
            .success();
        cmd(&db)
            .arg("add")
            .arg("known_issue")
            .arg(format!("Checkout impact risk {index}"))
            .arg("checkout impact target risk should be visible but bounded")
            .arg("--link")
            .arg(format!("file:{target}"))
            .assert()
            .success();
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Checkout impact note {index}"))
            .arg("checkout impact target implementation detail for focused edits")
            .arg("--link")
            .arg(format!("file:{target}"))
            .assert()
            .success();
    }

    let tiny = stdout(
        cmd(&db)
            .arg("impact")
            .arg(target)
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--limit")
            .arg("30")
            .arg("--json"),
    );
    let tiny_json: Value = serde_json::from_str(&tiny).unwrap();
    assert!(tiny_json["decisions"].as_array().unwrap().len() <= 2);
    assert!(tiny_json["constraints"].as_array().unwrap().len() <= 2);
    assert!(tiny_json["risks"].as_array().unwrap().len() <= 2);
    assert!(tiny_json["related"].as_array().unwrap().len() <= 2);
    assert!(tiny_json["links"].as_array().unwrap().len() <= 5);

    let normal = stdout(
        cmd(&db)
            .arg("impact")
            .arg(target)
            .arg("--budget-profile")
            .arg("normal")
            .arg("--limit")
            .arg("30")
            .arg("--json"),
    );
    let normal_json: Value = serde_json::from_str(&normal).unwrap();
    assert!(normal_json["decisions"].as_array().unwrap().len() >= 4);
    assert!(normal_json["constraints"].as_array().unwrap().len() >= 4);
    assert!(normal_json["risks"].as_array().unwrap().len() >= 4);
    assert!(normal_json["related"].as_array().unwrap().len() >= 4);
}

#[test]
fn impact_ultra_tight_budget_logs_no_unrendered_items() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    for index in 0..6 {
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg(format!("Impact tight {index}"))
            .arg(format!(
                "impact tight exact useful detail variant{index} {}",
                "tail noise ".repeat(40)
            ))
            .assert()
            .success();
    }

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg("impact tight")
            .arg("--budget")
            .arg("240"),
    );
    assert!(
        impact.len() <= 240,
        "impact exceeded budget: {}",
        impact.len()
    );
    assert_eq!(
        impact.lines().filter(|line| line.starts_with("- ")).count(),
        0
    );

    let usage = stdout(cmd(&db).arg("usage-report").arg("--json"));
    let usage_json: Value = serde_json::from_str(&usage).unwrap();
    let read = &usage_json["recent_reads"][0];
    assert_eq!(read["command"], "impact");
    assert_eq!(read["result_count"], 0);
    assert_eq!(read["memory_ids"].as_array().unwrap().len(), 0);
    assert_eq!(read["budget"], 240);
}

#[test]
fn impact_filters_noisy_top_candidates() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let target = "checkout impact token budget";

    cmd(&db)
        .arg("add")
        .arg("design_note")
        .arg("Useful impact memory")
        .arg("checkout impact token budget useful card should remain in impact")
        .assert()
        .success();
    let mut noisy_ids = Vec::new();
    for index in 0..10 {
        let id = stdout(
            cmd(&db)
                .arg("add")
                .arg("design_note")
                .arg(format!("Noisy impact memory {index}"))
                .arg("checkout impact token budget noisy card should be suppressed from impact"),
        )
        .trim()
        .to_string();
        noisy_ids.push(id);
    }
    for id in noisy_ids {
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg(id)
            .arg("--rating")
            .arg("useless")
            .arg("--command")
            .arg("impact")
            .arg("--query")
            .arg(target)
            .assert()
            .success();
    }

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg(target)
            .arg("--budget-profile")
            .arg("tiny")
            .arg("--limit")
            .arg("30")
            .arg("--json"),
    );
    let impact_json: Value = serde_json::from_str(&impact).unwrap();
    let related_titles = impact_json["related"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(related_titles.contains(&"Useful impact memory"));
    assert!(
        related_titles
            .iter()
            .all(|title| !title.starts_with("Noisy impact memory"))
    );
}

#[test]
fn v14_5_impact_and_drift_are_lightweight_and_structured() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let root = dir.path().join("repo");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/auth.rs"), "pub fn auth() {}\n").unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Auth local rate limit")
        .arg("Auth rate limiting must stay local and fast.")
        .arg("--link")
        .arg("file:src/auth.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("constraint")
        .arg("Auth rate budget")
        .arg("Do not add slow network calls to the auth rate limit path.")
        .arg("--link")
        .arg("symbol:auth::rate_limit")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("known_issue")
        .arg("Auth missing file risk")
        .arg("cargo test auth_rate_limit")
        .arg("--link")
        .arg("file:src/missing.rs")
        .assert()
        .success();
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Auth local rate limit")
        .arg("Duplicate active decision for drift conflict detection.")
        .assert()
        .success();

    let impact = stdout(
        cmd(&db)
            .arg("impact")
            .arg("src/auth.rs")
            .arg("--budget-profile")
            .arg("tiny"),
    );
    assert!(impact.len() <= 1200);
    assert!(impact.contains("Impact: src/auth.rs"));
    assert!(impact.contains("Memory: read impact"));
    assert!(impact.contains("Decisions:"));
    assert!(impact.contains("file:src/auth.rs"));

    let impact_json = stdout(cmd(&db).arg("impact").arg("auth::rate_limit").arg("--json"));
    let impact_value: Value = serde_json::from_str(&impact_json).unwrap();
    assert_eq!(impact_value["version"], 1);
    assert_eq!(impact_value["target"], "auth::rate_limit");
    assert!(
        impact_value["receipt"]
            .as_str()
            .unwrap()
            .contains("Memory: read impact")
    );
    assert!(!impact_value["constraints"].as_array().unwrap().is_empty());
    let impact_reasons = impact_value["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|item| item["reasons"].as_array().unwrap().iter())
        .collect::<Vec<_>>();
    assert!(
        impact_reasons
            .iter()
            .any(|reason| reason.as_str().unwrap() == "linked_target")
    );

    let drift = stdout(cmd(&db).arg("drift").arg("--root").arg(&root));
    assert!(drift.contains("Drift: needs_attention"));
    assert!(drift.contains("Missing Links:"));
    assert!(drift.contains("Potential Conflicts:"));

    let drift_json = stdout(cmd(&db).arg("drift").arg("--root").arg(&root).arg("--json"));
    let drift_value: Value = serde_json::from_str(&drift_json).unwrap();
    assert_eq!(drift_value["version"], 1);
    assert_eq!(drift_value["ok"], false);
    assert!(!drift_value["missing_links"].as_array().unwrap().is_empty());
    assert!(!drift_value["conflicts"].as_array().unwrap().is_empty());

    let changed_only_json = stdout(
        cmd(&db)
            .arg("drift")
            .arg("--root")
            .arg(&root)
            .arg("--changed-only")
            .arg("--json"),
    );
    let changed_only_value: Value = serde_json::from_str(&changed_only_json).unwrap();
    assert_eq!(changed_only_value["changed_only"], true);
    assert!(
        changed_only_value["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("git metadata not found"))
    );
}

#[test]
fn v14_9_codex_doctor_checks_mcp_config() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let agent_config = dir.path().join("agent-config.toml");
    let config = dir.path().join("config.toml");
    let binary = assert_cmd::cargo::cargo_bin("dukememory");
    fs::write(
        &agent_config,
        format!(
            "db_path = \"{}\"\ndefault_context_limit = 12\ndefault_context_max_chars = 4000\ndefault_statuses = [\"active\", \"uncertain\"]\n\n[embeddings]\nprovider = \"mock\"\nendpoint = \"local\"\nmodel = \"mock-small\"\n\n[codegraph]\nenabled = false\ncommand = \"codegraph\"\n",
            db.display()
        ),
    )
    .unwrap();
    fs::write(
        &config,
        format!(
            "[mcp_servers.dukememory]\ncommand = \"{}\"\nargs = [\"--db\", \"{}\", \"--config\", \"{}\", \"serve-mcp\"]\n",
            binary.display(),
            db.display(),
            agent_config.display()
        ),
    )
    .unwrap();

    let report = stdout(
        cmd(&db)
            .arg("codex-doctor")
            .arg("--config")
            .arg(&config)
            .arg("--json"),
    );
    let value: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(value["ok"], true);
    assert!(
        value["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "mcp_probe" && item["status"] == "ok")
    );
}

#[test]
fn v14_14_onboard_codex_mcp_and_autonomous_e2e() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("project");
    let db = root.join(".agent/memory.db");
    let agent_config = root.join(".agent/config.toml");
    let codex_config = dir.path().join("codex-config.toml");
    let skills = dir.path().join("skills");
    let binary = assert_cmd::cargo::cargo_bin("dukememory");

    cmd(&db)
        .arg("onboard")
        .arg("--root")
        .arg(&root)
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains("onboard:"));
    assert!(agent_config.exists());
    assert!(root.join("AGENTS.md").exists());

    cmd(&db)
        .arg("install-skill")
        .arg("--path")
        .arg(&skills)
        .assert()
        .success();
    assert!(skills.join("dukememory-use/SKILL.md").exists());

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("E2E memory route")
        .arg("Codex MCP should retrieve the onboarded project memory by root.")
        .assert()
        .success();

    fs::write(
        &codex_config,
        format!(
            "[mcp_servers.dukememory]\ncommand = \"{}\"\nargs = [\"--db\", \"{}\", \"--config\", \"{}\", \"serve-mcp\"]\n",
            binary.display(),
            db.display(),
            agent_config.display()
        ),
    )
    .unwrap();
    let doctor = stdout(
        cmd(&db)
            .arg("codex-doctor")
            .arg("--config")
            .arg(&codex_config)
            .arg("--json"),
    );
    let doctor_json: Value = serde_json::from_str(&doctor).unwrap();
    assert_eq!(doctor_json["ok"], true);

    let mut child = StdCommand::new(&binary)
        .arg("--db")
        .arg(dir.path().join("other/.agent/memory.db"))
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_brief","arguments":{"task":"route onboarded memory","root":root}}})
        )
        .unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("E2E memory route")
    );

    for idx in 0..3 {
        cmd(&db)
            .arg("add")
            .arg("note")
            .arg(format!("E2E operational note {idx}"))
            .arg(format!("Temporary e2e operational context {idx}."))
            .assert()
            .success();
    }

    let status_file = root.join(".agent/autonomous-status.json");
    let rollback_dir = root.join(".agent/autonomous-rollbacks");
    let backup_dir = root.join(".agent/backups");
    fs::create_dir_all(&rollback_dir).unwrap();
    for idx in 0..8 {
        fs::write(
            rollback_dir.join(format!("autonomous-old-{idx}.db")),
            format!("old autonomous rollback {idx}"),
        )
        .unwrap();
    }
    let run = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--rollback-keep")
            .arg("3")
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let run_json: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run_json["ok"], true);
    assert!(
        run_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "embed_index")
    );
    assert!(
        run_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "optimize_storage" && item["status"] == "ok")
    );
    assert!(
        run_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_retention" && item["status"] == "ok")
    );
    let rollback_backup_count = fs::read_dir(&rollback_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with("autonomous-") && name.ends_with(".db")
        })
        .count();
    assert!(rollback_backup_count <= 3);

    let rollback = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("rollback")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--json"),
    );
    let rollback_json: Value = serde_json::from_str(&rollback).unwrap();
    assert_eq!(rollback_json["level"], "rollback");
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_restore_status")
    );
}

#[test]
fn v14_1_daemon_autopilot_writes_status_backup_cleanup_and_ingests() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("session.md"),
        "We decided daemon autopilot should run without manual memory commands.\nTODO inspect daemon status.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Autopilot seed")
        .arg("Daemon autopilot should keep backup and embedding maintenance current.")
        .assert()
        .success();

    cmd(&db)
        .arg("daemon")
        .arg("--once")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--backup-every-secs")
        .arg("0")
        .assert()
        .success()
        .stdout(contains("daemon_tick"))
        .stdout(contains("backup_ran=true"))
        .stdout(contains("cleanup_ran=true"));

    let status_json: Value = serde_json::from_str(&fs::read_to_string(&status).unwrap()).unwrap();
    assert_eq!(status_json["autopilot"], true);
    assert_eq!(status_json["tick_ok"], true);
    assert_eq!(status_json["backup_ran"], true);
    assert_eq!(status_json["cleanup_ran"], true);
    assert_eq!(status_json["auto_inbox_added"], 2);
    assert_eq!(status_json["error"], Value::Null);

    let backup = fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("db"))
        .unwrap();
    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);

    let embed_status = stdout(
        cmd(&db)
            .arg("embed-status")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let embed_json: Value = serde_json::from_str(&embed_status).unwrap();
    assert!(embed_json["indexed"].as_u64().unwrap() >= 1);
    assert_eq!(embed_json["provider_reachable"], true);
    assert!(embed_json["provider_health_ms"].as_u64().is_some());
    assert_eq!(embed_json["provider_error"], Value::Null);
}

#[test]
fn daemon_tick_skips_embed_index_when_provider_is_unreachable() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let server = std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(3));
            drop(stream);
        }
    });
    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Daemon down provider")
        .arg("Daemon autopilot should keep backup and cleanup running when embeddings are down.")
        .assert()
        .success();

    let started = std::time::Instant::now();
    cmd(&db)
        .arg("daemon")
        .arg("--once")
        .arg("--provider")
        .arg("ollama")
        .arg("--endpoint")
        .arg(&endpoint)
        .arg("--model")
        .arg("bge-m3:latest")
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--backup-every-secs")
        .arg("0")
        .assert()
        .success()
        .stdout(contains("daemon_tick"))
        .stdout(contains("backup_ran=true"))
        .stdout(contains("cleanup_ran=true"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(6),
        "daemon tick should skip embeddings before the long embedding timeout"
    );

    let status_json: Value = serde_json::from_str(&fs::read_to_string(&status).unwrap()).unwrap();
    assert_eq!(status_json["tick_ok"], true);
    assert_eq!(status_json["embedding_skipped"], true);
    assert!(
        status_json["embedding_error"]
            .as_str()
            .unwrap()
            .contains("embedding provider is not reachable")
    );
    assert_eq!(status_json["error"], Value::Null);
    assert_eq!(status_json["backup_ran"], true);
    assert_eq!(status_json["cleanup_ran"], true);

    let history = stdout(cmd(&db).arg("autopilot").arg("history").arg("--json"));
    let history_json: Value = serde_json::from_str(&history).unwrap();
    assert_eq!(history_json[0]["event_type"], "daemon_tick");
    assert_eq!(history_json[0]["detail"]["embedding_skipped"], true);

    server.join().unwrap();

    let report = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("report")
            .arg("--status-file")
            .arg(&status)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg(&endpoint)
            .arg("--model")
            .arg("bge-m3:latest")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["failed_ticks"], 0);
    assert_eq!(report_json["embedding_skipped_ticks"], 1);
    assert!(
        report_json["latest_embedding_error"]
            .as_str()
            .unwrap()
            .contains("embedding provider is not reachable")
    );

    let alert_output = cmd(&db)
        .arg("autopilot")
        .arg("alert")
        .arg("--status-file")
        .arg(&status)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--provider")
        .arg("ollama")
        .arg("--endpoint")
        .arg(&endpoint)
        .arg("--model")
        .arg("bge-m3:latest")
        .arg("--json")
        .assert()
        .failure()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let alert_json: Value = serde_json::from_slice(&alert_output).unwrap();
    assert_eq!(alert_json["level"], "warn");
    assert!(
        alert_json["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "embedding_provider_skipped:1")
    );
}

#[test]
fn v14_2_autopilot_control_plane_status_doctor_run_once_and_install() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");
    let plist = dir.path().join("com.dukememory.daemon.plist");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("session.md"),
        "We decided autopilot control plane should manage unattended memory.\nTODO check autopilot doctor.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Autopilot control")
        .arg("Control plane should run and verify autopilot without manual maintenance commands.")
        .assert()
        .success();

    cmd(&db)
        .arg("autopilot")
        .arg("run-once")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--json")
        .assert()
        .success();
    let run_json: Value = serde_json::from_str(&fs::read_to_string(&status).unwrap()).unwrap();
    assert_eq!(run_json["tick_ok"], true);
    assert_eq!(run_json["autopilot"], true);
    assert_eq!(run_json["backup_ran"], true);

    let status_out = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("status")
            .arg("--status-file")
            .arg(&status)
            .arg("--json"),
    );
    let status_json: Value = serde_json::from_str(&status_out).unwrap();
    assert_eq!(status_json["tick_ok"], true);
    assert_eq!(status_json["auto_inbox_added"], 2);

    let doctor = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("doctor")
            .arg("--status-file")
            .arg(&status)
            .arg("--session-dir")
            .arg(&sessions)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--json"),
    );
    let doctor_json: Value = serde_json::from_str(&doctor).unwrap();
    assert_eq!(doctor_json["ok"], true);
    assert_eq!(doctor_json["status_fresh"], true);
    assert_eq!(doctor_json["backup_ok"], true);
    assert_eq!(doctor_json["endpoint_ok"], true);

    cmd(&db)
        .arg("autopilot")
        .arg("install")
        .arg("--output")
        .arg(&plist)
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("--backup-dir"))
        .stdout(contains("--status-file"));
}

#[test]
fn v14_3_autopilot_repair_self_heals_safe_prerequisites() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Repair autopilot")
        .arg("Autopilot repair should safely create prerequisites and run one maintenance tick.")
        .assert()
        .success();

    let repair = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("repair")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--session-dir")
            .arg(&sessions)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--status-file")
            .arg(&status)
            .arg("--json"),
    );
    let repair_json: Value = serde_json::from_str(&repair).unwrap();
    assert_eq!(repair_json["ok"], true);
    assert_eq!(repair_json["before"]["ok"], false);
    assert_eq!(repair_json["after"]["ok"], true);
    assert!(
        repair_json["actions_taken"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action.as_str().unwrap().starts_with("created_session_dir:"))
    );
    assert!(
        repair_json["actions_taken"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action.as_str().unwrap().starts_with("created_backup_dir:"))
    );
    assert!(
        repair_json["actions_taken"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action == "ran_autopilot_tick")
    );

    assert!(sessions.is_dir());
    assert!(backups.is_dir());
    assert!(status.exists());
    let backup = fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("db"))
        .unwrap();
    let verify = stdout(
        cmd(&db)
            .arg("backup-verify")
            .arg(&backup)
            .arg("--strict")
            .arg("--json"),
    );
    let verify_json: Value = serde_json::from_str(&verify).unwrap();
    assert_eq!(verify_json["verified"], true);

    let doctor_repair = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("doctor")
            .arg("--repair")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--status-file")
            .arg(&status)
            .arg("--session-dir")
            .arg(&sessions)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--json"),
    );
    let doctor_repair_json: Value = serde_json::from_str(&doctor_repair).unwrap();
    assert_eq!(doctor_repair_json["ok"], true);
    assert_eq!(doctor_repair_json["after"]["ok"], true);
}

#[test]
fn v14_4_autopilot_observability_history_report_and_export_status() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");
    let export = dir.path().join("autopilot-export.json");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("session.md"),
        "We decided autopilot observability should show history and reports.\nTODO inspect export status.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Autopilot observability")
        .arg("Autopilot should expose history, report, and export-status for monitoring.")
        .assert()
        .success();

    cmd(&db)
        .arg("autopilot")
        .arg("run-once")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--json")
        .assert()
        .success();

    let history = stdout(cmd(&db).arg("autopilot").arg("history").arg("--json"));
    let history_json: Value = serde_json::from_str(&history).unwrap();
    assert_eq!(history_json[0]["event_type"], "daemon_tick");
    assert_eq!(history_json[0]["detail"]["version"], 1);
    assert_eq!(history_json[0]["detail"]["tick_ok"], true);
    assert_eq!(history_json[0]["detail"]["backup_ran"], true);

    let report = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("report")
            .arg("--status-file")
            .arg(&status)
            .arg("--session-dir")
            .arg(&sessions)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let report_json: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report_json["ok"], true);
    assert_eq!(report_json["total_ticks"], 1);
    assert_eq!(report_json["failed_ticks"], 0);
    assert_eq!(report_json["backups_created"], 1);
    assert_eq!(report_json["current_pending"], 2);
    assert!(report_json["embeddings_indexed"].as_u64().unwrap() >= 1);
    assert_eq!(report_json["doctor"]["backup_ok"], true);

    cmd(&db)
        .arg("autopilot")
        .arg("export-status")
        .arg(&export)
        .arg("--status-file")
        .arg(&status)
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .assert()
        .success()
        .stdout(contains(export.display().to_string()));
    let export_json: Value = serde_json::from_str(&fs::read_to_string(&export).unwrap()).unwrap();
    assert_eq!(export_json["ok"], true);
    assert_eq!(export_json["history"][0]["event_type"], "daemon_tick");
}

#[test]
fn v14_5_autopilot_alert_thresholds_and_export() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let sessions = dir.path().join("sessions");
    let backups = dir.path().join("backups");
    let status = dir.path().join("daemon-status.json");
    let alert_file = dir.path().join("autopilot-alert.json");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join("session.md"),
        "We decided autopilot alerts should expose machine readable threshold status.\nTODO watch pending inbox.\n",
    )
    .unwrap();

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Autopilot alerts")
        .arg("Autopilot should return an alert level and violations for monitoring.")
        .assert()
        .success();

    cmd(&db)
        .arg("autopilot")
        .arg("run-once")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--status-file")
        .arg(&status)
        .arg("--json")
        .assert()
        .success();

    let ok = stdout(
        cmd(&db)
            .arg("autopilot")
            .arg("alert")
            .arg("--status-file")
            .arg(&status)
            .arg("--session-dir")
            .arg(&sessions)
            .arg("--backup-dir")
            .arg(&backups)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--max-pending")
            .arg("10")
            .arg("--require-backup")
            .arg("--require-endpoint")
            .arg("--json"),
    );
    let ok_json: Value = serde_json::from_str(&ok).unwrap();
    assert_eq!(ok_json["ok"], true);
    assert_eq!(ok_json["level"], "ok");
    assert_eq!(ok_json["violations"].as_array().unwrap().len(), 0);

    let output = cmd(&db)
        .arg("autopilot")
        .arg("alert")
        .arg("--status-file")
        .arg(&status)
        .arg("--session-dir")
        .arg(&sessions)
        .arg("--backup-dir")
        .arg(&backups)
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("mock-small")
        .arg("--max-pending")
        .arg("0")
        .arg("--write-alert")
        .arg(&alert_file)
        .arg("--json")
        .assert()
        .failure()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let warn_json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(warn_json["ok"], false);
    assert_eq!(warn_json["level"], "warn");
    assert!(
        warn_json["violations"][0]
            .as_str()
            .unwrap()
            .starts_with("pending_inbox_exceeds_threshold:")
    );

    let exported_json: Value =
        serde_json::from_str(&fs::read_to_string(&alert_file).unwrap()).unwrap();
    assert_eq!(exported_json["level"], "warn");
    assert_eq!(exported_json["report"]["doctor"]["backup_ok"], true);
}

#[test]
fn v14_6_local_memory_ui_and_http_actions() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let transcript = dir.path().join("transcript.md");
    fs::write(
        &transcript,
        "We decided the memory UI should be local and browser based.\nTODO approve UI inbox items.\n",
    )
    .unwrap();

    let memory_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Memory UI")
            .arg("The memory UI should show search, evidence, inbox, and status actions."),
    )
    .trim()
    .to_string();
    cmd(&db)
        .arg("ingest-transcript")
        .arg(&transcript)
        .assert()
        .success();
    cmd(&db)
        .arg("brief")
        .arg("memory ui")
        .assert()
        .success()
        .stdout(contains("Memory: read brief"));

    let html = http_once(
        &db,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(html.contains("200 OK"));
    assert!(html.contains("Content-Type: text/html"));
    assert!(html.contains("dukememory."));
    assert!(html.contains("<html lang=\"ru\">"));
    assert!(html.contains("Поиск памяти"));
    assert!(html.contains("Добавить память"));
    assert!(html.contains("id=\"lang\""));
    assert!(html.contains("data-tab=\"edit\""));
    assert!(html.contains("data-tab=\"autopilot\""));
    assert!(html.contains("data-tab=\"settings\""));
    assert!(html.contains("data-quick-type=\"decision\""));
    assert!(html.contains("id=\"activityPanel\""));
    assert!(html.contains("inline-edit"));
    assert!(html.contains("id=\"autopilotRun\""));
    assert!(html.contains("id=\"autonomousRun\""));
    assert!(html.contains("id=\"autonomousRollback\""));
    assert!(html.contains("id=\"dashboardRepair\""));
    assert!(html.contains("/memory?"));
    assert!(html.contains("запросы"));
    assert!(html.contains("id=\"usage\""));
    assert!(html.contains("id=\"sort\""));
    assert!(html.contains("id=\"reindexEmbeddings\""));
    assert!(html.contains("Project profile"));
    assert!(html.contains("policy decisions"));
    assert!(html.contains("live usefulness"));
    assert!(html.contains("live reads"));
    assert!(html.contains("live gaps"));
    assert!(html.contains("auto age"));
    assert!(html.contains("recommendations"));
    assert!(html.contains("missing live eval"));
    assert!(html.contains("gap projects"));
    assert!(html.contains("memory gaps"));
    assert!(html.contains("semantic gap projects"));
    assert!(html.contains("semantic gaps"));
    assert!(html.contains("semantic gap queries"));
    assert!(html.contains("semantic empty projects"));
    assert!(html.contains("semantic empty reads"));
    assert!(html.contains("semantic result warnings"));
    assert!(html.contains("semantic empty queries"));
    assert!(html.contains("embedding provider"));
    assert!(html.contains("daemon embeddings"));
    assert!(html.contains("gap inbox projects"));
    assert!(html.contains("gap inbox pending"));
    assert!(html.contains("gap inbox stale"));
    assert!(html.contains("gap inbox oldest"));
    assert!(html.contains("attention"));
    assert!(html.contains("attention reasons"));
    assert!(html.contains("repair actions"));
    assert!(html.contains("safe repairs"));
    assert!(html.contains("daemon skipped"));
    assert!(html.contains("daemon repaired"));
    assert!(html.contains("Repair history"));
    assert!(html.contains("repair loop"));
    assert!(html.contains("repair failed"));
    assert!(html.contains("safe skipped"));
    assert!(html.contains("repair action types"));
    assert!(html.contains("manual repair action types"));
    assert!(html.contains("actions by code"));
    assert!(html.contains("manual actions by code"));
    assert!(html.contains("<span>status</span>"));
    assert!(html.contains("Memory QA"));
    assert!(html.contains("semantic results"));
    assert!(html.contains("semantic empty"));
    assert!(html.contains("avg semantic results"));
    assert!(html.contains("Storage"));
    assert!(html.contains("/ops-status"));
    assert!(html.contains("/upgrade-project"));

    let memory = http_once(
        &db,
        "GET /memory?status=active&type=decision&q=ui HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(memory.contains("200 OK"));
    assert!(memory.contains("\"memories\""));
    assert!(memory.contains("Memory UI"));
    assert!(memory.contains("\"request_count\""));

    let hot_memory = http_once(
        &db,
        "GET /memory?status=active&type=decision&q=ui&usage=hot&sort=request_count HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(hot_memory.contains("200 OK"));
    assert!(hot_memory.contains("\"request_count\""));

    let usefulness = http_once(
        &db,
        "GET /usefulness?since_days=30&stale_days=30&hot_threshold=1 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(usefulness.contains("\"usefulness\""));
    assert!(usefulness.contains("\"hot\""));

    let quality = http_once(
        &db,
        "GET /quality?since_days=30&limit=10 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(quality.contains("\"quality\""));
    assert!(quality.contains("\"average_score\""));

    let budget = http_once(
        &db,
        "GET /budget-plan?task=small%20memory%20task HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(budget.contains("\"budget\""));
    assert!(budget.contains("\"profile\""));

    let profile = http_once(
        &db,
        "GET /project-profile HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(profile.contains("\"profile\""));
    assert!(profile.contains("\"memory_count\""));

    let dashboard = http_once(
        &db,
        "GET /dashboard HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(dashboard.contains("\"dashboard\""));
    assert!(dashboard.contains("\"projects\""));
    assert!(dashboard.contains("\"autonomous_live_reads\""));
    assert!(dashboard.contains("\"autonomous_inferred_missing\""));
    assert!(dashboard.contains("\"autonomous_age_secs\""));
    assert!(dashboard.contains("\"autonomous_fresh\""));
    assert!(dashboard.contains("\"recommendations\""));
    assert!(dashboard.contains("\"total_projects\""));
    assert!(dashboard.contains("\"status\""));
    assert!(dashboard.contains("\"attention\""));
    assert!(dashboard.contains("\"attention_reasons\""));
    assert!(dashboard.contains("\"attention_reason_counts\""));
    assert!(dashboard.contains("\"repair_actions\""));
    assert!(dashboard.contains("\"repair_actions_count\""));
    assert!(dashboard.contains("\"safe_repair_actions_count\""));
    assert!(dashboard.contains("\"repair_loop\""));
    assert!(dashboard.contains("\"repair_loop_projects\""));
    assert!(dashboard.contains("\"repair_loop_failed_projects\""));
    assert!(dashboard.contains("\"repair_loop_safe_skipped_projects\""));
    assert!(dashboard.contains("\"daemon_embedding_skipped_projects\""));
    assert!(dashboard.contains("\"daemon_embedding_skipped\""));
    assert!(dashboard.contains("\"daemon_embedding_error\""));
    assert!(dashboard.contains("\"daemon_embedding_repaired_projects\""));
    assert!(dashboard.contains("\"daemon_embedding_repaired_at\""));
    assert!(dashboard.contains("\"daemon_embedding_repair_source\""));
    assert!(dashboard.contains("\"memory_gap_projects\""));
    assert!(dashboard.contains("\"memory_gap_count\""));
    assert!(dashboard.contains("\"semantic_empty_gap_projects\""));
    assert!(dashboard.contains("\"semantic_empty_gap_count\""));
    assert!(dashboard.contains("\"autonomous_semantic_empty_missing\""));
    assert!(dashboard.contains("\"autonomous_semantic_empty_missing_queries\""));
    assert!(dashboard.contains("\"semantic_empty_projects\""));
    assert!(dashboard.contains("\"semantic_empty_read_count\""));
    assert!(dashboard.contains("\"semantic_result_warn_projects\""));
    assert!(dashboard.contains("\"semantic_eligible_result_rate\""));
    assert!(dashboard.contains("\"semantic_eligible_empty_read_count\""));
    assert!(dashboard.contains("\"semantic_empty_queries\""));
    assert!(dashboard.contains("\"gap_inbox\""));
    assert!(dashboard.contains("\"gap_inbox_pending_projects\""));
    assert!(dashboard.contains("\"gap_inbox_pending_count\""));
    assert!(dashboard.contains("\"gap_inbox_stale_projects\""));
    assert!(dashboard.contains("\"gap_inbox_stale_count\""));
    assert!(dashboard.contains("\"gap_inbox_oldest_pending_age_secs\""));
    assert!(dashboard.contains("\"stale_pending\""));
    assert!(dashboard.contains("\"oldest_pending_age_secs\""));
    assert!(dashboard.contains("\"attention_projects\""));
    assert!(dashboard.contains("\"missing_live_eval_projects\""));

    let dashboard_repair = http_once(
        &db,
        "GET /dashboard-repair HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(dashboard_repair.contains("\"repair\""));
    assert!(dashboard_repair.contains("\"apply\":false"));
    assert!(dashboard_repair.contains("\"skipped_actions\""));

    insert_empty_read_event(&db, "brief", "missing ui deployment memory");

    let eval_live = http_once(
        &db,
        "GET /eval-live?since_days=7 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(eval_live.contains("\"eval\""));
    assert!(eval_live.contains("\"useful_rate\""));
    assert!(eval_live.contains("\"useful_rate_source\":\"inferred\""));
    assert!(eval_live.contains("\"inferred_useful_rate\""));
    assert!(eval_live.contains("\"inferred_missing\":1"));
    assert!(eval_live.contains("missing ui deployment memory"));

    let dashboard_repair_body =
        r#"{"apply":true,"provider":"mock","endpoint":"local","model":"mock-small"}"#;
    let dashboard_repair_request = format!(
        "POST /dashboard-repair HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        dashboard_repair_body.len(),
        dashboard_repair_body
    );
    let dashboard_repair_apply = http_once(&db, &dashboard_repair_request);
    assert!(dashboard_repair_apply.contains("\"repair\""));
    assert!(dashboard_repair_apply.contains("\"apply\":true"));
    assert!(dashboard_repair_apply.contains("\"ok\":true"));
    let audit = http_once(
        &db,
        "GET /audit HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(audit.contains("dashboard_repair"));
    let dashboard_repair_history = http_once(
        &db,
        "GET /dashboard-repair-history HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(dashboard_repair_history.contains("\"history\""));
    assert!(dashboard_repair_history.contains("\"total_runs\""));
    assert!(dashboard_repair_history.contains("\"runs_by_source\""));

    let recall = http_once(
        &db,
        "GET /recall?q=memory%20ui&max_chars=800 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(recall.contains("\"recall\""));
    assert!(recall.contains("\"token_saving_estimate\""));

    let inbox_v2 = http_once(
        &db,
        "GET /inbox-v2 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(inbox_v2.contains("\"inbox_v2\""));
    assert!(inbox_v2.contains("\"groups\""));

    let feedback_body = format!(
        r#"{{"ids":["{memory_id}"],"rating":"useful","command":"test","query":"memory ui"}}"#
    );
    let feedback_request = format!(
        "POST /feedback HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        feedback_body.len(),
        feedback_body
    );
    let feedback = http_once(&db, &feedback_request);
    assert!(feedback.contains("\"feedback\""));
    assert!(feedback.contains("\"positive\""));

    let policy_body = r#"{"dry_run":true}"#;
    let policy_request = format!(
        "POST /policy-tune HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        policy_body.len(),
        policy_body
    );
    let policy = http_once(&db, &policy_request);
    assert!(policy.contains("\"policy\""));
    assert!(policy.contains("\"risk_limit\""));

    let qa = http_once(
        &db,
        "GET /memory-qa?since_days=7 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(qa.contains("\"qa\""));
    assert!(qa.contains("\"score\""));
    assert!(qa.contains("\"semantic_result_rate\""));
    assert!(qa.contains("\"semantic_eligible_result_rate\""));
    assert!(qa.contains("\"semantic_eligible_empty_read_count\""));
    assert!(qa.contains("\"semantic_empty_queries\""));

    let ops = http_once(
        &db,
        "GET /ops-status?since_days=7 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(ops.contains("\"ops\""));
    assert!(ops.contains("\"effectiveness\""));
    assert!(ops.contains("\"semantic_result_rate\""));
    assert!(ops.contains("\"semantic_eligible_result_rate\""));
    assert!(ops.contains("\"semantic_eligible_empty_read_count\""));
    assert!(ops.contains("\"semantic_empty_queries\""));
    assert!(ops.contains("\"provider_reachable\""));
    assert!(ops.contains("\"provider_health_ms\""));
    assert!(ops.contains("\"provider_error\""));
    assert!(ops.contains("\"agent_integration\""));
    assert!(ops.contains("\"skill_installed\""));
    assert!(ops.contains("\"fresh\""));
    assert!(ops.contains("\"age_secs\""));
    assert!(ops.contains("\"storage\""));
    assert!(ops.contains("\"db_bytes\""));
    assert!(ops.contains("\"vacuum_recommended\""));
    assert!(ops.contains("\"multi_device\""));
    assert!(ops.contains("\"inferred_missing\":1"));

    let contract = http_once(
        &db,
        "GET /memory-contract HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(contract.contains("\"contract\""));
    assert!(contract.contains("Project Contract"));

    let upgrade_body = r#"{"dry_run":true}"#;
    let upgrade_request = format!(
        "POST /upgrade-project HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        upgrade_body.len(),
        upgrade_body
    );
    let upgrade = http_once(&db, &upgrade_request);
    assert!(upgrade.contains("\"upgrade\""));
    assert!(upgrade.contains("\"dry_run\":true"));

    let embed_status = http_once(
        &db,
        "GET /embed-status HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(embed_status.contains("\"embedding\""));
    assert!(embed_status.contains("\"provider_reachable\""));
    assert!(embed_status.contains("\"provider_health_ms\""));
    assert!(embed_status.contains("\"provider_error\""));

    let embed_body = r#"{"provider":"mock","endpoint":"local","model":"mock-small"}"#;
    let embed_request = format!(
        "POST /embed-index HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        embed_body.len(),
        embed_body
    );
    let embed_index = http_once(&db, &embed_request);
    assert!(embed_index.contains("\"embedding\""));
    assert!(embed_index.contains("\"mock-small\""));

    let body = format!(r#"{{"id":"{memory_id}","status":"uncertain"}}"#);
    let request = format!(
        "POST /memory/status HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let status = http_once(&db, &request);
    assert!(status.contains("\"status\":\"uncertain\""));

    let evidence_body = format!(r#"{{"id":"{memory_id}"}}"#);
    let evidence_request = format!(
        "POST /evidence HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        evidence_body.len(),
        evidence_body
    );
    let evidence = http_once(&db, &evidence_request);
    assert!(evidence.contains("\"evidence\""));
    assert!(evidence.contains("Memory UI"));
    assert!(evidence.contains("\"request_count\""));

    let update_body = format!(
        r#"{{"id":"{memory_id}","title":"Updated Memory UI","body":"Updated body from the workbench.","type":"decision","scope":"project","status":"active","confidence":0.91,"replace_links":true,"links":["file:src/app/http_server.rs"]}}"#
    );
    let update_request = format!(
        "POST /memory/update HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        update_body.len(),
        update_body
    );
    let update = http_once(&db, &update_request);
    assert!(update.contains("\"ok\":true"));
    assert!(update.contains("Updated Memory UI"));
    assert!(update.contains("file"));

    let bulk_body = format!(r#"{{"ids":["{memory_id}"],"action":"uncertain"}}"#);
    let bulk_request = format!(
        "POST /memory/bulk HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        bulk_body.len(),
        bulk_body
    );
    let bulk = http_once(&db, &bulk_request);
    assert!(bulk.contains("\"changed\":1"));

    let autopilot = http_once(
        &db,
        "GET /autopilot/ui HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(autopilot.contains("\"alert\""));
    assert!(autopilot.contains("\"report\""));

    let autopilot_body = "{}";
    let repair_request = format!(
        "POST /autopilot/repair HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        autopilot_body.len(),
        autopilot_body
    );
    let repair = http_once(&db, &repair_request);
    assert!(repair.contains("\"repair\""));

    let run_once_request = format!(
        "POST /autopilot/run-once HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        autopilot_body.len(),
        autopilot_body
    );
    let run_once = http_once(&db, &run_once_request);
    assert!(run_once.contains("\"ok\":true"));

    let export_request = format!(
        "POST /autopilot/export-status HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        autopilot_body.len(),
        autopilot_body
    );
    let export_status = http_once(&db, &export_request);
    assert!(export_status.contains("\"output\""));

    let autonomous_status = http_once(
        &db,
        "GET /autonomous/status HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(autonomous_status.contains("\"status_file\""));

    let autonomous_body =
        r#"{"level":"conservative","provider":"mock","endpoint":"local","model":"mock-small"}"#;
    let autonomous_request = format!(
        "POST /autonomous/run-once HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        autonomous_body.len(),
        autonomous_body
    );
    let autonomous = http_once(&db, &autonomous_request);
    assert!(autonomous.contains("\"report\""));
    assert!(autonomous.contains("\"embed_index\""));
    assert!(autonomous.contains("\"optimize_storage\""));
    assert!(autonomous.contains("\"live_eval\""));
    assert!(autonomous.contains("\"live_eval_snapshot\""));

    let inbox = http_once(
        &db,
        "GET /inbox?status=pending&limit=10 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(inbox.contains("\"items\""));
    assert!(inbox.contains("browser based"));
}

#[test]
fn v14_9_autonomous_memory_runs_and_rolls_back() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let status_file = dir.path().join(".agent").join("autonomous-status.json");
    let rollback_dir = dir.path().join(".agent").join("autonomous-rollbacks");
    let backup_dir = dir.path().join(".agent").join("backups");
    fs::create_dir_all(dir.path().join("src").join("app")).unwrap();
    fs::write(dir.path().join("src").join("app").join("autonomous.rs"), "").unwrap();

    stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("Autonomous compacted project operational memory")
            .arg(
                "Autonomously compacted operational memory: Nested compact marker should not appear.",
            )
            .arg("--scope")
            .arg("project")
            .arg("--source")
            .arg("autonomous_compact"),
    );
    let legacy_compact_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("task_state")
            .arg("Autonomous compacted legacy operational memory")
            .arg("Legacy compact card that should inherit source links.")
            .arg("--scope")
            .arg("project")
            .arg("--source")
            .arg("autonomous_compact"),
    )
    .trim()
    .to_string();
    let legacy_source_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("note")
            .arg("Legacy source with evidence link")
            .arg("Source row for old compact link repair.")
            .arg("--scope")
            .arg("project")
            .arg("--link")
            .arg("file:src/app/autonomous.rs"),
    )
    .trim()
    .to_string();
    Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1 WHERE id = ?2",
            params![legacy_compact_id, legacy_source_id],
        )
        .unwrap();

    for idx in 0..3 {
        stdout(
            cmd(&db)
                .arg("add")
                .arg("note")
                .arg(format!("Operational note {idx}"))
                .arg(format!("Temporary operational context {idx}."))
                .arg("--scope")
                .arg("project")
                .arg("--link")
                .arg("file:src/app/autonomous.rs"),
        );
    }
    for version in ["0.14.20", "0.14.21", "0.14.22"] {
        let prefix = if version == "0.14.22" {
            "dukememory."
        } else {
            "dukememory"
        };
        stdout(
            cmd(&db)
                .arg("add")
                .arg("task_state")
                .arg(format!("{prefix} {version} release history noise released"))
                .arg(format!(
                    "Release {version} added autonomous memory behavior that should be compacted into release history."
                ))
                .arg("--scope")
                .arg("project")
                .arg("--link")
                .arg("file:src/app/autonomous.rs"),
        );
    }
    stdout(
        cmd(&db)
            .arg("add")
            .arg("known_issue")
            .arg("Verbose memory card without evidence")
            .arg("This weak memory card is intentionally long, unlinked, and unused. ".repeat(30))
            .arg("--scope")
            .arg("project"),
    );
    let slim_body = "Slim candidate durable detail. ".repeat(70);
    let slim_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Long autonomous slim candidate")
            .arg(&slim_body)
            .arg("--scope")
            .arg("project"),
    )
    .trim()
    .to_string();
    let explicit_link_id = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Explicit path link candidate")
            .arg("This card explicitly references src/app/autonomous.rs and should receive a file link.")
            .arg("--scope")
            .arg("project"),
    )
    .trim()
    .to_string();
    let resolved_quality_target = stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Healthy quality target")
            .arg("This concise linked memory should not remain a quality review candidate.")
            .arg("--scope")
            .arg("project")
            .arg("--link")
            .arg("file:src/app.rs"),
    )
    .trim()
    .to_string();
    let resolved_quality_inbox = "resolved-quality-test";
    Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO memory_inbox (id, type, scope, title, body, source, confidence, status, created_at, updated_at)
             VALUES (?1, 'task_state', 'project', ?2, ?3, 'autonomous_quality', 0.58, 'pending', 1, 1)",
            params![
                resolved_quality_inbox,
                format!("Review memory quality: {resolved_quality_target} Healthy quality target"),
                format!("Autonomous quality review candidate for memory {resolved_quality_target} (design_note). Score 35.0; requests=0; links=0; body_chars=80. Reasons: stale test.")
            ],
        )
        .unwrap();

    let run = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let run_json: Value = serde_json::from_str(&run).unwrap();
    assert_eq!(run_json["ok"], true);
    assert!(
        run_json["rollback_backup"]
            .as_str()
            .unwrap()
            .ends_with(".db")
    );
    assert!(run_json["quality"]["average_score"].as_f64().unwrap() >= 0.0);
    assert_eq!(run_json["budget"]["profile"], "deep");
    assert!(
        run_json["project_profile"]["memory_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(run_json["live_eval"]["reads"].as_u64().is_some());
    assert!(
        run_json["live_eval"]["useful_rate_source"]
            .as_str()
            .is_some()
    );
    let actions = run_json["actions"].as_array().unwrap();
    assert!(actions.iter().any(|item| item["kind"] == "embed_index"));
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "optimize_storage" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "agent_integration_repair" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "live_eval_snapshot" && item["status"] == "ok")
    );
    assert!(dir.path().join(".agent").join("config.toml").exists());
    assert!(
        fs::read_to_string(dir.path().join("AGENTS.md"))
            .unwrap()
            .contains("<!-- DUKEMEMORY_START -->")
    );
    let autonomous_status_text = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("status")
            .arg("--status-file")
            .arg(&status_file),
    );
    assert!(autonomous_status_text.contains("live_eval: reads="));
    let autonomous_explain_text = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("explain")
            .arg("--status-file")
            .arg(&status_file),
    );
    assert!(autonomous_explain_text.contains("live_eval: reads="));
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "compact_operational" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "compact_release_history" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "quality_inbox" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "slim_long_memory" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "repair_compact_links" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "repair_explicit_file_links" && item["status"] == "ok")
    );
    assert!(
        actions
            .iter()
            .any(|item| item["kind"] == "resolve_quality_inbox" && item["status"] == "ok")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "compact_operational")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "compact_release_history")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "quality_inbox")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "slim_long_memory")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "repair_compact_links")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "repair_explicit_file_links")
    );
    assert!(
        run_json["policy"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "resolve_quality_inbox")
    );
    assert!(status_file.exists());

    let conn = Connection::open(&db).unwrap();
    let resolved_quality_status: String = conn
        .query_row(
            "SELECT status FROM memory_inbox WHERE id = ?1",
            [resolved_quality_inbox],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved_quality_status, "rejected");
    let repaired_legacy_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_links WHERE memory_id = ?1 AND target = 'src/app/autonomous.rs'",
            [&legacy_compact_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(repaired_legacy_links, 1);
    let repaired_explicit_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_links WHERE memory_id = ?1 AND target = 'src/app/autonomous.rs'",
            [&explicit_link_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(repaired_explicit_links, 1);

    let slimmed_body: String = conn
        .query_row(
            "SELECT body FROM memories WHERE id = ?1",
            [&slim_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(slimmed_body.starts_with("Autonomously slimmed from"));
    assert!(slimmed_body.len() <= 900);
    assert!(slimmed_body.contains("Long autonomous slim candidate"));
    assert!(slimmed_body.len() < slim_body.len());

    let compact_body: String = conn
        .query_row(
            "SELECT body FROM memories WHERE title = 'Autonomous compacted project operational memory' AND body LIKE '%Operational note%' ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(compact_body.len() <= 1800);
    assert!(compact_body.contains("Operational note"));
    assert!(!compact_body.contains("Nested compact marker should not appear"));
    assert!(!compact_body.contains("release history noise"));
    let release_body: String = conn
        .query_row(
            "SELECT body FROM memories WHERE title = 'Autonomous compacted project release history' AND body LIKE '%release history noise%' ORDER BY updated_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(release_body.len() <= 1400);
    assert!(release_body.contains("0.14.20"));
    assert!(release_body.contains("0.14.22"));
    let inherited_operational_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_links l JOIN memories m ON m.id = l.memory_id WHERE m.title = 'Autonomous compacted project operational memory' AND l.target = 'src/app/autonomous.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(inherited_operational_links >= 1);
    let inherited_release_links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_links l JOIN memories m ON m.id = l.memory_id WHERE m.title = 'Autonomous compacted project release history' AND l.target = 'src/app/autonomous.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(inherited_release_links >= 1);

    let quality = stdout(
        cmd(&db)
            .arg("quality-report")
            .arg("--since-days")
            .arg("30")
            .arg("--json"),
    );
    let quality_json: Value = serde_json::from_str(&quality).unwrap();
    assert!(quality_json["average_score"].as_f64().unwrap() >= 0.0);
    let usefulness_after_run = stdout(cmd(&db).arg("usefulness-report").arg("--json"));
    let usefulness_after_run_json: Value = serde_json::from_str(&usefulness_after_run).unwrap();
    assert!(
        !usefulness_after_run_json["unused"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == explicit_link_id)
    );
    assert!(
        quality_json["weakest"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["id"] == explicit_link_id
                    && item["reasons"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|reason| reason == "fresh; waiting for use")
            })
    );

    let feedback = stdout(
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg("nonexistent-test-id")
            .arg("--rating")
            .arg("useful")
            .arg("--command")
            .arg("autonomous-test")
            .arg("--query")
            .arg("autonomous memory")
            .arg("--json"),
    );
    let feedback_json: Value = serde_json::from_str(&feedback).unwrap();
    assert_eq!(feedback_json["ok"], true);

    let budget = stdout(
        cmd(&db)
            .arg("budget-plan")
            .arg("small bug fix")
            .arg("--json"),
    );
    let budget_json: Value = serde_json::from_str(&budget).unwrap();
    assert!(budget_json["max_chars"].as_u64().unwrap() >= 1200);

    let profile = stdout(
        cmd(&db)
            .arg("project-profile")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let profile_json: Value = serde_json::from_str(&profile).unwrap();
    assert!(profile_json["memory_count"].as_u64().unwrap() >= 1);

    let recall = stdout(
        cmd(&db)
            .arg("recall")
            .arg("operational note")
            .arg("--max-chars")
            .arg("800")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let recall_json: Value = serde_json::from_str(&recall).unwrap();
    assert!(!recall_json["items"].as_array().unwrap().is_empty());

    insert_empty_read_event(&db, "impact", "missing autonomous rollback memory");
    insert_empty_read_event(
        &db,
        "brief",
        "memory agent project retrieval token quality context recall brief semantic",
    );
    let missing_feedback = stdout(
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg("missing-manual-id")
            .arg("--rating")
            .arg("missing")
            .arg("--command")
            .arg("impact")
            .arg("--query")
            .arg("manual missing memory policy")
            .arg("--json"),
    );
    let missing_feedback_json: Value = serde_json::from_str(&missing_feedback).unwrap();
    assert_eq!(missing_feedback_json["ok"], true);
    stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Resolved missing memory policy")
            .arg("This card resolves the resolved missing memory policy query.")
            .arg("--scope")
            .arg("project"),
    );
    let resolved_missing_feedback = stdout(
        cmd(&db)
            .arg("feedback")
            .arg("--id")
            .arg("resolved-missing-id")
            .arg("--rating")
            .arg("missing")
            .arg("--command")
            .arg("impact")
            .arg("--query")
            .arg("resolved missing memory policy")
            .arg("--json"),
    );
    let resolved_missing_feedback_json: Value =
        serde_json::from_str(&resolved_missing_feedback).unwrap();
    assert_eq!(resolved_missing_feedback_json["ok"], true);

    let eval_live = stdout(
        cmd(&db)
            .arg("eval")
            .arg("live")
            .arg("--since-days")
            .arg("7")
            .arg("--json"),
    );
    let eval_live_json: Value = serde_json::from_str(&eval_live).unwrap();
    assert!(eval_live_json["reads"].as_u64().unwrap() >= 1);
    assert!(eval_live_json["inferred_useful_rate"].as_f64().unwrap() > 0.0);
    assert_eq!(eval_live_json["inferred_missing"].as_u64().unwrap(), 1);
    assert_eq!(
        eval_live_json["inferred_missing_queries"][0],
        "missing autonomous rollback memory"
    );
    assert!(
        !eval_live_json["inferred_missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item
                == "memory agent project retrieval token quality context recall brief semantic")
    );
    assert!(
        eval_live_json["missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "manual missing memory policy")
    );
    assert!(
        eval_live_json["missing_queries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "resolved missing memory policy")
    );

    let inbox_v2 = stdout(cmd(&db).arg("inbox-v2").arg("report").arg("--json"));
    let inbox_v2_json: Value = serde_json::from_str(&inbox_v2).unwrap();
    assert_eq!(inbox_v2_json["version"], 1);

    let policy_tune = stdout(cmd(&db).arg("policy-tune").arg("--dry-run").arg("--json"));
    let policy_tune_json: Value = serde_json::from_str(&policy_tune).unwrap();
    assert!(policy_tune_json["risk_limit"].as_f64().unwrap() > 0.0);

    let qa = stdout(
        cmd(&db)
            .arg("memory-qa")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let qa_json: Value = serde_json::from_str(&qa).unwrap();
    assert!(qa_json["score"].as_f64().unwrap() >= 0.0);
    assert!(qa_json["active_memories"].as_u64().unwrap() >= 1);
    assert!(qa_json["semantic_result_rate"].as_f64().is_some());
    assert!(qa_json["semantic_empty_read_count"].as_u64().is_some());
    assert!(qa_json["semantic_avg_results"].as_f64().is_some());
    assert!(qa_json["semantic_eligible_result_rate"].as_f64().is_some());
    assert!(
        qa_json["semantic_eligible_empty_read_count"]
            .as_u64()
            .is_some()
    );

    let ops = stdout(
        cmd(&db)
            .arg("ops-status")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let ops_json: Value = serde_json::from_str(&ops).unwrap();
    assert!(ops_json["score"].as_f64().unwrap() >= 0.0);
    assert!(
        ops_json["effectiveness"]["semantic_result_rate"]
            .as_f64()
            .is_some()
    );
    assert!(
        ops_json["effectiveness"]["semantic_empty_read_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        ops_json["effectiveness"]["semantic_avg_results"]
            .as_f64()
            .is_some()
    );
    assert!(
        ops_json["effectiveness"]["semantic_eligible_result_rate"]
            .as_f64()
            .is_some()
    );
    assert!(
        ops_json["effectiveness"]["semantic_eligible_empty_read_count"]
            .as_u64()
            .is_some()
    );
    assert!(ops_json["agent_integration"]["ready"].as_bool().is_some());
    assert!(
        ops_json["agent_integration"]["project_memory_installed"]
            .as_bool()
            .is_some()
    );
    assert!(
        ops_json["agent_integration"]["project_config_present"]
            .as_bool()
            .is_some()
    );
    assert!(
        ops_json["agent_integration"]["agents_block_present"]
            .as_bool()
            .is_some()
    );
    assert!(
        ops_json["agent_integration"]["skill_installed"]
            .as_bool()
            .is_some()
    );
    assert!(ops_json["autonomous"]["fresh"].as_bool().is_some());
    assert!(
        ops_json["autonomous"]["age_secs"].is_number()
            || ops_json["autonomous"]["age_secs"].is_null()
    );
    assert!(
        ops_json["autonomous"]["last_action_count"].is_number()
            || ops_json["autonomous"]["last_action_count"].is_null()
    );
    assert!(
        ops_json["autonomous"]["daemon_embedding_skipped"].is_boolean()
            || ops_json["autonomous"]["daemon_embedding_skipped"].is_null()
    );
    assert!(
        ops_json["autonomous"]["daemon_embedding_error"].is_string()
            || ops_json["autonomous"]["daemon_embedding_error"].is_null()
    );
    assert!(
        ops_json["autonomous"]["daemon_embedding_repaired_at"].is_number()
            || ops_json["autonomous"]["daemon_embedding_repaired_at"].is_null()
    );
    assert!(
        ops_json["autonomous"]["daemon_embedding_repair_source"].is_string()
            || ops_json["autonomous"]["daemon_embedding_repair_source"].is_null()
    );
    assert!(ops_json["repair_loop"]["observed"].as_bool().is_some());
    assert!(ops_json["repair_loop"]["healthy"].as_bool().is_some());
    assert!(ops_json["repair_loop"]["runs"].as_u64().is_some());
    assert!(
        ops_json["repair_loop"]["actions_by_code"]
            .as_object()
            .is_some()
    );
    assert!(ops_json["gap_inbox"]["pending"].as_u64().is_some());
    assert!(ops_json["gap_inbox"]["stale_pending"].as_u64().is_some());
    assert!(ops_json["gap_inbox"]["total"].as_u64().is_some());
    assert!(ops_json["gap_inbox"]["approved"].as_u64().is_some());
    assert!(ops_json["gap_inbox"]["rejected"].as_u64().is_some());
    assert!(
        ops_json["gap_inbox"]["oldest_pending_age_secs"].is_number()
            || ops_json["gap_inbox"]["oldest_pending_age_secs"].is_null()
    );
    assert!(ops_json["effectiveness"]["reads"].as_u64().unwrap() >= 1);
    assert!(
        ["feedback", "inferred"].contains(
            &ops_json["effectiveness"]["useful_rate_source"]
                .as_str()
                .unwrap()
        )
    );
    assert!(
        ops_json["effectiveness"]["inferred_useful_rate"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(
        ops_json["effectiveness"]["inferred_missing"]
            .as_u64()
            .unwrap(),
        1
    );
    assert!(ops_json["storage"]["db_bytes"].as_u64().unwrap() > 0);
    assert!(ops_json["storage"]["page_count"].as_i64().unwrap() > 0);
    assert!(ops_json["storage"]["freelist_count"].as_i64().unwrap() >= 0);
    assert!(ops_json["storage"]["freelist_ratio"].as_f64().unwrap() >= 0.0);
    assert!(
        ops_json["storage"]["vacuum_recommended"]
            .as_bool()
            .is_some()
    );
    assert!(
        ops_json["storage"]["agent_bytes"].as_u64().unwrap()
            >= ops_json["storage"]["db_bytes"].as_u64().unwrap()
    );
    assert!(ops_json["storage"]["retention_ready"].as_bool().is_some());
    assert!(["ok", "warn"].contains(&ops_json["storage"]["pressure"].as_str().unwrap()));
    assert_eq!(ops_json["multi_device"]["local_first"], true);
    let ops_text = stdout(cmd(&db).arg("ops-status").arg("--root").arg(dir.path()));
    assert!(ops_text.contains("semantic_results="));
    assert!(ops_text.contains("gap_inbox: pending="));
    assert!(ops_text.contains("stale_pending="));
    assert!(ops_text.contains("oldest_pending_age_secs="));

    let gap_run = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let gap_run_json: Value = serde_json::from_str(&gap_run).unwrap();
    assert!(
        gap_run_json["inferred_feedback"]["written"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        gap_run_json["inferred_feedback"]["missing"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(gap_run_json["feedback"]["missing"].as_u64().unwrap() >= 1);
    assert!(
        gap_run_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "inferred_feedback" && item["status"] == "ok")
    );
    assert!(
        gap_run_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "gap_inbox" && item["status"] == "ok")
    );
    assert!(
        gap_run_json["rollback"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "RejectInboxItem")
    );
    let gap_inbox = stdout(cmd(&db).arg("inbox-list").arg("--json"));
    let gap_inbox_json: Value = serde_json::from_str(&gap_inbox).unwrap();
    let gap_items = gap_inbox_json.as_array().unwrap();
    assert!(gap_items.iter().any(|item| {
        item["source"] == "autonomous_gap"
            && item["body"]
                .as_str()
                .unwrap()
                .contains("missing autonomous rollback memory")
    }));
    assert!(gap_items.iter().any(|item| {
        item["source"] == "autonomous_gap"
            && item["body"]
                .as_str()
                .unwrap()
                .contains("manual missing memory policy")
    }));
    assert!(!gap_items.iter().any(|item| {
        item["source"] == "autonomous_gap"
            && item["body"]
                .as_str()
                .unwrap()
                .contains("resolved missing memory policy")
    }));
    assert!(gap_items.iter().any(|item| {
        item["source"] == "autonomous_quality"
            && item["title"]
                .as_str()
                .unwrap()
                .contains("Verbose memory card without evidence")
    }));
    stdout(
        cmd(&db)
            .arg("add")
            .arg("design_note")
            .arg("Resolved autonomous rollback gap")
            .arg("This durable card resolves missing autonomous rollback memory.")
            .arg("--scope")
            .arg("project"),
    );

    let gap_repeat = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let gap_repeat_json: Value = serde_json::from_str(&gap_repeat).unwrap();
    assert_eq!(
        gap_repeat_json["inferred_feedback"]["written"]
            .as_u64()
            .unwrap(),
        0
    );
    assert!(
        gap_repeat_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "gap_inbox" && item["status"] == "skipped")
    );
    assert!(
        gap_repeat_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "gap_inbox_resolved" && item["status"] == "ok")
    );
    assert!(
        gap_repeat_json["rollback"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "RestoreInboxStatus")
    );
    let resolved_gap_status: String = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT status FROM memory_inbox WHERE source = 'autonomous_gap' AND title = 'Fill memory gap: missing autonomous rollback memory'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved_gap_status, "rejected");

    let contract = stdout(
        cmd(&db)
            .arg("memory-contract")
            .arg("--root")
            .arg(dir.path())
            .arg("--write")
            .arg("--json"),
    );
    let contract_json: Value = serde_json::from_str(&contract).unwrap();
    assert_eq!(contract_json["written"], true);
    assert_eq!(contract_json["max_chars"], 1100);
    assert!(contract_json["content"].as_str().unwrap().chars().count() <= 1100);
    assert!(
        dir.path()
            .join(".agent")
            .join("MEMORY_CONTRACT.md")
            .exists()
    );
    assert!(contract_json["memory_id"].as_str().is_some());
    let contract_memory_body: String = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT body FROM memories WHERE id = ?1",
            [contract_json["memory_id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(contract_memory_body.chars().count() <= 1100);

    let upgrade = stdout(
        cmd(&db)
            .arg("upgrade-project")
            .arg("--root")
            .arg(dir.path())
            .arg("--dry-run")
            .arg("--json"),
    );
    let upgrade_json: Value = serde_json::from_str(&upgrade).unwrap();
    assert_eq!(upgrade_json["dry_run"], true);
    assert!(upgrade_json["actions"].as_array().unwrap().len() >= 3);

    Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE memory_inbox SET created_at = ?1 WHERE source = 'autonomous_gap' AND status = 'pending'",
            params![now_ms() - 7_200_000],
        )
        .unwrap();
    insert_empty_read_event(&db, "brief", "fresh dashboard memory gap");
    let dashboard = stdout(cmd(&db).arg("dashboard").arg("--json"));
    let dashboard_json: Value = serde_json::from_str(&dashboard).unwrap();
    assert!(!dashboard_json["projects"].as_array().unwrap().is_empty());
    assert!(dashboard_json["ok"].as_bool().is_some());
    assert!(dashboard_json["status"].as_str().is_some());
    assert!(dashboard_json["total_projects"].as_u64().unwrap() >= 1);
    assert!(dashboard_json["attention_projects"].as_u64().is_some());
    assert!(dashboard_json["memory_gap_projects"].as_u64().unwrap() >= 1);
    assert!(dashboard_json["memory_gap_count"].as_u64().unwrap() >= 1);
    assert!(
        dashboard_json["gap_inbox_pending_projects"]
            .as_u64()
            .is_some()
    );
    assert!(dashboard_json["gap_inbox_pending_count"].as_u64().is_some());
    assert!(
        dashboard_json["gap_inbox_stale_projects"]
            .as_u64()
            .is_some()
    );
    assert!(dashboard_json["gap_inbox_stale_count"].as_u64().is_some());
    assert!(
        dashboard_json["gap_inbox_oldest_pending_age_secs"].is_number()
            || dashboard_json["gap_inbox_oldest_pending_age_secs"].is_null()
    );
    assert!(dashboard_json["recommendations_count"].as_u64().is_some());
    assert!(
        dashboard_json["attention_reason_counts"]
            .as_object()
            .is_some()
    );
    assert!(dashboard_json["repair_actions_count"].as_u64().is_some());
    assert!(
        dashboard_json["safe_repair_actions_count"]
            .as_u64()
            .is_some()
    );
    assert!(dashboard_json["repair_loop_projects"].as_u64().is_some());
    assert!(
        dashboard_json["repair_loop_failed_projects"]
            .as_u64()
            .is_some()
    );
    assert!(
        dashboard_json["repair_loop_safe_skipped_projects"]
            .as_u64()
            .is_some()
    );
    assert!(
        dashboard_json["daemon_embedding_skipped_projects"]
            .as_u64()
            .is_some()
    );
    assert!(
        dashboard_json["daemon_embedding_repaired_projects"]
            .as_u64()
            .is_some()
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["autonomous_live_reads"].is_number()
                || item["autonomous_live_reads"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["autonomous_age_secs"].is_number()
                || item["autonomous_age_secs"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["daemon_embedding_skipped"].is_boolean()
                || item["daemon_embedding_skipped"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["daemon_embedding_error"].is_string()
                || item["daemon_embedding_error"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["daemon_embedding_repaired_at"].is_number()
                || item["daemon_embedding_repaired_at"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["daemon_embedding_repair_source"].is_string()
                || item["daemon_embedding_repair_source"].is_null())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["gap_inbox"]["pending"].as_u64().is_some()
                && item["gap_inbox"]["stale_pending"].as_u64().is_some()
                && item["gap_inbox"]["total"].as_u64().is_some()
                && (item["gap_inbox"]["oldest_pending_age_secs"].is_number()
                    || item["gap_inbox"]["oldest_pending_age_secs"].is_null()))
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["recommendations"].as_array().is_some())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["autonomous_inferred_missing"]
                .as_u64()
                .unwrap_or_default()
                >= 1
                && item["attention_reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reason| reason == "memory_gaps_detected")
                && item["repair_actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action["code"] == "run_autonomous"
                        && action["reason"] == "memory_gaps_detected"
                        && action["safe_auto"] == true))
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["gap_inbox"]["oldest_pending_age_secs"]
                .as_i64()
                .unwrap_or_default()
                >= 3_600
                && item["gap_inbox"]["stale_pending"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1
                && item["attention_reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reason| reason == "gap_inbox_stale")
                && item["recommendations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|recommendation| recommendation
                        .as_str()
                        .unwrap()
                        .contains("stale gap inbox item")))
    );
    let stale_ops = stdout(
        cmd(&db)
            .arg("ops-status")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let stale_ops_json: Value = serde_json::from_str(&stale_ops).unwrap();
    assert!(
        stale_ops_json["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap()
                .contains("autonomous gap inbox has stale pending items"))
    );
    assert!(
        stale_ops_json["gap_inbox"]["stale_pending"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        stale_ops_json["recommendations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|recommendation| recommendation
                .as_str()
                .unwrap()
                .contains("refresh stale gap inbox items"))
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["attention_reasons"].as_array().is_some())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["repair_actions"].as_array().is_some())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["repair_loop"]["observed"].as_bool().is_some()
                && item["repair_loop"]["runs"].as_u64().is_some()
                && item["repair_loop"]["actions_by_code"].as_object().is_some())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|item| item["repair_actions"].as_array().unwrap().iter())
            .any(|action| action["code"].as_str().is_some()
                && action["safe_auto"].as_bool().is_some()
                && action["command"].as_array().is_some())
    );
    assert!(
        dashboard_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"].as_str().is_some() && item["attention"].as_bool().is_some())
    );
    let dashboard_text = stdout(cmd(&db).arg("dashboard"));
    assert!(dashboard_text.contains("summary: status="));
    assert!(dashboard_text.contains("attention="));
    assert!(dashboard_text.contains("live_reads="));
    assert!(dashboard_text.contains("live_gaps="));
    assert!(dashboard_text.contains("memory_gap_projects="));
    assert!(dashboard_text.contains("memory_gap_count="));
    assert!(dashboard_text.contains("gap_inbox_pending_projects="));
    assert!(dashboard_text.contains("gap_inbox_pending_count="));
    assert!(dashboard_text.contains("gap_inbox_stale_projects="));
    assert!(dashboard_text.contains("gap_inbox_stale_count="));
    assert!(dashboard_text.contains("gap_inbox_oldest_age="));
    assert!(dashboard_text.contains("gap_inbox_pending="));
    assert!(dashboard_text.contains("gap_inbox_stale="));
    assert!(dashboard_text.contains("gap_inbox_stale"));
    assert!(dashboard_text.contains("auto_age="));
    assert!(dashboard_text.contains("reasons="));
    assert!(dashboard_text.contains("repairs="));
    assert!(dashboard_text.contains("repair_actions="));
    assert!(dashboard_text.contains("repair_loop_projects="));
    assert!(dashboard_text.contains("repair_runs="));
    assert!(dashboard_text.contains("repair_failed="));
    assert!(dashboard_text.contains("repair_safe_skipped="));
    assert!(dashboard_text.contains("daemon_embedding_skipped="));
    assert!(dashboard_text.contains("daemon_embedding_repaired="));
    assert!(dashboard_text.contains("daemon_embedding_repaired_at="));
    assert!(dashboard_text.contains("recommendations="));

    let dashboard_repair = stdout(
        cmd(&db)
            .arg("dashboard-repair")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let dashboard_repair_json: Value = serde_json::from_str(&dashboard_repair).unwrap();
    assert_eq!(dashboard_repair_json["apply"], false);
    assert!(dashboard_repair_json["total_actions"].as_u64().is_some());
    assert!(dashboard_repair_json["safe_actions"].as_u64().is_some());
    assert!(dashboard_repair_json["applied_actions"].as_u64().is_some());
    assert!(dashboard_repair_json["skipped_actions"].as_u64().is_some());
    let repair_projects = dashboard_repair_json["projects"].as_array().unwrap();
    assert!(repair_projects.iter().all(|project| {
        project["priority"].as_i64().is_some()
            && project["gap_inbox_stale_pending"].as_u64().is_some()
            && (project["gap_inbox_oldest_pending_age_secs"].is_number()
                || project["gap_inbox_oldest_pending_age_secs"].is_null())
    }));
    assert!(
        repair_projects
            .windows(2)
            .all(|pair| pair[0]["priority"].as_i64().unwrap()
                >= pair[1]["priority"].as_i64().unwrap())
    );
    assert!(repair_projects.iter().any(|project| {
        project["priority"].as_i64().unwrap() > 0
            && project["gap_inbox_stale_pending"]
                .as_u64()
                .unwrap_or_default()
                >= 1
    }));
    assert!(
        repair_projects
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .all(|action| action["applied"] == false && action["skipped"] == true)
    );
    let dashboard_repair_apply = stdout(
        cmd(&db)
            .arg("dashboard-repair")
            .arg("--apply")
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let dashboard_repair_apply_json: Value = serde_json::from_str(&dashboard_repair_apply).unwrap();
    assert_eq!(dashboard_repair_apply_json["apply"], true);
    assert_eq!(dashboard_repair_apply_json["ok"], true);
    assert!(
        dashboard_repair_apply_json["applied_actions"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        dashboard_repair_apply_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .any(|action| action["safe_auto"] == true && action["applied"] == true)
    );
    assert!(
        dashboard_repair_apply_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .any(|action| action["safe_auto"] == true
                && action["applied"] == true
                && action["detail"]
                    .as_str()
                    .unwrap()
                    .contains("inferred_feedback:"))
    );
    assert!(
        dashboard_repair_apply_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|project| project["actions"].as_array().unwrap().iter())
            .any(|action| action["safe_auto"] == true
                && action["applied"] == true
                && action["detail"].as_str().unwrap().contains("gap_inbox:"))
    );
    let dashboard_repair_history = stdout(cmd(&db).arg("dashboard-repair-history").arg("--json"));
    let dashboard_repair_history_json: Value =
        serde_json::from_str(&dashboard_repair_history).unwrap();
    assert!(
        dashboard_repair_history_json["total_runs"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        dashboard_repair_history_json["applied_actions"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        dashboard_repair_history_json["runs_by_source"]
            .as_object()
            .unwrap()
            .contains_key("cli")
    );
    assert!(
        dashboard_repair_history_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| !project["recent"].as_array().unwrap().is_empty())
    );
    let ops_after_repair = stdout(
        cmd(&db)
            .arg("ops-status")
            .arg("--root")
            .arg(dir.path())
            .arg("--json"),
    );
    let ops_after_repair_json: Value = serde_json::from_str(&ops_after_repair).unwrap();
    assert!(
        ops_after_repair_json["repair_loop"]["runs"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        ops_after_repair_json["repair_loop"]["applied_actions"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        !ops_after_repair_json["repair_loop"]["actions_by_code"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let dashboard_after_repair = stdout(cmd(&db).arg("dashboard").arg("--json"));
    let dashboard_after_repair_json: Value = serde_json::from_str(&dashboard_after_repair).unwrap();
    assert!(
        dashboard_after_repair_json["repair_loop_projects"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        dashboard_after_repair_json["projects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["repair_loop"]["runs"].as_u64().unwrap() >= 1)
    );
    let dashboard_repair_history_text = stdout(cmd(&db).arg("dashboard-repair-history"));
    assert!(dashboard_repair_history_text.contains("dukememory. Dashboard Repair History"));
    assert!(dashboard_repair_history_text.contains("summary: runs="));
    let dashboard_repair_text = stdout(cmd(&db).arg("dashboard-repair"));
    assert!(dashboard_repair_text.contains("dukememory. Dashboard Repair"));
    assert!(dashboard_repair_text.contains("summary: apply=false"));
    assert!(dashboard_repair_text.contains("priority="));
    assert!(dashboard_repair_text.contains("gap_inbox_stale="));

    let onboard_root = dir.path().join("onboarded");
    fs::create_dir_all(&onboard_root).unwrap();
    let onboard = stdout(
        cmd(&db)
            .arg("onboard")
            .arg("--root")
            .arg(&onboard_root)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let onboard_json: Value = serde_json::from_str(&onboard).unwrap();
    assert_eq!(onboard_json["ok"], true);
    assert!(onboard_root.join(".agent").join("memory.db").exists());

    let superseded = stdout(
        cmd(&db)
            .arg("list")
            .arg("--type")
            .arg("note")
            .arg("--status")
            .arg("superseded")
            .arg("--json"),
    );
    let superseded_json: Value = serde_json::from_str(&superseded).unwrap();
    assert_eq!(
        superseded_json
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["title"]
                .as_str()
                .unwrap()
                .starts_with("Operational note"))
            .count(),
        3
    );

    let no_change_run = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("conservative")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("mock")
            .arg("--endpoint")
            .arg("local")
            .arg("--model")
            .arg("mock-small")
            .arg("--json"),
    );
    let no_change_json: Value = serde_json::from_str(&no_change_run).unwrap();
    assert!(
        no_change_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "preserve_rollback")
    );
    assert!(!no_change_json["rollback"].as_array().unwrap().is_empty());

    let status = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("status")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--json"),
    );
    let status_json: Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status_json["level"], "conservative");

    let explain = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("explain")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--json"),
    );
    let explain_json: Value = serde_json::from_str(&explain).unwrap();
    assert_eq!(explain_json["rollback_available"], true);

    let rollback = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("rollback")
            .arg("--status-file")
            .arg(&status_file)
            .arg("--json"),
    );
    let rollback_json: Value = serde_json::from_str(&rollback).unwrap();
    assert_eq!(rollback_json["level"], "rollback");
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_restore_status")
    );
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_restore_body")
    );
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_restore_links")
    );
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_restore_inbox_status")
    );
    assert!(
        rollback_json["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["kind"] == "rollback_reject_inbox")
    );
    let restored_slim_body: String = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT body FROM memories WHERE id = ?1",
            [&slim_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_slim_body, slim_body);
    let restored_quality_inbox_status: String = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT status FROM memory_inbox WHERE id = ?1",
            [resolved_quality_inbox],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_quality_inbox_status, "pending");
    let restored_legacy_links: i64 = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM memory_links WHERE memory_id = ?1",
            [&legacy_compact_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_legacy_links, 0);
    let restored_explicit_links: i64 = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM memory_links WHERE memory_id = ?1",
            [&explicit_link_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_explicit_links, 0);
    let rejected_gap_inbox: i64 = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM memory_inbox WHERE source = 'autonomous_gap' AND status = 'rejected'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rejected_gap_inbox >= 1);
    let rejected_quality_inbox: i64 = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM memory_inbox WHERE source = 'autonomous_quality' AND status = 'rejected'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(rejected_quality_inbox >= 1);

    let active = stdout(
        cmd(&db)
            .arg("list")
            .arg("--type")
            .arg("note")
            .arg("--status")
            .arg("active")
            .arg("--json"),
    );
    let active_json: Value = serde_json::from_str(&active).unwrap();
    assert_eq!(active_json.as_array().unwrap().len(), 3);

    let plist = stdout(
        cmd(&db)
            .arg("autonomous")
            .arg("install")
            .arg("--output")
            .arg(dir.path().join("com.dukememory.autonomous.plist"))
            .arg("--status-file")
            .arg(&status_file)
            .arg("--rollback-dir")
            .arg(&rollback_dir)
            .arg("--backup-dir")
            .arg(&backup_dir)
            .arg("--provider")
            .arg("ollama")
            .arg("--endpoint")
            .arg("http://192.168.0.13:11434")
            .arg("--model")
            .arg("bge-m3:latest")
            .arg("--dry-run"),
    );
    assert!(plist.contains("<key>WorkingDirectory</key>"));
    assert!(plist.contains("dukememory-autonomous.out.log"));
    assert!(plist.contains("<string>--provider</string>"));
    assert!(plist.contains("<string>ollama</string>"));
    assert!(plist.contains("<string>http://192.168.0.13:11434</string>"));
    assert!(plist.contains("<string>bge-m3:latest</string>"));
    assert!(!plist.contains("<string>.agent/dukememory-autonomous.out.log</string>"));
}

#[test]
fn v14_7_memory_ui_selects_sibling_project_memory() {
    let dir = tempdir().unwrap();
    let alpha = dir.path().join("alpha_project");
    let beta = dir.path().join("beta_project");
    fs::create_dir_all(alpha.join(".agent")).unwrap();
    fs::create_dir_all(beta.join(".agent")).unwrap();
    let alpha_db = alpha.join(".agent").join("memory.db");
    let beta_db = beta.join(".agent").join("memory.db");

    cmd(&alpha_db)
        .arg("add")
        .arg("decision")
        .arg("Alpha memory")
        .arg("Only the alpha project should show this card.")
        .assert()
        .success();
    cmd(&beta_db)
        .arg("add")
        .arg("decision")
        .arg("Beta memory")
        .arg("Only the beta project should show this card.")
        .assert()
        .success();

    let projects = http_once(
        &alpha_db,
        "GET /projects HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(projects.contains("\"key\":\"alpha_project\""));
    assert!(projects.contains("\"key\":\"beta_project\""));
    assert!(projects.contains("\"current\":true"));

    let beta_memory = http_once(
        &alpha_db,
        "GET /memory?project=beta_project&status=active&type=decision HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(beta_memory.contains("Beta memory"));
    assert!(!beta_memory.contains("Alpha memory"));

    let html = http_once(
        &alpha_db,
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(html.contains("id=\"project\""));
    assert!(html.contains("id=\"lang\""));
    assert!(html.contains("/projects"));
}

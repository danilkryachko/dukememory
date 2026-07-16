use super::*;

#[test]
fn default_help_hides_compatibility_only_commands_but_keeps_them_callable() {
    let help = String::from_utf8(
        Command::cargo_bin("dukememory")
            .unwrap()
            .arg("--help")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    for legacy in [
        "release-gate-v2",
        "autonomous-loop-v2",
        "mcp-tool-surface-v2",
        "mcp-discipline-v2",
        "memory-control-center-v2",
        "memory-contract-v2",
        "fleet-dashboard-v2",
        "upgrade-all-projects-v2",
    ] {
        assert!(!help.contains(&format!("  {legacy}")), "{legacy}");
        Command::cargo_bin("dukememory")
            .unwrap()
            .arg(legacy)
            .arg("--help")
            .assert()
            .success();
    }
    let visible_commands = help
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("      "))
        .count();
    assert!(visible_commands <= 225, "visible commands={visible_commands}");
}

fn http_once_with_read_token(
    db: &std::path::Path,
    token_file: &std::path::Path,
    request: &str,
) -> String {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
        .env("DUKEMEMORY_HTTP_READ_TOKEN_FILE", token_file)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut url = String::new();
    reader.read_line(&mut url).unwrap();
    let port: u16 = url.trim().rsplit(':').next().unwrap().parse().unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(stream, "{request}").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(child.wait().unwrap().success());
    response
}

#[test]
fn read_only_http_token_enforces_http_and_mcp_operation_scopes() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let token_file = dir.path().join("read-token");
    fs::write(&token_file, "read-only-secret\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let read = http_once_with_read_token(
        &db,
        &token_file,
        "GET /memory HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer read-only-secret\r\nConnection: close\r\n\r\n",
    );
    assert!(read.contains("200 OK"));

    let write_body = r#"{"type":"decision","title":"denied","body":"denied"}"#;
    let write_request = format!(
        "POST /remember HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer read-only-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{write_body}",
        write_body.len()
    );
    let denied = http_once_with_read_token(&db, &token_file, &write_request);
    assert!(denied.contains("403 Forbidden"));

    let read_mcp = serde_json::json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"memory_status","arguments":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}
    })
    .to_string();
    let read_mcp_request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer read-only-secret\r\nContent-Type: application/json\r\nMcp-Method: tools/call\r\nMcp-Name: memory_status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{read_mcp}",
        read_mcp.len()
    );
    assert!(http_once_with_read_token(&db, &token_file, &read_mcp_request).contains("200 OK"));

    let write_mcp = serde_json::json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"memory_remember","arguments":{"text":"denied"},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}
    })
    .to_string();
    let write_mcp_request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer read-only-secret\r\nContent-Type: application/json\r\nMcp-Method: tools/call\r\nMcp-Name: memory_remember\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{write_mcp}",
        write_mcp.len()
    );
    assert!(
        http_once_with_read_token(&db, &token_file, &write_mcp_request)
            .contains("403 Forbidden")
    );
}

fn http_response_header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response
        .split_once("\r\n\r\n")?
        .0
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find_map(|(header_name, value)| {
            header_name
                .eq_ignore_ascii_case(name)
                .then(|| value.trim())
        })
}

fn mcp_http_post(body: &str, extra_headers: &str) -> String {
    format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
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
    assert!(html.contains("/ui.js"));
    let javascript = http_once(
        &alpha_db,
        "GET /ui.js HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(javascript.contains("/projects"));
}

#[test]
fn memory_mutations_roll_back_when_audit_logging_fails() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db).arg("stats").assert().success();
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        r#"
        CREATE TRIGGER fail_memory_added
        BEFORE INSERT ON memory_events
        WHEN NEW.event_type = 'memory_added'
        BEGIN
            SELECT RAISE(ABORT, 'forced audit failure');
        END;
        "#,
    )
    .unwrap();
    drop(conn);

    cmd(&db)
        .arg("add")
        .arg("decision")
        .arg("Must roll back")
        .arg("The memory insert must not survive a failed audit event.")
        .assert()
        .failure()
        .stderr(contains("transaction failed: add_memory"));

    let conn = Connection::open(&db).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE title = 'Must roll back'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn inbox_approval_uses_nested_savepoint_and_rolls_back_as_one_unit() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db).arg("stats").assert().success();
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        r#"
        INSERT INTO memory_inbox (
            id, type, scope, title, body, source, confidence, status,
            created_at, updated_at, layer
        ) VALUES ('atomic-inbox', 'decision', 'project', 'Atomic approval',
            'Approval must commit memory and inbox state together.', 'test', 1.0,
            'pending', 1, 1, 'architecture')
        "#,
        [],
    )
    .unwrap();
    conn.execute_batch(
        r#"
        CREATE TRIGGER fail_inbox_approved
        BEFORE INSERT ON memory_events
        WHEN NEW.event_type = 'inbox_approved'
        BEGIN
            SELECT RAISE(ABORT, 'forced approval audit failure');
        END;
        "#,
    )
    .unwrap();
    drop(conn);

    cmd(&db)
        .arg("inbox-approve")
        .arg("atomic-inbox")
        .assert()
        .failure()
        .stderr(contains("transaction failed: approve_inbox"));

    let conn = Connection::open(&db).unwrap();
    let memory_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE title = 'Atomic approval'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let inbox_status: String = conn
        .query_row(
            "SELECT status FROM memory_inbox WHERE id = 'atomic-inbox'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(memory_count, 0);
    assert_eq!(inbox_status, "pending");
}

#[test]
fn external_http_bind_requires_token_and_enforces_bearer_auth() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");

    cmd(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("0.0.0.0")
        .arg("--port")
        .arg("0")
        .arg("--once")
        .assert()
        .failure()
        .stderr(contains("external HTTP binds require"));

    let unauthorized = http_once_configured(
        &db,
        "127.0.0.1",
        Some("test-http-token"),
        None,
        "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(unauthorized.contains("401 Unauthorized"));

    let authorized = http_once_configured(
        &db,
        "127.0.0.1",
        Some("test-http-token"),
        None,
        "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer test-http-token\r\nConnection: close\r\n\r\n",
    );
    assert!(authorized.contains("200 OK"));
    assert!(authorized.contains("\"memories\""));

    let token_file = dir.path().join("http-token");
    fs::write(&token_file, "file-http-token\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let file_authorized = http_once_configured(
        &db,
        "127.0.0.1",
        None,
        Some(&token_file),
        "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer file-http-token\r\nConnection: close\r\n\r\n",
    );
    assert!(file_authorized.contains("200 OK"));

    let body = r#"{"query":"origin check"}"#;
    let cross_origin = http_once(
        &db,
        &format!(
            "POST /search HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://evil.example\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );
    assert!(cross_origin.contains("403 Forbidden"));
    let rebound_origin = http_once(
        &db,
        &format!(
            "POST /search HTTP/1.1\r\nHost: attacker.example\r\nOrigin: http://attacker.example\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );
    assert!(rebound_origin.contains("403 Forbidden"));
    assert!(rebound_origin.contains("request Host is not allowed"));
    let fetch_metadata_cross_site = http_once(
        &db,
        &format!(
            "POST /search HTTP/1.1\r\nHost: 127.0.0.1\r\nSec-Fetch-Site: cross-site\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );
    assert!(fetch_metadata_cross_site.contains("403 Forbidden"));
    let same_origin = http_once(
        &db,
        &format!(
            "POST /search HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );
    assert!(same_origin.contains("200 OK"));
}

#[test]
fn mcp_streamable_http_supports_stable_sessions_and_modern_stateless_discovery() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let server = PersistentHttpServer::start(&db);

    let initialize = serde_json::json!({
        "jsonrpc":"2.0",
        "id":1,
        "method":"initialize",
        "params":{
            "protocolVersion":"2025-11-25",
            "capabilities":{},
            "clientInfo":{"name":"http-test","version":"1"}
        }
    })
    .to_string();
    let initialized = server.request(&mcp_http_post(&initialize, ""));
    assert!(initialized.starts_with("HTTP/1.1 200 OK"), "{initialized}");
    let session_id = http_response_header(&initialized, "MCP-Session-Id")
        .expect("initialize must create an HTTP session")
        .to_string();
    assert_eq!(
        http_response_header(&initialized, "MCP-Protocol-Version"),
        Some("2025-11-25")
    );

    let notification = serde_json::json!({
        "jsonrpc":"2.0",
        "method":"notifications/initialized"
    })
    .to_string();
    let stable_headers = format!(
        "MCP-Session-Id: {session_id}\r\nMCP-Protocol-Version: 2025-11-25\r\n"
    );
    let accepted = server.request(&mcp_http_post(&notification, &stable_headers));
    assert!(accepted.starts_with("HTTP/1.1 202 Accepted"), "{accepted}");

    let list = serde_json::json!({
        "jsonrpc":"2.0",
        "id":2,
        "method":"tools/list",
        "params":{}
    })
    .to_string();
    let listed = server.request(&mcp_http_post(&list, &stable_headers));
    assert!(listed.starts_with("HTTP/1.1 200 OK"), "{listed}");
    let listed_json: Value = serde_json::from_str(listed.split_once("\r\n\r\n").unwrap().1)
        .unwrap();
    let tools = listed_json["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 12);
    assert!(tools.iter().any(|tool| tool["name"] == "memory_brief"));
    assert!(!tools.iter().any(|tool| tool["name"] == "memory_add"));

    let deleted = server.request(&format!(
        "DELETE /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nMCP-Session-Id: {session_id}\r\nConnection: close\r\n\r\n"
    ));
    assert!(deleted.starts_with("HTTP/1.1 204 No Content"), "{deleted}");
    let expired = server.request(&mcp_http_post(&list, &stable_headers));
    assert!(expired.starts_with("HTTP/1.1 404 Not Found"), "{expired}");

    let get = server.request(
        "GET /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
    );
    assert!(get.starts_with("HTTP/1.1 405 Method Not Allowed"), "{get}");
    assert_eq!(http_response_header(&get, "Allow"), Some("POST, DELETE"));

    let discover = serde_json::json!({
        "jsonrpc":"2.0",
        "id":3,
        "method":"server/discover",
        "params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion":"2026-07-28",
            "io.modelcontextprotocol/clientInfo":{"name":"http-test","version":"1"},
            "io.modelcontextprotocol/clientCapabilities":{}
        }}
    })
    .to_string();
    let modern_headers =
        "MCP-Protocol-Version: 2026-07-28\r\nMcp-Method: server/discover\r\n";
    let discovered = server.request(&mcp_http_post(&discover, modern_headers));
    assert!(discovered.starts_with("HTTP/1.1 200 OK"), "{discovered}");
    assert!(http_response_header(&discovered, "MCP-Session-Id").is_none());
    let discovered_json: Value =
        serde_json::from_str(discovered.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        discovered_json["result"]["supportedVersions"][0],
        "2026-07-28"
    );

    let missing_method = server.request(&mcp_http_post(
        &discover,
        "MCP-Protocol-Version: 2026-07-28\r\n",
    ));
    assert!(
        missing_method.starts_with("HTTP/1.1 400 Bad Request"),
        "{missing_method}"
    );
}

#[test]
fn production_deployment_templates_preserve_http_security_invariants() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let systemd = fs::read_to_string(root.join("deploy/systemd/dukememory.service")).unwrap();
    assert!(systemd.contains("serve-http --host 127.0.0.1 --port 8765"));
    assert!(systemd.contains("--auth-token-file /etc/dukememory/http-token"));
    assert!(systemd.contains("NoNewPrivileges=true"));
    assert!(systemd.contains("ProtectSystem=strict"));
    assert!(systemd.contains("KillSignal=SIGTERM"));

    let caddy = fs::read_to_string(root.join("deploy/caddy/Caddyfile")).unwrap();
    assert!(caddy.contains("reverse_proxy 127.0.0.1:8765"));
    assert!(caddy.contains("header_up Host {host}"));
    assert!(caddy.contains("Strict-Transport-Security"));

    let nginx = fs::read_to_string(root.join("deploy/nginx/dukememory.conf")).unwrap();
    assert!(nginx.contains("proxy_pass http://127.0.0.1:8765"));
    assert!(nginx.contains("proxy_set_header Host $host"));
    assert!(nginx.contains("ssl_protocols TLSv1.2 TLSv1.3"));

    let guide = fs::read_to_string(root.join("docs/production-deployment.md")).unwrap();
    assert!(guide.contains("curl --fail http://127.0.0.1:8765/health"));
    assert!(guide.contains("DUKEMEMORY_HTTP_ALLOWED_ORIGINS=https://memory.example.com"));
    assert!(guide.contains("DUKEMEMORY_SYNC_PASSPHRASE_FILE"));
}

#[cfg(unix)]
#[test]
fn http_server_drains_workers_on_termination_signal() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin("dukememory"))
        .arg("--db")
        .arg(&db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
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
    write!(
        stream,
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("200 OK"));

    let signal_status = StdCommand::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .unwrap();
    assert!(signal_status.success());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("HTTP server did not finish graceful shutdown within five seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn bitemporal_observations_are_available_through_cli() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("memory.db");
    let root = dir.path().join("project");
    fs::create_dir_all(&root).unwrap();
    let source = stdout(
        cmd(&db)
            .arg("add")
            .arg("decision")
            .arg("Evidence source")
            .arg("The decision has a durable evidence relationship."),
    )
    .trim()
    .to_string();
    let target = stdout(
        cmd(&db)
            .arg("add")
            .arg("constraint")
            .arg("Evidence target")
            .arg("The target constraint is independently addressable."),
    )
    .trim()
    .to_string();

    let observed = stdout(
        cmd(&db)
            .arg("observe")
            .arg(&source)
            .arg("--kind")
            .arg("verified")
            .arg("--statement")
            .arg("A test verified the relationship")
            .arg("--evidence-kind")
            .arg("test")
            .arg("--evidence-ref")
            .arg("cargo test bitemporal")
            .arg("--target-memory-id")
            .arg(&target)
            .arg("--valid-from")
            .arg("100")
            .arg("--root")
            .arg(&root)
            .arg("--json"),
    );
    let observed: Value = serde_json::from_str(&observed).unwrap();
    assert_eq!(observed["memory_id"], source);
    assert_eq!(observed["target_memory_id"], target);
    assert_eq!(observed["valid_from"], 100);

    let observations = stdout(
        cmd(&db)
            .arg("observations")
            .arg(&source)
            .arg("--valid-at")
            .arg("100")
            .arg("--json"),
    );
    let observations: Value = serde_json::from_str(&observations).unwrap();
    assert_eq!(observations.as_array().unwrap().len(), 1);

    let graph = stdout(
        cmd(&db)
            .arg("temporal-graph")
            .arg("--valid-at")
            .arg("100")
            .arg("--json"),
    );
    let graph: Value = serde_json::from_str(&graph).unwrap();
    assert_eq!(graph["edge_count"], 1);
    assert_eq!(graph["observation_count"], 1);
}

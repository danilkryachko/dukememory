use super::*;

pub(crate) fn serve_http(db: &Path, host: &str, port: u16, once: bool) -> Result<()> {
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind http server on {host}:{port}"))?;
    let addr = listener.local_addr()?;
    println!("http://{addr}");
    for stream in listener.incoming() {
        handle_http_stream(db, stream?)?;
        if once {
            break;
        }
    }
    Ok(())
}

fn handle_http_stream(db: &Path, mut stream: TcpStream) -> Result<()> {
    let response = match handle_http_request(db, &mut stream) {
        Ok(response) => response,
        Err(err) => HttpResponse::internal_error(err.to_string()),
    };
    crate::http_api::write_response(&mut stream, response)?;
    Ok(())
}

fn parse_json_body(body: &str) -> Result<Value> {
    serde_json::from_str(body).with_context(|| "request body must be valid JSON")
}

#[derive(Debug, Serialize)]
struct UiProject {
    key: String,
    name: String,
    root: String,
    db: String,
    current: bool,
    memories: i64,
    pending_inbox: i64,
}

struct UiProjectContext {
    db: PathBuf,
    root: PathBuf,
}

fn memory_rows_with_request_counts(conn: &Connection, rows: Vec<Memory>) -> Result<Vec<Value>> {
    let counts = memory_request_counts(conn)?;
    rows.into_iter()
        .map(|row| {
            let request_count = counts.get(&row.id).copied().unwrap_or(0);
            let mut value = serde_json::to_value(row)?;
            if let Value::Object(ref mut object) = value {
                object.insert("request_count".to_string(), json!(request_count));
            }
            Ok(value)
        })
        .collect()
}

fn filter_sort_memory_rows(
    conn: &Connection,
    mut rows: Vec<Memory>,
    usage: &str,
    sort: &str,
    stale_days: i64,
    limit: usize,
) -> Result<Vec<Value>> {
    let counts = memory_request_counts(conn)?;
    let stale_cutoff = now_ms().saturating_sub(stale_days.max(0).saturating_mul(86_400_000));
    rows.retain(|row| {
        let count = counts.get(&row.id).copied().unwrap_or(0);
        match usage {
            "hot" => count > 0,
            "unused" => count == 0,
            "stale" => row.updated_at < stale_cutoff,
            _ => true,
        }
    });
    match sort {
        "request_count" | "request_count_desc" => {
            rows.sort_by(|a, b| {
                counts
                    .get(&b.id)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&counts.get(&a.id).copied().unwrap_or(0))
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
            });
        }
        "request_count_asc" => {
            rows.sort_by(|a, b| {
                counts
                    .get(&a.id)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&counts.get(&b.id).copied().unwrap_or(0))
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
            });
        }
        "updated_asc" => rows.sort_by_key(|row| row.updated_at),
        _ => rows.sort_by_key(|row| std::cmp::Reverse(row.updated_at)),
    }
    rows.truncate(limit);
    memory_rows_with_request_counts(conn, rows)
}

fn parse_autonomous_level(value: Option<&str>) -> AutonomousLevel {
    match value.unwrap_or("normal") {
        "conservative" => AutonomousLevel::Conservative,
        "aggressive" => AutonomousLevel::Aggressive,
        _ => AutonomousLevel::Normal,
    }
}

fn handle_http_request(db: &Path, stream: &mut TcpStream) -> Result<HttpResponse> {
    let buffer = read_http_request(stream)?;
    let raw = String::from_utf8_lossy(&buffer);
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_ref(), ""));
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let parts = request_line.split_whitespace().collect::<Vec<_>>();
    let method = parts.first().copied().unwrap_or("");
    let raw_path = parts.get(1).copied().unwrap_or("/");
    let (path, query) = split_query(raw_path);
    let conn = open_db(db)?;
    let response = match (method, path) {
        ("GET", "/") | ("GET", "/ui") => HttpResponse::html(memory_ui_html()),
        ("GET", "/health") => {
            HttpResponse::ok(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
        }
        ("GET", "/projects") => HttpResponse::ok(json!({"projects": discover_projects(db)?})),
        ("GET", "/metrics") => HttpResponse::ok(http_metrics(&conn)?),
        ("GET", "/audit") => HttpResponse::ok(json!({"events": audit_events(&conn, 50)?})),
        ("GET", "/snapshot") => HttpResponse::ok(http_snapshot(&conn)?),
        ("GET", "/doctrine") => {
            HttpResponse::ok(json!({"doctrine": doctrine_report(&conn, None)?}))
        }
        ("GET", "/memory") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let q = params
                .get("q")
                .map(String::as_str)
                .filter(|value| !value.is_empty());
            let scope = params
                .get("scope")
                .map(String::as_str)
                .filter(|value| !value.is_empty());
            let types = params
                .get("type")
                .filter(|value| !value.is_empty() && value.as_str() != "all")
                .cloned()
                .into_iter()
                .collect::<Vec<_>>();
            let statuses = params
                .get("status")
                .filter(|value| !value.is_empty() && value.as_str() != "all")
                .cloned()
                .into_iter()
                .collect::<Vec<_>>();
            let limit = params
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(100)
                .min(500);
            let usage = params.get("usage").map(String::as_str).unwrap_or("all");
            let sort = params
                .get("sort")
                .map(String::as_str)
                .unwrap_or("updated_desc");
            let stale_days = params
                .get("stale_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30);
            let fetch_limit = if usage != "all" || sort != "updated_desc" {
                500
            } else {
                limit
            };
            let rows = query_memories(&conn, q, &types, &statuses, scope, fetch_limit)?;
            HttpResponse::ok(
                json!({"memories": filter_sort_memory_rows(&conn, rows, usage, sort, stale_days, limit)?}),
            )
        }
        ("GET", "/usefulness") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30);
            let stale_days = params
                .get("stale_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30);
            let hot_threshold = params
                .get("hot_threshold")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(3);
            HttpResponse::ok(
                json!({"usefulness": usefulness_report(&conn, since_days, stale_days, hot_threshold)?}),
            )
        }
        ("GET", "/quality") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30);
            let limit = params
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(20);
            HttpResponse::ok(json!({"quality": quality_report(&conn, since_days, limit)?}))
        }
        ("GET", "/budget-plan") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let task = params
                .get("task")
                .map(String::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("routine memory task");
            HttpResponse::ok(json!({"budget": budget_plan(&conn, task, Some("project"))?}))
        }
        ("GET", "/project-profile") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            HttpResponse::ok(
                json!({"profile": project_profile_snapshot(&conn, &ctx.root, "project")?}),
            )
        }
        ("GET", "/dashboard") => HttpResponse::ok(json!({"dashboard": dashboard_report(db)?})),
        ("GET", "/eval-live") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(7);
            HttpResponse::ok(json!({"eval": live_eval_report(&conn, since_days)?}))
        }
        ("GET", "/recall") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let q = params
                .get("q")
                .map(String::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("project memory");
            let max_chars = params
                .get("max_chars")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1200);
            let limit = params
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(8);
            HttpResponse::ok(json!({"recall": recall_report(&conn, &RecallRequest {
                query: q,
                max_chars,
                limit,
                scope: Some("project"),
                provider: DEFAULT_EMBED_PROVIDER,
                endpoint: DEFAULT_EMBED_ENDPOINT,
                model: DEFAULT_EMBED_MODEL,
                json_out: true,
            })?}))
        }
        ("GET", "/inbox-v2") => {
            let conn = open_selected_db(db, query, None)?;
            HttpResponse::ok(json!({"inbox_v2": inbox_v2_report(&conn, 100, false)?}))
        }
        ("POST", "/inbox-v2/auto-apply") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let dry_run = value
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            HttpResponse::ok(json!({"inbox_v2": inbox_v2_report(&conn, 100, !dry_run)?}))
        }
        ("POST", "/policy-tune") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let dry_run = value
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            HttpResponse::ok(json!({"policy": policy_tune_report(
                &conn,
                Path::new(".agent/autonomous-policy.json"),
                dry_run,
            )?}))
        }
        ("GET", "/memory-qa") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(7);
            HttpResponse::ok(json!({"qa": memory_qa_report(&conn, &ctx.root, since_days)?}))
        }
        ("GET", "/memory-contract") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            HttpResponse::ok(json!({"contract": memory_contract_report(&conn, &ctx.root, false)?}))
        }
        ("POST", "/upgrade-project") => {
            let value = parse_json_body(body)?;
            let ctx = selected_project_from_body(db, &value)?;
            let conn = open_db(&ctx.db)?;
            let dry_run = value
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            HttpResponse::ok(json!({"upgrade": upgrade_project_report(
                &conn,
                &ctx.root,
                None,
                "~/.local/bin/dukememory",
                &ctx.root.join(".agent/install-backups"),
                dry_run,
            )?}))
        }
        ("POST", "/feedback") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let ids = value
                .get("ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let rating = match value
                .get("rating")
                .and_then(Value::as_str)
                .unwrap_or("useful")
            {
                "useless" => FeedbackRating::Useless,
                "missing" => FeedbackRating::Missing,
                _ => FeedbackRating::Useful,
            };
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("web");
            let query = value.get("query").and_then(Value::as_str).unwrap_or("");
            let note = value.get("note").and_then(Value::as_str).unwrap_or("");
            let detail = serde_json::to_string(&json!({
                "rating": match rating {
                    FeedbackRating::Useful => "useful",
                    FeedbackRating::Useless => "useless",
                    FeedbackRating::Missing => "missing",
                },
                "ids": ids,
                "command": command,
                "query": query,
                "note": note,
            }))?;
            log_event(&conn, "memory_feedback", None, &detail)?;
            HttpResponse::ok(json!({"ok": true, "feedback": feedback_summary(&conn, 30)?}))
        }
        ("GET", "/embed-status") => {
            let conn = open_selected_db(db, query, None)?;
            HttpResponse::ok(json!({"embedding": embeddings::embed_status(
                &conn,
                DEFAULT_EMBED_PROVIDER,
                DEFAULT_EMBED_ENDPOINT,
                DEFAULT_EMBED_MODEL,
            )?}))
        }
        ("POST", "/embed-index") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_PROVIDER);
            let endpoint = value
                .get("endpoint")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_ENDPOINT);
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_MODEL);
            let report =
                embeddings::embed_index(&conn, provider, endpoint, model, &[], None, false)?;
            HttpResponse::ok(json!({"embedding": report}))
        }
        ("GET", "/inbox") => {
            let conn = open_selected_db(db, query, None)?;
            let params = parse_query(query);
            let status = params
                .get("status")
                .map(String::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("pending");
            let limit = params
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(100)
                .min(500);
            HttpResponse::ok(json!({"items": list_inbox(&conn, status, limit)?}))
        }
        ("GET", "/autopilot/ui") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let report = autopilot_report(
                &conn,
                AutopilotReportRequest {
                    status_file: &ctx.root.join(".agent").join("daemon-status.json"),
                    session_dir: &ctx.root.join(".agent").join("sessions"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    history_limit: 10,
                    provider: "mock",
                    endpoint: "local",
                    model: "mock-small",
                },
            )?;
            let alert = autopilot_alert(
                &conn,
                AutopilotAlertRequest {
                    status_file: &ctx.root.join(".agent").join("daemon-status.json"),
                    session_dir: &ctx.root.join(".agent").join("sessions"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    history_limit: 10,
                    max_pending: 100,
                    max_failed_ticks: 0,
                    max_status_age_secs: 180,
                    max_embedding_stale: 0,
                    require_backup: false,
                    require_endpoint: false,
                    provider: "mock",
                    endpoint: "local",
                    model: "mock-small",
                },
            )?;
            HttpResponse::ok(json!({"report": report, "alert": alert}))
        }
        ("POST", "/autopilot/run-once") => {
            let value = parse_json_body(body)?;
            let selected = value.get("project").and_then(Value::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            fs::create_dir_all(ctx.root.join(".agent").join("sessions"))?;
            fs::create_dir_all(ctx.root.join(".agent").join("backups"))?;
            run_daemon(
                &conn,
                DaemonRequest {
                    interval_secs: 1,
                    once: true,
                    quiet: true,
                    auto_ingest: true,
                    autopilot: true,
                    session_dir: &ctx.root.join(".agent").join("sessions"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    status_file: &ctx.root.join(".agent").join("daemon-status.json"),
                    backup_keep: 10,
                    backup_every_secs: 0,
                    cleanup_audit_keep: 5000,
                    db: &ctx.db,
                    scope: "project",
                    provider: "mock",
                    endpoint: "local",
                    model: "mock-small",
                },
            )?;
            HttpResponse::ok(json!({"ok": true}))
        }
        ("POST", "/autopilot/repair") => {
            let value = parse_json_body(body)?;
            let selected = value.get("project").and_then(Value::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let report = autopilot_repair(
                &conn,
                &ctx.db,
                AutopilotRepairRequest {
                    status_file: &ctx.root.join(".agent").join("daemon-status.json"),
                    session_dir: &ctx.root.join(".agent").join("sessions"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    backup_keep: 10,
                    cleanup_audit_keep: 5000,
                    scope: "project",
                    provider: "mock",
                    endpoint: "local",
                    model: "mock-small",
                    max_status_age_secs: 180,
                },
            )?;
            HttpResponse::ok(json!({"repair": report}))
        }
        ("POST", "/autopilot/export-status") => {
            let value = parse_json_body(body)?;
            let selected = value.get("project").and_then(Value::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let report = autopilot_report(
                &conn,
                AutopilotReportRequest {
                    status_file: &ctx.root.join(".agent").join("daemon-status.json"),
                    session_dir: &ctx.root.join(".agent").join("sessions"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    history_limit: 20,
                    provider: "mock",
                    endpoint: "local",
                    model: "mock-small",
                },
            )?;
            let output = ctx.root.join(".agent").join("autopilot-status.json");
            fs::create_dir_all(ctx.root.join(".agent"))?;
            write_file(&output, serde_json::to_string_pretty(&report)?.as_bytes())?;
            HttpResponse::ok(json!({"ok": true, "output": output.display().to_string()}))
        }
        ("GET", "/autonomous/status") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let status_file = ctx.root.join(".agent").join("autonomous-status.json");
            let report = read_autonomous_status(&status_file).ok();
            HttpResponse::ok(json!({
                "ok": report.as_ref().map(|report| report.ok).unwrap_or(false),
                "status_file": status_file.display().to_string(),
                "report": report,
            }))
        }
        ("POST", "/autonomous/run-once") => {
            let value = parse_json_body(body)?;
            let selected = value.get("project").and_then(Value::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let level = parse_autonomous_level(value.get("level").and_then(Value::as_str));
            let provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("mock");
            let endpoint = value
                .get("endpoint")
                .and_then(Value::as_str)
                .unwrap_or("local");
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("mock-small");
            let report = autonomous_run_once(
                &conn,
                AutonomousRunRequest {
                    level,
                    status_file: &ctx.root.join(".agent").join("autonomous-status.json"),
                    rollback_dir: &ctx.root.join(".agent").join("autonomous-rollbacks"),
                    backup_dir: &ctx.root.join(".agent").join("backups"),
                    backup_keep: 10,
                    db: &ctx.db,
                    scope: "project",
                    provider,
                    endpoint,
                    model,
                },
            )?;
            HttpResponse::ok(json!({"report": report}))
        }
        ("POST", "/autonomous/rollback") => {
            let value = parse_json_body(body)?;
            let selected = value.get("project").and_then(Value::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let status_file = ctx.root.join(".agent").join("autonomous-status.json");
            let report = read_autonomous_status(&status_file)?;
            let rollback = autonomous_rollback(&conn, &report)?;
            write_autonomous_status(&status_file, &rollback)?;
            HttpResponse::ok(json!({"report": rollback}))
        }
        ("POST", "/remember") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if text.is_empty() {
                HttpResponse::bad_request("missing text")
            } else {
                let id = add_memory(
                    &conn,
                    AddMemory {
                        id: None,
                        memory_type: value
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("note")
                            .to_string(),
                        title: truncate_words(text, 8),
                        body: text.to_string(),
                        scope: value
                            .get("scope")
                            .and_then(Value::as_str)
                            .unwrap_or("project")
                            .to_string(),
                        status: "active".to_string(),
                        source: Some("http".to_string()),
                        supersedes: None,
                        confidence: 0.8,
                        links: Vec::new(),
                    },
                )?;
                HttpResponse::ok(json!({"id": id}))
            }
        }
        ("POST", "/memory/status") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.is_empty() || status.is_empty() {
                return Ok(HttpResponse::bad_request("missing id or status"));
            }
            set_status(&conn, id, status.to_string())?;
            HttpResponse::ok(json!({"ok": true, "id": id, "status": status}))
        }
        ("POST", "/memory/delete") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() {
                return Ok(HttpResponse::bad_request("missing id"));
            }
            delete_memory(&conn, id)?;
            HttpResponse::ok(json!({"ok": true, "id": id}))
        }
        ("POST", "/memory/update") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() {
                return Ok(HttpResponse::bad_request("missing id"));
            }
            let links = value
                .get("links")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            update_memory(
                &conn,
                UpdateMemory {
                    id: id.to_string(),
                    memory_type: value
                        .get("type")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    title: value
                        .get("title")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    body: value
                        .get("body")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    scope: value
                        .get("scope")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    status: value
                        .get("status")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    source: value
                        .get("source")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    confidence: value.get("confidence").and_then(Value::as_f64),
                    links,
                    replace_links: value
                        .get("replace_links")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                },
            )?;
            HttpResponse::ok(json!({"ok": true, "memory": get_memory_with_links(&conn, id)?}))
        }
        ("POST", "/memory/bulk") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let ids = value
                .get("ids")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                .unwrap_or_default();
            let action = value
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if ids.is_empty() || action.is_empty() {
                return Ok(HttpResponse::bad_request("missing ids or action"));
            }
            let mut changed = 0;
            for id in ids {
                match action {
                    "active" => {
                        set_status(&conn, id, "active".to_string())?;
                        changed += 1;
                    }
                    "uncertain" => {
                        set_status(&conn, id, "uncertain".to_string())?;
                        changed += 1;
                    }
                    "reject" => {
                        set_status(&conn, id, "rejected".to_string())?;
                        changed += 1;
                    }
                    "delete" => {
                        delete_memory(&conn, id)?;
                        changed += 1;
                    }
                    _ => return Ok(HttpResponse::bad_request("unknown bulk action")),
                }
            }
            HttpResponse::ok(json!({"ok": true, "changed": changed}))
        }
        ("POST", "/context") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let task = value
                .get("task")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if task.is_empty() {
                return Ok(HttpResponse::bad_request("missing task"));
            }
            let rows = build_context_rows(
                &conn,
                ContextQuery {
                    task,
                    types: &[],
                    statuses: &["active".to_string(), "uncertain".to_string()],
                    scope: None,
                    limit: 12,
                    include_recent: 4,
                    rules: None,
                },
            )?;
            HttpResponse::ok(json!({"context": render_context_pack(&conn, &rows, 5000)?}))
        }
        ("POST", "/brief") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let task = value
                .get("task")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if task.is_empty() {
                return Ok(HttpResponse::bad_request("missing task"));
            }
            let budget = value
                .get("budget")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(1200);
            let limit = value
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(10);
            let scope = value.get("scope").and_then(Value::as_str);
            let provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_PROVIDER);
            let endpoint = value
                .get("endpoint")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_ENDPOINT);
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_EMBED_MODEL);
            let report = brief_report(
                &conn,
                &BriefRequest {
                    task,
                    limit,
                    budget,
                    scope,
                    rules: None,
                    provider,
                    endpoint,
                    model,
                    json_out: true,
                },
            )?;
            HttpResponse::ok(json!({"brief": report}))
        }
        ("POST", "/impact") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let target = value
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if target.is_empty() {
                return Ok(HttpResponse::bad_request("missing target"));
            }
            let budget = value
                .get("budget")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(1200);
            let limit = value
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(10);
            let scope = value.get("scope").and_then(Value::as_str);
            let report = impact_report(
                &conn,
                &ImpactRequest {
                    target,
                    limit,
                    budget,
                    scope,
                    json_out: true,
                },
            )?;
            HttpResponse::ok(json!({"impact": report}))
        }
        ("POST", "/drift") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let changed_only = value
                .get("changed_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let root = value.get("root").and_then(Value::as_str).unwrap_or(".");
            HttpResponse::ok(json!({"drift": drift_report(&conn, Path::new(root), changed_only)?}))
        }
        ("POST", "/search") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let query = value
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if query.is_empty() {
                return Ok(HttpResponse::bad_request("missing query"));
            }
            let rows = query_memories(
                &conn,
                Some(query),
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                10,
            )?;
            HttpResponse::ok(json!({"results": memory_rows_with_request_counts(&conn, rows)?}))
        }
        ("POST", "/inbox/approve") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() {
                return Ok(HttpResponse::bad_request("missing id"));
            }
            HttpResponse::ok(json!({"id": approve_inbox(&conn, id, false)?}))
        }
        ("POST", "/inbox/reject") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() {
                return Ok(HttpResponse::bad_request("missing id"));
            }
            reject_inbox(&conn, id)?;
            HttpResponse::ok(json!({"ok": true, "id": id}))
        }
        ("POST", "/evidence") => {
            let value = parse_json_body(body)?;
            let conn = open_selected_db(db, query, Some(&value))?;
            let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
            if id.is_empty() {
                return Ok(HttpResponse::bad_request("missing id"));
            }
            let evidence = evidence_report(&conn, id)?;
            let request_count = memory_request_count(&conn, id)?;
            HttpResponse::ok(json!({"evidence": evidence, "request_count": request_count}))
        }
        ("POST", "/auto-ingest") => {
            let value = parse_json_body(body)?;
            let input = value
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or(".agent/sessions");
            let scope = value
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let dry_run = value
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let report = auto_ingest_sessions(
                &conn,
                Path::new(input),
                scope,
                false,
                DEFAULT_EMBED_ENDPOINT,
                "qwen3:14b",
                dry_run,
            )?;
            HttpResponse::ok(json!({"auto_ingest": report}))
        }
        ("POST", "/doctor") => HttpResponse::ok(json!({
            "secrets": scan_secret_findings(&conn)?.len(),
            "pending_inbox": list_inbox(&conn, "pending", usize::MAX)?.len()
        })),
        ("POST", "/sync/export") => {
            let export = export_memories(&conn, &[], &[], None)?;
            HttpResponse::ok(json!({"export": export}))
        }
        ("POST", "/merge/candidates") => {
            HttpResponse::ok(json!({"candidates": merge_candidates(&conn, 20)?}))
        }
        ("POST", "/merge/apply") => {
            let value = parse_json_body(body)?;
            let primary = value
                .get("primary_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let duplicate = value
                .get("duplicate_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if primary.is_empty() || duplicate.is_empty() {
                return Ok(HttpResponse::bad_request(
                    "missing primary_id or duplicate_id",
                ));
            }
            merge_apply(&conn, primary, duplicate, false)?;
            HttpResponse::ok(json!({"ok": true, "primary_id": primary}))
        }
        _ => HttpResponse::not_found(),
    };
    Ok(response)
}

fn split_query(path: &str) -> (&str, &str) {
    path.split_once('?').unwrap_or((path, ""))
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            let key = percent_decode(key);
            if key.is_empty() {
                None
            } else {
                Some((key, percent_decode(value)))
            }
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_selected_db(default_db: &Path, query: &str, body: Option<&Value>) -> Result<Connection> {
    let selected = selected_project_key(query, body);
    let db = resolve_project_db(default_db, selected.as_deref())?;
    open_db(&db)
}

fn selected_project_from_body(default_db: &Path, body: &Value) -> Result<UiProjectContext> {
    project_context(
        default_db,
        body.get("project")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty()),
    )
}

fn selected_project_key(query: &str, body: Option<&Value>) -> Option<String> {
    body.and_then(|value| value.get("project").and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            parse_query(query)
                .get("project")
                .filter(|value| !value.is_empty())
                .cloned()
        })
}

fn resolve_project_db(default_db: &Path, selected: Option<&str>) -> Result<PathBuf> {
    let Some(key) = selected else {
        return Ok(default_db.to_path_buf());
    };
    if key == "current" {
        return Ok(default_db.to_path_buf());
    }
    discover_projects(default_db)?
        .into_iter()
        .find(|project| project.key == key)
        .map(|project| PathBuf::from(project.db))
        .with_context(|| format!("unknown project: {key}"))
}

fn project_context(default_db: &Path, selected: Option<&str>) -> Result<UiProjectContext> {
    let db = resolve_project_db(default_db, selected)?;
    let root = project_root_for_db(&db).unwrap_or_else(|| {
        db.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    Ok(UiProjectContext { db, root })
}

fn discover_projects(default_db: &Path) -> Result<Vec<UiProject>> {
    let current_db = canonical_or_absolute(default_db);
    let mut dbs = Vec::new();
    push_unique_db(&mut dbs, default_db);
    if let Some(root) = project_root_for_db(default_db)
        && let Some(parent) = root.parent()
    {
        for entry in fs::read_dir(parent)
            .with_context(|| format!("failed to scan projects in {}", parent.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let candidate = entry.path().join(".agent").join("memory.db");
                if candidate.exists() {
                    push_unique_db(&mut dbs, &candidate);
                }
            }
        }
    }
    let mut projects = dbs
        .into_iter()
        .filter_map(|db_path| ui_project_from_db(&current_db, &db_path).transpose())
        .collect::<Result<Vec<_>>>()?;
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

fn push_unique_db(dbs: &mut Vec<PathBuf>, db: &Path) {
    let key = canonical_or_absolute(db);
    if !dbs
        .iter()
        .any(|existing| canonical_or_absolute(existing) == key)
    {
        dbs.push(db.to_path_buf());
    }
}

fn ui_project_from_db(current_db: &Path, db: &Path) -> Result<Option<UiProject>> {
    let Some(root) = project_root_for_db(db) else {
        return Ok(None);
    };
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string();
    let db_path = canonical_or_absolute(db);
    let (memories, pending_inbox) = project_counts(&db_path).unwrap_or((0, 0));
    Ok(Some(UiProject {
        key: name.clone(),
        name,
        root: root.display().to_string(),
        db: db_path.display().to_string(),
        current: db_path == current_db,
        memories,
        pending_inbox,
    }))
}

fn project_counts(db: &Path) -> Result<(i64, i64)> {
    let conn = open_db(db)?;
    let memories = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    let pending = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    Ok((memories, pending))
}

fn project_root_for_db(db: &Path) -> Option<PathBuf> {
    let db = canonical_or_absolute(db);
    let agent_dir = db.parent()?;
    if agent_dir.file_name()?.to_str()? != ".agent" {
        return None;
    }
    agent_dir.parent().map(Path::to_path_buf)
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn memory_ui_html() -> &'static str {
    r##"<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>dukememory</title>
  <style>
    :root {
      --bg: #f5f6f8;
      --panel: #ffffff;
      --panel-soft: #fafbfc;
      --text: #202733;
      --muted: #6c788b;
      --line: #dfe4ec;
      --line-strong: #cfd7e3;
      --accent: #0f766e;
      --accent-strong: #0b625c;
      --danger: #b42318;
      --danger-bg: #fff5f4;
      --warn: #a15c07;
      --ok: #16703c;
      --shadow: 0 12px 34px rgba(24, 32, 44, .07);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font: 14px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--text);
      -webkit-font-smoothing: antialiased;
    }
    header {
      height: 66px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 18px;
      padding: 0 26px;
      border-bottom: 1px solid var(--line);
      background: rgba(255, 255, 255, .96);
      position: sticky;
      top: 0;
      z-index: 10;
    }
    .brand {
      display: grid;
      gap: 1px;
      min-width: 190px;
    }
    h1 { font-size: 18px; margin: 0; font-weight: 720; letter-spacing: 0; }
    .subtitle {
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }
    .header-actions {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 10px;
      min-width: 0;
      flex: 1;
    }
    .header-actions select {
      width: min(420px, 42vw);
      max-width: 420px;
      min-width: 220px;
      height: 40px;
      flex: 0 1 420px;
    }
    .header-actions .meta {
      white-space: nowrap;
      flex: 0 0 auto;
    }
    .header-actions .lang-select {
      width: 78px;
      min-width: 78px;
      max-width: 78px;
      flex: 0 0 78px;
    }
    .header-actions .icon {
      flex: 0 0 38px;
    }
    main {
      display: grid;
      grid-template-columns: minmax(420px, 1fr) minmax(340px, 420px);
      gap: 18px;
      padding: 20px;
      max-width: 1660px;
      margin: 0 auto;
    }
    .toolbar, .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      box-shadow: var(--shadow);
    }
    .toolbar {
      display: grid;
      grid-template-columns: 1fr 150px 150px 92px;
      gap: 10px;
      padding: 14px;
      margin-bottom: 14px;
    }
    input, select, textarea {
      width: 100%;
      border: 1px solid var(--line-strong);
      border-radius: 6px;
      padding: 9px 10px;
      background: #fff;
      color: var(--text);
      font: inherit;
      outline: none;
    }
    input:focus, select:focus, textarea:focus {
      border-color: var(--accent);
      box-shadow: 0 0 0 3px rgba(15, 118, 110, .11);
    }
    textarea { min-height: 124px; resize: vertical; }
    button {
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 8px 11px;
      background: #fff;
      color: var(--text);
      font: inherit;
      font-weight: 560;
      cursor: pointer;
      white-space: nowrap;
    }
    button:hover { border-color: #9aa5b5; background: #f8fafc; }
    button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    button.primary:hover { background: var(--accent-strong); border-color: var(--accent-strong); }
    button.danger { color: var(--danger); background: var(--danger-bg); border-color: #f0cbc7; }
    button.icon { width: 38px; padding: 8px 0; }
    .grid { display: grid; grid-template-columns: 1fr; gap: 12px; }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 14px;
      display: grid;
      gap: 10px;
    }
    .card:hover { border-color: var(--line-strong); }
    .card.selected { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(15, 118, 110, .13); }
    .row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
    .title { font-weight: 720; font-size: 15px; overflow-wrap: anywhere; }
    .body { color: #374151; white-space: pre-wrap; overflow-wrap: anywhere; max-width: 1120px; }
    .meta { color: var(--muted); font-size: 12px; font-weight: 540; }
    .pill {
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
      border: 1px solid var(--line);
      padding: 2px 8px;
      font-size: 12px;
      font-weight: 620;
      color: var(--muted);
      background: #fafbfc;
    }
    .pill.active { color: var(--ok); border-color: rgba(22, 112, 60, .25); }
    .pill.rejected { color: var(--danger); border-color: rgba(180, 35, 24, .25); }
    .pill.uncertain { color: var(--warn); border-color: rgba(161, 92, 7, .25); }
    .side {
      display: grid;
      gap: 12px;
      align-content: start;
      position: sticky;
      top: 86px;
      max-height: calc(100vh - 106px);
      overflow: auto;
      padding-bottom: 12px;
    }
    .panel { padding: 14px; }
    .panel h2 { margin: 0 0 12px; font-size: 15px; font-weight: 720; letter-spacing: 0; }
    .stack { display: grid; gap: 8px; }
    .split { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
    .actions { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; padding-top: 2px; }
    .actions button { padding: 7px 10px; font-size: 13px; }
    .quick-filters {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin: 0 0 12px;
    }
    .quick-filters button {
      padding: 6px 10px;
      font-size: 12px;
      color: var(--muted);
      background: var(--panel);
    }
    .quick-filters button.active {
      color: #fff;
      background: var(--accent);
      border-color: var(--accent);
    }
    mark {
      background: #fff1a8;
      color: inherit;
      border-radius: 3px;
      padding: 0 2px;
    }
    .tabs {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 6px;
      margin-bottom: 12px;
    }
    .tab {
      padding: 8px 6px;
      font-size: 12px;
      color: var(--muted);
      background: var(--panel-soft);
    }
    .tab.active {
      color: #fff;
      border-color: var(--accent);
      background: var(--accent);
    }
    .tab-panel { display: none; }
    .tab-panel.active { display: grid; gap: 10px; }
    .bulkbar {
      display: none;
      align-items: center;
      gap: 8px;
      flex-wrap: wrap;
      margin: 0 0 12px;
      padding: 10px 12px;
      background: #eef7f6;
      border: 1px solid rgba(15, 118, 110, .18);
      border-radius: 8px;
      color: var(--accent-strong);
      font-weight: 650;
    }
    .bulkbar.visible { display: flex; }
    .check {
      width: 18px;
      height: 18px;
      accent-color: var(--accent);
      flex: 0 0 auto;
    }
    .kv {
      display: grid;
      grid-template-columns: 108px 1fr;
      gap: 6px 10px;
      font-size: 13px;
    }
    .kv b { color: var(--muted); font-weight: 650; }
    .stat-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
    .stat {
      background: var(--panel-soft);
      border: 1px solid var(--line);
      border-radius: 7px;
      padding: 10px;
    }
    .stat strong { display: block; font-size: 18px; }
    .stat span { color: var(--muted); font-size: 12px; }
    .edit-form { display: grid; gap: 8px; }
    .edit-form label { display: grid; gap: 4px; color: var(--muted); font-size: 12px; font-weight: 650; }
    .inline-edit { display: grid; gap: 8px; }
    .timeline { display: grid; gap: 8px; }
    .event {
      border-left: 3px solid var(--accent);
      padding: 6px 0 6px 10px;
      background: var(--panel-soft);
      border-radius: 0 6px 6px 0;
    }
    .activity { display: grid; gap: 6px; max-height: 220px; overflow: auto; }
    .activity-item {
      padding: 7px 9px;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: var(--panel-soft);
      color: var(--muted);
      font-size: 12px;
    }
    .empty {
      padding: 28px 16px;
      text-align: center;
      color: var(--muted);
      background: var(--panel-soft);
      border: 1px dashed var(--line-strong);
      border-radius: 7px;
      font-weight: 560;
    }
    pre {
      margin: 0;
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      background: var(--panel-soft);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 10px;
      max-height: 360px;
      overflow: auto;
    }
    .toast {
      min-height: 20px;
      color: var(--muted);
      font-size: 12px;
      font-weight: 560;
    }
    @media (max-width: 900px) {
      main { grid-template-columns: 1fr; padding: 10px; }
      .toolbar { grid-template-columns: 1fr 1fr; }
      .side { position: static; max-height: none; overflow: visible; }
    }
    @media (max-width: 560px) {
      header { height: auto; min-height: 58px; padding: 8px 12px; align-items: flex-start; }
      .brand { min-width: 0; }
      .subtitle { display: none; }
      .header-actions { flex-wrap: wrap; }
      .header-actions select { width: 100%; min-width: 0; flex: 1 1 100%; }
      .header-actions .lang-select { width: calc(50% - 5px); min-width: 0; max-width: none; flex: 1 1 calc(50% - 5px); }
      .toolbar, .split { grid-template-columns: 1fr; }
      button { width: 100%; }
      button.icon { width: 100%; flex-basis: auto; }
    }
  </style>
</head>
<body>
  <header>
    <div class="brand">
      <h1>dukememory</h1>
      <span class="subtitle" data-i18n="subtitle">локальная память проектов</span>
    </div>
    <div class="header-actions">
      <select id="project" title="Project"></select>
      <select id="lang" class="lang-select" title="Language">
        <option value="ru">RU</option>
        <option value="en">EN</option>
      </select>
      <span id="health" class="meta">проверка</span>
      <button class="icon" id="refresh" title="Refresh">↻</button>
    </div>
  </header>
  <main>
    <section>
      <div class="toolbar">
        <input id="q" data-i18n-placeholder="searchMemory" placeholder="Поиск памяти" autocomplete="off">
        <select id="status">
          <option value="active" data-label-key="statusActive">active</option>
          <option value="uncertain" data-label-key="statusUncertain">uncertain</option>
          <option value="superseded" data-label-key="statusSuperseded">superseded</option>
          <option value="rejected" data-label-key="statusRejected">rejected</option>
          <option value="all" data-label-key="allStatuses">all</option>
        </select>
        <select id="type">
          <option value="all" data-label-key="allTypes">all types</option>
          <option value="decision" data-type-label="decision">decision</option>
          <option value="constraint" data-type-label="constraint">constraint</option>
          <option value="task_state" data-type-label="task_state">task_state</option>
          <option value="known_issue" data-type-label="known_issue">known_issue</option>
          <option value="command" data-type-label="command">command</option>
          <option value="note" data-type-label="note">note</option>
        </select>
        <select id="usage">
          <option value="all" data-i18n="usageAll">all usage</option>
          <option value="hot" data-i18n="usageHot">hot</option>
          <option value="unused" data-i18n="usageUnused">unused</option>
          <option value="stale" data-i18n="usageStale">stale</option>
        </select>
        <select id="sort">
          <option value="updated_desc" data-i18n="sortUpdated">recent</option>
          <option value="request_count" data-i18n="sortRequests">requests</option>
          <option value="request_count_asc" data-i18n="sortUnused">least used</option>
        </select>
        <button id="search" class="primary" data-i18n="search">Search</button>
      </div>
      <div class="quick-filters">
        <button data-quick-type="decision" data-type-label="decision">decision</button>
        <button data-quick-type="known_issue" data-type-label="known_issue">known_issue</button>
        <button data-quick-type="command" data-type-label="command">command</button>
        <button data-quick-type="task_state" data-type-label="task_state">task_state</button>
      </div>
      <div id="bulkbar" class="bulkbar">
        <span id="bulkCount"></span>
        <button data-bulk="active" data-i18n="activeButton">Активно</button>
        <button data-bulk="uncertain" data-i18n="uncertainButton">Неуверенно</button>
        <button data-bulk="reject" class="danger" data-i18n="rejectButton">Отклонить</button>
        <button data-bulk="delete" class="danger" data-i18n="deleteButton">Удалить</button>
      </div>
      <div id="cards" class="grid"></div>
    </section>
    <aside class="side">
      <section class="panel">
        <h2 id="projectTitle">Проект</h2>
        <div id="projectSummary" class="stack"></div>
      </section>
      <section class="panel">
        <div class="tabs">
          <button class="tab active" data-tab="detail" data-i18n="tabDetail">Карточка</button>
          <button class="tab" data-tab="edit" data-i18n="tabEdit">Правка</button>
          <button class="tab" data-tab="inbox" data-i18n="tabInbox">Входящие</button>
          <button class="tab" data-tab="add" data-i18n="tabAdd">Добавить</button>
          <button class="tab" data-tab="autopilot" data-i18n="tabAutopilot">Автопилот</button>
          <button class="tab" data-tab="projects" data-i18n="tabProjects">Проекты</button>
          <button class="tab" data-tab="settings" data-i18n="settings">Настройки</button>
        </div>
        <div id="tab-detail" class="tab-panel active">
          <h2 data-i18n="evidence">Evidence</h2>
          <div id="detail"></div>
        </div>
        <div id="tab-edit" class="tab-panel">
          <h2 data-i18n="editMemory">Редактировать</h2>
          <div id="editEmpty" class="empty"></div>
          <div id="editForm" class="edit-form" style="display:none">
            <label><span>Title</span><input id="editTitle"></label>
            <label><span>Type</span><select id="editType">
              <option value="decision" data-type-label="decision">decision</option>
              <option value="constraint" data-type-label="constraint">constraint</option>
              <option value="task_state" data-type-label="task_state">task_state</option>
              <option value="known_issue" data-type-label="known_issue">known_issue</option>
              <option value="command" data-type-label="command">command</option>
              <option value="note" data-type-label="note">note</option>
              <option value="product_goal" data-type-label="product_goal">product_goal</option>
              <option value="design_note" data-type-label="design_note">design_note</option>
            </select></label>
            <div class="split">
              <label><span>Scope</span><input id="editScope"></label>
              <label><span>Status</span><select id="editStatus">
                <option value="active" data-label-key="statusActive">active</option>
                <option value="uncertain" data-label-key="statusUncertain">uncertain</option>
                <option value="superseded" data-label-key="statusSuperseded">superseded</option>
                <option value="rejected" data-label-key="statusRejected">rejected</option>
              </select></label>
            </div>
            <label><span>Body</span><textarea id="editBody"></textarea></label>
            <label><span>Source</span><input id="editSource"></label>
            <label><span>Confidence</span><input id="editConfidence" type="number" min="0" max="1" step="0.01"></label>
            <label><span>Links</span><textarea id="editLinks" placeholder="file:src/main.rs"></textarea></label>
            <button id="saveEdit" class="primary" data-i18n="save">Сохранить</button>
          </div>
        </div>
        <div id="tab-inbox" class="tab-panel">
          <h2 data-i18n="inbox">Inbox</h2>
          <div id="inbox" class="stack"></div>
        </div>
        <div id="tab-add" class="tab-panel">
          <h2 data-i18n="addMemory">Add Memory</h2>
          <div class="stack">
          <div class="split">
            <select id="newType">
              <option value="decision" data-type-label="decision">decision</option>
              <option value="constraint" data-type-label="constraint">constraint</option>
              <option value="task_state" data-type-label="task_state">task_state</option>
              <option value="known_issue" data-type-label="known_issue">known_issue</option>
              <option value="command" data-type-label="command">command</option>
              <option value="note" data-type-label="note">note</option>
            </select>
            <input id="newScope" value="project">
          </div>
          <textarea id="newText" data-i18n-placeholder="memoryBody" placeholder="Текст памяти"></textarea>
          <button id="add" class="primary" data-i18n="add">Add</button>
          </div>
        </div>
        <div id="tab-autopilot" class="tab-panel">
          <h2 data-i18n="tabAutopilot">Автопилот</h2>
          <div class="actions">
            <button id="autopilotRun" class="primary" data-i18n="runOnce">Run once</button>
            <button id="autopilotRepair" data-i18n="repair">Repair</button>
            <button id="autopilotExport" data-i18n="exportStatus">Export</button>
          </div>
          <div id="autopilotPanel" class="stack"></div>
          <h2 data-i18n="autonomous">Автономная память</h2>
          <div class="actions">
            <button id="autonomousRun" class="primary" data-i18n="autonomousRun">Autonomous run</button>
            <button id="autonomousRollback" class="danger" data-i18n="rollback">Rollback</button>
          </div>
          <div id="autonomousPanel" class="stack"></div>
        </div>
        <div id="tab-projects" class="tab-panel">
          <h2 data-i18n="tabProjects">Проекты</h2>
          <div id="projectsPanel" class="stack"></div>
        </div>
        <div id="tab-settings" class="tab-panel">
          <h2 data-i18n="settings">Настройки</h2>
          <div id="settingsPanel" class="stack"></div>
          <h2 data-i18n="activity">Активность</h2>
          <div id="activityPanel" class="activity"></div>
        </div>
      </section>
      <div id="toast" class="toast"></div>
    </aside>
  </main>
  <script>
    const i18n = {
      ru: {
        searchMemory: "Поиск памяти", subtitle: "локальная память проектов", search: "Найти",
        allStatuses: "все статусы", allTypes: "все типы", statusActive: "активные",
        statusUncertain: "неуверенные", statusSuperseded: "замененные", statusRejected: "отклоненные",
        addMemory: "Добавить память", editMemory: "Редактировать", memoryBody: "Текст памяти",
        add: "Добавить", save: "Сохранить", inbox: "Входящие", evidence: "Карточка",
        evidenceButton: "Открыть", activeButton: "Активно", uncertainButton: "Неуверенно",
        rejectButton: "Отклонить", deleteButton: "Удалить", approveButton: "Принять",
        noMemoryCards: "Карточек памяти нет", inboxEmpty: "Входящие пусты",
        selectMemoryCard: "Выберите карточку памяти.", selectForEdit: "Выберите карточку для редактирования.",
        checking: "проверка", offline: "нет связи", ready: "готово", added: "добавлено",
        saved: "сохранено", deleted: "удалено", selected: "выбрано", deleteConfirm: "Удалить память",
        tabDetail: "Карточка", tabEdit: "Правка", tabInbox: "Входящие", tabAdd: "Добавить",
        tabAutopilot: "Автопилот", tabProjects: "Проекты", settings: "Настройки",
        memories: "память", pending: "входящие", root: "папка", latestBackup: "backup",
        autopilotOk: "автопилот ok", autopilotWarn: "автопилот требует внимания",
        noViolations: "нарушений нет", audit: "аудит", links: "ссылки", source: "источник", requests: "запросы",
        runOnce: "Запустить", repair: "Починить", exportStatus: "Экспорт", activity: "Активность",
        usageAll: "все", usageHot: "частые", usageUnused: "без запросов", usageStale: "старые",
        sortUpdated: "свежие", sortRequests: "по запросам", sortUnused: "мало запросов",
        usefulness: "Полезность", embeddings: "Эмбеддинги", reindex: "Переиндексировать",
        autonomous: "Автономная память", autonomousRun: "Автоцикл", rollback: "Откатить",
        inlineEdit: "Быстрая правка", timeline: "История", savedInline: "быстро сохранено",
        typeLabels: { decision: "решение", constraint: "ограничение", task_state: "состояние задачи", known_issue: "известная проблема", command: "команда", note: "заметка", product_goal: "цель продукта", design_note: "дизайн-заметка", user_preference: "предпочтение", domain_fact: "факт" },
        statusLabels: { active: "активно", uncertain: "неуверенно", superseded: "заменено", rejected: "отклонено" }
      },
      en: {
        searchMemory: "Search memory", subtitle: "local project memory", search: "Search",
        allStatuses: "all statuses", allTypes: "all types", statusActive: "active",
        statusUncertain: "uncertain", statusSuperseded: "superseded", statusRejected: "rejected",
        addMemory: "Add Memory", editMemory: "Edit Memory", memoryBody: "Memory body",
        add: "Add", save: "Save", inbox: "Inbox", evidence: "Card", evidenceButton: "Open",
        activeButton: "Active", uncertainButton: "Uncertain", rejectButton: "Reject", deleteButton: "Delete",
        approveButton: "Approve", noMemoryCards: "No memory cards", inboxEmpty: "Inbox is empty",
        selectMemoryCard: "Select a memory card.", selectForEdit: "Select a memory card to edit.",
        checking: "checking", offline: "offline", ready: "ready", added: "added", saved: "saved",
        deleted: "deleted", selected: "selected", deleteConfirm: "Delete memory",
        tabDetail: "Card", tabEdit: "Edit", tabInbox: "Inbox", tabAdd: "Add",
        tabAutopilot: "Autopilot", tabProjects: "Projects", settings: "Settings",
        memories: "memories", pending: "pending", root: "root", latestBackup: "backup",
        autopilotOk: "autopilot ok", autopilotWarn: "autopilot needs attention",
        noViolations: "no violations", audit: "audit", links: "links", source: "source", requests: "requests",
        runOnce: "Run once", repair: "Repair", exportStatus: "Export", activity: "Activity",
        usageAll: "all", usageHot: "hot", usageUnused: "unused", usageStale: "stale",
        sortUpdated: "recent", sortRequests: "by requests", sortUnused: "least used",
        usefulness: "Usefulness", embeddings: "Embeddings", reindex: "Reindex",
        autonomous: "Autonomous memory", autonomousRun: "Autonomous run", rollback: "Rollback",
        inlineEdit: "Inline edit", timeline: "Timeline", savedInline: "inline saved",
        typeLabels: {}, statusLabels: {}
      }
    };
    const saved = JSON.parse(localStorage.getItem("dukememory_ui") || "{}");
    const savedLang = localStorage.getItem("dukememory_lang") || saved.lang;
    const state = {
      selected: null, evidence: null, memories: [], project: saved.project || null,
      projects: [], selectedIds: new Set(), tab: saved.tab || "detail",
      lang: savedLang === "en" ? "en" : "ru", activity: [], usefulness: null, embedding: null, autonomous: null,
      quality: null, profile: null, budget: null, dashboard: null, qa: null, contract: null
    };
    const $ = (id) => document.getElementById(id);
    const t = (key) => i18n[state.lang][key] || i18n.en[key] || key;
    const typeLabel = (value) => i18n[state.lang].typeLabels[value] || value;
    const statusLabel = (value) => i18n[state.lang].statusLabels[value] || value;

    function applyLanguage() {
      document.documentElement.lang = state.lang;
      $("lang").value = state.lang;
      document.querySelectorAll("[data-i18n]").forEach((node) => {
        node.textContent = t(node.dataset.i18n);
      });
      document.querySelectorAll("[data-i18n-placeholder]").forEach((node) => {
        node.placeholder = t(node.dataset.i18nPlaceholder);
      });
      document.querySelectorAll("[data-label-key]").forEach((node) => {
        node.textContent = t(node.dataset.labelKey);
      });
      document.querySelectorAll("[data-type-label]").forEach((node) => {
        node.textContent = typeLabel(node.dataset.typeLabel);
      });
      if (!state.selected) $("detail").innerHTML = `<div class="empty">${escapeHtml(t("selectMemoryCard"))}</div>`;
      renderProjectSummary();
      renderProjectsPanel();
      renderSettings();
      renderActivity();
      renderBulkbar();
    }

    function persistUi() {
      localStorage.setItem("dukememory_ui", JSON.stringify({
        lang: state.lang, project: state.project, tab: state.tab,
        q: $("q").value, status: $("status").value, type: $("type").value,
        usage: $("usage").value, sort: $("sort").value
      }));
    }

    async function api(path, options = {}) {
      const response = await fetch(path, {
        headers: { "Content-Type": "application/json" },
        ...options
      });
      const text = await response.text();
      let data = {};
      try { data = text ? JSON.parse(text) : {}; } catch (_) { data = { raw: text }; }
      if (!response.ok) {
        throw new Error(data.error?.message || response.statusText);
      }
      return data;
    }

    function setToast(text) {
      $("toast").textContent = text;
      state.activity.unshift(`${new Date().toLocaleTimeString()} ${text}`);
      state.activity = state.activity.slice(0, 20);
      renderActivity();
    }

    function pill(value, label = value) {
      return `<span class="pill ${escapeHtml(value)}">${escapeHtml(label)}</span>`;
    }

    function escapeHtml(value) {
      return String(value ?? "").replace(/[&<>"']/g, (char) => ({
        "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
      }[char]));
    }

    function highlight(value) {
      const text = escapeHtml(value);
      const query = $("q").value.trim();
      if (!query) return text;
      const terms = query.split(/\s+/).filter((term) => term.length > 1).map((term) => term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
      if (!terms.length) return text;
      return text.replace(new RegExp(`(${terms.join("|")})`, "gi"), "<mark>$1</mark>");
    }

    function memoryCard(row) {
      const selected = state.selected === row.id ? " selected" : "";
      const checked = state.selectedIds.has(row.id) ? " checked" : "";
      return `<article class="card${selected}" data-id="${escapeHtml(row.id)}">
        <div class="row">
          <input class="check" type="checkbox" data-select="${escapeHtml(row.id)}"${checked}>
          <span class="title">${highlight(row.title)}</span>
          ${pill(row.type, typeLabel(row.type))}
          ${pill(row.status, statusLabel(row.status))}
        </div>
        <div class="body">${highlight(row.body)}</div>
        <div class="row meta">
          <span>${escapeHtml(row.id)}</span>
          <span>scope=${escapeHtml(row.scope)}</span>
          <span>confidence=${Number(row.confidence).toFixed(2)}</span>
          <span>${escapeHtml(t("requests"))}=${Number(row.request_count || 0)}</span>
        </div>
        <div class="actions">
          <button data-action="evidence" data-id="${escapeHtml(row.id)}">${escapeHtml(t("evidenceButton"))}</button>
          <button data-action="edit" data-id="${escapeHtml(row.id)}">${escapeHtml(t("tabEdit"))}</button>
          <button data-action="active" data-id="${escapeHtml(row.id)}">${escapeHtml(t("activeButton"))}</button>
          <button data-action="uncertain" data-id="${escapeHtml(row.id)}">${escapeHtml(t("uncertainButton"))}</button>
          <button class="danger" data-action="reject" data-id="${escapeHtml(row.id)}">${escapeHtml(t("rejectButton"))}</button>
          <button class="danger" data-action="delete" data-id="${escapeHtml(row.id)}">${escapeHtml(t("deleteButton"))}</button>
        </div>
      </article>`;
    }

    function renderBulkbar() {
      const count = state.selectedIds.size;
      $("bulkbar").classList.toggle("visible", count > 0);
      $("bulkCount").textContent = `${count} ${t("selected")}`;
    }

    function inboxCard(item) {
      return `<article class="card">
        <div class="row">
          <span class="title">${escapeHtml(item.title)}</span>
          ${pill(item.type, typeLabel(item.type))}
          ${pill(item.status, statusLabel(item.status))}
        </div>
        <div class="body">${escapeHtml(item.body)}</div>
        <div class="row meta"><span>${escapeHtml(item.id)}</span><span>scope=${escapeHtml(item.scope)}</span></div>
        <div class="actions">
          <button data-inbox="approve" data-id="${escapeHtml(item.id)}" class="primary">${escapeHtml(t("approveButton"))}</button>
          <button data-inbox="reject" data-id="${escapeHtml(item.id)}" class="danger">${escapeHtml(t("rejectButton"))}</button>
        </div>
      </article>`;
    }

    async function loadHealth() {
      const data = await api("/health");
      $("health").textContent = data.ok ? `v${data.version}` : t("offline");
    }

    async function loadProjects() {
      const data = await api("/projects");
      state.projects = data.projects || [];
      const current = state.projects.find((project) => project.current) || state.projects[0];
      if (!state.project && current) state.project = current.key;
      if (!state.projects.some((project) => project.key === state.project) && current) {
        state.project = current.key;
      }
      $("project").innerHTML = state.projects.map((project) => {
        const selected = project.key === state.project ? " selected" : "";
        const label = `${project.name} (${project.memories}/${project.pending_inbox})`;
        return `<option value="${escapeHtml(project.key)}"${selected}>${escapeHtml(label)}</option>`;
      }).join("");
      renderProjectSummary();
      renderProjectsPanel();
    }

    function withProject(payload = {}) {
      return state.project ? { ...payload, project: state.project } : payload;
    }

    async function loadMemories() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      if ($("q").value.trim()) params.set("q", $("q").value.trim());
      params.set("status", $("status").value);
      params.set("type", $("type").value);
      params.set("usage", $("usage").value);
      params.set("sort", $("sort").value);
      params.set("limit", "100");
      const data = await api(`/memory?${params.toString()}`);
      state.memories = data.memories || [];
      for (const id of [...state.selectedIds]) {
        if (!state.memories.some((row) => row.id === id)) state.selectedIds.delete(id);
      }
      $("cards").innerHTML = state.memories.length
        ? state.memories.map(memoryCard).join("")
        : `<div class="empty">${escapeHtml(t("noMemoryCards"))}</div>`;
      renderBulkbar();
      renderProjectSummary();
      renderQuickFilters();
      if (state.tab === "settings") await loadUsefulness();
    }

    async function loadInbox() {
      const params = new URLSearchParams({ status: "pending", limit: "20" });
      if (state.project) params.set("project", state.project);
      const data = await api(`/inbox?${params.toString()}`);
      const items = data.items || [];
      $("inbox").innerHTML = items.length ? items.map(inboxCard).join("") : `<div class="empty">${escapeHtml(t("inboxEmpty"))}</div>`;
    }

    async function loadUsefulness() {
      const params = new URLSearchParams({ since_days: "30", stale_days: "30", hot_threshold: "3" });
      if (state.project) params.set("project", state.project);
      const data = await api(`/usefulness?${params.toString()}`);
      state.usefulness = data.usefulness;
      renderSettings();
    }

    async function loadQuality() {
      const params = new URLSearchParams({ since_days: "30", limit: "20" });
      if (state.project) params.set("project", state.project);
      const data = await api(`/quality?${params.toString()}`);
      state.quality = data.quality;
      renderSettings();
    }

    async function loadProjectProfile() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      const data = await api(`/project-profile?${params.toString()}`);
      state.profile = data.profile;
      renderSettings();
    }

    async function loadDashboard() {
      const data = await api("/dashboard");
      state.dashboard = data.dashboard;
      renderProjectsPanel();
    }

    async function loadBudgetPlan() {
      const params = new URLSearchParams({ task: $("q").value.trim() || "routine memory task" });
      if (state.project) params.set("project", state.project);
      const data = await api(`/budget-plan?${params.toString()}`);
      state.budget = data.budget;
      renderSettings();
    }

    async function loadEmbeddingStatus() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      const data = await api(`/embed-status?${params.toString()}`);
      state.embedding = data.embedding;
      renderSettings();
    }

    async function loadMemoryQa() {
      const params = new URLSearchParams({ since_days: "7" });
      if (state.project) params.set("project", state.project);
      const data = await api(`/memory-qa?${params.toString()}`);
      state.qa = data.qa;
      renderSettings();
      renderProjectsPanel();
    }

    async function loadMemoryContract() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      const data = await api(`/memory-contract?${params.toString()}`);
      state.contract = data.contract;
      renderSettings();
    }

    async function reindexEmbeddings() {
      const data = await api("/embed-index", { method: "POST", body: JSON.stringify(withProject({})) });
      state.embedding = data.embedding;
      setToast(`${t("reindex")}: ${data.embedding.indexed || 0}`);
      await Promise.all([loadEmbeddingStatus(), loadMemories()]);
    }

    async function runUpgradeDryRun() {
      const data = await api("/upgrade-project", { method: "POST", body: JSON.stringify(withProject({ dry_run: true })) });
      setToast(`upgrade: ${data.upgrade?.ok ? "ok" : "warn"}`);
      state.activity.unshift(...(data.upgrade?.actions || []).slice(0, 5));
      state.activity = state.activity.slice(0, 20);
      renderActivity();
      await Promise.all([loadMemoryQa(), loadMemoryContract(), loadDashboard()]);
    }

    async function loadEvidence(id) {
      state.selected = id;
      const data = await api("/evidence", { method: "POST", body: JSON.stringify(withProject({ id })) });
      state.evidence = data.evidence;
      state.evidence.request_count = data.request_count || 0;
      renderDetail(data.evidence);
      fillEditor(data.evidence.memory);
      await loadMemories();
    }

    function renderDetail(evidence) {
      const memory = evidence.memory;
      const links = memory.links || [];
      const events = evidence.audit_events || [];
      const requestCount = Number(evidence.request_count ?? memory.request_count ?? 0);
      $("detail").innerHTML = `<div class="stack">
        <div class="row">${pill(memory.type, typeLabel(memory.type))}${pill(memory.status, statusLabel(memory.status))}</div>
        <div class="title">${escapeHtml(memory.title)}</div>
        <div class="body">${escapeHtml(memory.body)}</div>
        <div class="inline-edit">
          <div class="title">${escapeHtml(t("inlineEdit"))}</div>
          <input id="inlineTitle" value="${escapeHtml(memory.title)}">
          <textarea id="inlineBody">${escapeHtml(memory.body)}</textarea>
          <div class="split">
            <select id="inlineStatus">
              <option value="active">${escapeHtml(t("statusActive"))}</option>
              <option value="uncertain">${escapeHtml(t("statusUncertain"))}</option>
              <option value="superseded">${escapeHtml(t("statusSuperseded"))}</option>
              <option value="rejected">${escapeHtml(t("statusRejected"))}</option>
            </select>
            <button id="inlineSave" class="primary">${escapeHtml(t("save"))}</button>
          </div>
        </div>
        <div class="kv">
          <b>ID</b><span>${escapeHtml(memory.id)}</span>
          <b>Scope</b><span>${escapeHtml(memory.scope)}</span>
          <b>${escapeHtml(t("source"))}</b><span>${escapeHtml(memory.source || "-")}</span>
          <b>Confidence</b><span>${Number(memory.confidence).toFixed(2)}</span>
          <b>${escapeHtml(t("requests"))}</b><span>${requestCount}</span>
          <b>${escapeHtml(t("links"))}</b><span>${links.length ? links.map((link) => `${escapeHtml(link.kind)}:${escapeHtml(link.target)}`).join("<br>") : "-"}</span>
        </div>
        <div class="title">${escapeHtml(t("timeline"))}</div>
        <div class="timeline">${events.length ? events.map((event) => `<div class="event"><b>${escapeHtml(event.event_type)}</b><div class="meta">${new Date(event.created_at).toLocaleString()}</div><div>${escapeHtml(event.detail)}</div></div>`).join("") : `<div class="empty">${escapeHtml(t("audit"))}: -</div>`}</div>
      </div>`;
      $("inlineStatus").value = memory.status || "active";
      $("inlineSave").addEventListener("click", saveInline);
    }

    async function saveInline() {
      if (!state.evidence) return;
      const memory = state.evidence.memory;
      await api("/memory/update", { method: "POST", body: JSON.stringify(withProject({
        id: memory.id,
        title: $("inlineTitle").value,
        body: $("inlineBody").value,
        status: $("inlineStatus").value,
        type: memory.type,
        scope: memory.scope,
        source: memory.source || "",
        confidence: Number(memory.confidence ?? 0.8),
        links: (memory.links || []).map((link) => `${link.kind}:${link.target}`),
        replace_links: true
      }))});
      setToast(t("savedInline"));
      await loadEvidence(memory.id);
    }

    function fillEditor(memory) {
      $("editEmpty").style.display = "none";
      $("editForm").style.display = "grid";
      $("editTitle").value = memory.title || "";
      $("editType").value = memory.type || "note";
      $("editScope").value = memory.scope || "project";
      $("editStatus").value = memory.status || "active";
      $("editBody").value = memory.body || "";
      $("editSource").value = memory.source || "";
      $("editConfidence").value = Number(memory.confidence ?? 0.8).toFixed(2);
      $("editLinks").value = (memory.links || []).map((link) => `${link.kind}:${link.target}`).join("\n");
    }

    function clearDetail() {
      state.selected = null;
      state.evidence = null;
      $("detail").innerHTML = `<div class="empty">${escapeHtml(t("selectMemoryCard"))}</div>`;
      $("editEmpty").style.display = "block";
      $("editEmpty").textContent = t("selectForEdit");
      $("editForm").style.display = "none";
    }

    async function setStatus(id, status) {
      await api("/memory/status", { method: "POST", body: JSON.stringify(withProject({ id, status })) });
      setToast(`${id} -> ${status}`);
      await loadMemories();
      if (state.selected === id) await loadEvidence(id);
    }

    async function deleteMemory(id) {
      if (!confirm(`${t("deleteConfirm")} ${id}?`)) return;
      await api("/memory/delete", { method: "POST", body: JSON.stringify(withProject({ id })) });
      setToast(`${id} ${t("deleted")}`);
      if (state.selected === id) {
        state.selected = null;
        clearDetail();
      }
      await loadMemories();
    }

    async function saveEdit() {
      if (!state.selected) return;
      const links = $("editLinks").value.split("\n").map((line) => line.trim()).filter(Boolean);
      await api("/memory/update", { method: "POST", body: JSON.stringify(withProject({
        id: state.selected, title: $("editTitle").value, type: $("editType").value,
        scope: $("editScope").value, status: $("editStatus").value, body: $("editBody").value,
        source: $("editSource").value, confidence: Number($("editConfidence").value), links, replace_links: true
      }))});
      setToast(t("saved"));
      await loadEvidence(state.selected);
    }

    async function bulkAction(action) {
      const ids = [...state.selectedIds];
      if (!ids.length) return;
      if (action === "delete" && !confirm(`${t("deleteConfirm")} ${ids.length}?`)) return;
      await api("/memory/bulk", { method: "POST", body: JSON.stringify(withProject({ ids, action })) });
      state.selectedIds.clear();
      if (ids.includes(state.selected)) clearDetail();
      await loadMemories();
    }

    async function addMemory() {
      const text = $("newText").value.trim();
      if (!text) return;
      const data = await api("/remember", {
        method: "POST",
        body: JSON.stringify(withProject({ text, type: $("newType").value, scope: $("newScope").value || "project" }))
      });
      $("newText").value = "";
      setToast(`${t("added")} ${data.id}`);
      await loadMemories();
    }

    async function inboxAction(id, action) {
      const path = action === "approve" ? "/inbox/approve" : "/inbox/reject";
      await api(path, { method: "POST", body: JSON.stringify(withProject({ id })) });
      setToast(`${action} ${id}`);
      await Promise.all([loadInbox(), loadMemories()]);
    }

    function renderProjectSummary() {
      const project = state.projects.find((item) => item.key === state.project);
      const active = state.memories.filter((row) => row.status === "active").length;
      const decisions = state.memories.filter((row) => row.type === "decision").length;
      $("projectTitle").textContent = project ? project.name : "Project";
      $("projectSummary").innerHTML = project ? `<div class="stat-grid">
        <div class="stat"><strong>${project.memories}</strong><span>${escapeHtml(t("memories"))}</span></div>
        <div class="stat"><strong>${project.pending_inbox}</strong><span>${escapeHtml(t("pending"))}</span></div>
        <div class="stat"><strong>${active}</strong><span>${escapeHtml(t("statusActive"))}</span></div>
        <div class="stat"><strong>${decisions}</strong><span>${escapeHtml(typeLabel("decision"))}</span></div>
        <div class="stat"><strong>${project.current ? "●" : "○"}</strong><span>current</span></div>
      </div><div class="meta">${escapeHtml(t("root"))}: ${escapeHtml(project.root)}</div>` : "";
    }

    function renderQuickFilters() {
      document.querySelectorAll("[data-quick-type]").forEach((button) => {
        button.classList.toggle("active", $("type").value === button.dataset.quickType);
      });
    }

    function renderProjectsPanel() {
      const dashboardProjects = state.dashboard?.projects || [];
      $("projectsPanel").innerHTML = state.projects.map((project) => {
        const dash = dashboardProjects.find((item) => item.name === project.name || item.db === project.db) || {};
        const qaScore = project.key === state.project && state.qa ? Number(state.qa.score || 0).toFixed(1) : "-";
        return `<article class="card">
        <div class="row"><span class="title">${escapeHtml(project.name)}</span>${project.current ? pill("active", "current") : ""}</div>
        <div class="kv"><b>${escapeHtml(t("memories"))}</b><span>${project.memories}</span><b>${escapeHtml(t("pending"))}</b><span>${project.pending_inbox}</span><b>quality</b><span>${dash.quality_average == null ? "-" : Number(dash.quality_average).toFixed(1)}</span><b>QA</b><span>${qaScore}</span><b>autonomous</b><span>${dash.autonomous_ok == null ? "-" : String(dash.autonomous_ok)}</span><b>${escapeHtml(t("root"))}</b><span>${escapeHtml(project.root)}</span></div>
        <button data-project-open="${escapeHtml(project.key)}" class="primary">${escapeHtml(t("evidenceButton"))}</button>
      </article>`;
      }).join("");
    }

    function renderSettings() {
      const usefulness = state.usefulness || {};
      const embedding = state.embedding || {};
      const quality = state.quality || {};
      const profile = state.profile || {};
      const budget = state.budget || {};
      const qa = state.qa || {};
      const contract = state.contract || {};
      $("settingsPanel").innerHTML = `<div class="kv">
        <b>Language</b><span>${escapeHtml(state.lang.toUpperCase())}</span>
        <b>Project</b><span>${escapeHtml(state.project || "-")}</span>
        <b>Status</b><span>${escapeHtml($("status").value)}</span>
        <b>Type</b><span>${escapeHtml($("type").value)}</span>
        <b>Usage</b><span>${escapeHtml($("usage").value)}</span>
        <b>Query</b><span>${escapeHtml($("q").value || "-")}</span>
        <b>Budget</b><span>${escapeHtml(budget.profile || "-")} / ${Number(budget.max_chars || 0)}</span>
      </div>
      <h2>Memory QA</h2>
      <div class="stat-grid">
        <div class="stat"><strong>${Number(qa.score || 0).toFixed(1)}</strong><span>score</span></div>
        <div class="stat"><strong>${Number(qa.reads || 0)}</strong><span>reads</span></div>
        <div class="stat"><strong>${Number((qa.semantic_read_rate || 0) * 100).toFixed(0)}%</strong><span>semantic</span></div>
        <div class="stat"><strong>${Number(qa.token_saving_estimate || 0)}</strong><span>tokens saved</span></div>
      </div>
      <div class="card"><div class="title">issues</div><div class="body">${qa.issues?.length ? qa.issues.map(escapeHtml).join("<br>") : "-"}</div></div>
      <div class="card"><div class="title">contract</div><div class="body">${escapeHtml(contract.path || "-")}</div></div>
      <h2>${escapeHtml(t("usefulness"))}</h2>
      <div class="stat-grid">
        <div class="stat"><strong>${usefulness.hot?.length ?? 0}</strong><span>${escapeHtml(t("usageHot"))}</span></div>
        <div class="stat"><strong>${usefulness.unused?.length ?? 0}</strong><span>${escapeHtml(t("usageUnused"))}</span></div>
        <div class="stat"><strong>${usefulness.stale?.length ?? 0}</strong><span>${escapeHtml(t("usageStale"))}</span></div>
      </div>
      <h2>Quality</h2>
      <div class="stat-grid">
        <div class="stat"><strong>${Number(quality.average_score || 0).toFixed(1)}</strong><span>avg</span></div>
        <div class="stat"><strong>${Number(quality.total || 0)}</strong><span>cards</span></div>
        <div class="stat"><strong>${quality.weakest?.length ?? 0}</strong><span>weakest</span></div>
      </div>
      <div class="card"><div class="title">weakest memory</div><div class="body">${quality.weakest?.length ? quality.weakest.slice(0, 5).map((item) => `${Number(item.score || 0).toFixed(1)} ${escapeHtml(item.id)} ${escapeHtml(item.title)}`).join("<br>") : "-"}</div></div>
      <h2>Project profile</h2>
      <div class="kv">
        <b>active</b><span>${escapeHtml(profile.active_profile || "-")}</span>
        <b>decisions</b><span>${Number(profile.decisions || 0)}</span>
        <b>constraints</b><span>${Number(profile.constraints || 0)}</span>
        <b>commands</b><span>${Number(profile.commands || 0)}</span>
        <b>recommended</b><span>${escapeHtml(profile.recommended_budget || "-")}</span>
      </div>
      <h2>${escapeHtml(t("embeddings"))}</h2>
      <div class="kv">
        <b>model</b><span>${escapeHtml(embedding.model || "-")}</span>
        <b>indexed</b><span>${Number(embedding.indexed || 0)}</span>
        <b>missing</b><span>${Number(embedding.missing || 0)}</span>
        <b>stale</b><span>${Number(embedding.stale || 0)}</span>
      </div>
      <button id="reindexEmbeddings" class="primary">${escapeHtml(t("reindex"))}</button>
      <button id="upgradeProject">Upgrade dry-run</button>
      <button id="resetUi" class="danger">Reset UI state</button>`;
      const reindex = $("reindexEmbeddings");
      if (reindex) reindex.addEventListener("click", reindexEmbeddings);
      const upgrade = $("upgradeProject");
      if (upgrade) upgrade.addEventListener("click", runUpgradeDryRun);
      const reset = $("resetUi");
      if (reset) reset.addEventListener("click", async () => {
        localStorage.removeItem("dukememory_ui");
        localStorage.removeItem("dukememory_lang");
        state.lang = "ru";
        state.tab = "detail";
        $("q").value = "";
        $("status").value = "active";
        $("type").value = "all";
        $("usage").value = "all";
        $("sort").value = "updated_desc";
        applyLanguage();
        setTab("detail");
        await refreshAll();
      });
    }

    function renderActivity() {
      const panel = $("activityPanel");
      if (!panel) return;
      panel.innerHTML = state.activity.length
        ? state.activity.map((item) => `<div class="activity-item">${escapeHtml(item)}</div>`).join("")
        : `<div class="empty">${escapeHtml(t("ready"))}</div>`;
    }

    async function loadAutopilot() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      const data = await api(`/autopilot/ui?${params.toString()}`);
      const alert = data.alert || {};
      const report = data.report || {};
      $("autopilotPanel").innerHTML = `<div class="stat-grid">
        <div class="stat"><strong>${escapeHtml(alert.level || "ok")}</strong><span>${escapeHtml(alert.ok ? t("autopilotOk") : t("autopilotWarn"))}</span></div>
        <div class="stat"><strong>${report.failed_ticks ?? 0}</strong><span>failed</span></div>
        <div class="stat"><strong>${report.embeddings_stale ?? 0}</strong><span>stale</span></div>
      </div>
      <div class="card"><div class="title">${escapeHtml(t("latestBackup"))}</div><div class="body">${escapeHtml(report.latest_backup || "-")}</div></div>
      <div class="card"><div class="title">violations</div><div class="body">${(alert.violations || []).length ? (alert.violations || []).map(escapeHtml).join("<br>") : escapeHtml(t("noViolations"))}</div></div>`;
    }

    async function loadAutonomous() {
      const params = new URLSearchParams();
      if (state.project) params.set("project", state.project);
      const data = await api(`/autonomous/status?${params.toString()}`);
      state.autonomous = data.report || null;
      const report = state.autonomous || {};
      const actions = report.actions || [];
      const policy = report.policy || [];
      $("autonomousPanel").innerHTML = `<div class="stat-grid">
        <div class="stat"><strong>${escapeHtml(report.ok === false ? "warn" : report.ok === true ? "ok" : "-")}</strong><span>status</span></div>
        <div class="stat"><strong>${escapeHtml(report.level || "-")}</strong><span>level</span></div>
        <div class="stat"><strong>${actions.length}</strong><span>actions</span></div>
        <div class="stat"><strong>${policy.filter((item) => item.allowed).length}/${policy.length}</strong><span>policy</span></div>
      </div>
      <div class="card"><div class="title">status file</div><div class="body">${escapeHtml(data.status_file || "-")}</div></div>
      <div class="card"><div class="title">latest actions</div><div class="body">${actions.length ? actions.slice(0, 8).map((item) => `${escapeHtml(item.status)} ${escapeHtml(item.kind)} ${escapeHtml(item.detail)}`).join("<br>") : "-"}</div></div>
      <div class="card"><div class="title">policy decisions</div><div class="body">${policy.length ? policy.slice(0, 8).map((item) => `${item.allowed ? "allow" : "skip"} ${escapeHtml(item.action)} risk=${Number(item.risk_score || 0).toFixed(0)} ${escapeHtml(item.reason)}`).join("<br>") : "-"}</div></div>`;
    }

    async function autopilotAction(action) {
      if (action !== "export-status" && !confirm(action)) return;
      const data = await api(`/autopilot/${action}`, { method: "POST", body: JSON.stringify(withProject({})) });
      setToast(data.output || data.ok || action);
      await loadAutopilot();
      await loadProjects();
    }

    async function autonomousAction(action) {
      if (!confirm(action)) return;
      const body = action === "run-once"
        ? withProject({ level: "normal", provider: "mock", endpoint: "local", model: "mock-small" })
        : withProject({});
      const data = await api(`/autonomous/${action}`, { method: "POST", body: JSON.stringify(body) });
      setToast(`${t("autonomous")}: ${data.report?.actions?.length ?? 0}`);
      await Promise.all([loadAutonomous(), loadMemories(), loadProjects()]);
    }

    async function refreshAll() {
      try {
        await loadHealth();
        await loadProjects();
        restoreFilters();
        clearDetail();
        await Promise.all([loadMemories(), loadInbox(), loadAutopilot(), loadAutonomous(), loadUsefulness(), loadQuality(), loadProjectProfile(), loadBudgetPlan(), loadDashboard(), loadEmbeddingStatus(), loadMemoryQa(), loadMemoryContract()]);
        setToast(t("ready"));
      } catch (error) {
        setToast(error.message);
      }
    }

    $("refresh").addEventListener("click", refreshAll);
    $("lang").addEventListener("change", async () => {
      state.lang = $("lang").value === "en" ? "en" : "ru";
      localStorage.setItem("dukememory_lang", state.lang);
      applyLanguage();
      persistUi();
      await Promise.all([loadMemories(), loadInbox(), loadUsefulness(), loadQuality(), loadProjectProfile(), loadBudgetPlan(), loadDashboard(), loadEmbeddingStatus(), loadMemoryQa(), loadMemoryContract(), loadAutonomous()]);
    });
    $("project").addEventListener("change", async () => {
      state.project = $("project").value;
      state.selectedIds.clear();
      clearDetail();
      persistUi();
      await Promise.all([loadMemories(), loadInbox(), loadAutopilot(), loadAutonomous(), loadUsefulness(), loadQuality(), loadProjectProfile(), loadBudgetPlan(), loadDashboard(), loadEmbeddingStatus(), loadMemoryQa(), loadMemoryContract()]);
    });
    $("search").addEventListener("click", () => { persistUi(); loadMemories(); });
    $("q").addEventListener("keydown", (event) => { if (event.key === "Enter") { persistUi(); loadMemories(); } });
    $("status").addEventListener("change", () => { persistUi(); loadMemories(); });
    $("type").addEventListener("change", () => { persistUi(); loadMemories(); });
    $("usage").addEventListener("change", () => { persistUi(); loadMemories(); });
    $("sort").addEventListener("change", () => { persistUi(); loadMemories(); });
    $("add").addEventListener("click", addMemory);
    $("saveEdit").addEventListener("click", saveEdit);
    $("autopilotRun").addEventListener("click", () => autopilotAction("run-once"));
    $("autopilotRepair").addEventListener("click", () => autopilotAction("repair"));
    $("autopilotExport").addEventListener("click", () => autopilotAction("export-status"));
    $("autonomousRun").addEventListener("click", () => autonomousAction("run-once"));
    $("autonomousRollback").addEventListener("click", () => autonomousAction("rollback"));
    document.querySelectorAll("[data-quick-type]").forEach((button) => button.addEventListener("click", () => {
      $("type").value = $("type").value === button.dataset.quickType ? "all" : button.dataset.quickType;
      persistUi();
      loadMemories();
    }));
    document.querySelectorAll("[data-tab]").forEach((button) => button.addEventListener("click", () => setTab(button.dataset.tab)));
    $("bulkbar").addEventListener("click", (event) => {
      const button = event.target.closest("button[data-bulk]");
      if (button) bulkAction(button.dataset.bulk);
    });
    $("cards").addEventListener("click", async (event) => {
      const checkbox = event.target.closest("input[data-select]");
      if (checkbox) {
        checkbox.checked ? state.selectedIds.add(checkbox.dataset.select) : state.selectedIds.delete(checkbox.dataset.select);
        renderBulkbar();
        return;
      }
      const button = event.target.closest("button");
      if (!button) return;
      const id = button.dataset.id;
      const action = button.dataset.action;
      if (action === "evidence") return loadEvidence(id);
      if (action === "edit") { await loadEvidence(id); return setTab("edit"); }
      if (action === "active") return setStatus(id, "active");
      if (action === "uncertain") return setStatus(id, "uncertain");
      if (action === "reject") return setStatus(id, "rejected");
      if (action === "delete") return deleteMemory(id);
    });
    $("inbox").addEventListener("click", (event) => {
      const button = event.target.closest("button");
      if (button) inboxAction(button.dataset.id, button.dataset.inbox);
    });
    $("projectsPanel").addEventListener("click", async (event) => {
      const button = event.target.closest("button[data-project-open]");
      if (!button) return;
      state.project = button.dataset.projectOpen;
      $("project").value = state.project;
      clearDetail();
      persistUi();
      await Promise.all([loadMemories(), loadInbox(), loadAutopilot(), loadAutonomous(), loadUsefulness(), loadQuality(), loadProjectProfile(), loadBudgetPlan(), loadDashboard(), loadEmbeddingStatus(), loadMemoryQa(), loadMemoryContract()]);
      setTab("detail");
    });

    function setTab(tab) {
      state.tab = tab;
      document.querySelectorAll(".tab").forEach((item) => item.classList.toggle("active", item.dataset.tab === tab));
      document.querySelectorAll(".tab-panel").forEach((item) => item.classList.toggle("active", item.id === `tab-${tab}`));
      persistUi();
      if (tab === "autopilot") loadAutopilot();
      if (tab === "autopilot") loadAutonomous();
      if (tab === "settings") renderSettings();
    }

    function restoreFilters() {
      if (saved.q) $("q").value = saved.q;
      if (saved.status) $("status").value = saved.status;
      if (saved.type) $("type").value = saved.type;
      if (saved.usage) $("usage").value = saved.usage;
      if (saved.sort) $("sort").value = saved.sort;
      setTab(state.tab);
    }

    applyLanguage();
    refreshAll();
  </script>
</body>
</html>"##
}

fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            bail!("empty or incomplete HTTP request");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_header_end(&buffer) {
            break pos;
        }
        if buffer.len() > 1024 * 1024 {
            bail!("HTTP request headers are too large");
        }
    };
    let content_length = content_length(&buffer[..header_end.saturating_sub(4)])?;
    let target_len = header_end + content_length;
    while buffer.len() < target_len {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            bail!("HTTP request body ended before Content-Length");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > 16 * 1024 * 1024 {
            bail!("HTTP request body is too large");
        }
    }
    buffer.truncate(target_len);
    Ok(buffer)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

fn content_length(header: &[u8]) -> Result<usize> {
    let header = std::str::from_utf8(header).context("HTTP headers must be UTF-8")?;
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            return value
                .trim()
                .parse::<usize>()
                .context("invalid Content-Length header");
        }
    }
    Ok(0)
}

fn http_snapshot(conn: &Connection) -> Result<Value> {
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        30,
    )?;
    Ok(json!({"memories": rows}))
}

fn http_metrics(conn: &Connection) -> Result<Value> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    let events: i64 = conn.query_row("SELECT COUNT(*) FROM memory_events", [], |row| row.get(0))?;
    Ok(json!({
        "memories": total,
        "pending_inbox": pending,
        "events": events,
        "schema": schema_version(conn)?
    }))
}

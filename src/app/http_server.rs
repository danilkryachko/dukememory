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
            let mut fetch_limit = if usage != "all" || sort != "updated_desc" {
                500
            } else {
                limit
            };
            if q.is_some() {
                fetch_limit = fetch_limit.saturating_mul(2).min(500).max(fetch_limit);
            }
            let rows = query_memories(&conn, q, &types, &statuses, scope, fetch_limit)?;
            let rows = if let Some(query) = q {
                let quality_signals = retrieval_feedback_signals(&conn, 30).unwrap_or_default();
                filter_query_useless_memories(rows, query, &quality_signals)
            } else {
                rows
            };
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
        ("GET", "/dashboard-repair-history") => {
            let params = parse_query(query);
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(30);
            let limit = params
                .get("limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(20);
            HttpResponse::ok(json!({"history": dashboard_repair_history_report(
                db,
                since_days,
                limit,
                params.get("project").map(String::as_str),
            )?}))
        }
        ("GET", "/dashboard-repair") => HttpResponse::ok(json!({"repair": dashboard_repair_report(
            db,
            false,
            parse_query(query).get("project").map(String::as_str),
            DEFAULT_EMBED_PROVIDER,
            DEFAULT_EMBED_ENDPOINT,
            DEFAULT_EMBED_MODEL,
            "http",
        )?})),
        ("POST", "/dashboard-repair") => {
            let value = parse_json_body(body)?;
            let apply = value.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let selected = value.get("project").and_then(Value::as_str);
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
            HttpResponse::ok(json!({"repair": dashboard_repair_report(
                db,
                apply,
                selected,
                provider,
                endpoint,
                model,
                "http",
            )?}))
        }
        ("GET", "/ops-status") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(db, selected)?;
            let conn = open_db(&ctx.db)?;
            let since_days = params
                .get("since_days")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(7);
            HttpResponse::ok(json!({"ops": ops_status_report(
                &conn,
                &ctx.db,
                &ctx.root,
                since_days,
            )?}))
        }
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
                    rollback_keep: 10,
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
            HttpResponse::ok(
                json!({"context": render_context_pack_for_task(&conn, &rows, 5000, task)?}),
            )
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
            let report = impact_report(
                &conn,
                &ImpactRequest {
                    target,
                    limit,
                    budget,
                    scope,
                    provider,
                    endpoint,
                    model,
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
            let limit = value
                .get("limit")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(10)
                .min(100);
            let fetch_limit = limit.saturating_mul(2).max(limit).min(200);
            let rows = query_memories(
                &conn,
                Some(query),
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                fetch_limit,
            )?;
            let quality_signals = retrieval_feedback_signals(&conn, 30).unwrap_or_default();
            let mut rows = filter_query_useless_memories(rows, query, &quality_signals);
            rows.truncate(limit);
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
    include_str!("memory_ui.html")
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

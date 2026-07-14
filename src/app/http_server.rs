use super::*;

#[path = "http_routes.rs"]
mod routes;
#[path = "http_security.rs"]
mod security;

const HTTP_WORKERS: usize = 4;
const HTTP_QUEUE_CAPACITY: usize = 64;
const HTTP_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

struct HttpAppState {
    default_db: PathBuf,
    auth_token: Option<String>,
}

pub(crate) fn serve_http(
    db: &Path,
    host: &str,
    port: u16,
    once: bool,
    auth_token: Option<&str>,
) -> Result<()> {
    let auth_token = auth_token.map(str::trim).filter(|token| !token.is_empty());
    if !is_loopback_host(host) && auth_token.is_none() {
        bail!(
            "external HTTP binds require --auth-token or DUKEMEMORY_HTTP_TOKEN; use 127.0.0.1 for local-only access"
        );
    }
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind http server on {host}:{port}"))?;
    let addr = listener.local_addr()?;
    println!("http://{addr}");
    let state = std::sync::Arc::new(HttpAppState {
        default_db: db.to_path_buf(),
        auth_token: auth_token.map(ToOwned::to_owned),
    });

    if once {
        let (stream, _) = listener.accept()?;
        return handle_http_stream(&state, stream);
    }

    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_shutdown = std::sync::Arc::clone(&shutdown);
    ctrlc::set_handler(move || {
        signal_shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .with_context(|| "failed to install HTTP shutdown signal handler")?;
    listener.set_nonblocking(true)?;

    let (sender, receiver) = std::sync::mpsc::sync_channel::<TcpStream>(HTTP_QUEUE_CAPACITY);
    let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
    let mut workers = Vec::with_capacity(HTTP_WORKERS);
    for worker_index in 0..HTTP_WORKERS {
        let receiver = std::sync::Arc::clone(&receiver);
        let state = std::sync::Arc::clone(&state);
        let worker = std::thread::Builder::new()
            .name(format!("dukememory-http-{worker_index}"))
            .stack_size(HTTP_WORKER_STACK_BYTES)
            .spawn(move || {
                loop {
                    let stream = match receiver.lock() {
                        Ok(receiver) => receiver.recv(),
                        Err(_) => return,
                    };
                    let Ok(stream) = stream else {
                        return;
                    };
                    if let Err(err) = handle_http_stream(&state, stream) {
                        eprintln!("HTTP request failed: {err:#}");
                    }
                }
            })
            .with_context(|| format!("failed to start HTTP worker {worker_index}"))?;
        workers.push(worker);
    }
    while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => sender
                .send(stream)
                .with_context(|| "HTTP worker queue stopped")?,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(err) => return Err(err).with_context(|| "HTTP listener failed"),
        }
    }
    drop(sender);
    for worker in workers {
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("HTTP worker panicked during shutdown"))?;
    }
    Ok(())
}

pub(crate) fn resolve_http_auth_token(
    inline_token: Option<&str>,
    token_file: Option<&Path>,
) -> Result<Option<String>> {
    security::resolve_auth_token(inline_token, token_file)
}

fn handle_http_stream(state: &HttpAppState, mut stream: TcpStream) -> Result<()> {
    let started = std::time::Instant::now();
    let peer = stream
        .peer_addr()
        .map(|address| address.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let response = match routes::handle_http_request(
        &state.default_db,
        &mut stream,
        state.auth_token.as_deref(),
    ) {
        Ok(response) => response,
        Err(err) => HttpResponse::from_error(&err),
    };
    let status = response.status;
    crate::http_api::write_response(&mut stream, response)?;
    eprintln!(
        "{}",
        json!({
            "event": "http_access",
            "peer": peer,
            "status": status,
            "elapsed_ms": started.elapsed().as_millis(),
        })
    );
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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

fn parse_sync_profile(value: Option<&str>) -> SyncProfileMode {
    match value.unwrap_or("local_first_backup") {
        "local-only" | "local_only" => SyncProfileMode::LocalOnly,
        "local-first-sync" | "local_first_sync" => SyncProfileMode::LocalFirstSync,
        "remote-shared" | "remote_shared" => SyncProfileMode::RemoteShared,
        _ => SyncProfileMode::LocalFirstBackup,
    }
}

fn parse_ranking_profile(value: Option<&str>) -> RankingProfileMode {
    match value.unwrap_or("balanced") {
        "strict" => RankingProfileMode::Strict,
        "recall-heavy" | "recall_heavy" => RankingProfileMode::RecallHeavy,
        "precision-heavy" | "precision_heavy" => RankingProfileMode::PrecisionHeavy,
        _ => RankingProfileMode::Balanced,
    }
}

fn parse_project_template(value: Option<&str>) -> ProjectTemplateKind {
    match value.unwrap_or("rust-cli") {
        "frontend-app" | "frontend_app" => ProjectTemplateKind::FrontendApp,
        "game-mod" | "game_mod" => ProjectTemplateKind::GameMod,
        "electronics-cad" | "electronics_cad" => ProjectTemplateKind::ElectronicsCad,
        "docs-research" | "docs_research" => ProjectTemplateKind::DocsResearch,
        _ => ProjectTemplateKind::RustCli,
    }
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

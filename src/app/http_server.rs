use super::*;
use dukememory::protocol::http_content_length as content_length;

#[path = "http_ingest_routes.rs"]
mod ingest_routes;
#[path = "http_authorization.rs"]
mod route_auth;
#[path = "http_routes.rs"]
mod routes;
#[path = "http_security.rs"]
mod security;

const HTTP_WORKERS: usize = 4;
const HTTP_QUEUE_CAPACITY: usize = 64;
const HTTP_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;
const HTTP_MAX_HEADER_BYTES: usize = 64 * 1024;
const HTTP_MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const HTTP_REQUEST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);
const HTTP_IO_SLICE: std::time::Duration = std::time::Duration::from_secs(2);

struct HttpAppState {
    default_db: PathBuf,
    auth_policy: security::HttpAuthPolicy,
    security_policy: security::HttpSecurityPolicy,
    rate_limiter: security::HttpRateLimiter,
    concurrency_limiter: security::HttpConcurrencyLimiter,
    mcp_http: mcp_server::McpHttpService,
    otlp: Option<otlp::OtlpExporter>,
    telemetry_identifiers: TelemetryIdentifierPolicy,
}

#[derive(Default)]
struct HttpRequestMeta {
    method: String,
    path: String,
    client: String,
}

const TELEMETRY_HASH_KEY_FILE_ENV: &str = "DUKEMEMORY_TELEMETRY_HASH_KEY_FILE";

enum TelemetryIdentifierPolicy {
    Plain,
    Hash(Vec<u8>),
    Omit,
}

impl TelemetryIdentifierPolicy {
    fn from_environment() -> Result<Self> {
        match std::env::var("DUKEMEMORY_TELEMETRY_IDENTIFIERS")
            .unwrap_or_else(|_| "plain".to_string())
            .trim()
        {
            "plain" => Ok(Self::Plain),
            "hash" => {
                let path = std::env::var_os(TELEMETRY_HASH_KEY_FILE_ENV)
                    .map(PathBuf::from)
                    .with_context(|| {
                        format!(
                            "DUKEMEMORY_TELEMETRY_IDENTIFIERS=hash requires {TELEMETRY_HASH_KEY_FILE_ENV}"
                        )
                    })?;
                let key = security::read_private_token_file(&path)?;
                if key.len() < 16 {
                    bail!("{TELEMETRY_HASH_KEY_FILE_ENV} must contain at least 16 bytes");
                }
                Ok(Self::Hash(key.into_bytes()))
            }
            "omit" => Ok(Self::Omit),
            _ => bail!("DUKEMEMORY_TELEMETRY_IDENTIFIERS must be plain, hash, or omit"),
        }
    }

    fn protect(&self, value: &str) -> String {
        match self {
            Self::Plain => value.to_string(),
            Self::Hash(key) => {
                let digest = keyed_identifier_digest(key, value.as_bytes());
                format!("hmac-sha256:{}", &digest[..24])
            }
            Self::Omit => "redacted".to_string(),
        }
    }
}

fn keyed_identifier_digest(key: &[u8], value: &[u8]) -> String {
    let mut normalized = [0_u8; 64];
    if key.len() > normalized.len() {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for ((inner, outer), key) in inner_pad
        .iter_mut()
        .zip(outer_pad.iter_mut())
        .zip(normalized)
    {
        *inner ^= key;
        *outer ^= key;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(value);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    format!("{:x}", outer.finalize())
}

pub(crate) fn serve_http(
    db: &Path,
    host: &str,
    port: u16,
    once: bool,
    auth_token: Option<&str>,
    mcp_profile: &str,
    mcp_page_size: usize,
) -> Result<()> {
    let auth_token = auth_token.map(str::trim).filter(|token| !token.is_empty());
    let auth_policy = security::HttpAuthPolicy::from_environment(auth_token)?;
    if !is_loopback_host(host) && !auth_policy.configured() {
        bail!(
            "external HTTP binds require a full or read-only HTTP token; use 127.0.0.1 for local-only access"
        );
    }
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind http server on {host}:{port}"))?;
    let addr = listener.local_addr()?;
    let security_policy = security::HttpSecurityPolicy::from_environment(host, addr)?;
    let rate_limiter = security::HttpRateLimiter::from_environment()?;
    let concurrency_limiter = security::HttpConcurrencyLimiter::from_environment()?;
    let mcp_http = mcp_server::McpHttpService::new(mcp_profile, mcp_page_size)?;
    let otlp = otlp::OtlpExporter::from_environment()?;
    let telemetry_identifiers = TelemetryIdentifierPolicy::from_environment()?;
    println!("http://{addr}");
    let state = std::sync::Arc::new(HttpAppState {
        default_db: db.to_path_buf(),
        auth_policy,
        security_policy,
        rate_limiter,
        concurrency_limiter,
        mcp_http,
        otlp,
        telemetry_identifiers,
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
            Ok((stream, _)) => match sender.try_send(stream) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(mut stream)) => {
                    let _ = stream.set_write_timeout(Some(HTTP_IO_SLICE));
                    let _ = crate::http_api::write_response(
                        &mut stream,
                        HttpResponse::service_unavailable(
                            "HTTP worker queue is full; retry the request later",
                        ),
                    );
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    bail!("HTTP worker queue stopped");
                }
            },
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
    let request_id = Uuid::new_v4().simple().to_string()[..16].to_string();
    let mut request_meta = HttpRequestMeta::default();
    let peer_address = stream.peer_addr().ok();
    let peer = peer_address
        .map(|address| address.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mut response = match routes::handle_http_request(state, &mut stream, &mut request_meta) {
        Ok(response) => response,
        Err(err) => HttpResponse::from_error(&err),
    };
    let status = response.status;
    response.request_id = Some(request_id.clone());
    stream.set_write_timeout(Some(HTTP_REQUEST_DEADLINE))?;
    crate::http_api::write_response(&mut stream, response)?;
    let access_event = json!({
        "event": "http_access",
        "peer": state.telemetry_identifiers.protect(&peer),
        "client": state.telemetry_identifiers.protect(if request_meta.client.is_empty() { "unknown" } else { &request_meta.client }),
        "request_id": request_id,
        "method": request_meta.method,
        "path": request_meta.path,
        "status": status,
        "elapsed_ms": started.elapsed().as_millis(),
    });
    eprintln!("{access_event}");
    if let Some(exporter) = &state.otlp {
        exporter.emit_http_access(&access_event);
    }
    Ok(())
}

pub(super) fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(crate) fn parse_json_body(body: &str) -> Result<Value> {
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

pub(crate) fn memory_rows_with_request_counts(
    conn: &Connection,
    rows: Vec<Memory>,
) -> Result<Vec<Value>> {
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

pub(crate) fn filter_sort_memory_rows(
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

pub(crate) fn parse_query(query: &str) -> HashMap<String, String> {
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

fn resolve_project_input(root: &Path, input: &Path) -> Result<PathBuf> {
    let root = canonical_or_absolute(root);
    let candidate = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let candidate = canonical_or_absolute(&candidate);
    if !candidate.starts_with(&root) {
        bail!(
            "file input is outside selected project root: {}",
            candidate.display()
        );
    }
    Ok(candidate)
}

fn memory_ui_html() -> &'static str {
    static HTML: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HTML.get_or_init(|| {
        let mut html = include_str!("memory_ui.html").to_string();
        let style_start = html.find("  <style>").expect("memory UI style start");
        let style_end = html
            .find("  </style>")
            .map(|index| index + "  </style>".len())
            .expect("memory UI style end");
        html.replace_range(
            style_start..style_end,
            "  <link rel=\"stylesheet\" href=\"/ui.css\">",
        );
        let script_start = html.find("  <script>").expect("memory UI script start");
        let script_end = html
            .find("  </script>")
            .map(|index| index + "  </script>".len())
            .expect("memory UI script end");
        html.replace_range(
            script_start..script_end,
            "  <script src=\"/ui.js\" defer></script>",
        );
        html
    })
}

fn memory_ui_css() -> &'static str {
    static CSS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CSS.get_or_init(|| {
        let html = include_str!("memory_ui.html");
        html.split_once("  <style>")
            .and_then(|(_, rest)| rest.split_once("  </style>"))
            .map(|(css, _)| css.trim().to_string())
            .expect("memory UI inline style")
    })
}

fn memory_ui_javascript() -> &'static str {
    static JAVASCRIPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    JAVASCRIPT.get_or_init(|| {
        let html = include_str!("memory_ui.html");
        html.split_once("  <script>")
            .and_then(|(_, rest)| rest.split_once("  </script>"))
            .map(|(javascript, _)| javascript.trim().to_string())
            .expect("memory UI inline script")
    })
}

fn read_http_request(stream: &mut TcpStream) -> Result<Vec<u8>> {
    read_http_request_with_deadline(stream, HTTP_REQUEST_DEADLINE)
}

fn read_http_request_with_deadline(
    stream: &mut TcpStream,
    timeout: std::time::Duration,
) -> Result<Vec<u8>> {
    let deadline = std::time::Instant::now() + timeout;
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = read_http_chunk(stream, &mut chunk, deadline)?;
        if read == 0 {
            bail!("empty or incomplete HTTP request");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(pos) = find_header_end(&buffer) {
            break pos;
        }
        if buffer.len() > HTTP_MAX_HEADER_BYTES {
            bail!("HTTP request headers are too large");
        }
    };
    if header_end > HTTP_MAX_HEADER_BYTES {
        bail!("HTTP request headers are too large");
    }
    let content_length = content_length(&buffer[..header_end.saturating_sub(4)])?;
    if content_length > HTTP_MAX_BODY_BYTES {
        bail!("HTTP request body is too large");
    }
    let target_len = header_end
        .checked_add(content_length)
        .context("HTTP request size overflow")?;
    while buffer.len() < target_len {
        let read = read_http_chunk(stream, &mut chunk, deadline)?;
        if read == 0 {
            bail!("HTTP request body ended before Content-Length");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > target_len.max(HTTP_MAX_HEADER_BYTES + HTTP_MAX_BODY_BYTES) {
            bail!("HTTP request body is too large");
        }
    }
    buffer.truncate(target_len);
    Ok(buffer)
}

fn read_http_chunk(
    stream: &mut TcpStream,
    chunk: &mut [u8],
    deadline: std::time::Instant,
) -> Result<usize> {
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            bail!("HTTP request deadline exceeded");
        }
        stream.set_read_timeout(Some(remaining.min(HTTP_IO_SLICE)))?;
        match stream.read(chunk) {
            Ok(read) => return Ok(read),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if std::time::Instant::now() >= deadline {
                    bail!("HTTP request deadline exceeded");
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
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

#[cfg(test)]
mod http_framing_tests {
    use super::{
        TelemetryIdentifierPolicy, content_length, keyed_identifier_digest,
        read_http_request_with_deadline,
    };
    use proptest::prelude::*;
    use std::io::Write;

    #[test]
    fn accepts_one_canonical_content_length() {
        assert_eq!(
            content_length(b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 42").unwrap(),
            42
        );
        assert_eq!(
            content_length(b"GET / HTTP/1.1\r\nHost: localhost").unwrap(),
            0
        );
    }

    #[test]
    fn rejects_ambiguous_or_unsupported_body_framing() {
        for header in [
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1",
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked",
            "POST / HTTP/1.1\r\nTransfer-Encoding: identity\r\nContent-Length: 1",
            "POST / HTTP/1.1\r\nContent-Length: +1",
            "POST / HTTP/1.1\r\nContent-Length: 1, 1",
            "POST / HTTP/1.1\r\n folded: value",
            "GET / HTTP/1.1\r\nHost: one\r\nHost: two",
            "GET / HTTP/1.1\r\nHost: local\r\nAuthorization: Bearer one\r\nAuthorization: Bearer two",
            "GET / HTTP/1.1\r\nUser Agent: invalid",
            "GET / HTTP/1.1\r\nConnection: close",
        ] {
            assert!(
                content_length(header.as_bytes()).is_err(),
                "header={header:?}"
            );
        }
    }

    #[test]
    fn enforces_one_absolute_request_deadline_across_reads() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream.write_all(b"G").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        let (mut stream, _) = listener.accept().unwrap();
        let error =
            read_http_request_with_deadline(&mut stream, std::time::Duration::from_millis(25))
                .unwrap_err();
        assert!(error.to_string().contains("HTTP request deadline exceeded"));
        client.join().unwrap();
    }

    #[test]
    fn telemetry_identifier_policy_can_hash_or_omit_client_addresses() {
        let hashed = TelemetryIdentifierPolicy::Hash(b"0123456789abcdef".to_vec())
            .protect("203.0.113.7:443");
        assert!(hashed.starts_with("hmac-sha256:"));
        assert_eq!(hashed.len(), 36);
        assert_ne!(hashed, "203.0.113.7:443");
        assert_eq!(
            TelemetryIdentifierPolicy::Omit.protect("203.0.113.7:443"),
            "redacted"
        );
        assert_eq!(
            TelemetryIdentifierPolicy::Plain.protect("203.0.113.7:443"),
            "203.0.113.7:443"
        );
        assert_eq!(
            keyed_identifier_digest(b"key", b"The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn arbitrary_http_headers_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..70_000)) {
            let _ = content_length(&bytes);
        }

        #[test]
        fn canonical_content_lengths_round_trip(length in 0usize..=16 * 1024 * 1024) {
            let header = format!(
                "POST /memory HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}"
            );
            prop_assert_eq!(content_length(header.as_bytes()).unwrap(), length);
        }

        #[test]
        fn duplicate_singleton_headers_are_rejected(
            name in prop_oneof![
                Just("Host"),
                Just("Content-Length"),
                Just("Authorization"),
                Just("Accept"),
                Just("Content-Type"),
                Just("Mcp-Method"),
                Just("Mcp-Name"),
                Just("MCP-Protocol-Version"),
                Just("MCP-Session-Id"),
                Just("Proxy-Authorization"),
                Just("Origin"),
                Just("Sec-Fetch-Site"),
                Just("X-DukeMemory-Token"),
            ],
            first in "[A-Za-z0-9._-]{1,32}",
            second in "[A-Za-z0-9._-]{1,32}",
        ) {
            let request_line = if name == "Host" {
                "GET / HTTP/1.1"
            } else {
                "GET / HTTP/1.0"
            };
            let header = format!("{request_line}\r\n{name}: {first}\r\n{name}: {second}");
            prop_assert!(content_length(header.as_bytes()).is_err());
        }
    }
}

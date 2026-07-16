use super::*;

const MCP_DEFAULT_TASK_TTL_MS: u64 = 3_600_000;
const MCP_MAX_TASK_TTL_MS: u64 = 86_400_000;
const MCP_TASK_PAGE_SIZE: usize = 50;
const MCP_LEGACY_TASK_RESULT_WAIT_MS: u64 = 30_000;
const MCP_MAX_CONCURRENT_TASKS_ENV: &str = "DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS";
const MCP_MAX_CONCURRENT_TASKS_PER_OWNER_ENV: &str =
    "DUKEMEMORY_MCP_MAX_CONCURRENT_TASKS_PER_OWNER";
const MCP_DEFAULT_MAX_CONCURRENT_TASKS: usize = 32;
const MCP_DEFAULT_MAX_CONCURRENT_TASKS_PER_OWNER: usize = 4;

#[derive(Debug, Clone)]
pub(super) struct McpTaskRecord {
    pub(super) task_id: String,
    pub(super) owner_key: String,
    pub(super) protocol_version: String,
    pub(super) lifecycle: String,
    pub(super) operation_name: String,
    pub(super) status: String,
    pub(super) status_message: String,
    pub(super) created_at: String,
    pub(super) last_updated_at: String,
    pub(super) created_at_ms: i64,
    pub(super) last_updated_at_ms: i64,
    pub(super) ttl: u64,
    pub(super) poll_interval: u64,
    pub(super) expires_at_ms: i64,
    pub(super) result: Option<Value>,
    pub(super) error: Option<Value>,
    pub(super) cancellation_requested: bool,
}

#[derive(Debug, Default)]
struct McpTaskAdmissionState {
    total: usize,
    owners: HashMap<String, usize>,
}

#[derive(Debug)]
pub(super) struct McpTaskStore {
    cancellations: std::sync::Mutex<HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    wake_generation: std::sync::Mutex<u64>,
    changed: std::sync::Condvar,
    maximum_tasks: usize,
    maximum_tasks_per_owner: usize,
    admission: std::sync::Mutex<McpTaskAdmissionState>,
}

struct McpTaskPermit {
    store: std::sync::Arc<McpTaskStore>,
    owner_key: String,
}

impl Default for McpTaskStore {
    fn default() -> Self {
        Self::new(
            MCP_DEFAULT_MAX_CONCURRENT_TASKS,
            MCP_DEFAULT_MAX_CONCURRENT_TASKS_PER_OWNER,
        )
    }
}

impl McpTaskStore {
    pub(super) fn from_environment() -> Result<Self> {
        let maximum_tasks = mcp_positive_usize_env(
            MCP_MAX_CONCURRENT_TASKS_ENV,
            MCP_DEFAULT_MAX_CONCURRENT_TASKS,
        )?;
        let maximum_tasks_per_owner = mcp_positive_usize_env(
            MCP_MAX_CONCURRENT_TASKS_PER_OWNER_ENV,
            MCP_DEFAULT_MAX_CONCURRENT_TASKS_PER_OWNER,
        )?;
        if maximum_tasks_per_owner > maximum_tasks {
            bail!(
                "{MCP_MAX_CONCURRENT_TASKS_PER_OWNER_ENV} must not exceed {MCP_MAX_CONCURRENT_TASKS_ENV}"
            );
        }
        Ok(Self::new(maximum_tasks, maximum_tasks_per_owner))
    }

    fn new(maximum_tasks: usize, maximum_tasks_per_owner: usize) -> Self {
        Self {
            cancellations: std::sync::Mutex::new(HashMap::new()),
            wake_generation: std::sync::Mutex::new(0),
            changed: std::sync::Condvar::new(),
            maximum_tasks: maximum_tasks.max(1),
            maximum_tasks_per_owner: maximum_tasks_per_owner.max(1).min(maximum_tasks.max(1)),
            admission: std::sync::Mutex::new(McpTaskAdmissionState::default()),
        }
    }

    fn try_acquire(
        self: &std::sync::Arc<Self>,
        owner_key: &str,
    ) -> std::result::Result<Option<McpTaskPermit>, String> {
        let mut admission = self
            .admission
            .lock()
            .map_err(|_| "MCP task admission lock was poisoned".to_string())?;
        let owner_count = admission.owners.get(owner_key).copied().unwrap_or(0);
        if admission.total >= self.maximum_tasks || owner_count >= self.maximum_tasks_per_owner {
            return Ok(None);
        }
        admission.total += 1;
        *admission.owners.entry(owner_key.to_string()).or_default() += 1;
        Ok(Some(McpTaskPermit {
            store: std::sync::Arc::clone(self),
            owner_key: owner_key.to_string(),
        }))
    }

    fn release(&self, owner_key: &str) {
        let Ok(mut admission) = self.admission.lock() else {
            return;
        };
        admission.total = admission.total.saturating_sub(1);
        if let Some(count) = admission.owners.get_mut(owner_key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                admission.owners.remove(owner_key);
            }
        }
    }
}

impl Drop for McpTaskPermit {
    fn drop(&mut self) {
        self.store.release(&self.owner_key);
    }
}

fn mcp_positive_usize_env(name: &str, default: usize) -> Result<usize> {
    let value = std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("{name} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default);
    if value == 0 {
        bail!("{name} must be greater than zero");
    }
    Ok(value)
}

pub(super) fn mcp_tool_supports_tasks(name: &str) -> bool {
    matches!(
        name,
        "memory_advanced_eval"
            | "memory_auto_ingest"
            | "memory_context_pack"
            | "memory_fleet_dashboard_v2"
            | "memory_fleet_quality"
            | "memory_graph_rag_answer"
            | "memory_graph_rag_eval"
            | "memory_guided_tour"
            | "memory_onboard_guide"
            | "memory_quality_ci"
            | "memory_rag_answer"
            | "memory_rag_eval"
            | "memory_rag_ingest"
            | "memory_release_gate_v2"
            | "memory_release_gate_v3"
    )
}

pub(super) fn mcp_task_call_is_read_only(name: &str, args: &Value) -> bool {
    match name {
        "memory_auto_ingest" => args
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "memory_rag_ingest" => !args.get("apply").and_then(Value::as_bool).unwrap_or(false),
        "memory_evidence_autopilot" => !args.get("apply").and_then(Value::as_bool).unwrap_or(false),
        "memory_rag_eval" => !args
            .get("write_baseline")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "memory_release_gate_v3" => !args.get("run").and_then(Value::as_bool).unwrap_or(false),
        _ => mcp_tool_annotations(name)
            .get("readOnlyHint")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

pub(super) fn mcp_legacy_tasks_enabled(state: &McpSessionState) -> bool {
    state.protocol_version.as_deref() == Some(MCP_LATEST_PROTOCOL_VERSION) && state.initialized
}

pub(super) fn mcp_start_task(
    db: &Path,
    params: Value,
    state: &McpSessionState,
    owner_key: &str,
    lifecycle: &str,
) -> std::result::Result<Value, String> {
    let permit = state.tasks.try_acquire(owner_key)?.ok_or_else(|| {
        format!(
            "MCP task concurrency limit exceeded (global={}, per_owner={})",
            state.tasks.maximum_tasks, state.tasks.maximum_tasks_per_owner
        )
    })?;
    let ttl = params
        .get("task")
        .and_then(|value| value.get("ttl"))
        .and_then(Value::as_u64)
        .unwrap_or(MCP_DEFAULT_TASK_TTL_MS)
        .clamp(1_000, MCP_MAX_TASK_TTL_MS);
    let operation_name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?
        .to_string();
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let selected_db = mcp_selected_db(db, &args)?;
    let task_db = db.to_path_buf();
    let task_id = Uuid::new_v4().to_string();
    let created_at = mcp_task_timestamp();
    let created_at_ms = now_ms();
    let record = McpTaskRecord {
        task_id: task_id.clone(),
        owner_key: owner_key.to_string(),
        protocol_version: if lifecycle == "extension" {
            MCP_MODERN_PROTOCOL_VERSION
        } else {
            MCP_LATEST_PROTOCOL_VERSION
        }
        .to_string(),
        lifecycle: lifecycle.to_string(),
        operation_name,
        status: "working".to_string(),
        status_message: "The tool call is running.".to_string(),
        created_at: created_at.clone(),
        last_updated_at: created_at,
        created_at_ms,
        last_updated_at_ms: created_at_ms,
        ttl,
        poll_interval: 250,
        expires_at_ms: created_at_ms.saturating_add(ttl.min(i64::MAX as u64) as i64),
        result: None,
        error: None,
        cancellation_requested: false,
    };
    let conn = open_db(&task_db).map_err(|error| error.to_string())?;
    cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
    persist_mcp_task(&conn, &record).map_err(|error| error.to_string())?;

    let store = std::sync::Arc::clone(&state.tasks);
    let cancellation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    store
        .cancellations
        .lock()
        .map_err(|_| "MCP task cancellation store lock was poisoned".to_string())?
        .insert(task_id.clone(), std::sync::Arc::clone(&cancellation));
    let worker_db = selected_db;
    let task_registry_db = task_db.clone();
    let task_id_for_worker = task_id.clone();
    let worker = std::thread::Builder::new()
        .name(format!("dukememory-mcp-task-{}", &task_id[..8]))
        .spawn(move || {
            let _permit = permit;
            let cancellation_requested = cancellation.load(std::sync::atomic::Ordering::Acquire)
                || mcp_task_cancellation_requested(&task_registry_db, &task_id_for_worker)
                    .unwrap_or(false);
            if cancellation_requested {
                let _ = complete_cancelled_mcp_task(&task_registry_db, &task_id_for_worker);
                notify_mcp_task_store(&store);
                remove_mcp_task_cancellation(&store, &task_id_for_worker);
                return;
            }
            let mut result = crate::app::generation::with_generation_cancellation(
                std::sync::Arc::clone(&cancellation),
                || handle_mcp_tool_call(&worker_db, params).unwrap_or_else(mcp_tool_error_result),
            );
            let cancellation_requested = cancellation.load(std::sync::atomic::Ordering::Acquire)
                || mcp_task_cancellation_requested(&task_registry_db, &task_id_for_worker)
                    .unwrap_or(false);
            if cancellation_requested {
                let _ = complete_cancelled_mcp_task(&task_registry_db, &task_id_for_worker);
                notify_mcp_task_store(&store);
                remove_mcp_task_cancellation(&store, &task_id_for_worker);
                return;
            }
            attach_related_task_metadata(&mut result, &task_id_for_worker);
            let _ = complete_mcp_task(&task_registry_db, &task_id_for_worker, &result);
            notify_mcp_task_store(&store);
            remove_mcp_task_cancellation(&store, &task_id_for_worker);
        });
    if let Err(error) = worker {
        remove_mcp_task_cancellation(&state.tasks, &task_id);
        fail_mcp_task(
            &task_db,
            &task_id,
            -32603,
            &format!("failed to start MCP task: {error}"),
        )?;
        return Err(format!("failed to start MCP task: {error}"));
    }

    if lifecycle == "extension" {
        Ok(mcp_extension_task_value(&record, true))
    } else {
        Ok(json!({
            "task": mcp_legacy_task_value(&record),
            "_meta": {
                "io.modelcontextprotocol/model-immediate-response": "The DukeMemory operation is running in the background; poll tasks/get and retrieve it with tasks/result."
            }
        }))
    }
}

pub(super) fn mcp_task_get(
    db: &Path,
    params: Option<&Value>,
    owner_key: &str,
    lifecycle: &str,
    extension: bool,
) -> std::result::Result<Value, String> {
    let task_id = mcp_task_id(params)?;
    let conn = open_db(db).map_err(|error| error.to_string())?;
    cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
    let task = load_mcp_task(&conn, task_id, owner_key, lifecycle)?
        .ok_or_else(|| format!("unknown or expired task: {task_id}"))?;
    Ok(if extension {
        mcp_extension_task_value(&task, false)
    } else {
        mcp_legacy_task_value(&task)
    })
}

pub(super) fn mcp_task_list(
    db: &Path,
    params: Option<&Value>,
    owner_key: &str,
) -> std::result::Result<Value, String> {
    let prefix = "tasks:session:";
    let offset = parse_mcp_cursor(params, prefix)?;
    let conn = open_db(db).map_err(|error| error.to_string())?;
    cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
    let mut tasks = list_mcp_tasks(&conn, owner_key, "legacy", offset, MCP_TASK_PAGE_SIZE + 1)?;
    let has_more = tasks.len() > MCP_TASK_PAGE_SIZE;
    tasks.truncate(MCP_TASK_PAGE_SIZE);
    let mut result = json!({
        "tasks": tasks.iter().map(mcp_legacy_task_value).collect::<Vec<_>>()
    });
    if has_more {
        result["nextCursor"] = Value::String(format!(
            "{prefix}{}",
            offset.saturating_add(MCP_TASK_PAGE_SIZE)
        ));
    }
    Ok(result)
}

pub(super) fn mcp_task_result(
    db: &Path,
    params: Option<&Value>,
    state: &McpSessionState,
    owner_key: &str,
) -> std::result::Result<Value, String> {
    let task_id = mcp_task_id(params)?.to_string();
    let deadline =
        Instant::now() + std::time::Duration::from_millis(MCP_LEGACY_TASK_RESULT_WAIT_MS);
    let mut generation = state
        .tasks
        .wake_generation
        .lock()
        .map_err(|_| "MCP task store lock was poisoned".to_string())?;
    loop {
        let conn = open_db(db).map_err(|error| error.to_string())?;
        cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
        let task = load_mcp_task(&conn, &task_id, owner_key, "legacy")?
            .ok_or_else(|| format!("unknown or expired task: {task_id}"))?;
        if let Some(result) = &task.result {
            return Ok(result.clone());
        }
        if task.status == "failed" {
            return Err(task.status_message);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "task {task_id} is still working; poll tasks/get before retrying tasks/result"
            ));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait = remaining.min(std::time::Duration::from_millis(task.poll_interval.max(50)));
        let (next_generation, _) = state
            .tasks
            .changed
            .wait_timeout(generation, wait)
            .map_err(|_| "MCP task store lock was poisoned".to_string())?;
        generation = next_generation;
    }
}

pub(super) fn mcp_task_cancel(
    db: &Path,
    params: Option<&Value>,
    state: &McpSessionState,
    owner_key: &str,
    lifecycle: &str,
    extension: bool,
) -> std::result::Result<Value, String> {
    let task_id = mcp_task_id(params)?.to_string();
    let conn = open_db(db).map_err(|error| error.to_string())?;
    cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
    let task = load_mcp_task(&conn, &task_id, owner_key, lifecycle)?
        .ok_or_else(|| format!("unknown or expired task: {task_id}"))?;
    if matches!(task.status.as_str(), "completed" | "failed" | "cancelled") {
        return if extension {
            Ok(json!({"resultType":"complete"}))
        } else {
            Err(format!("task {task_id} is already terminal"))
        };
    }
    conn.execute(
        "UPDATE mcp_tasks SET cancellation_requested = 1, status_message = ?1, last_updated_at = ?2, last_updated_at_ms = ?3 WHERE task_id = ?4 AND owner_key = ?5 AND lifecycle = ?6",
        params![
            "Cancellation was requested; the worker will stop if it has not started.",
            mcp_task_timestamp(),
            now_ms(),
            task_id,
            owner_key,
            lifecycle,
        ],
    )
    .map_err(|error| error.to_string())?;
    if let Some(cancellation) = state
        .tasks
        .cancellations
        .lock()
        .map_err(|_| "MCP task cancellation store lock was poisoned".to_string())?
        .get(&task_id)
    {
        cancellation.store(true, std::sync::atomic::Ordering::Release);
    }
    notify_mcp_task_store(&state.tasks);
    if extension {
        Ok(json!({"resultType":"complete"}))
    } else {
        let task = load_mcp_task(&conn, &task_id, owner_key, lifecycle)?
            .ok_or_else(|| format!("unknown or expired task: {task_id}"))?;
        Ok(mcp_legacy_task_value(&task))
    }
}

pub(super) fn mcp_task_update(
    db: &Path,
    params: Option<&Value>,
    owner_key: &str,
    lifecycle: &str,
) -> std::result::Result<Value, String> {
    let task_id = mcp_task_id(params)?;
    if !params
        .and_then(|value| value.get("inputResponses"))
        .is_some_and(Value::is_object)
    {
        return Err("tasks/update requires an inputResponses object".to_string());
    }
    let conn = open_db(db).map_err(|error| error.to_string())?;
    cleanup_expired_mcp_tasks(&conn).map_err(|error| error.to_string())?;
    load_mcp_task(&conn, task_id, owner_key, lifecycle)?
        .ok_or_else(|| format!("unknown or expired task: {task_id}"))?;
    Ok(json!({"resultType":"complete"}))
}

pub(super) fn mcp_task_id(params: Option<&Value>) -> std::result::Result<&str, String> {
    params
        .and_then(|value| value.get("taskId"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "missing taskId".to_string())
}

pub(super) fn mcp_legacy_task_value(task: &McpTaskRecord) -> Value {
    json!({
        "taskId": task.task_id,
        "status": task.status,
        "statusMessage": task.status_message,
        "createdAt": task.created_at,
        "lastUpdatedAt": task.last_updated_at,
        "ttl": task.ttl,
        "pollInterval": task.poll_interval,
    })
}

pub(super) fn mcp_extension_task_value(task: &McpTaskRecord, creation: bool) -> Value {
    let mut value = json!({
        "resultType": if creation { "task" } else { "complete" },
        "taskId": task.task_id,
        "status": task.status,
        "statusMessage": task.status_message,
        "createdAt": task.created_at,
        "lastUpdatedAt": task.last_updated_at,
        "ttlMs": task.ttl,
        "pollIntervalMs": task.poll_interval,
    });
    if !creation {
        if let Some(result) = &task.result {
            value["result"] = result.clone();
        }
        if let Some(error) = &task.error {
            value["error"] = error.clone();
        }
    }
    value
}

const MCP_TASK_COLUMNS: &str = "task_id, owner_key, protocol_version, lifecycle, operation_name, status, status_message, created_at, last_updated_at, created_at_ms, last_updated_at_ms, ttl_ms, poll_interval_ms, expires_at_ms, result_json, error_json, cancellation_requested";

pub(super) fn persist_mcp_task(conn: &Connection, task: &McpTaskRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO mcp_tasks (task_id, owner_key, protocol_version, lifecycle, operation_name, status, status_message, created_at, last_updated_at, created_at_ms, last_updated_at_ms, ttl_ms, poll_interval_ms, expires_at_ms, result_json, error_json, cancellation_requested) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            task.task_id,
            task.owner_key,
            task.protocol_version,
            task.lifecycle,
            task.operation_name,
            task.status,
            task.status_message,
            task.created_at,
            task.last_updated_at,
            task.created_at_ms,
            task.last_updated_at_ms,
            task.ttl.min(i64::MAX as u64) as i64,
            task.poll_interval.min(i64::MAX as u64) as i64,
            task.expires_at_ms,
            task.result.as_ref().map(Value::to_string),
            task.error.as_ref().map(Value::to_string),
            i64::from(task.cancellation_requested),
        ],
    )?;
    Ok(())
}

pub(super) fn mcp_task_from_row(row: &Row<'_>) -> rusqlite::Result<McpTaskRecord> {
    let result_json = row.get::<_, Option<String>>(14)?;
    let error_json = row.get::<_, Option<String>>(15)?;
    Ok(McpTaskRecord {
        task_id: row.get(0)?,
        owner_key: row.get(1)?,
        protocol_version: row.get(2)?,
        lifecycle: row.get(3)?,
        operation_name: row.get(4)?,
        status: row.get(5)?,
        status_message: row.get(6)?,
        created_at: row.get(7)?,
        last_updated_at: row.get(8)?,
        created_at_ms: row.get(9)?,
        last_updated_at_ms: row.get(10)?,
        ttl: row.get::<_, i64>(11)?.max(0) as u64,
        poll_interval: row.get::<_, i64>(12)?.max(0) as u64,
        expires_at_ms: row.get(13)?,
        result: result_json.and_then(|value| serde_json::from_str(&value).ok()),
        error: error_json.and_then(|value| serde_json::from_str(&value).ok()),
        cancellation_requested: row.get::<_, i64>(16)? != 0,
    })
}

pub(super) fn load_mcp_task(
    conn: &Connection,
    task_id: &str,
    owner_key: &str,
    lifecycle: &str,
) -> std::result::Result<Option<McpTaskRecord>, String> {
    conn.query_row(
        &format!("SELECT {MCP_TASK_COLUMNS} FROM mcp_tasks WHERE task_id = ?1 AND owner_key = ?2 AND lifecycle = ?3"),
        params![task_id, owner_key, lifecycle],
        mcp_task_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub(super) fn list_mcp_tasks(
    conn: &Connection,
    owner_key: &str,
    lifecycle: &str,
    offset: usize,
    limit: usize,
) -> std::result::Result<Vec<McpTaskRecord>, String> {
    let mut statement = conn
        .prepare(&format!(
            "SELECT {MCP_TASK_COLUMNS} FROM mcp_tasks WHERE owner_key = ?1 AND lifecycle = ?2 ORDER BY last_updated_at_ms DESC, task_id DESC LIMIT ?3 OFFSET ?4"
        ))
        .map_err(|error| error.to_string())?;
    statement
        .query_map(
            params![owner_key, lifecycle, limit as i64, offset as i64],
            mcp_task_from_row,
        )
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

pub(super) fn cleanup_expired_mcp_tasks(conn: &Connection) -> Result<()> {
    let now = now_ms();
    let grace_expires = now.saturating_add(60_000);
    let timestamp = mcp_task_timestamp();
    let error = json!({"code":-32603,"message":"Task execution exceeded its TTL"}).to_string();
    conn.execute(
        "UPDATE mcp_tasks SET status = 'failed', status_message = 'Task execution exceeded its TTL.', error_json = ?1, last_updated_at = ?2, last_updated_at_ms = ?3, expires_at_ms = ?4 WHERE status IN ('working', 'input_required') AND expires_at_ms <= ?3",
        params![error, timestamp, now, grace_expires],
    )?;
    conn.execute(
        "DELETE FROM mcp_tasks WHERE status IN ('completed', 'cancelled', 'failed') AND expires_at_ms <= ?1",
        params![now],
    )?;
    Ok(())
}

pub(super) fn mcp_task_cancellation_requested(db: &Path, task_id: &str) -> Result<bool> {
    let conn = open_db(db)?;
    Ok(conn
        .query_row(
            "SELECT cancellation_requested FROM mcp_tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some_and(|value| value != 0))
}

pub(super) fn complete_cancelled_mcp_task(db: &Path, task_id: &str) -> Result<()> {
    let conn = open_db(db)?;
    let lifecycle = conn
        .query_row(
            "SELECT lifecycle FROM mcp_tasks WHERE task_id = ?1",
            params![task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let result = lifecycle
        .as_deref()
        .filter(|value| *value == "legacy")
        .map(|_| {
            let mut value = mcp_tool_error_result("task was cancelled".to_string());
            attach_related_task_metadata(&mut value, task_id);
            value.to_string()
        });
    conn.execute(
        "UPDATE mcp_tasks SET status = 'cancelled', status_message = 'The task was cancelled before execution.', result_json = ?1, last_updated_at = ?2, last_updated_at_ms = ?3 WHERE task_id = ?4 AND status = 'working' AND cancellation_requested = 1",
        params![result, mcp_task_timestamp(), now_ms(), task_id],
    )?;
    Ok(())
}

pub(super) fn complete_mcp_task(db: &Path, task_id: &str, result: &Value) -> Result<()> {
    let conn = open_db(db)?;
    conn.execute(
        "UPDATE mcp_tasks SET status = 'completed', status_message = 'The tool call completed.', result_json = ?1, error_json = NULL, last_updated_at = ?2, last_updated_at_ms = ?3 WHERE task_id = ?4 AND status = 'working'",
        params![result.to_string(), mcp_task_timestamp(), now_ms(), task_id],
    )?;
    Ok(())
}

pub(super) fn fail_mcp_task(
    db: &Path,
    task_id: &str,
    code: i64,
    message: &str,
) -> std::result::Result<(), String> {
    let conn = open_db(db).map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE mcp_tasks SET status = 'failed', status_message = ?1, error_json = ?2, last_updated_at = ?3, last_updated_at_ms = ?4 WHERE task_id = ?5 AND status = 'working'",
        params![message, json!({"code":code,"message":message}).to_string(), mcp_task_timestamp(), now_ms(), task_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn notify_mcp_task_store(store: &McpTaskStore) {
    if let Ok(mut generation) = store.wake_generation.lock() {
        *generation = generation.wrapping_add(1);
        store.changed.notify_all();
    }
}

pub(super) fn remove_mcp_task_cancellation(store: &McpTaskStore, task_id: &str) {
    if let Ok(mut cancellations) = store.cancellations.lock() {
        cancellations.remove(task_id);
    }
}

pub(super) fn attach_related_task_metadata(result: &mut Value, task_id: &str) {
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let meta = object
        .entry("_meta")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(meta) = meta {
        meta.insert(
            "io.modelcontextprotocol/related-task".to_string(),
            json!({"taskId": task_id}),
        );
    }
}

pub(super) fn mcp_task_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    #[test]
    fn task_admission_enforces_global_and_owner_limits_until_permit_drop() {
        let store = std::sync::Arc::new(McpTaskStore::new(2, 1));
        let first = store.try_acquire("owner-a").unwrap().unwrap();
        assert!(store.try_acquire("owner-a").unwrap().is_none());
        let second = store.try_acquire("owner-b").unwrap().unwrap();
        assert!(store.try_acquire("owner-c").unwrap().is_none());
        drop(first);
        let third = store.try_acquire("owner-c").unwrap().unwrap();
        drop(second);
        drop(third);
        assert!(store.try_acquire("owner-a").unwrap().is_some());
    }
}

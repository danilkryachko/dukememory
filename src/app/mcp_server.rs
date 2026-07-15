use super::*;

mod tasks;
use tasks::*;

const MCP_LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
const MCP_LEGACY_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2024-11-05"];
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    MCP_MODERN_PROTOCOL_VERSION,
    "2025-11-25",
    "2025-06-18",
    "2024-11-05",
];
const MCP_TASKS_EXTENSION: &str = "io.modelcontextprotocol/tasks";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpProfile {
    Core,
    Standard,
    Full,
}

impl McpProfile {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "standard" => Ok(Self::Standard),
            "full" => Ok(Self::Full),
            other => bail!("unsupported MCP profile: {other}"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }
}

#[derive(Debug)]
struct McpSessionState {
    protocol_version: Option<String>,
    initialized: bool,
    profile: McpProfile,
    page_size: usize,
    client_key: String,
    tasks: std::sync::Arc<McpTaskStore>,
}

pub(crate) fn serve_mcp(
    db: &Path,
    content_length: bool,
    profile: &str,
    page_size: usize,
) -> Result<()> {
    let mut state = McpSessionState {
        protocol_version: None,
        initialized: false,
        profile: McpProfile::parse(profile)?,
        page_size,
        client_key: "stdio:legacy-local".to_string(),
        tasks: std::sync::Arc::new(McpTaskStore::default()),
    };
    super::mcp_transport::serve_json_rpc(content_length, |request| {
        handle_mcp_request(db, request, &mut state)
    })
}

fn handle_mcp_request(db: &Path, request: Value, state: &mut McpSessionState) -> Option<Value> {
    let valid_request = request.as_object().is_some()
        && request.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && request.get("method").and_then(Value::as_str).is_some()
        && request.get("id").is_none_or(|id| {
            id.is_null() || id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()
        });
    if !valid_request {
        return Some(json!({
            "jsonrpc":"2.0",
            "id":Value::Null,
            "error":{"code":-32600,"message":"Invalid Request"}
        }));
    }
    let is_notification = request.get("id").is_none();
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let meta = request.get("params").and_then(|params| params.get("_meta"));
    let requested_version = meta
        .and_then(|meta| meta.get("io.modelcontextprotocol/protocolVersion"))
        .and_then(Value::as_str);
    if requested_version.is_some_and(|version| !MCP_SUPPORTED_PROTOCOL_VERSIONS.contains(&version))
    {
        if is_notification {
            return None;
        }
        return Some(mcp_rpc_error(
            id,
            -32004,
            "Unsupported protocol version",
            Some(json!({
                "supported": MCP_SUPPORTED_PROTOCOL_VERSIONS,
                "requested": requested_version.unwrap_or_default(),
            })),
        ));
    }
    let modern = requested_version == Some(MCP_MODERN_PROTOCOL_VERSION);
    let modern_client_info = meta.and_then(|meta| {
        meta.get("io.modelcontextprotocol/clientInfo")
            .filter(|value| {
                value.get("name").and_then(Value::as_str).is_some()
                    && value.get("version").and_then(Value::as_str).is_some()
            })
    });
    let modern_client_capabilities = meta.and_then(|meta| {
        meta.get("io.modelcontextprotocol/clientCapabilities")
            .filter(|value| value.is_object())
    });
    if modern && (modern_client_info.is_none() || modern_client_capabilities.is_none()) {
        if is_notification {
            return None;
        }
        return Some(mcp_rpc_error(
            id,
            -32602,
            "Modern MCP requests require clientInfo and clientCapabilities in params._meta",
            None,
        ));
    }
    let modern_tasks = modern_client_capabilities.is_some_and(|capabilities| {
        capabilities
            .get("extensions")
            .and_then(|extensions| extensions.get(MCP_TASKS_EXTENSION))
            .is_some_and(Value::is_object)
    });
    if modern && method.starts_with("tasks/") && !modern_tasks {
        if is_notification {
            return None;
        }
        return Some(mcp_rpc_error(
            id,
            -32003,
            "Missing required client capability",
            Some(json!({
                "requiredCapabilities": {
                    "extensions": {MCP_TASKS_EXTENSION: {}}
                }
            })),
        ));
    }
    let owner_key = if modern {
        mcp_client_key(modern_client_info, "modern-local")
    } else {
        state.client_key.clone()
    };
    let result = match method {
        "server/discover" if modern => Ok(mcp_server_discover(state)),
        "initialize" if !modern => initialize_mcp_session(request.get("params"), state),
        "notifications/initialized" if !modern => {
            state.initialized = true;
            Ok(json!({}))
        }
        "notifications/cancelled" => Ok(json!({})),
        "ping" if !modern => Ok(json!({})),
        "tools/list" if !modern && state.protocol_version.is_some() && !state.initialized => {
            Err("client must send notifications/initialized before tools/list".to_string())
        }
        "tools/list" => mcp_list_tools(request.get("params"), state).map(|mut result| {
            if modern {
                result["ttlMs"] = json!(60_000);
                result["cacheScope"] = json!("public");
            }
            result
        }),
        "tools/call" if !modern && state.protocol_version.is_some() && !state.initialized => {
            Err("client must send notifications/initialized before tools/call".to_string())
        }
        "tools/call" => handle_mcp_call(
            db,
            request.get("params").cloned().unwrap_or_default(),
            state,
            modern,
            modern_tasks,
            &owner_key,
        ),
        "resources/list" => mcp_list_resources(request.get("params"), state).map(|mut result| {
            if modern {
                result["ttlMs"] = json!(30_000);
                result["cacheScope"] = json!("private");
            }
            result
        }),
        "resources/templates/list" => {
            let mut result = mcp_resource_templates();
            if modern {
                result["ttlMs"] = json!(30_000);
                result["cacheScope"] = json!("private");
            }
            Ok(result)
        }
        "resources/read" => mcp_read_resource(db, request.get("params")),
        "tasks/get" if modern => {
            mcp_task_get(db, request.get("params"), &owner_key, "extension", true)
        }
        "tasks/update" if modern => {
            mcp_task_update(db, request.get("params"), &owner_key, "extension")
        }
        "tasks/cancel" if modern => mcp_task_cancel(
            db,
            request.get("params"),
            state,
            &owner_key,
            "extension",
            true,
        ),
        "tasks/get" if mcp_legacy_tasks_enabled(state) => {
            mcp_task_get(db, request.get("params"), &owner_key, "legacy", false)
        }
        "tasks/list" if mcp_legacy_tasks_enabled(state) => {
            mcp_task_list(db, request.get("params"), &owner_key)
        }
        "tasks/result" if mcp_legacy_tasks_enabled(state) => {
            mcp_task_result(db, request.get("params"), state, &owner_key)
        }
        "tasks/cancel" if mcp_legacy_tasks_enabled(state) => mcp_task_cancel(
            db,
            request.get("params"),
            state,
            &owner_key,
            "legacy",
            false,
        ),
        _ => Err(format!("unsupported method: {method}")),
    };
    if is_notification {
        return None;
    }
    Some(match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(message) => {
            let code = if method.is_empty() || message.starts_with("client must send") {
                -32600
            } else if message.starts_with("unsupported method") {
                -32601
            } else if method.starts_with("tasks/")
                || method == "tools/call"
                || method == "resources/read"
            {
                -32602
            } else {
                -32603
            };
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    })
}

fn mcp_rpc_error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({"code": code, "message": message});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc":"2.0","id":id,"error":error})
}

fn mcp_client_key(client_info: Option<&Value>, fallback: &str) -> String {
    let identity = client_info.map_or_else(
        || fallback.to_string(),
        |info| {
            format!(
                "{}:{}",
                info.get("name").and_then(Value::as_str).unwrap_or(fallback),
                info.get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            )
        },
    );
    let digest = Sha256::digest(identity.as_bytes());
    format!("stdio:{:x}", digest)
}

fn mcp_server_capabilities(modern: bool) -> Value {
    let mut capabilities = json!({
        "tools": {"listChanged": false},
        "resources": {"subscribe": false, "listChanged": false}
    });
    if modern {
        capabilities["extensions"] = json!({MCP_TASKS_EXTENSION: {}});
    } else {
        capabilities["tasks"] = json!({
            "list": {},
            "cancel": {},
            "requests": {"tools": {"call": {}}}
        });
    }
    capabilities
}

fn mcp_server_instructions(profile: McpProfile) -> String {
    format!(
        "MCP profile: {}. Call memory_budget_plan when budget is unclear, then memory_brief first for coding tasks. Use memory_impact for a touched file/symbol, memory_drift before larger edits, memory_doctrine for active project decisions, memory_agent_context for broader recall, memory_evidence for provenance, memory_auto_ingest after session logs are written, and memory_doctor before long sessions.",
        profile.as_str()
    )
}

fn mcp_server_discover(state: &McpSessionState) -> Value {
    json!({
        "supportedVersions": MCP_SUPPORTED_PROTOCOL_VERSIONS,
        "capabilities": mcp_server_capabilities(true),
        "serverInfo": {
            "name": "dukememory",
            "title": "DukeMemory",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Local-first project memory with audited retrieval and maintenance"
        },
        "instructions": mcp_server_instructions(state.profile),
    })
}

fn mcp_tool_error_result(message: String) -> Value {
    json!({
        "content":[{"type":"text","text":message.clone()}],
        "structuredContent":{"error":{"message":message}},
        "isError":true
    })
}

fn initialize_mcp_session(
    params: Option<&Value>,
    state: &mut McpSessionState,
) -> std::result::Result<Value, String> {
    let requested = params
        .and_then(|value| value.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(MCP_LATEST_PROTOCOL_VERSION);
    let selected = if MCP_LEGACY_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        MCP_LATEST_PROTOCOL_VERSION
    };
    state.protocol_version = Some(selected.to_string());
    state.initialized = false;
    state.client_key = mcp_client_key(
        params.and_then(|value| value.get("clientInfo")),
        "legacy-local",
    );
    let capabilities = if selected == MCP_LATEST_PROTOCOL_VERSION {
        mcp_server_capabilities(false)
    } else {
        json!({
            "tools": {"listChanged": false},
            "resources": {"subscribe": false, "listChanged": false}
        })
    };
    Ok(json!({
        "protocolVersion": selected,
        "capabilities": capabilities,
        "serverInfo": {
            "name": "dukememory",
            "title": "DukeMemory",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Local-first project memory with audited retrieval and maintenance"
        },
        "instructions": mcp_server_instructions(state.profile)
    }))
}

fn mcp_tools() -> Value {
    static TOOLS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_mcp_tools).clone()
}

fn build_mcp_tools() -> Value {
    let mut tools = json!([
        {"name":"memory_brief","description":"Return a tiny verified task brief","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_impact","description":"Return lightweight impact memory for a file, symbol, or topic","inputSchema":{"type":"object","properties":{"target":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["target"]}},
        {"name":"memory_budget_plan","description":"Choose the smallest useful memory budget for a task","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"scope":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_feedback","description":"Record lightweight useful/useless/missing feedback for memory reads","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"ids":{"type":"array","items":{"type":"string"}},"rating":{"type":"string"},"command":{"type":"string"},"query":{"type":"string"},"note":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["rating"]}},
        {"name":"memory_session_start","description":"Start a durable evidence-backed agent session","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"target":{"type":"string"},"scope":{"type":"string"},"runner_profile":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_session_context","description":"Load brief, impact, and doctrine into one audited agent session read","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"owner":{"type":"string"},"lease_token":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_session_claim","description":"Atomically claim an active agent session lease for one worker attempt","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"owner":{"type":"string"},"lease_secs":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","owner"]}},
        {"name":"memory_session_renew","description":"Renew an unexpired agent session lease","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"owner":{"type":"string"},"lease_token":{"type":"string"},"lease_secs":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","owner","lease_token"]}},
        {"name":"memory_session_release","description":"Release an active agent session lease without finishing","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"owner":{"type":"string"},"lease_token":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","owner","lease_token"]}},
        {"name":"memory_session_event","description":"Record a bounded retry-safe lifecycle event and refresh an active agent session heartbeat","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"event_type":{"type":"string","enum":["heartbeat","runner_selected","runner_started","runner_completed","runner_failed","validation","recovery"]},"detail":{"type":"object"},"event_id":{"type":"string"},"owner":{"type":"string"},"lease_token":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","event_type"]}},
        {"name":"memory_session_recover","description":"List or atomically claim active sessions whose heartbeat or lease is stale","inputSchema":{"type":"object","properties":{"stale_after_secs":{"type":"number"},"limit":{"type":"number"},"owner":{"type":"string"},"lease_secs":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_session_cleanup","description":"Preview or apply policy-based retention cleanup for terminal agent sessions","inputSchema":{"type":"object","properties":{"older_than_days":{"type":"number"},"statuses":{"type":"array","items":{"type":"string","enum":["completed","failed","partial","abandoned"]}},"limit":{"type":"number"},"apply":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_session_finish","description":"Finish an agent session; automatic useful feedback requires success plus explicit evidence","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"outcome":{"type":"string","enum":["success","failed","partial","abandoned"]},"summary":{"type":"string"},"changed_files":{"type":"array","items":{"type":"string"}},"validations":{"type":"array","items":{"type":"string"}},"commit":{"type":"string"},"owner":{"type":"string"},"lease_token":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","outcome","summary"]}},
        {"name":"memory_session_status","description":"Show one agent session or a filtered paginated session list","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"limit":{"type":"number"},"offset":{"type":"number"},"statuses":{"type":"array","items":{"type":"string"}},"outcomes":{"type":"array","items":{"type":"string"}},"page":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_session_trace","description":"Show recalled memory, actions, validation, and outcome for an agent session","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_runner_profiles","description":"List named Codex, Gemini, Antigravity, and local runner profiles with PATH readiness","inputSchema":{"type":"object","properties":{"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_drift","description":"Detect cheap local memory drift before coding as bounded summary by default","inputSchema":{"type":"object","properties":{"changed_only":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"}}}},
        {"name":MCP_OPERATIONS,"description":"Return the stable operation contract shared by CLI, MCP, and HTTP","inputSchema":{"type":"object","properties":{}}},
        {"name":MCP_MEMORY_ADD,"description":"Add a typed memory card","inputSchema":{"type":"object","properties":{"type":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"scope":{"type":"string"},"source":{"type":"string"},"layer":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["type","title","body"]}},
        {"name":MCP_MEMORY_REMEMBER,"description":"Remember plain text as local memory","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"type":{"type":"string"},"scope":{"type":"string"},"layer":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["text"]}},
        {"name":MCP_MEMORY_SEARCH,"description":"Search local memory with compact query-focused summaries","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_context_pack","description":"Return a compact relevant memory pack","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_rag_answer","description":"Answer a question using grounded project memory via LLM generation","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"scope":{"type":"string"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_graph_rag_answer","description":"Answer a question using 1-hop graph-expanded RAG via LLM generation","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"scope":{"type":"string"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_guided_tour","description":"Generate a pedagogical guided tour of the project memory via LLM generation","inputSchema":{"type":"object","properties":{"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":[]}},
        {"name":"memory_explain_component","description":"Perform a Deep Dive explanation of a specific memory component using its neighbors","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_onboard_guide","description":"Generate a comprehensive Onboarding Guide from the knowledge graph","inputSchema":{"type":"object","properties":{"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":[]}},
        {"name":"memory_agent_context","description":"Return agent-native context with planner defaults","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_snapshot","description":"Return compact bounded project snapshot","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_doctrine","description":"Return compact active decision doctrine by default","inputSchema":{"type":"object","properties":{"scope":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_evidence","description":"Return compact provenance for one memory card by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_auto_ingest","description":"Scan agent session files into pending inbox suggestions without duplicates as bounded summary","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"dry_run":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"}}}},
        {"name":MCP_MEMORY_GET,"description":"Get one memory card as compact summary by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_review","description":"Review stale/conflicting memory as a bounded summary","inputSchema":{"type":"object","properties":{"limit":{"type":"number"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_doctor","description":"Run compact memory health checks","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_inbox_list","description":"List pending inbox items as compact summaries by default","inputSchema":{"type":"object","properties":{"limit":{"type":"number"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_health_score","description":"Return V2 memory health score","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_explain_recall","description":"Explain why memory cards would be recalled","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_control_center_v2","description":"Aggregate health, intent, recall probes, audit, and autonomy","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_release_gate_v2","description":"Run V2 release readiness checks without build commands","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"strict":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_quality_ci","description":"CI-friendly memory quality gate","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"minimal":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_fleet_dashboard_v2","description":"Inspect all discovered project memories with V2 quality metrics","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"db":{"type":"string"}}}},
        {"name":"memory_governance_policy","description":"Inspect autonomous memory governance policy","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_status","description":"Return the stable cached DukeMemory control snapshot for agent startup","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_should_write","description":"Decide whether a durable memory write is warranted","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"memory_type":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["text"]}},
        {"name":"memory_after_task","description":"Return compact after-task memory maintenance guidance","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_project_health","description":"Return compact project memory health and role profile","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}
    ]);
    if let Some(items) = tools.as_array_mut() {
        items.extend([
            json!({"name":"memory_recall","description":"Return compressed recall, including recent/as-of/changed-since temporal modes","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"recent":{"type":"boolean"},"as_of":{"type":"string"},"as_of_days_ago":{"type":"number"},"changed_since":{"type":"string"},"changed_since_days":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}}),
            json!({"name":"memory_upload","description":"Review a local text/markdown/json/csv file as inbox-first memory candidates","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["input"]}}),
            json!({"name":"memory_rag_ingest","description":"Index text/code files as chunked local RAG sources; dry-run unless apply=true; set embed=true to refresh semantic chunk embeddings after apply","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"apply":{"type":"boolean"},"embed":{"type":"boolean"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"chunk_chars":{"type":"number"},"overlap_chars":{"type":"number"},"max_file_bytes":{"type":"number"},"max_files":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["input"]}}),
            json!({"name":"memory_rag_sources","description":"Inspect indexed RAG source freshness, stale files, chunk counts, and semantic chunk embedding freshness","inputSchema":{"type":"object","properties":{"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_rag_eval","description":"Run RAG eval with matrix, grounded-answer, retrieval tuning, and optional baseline write","inputSchema":{"type":"object","properties":{"scope":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"write_baseline":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_graph_rag_eval","description":"Run deterministic graph-RAG eval for connected memory relationships and grounded graph answers","inputSchema":{"type":"object","properties":{"scope":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_advanced_eval","description":"Audit explicit causal edges, retrieval-poisoning signals, global graph coverage, and bitemporal consistency","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_auto_ranking_tune","description":"Explain or apply the selected memory retrieval ranking profile from live QA and RAG eval signals","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_memanto_gap","description":"Report Memanto-style capability coverage for dukememory","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_timeline","description":"Show one memory card timeline with audit events and real agent reads","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}}),
            json!({"name":"memory_observe","description":"Record an evidence-backed bitemporal observation","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"target_memory_id":{"type":"string"},"kind":{"type":"string","enum":["asserted","verified","contradicted","superseded","file_changed","retrieved","outcome"]},"statement":{"type":"string"},"evidence_kind":{"type":"string"},"evidence_ref":{"type":"string"},"confidence":{"type":"number","minimum":0.0,"maximum":1.0},"valid_from":{"type":"number"},"valid_to":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id","kind","statement","evidence_kind","evidence_ref"]}}),
            json!({"name":"memory_observations","description":"List evidence observations as-of valid and knowledge time","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"valid_at":{"type":"number"},"known_at":{"type":"number"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}}),
            json!({"name":"memory_temporal_graph","description":"Read the memory graph as-of valid and knowledge time","inputSchema":{"type":"object","properties":{"valid_at":{"type":"number"},"known_at":{"type":"number"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_conflict_review","description":"Review duplicate, stale, superseded, and contradiction-prone memory groups","inputSchema":{"type":"object","properties":{"stale_days":{"type":"number"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_effectiveness_v2","description":"Measure memory usefulness with influence, waste, and semantic-read signals","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_recall_baselines","description":"Inspect or write guarded recall benchmark baselines","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_conflict_apply","description":"Dry-run or apply guarded reversible memory conflict-review actions","inputSchema":{"type":"object","properties":{"stale_days":{"type":"number"},"limit":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_mcp_surface_v3","description":"Inspect the MCP V3 memory tool surface","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"}}}}),
            json!({"name":"memory_mcp_discipline_v3","description":"Verify or record MCP V3 memory discipline","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_fleet_quality","description":"Inspect V3 quality across discovered project memories","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"db":{"type":"string"}}}}),
            json!({"name":"memory_release_gate_v3","description":"Gate releases with effectiveness, baselines, conflicts, MCP V3, fleet visibility, and an explicit RAG runtime profile","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"rag_profile":{"type":"string","enum":["deployment","canonical","offline"]},"strict":{"type":"boolean"},"run":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
        ]);
        for tool in items {
            enrich_mcp_tool_definition(tool);
        }
    }
    tools
}

fn mcp_list_tools(
    params: Option<&Value>,
    state: &McpSessionState,
) -> std::result::Result<Value, String> {
    let mut tools = mcp_tools()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| mcp_profile_includes(state.profile, name))
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| {
        left.get("name")
            .and_then(Value::as_str)
            .cmp(&right.get("name").and_then(Value::as_str))
    });
    paginated_mcp_values(
        "tools",
        state.profile.as_str(),
        tools,
        params,
        state.page_size,
    )
}

fn mcp_profile_includes(profile: McpProfile, name: &str) -> bool {
    const CORE: &[&str] = &[
        "memory_add",
        "memory_after_task",
        "memory_agent_context",
        "memory_brief",
        "memory_budget_plan",
        "memory_context_pack",
        "memory_doctrine",
        "memory_doctor",
        "memory_drift",
        "memory_evidence",
        "memory_feedback",
        "memory_get",
        "memory_impact",
        "memory_operations",
        "memory_project_health",
        "memory_recall",
        "memory_remember",
        "memory_search",
        "memory_should_write",
        "memory_status",
    ];
    const STANDARD_EXTRA: &[&str] = &[
        "memory_advanced_eval",
        "memory_auto_ingest",
        "memory_effectiveness_v2",
        "memory_graph_rag_answer",
        "memory_graph_rag_eval",
        "memory_health_score",
        "memory_inbox_list",
        "memory_observations",
        "memory_rag_answer",
        "memory_rag_eval",
        "memory_rag_ingest",
        "memory_rag_sources",
        "memory_release_gate_v2",
        "memory_review",
        "memory_session_claim",
        "memory_session_cleanup",
        "memory_session_context",
        "memory_session_event",
        "memory_session_finish",
        "memory_session_recover",
        "memory_session_release",
        "memory_session_renew",
        "memory_session_start",
        "memory_session_status",
        "memory_session_trace",
        "memory_snapshot",
        "memory_timeline",
        "memory_temporal_graph",
        "memory_observe",
        "memory_upload",
    ];
    match profile {
        McpProfile::Core => CORE.contains(&name),
        McpProfile::Standard => CORE.contains(&name) || STANDARD_EXTRA.contains(&name),
        McpProfile::Full => true,
    }
}

fn paginated_mcp_values(
    field: &str,
    namespace: &str,
    values: Vec<Value>,
    params: Option<&Value>,
    page_size: usize,
) -> std::result::Result<Value, String> {
    let prefix = format!("{field}:{namespace}:");
    let offset = parse_mcp_cursor(params, &prefix)?;
    if offset > values.len() {
        return Err("cursor is outside the current result set".to_string());
    }
    let effective_page_size = if page_size == 0 {
        values.len().max(1)
    } else {
        page_size.clamp(1, 100)
    };
    let end = offset.saturating_add(effective_page_size).min(values.len());
    let page = values[offset..end].to_vec();
    let mut result = serde_json::Map::new();
    result.insert(field.to_string(), Value::Array(page));
    if end < values.len() {
        result.insert(
            "nextCursor".to_string(),
            Value::String(format!("{prefix}{end}")),
        );
    }
    Ok(Value::Object(result))
}

fn parse_mcp_cursor(params: Option<&Value>, prefix: &str) -> std::result::Result<usize, String> {
    let Some(cursor) = params
        .and_then(|value| value.get("cursor"))
        .and_then(Value::as_str)
    else {
        return Ok(0);
    };
    cursor
        .strip_prefix(prefix)
        .ok_or_else(|| "invalid or stale cursor".to_string())?
        .parse::<usize>()
        .map_err(|_| "invalid or stale cursor".to_string())
}

fn mcp_list_resources(
    params: Option<&Value>,
    state: &McpSessionState,
) -> std::result::Result<Value, String> {
    let resources = vec![
        json!({
            "uri": "dukememory://project/status",
            "name": "project-status",
            "title": "Project memory status",
            "description": "Schema and card counts for the selected DukeMemory project",
            "mimeType": "application/json"
        }),
        json!({
            "uri": "dukememory://project/doctrine",
            "name": "project-doctrine",
            "title": "Active project doctrine",
            "description": "Active project decisions, supersession chains, and conflicts",
            "mimeType": "application/json"
        }),
    ];
    paginated_mcp_values("resources", "project", resources, params, state.page_size)
}

fn mcp_resource_templates() -> Value {
    json!({
        "resourceTemplates": [{
            "uriTemplate": "dukememory://memory/{id}",
            "name": "memory-card",
            "title": "Memory card by id",
            "description": "One DukeMemory card with its links and lifecycle metadata",
            "mimeType": "application/json"
        }]
    })
}

fn mcp_read_resource(db: &Path, params: Option<&Value>) -> std::result::Result<Value, String> {
    let uri = params
        .and_then(|value| value.get("uri"))
        .and_then(Value::as_str)
        .ok_or_else(|| "missing resource uri".to_string())?;
    let conn = open_db(db).map_err(|error| error.to_string())?;
    let payload = match uri {
        "dukememory://project/status" => {
            let memories: i64 = conn
                .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
                .map_err(|error| error.to_string())?;
            let active: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE status = 'active'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            json!({
                "schema": schema_version(&conn).map_err(|error| error.to_string())?,
                "memories": memories,
                "active": active,
                "db": db.display().to_string(),
            })
        }
        "dukememory://project/doctrine" => {
            serde_json::to_value(doctrine_report(&conn, None).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?
        }
        _ => {
            let id = uri
                .strip_prefix("dukememory://memory/")
                .filter(|id| !id.is_empty() && !id.contains('/'))
                .ok_or_else(|| format!("unknown resource uri: {uri}"))?;
            serde_json::to_value(
                get_memory_with_links(&conn, id).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
        }
    };
    let text = serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?;
    Ok(json!({
        "contents": [{"uri": uri, "mimeType": "application/json", "text": text}]
    }))
}

fn handle_mcp_call(
    db: &Path,
    params: Value,
    state: &McpSessionState,
    modern: bool,
    modern_tasks: bool,
    owner_key: &str,
) -> std::result::Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    validate_mcp_tool_arguments(name, &args)?;
    if !mcp_profile_includes(state.profile, name) {
        return Err(format!(
            "tool {name} is not available in the {} MCP profile",
            state.profile.as_str()
        ));
    }
    if modern && params.get("task").is_some() {
        return Err(
            "the 2026 Tasks extension is server-directed; remove the legacy task parameter"
                .to_string(),
        );
    }
    if modern
        && modern_tasks
        && mcp_tool_supports_tasks(name)
        && mcp_task_call_is_read_only(name, &args)
    {
        return mcp_start_task(db, params, state, owner_key, "extension");
    }
    if !modern && params.get("task").is_some() {
        if !mcp_legacy_tasks_enabled(state) {
            return Err("task-augmented calls require MCP protocol 2025-11-25".to_string());
        }
        if !mcp_tool_supports_tasks(name) {
            return Err(format!("tool {name} does not support task execution"));
        }
        return mcp_start_task(db, params, state, owner_key, "legacy");
    }
    Ok(handle_mcp_tool_call(db, params).unwrap_or_else(mcp_tool_error_result))
}

fn validate_mcp_tool_arguments(name: &str, arguments: &Value) -> std::result::Result<(), String> {
    let tools = mcp_tools();
    let tool = tools
        .as_array()
        .into_iter()
        .flatten()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| format!("unknown tool: {name}"))?;
    let schema = tool
        .get("inputSchema")
        .ok_or_else(|| format!("tool {name} has no input schema"))?;
    validate_mcp_json_value(arguments, schema, "arguments")
}

fn validate_mcp_json_value(
    value: &Value,
    schema: &Value,
    path: &str,
) -> std::result::Result<(), String> {
    let expected_type = schema.get("type").and_then(Value::as_str);
    let type_ok = match expected_type {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        None => true,
        Some(other) => return Err(format!("unsupported schema type {other} at {path}")),
    };
    if !type_ok {
        return Err(format!(
            "{path} must be {}",
            expected_type.unwrap_or("valid")
        ));
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err(format!("{path} is not one of the allowed values"));
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| length < minimum)
        {
            return Err(format!("{path} is shorter than the allowed minimum"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|maximum| length > maximum)
        {
            return Err(format!("{path} exceeds the allowed length"));
        }
    }
    if expected_type == Some("integer") {
        let number = value
            .as_i64()
            .map(i128::from)
            .or_else(|| value.as_u64().map(i128::from))
            .unwrap_or_default();
        if schema
            .get("minimum")
            .and_then(Value::as_i64)
            .is_some_and(|minimum| number < i128::from(minimum))
        {
            return Err(format!("{path} is below the allowed minimum"));
        }
        if schema
            .get("maximum")
            .and_then(Value::as_i64)
            .is_some_and(|maximum| number > i128::from(maximum))
        {
            return Err(format!("{path} exceeds the allowed maximum"));
        }
    }
    if expected_type == Some("number") {
        let number = value.as_f64().unwrap_or_default();
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|minimum| number < minimum)
        {
            return Err(format!("{path} is below the allowed minimum"));
        }
        if schema
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|maximum| number > maximum)
        {
            return Err(format!("{path} exceeds the allowed maximum"));
        }
    }
    if let Some(items) = value.as_array() {
        if schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|maximum| items.len() as u64 > maximum)
        {
            return Err(format!("{path} contains too many items"));
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_mcp_json_value(item, item_schema, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(field) {
                    return Err(format!("{path}.{field} is required"));
                }
            }
        }
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            for field in object.keys() {
                if !properties.contains_key(field) {
                    return Err(format!("{path}.{field} is not allowed"));
                }
            }
        }
        for (field, property_schema) in properties {
            if let Some(field_value) = object.get(&field) {
                validate_mcp_json_value(field_value, &property_schema, &format!("{path}.{field}"))?;
            }
        }
    }
    Ok(())
}

fn enrich_mcp_tool_definition(tool: &mut Value) {
    let Some(object) = tool.as_object_mut() else {
        return;
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let operation = operation_for_mcp(&name);
    let generated_title = name
        .trim_start_matches("memory_")
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    object.insert(
        "title".to_string(),
        Value::String(
            operation
                .map(|spec| spec.summary.to_string())
                .unwrap_or(generated_title),
        ),
    );
    if let Some(spec) = operation {
        object.insert(
            "x-operationId".to_string(),
            Value::String(spec.id.to_string()),
        );
        object.insert(
            "x-stability".to_string(),
            Value::String(spec.stability.as_str().to_string()),
        );
        object.insert(
            "x-authorizationScope".to_string(),
            Value::String(spec.authorization.as_str().to_string()),
        );
        object.insert(
            "x-supportsDryRun".to_string(),
            Value::Bool(spec.supports_dry_run),
        );
        if let Some(schema) = object.get_mut("inputSchema").and_then(Value::as_object_mut) {
            schema.insert(
                "$id".to_string(),
                Value::String(spec.input_schema.to_string()),
            );
        }
    }
    if let Some(schema) = object.get_mut("inputSchema").and_then(Value::as_object_mut) {
        harden_mcp_input_schema(schema);
    }
    object.insert(
        "outputSchema".to_string(),
        operation.map_or_else(
            || json!({"type":"object","additionalProperties":true}),
            |spec| json!({"$id":spec.output_schema,"type":"object","additionalProperties":true}),
        ),
    );
    object.insert("annotations".to_string(), mcp_tool_annotations(&name));
    if mcp_tool_supports_tasks(&name) {
        object.insert("execution".to_string(), json!({"taskSupport": "optional"}));
    }
}

fn harden_mcp_input_schema(schema: &mut serde_json::Map<String, Value>) {
    schema.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    schema.insert("additionalProperties".to_string(), Value::Bool(false));
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    for (name, property) in properties {
        let Some(property) = property.as_object_mut() else {
            continue;
        };
        if property.get("type").and_then(Value::as_str) == Some("number") && name != "confidence" {
            property.insert("type".to_string(), Value::String("integer".to_string()));
        }
        match property.get("type").and_then(Value::as_str) {
            Some("integer") => apply_mcp_integer_constraints(name, property),
            Some("number") if name == "confidence" => {
                property.insert("minimum".to_string(), json!(0.0));
                property.insert("maximum".to_string(), json!(1.0));
            }
            Some("string") => {
                if required.contains(name) {
                    property.insert("minLength".to_string(), json!(1));
                }
                property.insert(
                    "maxLength".to_string(),
                    json!(match name.as_str() {
                        "body" | "text" => 1_000_000,
                        "query" | "task" | "summary" | "note" => 50_000,
                        "root" | "project_root" | "db" | "input" | "endpoint" => 4_096,
                        _ => 20_000,
                    }),
                );
                apply_mcp_string_enum(name, property);
            }
            Some("array") => {
                property.insert("maxItems".to_string(), json!(1_000));
            }
            _ => {}
        }
    }
}

fn apply_mcp_integer_constraints(name: &str, property: &mut serde_json::Map<String, Value>) {
    let (minimum, maximum) = match name {
        "offset" | "overlap_chars" => (0, 1_000_000),
        "since_days" | "older_than_days" | "stale_days" | "as_of_days_ago"
        | "changed_since_days" => (0, 36_500),
        "lease_secs" | "stale_after_secs" => (1, 86_400),
        "limit" | "max_files" => (1, 10_000),
        "chunk_chars" => (256, 1_000_000),
        "budget" | "max_chars" | "max_file_bytes" => (1, 16_777_216),
        _ => (0, i64::MAX),
    };
    property.insert("minimum".to_string(), json!(minimum));
    property.insert("maximum".to_string(), json!(maximum));
}

fn apply_mcp_string_enum(name: &str, property: &mut serde_json::Map<String, Value>) {
    let values: Option<&[&str]> = match name {
        "rating" => Some(&["useful", "useless", "missing"]),
        "type" | "memory_type" => Some(&[
            "product_goal",
            "user_preference",
            "decision",
            "design_note",
            "known_issue",
            "command",
            "task_state",
            "domain_fact",
            "constraint",
            "note",
        ]),
        _ => None,
    };
    if let Some(values) = values {
        property.insert("enum".to_string(), json!(values));
    }
}

fn mcp_tool_annotations(name: &str) -> Value {
    if let Some(operation) = operation_for_mcp(name) {
        return json!({
            "readOnlyHint": !operation.mutation,
            "destructiveHint": operation.destructive,
            "idempotentHint": operation.idempotent,
            "openWorldHint": operation.open_world,
        });
    }
    let mutating = matches!(
        name,
        "memory_add"
            | "memory_remember"
            | "memory_feedback"
            | "memory_session_start"
            | "memory_session_claim"
            | "memory_session_renew"
            | "memory_session_release"
            | "memory_session_event"
            | "memory_session_recover"
            | "memory_session_cleanup"
            | "memory_session_finish"
            | "memory_auto_ingest"
            | "memory_upload"
            | "memory_rag_ingest"
            | "memory_rag_eval"
            | "memory_observe"
            | "memory_auto_ranking_tune"
            | "memory_recall_baselines"
            | "memory_conflict_apply"
            | "memory_mcp_discipline_v3"
            | "memory_release_gate_v3"
    );
    let destructive = matches!(name, "memory_session_cleanup" | "memory_conflict_apply");
    let open_world = matches!(
        name,
        "memory_auto_ingest"
            | "memory_upload"
            | "memory_rag_ingest"
            | "memory_rag_answer"
            | "memory_graph_rag_answer"
            | "memory_guided_tour"
            | "memory_explain_component"
            | "memory_onboard_guide"
    );
    json!({
        "readOnlyHint": !mutating,
        "destructiveHint": destructive,
        "idempotentHint": !mutating,
        "openWorldHint": open_world,
    })
}

fn handle_mcp_tool_call(db: &Path, params: Value) -> std::result::Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let selected_db = mcp_selected_db(db, &args)?;
    let conn = open_db(&selected_db).map_err(|err| err.to_string())?;
    let memory_app = MemoryApplication::new(MemoryStore::new(&conn));
    let selected_root = mcp_selected_root(&selected_db);
    let text = match name {
        "memory_session_start" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let target = json_string(&args, "target");
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            validate_scope(&scope).map_err(|err| err.to_string())?;
            let runner_profile = json_string(&args, "runner_profile");
            let session = start_agent_session(
                &conn,
                &task,
                target.as_deref(),
                &scope,
                runner_profile.as_deref(),
                &selected_root,
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&session).map_err(|err| err.to_string())?
        }
        "memory_session_context" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(4000);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let owner = json_string(&args, "owner");
            let lease_token = json_string(&args, "lease_token");
            let report = agent_session_context(
                &conn,
                &id,
                limit,
                max_chars,
                &provider,
                &endpoint,
                &model,
                owner.as_deref(),
                lease_token.as_deref(),
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_session_claim" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let owner = json_string(&args, "owner").ok_or_else(|| "missing owner".to_string())?;
            let report = claim_agent_session(
                &conn,
                &id,
                &owner,
                json_usize(&args, "lease_secs").unwrap_or(120) as u64,
                false,
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_session_renew" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let owner = json_string(&args, "owner").ok_or_else(|| "missing owner".to_string())?;
            let lease_token = json_string(&args, "lease_token")
                .ok_or_else(|| "missing lease_token".to_string())?;
            let report = renew_agent_session_lease(
                &conn,
                &id,
                &owner,
                &lease_token,
                json_usize(&args, "lease_secs").unwrap_or(120) as u64,
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_session_release" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let owner = json_string(&args, "owner").ok_or_else(|| "missing owner".to_string())?;
            let lease_token = json_string(&args, "lease_token")
                .ok_or_else(|| "missing lease_token".to_string())?;
            let session = release_agent_session_lease(&conn, &id, &owner, &lease_token)
                .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&session).map_err(|err| err.to_string())?
        }
        "memory_session_event" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let event_type =
                json_string(&args, "event_type").ok_or_else(|| "missing event_type".to_string())?;
            let detail = args.get("detail").cloned().unwrap_or_else(|| json!({}));
            let event_id = json_string(&args, "event_id");
            let owner = json_string(&args, "owner");
            let lease_token = json_string(&args, "lease_token");
            let session = record_agent_session_event(
                &conn,
                &id,
                &event_type,
                &detail,
                event_id.as_deref(),
                owner.as_deref(),
                lease_token.as_deref(),
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&session).map_err(|err| err.to_string())?
        }
        "memory_session_recover" => {
            let stale_after_secs = json_usize(&args, "stale_after_secs").unwrap_or(300) as u64;
            let limit = json_usize(&args, "limit").unwrap_or(20);
            if let Some(owner) = json_string(&args, "owner") {
                let claims = claim_recoverable_agent_sessions(
                    &conn,
                    stale_after_secs,
                    limit,
                    &owner,
                    json_usize(&args, "lease_secs").unwrap_or(120) as u64,
                )
                .map_err(|err| err.to_string())?;
                serde_json::to_string_pretty(&claims).map_err(|err| err.to_string())?
            } else {
                let sessions = recoverable_agent_sessions(&conn, stale_after_secs, limit)
                    .map_err(|err| err.to_string())?;
                serde_json::to_string_pretty(&sessions).map_err(|err| err.to_string())?
            }
        }
        "memory_session_cleanup" => {
            let statuses = json_string_array(&args, "statuses");
            let policy =
                agent_session_config_for_root(&selected_root).map_err(|err| err.to_string())?;
            let report = cleanup_agent_sessions_with_policy(
                &conn,
                &policy,
                &statuses,
                json_usize(&args, "older_than_days").map(|value| value as i64),
                json_usize(&args, "limit").unwrap_or(100),
                args.get("apply").and_then(Value::as_bool).unwrap_or(false),
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_session_finish" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let summary =
                json_string(&args, "summary").ok_or_else(|| "missing summary".to_string())?;
            let outcome = match json_string(&args, "outcome").as_deref() {
                Some("success") => AgentSessionOutcome::Success,
                Some("failed") => AgentSessionOutcome::Failed,
                Some("partial") => AgentSessionOutcome::Partial,
                Some("abandoned") => AgentSessionOutcome::Abandoned,
                _ => {
                    return Err(
                        "invalid outcome: expected success, failed, partial, or abandoned"
                            .to_string(),
                    );
                }
            };
            let changed_files = json_string_array(&args, "changed_files");
            let validations = json_string_array(&args, "validations");
            let commit = json_string(&args, "commit");
            let owner = json_string(&args, "owner");
            let lease_token = json_string(&args, "lease_token");
            let report = finish_agent_session(
                &conn,
                &id,
                outcome,
                &summary,
                &changed_files,
                &validations,
                commit.as_deref(),
                owner.as_deref(),
                lease_token.as_deref(),
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_session_status" => {
            let statuses = json_string_array(&args, "statuses");
            let outcomes = json_string_array(&args, "outcomes");
            let offset = json_usize(&args, "offset").unwrap_or(0);
            let page = args.get("page").and_then(Value::as_bool).unwrap_or(false);
            let policy =
                agent_session_config_for_root(&selected_root).map_err(|err| err.to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(policy.default_page_size);
            let value = if let Some(id) = json_string(&args, "id") {
                serde_json::to_value(vec![
                    get_agent_session(&conn, &id).map_err(|err| err.to_string())?,
                ])
                .map_err(|err| err.to_string())?
            } else if page || offset > 0 || !statuses.is_empty() || !outcomes.is_empty() {
                serde_json::to_value(
                    list_agent_sessions_page(&conn, &statuses, &outcomes, offset, limit)
                        .map_err(|err| err.to_string())?,
                )
                .map_err(|err| err.to_string())?
            } else {
                serde_json::to_value(
                    list_agent_sessions(&conn, limit).map_err(|err| err.to_string())?,
                )
                .map_err(|err| err.to_string())?
            };
            serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?
        }
        "memory_session_trace" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let trace = agent_session_trace(&conn, &id).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&trace).map_err(|err| err.to_string())?
        }
        "memory_runner_profiles" => {
            let profiles = runner_profiles_status(&selected_root).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&profiles).map_err(|err| err.to_string())?
        }
        MCP_OPERATIONS => {
            serde_json::to_string_pretty(OPERATION_CATALOG).map_err(|err| err.to_string())?
        }
        MCP_MEMORY_ADD => {
            let memory_type = json_string(&args, "type").unwrap_or_else(|| "note".to_string());
            let title = json_string(&args, "title").ok_or_else(|| "missing title".to_string())?;
            let body = json_string(&args, "body").ok_or_else(|| "missing body".to_string())?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            validate_scope(&scope).map_err(|err| err.to_string())?;
            reject_sensitive(&title, &body, false).map_err(|err| err.to_string())?;
            memory_app
                .create(AddMemory {
                    id: None,
                    memory_type: memory_type
                        .parse::<MemoryType>()
                        .map_err(|err| err.to_string())?,
                    title,
                    body,
                    scope: scope
                        .parse::<MemoryScope>()
                        .map_err(|err| err.to_string())?,
                    status: MemoryStatus::Active,
                    source: json_string(&args, "source"),
                    supersedes: None,
                    confidence: 1.0,
                    layer: json_string(&args, "layer"),
                    links: Vec::new(),
                    allow_sensitive: false,
                })
                .map_err(|err| err.to_string())?
        }
        MCP_MEMORY_REMEMBER => {
            let text = json_string(&args, "text").ok_or_else(|| "missing text".to_string())?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            validate_scope(&scope).map_err(|err| err.to_string())?;
            let memory_type = json_string(&args, "type").unwrap_or_else(|| "note".to_string());
            reject_sensitive(&truncate_words(&text, 8), &text, false)
                .map_err(|err| err.to_string())?;
            memory_app
                .create(AddMemory {
                    id: None,
                    memory_type: memory_type
                        .parse::<MemoryType>()
                        .map_err(|err| err.to_string())?,
                    title: truncate_words(&text, 8),
                    body: text,
                    scope: scope
                        .parse::<MemoryScope>()
                        .map_err(|err| err.to_string())?,
                    status: MemoryStatus::Active,
                    source: Some("mcp".to_string()),
                    supersedes: None,
                    confidence: 0.8,
                    layer: json_string(&args, "layer"),
                    links: Vec::new(),
                    allow_sensitive: false,
                })
                .map_err(|err| err.to_string())?
        }
        MCP_MEMORY_SEARCH => {
            let started = Instant::now();
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let effective_limit = mcp_effective_limit(limit, max_chars);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let (rows, semantic_used) = search_rows_with_semantic_fallback(
                &conn,
                SearchRowsRequest {
                    query: &query,
                    types: &[],
                    statuses: &["active".to_string()],
                    scope: None,
                    limit: effective_limit,
                    budget: max_chars,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )
            .map_err(|err| err.to_string())?;
            let quality_signals = retrieval_feedback_signals(&conn, 30).unwrap_or_default();
            let mut rows = filter_query_useless_memories(rows, &query, &quality_signals);
            rows.truncate(effective_limit);
            let (rendered, used_ids) = compact_mcp_search_response(&rows, &query, max_chars)
                .map_err(|err| err.to_string())?;
            log_read_event(
                &conn,
                ReadEventInput {
                    command: "memory_search",
                    query: &query,
                    ids: &used_ids,
                    semantic_used,
                    result_count: used_ids.len(),
                    budget: max_chars,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            )
            .map_err(|err| err.to_string())?;
            rendered
        }
        "memory_rag_answer" => {
            let started = Instant::now();
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let budget = json_usize(&args, "budget").unwrap_or(3000);
            let scope = json_string(&args, "scope");
            let embed_provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let embed_endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let embed_model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());

            let gen_provider = json_string(&args, "gen_provider").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_PROVIDER").unwrap_or_else(|_| "ollama".to_string())
            });
            let gen_endpoint = json_string(&args, "gen_endpoint").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string())
            });
            let gen_model = json_string(&args, "gen_model").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_MODEL").unwrap_or_else(|_| "llama3".to_string())
            });

            let report = memory_rag_report(
                &conn,
                &query,
                scope.as_deref(),
                limit,
                budget,
                &embed_provider,
                &embed_endpoint,
                &embed_model,
                &gen_provider,
                &gen_endpoint,
                &gen_model,
            )
            .map_err(|err| err.to_string())?;

            log_read_event(
                &conn,
                ReadEventInput {
                    command: "memory_rag_answer",
                    query: &query,
                    ids: &report.citations,
                    semantic_used: report.semantic_used,
                    result_count: report.citations.len(),
                    budget,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            )
            .map_err(|err| err.to_string())?;

            serde_json::to_string(&report).map_err(|err| err.to_string())?
        }
        "memory_graph_rag_answer" => {
            let started = Instant::now();
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let budget = json_usize(&args, "budget").unwrap_or(3000);
            let scope = json_string(&args, "scope");
            let embed_provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let embed_endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let embed_model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());

            let gen_provider = json_string(&args, "gen_provider").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_PROVIDER").unwrap_or_else(|_| "ollama".to_string())
            });
            let gen_endpoint = json_string(&args, "gen_endpoint").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string())
            });
            let gen_model = json_string(&args, "gen_model").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_MODEL").unwrap_or_else(|_| "llama3".to_string())
            });

            let config = crate::runtime_config::GenerationConfig {
                provider: gen_provider,
                endpoint: gen_endpoint,
                model: gen_model,
            };

            let report = crate::app::graph_rag::compute_graph_rag(
                &conn,
                &query,
                scope.as_deref(),
                limit,
                budget,
                &config,
                &embed_provider,
                &embed_endpoint,
                &embed_model,
            )
            .map_err(|err| err.to_string())?;

            log_read_event(
                &conn,
                ReadEventInput {
                    command: "memory_graph_rag_answer",
                    query: &query,
                    ids: &report.citations,
                    semantic_used: report.semantic_used,
                    result_count: report.citations.len(),
                    budget,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            )
            .map_err(|err| err.to_string())?;

            serde_json::to_string(&report).map_err(|err| err.to_string())?
        }
        "memory_guided_tour" => {
            let gen_provider = json_string(&args, "gen_provider").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_PROVIDER").unwrap_or_else(|_| "ollama".to_string())
            });
            let gen_endpoint = json_string(&args, "gen_endpoint").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string())
            });
            let gen_model = json_string(&args, "gen_model").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_MODEL").unwrap_or_else(|_| "llama3".to_string())
            });

            let topology_result =
                crate::app::topology::compute_topology(&conn).map_err(|err| err.to_string())?;
            let config = crate::runtime_config::GenerationConfig {
                provider: gen_provider,
                endpoint: gen_endpoint,
                model: gen_model,
            };
            let narrative =
                crate::app::generation::generate_tour_narrative(&config, &topology_result)
                    .map_err(|err| err.to_string())?;

            serde_json::to_string(&serde_json::json!({ "tour": narrative }))
                .map_err(|err| err.to_string())?
        }
        "memory_explain_component" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let gen_provider = json_string(&args, "gen_provider").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_PROVIDER").unwrap_or_else(|_| "ollama".to_string())
            });
            let gen_endpoint = json_string(&args, "gen_endpoint").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string())
            });
            let gen_model = json_string(&args, "gen_model").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_MODEL").unwrap_or_else(|_| "llama3".to_string())
            });

            let config = crate::runtime_config::GenerationConfig {
                provider: gen_provider,
                endpoint: gen_endpoint,
                model: gen_model,
            };

            let explanation = crate::app::explain::explain_component(&conn, &id, &config)
                .map_err(|err| err.to_string())?;

            serde_json::to_string(&serde_json::json!({ "explanation": explanation }))
                .map_err(|err| err.to_string())?
        }
        "memory_onboard_guide" => {
            let gen_provider = json_string(&args, "gen_provider").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_PROVIDER").unwrap_or_else(|_| "ollama".to_string())
            });
            let gen_endpoint = json_string(&args, "gen_endpoint").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string())
            });
            let gen_model = json_string(&args, "gen_model").unwrap_or_else(|| {
                std::env::var("DUKEMEMORY_GEN_MODEL").unwrap_or_else(|_| "llama3".to_string())
            });

            let config = crate::runtime_config::GenerationConfig {
                provider: gen_provider,
                endpoint: gen_endpoint,
                model: gen_model,
            };

            let guide = crate::app::onboard::generate_onboarding_guide(&conn, &config)
                .map_err(|err| err.to_string())?;

            serde_json::to_string(&serde_json::json!({ "onboard": guide }))
                .map_err(|err| err.to_string())?
        }
        "memory_context_pack" => {
            let started = Instant::now();
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(4000);
            let effective_limit = mcp_effective_limit(limit, max_chars);
            let statuses = ["active".to_string(), "uncertain".to_string()];
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let mut rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &[],
                    statuses: &statuses,
                    scope: None,
                    limit: effective_limit,
                    include_recent: 3,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            let semantic_used = append_semantic_context_rows(
                &conn,
                &mut rows,
                SemanticContextRequest {
                    task: &task,
                    limit: effective_limit,
                    budget: max_chars,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            let (rendered, used_ids) =
                render_context_pack_for_task_with_used_ids(&conn, &rows, max_chars, &task)
                    .map_err(|err| err.to_string())?;
            log_mcp_context_read(
                &conn,
                "memory_context_pack",
                &task,
                &used_ids,
                semantic_used,
                max_chars,
                started,
            )
            .map_err(|err| err.to_string())?;
            rendered
        }
        "memory_agent_context" => {
            let started = Instant::now();
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(5000);
            let effective_limit = mcp_effective_limit(limit, max_chars);
            let statuses = ["active".to_string(), "uncertain".to_string()];
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let mut rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &[],
                    statuses: &statuses,
                    scope: None,
                    limit: effective_limit,
                    include_recent: 4,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            let semantic_used = append_semantic_context_rows(
                &conn,
                &mut rows,
                SemanticContextRequest {
                    task: &task,
                    limit: effective_limit,
                    budget: max_chars,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            let (rendered, used_ids) =
                render_context_pack_for_task_with_used_ids(&conn, &rows, max_chars, &task)
                    .map_err(|err| err.to_string())?;
            log_mcp_context_read(
                &conn,
                "memory_agent_context",
                &task,
                &used_ids,
                semantic_used,
                max_chars,
                started,
            )
            .map_err(|err| err.to_string())?;
            rendered
        }
        "memory_snapshot" => {
            let started = Instant::now();
            let query = json_string(&args, "query").unwrap_or_default();
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let effective_limit = mcp_effective_limit(limit, max_chars);
            let query_filter = (!query.trim().is_empty()).then_some(query.as_str());
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let fetch_limit = if query_filter.is_some() {
                mcp_snapshot_query_candidate_limit(effective_limit, max_chars)
            } else {
                effective_limit
            };
            let mut rows = query_memories(
                &conn,
                query_filter,
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                fetch_limit,
            )
            .map_err(|err| err.to_string())?;
            if query.trim().is_empty() {
                let ids = memory_row_ids(&rows);
                log_mcp_context_read(
                    &conn,
                    "memory_snapshot",
                    "",
                    &ids,
                    false,
                    max_chars,
                    started,
                )
                .map_err(|err| err.to_string())?;
                render_context_pack(&conn, &rows, max_chars).map_err(|err| err.to_string())?
            } else {
                let quality_signals = retrieval_feedback_signals(&conn, 30).unwrap_or_default();
                rows = filter_query_useless_memories(rows, &query, &quality_signals);
                rows.truncate(effective_limit);
                let semantic_used = append_semantic_context_rows(
                    &conn,
                    &mut rows,
                    SemanticContextRequest {
                        task: &query,
                        limit: effective_limit,
                        budget: max_chars,
                        provider: &provider,
                        endpoint: &endpoint,
                        model: &model,
                        rules: None,
                    },
                )
                .map_err(|err| err.to_string())?;
                let ids = memory_row_ids(&rows);
                log_mcp_context_read(
                    &conn,
                    "memory_snapshot",
                    &query,
                    &ids,
                    semantic_used,
                    max_chars,
                    started,
                )
                .map_err(|err| err.to_string())?;
                render_context_pack_for_task(&conn, &rows, max_chars, &query)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_budget_plan" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let scope = mcp_memory_scope(&args);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(800);
            let plan =
                budget_plan(&conn, &task, scope.as_deref()).map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&plan, max_chars, &["reasons"])
                .map_err(|err| err.to_string())?
        }
        "memory_feedback" => {
            let mut ids = json_string_array(&args, "ids");
            if let Some(id) = json_string(&args, "id").filter(|id| !id.trim().is_empty()) {
                ids.push(id);
            }
            ids.retain(|id| !id.trim().is_empty());
            ids.sort();
            ids.dedup();

            let rating_text =
                json_string(&args, "rating").ok_or_else(|| "missing rating".to_string())?;
            let rating = match rating_text.as_str() {
                "useful" => FeedbackRating::Useful,
                "useless" => FeedbackRating::Useless,
                "missing" => FeedbackRating::Missing,
                _ => return Err("invalid rating: expected useful, useless, or missing".to_string()),
            };
            if ids.is_empty() && !matches!(rating, FeedbackRating::Missing) {
                return Err("missing ids for useful/useless feedback".to_string());
            }
            let rating = match rating {
                FeedbackRating::Useful => "useful",
                FeedbackRating::Useless => "useless",
                FeedbackRating::Missing => "missing",
            };
            let command = json_string(&args, "command").unwrap_or_else(|| "mcp".to_string());
            let query = json_string(&args, "query").unwrap_or_default();
            let note = json_string(&args, "note").unwrap_or_default();
            let detail = serde_json::to_string(&json!({
                "rating": rating,
                "ids": ids,
                "command": command,
                "query": query,
                "note": note,
            }))
            .map_err(|err| err.to_string())?;
            log_event(&conn, "memory_feedback", None, &detail).map_err(|err| err.to_string())?;
            let report = FeedbackReport {
                ok: true,
                rating: rating.to_string(),
                ids,
                written_event: "memory_feedback".to_string(),
                summary: feedback_summary(&conn, 30).map_err(|err| err.to_string())?,
            };
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_brief" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(budget);
            let scope = mcp_memory_scope(&args);
            let started = Instant::now();
            let report = brief_report(
                &conn,
                &BriefRequest {
                    task: &task,
                    limit,
                    budget,
                    scope: scope.as_deref(),
                    rules: None,
                    provider: DEFAULT_EMBED_PROVIDER,
                    endpoint: DEFAULT_EMBED_ENDPOINT,
                    model: DEFAULT_EMBED_MODEL,
                    json_out: true,
                    audit_read: false,
                },
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_memory_brief_response(&conn, report, &task, max_chars, started)
                .map_err(|err| err.to_string())?
        }
        "memory_impact" => {
            let target =
                json_string(&args, "target").ok_or_else(|| "missing target".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(budget);
            let scope = mcp_memory_scope(&args);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let started = Instant::now();
            let report = impact_report(
                &conn,
                &ImpactRequest {
                    target: &target,
                    limit,
                    budget,
                    scope: scope.as_deref(),
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    json_out: true,
                    audit_read: false,
                },
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_memory_impact_response(&conn, report, &target, max_chars, started)
                .map_err(|err| err.to_string())?
        }
        "memory_drift" => {
            let changed_only = args
                .get("changed_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let root = json_string(&args, "root").unwrap_or_else(|| ".".to_string());
            let report = drift_report(&conn, Path::new(&root), changed_only)
                .map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
            } else {
                compact_mcp_drift_response(&report, max_chars).map_err(|err| err.to_string())?
            }
        }
        "memory_doctrine" => {
            let scope = mcp_memory_scope(&args);
            let query = json_string(&args, "query").unwrap_or_default();
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let report = doctrine_report(&conn, scope.as_deref()).map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
            } else {
                compact_mcp_doctrine_response(&report, &query, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_evidence" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let query = json_string(&args, "query").unwrap_or_default();
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let report = evidence_report(&conn, &id).map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
            } else {
                compact_mcp_evidence_response(&report, &query, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_auto_ingest" => {
            let input =
                json_string(&args, "input").unwrap_or_else(|| ".agent/sessions".to_string());
            let input = mcp_resolve_project_input(&selected_root, Path::new(&input))?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let dry_run = args.get("dry_run").and_then(Value::as_bool).unwrap_or(true);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let report = auto_ingest_sessions(
                &conn,
                &input,
                &scope,
                false,
                DEFAULT_EMBED_ENDPOINT,
                "qwen3:14b",
                dry_run,
            )
            .map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
            } else {
                compact_mcp_auto_ingest_response(&report, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        MCP_MEMORY_GET => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let query = json_string(&args, "query").unwrap_or_default();
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let memory = get_memory_with_links(&conn, &id).map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&memory).map_err(|err| err.to_string())?
            } else {
                compact_mcp_memory_response(&memory, &query, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_review" => {
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut issues = Vec::new();
            issues.extend(review_stale(&conn, 30).map_err(|err| err.to_string())?);
            issues.extend(review_uncertain(&conn).map_err(|err| err.to_string())?);
            issues.extend(review_low_confidence(&conn).map_err(|err| err.to_string())?);
            issues.extend(review_duplicates(&conn).map_err(|err| err.to_string())?);
            if include_body {
                serde_json::to_string_pretty(&issues).map_err(|err| err.to_string())?
            } else {
                compact_mcp_review_response(&issues, limit, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_doctor" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let secrets = scan_secret_findings(&conn).map_err(|err| err.to_string())?;
            let pending = list_inbox(&conn, "pending", usize::MAX)
                .map_err(|err| err.to_string())?
                .len();
            let mut review = Vec::new();
            review.extend(review_stale(&conn, 30).map_err(|err| err.to_string())?);
            review.extend(review_uncertain(&conn).map_err(|err| err.to_string())?);
            review.extend(review_low_confidence(&conn).map_err(|err| err.to_string())?);
            review.extend(review_duplicates(&conn).map_err(|err| err.to_string())?);
            let value = json!({
                "secrets": secrets.len(),
                "pending_inbox": pending,
                "review_issues": review.len(),
                "ok": secrets.is_empty() && pending == 0 && review.is_empty(),
            });
            render_budgeted_json_value(value, max_chars, &[]).map_err(|err| err.to_string())?
        }
        "memory_inbox_list" => {
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let query = json_string(&args, "query").unwrap_or_default();
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let rows = list_inbox(&conn, "pending", limit).map_err(|err| err.to_string())?;
            if include_body {
                serde_json::to_string_pretty(&rows).map_err(|err| err.to_string())?
            } else {
                compact_mcp_inbox_response(&rows, &query, max_chars)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_health_score" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report =
                memory_health_score_report(&conn, &selected_db, &selected_root, since_days)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["recommendations", "memory_qa_recommendations"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_explain_recall" => {
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(8);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = explain_recall_report(&conn, &selected_root, &query, limit)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["items"])
                .map_err(|err| err.to_string())?
        }
        "memory_recall" => {
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(8);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let scope = mcp_memory_scope(&args);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let as_of = json_string(&args, "as_of");
            let changed_since = json_string(&args, "changed_since");
            let report = recall_report(
                &conn,
                &RecallRequest {
                    query: &query,
                    max_chars,
                    limit,
                    scope: scope.as_deref(),
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    recent: args.get("recent").and_then(Value::as_bool).unwrap_or(false),
                    as_of: as_of.as_deref(),
                    as_of_days_ago: json_i64(&args, "as_of_days_ago"),
                    changed_since: changed_since.as_deref(),
                    changed_since_days: json_i64(&args, "changed_since_days"),
                    json_out: true,
                },
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["items"])
                .map_err(|err| err.to_string())?
        }
        "memory_upload" => {
            let input = json_string(&args, "input").ok_or_else(|| "missing input".to_string())?;
            let input = mcp_resolve_project_input(&selected_root, Path::new(&input))?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = memory_upload_report(&conn, &selected_root, &input, &scope, apply)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["candidates", "quality_checks"])
                .map_err(|err| err.to_string())?
        }
        "memory_rag_ingest" => {
            let input = json_string(&args, "input").ok_or_else(|| "missing input".to_string())?;
            let input = mcp_resolve_project_input(&selected_root, Path::new(&input))?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let embed = args.get("embed").and_then(Value::as_bool).unwrap_or(false);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = crate::app::rag_ingest::rag_ingest_report(
                &conn,
                crate::app::rag_ingest::RagIngestRequest {
                    root: &selected_root,
                    input: &input,
                    scope: &scope,
                    apply,
                    embed,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    chunk_chars: json_usize(&args, "chunk_chars").unwrap_or(900),
                    overlap_chars: json_usize(&args, "overlap_chars").unwrap_or(140),
                    max_file_bytes: json_usize(&args, "max_file_bytes").unwrap_or(200_000),
                    max_files: json_usize(&args, "max_files").unwrap_or(128),
                    json: true,
                },
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["sources", "skipped", "actions"])
                .map_err(|err| err.to_string())?
        }
        "memory_rag_sources" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let report = crate::app::rag_ingest::rag_sources_report(
                &conn,
                &selected_root,
                &provider,
                &endpoint,
                &model,
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["sources", "issues", "recommendations"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_rag_eval" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let scope = json_string(&args, "scope");
            let report = rag_eval_report_with_baseline(
                &conn,
                scope.as_deref(),
                json_usize(&args, "limit").unwrap_or(8),
                json_usize(&args, "budget").unwrap_or(3_000),
                &provider,
                &endpoint,
                &model,
                Some(&selected_root),
                args.get("write_baseline")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &[
                    "cases",
                    "recommendations",
                    "baseline",
                    "eval_matrix",
                    "retrieval_tuning",
                ],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_graph_rag_eval" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let provider = json_string(&args, "provider")
                .unwrap_or_else(|| DEFAULT_EMBED_PROVIDER.to_string());
            let endpoint = json_string(&args, "endpoint")
                .unwrap_or_else(|| DEFAULT_EMBED_ENDPOINT.to_string());
            let model =
                json_string(&args, "model").unwrap_or_else(|| DEFAULT_EMBED_MODEL.to_string());
            let scope = json_string(&args, "scope");
            let gen_config = crate::runtime_config::GenerationConfig {
                provider: "mock".to_string(),
                endpoint: "local".to_string(),
                model: "extractive-fallback".to_string(),
            };
            let report = graph_rag_eval_report(
                &conn,
                scope.as_deref(),
                json_usize(&args, "limit").unwrap_or(8),
                json_usize(&args, "budget").unwrap_or(3_000),
                &gen_config,
                &provider,
                &endpoint,
                &model,
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["cases", "recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_advanced_eval" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2_400);
            let report = advanced_eval_report(&conn).map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["capabilities", "candidate_ids", "recommendations"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_auto_ranking_tune" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1800);
            let report = auto_ranking_tune_report(&conn, &selected_root, since_days, apply)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["signals", "apply_plan", "reasons", "ranking"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_memanto_gap" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = memanto_gap_report(&conn).map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["capabilities"])
                .map_err(|err| err.to_string())?
        }
        "memory_timeline" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report =
                memory_timeline_report(&conn, &id, limit).map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["recent_events", "recent_reads"])
                .map_err(|err| err.to_string())?
        }
        "memory_observe" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let kind = json_string(&args, "kind").ok_or_else(|| "missing kind".to_string())?;
            let statement =
                json_string(&args, "statement").ok_or_else(|| "missing statement".to_string())?;
            let evidence_kind = json_string(&args, "evidence_kind")
                .ok_or_else(|| "missing evidence_kind".to_string())?;
            let evidence_ref = json_string(&args, "evidence_ref")
                .ok_or_else(|| "missing evidence_ref".to_string())?;
            let target_memory_id = json_string(&args, "target_memory_id");
            let observation = record_memory_observation(
                &conn,
                &selected_root,
                &MemoryObservationRequest {
                    memory_id: &id,
                    target_memory_id: target_memory_id.as_deref(),
                    kind: &kind,
                    statement: &statement,
                    evidence_kind: &evidence_kind,
                    evidence_ref: &evidence_ref,
                    confidence: args
                        .get("confidence")
                        .and_then(Value::as_f64)
                        .unwrap_or(1.0),
                    valid_from: json_i64(&args, "valid_from"),
                    valid_to: json_i64(&args, "valid_to"),
                },
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&observation).map_err(|err| err.to_string())?
        }
        "memory_observations" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2_000);
            let observations = list_memory_observations(
                &conn,
                &id,
                json_i64(&args, "valid_at"),
                json_i64(&args, "known_at"),
                json_usize(&args, "limit").unwrap_or(100),
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&observations, max_chars, &[])
                .map_err(|err| err.to_string())?
        }
        "memory_temporal_graph" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(4_000);
            let report = temporal_memory_graph_report(
                &conn,
                json_i64(&args, "valid_at"),
                json_i64(&args, "known_at"),
                json_usize(&args, "limit").unwrap_or(500),
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["edges", "nodes"])
                .map_err(|err| err.to_string())?
        }
        "memory_conflict_review" => {
            let stale_days = json_i64(&args, "stale_days").unwrap_or(30);
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = memory_conflict_review_report(&conn, stale_days, limit)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["groups"])
                .map_err(|err| err.to_string())?
        }
        "memory_effectiveness_v2" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1600);
            let report = memory_effectiveness_v2_report(&conn, &selected_root, since_days)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["base", "top_useful_cards", "weak_reads", "recommendations"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_recall_baselines" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1600);
            let report =
                recall_benchmark_baselines_report(&conn, &selected_root, since_days, apply)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["benchmark", "recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_conflict_apply" => {
            let stale_days = json_i64(&args, "stale_days").unwrap_or(30);
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1600);
            let report = memory_conflict_apply_report(&conn, stale_days, limit, apply)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &[
                    "review",
                    "safe_actions",
                    "applied_actions",
                    "recommendations",
                ],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_mcp_surface_v3" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = mcp_tool_surface_v3_report();
            budgeted_mcp_json_response(&report, max_chars, &["expected_tools", "exposed_tools"])
                .map_err(|err| err.to_string())?
        }
        "memory_mcp_discipline_v3" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1600);
            let report =
                mcp_discipline_v3_report(&conn, &selected_db, &selected_root, since_days, apply)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["discipline_v2", "surface", "recommendations"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_fleet_quality" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let report =
                fleet_quality_report(&selected_db, since_days).map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["projects", "recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_release_gate_v3" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let rag_profile =
                ReleaseRagProfile::parse(args.get("rag_profile").and_then(Value::as_str))
                    .map_err(|err| err.to_string())?;
            let strict = args.get("strict").and_then(Value::as_bool).unwrap_or(false);
            let run = args.get("run").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let report = release_gate_v3_report_with_profile(
                &conn,
                &selected_db,
                &selected_root,
                since_days,
                strict,
                run,
                rag_profile,
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &[
                    "release_gate_v2",
                    "effectiveness_v2",
                    "baselines",
                    "conflict_apply",
                    "fleet_quality",
                    "recommendations",
                ],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_control_center_v2" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1800);
            let report =
                memory_control_center_v2_report(&conn, &selected_db, &selected_root, since_days)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &[
                    "recommendations",
                    "recall_probes",
                    "explain_recall",
                    "audit_v2",
                    "health",
                ],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_release_gate_v2" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let strict = args.get("strict").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1800);
            let report = release_gate_v2_report(
                &conn,
                &selected_db,
                &selected_root,
                since_days,
                strict,
                false,
            )
            .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["checks", "recommendations", "control_center"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_quality_ci" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let minimal = args.get("minimal").and_then(Value::as_bool).unwrap_or(true);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1600);
            let report =
                memory_quality_ci_report(&conn, &selected_db, &selected_root, since_days, minimal)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["failed_checks", "recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_fleet_dashboard_v2" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let report = fleet_dashboard_v2_report(&selected_db, since_days)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["projects", "attention"])
                .map_err(|err| err.to_string())?
        }
        "memory_governance_policy" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report = memory_governance_policy_report(&selected_root, false)
                .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_status" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1400);
            let report = control_snapshot_report(&conn, &selected_db, &selected_root, since_days)
                .map_err(|err| err.to_string())?;
            compact_mcp_status_response(&report, max_chars).map_err(|err| err.to_string())?
        }
        "memory_should_write" => {
            let text = json_string(&args, "text").ok_or_else(|| "missing text".to_string())?;
            let memory_type =
                json_string(&args, "memory_type").unwrap_or_else(|| "task_state".to_string());
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1000);
            let durable_type = matches!(
                memory_type.as_str(),
                "decision"
                    | "constraint"
                    | "user_preference"
                    | "command"
                    | "known_issue"
                    | "task_state"
                    | "design_note"
            );
            let should_write = durable_type && text.split_whitespace().count() >= 4;
            let value = json!({
                "version": 1,
                "should_write": should_write,
                "memory_type": memory_type,
                "reason": if should_write {
                    "text looks durable enough for a compact memory card"
                } else {
                    "skip transient, too-short, or unsupported memory content"
                },
                "recommended_command": if should_write {
                    "memory_add or dukememory add"
                } else {
                    "no durable write"
                },
            });
            render_budgeted_json_value(value, max_chars, &[]).map_err(|err| err.to_string())?
        }
        "memory_after_task" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1400);
            let diff = memory_diff_review_report(&conn, &selected_root, false)
                .map_err(|err| err.to_string())?;
            let inbox =
                inbox_ai_reviewer_report(&conn, 20, false).map_err(|err| err.to_string())?;
            let qa = memory_qa_report(&conn, &selected_root, since_days)
                .map_err(|err| err.to_string())?;
            let value = json!({
                "version": 1,
                "status": if diff.write_ready.is_empty() && inbox.approve_ready == 0 {
                    "ready"
                } else {
                    "attention"
                },
                "diff_write_ready": diff.write_ready.len(),
                "inbox_approve_ready": inbox.approve_ready,
                "inbox_merge_ready": inbox.merge_ready,
                "qa_score": qa.score,
                "recommendations": [
                    "save a compact durable card only for reusable outcomes",
                    "run memory_diff_review or inbox_ai_reviewer before broad writes",
                    "run embed-index once after important memory writes"
                ]
            });
            render_budgeted_json_value(value, max_chars, &["recommendations"])
                .map_err(|err| err.to_string())?
        }
        "memory_project_health" => {
            let since_days = json_usize(&args, "since_days").unwrap_or(7) as i64;
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1400);
            let health =
                memory_health_score_report(&conn, &selected_db, &selected_root, since_days)
                    .map_err(|err| err.to_string())?;
            let role = project_role_profile_report(&selected_root, None, false)
                .map_err(|err| err.to_string())?;
            let value = json!({
                "version": 1,
                "health": health,
                "role_profile": role,
            });
            render_budgeted_json_value(
                value,
                max_chars,
                &["components", "recommendations", "reasons"],
            )
            .map_err(|err| err.to_string())?
        }
        other => return Err(format!("unsupported tool: {other}")),
    };
    let structured = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(object)) => Value::Object(object),
        Ok(value) => json!({"value": value}),
        Err(_) => json!({"text": text.clone()}),
    };
    Ok(json!({
        "content":[{"type":"text","text":text}],
        "structuredContent": structured,
        "isError": false
    }))
}

fn log_mcp_context_read(
    conn: &Connection,
    command: &str,
    query: &str,
    ids: &[String],
    semantic_used: bool,
    budget: usize,
    started: Instant,
) -> Result<()> {
    log_read_event(
        conn,
        ReadEventInput {
            command,
            query,
            ids,
            semantic_used,
            result_count: ids.len(),
            budget,
            elapsed_ms: started.elapsed().as_millis(),
        },
    )
}

fn memory_row_ids(rows: &[Memory]) -> Vec<String> {
    rows.iter().map(|memory| memory.id.clone()).collect()
}

fn compact_mcp_search_response(
    rows: &[Memory],
    query: &str,
    max_chars: usize,
) -> Result<(String, Vec<String>)> {
    let mut items = Vec::new();
    let mut used_ids = Vec::new();
    for row in rows {
        items.push(compact_mcp_memory_value(row, &[], query));
        let rendered = serde_json::to_string_pretty(&items)?;
        if rendered.len() > max_chars {
            items.pop();
            break;
        }
        used_ids.push(row.id.clone());
    }
    Ok((
        render_budgeted_json_value(Value::Array(items), max_chars, &[])?,
        used_ids,
    ))
}

fn compact_mcp_memory_response(
    memory: &MemoryWithLinks,
    query: &str,
    max_chars: usize,
) -> Result<String> {
    render_budgeted_json_value(
        compact_mcp_memory_value(&memory.memory, &memory.links, query),
        max_chars,
        &["links"],
    )
}

fn compact_mcp_memory_value(memory: &Memory, links: &[MemoryLink], query: &str) -> Value {
    let query_terms = relevance_terms(query);
    json!({
        "id": memory.id,
        "type": memory.memory_type,
        "scope": memory.scope,
        "status": memory.status,
        "title": memory.title,
        "summary": query_focused_summary(&memory.body, &query_terms, 160),
        "confidence": memory.confidence,
        "updated_at": memory.updated_at,
        "links": links,
    })
}

fn budgeted_mcp_json_response<T: Serialize>(
    report: &T,
    max_chars: usize,
    sections: &[&str],
) -> Result<String> {
    render_budgeted_json_value(serde_json::to_value(report)?, max_chars, sections)
}

fn compact_mcp_status_response(report: &ControlSnapshot, max_chars: usize) -> Result<String> {
    let value = json!({
        "version": report.version,
        "ok": report.ok,
        "status": report.status,
        "revision": report.revision,
        "current_version": report.current_version,
        "cache": report.cache,
        "panels": report.panels,
        "summary": report.summary,
        "request_budget": report.request_budget,
        "details_endpoint": report.compatibility.details_endpoint,
    });
    render_budgeted_json_value(value, max_chars, &["panels"])
}

fn budgeted_mcp_memory_brief_response(
    conn: &Connection,
    report: BriefReport,
    query: &str,
    max_chars: usize,
    started: Instant,
) -> Result<String> {
    let semantic_status = if report.semantic_skipped {
        MemorySemanticStatus::Skipped
    } else if report.semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    budgeted_mcp_memory_report_response(McpMemoryReportRenderInput {
        conn,
        command: "brief",
        query,
        semantic_used: report.semantic_used,
        semantic_status,
        value: serde_json::to_value(report)?,
        max_chars,
        sections: &["checks", "files", "risks", "relevant", "must_follow"],
        started,
    })
}

fn budgeted_mcp_memory_impact_response(
    conn: &Connection,
    report: ImpactReport,
    query: &str,
    max_chars: usize,
    started: Instant,
) -> Result<String> {
    let semantic_status = if report.semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    budgeted_mcp_memory_report_response(McpMemoryReportRenderInput {
        conn,
        command: "impact",
        query,
        semantic_used: report.semantic_used,
        semantic_status,
        value: serde_json::to_value(report)?,
        max_chars,
        sections: &[
            "links",
            "checks",
            "related",
            "risks",
            "constraints",
            "decisions",
        ],
        started,
    })
}

struct McpMemoryReportRenderInput<'a> {
    conn: &'a Connection,
    command: &'a str,
    query: &'a str,
    semantic_used: bool,
    semantic_status: MemorySemanticStatus,
    value: Value,
    max_chars: usize,
    sections: &'a [&'a str],
    started: Instant,
}

fn budgeted_mcp_memory_report_response(input: McpMemoryReportRenderInput<'_>) -> Result<String> {
    let mut value = input.value;
    let mut last_ids: Option<Vec<String>> = None;
    for _ in 0..8 {
        let rendered = render_budgeted_json_value(value.clone(), input.max_chars, input.sections)?;
        let rendered_value: Value = serde_json::from_str(&rendered)?;
        let ids = memory_ids_in_json_sections(&rendered_value, input.sections);
        if last_ids.as_ref() == Some(&ids) {
            log_mcp_context_read(
                input.conn,
                input.command,
                input.query,
                &ids,
                input.semantic_used,
                input.max_chars,
                input.started,
            )?;
            return Ok(rendered);
        }
        set_json_receipt(
            &mut value,
            &memory_receipt_with_semantic(input.command, input.semantic_status, &ids, "none"),
        );
        last_ids = Some(ids);
    }

    let rendered = render_budgeted_json_value(value, input.max_chars, input.sections)?;
    let rendered_value: Value = serde_json::from_str(&rendered)?;
    let ids = memory_ids_in_json_sections(&rendered_value, input.sections);
    log_mcp_context_read(
        input.conn,
        input.command,
        input.query,
        &ids,
        input.semantic_used,
        input.max_chars,
        input.started,
    )?;
    Ok(rendered)
}

fn set_json_receipt(value: &mut Value, receipt: &str) {
    if let Some(object) = value.as_object_mut() {
        object.insert("receipt".to_string(), json!(receipt));
    }
}

fn memory_ids_in_json_sections(value: &Value, sections: &[&str]) -> Vec<String> {
    let mut ids = Vec::new();
    for section in sections {
        let Some(items) = value.get(*section).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            if let Some(id) = item.get("id").and_then(Value::as_str)
                && is_compact_memory_id(id)
            {
                ids.push(id.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn is_compact_memory_id(value: &str) -> bool {
    value.len() == 12 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn mcp_effective_limit(limit: usize, max_chars: usize) -> usize {
    context_effective_limit(limit, max_chars)
}

fn mcp_snapshot_query_candidate_limit(effective_limit: usize, max_chars: usize) -> usize {
    let effective_limit = effective_limit.max(1);
    let scan = if max_chars <= 1_200 {
        effective_limit.saturating_mul(3).min(24)
    } else if max_chars <= 3_000 {
        effective_limit.saturating_mul(3).min(48)
    } else {
        effective_limit.saturating_mul(2).min(64)
    };
    scan.max(effective_limit)
}

fn compact_mcp_drift_response(report: &DriftReport, max_chars: usize) -> Result<String> {
    let value = json!({
        "version": report.version,
        "ok": report.ok,
        "changed_only": report.changed_only,
        "root": report.root,
        "counts": {
            "changed_files": report.changed_files.len(),
            "missing_links": report.missing_links.len(),
            "conflicts": report.conflicts.len(),
            "stale_active": report.stale_active.len(),
            "warnings": report.warnings.len(),
        },
        "changed_files": report.changed_files.iter().take(8).collect::<Vec<_>>(),
        "missing_links": report.missing_links.iter().take(8).collect::<Vec<_>>(),
        "conflicts": report.conflicts.iter().take(8).collect::<Vec<_>>(),
        "stale_active": report.stale_active.iter().take(8).collect::<Vec<_>>(),
        "warnings": report.warnings.iter().take(8).collect::<Vec<_>>(),
    });
    render_budgeted_json_value(
        value,
        max_chars,
        &[
            "warnings",
            "stale_active",
            "changed_files",
            "missing_links",
            "conflicts",
        ],
    )
}

fn compact_mcp_auto_ingest_response(report: &AutoIngestReport, max_chars: usize) -> Result<String> {
    let files = report
        .files
        .iter()
        .map(|file| {
            json!({
                "path": truncate_chars(&file.path, 180),
                "status": file.status,
                "suggestions": file.suggestions,
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "scanned": report.scanned,
        "ingested": report.ingested,
        "skipped": report.skipped,
        "inbox_added": report.inbox_added,
        "returned_files": files.len(),
        "truncated": false,
        "files": files,
    });
    let rendered = render_budgeted_json_value(value, max_chars, &["files"])?;
    let mut value: Value = serde_json::from_str(&rendered)?;
    update_returned_count(&mut value, "files", "returned_files");
    Ok(serde_json::to_string_pretty(&value)?)
}

fn compact_mcp_evidence_response(
    report: &EvidenceReport,
    query: &str,
    max_chars: usize,
) -> Result<String> {
    let memory = &report.memory.memory;
    let query_terms = relevance_terms(query);
    let audit_events = report
        .audit_events
        .iter()
        .take(5)
        .map(|event| {
            json!({
                "id": event.id,
                "event_type": event.event_type,
                "detail": truncate_chars(&event.detail, 160),
                "created_at": event.created_at,
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "memory": {
            "id": memory.id,
            "type": memory.memory_type,
            "scope": memory.scope,
            "status": memory.status,
            "title": memory.title,
            "summary": query_focused_summary(&memory.body, &query_terms, 180),
            "confidence": memory.confidence,
            "updated_at": memory.updated_at,
            "links": report.memory.links,
        },
        "source": report.source,
        "supersedes_chain": report.supersedes_chain,
        "superseded_by": report.superseded_by,
        "audit_event_count": report.audit_events.len(),
        "audit_events": audit_events,
        "receipt": report.receipt,
    });
    render_budgeted_json_value(value, max_chars, &["audit_events", "supersedes_chain"])
}

fn compact_mcp_doctrine_response(
    report: &DoctrineReport,
    query: &str,
    max_chars: usize,
) -> Result<String> {
    let mut value = serde_json::to_value(report)?;
    let query_terms = relevance_terms(query);
    compact_doctrine_section(&mut value, "active", &query_terms);
    compact_doctrine_section(&mut value, "superseded", &query_terms);
    render_budgeted_json_value(value, max_chars, &["superseded", "conflicts", "active"])
}

fn compact_mcp_review_response(
    issues: &[ReviewIssue],
    limit: usize,
    max_chars: usize,
) -> Result<String> {
    let items = issues
        .iter()
        .take(limit)
        .map(|issue| {
            json!({
                "kind": issue.kind,
                "id": issue.id,
                "title": issue.title,
                "detail": truncate_chars(&issue.detail, 160),
            })
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "total": issues.len(),
        "returned": items.len(),
        "truncated": issues.len() > items.len(),
        "issues": items,
    });
    value = serde_json::from_str(&render_budgeted_json_value(value, max_chars, &["issues"])?)?;
    update_returned_count(&mut value, "issues", "returned");
    Ok(serde_json::to_string_pretty(&value)?)
}

fn compact_mcp_inbox_response(rows: &[InboxItem], query: &str, max_chars: usize) -> Result<String> {
    let query_terms = relevance_terms(query);
    let items = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "type": row.memory_type,
                "scope": row.scope,
                "status": row.status,
                "title": row.title,
                "summary": query_focused_summary(&row.body, &query_terms, 180),
                "source": row.source,
                "confidence": row.confidence,
                "updated_at": row.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "total": rows.len(),
        "returned": items.len(),
        "truncated": false,
        "items": items,
    });
    value = serde_json::from_str(&render_budgeted_json_value(value, max_chars, &["items"])?)?;
    update_returned_count(&mut value, "items", "returned");
    Ok(serde_json::to_string_pretty(&value)?)
}

fn compact_doctrine_section(value: &mut Value, key: &str, query_terms: &HashSet<String>) {
    let Some(items) = value.get_mut(key).and_then(Value::as_array_mut) else {
        return;
    };
    for item in items {
        if let Some(body) = item.get("body").and_then(Value::as_str).map(str::to_string)
            && let Some(object) = item.as_object_mut()
        {
            object.remove("body");
            object.insert(
                "summary".to_string(),
                Value::String(query_focused_summary(&body, query_terms, 180)),
            );
        }
    }
}

fn fit_json_array_sections(value: &mut Value, max_chars: usize, sections: &[&str]) -> Result<()> {
    let mut truncated = false;
    while serde_json::to_string_pretty(value)?.len() > max_chars {
        let mut removed = false;
        for section in sections {
            if let Some(items) = value.get_mut(*section).and_then(Value::as_array_mut)
                && items.pop().is_some()
            {
                truncated = true;
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }
    if truncated && let Some(object) = value.as_object_mut() {
        object.insert("truncated".to_string(), Value::Bool(true));
    }
    Ok(())
}

fn render_budgeted_json_value(
    mut value: Value,
    max_chars: usize,
    sections: &[&str],
) -> Result<String> {
    fit_json_array_sections(&mut value, max_chars, sections)?;
    if serde_json::to_string_pretty(&value)?.len() > max_chars {
        truncate_json_strings(&mut value, 180);
        fit_json_array_sections(&mut value, max_chars, sections)?;
    }
    let rendered = serde_json::to_string_pretty(&value)?;
    if rendered.len() <= max_chars {
        return Ok(rendered);
    }
    Ok(serde_json::to_string_pretty(&json!({
        "truncated": true,
        "max_chars": max_chars,
        "summary": "MCP response exceeded budget after compaction"
    }))?)
}

fn truncate_json_strings(value: &mut Value, max_chars: usize) {
    match value {
        Value::String(text) => {
            *text = truncate_chars(text, max_chars);
        }
        Value::Array(items) => {
            for item in items {
                truncate_json_strings(item, max_chars);
            }
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                truncate_json_strings(item, max_chars);
            }
        }
        _ => {}
    }
}

fn update_returned_count(value: &mut Value, array_key: &str, count_key: &str) {
    let count = value
        .get(array_key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if let Some(object) = value.as_object_mut() {
        object.insert(count_key.to_string(), json!(count));
    }
}

fn mcp_selected_db(default_db: &Path, args: &Value) -> std::result::Result<PathBuf, String> {
    let requested =
        if let Some(db) = json_string(args, "db").filter(|value| !value.trim().is_empty()) {
            Some(expand_mcp_path(&db))
        } else {
            ["root", "project_root", "project"]
                .into_iter()
                .find_map(|key| {
                    json_string(args, key)
                        .filter(|value| !value.trim().is_empty())
                        .map(|root| project_memory_db(&root))
                })
                .or_else(|| {
                    json_string(args, "scope")
                        .filter(|scope| mcp_scope_looks_like_project_root(scope))
                        .map(|scope| project_memory_db(&scope))
                })
        };
    let Some(requested) = requested else {
        return Ok(default_db.to_path_buf());
    };
    let requested_key = app_canonical_or_absolute(&requested);
    let allowed = mcp_allowed_project_dbs(default_db)?;
    allowed
        .into_iter()
        .find(|candidate| app_canonical_or_absolute(candidate) == requested_key)
        .ok_or_else(|| {
            format!(
                "MCP project is outside allowed roots: {}; use a discovered sibling project or set DUKEMEMORY_MCP_ALLOWED_ROOTS explicitly",
                requested.display()
            )
        })
}

fn mcp_allowed_project_dbs(default_db: &Path) -> std::result::Result<Vec<PathBuf>, String> {
    let mut allowed = discover_project_dbs(default_db).map_err(|err| err.to_string())?;
    if let Some(value) = std::env::var_os("DUKEMEMORY_MCP_ALLOWED_ROOTS") {
        for root in std::env::split_paths(&value) {
            let db = if root.file_name().is_some_and(|name| name == "memory.db") {
                root
            } else {
                root.join(DEFAULT_DB)
            };
            app_push_unique_db(&mut allowed, &db);
        }
    }
    Ok(allowed)
}

fn mcp_selected_root(selected_db: &Path) -> PathBuf {
    app_project_root_for_db(selected_db).unwrap_or_else(|| {
        selected_db
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

fn mcp_resolve_project_input(root: &Path, input: &Path) -> std::result::Result<PathBuf, String> {
    let root = app_canonical_or_absolute(root);
    let candidate = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let candidate = app_canonical_or_absolute(&candidate);
    if !candidate.starts_with(&root) {
        return Err(format!(
            "MCP file input is outside selected project root: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn mcp_memory_scope(args: &Value) -> Option<String> {
    json_string(args, "scope").filter(|scope| scope.parse::<MemoryScope>().is_ok())
}

fn project_memory_db(root: &str) -> PathBuf {
    let path = expand_mcp_path(root);
    if path.file_name().is_some_and(|name| name == "memory.db") {
        path
    } else {
        path.join(DEFAULT_DB)
    }
}

fn mcp_scope_looks_like_project_root(value: &str) -> bool {
    if value.parse::<MemoryScope>().is_ok() {
        return false;
    }
    value.starts_with('/')
        || value.starts_with("~/")
        || value == "."
        || value.starts_with("./")
        || value.starts_with("../")
        || value.contains("/.agent/")
}

fn expand_mcp_path(value: &str) -> PathBuf {
    if value == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value))
    } else if let Some(rest) = value.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or_else(|| PathBuf::from(value))
    } else {
        PathBuf::from(value)
    }
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn json_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn json_usize(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(profile: McpProfile, page_size: usize) -> McpSessionState {
        McpSessionState {
            protocol_version: None,
            initialized: false,
            profile,
            page_size,
            client_key: "stdio:test-client".to_string(),
            tasks: std::sync::Arc::new(McpTaskStore::default()),
        }
    }

    #[test]
    fn mcp_effective_limit_tracks_response_budget() {
        assert_eq!(mcp_effective_limit(20, 900), 4);
        assert_eq!(mcp_effective_limit(20, 3_000), 8);
        assert_eq!(mcp_effective_limit(20, 5_000), 20);
        assert_eq!(mcp_effective_limit(3, 900), 3);
        assert_eq!(mcp_effective_limit(0, 900), 1);
        assert_eq!(mcp_snapshot_query_candidate_limit(4, 900), 12);
        assert_eq!(mcp_snapshot_query_candidate_limit(8, 3_000), 24);
        assert_eq!(mcp_snapshot_query_candidate_limit(100, 3_000), 100);
        assert_eq!(mcp_snapshot_query_candidate_limit(20, 5_000), 40);
    }

    #[test]
    fn mcp_profiles_and_tool_pagination_bound_discovery() {
        let core = test_state(McpProfile::Core, 5);
        for tool in mcp_tools().as_array().unwrap().iter().filter(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| mcp_profile_includes(McpProfile::Core, name))
        }) {
            let name = tool["name"].as_str().unwrap();
            assert!(
                operation_for_mcp(name).is_some(),
                "core tool {name} is uncataloged"
            );
        }
        let first = mcp_list_tools(None, &core).unwrap();
        assert_eq!(first["tools"].as_array().unwrap().len(), 5);
        let cursor = first["nextCursor"].as_str().unwrap();
        let second = mcp_list_tools(Some(&json!({"cursor": cursor})), &core).unwrap();
        assert_eq!(second["tools"].as_array().unwrap().len(), 5);

        let full = test_state(McpProfile::Full, 0);
        let full = mcp_list_tools(None, &full).unwrap();
        assert!(full["tools"].as_array().unwrap().len() > 50);
        assert!(full.get("nextCursor").is_none());
    }

    #[test]
    fn mcp_input_schemas_are_closed_and_integer_bounded() {
        for tool in mcp_tools().as_array().unwrap() {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "tool={}", tool["name"]);
            assert_eq!(
                schema["additionalProperties"], false,
                "tool={}",
                tool["name"]
            );
            for (name, property) in schema["properties"].as_object().unwrap() {
                if property["type"] == "number" {
                    assert_eq!(name, "confidence");
                }
                if property["type"] == "integer" {
                    assert!(property.get("minimum").is_some(), "field={name}");
                    assert!(property.get("maximum").is_some(), "field={name}");
                }
            }
        }
    }

    #[test]
    fn mcp_argument_validation_rejects_unknown_and_malformed_fields() {
        assert!(validate_mcp_tool_arguments("memory_brief", &json!({"task":"review"})).is_ok());
        assert!(validate_mcp_tool_arguments("memory_brief", &json!({})).is_err());
        assert!(
            validate_mcp_tool_arguments("memory_brief", &json!({"task":"review", "limit":"ten"}))
                .is_err()
        );
        assert!(
            validate_mcp_tool_arguments(
                "memory_brief",
                &json!({"task":"review", "unexpected":true})
            )
            .is_err()
        );
        assert!(
            validate_mcp_tool_arguments("memory_feedback", &json!({"rating":"maybe"})).is_err()
        );
        let observation = json!({
            "id":"abc123",
            "kind":"verified",
            "statement":"verified by test",
            "evidence_kind":"test",
            "evidence_ref":"cargo test",
            "confidence":0.95
        });
        assert!(validate_mcp_tool_arguments("memory_observe", &observation).is_ok());
        let mut invalid_observation = observation;
        invalid_observation["confidence"] = json!(1.1);
        assert!(validate_mcp_tool_arguments("memory_observe", &invalid_observation).is_err());
    }

    #[test]
    fn mcp_resources_and_tasks_follow_latest_protocol_contract() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(".agent/memory.db");
        let mut state = test_state(McpProfile::Core, 0);
        let initialized = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"initialize",
                "params":{"protocolVersion": MCP_LATEST_PROTOCOL_VERSION}
            }),
            &mut state,
        )
        .unwrap();
        assert!(initialized["result"]["capabilities"]["resources"].is_object());
        assert!(initialized["result"]["capabilities"]["tasks"].is_object());
        assert!(
            handle_mcp_request(
                &db,
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                &mut state,
            )
            .is_none()
        );

        let resource = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"resources/read",
                "params":{"uri":"dukememory://project/status"}
            }),
            &mut state,
        )
        .unwrap();
        assert_eq!(
            resource["result"]["contents"][0]["mimeType"],
            "application/json"
        );

        let created = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":3,
                "method":"tools/call",
                "params":{
                    "name":"memory_context_pack",
                    "arguments":{
                        "task":"task protocol smoke test",
                        "provider":"mock",
                        "endpoint":"mock",
                        "model":"mock-small"
                    },
                    "task":{"ttl":60_000}
                }
            }),
            &mut state,
        )
        .unwrap();
        let task_id = created["result"]["task"]["taskId"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(created["result"]["task"]["status"], "working");

        let result = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":4,
                "method":"tasks/result",
                "params":{"taskId":task_id}
            }),
            &mut state,
        )
        .unwrap();
        assert_eq!(
            result["result"]["_meta"]["io.modelcontextprotocol/related-task"]["taskId"],
            task_id
        );
    }

    fn modern_meta(tasks: bool) -> Value {
        let extensions = if tasks {
            json!({MCP_TASKS_EXTENSION: {}})
        } else {
            json!({})
        };
        json!({
            "io.modelcontextprotocol/protocolVersion": MCP_MODERN_PROTOCOL_VERSION,
            "io.modelcontextprotocol/clientInfo": {"name":"dukememory-test","version":"1.0"},
            "io.modelcontextprotocol/clientCapabilities": {"extensions": extensions}
        })
    }

    #[test]
    fn mcp_modern_discovery_and_tasks_are_stateless_and_durable() {
        let directory = tempfile::tempdir().unwrap();
        let db = directory.path().join(".agent/memory.db");
        let mut state = test_state(McpProfile::Core, 0);
        let discovered = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"server/discover",
                "params":{"_meta":modern_meta(true)}
            }),
            &mut state,
        )
        .unwrap();
        assert_eq!(
            discovered["result"]["supportedVersions"][0],
            MCP_MODERN_PROTOCOL_VERSION
        );
        assert!(
            discovered["result"]["capabilities"]["extensions"][MCP_TASKS_EXTENSION].is_object()
        );

        let created = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"memory_context_pack",
                    "arguments":{
                        "task":"modern durable task smoke test",
                        "provider":"mock",
                        "endpoint":"mock",
                        "model":"mock-small"
                    },
                    "_meta":modern_meta(true)
                }
            }),
            &mut state,
        )
        .unwrap();
        assert_eq!(created["result"]["resultType"], "task");
        assert!(created["result"].get("task").is_none());
        let task_id = created["result"]["taskId"].as_str().unwrap().to_string();

        let mut completed = None;
        for id in 3..103 {
            let response = handle_mcp_request(
                &db,
                json!({
                    "jsonrpc":"2.0",
                    "id":id,
                    "method":"tasks/get",
                    "params":{"taskId":task_id,"_meta":modern_meta(true)}
                }),
                &mut state,
            )
            .unwrap();
            if response["result"]["status"] == "completed" {
                completed = Some(response);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let completed = completed.expect("modern task should complete");
        assert_eq!(completed["result"]["resultType"], "complete");
        assert!(completed["result"]["result"].is_object());

        let mut restarted_state = test_state(McpProfile::Core, 0);
        let after_restart = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":104,
                "method":"tasks/get",
                "params":{"taskId":task_id,"_meta":modern_meta(true)}
            }),
            &mut restarted_state,
        )
        .unwrap();
        assert_eq!(after_restart["result"]["status"], "completed");

        let missing_capability = handle_mcp_request(
            &db,
            json!({
                "jsonrpc":"2.0",
                "id":105,
                "method":"tasks/get",
                "params":{"taskId":task_id,"_meta":modern_meta(false)}
            }),
            &mut restarted_state,
        )
        .unwrap();
        assert_eq!(missing_capability["error"]["code"], -32003);
    }

    #[test]
    fn mcp_extension_cancellation_is_acknowledged_then_observed() {
        let directory = tempfile::tempdir().unwrap();
        let db = directory.path().join(".agent/memory.db");
        let conn = open_db(&db).unwrap();
        let now = now_ms();
        let task = McpTaskRecord {
            task_id: "cancel-me".to_string(),
            owner_key: "stdio:test-owner".to_string(),
            protocol_version: MCP_MODERN_PROTOCOL_VERSION.to_string(),
            lifecycle: "extension".to_string(),
            operation_name: "memory_context_pack".to_string(),
            status: "working".to_string(),
            status_message: "Working.".to_string(),
            created_at: mcp_task_timestamp(),
            last_updated_at: mcp_task_timestamp(),
            created_at_ms: now,
            last_updated_at_ms: now,
            ttl: 60_000,
            poll_interval: 250,
            expires_at_ms: now + 60_000,
            result: None,
            error: None,
            cancellation_requested: false,
        };
        persist_mcp_task(&conn, &task).unwrap();
        let state = test_state(McpProfile::Core, 0);
        let acknowledged = mcp_task_cancel(
            &db,
            Some(&json!({"taskId":"cancel-me"})),
            &state,
            "stdio:test-owner",
            "extension",
            true,
        )
        .unwrap();
        assert_eq!(acknowledged["resultType"], "complete");
        let requested = load_mcp_task(&conn, "cancel-me", "stdio:test-owner", "extension")
            .unwrap()
            .unwrap();
        assert_eq!(requested.status, "working");
        assert!(requested.cancellation_requested);

        complete_cancelled_mcp_task(&db, "cancel-me").unwrap();
        let observed = mcp_task_get(
            &db,
            Some(&json!({"taskId":"cancel-me"})),
            "stdio:test-owner",
            "extension",
            true,
        )
        .unwrap();
        assert_eq!(observed["status"], "cancelled");
        assert_eq!(observed["resultType"], "complete");
    }
}

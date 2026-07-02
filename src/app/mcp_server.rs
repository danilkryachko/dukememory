use super::*;

pub(crate) fn serve_mcp(db: &Path, content_length: bool) -> Result<()> {
    if content_length {
        return serve_mcp_content_length(db);
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                writeln!(
                    stdout,
                    "{}",
                    json!({"jsonrpc":"2.0","error":{"code":-32700,"message":err.to_string()}})
                )?;
                stdout.flush()?;
                continue;
            }
        };
        let response = handle_mcp_request(db, request);
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }
    Ok(())
}

fn serve_mcp_content_length(db: &Path) -> Result<()> {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;
    let mut offset = 0usize;
    let mut stdout = io::stdout();
    while let Some((headers_end, length)) = next_content_length_frame(&input, offset)? {
        let body_start = headers_end;
        let body_end = body_start + length;
        if body_end > input.len() {
            bail!("incomplete MCP frame body");
        }
        let request: Value = serde_json::from_slice(&input[body_start..body_end])?;
        let response = handle_mcp_request(db, request);
        let body = serde_json::to_vec(&response)?;
        write!(stdout, "Content-Length: {}\r\n\r\n", body.len())?;
        stdout.write_all(&body)?;
        stdout.flush()?;
        offset = body_end;
    }
    Ok(())
}

fn next_content_length_frame(input: &[u8], offset: usize) -> Result<Option<(usize, usize)>> {
    let Some(header_pos) = find_bytes(&input[offset..], b"\r\n\r\n") else {
        return Ok(None);
    };
    let header_start = offset;
    let header_end = offset + header_pos + 4;
    let header_text = std::str::from_utf8(&input[header_start..header_end - 4])?;
    let mut length = None;
    for line in header_text.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let Some(length) = length else {
        bail!("missing Content-Length header");
    };
    Ok(Some((header_end, length)))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn handle_mcp_request(db: &Path, request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": "dukememory",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "Call memory_budget_plan when budget is unclear, then memory_brief first for coding tasks. Use memory_impact for a touched file/symbol, memory_drift before larger edits, memory_doctrine for active project decisions, memory_agent_context for broader recall, memory_evidence for provenance, memory_auto_ingest after session logs are written, and memory_doctor before long sessions."
        })),
        "tools/list" => Ok(json!({"tools": mcp_tools()})),
        "tools/call" => {
            handle_mcp_tool_call(db, request.get("params").cloned().unwrap_or_default())
        }
        _ => Err(format!("unsupported method: {method}")),
    };
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(message) => {
            let code = if method.is_empty() { -32600 } else { -32601 };
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    }
}

fn mcp_tools() -> Value {
    let mut tools = json!([
        {"name":"memory_brief","description":"Return a tiny verified task brief","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_impact","description":"Return lightweight impact memory for a file, symbol, or topic","inputSchema":{"type":"object","properties":{"target":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["target"]}},
        {"name":"memory_budget_plan","description":"Choose the smallest useful memory budget for a task","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"scope":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_feedback","description":"Record lightweight useful/useless/missing feedback for memory reads","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"ids":{"type":"array","items":{"type":"string"}},"rating":{"type":"string"},"command":{"type":"string"},"query":{"type":"string"},"note":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["rating"]}},
        {"name":"memory_drift","description":"Detect cheap local memory drift before coding as bounded summary by default","inputSchema":{"type":"object","properties":{"changed_only":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"}}}},
        {"name":"memory_add","description":"Add a typed memory card","inputSchema":{"type":"object","properties":{"type":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"scope":{"type":"string"},"source":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["type","title","body"]}},
        {"name":"memory_remember","description":"Remember plain text as local memory","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"type":{"type":"string"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["text"]}},
        {"name":"memory_search","description":"Search local memory with compact query-focused summaries","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_context_pack","description":"Return a compact relevant memory pack","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_rag_answer","description":"Answer a question using grounded project memory via LLM generation","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"scope":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_graph_rag_answer","description":"Answer a question using 1-hop graph-expanded RAG via LLM generation","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"scope":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_guided_tour","description":"Generate a pedagogical guided tour of the project memory via LLM generation","inputSchema":{"type":"object","properties":{"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":[]}},
        {"name":"memory_explain_component","description":"Perform a Deep Dive explanation of a specific memory component using its neighbors","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_onboard_guide","description":"Generate a comprehensive Onboarding Guide from the knowledge graph","inputSchema":{"type":"object","properties":{"gen_provider":{"type":"string"},"gen_endpoint":{"type":"string"},"gen_model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":[]}},
        {"name":"memory_agent_context","description":"Return agent-native context with planner defaults","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_snapshot","description":"Return compact bounded project snapshot","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_doctrine","description":"Return compact active decision doctrine by default","inputSchema":{"type":"object","properties":{"scope":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_evidence","description":"Return compact provenance for one memory card by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_auto_ingest","description":"Scan agent session files into pending inbox suggestions without duplicates as bounded summary","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"dry_run":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"}}}},
        {"name":"memory_get","description":"Get one memory card as compact summary by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
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
        {"name":"memory_status","description":"Return compact V3 memory status for agent startup","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_should_write","description":"Decide whether a durable memory write is warranted","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"memory_type":{"type":"string"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["text"]}},
        {"name":"memory_after_task","description":"Return compact after-task memory maintenance guidance","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_project_health","description":"Return compact project memory health and role profile","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}
    ]);
    if let Some(items) = tools.as_array_mut() {
        items.extend([
            json!({"name":"memory_recall","description":"Return compressed recall, including recent/as-of/changed-since temporal modes","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"recent":{"type":"boolean"},"as_of":{"type":"string"},"as_of_days_ago":{"type":"number"},"changed_since":{"type":"string"},"changed_since_days":{"type":"number"},"provider":{"type":"string"},"endpoint":{"type":"string"},"model":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}}),
            json!({"name":"memory_upload","description":"Review a local text/markdown/json/csv file as inbox-first memory candidates","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["input"]}}),
            json!({"name":"memory_memanto_gap","description":"Report Memanto-style capability coverage for dukememory","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_timeline","description":"Show one memory card timeline with audit events and real agent reads","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}}),
            json!({"name":"memory_conflict_review","description":"Review duplicate, stale, superseded, and contradiction-prone memory groups","inputSchema":{"type":"object","properties":{"stale_days":{"type":"number"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_effectiveness_v2","description":"Measure memory usefulness with influence, waste, and semantic-read signals","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_recall_baselines","description":"Inspect or write guarded recall benchmark baselines","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_conflict_apply","description":"Dry-run or apply guarded reversible memory conflict-review actions","inputSchema":{"type":"object","properties":{"stale_days":{"type":"number"},"limit":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_mcp_surface_v3","description":"Inspect the MCP V3 memory tool surface","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"}}}}),
            json!({"name":"memory_mcp_discipline_v3","description":"Verify or record MCP V3 memory discipline","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"apply":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
            json!({"name":"memory_fleet_quality","description":"Inspect V3 quality across discovered project memories","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"max_chars":{"type":"number"},"db":{"type":"string"}}}}),
            json!({"name":"memory_release_gate_v3","description":"Gate releases with effectiveness, baselines, conflicts, MCP V3, and fleet visibility","inputSchema":{"type":"object","properties":{"since_days":{"type":"number"},"strict":{"type":"boolean"},"run":{"type":"boolean"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}),
        ]);
    }
    tools
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
    let selected_db = mcp_selected_db(db, &args);
    let conn = open_db(&selected_db).map_err(|err| err.to_string())?;
    let selected_root = mcp_selected_root(&selected_db, &args);
    let text = match name {
        "memory_add" => {
            let memory_type = json_string(&args, "type").unwrap_or_else(|| "note".to_string());
            let title = json_string(&args, "title").ok_or_else(|| "missing title".to_string())?;
            let body = json_string(&args, "body").ok_or_else(|| "missing body".to_string())?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            validate_scope(&scope).map_err(|err| err.to_string())?;
            reject_sensitive(&title, &body, false).map_err(|err| err.to_string())?;
            add_memory(
                &conn,
                AddMemory {
                    id: None,
                    memory_type,
                    title,
                    body,
                    scope,
                    status: "active".to_string(),
                    source: json_string(&args, "source"),
                    supersedes: None,
                    confidence: 1.0,
                    links: Vec::new(),
                },
            )
            .map_err(|err| err.to_string())?
        }
        "memory_remember" => {
            let text = json_string(&args, "text").ok_or_else(|| "missing text".to_string())?;
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            validate_scope(&scope).map_err(|err| err.to_string())?;
            let memory_type = json_string(&args, "type").unwrap_or_else(|| "note".to_string());
            reject_sensitive(&truncate_words(&text, 8), &text, false)
                .map_err(|err| err.to_string())?;
            add_memory(
                &conn,
                AddMemory {
                    id: None,
                    memory_type,
                    title: truncate_words(&text, 8),
                    body: text,
                    scope,
                    status: "active".to_string(),
                    source: Some("mcp".to_string()),
                    supersedes: None,
                    confidence: 0.8,
                    links: Vec::new(),
                },
            )
            .map_err(|err| err.to_string())?
        }
        "memory_search" => {
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
            let scope = json_string(&args, "scope");

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
                    semantic_used: true,
                    result_count: report.citations.len(),
                    budget: 1600,
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
            let scope = json_string(&args, "scope");

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
                &config,
            )
            .map_err(|err| err.to_string())?;

            log_read_event(
                &conn,
                ReadEventInput {
                    command: "memory_graph_rag_answer",
                    query: &query,
                    ids: &report.citations,
                    semantic_used: true,
                    result_count: report.citations.len(),
                    budget: 1600,
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
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let dry_run = args
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let include_body = args
                .get("include_body")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let report = auto_ingest_sessions(
                &conn,
                Path::new(&input),
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
        "memory_get" => {
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
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let report =
                memory_upload_report(&conn, &selected_root, Path::new(&input), &scope, apply)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["candidates", "quality_checks"])
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
            let strict = args.get("strict").and_then(Value::as_bool).unwrap_or(false);
            let run = args.get("run").and_then(Value::as_bool).unwrap_or(false);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(2200);
            let report = release_gate_v3_report(
                &conn,
                &selected_db,
                &selected_root,
                since_days,
                strict,
                run,
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
            let report =
                web_control_center_v3_report(&conn, &selected_db, &selected_root, None, since_days)
                    .map_err(|err| err.to_string())?;
            budgeted_mcp_json_response(&report, max_chars, &["tabs", "primary_actions"])
                .map_err(|err| err.to_string())?
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
    Ok(json!({"content":[{"type":"text","text":text}]}))
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

fn mcp_selected_db(default_db: &Path, args: &Value) -> PathBuf {
    if let Some(db) = json_string(args, "db").filter(|value| !value.trim().is_empty()) {
        return expand_mcp_path(&db);
    }
    for key in ["root", "project_root", "project"] {
        if let Some(root) = json_string(args, key).filter(|value| !value.trim().is_empty()) {
            return project_memory_db(&root);
        }
    }
    if let Some(scope) = json_string(args, "scope")
        && mcp_scope_looks_like_project_root(&scope)
    {
        return project_memory_db(&scope);
    }
    default_db.to_path_buf()
}

fn mcp_selected_root(selected_db: &Path, args: &Value) -> PathBuf {
    for key in ["root", "project_root", "project"] {
        if let Some(root) = json_string(args, key).filter(|value| !value.trim().is_empty()) {
            return expand_mcp_path(&root);
        }
    }
    app_project_root_for_db(selected_db).unwrap_or_else(|| {
        selected_db
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

fn mcp_memory_scope(args: &Value) -> Option<String> {
    json_string(args, "scope").filter(|scope| VALID_SCOPES.contains(&scope.as_str()))
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
    if VALID_SCOPES.contains(&value) {
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
}

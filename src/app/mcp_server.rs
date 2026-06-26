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
            "instructions": "Call memory_brief first for coding tasks. Use memory_impact for a touched file/symbol, memory_drift before larger edits, memory_doctrine for active project decisions, memory_agent_context for broader recall, memory_evidence for provenance, memory_auto_ingest after session logs are written, and memory_doctor before long sessions."
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
    json!([
        {"name":"memory_brief","description":"Return a tiny verified task brief","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"scope":{"type":"string"}},"required":["task"]}},
        {"name":"memory_impact","description":"Return lightweight impact memory for a file, symbol, or topic","inputSchema":{"type":"object","properties":{"target":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"scope":{"type":"string"}},"required":["target"]}},
        {"name":"memory_drift","description":"Detect cheap local memory drift before coding","inputSchema":{"type":"object","properties":{"changed_only":{"type":"boolean"},"root":{"type":"string"}}}},
        {"name":"memory_add","description":"Add a typed memory card","inputSchema":{"type":"object","properties":{"type":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"scope":{"type":"string"},"source":{"type":"string"}},"required":["type","title","body"]}},
        {"name":"memory_remember","description":"Remember plain text as local memory","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"type":{"type":"string"},"scope":{"type":"string"}},"required":["text"]}},
        {"name":"memory_search","description":"Search local memory","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"}},"required":["query"]}},
        {"name":"memory_context_pack","description":"Return a compact relevant memory pack","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"}},"required":["task"]}},
        {"name":"memory_agent_context","description":"Return agent-native context with planner defaults","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"}},"required":["task"]}},
        {"name":"memory_snapshot","description":"Return compact project snapshot","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"}}}},
        {"name":"memory_doctrine","description":"Return active decision doctrine and supersession chains","inputSchema":{"type":"object","properties":{"scope":{"type":"string"}}}},
        {"name":"memory_evidence","description":"Return provenance for one memory card","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"memory_auto_ingest","description":"Scan agent session files into pending inbox suggestions without duplicates","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"dry_run":{"type":"boolean"}}}},
        {"name":"memory_get","description":"Get one memory card","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}},
        {"name":"memory_review","description":"Review stale/conflicting memory","inputSchema":{"type":"object","properties":{}}},
        {"name":"memory_doctor","description":"Run memory health checks","inputSchema":{"type":"object","properties":{}}},
        {"name":"memory_inbox_list","description":"List pending inbox items","inputSchema":{"type":"object","properties":{"limit":{"type":"number"}}}}
    ])
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
    let conn = open_db(db).map_err(|err| err.to_string())?;
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
            let query = json_string(&args, "query").ok_or_else(|| "missing query".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let rows = query_memories(
                &conn,
                Some(&query),
                &[],
                &["active".to_string()],
                None,
                limit,
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&rows).map_err(|err| err.to_string())?
        }
        "memory_context_pack" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(4000);
            let statuses = ["active".to_string(), "uncertain".to_string()];
            let rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &[],
                    statuses: &statuses,
                    scope: None,
                    limit,
                    include_recent: 3,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            render_context_pack(&conn, &rows, max_chars).map_err(|err| err.to_string())?
        }
        "memory_agent_context" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(5000);
            let statuses = ["active".to_string(), "uncertain".to_string()];
            let rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &[],
                    statuses: &statuses,
                    scope: None,
                    limit,
                    include_recent: 4,
                    rules: None,
                },
            )
            .map_err(|err| err.to_string())?;
            render_context_pack(&conn, &rows, max_chars).map_err(|err| err.to_string())?
        }
        "memory_snapshot" => {
            let max_chars = json_usize(&args, "max_chars").unwrap_or(8000);
            let rows = query_memories(
                &conn,
                None,
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                30,
            )
            .map_err(|err| err.to_string())?;
            render_context_pack(&conn, &rows, max_chars).map_err(|err| err.to_string())?
        }
        "memory_brief" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let scope = json_string(&args, "scope");
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
                },
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_impact" => {
            let target =
                json_string(&args, "target").ok_or_else(|| "missing target".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let scope = json_string(&args, "scope");
            let report = impact_report(
                &conn,
                &ImpactRequest {
                    target: &target,
                    limit,
                    budget,
                    scope: scope.as_deref(),
                    json_out: true,
                },
            )
            .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_drift" => {
            let changed_only = args
                .get("changed_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let root = json_string(&args, "root").unwrap_or_else(|| ".".to_string());
            let report = drift_report(&conn, Path::new(&root), changed_only)
                .map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_doctrine" => {
            let scope = json_string(&args, "scope");
            let report = doctrine_report(&conn, scope.as_deref()).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_evidence" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let report = evidence_report(&conn, &id).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_auto_ingest" => {
            let input =
                json_string(&args, "input").unwrap_or_else(|| ".agent/sessions".to_string());
            let scope = json_string(&args, "scope").unwrap_or_else(|| "project".to_string());
            let dry_run = args
                .get("dry_run")
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
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        }
        "memory_get" => {
            let id = json_string(&args, "id").ok_or_else(|| "missing id".to_string())?;
            let memory = get_memory_with_links(&conn, &id).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&memory).map_err(|err| err.to_string())?
        }
        "memory_review" => {
            let mut issues = Vec::new();
            issues.extend(review_stale(&conn, 30).map_err(|err| err.to_string())?);
            issues.extend(review_uncertain(&conn).map_err(|err| err.to_string())?);
            issues.extend(review_low_confidence(&conn).map_err(|err| err.to_string())?);
            issues.extend(review_duplicates(&conn).map_err(|err| err.to_string())?);
            serde_json::to_string_pretty(&issues).map_err(|err| err.to_string())?
        }
        "memory_doctor" => {
            let secrets = scan_secret_findings(&conn).map_err(|err| err.to_string())?;
            let pending = list_inbox(&conn, "pending", usize::MAX)
                .map_err(|err| err.to_string())?
                .len();
            serde_json::to_string_pretty(&json!({
                "secrets": secrets.len(),
                "pending_inbox": pending
            }))
            .map_err(|err| err.to_string())?
        }
        "memory_inbox_list" => {
            let limit = json_usize(&args, "limit").unwrap_or(20);
            let rows = list_inbox(&conn, "pending", limit).map_err(|err| err.to_string())?;
            serde_json::to_string_pretty(&rows).map_err(|err| err.to_string())?
        }
        other => return Err(format!("unsupported tool: {other}")),
    };
    Ok(json!({"content":[{"type":"text","text":text}]}))
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn json_usize(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

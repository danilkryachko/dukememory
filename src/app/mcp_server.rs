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
        {"name":"memory_brief","description":"Return a tiny verified task brief","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_impact","description":"Return lightweight impact memory for a file, symbol, or topic","inputSchema":{"type":"object","properties":{"target":{"type":"string"},"limit":{"type":"number"},"budget":{"type":"number"},"max_chars":{"type":"number"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["target"]}},
        {"name":"memory_drift","description":"Detect cheap local memory drift before coding as bounded summary by default","inputSchema":{"type":"object","properties":{"changed_only":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"}}}},
        {"name":"memory_add","description":"Add a typed memory card","inputSchema":{"type":"object","properties":{"type":{"type":"string"},"title":{"type":"string"},"body":{"type":"string"},"scope":{"type":"string"},"source":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["type","title","body"]}},
        {"name":"memory_remember","description":"Remember plain text as local memory","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"type":{"type":"string"},"scope":{"type":"string"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["text"]}},
        {"name":"memory_search","description":"Search local memory with compact query-focused summaries","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["query"]}},
        {"name":"memory_context_pack","description":"Return a compact relevant memory pack","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_agent_context","description":"Return agent-native context with planner defaults","inputSchema":{"type":"object","properties":{"task":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["task"]}},
        {"name":"memory_snapshot","description":"Return compact bounded project snapshot","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"number"},"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_doctrine","description":"Return compact active decision doctrine by default","inputSchema":{"type":"object","properties":{"scope":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_evidence","description":"Return compact provenance for one memory card by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_auto_ingest","description":"Scan agent session files into pending inbox suggestions without duplicates as bounded summary","inputSchema":{"type":"object","properties":{"input":{"type":"string"},"scope":{"type":"string"},"dry_run":{"type":"boolean"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"}}}},
        {"name":"memory_get","description":"Get one memory card as compact summary by default","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}},"required":["id"]}},
        {"name":"memory_review","description":"Review stale/conflicting memory as a bounded summary","inputSchema":{"type":"object","properties":{"limit":{"type":"number"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_doctor","description":"Run compact memory health checks","inputSchema":{"type":"object","properties":{"max_chars":{"type":"number"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}},
        {"name":"memory_inbox_list","description":"List pending inbox items as compact summaries by default","inputSchema":{"type":"object","properties":{"limit":{"type":"number"},"query":{"type":"string"},"max_chars":{"type":"number"},"include_body":{"type":"boolean"},"root":{"type":"string"},"project_root":{"type":"string"},"db":{"type":"string"}}}}
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
    let selected_db = mcp_selected_db(db, &args);
    let conn = open_db(&selected_db).map_err(|err| err.to_string())?;
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
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let rows = query_memories(
                &conn,
                Some(&query),
                &[],
                &["active".to_string()],
                None,
                limit,
            )
            .map_err(|err| err.to_string())?;
            compact_mcp_search_response(&rows, &query, max_chars).map_err(|err| err.to_string())?
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
            render_context_pack_for_task(&conn, &rows, max_chars, &task)
                .map_err(|err| err.to_string())?
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
            render_context_pack_for_task(&conn, &rows, max_chars, &task)
                .map_err(|err| err.to_string())?
        }
        "memory_snapshot" => {
            let query = json_string(&args, "query").unwrap_or_default();
            let limit = json_usize(&args, "limit").unwrap_or(12);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(1200);
            let rows = query_memories(
                &conn,
                None,
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                limit,
            )
            .map_err(|err| err.to_string())?;
            if query.trim().is_empty() {
                render_context_pack(&conn, &rows, max_chars).map_err(|err| err.to_string())?
            } else {
                render_context_pack_for_task(&conn, &rows, max_chars, &query)
                    .map_err(|err| err.to_string())?
            }
        }
        "memory_brief" => {
            let task = json_string(&args, "task").ok_or_else(|| "missing task".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(budget);
            let scope = mcp_memory_scope(&args);
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
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &["checks", "files", "risks", "relevant", "must_follow"],
            )
            .map_err(|err| err.to_string())?
        }
        "memory_impact" => {
            let target =
                json_string(&args, "target").ok_or_else(|| "missing target".to_string())?;
            let limit = json_usize(&args, "limit").unwrap_or(10);
            let budget = json_usize(&args, "budget").unwrap_or(1200);
            let max_chars = json_usize(&args, "max_chars").unwrap_or(budget);
            let scope = mcp_memory_scope(&args);
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
            budgeted_mcp_json_response(
                &report,
                max_chars,
                &[
                    "links",
                    "checks",
                    "related",
                    "risks",
                    "constraints",
                    "decisions",
                ],
            )
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
            let rendered = serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?;
            truncate_chars(&rendered, max_chars)
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
        other => return Err(format!("unsupported tool: {other}")),
    };
    Ok(json!({"content":[{"type":"text","text":text}]}))
}

fn compact_mcp_search_response(rows: &[Memory], query: &str, max_chars: usize) -> Result<String> {
    let mut items = Vec::new();
    for row in rows {
        items.push(compact_mcp_memory_value(row, &[], query));
        let rendered = serde_json::to_string_pretty(&items)?;
        if rendered.len() > max_chars {
            items.pop();
            break;
        }
    }
    let rendered = serde_json::to_string_pretty(&items)?;
    Ok(truncate_chars(&rendered, max_chars))
}

fn compact_mcp_memory_response(
    memory: &MemoryWithLinks,
    query: &str,
    max_chars: usize,
) -> Result<String> {
    let rendered = serde_json::to_string_pretty(&compact_mcp_memory_value(
        &memory.memory,
        &memory.links,
        query,
    ))?;
    Ok(truncate_chars(&rendered, max_chars))
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
    Ok(truncate_chars(
        &serde_json::to_string_pretty(&value)?,
        max_chars,
    ))
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
    fit_json_array_sections(
        &mut value,
        max_chars,
        &["superseded", "conflicts", "active"],
    )?;
    Ok(serde_json::to_string_pretty(&value)?)
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

fn json_usize(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

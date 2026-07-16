use super::*;

pub(super) fn route_ingest_operation(
    default_db: &Path,
    conn: &Connection,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
) -> Result<Option<HttpResponse>> {
    let response = match (method, path) {
        ("POST", "/import-review/apply") => {
            let value = parse_json_body(body)?;
            let ctx = selected_project_from_body(default_db, &value)?;
            let input = value
                .get("input")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| ctx.root.join("README.md"));
            let input = resolve_project_input(&ctx.root, &input)?;
            let scope = value
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let apply = value.get("apply").and_then(Value::as_bool).unwrap_or(false);
            HttpResponse::ok(json!({"import_review": import_review_report(
                conn, &ctx.root, &input, scope, apply,
            )?}))
        }
        ("POST", "/memory-upload") => {
            let value = parse_json_body(body)?;
            let ctx = selected_project_from_body(default_db, &value)?;
            let input = value
                .get("input")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .with_context(|| "memory-upload requires input")?;
            let input = resolve_project_input(&ctx.root, &input)?;
            let scope = value
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let apply = value.get("apply").and_then(Value::as_bool).unwrap_or(false);
            HttpResponse::ok(json!({"memory_upload": memory_upload_report(
                conn, &ctx.root, &input, scope, apply,
            )?}))
        }
        ("POST", "/rag-ingest") => {
            let value = parse_json_body(body)?;
            let ctx = selected_project_from_body(default_db, &value)?;
            let input = value
                .get("input")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .with_context(|| "rag-ingest requires input")?;
            let input = resolve_project_input(&ctx.root, &input)?;
            let scope = value
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let apply = value.get("apply").and_then(Value::as_bool).unwrap_or(false);
            let embed = value.get("embed").and_then(Value::as_bool).unwrap_or(false);
            let reviewed = value
                .get("reviewed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
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
            HttpResponse::ok(json!({"rag_ingest": rag_ingest_report(
                conn,
                RagIngestRequest {
                    root: &ctx.root,
                    input: &input,
                    scope,
                    apply,
                    reviewed,
                    embed,
                    provider,
                    endpoint,
                    model,
                    chunk_chars: value.get("chunk_chars").and_then(Value::as_u64).unwrap_or(900) as usize,
                    overlap_chars: value.get("overlap_chars").and_then(Value::as_u64).unwrap_or(140) as usize,
                    max_file_bytes: value.get("max_file_bytes").and_then(Value::as_u64).unwrap_or(200_000) as usize,
                    max_files: value.get("max_files").and_then(Value::as_u64).unwrap_or(128) as usize,
                    json: true,
                },
            )?}))
        }
        ("GET", "/rag-sources") => {
            let params = parse_query(query);
            let selected = params.get("project").map(String::as_str);
            let ctx = project_context(default_db, selected)?;
            let provider = params
                .get("provider")
                .map(String::as_str)
                .unwrap_or(DEFAULT_EMBED_PROVIDER);
            let endpoint = params
                .get("endpoint")
                .map(String::as_str)
                .unwrap_or(DEFAULT_EMBED_ENDPOINT);
            let model = params
                .get("model")
                .map(String::as_str)
                .unwrap_or(DEFAULT_EMBED_MODEL);
            HttpResponse::ok(json!({"rag_sources": rag_sources_report(
                conn, &ctx.root, provider, endpoint, model,
            )?}))
        }
        ("POST", "/auto-ingest") => {
            let value = parse_json_body(body)?;
            let ctx = selected_project_from_body(default_db, &value)?;
            let input = value
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or(".agent/sessions");
            let input = resolve_project_input(&ctx.root, Path::new(input))?;
            let scope = value
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let dry_run = value
                .get("dry_run")
                .and_then(Value::as_bool)
                .or_else(|| {
                    value
                        .get("apply")
                        .and_then(Value::as_bool)
                        .map(|apply| !apply)
                })
                .unwrap_or(true);
            let report = auto_ingest_sessions(
                conn,
                &input,
                scope,
                false,
                DEFAULT_EMBED_ENDPOINT,
                "qwen3:14b",
                dry_run,
            )?;
            HttpResponse::ok(json!({"auto_ingest": report}))
        }
        _ => return Ok(None),
    };
    Ok(Some(response))
}

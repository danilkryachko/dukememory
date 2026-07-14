use super::http_server::{
    filter_sort_memory_rows, memory_rows_with_request_counts, parse_json_body, parse_query,
};
use super::*;

pub(crate) fn route_memory_operation(
    conn: &Connection,
    memory_app: &MemoryApplication<'_>,
    method: &str,
    path: &str,
    query: &str,
    body: &str,
) -> Result<Option<HttpResponse>> {
    let response = match (method, path) {
        ("GET", HTTP_OPERATIONS) => HttpResponse::ok(json!({
            "version": 1,
            "operations": CORE_OPERATION_CATALOG,
        })),
        ("GET", HTTP_MEMORY_GET) => list_memories(conn, query)?,
        ("POST", HTTP_REMEMBER) => remember(memory_app, body)?,
        ("POST", HTTP_MEMORY_STATUS) => update_status(memory_app, body)?,
        ("POST", HTTP_MEMORY_DELETE) => delete_memory_route(memory_app, body)?,
        ("POST", HTTP_MEMORY_UPDATE) => update_memory_route(memory_app, body)?,
        ("POST", HTTP_SEARCH) => search_memories(conn, body)?,
        _ => return Ok(None),
    };
    Ok(Some(response))
}

fn list_memories(conn: &Connection, query: &str) -> Result<HttpResponse> {
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
    let rows = if let Some(query) = q {
        search_rows_with_semantic_fallback(
            conn,
            SearchRowsRequest {
                query,
                types: &types,
                statuses: &statuses,
                scope,
                limit: if usage != "all" || sort != "updated_desc" {
                    500
                } else {
                    limit
                },
                budget: 1_200,
                provider: DEFAULT_EMBED_PROVIDER,
                endpoint: DEFAULT_EMBED_ENDPOINT,
                model: DEFAULT_EMBED_MODEL,
            },
        )?
        .0
    } else {
        query_memories(
            conn,
            None,
            &types,
            &statuses,
            scope,
            if usage != "all" || sort != "updated_desc" {
                500
            } else {
                limit
            },
        )?
    };
    let rows = if let Some(query) = q {
        let quality_signals = retrieval_feedback_signals(conn, 30).unwrap_or_default();
        filter_query_useless_memories(rows, query, &quality_signals)
    } else {
        rows
    };
    Ok(HttpResponse::ok(
        json!({"memories": filter_sort_memory_rows(conn, rows, usage, sort, stale_days, limit)?}),
    ))
}

fn remember(memory_app: &MemoryApplication<'_>, body: &str) -> Result<HttpResponse> {
    let value = parse_json_body(body)?;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if text.is_empty() {
        return Ok(HttpResponse::bad_request("missing text"));
    }
    let id = memory_app.create(AddMemory {
        id: None,
        memory_type: value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("note")
            .parse()?,
        title: truncate_words(text, 8),
        body: text.to_string(),
        scope: value
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("project")
            .parse()?,
        status: MemoryStatus::Active,
        source: Some("http".to_string()),
        supersedes: None,
        confidence: 0.8,
        layer: value
            .get("layer")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        links: Vec::new(),
        allow_sensitive: false,
    })?;
    Ok(HttpResponse::ok(json!({"id": id})))
}

fn update_status(memory_app: &MemoryApplication<'_>, body: &str) -> Result<HttpResponse> {
    let value = parse_json_body(body)?;
    let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if id.is_empty() || status.is_empty() {
        return Ok(HttpResponse::bad_request("missing id or status"));
    }
    memory_app.set_status(id, status.parse()?)?;
    Ok(HttpResponse::ok(
        json!({"ok": true, "id": id, "status": status}),
    ))
}

fn delete_memory_route(memory_app: &MemoryApplication<'_>, body: &str) -> Result<HttpResponse> {
    let value = parse_json_body(body)?;
    let id = value.get("id").and_then(Value::as_str).unwrap_or_default();
    if id.is_empty() {
        return Ok(HttpResponse::bad_request("missing id"));
    }
    memory_app.delete(id)?;
    Ok(HttpResponse::ok(json!({"ok": true, "id": id})))
}

fn update_memory_route(memory_app: &MemoryApplication<'_>, body: &str) -> Result<HttpResponse> {
    let value = parse_json_body(body)?;
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
    memory_app.update(UpdateMemory {
        id: id.to_string(),
        memory_type: value
            .get("type")
            .and_then(Value::as_str)
            .map(str::parse)
            .transpose()?,
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
            .map(str::parse)
            .transpose()?,
        status: value
            .get("status")
            .and_then(Value::as_str)
            .map(str::parse)
            .transpose()?,
        source: value
            .get("source")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        confidence: value.get("confidence").and_then(Value::as_f64),
        layer: value
            .get("layer")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        links,
        replace_links: value
            .get("replace_links")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allow_sensitive: false,
    })?;
    Ok(HttpResponse::ok(
        json!({"ok": true, "memory": memory_app.get_with_links(id)?}),
    ))
}

fn search_memories(conn: &Connection, body: &str) -> Result<HttpResponse> {
    let value = parse_json_body(body)?;
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
    let (rows, _) = search_rows_with_semantic_fallback(
        conn,
        SearchRowsRequest {
            query,
            types: &[],
            statuses: &["active".to_string(), "uncertain".to_string()],
            scope: None,
            limit,
            budget: 1_200,
            provider: DEFAULT_EMBED_PROVIDER,
            endpoint: DEFAULT_EMBED_ENDPOINT,
            model: DEFAULT_EMBED_MODEL,
        },
    )?;
    let quality_signals = retrieval_feedback_signals(conn, 30).unwrap_or_default();
    let mut rows = filter_query_useless_memories(rows, query, &quality_signals);
    rows.truncate(limit);
    Ok(HttpResponse::ok(
        json!({"results": memory_rows_with_request_counts(conn, rows)?}),
    ))
}

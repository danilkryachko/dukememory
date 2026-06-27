use super::{
    Memory, MemoryLink, MemoryWithLinks, log_event, now_ms, placeholders, relevance_terms,
    sanitize_fts_any_query, sanitize_fts_query,
};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use uuid::Uuid;

pub(crate) struct AddMemory {
    pub(crate) id: Option<String>,
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) scope: String,
    pub(crate) status: String,
    pub(crate) source: Option<String>,
    pub(crate) supersedes: Option<String>,
    pub(crate) confidence: f64,
    pub(crate) links: Vec<String>,
}

pub(crate) struct UpdateMemory {
    pub(crate) id: String,
    pub(crate) memory_type: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) confidence: Option<f64>,
    pub(crate) links: Vec<String>,
    pub(crate) replace_links: bool,
}

pub(crate) fn add_memory(conn: &Connection, input: AddMemory) -> Result<String> {
    validate_confidence(input.confidence)?;
    let id = input
        .id
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string()[..12].to_string());
    let ts = now_ms();
    let links = parse_links(&input.links)?;

    conn.execute(
        r#"
        INSERT INTO memories (
            id, type, scope, title, body, status, source,
            created_at, updated_at, supersedes, confidence
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            id,
            input.memory_type,
            input.scope,
            input.title,
            input.body,
            input.status,
            input.source,
            ts,
            ts,
            input.supersedes,
            input.confidence,
        ],
    )?;
    insert_links(conn, &id, &links)?;

    if let Some(old_id) = input.supersedes.as_deref() {
        conn.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
            params![id, ts, old_id],
        )?;
    }

    log_event(conn, "memory_added", Some(&id), "added memory card")?;
    Ok(id)
}

pub(crate) fn update_memory(conn: &Connection, input: UpdateMemory) -> Result<()> {
    let mut memory = get_memory(conn, &input.id)?;
    if let Some(value) = input.memory_type {
        memory.memory_type = value;
    }
    if let Some(value) = input.title {
        memory.title = value;
    }
    if let Some(value) = input.body {
        memory.body = value;
    }
    if let Some(value) = input.scope {
        memory.scope = value;
    }
    if let Some(value) = input.status {
        memory.status = value;
    }
    if let Some(value) = input.source {
        memory.source = Some(value);
    }
    if let Some(value) = input.confidence {
        validate_confidence(value)?;
        memory.confidence = value;
    }
    memory.updated_at = now_ms();

    conn.execute(
        r#"
        UPDATE memories SET
            type = ?1, scope = ?2, title = ?3, body = ?4, status = ?5,
            source = ?6, updated_at = ?7, confidence = ?8
        WHERE id = ?9
        "#,
        params![
            memory.memory_type,
            memory.scope,
            memory.title,
            memory.body,
            memory.status,
            memory.source,
            memory.updated_at,
            memory.confidence,
            memory.id,
        ],
    )?;

    let links = parse_links(&input.links)?;
    if input.replace_links {
        conn.execute(
            "DELETE FROM memory_links WHERE memory_id = ?1",
            params![input.id],
        )?;
    }
    insert_links(conn, &input.id, &links)?;
    log_event(
        conn,
        "memory_updated",
        Some(&input.id),
        "updated memory card",
    )?;
    println!("{}", input.id);
    Ok(())
}

pub(crate) fn delete_memory(conn: &Connection, id: &str) -> Result<()> {
    let changed = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
    if changed == 0 {
        bail!("Memory not found: {id}");
    }
    log_event(conn, "memory_deleted", Some(id), "deleted memory card")?;
    println!("{id}");
    Ok(())
}

pub(crate) fn set_status(conn: &Connection, id: &str, status: String) -> Result<()> {
    let changed = conn.execute(
        "UPDATE memories SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now_ms(), id],
    )?;
    if changed == 0 {
        bail!("Memory not found: {id}");
    }
    log_event(
        conn,
        "memory_status",
        Some(id),
        &format!("set status to {status}"),
    )?;
    println!("{id}");
    Ok(())
}

pub(crate) fn get_memory(conn: &Connection, id: &str) -> Result<Memory> {
    conn.query_row(
        "SELECT * FROM memories WHERE id = ?1",
        params![id],
        row_to_memory,
    )
    .optional()?
    .with_context(|| format!("Memory not found: {id}"))
}

pub(crate) fn get_memory_with_links(conn: &Connection, id: &str) -> Result<MemoryWithLinks> {
    let memory = get_memory(conn, id)?;
    let links = get_links(conn, id)?;
    Ok(MemoryWithLinks { memory, links })
}

pub(crate) fn get_links(conn: &Connection, id: &str) -> Result<Vec<MemoryLink>> {
    let mut stmt =
        conn.prepare("SELECT kind, target FROM memory_links WHERE memory_id = ?1 ORDER BY id ASC")?;
    stmt.query_map(params![id], |row| {
        Ok(MemoryLink {
            kind: row.get(0)?,
            target: row.get(1)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn query_memories(
    conn: &Connection,
    query: Option<&str>,
    types: &[String],
    statuses: &[String],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    let rows = query_memories_with_fts(
        conn,
        query.map(sanitize_fts_query),
        types,
        statuses,
        scope,
        limit,
    )?;
    if !rows.is_empty() || query.is_none() {
        return Ok(rows);
    }
    let query = query.unwrap_or_default();
    let Some(fallback_query) = sanitize_fts_any_query(query) else {
        return Ok(rows);
    };
    let candidate_limit = limit.saturating_mul(6).clamp(limit.max(1), 50);
    let candidates = query_memories_with_fts(
        conn,
        Some(fallback_query),
        types,
        statuses,
        scope,
        candidate_limit,
    )?;
    let mut terms = relevance_terms(query).into_iter().collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    let threshold = terms.len().clamp(1, 3);
    Ok(candidates
        .into_iter()
        .filter(|memory| memory_text_overlap(memory, &terms) >= threshold)
        .take(limit)
        .collect())
}

fn query_memories_with_fts(
    conn: &Connection,
    fts_query: Option<String>,
    types: &[String],
    statuses: &[String],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    let mut sql = String::from("SELECT m.* FROM memories m ");
    let mut where_parts = Vec::new();
    let mut values = Vec::new();
    let has_fts = fts_query.is_some();

    if let Some(query) = fts_query {
        sql.push_str("JOIN memories_fts fts ON fts.rowid = m.rowid ");
        where_parts.push("memories_fts MATCH ?".to_string());
        values.push(query);
    }
    if !types.is_empty() {
        where_parts.push(format!("m.type IN ({})", placeholders(types.len())));
        values.extend(types.iter().cloned());
    }
    if !statuses.is_empty() {
        where_parts.push(format!("m.status IN ({})", placeholders(statuses.len())));
        values.extend(statuses.iter().cloned());
    }
    if let Some(scope) = scope {
        where_parts.push("m.scope = ?".to_string());
        values.push(scope.to_string());
    }
    if !where_parts.is_empty() {
        sql.push_str("WHERE ");
        sql.push_str(&where_parts.join(" AND "));
        sql.push(' ');
    }
    if has_fts {
        sql.push_str("ORDER BY bm25(memories_fts), m.confidence DESC, m.updated_at DESC ");
    } else {
        sql.push_str("ORDER BY m.updated_at DESC ");
    }
    sql.push_str("LIMIT ?");
    values.push(limit.min(i64::MAX as usize).to_string());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), row_to_memory)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn memory_text_overlap(memory: &Memory, terms: &[String]) -> usize {
    let haystack = format!("{} {}", memory.title, memory.body).to_lowercase();
    terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
}

pub(crate) fn row_to_memory(row: &Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get("id")?,
        memory_type: row.get("type")?,
        scope: row.get("scope")?,
        title: row.get("title")?,
        body: row.get("body")?,
        status: row.get("status")?,
        source: row.get("source")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        supersedes: row.get("supersedes")?,
        superseded_by: row.get("superseded_by")?,
        confidence: row.get("confidence")?,
    })
}

pub(crate) fn parse_links(raw_links: &[String]) -> Result<Vec<MemoryLink>> {
    raw_links.iter().map(|raw| parse_link(raw)).collect()
}

fn parse_link(raw: &str) -> Result<MemoryLink> {
    let Some((kind, target)) = raw.split_once(':') else {
        bail!("links must look like kind:target, for example file:src/app.ts");
    };
    let kind = kind.trim();
    let target = target.trim();
    if kind.is_empty() || target.is_empty() {
        bail!("links must include both kind and target");
    }
    Ok(MemoryLink {
        kind: kind.to_string(),
        target: target.to_string(),
    })
}

pub(crate) fn insert_links(conn: &Connection, memory_id: &str, links: &[MemoryLink]) -> Result<()> {
    for link in links {
        conn.execute(
            "INSERT INTO memory_links (memory_id, kind, target) VALUES (?1, ?2, ?3)",
            params![memory_id, link.kind, link.target],
        )?;
    }
    Ok(())
}

pub(crate) fn validate_confidence(confidence: f64) -> Result<()> {
    if !(0.0..=1.0).contains(&confidence) {
        bail!("confidence must be between 0.0 and 1.0");
    }
    Ok(())
}

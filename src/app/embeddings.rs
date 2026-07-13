use super::*;
use sha2::{Digest, Sha256};

const PROVIDER_HEALTH_TIMEOUT_MS: u64 = 1_500;
const PROVIDER_HEALTH_OK_CACHE_MS: i64 = 5_000;
const PROVIDER_HEALTH_DOWN_COOLDOWN_MS: i64 = 60_000;

#[derive(Debug, Clone)]
struct RagChunkEmbeddingTarget {
    id: String,
    source_id: i64,
    path: String,
    scope: String,
    start_line: usize,
    end_line: usize,
    content: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticRagChunkRow {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) scope: String,
    pub(crate) chunk_index: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) content: String,
    pub(crate) score: f64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RagChunkEmbeddingFreshness {
    pub(crate) eligible: usize,
    pub(crate) indexed: usize,
    pub(crate) stale: usize,
    pub(crate) missing: usize,
}

pub(crate) fn embed_index(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    statuses: &[String],
    limit: Option<usize>,
    force: bool,
) -> Result<EmbeddingIndexReport> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let statuses = if statuses.is_empty() {
        vec!["active".to_string(), "uncertain".to_string()]
    } else {
        statuses.to_vec()
    };
    let rows = query_memories(
        conn,
        None,
        &[],
        &statuses,
        None,
        limit.unwrap_or(usize::MAX),
    )?;
    let mut skipped = 0;
    let mut pending = Vec::new();
    for memory in rows {
        let content = embedding_content(&memory);
        let hash = content_hash(&content);
        if !force && embedding_is_current(conn, &memory.id, &endpoint_key, model, &hash)? {
            skipped += 1;
            continue;
        }
        pending.push((memory, content, hash));
    }
    let chunk_rows = query_rag_chunk_embedding_targets(conn, limit)?;
    let mut rag_chunks_skipped = 0;
    let mut pending_chunks = Vec::new();
    for chunk in chunk_rows {
        let content = rag_chunk_embedding_content(
            &chunk.path,
            &chunk.scope,
            chunk.start_line,
            chunk.end_line,
            &chunk.content,
        );
        let hash = content_hash(&content);
        if !force && rag_chunk_embedding_is_current(conn, &chunk.id, &endpoint_key, model, &hash)? {
            rag_chunks_skipped += 1;
            continue;
        }
        pending_chunks.push((chunk, content, hash));
    }
    let provider_health = if pending.is_empty() && pending_chunks.is_empty() {
        None
    } else {
        Some(embedding_provider_health(conn, provider, endpoint))
    };
    if let Some(health) = provider_health
        && !health.reachable
    {
        let detail = health
            .error
            .map(|error| format!(": {error}"))
            .unwrap_or_default();
        bail!("embedding provider is not reachable; skipping embed-index{detail}");
    }
    let mut indexed = 0;
    for (memory, content, hash) in pending {
        let embedding = fetch_embedding(provider, endpoint, model, &content)
            .with_context(|| format!("embedding failed for memory {}", memory.id))?;
        store_embedding(conn, &memory.id, &endpoint_key, model, &hash, &embedding)?;
        indexed += 1;
    }
    let mut rag_chunks_indexed = 0;
    for (chunk, content, hash) in pending_chunks {
        let embedding = fetch_embedding(provider, endpoint, model, &content)
            .with_context(|| format!("embedding failed for RAG chunk {}", chunk.id))?;
        store_rag_chunk_embedding(conn, &chunk.id, &endpoint_key, model, &hash, &embedding)?;
        rag_chunks_indexed += 1;
    }
    Ok(EmbeddingIndexReport {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        indexed,
        skipped,
        rag_chunks_indexed,
        rag_chunks_skipped,
    })
}

pub(crate) fn embed_rag_chunks_for_paths(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    scope: &str,
    paths: &[String],
    force: bool,
) -> Result<EmbeddingIndexReport> {
    let mut unique_paths = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        if seen.insert(path.as_str()) {
            unique_paths.push(path.clone());
        }
    }
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let chunk_rows = query_rag_chunk_embedding_targets_for_paths(conn, scope, &unique_paths)?;
    let mut rag_chunks_skipped = 0;
    let mut pending_chunks = Vec::new();
    for chunk in chunk_rows {
        let content = rag_chunk_embedding_content(
            &chunk.path,
            &chunk.scope,
            chunk.start_line,
            chunk.end_line,
            &chunk.content,
        );
        let hash = content_hash(&content);
        if !force && rag_chunk_embedding_is_current(conn, &chunk.id, &endpoint_key, model, &hash)? {
            rag_chunks_skipped += 1;
            continue;
        }
        pending_chunks.push((chunk, content, hash));
    }
    if !pending_chunks.is_empty() {
        let health = embedding_provider_health(conn, provider, endpoint);
        if !health.reachable {
            let detail = health
                .error
                .map(|error| format!(": {error}"))
                .unwrap_or_default();
            bail!("embedding provider is not reachable; skipping embed-index{detail}");
        }
    }
    let mut rag_chunks_indexed = 0;
    for (chunk, content, hash) in pending_chunks {
        let embedding = fetch_embedding(provider, endpoint, model, &content)
            .with_context(|| format!("embedding failed for RAG chunk {}", chunk.id))?;
        store_rag_chunk_embedding(conn, &chunk.id, &endpoint_key, model, &hash, &embedding)?;
        rag_chunks_indexed += 1;
    }
    Ok(EmbeddingIndexReport {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        indexed: 0,
        skipped: 0,
        rag_chunks_indexed,
        rag_chunks_skipped,
    })
}

pub(crate) fn semantic_search(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<EmbeddingRow>> {
    semantic_search_with_filters(
        conn,
        SemanticSearchOptions {
            provider,
            endpoint,
            model,
            query,
            limit,
            types: &[],
            statuses: &[],
            scope: None,
        },
    )
}

pub(crate) fn semantic_search_with_backend(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    query: &str,
    limit: usize,
    backend: VectorBackend,
) -> Result<Vec<EmbeddingRow>> {
    semantic_search_with_filters_and_backend(
        conn,
        SemanticSearchOptions {
            provider,
            endpoint,
            model,
            query,
            limit,
            types: &[],
            statuses: &[],
            scope: None,
        },
        backend,
        false,
    )
}

pub(crate) struct SemanticSearchOptions<'a> {
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) query: &'a str,
    pub(crate) limit: usize,
    pub(crate) types: &'a [String],
    pub(crate) statuses: &'a [String],
    pub(crate) scope: Option<&'a str>,
}

fn default_vector_backend() -> VectorBackend {
    if cfg!(feature = "vec") {
        VectorBackend::SqliteVec
    } else {
        VectorBackend::Json
    }
}

pub(crate) fn semantic_search_with_filters(
    conn: &Connection,
    options: SemanticSearchOptions<'_>,
) -> Result<Vec<EmbeddingRow>> {
    semantic_search_with_filters_and_backend(conn, options, default_vector_backend(), true)
}

fn semantic_search_with_filters_and_backend(
    conn: &Connection,
    options: SemanticSearchOptions<'_>,
    backend: VectorBackend,
    allow_fallback: bool,
) -> Result<Vec<EmbeddingRow>> {
    let limit = options.limit.max(1);
    let endpoint_key = embedding_endpoint_key(options.provider, options.endpoint);
    let query_embedding = fetch_embedding(
        options.provider,
        options.endpoint,
        options.model,
        options.query,
    )?;
    let mut top_candidates = semantic_memory_candidates(
        conn,
        &options,
        &endpoint_key,
        &query_embedding,
        limit,
        backend,
        allow_fallback,
    )?;
    top_candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut scored = Vec::with_capacity(top_candidates.len());
    for candidate in top_candidates {
        let memory = get_memory_with_links(conn, &candidate.memory_id)?;
        scored.push(EmbeddingRow {
            memory,
            score: candidate.score,
        });
    }
    Ok(scored)
}

fn semantic_memory_candidates(
    conn: &Connection,
    options: &SemanticSearchOptions<'_>,
    endpoint_key: &str,
    query_embedding: &[f32],
    limit: usize,
    backend: VectorBackend,
    allow_fallback: bool,
) -> Result<Vec<ScoredEmbeddingCandidate>> {
    #[cfg(not(feature = "vec"))]
    let _ = allow_fallback;
    match backend {
        VectorBackend::Json => {
            semantic_memory_candidates_json(conn, options, endpoint_key, query_embedding, limit)
        }
        VectorBackend::SqliteVec => {
            #[cfg(feature = "vec")]
            {
                let native = sqlite_vec_memory_search(
                    conn,
                    SqliteVecMemorySearchOptions {
                        endpoint: endpoint_key,
                        model: options.model,
                        query_embedding,
                        limit,
                        types: options.types,
                        statuses: options.statuses,
                        scope: options.scope,
                    },
                );
                match native {
                    Ok(rows) => Ok(rows
                        .into_iter()
                        .map(|(memory_id, score)| ScoredEmbeddingCandidate { memory_id, score })
                        .collect()),
                    Err(_) if allow_fallback => semantic_memory_candidates_json(
                        conn,
                        options,
                        endpoint_key,
                        query_embedding,
                        limit,
                    ),
                    Err(error) => Err(error),
                }
            }
            #[cfg(not(feature = "vec"))]
            bail!("sqlite-vec search requires a binary built with --features vec");
        }
    }
}

fn semantic_memory_candidates_json(
    conn: &Connection,
    options: &SemanticSearchOptions<'_>,
    endpoint_key: &str,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<ScoredEmbeddingCandidate>> {
    let mut sql = String::from(
        r#"
        SELECT e.memory_id, e.embedding
        FROM memory_embeddings e
        JOIN memories m ON m.id = e.memory_id
        WHERE e.endpoint = ? AND e.model = ?
        "#,
    );
    let mut values = vec![endpoint_key.to_string(), options.model.to_string()];
    if !options.types.is_empty() {
        sql.push_str(&format!(
            " AND m.type IN ({})",
            placeholders(options.types.len())
        ));
        values.extend(options.types.iter().cloned());
    }
    if !options.statuses.is_empty() {
        sql.push_str(&format!(
            " AND m.status IN ({})",
            placeholders(options.statuses.len())
        ));
        values.extend(options.statuses.iter().cloned());
    }
    if let Some(scope) = options.scope {
        sql.push_str(" AND m.scope = ?");
        values.push(scope.to_string());
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut top_candidates = Vec::with_capacity(limit);
    for row in rows {
        let (memory_id, raw_embedding) = row?;
        let embedding: Vec<f32> = serde_json::from_str(&raw_embedding)?;
        let score = cosine_similarity(query_embedding, &embedding);
        push_top_embedding_candidate(
            &mut top_candidates,
            ScoredEmbeddingCandidate { memory_id, score },
            limit,
        );
    }
    Ok(top_candidates)
}

pub(crate) fn semantic_rag_chunk_search(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    query: &str,
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<SemanticRagChunkRow>> {
    let limit = limit.max(1);
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let query_embedding = fetch_embedding(provider, endpoint, model, query)?;
    let mut top_candidates =
        semantic_rag_candidates(conn, &endpoint_key, model, &query_embedding, scope, limit)?;
    top_candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.chunk_index.cmp(&b.chunk_index))
    });
    top_candidates.truncate(limit);
    Ok(top_candidates)
}

#[cfg(feature = "vec")]
fn semantic_rag_candidates(
    conn: &Connection,
    endpoint_key: &str,
    model: &str,
    query_embedding: &[f32],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<SemanticRagChunkRow>> {
    let rows = match sqlite_vec_rag_search(conn, endpoint_key, model, query_embedding, scope, limit)
    {
        Ok(rows) => rows,
        Err(_) => {
            return semantic_rag_candidates_json(
                conn,
                endpoint_key,
                model,
                query_embedding,
                scope,
                limit,
            );
        }
    };
    let mut candidates = Vec::with_capacity(limit);
    for row in rows {
        let current_hash = content_hash(&rag_chunk_embedding_content(
            &row.path,
            &row.scope,
            row.start_line,
            row.end_line,
            &row.content,
        ));
        if row.content_hash != current_hash {
            continue;
        }
        candidates.push(SemanticRagChunkRow {
            id: row.id,
            path: row.path,
            scope: row.scope,
            chunk_index: row.chunk_index,
            start_line: row.start_line,
            end_line: row.end_line,
            content: row.content,
            score: row.score,
        });
        if candidates.len() == limit {
            break;
        }
    }
    Ok(candidates)
}

#[cfg(not(feature = "vec"))]
fn semantic_rag_candidates(
    conn: &Connection,
    endpoint_key: &str,
    model: &str,
    query_embedding: &[f32],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<SemanticRagChunkRow>> {
    semantic_rag_candidates_json(conn, endpoint_key, model, query_embedding, scope, limit)
}

fn semantic_rag_candidates_json(
    conn: &Connection,
    endpoint_key: &str,
    model: &str,
    query_embedding: &[f32],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<SemanticRagChunkRow>> {
    let mut sql = String::from(
        r#"
        SELECT c.id, c.path, c.scope, c.chunk_index, c.start_line, c.end_line,
               c.content, e.embedding, e.content_hash
        FROM rag_chunk_embeddings e
        JOIN rag_chunks c ON c.id = e.chunk_id
        WHERE e.endpoint = ? AND e.model = ?
        "#,
    );
    let mut values = vec![endpoint_key.to_string(), model.to_string()];
    if let Some(scope) = scope {
        sql.push_str(" AND c.scope = ?");
        values.push(scope.to_string());
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        let chunk_index = row.get::<_, i64>(3)?.max(0) as usize;
        let start_line = row.get::<_, i64>(4)?.max(1) as usize;
        let end_line = row.get::<_, i64>(5)?.max(start_line as i64) as usize;
        Ok((
            SemanticRagChunkRow {
                id: row.get::<_, String>(0)?,
                path: row.get::<_, String>(1)?,
                scope: row.get::<_, String>(2)?,
                chunk_index,
                start_line,
                end_line,
                content: row.get::<_, String>(6)?,
                score: 0.0,
            },
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    let mut top_candidates = Vec::with_capacity(limit);
    for row in rows {
        let (mut chunk, raw_embedding, stored_hash) = row?;
        let current_hash = content_hash(&rag_chunk_embedding_content(
            &chunk.path,
            &chunk.scope,
            chunk.start_line,
            chunk.end_line,
            &chunk.content,
        ));
        if stored_hash != current_hash {
            continue;
        }
        let embedding: Vec<f32> = serde_json::from_str(&raw_embedding)?;
        chunk.score = cosine_similarity(query_embedding, &embedding);
        push_top_rag_chunk_candidate(&mut top_candidates, chunk, limit);
    }
    Ok(top_candidates)
}

struct ScoredEmbeddingCandidate {
    memory_id: String,
    score: f64,
}

fn push_top_embedding_candidate(
    candidates: &mut Vec<ScoredEmbeddingCandidate>,
    candidate: ScoredEmbeddingCandidate,
    limit: usize,
) {
    if candidates.len() < limit {
        candidates.push(candidate);
        return;
    }
    let Some((min_index, min_score)) = candidates
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, item)| (index, item.score))
    else {
        return;
    };
    if candidate.score > min_score {
        candidates[min_index] = candidate;
    }
}

fn push_top_rag_chunk_candidate(
    candidates: &mut Vec<SemanticRagChunkRow>,
    candidate: SemanticRagChunkRow,
    limit: usize,
) {
    if candidates.len() < limit {
        candidates.push(candidate);
        return;
    }
    let Some((min_index, min_score)) = candidates
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, item)| (index, item.score))
    else {
        return;
    };
    if candidate.score > min_score {
        candidates[min_index] = candidate;
    }
}

pub(crate) fn semantic_index_ready(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<bool> {
    Ok(semantic_readiness(conn, provider, endpoint, model)?.ready)
}

pub(crate) struct SemanticReadiness {
    pub(crate) ready: bool,
    pub(crate) reason: Option<String>,
}

pub(crate) fn semantic_readiness(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<SemanticReadiness> {
    let report = embed_status(conn, provider, endpoint, model)?;
    if report.indexed <= report.stale {
        return Ok(SemanticReadiness {
            ready: false,
            reason: Some("semantic index not ready; using FTS/local ranking".to_string()),
        });
    }
    if !report.provider_reachable {
        let detail = report
            .provider_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default();
        return Ok(SemanticReadiness {
            ready: false,
            reason: Some(format!(
                "embedding provider is not reachable; using FTS/local ranking{detail}"
            )),
        });
    }
    Ok(SemanticReadiness {
        ready: true,
        reason: None,
    })
}

fn embedding_endpoint_key(provider: &str, endpoint: &str) -> String {
    format!("{}:{}", provider.trim().to_lowercase(), endpoint.trim())
}

fn embedding_content(memory: &Memory) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        memory.memory_type, memory.scope, memory.title, memory.body, memory.status
    )
}

fn rag_chunk_embedding_content(
    path: &str,
    scope: &str,
    start_line: usize,
    end_line: usize,
    content: &str,
) -> String {
    format!(
        "source_chunk\n{scope}\n{path}:{}-{}\n{content}",
        start_line, end_line
    )
}

fn query_rag_chunk_embedding_targets(
    conn: &Connection,
    limit: Option<usize>,
) -> Result<Vec<RagChunkEmbeddingTarget>> {
    query_rag_chunk_embedding_targets_filtered(conn, None, None, limit)
}

fn query_rag_chunk_embedding_targets_for_paths(
    conn: &Connection,
    scope: &str,
    paths: &[String],
) -> Result<Vec<RagChunkEmbeddingTarget>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    query_rag_chunk_embedding_targets_filtered(conn, Some(scope), Some(paths), None)
}

fn query_rag_chunk_embedding_targets_filtered(
    conn: &Connection,
    scope: Option<&str>,
    paths: Option<&[String]>,
    limit: Option<usize>,
) -> Result<Vec<RagChunkEmbeddingTarget>> {
    let mut sql = String::from(
        r#"
        SELECT id, source_id, path, scope, start_line, end_line, content
        FROM rag_chunks
        "#,
    );
    let mut values = Vec::new();
    let mut clauses = Vec::new();
    if let Some(scope) = scope {
        clauses.push("scope = ?".to_string());
        values.push(scope.to_string());
    }
    if let Some(paths) = paths
        && !paths.is_empty()
    {
        clauses.push(format!("path IN ({})", placeholders(paths.len())));
        values.extend(paths.iter().cloned());
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY updated_at DESC, path ASC, chunk_index ASC");
    if limit.is_some() {
        sql.push_str(" LIMIT ?");
    }
    let mut stmt = conn.prepare(&sql)?;
    let map_row = |row: &Row<'_>| {
        let start_line = row.get::<_, i64>(4)?.max(1) as usize;
        let end_line = row.get::<_, i64>(5)?.max(start_line as i64) as usize;
        Ok(RagChunkEmbeddingTarget {
            id: row.get::<_, String>(0)?,
            source_id: row.get::<_, i64>(1)?,
            path: row.get::<_, String>(2)?,
            scope: row.get::<_, String>(3)?,
            start_line,
            end_line,
            content: row.get::<_, String>(6)?,
        })
    };
    if let Some(limit) = limit {
        values.push(limit.min(i64::MAX as usize).to_string());
    }
    stmt.query_map(rusqlite::params_from_iter(values), map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(crate) fn rag_chunk_embedding_freshness_by_source(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<HashMap<i64, RagChunkEmbeddingFreshness>> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let mut by_source = HashMap::<i64, RagChunkEmbeddingFreshness>::new();
    for chunk in query_rag_chunk_embedding_targets(conn, None)? {
        let freshness = by_source.entry(chunk.source_id).or_default();
        freshness.eligible += 1;
        let content = rag_chunk_embedding_content(
            &chunk.path,
            &chunk.scope,
            chunk.start_line,
            chunk.end_line,
            &chunk.content,
        );
        let hash = content_hash(&content);
        let existing: Option<String> = conn
            .query_row(
                r#"
                SELECT content_hash FROM rag_chunk_embeddings
                WHERE chunk_id = ?1 AND endpoint = ?2 AND model = ?3
                "#,
                params![chunk.id, endpoint_key, model],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(existing_hash) if existing_hash == hash => freshness.indexed += 1,
            Some(_) => {
                freshness.indexed += 1;
                freshness.stale += 1;
            }
            None => freshness.missing += 1,
        }
    }
    Ok(by_source)
}

pub(crate) fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn embedding_is_current(
    conn: &Connection,
    memory_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
) -> Result<bool> {
    let existing: Option<String> = conn
        .query_row(
            r#"
            SELECT content_hash FROM memory_embeddings
            WHERE memory_id = ?1 AND endpoint = ?2 AND model = ?3
            "#,
            params![memory_id, endpoint, model],
            |row| row.get(0),
        )
        .optional()?;
    Ok(existing.as_deref() == Some(hash))
}

fn rag_chunk_embedding_is_current(
    conn: &Connection,
    chunk_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
) -> Result<bool> {
    let existing: Option<String> = conn
        .query_row(
            r#"
            SELECT content_hash FROM rag_chunk_embeddings
            WHERE chunk_id = ?1 AND endpoint = ?2 AND model = ?3
            "#,
            params![chunk_id, endpoint, model],
            |row| row.get(0),
        )
        .optional()?;
    Ok(existing.as_deref() == Some(hash))
}

fn store_embedding(
    conn: &Connection,
    memory_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
    embedding: &[f32],
) -> Result<()> {
    ensure_sqlite_vec_memory_index(conn, embedding.len())?;
    conn.execute(
        r#"
        INSERT INTO memory_embeddings (
            memory_id, model, endpoint, dimensions, embedding, content_hash, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(memory_id, model, endpoint) DO UPDATE SET
            dimensions = excluded.dimensions,
            embedding = excluded.embedding,
            content_hash = excluded.content_hash,
            updated_at = excluded.updated_at
        "#,
        params![
            memory_id,
            model,
            endpoint,
            embedding.len() as i64,
            serde_json::to_string(embedding)?,
            hash,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn store_rag_chunk_embedding(
    conn: &Connection,
    chunk_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
    embedding: &[f32],
) -> Result<()> {
    ensure_sqlite_vec_rag_index(conn, embedding.len())?;
    conn.execute(
        r#"
        INSERT INTO rag_chunk_embeddings (
            chunk_id, model, endpoint, dimensions, embedding, content_hash, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(chunk_id, model, endpoint) DO UPDATE SET
            dimensions = excluded.dimensions,
            embedding = excluded.embedding,
            content_hash = excluded.content_hash,
            updated_at = excluded.updated_at
        "#,
        params![
            chunk_id,
            model,
            endpoint,
            embedding.len() as i64,
            serde_json::to_string(embedding)?,
            hash,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn fetch_ollama_embedding(endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let url = format!("{}/api/embeddings", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let response = client
        .post(url)
        .json(&OllamaEmbeddingRequest {
            model,
            prompt: text,
        })
        .send()?
        .error_for_status()?
        .json::<OllamaEmbeddingResponse>()?;
    if response.embedding.is_empty() {
        bail!("embedding response was empty");
    }
    Ok(response.embedding)
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

fn fetch_openai_embedding(endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let url = format!("{}/v1/embeddings", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let mut request = client
        .post(url)
        .json(&OpenAiEmbeddingRequest { model, input: text });
    if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()?
        .error_for_status()?
        .json::<OpenAiEmbeddingResponse>()?;
    let embedding = response
        .data
        .into_iter()
        .next()
        .map(|item| item.embedding)
        .unwrap_or_default();
    if embedding.is_empty() {
        bail!("embedding response was empty");
    }
    Ok(embedding)
}

fn fetch_mock_embedding(model: &str, text: &str) -> Vec<f32> {
    let dims = if model.contains("small") { 64 } else { 128 };
    let mut values = vec![0.0f32; dims];
    for token in tokenize(text) {
        let mut hasher = Sha256::new();
        hasher.update(model.as_bytes());
        hasher.update(token.as_bytes());
        let hash = hasher.finalize();
        for (i, byte) in hash.iter().enumerate() {
            let idx = ((i * 31) + (*byte as usize)) % dims;
            values[idx] += ((*byte as f32) / 127.5) - 1.0;
        }
    }
    if values.iter().all(|value| *value == 0.0) {
        values[0] = 1.0;
    }
    values
}

fn fetch_embedding(provider: &str, endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    match provider.trim().to_lowercase().as_str() {
        "local" => crate::app::local_embed::embed_local(text),
        "ollama" => fetch_ollama_embedding(endpoint, model, text),
        "openai" | "openai-compatible" | "openai_compatible" => {
            fetch_openai_embedding(endpoint, model, text)
        }
        "mock" => Ok(fetch_mock_embedding(model, text)),
        other => bail!("unsupported embedding provider: {other}"),
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..len {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

pub(crate) fn print_provider_models(provider: &str, endpoint: &str, json_out: bool) -> Result<()> {
    let models = provider_models(provider, endpoint)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&models)?);
    } else if models.is_empty() {
        println!("models: none");
    } else {
        for model in models {
            println!("{}", model.name);
        }
    }
    Ok(())
}

fn provider_models(provider: &str, endpoint: &str) -> Result<Vec<ProviderModel>> {
    match provider.trim().to_lowercase().as_str() {
        "local" => Ok(vec![ProviderModel {
            name: DEFAULT_EMBED_MODEL.to_string(),
            details: Some(json!({
                "endpoint": endpoint,
                "repo": "Xenova/paraphrase-multilingual-MiniLM-L12-v2",
                "runtime": "tract-onnx"
            })),
        }]),
        "mock" => Ok(vec![ProviderModel {
            name: "mock-embedding".to_string(),
            details: None,
        }]),
        "ollama" => {
            let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
            let value: Value = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?
                .get(url)
                .send()?
                .error_for_status()?
                .json()?;
            let models = value
                .get("models")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(|name| ProviderModel {
                            name: name.to_string(),
                            details: Some(item.clone()),
                        })
                })
                .collect();
            Ok(models)
        }
        "openai" | "openai-compatible" | "openai_compatible" => {
            let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
            let mut request = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?
                .get(url);
            if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
                && !key.trim().is_empty()
            {
                request = request.bearer_auth(key);
            }
            let value: Value = request.send()?.error_for_status()?.json()?;
            let models = value
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(|name| ProviderModel {
                            name: name.to_string(),
                            details: Some(item.clone()),
                        })
                })
                .collect();
            Ok(models)
        }
        other => bail!("unsupported embedding provider: {other}"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VectorBenchTiming {
    pub(crate) total_ms: f64,
    pub(crate) mean_ms: f64,
    pub(crate) p50_ms: f64,
    pub(crate) p95_ms: f64,
    pub(crate) p99_ms: f64,
    pub(crate) queries_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VectorBenchReport {
    pub(crate) version: u8,
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) vectors: usize,
    pub(crate) dimensions: usize,
    pub(crate) iterations: usize,
    pub(crate) warmup: usize,
    pub(crate) best_score: Option<f64>,
    pub(crate) json: Option<VectorBenchTiming>,
    pub(crate) sqlite_vec: Option<VectorBenchTiming>,
    pub(crate) top_match_equal: Option<bool>,
    pub(crate) speedup: Option<f64>,
    pub(crate) message: Option<String>,
    pub(crate) baseline_path: Option<String>,
    pub(crate) baseline_written: bool,
    pub(crate) regression: Option<VectorBenchRegression>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VectorBenchRegression {
    pub(crate) max_allowed_percent: f64,
    pub(crate) p95_percent: f64,
    pub(crate) qps_percent: f64,
    pub(crate) ok: bool,
}

pub(crate) struct VectorBenchOptions<'a> {
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) iterations: usize,
    pub(crate) warmup: usize,
    pub(crate) limit: Option<usize>,
    pub(crate) baseline: Option<&'a Path>,
    pub(crate) write_baseline: bool,
    pub(crate) max_regression_percent: f64,
    pub(crate) json_out: bool,
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = percentile.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let weight = rank - lower as f64;
        sorted[lower] + (sorted[upper] - sorted[lower]) * weight
    }
}

fn benchmark_queries<T>(
    iterations: usize,
    warmup: usize,
    mut query: impl FnMut() -> Result<T>,
) -> Result<(T, VectorBenchTiming)> {
    for _ in 0..warmup {
        let _ = query()?;
    }
    let mut samples = Vec::with_capacity(iterations);
    let mut last = None;
    for _ in 0..iterations {
        let started = std::time::Instant::now();
        last = Some(query()?);
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let total_ms = samples.iter().sum::<f64>();
    let mean_ms = total_ms / iterations as f64;
    samples.sort_by(f64::total_cmp);
    Ok((
        last.expect("iterations are validated as non-zero"),
        VectorBenchTiming {
            total_ms,
            mean_ms,
            p50_ms: percentile(&samples, 0.50),
            p95_ms: percentile(&samples, 0.95),
            p99_ms: percentile(&samples, 0.99),
            queries_per_second: if mean_ms > 0.0 { 1000.0 / mean_ms } else { 0.0 },
        },
    ))
}

#[cfg(feature = "vec")]
fn benchmark_sqlite_vec_queries(
    conn: &Connection,
    embeddings: &[(String, Vec<f32>)],
    query: &[f32],
    iterations: usize,
    warmup: usize,
) -> Result<((String, f64), VectorBenchTiming)> {
    let table = "dukememory_vector_bench_vec";
    let result = (|| -> Result<((String, f64), VectorBenchTiming)> {
        conn.execute_batch(&format!(
            r#"
            DROP TABLE IF EXISTS temp.{table};
            CREATE VIRTUAL TABLE temp.{table} USING vec0(
                embedding float[{}] distance_metric=cosine
            );
            "#,
            query.len()
        ))?;
        {
            let mut insert = conn.prepare(&format!(
                "INSERT INTO {table}(rowid, embedding) VALUES (?1, ?2)"
            ))?;
            for (index, (_, embedding)) in embeddings.iter().enumerate() {
                insert.execute(params![
                    i64::try_from(index + 1)?,
                    serde_json::to_string(embedding)?
                ])?;
            }
        }
        let query_json = serde_json::to_string(query)?;
        benchmark_queries(iterations, warmup, || {
            let (rowid, distance) = conn.query_row(
                &format!("SELECT rowid, distance FROM {table} WHERE embedding MATCH ?1 AND k = 1"),
                [&query_json],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)),
            )?;
            let index = usize::try_from(rowid.saturating_sub(1))?;
            let memory_id = embeddings
                .get(index)
                .map(|(memory_id, _)| memory_id.clone())
                .ok_or_else(|| anyhow::anyhow!("sqlite-vec benchmark returned invalid rowid"))?;
            Ok((memory_id, 1.0 - distance))
        })
    })();
    let _ = conn.execute_batch(&format!("DROP TABLE IF EXISTS temp.{table};"));
    result
}

pub(crate) fn print_vector_bench(conn: &Connection, options: VectorBenchOptions<'_>) -> Result<()> {
    let VectorBenchOptions {
        provider,
        endpoint,
        model,
        iterations,
        warmup,
        limit,
        baseline,
        write_baseline,
        max_regression_percent,
        json_out,
    } = options;
    if iterations == 0 || iterations > 10_000 {
        bail!("vector-bench --iterations must be between 1 and 10000");
    }
    if warmup > 10_000 {
        bail!("vector-bench --warmup must not exceed 10000");
    }
    if limit == Some(0) {
        bail!("vector-bench --limit must be greater than zero");
    }
    if !max_regression_percent.is_finite() || max_regression_percent < 0.0 {
        bail!("vector-bench --max-regression-percent must be a finite non-negative number");
    }
    if write_baseline && baseline.is_none() {
        bail!("vector-bench --write-baseline requires --baseline PATH");
    }
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let mut stmt = conn.prepare(
        r#"
        SELECT memory_id, embedding
        FROM memory_embeddings
        WHERE endpoint = ?1 AND model = ?2
        ORDER BY memory_id
        "#,
    )?;
    let mut embeddings = stmt
        .query_map(params![endpoint_key, model], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, raw)| {
            serde_json::from_str::<Vec<f32>>(&raw)
                .map(|embedding| (id, embedding))
                .map_err(Into::into)
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(limit) = limit {
        embeddings.truncate(limit);
    }
    if embeddings.is_empty() {
        let report = VectorBenchReport {
            version: 3,
            provider: provider.to_string(),
            endpoint: endpoint_key,
            model: model.to_string(),
            vectors: 0,
            dimensions: 0,
            iterations,
            warmup,
            best_score: None,
            json: None,
            sqlite_vec: None,
            top_match_equal: None,
            speedup: None,
            message: Some("no indexed embeddings".to_string()),
            baseline_path: baseline.map(|path| path.display().to_string()),
            baseline_written: false,
            regression: None,
        };
        if json_out {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("vectors: 0");
            println!("bench: no indexed embeddings");
        }
        return Ok(());
    }
    let query = embeddings[0].1.clone();
    let (fallback_best, json_timing) = benchmark_queries(iterations, warmup, || {
        let mut best = (String::new(), f64::NEG_INFINITY);
        for (memory_id, embedding) in &embeddings {
            let score = cosine_similarity(&query, embedding);
            if score > best.1 {
                best = (memory_id.clone(), score);
            }
        }
        Ok(best)
    })?;
    #[allow(unused_mut)]
    let mut report = VectorBenchReport {
        version: 3,
        provider: provider.to_string(),
        endpoint: endpoint_key.clone(),
        model: model.to_string(),
        vectors: embeddings.len(),
        dimensions: query.len(),
        iterations,
        warmup,
        best_score: Some(fallback_best.1),
        json: Some(json_timing),
        sqlite_vec: None,
        top_match_equal: None,
        speedup: None,
        message: None,
        baseline_path: baseline.map(|path| path.display().to_string()),
        baseline_written: false,
        regression: None,
    };
    #[cfg(feature = "vec")]
    {
        let (native_best, native_timing) =
            benchmark_sqlite_vec_queries(conn, &embeddings, &query, iterations, warmup)?;
        let top_match_equal = native_best.0 == fallback_best.0;
        report.top_match_equal = Some(top_match_equal);
        if native_timing.mean_ms > 0.0 {
            report.speedup = report
                .json
                .as_ref()
                .map(|timing| timing.mean_ms / native_timing.mean_ms);
        }
        report.sqlite_vec = Some(native_timing);
    }
    if let Some(path) = baseline {
        if write_baseline {
            report.baseline_written = true;
            let encoded = serde_json::to_vec_pretty(&report)?;
            write_file(path, &encoded)?;
        } else {
            let raw = fs::read_to_string(path).with_context(|| {
                format!(
                    "failed to read vector benchmark baseline {}",
                    path.display()
                )
            })?;
            let previous: VectorBenchReport = serde_json::from_str(&raw).with_context(|| {
                format!(
                    "failed to parse vector benchmark baseline {}",
                    path.display()
                )
            })?;
            report.regression = Some(vector_bench_regression(
                &report,
                &previous,
                max_regression_percent,
            )?);
        }
    }
    let regression_failed = report.regression.as_ref().is_some_and(|gate| !gate.ok);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        if regression_failed {
            bail!("vector benchmark regression gate failed");
        }
        return Ok(());
    }
    println!("vectors: {}", report.vectors);
    println!("dimensions: {}", report.dimensions);
    println!("iterations: {}", report.iterations);
    println!("warmup: {}", report.warmup);
    println!("best_score: {:.4}", report.best_score.unwrap_or_default());
    if let Some(timing) = &report.json {
        println!("json_elapsed_ms: {:.3}", timing.total_ms);
        println!("json_mean_ms: {:.3}", timing.mean_ms);
        println!("json_p50_ms: {:.3}", timing.p50_ms);
        println!("json_p95_ms: {:.3}", timing.p95_ms);
        println!("json_p99_ms: {:.3}", timing.p99_ms);
        println!("json_qps: {:.1}", timing.queries_per_second);
    }
    if let Some(timing) = &report.sqlite_vec {
        println!("sqlite_vec_elapsed_ms: {:.3}", timing.total_ms);
        println!("sqlite_vec_mean_ms: {:.3}", timing.mean_ms);
        println!("sqlite_vec_p50_ms: {:.3}", timing.p50_ms);
        println!("sqlite_vec_p95_ms: {:.3}", timing.p95_ms);
        println!("sqlite_vec_p99_ms: {:.3}", timing.p99_ms);
        println!("sqlite_vec_qps: {:.1}", timing.queries_per_second);
        println!(
            "top_match_equal: {}",
            report.top_match_equal.unwrap_or(false)
        );
        println!("speedup: {:.3}", report.speedup.unwrap_or_default());
    } else {
        println!("sqlite_vec_elapsed_ms: unavailable (build with --features vec)");
    }
    if let Some(regression) = &report.regression {
        println!("regression_p95_percent: {:.2}", regression.p95_percent);
        println!("regression_qps_percent: {:.2}", regression.qps_percent);
        println!("regression_ok: {}", regression.ok);
    }
    if report.baseline_written {
        println!("baseline_written: true");
    }
    if regression_failed {
        bail!("vector benchmark regression gate failed");
    }
    Ok(())
}

fn vector_bench_regression(
    current: &VectorBenchReport,
    previous: &VectorBenchReport,
    max_allowed_percent: f64,
) -> Result<VectorBenchRegression> {
    if current.vectors != previous.vectors || current.dimensions != previous.dimensions {
        bail!(
            "vector benchmark baseline scale mismatch: current={}/{} baseline={}/{}",
            current.vectors,
            current.dimensions,
            previous.vectors,
            previous.dimensions,
        );
    }
    let current_timing = current
        .sqlite_vec
        .as_ref()
        .or(current.json.as_ref())
        .ok_or_else(|| anyhow::anyhow!("current vector benchmark has no timing"))?;
    let previous_timing = previous
        .sqlite_vec
        .as_ref()
        .or(previous.json.as_ref())
        .ok_or_else(|| anyhow::anyhow!("baseline vector benchmark has no timing"))?;
    let p95_percent = percent_increase(current_timing.p95_ms, previous_timing.p95_ms);
    let qps_percent = percent_decrease(
        current_timing.queries_per_second,
        previous_timing.queries_per_second,
    );
    Ok(VectorBenchRegression {
        max_allowed_percent,
        p95_percent,
        qps_percent,
        ok: p95_percent <= max_allowed_percent && qps_percent <= max_allowed_percent,
    })
}

fn percent_increase(current: f64, previous: f64) -> f64 {
    if previous <= f64::EPSILON {
        return 0.0;
    }
    ((current - previous) / previous * 100.0).max(0.0)
}

fn percent_decrease(current: f64, previous: f64) -> f64 {
    if previous <= f64::EPSILON {
        return 0.0;
    }
    ((previous - current) / previous * 100.0).max(0.0)
}

#[derive(Debug, Serialize)]
pub(crate) struct EmbedStatusReport {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) eligible: usize,
    pub(crate) indexed: usize,
    pub(crate) stale: usize,
    pub(crate) missing: usize,
    pub(crate) rag_chunks_eligible: usize,
    pub(crate) rag_chunks_indexed: usize,
    pub(crate) rag_chunks_stale: usize,
    pub(crate) rag_chunks_missing: usize,
    pub(crate) provider_reachable: bool,
    pub(crate) provider_health_ms: Option<u128>,
    pub(crate) provider_error: Option<String>,
}

pub(crate) fn print_embed_status(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = embed_status(conn, provider, endpoint, model)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("provider: {}", report.provider);
        println!("endpoint: {}", report.endpoint);
        println!("model: {}", report.model);
        println!("eligible: {}", report.eligible);
        println!("indexed: {}", report.indexed);
        println!("missing: {}", report.missing);
        println!("stale: {}", report.stale);
        println!("rag_chunks_eligible: {}", report.rag_chunks_eligible);
        println!("rag_chunks_indexed: {}", report.rag_chunks_indexed);
        println!("rag_chunks_missing: {}", report.rag_chunks_missing);
        println!("rag_chunks_stale: {}", report.rag_chunks_stale);
        println!("provider_reachable: {}", report.provider_reachable);
        println!(
            "provider_health_ms: {}",
            report
                .provider_health_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        if let Some(error) = &report.provider_error {
            println!("provider_error: {error}");
        }
    }
    Ok(())
}

pub(crate) fn embed_status(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<EmbedStatusReport> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let mut indexed = 0;
    let mut stale = 0;
    let mut missing = 0;
    for memory in &rows {
        let content = embedding_content(memory);
        let hash = content_hash(&content);
        let existing: Option<String> = conn
            .query_row(
                r#"
                SELECT content_hash FROM memory_embeddings
                WHERE memory_id = ?1 AND endpoint = ?2 AND model = ?3
                "#,
                params![memory.id, endpoint_key, model],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(existing_hash) if existing_hash == hash => indexed += 1,
            Some(_) => {
                indexed += 1;
                stale += 1;
            }
            None => missing += 1,
        }
    }
    let chunk_rows = query_rag_chunk_embedding_targets(conn, None)?;
    let mut rag_chunks_indexed = 0;
    let mut rag_chunks_stale = 0;
    let mut rag_chunks_missing = 0;
    for chunk in &chunk_rows {
        let content = rag_chunk_embedding_content(
            &chunk.path,
            &chunk.scope,
            chunk.start_line,
            chunk.end_line,
            &chunk.content,
        );
        let hash = content_hash(&content);
        let existing: Option<String> = conn
            .query_row(
                r#"
                SELECT content_hash FROM rag_chunk_embeddings
                WHERE chunk_id = ?1 AND endpoint = ?2 AND model = ?3
                "#,
                params![chunk.id, endpoint_key, model],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(existing_hash) if existing_hash == hash => rag_chunks_indexed += 1,
            Some(_) => {
                rag_chunks_indexed += 1;
                rag_chunks_stale += 1;
            }
            None => rag_chunks_missing += 1,
        }
    }
    let provider_health = embedding_provider_health(conn, provider, endpoint);
    Ok(EmbedStatusReport {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        eligible: rows.len(),
        indexed,
        stale,
        missing,
        rag_chunks_eligible: chunk_rows.len(),
        rag_chunks_indexed,
        rag_chunks_stale,
        rag_chunks_missing,
        provider_reachable: provider_health.reachable,
        provider_health_ms: provider_health.elapsed_ms,
        provider_error: provider_health.error,
    })
}

struct EmbeddingProviderHealth {
    reachable: bool,
    elapsed_ms: Option<u128>,
    error: Option<String>,
}

fn embedding_provider_health(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
) -> EmbeddingProviderHealth {
    let provider_key = provider.trim().to_lowercase();
    let endpoint_key = endpoint.trim().trim_end_matches('/').to_string();
    if matches!(provider_key.as_str(), "local" | "mock") {
        return EmbeddingProviderHealth {
            reachable: true,
            elapsed_ms: Some(0),
            error: None,
        };
    }
    if let Some(cached) = cached_embedding_provider_health(conn, &provider_key, &endpoint_key) {
        return cached;
    }
    let started = std::time::Instant::now();
    let result = match provider_key.as_str() {
        "ollama" => {
            let url = format!("{endpoint_key}/api/tags");
            provider_health_client(&endpoint_key)
                .build()
                .and_then(|client| client.get(url).send())
                .and_then(|response| response.error_for_status().map(|_| ()))
                .map_err(Into::into)
        }
        "openai" | "openai-compatible" | "openai_compatible" => {
            let url = format!("{endpoint_key}/v1/models");
            let client = match provider_health_client(&endpoint_key).build() {
                Ok(client) => client,
                Err(error) => {
                    let health = provider_health_error(started, error.into());
                    store_embedding_provider_health(conn, &provider_key, &endpoint_key, &health);
                    return health;
                }
            };
            let mut request = client.get(url);
            if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
                && !key.trim().is_empty()
            {
                request = request.bearer_auth(key);
            }
            request
                .send()
                .and_then(|response| response.error_for_status().map(|_| ()))
                .map_err(Into::into)
        }
        other => Err(anyhow::anyhow!("unsupported embedding provider: {other}")),
    };
    let health = match result {
        Ok(()) => EmbeddingProviderHealth {
            reachable: true,
            elapsed_ms: Some(started.elapsed().as_millis()),
            error: None,
        },
        Err(error) => provider_health_error(started, error),
    };
    store_embedding_provider_health(conn, &provider_key, &endpoint_key, &health);
    health
}

fn provider_health_client(endpoint: &str) -> reqwest::blocking::ClientBuilder {
    let builder = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(PROVIDER_HEALTH_TIMEOUT_MS));
    if endpoint_is_loopback(endpoint) {
        builder.no_proxy()
    } else {
        builder
    }
}

fn endpoint_is_loopback(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn cached_embedding_provider_health(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
) -> Option<EmbeddingProviderHealth> {
    let now = now_ms();
    let row = conn
        .query_row(
            r#"
            SELECT reachable, error, elapsed_ms, checked_at, cooldown_until
            FROM embedding_provider_health
            WHERE provider = ?1 AND endpoint = ?2
            "#,
            params![provider, endpoint],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten()?;
    let (reachable, error, elapsed_ms, checked_at, cooldown_until) = row;
    let fresh = if reachable != 0 {
        now.saturating_sub(checked_at) <= PROVIDER_HEALTH_OK_CACHE_MS
    } else {
        cooldown_until > now
    };
    if !fresh {
        return None;
    }
    Some(EmbeddingProviderHealth {
        reachable: reachable != 0,
        elapsed_ms: elapsed_ms.map(|value| value.max(0) as u128),
        error,
    })
}

fn store_embedding_provider_health(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    health: &EmbeddingProviderHealth,
) {
    let now = now_ms();
    let cooldown_until = if health.reachable {
        now.saturating_add(PROVIDER_HEALTH_OK_CACHE_MS)
    } else {
        now.saturating_add(PROVIDER_HEALTH_DOWN_COOLDOWN_MS)
    };
    let elapsed_ms = health
        .elapsed_ms
        .map(|value| value.min(i64::MAX as u128) as i64);
    let _ = conn.execute(
        r#"
        INSERT OR REPLACE INTO embedding_provider_health (
            provider, endpoint, reachable, error, elapsed_ms, checked_at, cooldown_until
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            provider,
            endpoint,
            if health.reachable { 1 } else { 0 },
            health.error,
            elapsed_ms,
            now,
            cooldown_until,
        ],
    );
}

fn provider_health_error(
    started: std::time::Instant,
    error: anyhow::Error,
) -> EmbeddingProviderHealth {
    EmbeddingProviderHealth {
        reachable: false,
        elapsed_ms: Some(started.elapsed().as_millis()),
        error: Some(truncate_chars(&error.to_string(), 220)),
    }
}

pub(crate) fn embed_watch(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    interval_secs: u64,
    once: bool,
) -> Result<()> {
    loop {
        let report = embed_index(conn, provider, endpoint, model, &[], None, false)?;
        println!(
            "indexed={} skipped={} provider={} model={}",
            report.indexed, report.skipped, report.provider, report.model
        );
        if once {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
    }
}

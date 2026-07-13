use super::embeddings::{SemanticRagChunkRow, content_hash};
use super::*;

const RAG_INGEST_VERSION: u32 = 1;

pub(crate) struct RagIngestRequest<'a> {
    pub(crate) root: &'a Path,
    pub(crate) input: &'a Path,
    pub(crate) scope: &'a str,
    pub(crate) apply: bool,
    pub(crate) chunk_chars: usize,
    pub(crate) overlap_chars: usize,
    pub(crate) max_file_bytes: usize,
    pub(crate) max_files: usize,
    pub(crate) embed: bool,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) json: bool,
}

pub(crate) struct RagSourcesRequest<'a> {
    pub(crate) root: &'a Path,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) json: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagIngestReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) input: String,
    pub(crate) scope: String,
    pub(crate) applied: bool,
    pub(crate) embed_requested: bool,
    pub(crate) embedding_provider: String,
    pub(crate) embedding_endpoint: String,
    pub(crate) embedding_model: String,
    pub(crate) embed_index: Option<EmbeddingIndexReport>,
    pub(crate) embed_error: Option<String>,
    pub(crate) files_scanned: usize,
    pub(crate) files_indexed: usize,
    pub(crate) files_unchanged: usize,
    pub(crate) chunks_indexed: usize,
    pub(crate) chunks_written: usize,
    pub(crate) sources: Vec<RagIngestSource>,
    pub(crate) skipped: Vec<RagIngestSkip>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagSourcesReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) total_sources: usize,
    pub(crate) ready_sources: usize,
    pub(crate) stale_sources: usize,
    pub(crate) missing_sources: usize,
    pub(crate) orphan_sources: usize,
    pub(crate) duplicate_paths: usize,
    pub(crate) total_chunks: usize,
    pub(crate) embedding_provider: String,
    pub(crate) embedding_endpoint: String,
    pub(crate) embedding_model: String,
    pub(crate) embedding_ready_sources: usize,
    pub(crate) chunk_embeddings_indexed: usize,
    pub(crate) chunk_embeddings_missing: usize,
    pub(crate) chunk_embeddings_stale: usize,
    pub(crate) sources: Vec<RagSourceStatus>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RagSourceStatus {
    pub(crate) source_id: i64,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) content_hash: String,
    pub(crate) current_hash: Option<String>,
    pub(crate) chunks: usize,
    pub(crate) suggestions: usize,
    pub(crate) ingested_at: i64,
    pub(crate) missing: bool,
    pub(crate) stale: bool,
    pub(crate) orphan: bool,
    pub(crate) duplicate_path: bool,
    pub(crate) chunk_embeddings_indexed: usize,
    pub(crate) chunk_embeddings_missing: usize,
    pub(crate) chunk_embeddings_stale: usize,
    pub(crate) chunk_embeddings_ready: bool,
    pub(crate) ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RagIngestSource {
    pub(crate) path: String,
    pub(crate) content_hash: String,
    pub(crate) bytes: usize,
    pub(crate) chunks: usize,
    pub(crate) applied: bool,
    pub(crate) unchanged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RagIngestSkip {
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RagChunkHit {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) scope: String,
    pub(crate) chunk_index: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) content: String,
    pub(crate) score: f64,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Clone)]
struct RagChunkDraft {
    index: usize,
    start_line: usize,
    end_line: usize,
    content: String,
}

pub(crate) fn print_rag_ingest(conn: &Connection, request: RagIngestRequest<'_>) -> Result<()> {
    let json_out = request.json;
    let report = rag_ingest_report(conn, request)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("RAG Ingest");
    println!(
        "status: {} files: {}/{} unchanged: {} chunks: {} written: {} applied: {} embed: {}",
        report.status,
        report.files_indexed,
        report.files_scanned,
        report.files_unchanged,
        report.chunks_indexed,
        report.chunks_written,
        report.applied,
        report.embed_requested
    );
    if let Some(embed_index) = &report.embed_index {
        println!(
            "embedding: indexed={} skipped={} rag_chunks_indexed={} rag_chunks_skipped={}",
            embed_index.indexed,
            embed_index.skipped,
            embed_index.rag_chunks_indexed,
            embed_index.rag_chunks_skipped
        );
    }
    if let Some(error) = &report.embed_error {
        println!("embedding_error: {error}");
    }
    for source in &report.sources {
        println!(
            "- {} chunks={} unchanged={} hash={}",
            source.path, source.chunks, source.unchanged, source.content_hash
        );
    }
    for skipped in &report.skipped {
        println!("skip: {} ({})", skipped.path, skipped.reason);
    }
    Ok(())
}

pub(crate) fn print_rag_sources(conn: &Connection, request: RagSourcesRequest<'_>) -> Result<()> {
    let json_out = request.json;
    let report = rag_sources_report(
        conn,
        request.root,
        request.provider,
        request.endpoint,
        request.model,
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("RAG Sources");
    println!(
        "status: {} ready: {}/{} chunks: {} embeddings: indexed={} missing={} stale={}",
        report.status,
        report.ready_sources,
        report.total_sources,
        report.total_chunks,
        report.chunk_embeddings_indexed,
        report.chunk_embeddings_missing,
        report.chunk_embeddings_stale
    );
    for source in &report.sources {
        println!(
            "- {} chunks={} ready={} stale={} missing={} orphan={} embeddings_ready={} embeddings_missing={} embeddings_stale={}",
            source.path,
            source.chunks,
            source.ready,
            source.stale,
            source.missing,
            source.orphan,
            source.chunk_embeddings_ready,
            source.chunk_embeddings_missing,
            source.chunk_embeddings_stale
        );
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn rag_sources_report(
    conn: &Connection,
    root: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<RagSourcesReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let embedding_freshness = super::embeddings::rag_chunk_embedding_freshness_by_source(
        conn, provider, endpoint, model,
    )?;
    let mut stmt = conn.prepare(
        r#"
        SELECT s.id, s.path, s.content_hash, s.status, s.suggestions, s.ingested_at,
               COUNT(c.id) AS chunk_count
        FROM memory_sources s
        LEFT JOIN rag_chunks c ON c.source_id = s.id
        WHERE s.status = 'rag_indexed'
        GROUP BY s.id
        ORDER BY s.ingested_at DESC, s.path ASC
        "#,
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut path_counts = HashMap::new();
    for (_, path, _, _, _, _, _) in &rows {
        *path_counts.entry(path.clone()).or_insert(0usize) += 1;
    }
    let mut sources = Vec::new();
    for (source_id, path, stored_hash, status, suggestions, ingested_at, chunk_count) in rows {
        let source_path = source_path(&root, &path);
        let (missing, current_hash) = match fs::read_to_string(&source_path) {
            Ok(content) => (false, Some(content_hash(&content))),
            Err(_) => (true, None),
        };
        let chunks = chunk_count.max(0) as usize;
        let stale = current_hash
            .as_deref()
            .is_some_and(|hash| hash != stored_hash.as_str());
        let orphan = chunks == 0;
        let duplicate_path = path_counts.get(&path).copied().unwrap_or_default() > 1;
        let freshness = embedding_freshness
            .get(&source_id)
            .cloned()
            .unwrap_or_default();
        let chunk_embeddings_ready = chunks > 0
            && freshness.indexed == chunks
            && freshness.missing == 0
            && freshness.stale == 0;
        let ready = !missing && !stale && !orphan && chunk_embeddings_ready;
        sources.push(RagSourceStatus {
            source_id,
            path,
            status,
            content_hash: stored_hash,
            current_hash,
            chunks,
            suggestions: suggestions.max(0) as usize,
            ingested_at,
            missing,
            stale,
            orphan,
            duplicate_path,
            chunk_embeddings_indexed: freshness.indexed,
            chunk_embeddings_missing: freshness.missing,
            chunk_embeddings_stale: freshness.stale,
            chunk_embeddings_ready,
            ready,
        });
    }
    let total_sources = sources.len();
    let ready_sources = sources.iter().filter(|source| source.ready).count();
    let stale_sources = sources.iter().filter(|source| source.stale).count();
    let missing_sources = sources.iter().filter(|source| source.missing).count();
    let orphan_sources = sources.iter().filter(|source| source.orphan).count();
    let duplicate_paths = path_counts.values().filter(|count| **count > 1).count();
    let total_chunks = sources.iter().map(|source| source.chunks).sum();
    let embedding_ready_sources = sources
        .iter()
        .filter(|source| source.chunk_embeddings_ready)
        .count();
    let chunk_embeddings_indexed = sources
        .iter()
        .map(|source| source.chunk_embeddings_indexed)
        .sum();
    let chunk_embeddings_missing = sources
        .iter()
        .map(|source| source.chunk_embeddings_missing)
        .sum();
    let chunk_embeddings_stale = sources
        .iter()
        .map(|source| source.chunk_embeddings_stale)
        .sum();
    let mut issues = Vec::new();
    if stale_sources > 0 {
        issues.push(format!(
            "{stale_sources} RAG source(s) are stale; rerun rag-ingest --apply"
        ));
    }
    if missing_sources > 0 {
        issues.push(format!("{missing_sources} RAG source file(s) are missing"));
    }
    if orphan_sources > 0 {
        issues.push(format!("{orphan_sources} RAG source row(s) have no chunks"));
    }
    if duplicate_paths > 0 {
        issues.push(format!(
            "{duplicate_paths} RAG source path(s) have historical duplicates"
        ));
    }
    if chunk_embeddings_missing > 0 {
        issues.push(format!(
            "{chunk_embeddings_missing} RAG chunk embedding(s) are missing; run embed-index"
        ));
    }
    if chunk_embeddings_stale > 0 {
        issues.push(format!(
            "{chunk_embeddings_stale} RAG chunk embedding(s) are stale; run embed-index"
        ));
    }
    let ok = total_sources > 0
        && stale_sources == 0
        && missing_sources == 0
        && orphan_sources == 0
        && chunk_embeddings_missing == 0
        && chunk_embeddings_stale == 0;
    let mut recommendations = vec![
        "rerun `dukememory rag-ingest PATH --apply` after source files change".to_string(),
        "run `dukememory embed-index` after RAG source changes so semantic chunk recall stays current"
            .to_string(),
        "keep durable decisions in memory cards and use source chunks for file evidence"
            .to_string(),
    ];
    if duplicate_paths > 0 || orphan_sources > 0 {
        recommendations.push(
            "historical source rows are harmless for retrieval but should be pruned by a future guarded cleanup"
                .to_string(),
        );
    }
    if total_sources == 0 {
        recommendations
            .push("index a source file with `dukememory rag-ingest PATH --apply`".to_string());
    }
    Ok(RagSourcesReport {
        version: RAG_INGEST_VERSION,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        total_sources,
        ready_sources,
        stale_sources,
        missing_sources,
        orphan_sources,
        duplicate_paths,
        total_chunks,
        embedding_provider: provider.to_string(),
        embedding_endpoint: endpoint.to_string(),
        embedding_model: model.to_string(),
        embedding_ready_sources,
        chunk_embeddings_indexed,
        chunk_embeddings_missing,
        chunk_embeddings_stale,
        sources,
        issues,
        recommendations,
    })
}

pub(crate) fn rag_ingest_report(
    conn: &Connection,
    request: RagIngestRequest<'_>,
) -> Result<RagIngestReport> {
    validate_scope(request.scope)?;
    let root = request
        .root
        .canonicalize()
        .unwrap_or_else(|_| request.root.to_path_buf());
    let input = if request.input.is_absolute() {
        request.input.to_path_buf()
    } else {
        root.join(request.input)
    };
    if !input.exists() {
        bail!("RAG ingest input does not exist: {}", input.display());
    }
    let max_files = request.max_files.clamp(1, 2_000);
    let max_file_bytes = request.max_file_bytes.clamp(1_024, 8_000_000);
    let chunk_chars = request.chunk_chars.clamp(300, 8_000);
    let overlap_chars = request.overlap_chars.min(chunk_chars / 2);

    let mut skipped = Vec::new();
    let files = collect_rag_files(&input, max_files, &mut skipped)?;
    let files_scanned = files.len();
    let mut sources = Vec::new();
    let mut chunks_indexed = 0usize;
    let mut chunks_written = 0usize;
    let mut files_unchanged = 0usize;
    let mut actions = Vec::new();
    let mut applied_source_paths = Vec::new();

    for path in files {
        let display_path = display_path(&root, &path);
        let metadata = fs::metadata(&path)
            .with_context(|| format!("failed to stat RAG source {}", path.display()))?;
        let bytes = metadata.len().min(usize::MAX as u64) as usize;
        if bytes > max_file_bytes {
            skipped.push(RagIngestSkip {
                path: display_path,
                reason: format!("file exceeds max_file_bytes={max_file_bytes}"),
            });
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                skipped.push(RagIngestSkip {
                    path: display_path,
                    reason: format!("not valid UTF-8 text: {error}"),
                });
                continue;
            }
        };
        if content.trim().is_empty() {
            skipped.push(RagIngestSkip {
                path: display_path,
                reason: "empty text file".to_string(),
            });
            continue;
        }
        let chunks = chunk_text(&content, chunk_chars, overlap_chars);
        if chunks.is_empty() {
            skipped.push(RagIngestSkip {
                path: display_path,
                reason: "no indexable chunks".to_string(),
            });
            continue;
        }
        let file_hash = content_hash(&content);
        let mut unchanged = false;
        if request.apply {
            unchanged =
                rag_source_chunks_current(conn, &display_path, request.scope, &file_hash, &chunks)?;
            if unchanged {
                files_unchanged += 1;
                actions.push(format!("unchanged_source:{display_path}"));
            } else {
                write_rag_source_chunks(conn, &display_path, request.scope, &file_hash, &chunks)?;
                chunks_written += chunks.len();
                actions.push(format!("indexed_source:{display_path}"));
            }
            applied_source_paths.push(display_path.clone());
        } else {
            actions.push(format!("dry_run_source:{display_path}"));
        }
        chunks_indexed += chunks.len();
        sources.push(RagIngestSource {
            path: display_path,
            content_hash: file_hash,
            bytes,
            chunks: chunks.len(),
            applied: request.apply,
            unchanged,
        });
    }

    if files_scanned >= max_files {
        actions.push(format!("max_files_limit_reached:{max_files}"));
    }
    let mut embed_index = None;
    let mut embed_error = None;
    if request.embed {
        if !request.apply {
            actions.push("embed_skipped:requires_apply".to_string());
        } else if chunks_indexed == 0 {
            actions.push("embed_skipped:no_chunks".to_string());
        } else {
            match super::embeddings::embed_rag_chunks_for_paths(
                conn,
                request.provider,
                request.endpoint,
                request.model,
                request.scope,
                &applied_source_paths,
                false,
            ) {
                Ok(report) => {
                    actions.push(format!(
                        "embed_targeted:paths={} rag_chunks_indexed={} rag_chunks_skipped={}",
                        applied_source_paths.len(),
                        report.rag_chunks_indexed,
                        report.rag_chunks_skipped
                    ));
                    embed_index = Some(report);
                }
                Err(error) => {
                    let message = error.to_string();
                    actions.push(format!("embed_index_failed:{message}"));
                    embed_error = Some(message);
                }
            }
        }
    }
    let ok = !sources.is_empty() && chunks_indexed > 0 && embed_error.is_none();
    Ok(RagIngestReport {
        version: RAG_INGEST_VERSION,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        input: input.display().to_string(),
        scope: request.scope.to_string(),
        applied: request.apply,
        embed_requested: request.embed,
        embedding_provider: request.provider.to_string(),
        embedding_endpoint: request.endpoint.to_string(),
        embedding_model: request.model.to_string(),
        embed_index,
        embed_error,
        files_scanned,
        files_indexed: sources.len(),
        files_unchanged,
        chunks_indexed,
        chunks_written,
        sources,
        skipped,
        actions,
        recommendations: rag_ingest_recommendations(request.apply, request.embed),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn query_rag_chunks(
    conn: &Connection,
    query: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<Vec<RagChunkHit>> {
    let terms = relevance_terms(query);
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let candidate_limit = limit
        .max(1)
        .saturating_mul(if budget <= 1_600 { 3 } else { 6 })
        .clamp(6, 96);
    let mut hits = Vec::new();
    if let Ok(rows) = super::embeddings::semantic_rag_chunk_search(
        conn,
        provider,
        endpoint,
        model,
        query,
        scope,
        candidate_limit,
    ) {
        hits.extend(rows.into_iter().map(|row| semantic_chunk_hit(row, &terms)));
    }
    if terms.is_empty() {
        return Ok(merge_chunk_hits(hits).into_iter().take(limit).collect());
    }
    let mut queries = vec![sanitize_fts_query(query)];
    if let Some(fallback) = sanitize_fts_any_query(query)
        && !queries.iter().any(|query| query == &fallback)
    {
        queries.push(fallback);
    }
    for fts_query in queries {
        if fts_query.trim().is_empty() || fts_query == "\"\"" {
            continue;
        }
        let rows = query_rag_chunks_once(conn, &fts_query, scope, candidate_limit, &terms)?;
        if !rows.is_empty() {
            hits.extend(rows);
            break;
        }
    }
    Ok(merge_chunk_hits(hits).into_iter().take(limit).collect())
}

fn query_rag_chunks_once(
    conn: &Connection,
    fts_query: &str,
    scope: Option<&str>,
    limit: usize,
    terms: &HashSet<String>,
) -> Result<Vec<RagChunkHit>> {
    let mut sql = String::from(
        r#"
        SELECT c.id, c.path, c.scope, c.chunk_index, c.start_line, c.end_line, c.content
        FROM rag_chunks c
        JOIN rag_chunks_fts fts ON fts.rowid = c.rowid
        WHERE rag_chunks_fts MATCH ?
        "#,
    );
    let mut values = vec![fts_query.to_string()];
    if let Some(scope) = scope {
        sql.push_str(" AND c.scope = ?");
        values.push(scope.to_string());
    }
    sql.push_str(" ORDER BY bm25(rag_chunks_fts), c.updated_at DESC LIMIT ?");
    values.push(limit.min(i64::MAX as usize).to_string());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        let chunk_index = row.get::<_, i64>(3)?.max(0) as usize;
        let start_line = row.get::<_, i64>(4)?.max(1) as usize;
        let end_line = row.get::<_, i64>(5)?.max(start_line as i64) as usize;
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            chunk_index,
            start_line,
            end_line,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut hits = Vec::new();
    for (rank, row) in rows.enumerate() {
        let (id, path, scope, chunk_index, start_line, end_line, content) = row?;
        let (score, reasons) = score_chunk_hit(&path, &content, rank, terms);
        hits.push(RagChunkHit {
            id,
            path,
            scope,
            chunk_index,
            start_line,
            end_line,
            content,
            score,
            semantic_score: None,
            reasons,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.chunk_index.cmp(&b.chunk_index))
    });
    Ok(hits)
}

fn semantic_chunk_hit(row: SemanticRagChunkRow, terms: &HashSet<String>) -> RagChunkHit {
    let text_tokens = tokenize(&format!("{} {}", row.path, row.content));
    let overlap = terms.intersection(&text_tokens).count();
    let normalized = ((row.score + 1.0) / 2.0).clamp(0.0, 1.0);
    let mut score = 3.5 + normalized * 5.0 + overlap as f64 * 1.2;
    let mut reasons = vec![
        "source_kind:chunk".to_string(),
        format!("semantic_chunk:{:.3}", row.score),
    ];
    if overlap > 0 {
        reasons.push(format!("text_match:{overlap}"));
    }
    if path_contains_term(&row.path, terms) {
        score += 1.5;
        reasons.push("path_match".to_string());
    }
    RagChunkHit {
        id: row.id,
        path: row.path,
        scope: row.scope,
        chunk_index: row.chunk_index,
        start_line: row.start_line,
        end_line: row.end_line,
        content: row.content,
        score,
        semantic_score: Some(row.score),
        reasons,
    }
}

fn merge_chunk_hits(hits: Vec<RagChunkHit>) -> Vec<RagChunkHit> {
    let mut merged: HashMap<String, RagChunkHit> = HashMap::new();
    for hit in hits {
        if let Some(existing) = merged.get_mut(&hit.id) {
            let had_semantic = existing.semantic_score.is_some();
            let had_fts = existing
                .reasons
                .iter()
                .any(|reason| reason.starts_with("fts_rank:"));
            let has_semantic = hit.semantic_score.is_some();
            let has_fts = hit
                .reasons
                .iter()
                .any(|reason| reason.starts_with("fts_rank:"));
            existing.score = existing.score.max(hit.score);
            if existing.semantic_score.is_none() {
                existing.semantic_score = hit.semantic_score;
            }
            for reason in hit.reasons {
                if !existing.reasons.contains(&reason) {
                    existing.reasons.push(reason);
                }
            }
            if (had_semantic || has_semantic) && (had_fts || has_fts) {
                existing.score += 1.25;
                if !existing
                    .reasons
                    .iter()
                    .any(|reason| reason == "hybrid_chunk")
                {
                    existing.reasons.push("hybrid_chunk".to_string());
                }
            }
        } else {
            merged.insert(hit.id.clone(), hit);
        }
    }
    let mut hits = merged.into_values().collect::<Vec<_>>();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.chunk_index.cmp(&b.chunk_index))
    });
    hits
}

fn collect_rag_files(
    input: &Path,
    max_files: usize,
    skipped: &mut Vec<RagIngestSkip>,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if input.is_file() {
        files.push(input.to_path_buf());
        return Ok(files);
    }
    collect_rag_files_inner(input, max_files, &mut files, skipped)?;
    files.sort();
    Ok(files)
}

fn collect_rag_files_inner(
    dir: &Path,
    max_files: usize,
    files: &mut Vec<PathBuf>,
    skipped: &mut Vec<RagIngestSkip>,
) -> Result<()> {
    if files.len() >= max_files {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if should_skip_rag_dir(&path) {
                skipped.push(RagIngestSkip {
                    path: path.display().to_string(),
                    reason: "service directory skipped".to_string(),
                });
                continue;
            }
            collect_rag_files_inner(&path, max_files, files, skipped)?;
        } else if is_rag_text_file(&path) {
            files.push(path);
            if files.len() >= max_files {
                break;
            }
        }
    }
    Ok(())
}

fn should_skip_rag_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some(
            ".agent"
                | ".codegraph"
                | ".git"
                | ".idea"
                | ".vscode"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
        )
    )
}

fn is_rag_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "css"
                | "csv"
                | "html"
                | "js"
                | "json"
                | "jsonl"
                | "jsx"
                | "log"
                | "md"
                | "py"
                | "rs"
                | "sh"
                | "sql"
                | "toml"
                | "ts"
                | "tsx"
                | "txt"
                | "yaml"
                | "yml"
        )
    )
}

fn chunk_text(content: &str, chunk_chars: usize, overlap_chars: usize) -> Vec<RagChunkDraft> {
    let lines = content.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let mut end = start;
        let mut chars = 0usize;
        while end < lines.len() && (chars < chunk_chars || end == start) {
            chars = chars.saturating_add(lines[end].chars().count() + 1);
            end += 1;
        }
        let body = lines[start..end].join("\n").trim().to_string();
        if !body.is_empty() {
            chunks.push(RagChunkDraft {
                index: chunks.len(),
                start_line: start + 1,
                end_line: end,
                content: body,
            });
        }
        if end >= lines.len() {
            break;
        }
        let mut overlap_start = end;
        let mut overlap = 0usize;
        while overlap_start > start && overlap < overlap_chars {
            overlap_start -= 1;
            overlap = overlap.saturating_add(lines[overlap_start].chars().count() + 1);
        }
        start = if overlap_start > start {
            overlap_start
        } else {
            end
        };
    }
    chunks
}

fn rag_source_chunks_current(
    conn: &Connection,
    path: &str,
    scope: &str,
    file_hash: &str,
    chunks: &[RagChunkDraft],
) -> Result<bool> {
    let mut stmt = conn.prepare(
        r#"
        SELECT c.chunk_index, c.start_line, c.end_line, c.content_hash
        FROM rag_chunks c
        JOIN memory_sources s ON s.id = c.source_id
        WHERE c.path = ?1
          AND c.scope = ?2
          AND s.path = ?1
          AND s.content_hash = ?3
          AND s.status = 'rag_indexed'
        ORDER BY c.chunk_index ASC
        "#,
    )?;
    let rows = stmt
        .query_map(params![path, scope, file_hash], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != chunks.len() {
        return Ok(false);
    }
    for ((chunk_index, start_line, end_line, stored_hash), chunk) in rows.iter().zip(chunks) {
        if *chunk_index != chunk.index.min(i64::MAX as usize) as i64
            || *start_line != chunk.start_line.min(i64::MAX as usize) as i64
            || *end_line != chunk.end_line.min(i64::MAX as usize) as i64
            || stored_hash != &content_hash(&chunk.content)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn write_rag_source_chunks(
    conn: &Connection,
    path: &str,
    scope: &str,
    file_hash: &str,
    chunks: &[RagChunkDraft],
) -> Result<()> {
    let now = now_ms();
    transactional(conn, "write_rag_source_chunks", || {
        conn.execute(
            r#"
        INSERT INTO memory_sources (
            path, content_hash, status, suggestions, ingested_at
        ) VALUES (?1, ?2, 'rag_indexed', ?3, ?4)
        ON CONFLICT(path, content_hash) DO UPDATE SET
            status = excluded.status,
            suggestions = excluded.suggestions,
            ingested_at = excluded.ingested_at
        "#,
            params![
                path,
                file_hash,
                chunks.len().min(i64::MAX as usize) as i64,
                now
            ],
        )?;
        let source_id: i64 = conn.query_row(
            "SELECT id FROM memory_sources WHERE path = ?1 AND content_hash = ?2",
            params![path, file_hash],
            |row| row.get(0),
        )?;
        conn.execute(
            "DELETE FROM rag_chunks WHERE path = ?1 AND scope = ?2",
            params![path, scope],
        )?;
        for chunk in chunks {
            let chunk_hash = content_hash(&chunk.content);
            let id = stable_chunk_id(path, scope, file_hash, chunk.index);
            conn.execute(
                r#"
                INSERT INTO rag_chunks (
                    id, source_id, path, scope, chunk_index, start_line, end_line,
                    content, content_hash, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    id,
                    source_id,
                    path,
                    scope,
                    chunk.index.min(i64::MAX as usize) as i64,
                    chunk.start_line.min(i64::MAX as usize) as i64,
                    chunk.end_line.min(i64::MAX as usize) as i64,
                    chunk.content,
                    chunk_hash,
                    now,
                    now,
                ],
            )?;
        }
        conn.execute(
            r#"
            DELETE FROM memory_sources
            WHERE path = ?1
              AND id != ?2
              AND id NOT IN (SELECT DISTINCT source_id FROM rag_chunks)
            "#,
            params![path, source_id],
        )?;
        Ok(())
    })
}

fn stable_chunk_id(path: &str, scope: &str, file_hash: &str, index: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(scope.as_bytes());
    hasher.update(b"\0");
    hasher.update(file_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(index.to_string().as_bytes());
    format!("{:x}", hasher.finalize())[..12].to_string()
}

fn score_chunk_hit(
    path: &str,
    content: &str,
    rank: usize,
    terms: &HashSet<String>,
) -> (f64, Vec<String>) {
    let text_tokens = tokenize(&format!("{path} {content}"));
    let overlap = terms.intersection(&text_tokens).count();
    let mut score =
        4.0 + overlap as f64 * 2.5 + (24usize.saturating_sub(rank).min(24) as f64 / 8.0);
    let mut reasons = vec![
        "source_kind:chunk".to_string(),
        format!("fts_rank:{}", rank + 1),
    ];
    if overlap > 0 {
        reasons.push(format!("text_match:{overlap}"));
    }
    if path_contains_term(path, terms) {
        score += 2.0;
        reasons.push("path_match".to_string());
    }
    (score, reasons)
}

fn path_contains_term(path: &str, terms: &HashSet<String>) -> bool {
    let lower = path.to_lowercase();
    terms.iter().any(|term| lower.contains(term))
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn source_path(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn rag_ingest_recommendations(applied: bool, embed_requested: bool) -> Vec<String> {
    let mut recommendations = vec![
        "run rag-debug against project questions after indexing sources".to_string(),
        "keep durable decisions as memory cards; use chunks for file/document evidence".to_string(),
    ];
    if !applied {
        recommendations.push("rerun with --apply to write chunked RAG sources".to_string());
        if embed_requested {
            recommendations
                .push("use --apply with --embed to refresh semantic chunk embeddings".to_string());
        }
    } else if !embed_requested {
        recommendations.push(
            "run embed-index or pass --embed on the next apply to refresh semantic chunk embeddings"
                .to_string(),
        );
    }
    recommendations
}

#[cfg(test)]
mod rag_ingest_tests {
    use super::*;

    #[test]
    fn chunk_text_keeps_line_ranges_and_progresses() {
        let chunks = chunk_text("one\ntwo\nthree\nfour\nfive", 10, 4);
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].start_line, 1);
        assert!(
            chunks
                .windows(2)
                .all(|pair| pair[0].start_line < pair[1].start_line)
        );
    }

    #[test]
    fn stable_chunk_id_changes_by_scope() {
        let a = stable_chunk_id("README.md", "project", "abc", 0);
        let b = stable_chunk_id("README.md", "repo", "abc", 0);
        assert_ne!(a, b);
        assert_eq!(a.len(), 12);
    }

    #[test]
    fn rag_sources_report_gates_chunk_embedding_freshness() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let db = root.join(".agent").join("memory.db");
        let conn = open_db(&db)?;
        let input = root.join("source.md");
        fs::write(
            &input,
            "Agents can use `memory_rag_ingest` and `POST /rag-ingest` for source chunks.\n",
        )?;

        let ingest = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &input,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: false,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;
        assert!(ingest.ok);

        let before = rag_sources_report(&conn, root, "mock", "mock", "mock-embedding")?;
        assert!(!before.ok);
        assert_eq!(before.chunk_embeddings_missing, before.total_chunks);
        assert_eq!(before.chunk_embeddings_stale, 0);

        let indexed = crate::app::embeddings::embed_index(
            &conn,
            "mock",
            "mock",
            "mock-embedding",
            &[],
            None,
            false,
        )?;
        assert_eq!(indexed.rag_chunks_indexed, before.total_chunks);

        let after = rag_sources_report(&conn, root, "mock", "mock", "mock-embedding")?;
        assert!(after.ok);
        assert_eq!(after.chunk_embeddings_missing, 0);
        assert_eq!(after.chunk_embeddings_stale, 0);
        assert_eq!(after.embedding_ready_sources, after.total_sources);
        Ok(())
    }

    #[test]
    fn rag_ingest_can_refresh_chunk_embeddings_after_apply() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let db = root.join(".agent").join("memory.db");
        let conn = open_db(&db)?;
        let input = root.join("source.md");
        fs::write(
            &input,
            "Agents can index source chunks and refresh semantic embeddings in one apply pass.\n",
        )?;

        let report = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &input,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: true,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;

        assert!(report.ok);
        assert!(report.embed_requested);
        assert!(report.embed_error.is_none());
        let embed_index = report.embed_index.expect("embed report");
        assert_eq!(embed_index.rag_chunks_indexed, report.chunks_indexed);

        let sources = rag_sources_report(&conn, root, "mock", "mock", "mock-embedding")?;
        assert!(sources.ok);
        assert_eq!(sources.chunk_embeddings_missing, 0);
        assert_eq!(sources.chunk_embeddings_stale, 0);
        Ok(())
    }

    #[test]
    fn rag_ingest_preserves_unchanged_chunks_and_embeddings() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let db = root.join(".agent").join("memory.db");
        let conn = open_db(&db)?;
        let input = root.join("source.md");
        fs::write(
            &input,
            "Repeated RAG ingest should preserve unchanged source chunks and their semantic embeddings.\n",
        )?;

        let first = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &input,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: true,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;
        assert!(first.ok);
        assert_eq!(first.files_unchanged, 0);
        assert_eq!(first.chunks_written, first.chunks_indexed);
        assert!(!first.sources[0].unchanged);
        let first_embed = first.embed_index.as_ref().expect("first embed report");
        assert_eq!(first_embed.rag_chunks_indexed, first.chunks_indexed);
        assert_eq!(first_embed.rag_chunks_skipped, 0);

        let second = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &input,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: true,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;
        assert!(second.ok);
        assert_eq!(second.files_unchanged, 1);
        assert_eq!(second.chunks_written, 0);
        assert_eq!(second.chunks_indexed, first.chunks_indexed);
        assert!(second.sources[0].unchanged);
        assert!(
            second
                .actions
                .iter()
                .any(|action| action.starts_with("unchanged_source:"))
        );
        let second_embed = second.embed_index.as_ref().expect("second embed report");
        assert_eq!(second_embed.rag_chunks_indexed, 0);
        assert_eq!(second_embed.rag_chunks_skipped, second.chunks_indexed);
        Ok(())
    }

    #[test]
    fn rag_ingest_embed_only_refreshes_touched_source_paths() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let db = root.join(".agent").join("memory.db");
        let conn = open_db(&db)?;
        let first = root.join("first.md");
        let second = root.join("second.md");
        fs::write(
            &first,
            "First source is intentionally left without semantic chunk embeddings.\n",
        )?;
        fs::write(
            &second,
            "Second source should get semantic chunk embeddings during ingest.\n",
        )?;

        let first_report = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &first,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: false,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;
        assert!(first_report.ok);

        let second_report = rag_ingest_report(
            &conn,
            RagIngestRequest {
                root,
                input: &second,
                scope: "project",
                apply: true,
                chunk_chars: 900,
                overlap_chars: 140,
                max_file_bytes: 200_000,
                max_files: 8,
                embed: true,
                provider: "mock",
                endpoint: "mock",
                model: "mock-embedding",
                json: true,
            },
        )?;
        let embed_index = second_report.embed_index.expect("embed report");
        assert_eq!(embed_index.indexed, 0);
        assert_eq!(embed_index.skipped, 0);
        assert_eq!(embed_index.rag_chunks_indexed, second_report.chunks_indexed);

        let sources = rag_sources_report(&conn, root, "mock", "mock", "mock-embedding")?;
        assert!(!sources.ok);
        assert_eq!(
            sources.total_chunks,
            first_report.chunks_indexed + second_report.chunks_indexed
        );
        assert_eq!(
            sources.chunk_embeddings_missing,
            first_report.chunks_indexed
        );
        assert_eq!(
            sources.chunk_embeddings_indexed,
            second_report.chunks_indexed
        );
        Ok(())
    }

    #[test]
    fn merge_chunk_hits_combines_semantic_and_fts_signals() {
        let semantic = RagChunkHit {
            id: "chunk-a".to_string(),
            path: "README.md".to_string(),
            scope: "project".to_string(),
            chunk_index: 0,
            start_line: 1,
            end_line: 2,
            content: "semantic source".to_string(),
            score: 6.0,
            semantic_score: Some(0.82),
            reasons: vec![
                "source_kind:chunk".to_string(),
                "semantic_chunk:0.820".to_string(),
            ],
        };
        let fts = RagChunkHit {
            id: "chunk-a".to_string(),
            path: "README.md".to_string(),
            scope: "project".to_string(),
            chunk_index: 0,
            start_line: 1,
            end_line: 2,
            content: "semantic source".to_string(),
            score: 5.0,
            semantic_score: None,
            reasons: vec!["source_kind:chunk".to_string(), "fts_rank:1".to_string()],
        };

        let merged = merge_chunk_hits(vec![semantic, fts]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].semantic_score, Some(0.82));
        assert!(
            merged[0]
                .reasons
                .iter()
                .any(|reason| reason == "hybrid_chunk")
        );
        assert!(merged[0].score > 6.0);
    }
}

use super::*;

pub(crate) struct ContextQuery<'a> {
    pub(crate) task: &'a str,
    pub(crate) types: &'a [String],
    pub(crate) statuses: &'a [String],
    pub(crate) scope: Option<&'a str>,
    pub(crate) limit: usize,
    pub(crate) include_recent: usize,
    pub(crate) rules: Option<&'a Path>,
}

pub(crate) fn build_context_rows(
    conn: &Connection,
    query: ContextQuery<'_>,
) -> Result<Vec<Memory>> {
    let task_terms = relevance_terms(query.task);
    let mut rows = query_memories(
        conn,
        Some(query.task),
        query.types,
        query.statuses,
        query.scope,
        query.limit,
    )?;
    let has_direct_matches = !rows.is_empty();
    if query.include_recent > 0 {
        let recent = query_memories(
            conn,
            None,
            query.types,
            &["active".to_string()],
            query.scope,
            if has_direct_matches {
                query.include_recent
            } else {
                1
            },
        )?;
        for row in recent {
            if !rows.iter().any(|existing| existing.id == row.id)
                && (!has_direct_matches || context_recent_matches_task(conn, &row, &task_terms)?)
            {
                rows.push(row);
            }
        }
    }
    let quality_signals = retrieval_quality_signals(conn, 30).unwrap_or_default();
    rows = filter_query_useless_memories(rows, query.task, &quality_signals);
    rank_context_rows_with_quality(
        &mut rows,
        query.task,
        query.scope,
        query.rules,
        Some(&quality_signals),
    );
    rows = select_diverse_memories(rows, query.limit);
    Ok(rows)
}

fn context_recent_matches_task(
    conn: &Connection,
    memory: &Memory,
    task_terms: &HashSet<String>,
) -> Result<bool> {
    if task_terms.len() < 2 {
        return Ok(false);
    }
    let required_overlap = task_terms.len().min(2);
    let mut tokens = memory_signature(memory);
    for link in get_links(conn, &memory.id)? {
        tokens.extend(tokenize(&format!("{} {}", link.kind, link.target)));
    }
    Ok(task_terms.intersection(&tokens).count() >= required_overlap)
}

pub(crate) struct RetrieveRequest<'a> {
    pub(crate) query: &'a str,
    pub(crate) strategy: RetrievalStrategy,
    pub(crate) format: OutputFormat,
    pub(crate) limit: usize,
    pub(crate) budget: usize,
    pub(crate) scope: Option<&'a str>,
    pub(crate) rules: Option<&'a Path>,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) audit_read: bool,
}

pub(crate) fn print_retrieve(conn: &Connection, request: RetrieveRequest<'_>) -> Result<()> {
    let report = retrieve_report(conn, &request)?;
    match request.format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Agent => {
            println!("Retrieved Memory:");
            println!("{}", report.receipt);
            println!(
                "{}",
                render_retrieval_pack(
                    &report.hits,
                    request.budget,
                    request.query,
                    report.semantic_skip_reason.as_deref()
                )?
            );
            println!("\nSelection Reasons:");
            for hit in &report.hits {
                println!(
                    "- {} score={:.2}: {}",
                    hit.memory.memory.title,
                    hit.score,
                    hit.reasons.join(", ")
                );
            }
            if let Some(error) = &report.semantic_error {
                println!("\nSemantic fallback: {error}");
            }
            println!(
                "\nUse these memories as constraints unless contradicted by newer user input."
            );
        }
        OutputFormat::Markdown => {
            println!("## Retrieved Memory");
            println!("{}", report.receipt);
            for hit in &report.hits {
                let row = &hit.memory.memory;
                println!(
                    "- **{}** `{}` score={:.2}: {}",
                    row.title, row.memory_type, hit.score, row.body
                );
            }
        }
        OutputFormat::Plain => {
            println!(
                "{}",
                render_retrieval_pack(
                    &report.hits,
                    request.budget,
                    request.query,
                    report.semantic_skip_reason.as_deref()
                )?
            );
            println!("{}", report.receipt);
        }
    }
    Ok(())
}

pub(crate) fn retrieve_report(
    conn: &Connection,
    request: &RetrieveRequest<'_>,
) -> Result<RetrievalReport> {
    let started = Instant::now();
    let task_terms = relevance_terms(request.query);
    let effective_limit = budget_aware_hit_limit(request.limit, request.budget, task_terms.len());
    let mut candidates: HashMap<String, (Memory, Option<f64>)> = HashMap::new();
    let fts_rows = query_memories(
        conn,
        Some(request.query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        request.scope,
        request.limit.saturating_mul(2).max(request.limit),
    )?;
    let direct_fts_count = non_redundant_memory_count(&fts_rows, request.budget, &task_terms);
    for row in fts_rows {
        candidates.entry(row.id.clone()).or_insert((row, None));
    }
    if should_include_recent_fallback(
        &task_terms,
        request.budget,
        direct_fts_count,
        effective_limit,
    ) {
        for row in query_memories(
            conn,
            None,
            &[],
            &["active".to_string()],
            request.scope,
            (request.limit / 3).max(2),
        )? {
            candidates.entry(row.id.clone()).or_insert((row, None));
        }
    }
    let mut semantic_used = false;
    let semantic_skip_reason = if matches!(request.strategy, RetrievalStrategy::Hybrid) {
        semantic_skip_reason_for_query(
            &task_terms,
            request.budget,
            direct_fts_count,
            effective_limit,
        )
        .map(ToOwned::to_owned)
    } else {
        None
    };
    let semantic_skipped = semantic_skip_reason.is_some();
    let mut semantic_error = None;
    if matches!(request.strategy, RetrievalStrategy::Hybrid) && !semantic_skipped {
        let semantic_threshold = semantic_score_threshold(request.budget);
        match embeddings::semantic_index_ready(
            conn,
            request.provider,
            request.endpoint,
            request.model,
        ) {
            Ok(true) => {
                match embeddings::semantic_search(
                    conn,
                    request.provider,
                    request.endpoint,
                    request.model,
                    request.query,
                    request.limit,
                ) {
                    Ok(semantic) => {
                        for item in semantic {
                            if item.score < semantic_threshold {
                                continue;
                            }
                            let memory = item.memory.memory;
                            if !matches!(memory.status.as_str(), "active" | "uncertain") {
                                continue;
                            }
                            if let Some(scope) = request.scope
                                && memory.scope != scope
                            {
                                continue;
                            }
                            candidates
                                .entry(memory.id.clone())
                                .and_modify(|existing| existing.1 = Some(item.score))
                                .or_insert((memory, Some(item.score)));
                            semantic_used = true;
                        }
                    }
                    Err(err) => semantic_error = Some(err.to_string()),
                }
            }
            Ok(false) => {
                semantic_error =
                    Some("semantic index not ready; using FTS/local ranking".to_string());
            }
            Err(err) => semantic_error = Some(format!("semantic readiness check failed: {err}")),
        }
    }
    let rhai = request.rules.and_then(|path| load_rhai_rules(path).ok());
    let quality_signals = retrieval_quality_signals(conn, 30).unwrap_or_default();
    let mut hits = Vec::new();
    for (_, (memory, semantic_score)) in candidates {
        if !rhai_should_include(rhai.as_ref(), &memory, request.query)? {
            continue;
        }
        let links = get_links(conn, &memory.id)?;
        let (score, reasons) = retrieval_score(
            &memory,
            &links,
            RetrievalScoreContext {
                task_terms: &task_terms,
                requested_scope: request.scope,
                semantic_score,
                rules: rhai.as_ref(),
                task: request.query,
                quality_signals: Some(&quality_signals),
            },
        );
        let utility_score = memory_utility_score(&memory, links.len(), Some(&quality_signals));
        hits.push(RetrievalHit {
            memory: MemoryWithLinks { memory, links },
            score,
            utility_score,
            semantic_score,
            reasons,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory.memory.updated_at.cmp(&a.memory.memory.updated_at))
    });
    hits = apply_tiny_lexical_precision_gate(hits, request.budget, &task_terms);
    hits = apply_tiny_feedback_precision_gate(hits, request.budget, &task_terms, &quality_signals);
    hits = apply_relevance_floor(hits, request.budget);
    hits = filter_redundant_hits(hits, request.budget);
    hits = select_diverse_hits(hits, effective_limit);
    let ids = hits
        .iter()
        .map(|hit| hit.memory.memory.id.clone())
        .collect::<Vec<_>>();
    let semantic_status = match request.strategy {
        RetrievalStrategy::Hybrid if semantic_skipped => MemorySemanticStatus::Skipped,
        RetrievalStrategy::Hybrid if semantic_used => MemorySemanticStatus::Used,
        RetrievalStrategy::Hybrid => MemorySemanticStatus::Fallback,
        RetrievalStrategy::Fts => MemorySemanticStatus::None,
    };
    let receipt = memory_receipt_with_semantic("retrieve", semantic_status, &ids, "none");
    if request.audit_read {
        log_read_event(
            conn,
            ReadEventInput {
                command: "retrieve",
                query: request.query,
                ids: &ids,
                semantic_used,
                result_count: hits.len(),
                budget: request.budget,
                elapsed_ms: started.elapsed().as_millis(),
            },
        )?;
    }
    Ok(RetrievalReport {
        version: 14,
        query: request.query.to_string(),
        strategy: format!("{:?}", request.strategy).to_lowercase(),
        scope: request.scope.map(ToOwned::to_owned),
        semantic_used,
        semantic_skipped,
        semantic_skip_reason,
        semantic_error,
        receipt,
        hits,
    })
}

fn should_include_recent_fallback(
    task_terms: &HashSet<String>,
    budget: usize,
    direct_fts_count: usize,
    effective_limit: usize,
) -> bool {
    task_terms.len() >= 2
        && semantic_skip_reason_for_query(task_terms, budget, direct_fts_count, effective_limit)
            != Some("lexical_saturated")
}

fn semantic_skip_reason_for_query(
    task_terms: &HashSet<String>,
    budget: usize,
    direct_fts_count: usize,
    effective_limit: usize,
) -> Option<&'static str> {
    match task_terms.len() {
        0 => Some("generic_query"),
        1 => Some("weak_query"),
        _ if budget <= 1_200 && direct_fts_count >= effective_limit => Some("lexical_saturated"),
        _ => None,
    }
}

fn semantic_score_threshold(budget: usize) -> f64 {
    if budget <= 1_200 {
        0.18
    } else if budget <= 2_500 {
        0.12
    } else {
        0.05
    }
}

fn non_redundant_memory_count(
    rows: &[Memory],
    budget: usize,
    required_terms: &HashSet<String>,
) -> usize {
    let Some(threshold) = redundancy_threshold_for_budget(budget) else {
        return rows
            .iter()
            .filter(|row| required_terms.is_subset(&memory_signature(row)))
            .count();
    };
    let mut signatures: Vec<HashSet<String>> = Vec::new();
    for row in rows {
        if !is_trusted_lexical_saturation_candidate(row) {
            continue;
        }
        let signature = memory_signature(row);
        if !required_terms.is_subset(&signature) {
            continue;
        }
        if signature.len() >= 4
            && signatures
                .iter()
                .any(|existing| containment_overlap(&signature, existing) >= threshold)
        {
            continue;
        }
        signatures.push(signature);
    }
    signatures.len()
}

fn is_trusted_lexical_saturation_candidate(memory: &Memory) -> bool {
    memory.status == "active" && memory.confidence >= 0.8
}

pub(crate) fn retrieve_rows(
    conn: &Connection,
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<Vec<Memory>> {
    Ok(retrieve_report(
        conn,
        &RetrieveRequest {
            query,
            strategy,
            format: OutputFormat::Plain,
            limit,
            budget: usize::MAX,
            scope: None,
            rules: None,
            provider,
            endpoint,
            model,
            audit_read: false,
        },
    )?
    .hits
    .into_iter()
    .map(|hit| hit.memory.memory)
    .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RecallReport {
    query: String,
    max_chars: usize,
    token_saving_estimate: usize,
    receipt: String,
    items: Vec<RecallItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecallItem {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    title: String,
    summary: String,
    score: f64,
    reasons: Vec<String>,
}

pub(crate) struct RecallRequest<'a> {
    pub(crate) query: &'a str,
    pub(crate) max_chars: usize,
    pub(crate) limit: usize,
    pub(crate) scope: Option<&'a str>,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) json_out: bool,
}

pub(crate) fn print_recall(conn: &Connection, request: RecallRequest<'_>) -> Result<()> {
    let report = recall_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_recall_report(&report));
    }
    Ok(())
}

pub(crate) fn recall_report(
    conn: &Connection,
    request: &RecallRequest<'_>,
) -> Result<RecallReport> {
    let retrieval = retrieve_report(
        conn,
        &RetrieveRequest {
            query: request.query,
            strategy: RetrievalStrategy::Hybrid,
            format: OutputFormat::Plain,
            limit: request.limit,
            budget: request.max_chars,
            scope: request.scope,
            rules: None,
            provider: request.provider,
            endpoint: request.endpoint,
            model: request.model,
            audit_read: true,
        },
    )?;
    let mut raw_chars = 0;
    let mut items = Vec::new();
    let query_terms = relevance_terms(request.query);
    for hit in &retrieval.hits {
        let memory = &hit.memory.memory;
        raw_chars += memory.body.chars().count();
        items.push(RecallItem {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            summary: query_focused_summary(&memory.body, &query_terms, 120),
            score: hit.score,
            reasons: hit.reasons.iter().take(3).cloned().collect(),
        });
    }
    let token_saving_estimate = raw_chars.saturating_sub(request.max_chars) / 4;
    Ok(RecallReport {
        query: request.query.to_string(),
        max_chars: request.max_chars,
        token_saving_estimate,
        receipt: retrieval.receipt,
        items,
    })
}

fn render_recall_report(report: &RecallReport) -> String {
    let mut out = format!(
        "Compressed Recall: {}\n{}\nEstimated token saving: {}\n",
        report.query, report.receipt, report.token_saving_estimate
    );
    for item in &report.items {
        let line = format!(
            "- {} [{}] {} -- {} ({})\n",
            item.id,
            item.memory_type,
            item.title,
            item.summary,
            item.reasons.join(",")
        );
        if out.len() + line.len() > report.max_chars {
            break;
        }
        out.push_str(&line);
    }
    truncate_chars(&out, report.max_chars)
}

struct RetrievalScoreContext<'a> {
    task_terms: &'a HashSet<String>,
    requested_scope: Option<&'a str>,
    semantic_score: Option<f64>,
    rules: Option<&'a RhaiRules>,
    task: &'a str,
    quality_signals: Option<&'a RetrievalQualitySignals>,
}

fn retrieval_score(
    memory: &Memory,
    links: &[MemoryLink],
    context: RetrievalScoreContext<'_>,
) -> (f64, Vec<String>) {
    let mut score = context_score(memory, context.task_terms, context.requested_scope);
    let mut reasons = Vec::new();
    reasons.push(format!("type:{}", memory.memory_type));
    reasons.push(format!("status:{}", memory.status));
    if memory.confidence >= 0.8 {
        reasons.push("high_confidence".to_string());
    } else if memory.confidence < 0.5 {
        reasons.push("low_confidence".to_string());
    }
    if let Some(scope) = context.requested_scope
        && memory.scope == scope
    {
        reasons.push("scope_match".to_string());
    }
    let haystack = tokenize(&format!("{} {}", memory.title, memory.body));
    let overlap = context.task_terms.intersection(&haystack).count();
    if overlap > 0 {
        reasons.push(format!("text_match:{overlap}"));
        score += overlap as f64;
    }
    let link_overlap = links
        .iter()
        .map(|link| tokenize(&format!("{} {}", link.kind, link.target)))
        .map(|tokens| context.task_terms.intersection(&tokens).count())
        .sum::<usize>();
    if link_overlap > 0 {
        reasons.push(format!("link_match:{link_overlap}"));
        score += link_overlap as f64 * 3.0;
    }
    if let Some(value) = context.semantic_score {
        reasons.push(format!("semantic:{value:.3}"));
        score += value * 12.0;
    }
    score += retrieval_quality_adjustment(&memory.id, context.quality_signals, &mut reasons);
    let body_chars = memory.body.chars().count();
    if body_chars > 1_600 {
        let penalty = ((body_chars - 1_600) as f64 / 800.0).min(4.0);
        reasons.push(format!("token_heavy:-{penalty:.1}"));
        score -= penalty;
    }
    if let Some(id) = &memory.superseded_by {
        reasons.push(format!("superseded_by:{id}"));
        score -= 25.0;
    }
    if memory.supersedes.is_some() {
        reasons.push("supersedes_previous".to_string());
        score += 1.5;
    }
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    if age_days <= 7.0 {
        reasons.push("fresh".to_string());
    }
    let rhai = rhai_score(context.rules, memory, context.task).unwrap_or(0.0);
    if rhai != 0.0 {
        reasons.push(format!("rhai_score:{rhai:.2}"));
        score += rhai;
    }
    (score, reasons)
}

fn memory_utility_score(
    memory: &Memory,
    link_count: usize,
    quality_signals: Option<&RetrievalQualitySignals>,
) -> f64 {
    let mut score = memory.confidence * 10.0;
    score += link_count.min(6) as f64 * 1.5;
    score += match memory.memory_type.as_str() {
        "decision" | "constraint" | "product_goal" => 5.0,
        "known_issue" | "task_state" => 3.0,
        _ => 1.0,
    };
    if memory.superseded_by.is_some() {
        score -= 8.0;
    }
    let mut reasons = Vec::new();
    score += retrieval_quality_adjustment(&memory.id, quality_signals, &mut reasons) * 0.5;
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    score - (age_days / 14.0).min(4.0)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RetrievalQualitySignals {
    reads: HashMap<String, usize>,
    useful: HashMap<String, usize>,
    useless: HashMap<String, usize>,
    useless_query_terms: HashMap<String, Vec<HashSet<String>>>,
}

pub(crate) fn retrieval_quality_signals(
    conn: &Connection,
    since_days: i64,
) -> Result<RetrievalQualitySignals> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let mut stmt =
        conn.prepare("SELECT memory_ids FROM memory_read_events WHERE created_at >= ?1")?;
    let rows = stmt
        .query_map(params![since_ms], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut reads = HashMap::new();
    for row in rows {
        for id in split_csv(Some(&row)) {
            *reads.entry(id).or_insert(0) += 1;
        }
    }
    let mut useful = HashMap::new();
    let mut useless = HashMap::new();
    let mut useless_query_terms: HashMap<String, Vec<HashSet<String>>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT detail FROM memory_events WHERE event_type = 'memory_feedback' AND created_at >= ?1",
    )?;
    let feedback_rows = stmt
        .query_map(params![since_ms], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for detail in feedback_rows {
        let Ok(value) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        let rating = value.get("rating").and_then(Value::as_str).unwrap_or("");
        let ids = value
            .get("ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let query_terms = value
            .get("query")
            .and_then(Value::as_str)
            .map(relevance_terms)
            .unwrap_or_default();
        for id in ids
            .into_iter()
            .filter_map(|id| id.as_str().map(str::to_string))
        {
            match rating {
                "useful" => {
                    *useful.entry(id).or_insert(0) += 1;
                }
                "useless" => {
                    *useless.entry(id.clone()).or_insert(0) += 1;
                    if !query_terms.is_empty() {
                        useless_query_terms
                            .entry(id)
                            .or_default()
                            .push(query_terms.clone());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(RetrievalQualitySignals {
        reads,
        useful,
        useless,
        useless_query_terms,
    })
}

pub(crate) fn retrieval_quality_adjustment(
    memory_id: &str,
    signals: Option<&RetrievalQualitySignals>,
    reasons: &mut Vec<String>,
) -> f64 {
    let Some(signals) = signals else {
        return 0.0;
    };
    let reads = signals.reads.get(memory_id).copied().unwrap_or_default();
    let useful = signals.useful.get(memory_id).copied().unwrap_or_default();
    let useless = signals.useless.get(memory_id).copied().unwrap_or_default();
    let mut score = 0.0;
    if reads > 0 {
        let boost = (reads.min(8) as f64) * 0.6;
        reasons.push(format!("recent_reads:{reads}"));
        score += boost;
    }
    if useful > 0 {
        let boost = (useful.min(4) as f64) * 2.5;
        reasons.push(format!("useful_feedback:+{useful}"));
        score += boost;
    }
    if useless > 0 {
        let penalty = (useless.min(4) as f64) * 4.0;
        reasons.push(format!("useless_feedback:-{useless}"));
        score -= penalty;
    }
    score
}

fn select_diverse_hits(hits: Vec<RetrievalHit>, limit: usize) -> Vec<RetrievalHit> {
    select_diverse_by_type(hits, limit, |hit| &hit.memory.memory.memory_type)
}

fn budget_aware_hit_limit(limit: usize, budget: usize, relevance_term_count: usize) -> usize {
    let budget_limit = if budget <= 1_200 {
        match relevance_term_count {
            0 => 2,
            1 => 3,
            _ => 5,
        }
    } else if budget <= 2_500 {
        match relevance_term_count {
            0 => 3,
            1 => 5,
            _ => 8,
        }
    } else {
        limit
    };
    limit.min(budget_limit).max(1)
}

fn apply_relevance_floor(hits: Vec<RetrievalHit>, budget: usize) -> Vec<RetrievalHit> {
    let Some(top_score) = hits.first().map(|hit| hit.score) else {
        return hits;
    };
    let Some(floor) = relevance_floor_for_budget(budget, top_score) else {
        return hits;
    };
    let minimum_keep = relevance_floor_minimum_keep(budget);
    let mut kept = Vec::new();
    for hit in hits {
        if kept.len() < minimum_keep || hit.score >= floor {
            kept.push(hit);
        }
    }
    kept
}

fn apply_tiny_lexical_precision_gate(
    hits: Vec<RetrievalHit>,
    budget: usize,
    task_terms: &HashSet<String>,
) -> Vec<RetrievalHit> {
    if budget > 1_200 || task_terms.len() < 2 {
        return hits;
    }
    let required_overlap = task_terms.len().min(2);
    hits.into_iter()
        .filter(|hit| {
            hit.semantic_score.is_some() || hit_lexical_overlap(hit, task_terms) >= required_overlap
        })
        .collect()
}

fn apply_tiny_feedback_precision_gate(
    hits: Vec<RetrievalHit>,
    budget: usize,
    task_terms: &HashSet<String>,
    signals: &RetrievalQualitySignals,
) -> Vec<RetrievalHit> {
    if budget > 1_200 || task_terms.len() < 2 {
        return hits;
    }
    hits.into_iter()
        .filter(|hit| {
            let id = &hit.memory.memory.id;
            !should_suppress_for_query_feedback(id, task_terms, signals)
        })
        .collect()
}

fn should_suppress_for_query_feedback(
    memory_id: &str,
    task_terms: &HashSet<String>,
    signals: &RetrievalQualitySignals,
) -> bool {
    let useless = signals.useless.get(memory_id).copied().unwrap_or_default();
    if useless == 0 {
        return false;
    }
    let useful = signals.useful.get(memory_id).copied().unwrap_or_default();
    useless > useful && feedback_query_matches_task(memory_id, task_terms, signals)
}

fn feedback_query_matches_task(
    memory_id: &str,
    task_terms: &HashSet<String>,
    signals: &RetrievalQualitySignals,
) -> bool {
    let Some(feedback_queries) = signals.useless_query_terms.get(memory_id) else {
        return false;
    };
    let required_overlap = task_terms.len().min(2);
    feedback_queries
        .iter()
        .any(|terms| task_terms.intersection(terms).count() >= required_overlap)
}

fn hit_lexical_overlap(hit: &RetrievalHit, task_terms: &HashSet<String>) -> usize {
    let memory = &hit.memory.memory;
    let mut tokens = tokenize(&format!("{} {}", memory.title, memory.body));
    for link in &hit.memory.links {
        tokens.extend(tokenize(&format!("{} {}", link.kind, link.target)));
    }
    task_terms.intersection(&tokens).count()
}

fn relevance_floor_minimum_keep(budget: usize) -> usize {
    if budget <= 1_200 { 1 } else { 2 }
}

fn relevance_floor_for_budget(budget: usize, top_score: f64) -> Option<f64> {
    if budget <= 1_200 {
        Some((top_score - 18.0).max(8.0))
    } else if budget <= 2_500 {
        Some((top_score - 24.0).max(4.0))
    } else {
        None
    }
}

fn filter_redundant_hits(hits: Vec<RetrievalHit>, budget: usize) -> Vec<RetrievalHit> {
    let Some(threshold) = redundancy_threshold_for_budget(budget) else {
        return hits;
    };
    let mut selected = Vec::new();
    let mut signatures: Vec<HashSet<String>> = Vec::new();
    for hit in hits {
        let signature = memory_signature(&hit.memory.memory);
        if signature.len() >= 4
            && signatures
                .iter()
                .any(|existing| containment_overlap(&signature, existing) >= threshold)
        {
            continue;
        }
        signatures.push(signature);
        selected.push(hit);
    }
    selected
}

fn redundancy_threshold_for_budget(budget: usize) -> Option<f64> {
    if budget <= 1_200 {
        Some(0.72)
    } else if budget <= 2_500 {
        Some(0.84)
    } else {
        None
    }
}

fn memory_signature(memory: &Memory) -> HashSet<String> {
    tokenize(&format!("{} {}", memory.title, memory.body))
}

fn containment_overlap(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    let smaller = left.len().min(right.len());
    if smaller == 0 {
        return 0.0;
    }
    let overlap = left.intersection(right).count();
    overlap as f64 / smaller as f64
}

fn select_diverse_memories(rows: Vec<Memory>, limit: usize) -> Vec<Memory> {
    select_diverse_by_type(rows, limit, |memory| &memory.memory_type)
}

pub(crate) fn filter_query_useless_memories(
    rows: Vec<Memory>,
    task: &str,
    signals: &RetrievalQualitySignals,
) -> Vec<Memory> {
    let task_terms = relevance_terms(task);
    if task_terms.len() < 2 {
        return rows;
    }
    rows.into_iter()
        .filter(|memory| !should_suppress_for_query_feedback(&memory.id, &task_terms, signals))
        .collect()
}

fn select_diverse_by_type<T, F>(items: Vec<T>, limit: usize, memory_type: F) -> Vec<T>
where
    F: Fn(&T) -> &str,
{
    if items.len() <= limit {
        return items;
    }
    let per_type_limit = if limit <= 5 { 2 } else { 3 };
    let mut selected = Vec::new();
    let mut deferred = Vec::new();
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    for item in items {
        if selected.len() >= limit {
            deferred.push(item);
            continue;
        }
        let kind = memory_type(&item).to_string();
        let count = type_counts.get(&kind).copied().unwrap_or_default();
        if count < per_type_limit {
            *type_counts.entry(kind).or_insert(0) += 1;
            selected.push(item);
        } else {
            deferred.push(item);
        }
    }
    if limit <= 5 {
        return selected;
    }
    for item in deferred {
        if selected.len() >= limit {
            break;
        }
        selected.push(item);
    }
    selected
}

pub(crate) fn rank_context_rows(
    rows: &mut [Memory],
    task: &str,
    requested_scope: Option<&str>,
    rules: Option<&Path>,
) {
    rank_context_rows_with_quality(rows, task, requested_scope, rules, None);
}

fn rank_context_rows_with_quality(
    rows: &mut [Memory],
    task: &str,
    requested_scope: Option<&str>,
    rules: Option<&Path>,
    quality_signals: Option<&RetrievalQualitySignals>,
) {
    let task_terms = relevance_terms(task);
    let rhai = rules.and_then(|path| load_rhai_rules(path).ok());
    rows.sort_by(|a, b| {
        let mut a_reasons = Vec::new();
        let mut b_reasons = Vec::new();
        let a_score = context_score(a, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), a, task).unwrap_or(0.0)
            + retrieval_quality_adjustment(&a.id, quality_signals, &mut a_reasons);
        let b_score = context_score(b, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), b, task).unwrap_or(0.0)
            + retrieval_quality_adjustment(&b.id, quality_signals, &mut b_reasons);
        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn context_score(
    memory: &Memory,
    task_terms: &HashSet<String>,
    requested_scope: Option<&str>,
) -> f64 {
    let mut score = memory.confidence * 10.0;
    score += match memory.memory_type.as_str() {
        "decision" => 8.0,
        "product_goal" | "constraint" => 7.0,
        "user_preference" | "known_issue" => 6.0,
        "design_note" | "domain_fact" => 5.0,
        "command" | "task_state" => 4.0,
        _ => 2.0,
    };
    score += match memory.status.as_str() {
        "active" => 5.0,
        "uncertain" => 1.0,
        _ => -10.0,
    };
    if let Some(scope) = requested_scope {
        if memory.scope == scope {
            score += 4.0;
        }
    } else {
        score += match memory.scope.as_str() {
            "project" | "repo" => 3.0,
            "user" | "global" => 2.0,
            "thread" | "task" => 1.0,
            _ => 0.0,
        };
    }
    let haystack = tokenize(&format!("{} {}", memory.title, memory.body));
    let overlap = task_terms.intersection(&haystack).count() as f64;
    score += overlap * 2.0;
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    score -= (age_days / 30.0).min(3.0);
    score
}

pub(crate) fn render_context_pack(
    conn: &Connection,
    rows: &[Memory],
    max_chars: usize,
) -> Result<String> {
    render_context_pack_with_terms(conn, rows, max_chars, &HashSet::new())
}

pub(crate) fn render_context_pack_for_task(
    conn: &Connection,
    rows: &[Memory],
    max_chars: usize,
    task: &str,
) -> Result<String> {
    let query_terms = relevance_terms(task);
    render_context_pack_with_terms(conn, rows, max_chars, &query_terms)
}

fn render_context_pack_with_terms(
    conn: &Connection,
    rows: &[Memory],
    max_chars: usize,
    query_terms: &HashSet<String>,
) -> Result<String> {
    if rows.is_empty() {
        return Ok("Relevant Memory:\n- none".to_string());
    }
    let mut out = String::from("Relevant Memory:");
    for (title, group) in grouped_memories(rows) {
        let heading = format!("\n\n{title}:");
        if out.len() + heading.len() > max_chars {
            break;
        }
        out.push_str(&heading);
        for row in group {
            let card = format_context_card(conn, row, query_terms, max_chars)?;
            if out.len() + card.len() + 1 > max_chars {
                return Ok(out);
            }
            out.push('\n');
            out.push_str(&card);
        }
    }
    Ok(out)
}

fn format_context_card(
    conn: &Connection,
    row: &Memory,
    query_terms: &HashSet<String>,
    max_chars: usize,
) -> Result<String> {
    let body = query_focused_summary(&row.body, query_terms, retrieval_body_char_limit(max_chars));
    let links = get_links(conn, &row.id)?;
    let link_text = if links.is_empty() {
        String::new()
    } else {
        let rendered = links
            .iter()
            .map(|link| format!("{}:{}", link.kind, link.target))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ({rendered})")
    };
    Ok(format!(
        "- {}:{} [{}] {} -- {}{}",
        row.memory_type, row.status, row.scope, row.title, body, link_text
    ))
}

pub(crate) struct SemanticContextRequest<'a> {
    pub(crate) task: &'a str,
    pub(crate) limit: usize,
    pub(crate) budget: usize,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) rules: Option<&'a Path>,
}

pub(crate) fn append_semantic_context_rows(
    conn: &Connection,
    rows: &mut Vec<Memory>,
    request: SemanticContextRequest<'_>,
) -> Result<bool> {
    let task_terms = relevance_terms(request.task);
    if task_terms.len() < 2
        || !embeddings::semantic_index_ready(
            conn,
            request.provider,
            request.endpoint,
            request.model,
        )
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let threshold = semantic_score_threshold(request.budget);
    let max_additions = semantic_context_add_limit(request.limit, request.budget);
    let mut added = 0;
    for item in embeddings::semantic_search(
        conn,
        request.provider,
        request.endpoint,
        request.model,
        request.task,
        request.limit.saturating_mul(2).max(request.limit),
    )? {
        if item.score < threshold {
            continue;
        }
        let memory = item.memory.memory;
        if !matches!(memory.status.as_str(), "active" | "uncertain") {
            continue;
        }
        if rows.iter().any(|existing| existing.id == memory.id) {
            continue;
        }
        rows.push(memory);
        added += 1;
        if added >= max_additions {
            break;
        }
    }
    if added > 0 {
        rank_context_rows(rows, request.task, None, request.rules);
        rows.truncate(request.limit);
    }
    Ok(added > 0)
}

fn semantic_context_add_limit(limit: usize, budget: usize) -> usize {
    let budget_limit = if budget <= 1_200 {
        1
    } else if budget <= 2_500 {
        2
    } else {
        4
    };
    limit.min(budget_limit).max(1)
}

fn render_retrieval_pack(
    hits: &[RetrievalHit],
    max_chars: usize,
    query: &str,
    semantic_skip_reason: Option<&str>,
) -> Result<String> {
    if hits.is_empty() {
        if let Some(reason) = semantic_skip_reason {
            return Ok(format!(
                "Relevant Memory:\n- none ({}; semantic search skipped)",
                semantic_skip_label(reason)
            ));
        }
        return Ok("Relevant Memory:\n- none".to_string());
    }
    let query_terms = relevance_terms(query);
    let mut rows = hits
        .iter()
        .map(|hit| &hit.memory.memory)
        .collect::<Vec<_>>();
    rows.sort_by_key(|a| memory_group_order(a));
    let mut out = String::from("Relevant Memory:");
    let mut current_group = "";
    for row in rows {
        let group = memory_group_title(row);
        if group != current_group {
            let heading = format!("\n\n{group}:");
            if out.len() + heading.len() > max_chars {
                break;
            }
            out.push_str(&heading);
            current_group = group;
        }
        let body = query_focused_body(row, &query_terms, max_chars);
        let card = format!(
            "- {}:{} [{}] {} -- {}",
            row.memory_type, row.status, row.scope, row.title, body
        );
        if out.len() + card.len() + 1 > max_chars {
            break;
        }
        out.push('\n');
        out.push_str(&card);
    }
    Ok(out)
}

pub(crate) fn semantic_skip_label(reason: &str) -> &'static str {
    match reason {
        "generic_query" => "generic query",
        "weak_query" => "weak query",
        "lexical_saturated" => "lexical matches saturated",
        _ => "query",
    }
}

fn query_focused_body(memory: &Memory, query_terms: &HashSet<String>, max_chars: usize) -> String {
    query_focused_summary(
        &memory.body,
        query_terms,
        retrieval_body_char_limit(max_chars),
    )
}

fn retrieval_body_char_limit(max_chars: usize) -> usize {
    if max_chars <= 1_200 {
        180
    } else if max_chars <= 2_500 {
        260
    } else if max_chars <= 8_000 {
        520
    } else {
        800
    }
}

pub(crate) fn query_focused_summary(
    text: &str,
    query_terms: &HashSet<String>,
    max_chars: usize,
) -> String {
    let body = one_line_summary(text);
    if body.chars().count() <= max_chars {
        return body;
    }
    if query_terms.is_empty() {
        return truncate_chars(&body, max_chars);
    }
    focused_text_window(&body, query_terms, max_chars)
        .unwrap_or_else(|| truncate_chars(&body, max_chars))
}

pub(crate) fn relevance_terms(text: &str) -> HashSet<String> {
    tokenize(text)
        .into_iter()
        .filter(|term| !is_generic_relevance_term(term))
        .collect()
}

fn is_generic_relevance_term(term: &str) -> bool {
    matches!(
        term,
        "agent"
            | "agents"
            | "brief"
            | "briefs"
            | "card"
            | "cards"
            | "context"
            | "contexts"
            | "dukememory"
            | "fast"
            | "faster"
            | "generic"
            | "local"
            | "memory"
            | "memories"
            | "minimal"
            | "optimization"
            | "optimize"
            | "project"
            | "projects"
            | "quality"
            | "recall"
            | "retrieval"
            | "retrieve"
            | "scoring"
            | "semantic"
            | "token"
            | "tokens"
    )
}

fn focused_text_window(
    text: &str,
    query_terms: &HashSet<String>,
    max_chars: usize,
) -> Option<String> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    let target_chars = max_chars.saturating_sub(6).max(20);
    let mut best: Option<(usize, usize, usize, usize, usize, usize)> = None;
    for start in 0..words.len() {
        let mut end = start;
        while end < words.len() && window_words_len(&words[start..end + 1]) <= target_chars {
            end += 1;
        }
        if end == start {
            end += 1;
        }
        let (distinct_matches, query_span, first_offset, total_matches) =
            query_window_score(&words[start..end], query_terms);
        if distinct_matches == 0 {
            continue;
        }
        let candidate = (
            distinct_matches,
            query_span,
            first_offset,
            total_matches,
            start,
            end,
        );
        let should_replace = best
            .map(|current| {
                distinct_matches > current.0
                    || (distinct_matches == current.0
                        && (query_span < current.1
                            || (query_span == current.1
                                && (first_offset < current.2
                                    || (first_offset == current.2
                                        && (total_matches > current.3
                                            || (total_matches == current.3
                                                && start < current.4)))))))
            })
            .unwrap_or(true);
        if should_replace {
            best = Some(candidate);
        }
    }
    let (_, _, _, _, start, end) = best?;
    let mut out = words[start..end].join(" ");
    if start > 0 {
        out = format!("...{out}");
    }
    if end < words.len() {
        out.push_str("...");
    }
    Some(truncate_chars(&out, max_chars))
}

fn query_window_score(
    words: &[&str],
    query_terms: &HashSet<String>,
) -> (usize, usize, usize, usize) {
    let mut distinct = HashSet::new();
    let mut total = 0;
    let mut first_match = None;
    let mut last_match = None;
    for (index, word) in words.iter().enumerate() {
        for token in tokenize(word) {
            if query_terms.contains(&token) {
                distinct.insert(token);
                total += 1;
                first_match.get_or_insert(index);
                last_match = Some(index);
            }
        }
    }
    let query_span = match (first_match, last_match) {
        (Some(first), Some(last)) => last.saturating_sub(first),
        _ => usize::MAX,
    };
    (
        distinct.len(),
        query_span,
        first_match.unwrap_or(usize::MAX),
        total,
    )
}

fn window_words_len(words: &[&str]) -> usize {
    words.iter().map(|word| word.len()).sum::<usize>() + words.len().saturating_sub(1)
}

fn grouped_memories(rows: &[Memory]) -> Vec<(&'static str, Vec<&Memory>)> {
    let mut groups: Vec<(&'static str, Vec<&Memory>)> = vec![
        ("Decisions", Vec::new()),
        ("Constraints", Vec::new()),
        ("Current Facts", Vec::new()),
        ("Risks", Vec::new()),
        ("Recent Work", Vec::new()),
        ("Other", Vec::new()),
    ];
    for row in rows {
        let index = memory_group_order(row);
        groups[index].1.push(row);
    }
    groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

fn memory_group_order(memory: &Memory) -> usize {
    match memory.memory_type.as_str() {
        "decision" => 0,
        "constraint" | "product_goal" | "user_preference" => 1,
        "domain_fact" | "design_note" | "command" => 2,
        "known_issue" => 3,
        "task_state" => 4,
        _ => 5,
    }
}

fn memory_group_title(memory: &Memory) -> &'static str {
    match memory_group_order(memory) {
        0 => "Decisions",
        1 => "Constraints",
        2 => "Current Facts",
        3 => "Risks",
        4 => "Recent Work",
        _ => "Other",
    }
}

pub(crate) struct AgentContextRequest<'a> {
    pub(crate) task: &'a str,
    pub(crate) mode: ContextMode,
    pub(crate) limit: usize,
    pub(crate) max_chars: usize,
    pub(crate) json_out: bool,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) format: OutputFormat,
    pub(crate) rules: Option<&'a Path>,
}

pub(crate) fn print_agent_context(
    conn: &Connection,
    request: AgentContextRequest<'_>,
) -> Result<()> {
    let statuses = ["active".to_string(), "uncertain".to_string()];
    let include_recent = match request.mode {
        ContextMode::Fast => 2,
        ContextMode::Agent => 4,
        ContextMode::Deep => 8,
    };
    let mut rows = build_context_rows(
        conn,
        ContextQuery {
            task: request.task,
            types: &[],
            statuses: &statuses,
            scope: None,
            limit: request.limit,
            include_recent,
            rules: request.rules,
        },
    )?;
    if !matches!(request.mode, ContextMode::Fast) {
        append_semantic_context_rows(
            conn,
            &mut rows,
            SemanticContextRequest {
                task: request.task,
                limit: request.limit,
                budget: request.max_chars,
                provider: request.provider,
                endpoint: request.endpoint,
                model: request.model,
                rules: request.rules,
            },
        )?;
    }
    if request.json_out || matches!(request.format, OutputFormat::Json) {
        let full = rows
            .iter()
            .map(|m| get_memory_with_links(conn, &m.id))
            .collect::<Result<Vec<_>>>()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": format!("{:?}", request.mode).to_lowercase(),
                "task": request.task,
                "memories": full
            }))?
        );
        return Ok(());
    }
    if matches!(request.format, OutputFormat::Markdown | OutputFormat::Agent) {
        return print_memory_output(
            conn,
            &rows,
            request.format,
            request.max_chars,
            "Agent Context",
        );
    }
    let mut out = String::from("Agent Context\n");
    out.push_str(&render_context_pack_for_task(
        conn,
        &rows,
        request.max_chars,
        request.task,
    )?);
    if matches!(request.mode, ContextMode::Agent | ContextMode::Deep) {
        append_relevant_next_actions(conn, &mut out, request.task, request.max_chars)?;
    }
    if matches!(request.mode, ContextMode::Deep) {
        out.push_str(&render_codegraph_hints(&rows, request.task, Path::new(".")));
    }
    let out = truncate_chars(&out, request.max_chars);
    println!("{out}");
    Ok(())
}

fn append_relevant_next_actions(
    conn: &Connection,
    out: &mut String,
    task: &str,
    max_chars: usize,
) -> Result<()> {
    let task_terms = relevance_terms(task);
    if task_terms.len() < 2 {
        return Ok(());
    }
    let max_actions = next_action_limit(max_chars);
    let mut actions = Vec::new();
    for row in query_memories(
        conn,
        None,
        &["task_state".to_string()],
        &["active".to_string()],
        None,
        12,
    )? {
        if context_recent_matches_task(conn, &row, &task_terms)? {
            actions.push(row.title);
            if actions.len() >= max_actions {
                break;
            }
        }
    }
    if actions.is_empty() || !push_agent_context_line(out, max_chars, "\nNext Actions:") {
        return Ok(());
    }
    for action in actions {
        if !push_agent_context_line(out, max_chars, &format!("- {action}")) {
            break;
        }
    }
    Ok(())
}

fn next_action_limit(max_chars: usize) -> usize {
    if max_chars <= 1_200 {
        1
    } else if max_chars <= 2_500 {
        2
    } else {
        5
    }
}

fn push_agent_context_line(out: &mut String, max_chars: usize, line: &str) -> bool {
    let needed = line.len() + 1;
    if out.len() + needed <= max_chars {
        out.push_str(line);
        out.push('\n');
        true
    } else {
        false
    }
}

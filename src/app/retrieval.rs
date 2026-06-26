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
    let mut rows = query_memories(
        conn,
        Some(query.task),
        query.types,
        query.statuses,
        query.scope,
        query.limit,
    )?;
    if query.include_recent > 0 {
        let recent = query_memories(
            conn,
            None,
            query.types,
            &["active".to_string()],
            query.scope,
            query.include_recent,
        )?;
        for row in recent {
            if !rows.iter().any(|existing| existing.id == row.id) {
                rows.push(row);
            }
        }
    }
    rank_context_rows(&mut rows, query.task, query.scope, query.rules);
    rows.truncate(query.limit);
    Ok(rows)
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
            println!("{}", render_retrieval_pack(&report.hits, request.budget)?);
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
            println!("{}", render_retrieval_pack(&report.hits, request.budget)?);
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
    let mut candidates: HashMap<String, (Memory, Option<f64>)> = HashMap::new();
    for row in query_memories(
        conn,
        Some(request.query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        request.scope,
        request.limit.saturating_mul(2).max(request.limit),
    )? {
        candidates.entry(row.id.clone()).or_insert((row, None));
    }
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
    let mut semantic_used = false;
    let mut semantic_error = None;
    if matches!(request.strategy, RetrievalStrategy::Hybrid) {
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
                        semantic_used = !semantic.is_empty();
                        for item in semantic {
                            let memory = item.memory.memory;
                            candidates
                                .entry(memory.id.clone())
                                .and_modify(|existing| existing.1 = Some(item.score))
                                .or_insert((memory, Some(item.score)));
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
    let task_terms = tokenize(request.query);
    let mut hits = Vec::new();
    for (_, (memory, semantic_score)) in candidates {
        if !rhai_should_include(rhai.as_ref(), &memory, request.query)? {
            continue;
        }
        let links = get_links(conn, &memory.id)?;
        let (score, reasons) = retrieval_score(
            &memory,
            &links,
            &task_terms,
            request.scope,
            semantic_score,
            rhai.as_ref(),
            request.query,
        );
        let utility_score = memory_utility_score(&memory, links.len());
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
    hits.truncate(request.limit);
    let ids = hits
        .iter()
        .map(|hit| hit.memory.memory.id.clone())
        .collect::<Vec<_>>();
    let receipt = memory_receipt("retrieve", Some(semantic_used), &ids, "none");
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
        semantic_error,
        receipt,
        hits,
    })
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
    for hit in &retrieval.hits {
        let memory = &hit.memory.memory;
        raw_chars += memory.body.chars().count();
        items.push(RecallItem {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            summary: truncate_chars(&one_line_summary(&memory.body), 120),
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

fn retrieval_score(
    memory: &Memory,
    links: &[MemoryLink],
    task_terms: &HashSet<String>,
    requested_scope: Option<&str>,
    semantic_score: Option<f64>,
    rules: Option<&RhaiRules>,
    task: &str,
) -> (f64, Vec<String>) {
    let mut score = context_score(memory, task_terms, requested_scope);
    let mut reasons = Vec::new();
    reasons.push(format!("type:{}", memory.memory_type));
    reasons.push(format!("status:{}", memory.status));
    if memory.confidence >= 0.8 {
        reasons.push("high_confidence".to_string());
    } else if memory.confidence < 0.5 {
        reasons.push("low_confidence".to_string());
    }
    if let Some(scope) = requested_scope
        && memory.scope == scope
    {
        reasons.push("scope_match".to_string());
    }
    let haystack = tokenize(&format!("{} {}", memory.title, memory.body));
    let overlap = task_terms.intersection(&haystack).count();
    if overlap > 0 {
        reasons.push(format!("text_match:{overlap}"));
        score += overlap as f64;
    }
    let link_overlap = links
        .iter()
        .map(|link| tokenize(&format!("{} {}", link.kind, link.target)))
        .map(|tokens| task_terms.intersection(&tokens).count())
        .sum::<usize>();
    if link_overlap > 0 {
        reasons.push(format!("link_match:{link_overlap}"));
        score += link_overlap as f64 * 3.0;
    }
    if let Some(value) = semantic_score {
        reasons.push(format!("semantic:{value:.3}"));
        score += value * 12.0;
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
    let rhai = rhai_score(rules, memory, task).unwrap_or(0.0);
    if rhai != 0.0 {
        reasons.push(format!("rhai_score:{rhai:.2}"));
        score += rhai;
    }
    (score, reasons)
}

fn memory_utility_score(memory: &Memory, link_count: usize) -> f64 {
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
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    score - (age_days / 14.0).min(4.0)
}

pub(crate) fn rank_context_rows(
    rows: &mut [Memory],
    task: &str,
    requested_scope: Option<&str>,
    rules: Option<&Path>,
) {
    let task_terms = tokenize(task);
    let rhai = rules.and_then(|path| load_rhai_rules(path).ok());
    rows.sort_by(|a, b| {
        let a_score = context_score(a, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), a, task).unwrap_or(0.0);
        let b_score = context_score(b, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), b, task).unwrap_or(0.0);
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
            let card = format_compact_card(conn, row)?;
            if out.len() + card.len() + 1 > max_chars {
                return Ok(out);
            }
            out.push('\n');
            out.push_str(&card);
        }
    }
    Ok(out)
}

fn render_retrieval_pack(hits: &[RetrievalHit], max_chars: usize) -> Result<String> {
    if hits.is_empty() {
        return Ok("Relevant Memory:\n- none".to_string());
    }
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
        let body = row.body.split_whitespace().collect::<Vec<_>>().join(" ");
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
    if !matches!(request.mode, ContextMode::Fast)
        && embeddings::semantic_index_ready(conn, request.provider, request.endpoint, request.model)
            .unwrap_or(false)
        && let Ok(semantic_rows) = embeddings::semantic_search(
            conn,
            request.provider,
            request.endpoint,
            request.model,
            request.task,
            request.limit,
        )
    {
        for item in semantic_rows {
            if !rows
                .iter()
                .any(|existing| existing.id == item.memory.memory.id)
            {
                rows.push(item.memory.memory);
            }
        }
        rank_context_rows(&mut rows, request.task, None, request.rules);
        rows.truncate(request.limit);
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
    out.push_str(&render_context_pack(conn, &rows, request.max_chars)?);
    if matches!(request.mode, ContextMode::Agent | ContextMode::Deep) {
        out.push_str("\n\nNext Actions:\n");
        for row in query_memories(
            conn,
            None,
            &["task_state".to_string()],
            &["active".to_string()],
            None,
            5,
        )? {
            out.push_str("- ");
            out.push_str(&row.title);
            out.push('\n');
        }
    }
    if matches!(request.mode, ContextMode::Deep) {
        out.push_str(&render_codegraph_hints(&rows, request.task, Path::new(".")));
    }
    println!("{out}");
    Ok(())
}

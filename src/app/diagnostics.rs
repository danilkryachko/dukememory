use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LiveEvalReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) reads: usize,
    pub(crate) feedback_events: usize,
    pub(crate) useful: usize,
    pub(crate) useless: usize,
    pub(crate) missing: usize,
    pub(crate) useful_rate: f64,
    pub(crate) useful_rate_source: String,
    pub(crate) feedback_useful_rate: f64,
    pub(crate) inferred_useful: usize,
    pub(crate) inferred_total: usize,
    pub(crate) inferred_useful_rate: f64,
    pub(crate) inferred_missing: usize,
    pub(crate) noisy_memory_ids: Vec<String>,
    pub(crate) missing_queries: Vec<String>,
    pub(crate) inferred_missing_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InferredFeedbackReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) scanned: usize,
    pub(crate) written: usize,
    pub(crate) useful: usize,
    pub(crate) missing: usize,
    pub(crate) skipped: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctrineReport {
    active: Vec<DoctrineDecision>,
    superseded: Vec<DoctrineDecision>,
    conflicts: Vec<MergeCandidate>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctrineDecision {
    id: String,
    title: String,
    scope: String,
    status: String,
    confidence: f64,
    body: String,
    supersedes: Option<String>,
    superseded_by: Option<String>,
    chain: Vec<String>,
}

pub(crate) struct BriefRequest<'a> {
    pub(crate) task: &'a str,
    pub(crate) limit: usize,
    pub(crate) budget: usize,
    pub(crate) scope: Option<&'a str>,
    pub(crate) rules: Option<&'a Path>,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) json_out: bool,
}

pub(crate) struct ImpactRequest<'a> {
    pub(crate) target: &'a str,
    pub(crate) limit: usize,
    pub(crate) budget: usize,
    pub(crate) scope: Option<&'a str>,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) json_out: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SecretFinding {
    id: String,
    title: String,
    pattern: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorFinding {
    kind: String,
    status: String,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CodexDoctorReport {
    ok: bool,
    findings: Vec<DoctorFinding>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReviewIssue {
    pub(crate) kind: String,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvalResult {
    id: String,
    name: String,
    passed: bool,
    detail: String,
}

pub(crate) fn print_doctrine(conn: &Connection, scope: Option<&str>, json_out: bool) -> Result<()> {
    let report = doctrine_report(conn, scope)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Decision Doctrine");
    println!("Active Decisions:");
    if report.active.is_empty() {
        println!("- none");
    } else {
        for item in &report.active {
            println!(
                "- {}  {}  scope={} confidence={:.2}",
                item.id, item.title, item.scope, item.confidence
            );
            if let Some(line) = first_line(&item.body) {
                println!("  {line}");
            }
            if !item.chain.is_empty() {
                println!("  supersedes: {}", item.chain.join(" -> "));
            }
        }
    }
    println!("Superseded Decisions:");
    if report.superseded.is_empty() {
        println!("- none");
    } else {
        for item in &report.superseded {
            let target = item.superseded_by.as_deref().unwrap_or("unknown");
            println!("- {} -> {}  {}", item.id, target, item.title);
        }
    }
    println!("Potential Conflicts:");
    if report.conflicts.is_empty() {
        println!("- none");
    } else {
        for item in &report.conflicts {
            println!(
                "- {} <> {}  {}  {}",
                item.primary_id, item.duplicate_id, item.title, item.reason
            );
        }
    }
    Ok(())
}

pub(crate) fn doctrine_report(conn: &Connection, scope: Option<&str>) -> Result<DoctrineReport> {
    let active = query_memories(
        conn,
        None,
        &["decision".to_string()],
        &["active".to_string()],
        scope,
        usize::MAX,
    )?;
    let superseded = query_memories(
        conn,
        None,
        &["decision".to_string()],
        &["superseded".to_string()],
        scope,
        usize::MAX,
    )?;
    let active_ids = active
        .iter()
        .map(|row| row.id.clone())
        .collect::<HashSet<_>>();
    let conflicts = merge_candidates(conn, usize::MAX)?
        .into_iter()
        .filter(|item| {
            active_ids.contains(&item.primary_id) && active_ids.contains(&item.duplicate_id)
        })
        .collect::<Vec<_>>();
    let active = active
        .into_iter()
        .map(|row| doctrine_decision(conn, row))
        .collect::<Result<Vec<_>>>()?;
    let superseded = superseded
        .into_iter()
        .map(|row| doctrine_decision(conn, row))
        .collect::<Result<Vec<_>>>()?;
    Ok(DoctrineReport {
        active,
        superseded,
        conflicts,
    })
}

fn doctrine_decision(conn: &Connection, row: Memory) -> Result<DoctrineDecision> {
    let chain = decision_supersedes_chain(conn, row.supersedes.as_deref())?;
    Ok(DoctrineDecision {
        id: row.id,
        title: row.title,
        scope: row.scope,
        status: row.status,
        confidence: row.confidence,
        body: row.body,
        supersedes: row.supersedes,
        superseded_by: row.superseded_by,
        chain,
    })
}

fn decision_supersedes_chain(conn: &Connection, start: Option<&str>) -> Result<Vec<String>> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = start.map(str::to_string);
    while let Some(id) = current {
        if !seen.insert(id.clone()) {
            chain.push(format!("{id} (cycle)"));
            break;
        }
        let memory = get_memory(conn, &id)?;
        chain.push(format!("{} {}", memory.id, memory.title));
        current = memory.supersedes;
    }
    Ok(chain)
}

pub(crate) fn print_brief(conn: &Connection, request: BriefRequest<'_>) -> Result<()> {
    let report = brief_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_brief(&report));
    }
    Ok(())
}

pub(crate) fn brief_report(conn: &Connection, request: &BriefRequest<'_>) -> Result<BriefReport> {
    let started = Instant::now();
    let retrieval = retrieve_report(
        conn,
        &RetrieveRequest {
            query: request.task,
            strategy: RetrievalStrategy::Hybrid,
            format: OutputFormat::Plain,
            limit: request.limit.max(brief_retrieval_floor(request.budget)),
            budget: request.budget,
            scope: request.scope,
            rules: request.rules,
            provider: request.provider,
            endpoint: request.endpoint,
            model: request.model,
            audit_read: false,
        },
    )?;
    let mut must_follow = Vec::new();
    let mut relevant = Vec::new();
    let mut risks = Vec::new();
    let mut files = Vec::new();
    let mut checks = Vec::new();
    let mut seen_items = HashSet::new();
    let mut seen_files = HashSet::new();
    let mut seen_checks = HashSet::new();
    let task_terms = relevance_terms(request.task);
    let (file_limit, check_limit) = brief_artifact_limits(request.budget, task_terms.len());
    let section_limits = brief_section_limits(request.budget);

    for hit in &retrieval.hits {
        let memory = &hit.memory.memory;
        let item = brief_item_from_hit(hit, request.budget, &task_terms);
        match memory.memory_type.as_str() {
            "decision" | "constraint" | "product_goal" => {
                push_unique_brief_item(
                    &mut must_follow,
                    &mut seen_items,
                    item,
                    section_limits.must_follow,
                );
            }
            "known_issue" => {
                push_unique_brief_item(&mut risks, &mut seen_items, item, section_limits.risks);
            }
            "command" => {
                push_unique_check(&mut checks, &mut seen_checks, &memory.body, check_limit);
                push_unique_brief_item(
                    &mut relevant,
                    &mut seen_items,
                    item,
                    section_limits.relevant,
                );
            }
            _ => {
                push_unique_brief_item(
                    &mut relevant,
                    &mut seen_items,
                    item,
                    section_limits.relevant,
                );
            }
        }
        for link in &hit.memory.links {
            if matches!(link.kind.as_str(), "file" | "symbol") {
                let rendered = format!("{}:{}", link.kind, link.target);
                if files.len() < file_limit && seen_files.insert(rendered.clone()) {
                    files.push(rendered);
                }
            }
        }
        collect_check_hints(&mut checks, &mut seen_checks, &memory.body, check_limit);
    }

    let mut ids = must_follow
        .iter()
        .chain(relevant.iter())
        .chain(risks.iter())
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let semantic_status = if retrieval.semantic_skipped {
        MemorySemanticStatus::Skipped
    } else if retrieval.semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    let receipt = memory_receipt_with_semantic("brief", semantic_status, &ids, "none");
    log_read_event(
        conn,
        ReadEventInput {
            command: "brief",
            query: request.task,
            ids: &ids,
            semantic_used: retrieval.semantic_used,
            result_count: ids.len(),
            budget: request.budget,
            elapsed_ms: started.elapsed().as_millis(),
        },
    )?;

    Ok(BriefReport {
        version: 1,
        task: request.task.to_string(),
        budget: request.budget,
        semantic_used: retrieval.semantic_used,
        semantic_skipped: retrieval.semantic_skipped,
        semantic_skip_reason: retrieval.semantic_skip_reason,
        semantic_error: retrieval.semantic_error,
        receipt,
        must_follow,
        relevant,
        risks,
        files,
        checks,
    })
}

#[derive(Clone, Copy)]
struct BriefSectionLimits {
    must_follow: usize,
    relevant: usize,
    risks: usize,
}

fn brief_retrieval_floor(budget: usize) -> usize {
    if budget <= 1_200 { 4 } else { 6 }
}

fn brief_section_limits(budget: usize) -> BriefSectionLimits {
    if budget <= 1_200 {
        BriefSectionLimits {
            must_follow: 3,
            relevant: 3,
            risks: 2,
        }
    } else if budget <= 3_000 {
        BriefSectionLimits {
            must_follow: 5,
            relevant: 5,
            risks: 3,
        }
    } else {
        BriefSectionLimits {
            must_follow: 8,
            relevant: 8,
            risks: 5,
        }
    }
}

fn brief_artifact_limits(budget: usize, relevance_term_count: usize) -> (usize, usize) {
    if budget <= 1_200 {
        match relevance_term_count {
            0 => (0, 0),
            1 => (2, 1),
            _ => (4, 2),
        }
    } else if budget <= 3_000 {
        match relevance_term_count {
            0 => (0, 0),
            1 => (3, 2),
            _ => (6, 3),
        }
    } else {
        match relevance_term_count {
            0 => (0, 0),
            1 => (3, 2),
            _ => (8, 5),
        }
    }
}

fn brief_item_from_hit(
    hit: &RetrievalHit,
    budget: usize,
    query_terms: &HashSet<String>,
) -> BriefItem {
    let memory = &hit.memory.memory;
    brief_item_from_memory(
        memory,
        hit.score,
        hit.reasons.iter().take(4).cloned().collect(),
        query_terms,
        budget,
    )
}

fn brief_item_from_memory(
    memory: &Memory,
    score: f64,
    reasons: Vec<String>,
    query_terms: &HashSet<String>,
    budget: usize,
) -> BriefItem {
    BriefItem {
        id: memory.id.clone(),
        memory_type: memory.memory_type.clone(),
        title: memory.title.clone(),
        summary: query_focused_summary(&memory.body, query_terms, brief_summary_limit(budget)),
        score,
        reasons,
    }
}

fn brief_summary_limit(budget: usize) -> usize {
    if budget <= 1_200 {
        120
    } else if budget <= 3_000 {
        150
    } else {
        180
    }
}

fn push_unique_brief_item(
    items: &mut Vec<BriefItem>,
    seen: &mut HashSet<String>,
    item: BriefItem,
    limit: usize,
) {
    if items.len() < limit && seen.insert(item.id.clone()) {
        items.push(item);
    }
}

fn push_unique_check(
    checks: &mut Vec<String>,
    seen: &mut HashSet<String>,
    value: &str,
    limit: usize,
) {
    let check = truncate_chars(&one_line_summary(value), 140);
    if !check.is_empty() && checks.len() < limit && seen.insert(check.clone()) {
        checks.push(check);
    }
}

fn collect_check_hints(
    checks: &mut Vec<String>,
    seen: &mut HashSet<String>,
    text: &str,
    limit: usize,
) {
    for line in text.lines().map(str::trim) {
        let lower = line.to_lowercase();
        if lower.contains("cargo test")
            || lower.contains("npm test")
            || lower.contains("pytest")
            || lower.contains("pnpm test")
            || lower.contains("run test")
        {
            push_unique_check(checks, seen, line, limit);
        }
    }
}

fn render_brief(report: &BriefReport) -> String {
    let mut out = format!("Brief: {}\n", report.task);
    if report.semantic_used {
        push_line_budget(&mut out, report.budget, "Semantic: used");
    } else if let Some(error) = &report.semantic_error {
        push_line_budget(
            &mut out,
            report.budget,
            &format!("Semantic: fallback ({})", truncate_chars(error, 90)),
        );
    }
    push_line_budget(&mut out, report.budget, &report.receipt);
    if report.semantic_skipped && brief_is_empty(report) {
        let reason = report
            .semantic_skip_reason
            .as_deref()
            .map(semantic_skip_label)
            .unwrap_or("query");
        push_line_budget(
            &mut out,
            report.budget,
            &format!("Relevant: none ({reason}; semantic search skipped)"),
        );
    }
    render_brief_items(&mut out, report.budget, "Must Follow", &report.must_follow);
    render_brief_items(&mut out, report.budget, "Relevant", &report.relevant);
    render_brief_items(&mut out, report.budget, "Risks", &report.risks);
    render_brief_strings(&mut out, report.budget, "Files", &report.files);
    render_brief_strings(&mut out, report.budget, "Checks", &report.checks);
    truncate_chars(&out, report.budget)
}

fn brief_is_empty(report: &BriefReport) -> bool {
    report.must_follow.is_empty()
        && report.relevant.is_empty()
        && report.risks.is_empty()
        && report.files.is_empty()
        && report.checks.is_empty()
}

fn linked_memories(
    conn: &Connection,
    target: &str,
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<Memory>> {
    let mut sql = String::from(
        "SELECT DISTINCT m.* FROM memories m \
         JOIN memory_links l ON l.memory_id = m.id \
         WHERE (l.target = ? OR l.target LIKE ?) \
         AND m.status IN ('active', 'uncertain')",
    );
    let mut values = vec![target.to_string(), format!("%{target}%")];
    if let Some(scope) = scope {
        sql.push_str(" AND m.scope = ?");
        values.push(scope.to_string());
    }
    sql.push_str(" ORDER BY m.confidence DESC, m.updated_at DESC LIMIT ?");
    values.push(limit.min(i64::MAX as usize).to_string());
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), row_to_memory)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn memory_links_target(conn: &Connection, memory_id: &str, target: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_links WHERE memory_id = ?1 AND (target = ?2 OR target LIKE ?3)",
        params![memory_id, target, format!("%{target}%")],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn render_impact(report: &ImpactReport) -> String {
    let mut out = format!("Impact: {}\n", report.target);
    push_line_budget(&mut out, report.budget, &report.receipt);
    render_brief_items(&mut out, report.budget, "Decisions", &report.decisions);
    render_brief_items(&mut out, report.budget, "Constraints", &report.constraints);
    render_brief_items(&mut out, report.budget, "Risks", &report.risks);
    render_brief_items(&mut out, report.budget, "Related", &report.related);
    render_brief_strings(&mut out, report.budget, "Links", &report.links);
    render_brief_strings(&mut out, report.budget, "Checks", &report.checks);
    truncate_chars(&out, report.budget)
}

fn render_brief_items(out: &mut String, budget: usize, title: &str, items: &[BriefItem]) {
    if items.is_empty() {
        return;
    }
    if !push_line_budget(out, budget, &format!("\n{title}:")) {
        return;
    }
    for item in items {
        let line = format!(
            "- {} [{}] {} -- {}",
            item.id, item.memory_type, item.title, item.summary
        );
        if !push_line_budget(out, budget, &line) {
            return;
        }
    }
}

fn render_brief_strings(out: &mut String, budget: usize, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    if !push_line_budget(out, budget, &format!("\n{title}:")) {
        return;
    }
    for value in values {
        if !push_line_budget(out, budget, &format!("- {value}")) {
            return;
        }
    }
}

fn push_line_budget(out: &mut String, budget: usize, line: &str) -> bool {
    let needed = line.len() + 1;
    if out.len() + needed <= budget {
        out.push_str(line);
        out.push('\n');
        true
    } else {
        false
    }
}

pub(crate) fn print_impact(conn: &Connection, request: ImpactRequest<'_>) -> Result<()> {
    let report = impact_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_impact(&report));
    }
    Ok(())
}

pub(crate) fn impact_report(
    conn: &Connection,
    request: &ImpactRequest<'_>,
) -> Result<ImpactReport> {
    let started = Instant::now();
    let effective_limit = impact_effective_limit(request.limit, request.budget);
    let candidate_limit = impact_candidate_limit(request.limit, effective_limit, request.budget);
    let mut rows = linked_memories(conn, request.target, request.scope, candidate_limit)?;
    let fts_rows = query_memories(
        conn,
        Some(request.target),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        request.scope,
        candidate_limit,
    )?;
    for row in fts_rows {
        if !rows.iter().any(|existing| existing.id == row.id) {
            rows.push(row);
        }
    }
    let target_terms = relevance_terms(request.target);
    let semantic_used = append_semantic_impact_rows(
        conn,
        &mut rows,
        request,
        effective_limit,
        candidate_limit,
        &target_terms,
    )?;
    let quality_signals = retrieval_quality_signals(conn, 30).unwrap_or_default();
    rows = filter_query_useless_memories(rows, request.target, &quality_signals);
    let mut scored_rows = Vec::new();
    for memory in rows {
        let linked = memory_links_target(conn, &memory.id, request.target)?;
        let mut quality_reasons = Vec::new();
        let quality =
            retrieval_quality_adjustment(&memory.id, Some(&quality_signals), &mut quality_reasons);
        let type_score = match memory.memory_type.as_str() {
            "decision" | "constraint" | "product_goal" => 8.0,
            "known_issue" => 6.0,
            "command" | "task_state" => 4.0,
            _ => 2.0,
        };
        let lexical_score = impact_lexical_overlap_score(&memory, &target_terms);
        let score = if linked { 100.0 } else { 20.0 } + type_score + quality + lexical_score;
        scored_rows.push((memory, linked, quality_reasons, score));
    }
    scored_rows.sort_by(|a, b| {
        b.3.partial_cmp(&a.3)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.0.updated_at.cmp(&a.0.updated_at))
    });
    let scored_rows = scored_rows
        .into_iter()
        .take(effective_limit)
        .collect::<Vec<_>>();

    let mut decisions = Vec::new();
    let mut constraints = Vec::new();
    let mut risks = Vec::new();
    let mut checks = Vec::new();
    let mut related = Vec::new();
    let mut links = Vec::new();
    let mut seen_items = HashSet::new();
    let mut seen_checks = HashSet::new();
    let mut seen_links = HashSet::new();
    let section_limits = impact_section_limits(request.budget);

    for (memory, linked, quality_reasons, rank_score) in &scored_rows {
        let reason = if *linked {
            "linked_target"
        } else {
            "fts_match"
        };
        let mut reasons = vec![reason.to_string()];
        reasons.extend(quality_reasons.iter().cloned());
        let item =
            brief_item_from_memory(memory, *rank_score, reasons, &target_terms, request.budget);
        match memory.memory_type.as_str() {
            "decision" | "product_goal" => {
                push_unique_brief_item(
                    &mut decisions,
                    &mut seen_items,
                    item,
                    section_limits.decisions,
                );
            }
            "constraint" => {
                push_unique_brief_item(
                    &mut constraints,
                    &mut seen_items,
                    item,
                    section_limits.constraints,
                );
            }
            "known_issue" => {
                push_unique_brief_item(&mut risks, &mut seen_items, item, section_limits.risks);
            }
            "command" => {
                push_unique_check(
                    &mut checks,
                    &mut seen_checks,
                    &memory.body,
                    section_limits.checks,
                );
                push_unique_brief_item(&mut related, &mut seen_items, item, section_limits.related);
            }
            _ => {
                push_unique_brief_item(&mut related, &mut seen_items, item, section_limits.related);
            }
        }
        collect_check_hints(
            &mut checks,
            &mut seen_checks,
            &memory.body,
            section_limits.checks,
        );

        for link in get_links(conn, &memory.id)? {
            if matches!(link.kind.as_str(), "file" | "symbol") {
                let rendered = format!("{}:{}", link.kind, link.target);
                if links.len() < section_limits.links && seen_links.insert(rendered.clone()) {
                    links.push(rendered);
                }
            }
        }
    }

    let mut ids = decisions
        .iter()
        .chain(constraints.iter())
        .chain(risks.iter())
        .chain(related.iter())
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let semantic_status = if semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    let receipt = memory_receipt_with_semantic("impact", semantic_status, &ids, "none");
    log_read_event(
        conn,
        ReadEventInput {
            command: "impact",
            query: request.target,
            ids: &ids,
            semantic_used,
            result_count: ids.len(),
            budget: request.budget,
            elapsed_ms: started.elapsed().as_millis(),
        },
    )?;

    Ok(ImpactReport {
        version: 1,
        target: request.target.to_string(),
        budget: request.budget,
        semantic_used,
        receipt,
        decisions,
        constraints,
        risks,
        checks,
        related,
        links,
    })
}

fn append_semantic_impact_rows(
    conn: &Connection,
    rows: &mut Vec<Memory>,
    request: &ImpactRequest<'_>,
    effective_limit: usize,
    candidate_limit: usize,
    target_terms: &HashSet<String>,
) -> Result<bool> {
    if rows.len() >= effective_limit
        || target_terms.len() < 2
        || is_code_identifier_query(request.target)
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
    let max_additions =
        semantic_impact_add_limit(effective_limit.saturating_sub(rows.len()), request.budget);
    let mut added = 0;
    for item in embeddings::semantic_search(
        conn,
        request.provider,
        request.endpoint,
        request.model,
        request.target,
        semantic_impact_candidate_scan_limit(candidate_limit, request.budget),
    )? {
        if item.score < semantic_impact_score_threshold(request.budget) {
            continue;
        }
        if !semantic_impact_candidate_matches(
            &item.memory,
            target_terms,
            item.score,
            request.budget,
        ) {
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
        if rows.iter().any(|existing| existing.id == memory.id) {
            continue;
        }
        rows.push(memory);
        added += 1;
        if added >= max_additions {
            break;
        }
    }
    Ok(added > 0)
}

fn semantic_impact_candidate_scan_limit(candidate_limit: usize, budget: usize) -> usize {
    if budget <= 1_200 {
        candidate_limit.max(1).saturating_mul(2).min(12)
    } else if budget <= 3_000 {
        candidate_limit.max(1).saturating_mul(2).min(32)
    } else {
        candidate_limit.clamp(1, 64)
    }
}

fn semantic_impact_add_limit(remaining: usize, budget: usize) -> usize {
    let budget_limit = if budget <= 1_200 {
        1
    } else if budget <= 3_000 {
        2
    } else {
        4
    };
    remaining.min(budget_limit).max(1)
}

fn semantic_impact_score_threshold(budget: usize) -> f64 {
    if budget <= 1_200 {
        0.18
    } else if budget <= 3_000 {
        0.12
    } else {
        0.05
    }
}

fn semantic_impact_candidate_matches(
    item: &MemoryWithLinks,
    target_terms: &HashSet<String>,
    semantic_score: f64,
    budget: usize,
) -> bool {
    if budget > 1_200 || semantic_score >= 0.32 {
        return true;
    }
    let required_overlap = target_terms.len().min(2);
    let mut tokens = tokenize(&format!("{} {}", item.memory.title, item.memory.body));
    for link in &item.links {
        tokens.extend(tokenize(&format!("{} {}", link.kind, link.target)));
    }
    target_terms.intersection(&tokens).count() >= required_overlap
}

fn impact_lexical_overlap_score(memory: &Memory, terms: &HashSet<String>) -> f64 {
    if terms.is_empty() {
        return 0.0;
    }
    let haystack = format!("{} {}", memory.title, memory.body).to_lowercase();
    let overlap = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    (overlap.min(8) as f64) * 3.0
}

#[derive(Clone, Copy)]
struct ImpactSectionLimits {
    decisions: usize,
    constraints: usize,
    risks: usize,
    related: usize,
    checks: usize,
    links: usize,
}

fn impact_section_limits(budget: usize) -> ImpactSectionLimits {
    if budget <= 1_200 {
        ImpactSectionLimits {
            decisions: 2,
            constraints: 2,
            risks: 2,
            related: 2,
            checks: 3,
            links: 5,
        }
    } else if budget <= 3_000 {
        ImpactSectionLimits {
            decisions: 5,
            constraints: 5,
            risks: 5,
            related: 6,
            checks: 6,
            links: 10,
        }
    } else {
        ImpactSectionLimits {
            decisions: 8,
            constraints: 8,
            risks: 6,
            related: 8,
            checks: 8,
            links: 14,
        }
    }
}

fn impact_effective_limit(limit: usize, budget: usize) -> usize {
    let budget_limit = if budget <= 1_200 {
        8
    } else if budget <= 3_000 {
        24
    } else {
        limit
    };
    limit.min(budget_limit).max(1)
}

fn impact_candidate_limit(limit: usize, effective_limit: usize, budget: usize) -> usize {
    let requested_scan = limit.max(effective_limit).max(1);
    let budget_scan = if budget <= 1_200 {
        effective_limit.saturating_mul(2).max(effective_limit)
    } else if budget <= 3_000 {
        effective_limit
            .saturating_mul(2)
            .max(effective_limit)
            .min(48)
    } else {
        requested_scan
    };
    requested_scan.min(budget_scan).max(effective_limit).max(1)
}

pub(crate) fn print_evidence(conn: &Connection, id: &str, json_out: bool) -> Result<()> {
    let report = evidence_report(conn, id)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Evidence: {}", report.memory.memory.id);
    println!("{}", report.receipt);
    println!("title: {}", report.memory.memory.title);
    println!("type: {}", report.memory.memory.memory_type);
    println!("status: {}", report.memory.memory.status);
    if let Some(source) = &report.source {
        println!("source: {source}");
    }
    for link in &report.memory.links {
        println!("link: {}:{}", link.kind, link.target);
    }
    if !report.supersedes_chain.is_empty() {
        println!("supersedes: {}", report.supersedes_chain.join(" -> "));
    }
    if let Some(id) = &report.superseded_by {
        println!("superseded_by: {id}");
    }
    if report.audit_events.is_empty() {
        println!("audit: none");
    } else {
        println!("audit:");
        for event in &report.audit_events {
            println!("- {} {} {}", event.id, event.event_type, event.detail);
        }
    }
    Ok(())
}

pub(crate) fn evidence_report(conn: &Connection, id: &str) -> Result<EvidenceReport> {
    let started = Instant::now();
    let memory = get_memory_with_links(conn, id)?;
    let source = memory.memory.source.clone();
    let supersedes_chain = decision_supersedes_chain(conn, memory.memory.supersedes.as_deref())?;
    let superseded_by = memory.memory.superseded_by.clone();
    let audit_events = memory_events(conn, id, 20)?;
    let ids = vec![memory.memory.id.clone()];
    let receipt = memory_receipt("evidence", None, &ids, "none");
    log_read_event(
        conn,
        ReadEventInput {
            command: "evidence",
            query: id,
            ids: &ids,
            semantic_used: false,
            result_count: 1,
            budget: 0,
            elapsed_ms: started.elapsed().as_millis(),
        },
    )?;
    Ok(EvidenceReport {
        memory,
        source,
        supersedes_chain,
        superseded_by,
        audit_events,
        receipt,
    })
}

pub(crate) fn print_drift(
    conn: &Connection,
    root: &Path,
    changed_only: bool,
    json_out: bool,
) -> Result<()> {
    let report = drift_report(conn, root, changed_only)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!(
        "Drift: {}",
        if report.ok { "ok" } else { "needs_attention" }
    );
    if report.changed_only {
        if report.changed_files.is_empty() {
            println!("changed: none");
        } else {
            println!("changed:");
            for file in &report.changed_files {
                println!("- {file}");
            }
        }
    }
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    if !report.missing_links.is_empty() {
        println!("Missing Links:");
        for item in &report.missing_links {
            println!(
                "- {} {}:{} {}",
                item.memory_id, item.kind, item.target, item.detail
            );
        }
    }
    if !report.conflicts.is_empty() {
        println!("Potential Conflicts:");
        for item in &report.conflicts {
            println!(
                "- {} <> {} {}",
                item.primary_id, item.duplicate_id, item.reason
            );
        }
    }
    if !report.stale_active.is_empty() {
        println!("Stale Active:");
        for item in &report.stale_active {
            println!("- {} [{}] {}", item.id, item.memory_type, item.title);
        }
    }
    Ok(())
}

pub(crate) fn drift_report(
    conn: &Connection,
    root: &Path,
    changed_only: bool,
) -> Result<DriftReport> {
    let mut warnings = Vec::new();
    let changed_files = if changed_only {
        match git_changed_files(root) {
            Ok(files) => files,
            Err(err) => {
                warnings.push(err.to_string());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let changed_set = changed_files.iter().cloned().collect::<HashSet<_>>();
    let mut missing_links = link_report(conn, None, root, false)?
        .into_iter()
        .filter(|item| item.status == "missing")
        .filter(|item| {
            !changed_only
                || changed_set.contains(&item.target)
                || changed_set.contains(&normalize_git_path(&item.target))
        })
        .collect::<Vec<_>>();
    missing_links.truncate(20);

    let conflicts = merge_candidates(conn, 10)?;
    let empty_terms = HashSet::new();
    let stale_active = stale_active_memories(conn, 10)?
        .into_iter()
        .enumerate()
        .map(|(index, memory)| {
            brief_item_from_memory(
                &memory,
                100.0 - index as f64,
                vec!["active superseded_by".into()],
                &empty_terms,
                8_000,
            )
        })
        .collect::<Vec<_>>();
    let ok = missing_links.is_empty() && conflicts.is_empty() && stale_active.is_empty();

    Ok(DriftReport {
        version: 1,
        ok,
        changed_only,
        root: root.display().to_string(),
        changed_files,
        missing_links,
        conflicts,
        stale_active,
        warnings,
    })
}

fn git_changed_files(root: &Path) -> Result<Vec<String>> {
    if !root.join(".git").exists() {
        bail!("git metadata not found; changed-only drift needs a git worktree");
    }
    let mut files = HashSet::new();
    for args in [
        vec!["diff", "--name-only", "HEAD"],
        vec!["ls-files", "--others", "--exclude-standard"],
    ] {
        let output = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .context("failed to run git for changed-only drift")?;
        if !output.status.success() {
            bail!(
                "git changed file scan failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = normalize_git_path(line);
            if !path.is_empty() {
                files.insert(path);
            }
        }
    }
    let mut files = files.into_iter().collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn normalize_git_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn stale_active_memories(conn: &Connection, limit: usize) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM memories \
         WHERE status = 'active' AND superseded_by IS NOT NULL \
         ORDER BY updated_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit.min(i64::MAX as usize) as i64], row_to_memory)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(crate) fn handle_eval(conn: &Connection, command: EvalCommand) -> Result<()> {
    match command {
        EvalCommand::AddCase {
            name,
            query,
            expected,
            budget,
        } => {
            let id = Uuid::new_v4().simple().to_string()[..12].to_string();
            conn.execute(
                "INSERT INTO eval_cases (id, name, query, expected, budget, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, name, query, expected, budget as i64, now_ms()],
            )?;
            println!("{id}");
        }
        EvalCommand::Run { json } => run_eval(conn, json)?,
        EvalCommand::Live { since_days, json } => print_live_eval(conn, since_days, json)?,
    }
    Ok(())
}

fn run_eval(conn: &Connection, json_out: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id, name, query, expected, budget FROM eval_cases ORDER BY created_at ASC",
    )?;
    let cases = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut results = Vec::new();
    for case in cases {
        let (id, name, query, expected, _budget) = case?;
        let rows = retrieve_rows(
            conn,
            &query,
            RetrievalStrategy::Fts,
            12,
            DEFAULT_EMBED_PROVIDER,
            DEFAULT_EMBED_ENDPOINT,
            DEFAULT_EMBED_MODEL,
        )?;
        let haystack = rows
            .iter()
            .map(|row| format!("{} {} {}", row.id, row.title, row.body))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        let passed = haystack.contains(&expected.to_lowercase());
        results.push(EvalResult {
            id,
            name,
            passed,
            detail: if passed {
                "expected text found"
            } else {
                "expected text missing"
            }
            .to_string(),
        });
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for result in results {
            println!(
                "{}  {}  {}",
                if result.passed { "pass" } else { "fail" },
                result.id,
                result.name
            );
            println!("  {}", result.detail);
        }
    }
    Ok(())
}

fn print_live_eval(conn: &Connection, since_days: i64, json_out: bool) -> Result<()> {
    let report = live_eval_report(conn, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Live Eval");
        println!("reads: {}", report.reads);
        println!("feedback_events: {}", report.feedback_events);
        println!(
            "useful_rate: {:.1}% ({})",
            report.useful_rate * 100.0,
            report.useful_rate_source
        );
        println!(
            "inferred_useful_rate: {:.1}% ({}/{})",
            report.inferred_useful_rate * 100.0,
            report.inferred_useful,
            report.inferred_total
        );
        println!("inferred_missing: {}", report.inferred_missing);
        if !report.inferred_missing_queries.is_empty() {
            println!(
                "inferred_missing_queries: {}",
                report.inferred_missing_queries.join(" | ")
            );
        }
        println!("noisy_memory_ids: {}", report.noisy_memory_ids.join(","));
    }
    Ok(())
}

pub(crate) fn live_eval_report(conn: &Connection, since_days: i64) -> Result<LiveEvalReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let reads = read_events(conn, since_ms, usize::MAX)?;
    let feedback = memory_feedback_counts(conn, since_ms)?;
    let mut useful = 0;
    let mut useless = 0;
    let mut missing = 0;
    let mut noisy = Vec::new();
    for (id, (pos, neg, miss)) in &feedback {
        useful += *pos;
        useless += *neg;
        missing += *miss;
        if *neg > *pos {
            noisy.push(id.clone());
        }
    }
    noisy.sort();
    let mut missing_queries = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT detail FROM memory_events WHERE event_type = 'memory_feedback' AND created_at >= ?1",
    )?;
    let rows = stmt.query_map(params![since_ms], |row| row.get::<_, String>(0))?;
    for row in rows {
        let detail = row?;
        let Ok(value) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        if value.get("rating").and_then(Value::as_str) == Some("missing")
            && let Some(query) = value.get("query").and_then(Value::as_str)
            && !query.is_empty()
        {
            missing_queries.push(query.to_string());
        }
    }
    missing_queries.sort();
    missing_queries.dedup();
    let total_feedback = useful + useless + missing;
    let inferred = inferred_live_signals(conn, &reads)?;
    let feedback_useful_rate = if total_feedback == 0 {
        0.0
    } else {
        useful as f64 / total_feedback as f64
    };
    let inferred_useful_rate = if inferred.total == 0 {
        0.0
    } else {
        inferred.useful as f64 / inferred.total as f64
    };
    let (useful_rate, useful_rate_source) = if total_feedback > 0 {
        (feedback_useful_rate, "feedback")
    } else if inferred.total > 0 {
        (inferred_useful_rate, "inferred")
    } else {
        (0.0, "none")
    };
    Ok(LiveEvalReport {
        version: 1,
        since_days,
        reads: reads.len(),
        feedback_events: total_feedback,
        useful,
        useless,
        missing,
        useful_rate,
        useful_rate_source: useful_rate_source.to_string(),
        feedback_useful_rate,
        inferred_useful: inferred.useful,
        inferred_total: inferred.total,
        inferred_useful_rate,
        inferred_missing: inferred.missing_queries.len(),
        noisy_memory_ids: noisy,
        missing_queries,
        inferred_missing_queries: inferred.missing_queries,
    })
}

pub(crate) fn materialize_inferred_feedback(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<InferredFeedbackReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let reads = read_events(conn, since_ms, limit)?;
    let mut report = InferredFeedbackReport {
        version: 1,
        since_days,
        scanned: 0,
        written: 0,
        useful: 0,
        missing: 0,
        skipped: 0,
    };
    for read in reads {
        if !is_agent_memory_read(&read.command) {
            continue;
        }
        report.scanned += 1;
        if inferred_feedback_exists(conn, read.id)? {
            report.skipped += 1;
            continue;
        }
        if read.result_count > 0 && !read.memory_ids.is_empty() {
            let ids = read.memory_ids.iter().take(8).cloned().collect::<Vec<_>>();
            let detail = serde_json::to_string(&json!({
                "rating": "useful",
                "ids": ids,
                "command": read.command,
                "query": truncate_chars(&read.query, 500),
                "note": "autonomous inferred feedback from successful memory read",
                "source": "autonomous_inferred",
                "inferred_read_id": read.id,
            }))?;
            log_event(conn, "memory_feedback", None, &detail)?;
            report.written += 1;
            report.useful += 1;
        } else if should_infer_missing_memory_gap(conn, &read.query)? {
            let detail = serde_json::to_string(&json!({
                "rating": "missing",
                "ids": [],
                "command": read.command,
                "query": truncate_chars(&read.query, 500),
                "note": "autonomous inferred feedback from empty memory read",
                "source": "autonomous_inferred",
                "inferred_read_id": read.id,
            }))?;
            log_event(conn, "memory_feedback", None, &detail)?;
            report.written += 1;
            report.missing += 1;
        } else {
            report.skipped += 1;
        }
    }
    Ok(report)
}

fn inferred_feedback_exists(conn: &Connection, read_id: i64) -> Result<bool> {
    let pattern = format!("%\"inferred_read_id\":{read_id}%");
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM memory_events WHERE event_type = 'memory_feedback' AND detail LIKE ?1 LIMIT 1",
            params![pattern],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

struct InferredLiveSignals {
    useful: usize,
    total: usize,
    missing_queries: Vec<String>,
}

fn inferred_live_signals(
    conn: &Connection,
    reads: &[MemoryReadEvent],
) -> Result<InferredLiveSignals> {
    let mut useful = 0;
    let mut total = 0;
    let mut missing_queries = Vec::new();
    for read in reads {
        if !is_agent_memory_read(&read.command) {
            continue;
        }
        total += 1;
        if read.result_count > 0 && !read.memory_ids.is_empty() {
            useful += 1;
        } else if should_infer_missing_memory_gap(conn, &read.query)? {
            missing_queries.push(truncate_chars(&read.query, 140));
        }
    }
    missing_queries.sort();
    missing_queries.dedup();
    missing_queries.truncate(20);
    Ok(InferredLiveSignals {
        useful,
        total,
        missing_queries,
    })
}

pub(crate) fn should_infer_missing_memory_gap(conn: &Connection, query: &str) -> Result<bool> {
    let terms = relevance_terms(query);
    if terms.is_empty() || is_code_identifier_query(query) {
        return Ok(false);
    }
    unresolved_memory_gap(conn, query)
}

fn is_code_identifier_query(query: &str) -> bool {
    let query = query.trim();
    if query.len() < 3 || query.chars().any(char::is_whitespace) {
        return false;
    }
    query.contains("::")
        || query.contains('/')
        || query.contains('\\')
        || query.contains('.')
        || query.contains('_')
        || query
            .chars()
            .any(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

pub(crate) fn unresolved_memory_gap(conn: &Connection, query: &str) -> Result<bool> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(false);
    }
    let rows = query_memories(
        conn,
        Some(query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        1,
    )?;
    if !rows.is_empty() {
        return Ok(false);
    }
    Ok(!memory_link_resolves_query(conn, query)?)
}

fn memory_link_resolves_query(conn: &Connection, query: &str) -> Result<bool> {
    if query.chars().count() < 3 {
        return Ok(false);
    }
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories m \
         JOIN memory_links l ON l.memory_id = m.id \
         WHERE m.status IN ('active', 'uncertain') \
         AND (lower(l.target) = lower(?1) OR instr(lower(l.target), lower(?1)) > 0)",
        params![query],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn is_agent_memory_read(command: &str) -> bool {
    matches!(
        command,
        "brief" | "impact" | "context" | "context-pack" | "retrieve" | "recall" | "evidence"
    )
}

pub(crate) fn print_secret_scan(conn: &Connection, fix_redact: bool, json_out: bool) -> Result<()> {
    let findings = scan_secret_findings(conn)?;
    if fix_redact {
        let changed = redact_sensitive_memories(conn, &findings)?;
        if json_out {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "findings": findings,
                    "redacted": changed
                }))?
            );
        } else {
            println!("redacted: {changed}");
        }
        return Ok(());
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&findings)?);
    } else if findings.is_empty() {
        println!("secrets: none");
    } else {
        for finding in findings {
            println!("{}  {}  {}", finding.pattern, finding.id, finding.title);
        }
    }
    Ok(())
}

pub(crate) fn redact_export(export: &mut MemoryExport) -> Result<()> {
    for item in &mut export.memories {
        item.memory.title = redact_sensitive_text(&item.memory.title)?;
        item.memory.body = redact_sensitive_text(&item.memory.body)?;
    }
    Ok(())
}

fn redact_sensitive_memories(conn: &Connection, findings: &[SecretFinding]) -> Result<usize> {
    let mut changed = 0;
    let mut seen = HashSet::new();
    for finding in findings {
        if !seen.insert(finding.id.clone()) {
            continue;
        }
        let memory = get_memory(conn, &finding.id)?;
        let title = redact_sensitive_text(&memory.title)?;
        let body = redact_sensitive_text(&memory.body)?;
        if title != memory.title || body != memory.body {
            conn.execute(
                "UPDATE memories SET title = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
                params![title, body, now_ms(), finding.id],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

pub(crate) fn redact_sensitive_text(text: &str) -> Result<String> {
    let patterns = [
        Regex::new(r"sk-[A-Za-z0-9_-]{8,}")?,
        Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----")?,
        Regex::new(r"(?i)(api_key|token|password|secret)\s*[:=]\s*\S+")?,
    ];
    let mut out = text.to_string();
    for pattern in patterns {
        out = pattern.replace_all(&out, "[REDACTED]").to_string();
    }
    Ok(out)
}

pub(crate) fn scan_secret_findings(conn: &Connection) -> Result<Vec<SecretFinding>> {
    let rows = query_memories(conn, None, &[], &[], None, usize::MAX)?;
    let patterns = [
        ("openai_key", Regex::new(r"sk-[A-Za-z0-9_-]{8,}")?),
        (
            "private_key",
            Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")?,
        ),
        (
            "assignment_secret",
            Regex::new(r"(?i)(api_key|token|password|secret)\s*[:=]")?,
        ),
    ];
    let mut out = Vec::new();
    for row in rows {
        let text = format!("{}\n{}", row.title, row.body);
        for (name, regex) in &patterns {
            if regex.is_match(&text) {
                out.push(SecretFinding {
                    id: row.id.clone(),
                    title: row.title.clone(),
                    pattern: (*name).to_string(),
                });
            }
        }
    }
    Ok(out)
}

pub(crate) fn print_doctor(
    conn: &Connection,
    root: &Path,
    fix_redact: bool,
    json_out: bool,
    self_check: bool,
) -> Result<()> {
    let mut findings = Vec::new();
    let secret_findings = scan_secret_findings(conn)?;
    if fix_redact && !secret_findings.is_empty() {
        let changed = redact_sensitive_memories(conn, &secret_findings)?;
        findings.push(DoctorFinding {
            kind: "secrets".to_string(),
            status: "fixed".to_string(),
            detail: format!("redacted {changed} memory card(s)"),
        });
    } else {
        findings.push(DoctorFinding {
            kind: "secrets".to_string(),
            status: if secret_findings.is_empty() {
                "ok"
            } else {
                "warn"
            }
            .to_string(),
            detail: format!("{} finding(s)", secret_findings.len()),
        });
    }
    let mut review = Vec::new();
    review.extend(review_stale(conn, 30)?);
    review.extend(review_uncertain(conn)?);
    review.extend(review_low_confidence(conn)?);
    review.extend(review_duplicates(conn)?);
    findings.push(DoctorFinding {
        kind: "memory_quality".to_string(),
        status: if review.is_empty() { "ok" } else { "warn" }.to_string(),
        detail: format!("{} issue(s)", review.len()),
    });
    let pending = list_inbox(conn, "pending", usize::MAX)?.len();
    findings.push(DoctorFinding {
        kind: "inbox".to_string(),
        status: if pending == 0 { "ok" } else { "warn" }.to_string(),
        detail: format!("{pending} pending item(s)"),
    });
    let links = link_report(conn, None, root, false)?;
    let missing_links = links.iter().filter(|item| item.status == "missing").count();
    findings.push(DoctorFinding {
        kind: "links".to_string(),
        status: if missing_links == 0 { "ok" } else { "warn" }.to_string(),
        detail: format!("{missing_links} missing link(s)"),
    });
    let codegraph_ok = find_nearest_codegraph_root(root).is_some();
    findings.push(DoctorFinding {
        kind: "codegraph".to_string(),
        status: if codegraph_ok { "ok" } else { "info" }.to_string(),
        detail: if codegraph_ok {
            ".codegraph index found".to_string()
        } else {
            ".codegraph index not found".to_string()
        },
    });
    let embed = embeddings::embed_status(
        conn,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )?;
    findings.push(DoctorFinding {
        kind: "embeddings".to_string(),
        status: if embed.stale == 0 { "ok" } else { "warn" }.to_string(),
        detail: format!("indexed={}, stale={}", embed.indexed, embed.stale),
    });
    if self_check {
        let schema_ok = verify_schema(conn).is_ok();
        findings.push(DoctorFinding {
            kind: "self".to_string(),
            status: if schema_ok { "ok" } else { "warn" }.to_string(),
            detail: format!(
                "version={} schema={} vec_feature={}",
                env!("CARGO_PKG_VERSION"),
                schema_version(conn).unwrap_or_default(),
                cfg!(feature = "vec")
            ),
        });
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&findings)?);
    } else {
        for finding in findings {
            println!("{}  {}  {}", finding.status, finding.kind, finding.detail);
        }
    }
    Ok(())
}

pub(crate) fn print_codex_doctor(config: &Path, json_out: bool) -> Result<()> {
    let report = codex_doctor_report(config)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for finding in &report.findings {
            println!("{}  {}  {}", finding.status, finding.kind, finding.detail);
        }
    }
    Ok(())
}

pub(crate) fn codex_doctor_report(config: &Path) -> Result<CodexDoctorReport> {
    let mut findings = Vec::new();
    if !config.exists() {
        findings.push(DoctorFinding {
            kind: "config".to_string(),
            status: "warn".to_string(),
            detail: format!("missing {}", config.display()),
        });
        return Ok(CodexDoctorReport {
            ok: false,
            findings,
        });
    }
    let raw = fs::read_to_string(config)
        .with_context(|| format!("failed to read {}", config.display()))?;
    findings.push(DoctorFinding {
        kind: "config".to_string(),
        status: "ok".to_string(),
        detail: config.display().to_string(),
    });
    let Some(section) = toml_section(&raw, "mcp_servers.dukememory") else {
        findings.push(DoctorFinding {
            kind: "mcp_section".to_string(),
            status: "warn".to_string(),
            detail: "missing [mcp_servers.dukememory]".to_string(),
        });
        return Ok(CodexDoctorReport {
            ok: false,
            findings,
        });
    };
    findings.push(DoctorFinding {
        kind: "mcp_section".to_string(),
        status: "ok".to_string(),
        detail: "[mcp_servers.dukememory] found".to_string(),
    });
    let command = toml_string_value(&section, "command");
    let args = toml_array_strings(&section, "args");
    let Some(command) = command else {
        findings.push(DoctorFinding {
            kind: "command".to_string(),
            status: "warn".to_string(),
            detail: "missing command".to_string(),
        });
        return Ok(CodexDoctorReport {
            ok: false,
            findings,
        });
    };
    let command_path = expand_tilde(&command);
    findings.push(DoctorFinding {
        kind: "command".to_string(),
        status: if command_path.exists() { "ok" } else { "warn" }.to_string(),
        detail: command_path.display().to_string(),
    });
    let serve_mcp = args.iter().any(|arg| arg == "serve-mcp");
    findings.push(DoctorFinding {
        kind: "serve_mcp_arg".to_string(),
        status: if serve_mcp { "ok" } else { "warn" }.to_string(),
        detail: if serve_mcp {
            "args include serve-mcp".to_string()
        } else {
            format!("args={}", args.join(" "))
        },
    });
    for flag in ["--db", "--config"] {
        if let Some(path) = arg_after(&args, flag) {
            let path = expand_tilde(path);
            findings.push(DoctorFinding {
                kind: flag.trim_start_matches('-').to_string(),
                status: if path.exists() { "ok" } else { "warn" }.to_string(),
                detail: path.display().to_string(),
            });
        } else {
            findings.push(DoctorFinding {
                kind: flag.trim_start_matches('-').to_string(),
                status: "warn".to_string(),
                detail: format!("missing {flag} arg"),
            });
        }
    }
    let mcp_status = match probe_mcp_tools_list(&command_path, &args) {
        Ok(detail) => ("ok", detail),
        Err(err) => ("warn", err.to_string()),
    };
    findings.push(DoctorFinding {
        kind: "mcp_probe".to_string(),
        status: mcp_status.0.to_string(),
        detail: mcp_status.1,
    });
    let ok = findings.iter().all(|finding| finding.status != "warn");
    Ok(CodexDoctorReport { ok, findings })
}

fn toml_section(raw: &str, section: &str) -> Option<String> {
    let header = format!("[{section}]");
    let mut in_section = false;
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section {
                break;
            }
            in_section = trimmed == header;
            continue;
        }
        if in_section {
            out.push(line);
        }
    }
    in_section.then(|| out.join("\n"))
}

fn toml_string_value(section: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} =");
    section.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix(&prefix)
            .map(str::trim)
            .and_then(|value| value.trim_matches('"').split('"').next())
            .map(ToOwned::to_owned)
    })
}

fn toml_array_strings(section: &str, key: &str) -> Vec<String> {
    let prefix = format!("{key} =");
    let Some(raw) = section
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
    else {
        return Vec::new();
    };
    raw.trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_matches('"').to_string())
        .collect()
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair.first().is_some_and(|value| value == flag))
        .and_then(|pair| pair.get(1))
        .map(String::as_str)
}

fn probe_mcp_tools_list(command: &Path, args: &[String]) -> Result<String> {
    let mut child = ProcessCommand::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", command.display()))?;
    {
        let stdin = child.stdin.as_mut().context("failed to open mcp stdin")?;
        stdin.write_all(br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)?;
        stdin.write_all(b"\n")?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "serve-mcp exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("memory_brief") {
        Ok("serve-mcp tools/list includes memory_brief".to_string())
    } else {
        bail!("serve-mcp tools/list did not include memory_brief")
    }
}

pub(crate) fn print_review(conn: &Connection, stale_days: i64, as_json: bool) -> Result<()> {
    let mut issues = Vec::new();
    issues.extend(review_stale(conn, stale_days)?);
    issues.extend(review_uncertain(conn)?);
    issues.extend(review_low_confidence(conn)?);
    issues.extend(review_duplicates(conn)?);
    if as_json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else if issues.is_empty() {
        println!("review: clean");
    } else {
        for issue in issues {
            println!("{}  {}  {}", issue.kind, issue.id, issue.title);
            println!("  {}", issue.detail);
        }
    }
    Ok(())
}

pub(crate) fn print_stale(conn: &Connection, days: i64, as_json: bool) -> Result<()> {
    let issues = review_stale(conn, days)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else if issues.is_empty() {
        println!("stale: none");
    } else {
        for issue in issues {
            println!("{}  {}  {}", issue.kind, issue.id, issue.title);
            println!("  {}", issue.detail);
        }
    }
    Ok(())
}

pub(crate) fn print_conflicts(conn: &Connection, as_json: bool) -> Result<()> {
    let issues = review_duplicates(conn)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else if issues.is_empty() {
        println!("conflicts: none");
    } else {
        for issue in issues {
            println!("{}  {}  {}", issue.kind, issue.id, issue.title);
            println!("  {}", issue.detail);
        }
    }
    Ok(())
}

pub(crate) fn review_stale(conn: &Connection, days: i64) -> Result<Vec<ReviewIssue>> {
    let cutoff = now_ms() - days.max(0) * 86_400_000;
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    Ok(rows
        .into_iter()
        .filter(|m| m.updated_at < cutoff)
        .map(|m| ReviewIssue {
            kind: "stale".to_string(),
            id: m.id,
            title: m.title,
            detail: format!("not updated for at least {days} day(s)"),
        })
        .collect())
}

pub(crate) fn review_uncertain(conn: &Connection) -> Result<Vec<ReviewIssue>> {
    Ok(query_memories(
        conn,
        None,
        &[],
        &["uncertain".to_string()],
        None,
        usize::MAX,
    )?
    .into_iter()
    .map(|m| ReviewIssue {
        kind: "uncertain".to_string(),
        id: m.id,
        title: m.title,
        detail: "needs confirmation or promotion to active/rejected".to_string(),
    })
    .collect())
}

pub(crate) fn review_low_confidence(conn: &Connection) -> Result<Vec<ReviewIssue>> {
    Ok(
        query_memories(conn, None, &[], &["active".to_string()], None, usize::MAX)?
            .into_iter()
            .filter(|m| m.confidence < 0.5)
            .map(|m| ReviewIssue {
                kind: "low_confidence".to_string(),
                id: m.id,
                title: m.title,
                detail: format!("confidence is {:.2}", m.confidence),
            })
            .collect(),
    )
}

pub(crate) fn review_duplicates(conn: &Connection) -> Result<Vec<ReviewIssue>> {
    let rows = query_memories(conn, None, &[], &["active".to_string()], None, usize::MAX)?;
    let mut seen: std::collections::HashMap<(String, String, String), String> =
        std::collections::HashMap::new();
    let mut issues = Vec::new();
    for m in rows {
        let key = (
            m.memory_type.clone(),
            m.scope.clone(),
            normalize_title(&m.title),
        );
        if let Some(first_id) = seen.get(&key) {
            issues.push(ReviewIssue {
                kind: "possible_conflict".to_string(),
                id: m.id,
                title: m.title,
                detail: format!("same type/scope/title as active memory {first_id}"),
            });
        } else {
            seen.insert(key, m.id);
        }
    }
    Ok(issues)
}

pub(crate) fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn print_link_report(
    conn: &Connection,
    id: Option<&str>,
    root: &Path,
    validate_symbols: bool,
    as_json: bool,
) -> Result<()> {
    let reports = link_report(conn, id, root, validate_symbols)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else if reports.is_empty() {
        println!("links: none");
    } else {
        for report in reports {
            println!(
                "{}  {}:{}  {}",
                report.status, report.kind, report.target, report.memory_id
            );
            println!("  {}", report.detail);
        }
    }
    Ok(())
}

pub(crate) fn link_report(
    conn: &Connection,
    id: Option<&str>,
    root: &Path,
    validate_symbols: bool,
) -> Result<Vec<LinkReport>> {
    let mut sql = "SELECT memory_id, kind, target FROM memory_links".to_string();
    let mut params_vec = Vec::new();
    if let Some(id) = id {
        sql.push_str(" WHERE memory_id = ?");
        params_vec.push(id.to_string());
    }
    sql.push_str(" ORDER BY memory_id, id");
    let mut stmt = conn.prepare(&sql)?;
    let links = stmt.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for link in links {
        let (memory_id, kind, target) = link?;
        let (status, detail) = match kind.as_str() {
            "file" => {
                let path = root.join(&target);
                if path.exists() {
                    (
                        "ok".to_string(),
                        format!("file exists at {}", path.display()),
                    )
                } else {
                    (
                        "missing".to_string(),
                        format!("file not found at {}", path.display()),
                    )
                }
            }
            "symbol" => {
                if !validate_symbols {
                    if find_nearest_codegraph_root(root).is_some() {
                        (
                            "unknown".to_string(),
                            "use --validate-symbols to query CodeGraph".to_string(),
                        )
                    } else {
                        (
                            "unknown".to_string(),
                            "no .codegraph index found for symbol validation".to_string(),
                        )
                    }
                } else if let Some(codegraph_root) = find_nearest_codegraph_root(root) {
                    match run_codegraph_node(&codegraph_root, &target, 1200) {
                        Ok(output) if !output.trim().is_empty() => (
                            "ok".to_string(),
                            first_line(&output).unwrap_or_else(|| "symbol found".to_string()),
                        ),
                        Ok(_) => (
                            "missing".to_string(),
                            "CodeGraph returned no output".to_string(),
                        ),
                        Err(err) => ("unknown".to_string(), err.to_string()),
                    }
                } else {
                    (
                        "unknown".to_string(),
                        "no .codegraph index found for symbol validation".to_string(),
                    )
                }
            }
            _ => ("unknown".to_string(), "custom link kind".to_string()),
        };
        out.push(LinkReport {
            memory_id,
            kind,
            target,
            status,
            detail,
        });
    }
    Ok(out)
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn find_nearest_codegraph_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().ok()?;
    loop {
        if current.join(".codegraph").join("codegraph.db").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub(crate) fn print_review_tui(conn: &Connection, stale_days: i64) -> Result<()> {
    println!("dukememory. Review");
    println!();
    println!("Inbox");
    for item in list_inbox(conn, "pending", 10)? {
        println!("- {} {} {}", item.id, item.memory_type, item.title);
    }
    println!();
    println!("Review Issues");
    let mut issues = Vec::new();
    issues.extend(review_stale(conn, stale_days)?);
    issues.extend(review_uncertain(conn)?);
    issues.extend(review_low_confidence(conn)?);
    issues.extend(review_duplicates(conn)?);
    if issues.is_empty() {
        println!("- none");
    } else {
        for issue in issues.into_iter().take(20) {
            println!("- {} {} {}", issue.kind, issue.id, issue.title);
        }
    }
    println!();
    println!("Commands");
    println!("- inbox-approve <id>");
    println!("- inbox-reject <id>");
    println!("- scan-secrets --fix-redact");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_effective_limit_follows_budget() {
        assert_eq!(impact_effective_limit(30, 1_200), 8);
        assert_eq!(impact_effective_limit(30, 3_000), 24);
        assert_eq!(impact_effective_limit(30, 8_000), 30);
        assert_eq!(impact_effective_limit(3, 1_200), 3);
        assert_eq!(impact_effective_limit(0, 1_200), 1);
        assert_eq!(impact_candidate_limit(30, 8, 1_200), 16);
        assert_eq!(impact_candidate_limit(30, 24, 3_000), 30);
        assert_eq!(impact_candidate_limit(100, 24, 3_000), 48);
        assert_eq!(impact_candidate_limit(30, 30, 8_000), 30);
    }
}

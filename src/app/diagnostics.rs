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
    pub(crate) semantic_empty_missing: usize,
    pub(crate) noisy_memory_ids: Vec<String>,
    pub(crate) missing_queries: Vec<String>,
    pub(crate) inferred_missing_queries: Vec<String>,
    pub(crate) semantic_empty_missing_queries: Vec<String>,
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
    pub(crate) audit_read: bool,
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
    pub(crate) audit_read: bool,
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

    let semantic_status = if retrieval.semantic_skipped {
        MemorySemanticStatus::Skipped
    } else if retrieval.semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    let ids = brief_report_memory_ids(&must_follow, &relevant, &risks);
    let receipt = memory_receipt_with_semantic("brief", semantic_status, &ids, "none");
    let mut report = BriefReport {
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
    };
    let ids = if request.json_out {
        ids
    } else {
        sync_brief_plain_receipt_ids(&mut report, semantic_status)
    };
    if request.audit_read {
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
    }

    Ok(report)
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
    if budget <= 500 {
        BriefSectionLimits {
            must_follow: 0,
            relevant: 0,
            risks: 0,
        }
    } else if budget <= 1_200 {
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
    if budget <= 500 {
        (0, 0)
    } else if budget <= 1_200 {
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
    if budget <= 500 {
        48
    } else if budget <= 1_200 {
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

fn sync_brief_plain_receipt_ids(
    report: &mut BriefReport,
    semantic_status: MemorySemanticStatus,
) -> Vec<String> {
    sync_plain_receipt_ids(report, semantic_status, "brief", render_brief)
}

fn sync_impact_plain_receipt_ids(
    report: &mut ImpactReport,
    semantic_status: MemorySemanticStatus,
) -> Vec<String> {
    sync_plain_receipt_ids(report, semantic_status, "impact", render_impact)
}

fn sync_plain_receipt_ids<T>(
    report: &mut T,
    semantic_status: MemorySemanticStatus,
    command: &str,
    render: fn(&T) -> String,
) -> Vec<String>
where
    T: ReceiptReport,
{
    let mut ids = rendered_memory_ids(&render(report));
    report.set_receipt(memory_receipt_with_semantic(
        command,
        semantic_status,
        &ids,
        "none",
    ));
    for _ in 0..3 {
        let next_ids = rendered_memory_ids(&render(report));
        if next_ids == ids {
            return ids;
        }
        ids = next_ids;
        report.set_receipt(memory_receipt_with_semantic(
            command,
            semantic_status,
            &ids,
            "none",
        ));
    }
    ids
}

trait ReceiptReport {
    fn set_receipt(&mut self, receipt: String);
}

impl ReceiptReport for BriefReport {
    fn set_receipt(&mut self, receipt: String) {
        self.receipt = receipt;
    }
}

impl ReceiptReport for ImpactReport {
    fn set_receipt(&mut self, receipt: String) {
        self.receipt = receipt;
    }
}

fn rendered_memory_ids(rendered: &str) -> Vec<String> {
    let mut ids = rendered
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- ")?;
            let id = rest.split_whitespace().next()?;
            if is_compact_memory_id(id) {
                Some(id.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

fn is_compact_memory_id(value: &str) -> bool {
    value.len() == 12 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn brief_report_memory_ids(
    must_follow: &[BriefItem],
    relevant: &[BriefItem],
    risks: &[BriefItem],
) -> Vec<String> {
    collect_brief_item_ids(
        must_follow
            .iter()
            .chain(relevant.iter())
            .chain(risks.iter()),
    )
}

fn impact_report_memory_ids(
    decisions: &[BriefItem],
    constraints: &[BriefItem],
    risks: &[BriefItem],
    related: &[BriefItem],
) -> Vec<String> {
    collect_brief_item_ids(
        decisions
            .iter()
            .chain(constraints.iter())
            .chain(risks.iter())
            .chain(related.iter()),
    )
}

fn collect_brief_item_ids<'a>(items: impl Iterator<Item = &'a BriefItem>) -> Vec<String> {
    let mut ids = items.map(|item| item.id.clone()).collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
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

    let semantic_status = if semantic_used {
        MemorySemanticStatus::Used
    } else {
        MemorySemanticStatus::Fallback
    };
    let ids = impact_report_memory_ids(&decisions, &constraints, &risks, &related);
    let receipt = memory_receipt_with_semantic("impact", semantic_status, &ids, "none");
    let mut report = ImpactReport {
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
    };
    let ids = if request.json_out {
        ids
    } else {
        sync_impact_plain_receipt_ids(&mut report, semantic_status)
    };
    if request.audit_read {
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
    }

    Ok(report)
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
    if budget <= 500 {
        ImpactSectionLimits {
            decisions: 0,
            constraints: 0,
            risks: 0,
            related: 0,
            checks: 0,
            links: 0,
        }
    } else if budget <= 1_200 {
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
    let stale_evidence = stale_file_evidence(conn, 20)?;
    let mut stale_active = stale_active_memories(conn, 10)?
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
    let mut stale_ids = stale_active
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    for evidence in &stale_evidence {
        if !matches!(evidence.memory_status.as_str(), "active" | "uncertain")
            || !stale_ids.insert(evidence.memory_id.clone())
        {
            continue;
        }
        if let Ok(memory) = get_memory(conn, &evidence.memory_id) {
            stale_active.push(brief_item_from_memory(
                &memory,
                95.0,
                vec![format!(
                    "{} file evidence: {}",
                    evidence.status, evidence.path
                )],
                &empty_terms,
                8_000,
            ));
        }
    }
    stale_active.truncate(20);
    let ok = missing_links.is_empty()
        && conflicts.is_empty()
        && stale_active.is_empty()
        && stale_evidence.is_empty();

    Ok(DriftReport {
        version: 1,
        ok,
        changed_only,
        root: root.display().to_string(),
        changed_files,
        missing_links,
        conflicts,
        stale_active,
        stale_evidence,
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

pub(crate) fn handle_eval(
    conn: &Connection,
    command: EvalCommand,
    gen_config: &crate::runtime_config::GenerationConfig,
    root: &Path,
) -> Result<()> {
    match command {
        EvalCommand::AddCase {
            name,
            query,
            expected,
            budget,
            split,
        } => {
            let id = Uuid::new_v4().simple().to_string()[..12].to_string();
            conn.execute(
                "INSERT INTO eval_cases (id, name, query, expected, budget, split, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, name, query, expected, budget as i64, split, now_ms()],
            )?;
            println!("{id}");
        }
        EvalCommand::Run { json } => run_eval(conn, json)?,
        EvalCommand::Rag {
            scope,
            limit,
            budget,
            budget_profile,
            provider,
            endpoint,
            model,
            write_baseline,
            json,
        } => run_rag_eval(
            conn,
            root,
            scope.as_deref(),
            limit,
            budget
                .or_else(|| budget_profile_chars(budget_profile))
                .unwrap_or(3000),
            &provider,
            &endpoint,
            &model,
            write_baseline,
            json,
        )?,
        EvalCommand::GraphRag {
            scope,
            limit,
            budget,
            budget_profile,
            provider,
            endpoint,
            model,
            json,
        } => run_graph_rag_eval(
            conn,
            scope.as_deref(),
            limit,
            budget
                .or_else(|| budget_profile_chars(budget_profile))
                .unwrap_or(3000),
            gen_config,
            &provider,
            &endpoint,
            &model,
            json,
        )?,
        EvalCommand::Advanced { json } => print_advanced_eval(conn, json)?,
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

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) case_source: String,
    pub(crate) total: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) recall: f64,
    pub(crate) average_confidence: f64,
    pub(crate) semantic_used: usize,
    pub(crate) semantic_fallbacks: usize,
    pub(crate) packing: RagEvalPackingSummary,
    pub(crate) evidence_placement: RagEvalEvidencePlacementSummary,
    pub(crate) evaluation_layers: RagEvalLayersSummary,
    pub(crate) grounded_answers: RagEvalGroundedSummary,
    pub(crate) ranking: RagEvalRankingSummary,
    pub(crate) eval_matrix: RagEvalMatrixSummary,
    pub(crate) retrieval_tuning: RagEvalRetrievalTuningSummary,
    pub(crate) split: RagEvalSplitSummary,
    pub(crate) baseline: RagEvalBaselineSummary,
    pub(crate) cases: Vec<RagEvalCaseResult>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalLayersSummary {
    pub(crate) protocol_version: u32,
    pub(crate) retrieval: RagEvalRetrievalLayer,
    pub(crate) extractive_grounding: RagEvalExtractiveLayer,
    pub(crate) generated_output_guard: RagEvalGeneratedOutputLayer,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalRetrievalLayer {
    pub(crate) evaluation_kind: String,
    pub(crate) cases: usize,
    pub(crate) passed: usize,
    pub(crate) recall: f64,
    pub(crate) hit_at_3_rate: f64,
    pub(crate) mean_reciprocal_rank: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalExtractiveLayer {
    pub(crate) evaluation_kind: String,
    pub(crate) live_model_executed: bool,
    pub(crate) cases: usize,
    pub(crate) passed: usize,
    pub(crate) coverage: f64,
    pub(crate) unknown_citation_cases: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalGeneratedOutputLayer {
    pub(crate) evaluation_kind: String,
    pub(crate) live_model_executed: bool,
    pub(crate) fixture_version: u32,
    pub(crate) attack_vectors: usize,
    pub(crate) passed: usize,
    pub(crate) total: usize,
    pub(crate) false_accepts: usize,
    pub(crate) false_rejects: usize,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalPackingSummary {
    pub(crate) candidate_count: usize,
    pub(crate) selected_count: usize,
    pub(crate) memory_candidates: usize,
    pub(crate) chunk_candidates: usize,
    pub(crate) selected_memories: usize,
    pub(crate) selected_chunks: usize,
    pub(crate) suppressed_duplicate: usize,
    pub(crate) suppressed_overlap: usize,
    pub(crate) suppressed_file_cap: usize,
    pub(crate) suppressed_limit: usize,
    pub(crate) suppressed_sources: usize,
    pub(crate) expected_selected: usize,
    pub(crate) expected_suppressed_by_packing: usize,
    pub(crate) expected_missing_from_candidates: usize,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalEvidencePlacementSummary {
    pub(crate) expected_total: usize,
    pub(crate) selected: usize,
    pub(crate) suppressed_by_packing: usize,
    pub(crate) missing_from_candidates: usize,
    pub(crate) empty_expected: usize,
    pub(crate) selection_recall: f64,
    pub(crate) candidate_recall: f64,
    pub(crate) near_miss_count: usize,
    pub(crate) suppression_reasons: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalGroundedSummary {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) coverage: f64,
    pub(crate) expected_in_answer: usize,
    pub(crate) cited_answers: usize,
    pub(crate) unknown_citation_cases: usize,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalRankingSummary {
    pub(crate) total: usize,
    pub(crate) hit_at_1: usize,
    pub(crate) hit_at_3: usize,
    pub(crate) hit_at_5: usize,
    pub(crate) hit_at_1_rate: f64,
    pub(crate) hit_at_3_rate: f64,
    pub(crate) hit_at_5_rate: f64,
    pub(crate) mean_reciprocal_rank: f64,
}

const RAG_EVAL_RECOMMENDED_STORED_CASES: usize = 12;
const RAG_EVAL_RECOMMENDED_HOLDOUT_CASES: usize = 5;
const RAG_EVAL_PROTOCOL_VERSION: u32 = 2;
const RAG_EVAL_MATRIX_DIMENSIONS: [&str; 9] = [
    "source_chunk",
    "memory_card",
    "cli_workflow",
    "mcp_tooling",
    "http_api",
    "graph_memory",
    "multilingual",
    "negative_or_missing",
    "packing_near_miss",
];

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalMatrixSummary {
    pub(crate) status: String,
    pub(crate) stored_cases: usize,
    pub(crate) auto_cases: usize,
    pub(crate) recommended_min_stored_cases: usize,
    pub(crate) total_dimensions: usize,
    pub(crate) covered_dimensions: usize,
    pub(crate) coverage: f64,
    pub(crate) dimensions: std::collections::BTreeMap<String, usize>,
    pub(crate) missing_dimensions: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalRetrievalTuningSummary {
    pub(crate) status: String,
    pub(crate) selected_profile: String,
    pub(crate) candidate_recall: f64,
    pub(crate) selection_recall: f64,
    pub(crate) chunk_selection_rate: f64,
    pub(crate) memory_selection_rate: f64,
    pub(crate) semantic_fallback_rate: f64,
    pub(crate) near_miss_count: usize,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalSplitSummary {
    pub(crate) development_total: usize,
    pub(crate) development_passed: usize,
    pub(crate) development_recall: f64,
    pub(crate) holdout_total: usize,
    pub(crate) holdout_passed: usize,
    pub(crate) holdout_recall: f64,
    pub(crate) holdout_grounded_coverage: f64,
    pub(crate) recommended_min_holdout_cases: usize,
    pub(crate) holdout_ready: bool,
    pub(crate) tuning_isolation_enforced: bool,
    pub(crate) holdout_policy: String,
    pub(crate) origin_independence_verified: bool,
    pub(crate) development_signature: String,
    pub(crate) holdout_signature: String,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct RagEvalBaselineSummary {
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) present: bool,
    pub(crate) written: bool,
    pub(crate) regression: bool,
    pub(crate) current_signature: String,
    pub(crate) baseline_signature: Option<String>,
    pub(crate) baseline_recall: Option<f64>,
    pub(crate) baseline_grounded_coverage: Option<f64>,
    pub(crate) baseline_matrix_coverage: Option<f64>,
    pub(crate) baseline_candidate_recall: Option<f64>,
    pub(crate) baseline_selection_recall: Option<f64>,
    pub(crate) baseline_hit_at_3_rate: Option<f64>,
    pub(crate) baseline_mean_reciprocal_rank: Option<f64>,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RagEvalBaselineFile {
    version: u32,
    signature: String,
    #[serde(default)]
    corpus_signature: String,
    #[serde(default)]
    config_signature: String,
    total: usize,
    passed: usize,
    recall: f64,
    grounded_coverage: f64,
    matrix_coverage: f64,
    candidate_recall: f64,
    selection_recall: f64,
    #[serde(default)]
    hit_at_3_rate: f64,
    #[serde(default)]
    mean_reciprocal_rank: f64,
    #[serde(default)]
    holdout_total: usize,
    #[serde(default)]
    holdout_recall: f64,
    #[serde(default)]
    holdout_grounded_coverage: f64,
    covered_dimensions: usize,
    dimensions: std::collections::BTreeMap<String, usize>,
    written_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalGroundedAnswer {
    pub(crate) passed: bool,
    pub(crate) detail: String,
    pub(crate) answer: String,
    pub(crate) expected_found: bool,
    pub(crate) citation_count: usize,
    pub(crate) citations: Vec<String>,
    pub(crate) unknown_citations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RagEvalCaseResult {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) case_source: String,
    pub(crate) split: String,
    pub(crate) query: String,
    pub(crate) expected: String,
    pub(crate) expected_rank: Option<usize>,
    pub(crate) passed: bool,
    pub(crate) detail: String,
    pub(crate) confidence: String,
    pub(crate) confidence_score: f64,
    pub(crate) citation_count: usize,
    pub(crate) citations: Vec<String>,
    pub(crate) source_titles: Vec<String>,
    pub(crate) packing: RagPackingReport,
    pub(crate) expected_evidence_status: String,
    pub(crate) expected_in_candidates: bool,
    pub(crate) expected_suppressed_titles: Vec<String>,
    pub(crate) expected_suppressed_reasons: Vec<String>,
    pub(crate) semantic_used: bool,
    pub(crate) semantic_error: Option<String>,
    pub(crate) missing_evidence: Vec<String>,
    pub(crate) grounded_answer: RagEvalGroundedAnswer,
}

struct RagEvalCase {
    id: String,
    name: String,
    query: String,
    expected: String,
    budget: usize,
    source: String,
    split: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphRagEvalReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) case_source: String,
    pub(crate) total: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) recall: f64,
    pub(crate) grounded_coverage: f64,
    pub(crate) graph: GraphRagEvalGraphSummary,
    pub(crate) cases: Vec<GraphRagEvalCaseResult>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub(crate) struct GraphRagEvalGraphSummary {
    pub(crate) total_nodes: usize,
    pub(crate) total_edges: usize,
    pub(crate) connected_cases: usize,
    pub(crate) isolated_cases: usize,
    pub(crate) missing_graph_cases: usize,
    pub(crate) average_relationship_coverage: f64,
    pub(crate) average_edge_density: f64,
    pub(crate) relationship_kinds: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GraphRagEvalCaseResult {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) case_source: String,
    pub(crate) query: String,
    pub(crate) expected: String,
    pub(crate) passed: bool,
    pub(crate) detail: String,
    pub(crate) graph_status: String,
    pub(crate) confidence: String,
    pub(crate) confidence_score: f64,
    pub(crate) node_count: usize,
    pub(crate) edge_count: usize,
    pub(crate) relationship_coverage: f64,
    pub(crate) relationship_kinds: std::collections::BTreeMap<String, usize>,
    pub(crate) expected_in_graph: bool,
    pub(crate) expected_in_answer: bool,
    pub(crate) citation_count: usize,
    pub(crate) citations: Vec<String>,
    pub(crate) answer: String,
    pub(crate) ranked_node_titles: Vec<String>,
    pub(crate) missing_evidence: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn run_rag_eval(
    conn: &Connection,
    root: &Path,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
    write_baseline: bool,
    json_out: bool,
) -> Result<()> {
    let report = rag_eval_report_with_baseline(
        conn,
        scope,
        limit,
        budget,
        provider,
        endpoint,
        model,
        Some(root),
        write_baseline,
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("RAG Eval");
        println!(
            "status: {} recall: {:.1}% passed: {}/{} average_confidence: {:.2}",
            report.status, report.recall, report.passed, report.total, report.average_confidence
        );
        println!(
            "packing: selected={}/{} chunks={}/{} suppressed_overlap={} suppressed_file_cap={} suppressed_limit={} expected_selected={} expected_suppressed={} expected_missing={}",
            report.packing.selected_count,
            report.packing.candidate_count,
            report.packing.selected_chunks,
            report.packing.chunk_candidates,
            report.packing.suppressed_overlap,
            report.packing.suppressed_file_cap,
            report.packing.suppressed_limit,
            report.packing.expected_selected,
            report.packing.expected_suppressed_by_packing,
            report.packing.expected_missing_from_candidates
        );
        println!(
            "evidence_placement: selection_recall={:.1}% candidate_recall={:.1}% near_misses={} suppression_reasons={:?}",
            report.evidence_placement.selection_recall,
            report.evidence_placement.candidate_recall,
            report.evidence_placement.near_miss_count,
            report.evidence_placement.suppression_reasons
        );
        println!(
            "extractive_grounding: coverage={:.1}% passed={}/{} live_model=false expected_in_answer={} cited_answers={} unknown_citation_cases={}",
            report.grounded_answers.coverage,
            report.grounded_answers.passed,
            report.total,
            report.grounded_answers.expected_in_answer,
            report.grounded_answers.cited_answers,
            report.grounded_answers.unknown_citation_cases
        );
        println!(
            "generated_output_guard: fixture_v{} passed={}/{} attacks={} false_accepts={} false_rejects={} live_model=false",
            report
                .evaluation_layers
                .generated_output_guard
                .fixture_version,
            report.evaluation_layers.generated_output_guard.passed,
            report.evaluation_layers.generated_output_guard.total,
            report
                .evaluation_layers
                .generated_output_guard
                .attack_vectors,
            report
                .evaluation_layers
                .generated_output_guard
                .false_accepts,
            report
                .evaluation_layers
                .generated_output_guard
                .false_rejects
        );
        println!(
            "ranking: hit@1={:.1}% hit@3={:.1}% hit@5={:.1}% mrr={:.1}%",
            report.ranking.hit_at_1_rate,
            report.ranking.hit_at_3_rate,
            report.ranking.hit_at_5_rate,
            report.ranking.mean_reciprocal_rank
        );
        println!(
            "eval_matrix: status={} coverage={:.1}% stored={} auto={} covered={}/{} missing={:?}",
            report.eval_matrix.status,
            report.eval_matrix.coverage,
            report.eval_matrix.stored_cases,
            report.eval_matrix.auto_cases,
            report.eval_matrix.covered_dimensions,
            report.eval_matrix.total_dimensions,
            report.eval_matrix.missing_dimensions
        );
        println!(
            "retrieval_tuning: status={} profile={} selection_recall={:.1}% candidate_recall={:.1}% chunk_selection={:.1}% memory_selection={:.1}% semantic_fallbacks={:.1}%",
            report.retrieval_tuning.status,
            report.retrieval_tuning.selected_profile,
            report.retrieval_tuning.selection_recall,
            report.retrieval_tuning.candidate_recall,
            report.retrieval_tuning.chunk_selection_rate,
            report.retrieval_tuning.memory_selection_rate,
            report.retrieval_tuning.semantic_fallback_rate
        );
        println!(
            "split: development={}/{} ({:.1}%) holdout={}/{} ({:.1}%) extractive={:.1}% ready={} tuning_isolated={} origin_independence_verified={}",
            report.split.development_passed,
            report.split.development_total,
            report.split.development_recall,
            report.split.holdout_passed,
            report.split.holdout_total,
            report.split.holdout_recall,
            report.split.holdout_grounded_coverage,
            report.split.holdout_ready,
            report.split.tuning_isolation_enforced,
            report.split.origin_independence_verified
        );
        println!(
            "baseline: status={} present={} written={} regression={} path={} detail={}",
            report.baseline.status,
            report.baseline.present,
            report.baseline.written,
            report.baseline.regression,
            report.baseline.path,
            report.baseline.detail
        );
        for case in &report.cases {
            println!(
                "{}  {}  {}  rank={} confidence={} citations={}",
                if case.passed { "pass" } else { "fail" },
                case.id,
                case.name,
                case.expected_rank
                    .map(|rank| rank.to_string())
                    .unwrap_or_else(|| "missing".to_string()),
                case.confidence,
                case.citation_count
            );
            println!("  {}", case.detail);
            println!(
                "  packing: selected={}/{} chunks={}/{} suppressed_overlap={} suppressed_file_cap={} suppressed_limit={} expected={}",
                case.packing.selected_count,
                case.packing.candidate_count,
                case.packing.selected_chunks,
                case.packing.chunk_candidates,
                case.packing.suppressed_overlap,
                case.packing.suppressed_file_cap,
                case.packing.suppressed_limit,
                case.expected_evidence_status
            );
            println!(
                "  extractive_answer: {}  {}",
                if case.grounded_answer.passed {
                    "pass"
                } else {
                    "fail"
                },
                case.grounded_answer.detail
            );
        }
        for item in &report.recommendations {
            println!("recommendation: {item}");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_graph_rag_eval(
    conn: &Connection,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    gen_config: &crate::runtime_config::GenerationConfig,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = graph_rag_eval_report(
        conn, scope, limit, budget, gen_config, provider, endpoint, model,
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Graph RAG Eval");
        println!(
            "status: {} recall: {:.1}% grounded: {:.1}% passed: {}/{}",
            report.status, report.recall, report.grounded_coverage, report.passed, report.total
        );
        println!(
            "graph: nodes={} edges={} connected_cases={} isolated_cases={} avg_relationship_coverage={:.1}% kinds={:?}",
            report.graph.total_nodes,
            report.graph.total_edges,
            report.graph.connected_cases,
            report.graph.isolated_cases,
            report.graph.average_relationship_coverage,
            report.graph.relationship_kinds
        );
        for case in &report.cases {
            println!(
                "{}  {}  {}  graph={} confidence={} nodes={} edges={} coverage={:.1}%",
                if case.passed { "pass" } else { "fail" },
                case.id,
                case.name,
                case.graph_status,
                case.confidence,
                case.node_count,
                case.edge_count,
                case.relationship_coverage
            );
            println!("  {}", case.detail);
        }
        for item in &report.recommendations {
            println!("recommendation: {item}");
        }
    }
    Ok(())
}

pub(crate) fn rag_eval_report(
    conn: &Connection,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<RagEvalReport> {
    rag_eval_report_with_baseline(
        conn, scope, limit, budget, provider, endpoint, model, None, false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn rag_eval_report_with_baseline(
    conn: &Connection,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
    baseline_root: Option<&Path>,
    write_baseline: bool,
) -> Result<RagEvalReport> {
    let cases = load_rag_eval_cases(conn, budget)?;
    let corpus_signature = rag_eval_corpus_signature(&cases)?;
    let development_signature = rag_eval_split_corpus_signature(&cases, "development")?;
    let holdout_signature = rag_eval_split_corpus_signature(&cases, "holdout")?;
    let config_signature =
        rag_eval_config_signature(scope, limit, budget, provider, endpoint, model)?;
    let case_source = if cases.iter().any(|case| case.source == "stored") {
        "stored"
    } else if cases.is_empty() {
        "empty"
    } else {
        "auto"
    }
    .to_string();
    let mut results = Vec::new();
    for case in cases {
        let debug = memory_rag_debug_report(
            conn,
            &case.query,
            scope,
            limit,
            case.budget,
            provider,
            endpoint,
            model,
        )?;
        let expected_lower = case.expected.to_lowercase();
        let expected_rank = (!expected_lower.trim().is_empty())
            .then(|| {
                debug.source_pack.iter().position(|source| {
                    format!(
                        "{} {} {} {}",
                        source.id,
                        source.title,
                        source.summary,
                        source.reasons.join(" ")
                    )
                    .to_lowercase()
                    .contains(&expected_lower)
                })
            })
            .flatten()
            .map(|index| index + 1);
        let passed = expected_rank.is_some();
        let expected_suppressed_sources =
            rag_eval_expected_suppressed_sources(&case.expected, &debug.packing);
        let expected_suppressed_titles = expected_suppressed_sources
            .iter()
            .map(|source| source.title.clone())
            .collect::<Vec<_>>();
        let expected_suppressed_reasons = rag_eval_unique_suppressed_reasons(
            expected_suppressed_sources
                .iter()
                .map(|source| source.reason.as_str()),
        );
        let expected_evidence_status =
            rag_eval_expected_evidence_status(&case.expected, passed, &expected_suppressed_titles);
        let expected_in_candidates = passed || !expected_suppressed_titles.is_empty();
        let grounded_answer = rag_eval_grounded_answer(
            &case.query,
            &case.expected,
            passed,
            &debug.source_pack,
            &debug.missing_evidence,
        );
        results.push(RagEvalCaseResult {
            id: case.id,
            name: case.name,
            case_source: case.source,
            split: case.split,
            query: case.query,
            expected: case.expected,
            expected_rank,
            passed,
            detail: if passed {
                "expected text found in RAG source pack".to_string()
            } else if debug.source_pack.is_empty() {
                "RAG source pack is empty".to_string()
            } else {
                "expected text missing from RAG source pack".to_string()
            },
            confidence: debug.confidence,
            confidence_score: debug.confidence_score,
            citation_count: debug.citation_count,
            citations: debug.citations,
            source_titles: debug
                .source_pack
                .iter()
                .map(|source| source.title.clone())
                .collect(),
            packing: debug.packing,
            expected_evidence_status,
            expected_in_candidates,
            expected_suppressed_titles,
            expected_suppressed_reasons,
            semantic_used: debug.semantic_used,
            semantic_error: debug.semantic_error,
            missing_evidence: debug.missing_evidence,
            grounded_answer,
        });
    }
    let total = results.len();
    let passed = results.iter().filter(|case| case.passed).count();
    let failed = total.saturating_sub(passed);
    let recall = eval_ratio_percent(passed, total);
    let average_confidence = if total == 0 {
        0.0
    } else {
        (results
            .iter()
            .map(|case| case.confidence_score)
            .sum::<f64>()
            / total as f64
            * 100.0)
            .round()
            / 100.0
    };
    let semantic_used = results.iter().filter(|case| case.semantic_used).count();
    let semantic_fallbacks = results
        .iter()
        .filter(|case| case.semantic_error.is_some())
        .count();
    let packing = rag_eval_packing_summary(&results);
    let evidence_placement = rag_eval_evidence_placement_summary(&results);
    let grounded_answers = rag_eval_grounded_summary(&results);
    let ranking = rag_eval_ranking_summary(&results);
    let generated_output_guard = rag_generated_answer_guard_benchmark();
    let evaluation_layers = RagEvalLayersSummary {
        protocol_version: RAG_EVAL_PROTOCOL_VERSION,
        retrieval: RagEvalRetrievalLayer {
            evaluation_kind: "retrieval_ranking".to_string(),
            cases: total,
            passed,
            recall,
            hit_at_3_rate: ranking.hit_at_3_rate,
            mean_reciprocal_rank: ranking.mean_reciprocal_rank,
        },
        extractive_grounding: RagEvalExtractiveLayer {
            evaluation_kind: "deterministic_extractive_grounding".to_string(),
            live_model_executed: false,
            cases: total,
            passed: grounded_answers.passed,
            coverage: grounded_answers.coverage,
            unknown_citation_cases: grounded_answers.unknown_citation_cases,
        },
        generated_output_guard: RagEvalGeneratedOutputLayer {
            evaluation_kind: "versioned_synthetic_output_fixture".to_string(),
            live_model_executed: false,
            fixture_version: generated_output_guard.fixture_version,
            attack_vectors: generated_output_guard.attack_vectors,
            passed: generated_output_guard.passed,
            total: generated_output_guard.total,
            false_accepts: generated_output_guard.false_accepts,
            false_rejects: generated_output_guard.false_rejects,
        },
    };
    let eval_matrix = rag_eval_matrix_summary(&results);
    let retrieval_tuning = rag_eval_retrieval_tuning_summary(
        &results,
        &evidence_placement,
        &packing,
        semantic_fallbacks,
    );
    let mut split = rag_eval_split_summary(&results);
    split.development_signature = development_signature;
    split.holdout_signature = holdout_signature;
    let baseline = rag_eval_baseline_summary(
        baseline_root,
        write_baseline,
        &RagEvalBaselineInput {
            total,
            passed,
            recall,
            grounded_coverage: grounded_answers.coverage,
            eval_matrix: &eval_matrix,
            retrieval_tuning: &retrieval_tuning,
            ranking: &ranking,
            split: &split,
            corpus_signature: &corpus_signature,
            config_signature: &config_signature,
        },
    )?;
    let mut recommendations = Vec::new();
    if total == 0 {
        recommendations
            .push("add eval cases with `dukememory eval add-case NAME QUERY EXPECTED`".to_string());
    } else if case_source == "auto" {
        recommendations.push(
            "add stored eval cases for project-critical questions before release gating"
                .to_string(),
        );
    }
    if failed > 0 {
        recommendations.push(
            "inspect failing cases with `dukememory rag-debug QUERY --json` before changing generation".to_string(),
        );
    }
    if semantic_fallbacks > 0 {
        recommendations.push(
            "refresh embeddings or provider health before trusting semantic RAG scores".to_string(),
        );
    }
    if grounded_answers.failed > 0 {
        recommendations.push(
            "inspect grounded_answer fields: retrieval found evidence that did not make it into the deterministic extractive answer".to_string(),
        );
    }
    if generated_output_guard.passed < generated_output_guard.total {
        recommendations.push(
            "fix generated-output guard fixture regressions before running or accepting live model generation"
                .to_string(),
        );
    }
    if ranking.hit_at_3_rate < 80.0 {
        recommendations.push(format!(
            "expected evidence reaches the top 3 in only {:.1}% of cases; tune ranking before expanding context budgets",
            ranking.hit_at_3_rate
        ));
    }
    if evidence_placement.near_miss_count > 0 {
        recommendations.push(
            "inspect expected_suppressed_reasons: expected evidence was retrievable but suppressed by source packing".to_string(),
        );
    }
    if evidence_placement.missing_from_candidates > 0 {
        recommendations.push(
            "ingest or relink source chunks for cases where expected evidence is missing from candidates".to_string(),
        );
    }
    if eval_matrix.status == "auto_only" {
        recommendations.push(
            "promote representative auto eval cases into stored project-critical RAG eval cases"
                .to_string(),
        );
    }
    if eval_matrix.stored_cases > 0
        && eval_matrix.stored_cases < eval_matrix.recommended_min_stored_cases
    {
        recommendations.push(format!(
            "expand RAG eval matrix to at least {} stored cases before release confidence claims",
            eval_matrix.recommended_min_stored_cases
        ));
    }
    if split.holdout_total < split.recommended_min_holdout_cases {
        recommendations.push(format!(
            "add at least {} independent holdout RAG cases with `eval add-case --split holdout`; current holdout has {}",
            split.recommended_min_holdout_cases, split.holdout_total
        ));
    } else if !split.holdout_ready {
        recommendations.push(
            "holdout RAG cases are failing; tune only on development cases, then rerun the untouched holdout"
                .to_string(),
        );
    }
    if !eval_matrix.missing_dimensions.is_empty() {
        recommendations.push(format!(
            "add RAG eval cases for missing matrix dimensions: {}",
            eval_matrix.missing_dimensions.join(", ")
        ));
    }
    if baseline.status == "missing" {
        recommendations.push(
            "write a RAG eval matrix baseline with `dukememory eval rag --write-baseline --json` after reviewing cases"
                .to_string(),
        );
    }
    if baseline.regression {
        recommendations.push("RAG eval regressed against baseline; inspect failed cases, grounded answers, and matrix coverage before release".to_string());
    }
    for reason in &retrieval_tuning.reasons {
        if retrieval_tuning.status != "ready" {
            recommendations.push(format!("retrieval tuning: {reason}"));
        }
    }
    let ok = total > 0
        && failed == 0
        && grounded_answers.failed == 0
        && generated_output_guard.passed == generated_output_guard.total;
    Ok(RagEvalReport {
        version: 7,
        ok,
        status: if ok {
            "ready"
        } else if total == 0 {
            "empty"
        } else {
            "attention"
        }
        .to_string(),
        case_source,
        total,
        passed,
        failed,
        recall,
        average_confidence,
        semantic_used,
        semantic_fallbacks,
        packing,
        evidence_placement,
        evaluation_layers,
        grounded_answers,
        ranking,
        eval_matrix,
        retrieval_tuning,
        split,
        baseline,
        cases: results,
        recommendations,
    })
}

struct RagEvalBaselineInput<'a> {
    total: usize,
    passed: usize,
    recall: f64,
    grounded_coverage: f64,
    eval_matrix: &'a RagEvalMatrixSummary,
    retrieval_tuning: &'a RagEvalRetrievalTuningSummary,
    ranking: &'a RagEvalRankingSummary,
    split: &'a RagEvalSplitSummary,
    corpus_signature: &'a str,
    config_signature: &'a str,
}

fn rag_eval_baseline_summary(
    baseline_root: Option<&Path>,
    write_baseline: bool,
    input: &RagEvalBaselineInput<'_>,
) -> Result<RagEvalBaselineSummary> {
    let current = rag_eval_baseline_file(input)?;
    let Some(root) = baseline_root else {
        return Ok(RagEvalBaselineSummary {
            status: "unconfigured".to_string(),
            path: String::new(),
            present: false,
            written: false,
            regression: false,
            current_signature: current.signature,
            baseline_signature: None,
            baseline_recall: None,
            baseline_grounded_coverage: None,
            baseline_matrix_coverage: None,
            baseline_candidate_recall: None,
            baseline_selection_recall: None,
            baseline_hit_at_3_rate: None,
            baseline_mean_reciprocal_rank: None,
            detail: "no project root was supplied for RAG eval baseline comparison".to_string(),
        });
    };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = root.join(".agent/rag-eval-baseline.json");
    if write_baseline {
        write_file(&path, serde_json::to_string_pretty(&current)?.as_bytes())?;
        return Ok(RagEvalBaselineSummary {
            status: "written".to_string(),
            path: path.display().to_string(),
            present: true,
            written: true,
            regression: false,
            current_signature: current.signature.clone(),
            baseline_signature: Some(current.signature),
            baseline_recall: Some(current.recall),
            baseline_grounded_coverage: Some(current.grounded_coverage),
            baseline_matrix_coverage: Some(current.matrix_coverage),
            baseline_candidate_recall: Some(current.candidate_recall),
            baseline_selection_recall: Some(current.selection_recall),
            baseline_hit_at_3_rate: Some(current.hit_at_3_rate),
            baseline_mean_reciprocal_rank: Some(current.mean_reciprocal_rank),
            detail: "wrote current RAG eval matrix baseline".to_string(),
        });
    }

    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(RagEvalBaselineSummary {
            status: "missing".to_string(),
            path: path.display().to_string(),
            present: false,
            written: false,
            regression: false,
            current_signature: current.signature,
            baseline_signature: None,
            baseline_recall: None,
            baseline_grounded_coverage: None,
            baseline_matrix_coverage: None,
            baseline_candidate_recall: None,
            baseline_selection_recall: None,
            baseline_hit_at_3_rate: None,
            baseline_mean_reciprocal_rank: None,
            detail: "no RAG eval baseline has been written for this project".to_string(),
        });
    };
    let Ok(baseline) = serde_json::from_str::<RagEvalBaselineFile>(&raw) else {
        return Ok(RagEvalBaselineSummary {
            status: "invalid".to_string(),
            path: path.display().to_string(),
            present: true,
            written: false,
            regression: false,
            current_signature: current.signature,
            baseline_signature: None,
            baseline_recall: None,
            baseline_grounded_coverage: None,
            baseline_matrix_coverage: None,
            baseline_candidate_recall: None,
            baseline_selection_recall: None,
            baseline_hit_at_3_rate: None,
            baseline_mean_reciprocal_rank: None,
            detail: "RAG eval baseline file exists but could not be parsed".to_string(),
        });
    };

    let corpus_changed = baseline.corpus_signature.is_empty()
        || current.corpus_signature != baseline.corpus_signature;
    let config_changed = baseline.config_signature.is_empty()
        || current.config_signature != baseline.config_signature;
    let comparable = !corpus_changed && !config_changed;
    let regression = comparable
        && (current.recall + 0.1 < baseline.recall
            || current.grounded_coverage + 0.1 < baseline.grounded_coverage
            || current.matrix_coverage + 0.1 < baseline.matrix_coverage
            || current.candidate_recall + 0.1 < baseline.candidate_recall
            || current.selection_recall + 0.1 < baseline.selection_recall
            || current.hit_at_3_rate + 5.0 < baseline.hit_at_3_rate
            || current.mean_reciprocal_rank + 5.0 < baseline.mean_reciprocal_rank
            || current.holdout_recall + 0.1 < baseline.holdout_recall
            || current.holdout_grounded_coverage + 0.1 < baseline.holdout_grounded_coverage
            || current.holdout_total < baseline.holdout_total
            || current.passed < baseline.passed
            || current.covered_dimensions < baseline.covered_dimensions);
    let status = if corpus_changed {
        "corpus_changed"
    } else if config_changed {
        "config_changed"
    } else if regression {
        "regressed"
    } else if current.signature == baseline.signature {
        "matched"
    } else {
        "changed"
    }
    .to_string();
    let detail = if corpus_changed {
        format!(
            "RAG eval corpus changed (current {}, baseline {}); review cases and write a new baseline",
            current.corpus_signature,
            if baseline.corpus_signature.is_empty() {
                "legacy"
            } else {
                &baseline.corpus_signature
            }
        )
    } else if config_changed {
        format!(
            "RAG eval configuration changed (current {}, baseline {}); rerun and accept a new baseline",
            current.config_signature,
            if baseline.config_signature.is_empty() {
                "legacy"
            } else {
                &baseline.config_signature
            }
        )
    } else if regression {
        format!(
            "current recall {:.1}% / hit@3 {:.1}% / MRR {:.1}% is below baseline recall {:.1}% / hit@3 {:.1}% / MRR {:.1}%",
            current.recall,
            current.hit_at_3_rate,
            current.mean_reciprocal_rank,
            baseline.recall,
            baseline.hit_at_3_rate,
            baseline.mean_reciprocal_rank
        )
    } else if current.signature == baseline.signature {
        "current RAG eval matrix matches baseline".to_string()
    } else {
        "current RAG eval matrix differs from baseline without metric regression".to_string()
    };
    Ok(RagEvalBaselineSummary {
        status,
        path: path.display().to_string(),
        present: true,
        written: false,
        regression,
        current_signature: current.signature,
        baseline_signature: Some(baseline.signature),
        baseline_recall: Some(baseline.recall),
        baseline_grounded_coverage: Some(baseline.grounded_coverage),
        baseline_matrix_coverage: Some(baseline.matrix_coverage),
        baseline_candidate_recall: Some(baseline.candidate_recall),
        baseline_selection_recall: Some(baseline.selection_recall),
        baseline_hit_at_3_rate: Some(baseline.hit_at_3_rate),
        baseline_mean_reciprocal_rank: Some(baseline.mean_reciprocal_rank),
        detail,
    })
}

fn rag_eval_baseline_file(input: &RagEvalBaselineInput<'_>) -> Result<RagEvalBaselineFile> {
    let RagEvalBaselineInput {
        total,
        passed,
        recall,
        grounded_coverage,
        eval_matrix,
        retrieval_tuning,
        ranking,
        split,
        corpus_signature,
        config_signature,
    } = input;
    let payload = json!({
        "total": total,
        "passed": passed,
        "recall": recall,
        "grounded_coverage": grounded_coverage,
        "matrix_coverage": eval_matrix.coverage,
        "covered_dimensions": eval_matrix.covered_dimensions,
        "dimensions": eval_matrix.dimensions,
        "candidate_recall": retrieval_tuning.candidate_recall,
        "selection_recall": retrieval_tuning.selection_recall,
        "hit_at_3_rate": ranking.hit_at_3_rate,
        "mean_reciprocal_rank": ranking.mean_reciprocal_rank,
        "holdout_total": split.holdout_total,
        "holdout_recall": split.holdout_recall,
        "holdout_grounded_coverage": split.holdout_grounded_coverage,
        "corpus_signature": corpus_signature,
        "config_signature": config_signature,
    });
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&payload)?);
    Ok(RagEvalBaselineFile {
        version: 3,
        signature: format!("{:x}", hasher.finalize())[..16].to_string(),
        corpus_signature: corpus_signature.to_string(),
        config_signature: config_signature.to_string(),
        total: *total,
        passed: *passed,
        recall: *recall,
        grounded_coverage: *grounded_coverage,
        matrix_coverage: eval_matrix.coverage,
        candidate_recall: retrieval_tuning.candidate_recall,
        selection_recall: retrieval_tuning.selection_recall,
        hit_at_3_rate: ranking.hit_at_3_rate,
        mean_reciprocal_rank: ranking.mean_reciprocal_rank,
        holdout_total: split.holdout_total,
        holdout_recall: split.holdout_recall,
        holdout_grounded_coverage: split.holdout_grounded_coverage,
        covered_dimensions: eval_matrix.covered_dimensions,
        dimensions: eval_matrix.dimensions.clone(),
        written_at: now_ms(),
    })
}

fn rag_eval_corpus_signature(cases: &[RagEvalCase]) -> Result<String> {
    let mut canonical_cases = cases
        .iter()
        .map(|case| {
            json!({
                "id": case.id,
                "name": case.name,
                "query": case.query,
                "expected": case.expected,
                "budget": case.budget,
                "source": case.source,
                "split": case.split,
            })
        })
        .collect::<Vec<_>>();
    canonical_cases.sort_by_key(|case| serde_json::to_string(case).unwrap_or_default());
    short_eval_signature(&canonical_cases)
}

fn rag_eval_split_corpus_signature(cases: &[RagEvalCase], split: &str) -> Result<String> {
    let mut canonical_cases = cases
        .iter()
        .filter(|case| case.split == split)
        .map(|case| {
            json!({
                "id": case.id,
                "name": case.name,
                "query": case.query,
                "expected": case.expected,
                "budget": case.budget,
                "source": case.source,
                "split": case.split,
            })
        })
        .collect::<Vec<_>>();
    canonical_cases.sort_by_key(|case| serde_json::to_string(case).unwrap_or_default());
    short_eval_signature(&canonical_cases)
}

fn rag_eval_config_signature(
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    short_eval_signature(&json!({
        "protocol_version": RAG_EVAL_PROTOCOL_VERSION,
        "scope": scope,
        "limit": limit,
        "budget": budget,
        "provider": provider,
        "endpoint": endpoint,
        "model": model,
    }))
}

fn short_eval_signature(payload: &impl Serialize) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(payload)?);
    Ok(format!("{:x}", hasher.finalize())[..16].to_string())
}

pub(crate) fn rag_eval_baseline_blocks_release(status: &str) -> bool {
    matches!(
        status,
        "invalid" | "regressed" | "changed" | "corpus_changed" | "config_changed"
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn graph_rag_eval_report(
    conn: &Connection,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    _gen_config: &crate::runtime_config::GenerationConfig,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<GraphRagEvalReport> {
    let cases = load_graph_rag_eval_cases(conn, budget)?;
    let case_source = if cases.iter().any(|case| case.source == "stored_graph") {
        "stored_graph"
    } else if cases.is_empty() {
        "empty"
    } else {
        "auto_graph"
    }
    .to_string();
    let mut results = Vec::new();
    let eval_generation = crate::runtime_config::GenerationConfig {
        provider: "mock".to_string(),
        endpoint: "local".to_string(),
        model: "extractive-fallback".to_string(),
    };

    for case in cases {
        let report = crate::app::graph_rag::compute_graph_rag(
            conn,
            &case.query,
            scope,
            limit,
            case.budget,
            &eval_generation,
            provider,
            endpoint,
            model,
        )?;
        let expected = case.expected.trim().to_lowercase();
        let graph_haystack = graph_rag_eval_haystack(&report);
        let expected_in_graph = !expected.is_empty() && graph_haystack.contains(&expected);
        let answer_lower = report.answer.to_lowercase();
        let expected_in_answer = !expected.is_empty() && answer_lower.contains(&expected);
        let graph_connected =
            report.graph_summary.edge_count > 0 && report.graph_summary.connected_node_count > 0;
        let passed =
            expected_in_graph && expected_in_answer && report.citation_count > 0 && graph_connected;
        let detail = if passed {
            "expected evidence is present in connected graph nodes and cited answer"
        } else if !expected_in_graph {
            "expected evidence is missing from selected graph nodes and relationships"
        } else if !graph_connected {
            "expected evidence was selected but graph relationships are missing"
        } else if !expected_in_answer {
            "expected evidence was selected but missing from graph answer"
        } else if report.citation_count == 0 {
            "graph answer did not cite selected memory nodes"
        } else {
            "graph eval failed an unknown grounding check"
        }
        .to_string();
        results.push(GraphRagEvalCaseResult {
            id: case.id,
            name: case.name,
            case_source: case.source,
            query: case.query,
            expected: case.expected,
            passed,
            detail,
            graph_status: report.graph_summary.status,
            confidence: report.confidence,
            confidence_score: report.confidence_score,
            node_count: report.graph_summary.node_count,
            edge_count: report.graph_summary.edge_count,
            relationship_coverage: report.graph_summary.relationship_coverage,
            relationship_kinds: report.graph_summary.relationship_kinds,
            expected_in_graph,
            expected_in_answer,
            citation_count: report.citation_count,
            citations: report.citations,
            answer: report.answer,
            ranked_node_titles: report
                .ranked_nodes
                .iter()
                .map(|node| node.title.clone())
                .collect(),
            missing_evidence: report.missing_evidence,
        });
    }

    let total = results.len();
    let passed = results.iter().filter(|case| case.passed).count();
    let failed = total.saturating_sub(passed);
    let recall = eval_ratio_percent(passed, total);
    let grounded_coverage = eval_ratio_percent(
        results
            .iter()
            .filter(|case| case.expected_in_answer && case.citation_count > 0)
            .count(),
        total,
    );
    let graph = graph_rag_eval_graph_summary(&results);
    let mut recommendations = Vec::new();
    if total == 0 {
        recommendations.push(
            "add graph-focused eval cases or memory links before relying on graph-rag eval"
                .to_string(),
        );
    } else if case_source == "auto_graph" {
        recommendations.push(
            "add stored graph eval cases for project-critical relationship questions".to_string(),
        );
    }
    if failed > 0 {
        recommendations.push(
            "inspect failing graph cases with `dukememory graph-rag QUERY --json`".to_string(),
        );
    }
    if graph.missing_graph_cases > 0 || graph.isolated_cases > 0 {
        recommendations.push(
            "add or repair memory links for graph cases with isolated selected nodes".to_string(),
        );
    }
    let ok = total > 0 && failed == 0;
    Ok(GraphRagEvalReport {
        version: 1,
        ok,
        status: if ok {
            "ready"
        } else if total == 0 {
            "empty"
        } else {
            "attention"
        }
        .to_string(),
        case_source,
        total,
        passed,
        failed,
        recall,
        grounded_coverage,
        graph,
        cases: results,
        recommendations,
    })
}

fn graph_rag_eval_haystack(report: &crate::app::graph_rag::GraphRagReport) -> String {
    let mut parts = Vec::new();
    parts.push(report.answer.clone());
    for node in &report.ranked_nodes {
        parts.push(format!(
            "{} {} {} {} {}",
            node.id, node.title, node.memory_type, node.status, node.summary
        ));
    }
    for edge in &report.relevant_edges {
        parts.push(format!("{} {} {}", edge.source, edge.kind, edge.target));
    }
    parts.join("\n").to_lowercase()
}

fn graph_rag_eval_graph_summary(cases: &[GraphRagEvalCaseResult]) -> GraphRagEvalGraphSummary {
    let mut summary = GraphRagEvalGraphSummary::default();
    for case in cases {
        summary.total_nodes += case.node_count;
        summary.total_edges += case.edge_count;
        if case.edge_count > 0 {
            summary.connected_cases += 1;
        } else if case.node_count > 0 {
            summary.isolated_cases += 1;
        } else {
            summary.missing_graph_cases += 1;
        }
        for (kind, count) in &case.relationship_kinds {
            *summary.relationship_kinds.entry(kind.clone()).or_insert(0) += count;
        }
    }
    if !cases.is_empty() {
        summary.average_relationship_coverage = ((cases
            .iter()
            .map(|case| case.relationship_coverage)
            .sum::<f64>()
            / cases.len() as f64)
            * 10.0)
            .round()
            / 10.0;
        summary.average_edge_density = ((cases
            .iter()
            .map(|case| {
                if case.node_count <= 1 {
                    0.0
                } else {
                    case.edge_count as f64
                        / case.node_count.saturating_mul(case.node_count - 1) as f64
                }
            })
            .sum::<f64>()
            / cases.len() as f64)
            * 1000.0)
            .round()
            / 1000.0;
    }
    summary
}

fn rag_eval_packing_summary(cases: &[RagEvalCaseResult]) -> RagEvalPackingSummary {
    let mut summary = RagEvalPackingSummary::default();
    for case in cases {
        summary.candidate_count += case.packing.candidate_count;
        summary.selected_count += case.packing.selected_count;
        summary.memory_candidates += case.packing.memory_candidates;
        summary.chunk_candidates += case.packing.chunk_candidates;
        summary.selected_memories += case.packing.selected_memories;
        summary.selected_chunks += case.packing.selected_chunks;
        summary.suppressed_duplicate += case.packing.suppressed_duplicate;
        summary.suppressed_overlap += case.packing.suppressed_overlap;
        summary.suppressed_file_cap += case.packing.suppressed_file_cap;
        summary.suppressed_limit += case.packing.suppressed_limit;
        summary.suppressed_sources += case.packing.suppressed_sources.len();
        match case.expected_evidence_status.as_str() {
            "selected" => summary.expected_selected += 1,
            "suppressed_by_packing" => summary.expected_suppressed_by_packing += 1,
            "missing_from_candidates" => summary.expected_missing_from_candidates += 1,
            _ => {}
        }
    }
    summary
}

fn rag_eval_evidence_placement_summary(
    cases: &[RagEvalCaseResult],
) -> RagEvalEvidencePlacementSummary {
    let mut summary = RagEvalEvidencePlacementSummary::default();
    for case in cases {
        match case.expected_evidence_status.as_str() {
            "selected" => summary.selected += 1,
            "suppressed_by_packing" => {
                summary.suppressed_by_packing += 1;
                for reason in &case.expected_suppressed_reasons {
                    *summary
                        .suppression_reasons
                        .entry(reason.clone())
                        .or_insert(0) += 1;
                }
            }
            "missing_from_candidates" => summary.missing_from_candidates += 1,
            "empty_expected" => summary.empty_expected += 1,
            _ => {}
        }
    }
    summary.expected_total = cases.len().saturating_sub(summary.empty_expected);
    summary.selection_recall = eval_ratio_percent(summary.selected, summary.expected_total);
    summary.candidate_recall = eval_ratio_percent(
        summary.selected + summary.suppressed_by_packing,
        summary.expected_total,
    );
    summary.near_miss_count = summary.suppressed_by_packing;
    summary
}

fn rag_eval_grounded_summary(cases: &[RagEvalCaseResult]) -> RagEvalGroundedSummary {
    let passed = cases
        .iter()
        .filter(|case| case.grounded_answer.passed)
        .count();
    let total = cases.len();
    RagEvalGroundedSummary {
        passed,
        failed: total.saturating_sub(passed),
        coverage: eval_ratio_percent(passed, total),
        expected_in_answer: cases
            .iter()
            .filter(|case| case.grounded_answer.expected_found)
            .count(),
        cited_answers: cases
            .iter()
            .filter(|case| case.grounded_answer.citation_count > 0)
            .count(),
        unknown_citation_cases: cases
            .iter()
            .filter(|case| !case.grounded_answer.unknown_citations.is_empty())
            .count(),
    }
}

fn rag_eval_ranking_summary(cases: &[RagEvalCaseResult]) -> RagEvalRankingSummary {
    let total = cases.len();
    let hit_at_1 = cases
        .iter()
        .filter(|case| case.expected_rank.is_some_and(|rank| rank <= 1))
        .count();
    let hit_at_3 = cases
        .iter()
        .filter(|case| case.expected_rank.is_some_and(|rank| rank <= 3))
        .count();
    let hit_at_5 = cases
        .iter()
        .filter(|case| case.expected_rank.is_some_and(|rank| rank <= 5))
        .count();
    let mean_reciprocal_rank = if total == 0 {
        0.0
    } else {
        (cases
            .iter()
            .filter_map(|case| case.expected_rank)
            .map(|rank| 1.0 / rank as f64)
            .sum::<f64>()
            / total as f64
            * 1_000.0)
            .round()
            / 10.0
    };
    RagEvalRankingSummary {
        total,
        hit_at_1,
        hit_at_3,
        hit_at_5,
        hit_at_1_rate: eval_ratio_percent(hit_at_1, total),
        hit_at_3_rate: eval_ratio_percent(hit_at_3, total),
        hit_at_5_rate: eval_ratio_percent(hit_at_5, total),
        mean_reciprocal_rank,
    }
}

fn rag_eval_split_summary(cases: &[RagEvalCaseResult]) -> RagEvalSplitSummary {
    let development = cases
        .iter()
        .filter(|case| case.split == "development")
        .collect::<Vec<_>>();
    let holdout = cases
        .iter()
        .filter(|case| case.split == "holdout")
        .collect::<Vec<_>>();
    let development_passed = development.iter().filter(|case| case.passed).count();
    let holdout_passed = holdout.iter().filter(|case| case.passed).count();
    let holdout_grounded = holdout
        .iter()
        .filter(|case| case.grounded_answer.passed)
        .count();
    let holdout_total = holdout.len();
    RagEvalSplitSummary {
        development_total: development.len(),
        development_passed,
        development_recall: eval_ratio_percent(development_passed, development.len()),
        holdout_total,
        holdout_passed,
        holdout_recall: eval_ratio_percent(holdout_passed, holdout_total),
        holdout_grounded_coverage: eval_ratio_percent(holdout_grounded, holdout_total),
        recommended_min_holdout_cases: RAG_EVAL_RECOMMENDED_HOLDOUT_CASES,
        holdout_ready: holdout_total >= RAG_EVAL_RECOMMENDED_HOLDOUT_CASES
            && holdout_passed == holdout_total
            && holdout_grounded == holdout_total,
        tuning_isolation_enforced: true,
        holdout_policy: "labelled holdout is evaluated after retrieval configuration is fixed; evaluation never mutates ranking"
            .to_string(),
        origin_independence_verified: false,
        development_signature: String::new(),
        holdout_signature: String::new(),
    }
}

fn rag_eval_matrix_summary(cases: &[RagEvalCaseResult]) -> RagEvalMatrixSummary {
    let mut dimensions = RAG_EVAL_MATRIX_DIMENSIONS
        .iter()
        .map(|dimension| (dimension.to_string(), 0usize))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut stored_cases = 0usize;
    let mut auto_cases = 0usize;

    for case in cases {
        if case.case_source == "stored" {
            stored_cases += 1;
        } else {
            auto_cases += 1;
        }
        for dimension in rag_eval_case_dimensions(case) {
            *dimensions.entry(dimension.to_string()).or_insert(0) += 1;
        }
    }

    let missing_dimensions = RAG_EVAL_MATRIX_DIMENSIONS
        .iter()
        .filter(|dimension| dimensions.get(**dimension).copied().unwrap_or_default() == 0)
        .map(|dimension| dimension.to_string())
        .collect::<Vec<_>>();
    let total_dimensions = RAG_EVAL_MATRIX_DIMENSIONS.len();
    let covered_dimensions = total_dimensions.saturating_sub(missing_dimensions.len());
    let coverage = eval_ratio_percent(covered_dimensions, total_dimensions);
    let status = if cases.is_empty() {
        "empty"
    } else if stored_cases == 0 {
        "auto_only"
    } else if !missing_dimensions.is_empty() {
        "partial"
    } else {
        "ready"
    }
    .to_string();

    RagEvalMatrixSummary {
        status,
        stored_cases,
        auto_cases,
        recommended_min_stored_cases: RAG_EVAL_RECOMMENDED_STORED_CASES,
        total_dimensions,
        covered_dimensions,
        coverage,
        dimensions,
        missing_dimensions,
    }
}

fn rag_eval_case_dimensions(case: &RagEvalCaseResult) -> Vec<&'static str> {
    let mut dimensions = Vec::new();
    let text = format!(
        "{} {} {} {}",
        case.query,
        case.expected,
        case.source_titles.join(" "),
        case.citations.join(" ")
    )
    .to_lowercase();

    if case.packing.chunk_candidates > 0
        || case.packing.selected_chunks > 0
        || case.source_titles.iter().any(|title| {
            title.contains(".rs")
                || title.contains(".md")
                || title.contains(".toml")
                || title.contains(':')
        })
    {
        dimensions.push("source_chunk");
    }
    if case.packing.memory_candidates > 0 || case.packing.selected_memories > 0 {
        dimensions.push("memory_card");
    }
    if text.contains("dukememory")
        || text.contains("rag-ingest")
        || text.contains(" --")
        || text.contains(" cli")
    {
        dimensions.push("cli_workflow");
    }
    if text.contains("mcp") || text.contains("memory_") || text.contains("agent-session") {
        dimensions.push("mcp_tooling");
    }
    if text.contains("http")
        || text.contains("endpoint")
        || text.contains("/web-control")
        || text.contains(" get ")
        || text.contains(" post ")
    {
        dimensions.push("http_api");
    }
    if text.contains("graph")
        || text.contains("relationship")
        || text.contains("edge")
        || text.contains("node")
    {
        dimensions.push("graph_memory");
    }
    if text
        .chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch))
    {
        dimensions.push("multilingual");
    }
    if !case.passed
        || case.expected_evidence_status == "missing_from_candidates"
        || text.contains("missing")
        || text.contains("нет ")
        || text.contains("не ")
    {
        dimensions.push("negative_or_missing");
    }
    if case.expected_evidence_status == "suppressed_by_packing"
        || !case.expected_suppressed_reasons.is_empty()
        || !case.packing.suppressed_sources.is_empty()
    {
        dimensions.push("packing_near_miss");
    }

    dimensions.sort_unstable();
    dimensions.dedup();
    dimensions
}

fn rag_eval_retrieval_tuning_summary(
    cases: &[RagEvalCaseResult],
    evidence: &RagEvalEvidencePlacementSummary,
    packing: &RagEvalPackingSummary,
    semantic_fallbacks: usize,
) -> RagEvalRetrievalTuningSummary {
    let semantic_fallback_rate = eval_ratio_percent(semantic_fallbacks, cases.len());
    let chunk_selection_rate =
        eval_ratio_percent(packing.selected_chunks, packing.chunk_candidates);
    let memory_selection_rate =
        eval_ratio_percent(packing.selected_memories, packing.memory_candidates);
    let mut selected_profile = "balanced".to_string();
    let mut status = "ready".to_string();
    let mut reasons = Vec::new();

    if cases.is_empty() {
        return RagEvalRetrievalTuningSummary {
            status: "unconfigured".to_string(),
            selected_profile,
            candidate_recall: evidence.candidate_recall,
            selection_recall: evidence.selection_recall,
            chunk_selection_rate,
            memory_selection_rate,
            semantic_fallback_rate,
            near_miss_count: evidence.near_miss_count,
            reasons: vec!["no eval cases are available for retrieval tuning".to_string()],
        };
    }

    if semantic_fallbacks > 0 {
        status = "attention".to_string();
        selected_profile = "recall_heavy".to_string();
        reasons.push(
            "semantic fallback occurred during eval; refresh embeddings/provider health"
                .to_string(),
        );
    }
    if evidence.missing_from_candidates > 0 {
        status = "attention".to_string();
        selected_profile = "recall_heavy".to_string();
        reasons.push("expected evidence is missing from candidates; broaden retrieval or ingest missing chunks".to_string());
    }
    if evidence.near_miss_count > 0 {
        status = "attention".to_string();
        selected_profile = "recall_heavy".to_string();
        reasons.push(
            "expected evidence appears in candidates but is suppressed by packing".to_string(),
        );
    }
    if evidence.selection_recall < 90.0 {
        status = "attention".to_string();
        selected_profile = "recall_heavy".to_string();
        reasons.push(format!(
            "selection recall {:.1}% is below the 90% tuning target",
            evidence.selection_recall
        ));
    }
    if status == "ready" && chunk_selection_rate < 20.0 && packing.chunk_candidates >= 5 {
        selected_profile = "precision_heavy".to_string();
        reasons.push("chunk pool is broad while selected evidence remains complete".to_string());
    }
    if reasons.is_empty() {
        reasons.push("eval retrieval signals are balanced".to_string());
    }

    RagEvalRetrievalTuningSummary {
        status,
        selected_profile,
        candidate_recall: evidence.candidate_recall,
        selection_recall: evidence.selection_recall,
        chunk_selection_rate,
        memory_selection_rate,
        semantic_fallback_rate,
        near_miss_count: evidence.near_miss_count,
        reasons,
    }
}

fn rag_eval_grounded_answer(
    query: &str,
    expected: &str,
    source_pack_passed: bool,
    source_pack: &[RagSource],
    missing_evidence: &[String],
) -> RagEvalGroundedAnswer {
    let answer = rag_extractive_answer(query, source_pack, missing_evidence);
    let expected = expected.trim();
    let expected_found =
        !expected.is_empty() && answer.to_lowercase().contains(&expected.to_lowercase());
    let citations = rag_eval_answer_source_citations(&answer, source_pack);
    let unknown_citations = rag_eval_unknown_answer_citations(&answer, source_pack);
    let passed = source_pack_passed
        && expected_found
        && !citations.is_empty()
        && unknown_citations.is_empty();
    let detail = if passed {
        "expected evidence is present in a cited grounded answer".to_string()
    } else if !source_pack_passed {
        "expected evidence was not selected into the source pack".to_string()
    } else if !expected_found {
        "expected evidence was selected but missing from the grounded answer".to_string()
    } else if citations.is_empty() {
        "grounded answer did not cite any selected source".to_string()
    } else if !unknown_citations.is_empty() {
        "grounded answer contains citation ids outside the selected source pack".to_string()
    } else {
        "grounded answer failed an unknown grounding check".to_string()
    };
    RagEvalGroundedAnswer {
        passed,
        detail,
        answer,
        expected_found,
        citation_count: citations.len(),
        citations,
        unknown_citations,
    }
}

fn rag_eval_answer_source_citations(answer: &str, source_pack: &[RagSource]) -> Vec<String> {
    source_pack
        .iter()
        .filter(|source| rag_eval_answer_mentions_id(answer, &source.id))
        .map(|source| source.id.clone())
        .collect()
}

fn rag_eval_unknown_answer_citations(answer: &str, source_pack: &[RagSource]) -> Vec<String> {
    let source_ids = source_pack
        .iter()
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    rag_eval_bracketed_citations(answer)
        .into_iter()
        .filter(|id| !source_ids.contains(id.as_str()))
        .collect()
}

fn rag_eval_answer_mentions_id(answer: &str, id: &str) -> bool {
    answer.contains(&format!("[{id}]")) || answer.contains(id)
}

fn rag_eval_bracketed_citations(answer: &str) -> Vec<String> {
    let mut citations = Vec::new();
    let mut rest = answer;
    while let Some(start) = rest.find('[') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find(']') else {
            break;
        };
        let candidate = rest[..end].trim();
        if !candidate.is_empty()
            && candidate.chars().count() <= 96
            && candidate
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '/' | '.'))
            && !citations.iter().any(|existing| existing == candidate)
        {
            citations.push(candidate.to_string());
        }
        rest = &rest[end + 1..];
    }
    citations
}

fn rag_eval_expected_suppressed_sources<'a>(
    expected: &str,
    packing: &'a RagPackingReport,
) -> Vec<&'a RagPackingSuppressedSource> {
    let expected = expected.trim().to_lowercase();
    if expected.is_empty() {
        return Vec::new();
    }
    packing
        .suppressed_sources
        .iter()
        .filter(|source| {
            format!(
                "{} {} {} {}",
                source.id, source.title, source.summary, source.reason
            )
            .to_lowercase()
            .contains(&expected)
        })
        .collect()
}

#[cfg(test)]
fn rag_eval_expected_suppressed_titles(expected: &str, packing: &RagPackingReport) -> Vec<String> {
    rag_eval_expected_suppressed_sources(expected, packing)
        .into_iter()
        .map(|source| source.title.clone())
        .collect()
}

fn rag_eval_unique_suppressed_reasons<'a>(reasons: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for reason in reasons {
        if seen.insert(reason) {
            unique.push(reason.to_string());
        }
    }
    unique
}

fn rag_eval_expected_evidence_status(
    expected: &str,
    selected_match: bool,
    suppressed_titles: &[String],
) -> String {
    if expected.trim().is_empty() {
        "empty_expected".to_string()
    } else if selected_match {
        "selected".to_string()
    } else if !suppressed_titles.is_empty() {
        "suppressed_by_packing".to_string()
    } else {
        "missing_from_candidates".to_string()
    }
}

fn load_rag_eval_cases(conn: &Connection, default_budget: usize) -> Result<Vec<RagEvalCase>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, query, expected, budget, split FROM eval_cases ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let budget = row.get::<_, i64>(4)?;
        Ok(RagEvalCase {
            id: row.get(0)?,
            name: row.get(1)?,
            query: row.get(2)?,
            expected: row.get(3)?,
            budget: if budget > 0 {
                budget as usize
            } else {
                default_budget
            },
            source: "stored".to_string(),
            split: row.get(5)?,
        })
    })?;
    let mut cases = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if !cases.is_empty() {
        return Ok(cases);
    }
    let mut stmt = conn.prepare(
        "SELECT id, title FROM memories \
         WHERE status IN ('active','uncertain') \
         ORDER BY updated_at DESC LIMIT 12",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: String = row.get(1)?;
        Ok(RagEvalCase {
            id: format!("auto-{id}"),
            name: truncate_chars(&title, 80),
            query: title,
            expected: id,
            budget: default_budget,
            source: "auto".to_string(),
            split: "auto".to_string(),
        })
    })?;
    cases = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cases)
}

fn load_graph_rag_eval_cases(conn: &Connection, default_budget: usize) -> Result<Vec<RagEvalCase>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, query, expected, budget, split FROM eval_cases \
         WHERE lower(name || ' ' || query || ' ' || expected) LIKE '%graph%' \
            OR lower(name || ' ' || query || ' ' || expected) LIKE '%relationship%' \
            OR lower(name || ' ' || query || ' ' || expected) LIKE '% related%' \
            OR lower(name || ' ' || query || ' ' || expected) LIKE '% link%' \
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let budget = row.get::<_, i64>(4)?;
        Ok(RagEvalCase {
            id: row.get(0)?,
            name: row.get(1)?,
            query: row.get(2)?,
            expected: row.get(3)?,
            budget: if budget > 0 {
                budget as usize
            } else {
                default_budget
            },
            source: "stored_graph".to_string(),
            split: row.get(5)?,
        })
    })?;
    let cases = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if !cases.is_empty() {
        return Ok(cases);
    }

    let mut stmt = conn.prepare(
        "SELECT l.id, l.memory_id, l.kind, l.target, source.title, target.title \
         FROM memory_links l \
         JOIN memories source ON source.id = l.memory_id \
         JOIN memories target ON target.id = l.target \
         WHERE source.status IN ('active','uncertain') \
           AND target.status IN ('active','uncertain') \
         ORDER BY l.id ASC \
         LIMIT 12",
    )?;
    let rows = stmt.query_map([], |row| {
        let link_id: i64 = row.get(0)?;
        let source_id: String = row.get(1)?;
        let kind: String = row.get(2)?;
        let target_id: String = row.get(3)?;
        let source_title: String = row.get(4)?;
        let target_title: String = row.get(5)?;
        Ok(RagEvalCase {
            id: format!("auto-graph-{link_id}"),
            name: truncate_chars(&format!("{source_title} -> {target_title}"), 80),
            query: format!("Which memory cards are related to {source_title} through {kind}?"),
            expected: if target_id.is_empty() {
                source_id
            } else {
                target_id
            },
            budget: default_budget,
            source: "auto_graph".to_string(),
            split: "auto".to_string(),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn eval_ratio_percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64 * 1000.0).round() / 10.0
    }
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
        println!("semantic_empty_missing: {}", report.semantic_empty_missing);
        if !report.inferred_missing_queries.is_empty() {
            println!(
                "inferred_missing_queries: {}",
                report.inferred_missing_queries.join(" | ")
            );
        }
        if !report.semantic_empty_missing_queries.is_empty() {
            println!(
                "semantic_empty_missing_queries: {}",
                report.semantic_empty_missing_queries.join(" | ")
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
    let mut unresolved_missing_queries = Vec::new();
    for query in missing_queries {
        if should_infer_missing_memory_gap(conn, &query)? {
            unresolved_missing_queries.push(query);
        }
    }
    let missing_queries = unresolved_missing_queries;
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
        semantic_empty_missing: inferred.semantic_empty_missing_queries.len(),
        noisy_memory_ids: noisy,
        missing_queries,
        inferred_missing_queries: inferred.missing_queries,
        semantic_empty_missing_queries: inferred.semantic_empty_missing_queries,
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
            let empty_source = if read.semantic_used {
                "semantic"
            } else {
                "nonsemantic"
            };
            let detail = serde_json::to_string(&json!({
                "rating": "missing",
                "ids": [],
                "command": read.command,
                "query": truncate_chars(&read.query, 500),
                "note": if read.semantic_used {
                    "autonomous inferred feedback from empty semantic memory read"
                } else {
                    "autonomous inferred feedback from empty memory read"
                },
                "source": "autonomous_inferred",
                "empty_source": empty_source,
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
    semantic_empty_missing_queries: Vec<String>,
}

fn inferred_live_signals(
    conn: &Connection,
    reads: &[MemoryReadEvent],
) -> Result<InferredLiveSignals> {
    let mut useful = 0;
    let mut total = 0;
    let mut missing_queries = Vec::new();
    let mut semantic_empty_missing_queries = Vec::new();
    for read in reads {
        if !is_agent_memory_read(&read.command) {
            continue;
        }
        total += 1;
        if read.result_count > 0 && !read.memory_ids.is_empty() {
            useful += 1;
        } else if should_infer_missing_memory_gap(conn, &read.query)? {
            let query = truncate_chars(&read.query, 140);
            if read.semantic_used {
                semantic_empty_missing_queries.push(query.clone());
            }
            missing_queries.push(query);
        }
    }
    missing_queries.sort();
    missing_queries.dedup();
    missing_queries.truncate(20);
    semantic_empty_missing_queries.sort();
    semantic_empty_missing_queries.dedup();
    semantic_empty_missing_queries.truncate(20);
    Ok(InferredLiveSignals {
        useful,
        total,
        missing_queries,
        semantic_empty_missing_queries,
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
    if memory_link_resolves_query(conn, query)? {
        return Ok(false);
    }
    let normalized = query.replace(['_', '-'], " ");
    if normalized != query {
        let rows = query_memories(
            conn,
            Some(&normalized),
            &[],
            &["active".to_string(), "uncertain".to_string()],
            None,
            1,
        )?;
        if !rows.is_empty() || memory_link_resolves_query(conn, &normalized)? {
            return Ok(false);
        }
    }
    Ok(true)
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
    Ok(redact_sensitive_patterns(text))
}

pub(crate) fn scan_secret_findings(conn: &Connection) -> Result<Vec<SecretFinding>> {
    let rows = query_memories(conn, None, &[], &[], None, usize::MAX)?;
    let mut out = Vec::new();
    for row in rows {
        let text = format!("{}\n{}", row.title, row.body);
        for name in sensitive_text_patterns(&text) {
            out.push(SecretFinding {
                id: row.id.clone(),
                title: row.title.clone(),
                pattern: name.to_string(),
            });
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
    let mut issues = rows
        .into_iter()
        .filter(|m| m.updated_at < cutoff)
        .map(|m| ReviewIssue {
            kind: "stale".to_string(),
            id: m.id,
            title: m.title,
            detail: format!("not updated for at least {days} day(s)"),
        })
        .collect::<Vec<_>>();
    let mut seen = issues
        .iter()
        .map(|issue| issue.id.clone())
        .collect::<HashSet<_>>();
    for evidence in stale_file_evidence(conn, 10_000)? {
        if seen.insert(evidence.memory_id.clone()) {
            issues.push(ReviewIssue {
                kind: "stale_evidence".to_string(),
                id: evidence.memory_id,
                title: evidence.memory_title,
                detail: format!(
                    "{} file evidence `{}`: {}",
                    evidence.status, evidence.path, evidence.detail
                ),
            });
        }
    }
    Ok(issues)
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
    let mut sql = "SELECT l.memory_id, l.kind, l.target FROM memory_links l \
                   JOIN memories m ON m.id = l.memory_id"
        .to_string();
    let mut params_vec = Vec::new();
    if let Some(id) = id {
        sql.push_str(" WHERE l.memory_id = ?");
        params_vec.push(id.to_string());
    } else {
        sql.push_str(" WHERE m.status IN ('active', 'uncertain')");
    }
    sql.push_str(" ORDER BY l.memory_id, l.id");
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

    fn brief_item(id: &str, title: &str) -> BriefItem {
        BriefItem {
            id: id.to_string(),
            memory_type: "design_note".to_string(),
            title: title.to_string(),
            summary: "long query focused summary ".repeat(8),
            score: 1.0,
            reasons: vec!["test".to_string()],
        }
    }

    fn rag_eval_case(id: &str, expected: &str, budget: usize) -> RagEvalCase {
        RagEvalCase {
            id: id.to_string(),
            name: format!("case {id}"),
            query: format!("query {id}"),
            expected: expected.to_string(),
            budget,
            source: "stored".to_string(),
            split: "development".to_string(),
        }
    }

    #[test]
    fn rag_eval_corpus_signature_is_order_independent_and_content_aware() {
        let first = rag_eval_case("a", "memory-a", 1_000);
        let second = rag_eval_case("b", "memory-b", 2_000);
        let forward = rag_eval_corpus_signature(&[first, second]).unwrap();

        let reversed = rag_eval_corpus_signature(&[
            rag_eval_case("b", "memory-b", 2_000),
            rag_eval_case("a", "memory-a", 1_000),
        ])
        .unwrap();
        let changed = rag_eval_corpus_signature(&[
            rag_eval_case("a", "memory-a", 1_000),
            rag_eval_case("b", "different", 2_000),
        ])
        .unwrap();

        assert_eq!(forward, reversed);
        assert_ne!(forward, changed);
    }

    #[test]
    fn rag_eval_split_signatures_keep_holdout_changes_separate() {
        let development = rag_eval_case("development", "memory-a", 1_000);
        let mut holdout = rag_eval_case("holdout", "memory-b", 1_000);
        holdout.split = "holdout".to_string();
        let cases = [development, holdout];
        let development_signature = rag_eval_split_corpus_signature(&cases, "development").unwrap();
        let holdout_signature = rag_eval_split_corpus_signature(&cases, "holdout").unwrap();

        let development_changed = rag_eval_split_corpus_signature(
            &[rag_eval_case("development", "changed", 1_000), {
                let mut case = rag_eval_case("holdout", "memory-b", 1_000);
                case.split = "holdout".to_string();
                case
            }],
            "holdout",
        )
        .unwrap();

        assert_ne!(development_signature, holdout_signature);
        assert_eq!(holdout_signature, development_changed);
    }

    #[test]
    fn rag_eval_labels_retrieval_extractive_and_generated_layers_honestly() {
        let temp = tempfile::tempdir().unwrap();
        let conn = open_db(&temp.path().join("memory.db")).unwrap();
        let report = rag_eval_report(&conn, None, 8, 3_000, "mock", "local", "mock").unwrap();

        assert_eq!(report.version, 7);
        assert_eq!(
            report.evaluation_layers.retrieval.evaluation_kind,
            "retrieval_ranking"
        );
        assert_eq!(
            report
                .evaluation_layers
                .extractive_grounding
                .evaluation_kind,
            "deterministic_extractive_grounding"
        );
        assert!(
            !report
                .evaluation_layers
                .extractive_grounding
                .live_model_executed
        );
        assert_eq!(
            report.evaluation_layers.generated_output_guard.passed,
            report.evaluation_layers.generated_output_guard.total
        );
        assert!(
            !report
                .evaluation_layers
                .generated_output_guard
                .live_model_executed
        );
    }

    #[test]
    fn rag_eval_config_signature_covers_retrieval_inputs() {
        let baseline =
            rag_eval_config_signature(None, 6, 3_000, "local", "local", "model").unwrap();
        let changed_limit =
            rag_eval_config_signature(None, 8, 3_000, "local", "local", "model").unwrap();
        let changed_scope =
            rag_eval_config_signature(Some("project"), 6, 3_000, "local", "local", "model")
                .unwrap();

        assert_ne!(baseline, changed_limit);
        assert_ne!(baseline, changed_scope);
    }

    #[test]
    fn changed_rag_baselines_block_release_until_reviewed() {
        for status in [
            "invalid",
            "regressed",
            "changed",
            "corpus_changed",
            "config_changed",
        ] {
            assert!(rag_eval_baseline_blocks_release(status), "status={status}");
        }
        for status in ["matched", "written", "missing", "unconfigured"] {
            assert!(!rag_eval_baseline_blocks_release(status), "status={status}");
        }
    }

    fn rag_eval_source(id: &str, summary: &str) -> RagSource {
        RagSource {
            id: id.to_string(),
            source_kind: "memory".to_string(),
            memory_type: "design_note".to_string(),
            scope: "project".to_string(),
            title: format!("source {id}"),
            status: "active".to_string(),
            score: 10.0,
            utility_score: 1.0,
            semantic_score: Some(0.8),
            confidence: 1.0,
            reasons: vec!["test".to_string()],
            summary: summary.to_string(),
            links: Vec::new(),
            provenance: RagSourceProvenance {
                origin: "memory_store".to_string(),
                trust_lane: "durable_memory".to_string(),
                evidence_ref: format!("dukememory:memory:{id}"),
                content_hash: "test-hash".to_string(),
                source: Some("test".to_string()),
                updated_at: Some(1),
            },
            path: None,
            chunk_index: None,
            start_line: None,
            end_line: None,
        }
    }

    fn rag_eval_case_with_packing(
        expected_evidence_status: &str,
        packing: RagPackingReport,
    ) -> RagEvalCaseResult {
        let expected_suppressed_reasons = if expected_evidence_status == "suppressed_by_packing" {
            rag_eval_unique_suppressed_reasons(
                packing
                    .suppressed_sources
                    .iter()
                    .map(|source| source.reason.as_str()),
            )
        } else {
            Vec::new()
        };
        RagEvalCaseResult {
            id: "case".to_string(),
            name: "case".to_string(),
            case_source: "stored".to_string(),
            split: "development".to_string(),
            query: "query".to_string(),
            expected: "expected".to_string(),
            expected_rank: (expected_evidence_status == "selected").then_some(1),
            passed: expected_evidence_status == "selected",
            detail: "detail".to_string(),
            confidence: "medium".to_string(),
            confidence_score: 0.5,
            citation_count: 0,
            citations: Vec::new(),
            source_titles: Vec::new(),
            packing,
            expected_evidence_status: expected_evidence_status.to_string(),
            expected_in_candidates: expected_evidence_status != "missing_from_candidates",
            expected_suppressed_titles: Vec::new(),
            expected_suppressed_reasons,
            semantic_used: true,
            semantic_error: None,
            missing_evidence: Vec::new(),
            grounded_answer: RagEvalGroundedAnswer {
                passed: expected_evidence_status == "selected",
                detail: "detail".to_string(),
                answer: "answer [case]".to_string(),
                expected_found: expected_evidence_status == "selected",
                citation_count: usize::from(expected_evidence_status == "selected"),
                citations: if expected_evidence_status == "selected" {
                    vec!["case".to_string()]
                } else {
                    Vec::new()
                },
                unknown_citations: Vec::new(),
            },
        }
    }

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

    #[test]
    fn eval_ratio_percent_rounds_to_one_decimal() {
        assert_eq!(eval_ratio_percent(0, 0), 0.0);
        assert_eq!(eval_ratio_percent(1, 3), 33.3);
        assert_eq!(eval_ratio_percent(2, 3), 66.7);
        assert_eq!(eval_ratio_percent(3, 3), 100.0);
    }

    #[test]
    fn rag_eval_case_serializes_packing_diagnostics() {
        let case = RagEvalCaseResult {
            id: "case-1".to_string(),
            name: "packing visible".to_string(),
            case_source: "stored".to_string(),
            split: "development".to_string(),
            query: "how is RAG packed?".to_string(),
            expected: "packing".to_string(),
            expected_rank: Some(2),
            passed: true,
            detail: "expected text found in RAG source pack".to_string(),
            confidence: "medium".to_string(),
            confidence_score: 0.74,
            citation_count: 1,
            citations: vec!["abc123".to_string()],
            source_titles: vec!["README.md:1-10".to_string()],
            packing: RagPackingReport {
                candidate_count: 4,
                selected_count: 2,
                memory_candidates: 1,
                chunk_candidates: 3,
                selected_memories: 1,
                selected_chunks: 1,
                suppressed_duplicate: 0,
                suppressed_overlap: 1,
                suppressed_file_cap: 1,
                suppressed_limit: 0,
                suppressed_sources: vec![RagPackingSuppressedSource {
                    id: "suppressed".to_string(),
                    source_kind: "chunk".to_string(),
                    title: "README.md:20-30".to_string(),
                    reason: "overlap".to_string(),
                    score: 3.0,
                    semantic_score: Some(0.5),
                    location: Some("README.md:20-30".to_string()),
                    summary: "packing candidate was suppressed".to_string(),
                }],
                chunk_files: vec![RagPackingFileReport {
                    path: "README.md".to_string(),
                    candidates: 3,
                    selected: 1,
                    suppressed_overlap: 1,
                    suppressed_file_cap: 1,
                }],
            },
            expected_evidence_status: "selected".to_string(),
            expected_in_candidates: true,
            expected_suppressed_titles: Vec::new(),
            expected_suppressed_reasons: Vec::new(),
            semantic_used: true,
            semantic_error: None,
            missing_evidence: Vec::new(),
            grounded_answer: RagEvalGroundedAnswer {
                passed: true,
                detail: "expected evidence is present in a cited grounded answer".to_string(),
                answer: "packing [abc123]".to_string(),
                expected_found: true,
                citation_count: 1,
                citations: vec!["abc123".to_string()],
                unknown_citations: Vec::new(),
            },
        };

        let value = serde_json::to_value(case).expect("serialize eval case");

        assert_eq!(value["packing"]["candidate_count"], 4);
        assert_eq!(value["packing"]["suppressed_overlap"], 1);
        assert_eq!(value["packing"]["chunk_files"][0]["path"], "README.md");
        assert_eq!(
            value["packing"]["suppressed_sources"][0]["reason"],
            "overlap"
        );
        assert_eq!(value["expected_evidence_status"], "selected");
        assert_eq!(
            value["expected_suppressed_reasons"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(value["grounded_answer"]["passed"], true);
        assert_eq!(value["grounded_answer"]["citation_count"], 1);
    }

    #[test]
    fn rag_eval_expected_status_detects_suppressed_candidates() {
        let packing = RagPackingReport {
            suppressed_sources: vec![RagPackingSuppressedSource {
                id: "chunk-a".to_string(),
                source_kind: "chunk".to_string(),
                title: "README.md:10-20".to_string(),
                reason: "file_cap".to_string(),
                score: 4.0,
                semantic_score: None,
                location: Some("README.md:10-20".to_string()),
                summary: "This candidate contains memory_rag_ingest evidence.".to_string(),
            }],
            ..RagPackingReport::default()
        };

        let titles = rag_eval_expected_suppressed_titles("memory_rag_ingest", &packing);
        let status = rag_eval_expected_evidence_status("memory_rag_ingest", false, &titles);

        assert_eq!(titles, vec!["README.md:10-20".to_string()]);
        assert_eq!(status, "suppressed_by_packing");
    }

    #[test]
    fn rag_eval_packing_summary_totals_cases() {
        let cases = vec![
            rag_eval_case_with_packing(
                "selected",
                RagPackingReport {
                    candidate_count: 4,
                    selected_count: 2,
                    memory_candidates: 1,
                    chunk_candidates: 3,
                    selected_memories: 1,
                    selected_chunks: 1,
                    suppressed_overlap: 1,
                    suppressed_sources: vec![RagPackingSuppressedSource {
                        id: "overlap".to_string(),
                        source_kind: "chunk".to_string(),
                        title: "README.md:1-8".to_string(),
                        reason: "overlap".to_string(),
                        score: 1.0,
                        semantic_score: None,
                        location: Some("README.md:1-8".to_string()),
                        summary: "overlap".to_string(),
                    }],
                    ..RagPackingReport::default()
                },
            ),
            rag_eval_case_with_packing(
                "missing_from_candidates",
                RagPackingReport {
                    candidate_count: 3,
                    selected_count: 1,
                    chunk_candidates: 2,
                    selected_chunks: 1,
                    suppressed_limit: 2,
                    suppressed_sources: vec![
                        RagPackingSuppressedSource {
                            id: "limit-a".to_string(),
                            source_kind: "chunk".to_string(),
                            title: "README.md:10-20".to_string(),
                            reason: "limit".to_string(),
                            score: 1.0,
                            semantic_score: None,
                            location: Some("README.md:10-20".to_string()),
                            summary: "limit".to_string(),
                        },
                        RagPackingSuppressedSource {
                            id: "limit-b".to_string(),
                            source_kind: "chunk".to_string(),
                            title: "README.md:30-40".to_string(),
                            reason: "limit".to_string(),
                            score: 1.0,
                            semantic_score: None,
                            location: Some("README.md:30-40".to_string()),
                            summary: "limit".to_string(),
                        },
                    ],
                    ..RagPackingReport::default()
                },
            ),
        ];

        let summary = rag_eval_packing_summary(&cases);

        assert_eq!(summary.candidate_count, 7);
        assert_eq!(summary.selected_count, 3);
        assert_eq!(summary.selected_chunks, 2);
        assert_eq!(summary.suppressed_overlap, 1);
        assert_eq!(summary.suppressed_limit, 2);
        assert_eq!(summary.suppressed_sources, 3);
        assert_eq!(summary.expected_selected, 1);
        assert_eq!(summary.expected_missing_from_candidates, 1);

        let grounded = rag_eval_grounded_summary(&cases);
        assert_eq!(grounded.passed, 1);
        assert_eq!(grounded.failed, 1);
        assert_eq!(grounded.coverage, 50.0);
        assert_eq!(grounded.expected_in_answer, 1);
        assert_eq!(grounded.cited_answers, 1);

        let ranking = rag_eval_ranking_summary(&cases);
        assert_eq!(ranking.total, 2);
        assert_eq!(ranking.hit_at_1, 1);
        assert_eq!(ranking.hit_at_3_rate, 50.0);
        assert_eq!(ranking.mean_reciprocal_rank, 50.0);
    }

    #[test]
    fn rag_eval_holdout_requires_enough_untouched_grounded_cases() {
        let mut cases = (0..RAG_EVAL_RECOMMENDED_HOLDOUT_CASES)
            .map(|_| {
                let mut case = rag_eval_case_with_packing("selected", RagPackingReport::default());
                case.split = "holdout".to_string();
                case
            })
            .collect::<Vec<_>>();
        let ready = rag_eval_split_summary(&cases);
        assert!(ready.holdout_ready);
        assert_eq!(ready.holdout_recall, 100.0);

        cases.pop();
        let insufficient = rag_eval_split_summary(&cases);
        assert!(!insufficient.holdout_ready);
    }

    #[test]
    fn rag_eval_evidence_placement_summarizes_near_misses() {
        let cases = vec![
            rag_eval_case_with_packing("selected", RagPackingReport::default()),
            rag_eval_case_with_packing(
                "suppressed_by_packing",
                RagPackingReport {
                    suppressed_sources: vec![RagPackingSuppressedSource {
                        id: "chunk-a".to_string(),
                        source_kind: "chunk".to_string(),
                        title: "README.md:1-8".to_string(),
                        reason: "file_cap".to_string(),
                        score: 1.0,
                        semantic_score: None,
                        location: Some("README.md:1-8".to_string()),
                        summary: "expected evidence".to_string(),
                    }],
                    ..RagPackingReport::default()
                },
            ),
            rag_eval_case_with_packing("missing_from_candidates", RagPackingReport::default()),
        ];

        let summary = rag_eval_evidence_placement_summary(&cases);

        assert_eq!(summary.expected_total, 3);
        assert_eq!(summary.selected, 1);
        assert_eq!(summary.suppressed_by_packing, 1);
        assert_eq!(summary.missing_from_candidates, 1);
        assert_eq!(summary.selection_recall, 33.3);
        assert_eq!(summary.candidate_recall, 66.7);
        assert_eq!(summary.near_miss_count, 1);
        assert_eq!(summary.suppression_reasons.get("file_cap"), Some(&1));
    }

    #[test]
    fn rag_eval_matrix_reports_dimension_coverage() {
        let mut case = rag_eval_case_with_packing(
            "selected",
            RagPackingReport {
                selected_chunks: 1,
                chunk_candidates: 2,
                selected_memories: 1,
                memory_candidates: 1,
                ..RagPackingReport::default()
            },
        );
        case.query =
            "Как dukememory CLI MCP /web-control проверяет graph relationship?".to_string();
        case.expected = "graph".to_string();
        case.source_titles = vec!["README.md:1-10".to_string()];

        let summary = rag_eval_matrix_summary(&[case]);

        assert_eq!(summary.stored_cases, 1);
        assert_eq!(summary.dimensions.get("source_chunk"), Some(&1));
        assert_eq!(summary.dimensions.get("memory_card"), Some(&1));
        assert_eq!(summary.dimensions.get("cli_workflow"), Some(&1));
        assert_eq!(summary.dimensions.get("mcp_tooling"), Some(&1));
        assert_eq!(summary.dimensions.get("http_api"), Some(&1));
        assert_eq!(summary.dimensions.get("graph_memory"), Some(&1));
        assert_eq!(summary.dimensions.get("multilingual"), Some(&1));
        assert_eq!(summary.status, "partial");
        assert!(
            summary
                .missing_dimensions
                .contains(&"negative_or_missing".to_string())
        );
    }

    #[test]
    fn rag_eval_retrieval_tuning_recommends_recall_for_near_misses() {
        let cases = vec![
            rag_eval_case_with_packing("selected", RagPackingReport::default()),
            rag_eval_case_with_packing(
                "suppressed_by_packing",
                RagPackingReport {
                    chunk_candidates: 4,
                    selected_chunks: 1,
                    suppressed_sources: vec![RagPackingSuppressedSource {
                        id: "chunk-a".to_string(),
                        source_kind: "chunk".to_string(),
                        title: "README.md:1-8".to_string(),
                        reason: "file_cap".to_string(),
                        score: 1.0,
                        semantic_score: None,
                        location: Some("README.md:1-8".to_string()),
                        summary: "expected evidence".to_string(),
                    }],
                    ..RagPackingReport::default()
                },
            ),
            rag_eval_case_with_packing("missing_from_candidates", RagPackingReport::default()),
        ];
        let evidence = rag_eval_evidence_placement_summary(&cases);
        let packing = rag_eval_packing_summary(&cases);

        let tuning = rag_eval_retrieval_tuning_summary(&cases, &evidence, &packing, 0);

        assert_eq!(tuning.status, "attention");
        assert_eq!(tuning.selected_profile, "recall_heavy");
        assert_eq!(tuning.selection_recall, 33.3);
        assert_eq!(tuning.candidate_recall, 66.7);
        assert_eq!(tuning.near_miss_count, 1);
        assert!(
            tuning
                .reasons
                .iter()
                .any(|reason| reason.contains("suppressed by packing"))
        );
    }

    #[test]
    fn graph_rag_eval_graph_summary_counts_connected_cases() {
        let mut kinds = std::collections::BTreeMap::new();
        kinds.insert("relates_to".to_string(), 2);
        let cases = vec![
            GraphRagEvalCaseResult {
                id: "case-a".to_string(),
                name: "case a".to_string(),
                case_source: "auto_graph".to_string(),
                query: "query".to_string(),
                expected: "expected".to_string(),
                passed: true,
                detail: "detail".to_string(),
                graph_status: "connected".to_string(),
                confidence: "high".to_string(),
                confidence_score: 0.9,
                node_count: 3,
                edge_count: 2,
                relationship_coverage: 100.0,
                relationship_kinds: kinds,
                expected_in_graph: true,
                expected_in_answer: true,
                citation_count: 2,
                citations: vec!["a".to_string(), "b".to_string()],
                answer: "answer".to_string(),
                ranked_node_titles: vec!["a".to_string()],
                missing_evidence: Vec::new(),
            },
            GraphRagEvalCaseResult {
                id: "case-b".to_string(),
                name: "case b".to_string(),
                case_source: "auto_graph".to_string(),
                query: "query".to_string(),
                expected: "expected".to_string(),
                passed: false,
                detail: "detail".to_string(),
                graph_status: "isolated".to_string(),
                confidence: "low".to_string(),
                confidence_score: 0.2,
                node_count: 2,
                edge_count: 0,
                relationship_coverage: 0.0,
                relationship_kinds: std::collections::BTreeMap::new(),
                expected_in_graph: true,
                expected_in_answer: false,
                citation_count: 1,
                citations: vec!["c".to_string()],
                answer: "answer".to_string(),
                ranked_node_titles: vec!["c".to_string()],
                missing_evidence: vec!["missing edge".to_string()],
            },
        ];

        let summary = graph_rag_eval_graph_summary(&cases);

        assert_eq!(summary.total_nodes, 5);
        assert_eq!(summary.total_edges, 2);
        assert_eq!(summary.connected_cases, 1);
        assert_eq!(summary.isolated_cases, 1);
        assert_eq!(summary.missing_graph_cases, 0);
        assert_eq!(summary.average_relationship_coverage, 50.0);
        assert_eq!(summary.relationship_kinds.get("relates_to"), Some(&2));
    }

    #[test]
    fn rag_eval_grounded_answer_requires_expected_evidence_and_valid_citation() {
        let sources = vec![rag_eval_source(
            "abc123",
            "Agents use `dukememory rag-ingest --apply` to index source chunks.",
        )];

        let grounded = rag_eval_grounded_answer(
            "How do agents index source chunks?",
            "rag-ingest",
            true,
            &sources,
            &[],
        );

        assert!(grounded.passed);
        assert!(grounded.expected_found);
        assert_eq!(grounded.citations, vec!["abc123".to_string()]);
        assert!(grounded.unknown_citations.is_empty());

        let missing = rag_eval_grounded_answer(
            "How do agents index source chunks?",
            "memory_rag_ingest",
            true,
            &sources,
            &[],
        );

        assert!(!missing.passed);
        assert!(missing.detail.contains("missing from the grounded answer"));
    }

    #[test]
    fn rag_eval_unknown_answer_citations_detects_ids_outside_source_pack() {
        let sources = vec![rag_eval_source("abc123", "grounded source")];

        let unknown =
            rag_eval_unknown_answer_citations("Use [abc123] but not [missing456].", &sources);

        assert_eq!(unknown, vec!["missing456".to_string()]);
    }

    #[test]
    fn brief_plain_receipt_counts_only_rendered_memory_items() {
        let mut report = BriefReport {
            version: 1,
            task: "budget visible".to_string(),
            budget: 460,
            semantic_used: true,
            semantic_skipped: false,
            semantic_skip_reason: None,
            semantic_error: None,
            receipt: String::new(),
            must_follow: Vec::new(),
            relevant: vec![
                brief_item("111111111111", "First visible"),
                brief_item("222222222222", "Second hidden"),
                brief_item("333333333333", "Third hidden"),
            ],
            risks: Vec::new(),
            files: vec![
                "file:src/first.rs".to_string(),
                "file:src/second.rs".to_string(),
            ],
            checks: Vec::new(),
        };
        report.receipt = memory_receipt_with_semantic(
            "brief",
            MemorySemanticStatus::Used,
            &brief_report_memory_ids(&report.must_follow, &report.relevant, &report.risks),
            "none",
        );

        let ids = sync_brief_plain_receipt_ids(&mut report, MemorySemanticStatus::Used);
        let rendered = render_brief(&report);

        assert_eq!(ids.len(), rendered_memory_ids(&rendered).len());
        assert!(rendered.contains(&format!("matched {} card", ids.len())));
        assert!(!ids.contains(&"333333333333".to_string()));
        assert!(!rendered_memory_ids(&rendered).iter().any(|id| id == "file"));
    }

    #[test]
    fn impact_plain_receipt_counts_only_rendered_memory_items() {
        let mut report = ImpactReport {
            version: 1,
            target: "src/budget.rs".to_string(),
            budget: 430,
            semantic_used: true,
            receipt: String::new(),
            decisions: Vec::new(),
            constraints: Vec::new(),
            risks: Vec::new(),
            checks: Vec::new(),
            related: vec![
                brief_item("aaaaaaaaaaaa", "First visible"),
                brief_item("bbbbbbbbbbbb", "Second hidden"),
                brief_item("cccccccccccc", "Third hidden"),
            ],
            links: vec!["file:src/budget.rs".to_string()],
        };
        report.receipt = memory_receipt_with_semantic(
            "impact",
            MemorySemanticStatus::Used,
            &impact_report_memory_ids(
                &report.decisions,
                &report.constraints,
                &report.risks,
                &report.related,
            ),
            "none",
        );

        let ids = sync_impact_plain_receipt_ids(&mut report, MemorySemanticStatus::Used);
        let rendered = render_impact(&report);

        assert_eq!(ids.len(), rendered_memory_ids(&rendered).len());
        assert!(rendered.contains(&format!("matched {} card", ids.len())));
        assert!(!ids.contains(&"cccccccccccc".to_string()));
    }
}

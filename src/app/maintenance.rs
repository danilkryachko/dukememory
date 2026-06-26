use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InboxV2Report {
    pub(crate) version: u32,
    pub(crate) pending: usize,
    pub(crate) groups: Vec<InboxV2Group>,
    pub(crate) actions: Vec<AutonomousAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InboxV2Group {
    pub(crate) key: String,
    pub(crate) count: usize,
    pub(crate) ids: Vec<String>,
    pub(crate) title: String,
    pub(crate) memory_type: String,
    pub(crate) max_confidence: f64,
    pub(crate) recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PolicyTuneReport {
    pub(crate) version: u32,
    pub(crate) level: String,
    pub(crate) risk_limit: f64,
    pub(crate) approve_threshold: f64,
    pub(crate) duplicate_limit: usize,
    pub(crate) reasons: Vec<String>,
    pub(crate) output: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SuggestedMemory {
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) confidence: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutoIngestReport {
    pub(crate) scanned: usize,
    pub(crate) ingested: usize,
    pub(crate) skipped: usize,
    pub(crate) inbox_added: usize,
    pub(crate) files: Vec<AutoIngestFile>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutoIngestFile {
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) suggestions: usize,
}

pub(crate) struct AutoIngestPrintRequest<'a> {
    pub(crate) input: &'a Path,
    pub(crate) scope: &'a str,
    pub(crate) llm: bool,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) dry_run: bool,
    pub(crate) json: bool,
}

pub(crate) fn handle_inbox_v2(conn: &Connection, command: InboxV2Command) -> Result<()> {
    match command {
        InboxV2Command::Report { limit, json } => {
            let report = inbox_v2_report(conn, limit, false)?;
            print_inbox_v2_report(&report, json)
        }
        InboxV2Command::AutoApply { dry_run, json } => {
            let report = inbox_v2_report(conn, 100, !dry_run)?;
            print_inbox_v2_report(&report, json)
        }
    }
}

pub(crate) fn print_inbox_v2_report(report: &InboxV2Report, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("pending: {}", report.pending);
        for group in &report.groups {
            println!(
                "- {} count={} confidence={:.2} {}",
                group.key, group.count, group.max_confidence, group.recommendation
            );
        }
        for action in &report.actions {
            println!("{} {} {}", action.status, action.kind, action.detail);
        }
    }
    Ok(())
}

pub(crate) fn inbox_v2_report(
    conn: &Connection,
    limit: usize,
    apply: bool,
) -> Result<InboxV2Report> {
    let items = list_inbox(conn, "pending", limit)?;
    let mut groups_map: BTreeMap<String, Vec<InboxItem>> = BTreeMap::new();
    for item in items {
        let key = format!("{}:{}", item.memory_type, normalize_title(&item.title));
        groups_map.entry(key).or_default().push(item);
    }
    let mut groups = Vec::new();
    let mut actions = Vec::new();
    for (key, mut group) in groups_map {
        group.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top = group[0].clone();
        let ids = group.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let recommendation = if group.len() > 1 {
            "merge_duplicates"
        } else if top.confidence >= 0.9
            && !matches!(top.memory_type.as_str(), "decision" | "constraint")
        {
            "approve_high_confidence"
        } else if top.confidence < 0.35 {
            "reject_low_confidence"
        } else {
            "keep_pending"
        };
        if apply {
            match recommendation {
                "merge_duplicates" => {
                    for duplicate in group.iter().skip(1) {
                        reject_inbox(conn, &duplicate.id)?;
                        actions.push(AutonomousAction {
                            kind: "inbox_v2_reject_duplicate".to_string(),
                            status: "ok".to_string(),
                            detail: duplicate.id.clone(),
                            memory_id: None,
                        });
                    }
                }
                "approve_high_confidence" => {
                    let memory_id = approve_inbox(conn, &top.id, false)?;
                    actions.push(AutonomousAction {
                        kind: "inbox_v2_approve".to_string(),
                        status: "ok".to_string(),
                        detail: top.id.clone(),
                        memory_id: Some(memory_id),
                    });
                }
                "reject_low_confidence" => {
                    reject_inbox(conn, &top.id)?;
                    actions.push(AutonomousAction {
                        kind: "inbox_v2_reject_low_confidence".to_string(),
                        status: "ok".to_string(),
                        detail: top.id.clone(),
                        memory_id: None,
                    });
                }
                _ => {}
            }
        }
        groups.push(InboxV2Group {
            key,
            count: ids.len(),
            ids,
            title: top.title,
            memory_type: top.memory_type,
            max_confidence: top.confidence,
            recommendation: recommendation.to_string(),
        });
    }
    Ok(InboxV2Report {
        version: 1,
        pending: groups.iter().map(|group| group.count).sum(),
        groups,
        actions,
    })
}

pub(crate) fn print_policy_tune(
    conn: &Connection,
    output: &Path,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = policy_tune_report(conn, output, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("level: {}", report.level);
        println!("risk_limit: {:.1}", report.risk_limit);
        println!("approve_threshold: {:.2}", report.approve_threshold);
        for reason in report.reasons {
            println!("- {reason}");
        }
    }
    Ok(())
}

pub(crate) fn policy_tune_report(
    conn: &Connection,
    output: &Path,
    dry_run: bool,
) -> Result<PolicyTuneReport> {
    let feedback = feedback_summary(conn, 30)?;
    let quality = quality_report(conn, 30, 20)?;
    let rollback_count = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_events WHERE event_type = 'autonomous_rollback' AND created_at >= ?1",
            params![now_ms().saturating_sub(30 * 86_400_000)],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        .max(0) as usize;
    let mut reasons = Vec::new();
    let mut level = "normal";
    let mut risk_limit = 45.0;
    let mut approve_threshold = 0.85;
    let mut duplicate_limit = 3;
    if rollback_count > 0 || feedback.negative > feedback.positive {
        level = "conservative";
        risk_limit = 15.0;
        approve_threshold = 0.92;
        duplicate_limit = 1;
        reasons.push("rollback or negative feedback detected".to_string());
    } else if quality.average_score > 75.0 && feedback.positive >= feedback.negative {
        level = "aggressive";
        risk_limit = 70.0;
        approve_threshold = 0.75;
        duplicate_limit = 10;
        reasons.push("high quality and non-negative feedback".to_string());
    } else {
        reasons.push("balanced quality and feedback".to_string());
    }
    let report = PolicyTuneReport {
        version: 1,
        level: level.to_string(),
        risk_limit,
        approve_threshold,
        duplicate_limit,
        reasons,
        output: if dry_run {
            None
        } else {
            Some(output.display().to_string())
        },
    };
    if !dry_run {
        write_file(output, serde_json::to_string_pretty(&report)?.as_bytes())?;
    }
    Ok(report)
}

pub(crate) fn suggest_from_file(
    conn: &Connection,
    input: &Path,
    scope: &str,
    to_inbox: bool,
    json_out: bool,
) -> Result<()> {
    validate_scope(scope)?;
    let text =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let suggestions = suggest_from_text(&text);
    if to_inbox {
        let count =
            insert_inbox_suggestions(conn, &suggestions, scope, Some(input.display().to_string()))?;
        if json_out {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"inbox_added": count}))?
            );
        } else {
            println!("inbox_added: {count}");
        }
        return Ok(());
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(&suggestions)?);
    } else if suggestions.is_empty() {
        println!("suggestions: none");
    } else {
        for s in suggestions {
            println!("{}  {:.2}  {}", s.memory_type, s.confidence, s.title);
            println!("  {}", s.body);
        }
    }
    Ok(())
}

pub(crate) fn ingest_transcript(
    conn: &Connection,
    input: &Path,
    scope: &str,
    llm: bool,
    endpoint: &str,
    model: &str,
) -> Result<usize> {
    validate_scope(scope)?;
    let text =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let suggestions = if llm {
        suggest_from_llm(endpoint, model, &text).unwrap_or_else(|_| suggest_from_text(&text))
    } else {
        suggest_from_text(&text)
    };
    insert_inbox_suggestions(conn, &suggestions, scope, Some(input.display().to_string()))
}

pub(crate) fn print_auto_ingest(
    conn: &Connection,
    request: AutoIngestPrintRequest<'_>,
) -> Result<()> {
    let report = auto_ingest_sessions(
        conn,
        request.input,
        request.scope,
        request.llm,
        request.endpoint,
        request.model,
        request.dry_run,
    )?;
    if request.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!(
        "auto_ingest scanned={} ingested={} skipped={} inbox_added={}",
        report.scanned, report.ingested, report.skipped, report.inbox_added
    );
    for file in report.files {
        println!(
            "{}  {}  suggestions={}",
            file.status, file.path, file.suggestions
        );
    }
    Ok(())
}

pub(crate) fn auto_ingest_sessions(
    conn: &Connection,
    input: &Path,
    scope: &str,
    llm: bool,
    endpoint: &str,
    model: &str,
    dry_run: bool,
) -> Result<AutoIngestReport> {
    validate_scope(scope)?;
    let files = collect_session_files(input)?;
    let mut report = AutoIngestReport {
        scanned: files.len(),
        ingested: 0,
        skipped: 0,
        inbox_added: 0,
        files: Vec::new(),
    };
    for file in files {
        let path = file.display().to_string();
        let text = fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let hash = embeddings::content_hash(&text);
        if source_already_ingested(conn, &path, &hash)? {
            report.skipped += 1;
            report.files.push(AutoIngestFile {
                path,
                status: "skipped".to_string(),
                suggestions: 0,
            });
            continue;
        }
        let suggestions = if llm {
            suggest_from_llm(endpoint, model, &text).unwrap_or_else(|_| suggest_from_text(&text))
        } else {
            suggest_from_text(&text)
        };
        let count = if dry_run {
            suggestions.len()
        } else {
            let count = insert_inbox_suggestions(conn, &suggestions, scope, Some(path.clone()))?;
            record_memory_source(conn, &path, &hash, "ingested", count)?;
            count
        };
        report.ingested += 1;
        report.inbox_added += count;
        report.files.push(AutoIngestFile {
            path,
            status: if dry_run { "would_ingest" } else { "ingested" }.to_string(),
            suggestions: count,
        });
    }
    Ok(report)
}

fn collect_session_files(input: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !input.exists() {
        return Ok(files);
    }
    if input.is_file() {
        if is_session_file(input) {
            files.push(input.to_path_buf());
        }
        return Ok(files);
    }
    collect_session_files_inner(input, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_session_files_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_session_files_inner(&path, files)?;
        } else if is_session_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_session_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md" | "txt" | "log" | "jsonl")
    )
}

fn source_already_ingested(conn: &Connection, path: &str, hash: &str) -> Result<bool> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM memory_sources WHERE path = ?1 AND content_hash = ?2 LIMIT 1",
            params![path, hash],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

fn record_memory_source(
    conn: &Connection,
    path: &str,
    hash: &str,
    status: &str,
    suggestions: usize,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR IGNORE INTO memory_sources (
            path, content_hash, status, suggestions, ingested_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            path,
            hash,
            status,
            suggestions.min(i64::MAX as usize) as i64,
            now_ms()
        ],
    )?;
    Ok(())
}

pub(crate) fn suggest_from_llm(
    endpoint: &str,
    model: &str,
    text: &str,
) -> Result<Vec<SuggestedMemory>> {
    let prompt = format!(
        "Extract durable project memory from this transcript. Return lines only in this format: type|title|body. Valid types: product_goal,user_preference,decision,design_note,known_issue,command,task_state,domain_fact,constraint,note.\n\n{text}"
    );
    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    let value: Value = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?
        .post(url)
        .json(&json!({"model": model, "prompt": prompt, "stream": false}))
        .send()?
        .error_for_status()?
        .json()?;
    let response = value
        .get("response")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut out = Vec::new();
    for line in response
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts = line.splitn(3, '|').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 || !is_valid_memory_type(parts[0]) {
            continue;
        }
        out.push(SuggestedMemory {
            memory_type: parts[0].to_string(),
            title: parts[1].to_string(),
            body: parts[2].to_string(),
            confidence: 0.75,
        });
    }
    if out.is_empty() {
        bail!("LLM did not return parseable suggestions");
    }
    out.truncate(30);
    Ok(out)
}

fn is_valid_memory_type(value: &str) -> bool {
    matches!(
        value,
        "product_goal"
            | "user_preference"
            | "decision"
            | "design_note"
            | "known_issue"
            | "command"
            | "task_state"
            | "domain_fact"
            | "constraint"
            | "note"
    )
}

fn insert_inbox_suggestions(
    conn: &Connection,
    suggestions: &[SuggestedMemory],
    scope: &str,
    source: Option<String>,
) -> Result<usize> {
    let mut count = 0;
    for suggestion in suggestions {
        validate_confidence(suggestion.confidence)?;
        conn.execute(
            r#"
            INSERT INTO memory_inbox (
                id, type, scope, title, body, source, confidence, status, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?9)
            "#,
            params![
                Uuid::new_v4().simple().to_string()[..12].to_string(),
                suggestion.memory_type,
                scope,
                suggestion.title,
                suggestion.body,
                source,
                suggestion.confidence,
                now_ms(),
                now_ms(),
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn insert_gap_inbox_suggestion(
    conn: &Connection,
    scope: &str,
    query: &str,
) -> Result<Option<String>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let title = format!("Fill memory gap: {}", truncate_chars(query, 80));
    let source = "autonomous_gap";
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_inbox WHERE status = 'pending' AND source = ?1 AND title = ?2 LIMIT 1",
            params![source, title],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(None);
    }
    let id = Uuid::new_v4().simple().to_string()[..12].to_string();
    let now = now_ms();
    let body = format!(
        "Autonomous memory gap detected from an agent read that returned no usable memory. Query: {query}. Add a concise durable memory card only if this gap reflects real project knowledge."
    );
    conn.execute(
        r#"
        INSERT INTO memory_inbox (
            id, type, scope, title, body, source, confidence, status, created_at, updated_at
        ) VALUES (?1, 'task_state', ?2, ?3, ?4, ?5, 0.62, 'pending', ?6, ?7)
        "#,
        params![id, scope, title, body, source, now, now],
    )?;
    log_event(
        conn,
        "autonomous_gap_inbox",
        None,
        &format!("created inbox suggestion {id} for gap {query}"),
    )?;
    Ok(Some(id))
}

pub(crate) fn insert_quality_inbox_suggestion(
    conn: &Connection,
    scope: &str,
    item: &MemoryQuality,
) -> Result<Option<String>> {
    let title = format!(
        "Review memory quality: {} {}",
        item.id,
        truncate_chars(&item.title, 70)
    );
    let source = "autonomous_quality";
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_inbox WHERE status = 'pending' AND source = ?1 AND title = ?2 LIMIT 1",
            params![source, title],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(None);
    }
    let id = Uuid::new_v4().simple().to_string()[..12].to_string();
    let now = now_ms();
    let reasons = if item.reasons.is_empty() {
        "no explicit quality reason".to_string()
    } else {
        item.reasons.join("; ")
    };
    let body = format!(
        "Autonomous quality review candidate for memory {memory_id} ({memory_type}). Score {score:.1}; requests={requests}; links={links}; body_chars={body_chars}. Reasons: {reasons}. Improve, link, compact, or reject this card only if the review is still accurate.",
        memory_id = item.id,
        memory_type = item.memory_type,
        score = item.score,
        requests = item.request_count,
        links = item.links,
        body_chars = item.body_chars,
    );
    conn.execute(
        r#"
        INSERT INTO memory_inbox (
            id, type, scope, title, body, source, confidence, status, created_at, updated_at
        ) VALUES (?1, 'task_state', ?2, ?3, ?4, ?5, 0.58, 'pending', ?6, ?7)
        "#,
        params![id, scope, title, body, source, now, now],
    )?;
    log_event(
        conn,
        "autonomous_quality_inbox",
        Some(&item.id),
        &format!("created inbox suggestion {id} for weak memory {}", item.id),
    )?;
    Ok(Some(id))
}

pub(crate) fn print_inbox(
    conn: &Connection,
    status: &str,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let rows = list_inbox(conn, status, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("inbox: none");
        return Ok(());
    }
    for row in rows {
        println!(
            "{}  {}  {}  scope={}  confidence={:.2}",
            row.id, row.memory_type, row.status, row.scope, row.confidence
        );
        println!("{}", row.title);
        println!("  {}", row.body);
    }
    Ok(())
}

pub(crate) fn list_inbox(conn: &Connection, status: &str, limit: usize) -> Result<Vec<InboxItem>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, type, scope, title, body, source, confidence, status, created_at, updated_at
        FROM memory_inbox
        WHERE status = ?1
        ORDER BY updated_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(params![status, limit.min(i64::MAX as usize)], row_to_inbox)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn row_to_inbox(row: &Row<'_>) -> rusqlite::Result<InboxItem> {
    Ok(InboxItem {
        id: row.get("id")?,
        memory_type: row.get("type")?,
        scope: row.get("scope")?,
        title: row.get("title")?,
        body: row.get("body")?,
        source: row.get("source")?,
        confidence: row.get("confidence")?,
        status: row.get("status")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

pub(crate) fn get_inbox_item(conn: &Connection, id: &str) -> Result<InboxItem> {
    conn.query_row(
        r#"
        SELECT id, type, scope, title, body, source, confidence, status, created_at, updated_at
        FROM memory_inbox
        WHERE id = ?1
        "#,
        params![id],
        row_to_inbox,
    )
    .optional()?
    .with_context(|| format!("Inbox item not found: {id}"))
}

pub(crate) fn approve_inbox(conn: &Connection, id: &str, allow_sensitive: bool) -> Result<String> {
    let item = get_inbox_item(conn, id)?;
    if item.status != "pending" {
        bail!("inbox item is not pending: {id}");
    }
    reject_sensitive(&item.title, &item.body, allow_sensitive)?;
    let memory_id = add_memory(
        conn,
        AddMemory {
            id: None,
            memory_type: item.memory_type,
            title: item.title,
            body: item.body,
            scope: item.scope,
            status: "active".to_string(),
            source: item.source.or_else(|| Some("inbox".to_string())),
            supersedes: None,
            confidence: item.confidence,
            links: Vec::new(),
        },
    )?;
    conn.execute(
        "UPDATE memory_inbox SET status = 'approved', updated_at = ?1 WHERE id = ?2",
        params![now_ms(), id],
    )?;
    log_event(
        conn,
        "inbox_approved",
        Some(&memory_id),
        &format!("approved inbox item {id}"),
    )?;
    Ok(memory_id)
}

pub(crate) fn reject_inbox(conn: &Connection, id: &str) -> Result<()> {
    let changed = conn.execute(
        "UPDATE memory_inbox SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
        params![now_ms(), id],
    )?;
    if changed == 0 {
        bail!("Inbox item not found: {id}");
    }
    log_event(
        conn,
        "inbox_rejected",
        None,
        &format!("rejected inbox item {id}"),
    )?;
    println!("{id}");
    Ok(())
}

pub(crate) fn suggest_from_text(text: &str) -> Vec<SuggestedMemory> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| line.len() > 8) {
        let lower = line.to_lowercase();
        let memory_type = if lower.contains("decided")
            || lower.contains("решили")
            || lower.contains("decision")
        {
            "decision"
        } else if lower.contains("todo")
            || lower.contains("next")
            || lower.contains("дальше")
            || lower.contains("след")
        {
            "task_state"
        } else if lower.contains("bug")
            || lower.contains("issue")
            || lower.contains("problem")
            || lower.contains("ошиб")
        {
            "known_issue"
        } else if lower.contains("prefer") || lower.contains("нрав") || lower.contains("предпоч")
        {
            "user_preference"
        } else {
            continue;
        };
        out.push(SuggestedMemory {
            memory_type: memory_type.to_string(),
            title: truncate_words(line, 8),
            body: line.to_string(),
            confidence: 0.65,
        });
    }
    out.truncate(20);
    out
}

pub(crate) fn truncate_words(text: &str, count: usize) -> String {
    let words = text.split_whitespace().take(count).collect::<Vec<_>>();
    let mut out = words.join(" ");
    if text.split_whitespace().count() > count {
        out.push_str("...");
    }
    out
}

pub(crate) fn one_line_summary(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }
    let mut out = text.chars().take(max_chars - 3).collect::<String>();
    out.push_str("...");
    out
}

pub(crate) fn compact_task_state(
    conn: &Connection,
    scope: &str,
    limit: usize,
    dry_run: bool,
) -> Result<()> {
    validate_scope(scope)?;
    let rows = query_memories(
        conn,
        None,
        &["task_state".to_string()],
        &["active".to_string()],
        Some(scope),
        limit,
    )?;
    if rows.is_empty() {
        println!("compact: nothing to compact");
        return Ok(());
    }
    let mut body = String::from("Compacted task state:\n");
    for row in &rows {
        body.push_str("- ");
        body.push_str(&row.title);
        body.push_str(": ");
        body.push_str(&row.body.replace('\n', " "));
        body.push('\n');
    }
    if dry_run {
        println!("{body}");
        return Ok(());
    }
    let id = add_memory(
        conn,
        AddMemory {
            id: None,
            memory_type: "task_state".to_string(),
            title: format!("Compacted {scope} task state"),
            body,
            scope: scope.to_string(),
            status: "active".to_string(),
            source: Some("compact".to_string()),
            supersedes: None,
            confidence: 0.9,
            links: Vec::new(),
        },
    )?;
    for row in rows {
        conn.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
            params![id, now_ms(), row.id],
        )?;
    }
    println!("{id}");
    Ok(())
}

pub(crate) fn compact_v2(
    conn: &Connection,
    scope: &str,
    limit: usize,
    dry_run: bool,
) -> Result<()> {
    validate_scope(scope)?;
    let rows = query_memories(
        conn,
        None,
        &["task_state".to_string(), "note".to_string()],
        &["active".to_string(), "uncertain".to_string()],
        Some(scope),
        limit,
    )?;
    if rows.is_empty() {
        println!("compact_v2: nothing to compact");
        return Ok(());
    }
    let mut body = String::from("Compacted operational memory:\n");
    for row in &rows {
        body.push_str("- ");
        body.push_str(&row.memory_type);
        body.push_str(": ");
        body.push_str(&row.title);
        body.push_str(" -- ");
        body.push_str(&row.body.replace('\n', " "));
        body.push('\n');
    }
    if dry_run {
        println!("{body}");
        return Ok(());
    }
    let id_holder = std::cell::RefCell::new(String::new());
    transactional(conn, "compact_v2", || {
        let id = add_memory(
            conn,
            AddMemory {
                id: None,
                memory_type: "task_state".to_string(),
                title: format!("Compacted v2 {scope} operational memory"),
                body: body.clone(),
                scope: scope.to_string(),
                status: "active".to_string(),
                source: Some("compact_v2".to_string()),
                supersedes: None,
                confidence: 0.9,
                links: Vec::new(),
            },
        )?;
        for row in &rows {
            conn.execute(
                "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
                params![id, now_ms(), row.id],
            )?;
        }
        log_event(
            conn,
            "compact_v2",
            Some(&id),
            &format!("compacted {} operational memories", rows.len()),
        )?;
        *id_holder.borrow_mut() = id;
        Ok(())
    })?;
    println!("{}", id_holder.borrow());
    Ok(())
}

pub(crate) fn render_codegraph_hints(rows: &[Memory], task: &str, root: &Path) -> String {
    let mut files = Vec::new();
    for row in rows {
        for raw in row
            .body
            .split_whitespace()
            .chain(row.title.split_whitespace())
        {
            if raw.contains('/') || raw.ends_with(".rs") || raw.ends_with(".ts") {
                files.push(raw.trim_matches(|c: char| c == ',' || c == '.').to_string());
            }
        }
    }
    let mut out = String::from("\n\nCodeGraph Hints:\n");
    if let Some(codegraph_root) = find_nearest_codegraph_root(root) {
        match run_codegraph_explore(&codegraph_root, task, 3000) {
            Ok(output) if !output.trim().is_empty() => {
                out.push_str("- CodeGraph explore result:\n");
                out.push_str(&indent_block(&output, "  "));
                out.push('\n');
            }
            Ok(_) => out.push_str("- CodeGraph returned no output for this task.\n"),
            Err(err) => {
                out.push_str("- CodeGraph index exists, but query failed: ");
                out.push_str(&err.to_string());
                out.push('\n');
            }
        }
    } else {
        out.push_str("- No .codegraph index found. Run `codegraph explore \"");
        out.push_str(task);
        out.push_str("\"` only after indexing the repo.\n");
    }
    if !files.is_empty() {
        out.push_str("- Candidate files from memory: ");
        out.push_str(&files.into_iter().take(8).collect::<Vec<_>>().join(", "));
        out.push('\n');
    }
    out
}

fn indent_block(text: &str, prefix: &str) -> String {
    text.lines()
        .take(80)
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_codegraph_explore(root: &Path, task: &str, max_chars: usize) -> Result<String> {
    run_codegraph(root, &["explore", task], max_chars)
}

pub(crate) fn run_codegraph_node(root: &Path, symbol: &str, max_chars: usize) -> Result<String> {
    run_codegraph(root, &["node", symbol], max_chars)
}

fn run_codegraph(root: &Path, args: &[&str], max_chars: usize) -> Result<String> {
    let output = ProcessCommand::new("codegraph")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| "failed to execute `codegraph`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("codegraph failed: {}", stderr.trim());
    }
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.len() > max_chars {
        text.truncate(max_chars);
        text.push_str("\n...");
    }
    Ok(text)
}

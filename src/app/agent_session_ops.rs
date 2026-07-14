use super::*;

const TERMINAL_SESSION_STATUSES: &[&str] = &["completed", "failed", "partial", "abandoned"];
const SESSION_STATUSES: &[&str] = &["active", "completed", "failed", "partial", "abandoned"];
const SESSION_OUTCOMES: &[&str] = &["success", "failed", "partial", "abandoned"];

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionPage {
    pub(crate) version: u32,
    pub(crate) total: usize,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) has_more: bool,
    pub(crate) statuses: Vec<String>,
    pub(crate) outcomes: Vec<String>,
    pub(crate) sessions: Vec<AgentSession>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionCleanupReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) dry_run: bool,
    pub(crate) older_than_days: i64,
    pub(crate) cutoff: i64,
    pub(crate) policy: AgentSessionConfig,
    pub(crate) selected_statuses: Vec<String>,
    pub(crate) cutoffs: BTreeMap<String, i64>,
    pub(crate) status_counts: BTreeMap<String, usize>,
    pub(crate) candidate_count: usize,
    pub(crate) candidate_events: usize,
    pub(crate) deleted_sessions: usize,
    pub(crate) deleted_events: usize,
    pub(crate) candidate_ids: Vec<String>,
    pub(crate) actions: Vec<String>,
}

#[derive(Debug)]
struct CleanupCandidate {
    id: String,
    status: String,
    cutoff: i64,
}

pub(crate) fn agent_session_config_for_root(root: &Path) -> Result<AgentSessionConfig> {
    let path = root.join(DEFAULT_CONFIG);
    if !path.exists() {
        return Ok(AgentSessionConfig::default());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_agent_config_with_compat_defaults(
        &raw,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )?
    .agent_sessions)
}

pub(crate) fn session_filter_values(value: Option<&String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn list_agent_sessions_page(
    conn: &Connection,
    statuses: &[String],
    outcomes: &[String],
    offset: usize,
    limit: usize,
) -> Result<AgentSessionPage> {
    validate_filters("status", statuses, SESSION_STATUSES)?;
    validate_filters("outcome", outcomes, SESSION_OUTCOMES)?;
    let limit = limit.clamp(1, 200);
    let offset = offset.min(i64::MAX as usize);
    let filter = session_filter_sql(statuses, outcomes);
    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM agent_sessions{filter}"),
        [],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(&format!(
        "SELECT id, task, target, scope, runner_profile, status, outcome, summary, changed_files, \
         validation_commands, commit_hash, memory_ids, feedback_written, lease_owner, current_attempt_id, \
         lease_expires_at, attempt_count, last_event_sequence, last_heartbeat_at, started_at, updated_at, finished_at \
         FROM agent_sessions{filter} ORDER BY updated_at DESC, id DESC LIMIT ?1 OFFSET ?2"
    ))?;
    let sessions = stmt
        .query_map(
            params![limit.min(i64::MAX as usize) as i64, offset as i64],
            agent_session_from_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total = total.max(0) as usize;
    Ok(AgentSessionPage {
        version: 1,
        total,
        offset,
        limit,
        has_more: offset.saturating_add(sessions.len()) < total,
        statuses: statuses.to_vec(),
        outcomes: outcomes.to_vec(),
        sessions,
    })
}

pub(crate) fn cleanup_agent_sessions(
    conn: &Connection,
    older_than_days: i64,
    limit: usize,
    apply: bool,
) -> Result<AgentSessionCleanupReport> {
    cleanup_agent_sessions_with_policy(
        conn,
        &AgentSessionConfig::default(),
        &["completed".to_string()],
        Some(older_than_days),
        limit,
        apply,
    )
}

pub(crate) fn cleanup_agent_sessions_with_policy(
    conn: &Connection,
    policy: &AgentSessionConfig,
    statuses: &[String],
    older_than_days: Option<i64>,
    limit: usize,
    apply: bool,
) -> Result<AgentSessionCleanupReport> {
    let selected_statuses = if statuses.is_empty() {
        vec!["completed".to_string()]
    } else {
        statuses.to_vec()
    };
    validate_filters(
        "cleanup status",
        &selected_statuses,
        TERMINAL_SESSION_STATUSES,
    )?;
    let limit = limit.clamp(1, 1_000);
    let now = now_ms();
    let mut cutoffs = BTreeMap::new();
    for status in &selected_statuses {
        let days = older_than_days
            .unwrap_or_else(|| retention_days(policy, status))
            .max(0);
        cutoffs.insert(
            status.clone(),
            now.saturating_sub(days.saturating_mul(86_400_000)),
        );
    }
    let conditions = cutoffs
        .iter()
        .map(|(status, cutoff)| {
            format!("(status = '{status}' AND finished_at IS NOT NULL AND finished_at <= {cutoff})")
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut stmt = conn.prepare(&format!(
        "SELECT id, status FROM agent_sessions WHERE {conditions} \
         ORDER BY finished_at ASC, id ASC LIMIT ?1"
    ))?;
    let candidates = stmt
        .query_map(params![limit.min(i64::MAX as usize) as i64], |row| {
            let status: String = row.get(1)?;
            Ok(CleanupCandidate {
                id: row.get(0)?,
                cutoff: *cutoffs.get(&status).unwrap_or(&0),
                status,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut candidate_events = 0usize;
    let mut status_counts = BTreeMap::new();
    for candidate in &candidates {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_session_events WHERE session_id = ?1",
            params![candidate.id],
            |row| row.get(0),
        )?;
        candidate_events = candidate_events.saturating_add(count.max(0) as usize);
        *status_counts.entry(candidate.status.clone()).or_insert(0) += 1;
    }
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let mut deleted_sessions = 0usize;
    let mut deleted_events = 0usize;
    let mut actions = Vec::new();
    if apply && !candidates.is_empty() {
        let tx = conn.unchecked_transaction()?;
        for candidate in &candidates {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM agent_session_events WHERE session_id = ?1",
                params![candidate.id],
                |row| row.get(0),
            )?;
            let deleted = tx.execute(
                "DELETE FROM agent_sessions WHERE id = ?1 AND status = ?2 AND finished_at <= ?3",
                params![candidate.id, candidate.status, candidate.cutoff],
            )?;
            if deleted == 1 {
                deleted_sessions = deleted_sessions.saturating_add(1);
                deleted_events = deleted_events.saturating_add(count.max(0) as usize);
            }
        }
        tx.commit()?;
        actions.push(format!(
            "deleted {deleted_sessions} terminal session(s) and {deleted_events} event(s)"
        ));
    } else if candidates.is_empty() {
        actions.push("no terminal sessions matched the retention policy".to_string());
    } else {
        actions.push("dry_run: terminal sessions were not deleted".to_string());
    }
    let completed_days = older_than_days
        .unwrap_or(policy.completed_retention_days)
        .max(0);
    Ok(AgentSessionCleanupReport {
        version: 2,
        ok: true,
        dry_run: !apply,
        older_than_days: completed_days,
        cutoff: now.saturating_sub(completed_days.saturating_mul(86_400_000)),
        policy: policy.clone(),
        selected_statuses,
        cutoffs,
        status_counts,
        candidate_count: candidate_ids.len(),
        candidate_events,
        deleted_sessions,
        deleted_events,
        candidate_ids,
        actions,
    })
}

fn retention_days(policy: &AgentSessionConfig, status: &str) -> i64 {
    match status {
        "completed" => policy.completed_retention_days,
        "failed" => policy.failed_retention_days,
        "partial" => policy.partial_retention_days,
        "abandoned" => policy.abandoned_retention_days,
        _ => policy.completed_retention_days,
    }
}

fn session_filter_sql(statuses: &[String], outcomes: &[String]) -> String {
    let mut clauses = Vec::new();
    if !statuses.is_empty() {
        clauses.push(format!("status IN ({})", quoted_values(statuses)));
    }
    if !outcomes.is_empty() {
        clauses.push(format!("outcome IN ({})", quoted_values(outcomes)));
    }
    if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    }
}

fn quoted_values(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(",")
}

fn validate_filters(label: &str, values: &[String], allowed: &[&str]) -> Result<()> {
    for value in values {
        if !allowed.contains(&value.as_str()) {
            bail!(
                "unsupported {label}: {value}; expected one of {}",
                allowed.join(", ")
            );
        }
    }
    Ok(())
}

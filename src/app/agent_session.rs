use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentSession {
    pub(crate) id: String,
    pub(crate) task: String,
    pub(crate) target: Option<String>,
    pub(crate) scope: String,
    pub(crate) runner_profile: Option<String>,
    pub(crate) status: String,
    pub(crate) outcome: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) changed_files: Vec<String>,
    pub(crate) validation_commands: Vec<String>,
    pub(crate) commit_hash: Option<String>,
    pub(crate) memory_ids: Vec<String>,
    pub(crate) feedback_written: bool,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) finished_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionContextReport {
    version: u32,
    session: AgentSession,
    brief: BriefReport,
    impacts: Vec<ImpactReport>,
    doctrine: DoctrineReport,
    memory_ids: Vec<String>,
    receipt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionFinishReport {
    version: u32,
    session: AgentSession,
    idempotent: bool,
    evidence_present: bool,
    feedback: String,
    causal_trace: AgentSessionTrace,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionTrace {
    version: u32,
    session_id: String,
    task: String,
    recalled_memory_ids: Vec<String>,
    actions: Vec<String>,
    validations: Vec<String>,
    commit: Option<String>,
    outcome: Option<String>,
    events: Vec<AgentSessionEvent>,
}

#[derive(Debug, Serialize)]
struct AgentSessionEvent {
    id: i64,
    event_type: String,
    detail: Value,
    created_at: i64,
}

pub(crate) fn handle_agent_session(
    conn: &Connection,
    command: AgentSessionCommand,
    profile_root: &Path,
    config_provider: &str,
    config_endpoint: &str,
    config_model: &str,
) -> Result<()> {
    match command {
        AgentSessionCommand::Start {
            task,
            target,
            scope,
            runner_profile,
            json,
        } => {
            validate_scope(&scope)?;
            let session = start_agent_session(
                conn,
                &task,
                target.as_deref(),
                &scope,
                runner_profile.as_deref(),
                profile_root,
            )?;
            print_session_value(&session, json)?;
        }
        AgentSessionCommand::Context {
            id,
            limit,
            max_chars,
            embed_provider,
            embed_endpoint,
            embed_model,
            json,
        } => {
            let provider =
                select_cli_or_config(&embed_provider, DEFAULT_EMBED_PROVIDER, config_provider);
            let endpoint =
                select_cli_or_config(&embed_endpoint, DEFAULT_EMBED_ENDPOINT, config_endpoint);
            let model = select_cli_or_config(&embed_model, DEFAULT_EMBED_MODEL, config_model);
            let report =
                agent_session_context(conn, &id, limit, max_chars, provider, endpoint, model)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("session: {}", report.session.id);
                println!("task: {}", report.session.task);
                println!("memory_ids: {}", report.memory_ids.join(","));
                println!("{}", report.receipt);
            }
        }
        AgentSessionCommand::Finish {
            id,
            outcome,
            summary,
            changed_files,
            validations,
            commit,
            json,
        } => {
            let report = finish_agent_session(
                conn,
                &id,
                outcome,
                &summary,
                &changed_files,
                &validations,
                commit.as_deref(),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("session: {}", report.session.id);
                println!("status: {}", report.session.status);
                println!("feedback: {}", report.feedback);
                println!("idempotent: {}", report.idempotent);
            }
        }
        AgentSessionCommand::Status { id, limit, json } => {
            let sessions = if let Some(id) = id {
                vec![get_agent_session(conn, &id)?]
            } else {
                list_agent_sessions(conn, limit)?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            } else if sessions.is_empty() {
                println!("agent sessions: none");
            } else {
                for session in sessions {
                    println!("{}  {}  {}", session.id, session.status, session.task);
                }
            }
        }
        AgentSessionCommand::Trace { id, json } => {
            let trace = agent_session_trace(conn, &id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&trace)?);
            } else {
                println!("session: {}", trace.session_id);
                println!("memory: {}", trace.recalled_memory_ids.join(","));
                println!("actions: {}", trace.actions.join(","));
                println!("validations: {}", trace.validations.join(","));
                println!("outcome: {}", trace.outcome.as_deref().unwrap_or("active"));
            }
        }
    }
    Ok(())
}

pub(crate) fn start_agent_session(
    conn: &Connection,
    task: &str,
    target: Option<&str>,
    scope: &str,
    runner_profile: Option<&str>,
    profile_root: &Path,
) -> Result<AgentSession> {
    let task = task.trim();
    if task.is_empty() {
        bail!("agent session task must not be empty");
    }
    if let Some(profile) = runner_profile {
        ensure_runner_profile_exists(profile_root, profile)?;
    }
    let id = Uuid::new_v4().simple().to_string();
    let now = now_ms();
    conn.execute(
        "INSERT INTO agent_sessions (id, task, target, scope, runner_profile, status, started_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6)",
        params![id, task, target, scope, runner_profile, now],
    )?;
    log_agent_session_event(
        conn,
        &id,
        "started",
        &json!({
            "task": task,
            "target": target,
            "scope": scope,
            "runner_profile": runner_profile,
        }),
    )?;
    get_agent_session(conn, &id)
}

pub(crate) fn agent_session_context(
    conn: &Connection,
    id: &str,
    limit: usize,
    max_chars: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<AgentSessionContextReport> {
    let session = get_agent_session(conn, id)?;
    ensure_active(&session)?;
    let started = Instant::now();
    let brief = brief_report(
        conn,
        &BriefRequest {
            task: &session.task,
            limit,
            budget: max_chars,
            scope: Some(&session.scope),
            rules: None,
            provider,
            endpoint,
            model,
            json_out: true,
            audit_read: false,
        },
    )?;
    let mut impacts = Vec::new();
    if let Some(target) = session.target.as_deref() {
        impacts.push(impact_report(
            conn,
            &ImpactRequest {
                target,
                limit,
                budget: max_chars.min(2400),
                scope: Some(&session.scope),
                provider,
                endpoint,
                model,
                json_out: true,
                audit_read: false,
            },
        )?);
    }
    let doctrine = doctrine_report(conn, Some(&session.scope))?;
    let mut memory_ids = BTreeSet::new();
    collect_json_ids(&serde_json::to_value(&brief)?, &mut memory_ids);
    collect_json_ids(&serde_json::to_value(&impacts)?, &mut memory_ids);
    collect_json_ids(&serde_json::to_value(&doctrine)?, &mut memory_ids);
    let memory_ids = memory_ids.into_iter().collect::<Vec<_>>();
    let updated = conn.execute(
        "UPDATE agent_sessions SET memory_ids = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'active'",
        params![serde_json::to_string(&memory_ids)?, now_ms(), id],
    )?;
    if updated != 1 {
        bail!("agent session {id} finished while context was loading");
    }
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at, session_id) \
         VALUES ('agent_session_context', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session.task,
            memory_ids.join(","),
            if brief.semantic_used || impacts.iter().any(|item| item.semantic_used) { 1 } else { 0 },
            memory_ids.len().min(i64::MAX as usize) as i64,
            max_chars.min(i64::MAX as usize) as i64,
            started.elapsed().as_millis().min(i64::MAX as u128) as i64,
            now_ms(),
            id,
        ],
    )?;
    log_agent_session_event(
        conn,
        id,
        "context_loaded",
        &json!({
            "memory_ids": memory_ids,
            "target": session.target,
            "budget": max_chars,
        }),
    )?;
    let session = get_agent_session(conn, id)?;
    let receipt = memory_receipt_with_semantic(
        "agent-session context",
        if brief.semantic_used {
            MemorySemanticStatus::Used
        } else {
            MemorySemanticStatus::Fallback
        },
        &memory_ids,
        "none",
    );
    Ok(AgentSessionContextReport {
        version: 1,
        session,
        brief,
        impacts,
        doctrine,
        memory_ids,
        receipt,
    })
}

pub(crate) fn finish_agent_session(
    conn: &Connection,
    id: &str,
    outcome: AgentSessionOutcome,
    summary: &str,
    changed_files: &[String],
    validations: &[String],
    commit: Option<&str>,
) -> Result<AgentSessionFinishReport> {
    let existing = get_agent_session(conn, id)?;
    let outcome_text = outcome.to_string();
    if existing.status != "active" {
        let same = existing.outcome.as_deref() == Some(outcome_text.as_str())
            && existing.summary.as_deref() == Some(summary)
            && existing.changed_files == changed_files
            && existing.validation_commands == validations
            && existing.commit_hash.as_deref() == commit;
        if !same {
            bail!("agent session {id} is already finished with different evidence");
        }
        let evidence_present =
            !changed_files.is_empty() || !validations.is_empty() || commit.is_some();
        return Ok(AgentSessionFinishReport {
            version: 1,
            feedback: if existing.feedback_written {
                "useful"
            } else {
                "none"
            }
            .to_string(),
            causal_trace: agent_session_trace(conn, id)?,
            session: existing,
            idempotent: true,
            evidence_present,
        });
    }
    if summary.trim().is_empty() {
        bail!("agent session finish summary must not be empty");
    }
    let evidence_present = !changed_files.is_empty() || !validations.is_empty() || commit.is_some();
    let status = match outcome {
        AgentSessionOutcome::Success => "completed",
        AgentSessionOutcome::Failed => "failed",
        AgentSessionOutcome::Partial => "partial",
        AgentSessionOutcome::Abandoned => "abandoned",
    };
    let feedback_written = matches!(outcome, AgentSessionOutcome::Success)
        && evidence_present
        && !existing.memory_ids.is_empty();
    let now = now_ms();
    let updated = conn.execute(
        "UPDATE agent_sessions SET status = ?1, outcome = ?2, summary = ?3, changed_files = ?4, \
         validation_commands = ?5, commit_hash = ?6, feedback_written = ?7, updated_at = ?8, finished_at = ?8 \
         WHERE id = ?9 AND status = 'active'",
        params![
            status,
            outcome_text,
            summary,
            serde_json::to_string(changed_files)?,
            serde_json::to_string(validations)?,
            commit,
            if feedback_written { 1 } else { 0 },
            now,
            id,
        ],
    )?;
    if updated != 1 {
        let current = get_agent_session(conn, id)?;
        let same = current.outcome.as_deref() == Some(outcome_text.as_str())
            && current.summary.as_deref() == Some(summary)
            && current.changed_files == changed_files
            && current.validation_commands == validations
            && current.commit_hash.as_deref() == commit;
        if !same {
            bail!("agent session {id} was concurrently finished with different evidence");
        }
        return Ok(AgentSessionFinishReport {
            version: 1,
            feedback: if current.feedback_written {
                "useful"
            } else {
                "none"
            }
            .to_string(),
            causal_trace: agent_session_trace(conn, id)?,
            session: current,
            idempotent: true,
            evidence_present,
        });
    }
    log_agent_session_event(
        conn,
        id,
        "finished",
        &json!({
            "outcome": outcome_text,
            "summary": summary,
            "changed_files": changed_files,
            "validations": validations,
            "commit": commit,
            "evidence_present": evidence_present,
        }),
    )?;
    if feedback_written {
        let detail = json!({
            "rating": "useful",
            "ids": existing.memory_ids,
            "command": "agent_session_finish",
            "query": existing.task,
            "note": "explicit successful result with recorded evidence",
            "session_id": id,
            "outcome": outcome_text,
            "evidence": {
                "changed_files": changed_files,
                "validations": validations,
                "commit": commit,
            },
        });
        log_event(
            conn,
            "memory_feedback",
            None,
            &serde_json::to_string(&detail)?,
        )?;
        log_agent_session_event(
            conn,
            id,
            "feedback_written",
            &json!({
                "rating": "useful",
                "memory_ids": existing.memory_ids,
            }),
        )?;
    } else {
        log_agent_session_event(
            conn,
            id,
            "feedback_skipped",
            &json!({
                "reason": if !matches!(outcome, AgentSessionOutcome::Success) {
                    "outcome_not_success"
                } else if !evidence_present {
                    "missing_explicit_evidence"
                } else {
                    "no_recalled_memory"
                },
            }),
        )?;
    }
    let session = get_agent_session(conn, id)?;
    Ok(AgentSessionFinishReport {
        version: 1,
        feedback: if feedback_written { "useful" } else { "none" }.to_string(),
        causal_trace: agent_session_trace(conn, id)?,
        session,
        idempotent: false,
        evidence_present,
    })
}

pub(crate) fn list_agent_sessions(conn: &Connection, limit: usize) -> Result<Vec<AgentSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, task, target, scope, runner_profile, status, outcome, summary, changed_files, \
         validation_commands, commit_hash, memory_ids, feedback_written, started_at, updated_at, finished_at \
         FROM agent_sessions ORDER BY updated_at DESC, id DESC LIMIT ?1",
    )?;
    stmt.query_map(
        params![limit.min(i64::MAX as usize) as i64],
        agent_session_from_row,
    )?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn get_agent_session(conn: &Connection, id: &str) -> Result<AgentSession> {
    conn.query_row(
        "SELECT id, task, target, scope, runner_profile, status, outcome, summary, changed_files, \
         validation_commands, commit_hash, memory_ids, feedback_written, started_at, updated_at, finished_at \
         FROM agent_sessions WHERE id = ?1",
        params![id],
        agent_session_from_row,
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("agent session not found: {id}"))
}

fn agent_session_from_row(row: &Row<'_>) -> rusqlite::Result<AgentSession> {
    let changed_files: String = row.get(8)?;
    let validation_commands: String = row.get(9)?;
    let memory_ids: String = row.get(11)?;
    Ok(AgentSession {
        id: row.get(0)?,
        task: row.get(1)?,
        target: row.get(2)?,
        scope: row.get(3)?,
        runner_profile: row.get(4)?,
        status: row.get(5)?,
        outcome: row.get(6)?,
        summary: row.get(7)?,
        changed_files: serde_json::from_str(&changed_files).unwrap_or_default(),
        validation_commands: serde_json::from_str(&validation_commands).unwrap_or_default(),
        commit_hash: row.get(10)?,
        memory_ids: serde_json::from_str(&memory_ids).unwrap_or_default(),
        feedback_written: row.get::<_, i64>(12)? != 0,
        started_at: row.get(13)?,
        updated_at: row.get(14)?,
        finished_at: row.get(15)?,
    })
}

pub(crate) fn agent_session_trace(conn: &Connection, id: &str) -> Result<AgentSessionTrace> {
    let session = get_agent_session(conn, id)?;
    let mut stmt = conn.prepare(
        "SELECT id, event_type, detail, created_at FROM agent_session_events \
         WHERE session_id = ?1 ORDER BY created_at, id",
    )?;
    let events = stmt
        .query_map(params![id], |row| {
            let detail: String = row.get(2)?;
            Ok(AgentSessionEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                detail: serde_json::from_str(&detail).unwrap_or(Value::String(detail)),
                created_at: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(AgentSessionTrace {
        version: 1,
        session_id: session.id,
        task: session.task,
        recalled_memory_ids: session.memory_ids,
        actions: session.changed_files,
        validations: session.validation_commands,
        commit: session.commit_hash,
        outcome: session.outcome,
        events,
    })
}

fn log_agent_session_event(
    conn: &Connection,
    id: &str,
    event_type: &str,
    detail: &Value,
) -> Result<()> {
    conn.execute(
        "INSERT INTO agent_session_events (session_id, event_type, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, event_type, serde_json::to_string(detail)?, now_ms()],
    )?;
    Ok(())
}

fn ensure_active(session: &AgentSession) -> Result<()> {
    if session.status != "active" {
        bail!(
            "agent session {} is not active (status={})",
            session.id,
            session.status
        );
    }
    Ok(())
}

fn collect_json_ids(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(id) = map.get("id").and_then(Value::as_str) {
                ids.insert(id.to_string());
            }
            for child in map.values() {
                collect_json_ids(child, ids);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_json_ids(child, ids);
            }
        }
        _ => {}
    }
}

fn print_session_value(session: &AgentSession, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(session)?);
    } else {
        println!("{}", session.id);
    }
    Ok(())
}

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
    pub(crate) lease_owner: Option<String>,
    pub(crate) current_attempt_id: Option<String>,
    pub(crate) attempt_state: String,
    pub(crate) lease_expires_at: Option<i64>,
    pub(crate) attempt_count: i64,
    pub(crate) last_event_sequence: i64,
    pub(crate) last_heartbeat_at: Option<i64>,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) finished_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentSessionClaimReport {
    version: u32,
    session: AgentSession,
    owner: String,
    lease_token: String,
    attempt_id: String,
    lease_expires_at: i64,
    idempotent: bool,
    recovered: bool,
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
    metrics: AgentSessionMetrics,
    effectiveness: AgentSessionEffectiveness,
    events: Vec<AgentSessionEvent>,
}

#[derive(Debug, Serialize)]
struct AgentSessionEvent {
    id: i64,
    event_id: Option<String>,
    sequence: i64,
    attempt_id: Option<String>,
    event_type: String,
    detail: Value,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct AgentSessionMetrics {
    duration_ms: i64,
    event_count: usize,
    attempt_count: i64,
    heartbeat_count: usize,
    validation_event_count: usize,
    runner_failure_count: usize,
    recovery_count: usize,
    lease_contention_count: usize,
    orphaned_attempt_count: usize,
    recovery_latency_ms: Option<i64>,
    heartbeat_stale: bool,
    last_heartbeat_at: Option<i64>,
    heartbeat_lag_ms: Option<i64>,
    lease_state: String,
    lease_owner: Option<String>,
    lease_expires_at: Option<i64>,
    current_attempt_id: Option<String>,
    runner_profile: Option<String>,
    runner_model: Option<String>,
    last_error: Option<String>,
    evidence_count: usize,
}

#[derive(Debug, Serialize)]
struct AgentSessionEffectiveness {
    classification: String,
    evidence_present: bool,
    recalled_memory_count: usize,
    feedback_eligible: bool,
    feedback_written: bool,
    feedback_reason: String,
}

pub(crate) fn handle_agent_session(
    conn: &Connection,
    command: AgentSessionCommand,
    profile_root: &Path,
    config_provider: &str,
    config_endpoint: &str,
    config_model: &str,
    session_config: &AgentSessionConfig,
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
            owner,
            lease_token,
            json,
        } => {
            let provider =
                select_cli_or_config(&embed_provider, DEFAULT_EMBED_PROVIDER, config_provider);
            let endpoint =
                select_cli_or_config(&embed_endpoint, DEFAULT_EMBED_ENDPOINT, config_endpoint);
            let model = select_cli_or_config(&embed_model, DEFAULT_EMBED_MODEL, config_model);
            let report = agent_session_context(
                conn,
                &id,
                limit,
                max_chars,
                provider,
                endpoint,
                model,
                owner.as_deref(),
                lease_token.as_deref(),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("session: {}", report.session.id);
                println!("task: {}", report.session.task);
                println!("memory_ids: {}", report.memory_ids.join(","));
                println!("{}", report.receipt);
            }
        }
        AgentSessionCommand::Claim {
            id,
            owner,
            lease_secs,
            json,
        } => {
            let report = claim_agent_session(conn, &id, &owner, lease_secs, false)?;
            print_claim_value(&report, json)?;
        }
        AgentSessionCommand::Renew {
            id,
            owner,
            lease_token,
            lease_secs,
            json,
        } => {
            let report = renew_agent_session_lease(conn, &id, &owner, &lease_token, lease_secs)?;
            print_claim_value(&report, json)?;
        }
        AgentSessionCommand::Release {
            id,
            owner,
            lease_token,
            json,
        } => {
            let session = release_agent_session_lease(conn, &id, &owner, &lease_token)?;
            print_session_value(&session, json)?;
        }
        AgentSessionCommand::Event {
            id,
            event_type,
            detail,
            event_id,
            owner,
            lease_token,
            json,
        } => {
            let detail: Value = serde_json::from_str(&detail)
                .with_context(|| "agent session event detail must be valid JSON")?;
            let session = record_agent_session_event(
                conn,
                &id,
                &event_type.to_string(),
                &detail,
                event_id.as_deref(),
                owner.as_deref(),
                lease_token.as_deref(),
            )?;
            print_session_value(&session, json)?;
        }
        AgentSessionCommand::Recover {
            stale_after_secs,
            limit,
            owner,
            lease_secs,
            json,
        } => {
            if let Some(owner) = owner {
                let claims = claim_recoverable_agent_sessions(
                    conn,
                    stale_after_secs,
                    limit,
                    &owner,
                    lease_secs,
                )?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&claims)?);
                } else if claims.is_empty() {
                    println!("claimed recoverable agent sessions: none");
                } else {
                    for claim in claims {
                        println!(
                            "{}  {}  {}",
                            claim.session.id, claim.attempt_id, claim.session.task
                        );
                    }
                }
            } else {
                let sessions = recoverable_agent_sessions(conn, stale_after_secs, limit)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&sessions)?);
                } else if sessions.is_empty() {
                    println!("recoverable agent sessions: none");
                } else {
                    for session in sessions {
                        println!("{}  {}  {}", session.id, session.updated_at, session.task);
                    }
                }
            }
        }
        AgentSessionCommand::Finish {
            id,
            outcome,
            summary,
            changed_files,
            validations,
            commit,
            owner,
            lease_token,
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
                owner.as_deref(),
                lease_token.as_deref(),
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
        AgentSessionCommand::Status {
            id,
            limit,
            offset,
            statuses,
            outcomes,
            page,
            json,
        } => {
            let limit = limit.unwrap_or(session_config.default_page_size);
            if id.is_none() && (page || offset > 0 || !statuses.is_empty() || !outcomes.is_empty())
            {
                let report = list_agent_sessions_page(conn, &statuses, &outcomes, offset, limit)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else if report.sessions.is_empty() {
                    println!("agent sessions: none");
                } else {
                    for session in report.sessions {
                        println!(
                            "{}  {}  {}  {}",
                            session.id, session.status, session.attempt_state, session.task
                        );
                    }
                    println!(
                        "page: {}-{} of {}",
                        report.offset,
                        report.offset + report.limit,
                        report.total
                    );
                }
                return Ok(());
            }
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
                    println!(
                        "{}  {}  {}  {}",
                        session.id, session.status, session.attempt_state, session.task
                    );
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
        AgentSessionCommand::Cleanup {
            older_than_days,
            statuses,
            limit,
            apply,
            json,
        } => {
            let report = cleanup_agent_sessions_with_policy(
                conn,
                session_config,
                &statuses,
                older_than_days,
                limit,
                apply,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "agent session cleanup: {} candidate(s)",
                    report.candidate_count
                );
                println!("deleted sessions: {}", report.deleted_sessions);
                println!("deleted events: {}", report.deleted_events);
                println!("dry_run: {}", report.dry_run);
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn agent_session_context(
    conn: &Connection,
    id: &str,
    limit: usize,
    max_chars: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
    owner: Option<&str>,
    lease_token: Option<&str>,
) -> Result<AgentSessionContextReport> {
    let session = get_agent_session(conn, id)?;
    ensure_active(&session)?;
    verify_session_lease(conn, &session, owner, lease_token)?;
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_agent_session(
    conn: &Connection,
    id: &str,
    outcome: AgentSessionOutcome,
    summary: &str,
    changed_files: &[String],
    validations: &[String],
    commit: Option<&str>,
    owner: Option<&str>,
    lease_token: Option<&str>,
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
    verify_session_lease(conn, &existing, owner, lease_token)?;
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
         validation_commands = ?5, commit_hash = ?6, feedback_written = ?7, updated_at = ?8, finished_at = ?8, \
         lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL \
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

pub(crate) fn record_agent_session_event(
    conn: &Connection,
    id: &str,
    event_type: &str,
    detail: &Value,
    event_id: Option<&str>,
    owner: Option<&str>,
    lease_token: Option<&str>,
) -> Result<AgentSession> {
    const MAX_EVENT_DETAIL_BYTES: usize = 32 * 1024;
    const ALLOWED_EVENT_TYPES: &[&str] = &[
        "heartbeat",
        "runner_selected",
        "runner_started",
        "runner_completed",
        "runner_failed",
        "validation",
        "recovery",
    ];
    if !ALLOWED_EVENT_TYPES.contains(&event_type) {
        bail!("unsupported agent session event type: {event_type}");
    }
    if !detail.is_object() {
        bail!("agent session event detail must be a JSON object");
    }
    let encoded = serde_json::to_string(detail)?;
    if encoded.len() > MAX_EVENT_DETAIL_BYTES {
        bail!("agent session event detail exceeds {MAX_EVENT_DETAIL_BYTES} bytes");
    }
    if let Some(event_id) = event_id {
        validate_event_id(event_id)?;
    }

    let tx = conn.unchecked_transaction()?;
    if let Some(event_id) = event_id
        && let Some(existing) = find_agent_session_event(&tx, id, event_id)?
    {
        if existing.event_type != event_type || existing.detail != *detail {
            bail!("agent session event id {event_id} already exists with different payload");
        }
        return get_agent_session(&tx, id);
    }
    let session = get_agent_session(&tx, id)?;
    ensure_active(&session)?;
    verify_session_lease(&tx, &session, owner, lease_token)?;
    let now = now_ms();
    insert_agent_session_event(
        &tx,
        id,
        event_id,
        session.current_attempt_id.as_deref(),
        event_type,
        detail,
        now,
    )?;
    let updated = if event_type == "heartbeat" {
        tx.execute(
            "UPDATE agent_sessions SET updated_at = ?1, last_heartbeat_at = ?1 WHERE id = ?2 AND status = 'active'",
            params![now, id],
        )?
    } else {
        tx.execute(
            "UPDATE agent_sessions SET updated_at = ?1 WHERE id = ?2 AND status = 'active'",
            params![now, id],
        )?
    };
    if updated != 1 {
        bail!("agent session {id} finished while recording event");
    }
    tx.commit()?;
    get_agent_session(conn, id)
}

pub(crate) fn claim_agent_session(
    conn: &Connection,
    id: &str,
    owner: &str,
    lease_secs: u64,
    recovered: bool,
) -> Result<AgentSessionClaimReport> {
    validate_lease_owner(owner)?;
    let lease_ms = validate_lease_secs(lease_secs)?;
    let tx = conn.unchecked_transaction()?;
    let session = get_agent_session(&tx, id)?;
    ensure_active(&session)?;
    let now = now_ms();
    let existing_token: Option<String> = tx.query_row(
        "SELECT lease_token FROM agent_sessions WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if session
        .lease_expires_at
        .is_some_and(|expires| expires > now)
        && let Some(existing_owner) = session.lease_owner.as_deref()
    {
        if existing_owner == owner {
            let lease_token = existing_token
                .ok_or_else(|| anyhow::anyhow!("agent session {id} lease token is missing"))?;
            let attempt_id = session.current_attempt_id.clone().ok_or_else(|| {
                anyhow::anyhow!("agent session {id} current attempt id is missing")
            })?;
            return Ok(AgentSessionClaimReport {
                version: 1,
                owner: owner.to_string(),
                lease_token,
                attempt_id,
                lease_expires_at: session.lease_expires_at.unwrap_or(now),
                session,
                idempotent: true,
                recovered,
            });
        }
        let expires_at = session.lease_expires_at.unwrap_or(now);
        insert_agent_session_event(
            &tx,
            id,
            None,
            session.current_attempt_id.as_deref(),
            "lease_contended",
            &json!({
                "requested_owner": owner,
                "current_owner": existing_owner,
                "lease_expires_at": expires_at,
            }),
            now,
        )?;
        tx.commit()?;
        bail!(
            "agent session {id} is already leased by {existing_owner} until {}",
            expires_at
        );
    }

    let attempt_id = Uuid::new_v4().simple().to_string();
    let lease_token = Uuid::new_v4().simple().to_string();
    let lease_expires_at = now.saturating_add(lease_ms);
    let updated = tx.execute(
        "UPDATE agent_sessions SET lease_owner = ?1, lease_token = ?2, current_attempt_id = ?3, \
         lease_expires_at = ?4, attempt_count = attempt_count + 1, updated_at = ?5 \
         WHERE id = ?6 AND status = 'active' AND (lease_expires_at IS NULL OR lease_expires_at <= ?5)",
        params![owner, lease_token, attempt_id, lease_expires_at, now, id],
    )?;
    if updated != 1 {
        bail!("agent session {id} was concurrently claimed");
    }
    insert_agent_session_event(
        &tx,
        id,
        None,
        Some(&attempt_id),
        if recovered {
            "recovery"
        } else {
            "lease_claimed"
        },
        &json!({
            "owner": owner,
            "attempt_id": attempt_id,
            "lease_expires_at": lease_expires_at,
            "recovered": recovered,
            "previous_owner": session.lease_owner,
            "previous_lease_expires_at": session.lease_expires_at,
            "recovery_latency_ms": recovered.then(|| now.saturating_sub(
                session.lease_expires_at.unwrap_or(session.updated_at)
            )),
        }),
        now,
    )?;
    tx.commit()?;
    Ok(AgentSessionClaimReport {
        version: 1,
        session: get_agent_session(conn, id)?,
        owner: owner.to_string(),
        lease_token,
        attempt_id,
        lease_expires_at,
        idempotent: false,
        recovered,
    })
}

pub(crate) fn renew_agent_session_lease(
    conn: &Connection,
    id: &str,
    owner: &str,
    lease_token: &str,
    lease_secs: u64,
) -> Result<AgentSessionClaimReport> {
    validate_lease_owner(owner)?;
    validate_lease_token(lease_token)?;
    let lease_ms = validate_lease_secs(lease_secs)?;
    let tx = conn.unchecked_transaction()?;
    let session = get_agent_session(&tx, id)?;
    ensure_active(&session)?;
    verify_session_lease(&tx, &session, Some(owner), Some(lease_token))?;
    let now = now_ms();
    let lease_expires_at = now.saturating_add(lease_ms);
    let updated = tx.execute(
        "UPDATE agent_sessions SET lease_expires_at = ?1, updated_at = ?2, last_heartbeat_at = ?2 \
         WHERE id = ?3 AND status = 'active' AND lease_owner = ?4 AND lease_token = ?5 AND lease_expires_at > ?2",
        params![lease_expires_at, now, id, owner, lease_token],
    )?;
    if updated != 1 {
        bail!("agent session {id} lease expired while renewing");
    }
    let attempt_id = session
        .current_attempt_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("agent session {id} current attempt id is missing"))?;
    insert_agent_session_event(
        &tx,
        id,
        None,
        Some(&attempt_id),
        "lease_renewed",
        &json!({
            "owner": owner,
            "attempt_id": attempt_id,
            "lease_expires_at": lease_expires_at,
        }),
        now,
    )?;
    tx.commit()?;
    Ok(AgentSessionClaimReport {
        version: 1,
        session: get_agent_session(conn, id)?,
        owner: owner.to_string(),
        lease_token: lease_token.to_string(),
        attempt_id,
        lease_expires_at,
        idempotent: false,
        recovered: false,
    })
}

pub(crate) fn release_agent_session_lease(
    conn: &Connection,
    id: &str,
    owner: &str,
    lease_token: &str,
) -> Result<AgentSession> {
    validate_lease_owner(owner)?;
    validate_lease_token(lease_token)?;
    let tx = conn.unchecked_transaction()?;
    let session = get_agent_session(&tx, id)?;
    ensure_active(&session)?;
    verify_session_lease(&tx, &session, Some(owner), Some(lease_token))?;
    let now = now_ms();
    let updated = tx.execute(
        "UPDATE agent_sessions SET lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = ?1 \
         WHERE id = ?2 AND status = 'active' AND lease_owner = ?3 AND lease_token = ?4",
        params![now, id, owner, lease_token],
    )?;
    if updated != 1 {
        bail!("agent session {id} lease changed while releasing");
    }
    insert_agent_session_event(
        &tx,
        id,
        None,
        session.current_attempt_id.as_deref(),
        "lease_released",
        &json!({
            "owner": owner,
            "attempt_id": session.current_attempt_id,
        }),
        now,
    )?;
    tx.commit()?;
    get_agent_session(conn, id)
}

pub(crate) fn claim_recoverable_agent_sessions(
    conn: &Connection,
    stale_after_secs: u64,
    limit: usize,
    owner: &str,
    lease_secs: u64,
) -> Result<Vec<AgentSessionClaimReport>> {
    let candidates = recoverable_agent_sessions(conn, stale_after_secs, limit)?;
    let mut claims = Vec::new();
    for candidate in candidates {
        match claim_agent_session(conn, &candidate.id, owner, lease_secs, true) {
            Ok(claim) => claims.push(claim),
            Err(err)
                if err.to_string().contains("concurrently claimed")
                    || err.to_string().contains("already leased") => {}
            Err(err) => return Err(err),
        }
    }
    Ok(claims)
}

pub(crate) fn recoverable_agent_sessions(
    conn: &Connection,
    stale_after_secs: u64,
    limit: usize,
) -> Result<Vec<AgentSession>> {
    let stale_ms = stale_after_secs
        .min((i64::MAX / 1000) as u64)
        .saturating_mul(1000) as i64;
    let threshold = now_ms().saturating_sub(stale_ms);
    let now = now_ms();
    let mut stmt = conn.prepare(
        "SELECT id, task, target, scope, runner_profile, status, outcome, summary, changed_files, \
         validation_commands, commit_hash, memory_ids, feedback_written, lease_owner, current_attempt_id, \
         lease_expires_at, attempt_count, last_event_sequence, last_heartbeat_at, started_at, updated_at, finished_at \
         FROM agent_sessions WHERE status = 'active' AND \
         ((lease_expires_at IS NOT NULL AND lease_expires_at <= ?2) OR \
          (lease_expires_at IS NULL AND updated_at <= ?1)) \
         ORDER BY COALESCE(lease_expires_at, updated_at) ASC, id ASC LIMIT ?3",
    )?;
    stmt.query_map(
        params![threshold, now, limit.min(i64::MAX as usize) as i64],
        agent_session_from_row,
    )?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn list_agent_sessions(conn: &Connection, limit: usize) -> Result<Vec<AgentSession>> {
    let mut stmt = conn.prepare(
        "SELECT id, task, target, scope, runner_profile, status, outcome, summary, changed_files, \
         validation_commands, commit_hash, memory_ids, feedback_written, lease_owner, current_attempt_id, \
         lease_expires_at, attempt_count, last_event_sequence, last_heartbeat_at, started_at, updated_at, finished_at \
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
         validation_commands, commit_hash, memory_ids, feedback_written, lease_owner, current_attempt_id, \
         lease_expires_at, attempt_count, last_event_sequence, last_heartbeat_at, started_at, updated_at, finished_at \
         FROM agent_sessions WHERE id = ?1",
        params![id],
        agent_session_from_row,
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("agent session not found: {id}"))
}

pub(crate) fn agent_session_from_row(row: &Row<'_>) -> rusqlite::Result<AgentSession> {
    let changed_files: String = row.get(8)?;
    let validation_commands: String = row.get(9)?;
    let memory_ids: String = row.get(11)?;
    let status: String = row.get(5)?;
    let current_attempt_id: Option<String> = row.get(14)?;
    let lease_expires_at: Option<i64> = row.get(15)?;
    let attempt_count: i64 = row.get(16)?;
    let attempt_state = agent_session_attempt_state(
        &status,
        current_attempt_id.as_deref(),
        lease_expires_at,
        attempt_count,
    );
    Ok(AgentSession {
        id: row.get(0)?,
        task: row.get(1)?,
        target: row.get(2)?,
        scope: row.get(3)?,
        runner_profile: row.get(4)?,
        status,
        outcome: row.get(6)?,
        summary: row.get(7)?,
        changed_files: serde_json::from_str(&changed_files).unwrap_or_default(),
        validation_commands: serde_json::from_str(&validation_commands).unwrap_or_default(),
        commit_hash: row.get(10)?,
        memory_ids: serde_json::from_str(&memory_ids).unwrap_or_default(),
        feedback_written: row.get::<_, i64>(12)? != 0,
        lease_owner: row.get(13)?,
        current_attempt_id,
        attempt_state,
        lease_expires_at,
        attempt_count,
        last_event_sequence: row.get(17)?,
        last_heartbeat_at: row.get(18)?,
        started_at: row.get(19)?,
        updated_at: row.get(20)?,
        finished_at: row.get(21)?,
    })
}

fn agent_session_attempt_state(
    status: &str,
    current_attempt_id: Option<&str>,
    lease_expires_at: Option<i64>,
    attempt_count: i64,
) -> String {
    if status != "active" {
        return status.to_string();
    }
    if current_attempt_id.is_some() {
        match lease_expires_at {
            Some(expires_at) if expires_at <= now_ms() => "stale",
            Some(_) => "leased",
            None => "released",
        }
        .to_string()
    } else if attempt_count > 0 {
        "released".to_string()
    } else {
        "idle".to_string()
    }
}

pub(crate) fn agent_session_trace(conn: &Connection, id: &str) -> Result<AgentSessionTrace> {
    let session = get_agent_session(conn, id)?;
    let mut stmt = conn.prepare(
        "SELECT id, event_id, sequence, attempt_id, event_type, detail, created_at \
         FROM agent_session_events WHERE session_id = ?1 ORDER BY sequence, id",
    )?;
    let events = stmt
        .query_map(params![id], |row| {
            let detail: String = row.get(5)?;
            Ok(AgentSessionEvent {
                id: row.get(0)?,
                event_id: row.get(1)?,
                sequence: row.get(2)?,
                attempt_id: row.get(3)?,
                event_type: row.get(4)?,
                detail: serde_json::from_str(&detail).unwrap_or(Value::String(detail)),
                created_at: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let metrics = agent_session_metrics(&session, &events);
    let effectiveness = agent_session_effectiveness(&session);
    Ok(AgentSessionTrace {
        version: 2,
        session_id: session.id,
        task: session.task,
        recalled_memory_ids: session.memory_ids,
        actions: session.changed_files,
        validations: session.validation_commands,
        commit: session.commit_hash,
        outcome: session.outcome,
        metrics,
        effectiveness,
        events,
    })
}

fn log_agent_session_event(
    conn: &Connection,
    id: &str,
    event_type: &str,
    detail: &Value,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let attempt_id: Option<String> = tx.query_row(
        "SELECT current_attempt_id FROM agent_sessions WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    insert_agent_session_event(
        &tx,
        id,
        None,
        attempt_id.as_deref(),
        event_type,
        detail,
        now_ms(),
    )?;
    tx.commit()?;
    Ok(())
}

fn insert_agent_session_event(
    conn: &Connection,
    session_id: &str,
    event_id: Option<&str>,
    attempt_id: Option<&str>,
    event_type: &str,
    detail: &Value,
    created_at: i64,
) -> Result<AgentSessionEvent> {
    let sequence: i64 = conn.query_row(
        "SELECT last_event_sequence + 1 FROM agent_sessions WHERE id = ?1",
        params![session_id],
        |row| row.get(0),
    )?;
    let updated = conn.execute(
        "UPDATE agent_sessions SET last_event_sequence = ?1 WHERE id = ?2 AND last_event_sequence < ?1",
        params![sequence, session_id],
    )?;
    if updated != 1 {
        bail!("agent session {session_id} event sequence changed concurrently");
    }
    conn.execute(
        "INSERT INTO agent_session_events \
         (session_id, event_id, sequence, attempt_id, event_type, detail, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            session_id,
            event_id,
            sequence,
            attempt_id,
            event_type,
            serde_json::to_string(detail)?,
            created_at,
        ],
    )?;
    Ok(AgentSessionEvent {
        id: conn.last_insert_rowid(),
        event_id: event_id.map(str::to_string),
        sequence,
        attempt_id: attempt_id.map(str::to_string),
        event_type: event_type.to_string(),
        detail: detail.clone(),
        created_at,
    })
}

fn find_agent_session_event(
    conn: &Connection,
    session_id: &str,
    event_id: &str,
) -> Result<Option<AgentSessionEvent>> {
    conn.query_row(
        "SELECT id, event_id, sequence, attempt_id, event_type, detail, created_at \
         FROM agent_session_events WHERE session_id = ?1 AND event_id = ?2",
        params![session_id, event_id],
        |row| {
            let detail: String = row.get(5)?;
            Ok(AgentSessionEvent {
                id: row.get(0)?,
                event_id: row.get(1)?,
                sequence: row.get(2)?,
                attempt_id: row.get(3)?,
                event_type: row.get(4)?,
                detail: serde_json::from_str(&detail).unwrap_or(Value::String(detail)),
                created_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn agent_session_metrics(
    session: &AgentSession,
    events: &[AgentSessionEvent],
) -> AgentSessionMetrics {
    let now = now_ms();
    let end = session.finished_at.unwrap_or(now);
    let heartbeat_count = events
        .iter()
        .filter(|event| matches!(event.event_type.as_str(), "heartbeat" | "lease_renewed"))
        .count();
    let validation_event_count = events
        .iter()
        .filter(|event| event.event_type == "validation")
        .count();
    let runner_failure_count = events
        .iter()
        .filter(|event| event.event_type == "runner_failed")
        .count();
    let recovery_count = events
        .iter()
        .filter(|event| event.event_type == "recovery")
        .count();
    let lease_contention_count = events
        .iter()
        .filter(|event| event.event_type == "lease_contended")
        .count();
    let recovery_latency_ms = events.iter().rev().find_map(|event| {
        (event.event_type == "recovery")
            .then(|| {
                event
                    .detail
                    .get("recovery_latency_ms")
                    .and_then(Value::as_i64)
            })
            .flatten()
    });
    let attempt_ids = events
        .iter()
        .filter(|event| matches!(event.event_type.as_str(), "lease_claimed" | "recovery"))
        .filter_map(|event| event.attempt_id.clone())
        .collect::<BTreeSet<_>>();
    let terminal_attempt_ids = events
        .iter()
        .filter(|event| {
            matches!(
                event.event_type.as_str(),
                "runner_completed" | "runner_failed" | "finished" | "lease_released"
            )
        })
        .filter_map(|event| event.attempt_id.clone())
        .collect::<BTreeSet<_>>();
    let orphaned_attempt_count = attempt_ids
        .iter()
        .filter(|attempt_id| {
            !(terminal_attempt_ids.contains(*attempt_id)
                || session.status == "active"
                    && session.current_attempt_id.as_ref() == Some(*attempt_id))
        })
        .count();
    let last_heartbeat_at = session.last_heartbeat_at.or_else(|| {
        events
            .iter()
            .rev()
            .find(|event| matches!(event.event_type.as_str(), "heartbeat" | "lease_renewed"))
            .map(|event| event.created_at)
    });
    let runner_model = events.iter().rev().find_map(|event| {
        if event.event_type != "runner_selected" {
            return None;
        }
        event
            .detail
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let last_error = events.iter().rev().find_map(|event| {
        if event.event_type != "runner_failed" {
            return None;
        }
        ["error", "message", "stderr"]
            .into_iter()
            .find_map(|key| event.detail.get(key).and_then(Value::as_str))
            .map(str::to_string)
    });
    let lease_state = if session.status != "active" {
        "released"
    } else if session.lease_owner.is_none() {
        "unclaimed"
    } else if session
        .lease_expires_at
        .is_some_and(|expires| expires > now)
    {
        "active"
    } else {
        "expired"
    };
    let heartbeat_stale = session.status == "active"
        && (session
            .lease_expires_at
            .is_some_and(|expires_at| expires_at <= now)
            || last_heartbeat_at.is_some_and(|at| now.saturating_sub(at) > 300_000));
    AgentSessionMetrics {
        duration_ms: end.saturating_sub(session.started_at),
        event_count: events.len(),
        attempt_count: session.attempt_count,
        heartbeat_count,
        validation_event_count,
        runner_failure_count,
        recovery_count,
        lease_contention_count,
        orphaned_attempt_count,
        recovery_latency_ms,
        heartbeat_stale,
        last_heartbeat_at,
        heartbeat_lag_ms: last_heartbeat_at.map(|at| now.saturating_sub(at)),
        lease_state: lease_state.to_string(),
        lease_owner: session.lease_owner.clone(),
        lease_expires_at: session.lease_expires_at,
        current_attempt_id: session.current_attempt_id.clone(),
        runner_profile: session.runner_profile.clone(),
        runner_model,
        last_error,
        evidence_count: session.changed_files.len()
            + session.validation_commands.len()
            + usize::from(session.commit_hash.is_some()),
    }
}

fn agent_session_effectiveness(session: &AgentSession) -> AgentSessionEffectiveness {
    let evidence_present = !session.changed_files.is_empty()
        || !session.validation_commands.is_empty()
        || session.commit_hash.is_some();
    let feedback_eligible = session.outcome.as_deref() == Some("success")
        && evidence_present
        && !session.memory_ids.is_empty();
    let classification = match (
        session.status.as_str(),
        session.outcome.as_deref(),
        evidence_present,
    ) {
        ("active", _, _) => "active",
        (_, Some("success"), true) => "validated_success",
        (_, Some("success"), false) => "unvalidated_success",
        (_, Some("failed" | "partial"), true) => "failed_with_evidence",
        (_, Some("failed" | "partial" | "abandoned"), false) => "incomplete_without_evidence",
        _ => "completed",
    };
    let feedback_reason = if session.feedback_written {
        "explicit_success_with_evidence"
    } else if session.outcome.as_deref() != Some("success") {
        "outcome_not_success"
    } else if !evidence_present {
        "missing_explicit_evidence"
    } else if session.memory_ids.is_empty() {
        "no_recalled_memory"
    } else {
        "not_written"
    };
    AgentSessionEffectiveness {
        classification: classification.to_string(),
        evidence_present,
        recalled_memory_count: session.memory_ids.len(),
        feedback_eligible,
        feedback_written: session.feedback_written,
        feedback_reason: feedback_reason.to_string(),
    }
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

fn verify_session_lease(
    conn: &Connection,
    session: &AgentSession,
    owner: Option<&str>,
    lease_token: Option<&str>,
) -> Result<()> {
    match (session.lease_owner.as_deref(), session.lease_expires_at) {
        (None, _) => {
            if owner.is_some() || lease_token.is_some() {
                bail!("agent session {} is not currently leased", session.id);
            }
            Ok(())
        }
        (Some(expected_owner), Some(expires_at)) => {
            let now = now_ms();
            if expires_at <= now {
                bail!(
                    "agent session {} lease expired at {expires_at}; claim a new attempt",
                    session.id
                );
            }
            let owner = owner
                .ok_or_else(|| anyhow::anyhow!("agent session {} requires --owner", session.id))?;
            let lease_token = lease_token.ok_or_else(|| {
                anyhow::anyhow!("agent session {} requires --lease-token", session.id)
            })?;
            validate_lease_owner(owner)?;
            validate_lease_token(lease_token)?;
            if owner != expected_owner {
                bail!(
                    "agent session {} lease is owned by {expected_owner}, not {owner}",
                    session.id
                );
            }
            let expected_token: Option<String> = conn.query_row(
                "SELECT lease_token FROM agent_sessions WHERE id = ?1",
                params![session.id],
                |row| row.get(0),
            )?;
            if expected_token.as_deref() != Some(lease_token) {
                bail!("agent session {} lease token does not match", session.id);
            }
            Ok(())
        }
        (Some(_), None) => bail!("agent session {} lease expiry is missing", session.id),
    }
}

fn validate_lease_owner(owner: &str) -> Result<()> {
    let owner = owner.trim();
    if owner.is_empty() {
        bail!("agent session lease owner must not be empty");
    }
    if owner.len() > 128 {
        bail!("agent session lease owner exceeds 128 bytes");
    }
    Ok(())
}

fn validate_lease_token(lease_token: &str) -> Result<()> {
    if lease_token.is_empty() || lease_token.len() > 128 {
        bail!("agent session lease token must contain 1..=128 bytes");
    }
    Ok(())
}

fn validate_event_id(event_id: &str) -> Result<()> {
    if event_id.is_empty() || event_id.len() > 128 {
        bail!("agent session event id must contain 1..=128 bytes");
    }
    Ok(())
}

fn validate_lease_secs(lease_secs: u64) -> Result<i64> {
    const MIN_LEASE_SECS: u64 = 5;
    const MAX_LEASE_SECS: u64 = 6 * 60 * 60;
    if !(MIN_LEASE_SECS..=MAX_LEASE_SECS).contains(&lease_secs) {
        bail!("agent session lease must be between {MIN_LEASE_SECS} and {MAX_LEASE_SECS} seconds");
    }
    Ok((lease_secs * 1000) as i64)
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

fn print_claim_value(report: &AgentSessionClaimReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("{}", report.session.id);
        println!("attempt_id: {}", report.attempt_id);
        println!("lease_token: {}", report.lease_token);
        println!("lease_expires_at: {}", report.lease_expires_at);
    }
    Ok(())
}

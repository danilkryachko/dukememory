use super::*;

pub(crate) struct DaemonRequest<'a> {
    pub(crate) interval_secs: u64,
    pub(crate) once: bool,
    pub(crate) quiet: bool,
    pub(crate) auto_ingest: bool,
    pub(crate) autopilot: bool,
    pub(crate) session_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) status_file: &'a Path,
    pub(crate) backup_keep: usize,
    pub(crate) backup_every_secs: u64,
    pub(crate) cleanup_audit_keep: usize,
    pub(crate) db: &'a Path,
    pub(crate) scope: &'a str,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonStatus {
    version: u32,
    updated_at: i64,
    autopilot: bool,
    tick_ok: bool,
    indexed: usize,
    skipped: usize,
    auto_inbox_added: usize,
    secrets: usize,
    pending: usize,
    backup_ran: bool,
    backup_dir: String,
    cleanup_ran: bool,
    next_backup_after: i64,
    error: Option<String>,
}

pub(crate) fn run_daemon(conn: &Connection, request: DaemonRequest<'_>) -> Result<()> {
    validate_scope(request.scope)?;
    loop {
        acquire_lock(
            conn,
            "daemon",
            "dukememory",
            (request.interval_secs.max(1) as i64) * 2_000,
        )?;
        let tick = run_daemon_tick(conn, &request);
        let release = release_lock(conn, "daemon");
        match (tick, release) {
            (Ok(()), Ok(())) => {}
            (Err(tick_err), Ok(())) => return Err(tick_err),
            (Ok(()), Err(release_err)) => return Err(release_err),
            (Err(tick_err), Err(release_err)) => {
                return Err(tick_err)
                    .with_context(|| format!("also failed to release daemon lock: {release_err}"));
            }
        }
        if request.once {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(request.interval_secs.max(1)));
    }
}

fn run_daemon_tick(conn: &Connection, request: &DaemonRequest<'_>) -> Result<()> {
    let tick = (|| -> Result<DaemonStatus> {
        let embed = embeddings::embed_index(
            conn,
            request.provider,
            request.endpoint,
            request.model,
            &[],
            None,
            false,
        )?;
        let auto_report = if request.auto_ingest {
            Some(auto_ingest_sessions(
                conn,
                request.session_dir,
                request.scope,
                false,
                DEFAULT_EMBED_ENDPOINT,
                "qwen3:14b",
                false,
            )?)
        } else {
            None
        };
        let backup_ran =
            request.autopilot && daemon_backup_due(request.status_file, request.backup_every_secs);
        if backup_ran {
            ops::run_backup_policy_quiet(request.db, request.backup_dir, request.backup_keep)?;
        }
        let cleanup_ran = request.autopilot;
        if cleanup_ran {
            ops::run_cleanup_quiet(conn, request.cleanup_audit_keep, 30)?;
        }
        let secrets = scan_secret_findings(conn)?.len();
        let pending = list_inbox(conn, "pending", usize::MAX)?.len();
        let auto_added = auto_report.as_ref().map(|r| r.inbox_added).unwrap_or(0);
        Ok(DaemonStatus {
            version: 1,
            updated_at: now_ms(),
            autopilot: request.autopilot,
            tick_ok: true,
            indexed: embed.indexed,
            skipped: embed.skipped,
            auto_inbox_added: auto_added,
            secrets,
            pending,
            backup_ran,
            backup_dir: request.backup_dir.display().to_string(),
            cleanup_ran,
            next_backup_after: now_ms() + (request.backup_every_secs as i64) * 1000,
            error: None,
        })
    })();
    match tick {
        Ok(status) => {
            write_daemon_status(request.status_file, &status)?;
            log_event(
                conn,
                "daemon_tick",
                None,
                &serde_json::to_string(&json!({
                    "version": 1,
                    "status_file": request.status_file.display().to_string(),
                    "autopilot": status.autopilot,
                    "tick_ok": status.tick_ok,
                    "indexed": status.indexed,
                    "skipped": status.skipped,
                    "auto_inbox_added": status.auto_inbox_added,
                    "secrets": status.secrets,
                    "pending": status.pending,
                    "backup_ran": status.backup_ran,
                    "backup_dir": status.backup_dir,
                    "cleanup_ran": status.cleanup_ran,
                    "next_backup_after": status.next_backup_after,
                }))?,
            )?;
            if !request.quiet {
                println!(
                    "daemon_tick indexed={} skipped={} auto_inbox_added={} secrets={} pending={} backup_ran={} cleanup_ran={} status={}",
                    status.indexed,
                    status.skipped,
                    status.auto_inbox_added,
                    status.secrets,
                    status.pending,
                    status.backup_ran,
                    status.cleanup_ran,
                    request.status_file.display()
                );
            }
            Ok(())
        }
        Err(err) => {
            let status = DaemonStatus {
                version: 1,
                updated_at: now_ms(),
                autopilot: request.autopilot,
                tick_ok: false,
                indexed: 0,
                skipped: 0,
                auto_inbox_added: 0,
                secrets: 0,
                pending: 0,
                backup_ran: false,
                backup_dir: request.backup_dir.display().to_string(),
                cleanup_ran: false,
                next_backup_after: now_ms(),
                error: Some(format!("{err:#}")),
            };
            let _ = write_daemon_status(request.status_file, &status);
            let _ = log_event(
                conn,
                "daemon_tick_failed",
                None,
                &serde_json::to_string(&json!({
                    "version": 1,
                    "status_file": request.status_file.display().to_string(),
                    "autopilot": request.autopilot,
                    "tick_ok": false,
                    "backup_dir": request.backup_dir.display().to_string(),
                    "error": status.error,
                }))
                .unwrap_or_else(|_| {
                    "{\"error\":\"failed to serialize daemon failure\"}".to_string()
                }),
            );
            Err(err)
        }
    }
}

fn daemon_backup_due(status_file: &Path, backup_every_secs: u64) -> bool {
    if backup_every_secs == 0 || !status_file.exists() {
        return true;
    }
    let Ok(raw) = fs::read_to_string(status_file) else {
        return true;
    };
    let Ok(status) = serde_json::from_str::<DaemonStatus>(&raw) else {
        return true;
    };
    now_ms() >= status.next_backup_after
}

fn write_daemon_status(path: &Path, status: &DaemonStatus) -> Result<()> {
    write_file(path, serde_json::to_string_pretty(status)?.as_bytes())
}

#[derive(Debug, Serialize)]
struct AutopilotDoctorReport {
    ok: bool,
    status_file: String,
    status_fresh: bool,
    status_age_secs: Option<i64>,
    session_dir_ok: bool,
    backup_ok: bool,
    latest_backup: Option<String>,
    lock_ok: bool,
    endpoint_ok: bool,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutopilotRepairReport {
    ok: bool,
    before: AutopilotDoctorReport,
    after: AutopilotDoctorReport,
    actions_taken: Vec<String>,
    actions_skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AutopilotHistoryEvent {
    id: i64,
    event_type: String,
    created_at: i64,
    detail: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutopilotReport {
    ok: bool,
    generated_at: i64,
    current_status: Option<DaemonStatus>,
    doctor: AutopilotDoctorReport,
    history: Vec<AutopilotHistoryEvent>,
    total_ticks: usize,
    failed_ticks: usize,
    backups_created: usize,
    inbox_added: usize,
    current_pending: usize,
    embeddings_indexed: usize,
    embeddings_stale: usize,
    embeddings_missing: usize,
    latest_backup: Option<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutopilotAlert {
    ok: bool,
    level: String,
    generated_at: i64,
    violations: Vec<String>,
    recommendations: Vec<String>,
    report: AutopilotReport,
}

pub(crate) fn handle_autopilot(
    conn: &Connection,
    db: &Path,
    command: AutopilotCommand,
) -> Result<()> {
    match command {
        AutopilotCommand::Status { status_file, json } => {
            let status = read_daemon_status(&status_file)?;
            print_autopilot_status(&status_file, &status, json)
        }
        AutopilotCommand::Doctor {
            status_file,
            session_dir,
            backup_dir,
            max_status_age_secs,
            provider,
            endpoint,
            repair,
            json,
        } => {
            if repair {
                let report = autopilot_repair(
                    conn,
                    db,
                    AutopilotRepairRequest {
                        status_file: &status_file,
                        session_dir: &session_dir,
                        backup_dir: &backup_dir,
                        backup_keep: 10,
                        cleanup_audit_keep: 5000,
                        scope: "project",
                        provider: &provider,
                        endpoint: &endpoint,
                        model: DEFAULT_EMBED_MODEL,
                        max_status_age_secs,
                    },
                )?;
                print_autopilot_repair(report, json)
            } else {
                let report = autopilot_doctor(
                    conn,
                    &status_file,
                    &session_dir,
                    &backup_dir,
                    max_status_age_secs,
                    &provider,
                    &endpoint,
                );
                print_autopilot_doctor(report?, json)
            }
        }
        AutopilotCommand::Repair {
            status_file,
            session_dir,
            backup_dir,
            backup_keep,
            cleanup_audit_keep,
            scope,
            provider,
            endpoint,
            model,
            json,
        } => {
            let report = autopilot_repair(
                conn,
                db,
                AutopilotRepairRequest {
                    status_file: &status_file,
                    session_dir: &session_dir,
                    backup_dir: &backup_dir,
                    backup_keep,
                    cleanup_audit_keep,
                    scope: &scope,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                    max_status_age_secs: 180,
                },
            )?;
            print_autopilot_repair(report, json)
        }
        AutopilotCommand::History { limit, json } => {
            let history = autopilot_history(conn, limit)?;
            print_autopilot_history(&history, json)
        }
        AutopilotCommand::Report {
            status_file,
            session_dir,
            backup_dir,
            history_limit,
            provider,
            endpoint,
            model,
            json,
        } => {
            let report = autopilot_report(
                conn,
                AutopilotReportRequest {
                    status_file: &status_file,
                    session_dir: &session_dir,
                    backup_dir: &backup_dir,
                    history_limit,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )?;
            print_autopilot_report(&report, json)
        }
        AutopilotCommand::ExportStatus {
            output,
            status_file,
            session_dir,
            backup_dir,
            history_limit,
            provider,
            endpoint,
            model,
        } => {
            let report = autopilot_report(
                conn,
                AutopilotReportRequest {
                    status_file: &status_file,
                    session_dir: &session_dir,
                    backup_dir: &backup_dir,
                    history_limit,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )?;
            write_file(&output, serde_json::to_string_pretty(&report)?.as_bytes())?;
            println!("{}", output.display());
            Ok(())
        }
        AutopilotCommand::Alert {
            status_file,
            session_dir,
            backup_dir,
            history_limit,
            max_pending,
            max_failed_ticks,
            max_status_age_secs,
            max_embedding_stale,
            require_backup,
            require_endpoint,
            provider,
            endpoint,
            model,
            write_alert,
            json,
        } => {
            let alert = autopilot_alert(
                conn,
                AutopilotAlertRequest {
                    status_file: &status_file,
                    session_dir: &session_dir,
                    backup_dir: &backup_dir,
                    history_limit,
                    max_pending,
                    max_failed_ticks,
                    max_status_age_secs,
                    max_embedding_stale,
                    require_backup,
                    require_endpoint,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )?;
            let ok = alert.ok;
            print_autopilot_alert(&alert, json, write_alert.as_deref())?;
            if ok {
                Ok(())
            } else {
                std::process::exit(2);
            }
        }
        AutopilotCommand::RunOnce {
            session_dir,
            backup_dir,
            status_file,
            backup_keep,
            backup_every_secs,
            cleanup_audit_keep,
            scope,
            provider,
            endpoint,
            model,
            json,
        } => {
            run_daemon(
                conn,
                DaemonRequest {
                    interval_secs: 1,
                    once: true,
                    quiet: json,
                    auto_ingest: true,
                    autopilot: true,
                    session_dir: &session_dir,
                    backup_dir: &backup_dir,
                    status_file: &status_file,
                    backup_keep,
                    backup_every_secs,
                    cleanup_audit_keep,
                    db,
                    scope: &scope,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )?;
            let status = read_daemon_status(&status_file)?;
            print_autopilot_status(&status_file, &status, json)
        }
        AutopilotCommand::Install {
            output,
            interval_secs,
            session_dir,
            backup_dir,
            status_file,
            force,
            dry_run,
        } => ops::write_autopilot_launchd_plist(ops::AutopilotLaunchdRequest {
            db,
            output: &output,
            interval_secs,
            session_dir: &session_dir,
            backup_dir: &backup_dir,
            status_file: &status_file,
            force,
            dry_run,
        }),
    }
}

pub(crate) struct AutopilotRepairRequest<'a> {
    pub(crate) status_file: &'a Path,
    pub(crate) session_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) backup_keep: usize,
    pub(crate) cleanup_audit_keep: usize,
    pub(crate) scope: &'a str,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) max_status_age_secs: u64,
}

pub(crate) fn autopilot_repair(
    conn: &Connection,
    db: &Path,
    request: AutopilotRepairRequest<'_>,
) -> Result<AutopilotRepairReport> {
    let before = autopilot_doctor(
        conn,
        request.status_file,
        request.session_dir,
        request.backup_dir,
        request.max_status_age_secs,
        request.provider,
        request.endpoint,
    )?;
    let mut actions_taken = Vec::new();
    let mut actions_skipped = Vec::new();

    if !request.session_dir.exists() {
        fs::create_dir_all(request.session_dir)
            .with_context(|| format!("failed to create {}", request.session_dir.display()))?;
        actions_taken.push(format!(
            "created_session_dir:{}",
            request.session_dir.display()
        ));
    } else {
        actions_skipped.push("session_dir_exists".to_string());
    }
    if !request.backup_dir.exists() {
        fs::create_dir_all(request.backup_dir)
            .with_context(|| format!("failed to create {}", request.backup_dir.display()))?;
        actions_taken.push(format!(
            "created_backup_dir:{}",
            request.backup_dir.display()
        ));
    } else {
        actions_skipped.push("backup_dir_exists".to_string());
    }

    let expired_locks = clear_expired_daemon_locks(conn)?;
    if expired_locks > 0 {
        actions_taken.push(format!("cleared_expired_daemon_locks:{expired_locks}"));
    } else {
        actions_skipped.push("no_expired_daemon_lock".to_string());
    }

    let endpoint_ok = autopilot_endpoint_ok(request.provider, request.endpoint);
    let embed = embeddings::embed_status(conn, request.provider, request.endpoint, request.model)?;
    let needs_tick = !before.status_fresh
        || !before.backup_ok
        || embed.stale > 0
        || embed.missing > 0
        || !request.status_file.exists();
    if needs_tick && endpoint_ok {
        run_daemon(
            conn,
            DaemonRequest {
                interval_secs: 1,
                once: true,
                quiet: true,
                auto_ingest: true,
                autopilot: true,
                session_dir: request.session_dir,
                backup_dir: request.backup_dir,
                status_file: request.status_file,
                backup_keep: request.backup_keep,
                backup_every_secs: 0,
                cleanup_audit_keep: request.cleanup_audit_keep,
                db,
                scope: request.scope,
                provider: request.provider,
                endpoint: request.endpoint,
                model: request.model,
            },
        )?;
        actions_taken.push("ran_autopilot_tick".to_string());
    } else if needs_tick {
        actions_skipped.push("autopilot_tick_skipped_endpoint_unreachable".to_string());
    } else {
        actions_skipped.push("autopilot_tick_not_needed".to_string());
    }

    let after = autopilot_doctor(
        conn,
        request.status_file,
        request.session_dir,
        request.backup_dir,
        request.max_status_age_secs,
        request.provider,
        request.endpoint,
    )?;
    Ok(AutopilotRepairReport {
        ok: after.ok,
        before,
        after,
        actions_taken,
        actions_skipped,
    })
}

fn print_autopilot_repair(report: AutopilotRepairReport, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("repair: {}", if report.ok { "ok" } else { "warn" });
        for action in &report.actions_taken {
            println!("action: {action}");
        }
        for action in &report.actions_skipped {
            println!("skipped: {action}");
        }
        for recommendation in &report.after.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

fn autopilot_history(conn: &Connection, limit: usize) -> Result<Vec<AutopilotHistoryEvent>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, event_type, detail, created_at
        FROM memory_events
        WHERE event_type IN ('daemon_tick', 'daemon_tick_failed')
        ORDER BY created_at DESC, id DESC
        LIMIT ?1
        "#,
    )?;
    let rows = stmt.query_map(params![limit.min(i64::MAX as usize) as i64], |row| {
        let detail_text: String = row.get(2)?;
        let detail = serde_json::from_str(&detail_text).unwrap_or_else(|_| {
            json!({
                "version": 0,
                "legacy_detail": detail_text
            })
        });
        Ok(AutopilotHistoryEvent {
            id: row.get(0)?,
            event_type: row.get(1)?,
            detail,
            created_at: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn print_autopilot_history(history: &[AutopilotHistoryEvent], json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(history)?);
    } else if history.is_empty() {
        println!("autopilot_history: none");
    } else {
        for event in history {
            let indexed = event
                .detail
                .get("indexed")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let skipped = event
                .detail
                .get("skipped")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let pending = event
                .detail
                .get("pending")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let backup = event
                .detail
                .get("backup_ran")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let cleanup = event
                .detail
                .get("cleanup_ran")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            println!(
                "{} {} indexed={} skipped={} pending={} backup_ran={} cleanup_ran={}",
                event.created_at, event.event_type, indexed, skipped, pending, backup, cleanup
            );
        }
    }
    Ok(())
}

pub(crate) struct AutopilotReportRequest<'a> {
    pub(crate) status_file: &'a Path,
    pub(crate) session_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) history_limit: usize,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
}

pub(crate) struct AutopilotAlertRequest<'a> {
    pub(crate) status_file: &'a Path,
    pub(crate) session_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) history_limit: usize,
    pub(crate) max_pending: usize,
    pub(crate) max_failed_ticks: usize,
    pub(crate) max_status_age_secs: u64,
    pub(crate) max_embedding_stale: usize,
    pub(crate) require_backup: bool,
    pub(crate) require_endpoint: bool,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
}

pub(crate) fn autopilot_report(
    conn: &Connection,
    request: AutopilotReportRequest<'_>,
) -> Result<AutopilotReport> {
    let current_status = read_daemon_status(request.status_file).ok();
    let doctor = autopilot_doctor(
        conn,
        request.status_file,
        request.session_dir,
        request.backup_dir,
        180,
        request.provider,
        request.endpoint,
    )?;
    let history = autopilot_history(conn, request.history_limit)?;
    let total_ticks = history
        .iter()
        .filter(|event| event.event_type == "daemon_tick")
        .count();
    let failed_ticks = history
        .iter()
        .filter(|event| event.event_type == "daemon_tick_failed")
        .count();
    let backups_created = history
        .iter()
        .filter(|event| {
            event
                .detail
                .get("backup_ran")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let inbox_added = history
        .iter()
        .map(|event| {
            event
                .detail
                .get("auto_inbox_added")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        })
        .sum();
    let current_pending = list_inbox(conn, "pending", usize::MAX)?.len();
    let embed = embeddings::embed_status(conn, request.provider, request.endpoint, request.model)?;
    let recommendations = doctor.recommendations.clone();
    Ok(AutopilotReport {
        ok: doctor.ok,
        generated_at: now_ms(),
        current_status,
        total_ticks,
        failed_ticks,
        backups_created,
        inbox_added,
        current_pending,
        embeddings_indexed: embed.indexed,
        embeddings_stale: embed.stale,
        embeddings_missing: embed.missing,
        latest_backup: doctor.latest_backup.clone(),
        recommendations,
        doctor,
        history,
    })
}

pub(crate) fn autopilot_alert(
    conn: &Connection,
    request: AutopilotAlertRequest<'_>,
) -> Result<AutopilotAlert> {
    let report = autopilot_report(
        conn,
        AutopilotReportRequest {
            status_file: request.status_file,
            session_dir: request.session_dir,
            backup_dir: request.backup_dir,
            history_limit: request.history_limit,
            provider: request.provider,
            endpoint: request.endpoint,
            model: request.model,
        },
    )?;
    let mut violations = Vec::new();
    let mut critical = false;

    match report.doctor.status_age_secs {
        Some(age) => {
            if age > request.max_status_age_secs as i64 {
                critical = true;
                violations.push(format!(
                    "status_age_exceeds_threshold:{age}>{}",
                    request.max_status_age_secs
                ));
            }
        }
        None => {
            critical = true;
            violations.push("status_missing".to_string());
        }
    }
    if !report.doctor.session_dir_ok {
        critical = true;
        violations.push("session_dir_missing".to_string());
    }
    if request.require_backup && !report.doctor.backup_ok {
        critical = true;
        violations.push("backup_missing_or_invalid".to_string());
    }
    if !report.doctor.lock_ok {
        critical = true;
        violations.push("daemon_lock_active_or_stale".to_string());
    }
    if request.require_endpoint && !report.doctor.endpoint_ok {
        critical = true;
        violations.push("endpoint_unreachable".to_string());
    }
    if report.current_pending > request.max_pending {
        violations.push(format!(
            "pending_inbox_exceeds_threshold:{}>{}",
            report.current_pending, request.max_pending
        ));
    }
    if report.failed_ticks > request.max_failed_ticks {
        critical = true;
        violations.push(format!(
            "failed_ticks_exceeds_threshold:{}>{}",
            report.failed_ticks, request.max_failed_ticks
        ));
    }
    if report.embeddings_stale > request.max_embedding_stale {
        violations.push(format!(
            "embedding_stale_exceeds_threshold:{}>{}",
            report.embeddings_stale, request.max_embedding_stale
        ));
    }

    let level = if violations.is_empty() {
        "ok"
    } else if critical {
        "critical"
    } else {
        "warn"
    };
    let mut recommendations = report.recommendations.clone();
    if !violations.is_empty() {
        recommendations.push(
            "run `dukememory autopilot report --json` for the full diagnostic snapshot".to_string(),
        );
    }
    Ok(AutopilotAlert {
        ok: violations.is_empty(),
        level: level.to_string(),
        generated_at: now_ms(),
        violations,
        recommendations,
        report,
    })
}

fn print_autopilot_report(report: &AutopilotReport, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "autopilot_report: {}",
            if report.ok { "ok" } else { "warn" }
        );
        println!("total_ticks: {}", report.total_ticks);
        println!("failed_ticks: {}", report.failed_ticks);
        println!("backups_created: {}", report.backups_created);
        println!("inbox_added: {}", report.inbox_added);
        println!("current_pending: {}", report.current_pending);
        println!("embeddings_indexed: {}", report.embeddings_indexed);
        println!("embeddings_stale: {}", report.embeddings_stale);
        println!("embeddings_missing: {}", report.embeddings_missing);
        if let Some(backup) = &report.latest_backup {
            println!("latest_backup: {backup}");
        }
        for recommendation in &report.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

fn print_autopilot_alert(
    alert: &AutopilotAlert,
    json_out: bool,
    write_alert: Option<&Path>,
) -> Result<()> {
    if let Some(path) = write_alert {
        write_file(path, serde_json::to_string_pretty(alert)?.as_bytes())?;
    }
    if json_out {
        println!("{}", serde_json::to_string_pretty(alert)?);
    } else {
        println!("autopilot_alert: {}", alert.level);
        for violation in &alert.violations {
            println!("violation: {violation}");
        }
        for recommendation in &alert.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

fn read_daemon_status(path: &Path) -> Result<DaemonStatus> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read daemon status {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("invalid daemon status {}", path.display()))
}

fn print_autopilot_status(path: &Path, status: &DaemonStatus, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(status)?);
    } else {
        println!("status_file: {}", path.display());
        println!("tick_ok: {}", status.tick_ok);
        println!("autopilot: {}", status.autopilot);
        println!("updated_at: {}", status.updated_at);
        println!("indexed: {}", status.indexed);
        println!("skipped: {}", status.skipped);
        println!("pending: {}", status.pending);
        println!("backup_ran: {}", status.backup_ran);
        println!("cleanup_ran: {}", status.cleanup_ran);
        if let Some(error) = &status.error {
            println!("error: {error}");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutonomousPolicyDecision {
    action: String,
    allowed: bool,
    level: String,
    risk_score: f64,
    usefulness_score: f64,
    token_saving_score: f64,
    confidence: f64,
    rollback: bool,
    reason: String,
}

struct AutonomousPolicyInput {
    action: &'static str,
    level: AutonomousLevel,
    risk_score: f64,
    usefulness_score: f64,
    token_saving_score: f64,
    confidence: f64,
    rollback: bool,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AutonomousReport {
    version: u32,
    pub(crate) ok: bool,
    level: String,
    updated_at: i64,
    rollback_backup: Option<String>,
    actions: Vec<AutonomousAction>,
    rollback: Vec<AutonomousRollback>,
    #[serde(default)]
    policy: Vec<AutonomousPolicyDecision>,
    #[serde(default)]
    quality: Option<QualityReport>,
    #[serde(default)]
    feedback: Option<FeedbackSummary>,
    #[serde(default)]
    budget: Option<BudgetPlan>,
    #[serde(default)]
    project_profile: Option<ProjectProfileSnapshot>,
    #[serde(default)]
    policy_tuning: Option<PolicyTuneReport>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AutonomousAction {
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) detail: String,
    pub(crate) memory_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum AutonomousRollback {
    RestoreMemoryStatus {
        id: String,
        status: String,
        superseded_by: Option<String>,
    },
    RejectAddedMemory {
        id: String,
    },
    RejectInboxItem {
        inbox_id: String,
    },
    RestoreInboxPending {
        inbox_id: String,
        memory_id: String,
    },
}

pub(crate) struct AutonomousRunRequest<'a> {
    pub(crate) level: AutonomousLevel,
    pub(crate) status_file: &'a Path,
    pub(crate) rollback_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) backup_keep: usize,
    pub(crate) db: &'a Path,
    pub(crate) scope: &'a str,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
}

pub(crate) fn handle_autonomous(
    conn: &Connection,
    db: &Path,
    command: AutonomousCommand,
) -> Result<()> {
    match command {
        AutonomousCommand::Status { status_file, json } => {
            let report = read_autonomous_status(&status_file)?;
            print_autonomous_report(&report, json)
        }
        AutonomousCommand::RunOnce {
            level,
            status_file,
            rollback_dir,
            backup_dir,
            backup_keep,
            scope,
            provider,
            endpoint,
            model,
            json,
        } => {
            let report = autonomous_run_once(
                conn,
                AutonomousRunRequest {
                    level,
                    status_file: &status_file,
                    rollback_dir: &rollback_dir,
                    backup_dir: &backup_dir,
                    backup_keep,
                    db,
                    scope: &scope,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            )?;
            print_autonomous_report(&report, json)
        }
        AutonomousCommand::Daemon {
            level,
            interval_secs,
            status_file,
            rollback_dir,
            backup_dir,
            backup_keep,
            scope,
            provider,
            endpoint,
            model,
        } => loop {
            let _ = autonomous_run_once(
                conn,
                AutonomousRunRequest {
                    level,
                    status_file: &status_file,
                    rollback_dir: &rollback_dir,
                    backup_dir: &backup_dir,
                    backup_keep,
                    db,
                    scope: &scope,
                    provider: &provider,
                    endpoint: &endpoint,
                    model: &model,
                },
            );
            std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
        },
        AutonomousCommand::Rollback { status_file, json } => {
            let report = read_autonomous_status(&status_file)?;
            let rollback = autonomous_rollback(conn, &report)?;
            print_autonomous_report(&rollback, json)
        }
        AutonomousCommand::Explain { status_file, json } => {
            let report = read_autonomous_status(&status_file)?;
            print_autonomous_explain(&report, json)
        }
        AutonomousCommand::Install {
            output,
            level,
            interval_secs,
            status_file,
            rollback_dir,
            backup_dir,
            provider,
            endpoint,
            model,
            force,
            dry_run,
        } => write_autonomous_launchd_plist(AutonomousLaunchdRequest {
            db,
            output: &output,
            level,
            interval_secs,
            status_file: &status_file,
            rollback_dir: &rollback_dir,
            backup_dir: &backup_dir,
            provider: &provider,
            endpoint: &endpoint,
            model: &model,
            force,
            dry_run,
        }),
    }
}

pub(crate) fn autonomous_run_once(
    conn: &Connection,
    request: AutonomousRunRequest<'_>,
) -> Result<AutonomousReport> {
    let mut report = AutonomousReport {
        version: 1,
        ok: true,
        level: request.level.to_string(),
        updated_at: now_ms(),
        rollback_backup: None,
        actions: Vec::new(),
        rollback: Vec::new(),
        policy: Vec::new(),
        quality: None,
        feedback: None,
        budget: None,
        project_profile: None,
        policy_tuning: None,
        error: None,
    };
    let run = (|| -> Result<()> {
        validate_scope(request.scope)?;
        let rollback_backup = autonomous_backup(request.db, request.rollback_dir)?;
        report.rollback_backup = Some(rollback_backup.display().to_string());
        report.actions.push(AutonomousAction {
            kind: "rollback_backup".to_string(),
            status: "ok".to_string(),
            detail: rollback_backup.display().to_string(),
            memory_id: None,
        });
        fs::create_dir_all(request.backup_dir)?;
        ops::run_backup_policy_quiet(request.db, request.backup_dir, request.backup_keep)?;
        report.actions.push(AutonomousAction {
            kind: "backup_policy".to_string(),
            status: "ok".to_string(),
            detail: request.backup_dir.display().to_string(),
            memory_id: None,
        });
        let embed = embeddings::embed_index(
            conn,
            request.provider,
            request.endpoint,
            request.model,
            &[],
            None,
            false,
        )?;
        report.actions.push(AutonomousAction {
            kind: "embed_index".to_string(),
            status: "ok".to_string(),
            detail: format!("indexed={} skipped={}", embed.indexed, embed.skipped),
            memory_id: None,
        });
        ops::run_cleanup_quiet(conn, 5000, 30)?;
        report.actions.push(AutonomousAction {
            kind: "cleanup".to_string(),
            status: "ok".to_string(),
            detail: "operational retention cleanup applied".to_string(),
            memory_id: None,
        });
        let usefulness = usefulness_report(conn, 30, 30, 3)?;
        report.actions.push(AutonomousAction {
            kind: "usefulness_scan".to_string(),
            status: "ok".to_string(),
            detail: format!(
                "hot={} unused={} stale={} suggestions={}",
                usefulness.hot.len(),
                usefulness.unused.len(),
                usefulness.stale.len(),
                usefulness.suggestions.len()
            ),
            memory_id: None,
        });
        report.quality = Some(quality_report(conn, 30, 20)?);
        report.feedback = Some(feedback_summary(conn, 30)?);
        report.budget = Some(budget_plan(
            conn,
            "autonomous memory maintenance",
            Some(request.scope),
        )?);
        report.project_profile = Some(project_profile_snapshot(
            conn,
            request
                .db
                .parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new(".")),
            request.scope,
        )?);
        let policy_output = request
            .db
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("autonomous-policy.json");
        let tuning = policy_tune_report(conn, &policy_output, true)?;
        let effective_level = autonomous_level_from_str(&tuning.level).unwrap_or(request.level);
        report.policy_tuning = Some(tuning.clone());
        report.actions.push(AutonomousAction {
            kind: "quality_score".to_string(),
            status: "ok".to_string(),
            detail: report
                .quality
                .as_ref()
                .map(|quality| {
                    format!(
                        "average={:.1} total={}",
                        quality.average_score, quality.total
                    )
                })
                .unwrap_or_else(|| "unavailable".to_string()),
            memory_id: None,
        });
        report.actions.push(AutonomousAction {
            kind: "policy_tune".to_string(),
            status: "ok".to_string(),
            detail: format!(
                "level={} risk_limit={:.0} approve_threshold={:.2}",
                tuning.level, tuning.risk_limit, tuning.approve_threshold
            ),
            memory_id: None,
        });
        autonomous_create_gap_inbox(conn, effective_level, request.scope, &mut report)?;
        if !matches!(request.level, AutonomousLevel::Conservative) {
            autonomous_approve_inbox(conn, effective_level, &mut report)?;
            autonomous_compact_operational(conn, effective_level, request.scope, &mut report)?;
            autonomous_supersede_duplicates(conn, effective_level, &mut report)?;
        }
        log_event(
            conn,
            "autonomous_tick",
            None,
            &serde_json::to_string(&json!({
                "level": report.level,
                "actions": report.actions.len(),
                "rollback": report.rollback.len(),
                "rollback_backup": report.rollback_backup,
            }))?,
        )?;
        Ok(())
    })();
    if let Err(err) = run {
        report.ok = false;
        report.error = Some(err.to_string());
        report.actions.push(AutonomousAction {
            kind: "error".to_string(),
            status: "warn".to_string(),
            detail: err.to_string(),
            memory_id: None,
        });
    }
    if let Ok(previous) = read_autonomous_status(request.status_file)
        && !previous.rollback.is_empty()
    {
        if report.rollback.is_empty() {
            report.rollback = previous.rollback;
            report.rollback_backup = previous.rollback_backup;
            report.actions.push(AutonomousAction {
                kind: "preserve_rollback".to_string(),
                status: "ok".to_string(),
                detail: "preserved last reversible autonomous change".to_string(),
                memory_id: None,
            });
        } else {
            let current_len = report.rollback.len();
            let mut seen = std::collections::BTreeSet::new();
            let mut merged = Vec::new();
            for action in previous.rollback.iter().chain(report.rollback.iter()) {
                let key = serde_json::to_string(action)?;
                if seen.insert(key) {
                    merged.push(action.clone());
                }
            }
            if merged.len() > current_len {
                let preserved = merged.len() - current_len;
                report.rollback = merged;
                report.actions.push(AutonomousAction {
                    kind: "merge_rollback".to_string(),
                    status: "ok".to_string(),
                    detail: format!("preserved {preserved} previous rollback item(s)"),
                    memory_id: None,
                });
            }
        }
    }
    write_autonomous_status(request.status_file, &report)?;
    Ok(report)
}

fn autonomous_backup(db: &Path, rollback_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(rollback_dir)?;
    let output = rollback_dir.join(format!("autonomous-{}.db", now_ms()));
    fs::copy(db, &output).with_context(|| {
        format!(
            "failed to create autonomous rollback backup {}",
            output.display()
        )
    })?;
    Ok(output)
}

fn autonomous_approve_inbox(
    conn: &Connection,
    level: AutonomousLevel,
    report: &mut AutonomousReport,
) -> Result<()> {
    let threshold = match level {
        AutonomousLevel::Conservative => 1.1,
        AutonomousLevel::Normal => 0.85,
        AutonomousLevel::Aggressive => 0.70,
    };
    let items = list_inbox(conn, "pending", 50)?;
    for item in items
        .into_iter()
        .filter(|item| item.confidence >= threshold)
    {
        let decision = autonomous_policy_decision(AutonomousPolicyInput {
            action: "approve_inbox",
            level,
            risk_score: 12.0,
            usefulness_score: 70.0,
            token_saving_score: 35.0,
            confidence: item.confidence,
            rollback: true,
            reason: format!(
                "confidence {:.2} >= threshold {:.2}",
                item.confidence, threshold
            ),
        });
        report.policy.push(decision.clone());
        if !decision.allowed {
            report.actions.push(AutonomousAction {
                kind: "approve_inbox".to_string(),
                status: "skipped".to_string(),
                detail: decision.reason,
                memory_id: None,
            });
            continue;
        }
        let inbox_id = item.id.clone();
        let memory_id = approve_inbox(conn, &inbox_id, false)?;
        report
            .rollback
            .push(AutonomousRollback::RestoreInboxPending {
                inbox_id: inbox_id.clone(),
                memory_id: memory_id.clone(),
            });
        report.actions.push(AutonomousAction {
            kind: "approve_inbox".to_string(),
            status: "ok".to_string(),
            detail: format!("approved high-confidence inbox item {inbox_id}"),
            memory_id: Some(memory_id),
        });
    }
    Ok(())
}

fn autonomous_create_gap_inbox(
    conn: &Connection,
    level: AutonomousLevel,
    scope: &str,
    report: &mut AutonomousReport,
) -> Result<()> {
    let live = live_eval_report(conn, 7)?;
    let max = match level {
        AutonomousLevel::Conservative => 1,
        AutonomousLevel::Normal => 3,
        AutonomousLevel::Aggressive => 8,
    };
    let gaps = live
        .inferred_missing_queries
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>();
    let decision = autonomous_policy_decision(AutonomousPolicyInput {
        action: "gap_inbox",
        level,
        risk_score: 10.0,
        usefulness_score: (gaps.len().min(8) as f64) * 10.0,
        token_saving_score: (gaps.len().min(8) as f64) * 4.0,
        confidence: if gaps.is_empty() { 0.0 } else { 0.7 },
        rollback: true,
        reason: format!("{} inferred gap query(s)", gaps.len()),
    });
    report.policy.push(decision.clone());
    if gaps.is_empty() {
        report.actions.push(AutonomousAction {
            kind: "gap_inbox".to_string(),
            status: "skipped".to_string(),
            detail: "no unresolved inferred memory gaps".to_string(),
            memory_id: None,
        });
        return Ok(());
    }
    if !decision.allowed {
        report.actions.push(AutonomousAction {
            kind: "gap_inbox".to_string(),
            status: "skipped".to_string(),
            detail: decision.reason,
            memory_id: None,
        });
        return Ok(());
    }
    let mut created = Vec::new();
    for query in gaps {
        if let Some(inbox_id) = insert_gap_inbox_suggestion(conn, scope, &query)? {
            report.rollback.push(AutonomousRollback::RejectInboxItem {
                inbox_id: inbox_id.clone(),
            });
            created.push(inbox_id);
        }
    }
    report.actions.push(AutonomousAction {
        kind: "gap_inbox".to_string(),
        status: if created.is_empty() { "skipped" } else { "ok" }.to_string(),
        detail: if created.is_empty() {
            "all inferred gap inbox suggestions already existed".to_string()
        } else {
            format!("created {} gap inbox suggestion(s)", created.len())
        },
        memory_id: None,
    });
    Ok(())
}

fn is_compacted_operational_memory(memory: &Memory) -> bool {
    memory.title.starts_with("Autonomous compacted ")
        || memory
            .body
            .starts_with("Autonomously compacted operational memory")
        || memory.source.as_deref() == Some("autonomous_compact")
}

fn compact_operational_candidates(rows: Vec<Memory>) -> Vec<Memory> {
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for row in rows {
        if is_compacted_operational_memory(&row) {
            continue;
        }
        let key = format!("{}:{}", row.memory_type, normalize_title(&row.title));
        if !seen.insert(key) {
            continue;
        }
        selected.push(row);
        if selected.len() >= 8 {
            break;
        }
    }
    selected
}

fn render_operational_compact_body(rows: &[Memory]) -> String {
    let mut body = String::from("Autonomously compacted operational memory:\n");
    for row in rows {
        let line = format!(
            "- {}: {} -- {}\n",
            row.memory_type,
            truncate_chars(&one_line_summary(&row.title), 90),
            truncate_chars(&one_line_summary(&row.body), 260)
        );
        if body.len() + line.len() > 1800 {
            break;
        }
        body.push_str(&line);
    }
    truncate_chars(&body, 1800)
}

fn autonomous_compact_operational(
    conn: &Connection,
    level: AutonomousLevel,
    scope: &str,
    report: &mut AutonomousReport,
) -> Result<()> {
    let raw_rows = query_memories(
        conn,
        None,
        &["task_state".to_string(), "note".to_string()],
        &["active".to_string(), "uncertain".to_string()],
        Some(scope),
        50,
    )?;
    let rows = compact_operational_candidates(raw_rows);
    let decision = autonomous_policy_decision(AutonomousPolicyInput {
        action: "compact_operational",
        level,
        risk_score: 18.0,
        usefulness_score: (rows.len().min(10) as f64) * 8.0,
        token_saving_score: (rows.len().min(10) as f64) * 6.0,
        confidence: if rows.len() >= 3 { 0.9 } else { 0.2 },
        rollback: true,
        reason: format!("{} operational card(s)", rows.len()),
    });
    report.policy.push(decision.clone());
    if rows.len() < 3 {
        report.actions.push(AutonomousAction {
            kind: "compact_operational".to_string(),
            status: "skipped".to_string(),
            detail: format!("{} operational card(s), need at least 3", rows.len()),
            memory_id: None,
        });
        return Ok(());
    }
    if !decision.allowed {
        report.actions.push(AutonomousAction {
            kind: "compact_operational".to_string(),
            status: "skipped".to_string(),
            detail: decision.reason,
            memory_id: None,
        });
        return Ok(());
    }
    let body = render_operational_compact_body(&rows);
    let id = add_memory(
        conn,
        AddMemory {
            id: None,
            memory_type: "task_state".to_string(),
            title: format!("Autonomous compacted {scope} operational memory"),
            body,
            scope: scope.to_string(),
            status: "active".to_string(),
            source: Some("autonomous_compact".to_string()),
            supersedes: None,
            confidence: 0.9,
            links: Vec::new(),
        },
    )?;
    report
        .rollback
        .push(AutonomousRollback::RejectAddedMemory { id: id.clone() });
    for row in &rows {
        report
            .rollback
            .push(AutonomousRollback::RestoreMemoryStatus {
                id: row.id.clone(),
                status: row.status.clone(),
                superseded_by: row.superseded_by.clone(),
            });
        conn.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
            params![id, now_ms(), row.id],
        )?;
    }
    log_event(
        conn,
        "autonomous_compact",
        Some(&id),
        &format!("compacted {} operational memories", rows.len()),
    )?;
    report.actions.push(AutonomousAction {
        kind: "compact_operational".to_string(),
        status: "ok".to_string(),
        detail: format!("compacted {} operational cards", rows.len()),
        memory_id: Some(id),
    });
    Ok(())
}

fn autonomous_supersede_duplicates(
    conn: &Connection,
    level: AutonomousLevel,
    report: &mut AutonomousReport,
) -> Result<()> {
    let max = match level {
        AutonomousLevel::Conservative => 0,
        AutonomousLevel::Normal => 3,
        AutonomousLevel::Aggressive => 10,
    };
    if max == 0 {
        return Ok(());
    }
    let mut changed = 0;
    for candidate in merge_candidates(conn, 50)? {
        if changed >= max {
            break;
        }
        let duplicate = get_memory(conn, &candidate.duplicate_id)?;
        if matches!(
            duplicate.memory_type.as_str(),
            "decision" | "constraint" | "user_preference" | "product_goal"
        ) {
            report
                .policy
                .push(autonomous_policy_decision(AutonomousPolicyInput {
                    action: "supersede_duplicate",
                    level,
                    risk_score: 85.0,
                    usefulness_score: 45.0,
                    token_saving_score: 25.0,
                    confidence: duplicate.confidence,
                    rollback: true,
                    reason: format!("protected type {}", duplicate.memory_type),
                }));
            continue;
        }
        let decision = autonomous_policy_decision(AutonomousPolicyInput {
            action: "supersede_duplicate",
            level,
            risk_score: 22.0,
            usefulness_score: 55.0,
            token_saving_score: 40.0,
            confidence: duplicate.confidence,
            rollback: true,
            reason: candidate.reason.clone(),
        });
        report.policy.push(decision.clone());
        if !decision.allowed {
            report.actions.push(AutonomousAction {
                kind: "supersede_duplicate".to_string(),
                status: "skipped".to_string(),
                detail: decision.reason,
                memory_id: Some(candidate.duplicate_id),
            });
            continue;
        }
        report
            .rollback
            .push(AutonomousRollback::RestoreMemoryStatus {
                id: duplicate.id.clone(),
                status: duplicate.status.clone(),
                superseded_by: duplicate.superseded_by.clone(),
            });
        conn.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
            params![candidate.primary_id, now_ms(), duplicate.id],
        )?;
        log_event(
            conn,
            "autonomous_supersede_duplicate",
            Some(&candidate.primary_id),
            &format!("superseded duplicate {}", candidate.duplicate_id),
        )?;
        report.actions.push(AutonomousAction {
            kind: "supersede_duplicate".to_string(),
            status: "ok".to_string(),
            detail: format!(
                "{} -> {} ({})",
                candidate.duplicate_id, candidate.primary_id, candidate.reason
            ),
            memory_id: Some(candidate.duplicate_id),
        });
        changed += 1;
    }
    if changed == 0 {
        report.actions.push(AutonomousAction {
            kind: "supersede_duplicate".to_string(),
            status: "skipped".to_string(),
            detail: "no safe duplicate candidate".to_string(),
            memory_id: None,
        });
    }
    Ok(())
}

fn autonomous_policy_decision(input: AutonomousPolicyInput) -> AutonomousPolicyDecision {
    let risk_limit = match input.level {
        AutonomousLevel::Conservative => 15.0,
        AutonomousLevel::Normal => 45.0,
        AutonomousLevel::Aggressive => 70.0,
    };
    let allowed = input.rollback
        && input.confidence >= 0.70
        && input.risk_score <= risk_limit
        && input.usefulness_score + input.token_saving_score > input.risk_score;
    AutonomousPolicyDecision {
        action: input.action.to_string(),
        allowed,
        level: input.level.to_string(),
        risk_score: input.risk_score,
        usefulness_score: input.usefulness_score,
        token_saving_score: input.token_saving_score,
        confidence: input.confidence,
        rollback: input.rollback,
        reason: input.reason,
    }
}

fn autonomous_level_from_str(value: &str) -> Option<AutonomousLevel> {
    match value {
        "conservative" => Some(AutonomousLevel::Conservative),
        "normal" => Some(AutonomousLevel::Normal),
        "aggressive" => Some(AutonomousLevel::Aggressive),
        _ => None,
    }
}

pub(crate) fn autonomous_rollback(
    conn: &Connection,
    report: &AutonomousReport,
) -> Result<AutonomousReport> {
    let mut out = AutonomousReport {
        version: 1,
        ok: true,
        level: "rollback".to_string(),
        updated_at: now_ms(),
        rollback_backup: report.rollback_backup.clone(),
        actions: Vec::new(),
        rollback: Vec::new(),
        policy: Vec::new(),
        quality: None,
        feedback: None,
        budget: None,
        project_profile: None,
        policy_tuning: None,
        error: None,
    };
    for action in report.rollback.iter().rev() {
        match action {
            AutonomousRollback::RestoreMemoryStatus {
                id,
                status,
                superseded_by,
            } => {
                conn.execute(
                    "UPDATE memories SET status = ?1, superseded_by = ?2, updated_at = ?3 WHERE id = ?4",
                    params![status, superseded_by, now_ms(), id],
                )?;
                out.actions.push(AutonomousAction {
                    kind: "rollback_restore_status".to_string(),
                    status: "ok".to_string(),
                    detail: status.clone(),
                    memory_id: Some(id.clone()),
                });
            }
            AutonomousRollback::RejectAddedMemory { id } => {
                conn.execute(
                    "UPDATE memories SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
                    params![now_ms(), id],
                )?;
                out.actions.push(AutonomousAction {
                    kind: "rollback_reject_added".to_string(),
                    status: "ok".to_string(),
                    detail: "marked autonomous-created card rejected".to_string(),
                    memory_id: Some(id.clone()),
                });
            }
            AutonomousRollback::RejectInboxItem { inbox_id } => {
                conn.execute(
                    "UPDATE memory_inbox SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
                    params![now_ms(), inbox_id],
                )?;
                out.actions.push(AutonomousAction {
                    kind: "rollback_reject_inbox".to_string(),
                    status: "ok".to_string(),
                    detail: format!("rejected autonomous-created inbox {inbox_id}"),
                    memory_id: None,
                });
            }
            AutonomousRollback::RestoreInboxPending {
                inbox_id,
                memory_id,
            } => {
                conn.execute(
                    "UPDATE memory_inbox SET status = 'pending', updated_at = ?1 WHERE id = ?2",
                    params![now_ms(), inbox_id],
                )?;
                conn.execute(
                    "UPDATE memories SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
                    params![now_ms(), memory_id],
                )?;
                out.actions.push(AutonomousAction {
                    kind: "rollback_restore_inbox".to_string(),
                    status: "ok".to_string(),
                    detail: format!("restored inbox {inbox_id} and rejected {memory_id}"),
                    memory_id: Some(memory_id.clone()),
                });
            }
        }
    }
    log_event(
        conn,
        "autonomous_rollback",
        None,
        &format!("rolled back {} action(s)", out.actions.len()),
    )?;
    Ok(out)
}

fn print_autonomous_report(report: &AutonomousReport, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("autonomous: {}", if report.ok { "ok" } else { "warn" });
    println!("level: {}", report.level);
    if let Some(backup) = &report.rollback_backup {
        println!("rollback_backup: {backup}");
    }
    for action in &report.actions {
        let id = action.memory_id.as_deref().unwrap_or("-");
        println!(
            "{}  {}  {}  {}",
            action.status, action.kind, id, action.detail
        );
    }
    if let Some(error) = &report.error {
        println!("error: {error}");
    }
    Ok(())
}

fn print_autonomous_explain(report: &AutonomousReport, json_out: bool) -> Result<()> {
    let summary = json!({
        "ok": report.ok,
        "level": report.level,
        "updated_at": report.updated_at,
        "actions": report.actions,
        "allowed_policy": report.policy.iter().filter(|item| item.allowed).count(),
        "skipped_policy": report.policy.iter().filter(|item| !item.allowed).count(),
        "quality_average": report.quality.as_ref().map(|quality| quality.average_score),
        "rollback_available": !report.rollback.is_empty(),
        "rollback_backup": report.rollback_backup,
    });
    if json_out {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    println!("Autonomous Explain");
    println!("status: {}", if report.ok { "ok" } else { "warn" });
    println!("level: {}", report.level);
    if let Some(quality) = &report.quality {
        println!(
            "quality: average={:.1} total={}",
            quality.average_score, quality.total
        );
    }
    println!(
        "policy: allowed={} skipped={}",
        report.policy.iter().filter(|item| item.allowed).count(),
        report.policy.iter().filter(|item| !item.allowed).count()
    );
    for action in &report.actions {
        println!("- {} {} {}", action.status, action.kind, action.detail);
    }
    if !report.policy.is_empty() {
        println!("Policy Decisions:");
        for item in &report.policy {
            println!(
                "- {} {} risk={:.0} useful={:.0} token={:.0} confidence={:.2} {}",
                if item.allowed { "allow" } else { "skip" },
                item.action,
                item.risk_score,
                item.usefulness_score,
                item.token_saving_score,
                item.confidence,
                item.reason
            );
        }
    }
    Ok(())
}

pub(crate) fn read_autonomous_status(path: &Path) -> Result<AutonomousReport> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read autonomous status {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("invalid autonomous status {}", path.display()))
}

pub(crate) fn write_autonomous_status(path: &Path, report: &AutonomousReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_file(path, serde_json::to_string_pretty(report)?.as_bytes())
}

pub(crate) struct AutonomousLaunchdRequest<'a> {
    pub(crate) db: &'a Path,
    pub(crate) output: &'a Path,
    pub(crate) level: AutonomousLevel,
    pub(crate) interval_secs: u64,
    pub(crate) status_file: &'a Path,
    pub(crate) rollback_dir: &'a Path,
    pub(crate) backup_dir: &'a Path,
    pub(crate) provider: &'a str,
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
}

pub(crate) fn write_autonomous_launchd_plist(request: AutonomousLaunchdRequest<'_>) -> Result<()> {
    let output = expand_tilde(&request.output.display().to_string());
    if output.exists() && !request.force && !request.dry_run {
        bail!(
            "{} already exists (use --force to overwrite)",
            output.display()
        );
    }
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let plist = autonomous_launchd_plist(&exe, &request);
    if request.dry_run {
        println!("{plist}");
        return Ok(());
    }
    write_file(&output, plist.as_bytes())?;
    println!("{}", output.display());
    Ok(())
}

fn autonomous_launchd_plist(exe: &Path, request: &AutonomousLaunchdRequest<'_>) -> String {
    let working_dir = request
        .db
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let log_dir = request.db.parent().unwrap_or_else(|| Path::new("."));
    let stdout_log = log_dir.join("dukememory-autonomous.out.log");
    let stderr_log = log_dir.join("dukememory-autonomous.err.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.dukememory.autonomous</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--db</string>
    <string>{}</string>
    <string>autonomous</string>
    <string>daemon</string>
    <string>--level</string>
    <string>{}</string>
    <string>--interval-secs</string>
    <string>{}</string>
    <string>--status-file</string>
    <string>{}</string>
    <string>--rollback-dir</string>
    <string>{}</string>
    <string>--backup-dir</string>
    <string>{}</string>
    <string>--provider</string>
    <string>{}</string>
    <string>--endpoint</string>
    <string>{}</string>
    <string>--model</string>
    <string>{}</string>
  </array>
  <key>WorkingDirectory</key>
  <string>{}</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
        "#,
        xml_escape_local(&exe.display().to_string()),
        xml_escape_local(&request.db.display().to_string()),
        request.level,
        request.interval_secs.max(1),
        xml_escape_local(&request.status_file.display().to_string()),
        xml_escape_local(&request.rollback_dir.display().to_string()),
        xml_escape_local(&request.backup_dir.display().to_string()),
        xml_escape_local(request.provider),
        xml_escape_local(request.endpoint),
        xml_escape_local(request.model),
        xml_escape_local(&working_dir.display().to_string()),
        xml_escape_local(&stdout_log.display().to_string()),
        xml_escape_local(&stderr_log.display().to_string()),
    )
}

fn xml_escape_local(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn autopilot_doctor(
    conn: &Connection,
    status_file: &Path,
    session_dir: &Path,
    backup_dir: &Path,
    max_status_age_secs: u64,
    provider: &str,
    endpoint: &str,
) -> Result<AutopilotDoctorReport> {
    let mut recommendations = Vec::new();
    let status = read_daemon_status(status_file).ok();
    let status_age_secs = status
        .as_ref()
        .map(|status| ((now_ms() - status.updated_at).max(0)) / 1000);
    let status_fresh = status_age_secs
        .map(|age| age <= max_status_age_secs as i64)
        .unwrap_or(false);
    if !status_fresh {
        recommendations
            .push("run `dukememory autopilot run-once` or load the launchd daemon".to_string());
    }
    let session_dir_ok = session_dir.is_dir();
    if !session_dir_ok {
        recommendations.push(format!("create session dir: {}", session_dir.display()));
    }
    let latest_backup = latest_backup_path(backup_dir)?;
    let backup_ok = latest_backup
        .as_ref()
        .map(|path| ops::ensure_backup_verified(path, true).is_ok())
        .unwrap_or(false);
    if !backup_ok {
        recommendations.push(
            "run `dukememory autopilot run-once` to create a strict verified backup".to_string(),
        );
    }
    let lock_ok = daemon_lock_ok(conn)?;
    if !lock_ok {
        recommendations.push(
            "run `dukememory lock status`; clear stale daemon lock only if no daemon is running"
                .to_string(),
        );
    }
    let endpoint_ok = autopilot_endpoint_ok(provider, endpoint);
    if !endpoint_ok {
        recommendations.push(format!(
            "check embedding endpoint/provider: provider={provider} endpoint={endpoint}"
        ));
    }
    let ok = status_fresh && session_dir_ok && backup_ok && lock_ok && endpoint_ok;
    Ok(AutopilotDoctorReport {
        ok,
        status_file: status_file.display().to_string(),
        status_fresh,
        status_age_secs,
        session_dir_ok,
        backup_ok,
        latest_backup: latest_backup.map(|path| path.display().to_string()),
        lock_ok,
        endpoint_ok,
        recommendations,
    })
}

fn print_autopilot_doctor(report: AutopilotDoctorReport, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("autopilot: {}", if report.ok { "ok" } else { "warn" });
        println!("status_fresh: {}", report.status_fresh);
        println!("session_dir_ok: {}", report.session_dir_ok);
        println!("backup_ok: {}", report.backup_ok);
        println!("lock_ok: {}", report.lock_ok);
        println!("endpoint_ok: {}", report.endpoint_ok);
        for recommendation in &report.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

fn latest_backup_path(dir: &Path) -> Result<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with("dukememory-") && name.ends_with(".db") {
            backups.push(path);
        }
    }
    backups.sort();
    Ok(backups.pop())
}

fn daemon_lock_ok(conn: &Connection) -> Result<bool> {
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_locks WHERE name = 'daemon' AND expires_at > ?1",
        params![now_ms()],
        |row| row.get(0),
    )?;
    Ok(active == 0)
}

fn clear_expired_daemon_locks(conn: &Connection) -> Result<usize> {
    conn.execute(
        "DELETE FROM memory_locks WHERE name = 'daemon' AND expires_at <= ?1",
        params![now_ms()],
    )
    .map_err(Into::into)
}

fn autopilot_endpoint_ok(provider: &str, endpoint: &str) -> bool {
    let provider = provider.trim().to_lowercase();
    let endpoint = endpoint.trim();
    if provider == "mock" || endpoint.is_empty() || endpoint == "local" {
        return true;
    }
    let url = if provider == "ollama" {
        format!("{}/api/tags", endpoint.trim_end_matches('/'))
    } else {
        format!("{}/v1/models", endpoint.trim_end_matches('/'))
    };
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .and_then(|client| client.get(url).send())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

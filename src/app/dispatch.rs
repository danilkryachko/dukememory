use super::*;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let runtime = load_runtime_config(
        cli.config.as_deref(),
        &cli.db,
        DEFAULT_CONFIG,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )?;

    match cli.command {
        Command::Restore {
            input,
            force,
            dry_run,
            strict,
            rollback_dir,
            journal_dir,
            no_rollback,
        } => {
            restore_db(RestoreDbRequest {
                db: &cli.db,
                input: &input,
                force,
                dry_run,
                strict,
                rollback_dir: &rollback_dir,
                journal_dir: &journal_dir,
                rollback: !no_rollback,
            })?;
            return Ok(());
        }
        Command::BackupVerify {
            input,
            strict,
            json,
        } => {
            ops::print_backup_verify(&input, strict, json)?;
            return Ok(());
        }
        Command::Backup { output } => {
            let conn = open_db(&cli.db)?;
            sqlite_backup_to(&conn, &output)?;
            println!("{}", output.display());
            return Ok(());
        }
        _ => {}
    }

    let conn = open_db(&cli.db)?;

    match cli.command {
        Command::Init { config, force } => init_project(&conn, &cli.db, &config, force)?,
        Command::Add {
            memory_type,
            title,
            body,
            id,
            scope,
            status,
            source,
            supersedes,
            confidence,
            layer,
            links,
            allow_sensitive,
        } => {
            validate_scope(&scope)?;
            reject_sensitive(&title, &body, allow_sensitive)?;
            let id = add_memory(
                &conn,
                AddMemory {
                    id,
                    memory_type: memory_type.to_string(),
                    title,
                    body,
                    scope,
                    status: status.to_string(),
                    source,
                    supersedes,
                    confidence,
                    layer,
                    links,
                },
            )?;
            println!("{id}");
        }
        Command::Get { id, json } => {
            let memory = get_memory_with_links(&conn, &id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&memory)?);
            } else {
                println!("{}", format_card(&memory));
            }
        }
        Command::Update {
            id,
            memory_type,
            title,
            body,
            scope,
            status,
            source,
            confidence,
            layer,
            links,
            replace_links,
            allow_sensitive,
        } => {
            if let Some(scope) = &scope {
                validate_scope(scope)?;
            }
            if let Some(body) = &body {
                reject_sensitive(title.as_deref().unwrap_or_default(), body, allow_sensitive)?;
            }
            update_memory(
                &conn,
                UpdateMemory {
                    id,
                    memory_type: memory_type.map(|v| v.to_string()),
                    title,
                    body,
                    scope,
                    status: status.map(|v| v.to_string()),
                    source,
                    confidence,
                    layer,
                    links,
                    replace_links,
                },
            )?;
        }
        Command::Delete { id } => delete_memory(&conn, &id)?,
        Command::Search {
            query,
            memory_type,
            status,
            scope,
            limit,
            provider,
            endpoint,
            model,
            json,
        } => {
            let started = Instant::now();
            let types = split_csv(memory_type.as_deref());
            let statuses = split_csv(Some(&status));
            let (rows, semantic_used) = search_rows_with_semantic_fallback(
                &conn,
                SearchRowsRequest {
                    query: &query,
                    types: &types,
                    statuses: &statuses,
                    scope: scope.as_deref(),
                    limit,
                    budget: 1_200,
                    provider: select_cli_or_config(
                        &provider,
                        DEFAULT_EMBED_PROVIDER,
                        &runtime.config.embeddings.provider,
                    ),
                    endpoint: select_cli_or_config(
                        &endpoint,
                        DEFAULT_EMBED_ENDPOINT,
                        &runtime.config.embeddings.endpoint,
                    ),
                    model: select_cli_or_config(
                        &model,
                        DEFAULT_EMBED_MODEL,
                        &runtime.config.embeddings.model,
                    ),
                },
            )?;
            let quality_signals = retrieval_feedback_signals(&conn, 30).unwrap_or_default();
            let mut rows = filter_query_useless_memories(rows, &query, &quality_signals);
            rows.truncate(limit);
            let ids = rows
                .iter()
                .map(|memory| memory.id.clone())
                .collect::<Vec<_>>();
            log_read_event(
                &conn,
                ReadEventInput {
                    command: "search",
                    query: &query,
                    ids: &ids,
                    semantic_used,
                    result_count: ids.len(),
                    budget: 1_200,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            )?;
            print_rows(&conn, &rows, json)?;
        }
        Command::List {
            memory_type,
            status,
            scope,
            limit,
            json,
        } => {
            let rows = query_memories(
                &conn,
                None,
                &split_csv(memory_type.as_deref()),
                &split_csv(status.as_deref()),
                scope.as_deref(),
                limit,
            )?;
            print_rows(&conn, &rows, json)?;
        }
        Command::Status { id, status } => set_status(&conn, &id, status.to_string())?,
        Command::ContextPack {
            task,
            memory_type,
            status,
            scope,
            limit,
            max_chars,
            budget_profile,
            include_recent,
            json,
            with_codegraph,
            rules,
            semantic: _,
            embed_provider,
            embed_endpoint,
            embed_model,
        } => {
            let started = Instant::now();
            let types = split_csv(memory_type.as_deref());
            let statuses = split_csv(Some(&status));
            let max_chars = budget_profile_chars(budget_profile).unwrap_or(max_chars);
            let effective_limit = context_effective_limit(limit, max_chars);
            let mut rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &types,
                    statuses: &statuses,
                    scope: scope.as_deref(),
                    limit: effective_limit,
                    include_recent,
                    rules: rules.as_deref(),
                },
            )?;
            let semantic_used = append_semantic_context_rows(
                &conn,
                &mut rows,
                SemanticContextRequest {
                    task: &task,
                    limit: effective_limit,
                    budget: max_chars,
                    provider: &embed_provider,
                    endpoint: &embed_endpoint,
                    model: &embed_model,
                    rules: rules.as_deref(),
                },
            )?;
            let (json_rendered, ids) = if json {
                let (rendered, used_ids) =
                    render_compact_context_rows_json(&conn, &rows, &task, max_chars)?;
                (Some(rendered), used_ids)
            } else {
                (
                    None,
                    rows.iter()
                        .map(|memory| memory.id.clone())
                        .collect::<Vec<_>>(),
                )
            };
            log_read_event(
                &conn,
                ReadEventInput {
                    command: "context-pack",
                    query: &task,
                    ids: &ids,
                    semantic_used,
                    result_count: ids.len(),
                    budget: max_chars,
                    elapsed_ms: started.elapsed().as_millis(),
                },
            )?;
            if json {
                println!("{}", json_rendered.unwrap_or_else(|| "[]".to_string()));
            } else {
                let mut rendered = render_context_pack_for_task(&conn, &rows, max_chars, &task)?;
                if with_codegraph {
                    rendered.push_str(&render_codegraph_hints(&rows, &task, Path::new(".")));
                }
                println!("{rendered}");
            }
        }
        Command::Context {
            task,
            mode,
            limit,
            max_chars,
            json,
            embed_provider,
            embed_endpoint,
            embed_model,
            budget,
            budget_profile,
            format,
            rules,
        } => print_agent_context(
            &conn,
            AgentContextRequest {
                task: &task,
                mode,
                limit,
                max_chars: budget
                    .or_else(|| budget_profile_chars(budget_profile))
                    .unwrap_or(max_chars),
                json_out: json,
                provider: select_cli_or_config(
                    &embed_provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &embed_endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &embed_model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                format,
                rules: rules.as_deref(),
            },
        )?,
        Command::Brief {
            task,
            limit,
            budget,
            budget_profile,
            scope,
            rules,
            provider,
            endpoint,
            model,
            json,
        } => print_brief(
            &conn,
            BriefRequest {
                task: &task,
                limit,
                budget: budget
                    .unwrap_or_else(|| budget_profile_chars(Some(budget_profile)).unwrap_or(1200)),
                scope: scope.as_deref(),
                rules: rules.as_deref(),
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                json_out: json,
                audit_read: true,
            },
        )?,
        Command::Impact {
            target,
            limit,
            budget,
            budget_profile,
            scope,
            provider,
            endpoint,
            model,
            json,
        } => print_impact(
            &conn,
            ImpactRequest {
                target: &target,
                limit,
                budget: budget
                    .unwrap_or_else(|| budget_profile_chars(Some(budget_profile)).unwrap_or(1200)),
                scope: scope.as_deref(),
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                json_out: json,
                audit_read: true,
            },
        )?,
        Command::Export {
            output,
            memory_type,
            status,
            scope,
            redact,
        } => {
            let mut export = export_memories(
                &conn,
                &split_csv(memory_type.as_deref()),
                &split_csv(status.as_deref()),
                scope.as_deref(),
            )?;
            if redact {
                redact_export(&mut export)?;
            }
            let json = serde_json::to_string_pretty(&export)?;
            if let Some(output) = output {
                write_file(&output, json.as_bytes())?;
                println!("{}", output.display());
            } else {
                println!("{json}");
            }
        }
        Command::Import { input, replace } => import_memories(&conn, &input, replace)?,
        Command::Stats => print_stats(&conn, &cli.db)?,
        Command::Review { stale_days, json } => print_review(&conn, stale_days, json)?,
        Command::Stale { days, json } => print_stale(&conn, days, json)?,
        Command::Conflicts { json } => print_conflicts(&conn, json)?,
        Command::Links {
            id,
            root,
            validate_symbols,
            json,
        } => print_link_report(&conn, id.as_deref(), &root, validate_symbols, json)?,
        Command::SessionClose {
            title,
            summary,
            next,
            scope,
            source,
            allow_sensitive,
        } => {
            validate_scope(&scope)?;
            let body = render_session_body(&summary, &next);
            reject_sensitive(&title, &body, allow_sensitive)?;
            let id = add_memory(
                &conn,
                AddMemory {
                    id: None,
                    memory_type: "task_state".to_string(),
                    title,
                    body,
                    scope,
                    status: "active".to_string(),
                    source,
                    supersedes: None,
                    confidence: 1.0,
                    layer: None,
                    links: Vec::new(),
                },
            )?;
            println!("{id}");
        }
        Command::AgentSession { command } => handle_agent_session(
            &conn,
            command,
            &runner_profile_root(&cli.db),
            &runtime.config.embeddings.provider,
            &runtime.config.embeddings.endpoint,
            &runtime.config.embeddings.model,
            &runtime.config.agent_sessions,
        )?,
        Command::RunnerProfile { command } => handle_runner_profile(command)?,
        Command::Install { to, force } => install_binary(&to, force)?,
        Command::InstallSkill { path, force } => install_codex_skill(&expand_tilde(&path), force)?,
        Command::UpdateInstall {
            from,
            to,
            backup_dir,
            backup_keep,
            dry_run,
            json,
        } => print_update_install(
            from.as_deref(),
            &to,
            &backup_dir,
            backup_keep,
            dry_run,
            json,
        )?,
        Command::VecStatus => print_vec_status(&conn),
        Command::VecIndex { rebuild, json } => print_vec_index(&conn, rebuild, json)?,
        Command::ServeMcp { content_length } => mcp_server::serve_mcp(&cli.db, content_length)?,
        Command::ProjectSummary { max_chars, json } => {
            print_project_summary(&conn, max_chars, json)?
        }
        Command::Decisions { scope, json } => {
            let rows = query_memories(
                &conn,
                None,
                &["decision".to_string()],
                &["active".to_string()],
                scope.as_deref(),
                usize::MAX,
            )?;
            print_rows(&conn, &rows, json)?;
        }
        Command::OpenQuestions { json } => print_open_questions(&conn, json)?,
        Command::NextActions { limit, json } => print_next_actions(&conn, limit, json)?,
        Command::Lifecycle {
            stale_days,
            dry_run,
            rules,
        } => apply_lifecycle(&conn, stale_days, dry_run, rules.as_deref())?,
        Command::ScanSecrets { fix_redact, json } => print_secret_scan(&conn, fix_redact, json)?,
        Command::Suggest {
            input,
            to_inbox,
            scope,
            json,
        } => suggest_from_file(&conn, &input, &scope, to_inbox, json)?,
        Command::IngestTranscript {
            input,
            scope,
            llm,
            endpoint,
            model,
        } => {
            let count = ingest_transcript(&conn, &input, &scope, llm, &endpoint, &model)?;
            println!("inbox_added: {count}");
        }
        Command::AutoIngest {
            input,
            scope,
            llm,
            endpoint,
            model,
            dry_run,
            json,
        } => print_auto_ingest(
            &conn,
            AutoIngestPrintRequest {
                input: &input,
                scope: &scope,
                llm,
                endpoint: &endpoint,
                model: &model,
                dry_run,
                json,
            },
        )?,
        Command::InboxList {
            status,
            limit,
            json,
        } => print_inbox(&conn, &status, limit, json)?,
        Command::InboxApprove {
            id,
            allow_sensitive,
        } => {
            let memory_id = approve_inbox(&conn, &id, allow_sensitive)?;
            println!("{memory_id}");
        }
        Command::InboxReject { id } => reject_inbox(&conn, &id)?,
        Command::ReviewTui { stale_days } => print_review_tui(&conn, stale_days)?,
        Command::Remember {
            text,
            memory_type,
            scope,
            allow_sensitive,
        } => remember_text(&conn, &text, memory_type, &scope, allow_sensitive)?,
        Command::WhatDoWeKnow { query, limit, json } => {
            let rows = query_memories(
                &conn,
                Some(&query),
                &[],
                &["active".to_string(), "uncertain".to_string()],
                None,
                limit,
            )?;
            print_rows(&conn, &rows, json)?;
        }
        Command::WhatNext { limit, json } => print_next_actions(&conn, limit, json)?,
        Command::Forget { query, dry_run } => forget_matching(&conn, &query, dry_run)?,
        Command::Doctor {
            root,
            fix_redact,
            json,
            self_check,
        } => print_doctor(&conn, &root, fix_redact, json, self_check)?,
        Command::Snapshot {
            max_chars,
            with_codegraph,
            json,
        } => print_snapshot(&conn, max_chars, with_codegraph, json)?,
        Command::Compact {
            scope,
            limit,
            dry_run,
        } => compact_task_state(&conn, &scope, limit, dry_run)?,
        Command::CompactV2 {
            scope,
            limit,
            dry_run,
        } => compact_v2(&conn, &scope, limit, dry_run)?,
        Command::RhaiCheck { rules } => check_rhai_rules(&rules)?,
        Command::PolicyCheck { rules } => check_policy_rules(&rules)?,
        Command::PolicyApply { rules, dry_run } => apply_policy_rules(&conn, &rules, dry_run)?,
        Command::EmbedIndex {
            provider,
            endpoint,
            model,
            status,
            limit,
            force,
        } => {
            let report = embeddings::embed_index(
                &conn,
                &provider,
                &endpoint,
                &model,
                &split_csv(status.as_deref()),
                limit,
                force,
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::EmbedSearch {
            query,
            provider,
            endpoint,
            model,
            limit,
            json,
            backend,
        } => {
            ensure_vector_backend(&conn, backend)?;
            let rows = embeddings::semantic_search_with_backend(
                &conn, &provider, &endpoint, &model, &query, limit, backend,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for row in rows {
                    println!(
                        "{:.4}  {}  {}",
                        row.score, row.memory.memory.id, row.memory.memory.title
                    );
                }
            }
        }
        Command::ProviderList {
            provider,
            endpoint,
            json,
        } => embeddings::print_provider_models(&provider, &endpoint, json)?,
        Command::VectorBench {
            provider,
            endpoint,
            model,
            iterations,
            warmup,
            limit,
            baseline,
            write_baseline,
            max_regression_percent,
            json,
        } => embeddings::print_vector_bench(
            &conn,
            embeddings::VectorBenchOptions {
                provider: &provider,
                endpoint: &endpoint,
                model: &model,
                iterations,
                warmup,
                limit,
                baseline: baseline.as_deref(),
                write_baseline,
                max_regression_percent,
                json_out: json,
            },
        )?,
        Command::EmbedStatus {
            provider,
            endpoint,
            model,
            json,
        } => embeddings::print_embed_status(&conn, &provider, &endpoint, &model, json)?,
        Command::EmbedWatch {
            provider,
            endpoint,
            model,
            interval_secs,
            once,
        } => embeddings::embed_watch(&conn, &provider, &endpoint, &model, interval_secs, once)?,
        Command::Completions { shell } => print_completions(shell),
        Command::Man => print_manpage(),
        Command::Audit { limit, json } => print_audit(&conn, limit, json)?,
        Command::UsageReport {
            since_days,
            limit,
            json,
        } => print_usage_report(&conn, since_days, limit, json)?,
        Command::UsefulnessReport {
            since_days,
            stale_days,
            hot_threshold,
            json,
        } => print_usefulness_report(&conn, since_days, stale_days, hot_threshold, json)?,
        Command::QualityReport {
            since_days,
            limit,
            json,
        } => print_quality_report(&conn, since_days, limit, json)?,
        Command::RoiReport { since_days, json } => print_roi_report(&conn, since_days, json)?,
        Command::AgentAudit { since_days, json } => print_agent_audit(&conn, since_days, json)?,
        Command::DecisionTrace {
            since_days,
            limit,
            json,
        } => print_decision_trace(&conn, since_days, limit, json)?,
        Command::AutoFeedback {
            since_days,
            limit,
            dry_run,
            json,
        } => print_auto_feedback_v2(&conn, since_days, limit, dry_run, json)?,
        Command::CostGuard { since_days, json } => print_cost_guard(&conn, since_days, json)?,
        Command::Feedback {
            ids,
            rating,
            command,
            query,
            note,
            json,
        } => print_feedback_report(&conn, &ids, rating, &command, &query, &note, json)?,
        Command::BudgetPlan { task, scope, json } => {
            print_budget_plan(&conn, &task, scope.as_deref(), json)?
        }
        Command::ProjectProfile { root, json } => print_project_profile(&conn, &root, json)?,
        Command::Recall {
            query,
            max_chars,
            limit,
            scope,
            provider,
            endpoint,
            model,
            recent,
            as_of,
            as_of_days_ago,
            changed_since,
            changed_since_days,
            json,
        } => print_recall(
            &conn,
            RecallRequest {
                query: &query,
                max_chars,
                limit,
                scope: scope.as_deref(),
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                recent,
                as_of: as_of.as_deref(),
                as_of_days_ago,
                changed_since: changed_since.as_deref(),
                changed_since_days,
                json_out: json,
            },
        )?,
        Command::Onboard {
            root,
            install_autonomous,
            provider,
            endpoint,
            model,
            json,
        } => print_onboard(
            &root,
            install_autonomous,
            select_cli_or_config(
                &provider,
                DEFAULT_EMBED_PROVIDER,
                &runtime.config.embeddings.provider,
            ),
            select_cli_or_config(
                &endpoint,
                DEFAULT_EMBED_ENDPOINT,
                &runtime.config.embeddings.endpoint,
            ),
            select_cli_or_config(
                &model,
                DEFAULT_EMBED_MODEL,
                &runtime.config.embeddings.model,
            ),
            json,
        )?,
        Command::Dashboard { json } => print_dashboard(&cli.db, json)?,
        Command::DashboardRepair {
            apply,
            project,
            provider,
            endpoint,
            model,
            json,
        } => print_dashboard_repair(
            &cli.db,
            apply,
            project.as_deref(),
            select_cli_or_config(
                &provider,
                DEFAULT_EMBED_PROVIDER,
                &runtime.config.embeddings.provider,
            ),
            select_cli_or_config(
                &endpoint,
                DEFAULT_EMBED_ENDPOINT,
                &runtime.config.embeddings.endpoint,
            ),
            select_cli_or_config(
                &model,
                DEFAULT_EMBED_MODEL,
                &runtime.config.embeddings.model,
            ),
            json,
        )?,
        Command::DashboardRepairHistory {
            since_days,
            limit,
            project,
            json,
        } => print_dashboard_repair_history(&cli.db, since_days, limit, project.as_deref(), json)?,
        Command::OpsStatus {
            root,
            since_days,
            json,
        } => print_ops_status(&conn, &cli.db, &root, since_days, json)?,
        Command::RemoteStatus {
            root,
            since_days,
            json,
        } => print_remote_status(&conn, &cli.db, &root, since_days, json)?,
        Command::ProjectDiff {
            root,
            changed_only,
            json,
        } => print_project_diff(&conn, &root, changed_only, json)?,
        Command::IntelligenceDashboard {
            root,
            since_days,
            json,
        } => print_intelligence_dashboard(&conn, &cli.db, &root, since_days, json)?,
        Command::RemoteSyncDryRun {
            root,
            since_days,
            json,
        } => print_remote_sync_dry_run(&conn, &cli.db, &root, since_days, json)?,
        Command::DoctorProject {
            root,
            since_days,
            fix,
            json,
        } => print_project_doctor(&conn, &cli.db, &root, since_days, fix, json)?,
        Command::ReleaseGate {
            root,
            since_days,
            strict,
            run,
            json,
        } => print_release_gate(&conn, &cli.db, &root, since_days, strict, run, json)?,
        Command::MemoryReplay {
            since_days,
            limit,
            json,
        } => print_memory_replay(&conn, since_days, limit, json)?,
        Command::ProjectWatch {
            since_days,
            fix,
            json,
        } => print_project_watch(&cli.db, since_days, fix, json)?,
        Command::AutonomousLoop {
            root,
            since_days,
            level,
            apply,
            watch,
            interval_secs,
            max_runs,
            json,
        } => print_autonomous_loop(
            &conn,
            &cli.db,
            &root,
            since_days,
            level,
            apply,
            watch,
            interval_secs,
            max_runs,
            json,
        )?,
        Command::AutonomousWatchInstall {
            root,
            interval_secs,
            label,
            dry_run,
            json,
        } => print_autonomous_watch_install(&cli.db, &root, interval_secs, &label, dry_run, json)?,
        Command::ActionJournal {
            since_days,
            limit,
            json,
        } => print_action_journal(&conn, since_days, limit, json)?,
        Command::UsefulnessEngine {
            root,
            since_days,
            apply,
            json,
        } => print_usefulness_engine(&conn, &root, since_days, apply, json)?,
        Command::RankingProfile {
            root,
            profile,
            apply,
            json,
        } => print_ranking_profile(&root, profile, apply, json)?,
        Command::ContextGovernor {
            task,
            root,
            target,
            json,
        } => print_context_governor(&conn, &root, &task, target.as_deref(), json)?,
        Command::MemoryRouter {
            query,
            root,
            include_siblings,
            json,
        } => print_memory_router(&cli.db, &root, &query, include_siblings, json)?,
        Command::AutoRankingTune {
            root,
            since_days,
            apply,
            json,
        } => print_auto_ranking_tune(&conn, &root, since_days, apply, json)?,
        Command::MemoryHealthScore {
            root,
            since_days,
            json,
        } => print_memory_health_score(&conn, &cli.db, &root, since_days, json)?,
        Command::ExplainRecall {
            query,
            root,
            limit,
            json,
        } => print_explain_recall(&conn, &root, &query, limit, json)?,
        Command::ProjectIntentMap { root, json } => print_project_intent_map(&conn, &root, json)?,
        Command::MemoryTestHarness {
            root,
            since_days,
            limit,
            json,
        } => print_memory_test_harness(&conn, &root, since_days, limit, json)?,
        Command::AgentAuditV2 {
            root,
            since_days,
            json,
        } => print_agent_audit_v2(&conn, &root, since_days, json)?,
        Command::MemoryControlCenterV2 {
            root,
            since_days,
            json,
        } => print_memory_control_center_v2(&conn, &cli.db, &root, since_days, json)?,
        Command::MemoryControlCenter {
            root,
            since_days,
            json,
        } => print_memory_control_center_v2(&conn, &cli.db, &root, since_days, json)?,
        Command::AutoSupersedeV2 {
            root,
            since_days,
            apply,
            json,
        } => print_auto_supersede_v2(&conn, &root, since_days, apply, json)?,
        Command::MemoryDiffApply { root, apply, json } => {
            print_memory_diff_apply(&conn, &root, apply, json)?
        }
        Command::RecallBenchmarkSuite {
            root,
            since_days,
            limit,
            write_baseline,
            json,
        } => print_recall_benchmark_suite(&conn, &root, since_days, limit, write_baseline, json)?,
        Command::ReleaseGateV2 {
            root,
            since_days,
            strict,
            run,
            json,
        } => print_release_gate_v2(&conn, &cli.db, &root, since_days, strict, run, json)?,
        Command::RemoteSyncWizard {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_remote_sync_wizard(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::MemoryGovernancePolicy { root, apply, json } => {
            print_memory_governance_policy(&root, apply, json)?
        }
        Command::AutonomousLoopV2 {
            root,
            since_days,
            apply,
            json,
        } => print_autonomous_loop_v2(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::GovernanceEnforce {
            root,
            since_days,
            apply,
            json,
        } => print_governance_enforce(&conn, &root, since_days, apply, json)?,
        Command::MemoryQualityCi {
            root,
            since_days,
            minimal,
            json,
        } => print_memory_quality_ci(&conn, &cli.db, &root, since_days, minimal, json)?,
        Command::FleetDashboardV2 { since_days, json } => {
            print_fleet_dashboard_v2(&cli.db, since_days, json)?
        }
        Command::RemoteSyncApplyFlow {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_remote_sync_apply_flow(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::McpToolSurfaceV2 { json } => print_mcp_tool_surface_v2(json)?,
        Command::AutopilotV3 {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_autopilot_v3(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::SelfLearningRetrieval {
            root,
            since_days,
            apply,
            json,
        } => print_self_learning_retrieval(&conn, &root, since_days, apply, json)?,
        Command::ProjectRoleProfile {
            root,
            kind,
            apply,
            json,
        } => print_project_role_profile(&root, kind, apply, json)?,
        Command::InboxAiReviewer { limit, apply, json } => {
            print_inbox_ai_reviewer(&conn, limit, apply, json)?
        }
        Command::WebControlCenterV3 {
            root,
            target,
            since_days,
            json,
        } => {
            print_web_control_center_v3(&conn, &cli.db, &root, target.as_deref(), since_days, json)?
        }
        Command::RemoteSyncApply {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_remote_sync_apply(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::McpQualityTools { json } => print_mcp_quality_tools(json)?,
        Command::RemoteSyncControl {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_remote_sync_control(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::WebControlCenterV4 {
            root,
            target,
            since_days,
            json,
        } => {
            print_web_control_center_v4(&conn, &cli.db, &root, target.as_deref(), since_days, json)?
        }
        Command::McpDisciplineV2 {
            root,
            since_days,
            apply,
            json,
        } => print_mcp_discipline_v2(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::FeedbackLoopV2 {
            root,
            since_days,
            apply,
            json,
        } => print_feedback_loop_v2(&conn, &root, since_days, apply, json)?,
        Command::UpgradeAllProjectsV2 {
            from,
            to,
            backup_dir,
            dry_run,
            json,
        } => print_upgrade_all_projects_v2(
            &cli.db,
            from.as_deref(),
            &to,
            &backup_dir,
            dry_run,
            json,
        )?,
        Command::VdsSyncPack {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_vds_sync_pack(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::WebControlCenterV5 {
            root,
            target,
            since_days,
            json,
        } => {
            print_web_control_center_v5(&conn, &cli.db, &root, target.as_deref(), since_days, json)?
        }
        Command::QualityAutopilotV31 {
            root,
            since_days,
            apply,
            json,
        } => print_quality_autopilot_v31(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::MemoryRouterV2 {
            query,
            root,
            include_siblings,
            json,
        } => print_memory_router_v2(&cli.db, &root, &query, include_siblings, json)?,
        Command::BenchmarkProfiles {
            root,
            kind,
            since_days,
            write_baseline,
            apply,
            json,
        } => print_benchmark_profiles(&conn, &root, kind, since_days, write_baseline, apply, json)?,
        Command::InstallPolish { root, apply, json } => print_install_polish(&root, apply, json)?,
        Command::MemoryEffectivenessLab {
            root,
            since_days,
            json,
        } => print_memory_effectiveness_lab(&conn, &root, since_days, json)?,
        Command::AutoContextBudgeterV2 {
            task,
            root,
            target,
            apply,
            json,
        } => print_auto_context_budgeter_v2(&conn, &root, &task, target.as_deref(), apply, json)?,
        Command::MemoryContractV2 { root, write, json } => {
            print_memory_contract_v2(&conn, &root, write, json)?
        }
        Command::CrossProjectLearning {
            query,
            root,
            apply,
            json,
        } => print_cross_project_learning(&cli.db, &root, &query, apply, json)?,
        Command::AgentTrace {
            root,
            since_days,
            limit,
            json,
        } => print_agent_trace(&conn, &root, since_days, limit, json)?,
        Command::VdsSyncHardening {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_vds_sync_hardening(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::InstallQuality {
            root,
            since_days,
            apply,
            json,
        } => print_install_quality(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::WebControlCenterV6 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v6(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::Answer {
            question,
            root,
            scope,
            limit,
            json,
        } => print_memory_answer(&conn, &root, &question, scope.as_deref(), limit, json)?,
        Command::RagAnswer {
            question,
            root: _,
            scope,
            limit,
            budget,
            budget_profile,
            provider,
            endpoint,
            model,
            json,
        } => print_memory_rag_answer(
            &conn,
            &question,
            scope.as_deref(),
            limit,
            budget
                .or_else(|| budget_profile_chars(budget_profile))
                .unwrap_or(3000),
            select_cli_or_config(
                &provider,
                DEFAULT_EMBED_PROVIDER,
                &runtime.config.embeddings.provider,
            ),
            select_cli_or_config(
                &endpoint,
                DEFAULT_EMBED_ENDPOINT,
                &runtime.config.embeddings.endpoint,
            ),
            select_cli_or_config(
                &model,
                DEFAULT_EMBED_MODEL,
                &runtime.config.embeddings.model,
            ),
            &runtime.config.generation.provider,
            &runtime.config.generation.endpoint,
            &runtime.config.generation.model,
            json,
        )?,
        Command::RagDebug {
            question,
            scope,
            limit,
            budget,
            budget_profile,
            provider,
            endpoint,
            model,
            json,
        } => print_memory_rag_debug(
            &conn,
            &question,
            scope.as_deref(),
            limit,
            budget
                .or_else(|| budget_profile_chars(budget_profile))
                .unwrap_or(3000),
            select_cli_or_config(
                &provider,
                DEFAULT_EMBED_PROVIDER,
                &runtime.config.embeddings.provider,
            ),
            select_cli_or_config(
                &endpoint,
                DEFAULT_EMBED_ENDPOINT,
                &runtime.config.embeddings.endpoint,
            ),
            select_cli_or_config(
                &model,
                DEFAULT_EMBED_MODEL,
                &runtime.config.embeddings.model,
            ),
            json,
        )?,
        Command::RagIngest {
            input,
            root,
            scope,
            apply,
            embed,
            provider,
            endpoint,
            model,
            chunk_chars,
            overlap_chars,
            max_file_bytes,
            max_files,
            json,
        } => print_rag_ingest(
            &conn,
            RagIngestRequest {
                root: &root,
                input: &input,
                scope: &scope,
                apply,
                embed,
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                chunk_chars,
                overlap_chars,
                max_file_bytes,
                max_files,
                json,
            },
        )?,
        Command::RagSources {
            root,
            provider,
            endpoint,
            model,
            json,
        } => print_rag_sources(
            &conn,
            RagSourcesRequest {
                root: &root,
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                json,
            },
        )?,
        Command::GraphRag {
            query,
            scope,
            limit,
            budget,
            budget_profile,
            provider,
            endpoint,
            model,
            json,
        } => {
            let report = crate::app::graph_rag::compute_graph_rag(
                &conn,
                &query,
                scope.as_deref(),
                limit,
                budget
                    .or_else(|| budget_profile_chars(budget_profile))
                    .unwrap_or(3000),
                &runtime.config.generation,
                select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
            )?;
            if json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                println!(
                    "status: {} confidence: {} ({:.2})",
                    report.status, report.confidence, report.confidence_score
                );
                println!(
                    "graph: {} nodes={} seeds={} expanded={} edges={} isolated={}",
                    report.graph_summary.status,
                    report.graph_summary.node_count,
                    report.graph_summary.seed_count,
                    report.graph_summary.expanded_count,
                    report.graph_summary.edge_count,
                    report.graph_summary.isolated_node_count
                );
                println!("{}", report.answer);
                if !report.missing_evidence.is_empty() {
                    println!("\nmissing evidence:");
                    for item in &report.missing_evidence {
                        println!("- {item}");
                    }
                }
                if !report.trace.is_empty() {
                    println!("\ntrace:");
                    for entry in &report.trace {
                        println!(
                            "- #{} {} [{}] score={:.2} seed={}: {}",
                            entry.rank,
                            entry.id,
                            entry.memory_type,
                            entry.score,
                            entry.seed,
                            entry.title
                        );
                        if !entry.reasons.is_empty() {
                            println!("  reasons: {}", entry.reasons.join(", "));
                        }
                        if !entry.relationships.is_empty() {
                            println!("  relationships: {}", entry.relationships.join("; "));
                        }
                    }
                }
                if !report.relevant_edges.is_empty() {
                    println!("\ncontext relationships:");
                    for edge in &report.relevant_edges {
                        println!("- {} --[{}]--> {}", edge.source, edge.kind, edge.target);
                    }
                }
            }
        }
        Command::Tour { json } => {
            let topology_result = crate::app::topology::compute_topology(&conn)?;
            let narrative = crate::app::generation::generate_tour_narrative(
                &runtime.config.generation,
                &topology_result,
            )?;
            if json {
                println!("{}", serde_json::json!({ "tour": narrative }));
            } else {
                println!("{}", narrative);
            }
        }
        Command::Explain { id, json } => {
            let explanation =
                crate::app::explain::explain_component(&conn, &id, &runtime.config.generation)?;
            if json {
                println!("{}", serde_json::json!({ "explanation": explanation }));
            } else {
                println!("{}", explanation);
            }
        }
        Command::OnboardingGuide { json } => {
            let guide =
                crate::app::onboard::generate_onboarding_guide(&conn, &runtime.config.generation)?;
            if json {
                println!("{}", serde_json::json!({ "onboard": guide }));
            } else {
                println!("{}", guide);
            }
        }
        Command::ConnectCodex {
            root,
            since_days,
            apply,
            json,
        } => print_connect_codex(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::MemoryTypeGuide { json } => print_memory_type_guide(json)?,
        Command::MemoryEvalStory {
            root,
            since_days,
            write_baseline,
            json,
        } => print_memory_eval_story(&conn, &root, since_days, write_baseline, json)?,
        Command::ImportReview {
            input,
            root,
            scope,
            apply,
            json,
        } => print_import_review(&conn, &root, &input, &scope, apply, json)?,
        Command::MemoryUpload {
            input,
            root,
            scope,
            apply,
            json,
        } => print_memory_upload(&conn, &root, &input, &scope, apply, json)?,
        Command::MemantoGapReport { json } => print_memanto_gap_report(&conn, json)?,
        Command::MemoryTimeline { id, limit, json } => {
            print_memory_timeline(&conn, &id, limit, json)?
        }
        Command::MemoryConflictReview {
            stale_days,
            limit,
            json,
        } => print_memory_conflict_review(&conn, stale_days, limit, json)?,
        Command::WebControlCenterV7 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v7(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::AutonomousUsefulness {
            root,
            since_days,
            apply,
            json,
        } => print_autonomous_usefulness(&conn, &root, since_days, apply, json)?,
        Command::BenchmarkPolish {
            root,
            since_days,
            write_baseline,
            json,
        } => print_benchmark_polish(&conn, &root, since_days, write_baseline, json)?,
        Command::WebControlCenterV8 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v8(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::AutonomousSupervisor {
            root,
            since_days,
            apply,
            json,
        } => print_autonomous_supervisor(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::WebControlCenterV9 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v9(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::FleetSupervisor {
            since_days,
            apply,
            json,
        } => print_fleet_supervisor(&cli.db, since_days, apply, json)?,
        Command::WebControlCenterV10 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v10(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::FleetSupervisorWatchInstall {
            root,
            interval_secs,
            label,
            dry_run,
            json,
        } => print_fleet_supervisor_watch_install(
            &cli.db,
            &root,
            interval_secs,
            &label,
            dry_run,
            json,
        )?,
        Command::WebControlCenterV11 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v11(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::MemoryEffectivenessV2 {
            root,
            since_days,
            json,
        } => print_memory_effectiveness_v2(&conn, &root, since_days, json)?,
        Command::RecallBenchmarkBaselines {
            root,
            since_days,
            apply,
            json,
        } => print_recall_benchmark_baselines(&conn, &root, since_days, apply, json)?,
        Command::MemoryConflictApply {
            stale_days,
            limit,
            apply,
            json,
        } => print_memory_conflict_apply(&conn, stale_days, limit, apply, json)?,
        Command::McpToolSurfaceV3 { json } => print_mcp_tool_surface_v3(json)?,
        Command::McpDisciplineV3 {
            root,
            since_days,
            apply,
            json,
        } => print_mcp_discipline_v3(&conn, &cli.db, &root, since_days, apply, json)?,
        Command::FleetQuality { since_days, json } => {
            print_fleet_quality(&cli.db, since_days, json)?
        }
        Command::ReleaseGateV3 {
            root,
            since_days,
            strict,
            run,
            json,
        } => print_release_gate_v3(&conn, &cli.db, &root, since_days, strict, run, json)?,
        Command::WebControlCenterV12 {
            root,
            target,
            task,
            since_days,
            json,
        } => print_web_control_center_v12(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            &task,
            since_days,
            json,
        )?,
        Command::WebControlCenter {
            root,
            target,
            task,
            since_days,
            details,
            json,
        } => {
            if details {
                print_web_control_center_v12(
                    &conn,
                    &cli.db,
                    &root,
                    target.as_deref(),
                    &task,
                    since_days,
                    json,
                )?;
            } else {
                print_control_snapshot(&conn, &cli.db, &root, since_days, json)?;
            }
        }
        Command::ProjectTemplate {
            root,
            kind,
            apply,
            json,
        } => print_project_template(&root, kind, apply, json)?,
        Command::WatchControl {
            root,
            interval_secs,
            label,
            apply,
            json,
        } => print_watch_control(&cli.db, &root, interval_secs, &label, apply, json)?,
        Command::AutonomyControlCenter {
            root,
            since_days,
            json,
        } => print_autonomy_control_center(&conn, &cli.db, &root, since_days, json)?,
        Command::SyncLatency {
            root,
            target,
            samples,
            json,
        } => print_sync_latency(&conn, &cli.db, &root, target.as_deref(), samples, json)?,
        Command::SyncProfile {
            root,
            profile,
            target,
            apply,
            run_dry_run,
            json,
        } => print_sync_profile(
            &conn,
            &cli.db,
            &root,
            profile,
            target.as_deref(),
            apply,
            run_dry_run,
            json,
        )?,
        Command::MemoryDiffReview { root, apply, json } => {
            print_memory_diff_review(&conn, &root, apply, json)?
        }
        Command::RemoteSyncV2 {
            root,
            target,
            since_days,
            apply,
            json,
        } => print_remote_sync_v2(
            &conn,
            &cli.db,
            &root,
            target.as_deref(),
            since_days,
            apply,
            json,
        )?,
        Command::AgentEnforce {
            root,
            since_days,
            fix,
            json,
        } => print_agent_enforce(&conn, &cli.db, &root, since_days, fix, json)?,
        Command::InboxV2 { command } => handle_inbox_v2(&conn, command)?,
        Command::PolicyTune {
            output,
            dry_run,
            json,
        } => print_policy_tune(&conn, &output, dry_run, json)?,
        Command::MemoryQa {
            root,
            since_days,
            json,
        } => print_memory_qa(&conn, &root, since_days, json)?,
        Command::MemoryContract { root, write, json } => {
            print_memory_contract(&conn, &root, write, json)?
        }
        Command::UpgradeProject {
            root,
            from,
            to,
            backup_dir,
            dry_run,
            json,
        } => print_upgrade_project(
            &conn,
            &root,
            from.as_deref(),
            &to,
            &backup_dir,
            dry_run,
            json,
        )?,
        Command::UpgradeAllProjects {
            from,
            to,
            backup_dir,
            dry_run,
            json,
        } => print_upgrade_all_projects(&cli.db, from.as_deref(), &to, &backup_dir, dry_run, json)?,
        Command::CodexDoctor { config, json } => print_codex_doctor(&expand_tilde(&config), json)?,
        Command::WorkspaceInit { root, force } => workspace_init(&root, force)?,
        Command::Bundle { output, redact } => {
            release_ops::write_bundle(&conn, &cli.db, &output, redact)?
        }
        Command::Daemon {
            interval_secs,
            once,
            auto_ingest,
            no_autopilot,
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
        } => run_daemon(
            &conn,
            DaemonRequest {
                interval_secs,
                once,
                quiet: false,
                auto_ingest: auto_ingest || !no_autopilot,
                autopilot: !no_autopilot,
                session_dir: &session_dir,
                backup_dir: &backup_dir,
                status_file: &status_file,
                backup_keep,
                backup_every_secs,
                cleanup_audit_keep,
                db: &cli.db,
                scope: &scope,
                provider: &provider,
                endpoint: &endpoint,
                model: &model,
            },
        )?,
        Command::Autopilot { command } => handle_autopilot(&conn, &cli.db, command)?,
        Command::Autonomous { command } => handle_autonomous(&conn, &cli.db, command)?,
        Command::ServeHttp {
            host,
            port,
            once,
            auth_token,
            auth_token_file,
        } => {
            let auth_token = http_server::resolve_http_auth_token(
                auth_token.as_deref(),
                auth_token_file.as_deref(),
            )?;
            http_server::serve_http(&cli.db, &host, port, once, auth_token.as_deref())?;
        }
        Command::VecValidate { backend } => vec_validate(&conn, backend)?,
        Command::MergeCandidates { limit, json } => print_merge_candidates(&conn, limit, json)?,
        Command::MergeApply {
            primary_id,
            duplicate_id,
            dry_run,
        } => merge_apply(&conn, &primary_id, &duplicate_id, dry_run)?,
        Command::ResolveContradictions { dry_run } => resolve_contradictions(&conn, dry_run)?,
        Command::Doctrine { scope, json } => print_doctrine(&conn, scope.as_deref(), json)?,
        Command::Evidence { id, json } => print_evidence(&conn, &id, json)?,
        Command::Drift {
            changed_only,
            root,
            json,
        } => print_drift(&conn, &root, changed_only, json)?,
        Command::Profile { command } => handle_profile(command)?,
        Command::ReviewUi { stale_days } => print_review_tui(&conn, stale_days)?,
        Command::Maintain {
            llm,
            endpoint,
            model,
        } => maintain_memory(&conn, llm, &endpoint, &model)?,
        Command::Sync { command } => handle_sync(&conn, command)?,
        Command::Schema { command } => handle_schema(&conn, command)?,
        Command::Lock { command } => handle_lock(&conn, command)?,
        Command::Retrieve {
            query,
            strategy,
            format,
            limit,
            budget,
            budget_profile,
            scope,
            rules,
            provider,
            endpoint,
            model,
        } => print_retrieve(
            &conn,
            RetrieveRequest {
                query: &query,
                strategy,
                format,
                limit,
                budget: budget
                    .or_else(|| budget_profile_chars(budget_profile))
                    .unwrap_or(5000),
                scope: scope.as_deref(),
                rules: rules.as_deref(),
                provider: select_cli_or_config(
                    &provider,
                    DEFAULT_EMBED_PROVIDER,
                    &runtime.config.embeddings.provider,
                ),
                endpoint: select_cli_or_config(
                    &endpoint,
                    DEFAULT_EMBED_ENDPOINT,
                    &runtime.config.embeddings.endpoint,
                ),
                model: select_cli_or_config(
                    &model,
                    DEFAULT_EMBED_MODEL,
                    &runtime.config.embeddings.model,
                ),
                audit_read: true,
            },
        )?,
        Command::Eval { command } => {
            let eval_root = app_project_root_for_db(&cli.db).unwrap_or_else(|| PathBuf::from("."));
            handle_eval(&conn, command, &runtime.config.generation, &eval_root)?
        }
        Command::BuildInfo => print_build_info(&runtime),
        Command::ReleaseBundle { output } => {
            release_ops::write_release_bundle(&conn, &cli.db, &output)?
        }
        Command::Bench { json } => release_ops::print_bench(&conn, &cli.db, json)?,
        Command::SelfHost { force } => release_ops::self_host_memory(&conn, force)?,
        Command::Health {
            root,
            endpoint,
            json,
        } => ops::print_health(&conn, &cli.db, &root, &endpoint, json)?,
        Command::BackupPolicy {
            output_dir,
            keep,
            dry_run,
            json,
        } => ops::run_backup_policy(&cli.db, &output_dir, keep, dry_run, json)?,
        Command::Cleanup {
            audit_keep,
            rejected_inbox_days,
            dry_run,
            json,
        } => ops::run_cleanup(&conn, audit_keep, rejected_inbox_days, dry_run, json)?,
        Command::DaemonInstall {
            output,
            interval_secs,
            session_dir,
            force,
            dry_run,
        } => ops::write_launchd_plist(
            &cli.db,
            &output,
            interval_secs,
            &session_dir,
            force,
            dry_run,
        )?,
        Command::Integrity { json } => db::print_integrity(&conn, json)?,
        Command::Optimize { vacuum, json } => db::optimize_db(&conn, vacuum, json)?,
        Command::Backup { .. } | Command::Restore { .. } | Command::BackupVerify { .. } => {
            unreachable!()
        }
    }

    Ok(())
}

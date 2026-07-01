use crate::build_info::BuildInfo;
use crate::http_api::HttpResponse;
use crate::runtime_config::{AgentConfig, load_runtime_config};
use crate::services;
use crate::services::{MaintenanceService, MemoryService, RetrievalService};
use crate::storage::MemoryStore;
use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use regex::Regex;
use rhai::{Engine, Scope as RhaiScope};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DEFAULT_DB: &str = ".agent/memory.db";
const DEFAULT_CONFIG: &str = ".agent/config.toml";
const DEFAULT_EMBED_ENDPOINT: &str = "http://192.168.0.13:11434";
const DEFAULT_EMBED_MODEL: &str = "bge-m3:latest";
const DEFAULT_EMBED_PROVIDER: &str = "ollama";
const DEFAULT_INSTALL_BACKUP_KEEP: usize = 3;
const CURRENT_SCHEMA_VERSION: i64 = 15;
const EXPORT_VERSION: u32 = 1;
const VALID_SCOPES: &[&str] = &["global", "user", "project", "repo", "thread", "task"];

mod autonomous;
mod cli;
mod db;
mod diagnostics;
mod embeddings;
mod http_server;
mod maintenance;
mod mcp_server;
mod memory;
mod model;
mod observability;
mod ops;
mod project;
mod release_ops;
mod retrieval;
mod shared;
use autonomous::*;
use cli::*;
use db::*;
use diagnostics::*;
use maintenance::*;
use memory::*;
use model::*;
use observability::*;
use project::*;
use retrieval::*;
use shared::*;

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
                    links: Vec::new(),
                },
            )?;
            println!("{id}");
        }
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
        Command::VecStatus => print_vec_status(),
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
            let rows =
                embeddings::semantic_search(&conn, &provider, &endpoint, &model, &query, limit)?;
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
        } => embeddings::print_vector_bench(&conn, &provider, &endpoint, &model)?,
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
            json,
        } => print_project_doctor(&conn, &cli.db, &root, since_days, json)?,
        Command::ReleaseGate {
            root,
            since_days,
            strict,
            json,
        } => print_release_gate(&conn, &cli.db, &root, since_days, strict, json)?,
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
        Command::ServeHttp { host, port, once } => {
            http_server::serve_http(&cli.db, &host, port, once)?
        }
        Command::VecMigrate { backend } => vec_migrate(&conn, backend)?,
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
        Command::Eval { command } => handle_eval(&conn, command)?,
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

fn init_project(conn: &Connection, db: &Path, config: &Path, force: bool) -> Result<()> {
    if config.exists() && !force {
        bail!(
            "config already exists: {} (use --force to overwrite)",
            config.display()
        );
    }
    let cfg = AgentConfig::production_defaults(
        db,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    );
    let content = toml::to_string_pretty(&cfg)?;
    write_file(config, content.as_bytes())?;
    if let Some(root) = project_root_from_config(config) {
        upsert_project_agents(&root)?;
    }
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    println!("config: {}", config.display());
    println!("database: {}", db.display());
    println!("memories: {total}");
    Ok(())
}

fn write_project_config(
    config: &Path,
    db: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    let mut cfg = if config.exists() {
        let raw = fs::read_to_string(config)
            .with_context(|| format!("failed to read {}", config.display()))?;
        toml::from_str::<AgentConfig>(&raw)
            .with_context(|| format!("failed to parse {}", config.display()))?
    } else {
        AgentConfig::production_defaults(db, provider, endpoint, model)
    };
    cfg.db_path = db.display().to_string();
    cfg.embeddings.provider = provider.to_string();
    cfg.embeddings.endpoint = endpoint.to_string();
    cfg.embeddings.model = model.to_string();
    let content = toml::to_string_pretty(&cfg)?;
    write_file(config, content.as_bytes())?;
    Ok(())
}

fn export_memories(
    conn: &Connection,
    types: &[String],
    statuses: &[String],
    scope: Option<&str>,
) -> Result<MemoryExport> {
    let memories = query_memories(conn, None, types, statuses, scope, usize::MAX)?
        .into_iter()
        .map(|memory| {
            let links = get_links(conn, &memory.id)?;
            Ok(MemoryWithLinks { memory, links })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(MemoryExport {
        version: EXPORT_VERSION,
        exported_at: now_ms(),
        memories,
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncBundle {
    version: u32,
    kind: String,
    created_at: i64,
    dukememory_version: String,
    manifest: SyncBundleManifest,
    export: MemoryExport,
}

#[derive(Debug, Serialize, Deserialize)]
struct SyncBundleManifest {
    memory_count: usize,
    redacted: bool,
    checksum_algorithm: String,
    export_sha256: String,
    local_first: bool,
    source_schema: i64,
}

#[derive(Debug, Serialize)]
struct SyncExportReport {
    version: u32,
    ok: bool,
    dry_run: bool,
    output: String,
    memory_count: usize,
    redacted: bool,
    export_sha256: String,
    bytes: usize,
    wrote: bool,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SyncImportReport {
    version: u32,
    ok: bool,
    dry_run: bool,
    input: String,
    replace: bool,
    memory_count: usize,
    export_sha256: Option<String>,
    checksum_ok: Option<bool>,
    rollback: Option<String>,
    imported: usize,
    recommendations: Vec<String>,
}

fn sync_bundle(conn: &Connection, redact: bool) -> Result<SyncBundle> {
    let mut export = export_memories(conn, &[], &[], None)?;
    if redact {
        redact_export(&mut export)?;
    }
    let export_json = serde_json::to_vec(&export)?;
    let export_sha256 = sha256_bytes(&export_json);
    Ok(SyncBundle {
        version: 1,
        kind: "dukememory.sync.bundle".to_string(),
        created_at: now_ms(),
        dukememory_version: env!("CARGO_PKG_VERSION").to_string(),
        manifest: SyncBundleManifest {
            memory_count: export.memories.len(),
            redacted: redact,
            checksum_algorithm: "sha256".to_string(),
            export_sha256,
            local_first: true,
            source_schema: schema_version(conn).unwrap_or(CURRENT_SCHEMA_VERSION),
        },
        export,
    })
}

fn parse_sync_input(input: &Path) -> Result<(MemoryExport, Option<SyncBundleManifest>)> {
    let raw =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let value: Value = serde_json::from_str(&raw)?;
    if value.get("kind").and_then(Value::as_str) == Some("dukememory.sync.bundle") {
        let bundle: SyncBundle = serde_json::from_value(value)?;
        if bundle.version != 1 {
            bail!("unsupported sync bundle version: {}", bundle.version);
        }
        let export_json = serde_json::to_vec(&bundle.export)?;
        let actual = sha256_bytes(&export_json);
        if actual != bundle.manifest.export_sha256 {
            bail!("sync bundle checksum mismatch");
        }
        return Ok((bundle.export, Some(bundle.manifest)));
    }
    let export: MemoryExport = serde_json::from_value(value)?;
    Ok((export, None))
}

fn import_memories(conn: &Connection, input: &Path, replace: bool) -> Result<()> {
    let (export, _) = parse_sync_input(input)?;
    import_memory_export(conn, export, replace).map(|count| {
        println!("imported: {count}");
    })
}

fn import_memory_export(conn: &Connection, export: MemoryExport, replace: bool) -> Result<usize> {
    if export.version != EXPORT_VERSION {
        bail!("unsupported export version: {}", export.version);
    }
    if replace {
        conn.execute("DELETE FROM memories", [])?;
    }
    let mut count = 0;
    for item in export.memories {
        conn.execute(
            r#"
            INSERT OR REPLACE INTO memories (
                id, type, scope, title, body, status, source,
                created_at, updated_at, supersedes, superseded_by, confidence
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                item.memory.id,
                item.memory.memory_type,
                item.memory.scope,
                item.memory.title,
                item.memory.body,
                item.memory.status,
                item.memory.source,
                item.memory.created_at,
                item.memory.updated_at,
                item.memory.supersedes,
                item.memory.superseded_by,
                item.memory.confidence,
            ],
        )?;
        conn.execute(
            "DELETE FROM memory_links WHERE memory_id = ?1",
            params![item.memory.id],
        )?;
        insert_links(conn, &item.memory.id, &item.links)?;
        count += 1;
    }
    Ok(count)
}

struct RestoreDbRequest<'a> {
    db: &'a Path,
    input: &'a Path,
    force: bool,
    dry_run: bool,
    strict: bool,
    rollback_dir: &'a Path,
    journal_dir: &'a Path,
    rollback: bool,
}

#[derive(Debug, Serialize)]
struct RestoreJournal {
    format_version: u32,
    created_at: i64,
    dukememory_version: String,
    status: String,
    target: String,
    source: String,
    force: bool,
    strict: bool,
    dry_run: bool,
    rollback_enabled: bool,
    rollback: Option<String>,
    rollback_verified: Option<bool>,
    error: Option<String>,
}

fn restore_db(request: RestoreDbRequest<'_>) -> Result<()> {
    let mut journal = RestoreJournal {
        format_version: 1,
        created_at: now_ms(),
        dukememory_version: env!("CARGO_PKG_VERSION").to_string(),
        status: "started".to_string(),
        target: request.db.display().to_string(),
        source: request.input.display().to_string(),
        force: request.force,
        strict: request.strict,
        dry_run: request.dry_run,
        rollback_enabled: request.rollback,
        rollback: None,
        rollback_verified: None,
        error: None,
    };

    let result = (|| -> Result<()> {
        if request.db.exists() && !request.force {
            bail!(
                "database already exists: {} (use --force to replace)",
                request.db.display()
            );
        }
        ops::ensure_backup_verified(request.input, request.strict)?;
        let rollback_path = if request.rollback && request.db.exists() {
            Some(next_restore_rollback_path(request.rollback_dir))
        } else {
            None
        };
        journal.rollback = rollback_path
            .as_ref()
            .map(|path| path.display().to_string());
        if request.dry_run {
            println!("restore: verified");
            println!("target: {}", request.db.display());
            println!("source: {}", request.input.display());
            if let Some(path) = rollback_path {
                println!("rollback: {}", path.display());
            } else {
                println!("rollback: none");
            }
            return Ok(());
        }
        if let Some(path) = rollback_path {
            let existing = Connection::open(request.db)
                .with_context(|| format!("failed to open target db {}", request.db.display()))?;
            existing.busy_timeout(std::time::Duration::from_secs(15))?;
            sqlite_backup_to(&existing, &path)?;
            ops::write_backup_metadata(&existing, &path)?;
            ops::ensure_backup_verified(&path, true)?;
            journal.rollback_verified = Some(true);
            println!("rollback: {}", path.display());
        }
        restore_db_atomically(request.db, request.input)?;
        println!("{}", request.db.display());
        Ok(())
    })();

    match result {
        Ok(()) => {
            if !request.dry_run {
                journal.status = "success".to_string();
                let path = write_restore_journal(request.journal_dir, &journal)?;
                println!("journal: {}", path.display());
            }
            Ok(())
        }
        Err(error) => {
            if !request.dry_run {
                journal.status = "failed".to_string();
                journal.error = Some(format!("{error:#}"));
                let _ = write_restore_journal(request.journal_dir, &journal);
            }
            Err(error)
        }
    }
}

fn write_restore_journal(dir: &Path, journal: &RestoreJournal) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = dir.join(format!("restore-{}.json", now_ms()));
    write_file(&path, serde_json::to_string_pretty(journal)?.as_bytes())?;
    Ok(path)
}

fn next_restore_rollback_path(dir: &Path) -> PathBuf {
    let ts = now_ms();
    let first = dir.join(format!("restore-rollback-{ts}.db"));
    if !first.exists() {
        return first;
    }
    for suffix in 1..=999 {
        let path = dir.join(format!("restore-rollback-{ts}-{suffix}.db"));
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("restore-rollback-{}-overflow.db", now_ms()))
}

fn restore_db_atomically(db: &Path, input: &Path) -> Result<()> {
    if let Some(parent) = db.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = db.with_extension("db.restore.tmp");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("failed to remove {}", tmp.display()))?;
    }
    copy_file(input, &tmp)?;
    let conn = Connection::open(&tmp)
        .with_context(|| format!("failed to open restored temp db {}", tmp.display()))?;
    if !ops::sqlite_integrity_ok(&conn) {
        let _ = fs::remove_file(&tmp);
        bail!("restored temp database failed SQLite integrity check");
    }
    drop(conn);
    if db.exists() {
        fs::remove_file(db).with_context(|| format!("failed to remove {}", db.display()))?;
    }
    fs::rename(&tmp, db)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), db.display()))?;
    Ok(())
}

fn sqlite_backup_to(conn: &Connection, output: &Path) -> Result<()> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if output.exists() {
        fs::remove_file(output)
            .with_context(|| format!("failed to remove {}", output.display()))?;
    }
    let tmp = output.with_extension("db.tmp");
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("failed to remove {}", tmp.display()))?;
    }
    let tmp_sql = tmp.display().to_string();
    conn.execute("VACUUM INTO ?1", params![tmp_sql])?;
    fs::rename(&tmp, output)
        .with_context(|| format!("failed to rename {} to {}", tmp.display(), output.display()))?;
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn read_events(conn: &Connection, since_ms: i64, limit: usize) -> Result<Vec<MemoryReadEvent>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at
        FROM memory_read_events
        WHERE created_at >= ?1
        ORDER BY created_at DESC, id DESC
        LIMIT ?2
        "#,
    )?;
    stmt.query_map(
        params![since_ms, limit.min(i64::MAX as usize) as i64],
        |row| {
            let ids: String = row.get(3)?;
            Ok(MemoryReadEvent {
                id: row.get(0)?,
                command: row.get(1)?,
                query: row.get(2)?,
                memory_ids: split_csv(Some(&ids)),
                semantic_used: row.get::<_, i64>(4)? != 0,
                result_count: row.get::<_, i64>(5)?.max(0) as usize,
                budget: row.get::<_, i64>(6)?.max(0) as usize,
                elapsed_ms: row.get::<_, i64>(7)?.max(0) as u128,
                created_at: row.get(8)?,
            })
        },
    )?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn memory_request_counts(conn: &Connection) -> Result<HashMap<String, usize>> {
    memory_request_counts_since(conn, None)
}

fn memory_request_counts_since(
    conn: &Connection,
    since_ms: Option<i64>,
) -> Result<HashMap<String, usize>> {
    let mut sql = "SELECT memory_ids FROM memory_read_events".to_string();
    if since_ms.is_some() {
        sql.push_str(" WHERE created_at >= ?1");
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(since_ms) = since_ms {
        stmt.query_map(params![since_ms], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut counts = HashMap::new();
    for row in rows {
        for id in split_csv(Some(&row)) {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

fn memory_request_count(conn: &Connection, memory_id: &str) -> Result<usize> {
    Ok(memory_request_counts(conn)?
        .get(memory_id)
        .copied()
        .unwrap_or(0))
}

fn audit_events(conn: &Connection, limit: usize) -> Result<Vec<MemoryEvent>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, event_type, memory_id, detail, created_at
        FROM memory_events
        ORDER BY created_at DESC, id DESC
        LIMIT ?1
        "#,
    )?;
    stmt.query_map(params![limit.min(i64::MAX as usize)], |row| {
        Ok(MemoryEvent {
            id: row.get(0)?,
            event_type: row.get(1)?,
            memory_id: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn memory_events(conn: &Connection, memory_id: &str, limit: usize) -> Result<Vec<MemoryEvent>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, event_type, memory_id, detail, created_at
        FROM memory_events
        WHERE memory_id = ?1
        ORDER BY created_at DESC, id DESC
        LIMIT ?2
        "#,
    )?;
    stmt.query_map(params![memory_id, limit.min(i64::MAX as usize)], |row| {
        Ok(MemoryEvent {
            id: row.get(0)?,
            event_type: row.get(1)?,
            memory_id: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn ensure_vector_backend(conn: &Connection, backend: VectorBackend) -> Result<()> {
    match backend {
        VectorBackend::Json => Ok(()),
        VectorBackend::SqliteVec => {
            if !cfg!(feature = "vec") {
                bail!(
                    "sqlite-vec backend requested, but this binary was built without --features vec"
                );
            }
            let available = conn
                .query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0))
                .optional()
                .is_ok();
            if !available {
                bail!("sqlite-vec backend requested, but sqlite vec_version() is not available");
            }
            Ok(())
        }
    }
}

fn vec_migrate(conn: &Connection, backend: VectorBackend) -> Result<()> {
    ensure_vector_backend(conn, backend)?;
    let detail = match backend {
        VectorBackend::Json => "vector backend set to json fallback",
        VectorBackend::SqliteVec => "vector backend set to sqlite-vec",
    };
    log_event(conn, "vec_migrate", None, detail)?;
    println!("{detail}");
    Ok(())
}

fn print_merge_candidates(conn: &Connection, limit: usize, json_out: bool) -> Result<()> {
    let candidates = merge_candidates(conn, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&candidates)?);
    } else if candidates.is_empty() {
        println!("merge_candidates: none");
    } else {
        for item in candidates {
            println!(
                "{}  {}  {}  {}",
                item.primary_id, item.duplicate_id, item.title, item.reason
            );
        }
    }
    Ok(())
}

fn merge_candidates(conn: &Connection, limit: usize) -> Result<Vec<MergeCandidate>> {
    let rows = query_memories(conn, None, &[], &["active".to_string()], None, usize::MAX)?;
    let mut out = Vec::new();
    for i in 0..rows.len() {
        for j in (i + 1)..rows.len() {
            if rows[i].memory_type == rows[j].memory_type
                && rows[i].scope == rows[j].scope
                && !titles_have_different_versions(&rows[i].title, &rows[j].title)
                && title_similarity(&rows[i].title, &rows[j].title) >= 0.65
            {
                out.push(MergeCandidate {
                    primary_id: rows[i].id.clone(),
                    duplicate_id: rows[j].id.clone(),
                    title: rows[i].title.clone(),
                    reason: "similar type/scope/title".to_string(),
                });
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

fn title_similarity(a: &str, b: &str) -> f64 {
    let a = tokenize(a);
    let b = tokenize(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let overlap = a.intersection(&b).count() as f64;
    overlap / a.len().max(b.len()) as f64
}

fn titles_have_different_versions(a: &str, b: &str) -> bool {
    let a_versions = title_versions(a);
    let b_versions = title_versions(b);
    !a_versions.is_empty() && !b_versions.is_empty() && a_versions.is_disjoint(&b_versions)
}

fn title_versions(title: &str) -> HashSet<String> {
    let mut versions = HashSet::new();
    let chars = title.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut dots = 0;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            if chars[i] == '.' {
                dots += 1;
            }
            i += 1;
        }
        if dots > 0 {
            let candidate = chars[start..i].iter().collect::<String>();
            let parts = candidate.split('.').collect::<Vec<_>>();
            if parts.len() >= 2 && parts.iter().all(|part| !part.is_empty()) {
                versions.insert(candidate);
            }
        }
    }
    versions
}

fn merge_apply(
    conn: &Connection,
    primary_id: &str,
    duplicate_id: &str,
    dry_run: bool,
) -> Result<()> {
    let primary = get_memory(conn, primary_id)?;
    let duplicate = get_memory(conn, duplicate_id)?;
    if dry_run {
        println!("would_merge {duplicate_id} -> {primary_id}");
        return Ok(());
    }
    let body = format!(
        "{}\n\nMerged from {}:\n{}",
        primary.body, duplicate.id, duplicate.body
    );
    transactional(conn, "merge_apply", || {
        conn.execute(
            "UPDATE memories SET body = ?1, updated_at = ?2 WHERE id = ?3",
            params![body, now_ms(), primary_id],
        )?;
        conn.execute(
            "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
            params![primary_id, now_ms(), duplicate_id],
        )?;
        log_event(
            conn,
            "memory_merged",
            Some(primary_id),
            &format!("merged duplicate {duplicate_id}"),
        )?;
        Ok(())
    })?;
    println!("{primary_id}");
    Ok(())
}

fn resolve_contradictions(conn: &Connection, dry_run: bool) -> Result<()> {
    let rows = query_memories(
        conn,
        None,
        &["decision".to_string()],
        &["active".to_string()],
        None,
        usize::MAX,
    )?;
    let mut changed = 0;
    for candidate in merge_candidates(conn, usize::MAX)? {
        let old = rows.iter().find(|row| row.id == candidate.duplicate_id);
        let new = rows.iter().find(|row| row.id == candidate.primary_id);
        if let (Some(old), Some(new)) = (old, new)
            && old.created_at < new.created_at
        {
            if dry_run {
                println!("would_supersede {} -> {}", old.id, new.id);
            } else {
                conn.execute(
                    "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3",
                    params![new.id, now_ms(), old.id],
                )?;
                log_event(
                    conn,
                    "contradiction_resolved",
                    Some(&new.id),
                    &format!("superseded {}", old.id),
                )?;
            }
            changed += 1;
        }
    }
    println!("resolved: {changed}");
    Ok(())
}

fn handle_profile(command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::List { dir } => {
            fs::create_dir_all(&dir)?;
            let mut names = Vec::new();
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    names.push(entry.file_name().to_string_lossy().to_string());
                }
            }
            names.sort();
            for name in names {
                println!("{name}");
            }
        }
        ProfileCommand::Use { name, dir } => {
            fs::create_dir_all(dir.join(&name))?;
            write_file(&PathBuf::from(".agent/active_profile"), name.as_bytes())?;
            println!("{name}");
        }
    }
    Ok(())
}

fn maintain_memory(conn: &Connection, llm: bool, endpoint: &str, model: &str) -> Result<()> {
    println!("Maintenance Suggestions");
    for candidate in merge_candidates(conn, 10)? {
        println!(
            "- merge {} into {} ({})",
            candidate.duplicate_id, candidate.primary_id, candidate.reason
        );
    }
    for issue in review_duplicates(conn)? {
        println!("- conflict {} {}", issue.id, issue.title);
    }
    if llm {
        let snapshot = render_context_pack(
            conn,
            &query_memories(conn, None, &[], &["active".to_string()], None, 20)?,
            4000,
        )?;
        let prompt = format!("Suggest memory maintenance actions:\n{snapshot}");
        match suggest_from_llm(endpoint, model, &prompt) {
            Ok(suggestions) => {
                for item in suggestions {
                    println!("- llm {} {}", item.memory_type, item.title);
                }
            }
            Err(err) => println!("- llm unavailable: {err}"),
        }
    }
    Ok(())
}

fn handle_sync(conn: &Connection, command: SyncCommand) -> Result<()> {
    match command {
        SyncCommand::Export {
            output,
            redact,
            dry_run,
            json,
        } => {
            let bundle = sync_bundle(conn, redact)?;
            let payload = serde_json::to_vec_pretty(&bundle)?;
            let report = SyncExportReport {
                version: 1,
                ok: true,
                dry_run,
                output: output.display().to_string(),
                memory_count: bundle.manifest.memory_count,
                redacted: redact,
                export_sha256: bundle.manifest.export_sha256.clone(),
                bytes: payload.len(),
                wrote: !dry_run,
                recommendations: vec![
                    "import with dukememory sync import --dry-run before applying".to_string(),
                    "keep agent reads local-first; use this bundle for backup/sync only"
                        .to_string(),
                ],
            };
            if !dry_run {
                write_file(&output, &payload)?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if dry_run {
                println!("sync export dry-run: {}", output.display());
                println!("memories: {}", report.memory_count);
                println!("bytes: {}", report.bytes);
            } else {
                println!("{}", output.display());
            }
            Ok(())
        }
        SyncCommand::Import {
            input,
            replace,
            dry_run,
            json,
        } => {
            let (export, manifest) = parse_sync_input(&input)?;
            let memory_count = export.memories.len();
            let export_sha256 = manifest
                .as_ref()
                .map(|manifest| manifest.export_sha256.clone());
            let rollback = if dry_run {
                None
            } else {
                let rollback_dir = PathBuf::from(".agent/sync-rollbacks");
                fs::create_dir_all(&rollback_dir)?;
                let rollback_path = rollback_dir.join(format!("sync-{}.json", now_ms()));
                let rollback_export = export_memories(conn, &[], &[], None)?;
                write_file(
                    &rollback_path,
                    serde_json::to_string_pretty(&rollback_export)?.as_bytes(),
                )?;
                Some(rollback_path.display().to_string())
            };
            let imported = if dry_run {
                0
            } else {
                import_memory_export(conn, export, replace)?
            };
            let report = SyncImportReport {
                version: 1,
                ok: true,
                dry_run,
                input: input.display().to_string(),
                replace,
                memory_count,
                export_sha256,
                checksum_ok: manifest.as_ref().map(|_| true),
                rollback,
                imported,
                recommendations: if dry_run {
                    vec![
                        "rerun without --dry-run only after reviewing memory_count and checksum"
                            .to_string(),
                    ]
                } else {
                    vec!["run dukememory embed-index after importing synced memory".to_string()]
                },
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if dry_run {
                println!("sync import dry-run: {}", input.display());
                println!("memories: {memory_count}");
            } else {
                println!("imported: {imported}");
                if let Some(rollback) = report.rollback {
                    println!("rollback: {rollback}");
                }
            }
            Ok(())
        }
    }
}

fn handle_lock(conn: &Connection, command: LockCommand) -> Result<()> {
    match command {
        LockCommand::Status => {
            let mut stmt = conn.prepare(
                "SELECT name, owner, acquired_at, expires_at FROM memory_locks ORDER BY name",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            let mut count = 0;
            for row in rows {
                let (name, owner, acquired_at, expires_at) = row?;
                println!("{name}  {owner}  acquired={acquired_at} expires={expires_at}");
                count += 1;
            }
            if count == 0 {
                println!("locks: none");
            }
        }
        LockCommand::Clear { name } => {
            let changed = if let Some(name) = name {
                conn.execute("DELETE FROM memory_locks WHERE name = ?1", params![name])?
            } else {
                conn.execute("DELETE FROM memory_locks", [])?
            };
            println!("cleared: {changed}");
        }
    }
    Ok(())
}

fn acquire_lock(conn: &Connection, name: &str, owner: &str, ttl_ms: i64) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "DELETE FROM memory_locks WHERE name = ?1 AND expires_at < ?2",
        params![name, now],
    )?;
    let changed = conn.execute(
        "INSERT OR IGNORE INTO memory_locks (name, owner, acquired_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![name, owner, now, now + ttl_ms],
    )?;
    if changed == 0 {
        bail!("lock is already held: {name}");
    }
    Ok(())
}

fn release_lock(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM memory_locks WHERE name = ?1", params![name])?;
    Ok(())
}

fn print_memory_output(
    conn: &Connection,
    rows: &[Memory],
    format: OutputFormat,
    max_chars: usize,
    title: &str,
) -> Result<()> {
    match format {
        OutputFormat::Plain => println!("{}", render_context_pack(conn, rows, max_chars)?),
        OutputFormat::Json => {
            let full = rows
                .iter()
                .map(|m| get_memory_with_links(conn, &m.id))
                .collect::<Result<Vec<_>>>()?;
            println!("{}", serde_json::to_string_pretty(&full)?);
        }
        OutputFormat::Markdown => {
            println!("## {title}");
            for row in rows {
                println!("- **{}** `{}`: {}", row.title, row.memory_type, row.body);
            }
        }
        OutputFormat::Agent => {
            println!("{title}:");
            println!("{}", render_context_pack(conn, rows, max_chars)?);
            println!(
                "\nUse these memories as constraints unless contradicted by newer user input."
            );
        }
    }
    Ok(())
}

fn select_cli_or_config<'a>(
    cli_value: &'a str,
    default_value: &str,
    config_value: &'a str,
) -> &'a str {
    if cli_value == default_value {
        config_value
    } else {
        cli_value
    }
}

fn budget_profile_chars(profile: Option<BudgetProfile>) -> Option<usize> {
    profile.map(|profile| match profile {
        BudgetProfile::Tiny => 1200,
        BudgetProfile::Normal => 3000,
        BudgetProfile::Deep => 8000,
    })
}

struct RhaiRules {
    engine: Engine,
    ast: rhai::AST,
}

fn load_rhai_rules(path: &Path) -> Result<RhaiRules> {
    let script =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let engine = Engine::new();
    let ast = engine.compile(&script)?;
    Ok(RhaiRules { engine, ast })
}

fn rhai_score(rules: Option<&RhaiRules>, memory: &Memory, task: &str) -> Result<f64> {
    let Some(rules) = rules else {
        return Ok(0.0);
    };
    let mut scope = RhaiScope::new();
    let result = rules.engine.call_fn::<f64>(
        &mut scope,
        &rules.ast,
        "score_memory",
        (
            memory.memory_type.clone(),
            memory.status.clone(),
            memory.scope.clone(),
            memory.title.clone(),
            memory.body.clone(),
            task.to_string(),
            memory.confidence,
        ),
    );
    match result {
        Ok(score) => Ok(score),
        Err(_) => Ok(0.0),
    }
}

fn check_rhai_rules(path: &Path) -> Result<()> {
    let rules = load_rhai_rules(path)?;
    let sample = Memory {
        id: "sample".to_string(),
        memory_type: "decision".to_string(),
        scope: "project".to_string(),
        title: "Sample".to_string(),
        body: "Sample body".to_string(),
        status: "active".to_string(),
        source: None,
        created_at: now_ms(),
        updated_at: now_ms(),
        supersedes: None,
        superseded_by: None,
        confidence: 1.0,
    };
    let score = rhai_score(Some(&rules), &sample, "sample task")?;
    println!("ok score={score}");
    Ok(())
}

fn check_policy_rules(path: &Path) -> Result<()> {
    let rules = load_rhai_rules(path)?;
    let sample = Memory {
        id: "sample".to_string(),
        memory_type: "decision".to_string(),
        scope: "project".to_string(),
        title: "Sample".to_string(),
        body: "Sample body with token = demo".to_string(),
        status: "active".to_string(),
        source: None,
        created_at: now_ms(),
        updated_at: now_ms(),
        supersedes: None,
        superseded_by: None,
        confidence: 1.0,
    };
    let score = rhai_score(Some(&rules), &sample, "sample task")?;
    let include = rhai_should_include(Some(&rules), &sample, "sample task")?;
    let redact = rhai_should_redact(Some(&rules), &sample)?;
    println!("ok score={score} include={include} redact={redact}");
    Ok(())
}

fn rhai_should_include(rules: Option<&RhaiRules>, memory: &Memory, task: &str) -> Result<bool> {
    let Some(rules) = rules else {
        return Ok(true);
    };
    let mut scope = RhaiScope::new();
    let result = rules.engine.call_fn::<bool>(
        &mut scope,
        &rules.ast,
        "should_include",
        (
            memory.memory_type.clone(),
            memory.status.clone(),
            memory.scope.clone(),
            memory.title.clone(),
            memory.body.clone(),
            task.to_string(),
            memory.confidence,
        ),
    );
    Ok(result.unwrap_or(true))
}

fn rhai_should_redact(rules: Option<&RhaiRules>, memory: &Memory) -> Result<bool> {
    let Some(rules) = rules else {
        return Ok(false);
    };
    let mut scope = RhaiScope::new();
    let result = rules.engine.call_fn::<bool>(
        &mut scope,
        &rules.ast,
        "should_redact",
        (
            memory.memory_type.clone(),
            memory.status.clone(),
            memory.scope.clone(),
            memory.title.clone(),
            memory.body.clone(),
            memory.confidence,
        ),
    );
    Ok(result.unwrap_or(false))
}

fn apply_policy_rules(conn: &Connection, path: &Path, dry_run: bool) -> Result<()> {
    let rules = load_rhai_rules(path)?;
    let rows = query_memories(conn, None, &[], &[], None, usize::MAX)?;
    let mut redacted = 0;
    let mut rejected = 0;
    for row in rows {
        if rhai_should_redact(Some(&rules), &row)? {
            if dry_run {
                println!("would_redact {} {}", row.id, row.title);
            } else {
                let title = redact_sensitive_text(&row.title)?;
                let body = redact_sensitive_text(&row.body)?;
                conn.execute(
                    "UPDATE memories SET title = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
                    params![title, body, now_ms(), row.id],
                )?;
                log_event(
                    conn,
                    "policy_redacted",
                    Some(&row.id),
                    "redacted by Rhai policy",
                )?;
            }
            redacted += 1;
        }
        if !rhai_should_include(Some(&rules), &row, "policy apply")? && row.status == "active" {
            if dry_run {
                println!("would_reject {} {}", row.id, row.title);
            } else {
                conn.execute(
                    "UPDATE memories SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
                    params![now_ms(), row.id],
                )?;
                log_event(
                    conn,
                    "policy_rejected",
                    Some(&row.id),
                    "rejected by Rhai policy",
                )?;
            }
            rejected += 1;
        }
    }
    println!("policy_redact: {redacted}");
    println!("policy_reject: {rejected}");
    Ok(())
}

fn print_project_summary(conn: &Connection, max_chars: usize, json_out: bool) -> Result<()> {
    let mut rows = Vec::new();
    for (kind, limit) in [
        ("product_goal", 5usize),
        ("constraint", 5),
        ("decision", 10),
        ("user_preference", 8),
        ("known_issue", 8),
        ("task_state", 5),
    ] {
        rows.extend(query_memories(
            conn,
            None,
            &[kind.to_string()],
            &["active".to_string(), "uncertain".to_string()],
            None,
            limit,
        )?);
    }
    rank_context_rows(&mut rows, "project summary", None, None);
    if json_out {
        let full = rows
            .iter()
            .map(|m| get_memory_with_links(conn, &m.id))
            .collect::<Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string_pretty(&full)?);
    } else {
        println!("{}", render_context_pack(conn, &rows, max_chars)?);
    }
    Ok(())
}

fn print_open_questions(conn: &Connection, json_out: bool) -> Result<()> {
    let mut rows = query_memories(
        conn,
        None,
        &[],
        &["uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    rows.extend(query_memories(
        conn,
        Some("question open todo decide unresolved"),
        &[],
        &["active".to_string()],
        None,
        20,
    )?);
    dedup_memories(&mut rows);
    print_rows(conn, &rows, json_out)
}

fn print_next_actions(conn: &Connection, limit: usize, json_out: bool) -> Result<()> {
    let rows = query_memories(
        conn,
        None,
        &["task_state".to_string()],
        &["active".to_string()],
        None,
        limit,
    )?;
    print_rows(conn, &rows, json_out)
}

fn dedup_memories(rows: &mut Vec<Memory>) {
    let mut seen = HashSet::new();
    rows.retain(|m| seen.insert(m.id.clone()));
}

fn apply_lifecycle(
    conn: &Connection,
    stale_days: i64,
    dry_run: bool,
    rules: Option<&Path>,
) -> Result<()> {
    if let Some(path) = rules {
        check_rhai_rules(path)?;
    }
    let stale = review_stale(conn, stale_days)?;
    if dry_run {
        println!("would_mark_uncertain: {}", stale.len());
        for issue in stale {
            println!("{}  {}", issue.id, issue.title);
        }
        return Ok(());
    }
    let mut changed = 0;
    for issue in stale {
        conn.execute(
            "UPDATE memories SET status = 'uncertain', updated_at = ?1 WHERE id = ?2 AND status = 'active'",
            params![now_ms(), issue.id],
        )?;
        changed += 1;
    }
    println!("marked_uncertain: {changed}");
    Ok(())
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn split_csv(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn sanitize_fts_query(query: &str) -> String {
    let cleaned = query
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '_' || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();
    let terms = cleaned.split_whitespace().collect::<Vec<_>>();
    if terms.is_empty() {
        "\"\"".to_string()
    } else {
        terms.join(" ")
    }
}

fn sanitize_fts_any_query(query: &str) -> Option<String> {
    let mut terms = relevance_terms(query).into_iter().collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms.truncate(8);
    if terms.len() < 2 {
        return None;
    }
    Some(terms.join(" OR "))
}

fn print_rows(conn: &Connection, rows: &[Memory], json: bool) -> Result<()> {
    if json {
        let full = rows
            .iter()
            .map(|m| get_memory_with_links(conn, &m.id))
            .collect::<Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string_pretty(&full)?);
        return Ok(());
    }
    for row in rows {
        println!(
            "{}",
            format_card(&MemoryWithLinks {
                memory: row.clone(),
                links: get_links(conn, &row.id)?,
            })
        );
    }
    Ok(())
}

fn format_card(row: &MemoryWithLinks) -> String {
    let memory = &row.memory;
    let mut out = format!(
        "{}  {}  {}  scope={}  confidence={:.2}\n{}\n  {}",
        memory.id,
        memory.memory_type,
        memory.status,
        memory.scope,
        memory.confidence,
        memory.title,
        memory.body
    );
    if let Some(source) = &memory.source {
        out.push_str(&format!("\n  source: {source}"));
    }
    if let Some(id) = &memory.supersedes {
        out.push_str(&format!("\n  supersedes: {id}"));
    }
    if let Some(id) = &memory.superseded_by {
        out.push_str(&format!("\n  superseded_by: {id}"));
    }
    for link in &row.links {
        out.push_str(&format!("\n  link:{}:{}", link.kind, link.target));
    }
    out.push('\n');
    out
}

fn remember_text(
    conn: &Connection,
    text: &str,
    memory_type: Option<MemoryType>,
    scope: &str,
    allow_sensitive: bool,
) -> Result<()> {
    validate_scope(scope)?;
    let inferred = suggest_from_text(text).into_iter().next();
    let kind = memory_type
        .map(|value| value.to_string())
        .or_else(|| inferred.as_ref().map(|s| s.memory_type.clone()))
        .unwrap_or_else(|| "note".to_string());
    let title = inferred
        .map(|s| s.title)
        .unwrap_or_else(|| truncate_words(text, 8));
    reject_sensitive(&title, text, allow_sensitive)?;
    let id = add_memory(
        conn,
        AddMemory {
            id: None,
            memory_type: kind,
            title,
            body: text.to_string(),
            scope: scope.to_string(),
            status: "active".to_string(),
            source: Some("remember".to_string()),
            supersedes: None,
            confidence: 0.8,
            links: Vec::new(),
        },
    )?;
    println!("{id}");
    Ok(())
}

fn forget_matching(conn: &Connection, query: &str, dry_run: bool) -> Result<()> {
    let rows = query_memories(
        conn,
        Some(query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        20,
    )?;
    if dry_run {
        for row in rows {
            println!("would_reject {} {}", row.id, row.title);
        }
        return Ok(());
    }
    let mut changed = 0;
    for row in rows {
        conn.execute(
            "UPDATE memories SET status = 'rejected', updated_at = ?1 WHERE id = ?2",
            params![now_ms(), row.id],
        )?;
        changed += 1;
    }
    println!("rejected: {changed}");
    Ok(())
}

fn print_snapshot(
    conn: &Connection,
    max_chars: usize,
    with_codegraph: bool,
    json_out: bool,
) -> Result<()> {
    let mut rows = Vec::new();
    for (kind, limit) in [
        ("product_goal", 5usize),
        ("constraint", 5),
        ("decision", 10),
        ("user_preference", 8),
        ("known_issue", 8),
        ("task_state", 8),
    ] {
        rows.extend(query_memories(
            conn,
            None,
            &[kind.to_string()],
            &["active".to_string(), "uncertain".to_string()],
            None,
            limit,
        )?);
    }
    rank_context_rows(&mut rows, "project snapshot", None, None);
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&compact_snapshot_rows(conn, &rows, max_chars)?)?
        );
        return Ok(());
    }
    let mut out = String::from("Project Snapshot\n");
    out.push_str(&render_context_pack(conn, &rows, max_chars)?);
    if with_codegraph {
        out.push_str(&render_codegraph_hints(
            &rows,
            "project snapshot",
            Path::new("."),
        ));
    }
    println!("{out}");
    Ok(())
}

#[derive(Serialize)]
struct SnapshotMemory {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    scope: String,
    title: String,
    summary: String,
    status: String,
    confidence: f64,
    links: Vec<MemoryLink>,
}

#[derive(Serialize)]
struct CompactContextMemory {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    scope: String,
    title: String,
    summary: String,
    status: String,
    confidence: f64,
    links: Vec<MemoryLink>,
}

fn compact_context_rows(
    conn: &Connection,
    rows: &[Memory],
    task: &str,
    max_chars: usize,
) -> Result<Vec<CompactContextMemory>> {
    let query_terms = relevance_terms(task);
    let summary_limit = if max_chars <= 1_200 {
        180
    } else if max_chars <= 3_000 {
        260
    } else {
        420
    };
    rows.iter()
        .map(|memory| {
            Ok(CompactContextMemory {
                id: memory.id.clone(),
                memory_type: memory.memory_type.clone(),
                scope: memory.scope.clone(),
                title: memory.title.clone(),
                summary: query_focused_summary(&memory.body, &query_terms, summary_limit),
                status: memory.status.clone(),
                confidence: memory.confidence,
                links: get_links(conn, &memory.id)?,
            })
        })
        .collect()
}

fn render_compact_context_rows_json(
    conn: &Connection,
    rows: &[Memory],
    task: &str,
    max_chars: usize,
) -> Result<(String, Vec<String>)> {
    let mut rendered_rows = Vec::new();
    let mut used_ids = Vec::new();
    for row in compact_context_rows(conn, rows, task, max_chars)? {
        let id = row.id.clone();
        rendered_rows.push(row);
        let rendered = serde_json::to_string_pretty(&rendered_rows)?;
        if rendered.len() > max_chars {
            rendered_rows.pop();
            break;
        }
        used_ids.push(id);
    }
    Ok((serde_json::to_string_pretty(&rendered_rows)?, used_ids))
}

fn compact_snapshot_rows(
    conn: &Connection,
    rows: &[Memory],
    max_chars: usize,
) -> Result<Vec<SnapshotMemory>> {
    let summary_limit = if max_chars <= 1_200 {
        180
    } else if max_chars <= 3_000 {
        260
    } else {
        420
    };
    let query_terms = HashSet::new();
    rows.iter()
        .map(|memory| {
            Ok(SnapshotMemory {
                id: memory.id.clone(),
                memory_type: memory.memory_type.clone(),
                scope: memory.scope.clone(),
                title: memory.title.clone(),
                summary: query_focused_summary(&memory.body, &query_terms, summary_limit),
                status: memory.status.clone(),
                confidence: memory.confidence,
                links: get_links(conn, &memory.id)?,
            })
        })
        .collect()
}

fn print_stats(conn: &Connection, db: &Path) -> Result<()> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    println!("database: {}", db.display());
    println!("total: {total}");
    println!("by type:");
    let mut stmt = conn
        .prepare("SELECT type, COUNT(*) AS n FROM memories GROUP BY type ORDER BY n DESC, type")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (kind, count) = row?;
        println!("  {kind}: {count}");
    }
    println!("by status:");
    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) AS n FROM memories GROUP BY status ORDER BY n DESC, status",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (status, count) = row?;
        println!("  {status}: {count}");
    }
    Ok(())
}

fn render_session_body(summary: &str, next: &[String]) -> String {
    if next.is_empty() {
        return summary.to_string();
    }
    let mut body = String::from(summary);
    body.push_str("\n\nNext steps:");
    for item in next {
        body.push_str("\n- ");
        body.push_str(item);
    }
    body
}

fn install_binary(to: &str, force: bool) -> Result<()> {
    let dest_dir = expand_tilde(to);
    fs::create_dir_all(&dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;
    let exe = std::env::current_exe()?;
    let dest = dest_dir.join("dukememory");
    if dest.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            dest.display()
        );
    }
    fs::copy(&exe, &dest)
        .with_context(|| format!("failed to copy {} to {}", exe.display(), dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
    }
    println!("{}", dest.display());
    match install_codex_skill(&expand_tilde("~/.codex/skills"), false) {
        Ok(()) => {}
        Err(err) => println!("skill_install_skipped: {err}"),
    }
    Ok(())
}

fn install_codex_skill(skills_root: &Path, force: bool) -> Result<()> {
    let skill_dir = write_codex_skill(skills_root, force)?;
    println!("{}", skill_dir.display());
    Ok(())
}

fn write_codex_skill(skills_root: &Path, force: bool) -> Result<PathBuf> {
    let skill_dir = skills_root.join("dukememory-use");
    let skill_file = skill_dir.join("SKILL.md");
    if skill_file.exists() && !force {
        return Ok(skill_dir);
    }
    fs::create_dir_all(skill_dir.join("agents"))
        .with_context(|| format!("failed to create {}", skill_dir.display()))?;
    write_file(&skill_file, DUKEMEMORY_SKILL_MD.as_bytes())?;
    write_file(
        &skill_dir.join("agents/openai.yaml"),
        DUKEMEMORY_SKILL_OPENAI_YAML.as_bytes(),
    )?;
    Ok(skill_dir)
}

const DUKEMEMORY_SKILL_MD: &str = r#"---
name: dukememory-use
description: Use local dukememory project memory automatically, safely, and token-lightly. Trigger in repositories with `.agent/memory.db` or `.agent/config.toml`, when dukememory/project memory is mentioned, when MCP tools memory_brief/memory_impact/memory_drift/memory_doctrine/memory_evidence/memory_remember are available, or when Codex needs to remember decisions, constraints, commands, preferences, task state, or project context across chats.
---

# dukememory. Use

Use the dukememory. memory layer as the first, smallest context layer for repositories with local memory.

## Invariants

- Confirm the current project root before reading or writing memory.
- Never write durable memory into another project's `.agent/memory.db`.
- Default to read-only memory use unless there is a durable fact worth saving.
- Keep context small: prefer `brief` and `impact` over broad retrieval.
- If MCP tools are absent but the `dukememory` CLI works, use CLI fallback immediately instead of skipping memory.
- Do not block work on embeddings; if semantic recall is unavailable, continue with FTS/local ranking.
- Memory maintenance is autonomous by default. Normal autonomous runs may refresh embeddings, backups, cleanup, high-confidence inbox items, operational compaction, and safe duplicate superseding without human prompting.
- Autonomous mode must be reversible: use `dukememory autonomous rollback` for the last cycle and avoid hard delete by default.

## Start Routine

For every coding task in a repository with `.agent/memory.db` or `.agent/config.toml`:

1. Load `memory_brief` with `{ "task": "<user task>", "budget": 1200 }`.
   Fallback: `dukememory brief "<user task>" --budget-profile tiny`.
2. If a file, symbol, subsystem, command, UI area, or error is named, load `memory_impact` with `{ "target": "<target>", "budget": 1200 }`.
   Fallback: `dukememory impact <target> --budget-profile tiny`.
3. Before broad edits, refactors, dependency changes, schema changes, release work, or cleanup, run `memory_drift` with `{ "root": "." }`.
   Fallback: `dukememory drift --root .`.

## Tool Availability

Use this order:

1. MCP tools from the `dukememory` server.
2. CLI `dukememory` from the current project root.
3. CLI `/Users/daniil/.local/bin/dukememory` from the current project root.

Do not say memory is unavailable while CLI fallback works.

If neither MCP nor CLI works, mention that project memory is installed but currently unreachable, then continue with normal repo inspection.

## Read Decision Table

| Situation | Use |
| --- | --- |
| New coding task | `memory_brief` |
| Specific file/symbol/subsystem | `memory_impact` |
| Architectural or policy question | `memory_doctrine`, then `memory_evidence` for critical ids |
| User asks why/where a fact came from | `memory_evidence` |
| Risky edit, cleanup, or migration | `memory_drift` |
| Brief/impact insufficient | `memory_search` or `dukememory retrieve --strategy hybrid --budget-profile tiny` |

## Write Decision Table

| Durable fact | Card type |
| --- | --- |
| Accepted technical/product choice | `decision` |
| Rule that must be followed | `constraint` |
| User preference | `user_preference` |
| Build/test/setup command | `command` |
| Risk, bug, caveat | `known_issue` |
| Current continuation state | `task_state` |
| Useful implementation note | `design_note` |

Prefer structured cards over generic notes:

```bash
dukememory add decision "Title" "Body" --link file:src/App.tsx
dukememory add constraint "Title" "Body"
dukememory add command "Build command" "npm run build" --link file:package.json
dukememory add known_issue "Title" "Body" --link file:src/App.tsx
```

Use `dukememory remember "Short durable project fact"` only for simple facts that do not need type-specific handling.

## Decision Hygiene

Before adding a `decision`, check `memory_doctrine` or `dukememory doctrine --json`.

If a new decision replaces an old one, use:

```bash
dukememory add decision "New title" "New body" --supersedes <old-memory-id>
```

Use `memory_evidence` or `dukememory evidence <memory-id>` before relying on high-impact or surprising memory.

## What Not To Save

Do not save transient scratch notes, large logs, secrets, credentials, full file dumps, obvious facts from nearby code, or noisy "I changed X" notes.

Do not save memory merely because a tool ran. Save the durable result, command, decision, or unresolved next action.

## End Routine

Before the final response after substantial work:

1. Save durable outcomes only if they will help a future chat.
2. Include file/symbol links when they make future `impact` useful.
3. Save validation commands that actually matter.
4. Run `dukememory embed-index` once after a batch of memory writes.
5. Include a short human-readable final receipt in the user's language. Example in English: `Memory: read brief+impact; matched 6 cards; saved task_state abc123.` Example in Russian: `Память: прочитал brief+impact по 6 карточкам; сохранил task_state abc123.` If nothing durable was saved, say that naturally in the user's language. Do not paste long raw id lists.
6. If no durable outcome exists, do not write memory.

Never assume the chat transcript is automatically durable memory. Write the small durable card explicitly when it matters.

One useful memory card is better than a transcript.

## Observability

Use `dukememory usage-report --since-days 7` to check whether agents are reading memory, which commands they use, whether semantic recall is active, how many unique memory cards are reused, and whether useful writes are happening.

Use `dukememory usefulness-report` to inspect hot, unused, stale, long, unlinked, missing-link, and duplicate memory before cleanup. Treat it as suggestions, not automatic deletion.

Use `dukememory autonomous status --json` to inspect the latest autonomous maintenance cycle, action count, rollback backup, and errors.

Use `dukememory quality-report --json` to inspect per-card quality, feedback, token-saving value, evidence links, and risk.

Use `dukememory roi-report --json` to inspect memory ROI, top reused cards, useful rate, and write pressure.

Use `dukememory agent-audit --json` to inspect whether agents start with brief/impact, use semantic recall, and write durable memory responsibly.

Use `dukememory decision-trace --json` to explain which recent memory reads influenced agent behavior and which cards were confirmed or questioned by feedback.

Use `dukememory auto-feedback --dry-run --json` to preview autonomous inferred feedback; use `dukememory auto-feedback --json` when safe to materialize useful/missing feedback events.

Use `dukememory cost-guard --json` to keep memory token-light and detect high budgets, high write pressure, noisy cards, or oversized cards.

Use `dukememory budget-plan "<task>" --json` when unsure how much memory context is enough. Prefer the returned smallest useful profile.

Use `dukememory project-profile --json` to inspect the project memory profile, embedding configuration, and recommended budget.

Use `dukememory recall "<task>" --max-chars 1200` when brief/impact is not enough but full context would waste tokens.

Use `dukememory eval live --json` to inspect whether memory reads are later judged useful, useless, or missing.

Use `dukememory dashboard --json` to inspect all discovered project memories and autonomous health.

Use `dukememory intelligence-dashboard --json` to inspect ROI, agent behavior, decision trace, auto-feedback status, cost guard, project diff, and remote sync dry-run in one compact report.

Use `dukememory project-diff --changed-only --json` to compare current project changes with memory links, stale facts, and duplicate decisions.

Use `dukememory remote-sync-dry-run --json` before using VDS/remote memory sync. Keep reads local-first unless measured latency is acceptable.

Use `dukememory doctor-project --json` to verify project memory DB, AGENTS block, Codex skill, embeddings, QA, and autonomous status.

Use `dukememory release-gate --json` before committing or publishing a release. In strict mode it also requires a clean worktree.

Use `dukememory sync export bundle.json --dry-run --json` before writing a local-first sync bundle; use `dukememory sync import bundle.json --dry-run --json` before applying it.

Use `dukememory inbox-v2 report --json` before processing pending inbox items; use `dukememory inbox-v2 auto-apply --dry-run --json` before allowing changes.

Use `dukememory policy-tune --json` to adapt autonomous policy thresholds from feedback, quality, and rollback history.

Use `dukememory memory-qa --json` to answer whether memory is actually useful, noisy, complete, semantically indexed, and autonomous-ready.

Use `dukememory memory-contract --write` after meaningful project changes to keep one compact project-wide contract current.

Use `dukememory upgrade-project --json` after a dukememory release to refresh binary, skill, AGENTS/rules, memory contract, and QA in one pass.

After a task, record lightweight feedback when memory was notably helpful, misleading, or missing:

```bash
dukememory feedback --id <memory-id> --rating useful --command brief --query "<task>"
```

## Health And Recovery

If memory behavior seems wrong:

```bash
dukememory build-info
dukememory quality-report --json
dukememory embed-status --json
dukememory memory-qa --json
dukememory intelligence-dashboard --json
dukememory cost-guard --json
dukememory doctor-project --json
dukememory release-gate --json
dukememory autonomous run-once --level normal --json
dukememory autonomous status --json
dukememory drift --root . --json
```

If an autonomous cycle made an unwanted change, run:

```bash
dukememory autonomous rollback --json
```

If MCP is missing but CLI works, continue with CLI fallback and mention that Codex may need restart to reload MCP servers.

If no `.agent` memory exists and the user wants project memory:

```bash
dukememory onboard --root . --install-autonomous
dukememory install-skill
dukememory memory-contract --write
```

Seed new project memory with project goal, build/test commands, main entrypoints, and the memory budget constraint.
"#;

const DUKEMEMORY_SKILL_OPENAI_YAML: &str = r#"interface:
  display_name: "dukememory."
  short_description: "Use project memory with discipline."
  default_prompt: "Use $dukememory-use to read the smallest useful memory, verify critical facts, and save only durable outcomes."

dependencies:
  tools:
    - type: "mcp"
      value: "dukememory"
      description: "Local dukememory. MCP server for project memory briefs, impact, drift, and writes."

policy:
  allow_implicit_invocation: true
"#;

fn print_update_install(
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    backup_keep: usize,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = update_install(from, to, backup_dir, backup_keep, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!(
        "update_install: {}",
        if report.changed { "updated" } else { "current" }
    );
    println!("source: {}", report.source);
    println!("target: {}", report.target);
    if let Some(version) = &report.previous_version {
        println!("previous_version: {version}");
    }
    if let Some(version) = &report.source_version {
        println!("source_version: {version}");
    }
    if let Some(backup) = &report.backup {
        println!("backup: {backup}");
    }
    if !report.pruned_backups.is_empty() {
        println!("pruned_backups: {}", report.pruned_backups.len());
    }
    if report.dry_run {
        println!("dry_run: true");
    }
    Ok(())
}

fn update_install(
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    backup_keep: usize,
    dry_run: bool,
) -> Result<InstallUpdateReport> {
    if backup_keep == 0 {
        bail!("--backup-keep must be at least 1");
    }
    let source = from
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_exe()?);
    if !source.is_file() {
        bail!("source binary not found: {}", source.display());
    }
    let target = resolve_install_target(to);
    if let (Ok(source_real), Ok(target_real)) = (source.canonicalize(), target.canonicalize())
        && source_real == target_real
    {
        bail!(
            "source and target are the same binary: {}",
            target.display()
        );
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let source_sha256 = sha256_path(&source)?;
    let source_version = binary_version(&source);
    let previous_sha256 = if target.exists() {
        Some(sha256_path(&target)?)
    } else {
        None
    };
    let previous_version = if target.exists() {
        binary_version(&target)
    } else {
        None
    };
    let changed = previous_sha256.as_deref() != Some(source_sha256.as_str());
    let mut backup = None;
    let mut pruned_backups = Vec::new();
    let mut kept_backups = Vec::new();

    if changed && !dry_run {
        let tmp = target.with_extension(format!("update-{}.tmp", now_ms()));
        copy_file(&source, &tmp)?;
        set_executable(&tmp)?;

        if target.exists() {
            fs::create_dir_all(backup_dir)
                .with_context(|| format!("failed to create {}", backup_dir.display()))?;
            let backup_path = backup_dir.join(format!(
                "{}-{}-{}.bak",
                binary_name(),
                backup_label(previous_version.as_deref().unwrap_or("unknown")),
                now_ms()
            ));
            copy_file(&target, &backup_path)?;
            backup = Some(backup_path.display().to_string());
            fs::remove_file(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
        }

        if let Err(err) = fs::rename(&tmp, &target) {
            if let Some(backup_path) = backup.as_deref()
                && !target.exists()
            {
                let _ = copy_file(Path::new(backup_path), &target);
                let _ = set_executable(&target);
            }
            let _ = fs::remove_file(&tmp);
            bail!(
                "failed to replace {} from {}: {err}",
                target.display(),
                source.display()
            );
        }
    }
    if !dry_run {
        let retention = prune_install_backups(backup_dir, backup_keep)?;
        pruned_backups = retention.pruned;
        kept_backups = retention.kept;
    } else if backup_dir.exists() {
        kept_backups = list_install_backups(backup_dir)?
            .into_iter()
            .rev()
            .take(backup_keep)
            .map(|item| item.path.display().to_string())
            .collect();
    }

    Ok(InstallUpdateReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        source: source.display().to_string(),
        target: target.display().to_string(),
        backup,
        dry_run,
        changed,
        previous_version,
        source_version,
        previous_sha256,
        source_sha256,
        backup_keep,
        pruned_backups,
        kept_backups,
    })
}

struct InstallBackupItem {
    path: PathBuf,
    modified: SystemTime,
}

struct InstallBackupRetention {
    kept: Vec<String>,
    pruned: Vec<String>,
}

fn prune_install_backups(backup_dir: &Path, keep: usize) -> Result<InstallBackupRetention> {
    let backups = list_install_backups(backup_dir)?;
    let kept = backups
        .iter()
        .rev()
        .take(keep)
        .map(|item| item.path.display().to_string())
        .collect::<Vec<_>>();
    let prune_paths = backups
        .into_iter()
        .rev()
        .skip(keep)
        .map(|item| item.path)
        .collect::<Vec<_>>();
    let mut pruned = Vec::new();
    for path in prune_paths {
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        pruned.push(path.display().to_string());
    }
    Ok(InstallBackupRetention { kept, pruned })
}

fn list_install_backups(backup_dir: &Path) -> Result<Vec<InstallBackupItem>> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(backup_dir)
        .with_context(|| format!("failed to read {}", backup_dir.display()))?
    {
        let path = entry?.path();
        if !path.is_file() || !is_install_backup_file(&path) {
            continue;
        }
        let modified = fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        backups.push(InstallBackupItem { path, modified });
    }
    backups.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(backups)
}

fn is_install_backup_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.starts_with(binary_name()) && name.ends_with(".bak")
}

fn resolve_install_target(to: &str) -> PathBuf {
    let path = expand_tilde(to);
    if path.exists() && path.is_dir() {
        path.join(binary_name())
    } else {
        path
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "dukememory.exe"
    } else {
        "dukememory"
    }
}

fn binary_version(path: &Path) -> Option<String> {
    let output = ProcessCommand::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().last().map(ToOwned::to_owned)
}

fn backup_label(value: &str) -> String {
    let mut out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

fn sha256_path(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn print_vec_status() {
    if cfg!(feature = "vec") {
        println!("sqlite-vec feature: enabled");
        println!("embedding providers: ollama, openai-compatible, mock");
    } else {
        println!("sqlite-vec feature: disabled");
        println!("build with: cargo build --features vec");
    }
    println!("default provider: {DEFAULT_EMBED_PROVIDER}");
    println!("default endpoint: {DEFAULT_EMBED_ENDPOINT}");
    println!("default model: {DEFAULT_EMBED_MODEL}");
    println!("commands: embed-index, embed-search, context-pack");
}

fn print_completions(shell: CompletionShell) {
    let _ = Cli::command();
    let commands = [
        "init",
        "add",
        "remember",
        "what-do-we-know",
        "what-next",
        "forget",
        "brief",
        "impact",
        "drift",
        "context",
        "context-pack",
        "snapshot",
        "doctor",
        "policy-check",
        "policy-apply",
        "search",
        "list",
        "get",
        "update",
        "delete",
        "review",
        "stale",
        "conflicts",
        "links",
        "install",
        "install-skill",
        "update-install",
        "ingest-transcript",
        "auto-ingest",
        "inbox-list",
        "inbox-approve",
        "inbox-reject",
        "embed-index",
        "embed-search",
        "embed-status",
        "embed-watch",
        "provider-list",
        "vector-bench",
        "serve-mcp",
        "completions",
        "man",
        "audit",
        "usage-report",
        "usefulness-report",
        "quality-report",
        "roi-report",
        "agent-audit",
        "decision-trace",
        "auto-feedback",
        "cost-guard",
        "feedback",
        "budget-plan",
        "project-profile",
        "recall",
        "onboard",
        "dashboard",
        "dashboard-repair",
        "dashboard-repair-history",
        "ops-status",
        "remote-status",
        "project-diff",
        "intelligence-dashboard",
        "remote-sync-dry-run",
        "doctor-project",
        "release-gate",
        "inbox-v2",
        "policy-tune",
        "memory-qa",
        "memory-contract",
        "upgrade-project",
        "codex-doctor",
        "workspace-init",
        "bundle",
        "doctrine",
        "evidence",
        "schema",
        "lock",
        "retrieve",
        "eval",
        "compact-v2",
        "build-info",
        "release-bundle",
        "bench",
        "self-host",
        "health",
        "backup-policy",
        "backup-verify",
        "cleanup",
        "autonomous",
        "daemon-install",
        "integrity",
        "optimize",
    ];
    match shell {
        CompletionShell::Bash => {
            println!("_dukememory() {{");
            println!(
                "  COMPREPLY=( $(compgen -W \"{}\" -- \"${{COMP_WORDS[COMP_CWORD]}}\") )",
                commands.join(" ")
            );
            println!("}}");
            println!("complete -F _dukememory dukememory");
        }
        CompletionShell::Zsh => {
            println!("#compdef dukememory");
            println!("_arguments '1:command:({})'", commands.join(" "));
        }
        CompletionShell::Fish => {
            for command in commands {
                println!("complete -c dukememory -f -a {command}");
            }
        }
    }
}

fn print_manpage() {
    println!("dukememory(1)");
    println!("NAME");
    println!("  dukememory - local structured memory for agent-driven projects");
    println!("SYNOPSIS");
    println!("  dukememory <command> [options]");
    println!("AGENT-NATIVE COMMANDS");
    println!("  remember TEXT                 store durable memory");
    println!("  what-do-we-know QUERY         search memory");
    println!("  what-next                     print current next actions");
    println!("  brief TASK                    tiny verified task brief");
    println!("  impact TARGET                 linked decisions/risks for file or symbol");
    println!("  drift --changed-only          cheap local memory drift check");
    println!("  context TASK --mode agent     return planned agent context");
    println!("  context TASK --budget-profile tiny|normal|deep");
    println!("  retrieve QUERY --strategy hybrid --budget-profile tiny");
    println!("  snapshot                      compact project state");
    println!("  doctor                        health checks");
    println!("EMBEDDINGS");
    println!("  embed-index                   incremental indexing");
    println!("  embed-status                  freshness report");
    println!("  embed-watch --once            one incremental pass");
    println!("TRANSCRIPTS");
    println!("  ingest-transcript FILE --llm  extract inbox suggestions via local Ollama");
    println!("  auto-ingest --input DIR       scan session files into inbox without duplicates");
    println!("DECISIONS");
    println!("  doctrine                      print active decision doctrine");
    println!("  evidence ID                   show provenance for one memory card");
    println!("MCP");
    println!("  serve-mcp                     newline JSON-RPC MCP-style server");
    println!("  serve-mcp --content-length    framed MCP transport");
    println!("POLICY");
    println!("  policy-check FILE             validate Rhai policy hooks");
    println!("  policy-apply FILE --dry-run   preview policy actions");
    println!("OPS");
    println!("  audit                         print mutation events");
    println!("  usage-report --since-days 7   show memory reads, writes, and reuse");
    println!("  usefulness-report             show hot/unused/stale memory suggestions");
    println!("  quality-report --json         score memory usefulness and token value");
    println!("  roi-report --json             estimate memory ROI and write pressure");
    println!("  agent-audit --json            audit agent memory behavior");
    println!("  decision-trace --json         explain recent memory influence");
    println!("  auto-feedback --dry-run       infer feedback from recent reads");
    println!("  cost-guard --json             protect memory token budget");
    println!("  feedback --id ID --rating useful|useless|missing");
    println!("  budget-plan TASK --json       choose smallest useful memory budget");
    println!("  project-profile --json        structured project memory profile");
    println!("  recall QUERY --max-chars 1200 compressed token-light recall");
    println!("  dashboard --json              multi-project memory health dashboard");
    println!("  dashboard-repair --apply      run safe dashboard repair actions");
    println!("  dashboard-repair-history      summarize safe repair audit history");
    println!("  ops-status --json             one UI/autonomy/effectiveness/sync status");
    println!("  remote-status --json          local-first remote/VDS readiness");
    println!("  project-diff --changed-only   diff project changes against memory");
    println!("  intelligence-dashboard --json aggregate memory intelligence");
    println!("  remote-sync-dry-run --json    simulate VDS sync without moving data");
    println!("  doctor-project --json         verify project memory installation");
    println!("  release-gate --json           aggregate local release readiness");
    println!("  onboard --root DIR            initialize memory/profile/embeddings");
    println!("  inbox-v2 report|auto-apply    group and process pending suggestions");
    println!("  policy-tune --json            tune autonomous policy from feedback");
    println!("  memory-qa --json              score memory usefulness, noise, and health");
    println!("  memory-contract --write       write compact project memory contract");
    println!("  upgrade-project --json        refresh binary/skill/rules/contract/QA");
    println!("  codex-doctor                  check Codex MCP dukememory wiring");
    println!("  workspace-init                create .agent/rules.rhai");
    println!("  bundle out.json --redact      diagnostics + export bundle");
    println!("  release-bundle DIR            create release manifest, binary, and config");
    println!("  install --to DIR              copy current binary into install dir");
    println!("  install-skill                 install Codex dukememory skill");
    println!("  update-install --from BIN     update installed binary with backup");
    println!("  bench --json                  benchmark local memory operations");
    println!("  self-host --force             seed durable memory about this system");
    println!("  health --json                 check permanent-use readiness");
    println!("  backup-policy --keep 10       create and rotate database backups");
    println!("  backup-verify BACKUP --json   verify backup integrity/checksum");
    println!("  cleanup --dry-run             preview operational retention cleanup");
    println!("  autonomous run-once           autonomous reversible memory maintenance");
    println!("  daemon-install                write macOS launchd plist");
    println!("  integrity --json              run SQLite integrity checks");
    println!("  optimize --vacuum --json      optimize SQLite/FTS storage");
    println!("V9");
    println!("  schema status|verify|upgrade  schema migrations");
    println!("  lock status|clear             local lock management");
    println!("  retrieve QUERY --strategy hybrid --format agent");
    println!("  eval add-case NAME QUERY EXPECTED");
    println!("  eval run");
    println!("  compact-v2 --dry-run");
    println!("  build-info");
}

fn print_build_info(runtime: &crate::runtime_config::RuntimeConfig) {
    let info = BuildInfo::current(CURRENT_SCHEMA_VERSION);
    println!("version: {}", info.version);
    println!("schema: {}", info.schema);
    println!("vec_feature: {}", info.vec_feature);
    println!("target: {}", info.os);
    println!("arch: {}", info.arch);
    println!("config: {}", runtime.config_path.display());
    println!("embed_provider: {}", runtime.config.embeddings.provider);
    println!("embed_endpoint: {}", runtime.config.embeddings.endpoint);
    println!("embed_model: {}", runtime.config.embeddings.model);
}

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
const CURRENT_SCHEMA_VERSION: i64 = 14;
const EXPORT_VERSION: u32 = 1;
const VALID_SCOPES: &[&str] = &["global", "user", "project", "repo", "thread", "task"];

mod autonomous;
mod cli;
mod db;
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
use autonomous::*;
use cli::*;
use db::*;
use maintenance::*;
use memory::*;
use model::*;
use observability::*;
use project::*;
use retrieval::*;

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
            json,
        } => {
            let rows = query_memories(
                &conn,
                Some(&query),
                &split_csv(memory_type.as_deref()),
                &split_csv(Some(&status)),
                scope.as_deref(),
                limit,
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
            semantic,
            embed_provider,
            embed_endpoint,
            embed_model,
        } => {
            let types = split_csv(memory_type.as_deref());
            let statuses = split_csv(Some(&status));
            let mut rows = build_context_rows(
                &conn,
                ContextQuery {
                    task: &task,
                    types: &types,
                    statuses: &statuses,
                    scope: scope.as_deref(),
                    limit,
                    include_recent,
                    rules: rules.as_deref(),
                },
            )?;
            if semantic
                && embeddings::semantic_index_ready(
                    &conn,
                    &embed_provider,
                    &embed_endpoint,
                    &embed_model,
                )
                .unwrap_or(false)
            {
                let semantic_rows = embeddings::semantic_search(
                    &conn,
                    &embed_provider,
                    &embed_endpoint,
                    &embed_model,
                    &task,
                    limit,
                )?;
                for item in semantic_rows {
                    if !rows
                        .iter()
                        .any(|existing| existing.id == item.memory.memory.id)
                    {
                        rows.push(item.memory.memory);
                    }
                }
                rows.truncate(limit);
            }
            let max_chars = budget_profile_chars(budget_profile).unwrap_or(max_chars);
            if json {
                let full = rows
                    .iter()
                    .map(|m| get_memory_with_links(&conn, &m.id))
                    .collect::<Result<Vec<_>>>()?;
                println!("{}", serde_json::to_string_pretty(&full)?);
            } else {
                let mut rendered = render_context_pack(&conn, &rows, max_chars)?;
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
            },
        )?,
        Command::Impact {
            target,
            limit,
            budget,
            budget_profile,
            scope,
            json,
        } => print_impact(
            &conn,
            ImpactRequest {
                target: &target,
                limit,
                budget: budget
                    .unwrap_or_else(|| budget_profile_chars(Some(budget_profile)).unwrap_or(1200)),
                scope: scope.as_deref(),
                json_out: json,
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
            dry_run,
            json,
        } => print_update_install(from.as_deref(), &to, &backup_dir, dry_run, json)?,
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

fn import_memories(conn: &Connection, input: &Path, replace: bool) -> Result<()> {
    let raw =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;
    let export: MemoryExport = serde_json::from_str(&raw)?;
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
    println!("imported: {count}");
    Ok(())
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

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn transactional(conn: &Connection, label: &str, f: impl FnOnce() -> Result<()>) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    match f() {
        Ok(()) => {
            conn.execute_batch("COMMIT;")?;
            Ok(())
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(err).with_context(|| format!("transaction failed: {label}"))
        }
    }
}

fn log_event(
    conn: &Connection,
    event_type: &str,
    memory_id: Option<&str>,
    detail: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_events (event_type, memory_id, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![event_type, memory_id, detail, now_ms()],
    )?;
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveEvalReport {
    version: u32,
    since_days: i64,
    reads: usize,
    feedback_events: usize,
    useful: usize,
    useless: usize,
    missing: usize,
    useful_rate: f64,
    noisy_memory_ids: Vec<String>,
    missing_queries: Vec<String>,
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

#[derive(Debug, Serialize)]
struct DoctrineReport {
    active: Vec<DoctrineDecision>,
    superseded: Vec<DoctrineDecision>,
    conflicts: Vec<MergeCandidate>,
}

#[derive(Debug, Serialize)]
struct DoctrineDecision {
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

fn print_doctrine(conn: &Connection, scope: Option<&str>, json_out: bool) -> Result<()> {
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

fn doctrine_report(conn: &Connection, scope: Option<&str>) -> Result<DoctrineReport> {
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

struct BriefRequest<'a> {
    task: &'a str,
    limit: usize,
    budget: usize,
    scope: Option<&'a str>,
    rules: Option<&'a Path>,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    json_out: bool,
}

fn print_brief(conn: &Connection, request: BriefRequest<'_>) -> Result<()> {
    let report = brief_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_brief(&report));
    }
    Ok(())
}

fn brief_report(conn: &Connection, request: &BriefRequest<'_>) -> Result<BriefReport> {
    let started = Instant::now();
    let retrieval = retrieve_report(
        conn,
        &RetrieveRequest {
            query: request.task,
            strategy: RetrievalStrategy::Hybrid,
            format: OutputFormat::Plain,
            limit: request.limit.max(6),
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

    for hit in &retrieval.hits {
        let memory = &hit.memory.memory;
        let item = brief_item_from_hit(hit);
        match memory.memory_type.as_str() {
            "decision" | "constraint" | "product_goal" => {
                push_unique_brief_item(&mut must_follow, &mut seen_items, item, 5);
            }
            "known_issue" => {
                push_unique_brief_item(&mut risks, &mut seen_items, item, 3);
            }
            "command" => {
                push_unique_check(&mut checks, &mut seen_checks, &memory.body, 5);
                push_unique_brief_item(&mut relevant, &mut seen_items, item, 5);
            }
            _ => {
                push_unique_brief_item(&mut relevant, &mut seen_items, item, 5);
            }
        }
        for link in &hit.memory.links {
            if matches!(link.kind.as_str(), "file" | "symbol") {
                let rendered = format!("{}:{}", link.kind, link.target);
                if files.len() < 8 && seen_files.insert(rendered.clone()) {
                    files.push(rendered);
                }
            }
        }
        collect_check_hints(&mut checks, &mut seen_checks, &memory.body, 5);
    }

    let mut ids = must_follow
        .iter()
        .chain(relevant.iter())
        .chain(risks.iter())
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let receipt = memory_receipt("brief", Some(retrieval.semantic_used), &ids, "none");
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
        semantic_error: retrieval.semantic_error,
        receipt,
        must_follow,
        relevant,
        risks,
        files,
        checks,
    })
}

fn brief_item_from_hit(hit: &RetrievalHit) -> BriefItem {
    let memory = &hit.memory.memory;
    brief_item_from_memory(
        memory,
        hit.score,
        hit.reasons.iter().take(4).cloned().collect(),
    )
}

fn brief_item_from_memory(memory: &Memory, score: f64, reasons: Vec<String>) -> BriefItem {
    BriefItem {
        id: memory.id.clone(),
        memory_type: memory.memory_type.clone(),
        title: memory.title.clone(),
        summary: truncate_chars(&one_line_summary(&memory.body), 180),
        score,
        reasons,
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
    render_brief_items(&mut out, report.budget, "Must Follow", &report.must_follow);
    render_brief_items(&mut out, report.budget, "Relevant", &report.relevant);
    render_brief_items(&mut out, report.budget, "Risks", &report.risks);
    render_brief_strings(&mut out, report.budget, "Files", &report.files);
    render_brief_strings(&mut out, report.budget, "Checks", &report.checks);
    truncate_chars(&out, report.budget)
}

struct ImpactRequest<'a> {
    target: &'a str,
    limit: usize,
    budget: usize,
    scope: Option<&'a str>,
    json_out: bool,
}

fn print_impact(conn: &Connection, request: ImpactRequest<'_>) -> Result<()> {
    let report = impact_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_impact(&report));
    }
    Ok(())
}

fn impact_report(conn: &Connection, request: &ImpactRequest<'_>) -> Result<ImpactReport> {
    let started = Instant::now();
    let mut rows = linked_memories(conn, request.target, request.scope, request.limit)?;
    let fts_rows = query_memories(
        conn,
        Some(request.target),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        request.scope,
        request.limit,
    )?;
    for row in fts_rows {
        if !rows.iter().any(|existing| existing.id == row.id) {
            rows.push(row);
        }
    }
    rows.truncate(request.limit.max(1));

    let mut decisions = Vec::new();
    let mut constraints = Vec::new();
    let mut risks = Vec::new();
    let mut checks = Vec::new();
    let mut related = Vec::new();
    let mut links = Vec::new();
    let mut seen_items = HashSet::new();
    let mut seen_checks = HashSet::new();
    let mut seen_links = HashSet::new();

    for (index, memory) in rows.iter().enumerate() {
        let reason = if memory_links_target(conn, &memory.id, request.target)? {
            "linked target"
        } else {
            "fts match"
        };
        let score = 100.0 - index as f64;
        let item = brief_item_from_memory(memory, score, vec![reason.to_string()]);
        match memory.memory_type.as_str() {
            "decision" | "product_goal" => {
                push_unique_brief_item(&mut decisions, &mut seen_items, item, 5);
            }
            "constraint" => {
                push_unique_brief_item(&mut constraints, &mut seen_items, item, 5);
            }
            "known_issue" => {
                push_unique_brief_item(&mut risks, &mut seen_items, item, 5);
            }
            "command" => {
                push_unique_check(&mut checks, &mut seen_checks, &memory.body, 6);
                push_unique_brief_item(&mut related, &mut seen_items, item, 6);
            }
            _ => {
                push_unique_brief_item(&mut related, &mut seen_items, item, 6);
            }
        }
        collect_check_hints(&mut checks, &mut seen_checks, &memory.body, 6);

        for link in get_links(conn, &memory.id)? {
            if matches!(link.kind.as_str(), "file" | "symbol") {
                let rendered = format!("{}:{}", link.kind, link.target);
                if links.len() < 10 && seen_links.insert(rendered.clone()) {
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
    let receipt = memory_receipt("impact", None, &ids, "none");
    log_read_event(
        conn,
        ReadEventInput {
            command: "impact",
            query: request.target,
            ids: &ids,
            semantic_used: false,
            result_count: ids.len(),
            budget: request.budget,
            elapsed_ms: started.elapsed().as_millis(),
        },
    )?;

    Ok(ImpactReport {
        version: 1,
        target: request.target.to_string(),
        budget: request.budget,
        receipt,
        decisions,
        constraints,
        risks,
        checks,
        related,
        links,
    })
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

fn print_evidence(conn: &Connection, id: &str, json_out: bool) -> Result<()> {
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

fn evidence_report(conn: &Connection, id: &str) -> Result<EvidenceReport> {
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

fn print_drift(conn: &Connection, root: &Path, changed_only: bool, json_out: bool) -> Result<()> {
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

fn drift_report(conn: &Connection, root: &Path, changed_only: bool) -> Result<DriftReport> {
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
    let stale_active = stale_active_memories(conn, 10)?
        .into_iter()
        .enumerate()
        .map(|(index, memory)| {
            brief_item_from_memory(
                &memory,
                100.0 - index as f64,
                vec!["active superseded_by".into()],
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
        SyncCommand::Export { output, redact } => {
            let mut export = export_memories(conn, &[], &[], None)?;
            if redact {
                redact_export(&mut export)?;
            }
            write_file(&output, serde_json::to_string_pretty(&export)?.as_bytes())?;
            println!("{}", output.display());
            Ok(())
        }
        SyncCommand::Import { input, replace } => import_memories(conn, &input, replace),
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

fn handle_eval(conn: &Connection, command: EvalCommand) -> Result<()> {
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

#[derive(Debug, Serialize)]
struct EvalResult {
    id: String,
    name: String,
    passed: bool,
    detail: String,
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
        println!("useful_rate: {:.1}%", report.useful_rate * 100.0);
        println!("noisy_memory_ids: {}", report.noisy_memory_ids.join(","));
    }
    Ok(())
}

fn live_eval_report(conn: &Connection, since_days: i64) -> Result<LiveEvalReport> {
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
    Ok(LiveEvalReport {
        version: 1,
        since_days,
        reads: reads.len(),
        feedback_events: total_feedback,
        useful,
        useless,
        missing,
        useful_rate: if total_feedback == 0 {
            0.0
        } else {
            useful as f64 / total_feedback as f64
        },
        noisy_memory_ids: noisy,
        missing_queries,
    })
}

fn validate_scope(scope: &str) -> Result<()> {
    if VALID_SCOPES.contains(&scope) {
        Ok(())
    } else {
        bail!(
            "invalid scope: {scope}. Expected one of: {}",
            VALID_SCOPES.join(", ")
        )
    }
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

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(str::trim)
        .filter(|part| part.len() > 2)
        .map(|part| part.to_lowercase())
        .collect()
}

fn reject_sensitive(title: &str, body: &str, allow_sensitive: bool) -> Result<()> {
    if allow_sensitive {
        return Ok(());
    }
    let text = format!("{title}\n{body}").to_lowercase();
    let suspicious_keys = [
        "api_key",
        "apikey",
        "secret",
        "password",
        "passwd",
        "token",
        "private_key",
        "access_key",
    ];
    if suspicious_keys
        .iter()
        .any(|key| text.contains(key) && (text.contains('=') || text.contains(':')))
    {
        bail!(
            "memory looks like it may contain a secret; use --allow-sensitive to store it intentionally"
        );
    }
    if text.contains("sk-") || text.contains("-----begin private key-----") {
        bail!(
            "memory looks like it may contain a secret; use --allow-sensitive to store it intentionally"
        );
    }
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

#[derive(Debug, Serialize)]
struct SecretFinding {
    id: String,
    title: String,
    pattern: String,
}

fn print_secret_scan(conn: &Connection, fix_redact: bool, json_out: bool) -> Result<()> {
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

fn redact_export(export: &mut MemoryExport) -> Result<()> {
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

fn redact_sensitive_text(text: &str) -> Result<String> {
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

fn scan_secret_findings(conn: &Connection) -> Result<Vec<SecretFinding>> {
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

fn format_compact_card(conn: &Connection, row: &Memory) -> Result<String> {
    let body = row.body.split_whitespace().collect::<Vec<_>>().join(" ");
    let links = get_links(conn, &row.id)?;
    let link_text = if links.is_empty() {
        String::new()
    } else {
        let rendered = links
            .iter()
            .map(|link| format!("{}:{}", link.kind, link.target))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ({rendered})")
    };
    Ok(format!(
        "- {}:{} [{}] {} -- {}{}",
        row.memory_type, row.status, row.scope, row.title, body, link_text
    ))
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

#[derive(Debug, Serialize)]
struct DoctorFinding {
    kind: String,
    status: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct CodexDoctorReport {
    ok: bool,
    findings: Vec<DoctorFinding>,
}

fn print_doctor(
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

fn print_codex_doctor(config: &Path, json_out: bool) -> Result<()> {
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

fn codex_doctor_report(config: &Path) -> Result<CodexDoctorReport> {
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
        let full = rows
            .iter()
            .map(|m| get_memory_with_links(conn, &m.id))
            .collect::<Result<Vec<_>>>()?;
        println!("{}", serde_json::to_string_pretty(&full)?);
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

#[derive(Debug, Serialize)]
struct ReviewIssue {
    kind: String,
    id: String,
    title: String,
    detail: String,
}

fn print_review(conn: &Connection, stale_days: i64, as_json: bool) -> Result<()> {
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

fn print_stale(conn: &Connection, days: i64, as_json: bool) -> Result<()> {
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

fn print_conflicts(conn: &Connection, as_json: bool) -> Result<()> {
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

fn review_stale(conn: &Connection, days: i64) -> Result<Vec<ReviewIssue>> {
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

fn review_uncertain(conn: &Connection) -> Result<Vec<ReviewIssue>> {
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

fn review_low_confidence(conn: &Connection) -> Result<Vec<ReviewIssue>> {
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

fn review_duplicates(conn: &Connection) -> Result<Vec<ReviewIssue>> {
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

fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_link_report(
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

fn link_report(
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

fn find_nearest_codegraph_root(start: &Path) -> Option<PathBuf> {
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
5. Include a short final receipt: `Memory: used <brief/impact/...>, ids=[...], wrote=<id|none>`. Do not omit it when memory was used.
6. If no durable outcome exists, do not write memory.

Never assume the chat transcript is automatically durable memory. Write the small durable card explicitly when it matters.

One useful memory card is better than a transcript.

## Observability

Use `dukememory usage-report --since-days 7` to check whether agents are reading memory, which commands they use, whether semantic recall is active, how many unique memory cards are reused, and whether useful writes are happening.

Use `dukememory usefulness-report` to inspect hot, unused, stale, long, unlinked, missing-link, and duplicate memory before cleanup. Treat it as suggestions, not automatic deletion.

Use `dukememory autonomous status --json` to inspect the latest autonomous maintenance cycle, action count, rollback backup, and errors.

Use `dukememory quality-report --json` to inspect per-card quality, feedback, token-saving value, evidence links, and risk.

Use `dukememory budget-plan "<task>" --json` when unsure how much memory context is enough. Prefer the returned smallest useful profile.

Use `dukememory project-profile --json` to inspect the project memory profile, embedding configuration, and recommended budget.

Use `dukememory recall "<task>" --max-chars 1200` when brief/impact is not enough but full context would waste tokens.

Use `dukememory eval live --json` to inspect whether memory reads are later judged useful, useless, or missing.

Use `dukememory dashboard --json` to inspect all discovered project memories and autonomous health.

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
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = update_install(from, to, backup_dir, dry_run)?;
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
    if report.dry_run {
        println!("dry_run: true");
    }
    Ok(())
}

fn update_install(
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
) -> Result<InstallUpdateReport> {
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
    })
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
    println!("commands: embed-index, embed-search, context-pack --semantic");
}

fn print_review_tui(conn: &Connection, stale_days: i64) -> Result<()> {
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
        "feedback",
        "budget-plan",
        "project-profile",
        "recall",
        "onboard",
        "dashboard",
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
    println!("  feedback --id ID --rating useful|useless|missing");
    println!("  budget-plan TASK --json       choose smallest useful memory budget");
    println!("  project-profile --json        structured project memory profile");
    println!("  recall QUERY --max-chars 1200 compressed token-light recall");
    println!("  dashboard --json              multi-project memory health dashboard");
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_millis() as i64
}

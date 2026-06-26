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

mod cli;
mod db;
mod embeddings;
mod http_server;
mod mcp_server;
mod memory;
mod model;
mod ops;
mod release_ops;
use cli::*;
use db::*;
use memory::*;
use model::*;

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

struct ContextQuery<'a> {
    task: &'a str,
    types: &'a [String],
    statuses: &'a [String],
    scope: Option<&'a str>,
    limit: usize,
    include_recent: usize,
    rules: Option<&'a Path>,
}

fn build_context_rows(conn: &Connection, query: ContextQuery<'_>) -> Result<Vec<Memory>> {
    let mut rows = query_memories(
        conn,
        Some(query.task),
        query.types,
        query.statuses,
        query.scope,
        query.limit,
    )?;
    if query.include_recent > 0 {
        let recent = query_memories(
            conn,
            None,
            query.types,
            &["active".to_string()],
            query.scope,
            query.include_recent,
        )?;
        for row in recent {
            if !rows.iter().any(|existing| existing.id == row.id) {
                rows.push(row);
            }
        }
    }
    rank_context_rows(&mut rows, query.task, query.scope, query.rules);
    rows.truncate(query.limit);
    Ok(rows)
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

#[derive(Debug, Serialize)]
struct MemoryReadEvent {
    id: i64,
    command: String,
    query: String,
    memory_ids: Vec<String>,
    semantic_used: bool,
    result_count: usize,
    budget: usize,
    elapsed_ms: u128,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct UsageReport {
    since_days: i64,
    read_count: usize,
    write_count: usize,
    semantic_read_count: usize,
    fallback_read_count: usize,
    unique_memory_ids: usize,
    reads_by_command: BTreeMap<String, usize>,
    writes_by_type: BTreeMap<String, usize>,
    recent_reads: Vec<MemoryReadEvent>,
}

#[derive(Debug, Serialize)]
struct UsefulnessReport {
    since_days: i64,
    stale_days: i64,
    hot_threshold: usize,
    total_active: usize,
    hot: Vec<UsefulnessItem>,
    unused: Vec<UsefulnessItem>,
    stale: Vec<UsefulnessItem>,
    too_long: Vec<UsefulnessItem>,
    no_links: Vec<UsefulnessItem>,
    missing_links: Vec<LinkReport>,
    duplicate_candidates: Vec<MergeCandidate>,
    suggestions: Vec<UsefulnessSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
struct UsefulnessItem {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    title: String,
    request_count: usize,
    updated_at: i64,
    body_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsefulnessSuggestion {
    action: String,
    id: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryQuality {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    title: String,
    score: f64,
    usefulness_score: f64,
    token_saving_score: f64,
    risk_score: f64,
    request_count: usize,
    positive_feedback: usize,
    negative_feedback: usize,
    body_chars: usize,
    links: usize,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QualityReport {
    version: u32,
    since_days: i64,
    total: usize,
    average_score: f64,
    strongest: Vec<MemoryQuality>,
    weakest: Vec<MemoryQuality>,
    items: Vec<MemoryQuality>,
    suggestions: Vec<UsefulnessSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeedbackSummary {
    since_days: i64,
    positive: usize,
    negative: usize,
    missing: usize,
    events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeedbackReport {
    ok: bool,
    rating: String,
    ids: Vec<String>,
    written_event: String,
    summary: FeedbackSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BudgetPlan {
    task: String,
    profile: String,
    max_chars: usize,
    include_recent: usize,
    semantic: bool,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectProfileSnapshot {
    root: String,
    active_profile: Option<String>,
    memory_count: usize,
    pending_inbox: usize,
    decisions: usize,
    constraints: usize,
    commands: usize,
    known_issues: usize,
    embedding_provider: String,
    embedding_endpoint: String,
    embedding_model: String,
    recommended_budget: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecallReport {
    query: String,
    max_chars: usize,
    token_saving_estimate: usize,
    receipt: String,
    items: Vec<RecallItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecallItem {
    id: String,
    #[serde(rename = "type")]
    memory_type: String,
    title: String,
    summary: String,
    score: f64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnboardReport {
    ok: bool,
    root: String,
    db: String,
    actions: Vec<String>,
    profile: ProjectProfileSnapshot,
    embedding: Option<EmbeddingIndexReport>,
    autonomous_plist: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DashboardReport {
    version: u32,
    projects: Vec<ProjectDashboardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectDashboardItem {
    name: String,
    root: String,
    db: String,
    memories: i64,
    pending_inbox: i64,
    quality_average: Option<f64>,
    autonomous_ok: Option<bool>,
    embedding_missing: Option<usize>,
    recommended_budget: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InboxV2Report {
    version: u32,
    pending: usize,
    groups: Vec<InboxV2Group>,
    actions: Vec<AutonomousAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InboxV2Group {
    key: String,
    count: usize,
    ids: Vec<String>,
    title: String,
    memory_type: String,
    max_confidence: f64,
    recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyTuneReport {
    version: u32,
    level: String,
    risk_limit: f64,
    approve_threshold: f64,
    duplicate_limit: usize,
    reasons: Vec<String>,
    output: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryQaReport {
    version: u32,
    ok: bool,
    score: f64,
    root: String,
    since_days: i64,
    reads: usize,
    semantic_read_rate: f64,
    useful_rate: f64,
    quality_average: f64,
    active_memories: usize,
    unused: usize,
    stale: usize,
    too_long: usize,
    duplicate_candidates: usize,
    embedding_missing: usize,
    embedding_stale: usize,
    autonomous_ok: Option<bool>,
    token_saving_estimate: usize,
    issues: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryContractReport {
    version: u32,
    root: String,
    path: String,
    written: bool,
    memory_id: Option<String>,
    max_chars: usize,
    content: String,
}

#[derive(Debug, Serialize)]
struct UpgradeProjectReport {
    version: String,
    ok: bool,
    root: String,
    dry_run: bool,
    actions: Vec<String>,
    install: Option<InstallUpdateReport>,
    qa: Option<MemoryQaReport>,
    contract: Option<MemoryContractReport>,
    errors: Vec<String>,
}

struct ReadEventInput<'a> {
    command: &'a str,
    query: &'a str,
    ids: &'a [String],
    semantic_used: bool,
    result_count: usize,
    budget: usize,
    elapsed_ms: u128,
}

fn memory_receipt(
    command: &str,
    semantic_used: Option<bool>,
    ids: &[String],
    wrote: &str,
) -> String {
    let semantic = semantic_used
        .map(|used| format!(", semantic={}", if used { "used" } else { "fallback" }))
        .unwrap_or_default();
    let rendered_ids = if ids.is_empty() {
        "-".to_string()
    } else {
        ids.iter().take(8).cloned().collect::<Vec<_>>().join(",")
    };
    format!("Memory: used {command}{semantic}, ids=[{rendered_ids}], wrote={wrote}")
}

fn log_read_event(conn: &Connection, input: ReadEventInput<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            input.command,
            truncate_chars(input.query, 500),
            input.ids.join(","),
            if input.semantic_used { 1 } else { 0 },
            input.result_count.min(i64::MAX as usize) as i64,
            input.budget.min(i64::MAX as usize) as i64,
            input.elapsed_ms.min(i64::MAX as u128) as i64,
            now_ms()
        ],
    )?;
    Ok(())
}

fn print_audit(conn: &Connection, limit: usize, json_out: bool) -> Result<()> {
    let events = audit_events(conn, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else if events.is_empty() {
        println!("audit: none");
    } else {
        for event in events {
            let memory_id = event.memory_id.unwrap_or_else(|| "-".to_string());
            println!(
                "{}  {}  {}  {}",
                event.id, event.event_type, memory_id, event.detail
            );
        }
    }
    Ok(())
}

fn print_usage_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = usage_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Usage Report");
    println!("since_days: {}", report.since_days);
    println!("reads: {}", report.read_count);
    println!("writes: {}", report.write_count);
    println!("semantic_reads: {}", report.semantic_read_count);
    println!("fallback_reads: {}", report.fallback_read_count);
    println!("unique_memory_ids: {}", report.unique_memory_ids);
    if !report.reads_by_command.is_empty() {
        println!("reads_by_command:");
        for (command, count) in &report.reads_by_command {
            println!("- {command}: {count}");
        }
    }
    if !report.writes_by_type.is_empty() {
        println!("writes_by_type:");
        for (event_type, count) in &report.writes_by_type {
            println!("- {event_type}: {count}");
        }
    }
    if report.recent_reads.is_empty() {
        println!("recent_reads: none");
    } else {
        println!("recent_reads:");
        for event in &report.recent_reads {
            let ids = if event.memory_ids.is_empty() {
                "-".to_string()
            } else {
                event
                    .memory_ids
                    .iter()
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            };
            println!(
                "- {} {} semantic={} results={} ids=[{}] {}",
                event.id,
                event.command,
                event.semantic_used,
                event.result_count,
                ids,
                truncate_chars(&event.query, 90)
            );
        }
    }
    Ok(())
}

fn usage_report(conn: &Connection, since_days: i64, limit: usize) -> Result<UsageReport> {
    let since_days = since_days.max(0);
    let since_ms = now_ms().saturating_sub(since_days.saturating_mul(86_400_000));
    let recent_reads = read_events(conn, since_ms, limit)?;
    let all_reads = read_events(conn, since_ms, usize::MAX)?;
    let mut reads_by_command = BTreeMap::new();
    let mut unique_ids = HashSet::new();
    let mut semantic_read_count = 0;
    for event in &all_reads {
        *reads_by_command.entry(event.command.clone()).or_insert(0) += 1;
        if event.semantic_used {
            semantic_read_count += 1;
        }
        for id in &event.memory_ids {
            unique_ids.insert(id.clone());
        }
    }
    let mut writes_by_type = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT event_type, COUNT(*) FROM memory_events WHERE created_at >= ?1 GROUP BY event_type",
    )?;
    let write_rows = stmt.query_map(params![since_ms], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut write_count = 0;
    for row in write_rows {
        let (event_type, count) = row?;
        let count = count.max(0) as usize;
        write_count += count;
        writes_by_type.insert(event_type, count);
    }
    Ok(UsageReport {
        since_days,
        read_count: all_reads.len(),
        write_count,
        semantic_read_count,
        fallback_read_count: all_reads.len().saturating_sub(semantic_read_count),
        unique_memory_ids: unique_ids.len(),
        reads_by_command,
        writes_by_type,
        recent_reads,
    })
}

fn print_usefulness_report(
    conn: &Connection,
    since_days: i64,
    stale_days: i64,
    hot_threshold: usize,
    json_out: bool,
) -> Result<()> {
    let report = usefulness_report(conn, since_days, stale_days, hot_threshold)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Usefulness Report");
    println!("active: {}", report.total_active);
    println!("hot: {}", report.hot.len());
    println!("unused: {}", report.unused.len());
    println!("stale: {}", report.stale.len());
    println!("too_long: {}", report.too_long.len());
    println!("no_links: {}", report.no_links.len());
    println!("missing_links: {}", report.missing_links.len());
    println!(
        "duplicate_candidates: {}",
        report.duplicate_candidates.len()
    );
    render_usefulness_items("Hot", &report.hot);
    render_usefulness_items("Unused", &report.unused);
    render_usefulness_items("Stale", &report.stale);
    if !report.suggestions.is_empty() {
        println!("Suggestions:");
        for suggestion in &report.suggestions {
            let id = suggestion.id.as_deref().unwrap_or("-");
            println!("- {} {} {}", suggestion.action, id, suggestion.detail);
        }
    }
    Ok(())
}

fn render_usefulness_items(title: &str, items: &[UsefulnessItem]) {
    if items.is_empty() {
        return;
    }
    println!("{title}:");
    for item in items.iter().take(10) {
        println!(
            "- {} [{}] requests={} {}",
            item.id, item.memory_type, item.request_count, item.title
        );
    }
}

fn usefulness_report(
    conn: &Connection,
    since_days: i64,
    stale_days: i64,
    hot_threshold: usize,
) -> Result<UsefulnessReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let counts = memory_request_counts_since(conn, Some(since_ms))?;
    let active = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let stale_cutoff = now_ms().saturating_sub(stale_days.max(0).saturating_mul(86_400_000));
    let mut hot = Vec::new();
    let mut unused = Vec::new();
    let mut stale = Vec::new();
    let mut too_long = Vec::new();
    let mut no_links = Vec::new();
    let mut suggestions = Vec::new();
    for memory in &active {
        let request_count = counts.get(&memory.id).copied().unwrap_or(0);
        let item = UsefulnessItem {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            request_count,
            updated_at: memory.updated_at,
            body_chars: memory.body.chars().count(),
        };
        if request_count >= hot_threshold.max(1) {
            hot.push(item.clone());
        }
        if request_count == 0 {
            unused.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "review_unused".to_string(),
                id: Some(memory.id.clone()),
                detail: "not used by recent memory reads; verify, link, supersede, or reject"
                    .to_string(),
            });
        }
        if memory.updated_at < stale_cutoff {
            stale.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "review_stale".to_string(),
                id: Some(memory.id.clone()),
                detail: format!("not updated for at least {stale_days} day(s)"),
            });
        }
        if item.body_chars > 1200 {
            too_long.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "compact_body".to_string(),
                id: Some(memory.id.clone()),
                detail: "body is over 1200 chars; summarize for token-light recall".to_string(),
            });
        }
        if get_links(conn, &memory.id)?.is_empty() {
            no_links.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "add_links".to_string(),
                id: Some(memory.id.clone()),
                detail: "add file:/symbol: links so memory_impact can target it".to_string(),
            });
        }
    }
    hot.sort_by_key(|item| std::cmp::Reverse(item.request_count));
    unused.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
    stale.sort_by_key(|item| item.updated_at);
    too_long.sort_by_key(|item| std::cmp::Reverse(item.body_chars));
    no_links.sort_by_key(|item| std::cmp::Reverse(item.request_count));
    let missing_links = link_report(conn, None, Path::new("."), false)?
        .into_iter()
        .filter(|link| link.status == "missing")
        .collect::<Vec<_>>();
    for link in &missing_links {
        suggestions.push(UsefulnessSuggestion {
            action: "fix_missing_link".to_string(),
            id: Some(link.memory_id.clone()),
            detail: format!("{}:{} {}", link.kind, link.target, link.detail),
        });
    }
    let duplicate_candidates = merge_candidates(conn, 20)?;
    for candidate in &duplicate_candidates {
        suggestions.push(UsefulnessSuggestion {
            action: "merge_candidate".to_string(),
            id: Some(candidate.duplicate_id.clone()),
            detail: format!("merge into {} ({})", candidate.primary_id, candidate.reason),
        });
    }
    suggestions.truncate(100);
    Ok(UsefulnessReport {
        since_days,
        stale_days,
        hot_threshold,
        total_active: active.len(),
        hot,
        unused,
        stale,
        too_long,
        no_links,
        missing_links,
        duplicate_candidates,
        suggestions,
    })
}

fn print_quality_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = quality_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Quality Report");
    println!("total: {}", report.total);
    println!("average_score: {:.1}", report.average_score);
    for item in report.weakest.iter().take(10) {
        println!(
            "- {:.1} {} [{}] requests={} +{} -{} {}",
            item.score,
            item.id,
            item.memory_type,
            item.request_count,
            item.positive_feedback,
            item.negative_feedback,
            item.title
        );
    }
    Ok(())
}

fn quality_report(conn: &Connection, since_days: i64, limit: usize) -> Result<QualityReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let request_counts = memory_request_counts_since(conn, Some(since_ms))?;
    let feedback = memory_feedback_counts(conn, since_ms)?;
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let mut items = Vec::new();
    let mut suggestions = Vec::new();
    for memory in rows {
        let request_count = request_counts.get(&memory.id).copied().unwrap_or(0);
        let (positive_feedback, negative_feedback, _missing_feedback) =
            feedback.get(&memory.id).copied().unwrap_or((0, 0, 0));
        let links = get_links(conn, &memory.id)?.len();
        let body_chars = memory.body.chars().count();
        let mut usefulness_score = 20.0 + (request_count.min(10) as f64 * 4.0);
        usefulness_score += positive_feedback.min(10) as f64 * 5.0;
        usefulness_score -= negative_feedback.min(10) as f64 * 6.0;
        usefulness_score += match memory.memory_type.as_str() {
            "decision" | "constraint" | "user_preference" | "product_goal" => 12.0,
            "known_issue" | "command" | "design_note" => 8.0,
            "task_state" => 4.0,
            _ => 2.0,
        };
        if memory.status == "uncertain" {
            usefulness_score -= 8.0;
        }
        let mut token_saving_score = if body_chars <= 600 {
            18.0
        } else if body_chars <= 1200 {
            10.0
        } else {
            -10.0
        };
        if request_count > 0 {
            token_saving_score += 8.0;
        }
        if links > 0 {
            token_saving_score += 6.0;
        }
        let mut risk_score = 5.0;
        if matches!(
            memory.memory_type.as_str(),
            "decision" | "constraint" | "user_preference" | "product_goal"
        ) {
            risk_score += 25.0;
        }
        if memory.status == "uncertain" {
            risk_score += 10.0;
        }
        if links == 0 {
            risk_score += 8.0;
        }
        if body_chars > 1200 {
            risk_score += 5.0;
        }
        let mut reasons = Vec::new();
        if request_count > 0 {
            reasons.push(format!("used {request_count} time(s) recently"));
        } else {
            reasons.push("unused recently".to_string());
            suggestions.push(UsefulnessSuggestion {
                action: "review_unused".to_string(),
                id: Some(memory.id.clone()),
                detail: "low quality score because no recent retrieval used this card".to_string(),
            });
        }
        if links == 0 {
            reasons.push("no evidence links".to_string());
        }
        if body_chars > 1200 {
            reasons.push("large body increases token cost".to_string());
        }
        if positive_feedback > 0 || negative_feedback > 0 {
            reasons.push(format!(
                "feedback +{positive_feedback} -{negative_feedback}"
            ));
        }
        let score = (usefulness_score + token_saving_score - risk_score).clamp(0.0, 100.0);
        items.push(MemoryQuality {
            id: memory.id,
            memory_type: memory.memory_type,
            title: memory.title,
            score,
            usefulness_score,
            token_saving_score,
            risk_score,
            request_count,
            positive_feedback,
            negative_feedback,
            body_chars,
            links,
            reasons,
        });
    }
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let strongest = items.iter().take(limit).cloned().collect::<Vec<_>>();
    let mut weakest = items.clone();
    weakest.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    weakest.truncate(limit);
    let average_score = if items.is_empty() {
        0.0
    } else {
        items.iter().map(|item| item.score).sum::<f64>() / items.len() as f64
    };
    Ok(QualityReport {
        version: 1,
        since_days,
        total: items.len(),
        average_score,
        strongest,
        weakest,
        items: items.into_iter().take(limit).collect(),
        suggestions,
    })
}

fn memory_feedback_counts(
    conn: &Connection,
    since_ms: i64,
) -> Result<HashMap<String, (usize, usize, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT detail FROM memory_events WHERE event_type = 'memory_feedback' AND created_at >= ?1",
    )?;
    let rows = stmt.query_map(params![since_ms], |row| row.get::<_, String>(0))?;
    let mut counts = HashMap::new();
    for row in rows {
        let detail = row?;
        let Ok(value) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        let rating = value.get("rating").and_then(Value::as_str).unwrap_or("");
        let ids = value
            .get("ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for id in ids
            .into_iter()
            .filter_map(|id| id.as_str().map(str::to_string))
        {
            let entry = counts.entry(id).or_insert((0, 0, 0));
            match rating {
                "useful" => entry.0 += 1,
                "useless" => entry.1 += 1,
                "missing" => entry.2 += 1,
                _ => {}
            }
        }
    }
    Ok(counts)
}

fn feedback_summary(conn: &Connection, since_days: i64) -> Result<FeedbackSummary> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let counts = memory_feedback_counts(conn, since_ms)?;
    let mut positive = 0;
    let mut negative = 0;
    let mut missing = 0;
    for (pos, neg, miss) in counts.values() {
        positive += *pos;
        negative += *neg;
        missing += *miss;
    }
    Ok(FeedbackSummary {
        since_days,
        positive,
        negative,
        missing,
        events: positive + negative + missing,
    })
}

fn print_feedback_report(
    conn: &Connection,
    ids: &[String],
    rating: FeedbackRating,
    command: &str,
    query: &str,
    note: &str,
    json_out: bool,
) -> Result<()> {
    let rating = match rating {
        FeedbackRating::Useful => "useful",
        FeedbackRating::Useless => "useless",
        FeedbackRating::Missing => "missing",
    };
    let detail = serde_json::to_string(&json!({
        "rating": rating,
        "ids": ids,
        "command": command,
        "query": query,
        "note": note,
    }))?;
    log_event(conn, "memory_feedback", None, &detail)?;
    let report = FeedbackReport {
        ok: true,
        rating: rating.to_string(),
        ids: ids.to_vec(),
        written_event: "memory_feedback".to_string(),
        summary: feedback_summary(conn, 30)?,
    };
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("feedback: {rating}");
        println!("ids: {}", ids.join(","));
    }
    Ok(())
}

fn print_budget_plan(
    conn: &Connection,
    task: &str,
    scope: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let plan = budget_plan(conn, task, scope)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("profile: {}", plan.profile);
        println!("max_chars: {}", plan.max_chars);
        for reason in plan.reasons {
            println!("- {reason}");
        }
    }
    Ok(())
}

fn budget_plan(conn: &Connection, task: &str, scope: Option<&str>) -> Result<BudgetPlan> {
    let terms = tokenize(task);
    let risky = task.to_lowercase();
    let broad = [
        "refactor",
        "migration",
        "schema",
        "release",
        "architecture",
        "autonomous",
        "security",
    ]
    .iter()
    .any(|needle| risky.contains(needle));
    let pending = list_inbox(conn, "pending", usize::MAX)?.len();
    let active_count = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        scope,
        500,
    )?
    .len();
    let mut reasons = Vec::new();
    let profile = if broad || terms.len() > 14 {
        reasons.push("broad or risky task needs more doctrine and impact memory".to_string());
        BudgetProfile::Deep
    } else if pending > 20 || active_count > 80 || terms.len() > 8 {
        reasons.push("moderate project state or task complexity".to_string());
        BudgetProfile::Normal
    } else {
        reasons.push("small task should stay token-light".to_string());
        BudgetProfile::Tiny
    };
    if pending > 0 {
        reasons.push(format!(
            "{pending} pending inbox item(s) may affect context freshness"
        ));
    }
    let profile_name = match profile {
        BudgetProfile::Tiny => "tiny",
        BudgetProfile::Normal => "normal",
        BudgetProfile::Deep => "deep",
    };
    Ok(BudgetPlan {
        task: task.to_string(),
        profile: profile_name.to_string(),
        max_chars: budget_profile_chars(Some(profile)).unwrap_or(1200),
        include_recent: match profile {
            BudgetProfile::Tiny => 3,
            BudgetProfile::Normal => 6,
            BudgetProfile::Deep => 12,
        },
        semantic: true,
        reasons,
    })
}

fn print_project_profile(conn: &Connection, root: &Path, json_out: bool) -> Result<()> {
    let profile = project_profile_snapshot(conn, root, "project")?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        println!("root: {}", profile.root);
        println!(
            "active_profile: {}",
            profile.active_profile.as_deref().unwrap_or("-")
        );
        println!("memory_count: {}", profile.memory_count);
        println!("pending_inbox: {}", profile.pending_inbox);
        println!("recommended_budget: {}", profile.recommended_budget);
        println!(
            "embeddings: {} {} {}",
            profile.embedding_provider, profile.embedding_endpoint, profile.embedding_model
        );
    }
    Ok(())
}

fn project_profile_snapshot(
    conn: &Connection,
    root: &Path,
    scope: &str,
) -> Result<ProjectProfileSnapshot> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let active_profile = fs::read_to_string(root.join(".agent/active_profile"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let count_type = |memory_type: &str| -> Result<usize> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE type = ?1 AND status IN ('active','uncertain')",
            params![memory_type],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as usize)
    };
    let memory_count = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        Some(scope),
        usize::MAX,
    )?
    .len();
    let (provider, endpoint, model) = read_project_embedding_config(&root);
    let budget = budget_plan(conn, "routine project memory task", Some(scope))?;
    Ok(ProjectProfileSnapshot {
        root: root.display().to_string(),
        active_profile,
        memory_count,
        pending_inbox: list_inbox(conn, "pending", usize::MAX)?.len(),
        decisions: count_type("decision")?,
        constraints: count_type("constraint")?,
        commands: count_type("command")?,
        known_issues: count_type("known_issue")?,
        embedding_provider: provider,
        embedding_endpoint: endpoint,
        embedding_model: model,
        recommended_budget: budget.profile,
    })
}

fn read_project_embedding_config(root: &Path) -> (String, String, String) {
    let default = (
        DEFAULT_EMBED_PROVIDER.to_string(),
        DEFAULT_EMBED_ENDPOINT.to_string(),
        DEFAULT_EMBED_MODEL.to_string(),
    );
    let Ok(raw) = fs::read_to_string(root.join(".agent/config.toml")) else {
        return default;
    };
    let Ok(value) = raw.parse::<toml::Value>() else {
        return default;
    };
    let Some(embeddings) = value.get("embeddings") else {
        return default;
    };
    (
        embeddings
            .get("provider")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_PROVIDER)
            .to_string(),
        embeddings
            .get("endpoint")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_ENDPOINT)
            .to_string(),
        embeddings
            .get("model")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_MODEL)
            .to_string(),
    )
}

fn app_project_root_for_db(db: &Path) -> Option<PathBuf> {
    let db = app_canonical_or_absolute(db);
    let agent_dir = db.parent()?;
    if agent_dir.file_name()?.to_str()? != ".agent" {
        return None;
    }
    agent_dir.parent().map(Path::to_path_buf)
}

fn app_push_unique_db(dbs: &mut Vec<PathBuf>, db: &Path) {
    let key = app_canonical_or_absolute(db);
    if !dbs
        .iter()
        .any(|existing| app_canonical_or_absolute(existing) == key)
    {
        dbs.push(db.to_path_buf());
    }
}

fn app_canonical_or_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn app_project_counts(db: &Path) -> Result<(i64, i64)> {
    let conn = open_db(db)?;
    let memories = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    let pending = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    Ok((memories, pending))
}

fn print_onboard(
    root: &Path,
    install_autonomous: bool,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = onboard_project(root, install_autonomous, provider, endpoint, model)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("onboard: {}", if report.ok { "ok" } else { "warn" });
        println!("root: {}", report.root);
        for action in report.actions {
            println!("- {action}");
        }
    }
    Ok(())
}

fn onboard_project(
    root: &Path,
    install_autonomous: bool,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<OnboardReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    fs::create_dir_all(root.join(".agent"))?;
    let db = root.join(".agent").join("memory.db");
    let conn = open_db(&db)?;
    let mut actions = Vec::new();
    write_project_config(
        &root.join(".agent").join("config.toml"),
        &db,
        provider,
        endpoint,
        model,
    )?;
    actions.push("config".to_string());
    write_workspace_rules(&root, true)?;
    upsert_project_agents(&root)?;
    actions.push("workspace_init".to_string());
    let profile = project_profile_snapshot(&conn, &root, "project")?;
    if profile.memory_count == 0 {
        let title = format!(
            "{} project memory profile",
            root.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Project")
        );
        add_memory(
            &conn,
            AddMemory {
                id: None,
                memory_type: "product_goal".to_string(),
                title,
                body: "Project memory was onboarded automatically; future agents should keep recall token-light and save only durable decisions, constraints, commands, risks, and task state.".to_string(),
                scope: "project".to_string(),
                status: "active".to_string(),
                source: Some("onboard".to_string()),
                supersedes: None,
                confidence: 0.8,
                links: Vec::new(),
            },
        )?;
        actions.push("seed_project_goal".to_string());
    }
    let embedding =
        embeddings::embed_index(&conn, provider, endpoint, model, &[], None, false).ok();
    if embedding.is_some() {
        actions.push("embed_index".to_string());
    }
    let autonomous_plist = if install_autonomous {
        let output = expand_tilde("~/Library/LaunchAgents/com.dukememory.autonomous.plist");
        write_autonomous_launchd_plist(AutonomousLaunchdRequest {
            db: &db,
            output: &output,
            level: AutonomousLevel::Normal,
            interval_secs: 300,
            status_file: &root.join(".agent").join("autonomous-status.json"),
            rollback_dir: &root.join(".agent").join("autonomous-rollbacks"),
            backup_dir: &root.join(".agent").join("backups"),
            provider,
            endpoint,
            model,
            force: true,
            dry_run: false,
        })?;
        actions.push("autonomous_install".to_string());
        Some(output.display().to_string())
    } else {
        None
    };
    Ok(OnboardReport {
        ok: true,
        root: root.display().to_string(),
        db: db.display().to_string(),
        actions,
        profile: project_profile_snapshot(&conn, &root, "project")?,
        embedding,
        autonomous_plist,
    })
}

fn print_dashboard(default_db: &Path, json_out: bool) -> Result<()> {
    let report = dashboard_report(default_db)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("dukememory. Dashboard");
        for project in report.projects {
            println!(
                "- {} memories={} pending={} quality={} autonomous={}",
                project.name,
                project.memories,
                project.pending_inbox,
                project
                    .quality_average
                    .map(|value| format!("{value:.1}"))
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_ok
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
    }
    Ok(())
}

fn dashboard_report(default_db: &Path) -> Result<DashboardReport> {
    let projects = discover_project_dbs(default_db)?
        .into_iter()
        .filter_map(|db| {
            let root = app_project_root_for_db(&db).unwrap_or_else(|| {
                db.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            let conn = open_db(&db).ok()?;
            let profile = project_profile_snapshot(&conn, &root, "project").ok();
            let quality = quality_report(&conn, 30, 10).ok();
            let autonomous =
                read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
            let embedding = embeddings::embed_status(
                &conn,
                DEFAULT_EMBED_PROVIDER,
                DEFAULT_EMBED_ENDPOINT,
                DEFAULT_EMBED_MODEL,
            )
            .ok();
            let (memories, pending_inbox) = app_project_counts(&db).unwrap_or((0, 0));
            Some(ProjectDashboardItem {
                name: root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("project")
                    .to_string(),
                root: root.display().to_string(),
                db: db.display().to_string(),
                memories,
                pending_inbox,
                quality_average: quality.map(|quality| quality.average_score),
                autonomous_ok: autonomous.map(|status| status.ok),
                embedding_missing: embedding.map(|status| status.missing),
                recommended_budget: profile.map(|profile| profile.recommended_budget),
            })
        })
        .collect::<Vec<_>>();
    Ok(DashboardReport {
        version: 1,
        projects,
    })
}

fn discover_project_dbs(default_db: &Path) -> Result<Vec<PathBuf>> {
    let mut dbs = Vec::new();
    app_push_unique_db(&mut dbs, default_db);
    if let Some(root) = app_project_root_for_db(default_db)
        && let Some(parent) = root.parent()
    {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let candidate = entry.path().join(".agent").join("memory.db");
                if candidate.exists() {
                    app_push_unique_db(&mut dbs, &candidate);
                }
            }
        }
    }
    Ok(dbs)
}

fn handle_inbox_v2(conn: &Connection, command: InboxV2Command) -> Result<()> {
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

fn print_inbox_v2_report(report: &InboxV2Report, json_out: bool) -> Result<()> {
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

fn inbox_v2_report(conn: &Connection, limit: usize, apply: bool) -> Result<InboxV2Report> {
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

fn print_policy_tune(
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

fn policy_tune_report(conn: &Connection, output: &Path, dry_run: bool) -> Result<PolicyTuneReport> {
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

fn print_memory_qa(conn: &Connection, root: &Path, since_days: i64, json_out: bool) -> Result<()> {
    let report = memory_qa_report(conn, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Memory QA: {}", if report.ok { "ok" } else { "warn" });
        println!("score: {:.1}", report.score);
        println!("reads: {}", report.reads);
        println!(
            "semantic_read_rate: {:.1}%",
            report.semantic_read_rate * 100.0
        );
        println!("useful_rate: {:.1}%", report.useful_rate * 100.0);
        println!("quality_average: {:.1}", report.quality_average);
        println!("token_saving_estimate: {}", report.token_saving_estimate);
        for issue in &report.issues {
            println!("issue: {issue}");
        }
        for recommendation in &report.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

fn memory_qa_report(conn: &Connection, root: &Path, since_days: i64) -> Result<MemoryQaReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let usage = usage_report(conn, since_days, 20)?;
    let quality = quality_report(conn, since_days, 20)?;
    let usefulness = usefulness_report(conn, since_days, 30, 3)?;
    let live = live_eval_report(conn, since_days)?;
    let embedding = embeddings::embed_status(
        conn,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )
    .ok();
    let autonomous = read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
    let semantic_read_rate = if usage.read_count == 0 {
        0.0
    } else {
        usage.semantic_read_count as f64 / usage.read_count as f64
    };
    let token_saving_estimate = quality
        .items
        .iter()
        .map(|item| {
            item.request_count
                .saturating_mul(item.body_chars.saturating_sub(240))
                / 4
        })
        .sum::<usize>();
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    if usage.read_count == 0 {
        issues.push("no recent memory reads".to_string());
        recommendations
            .push("ensure agents start with dukememory brief or MCP memory_brief".to_string());
    }
    if semantic_read_rate < 0.50 && usage.read_count > 0 {
        issues.push("semantic recall is used by less than half of recent reads".to_string());
        recommendations
            .push("run dukememory embed-status and embed-index if missing or stale".to_string());
    }
    if quality.average_score < 60.0 && quality.total > 0 {
        issues.push("average memory quality is low".to_string());
        recommendations
            .push("review weakest cards with dukememory quality-report --json".to_string());
    }
    if usefulness.too_long.len() > 3 {
        issues.push(format!(
            "{} memory cards are too long",
            usefulness.too_long.len()
        ));
        recommendations.push("compact long cards into bounded summaries".to_string());
    }
    if usefulness.duplicate_candidates.len() > 3 {
        issues.push(format!(
            "{} duplicate candidates detected",
            usefulness.duplicate_candidates.len()
        ));
        recommendations.push(
            "let autonomous supersede safe duplicates or review merge-candidates".to_string(),
        );
    }
    if let Some(embedding) = &embedding {
        if embedding.missing > 0 || embedding.stale > 0 {
            issues.push(format!(
                "embedding index is not current: missing={} stale={}",
                embedding.missing, embedding.stale
            ));
            recommendations.push("run dukememory embed-index".to_string());
        }
    } else {
        issues.push("embedding status unavailable".to_string());
        recommendations.push("check embedding provider configuration".to_string());
    }
    if autonomous.as_ref().is_some_and(|status| !status.ok) {
        issues.push("latest autonomous status is not ok".to_string());
        recommendations.push("run dukememory autonomous explain --json".to_string());
    }
    if live.missing > 0 {
        issues.push(format!(
            "{} feedback event(s) reported missing memory",
            live.missing
        ));
        recommendations
            .push("convert repeated missing facts into durable memory cards".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    let mut score = 100.0;
    score -= usefulness.unused.len().min(10) as f64 * 2.0;
    score -= usefulness.too_long.len().min(10) as f64 * 3.0;
    score -= usefulness.duplicate_candidates.len().min(10) as f64 * 2.0;
    score -= embedding
        .as_ref()
        .map(|item| item.missing + item.stale)
        .unwrap_or(5)
        .min(10) as f64
        * 4.0;
    if usage.read_count == 0 {
        score -= 20.0;
    }
    if autonomous.as_ref().is_some_and(|status| !status.ok) {
        score -= 12.0;
    }
    score = score.clamp(0.0, 100.0);
    Ok(MemoryQaReport {
        version: 1,
        ok: score >= 70.0 && issues.len() <= 3,
        score,
        root: root.display().to_string(),
        since_days,
        reads: usage.read_count,
        semantic_read_rate,
        useful_rate: live.useful_rate,
        quality_average: quality.average_score,
        active_memories: usefulness.total_active,
        unused: usefulness.unused.len(),
        stale: usefulness.stale.len(),
        too_long: usefulness.too_long.len(),
        duplicate_candidates: usefulness.duplicate_candidates.len(),
        embedding_missing: embedding.as_ref().map(|item| item.missing).unwrap_or(0),
        embedding_stale: embedding.as_ref().map(|item| item.stale).unwrap_or(0),
        autonomous_ok: autonomous.map(|status| status.ok),
        token_saving_estimate,
        issues,
        recommendations,
    })
}

fn print_memory_contract(
    conn: &Connection,
    root: &Path,
    write: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_contract_report(conn, root, write)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.content);
        if report.written {
            println!("written: {}", report.path);
        }
        if let Some(id) = &report.memory_id {
            println!("memory_id: {id}");
        }
    }
    Ok(())
}

fn memory_contract_report(
    conn: &Connection,
    root: &Path,
    write: bool,
) -> Result<MemoryContractReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = root.join(".agent").join("MEMORY_CONTRACT.md");
    let content = render_memory_contract(conn, &root, 3600)?;
    let mut memory_id = None;
    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_file(&path, content.as_bytes())?;
        memory_id = Some(upsert_memory_contract_card(conn, &content)?);
    }
    Ok(MemoryContractReport {
        version: 1,
        root: root.display().to_string(),
        path: path.display().to_string(),
        written: write,
        memory_id,
        max_chars: 3600,
        content,
    })
}

fn render_memory_contract(conn: &Connection, root: &Path, max_chars: usize) -> Result<String> {
    let profile = project_profile_snapshot(conn, root, "project")?;
    let mut out = String::new();
    out.push_str("# dukememory. Project Contract\n\n");
    out.push_str(&format!("Root: {}\n", root.display()));
    out.push_str(&format!(
        "Memory: {} cards, {} decisions, {} constraints, {} commands, recommended budget: {}.\n",
        profile.memory_count,
        profile.decisions,
        profile.constraints,
        profile.commands,
        profile.recommended_budget
    ));
    out.push_str(&format!(
        "Embeddings: provider={}, model={}\n\n",
        profile.embedding_provider, profile.embedding_model
    ));
    out.push_str("Rules:\n");
    out.push_str("- Start coding tasks with `dukememory brief \"<task>\" --budget-profile tiny` or MCP `memory_brief`.\n");
    out.push_str("- Use `dukememory impact <file-or-symbol> --budget-profile tiny` before editing known areas.\n");
    out.push_str(
        "- Save only durable decisions, constraints, commands, known issues, and task state.\n",
    );
    out.push_str("- Keep context small; use `dukememory recall \"<task>\" --max-chars 1200` instead of broad dumps.\n");
    out.push_str("- Autonomous maintenance is allowed when reversible; use rollback instead of hard delete.\n\n");
    append_contract_section(conn, &mut out, "Goals", &["product_goal".to_string()], 3)?;
    append_contract_section(
        conn,
        &mut out,
        "Decisions",
        &["decision".to_string(), "constraint".to_string()],
        8,
    )?;
    append_contract_section(conn, &mut out, "Commands", &["command".to_string()], 5)?;
    append_contract_section(
        conn,
        &mut out,
        "Known Risks",
        &["known_issue".to_string()],
        5,
    )?;
    Ok(truncate_chars(&out, max_chars))
}

fn append_contract_section(
    conn: &Connection,
    out: &mut String,
    title: &str,
    types: &[String],
    limit: usize,
) -> Result<()> {
    let rows = query_memories(
        conn,
        None,
        types,
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        limit,
    )?;
    if rows.is_empty() {
        return Ok(());
    }
    out.push_str(title);
    out.push_str(":\n");
    for row in rows {
        out.push_str(&format!(
            "- {} [{}]: {}\n",
            truncate_chars(&row.title, 96),
            row.memory_type,
            truncate_chars(&one_line_summary(&row.body), 220)
        ));
    }
    out.push('\n');
    Ok(())
}

fn upsert_memory_contract_card(conn: &Connection, content: &str) -> Result<String> {
    let existing = query_memories(
        conn,
        None,
        &["design_note".to_string()],
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        100,
    )?
    .into_iter()
    .find(|memory| memory.title == "Project memory contract");
    if let Some(memory) = existing {
        conn.execute(
            r#"
            UPDATE memories SET
                type = 'design_note',
                scope = 'project',
                title = 'Project memory contract',
                body = ?1,
                status = 'active',
                source = 'memory_contract',
                updated_at = ?2,
                confidence = 0.95
            WHERE id = ?3
            "#,
            params![content, now_ms(), memory.id],
        )?;
        conn.execute(
            "DELETE FROM memory_links WHERE memory_id = ?1",
            params![memory.id],
        )?;
        insert_links(
            conn,
            &memory.id,
            &parse_links(&["file:.agent/MEMORY_CONTRACT.md".to_string()])?,
        )?;
        log_event(
            conn,
            "memory_updated",
            Some(&memory.id),
            "updated project memory contract",
        )?;
        Ok(memory.id)
    } else {
        add_memory(
            conn,
            AddMemory {
                id: None,
                memory_type: "design_note".to_string(),
                title: "Project memory contract".to_string(),
                body: content.to_string(),
                scope: "project".to_string(),
                status: "active".to_string(),
                source: Some("memory_contract".to_string()),
                supersedes: None,
                confidence: 0.95,
                links: vec!["file:.agent/MEMORY_CONTRACT.md".to_string()],
            },
        )
    }
}

fn print_upgrade_project(
    conn: &Connection,
    root: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = upgrade_project_report(conn, root, from, to, backup_dir, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("upgrade_project: {}", if report.ok { "ok" } else { "warn" });
        for action in &report.actions {
            println!("- {action}");
        }
        for error in &report.errors {
            println!("error: {error}");
        }
    }
    Ok(())
}

fn upgrade_project_report(
    conn: &Connection,
    root: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
) -> Result<UpgradeProjectReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    let mut install = None;
    let source = from
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_exe()?);
    let target = resolve_install_target(to);
    let same_binary = source
        .canonicalize()
        .ok()
        .zip(target.canonicalize().ok())
        .is_some_and(|(source, target)| source == target);
    if same_binary {
        actions.push(format!("binary already running from {}", target.display()));
    } else {
        match update_install(Some(&source), to, backup_dir, dry_run) {
            Ok(report) => {
                actions.push(if report.changed {
                    format!("binary update prepared: {}", report.target)
                } else {
                    format!("binary already current: {}", report.target)
                });
                install = Some(report);
            }
            Err(err) => errors.push(format!("binary update failed: {err}")),
        }
    }
    if dry_run {
        actions.push("would refresh workspace rules and AGENTS.md".to_string());
        actions.push("would refresh Codex skill".to_string());
        actions.push("would write memory contract".to_string());
    } else {
        match write_workspace_rules(&root, true) {
            Ok(path) => actions.push(format!("workspace rules refreshed: {}", path.display())),
            Err(err) => errors.push(format!("workspace rules failed: {err}")),
        }
        match upsert_project_agents(&root) {
            Ok(()) => actions.push("AGENTS.md dukememory. block refreshed".to_string()),
            Err(err) => errors.push(format!("AGENTS.md refresh failed: {err}")),
        }
        match write_codex_skill(&expand_tilde("~/.codex/skills"), true) {
            Ok(path) => actions.push(format!("Codex skill refreshed: {}", path.display())),
            Err(err) => errors.push(format!("Codex skill refresh failed: {err}")),
        }
    }
    let contract = match memory_contract_report(conn, &root, !dry_run) {
        Ok(report) => {
            if report.written {
                actions.push(format!("memory contract written: {}", report.path));
            }
            Some(report)
        }
        Err(err) => {
            errors.push(format!("memory contract failed: {err}"));
            None
        }
    };
    let qa = match memory_qa_report(conn, &root, 7) {
        Ok(report) => Some(report),
        Err(err) => {
            errors.push(format!("memory qa failed: {err}"));
            None
        }
    };
    let ok = errors.is_empty() && qa.as_ref().is_none_or(|report| report.ok || dry_run);
    Ok(UpgradeProjectReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        ok,
        root: root.display().to_string(),
        dry_run,
        actions,
        install,
        qa,
        contract,
        errors,
    })
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

fn workspace_init(root: &Path, force: bool) -> Result<()> {
    let rules = write_workspace_rules(root, force)?;
    upsert_project_agents(root)?;
    println!("{}", rules.display());
    Ok(())
}

fn write_workspace_rules(root: &Path, force: bool) -> Result<PathBuf> {
    let agent_dir = root.join(".agent");
    fs::create_dir_all(&agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;
    let rules = agent_dir.join("rules.rhai");
    if rules.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            rules.display()
        );
    }
    write_file(
        &rules,
        br#"fn score_memory(type, status, scope, title, body, task, confidence) {
  if status == "active" { confidence * 2.0 } else { 0.0 }
}

fn should_include(type, status, scope, title, body, task, confidence) {
  status != "rejected"
}

fn should_redact(type, status, scope, title, body, confidence) {
  body.contains("api_key") || body.contains("token =")
}
"#,
    )?;
    Ok(rules)
}

fn project_root_from_config(config: &Path) -> Option<PathBuf> {
    let parent = config.parent()?;
    if parent.file_name().and_then(|value| value.to_str()) == Some(".agent") {
        parent.parent().map(Path::to_path_buf)
    } else {
        parent.canonicalize().ok()
    }
}

fn upsert_project_agents(root: &Path) -> Result<()> {
    let path = root.join("AGENTS.md");
    let block = project_memory_agents_block();
    let content = if path.exists() {
        let current = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if current.contains("<!-- DUKEMEMORY_START -->") {
            replace_marked_block(
                &current,
                "<!-- DUKEMEMORY_START -->",
                "<!-- DUKEMEMORY_END -->",
                block,
            )
        } else if current.trim().is_empty() {
            block.to_string()
        } else {
            format!("{}\n\n{}", current.trim_end(), block)
        }
    } else {
        block.to_string()
    };
    write_file(&path, content.as_bytes())?;
    Ok(())
}

fn replace_marked_block(content: &str, start: &str, end: &str, replacement: &str) -> String {
    let Some(start_pos) = content.find(start) else {
        return format!("{}\n\n{}", content.trim_end(), replacement);
    };
    let Some(end_rel) = content[start_pos..].find(end) else {
        return format!("{}\n\n{}", content.trim_end(), replacement);
    };
    let end_pos = start_pos + end_rel + end.len();
    let mut out = String::new();
    out.push_str(content[..start_pos].trim_end());
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(replacement);
    let tail = content[end_pos..].trim_start();
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    out
}

fn project_memory_agents_block() -> &'static str {
    r#"<!-- DUKEMEMORY_START -->
## dukememory.

This repository has local project memory installed in `.agent/memory.db`.

For every new chat or coding task in this repository:
- Use the Codex skill `$dukememory-use` when available.
- Confirm the current project root before reading or writing memory; never write this project's durable facts into another `.agent/memory.db`.
- Default to read-only memory use unless there is a durable fact worth saving.
- Start with project memory before broad exploration. Prefer MCP `memory_brief` with a tiny budget for the user's task.
- Memory use is mandatory when `.agent/memory.db` exists: if MCP tools are not available, use the CLI fallback from the project root instead of skipping memory.
- CLI brief fallback: `dukememory brief "<task>" --budget-profile tiny`.
- When a touched file, symbol, subsystem, command, UI area, or error is known, call MCP `memory_impact` or run `dukememory impact <target> --budget-profile tiny`.
- For architectural/policy questions, call MCP `memory_doctrine`; use MCP `memory_evidence` for critical memory ids before relying on them.
- Before broad edits/refactors/dependency changes/schema changes/release work, call MCP `memory_drift` or run `dukememory drift --root .`.
- Persist only durable decisions, constraints, user preferences, project commands, known issues, and task state with MCP `memory_remember`/`memory_add` or the `dukememory remember`/`dukememory add` CLI.
- Before adding decisions, check MCP `memory_doctrine` or `dukememory doctrine --json`; use MCP `memory_evidence` for high-impact or surprising memory before relying on it.
- Do not save transient scratch notes, large logs, secrets, full file dumps, or obvious facts from nearby code.
- After a batch of important memory writes, run `dukememory embed-index` once so embeddings stay ready.
- Keep operational memory compact. Do not create recursive "compact of compact" summaries; prefer `dukememory memory-contract --write` for durable project-wide context.
- Memory maintenance is autonomous by default: `dukememory autonomous run-once --level normal` may refresh embeddings, backups, cleanup, safe inbox approvals, compact stale operational notes, and supersede safe duplicates without hard deletion.
- Roll back the last autonomous maintenance cycle with `dukememory autonomous rollback`; autonomous mode must keep rollback metadata and avoid hard delete by default.
- Before the final response after substantial work, run the same end routine: save useful durable outcomes or task state, then refresh embeddings once after writes.
- If memory was read or written, the final response must include a short receipt such as `Memory: used brief+impact, ids=[...], wrote=...`; if nothing durable was saved, say `wrote=none`.
- To inspect whether memory is being used and reused, run `dukememory usage-report --since-days 7`.
- To inspect memory quality and cleanup candidates, run `dukememory usefulness-report`.
- To inspect autonomous maintenance, run `dukememory autonomous status --json`.
- To inspect evidence-backed memory quality, run `dukememory quality-report --json`.
- To choose the smallest useful context budget, run `dukememory budget-plan "<task>" --json`.
- To get compressed token-light recall, run `dukememory recall "<task>" --max-chars 1200`.
- To inspect live memory usefulness from reads and feedback, run `dukememory eval live --json`.
- To inspect all local projects, run `dukememory dashboard --json`.
- To safely group and process inbox suggestions, run `dukememory inbox-v2 report --json`.
- To check whether memory is useful or noisy, run `dukememory memory-qa --json`.
- To refresh project-wide memory instructions and the compact contract, run `dukememory upgrade-project --json`.
- After a task, agents may record lightweight memory utility feedback with `dukememory feedback --id <memory-id> --rating useful|useless|missing`.

Keep memory use lightweight: prefer `brief`/`impact`; do not dump large context packs unless needed.
<!-- DUKEMEMORY_END -->"#
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

struct DaemonRequest<'a> {
    interval_secs: u64,
    once: bool,
    quiet: bool,
    auto_ingest: bool,
    autopilot: bool,
    session_dir: &'a Path,
    backup_dir: &'a Path,
    status_file: &'a Path,
    backup_keep: usize,
    backup_every_secs: u64,
    cleanup_audit_keep: usize,
    db: &'a Path,
    scope: &'a str,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
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

fn run_daemon(conn: &Connection, request: DaemonRequest<'_>) -> Result<()> {
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
struct AutopilotRepairReport {
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
struct AutopilotReport {
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
struct AutopilotAlert {
    ok: bool,
    level: String,
    generated_at: i64,
    violations: Vec<String>,
    recommendations: Vec<String>,
    report: AutopilotReport,
}

fn handle_autopilot(conn: &Connection, db: &Path, command: AutopilotCommand) -> Result<()> {
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

struct AutopilotRepairRequest<'a> {
    status_file: &'a Path,
    session_dir: &'a Path,
    backup_dir: &'a Path,
    backup_keep: usize,
    cleanup_audit_keep: usize,
    scope: &'a str,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    max_status_age_secs: u64,
}

fn autopilot_repair(
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

struct AutopilotReportRequest<'a> {
    status_file: &'a Path,
    session_dir: &'a Path,
    backup_dir: &'a Path,
    history_limit: usize,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
}

struct AutopilotAlertRequest<'a> {
    status_file: &'a Path,
    session_dir: &'a Path,
    backup_dir: &'a Path,
    history_limit: usize,
    max_pending: usize,
    max_failed_ticks: usize,
    max_status_age_secs: u64,
    max_embedding_stale: usize,
    require_backup: bool,
    require_endpoint: bool,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
}

fn autopilot_report(
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

fn autopilot_alert(
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
struct AutonomousReport {
    version: u32,
    ok: bool,
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
struct AutonomousAction {
    kind: String,
    status: String,
    detail: String,
    memory_id: Option<String>,
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
    RestoreInboxPending {
        inbox_id: String,
        memory_id: String,
    },
}

struct AutonomousRunRequest<'a> {
    level: AutonomousLevel,
    status_file: &'a Path,
    rollback_dir: &'a Path,
    backup_dir: &'a Path,
    backup_keep: usize,
    db: &'a Path,
    scope: &'a str,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
}

fn handle_autonomous(conn: &Connection, db: &Path, command: AutonomousCommand) -> Result<()> {
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

fn autonomous_run_once(
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
    if report.rollback.is_empty()
        && let Ok(previous) = read_autonomous_status(request.status_file)
        && !previous.rollback.is_empty()
    {
        report.rollback = previous.rollback;
        report.rollback_backup = previous.rollback_backup;
        report.actions.push(AutonomousAction {
            kind: "preserve_rollback".to_string(),
            status: "ok".to_string(),
            detail: "preserved last reversible autonomous change".to_string(),
            memory_id: None,
        });
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

fn autonomous_rollback(conn: &Connection, report: &AutonomousReport) -> Result<AutonomousReport> {
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

fn read_autonomous_status(path: &Path) -> Result<AutonomousReport> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read autonomous status {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("invalid autonomous status {}", path.display()))
}

fn write_autonomous_status(path: &Path, report: &AutonomousReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_file(path, serde_json::to_string_pretty(report)?.as_bytes())
}

struct AutonomousLaunchdRequest<'a> {
    db: &'a Path,
    output: &'a Path,
    level: AutonomousLevel,
    interval_secs: u64,
    status_file: &'a Path,
    rollback_dir: &'a Path,
    backup_dir: &'a Path,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    force: bool,
    dry_run: bool,
}

fn write_autonomous_launchd_plist(request: AutonomousLaunchdRequest<'_>) -> Result<()> {
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

struct RetrieveRequest<'a> {
    query: &'a str,
    strategy: RetrievalStrategy,
    format: OutputFormat,
    limit: usize,
    budget: usize,
    scope: Option<&'a str>,
    rules: Option<&'a Path>,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    audit_read: bool,
}

fn print_retrieve(conn: &Connection, request: RetrieveRequest<'_>) -> Result<()> {
    let report = retrieve_report(conn, &request)?;
    match request.format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Agent => {
            println!("Retrieved Memory:");
            println!("{}", report.receipt);
            println!("{}", render_retrieval_pack(&report.hits, request.budget)?);
            println!("\nSelection Reasons:");
            for hit in &report.hits {
                println!(
                    "- {} score={:.2}: {}",
                    hit.memory.memory.title,
                    hit.score,
                    hit.reasons.join(", ")
                );
            }
            if let Some(error) = &report.semantic_error {
                println!("\nSemantic fallback: {error}");
            }
            println!(
                "\nUse these memories as constraints unless contradicted by newer user input."
            );
        }
        OutputFormat::Markdown => {
            println!("## Retrieved Memory");
            println!("{}", report.receipt);
            for hit in &report.hits {
                let row = &hit.memory.memory;
                println!(
                    "- **{}** `{}` score={:.2}: {}",
                    row.title, row.memory_type, hit.score, row.body
                );
            }
        }
        OutputFormat::Plain => {
            println!("{}", render_retrieval_pack(&report.hits, request.budget)?);
            println!("{}", report.receipt);
        }
    }
    Ok(())
}

fn retrieve_report(conn: &Connection, request: &RetrieveRequest<'_>) -> Result<RetrievalReport> {
    let started = Instant::now();
    let mut candidates: HashMap<String, (Memory, Option<f64>)> = HashMap::new();
    for row in query_memories(
        conn,
        Some(request.query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        request.scope,
        request.limit.saturating_mul(2).max(request.limit),
    )? {
        candidates.entry(row.id.clone()).or_insert((row, None));
    }
    for row in query_memories(
        conn,
        None,
        &[],
        &["active".to_string()],
        request.scope,
        (request.limit / 3).max(2),
    )? {
        candidates.entry(row.id.clone()).or_insert((row, None));
    }
    let mut semantic_used = false;
    let mut semantic_error = None;
    if matches!(request.strategy, RetrievalStrategy::Hybrid) {
        match embeddings::semantic_index_ready(
            conn,
            request.provider,
            request.endpoint,
            request.model,
        ) {
            Ok(true) => {
                match embeddings::semantic_search(
                    conn,
                    request.provider,
                    request.endpoint,
                    request.model,
                    request.query,
                    request.limit,
                ) {
                    Ok(semantic) => {
                        semantic_used = !semantic.is_empty();
                        for item in semantic {
                            let memory = item.memory.memory;
                            candidates
                                .entry(memory.id.clone())
                                .and_modify(|existing| existing.1 = Some(item.score))
                                .or_insert((memory, Some(item.score)));
                        }
                    }
                    Err(err) => semantic_error = Some(err.to_string()),
                }
            }
            Ok(false) => {
                semantic_error =
                    Some("semantic index not ready; using FTS/local ranking".to_string());
            }
            Err(err) => semantic_error = Some(format!("semantic readiness check failed: {err}")),
        }
    }
    let rhai = request.rules.and_then(|path| load_rhai_rules(path).ok());
    let task_terms = tokenize(request.query);
    let mut hits = Vec::new();
    for (_, (memory, semantic_score)) in candidates {
        if !rhai_should_include(rhai.as_ref(), &memory, request.query)? {
            continue;
        }
        let links = get_links(conn, &memory.id)?;
        let (score, reasons) = retrieval_score(
            &memory,
            &links,
            &task_terms,
            request.scope,
            semantic_score,
            rhai.as_ref(),
            request.query,
        );
        let utility_score = memory_utility_score(&memory, links.len());
        hits.push(RetrievalHit {
            memory: MemoryWithLinks { memory, links },
            score,
            utility_score,
            semantic_score,
            reasons,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory.memory.updated_at.cmp(&a.memory.memory.updated_at))
    });
    hits.truncate(request.limit);
    let ids = hits
        .iter()
        .map(|hit| hit.memory.memory.id.clone())
        .collect::<Vec<_>>();
    let receipt = memory_receipt("retrieve", Some(semantic_used), &ids, "none");
    if request.audit_read {
        log_read_event(
            conn,
            ReadEventInput {
                command: "retrieve",
                query: request.query,
                ids: &ids,
                semantic_used,
                result_count: hits.len(),
                budget: request.budget,
                elapsed_ms: started.elapsed().as_millis(),
            },
        )?;
    }
    Ok(RetrievalReport {
        version: 14,
        query: request.query.to_string(),
        strategy: format!("{:?}", request.strategy).to_lowercase(),
        scope: request.scope.map(ToOwned::to_owned),
        semantic_used,
        semantic_error,
        receipt,
        hits,
    })
}

fn retrieve_rows(
    conn: &Connection,
    query: &str,
    strategy: RetrievalStrategy,
    limit: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<Vec<Memory>> {
    Ok(retrieve_report(
        conn,
        &RetrieveRequest {
            query,
            strategy,
            format: OutputFormat::Plain,
            limit,
            budget: usize::MAX,
            scope: None,
            rules: None,
            provider,
            endpoint,
            model,
            audit_read: false,
        },
    )?
    .hits
    .into_iter()
    .map(|hit| hit.memory.memory)
    .collect())
}

struct RecallRequest<'a> {
    query: &'a str,
    max_chars: usize,
    limit: usize,
    scope: Option<&'a str>,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    json_out: bool,
}

fn print_recall(conn: &Connection, request: RecallRequest<'_>) -> Result<()> {
    let report = recall_report(conn, &request)?;
    if request.json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_recall_report(&report));
    }
    Ok(())
}

fn recall_report(conn: &Connection, request: &RecallRequest<'_>) -> Result<RecallReport> {
    let retrieval = retrieve_report(
        conn,
        &RetrieveRequest {
            query: request.query,
            strategy: RetrievalStrategy::Hybrid,
            format: OutputFormat::Plain,
            limit: request.limit,
            budget: request.max_chars,
            scope: request.scope,
            rules: None,
            provider: request.provider,
            endpoint: request.endpoint,
            model: request.model,
            audit_read: true,
        },
    )?;
    let mut raw_chars = 0;
    let mut items = Vec::new();
    for hit in &retrieval.hits {
        let memory = &hit.memory.memory;
        raw_chars += memory.body.chars().count();
        items.push(RecallItem {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            summary: truncate_chars(&one_line_summary(&memory.body), 120),
            score: hit.score,
            reasons: hit.reasons.iter().take(3).cloned().collect(),
        });
    }
    let token_saving_estimate = raw_chars.saturating_sub(request.max_chars) / 4;
    Ok(RecallReport {
        query: request.query.to_string(),
        max_chars: request.max_chars,
        token_saving_estimate,
        receipt: retrieval.receipt,
        items,
    })
}

fn render_recall_report(report: &RecallReport) -> String {
    let mut out = format!(
        "Compressed Recall: {}\n{}\nEstimated token saving: {}\n",
        report.query, report.receipt, report.token_saving_estimate
    );
    for item in &report.items {
        let line = format!(
            "- {} [{}] {} -- {} ({})\n",
            item.id,
            item.memory_type,
            item.title,
            item.summary,
            item.reasons.join(",")
        );
        if out.len() + line.len() > report.max_chars {
            break;
        }
        out.push_str(&line);
    }
    truncate_chars(&out, report.max_chars)
}

fn retrieval_score(
    memory: &Memory,
    links: &[MemoryLink],
    task_terms: &HashSet<String>,
    requested_scope: Option<&str>,
    semantic_score: Option<f64>,
    rules: Option<&RhaiRules>,
    task: &str,
) -> (f64, Vec<String>) {
    let mut score = context_score(memory, task_terms, requested_scope);
    let mut reasons = Vec::new();
    reasons.push(format!("type:{}", memory.memory_type));
    reasons.push(format!("status:{}", memory.status));
    if memory.confidence >= 0.8 {
        reasons.push("high_confidence".to_string());
    } else if memory.confidence < 0.5 {
        reasons.push("low_confidence".to_string());
    }
    if let Some(scope) = requested_scope
        && memory.scope == scope
    {
        reasons.push("scope_match".to_string());
    }
    let haystack = tokenize(&format!("{} {}", memory.title, memory.body));
    let overlap = task_terms.intersection(&haystack).count();
    if overlap > 0 {
        reasons.push(format!("text_match:{overlap}"));
        score += overlap as f64;
    }
    let link_overlap = links
        .iter()
        .map(|link| tokenize(&format!("{} {}", link.kind, link.target)))
        .map(|tokens| task_terms.intersection(&tokens).count())
        .sum::<usize>();
    if link_overlap > 0 {
        reasons.push(format!("link_match:{link_overlap}"));
        score += link_overlap as f64 * 3.0;
    }
    if let Some(value) = semantic_score {
        reasons.push(format!("semantic:{value:.3}"));
        score += value * 12.0;
    }
    if let Some(id) = &memory.superseded_by {
        reasons.push(format!("superseded_by:{id}"));
        score -= 25.0;
    }
    if memory.supersedes.is_some() {
        reasons.push("supersedes_previous".to_string());
        score += 1.5;
    }
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    if age_days <= 7.0 {
        reasons.push("fresh".to_string());
    }
    let rhai = rhai_score(rules, memory, task).unwrap_or(0.0);
    if rhai != 0.0 {
        reasons.push(format!("rhai_score:{rhai:.2}"));
        score += rhai;
    }
    (score, reasons)
}

fn memory_utility_score(memory: &Memory, link_count: usize) -> f64 {
    let mut score = memory.confidence * 10.0;
    score += link_count.min(6) as f64 * 1.5;
    score += match memory.memory_type.as_str() {
        "decision" | "constraint" | "product_goal" => 5.0,
        "known_issue" | "task_state" => 3.0,
        _ => 1.0,
    };
    if memory.superseded_by.is_some() {
        score -= 8.0;
    }
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    score - (age_days / 14.0).min(4.0)
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

fn rank_context_rows(
    rows: &mut [Memory],
    task: &str,
    requested_scope: Option<&str>,
    rules: Option<&Path>,
) {
    let task_terms = tokenize(task);
    let rhai = rules.and_then(|path| load_rhai_rules(path).ok());
    rows.sort_by(|a, b| {
        let a_score = context_score(a, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), a, task).unwrap_or(0.0);
        let b_score = context_score(b, &task_terms, requested_scope)
            + rhai_score(rhai.as_ref(), b, task).unwrap_or(0.0);
        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn context_score(
    memory: &Memory,
    task_terms: &HashSet<String>,
    requested_scope: Option<&str>,
) -> f64 {
    let mut score = memory.confidence * 10.0;
    score += match memory.memory_type.as_str() {
        "decision" => 8.0,
        "product_goal" | "constraint" => 7.0,
        "user_preference" | "known_issue" => 6.0,
        "design_note" | "domain_fact" => 5.0,
        "command" | "task_state" => 4.0,
        _ => 2.0,
    };
    score += match memory.status.as_str() {
        "active" => 5.0,
        "uncertain" => 1.0,
        _ => -10.0,
    };
    if let Some(scope) = requested_scope {
        if memory.scope == scope {
            score += 4.0;
        }
    } else {
        score += match memory.scope.as_str() {
            "project" | "repo" => 3.0,
            "user" | "global" => 2.0,
            "thread" | "task" => 1.0,
            _ => 0.0,
        };
    }
    let haystack = tokenize(&format!("{} {}", memory.title, memory.body));
    let overlap = task_terms.intersection(&haystack).count() as f64;
    score += overlap * 2.0;
    let age_days = ((now_ms() - memory.updated_at).max(0) as f64) / 86_400_000.0;
    score -= (age_days / 30.0).min(3.0);
    score
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

#[derive(Debug, Serialize)]
struct SuggestedMemory {
    #[serde(rename = "type")]
    memory_type: String,
    title: String,
    body: String,
    confidence: f64,
}

fn suggest_from_file(
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

fn ingest_transcript(
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

#[derive(Debug, Serialize)]
struct AutoIngestReport {
    scanned: usize,
    ingested: usize,
    skipped: usize,
    inbox_added: usize,
    files: Vec<AutoIngestFile>,
}

#[derive(Debug, Serialize)]
struct AutoIngestFile {
    path: String,
    status: String,
    suggestions: usize,
}

struct AutoIngestPrintRequest<'a> {
    input: &'a Path,
    scope: &'a str,
    llm: bool,
    endpoint: &'a str,
    model: &'a str,
    dry_run: bool,
    json: bool,
}

fn print_auto_ingest(conn: &Connection, request: AutoIngestPrintRequest<'_>) -> Result<()> {
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

fn auto_ingest_sessions(
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

fn suggest_from_llm(endpoint: &str, model: &str, text: &str) -> Result<Vec<SuggestedMemory>> {
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

fn print_inbox(conn: &Connection, status: &str, limit: usize, json_out: bool) -> Result<()> {
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

fn list_inbox(conn: &Connection, status: &str, limit: usize) -> Result<Vec<InboxItem>> {
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

fn get_inbox_item(conn: &Connection, id: &str) -> Result<InboxItem> {
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

fn approve_inbox(conn: &Connection, id: &str, allow_sensitive: bool) -> Result<String> {
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

fn reject_inbox(conn: &Connection, id: &str) -> Result<()> {
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

fn suggest_from_text(text: &str) -> Vec<SuggestedMemory> {
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

fn truncate_words(text: &str, count: usize) -> String {
    let words = text.split_whitespace().take(count).collect::<Vec<_>>();
    let mut out = words.join(" ");
    if text.split_whitespace().count() > count {
        out.push_str("...");
    }
    out
}

fn one_line_summary(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
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

fn compact_task_state(conn: &Connection, scope: &str, limit: usize, dry_run: bool) -> Result<()> {
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

fn compact_v2(conn: &Connection, scope: &str, limit: usize, dry_run: bool) -> Result<()> {
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

fn render_codegraph_hints(rows: &[Memory], task: &str, root: &Path) -> String {
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

fn run_codegraph_node(root: &Path, symbol: &str, max_chars: usize) -> Result<String> {
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

fn render_context_pack(conn: &Connection, rows: &[Memory], max_chars: usize) -> Result<String> {
    if rows.is_empty() {
        return Ok("Relevant Memory:\n- none".to_string());
    }
    let mut out = String::from("Relevant Memory:");
    for (title, group) in grouped_memories(rows) {
        let heading = format!("\n\n{title}:");
        if out.len() + heading.len() > max_chars {
            break;
        }
        out.push_str(&heading);
        for row in group {
            let card = format_compact_card(conn, row)?;
            if out.len() + card.len() + 1 > max_chars {
                return Ok(out);
            }
            out.push('\n');
            out.push_str(&card);
        }
    }
    Ok(out)
}

fn render_retrieval_pack(hits: &[RetrievalHit], max_chars: usize) -> Result<String> {
    if hits.is_empty() {
        return Ok("Relevant Memory:\n- none".to_string());
    }
    let mut rows = hits
        .iter()
        .map(|hit| &hit.memory.memory)
        .collect::<Vec<_>>();
    rows.sort_by_key(|a| memory_group_order(a));
    let mut out = String::from("Relevant Memory:");
    let mut current_group = "";
    for row in rows {
        let group = memory_group_title(row);
        if group != current_group {
            let heading = format!("\n\n{group}:");
            if out.len() + heading.len() > max_chars {
                break;
            }
            out.push_str(&heading);
            current_group = group;
        }
        let body = row.body.split_whitespace().collect::<Vec<_>>().join(" ");
        let card = format!(
            "- {}:{} [{}] {} -- {}",
            row.memory_type, row.status, row.scope, row.title, body
        );
        if out.len() + card.len() + 1 > max_chars {
            break;
        }
        out.push('\n');
        out.push_str(&card);
    }
    Ok(out)
}

fn grouped_memories(rows: &[Memory]) -> Vec<(&'static str, Vec<&Memory>)> {
    let mut groups: Vec<(&'static str, Vec<&Memory>)> = vec![
        ("Decisions", Vec::new()),
        ("Constraints", Vec::new()),
        ("Current Facts", Vec::new()),
        ("Risks", Vec::new()),
        ("Recent Work", Vec::new()),
        ("Other", Vec::new()),
    ];
    for row in rows {
        let index = memory_group_order(row);
        groups[index].1.push(row);
    }
    groups
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

fn memory_group_order(memory: &Memory) -> usize {
    match memory.memory_type.as_str() {
        "decision" => 0,
        "constraint" | "product_goal" | "user_preference" => 1,
        "domain_fact" | "design_note" | "command" => 2,
        "known_issue" => 3,
        "task_state" => 4,
        _ => 5,
    }
}

fn memory_group_title(memory: &Memory) -> &'static str {
    match memory_group_order(memory) {
        0 => "Decisions",
        1 => "Constraints",
        2 => "Current Facts",
        3 => "Risks",
        4 => "Recent Work",
        _ => "Other",
    }
}

struct AgentContextRequest<'a> {
    task: &'a str,
    mode: ContextMode,
    limit: usize,
    max_chars: usize,
    json_out: bool,
    provider: &'a str,
    endpoint: &'a str,
    model: &'a str,
    format: OutputFormat,
    rules: Option<&'a Path>,
}

fn print_agent_context(conn: &Connection, request: AgentContextRequest<'_>) -> Result<()> {
    let statuses = ["active".to_string(), "uncertain".to_string()];
    let include_recent = match request.mode {
        ContextMode::Fast => 2,
        ContextMode::Agent => 4,
        ContextMode::Deep => 8,
    };
    let mut rows = build_context_rows(
        conn,
        ContextQuery {
            task: request.task,
            types: &[],
            statuses: &statuses,
            scope: None,
            limit: request.limit,
            include_recent,
            rules: request.rules,
        },
    )?;
    if !matches!(request.mode, ContextMode::Fast)
        && embeddings::semantic_index_ready(conn, request.provider, request.endpoint, request.model)
            .unwrap_or(false)
        && let Ok(semantic_rows) = embeddings::semantic_search(
            conn,
            request.provider,
            request.endpoint,
            request.model,
            request.task,
            request.limit,
        )
    {
        for item in semantic_rows {
            if !rows
                .iter()
                .any(|existing| existing.id == item.memory.memory.id)
            {
                rows.push(item.memory.memory);
            }
        }
        rank_context_rows(&mut rows, request.task, None, request.rules);
        rows.truncate(request.limit);
    }
    if request.json_out || matches!(request.format, OutputFormat::Json) {
        let full = rows
            .iter()
            .map(|m| get_memory_with_links(conn, &m.id))
            .collect::<Result<Vec<_>>>()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": format!("{:?}", request.mode).to_lowercase(),
                "task": request.task,
                "memories": full
            }))?
        );
        return Ok(());
    }
    if matches!(request.format, OutputFormat::Markdown | OutputFormat::Agent) {
        return print_memory_output(
            conn,
            &rows,
            request.format,
            request.max_chars,
            "Agent Context",
        );
    }
    let mut out = String::from("Agent Context\n");
    out.push_str(&render_context_pack(conn, &rows, request.max_chars)?);
    if matches!(request.mode, ContextMode::Agent | ContextMode::Deep) {
        out.push_str("\n\nNext Actions:\n");
        for row in query_memories(
            conn,
            None,
            &["task_state".to_string()],
            &["active".to_string()],
            None,
            5,
        )? {
            out.push_str("- ");
            out.push_str(&row.title);
            out.push('\n');
        }
    }
    if matches!(request.mode, ContextMode::Deep) {
        out.push_str(&render_codegraph_hints(&rows, request.task, Path::new(".")));
    }
    println!("{out}");
    Ok(())
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

use crate::build_info::BuildInfo;
use crate::http_api::HttpResponse;
use crate::runtime_config::{
    AgentConfig, load_runtime_config, parse_agent_config_with_compat_defaults,
};
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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
const DEFAULT_EMBED_ENDPOINT: &str = "local";
const DEFAULT_EMBED_MODEL: &str = "paraphrase-multilingual-MiniLM-L12-v2";
const DEFAULT_EMBED_PROVIDER: &str = "local";
const DEFAULT_INSTALL_BACKUP_KEEP: usize = 3;
const CURRENT_SCHEMA_VERSION: i64 = 19;
const EXPORT_VERSION: u32 = 1;
const VALID_SCOPES: &[&str] = &["global", "user", "project", "repo", "thread", "task"];

mod autonomous;
mod cli;
mod db;
mod diagnostics;
mod dispatch;
mod embeddings;
mod explain;
mod generation;
mod graph_rag;
mod http_server;
mod local_embed;
mod local_generation;
mod maintenance;
mod mcp_server;
mod memory;
mod model;
mod observability;
mod onboard;
mod ops;
mod project;
mod rag;
pub(crate) mod rag_ingest;
mod release_ops;
mod retrieval;
mod shared;
mod sync_planning;
mod sync_transport;
mod topology;
mod vec_backend;
use autonomous::*;
use cli::*;
use db::*;
use diagnostics::*;
pub(crate) use dispatch::run;
use maintenance::*;
use memory::*;
use model::*;
use observability::*;
use project::*;
use rag::*;
use rag_ingest::*;
use retrieval::*;
use shared::*;
use sync_planning::*;
use sync_transport::*;
use vec_backend::*;

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
        parse_agent_config_with_compat_defaults(&raw, provider, endpoint, model)
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
    #[serde(default)]
    generation: String,
    #[serde(default)]
    parent_generation: Option<String>,
}

#[derive(Debug, Serialize)]
struct SyncExportReport {
    version: u32,
    ok: bool,
    dry_run: bool,
    output: String,
    memory_count: usize,
    redacted: bool,
    encrypted: bool,
    encryption_mode: Option<String>,
    export_sha256: String,
    generation: String,
    parent_generation: Option<String>,
    bytes: usize,
    wrote: bool,
    previous_bundle: Option<String>,
    lock_wait_ms: u128,
    stale_lock_recovered: bool,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SyncConflictItem {
    id: String,
    title: String,
    local_updated_at: i64,
    incoming_updated_at: i64,
    resolution: String,
}

#[derive(Debug, Serialize)]
struct SyncImportPlan {
    policy: SyncConflictPolicy,
    incoming_count: usize,
    insert_count: usize,
    update_count: usize,
    skip_count: usize,
    conflicts: Vec<SyncConflictItem>,
    selected_ids: HashSet<String>,
    blocked: bool,
}

#[derive(Debug, Serialize)]
struct SyncImportReport {
    version: u32,
    ok: bool,
    dry_run: bool,
    input: String,
    replace: bool,
    policy: SyncConflictPolicy,
    memory_count: usize,
    encrypted: bool,
    encryption_mode: Option<String>,
    export_sha256: Option<String>,
    generation: Option<String>,
    parent_generation: Option<String>,
    checksum_ok: Option<bool>,
    rollback: Option<String>,
    imported: usize,
    insert_count: usize,
    update_count: usize,
    skip_count: usize,
    conflict_count: usize,
    conflicts: Vec<SyncConflictItem>,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SyncRemoteStatusReport {
    version: u32,
    ok: bool,
    target: String,
    bundle: String,
    exists: bool,
    encrypted: bool,
    encryption_mode: Option<String>,
    verified: bool,
    memory_count: Option<usize>,
    export_sha256: Option<String>,
    generation: Option<String>,
    parent_generation: Option<String>,
    updated_at: Option<i64>,
    previous_bundle: String,
    recovery_available: bool,
    corrupt: bool,
    error: Option<String>,
    lock_active: bool,
    lock_age_ms: Option<i64>,
    lock_stale: bool,
    last_seen_generation: Option<String>,
    stale_remote: Option<bool>,
    local_first: bool,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SyncRecoveryReport {
    version: u32,
    ok: bool,
    target: String,
    bundle: String,
    restored_from: String,
    corrupt_archive: Option<String>,
    generation: String,
    lock_wait_ms: u128,
    stale_lock_recovered: bool,
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
            generation: Uuid::new_v4().to_string(),
            parent_generation: None,
        },
        export,
    })
}

struct PreparedSyncPayload {
    bundle: SyncBundle,
    bytes: Vec<u8>,
    encrypted: bool,
}

fn prepare_sync_payload(
    conn: &Connection,
    redact: bool,
    encrypt: bool,
) -> Result<PreparedSyncPayload> {
    prepare_sync_payload_with_parent(conn, redact, encrypt, None)
}

fn prepare_sync_payload_with_parent(
    conn: &Connection,
    redact: bool,
    encrypt: bool,
    parent_generation: Option<String>,
) -> Result<PreparedSyncPayload> {
    let mut bundle = sync_bundle(conn, redact)?;
    bundle.manifest.parent_generation = parent_generation;
    let plaintext = serde_json::to_vec_pretty(&bundle)?;
    let bytes = if encrypt {
        encrypt_sync_payload(&plaintext)?
    } else {
        plaintext
    };
    Ok(PreparedSyncPayload {
        bundle,
        bytes,
        encrypted: encrypt,
    })
}

fn parse_sync_input(input: &Path) -> Result<(MemoryExport, Option<SyncBundleManifest>, bool)> {
    let raw = fs::read(input).with_context(|| format!("failed to read {}", input.display()))?;
    let encrypted = is_encrypted_sync_payload(&raw);
    let plaintext = if encrypted {
        decrypt_sync_payload(&raw)?
    } else {
        raw
    };
    let value: Value = serde_json::from_slice(&plaintext)
        .with_context(|| format!("failed to parse sync bundle {}", input.display()))?;
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
        return Ok((bundle.export, Some(bundle.manifest), encrypted));
    }
    let export: MemoryExport = serde_json::from_value(value)?;
    Ok((export, None, encrypted))
}

fn sync_import_plan(
    conn: &Connection,
    export: &MemoryExport,
    policy: SyncConflictPolicy,
    replace: bool,
) -> Result<SyncImportPlan> {
    let mut selected_ids = HashSet::new();
    let mut conflicts = Vec::new();
    let mut insert_count = 0;
    let mut update_count = 0;
    let mut skip_count = 0;
    for item in &export.memories {
        let local = conn
            .query_row(
                "SELECT title, body, status, updated_at FROM memories WHERE id = ?1",
                params![item.memory.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((local_title, local_body, local_status, local_updated_at)) = local else {
            insert_count += 1;
            selected_ids.insert(item.memory.id.clone());
            continue;
        };
        let changed = replace
            || local_title != item.memory.title
            || local_body != item.memory.body
            || local_status != item.memory.status
            || local_updated_at != item.memory.updated_at;
        if !changed {
            skip_count += 1;
            continue;
        }
        let resolution = match policy {
            SyncConflictPolicy::LocalWins => "skip_local_wins",
            SyncConflictPolicy::RemoteWins => "apply_remote_wins",
            SyncConflictPolicy::NewerWins => {
                if item.memory.updated_at > local_updated_at {
                    "apply_incoming_newer"
                } else {
                    "skip_local_newer_or_equal"
                }
            }
            SyncConflictPolicy::Manual => "manual_required",
        }
        .to_string();
        if matches!(
            policy,
            SyncConflictPolicy::RemoteWins | SyncConflictPolicy::NewerWins
        ) && resolution.starts_with("apply")
        {
            update_count += 1;
            selected_ids.insert(item.memory.id.clone());
        } else if matches!(
            policy,
            SyncConflictPolicy::LocalWins | SyncConflictPolicy::Manual
        ) || resolution.starts_with("skip")
        {
            skip_count += 1;
        }
        conflicts.push(SyncConflictItem {
            id: item.memory.id.clone(),
            title: item.memory.title.clone(),
            local_updated_at,
            incoming_updated_at: item.memory.updated_at,
            resolution,
        });
    }
    Ok(SyncImportPlan {
        policy,
        incoming_count: export.memories.len(),
        insert_count,
        update_count,
        skip_count,
        blocked: policy == SyncConflictPolicy::Manual && !conflicts.is_empty(),
        conflicts,
        selected_ids,
    })
}

fn filtered_export_for_plan(export: MemoryExport, plan: &SyncImportPlan) -> MemoryExport {
    MemoryExport {
        version: export.version,
        exported_at: export.exported_at,
        memories: export
            .memories
            .into_iter()
            .filter(|item| plan.selected_ids.contains(&item.memory.id))
            .collect(),
    }
}

fn import_memories(conn: &Connection, input: &Path, replace: bool) -> Result<()> {
    let (export, _, _) = parse_sync_input(input)?;
    import_memory_export(conn, export, replace).map(|count| {
        println!("imported: {count}");
    })
}

fn import_memory_export(conn: &Connection, export: MemoryExport, replace: bool) -> Result<usize> {
    if export.version != EXPORT_VERSION {
        bail!("unsupported export version: {}", export.version);
    }
    transactional(conn, "import_memory_export", || {
        if replace {
            conn.execute("DELETE FROM memories", [])?;
        }
        let memories = export.memories;
        for item in &memories {
            conn.execute(
                r#"
                INSERT INTO memories (
                    id, type, scope, title, body, status, source,
                    created_at, updated_at, supersedes, superseded_by, confidence, layer
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10, ?11)
                ON CONFLICT(id) DO UPDATE SET
                    type = excluded.type,
                    scope = excluded.scope,
                    title = excluded.title,
                    body = excluded.body,
                    status = excluded.status,
                    source = excluded.source,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    supersedes = NULL,
                    superseded_by = NULL,
                    confidence = excluded.confidence,
                    layer = excluded.layer
                "#,
                params![
                    &item.memory.id,
                    &item.memory.memory_type,
                    &item.memory.scope,
                    &item.memory.title,
                    &item.memory.body,
                    &item.memory.status,
                    &item.memory.source,
                    item.memory.created_at,
                    item.memory.updated_at,
                    item.memory.confidence,
                    &item.memory.layer,
                ],
            )?;
        }
        for item in &memories {
            conn.execute(
                r#"
                UPDATE memories
                SET supersedes = (SELECT id FROM memories WHERE id = ?2),
                    superseded_by = (SELECT id FROM memories WHERE id = ?3)
                WHERE id = ?1
                "#,
                params![
                    &item.memory.id,
                    &item.memory.supersedes,
                    &item.memory.superseded_by,
                ],
            )?;
            conn.execute(
                "DELETE FROM memory_links WHERE memory_id = ?1",
                params![&item.memory.id],
            )?;
            insert_links(conn, &item.memory.id, &item.links)?;
        }
        Ok(memories.len())
    })
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

fn ensure_vector_backend(conn: &Connection, backend: VectorBackend) -> Result<Option<String>> {
    match backend {
        VectorBackend::Json => Ok(None),
        VectorBackend::SqliteVec => {
            if !cfg!(feature = "vec") {
                bail!(
                    "sqlite-vec backend requested, but this binary was built without --features vec"
                );
            }
            sqlite_vec_probe(conn).map(Some)
        }
    }
}

fn vec_validate(conn: &Connection, backend: VectorBackend) -> Result<()> {
    let sqlite_vec_version = ensure_vector_backend(conn, backend)?;
    let detail = match backend {
        VectorBackend::Json => {
            "validated JSON embedding storage with application-side cosine search".to_string()
        }
        VectorBackend::SqliteVec => {
            let report = sqlite_vec_index_report(conn)?;
            if !report.consistent {
                bail!("persistent sqlite-vec index is inconsistent; run vec-index --rebuild");
            }
            format!(
                "validated bundled sqlite-vec {} with native SQL cosine search and vec0 KNN; {} persistent index(es) consistent",
                sqlite_vec_version.as_deref().unwrap_or("unknown"),
                report.indexes.len()
            )
        }
    };
    log_event(conn, "vec_validate", None, &detail)?;
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
            encrypt,
            dry_run,
            json,
        } => {
            let prepared = prepare_sync_payload(conn, redact, encrypt)?;
            let report = SyncExportReport {
                version: 1,
                ok: true,
                dry_run,
                output: output.display().to_string(),
                memory_count: prepared.bundle.manifest.memory_count,
                redacted: redact,
                encrypted: prepared.encrypted,
                encryption_mode: prepared.encrypted.then(|| SYNC_ENCRYPTION_MODE.to_string()),
                export_sha256: prepared.bundle.manifest.export_sha256.clone(),
                generation: sync_manifest_generation(&prepared.bundle.manifest),
                parent_generation: prepared.bundle.manifest.parent_generation.clone(),
                bytes: prepared.bytes.len(),
                wrote: !dry_run,
                previous_bundle: None,
                lock_wait_ms: 0,
                stale_lock_recovered: false,
                recommendations: vec![
                    "import with dukememory sync import --dry-run before applying".to_string(),
                    "keep agent reads local-first; use this bundle for backup/sync only"
                        .to_string(),
                ],
            };
            if !dry_run {
                write_private_atomic(&output, &prepared.bytes)?;
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
            policy,
            dry_run,
            json,
        } => {
            let (export, manifest, encrypted) = parse_sync_input(&input)?;
            let plan = sync_import_plan(conn, &export, policy, replace)?;
            let memory_count = export.memories.len();
            let export_sha256 = manifest
                .as_ref()
                .map(|manifest| manifest.export_sha256.clone());
            let generation = manifest.as_ref().map(sync_manifest_generation);
            let parent_generation = manifest
                .as_ref()
                .and_then(|manifest| manifest.parent_generation.clone());
            let blocked = plan.blocked;
            let rollback = if dry_run || blocked {
                None
            } else {
                let rollback_dir = sync_rollback_dir(conn)?;
                let extension = if encrypted { "age" } else { "json" };
                let rollback_path = rollback_dir.join(format!("sync-{}.{}", now_ms(), extension));
                let rollback_export = export_memories(conn, &[], &[], None)?;
                let rollback_plaintext = serde_json::to_vec_pretty(&rollback_export)?;
                let rollback_bytes = if encrypted {
                    encrypt_sync_payload(&rollback_plaintext)?
                } else {
                    rollback_plaintext
                };
                write_private_atomic(&rollback_path, &rollback_bytes)?;
                Some(rollback_path.display().to_string())
            };
            let imported = if dry_run || blocked {
                0
            } else {
                let filtered = filtered_export_for_plan(export, &plan);
                import_memory_export(conn, filtered, replace)?
            };
            let report = SyncImportReport {
                version: 1,
                ok: !blocked,
                dry_run,
                input: input.display().to_string(),
                replace,
                policy,
                memory_count,
                encrypted,
                encryption_mode: encrypted.then(|| SYNC_ENCRYPTION_MODE.to_string()),
                export_sha256,
                generation,
                parent_generation,
                checksum_ok: manifest.as_ref().map(|_| true),
                rollback,
                imported,
                insert_count: plan.insert_count,
                update_count: plan.update_count,
                skip_count: plan.skip_count,
                conflict_count: plan.conflicts.len(),
                conflicts: plan.conflicts,
                recommendations: if dry_run {
                    vec![
                        "rerun without --dry-run only after reviewing memory_count and checksum"
                            .to_string(),
                    ]
                } else if blocked {
                    vec![
                        "resolve conflicts or rerun with --policy local-wins|remote-wins|newer-wins"
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
            } else if blocked {
                println!("sync import blocked: manual conflict resolution required");
                println!("conflicts: {}", report.conflict_count);
            } else {
                println!("imported: {imported}");
                if let Some(rollback) = report.rollback {
                    println!("rollback: {rollback}");
                }
            }
            Ok(())
        }
        SyncCommand::Push {
            target,
            redact,
            encrypt,
            dry_run,
            force,
            json,
        } => {
            let bundle_path = sync_target_bundle_path(&target, encrypt);
            let existing_bundle = sync_target_bundle_path_for_read(&target);
            let target_lock = if dry_run {
                None
            } else {
                Some(acquire_sync_target_lock(&target)?)
            };
            let lock_wait_ms = target_lock.as_ref().map_or(0, |lock| lock.wait_ms);
            let stale_lock_recovered = target_lock
                .as_ref()
                .is_some_and(|lock| lock.stale_recovered);
            let local_generation = sync_peer_generation(conn, &target)?;
            let inspect_remote = || -> Result<Option<SyncBundleManifest>> {
                if !existing_bundle.exists() {
                    return Ok(None);
                }
                let (_, manifest, _) = parse_sync_input(&existing_bundle)?;
                manifest
                    .ok_or_else(|| anyhow::anyhow!("remote is a legacy export, not a sync bundle"))
                    .map(Some)
            };
            let remote_manifest = match inspect_remote() {
                Ok(manifest) => manifest,
                Err(error) if force => {
                    let _ = error;
                    None
                }
                Err(error) => {
                    bail!(
                        "remote sync bundle cannot be verified: {error}; run sync status/recover or rerun push with --force"
                    )
                }
            };
            let remote_generation = remote_manifest.as_ref().map(sync_manifest_generation);
            let stale_remote = if existing_bundle.exists() {
                local_generation.as_deref() != remote_generation.as_deref()
            } else {
                local_generation.is_some()
            };
            if stale_remote && !force {
                bail!(
                    "stale or untracked remote generation (local={:?}, remote={:?}); pull and merge first or rerun with --force",
                    local_generation,
                    remote_generation
                );
            }
            let prepared =
                prepare_sync_payload_with_parent(conn, redact, encrypt, remote_generation.clone())?;
            let mut previous_bundle = None;
            if !dry_run {
                if existing_bundle.exists() {
                    if remote_manifest.is_some() {
                        previous_bundle = preserve_previous_sync_bundle(&existing_bundle)?
                            .map(|path| path.display().to_string());
                    } else {
                        let archive = sync_corrupt_archive_path(&existing_bundle);
                        let bytes = fs::read(&existing_bundle)?;
                        write_private_atomic(&archive, &bytes)?;
                        previous_bundle = Some(archive.display().to_string());
                    }
                }
                write_private_atomic(&bundle_path, &prepared.bytes)?;
                let (_, stored_manifest, stored_encrypted) = parse_sync_input(&bundle_path)?;
                let stored_manifest = stored_manifest
                    .ok_or_else(|| anyhow::anyhow!("written remote is not a sync bundle"))?;
                if stored_encrypted != encrypt
                    || stored_manifest.export_sha256 != prepared.bundle.manifest.export_sha256
                    || sync_manifest_generation(&stored_manifest)
                        != sync_manifest_generation(&prepared.bundle.manifest)
                {
                    bail!("remote sync read-back verification failed");
                }
                if existing_bundle != bundle_path && existing_bundle.exists() {
                    fs::remove_file(&existing_bundle).with_context(|| {
                        format!(
                            "failed to remove superseded bundle {}",
                            existing_bundle.display()
                        )
                    })?;
                }
                record_sync_peer_generation(
                    conn,
                    &target,
                    &sync_manifest_generation(&prepared.bundle.manifest),
                    "push",
                )?;
            }
            let report = SyncExportReport {
                version: 1,
                ok: true,
                dry_run,
                output: bundle_path.display().to_string(),
                memory_count: prepared.bundle.manifest.memory_count,
                redacted: redact,
                encrypted: prepared.encrypted,
                encryption_mode: prepared.encrypted.then(|| SYNC_ENCRYPTION_MODE.to_string()),
                export_sha256: prepared.bundle.manifest.export_sha256.clone(),
                generation: sync_manifest_generation(&prepared.bundle.manifest),
                parent_generation: prepared.bundle.manifest.parent_generation.clone(),
                bytes: prepared.bytes.len(),
                wrote: !dry_run,
                previous_bundle,
                lock_wait_ms,
                stale_lock_recovered,
                recommendations: vec![
                    "run dukememory sync status TARGET after push".to_string(),
                    "remote connector is local-first; agents should still read local memory"
                        .to_string(),
                ],
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", bundle_path.display());
            }
            Ok(())
        }
        SyncCommand::Pull {
            target,
            policy,
            dry_run,
            json,
        } => {
            let bundle_path = sync_target_bundle_path_for_read(&target);
            let (remote_export, remote_manifest, _) = parse_sync_input(&bundle_path)?;
            let plan = sync_import_plan(conn, &remote_export, policy, false)?;
            handle_sync(
                conn,
                SyncCommand::Import {
                    input: bundle_path.clone(),
                    replace: false,
                    policy,
                    dry_run,
                    json,
                },
            )?;
            if !dry_run
                && !plan.blocked
                && let Some(manifest) = remote_manifest.as_ref()
            {
                record_sync_peer_generation(
                    conn,
                    &target,
                    &sync_manifest_generation(manifest),
                    "pull",
                )?;
            }
            Ok(())
        }
        SyncCommand::Status { target, json } => {
            let report = sync_remote_status(conn, &target)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("target: {}", report.target);
                println!("bundle: {}", report.bundle);
                println!("exists: {}", report.exists);
                if let Some(count) = report.memory_count {
                    println!("memories: {count}");
                }
            }
            Ok(())
        }
        SyncCommand::Recover { target, json } => {
            let current_bundle = sync_target_bundle_path_for_read(&target);
            let previous = sync_recovery_candidate(&target, &current_bundle);
            if !previous.exists() {
                bail!("no previous sync generation found for {}", target.display());
            }
            let target_lock = acquire_sync_target_lock(&target)?;
            let (_, manifest, previous_encrypted) =
                parse_sync_input(&previous).with_context(|| {
                    format!(
                        "previous sync generation is invalid: {}",
                        previous.display()
                    )
                })?;
            let manifest = manifest
                .ok_or_else(|| anyhow::anyhow!("previous file is not a versioned sync bundle"))?;
            let bundle = if current_bundle.exists() {
                current_bundle
            } else {
                sync_target_bundle_path(&target, previous_encrypted)
            };
            let corrupt_archive = if bundle.exists() {
                let archive = sync_corrupt_archive_path(&bundle);
                write_private_atomic(&archive, &fs::read(&bundle)?)?;
                Some(archive)
            } else {
                None
            };
            let bytes = fs::read(&previous)?;
            write_private_atomic(&bundle, &bytes)?;
            let (_, restored_manifest, _) = parse_sync_input(&bundle)?;
            let restored_manifest = restored_manifest
                .ok_or_else(|| anyhow::anyhow!("restored file is not a sync bundle"))?;
            let generation = sync_manifest_generation(&restored_manifest);
            if generation != sync_manifest_generation(&manifest) {
                bail!("restored sync generation failed read-back verification");
            }
            record_sync_peer_generation(conn, &target, &generation, "recover")?;
            let report = SyncRecoveryReport {
                version: 1,
                ok: true,
                target: target.display().to_string(),
                bundle: bundle.display().to_string(),
                restored_from: previous.display().to_string(),
                corrupt_archive: corrupt_archive.map(|path| path.display().to_string()),
                generation,
                lock_wait_ms: target_lock.wait_ms,
                stale_lock_recovered: target_lock.stale_recovered,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("recovered: {}", report.bundle);
                println!("generation: {}", report.generation);
            }
            Ok(())
        }
    }
}

fn sync_rollback_dir(conn: &Connection) -> Result<PathBuf> {
    let database: String = conn
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    if database.is_empty() {
        return Ok(PathBuf::from(".agent/sync-rollbacks"));
    }
    let parent = Path::new(&database)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    if parent.file_name().and_then(|value| value.to_str()) == Some(".agent") {
        Ok(parent.join("sync-rollbacks"))
    } else {
        Ok(parent.join(".agent/sync-rollbacks"))
    }
}

fn sync_target_bundle_path(target: &Path, encrypted: bool) -> PathBuf {
    if matches!(
        target.extension().and_then(|value| value.to_str()),
        Some("json" | "age")
    ) {
        target.to_path_buf()
    } else if encrypted {
        target.join("dukememory-sync-bundle.age")
    } else {
        target.join("dukememory-sync-bundle.json")
    }
}

fn sync_target_bundle_path_for_read(target: &Path) -> PathBuf {
    if target.extension().is_some() {
        return target.to_path_buf();
    }
    let encrypted = sync_target_bundle_path(target, true);
    if encrypted.exists() {
        encrypted
    } else {
        sync_target_bundle_path(target, false)
    }
}

fn sync_previous_bundle_path(bundle: &Path) -> PathBuf {
    let parent = bundle.parent().unwrap_or_else(|| Path::new("."));
    let stem = bundle
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("dukememory-sync-bundle");
    match bundle.extension().and_then(|value| value.to_str()) {
        Some(extension) => parent.join(format!("{stem}.previous.{extension}")),
        None => parent.join(format!("{stem}.previous")),
    }
}

fn sync_corrupt_archive_path(bundle: &Path) -> PathBuf {
    let parent = bundle.parent().unwrap_or_else(|| Path::new("."));
    let stem = bundle
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("dukememory-sync-bundle");
    match bundle.extension().and_then(|value| value.to_str()) {
        Some(extension) => parent.join(format!("{stem}.corrupt-{}.{}", now_ms(), extension)),
        None => parent.join(format!("{stem}.corrupt-{}", now_ms())),
    }
}

fn sync_manifest_generation(manifest: &SyncBundleManifest) -> String {
    if manifest.generation.trim().is_empty() {
        format!(
            "legacy-{}",
            &manifest.export_sha256[..16.min(manifest.export_sha256.len())]
        )
    } else {
        manifest.generation.clone()
    }
}

fn sync_target_key(target: &Path) -> String {
    if let Ok(canonical) = target.canonicalize() {
        return canonical.display().to_string();
    }
    if target.is_absolute() {
        target.display().to_string()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(target)
            .display()
            .to_string()
    }
}

fn sync_peer_generation(conn: &Connection, target: &Path) -> Result<Option<String>> {
    conn.query_row(
        "SELECT last_seen_generation FROM sync_peer_state WHERE target = ?1",
        [sync_target_key(target)],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn record_sync_peer_generation(
    conn: &Connection,
    target: &Path,
    generation: &str,
    operation: &str,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO sync_peer_state(target, last_seen_generation, last_operation, updated_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(target) DO UPDATE SET
            last_seen_generation = excluded.last_seen_generation,
            last_operation = excluded.last_operation,
            updated_at = excluded.updated_at
        "#,
        params![sync_target_key(target), generation, operation, now_ms()],
    )?;
    Ok(())
}

fn preserve_previous_sync_bundle(bundle: &Path) -> Result<Option<PathBuf>> {
    if !bundle.exists() {
        return Ok(None);
    }
    let previous = sync_previous_bundle_path(bundle);
    let bytes = fs::read(bundle)
        .with_context(|| format!("failed to read previous sync bundle {}", bundle.display()))?;
    write_private_atomic(&previous, &bytes)?;
    Ok(Some(previous))
}

fn sync_recovery_candidate(target: &Path, bundle: &Path) -> PathBuf {
    let direct = sync_previous_bundle_path(bundle);
    if direct.exists() || target.extension().is_some() {
        return direct;
    }
    for encrypted in [true, false] {
        let candidate = sync_previous_bundle_path(&sync_target_bundle_path(target, encrypted));
        if candidate.exists() {
            return candidate;
        }
    }
    direct
}

fn sync_remote_status(conn: &Connection, target: &Path) -> Result<SyncRemoteStatusReport> {
    let bundle = sync_target_bundle_path_for_read(target);
    let previous = sync_recovery_candidate(target, &bundle);
    let local_generation = sync_peer_generation(conn, target)?;
    let (lock_active, lock_age_ms, lock_stale) = sync_target_lock_status(target);
    if !bundle.exists() {
        let recovery_available = previous.exists()
            && parse_sync_input(&previous)
                .ok()
                .and_then(|(_, manifest, _)| manifest)
                .is_some();
        return Ok(SyncRemoteStatusReport {
            version: 1,
            ok: recovery_available,
            target: target.display().to_string(),
            bundle: bundle.display().to_string(),
            exists: false,
            encrypted: false,
            encryption_mode: None,
            verified: false,
            memory_count: None,
            export_sha256: None,
            generation: None,
            parent_generation: None,
            updated_at: None,
            previous_bundle: previous.display().to_string(),
            recovery_available,
            corrupt: false,
            error: None,
            lock_active,
            lock_age_ms,
            lock_stale,
            last_seen_generation: local_generation.clone(),
            stale_remote: local_generation.as_ref().map(|_| true),
            local_first: true,
            recommendations: if recovery_available {
                vec!["run dukememory sync recover TARGET --json".to_string()]
            } else if local_generation.is_some() {
                vec![
                    "remote disappeared after a previously observed generation; recover it or push with --force"
                        .to_string(),
                ]
            } else {
                vec!["run dukememory sync push TARGET --json".to_string()]
            },
        });
    }
    let raw = fs::read(&bundle)
        .with_context(|| format!("failed to read sync bundle {}", bundle.display()))?;
    let encrypted = is_encrypted_sync_payload(&raw);
    let can_decrypt = !encrypted || sync_passphrase_is_configured();
    let (manifest, corrupt, error) = if !can_decrypt {
        (None, false, None)
    } else {
        match parse_sync_input(&bundle) {
            Ok((_, manifest, _)) => (manifest, false, None),
            Err(error) => (None, true, Some(error.to_string())),
        }
    };
    let recovery_available = previous.exists()
        && parse_sync_input(&previous)
            .ok()
            .and_then(|(_, manifest, _)| manifest)
            .is_some();
    let modified = fs::metadata(&bundle)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64);
    let generation = manifest.as_ref().map(sync_manifest_generation);
    let stale_remote = generation
        .as_ref()
        .map(|generation| local_generation.as_deref() != Some(generation.as_str()));
    let mut recommendations = Vec::new();
    if encrypted && !can_decrypt {
        recommendations.push(
            "configure the sync passphrase to verify checksum, generation, and recovery data"
                .to_string(),
        );
    }
    if corrupt {
        if recovery_available {
            recommendations.push("run dukememory sync recover TARGET --json".to_string());
        } else {
            recommendations.push(
                "remote is corrupt and has no verified previous generation; restore a backup or push with --force"
                    .to_string(),
            );
        }
    } else if stale_remote == Some(true) {
        recommendations.push(
            "remote generation is untracked or newer; pull and merge before the next push"
                .to_string(),
        );
    }
    if lock_active {
        recommendations.push(if lock_stale {
            "the sync lease is stale and the next guarded writer can recover it".to_string()
        } else {
            "wait for the active sync lease before writing the target".to_string()
        });
    }
    recommendations.push(
        "run dukememory sync pull TARGET --policy manual --dry-run --json before applying"
            .to_string(),
    );
    Ok(SyncRemoteStatusReport {
        version: 1,
        ok: !corrupt,
        target: target.display().to_string(),
        bundle: bundle.display().to_string(),
        exists: true,
        encrypted,
        encryption_mode: encrypted.then(|| SYNC_ENCRYPTION_MODE.to_string()),
        verified: manifest.is_some() && !corrupt,
        memory_count: manifest.as_ref().map(|manifest| manifest.memory_count),
        export_sha256: manifest
            .as_ref()
            .map(|manifest| manifest.export_sha256.clone()),
        generation,
        parent_generation: manifest
            .as_ref()
            .and_then(|manifest| manifest.parent_generation.clone()),
        updated_at: modified,
        previous_bundle: previous.display().to_string(),
        recovery_available,
        corrupt,
        error,
        lock_active,
        lock_age_ms,
        lock_stale,
        last_seen_generation: local_generation,
        stale_remote,
        local_first: true,
        recommendations,
    })
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
        layer: None,
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
        layer: None,
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
            layer: None,
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
    layer: Option<String>,
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
    layer: Option<String>,
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
                layer: None,
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
                layer: None,
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

Use `dukememory context-governor "<task>" --json` to choose the smallest useful read flow; add `--target <file-or-symbol>` before focused edits.

Use `dukememory budget-plan "<task>" --json` when unsure how much memory context is enough. Prefer the returned smallest useful profile.

Use `dukememory memory-router "<query>" --include-siblings --json` to route memory across nearby projects without treating other projects as authoritative.

Use `dukememory memory-health-score --json` to inspect one end-to-end project memory health score.

Use `dukememory explain-recall "<query>" --json` to explain why specific cards would be recalled.

Use `dukememory project-intent-map --json` to inspect goals, decisions, constraints, commands, risks, active tasks, and the compact contract.

Use `dukememory memory-test-harness --json` to run lightweight retrieval probes against durable memory.

Use `dukememory agent-audit-v2 --json` to audit read discipline, semantic effectiveness, write pressure, feedback, and explainability.

Use `dukememory memory-control-center-v2 --json` to aggregate health, intent, probes, audit, recall explanations, and autonomy.

Use `dukememory auto-supersede-v2 --json` to safely supersede duplicate/obsolete cards; use `--apply` only for high-confidence reversible status changes.

Use `dukememory memory-diff-apply --json` to write high-confidence changed-file memory candidates after review.

Use `dukememory recall-benchmark-suite --json` to detect retrieval regressions; use `--write-baseline` after reviewing stable probes.

Use `dukememory release-gate-v2 --json` to gate releases with health, recall benchmark, audit v2, and control-center checks.

Use `dukememory memory-effectiveness-v2 --json` to measure memory usefulness with influence, wasted reads, and semantic-read signals.

Use `dukememory recall-benchmark-baselines --json` to inspect or write guarded recall benchmark baselines; use `--apply` only after reviewing stable probes.

Use `dukememory memory-conflict-apply --json` to apply only guarded reversible conflict-review actions; use `--apply` after reviewing the dry-run.

Use `dukememory remote-sync-wizard --json` to configure local-first VDS/remote sync safely.

Use `dukememory memory-governance-policy --json` to inspect or write autonomous memory governance policy.

Use `dukememory autonomous-loop-v2 --json` to run the V2 autonomous memory loop with governance and quality gates; use `--apply` only when governance is ready.

Use `dukememory governance-enforce --json` to enforce autonomous memory governance; use `--apply` to log a clean enforcement pass.

Use `dukememory memory-quality-ci --json` to run a CI-friendly memory quality gate.

Use `dukememory fleet-dashboard-v2 --json` to inspect all discovered project memories with V2 quality metrics.

Use `dukememory remote-sync-apply-flow --json` to plan guarded remote sync apply; use `--target` and a mode-600 sync passphrase file before `--apply`.

Use `dukememory mcp-tool-surface-v2 --json` to inspect MCP V2 memory tool exposure.

Use `dukememory mcp-tool-surface-v3 --json` to inspect MCP V3 memory tool exposure.

Use `dukememory autopilot-v3 --json` to run the V3 autonomous memory autopilot across learning, role profile, inbox review, sync, web control, and MCP quality.

Use `dukememory self-learning-retrieval --json` to tune retrieval from live usefulness, feedback, quality, and ranking signals.

Use `dukememory project-role-profile --json` to detect project-specific memory defaults; use `--apply` after reviewing inferred kind.

Use `dukememory inbox-ai-reviewer --json` to explain inbox groups and safely process high-confidence suggestions.

Use `dukememory web-control-center-v3 --json` to inspect the simplified Health, Autonomy, Projects, and Sync control model.

Use `dukememory remote-sync-apply --json` to apply guarded local-first encrypted sync; use `--target` and a mode-600 sync passphrase file before `--apply`.

Use `dukememory mcp-quality-tools --json` to inspect MCP helper tools for memory discipline.

Use `dukememory remote-sync-control --json` to inspect local-first VDS/remote sync readiness, real push/pull dry-run commands, remote bundle status, and rollback hints.

Use `dukememory web-control-center-v4 --json` to inspect actionable Health, Autonomy, Projects, Sync, MCP, Feedback, and Upgrade controls for the web UI.

Use `dukememory mcp-discipline-v2 --json` to enforce agent startup, write-decision, and after-task memory discipline; use `--apply` to repair wiring.

Use `dukememory mcp-discipline-v3 --json` to enforce V3 startup, before-edit, and after-task memory discipline; use `--apply` only to record a clean verified pass.

Use `dukememory feedback-loop-v2 --json` to inspect autonomous usefulness feedback, safe supersede, diff apply candidates, and recall benchmark quality; use `--apply` after reviewing dry-run output.

Use `dukememory upgrade-all-projects-v2 --json` to inspect all installed project memories with richer version/action summaries before applying upgrades.

Use `dukememory fleet-quality --json` to inspect V3 quality across discovered project memories.

Use `dukememory vds-sync-pack --json` to inspect a local-first VDS sync pack with dry-run, apply, and verify commands; pass `--target PATH` before `--apply`.

Use `dukememory web-control-center-v5 --json` to inspect the 0.24 web control model with VDS sync pack, quality autopilot, router v2, benchmark profiles, and install polish.

Use `dukememory quality-autopilot-v31 --json` to inspect safe feedback, quality, cost, health, diff apply, supersede, and benchmark gates; use `--apply` only for reversible policy writes.

Use `dukememory memory-router-v2 "<query>" --include-siblings --json` to route cross-project memory while keeping writes pinned to the current project.

Use `dukememory benchmark-profiles --json` to select project-aware retrieval benchmark profiles; use `--write-baseline` only after reviewing probes.

Use `dukememory install-polish --json` to inspect README, screenshot, license, package metadata, and GitHub install readiness before release.

Use `dukememory memory-effectiveness-lab --json` to measure whether recent memory reads actually helped agent work.

Use `dukememory auto-context-budgeter-v2 "<task>" --json` to choose the smallest useful memory flow for the task; use `--apply` to write the selected policy.

Use `dukememory memory-contract-v2 --json` to inspect the compact project contract v2; use `--write` after releases or architecture changes.

Use `dukememory cross-project-learning "<query>" --json` to surface sibling-project hints without writing outside the current project.

Use `dukememory agent-trace --json` to inspect recent agent reads, influence, feedback, and durable writes.

Use `dukememory vds-sync-hardening --json` to verify local-first VDS sync target, latency, dry-runs, and rollback readiness.

Use `dukememory install-quality --json` to verify install, skill, AGENTS, doctor, and future-chat memory readiness.

Use `dukememory web-control-center-v6 --json` to inspect the 0.25 web control model with effectiveness, budgeter, contract v2, cross-project learning, trace, VDS hardening, and install quality.

Use `dukememory answer "<question>" --json` to answer from grounded project memory with citation ids and explicit gaps.

Use `dukememory connect-codex --json` to verify that Codex future chats are wired to project memory; use `--apply` to write the checked connection policy after doctor/enforce pass.

Use `dukememory memory-type-guide --json` to explain memory card types, filters, examples, and write guardrails.

Use `dukememory memory-eval-story --json` to inspect the reproducible local recall/effectiveness benchmark story; use `--write-baseline` only after reviewing probes.

Use `dukememory import-review FILE --json` to turn text into reviewed inbox candidates; use `--apply` only when the file is safe and durable.

Use `dukememory memory-upload FILE --json` to upload a local text/markdown/json/csv file into reviewed inbox candidates; use `--apply` only after reviewing the source.

Use `dukememory memanto-gap-report --json` to inspect Memanto-style remember, recall, answer, temporal, conflict, integration, local-first, and operations coverage without changing memory.

Use `dukememory web-control-center-v7 --json` to inspect the 0.26 web control model with answer, connect, type guide, eval story, and import review controls.

Use `dukememory autonomous-usefulness --json` to plan autonomous memory usefulness improvements; use `--apply` only for reversible feedback materialization.

Use `dukememory benchmark-polish --json` to inspect polished local benchmark evidence and dashboard-ready proof points.

Use `dukememory web-control-center-v8 --json` to inspect the 0.27 web control model with Answer v2, autonomous usefulness, and benchmark polish panels.

Use `dukememory autonomous-supervisor --json` to plan safe autonomous repair; use `--apply` to run embed-index, autonomous loop, agent enforcement, contract refresh, and doctor verification in order.

Use `dukememory web-control-center-v9 --json` to inspect the 0.28 web control model with autonomous supervisor panels.

Use `dukememory fleet-supervisor --json` to plan safe autonomous supervisor repairs across every discovered project memory; use `--apply` for reversible fleet maintenance.

Use `dukememory web-control-center-v10 --json` to inspect the 0.29 web control model with fleet supervisor panels.

Use `dukememory fleet-supervisor-watch-install --dry-run --json` to preview periodic launchd maintenance across discovered project memories; omit `--dry-run` to write the plist.

Use `dukememory web-control-center-v11 --json` to inspect the 0.30 web control model with fleet watch installation.

Use `dukememory web-control-center-v12 --json` to inspect the 0.33 web control model with effectiveness, baselines, conflict apply, MCP V3, fleet quality, and release gate V3 panels.

Use `dukememory project-profile --json` to inspect the project memory profile, embedding configuration, and recommended budget.

Use `dukememory recall "<task>" --max-chars 1200` when brief/impact is not enough but full context would waste tokens; use `--recent`, `--as-of YYYY-MM-DD`, `--as-of-days-ago N`, `--changed-since YYYY-MM-DD`, or `--changed-since-days N` for temporal recall.

Use `dukememory memory-timeline <memory-id> --json` to inspect one card's facts, audit events, and real agent read influence before editing surprising or high-impact memory.

Use `dukememory memory-conflict-review --json` to review duplicate, stale, active-superseded, and contradiction-prone groups without mutating memory.

Use `dukememory eval live --json` to inspect whether memory reads are later judged useful, useless, or missing.

Use `dukememory dashboard --json` to inspect all discovered project memories and autonomous health.

Use `dukememory intelligence-dashboard --json` to inspect ROI, agent behavior, decision trace, auto-feedback status, cost guard, project diff, and remote sync dry-run in one compact report.

Use `dukememory project-diff --changed-only --json` to compare current project changes with memory links, stale facts, and duplicate decisions.

Use `dukememory remote-sync-dry-run --json` before using VDS/remote memory sync. Keep reads local-first unless measured latency is acceptable.

Use `dukememory doctor-project --json` to verify project memory DB, AGENTS block, Codex skill, embeddings, QA, and autonomous status.

Use `dukememory release-gate --json` before committing or publishing a release. In strict mode it also requires a clean worktree.

Use `dukememory release-gate --run --json` when Codex should execute fmt/check/test/build and return the result bundle.

Use `dukememory memory-replay --json` to inspect how recent memory reads influenced work.

Use `dukememory project-watch --json` to inspect all discovered project memories; use `dukememory project-watch --fix --json` for autonomous repair.

Use `dukememory autonomous-loop --json` to plan one autonomous memory control loop; use `dukememory autonomous-loop --apply --json` to run reversible maintenance and project repair.

Use `dukememory autonomous-loop --watch --apply --interval-secs 3600 --json` to run the same reversible loop periodically without token-heavy context.

Use `dukememory autonomous-watch-install --dry-run --json` to preview a local launchd watch plist for autonomous-loop watch mode.

Use `dukememory action-journal --json` to inspect autonomous actions, skipped actions, failures, and rollback availability.

Use `dukememory usefulness-engine --json` to rank useful/noisy memory and preview safe inferred feedback; use `dukememory usefulness-engine --apply --json` to materialize safe feedback.

Use `dukememory ranking-profile --profile balanced|strict|recall-heavy|precision-heavy --json` to inspect retrieval ranking weights; use `--apply` to make the profile durable for a project.

Use `dukememory auto-ranking-tune --json` to adapt retrieval strictness from live usefulness, semantic, and quality signals.

Use `dukememory project-template --kind rust-cli|frontend-app|game-mod|electronics-cad|docs-research --json` to seed project-type memory defaults.

Use `dukememory watch-control --json` to inspect launchd watch readiness; use `--apply` to write/load the autonomous watch plist.

Use `dukememory autonomy-control-center --json` to inspect context, ranking, watch, diff, and sync readiness in one control view.

Use `dukememory sync-latency --json` to measure local/VDS sync latency while keeping reads local-first.

Use `dukememory sync-profile --profile local-first-backup --run-dry-run --json` to choose a safe local-first sync mode before push/pull.

Use `dukememory agent-enforce --json` to verify future chats will use memory; use `dukememory agent-enforce --fix --json` to repair AGENTS/skill/project wiring.

Use `dukememory memory-diff-review --json` to review changed files against memory and decide whether durable task_state/design_note cards are needed.

Use `dukememory remote-sync-v2 --target PATH --json` to preview encrypted local-first sync; set a permission-restricted sync passphrase file before `--apply`.

Use `dukememory sync export bundle.json --dry-run --json` before writing a local-first sync bundle; use `dukememory sync import bundle.json --dry-run --json` before applying it.

Use `dukememory sync import bundle.json --policy manual|local-wins|remote-wins|newer-wins --dry-run --json` to inspect conflicts before applying.

Use `dukememory sync push TARGET --dry-run --json`, `dukememory sync pull TARGET --dry-run --json`, and `dukememory sync status TARGET --json` for local-first remote/VDS connector workflows.

Use `dukememory inbox-v2 report --json` before processing pending inbox items; use `dukememory inbox-v2 auto-apply --dry-run --json` before allowing changes.

Use `dukememory policy-tune --json` to adapt autonomous policy thresholds from feedback, quality, and rollback history.

Use `dukememory memory-qa --json` to answer whether memory is actually useful, noisy, complete, semantically indexed, and autonomous-ready.

Use `dukememory memory-contract --write` after meaningful project changes to keep one compact project-wide contract current.

Use `dukememory upgrade-project --json` after a dukememory release to refresh binary, skill, AGENTS/rules, memory contract, and QA in one pass.

Use `dukememory upgrade-all-projects --json` after a dukememory release to refresh every discovered local project memory.

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
dukememory context-governor "task" --json
dukememory memory-router "query" --include-siblings --json
dukememory memory-health-score --json
dukememory explain-recall "query" --json
dukememory project-intent-map --json
dukememory memory-test-harness --json
dukememory agent-audit-v2 --json
dukememory memory-control-center-v2 --json
dukememory auto-supersede-v2 --json
dukememory memory-diff-apply --json
dukememory recall-benchmark-suite --json
dukememory release-gate-v2 --json
dukememory memory-effectiveness-v2 --json
dukememory recall-benchmark-baselines --json
dukememory memory-conflict-apply --json
dukememory remote-sync-wizard --json
dukememory memory-governance-policy --json
dukememory autonomous-loop-v2 --json
dukememory governance-enforce --json
dukememory memory-quality-ci --json
dukememory fleet-dashboard-v2 --json
dukememory remote-sync-apply-flow --json
dukememory mcp-tool-surface-v2 --json
dukememory mcp-tool-surface-v3 --json
dukememory autopilot-v3 --json
dukememory self-learning-retrieval --json
dukememory project-role-profile --json
dukememory inbox-ai-reviewer --json
dukememory web-control-center-v3 --json
dukememory remote-sync-apply --json
dukememory mcp-quality-tools --json
dukememory remote-sync-control --json
dukememory web-control-center-v4 --json
dukememory mcp-discipline-v2 --json
dukememory mcp-discipline-v3 --json
dukememory feedback-loop-v2 --json
dukememory upgrade-all-projects-v2 --json
dukememory fleet-quality --json
dukememory vds-sync-pack --json
dukememory web-control-center-v5 --json
dukememory quality-autopilot-v31 --json
dukememory memory-router-v2 "project memory" --include-siblings --json
dukememory benchmark-profiles --json
dukememory install-polish --json
dukememory memory-effectiveness-lab --json
dukememory auto-context-budgeter-v2 "project memory" --json
dukememory memory-contract-v2 --json
dukememory cross-project-learning "project memory" --json
dukememory agent-trace --json
dukememory vds-sync-hardening --json
dukememory install-quality --json
dukememory web-control-center-v6 --json
dukememory answer "project memory" --json
dukememory connect-codex --json
dukememory memory-type-guide --json
dukememory memory-eval-story --json
dukememory import-review README.md --json
dukememory memory-upload README.md --json
dukememory memanto-gap-report --json
dukememory memory-timeline <memory-id> --json
dukememory memory-conflict-review --json
dukememory web-control-center-v7 --json
dukememory autonomous-usefulness --json
dukememory benchmark-polish --json
dukememory web-control-center-v8 --json
dukememory autonomous-supervisor --json
dukememory web-control-center-v9 --json
dukememory fleet-supervisor --json
dukememory web-control-center-v10 --json
dukememory fleet-supervisor-watch-install --dry-run --json
dukememory web-control-center-v11 --json
dukememory web-control-center-v12 --json
dukememory doctor-project --json
dukememory release-gate --json
dukememory project-watch --json
dukememory memory-replay --json
dukememory autonomous-loop --json
dukememory autonomous-watch-install --dry-run --json
dukememory action-journal --json
dukememory usefulness-engine --json
dukememory ranking-profile --json
dukememory auto-ranking-tune --json
dukememory project-template --kind rust-cli --json
dukememory watch-control --json
dukememory autonomy-control-center --json
dukememory sync-latency --json
dukememory sync-profile --run-dry-run --json
dukememory agent-enforce --json
dukememory memory-diff-review --json
dukememory remote-sync-v2 --json
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

fn print_vec_status(conn: &Connection) {
    let sqlite_vec_version = conn
        .query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0))
        .ok();
    if cfg!(feature = "vec") {
        println!("retrieval backend: native sqlite-vec SQL cosine search");
        println!("embedding storage: SQLite JSON, consumed directly by sqlite-vec");
    } else {
        println!("retrieval backend: application-side cosine search");
        println!("embedding storage: SQLite JSON");
    }
    println!("sqlite-vec bundled feature: {}", cfg!(feature = "vec"));
    println!(
        "sqlite-vec extension: {}",
        sqlite_vec_version.as_deref().unwrap_or("not loaded")
    );
    if let Ok(report) = sqlite_vec_index_report(conn) {
        println!("persistent indexes: {}", report.indexes.len());
        println!("persistent indexes consistent: {}", report.consistent);
    }
    println!("embedding providers: local, ollama, openai-compatible, gemini, mock");
    println!("default provider: {DEFAULT_EMBED_PROVIDER}");
    println!("default endpoint: {DEFAULT_EMBED_ENDPOINT}");
    println!("default model: {DEFAULT_EMBED_MODEL}");
    println!("commands: embed-index, embed-search, vec-index, context-pack");
}

fn print_vec_index(conn: &Connection, rebuild: bool, json_out: bool) -> Result<()> {
    let rebuilt = if rebuild {
        rebuild_all_sqlite_vec_indexes(conn)?
    } else {
        0
    };
    let report = sqlite_vec_index_report(conn)?;
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "rebuilt": rebuilt,
                "report": report,
            }))?
        );
        return Ok(());
    }
    println!("sqlite_vec_indexes: {}", report.indexes.len());
    println!("consistent: {}", report.consistent);
    if rebuild {
        println!("rebuilt: {rebuilt}");
    }
    for index in report.indexes {
        println!(
            "{} {}d source={} indexed={} table={}",
            index.kind, index.dimensions, index.source_rows, index.indexed_rows, index.table_name
        );
    }
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
        "vec-status",
        "vec-index",
        "vec-validate",
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
        "context-governor",
        "memory-router",
        "memory-health-score",
        "explain-recall",
        "project-intent-map",
        "memory-test-harness",
        "agent-audit-v2",
        "memory-control-center-v2",
        "auto-supersede-v2",
        "memory-diff-apply",
        "recall-benchmark-suite",
        "release-gate-v2",
        "memory-effectiveness-v2",
        "recall-benchmark-baselines",
        "memory-conflict-apply",
        "remote-sync-wizard",
        "memory-governance-policy",
        "autonomous-loop-v2",
        "governance-enforce",
        "memory-quality-ci",
        "fleet-dashboard-v2",
        "remote-sync-apply-flow",
        "mcp-tool-surface-v2",
        "mcp-tool-surface-v3",
        "autopilot-v3",
        "self-learning-retrieval",
        "project-role-profile",
        "inbox-ai-reviewer",
        "web-control-center-v3",
        "remote-sync-apply",
        "mcp-quality-tools",
        "remote-sync-control",
        "web-control-center-v4",
        "mcp-discipline-v2",
        "mcp-discipline-v3",
        "feedback-loop-v2",
        "upgrade-all-projects-v2",
        "fleet-quality",
        "vds-sync-pack",
        "web-control-center-v5",
        "quality-autopilot-v31",
        "memory-router-v2",
        "benchmark-profiles",
        "install-polish",
        "memory-effectiveness-lab",
        "auto-context-budgeter-v2",
        "memory-contract-v2",
        "cross-project-learning",
        "agent-trace",
        "vds-sync-hardening",
        "install-quality",
        "web-control-center-v6",
        "answer",
        "connect-codex",
        "memory-type-guide",
        "memory-eval-story",
        "import-review",
        "memory-upload",
        "memanto-gap-report",
        "memory-timeline",
        "memory-conflict-review",
        "web-control-center-v7",
        "autonomous-usefulness",
        "benchmark-polish",
        "web-control-center-v8",
        "autonomous-supervisor",
        "web-control-center-v9",
        "fleet-supervisor",
        "web-control-center-v10",
        "fleet-supervisor-watch-install",
        "web-control-center-v11",
        "web-control-center-v12",
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
        "memory-replay",
        "project-watch",
        "autonomous-loop",
        "autonomous-watch-install",
        "action-journal",
        "usefulness-engine",
        "ranking-profile",
        "auto-ranking-tune",
        "project-template",
        "watch-control",
        "autonomy-control-center",
        "sync-latency",
        "sync-profile",
        "memory-diff-review",
        "remote-sync-v2",
        "agent-enforce",
        "inbox-v2",
        "policy-tune",
        "memory-qa",
        "memory-contract",
        "upgrade-project",
        "upgrade-all-projects",
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
    println!("  vec-index --rebuild           rebuild persistent sqlite-vec indexes");
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
    println!("  context-governor TASK         choose smallest memory read flow");
    println!("  memory-router QUERY           route memory across local projects");
    println!("  memory-health-score --json    score end-to-end memory health");
    println!("  explain-recall QUERY --json   explain recalled memory cards");
    println!("  project-intent-map --json     summarize goals, constraints, tasks");
    println!("  memory-test-harness --json    run retrieval quality probes");
    println!("  agent-audit-v2 --json         stricter agent memory behavior audit");
    println!("  memory-control-center-v2      aggregate health, recall, tests, autonomy");
    println!("  auto-supersede-v2 --json      safely supersede duplicate memory");
    println!("  memory-diff-apply --json      write high-confidence diff memory cards");
    println!("  recall-benchmark-suite        compare retrieval probes against baseline");
    println!("  release-gate-v2 --json        release gate with memory health checks");
    println!("  remote-sync-wizard --json     guided local-first remote sync setup");
    println!("  memory-governance-policy      inspect/write autonomous memory policy");
    println!("  autonomous-loop-v2 --json     guarded autonomous loop with quality gates");
    println!("  governance-enforce --json     enforce autonomous memory governance");
    println!("  memory-quality-ci --json      CI-friendly memory quality gate");
    println!("  fleet-dashboard-v2 --json     V2 quality status for all project memories");
    println!("  remote-sync-apply-flow        guarded local-first remote sync apply plan");
    println!("  mcp-tool-surface-v2 --json    inspect exposed MCP V2 memory tools");
    println!("  autopilot-v3 --json           autonomous learning, role, inbox, sync, MCP loop");
    println!("  self-learning-retrieval       tune retrieval from live usefulness signals");
    println!("  project-role-profile --apply  detect/apply project-specific memory profile");
    println!("  inbox-ai-reviewer --json      explain and safely process inbox suggestions");
    println!("  web-control-center-v3         Health/Autonomy/Projects/Sync control model");
    println!("  remote-sync-apply --json      guarded local-first remote sync apply surface");
    println!("  mcp-quality-tools --json      inspect MCP helper tools for memory discipline");
    println!("  remote-sync-control --json    local-first VDS sync control and dry-runs");
    println!("  web-control-center-v4         actionable UI control model with apply endpoints");
    println!("  mcp-discipline-v2 --json      enforce startup/write/after-task memory discipline");
    println!(
        "  feedback-loop-v2 --json       autonomous usefulness, supersede, diff, benchmark loop"
    );
    println!("  upgrade-all-projects-v2       richer all-project upgrade/version summary");
    println!("  vds-sync-pack --json          local-first VDS sync pack with verify commands");
    println!("  web-control-center-v5         0.24 UI control model and release surfaces");
    println!("  quality-autopilot-v31         safe quality/cost/health autopilot");
    println!("  memory-router-v2 QUERY        cross-project router with current-write guardrails");
    println!("  benchmark-profiles --json     project-aware retrieval benchmark profile");
    println!("  install-polish --json         README/license/screenshot/install release checks");
    println!("  memory-effectiveness-lab      measure whether memory reads helped agents");
    println!("  auto-context-budgeter-v2 TASK smallest useful memory context flow");
    println!("  memory-contract-v2 --write    compact project contract v2");
    println!("  cross-project-learning QUERY  sibling-project hints without cross writes");
    println!("  agent-trace --json            recent memory influence and writes");
    println!("  vds-sync-hardening --json     VDS target/latency/dry-run/rollback checks");
    println!("  install-quality --json        install, skill, AGENTS, doctor readiness");
    println!("  web-control-center-v6         0.25 effectiveness and trace control model");
    println!("  answer QUESTION --json        grounded memory answer with citations");
    println!("  connect-codex --apply         one-command Codex memory connection check");
    println!("  memory-type-guide --json      explain memory types, filters, guardrails");
    println!("  memory-eval-story --json      reproducible recall/effectiveness story");
    println!("  import-review FILE --json     turn text into reviewed inbox candidates");
    println!("  memory-upload FILE --json     upload file into reviewed inbox candidates");
    println!("  memanto-gap-report --json     compare Memanto-style capability coverage");
    println!("  memory-timeline ID --json     show card events and real read influence");
    println!("  memory-conflict-review --json review duplicate/stale/contradiction groups");
    println!("  web-control-center-v7         0.26 answer/connect/eval/import control model");
    println!("  autonomous-usefulness --json  plan autonomous usefulness improvements");
    println!("  benchmark-polish --json       polished local benchmark evidence");
    println!("  web-control-center-v8         0.27 answer/usefulness/benchmark control model");
    println!("  autonomous-supervisor --json  safe autonomous repair sequence");
    println!("  web-control-center-v9         0.28 supervisor control model");
    println!("  fleet-supervisor --json       safe autonomous repair across projects");
    println!("  web-control-center-v10        0.29 fleet supervisor control model");
    println!("  fleet-supervisor-watch-install preview/install periodic fleet repair");
    println!("  web-control-center-v11        0.30 fleet watch control model");
    println!("  memory-effectiveness-v2       V2 influence, waste, and semantic usefulness");
    println!("  recall-benchmark-baselines    inspect/write guarded recall baselines");
    println!("  memory-conflict-apply --json  dry-run guarded reversible conflict actions");
    println!("  mcp-tool-surface-v3 --json    inspect MCP V3 memory tool exposure");
    println!("  mcp-discipline-v3 --json      verify V3 memory discipline");
    println!("  fleet-quality --json          V3 quality across discovered projects");
    println!("  release-gate-v3 --json        release gate with effectiveness and MCP V3");
    println!("  web-control-center-v12        0.33 effectiveness/release control model");
    println!("  feedback --id ID --rating useful|useless|missing");
    println!("  budget-plan TASK --json       choose smallest useful memory budget");
    println!("  project-profile --json        structured project memory profile");
    println!("  recall QUERY --recent         compressed or temporal token-light recall");
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
    println!("  memory-replay --json          replay recent memory influence");
    println!("  project-watch --fix --json    inspect or repair installed project memories");
    println!("  autonomous-loop --apply       run reversible memory control loop");
    println!("  autonomous-watch-install      preview/install launchd watch plist");
    println!("  action-journal --json         inspect autonomous action timeline");
    println!("  usefulness-engine --apply     rank memory and apply safe feedback");
    println!("  ranking-profile --apply       configure retrieval ranking strictness");
    println!("  auto-ranking-tune --apply     tune ranking profile from live signals");
    println!("  project-template --apply      write project-type starter memory defaults");
    println!("  watch-control --apply         inspect or enable autonomous watch loop");
    println!("  autonomy-control-center       aggregate autonomy control status");
    println!("  sync-latency --json           measure local/VDS sync latency");
    println!("  sync-profile --json           choose a local-first sync profile");
    println!("  memory-diff-review --json     review changed files for memory updates");
    println!("  remote-sync-v2 --json         preview or apply encrypted local-first sync");
    println!("  agent-enforce --fix --json    enforce memory use for future chats");
    println!("  onboard --root DIR            initialize memory/profile/embeddings");
    println!("  inbox-v2 report|auto-apply    group and process pending suggestions");
    println!("  policy-tune --json            tune autonomous policy from feedback");
    println!("  memory-qa --json              score memory usefulness, noise, and health");
    println!("  memory-contract --write       write compact project memory contract");
    println!("  upgrade-project --json        refresh binary/skill/rules/contract/QA");
    println!("  upgrade-all-projects --json   refresh every discovered project memory");
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
    println!("  eval rag --json");
    println!("  rag-ingest PATH --apply --json");
    println!("  rag-sources --json");
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
    println!(
        "generation_provider: {}",
        runtime.config.generation.provider
    );
    println!(
        "generation_endpoint: {}",
        runtime.config.generation.endpoint
    );
    println!("generation_model: {}", runtime.config.generation.model);
}

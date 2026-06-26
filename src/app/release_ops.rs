use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn write_bundle(
    conn: &Connection,
    db: &Path,
    output: &Path,
    redact: bool,
) -> Result<()> {
    let mut export = export_memories(conn, &[], &[], None)?;
    if redact {
        redact_export(&mut export)?;
    }
    let stats = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "db": db.display().to_string(),
        "events": audit_events(conn, 50)?,
        "doctor": {
            "secrets": scan_secret_findings(conn)?.len(),
            "pending_inbox": list_inbox(conn, "pending", usize::MAX)?.len()
        },
        "export": export
    });
    write_file(output, serde_json::to_string_pretty(&stats)?.as_bytes())?;
    println!("{}", output.display());
    Ok(())
}

#[derive(Debug, Serialize)]
struct ReleaseBundleManifest {
    name: String,
    version: String,
    schema: i64,
    generated_at: i64,
    binary: String,
    binary_sha256: String,
    config_template: String,
    database: String,
    memory_stats: services::MemoryStats,
}

pub(crate) fn write_release_bundle(conn: &Connection, db: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("failed to create {}", output.display()))?;
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let binary_name = if cfg!(windows) {
        "dukememory.exe"
    } else {
        "dukememory"
    };
    let binary_path = output.join(binary_name);
    copy_file(&exe, &binary_path)?;

    let config_path = output.join("dukememory.toml");
    let cfg = AgentConfig::production_defaults(
        db,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    );
    write_file(&config_path, toml::to_string_pretty(&cfg)?.as_bytes())?;

    let readme = Path::new("README.md");
    if readme.exists() {
        copy_file(readme, &output.join("README.md"))?;
    }

    let store = MemoryStore::new(conn);
    let service = MemoryService::new(store);
    let manifest = ReleaseBundleManifest {
        name: "dukememory".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: CURRENT_SCHEMA_VERSION,
        generated_at: now_ms(),
        binary: binary_name.to_string(),
        binary_sha256: sha256_file(&binary_path)?,
        config_template: "dukememory.toml".to_string(),
        database: db.display().to_string(),
        memory_stats: service.stats()?,
    };
    write_file(
        &output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )?;
    println!("{}", output.display());
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
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

#[derive(Debug, Serialize)]
struct BenchReport {
    version: String,
    schema: i64,
    db_bytes: u64,
    memory_count: i64,
    active_memory_count: i64,
    pending_inbox_count: i64,
    embedding_count: i64,
    event_count: i64,
    fts_probe_rows: usize,
    fts_probe_ms: u128,
    stats_ms: u128,
    export_ms: u128,
}

pub(crate) fn print_bench(conn: &Connection, db: &Path, json_out: bool) -> Result<()> {
    let store = MemoryStore::new(conn);
    let service = MemoryService::new(store);

    let stats_start = std::time::Instant::now();
    let stats = service.stats()?;
    let stats_ms = stats_start.elapsed().as_millis();

    let fts_start = std::time::Instant::now();
    let retrieval = RetrievalService::new(service.store());
    let fts_probe_rows = retrieval.fts_probe("memory", 25)?;
    let fts_probe_ms = fts_start.elapsed().as_millis();

    let export_start = std::time::Instant::now();
    let _export = export_memories(conn, &[], &[], None)?;
    let export_ms = export_start.elapsed().as_millis();

    let report = BenchReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema: stats.schema,
        db_bytes: fs::metadata(db).map(|meta| meta.len()).unwrap_or(0),
        memory_count: stats.total,
        active_memory_count: stats.active,
        pending_inbox_count: stats.pending_inbox,
        embedding_count: stats.embeddings,
        event_count: stats.events,
        fts_probe_rows,
        fts_probe_ms,
        stats_ms,
        export_ms,
    };

    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("version: {}", report.version);
        println!("schema: {}", report.schema);
        println!("db_bytes: {}", report.db_bytes);
        println!("memory_count: {}", report.memory_count);
        println!("active_memory_count: {}", report.active_memory_count);
        println!("pending_inbox_count: {}", report.pending_inbox_count);
        println!("embedding_count: {}", report.embedding_count);
        println!("event_count: {}", report.event_count);
        println!("fts_probe_rows: {}", report.fts_probe_rows);
        println!("fts_probe_ms: {}", report.fts_probe_ms);
        println!("stats_ms: {}", report.stats_ms);
        println!("export_ms: {}", report.export_ms);
    }
    Ok(())
}

struct SelfMemorySeed {
    id: &'static str,
    memory_type: &'static str,
    title: &'static str,
    body: &'static str,
}

const SELF_MEMORY_SEEDS: &[SelfMemorySeed] = &[
    SelfMemorySeed {
        id: "self-v11-purpose",
        memory_type: "product_goal",
        title: "dukememory purpose",
        body: "dukememory is the local Rust + SQLite + FTS5 + Rhai memory layer for agent-driven projects. It stores durable product intent, decisions, preferences, commands, task state, constraints, and notes.",
    },
    SelfMemorySeed {
        id: "self-v11-architecture",
        memory_type: "design_note",
        title: "dukememory architecture",
        body: "Production v14.5 uses a schema-migrated SQLite store, FTS-first retrieval, optional ready-index embedding recall, Rhai policy hooks, auto-ingest, doctrine extraction, tiny briefs, evidence reports, HTTP/MCP surfaces, and service/storage modules for maintainable operations.",
    },
    SelfMemorySeed {
        id: "self-v11-operations",
        memory_type: "command",
        title: "dukememory release operations",
        body: "Use schema verify, doctor --self-check, bench --json, release-bundle, cargo test, cargo test --features vec, cargo clippy --all-targets --all-features -- -D warnings, and cargo build --release before calling a release ready.",
    },
    SelfMemorySeed {
        id: "self-v11-local-first",
        memory_type: "constraint",
        title: "dukememory local-first constraint",
        body: "The memory system must stay local, fast, and useful without a model server. Embeddings improve recall when available, but FTS, structured cards, doctrine, and context packing remain the reliable baseline.",
    },
];

#[derive(Debug, Serialize)]
struct SelfHostReport {
    added: usize,
    skipped: usize,
    total: usize,
}

pub(crate) fn self_host_memory(conn: &Connection, force: bool) -> Result<()> {
    let mut added = 0;
    let mut skipped = 0;
    for seed in SELF_MEMORY_SEEDS {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM memories WHERE id = ?1)",
                params![seed.id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if exists && !force {
            skipped += 1;
            continue;
        }
        upsert_self_memory(conn, seed)?;
        added += 1;
    }
    let maintenance_store = MemoryStore::new(conn);
    let maintenance = MaintenanceService::new(&maintenance_store);
    let report = SelfHostReport {
        added,
        skipped,
        total: SELF_MEMORY_SEEDS.len(),
    };
    log_event(
        conn,
        "self_host",
        None,
        &format!(
            "seeded self memory added={} skipped={} pending_inbox={}",
            report.added,
            report.skipped,
            maintenance.pending_work_count()?
        ),
    )?;
    println!(
        "self_hosted: added={} skipped={} total={}",
        report.added, report.skipped, report.total
    );
    Ok(())
}

fn upsert_self_memory(conn: &Connection, seed: &SelfMemorySeed) -> Result<()> {
    let ts = now_ms();
    conn.execute(
        r#"
        INSERT INTO memories (
            id, type, scope, title, body, status, source,
            created_at, updated_at, supersedes, superseded_by, confidence
        ) VALUES (?1, ?2, 'project', ?3, ?4, 'active', 'self-host',
            ?5, ?5, NULL, NULL, 1.0)
        ON CONFLICT(id) DO UPDATE SET
            type = excluded.type,
            scope = excluded.scope,
            title = excluded.title,
            body = excluded.body,
            status = excluded.status,
            source = excluded.source,
            updated_at = excluded.updated_at,
            confidence = excluded.confidence
        "#,
        params![seed.id, seed.memory_type, seed.title, seed.body, ts],
    )?;
    log_event(
        conn,
        "memory_upserted",
        Some(seed.id),
        "upserted self memory",
    )?;
    Ok(())
}

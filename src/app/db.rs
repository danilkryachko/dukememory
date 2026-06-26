use super::*;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -20000;
PRAGMA mmap_size = 268435456;

CREATE TABLE IF NOT EXISTS schema_versions (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL,
    description TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'project',
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    source TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    supersedes TEXT,
    superseded_by TEXT,
    confidence REAL NOT NULL DEFAULT 1.0,
    FOREIGN KEY (supersedes) REFERENCES memories(id) ON DELETE SET NULL,
    FOREIGN KEY (superseded_by) REFERENCES memories(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS memory_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    memory_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    title,
    body,
    type,
    scope,
    content='memories',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, title, body, type, scope)
    VALUES (new.rowid, new.title, new.body, new.type, new.scope);
END;

CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, title, body, type, scope)
    VALUES ('delete', old.rowid, old.title, old.body, old.type, old.scope);
END;

CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, title, body, type, scope)
    VALUES ('delete', old.rowid, old.title, old.body, old.type, old.scope);
    INSERT INTO memories_fts(rowid, title, body, type, scope)
    VALUES (new.rowid, new.title, new.body, new.type, new.scope);
END;

CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(type);
CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
CREATE INDEX IF NOT EXISTS idx_memories_updated_at ON memories(updated_at);
CREATE INDEX IF NOT EXISTS idx_memories_superseded_by ON memories(superseded_by);
CREATE INDEX IF NOT EXISTS idx_memories_status_scope_updated_at ON memories(status, scope, updated_at);
CREATE INDEX IF NOT EXISTS idx_memory_links_memory_id ON memory_links(memory_id);

CREATE TABLE IF NOT EXISTS memory_embeddings (
    memory_id TEXT NOT NULL,
    model TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    embedding TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (memory_id, model, endpoint),
    FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_memory_embeddings_model ON memory_embeddings(model, endpoint);

CREATE TABLE IF NOT EXISTS memory_inbox (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'project',
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    source TEXT,
    confidence REAL NOT NULL DEFAULT 0.7,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_inbox_status ON memory_inbox(status);
CREATE INDEX IF NOT EXISTS idx_memory_inbox_updated_at ON memory_inbox(updated_at);
CREATE INDEX IF NOT EXISTS idx_memory_inbox_status_updated_at ON memory_inbox(status, updated_at);

CREATE TABLE IF NOT EXISTS memory_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    memory_id TEXT,
    detail TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_events_created_at ON memory_events(created_at);
CREATE INDEX IF NOT EXISTS idx_memory_events_memory_id ON memory_events(memory_id);
CREATE INDEX IF NOT EXISTS idx_memory_events_created_id ON memory_events(created_at, id);

CREATE TABLE IF NOT EXISTS memory_read_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command TEXT NOT NULL,
    query TEXT NOT NULL,
    memory_ids TEXT NOT NULL DEFAULT '',
    semantic_used INTEGER NOT NULL DEFAULT 0,
    result_count INTEGER NOT NULL DEFAULT 0,
    budget INTEGER NOT NULL DEFAULT 0,
    elapsed_ms INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_memory_read_events_created_at ON memory_read_events(created_at);
CREATE INDEX IF NOT EXISTS idx_memory_read_events_command ON memory_read_events(command);

CREATE TABLE IF NOT EXISTS memory_locks (
    name TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    acquired_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_cases (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    expected TEXT NOT NULL,
    budget INTEGER NOT NULL DEFAULT 4000,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    suggestions INTEGER NOT NULL DEFAULT 0,
    ingested_at INTEGER NOT NULL,
    UNIQUE(path, content_hash)
);
CREATE INDEX IF NOT EXISTS idx_memory_sources_hash ON memory_sources(content_hash);
CREATE INDEX IF NOT EXISTS idx_memory_sources_path_hash ON memory_sources(path, content_hash);
"#;

pub(crate) fn open_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(15))?;
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        PRAGMA cache_size = -20000;
        PRAGMA mmap_size = 268435456;
        "#,
    )?;
    conn.execute_batch(SCHEMA)?;
    run_migrations(&conn)?;
    Ok(conn)
}

fn run_migrations(conn: &Connection) -> Result<()> {
    ensure_column(conn, "memories", "superseded_by", "TEXT")?;
    ensure_column(conn, "memories", "confidence", "REAL NOT NULL DEFAULT 1.0")?;
    let version: Option<i64> =
        conn.query_row("SELECT MAX(version) FROM schema_versions", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?;
    if version.unwrap_or(0) < 1 {
        conn.execute(
            "INSERT OR IGNORE INTO schema_versions (version, applied_at, description) VALUES (1, ?1, 'Initial production schema')",
            params![now_ms()],
        )?;
    }
    for migration in migrations() {
        conn.execute(
            "INSERT OR IGNORE INTO schema_versions (version, applied_at, description) VALUES (?1, ?2, ?3)",
            params![migration.version, now_ms(), migration.name],
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Migration {
    version: i64,
    name: &'static str,
}

fn migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 2,
            name: "Production v2 workflow schema",
        },
        Migration {
            version: 3,
            name: "Production v3 embeddings schema",
        },
        Migration {
            version: 4,
            name: "Production v4 inbox schema",
        },
        Migration {
            version: 5,
            name: "Production v5 agent-native schema",
        },
        Migration {
            version: 6,
            name: "Production v6 policy schema",
        },
        Migration {
            version: 7,
            name: "Production v7 audit schema",
        },
        Migration {
            version: 8,
            name: "Production v8 service schema",
        },
        Migration {
            version: 9,
            name: "Production v9 hardening schema",
        },
        Migration {
            version: 10,
            name: "Production v10 stabilization schema",
        },
        Migration {
            version: 11,
            name: "Production v11 session ingest and doctrine schema",
        },
        Migration {
            version: 12,
            name: "Production v12 always-on operations schema",
        },
        Migration {
            version: 13,
            name: "Production v13 stabilization and optimization schema",
        },
        Migration {
            version: 14,
            name: "Production v14 read audit and usage telemetry schema",
        },
    ]
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    if !columns.contains(column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition};"
        ))?;
    }
    Ok(())
}

pub(crate) fn handle_schema(conn: &Connection, command: SchemaCommand) -> Result<()> {
    match command {
        SchemaCommand::Status => {
            let current = schema_version(conn)?;
            println!("current: {current}");
            println!("expected: {CURRENT_SCHEMA_VERSION}");
            println!("registered_migrations:");
            for migration in migrations() {
                println!("  {}  {}", migration.version, migration.name);
            }
        }
        SchemaCommand::Verify => {
            verify_schema(conn)?;
            println!("schema: ok");
        }
        SchemaCommand::Upgrade => {
            run_migrations(conn)?;
            println!("schema: upgraded");
        }
    }
    Ok(())
}

pub(crate) fn schema_version(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_versions",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn verify_schema(conn: &Connection) -> Result<()> {
    for table in [
        "memories",
        "memory_links",
        "memory_embeddings",
        "memory_inbox",
        "memory_events",
        "memory_read_events",
        "memory_locks",
        "eval_cases",
        "memory_sources",
    ] {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','virtual table') AND name = ?1",
            params![table],
            |row| row.get(0),
        )?;
        if exists == 0 {
            bail!("missing table: {table}");
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct IntegrityReport {
    ok: bool,
    schema: i64,
    integrity_check: String,
    quick_check: String,
    foreign_key_violations: usize,
    page_count: i64,
    freelist_count: i64,
}

pub(crate) fn print_integrity(conn: &Connection, json_out: bool) -> Result<()> {
    let report = integrity_report(conn)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("integrity: {}", if report.ok { "ok" } else { "fail" });
        println!("schema: {}", report.schema);
        println!("integrity_check: {}", report.integrity_check);
        println!("quick_check: {}", report.quick_check);
        println!("foreign_key_violations: {}", report.foreign_key_violations);
        println!("page_count: {}", report.page_count);
        println!("freelist_count: {}", report.freelist_count);
    }
    Ok(())
}

fn integrity_report(conn: &Connection) -> Result<IntegrityReport> {
    let integrity_check: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let quick_check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    let foreign_key_violations = conn
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |_| Ok(()))?
        .count();
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let freelist_count: i64 = conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
    Ok(IntegrityReport {
        ok: integrity_check == "ok" && quick_check == "ok" && foreign_key_violations == 0,
        schema: schema_version(conn)?,
        integrity_check,
        quick_check,
        foreign_key_violations,
        page_count,
        freelist_count,
    })
}

#[derive(Debug, Serialize)]
struct OptimizeReport {
    analyzed: bool,
    fts_optimized: bool,
    vacuumed: bool,
    wal_checkpointed: bool,
    page_count: i64,
    freelist_count: i64,
}

pub(crate) fn optimize_db(conn: &Connection, vacuum: bool, json_out: bool) -> Result<()> {
    conn.execute_batch(
        r#"
        ANALYZE;
        PRAGMA optimize;
        INSERT INTO memories_fts(memories_fts) VALUES('optimize');
        PRAGMA wal_checkpoint(TRUNCATE);
        "#,
    )?;
    if vacuum {
        conn.execute_batch("VACUUM;")?;
    }
    let report = OptimizeReport {
        analyzed: true,
        fts_optimized: true,
        vacuumed: vacuum,
        wal_checkpointed: true,
        page_count: conn.query_row("PRAGMA page_count", [], |row| row.get(0))?,
        freelist_count: conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?,
    };
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("analyzed: true");
        println!("fts_optimized: true");
        println!("wal_checkpointed: true");
        println!("vacuumed: {vacuum}");
        println!("page_count: {}", report.page_count);
        println!("freelist_count: {}", report.freelist_count);
    }
    Ok(())
}

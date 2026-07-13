use super::*;

#[cfg(feature = "vec")]
use std::sync::OnceLock;

#[cfg(feature = "vec")]
static SQLITE_VEC_REGISTRATION: OnceLock<i32> = OnceLock::new();

#[cfg(feature = "vec")]
type SqliteExtensionEntry = unsafe extern "C" fn(
    *mut rusqlite::ffi::sqlite3,
    *mut *mut std::os::raw::c_char,
    *const rusqlite::ffi::sqlite3_api_routines,
) -> std::os::raw::c_int;

#[derive(Debug, Serialize)]
pub(crate) struct SqliteVecIndexRow {
    pub(crate) kind: String,
    pub(crate) dimensions: usize,
    pub(crate) table_name: String,
    pub(crate) registered: bool,
    pub(crate) source_rows: usize,
    pub(crate) indexed_rows: usize,
    pub(crate) registry_rows: usize,
    pub(crate) rebuilt_at: i64,
    pub(crate) triggers_ok: bool,
    pub(crate) consistent: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct SqliteVecIndexReport {
    pub(crate) enabled: bool,
    pub(crate) version: Option<String>,
    pub(crate) consistent: bool,
    pub(crate) indexes: Vec<SqliteVecIndexRow>,
}

#[cfg(feature = "vec")]
#[derive(Clone, Copy)]
enum VecIndexKind {
    Memory,
    Rag,
}

#[cfg(feature = "vec")]
impl VecIndexKind {
    fn name(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Rag => "rag",
        }
    }

    fn source_table(self) -> &'static str {
        match self {
            Self::Memory => "memory_embeddings",
            Self::Rag => "rag_chunk_embeddings",
        }
    }

    fn table_name(self, dimensions: usize) -> String {
        format!("dukememory_{}_vec_{dimensions}", self.name())
    }
}

#[cfg(feature = "vec")]
pub(crate) fn register_sqlite_vec() -> Result<()> {
    let result = *SQLITE_VEC_REGISTRATION.get_or_init(|| unsafe {
        let entry = std::mem::transmute::<*const (), SqliteExtensionEntry>(
            sqlite_vec::sqlite3_vec_init as *const (),
        );
        rusqlite::ffi::sqlite3_auto_extension(Some(entry))
    });
    if result != rusqlite::ffi::SQLITE_OK {
        bail!("failed to register sqlite-vec extension: SQLite error {result}");
    }
    Ok(())
}

#[cfg(not(feature = "vec"))]
pub(crate) fn register_sqlite_vec() -> Result<()> {
    Ok(())
}

#[cfg(feature = "vec")]
pub(crate) fn initialize_sqlite_vec_indexes(conn: &Connection) -> Result<()> {
    for kind in [VecIndexKind::Memory, VecIndexKind::Rag] {
        let sql = format!(
            "SELECT DISTINCT dimensions FROM {} WHERE dimensions > 0 ORDER BY dimensions",
            kind.source_table()
        );
        let mut stmt = conn.prepare(&sql)?;
        let dimensions = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for dimensions in dimensions {
            let _ = ensure_vec_index(conn, kind, usize::try_from(dimensions)?, false);
        }
    }
    Ok(())
}

#[cfg(not(feature = "vec"))]
pub(crate) fn initialize_sqlite_vec_indexes(_conn: &Connection) -> Result<()> {
    Ok(())
}

#[cfg(feature = "vec")]
fn validate_dimensions(dimensions: usize) -> Result<()> {
    if dimensions == 0 || dimensions > 65_536 {
        bail!("unsupported vector dimensions: {dimensions}");
    }
    Ok(())
}

#[cfg(feature = "vec")]
fn ensure_vec_index(
    conn: &Connection,
    kind: VecIndexKind,
    dimensions: usize,
    rebuild: bool,
) -> Result<String> {
    validate_dimensions(dimensions)?;
    let table_name = kind.table_name(dimensions);
    let source_table = kind.source_table();
    let table_existed = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [&table_name],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    conn.execute_batch(&format!(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS {table_name} USING vec0(
            embedding_rowid INTEGER PRIMARY KEY,
            embedding float[{dimensions}] distance_metric=cosine,
            endpoint TEXT,
            model TEXT
        );
        "#
    ))?;
    let registered_index = conn
        .query_row(
            "SELECT table_name, trigger_version FROM vector_index_registry WHERE kind = ?1 AND dimensions = ?2",
            params![kind.name(), dimensions as i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let registered = table_existed
        && registered_index
            .as_ref()
            .is_some_and(|(stored_table, trigger_version)| {
                stored_table == &table_name && *trigger_version == 1
            });
    if rebuild || !registered {
        install_vec_index_triggers(conn, source_table, &table_name, dimensions)?;
        rebuild_vec_index(conn, kind, dimensions, &table_name)?;
    }
    Ok(table_name)
}

#[cfg(feature = "vec")]
fn install_vec_index_triggers(
    conn: &Connection,
    source_table: &str,
    table_name: &str,
    dimensions: usize,
) -> Result<()> {
    conn.execute_batch(&format!(
        r#"
        DROP TRIGGER IF EXISTS {table_name}_ai;
        DROP TRIGGER IF EXISTS {table_name}_au_remove;
        DROP TRIGGER IF EXISTS {table_name}_au_upsert;
        DROP TRIGGER IF EXISTS {table_name}_ad;
        CREATE TRIGGER {table_name}_ai
        AFTER INSERT ON {source_table}
        WHEN NEW.dimensions = {dimensions}
        BEGIN
            INSERT INTO {table_name}(
                embedding_rowid, embedding, endpoint, model
            ) VALUES (NEW.rowid, NEW.embedding, NEW.endpoint, NEW.model);
        END;
        CREATE TRIGGER {table_name}_au_remove
        AFTER UPDATE ON {source_table}
        WHEN OLD.dimensions = {dimensions} AND NEW.dimensions != {dimensions}
        BEGIN
            DELETE FROM {table_name} WHERE embedding_rowid = OLD.rowid;
        END;
        CREATE TRIGGER {table_name}_au_upsert
        AFTER UPDATE ON {source_table}
        WHEN NEW.dimensions = {dimensions}
        BEGIN
            DELETE FROM {table_name} WHERE embedding_rowid = NEW.rowid;
            INSERT INTO {table_name}(
                embedding_rowid, embedding, endpoint, model
            ) VALUES (NEW.rowid, NEW.embedding, NEW.endpoint, NEW.model);
        END;
        CREATE TRIGGER {table_name}_ad
        AFTER DELETE ON {source_table}
        WHEN OLD.dimensions = {dimensions}
        BEGIN
            DELETE FROM {table_name} WHERE embedding_rowid = OLD.rowid;
        END;
        "#
    ))?;
    Ok(())
}

#[cfg(feature = "vec")]
fn rebuild_vec_index(
    conn: &Connection,
    kind: VecIndexKind,
    dimensions: usize,
    table_name: &str,
) -> Result<()> {
    let source_table = kind.source_table();
    let now = now_ms();
    let sql = format!(
        r#"
        SAVEPOINT dukememory_vec_rebuild;
        DELETE FROM {table_name};
        INSERT INTO {table_name}(embedding_rowid, embedding, endpoint, model)
        SELECT rowid, embedding, endpoint, model
        FROM {source_table}
        WHERE dimensions = {dimensions};
        INSERT INTO vector_index_registry(
            kind, dimensions, table_name, indexed_rows, rebuilt_at, trigger_version
        ) VALUES (
            '{}', {dimensions}, '{table_name}',
            (SELECT COUNT(*) FROM {source_table} WHERE dimensions = {dimensions}),
            {now}, 1
        )
        ON CONFLICT(kind, dimensions) DO UPDATE SET
            table_name = excluded.table_name,
            indexed_rows = excluded.indexed_rows,
            rebuilt_at = excluded.rebuilt_at,
            trigger_version = excluded.trigger_version;
        RELEASE dukememory_vec_rebuild;
        "#,
        kind.name()
    );
    if let Err(error) = conn.execute_batch(&sql) {
        let _ = conn
            .execute_batch("ROLLBACK TO dukememory_vec_rebuild; RELEASE dukememory_vec_rebuild;");
        return Err(error).context("failed to rebuild persistent sqlite-vec index");
    }
    Ok(())
}

#[cfg(feature = "vec")]
pub(crate) fn ensure_sqlite_vec_memory_index(conn: &Connection, dimensions: usize) -> Result<()> {
    ensure_vec_index(conn, VecIndexKind::Memory, dimensions, false).map(|_| ())
}

#[cfg(not(feature = "vec"))]
pub(crate) fn ensure_sqlite_vec_memory_index(_conn: &Connection, _dimensions: usize) -> Result<()> {
    Ok(())
}

#[cfg(feature = "vec")]
pub(crate) fn ensure_sqlite_vec_rag_index(conn: &Connection, dimensions: usize) -> Result<()> {
    ensure_vec_index(conn, VecIndexKind::Rag, dimensions, false).map(|_| ())
}

#[cfg(not(feature = "vec"))]
pub(crate) fn ensure_sqlite_vec_rag_index(_conn: &Connection, _dimensions: usize) -> Result<()> {
    Ok(())
}

#[cfg(feature = "vec")]
pub(crate) fn rebuild_all_sqlite_vec_indexes(conn: &Connection) -> Result<usize> {
    let mut rebuilt = 0;
    for kind in [VecIndexKind::Memory, VecIndexKind::Rag] {
        let sql = format!(
            r#"
            SELECT dimensions FROM (
                SELECT DISTINCT dimensions FROM {} WHERE dimensions > 0
                UNION
                SELECT dimensions FROM vector_index_registry WHERE kind = ?1
            ) ORDER BY dimensions
            "#,
            kind.source_table()
        );
        let mut stmt = conn.prepare(&sql)?;
        let dimensions = stmt
            .query_map([kind.name()], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for dimensions in dimensions {
            ensure_vec_index(conn, kind, usize::try_from(dimensions)?, true)?;
            rebuilt += 1;
        }
    }
    Ok(rebuilt)
}

#[cfg(not(feature = "vec"))]
pub(crate) fn rebuild_all_sqlite_vec_indexes(_conn: &Connection) -> Result<usize> {
    bail!("persistent sqlite-vec indexes require a binary built with --features vec")
}

#[cfg(feature = "vec")]
pub(crate) fn sqlite_vec_index_report(conn: &Connection) -> Result<SqliteVecIndexReport> {
    let version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
    let mut stmt = conn.prepare(
        r#"
        SELECT kind, dimensions, table_name, indexed_rows, rebuilt_at
        FROM vector_index_registry
        ORDER BY kind, dimensions
        "#,
    )?;
    let registry_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut registry = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for (kind, dimensions, table_name, indexed_rows, rebuilt_at) in registry_rows {
        keys.insert((kind.clone(), dimensions));
        registry.insert((kind, dimensions), (table_name, indexed_rows, rebuilt_at));
    }
    for kind in [VecIndexKind::Memory, VecIndexKind::Rag] {
        let mut dimensions = conn.prepare(&format!(
            "SELECT DISTINCT dimensions FROM {} WHERE dimensions > 0",
            kind.source_table()
        ))?;
        let dimensions = dimensions
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for dimensions in dimensions {
            keys.insert((kind.name().to_string(), dimensions));
        }
    }
    let mut indexes = Vec::with_capacity(keys.len());
    for (kind, dimensions) in keys {
        let source_table = match kind.as_str() {
            "memory" => "memory_embeddings",
            "rag" => "rag_chunk_embeddings",
            other => bail!("unknown vector index kind in registry: {other}"),
        };
        let expected_table = format!("dukememory_{kind}_vec_{dimensions}");
        let registered_row = registry.remove(&(kind.clone(), dimensions));
        let registered = registered_row
            .as_ref()
            .is_some_and(|(table_name, _, _)| table_name == &expected_table);
        let (table_name, rebuilt_rows, rebuilt_at) =
            registered_row.unwrap_or_else(|| (expected_table.clone(), 0, 0));
        let source_rows: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {source_table} WHERE dimensions = ?1"),
            [dimensions],
            |row| row.get(0),
        )?;
        let table_exists = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [&table_name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let indexed_rows: i64 = if table_exists {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table_name}"), [], |row| {
                row.get(0)
            })?
        } else {
            0
        };
        let trigger_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name IN (?1, ?2, ?3, ?4)",
            params![
                format!("{table_name}_ai"),
                format!("{table_name}_au_remove"),
                format!("{table_name}_au_upsert"),
                format!("{table_name}_ad"),
            ],
            |row| row.get(0),
        )?;
        let triggers_ok = table_exists && trigger_count == 4;
        indexes.push(SqliteVecIndexRow {
            kind,
            dimensions: usize::try_from(dimensions)?,
            table_name,
            registered,
            source_rows: usize::try_from(source_rows)?,
            indexed_rows: usize::try_from(indexed_rows)?,
            registry_rows: usize::try_from(rebuilt_rows)?,
            rebuilt_at,
            triggers_ok,
            consistent: registered && source_rows == indexed_rows && triggers_ok,
        });
    }
    Ok(SqliteVecIndexReport {
        enabled: true,
        version: Some(version),
        consistent: indexes.iter().all(|row| row.consistent),
        indexes,
    })
}

#[cfg(not(feature = "vec"))]
pub(crate) fn sqlite_vec_index_report(_conn: &Connection) -> Result<SqliteVecIndexReport> {
    Ok(SqliteVecIndexReport {
        enabled: false,
        version: None,
        consistent: true,
        indexes: Vec::new(),
    })
}

#[cfg(feature = "vec")]
pub(crate) fn sqlite_vec_probe(conn: &Connection) -> Result<String> {
    let version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS temp.dukememory_vec_probe;
        CREATE VIRTUAL TABLE temp.dukememory_vec_probe USING vec0(
            embedding float[3] distance_metric=cosine
        );
        INSERT INTO dukememory_vec_probe(rowid, embedding)
        VALUES (1, '[1.0, 0.0, 0.0]'), (2, '[0.0, 1.0, 0.0]');
        "#,
    )?;
    let result = conn.query_row(
        r#"
        SELECT rowid, distance
        FROM dukememory_vec_probe
        WHERE embedding MATCH ?1 AND k = 1
        "#,
        ["[1.0, 0.0, 0.0]"],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?)),
    );
    let _ = conn.execute_batch("DROP TABLE IF EXISTS temp.dukememory_vec_probe;");
    let (rowid, distance) = result?;
    if rowid != 1 || distance.abs() > 1e-6 {
        bail!("sqlite-vec KNN probe returned an unexpected result");
    }
    Ok(version)
}

#[cfg(not(feature = "vec"))]
pub(crate) fn sqlite_vec_probe(_conn: &Connection) -> Result<String> {
    bail!("sqlite-vec is unavailable; rebuild with --features vec")
}

#[cfg(feature = "vec")]
pub(crate) struct SqliteVecMemorySearchOptions<'a> {
    pub(crate) endpoint: &'a str,
    pub(crate) model: &'a str,
    pub(crate) query_embedding: &'a [f32],
    pub(crate) limit: usize,
    pub(crate) types: &'a [String],
    pub(crate) statuses: &'a [String],
    pub(crate) scope: Option<&'a str>,
}

#[cfg(feature = "vec")]
pub(crate) fn sqlite_vec_memory_search(
    conn: &Connection,
    options: SqliteVecMemorySearchOptions<'_>,
) -> Result<Vec<(String, f64)>> {
    let dimensions = options.query_embedding.len();
    let table_name = ensure_vec_index(conn, VecIndexKind::Memory, dimensions, false)?;
    let total: usize = conn.query_row(
        r#"
        SELECT COUNT(*) FROM memory_embeddings
        WHERE endpoint = ?1 AND model = ?2 AND dimensions = ?3
        "#,
        params![options.endpoint, options.model, dimensions as i64],
        |row| row.get(0),
    )?;
    if total == 0 {
        return Ok(Vec::new());
    }
    let limit = options.limit.max(1);
    let candidate_limit = total.min(limit.saturating_mul(8).max(32));
    let mut rows = query_memory_vec_index(conn, &table_name, &options, candidate_limit)?;
    if rows.len() < limit && candidate_limit < total {
        rows = query_memory_vec_index(conn, &table_name, &options, total)?;
    }
    rows.truncate(limit);
    Ok(rows)
}

#[cfg(feature = "vec")]
fn query_memory_vec_index(
    conn: &Connection,
    table_name: &str,
    options: &SqliteVecMemorySearchOptions<'_>,
    candidate_limit: usize,
) -> Result<Vec<(String, f64)>> {
    use rusqlite::types::Value as SqlValue;

    let mut sql = format!(
        r#"
        WITH knn AS (
            SELECT embedding_rowid, distance
            FROM {table_name}
            WHERE embedding MATCH ?1 AND k = ?2
              AND endpoint = ?3 AND model = ?4
        )
        SELECT e.memory_id, 1.0 - knn.distance AS score
        FROM knn
        JOIN memory_embeddings e ON e.rowid = knn.embedding_rowid
        JOIN memories m ON m.id = e.memory_id
        WHERE 1 = 1
        "#
    );
    let mut values = vec![
        SqlValue::Text(serde_json::to_string(options.query_embedding)?),
        SqlValue::Integer(candidate_limit as i64),
        SqlValue::Text(options.endpoint.to_string()),
        SqlValue::Text(options.model.to_string()),
    ];
    if !options.types.is_empty() {
        sql.push_str(&format!(
            " AND m.type IN ({})",
            placeholders(options.types.len())
        ));
        values.extend(options.types.iter().cloned().map(SqlValue::Text));
    }
    if !options.statuses.is_empty() {
        sql.push_str(&format!(
            " AND m.status IN ({})",
            placeholders(options.statuses.len())
        ));
        values.extend(options.statuses.iter().cloned().map(SqlValue::Text));
    }
    if let Some(scope) = options.scope {
        sql.push_str(" AND m.scope = ?");
        values.push(SqlValue::Text(scope.to_string()));
    }
    sql.push_str(&format!(
        " ORDER BY knn.distance ASC LIMIT {}",
        options.limit.max(1)
    ));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

#[cfg(feature = "vec")]
pub(crate) struct SqliteVecRagCandidate {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) scope: String,
    pub(crate) chunk_index: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) content: String,
    pub(crate) content_hash: String,
    pub(crate) score: f64,
}

#[cfg(feature = "vec")]
pub(crate) fn sqlite_vec_rag_search(
    conn: &Connection,
    endpoint: &str,
    model: &str,
    query_embedding: &[f32],
    scope: Option<&str>,
    limit: usize,
) -> Result<Vec<SqliteVecRagCandidate>> {
    use rusqlite::types::Value as SqlValue;

    let dimensions = query_embedding.len();
    let table_name = ensure_vec_index(conn, VecIndexKind::Rag, dimensions, false)?;
    let total: usize = conn.query_row(
        r#"
        SELECT COUNT(*) FROM rag_chunk_embeddings
        WHERE endpoint = ?1 AND model = ?2 AND dimensions = ?3
        "#,
        params![endpoint, model, dimensions as i64],
        |row| row.get(0),
    )?;
    if total == 0 {
        return Ok(Vec::new());
    }
    let desired = limit.max(1).saturating_mul(4);
    let mut candidate_limit = total.min(desired.saturating_mul(4).max(32));
    let query_json = serde_json::to_string(query_embedding)?;
    loop {
        let mut sql = format!(
            r#"
            WITH knn AS (
                SELECT embedding_rowid, distance
                FROM {table_name}
                WHERE embedding MATCH ?1 AND k = ?2
                  AND endpoint = ?3 AND model = ?4
            )
            SELECT c.id, c.path, c.scope, c.chunk_index, c.start_line, c.end_line,
                   c.content, e.content_hash, 1.0 - knn.distance AS score
            FROM knn
            JOIN rag_chunk_embeddings e ON e.rowid = knn.embedding_rowid
            JOIN rag_chunks c ON c.id = e.chunk_id
            WHERE 1 = 1
            "#
        );
        let mut values = vec![
            SqlValue::Text(query_json.clone()),
            SqlValue::Integer(candidate_limit as i64),
            SqlValue::Text(endpoint.to_string()),
            SqlValue::Text(model.to_string()),
        ];
        if let Some(scope) = scope {
            sql.push_str(" AND c.scope = ?");
            values.push(SqlValue::Text(scope.to_string()));
        }
        sql.push_str(&format!(
            " ORDER BY knn.distance ASC LIMIT {}",
            desired.max(1)
        ));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values), |row| {
                let chunk_index = row.get::<_, i64>(3)?.max(0) as usize;
                let start_line = row.get::<_, i64>(4)?.max(1) as usize;
                let end_line = row.get::<_, i64>(5)?.max(start_line as i64) as usize;
                Ok(SqliteVecRagCandidate {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    scope: row.get(2)?,
                    chunk_index,
                    start_line,
                    end_line,
                    content: row.get(6)?,
                    content_hash: row.get(7)?,
                    score: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() >= desired || candidate_limit == total {
            return Ok(rows);
        }
        candidate_limit = total;
    }
}

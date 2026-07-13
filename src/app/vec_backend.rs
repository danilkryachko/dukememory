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
    let query_json = serde_json::to_string(options.query_embedding)?;
    let mut sql = format!(
        r#"
        SELECT e.memory_id, vec_distance_cosine(e.embedding, ?) AS distance
        FROM memory_embeddings e
        JOIN memories m ON m.id = e.memory_id
        WHERE e.endpoint = ? AND e.model = ? AND e.dimensions = {}
        "#,
        options.query_embedding.len()
    );
    let mut values = vec![
        query_json,
        options.endpoint.to_string(),
        options.model.to_string(),
    ];
    if !options.types.is_empty() {
        sql.push_str(&format!(
            " AND m.type IN ({})",
            placeholders(options.types.len())
        ));
        values.extend(options.types.iter().cloned());
    }
    if !options.statuses.is_empty() {
        sql.push_str(&format!(
            " AND m.status IN ({})",
            placeholders(options.statuses.len())
        ));
        values.extend(options.statuses.iter().cloned());
    }
    if let Some(scope) = options.scope {
        sql.push_str(" AND m.scope = ?");
        values.push(scope.to_string());
    }
    sql.push_str(&format!(
        " ORDER BY distance ASC LIMIT {}",
        options.limit.max(1)
    ));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        let distance = row.get::<_, f64>(1)?;
        Ok((row.get::<_, String>(0)?, 1.0 - distance))
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
) -> Result<Vec<SqliteVecRagCandidate>> {
    let query_json = serde_json::to_string(query_embedding)?;
    let mut sql = format!(
        r#"
        SELECT c.id, c.path, c.scope, c.chunk_index, c.start_line, c.end_line,
               c.content, e.content_hash,
               vec_distance_cosine(e.embedding, ?) AS distance
        FROM rag_chunk_embeddings e
        JOIN rag_chunks c ON c.id = e.chunk_id
        WHERE e.endpoint = ? AND e.model = ? AND e.dimensions = {}
        "#,
        query_embedding.len()
    );
    let mut values = vec![query_json, endpoint.to_string(), model.to_string()];
    if let Some(scope) = scope {
        sql.push_str(" AND c.scope = ?");
        values.push(scope.to_string());
    }
    sql.push_str(" ORDER BY distance ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        let chunk_index = row.get::<_, i64>(3)?.max(0) as usize;
        let start_line = row.get::<_, i64>(4)?.max(1) as usize;
        let end_line = row.get::<_, i64>(5)?.max(start_line as i64) as usize;
        let distance = row.get::<_, f64>(8)?;
        Ok(SqliteVecRagCandidate {
            id: row.get(0)?,
            path: row.get(1)?,
            scope: row.get(2)?,
            chunk_index,
            start_line,
            end_line,
            content: row.get(6)?,
            content_hash: row.get(7)?,
            score: 1.0 - distance,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

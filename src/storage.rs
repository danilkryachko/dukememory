use anyhow::Result;
use rusqlite::Connection;

#[derive(Clone, Copy)]
pub(crate) struct MemoryStore<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub(crate) fn connection(&self) -> &'a Connection {
        self.conn
    }

    pub(crate) fn memory_count(&self) -> Result<i64> {
        self.count_table("memories")
    }

    pub(crate) fn active_memory_count(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn event_count(&self) -> Result<i64> {
        self.count_table("memory_events")
    }

    pub(crate) fn inbox_pending_count(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn embedding_count(&self) -> Result<i64> {
        self.count_table("memory_embeddings")
    }

    pub(crate) fn schema_version(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_versions",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn count_table(&self, table: &str) -> Result<i64> {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        self.conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(Into::into)
    }
}

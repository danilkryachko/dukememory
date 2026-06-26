use anyhow::Result;
use rusqlite::Connection;

pub struct MemoryStore<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn connection(&self) -> &'a Connection {
        self.conn
    }

    pub fn memory_count(&self) -> Result<i64> {
        self.count_table("memories")
    }

    pub fn active_memory_count(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn event_count(&self) -> Result<i64> {
        self.count_table("memory_events")
    }

    pub fn inbox_pending_count(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn embedding_count(&self) -> Result<i64> {
        self.count_table("memory_embeddings")
    }

    pub fn schema_version(&self) -> Result<i64> {
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

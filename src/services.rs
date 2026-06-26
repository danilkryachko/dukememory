use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::storage::MemoryStore;

pub struct MemoryService<'a> {
    store: MemoryStore<'a>,
}

impl<'a> MemoryService<'a> {
    pub fn new(store: MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub fn stats(&self) -> Result<MemoryStats> {
        Ok(MemoryStats {
            total: self.store.memory_count()?,
            active: self.store.active_memory_count()?,
            pending_inbox: self.store.inbox_pending_count()?,
            embeddings: self.store.embedding_count()?,
            events: self.store.event_count()?,
            schema: self.store.schema_version()?,
        })
    }

    pub fn store(&self) -> &MemoryStore<'a> {
        &self.store
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryStats {
    pub total: i64,
    pub active: i64,
    pub pending_inbox: i64,
    pub embeddings: i64,
    pub events: i64,
    pub schema: i64,
}

pub struct RetrievalService<'a> {
    store: &'a MemoryStore<'a>,
}

impl<'a> RetrievalService<'a> {
    pub fn new(store: &'a MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub fn fts_probe(&self, query: &str, limit: usize) -> Result<usize> {
        let sanitized = query.replace('"', " ");
        let mut stmt = self.store.connection().prepare(
            "SELECT m.id FROM memories m JOIN memories_fts fts ON fts.rowid = m.rowid WHERE memories_fts MATCH ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![sanitized, limit.min(i64::MAX as usize) as i64],
            |_| Ok(()),
        )?;
        Ok(rows.count())
    }
}

pub struct MaintenanceService<'a> {
    store: &'a MemoryStore<'a>,
}

impl<'a> MaintenanceService<'a> {
    pub fn new(store: &'a MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub fn pending_work_count(&self) -> Result<i64> {
        self.store.inbox_pending_count()
    }
}

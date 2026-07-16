use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

use crate::app::memory::{
    AddMemory, UpdateMemory, add_memory, delete_memory, get_memory_with_links, set_status,
    update_memory,
};
use crate::app::model::MemoryWithLinks;
use crate::domain::MemoryStatus;
use crate::storage::MemoryStore;

pub(crate) struct MemoryApplication<'a> {
    store: MemoryStore<'a>,
}

impl<'a> MemoryApplication<'a> {
    pub(crate) fn new(store: MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub(crate) fn create(&self, input: AddMemory) -> Result<String> {
        add_memory(self.store.connection(), input)
    }

    pub(crate) fn update(&self, input: UpdateMemory) -> Result<()> {
        update_memory(self.store.connection(), input)
    }

    pub(crate) fn delete(&self, id: &str) -> Result<()> {
        delete_memory(self.store.connection(), id)
    }

    pub(crate) fn set_status(&self, id: &str, status: MemoryStatus) -> Result<()> {
        set_status(self.store.connection(), id, status)
    }

    pub(crate) fn get_with_links(&self, id: &str) -> Result<MemoryWithLinks> {
        get_memory_with_links(self.store.connection(), id)
    }

    pub(crate) fn stats(&self) -> Result<MemoryStats> {
        Ok(MemoryStats {
            total: self.store.memory_count()?,
            active: self.store.active_memory_count()?,
            pending_inbox: self.store.inbox_pending_count()?,
            embeddings: self.store.embedding_count()?,
            events: self.store.event_count()?,
            schema: self.store.schema_version()?,
        })
    }

    pub(crate) fn store(&self) -> &MemoryStore<'a> {
        &self.store
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryStats {
    pub(crate) total: i64,
    pub(crate) active: i64,
    pub(crate) pending_inbox: i64,
    pub(crate) embeddings: i64,
    pub(crate) events: i64,
    pub(crate) schema: i64,
}

pub(crate) struct RetrievalApplication<'a> {
    store: &'a MemoryStore<'a>,
}

impl<'a> RetrievalApplication<'a> {
    pub(crate) fn new(store: &'a MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub(crate) fn fts_probe(&self, query: &str, limit: usize) -> Result<usize> {
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

pub(crate) struct MaintenanceApplication<'a> {
    store: &'a MemoryStore<'a>,
}

impl<'a> MaintenanceApplication<'a> {
    pub(crate) fn new(store: &'a MemoryStore<'a>) -> Self {
        Self { store }
    }

    pub(crate) fn pending_work_count(&self) -> Result<i64> {
        self.store.inbox_pending_count()
    }
}

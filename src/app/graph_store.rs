use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use std::collections::HashSet;

use super::{MemoryLink, now_ms, validate_confidence};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredMemoryEdge {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) kind: String,
    pub(crate) confidence: f64,
    pub(crate) provenance: String,
}

pub(crate) fn memory_edge_is_symmetric(kind: &str) -> bool {
    matches!(kind, "relates_to")
}

pub(crate) fn canonical_memory_edge<'a>(
    source_id: &'a str,
    target_id: &'a str,
    kind: &str,
) -> (&'a str, &'a str) {
    if memory_edge_is_symmetric(kind) && source_id > target_id {
        (target_id, source_id)
    } else {
        (source_id, target_id)
    }
}

pub(crate) fn insert_memory_edge(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
    kind: &str,
    confidence: f64,
    provenance: &str,
) -> Result<bool> {
    validate_confidence(confidence)?;
    let kind = kind.trim();
    let provenance = provenance.trim();
    if kind.is_empty() {
        bail!("memory edge kind must not be empty");
    }
    if provenance.is_empty() {
        bail!("memory edge provenance must not be empty");
    }
    if source_id == target_id {
        bail!("memory edge must connect two different memories");
    }
    let (source_id, target_id) = canonical_memory_edge(source_id, target_id, kind);
    let changed = conn.execute(
        "INSERT OR IGNORE INTO memory_edges \
         (source_id, target_id, kind, confidence, provenance, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![source_id, target_id, kind, confidence, provenance, now_ms()],
    )?;
    Ok(changed == 1)
}

pub(crate) fn list_memory_edges(conn: &Connection) -> Result<Vec<StoredMemoryEdge>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, target_id, kind, confidence, provenance \
         FROM memory_edges ORDER BY source_id, target_id, kind",
    )?;
    stmt.query_map([], |row| {
        Ok(StoredMemoryEdge {
            source_id: row.get(0)?,
            target_id: row.get(1)?,
            kind: row.get(2)?,
            confidence: row.get(3)?,
            provenance: row.get(4)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn graph_neighbor_edges(
    conn: &Connection,
    memory_id: &str,
) -> Result<Vec<StoredMemoryEdge>> {
    let mut edges = list_memory_edges(conn)?
        .into_iter()
        .filter(|edge| {
            edge.source_id == memory_id
                || (edge.target_id == memory_id && memory_edge_is_symmetric(&edge.kind))
        })
        .collect::<Vec<_>>();

    let mut stmt = conn.prepare(
        "SELECT l.memory_id, l.target, l.kind \
         FROM memory_links l \
         JOIN memories target ON target.id = l.target \
         WHERE l.memory_id = ?1 OR (l.target = ?1 AND l.kind = 'relates_to')",
    )?;
    let legacy = stmt
        .query_map(params![memory_id], |row| {
            Ok(StoredMemoryEdge {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
                kind: row.get(2)?,
                confidence: 1.0,
                provenance: "legacy_memory_link".to_string(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    edges.extend(legacy);
    deduplicate_edges(&mut edges);
    Ok(edges)
}

pub(crate) fn graph_edges_for_nodes(
    conn: &Connection,
    selected_ids: &HashSet<String>,
) -> Result<Vec<StoredMemoryEdge>> {
    let mut edges = list_memory_edges(conn)?
        .into_iter()
        .filter(|edge| {
            selected_ids.contains(&edge.source_id) && selected_ids.contains(&edge.target_id)
        })
        .collect::<Vec<_>>();

    for source_id in selected_ids {
        let mut stmt = conn.prepare(
            "SELECT l.memory_id, l.target, l.kind \
             FROM memory_links l JOIN memories target ON target.id = l.target \
             WHERE l.memory_id = ?1",
        )?;
        let legacy = stmt
            .query_map(params![source_id], |row| {
                Ok(StoredMemoryEdge {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    kind: row.get(2)?,
                    confidence: 1.0,
                    provenance: "legacy_memory_link".to_string(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        edges.extend(
            legacy
                .into_iter()
                .filter(|edge| selected_ids.contains(&edge.target_id)),
        );
    }
    deduplicate_edges(&mut edges);
    Ok(edges)
}

pub(crate) fn edge_as_link_for(edge: &StoredMemoryEdge, memory_id: &str) -> Option<MemoryLink> {
    if edge.source_id == memory_id {
        Some(MemoryLink {
            kind: edge.kind.clone(),
            target: edge.target_id.clone(),
        })
    } else if edge.target_id == memory_id && memory_edge_is_symmetric(&edge.kind) {
        Some(MemoryLink {
            kind: edge.kind.clone(),
            target: edge.source_id.clone(),
        })
    } else {
        None
    }
}

fn deduplicate_edges(edges: &mut Vec<StoredMemoryEdge>) {
    edges.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.target_id.cmp(&right.target_id))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    edges.dedup_by(|left, right| {
        left.source_id == right.source_id
            && left.target_id == right.target_id
            && left.kind == right.kind
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relates_to_edges_have_a_stable_canonical_direction() {
        assert_eq!(canonical_memory_edge("z", "a", "relates_to"), ("a", "z"));
        assert_eq!(canonical_memory_edge("z", "a", "depends_on"), ("z", "a"));
    }
}

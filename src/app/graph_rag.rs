use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::app::generation;
use crate::app::memory::{get_links, get_memory};
use crate::app::model::Memory;
use crate::app::retrieval::{SearchRowsRequest, search_rows_with_semantic_fallback};
use crate::runtime_config::GenerationConfig;

use super::{DEFAULT_EMBED_ENDPOINT, DEFAULT_EMBED_MODEL, DEFAULT_EMBED_PROVIDER};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphRagReport {
    pub(crate) query: String,
    pub(crate) answer: String,
    pub(crate) citations: Vec<String>,
    pub(crate) relevant_nodes: Vec<Memory>,
    pub(crate) relevant_edges: Vec<GraphRagEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphRagEdge {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) kind: String,
}

pub(crate) fn compute_graph_rag(
    conn: &Connection,
    query: &str,
    scope: Option<&str>,
    limit: usize,
    gen_config: &GenerationConfig,
) -> Result<GraphRagReport> {
    let limit = limit.clamp(3, 16);
    let statuses = vec!["active".to_string(), "uncertain".to_string()];

    // 1. Semantic Search for Entry Nodes
    let (initial_rows, _) = search_rows_with_semantic_fallback(
        conn,
        SearchRowsRequest {
            query,
            types: &[],
            statuses: &statuses,
            scope,
            limit,
            budget: 1_600,
            provider: DEFAULT_EMBED_PROVIDER,
            endpoint: DEFAULT_EMBED_ENDPOINT,
            model: DEFAULT_EMBED_MODEL,
        },
    )?;

    if initial_rows.is_empty() {
        return Ok(GraphRagReport {
            query: query.to_string(),
            answer: "No relevant memory cards found to answer the question.".to_string(),
            citations: vec![],
            relevant_nodes: vec![],
            relevant_edges: vec![],
        });
    }

    let mut nodes_map: HashMap<String, Memory> = HashMap::new();
    let mut expanded_ids: HashSet<String> = HashSet::new();

    for row in initial_rows.iter().take(limit) {
        nodes_map.insert(row.id.clone(), row.clone());
        expanded_ids.insert(row.id.clone());
    }

    let mut relevant_edges: Vec<GraphRagEdge> = Vec::new();

    // 2. Expand 1 hop using links
    let current_ids: Vec<String> = expanded_ids.iter().cloned().collect();
    for id in &current_ids {
        if let Ok(links) = get_links(conn, id) {
            for link in links {
                expanded_ids.insert(link.target.clone());
                relevant_edges.push(GraphRagEdge {
                    source: id.clone(),
                    target: link.target.clone(),
                    kind: link.kind.clone(),
                });
            }
        }
    }

    for id in &expanded_ids {
        if !nodes_map.contains_key(id) {
            if let Ok(mem) = get_memory(conn, id) {
                nodes_map.insert(id.clone(), mem);
            }
        }
    }

    let mut final_edges: Vec<GraphRagEdge> = Vec::new();
    for id in &expanded_ids {
        if let Ok(links) = get_links(conn, id) {
            for link in links {
                if expanded_ids.contains(&link.target) {
                    final_edges.push(GraphRagEdge {
                        source: id.clone(),
                        target: link.target.clone(),
                        kind: link.kind.clone(),
                    });
                }
            }
        }
    }

    final_edges.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.target.cmp(&b.target))
    });
    final_edges.dedup_by(|a, b| a.source == b.source && a.target == b.target && a.kind == b.kind);

    let relevant_nodes: Vec<Memory> = nodes_map.values().cloned().collect();

    // 3. Build Prompt
    let mut prompt = String::new();
    prompt.push_str(
        "You are a knowledgeable assistant that answers questions about a software codebase.\n",
    );
    prompt.push_str("Use the following knowledge graph context to inform your answer.\n");
    prompt
        .push_str("Reference specific components, decisions, and their explicit relationships.\n");
    prompt.push_str("Be concise but thorough — link concepts to actual memory [id] locations.\n\n");
    prompt.push_str("---\n\n");

    prompt.push_str("## Relevant Context Nodes\n\n");
    for node in &relevant_nodes {
        prompt.push_str(&format!(
            "### [{}] {} ({})\n",
            node.id, node.title, node.memory_type
        ));
        prompt.push_str(&format!("- **Summary**: {}\n", node.body));
        prompt.push_str("\n");
    }

    if !final_edges.is_empty() {
        prompt.push_str("## Relationships\n\n");
        for edge in &final_edges {
            let src_title = nodes_map
                .get(&edge.source)
                .map(|m| m.title.as_str())
                .unwrap_or(&edge.source);
            let tgt_title = nodes_map
                .get(&edge.target)
                .map(|m| m.title.as_str())
                .unwrap_or(&edge.target);
            prompt.push_str(&format!(
                "- [{}] {} --[{}]--> [{}] {}\n",
                edge.source, src_title, edge.kind, edge.target, tgt_title
            ));
        }
        prompt.push_str("\n");
    }
    prompt.push_str("---\n\n");
    prompt.push_str(&format!("**User question:** {}\n", query));

    // 4. Call LLM
    let answer = generation::generate_answer(
        &gen_config.provider,
        &gen_config.endpoint,
        &gen_config.model,
        &prompt,
    )?;

    let citations: Vec<String> = initial_rows
        .iter()
        .take(limit)
        .map(|m| m.id.clone())
        .collect();

    Ok(GraphRagReport {
        query: query.to_string(),
        answer,
        citations,
        relevant_nodes,
        relevant_edges: final_edges,
    })
}

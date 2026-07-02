use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::app::generation::generate_answer;
use crate::app::memory::{get_links, get_memory};
use crate::runtime_config::GenerationConfig;

pub(crate) fn explain_component(
    conn: &Connection,
    memory_id: &str,
    gen_config: &GenerationConfig,
) -> Result<String> {
    let target_node = get_memory(conn, memory_id)
        .context(format!("Failed to find memory card with id {}", memory_id))?;

    let mut child_nodes = Vec::new();
    let mut connected_nodes = Vec::new();
    let mut relevant_edges = Vec::new();

    if let Ok(links) = get_links(conn, memory_id) {
        for link in links {
            relevant_edges.push(link.clone());
            if link.kind == "contains" || link.kind == "child_of" {
                if let Ok(child_mem) = get_memory(conn, &link.target) {
                    child_nodes.push(child_mem);
                }
            } else {
                if let Ok(connected_mem) = get_memory(conn, &link.target) {
                    connected_nodes.push(connected_mem);
                }
            }
        }
    }

    let mut prompt = String::new();
    prompt.push_str(
        "You are an expert software architect analyzing a specific codebase component.\n",
    );
    prompt.push_str(
        "Provide a thorough explanation of this component based on the graph context below:\n",
    );
    prompt.push_str("1. What it does and why it exists in the project\n");
    prompt.push_str("2. How data flows through it (inputs, processing, outputs)\n");
    prompt.push_str("3. How it interacts with connected components\n");
    prompt.push_str("4. Any patterns, idioms, or design decisions worth noting\n");
    prompt.push_str("5. Potential gotchas or areas of complexity\n\n");
    prompt.push_str("---\n\n");

    prompt.push_str(&format!(
        "# Deep Dive: {} ({})\n",
        target_node.title, target_node.memory_type
    ));
    prompt.push_str(&format!("**Summary:** {}\n\n", target_node.body));

    if !child_nodes.is_empty() {
        prompt.push_str("## Internal Components\n");
        for child in &child_nodes {
            prompt.push_str(&format!(
                "- **[{}] {}** ({}): {}\n",
                child.id,
                child.title,
                child.memory_type,
                child.body.lines().next().unwrap_or("")
            ));
        }
        prompt.push_str("\n");
    }

    if !connected_nodes.is_empty() {
        prompt.push_str("## Connected Components\n");
        for node in &connected_nodes {
            prompt.push_str(&format!(
                "- **[{}] {}** ({}): {}\n",
                node.id,
                node.title,
                node.memory_type,
                node.body.lines().next().unwrap_or("")
            ));
        }
        prompt.push_str("\n");
    }

    if !relevant_edges.is_empty() {
        prompt.push_str("## Relationships\n");
        for edge in &relevant_edges {
            prompt.push_str(&format!(
                "- {} --[{}]--> {}\n",
                memory_id, edge.kind, edge.target
            ));
        }
        prompt.push_str("\n");
    }

    prompt.push_str("---\n\n");

    let answer = generate_answer(
        &gen_config.provider,
        &gen_config.endpoint,
        &gen_config.model,
        &prompt,
    )?;

    Ok(answer)
}

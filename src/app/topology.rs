use crate::app::memory::{get_links, query_memories};
use crate::app::model::Memory;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) struct TopologyResult {
    pub(crate) entry_points: Vec<String>,
    pub(crate) fan_in: Vec<(String, usize)>,
    pub(crate) fan_out: Vec<(String, usize)>,
    pub(crate) bfs_by_depth: HashMap<usize, Vec<String>>,
    pub(crate) nodes: HashMap<String, Memory>,
    pub(crate) total_nodes: usize,
    pub(crate) total_edges: usize,
}

pub(crate) fn compute_topology(conn: &Connection) -> Result<TopologyResult> {
    // 1. Fetch all active memories
    let memories = query_memories(conn, None, &[], &["active".to_string()], None, usize::MAX)?;
    let mut nodes: HashMap<String, Memory> = HashMap::new();
    let mut adjacency_out: HashMap<String, Vec<String>> = HashMap::new();
    let mut fan_in_counts: HashMap<String, usize> = HashMap::new();
    let mut fan_out_counts: HashMap<String, usize> = HashMap::new();

    for memory in memories.into_iter() {
        let id = memory.id.clone();
        nodes.insert(id.clone(), memory);
        adjacency_out.entry(id.clone()).or_default();
        fan_in_counts.entry(id.clone()).or_insert(0);
        fan_out_counts.entry(id.clone()).or_insert(0);
    }

    let mut total_edges = 0;

    // 2. Build graph from links
    for node_id in nodes.keys() {
        let links = get_links(conn, node_id)?;
        for link in links {
            if nodes.contains_key(&link.target) {
                adjacency_out
                    .get_mut(node_id)
                    .unwrap()
                    .push(link.target.clone());
                *fan_out_counts.get_mut(node_id).unwrap() += 1;
                *fan_in_counts.entry(link.target.clone()).or_insert(0) += 1;
                total_edges += 1;
            }
        }
    }

    // 3. Compute rankings
    let mut fan_in: Vec<_> = fan_in_counts.clone().into_iter().collect();
    fan_in.sort_by(|a, b| b.1.cmp(&a.1)); // descending

    let mut fan_out: Vec<_> = fan_out_counts.clone().into_iter().collect();
    fan_out.sort_by(|a, b| b.1.cmp(&a.1)); // descending

    // 4. Find Entry Points (Heuristics: low fan_in, high fan_out, or specific types)
    let mut entry_candidates: Vec<(String, isize)> = Vec::new();
    for (id, memory) in &nodes {
        let mut score = 0;
        let r_fan_in = *fan_in_counts.get(id).unwrap_or(&0);
        let r_fan_out = *fan_out_counts.get(id).unwrap_or(&0);

        if r_fan_in == 0 {
            score += 2;
        }
        if r_fan_out > 2 {
            score += 1;
        }
        if memory.memory_type == "product_goal" || memory.memory_type == "architecture" {
            score += 5;
        }
        entry_candidates.push((id.clone(), score));
    }
    entry_candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let entry_points: Vec<String> = entry_candidates
        .into_iter()
        .take(5)
        .map(|(id, _)| id)
        .collect();

    // 5. BFS Traversal starting from the top entry point
    let mut bfs_by_depth: HashMap<usize, Vec<String>> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();

    if let Some(start_node) = entry_points.first() {
        queue.push_back((start_node.clone(), 0));
        visited.insert(start_node.clone());

        while let Some((curr, depth)) = queue.pop_front() {
            bfs_by_depth.entry(depth).or_default().push(curr.clone());

            if let Some(neighbors) = adjacency_out.get(&curr) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        visited.insert(neighbor.clone());
                        queue.push_back((neighbor.clone(), depth + 1));
                    }
                }
            }
        }
    }

    // Include isolated or unvisited nodes at a deep layer
    let mut unvisited: Vec<String> = Vec::new();
    for id in nodes.keys() {
        if !visited.contains(id) {
            unvisited.push(id.clone());
        }
    }
    if !unvisited.is_empty() {
        let max_depth = bfs_by_depth.keys().max().copied().unwrap_or(0);
        bfs_by_depth.insert(max_depth + 1, unvisited);
    }

    Ok(TopologyResult {
        entry_points,
        fan_in,
        fan_out,
        bfs_by_depth,
        total_nodes: nodes.len(),
        total_edges,
        nodes,
    })
}

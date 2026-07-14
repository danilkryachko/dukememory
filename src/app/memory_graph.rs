use super::*;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryGraphLinksReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) limit: usize,
    pub(crate) scanned_memories: usize,
    pub(crate) candidate_count: usize,
    pub(crate) safe_candidate_count: usize,
    pub(crate) existing_count: usize,
    pub(crate) applied_count: usize,
    pub(crate) candidates: Vec<MemoryGraphLinkCandidate>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryGraphLinkCandidate {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) source_title: String,
    pub(crate) target_title: String,
    pub(crate) kind: String,
    pub(crate) confidence: f64,
    pub(crate) reasons: Vec<String>,
    pub(crate) exists: bool,
    pub(crate) safe_to_apply: bool,
    pub(crate) applied: bool,
}

pub(crate) fn print_memory_graph_links(
    conn: &Connection,
    root: &Path,
    limit: usize,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_graph_links_report(conn, root, limit, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Graph Links");
    println!("applied: {}", report.applied);
    println!("candidates: {}", report.candidate_count);
    println!("safe: {}", report.safe_candidate_count);
    println!("written: {}", report.applied_count);
    for candidate in &report.candidates {
        println!(
            "{} --[{} {:.2}]--> {}{}",
            candidate.source_id,
            candidate.kind,
            candidate.confidence,
            candidate.target_id,
            if candidate.exists { " (exists)" } else { "" }
        );
    }
    Ok(())
}

pub(crate) fn memory_graph_links_report(
    conn: &Connection,
    root: &Path,
    limit: usize,
    apply: bool,
) -> Result<MemoryGraphLinksReport> {
    let statuses = vec!["active".to_string(), "uncertain".to_string()];
    let memories = query_memories(conn, None, &[], &statuses, None, 500)?;
    let memory_ids = memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<HashSet<_>>();
    let mut indexed = Vec::new();
    for memory in memories {
        let links = get_links(conn, &memory.id)?;
        indexed.push(MemoryGraphIndexedMemory {
            file_links: memory_graph_file_links(&links),
            terms: memory_graph_terms(&memory),
            links,
            memory,
        });
    }
    let existing_edges = memory_graph_existing_edges(conn, &indexed, &memory_ids)?;
    let file_counts = memory_graph_file_counts(&indexed);
    let mut candidates: HashMap<(String, String, String), MemoryGraphLinkCandidate> =
        HashMap::new();

    for source in &indexed {
        let haystack = format!(
            "{} {}",
            source.memory.title.to_lowercase(),
            source.memory.body.to_lowercase()
        );
        for target in &indexed {
            if source.memory.id == target.memory.id {
                continue;
            }
            if haystack.contains(&target.memory.id.to_lowercase()) {
                memory_graph_add_candidate(
                    &mut candidates,
                    source,
                    target,
                    "relates_to",
                    0.96,
                    "explicit_memory_id_mention".to_string(),
                    &existing_edges,
                );
            }
        }
    }

    for left_index in 0..indexed.len() {
        for right_index in (left_index + 1)..indexed.len() {
            let left = &indexed[left_index];
            let right = &indexed[right_index];
            let shared_files = left
                .file_links
                .intersection(&right.file_links)
                .filter(|file| file_counts.get(*file).copied().unwrap_or(0) <= 6)
                .cloned()
                .collect::<Vec<_>>();
            let overlap = memory_graph_term_overlap(&left.terms, &right.terms);
            let topic_ratio = memory_graph_topic_ratio(&left.terms, &right.terms);
            let (source, target) = memory_graph_direction(left, right);
            if !shared_files.is_empty() && overlap >= 3 {
                let confidence = if shared_files.len() >= 2 && overlap >= 5 {
                    0.90
                } else {
                    0.85
                };
                memory_graph_add_candidate(
                    &mut candidates,
                    source,
                    target,
                    "relates_to",
                    confidence,
                    format!(
                        "shared_file:{}",
                        shared_files
                            .iter()
                            .take(2)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    &existing_edges,
                );
                memory_graph_add_candidate(
                    &mut candidates,
                    source,
                    target,
                    "relates_to",
                    confidence,
                    format!("topic_overlap:{overlap}"),
                    &existing_edges,
                );
            } else if overlap >= 6
                && topic_ratio >= 0.55
                && memory_graph_types_compatible(
                    &left.memory.memory_type,
                    &right.memory.memory_type,
                )
            {
                memory_graph_add_candidate(
                    &mut candidates,
                    source,
                    target,
                    "relates_to",
                    0.86,
                    format!("strong_topic_overlap:{overlap}:{topic_ratio:.2}"),
                    &existing_edges,
                );
            }
        }
    }

    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    for candidate in &mut candidates {
        candidate.reasons.sort();
        candidate.reasons.dedup();
        if candidate.reasons.len() >= 2 && candidate.confidence < 0.91 {
            candidate.confidence = (candidate.confidence + 0.01).min(0.91);
        }
        candidate.safe_to_apply = !candidate.exists && candidate.confidence >= 0.92;
    }
    candidates.sort_by(|left, right| {
        right
            .safe_to_apply
            .cmp(&left.safe_to_apply)
            .then_with(|| {
                right
                    .confidence
                    .partial_cmp(&left.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.source_title.cmp(&right.source_title))
            .then_with(|| left.target_title.cmp(&right.target_title))
    });
    let candidate_count = candidates.len();
    let safe_candidate_count = candidates
        .iter()
        .filter(|candidate| candidate.safe_to_apply)
        .count();
    let existing_count = candidates
        .iter()
        .filter(|candidate| candidate.exists)
        .count();
    candidates.truncate(limit.max(1));

    let mut applied_count = 0;
    let mut actions = Vec::new();
    if apply {
        transactional(conn, "apply_memory_graph_links", || {
            for candidate in &mut candidates {
                if !candidate.safe_to_apply {
                    actions.push(format!(
                        "skipped:{}->{} confidence={:.2} exists={}",
                        candidate.source_id,
                        candidate.target_id,
                        candidate.confidence,
                        candidate.exists
                    ));
                    continue;
                }
                let provenance = serde_json::to_string(&json!({
                    "source": "memory_graph_links",
                    "confidence": candidate.confidence,
                    "reasons": candidate.reasons,
                }))?;
                let inserted = insert_memory_edge(
                    conn,
                    &candidate.source_id,
                    &candidate.target_id,
                    &candidate.kind,
                    candidate.confidence,
                    &provenance,
                )?;
                if !inserted {
                    candidate.exists = true;
                    candidate.safe_to_apply = false;
                    actions.push(format!(
                        "skipped:{}->{} already_exists",
                        candidate.source_id, candidate.target_id
                    ));
                    continue;
                }
                log_event(
                    conn,
                    "memory_graph_links",
                    Some(&candidate.source_id),
                    &serde_json::to_string(&json!({
                        "target_id": candidate.target_id,
                        "kind": candidate.kind,
                        "confidence": candidate.confidence,
                        "reasons": candidate.reasons,
                    }))?,
                )?;
                candidate.applied = true;
                applied_count += 1;
                actions.push(format!(
                    "linked:{} --[{}]--> {}",
                    candidate.source_id, candidate.kind, candidate.target_id
                ));
            }
            Ok(())
        })?;
    } else if safe_candidate_count > 0 {
        actions.push("dry_run: safe candidates available".to_string());
    } else {
        actions.push("dry_run: no safe graph links to apply".to_string());
    }

    let mut recommendations = Vec::new();
    if safe_candidate_count == 0 {
        recommendations.push(
            "no high-confidence memory-to-memory links found; keep graph-rag on existing links"
                .to_string(),
        );
    } else if !apply {
        recommendations.push(
            "review candidates and rerun memory-graph-links --apply --json to add safe links"
                .to_string(),
        );
    }
    if existing_count > 0 {
        recommendations.push("existing memory-to-memory links were skipped".to_string());
    }

    Ok(MemoryGraphLinksReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        applied: apply,
        limit,
        scanned_memories: indexed.len(),
        candidate_count,
        safe_candidate_count,
        existing_count,
        applied_count,
        candidates,
        actions,
        recommendations,
    })
}

struct MemoryGraphIndexedMemory {
    memory: Memory,
    links: Vec<MemoryLink>,
    file_links: BTreeSet<String>,
    terms: BTreeSet<String>,
}

fn memory_graph_file_links(links: &[MemoryLink]) -> BTreeSet<String> {
    links
        .iter()
        .filter(|link| link.kind == "file")
        .map(|link| link.target.trim().to_string())
        .filter(|target| !target.is_empty())
        .collect()
}

fn memory_graph_terms(memory: &Memory) -> BTreeSet<String> {
    relevance_terms(&format!("{} {}", memory.title, memory.body))
        .into_iter()
        .filter(|term| term.len() >= 4)
        .collect()
}

fn memory_graph_existing_edges(
    conn: &Connection,
    indexed: &[MemoryGraphIndexedMemory],
    memory_ids: &HashSet<String>,
) -> Result<HashSet<(String, String, String)>> {
    let mut edges = HashSet::new();
    for item in indexed {
        for link in &item.links {
            if memory_ids.contains(&link.target) {
                let (source, target) =
                    canonical_memory_edge(&item.memory.id, &link.target, &link.kind);
                edges.insert((source.to_string(), target.to_string(), link.kind.clone()));
            }
        }
    }
    for edge in list_memory_edges(conn)? {
        let (source, target) = canonical_memory_edge(&edge.source_id, &edge.target_id, &edge.kind);
        edges.insert((source.to_string(), target.to_string(), edge.kind));
    }
    Ok(edges)
}

fn memory_graph_file_counts(indexed: &[MemoryGraphIndexedMemory]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in indexed {
        for file in &item.file_links {
            *counts.entry(file.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn memory_graph_add_candidate(
    candidates: &mut HashMap<(String, String, String), MemoryGraphLinkCandidate>,
    source_memory: &MemoryGraphIndexedMemory,
    target_memory: &MemoryGraphIndexedMemory,
    kind: &str,
    confidence: f64,
    reason: String,
    existing_edges: &HashSet<(String, String, String)>,
) {
    let (source_id, target_id) =
        canonical_memory_edge(&source_memory.memory.id, &target_memory.memory.id, kind);
    let key = (
        source_id.to_string(),
        target_id.to_string(),
        kind.to_string(),
    );
    let entry = candidates
        .entry(key)
        .or_insert_with(|| MemoryGraphLinkCandidate {
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            source_title: truncate_chars(
                if source_id == source_memory.memory.id {
                    &source_memory.memory.title
                } else {
                    &target_memory.memory.title
                },
                90,
            ),
            target_title: truncate_chars(
                if target_id == target_memory.memory.id {
                    &target_memory.memory.title
                } else {
                    &source_memory.memory.title
                },
                90,
            ),
            kind: kind.to_string(),
            confidence,
            reasons: Vec::new(),
            exists: existing_edges.contains(&(
                source_id.to_string(),
                target_id.to_string(),
                kind.to_string(),
            )),
            safe_to_apply: false,
            applied: false,
        });
    entry.confidence = entry.confidence.max(confidence);
    entry.reasons.push(reason);
}

fn memory_graph_direction<'a>(
    left: &'a MemoryGraphIndexedMemory,
    right: &'a MemoryGraphIndexedMemory,
) -> (&'a MemoryGraphIndexedMemory, &'a MemoryGraphIndexedMemory) {
    if left.memory.updated_at >= right.memory.updated_at {
        (left, right)
    } else {
        (right, left)
    }
}

fn memory_graph_term_overlap(left: &BTreeSet<String>, right: &BTreeSet<String>) -> usize {
    left.intersection(right).count()
}

fn memory_graph_topic_ratio(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    let overlap = memory_graph_term_overlap(left, right);
    let smaller = left.len().min(right.len());
    if smaller == 0 {
        0.0
    } else {
        overlap as f64 / smaller as f64
    }
}

fn memory_graph_types_compatible(left: &str, right: &str) -> bool {
    left == right
        || matches!(
            (left, right),
            ("design_note", "decision")
                | ("decision", "design_note")
                | ("design_note", "task_state")
                | ("task_state", "design_note")
                | ("known_issue", "task_state")
                | ("task_state", "known_issue")
        )
}

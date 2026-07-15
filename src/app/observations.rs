use super::*;

pub(crate) const OBSERVATION_KINDS: &[&str] = &[
    "asserted",
    "verified",
    "contradicted",
    "superseded",
    "file_changed",
    "retrieved",
    "outcome",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryObservation {
    pub(crate) id: String,
    pub(crate) memory_id: String,
    pub(crate) target_memory_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) statement: String,
    pub(crate) evidence_kind: String,
    pub(crate) evidence_ref: String,
    pub(crate) confidence: f64,
    pub(crate) valid_from: i64,
    pub(crate) valid_to: Option<i64>,
    pub(crate) observed_at: i64,
    pub(crate) branch: Option<String>,
    pub(crate) commit_hash: Option<String>,
    pub(crate) worktree_root: Option<String>,
}

pub(crate) struct MemoryObservationRequest<'a> {
    pub(crate) memory_id: &'a str,
    pub(crate) target_memory_id: Option<&'a str>,
    pub(crate) kind: &'a str,
    pub(crate) statement: &'a str,
    pub(crate) evidence_kind: &'a str,
    pub(crate) evidence_ref: &'a str,
    pub(crate) confidence: f64,
    pub(crate) valid_from: Option<i64>,
    pub(crate) valid_to: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TemporalMemoryGraphReport {
    pub(crate) version: u32,
    pub(crate) valid_at: i64,
    pub(crate) known_at: i64,
    pub(crate) node_count: usize,
    pub(crate) edge_count: usize,
    pub(crate) observation_count: usize,
    pub(crate) nodes: Vec<TemporalMemoryGraphNode>,
    pub(crate) edges: Vec<TemporalMemoryGraphEdge>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TemporalMemoryGraphNode {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TemporalMemoryGraphEdge {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) kind: String,
    pub(crate) confidence: f64,
    pub(crate) provenance: String,
    pub(crate) valid_from: i64,
    pub(crate) valid_to: Option<i64>,
    pub(crate) observed_at: i64,
    pub(crate) observation_id: Option<String>,
}

pub(crate) fn record_memory_observation(
    conn: &Connection,
    root: &Path,
    request: &MemoryObservationRequest<'_>,
) -> Result<MemoryObservation> {
    let kind = request.kind.trim().to_ascii_lowercase();
    if !OBSERVATION_KINDS.contains(&kind.as_str()) {
        bail!(
            "invalid observation kind: {}; expected {}",
            request.kind,
            OBSERVATION_KINDS.join(", ")
        );
    }
    let statement = request.statement.trim();
    let evidence_kind = request.evidence_kind.trim();
    let evidence_ref = request.evidence_ref.trim();
    if statement.is_empty() || evidence_kind.is_empty() || evidence_ref.is_empty() {
        bail!("observation statement, evidence kind, and evidence ref must not be empty");
    }
    validate_confidence(request.confidence)?;
    get_memory(conn, request.memory_id)?;
    if let Some(target) = request.target_memory_id {
        if target == request.memory_id {
            bail!("observation target must differ from source memory");
        }
        get_memory(conn, target)?;
    }
    let observed_at = now_ms();
    let valid_from = request.valid_from.unwrap_or(observed_at);
    if request
        .valid_to
        .is_some_and(|valid_to| valid_to < valid_from)
    {
        bail!("observation valid_to must be greater than or equal to valid_from");
    }
    let id = Uuid::new_v4().simple().to_string();
    let branch = git_value(root, &["branch", "--show-current"]);
    let commit_hash = git_value(root, &["rev-parse", "HEAD"]);
    let worktree_root = git_value(root, &["rev-parse", "--show-toplevel"]);
    transactional(conn, "record_memory_observation", || {
        conn.execute(
            "INSERT INTO memory_observations (id, memory_id, target_memory_id, kind, statement, evidence_kind, evidence_ref, confidence, valid_from, valid_to, observed_at, branch, commit_hash, worktree_root) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                request.memory_id,
                request.target_memory_id,
                kind,
                statement,
                evidence_kind,
                evidence_ref,
                request.confidence,
                valid_from,
                request.valid_to,
                observed_at,
                branch,
                commit_hash,
                worktree_root,
            ],
        )?;
        if let Some(target) = request.target_memory_id {
            let edge_kind = observation_edge_kind(&kind);
            let provenance = serde_json::to_string(&json!({
                "observation_id": id,
                "evidence_kind": evidence_kind,
                "evidence_ref": evidence_ref,
            }))?;
            conn.execute(
                "INSERT INTO memory_edges (source_id, target_id, kind, confidence, provenance, created_at, valid_from, valid_to, observed_at, observation_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(source_id, target_id, kind) DO UPDATE SET confidence=excluded.confidence, provenance=excluded.provenance, valid_from=excluded.valid_from, valid_to=excluded.valid_to, observed_at=excluded.observed_at, observation_id=excluded.observation_id",
                params![
                    request.memory_id,
                    target,
                    edge_kind,
                    request.confidence,
                    provenance,
                    observed_at,
                    valid_from,
                    request.valid_to,
                    observed_at,
                    id,
                ],
            )?;
        }
        log_event(
            conn,
            "memory_observation",
            Some(request.memory_id),
            &serde_json::to_string(&json!({
                "observation_id": id,
                "kind": kind,
                "evidence_kind": evidence_kind,
                "evidence_ref": evidence_ref,
                "target_memory_id": request.target_memory_id,
                "valid_from": valid_from,
                "valid_to": request.valid_to,
            }))?,
        )
    })?;
    get_memory_observation(conn, &id)
}

pub(crate) fn list_memory_observations(
    conn: &Connection,
    memory_id: &str,
    valid_at: Option<i64>,
    known_at: Option<i64>,
    limit: usize,
) -> Result<Vec<MemoryObservation>> {
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, target_memory_id, kind, statement, evidence_kind, evidence_ref, confidence, valid_from, valid_to, observed_at, branch, commit_hash, worktree_root \
         FROM memory_observations \
         WHERE memory_id = ?1 \
           AND (?2 IS NULL OR (valid_from <= ?2 AND (valid_to IS NULL OR valid_to >= ?2))) \
           AND (?3 IS NULL OR observed_at <= ?3) \
         ORDER BY observed_at DESC, id DESC LIMIT ?4",
    )?;
    stmt.query_map(
        params![memory_id, valid_at, known_at, limit.clamp(1, 1_000) as i64],
        row_to_observation,
    )?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(crate) fn temporal_memory_graph_report(
    conn: &Connection,
    valid_at: Option<i64>,
    known_at: Option<i64>,
    limit: usize,
) -> Result<TemporalMemoryGraphReport> {
    let valid_at = valid_at.unwrap_or_else(now_ms);
    let known_at = known_at.unwrap_or_else(now_ms);
    let mut stmt = conn.prepare(
        "SELECT id, memory_id, target_memory_id, kind, evidence_kind, evidence_ref, confidence, valid_from, valid_to, observed_at \
         FROM memory_observations \
         WHERE target_memory_id IS NOT NULL \
           AND valid_from <= ?1 AND (valid_to IS NULL OR valid_to >= ?1) AND observed_at <= ?2 \
         ORDER BY confidence DESC, observed_at DESC LIMIT ?3",
    )?;
    let mut edges = stmt
        .query_map(
            params![valid_at, known_at, limit.clamp(1, 5_000) as i64],
            |row| {
                let observation_id = row.get::<_, String>(0)?;
                let observation_kind = row.get::<_, String>(3)?;
                let evidence_kind = row.get::<_, String>(4)?;
                let evidence_ref = row.get::<_, String>(5)?;
                Ok(TemporalMemoryGraphEdge {
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    kind: observation_edge_kind(&observation_kind).to_string(),
                    confidence: row.get(6)?,
                    provenance: json!({
                        "observation_id": &observation_id,
                        "evidence_kind": evidence_kind,
                        "evidence_ref": evidence_ref,
                    })
                    .to_string(),
                    valid_from: row.get(7)?,
                    valid_to: row.get(8)?,
                    observed_at: row.get(9)?,
                    observation_id: Some(observation_id),
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let observation_count = edges.len();
    let remaining = limit.clamp(1, 5_000).saturating_sub(edges.len());
    if remaining > 0 {
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, kind, confidence, provenance, valid_from, valid_to, observed_at \
             FROM memory_edges \
             WHERE observation_id IS NULL \
               AND valid_from <= ?1 AND (valid_to IS NULL OR valid_to >= ?1) AND observed_at <= ?2 \
             ORDER BY confidence DESC, observed_at DESC LIMIT ?3",
        )?;
        edges.extend(
            stmt.query_map(params![valid_at, known_at, remaining as i64], |row| {
                Ok(TemporalMemoryGraphEdge {
                    source_id: row.get(0)?,
                    target_id: row.get(1)?,
                    kind: row.get(2)?,
                    confidence: row.get(3)?,
                    provenance: row.get(4)?,
                    valid_from: row.get(5)?,
                    valid_to: row.get(6)?,
                    observed_at: row.get(7)?,
                    observation_id: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    let ids = edges
        .iter()
        .flat_map(|edge| [&edge.source_id, &edge.target_id])
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut nodes = Vec::new();
    for id in ids {
        if let Ok(memory) = get_memory(conn, &id) {
            nodes.push(TemporalMemoryGraphNode {
                id: memory.id,
                title: memory.title,
                status: memory.status,
            });
        }
    }
    Ok(TemporalMemoryGraphReport {
        version: 1,
        valid_at,
        known_at,
        node_count: nodes.len(),
        edge_count: edges.len(),
        observation_count,
        nodes,
        edges,
    })
}

pub(crate) fn print_memory_observations(
    conn: &Connection,
    memory_id: &str,
    valid_at: Option<i64>,
    known_at: Option<i64>,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let observations = list_memory_observations(conn, memory_id, valid_at, known_at, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&observations)?);
    } else {
        for item in observations {
            println!(
                "{} {} valid={}..{} observed={} {}",
                item.id,
                item.kind,
                item.valid_from,
                item.valid_to
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "open".to_string()),
                item.observed_at,
                item.statement
            );
        }
    }
    Ok(())
}

pub(crate) fn print_temporal_memory_graph(
    conn: &Connection,
    valid_at: Option<i64>,
    known_at: Option<i64>,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = temporal_memory_graph_report(conn, valid_at, known_at, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Temporal Memory Graph: nodes={} edges={} observations={} valid_at={} known_at={}",
            report.node_count,
            report.edge_count,
            report.observation_count,
            report.valid_at,
            report.known_at
        );
    }
    Ok(())
}

fn get_memory_observation(conn: &Connection, id: &str) -> Result<MemoryObservation> {
    conn.query_row(
        "SELECT id, memory_id, target_memory_id, kind, statement, evidence_kind, evidence_ref, confidence, valid_from, valid_to, observed_at, branch, commit_hash, worktree_root FROM memory_observations WHERE id = ?1",
        params![id],
        row_to_observation,
    )
    .map_err(Into::into)
}

fn row_to_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryObservation> {
    Ok(MemoryObservation {
        id: row.get(0)?,
        memory_id: row.get(1)?,
        target_memory_id: row.get(2)?,
        kind: row.get(3)?,
        statement: row.get(4)?,
        evidence_kind: row.get(5)?,
        evidence_ref: row.get(6)?,
        confidence: row.get(7)?,
        valid_from: row.get(8)?,
        valid_to: row.get(9)?,
        observed_at: row.get(10)?,
        branch: row.get(11)?,
        commit_hash: row.get(12)?,
        worktree_root: row.get(13)?,
    })
}

fn observation_edge_kind(kind: &str) -> &'static str {
    match kind {
        "contradicted" => "contradicts",
        "superseded" => "supersedes",
        "verified" => "supports",
        _ => "evidence_for",
    }
}

fn git_value(root: &Path, args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_create_queryable_bitemporal_graph_edges() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_db(&dir.path().join(".agent/memory.db")).unwrap();
        let source = test_memory(&conn, MemoryType::Decision, "Source decision");
        let target = test_memory(&conn, MemoryType::Constraint, "Target constraint");
        let observed = record_memory_observation(
            &conn,
            Path::new("."),
            &MemoryObservationRequest {
                memory_id: &source,
                target_memory_id: Some(&target),
                kind: "verified",
                statement: "The constraint supports the decision",
                evidence_kind: "test",
                evidence_ref: "cargo test observations",
                confidence: 0.95,
                valid_from: Some(100),
                valid_to: Some(199),
            },
        )
        .unwrap();
        let known =
            list_memory_observations(&conn, &source, Some(100), Some(observed.observed_at), 10)
                .unwrap();
        assert_eq!(known.len(), 1);
        let before_known =
            temporal_memory_graph_report(&conn, Some(100), Some(observed.observed_at - 1), 10)
                .unwrap();
        assert_eq!(before_known.edge_count, 0);
        let graph =
            temporal_memory_graph_report(&conn, Some(100), Some(observed.observed_at), 10).unwrap();
        assert_eq!(graph.edge_count, 1);
        assert_eq!(graph.observation_count, 1);

        let changed = record_memory_observation(
            &conn,
            Path::new("."),
            &MemoryObservationRequest {
                memory_id: &source,
                target_memory_id: Some(&target),
                kind: "contradicted",
                statement: "Later evidence changed the relationship",
                evidence_kind: "test",
                evidence_ref: "cargo test observations changed",
                confidence: 0.9,
                valid_from: Some(200),
                valid_to: None,
            },
        )
        .unwrap();
        let historical =
            temporal_memory_graph_report(&conn, Some(100), Some(changed.observed_at), 10).unwrap();
        assert_eq!(historical.edge_count, 1);
        assert_eq!(historical.edges[0].kind, "supports");
        assert_eq!(
            historical.edges[0].observation_id.as_deref(),
            Some(observed.id.as_str())
        );
        let current =
            temporal_memory_graph_report(&conn, Some(200), Some(changed.observed_at), 10).unwrap();
        assert_eq!(current.edge_count, 1);
        assert_eq!(current.edges[0].kind, "contradicts");
        assert_eq!(
            current.edges[0].observation_id.as_deref(),
            Some(changed.id.as_str())
        );
    }

    fn test_memory(conn: &Connection, memory_type: MemoryType, title: &str) -> String {
        add_memory(
            conn,
            AddMemory {
                id: None,
                memory_type,
                title: title.to_string(),
                body: format!("Evidence body for {title}"),
                scope: MemoryScope::Project,
                status: MemoryStatus::Active,
                source: Some("observation_test".to_string()),
                supersedes: None,
                confidence: 1.0,
                layer: None,
                links: Vec::new(),
                allow_sensitive: false,
            },
        )
        .unwrap()
    }
}

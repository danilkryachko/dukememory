use super::*;

const EVIDENCE_AUTOPILOT_SOURCE: &str = "evidence_autopilot";
const EVIDENCE_AUTOPILOT_KIND: &str = "memory_reference";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvidenceAutopilotReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) reversible: bool,
    pub(crate) scanned_memories: usize,
    pub(crate) graph_candidates: usize,
    pub(crate) eligible_candidates: usize,
    pub(crate) already_evidenced: usize,
    pub(crate) applied_count: usize,
    pub(crate) rolled_back_count: usize,
    pub(crate) candidates: Vec<EvidenceAutopilotCandidate>,
    pub(crate) rollback_observation_ids: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvidenceAutopilotCandidate {
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) confidence: f64,
    pub(crate) reasons: Vec<String>,
    pub(crate) evidence_ref: String,
    pub(crate) safe_to_apply: bool,
    pub(crate) status: String,
    pub(crate) observation_id: Option<String>,
}

pub(crate) fn print_evidence_autopilot(
    conn: &Connection,
    root: &Path,
    limit: usize,
    apply: bool,
    rollback_observation_ids: &[String],
    json_out: bool,
) -> Result<()> {
    let report = evidence_autopilot_report(conn, root, limit, apply, rollback_observation_ids)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Evidence Autopilot");
    println!("status: {}", report.status);
    println!("eligible: {}", report.eligible_candidates);
    println!("applied: {}", report.applied_count);
    println!("rolled back: {}", report.rolled_back_count);
    println!("reversible: {}", report.reversible);
    Ok(())
}

pub(crate) fn evidence_autopilot_report(
    conn: &Connection,
    root: &Path,
    limit: usize,
    apply: bool,
    rollback_observation_ids: &[String],
) -> Result<EvidenceAutopilotReport> {
    if apply && !rollback_observation_ids.is_empty() {
        bail!("evidence autopilot apply and rollback are mutually exclusive");
    }
    if rollback_observation_ids.len() > 100 {
        bail!("evidence autopilot rollback accepts at most 100 observation ids");
    }
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let rolled_back_count = rollback_evidence_autopilot(conn, rollback_observation_ids)?;
    let graph = memory_graph_links_report(conn, &root, limit.clamp(1, 200), false)?;
    let mut candidates = Vec::new();
    let mut already_evidenced = 0usize;
    for candidate in graph.candidates {
        if !candidate
            .reasons
            .iter()
            .any(|reason| reason == "explicit_memory_id_mention")
        {
            continue;
        }
        let evidence_ref = format!(
            "memory:{}#explicit-id:{}",
            candidate.source_id, candidate.target_id
        );
        let existing = existing_autopilot_observation(
            conn,
            &candidate.source_id,
            &candidate.target_id,
            &evidence_ref,
        )?;
        if existing.is_some() {
            already_evidenced += 1;
        }
        let safe_to_apply = candidate.safe_to_apply && existing.is_none();
        candidates.push(EvidenceAutopilotCandidate {
            source_id: candidate.source_id,
            target_id: candidate.target_id,
            confidence: candidate.confidence,
            reasons: candidate.reasons,
            evidence_ref,
            safe_to_apply,
            status: if existing.is_some() {
                "already_evidenced"
            } else if safe_to_apply {
                "eligible"
            } else {
                "review"
            }
            .to_string(),
            observation_id: existing,
        });
    }
    let eligible_candidates = candidates
        .iter()
        .filter(|candidate| candidate.safe_to_apply)
        .count();
    let mut rollback_observation_ids = Vec::new();
    if apply {
        transactional(conn, "evidence_autopilot", || {
            for candidate in &mut candidates {
                if !candidate.safe_to_apply {
                    continue;
                }
                let statement = format!(
                    "Memory {} explicitly references memory {} by its durable id.",
                    candidate.source_id, candidate.target_id
                );
                let observation = record_memory_observation(
                    conn,
                    &root,
                    &MemoryObservationRequest {
                        memory_id: &candidate.source_id,
                        target_memory_id: Some(&candidate.target_id),
                        kind: "asserted",
                        statement: &statement,
                        evidence_kind: EVIDENCE_AUTOPILOT_KIND,
                        evidence_ref: &candidate.evidence_ref,
                        confidence: candidate.confidence,
                        valid_from: None,
                        valid_to: None,
                    },
                )?;
                let provenance = serde_json::to_string(&json!({
                    "source": EVIDENCE_AUTOPILOT_SOURCE,
                    "observation_id": observation.id,
                    "evidence_ref": candidate.evidence_ref,
                    "reasons": candidate.reasons,
                }))?;
                insert_memory_edge_with_observation(
                    conn,
                    &candidate.source_id,
                    &candidate.target_id,
                    "relates_to",
                    candidate.confidence,
                    &provenance,
                    Some(&observation.id),
                )?;
                candidate.status = "applied".to_string();
                candidate.observation_id = Some(observation.id.clone());
                rollback_observation_ids.push(observation.id);
            }
            log_event(
                conn,
                "evidence_autopilot",
                None,
                &serde_json::to_string(&json!({
                    "applied": rollback_observation_ids.len(),
                    "observation_ids": rollback_observation_ids,
                    "reversible": true,
                }))?,
            )
        })?;
    }
    let applied_count = rollback_observation_ids.len();
    let mut recommendations = Vec::new();
    if eligible_candidates > 0 && !apply {
        recommendations.push(
            "review eligible explicit-id references, then rerun evidence-autopilot --apply --json"
                .to_string(),
        );
    }
    if candidates
        .iter()
        .any(|candidate| candidate.status == "review")
    {
        recommendations.push(
            "keep topic-overlap-only graph candidates in manual review; the autopilot applies explicit durable-id evidence only"
                .to_string(),
        );
    }
    let status = if rolled_back_count > 0 {
        "rolled_back"
    } else if apply && applied_count > 0 {
        "applied"
    } else if eligible_candidates > 0 {
        "review_ready"
    } else {
        "no_safe_actions"
    };
    Ok(EvidenceAutopilotReport {
        version: 1,
        ok: true,
        status: status.to_string(),
        root: root.display().to_string(),
        applied: apply,
        reversible: true,
        scanned_memories: graph.scanned_memories,
        graph_candidates: graph.candidate_count,
        eligible_candidates,
        already_evidenced,
        applied_count,
        rolled_back_count,
        candidates,
        rollback_observation_ids,
        recommendations,
    })
}

fn rollback_evidence_autopilot(conn: &Connection, observation_ids: &[String]) -> Result<usize> {
    let mut observation_ids = observation_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    observation_ids.sort_unstable();
    observation_ids.dedup();
    if observation_ids.is_empty() {
        return Ok(0);
    }
    transactional(conn, "rollback_evidence_autopilot", || {
        for observation_id in &observation_ids {
            let evidence_ref = conn
                .query_row(
                    "SELECT evidence_ref FROM memory_observations WHERE id = ?1 AND evidence_kind = ?2",
                    params![observation_id, EVIDENCE_AUTOPILOT_KIND],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "observation {observation_id} is not an evidence-autopilot observation"
                    )
                })?;
            if !evidence_ref.starts_with("memory:") || !evidence_ref.contains("#explicit-id:") {
                bail!("observation {observation_id} has an invalid evidence-autopilot reference");
            }
        }
        for observation_id in &observation_ids {
            conn.execute(
                "DELETE FROM memory_edges WHERE observation_id = ?1",
                params![observation_id],
            )?;
            conn.execute(
                "DELETE FROM memory_observations WHERE id = ?1",
                params![observation_id],
            )?;
        }
        log_event(
            conn,
            "evidence_autopilot_rollback",
            None,
            &serde_json::to_string(&json!({
                "rolled_back": observation_ids.len(),
                "observation_ids": observation_ids,
            }))?,
        )?;
        Ok(observation_ids.len())
    })
}

fn existing_autopilot_observation(
    conn: &Connection,
    source_id: &str,
    target_id: &str,
    evidence_ref: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT id FROM memory_observations WHERE memory_id = ?1 AND target_memory_id = ?2 AND evidence_kind = ?3 AND evidence_ref = ?4 ORDER BY observed_at DESC LIMIT 1",
        params![source_id, target_id, EVIDENCE_AUTOPILOT_KIND, evidence_ref],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn explicit_id_evidence_is_dry_run_first_atomic_and_idempotent() {
        let dir = tempdir().unwrap();
        let conn = open_db(&dir.path().join("memory.db")).unwrap();
        let now = now_ms();
        for (id, body) in [
            (
                "source000001",
                "This decision references target000002 explicitly.",
            ),
            ("target000002", "Independent target memory."),
        ] {
            conn.execute(
                "INSERT INTO memories (id,type,scope,title,body,status,source,created_at,updated_at,confidence) VALUES (?1,'design_note','project',?1,?2,'active','test',?3,?3,1.0)",
                params![id, body, now],
            )
            .unwrap();
        }

        let preview = evidence_autopilot_report(&conn, dir.path(), 20, false, &[]).unwrap();
        assert_eq!(preview.status, "review_ready");
        assert_eq!(preview.eligible_candidates, 1);
        assert_eq!(preview.applied_count, 0);
        let stored: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, 0);

        let applied = evidence_autopilot_report(&conn, dir.path(), 20, true, &[]).unwrap();
        assert_eq!(applied.status, "applied");
        assert_eq!(applied.applied_count, 1);
        assert_eq!(applied.rollback_observation_ids.len(), 1);
        let observation_backed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_edges WHERE observation_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(observation_backed >= 1);

        let repeated = evidence_autopilot_report(&conn, dir.path(), 20, true, &[]).unwrap();
        assert_eq!(repeated.applied_count, 0);
        assert_eq!(repeated.already_evidenced, 1);

        let rollback_ids = applied.rollback_observation_ids.clone();
        let rolled_back =
            evidence_autopilot_report(&conn, dir.path(), 20, false, &rollback_ids).unwrap();
        assert_eq!(rolled_back.status, "rolled_back");
        assert_eq!(rolled_back.rolled_back_count, 1);
        let observation_backed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_edges WHERE observation_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(observation_backed, 0);
        let remaining_observations: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining_observations, 0);

        let reapplied = evidence_autopilot_report(&conn, dir.path(), 20, true, &[]).unwrap();
        assert_eq!(reapplied.applied_count, 1);
    }
}

use super::*;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const CONTROL_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const CONTROL_SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(3);
const CONTROL_SNAPSHOT_CACHE_LIMIT: usize = 16;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSnapshot {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) generated_at: i64,
    pub(crate) revision: String,
    pub(crate) cache: ControlSnapshotCacheInfo,
    pub(crate) control: ControlSnapshotHeader,
    pub(crate) current_version: String,
    pub(crate) agent_sessions: Vec<ControlSessionItem>,
    pub(crate) runner_profiles: Vec<ControlRunnerProfileItem>,
    pub(crate) summary: ControlSignalSummary,
    pub(crate) panels: Vec<ControlSnapshotPanel>,
    pub(crate) recommendations: Vec<String>,
    pub(crate) request_budget: ControlRequestBudget,
    pub(crate) compatibility: ControlCompatibility,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSnapshotHeader {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) panels: Vec<ControlSnapshotPanel>,
    pub(crate) controls: Vec<String>,
    pub(crate) recommendations: Vec<String>,
    pub(crate) details_endpoint: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSnapshotPanel {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) headline: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSignalSummary {
    pub(crate) health: ControlHealthSignal,
    pub(crate) quality: ControlQualitySignal,
    pub(crate) recall: ControlRecallSignal,
    pub(crate) rag: ControlRagSignal,
    pub(crate) diff_impact: ControlDiffImpactSignal,
    pub(crate) autonomy: ControlAutonomySignal,
    pub(crate) sessions: ControlSessionSignal,
    pub(crate) profiles: ControlProfileSignal,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlQualitySignal {
    pub(crate) version: u32,
    pub(crate) total: usize,
    pub(crate) average_score: f64,
    pub(crate) actionable_count: usize,
    pub(crate) classifications: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSessionItem {
    pub(crate) id: String,
    pub(crate) task: String,
    pub(crate) status: String,
    pub(crate) attempt_state: String,
    pub(crate) lease_owner: Option<String>,
    pub(crate) lease_expires_at: Option<i64>,
    pub(crate) attempt_count: i64,
    pub(crate) last_event_sequence: i64,
    pub(crate) last_heartbeat_at: Option<i64>,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlRunnerProfileItem {
    pub(crate) name: String,
    pub(crate) runner: String,
    pub(crate) model: Option<String>,
    pub(crate) role: String,
    pub(crate) available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlHealthSignal {
    pub(crate) score: f64,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlRecallSignal {
    pub(crate) score: f64,
    pub(crate) ok: bool,
    pub(crate) regression: bool,
    pub(crate) baseline_score: Option<f64>,
    pub(crate) baseline_compatible: bool,
    pub(crate) baseline_stale: bool,
    pub(crate) probe_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlRagSignal {
    pub(crate) status: String,
    pub(crate) recall: f64,
    pub(crate) grounded_coverage: f64,
    pub(crate) candidate_recall: f64,
    pub(crate) near_miss_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlDiffImpactSignal {
    pub(crate) severity: String,
    pub(crate) changed_files: usize,
    pub(crate) linked_memory_count: usize,
    pub(crate) unlinked_changed_files: usize,
    pub(crate) write_ready_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlAutonomySignal {
    pub(crate) local_ready: bool,
    pub(crate) optional_sync_ready: bool,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSessionSignal {
    pub(crate) active: usize,
    pub(crate) leased: usize,
    pub(crate) stale: usize,
    pub(crate) recent: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlProfileSignal {
    pub(crate) ready: usize,
    pub(crate) configured: usize,
    pub(crate) optional: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlSnapshotCacheInfo {
    pub(crate) hit: bool,
    pub(crate) age_ms: u128,
    pub(crate) ttl_ms: u128,
    pub(crate) compute_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlRequestBudget {
    pub(crate) initial_requests: usize,
    pub(crate) details: String,
    pub(crate) legacy_fanout: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ControlCompatibility {
    pub(crate) canonical_cli: String,
    pub(crate) canonical_endpoint: String,
    pub(crate) details_endpoint: String,
    pub(crate) latest_legacy_alias: String,
    pub(crate) deprecated_aliases: Vec<String>,
}

#[derive(Clone)]
struct CachedControlSnapshot {
    created_at: Instant,
    snapshot: ControlSnapshot,
}

static CONTROL_SNAPSHOT_CACHE: OnceLock<Mutex<BTreeMap<String, CachedControlSnapshot>>> =
    OnceLock::new();

pub(crate) fn print_control_snapshot(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = control_snapshot_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("DukeMemory Control Snapshot");
        println!("status: {}", report.status);
        println!("revision: {}", report.revision);
        for panel in &report.panels {
            println!("{}: {}", panel.name, panel.headline);
        }
    }
    Ok(())
}

pub(crate) fn control_snapshot_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<ControlSnapshot> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let revision = control_snapshot_revision(conn, &root)?;
    let key = format!(
        "{}|{}|{}|{}",
        db.canonicalize()
            .unwrap_or_else(|_| db.to_path_buf())
            .display(),
        root.display(),
        since_days,
        revision
    );
    let cache = CONTROL_SNAPSHOT_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("control snapshot cache lock poisoned"))?;
        cache.retain(|_, entry| entry.created_at.elapsed() <= Duration::from_secs(60));
        if let Some(entry) = cache.get(&key)
            && entry.created_at.elapsed() <= CONTROL_SNAPSHOT_CACHE_TTL
        {
            let mut snapshot = entry.snapshot.clone();
            snapshot.cache.hit = true;
            snapshot.cache.age_ms = entry.created_at.elapsed().as_millis();
            return Ok(snapshot);
        }
    }

    let started = Instant::now();
    let sessions = list_agent_sessions(conn, 8)?;
    let profiles = runner_profiles_status(&root)?;
    let quality = quality_report(conn, 30, 20)?;
    let recall = recall_benchmark_suite_report(conn, &root, since_days, 8, false)?;
    let autonomy = autonomy_control_center_report(conn, db, &root, since_days)?;
    let rag_signal = control_rag_signal(conn)?;
    let diff_impact = autonomy.diff_review.impact.clone();

    let active_sessions = sessions
        .iter()
        .filter(|session| session.status == "active")
        .count();
    let leased_sessions = sessions
        .iter()
        .filter(|session| session.attempt_state == "leased")
        .count();
    let stale_sessions = sessions
        .iter()
        .filter(|session| session.attempt_state == "stale")
        .count();
    let ready_profiles = profiles.iter().filter(|profile| profile.available).count();
    let memory_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    let pending_inbox: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;

    let recall_ok =
        recall.ok && !recall.regression && recall.baseline_compatible && !recall.baseline_stale;
    let ok = autonomy.local_ready && recall_ok && quality.actionable_count == 0;
    let status = if ok { "ready" } else { "attention" }.to_string();
    let panels = vec![
        ControlSnapshotPanel {
            name: "health".to_string(),
            status: if autonomy.qa.ok { "ready" } else { "attention" }.to_string(),
            headline: format!("score {:.1}", autonomy.qa.score),
        },
        ControlSnapshotPanel {
            name: "quality".to_string(),
            status: if quality.actionable_count == 0 {
                "ready"
            } else {
                "attention"
            }
            .to_string(),
            headline: format!(
                "average {:.1} / {} actionable",
                quality.average_score, quality.actionable_count
            ),
        },
        ControlSnapshotPanel {
            name: "recall".to_string(),
            status: if recall_ok { "ready" } else { "attention" }.to_string(),
            headline: format!(
                "score {:.1} / regression {}",
                recall.score, recall.regression
            ),
        },
        ControlSnapshotPanel {
            name: "rag_eval".to_string(),
            status: match rag_signal.status.as_str() {
                "ready" => "ready",
                "unconfigured" => "optional",
                _ => "attention",
            }
            .to_string(),
            headline: format!(
                "recall {:.1}% / grounded {:.1}% / near_misses {}",
                rag_signal.recall, rag_signal.grounded_coverage, rag_signal.near_miss_count
            ),
        },
        ControlSnapshotPanel {
            name: "diff_impact".to_string(),
            status: if diff_impact.severity == "high" {
                "attention"
            } else {
                "ready"
            }
            .to_string(),
            headline: format!(
                "{} / {} changed / {} write-ready",
                diff_impact.severity, diff_impact.changed_files, diff_impact.write_ready_count
            ),
        },
        ControlSnapshotPanel {
            name: "autonomy".to_string(),
            status: autonomy.status.clone(),
            headline: format!(
                "local {} / optional sync {}",
                readiness(autonomy.local_ready),
                readiness(autonomy.optional_sync_ready)
            ),
        },
        ControlSnapshotPanel {
            name: "agent_sessions".to_string(),
            status: if stale_sessions == 0 {
                "ready"
            } else {
                "attention"
            }
            .to_string(),
            headline: format!(
                "{} active / {} leased / {} stale / {} recent",
                active_sessions,
                leased_sessions,
                stale_sessions,
                sessions.len()
            ),
        },
        ControlSnapshotPanel {
            name: "runner_profiles".to_string(),
            status: if ready_profiles > 0 {
                "ready"
            } else {
                "optional"
            }
            .to_string(),
            headline: format!(
                "{} ready / {} configured (optional)",
                ready_profiles,
                profiles.len()
            ),
        },
        ControlSnapshotPanel {
            name: "project_memory".to_string(),
            status: "ready".to_string(),
            headline: format!("{} memories / {} pending", memory_count, pending_inbox),
        },
    ];
    let details_endpoint = "/web-control-center?view=details".to_string();
    let mut recommendations = recall.recommendations.clone();
    recommendations.extend(autonomy.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let compact_sessions = sessions
        .iter()
        .map(|session| ControlSessionItem {
            id: session.id.clone(),
            task: session.task.clone(),
            status: session.status.clone(),
            attempt_state: session.attempt_state.clone(),
            lease_owner: session.lease_owner.clone(),
            lease_expires_at: session.lease_expires_at,
            attempt_count: session.attempt_count,
            last_event_sequence: session.last_event_sequence,
            last_heartbeat_at: session.last_heartbeat_at,
            updated_at: session.updated_at,
        })
        .collect::<Vec<_>>();
    let compact_profiles = profiles
        .iter()
        .map(|profile| ControlRunnerProfileItem {
            name: profile.name.clone(),
            runner: profile.profile.runner.clone(),
            model: profile.profile.model.clone(),
            role: profile.profile.role.clone(),
            available: profile.available,
        })
        .collect::<Vec<_>>();
    let summary = ControlSignalSummary {
        health: ControlHealthSignal {
            score: autonomy.qa.score,
            status: if autonomy.qa.ok { "ready" } else { "attention" }.to_string(),
        },
        quality: ControlQualitySignal {
            version: quality.version,
            total: quality.total,
            average_score: quality.average_score,
            actionable_count: quality.actionable_count,
            classifications: quality.classifications,
        },
        recall: ControlRecallSignal {
            score: recall.score,
            ok: recall.ok,
            regression: recall.regression,
            baseline_score: recall.baseline_score,
            baseline_compatible: recall.baseline_compatible,
            baseline_stale: recall.baseline_stale,
            probe_count: recall.current_probe_ids.len(),
        },
        rag: rag_signal,
        diff_impact: ControlDiffImpactSignal {
            severity: diff_impact.severity,
            changed_files: diff_impact.changed_files,
            linked_memory_count: diff_impact.affected_memory_ids.len(),
            unlinked_changed_files: diff_impact.unlinked_changed_files.len(),
            write_ready_count: diff_impact.write_ready_count,
        },
        autonomy: ControlAutonomySignal {
            local_ready: autonomy.local_ready,
            optional_sync_ready: autonomy.optional_sync_ready,
            status: autonomy.status,
        },
        sessions: ControlSessionSignal {
            active: active_sessions,
            leased: leased_sessions,
            stale: stale_sessions,
            recent: sessions.len(),
        },
        profiles: ControlProfileSignal {
            ready: ready_profiles,
            configured: profiles.len(),
            optional: true,
        },
    };
    let control = ControlSnapshotHeader {
        version: CONTROL_SNAPSHOT_SCHEMA_VERSION,
        ok,
        status: status.clone(),
        root: root.display().to_string(),
        panels: panels.clone(),
        controls: Vec::new(),
        recommendations: recommendations.clone(),
        details_endpoint: details_endpoint.clone(),
    };
    let snapshot = ControlSnapshot {
        version: CONTROL_SNAPSHOT_SCHEMA_VERSION,
        ok,
        status,
        root: root.display().to_string(),
        generated_at: now_ms(),
        revision,
        cache: ControlSnapshotCacheInfo {
            hit: false,
            age_ms: 0,
            ttl_ms: CONTROL_SNAPSHOT_CACHE_TTL.as_millis(),
            compute_ms: started.elapsed().as_millis(),
        },
        control,
        current_version: "stable-v1".to_string(),
        agent_sessions: compact_sessions,
        runner_profiles: compact_profiles,
        summary,
        panels,
        recommendations,
        request_budget: ControlRequestBudget {
            initial_requests: 1,
            details: "single stable request".to_string(),
            legacy_fanout: false,
        },
        compatibility: ControlCompatibility {
            canonical_cli: "dukememory web-control-center --json".to_string(),
            canonical_endpoint: "/web-control-center".to_string(),
            details_endpoint,
            latest_legacy_alias: "/web-control-center-v12".to_string(),
            deprecated_aliases: (3..=12)
                .map(|version| format!("/web-control-center-v{version}"))
                .collect(),
        },
    };

    let mut cache = cache
        .lock()
        .map_err(|_| anyhow::anyhow!("control snapshot cache lock poisoned"))?;
    cache.insert(
        key,
        CachedControlSnapshot {
            created_at: Instant::now(),
            snapshot: snapshot.clone(),
        },
    );
    while cache.len() > CONTROL_SNAPSHOT_CACHE_LIMIT {
        let oldest = cache
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(key, _)| key.clone());
        if let Some(oldest) = oldest {
            cache.remove(&oldest);
        } else {
            break;
        }
    }
    Ok(snapshot)
}

fn readiness(value: bool) -> &'static str {
    if value { "ready" } else { "attention" }
}

fn control_rag_signal(conn: &Connection) -> Result<ControlRagSignal> {
    let stored_cases: i64 =
        conn.query_row("SELECT COUNT(*) FROM eval_cases", [], |row| row.get(0))?;
    if stored_cases == 0 {
        return Ok(ControlRagSignal {
            status: "unconfigured".to_string(),
            recall: 0.0,
            grounded_coverage: 0.0,
            candidate_recall: 0.0,
            near_miss_count: 0,
        });
    }
    let report = rag_eval_report(
        conn,
        None,
        8,
        3_000,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )?;
    Ok(ControlRagSignal {
        status: report.status,
        recall: report.recall,
        grounded_coverage: report.grounded_answers.coverage,
        candidate_recall: report.evidence_placement.candidate_recall,
        near_miss_count: report.evidence_placement.near_miss_count,
    })
}

fn control_snapshot_revision(conn: &Connection, root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    for (table, updated_column) in [
        ("memories", Some("updated_at")),
        ("memory_links", None),
        ("memory_inbox", Some("updated_at")),
        ("memory_events", Some("created_at")),
        ("memory_read_events", Some("created_at")),
        ("agent_sessions", Some("updated_at")),
        ("agent_session_events", Some("created_at")),
    ] {
        let timestamp = updated_column.unwrap_or("rowid");
        let sql = format!(
            "SELECT COUNT(*), COALESCE(MAX(rowid), 0), COALESCE(MAX({timestamp}), 0) FROM {table}"
        );
        let revision: (i64, i64, i64) =
            conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        hasher.update(format!("{table}:{revision:?};").as_bytes());
    }
    for relative in [
        ".agent/config.toml",
        ".agent/runner-profiles.toml",
        ".agent/autonomous-status.json",
        ".agent/memory-governance.json",
    ] {
        let path = root.join(relative);
        if let Ok(metadata) = fs::metadata(&path) {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            hasher.update(format!("{relative}:{}:{modified};", metadata.len()).as_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize())[..16].to_string())
}

use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct ReleaseGateV3Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) strict: bool,
    pub(crate) run: bool,
    pub(crate) rag_profile: ReleaseGateRagProfileReport,
    pub(crate) release_gate_v2: ReleaseGateV2Report,
    pub(crate) effectiveness_v2: MemoryEffectivenessV2Report,
    pub(crate) baselines: RecallBenchmarkBaselinesReport,
    pub(crate) conflict_apply: MemoryConflictApplyReport,
    pub(crate) mcp_surface_v3: McpToolSurfaceV3Report,
    pub(crate) mcp_discipline_v3: McpDisciplineV3Report,
    pub(crate) fleet_quality: FleetQualityReport,
    pub(crate) storage: OpsStorageStatus,
    pub(crate) rag_eval: RagEvalReport,
    pub(crate) graph_rag_eval: GraphRagEvalReport,
    pub(crate) advanced_eval: AdvancedEvalReport,
    pub(crate) checks: Vec<ReleaseGateCheck>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ReleaseRagProfile {
    Deployment,
    Canonical,
    Offline,
}

impl ReleaseRagProfile {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Deployment => "deployment",
            Self::Canonical => "canonical",
            Self::Offline => "offline",
        }
    }

    pub(crate) fn parse(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("deployment") {
            "deployment" => Ok(Self::Deployment),
            "canonical" => Ok(Self::Canonical),
            "offline" => Ok(Self::Offline),
            other => bail!(
                "unsupported RAG release profile `{other}`; expected deployment, canonical, or offline"
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleaseGateRagProfileReport {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) limit: usize,
    pub(crate) budget: usize,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_release_gate_v3(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
    rag_profile: ReleaseRagProfile,
    json_out: bool,
) -> Result<()> {
    let report =
        release_gate_v3_report_with_profile(conn, db, root, since_days, strict, run, rag_profile)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Release Gate v3");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn release_gate_v3_report_with_profile(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
    rag_profile: ReleaseRagProfile,
) -> Result<ReleaseGateV3Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let rag_profile = release_gate_rag_profile(&root, rag_profile);
    let release_gate_v2 = release_gate_v2_report(conn, db, &root, since_days, strict, run)?;
    let effectiveness_v2 = memory_effectiveness_v2_report(conn, &root, since_days)?;
    let baselines = recall_benchmark_baselines_report(conn, &root, since_days, false)?;
    let conflict_apply = memory_conflict_apply_report(conn, 90, 12, false)?;
    let mcp_surface_v3 = mcp_tool_surface_v3_report();
    let mcp_discipline_v3 = mcp_discipline_v3_report(conn, db, &root, since_days, false)?;
    let fleet_quality = fleet_quality_report(db, since_days)?;
    let storage = ops_storage_status(conn, db, &root)?;
    let rag_sources = crate::app::rag_ingest::rag_sources_report(
        conn,
        &root,
        &rag_profile.provider,
        &rag_profile.endpoint,
        &rag_profile.model,
    )?;
    let rag_eval = rag_eval_report_with_baseline(
        conn,
        None,
        rag_profile.limit,
        rag_profile.budget,
        &rag_profile.provider,
        &rag_profile.endpoint,
        &rag_profile.model,
        Some(&root),
        false,
    )?;
    let graph_generation = crate::runtime_config::GenerationConfig {
        provider: "mock".to_string(),
        endpoint: "local".to_string(),
        model: "extractive-fallback".to_string(),
    };
    let graph_rag_eval = graph_rag_eval_report(
        conn,
        None,
        rag_profile.limit,
        rag_profile.budget,
        &graph_generation,
        &rag_profile.provider,
        &rag_profile.endpoint,
        &rag_profile.model,
    )?;
    let advanced_eval = advanced_eval_report(conn)?;
    let mut checks = release_gate_v2.checks.clone();
    checks.push(ReleaseGateCheck {
        name: "memory_effectiveness_v2".to_string(),
        ok: effectiveness_v2.ok && effectiveness_v2.score >= 75.0,
        required: true,
        detail: format!(
            "score={:.1} confidence={}",
            effectiveness_v2.score, effectiveness_v2.confidence
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "recall_benchmark_baselines".to_string(),
        ok: baselines.ok && !baselines.regression && baselines.current_score >= 80.0,
        required: true,
        detail: format!(
            "current={:.1} baseline_present={} regression={}",
            baselines.current_score, baselines.baseline_present, baselines.regression
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "memory_conflict_apply_dry_run".to_string(),
        ok: conflict_apply.status != "manual_review",
        required: true,
        detail: format!(
            "status={} safe_actions={} skipped={}",
            conflict_apply.status,
            conflict_apply.safe_actions.len(),
            conflict_apply.skipped.len()
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "mcp_tool_surface_v3".to_string(),
        ok: mcp_surface_v3.ok,
        required: true,
        detail: format!("missing={}", mcp_surface_v3.missing_tools.len()),
    });
    checks.push(ReleaseGateCheck {
        name: "mcp_discipline_v3".to_string(),
        ok: mcp_discipline_v3.ok,
        required: true,
        detail: format!("missing={}", mcp_discipline_v3.missing_commands.len()),
    });
    checks.push(ReleaseGateCheck {
        name: "fleet_quality_observed".to_string(),
        ok: fleet_quality.ready_projects > 0,
        required: false,
        detail: format!(
            "ready={} attention={} avg_effectiveness={:.1}",
            fleet_quality.ready_projects,
            fleet_quality.attention_projects,
            fleet_quality.average_effectiveness_score
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "ops_storage".to_string(),
        ok: storage.pressure == "ok" && storage.retention_ready,
        required: true,
        detail: format!(
            "pressure={} retention_ready={} over_quota={} install_backups={}/{}",
            storage.pressure,
            storage.retention_ready,
            storage.over_quota.join(","),
            storage.install_backups_bytes,
            storage.install_backups_quota_bytes
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "rag_sources_freshness".to_string(),
        ok: rag_sources.ok,
        required: true,
        detail: format!(
            "profile={} provider={} model={} ready={}/{} stale={} missing={} orphan={} chunks={} embedding_missing={} embedding_stale={}",
            rag_profile.name,
            rag_profile.provider,
            rag_profile.model,
            rag_sources.ready_sources,
            rag_sources.total_sources,
            rag_sources.stale_sources,
            rag_sources.missing_sources,
            rag_sources.orphan_sources,
            rag_sources.total_chunks,
            rag_sources.chunk_embeddings_missing,
            rag_sources.chunk_embeddings_stale
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "rag_source_pack_eval".to_string(),
        ok: rag_eval.ok
            && rag_eval.recall >= 80.0
            && rag_eval.ranking.hit_at_3_rate >= 50.0
            && rag_eval.split.holdout_ready,
        required: true,
        detail: format!(
            "recall={:.1}% passed={}/{} source={} semantic_fallbacks={} grounded={:.1}% grounded_passed={}/{} hit_at_3={:.1}% mrr={:.1}% packing_selected={}/{} packing_chunks={}/{} suppressed_overlap={} suppressed_file_cap={} suppressed_limit={} expected_selected={} expected_suppressed={} expected_missing={} evidence_selection={:.1}% evidence_candidate={:.1}% near_misses={} matrix={} matrix_coverage={:.1}% matrix_missing={} retrieval_profile={} retrieval_tuning={} holdout={}/{} holdout_recall={:.1}% holdout_grounded={:.1}% holdout_ready={}",
            rag_eval.recall,
            rag_eval.passed,
            rag_eval.total,
            rag_eval.case_source,
            rag_eval.semantic_fallbacks,
            rag_eval.grounded_answers.coverage,
            rag_eval.grounded_answers.passed,
            rag_eval.total,
            rag_eval.ranking.hit_at_3_rate,
            rag_eval.ranking.mean_reciprocal_rank,
            rag_eval.packing.selected_count,
            rag_eval.packing.candidate_count,
            rag_eval.packing.selected_chunks,
            rag_eval.packing.chunk_candidates,
            rag_eval.packing.suppressed_overlap,
            rag_eval.packing.suppressed_file_cap,
            rag_eval.packing.suppressed_limit,
            rag_eval.packing.expected_selected,
            rag_eval.packing.expected_suppressed_by_packing,
            rag_eval.packing.expected_missing_from_candidates,
            rag_eval.evidence_placement.selection_recall,
            rag_eval.evidence_placement.candidate_recall,
            rag_eval.evidence_placement.near_miss_count,
            rag_eval.eval_matrix.status,
            rag_eval.eval_matrix.coverage,
            rag_eval.eval_matrix.missing_dimensions.len(),
            rag_eval.retrieval_tuning.selected_profile,
            rag_eval.retrieval_tuning.status,
            rag_eval.split.holdout_passed,
            rag_eval.split.holdout_total,
            rag_eval.split.holdout_recall,
            rag_eval.split.holdout_grounded_coverage,
            rag_eval.split.holdout_ready
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "rag_eval_baseline".to_string(),
        ok: !rag_eval_baseline_blocks_release(&rag_eval.baseline.status),
        required: true,
        detail: format!(
            "status={} present={} regression={} signature={} baseline={}",
            rag_eval.baseline.status,
            rag_eval.baseline.present,
            rag_eval.baseline.regression,
            rag_eval.baseline.current_signature,
            rag_eval
                .baseline
                .baseline_signature
                .as_deref()
                .unwrap_or("-")
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "graph_rag_eval".to_string(),
        ok: graph_rag_eval.ok || graph_rag_eval.total == 0,
        required: true,
        detail: format!(
            "status={} recall={:.1}% grounded={:.1}% passed={}/{} edges={} relationship_coverage={:.1}%",
            graph_rag_eval.status,
            graph_rag_eval.recall,
            graph_rag_eval.grounded_coverage,
            graph_rag_eval.passed,
            graph_rag_eval.total,
            graph_rag_eval.graph.total_edges,
            graph_rag_eval.graph.average_relationship_coverage
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "advanced_eval_integrity".to_string(),
        ok: advanced_eval.ok,
        required: true,
        detail: format!(
            "status={} causal_cycles={} poison_candidates={} temporal_invalid={} temporal_future={}",
            advanced_eval.status,
            advanced_eval.causal.cycle_nodes,
            advanced_eval.poisoning.prompt_injection_candidates,
            advanced_eval.temporal.invalid_intervals,
            advanced_eval.temporal.future_knowledge_events
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "advanced_eval_poisoning_review".to_string(),
        ok: advanced_eval.poisoning.status != "attention",
        required: false,
        detail: format!(
            "status={} risk={:.1} candidates={} duplicate_groups={}",
            advanced_eval.poisoning.status,
            advanced_eval.poisoning.risk_score,
            advanced_eval.poisoning.prompt_injection_candidates,
            advanced_eval.poisoning.duplicate_cross_source_groups
        ),
    });
    let mut issues = release_gate_v2.issues.clone();
    for check in &checks {
        if check.required && !check.ok {
            issues.push(format!("release gate v3 failed: {}", check.name));
        }
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = release_gate_v2.recommendations.clone();
    recommendations.extend(effectiveness_v2.recommendations.clone());
    recommendations.extend(baselines.recommendations.clone());
    recommendations.extend(conflict_apply.recommendations.clone());
    recommendations.extend(mcp_surface_v3.recommendations.clone());
    recommendations.extend(mcp_discipline_v3.recommendations.clone());
    recommendations.extend(fleet_quality.recommendations.clone());
    if storage.pressure != "ok" || !storage.retention_ready {
        recommendations.push(
            "restore local storage below byte and retention quotas before release".to_string(),
        );
    }
    recommendations.extend(rag_sources.recommendations.clone());
    recommendations.extend(graph_rag_eval.recommendations.clone());
    recommendations.extend(advanced_eval.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(ReleaseGateV3Report {
        version: 2,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        strict,
        run,
        rag_profile,
        release_gate_v2,
        effectiveness_v2,
        baselines,
        conflict_apply,
        mcp_surface_v3,
        mcp_discipline_v3,
        fleet_quality,
        storage,
        rag_eval,
        graph_rag_eval,
        advanced_eval,
        checks,
        issues,
        recommendations,
    })
}

fn release_gate_rag_profile(
    root: &Path,
    profile: ReleaseRagProfile,
) -> ReleaseGateRagProfileReport {
    match profile {
        ReleaseRagProfile::Deployment => {
            let (mut provider, mut endpoint, mut model) = read_project_embedding_config(root);
            if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_PROVIDER") {
                provider = value;
            }
            if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_ENDPOINT") {
                endpoint = value;
            }
            if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_MODEL") {
                model = value;
            }
            let (limit, budget) = read_project_rag_budget(root);
            let source = if [
                "DUKEMEMORY_EMBED_PROVIDER",
                "DUKEMEMORY_EMBED_ENDPOINT",
                "DUKEMEMORY_EMBED_MODEL",
            ]
            .iter()
            .any(|name| std::env::var_os(name).is_some())
            {
                "environment"
            } else if root.join(".agent/config.toml").is_file() {
                "project_config"
            } else {
                "defaults"
            };
            ReleaseGateRagProfileReport {
                name: profile.as_str().to_string(),
                source: source.to_string(),
                provider,
                endpoint,
                model,
                limit,
                budget,
            }
        }
        ReleaseRagProfile::Canonical => ReleaseGateRagProfileReport {
            name: profile.as_str().to_string(),
            source: "built_in".to_string(),
            provider: DEFAULT_EMBED_PROVIDER.to_string(),
            endpoint: DEFAULT_EMBED_ENDPOINT.to_string(),
            model: DEFAULT_EMBED_MODEL.to_string(),
            limit: 8,
            budget: 3_000,
        },
        ReleaseRagProfile::Offline => ReleaseGateRagProfileReport {
            name: profile.as_str().to_string(),
            source: "built_in".to_string(),
            provider: "mock".to_string(),
            endpoint: "local".to_string(),
            model: "mock-small".to_string(),
            limit: 8,
            budget: 3_000,
        },
    }
}

fn read_project_rag_budget(root: &Path) -> (usize, usize) {
    let defaults = (12, 4_000);
    let Ok(raw) = fs::read_to_string(root.join(".agent/config.toml")) else {
        return defaults;
    };
    let Ok(value) = raw.parse::<toml::Value>() else {
        return defaults;
    };
    (
        value
            .get("default_context_limit")
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(defaults.0),
        value
            .get("default_context_max_chars")
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(defaults.1),
    )
}

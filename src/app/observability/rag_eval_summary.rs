use super::*;

const WEB_RAG_EVAL_MATRIX_DIMENSIONS: [&str; 9] = [
    "source_chunk",
    "memory_card",
    "cli_workflow",
    "mcp_tooling",
    "http_api",
    "graph_memory",
    "multilingual",
    "negative_or_missing",
    "packing_near_miss",
];

fn web_ratio_percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        ((part as f64 / total as f64) * 1000.0).round() / 10.0
    }
}

pub(super) fn web_rag_eval_quick_summary(
    conn: &Connection,
    root: &Path,
) -> Result<WebRagEvalQuickSummary> {
    let mut stmt =
        conn.prepare("SELECT name, query, expected FROM eval_cases ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut total = 0usize;
    let mut dimensions = WEB_RAG_EVAL_MATRIX_DIMENSIONS
        .iter()
        .map(|dimension| (dimension.to_string(), 0usize))
        .collect::<std::collections::BTreeMap<_, _>>();
    for row in rows {
        let (name, query, expected) = row?;
        total += 1;
        for dimension in web_rag_eval_case_dimensions(&format!("{name} {query} {expected}")) {
            *dimensions.entry(dimension.to_string()).or_insert(0) += 1;
        }
    }
    let missing_dimensions = WEB_RAG_EVAL_MATRIX_DIMENSIONS
        .iter()
        .filter(|dimension| dimensions.get(**dimension).copied().unwrap_or_default() == 0)
        .map(|dimension| dimension.to_string())
        .collect::<Vec<_>>();
    let total_dimensions = WEB_RAG_EVAL_MATRIX_DIMENSIONS.len();
    let covered_dimensions = total_dimensions.saturating_sub(missing_dimensions.len());
    let coverage = web_ratio_percent(covered_dimensions, total_dimensions);
    let baseline = web_rag_eval_baseline_quick_summary(root)?;
    let baseline_value = fs::read_to_string(root.join(".agent/rag-eval-baseline.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let baseline_total = baseline_value
        .as_ref()
        .and_then(|value| value.get("total"))
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let baseline_passed = baseline_value
        .as_ref()
        .and_then(|value| value.get("passed"))
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let grounded_coverage = baseline_value
        .as_ref()
        .and_then(|value| value.get("grounded_coverage"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let candidate_recall = baseline_value
        .as_ref()
        .and_then(|value| value.get("candidate_recall"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let selection_recall = baseline_value
        .as_ref()
        .and_then(|value| value.get("selection_recall"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let holdout_total = baseline_value
        .as_ref()
        .and_then(|value| value.get("holdout_total"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let holdout_recall = baseline_value
        .as_ref()
        .and_then(|value| value.get("holdout_recall"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let holdout_grounded_coverage = baseline_value
        .as_ref()
        .and_then(|value| value.get("holdout_grounded_coverage"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let holdout_passed = ((holdout_recall / 100.0) * holdout_total as f64).round() as usize;
    let holdout_ready =
        holdout_total >= 5 && holdout_recall >= 99.9 && holdout_grounded_coverage >= 99.9;
    let total = baseline_total.unwrap_or(total);
    let passed = baseline_passed.unwrap_or(0);
    let failed = total.saturating_sub(passed);
    let grounded_passed = ((grounded_coverage / 100.0) * total as f64).round() as usize;
    let matrix_status = if total == 0 {
        "empty"
    } else if missing_dimensions.is_empty() {
        "ready"
    } else {
        "partial"
    }
    .to_string();
    let selected_profile = current_ranking_profile(root).unwrap_or_else(|| "balanced".to_string());
    let retrieval_status =
        if baseline.present && candidate_recall >= 90.0 && selection_recall >= 90.0 {
            "ready"
        } else if baseline.present {
            "attention"
        } else {
            "unconfigured"
        }
        .to_string();
    let ok = total > 0
        && failed == 0
        && grounded_coverage >= 99.9
        && missing_dimensions.is_empty()
        && holdout_ready
        && !rag_eval_baseline_blocks_release(&baseline.status);
    let status = if ok {
        "ready"
    } else if total == 0 {
        "empty"
    } else {
        "attention"
    }
    .to_string();
    let development_total = total.saturating_sub(holdout_total);
    let development_passed = passed.saturating_sub(holdout_passed);
    Ok(WebRagEvalQuickSummary {
        ok,
        status,
        total,
        passed,
        failed,
        semantic_fallbacks: 0,
        grounded_answers: RagEvalGroundedSummary {
            passed: grounded_passed,
            failed: total.saturating_sub(grounded_passed),
            coverage: grounded_coverage,
            expected_in_answer: grounded_passed,
            cited_answers: grounded_passed,
            unknown_citation_cases: 0,
        },
        eval_matrix: RagEvalMatrixSummary {
            status: matrix_status,
            stored_cases: total,
            auto_cases: 0,
            recommended_min_stored_cases: 12,
            total_dimensions,
            covered_dimensions,
            coverage,
            dimensions,
            missing_dimensions,
        },
        retrieval_tuning: RagEvalRetrievalTuningSummary {
            status: retrieval_status,
            selected_profile,
            candidate_recall,
            selection_recall,
            chunk_selection_rate: 0.0,
            memory_selection_rate: 0.0,
            semantic_fallback_rate: 0.0,
            near_miss_count: 0,
            reasons: vec![
                "quick web summary uses the latest RAG eval baseline; run GET /rag-eval for full retrieval diagnostics"
                    .to_string(),
            ],
        },
        split: RagEvalSplitSummary {
            development_total,
            development_passed,
            development_recall: web_ratio_percent(development_passed, development_total),
            holdout_total,
            holdout_passed,
            holdout_recall,
            holdout_grounded_coverage,
            recommended_min_holdout_cases: 5,
            holdout_ready,
            tuning_isolation_enforced: true,
            holdout_policy: "labelled holdout is evaluated after retrieval configuration is fixed; evaluation never mutates ranking"
                .to_string(),
            origin_independence_verified: false,
            development_signature: String::new(),
            holdout_signature: String::new(),
        },
        baseline,
        detail: "quick summary; full RAG eval is available through /rag-eval".to_string(),
    })
}

fn web_rag_eval_case_dimensions(text: &str) -> Vec<&'static str> {
    let text = text.to_lowercase();
    let mut dimensions = Vec::new();
    if text.contains(".rs")
        || text.contains(".md")
        || text.contains(".toml")
        || text.contains("chunk")
        || text.contains("source")
        || text.contains("rag")
    {
        dimensions.push("source_chunk");
    }
    if text.contains("memory") || text.contains("card") || text.contains("пам") {
        dimensions.push("memory_card");
    }
    if text.contains("dukememory")
        || text.contains(" --")
        || text.contains(" cli")
        || text.contains("command")
    {
        dimensions.push("cli_workflow");
    }
    if text.contains("mcp") || text.contains("memory_") || text.contains("agent-session") {
        dimensions.push("mcp_tooling");
    }
    if text.contains("http")
        || text.contains("endpoint")
        || text.contains("/web-control")
        || text.contains("web")
    {
        dimensions.push("http_api");
    }
    if text.contains("graph")
        || text.contains("relationship")
        || text.contains("edge")
        || text.contains("node")
        || text.contains("link")
    {
        dimensions.push("graph_memory");
    }
    if text
        .chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch))
    {
        dimensions.push("multilingual");
    }
    if text.contains("missing") || text.contains("нет ") || text.contains("не ") {
        dimensions.push("negative_or_missing");
    }
    if text.contains("packing")
        || text.contains("suppress")
        || text.contains("overlap")
        || text.contains("near")
        || text.contains("file-cap")
    {
        dimensions.push("packing_near_miss");
    }
    dimensions.sort_unstable();
    dimensions.dedup();
    dimensions
}

fn web_rag_eval_baseline_quick_summary(root: &Path) -> Result<RagEvalBaselineSummary> {
    let path = root.join(".agent/rag-eval-baseline.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(RagEvalBaselineSummary {
            status: "missing".to_string(),
            path: path.display().to_string(),
            present: false,
            written: false,
            regression: false,
            current_signature: String::new(),
            baseline_signature: None,
            baseline_recall: None,
            baseline_grounded_coverage: None,
            baseline_matrix_coverage: None,
            baseline_candidate_recall: None,
            baseline_selection_recall: None,
            baseline_hit_at_3_rate: None,
            baseline_mean_reciprocal_rank: None,
            detail: "no RAG eval baseline has been written for this project".to_string(),
        });
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Ok(RagEvalBaselineSummary {
            status: "invalid".to_string(),
            path: path.display().to_string(),
            present: true,
            written: false,
            regression: false,
            current_signature: String::new(),
            baseline_signature: None,
            baseline_recall: None,
            baseline_grounded_coverage: None,
            baseline_matrix_coverage: None,
            baseline_candidate_recall: None,
            baseline_selection_recall: None,
            baseline_hit_at_3_rate: None,
            baseline_mean_reciprocal_rank: None,
            detail: "RAG eval baseline file exists but could not be parsed".to_string(),
        });
    };
    let signature = value
        .get("signature")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(RagEvalBaselineSummary {
        status: "present".to_string(),
        path: path.display().to_string(),
        present: true,
        written: false,
        regression: false,
        current_signature: signature.clone(),
        baseline_signature: Some(signature),
        baseline_recall: value.get("recall").and_then(Value::as_f64),
        baseline_grounded_coverage: value.get("grounded_coverage").and_then(Value::as_f64),
        baseline_matrix_coverage: value.get("matrix_coverage").and_then(Value::as_f64),
        baseline_candidate_recall: value.get("candidate_recall").and_then(Value::as_f64),
        baseline_selection_recall: value.get("selection_recall").and_then(Value::as_f64),
        baseline_hit_at_3_rate: value.get("hit_at_3_rate").and_then(Value::as_f64),
        baseline_mean_reciprocal_rank: value.get("mean_reciprocal_rank").and_then(Value::as_f64),
        detail: "baseline present; run GET /rag-eval for full signature comparison".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_rag_summary_is_empty_and_honest_without_cases_or_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let conn = open_db(&temp.path().join("memory.db")).unwrap();
        let report = web_rag_eval_quick_summary(&conn, temp.path()).unwrap();

        assert!(!report.ok);
        assert_eq!(report.status, "empty");
        assert_eq!(report.total, 0);
        assert_eq!(report.baseline.status, "missing");
        assert!(report.split.tuning_isolation_enforced);
        assert!(!report.split.origin_independence_verified);
    }

    #[test]
    fn quick_rag_dimensions_keep_multilingual_and_transport_coverage() {
        let dimensions = web_rag_eval_case_dimensions(
            "Память MCP HTTP graph RAG source missing packing overlap",
        );
        assert!(dimensions.contains(&"multilingual"));
        assert!(dimensions.contains(&"mcp_tooling"));
        assert!(dimensions.contains(&"http_api"));
        assert!(dimensions.contains(&"graph_memory"));
        assert!(dimensions.contains(&"packing_near_miss"));
    }
}

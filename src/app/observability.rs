use super::*;

const FRESH_MEMORY_GRACE_MS: i64 = 86_400_000;
const GAP_INBOX_STALE_MS: i64 = 3_600_000;

#[derive(Debug, Serialize)]
pub(crate) struct MemoryReadEvent {
    pub(crate) id: i64,
    pub(crate) command: String,
    pub(crate) query: String,
    pub(crate) memory_ids: Vec<String>,
    pub(crate) semantic_used: bool,
    pub(crate) result_count: usize,
    pub(crate) budget: usize,
    pub(crate) elapsed_ms: u128,
    pub(crate) created_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct UsageReport {
    pub(crate) since_days: i64,
    pub(crate) read_count: usize,
    pub(crate) write_count: usize,
    pub(crate) write_pressure: f64,
    pub(crate) semantic_read_count: usize,
    pub(crate) fallback_read_count: usize,
    pub(crate) semantic_eligible_read_count: usize,
    pub(crate) semantic_eligible_total: usize,
    pub(crate) semantic_eligible_read_rate: f64,
    pub(crate) semantic_reads_with_results: usize,
    pub(crate) semantic_empty_read_count: usize,
    pub(crate) semantic_result_rate: f64,
    pub(crate) semantic_avg_results: f64,
    pub(crate) semantic_eligible_reads_with_results: usize,
    pub(crate) semantic_eligible_empty_read_count: usize,
    pub(crate) semantic_eligible_result_rate: f64,
    pub(crate) semantic_empty_queries: Vec<String>,
    pub(crate) nonsemantic_read_count: usize,
    pub(crate) unique_memory_ids: usize,
    pub(crate) top_memories: Vec<UsageMemoryItem>,
    pub(crate) reads_by_command: BTreeMap<String, usize>,
    pub(crate) writes_by_type: BTreeMap<String, usize>,
    pub(crate) recent_reads: Vec<MemoryReadEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UsageMemoryItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) request_count: usize,
    pub(crate) last_read_at: i64,
    pub(crate) body_chars: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct UsefulnessReport {
    pub(crate) since_days: i64,
    pub(crate) stale_days: i64,
    pub(crate) hot_threshold: usize,
    pub(crate) total_active: usize,
    pub(crate) hot: Vec<UsefulnessItem>,
    pub(crate) unused: Vec<UsefulnessItem>,
    pub(crate) stale: Vec<UsefulnessItem>,
    pub(crate) too_long: Vec<UsefulnessItem>,
    pub(crate) no_links: Vec<UsefulnessItem>,
    pub(crate) missing_links: Vec<LinkReport>,
    pub(crate) duplicate_candidates: Vec<MergeCandidate>,
    pub(crate) suggestions: Vec<UsefulnessSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UsefulnessItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) request_count: usize,
    pub(crate) updated_at: i64,
    pub(crate) body_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UsefulnessSuggestion {
    pub(crate) action: String,
    pub(crate) id: Option<String>,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryQuality {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) score: f64,
    pub(crate) usefulness_score: f64,
    pub(crate) token_saving_score: f64,
    pub(crate) risk_score: f64,
    pub(crate) request_count: usize,
    pub(crate) positive_feedback: usize,
    pub(crate) negative_feedback: usize,
    pub(crate) body_chars: usize,
    pub(crate) links: usize,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QualityReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) total: usize,
    pub(crate) average_score: f64,
    pub(crate) strongest: Vec<MemoryQuality>,
    pub(crate) weakest: Vec<MemoryQuality>,
    pub(crate) items: Vec<MemoryQuality>,
    pub(crate) suggestions: Vec<UsefulnessSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryRoiReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) read_count: usize,
    pub(crate) write_count: usize,
    pub(crate) write_pressure: f64,
    pub(crate) unique_memory_ids: usize,
    pub(crate) reused_card_rate: f64,
    pub(crate) useful_rate: f64,
    pub(crate) token_saving_estimate: usize,
    pub(crate) top_memories: Vec<UsageMemoryItem>,
    pub(crate) noisy_memory_ids: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentAuditReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) read_count: usize,
    pub(crate) brief_reads: usize,
    pub(crate) impact_reads: usize,
    pub(crate) evidence_reads: usize,
    pub(crate) feedback_events: usize,
    pub(crate) durable_writes: usize,
    pub(crate) semantic_eligible_result_rate: f64,
    pub(crate) inferred_missing: usize,
    pub(crate) commands: BTreeMap<String, usize>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RemoteStatusReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) local_first: bool,
    pub(crate) ready: bool,
    pub(crate) export_command: String,
    pub(crate) import_command: String,
    pub(crate) write_pressure: f64,
    pub(crate) embedding_current: bool,
    pub(crate) provider_reachable: bool,
    pub(crate) backup_ready: bool,
    pub(crate) estimated_local_latency_ms: u32,
    pub(crate) estimated_vds_latency_ms: u32,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FeedbackSummary {
    pub(crate) since_days: i64,
    pub(crate) positive: usize,
    pub(crate) negative: usize,
    pub(crate) missing: usize,
    pub(crate) events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FeedbackReport {
    pub(crate) ok: bool,
    pub(crate) rating: String,
    pub(crate) ids: Vec<String>,
    pub(crate) written_event: String,
    pub(crate) summary: FeedbackSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BudgetPlan {
    pub(crate) task: String,
    pub(crate) profile: String,
    pub(crate) max_chars: usize,
    pub(crate) include_recent: usize,
    pub(crate) semantic: bool,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProjectProfileSnapshot {
    pub(crate) root: String,
    pub(crate) active_profile: Option<String>,
    pub(crate) memory_count: usize,
    pub(crate) pending_inbox: usize,
    pub(crate) decisions: usize,
    pub(crate) constraints: usize,
    pub(crate) commands: usize,
    pub(crate) known_issues: usize,
    pub(crate) embedding_provider: String,
    pub(crate) embedding_endpoint: String,
    pub(crate) embedding_model: String,
    pub(crate) recommended_budget: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) total_projects: usize,
    pub(crate) ready_projects: usize,
    pub(crate) attention_projects: usize,
    pub(crate) stale_projects: usize,
    pub(crate) missing_live_eval_projects: usize,
    pub(crate) memory_gap_projects: usize,
    pub(crate) memory_gap_count: usize,
    pub(crate) semantic_empty_gap_projects: usize,
    pub(crate) semantic_empty_gap_count: usize,
    pub(crate) semantic_empty_projects: usize,
    pub(crate) semantic_empty_read_count: usize,
    pub(crate) semantic_result_warn_projects: usize,
    pub(crate) gap_inbox_pending_projects: usize,
    pub(crate) gap_inbox_pending_count: usize,
    pub(crate) gap_inbox_stale_projects: usize,
    pub(crate) gap_inbox_stale_count: usize,
    pub(crate) gap_inbox_oldest_pending_age_secs: Option<i64>,
    pub(crate) recommendations_count: usize,
    pub(crate) attention_reason_counts: BTreeMap<String, usize>,
    pub(crate) repair_actions_count: usize,
    pub(crate) safe_repair_actions_count: usize,
    pub(crate) repair_loop_projects: usize,
    pub(crate) repair_loop_failed_projects: usize,
    pub(crate) repair_loop_safe_skipped_projects: usize,
    pub(crate) daemon_embedding_skipped_projects: usize,
    pub(crate) daemon_embedding_repaired_projects: usize,
    pub(crate) projects: Vec<ProjectDashboardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairAction {
    pub(crate) code: String,
    pub(crate) reason: String,
    pub(crate) safe_auto: bool,
    pub(crate) description: String,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairReport {
    pub(crate) version: u32,
    pub(crate) apply: bool,
    pub(crate) ok: bool,
    pub(crate) total_actions: usize,
    pub(crate) safe_actions: usize,
    pub(crate) applied_actions: usize,
    pub(crate) skipped_actions: usize,
    pub(crate) failed_actions: usize,
    pub(crate) projects: Vec<DashboardRepairProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairHistoryReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) total_runs: usize,
    pub(crate) applied_actions: usize,
    pub(crate) skipped_actions: usize,
    pub(crate) failed_actions: usize,
    pub(crate) safe_actions: usize,
    pub(crate) runs_by_source: BTreeMap<String, usize>,
    pub(crate) actions_by_code: BTreeMap<String, usize>,
    pub(crate) manual_actions_by_code: BTreeMap<String, usize>,
    pub(crate) projects: Vec<DashboardRepairHistoryProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairHistoryProject {
    pub(crate) name: String,
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) total_runs: usize,
    pub(crate) applied_actions: usize,
    pub(crate) skipped_actions: usize,
    pub(crate) failed_actions: usize,
    pub(crate) safe_actions: usize,
    pub(crate) recent: Vec<DashboardRepairHistoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairHistoryEvent {
    pub(crate) id: i64,
    pub(crate) created_at: i64,
    pub(crate) source: String,
    pub(crate) total_actions: usize,
    pub(crate) applied_actions: usize,
    pub(crate) skipped_actions: usize,
    pub(crate) failed_actions: usize,
    pub(crate) safe_actions: usize,
    pub(crate) actions: Vec<DashboardRepairResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairProject {
    pub(crate) name: String,
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) priority: i64,
    pub(crate) gap_inbox_stale_pending: usize,
    pub(crate) gap_inbox_oldest_pending_age_secs: Option<i64>,
    pub(crate) actions: Vec<DashboardRepairResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DashboardRepairResult {
    pub(crate) code: String,
    pub(crate) reason: String,
    pub(crate) safe_auto: bool,
    pub(crate) applied: bool,
    pub(crate) skipped: bool,
    pub(crate) ok: bool,
    pub(crate) detail: String,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProjectDashboardItem {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) attention: bool,
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) memories: i64,
    pub(crate) pending_inbox: i64,
    pub(crate) quality_average: Option<f64>,
    pub(crate) autonomous_ok: Option<bool>,
    pub(crate) autonomous_age_secs: Option<i64>,
    pub(crate) autonomous_fresh: Option<bool>,
    pub(crate) autonomous_live_reads: Option<usize>,
    pub(crate) autonomous_useful_rate: Option<f64>,
    pub(crate) autonomous_useful_rate_source: Option<String>,
    pub(crate) autonomous_inferred_missing: Option<usize>,
    pub(crate) autonomous_semantic_empty_missing: Option<usize>,
    pub(crate) autonomous_semantic_empty_missing_queries: Vec<String>,
    pub(crate) daemon_embedding_skipped: Option<bool>,
    pub(crate) daemon_embedding_error: Option<String>,
    pub(crate) daemon_embedding_repaired_at: Option<i64>,
    pub(crate) daemon_embedding_repair_source: Option<String>,
    pub(crate) embedding_missing: Option<usize>,
    pub(crate) embedding_provider_reachable: Option<bool>,
    pub(crate) embedding_provider_health_ms: Option<u128>,
    pub(crate) embedding_provider_error: Option<String>,
    pub(crate) semantic_read_rate: Option<f64>,
    pub(crate) semantic_result_rate: Option<f64>,
    pub(crate) semantic_empty_read_count: Option<usize>,
    pub(crate) semantic_avg_results: Option<f64>,
    pub(crate) semantic_eligible_result_rate: Option<f64>,
    pub(crate) semantic_eligible_empty_read_count: Option<usize>,
    pub(crate) semantic_empty_queries: Vec<String>,
    pub(crate) recommended_budget: Option<String>,
    pub(crate) write_pressure: Option<f64>,
    pub(crate) top_memories: Vec<UsageMemoryItem>,
    pub(crate) repair_loop: OpsRepairLoopStatus,
    pub(crate) gap_inbox: DashboardGapInboxStatus,
    pub(crate) attention_reasons: Vec<String>,
    pub(crate) repair_actions: Vec<DashboardRepairAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct DaemonEmbeddingSnapshot {
    skipped: Option<bool>,
    error: Option<String>,
    repaired_at: Option<i64>,
    repair_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct DashboardGapInboxStatus {
    pub(crate) total: usize,
    pub(crate) pending: usize,
    pub(crate) stale_pending: usize,
    pub(crate) approved: usize,
    pub(crate) rejected: usize,
    pub(crate) oldest_pending_age_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryQaReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) score: f64,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) reads: usize,
    pub(crate) writes: usize,
    pub(crate) write_pressure: f64,
    pub(crate) semantic_read_rate: f64,
    pub(crate) semantic_result_rate: f64,
    pub(crate) semantic_empty_read_count: usize,
    pub(crate) semantic_avg_results: f64,
    pub(crate) semantic_eligible_result_rate: f64,
    pub(crate) semantic_eligible_empty_read_count: usize,
    pub(crate) semantic_empty_queries: Vec<String>,
    pub(crate) useful_rate: f64,
    pub(crate) useful_rate_source: String,
    pub(crate) feedback_useful_rate: f64,
    pub(crate) inferred_useful_rate: f64,
    pub(crate) inferred_missing: usize,
    pub(crate) inferred_missing_queries: Vec<String>,
    pub(crate) quality_average: f64,
    pub(crate) active_memories: usize,
    pub(crate) unused: usize,
    pub(crate) stale: usize,
    pub(crate) too_long: usize,
    pub(crate) duplicate_candidates: usize,
    pub(crate) embedding_missing: usize,
    pub(crate) embedding_stale: usize,
    pub(crate) autonomous_ok: Option<bool>,
    pub(crate) token_saving_estimate: usize,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsStatusReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) score: f64,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) effectiveness: OpsEffectivenessStatus,
    pub(crate) quality_loop: OpsQualityLoopStatus,
    pub(crate) embeddings: OpsEmbeddingStatus,
    pub(crate) autonomous: OpsAutonomousStatus,
    pub(crate) repair_loop: OpsRepairLoopStatus,
    pub(crate) gap_inbox: DashboardGapInboxStatus,
    pub(crate) agent_integration: OpsAgentIntegrationStatus,
    pub(crate) storage: OpsStorageStatus,
    pub(crate) multi_device: OpsMultiDeviceStatus,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsEffectivenessStatus {
    pub(crate) reads: usize,
    pub(crate) writes: usize,
    pub(crate) unique_memory_ids: usize,
    pub(crate) semantic_read_rate: f64,
    pub(crate) semantic_result_rate: f64,
    pub(crate) semantic_empty_read_count: usize,
    pub(crate) semantic_avg_results: f64,
    pub(crate) semantic_eligible_result_rate: f64,
    pub(crate) semantic_eligible_empty_read_count: usize,
    pub(crate) semantic_empty_queries: Vec<String>,
    pub(crate) useful_rate: f64,
    pub(crate) useful_rate_source: String,
    pub(crate) feedback_useful_rate: f64,
    pub(crate) inferred_useful_rate: f64,
    pub(crate) inferred_missing: usize,
    pub(crate) inferred_missing_queries: Vec<String>,
    pub(crate) token_saving_estimate: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsQualityLoopStatus {
    pub(crate) average_score: f64,
    pub(crate) total_cards: usize,
    pub(crate) weakest_cards: usize,
    pub(crate) unused_cards: usize,
    pub(crate) stale_cards: usize,
    pub(crate) too_long_cards: usize,
    pub(crate) duplicate_candidates: usize,
    pub(crate) reversible_cleanup_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsEmbeddingStatus {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) eligible: usize,
    pub(crate) indexed: usize,
    pub(crate) missing: usize,
    pub(crate) stale: usize,
    pub(crate) current: bool,
    pub(crate) provider_reachable: bool,
    pub(crate) provider_health_ms: Option<u128>,
    pub(crate) provider_error: Option<String>,
    pub(crate) background_sync_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsAutonomousStatus {
    pub(crate) installed: bool,
    pub(crate) ok: Option<bool>,
    pub(crate) status_file: String,
    pub(crate) rollback_ready: bool,
    pub(crate) updated_at: Option<i64>,
    pub(crate) age_secs: Option<i64>,
    pub(crate) fresh: bool,
    pub(crate) last_action_count: Option<usize>,
    pub(crate) daemon_embedding_skipped: Option<bool>,
    pub(crate) daemon_embedding_error: Option<String>,
    pub(crate) daemon_embedding_repaired_at: Option<i64>,
    pub(crate) daemon_embedding_repair_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpsRepairLoopStatus {
    pub(crate) observed: bool,
    pub(crate) healthy: bool,
    pub(crate) runs: usize,
    pub(crate) applied_actions: usize,
    pub(crate) skipped_actions: usize,
    pub(crate) failed_actions: usize,
    pub(crate) safe_actions: usize,
    pub(crate) safe_skipped_actions: usize,
    pub(crate) manual_actions: usize,
    pub(crate) last_run_at: Option<i64>,
    pub(crate) last_source: Option<String>,
    pub(crate) last_action_count: Option<usize>,
    pub(crate) actions_by_code: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsAgentIntegrationStatus {
    pub(crate) ready: bool,
    pub(crate) project_memory_installed: bool,
    pub(crate) project_config_present: bool,
    pub(crate) agents_block_present: bool,
    pub(crate) skill_installed: bool,
    pub(crate) codex_mcp_configured: bool,
    pub(crate) skill_path: String,
    pub(crate) codex_config: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsStorageStatus {
    pub(crate) db_bytes: u64,
    pub(crate) page_count: i64,
    pub(crate) freelist_count: i64,
    pub(crate) freelist_ratio: f64,
    pub(crate) vacuum_recommended: bool,
    pub(crate) agent_bytes: u64,
    pub(crate) backups_bytes: u64,
    pub(crate) backups_count: usize,
    pub(crate) rollback_bytes: u64,
    pub(crate) rollback_count: usize,
    pub(crate) install_backups_bytes: u64,
    pub(crate) install_backups_count: usize,
    pub(crate) retention_ready: bool,
    pub(crate) pressure: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OpsMultiDeviceStatus {
    pub(crate) ready: bool,
    pub(crate) local_first: bool,
    pub(crate) export_command: String,
    pub(crate) import_command: String,
    pub(crate) blockers: Vec<String>,
}

pub(crate) struct ReadEventInput<'a> {
    pub(crate) command: &'a str,
    pub(crate) query: &'a str,
    pub(crate) ids: &'a [String],
    pub(crate) semantic_used: bool,
    pub(crate) result_count: usize,
    pub(crate) budget: usize,
    pub(crate) elapsed_ms: u128,
}

pub(crate) fn memory_receipt(
    command: &str,
    semantic_used: Option<bool>,
    ids: &[String],
    wrote: &str,
) -> String {
    let semantic = semantic_used
        .map(MemorySemanticStatus::from)
        .unwrap_or_default();
    memory_receipt_with_semantic(command, semantic, ids, wrote)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum MemorySemanticStatus {
    #[default]
    None,
    Used,
    Fallback,
    Skipped,
}

impl From<bool> for MemorySemanticStatus {
    fn from(value: bool) -> Self {
        if value { Self::Used } else { Self::Fallback }
    }
}

pub(crate) fn memory_receipt_with_semantic(
    command: &str,
    semantic_status: MemorySemanticStatus,
    ids: &[String],
    wrote: &str,
) -> String {
    let semantic = match semantic_status {
        MemorySemanticStatus::None => "",
        MemorySemanticStatus::Used => "; semantic search used",
        MemorySemanticStatus::Fallback => "; semantic search fallback",
        MemorySemanticStatus::Skipped => "; semantic search skipped",
    };
    let matched = match ids.len() {
        1 => "1 card".to_string(),
        count => format!("{count} cards"),
    };
    let saved = if wrote == "none" {
        "saved nothing".to_string()
    } else {
        format!("saved {wrote}")
    };
    format!("Memory: read {command}; matched {matched}{semantic}; {saved}.")
}

pub(crate) fn log_read_event(conn: &Connection, input: ReadEventInput<'_>) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_read_events \
         (command, query, memory_ids, semantic_used, result_count, budget, elapsed_ms, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            input.command,
            truncate_chars(input.query, 500),
            input.ids.join(","),
            if input.semantic_used { 1 } else { 0 },
            input.result_count.min(i64::MAX as usize) as i64,
            input.budget.min(i64::MAX as usize) as i64,
            input.elapsed_ms.min(i64::MAX as u128) as i64,
            now_ms()
        ],
    )?;
    Ok(())
}

pub(crate) fn print_audit(conn: &Connection, limit: usize, json_out: bool) -> Result<()> {
    let events = audit_events(conn, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else if events.is_empty() {
        println!("audit: none");
    } else {
        for event in events {
            let memory_id = event.memory_id.unwrap_or_else(|| "-".to_string());
            println!(
                "{}  {}  {}  {}",
                event.id, event.event_type, memory_id, event.detail
            );
        }
    }
    Ok(())
}

pub(crate) fn print_usage_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = usage_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Usage Report");
    println!("since_days: {}", report.since_days);
    println!("reads: {}", report.read_count);
    println!("writes: {}", report.write_count);
    println!("write_pressure: {:.2}", report.write_pressure);
    println!("semantic_reads: {}", report.semantic_read_count);
    println!("fallback_reads: {}", report.fallback_read_count);
    println!(
        "semantic_results: {}/{} ({:.1}%), avg_results={:.2}, empty={}",
        report.semantic_reads_with_results,
        report.semantic_read_count,
        report.semantic_result_rate * 100.0,
        report.semantic_avg_results,
        report.semantic_empty_read_count
    );
    println!(
        "semantic_eligible_reads: {}/{} ({:.1}%)",
        report.semantic_eligible_read_count,
        report.semantic_eligible_total,
        report.semantic_eligible_read_rate * 100.0
    );
    println!(
        "semantic_eligible_results: {}/{} ({:.1}%), empty={}",
        report.semantic_eligible_reads_with_results,
        report.semantic_eligible_read_count,
        report.semantic_eligible_result_rate * 100.0,
        report.semantic_eligible_empty_read_count
    );
    if !report.semantic_empty_queries.is_empty() {
        println!("semantic_empty_queries:");
        for query in &report.semantic_empty_queries {
            println!("- {query}");
        }
    }
    println!("nonsemantic_reads: {}", report.nonsemantic_read_count);
    println!("unique_memory_ids: {}", report.unique_memory_ids);
    if !report.top_memories.is_empty() {
        println!("top_memories:");
        for memory in &report.top_memories {
            println!(
                "- {} [{}] requests={} last_read_at={} {}",
                memory.id,
                memory.memory_type,
                memory.request_count,
                memory.last_read_at,
                memory.title
            );
        }
    }
    if !report.reads_by_command.is_empty() {
        println!("reads_by_command:");
        for (command, count) in &report.reads_by_command {
            println!("- {command}: {count}");
        }
    }
    if !report.writes_by_type.is_empty() {
        println!("writes_by_type:");
        for (event_type, count) in &report.writes_by_type {
            println!("- {event_type}: {count}");
        }
    }
    if report.recent_reads.is_empty() {
        println!("recent_reads: none");
    } else {
        println!("recent_reads:");
        for event in &report.recent_reads {
            let ids = if event.memory_ids.is_empty() {
                "-".to_string()
            } else {
                event
                    .memory_ids
                    .iter()
                    .take(6)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            };
            println!(
                "- {} {} semantic={} results={} ids=[{}] {}",
                event.id,
                event.command,
                event.semantic_used,
                event.result_count,
                ids,
                truncate_chars(&event.query, 90)
            );
        }
    }
    Ok(())
}

pub(crate) fn usage_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<UsageReport> {
    let since_days = since_days.max(0);
    let since_ms = now_ms().saturating_sub(since_days.saturating_mul(86_400_000));
    let recent_reads = read_events(conn, since_ms, limit)?;
    let all_reads = read_events(conn, since_ms, usize::MAX)?;
    let mut reads_by_command = BTreeMap::new();
    let mut unique_ids = HashSet::new();
    let mut memory_counts: HashMap<String, usize> = HashMap::new();
    let mut memory_last_read_at: HashMap<String, i64> = HashMap::new();
    let mut semantic_read_count = 0;
    let mut semantic_reads_with_results = 0;
    let mut semantic_result_total = 0usize;
    let mut semantic_eligible_read_count = 0;
    let mut semantic_eligible_total = 0;
    let mut semantic_eligible_reads_with_results = 0;
    let mut semantic_empty_queries = Vec::new();
    let mut semantic_empty_query_seen = HashSet::new();
    for event in &all_reads {
        *reads_by_command.entry(event.command.clone()).or_insert(0) += 1;
        if event.semantic_used {
            semantic_read_count += 1;
            semantic_result_total = semantic_result_total.saturating_add(event.result_count);
            if event.result_count > 0 {
                semantic_reads_with_results += 1;
            }
        }
        if semantic_eligible_read_event(event) {
            semantic_eligible_total += 1;
            if event.semantic_used {
                semantic_eligible_read_count += 1;
                if event.result_count > 0 {
                    semantic_eligible_reads_with_results += 1;
                } else if semantic_empty_queries.len() < 5 {
                    let query = truncate_chars(event.query.trim(), 160);
                    if semantic_empty_query_seen.insert(query.clone()) {
                        semantic_empty_queries.push(query);
                    }
                }
            }
        }
        for id in &event.memory_ids {
            unique_ids.insert(id.clone());
            *memory_counts.entry(id.clone()).or_insert(0) += 1;
            memory_last_read_at
                .entry(id.clone())
                .and_modify(|last| *last = (*last).max(event.created_at))
                .or_insert(event.created_at);
        }
    }
    let top_memories = usage_top_memories(conn, &memory_counts, &memory_last_read_at, limit)?;
    let mut writes_by_type = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT event_type, COUNT(*) FROM memory_events WHERE created_at >= ?1 GROUP BY event_type",
    )?;
    let write_rows = stmt.query_map(params![since_ms], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut write_count = 0;
    for row in write_rows {
        let (event_type, count) = row?;
        let count = count.max(0) as usize;
        write_count += count;
        writes_by_type.insert(event_type, count);
    }
    Ok(UsageReport {
        since_days,
        read_count: all_reads.len(),
        write_count,
        write_pressure: ratio(write_count, all_reads.len().max(1)),
        semantic_read_count,
        fallback_read_count: all_reads.len().saturating_sub(semantic_read_count),
        semantic_eligible_read_count,
        semantic_eligible_total,
        semantic_eligible_read_rate: if semantic_eligible_total == 0 {
            0.0
        } else {
            semantic_eligible_read_count as f64 / semantic_eligible_total as f64
        },
        semantic_reads_with_results,
        semantic_empty_read_count: semantic_read_count.saturating_sub(semantic_reads_with_results),
        semantic_result_rate: ratio(semantic_reads_with_results, semantic_read_count),
        semantic_avg_results: if semantic_read_count == 0 {
            0.0
        } else {
            semantic_result_total as f64 / semantic_read_count as f64
        },
        semantic_eligible_reads_with_results,
        semantic_eligible_empty_read_count: semantic_eligible_read_count
            .saturating_sub(semantic_eligible_reads_with_results),
        semantic_eligible_result_rate: ratio(
            semantic_eligible_reads_with_results,
            semantic_eligible_read_count,
        ),
        semantic_empty_queries,
        nonsemantic_read_count: all_reads.len().saturating_sub(semantic_eligible_total),
        unique_memory_ids: unique_ids.len(),
        top_memories,
        reads_by_command,
        writes_by_type,
        recent_reads,
    })
}

fn usage_top_memories(
    conn: &Connection,
    counts: &HashMap<String, usize>,
    last_read_at: &HashMap<String, i64>,
    limit: usize,
) -> Result<Vec<UsageMemoryItem>> {
    if counts.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let rows = query_memories(
        conn,
        None,
        &[],
        &[
            "active".to_string(),
            "uncertain".to_string(),
            "superseded".to_string(),
        ],
        None,
        usize::MAX,
    )?;
    let mut items = rows
        .into_iter()
        .filter_map(|memory| {
            let request_count = counts.get(&memory.id).copied().unwrap_or(0);
            if request_count == 0 {
                return None;
            }
            Some(UsageMemoryItem {
                id: memory.id.clone(),
                memory_type: memory.memory_type,
                scope: memory.scope,
                title: memory.title,
                status: memory.status,
                request_count,
                last_read_at: last_read_at.get(&memory.id).copied().unwrap_or_default(),
                body_chars: memory.body.chars().count(),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .request_count
            .cmp(&left.request_count)
            .then_with(|| right.last_read_at.cmp(&left.last_read_at))
            .then_with(|| left.title.cmp(&right.title))
    });
    items.truncate(limit);
    Ok(items)
}

fn semantic_eligible_read_event(event: &MemoryReadEvent) -> bool {
    if !matches!(
        event.command.as_str(),
        "brief"
            | "impact"
            | "retrieve"
            | "recall"
            | "search"
            | "context"
            | "context-pack"
            | "memory_search"
            | "memory_context_pack"
            | "memory_agent_context"
            | "memory_snapshot"
    ) {
        return false;
    }
    let query = event.query.trim();
    if query.is_empty() || semantic_usage_code_identifier_query(query) {
        return false;
    }
    relevance_terms(query).len() >= 2
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn semantic_usage_code_identifier_query(query: &str) -> bool {
    let query = query.trim();
    !query.is_empty()
        && !query.chars().any(char::is_whitespace)
        && (query.contains("::")
            || query.contains('/')
            || query.contains('\\')
            || query.contains('.')
            || query.contains('_')
            || query
                .chars()
                .any(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit()))
}

pub(crate) fn print_usefulness_report(
    conn: &Connection,
    since_days: i64,
    stale_days: i64,
    hot_threshold: usize,
    json_out: bool,
) -> Result<()> {
    let report = usefulness_report(conn, since_days, stale_days, hot_threshold)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Usefulness Report");
    println!("active: {}", report.total_active);
    println!("hot: {}", report.hot.len());
    println!("unused: {}", report.unused.len());
    println!("stale: {}", report.stale.len());
    println!("too_long: {}", report.too_long.len());
    println!("no_links: {}", report.no_links.len());
    println!("missing_links: {}", report.missing_links.len());
    println!(
        "duplicate_candidates: {}",
        report.duplicate_candidates.len()
    );
    render_usefulness_items("Hot", &report.hot);
    render_usefulness_items("Unused", &report.unused);
    render_usefulness_items("Stale", &report.stale);
    if !report.suggestions.is_empty() {
        println!("Suggestions:");
        for suggestion in &report.suggestions {
            let id = suggestion.id.as_deref().unwrap_or("-");
            println!("- {} {} {}", suggestion.action, id, suggestion.detail);
        }
    }
    Ok(())
}

pub(crate) fn render_usefulness_items(title: &str, items: &[UsefulnessItem]) {
    if items.is_empty() {
        return;
    }
    println!("{title}:");
    for item in items.iter().take(10) {
        println!(
            "- {} [{}] requests={} {}",
            item.id, item.memory_type, item.request_count, item.title
        );
    }
}

pub(crate) fn usefulness_report(
    conn: &Connection,
    since_days: i64,
    stale_days: i64,
    hot_threshold: usize,
) -> Result<UsefulnessReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let counts = memory_request_counts_since(conn, Some(since_ms))?;
    let fresh_cutoff = now_ms().saturating_sub(FRESH_MEMORY_GRACE_MS);
    let active = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let stale_cutoff = now_ms().saturating_sub(stale_days.max(0).saturating_mul(86_400_000));
    let mut hot = Vec::new();
    let mut unused = Vec::new();
    let mut stale = Vec::new();
    let mut too_long = Vec::new();
    let mut no_links = Vec::new();
    let mut suggestions = Vec::new();
    for memory in &active {
        let request_count = counts.get(&memory.id).copied().unwrap_or(0);
        let item = UsefulnessItem {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            request_count,
            updated_at: memory.updated_at,
            body_chars: memory.body.chars().count(),
        };
        if request_count >= hot_threshold.max(1) {
            hot.push(item.clone());
        }
        let fresh = memory.updated_at >= fresh_cutoff;
        if request_count == 0 && !fresh {
            unused.push(item.clone());
            if !quality_broad_history_task_state(memory) {
                suggestions.push(UsefulnessSuggestion {
                    action: "review_unused".to_string(),
                    id: Some(memory.id.clone()),
                    detail: "not used by recent memory reads; verify, link, supersede, or reject"
                        .to_string(),
                });
            }
        }
        if memory.updated_at < stale_cutoff {
            stale.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "review_stale".to_string(),
                id: Some(memory.id.clone()),
                detail: format!("not updated for at least {stale_days} day(s)"),
            });
        }
        if item.body_chars > 1200 {
            too_long.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "compact_body".to_string(),
                id: Some(memory.id.clone()),
                detail: "body is over 1200 chars; summarize for token-light recall".to_string(),
            });
        }
        if get_links(conn, &memory.id)?.is_empty() {
            no_links.push(item.clone());
            suggestions.push(UsefulnessSuggestion {
                action: "add_links".to_string(),
                id: Some(memory.id.clone()),
                detail: "add file:/symbol: links so memory_impact can target it".to_string(),
            });
        }
    }
    hot.sort_by_key(|item| std::cmp::Reverse(item.request_count));
    unused.sort_by_key(|item| std::cmp::Reverse(item.updated_at));
    stale.sort_by_key(|item| item.updated_at);
    too_long.sort_by_key(|item| std::cmp::Reverse(item.body_chars));
    no_links.sort_by_key(|item| std::cmp::Reverse(item.request_count));
    let missing_links = link_report(conn, None, Path::new("."), false)?
        .into_iter()
        .filter(|link| link.status == "missing")
        .collect::<Vec<_>>();
    for link in &missing_links {
        suggestions.push(UsefulnessSuggestion {
            action: "fix_missing_link".to_string(),
            id: Some(link.memory_id.clone()),
            detail: format!("{}:{} {}", link.kind, link.target, link.detail),
        });
    }
    let duplicate_candidates = merge_candidates(conn, 20)?;
    for candidate in &duplicate_candidates {
        suggestions.push(UsefulnessSuggestion {
            action: "merge_candidate".to_string(),
            id: Some(candidate.duplicate_id.clone()),
            detail: format!("merge into {} ({})", candidate.primary_id, candidate.reason),
        });
    }
    suggestions.truncate(100);
    Ok(UsefulnessReport {
        since_days,
        stale_days,
        hot_threshold,
        total_active: active.len(),
        hot,
        unused,
        stale,
        too_long,
        no_links,
        missing_links,
        duplicate_candidates,
        suggestions,
    })
}

pub(crate) fn print_quality_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = quality_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Quality Report");
    println!("total: {}", report.total);
    println!("average_score: {:.1}", report.average_score);
    for item in report.weakest.iter().take(10) {
        println!(
            "- {:.1} {} [{}] requests={} +{} -{} {}",
            item.score,
            item.id,
            item.memory_type,
            item.request_count,
            item.positive_feedback,
            item.negative_feedback,
            item.title
        );
    }
    Ok(())
}

pub(crate) fn print_roi_report(conn: &Connection, since_days: i64, json_out: bool) -> Result<()> {
    let report = roi_report(conn, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory ROI Report");
    println!("score: {:.1}", report.score);
    println!("reads: {}", report.read_count);
    println!("writes: {}", report.write_count);
    println!("write_pressure: {:.2}", report.write_pressure);
    println!("unique_memory_ids: {}", report.unique_memory_ids);
    println!("reused_card_rate: {:.1}%", report.reused_card_rate * 100.0);
    println!("useful_rate: {:.1}%", report.useful_rate * 100.0);
    println!("token_saving_estimate: {}", report.token_saving_estimate);
    if !report.top_memories.is_empty() {
        println!("top_memories:");
        for item in report.top_memories.iter().take(8) {
            println!(
                "- {} requests={} {}",
                item.id, item.request_count, item.title
            );
        }
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn roi_report(conn: &Connection, since_days: i64) -> Result<MemoryRoiReport> {
    let usage = usage_report(conn, since_days, 20)?;
    let quality = quality_report(conn, since_days, 20)?;
    let live = live_eval_report(conn, since_days)?;
    let reused = usage
        .top_memories
        .iter()
        .filter(|item| item.request_count >= 2)
        .count();
    let reused_card_rate = ratio(reused, usage.unique_memory_ids.max(1));
    let token_saving_estimate = quality
        .items
        .iter()
        .map(|item| {
            item.request_count
                .saturating_mul(item.body_chars.saturating_sub(240))
                / 4
        })
        .sum::<usize>();
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    if usage.read_count == 0 {
        issues.push("no recent memory reads".to_string());
        recommendations.push("start agent turns with dukememory brief or memory_brief".to_string());
    }
    if usage.write_pressure > 2.0 && usage.read_count >= 20 {
        issues.push(format!(
            "write pressure is high: {:.2}",
            usage.write_pressure
        ));
        recommendations.push("let autonomous throttling reduce low-value writes".to_string());
    }
    if live.useful_rate < 0.80 && live.feedback_events >= 5 {
        issues.push(format!(
            "useful rate is low: {:.0}%",
            live.useful_rate * 100.0
        ));
        recommendations.push("review noisy memories and missing feedback".to_string());
    }
    if reused == 0 && usage.read_count >= 5 {
        recommendations.push("promote reusable decisions/commands into durable cards".to_string());
    }
    let mut score = 100.0;
    if usage.read_count == 0 {
        score -= 25.0;
    }
    score -= (usage.write_pressure - 1.5).max(0.0).min(2.0) * 10.0;
    score -= live.inferred_missing.min(5) as f64 * 4.0;
    if live.feedback_events >= 5 {
        score -= ((0.90 - live.useful_rate).max(0.0) * 50.0).min(15.0);
    }
    score += (reused_card_rate.min(0.50) * 10.0).min(5.0);
    score = score.clamp(0.0, 100.0);
    Ok(MemoryRoiReport {
        version: 1,
        since_days,
        score,
        read_count: usage.read_count,
        write_count: usage.write_count,
        write_pressure: usage.write_pressure,
        unique_memory_ids: usage.unique_memory_ids,
        reused_card_rate,
        useful_rate: live.useful_rate,
        token_saving_estimate,
        top_memories: usage.top_memories,
        noisy_memory_ids: live.noisy_memory_ids,
        issues,
        recommendations,
    })
}

pub(crate) fn print_agent_audit(conn: &Connection, since_days: i64, json_out: bool) -> Result<()> {
    let report = agent_audit_report(conn, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Agent Behavior Audit");
    println!("score: {:.1}", report.score);
    println!("reads: {}", report.read_count);
    println!("brief_reads: {}", report.brief_reads);
    println!("impact_reads: {}", report.impact_reads);
    println!("feedback_events: {}", report.feedback_events);
    println!("durable_writes: {}", report.durable_writes);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn agent_audit_report(conn: &Connection, since_days: i64) -> Result<AgentAuditReport> {
    let usage = usage_report(conn, since_days, 50)?;
    let live = live_eval_report(conn, since_days)?;
    let commands = usage.reads_by_command.clone();
    let brief_reads =
        *commands.get("brief").unwrap_or(&0) + *commands.get("memory_brief").unwrap_or(&0);
    let impact_reads =
        *commands.get("impact").unwrap_or(&0) + *commands.get("memory_impact").unwrap_or(&0);
    let evidence_reads =
        *commands.get("evidence").unwrap_or(&0) + *commands.get("memory_evidence").unwrap_or(&0);
    let feedback_events = *usage.writes_by_type.get("memory_feedback").unwrap_or(&0);
    let durable_writes = ["memory_added", "memory_updated", "memory_merged"]
        .iter()
        .map(|key| usage.writes_by_type.get(*key).copied().unwrap_or(0))
        .sum::<usize>();
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    if usage.read_count == 0 {
        issues.push("no memory reads were recorded".to_string());
        recommendations.push("ensure the agent skill starts with memory_brief".to_string());
    }
    if brief_reads == 0 && usage.read_count >= 3 {
        issues.push("no brief/memory_brief reads recorded".to_string());
        recommendations.push("start each new task with brief before broad exploration".to_string());
    }
    if impact_reads == 0 && usage.read_count >= 10 {
        recommendations
            .push("use impact when touching a known file, symbol, or subsystem".to_string());
    }
    if usage.semantic_eligible_read_count >= 3 && usage.semantic_eligible_result_rate < 0.75 {
        issues.push("semantic eligible reads often return no results".to_string());
        recommendations.push("refresh embeddings or tune retrieval".to_string());
    }
    if live.inferred_missing > 0 {
        issues.push(format!("{} inferred memory gap(s)", live.inferred_missing));
        recommendations.push("add durable cards for repeated missing context".to_string());
    }
    if feedback_events == 0 && usage.read_count >= 20 {
        recommendations
            .push("record lightweight feedback for useful/useless/missing memory".to_string());
    }
    let mut score = 100.0;
    if usage.read_count == 0 {
        score -= 35.0;
    }
    if brief_reads == 0 && usage.read_count >= 3 {
        score -= 20.0;
    }
    if impact_reads == 0 && usage.read_count >= 10 {
        score -= 8.0;
    }
    if usage.semantic_eligible_read_count >= 3 {
        score -= ((1.0 - usage.semantic_eligible_result_rate) * 20.0).min(20.0);
    }
    score -= live.inferred_missing.min(5) as f64 * 4.0;
    score = score.clamp(0.0, 100.0);
    recommendations.sort();
    recommendations.dedup();
    issues.sort();
    issues.dedup();
    Ok(AgentAuditReport {
        version: 1,
        since_days,
        score,
        read_count: usage.read_count,
        brief_reads,
        impact_reads,
        evidence_reads,
        feedback_events,
        durable_writes,
        semantic_eligible_result_rate: usage.semantic_eligible_result_rate,
        inferred_missing: live.inferred_missing,
        commands,
        issues,
        recommendations,
    })
}

pub(crate) fn quality_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<QualityReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let fresh_cutoff = now_ms().saturating_sub(FRESH_MEMORY_GRACE_MS);
    let request_counts = memory_request_counts_since(conn, Some(since_ms))?;
    let feedback = memory_feedback_counts(conn, since_ms)?;
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let mut items = Vec::new();
    let mut suggestions = Vec::new();
    for memory in rows {
        let request_count = request_counts.get(&memory.id).copied().unwrap_or(0);
        let (positive_feedback, negative_feedback, _missing_feedback) =
            feedback.get(&memory.id).copied().unwrap_or((0, 0, 0));
        let links = get_links(conn, &memory.id)?.len();
        let body_chars = memory.body.chars().count();
        let fresh = memory.updated_at >= fresh_cutoff;
        let broad_history = quality_broad_history_task_state(&memory);
        let scored_request_count = if broad_history {
            request_count.min(3)
        } else {
            request_count
        };
        let mut usefulness_score = 20.0 + (scored_request_count.min(10) as f64 * 4.0);
        usefulness_score += positive_feedback.min(10) as f64 * 5.0;
        usefulness_score -= negative_feedback.min(10) as f64 * 6.0;
        usefulness_score += match memory.memory_type.as_str() {
            "decision" | "constraint" | "user_preference" | "product_goal" => 12.0,
            "known_issue" | "command" | "design_note" => 8.0,
            "task_state" => 4.0,
            _ => 2.0,
        };
        if memory.status == "uncertain" {
            usefulness_score -= 8.0;
        }
        let mut token_saving_score = if body_chars <= 600 {
            18.0
        } else if body_chars <= 1200 {
            10.0
        } else {
            -10.0
        };
        if request_count > 0 {
            token_saving_score += 8.0;
        }
        if links > 0 {
            token_saving_score += 6.0;
        }
        let mut risk_score = 5.0;
        if matches!(
            memory.memory_type.as_str(),
            "decision" | "constraint" | "user_preference" | "product_goal"
        ) {
            risk_score += 25.0;
        }
        if memory.status == "uncertain" {
            risk_score += 10.0;
        }
        if links == 0 {
            risk_score += 8.0;
        }
        if body_chars > 1200 {
            risk_score += 5.0;
        }
        if broad_history && request_count >= 8 && positive_feedback == 0 {
            risk_score += 18.0;
        }
        let mut reasons = Vec::new();
        if request_count > 0 {
            reasons.push(format!("used {request_count} time(s) recently"));
        } else if fresh {
            usefulness_score += 10.0;
            reasons.push("fresh; waiting for use".to_string());
        } else {
            reasons.push("unused recently".to_string());
            if !broad_history {
                suggestions.push(UsefulnessSuggestion {
                    action: "review_unused".to_string(),
                    id: Some(memory.id.clone()),
                    detail: "low quality score because no recent retrieval used this card"
                        .to_string(),
                });
            }
        }
        if links == 0 {
            reasons.push("no evidence links".to_string());
        }
        if body_chars > 1200 {
            reasons.push("large body increases token cost".to_string());
        }
        if broad_history {
            reasons.push("broad history card; frequent reads are capped".to_string());
        }
        if positive_feedback > 0 || negative_feedback > 0 {
            reasons.push(format!(
                "feedback +{positive_feedback} -{negative_feedback}"
            ));
        }
        let score = (usefulness_score + token_saving_score - risk_score).clamp(0.0, 100.0);
        items.push(MemoryQuality {
            id: memory.id,
            memory_type: memory.memory_type,
            title: memory.title,
            score,
            usefulness_score,
            token_saving_score,
            risk_score,
            request_count,
            positive_feedback,
            negative_feedback,
            body_chars,
            links,
            reasons,
        });
    }
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let strongest = items.iter().take(limit).cloned().collect::<Vec<_>>();
    let mut weakest = items.clone();
    weakest.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    weakest.truncate(limit);
    let average_score = if items.is_empty() {
        0.0
    } else {
        items.iter().map(|item| item.score).sum::<f64>() / items.len() as f64
    };
    Ok(QualityReport {
        version: 1,
        since_days,
        total: items.len(),
        average_score,
        strongest,
        weakest,
        items: items.into_iter().take(limit).collect(),
        suggestions,
    })
}

fn quality_broad_history_task_state(memory: &Memory) -> bool {
    if memory.memory_type != "task_state" {
        return false;
    }
    let title = memory.title.to_lowercase();
    let body = memory.body.to_lowercase();
    title.contains("autonomous compacted")
        || title.contains("compacted project")
        || title.contains("release history")
        || title.ends_with(" release")
        || title.ends_with(" released")
        || body.starts_with("autonomously compacted")
}

pub(crate) fn memory_feedback_counts(
    conn: &Connection,
    since_ms: i64,
) -> Result<HashMap<String, (usize, usize, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT detail FROM memory_events WHERE event_type = 'memory_feedback' AND created_at >= ?1",
    )?;
    let rows = stmt.query_map(params![since_ms], |row| row.get::<_, String>(0))?;
    let mut counts = HashMap::new();
    for row in rows {
        let detail = row?;
        let Ok(value) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        let rating = value.get("rating").and_then(Value::as_str).unwrap_or("");
        let ids = value
            .get("ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if ids.is_empty() && rating == "missing" {
            let entry = counts.entry("__missing__".to_string()).or_insert((0, 0, 0));
            entry.2 += 1;
            continue;
        }
        for id in ids
            .into_iter()
            .filter_map(|id| id.as_str().map(str::to_string))
        {
            let entry = counts.entry(id).or_insert((0, 0, 0));
            match rating {
                "useful" => entry.0 += 1,
                "useless" => entry.1 += 1,
                "missing" => entry.2 += 1,
                _ => {}
            }
        }
    }
    Ok(counts)
}

pub(crate) fn feedback_summary(conn: &Connection, since_days: i64) -> Result<FeedbackSummary> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let counts = memory_feedback_counts(conn, since_ms)?;
    let mut positive = 0;
    let mut negative = 0;
    let mut missing = 0;
    for (pos, neg, miss) in counts.values() {
        positive += *pos;
        negative += *neg;
        missing += *miss;
    }
    Ok(FeedbackSummary {
        since_days,
        positive,
        negative,
        missing,
        events: positive + negative + missing,
    })
}

fn missing_feedback_query_count(conn: &Connection, task: &str, since_days: i64) -> Result<usize> {
    let task_terms = tokenize(task);
    if task_terms.len() < 2 {
        return Ok(0);
    }
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let mut stmt = conn.prepare(
        "SELECT detail FROM memory_events WHERE event_type = 'memory_feedback' AND created_at >= ?1",
    )?;
    let rows = stmt
        .query_map(params![since_ms], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let required_overlap = task_terms.len().min(2);
    let mut count = 0;
    for detail in rows {
        let Ok(value) = serde_json::from_str::<Value>(&detail) else {
            continue;
        };
        if value.get("rating").and_then(Value::as_str) != Some("missing") {
            continue;
        }
        let has_ids = value
            .get("ids")
            .and_then(Value::as_array)
            .is_some_and(|ids| !ids.is_empty());
        if has_ids {
            continue;
        }
        let feedback_terms = value
            .get("query")
            .and_then(Value::as_str)
            .map(tokenize)
            .unwrap_or_default();
        let query = value.get("query").and_then(Value::as_str).unwrap_or("");
        if task_terms.intersection(&feedback_terms).count() >= required_overlap
            && !missing_feedback_query_resolved(conn, query)?
        {
            count += 1;
        }
    }
    Ok(count)
}

fn missing_feedback_query_resolved(conn: &Connection, query: &str) -> Result<bool> {
    if query.trim().is_empty() {
        return Ok(false);
    }
    Ok(!query_memories(
        conn,
        Some(query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        1,
    )?
    .is_empty())
}

pub(crate) fn print_feedback_report(
    conn: &Connection,
    ids: &[String],
    rating: FeedbackRating,
    command: &str,
    query: &str,
    note: &str,
    json_out: bool,
) -> Result<()> {
    let rating = match rating {
        FeedbackRating::Useful => "useful",
        FeedbackRating::Useless => "useless",
        FeedbackRating::Missing => "missing",
    };
    let detail = serde_json::to_string(&json!({
        "rating": rating,
        "ids": ids,
        "command": command,
        "query": query,
        "note": note,
    }))?;
    log_event(conn, "memory_feedback", None, &detail)?;
    let report = FeedbackReport {
        ok: true,
        rating: rating.to_string(),
        ids: ids.to_vec(),
        written_event: "memory_feedback".to_string(),
        summary: feedback_summary(conn, 30)?,
    };
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("feedback: {rating}");
        println!("ids: {}", ids.join(","));
    }
    Ok(())
}

pub(crate) fn print_budget_plan(
    conn: &Connection,
    task: &str,
    scope: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let plan = budget_plan(conn, task, scope)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("profile: {}", plan.profile);
        println!("max_chars: {}", plan.max_chars);
        for reason in plan.reasons {
            println!("- {reason}");
        }
    }
    Ok(())
}

pub(crate) fn budget_plan(
    conn: &Connection,
    task: &str,
    scope: Option<&str>,
) -> Result<BudgetPlan> {
    let terms = tokenize(task);
    let risky = task.to_lowercase();
    let broad = [
        "refactor",
        "migration",
        "schema",
        "release",
        "architecture",
        "autonomous",
        "security",
    ]
    .iter()
    .any(|needle| risky.contains(needle));
    let pending = list_inbox(conn, "pending", usize::MAX)?.len();
    let active_count = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        scope,
        500,
    )?
    .len();
    let missing_feedback = missing_feedback_query_count(conn, task, 30)?;
    let mut reasons = Vec::new();
    let mut profile = if broad || terms.len() > 14 {
        reasons.push("broad or risky task needs more doctrine and impact memory".to_string());
        BudgetProfile::Deep
    } else if pending > 20 || active_count > 80 || terms.len() > 8 {
        reasons.push("moderate project state or task complexity".to_string());
        BudgetProfile::Normal
    } else {
        reasons.push("small task should stay token-light".to_string());
        BudgetProfile::Tiny
    };
    if missing_feedback > 0 && matches!(profile, BudgetProfile::Tiny) {
        profile = BudgetProfile::Normal;
        reasons.push(format!(
            "{missing_feedback} recent missing feedback event(s) for a similar task; use the next smallest budget"
        ));
    }
    if pending > 0 {
        reasons.push(format!(
            "{pending} pending inbox item(s) may affect context freshness"
        ));
    }
    let profile_name = match profile {
        BudgetProfile::Tiny => "tiny",
        BudgetProfile::Normal => "normal",
        BudgetProfile::Deep => "deep",
    };
    Ok(BudgetPlan {
        task: task.to_string(),
        profile: profile_name.to_string(),
        max_chars: budget_profile_chars(Some(profile)).unwrap_or(1200),
        include_recent: match profile {
            BudgetProfile::Tiny => 3,
            BudgetProfile::Normal => 6,
            BudgetProfile::Deep => 12,
        },
        semantic: true,
        reasons,
    })
}

pub(crate) fn print_project_profile(conn: &Connection, root: &Path, json_out: bool) -> Result<()> {
    let profile = project_profile_snapshot(conn, root, "project")?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        println!("root: {}", profile.root);
        println!(
            "active_profile: {}",
            profile.active_profile.as_deref().unwrap_or("-")
        );
        println!("memory_count: {}", profile.memory_count);
        println!("pending_inbox: {}", profile.pending_inbox);
        println!("recommended_budget: {}", profile.recommended_budget);
        println!(
            "embeddings: {} {} {}",
            profile.embedding_provider, profile.embedding_endpoint, profile.embedding_model
        );
    }
    Ok(())
}

pub(crate) fn project_profile_snapshot(
    conn: &Connection,
    root: &Path,
    scope: &str,
) -> Result<ProjectProfileSnapshot> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let active_profile = fs::read_to_string(root.join(".agent/active_profile"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let count_type = |memory_type: &str| -> Result<usize> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE type = ?1 AND status IN ('active','uncertain')",
            params![memory_type],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as usize)
    };
    let memory_count = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        Some(scope),
        usize::MAX,
    )?
    .len();
    let (provider, endpoint, model) = read_project_embedding_config(&root);
    let budget = budget_plan(conn, "routine project memory task", Some(scope))?;
    Ok(ProjectProfileSnapshot {
        root: root.display().to_string(),
        active_profile,
        memory_count,
        pending_inbox: list_inbox(conn, "pending", usize::MAX)?.len(),
        decisions: count_type("decision")?,
        constraints: count_type("constraint")?,
        commands: count_type("command")?,
        known_issues: count_type("known_issue")?,
        embedding_provider: provider,
        embedding_endpoint: endpoint,
        embedding_model: model,
        recommended_budget: budget.profile,
    })
}

pub(crate) fn read_project_embedding_config(root: &Path) -> (String, String, String) {
    let default = (
        DEFAULT_EMBED_PROVIDER.to_string(),
        DEFAULT_EMBED_ENDPOINT.to_string(),
        DEFAULT_EMBED_MODEL.to_string(),
    );
    let Ok(raw) = fs::read_to_string(root.join(".agent/config.toml")) else {
        return default;
    };
    let Ok(value) = raw.parse::<toml::Value>() else {
        return default;
    };
    let Some(embeddings) = value.get("embeddings") else {
        return default;
    };
    (
        embeddings
            .get("provider")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_PROVIDER)
            .to_string(),
        embeddings
            .get("endpoint")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_ENDPOINT)
            .to_string(),
        embeddings
            .get("model")
            .and_then(toml::Value::as_str)
            .unwrap_or(DEFAULT_EMBED_MODEL)
            .to_string(),
    )
}

pub(crate) fn app_project_root_for_db(db: &Path) -> Option<PathBuf> {
    let db = app_canonical_or_absolute(db);
    let agent_dir = db.parent()?;
    if agent_dir.file_name()?.to_str()? != ".agent" {
        return None;
    }
    agent_dir.parent().map(Path::to_path_buf)
}

pub(crate) fn app_push_unique_db(dbs: &mut Vec<PathBuf>, db: &Path) {
    let key = app_canonical_or_absolute(db);
    if !dbs
        .iter()
        .any(|existing| app_canonical_or_absolute(existing) == key)
    {
        dbs.push(db.to_path_buf());
    }
}

pub(crate) fn app_canonical_or_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

pub(crate) fn app_project_counts(db: &Path) -> Result<(i64, i64)> {
    let conn = open_db(db)?;
    let memories = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
    let pending = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    Ok((memories, pending))
}

pub(crate) fn print_dashboard(default_db: &Path, json_out: bool) -> Result<()> {
    let report = dashboard_report(default_db)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("dukememory. Dashboard");
        println!(
            "summary: status={} total={} ready={} attention={} stale={} missing_live_eval={} memory_gap_projects={} memory_gap_count={} semantic_empty_gap_projects={} semantic_empty_gap_count={} semantic_empty_projects={} semantic_empty_reads={} semantic_result_warn_projects={} gap_inbox_pending_projects={} gap_inbox_pending_count={} gap_inbox_stale_projects={} gap_inbox_stale_count={} gap_inbox_oldest_age={} recommendations={} reason_types={} repair_actions={} safe_repair_actions={} repair_loop_projects={} repair_loop_failed={} repair_loop_safe_skipped={} daemon_embedding_skipped={} daemon_embedding_repaired={}",
            report.status,
            report.total_projects,
            report.ready_projects,
            report.attention_projects,
            report.stale_projects,
            report.missing_live_eval_projects,
            report.memory_gap_projects,
            report.memory_gap_count,
            report.semantic_empty_gap_projects,
            report.semantic_empty_gap_count,
            report.semantic_empty_projects,
            report.semantic_empty_read_count,
            report.semantic_result_warn_projects,
            report.gap_inbox_pending_projects,
            report.gap_inbox_pending_count,
            report.gap_inbox_stale_projects,
            report.gap_inbox_stale_count,
            format_optional_secs(report.gap_inbox_oldest_pending_age_secs),
            report.recommendations_count,
            report.attention_reason_counts.len(),
            report.repair_actions_count,
            report.safe_repair_actions_count,
            report.repair_loop_projects,
            report.repair_loop_failed_projects,
            report.repair_loop_safe_skipped_projects,
            report.daemon_embedding_skipped_projects,
            report.daemon_embedding_repaired_projects
        );
        for project in report.projects {
            let reasons = if project.attention_reasons.is_empty() {
                "-".to_string()
            } else {
                project.attention_reasons.join(",")
            };
            let repairs = if project.repair_actions.is_empty() {
                "-".to_string()
            } else {
                project
                    .repair_actions
                    .iter()
                    .map(|action| action.code.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            println!(
                "- {} status={} attention={} reasons={} repairs={} repair_runs={} repair_failed={} repair_safe_skipped={} daemon_embedding_skipped={} daemon_embedding_repaired_at={} gap_inbox_pending={} gap_inbox_stale={} gap_inbox_oldest_age={} memories={} pending={} quality={} autonomous={} auto_age={} auto_fresh={} live_reads={} live_useful={} live_gaps={} semantic_empty_gaps={} semantic_results={} semantic_empty={} recommendations={}",
                project.name,
                project.status,
                project.attention,
                reasons,
                repairs,
                project.repair_loop.runs,
                project.repair_loop.failed_actions,
                project.repair_loop.safe_skipped_actions,
                project.daemon_embedding_skipped.unwrap_or(false),
                project
                    .daemon_embedding_repaired_at
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project.gap_inbox.pending,
                project.gap_inbox.stale_pending,
                format_optional_secs(project.gap_inbox.oldest_pending_age_secs),
                project.memories,
                project.pending_inbox,
                project
                    .quality_average
                    .map(|value| format!("{value:.1}"))
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_ok
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_age_secs
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_fresh
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_live_reads
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_useful_rate
                    .map(|value| format!("{:.0}%", value * 100.0))
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_inferred_missing
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .autonomous_semantic_empty_missing
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .semantic_eligible_result_rate
                    .map(|value| format!("{:.0}%", value * 100.0))
                    .unwrap_or_else(|| "-".to_string()),
                project
                    .semantic_eligible_empty_read_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                project.recommendations.len()
            );
        }
    }
    Ok(())
}

pub(crate) fn print_dashboard_repair(
    default_db: &Path,
    apply: bool,
    project_filter: Option<&str>,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = dashboard_repair_report(
        default_db,
        apply,
        project_filter,
        provider,
        endpoint,
        model,
        "cli",
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("dukememory. Dashboard Repair");
        println!(
            "summary: apply={} ok={} total={} safe={} applied={} skipped={} failed={}",
            report.apply,
            report.ok,
            report.total_actions,
            report.safe_actions,
            report.applied_actions,
            report.skipped_actions,
            report.failed_actions
        );
        for project in report.projects {
            for action in project.actions {
                println!(
                    "- {} priority={} gap_inbox_stale={} gap_inbox_oldest_age={} action={} reason={} safe={} applied={} skipped={} ok={} detail={}",
                    project.name,
                    project.priority,
                    project.gap_inbox_stale_pending,
                    format_optional_secs(project.gap_inbox_oldest_pending_age_secs),
                    action.code,
                    action.reason,
                    action.safe_auto,
                    action.applied,
                    action.skipped,
                    action.ok,
                    action.detail
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn print_dashboard_repair_history(
    default_db: &Path,
    since_days: i64,
    limit: usize,
    project_filter: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let report = dashboard_repair_history_report(default_db, since_days, limit, project_filter)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("dukememory. Dashboard Repair History");
        println!(
            "summary: runs={} applied={} skipped={} failed={} safe={}",
            report.total_runs,
            report.applied_actions,
            report.skipped_actions,
            report.failed_actions,
            report.safe_actions
        );
        for project in report.projects {
            println!(
                "- {} runs={} applied={} skipped={} failed={} safe={}",
                project.name,
                project.total_runs,
                project.applied_actions,
                project.skipped_actions,
                project.failed_actions,
                project.safe_actions
            );
            for event in project.recent {
                println!(
                    "  event={} source={} applied={} skipped={} failed={} actions={}",
                    event.id,
                    event.source,
                    event.applied_actions,
                    event.skipped_actions,
                    event.failed_actions,
                    event.total_actions
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn dashboard_repair_history_report(
    default_db: &Path,
    since_days: i64,
    limit: usize,
    project_filter: Option<&str>,
) -> Result<DashboardRepairHistoryReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let mut projects = Vec::new();
    let mut runs_by_source = BTreeMap::new();
    let mut actions_by_code = BTreeMap::new();
    let mut manual_actions_by_code = BTreeMap::new();
    for db in discover_project_dbs(default_db)? {
        let root = app_project_root_for_db(&db).unwrap_or_else(|| {
            db.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        let item = ProjectDashboardItem {
            name: name.clone(),
            status: String::new(),
            attention: false,
            root: root.display().to_string(),
            db: db.display().to_string(),
            memories: 0,
            pending_inbox: 0,
            quality_average: None,
            autonomous_ok: None,
            autonomous_age_secs: None,
            autonomous_fresh: None,
            autonomous_live_reads: None,
            autonomous_useful_rate: None,
            autonomous_useful_rate_source: None,
            autonomous_inferred_missing: None,
            autonomous_semantic_empty_missing: None,
            autonomous_semantic_empty_missing_queries: Vec::new(),
            daemon_embedding_skipped: None,
            daemon_embedding_error: None,
            daemon_embedding_repaired_at: None,
            daemon_embedding_repair_source: None,
            embedding_missing: None,
            embedding_provider_reachable: None,
            embedding_provider_health_ms: None,
            embedding_provider_error: None,
            semantic_read_rate: None,
            semantic_result_rate: None,
            semantic_empty_read_count: None,
            semantic_avg_results: None,
            semantic_eligible_result_rate: None,
            semantic_eligible_empty_read_count: None,
            semantic_empty_queries: Vec::new(),
            recommended_budget: None,
            write_pressure: None,
            top_memories: Vec::new(),
            repair_loop: empty_repair_loop_status(),
            gap_inbox: DashboardGapInboxStatus::default(),
            attention_reasons: Vec::new(),
            repair_actions: Vec::new(),
            recommendations: Vec::new(),
        };
        if !dashboard_project_matches(&item, project_filter) {
            continue;
        }
        let conn = open_db(&db)?;
        let recent = dashboard_repair_events(&conn, since_ms, limit)?;
        if recent.is_empty() {
            continue;
        }
        let total_runs = recent.len();
        let applied_actions = recent.iter().map(|event| event.applied_actions).sum();
        let skipped_actions = recent.iter().map(|event| event.skipped_actions).sum();
        let failed_actions = recent.iter().map(|event| event.failed_actions).sum();
        let safe_actions = recent.iter().map(|event| event.safe_actions).sum();
        for event in &recent {
            *runs_by_source.entry(event.source.clone()).or_insert(0) += 1;
            for action in &event.actions {
                *actions_by_code.entry(action.code.clone()).or_insert(0) += 1;
                if !action.safe_auto {
                    *manual_actions_by_code
                        .entry(action.code.clone())
                        .or_insert(0) += 1;
                }
            }
        }
        projects.push(DashboardRepairHistoryProject {
            name,
            root: root.display().to_string(),
            db: db.display().to_string(),
            total_runs,
            applied_actions,
            skipped_actions,
            failed_actions,
            safe_actions,
            recent,
        });
    }
    let total_runs = projects.iter().map(|project| project.total_runs).sum();
    let applied_actions = projects.iter().map(|project| project.applied_actions).sum();
    let skipped_actions = projects.iter().map(|project| project.skipped_actions).sum();
    let failed_actions = projects.iter().map(|project| project.failed_actions).sum();
    let safe_actions = projects.iter().map(|project| project.safe_actions).sum();
    Ok(DashboardRepairHistoryReport {
        version: 1,
        since_days,
        total_runs,
        applied_actions,
        skipped_actions,
        failed_actions,
        safe_actions,
        runs_by_source,
        actions_by_code,
        manual_actions_by_code,
        projects,
    })
}

fn dashboard_repair_events(
    conn: &Connection,
    since_ms: i64,
    limit: usize,
) -> Result<Vec<DashboardRepairHistoryEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, detail, created_at FROM memory_events WHERE event_type = 'dashboard_repair' AND created_at >= ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(
        params![since_ms, limit.min(i64::MAX as usize) as i64],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )?;
    let mut events = Vec::new();
    for row in rows {
        let (id, detail, created_at) = row?;
        let value = serde_json::from_str::<Value>(&detail).unwrap_or_else(|_| json!({}));
        let actions = value
            .get("actions")
            .and_then(Value::as_array)
            .map(|actions| {
                actions
                    .iter()
                    .filter_map(|action| serde_json::from_value(action.clone()).ok())
                    .collect::<Vec<DashboardRepairResult>>()
            })
            .unwrap_or_default();
        let safe_actions = value
            .get("safe_actions")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| actions.iter().filter(|action| action.safe_auto).count());
        let applied_actions = value
            .get("applied_actions")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| actions.iter().filter(|action| action.applied).count());
        let skipped_actions = value
            .get("skipped_actions")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| actions.iter().filter(|action| action.skipped).count());
        let failed_actions = value
            .get("failed_actions")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or_else(|| actions.iter().filter(|action| !action.ok).count());
        events.push(DashboardRepairHistoryEvent {
            id,
            created_at,
            source: value
                .get("source")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            total_actions: value
                .get("total_actions")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(actions.len()),
            applied_actions,
            skipped_actions,
            failed_actions,
            safe_actions,
            actions,
        });
    }
    Ok(events)
}

pub(crate) fn dashboard_repair_report(
    default_db: &Path,
    apply: bool,
    project_filter: Option<&str>,
    provider: &str,
    endpoint: &str,
    model: &str,
    source: &str,
) -> Result<DashboardRepairReport> {
    let dashboard = dashboard_report(default_db)?;
    let mut projects = Vec::new();
    let mut dashboard_projects = dashboard.projects;
    dashboard_projects.sort_by(|left, right| {
        dashboard_repair_priority(right)
            .cmp(&dashboard_repair_priority(left))
            .then_with(|| left.name.cmp(&right.name))
    });
    for project in dashboard_projects {
        if !dashboard_project_matches(&project, project_filter) {
            continue;
        }
        let priority = dashboard_repair_priority(&project);
        let gap_inbox_stale_pending = project.gap_inbox.stale_pending;
        let gap_inbox_oldest_pending_age_secs = project.gap_inbox.oldest_pending_age_secs;
        let mut actions = Vec::new();
        for action in &project.repair_actions {
            actions.push(run_dashboard_repair_action(
                &project, action, apply, provider, endpoint, model,
            ));
        }
        if !actions.is_empty() {
            if apply {
                log_dashboard_repair_project(&project, &actions, source)?;
            }
            projects.push(DashboardRepairProject {
                name: project.name,
                root: project.root,
                db: project.db,
                priority,
                gap_inbox_stale_pending,
                gap_inbox_oldest_pending_age_secs,
                actions,
            });
        }
    }
    let total_actions = projects
        .iter()
        .map(|project| project.actions.len())
        .sum::<usize>();
    let safe_actions = projects
        .iter()
        .flat_map(|project| project.actions.iter())
        .filter(|action| action.safe_auto)
        .count();
    let applied_actions = projects
        .iter()
        .flat_map(|project| project.actions.iter())
        .filter(|action| action.applied)
        .count();
    let skipped_actions = projects
        .iter()
        .flat_map(|project| project.actions.iter())
        .filter(|action| action.skipped)
        .count();
    let failed_actions = projects
        .iter()
        .flat_map(|project| project.actions.iter())
        .filter(|action| !action.ok)
        .count();
    Ok(DashboardRepairReport {
        version: 1,
        apply,
        ok: failed_actions == 0,
        total_actions,
        safe_actions,
        applied_actions,
        skipped_actions,
        failed_actions,
        projects,
    })
}

fn dashboard_repair_priority(project: &ProjectDashboardItem) -> i64 {
    let safe_actions = project
        .repair_actions
        .iter()
        .filter(|action| action.safe_auto)
        .count() as i64;
    let oldest_minutes = project
        .gap_inbox
        .oldest_pending_age_secs
        .unwrap_or_default()
        .saturating_div(60);
    (project.gap_inbox.stale_pending as i64)
        .saturating_mul(100_000)
        .saturating_add(oldest_minutes)
        .saturating_add(safe_actions.saturating_mul(10))
}

fn log_dashboard_repair_project(
    project: &ProjectDashboardItem,
    actions: &[DashboardRepairResult],
    source: &str,
) -> Result<()> {
    let conn = open_db(Path::new(&project.db))?;
    let detail = serde_json::to_string(&json!({
        "version": 1,
        "source": source,
        "project": project.name,
        "root": project.root,
        "priority": dashboard_repair_priority(project),
        "gap_inbox_stale_pending": project.gap_inbox.stale_pending,
        "gap_inbox_oldest_pending_age_secs": project.gap_inbox.oldest_pending_age_secs,
        "total_actions": actions.len(),
        "safe_actions": actions.iter().filter(|action| action.safe_auto).count(),
        "applied_actions": actions.iter().filter(|action| action.applied).count(),
        "skipped_actions": actions.iter().filter(|action| action.skipped).count(),
        "failed_actions": actions.iter().filter(|action| !action.ok).count(),
        "actions": actions,
    }))?;
    log_event(&conn, "dashboard_repair", None, &detail)
}

fn dashboard_project_matches(project: &ProjectDashboardItem, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    project.name == filter || project.root == filter || project.db == filter
}

fn run_dashboard_repair_action(
    project: &ProjectDashboardItem,
    action: &DashboardRepairAction,
    apply: bool,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> DashboardRepairResult {
    if !action.safe_auto {
        return DashboardRepairResult {
            code: action.code.clone(),
            reason: action.reason.clone(),
            safe_auto: false,
            applied: false,
            skipped: true,
            ok: true,
            detail: "manual action skipped".to_string(),
            command: action.command.clone(),
        };
    }
    if !apply {
        return DashboardRepairResult {
            code: action.code.clone(),
            reason: action.reason.clone(),
            safe_auto: true,
            applied: false,
            skipped: true,
            ok: true,
            detail: "dry run".to_string(),
            command: action.command.clone(),
        };
    }
    let root = PathBuf::from(&project.root);
    let db = PathBuf::from(&project.db);
    let result = match action.code.as_str() {
        "run_autonomous" => run_dashboard_autonomous_repair(&root, &db, provider, endpoint, model),
        "embed_index" => run_dashboard_embed_repair(&root, &db, provider, endpoint, model),
        "daemon_embed_index" => {
            run_dashboard_daemon_embed_repair(&root, &db, provider, endpoint, model)
        }
        other => Err(anyhow::anyhow!("unknown safe repair action: {other}")),
    };
    match result {
        Ok(detail) => DashboardRepairResult {
            code: action.code.clone(),
            reason: action.reason.clone(),
            safe_auto: true,
            applied: true,
            skipped: false,
            ok: true,
            detail,
            command: action.command.clone(),
        },
        Err(err) => DashboardRepairResult {
            code: action.code.clone(),
            reason: action.reason.clone(),
            safe_auto: true,
            applied: false,
            skipped: false,
            ok: false,
            detail: format!("{err:#}"),
            command: action.command.clone(),
        },
    }
}

fn run_dashboard_autonomous_repair(
    root: &Path,
    db: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    let conn = open_db(db)?;
    let (provider, endpoint, model) =
        project_embedding_or_fallback(root, provider, endpoint, model);
    let status_file = root.join(".agent/autonomous-status.json");
    let rollback_dir = root.join(".agent/autonomous-rollbacks");
    let backup_dir = root.join(".agent/backups");
    let report = autonomous_run_once(
        &conn,
        AutonomousRunRequest {
            level: AutonomousLevel::Normal,
            status_file: &status_file,
            rollback_dir: &rollback_dir,
            backup_dir: &backup_dir,
            backup_keep: 10,
            rollback_keep: 10,
            db,
            scope: "project",
            provider: &provider,
            endpoint: &endpoint,
            model: &model,
        },
    )?;
    Ok(compact_autonomous_repair_detail(&report))
}

fn compact_autonomous_repair_detail(report: &AutonomousReport) -> String {
    let mut parts = vec![format!("ok={} actions={}", report.ok, report.actions.len())];
    for kind in [
        "inferred_feedback",
        "gap_inbox",
        "gap_inbox_resolved",
        "live_eval_snapshot",
    ] {
        if let Some(action) = report.actions.iter().find(|action| action.kind == kind) {
            parts.push(format!("{kind}:{}:{}", action.status, action.detail));
        }
    }
    parts.join(" | ")
}

fn run_dashboard_embed_repair(
    root: &Path,
    db: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    let conn = open_db(db)?;
    let (provider, endpoint, model) =
        project_embedding_or_fallback(root, provider, endpoint, model);
    let report = embeddings::embed_index(&conn, &provider, &endpoint, &model, &[], None, false)?;
    Ok(format!(
        "indexed={} skipped={}",
        report.indexed, report.skipped
    ))
}

fn run_dashboard_daemon_embed_repair(
    root: &Path,
    db: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<String> {
    let detail = run_dashboard_embed_repair(root, db, provider, endpoint, model)?;
    clear_daemon_embedding_skip(root)?;
    Ok(format!("{detail} | daemon_status=cleared"))
}

fn clear_daemon_embedding_skip(root: &Path) -> Result<()> {
    let path = root.join(".agent/daemon-status.json");
    let raw = fs::read_to_string(&path)?;
    let mut value = serde_json::from_str::<Value>(&raw)?;
    let Some(object) = value.as_object_mut() else {
        return Err(anyhow::anyhow!("daemon status is not a JSON object"));
    };
    object.insert("embedding_skipped".to_string(), Value::Bool(false));
    object.insert("embedding_error".to_string(), Value::Null);
    object.insert("embedding_repaired_at".to_string(), json!(now_ms()));
    object.insert(
        "embedding_repair_source".to_string(),
        Value::String("dashboard_repair".to_string()),
    );
    fs::write(&path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

fn project_embedding_or_fallback(
    root: &Path,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> (String, String, String) {
    let (project_provider, project_endpoint, project_model) = read_project_embedding_config(root);
    (
        if project_provider == DEFAULT_EMBED_PROVIDER {
            provider.to_string()
        } else {
            project_provider
        },
        if project_endpoint == DEFAULT_EMBED_ENDPOINT {
            endpoint.to_string()
        } else {
            project_endpoint
        },
        if project_model == DEFAULT_EMBED_MODEL {
            model.to_string()
        } else {
            project_model
        },
    )
}

fn push_repair_action(
    actions: &mut Vec<DashboardRepairAction>,
    code: &str,
    reason: &str,
    safe_auto: bool,
    description: &str,
    command: Vec<String>,
) {
    if actions.iter().any(|action| action.code == code) {
        return;
    }
    actions.push(DashboardRepairAction {
        code: code.to_string(),
        reason: reason.to_string(),
        safe_auto,
        description: description.to_string(),
        command,
    });
}

fn autonomous_repair_command(root: &Path, db: &Path) -> Vec<String> {
    vec![
        "dukememory".to_string(),
        "--db".to_string(),
        db.display().to_string(),
        "autonomous".to_string(),
        "run-once".to_string(),
        "--level".to_string(),
        "normal".to_string(),
        "--status-file".to_string(),
        root.join(".agent/autonomous-status.json")
            .display()
            .to_string(),
        "--rollback-dir".to_string(),
        root.join(".agent/autonomous-rollbacks")
            .display()
            .to_string(),
        "--backup-dir".to_string(),
        root.join(".agent/backups").display().to_string(),
    ]
}

fn embed_repair_command(db: &Path) -> Vec<String> {
    vec![
        "dukememory".to_string(),
        "--db".to_string(),
        db.display().to_string(),
        "embed-index".to_string(),
    ]
}

fn inbox_review_command(db: &Path) -> Vec<String> {
    vec![
        "dukememory".to_string(),
        "--db".to_string(),
        db.display().to_string(),
        "inbox-v2".to_string(),
        "report".to_string(),
        "--json".to_string(),
    ]
}

fn freshest_dashboard_live_eval(
    status_live_eval: Option<LiveEvalReport>,
    current_live_eval: Option<LiveEvalReport>,
) -> Option<LiveEvalReport> {
    match (status_live_eval, current_live_eval) {
        (Some(status), Some(current)) if current.reads >= status.reads => Some(current),
        (Some(status), _) => Some(status),
        (None, current) => current,
    }
}

pub(crate) fn dashboard_report(default_db: &Path) -> Result<DashboardReport> {
    let projects = discover_project_dbs(default_db)?
        .into_iter()
        .filter_map(|db| {
            let root = app_project_root_for_db(&db).unwrap_or_else(|| {
                db.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
            let conn = open_db(&db).ok()?;
            let profile = project_profile_snapshot(&conn, &root, "project").ok();
            let quality = quality_report(&conn, 30, 10).ok();
            let usage = usage_report(&conn, 7, 10).ok();
            let autonomous =
                read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
            let daemon_embedding = daemon_embedding_snapshot(&root);
            let autonomous_age_secs = autonomous
                .as_ref()
                .map(|status| ((now_ms() - status.updated_at).max(0)) / 1000);
            let autonomous_fresh = autonomous_age_secs.map(|age| age <= 86_400);
            let status_live_eval = autonomous
                .as_ref()
                .and_then(|status| status.live_eval.clone());
            let current_live_eval = live_eval_report(&conn, 7).ok();
            let live_eval = freshest_dashboard_live_eval(status_live_eval, current_live_eval);
            let embedding = embeddings::embed_status(
                &conn,
                DEFAULT_EMBED_PROVIDER,
                DEFAULT_EMBED_ENDPOINT,
                DEFAULT_EMBED_MODEL,
            )
            .ok();
            let (memories, pending_inbox) = app_project_counts(&db).unwrap_or((0, 0));
            let embedding_missing = embedding.as_ref().map(|status| status.missing);
            let repair_loop =
                ops_repair_loop_status(&conn, 30).unwrap_or_else(|_| empty_repair_loop_status());
            let gap_inbox = dashboard_gap_inbox_status(&conn).unwrap_or_default();
            let mut recommendations = Vec::new();
            let mut attention_reasons = Vec::new();
            let mut repair_actions = Vec::new();
            match &autonomous {
                None => {
                    attention_reasons.push("autonomous_status_missing".to_string());
                    recommendations.push(
                        "run dukememory autonomous run-once --level normal to create project status"
                            .to_string(),
                    );
                    push_repair_action(
                        &mut repair_actions,
                        "run_autonomous",
                        "autonomous_status_missing",
                        true,
                        "Create autonomous project status.",
                        autonomous_repair_command(&root, &db),
                    );
                }
                Some(status) if !status.ok => {
                    attention_reasons.push("autonomous_status_warn".to_string());
                    recommendations
                        .push("inspect dukememory autonomous status for warnings".to_string());
                    push_repair_action(
                        &mut repair_actions,
                        "run_autonomous",
                        "autonomous_status_warn",
                        true,
                        "Refresh autonomous maintenance status.",
                        autonomous_repair_command(&root, &db),
                    );
                }
                Some(_) => {}
            }
            if autonomous_fresh == Some(false) {
                attention_reasons.push("autonomous_status_stale".to_string());
                recommendations.push(
                    "run dukememory autonomous run-once --level normal to refresh status"
                        .to_string(),
                );
                push_repair_action(
                    &mut repair_actions,
                    "run_autonomous",
                    "autonomous_status_stale",
                    true,
                    "Refresh stale autonomous project status.",
                    autonomous_repair_command(&root, &db),
                );
            }
            if live_eval.is_none() {
                attention_reasons.push("live_eval_missing".to_string());
                recommendations.push(
                    "run dukememory autonomous run-once --level normal to record live eval"
                        .to_string(),
                );
                push_repair_action(
                    &mut repair_actions,
                    "run_autonomous",
                    "live_eval_missing",
                    true,
                    "Record live memory usefulness signals.",
                    autonomous_repair_command(&root, &db),
                );
            }
            let active_memory_gaps = active_dashboard_memory_gap_count(live_eval.as_ref(), &gap_inbox);
            if active_memory_gaps > 0 {
                attention_reasons.push("memory_gaps_detected".to_string());
                recommendations.push(
                    "run dukememory autonomous run-once --level normal to materialize memory gaps"
                        .to_string(),
                );
                push_repair_action(
                    &mut repair_actions,
                    "run_autonomous",
                    "memory_gaps_detected",
                    true,
                    "Materialize inferred memory gaps into reviewable inbox suggestions.",
                    autonomous_repair_command(&root, &db),
                );
            }
            if gap_inbox
                .stale_pending
                > 0
            {
                attention_reasons.push("gap_inbox_stale".to_string());
                recommendations.push(format!(
                    "run dukememory autonomous run-once --level normal to refresh {} stale gap inbox item(s)",
                    gap_inbox.stale_pending
                ));
                push_repair_action(
                    &mut repair_actions,
                    "run_autonomous",
                    "gap_inbox_stale",
                    true,
                    "Refresh stale autonomous gap inbox suggestions.",
                    autonomous_repair_command(&root, &db),
                );
            }
            if embedding_missing.unwrap_or(0) > 0 {
                attention_reasons.push("embeddings_missing".to_string());
                recommendations.push("run dukememory embed-index".to_string());
                push_repair_action(
                    &mut repair_actions,
                    "embed_index",
                    "embeddings_missing",
                    true,
                    "Refresh missing memory embeddings.",
                    embed_repair_command(&db),
                );
            }
            if usage
                .as_ref()
                .is_some_and(|usage| {
                    usage.semantic_eligible_read_count >= 3
                        && usage.semantic_eligible_result_rate < 0.75
                })
            {
                attention_reasons.push("semantic_empty_results".to_string());
                recommendations.push(
                    "inspect usage-report semantic empty reads, then refresh embeddings"
                        .to_string(),
                );
                push_repair_action(
                    &mut repair_actions,
                    "embed_index",
                    "semantic_empty_results",
                    true,
                    "Refresh embeddings after repeated empty semantic reads.",
                    embed_repair_command(&db),
                );
            }
            if pending_inbox > 0 {
                attention_reasons.push("pending_inbox".to_string());
                recommendations.push("review pending memory inbox".to_string());
                push_repair_action(
                    &mut repair_actions,
                    "review_inbox",
                    "pending_inbox",
                    false,
                    "Review pending inbox suggestions before accepting them.",
                    inbox_review_command(&db),
                );
            }
            if repair_loop.failed_actions > 0 {
                attention_reasons.push("repair_loop_failed".to_string());
                recommendations
                    .push("inspect dashboard repair history for failed actions".to_string());
            }
            if repair_loop.safe_skipped_actions > 0 {
                attention_reasons.push("repair_loop_safe_skipped".to_string());
                recommendations
                    .push("run dukememory dashboard-repair --apply for safe repairs".to_string());
            }
            if daemon_embedding.skipped == Some(true) {
                attention_reasons.push("daemon_embedding_skipped".to_string());
                recommendations.push(
                    "run dukememory dashboard-repair --apply after checking embedding provider"
                        .to_string(),
                );
                push_repair_action(
                    &mut repair_actions,
                    "daemon_embed_index",
                    "daemon_embedding_skipped",
                    true,
                    "Refresh embeddings after daemon skipped embedding maintenance.",
                    embed_repair_command(&db),
                );
            }
            let attention = !attention_reasons.is_empty() || !recommendations.is_empty();
            let status = if attention { "attention" } else { "ready" }.to_string();
            Some(ProjectDashboardItem {
                name: root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("project")
                    .to_string(),
                status,
                attention,
                root: root.display().to_string(),
                db: db.display().to_string(),
                memories,
                pending_inbox,
                quality_average: quality.map(|quality| quality.average_score),
                autonomous_ok: autonomous.as_ref().map(|status| status.ok),
                autonomous_age_secs,
                autonomous_fresh,
                autonomous_live_reads: live_eval.as_ref().map(|live| live.reads),
                autonomous_useful_rate: live_eval.as_ref().map(|live| live.useful_rate),
                autonomous_useful_rate_source: live_eval
                    .as_ref()
                    .map(|live| live.useful_rate_source.clone()),
                autonomous_inferred_missing: live_eval.as_ref().map(|live| live.inferred_missing),
                autonomous_semantic_empty_missing: live_eval
                    .as_ref()
                    .map(|live| live.semantic_empty_missing),
                autonomous_semantic_empty_missing_queries: live_eval
                    .as_ref()
                    .map(|live| live.semantic_empty_missing_queries.clone())
                    .unwrap_or_default(),
                daemon_embedding_skipped: daemon_embedding.skipped,
                daemon_embedding_error: daemon_embedding.error,
                daemon_embedding_repaired_at: daemon_embedding.repaired_at,
                daemon_embedding_repair_source: daemon_embedding.repair_source,
                embedding_missing,
                embedding_provider_reachable: embedding
                    .as_ref()
                    .map(|status| status.provider_reachable),
                embedding_provider_health_ms: embedding
                    .as_ref()
                    .and_then(|status| status.provider_health_ms),
                embedding_provider_error: embedding
                    .as_ref()
                    .and_then(|status| status.provider_error.clone()),
                semantic_read_rate: usage.as_ref().map(|usage| usage.semantic_eligible_read_rate),
                semantic_result_rate: usage.as_ref().map(|usage| usage.semantic_result_rate),
                semantic_empty_read_count: usage
                    .as_ref()
                    .map(|usage| usage.semantic_empty_read_count),
                semantic_avg_results: usage.as_ref().map(|usage| usage.semantic_avg_results),
                semantic_eligible_result_rate: usage
                    .as_ref()
                    .map(|usage| usage.semantic_eligible_result_rate),
                semantic_eligible_empty_read_count: usage
                    .as_ref()
                    .map(|usage| usage.semantic_eligible_empty_read_count),
                semantic_empty_queries: usage
                    .as_ref()
                    .map(|usage| usage.semantic_empty_queries.clone())
                    .unwrap_or_default(),
                recommended_budget: profile.map(|profile| profile.recommended_budget),
                write_pressure: usage.as_ref().map(|usage| usage.write_pressure),
                top_memories: usage
                    .as_ref()
                    .map(|usage| usage.top_memories.clone())
                    .unwrap_or_default(),
                repair_loop,
                gap_inbox,
                attention_reasons,
                repair_actions,
                recommendations,
            })
        })
        .collect::<Vec<_>>();
    let total_projects = projects.len();
    let ready_projects = projects
        .iter()
        .filter(|project| {
            project.autonomous_ok == Some(true)
                && project.autonomous_fresh != Some(false)
                && project.embedding_missing.unwrap_or(0) == 0
                && project.pending_inbox == 0
                && project.recommendations.is_empty()
        })
        .count();
    let stale_projects = projects
        .iter()
        .filter(|project| project.autonomous_fresh == Some(false))
        .count();
    let missing_live_eval_projects = projects
        .iter()
        .filter(|project| project.autonomous_live_reads.is_none())
        .count();
    let memory_gap_projects = projects
        .iter()
        .filter(|project| active_project_memory_gap_count(project) > 0)
        .count();
    let memory_gap_count = projects.iter().map(active_project_memory_gap_count).sum();
    let semantic_empty_gap_projects = projects
        .iter()
        .filter(|project| {
            project
                .autonomous_semantic_empty_missing
                .unwrap_or_default()
                > 0
        })
        .count();
    let semantic_empty_gap_count = projects
        .iter()
        .map(|project| {
            project
                .autonomous_semantic_empty_missing
                .unwrap_or_default()
        })
        .sum();
    let semantic_empty_projects = projects
        .iter()
        .filter(|project| {
            project
                .semantic_eligible_empty_read_count
                .unwrap_or_default()
                > 0
        })
        .count();
    let semantic_empty_read_count = projects
        .iter()
        .map(|project| {
            project
                .semantic_eligible_empty_read_count
                .unwrap_or_default()
        })
        .sum();
    let semantic_result_warn_projects = projects
        .iter()
        .filter(|project| {
            project
                .attention_reasons
                .iter()
                .any(|reason| reason == "semantic_empty_results")
        })
        .count();
    let gap_inbox_pending_projects = projects
        .iter()
        .filter(|project| project.gap_inbox.pending > 0)
        .count();
    let gap_inbox_pending_count = projects
        .iter()
        .map(|project| project.gap_inbox.pending)
        .sum();
    let gap_inbox_stale_projects = projects
        .iter()
        .filter(|project| project.gap_inbox.stale_pending > 0)
        .count();
    let gap_inbox_stale_count = projects
        .iter()
        .map(|project| project.gap_inbox.stale_pending)
        .sum();
    let gap_inbox_oldest_pending_age_secs = projects
        .iter()
        .filter_map(|project| project.gap_inbox.oldest_pending_age_secs)
        .max();
    let recommendations_count = projects
        .iter()
        .map(|project| project.recommendations.len())
        .sum();
    let mut attention_reason_counts = BTreeMap::new();
    for reason in projects
        .iter()
        .flat_map(|project| project.attention_reasons.iter())
    {
        *attention_reason_counts.entry(reason.clone()).or_insert(0) += 1;
    }
    let repair_actions_count = projects
        .iter()
        .map(|project| project.repair_actions.len())
        .sum();
    let safe_repair_actions_count = projects
        .iter()
        .flat_map(|project| project.repair_actions.iter())
        .filter(|action| action.safe_auto)
        .count();
    let repair_loop_projects = projects
        .iter()
        .filter(|project| project.repair_loop.observed)
        .count();
    let repair_loop_failed_projects = projects
        .iter()
        .filter(|project| project.repair_loop.failed_actions > 0)
        .count();
    let repair_loop_safe_skipped_projects = projects
        .iter()
        .filter(|project| project.repair_loop.safe_skipped_actions > 0)
        .count();
    let daemon_embedding_skipped_projects = projects
        .iter()
        .filter(|project| project.daemon_embedding_skipped == Some(true))
        .count();
    let daemon_embedding_repaired_projects = projects
        .iter()
        .filter(|project| project.daemon_embedding_repaired_at.is_some())
        .count();
    let attention_projects = total_projects.saturating_sub(ready_projects);
    let ok = attention_projects == 0;
    let status = if ok { "ready" } else { "attention" }.to_string();
    Ok(DashboardReport {
        version: 1,
        ok,
        status,
        total_projects,
        ready_projects,
        attention_projects,
        stale_projects,
        missing_live_eval_projects,
        memory_gap_projects,
        memory_gap_count,
        semantic_empty_gap_projects,
        semantic_empty_gap_count,
        semantic_empty_projects,
        semantic_empty_read_count,
        semantic_result_warn_projects,
        gap_inbox_pending_projects,
        gap_inbox_pending_count,
        gap_inbox_stale_projects,
        gap_inbox_stale_count,
        gap_inbox_oldest_pending_age_secs,
        recommendations_count,
        attention_reason_counts,
        repair_actions_count,
        safe_repair_actions_count,
        repair_loop_projects,
        repair_loop_failed_projects,
        repair_loop_safe_skipped_projects,
        daemon_embedding_skipped_projects,
        daemon_embedding_repaired_projects,
        projects,
    })
}

pub(crate) fn print_memory_qa(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = memory_qa_report(conn, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Memory QA: {}", if report.ok { "ok" } else { "warn" });
        println!("score: {:.1}", report.score);
        println!("reads: {}", report.reads);
        println!(
            "semantic_read_rate: {:.1}%",
            report.semantic_read_rate * 100.0
        );
        println!(
            "semantic_result_rate: {:.1}% empty={} avg_results={:.2}",
            report.semantic_result_rate * 100.0,
            report.semantic_empty_read_count,
            report.semantic_avg_results
        );
        println!(
            "semantic_eligible_result_rate: {:.1}% empty={}",
            report.semantic_eligible_result_rate * 100.0,
            report.semantic_eligible_empty_read_count
        );
        if !report.semantic_empty_queries.is_empty() {
            println!("semantic_empty_queries:");
            for query in &report.semantic_empty_queries {
                println!("- {query}");
            }
        }
        println!(
            "useful_rate: {:.1}% ({})",
            report.useful_rate * 100.0,
            report.useful_rate_source
        );
        println!("quality_average: {:.1}", report.quality_average);
        println!("inferred_missing: {}", report.inferred_missing);
        println!("token_saving_estimate: {}", report.token_saving_estimate);
        for issue in &report.issues {
            println!("issue: {issue}");
        }
        for recommendation in &report.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

pub(crate) fn memory_qa_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
) -> Result<MemoryQaReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let usage = usage_report(conn, since_days, 20)?;
    let quality = quality_report(conn, since_days, 20)?;
    let usefulness = usefulness_report(conn, since_days, 30, 3)?;
    let live = live_eval_report(conn, since_days)?;
    let embedding = embeddings::embed_status(
        conn,
        DEFAULT_EMBED_PROVIDER,
        DEFAULT_EMBED_ENDPOINT,
        DEFAULT_EMBED_MODEL,
    )
    .ok();
    let autonomous = read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
    let semantic_read_rate = usage.semantic_eligible_read_rate;
    let semantic_result_rate = usage.semantic_result_rate;
    let semantic_empty_read_count = usage.semantic_empty_read_count;
    let semantic_avg_results = usage.semantic_avg_results;
    let semantic_eligible_result_rate = usage.semantic_eligible_result_rate;
    let semantic_eligible_empty_read_count = usage.semantic_eligible_empty_read_count;
    let semantic_empty_queries = usage.semantic_empty_queries.clone();
    let token_saving_estimate = quality
        .items
        .iter()
        .map(|item| {
            item.request_count
                .saturating_mul(item.body_chars.saturating_sub(240))
                / 4
        })
        .sum::<usize>();
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    if usage.read_count == 0 {
        issues.push("no recent memory reads".to_string());
        recommendations
            .push("ensure agents start with dukememory brief or MCP memory_brief".to_string());
    }
    if usage.read_count >= 20 && usage.write_pressure > 2.0 {
        issues.push(format!(
            "memory write pressure is high: {:.2} writes per read",
            usage.write_pressure
        ));
        recommendations.push(
            "let autonomous throttling reduce non-critical writes before adding more memory"
                .to_string(),
        );
    }
    if semantic_read_rate < 0.50 && usage.semantic_eligible_total > 0 {
        issues
            .push("semantic recall is used by less than half of eligible recent reads".to_string());
        recommendations
            .push("run dukememory embed-status and embed-index if missing or stale".to_string());
    }
    if usage.semantic_eligible_read_count >= 3 && semantic_eligible_result_rate < 0.75 {
        issues.push(format!(
            "semantic recall returns results for only {:.0}% of eligible semantic reads",
            semantic_eligible_result_rate * 100.0
        ));
        recommendations.push(
            "inspect usage-report semantic empty reads, then refresh embeddings or tune retrieval"
                .to_string(),
        );
    }
    if quality.average_score < 60.0 && quality.total > 0 {
        issues.push("average memory quality is low".to_string());
        recommendations
            .push("review weakest cards with dukememory quality-report --json".to_string());
    }
    if usefulness.too_long.len() > 3 {
        issues.push(format!(
            "{} memory cards are too long",
            usefulness.too_long.len()
        ));
        recommendations.push("compact long cards into bounded summaries".to_string());
    }
    if usefulness.duplicate_candidates.len() > 3 {
        issues.push(format!(
            "{} duplicate candidates detected",
            usefulness.duplicate_candidates.len()
        ));
        recommendations.push(
            "let autonomous supersede safe duplicates or review merge-candidates".to_string(),
        );
    }
    if let Some(embedding) = &embedding {
        if embedding.missing > 0 || embedding.stale > 0 {
            issues.push(format!(
                "embedding index is not current: missing={} stale={}",
                embedding.missing, embedding.stale
            ));
            recommendations.push("run dukememory embed-index".to_string());
        }
    } else {
        issues.push("embedding status unavailable".to_string());
        recommendations.push("check embedding provider configuration".to_string());
    }
    if autonomous.as_ref().is_some_and(|status| !status.ok) {
        issues.push("latest autonomous status is not ok".to_string());
        recommendations.push("run dukememory autonomous explain --json".to_string());
    }
    let mut actionable_missing_queries = Vec::new();
    for query in &live.missing_queries {
        if should_infer_missing_memory_gap(conn, query)? {
            actionable_missing_queries.push(query.clone());
        }
    }
    actionable_missing_queries.sort();
    actionable_missing_queries.dedup();
    if !actionable_missing_queries.is_empty() {
        issues.push(format!(
            "{} unresolved missing feedback query(s)",
            actionable_missing_queries.len()
        ));
        recommendations.push(
            "convert repeated unresolved missing facts into durable memory cards".to_string(),
        );
    }
    if live.inferred_missing > 0 {
        issues.push(format!(
            "{} inferred memory gap(s) from empty agent reads",
            live.inferred_missing
        ));
        recommendations.push(
            "review eval live --json inferred_missing_queries and add durable cards for repeated gaps"
                .to_string(),
        );
    }
    recommendations.sort();
    recommendations.dedup();
    let mut score = 100.0;
    score -= usefulness.unused.len().min(10) as f64 * 2.0;
    score -= usefulness.too_long.len().min(10) as f64 * 3.0;
    score -= usefulness.duplicate_candidates.len().min(10) as f64 * 2.0;
    score -= embedding
        .as_ref()
        .map(|item| item.missing + item.stale)
        .unwrap_or(5)
        .min(10) as f64
        * 4.0;
    if usage.read_count == 0 {
        score -= 20.0;
    }
    if usage.semantic_eligible_read_count >= 3 {
        score -= semantic_eligible_empty_read_count.min(5) as f64 * 3.0;
    }
    if autonomous.as_ref().is_some_and(|status| !status.ok) {
        score -= 12.0;
    }
    score -= live.inferred_missing.min(5) as f64 * 3.0;
    score = score.clamp(0.0, 100.0);
    Ok(MemoryQaReport {
        version: 1,
        ok: score >= 70.0 && issues.len() <= 3,
        score,
        root: root.display().to_string(),
        since_days,
        reads: usage.read_count,
        writes: usage.write_count,
        write_pressure: usage.write_pressure,
        semantic_read_rate,
        semantic_result_rate,
        semantic_empty_read_count,
        semantic_avg_results,
        semantic_eligible_result_rate,
        semantic_eligible_empty_read_count,
        semantic_empty_queries,
        useful_rate: live.useful_rate,
        useful_rate_source: live.useful_rate_source,
        feedback_useful_rate: live.feedback_useful_rate,
        inferred_useful_rate: live.inferred_useful_rate,
        inferred_missing: live.inferred_missing,
        inferred_missing_queries: live.inferred_missing_queries,
        quality_average: quality.average_score,
        active_memories: usefulness.total_active,
        unused: usefulness.unused.len(),
        stale: usefulness.stale.len(),
        too_long: usefulness.too_long.len(),
        duplicate_candidates: usefulness.duplicate_candidates.len(),
        embedding_missing: embedding.as_ref().map(|item| item.missing).unwrap_or(0),
        embedding_stale: embedding.as_ref().map(|item| item.stale).unwrap_or(0),
        autonomous_ok: autonomous.map(|status| status.ok),
        token_saving_estimate,
        issues,
        recommendations,
    })
}

pub(crate) fn print_ops_status(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = ops_status_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("dukememory. ops: {}", report.status);
        println!("score: {:.1}", report.score);
        println!("root: {}", report.root);
        println!(
            "effectiveness: reads={} unique={} semantic={:.0}% semantic_results={:.0}% useful={:.0}% saved_tokens={}",
            report.effectiveness.reads,
            report.effectiveness.unique_memory_ids,
            report.effectiveness.semantic_read_rate * 100.0,
            report.effectiveness.semantic_eligible_result_rate * 100.0,
            report.effectiveness.useful_rate * 100.0,
            report.effectiveness.token_saving_estimate
        );
        println!(
            "quality: avg={:.1} weak={} duplicates={} reversible_cleanup={}",
            report.quality_loop.average_score,
            report.quality_loop.weakest_cards,
            report.quality_loop.duplicate_candidates,
            report.quality_loop.reversible_cleanup_ready
        );
        println!(
            "embeddings: {}/{} current={} reachable={} missing={} stale={}",
            report.embeddings.provider,
            report.embeddings.model,
            report.embeddings.current,
            report.embeddings.provider_reachable,
            report.embeddings.missing,
            report.embeddings.stale
        );
        println!(
            "autonomous: installed={} ok={} rollback_ready={} daemon_embedding_skipped={} daemon_embedding_repaired_at={} daemon_embedding_repair_source={}",
            report.autonomous.installed,
            report
                .autonomous
                .ok
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            report.autonomous.rollback_ready,
            report
                .autonomous
                .daemon_embedding_skipped
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            report
                .autonomous
                .daemon_embedding_repaired_at
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            report
                .autonomous
                .daemon_embedding_repair_source
                .as_deref()
                .unwrap_or("-")
        );
        println!(
            "gap_inbox: pending={} stale_pending={} total={} approved={} rejected={} oldest_pending_age_secs={}",
            report.gap_inbox.pending,
            report.gap_inbox.stale_pending,
            report.gap_inbox.total,
            report.gap_inbox.approved,
            report.gap_inbox.rejected,
            format_optional_secs(report.gap_inbox.oldest_pending_age_secs)
        );
        println!(
            "storage: db={} agent={} backups={}/{} rollbacks={}/{} install_backups={}/{} pressure={}",
            report.storage.db_bytes,
            report.storage.agent_bytes,
            report.storage.backups_count,
            report.storage.backups_bytes,
            report.storage.rollback_count,
            report.storage.rollback_bytes,
            report.storage.install_backups_count,
            report.storage.install_backups_bytes,
            report.storage.pressure
        );
        println!(
            "multi_device: ready={} local_first={}",
            report.multi_device.ready, report.multi_device.local_first
        );
        for issue in &report.issues {
            println!("issue: {issue}");
        }
        for recommendation in &report.recommendations {
            println!("recommendation: {recommendation}");
        }
    }
    Ok(())
}

pub(crate) fn print_remote_status(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = remote_status_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote/VDS Readiness");
    println!("status: {}", report.status);
    println!("local_first: {}", report.local_first);
    println!("ready: {}", report.ready);
    println!("write_pressure: {:.2}", report.write_pressure);
    println!("embedding_current: {}", report.embedding_current);
    println!("provider_reachable: {}", report.provider_reachable);
    println!("backup_ready: {}", report.backup_ready);
    println!("export: {}", report.export_command);
    println!("import: {}", report.import_command);
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn remote_status_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<RemoteStatusReport> {
    let ops = ops_status_report(conn, db, root, since_days)?;
    let backup_ready = ops.storage.backups_count > 0 || ops.autonomous.rollback_ready;
    let mut blockers = ops.multi_device.blockers.clone();
    if !backup_ready {
        blockers.push("no backup or rollback metadata available".to_string());
    }
    let mut recommendations = ops.recommendations.clone();
    recommendations
        .push("keep memory local-first; use remote/VDS only as optional sync target".to_string());
    recommendations.push("measure VDS latency before enabling remote-first reads".to_string());
    recommendations.sort();
    recommendations.dedup();
    blockers.sort();
    blockers.dedup();
    let ready = blockers.is_empty();
    Ok(RemoteStatusReport {
        version: 1,
        ok: ready,
        status: if ready { "ready" } else { "blocked" }.to_string(),
        local_first: true,
        ready,
        export_command: ops.multi_device.export_command,
        import_command: ops.multi_device.import_command,
        write_pressure: ops.effectiveness.writes as f64 / ops.effectiveness.reads.max(1) as f64,
        embedding_current: ops.embeddings.current,
        provider_reachable: ops.embeddings.provider_reachable,
        backup_ready,
        estimated_local_latency_ms: 2,
        estimated_vds_latency_ms: 50,
        blockers,
        recommendations,
    })
}

pub(crate) fn ops_status_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<OpsStatusReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let usage = usage_report(conn, since_days, 20)?;
    let quality = quality_report(conn, since_days, 20)?;
    let usefulness = usefulness_report(conn, since_days, 30, 3)?;
    let (provider, endpoint, model) = read_project_embedding_config(&root);
    let embedding = embeddings::embed_status(conn, &provider, &endpoint, &model)?;
    let status_file = root.join(".agent/autonomous-status.json");
    let rollback_dir = root.join(".agent/autonomous-rollbacks");
    let backup_dir = root.join(".agent/backups");
    let install_backup_dir = root.join(".agent/install-backups");
    let autonomous = read_autonomous_status(&status_file).ok();
    let daemon_embedding = daemon_embedding_snapshot(&root);
    let embedding_current = embedding.missing == 0 && embedding.stale == 0;
    let rollback_ready = rollback_dir.is_dir();
    let storage = ops_storage_status(conn, db, &root)?;
    let agent_integration = ops_agent_integration_status(db, &root);
    let autonomous_age_secs = autonomous
        .as_ref()
        .map(|report| ((now_ms() - report.updated_at).max(0)) / 1000);
    let autonomous_fresh = autonomous_age_secs.is_some_and(|age| age <= 86_400);
    let repair_loop = ops_repair_loop_status(conn, since_days)?;
    let gap_inbox = dashboard_gap_inbox_status(conn).unwrap_or_default();

    let quality_loop = OpsQualityLoopStatus {
        average_score: quality.average_score,
        total_cards: quality.total,
        weakest_cards: quality.weakest.len(),
        unused_cards: usefulness.unused.len(),
        stale_cards: usefulness.stale.len(),
        too_long_cards: usefulness.too_long.len(),
        duplicate_candidates: usefulness.duplicate_candidates.len(),
        reversible_cleanup_ready: rollback_ready || status_file.exists(),
    };

    let mut issues = qa.issues.clone();
    let mut recommendations = qa.recommendations.clone();
    if autonomous.is_none() {
        issues.push("autonomous maintenance has not written a status report".to_string());
        recommendations.push("run dukememory autonomous run-once --level normal".to_string());
    }
    if let Some(age) = autonomous_age_secs
        && age > 86_400
    {
        issues.push(format!(
            "autonomous maintenance status is stale: age_secs={age}"
        ));
        recommendations.push("run dukememory autonomous run-once --level normal".to_string());
    }
    if !rollback_ready {
        recommendations.push(
            "run one autonomous cycle to create rollback metadata before unattended cleanup"
                .to_string(),
        );
    }
    if !embedding_current {
        recommendations.push("run dukememory embed-index before cross-device sync".to_string());
    }
    if !embedding.provider_reachable {
        issues.push("embedding provider is not reachable".to_string());
        recommendations.push(
            "check embedding endpoint/model before relying on semantic recall or embed-index"
                .to_string(),
        );
    }
    if daemon_embedding.skipped == Some(true) {
        issues.push("daemon skipped embedding maintenance".to_string());
        recommendations.push("check daemon embedding_error and provider health".to_string());
    }
    if repair_loop.failed_actions > 0 {
        issues.push(format!(
            "dashboard repair loop has failed actions: failed={}",
            repair_loop.failed_actions
        ));
        recommendations.push("run dukememory dashboard-repair-history --json".to_string());
    }
    if repair_loop.safe_skipped_actions > 0 {
        recommendations.push(
            "run dukememory dashboard-repair --apply to apply pending safe repairs".to_string(),
        );
    }
    if gap_inbox.pending > 0 {
        recommendations.push(format!(
            "review {} pending autonomous gap inbox item(s)",
            gap_inbox.pending
        ));
    }
    if gap_inbox.stale_pending > 0 {
        issues.push(format!(
            "autonomous gap inbox has stale pending items: stale_pending={} oldest_pending_age_secs={}",
            gap_inbox.stale_pending,
            format_optional_secs(gap_inbox.oldest_pending_age_secs)
        ));
        recommendations.push(
            "run dukememory autonomous run-once --level normal to refresh stale gap inbox items"
                .to_string(),
        );
    }
    if storage.pressure == "warn" {
        issues.push(format!(
            "local memory storage is growing: .agent={} bytes",
            storage.agent_bytes
        ));
        recommendations.push(
            "run dukememory autonomous run-once --level normal to refresh retention".to_string(),
        );
    }
    if storage.backups_count > 10 {
        recommendations.push(format!(
            "rotate database backups in {}",
            backup_dir.display()
        ));
    }
    if storage.rollback_count > 10 {
        recommendations.push(format!(
            "rotate autonomous rollback backups in {}",
            rollback_dir.display()
        ));
    }
    if storage.install_backups_count > DEFAULT_INSTALL_BACKUP_KEEP {
        recommendations.push(format!(
            "run update-install --backup-keep {} for {}",
            DEFAULT_INSTALL_BACKUP_KEEP,
            install_backup_dir.display(),
        ));
    }
    if storage.vacuum_recommended {
        recommendations.push(format!(
            "run dukememory --db {} optimize --vacuum during idle time",
            db.display()
        ));
    }
    if !agent_integration.project_memory_installed {
        issues.push("project memory database is missing".to_string());
        recommendations.push("run dukememory onboard --root . --install-autonomous".to_string());
    }
    if !agent_integration.project_config_present {
        recommendations.push(
            "run dukememory upgrade-project --json to refresh .agent/config.toml".to_string(),
        );
    }
    if !agent_integration.agents_block_present {
        recommendations
            .push("run dukememory upgrade-project --json to refresh AGENTS.md".to_string());
    }
    if !agent_integration.skill_installed {
        recommendations.push("run dukememory install-skill --force".to_string());
    }
    if !agent_integration.codex_mcp_configured {
        recommendations
            .push("run dukememory codex-doctor --json to inspect MCP wiring".to_string());
    }

    let mut blockers = Vec::new();
    if qa.active_memories == 0 {
        blockers.push("no active project memories to sync".to_string());
    }
    if !embedding_current {
        blockers.push(format!(
            "embedding index not current: missing={} stale={}",
            embedding.missing, embedding.stale
        ));
    }
    if !embedding.provider_reachable {
        blockers.push(format!(
            "embedding provider is unreachable: {}",
            embedding
                .provider_error
                .as_deref()
                .unwrap_or("health check failed")
        ));
    }
    if quality_loop.duplicate_candidates > 8 {
        blockers.push(format!(
            "{} duplicate candidates should be resolved before sharing",
            quality_loop.duplicate_candidates
        ));
    }
    if !qa.ok {
        blockers.push("memory QA score is below ready threshold".to_string());
    }

    recommendations.sort();
    recommendations.dedup();
    issues.sort();
    issues.dedup();

    let mut score = qa.score;
    if autonomous.is_none() {
        score -= 8.0;
    }
    if !rollback_ready {
        score -= 4.0;
    }
    if !blockers.is_empty() {
        score -= blockers.len().min(5) as f64 * 3.0;
    }
    if storage.pressure == "warn" {
        score -= 4.0;
    }
    if repair_loop.failed_actions > 0 {
        score -= 5.0;
    }
    score = score.clamp(0.0, 100.0);

    let ok = score >= 70.0 && blockers.len() <= 2;
    let status = if ok {
        "ready"
    } else if score >= 50.0 {
        "needs-attention"
    } else {
        "blocked"
    }
    .to_string();

    Ok(OpsStatusReport {
        version: 1,
        ok,
        status,
        score,
        root: root.display().to_string(),
        since_days,
        effectiveness: OpsEffectivenessStatus {
            reads: usage.read_count,
            writes: usage.write_count,
            unique_memory_ids: usage.unique_memory_ids,
            semantic_read_rate: qa.semantic_read_rate,
            semantic_result_rate: qa.semantic_result_rate,
            semantic_empty_read_count: qa.semantic_empty_read_count,
            semantic_avg_results: qa.semantic_avg_results,
            semantic_eligible_result_rate: qa.semantic_eligible_result_rate,
            semantic_eligible_empty_read_count: qa.semantic_eligible_empty_read_count,
            semantic_empty_queries: qa.semantic_empty_queries,
            useful_rate: qa.useful_rate,
            useful_rate_source: qa.useful_rate_source,
            feedback_useful_rate: qa.feedback_useful_rate,
            inferred_useful_rate: qa.inferred_useful_rate,
            inferred_missing: qa.inferred_missing,
            inferred_missing_queries: qa.inferred_missing_queries,
            token_saving_estimate: qa.token_saving_estimate,
        },
        quality_loop,
        embeddings: OpsEmbeddingStatus {
            provider: embedding.provider,
            endpoint: embedding.endpoint,
            model: embedding.model,
            eligible: embedding.eligible,
            indexed: embedding.indexed,
            missing: embedding.missing,
            stale: embedding.stale,
            current: embedding_current,
            provider_reachable: embedding.provider_reachable,
            provider_health_ms: embedding.provider_health_ms,
            provider_error: embedding.provider_error,
            background_sync_ready: embedding_current && embedding.provider_reachable,
        },
        autonomous: OpsAutonomousStatus {
            installed: autonomous.is_some(),
            ok: autonomous.as_ref().map(|report| report.ok),
            status_file: status_file.display().to_string(),
            rollback_ready,
            updated_at: autonomous.as_ref().map(|report| report.updated_at),
            age_secs: autonomous_age_secs,
            fresh: autonomous_fresh,
            last_action_count: autonomous.as_ref().map(|report| report.actions.len()),
            daemon_embedding_skipped: daemon_embedding.skipped,
            daemon_embedding_error: daemon_embedding.error,
            daemon_embedding_repaired_at: daemon_embedding.repaired_at,
            daemon_embedding_repair_source: daemon_embedding.repair_source,
        },
        repair_loop,
        gap_inbox,
        agent_integration,
        storage,
        multi_device: OpsMultiDeviceStatus {
            ready: blockers.is_empty(),
            local_first: true,
            export_command: format!(
                "dukememory --db {} sync export memory-sync.json",
                db.display()
            ),
            import_command: "dukememory sync import memory-sync.json".to_string(),
            blockers,
        },
        issues,
        recommendations,
    })
}

fn ops_repair_loop_status(conn: &Connection, since_days: i64) -> Result<OpsRepairLoopStatus> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let events = dashboard_repair_events(conn, since_ms, 50)?;
    let mut actions_by_code = BTreeMap::new();
    let mut applied_actions = 0;
    let mut skipped_actions = 0;
    let mut failed_actions = 0;
    let mut safe_actions = 0;
    let mut safe_skipped_actions = 0;
    let mut manual_actions = 0;
    for event in &events {
        applied_actions += event.applied_actions;
        skipped_actions += event.skipped_actions;
        failed_actions += event.failed_actions;
        safe_actions += event.safe_actions;
        for action in &event.actions {
            *actions_by_code.entry(action.code.clone()).or_insert(0) += 1;
            if action.safe_auto && action.skipped {
                safe_skipped_actions += 1;
            }
            if !action.safe_auto {
                manual_actions += 1;
            }
        }
    }
    let last = events.first();
    Ok(OpsRepairLoopStatus {
        observed: !events.is_empty(),
        healthy: failed_actions == 0,
        runs: events.len(),
        applied_actions,
        skipped_actions,
        failed_actions,
        safe_actions,
        safe_skipped_actions,
        manual_actions,
        last_run_at: last.map(|event| event.created_at),
        last_source: last.map(|event| event.source.clone()),
        last_action_count: last.map(|event| event.total_actions),
        actions_by_code,
    })
}

fn empty_repair_loop_status() -> OpsRepairLoopStatus {
    OpsRepairLoopStatus {
        observed: false,
        healthy: true,
        runs: 0,
        applied_actions: 0,
        skipped_actions: 0,
        failed_actions: 0,
        safe_actions: 0,
        safe_skipped_actions: 0,
        manual_actions: 0,
        last_run_at: None,
        last_source: None,
        last_action_count: None,
        actions_by_code: BTreeMap::new(),
    }
}

fn daemon_embedding_snapshot(root: &Path) -> DaemonEmbeddingSnapshot {
    let path = root.join(".agent/daemon-status.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return DaemonEmbeddingSnapshot::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return DaemonEmbeddingSnapshot::default();
    };
    DaemonEmbeddingSnapshot {
        skipped: value.get("embedding_skipped").and_then(Value::as_bool),
        error: value
            .get("embedding_error")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        repaired_at: value.get("embedding_repaired_at").and_then(Value::as_i64),
        repair_source: value
            .get("embedding_repair_source")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn dashboard_gap_inbox_status(conn: &Connection) -> Result<DashboardGapInboxStatus> {
    let mut status = DashboardGapInboxStatus::default();
    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) FROM memory_inbox WHERE source = 'autonomous_gap' GROUP BY status",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (state, count) = row?;
        let count = count.max(0) as usize;
        status.total += count;
        match state.as_str() {
            "pending" => status.pending += count,
            "approved" => status.approved += count,
            "rejected" => status.rejected += count,
            _ => {}
        }
    }
    let oldest_pending_created_at = conn.query_row(
        "SELECT MIN(created_at) FROM memory_inbox WHERE source = 'autonomous_gap' AND status = 'pending'",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    status.oldest_pending_age_secs =
        oldest_pending_created_at.map(|created_at| now_ms().saturating_sub(created_at) / 1000);
    let stale_cutoff = now_ms().saturating_sub(GAP_INBOX_STALE_MS);
    let stale_pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE source = 'autonomous_gap' AND status = 'pending' AND created_at <= ?1",
        [stale_cutoff],
        |row| row.get(0),
    )?;
    status.stale_pending = stale_pending.max(0) as usize;
    Ok(status)
}

fn active_dashboard_memory_gap_count(
    live_eval: Option<&LiveEvalReport>,
    gap_inbox: &DashboardGapInboxStatus,
) -> usize {
    active_memory_gap_count(
        live_eval
            .map(|live| live.inferred_missing)
            .unwrap_or_default(),
        gap_inbox,
    )
}

fn active_memory_gap_count(live_gaps: usize, gap_inbox: &DashboardGapInboxStatus) -> usize {
    if live_gaps == 0 {
        return 0;
    }
    if gap_inbox.pending > 0 || gap_inbox.stale_pending > 0 {
        return live_gaps;
    }
    0
}

fn active_project_memory_gap_count(project: &ProjectDashboardItem) -> usize {
    active_memory_gap_count(
        project.autonomous_inferred_missing.unwrap_or_default(),
        &project.gap_inbox,
    )
}

fn format_optional_secs(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn ops_agent_integration_status(db: &Path, root: &Path) -> OpsAgentIntegrationStatus {
    let skill_path = expand_tilde("~/.codex/skills/dukememory-use/SKILL.md");
    let codex_config = expand_tilde("~/.codex/config.toml");
    let agents_path = root.join("AGENTS.md");
    let project_memory_installed = db.exists();
    let project_config_present = root.join(".agent/config.toml").exists();
    let agents_block_present = fs::read_to_string(&agents_path)
        .map(|content| content.contains("<!-- DUKEMEMORY_START -->"))
        .unwrap_or(false);
    let skill_installed = fs::read_to_string(&skill_path)
        .map(|content| content.contains("name: dukememory-use"))
        .unwrap_or(false);
    let codex_mcp_configured = fs::read_to_string(&codex_config)
        .map(|content| content.contains("[mcp_servers.dukememory]"))
        .unwrap_or(false);
    let ready = project_memory_installed
        && project_config_present
        && agents_block_present
        && skill_installed;
    OpsAgentIntegrationStatus {
        ready,
        project_memory_installed,
        project_config_present,
        agents_block_present,
        skill_installed,
        codex_mcp_configured,
        skill_path: skill_path.display().to_string(),
        codex_config: codex_config.display().to_string(),
    }
}

fn ops_storage_status(conn: &Connection, db: &Path, root: &Path) -> Result<OpsStorageStatus> {
    let agent_dir = root.join(".agent");
    let backup_dir = agent_dir.join("backups");
    let rollback_dir = agent_dir.join("autonomous-rollbacks");
    let install_backup_dir = agent_dir.join("install-backups");
    let db_bytes = file_size(db);
    let page_count = sqlite_i64_pragma(conn, "PRAGMA page_count")?;
    let freelist_count = sqlite_i64_pragma(conn, "PRAGMA freelist_count")?;
    let freelist_ratio = if page_count <= 0 {
        0.0
    } else {
        freelist_count.max(0) as f64 / page_count as f64
    };
    let vacuum_recommended = db_bytes > 4 * 1024 * 1024 && freelist_ratio >= 0.20;
    let agent_bytes = dir_size(&agent_dir)?;
    let backups_bytes = dir_size(&backup_dir)?;
    let rollback_bytes = dir_size(&rollback_dir)?;
    let install_backups_bytes = dir_size(&install_backup_dir)?;
    let backups_count = count_named_files(&backup_dir, |name| {
        name.starts_with("dukememory-") && name.ends_with(".db")
    })?;
    let rollback_count = count_named_files(&rollback_dir, |name| {
        name.starts_with("autonomous-") && name.ends_with(".db")
    })?;
    let install_backups_count = count_named_files(&install_backup_dir, |name| {
        name.starts_with("dukememory") && name.ends_with(".bak")
    })?;
    let retention_ready = backups_count <= 10
        && rollback_count <= 10
        && install_backups_count <= DEFAULT_INSTALL_BACKUP_KEEP;
    let pressure = if agent_bytes > 512 * 1024 * 1024
        || backups_count > 20
        || rollback_count > 20
        || install_backups_count > 20
    {
        "warn"
    } else {
        "ok"
    }
    .to_string();
    Ok(OpsStorageStatus {
        db_bytes,
        page_count,
        freelist_count,
        freelist_ratio,
        vacuum_recommended,
        agent_bytes,
        backups_bytes,
        backups_count,
        rollback_bytes,
        rollback_count,
        install_backups_bytes,
        install_backups_count,
        retention_ready,
        pressure,
    })
}

fn sqlite_i64_pragma(conn: &Connection, sql: &str) -> Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
        .map_err(Into::into)
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn dir_size(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let entry_path = entry.path();
        if file_type.is_file() {
            total = total.saturating_add(file_size(&entry_path));
        } else if file_type.is_dir() {
            total = total.saturating_add(dir_size(&entry_path)?);
        }
    }
    Ok(total)
}

fn count_named_files(path: &Path, matches_name: impl Fn(&str) -> bool) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let mut count = 0_usize;
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if matches_name(name) {
            count += 1;
        }
    }
    Ok(count)
}

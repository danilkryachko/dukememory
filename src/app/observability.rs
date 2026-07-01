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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DecisionTraceReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) traced_reads: usize,
    pub(crate) influenced_reads: usize,
    pub(crate) empty_reads: usize,
    pub(crate) confirmed_reads: usize,
    pub(crate) questioned_reads: usize,
    pub(crate) semantic_influenced_reads: usize,
    pub(crate) positive_feedback: usize,
    pub(crate) negative_feedback: usize,
    pub(crate) missing_feedback: usize,
    pub(crate) items: Vec<DecisionTraceItem>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DecisionTraceItem {
    pub(crate) read_id: i64,
    pub(crate) command: String,
    pub(crate) query: String,
    pub(crate) semantic_used: bool,
    pub(crate) result_count: usize,
    pub(crate) memory_ids: Vec<String>,
    pub(crate) memory_titles: Vec<String>,
    pub(crate) influence: String,
    pub(crate) explanation: String,
    pub(crate) without_memory: String,
    pub(crate) feedback_positive: usize,
    pub(crate) feedback_negative: usize,
    pub(crate) feedback_missing: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AutoFeedbackV2Report {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) applied: bool,
    pub(crate) scanned: usize,
    pub(crate) written: usize,
    pub(crate) useful: usize,
    pub(crate) missing: usize,
    pub(crate) skipped: usize,
    pub(crate) useful_rate_before: f64,
    pub(crate) useful_rate_after: f64,
    pub(crate) inferred_missing_before: usize,
    pub(crate) inferred_missing_after: usize,
    pub(crate) closed_missing: usize,
    pub(crate) unresolved_missing_queries: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CostGuardReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) recommended_profile: String,
    pub(crate) recommended_max_chars: usize,
    pub(crate) average_read_budget: f64,
    pub(crate) max_read_budget: usize,
    pub(crate) write_pressure: f64,
    pub(crate) token_saving_estimate: usize,
    pub(crate) large_memory_count: usize,
    pub(crate) noisy_memory_count: usize,
    pub(crate) guard_active: bool,
    pub(crate) issues: Vec<String>,
    pub(crate) actions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectDiffReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) changed_only: bool,
    pub(crate) changed_files: Vec<String>,
    pub(crate) missing_links: usize,
    pub(crate) conflicts: usize,
    pub(crate) stale_active: usize,
    pub(crate) new_or_changed_memory_ids: Vec<String>,
    pub(crate) drift: DriftReport,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RemoteSyncDryRunReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) local_first: bool,
    pub(crate) db_bytes: u64,
    pub(crate) estimated_export_bytes: u64,
    pub(crate) estimated_upload_ms: u32,
    pub(crate) estimated_download_ms: u32,
    pub(crate) estimated_roundtrip_ms: u32,
    pub(crate) export_command: String,
    pub(crate) import_command: String,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct IntelligenceDashboardReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) roi: MemoryRoiReport,
    pub(crate) agent_audit: AgentAuditReport,
    pub(crate) cost_guard: CostGuardReport,
    pub(crate) decision_trace: DecisionTraceReport,
    pub(crate) auto_feedback: AutoFeedbackV2Report,
    pub(crate) project_diff: ProjectDiffReport,
    pub(crate) remote_sync: RemoteSyncDryRunReport,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectDoctorReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) fixed: bool,
    pub(crate) fix_actions: Vec<String>,
    pub(crate) checks: Vec<ProjectDoctorCheck>,
    pub(crate) memory_qa: MemoryQaReport,
    pub(crate) embedding: Option<DoctorEmbeddingStatus>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectDoctorCheck {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DoctorEmbeddingStatus {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) eligible: usize,
    pub(crate) indexed: usize,
    pub(crate) missing: usize,
    pub(crate) stale: usize,
    pub(crate) provider_reachable: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReleaseGateReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) strict: bool,
    pub(crate) run: bool,
    pub(crate) checks: Vec<ReleaseGateCheck>,
    pub(crate) commands: Vec<ReleaseGateCommandResult>,
    pub(crate) doctor: ProjectDoctorReport,
    pub(crate) intelligence: IntelligenceDashboardReport,
    pub(crate) autonomous_loop: AutonomousLoopReport,
    pub(crate) usefulness_engine: UsefulnessEngineReport,
    pub(crate) sync_latency: SyncLatencyReport,
    pub(crate) action_journal: ActionJournalReport,
    pub(crate) sync_profile: SyncProfileReport,
    pub(crate) agent_enforce: AgentEnforceReport,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleaseGateCheck {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) required: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReleaseGateCommandResult {
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) ok: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) elapsed_ms: u128,
    pub(crate) output_tail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryReplayReport {
    pub(crate) version: u32,
    pub(crate) since_days: i64,
    pub(crate) reads: usize,
    pub(crate) influenced_reads: usize,
    pub(crate) semantic_reads: usize,
    pub(crate) items: Vec<MemoryReplayItem>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryReplayItem {
    pub(crate) read_id: i64,
    pub(crate) command: String,
    pub(crate) query: String,
    pub(crate) semantic_used: bool,
    pub(crate) result_count: usize,
    pub(crate) memory_ids: Vec<String>,
    pub(crate) memory_titles: Vec<String>,
    pub(crate) effect: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectWatchReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) fix: bool,
    pub(crate) total_projects: usize,
    pub(crate) attention_projects: usize,
    pub(crate) projects: Vec<ProjectWatchItem>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectWatchItem {
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) ok: bool,
    pub(crate) doctor_status: String,
    pub(crate) intelligence_status: String,
    pub(crate) fix_actions: Vec<String>,
    pub(crate) issues: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomousLoopReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) level: String,
    pub(crate) applied: bool,
    pub(crate) scheduled: bool,
    pub(crate) run_index: usize,
    pub(crate) next_interval_secs: Option<u64>,
    pub(crate) watch: ProjectWatchReport,
    pub(crate) doctor: ProjectDoctorReport,
    pub(crate) intelligence: IntelligenceDashboardReport,
    pub(crate) actions: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ActionJournalReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) since_days: i64,
    pub(crate) total: usize,
    pub(crate) applied: usize,
    pub(crate) skipped: usize,
    pub(crate) failed: usize,
    pub(crate) rollback_events: usize,
    pub(crate) items: Vec<ActionJournalItem>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomousWatchInstallReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) dry_run: bool,
    pub(crate) label: String,
    pub(crate) plist: String,
    pub(crate) command: Vec<String>,
    pub(crate) interval_secs: u64,
    pub(crate) log_path: String,
    pub(crate) status_file: String,
    pub(crate) actions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ActionJournalItem {
    pub(crate) id: i64,
    pub(crate) event_type: String,
    pub(crate) status: String,
    pub(crate) action: String,
    pub(crate) detail: String,
    pub(crate) created_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct UsefulnessEngineReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) qa: MemoryQaReport,
    pub(crate) quality: QualityReport,
    pub(crate) replay: MemoryReplayReport,
    pub(crate) auto_feedback: AutoFeedbackV2Report,
    pub(crate) ranking_policy: Vec<String>,
    pub(crate) suppress_candidates: Vec<String>,
    pub(crate) promote_candidates: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RankingProfileReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) profile: String,
    pub(crate) applied: bool,
    pub(crate) path: String,
    pub(crate) weights: BTreeMap<String, f64>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectTemplateReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) kind: String,
    pub(crate) applied: bool,
    pub(crate) path: String,
    pub(crate) budget_profile: String,
    pub(crate) recommended_commands: Vec<String>,
    pub(crate) starter_memory: Vec<String>,
    pub(crate) actions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncLatencyReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) local_first: bool,
    pub(crate) samples: usize,
    pub(crate) local_db_bytes: u64,
    pub(crate) local_read_ms: u128,
    pub(crate) target: Option<String>,
    pub(crate) target_write_ms: Option<u128>,
    pub(crate) target_read_ms: Option<u128>,
    pub(crate) estimated_roundtrip_ms: u32,
    pub(crate) recommended_mode: String,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncProfileReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) profile: String,
    pub(crate) applied: bool,
    pub(crate) local_first: bool,
    pub(crate) target: Option<String>,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) commands: Vec<String>,
    pub(crate) flow_steps: Vec<SyncProfileFlowStep>,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncProfileFlowStep {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryDiffReviewReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) changed_files: Vec<String>,
    pub(crate) suggested_memory: Vec<String>,
    pub(crate) stale_memory_ids: Vec<String>,
    pub(crate) conflict_count: usize,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentEnforceReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) fixed: bool,
    pub(crate) required_commands: Vec<String>,
    pub(crate) missing_commands: Vec<String>,
    pub(crate) doctor: ProjectDoctorReport,
    pub(crate) issues: Vec<String>,
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

pub(crate) fn print_decision_trace(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = decision_trace_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Decision Trace");
    println!("traced_reads: {}", report.traced_reads);
    println!("influenced_reads: {}", report.influenced_reads);
    println!("empty_reads: {}", report.empty_reads);
    for item in &report.items {
        println!(
            "- read={} {} influence={} results={} ids=[{}] {}",
            item.read_id,
            item.command,
            item.influence,
            item.result_count,
            item.memory_ids
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(","),
            truncate_chars(&item.query, 90)
        );
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn decision_trace_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<DecisionTraceReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let reads = read_events(conn, since_ms, limit)?;
    let feedback = memory_feedback_counts(conn, since_ms)?;
    let memory_titles = memory_title_map(conn)?;
    let mut items = Vec::new();
    let mut influenced_reads = 0;
    let mut empty_reads = 0;
    let mut confirmed_reads = 0;
    let mut questioned_reads = 0;
    let mut semantic_influenced_reads = 0;
    let mut positive_feedback = 0;
    let mut negative_feedback = 0;
    let mut missing_feedback = 0;
    for read in reads {
        if read.memory_ids.is_empty() || read.result_count == 0 {
            empty_reads += 1;
        } else {
            influenced_reads += 1;
        }
        let mut item_positive = 0;
        let mut item_negative = 0;
        let mut item_missing = 0;
        for id in &read.memory_ids {
            let (pos, neg, miss) = feedback.get(id).copied().unwrap_or_default();
            item_positive += pos;
            item_negative += neg;
            item_missing += miss;
        }
        positive_feedback += item_positive;
        negative_feedback += item_negative;
        missing_feedback += item_missing;
        let influence = if read.memory_ids.is_empty() || read.result_count == 0 {
            "none"
        } else if item_negative > item_positive {
            "questioned"
        } else if item_positive > 0 {
            "confirmed"
        } else if read.semantic_used {
            "semantic_candidate"
        } else {
            "candidate"
        }
        .to_string();
        if influence == "confirmed" {
            confirmed_reads += 1;
        }
        if influence == "questioned" {
            questioned_reads += 1;
        }
        if read.semantic_used && !read.memory_ids.is_empty() && read.result_count > 0 {
            semantic_influenced_reads += 1;
        }
        let titles = read
            .memory_ids
            .iter()
            .filter_map(|id| memory_titles.get(id).cloned())
            .collect::<Vec<_>>();
        let explanation = if titles.is_empty() {
            "no memory card was available for this read".to_string()
        } else {
            format!(
                "used {} card(s): {}",
                read.memory_ids.len(),
                titles
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        let without_memory = if titles.is_empty() {
            "agent would continue without durable project context".to_string()
        } else if item_positive > 0 {
            "agent likely avoided rediscovering confirmed project context".to_string()
        } else if read.semantic_used {
            "agent likely avoided a broader manual search by using semantic recall".to_string()
        } else {
            "agent received candidate project context before reading more files".to_string()
        };
        items.push(DecisionTraceItem {
            read_id: read.id,
            command: read.command,
            query: read.query,
            semantic_used: read.semantic_used,
            result_count: read.result_count,
            memory_ids: read.memory_ids,
            memory_titles: titles,
            influence,
            explanation,
            without_memory: without_memory.to_string(),
            feedback_positive: item_positive,
            feedback_negative: item_negative,
            feedback_missing: item_missing,
        });
    }
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    if influenced_reads == 0 && !items.is_empty() {
        issues.push("recent reads did not return memory cards".to_string());
        recommendations.push("refresh embeddings or add durable missing-context cards".to_string());
    }
    if negative_feedback > positive_feedback && negative_feedback > 0 {
        issues.push("recent traced memories have more negative than positive feedback".to_string());
        recommendations.push("review noisy memory cards before broad recall".to_string());
    }
    Ok(DecisionTraceReport {
        version: 1,
        since_days,
        traced_reads: items.len(),
        influenced_reads,
        empty_reads,
        confirmed_reads,
        questioned_reads,
        semantic_influenced_reads,
        positive_feedback,
        negative_feedback,
        missing_feedback,
        items,
        issues,
        recommendations,
    })
}

pub(crate) fn print_memory_replay(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = memory_replay_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Replay");
    println!("reads: {}", report.reads);
    println!("influenced_reads: {}", report.influenced_reads);
    for item in &report.items {
        println!(
            "- {} {} results={} {}",
            item.command,
            item.effect,
            item.result_count,
            truncate_chars(&item.query, 90)
        );
    }
    Ok(())
}

pub(crate) fn memory_replay_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<MemoryReplayReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let reads = read_events(conn, since_ms, limit)?;
    let memory_titles = memory_title_map(conn)?;
    let mut items = Vec::new();
    let mut influenced_reads = 0;
    let mut semantic_reads = 0;
    for read in reads {
        if read.semantic_used {
            semantic_reads += 1;
        }
        if !read.memory_ids.is_empty() && read.result_count > 0 {
            influenced_reads += 1;
        }
        let titles = read
            .memory_ids
            .iter()
            .filter_map(|id| memory_titles.get(id).cloned())
            .collect::<Vec<_>>();
        let effect = if read.memory_ids.is_empty() || read.result_count == 0 {
            "no_memory_used"
        } else if read.semantic_used {
            "semantic_recall_used"
        } else {
            "local_recall_used"
        }
        .to_string();
        items.push(MemoryReplayItem {
            read_id: read.id,
            command: read.command,
            query: read.query,
            semantic_used: read.semantic_used,
            result_count: read.result_count,
            memory_ids: read.memory_ids,
            memory_titles: titles,
            effect,
        });
    }
    Ok(MemoryReplayReport {
        version: 1,
        since_days,
        reads: items.len(),
        influenced_reads,
        semantic_reads,
        items,
    })
}

pub(crate) fn print_project_watch(
    default_db: &Path,
    since_days: i64,
    fix: bool,
    json_out: bool,
) -> Result<()> {
    let report = project_watch_report(default_db, since_days, fix)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Watch");
    println!("projects: {}", report.total_projects);
    println!("attention: {}", report.attention_projects);
    for project in &report.projects {
        println!(
            "{} {} doctor={} intelligence={}",
            if project.ok { "ok" } else { "warn" },
            project.root,
            project.doctor_status,
            project.intelligence_status
        );
    }
    Ok(())
}

pub(crate) fn project_watch_report(
    default_db: &Path,
    since_days: i64,
    fix: bool,
) -> Result<ProjectWatchReport> {
    let mut projects = Vec::new();
    for db in discover_project_dbs(default_db)? {
        let db = db.canonicalize().unwrap_or(db);
        let root = db
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let conn = open_db(&db)?;
        let fixed_doctor = project_doctor_report(&conn, &db, &root, since_days, fix)?;
        let doctor = if fix {
            project_doctor_report(&conn, &db, &root, since_days, false)?
        } else {
            fixed_doctor.clone()
        };
        let intelligence = intelligence_dashboard_report(&conn, &db, &root, since_days)?;
        let ok = doctor.ok && intelligence.ok;
        let mut issues = doctor.issues.clone();
        issues.extend(intelligence.issues.iter().cloned());
        issues.sort();
        issues.dedup();
        projects.push(ProjectWatchItem {
            root: root.display().to_string(),
            db: db.display().to_string(),
            ok,
            doctor_status: doctor.status,
            intelligence_status: intelligence.status,
            fix_actions: fixed_doctor.fix_actions,
            issues,
        });
    }
    projects.sort_by(|left, right| left.root.cmp(&right.root));
    let attention_projects = projects.iter().filter(|project| !project.ok).count();
    Ok(ProjectWatchReport {
        version: 1,
        ok: attention_projects == 0,
        fix,
        total_projects: projects.len(),
        attention_projects,
        projects,
        recommendations: if attention_projects == 0 {
            vec!["all discovered project memories are ready".to_string()]
        } else {
            vec!["run dukememory project-watch --fix --json".to_string()]
        },
    })
}

pub(crate) fn print_autonomous_loop(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    level: AutonomousLevel,
    apply: bool,
    watch: bool,
    interval_secs: u64,
    max_runs: Option<usize>,
    json_out: bool,
) -> Result<()> {
    let max_runs = if watch { max_runs } else { Some(1) };
    let mut run_index = 0usize;
    loop {
        run_index += 1;
        let report = autonomous_loop_once_report(
            conn,
            db,
            root,
            since_days,
            level,
            apply,
            watch,
            run_index,
            if watch { Some(interval_secs) } else { None },
        )?;
        if json_out {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Autonomous Loop");
            println!("status: {}", report.status);
            println!("applied: {}", report.applied);
            println!("scheduled: {}", report.scheduled);
            for action in &report.actions {
                println!("action: {action}");
            }
            for issue in &report.issues {
                println!("issue: {issue}");
            }
            for recommendation in &report.recommendations {
                println!("recommendation: {recommendation}");
            }
        }
        if !watch || max_runs.is_some_and(|max| run_index >= max) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
    }
    Ok(())
}

pub(crate) fn autonomous_loop_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    level: AutonomousLevel,
    apply: bool,
) -> Result<AutonomousLoopReport> {
    autonomous_loop_once_report(conn, db, root, since_days, level, apply, false, 1, None)
}

fn autonomous_loop_once_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    level: AutonomousLevel,
    apply: bool,
    scheduled: bool,
    run_index: usize,
    next_interval_secs: Option<u64>,
) -> Result<AutonomousLoopReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut actions = Vec::new();
    if apply {
        let (provider, endpoint, model) = read_project_embedding_config(&root);
        let report = autonomous_run_once(
            conn,
            AutonomousRunRequest {
                level,
                status_file: &root.join(".agent/autonomous-status.json"),
                rollback_dir: &root.join(".agent/autonomous-rollbacks"),
                backup_dir: &root.join(".agent/backups"),
                backup_keep: 10,
                rollback_keep: 10,
                db,
                scope: "project",
                provider: &provider,
                endpoint: &endpoint,
                model: &model,
            },
        )?;
        actions.push(format!(
            "autonomous_run_once:ok={} actions={}",
            report.ok,
            report.actions.len()
        ));
    } else {
        actions.push("plan: run reversible autonomous run-once".to_string());
    }
    let watch = project_watch_report(db, since_days, apply)?;
    if apply {
        actions.push(format!(
            "project_watch_fix:projects={} attention={}",
            watch.total_projects, watch.attention_projects
        ));
    } else {
        actions.push("plan: project-watch --fix if attention appears".to_string());
    }
    let doctor = project_doctor_report(conn, db, &root, since_days, false)?;
    let intelligence = intelligence_dashboard_report(conn, db, &root, since_days)?;
    let mut issues = Vec::new();
    if !watch.ok {
        issues.push("project watch has attention projects".to_string());
    }
    issues.extend(doctor.issues.iter().cloned());
    issues.extend(intelligence.issues.iter().cloned());
    issues.sort();
    issues.dedup();
    let mut recommendations = Vec::new();
    if !apply {
        recommendations.push("rerun with --apply to execute reversible maintenance".to_string());
    }
    recommendations.extend(watch.recommendations.iter().cloned());
    recommendations.extend(doctor.recommendations.iter().cloned());
    recommendations.extend(intelligence.recommendations.iter().cloned());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    let report = AutonomousLoopReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        level: level.to_string(),
        applied: apply,
        scheduled,
        run_index,
        next_interval_secs,
        watch,
        doctor,
        intelligence,
        actions,
        issues,
        recommendations,
    };
    let status = if report.ok { "ok" } else { "attention" };
    let detail = serde_json::to_string(&json!({
        "status": status,
        "applied": apply,
        "scheduled": scheduled,
        "run_index": run_index,
        "actions": &report.actions,
        "issues": &report.issues,
        "attention_projects": report.watch.attention_projects,
    }))?;
    let _ = log_event(conn, "autonomous_loop", None, &detail);
    Ok(report)
}

pub(crate) fn print_action_journal(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = action_journal_report(conn, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Action Journal");
    println!("total: {}", report.total);
    println!(
        "applied={} skipped={} failed={} rollback={}",
        report.applied, report.skipped, report.failed, report.rollback_events
    );
    for item in &report.items {
        println!(
            "{} {} {} {}",
            item.status, item.event_type, item.action, item.detail
        );
    }
    Ok(())
}

pub(crate) fn action_journal_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
) -> Result<ActionJournalReport> {
    let since_ms = now_ms().saturating_sub(since_days.max(0).saturating_mul(86_400_000));
    let mut stmt = conn.prepare(
        r#"
        SELECT id, event_type, detail, created_at
        FROM memory_events
        WHERE created_at >= ?1
          AND (
            event_type LIKE 'autonomous%'
            OR event_type LIKE 'dashboard_repair%'
            OR event_type LIKE 'autopilot%'
            OR event_type LIKE 'memory_feedback%'
            OR event_type LIKE 'sync_%'
          )
        ORDER BY created_at DESC, id DESC
        LIMIT ?2
        "#,
    )?;
    let rows = stmt
        .query_map(params![since_ms, limit.min(i64::MAX as usize)], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items = Vec::new();
    for (id, event_type, detail, created_at) in rows {
        let parsed = serde_json::from_str::<Value>(&detail).ok();
        let action = parsed
            .as_ref()
            .and_then(|value| value.get("action").or_else(|| value.get("kind")))
            .and_then(Value::as_str)
            .unwrap_or(event_type.as_str())
            .to_string();
        let status = parsed
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| infer_action_status(&event_type, &detail));
        items.push(ActionJournalItem {
            id,
            event_type,
            status,
            action,
            detail: truncate_chars(&detail, 500),
            created_at,
        });
    }
    let applied = items
        .iter()
        .filter(|item| matches!(item.status.as_str(), "ok" | "applied" | "ready"))
        .count();
    let skipped = items
        .iter()
        .filter(|item| item.status.contains("skip") || item.status == "dry_run")
        .count();
    let failed = items
        .iter()
        .filter(|item| item.status.contains("fail") || item.status == "error")
        .count();
    let rollback_events = items
        .iter()
        .filter(|item| item.event_type.contains("rollback") || item.detail.contains("rollback"))
        .count();
    let mut recommendations = Vec::new();
    if failed > 0 {
        recommendations
            .push("inspect failed autonomous actions before enabling watch mode".to_string());
    }
    if rollback_events > 0 {
        recommendations
            .push("rollback metadata is available for recent autonomous cycles".to_string());
    }
    Ok(ActionJournalReport {
        version: 1,
        ok: failed == 0,
        since_days,
        total: items.len(),
        applied,
        skipped,
        failed,
        rollback_events,
        items,
        recommendations,
    })
}

fn infer_action_status(event_type: &str, detail: &str) -> String {
    let lower = format!("{event_type} {detail}").to_lowercase();
    if lower.contains("failed") || lower.contains("error") {
        "failed".to_string()
    } else if lower.contains("skipped") || lower.contains("\"dry_run\":true") {
        "skipped".to_string()
    } else {
        "ok".to_string()
    }
}

pub(crate) fn print_autonomous_watch_install(
    db: &Path,
    root: &Path,
    interval_secs: u64,
    label: &str,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = autonomous_watch_install_report(db, root, interval_secs, label, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Autonomous Watch Install");
    println!("status: {}", if report.ok { "ready" } else { "blocked" });
    println!("plist: {}", report.plist);
    println!("command: {}", report.command.join(" "));
    for action in &report.actions {
        println!("action: {action}");
    }
    Ok(())
}

pub(crate) fn autonomous_watch_install_report(
    db: &Path,
    root: &Path,
    interval_secs: u64,
    label: &str,
    dry_run: bool,
) -> Result<AutonomousWatchInstallReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let safe_label = label
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '.' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let plist = expand_tilde(&format!("~/Library/LaunchAgents/{safe_label}.plist"));
    let log_path = root.join(".agent/autonomous-loop-watch.log");
    let status_file = root.join(".agent/autonomous-status.json");
    let exe = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("dukememory"))
        .display()
        .to_string();
    let command = vec![
        exe.clone(),
        "--db".to_string(),
        db.display().to_string(),
        "autonomous-loop".to_string(),
        "--root".to_string(),
        root.display().to_string(),
        "--watch".to_string(),
        "--apply".to_string(),
        "--interval-secs".to_string(),
        interval_secs.max(60).to_string(),
        "--json".to_string(),
    ];
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{safe_label}</string>
  <key>ProgramArguments</key>
  <array>
{}
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><false/>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
        command
            .iter()
            .map(|arg| format!("    <string>{}</string>", xml_escape(arg)))
            .collect::<Vec<_>>()
            .join("\n"),
        xml_escape(&log_path.display().to_string()),
        xml_escape(&log_path.display().to_string())
    );
    let mut actions = Vec::new();
    if dry_run {
        actions.push("dry_run: plist not written".to_string());
    } else {
        write_file(&plist, content.as_bytes())?;
        actions.push(format!("plist_written:{}", plist.display()));
        actions.push(
            "load manually with launchctl load ~/Library/LaunchAgents/<label>.plist".to_string(),
        );
    }
    Ok(AutonomousWatchInstallReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        dry_run,
        label: safe_label,
        plist: plist.display().to_string(),
        command,
        interval_secs: interval_secs.max(60),
        log_path: log_path.display().to_string(),
        status_file: status_file.display().to_string(),
        actions,
    })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn print_usefulness_engine(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = usefulness_engine_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Usefulness Engine");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for item in &report.promote_candidates {
        println!("promote: {item}");
    }
    for item in &report.suppress_candidates {
        println!("suppress: {item}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn usefulness_engine_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<UsefulnessEngineReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let quality = quality_report(conn, since_days, 30)?;
    let replay = memory_replay_report(conn, since_days, 30)?;
    let auto_feedback = auto_feedback_v2_report(conn, since_days, 100, apply)?;
    let suppress_candidates = quality
        .weakest
        .iter()
        .filter(|item| item.score < 55.0 && item.request_count == 0)
        .take(10)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let promote_candidates = quality
        .strongest
        .iter()
        .filter(|item| item.request_count > 0)
        .take(10)
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let ranking_policy = vec![
        "prefer cards with recent useful reads and positive feedback".to_string(),
        "keep semantic recall enabled when eligible result rate is healthy".to_string(),
        "soft-suppress unused weak cards before deletion; never hard-delete automatically"
            .to_string(),
        "materialize inferred feedback only when --apply is explicit".to_string(),
    ];
    let mut issues = qa.issues.clone();
    issues.sort();
    issues.dedup();
    let mut recommendations = qa.recommendations.clone();
    if !apply && auto_feedback.written > 0 {
        recommendations
            .push("rerun usefulness-engine --apply to materialize inferred feedback".to_string());
    }
    if !suppress_candidates.is_empty() {
        recommendations.push(
            "review soft-suppress candidates in quality-report before changing statuses"
                .to_string(),
        );
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(UsefulnessEngineReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        qa,
        quality,
        replay,
        auto_feedback,
        ranking_policy,
        suppress_candidates,
        promote_candidates,
        issues,
        recommendations,
    })
}

pub(crate) fn print_ranking_profile(
    root: &Path,
    profile: RankingProfileMode,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = ranking_profile_report(root, profile, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Ranking Profile");
    println!("profile: {}", report.profile);
    println!("applied: {}", report.applied);
    println!("path: {}", report.path);
    Ok(())
}

pub(crate) fn ranking_profile_report(
    root: &Path,
    profile: RankingProfileMode,
    apply: bool,
) -> Result<RankingProfileReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut weights = BTreeMap::new();
    match profile {
        RankingProfileMode::Balanced => {
            weights.insert("recent_read".to_string(), 0.9);
            weights.insert("useful_feedback".to_string(), 4.0);
            weights.insert("useless_feedback".to_string(), -7.0);
        }
        RankingProfileMode::Strict => {
            weights.insert("recent_read".to_string(), 0.6);
            weights.insert("useful_feedback".to_string(), 3.0);
            weights.insert("useless_feedback".to_string(), -10.0);
        }
        RankingProfileMode::RecallHeavy => {
            weights.insert("recent_read".to_string(), 1.1);
            weights.insert("useful_feedback".to_string(), 3.5);
            weights.insert("useless_feedback".to_string(), -4.0);
        }
        RankingProfileMode::PrecisionHeavy => {
            weights.insert("recent_read".to_string(), 0.7);
            weights.insert("useful_feedback".to_string(), 5.0);
            weights.insert("useless_feedback".to_string(), -12.0);
        }
    }
    let path = root.join(".agent/ranking-profile.json");
    if apply {
        write_file(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "profile": profile.to_string(),
                "weights": &weights,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
    }
    Ok(RankingProfileReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        profile: profile.to_string(),
        applied: apply,
        path: path.display().to_string(),
        weights,
        recommendations: vec![
            "profile is read from DUKEMEMORY_RANKING_PROFILE or .agent/ranking-profile.json"
                .to_string(),
        ],
    })
}

pub(crate) fn print_project_template(
    root: &Path,
    kind: ProjectTemplateKind,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = project_template_report(root, kind, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Template");
    println!("kind: {}", report.kind);
    println!("applied: {}", report.applied);
    for command in &report.recommended_commands {
        println!("command: {command}");
    }
    Ok(())
}

pub(crate) fn project_template_report(
    root: &Path,
    kind: ProjectTemplateKind,
    apply: bool,
) -> Result<ProjectTemplateReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let (budget_profile, recommended_commands, starter_memory) = match kind {
        ProjectTemplateKind::FrontendApp => (
            "tiny",
            vec![
                "npm run build",
                "npm test",
                "dukememory impact src/App.tsx --budget-profile tiny",
            ],
            vec![
                "UI conventions",
                "Build command",
                "Known browser constraints",
            ],
        ),
        ProjectTemplateKind::RustCli => (
            "tiny",
            vec![
                "cargo check",
                "cargo test --test cli",
                "cargo build --release",
            ],
            vec![
                "CLI command surface",
                "Release gate",
                "Install/update command",
            ],
        ),
        ProjectTemplateKind::GameMod => (
            "normal",
            vec![
                "cargo test",
                "npm run build",
                "dukememory impact assets --budget-profile tiny",
            ],
            vec!["Game rules", "Asset pipeline", "Performance constraints"],
        ),
        ProjectTemplateKind::ElectronicsCad => (
            "normal",
            vec![
                "npm run build",
                "cargo test",
                "dukememory impact harness --budget-profile tiny",
            ],
            vec![
                "Harness source of truth",
                "Connector catalog constraints",
                "Export formats",
            ],
        ),
        ProjectTemplateKind::DocsResearch => (
            "tiny",
            vec![
                "dukememory brief \"research task\" --budget-profile tiny",
                "dukememory recall \"topic\" --max-chars 1200",
            ],
            vec![
                "Source policy",
                "Citation preferences",
                "Research decisions",
            ],
        ),
    };
    let recommended_commands = recommended_commands
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let starter_memory = starter_memory
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let path = root.join(".agent/project-template.json");
    let mut actions = Vec::new();
    if apply {
        write_file(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "kind": kind.to_string(),
                "budget_profile": budget_profile,
                "recommended_commands": &recommended_commands,
                "starter_memory": &starter_memory,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("template_written:{}", path.display()));
    } else {
        actions.push("dry_run: template not written".to_string());
    }
    Ok(ProjectTemplateReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        kind: kind.to_string(),
        applied: apply,
        path: path.display().to_string(),
        budget_profile: budget_profile.to_string(),
        recommended_commands,
        starter_memory,
        actions,
    })
}

pub(crate) fn print_memory_diff_review(
    conn: &Connection,
    root: &Path,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_diff_review_report(conn, root, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Diff Review");
    println!("changed_files: {}", report.changed_files.len());
    for item in &report.suggested_memory {
        println!("suggest: {item}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn memory_diff_review_report(
    conn: &Connection,
    root: &Path,
    apply: bool,
) -> Result<MemoryDiffReviewReport> {
    let diff = project_diff_report(conn, root, true)?;
    let mut suggested_memory = Vec::new();
    for file in diff.changed_files.iter().take(10) {
        suggested_memory.push(format!(
            "review durable task_state/design_note for changed file {file}"
        ));
    }
    if diff.changed_files.is_empty() {
        suggested_memory.push("no changed files detected; no memory write suggested".to_string());
    }
    let stale_memory_ids = diff
        .drift
        .stale_active
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let path = PathBuf::from(&diff.root).join(".agent/memory-diff-review.json");
    let mut actions = Vec::new();
    if apply {
        write_file(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "changed_files": &diff.changed_files,
                "suggested_memory": &suggested_memory,
                "stale_memory_ids": &stale_memory_ids,
                "conflict_count": diff.conflicts,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("review_written:{}", path.display()));
    } else {
        actions.push("dry_run: review not written".to_string());
    }
    Ok(MemoryDiffReviewReport {
        version: 1,
        ok: diff.ok,
        root: diff.root,
        applied: apply,
        changed_files: diff.changed_files,
        suggested_memory,
        stale_memory_ids,
        conflict_count: diff.conflicts,
        actions,
        recommendations: diff.recommendations,
    })
}

pub(crate) fn print_sync_latency(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    samples: usize,
    json_out: bool,
) -> Result<()> {
    let report = sync_latency_report(conn, db, root, target, samples)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Sync Latency");
    println!("status: {}", report.status);
    println!("mode: {}", report.recommended_mode);
    println!("local_read_ms: {}", report.local_read_ms);
    if let Some(ms) = report.target_write_ms {
        println!("target_write_ms: {ms}");
    }
    if let Some(ms) = report.target_read_ms {
        println!("target_read_ms: {ms}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn sync_latency_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    samples: usize,
) -> Result<SyncLatencyReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let samples = samples.clamp(1, 10);
    let remote = remote_sync_dry_run_report(conn, db, &root, 7)?;
    let db_bytes = fs::metadata(db).map(|meta| meta.len()).unwrap_or(0);
    let mut local_total = 0u128;
    for _ in 0..samples {
        let started = Instant::now();
        let _ = fs::metadata(db)?;
        local_total = local_total.saturating_add(started.elapsed().as_millis());
    }
    let local_read_ms = local_total / samples as u128;
    let mut target_write_ms = None;
    let mut target_read_ms = None;
    let target_string = target.map(|path| path.display().to_string());
    if let Some(target) = target {
        fs::create_dir_all(target)?;
        let probe = target.join(".dukememory-latency-check.tmp");
        let payload = format!("dukememory latency {}\n", now_ms());
        let mut write_total = 0u128;
        let mut read_total = 0u128;
        for _ in 0..samples {
            let started = Instant::now();
            write_file(&probe, payload.as_bytes())?;
            write_total = write_total.saturating_add(started.elapsed().as_millis());
            let started = Instant::now();
            let _ = fs::read(&probe)?;
            read_total = read_total.saturating_add(started.elapsed().as_millis());
        }
        let _ = fs::remove_file(&probe);
        target_write_ms = Some(write_total / samples as u128);
        target_read_ms = Some(read_total / samples as u128);
    }
    let measured_roundtrip = target_write_ms
        .zip(target_read_ms)
        .map(|(write, read)| write.saturating_add(read));
    let mut issues = Vec::new();
    if measured_roundtrip.is_some_and(|ms| ms > 800) || remote.estimated_roundtrip_ms > 800 {
        issues.push("remote sync latency is high for interactive reads".to_string());
    }
    let recommended_mode = if issues.is_empty() {
        "local_first_with_optional_sync".to_string()
    } else {
        "local_only_reads_remote_backup".to_string()
    };
    let mut recommendations = remote.recommendations.clone();
    recommendations.push(
        "keep agent memory reads local; sync remote/VDS in explicit push/pull steps".to_string(),
    );
    if target.is_none() {
        recommendations.push("pass --target PATH to measure a real sync directory".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(SyncLatencyReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        local_first: true,
        samples,
        local_db_bytes: db_bytes,
        local_read_ms,
        target: target_string,
        target_write_ms,
        target_read_ms,
        estimated_roundtrip_ms: remote.estimated_roundtrip_ms,
        recommended_mode,
        issues,
        recommendations,
    })
}

pub(crate) fn print_sync_profile(
    conn: &Connection,
    db: &Path,
    root: &Path,
    profile: SyncProfileMode,
    target: Option<&Path>,
    apply: bool,
    run_dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = sync_profile_report(conn, db, root, profile, target, apply, run_dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Sync Profile");
    println!("status: {}", report.status);
    println!("profile: {}", report.profile);
    println!("applied: {}", report.applied);
    for command in &report.commands {
        println!("command: {command}");
    }
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn sync_profile_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    profile: SyncProfileMode,
    target: Option<&Path>,
    apply: bool,
    run_dry_run: bool,
) -> Result<SyncProfileReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let latency = sync_latency_report(conn, db, &root, target, 1)?;
    let mut blockers = Vec::new();
    let mut recommendations = latency.recommendations.clone();
    let local_first = !matches!(profile, SyncProfileMode::RemoteShared);
    if matches!(
        profile,
        SyncProfileMode::LocalFirstBackup
            | SyncProfileMode::LocalFirstSync
            | SyncProfileMode::RemoteShared
    ) && target.is_none()
    {
        blockers.push("sync profile needs --target PATH".to_string());
    }
    if matches!(profile, SyncProfileMode::RemoteShared) {
        blockers.push("remote-shared is not enabled automatically; keep reads local-first unless latency and conflict policy are explicit".to_string());
        recommendations.push("prefer local-first-sync for agent workflows".to_string());
    }
    if !latency.ok {
        blockers.extend(latency.issues.iter().cloned());
    }
    let target_string = target.map(|path| path.display().to_string());
    let target_arg = target_string.as_deref().unwrap_or("TARGET");
    let commands = match profile {
        SyncProfileMode::LocalOnly => vec![
            "dukememory project-watch --json".to_string(),
            "dukememory memory-qa --json".to_string(),
        ],
        SyncProfileMode::LocalFirstBackup => vec![
            format!("dukememory sync push {target_arg} --dry-run --json"),
            format!("dukememory sync push {target_arg} --json"),
            format!("dukememory sync status {target_arg} --json"),
        ],
        SyncProfileMode::LocalFirstSync => vec![
            format!("dukememory sync push {target_arg} --dry-run --json"),
            format!("dukememory sync pull {target_arg} --policy manual --dry-run --json"),
            format!("dukememory sync status {target_arg} --json"),
        ],
        SyncProfileMode::RemoteShared => vec![
            format!("dukememory sync pull {target_arg} --policy manual --dry-run --json"),
            format!("dukememory sync push {target_arg} --dry-run --json"),
            "measure latency and resolve conflicts before enabling shared writes".to_string(),
        ],
    };
    let mut flow_steps = vec![SyncProfileFlowStep {
        name: "latency_check".to_string(),
        ok: latency.ok,
        detail: format!("roundtrip={}ms", latency.estimated_roundtrip_ms),
    }];
    if run_dry_run && target.is_some() {
        flow_steps.push(SyncProfileFlowStep {
            name: "push_dry_run".to_string(),
            ok: true,
            detail: "sync push dry-run command prepared; no data moved by profile planner"
                .to_string(),
        });
        if matches!(
            profile,
            SyncProfileMode::LocalFirstSync | SyncProfileMode::RemoteShared
        ) {
            flow_steps.push(SyncProfileFlowStep {
                name: "pull_dry_run".to_string(),
                ok: true,
                detail: "sync pull --policy manual dry-run command prepared".to_string(),
            });
        }
        flow_steps.push(SyncProfileFlowStep {
            name: "conflict_policy".to_string(),
            ok: !matches!(profile, SyncProfileMode::RemoteShared),
            detail: if matches!(profile, SyncProfileMode::RemoteShared) {
                "remote-shared requires explicit manual conflict review".to_string()
            } else {
                "local-first profile keeps manual/newer-wins conflict policy explicit".to_string()
            },
        });
    } else if run_dry_run {
        flow_steps.push(SyncProfileFlowStep {
            name: "dry_run_flow".to_string(),
            ok: false,
            detail: "target is required to prepare sync dry-run flow".to_string(),
        });
    }
    recommendations.push("run dry-run commands before any sync mutation".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    let mut actions = Vec::new();
    if apply && ok {
        let path = root.join(".agent/sync-profile.json");
        let value = json!({
            "version": 1,
            "profile": profile.to_string(),
            "local_first": local_first,
            "target": &target_string,
            "updated_at": now_ms(),
            "commands": &commands,
            "flow_steps": &flow_steps,
        });
        write_file(&path, serde_json::to_string_pretty(&value)?.as_bytes())?;
        actions.push(format!("sync_profile_written:{}", path.display()));
        let _ = log_event(
            conn,
            "sync_profile",
            None,
            &serde_json::to_string(&json!({
                "status": "ok",
                "profile": profile.to_string(),
                "target": &target_string,
                "path": path.display().to_string(),
            }))?,
        );
    } else if apply {
        actions.push("sync_profile_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:profile_not_written".to_string());
    }
    Ok(SyncProfileReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        profile: profile.to_string(),
        applied: apply && ok,
        local_first,
        target: target_string,
        latency,
        commands,
        flow_steps,
        actions,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_agent_enforce(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    fix: bool,
    json_out: bool,
) -> Result<()> {
    let report = agent_enforce_report(conn, db, root, since_days, fix)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Agent Enforce");
    println!("status: {}", report.status);
    println!("fixed: {}", report.fixed);
    for missing in &report.missing_commands {
        println!("missing: {missing}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn agent_enforce_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    fix: bool,
) -> Result<AgentEnforceReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let doctor = project_doctor_report(conn, db, &root, since_days, fix)?;
    let required_commands = agent_required_commands()
        .iter()
        .map(|command| (*command).to_string())
        .collect::<Vec<_>>();
    let agents_content = fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let skill_content = fs::read_to_string(expand_tilde("~/.codex/skills/dukememory-use/SKILL.md"))
        .unwrap_or_default();
    let missing_commands = required_commands
        .iter()
        .filter(|command| {
            !agents_content.contains(command.as_str()) || !skill_content.contains(command.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut issues = doctor.issues.clone();
    if !missing_commands.is_empty() {
        issues.push("agent memory instructions are missing required commands".to_string());
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = doctor.recommendations.clone();
    if !fix && (!doctor.ok || !missing_commands.is_empty()) {
        recommendations.push("rerun agent-enforce --fix --json".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(AgentEnforceReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        fixed: fix,
        required_commands,
        missing_commands,
        doctor,
        issues,
        recommendations,
    })
}

fn agent_required_commands() -> &'static [&'static str] {
    &[
        "brief",
        "impact",
        "memory-qa",
        "usage-report",
        "decision-trace",
        "auto-feedback",
        "cost-guard",
        "intelligence-dashboard",
        "project-diff",
        "remote-sync-dry-run",
        "doctor-project",
        "release-gate",
        "memory-replay",
        "project-watch",
        "autonomous-loop",
        "autonomous-watch-install",
        "action-journal",
        "usefulness-engine",
        "ranking-profile",
        "project-template",
        "sync-latency",
        "sync-profile",
        "memory-diff-review",
        "agent-enforce",
        "upgrade-project",
    ]
}

pub(crate) fn print_auto_feedback_v2(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = auto_feedback_v2_report(conn, since_days, limit, !dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Auto Feedback v2");
    println!("applied: {}", report.applied);
    println!("scanned: {}", report.scanned);
    println!("written: {}", report.written);
    println!("useful: {}", report.useful);
    println!("missing: {}", report.missing);
    println!("skipped: {}", report.skipped);
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn auto_feedback_v2_report(
    conn: &Connection,
    since_days: i64,
    limit: usize,
    apply: bool,
) -> Result<AutoFeedbackV2Report> {
    let before = live_eval_report(conn, since_days)?;
    let mut recommendations = Vec::new();
    let materialized = if apply {
        materialize_inferred_feedback(conn, since_days, limit)?
    } else {
        let usage = usage_report(conn, since_days, limit)?;
        let scanned = usage
            .recent_reads
            .iter()
            .filter(|event| auto_feedback_memory_read_command(&event.command))
            .count();
        InferredFeedbackReport {
            version: 1,
            since_days,
            scanned,
            written: 0,
            useful: 0,
            missing: before.inferred_missing,
            skipped: scanned,
        }
    };
    let after = if apply {
        live_eval_report(conn, since_days)?
    } else {
        before.clone()
    };
    let closed_missing = before
        .inferred_missing
        .saturating_sub(after.inferred_missing);
    if materialized.written == 0 {
        recommendations.push("no new feedback was needed for recent memory reads".to_string());
    }
    if after.inferred_missing > 0 {
        recommendations
            .push("promote repeated missing context into durable memory cards".to_string());
    }
    Ok(AutoFeedbackV2Report {
        version: 1,
        since_days,
        applied: apply,
        scanned: materialized.scanned,
        written: materialized.written,
        useful: materialized.useful,
        missing: materialized.missing,
        skipped: materialized.skipped,
        useful_rate_before: before.useful_rate,
        useful_rate_after: after.useful_rate,
        inferred_missing_before: before.inferred_missing,
        inferred_missing_after: after.inferred_missing,
        closed_missing,
        unresolved_missing_queries: after.inferred_missing_queries,
        recommendations,
    })
}

pub(crate) fn print_cost_guard(conn: &Connection, since_days: i64, json_out: bool) -> Result<()> {
    let report = cost_guard_report(conn, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Cost Guard");
    println!("score: {:.1}", report.score);
    println!("recommended_profile: {}", report.recommended_profile);
    println!("recommended_max_chars: {}", report.recommended_max_chars);
    println!("average_read_budget: {:.0}", report.average_read_budget);
    println!("write_pressure: {:.2}", report.write_pressure);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for action in &report.actions {
        println!("action: {action}");
    }
    Ok(())
}

pub(crate) fn cost_guard_report(conn: &Connection, since_days: i64) -> Result<CostGuardReport> {
    let usage = usage_report(conn, since_days, 50)?;
    let quality = quality_report(conn, since_days, 50)?;
    let roi = roi_report(conn, since_days)?;
    let total_budget = usage
        .recent_reads
        .iter()
        .map(|event| event.budget)
        .sum::<usize>();
    let average_read_budget = if usage.recent_reads.is_empty() {
        0.0
    } else {
        total_budget as f64 / usage.recent_reads.len() as f64
    };
    let max_read_budget = usage
        .recent_reads
        .iter()
        .map(|event| event.budget)
        .max()
        .unwrap_or(0);
    let large_memory_count = quality
        .items
        .iter()
        .filter(|item| item.body_chars > 1200)
        .count();
    let noisy_memory_count = roi.noisy_memory_ids.len();
    let mut issues = Vec::new();
    let mut actions = Vec::new();
    if average_read_budget > 4_000.0 {
        issues.push(format!(
            "average read budget is high: {:.0}",
            average_read_budget
        ));
        actions.push("prefer brief/impact tiny budgets before recall/context-pack".to_string());
    }
    if max_read_budget > 8_000 {
        issues.push(format!("max read budget is high: {max_read_budget}"));
        actions.push("cap broad context calls unless a risky migration needs them".to_string());
    }
    if usage.write_pressure > 2.0 && usage.read_count >= 20 {
        issues.push(format!(
            "write pressure is high: {:.2}",
            usage.write_pressure
        ));
        actions.push(
            "allow autonomous cleanup/compaction before adding more task_state cards".to_string(),
        );
    }
    if large_memory_count > 0 {
        actions.push("slim long memory cards or move detail to linked files".to_string());
    }
    if noisy_memory_count > 0 {
        actions.push("suppress or fix memories with negative feedback".to_string());
    }
    let recommended_profile = if issues.is_empty() && usage.semantic_eligible_result_rate >= 0.95 {
        "tiny"
    } else if average_read_budget <= 4_000.0 {
        "normal"
    } else {
        "tight"
    }
    .to_string();
    let recommended_max_chars = match recommended_profile.as_str() {
        "tiny" => 1200,
        "tight" => 3000,
        _ => 4000,
    };
    let mut score = 100.0;
    score -= ((average_read_budget / 1000.0) - 2.0).max(0.0).min(8.0) * 4.0;
    score -= (usage.write_pressure - 1.5).max(0.0).min(3.0) * 8.0;
    score -= large_memory_count.min(5) as f64 * 4.0;
    score -= noisy_memory_count.min(5) as f64 * 5.0;
    score = score.clamp(0.0, 100.0);
    actions.sort();
    actions.dedup();
    Ok(CostGuardReport {
        version: 1,
        since_days,
        score,
        recommended_profile,
        recommended_max_chars,
        average_read_budget,
        max_read_budget,
        write_pressure: usage.write_pressure,
        token_saving_estimate: roi.token_saving_estimate,
        large_memory_count,
        noisy_memory_count,
        guard_active: true,
        issues,
        actions,
    })
}

pub(crate) fn print_project_diff(
    conn: &Connection,
    root: &Path,
    changed_only: bool,
    json_out: bool,
) -> Result<()> {
    let report = project_diff_report(conn, root, changed_only)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Intelligence Diff");
    println!("ok: {}", report.ok);
    println!("changed_files: {}", report.changed_files.len());
    println!("missing_links: {}", report.missing_links);
    println!("conflicts: {}", report.conflicts);
    println!("stale_active: {}", report.stale_active);
    for file in &report.changed_files {
        println!("changed: {file}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn project_diff_report(
    conn: &Connection,
    root: &Path,
    changed_only: bool,
) -> Result<ProjectDiffReport> {
    let drift = drift_report(conn, root, changed_only)?;
    let since_ms = now_ms().saturating_sub(86_400_000);
    let mut stmt = conn.prepare(
        "SELECT id FROM memories WHERE updated_at >= ?1 ORDER BY updated_at DESC LIMIT 20",
    )?;
    let new_or_changed_memory_ids = stmt
        .query_map(params![since_ms], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut recommendations = Vec::new();
    if !drift.missing_links.is_empty() {
        recommendations.push("repair or remove memory links pointing at missing files".to_string());
    }
    if !drift.conflicts.is_empty() {
        recommendations.push("merge or supersede duplicate decision candidates".to_string());
    }
    if !drift.stale_active.is_empty() {
        recommendations.push("mark stale active cards uncertain or superseded".to_string());
    }
    if drift.changed_files.is_empty() && changed_only {
        recommendations.push("no changed files detected; memory diff is stable".to_string());
    }
    Ok(ProjectDiffReport {
        version: 1,
        ok: drift.ok,
        root: drift.root.clone(),
        changed_only,
        changed_files: drift.changed_files.clone(),
        missing_links: drift.missing_links.len(),
        conflicts: drift.conflicts.len(),
        stale_active: drift.stale_active.len(),
        new_or_changed_memory_ids,
        drift,
        recommendations,
    })
}

pub(crate) fn print_intelligence_dashboard(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = intelligence_dashboard_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Intelligence Dashboard");
    println!("status: {}", report.status);
    println!("roi: {:.1}", report.roi.score);
    println!("agent_audit: {:.1}", report.agent_audit.score);
    println!("cost_guard: {:.1}", report.cost_guard.score);
    println!("trace_reads: {}", report.decision_trace.traced_reads);
    println!("remote_sync: {}", report.remote_sync.status);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn intelligence_dashboard_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<IntelligenceDashboardReport> {
    let roi = roi_report(conn, since_days)?;
    let agent_audit = agent_audit_report(conn, since_days)?;
    let cost_guard = cost_guard_report(conn, since_days)?;
    let decision_trace = decision_trace_report(conn, since_days, 12)?;
    let auto_feedback = auto_feedback_v2_report(conn, since_days, 100, false)?;
    let project_diff = project_diff_report(conn, root, true)?;
    let remote_sync = remote_sync_dry_run_report(conn, db, root, since_days)?;
    let mut issues = Vec::new();
    issues.extend(roi.issues.iter().cloned());
    issues.extend(agent_audit.issues.iter().cloned());
    issues.extend(cost_guard.issues.iter().cloned());
    issues.extend(decision_trace.issues.iter().cloned());
    if !project_diff.ok {
        issues.push("project diff needs attention".to_string());
    }
    if !remote_sync.ok {
        issues.push("remote sync dry-run is blocked".to_string());
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = Vec::new();
    recommendations.extend(roi.recommendations.iter().cloned());
    recommendations.extend(agent_audit.recommendations.iter().cloned());
    recommendations.extend(cost_guard.actions.iter().cloned());
    recommendations.extend(project_diff.recommendations.iter().cloned());
    recommendations.extend(remote_sync.recommendations.iter().cloned());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(IntelligenceDashboardReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        roi,
        agent_audit,
        cost_guard,
        decision_trace,
        auto_feedback,
        project_diff,
        remote_sync,
        issues,
        recommendations,
    })
}

pub(crate) fn print_project_doctor(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    fix: bool,
    json_out: bool,
) -> Result<()> {
    let report = project_doctor_report(conn, db, root, since_days, fix)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Memory Doctor");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!(
            "{} {} {}",
            if check.ok { "ok" } else { "warn" },
            check.name,
            check.detail
        );
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn project_doctor_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    fix: bool,
) -> Result<ProjectDoctorReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut fix_actions = Vec::new();
    if fix {
        match write_project_config(
            &root.join(".agent/config.toml"),
            db,
            &read_project_embedding_config(&root).0,
            &read_project_embedding_config(&root).1,
            &read_project_embedding_config(&root).2,
        ) {
            Ok(()) => fix_actions.push("project_config".to_string()),
            Err(err) => fix_actions.push(format!("project_config_failed:{err}")),
        }
        match write_workspace_rules(&root, true) {
            Ok(path) => fix_actions.push(format!("workspace_rules:{}", path.display())),
            Err(err) => fix_actions.push(format!("workspace_rules_failed:{err}")),
        }
        match upsert_project_agents(&root) {
            Ok(()) => fix_actions.push("agents_block".to_string()),
            Err(err) => fix_actions.push(format!("agents_block_failed:{err}")),
        }
        match write_codex_skill(&expand_tilde("~/.codex/skills"), true) {
            Ok(path) => fix_actions.push(format!("codex_skill:{}", path.display())),
            Err(err) => fix_actions.push(format!("codex_skill_failed:{err}")),
        }
        match memory_contract_report(conn, &root, true) {
            Ok(_) => fix_actions.push("memory_contract".to_string()),
            Err(err) => fix_actions.push(format!("memory_contract_failed:{err}")),
        }
    }
    let qa = memory_qa_report(conn, &root, since_days)?;
    let integration = ops_agent_integration_status(db, &root);
    let (provider, endpoint, model) = read_project_embedding_config(&root);
    let embedding = embeddings::embed_status(conn, &provider, &endpoint, &model).ok();
    if fix
        && embedding
            .as_ref()
            .is_some_and(|report| report.missing > 0 || report.stale > 0)
    {
        match embeddings::embed_index(conn, &provider, &endpoint, &model, &[], None, false) {
            Ok(report) => fix_actions.push(format!(
                "embed_index:indexed={} skipped={}",
                report.indexed, report.skipped
            )),
            Err(err) => fix_actions.push(format!("embed_index_failed:{err}")),
        }
    }
    let embedding = embeddings::embed_status(conn, &provider, &endpoint, &model).ok();
    let embedding_status = embedding.as_ref().map(|report| DoctorEmbeddingStatus {
        provider: report.provider.clone(),
        endpoint: report.endpoint.clone(),
        model: report.model.clone(),
        eligible: report.eligible,
        indexed: report.indexed,
        missing: report.missing,
        stale: report.stale,
        provider_reachable: report.provider_reachable,
    });
    let agents_content = fs::read_to_string(root.join("AGENTS.md")).unwrap_or_default();
    let skill_content = fs::read_to_string(expand_tilde("~/.codex/skills/dukememory-use/SKILL.md"))
        .unwrap_or_default();
    let required_commands = agent_required_commands();
    let agents_commands_ok = required_commands
        .iter()
        .all(|command| agents_content.contains(command));
    let skill_commands_ok = required_commands
        .iter()
        .all(|command| skill_content.contains(command));
    let embedding_current = embedding
        .as_ref()
        .is_some_and(|report| report.missing == 0 && report.stale == 0);
    let embedding_reachable = embedding
        .as_ref()
        .is_some_and(|report| report.provider_reachable);
    let autonomous_status =
        read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
    if fix && autonomous_status.as_ref().is_some_and(|report| !report.ok) {
        let output = ProcessCommand::new(std::env::current_exe()?)
            .arg("--db")
            .arg(db)
            .arg("autonomous")
            .arg("run-once")
            .arg("--level")
            .arg("normal")
            .arg("--json")
            .current_dir(&root)
            .output();
        match output {
            Ok(output) if output.status.success() => {
                fix_actions.push("autonomous_run_once".to_string())
            }
            Ok(output) => {
                let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let detail = if detail.is_empty() {
                    format!("{}", output.status)
                } else {
                    format!("{}: {}", output.status, tail_chars(&detail, 500))
                };
                fix_actions.push(format!("autonomous_run_once_failed:{detail}"));
            }
            Err(err) => fix_actions.push(format!("autonomous_run_once_failed:{err}")),
        }
    }
    let autonomous_status =
        read_autonomous_status(&root.join(".agent/autonomous-status.json")).ok();
    let autonomous_ok = autonomous_status
        .as_ref()
        .map(|report| report.ok)
        .unwrap_or(true);
    let checks = vec![
        ProjectDoctorCheck {
            name: "project_memory_db".to_string(),
            ok: integration.project_memory_installed,
            detail: db.display().to_string(),
        },
        ProjectDoctorCheck {
            name: "project_config".to_string(),
            ok: integration.project_config_present,
            detail: root.join(".agent/config.toml").display().to_string(),
        },
        ProjectDoctorCheck {
            name: "agents_block".to_string(),
            ok: integration.agents_block_present && agents_commands_ok,
            detail: "AGENTS.md contains dukememory. block and required commands".to_string(),
        },
        ProjectDoctorCheck {
            name: "codex_skill".to_string(),
            ok: integration.skill_installed && skill_commands_ok,
            detail: integration.skill_path.clone(),
        },
        ProjectDoctorCheck {
            name: "mcp_config".to_string(),
            ok: integration.codex_mcp_configured,
            detail: integration.codex_config.clone(),
        },
        ProjectDoctorCheck {
            name: "memory_qa".to_string(),
            ok: qa.ok,
            detail: format!("score={:.1}", qa.score),
        },
        ProjectDoctorCheck {
            name: "embeddings".to_string(),
            ok: embedding_current && embedding_reachable,
            detail: embedding
                .as_ref()
                .map(|report| {
                    format!(
                        "provider={} model={} missing={} stale={} reachable={}",
                        report.provider,
                        report.model,
                        report.missing,
                        report.stale,
                        report.provider_reachable
                    )
                })
                .unwrap_or_else(|| "embedding status unavailable".to_string()),
        },
        ProjectDoctorCheck {
            name: "autonomous_status".to_string(),
            ok: autonomous_ok,
            detail: autonomous_status
                .as_ref()
                .map(|report| format!("ok={} updated_at={}", report.ok, report.updated_at))
                .unwrap_or_else(|| {
                    "no status yet; run autonomous run-once for telemetry".to_string()
                }),
        },
    ];
    let mut issues = qa.issues.clone();
    let mut recommendations = qa.recommendations.clone();
    for check in &checks {
        if !check.ok && check.name != "mcp_config" {
            issues.push(format!("{} is not ready", check.name));
        }
        if !check.ok {
            match check.name.as_str() {
                "agents_block" | "project_config" => {
                    recommendations.push("run dukememory upgrade-project --json".to_string())
                }
                "codex_skill" => recommendations.push("run dukememory install-skill".to_string()),
                "embeddings" => recommendations.push("run dukememory embed-index".to_string()),
                "autonomous_status" => recommendations
                    .push("run dukememory autonomous run-once --level normal --json".to_string()),
                _ => {}
            }
        }
    }
    issues.sort();
    issues.dedup();
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(ProjectDoctorReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        db: db.display().to_string(),
        fixed: fix,
        fix_actions,
        checks,
        memory_qa: qa,
        embedding: embedding_status,
        issues,
        recommendations,
    })
}

pub(crate) fn print_release_gate(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
    json_out: bool,
) -> Result<()> {
    let report = release_gate_report(conn, db, root, since_days, strict, run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Release Gate");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!(
            "{} {} {}",
            if check.ok { "ok" } else { "warn" },
            check.name,
            check.detail
        );
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn release_gate_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
) -> Result<ReleaseGateReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let doctor = project_doctor_report(conn, db, &root, since_days, false)?;
    let intelligence = intelligence_dashboard_report(conn, db, &root, since_days)?;
    let autonomous_loop =
        autonomous_loop_report(conn, db, &root, since_days, AutonomousLevel::Normal, false)?;
    let usefulness_engine = usefulness_engine_report(conn, &root, since_days, false)?;
    let sync_latency = sync_latency_report(conn, db, &root, None, 1)?;
    let action_journal = action_journal_report(conn, since_days, 30)?;
    let sync_profile = sync_profile_report(
        conn,
        db,
        &root,
        SyncProfileMode::LocalFirstBackup,
        None,
        false,
        false,
    )?;
    let agent_enforce = agent_enforce_report(conn, db, &root, since_days, false)?;
    let project_diff = project_diff_report(conn, &root, true)?;
    let cargo_toml = root.join("Cargo.toml");
    let cargo_version_ok = fs::read_to_string(&cargo_toml)
        .map(|content| content.contains(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION"))))
        .unwrap_or(false);
    let git_status = ProcessCommand::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(&root)
        .output()
        .ok();
    let git_clean = git_status
        .as_ref()
        .filter(|output| output.status.success())
        .map(|output| output.stdout.is_empty())
        .unwrap_or(true);
    let mut commands = Vec::new();
    if run {
        commands.push(run_release_gate_command(
            &root,
            "fmt",
            &["cargo", "fmt", "--check"],
        ));
        commands.push(run_release_gate_command(
            &root,
            "check",
            &["cargo", "check"],
        ));
        commands.push(run_release_gate_command(
            &root,
            "test_cli",
            &["cargo", "test", "--test", "cli"],
        ));
        commands.push(run_release_gate_command(
            &root,
            "build_release",
            &["cargo", "build", "--release"],
        ));
    }
    let commands_ok = commands.iter().all(|command| command.ok);
    let checks = vec![
        ReleaseGateCheck {
            name: "doctor_project".to_string(),
            ok: doctor.ok,
            required: true,
            detail: doctor.status.clone(),
        },
        ReleaseGateCheck {
            name: "intelligence_dashboard".to_string(),
            ok: intelligence.ok,
            required: true,
            detail: intelligence.status.clone(),
        },
        ReleaseGateCheck {
            name: "project_diff".to_string(),
            ok: project_diff.ok,
            required: true,
            detail: format!(
                "changed={} missing_links={} conflicts={} stale={}",
                project_diff.changed_files.len(),
                project_diff.missing_links,
                project_diff.conflicts,
                project_diff.stale_active
            ),
        },
        ReleaseGateCheck {
            name: "autonomous_loop".to_string(),
            ok: autonomous_loop.ok,
            required: true,
            detail: autonomous_loop.status.clone(),
        },
        ReleaseGateCheck {
            name: "usefulness_engine".to_string(),
            ok: usefulness_engine.ok,
            required: true,
            detail: usefulness_engine.status.clone(),
        },
        ReleaseGateCheck {
            name: "sync_latency".to_string(),
            ok: sync_latency.ok,
            required: true,
            detail: sync_latency.recommended_mode.clone(),
        },
        ReleaseGateCheck {
            name: "action_journal".to_string(),
            ok: action_journal.ok,
            required: true,
            detail: format!(
                "events={} failed={} rollback={}",
                action_journal.total, action_journal.failed, action_journal.rollback_events
            ),
        },
        ReleaseGateCheck {
            name: "sync_profile".to_string(),
            ok: sync_profile.ok
                || (sync_profile.blockers.len() == 1
                    && sync_profile.blockers[0] == "sync profile needs --target PATH"),
            required: true,
            detail: sync_profile.profile.clone(),
        },
        ReleaseGateCheck {
            name: "agent_enforce".to_string(),
            ok: agent_enforce.ok,
            required: true,
            detail: agent_enforce.status.clone(),
        },
        ReleaseGateCheck {
            name: "cargo_version".to_string(),
            ok: cargo_version_ok,
            required: true,
            detail: cargo_toml.display().to_string(),
        },
        ReleaseGateCheck {
            name: "git_clean".to_string(),
            ok: git_clean,
            required: strict,
            detail: if git_clean {
                "working tree clean".to_string()
            } else {
                "working tree has local changes".to_string()
            },
        },
        ReleaseGateCheck {
            name: "required_commands".to_string(),
            ok: !run || commands_ok,
            required: run,
            detail: if run {
                format!("executed={} ok={}", commands.len(), commands_ok)
            } else {
                "run: cargo fmt --check; cargo check; cargo test --test cli; cargo build --release"
                    .to_string()
            },
        },
    ];
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();
    for check in &checks {
        if check.required && !check.ok {
            issues.push(format!("release gate failed: {}", check.name));
        }
    }
    if !git_clean && !strict {
        recommendations.push("commit or stash local changes before publishing".to_string());
    }
    recommendations.extend(doctor.recommendations.iter().cloned());
    recommendations.extend(intelligence.recommendations.iter().cloned());
    recommendations.extend(autonomous_loop.recommendations.iter().cloned());
    recommendations.extend(usefulness_engine.recommendations.iter().cloned());
    recommendations.extend(sync_latency.recommendations.iter().cloned());
    recommendations.extend(action_journal.recommendations.iter().cloned());
    recommendations.extend(sync_profile.recommendations.iter().cloned());
    recommendations.extend(agent_enforce.recommendations.iter().cloned());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(ReleaseGateReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        strict,
        run,
        checks,
        commands,
        doctor,
        intelligence,
        autonomous_loop,
        usefulness_engine,
        sync_latency,
        action_journal,
        sync_profile,
        agent_enforce,
        issues,
        recommendations,
    })
}

fn run_release_gate_command(root: &Path, name: &str, command: &[&str]) -> ReleaseGateCommandResult {
    let started = Instant::now();
    let output = if let Some((program, args)) = command.split_first() {
        ProcessCommand::new(program)
            .args(args)
            .current_dir(root)
            .output()
    } else {
        return ReleaseGateCommandResult {
            name: name.to_string(),
            command: String::new(),
            ok: false,
            exit_code: None,
            elapsed_ms: 0,
            output_tail: "empty command".to_string(),
        };
    };
    match output {
        Ok(output) => {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&output.stdout));
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            ReleaseGateCommandResult {
                name: name.to_string(),
                command: command.join(" "),
                ok: output.status.success(),
                exit_code: output.status.code(),
                elapsed_ms: started.elapsed().as_millis(),
                output_tail: tail_chars(&combined, 1200),
            }
        }
        Err(err) => ReleaseGateCommandResult {
            name: name.to_string(),
            command: command.join(" "),
            ok: false,
            exit_code: None,
            elapsed_ms: started.elapsed().as_millis(),
            output_tail: err.to_string(),
        },
    }
}

fn tail_chars(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    chars[chars.len().saturating_sub(max_chars)..]
        .iter()
        .collect()
}

fn memory_title_map(conn: &Connection) -> Result<HashMap<String, String>> {
    let rows = query_memories(
        conn,
        None,
        &[],
        &[
            "active".to_string(),
            "uncertain".to_string(),
            "superseded".to_string(),
            "rejected".to_string(),
        ],
        None,
        usize::MAX,
    )?;
    Ok(rows
        .into_iter()
        .map(|memory| (memory.id, memory.title))
        .collect())
}

fn auto_feedback_memory_read_command(command: &str) -> bool {
    matches!(
        command,
        "brief"
            | "impact"
            | "retrieve"
            | "recall"
            | "search"
            | "context"
            | "context-pack"
            | "memory_brief"
            | "memory_impact"
            | "memory_search"
            | "memory_context_pack"
            | "memory_agent_context"
            | "memory_snapshot"
    )
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

pub(crate) fn print_remote_sync_dry_run(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_dry_run_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync Dry-Run");
    println!("status: {}", report.status);
    println!("local_first: {}", report.local_first);
    println!("db_bytes: {}", report.db_bytes);
    println!("estimated_roundtrip_ms: {}", report.estimated_roundtrip_ms);
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

pub(crate) fn remote_sync_dry_run_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<RemoteSyncDryRunReport> {
    let remote = remote_status_report(conn, db, root, since_days)?;
    let ops = ops_status_report(conn, db, root, since_days)?;
    let db_bytes = ops.storage.db_bytes;
    let estimated_export_bytes = db_bytes.saturating_add(db_bytes / 8);
    let transfer_units = (estimated_export_bytes / 256_000).min(u32::MAX as u64) as u32;
    let estimated_upload_ms = 50u32.saturating_add(transfer_units.saturating_mul(12));
    let estimated_download_ms = 50u32.saturating_add(transfer_units.saturating_mul(12));
    let estimated_roundtrip_ms = estimated_upload_ms
        .saturating_add(estimated_download_ms)
        .saturating_add(remote.estimated_vds_latency_ms);
    let mut blockers = remote.blockers.clone();
    if !remote.embedding_current {
        blockers.push("embedding index is not current before sync".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let mut recommendations = remote.recommendations.clone();
    recommendations
        .push("run sync export/import in dry-run until conflict policy is explicit".to_string());
    recommendations
        .push("keep agent reads local unless measured VDS roundtrip is acceptable".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    Ok(RemoteSyncDryRunReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        local_first: true,
        db_bytes,
        estimated_export_bytes,
        estimated_upload_ms,
        estimated_download_ms,
        estimated_roundtrip_ms,
        export_command: remote.export_command,
        import_command: remote.import_command,
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

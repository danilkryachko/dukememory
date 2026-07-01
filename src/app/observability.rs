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
pub(crate) struct ContextGovernorReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) task: String,
    pub(crate) target: Option<String>,
    pub(crate) selected_flow: Vec<String>,
    pub(crate) budget: BudgetPlan,
    pub(crate) max_context_chars: usize,
    pub(crate) commands: Vec<String>,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryRouterReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) include_siblings: bool,
    pub(crate) routes: Vec<MemoryRouteItem>,
    pub(crate) selected_route: Option<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryRouteItem {
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) current: bool,
    pub(crate) mode: String,
    pub(crate) matches: usize,
    pub(crate) top_titles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutoRankingTuneReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) selected_profile: String,
    pub(crate) applied: bool,
    pub(crate) qa_score: f64,
    pub(crate) useful_rate: f64,
    pub(crate) inferred_missing: usize,
    pub(crate) semantic_empty: usize,
    pub(crate) noisy_cards: usize,
    pub(crate) reasons: Vec<String>,
    pub(crate) ranking: RankingProfileReport,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryHealthScoreReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) grade: String,
    pub(crate) qa_score: f64,
    pub(crate) roi_score: f64,
    pub(crate) agent_score: f64,
    pub(crate) cost_score: f64,
    pub(crate) autonomy_ready: bool,
    pub(crate) semantic_ready: bool,
    pub(crate) write_pressure: f64,
    pub(crate) token_saving_estimate: usize,
    pub(crate) components: Vec<MemoryHealthComponent>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryHealthComponent {
    pub(crate) name: String,
    pub(crate) score: f64,
    pub(crate) weight: f64,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExplainRecallReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) limit: usize,
    pub(crate) hits: Vec<ExplainRecallHit>,
    pub(crate) missing_signals: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExplainRecallHit {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) confidence: f64,
    pub(crate) score: f64,
    pub(crate) matched_terms: Vec<String>,
    pub(crate) reasons: Vec<String>,
    pub(crate) explanation: String,
    pub(crate) preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProjectIntentMapReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) goals: Vec<IntentMapItem>,
    pub(crate) decisions: Vec<IntentMapItem>,
    pub(crate) constraints: Vec<IntentMapItem>,
    pub(crate) commands: Vec<IntentMapItem>,
    pub(crate) risks: Vec<IntentMapItem>,
    pub(crate) active_tasks: Vec<IntentMapItem>,
    pub(crate) contract_path: String,
    pub(crate) contract_preview: String,
    pub(crate) recommended_start_flow: Vec<String>,
    pub(crate) gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct IntentMapItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryTestHarnessReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) probes: Vec<MemoryTestProbe>,
    pub(crate) failures: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryTestProbe {
    pub(crate) query: String,
    pub(crate) expected_type: Option<String>,
    pub(crate) expected_id: Option<String>,
    pub(crate) found: bool,
    pub(crate) matched_id: Option<String>,
    pub(crate) matched_title: Option<String>,
    pub(crate) result_count: usize,
    pub(crate) explanation: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AgentAuditV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) base_score: f64,
    pub(crate) disciplined_reads: bool,
    pub(crate) semantic_effective: bool,
    pub(crate) writes_controlled: bool,
    pub(crate) feedback_loop_active: bool,
    pub(crate) trace_explainable: bool,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
    pub(crate) audit: AgentAuditReport,
    pub(crate) trace: DecisionTraceReport,
    pub(crate) cost_guard: CostGuardReport,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryControlCenterV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) health: MemoryHealthScoreReport,
    pub(crate) intent_map: ProjectIntentMapReport,
    pub(crate) test_harness: MemoryTestHarnessReport,
    pub(crate) agent_audit_v2: AgentAuditV2Report,
    pub(crate) explain_recall: ExplainRecallReport,
    pub(crate) autonomy: AutonomyControlCenterReport,
    pub(crate) next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AutoSupersedeV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) applied: bool,
    pub(crate) candidates: Vec<AutoSupersedeCandidate>,
    pub(crate) superseded: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) rollback_hint: String,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AutoSupersedeCandidate {
    pub(crate) primary_id: String,
    pub(crate) duplicate_id: String,
    pub(crate) title: String,
    pub(crate) reason: String,
    pub(crate) confidence: f64,
    pub(crate) safe_to_apply: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryDiffApplyReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) reviewed: MemoryDiffReviewReport,
    pub(crate) written_ids: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RecallBenchmarkBaseline {
    pub(crate) version: u32,
    pub(crate) score: f64,
    pub(crate) probe_count: usize,
    pub(crate) written_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RecallBenchmarkSuiteReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) baseline_score: Option<f64>,
    pub(crate) regression: bool,
    pub(crate) baseline_written: bool,
    pub(crate) baseline_path: String,
    pub(crate) harness: MemoryTestHarnessReport,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReleaseGateV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) strict: bool,
    pub(crate) run: bool,
    pub(crate) base_gate: ReleaseGateReport,
    pub(crate) health: MemoryHealthScoreReport,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) audit_v2: AgentAuditV2Report,
    pub(crate) control_center: MemoryControlCenterV2Report,
    pub(crate) checks: Vec<ReleaseGateCheck>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncWizardReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) steps: Vec<RemoteSyncWizardStep>,
    pub(crate) sync: RemoteSyncV2Report,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RemoteSyncWizardStep {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryGovernancePolicyReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) path: String,
    pub(crate) auto_write_types: Vec<String>,
    pub(crate) auto_supersede_min_confidence: f64,
    pub(crate) diff_apply_min_confidence: f64,
    pub(crate) max_write_pressure: f64,
    pub(crate) require_embeddings_for_semantic: bool,
    pub(crate) remote_sync_mode: String,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomousLoopV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) governance: MemoryGovernancePolicyReport,
    pub(crate) enforcement: GovernanceEnforceReport,
    pub(crate) loop_v1: AutonomousLoopReport,
    pub(crate) supersede: AutoSupersedeV2Report,
    pub(crate) diff_apply: MemoryDiffApplyReport,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) release_gate_v2: ReleaseGateV2Report,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GovernanceEnforceReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) policy: MemoryGovernancePolicyReport,
    pub(crate) write_pressure: f64,
    pub(crate) embedding_required_ok: bool,
    pub(crate) remote_mode_ok: bool,
    pub(crate) allowed_auto_write_types: Vec<String>,
    pub(crate) violations: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryQualityCiReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) minimal: bool,
    pub(crate) health_score: f64,
    pub(crate) benchmark_score: f64,
    pub(crate) audit_score: f64,
    pub(crate) failed_checks: Vec<String>,
    pub(crate) release_gate_v2: Option<ReleaseGateV2Report>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FleetDashboardV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) since_days: i64,
    pub(crate) total_projects: usize,
    pub(crate) ready_projects: usize,
    pub(crate) attention_projects: usize,
    pub(crate) projects: Vec<FleetDashboardV2Project>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FleetDashboardV2Project {
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) status: String,
    pub(crate) health_score: f64,
    pub(crate) benchmark_score: f64,
    pub(crate) audit_score: f64,
    pub(crate) issues: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncApplyFlowReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) wizard: RemoteSyncWizardReport,
    pub(crate) dry_run_commands: Vec<String>,
    pub(crate) commands: Vec<String>,
    pub(crate) passphrase_ready: bool,
    pub(crate) apply_allowed: bool,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpToolSurfaceV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) expected_tools: Vec<String>,
    pub(crate) exposed_tools: Vec<String>,
    pub(crate) missing_tools: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutopilotV3Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) since_days: i64,
    pub(crate) learning: SelfLearningRetrievalReport,
    pub(crate) role_profile: ProjectRoleProfileReport,
    pub(crate) inbox_reviewer: InboxAiReviewerReport,
    pub(crate) loop_v2: AutonomousLoopV2Report,
    pub(crate) web_control: WebControlCenterV3Report,
    pub(crate) remote_sync: RemoteSyncApplyReport,
    pub(crate) mcp_quality: McpQualityToolsReport,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SelfLearningRetrievalReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) selected_profile: String,
    pub(crate) ranking: AutoRankingTuneReport,
    pub(crate) usefulness: UsefulnessEngineReport,
    pub(crate) signals: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProjectRoleProfileReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) inferred_kind: String,
    pub(crate) profile_path: String,
    pub(crate) template: ProjectTemplateReport,
    pub(crate) reasons: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InboxAiReviewerReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) applied: bool,
    pub(crate) pending: usize,
    pub(crate) approve_ready: usize,
    pub(crate) merge_ready: usize,
    pub(crate) reject_ready: usize,
    pub(crate) keep_pending: usize,
    pub(crate) inbox: InboxV2Report,
    pub(crate) explanations: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV3Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) tabs: Vec<WebControlTab>,
    pub(crate) primary_actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlTab {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) score: f64,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncApplyReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) flow: RemoteSyncApplyFlowReport,
    pub(crate) sync_v2: RemoteSyncV2Report,
    pub(crate) sync_profile: SyncProfileReport,
    pub(crate) commands: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpQualityToolsReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) surface: McpToolSurfaceV2Report,
    pub(crate) quality_tools: Vec<String>,
    pub(crate) missing_quality_tools: Vec<String>,
    pub(crate) recommended_flow: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncControlReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) local_first: bool,
    pub(crate) target_ready: bool,
    pub(crate) remote_bundle: String,
    pub(crate) remote_bundle_exists: bool,
    pub(crate) remote_memory_count: Option<usize>,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) sync_profile: SyncProfileReport,
    pub(crate) remote_sync: RemoteSyncV2Report,
    pub(crate) dry_run_commands: Vec<String>,
    pub(crate) apply_commands: Vec<String>,
    pub(crate) rollback_hint: String,
    pub(crate) blockers: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV4Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) v3: WebControlCenterV3Report,
    pub(crate) remote_sync: RemoteSyncControlReport,
    pub(crate) mcp_discipline: McpDisciplineV2Report,
    pub(crate) feedback_loop: FeedbackLoopV2Report,
    pub(crate) upgrade_all: UpgradeAllProjectsV2Report,
    pub(crate) controls: Vec<WebControlAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WebControlAction {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) method: String,
    pub(crate) endpoint: String,
    pub(crate) cli: String,
    pub(crate) safe_auto: bool,
    pub(crate) requires_apply: bool,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct McpDisciplineV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) agent_enforce: AgentEnforceReport,
    pub(crate) mcp_quality: McpQualityToolsReport,
    pub(crate) budget: BudgetPlan,
    pub(crate) startup_flow: Vec<String>,
    pub(crate) after_task_flow: Vec<String>,
    pub(crate) missing_commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FeedbackLoopV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) qa: MemoryQaReport,
    pub(crate) usefulness: UsefulnessEngineReport,
    pub(crate) auto_feedback: AutoFeedbackV2Report,
    pub(crate) supersede: AutoSupersedeV2Report,
    pub(crate) diff_apply: MemoryDiffApplyReport,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) actions: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeAllProjectsV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) dry_run: bool,
    pub(crate) total_projects: usize,
    pub(crate) upgraded_projects: usize,
    pub(crate) ready_projects: usize,
    pub(crate) attention_projects: usize,
    pub(crate) base: UpgradeAllProjectsReport,
    pub(crate) project_summaries: Vec<UpgradeAllProjectSummary>,
    pub(crate) commands: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeAllProjectSummary {
    pub(crate) root: String,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) actions: usize,
    pub(crate) current_version: Option<String>,
    pub(crate) target_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct VdsSyncPackReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) pack_path: String,
    pub(crate) local_first: bool,
    pub(crate) control: RemoteSyncControlReport,
    pub(crate) dry_run_commands: Vec<String>,
    pub(crate) apply_commands: Vec<String>,
    pub(crate) verify_commands: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV5Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) v4: WebControlCenterV4Report,
    pub(crate) vds_sync: VdsSyncPackReport,
    pub(crate) quality_autopilot: QualityAutopilotV31Report,
    pub(crate) router: MemoryRouterV2Report,
    pub(crate) benchmark_profiles: BenchmarkProfilesReport,
    pub(crate) install_polish: InstallPolishReport,
    pub(crate) controls: Vec<WebControlAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct QualityAutopilotV31Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) applied: bool,
    pub(crate) loop_v2: FeedbackLoopV2Report,
    pub(crate) quality: QualityReport,
    pub(crate) cost_guard: CostGuardReport,
    pub(crate) health: MemoryHealthScoreReport,
    pub(crate) actions: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryRouterV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) include_siblings: bool,
    pub(crate) selected_authority: Option<String>,
    pub(crate) route_count: usize,
    pub(crate) sibling_hint_count: usize,
    pub(crate) routes: Vec<MemoryRouterV2Route>,
    pub(crate) base: MemoryRouterReport,
    pub(crate) guardrails: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryRouterV2Route {
    pub(crate) root: String,
    pub(crate) db: String,
    pub(crate) role: String,
    pub(crate) matches: usize,
    pub(crate) selected_for_write: bool,
    pub(crate) top_titles: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BenchmarkProfilesReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) selected_kind: String,
    pub(crate) since_days: i64,
    pub(crate) profile_path: String,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) role_profile: ProjectRoleProfileReport,
    pub(crate) commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallPolishReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) checks: Vec<InstallPolishCheck>,
    pub(crate) commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallPolishCheck {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryEffectivenessLabReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) score: f64,
    pub(crate) read_count: usize,
    pub(crate) influenced_reads: usize,
    pub(crate) confirmed_reads: usize,
    pub(crate) questioned_reads: usize,
    pub(crate) empty_reads: usize,
    pub(crate) semantic_result_rate: f64,
    pub(crate) unique_memory_ids: usize,
    pub(crate) trace: DecisionTraceReport,
    pub(crate) usage: UsageReport,
    pub(crate) roi: MemoryRoiReport,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutoContextBudgeterV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) task: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) policy_path: String,
    pub(crate) selected_profile: String,
    pub(crate) max_chars: usize,
    pub(crate) selected_commands: Vec<String>,
    pub(crate) avoided_commands: Vec<String>,
    pub(crate) governor: ContextGovernorReport,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryContractV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) written: bool,
    pub(crate) path: String,
    pub(crate) memory_id: Option<String>,
    pub(crate) max_chars: usize,
    pub(crate) freshness_score: f64,
    pub(crate) sections: Vec<MemoryContractV2Section>,
    pub(crate) base: MemoryContractReport,
    pub(crate) commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryContractV2Section {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CrossProjectLearningReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) applied: bool,
    pub(crate) policy_path: String,
    pub(crate) advisory_projects: usize,
    pub(crate) candidates: Vec<CrossProjectLearningCandidate>,
    pub(crate) router: MemoryRouterV2Report,
    pub(crate) guardrails: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CrossProjectLearningCandidate {
    pub(crate) project_root: String,
    pub(crate) matches: usize,
    pub(crate) titles: Vec<String>,
    pub(crate) use_as: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentTraceReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) traced_reads: usize,
    pub(crate) influenced_reads: usize,
    pub(crate) saved_writes: usize,
    pub(crate) timeline: Vec<AgentTraceItem>,
    pub(crate) trace: DecisionTraceReport,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentTraceItem {
    pub(crate) read_id: i64,
    pub(crate) command: String,
    pub(crate) query: String,
    pub(crate) influence: String,
    pub(crate) memory_count: usize,
    pub(crate) semantic_used: bool,
    pub(crate) feedback_positive: usize,
    pub(crate) feedback_negative: usize,
    pub(crate) feedback_missing: usize,
    pub(crate) explanation: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct VdsSyncHardeningReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) policy_path: String,
    pub(crate) pack: VdsSyncPackReport,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) remote_sync: RemoteSyncV2Report,
    pub(crate) apply_flow: RemoteSyncApplyFlowReport,
    pub(crate) checks: Vec<InstallPolishCheck>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallQualityReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) policy_path: String,
    pub(crate) install_polish: InstallPolishReport,
    pub(crate) doctor: ProjectDoctorReport,
    pub(crate) agent_enforce: AgentEnforceReport,
    pub(crate) checks: Vec<InstallPolishCheck>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV6Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) v5: WebControlCenterV5Report,
    pub(crate) effectiveness: MemoryEffectivenessLabReport,
    pub(crate) budgeter: AutoContextBudgeterV2Report,
    pub(crate) contract_v2: MemoryContractV2Report,
    pub(crate) cross_project: CrossProjectLearningReport,
    pub(crate) agent_trace: AgentTraceReport,
    pub(crate) vds_hardening: VdsSyncHardeningReport,
    pub(crate) install_quality: InstallQualityReport,
    pub(crate) controls: Vec<WebControlAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryAnswerReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) question: String,
    pub(crate) answer: String,
    pub(crate) actionable_answer: String,
    pub(crate) certainty: String,
    pub(crate) needs_verification: bool,
    pub(crate) semantic_used: bool,
    pub(crate) citation_count: usize,
    pub(crate) citations: Vec<MemoryAnswerCitation>,
    pub(crate) evidence_groups: Vec<MemoryAnswerGroup>,
    pub(crate) gaps: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryAnswerCitation {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) confidence: f64,
    pub(crate) status: String,
    pub(crate) why: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryAnswerGroup {
    pub(crate) name: String,
    pub(crate) count: usize,
    pub(crate) ids: Vec<String>,
    pub(crate) summary: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConnectCodexReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) policy_path: String,
    pub(crate) doctor: ProjectDoctorReport,
    pub(crate) agent_enforce: AgentEnforceReport,
    pub(crate) commands: Vec<String>,
    pub(crate) checks: Vec<InstallPolishCheck>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryTypeGuideReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) recommended_order: Vec<String>,
    pub(crate) types: Vec<MemoryTypeGuideItem>,
    pub(crate) filters: Vec<String>,
    pub(crate) guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryTypeGuideItem {
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) label: String,
    pub(crate) use_when: String,
    pub(crate) avoid_when: String,
    pub(crate) example: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryEvalStoryReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) write_baseline: bool,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) profiles: BenchmarkProfilesReport,
    pub(crate) harness: MemoryTestHarnessReport,
    pub(crate) effectiveness: MemoryEffectivenessLabReport,
    pub(crate) commands: Vec<String>,
    pub(crate) public_claims: Vec<String>,
    pub(crate) proof_points: Vec<MemoryEvalProofPoint>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryEvalProofPoint {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImportReviewReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) input: String,
    pub(crate) scope: String,
    pub(crate) applied: bool,
    pub(crate) candidate_count: usize,
    pub(crate) inserted: usize,
    pub(crate) duplicate_count: usize,
    pub(crate) candidates: Vec<ImportReviewCandidate>,
    pub(crate) quality_checks: Vec<InstallPolishCheck>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImportReviewCandidate {
    pub(crate) title: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) body: String,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
    pub(crate) duplicate: bool,
    pub(crate) fingerprint: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomousUsefulnessReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) usefulness: UsefulnessEngineReport,
    pub(crate) effectiveness: MemoryEffectivenessLabReport,
    pub(crate) quality: QualityReport,
    pub(crate) action_plan: Vec<AutonomousUsefulnessAction>,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomousUsefulnessAction {
    pub(crate) kind: String,
    pub(crate) target: String,
    pub(crate) reason: String,
    pub(crate) safe_auto: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct BenchmarkPolishReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) write_baseline: bool,
    pub(crate) eval_story: MemoryEvalStoryReport,
    pub(crate) benchmark: RecallBenchmarkSuiteReport,
    pub(crate) public_summary: Vec<String>,
    pub(crate) dashboard_stats: Vec<MemoryEvalProofPoint>,
    pub(crate) commands: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV7Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) v6: WebControlCenterV6Report,
    pub(crate) answer: MemoryAnswerReport,
    pub(crate) connect_codex: ConnectCodexReport,
    pub(crate) type_guide: MemoryTypeGuideReport,
    pub(crate) eval_story: MemoryEvalStoryReport,
    pub(crate) import_review: Option<ImportReviewReport>,
    pub(crate) controls: Vec<WebControlAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlCenterV8Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) v7: WebControlCenterV7Report,
    pub(crate) autonomous_usefulness: AutonomousUsefulnessReport,
    pub(crate) benchmark_polish: BenchmarkPolishReport,
    pub(crate) panels: Vec<WebControlPanel>,
    pub(crate) controls: Vec<WebControlAction>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WebControlPanel {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) headline: String,
    pub(crate) metrics: Vec<MemoryEvalProofPoint>,
    pub(crate) actions: Vec<String>,
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
    pub(crate) candidate_cards: Vec<MemoryDiffCandidate>,
    pub(crate) write_ready: Vec<MemoryDiffCandidate>,
    pub(crate) stale_memory_ids: Vec<String>,
    pub(crate) conflict_count: usize,
    pub(crate) actions: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemoryDiffCandidate {
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) confidence: f64,
    pub(crate) link: String,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WatchControlReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) root: String,
    pub(crate) applied: bool,
    pub(crate) installed: bool,
    pub(crate) running: bool,
    pub(crate) label: String,
    pub(crate) plist: String,
    pub(crate) status_file: String,
    pub(crate) install: AutonomousWatchInstallReport,
    pub(crate) actions: Vec<String>,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AutonomyControlCenterReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) since_days: i64,
    pub(crate) qa: MemoryQaReport,
    pub(crate) ranking: AutoRankingTuneReport,
    pub(crate) watch: WatchControlReport,
    pub(crate) diff_review: MemoryDiffReviewReport,
    pub(crate) remote_sync: RemoteSyncV2Report,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) local_first: bool,
    pub(crate) encrypted_bundle: bool,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) conflict_policy: String,
    pub(crate) commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpgradeAllProjectsReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) dry_run: bool,
    pub(crate) total_projects: usize,
    pub(crate) upgraded_projects: usize,
    pub(crate) projects: Vec<Value>,
    pub(crate) errors: Vec<String>,
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

pub(crate) fn print_context_governor(
    conn: &Connection,
    root: &Path,
    task: &str,
    target: Option<&str>,
    json_out: bool,
) -> Result<()> {
    let report = context_governor_report(conn, root, task, target)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Context Governor");
    println!(
        "budget: {} / {}",
        report.budget.profile, report.max_context_chars
    );
    for command in &report.commands {
        println!("command: {command}");
    }
    Ok(())
}

pub(crate) fn context_governor_report(
    conn: &Connection,
    root: &Path,
    task: &str,
    target: Option<&str>,
) -> Result<ContextGovernorReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let budget = budget_plan(conn, task, Some("project"))?;
    let mut selected_flow = vec!["brief".to_string()];
    let mut commands = vec![format!(
        "dukememory brief \"{}\" --budget-profile {}",
        shell_safe_inline(task),
        budget.profile
    )];
    let mut reasons = budget.reasons.clone();
    if let Some(target) = target {
        selected_flow.push("impact".to_string());
        commands.push(format!(
            "dukememory impact {} --budget-profile tiny",
            shell_safe_inline(target)
        ));
        reasons.push("target supplied; impact memory should be read before edits".to_string());
    }
    if budget.profile != "tiny" || task.to_lowercase().contains("architecture") {
        selected_flow.push("doctrine".to_string());
        commands.push("dukememory doctrine --json".to_string());
        reasons.push("broad task may depend on durable decisions".to_string());
    }
    if task.to_lowercase().contains("release") || task.to_lowercase().contains("schema") {
        selected_flow.push("drift".to_string());
        commands.push("dukememory drift --root . --json".to_string());
        reasons.push("release/schema task needs drift check before mutation".to_string());
    }
    if budget.semantic {
        selected_flow.push("recall-if-needed".to_string());
        commands.push(format!(
            "dukememory recall \"{}\" --max-chars {}",
            shell_safe_inline(task),
            budget.max_chars.min(1200)
        ));
    }
    selected_flow.sort();
    selected_flow.dedup();
    reasons.sort();
    reasons.dedup();
    Ok(ContextGovernorReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        task: task.to_string(),
        target: target.map(str::to_string),
        selected_flow,
        max_context_chars: budget.max_chars.min(1600),
        budget,
        commands,
        reasons,
    })
}

fn shell_safe_inline(value: &str) -> String {
    value.replace('"', "\\\"")
}

pub(crate) fn print_memory_router(
    default_db: &Path,
    root: &Path,
    query: &str,
    include_siblings: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_router_report(default_db, root, query, include_siblings)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Router");
    for route in &report.routes {
        println!(
            "{}: {} match(es), {}",
            route.mode, route.matches, route.root
        );
    }
    Ok(())
}

pub(crate) fn memory_router_report(
    default_db: &Path,
    root: &Path,
    query: &str,
    include_siblings: bool,
) -> Result<MemoryRouterReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let current_db = root.join(".agent/memory.db");
    let mut dbs = vec![if current_db.exists() {
        current_db
    } else {
        default_db.to_path_buf()
    }];
    if include_siblings {
        for db in discover_project_dbs(default_db)? {
            if !dbs.iter().any(|existing| paths_same(existing, &db)) {
                dbs.push(db);
            }
        }
    }
    let mut routes = Vec::new();
    let mut warnings = Vec::new();
    for db in dbs {
        let project_root = app_project_root_for_db(&db).unwrap_or_else(|| root.clone());
        let current = paths_same(&project_root, &root);
        let conn = match open_db(&db) {
            Ok(conn) => conn,
            Err(err) => {
                warnings.push(format!("{}: {err}", db.display()));
                continue;
            }
        };
        let matches = query_memories(
            &conn,
            Some(query),
            &[],
            &["active".to_string(), "uncertain".to_string()],
            Some("project"),
            5,
        )?;
        routes.push(MemoryRouteItem {
            root: project_root.display().to_string(),
            db: db.display().to_string(),
            current,
            mode: if current { "authoritative" } else { "advisory" }.to_string(),
            matches: matches.len(),
            top_titles: matches.into_iter().map(|memory| memory.title).collect(),
        });
    }
    routes.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| b.matches.cmp(&a.matches))
            .then_with(|| a.root.cmp(&b.root))
    });
    let selected_route = routes
        .iter()
        .find(|route| route.current)
        .or_else(|| routes.first())
        .map(|route| route.root.clone());
    let mut recommendations =
        vec!["treat non-current project routes as advisory until evidence is checked".to_string()];
    if !include_siblings {
        recommendations
            .push("pass --include-siblings to inspect nearby project memories".to_string());
    }
    Ok(MemoryRouterReport {
        version: 1,
        ok: warnings.is_empty(),
        root: root.display().to_string(),
        query: query.to_string(),
        include_siblings,
        routes,
        selected_route,
        warnings,
        recommendations,
    })
}

fn paths_same(left: &Path, right: &Path) -> bool {
    left.canonicalize().ok() == right.canonicalize().ok()
}

pub(crate) fn print_auto_ranking_tune(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = auto_ranking_tune_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Auto Ranking Tune");
    println!("selected_profile: {}", report.selected_profile);
    println!("applied: {}", report.applied);
    for reason in &report.reasons {
        println!("- {reason}");
    }
    Ok(())
}

pub(crate) fn auto_ranking_tune_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<AutoRankingTuneReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let quality = quality_report(conn, since_days, 30)?;
    let noisy_cards = quality
        .weakest
        .iter()
        .filter(|item| item.score < 55.0 && item.request_count > 0)
        .count();
    let (profile, mut reasons) =
        if qa.inferred_missing > 0 || qa.semantic_eligible_empty_read_count > 0 {
            (
                RankingProfileMode::RecallHeavy,
                vec!["missing or empty semantic reads need broader recall".to_string()],
            )
        } else if noisy_cards > 0 || qa.useful_rate < 0.85 || qa.quality_average < 65.0 {
            (
                RankingProfileMode::Strict,
                vec!["noisy/weak memory signals need stricter retrieval".to_string()],
            )
        } else if qa.useful_rate > 0.95 && qa.semantic_eligible_result_rate >= 0.95 {
            (
                RankingProfileMode::Balanced,
                vec!["memory reads are useful and semantic recall is healthy".to_string()],
            )
        } else {
            (
                RankingProfileMode::PrecisionHeavy,
                vec!["mixed quality signals favor precision over recall".to_string()],
            )
        };
    if apply {
        reasons.push("applied durable .agent/ranking-profile.json".to_string());
    }
    let ranking = ranking_profile_report(&root, profile, apply)?;
    Ok(AutoRankingTuneReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        since_days,
        selected_profile: profile.to_string(),
        applied: apply,
        qa_score: qa.score,
        useful_rate: qa.useful_rate,
        inferred_missing: qa.inferred_missing,
        semantic_empty: qa.semantic_eligible_empty_read_count,
        noisy_cards,
        reasons,
        ranking,
    })
}

pub(crate) fn print_memory_health_score(
    conn: &Connection,
    default_db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = memory_health_score_report(conn, default_db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Health Score: {:.1} {}", report.score, report.grade);
    println!("status: {}", report.status);
    for component in &report.components {
        println!(
            "- {} {:.1} ({:.0}%): {}",
            component.name,
            component.score,
            component.weight * 100.0,
            component.detail
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

pub(crate) fn memory_health_score_report(
    conn: &Connection,
    default_db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<MemoryHealthScoreReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let roi = roi_report(conn, since_days)?;
    let audit = agent_audit_report(conn, since_days)?;
    let cost = cost_guard_report(conn, since_days)?;
    let autonomy = autonomy_control_center_report(conn, default_db, &root, since_days).ok();
    let autonomy_ready = autonomy.as_ref().is_some_and(|item| item.ok);
    let semantic_ready = qa.embedding_missing == 0
        && qa.embedding_stale == 0
        && qa.semantic_eligible_result_rate >= 0.80;
    let cost_score = cost.score;
    let autonomy_score = if autonomy_ready { 100.0 } else { 65.0 };
    let components = vec![
        MemoryHealthComponent {
            name: "qa".to_string(),
            score: qa.score,
            weight: 0.30,
            status: score_status(qa.score),
            detail: format!(
                "quality {:.1}, useful {:.0}%, active {}",
                qa.quality_average,
                qa.useful_rate * 100.0,
                qa.active_memories
            ),
        },
        MemoryHealthComponent {
            name: "roi".to_string(),
            score: roi.score,
            weight: 0.20,
            status: score_status(roi.score),
            detail: format!(
                "reuse {:.0}%, token saving {}",
                roi.reused_card_rate * 100.0,
                roi.token_saving_estimate
            ),
        },
        MemoryHealthComponent {
            name: "agent".to_string(),
            score: audit.score,
            weight: 0.20,
            status: score_status(audit.score),
            detail: format!(
                "brief {} impact {} feedback {}",
                audit.brief_reads, audit.impact_reads, audit.feedback_events
            ),
        },
        MemoryHealthComponent {
            name: "cost".to_string(),
            score: cost_score,
            weight: 0.15,
            status: score_status(cost_score),
            detail: format!(
                "profile {}, max chars {}",
                cost.recommended_profile, cost.recommended_max_chars
            ),
        },
        MemoryHealthComponent {
            name: "autonomy".to_string(),
            score: autonomy_score,
            weight: 0.15,
            status: score_status(autonomy_score),
            detail: autonomy
                .as_ref()
                .map(|item| item.status.clone())
                .unwrap_or_else(|| "unavailable".to_string()),
        },
    ];
    let score = components
        .iter()
        .map(|component| component.score * component.weight)
        .sum::<f64>()
        .clamp(0.0, 100.0);
    let mut issues = Vec::new();
    issues.extend(qa.issues.clone());
    issues.extend(roi.issues.clone());
    issues.extend(audit.issues.clone());
    issues.extend(cost.issues.clone());
    if !autonomy_ready {
        issues.push("autonomy control center is not fully ready".to_string());
    }
    let mut recommendations = Vec::new();
    recommendations.extend(qa.recommendations.clone());
    recommendations.extend(roi.recommendations.clone());
    recommendations.extend(audit.recommendations.clone());
    recommendations.extend(cost.actions.clone());
    if !semantic_ready {
        recommendations.push("refresh embeddings and inspect semantic empty reads".to_string());
    }
    if !autonomy_ready {
        recommendations.push("run dukememory autonomous-loop --apply --json".to_string());
    }
    issues.sort();
    issues.dedup();
    recommendations.sort();
    recommendations.dedup();
    let status = if score >= 85.0 && issues.len() <= 2 {
        "ready"
    } else if score >= 70.0 {
        "attention"
    } else {
        "blocked"
    }
    .to_string();
    Ok(MemoryHealthScoreReport {
        version: 1,
        ok: status == "ready" || status == "attention",
        status,
        root: root.display().to_string(),
        since_days,
        score,
        grade: health_grade(score),
        qa_score: qa.score,
        roi_score: roi.score,
        agent_score: audit.score,
        cost_score,
        autonomy_ready,
        semantic_ready,
        write_pressure: qa.write_pressure,
        token_saving_estimate: qa.token_saving_estimate,
        components,
        issues,
        recommendations,
    })
}

pub(crate) fn print_explain_recall(
    conn: &Connection,
    root: &Path,
    query: &str,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = explain_recall_report(conn, root, query, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Explain Recall: {}", report.query);
    for hit in &report.hits {
        println!(
            "- {:.2} {} [{}] {}",
            hit.score, hit.id, hit.memory_type, hit.explanation
        );
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn explain_recall_report(
    conn: &Connection,
    root: &Path,
    query: &str,
    limit: usize,
) -> Result<ExplainRecallReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let limit = limit.clamp(1, 25);
    let memories = query_memories(
        conn,
        Some(query),
        &[],
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        limit,
    )?;
    let usage = usage_report(conn, 30, 50).ok();
    let usage_counts = usage
        .as_ref()
        .map(|usage| {
            usage
                .top_memories
                .iter()
                .map(|item| (item.id.clone(), item.request_count))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let terms = recall_terms(query);
    let mut hits = Vec::new();
    for memory in memories {
        let haystack = format!(
            "{} {} {}",
            memory.title.to_lowercase(),
            memory.body.to_lowercase(),
            memory.memory_type.to_lowercase()
        );
        let matched_terms = terms
            .iter()
            .filter(|term| haystack.contains(term.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let reuse = usage_counts.get(&memory.id).copied().unwrap_or(0);
        let type_weight = recall_type_weight(&memory.memory_type);
        let status_weight = if memory.status == "active" { 8.0 } else { 2.0 };
        let score = (matched_terms.len() as f64 * 12.0)
            + memory.confidence.clamp(0.0, 1.0) * 20.0
            + type_weight
            + status_weight
            + (reuse.min(5) as f64 * 4.0);
        let mut reasons = Vec::new();
        if !matched_terms.is_empty() {
            reasons.push(format!("matched terms: {}", matched_terms.join(", ")));
        }
        if reuse > 0 {
            reasons.push(format!("reused by recent agents {reuse} time(s)"));
        }
        if memory.confidence >= 0.9 {
            reasons.push("high-confidence durable memory".to_string());
        }
        reasons.push(format!(
            "type {} is useful for task context",
            memory.memory_type
        ));
        let explanation = if matched_terms.is_empty() {
            format!(
                "selected by FTS fallback and durable {} context",
                memory.memory_type
            )
        } else {
            format!(
                "selected because {} matched the query and {} has confidence {:.2}",
                matched_terms.join(", "),
                memory.memory_type,
                memory.confidence
            )
        };
        hits.push(ExplainRecallHit {
            id: memory.id,
            memory_type: memory.memory_type,
            title: memory.title,
            status: memory.status,
            confidence: memory.confidence,
            score,
            matched_terms,
            reasons,
            explanation,
            preview: truncate_chars(&memory.body, 220),
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.title.cmp(&b.title))
    });
    let mut missing_signals = Vec::new();
    if hits.is_empty() {
        missing_signals.push("no active project memory matched this query".to_string());
    }
    if !terms.is_empty() && hits.iter().all(|hit| hit.matched_terms.is_empty()) {
        missing_signals.push("no recalled card shares explicit query terms".to_string());
    }
    let mut recommendations = Vec::new();
    if hits.is_empty() {
        recommendations.push("add a durable card if this is a repeated project fact".to_string());
    } else {
        recommendations
            .push("use top hits as bounded context; do not expand to full memory dump".to_string());
    }
    Ok(ExplainRecallReport {
        version: 1,
        ok: missing_signals.is_empty(),
        root: root.display().to_string(),
        query: query.to_string(),
        limit,
        hits,
        missing_signals,
        recommendations,
    })
}

pub(crate) fn print_project_intent_map(
    conn: &Connection,
    root: &Path,
    json_out: bool,
) -> Result<()> {
    let report = project_intent_map_report(conn, root)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Intent Map");
    for goal in &report.goals {
        println!("goal: {}", goal.title);
    }
    for decision in &report.decisions {
        println!("decision: {}", decision.title);
    }
    for gap in &report.gaps {
        println!("gap: {gap}");
    }
    Ok(())
}

pub(crate) fn project_intent_map_report(
    conn: &Connection,
    root: &Path,
) -> Result<ProjectIntentMapReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let goals = intent_items(conn, &["product_goal"], 5)?;
    let decisions = intent_items(conn, &["decision", "design_note"], 8)?;
    let constraints = intent_items(conn, &["constraint", "user_preference"], 6)?;
    let commands = intent_items(conn, &["command"], 6)?;
    let risks = intent_items(conn, &["known_issue"], 6)?;
    let active_tasks = intent_items(conn, &["task_state"], 8)?;
    let contract = memory_contract_report(conn, &root, false)?;
    let mut gaps = Vec::new();
    if goals.is_empty() {
        gaps.push("no product_goal cards define project intent".to_string());
    }
    if decisions.is_empty() {
        gaps.push("no active decisions/design notes were found".to_string());
    }
    if commands.is_empty() {
        gaps.push("no command cards document validation workflows".to_string());
    }
    Ok(ProjectIntentMapReport {
        version: 1,
        ok: gaps.len() <= 1,
        root: root.display().to_string(),
        goals,
        decisions,
        constraints,
        commands,
        risks,
        active_tasks,
        contract_path: contract.path,
        contract_preview: truncate_chars(&contract.content, 1400),
        recommended_start_flow: vec![
            "context-governor".to_string(),
            "brief".to_string(),
            "impact when target is known".to_string(),
            "explain-recall for surprising context".to_string(),
        ],
        gaps,
    })
}

pub(crate) fn print_memory_test_harness(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = memory_test_harness_report(conn, root, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Test Harness: {:.1}", report.score);
    for probe in &report.probes {
        println!(
            "- {}: {} ({})",
            if probe.found { "ok" } else { "miss" },
            probe.query,
            probe.explanation
        );
    }
    Ok(())
}

pub(crate) fn memory_test_harness_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
) -> Result<MemoryTestHarnessReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let limit = limit.clamp(3, 20);
    let usage = usage_report(conn, since_days, 10)?;
    let mut seed_queries = Vec::new();
    for item in usage.top_memories.iter().take(4) {
        seed_queries.push((
            item.title.clone(),
            Some(item.memory_type.clone()),
            Some(item.id.clone()),
        ));
    }
    if seed_queries.len() < limit {
        for item in intent_items(
            conn,
            &["decision", "constraint", "command", "task_state"],
            limit,
        )? {
            seed_queries.push((item.title, Some(item.memory_type), Some(item.id)));
            if seed_queries.len() >= limit {
                break;
            }
        }
    }
    seed_queries.sort_by(|a, b| a.0.cmp(&b.0));
    seed_queries.dedup_by(|a, b| a.0 == b.0);
    let mut probes = Vec::new();
    let mut failures = Vec::new();
    for (query, expected_type, expected_id) in seed_queries.into_iter().take(limit) {
        let hits = query_memories(
            conn,
            Some(&query),
            &[],
            &["active".to_string(), "uncertain".to_string()],
            Some("project"),
            5,
        )?;
        let matched = hits.iter().find(|memory| {
            expected_id.as_ref().is_some_and(|id| memory.id == *id)
                || expected_type
                    .as_ref()
                    .is_some_and(|kind| memory.memory_type == *kind)
        });
        let found = matched.is_some();
        if !found {
            failures.push(format!("query did not recover expected memory: {query}"));
        }
        probes.push(MemoryTestProbe {
            query: query.clone(),
            expected_type,
            expected_id,
            found,
            matched_id: matched.map(|memory| memory.id.clone()),
            matched_title: matched.map(|memory| memory.title.clone()),
            result_count: hits.len(),
            explanation: if found {
                "retrieval recovered the expected card or type".to_string()
            } else if hits.is_empty() {
                "retrieval returned no active project cards".to_string()
            } else {
                "retrieval returned cards, but not the expected card/type".to_string()
            },
        });
    }
    if probes.is_empty() {
        failures.push("no suitable memory cards were available for probes".to_string());
    }
    let score = if probes.is_empty() {
        0.0
    } else {
        probes.iter().filter(|probe| probe.found).count() as f64 / probes.len() as f64 * 100.0
    };
    let mut recommendations = Vec::new();
    if score < 80.0 {
        recommendations
            .push("review missing probes and add links or compact clearer card titles".to_string());
    }
    if probes.len() < 3 {
        recommendations.push(
            "add durable decision/constraint/command cards before relying on memory tests"
                .to_string(),
        );
    }
    Ok(MemoryTestHarnessReport {
        version: 1,
        ok: score >= 75.0 && !probes.is_empty(),
        root: root.display().to_string(),
        since_days,
        score,
        probes,
        failures,
        recommendations,
    })
}

pub(crate) fn print_agent_audit_v2(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = agent_audit_v2_report(conn, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Agent Audit v2: {:.1} {}", report.score, report.status);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn agent_audit_v2_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
) -> Result<AgentAuditV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let audit = agent_audit_report(conn, since_days)?;
    let trace = decision_trace_report(conn, since_days, 20)?;
    let cost_guard = cost_guard_report(conn, since_days)?;
    let disciplined_reads = audit.read_count == 0
        || audit.brief_reads > 0
        || audit.impact_reads > 0
        || audit.commands.contains_key("recall");
    let semantic_effective = audit.semantic_eligible_result_rate >= 0.75 || audit.read_count < 3;
    let writes_controlled = cost_guard.write_pressure <= 1.0 || audit.read_count < 5;
    let feedback_loop_active = audit.feedback_events > 0 || trace.positive_feedback > 0;
    let trace_explainable = trace.traced_reads == 0
        || trace.influenced_reads > 0
        || trace
            .items
            .iter()
            .any(|item| !item.memory_titles.is_empty());
    let mut score = audit.score;
    if !disciplined_reads {
        score -= 12.0;
    }
    if !semantic_effective {
        score -= 12.0;
    }
    if !writes_controlled {
        score -= 10.0;
    }
    if !feedback_loop_active {
        score -= 6.0;
    }
    if !trace_explainable {
        score -= 8.0;
    }
    score = score.clamp(0.0, 100.0);
    let mut issues = audit.issues.clone();
    if !disciplined_reads {
        issues.push("recent agents did not start with brief/impact/recall".to_string());
    }
    if !semantic_effective {
        issues.push("semantic eligible reads are not returning enough results".to_string());
    }
    if !writes_controlled {
        issues.push("memory write pressure is too high for lightweight use".to_string());
    }
    if !trace_explainable {
        issues.push("recent memory influence is not explainable enough".to_string());
    }
    let mut recommendations = audit.recommendations.clone();
    recommendations.extend(trace.recommendations.clone());
    recommendations.extend(cost_guard.actions.clone());
    if !feedback_loop_active {
        recommendations
            .push("record lightweight feedback for useful/useless/missing cards".to_string());
    }
    issues.sort();
    issues.dedup();
    recommendations.sort();
    recommendations.dedup();
    let status = if score >= 85.0 {
        "ready"
    } else if score >= 70.0 {
        "attention"
    } else {
        "blocked"
    }
    .to_string();
    Ok(AgentAuditV2Report {
        version: 1,
        ok: status != "blocked",
        status,
        root: root.display().to_string(),
        since_days,
        score,
        base_score: audit.score,
        disciplined_reads,
        semantic_effective,
        writes_controlled,
        feedback_loop_active,
        trace_explainable,
        issues,
        recommendations,
        audit,
        trace,
        cost_guard,
    })
}

pub(crate) fn print_memory_control_center_v2(
    conn: &Connection,
    default_db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = memory_control_center_v2_report(conn, default_db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Control Center v2: {}", report.status);
    println!("health: {:.1}", report.health.score);
    println!("test harness: {:.1}", report.test_harness.score);
    for action in &report.next_actions {
        println!("next: {action}");
    }
    Ok(())
}

pub(crate) fn memory_control_center_v2_report(
    conn: &Connection,
    default_db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<MemoryControlCenterV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let health = memory_health_score_report(conn, default_db, &root, since_days)?;
    let intent_map = project_intent_map_report(conn, &root)?;
    let test_harness = memory_test_harness_report(conn, &root, since_days, 8)?;
    let agent_audit_v2 = agent_audit_v2_report(conn, &root, since_days)?;
    let explain_query = intent_map
        .active_tasks
        .first()
        .or_else(|| intent_map.decisions.first())
        .map(|item| item.title.as_str())
        .unwrap_or("project memory");
    let explain_recall = explain_recall_report(conn, &root, explain_query, 6)?;
    let autonomy = autonomy_control_center_report(conn, default_db, &root, since_days)?;
    let mut next_actions = Vec::new();
    if health.score < 85.0 {
        next_actions.push("inspect memory-health-score recommendations".to_string());
    }
    if !test_harness.ok {
        next_actions.push("fix retrieval probes before expanding memory usage".to_string());
    }
    if !agent_audit_v2.ok {
        next_actions.push("run agent-audit-v2 and repair read/write discipline".to_string());
    }
    if !intent_map.gaps.is_empty() {
        next_actions.push("fill project-intent-map gaps with durable cards".to_string());
    }
    if !autonomy.ok {
        next_actions.push("run autonomous-loop --apply, then project-watch --fix".to_string());
    }
    if next_actions.is_empty() {
        next_actions
            .push("keep using context-governor and explain-recall for new tasks".to_string());
    }
    let ok = health.ok && test_harness.ok && agent_audit_v2.ok && autonomy.ok;
    let status = if ok {
        "ready"
    } else if health.score >= 70.0 {
        "attention"
    } else {
        "blocked"
    }
    .to_string();
    Ok(MemoryControlCenterV2Report {
        version: 1,
        ok,
        status,
        root: root.display().to_string(),
        since_days,
        health,
        intent_map,
        test_harness,
        agent_audit_v2,
        explain_recall,
        autonomy,
        next_actions,
    })
}

pub(crate) fn print_auto_supersede_v2(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = auto_supersede_v2_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Auto Supersede v2");
    println!("applied: {}", report.applied);
    println!("candidates: {}", report.candidates.len());
    println!("superseded: {}", report.superseded.len());
    for item in &report.candidates {
        println!(
            "- {} <- {} {:.2} {}",
            item.primary_id, item.duplicate_id, item.confidence, item.reason
        );
    }
    Ok(())
}

pub(crate) fn auto_supersede_v2_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<AutoSupersedeV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let usefulness = usefulness_report(conn, since_days, 30, 3)?;
    let candidates = usefulness
        .duplicate_candidates
        .iter()
        .take(20)
        .map(|candidate| {
            let confidence = auto_supersede_confidence(candidate);
            AutoSupersedeCandidate {
                primary_id: candidate.primary_id.clone(),
                duplicate_id: candidate.duplicate_id.clone(),
                title: candidate.title.clone(),
                reason: candidate.reason.clone(),
                confidence,
                safe_to_apply: confidence >= 0.90,
            }
        })
        .collect::<Vec<_>>();
    let mut superseded = Vec::new();
    let mut skipped = Vec::new();
    if apply {
        for candidate in &candidates {
            if !candidate.safe_to_apply {
                skipped.push(format!(
                    "{}: confidence {:.2} below 0.90",
                    candidate.duplicate_id, candidate.confidence
                ));
                continue;
            }
            let changed = conn.execute(
                "UPDATE memories SET status = 'superseded', superseded_by = ?1, updated_at = ?2 WHERE id = ?3 AND status IN ('active', 'uncertain')",
                params![candidate.primary_id, now_ms(), candidate.duplicate_id],
            )?;
            if changed > 0 {
                superseded.push(candidate.duplicate_id.clone());
                log_event(
                    conn,
                    "auto_supersede_v2",
                    Some(&candidate.duplicate_id),
                    &serde_json::to_string(&json!({
                        "primary_id": candidate.primary_id,
                        "confidence": candidate.confidence,
                        "reason": candidate.reason,
                    }))?,
                )?;
            } else {
                skipped.push(format!("{}: not active/uncertain", candidate.duplicate_id));
            }
        }
    } else {
        skipped.push("dry_run: no memory status changed".to_string());
    }
    let mut recommendations = Vec::new();
    if candidates.is_empty() {
        recommendations.push("no duplicate candidates currently need supersede review".to_string());
    }
    if !apply && candidates.iter().any(|candidate| candidate.safe_to_apply) {
        recommendations
            .push("rerun with --apply to supersede safe duplicate candidates".to_string());
    }
    Ok(AutoSupersedeV2Report {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        since_days,
        applied: apply,
        candidates,
        superseded,
        skipped,
        rollback_hint: "Use audit/action journal plus memory update to reactivate a superseded card; autonomous rollback remains available for autonomous cycles.".to_string(),
        recommendations,
    })
}

pub(crate) fn print_memory_diff_apply(
    conn: &Connection,
    root: &Path,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_diff_apply_report(conn, root, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Diff Apply");
    println!("applied: {}", report.applied);
    println!("written: {}", report.written_ids.len());
    for id in &report.written_ids {
        println!("written: {id}");
    }
    for skipped in &report.skipped {
        println!("skipped: {skipped}");
    }
    Ok(())
}

pub(crate) fn memory_diff_apply_report(
    conn: &Connection,
    root: &Path,
    apply: bool,
) -> Result<MemoryDiffApplyReport> {
    let reviewed = memory_diff_review_report(conn, root, false)?;
    let mut written_ids = Vec::new();
    let mut skipped = Vec::new();
    let mut actions = Vec::new();
    for candidate in &reviewed.write_ready {
        if candidate.confidence < 0.85 {
            skipped.push(format!("{}: confidence below 0.85", candidate.title));
            continue;
        }
        if memory_title_exists(conn, &candidate.memory_type, &candidate.title)? {
            skipped.push(format!(
                "{}: matching active card already exists",
                candidate.title
            ));
            continue;
        }
        if apply {
            let id = add_memory(
                conn,
                AddMemory {
                    id: None,
                    memory_type: candidate.memory_type.clone(),
                    title: candidate.title.clone(),
                    body: candidate.body.clone(),
                    scope: "project".to_string(),
                    status: "active".to_string(),
                    source: Some("memory_diff_apply".to_string()),
                    supersedes: None,
                    confidence: candidate.confidence,
                    links: vec![candidate.link.clone()],
                },
            )?;
            written_ids.push(id.clone());
            actions.push(format!("memory_written:{id}"));
        } else {
            actions.push(format!("dry_run: would write {}", candidate.title));
        }
    }
    if reviewed.write_ready.is_empty() {
        skipped.push("no high-confidence diff candidates".to_string());
    }
    let mut recommendations = reviewed.recommendations.clone();
    if !apply && !reviewed.write_ready.is_empty() {
        recommendations
            .push("rerun memory-diff-apply --apply to write high-confidence cards".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    Ok(MemoryDiffApplyReport {
        version: 1,
        ok: reviewed.ok,
        root: reviewed.root.clone(),
        applied: apply,
        reviewed,
        written_ids,
        skipped,
        actions,
        recommendations,
    })
}

pub(crate) fn print_recall_benchmark_suite(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
    write_baseline: bool,
    json_out: bool,
) -> Result<()> {
    let report = recall_benchmark_suite_report(conn, root, since_days, limit, write_baseline)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Recall Benchmark Suite: {:.1}", report.score);
    println!("baseline: {:?}", report.baseline_score);
    println!("regression: {}", report.regression);
    for recommendation in &report.recommendations {
        println!("recommendation: {recommendation}");
    }
    Ok(())
}

pub(crate) fn recall_benchmark_suite_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
    write_baseline: bool,
) -> Result<RecallBenchmarkSuiteReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let harness = memory_test_harness_report(conn, &root, since_days, limit)?;
    let baseline_path = root.join(".agent/recall-benchmark.json");
    let baseline = fs::read_to_string(&baseline_path)
        .ok()
        .and_then(|content| serde_json::from_str::<RecallBenchmarkBaseline>(&content).ok());
    let baseline_score = baseline.as_ref().map(|item| item.score);
    let regression = baseline_score.is_some_and(|score| harness.score + 5.0 < score);
    let mut baseline_written = false;
    if write_baseline {
        if let Some(parent) = baseline_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let value = RecallBenchmarkBaseline {
            version: 1,
            score: harness.score,
            probe_count: harness.probes.len(),
            written_at: now_ms(),
        };
        write_file(
            &baseline_path,
            serde_json::to_string_pretty(&value)?.as_bytes(),
        )?;
        baseline_written = true;
    }
    let mut recommendations = harness.recommendations.clone();
    if baseline_score.is_none() && !write_baseline {
        recommendations
            .push("write a baseline with --write-baseline after reviewing probes".to_string());
    }
    if regression {
        recommendations.push("recall benchmark regressed by more than 5 points; inspect failed probes before release".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    Ok(RecallBenchmarkSuiteReport {
        version: 1,
        ok: harness.ok && !regression,
        root: root.display().to_string(),
        since_days,
        score: harness.score,
        baseline_score,
        regression,
        baseline_written,
        baseline_path: baseline_path.display().to_string(),
        harness,
        recommendations,
    })
}

pub(crate) fn print_release_gate_v2(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
    json_out: bool,
) -> Result<()> {
    let report = release_gate_v2_report(conn, db, root, since_days, strict, run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Release Gate v2");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn release_gate_v2_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    strict: bool,
    run: bool,
) -> Result<ReleaseGateV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let base_gate = release_gate_report(conn, db, &root, since_days, strict, run)?;
    let health = memory_health_score_report(conn, db, &root, since_days)?;
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, false)?;
    let audit_v2 = agent_audit_v2_report(conn, &root, since_days)?;
    let control_center = memory_control_center_v2_report(conn, db, &root, since_days)?;
    let mut checks = base_gate.checks.clone();
    checks.push(ReleaseGateCheck {
        name: "memory_health_score".to_string(),
        ok: health.score >= 85.0 && health.ok,
        required: true,
        detail: format!("score={:.1} grade={}", health.score, health.grade),
    });
    checks.push(ReleaseGateCheck {
        name: "recall_benchmark".to_string(),
        ok: benchmark.ok && benchmark.score >= 80.0,
        required: true,
        detail: format!(
            "score={:.1} baseline={}",
            benchmark.score,
            benchmark
                .baseline_score
                .map(|score| format!("{score:.1}"))
                .unwrap_or_else(|| "-".to_string())
        ),
    });
    checks.push(ReleaseGateCheck {
        name: "agent_audit_v2".to_string(),
        ok: audit_v2.ok && audit_v2.score >= 80.0,
        required: true,
        detail: format!("score={:.1} status={}", audit_v2.score, audit_v2.status),
    });
    checks.push(ReleaseGateCheck {
        name: "control_center_v2".to_string(),
        ok: control_center.ok,
        required: true,
        detail: control_center.status.clone(),
    });
    checks.push(ReleaseGateCheck {
        name: "intent_map".to_string(),
        ok: control_center.intent_map.gaps.is_empty(),
        required: true,
        detail: format!("gaps={}", control_center.intent_map.gaps.len()),
    });
    let mut issues = base_gate.issues.clone();
    for check in &checks {
        if check.required && !check.ok {
            issues.push(format!("release gate v2 failed: {}", check.name));
        }
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = base_gate.recommendations.clone();
    recommendations.extend(health.recommendations.clone());
    recommendations.extend(benchmark.recommendations.clone());
    recommendations.extend(audit_v2.recommendations.clone());
    recommendations.extend(control_center.next_actions.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(ReleaseGateV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        strict,
        run,
        base_gate,
        health,
        benchmark,
        audit_v2,
        control_center,
        checks,
        issues,
        recommendations,
    })
}

pub(crate) fn print_remote_sync_wizard(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_wizard_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync Wizard");
    println!("status: {}", report.status);
    for step in &report.steps {
        println!("{} {}", if step.ok { "ok" } else { "warn" }, step.name);
    }
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

pub(crate) fn remote_sync_wizard_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<RemoteSyncWizardReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let sync = remote_sync_v2_report(conn, db, &root, target, since_days, apply)?;
    let passphrase_ready = std::env::var("DUKEMEMORY_SYNC_PASSPHRASE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let mut steps = vec![
        RemoteSyncWizardStep {
            name: "target".to_string(),
            ok: target.is_some(),
            detail: target
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "set --target PATH before apply".to_string()),
        },
        RemoteSyncWizardStep {
            name: "encryption".to_string(),
            ok: passphrase_ready || !apply,
            detail: if passphrase_ready {
                "DUKEMEMORY_SYNC_PASSPHRASE is set".to_string()
            } else {
                "set DUKEMEMORY_SYNC_PASSPHRASE before --apply".to_string()
            },
        },
        RemoteSyncWizardStep {
            name: "latency".to_string(),
            ok: sync.latency.ok,
            detail: format!("roundtrip={}ms", sync.latency.estimated_roundtrip_ms),
        },
        RemoteSyncWizardStep {
            name: "conflict_policy".to_string(),
            ok: sync.conflict_policy == "manual",
            detail: sync.conflict_policy.clone(),
        },
        RemoteSyncWizardStep {
            name: "local_first".to_string(),
            ok: sync.local_first,
            detail: "reads stay local; remote is backup/sync target".to_string(),
        },
    ];
    let mut blockers = sync.blockers.clone();
    if target.is_none() {
        blockers.push("target is required".to_string());
    }
    if apply && !passphrase_ready {
        blockers.push("DUKEMEMORY_SYNC_PASSPHRASE is required for encrypted apply".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let mut recommendations = sync.recommendations.clone();
    recommendations
        .push("keep reads local-first unless measured latency remains acceptable".to_string());
    recommendations.push("review conflicts manually before import/apply".to_string());
    if !apply {
        recommendations
            .push("rerun with --apply only after target and passphrase are ready".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty()
        && steps
            .iter()
            .all(|step| step.ok || (!apply && step.name == "encryption"));
    let status = if ok { "ready" } else { "blocked" }.to_string();
    Ok(RemoteSyncWizardReport {
        version: 1,
        ok,
        status,
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        applied: apply && sync.applied && blockers.is_empty(),
        steps: {
            steps.sort_by(|a, b| a.name.cmp(&b.name));
            steps
        },
        sync,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_memory_governance_policy(
    root: &Path,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_governance_policy_report(root, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Governance Policy");
    println!("applied: {}", report.applied);
    println!("path: {}", report.path);
    for action in &report.actions {
        println!("action: {action}");
    }
    Ok(())
}

pub(crate) fn memory_governance_policy_report(
    root: &Path,
    apply: bool,
) -> Result<MemoryGovernancePolicyReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = root.join(".agent/memory-governance.json");
    let report = MemoryGovernancePolicyReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        applied: apply,
        path: path.display().to_string(),
        auto_write_types: vec![
            "task_state".to_string(),
            "design_note".to_string(),
            "command".to_string(),
            "known_issue".to_string(),
        ],
        auto_supersede_min_confidence: 0.90,
        diff_apply_min_confidence: 0.85,
        max_write_pressure: 1.0,
        require_embeddings_for_semantic: true,
        remote_sync_mode: "local-first-backup".to_string(),
        actions: if apply {
            vec![format!("policy_written:{}", path.display())]
        } else {
            vec!["dry_run: policy not written".to_string()]
        },
        recommendations: vec![
            "allow autonomous writes only for durable high-confidence outcomes".to_string(),
            "keep remote sync local-first with manual conflict policy".to_string(),
            "use release-gate-v2 before publishing memory changes".to_string(),
        ],
    };
    if apply {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_file(&path, serde_json::to_string_pretty(&report)?.as_bytes())?;
    }
    Ok(report)
}

pub(crate) fn print_autonomous_loop_v2(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = autonomous_loop_v2_report(conn, db, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Autonomous Loop v2");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for action in &report.actions {
        println!("action: {action}");
    }
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

pub(crate) fn autonomous_loop_v2_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<AutonomousLoopV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let governance = memory_governance_policy_report(&root, apply)?;
    let enforcement = governance_enforce_report(conn, &root, since_days, apply)?;
    let loop_v1 = autonomous_loop_report(
        conn,
        db,
        &root,
        since_days,
        AutonomousLevel::Normal,
        apply && enforcement.ok,
    )?;
    let supersede = auto_supersede_v2_report(conn, &root, since_days, apply && enforcement.ok)?;
    let diff_apply = memory_diff_apply_report(conn, &root, apply && enforcement.ok)?;
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, apply)?;
    let release_gate_v2 = release_gate_v2_report(conn, db, &root, since_days, false, false)?;
    let mut actions = Vec::new();
    actions.extend(governance.actions.clone());
    actions.extend(enforcement.actions.clone());
    actions.extend(loop_v1.actions.clone());
    actions.extend(
        supersede
            .superseded
            .iter()
            .map(|id| format!("superseded:{id}")),
    );
    actions.extend(diff_apply.actions.clone());
    if benchmark.baseline_written {
        actions.push(format!(
            "benchmark_baseline_written:{}",
            benchmark.baseline_path
        ));
    }
    let mut blockers = enforcement.violations.clone();
    if !loop_v1.ok {
        blockers.push("autonomous-loop v1 is not ready".to_string());
    }
    if !benchmark.ok {
        blockers.push("recall benchmark is not ready".to_string());
    }
    if !release_gate_v2.ok {
        blockers.push("release-gate-v2 is not ready".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let mut recommendations = Vec::new();
    recommendations.extend(enforcement.recommendations.clone());
    recommendations.extend(loop_v1.recommendations.clone());
    recommendations.extend(supersede.recommendations.clone());
    recommendations.extend(diff_apply.recommendations.clone());
    recommendations.extend(benchmark.recommendations.clone());
    recommendations.extend(release_gate_v2.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    Ok(AutonomousLoopV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply && ok,
        governance,
        enforcement,
        loop_v1,
        supersede,
        diff_apply,
        benchmark,
        release_gate_v2,
        actions,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_governance_enforce(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = governance_enforce_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Governance Enforce");
    println!("status: {}", report.status);
    for violation in &report.violations {
        println!("violation: {violation}");
    }
    Ok(())
}

pub(crate) fn governance_enforce_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<GovernanceEnforceReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let policy = memory_governance_policy_report(&root, apply)?;
    let qa = memory_qa_report(conn, &root, since_days)?;
    let remote_mode_ok = policy.remote_sync_mode == "local-first-backup";
    let embedding_required_ok = !policy.require_embeddings_for_semantic
        || (qa.embedding_missing == 0 && qa.embedding_stale == 0);
    let mut violations = Vec::new();
    if qa.write_pressure > policy.max_write_pressure {
        violations.push(format!(
            "write pressure {:.2} exceeds policy {:.2}",
            qa.write_pressure, policy.max_write_pressure
        ));
    }
    if !embedding_required_ok {
        violations.push("semantic embeddings are required but missing/stale".to_string());
    }
    if !remote_mode_ok {
        violations.push(format!(
            "remote sync mode {} is not local-first-backup",
            policy.remote_sync_mode
        ));
    }
    let mut actions = policy.actions.clone();
    if apply && violations.is_empty() {
        actions.push("governance_enforced:ok".to_string());
        let _ = log_event(
            conn,
            "governance_enforce",
            None,
            &serde_json::to_string(&json!({
                "status": "ok",
                "root": root.display().to_string(),
                "write_pressure": qa.write_pressure,
            }))?,
        );
    } else if apply {
        actions.push("governance_enforced:blocked".to_string());
    } else {
        actions.push("dry_run: governance not enforced".to_string());
    }
    let mut recommendations = policy.recommendations.clone();
    if !violations.is_empty() {
        recommendations.push("resolve governance violations before autonomous apply".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = violations.is_empty();
    Ok(GovernanceEnforceReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        applied: apply && ok,
        policy,
        write_pressure: qa.write_pressure,
        embedding_required_ok,
        remote_mode_ok,
        allowed_auto_write_types: vec![
            "task_state".to_string(),
            "design_note".to_string(),
            "command".to_string(),
            "known_issue".to_string(),
        ],
        violations,
        actions,
        recommendations,
    })
}

pub(crate) fn print_memory_quality_ci(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    minimal: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_quality_ci_report(conn, db, root, since_days, minimal)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Quality CI: {}", report.status);
    println!("health: {:.1}", report.health_score);
    println!("benchmark: {:.1}", report.benchmark_score);
    println!("audit: {:.1}", report.audit_score);
    for failed in &report.failed_checks {
        println!("failed: {failed}");
    }
    Ok(())
}

pub(crate) fn memory_quality_ci_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    minimal: bool,
) -> Result<MemoryQualityCiReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let gate = release_gate_v2_report(conn, db, &root, since_days, true, false)?;
    let mut failed_checks = gate
        .checks
        .iter()
        .filter(|check| {
            check.required
                && !check.ok
                && !matches!(check.name.as_str(), "cargo_version" | "git_clean")
        })
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();
    failed_checks.sort();
    failed_checks.dedup();
    let ok = failed_checks.is_empty()
        && gate.health.score >= 85.0
        && gate.benchmark.score >= 80.0
        && gate.audit_v2.score >= 80.0;
    Ok(MemoryQualityCiReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        since_days,
        minimal,
        health_score: gate.health.score,
        benchmark_score: gate.benchmark.score,
        audit_score: gate.audit_v2.score,
        failed_checks,
        release_gate_v2: if minimal { None } else { Some(gate) },
        recommendations: if ok {
            vec!["memory quality CI passed".to_string()]
        } else {
            vec!["inspect release-gate-v2 failed checks before publishing".to_string()]
        },
    })
}

pub(crate) fn print_fleet_dashboard_v2(
    default_db: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = fleet_dashboard_v2_report(default_db, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Fleet Dashboard v2");
    println!("projects: {}", report.total_projects);
    println!("ready: {}", report.ready_projects);
    for project in &report.projects {
        println!(
            "{} {:.1} {}",
            project.status, project.health_score, project.root
        );
    }
    Ok(())
}

pub(crate) fn fleet_dashboard_v2_report(
    default_db: &Path,
    since_days: i64,
) -> Result<FleetDashboardV2Report> {
    let mut projects = Vec::new();
    for db in discover_project_dbs(default_db)? {
        let root = app_project_root_for_db(&db).unwrap_or_else(|| {
            db.parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        });
        match open_db(&db) {
            Ok(conn) => {
                let ci = memory_quality_ci_report(&conn, &db, &root, since_days, true)?;
                projects.push(FleetDashboardV2Project {
                    root: root.display().to_string(),
                    db: db.display().to_string(),
                    status: ci.status,
                    health_score: ci.health_score,
                    benchmark_score: ci.benchmark_score,
                    audit_score: ci.audit_score,
                    issues: ci.failed_checks,
                });
            }
            Err(err) => projects.push(FleetDashboardV2Project {
                root: root.display().to_string(),
                db: db.display().to_string(),
                status: "blocked".to_string(),
                health_score: 0.0,
                benchmark_score: 0.0,
                audit_score: 0.0,
                issues: vec![err.to_string()],
            }),
        }
    }
    let ready_projects = projects
        .iter()
        .filter(|project| project.status == "ready")
        .count();
    let total_projects = projects.len();
    let attention_projects = total_projects.saturating_sub(ready_projects);
    let ok = attention_projects == 0;
    Ok(FleetDashboardV2Report {
        version: 1,
        ok,
        since_days,
        total_projects,
        ready_projects,
        attention_projects,
        projects,
        recommendations: if ok {
            vec!["all discovered project memories passed V2 quality gates".to_string()]
        } else {
            vec!["inspect blocked projects and run project-watch --fix".to_string()]
        },
    })
}

pub(crate) fn print_remote_sync_apply_flow(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_apply_flow_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync Apply Flow");
    println!("status: {}", report.status);
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

pub(crate) fn remote_sync_apply_flow_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<RemoteSyncApplyFlowReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let wizard = remote_sync_wizard_report(conn, db, &root, target, since_days, apply)?;
    let dry_run_commands = wizard.sync.commands.clone();
    let mut blockers = wizard.blockers.clone();
    let passphrase_ready = std::env::var("DUKEMEMORY_SYNC_PASSPHRASE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if apply && !passphrase_ready {
        blockers.push("encrypted apply requires DUKEMEMORY_SYNC_PASSPHRASE".to_string());
    }
    if !wizard.sync.local_first {
        blockers.push("remote sync apply requires local-first mode".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let apply_allowed = blockers.is_empty() && target.is_some() && passphrase_ready;
    Ok(RemoteSyncApplyFlowReport {
        version: 1,
        ok: blockers.is_empty(),
        status: if blockers.is_empty() {
            "ready"
        } else {
            "blocked"
        }
        .to_string(),
        root: root.display().to_string(),
        applied: apply && wizard.applied && apply_allowed,
        wizard,
        commands: dry_run_commands.clone(),
        dry_run_commands,
        passphrase_ready,
        apply_allowed,
        blockers,
        recommendations: vec![
            "run dry-run commands and inspect conflicts before apply".to_string(),
            "keep reads local-first; use remote only as encrypted backup/sync target".to_string(),
        ],
    })
}

pub(crate) fn print_mcp_tool_surface_v2(json_out: bool) -> Result<()> {
    let report = mcp_tool_surface_v2_report();
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("MCP Tool Surface v2");
    println!("ok: {}", report.ok);
    for missing in &report.missing_tools {
        println!("missing: {missing}");
    }
    Ok(())
}

pub(crate) fn mcp_tool_surface_v2_report() -> McpToolSurfaceV2Report {
    let expected_tools = mcp_v2_tool_names();
    let exposed_tools = mcp_v2_tool_names();
    let missing_tools = expected_tools
        .iter()
        .filter(|tool| !exposed_tools.contains(tool))
        .cloned()
        .collect::<Vec<_>>();
    McpToolSurfaceV2Report {
        version: 1,
        ok: missing_tools.is_empty(),
        expected_tools,
        exposed_tools,
        missing_tools,
        recommendations: vec![
            "use MCP V2 tools for health, recall explanation, release gates, and fleet checks"
                .to_string(),
        ],
    }
}

fn mcp_v2_tool_names() -> Vec<String> {
    [
        "memory_health_score",
        "memory_explain_recall",
        "memory_control_center_v2",
        "memory_release_gate_v2",
        "memory_quality_ci",
        "memory_fleet_dashboard_v2",
        "memory_governance_policy",
        "memory_status",
        "memory_should_write",
        "memory_after_task",
        "memory_project_health",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn print_autopilot_v3(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = autopilot_v3_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Autopilot v3");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    for action in &report.actions {
        println!("action: {action}");
    }
    Ok(())
}

pub(crate) fn autopilot_v3_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<AutopilotV3Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let learning = self_learning_retrieval_report(conn, &root, since_days, apply)?;
    let role_profile = project_role_profile_report(&root, None, apply)?;
    let inbox_reviewer = inbox_ai_reviewer_report(conn, 100, apply)?;
    let loop_v2 = autonomous_loop_v2_report(conn, db, &root, since_days, apply)?;
    let web_control = web_control_center_v3_report(conn, db, &root, target, since_days)?;
    let remote_sync = remote_sync_apply_report(conn, db, &root, target, since_days, apply)?;
    let mcp_quality = mcp_quality_tools_report();
    let mut actions = Vec::new();
    actions.extend(learning.actions.clone());
    actions.extend(role_profile.actions.clone());
    actions.extend(inbox_reviewer.actions.clone());
    actions.extend(loop_v2.actions.clone());
    actions.extend(
        remote_sync
            .commands
            .iter()
            .map(|command| format!("sync_command:{command}")),
    );
    let mut blockers = Vec::new();
    if !learning.ok {
        blockers.push("self-learning retrieval needs attention".to_string());
    }
    if !role_profile.ok {
        blockers.push("project role profile is not ready".to_string());
    }
    if !loop_v2.ok {
        blockers.extend(loop_v2.blockers.clone());
    }
    if !web_control.ok {
        blockers.push("web control center has attention tabs".to_string());
    }
    if !remote_sync.ok {
        blockers.extend(remote_sync.blockers.clone());
    }
    if !mcp_quality.ok {
        blockers.extend(mcp_quality.missing_quality_tools.clone());
    }
    blockers.sort();
    blockers.dedup();
    let mut recommendations = Vec::new();
    recommendations.extend(learning.recommendations.clone());
    recommendations.extend(role_profile.recommendations.clone());
    recommendations.extend(inbox_reviewer.recommendations.clone());
    recommendations.extend(loop_v2.recommendations.clone());
    recommendations.extend(web_control.recommendations.clone());
    recommendations.extend(remote_sync.recommendations.clone());
    recommendations.extend(mcp_quality.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    Ok(AutopilotV3Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        since_days,
        learning,
        role_profile,
        inbox_reviewer,
        loop_v2,
        web_control,
        remote_sync,
        mcp_quality,
        actions,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_self_learning_retrieval(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = self_learning_retrieval_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Self-Learning Retrieval");
    println!("status: {}", report.status);
    println!("profile: {}", report.selected_profile);
    for signal in &report.signals {
        println!("signal: {signal}");
    }
    Ok(())
}

pub(crate) fn self_learning_retrieval_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<SelfLearningRetrievalReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let ranking = auto_ranking_tune_report(conn, &root, since_days, apply)?;
    let usefulness = usefulness_engine_report(conn, &root, since_days, apply)?;
    let mut signals = ranking.reasons.clone();
    signals.extend(usefulness.ranking_policy.clone());
    signals.sort();
    signals.dedup();
    let mut actions = Vec::new();
    actions.push(format!("selected_profile:{}", ranking.selected_profile));
    if ranking.applied {
        actions.push("ranking_profile_applied".to_string());
    }
    if usefulness.applied {
        actions.push("usefulness_feedback_applied".to_string());
    }
    let mut recommendations = usefulness.recommendations.clone();
    recommendations.extend(ranking.reasons.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = ranking.ok && usefulness.ok;
    Ok(SelfLearningRetrievalReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        selected_profile: ranking.selected_profile.clone(),
        ranking,
        usefulness,
        signals,
        actions,
        recommendations,
    })
}

pub(crate) fn print_project_role_profile(
    root: &Path,
    kind: Option<ProjectTemplateKind>,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = project_role_profile_report(root, kind, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Project Role Profile");
    println!("kind: {}", report.inferred_kind);
    println!("applied: {}", report.applied);
    for reason in &report.reasons {
        println!("reason: {reason}");
    }
    Ok(())
}

pub(crate) fn project_role_profile_report(
    root: &Path,
    kind: Option<ProjectTemplateKind>,
    apply: bool,
) -> Result<ProjectRoleProfileReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let (kind, reasons) = match kind {
        Some(kind) => (kind, vec![format!("explicit role {}", kind)]),
        None => infer_project_template_kind(&root),
    };
    let template = project_template_report(&root, kind, apply)?;
    let profile_path = root.join(".agent/project-role-profile.json");
    let mut actions = template.actions.clone();
    if apply {
        write_file(
            &profile_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "kind": kind.to_string(),
                "reasons": &reasons,
                "template_path": template.path,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("role_profile_written:{}", profile_path.display()));
    } else {
        actions.push("dry_run: role profile not written".to_string());
    }
    Ok(ProjectRoleProfileReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        applied: apply,
        inferred_kind: kind.to_string(),
        profile_path: profile_path.display().to_string(),
        template,
        reasons,
        actions,
        recommendations: vec![
            "use project-role-profile before fleet checks so memory gates match project type"
                .to_string(),
        ],
    })
}

fn infer_project_template_kind(root: &Path) -> (ProjectTemplateKind, Vec<String>) {
    let root_text = root.display().to_string().to_lowercase();
    if root_text.contains("elect") || root_text.contains("cad") {
        return (
            ProjectTemplateKind::ElectronicsCad,
            vec!["project path suggests electronics/CAD workflow".to_string()],
        );
    }
    if root.join("package.json").exists() {
        return (
            ProjectTemplateKind::FrontendApp,
            vec!["package.json detected".to_string()],
        );
    }
    if root.join("Cargo.toml").exists() {
        return (
            ProjectTemplateKind::RustCli,
            vec!["Cargo.toml detected".to_string()],
        );
    }
    if root.join("docs").exists() || root.join("README.md").exists() {
        return (
            ProjectTemplateKind::DocsResearch,
            vec!["documentation-oriented project files detected".to_string()],
        );
    }
    (
        ProjectTemplateKind::DocsResearch,
        vec!["fallback role uses token-light docs/research defaults".to_string()],
    )
}

pub(crate) fn print_inbox_ai_reviewer(
    conn: &Connection,
    limit: usize,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = inbox_ai_reviewer_report(conn, limit, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Inbox AI Reviewer");
    println!("pending: {}", report.pending);
    println!("approve_ready: {}", report.approve_ready);
    for explanation in &report.explanations {
        println!("review: {explanation}");
    }
    Ok(())
}

pub(crate) fn inbox_ai_reviewer_report(
    conn: &Connection,
    limit: usize,
    apply: bool,
) -> Result<InboxAiReviewerReport> {
    let inbox = inbox_v2_report(conn, limit, apply)?;
    let approve_ready = inbox
        .groups
        .iter()
        .filter(|group| group.recommendation == "approve_high_confidence")
        .count();
    let merge_ready = inbox
        .groups
        .iter()
        .filter(|group| group.recommendation == "merge_duplicates")
        .count();
    let reject_ready = inbox
        .groups
        .iter()
        .filter(|group| group.recommendation == "reject_low_confidence")
        .count();
    let keep_pending = inbox
        .groups
        .iter()
        .filter(|group| group.recommendation == "keep_pending")
        .count();
    let explanations = inbox
        .groups
        .iter()
        .take(10)
        .map(|group| {
            format!(
                "{}: {} confidence={:.2} count={}",
                group.recommendation, group.title, group.max_confidence, group.count
            )
        })
        .collect::<Vec<_>>();
    let actions = inbox
        .actions
        .iter()
        .map(|action| format!("{}:{}:{}", action.status, action.kind, action.detail))
        .collect::<Vec<_>>();
    Ok(InboxAiReviewerReport {
        version: 1,
        ok: true,
        applied: apply,
        pending: inbox.pending,
        approve_ready,
        merge_ready,
        reject_ready,
        keep_pending,
        inbox,
        explanations,
        actions,
        recommendations: vec![
            "approve only high-confidence non-policy inbox cards automatically".to_string(),
            "keep decisions and constraints pending unless explicitly reviewed".to_string(),
        ],
    })
}

pub(crate) fn print_web_control_center_v3(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v3_report(conn, db, root, target, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v3");
    println!("status: {}", report.status);
    for tab in &report.tabs {
        println!("{} {:.1} {}", tab.name, tab.score, tab.status);
    }
    Ok(())
}

pub(crate) fn web_control_center_v3_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
) -> Result<WebControlCenterV3Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let health = memory_health_score_report(conn, db, &root, since_days)?;
    let autonomy = autonomy_control_center_report(conn, db, &root, since_days)?;
    let fleet = fleet_dashboard_v2_report(db, since_days)?;
    let sync = remote_sync_apply_flow_report(conn, db, &root, target, since_days, false)?;
    let tabs = vec![
        WebControlTab {
            name: "Health".to_string(),
            status: health.status.clone(),
            score: health.score,
            detail: format!("grade={} issues={}", health.grade, health.issues.len()),
        },
        WebControlTab {
            name: "Autonomy".to_string(),
            status: autonomy.status.clone(),
            score: if autonomy.ok { 100.0 } else { 70.0 },
            detail: format!("issues={}", autonomy.issues.len()),
        },
        WebControlTab {
            name: "Projects".to_string(),
            status: if fleet.ok { "ready" } else { "attention" }.to_string(),
            score: if fleet.total_projects == 0 {
                0.0
            } else {
                (fleet.ready_projects as f64 / fleet.total_projects as f64) * 100.0
            },
            detail: format!(
                "ready={}/{} attention={}",
                fleet.ready_projects, fleet.total_projects, fleet.attention_projects
            ),
        },
        WebControlTab {
            name: "Sync".to_string(),
            status: sync.status.clone(),
            score: if sync.apply_allowed || sync.ok {
                100.0
            } else {
                60.0
            },
            detail: format!("apply_allowed={}", sync.apply_allowed),
        },
    ];
    let ok = tabs.iter().all(|tab| tab.status == "ready");
    let primary_actions = vec![
        "autopilot-v3 --json".to_string(),
        "memory-quality-ci --json".to_string(),
        "fleet-dashboard-v2 --json".to_string(),
        "remote-sync-apply --json".to_string(),
    ];
    Ok(WebControlCenterV3Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        tabs,
        primary_actions,
        recommendations: vec![
            "show Health, Autonomy, Projects, and Sync as first-level web tabs".to_string(),
        ],
    })
}

pub(crate) fn print_remote_sync_apply(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_apply_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync Apply");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for command in &report.commands {
        println!("command: {command}");
    }
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

pub(crate) fn remote_sync_apply_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<RemoteSyncApplyReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let flow = remote_sync_apply_flow_report(conn, db, &root, target, since_days, apply)?;
    let sync_v2 = remote_sync_v2_report(
        conn,
        db,
        &root,
        target,
        since_days,
        apply && flow.apply_allowed,
    )?;
    let sync_profile = sync_profile_report(
        conn,
        db,
        &root,
        SyncProfileMode::LocalFirstBackup,
        target,
        apply && flow.apply_allowed,
        true,
    )?;
    let mut commands = flow.commands.clone();
    commands.extend(sync_profile.commands.clone());
    commands.sort();
    commands.dedup();
    let mut blockers = flow.blockers.clone();
    if !flow.apply_allowed {
        blockers.push("apply requires target and DUKEMEMORY_SYNC_PASSPHRASE".to_string());
    }
    blockers.extend(sync_v2.blockers.clone());
    blockers.extend(sync_profile.blockers.clone());
    blockers.sort();
    blockers.dedup();
    let mut recommendations = flow.recommendations.clone();
    recommendations.extend(sync_v2.recommendations.clone());
    recommendations.extend(sync_profile.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty() || (!apply && flow.ok);
    Ok(RemoteSyncApplyReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        applied: apply && flow.apply_allowed && sync_v2.applied && sync_profile.applied,
        flow,
        sync_v2,
        sync_profile,
        commands,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_mcp_quality_tools(json_out: bool) -> Result<()> {
    let report = mcp_quality_tools_report();
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("MCP Quality Tools");
    println!("ok: {}", report.ok);
    for tool in &report.quality_tools {
        println!("tool: {tool}");
    }
    for missing in &report.missing_quality_tools {
        println!("missing: {missing}");
    }
    Ok(())
}

pub(crate) fn mcp_quality_tools_report() -> McpQualityToolsReport {
    let surface = mcp_tool_surface_v2_report();
    let quality_tools = mcp_quality_tool_names();
    let missing_quality_tools = quality_tools
        .iter()
        .filter(|tool| !surface.exposed_tools.contains(tool))
        .cloned()
        .collect::<Vec<_>>();
    let ok = surface.ok && missing_quality_tools.is_empty();
    McpQualityToolsReport {
        version: 1,
        ok,
        surface,
        quality_tools,
        missing_quality_tools,
        recommended_flow: vec![
            "memory_status".to_string(),
            "memory_should_write".to_string(),
            "memory_after_task".to_string(),
            "memory_project_health".to_string(),
        ],
        recommendations: vec![
            "agents should use quality tools before writing durable memory".to_string(),
        ],
    }
}

fn mcp_quality_tool_names() -> Vec<String> {
    [
        "memory_status",
        "memory_should_write",
        "memory_after_task",
        "memory_project_health",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn print_remote_sync_control(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_control_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync Control");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    for command in &report.dry_run_commands {
        println!("dry-run: {command}");
    }
    Ok(())
}

pub(crate) fn remote_sync_control_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<RemoteSyncControlReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let target_string = target.map(|path| path.display().to_string());
    let bundle_path = target
        .map(remote_sync_control_bundle_path)
        .unwrap_or_else(|| PathBuf::from("TARGET/dukememory-sync-bundle.json"));
    let (remote_bundle_exists, remote_memory_count) =
        remote_sync_control_bundle_summary(&bundle_path);
    let latency = sync_latency_report(conn, db, &root, target, 3)?;
    let sync_profile = sync_profile_report(
        conn,
        db,
        &root,
        SyncProfileMode::LocalFirstSync,
        target,
        false,
        true,
    )?;
    let remote_sync = remote_sync_v2_report(conn, db, &root, target, since_days, false)?;
    let mut blockers = Vec::new();
    if target.is_none() {
        blockers.push("target is required for VDS/local-first sync control".to_string());
    }
    if !latency.ok {
        blockers.extend(latency.issues.iter().cloned());
    }
    if !sync_profile.local_first {
        blockers.push("sync control requires local-first profile".to_string());
    }
    if !remote_sync.local_first {
        blockers.push("remote sync must remain local-first".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let target_arg = target_string.as_deref().unwrap_or("TARGET");
    let dry_run_commands = vec![
        format!("dukememory sync push {target_arg} --dry-run --json"),
        format!("dukememory sync pull {target_arg} --policy manual --dry-run --json"),
        format!("dukememory sync status {target_arg} --json"),
    ];
    let apply_commands = vec![
        format!("dukememory sync push {target_arg} --json"),
        format!("dukememory sync pull {target_arg} --policy newer-wins --dry-run --json"),
        "dukememory embed-index".to_string(),
    ];
    let mut actions = Vec::new();
    let target_ready = target.is_some() && blockers.is_empty();
    if apply && target_ready {
        let path = root.join(".agent/remote-sync-control.json");
        let value = json!({
            "version": 1,
            "target": &target_string,
            "local_first": true,
            "remote_bundle": bundle_path.display().to_string(),
            "dry_run_commands": &dry_run_commands,
            "apply_commands": &apply_commands,
            "updated_at": now_ms(),
        });
        write_file(&path, serde_json::to_string_pretty(&value)?.as_bytes())?;
        let _ = log_event(
            conn,
            "remote_sync_control",
            None,
            &serde_json::to_string(&json!({
                "status": "ok",
                "target": &target_string,
                "path": path.display().to_string(),
            }))?,
        );
        actions.push(format!("remote_sync_control_written:{}", path.display()));
    } else if apply {
        actions.push("remote_sync_control_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:remote_sync_control_not_written".to_string());
    }
    let mut recommendations = latency.recommendations.clone();
    recommendations.extend(sync_profile.recommendations.clone());
    recommendations.extend(remote_sync.recommendations.clone());
    recommendations
        .push("keep all agent reads local; use VDS only for explicit push/pull sync".to_string());
    recommendations
        .push("start with sync push dry-run, then status, then pull manual dry-run".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    Ok(RemoteSyncControlReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        target: target_string,
        applied: apply && ok,
        local_first: true,
        target_ready,
        remote_bundle: bundle_path.display().to_string(),
        remote_bundle_exists,
        remote_memory_count,
        latency,
        sync_profile,
        remote_sync,
        dry_run_commands,
        apply_commands,
        rollback_hint: ".agent/sync-rollbacks stores import rollback backups when pull applies"
            .to_string(),
        blockers,
        actions,
        recommendations,
    })
}

fn remote_sync_control_bundle_path(target: &Path) -> PathBuf {
    if target.extension().and_then(|value| value.to_str()) == Some("json") {
        target.to_path_buf()
    } else {
        target.join("dukememory-sync-bundle.json")
    }
}

fn remote_sync_control_bundle_summary(path: &Path) -> (bool, Option<usize>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return (false, None);
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return (true, None);
    };
    let count = value
        .get("manifest")
        .and_then(|manifest| manifest.get("memory_count"))
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("memories")
                .and_then(Value::as_array)
                .map(|items| items.len() as u64)
        })
        .map(|value| value as usize);
    (true, count)
}

pub(crate) fn print_mcp_discipline_v2(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = mcp_discipline_v2_report(conn, db, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("MCP Discipline v2");
    println!("status: {}", report.status);
    for missing in &report.missing_commands {
        println!("missing: {missing}");
    }
    Ok(())
}

pub(crate) fn mcp_discipline_v2_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<McpDisciplineV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let agent_enforce = agent_enforce_report(conn, db, &root, since_days, apply)?;
    let mcp_quality = mcp_quality_tools_report();
    let budget = budget_plan(
        conn,
        "routine coding task with project memory",
        Some("project"),
    )?;
    let mut missing_commands = agent_enforce.missing_commands.clone();
    missing_commands.extend(mcp_quality.missing_quality_tools.clone());
    missing_commands.sort();
    missing_commands.dedup();
    let startup_flow = vec![
        "memory_status".to_string(),
        "memory_project_health".to_string(),
        "brief --budget-profile tiny".to_string(),
    ];
    let after_task_flow = vec![
        "memory_should_write".to_string(),
        "memory_after_task".to_string(),
        "embed-index after durable writes".to_string(),
    ];
    let mut actions = Vec::new();
    if apply && agent_enforce.fixed {
        actions.push("agent_enforce_fixed".to_string());
    } else if apply {
        actions.push("agent_enforce_checked_no_fix_needed".to_string());
    } else {
        actions.push("dry_run:mcp_discipline_not_written".to_string());
    }
    let mut recommendations = agent_enforce.recommendations.clone();
    recommendations.extend(mcp_quality.recommendations.clone());
    recommendations
        .push("agents should start with memory_status and end with memory_after_task".to_string());
    recommendations.push("use memory_should_write before any durable memory write".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = agent_enforce.ok && mcp_quality.ok && missing_commands.is_empty();
    Ok(McpDisciplineV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        agent_enforce,
        mcp_quality,
        budget,
        startup_flow,
        after_task_flow,
        missing_commands,
        actions,
        recommendations,
    })
}

pub(crate) fn print_feedback_loop_v2(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = feedback_loop_v2_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Feedback Loop v2");
    println!("status: {}", report.status);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn feedback_loop_v2_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<FeedbackLoopV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let usefulness = usefulness_engine_report(conn, &root, since_days, apply)?;
    let auto_feedback = auto_feedback_v2_report(conn, since_days, 100, apply)?;
    let supersede = auto_supersede_v2_report(conn, &root, since_days, apply)?;
    let diff_apply = memory_diff_apply_report(conn, &root, apply)?;
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, apply)?;
    let mut actions = Vec::new();
    if apply {
        actions.push(format!("auto_feedback_written:{}", auto_feedback.written));
        actions.push(format!("superseded:{}", supersede.superseded.len()));
        actions.push(format!(
            "diff_memory_written:{}",
            diff_apply.written_ids.len()
        ));
        if benchmark.baseline_written {
            actions.push("recall_benchmark_baseline_written".to_string());
        }
    } else {
        actions.push("dry_run:feedback_loop_not_applied".to_string());
    }
    let mut issues = qa.issues.clone();
    if !usefulness.ok {
        issues.extend(usefulness.issues.clone());
    }
    if benchmark.regression {
        issues.push("recall benchmark regression detected".to_string());
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = qa.recommendations.clone();
    recommendations.extend(usefulness.recommendations.clone());
    recommendations.extend(auto_feedback.recommendations.clone());
    recommendations.extend(supersede.recommendations.clone());
    recommendations.extend(diff_apply.recommendations.clone());
    recommendations.extend(benchmark.recommendations.clone());
    recommendations
        .push("run feedback-loop-v2 --apply only after reviewing dry-run output".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty() && !benchmark.regression;
    Ok(FeedbackLoopV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        qa,
        usefulness,
        auto_feedback,
        supersede,
        diff_apply,
        benchmark,
        actions,
        issues,
        recommendations,
    })
}

pub(crate) fn print_upgrade_all_projects_v2(
    default_db: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = upgrade_all_projects_v2_report(default_db, from, to, backup_dir, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Upgrade All Projects v2");
    println!("status: {}", report.status);
    println!("ready: {}/{}", report.ready_projects, report.total_projects);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn upgrade_all_projects_v2_report(
    default_db: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
) -> Result<UpgradeAllProjectsV2Report> {
    let base = upgrade_all_projects_report(default_db, from, to, backup_dir, dry_run)?;
    let target_version = env!("CARGO_PKG_VERSION").to_string();
    let project_summaries = base
        .projects
        .iter()
        .map(|project| {
            let root = project
                .get("root")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let ok = project.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let actions = project
                .get("actions")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let current_version = project
                .get("current_version")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    project
                        .get("installed_version")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                });
            UpgradeAllProjectSummary {
                root,
                ok,
                status: if ok { "ready" } else { "attention" }.to_string(),
                actions,
                current_version,
                target_version: target_version.clone(),
            }
        })
        .collect::<Vec<_>>();
    let ready_projects = project_summaries
        .iter()
        .filter(|project| project.ok)
        .count();
    let attention_projects = base.total_projects.saturating_sub(ready_projects);
    let mut issues = base.errors.clone();
    if attention_projects > 0 {
        issues.push(format!("attention_projects={attention_projects}"));
    }
    let commands = vec![
        "dukememory upgrade-all-projects --dry-run --json".to_string(),
        "dukememory upgrade-all-projects --json".to_string(),
        "dukememory project-watch --fix --json".to_string(),
        "dukememory release-gate-v2 --json".to_string(),
    ];
    let mut recommendations = base.recommendations.clone();
    recommendations.push("review project_summaries before non-dry-run upgrades".to_string());
    recommendations
        .push("run project-watch --fix after upgrading all project memories".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = base.ok && issues.is_empty();
    Ok(UpgradeAllProjectsV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        dry_run,
        total_projects: base.total_projects,
        upgraded_projects: base.upgraded_projects,
        ready_projects,
        attention_projects,
        base,
        project_summaries,
        commands,
        issues,
        recommendations,
    })
}

pub(crate) fn print_web_control_center_v4(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v4_report(conn, db, root, target, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v4");
    println!("status: {}", report.status);
    for control in &report.controls {
        println!("{} {} {}", control.name, control.method, control.endpoint);
    }
    Ok(())
}

pub(crate) fn web_control_center_v4_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
) -> Result<WebControlCenterV4Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let v3 = web_control_center_v3_report(conn, db, &root, target, since_days)?;
    let remote_sync = remote_sync_control_report(conn, db, &root, target, since_days, false)?;
    let mcp_discipline = mcp_discipline_v2_report(conn, db, &root, since_days, false)?;
    let feedback_loop = feedback_loop_v2_report(conn, &root, since_days, false)?;
    let upgrade_all = upgrade_all_projects_v2_report(
        db,
        None,
        "~/.local/bin/dukememory",
        Path::new(".agent/install-backups"),
        true,
    )?;
    let controls = vec![
        WebControlAction {
            name: "autopilot".to_string(),
            label: "Run autopilot".to_string(),
            method: "POST".to_string(),
            endpoint: "/autopilot-v3/apply".to_string(),
            cli: "dukememory autopilot-v3 --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: "ready".to_string(),
        },
        WebControlAction {
            name: "remote_sync_dry_run".to_string(),
            label: "Sync dry-run".to_string(),
            method: "GET".to_string(),
            endpoint: "/remote-sync-control".to_string(),
            cli: "dukememory remote-sync-control --target TARGET --json".to_string(),
            safe_auto: true,
            requires_apply: false,
            status: remote_sync.status.clone(),
        },
        WebControlAction {
            name: "mcp_enforce".to_string(),
            label: "MCP enforce".to_string(),
            method: "POST".to_string(),
            endpoint: "/mcp-discipline-v2/apply".to_string(),
            cli: "dukememory mcp-discipline-v2 --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: mcp_discipline.status.clone(),
        },
        WebControlAction {
            name: "feedback_loop".to_string(),
            label: "Feedback loop".to_string(),
            method: "POST".to_string(),
            endpoint: "/feedback-loop-v2/apply".to_string(),
            cli: "dukememory feedback-loop-v2 --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: feedback_loop.status.clone(),
        },
        WebControlAction {
            name: "upgrade_all".to_string(),
            label: "Upgrade projects".to_string(),
            method: "POST".to_string(),
            endpoint: "/upgrade-all-projects-v2/apply".to_string(),
            cli: "dukememory upgrade-all-projects-v2 --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: upgrade_all.status.clone(),
        },
    ];
    let mut recommendations = v3.recommendations.clone();
    recommendations.extend(remote_sync.recommendations.clone());
    recommendations.extend(mcp_discipline.recommendations.clone());
    recommendations.extend(feedback_loop.recommendations.clone());
    recommendations.extend(upgrade_all.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = v3.ok && remote_sync.ok && mcp_discipline.ok && feedback_loop.ok && upgrade_all.ok;
    Ok(WebControlCenterV4Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        v3,
        remote_sync,
        mcp_discipline,
        feedback_loop,
        upgrade_all,
        controls,
        recommendations,
    })
}

pub(crate) fn print_vds_sync_pack(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = vds_sync_pack_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("VDS Sync Pack");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    for command in &report.dry_run_commands {
        println!("dry-run: {command}");
    }
    Ok(())
}

pub(crate) fn vds_sync_pack_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<VdsSyncPackReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let target_string = target.map(|path| path.display().to_string());
    let control = remote_sync_control_report(conn, db, &root, target, since_days, false)?;
    let target_arg = target_string.as_deref().unwrap_or("TARGET");
    let pack_path = root.join(".agent/vds-sync-pack.json");
    let dry_run_commands = vec![
        format!("dukememory sync push {target_arg} --dry-run --json"),
        format!("dukememory sync status {target_arg} --json"),
        format!("dukememory sync pull {target_arg} --policy manual --dry-run --json"),
    ];
    let apply_commands = vec![
        format!("dukememory sync push {target_arg} --json"),
        format!("dukememory sync pull {target_arg} --policy newer-wins --dry-run --json"),
        "dukememory embed-index".to_string(),
    ];
    let verify_commands = vec![
        "dukememory doctor-project --json".to_string(),
        "dukememory sync-latency --json".to_string(),
        "dukememory remote-sync-control --target TARGET --json".to_string(),
    ];
    let mut blockers = control.blockers.clone();
    if target.is_none() {
        blockers.push("target is required before writing VDS sync pack".to_string());
    }
    blockers.sort();
    blockers.dedup();
    let ok = blockers.is_empty();
    let mut actions = Vec::new();
    if apply && ok {
        write_file(
            &pack_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "target": target_string,
                "local_first": true,
                "dry_run_commands": &dry_run_commands,
                "apply_commands": &apply_commands,
                "verify_commands": &verify_commands,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        let _ = log_event(
            conn,
            "vds_sync_pack",
            None,
            &serde_json::to_string(&json!({
                "status": "ok",
                "path": pack_path.display().to_string(),
            }))?,
        );
        actions.push(format!("vds_sync_pack_written:{}", pack_path.display()));
    } else if apply {
        actions.push("vds_sync_pack_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:vds_sync_pack_not_written".to_string());
    }
    let mut recommendations = control.recommendations.clone();
    recommendations
        .push("agents must keep reads local and use VDS only for explicit sync".to_string());
    recommendations
        .push("run push dry-run, status, pull manual dry-run before any real import".to_string());
    recommendations.sort();
    recommendations.dedup();
    Ok(VdsSyncPackReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        target: target_string,
        applied: apply && ok,
        pack_path: pack_path.display().to_string(),
        local_first: true,
        control,
        dry_run_commands,
        apply_commands,
        verify_commands,
        blockers,
        actions,
        recommendations,
    })
}

pub(crate) fn print_quality_autopilot_v31(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = quality_autopilot_v31_report(conn, db, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Quality Autopilot v3.1");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn quality_autopilot_v31_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<QualityAutopilotV31Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let loop_v2 = feedback_loop_v2_report(conn, &root, since_days, apply)?;
    let quality = quality_report(conn, since_days, 50)?;
    let cost_guard = cost_guard_report(conn, since_days)?;
    let health = memory_health_score_report(conn, db, &root, since_days)?;
    let policy_path = root.join(".agent/quality-autopilot-v31.json");
    let mut actions = loop_v2.actions.clone();
    if apply {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "since_days": since_days,
                "quality_average": quality.average_score,
                "cost_profile": cost_guard.recommended_profile,
                "health_score": health.score,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!(
            "quality_autopilot_policy_written:{}",
            policy_path.display()
        ));
    } else {
        actions.push("dry_run:quality_autopilot_policy_not_written".to_string());
    }
    let mut issues = loop_v2.issues.clone();
    issues.extend(cost_guard.issues.clone());
    issues.extend(health.issues.clone());
    if quality.average_score < 70.0 {
        issues.push(format!(
            "quality average is low: {:.1}",
            quality.average_score
        ));
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = loop_v2.recommendations.clone();
    recommendations.extend(cost_guard.actions.clone());
    recommendations.extend(health.recommendations.clone());
    recommendations.push(
        "prefer safe feedback, supersede, diff apply, and benchmark baseline before broad cleanup"
            .to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    let ok = loop_v2.ok && cost_guard.score >= 70.0 && health.ok && quality.average_score >= 70.0;
    Ok(QualityAutopilotV31Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        applied: apply,
        loop_v2,
        quality,
        cost_guard,
        health,
        actions,
        issues,
        recommendations,
    })
}

pub(crate) fn print_memory_router_v2(
    default_db: &Path,
    root: &Path,
    query: &str,
    include_siblings: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_router_v2_report(default_db, root, query, include_siblings)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Router v2");
    println!("status: {}", report.status);
    println!(
        "selected_authority: {}",
        report.selected_authority.as_deref().unwrap_or("-")
    );
    for route in &report.routes {
        println!("{} {} match(es) {}", route.role, route.matches, route.root);
    }
    Ok(())
}

pub(crate) fn memory_router_v2_report(
    default_db: &Path,
    root: &Path,
    query: &str,
    include_siblings: bool,
) -> Result<MemoryRouterV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let base = memory_router_report(default_db, &root, query, include_siblings)?;
    let selected_authority = base
        .routes
        .iter()
        .find(|route| route.current)
        .map(|route| route.root.clone())
        .or_else(|| base.selected_route.clone());
    let routes = base
        .routes
        .iter()
        .map(|route| MemoryRouterV2Route {
            root: route.root.clone(),
            db: route.db.clone(),
            role: if route.current {
                "authoritative".to_string()
            } else {
                "advisory".to_string()
            },
            matches: route.matches,
            selected_for_write: route.current,
            top_titles: route.top_titles.clone(),
        })
        .collect::<Vec<_>>();
    let sibling_hint_count = routes
        .iter()
        .filter(|route| !route.selected_for_write && route.matches > 0)
        .count();
    let guardrails = vec![
        "write only to the current project's .agent/memory.db".to_string(),
        "use sibling memories as hints unless the user explicitly switches project root"
            .to_string(),
        "include sibling routes only when useful; keep briefs token-light".to_string(),
    ];
    let mut warnings = base.warnings.clone();
    if selected_authority.is_none() {
        warnings.push("no authoritative current project route was selected".to_string());
    }
    warnings.sort();
    warnings.dedup();
    let mut recommendations = base.recommendations.clone();
    if sibling_hint_count > 0 {
        recommendations.push(
            "review advisory sibling matches before copying decisions across projects".to_string(),
        );
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = selected_authority.is_some() && warnings.is_empty();
    Ok(MemoryRouterV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        query: query.to_string(),
        include_siblings,
        selected_authority,
        route_count: routes.len(),
        sibling_hint_count,
        routes,
        base,
        guardrails,
        warnings,
        recommendations,
    })
}

pub(crate) fn print_benchmark_profiles(
    conn: &Connection,
    root: &Path,
    kind: Option<ProjectTemplateKind>,
    since_days: i64,
    write_baseline: bool,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = benchmark_profiles_report(conn, root, kind, since_days, write_baseline, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Benchmark Profiles");
    println!("status: {}", report.status);
    println!("kind: {}", report.selected_kind);
    println!("score: {:.1}", report.benchmark.score);
    Ok(())
}

pub(crate) fn benchmark_profiles_report(
    conn: &Connection,
    root: &Path,
    kind: Option<ProjectTemplateKind>,
    since_days: i64,
    write_baseline: bool,
    apply: bool,
) -> Result<BenchmarkProfilesReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let role_profile = project_role_profile_report(&root, kind, apply)?;
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, write_baseline)?;
    let selected_kind = role_profile.inferred_kind.clone();
    let profile_path = root.join(".agent/benchmark-profile.json");
    let commands = vec![
        "dukememory recall-benchmark-suite --json".to_string(),
        "dukememory recall-benchmark-suite --write-baseline --json".to_string(),
        "dukememory project-role-profile --json".to_string(),
        "dukememory memory-test-harness --json".to_string(),
    ];
    let mut actions = role_profile.actions.clone();
    if apply {
        write_file(
            &profile_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "kind": selected_kind,
                "score": benchmark.score,
                "baseline_score": benchmark.baseline_score,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!(
            "benchmark_profile_written:{}",
            profile_path.display()
        ));
    } else {
        actions.push("dry_run:benchmark_profile_not_written".to_string());
    }
    let mut recommendations = benchmark.recommendations.clone();
    recommendations.extend(role_profile.recommendations.clone());
    recommendations.push(
        "treat regressions as release blockers when benchmark score drops more than five points"
            .to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    let ok = benchmark.ok && benchmark.score >= 70.0;
    Ok(BenchmarkProfilesReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        selected_kind,
        since_days,
        profile_path: profile_path.display().to_string(),
        benchmark,
        role_profile,
        commands,
        actions,
        recommendations,
    })
}

pub(crate) fn print_install_polish(root: &Path, apply: bool, json_out: bool) -> Result<()> {
    let report = install_polish_report(root, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Install Polish");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    Ok(())
}

pub(crate) fn install_polish_report(root: &Path, apply: bool) -> Result<InstallPolishReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    let readme = fs::read_to_string(root.join("README.md")).unwrap_or_default();
    let license = fs::read_to_string(root.join("LICENSE")).unwrap_or_default();
    let screenshot_exists = root.join("docs/screenshot.png").exists();
    let checks = vec![
        InstallPolishCheck {
            name: "package_name".to_string(),
            ok: cargo_toml.contains("name = \"dukememory\""),
            detail: "Cargo package should be lowercase dukememory".to_string(),
        },
        InstallPolishCheck {
            name: "package_version".to_string(),
            ok: cargo_toml.contains(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION"))),
            detail: format!(
                "Cargo version should match release {}",
                env!("CARGO_PKG_VERSION")
            ),
        },
        InstallPolishCheck {
            name: "apache_2_license".to_string(),
            ok: cargo_toml.contains("license = \"Apache-2.0\"")
                && license.contains("Apache License"),
            detail: "Apache-2.0 should be declared and present in LICENSE".to_string(),
        },
        InstallPolishCheck {
            name: "github_repository".to_string(),
            ok: cargo_toml.contains("github.com/danilkryachko/dukememory")
                && readme.contains("github.com/danilkryachko/dukememory"),
            detail: "Repository metadata and README should point to GitHub".to_string(),
        },
        InstallPolishCheck {
            name: "screenshot".to_string(),
            ok: screenshot_exists && readme.contains("docs/screenshot.png"),
            detail: "README should show the web UI screenshot".to_string(),
        },
        InstallPolishCheck {
            name: "search_keywords".to_string(),
            ok: cargo_toml.contains("ai-agent")
                && cargo_toml.contains("memory")
                && readme.to_lowercase().contains("local-first"),
            detail: "Metadata should remain searchable for AI agent memory use cases".to_string(),
        },
    ];
    let polish_path = root.join(".agent/install-polish.json");
    let mut actions = Vec::new();
    if apply {
        write_file(
            &polish_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "checks": checks,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("install_polish_written:{}", polish_path.display()));
    } else {
        actions.push("dry_run:install_polish_not_written".to_string());
    }
    let commands = vec![
        "cargo package --list".to_string(),
        "cargo test --test cli".to_string(),
        "dukememory release-gate-v2 --run --json".to_string(),
        "gh repo view danilkryachko/dukememory --json name,visibility,licenseInfo".to_string(),
    ];
    let recommendations = vec![
        "keep README short, screenshot visible, and install commands copy-pasteable".to_string(),
        "publish GitHub updates on minor releases and keep patch releases local unless requested"
            .to_string(),
    ];
    let ok = checks.iter().all(|check| check.ok);
    Ok(InstallPolishReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        checks,
        commands,
        actions,
        recommendations,
    })
}

pub(crate) fn print_web_control_center_v5(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v5_report(conn, db, root, target, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v5");
    println!("status: {}", report.status);
    for control in &report.controls {
        println!("{} {} {}", control.name, control.method, control.endpoint);
    }
    Ok(())
}

pub(crate) fn web_control_center_v5_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
) -> Result<WebControlCenterV5Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let v4 = web_control_center_v4_report(conn, db, &root, target, since_days)?;
    let vds_sync = vds_sync_pack_report(conn, db, &root, target, since_days, false)?;
    let quality_autopilot = quality_autopilot_v31_report(conn, db, &root, since_days, false)?;
    let router = memory_router_v2_report(db, &root, "project memory", true)?;
    let benchmark_profiles =
        benchmark_profiles_report(conn, &root, None, since_days, false, false)?;
    let install_polish = install_polish_report(&root, false)?;
    let target_arg = target
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "TARGET".to_string());
    let mut controls = v4.controls.clone();
    controls.extend(vec![
        WebControlAction {
            name: "vds_sync_pack".to_string(),
            label: "VDS sync pack".to_string(),
            method: "POST".to_string(),
            endpoint: "/vds-sync-pack/apply".to_string(),
            cli: format!("dukememory vds-sync-pack --target {target_arg} --apply --json"),
            safe_auto: true,
            requires_apply: true,
            status: vds_sync.status.clone(),
        },
        WebControlAction {
            name: "quality_autopilot_v31".to_string(),
            label: "Quality autopilot".to_string(),
            method: "POST".to_string(),
            endpoint: "/quality-autopilot-v31/apply".to_string(),
            cli: "dukememory quality-autopilot-v31 --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: quality_autopilot.status.clone(),
        },
        WebControlAction {
            name: "benchmark_profiles".to_string(),
            label: "Benchmark profiles".to_string(),
            method: "POST".to_string(),
            endpoint: "/benchmark-profiles/apply".to_string(),
            cli: "dukememory benchmark-profiles --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: benchmark_profiles.status.clone(),
        },
        WebControlAction {
            name: "install_polish".to_string(),
            label: "Install polish".to_string(),
            method: "POST".to_string(),
            endpoint: "/install-polish/apply".to_string(),
            cli: "dukememory install-polish --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: install_polish.status.clone(),
        },
    ]);
    let mut recommendations = v4.recommendations.clone();
    recommendations.extend(vds_sync.recommendations.clone());
    recommendations.extend(quality_autopilot.recommendations.clone());
    recommendations.extend(router.recommendations.clone());
    recommendations.extend(benchmark_profiles.recommendations.clone());
    recommendations.extend(install_polish.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = v4.ok
        && vds_sync.ok
        && quality_autopilot.ok
        && router.ok
        && benchmark_profiles.ok
        && install_polish.ok;
    Ok(WebControlCenterV5Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        v4,
        vds_sync,
        quality_autopilot,
        router,
        benchmark_profiles,
        install_polish,
        controls,
        recommendations,
    })
}

pub(crate) fn print_memory_effectiveness_lab(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = memory_effectiveness_lab_report(conn, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Effectiveness Lab");
    println!("status: {}", report.status);
    println!("score: {:.1}", report.score);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn memory_effectiveness_lab_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
) -> Result<MemoryEffectivenessLabReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let usage = usage_report(conn, since_days, 30)?;
    let trace = decision_trace_report(conn, since_days, 30)?;
    let roi = roi_report(conn, since_days)?;
    let read_count = usage.read_count.max(trace.traced_reads);
    let empty_rate = ratio(trace.empty_reads, read_count.max(1));
    let questioned_rate = ratio(trace.questioned_reads, trace.influenced_reads.max(1));
    let confirmed_rate = ratio(trace.confirmed_reads, trace.influenced_reads.max(1));
    let semantic_penalty = if usage.semantic_eligible_total > 0 {
        (1.0 - usage.semantic_eligible_result_rate).max(0.0) * 15.0
    } else {
        0.0
    };
    let score = (70.0 + roi.score * 0.20 + confirmed_rate * 20.0
        - empty_rate * 25.0
        - questioned_rate * 20.0
        - semantic_penalty)
        .clamp(0.0, 100.0);
    let mut issues = trace.issues.clone();
    if empty_rate > 0.35 && read_count > 3 {
        issues.push("too many memory reads returned no useful cards".to_string());
    }
    if questioned_rate > 0.25 && trace.influenced_reads > 3 {
        issues.push("questioned memory influence is high".to_string());
    }
    if usage.semantic_eligible_total > 0 && usage.semantic_eligible_result_rate < 0.70 {
        issues.push("semantic eligible reads often return empty results".to_string());
    }
    issues.sort();
    issues.dedup();
    let mut recommendations = trace.recommendations.clone();
    recommendations.extend(roi.recommendations.clone());
    recommendations
        .push("use agent-trace to inspect which cards actually influenced recent work".to_string());
    recommendations
        .push("feed missing/useless signals back before increasing retrieval budget".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty() && score >= 75.0;
    Ok(MemoryEffectivenessLabReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        score,
        read_count,
        influenced_reads: trace.influenced_reads,
        confirmed_reads: trace.confirmed_reads,
        questioned_reads: trace.questioned_reads,
        empty_reads: trace.empty_reads,
        semantic_result_rate: usage.semantic_eligible_result_rate,
        unique_memory_ids: usage.unique_memory_ids,
        trace,
        usage,
        roi,
        issues,
        recommendations,
    })
}

pub(crate) fn print_auto_context_budgeter_v2(
    conn: &Connection,
    root: &Path,
    task: &str,
    target: Option<&str>,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = auto_context_budgeter_v2_report(conn, root, task, target, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Auto Context Budgeter v2");
    println!("status: {}", report.status);
    println!("profile: {}", report.selected_profile);
    for command in &report.selected_commands {
        println!("command: {command}");
    }
    Ok(())
}

pub(crate) fn auto_context_budgeter_v2_report(
    conn: &Connection,
    root: &Path,
    task: &str,
    target: Option<&str>,
    apply: bool,
) -> Result<AutoContextBudgeterV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let governor = context_governor_report(conn, &root, task, target)?;
    let policy_path = root.join(".agent/auto-context-budgeter-v2.json");
    let mut selected_commands = governor.commands.clone();
    if !selected_commands
        .iter()
        .any(|command| command.contains("brief"))
    {
        selected_commands.insert(
            0,
            format!(
                "dukememory brief \"{}\" --budget-profile tiny",
                shell_safe_inline(task)
            ),
        );
    }
    let avoided_commands = vec![
        "full chat transcript dump".to_string(),
        "broad context-pack before brief/impact".to_string(),
        "deep recall unless tiny/normal budget misses".to_string(),
    ];
    let mut actions = Vec::new();
    if apply {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "task": task,
                "target": target,
                "selected_profile": governor.budget.profile,
                "max_chars": governor.max_context_chars,
                "selected_commands": &selected_commands,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!(
            "auto_context_budgeter_v2_written:{}",
            policy_path.display()
        ));
    } else {
        actions.push("dry_run:auto_context_budgeter_v2_not_written".to_string());
    }
    let mut recommendations = governor.reasons.clone();
    recommendations.push(
        "start with the smallest selected command and escalate only on missing evidence"
            .to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    Ok(AutoContextBudgeterV2Report {
        version: 1,
        ok: true,
        status: "ready".to_string(),
        root: root.display().to_string(),
        task: task.to_string(),
        target: target.map(str::to_string),
        applied: apply,
        policy_path: policy_path.display().to_string(),
        selected_profile: governor.budget.profile.clone(),
        max_chars: governor.max_context_chars,
        selected_commands,
        avoided_commands,
        governor,
        actions,
        recommendations,
    })
}

pub(crate) fn print_memory_contract_v2(
    conn: &Connection,
    root: &Path,
    write: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_contract_v2_report(conn, root, write)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Contract v2");
    println!("status: {}", report.status);
    println!("path: {}", report.path);
    Ok(())
}

pub(crate) fn memory_contract_v2_report(
    conn: &Connection,
    root: &Path,
    write: bool,
) -> Result<MemoryContractV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let base = memory_contract_report(conn, &root, write)?;
    let profile = project_profile_snapshot(conn, &root, "project")?;
    let qa = memory_qa_report(conn, &root, 7)?;
    let root_db = root.join(".agent/memory.db");
    let health_db = if root_db.exists() {
        root_db.as_path()
    } else {
        Path::new(DEFAULT_DB)
    };
    let health = memory_health_score_report(conn, health_db, &root, 7).ok();
    let freshness_score = health.as_ref().map(|item| item.score).unwrap_or(qa.score);
    let sections = vec![
        MemoryContractV2Section {
            name: "identity".to_string(),
            status: "ready".to_string(),
            detail: format!(
                "{} cards, budget {}",
                profile.memory_count, profile.recommended_budget
            ),
        },
        MemoryContractV2Section {
            name: "decisions".to_string(),
            status: if profile.decisions > 0 {
                "ready"
            } else {
                "thin"
            }
            .to_string(),
            detail: format!(
                "{} decisions, {} constraints",
                profile.decisions, profile.constraints
            ),
        },
        MemoryContractV2Section {
            name: "operations".to_string(),
            status: if profile.commands > 0 {
                "ready"
            } else {
                "thin"
            }
            .to_string(),
            detail: format!(
                "{} commands, {} known issues",
                profile.commands, profile.known_issues
            ),
        },
        MemoryContractV2Section {
            name: "health".to_string(),
            status: score_status(freshness_score),
            detail: format!("freshness score {:.1}", freshness_score),
        },
    ];
    let path = root.join(".agent/MEMORY_CONTRACT_V2.md");
    let mut actions = Vec::new();
    if write {
        let mut content = base.content.clone();
        content.push_str("\n\n## Contract v2 Signals\n");
        for section in &sections {
            content.push_str(&format!(
                "- {}: {} - {}\n",
                section.name, section.status, section.detail
            ));
        }
        write_file(&path, content.as_bytes())?;
        actions.push(format!("memory_contract_v2_written:{}", path.display()));
        if let Some(id) = &base.memory_id {
            actions.push(format!("memory_contract_card_updated:{id}"));
        }
    } else {
        actions.push("dry_run:memory_contract_v2_not_written".to_string());
    }
    let commands = vec![
        "dukememory memory-contract --write".to_string(),
        "dukememory memory-contract-v2 --write --json".to_string(),
        "dukememory project-intent-map --json".to_string(),
    ];
    let recommendations = vec![
        "keep the contract compact; use it as project-wide context, not as a transcript"
            .to_string(),
        "refresh contract v2 after releases, architecture changes, or large memory cleanup"
            .to_string(),
    ];
    let ok = freshness_score >= 75.0 && sections.iter().all(|section| section.status != "thin");
    Ok(MemoryContractV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        written: write,
        path: path.display().to_string(),
        memory_id: base.memory_id.clone(),
        max_chars: base.max_chars,
        freshness_score,
        sections,
        base,
        commands,
        actions,
        recommendations,
    })
}

pub(crate) fn print_cross_project_learning(
    default_db: &Path,
    root: &Path,
    query: &str,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = cross_project_learning_report(default_db, root, query, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Cross-Project Learning");
    println!("status: {}", report.status);
    println!("advisory_projects: {}", report.advisory_projects);
    Ok(())
}

pub(crate) fn cross_project_learning_report(
    default_db: &Path,
    root: &Path,
    query: &str,
    apply: bool,
) -> Result<CrossProjectLearningReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let router = memory_router_v2_report(default_db, &root, query, true)?;
    let candidates = router
        .routes
        .iter()
        .filter(|route| !route.selected_for_write && route.matches > 0)
        .map(|route| CrossProjectLearningCandidate {
            project_root: route.root.clone(),
            matches: route.matches,
            titles: route.top_titles.iter().take(5).cloned().collect(),
            use_as: "advisory pattern only; do not write into this project".to_string(),
        })
        .collect::<Vec<_>>();
    let policy_path = root.join(".agent/cross-project-learning.json");
    let mut actions = Vec::new();
    if apply {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "query": query,
                "candidate_count": candidates.len(),
                "guardrails": &router.guardrails,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!(
            "cross_project_learning_written:{}",
            policy_path.display()
        ));
    } else {
        actions.push("dry_run:cross_project_learning_not_written".to_string());
    }
    let guardrails = vec![
        "never auto-copy sibling cards into the current project".to_string(),
        "use sibling matches as examples, then write a current-project decision only if validated"
            .to_string(),
        "current project remains the only authoritative write target".to_string(),
    ];
    let mut recommendations = router.recommendations.clone();
    recommendations.push(
        "show sibling matches as hints in Web UI agent trace, not as durable facts".to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    Ok(CrossProjectLearningReport {
        version: 1,
        ok: router.ok,
        status: if router.ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        query: query.to_string(),
        applied: apply,
        policy_path: policy_path.display().to_string(),
        advisory_projects: candidates.len(),
        candidates,
        router,
        guardrails,
        actions,
        recommendations,
    })
}

pub(crate) fn print_agent_trace(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = agent_trace_report(conn, root, since_days, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Agent Trace");
    println!("status: {}", report.status);
    println!("traced_reads: {}", report.traced_reads);
    Ok(())
}

pub(crate) fn agent_trace_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    limit: usize,
) -> Result<AgentTraceReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let trace = decision_trace_report(conn, since_days, limit)?;
    let usage = usage_report(conn, since_days, limit)?;
    let timeline = trace
        .items
        .iter()
        .map(|item| AgentTraceItem {
            read_id: item.read_id,
            command: item.command.clone(),
            query: truncate_chars(&item.query, 180),
            influence: item.influence.clone(),
            memory_count: item.memory_ids.len(),
            semantic_used: item.semantic_used,
            feedback_positive: item.feedback_positive,
            feedback_negative: item.feedback_negative,
            feedback_missing: item.feedback_missing,
            explanation: item.explanation.clone(),
        })
        .collect::<Vec<_>>();
    let mut recommendations = trace.recommendations.clone();
    recommendations.push("use this trace to verify that agents read memory before deciding and write only durable outcomes".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = trace.issues.is_empty();
    Ok(AgentTraceReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        traced_reads: trace.traced_reads,
        influenced_reads: trace.influenced_reads,
        saved_writes: usage.write_count,
        timeline,
        trace,
        recommendations,
    })
}

pub(crate) fn print_vds_sync_hardening(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = vds_sync_hardening_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("VDS Sync Hardening");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    Ok(())
}

pub(crate) fn vds_sync_hardening_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
) -> Result<VdsSyncHardeningReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let pack = vds_sync_pack_report(conn, db, &root, target, since_days, false)?;
    let latency = sync_latency_report(conn, db, &root, target, 3)?;
    let remote_sync = remote_sync_v2_report(conn, db, &root, target, since_days, false)?;
    let apply_flow = remote_sync_apply_flow_report(conn, db, &root, target, since_days, false)?;
    let checks = vec![
        InstallPolishCheck {
            name: "target".to_string(),
            ok: target.is_some(),
            detail: target
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "pass --target PATH before apply".to_string()),
        },
        InstallPolishCheck {
            name: "local_first".to_string(),
            ok: pack.local_first && remote_sync.local_first,
            detail: "agent reads remain local; VDS is sync/backup only".to_string(),
        },
        InstallPolishCheck {
            name: "latency".to_string(),
            ok: latency.ok,
            detail: format!("estimated roundtrip {}ms", latency.estimated_roundtrip_ms),
        },
        InstallPolishCheck {
            name: "dry_run_commands".to_string(),
            ok: !pack.dry_run_commands.is_empty() && !apply_flow.dry_run_commands.is_empty(),
            detail: "push/pull/status dry-run commands are available".to_string(),
        },
        InstallPolishCheck {
            name: "rollback_hint".to_string(),
            ok: !pack.control.rollback_hint.is_empty(),
            detail: pack.control.rollback_hint.clone(),
        },
    ];
    let ok =
        checks.iter().all(|check| check.ok) && pack.ok && latency.ok && remote_sync.local_first;
    let policy_path = root.join(".agent/vds-sync-hardening.json");
    let mut actions = Vec::new();
    if apply && ok {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "target": target.map(|path| path.display().to_string()),
                "local_first": true,
                "estimated_roundtrip_ms": latency.estimated_roundtrip_ms,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!(
            "vds_sync_hardening_written:{}",
            policy_path.display()
        ));
    } else if apply {
        actions.push("vds_sync_hardening_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:vds_sync_hardening_not_written".to_string());
    }
    let mut recommendations = pack.recommendations.clone();
    recommendations.extend(latency.recommendations.clone());
    recommendations.extend(remote_sync.recommendations.clone());
    recommendations.push(
        "keep VDS sync opt-in and measurable; never make remote reads the default".to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    Ok(VdsSyncHardeningReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        applied: apply && ok,
        policy_path: policy_path.display().to_string(),
        pack,
        latency,
        remote_sync,
        apply_flow,
        checks,
        actions,
        recommendations,
    })
}

pub(crate) fn print_install_quality(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = install_quality_report(conn, db, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Install Quality");
    println!("status: {}", report.status);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    Ok(())
}

pub(crate) fn install_quality_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<InstallQualityReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let install_polish = install_polish_report(&root, false)?;
    let doctor = project_doctor_report(conn, db, &root, since_days, false)?;
    let agent_enforce = agent_enforce_report(conn, db, &root, since_days, apply)?;
    let checks = vec![
        InstallPolishCheck {
            name: "install_polish".to_string(),
            ok: install_polish.ok,
            detail: install_polish.status.clone(),
        },
        InstallPolishCheck {
            name: "doctor_project".to_string(),
            ok: doctor.ok,
            detail: doctor.status.clone(),
        },
        InstallPolishCheck {
            name: "agent_enforce".to_string(),
            ok: agent_enforce.ok,
            detail: agent_enforce.status.clone(),
        },
        InstallPolishCheck {
            name: "future_chats".to_string(),
            ok: agent_enforce.missing_commands.is_empty(),
            detail: "AGENTS and skill include required memory commands".to_string(),
        },
    ];
    let ok = checks.iter().all(|check| check.ok);
    let policy_path = root.join(".agent/install-quality.json");
    let mut actions = agent_enforce
        .missing_commands
        .iter()
        .map(|command| format!("missing_command:{command}"))
        .collect::<Vec<_>>();
    if apply && ok {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "status": "ready",
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("install_quality_written:{}", policy_path.display()));
    } else if apply {
        actions.push("install_quality_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:install_quality_not_written".to_string());
    }
    let mut recommendations = install_polish.recommendations.clone();
    recommendations.extend(doctor.recommendations.clone());
    recommendations.extend(agent_enforce.recommendations.clone());
    recommendations
        .push("run install-quality after update-install and upgrade-all-projects-v2".to_string());
    recommendations.sort();
    recommendations.dedup();
    Ok(InstallQualityReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply && ok,
        policy_path: policy_path.display().to_string(),
        install_polish,
        doctor,
        agent_enforce,
        checks,
        actions,
        recommendations,
    })
}

pub(crate) fn print_web_control_center_v6(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v6_report(conn, db, root, target, task, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v6");
    println!("status: {}", report.status);
    for control in &report.controls {
        println!("{} {} {}", control.name, control.method, control.endpoint);
    }
    Ok(())
}

pub(crate) fn web_control_center_v6_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
) -> Result<WebControlCenterV6Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let v5 = web_control_center_v5_report(conn, db, &root, target, since_days)?;
    let effectiveness = memory_effectiveness_lab_report(conn, &root, since_days)?;
    let budgeter = auto_context_budgeter_v2_report(
        conn,
        &root,
        task,
        target.map(|path| path.to_string_lossy()).as_deref(),
        false,
    )?;
    let contract_v2 = memory_contract_v2_report(conn, &root, false)?;
    let cross_project = cross_project_learning_report(db, &root, task, false)?;
    let agent_trace = agent_trace_report(conn, &root, since_days, 12)?;
    let vds_hardening = vds_sync_hardening_report(conn, db, &root, target, since_days, false)?;
    let install_quality = install_quality_report(conn, db, &root, since_days, false)?;
    let target_arg = target
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "TARGET".to_string());
    let mut controls = v5.controls.clone();
    controls.extend(vec![
        WebControlAction {
            name: "auto_context_budgeter_v2".to_string(),
            label: "Context budget".to_string(),
            method: "POST".to_string(),
            endpoint: "/auto-context-budgeter-v2/apply".to_string(),
            cli: format!(
                "dukememory auto-context-budgeter-v2 \"{}\" --apply --json",
                shell_safe_inline(task)
            ),
            safe_auto: true,
            requires_apply: true,
            status: budgeter.status.clone(),
        },
        WebControlAction {
            name: "memory_contract_v2".to_string(),
            label: "Contract v2".to_string(),
            method: "POST".to_string(),
            endpoint: "/memory-contract-v2/write".to_string(),
            cli: "dukememory memory-contract-v2 --write --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: contract_v2.status.clone(),
        },
        WebControlAction {
            name: "cross_project_learning".to_string(),
            label: "Cross-project learning".to_string(),
            method: "POST".to_string(),
            endpoint: "/cross-project-learning/apply".to_string(),
            cli: format!(
                "dukememory cross-project-learning \"{}\" --apply --json",
                shell_safe_inline(task)
            ),
            safe_auto: true,
            requires_apply: true,
            status: cross_project.status.clone(),
        },
        WebControlAction {
            name: "vds_sync_hardening".to_string(),
            label: "VDS hardening".to_string(),
            method: "POST".to_string(),
            endpoint: "/vds-sync-hardening/apply".to_string(),
            cli: format!("dukememory vds-sync-hardening --target {target_arg} --apply --json"),
            safe_auto: true,
            requires_apply: true,
            status: vds_hardening.status.clone(),
        },
        WebControlAction {
            name: "install_quality".to_string(),
            label: "Install quality".to_string(),
            method: "POST".to_string(),
            endpoint: "/install-quality/apply".to_string(),
            cli: "dukememory install-quality --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: install_quality.status.clone(),
        },
    ]);
    let mut recommendations = v5.recommendations.clone();
    recommendations.extend(effectiveness.recommendations.clone());
    recommendations.extend(budgeter.recommendations.clone());
    recommendations.extend(contract_v2.recommendations.clone());
    recommendations.extend(cross_project.recommendations.clone());
    recommendations.extend(agent_trace.recommendations.clone());
    recommendations.extend(vds_hardening.recommendations.clone());
    recommendations.extend(install_quality.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = effectiveness.ok
        && budgeter.ok
        && contract_v2.ok
        && cross_project.ok
        && agent_trace.ok
        && vds_hardening.ok
        && install_quality.ok;
    Ok(WebControlCenterV6Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        v5,
        effectiveness,
        budgeter,
        contract_v2,
        cross_project,
        agent_trace,
        vds_hardening,
        install_quality,
        controls,
        recommendations,
    })
}

pub(crate) fn print_memory_answer(
    conn: &Connection,
    root: &Path,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = memory_answer_report(conn, root, question, scope, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Answer");
    println!("status: {}", report.status);
    println!("{}", report.answer);
    if !report.citations.is_empty() {
        println!("citations:");
        for citation in &report.citations {
            println!(
                "- {} [{}] {}",
                citation.id, citation.memory_type, citation.title
            );
        }
    }
    for gap in &report.gaps {
        println!("gap: {gap}");
    }
    Ok(())
}

pub(crate) fn memory_answer_report(
    conn: &Connection,
    root: &Path,
    question: &str,
    scope: Option<&str>,
    limit: usize,
) -> Result<MemoryAnswerReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let limit = limit.clamp(3, 16);
    let statuses = vec!["active".to_string(), "uncertain".to_string()];
    let (rows, semantic_used) = search_rows_with_semantic_fallback(
        conn,
        SearchRowsRequest {
            query: question,
            types: &[],
            statuses: &statuses,
            scope,
            limit,
            budget: 1_600,
            provider: DEFAULT_EMBED_PROVIDER,
            endpoint: DEFAULT_EMBED_ENDPOINT,
            model: DEFAULT_EMBED_MODEL,
        },
    )?;
    let citations = rows
        .iter()
        .take(limit)
        .map(|memory| MemoryAnswerCitation {
            id: memory.id.clone(),
            memory_type: memory.memory_type.clone(),
            title: memory.title.clone(),
            summary: truncate_chars(&memory.body, 320),
            confidence: memory.confidence,
            status: memory.status.clone(),
            why: memory_answer_citation_why(memory),
        })
        .collect::<Vec<_>>();
    let mut gaps = Vec::new();
    if citations.is_empty() {
        gaps.push("no active or uncertain memory cards matched the question".to_string());
    }
    if citations.len() < 3 {
        gaps.push("answer confidence is limited because few memory cards matched".to_string());
    }
    if citations.iter().any(|item| item.status == "uncertain") {
        gaps.push(
            "some cited memory is uncertain and should be verified before acting".to_string(),
        );
    }
    let certainty = memory_answer_certainty(&citations);
    let needs_verification = citations.is_empty()
        || citations.iter().any(|item| item.status == "uncertain")
        || certainty == "low";
    let evidence_groups = memory_answer_groups(&citations);
    let actionable_answer = memory_answer_actionable_answer(question, &citations, &gaps);
    let answer = if citations.is_empty() {
        format!(
            "I do not have enough grounded project memory to answer \"{}\". Add or approve a concise memory card first, then rerun `dukememory answer`.",
            truncate_chars(question, 140)
        )
    } else {
        let mut text = format!(
            "Grounded answer from {} memory card{}: ",
            citations.len(),
            if citations.len() == 1 { "" } else { "s" }
        );
        let points = citations
            .iter()
            .take(5)
            .map(|item| {
                format!(
                    "{} [{}]: {}",
                    item.title,
                    item.id,
                    truncate_chars(&item.summary, 180)
                )
            })
            .collect::<Vec<_>>();
        text.push_str(&points.join(" "));
        text
    };
    let recommendations = vec![
        "use answer for user-facing grounded summaries; use brief/impact for coding context"
            .to_string(),
        "if a gap is reported, add a reviewed memory card instead of raising the read budget"
            .to_string(),
        "treat cited ids as evidence handles; inspect with dukememory evidence ID when needed"
            .to_string(),
    ];
    let ok = !citations.is_empty();
    Ok(MemoryAnswerReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        question: question.to_string(),
        answer,
        actionable_answer,
        certainty,
        needs_verification,
        semantic_used,
        citation_count: citations.len(),
        citations,
        evidence_groups,
        gaps,
        recommendations,
    })
}

fn memory_answer_citation_why(memory: &Memory) -> String {
    let mut parts = vec![format!("matched {} memory", memory.memory_type)];
    if memory.confidence > 0.0 {
        parts.push(format!("confidence {:.2}", memory.confidence));
    }
    if memory.status == "uncertain" {
        parts.push("requires verification".to_string());
    } else {
        parts.push(format!("status {}", memory.status));
    }
    parts.join("; ")
}

fn memory_answer_certainty(citations: &[MemoryAnswerCitation]) -> String {
    if citations.len() >= 5 && citations.iter().all(|item| item.status == "active") {
        "high"
    } else if citations.len() >= 3 && citations.iter().all(|item| item.status != "rejected") {
        "medium"
    } else {
        "low"
    }
    .to_string()
}

fn memory_answer_groups(citations: &[MemoryAnswerCitation]) -> Vec<MemoryAnswerGroup> {
    let mut grouped: BTreeMap<String, Vec<&MemoryAnswerCitation>> = BTreeMap::new();
    for citation in citations {
        grouped
            .entry(citation.memory_type.clone())
            .or_default()
            .push(citation);
    }
    grouped
        .into_iter()
        .map(|(name, items)| {
            let ids = items
                .iter()
                .take(6)
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            let summary = items
                .iter()
                .take(2)
                .map(|item| format!("{}: {}", item.title, truncate_chars(&item.summary, 120)))
                .collect::<Vec<_>>()
                .join(" ");
            MemoryAnswerGroup {
                name,
                count: items.len(),
                ids,
                summary,
            }
        })
        .collect()
}

fn memory_answer_actionable_answer(
    question: &str,
    citations: &[MemoryAnswerCitation],
    gaps: &[String],
) -> String {
    if citations.is_empty() {
        return format!(
            "No grounded answer yet for \"{}\". Add a reviewed memory card or narrow the question.",
            truncate_chars(question, 120)
        );
    }
    let confirmed = citations
        .iter()
        .filter(|item| item.status == "active")
        .take(4)
        .map(|item| format!("{}: {}", item.title, truncate_chars(&item.summary, 170)))
        .collect::<Vec<_>>();
    let uncertain = citations
        .iter()
        .filter(|item| item.status == "uncertain")
        .take(2)
        .map(|item| item.title.clone())
        .collect::<Vec<_>>();
    let mut parts = Vec::new();
    if !confirmed.is_empty() {
        parts.push(format!("Use: {}", confirmed.join(" ")));
    }
    if !uncertain.is_empty() {
        parts.push(format!(
            "Verify before relying on: {}",
            uncertain.join(", ")
        ));
    }
    if !gaps.is_empty() {
        parts.push(format!("Gap: {}", gaps.join("; ")));
    }
    parts.join(" ")
}

pub(crate) fn print_connect_codex(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = connect_codex_report(conn, db, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Connect Codex");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for check in &report.checks {
        println!("{} {}", if check.ok { "ok" } else { "warn" }, check.name);
    }
    Ok(())
}

pub(crate) fn connect_codex_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<ConnectCodexReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let doctor = project_doctor_report(conn, db, &root, since_days, apply)?;
    let agent_enforce = agent_enforce_report(conn, db, &root, since_days, apply)?;
    let checks = vec![
        InstallPolishCheck {
            name: "memory_db".to_string(),
            ok: root.join(".agent/memory.db").exists(),
            detail: root.join(".agent/memory.db").display().to_string(),
        },
        InstallPolishCheck {
            name: "agents_instructions".to_string(),
            ok: root.join("AGENTS.md").exists() && agent_enforce.missing_commands.is_empty(),
            detail: "future chats should see dukememory instructions without reminders".to_string(),
        },
        InstallPolishCheck {
            name: "codex_skill".to_string(),
            ok: expand_tilde("~/.codex/skills/dukememory-use/SKILL.md").exists(),
            detail: "~/.codex/skills/dukememory-use/SKILL.md".to_string(),
        },
        InstallPolishCheck {
            name: "doctor".to_string(),
            ok: doctor.ok,
            detail: doctor.status.clone(),
        },
    ];
    let ok = checks.iter().all(|check| check.ok) && agent_enforce.ok;
    let policy_path = root.join(".agent/connect-codex.json");
    let mut actions = agent_enforce
        .missing_commands
        .iter()
        .map(|command| format!("missing_command:{command}"))
        .collect::<Vec<_>>();
    if apply && ok {
        write_file(
            &policy_path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "connected": true,
                "agent": "codex",
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
        actions.push(format!("connect_codex_written:{}", policy_path.display()));
    } else if apply {
        actions.push("connect_codex_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:connect_codex_not_written".to_string());
    }
    let commands = vec![
        "dukememory connect-codex --apply --json".to_string(),
        "dukememory doctor-project --fix --json".to_string(),
        "dukememory agent-enforce --fix --json".to_string(),
        "dukememory install-skill".to_string(),
    ];
    let mut recommendations = doctor.recommendations.clone();
    recommendations.extend(agent_enforce.recommendations.clone());
    recommendations.push(
        "restart Codex after MCP/server config changes if the tool surface was not loaded"
            .to_string(),
    );
    recommendations.sort();
    recommendations.dedup();
    Ok(ConnectCodexReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply && ok,
        policy_path: policy_path.display().to_string(),
        doctor,
        agent_enforce,
        commands,
        checks,
        actions,
        recommendations,
    })
}

pub(crate) fn print_memory_type_guide(json_out: bool) -> Result<()> {
    let report = memory_type_guide_report();
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Type Guide");
    for item in &report.types {
        println!("{}: {}", item.memory_type, item.use_when);
    }
    Ok(())
}

pub(crate) fn memory_type_guide_report() -> MemoryTypeGuideReport {
    let types = vec![
        MemoryTypeGuideItem {
            memory_type: "product_goal".to_string(),
            label: "Product goal".to_string(),
            use_when: "the product direction should guide future work".to_string(),
            avoid_when: "it is only a temporary task note".to_string(),
            example: "dukememory must stay local-first, fast, and useful for coding agents"
                .to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "constraint".to_string(),
            label: "Constraint".to_string(),
            use_when: "future agents must obey a rule or boundary".to_string(),
            avoid_when: "the rule is obvious from nearby code".to_string(),
            example: "remote/VDS sync must never make remote reads authoritative by default"
                .to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "decision".to_string(),
            label: "Decision".to_string(),
            use_when: "a durable technical or product choice was accepted".to_string(),
            avoid_when: "the choice is tentative or superseded".to_string(),
            example: "use Rust + SQLite as the default local memory runtime".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "design_note".to_string(),
            label: "Design note".to_string(),
            use_when: "an implementation pattern or subsystem rationale should be reused"
                .to_string(),
            avoid_when: "it is just a changelog entry".to_string(),
            example: "observability/reporting belongs in src/app/observability.rs".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "known_issue".to_string(),
            label: "Known issue".to_string(),
            use_when: "a real bug, risk, or caveat is not fixed yet".to_string(),
            avoid_when: "the issue has been fixed and should be superseded".to_string(),
            example: "semantic provider may be unreachable; fallback to FTS must keep working"
                .to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "command".to_string(),
            label: "Command".to_string(),
            use_when: "a validated build, test, release, or maintenance command matters"
                .to_string(),
            avoid_when: "the command is a one-off scratch command".to_string(),
            example: "cargo test --test cli".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "task_state".to_string(),
            label: "Task state".to_string(),
            use_when: "work must continue across chats with exact state".to_string(),
            avoid_when: "the task is complete and better represented as a decision/design note"
                .to_string(),
            example: "0.26.0 implementation is mid-release; tests still pending".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "user_preference".to_string(),
            label: "User preference".to_string(),
            use_when: "the user gave a durable workflow or product preference".to_string(),
            avoid_when: "it is only relevant to the current prompt".to_string(),
            example: "push to GitHub only on minor version changes".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "domain_fact".to_string(),
            label: "Domain fact".to_string(),
            use_when: "project domain knowledge should be recalled".to_string(),
            avoid_when: "the fact may change often and has no source".to_string(),
            example: "project uses an Ollama bge-m3 embedding endpoint when configured".to_string(),
        },
        MemoryTypeGuideItem {
            memory_type: "note".to_string(),
            label: "Note".to_string(),
            use_when: "no more specific type fits".to_string(),
            avoid_when: "a structured type would make retrieval clearer".to_string(),
            example: "short context note with links".to_string(),
        },
    ];
    MemoryTypeGuideReport {
        version: 1,
        ok: true,
        recommended_order: vec![
            "product_goal".to_string(),
            "constraint".to_string(),
            "decision".to_string(),
            "design_note".to_string(),
            "known_issue".to_string(),
            "command".to_string(),
            "task_state".to_string(),
        ],
        types,
        filters: vec![
            "active: trusted current memory".to_string(),
            "uncertain: useful but must be verified".to_string(),
            "superseded: historical context only".to_string(),
            "rejected: not used for recall".to_string(),
        ],
        guardrails: vec![
            "prefer one compact durable card over transcript-sized notes".to_string(),
            "use inbox/import review for raw documents before approving memory".to_string(),
            "link files or commands when that will improve impact recall".to_string(),
        ],
    }
}

pub(crate) fn print_memory_eval_story(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    write_baseline: bool,
    json_out: bool,
) -> Result<()> {
    let report = memory_eval_story_report(conn, root, since_days, write_baseline)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Eval Story");
    println!("status: {}", report.status);
    for claim in &report.public_claims {
        println!("claim: {claim}");
    }
    Ok(())
}

pub(crate) fn memory_eval_story_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    write_baseline: bool,
) -> Result<MemoryEvalStoryReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, write_baseline)?;
    let profiles = benchmark_profiles_report(conn, &root, None, since_days, false, false)?;
    let harness = memory_test_harness_report(conn, &root, since_days, 8)?;
    let effectiveness = memory_effectiveness_lab_report(conn, &root, since_days)?;
    let ok = !benchmark.regression && harness.score >= 60.0 && effectiveness.score >= 60.0;
    let commands = vec![
        "dukememory memory-eval-story --json".to_string(),
        "dukememory recall-benchmark-suite --json".to_string(),
        "dukememory recall-benchmark-suite --write-baseline --json".to_string(),
        "dukememory memory-test-harness --json".to_string(),
        "dukememory memory-effectiveness-lab --json".to_string(),
        "dukememory benchmark-profiles --json".to_string(),
    ];
    let public_claims = vec![
        format!("recall benchmark score {:.1}", benchmark.harness.score),
        format!("test harness score {:.1}", harness.score),
        format!("effectiveness score {:.1}", effectiveness.score),
        "benchmarks are local, reproducible, and project-specific; they are not broad public dataset claims".to_string(),
    ];
    let proof_points = vec![
        MemoryEvalProofPoint {
            name: "recall_benchmark".to_string(),
            value: format!("{:.1}", benchmark.harness.score),
            status: if benchmark.regression {
                "regression"
            } else {
                "ready"
            }
            .to_string(),
        },
        MemoryEvalProofPoint {
            name: "test_harness".to_string(),
            value: format!("{:.1}", harness.score),
            status: score_status(harness.score),
        },
        MemoryEvalProofPoint {
            name: "effectiveness".to_string(),
            value: format!("{:.1}", effectiveness.score),
            status: score_status(effectiveness.score),
        },
        MemoryEvalProofPoint {
            name: "baseline_write".to_string(),
            value: write_baseline.to_string(),
            status: if write_baseline { "applied" } else { "dry_run" }.to_string(),
        },
    ];
    let mut recommendations = benchmark.recommendations.clone();
    recommendations.extend(profiles.recommendations.clone());
    recommendations.extend(effectiveness.recommendations.clone());
    if !write_baseline {
        recommendations.push("write a baseline only after reviewing stable probes".to_string());
    }
    recommendations.sort();
    recommendations.dedup();
    Ok(MemoryEvalStoryReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        write_baseline,
        benchmark,
        profiles,
        harness,
        effectiveness,
        commands,
        public_claims,
        proof_points,
        recommendations,
    })
}

pub(crate) fn print_import_review(
    conn: &Connection,
    root: &Path,
    input: &Path,
    scope: &str,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    validate_scope(scope)?;
    let report = import_review_report(conn, root, input, scope, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Import Review");
    println!("status: {}", report.status);
    println!("candidates: {}", report.candidate_count);
    println!("inserted: {}", report.inserted);
    Ok(())
}

pub(crate) fn import_review_report(
    conn: &Connection,
    root: &Path,
    input: &Path,
    scope: &str,
    apply: bool,
) -> Result<ImportReviewReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let input = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let content = fs::read_to_string(&input)
        .with_context(|| format!("failed to read import input {}", input.display()))?;
    let candidates = import_review_candidates(conn, &content, &input.display().to_string())?;
    let mut inserted = 0;
    let mut actions = Vec::new();
    if apply {
        for candidate in &candidates {
            if candidate.duplicate {
                actions.push(format!("skipped_duplicate:{}", candidate.title));
                continue;
            }
            reject_sensitive(&candidate.title, &candidate.body, false)?;
            if import_review_inbox_exists(conn, &candidate.title, &input.display().to_string())? {
                actions.push(format!("skipped_duplicate:{}", candidate.title));
                continue;
            }
            let id = Uuid::new_v4().simple().to_string()[..12].to_string();
            let now = now_ms();
            conn.execute(
                r#"
                INSERT INTO memory_inbox (
                    id, type, scope, title, body, source, confidence, status, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?9)
                "#,
                params![
                    id,
                    candidate.memory_type,
                    scope,
                    candidate.title,
                    candidate.body,
                    format!("import_review:{}", input.display()),
                    candidate.confidence,
                    now,
                    now,
                ],
            )?;
            actions.push(format!("inbox_candidate_added:{id}"));
            inserted += 1;
        }
    } else {
        actions.push("dry_run:inbox_candidates_not_written".to_string());
    }
    let recommendations = vec![
        "approve candidates from inbox only after checking they are durable and project-specific"
            .to_string(),
        "reject raw transcript fragments, secrets, temporary logs, and duplicate facts".to_string(),
        "run embed-index after approving imported memory cards".to_string(),
    ];
    let ok = !candidates.is_empty();
    let duplicate_count = candidates
        .iter()
        .filter(|candidate| candidate.duplicate)
        .count();
    let quality_checks = vec![
        InstallPolishCheck {
            name: "input_readable".to_string(),
            ok: !content.trim().is_empty(),
            detail: input.display().to_string(),
        },
        InstallPolishCheck {
            name: "candidate_count".to_string(),
            ok,
            detail: candidates.len().to_string(),
        },
        InstallPolishCheck {
            name: "duplicate_screen".to_string(),
            ok: duplicate_count < candidates.len().max(1),
            detail: format!("{duplicate_count} duplicate candidate(s)"),
        },
        InstallPolishCheck {
            name: "inbox_first".to_string(),
            ok: true,
            detail: "import review writes pending inbox candidates, not active memory".to_string(),
        },
    ];
    Ok(ImportReviewReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        input: input.display().to_string(),
        scope: scope.to_string(),
        applied: apply,
        candidate_count: candidates.len(),
        inserted,
        duplicate_count,
        candidates,
        quality_checks,
        actions,
        recommendations,
    })
}

fn import_review_candidates(
    conn: &Connection,
    content: &str,
    source_path: &str,
) -> Result<Vec<ImportReviewCandidate>> {
    content
        .split("\n\n")
        .map(str::trim)
        .filter(|part| part.chars().filter(|ch| !ch.is_whitespace()).count() >= 40)
        .take(12)
        .map(|part| {
            let first_line = part
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or(part);
            let memory_type = import_review_type(part);
            let title = truncate_chars(
                first_line.trim_matches(|ch: char| matches!(ch, '#' | '-' | '*' | ' ')),
                90,
            );
            let duplicate = memory_title_exists(conn, &memory_type, &title)?
                || import_review_inbox_exists(conn, &title, source_path)?;
            Ok(ImportReviewCandidate {
                title,
                memory_type: memory_type.clone(),
                body: truncate_chars(part, 900),
                confidence: import_review_confidence(part, duplicate),
                reason: import_review_reason(part, &memory_type, duplicate),
                duplicate,
                fingerprint: import_review_fingerprint(part),
            })
        })
        .collect()
}

fn import_review_type(part: &str) -> String {
    let lower = part.to_lowercase();
    if lower.contains("constraint") || lower.contains("must") || lower.contains("нужно") {
        "constraint"
    } else if lower.contains("decision") || lower.contains("decided") || lower.contains("решили")
    {
        "decision"
    } else if lower.contains("bug") || lower.contains("risk") || lower.contains("issue") {
        "known_issue"
    } else if lower.contains("command") || lower.contains("cargo ") || lower.contains("npm ") {
        "command"
    } else if lower.contains("goal") || lower.contains("цель") {
        "product_goal"
    } else {
        "design_note"
    }
    .to_string()
}

fn import_review_confidence(part: &str, duplicate: bool) -> f64 {
    let mut confidence: f64 = 0.58;
    let lower = part.to_lowercase();
    if lower.contains("must")
        || lower.contains("decision")
        || lower.contains("decided")
        || lower.contains("нужно")
        || lower.contains("решили")
    {
        confidence += 0.12;
    }
    if part.contains("file:") || part.contains("cargo ") || part.contains("dukememory ") {
        confidence += 0.08;
    }
    if part.chars().count() > 220 {
        confidence += 0.05;
    }
    if duplicate {
        confidence -= 0.22;
    }
    confidence.clamp(0.35, 0.92)
}

fn import_review_reason(part: &str, memory_type: &str, duplicate: bool) -> String {
    let mut reasons = vec![format!("classified as {memory_type}")];
    let lower = part.to_lowercase();
    if lower.contains("must") || lower.contains("нужно") {
        reasons.push("contains durable requirement language".to_string());
    }
    if lower.contains("decision") || lower.contains("решили") {
        reasons.push("contains decision language".to_string());
    }
    if lower.contains("bug") || lower.contains("risk") || lower.contains("issue") {
        reasons.push("contains risk/issue language".to_string());
    }
    if duplicate {
        reasons.push("similar active memory or pending inbox candidate already exists".to_string());
    }
    reasons.join("; ")
}

fn import_review_fingerprint(part: &str) -> String {
    let normalized = part
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())[..12].to_string()
}

fn import_review_inbox_exists(conn: &Connection, title: &str, source_path: &str) -> Result<bool> {
    let pattern = format!("import_review:{source_path}");
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_inbox WHERE title = ?1 AND source = ?2 AND status IN ('pending', 'approved', 'rejected')",
        params![title, pattern],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

pub(crate) fn print_web_control_center_v7(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v7_report(conn, db, root, target, task, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v7");
    println!("status: {}", report.status);
    for control in &report.controls {
        println!("{} {} {}", control.name, control.method, control.endpoint);
    }
    Ok(())
}

pub(crate) fn web_control_center_v7_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
) -> Result<WebControlCenterV7Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let v6 = web_control_center_v6_report(conn, db, &root, target, task, since_days)?;
    let answer = memory_answer_report(conn, &root, task, None, 8)?;
    let connect_codex = connect_codex_report(conn, db, &root, since_days, false)?;
    let type_guide = memory_type_guide_report();
    let eval_story = memory_eval_story_report(conn, &root, since_days, false)?;
    let mut controls = v6.controls.clone();
    controls.extend(vec![
        WebControlAction {
            name: "answer".to_string(),
            label: "Answer".to_string(),
            method: "GET".to_string(),
            endpoint: "/answer".to_string(),
            cli: format!("dukememory answer \"{}\" --json", shell_safe_inline(task)),
            safe_auto: true,
            requires_apply: false,
            status: answer.status.clone(),
        },
        WebControlAction {
            name: "connect_codex".to_string(),
            label: "Connect Codex".to_string(),
            method: "POST".to_string(),
            endpoint: "/connect-codex/apply".to_string(),
            cli: "dukememory connect-codex --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: connect_codex.status.clone(),
        },
        WebControlAction {
            name: "memory_type_guide".to_string(),
            label: "Type guide".to_string(),
            method: "GET".to_string(),
            endpoint: "/memory-type-guide".to_string(),
            cli: "dukememory memory-type-guide --json".to_string(),
            safe_auto: true,
            requires_apply: false,
            status: "ready".to_string(),
        },
        WebControlAction {
            name: "memory_eval_story".to_string(),
            label: "Eval story".to_string(),
            method: "GET".to_string(),
            endpoint: "/memory-eval-story".to_string(),
            cli: "dukememory memory-eval-story --json".to_string(),
            safe_auto: true,
            requires_apply: false,
            status: eval_story.status.clone(),
        },
        WebControlAction {
            name: "import_review".to_string(),
            label: "Import review".to_string(),
            method: "POST".to_string(),
            endpoint: "/import-review/apply".to_string(),
            cli: "dukememory import-review FILE --apply --json".to_string(),
            safe_auto: false,
            requires_apply: true,
            status: "manual".to_string(),
        },
    ]);
    let mut recommendations = v6.recommendations.clone();
    recommendations.extend(answer.recommendations.clone());
    recommendations.extend(connect_codex.recommendations.clone());
    recommendations.extend(type_guide.guardrails.clone());
    recommendations.extend(eval_story.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = v6.ok && connect_codex.ok && eval_story.ok;
    Ok(WebControlCenterV7Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        v6,
        answer,
        connect_codex,
        type_guide,
        eval_story,
        import_review: None,
        controls,
        recommendations,
    })
}

pub(crate) fn print_autonomous_usefulness(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = autonomous_usefulness_report(conn, root, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Autonomous Usefulness");
    println!("status: {}", report.status);
    println!("applied: {}", report.applied);
    for action in &report.action_plan {
        println!("{}: {} ({})", action.kind, action.target, action.reason);
    }
    Ok(())
}

pub(crate) fn autonomous_usefulness_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    apply: bool,
) -> Result<AutonomousUsefulnessReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let usefulness = usefulness_engine_report(conn, &root, since_days, apply)?;
    let effectiveness = memory_effectiveness_lab_report(conn, &root, since_days)?;
    let quality = quality_report(conn, since_days, 20)?;
    let mut action_plan = Vec::new();
    for id in &usefulness.promote_candidates {
        action_plan.push(AutonomousUsefulnessAction {
            kind: "promote".to_string(),
            target: id.clone(),
            reason: "recent useful reads or positive feedback".to_string(),
            safe_auto: true,
        });
    }
    for id in &usefulness.suppress_candidates {
        action_plan.push(AutonomousUsefulnessAction {
            kind: "soft_suppress_review".to_string(),
            target: id.clone(),
            reason: "weak quality and no recent requests".to_string(),
            safe_auto: false,
        });
    }
    for suggestion in quality.suggestions.iter().take(8) {
        action_plan.push(AutonomousUsefulnessAction {
            kind: suggestion.action.clone(),
            target: suggestion
                .id
                .clone()
                .unwrap_or_else(|| "project_memory".to_string()),
            reason: suggestion.detail.clone(),
            safe_auto: suggestion.action.contains("feedback")
                || suggestion.action.contains("link")
                || suggestion.action.contains("compact"),
        });
    }
    let mut actions = Vec::new();
    if apply {
        actions.push("applied: usefulness-engine feedback materialization requested".to_string());
    } else {
        actions.push("dry_run: autonomous usefulness actions were only planned".to_string());
    }
    let mut recommendations = usefulness.recommendations.clone();
    recommendations.extend(effectiveness.recommendations.clone());
    recommendations.extend(
        quality
            .suggestions
            .iter()
            .take(5)
            .map(|item| format!("{}: {}", item.action, item.detail)),
    );
    recommendations.sort();
    recommendations.dedup();
    let ok = usefulness.ok && effectiveness.ok && quality.average_score >= 60.0;
    Ok(AutonomousUsefulnessReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        applied: apply,
        usefulness,
        effectiveness,
        quality,
        action_plan,
        actions,
        recommendations,
    })
}

pub(crate) fn print_benchmark_polish(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    write_baseline: bool,
    json_out: bool,
) -> Result<()> {
    let report = benchmark_polish_report(conn, root, since_days, write_baseline)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Benchmark Polish");
    println!("status: {}", report.status);
    for summary in &report.public_summary {
        println!("summary: {summary}");
    }
    Ok(())
}

pub(crate) fn benchmark_polish_report(
    conn: &Connection,
    root: &Path,
    since_days: i64,
    write_baseline: bool,
) -> Result<BenchmarkPolishReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let eval_story = memory_eval_story_report(conn, &root, since_days, write_baseline)?;
    let benchmark = recall_benchmark_suite_report(conn, &root, since_days, 8, false)?;
    let dashboard_stats = vec![
        MemoryEvalProofPoint {
            name: "benchmark_score".to_string(),
            value: format!("{:.1}", benchmark.score),
            status: score_status(benchmark.score),
        },
        MemoryEvalProofPoint {
            name: "harness_score".to_string(),
            value: format!("{:.1}", eval_story.harness.score),
            status: score_status(eval_story.harness.score),
        },
        MemoryEvalProofPoint {
            name: "effectiveness_score".to_string(),
            value: format!("{:.1}", eval_story.effectiveness.score),
            status: score_status(eval_story.effectiveness.score),
        },
        MemoryEvalProofPoint {
            name: "regression".to_string(),
            value: benchmark.regression.to_string(),
            status: if benchmark.regression {
                "attention"
            } else {
                "ready"
            }
            .to_string(),
        },
    ];
    let public_summary = vec![
        format!("Local recall benchmark score: {:.1}", benchmark.score),
        format!(
            "Agent effectiveness score: {:.1}",
            eval_story.effectiveness.score
        ),
        "Use these numbers as project-local regression checks, not public leaderboard claims"
            .to_string(),
    ];
    let commands = vec![
        "dukememory benchmark-polish --json".to_string(),
        "dukememory memory-eval-story --json".to_string(),
        "dukememory recall-benchmark-suite --json".to_string(),
        "dukememory benchmark-profiles --json".to_string(),
    ];
    let mut recommendations = eval_story.recommendations.clone();
    recommendations.extend(benchmark.recommendations.clone());
    if benchmark.regression {
        recommendations.push(
            "inspect changed retrieval/ranking behavior before writing a new baseline".to_string(),
        );
    }
    recommendations.sort();
    recommendations.dedup();
    let ok = eval_story.ok && benchmark.ok && !benchmark.regression;
    Ok(BenchmarkPolishReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        write_baseline,
        eval_story,
        benchmark,
        public_summary,
        dashboard_stats,
        commands,
        recommendations,
    })
}

pub(crate) fn print_web_control_center_v8(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = web_control_center_v8_report(conn, db, root, target, task, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Web Control Center v8");
    println!("status: {}", report.status);
    for panel in &report.panels {
        println!("{}: {}", panel.name, panel.headline);
    }
    Ok(())
}

pub(crate) fn web_control_center_v8_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    task: &str,
    since_days: i64,
) -> Result<WebControlCenterV8Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let v7 = web_control_center_v7_report(conn, db, &root, target, task, since_days)?;
    let autonomous_usefulness = autonomous_usefulness_report(conn, &root, since_days, false)?;
    let benchmark_polish = benchmark_polish_report(conn, &root, since_days, false)?;
    let panels = vec![
        WebControlPanel {
            name: "answer_v2".to_string(),
            status: v7.answer.status.clone(),
            headline: format!(
                "{} certainty, {} citation(s)",
                v7.answer.certainty, v7.answer.citation_count
            ),
            metrics: vec![MemoryEvalProofPoint {
                name: "needs_verification".to_string(),
                value: v7.answer.needs_verification.to_string(),
                status: if v7.answer.needs_verification {
                    "attention"
                } else {
                    "ready"
                }
                .to_string(),
            }],
            actions: vec!["dukememory answer \"QUESTION\" --json".to_string()],
        },
        WebControlPanel {
            name: "autonomous_usefulness".to_string(),
            status: autonomous_usefulness.status.clone(),
            headline: format!(
                "{} planned action(s)",
                autonomous_usefulness.action_plan.len()
            ),
            metrics: vec![MemoryEvalProofPoint {
                name: "effectiveness".to_string(),
                value: format!("{:.1}", autonomous_usefulness.effectiveness.score),
                status: score_status(autonomous_usefulness.effectiveness.score),
            }],
            actions: vec![
                "dukememory autonomous-usefulness --json".to_string(),
                "dukememory autonomous-usefulness --apply --json".to_string(),
            ],
        },
        WebControlPanel {
            name: "benchmark_polish".to_string(),
            status: benchmark_polish.status.clone(),
            headline: format!("benchmark {:.1}", benchmark_polish.benchmark.score),
            metrics: benchmark_polish.dashboard_stats.clone(),
            actions: benchmark_polish.commands.clone(),
        },
    ];
    let mut controls = v7.controls.clone();
    controls.extend(vec![
        WebControlAction {
            name: "autonomous_usefulness".to_string(),
            label: "Autonomous usefulness".to_string(),
            method: "POST".to_string(),
            endpoint: "/autonomous-usefulness/apply".to_string(),
            cli: "dukememory autonomous-usefulness --apply --json".to_string(),
            safe_auto: true,
            requires_apply: true,
            status: autonomous_usefulness.status.clone(),
        },
        WebControlAction {
            name: "benchmark_polish".to_string(),
            label: "Benchmark polish".to_string(),
            method: "GET".to_string(),
            endpoint: "/benchmark-polish".to_string(),
            cli: "dukememory benchmark-polish --json".to_string(),
            safe_auto: true,
            requires_apply: false,
            status: benchmark_polish.status.clone(),
        },
    ]);
    let mut recommendations = v7.recommendations.clone();
    recommendations.extend(autonomous_usefulness.recommendations.clone());
    recommendations.extend(benchmark_polish.recommendations.clone());
    recommendations.sort();
    recommendations.dedup();
    let ok = v7.ok && autonomous_usefulness.ok && benchmark_polish.ok;
    Ok(WebControlCenterV8Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        target: target.map(|path| path.display().to_string()),
        v7,
        autonomous_usefulness,
        benchmark_polish,
        panels,
        controls,
        recommendations,
    })
}

fn auto_supersede_confidence(candidate: &MergeCandidate) -> f64 {
    let reason = candidate.reason.to_lowercase();
    let title_bonus: f64 = if reason.contains("same title") || reason.contains("duplicate") {
        0.08
    } else {
        0.0
    };
    (0.86_f64 + title_bonus).min(0.96)
}

fn memory_title_exists(conn: &Connection, memory_type: &str, title: &str) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE type = ?1 AND title = ?2 AND status IN ('active', 'uncertain')",
        params![memory_type, title],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

fn score_status(score: f64) -> String {
    if score >= 85.0 {
        "ready"
    } else if score >= 70.0 {
        "attention"
    } else {
        "blocked"
    }
    .to_string()
}

fn health_grade(score: f64) -> String {
    if score >= 95.0 {
        "A+"
    } else if score >= 90.0 {
        "A"
    } else if score >= 80.0 {
        "B"
    } else if score >= 70.0 {
        "C"
    } else {
        "D"
    }
    .to_string()
}

fn recall_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .map(str::trim)
        .filter(|term| term.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}

fn recall_type_weight(memory_type: &str) -> f64 {
    match memory_type {
        "decision" | "constraint" | "user_preference" => 14.0,
        "task_state" | "known_issue" => 12.0,
        "command" | "design_note" => 10.0,
        "product_goal" => 8.0,
        _ => 4.0,
    }
}

fn intent_items(conn: &Connection, types: &[&str], limit: usize) -> Result<Vec<IntentMapItem>> {
    let type_filters = types
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>();
    let rows = query_memories(
        conn,
        None,
        &type_filters,
        &["active".to_string(), "uncertain".to_string()],
        Some("project"),
        limit,
    )?;
    Ok(rows
        .into_iter()
        .map(|memory| IntentMapItem {
            id: memory.id,
            memory_type: memory.memory_type,
            title: memory.title,
            summary: truncate_chars(&memory.body, 260),
            confidence: memory.confidence,
        })
        .collect())
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
    let mut candidate_cards = Vec::new();
    for file in diff.changed_files.iter().take(10) {
        suggested_memory.push(format!(
            "review durable task_state/design_note for changed file {file}"
        ));
        candidate_cards.push(memory_diff_candidate_for_file(file));
    }
    if diff.changed_files.is_empty() {
        suggested_memory.push("no changed files detected; no memory write suggested".to_string());
    }
    let write_ready = candidate_cards
        .iter()
        .filter(|candidate| candidate.confidence >= 0.85)
        .cloned()
        .collect::<Vec<_>>();
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
                "candidate_cards": &candidate_cards,
                "write_ready": &write_ready,
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
        candidate_cards,
        write_ready,
        stale_memory_ids,
        conflict_count: diff.conflicts,
        actions,
        recommendations: diff.recommendations,
    })
}

fn memory_diff_candidate_for_file(file: &str) -> MemoryDiffCandidate {
    let memory_type = if file.ends_with("Cargo.toml")
        || file.ends_with("Cargo.lock")
        || file.ends_with("README.md")
        || file.ends_with("AGENTS.md")
    {
        "task_state"
    } else if file.ends_with(".rs") || file.ends_with(".html") || file.ends_with(".ts") {
        "design_note"
    } else {
        "known_issue"
    };
    let title = format!("Review durable memory for {file}");
    let body = format!(
        "Changed file {file}. Save a compact durable note only if this change affects future agent behavior, commands, constraints, risks, or project architecture."
    );
    let confidence = if memory_type == "design_note" {
        0.86
    } else {
        0.78
    };
    MemoryDiffCandidate {
        memory_type: memory_type.to_string(),
        title,
        body,
        confidence,
        link: format!("file:{file}"),
        reason: "changed file may carry reusable project context".to_string(),
    }
}

pub(crate) fn print_watch_control(
    db: &Path,
    root: &Path,
    interval_secs: u64,
    label: &str,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = watch_control_report(db, root, interval_secs, label, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Watch Control");
    println!("installed: {}", report.installed);
    println!("running: {}", report.running);
    for action in &report.actions {
        println!("action: {action}");
    }
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn watch_control_report(
    db: &Path,
    root: &Path,
    interval_secs: u64,
    label: &str,
    apply: bool,
) -> Result<WatchControlReport> {
    let install = autonomous_watch_install_report(db, root, interval_secs, label, !apply)?;
    let plist = PathBuf::from(&install.plist);
    let installed = plist.exists();
    let mut actions = install.actions.clone();
    let mut issues = Vec::new();
    let mut running = launchctl_label_running(&install.label);
    if apply {
        if cfg!(target_os = "macos") {
            let output = ProcessCommand::new("launchctl")
                .arg("load")
                .arg("-w")
                .arg(&install.plist)
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    actions.push(format!("launchctl_loaded:{}", install.label));
                    running = launchctl_label_running(&install.label);
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    issues.push(format!("launchctl load failed: {stderr}"));
                }
                Err(err) => issues.push(format!("launchctl unavailable: {err}")),
            }
        } else {
            issues.push("watch-control apply is only supported on macOS launchd".to_string());
        }
    }
    let mut recommendations = Vec::new();
    if !installed && !apply {
        recommendations
            .push("rerun watch-control --apply to write and load the watch plist".to_string());
    }
    if installed && !running {
        recommendations.push("check launchctl list com.dukememory.autonomous-loop".to_string());
    }
    Ok(WatchControlReport {
        version: 1,
        ok: issues.is_empty(),
        root: install.root.clone(),
        applied: apply,
        installed: plist.exists(),
        running,
        label: install.label.clone(),
        plist: install.plist.clone(),
        status_file: install.status_file.clone(),
        install,
        actions,
        issues,
        recommendations,
    })
}

fn launchctl_label_running(label: &str) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    ProcessCommand::new("launchctl")
        .arg("list")
        .arg(label)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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

pub(crate) fn print_remote_sync_v2(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    since_days: i64,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = remote_sync_v2_report(conn, db, root, target, since_days, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Remote Sync V2");
    println!("status: {}", report.status);
    for command in &report.commands {
        println!("command: {command}");
    }
    for blocker in &report.blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

pub(crate) fn remote_sync_v2_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    target: Option<&Path>,
    _since_days: i64,
    apply: bool,
) -> Result<RemoteSyncV2Report> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let latency = sync_latency_report(conn, db, &root, target, 1)?;
    let target_string = target.map(|path| path.display().to_string());
    let target_arg = target_string.as_deref().unwrap_or("TARGET");
    let encrypted_bundle = true;
    let mut blockers = Vec::new();
    if target.is_none() {
        blockers.push(
            "remote-sync-v2 needs --target PATH for apply or measured VDS planning".to_string(),
        );
    }
    if apply && std::env::var("DUKEMEMORY_SYNC_PASSPHRASE").is_err() {
        blockers.push(
            "DUKEMEMORY_SYNC_PASSPHRASE is required before writing an encrypted sync plan"
                .to_string(),
        );
    }
    if !latency.ok {
        blockers.extend(latency.issues.iter().cloned());
    }
    blockers.sort();
    blockers.dedup();
    let commands = vec![
        "dukememory sync export memory-sync.json --dry-run --json".to_string(),
        "openssl enc -aes-256-cbc -pbkdf2 -salt -in memory-sync.json -out memory-sync.json.enc -pass env:DUKEMEMORY_SYNC_PASSPHRASE".to_string(),
        format!("dukememory sync push {target_arg} --dry-run --json"),
        format!("dukememory sync pull {target_arg} --policy manual --dry-run --json"),
        format!("dukememory sync status {target_arg} --json"),
    ];
    let mut recommendations = latency.recommendations.clone();
    recommendations.push(
        "keep agent reads local; never use remote as authoritative memory by default".to_string(),
    );
    recommendations.push("review conflicts manually before import or pull apply".to_string());
    recommendations.sort();
    recommendations.dedup();
    let ok = blockers.is_empty();
    let mut actions = Vec::new();
    if apply && ok {
        let path = root.join(".agent/remote-sync-v2.json");
        let value = json!({
            "version": 1,
            "local_first": true,
            "target": &target_string,
            "encrypted_bundle": encrypted_bundle,
            "conflict_policy": "manual",
            "commands": &commands,
            "updated_at": now_ms(),
        });
        write_file(&path, serde_json::to_string_pretty(&value)?.as_bytes())?;
        actions.push(format!("remote_sync_v2_written:{}", path.display()));
    } else if apply {
        actions.push("remote_sync_v2_not_written:blockers_present".to_string());
    } else {
        actions.push("dry_run:remote_sync_v2_not_written".to_string());
    }
    Ok(RemoteSyncV2Report {
        version: 1,
        ok,
        status: if ok { "ready" } else { "blocked" }.to_string(),
        root: root.display().to_string(),
        target: target_string,
        applied: apply && ok,
        local_first: true,
        encrypted_bundle,
        latency,
        conflict_policy: "manual".to_string(),
        commands,
        actions,
        blockers,
        recommendations,
    })
}

pub(crate) fn print_autonomy_control_center(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
    json_out: bool,
) -> Result<()> {
    let report = autonomy_control_center_report(conn, db, root, since_days)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Autonomy Control Center");
    println!("status: {}", report.status);
    for issue in &report.issues {
        println!("issue: {issue}");
    }
    Ok(())
}

pub(crate) fn autonomy_control_center_report(
    conn: &Connection,
    db: &Path,
    root: &Path,
    since_days: i64,
) -> Result<AutonomyControlCenterReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let qa = memory_qa_report(conn, &root, since_days)?;
    let ranking = auto_ranking_tune_report(conn, &root, since_days, false)?;
    let watch = watch_control_report(db, &root, 3600, "com.dukememory.autonomous-loop", false)?;
    let diff_review = memory_diff_review_report(conn, &root, false)?;
    let remote_sync = remote_sync_v2_report(conn, db, &root, None, since_days, false)?;
    let mut issues = qa.issues.clone();
    if !watch.ok {
        issues.extend(watch.issues.iter().cloned());
    }
    if !diff_review.ok {
        issues.push("memory diff review needs attention".to_string());
    }
    let mut recommendations = qa.recommendations.clone();
    recommendations.extend(ranking.reasons.iter().cloned());
    recommendations.extend(watch.recommendations.iter().cloned());
    recommendations.extend(remote_sync.recommendations.iter().cloned());
    recommendations.sort();
    recommendations.dedup();
    let ok = issues.is_empty();
    Ok(AutonomyControlCenterReport {
        version: 1,
        ok,
        status: if ok { "ready" } else { "attention" }.to_string(),
        root: root.display().to_string(),
        since_days,
        qa,
        ranking,
        watch,
        diff_review,
        remote_sync,
        issues,
        recommendations,
    })
}

pub(crate) fn print_upgrade_all_projects(
    default_db: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
    json_out: bool,
) -> Result<()> {
    let report = upgrade_all_projects_report(default_db, from, to, backup_dir, dry_run)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Upgrade All Projects");
    println!(
        "projects: {}/{}",
        report.upgraded_projects, report.total_projects
    );
    for error in &report.errors {
        println!("error: {error}");
    }
    Ok(())
}

pub(crate) fn upgrade_all_projects_report(
    default_db: &Path,
    from: Option<&Path>,
    to: &str,
    backup_dir: &Path,
    dry_run: bool,
) -> Result<UpgradeAllProjectsReport> {
    let mut projects = Vec::new();
    let mut errors = Vec::new();
    for db in discover_project_dbs(default_db)? {
        let root = app_project_root_for_db(&db).unwrap_or_else(|| {
            db.parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        });
        match open_db(&db)
            .and_then(|conn| upgrade_project_report(&conn, &root, from, to, backup_dir, dry_run))
        {
            Ok(report) => projects.push(serde_json::to_value(report)?),
            Err(err) => errors.push(format!("{}: {err}", root.display())),
        }
    }
    let upgraded_projects = projects
        .iter()
        .filter(|project| project.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let total_projects = projects.len() + errors.len();
    let ok = errors.is_empty() && upgraded_projects == total_projects;
    let recommendations = if ok {
        vec!["all discovered project memories refreshed".to_string()]
    } else {
        vec!["inspect errors and rerun upgrade-all-projects".to_string()]
    };
    Ok(UpgradeAllProjectsReport {
        version: 1,
        ok,
        dry_run,
        total_projects,
        upgraded_projects,
        projects,
        errors,
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
        "context-governor",
        "memory-router",
        "memory-health-score",
        "explain-recall",
        "project-intent-map",
        "memory-test-harness",
        "agent-audit-v2",
        "memory-control-center-v2",
        "auto-supersede-v2",
        "memory-diff-apply",
        "recall-benchmark-suite",
        "release-gate-v2",
        "remote-sync-wizard",
        "memory-governance-policy",
        "autonomous-loop-v2",
        "governance-enforce",
        "memory-quality-ci",
        "fleet-dashboard-v2",
        "remote-sync-apply-flow",
        "mcp-tool-surface-v2",
        "autopilot-v3",
        "self-learning-retrieval",
        "project-role-profile",
        "inbox-ai-reviewer",
        "web-control-center-v3",
        "remote-sync-apply",
        "mcp-quality-tools",
        "remote-sync-control",
        "web-control-center-v4",
        "mcp-discipline-v2",
        "feedback-loop-v2",
        "upgrade-all-projects-v2",
        "vds-sync-pack",
        "web-control-center-v5",
        "quality-autopilot-v31",
        "memory-router-v2",
        "benchmark-profiles",
        "install-polish",
        "memory-effectiveness-lab",
        "auto-context-budgeter-v2",
        "memory-contract-v2",
        "cross-project-learning",
        "agent-trace",
        "vds-sync-hardening",
        "install-quality",
        "web-control-center-v6",
        "answer",
        "connect-codex",
        "memory-type-guide",
        "memory-eval-story",
        "import-review",
        "web-control-center-v7",
        "autonomous-usefulness",
        "benchmark-polish",
        "web-control-center-v8",
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
        "auto-ranking-tune",
        "project-template",
        "watch-control",
        "autonomy-control-center",
        "sync-latency",
        "sync-profile",
        "memory-diff-review",
        "remote-sync-v2",
        "agent-enforce",
        "upgrade-project",
        "upgrade-all-projects",
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

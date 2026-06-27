use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Memory {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) status: String,
    pub(crate) source: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) supersedes: Option<String>,
    pub(crate) superseded_by: Option<String>,
    pub(crate) confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryLink {
    pub(crate) kind: String,
    pub(crate) target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryWithLinks {
    #[serde(flatten)]
    pub(crate) memory: Memory,
    pub(crate) links: Vec<MemoryLink>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct MemoryExport {
    pub(crate) version: u32,
    pub(crate) exported_at: i64,
    pub(crate) memories: Vec<MemoryWithLinks>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EmbeddingRow {
    pub(crate) memory: MemoryWithLinks,
    pub(crate) score: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct RetrievalReport {
    pub(crate) version: u32,
    pub(crate) query: String,
    pub(crate) strategy: String,
    pub(crate) scope: Option<String>,
    pub(crate) semantic_used: bool,
    pub(crate) semantic_skipped: bool,
    pub(crate) semantic_error: Option<String>,
    pub(crate) receipt: String,
    pub(crate) hits: Vec<RetrievalHit>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RetrievalHit {
    pub(crate) memory: MemoryWithLinks,
    pub(crate) score: f64,
    pub(crate) utility_score: f64,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BriefReport {
    pub(crate) version: u32,
    pub(crate) task: String,
    pub(crate) budget: usize,
    pub(crate) semantic_used: bool,
    pub(crate) semantic_skipped: bool,
    pub(crate) semantic_error: Option<String>,
    pub(crate) receipt: String,
    pub(crate) must_follow: Vec<BriefItem>,
    pub(crate) relevant: Vec<BriefItem>,
    pub(crate) risks: Vec<BriefItem>,
    pub(crate) files: Vec<String>,
    pub(crate) checks: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImpactReport {
    pub(crate) version: u32,
    pub(crate) target: String,
    pub(crate) budget: usize,
    pub(crate) receipt: String,
    pub(crate) decisions: Vec<BriefItem>,
    pub(crate) constraints: Vec<BriefItem>,
    pub(crate) risks: Vec<BriefItem>,
    pub(crate) checks: Vec<String>,
    pub(crate) related: Vec<BriefItem>,
    pub(crate) links: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BriefItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) score: f64,
    pub(crate) reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct MemoryEvent {
    pub(crate) id: i64,
    pub(crate) event_type: String,
    pub(crate) memory_id: Option<String>,
    pub(crate) detail: String,
    pub(crate) created_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct LinkReport {
    pub(crate) memory_id: String,
    pub(crate) kind: String,
    pub(crate) target: String,
    pub(crate) status: String,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MergeCandidate {
    pub(crate) primary_id: String,
    pub(crate) duplicate_id: String,
    pub(crate) title: String,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvidenceReport {
    pub(crate) memory: MemoryWithLinks,
    pub(crate) source: Option<String>,
    pub(crate) supersedes_chain: Vec<String>,
    pub(crate) superseded_by: Option<String>,
    pub(crate) audit_events: Vec<MemoryEvent>,
    pub(crate) receipt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DriftReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) changed_only: bool,
    pub(crate) root: String,
    pub(crate) changed_files: Vec<String>,
    pub(crate) missing_links: Vec<LinkReport>,
    pub(crate) conflicts: Vec<MergeCandidate>,
    pub(crate) stale_active: Vec<BriefItem>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InstallUpdateReport {
    pub(crate) version: String,
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) backup: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) changed: bool,
    pub(crate) previous_version: Option<String>,
    pub(crate) source_version: Option<String>,
    pub(crate) previous_sha256: Option<String>,
    pub(crate) source_sha256: String,
    pub(crate) backup_keep: usize,
    pub(crate) pruned_backups: Vec<String>,
    pub(crate) kept_backups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EmbeddingIndexReport {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) indexed: usize,
    pub(crate) skipped: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct OllamaEmbeddingRequest<'a> {
    pub(crate) model: &'a str,
    pub(crate) prompt: &'a str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OllamaEmbeddingResponse {
    pub(crate) embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InboxItem {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) source: Option<String>,
    pub(crate) confidence: f64,
    pub(crate) status: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProviderModel {
    pub(crate) name: String,
    pub(crate) details: Option<Value>,
}

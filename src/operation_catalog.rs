use anyhow::Result;
use serde::Serialize;

pub(crate) const CLI_ADD: &str = "add";
pub(crate) const CLI_REMEMBER: &str = "remember";
pub(crate) const CLI_GET: &str = "get";
pub(crate) const CLI_SEARCH: &str = "search";
pub(crate) const CLI_UPDATE: &str = "update";
pub(crate) const CLI_STATUS: &str = "status";
pub(crate) const CLI_DELETE: &str = "delete";

pub(crate) const MCP_MEMORY_ADD: &str = "memory_add";
pub(crate) const MCP_MEMORY_REMEMBER: &str = "memory_remember";
pub(crate) const MCP_MEMORY_GET: &str = "memory_get";
pub(crate) const MCP_MEMORY_SEARCH: &str = "memory_search";
pub(crate) const MCP_MEMORY_UPDATE: &str = "memory_update";
pub(crate) const MCP_MEMORY_SET_STATUS: &str = "memory_set_status";
pub(crate) const MCP_MEMORY_DELETE: &str = "memory_delete";
pub(crate) const MCP_OPERATIONS: &str = "memory_operations";

pub(crate) const HTTP_OPERATIONS: &str = "/operations";
pub(crate) const HTTP_REMEMBER: &str = "/remember";
pub(crate) const HTTP_MEMORY_GET: &str = "/memory";
pub(crate) const HTTP_MEMORY_UPDATE: &str = "/memory/update";
pub(crate) const HTTP_MEMORY_STATUS: &str = "/memory/status";
pub(crate) const HTTP_MEMORY_DELETE: &str = "/memory/delete";
pub(crate) const HTTP_SEARCH: &str = "/search";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationStability {
    Stable,
    Preview,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum OperationAuthorization {
    #[serde(rename = "project_read")]
    Read,
    #[serde(rename = "project_write")]
    Write,
    #[serde(rename = "project_maintenance")]
    Maintenance,
    #[serde(rename = "project_filesystem")]
    Filesystem,
}

impl OperationStability {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
            Self::Deprecated => "deprecated",
        }
    }
}

impl OperationAuthorization {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "project_read",
            Self::Write => "project_write",
            Self::Maintenance => "project_maintenance",
            Self::Filesystem => "project_filesystem",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OperationSpec {
    pub(crate) id: &'static str,
    pub(crate) category: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) cli: &'static [&'static str],
    pub(crate) mcp: &'static [&'static str],
    pub(crate) http: &'static [&'static str],
    pub(crate) stability: OperationStability,
    pub(crate) authorization: OperationAuthorization,
    pub(crate) mutation: bool,
    pub(crate) supports_dry_run: bool,
    pub(crate) idempotent: bool,
    pub(crate) destructive: bool,
    pub(crate) open_world: bool,
    pub(crate) input_schema: &'static str,
    pub(crate) output_schema: &'static str,
}

macro_rules! operation {
    ($id:literal, $category:literal, $summary:literal, $cli:expr, $mcp:expr, $http:expr, $mutation:literal, $dry_run:literal) => {
        OperationSpec {
            id: $id,
            category: $category,
            summary: $summary,
            cli: $cli,
            mcp: $mcp,
            http: $http,
            stability: OperationStability::Stable,
            authorization: if $mutation {
                OperationAuthorization::Write
            } else {
                OperationAuthorization::Read
            },
            mutation: $mutation,
            supports_dry_run: $dry_run,
            idempotent: !$mutation,
            destructive: false,
            open_world: false,
            input_schema: concat!("https://dukememory.local/schemas/", $id, "/input"),
            output_schema: concat!("https://dukememory.local/schemas/", $id, "/output"),
        }
    };
    ($id:literal, $category:literal, $summary:literal, $cli:expr, $mcp:expr, $http:expr, $mutation:literal, $dry_run:literal;
        $stability:ident, $authorization:ident, $idempotent:literal, $destructive:literal, $open_world:literal) => {
        OperationSpec {
            id: $id,
            category: $category,
            summary: $summary,
            cli: $cli,
            mcp: $mcp,
            http: $http,
            stability: OperationStability::$stability,
            authorization: OperationAuthorization::$authorization,
            mutation: $mutation,
            supports_dry_run: $dry_run,
            idempotent: $idempotent,
            destructive: $destructive,
            open_world: $open_world,
            input_schema: concat!("https://dukememory.local/schemas/", $id, "/input"),
            output_schema: concat!("https://dukememory.local/schemas/", $id, "/output"),
        }
    };
}

pub(crate) const OPERATION_CATALOG: &[OperationSpec] = &[
    operation!(
        "catalog.list",
        "catalog",
        "List stable cross-surface operations",
        &["operations"],
        &[MCP_OPERATIONS],
        &[HTTP_OPERATIONS],
        false,
        false
    ),
    operation!(
        "memory.create",
        "memory",
        "Create durable memory",
        &[CLI_ADD, CLI_REMEMBER],
        &[MCP_MEMORY_ADD, MCP_MEMORY_REMEMBER],
        &[HTTP_REMEMBER],
        true,
        false
    ),
    operation!(
        "memory.get",
        "memory",
        "Read memory cards",
        &[CLI_GET],
        &[MCP_MEMORY_GET],
        &[HTTP_MEMORY_GET],
        false,
        false
    ),
    operation!(
        "memory.search",
        "memory",
        "Search memory",
        &[CLI_SEARCH],
        &[MCP_MEMORY_SEARCH],
        &[HTTP_SEARCH],
        false,
        false
    ),
    operation!(
        "memory.update",
        "memory",
        "Update a memory card",
        &[CLI_UPDATE],
        &[MCP_MEMORY_UPDATE],
        &[HTTP_MEMORY_UPDATE],
        true,
        false;
        Stable, Write, true, false, false
    ),
    operation!(
        "memory.status",
        "memory",
        "Change memory status",
        &[CLI_STATUS],
        &[MCP_MEMORY_SET_STATUS],
        &[HTTP_MEMORY_STATUS],
        true,
        false;
        Stable, Write, true, false, false
    ),
    operation!(
        "memory.delete",
        "memory",
        "Delete a memory card",
        &[CLI_DELETE],
        &[MCP_MEMORY_DELETE],
        &[HTTP_MEMORY_DELETE],
        true,
        false;
        Stable, Maintenance, true, true, false
    ),
    operation!(
        "memory.feedback",
        "memory",
        "Record retrieval usefulness feedback",
        &["feedback"],
        &["memory_feedback"],
        &["/feedback"],
        true,
        false;
        Stable, Write, false, false, false
    ),
    operation!(
        "memory.doctrine",
        "memory",
        "Read active project decisions",
        &["doctrine"],
        &["memory_doctrine"],
        &["/doctrine"],
        false,
        false
    ),
    operation!(
        "memory.evidence",
        "memory",
        "Read provenance for one memory card",
        &["evidence"],
        &["memory_evidence"],
        &["/evidence"],
        false,
        false
    ),
    operation!(
        "evidence.observe",
        "evidence",
        "Record a bitemporal evidence observation",
        &["observe"],
        &["memory_observe"],
        &[],
        true,
        false;
        Preview, Write, false, false, false
    ),
    operation!(
        "evidence.list",
        "evidence",
        "Read evidence observations as-of two times",
        &["observations"],
        &["memory_observations"],
        &[],
        false,
        false;
        Preview, Read, true, false, false
    ),
    operation!(
        "graph.temporal",
        "graph",
        "Read the bitemporal memory graph",
        &["temporal-graph"],
        &["memory_temporal_graph"],
        &[],
        false,
        false;
        Preview, Read, true, false, false
    ),
    operation!(
        "evidence.autopilot",
        "evidence",
        "Create reversible bitemporal evidence from explicit durable-id references",
        &["evidence-autopilot"],
        &["memory_evidence_autopilot"],
        &[
            "/evidence-autopilot",
            "/evidence-autopilot/apply",
            "/evidence-autopilot/rollback",
        ],
        true,
        true;
        Preview, Write, false, true, false
    ),
    operation!(
        "memory.drift",
        "memory",
        "Detect memory drift against project files",
        &["drift"],
        &["memory_drift"],
        &["/drift"],
        false,
        false;
        Stable, Filesystem, true, false, true
    ),
    operation!(
        "retrieval.brief",
        "retrieval",
        "Build a tiny verified task brief",
        &["brief"],
        &["memory_brief"],
        &["/brief"],
        false,
        false
    ),
    operation!(
        "retrieval.impact",
        "retrieval",
        "Find memory relevant to a target",
        &["impact"],
        &["memory_impact"],
        &["/impact"],
        false,
        false
    ),
    operation!(
        "retrieval.context",
        "retrieval",
        "Build a bounded context pack",
        &["context-pack"],
        &["memory_context_pack"],
        &[],
        false,
        false
    ),
    operation!(
        "retrieval.agent_context",
        "retrieval",
        "Build agent-native project context",
        &["context"],
        &["memory_agent_context"],
        &[],
        false,
        false
    ),
    operation!(
        "retrieval.budget_plan",
        "retrieval",
        "Choose the smallest useful context budget",
        &["budget-plan"],
        &["memory_budget_plan"],
        &["/budget-plan"],
        false,
        false
    ),
    operation!(
        "retrieval.recall",
        "retrieval",
        "Return compressed temporal recall",
        &["recall"],
        &["memory_recall"],
        &["/recall"],
        false,
        false
    ),
    operation!(
        "retrieval.rag_answer",
        "retrieval",
        "Answer from grounded project memory",
        &["rag-answer"],
        &["memory_rag_answer"],
        &[],
        false,
        false;
        Stable, Read, true, false, true
    ),
    operation!(
        "retrieval.graph_rag_answer",
        "retrieval",
        "Answer with graph-expanded evidence",
        &["graph-rag"],
        &["memory_graph_rag_answer"],
        &[],
        false,
        false;
        Preview, Read, true, false, true
    ),
    operation!(
        "memory.doctor",
        "operations",
        "Run compact memory health checks",
        &["doctor"],
        &["memory_doctor"],
        &["/doctor"],
        false,
        false
    ),
    operation!(
        "control.status",
        "control",
        "Read the cached project control snapshot",
        &[],
        &["memory_status"],
        &[],
        false,
        false
    ),
    operation!(
        "control.should_write",
        "control",
        "Decide whether a durable memory write is warranted",
        &[],
        &["memory_should_write"],
        &[],
        false,
        false
    ),
    operation!(
        "control.after_task",
        "control",
        "Return after-task memory guidance",
        &[],
        &["memory_after_task"],
        &[],
        false,
        false
    ),
    operation!(
        "control.project_health",
        "control",
        "Read compact project memory health",
        &[],
        &["memory_project_health"],
        &[],
        false,
        false
    ),
    operation!(
        "rag.ingest",
        "rag",
        "Index local source files",
        &["rag-ingest"],
        &["memory_rag_ingest"],
        &["/rag-ingest"],
        true,
        true;
        Stable, Filesystem, true, false, true
    ),
    operation!(
        "rag.sources",
        "rag",
        "Inspect indexed RAG sources",
        &["rag-sources"],
        &["memory_rag_sources"],
        &["/rag-sources"],
        false,
        false
    ),
    operation!(
        "rag.eval",
        "rag",
        "Evaluate grounded RAG retrieval",
        &["eval rag"],
        &["memory_rag_eval"],
        &["/rag-eval"],
        true,
        false;
        Stable, Maintenance, true, false, false
    ),
    operation!(
        "rag.graph_eval",
        "rag",
        "Evaluate graph-RAG relationships",
        &["eval graph-rag"],
        &["memory_graph_rag_eval"],
        &["/graph-rag-eval"],
        false,
        false;
        Preview, Read, true, false, false
    ),
    operation!(
        "memory.advanced_eval",
        "evaluation",
        "Audit causal, poisoning, global, and temporal memory signals",
        &["eval advanced"],
        &["memory_advanced_eval"],
        &["/advanced-eval"],
        false,
        false;
        Preview, Read, true, false, false
    ),
    operation!(
        "release.gate_v2",
        "release",
        "Run V2 release readiness checks",
        &["release-gate-v2"],
        &["memory_release_gate_v2"],
        &["/release-gate-v2"],
        true,
        false;
        Deprecated, Maintenance, true, false, true
    ),
    operation!(
        "release.gate_v3",
        "release",
        "Run V3 release readiness checks",
        &["release-gate-v3"],
        &["memory_release_gate_v3"],
        &["/release-gate-v3"],
        true,
        false;
        Stable, Maintenance, true, false, true
    ),
    operation!(
        "deployment.profile",
        "deployment",
        "Validate local or reverse-proxy deployment security and observability",
        &["deployment-profile"],
        &["memory_deployment_profile"],
        &["/deployment-profile"],
        false,
        false;
        Preview, Filesystem, true, false, true
    ),
    operation!(
        "agent_session.start",
        "agent_session",
        "Start an evidence-backed session",
        &["agent-session start"],
        &["memory_session_start"],
        &["/agent-sessions/start"],
        true,
        false
    ),
    operation!(
        "agent_session.context",
        "agent_session",
        "Load audited session context",
        &["agent-session context"],
        &["memory_session_context"],
        &["/agent-sessions/context"],
        false,
        false
    ),
    operation!(
        "agent_session.claim",
        "agent_session",
        "Claim a worker lease",
        &["agent-session claim"],
        &["memory_session_claim"],
        &["/agent-sessions/claim"],
        true,
        false
    ),
    operation!(
        "agent_session.renew",
        "agent_session",
        "Renew a worker lease",
        &["agent-session renew"],
        &["memory_session_renew"],
        &["/agent-sessions/renew"],
        true,
        false
    ),
    operation!(
        "agent_session.release",
        "agent_session",
        "Release a worker lease",
        &["agent-session release"],
        &["memory_session_release"],
        &["/agent-sessions/release"],
        true,
        false
    ),
    operation!(
        "agent_session.event",
        "agent_session",
        "Record a retry-safe lifecycle event",
        &["agent-session event"],
        &["memory_session_event"],
        &["/agent-sessions/event"],
        true,
        false;
        Stable, Write, true, false, false
    ),
    operation!(
        "agent_session.recover",
        "agent_session",
        "Inspect or claim stale sessions",
        &["agent-session recover"],
        &["memory_session_recover"],
        &["/agent-sessions/recover"],
        true,
        true
    ),
    operation!(
        "agent_session.finish",
        "agent_session",
        "Finish a session with evidence",
        &["agent-session finish"],
        &["memory_session_finish"],
        &["/agent-sessions/finish"],
        true,
        false
    ),
    operation!(
        "agent_session.status",
        "agent_session",
        "Inspect session status",
        &["agent-session status"],
        &["memory_session_status"],
        &["/agent-sessions"],
        false,
        false
    ),
    operation!(
        "agent_session.trace",
        "agent_session",
        "Trace memory influence to outcome",
        &["agent-session trace"],
        &["memory_session_trace"],
        &["/agent-sessions/trace"],
        false,
        false
    ),
    operation!(
        "agent_session.cleanup",
        "agent_session",
        "Apply session retention policy",
        &["agent-session cleanup"],
        &["memory_session_cleanup"],
        &["/agent-sessions/cleanup"],
        true,
        true;
        Stable, Maintenance, true, true, false
    ),
];

pub(crate) fn operation_for_mcp(name: &str) -> Option<&'static OperationSpec> {
    OPERATION_CATALOG
        .iter()
        .find(|operation| operation.mcp.contains(&name))
}

pub(crate) fn operation_for_http(path: &str) -> Option<&'static OperationSpec> {
    OPERATION_CATALOG
        .iter()
        .find(|operation| operation.http.contains(&path))
}

pub(crate) fn print_operation_catalog(json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(OPERATION_CATALOG)?);
        return Ok(());
    }
    for operation in OPERATION_CATALOG {
        println!(
            "{:<30} {:<14} {}",
            operation.id, operation.category, operation.summary
        );
    }
    Ok(())
}

#[cfg(test)]
fn render_operation_markdown() -> String {
    let mut output = String::from(
        "# Operation catalog\n\n\
         Generated from `src/operation_catalog.rs`. This is the stable operation contract shared by CLI, MCP, and HTTP.\n\n\
         Every JSON entry also exposes stable `input_schema` and `output_schema` identifiers used by MCP.\n\n\
         | Operation | Category | Summary | CLI | MCP | HTTP | Stability | Authorization | Mutation | Dry run | Idempotent | Destructive | Open world |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for operation in OPERATION_CATALOG {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | `{}` | `{}` | {} | {} | {} | {} | {} |\n",
            operation.id,
            operation.category,
            operation.summary,
            markdown_names(operation.cli),
            markdown_names(operation.mcp),
            markdown_names(operation.http),
            operation.stability.as_str(),
            operation.authorization.as_str(),
            if operation.mutation { "yes" } else { "no" },
            if operation.supports_dry_run {
                "yes"
            } else {
                "no"
            },
            if operation.idempotent { "yes" } else { "no" },
            if operation.destructive { "yes" } else { "no" },
            if operation.open_world { "yes" } else { "no" },
        ));
    }
    output
}

#[cfg(test)]
fn markdown_names(names: &[&str]) -> String {
    if names.is_empty() {
        "—".to_string()
    } else {
        names
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join("<br>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn operation_catalog_ids_and_surface_names_are_unique() {
        let mut ids = HashSet::new();
        let mut cli = HashSet::new();
        let mut mcp = HashSet::new();
        let mut http = HashSet::new();
        let mut schemas = HashSet::new();
        for operation in OPERATION_CATALOG {
            assert!(ids.insert(operation.id));
            assert!(schemas.insert(operation.input_schema));
            assert!(schemas.insert(operation.output_schema));
            assert!(!operation.mutation || operation.authorization != OperationAuthorization::Read);
            assert!(!operation.destructive || operation.mutation);
            assert!(!operation.supports_dry_run || operation.mutation);
            for name in operation.cli {
                assert!(cli.insert(*name), "duplicate CLI operation: {name}");
            }
            for name in operation.mcp {
                assert!(mcp.insert(*name), "duplicate MCP operation: {name}");
                assert_eq!(
                    operation_for_mcp(name).map(|found| found.id),
                    Some(operation.id)
                );
            }
            for path in operation.http {
                assert!(http.insert(*path), "duplicate HTTP operation: {path}");
            }
        }
        assert_eq!(
            operation_for_mcp("memory_rag_ingest").map(|operation| operation.id),
            Some("rag.ingest")
        );
        assert_eq!(
            operation_for_http("/memory/delete").map(|operation| operation.destructive),
            Some(true)
        );
        for id in [
            "memory.create",
            "memory.get",
            "memory.update",
            "memory.status",
            "memory.delete",
        ] {
            let operation = OPERATION_CATALOG
                .iter()
                .find(|operation| operation.id == id)
                .unwrap();
            assert!(!operation.cli.is_empty(), "missing CLI surface for {id}");
            assert!(!operation.mcp.is_empty(), "missing MCP surface for {id}");
            assert!(!operation.http.is_empty(), "missing HTTP surface for {id}");
        }
    }

    #[test]
    fn checked_in_operation_documentation_matches_the_catalog() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/operations.md");
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            render_operation_markdown()
        );
    }
}

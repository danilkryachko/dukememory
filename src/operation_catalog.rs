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
pub(crate) const MCP_OPERATIONS: &str = "memory_operations";

pub(crate) const HTTP_OPERATIONS: &str = "/operations";
pub(crate) const HTTP_REMEMBER: &str = "/remember";
pub(crate) const HTTP_MEMORY_GET: &str = "/memory";
pub(crate) const HTTP_MEMORY_UPDATE: &str = "/memory/update";
pub(crate) const HTTP_MEMORY_STATUS: &str = "/memory/status";
pub(crate) const HTTP_MEMORY_DELETE: &str = "/memory/delete";
pub(crate) const HTTP_SEARCH: &str = "/search";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OperationSpec {
    pub(crate) id: &'static str,
    pub(crate) category: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) cli: &'static [&'static str],
    pub(crate) mcp: &'static [&'static str],
    pub(crate) http: &'static [&'static str],
    pub(crate) mutation: bool,
    pub(crate) supports_dry_run: bool,
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
            mutation: $mutation,
            supports_dry_run: $dry_run,
        }
    };
}

pub(crate) const OPERATION_CATALOG: &[OperationSpec] = &[
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
        &[],
        &[HTTP_MEMORY_UPDATE],
        true,
        false
    ),
    operation!(
        "memory.status",
        "memory",
        "Change memory status",
        &[CLI_STATUS],
        &[],
        &[HTTP_MEMORY_STATUS],
        true,
        false
    ),
    operation!(
        "memory.delete",
        "memory",
        "Delete a memory card",
        &[CLI_DELETE],
        &[],
        &[HTTP_MEMORY_DELETE],
        true,
        false
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
        "retrieval.rag_answer",
        "retrieval",
        "Answer from grounded project memory",
        &["rag-answer"],
        &["memory_rag_answer"],
        &[],
        false,
        false
    ),
    operation!(
        "retrieval.graph_rag_answer",
        "retrieval",
        "Answer with graph-expanded evidence",
        &["graph-rag"],
        &["memory_graph_rag_answer"],
        &[],
        false,
        false
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
        "rag.ingest",
        "rag",
        "Index local source files",
        &["rag-ingest"],
        &["memory_rag_ingest"],
        &["/rag-ingest"],
        true,
        true
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
        false,
        false
    ),
    operation!(
        "rag.graph_eval",
        "rag",
        "Evaluate graph-RAG relationships",
        &["eval graph-rag"],
        &["memory_graph_rag_eval"],
        &["/graph-rag-eval"],
        false,
        false
    ),
    operation!(
        "release.gate_v2",
        "release",
        "Run V2 release readiness checks",
        &["release-gate-v2"],
        &["memory_release_gate_v2"],
        &["/release-gate-v2"],
        false,
        false
    ),
    operation!(
        "release.gate_v3",
        "release",
        "Run V3 release readiness checks",
        &["release-gate-v3"],
        &["memory_release_gate_v3"],
        &["/release-gate-v3"],
        false,
        false
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
        false
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
        true
    ),
];

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
         | Operation | Category | Summary | CLI | MCP | HTTP | Mutation | Dry run |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for operation in OPERATION_CATALOG {
        output.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | {} |\n",
            operation.id,
            operation.category,
            operation.summary,
            markdown_names(operation.cli),
            markdown_names(operation.mcp),
            markdown_names(operation.http),
            if operation.mutation { "yes" } else { "no" },
            if operation.supports_dry_run {
                "yes"
            } else {
                "no"
            },
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
        for operation in OPERATION_CATALOG {
            assert!(ids.insert(operation.id));
            for name in operation.cli {
                assert!(cli.insert(*name), "duplicate CLI operation: {name}");
            }
            for name in operation.mcp {
                assert!(mcp.insert(*name), "duplicate MCP operation: {name}");
            }
            for path in operation.http {
                assert!(http.insert(*path), "duplicate HTTP operation: {path}");
            }
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

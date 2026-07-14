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
    pub(crate) cli: &'static [&'static str],
    pub(crate) mcp: &'static [&'static str],
    pub(crate) http: &'static [&'static str],
    pub(crate) mutation: bool,
    pub(crate) supports_dry_run: bool,
}

pub(crate) const CORE_OPERATION_CATALOG: &[OperationSpec] = &[
    OperationSpec {
        id: "memory.create",
        cli: &[CLI_ADD, CLI_REMEMBER],
        mcp: &[MCP_MEMORY_ADD, MCP_MEMORY_REMEMBER],
        http: &[HTTP_REMEMBER],
        mutation: true,
        supports_dry_run: false,
    },
    OperationSpec {
        id: "memory.get",
        cli: &[CLI_GET],
        mcp: &[MCP_MEMORY_GET],
        http: &[HTTP_MEMORY_GET],
        mutation: false,
        supports_dry_run: false,
    },
    OperationSpec {
        id: "memory.search",
        cli: &[CLI_SEARCH],
        mcp: &[MCP_MEMORY_SEARCH],
        http: &[HTTP_SEARCH],
        mutation: false,
        supports_dry_run: false,
    },
    OperationSpec {
        id: "memory.update",
        cli: &[CLI_UPDATE],
        mcp: &[],
        http: &[HTTP_MEMORY_UPDATE],
        mutation: true,
        supports_dry_run: false,
    },
    OperationSpec {
        id: "memory.status",
        cli: &[CLI_STATUS],
        mcp: &[],
        http: &[HTTP_MEMORY_STATUS],
        mutation: true,
        supports_dry_run: false,
    },
    OperationSpec {
        id: "memory.delete",
        cli: &[CLI_DELETE],
        mcp: &[],
        http: &[HTTP_MEMORY_DELETE],
        mutation: true,
        supports_dry_run: false,
    },
];

#[cfg(test)]
fn render_core_operation_markdown() -> String {
    let mut output = String::from(
        "# Core operation catalog\n\n\
         Generated from `src/operation_catalog.rs`. The catalog covers the stable core memory operations shared across CLI, MCP, and HTTP.\n\n\
         | Operation | CLI | MCP | HTTP | Mutation | Dry run |\n\
         | --- | --- | --- | --- | --- | --- |\n",
    );
    for operation in CORE_OPERATION_CATALOG {
        output.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            operation.id,
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
        for operation in CORE_OPERATION_CATALOG {
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
            render_core_operation_markdown()
        );
    }
}

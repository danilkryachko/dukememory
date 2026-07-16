use super::*;

pub(super) fn http_required_scope(method: &str, path: &str) -> &'static str {
    if let Some(operation) = operation_for_http(path) {
        return operation.authorization.oauth_scope();
    }
    if matches!(method, "GET" | "HEAD") {
        "memory:read"
    } else {
        "memory:write"
    }
}

pub(super) fn with_insufficient_scope_challenge(
    response: HttpResponse,
    auth_policy: &security::HttpAuthPolicy,
    scope: &str,
) -> HttpResponse {
    if let Some(metadata) = auth_policy.resource_metadata_url() {
        response.with_header(
            "WWW-Authenticate",
            format!(
                "Bearer error=\"insufficient_scope\", scope=\"{scope}\", resource_metadata=\"{metadata}\""
            ),
        )
    } else {
        response
    }
}

pub(super) fn mcp_required_scope(request: &Value) -> Option<&'static str> {
    match request.get("method").and_then(Value::as_str) {
        Some("tools/call") => request
            .get("params")
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
            .and_then(operation_for_mcp)
            .map(|operation| operation.authorization.oauth_scope()),
        Some(
            "initialize"
            | "notifications/initialized"
            | "ping"
            | "tools/list"
            | "resources/list"
            | "resources/read"
            | "resources/templates/list"
            | "prompts/list"
            | "prompts/get"
            | "completion/complete"
            | "tasks/list"
            | "tasks/get"
            | "tasks/result"
            | "server/discover",
        ) => Some("memory:read"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_changing_http_methods_fail_closed_without_a_read_catalog_entry() {
        assert_eq!(http_required_scope("GET", "/memory"), "memory:read");
        assert_eq!(http_required_scope("POST", "/search"), "memory:read");
        assert_eq!(http_required_scope("POST", "/remember"), "memory:write");
        assert_eq!(
            http_required_scope("POST", "/memory/delete"),
            "memory:maintenance"
        );
        assert_eq!(
            http_required_scope("POST", "/rag-ingest"),
            "memory:filesystem"
        );
        assert_eq!(
            http_required_scope("POST", "/unknown-future-route"),
            "memory:write"
        );
        assert_eq!(
            http_required_scope("DELETE", "/unknown-future-route"),
            "memory:write"
        );
    }

    #[test]
    fn mcp_read_only_authorization_fails_closed_for_unknown_or_write_tools() {
        assert_eq!(
            mcp_required_scope(&json!({"method": "tools/list"})),
            Some("memory:read")
        );
        assert_eq!(
            mcp_required_scope(&json!({
                "method": "tools/call",
                "params": {"name": "memory_status"}
            })),
            Some("memory:read")
        );
        assert_eq!(
            mcp_required_scope(&json!({
                "method": "tools/call",
                "params": {"name": "memory_remember"}
            })),
            Some("memory:write")
        );
        assert_eq!(
            mcp_required_scope(&json!({
                "method": "tools/call",
                "params": {"name": "memory_delete"}
            })),
            Some("memory:maintenance")
        );
        assert_eq!(
            mcp_required_scope(&json!({
                "method": "tools/call",
                "params": {"name": "memory_rag_ingest"}
            })),
            Some("memory:filesystem")
        );
        assert_eq!(
            mcp_required_scope(&json!({
                "method": "tools/call",
                "params": {"name": "unknown_future_tool"}
            })),
            None
        );
    }

    #[test]
    fn oauth_scope_failures_advertise_step_up_metadata() {
        let response = with_insufficient_scope_challenge(
            HttpResponse::forbidden("scope required"),
            &security::HttpAuthPolicy::oauth_test_policy(),
            "memory:write",
        );
        assert_eq!(response.status, 403);
        assert!(response.headers.iter().any(|(name, value)| {
            *name == "WWW-Authenticate"
                && value.contains("error=\"insufficient_scope\"")
                && value.contains("scope=\"memory:write\"")
                && value.contains("/.well-known/oauth-protected-resource")
        }));
    }
}

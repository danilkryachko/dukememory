use super::*;

pub(super) fn http_read_only_request_allowed(method: &str, path: &str) -> bool {
    if let Some(operation) = operation_for_http(path) {
        return operation.authorization == OperationAuthorization::Read;
    }
    matches!(method, "GET" | "HEAD")
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

pub(super) fn mcp_read_only_request_allowed(request: &Value) -> bool {
    match request.get("method").and_then(Value::as_str) {
        Some("tools/call") => request
            .get("params")
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
            .and_then(operation_for_mcp)
            .is_some_and(|operation| operation.authorization == OperationAuthorization::Read),
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
        ) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_changing_http_methods_fail_closed_without_a_read_catalog_entry() {
        assert!(http_read_only_request_allowed("GET", "/memory"));
        assert!(http_read_only_request_allowed("POST", "/search"));
        assert!(!http_read_only_request_allowed("POST", "/remember"));
        assert!(!http_read_only_request_allowed(
            "POST",
            "/unknown-future-route"
        ));
        assert!(!http_read_only_request_allowed(
            "DELETE",
            "/unknown-future-route"
        ));
    }

    #[test]
    fn mcp_read_only_authorization_fails_closed_for_unknown_or_write_tools() {
        assert!(mcp_read_only_request_allowed(
            &json!({"method": "tools/list"})
        ));
        assert!(mcp_read_only_request_allowed(&json!({
            "method": "tools/call",
            "params": {"name": "memory_status"}
        })));
        assert!(!mcp_read_only_request_allowed(&json!({
            "method": "tools/call",
            "params": {"name": "memory_remember"}
        })));
        assert!(!mcp_read_only_request_allowed(&json!({
            "method": "tools/call",
            "params": {"name": "unknown_future_tool"}
        })));
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

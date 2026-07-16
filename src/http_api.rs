use crate::domain::DomainParseError;
use anyhow::Error;
use serde_json::{Value, json};
use std::io::{Result, Write};
use std::net::TcpStream;
use uuid::Uuid;

pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub request_id: Option<String>,
    pub headers: Vec<(&'static str, String)>,
}

impl HttpResponse {
    pub fn ok(body: Value) -> Self {
        Self::json(200, "OK", body)
    }

    pub fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type: "text/html; charset=utf-8",
            body: body.into().into_bytes(),
            request_id: None,
            headers: Vec::new(),
        }
    }

    pub fn asset(content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            content_type,
            body: body.into(),
            request_id: None,
            headers: Vec::new(),
        }
    }

    pub fn accepted() -> Self {
        Self::empty(202, "Accepted")
    }

    pub fn no_content() -> Self {
        Self::empty(204, "No Content")
    }

    pub fn method_not_allowed() -> Self {
        Self::json(
            405,
            "Method Not Allowed",
            json!({"error": {"code": "method_not_allowed", "message": "method not allowed"}}),
        )
    }

    pub fn json_rpc(status: u16, reason: &'static str, body: Value) -> Self {
        Self::json(status, reason, body)
    }

    pub fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte.is_ascii_control())
        {
            self.headers.push((name, value));
        }
        self
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::json(
            400,
            "Bad Request",
            json!({"error": {"code": "bad_request", "message": message.into()}}),
        )
    }

    pub fn unauthorized() -> Self {
        Self::json(
            401,
            "Unauthorized",
            json!({"error": {"code": "unauthorized", "message": "missing or invalid HTTP bearer token"}}),
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::json(
            403,
            "Forbidden",
            json!({"error": {"code": "forbidden", "message": message.into()}}),
        )
    }

    pub fn not_found() -> Self {
        Self::json(
            404,
            "Not Found",
            json!({"error": {"code": "not_found", "message": "endpoint not found"}}),
        )
    }

    pub fn not_found_message(message: impl Into<String>) -> Self {
        Self::json(
            404,
            "Not Found",
            json!({"error": {"code": "not_found", "message": message.into()}}),
        )
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::json(
            409,
            "Conflict",
            json!({"error": {"code": "conflict", "message": message.into()}}),
        )
    }

    pub fn request_timeout() -> Self {
        Self::json(
            408,
            "Request Timeout",
            json!({"error": {"code": "request_timeout", "message": "HTTP request deadline exceeded"}}),
        )
    }

    pub fn too_many_requests(retry_after_seconds: u64) -> Self {
        Self::json(
            429,
            "Too Many Requests",
            json!({"error": {"code": "rate_limited", "message": "HTTP request rate limit exceeded"}}),
        )
        .with_header("Retry-After", retry_after_seconds.to_string())
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::json(
            503,
            "Service Unavailable",
            json!({"error": {"code": "service_unavailable", "message": message.into()}}),
        )
    }

    pub fn internal_error(incident_id: &str) -> Self {
        Self::json(
            500,
            "Internal Server Error",
            json!({"error": {
                "code": "internal_error",
                "message": "internal server error",
                "incident_id": incident_id
            }}),
        )
    }

    pub fn from_error(error: &Error) -> Self {
        let message = error.to_string();
        let normalized = error
            .chain()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(": ")
            .to_ascii_lowercase();
        if error
            .chain()
            .any(|cause| cause.downcast_ref::<DomainParseError>().is_some())
            || normalized.starts_with("missing ")
            || normalized.starts_with("invalid ")
            || normalized.contains("request body must")
            || normalized.contains("must not be empty")
            || normalized.contains("must be between")
            || normalized.contains("links must")
            || normalized.contains("outside selected project root")
            || normalized.contains("looks like it may contain a secret")
        {
            return Self::bad_request(message);
        }
        if normalized.contains("not found") {
            return Self::not_found_message(message);
        }
        if normalized.contains("http request deadline exceeded") {
            return Self::request_timeout();
        }
        if normalized.contains("constraint failed")
            || normalized.contains("already exists")
            || normalized.contains("lease conflict")
        {
            return Self::conflict(message);
        }
        let incident_id = Uuid::new_v4().simple().to_string()[..12].to_string();
        eprintln!(
            "{}",
            json!({
                "event": "http_internal_error",
                "incident_id": incident_id.clone(),
                "error": error.chain().map(ToString::to_string).collect::<Vec<_>>(),
            })
        );
        Self::internal_error(&incident_id)
    }

    fn json(status: u16, reason: &'static str, body: Value) -> Self {
        let body = serde_json::to_vec(&body).unwrap_or_else(|err| {
            json!({"error": {"code": "serialization_error", "message": err.to_string()}})
                .to_string()
                .into_bytes()
        });
        Self {
            status,
            reason,
            content_type: "application/json",
            body,
            request_id: None,
            headers: Vec::new(),
        }
    }

    fn empty(status: u16, reason: &'static str) -> Self {
        Self {
            status,
            reason,
            content_type: "application/json",
            body: Vec::new(),
            request_id: None,
            headers: Vec::new(),
        }
    }
}

pub fn write_response(stream: &mut TcpStream, response: HttpResponse) -> Result<()> {
    let request_id_header = response
        .request_id
        .as_deref()
        .map(|id| format!("X-Request-Id: {id}\r\n"))
        .unwrap_or_default();
    let extra_headers = response
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    write!(
        stream,
        concat!(
            "HTTP/1.1 {} {}\r\n",
            "Content-Type: {}\r\n",
            "Content-Length: {}\r\n",
            "Cache-Control: no-store\r\n",
            "Content-Security-Policy: default-src 'none'; img-src 'self' data:; style-src 'self'; style-src-attr 'none'; script-src 'self'; script-src-attr 'none'; connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'; form-action 'self'; manifest-src 'none'\r\n",
            "Cross-Origin-Opener-Policy: same-origin\r\n",
            "Permissions-Policy: camera=(), microphone=(), geolocation=()\r\n",
            "Referrer-Policy: no-referrer\r\n",
            "X-Content-Type-Options: nosniff\r\n",
            "X-Frame-Options: DENY\r\n",
            "{}",
            "{}",
            "Connection: close\r\n\r\n"
        ),
        response.status,
        response.reason,
        response.content_type,
        response.body.len(),
        request_id_header,
        extra_headers,
    )?;
    stream.write_all(&response.body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_errors_map_to_bad_request() {
        let error = anyhow::anyhow!(
            "deleted"
                .parse::<crate::domain::MemoryStatus>()
                .unwrap_err()
        );
        assert_eq!(HttpResponse::from_error(&error).status, 400);
    }

    #[test]
    fn missing_resources_and_conflicts_have_stable_statuses() {
        assert_eq!(
            HttpResponse::from_error(&anyhow::anyhow!("Memory not found: abc")).status,
            404
        );
        assert_eq!(
            HttpResponse::from_error(&anyhow::anyhow!("UNIQUE constraint failed")).status,
            409
        );
    }

    #[test]
    fn internal_errors_are_opaque_and_have_an_incident_id() {
        let response = HttpResponse::from_error(&anyhow::anyhow!(
            "sqlite failure at /private/project/.agent/memory.db"
        ));
        assert_eq!(response.status, 500);
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["error"]["message"], "internal server error");
        assert!(body["error"]["incident_id"].as_str().is_some());
        assert!(
            !String::from_utf8(response.body)
                .unwrap()
                .contains("/private/project")
        );
    }
}

use crate::domain::DomainParseError;
use anyhow::Error;
use serde_json::{Value, json};
use std::io::{Result, Write};
use std::net::TcpStream;

pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
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
        }
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

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::json(
            500,
            "Internal Server Error",
            json!({"error": {"code": "internal_error", "message": message.into()}}),
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
            || normalized.contains("looks like it may contain a secret")
        {
            return Self::bad_request(message);
        }
        if normalized.contains("not found") {
            return Self::not_found_message(message);
        }
        if normalized.contains("constraint failed")
            || normalized.contains("already exists")
            || normalized.contains("lease conflict")
        {
            return Self::conflict(message);
        }
        Self::internal_error(message)
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
        }
    }
}

pub fn write_response(stream: &mut TcpStream, response: HttpResponse) -> Result<()> {
    write!(
        stream,
        concat!(
            "HTTP/1.1 {} {}\r\n",
            "Content-Type: {}\r\n",
            "Content-Length: {}\r\n",
            "Cache-Control: no-store\r\n",
            "Content-Security-Policy: default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'\r\n",
            "Cross-Origin-Opener-Policy: same-origin\r\n",
            "Permissions-Policy: camera=(), microphone=(), geolocation=()\r\n",
            "Referrer-Policy: no-referrer\r\n",
            "X-Content-Type-Options: nosniff\r\n",
            "X-Frame-Options: DENY\r\n",
            "Connection: close\r\n\r\n"
        ),
        response.status,
        response.reason,
        response.content_type,
        response.body.len(),
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
}

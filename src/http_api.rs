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

    pub fn not_found() -> Self {
        Self::json(
            404,
            "Not Found",
            json!({"error": {"code": "not_found", "message": "endpoint not found"}}),
        )
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::json(
            500,
            "Internal Server Error",
            json!({"error": {"code": "internal_error", "message": message.into()}}),
        )
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
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.reason,
        response.content_type,
        response.body.len(),
    )?;
    stream.write_all(&response.body)
}

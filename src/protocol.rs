use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::io::BufRead;

pub const MCP_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MCP_MAX_HEADER_BYTES: usize = 8 * 1024;
pub const SYNC_MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
pub const SYNC_MAX_MEMORIES: usize = 1_000_000;

pub fn read_mcp_content_length_header(reader: &mut impl BufRead) -> Result<Option<usize>> {
    let mut length = None;
    let mut saw_header = false;
    let mut header_bytes = 0_usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            if saw_header {
                bail!("incomplete MCP frame header");
            }
            return Ok(None);
        }
        header_bytes = header_bytes.saturating_add(line.len());
        if header_bytes > MCP_MAX_HEADER_BYTES {
            bail!("MCP frame header exceeds {MCP_MAX_HEADER_BYTES} bytes");
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        saw_header = true;
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            let parsed = value.trim().parse::<usize>()?;
            if length.is_some() {
                bail!("duplicate Content-Length headers");
            }
            length = Some(parsed);
        }
    }
    length
        .map(Some)
        .ok_or_else(|| anyhow::anyhow!("missing Content-Length header"))
}

pub fn http_content_length(header: &[u8]) -> Result<usize> {
    let header = std::str::from_utf8(header).context("HTTP headers must be UTF-8")?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let request_parts = request_line.split_whitespace().collect::<Vec<_>>();
    if request_parts.len() != 3 || !matches!(request_parts[2], "HTTP/1.0" | "HTTP/1.1") {
        bail!("malformed or unsupported HTTP request line");
    }
    let mut content_length = None;
    let mut singleton_headers = HashSet::new();
    let mut host_present = false;
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            bail!("obsolete folded HTTP headers are not supported");
        }
        let Some((name, value)) = line.split_once(':') else {
            bail!("malformed HTTP header line");
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || !name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
        {
            bail!("invalid HTTP header name");
        }
        let normalized_name = name.to_ascii_lowercase();
        if matches!(
            normalized_name.as_str(),
            "host"
                | "content-length"
                | "authorization"
                | "accept"
                | "content-type"
                | "mcp-method"
                | "mcp-name"
                | "mcp-protocol-version"
                | "mcp-session-id"
                | "proxy-authorization"
                | "origin"
                | "sec-fetch-site"
                | "x-dukememory-token"
        ) && !singleton_headers.insert(normalized_name.clone())
        {
            bail!("duplicate {name} headers are not allowed");
        }
        if normalized_name == "host" {
            if value.is_empty() {
                bail!("Host header must not be empty");
            }
            host_present = true;
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            bail!("Transfer-Encoding is not supported");
        }
        if name.eq_ignore_ascii_case("content-length") {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                bail!("invalid Content-Length header");
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .context("invalid Content-Length header")?,
            );
        }
    }
    if request_parts[2] == "HTTP/1.1" && !host_present {
        bail!("HTTP/1.1 requests require exactly one Host header");
    }
    Ok(content_length.unwrap_or(0))
}

/// Parse and structurally bound an unencrypted sync/export JSON payload before
/// it reaches the schema-specific deserializer.
pub fn parse_sync_payload_json(input: &[u8]) -> Result<serde_json::Value> {
    if input.len() > SYNC_MAX_PAYLOAD_BYTES {
        bail!("sync payload exceeds {SYNC_MAX_PAYLOAD_BYTES} bytes");
    }
    let value: serde_json::Value = serde_json::from_slice(input).context("invalid sync JSON")?;
    let root = value
        .as_object()
        .context("sync payload root must be a JSON object")?;
    let export =
        if root.get("kind").and_then(serde_json::Value::as_str) == Some("dukememory.sync.bundle") {
            root.get("manifest")
                .and_then(serde_json::Value::as_object)
                .context("sync bundle must include a manifest object")?;
            root.get("export")
                .and_then(serde_json::Value::as_object)
                .context("sync bundle must include an export object")?
        } else {
            root
        };
    export
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .context("sync export must include a numeric version")?;
    let memories = export
        .get("memories")
        .and_then(serde_json::Value::as_array)
        .context("sync export must include a memories array")?;
    if memories.len() > SYNC_MAX_MEMORIES {
        bail!("sync export exceeds {SYNC_MAX_MEMORIES} memories");
    }
    Ok(value)
}

/// Validate URL syntax before any DNS resolution or network access occurs.
pub fn validate_egress_url_shape(raw_url: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw_url).context("invalid HTTP endpoint URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("egress endpoint must use http or https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("egress endpoint must not contain URL credentials");
    }
    if url.fragment().is_some() {
        bail!("egress endpoint must not contain a URL fragment");
    }
    url.host_str()
        .context("egress endpoint must include a host")?;
    url.port_or_known_default()
        .context("egress endpoint must include a valid port")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_payload_shape_accepts_bundle_and_legacy_export() {
        let legacy = br#"{"version":1,"exported_at":1,"memories":[]}"#;
        assert!(parse_sync_payload_json(legacy).is_ok());
        let bundle = br#"{"version":1,"kind":"dukememory.sync.bundle","manifest":{},"export":{"version":1,"exported_at":1,"memories":[]}}"#;
        assert!(parse_sync_payload_json(bundle).is_ok());
        assert!(parse_sync_payload_json(br#"[]"#).is_err());
        assert!(parse_sync_payload_json(br#"{"version":1}"#).is_err());
    }

    #[test]
    fn egress_url_shape_rejects_credentials_fragments_and_non_http_schemes() {
        assert!(validate_egress_url_shape("https://example.com/v1").is_ok());
        assert!(validate_egress_url_shape("file:///etc/passwd").is_err());
        assert!(validate_egress_url_shape("https://user:secret@example.com/v1").is_err());
        assert!(validate_egress_url_shape("https://example.com/v1#secret").is_err());
    }
}

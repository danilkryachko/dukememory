use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::io::BufRead;

pub const MCP_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MCP_MAX_HEADER_BYTES: usize = 8 * 1024;

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
                | "proxy-authorization"
                | "origin"
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

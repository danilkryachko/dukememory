use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

const MCP_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MCP_MAX_HEADER_BYTES: usize = 8 * 1024;

pub(super) fn serve_json_rpc<F>(content_length: bool, mut handle: F) -> Result<()>
where
    F: FnMut(Value) -> Option<Value>,
{
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    if content_length {
        serve_content_length_stream(
            &mut io::BufReader::new(stdin.lock()),
            &mut stdout,
            &mut handle,
        )
    } else {
        serve_newline_stream(&mut stdin.lock(), &mut stdout, &mut handle)
    }
}

fn serve_newline_stream(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    handle: &mut impl FnMut(Value) -> Option<Value>,
) -> Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str(&line) {
            Ok(request) => handle(request),
            Err(err) => Some(parse_error(err.to_string())),
        };
        if let Some(response) = response {
            writeln!(writer, "{response}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn serve_content_length_stream(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    handle: &mut impl FnMut(Value) -> Option<Value>,
) -> Result<()> {
    loop {
        let Some(length) = read_content_length_header(reader)? else {
            break;
        };
        if length > MCP_MAX_FRAME_BYTES {
            bail!("MCP frame exceeds {MCP_MAX_FRAME_BYTES} bytes");
        }
        let mut body = vec![0_u8; length];
        reader
            .read_exact(&mut body)
            .with_context(|| "incomplete MCP frame body")?;
        let response = match serde_json::from_slice(&body) {
            Ok(request) => handle(request),
            Err(err) => Some(parse_error(err.to_string())),
        };
        if let Some(response) = response {
            let body = serde_json::to_vec(&response)?;
            write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
            writer.write_all(&body)?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn read_content_length_header(reader: &mut impl BufRead) -> Result<Option<usize>> {
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

fn parse_error(message: String) -> Value {
    json!({
        "jsonrpc":"2.0",
        "id":Value::Null,
        "error":{"code":-32700,"message":message}
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::{Cursor, Read};

    #[test]
    fn content_length_stream_recovers_after_invalid_json() {
        let invalid = b"not-json";
        let valid = br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;
        let mut input = Vec::new();
        write!(input, "Content-Length: {}\r\n\r\n", invalid.len()).unwrap();
        input.extend_from_slice(invalid);
        write!(input, "Content-Length: {}\r\n\r\n", valid.len()).unwrap();
        input.extend_from_slice(valid);
        let mut output = Vec::new();
        serve_content_length_stream(&mut Cursor::new(input), &mut output, &mut |request| {
            Some(json!({"jsonrpc":"2.0","id":request["id"],"result":{}}))
        })
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\"code\":-32700"));
        assert!(output.contains("\"id\":2"));
    }

    #[test]
    fn content_length_headers_are_bounded_and_unambiguous() {
        let conflicting = b"Content-Length: 1\r\ncontent-length: 2\r\n\r\n{}";
        assert!(
            read_content_length_header(&mut Cursor::new(conflicting))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let duplicate = b"Content-Length: 1\r\ncontent-length: 1\r\n\r\n{}";
        assert!(
            read_content_length_header(&mut Cursor::new(duplicate))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        let oversized = format!("X-Fill: {}\r\n\r\n", "x".repeat(MCP_MAX_HEADER_BYTES));
        assert!(
            read_content_length_header(&mut Cursor::new(oversized))
                .unwrap_err()
                .to_string()
                .contains("exceeds")
        );
    }

    #[test]
    fn content_length_round_trips_varied_payload_sizes() {
        let mut input = Vec::new();
        for (id, size) in [0_usize, 1, 127, 1_024, 8_192].into_iter().enumerate() {
            let request = json!({
                "jsonrpc":"2.0",
                "id":id,
                "method":"echo",
                "params":{"payload":"x".repeat(size)}
            });
            let body = serde_json::to_vec(&request).unwrap();
            write!(
                input,
                "content-length: {}\r\nX-Test: yes\r\n\r\n",
                body.len()
            )
            .unwrap();
            input.extend_from_slice(&body);
        }
        let mut output = Vec::new();
        serve_content_length_stream(&mut Cursor::new(input), &mut output, &mut |request| {
            Some(json!({
                "jsonrpc":"2.0",
                "id":request["id"],
                "result":request["params"].clone()
            }))
        })
        .unwrap();

        let mut output = Cursor::new(output);
        for (id, size) in [0_usize, 1, 127, 1_024, 8_192].into_iter().enumerate() {
            let length = read_content_length_header(&mut output).unwrap().unwrap();
            let mut body = vec![0_u8; length];
            output.read_exact(&mut body).unwrap();
            let response: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(response["id"], id);
            assert_eq!(response["result"]["payload"].as_str().unwrap().len(), size);
        }
        assert!(read_content_length_header(&mut output).unwrap().is_none());
    }

    #[test]
    fn oversized_content_length_is_rejected_before_body_allocation() {
        let input = format!("Content-Length: {}\r\n\r\n", MCP_MAX_FRAME_BYTES + 1);
        let error =
            serve_content_length_stream(&mut Cursor::new(input), &mut Vec::new(), &mut |_| {
                Some(json!({}))
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("frame exceeds"));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn arbitrary_mcp_frame_headers_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..20_000)) {
            let _ = read_content_length_header(&mut Cursor::new(bytes));
        }

        #[test]
        fn content_length_value_round_trips_with_bounded_whitespace(
            length in 0usize..=MCP_MAX_FRAME_BYTES,
            leading in 0usize..8,
            trailing in 0usize..8,
        ) {
            let header = format!(
                "Content-Length:{}{}{}\r\n\r\n",
                " ".repeat(leading),
                length,
                " ".repeat(trailing),
            );
            prop_assert_eq!(
                read_content_length_header(&mut Cursor::new(header)).unwrap(),
                Some(length)
            );
        }

        #[test]
        fn duplicate_content_lengths_are_always_rejected(first in 0usize..10_000, second in 0usize..10_000) {
            let header = format!(
                "Content-Length: {first}\r\ncontent-length: {second}\r\n\r\n"
            );
            prop_assert!(read_content_length_header(&mut Cursor::new(header)).is_err());
        }
    }
}

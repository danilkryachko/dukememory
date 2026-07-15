use assert_cmd::Command;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command as StdCommand, Stdio};
use tempfile::tempdir;

fn command(db: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("dukememory"));
    command
        .arg("--db")
        .arg(db)
        .env("DUKEMEMORY_EMBED_PROVIDER", "mock")
        .env("DUKEMEMORY_GEN_PROVIDER", "mock");
    command
}

fn http_get(db: &std::path::Path, path: &str) -> Value {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin!("dukememory"))
        .arg("--db")
        .arg(db)
        .arg("serve-http")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("0")
        .arg("--once")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut url = String::new();
    reader.read_line(&mut url).unwrap();
    let port = url
        .trim()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn mcp_call(db: &std::path::Path, name: &str) -> Value {
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin!("dukememory"))
        .arg("--db")
        .arg(db)
        .arg("serve-mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": name, "arguments": {"max_chars": 8000}}
        })
    )
    .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn advanced_eval_is_consistent_across_cli_mcp_and_http() {
    let directory = tempdir().unwrap();
    let db = directory.path().join("memory.db");

    let cli_output = command(&db)
        .arg("eval")
        .arg("advanced")
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(cli["version"], 1);
    assert_eq!(cli["status"], "unconfigured");
    assert_eq!(cli["surfaces"]["mcp"], "memory_advanced_eval");

    let http = http_get(&db, "/advanced-eval");
    assert_eq!(http["advanced_eval"]["version"], 1);
    assert_eq!(http["advanced_eval"]["status"], cli["status"]);

    let mcp = mcp_call(&db, "memory_advanced_eval");
    assert_eq!(mcp["result"]["isError"], false);
    let text = mcp["result"]["content"][0]["text"].as_str().unwrap();
    let report: Value = serde_json::from_str(text).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["status"], cli["status"]);
}

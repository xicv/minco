use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};

fn repository_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

#[tokio::test]
async fn check_reports_a_non_serving_read_only_stdio_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .args(["mcp", "--check", "--json"])
        .current_dir(repository_root())
        .output()
        .await
        .expect("run MCP check");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("MCP check JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["transport"], "stdio");
    assert_eq!(report["listening_sockets"], 0);
    assert_eq!(
        report["tool_names"].as_array().expect("tool names").len(),
        6
    );
}

#[tokio::test]
async fn serving_requires_an_explicit_root() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .arg("mcp")
        .current_dir(repository_root())
        .output()
        .await
        .expect("run MCP without root");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("requires an explicit canonical project root via --root")
    );
    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn child_process_stdio_discovers_current_protocol_and_lists_only_read_only_tools() {
    let root = repository_root();
    let mut child = Command::new(env!("CARGO_BIN_EXE_cargo-minco"))
        .arg("--root")
        .arg(&root)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn MCP child process");
    let mut stdin = child.stdin.take().expect("MCP stdin");
    let stdout = child.stdout.take().expect("MCP stdout");
    let mut lines = BufReader::new(stdout).lines();

    stdin
        .write_all(
            format!(
                "{}\n",
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "server/discover",
                    "params": {
                        "_meta": {
                            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                            "io.modelcontextprotocol/clientInfo": {
                                "name": "minco-test",
                                "version": "1"
                            },
                            "io.modelcontextprotocol/clientCapabilities": {}
                        }
                    }
                })
            )
            .as_bytes(),
        )
        .await
        .expect("write server/discover");
    let discovered = timeout(Duration::from_secs(10), lines.next_line())
        .await
        .expect("server/discover timeout")
        .expect("read server/discover response")
        .expect("server/discover response line");
    let discovered: Value = serde_json::from_str(&discovered).expect("server/discover JSON");
    assert_eq!(discovered["id"], 1);
    assert_eq!(discovered["result"]["resultType"], "complete");
    assert!(
        discovered["result"]["supportedVersions"]
            .as_array()
            .expect("supported protocol versions")
            .contains(&json!("2026-07-28"))
    );

    let list_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientInfo": {
                    "name": "minco-test",
                    "version": "1"
                },
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    });
    stdin
        .write_all(format!("{list_request}\n").as_bytes())
        .await
        .expect("write tools/list");
    let listed = timeout(Duration::from_secs(10), lines.next_line())
        .await
        .expect("tools/list timeout")
        .expect("read tools/list response")
        .expect("tools/list response line");
    assert!(!listed.contains(root.to_string_lossy().as_ref()));
    let listed: Value = serde_json::from_str(&listed).expect("tools/list JSON");
    let tools = listed["result"]["tools"].as_array().expect("listed tools");
    assert_eq!(tools.len(), 6);
    assert!(
        tools
            .iter()
            .all(|tool| tool["annotations"]["readOnlyHint"] == true)
    );

    drop(stdin);
    let status = timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("MCP exit timeout")
        .expect("wait for MCP child");
    assert!(status.success());
}

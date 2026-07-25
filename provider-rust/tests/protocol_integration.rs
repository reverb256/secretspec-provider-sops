//! Integration test for the NDJSON plugin protocol (cachix/secretspec#98).
//!
//! Spawns the compiled binary (`CARGO_BIN_EXE_secretspec-provider-sops`)
//! as a subprocess, pipes in a synthetic Hello + Get request, and asserts
//! the JSON response on stdout matches the spec shape.
//!
//! References:
//!   - docs/spec/provider-protocol.md (vendored from cachix/secretspec#98)
//!   - provider-rust/src/protocol.rs (request/response serde types)

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

fn bin_path() -> &'static str {
    // NDJSON protocol provider lives in a separate bin target since
    // the user-facing CLI binary (`secretspec-provider-sops`) gained
    // clap subcommands for get/doctor/--help; see Cargo.toml's two
    // `[[bin]]` entries.
    env!("CARGO_BIN_EXE_secretspec-provider-sops-protocol")
}

#[tokio::test]
async fn hello_advertises_v1_capabilities() {
    let mut child = Command::new(bin_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provider binary");

    let mut stdin = child.stdin.take().expect("stdin pipe");
    let stdout = child.stdout.take().expect("stdout pipe");
    let mut lines = BufReader::new(stdout).lines();

    // Hello: per spec example shape verbatim.
    let hello = r#"{"op":"hello","protocol_version":1,"uri":"sops://./secrets.yaml","config_file":"/tmp/secretspec.toml","context":{}}"#;
    stdin
        .write_all(hello.as_bytes())
        .await
        .expect("write hello");
    stdin.write_all(b"\n").await.expect("newline after hello");
    stdin.flush().await.expect("flush hello");

    let resp_line = lines
        .next_line()
        .await
        .expect("read hello resp")
        .expect("non-eof");
    let v: serde_json::Value = serde_json::from_str(&resp_line).expect("parse hello resp");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
    assert_eq!(v["protocol_version"], serde_json::Value::from(1u32));
    assert_eq!(v["name"], serde_json::Value::from("sops"));

    let caps = v["capabilities"].as_array().expect("capabilities is array");
    let cap_set: std::collections::HashSet<&str> = caps.iter().filter_map(|c| c.as_str()).collect();
    for required in ["get", "set", "batch_get", "reflect", "bye"] {
        assert!(
            cap_set.contains(required),
            "missing capability {required}; got {caps:?}"
        );
    }

    // Close stdin cleanly so child exits.
    drop(stdin);
    let _ = child.wait().await;
}

#[tokio::test]
async fn get_returns_null_on_missing_key_per_spec() {
    let mut child = Command::new(bin_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provider binary");

    let mut stdin = child.stdin.take().expect("stdin pipe");
    let stdout = child.stdout.take().expect("stdout pipe");
    let mut lines = BufReader::new(stdout).lines();

    // Skip past the optional Hello by sending one first.
    let hello = r#"{"op":"hello","protocol_version":1,"uri":"sops://./secrets.yaml","config_file":"/tmp/secretspec.toml","context":{}}"#;
    stdin
        .write_all(hello.as_bytes())
        .await
        .expect("hello write");
    stdin.write_all(b"\n").await.expect("hello nl");
    let _ = lines
        .next_line()
        .await
        .expect("hello resp")
        .expect("non-eof");

    // Get request for a key that we expect Phase 1 to NOT resolve.
    let get_req = r#"{"op":"get","project":"homelab","key":"nvidia_api_key","profile":"default"}"#;
    stdin
        .write_all(get_req.as_bytes())
        .await
        .expect("get write");
    stdin.write_all(b"\n").await.expect("get nl");

    let resp_line = lines.next_line().await.expect("get resp").expect("non-eof");
    let v: serde_json::Value = serde_json::from_str(&resp_line).expect("parse get resp");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
    // CRITICAL: per cachix/secretspec#98 section 5.1, missing key MUST
    // return {"value": null} (not an error).
    assert!(
        v["value"].is_null(),
        "expected value:null for missing key; got: {v}"
    );

    drop(stdin);
    let _ = child.wait().await;
}

#[tokio::test]
async fn bye_returns_ok() {
    let mut child = Command::new(bin_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn provider binary");

    let mut stdin = child.stdin.take().expect("stdin pipe");
    let stdout = child.stdout.take().expect("stdout pipe");
    let mut lines = BufReader::new(stdout).lines();

    stdin
        .write_all(b"{\"op\":\"bye\"}\n")
        .await
        .expect("bye write");
    let resp_line = lines.next_line().await.expect("bye resp").expect("non-eof");
    let v: serde_json::Value = serde_json::from_str(&resp_line).expect("bye parse");
    assert_eq!(v["ok"], serde_json::Value::Bool(true));

    drop(stdin);
    let _ = child.wait().await;
}

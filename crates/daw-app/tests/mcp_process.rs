//! `daw-mcp` processes as an AI client starts them: windowless stdio and the `--attach` bridge to a running app.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: failing with a clear message is the check itself"
)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;

use daw_bootstrap::build_session;
use daw_mcp::{HttpEndpoint, McpServer};
use serde_json::{Value, json};

/// Starts `daw-mcp` the way an AI client does: stdin and stdout are pipes, stderr is not needed.
fn spawn(args: &[&str]) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_daw-mcp"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the process starts");
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    (child, stdin, BufReader::new(stdout))
}

/// Sends a message; reads the reply only if one is expected (notifications get none).
fn send(stdin: &mut ChildStdin, reader: &mut BufReader<ChildStdout>, request: &Value, reply: bool) -> Value {
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
    if !reply {
        return Value::Null;
    }
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {line:?}"))
}

/// A real process: stdout carries only the protocol (one reply per line), everything else goes to stderr.
#[test]
fn the_real_process_speaks_line_delimited_json_over_stdio() {
    let state = std::env::temp_dir().join(format!("daw-mcp-test-{}", std::process::id()));
    let (mut child, mut stdin, mut reader) = spawn(&["--state-dir", state.to_str().unwrap()]);
    let mut send = |request: Value, reply: bool| send(&mut stdin, &mut reader, &request, reply);

    let init = send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }), true);
    assert_eq!(init["result"]["serverInfo"]["name"], "daw");
    send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), false);
    let list = send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }), true);
    assert!(list["result"]["tools"].as_array().unwrap().len() >= 100);
    let track = send(
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "tracks_add", "arguments": { "name": "Synth", "kind": "kick", "defaultPitch": 36 } } }),
        true,
    );
    assert_eq!(track["id"], 3);
    assert_eq!(track["result"]["isError"], false);

    drop(send);
    drop(stdin);
    assert!(child.wait().unwrap().success(), "the process exits when the client closes its input");
    std::fs::remove_dir_all(state).ok();
}

/// The bridge to a "running app": the server inside this test plays the app window, the `daw-mcp --attach` process plays
/// the AI client. The AI's edits are visible in the same session, unlike the separate session of the stdio process.
#[test]
fn the_attach_bridge_edits_the_session_of_the_running_application() {
    let session = build_session(false);
    let controls = session.controls.clone();
    let endpoint = HttpEndpoint::start(Arc::new(McpServer::new(controls.clone())), 0).expect("the server starts on a free port");

    let (mut child, mut stdin, mut reader) = spawn(&["--attach", "--port", &endpoint.port().to_string()]);
    let mut ask = |request: Value| send(&mut stdin, &mut reader, &request, true);

    assert_eq!(ask(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }))["result"]["serverInfo"]["name"], "daw");
    let reply = ask(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "drums_build", "arguments": { "slots": [{ "pattern": "rock_basic", "repeat": 2 }] } } }),
    );
    assert_eq!(reply["result"]["isError"], false, "{reply}");

    // The app window (this session) sees what the AI did.
    assert_eq!(controls.tracks.tracks().len(), 3, "kick, snare, hat");
    assert_eq!(controls.clips.clips().len(), 3);
    assert!(controls.history.can_undo(), "the AI's edit is undone from the window with a normal undo");

    drop(ask);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn the_bridge_explains_when_the_application_is_not_running() {
    // No busy port: start and close a server right away to get a port that is surely free.
    let port = {
        let session = build_session(false);
        HttpEndpoint::start(Arc::new(McpServer::new(session.controls)), 0).unwrap().port()
    };
    let (mut child, mut stdin, mut reader) = spawn(&["--attach", "--port", &port.to_string()]);
    let reply = send(&mut stdin, &mut reader, &json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/list" }), true);
    drop(stdin);
    assert_eq!(reply["id"], 5);
    assert!(reply["error"]["message"].as_str().unwrap().contains("does not answer"), "{reply}");
    assert!(child.wait().unwrap().success());
}

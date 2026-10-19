//! The MCP protocol on a real session and a real process: handshake, tool list, calls, errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: failing with a clear message is the check itself"
)]

use daw_bootstrap::build_session;
use daw_mcp::McpServer;
use serde_json::{Value, json};

fn server() -> McpServer {
    McpServer::new(build_session(false).controls)
}

fn ask(server: &McpServer, id: u32, method: &str, params: Value) -> Value {
    server.handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).expect("a request must get a reply")
}

fn call(server: &McpServer, tool: &str, arguments: Value) -> (bool, String) {
    let reply = ask(server, 1, "tools/call", json!({ "name": tool, "arguments": arguments }));
    let result = &reply["result"];
    (result["isError"].as_bool().unwrap(), result["content"][0]["text"].as_str().unwrap().to_string())
}

#[test]
fn the_handshake_names_the_server_and_explains_the_ground_rules() {
    let server = server();
    let reply = ask(
        &server,
        7,
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }),
    );
    assert_eq!(reply["id"], 7);
    assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(reply["result"]["serverInfo"]["name"], "daw");
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    let text = reply["result"]["instructions"].as_str().unwrap();
    assert!(text.contains("TICKS") && text.contains("drums_build") && text.contains("chords_build"), "{text}");
    assert!(text.contains("Start with `project_summary`"), "the AI orients by the summary, not the whole project: {text}");
    assert!(text.contains("CODE: message"), "the AI knows how errors look: {text}");
    assert!(text.contains("analysis_check") && text.contains("clips_addNoteMany"), "the AI knows how to check its work: {text}");
    assert!(text.contains("analysis_balance"), "the AI knows how to balance the volumes: {text}");
    assert!(text.contains("history_checkpoint") && text.contains("history_squash"), "a request is one undo step: {text}");
    assert!(text.contains("automation_addDrawnClip") && text.contains("`params`"), "the AI knows the one-call shortcuts: {text}");
    // The server replaces an unknown protocol version with its own instead of refusing.
    let other = ask(&server, 8, "initialize", json!({ "protocolVersion": "1999-01-01" }));
    assert_eq!(other["result"]["protocolVersion"], "2025-03-26");
    // Notifications get no reply.
    assert!(server.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
    assert_eq!(ask(&server, 9, "ping", Value::Null)["result"], json!({}));
}

#[test]
fn every_rpc_method_is_a_tool_with_a_description_and_an_object_schema() {
    let server = server();
    let reply = ask(&server, 1, "tools/list", Value::Null);
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), server.tool_count());
    assert!(tools.len() >= 100);
    for tool in tools {
        assert!(tool["description"].as_str().unwrap().len() > 12, "{tool}");
        assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
    }
    let build = tools.iter().find(|t| t["name"] == "drums_build").unwrap();
    assert!(build["inputSchema"]["properties"]["slots"].is_object(), "the AI sees the drums_build params: {build}");
    for name in [
        "preferences_get",
        "preferences_set",
        "plugins_list",
        "analysis_levels",
        "analysis_balance",
        "analysis_check",
        "clips_addNoteMany",
        "project_clear",
        "project_summary",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name), "no tool {name}");
    }
    let set = tools.iter().find(|t| t["name"] == "preferences_set").unwrap();
    assert!(set["inputSchema"]["properties"]["preferences"].is_object(), "the AI sees the preferences form: {set}");
}

#[test]
fn tools_change_the_project_and_report_results_as_text() {
    let server = server();
    let (error, text) = call(&server, "tracks_add", json!({ "name": "Kick", "kind": "kick", "defaultPitch": 36 }));
    assert!(!error, "{text}");
    let track: u32 = text.parse().expect("the method returned a track id");

    // A method without params and a method without a result.
    let (error, status) = call(&server, "project_status", json!({}));
    assert!(!error && status.contains('{'), "{status}");
    let (error, text) = call(&server, "tempo_set", json!({ "bpm": 140 }));
    assert!(!error && text == "done", "{text}");

    // A drum part on the track the AI has just created.
    let (error, text) = call(
        &server,
        "drums_build",
        json!({ "slots": [{ "pattern": "rock_basic", "repeat": 2 }], "lanes": [{ "lane": "kick", "target": { "type": "track", "id": track } }] }),
    );
    assert!(!error, "{text}");
    let report: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(report["bars"], 2);
    assert_eq!(report["createdTracks"], json!(["Snare", "Hi-hat"]));

    let (_, project) = call(&server, "project_get", json!({}));
    let project: Value = serde_json::from_str(&project).unwrap();
    assert_eq!(project["tempoMap"]["tempos"][0]["bpm"], 140.0);
    assert_eq!(project["tracks"].as_array().unwrap().len(), 3);
    assert_eq!(call(&server, "history_undo", json!({})).1, "true");
}

#[test]
fn a_failing_method_is_a_readable_tool_error_not_a_protocol_error() {
    let server = server();
    let (error, text) = call(&server, "drums_build", json!({ "slots": [{ "pattern": "no_such_pattern" }] }));
    assert!(error && text.contains("no_such_pattern"), "{text}");
    let (error, text) = call(&server, "tracks_add", json!({ "name": 5 }));
    assert!(error && text.starts_with("RP-101: ") && text.contains("tracks.add"), "{text}");
    // An unknown tool and an unknown protocol method are protocol errors.
    let reply = ask(&server, 3, "tools/call", json!({ "name": "no_such_tool", "arguments": {} }));
    assert_eq!(reply["error"]["code"], -32602);
    assert_eq!(ask(&server, 4, "resources/list", Value::Null)["error"]["code"], -32601);
    assert!(server.handle_line("{ this is not json").unwrap().contains("-32700"));
    assert!(server.handle_line("   ").is_none());
}

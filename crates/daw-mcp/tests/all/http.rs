//! The MCP HTTP server inside the app: working requests and protection against requests from the wrong places.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: failing with a clear message is the check itself"
)]

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;

use daw_bootstrap::build_session;
use daw_mcp::{HttpEndpoint, McpServer, relay};
use serde_json::{Value, json};

fn endpoint() -> (HttpEndpoint, daw_contracts::Controls) {
    let controls = build_session(false).controls;
    (HttpEndpoint::start(Arc::new(McpServer::new(controls.clone())), 0).unwrap(), controls)
}

/// A raw HTTP request; returns (status, body).
fn raw(port: u16, request: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    (head.split_whitespace().nth(1).unwrap().parse().unwrap(), body.to_string())
}

fn post(port: u16, extra_headers: &str, body: &Value) -> (u16, String) {
    let body = body.to_string();
    raw(
        port,
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

#[test]
fn a_json_rpc_post_is_answered_and_edits_the_shared_session() {
    let (endpoint, controls) = endpoint();
    let (status, body) = post(
        endpoint.port(),
        "",
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "tracks_add", "arguments": { "name": "Kick", "kind": "kick", "defaultPitch": 36 } } }),
    );
    assert_eq!(status, 200);
    let reply: Value = serde_json::from_str(&body).unwrap();
    assert_eq!((reply["id"].clone(), reply["result"]["isError"].clone()), (json!(1), json!(false)), "{body}");
    assert_eq!(controls.tracks.tracks().len(), 1, "this is the very session the window sees");

    // A notification: 202 without a body. A localhost Host line is fine too.
    let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string();
    let (status, body) = raw(
        endpoint.port(),
        &format!(
            "POST /mcp HTTP/1.1\r\nHost: localhost:{}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\n\r\n{note}",
            endpoint.port(),
            note.len()
        ),
    );
    assert_eq!((status, body.as_str()), (202, ""));
}

#[test]
fn requests_that_look_like_a_browser_or_a_foreign_host_are_refused_and_change_nothing() {
    let (endpoint, controls) = endpoint();
    let port = endpoint.port();
    let add = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "tracks_add", "arguments": { "name": "K", "kind": "kick", "defaultPitch": 36 } } });

    // A browser page always adds Origin: this way a website cannot control the program.
    assert_eq!(post(port, "Origin: https://evil.example\r\n", &add).0, 403);
    // A foreign host name (DNS rebinding).
    let body = add.to_string();
    let spoofed = format!(
        "POST /mcp HTTP/1.1\r\nHost: evil.example:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(raw(port, &spoofed).0, 403);
    // Not JSON (this is how "simple" browser requests skip the preflight check).
    let plain =
        format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    assert_eq!(raw(port, &plain).0, 415);
    assert!(controls.tracks.tracks().is_empty(), "no refused request changed anything");

    assert_eq!(raw(port, &format!("GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")).0, 405);
    assert_eq!(
        raw(
            port,
            &format!("POST /other HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}")
        )
        .0,
        404
    );
}

#[test]
fn the_server_survives_garbage_and_stops_when_dropped() {
    let (endpoint, _controls) = endpoint();
    let port = endpoint.port();
    assert!(matches!(raw(port, "this is not http\r\n\r\n").0, 400 | 403), "garbage is refused");
    assert_eq!(post(port, "", &json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" })).0, 200, "the server answers after garbage");
    drop(endpoint);
    assert!(TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_err(), "the port is released");
}

#[test]
fn a_busy_port_is_an_error_and_not_a_crash() {
    let (first, _controls) = endpoint();
    let session = build_session(false);
    assert!(HttpEndpoint::start(Arc::new(McpServer::new(session.controls)), first.port()).is_err());
}

#[test]
fn the_relay_forwards_lines_and_keeps_notifications_silent() {
    let (endpoint, controls) = endpoint();
    let input = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
        String::new(),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "tracks_add", "arguments": { "name": "Kick", "kind": "kick", "defaultPitch": 36 } } }).to_string(),
    ]
    .join("\n");
    let mut output = Vec::new();
    relay(endpoint.port(), input.as_bytes(), &mut output).unwrap();
    let replies: Vec<Value> = String::from_utf8(output).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies.len(), 2, "a notification and an empty line get no replies");
    assert_eq!((replies[0]["id"].clone(), replies[1]["id"].clone()), (json!(1), json!(2)));
    assert_eq!(controls.tracks.tracks().len(), 1);
}

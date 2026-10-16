//! The DAW's MCP server: turns the `daw-rpc` method table into tools for AI clients (Claude Code, Claude Desktop…).
//! Every RPC method is one tool (the method name with `_` instead of `.`: `drums_build`); the description and the
//! params schema come from the `RpcTable::describe` catalog, so a new RPC method reaches the AI without changes here.
//!
//! The protocol is JSON-RPC 2.0. Transports: stdio, one message per line (the windowless process, `daw-mcp` in
//! `daw-app`), and HTTP on `127.0.0.1` inside the running app (`http.rs`: the AI works in the same project that is open
//! in the window); `bridge.rs` is a bridge for clients that only speak stdio. The server knows nothing about sound or
//! the project: it only forwards calls to `RpcTable`. The protocol logic is the pure function `McpServer::handle`,
//! which the tests check.

mod bridge;
mod envelope;
mod http;

pub use bridge::relay;
pub use http::{HttpEndpoint, port_from_env};

use std::sync::Arc;

use daw_contracts::codes::rpc as codes;
use daw_contracts::{AppLog, ContractError, Controls, PluginStateFlush};
use daw_rpc::{MethodInfo, RpcTable};
use serde_json::{Value, json};

use envelope::{INVALID_PARAMS, METHOD_NOT_FOUND, PARSE_ERROR, failure, success};

/// The protocol version the server offers when the client names an unknown one.
const LATEST_PROTOCOL: &str = "2025-03-26";
/// Protocol versions the server is compatible with (the tool set is the same in all of them).
const SUPPORTED_PROTOCOLS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

const INSTRUCTIONS: &str = "This is a DAW (a program for making music). The tools are its methods. Start with `project_summary` (a short \
overview: sections, tracks with instruments, mixer, effects and where they play) and `catalog_get` (which instruments and \
effects can be created); `project_get` is the whole project with every note and parameter: read it only for the details you \
need. Time is always in TICKS: 960 per quarter note, a 4/4 bar = 3840, a sixteenth = 240. Tracks, clips, notes, graph nodes and \
samples are numeric ids returned by the create methods, `project_summary` and `project_get`. Notes \
live inside note clips; their time is counted from the clip start. A clip with notes is created in one call: `clips_addNote` with `notes` \
(and `loopTicks` if the pattern repeats); all clips of a section or part at once — `clips_addNoteMany` (one undo step). To start over — \
`project_clear`. Drums are easiest to build from ready patterns: `patterns_list`, then `drums_build` (groove, fill, groove…); listen \
before building with `drums_preview`. Chords — `chords_build`: a strum (`chords_strums`: \"four\", \"six\", \"eight\") \
and one or two chord symbols per bar ([[\"C\"], [\"G\"], [\"Am\", \"F\"]]) on a track or a new \"guitar\", \"electric_guitar\", \
\"ukulele\" or \"bass_guitar\" track; listen first with `chords_preview`. Mark song sections with `markers_add` (\"Intro\" at 0, \"Drop\" at bar 33 = 122880), group tracks \
into folders with `folders_create`. Song tempo and meter: `tempo_set` (the starting tempo), tempo changes and smooth ramps along the song — \
`tempo_setChanges` (`ramp`), meter changes — `tempo_setSignatureChanges`; with meter changes bars have different lengths. Every project \
edit can be undone with `history_undo` (a compound edit as a whole). Make each request of the user ONE undo step: call \
`history_checkpoint` before your first edit and, when the request is done, `history_squash` with that mark and a short label \
(\"AI: drum part\"); to throw away a failed attempt — squash, then `history_undo`. Errors come as `CODE: message` (for example \
`TR-001: Track 5 not found`): the code names the situation, the message explains it; fix the request and try again. \
You cannot hear the sound, so check your work with numbers: after building or changing a track, call `analysis_check` and fix the \
findings (silent or quiet tracks, master overload, mute/solo, params at their minimum) BEFORE reporting that it is done; levels of \
tracks, buses and the master per bar — `analysis_levels`.";

/// An MCP tool: name, description and input schema; `method` is the RPC method it calls.
struct Tool {
    name: String,
    method: String,
    description: String,
    input_schema: Value,
    /// A method without params: the client's arguments (`{}`) are not passed on, because the method expects `null`.
    no_params: bool,
}

/// An MCP tool input schema must be an object. A method without params gets an empty object; an optional param
/// (`anyOf: [object, null]`) gets that object.
fn input_schema(schema: &Value) -> (Value, bool) {
    let empty = json!({ "type": "object", "properties": {} });
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => (schema.clone(), false),
        Some("null") => (empty, true),
        _ => {
            let object = schema
                .get("anyOf")
                .and_then(Value::as_array)
                .and_then(|options| options.iter().find(|o| o.get("type").and_then(Value::as_str) == Some("object")));
            (object.cloned().unwrap_or(empty), false)
        }
    }
}

fn tool_of(info: &MethodInfo) -> Tool {
    let (input_schema, no_params) = input_schema(&info.params);
    Tool { name: info.name.replace('.', "_"), method: info.name.clone(), description: info.description.clone(), input_schema, no_params }
}

pub struct McpServer {
    table: RpcTable,
    controls: Controls,
    tools: Vec<Tool>,
}

impl McpServer {
    pub fn new(controls: Controls) -> Self {
        let table = RpcTable::new();
        let tools = table.describe().iter().map(tool_of).collect();
        Self { table, controls, tools }
    }

    /// The app has plugin windows: saving and rendering tools let them hand their state to the project first.
    pub fn with_plugin_windows(mut self, windows: Arc<dyn PluginStateFlush>) -> Self {
        self.table.set_plugin_windows(windows);
        self
    }

    /// The app log: transports write their failures here.
    pub(crate) fn log(&self) -> &dyn AppLog {
        &*self.controls.log
    }

    /// How many tools the client sees (one per RPC method, including `rpc.describe`).
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Handles one client message. `None` means no reply is needed (a notification).
    pub fn handle(&self, message: &Value) -> Option<Value> {
        let method = message.get("method").and_then(Value::as_str)?;
        // No `id` means a notification (e.g. `notifications/initialized`): those must not be answered.
        let id = message.get("id")?;
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => success(id, Self::initialize(&params)),
            "ping" => success(id, json!({})),
            "tools/list" => success(id, self.list()),
            "tools/call" => match self.call(&params) {
                Ok(result) => success(id, result),
                Err(error) => failure(id, INVALID_PARAMS, &coded(&error)),
            },
            other => failure(id, METHOD_NOT_FOUND, &coded(&codes::UNSUPPORTED_PROTOCOL_METHOD.with("method", other))),
        })
    }

    /// Parses an input line: the reply to it or a parse error. Empty lines and notifications get no reply.
    pub fn handle_line(&self, line: &str) -> Option<String> {
        if line.trim().is_empty() {
            return None;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle(&message)?,
            Err(error) => failure(&Value::Null, PARSE_ERROR, &coded(&codes::BAD_JSON.with("details", error))),
        };
        Some(reply.to_string())
    }

    fn initialize(params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(LATEST_PROTOCOL);
        let version = if SUPPORTED_PROTOCOLS.contains(&asked) { asked } else { LATEST_PROTOCOL };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "daw", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn list(&self) -> Value {
        let tools: Vec<Value> =
            self.tools.iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.input_schema })).collect();
        json!({ "tools": tools })
    }

    /// Calls a tool. An unknown tool is a protocol error; a failure of the method itself is a normal result with
    /// `isError`, so the AI sees the code and the message and fixes the request.
    fn call(&self, params: &Value) -> Result<Value, ContractError> {
        let name = params.get("name").and_then(Value::as_str).ok_or_else(|| codes::NO_TOOL_NAME.err())?;
        let tool = self.tools.iter().find(|t| t.name == name).ok_or_else(|| codes::UNKNOWN_TOOL.with("tool", name))?;
        let arguments = if tool.no_params { Value::Null } else { params.get("arguments").cloned().unwrap_or_else(|| json!({})) };
        Ok(match self.table.call(&self.controls, &tool.method, arguments) {
            Ok(value) => json!({ "content": [{ "type": "text", "text": value_text(&value) }], "isError": false }),
            Err(error) => json!({ "content": [{ "type": "text", "text": coded(&error) }], "isError": true }),
        })
    }
}

/// A method result as text for the AI: `null` means "done", anything else is JSON.
fn value_text(value: &Value) -> String {
    if value.is_null() { "done".to_string() } else { value.to_string() }
}

/// An error as an AI client reads it: code, then the English message (`TR-001: Track 5 not found`).
fn coded(error: &ContractError) -> String {
    format!("{}: {error}", error.code().code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_schemas_are_always_objects() {
        let object = json!({ "type": "object", "properties": { "id": { "type": "integer" } } });
        assert_eq!(input_schema(&object), (object.clone(), false));
        assert_eq!(input_schema(&json!({ "type": "null" })), (json!({ "type": "object", "properties": {} }), true));
        let optional = json!({ "anyOf": [object.clone(), { "type": "null" }] });
        assert_eq!(input_schema(&optional), (object, false));
        assert_eq!(input_schema(&Value::Null).0["type"], "object");
    }

    #[test]
    fn tool_names_are_valid_identifiers_for_clients() {
        let table = RpcTable::new();
        let names: Vec<String> = table.describe().iter().map(|m| tool_of(m).name).collect();
        assert!(names.iter().all(|n| n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')), "{names:?}");
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "tool names are unique");
    }
}

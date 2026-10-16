//! The AI server inside the app: MCP over HTTP on `127.0.0.1` (see `daw-mcp`). An AI client (Claude Code etc.) connects to
//! THIS app and works in the same project that is open in the window; edits are visible at once because they go through the same
//! contracts, and the window refreshes on the project change event.
//!
//! Starts after the start project is open (as interface calls do). Turn off: `DAW_MCP=off`;
//! another port: `DAW_MCP_PORT`. A busy port (e.g. a second copy is running) does not break the app — it is logged.

use std::sync::Arc;

use daw_contracts::{Controls, LogEntry, PluginStateFlush};
use daw_mcp::{HttpEndpoint, McpServer, port_from_env};

/// Starts the server unless it is turned off; returns a handle — while it is alive, the server listens. Its saving and
/// rendering tools let the open plugin windows hand their state to the project first, as the window's calls do.
pub fn start(controls: &Controls, windows: Arc<dyn PluginStateFlush>) -> Option<HttpEndpoint> {
    if std::env::var("DAW_MCP").is_ok_and(|value| value.eq_ignore_ascii_case("off")) {
        return None;
    }
    let port = port_from_env();
    match HttpEndpoint::start(Arc::new(McpServer::new(controls.clone()).with_plugin_windows(windows)), port) {
        // Success is not logged: this is an error log. The address is known in advance: `http://127.0.0.1:<port>/mcp`.
        Ok(endpoint) => Some(endpoint),
        Err(error) => {
            controls.log.record(LogEntry::warn("ai", format!("the AI server did not start on port {port}: {error}")));
            None
        }
    }
}

#!/bin/bash
# Builds the headless DAW MCP server (target/release/daw-mcp) — needed for the "headless" and "bridge to the app" modes.
# The server INSIDE the windowed app needs no separate build: it starts together with the app.
#
# Connecting to Claude Code:
#   app is running, the AI works in your project:  claude mcp add --transport http daw http://127.0.0.1:47321/mcp
#   bridge for clients that can only run processes: claude mcp add daw -- "$(pwd)/target/release/daw-mcp" --attach
#   headless, its own session:                      claude mcp add daw-headless -- "$(pwd)/target/release/daw-mcp"
set -euo pipefail
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd "$(dirname "$0")/.."
cargo build --release -p daw-app --bin daw-mcp
echo "Done: $(pwd)/target/release/daw-mcp"

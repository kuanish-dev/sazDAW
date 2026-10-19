# `daw-mcp`

**MCP protocol for AI** (a layer over RPC): `McpServer` (every RPC method is a tool), `HttpEndpoint` (HTTP on `127.0.0.1` inside the running app), `relay` (a stdio → app bridge). The `daw-mcp` binary (windowless) lives in `daw-app`.

Depends on: contracts, rpc.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [mix-check](../../docs/features/mix-check.md).

The RPC self-description and the MCP server are described together in "AI interface" ([crates/daw-rpc/CLAUDE.md](../daw-rpc/CLAUDE.md)).

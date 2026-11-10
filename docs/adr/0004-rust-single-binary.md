# Rust, shipped as one binary

The Broker, the MCP server and the `watch` command ship as one Rust binary built on `rmcp`, the official Tier 1 Rust MCP SDK (the same library Codex CLI uses for MCP). A long-running local process is best as one native file with no runtime to install or break, and the maintainer already writes Rust. Most MCP servers ship through `npx`, so a reader may expect TypeScript; we accept the cost of building release binaries per platform instead.

## Consequences

- Distribution starts with GitHub release binaries and `cargo install`; an `npx` wrapper that downloads the binary is added only if users ask.

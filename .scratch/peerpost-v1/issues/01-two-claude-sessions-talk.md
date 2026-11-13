# 01: Two Claude Sessions exchange Peer messages

**What to build:** A Human adds peerpost to Claude Code and starts two Claude Sessions. One Session calls `who`, sees the other, `send`s it a Peer message, and the other gets it with `read`, stamped with the sender's Sender name. The Broker starts by itself on first use. This is the tracer bullet through the whole binary: MCP server over stdio (rmcp), Broker, Message log, registration. See spec.md and ADRs 0001-0004.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] One Rust binary (rmcp) with an MCP stdio server exposing `who`, `send`, `read`; Cargo project, licence fields `MIT OR Apache-2.0`
- [x] A Claude Session (detected by `CLAUDECODE=1`) registers as `RELAY_NAME` if set, else `claude-<4 hex>`; every tool result states the caller's own Sender name
- [x] The first client that finds no Broker starts it in the background and waits until it answers
- [x] `send(recipient, text)` to a live Session returns a message id; the Broker stamps the Sender name, never taken from tool arguments
- [x] `read` returns only the caller's unread Peer messages, oldest first, each with Sender name, and marks them read; a third Session never sees them
- [x] Broker socket and Message log are private to the OS user (0600 files, 0700 directory) in a per-user runtime/data directory; no installation-specific path
- [x] Manual check recorded in the ticket: `RELAY_NAME` set at `claude` start reaches Claude Code's MCP server
- [x] Tests at Seam 1 (built binary in a temp directory, driven as an MCP client); README section: what peerpost is, trust boundary (ADR 0003)

## Comments

**2026-11-13, implementer (ticket 01):** Built on branch `peerpost-v1-01`.

- One Rust binary on rmcp 3.10.0 (`Cargo.toml`, licence `MIT OR Apache-2.0`). With no argument it runs the stdio MCP server, which offers `who`, `send` and `read`. `peerpost broker` runs the Broker.
- The MCP server registers a Claude Session (`CLAUDECODE=1`) at start, as `RELAY_NAME` or `claude-<4 hex>`. Every tool result is JSON with `you` (the caller's own Sender name), errors included.
- The first client that finds no Broker starts it detached and polls until it answers. The Broker speaks line-delimited JSON over `broker.sock` in the runtime directory. It stamps the Sender name from the connection's registration and ignores any sender-like tool arguments.
- The Message log is an append-only `messages.jsonl` in the data directory. It is replayed on start, and `read` returns unread messages oldest first and marks them read. The directories are 0700 and the files 0600. Defaults come from XDG or HOME; `PEERPOST_RUNTIME_DIR` and `PEERPOST_DATA_DIR` override them.
- There are 9 Seam 1 tests in `tests/claude_sessions.rs`, built on the shared harness `tests/common/mod.rs`. The README covers what peerpost is, where it keeps files, and the trust boundary.
- Manual check, done 2026-11-13 with Claude Code 2.1.323. A probe stdio server logged only `RELAY_NAME` and `CLAUDECODE`. It ran via `RELAY_NAME=probe-two claude -p --strict-mcp-config --mcp-config <tmp>/mcp.json ...` in a temp directory, with `CLAUDECODE` removed from the parent environment. It logged `RELAY_NAME=probe-two CLAUDECODE=1`. So `RELAY_NAME` reaches the MCP server, and Claude Code sets `CLAUDECODE=1` itself.
- Left for later tickets:
  - Concurrent start can race (lock file, ticket 04).
  - The client does not reconnect (ticket 04).
  - An unknown Recipient is not refused yet (ticket 02).
  - The Message log has no cap (ticket 03).

# 01: Two Claude Sessions exchange Peer messages

**What to build:** A Human adds peerpost to Claude Code and starts two Claude Sessions. One Session calls `who`, sees the other, `send`s it a Peer message, and the other gets it with `read`, stamped with the sender's Sender name. The Broker starts by itself on first use. This is the tracer bullet through the whole binary: MCP server over stdio (rmcp), Broker, Message log, registration. See spec.md and ADRs 0001-0004.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] One Rust binary (rmcp) with an MCP stdio server exposing `who`, `send`, `read`; Cargo project, licence fields `MIT OR Apache-2.0`
- [ ] A Claude Session (detected by `CLAUDECODE=1`) registers as `RELAY_NAME` if set, else `claude-<4 hex>`; every tool result states the caller's own Sender name
- [ ] The first client that finds no Broker starts it in the background and waits until it answers
- [ ] `send(recipient, text)` to a live Session returns a message id; the Broker stamps the Sender name, never taken from tool arguments
- [ ] `read` returns only the caller's unread Peer messages, oldest first, each with Sender name, and marks them read; a third Session never sees them
- [ ] Broker socket and Message log are private to the OS user (0600 files, 0700 directory) in a per-user runtime/data directory; no installation-specific path
- [ ] Manual check recorded in the ticket: `RELAY_NAME` set at `claude` start reaches Claude Code's MCP server
- [ ] Tests at Seam 1 (built binary in a temp directory, driven as an MCP client); README section: what peerpost is, trust boundary (ADR 0003)

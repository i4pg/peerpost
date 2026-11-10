# 06: Codex Sessions named from the per-call thread id

**What to build:** Codex chats, even when many share one MCP server process, each become their own Session, named `codex-` plus a short part of the `_meta.threadId` Codex puts on every tool call (ADR 0006). A call without it is refused clearly.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] The first relay tool call carrying a new `_meta.threadId` registers a Codex Session `codex-<first 4 hex>`, lengthened until unique
- [ ] Two threads over one MCP connection are two Sessions; each reads only its own Peer messages
- [ ] A non-Claude call without `_meta.threadId` gets a clear error and nothing is guessed
- [ ] A compatibility test pins the field to the supported Codex versions and fails if it disappears
- [ ] README labels the field undocumented and lists the tested Codex versions; shipped AGENTS.md snippet tells Codex to call `who` at start
- [ ] Tests at Seam 1 with an MCP client that sets `_meta.threadId`

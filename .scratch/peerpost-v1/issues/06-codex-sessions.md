# 06: Codex Sessions named from the per-call thread id

**What to build:** Codex chats, even when many share one MCP server process, each become their own Session, named `codex-` plus a short part of the `_meta.threadId` Codex puts on every tool call (ADR 0006). A call without it is refused clearly.

**Blocked by:** 01

**Status:** resolved

- [x] The first relay tool call carrying a new `_meta.threadId` registers a Codex Session `codex-<first 4 hex>`, lengthened until unique
- [x] Two threads over one MCP connection are two Sessions; each reads only its own Peer messages
- [x] A non-Claude call without `_meta.threadId` gets a clear error and nothing is guessed
- [x] A compatibility test pins the field to the supported Codex versions and fails if it disappears
- [x] README labels the field undocumented and lists the tested Codex versions; shipped AGENTS.md snippet tells Codex to call `who` at start
- [x] Tests at Seam 1 with an MCP client that sets `_meta.threadId`

## Comments

**Implementer (ticket 06, branch `peerpost-v1-06`):** Built.

- `src/mcp.rs`: a non-Claude MCP server identifies the caller per tool call from `context.meta["threadId"]` (`CodexThread::from_meta`). It registers the thread once as `codex-<first 4 hex>`, lengthening one hex digit at a time while the Broker says the name is taken (`protocol::NAME_TAKEN`). The thread-to-name map is per process. A missing, non-string or non-hex `threadId` is refused with an error naming the field, and nothing is registered. `RELAY_NAME` may not start with `codex-`.
- Broker: `Register` now carries `codex_thread: Option<String>`, kept per live Session (`LiveSession.codex_thread`) for ticket 07's `codex queue --thread`. A thread already live on another connection is refused, so each thread has one Session.
- Compatibility: `tests/fixtures/codex/<version>.json` records the `tools/call` shape for each supported Codex version, with its source location. `tests/codex_compat.rs` replays each fixture through a real server. It also checks that the README's `Tested Codex CLI versions:` line matches the fixtures. An `#[ignore]`d test checks a Codex source checkout (`PEERPOST_CODEX_SOURCE=<checkout> cargo test --test codex_compat -- --ignored`). That check was **not run**: there is no Codex checkout offline. The 0.167.0 fixture's source citation (`codex-rs/core/src/mcp_tool_call.rs` 572-577) comes from the 2026-11-10 research, not a fresh read.
- Real Codex thread ids are UUIDv7 (time first), so all current threads share their first 4 hex digits (as of 2026-11 they start `01a1`). Following the spec, only the first Codex Session gets a 4-digit name; the rest become `codex-01a1b`, `codex-01a1bd` and so on, typically 8 digits or more. Using the random tail instead would keep names short. That is a spec change for the owner to decide.
- README: a Codex section that labels the field undocumented and lists the tested versions. Shipped `snippets/codex-AGENTS.md` tells Codex to call `who` at start.
- Tests at Seam 1: `tests/codex_sessions.rs` (7) and `tests/codex_compat.rs`.

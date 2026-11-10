# Read Codex's per-call thread id, though only its source defines it

Narrows ADR 0001 for Codex only. Codex does not tell an MCP server which Session started it: the server's environment carries no thread id, a plain `codex` start spawns servers from one shared background daemon (so an environment variable set at `codex` start never reaches them), and a server speaking the newest MCP protocol may be shared by every Codex chat. The one way to tell Codex Sessions apart, and to know which thread to Nudge, is the `threadId` Codex puts in the `_meta` of every tool call, defined in Codex's Apache-2.0 source but not in its docs. We use it: Codex is open source, so ADR 0001's terms reason does not apply, and the remaining risk (OpenAI may change it without notice) is handled openly.

## Consequences

- A field defined only in Codex's open-source code may be used when the README labels it undocumented, the supported Codex versions are listed, and a test fails if the field disappears. If a call arrives without it, peerpost refuses with a clear error and never guesses.
- A Codex Session's Sender name is `codex-` plus a short part of its thread id; `RELAY_NAME` names only Claude Sessions for now. Readable Codex names come later.
- Nothing from Claude Code's internals is used; ADR 0001 stands in full for Claude.
- We ask OpenAI to document the field.

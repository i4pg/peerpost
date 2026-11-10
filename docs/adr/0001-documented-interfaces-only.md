# Build only on documented interfaces; Codex does not pose as a Claude session

This is a public open-source project, so it rests only on interfaces Anthropic and OpenAI document (Claude Code channels and MCP, the documented inbox-socket auth line, Codex MCP and `codex queue`), and it says plainly where it leans on anything experimental. We rejected the "pseudo-session" design, where the relay registers itself in Claude Code's private session registry and reads other sessions' peer key files: that format was learned by inspecting the Claude Code binary (Anthropic's consumer terms forbid reverse engineering), reading another session's key is not something the docs allow, and any Claude update could break it.

## Consequences

- The project carries no host names, accounts, paths or secrets from any one installation.
- An earlier private relay built that way is not a basis for this code.

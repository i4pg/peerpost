# The Codex Nudge is a fixed line sent with `codex queue`

Codex has no documented way to push into a running session except `codex queue`, whose text the model sees as if the Human typed it, with no sender field. We still use it to wake an idle Codex Session, but only with one fixed line that carries no Peer message text and asks for nothing except calling the relay's `read` tool; the messages themselves arrive as tool output. The line's "from the relay, not the human" wording and the shipped AGENTS.md snippet are guidance to the model, not an enforced label, and the README says so.

## Considered Options

- **Never push into Codex**: safest, but two agents could not hold a conversation without the Human.
- **App-server `additionalContext` of kind `untrusted`**: in Codex's schema, but its effect on the model is undocumented (breaks ADR 0001).

## Consequences

- The Nudge waits for the Human when the Codex Session's last turn was interrupted, or when Codex was started with its own private server (`-c`, `--profile`, `--oss`, `--no-daemon`); the README lists both.

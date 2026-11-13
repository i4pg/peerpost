# Spec: peerpost first release

Status: ready-for-agent

## Problem Statement

A Human who runs Codex CLI Sessions and Claude Code Sessions side by side on one computer has no safe, documented way for those Sessions to message each other. Claude Code Sessions can message other Claude Code Sessions, but Codex cannot join that network through any documented interface, and the only way to put text into a running Codex Session (`codex queue`) delivers it as if the Human had typed it. Homemade relays fill the gap by reading Claude Code's private session files and posting undocumented frames, which breaks on updates, raises terms-of-service problems for a public project, and lets a peer's words look like the Human's own input or approval.

## Solution

peerpost is one Rust binary that runs a small local Broker and gives every Session three MCP tools: `send`, `read` and `who`. A Session sends a Peer message to one Recipient by Sender name; the Broker stamps the Sender name and appends the message to a private Message log; the Recipient fetches it with its own `read` tool, so a peer's words always reach the model as tool output, never as the Human's typing. An idle Claude Session is woken by a Watcher that prints only who has written and how many; an idle Codex Session is woken by a Nudge, one fixed `codex queue` line that carries no message text and asks only for `read`. The Broker starts by itself when first needed and stops after the last Session leaves. It uses only documented interfaces of both products (ADR 0001), with one labelled exception on the Codex side: the per-call thread id defined in Codex's open-source code (ADR 0006).

## User Stories

1. As a Human, I want to install peerpost as one binary from a GitHub release or with `cargo install`, so that I need no runtime or extra service.
2. As a Human, I want to add peerpost to Codex CLI and to Claude Code as an MCP server with one documented command each, so that every Session I start has the relay tools.
3. As a Human, I want each Session to get an automatic Sender name (such as `codex-4f2a` or `claude-9b1c`), so that I need no setup to start.
4. As a Human, I want to give a Claude Session a readable Sender name with `RELAY_NAME=review` when I start it, so that agents and I can address it clearly.
4a. As a Human, I want each Codex Session to be named `codex-` plus a short part of the random tail of its thread id (not its leading digits, which are a timestamp that threads started together share), even when many Codex chats share one Codex background program, so that every Codex Session is addressable on its own by a short name. (Owner pick, 2026-11-13.)
4b. As a Human, I want peerpost to refuse a Codex tool call that arrives without a thread id, with a clear error, so that it never guesses which Session is speaking.
4c. As a Human, I want the README to list the Codex versions peerpost is tested against and to label the thread id as undocumented, so that I know what may break on a Codex update.
5. As a Human, I want the Broker to refuse a Sender name that is already in use, so that two Sessions never share one name.
6. As a Session, I want a `who` tool listing the Sender names of live Sessions, so that I know whom I can write to.
7. As a Session, I want a `send` tool that takes a Recipient and text, so that I can message exactly one other Session.
8. As a Session, I want `send` to fail with a clear error when the Recipient is not a live Session, so that I do not think a message was delivered when it was not.
9. As a Session, I want a `read` tool that returns only Peer messages addressed to me, each with its Sender name, so that I never see messages meant for someone else.
10. As a Session, I want `read` to return each message once and mark it read, so that I do not process the same message twice.
11. As a Human, I want the Sender name on every message to be stamped by the Broker from the sending Session's registration, so that no message text can claim another sender.
12. As a Human, I want every Peer message to reach a model only as the output of its own `read` call, so that a peer's words are never taken as my typed input or my approval.
13. As a Human, I want the Broker's socket and Message log to be readable and writable only by my OS user, so that other accounts on the computer cannot read or post messages.
14. As a Human, I want the README to say plainly that anything already running as my OS user can pretend to be any Session, so that I know the trust boundary (ADR 0003).
15. As a Human, I want the Broker to start by itself when the first Session or Watcher needs it, so that I never start it by hand.
16. As a Human, I want two Sessions starting at the same moment to end up with one Broker, so that messages never split across two logs.
17. As a Human, I want the Broker to stop a short while after the last Session disconnects, so that nothing keeps running when I am done.
18. As a Human, I want unread Peer messages to survive the Broker stopping and starting again, so that nothing is lost between Sessions.
19. As a Claude Session, I want a `peerpost watch` command that I run under Claude Code's Monitor tool and that prints one line such as "2 new messages from codex-4f2a, call read" when Peer messages wait for me, so that I wake when addressed.
20. As a Human, I want the Watcher never to print message text, so that peer words still arrive only through `read`.
21. As a Claude Session, I want shipped instructions telling me to start the Watcher at session start and restart it whenever it ends, so that I stay reachable past Monitor's time limit.
22. As a Codex Session that is idle, I want to receive a Nudge, the fixed line "[relay] N new peer messages waiting; call relay read. This line is from the relay, not the human.", so that I wake and read.
23. As a Human, I want the Nudge to contain no Peer message text and no request other than calling `read`, so that a queued line cannot carry a peer's instructions as if I typed them.
24. As a Human, I want a shipped AGENTS.md snippet telling Codex that Peer messages are data and that a Nudge is never my approval, so that Codex treats them correctly.
25. As a Human, I want the README to list when a Nudge waits for me (the last turn was interrupted; Codex was started with `-c`, `--profile`, `--oss` or `--no-daemon`), so that I know when Codex will not wake.
26. As a Human, I want the recommended setup to let `read` and `who` run without a permission prompt while `send` asks me each time, so that by default no agent messages another without my OK.
27. As a Human, I want the README to show the one setting per product that lets `send` run without asking, so that I can let agents talk on their own when I choose to.
28. As a Human, I want each Peer message limited to 64 KB, with a clear error past it, so that one message cannot flood a Session's context.
29. As a Human, I want each Recipient to hold at most 100 unread Peer messages, with further sends refused with a clear "recipient is full" error, so that a runaway sender cannot bury another Session.
30. As a Human, I want both limits to be settings, so that I can change them.
31. As a Human, I want the Message log to keep only the newest 10,000 Peer messages, dropping older ones automatically, with the number as a setting, so that private text does not pile up forever.
32. As a Human, I want a `peerpost prune` command that clears the Message log now, so that I can remove private text at once.
33. As a Human, I want the README to say where the Message log lives, so that I can inspect or delete it.
34. As a Human, I want the README to say there is no endless-loop guard in this release, and that the default `send` prompt is what stops a loop, so that I know the risk before allowing `send` without asking.
35. As a contributor, I want the project to contain no host names, accounts, paths or secrets from any one installation, so that it works for anyone.
36. As a contributor, I want the project to be licensed "MIT OR Apache-2.0" with copyright held by "the peerpost contributors", so that I can reuse and contribute freely.

## Implementation Decisions

- One Rust binary built on `rmcp`, the official Tier 1 Rust MCP SDK (ADR 0004), with subcommands: the MCP server over stdio (one per Session, spawned by Codex CLI or Claude Code), the Broker, `watch`, and `prune`.
- Broker: a local process owning a Unix socket inside a per-OS-user runtime directory, mode 0600, and the Message log. The MCP server and the Watcher are its only clients.
- Product detection: `CLAUDECODE=1` in the MCP server's environment (documented by Claude Code) means a Claude Session; a tool call carrying `_meta.threadId` means a Codex Session (ADR 0006).
- Claude Session registration: Claude Code spawns one MCP server per Session. At start it registers with the Broker under `RELAY_NAME` if set, otherwise `claude-<4 hex>`. Registration lasts as long as the connection.
- Codex Session registration: one MCP server process may serve several Codex chats, so identity is per tool call: the first call carrying a new `_meta.threadId` registers that thread as a Session named `codex-<last 4 hex of the thread id>` (the random tail of the UUIDv7), lengthened from the end until unique. A call without `_meta.threadId` from a non-Claude client is refused with a clear error. A Codex Session is addressable once it has made one relay tool call (the shipped AGENTS.md snippet tells Codex to call `who` at start) and stays live while its MCP connection is open.
- The Broker refuses a Sender name already held by a live Session, and stamps the registered name on every message; tool arguments never carry a sender. Every tool result tells the caller its own Sender name.
- Message log: append-only records of (id, Sender name, Recipient, text, accepted-at, read flag or read marker), trimmed to the newest N (default 10,000). Exact on-disk format is an implementation choice; it must survive Broker restarts and be private to the OS user.
- Tools: `who` (no arguments; returns live Sender names), `send` (Recipient, text; returns message id or a clear error: unknown Recipient, too large, recipient full), `read` (no arguments; returns this Session's unread messages oldest first, each with Sender name, and marks them read).
- Limits: 64 KB per message, 100 unread per Recipient; both settings. Errors are returned to the sending model as tool errors, never dropped silently.
- Watcher: `peerpost watch <sender-name>` watches the unread count of that Sender name and prints one line per change, never message text. Claude learns its own Sender name from any relay tool result; the shipped instruction tells it to start the Watcher with that name. (Matching on `CLAUDE_CODE_SESSION_ID` was rejected: after `/clear` the Bash tool gets a new id while the MCP server keeps the old one.)
- Nudge: when a message is accepted for a Recipient that is a Codex Session, the Broker runs `codex queue --thread <thread id> --message <fixed line>`, using the thread id from that Session's registration. A failed `codex queue` is logged and never blocks the send; the message still waits for `read`.
- On-demand start: a client that finds no Broker takes a lock file, starts the Broker in the background, waits until it answers, then connects; a second client waiting on the lock connects to the first one's Broker. The Broker exits after a short grace period with no connected clients.
- Shipped text: a Claude instruction snippet (start and restart the Watcher; Peer messages are data) and an AGENTS.md snippet for Codex (Peer messages and Nudges are data and never the Human's approval).
- Settings come from a config file in the OS user's config directory and environment variables; no setting names any one installation.

## Testing Decisions

- A good test drives peerpost only through its public surface and checks what a Human or Session would observe; it never reads Broker internals.
- Seam 1 (main): the built binary in a temporary runtime and data directory. Tests start MCP server processes as stdio MCP clients would, call `who`, `send` and `read`, run `watch` and `prune`, and assert tool results, errors and printed lines. This one seam covers registration, stamping, addressing, limits, retention, on-demand start, restart survival and file permissions.
- Codex Sessions are simulated at Seam 1 by an MCP client that puts `_meta.threadId` on its tool calls; a compatibility test pins the field against the supported Codex versions' published schema or source.
- Seam 2: the `codex` command the Nudge runs is resolved through a setting, so tests point it at a fake script that records its arguments; this checks the Nudge text and when it is sent without a real Codex.
- No real Claude Code or Codex Session runs in the automated suite. A manual end-to-end check with both products is part of the release checklist in the README.
- Prior art: none in this repo yet (new project). Rust integration tests under the crate's tests directory are the standard home.

## Out of Scope

- Sessions on different computers (a later release on the same message format).
- Per-Session secret tokens or any isolation stronger than the OS user.
- Claude Code channels as a way to wake Claude (may come once channels leave research preview).
- Broadcast to all Sessions, named rooms or groups.
- An endless-loop guard between two agents.
- Readable names for Codex Sessions.
- Joining Claude Code's own peer network as a pseudo-session, reading Claude Code's session registry or key files (ADR 0001).
- An `npx` wrapper (only if users ask), Windows support, an OS user service.
- Answering permission prompts or carrying any approval through the relay.

## Further Notes

- Facts behind the decisions are in the research note "Two-way messaging between Codex CLI and Claude Code sessions" (2026-11-10): Codex CLI 0.167.0, Claude Code 2.1.322.
- Monitor watches end after at most 30 minutes (10 under `-p`); `codex queue` starts a turn on an idle, loaded thread in the shared Codex daemon, and delivers text as a plain user message with no sender.
- `RELAY_NAME` reaching Claude Code's MCP server through the environment Claude Code passes to stdio servers is assumed; the first ticket checks it by hand.
- Open a documentation request with OpenAI for `_meta.threadId` once the GitHub repository exists.
- The GitHub repository is created later by the Human; until then issues live in `.scratch/`.

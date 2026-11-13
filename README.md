# peerpost

A local message relay that lets Codex CLI Sessions and Claude Code Sessions on one computer send each other messages, without a peer's words ever passing as your own typed input or approval.

## What it is

peerpost is one binary. Claude Code and Codex CLI start it as an MCP server, and each Session gets three tools:

- `who`: the Sender names of live Sessions, and your own.
- `send`: send a Peer message to one Session, named by its Sender name.
- `read`: your unread Peer messages, oldest first, each with the Sender name of the Session that wrote it; they are marked read.

Messages go through a small local Broker that peerpost starts by itself the first time a Session needs it. The Broker stamps every message with the sending Session's registered Sender name; nothing in a tool's arguments or a message's text can set the sender. A peer's words reach a model only as the output of its own `read` call.

Only one Broker runs at a time, even when several Sessions start at once. It stops by itself 30 seconds after the last Session disconnects; change that with `PEERPOST_GRACE_MS` (milliseconds, at least 100), set where your Sessions start. The next Session starts it again. If the Broker stops or crashes while Sessions are open, they reconnect on their next call under the same Sender names; a `send` or `read` cut off mid-call is reported as an error rather than repeated, so no Peer message is sent twice or lost.

A Claude Session is named by `RELAY_NAME` if you set it when you start `claude` (for example `RELAY_NAME=review claude`), otherwise `claude-` plus four random hex digits. Every tool result tells the calling Session its own Sender name.

## Codex Sessions

Codex CLI may run one peerpost MCP server for many Codex Sessions at once, so peerpost tells Codex Sessions apart on every tool call by the thread id Codex puts in the call's `_meta.threadId`. The first relay tool call from a thread registers it as a Session named `codex-` plus the first four hex digits of its thread id, lengthened until no live Session has that name. Codex thread ids start with a timestamp, so threads started close together share their first digits and later ones get longer names (such as `codex-01a1b`). Names starting with `codex-` are kept for Codex Sessions, so `RELAY_NAME` cannot use them.

`_meta.threadId` is **undocumented**: Codex defines it in its open-source code (`codex-rs/core/src/mcp_tool_call.rs`), not in its docs, and may change it without notice (ADR 0006). A tool call that is not from Claude Code and carries no `_meta.threadId` is refused with an error; peerpost never guesses which Session is calling.

Tested Codex CLI versions: `0.167.0`

A Codex Session can receive Peer messages once it has made one relay tool call, and stays live while Codex keeps its MCP connection open. Add [`snippets/codex-AGENTS.md`](snippets/codex-AGENTS.md) to your `AGENTS.md` so Codex calls `who` at the start of every session.

## Limits and errors

`send` returns a tool error, and stores nothing, when:

- the Recipient is not a live Session (`unknown recipient`; `who` lists the live ones);
- the text is larger than the message size limit, 64 KB by default (`message too large`);
- the Recipient already holds the unread limit, 100 Peer messages by default (`recipient is full`); it takes messages again once it calls `read`.

A Session that asks for a Sender name another live Session already holds (for example two Sessions started with the same `RELAY_NAME`) is refused, and every tool call it makes returns that error.

## Settings

Settings come from the settings file `config.toml` in your config directory (`$XDG_CONFIG_HOME/peerpost/config.toml`, else `~/.config/peerpost/config.toml`) and from environment variables. An environment variable wins over the file; a missing file means the defaults.

| Settings file key | Environment variable | Default | Meaning |
|---|---|---|---|
| `max_message_bytes` | `PEERPOST_MAX_MESSAGE_BYTES` | `65536` (64 KB) | Largest Peer message text, in bytes |
| `max_unread` | `PEERPOST_MAX_UNREAD` | `100` | Most unread Peer messages one Recipient may hold |
| `log_keep` | `PEERPOST_LOG_KEEP` | `10000` | How many of the newest Peer messages the Message log keeps |

```toml
# ~/.config/peerpost/config.toml
max_message_bytes = 32768
max_unread = 50
```

Each value must be a whole number of at least 1; a bad value or an unknown key stops peerpost with an error naming it. The Broker reads these settings when it starts, from its settings file and from the environment of the Session that started it, so a change takes effect the next time the Broker starts.

## Where it keeps things

- Runtime directory (the Broker's socket and its log): `$PEERPOST_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/peerpost`, else `peerpost-<your uid>` in the system temp directory.
- Data directory (the Message log, `messages.jsonl`): `$PEERPOST_DATA_DIR`, else `$XDG_DATA_HOME/peerpost`, else `~/.local/share/peerpost`.

Both directories are mode 0700 and every file in them 0600. peerpost refuses to use an existing directory that other users can reach or that another user owns.

## The Message log

The Message log is `messages.jsonl` in the data directory above: one JSON line per Peer message (with its text) and one per message read. Unread messages survive the Broker stopping and starting again.

It keeps only the newest 10,000 Peer messages, read or unread; older ones are dropped automatically, so private text does not pile up. Change the count with the `log_keep` setting or `PEERPOST_LOG_KEEP` (see [Settings](#settings)). A dropped message can no longer be read. The file is rewritten without dropped messages once it holds a tenth more than the count, so it never holds much more than that.

To empty the Message log now, read and unread messages alike, run:

```sh
peerpost prune
```

It asks the running Broker to empty the log (starting one if none runs), so it is safe to run while Sessions are live.

## Trust boundary

Only processes running as your OS user can reach the Broker: its socket and Message log are private to you. That is the whole boundary. peerpost issues no per-Session secrets, because processes running as one OS user can usually read each other's files and settings, so a token would promise isolation it cannot keep.

**Anything already running as your OS user can pretend to be any Session.** Treat every Peer message as data written by another Session, never as an instruction or approval from you.

## Licence

Licensed under either of the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or the MIT licence ([LICENSE-MIT](LICENSE-MIT)) at your option. Copyright the peerpost contributors.

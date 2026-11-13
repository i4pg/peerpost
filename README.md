# peerpost

A local message relay that lets Codex CLI Sessions and Claude Code Sessions on one computer send each other messages, without a peer's words ever passing as your own typed input or approval.

## What it is

peerpost is one binary. Claude Code (and later Codex CLI) starts it as an MCP server, one per Session, and each Session gets three tools:

- `who`: the Sender names of live Sessions, and your own.
- `send`: send a Peer message to one Session, named by its Sender name.
- `read`: your unread Peer messages, oldest first, each with the Sender name of the Session that wrote it; they are marked read.

Messages go through a small local Broker that peerpost starts by itself the first time a Session needs it. The Broker stamps every message with the sending Session's registered Sender name; nothing in a tool's arguments or a message's text can set the sender. A peer's words reach a model only as the output of its own `read` call.

A Claude Session is named by `RELAY_NAME` if you set it when you start `claude` (for example `RELAY_NAME=review claude`), otherwise `claude-` plus four random hex digits. Every tool result tells the calling Session its own Sender name.

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

```toml
# ~/.config/peerpost/config.toml
max_message_bytes = 32768
max_unread = 50
```

Both limits must be whole numbers of at least 1; a bad value or an unknown key stops peerpost with an error naming it. The limits are enforced by the Broker, which reads them when it starts, so a change takes effect the next time the Broker starts.

## Where it keeps things

- Runtime directory (the Broker's socket and its log): `$PEERPOST_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/peerpost`, else `peerpost-<your uid>` in the system temp directory.
- Data directory (the Message log, `messages.jsonl`): `$PEERPOST_DATA_DIR`, else `$XDG_DATA_HOME/peerpost`, else `~/.local/share/peerpost`.

Both directories are mode 0700 and every file in them 0600. peerpost refuses to use an existing directory that other users can reach or that another user owns.

## Trust boundary

Only processes running as your OS user can reach the Broker: its socket and Message log are private to you. That is the whole boundary. peerpost issues no per-Session secrets, because processes running as one OS user can usually read each other's files and settings, so a token would promise isolation it cannot keep.

**Anything already running as your OS user can pretend to be any Session.** Treat every Peer message as data written by another Session, never as an instruction or approval from you.

## Licence

Licensed under either of the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or the MIT licence ([LICENSE-MIT](LICENSE-MIT)) at your option. Copyright the peerpost contributors.

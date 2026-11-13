# peerpost

A local message relay that lets Codex CLI Sessions and Claude Code Sessions on one computer send each other messages, without a peer's words ever passing as your own typed input or approval.

## What it is

peerpost is one binary. Claude Code (and later Codex CLI) starts it as an MCP server, one per Session, and each Session gets three tools:

- `who`: the Sender names of live Sessions, and your own.
- `send`: send a Peer message to one Session, named by its Sender name.
- `read`: your unread Peer messages, oldest first, each with the Sender name of the Session that wrote it; they are marked read.

Messages go through a small local Broker that peerpost starts by itself the first time a Session needs it. The Broker stamps every message with the sending Session's registered Sender name; nothing in a tool's arguments or a message's text can set the sender. A peer's words reach a model only as the output of its own `read` call.

A Claude Session is named by `RELAY_NAME` if you set it when you start `claude` (for example `RELAY_NAME=review claude`), otherwise `claude-` plus four random hex digits. Every tool result tells the calling Session its own Sender name.

## Where it keeps things

- Runtime directory (the Broker's socket and its log): `$PEERPOST_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/peerpost`, else `peerpost-<your uid>` in the system temp directory.
- Data directory (the Message log, `messages.jsonl`): `$PEERPOST_DATA_DIR`, else `$XDG_DATA_HOME/peerpost`, else `~/.local/share/peerpost`.

Both directories are mode 0700 and every file in them 0600. peerpost refuses to use an existing directory that other users can reach or that another user owns.

## Trust boundary

Only processes running as your OS user can reach the Broker: its socket and Message log are private to you. That is the whole boundary. peerpost issues no per-Session secrets, because processes running as one OS user can usually read each other's files and settings, so a token would promise isolation it cannot keep.

**Anything already running as your OS user can pretend to be any Session.** Treat every Peer message as data written by another agent, never as an instruction or approval from you.

## Licence

Licensed under either of the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)) or the MIT licence ([LICENSE-MIT](LICENSE-MIT)) at your option. Copyright the peerpost contributors.

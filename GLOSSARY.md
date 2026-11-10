# Codex–Claude relay

Lets Codex CLI sessions and Claude Code sessions on one computer exchange messages both ways, without a peer's words ever passing as the human's own input or approval.

## Language

**Human**:
The person running the sessions; the only one whose typed input or approval counts.
_Avoid_: user (ambiguous with the OS user), owner, operator

**Session**:
One running conversation with one agent (a Claude Code session or a Codex thread).
_Avoid_: agent, instance, chat

**Peer message**:
Text one session sends to another through the relay; always data, never an instruction from the Human.
_Avoid_: prompt, user message

**Broker**:
The one local process that receives, stores and hands out Peer messages.
_Avoid_: server, hub, daemon

**Message log**:
The append-only record of every Peer message the Broker has accepted.
_Avoid_: inbox, queue, history

**Nudge**:
A wake-up signal to an idle Session that carries no message text, only that new Peer messages are waiting.
_Avoid_: notification, ping, poke

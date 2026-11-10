# A local broker with a message log; agents send and read through their own tools

Messages between Codex and Claude sessions on one computer go through one small local broker that appends every message to a log. Each agent sends with a `send` tool and reads with a `read` tool, so a peer's words always reach the model as the output of a tool it called itself, never as the human's typed input. An idle session is woken only by a nudge that carries no message text. This is the smallest design that keeps the safety rule while using no preview features or special start flags.

## Considered Options

- **Claude Code channels** (push into Claude, sender stamped by Claude Code): rejected for now because a self-built channel needs `--dangerously-load-development-channels`, a confirmation dialog at every start, and does not run headless or on Bedrock/Vertex. It may come back later as an extra way to wake Claude once channels leave research preview.
- **A plain shared file both sides poll**: rejected because anyone who can write the file can claim to be any sender, and polling costs tokens.

## Consequences

- The first release works only between sessions on the same computer; reaching other computers is a later release on the same message format.

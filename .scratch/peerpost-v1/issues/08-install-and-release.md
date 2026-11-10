# 08: Install, setup and release

**What to build:** A stranger can install peerpost from a GitHub release file (Linux, macOS) or `cargo install`, add it to Codex CLI and Claude Code with one documented command each, and follow the recommended permission setup.

**Blocked by:** 02, 03, 04, 05, 07

**Status:** ready-for-agent

- [ ] A release build produces binaries for Linux and macOS (x86_64 and arm64)
- [ ] `cargo install` works from the repo
- [ ] README: one setup command each for Codex CLI and Claude Code; recommended permissions (`read`, `who` without asking, `send` asks) and the one setting per product to let `send` run without asking
- [ ] README states: the OS-user trust boundary, no endless-loop guard in this release, no Windows, same computer only
- [ ] README release checklist includes a hands-on check with a real Claude Code and a real Codex Session talking both ways
- [ ] The full test suite passes at the release commit

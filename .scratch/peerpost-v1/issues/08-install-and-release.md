# 08: Install, setup and release

**What to build:** A stranger can install peerpost from a GitHub release file (Linux, macOS) or `cargo install`, add it to Codex CLI and Claude Code with one documented command each, and follow the recommended permission setup.

**Blocked by:** 02, 03, 04, 05, 07

**Status:** resolved

- [x] A release build produces binaries for Linux and macOS (x86_64 and arm64)
- [x] `cargo install` works from the repo
- [x] README: one setup command each for Codex CLI and Claude Code; recommended permissions (`read`, `who` without asking, `send` asks) and the one setting per product to let `send` run without asking
- [x] README states: the OS-user trust boundary, no endless-loop guard in this release, no Windows, same computer only
- [x] README release checklist includes a hands-on check with a real Claude Code and a real Codex Session talking both ways
- [x] The full test suite passes at the release commit

## Comments

**Implementer (ticket 08):** Built `peerpost --version`/`--help` (answered before settings are read; `tests/install.rs`), README Before you install / Install / Set up / Permissions / Environment variables and Codex / Releasing, `docs/release-checklist.md` (the release checklist, with the hands-on two-product check, maintainer steps marked), and `.github/workflows/release.yml` (on a `v*` tag: tests on Linux x86_64, builds the four Linux/macOS x86_64+arm64 archives with SHA-256 checksums, reruns tests on Linux arm64, runs the macOS arm64 binary, publishes with `GITHUB_TOKEN`).

Found during setup: Codex CLI starts MCP servers with a filtered environment (no `XDG_RUNTIME_DIR`), while Claude Code passes it, so on a Linux desktop a Codex Session and a Claude Session used different runtime directories and never met. Fixed: the runtime directory falls back to `/run/user/<uid>/peerpost` when that directory exists (unit-tested in `settings.rs`; checked by hand with two Sessions, one given `XDG_RUNTIME_DIR=/run/user/<uid>` and one not).

Proven locally (x86_64 Linux only): `cargo build --release`, the workflow's package and checksum steps, `cargo install --locked --path .` into a temp root plus a two-Session (Claude + simulated Codex) smoke both ways with the installed binary, the full test suite, and the workflow YAML parsing and shellcheck of its scripts. Written but not run: the workflow on GitHub (arm64 and macOS builds), `cargo install --git`, and the hands-on check with real Claude Code and Codex Sessions; those are in the checklist. The repository URL `https://github.com/i4pg/peerpost` is used in `Cargo.toml`, README and the checklist.

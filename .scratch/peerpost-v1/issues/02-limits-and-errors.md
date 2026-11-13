# 02: Size and unread limits, clear errors

**What to build:** A runaway or confused Session gets a clear tool error instead of flooding another Session: messages over 64 KB, a Recipient holding 100 unread, an unknown Recipient, and a Sender name already in use are all refused. Both limits are settings.

**Blocked by:** 01

**Status:** resolved

- [x] `send` over 64 KB returns a too-large error; nothing stored
- [x] `send` to a Recipient with 100 unread returns a recipient-full error until it reads
- [x] `send` to a name that is not a live Session returns an unknown-recipient error
- [x] A second Session registering a Sender name already held by a live Session is refused with a clear error
- [x] Both limits come from a settings file in the OS user's config directory and environment variables; defaults 64 KB / 100
- [x] Tests at Seam 1; README lists the limits and settings

## Comments

**Resolved (branch `peerpost-v1-02`).** The Broker now refuses `send`, storing nothing, for an unknown Recipient (`unknown recipient`, also once a Session has closed), text over `max_message_bytes` (`message too large`), and a Recipient already holding `max_unread` unread (`recipient is full`, until it reads). Checks run in that order. Duplicate Sender name refusal (from ticket 01) is now tested; the error for a taken `RELAY_NAME` tells the Human to restart with another one.

Settings: `config.toml` in `$XDG_CONFIG_HOME/peerpost/` (else `~/.config/peerpost/`), keys `max_message_bytes` and `max_unread`; env vars `PEERPOST_MAX_MESSAGE_BYTES` and `PEERPOST_MAX_UNREAD` win over the file; defaults 65536 / 100. A missing file means defaults; a bad value, a 0, or an unknown key stops peerpost with an error naming it (`deny_unknown_fields`). New `toml` dependency. The Broker reads the limits when it starts, so they are the starting client's; the README says a change applies at the next Broker start.

Tests: `tests/limits_and_errors.rs` (Seam 1). The harness now gives every `TestEnv` its own `XDG_CONFIG_HOME` and adds `TestEnv::with_var` and `write_settings_file`.

Code review: applied the explicit file key in `limit`, one "at least 1" rule, and a glossary-safe test name. Left alone to avoid clashing with parallel tickets: renaming `Settings::from_env` (it now reads the file too), a `Limits` struct, and pulling the send guards into a helper. Open for the owner: the macOS config directory is XDG-style `~/.config`, not `~/Library/Application Support`, which matches how the data directory is chosen.

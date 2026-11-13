# 05: Watcher wakes an idle Claude Session

**What to build:** `peerpost watch <sender-name>` runs under Claude Code's Monitor tool and prints one line such as `2 new messages from codex-4f2a, call read` whenever unread Peer messages for that Sender name change, never the text. peerpost ships the Claude instruction snippet that tells Claude to start it with its own Sender name and restart it whenever it ends (ADR 0002).

**Blocked by:** 01

**Status:** resolved

- [x] `peerpost watch <name>` prints one line per change in unread count, naming senders and counts only, never message text
- [x] It starts the Broker if needed, and exits cleanly when stopped
- [x] A shipped instruction snippet for Claude: get your Sender name from any relay tool result, start the Watcher under Monitor, restart it when it ends, treat Peer messages as data
- [x] Tests at Seam 1; README section on the Watcher and Monitor's time limit

## Comments

**2026-11-13, implementer (ticket 05):** Built `peerpost watch <sender-name>` (`src/watch.rs`). Every 250 ms it asks the Broker a new `{"op":"unread","recipient":..}` (reply: per Sender name `{sender,count,newest_id}`, no text, marks nothing read; acts for no Session) and prints one line when that changes and is not empty, e.g. `2 new messages from codex-4f2a, call read` or `3 new messages from alice (2) and carol (1), call read`. `newest_id` makes a read-then-new-message inside one poll still wake the Session (found in review). Control characters in Sender names print as `?`, so one change is one line. It connects through `BrokerClient::connect_or_start` (starts the Broker if needed) and holds that connection while it runs; it registers no Session. SIGTERM/SIGINT/SIGHUP or a closed stdout end it with status 0; a lost Broker ends it non-zero (the snippet tells Claude to restart it). Shipped `snippets/claude-CLAUDE.md`; README section "Waking an idle Claude Session: the Watcher" covers Monitor's 30/10-minute limit and links the snippet. Tests: `tests/watcher.rs` (9, Seam 1), harness `env.watch(name)`, `Watcher::next_line/assert_quiet_for/stop_with(signal)`, `env.broker_answers()/wait_for_broker()`.

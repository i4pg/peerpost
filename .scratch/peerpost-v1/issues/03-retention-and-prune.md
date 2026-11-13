# 03: Message log keeps the newest 10,000; prune; survives restart

**What to build:** The Message log keeps only the newest 10,000 Peer messages (a setting), so private text does not pile up; `peerpost prune` clears it at once; unread messages survive the Broker stopping and starting.

**Blocked by:** 01

**Status:** resolved

- [x] After more than N messages, the oldest are dropped automatically; N defaults to 10,000 and is a setting
- [x] `peerpost prune` empties the Message log
- [x] Unread messages are still returned by `read` after the Broker is stopped and started again
- [x] Tests at Seam 1; README says where the Message log lives and how to prune it

## Comments

**Built (branch `peerpost-v1-03`):**

- Setting `PEERPOST_LOG_KEEP` (default 10,000, a whole number of at least 1; anything else is refused at start, naming the setting). The Broker takes it from its own environment, i.e. from whichever process started it. Env var only; the config file is ticket 02's.
- Retention in `MessageLog`: past N, the oldest messages leave memory at once (no longer readable). The file is compacted (temp file 0600, fsync, rename, best-effort directory sync) once it holds more than N + N/10 message records, so a send does not rewrite the whole file each time. The README states that bound. A failed compaction is logged and does not fail the send. On start, a log over N is trimmed and compacted (so lowering N applies at once).
- `peerpost prune`: asks the Broker (protocol op `prune`, reply `pruned {removed}`) to empty the log, starting a Broker if none runs. The Broker is the only writer of the log, so a running Broker's memory and file stay in step. It writes the file first, then clears memory. It prints `Message log emptied: N Peer messages removed (<path>)`.
- Message ids never repeat: each rewrite starts with a `next_id` record, which is replayed on open.
- Unread messages survive a Broker stop/start; this was already true after ticket 01 and is now tested (the Broker is killed by the test).
- Tests: `tests/retention.rs` (8 tests). Harness: `TestEnv::set_var`, `run(args)`, `message_log()`, `start_broker()` (a Broker child the test can kill); `PEERPOST_LOG_KEEP` was added to `SCRUBBED_VARS`.
- README: a "The Message log" section covering location, retention, the setting and `peerpost prune`.

**For ticket 04:** compaction renames a fixed `messages.jsonl.tmp` over the log. Two Brokers sharing one data directory (the start race ticket 04 closes with a lock) would orphan each other's file handle, so the lock matters more now. `prune` starts a Broker through `connect_or_start`, so it takes the same lock path.

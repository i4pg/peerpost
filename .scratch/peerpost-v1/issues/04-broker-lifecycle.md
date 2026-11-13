# 04: One Broker under concurrent start; stops after the last Session

**What to build:** Two Sessions starting at the same moment end up on one Broker (lock file), and the Broker exits a short grace period after its last client disconnects, so nothing keeps running when the Human is done.

**Blocked by:** 01

**Status:** resolved

- [x] Several clients started at once all connect to a single Broker; only one Broker process exists
- [x] With no clients connected, the Broker exits after a grace period (a setting) and a later client starts it again
- [x] A stale lock or socket left by a crashed Broker does not stop a new one from starting
- [x] Tests at Seam 1

## Comments

**Resolved (branch `peerpost-v1-04`).** Built test-first at Seam 1 (`tests/broker_lifecycle.rs`, 8 tests).

- One Broker: the Broker holds an `flock` on `broker.lock` in the runtime directory and one in the data directory for its whole life (one Broker per socket and per Message log). It takes them before removing a stale socket and binding; a Broker that cannot get them within 3 s gives way. Clients that find no Broker queue on `start.lock`, so concurrent starts spawn one Broker, and the others connect to it. A crashed Broker's locks die with it, so only its socket file can be left, and the next Broker removes it.
- Grace period: `PEERPOST_GRACE_MS` (default 30 s, at least 100 ms). The Broker counts open client connections and stops once none has been open for the grace period, removing its socket while still holding the locks. The old "socket removed or replaced" exit is the same stop check: an unreachable Broker can serve no one, so it stops at once. Test Brokers still go when `TestEnv` deletes its directories.
- `BrokerClient` reconnects, starting a Broker if none runs, and registers its Sessions again (Claude and Codex registrations alike). It checks an idle connection is still open before sending. A `send` or `read` that went out and got no reply is reported, not repeated, so no message is sent twice or lost. `who`, `register` and `prune` are retried, up to 3 Brokers.
- Config file: grace comes from the environment only, like the other settings until ticket 02's config file lands.


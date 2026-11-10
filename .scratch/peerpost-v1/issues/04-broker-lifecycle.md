# 04: One Broker under concurrent start; stops after the last Session

**What to build:** Two Sessions starting at the same moment end up on one Broker (lock file), and the Broker exits a short grace period after its last client disconnects, so nothing keeps running when the Human is done.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Several clients started at once all connect to a single Broker; only one Broker process exists
- [ ] With no clients connected, the Broker exits after a grace period (a setting) and a later client starts it again
- [ ] A stale lock or socket left by a crashed Broker does not stop a new one from starting
- [ ] Tests at Seam 1

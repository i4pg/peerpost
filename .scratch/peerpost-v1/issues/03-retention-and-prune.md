# 03: Message log keeps the newest 10,000; prune; survives restart

**What to build:** The Message log keeps only the newest 10,000 Peer messages (a setting), so private text does not pile up; `peerpost prune` clears it at once; unread messages survive the Broker stopping and starting.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] After more than N messages, the oldest are dropped automatically; N defaults to 10,000 and is a setting
- [ ] `peerpost prune` empties the Message log
- [ ] Unread messages are still returned by `read` after the Broker is stopped and started again
- [ ] Tests at Seam 1; README says where the Message log lives and how to prune it

# The trust boundary is the OS user

Only processes running as the same OS user can reach the Broker (its socket is private to that user), and the Broker stamps each Peer message with the Sender name its session registered; text written by a model can never set the sender. We do not issue per-session secrets: processes under one OS user can usually read each other's files and configuration, so a token would promise isolation it cannot keep. The README states the limit plainly: anything already running as you can pretend to be any Session.

## Consequences

- Per-session credentials are revisited only for the across-computers release, where a network boundary makes them meaningful.

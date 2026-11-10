# 02: Size and unread limits, clear errors

**What to build:** A runaway or confused Session gets a clear tool error instead of flooding another Session: messages over 64 KB, a Recipient holding 100 unread, an unknown Recipient, and a Sender name already in use are all refused. Both limits are settings.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] `send` over 64 KB returns a too-large error; nothing stored
- [ ] `send` to a Recipient with 100 unread returns a recipient-full error until it reads
- [ ] `send` to a name that is not a live Session returns an unknown-recipient error
- [ ] A second Session registering a Sender name already held by a live Session is refused with a clear error
- [ ] Both limits come from a settings file in the OS user's config directory and environment variables; defaults 64 KB / 100
- [ ] Tests at Seam 1; README lists the limits and settings

//! The Watcher: `peerpost watch <sender-name>`, run by a Claude Session under
//! Claude Code's Monitor tool. Each line it prints wakes the Session, so it
//! prints one line whenever the unread Peer messages for that Sender name
//! change, naming only who wrote and how many: never the text, which reaches
//! the model only through its own `read` call (ADR 0002).
//!
//! It connects through the same start path as the MCP server, so it starts
//! the Broker if none runs, and it holds that connection open for as long as
//! it runs. It registers no Session: it only asks about `recipient`. It ends
//! with status 0 on SIGTERM, SIGINT or SIGHUP, or when nobody reads its
//! output any more.

use std::io::Write;
use std::time::Duration;

use anyhow::{bail, Result};
use tokio::signal::unix::{signal, SignalKind};

use crate::client::BrokerClient;
use crate::protocol::{Reply, Request, UnreadCount};
use crate::settings::Settings;

/// How often the Watcher asks the Broker for the unread counts.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

pub async fn run(settings: Settings, recipient: &str) -> Result<()> {
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    let broker = BrokerClient::connect_or_start(&settings).await?;
    let mut last: Vec<UnreadCount> = Vec::new();
    let mut poll = tokio::time::interval(POLL_INTERVAL);
    loop {
        tokio::select! {
            _ = term.recv() => return Ok(()),
            _ = int.recv() => return Ok(()),
            _ = hup.recv() => return Ok(()),
            _ = poll.tick() => {
                let counts = match broker.request(&Request::Unread { recipient: recipient.to_string() }).await {
                    Ok(Reply::Unread(counts)) => counts,
                    Ok(other) => bail!("unexpected Broker reply: {other:?}"),
                    Err(e) => bail!("the Watcher for `{recipient}` stopped: {e}"),
                };
                // Any change in who, how many, or which newest message is
                // news; all read is not.
                if counts != last && !counts.is_empty() {
                    let mut out = std::io::stdout().lock();
                    if writeln!(out, "{}", wake_line(&counts)).and_then(|_| out.flush()).is_err() {
                        // Nobody reads the output any more (Monitor stopped).
                        return Ok(());
                    }
                }
                last = counts;
            }
        }
    }
}

/// `2 new messages from codex-4f2a, call read`, or with several senders
/// `3 new messages from alice (2) and bob (1), call read`. Counts are of all
/// messages still unread, so a restarted Watcher repeats them once.
fn wake_line(counts: &[UnreadCount]) -> String {
    let total: usize = counts.iter().map(|c| c.count).sum();
    let plural = if total == 1 { "" } else { "s" };
    let senders = match counts {
        [only] => printable_sender_name(&only.sender),
        _ => {
            let parts: Vec<String> =
                counts.iter().map(|c| format!("{} ({})", printable_sender_name(&c.sender), c.count)).collect();
            let (last, rest) = parts.split_last().expect("at least two senders");
            format!("{} and {last}", rest.join(", "))
        }
    };
    format!("{total} new message{plural} from {senders}, call read")
}

/// A Sender name as printed: control characters (a newline in `RELAY_NAME`,
/// say) become `?`, so one change is always exactly one line.
fn printable_sender_name(name: &str) -> String {
    name.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

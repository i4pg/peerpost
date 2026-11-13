//! peerpost: a local message relay for Codex CLI and Claude Code Sessions.
//!
//! One binary, several roles chosen by the first argument:
//! - no argument: the MCP server over stdio (one per Session)
//! - `broker`: the Broker (normally started on demand, never by hand)
//! - `prune`: empty the Message log now
//! - `watch <sender-name>`: the Watcher, run by a Claude Session under Monitor

mod broker;
mod client;
mod mcp;
mod message_log;
mod nudge;
mod private_fs;
mod protocol;
mod settings;
mod watch;

use anyhow::{bail, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let settings = settings::Settings::from_env()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => mcp::serve_stdio(settings).await,
        Some("broker") => broker::run(settings).await,
        Some("prune") => prune(settings).await,
        Some("watch") => match args.get(1..).unwrap_or_default() {
            [name] if !name.is_empty() => watch::run(settings, name).await,
            _ => bail!("usage: peerpost watch <sender-name> (your own Sender name, from any relay tool result)"),
        },
        Some(other) => {
            bail!("unknown command `{other}`; expected no argument (MCP server), `broker`, `prune` or `watch`")
        }
    }
}

/// Empty the Message log. The Broker owns the log, so this asks the running
/// Broker (starting one if none runs) rather than touching the file behind
/// its back; a running Broker's memory and file stay in step.
async fn prune(settings: settings::Settings) -> Result<()> {
    let broker = client::BrokerClient::connect_or_start(&settings).await?;
    match broker.request(&protocol::Request::Prune).await {
        Ok(protocol::Reply::Pruned { removed }) => {
            println!(
                "Message log emptied: {removed} Peer message{} removed ({})",
                if removed == 1 { "" } else { "s" },
                settings.message_log_path().display()
            );
            Ok(())
        }
        Ok(other) => bail!("unexpected Broker reply: {other:?}"),
        Err(e) => bail!("prune failed: {e}"),
    }
}

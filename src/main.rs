//! peerpost: a local message relay for Codex CLI and Claude Code Sessions.
//!
//! One binary, several roles chosen by the first argument:
//! - no argument: the MCP server over stdio (one per Session)
//! - `broker`: the Broker (normally started on demand, never by hand)

mod broker;
mod client;
mod mcp;
mod message_log;
mod private_fs;
mod protocol;
mod settings;

use anyhow::{bail, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let settings = settings::Settings::from_env()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("serve") => mcp::serve_stdio(settings).await,
        Some("broker") => broker::run(settings).await,
        Some(other) => bail!("unknown command `{other}`; expected no argument (MCP server) or `broker`"),
    }
}

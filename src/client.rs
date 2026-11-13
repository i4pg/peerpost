//! A connection to the Broker, starting the Broker on demand.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::private_fs;
use crate::protocol::{Reply, Request, Response};
use crate::settings::Settings;

/// How long a client waits for a Broker it started to answer.
const START_TIMEOUT: Duration = Duration::from_secs(5);
const START_POLL: Duration = Duration::from_millis(25);

pub struct BrokerClient {
    /// One request in flight at a time; responses come back in order.
    conn: Mutex<(Lines<BufReader<OwnedReadHalf>>, OwnedWriteHalf)>,
}

impl BrokerClient {
    /// Connect to the running Broker, or start one in the background and
    /// wait until it answers.
    pub async fn connect_or_start(settings: &Settings) -> Result<Self> {
        let socket = settings.socket_path();
        let stream = match UnixStream::connect(&socket).await {
            Ok(stream) => stream,
            Err(_) => {
                start_broker(settings)?;
                wait_for_broker(settings).await?
            }
        };
        let (r, w) = stream.into_split();
        Ok(Self { conn: Mutex::new((BufReader::new(r).lines(), w)) })
    }

    /// Send one request; `Err` carries the Broker's error text (meant for the
    /// calling model) or a transport failure.
    pub async fn request(&self, request: &Request) -> Result<Reply, String> {
        let mut conn = self.conn.lock().await;
        let (lines, writer) = &mut *conn;
        let mut line = serde_json::to_string(request).map_err(|e| e.to_string())?;
        line.push('\n');
        writer.write_all(line.as_bytes()).await.map_err(|e| format!("lost the Broker: {e}"))?;
        let reply = lines
            .next_line()
            .await
            .map_err(|e| format!("lost the Broker: {e}"))?
            .ok_or_else(|| "lost the Broker: connection closed".to_string())?;
        match serde_json::from_str::<Response>(&reply).map_err(|e| format!("bad Broker reply: {e}"))? {
            Response::Ok { ok } => Ok(ok),
            Response::Err { error } => Err(error),
        }
    }
}

/// Start `<this binary> broker` detached in its own process group, so it
/// outlives the Session that started it. Its stderr goes to the broker log.
fn start_broker(settings: &Settings) -> Result<()> {
    use std::os::unix::process::CommandExt;
    private_fs::ensure_dir(&settings.runtime_dir)?;
    let log = private_fs::open_append(&settings.broker_log_path())?;
    let exe = std::env::current_exe().context("cannot find the peerpost binary")?;
    let mut child = std::process::Command::new(exe)
        .arg("broker")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("cannot start the Broker")?;
    // Reap the child when it exits so it never lingers as a zombie while
    // this process runs.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

async fn wait_for_broker(settings: &Settings) -> Result<UnixStream> {
    let socket = settings.socket_path();
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    loop {
        match UnixStream::connect(&socket).await {
            Ok(stream) => return Ok(stream),
            Err(e) if tokio::time::Instant::now() >= deadline => {
                bail!(anyhow!(e).context(format!(
                    "the Broker did not answer on {} (see {})",
                    socket.display(),
                    settings.broker_log_path().display()
                )))
            }
            Err(_) => tokio::time::sleep(START_POLL).await,
        }
    }
}

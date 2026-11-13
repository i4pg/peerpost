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

type Conn = (Lines<BufReader<OwnedReadHalf>>, OwnedWriteHalf);

pub struct BrokerClient {
    settings: Settings,
    /// One request in flight at a time; responses come back in order.
    inner: Mutex<Inner>,
}

struct Inner {
    /// `None` after the Broker went away, until the next request reconnects.
    conn: Option<Conn>,
    /// Sessions registered through this client, registered again on every
    /// reconnect so a restarted Broker knows them.
    sessions: Vec<String>,
}

impl BrokerClient {
    /// Connect to the running Broker, or start one in the background and
    /// wait until it answers.
    pub async fn connect_or_start(settings: &Settings) -> Result<Self> {
        let conn = connect_or_start(settings).await?;
        Ok(Self { settings: settings.clone(), inner: Mutex::new(Inner { conn: Some(conn), sessions: Vec::new() }) })
    }

    /// Send one request; `Err` carries the Broker's error text (meant for the
    /// calling model) or a transport failure.
    ///
    /// If the Broker has gone away (it stopped, crashed, or lost a start
    /// race), this connects to, or starts, a new one, registers this client's
    /// Sessions again and retries the request once. A request the old Broker
    /// handled just before it died may therefore be handled twice.
    pub async fn request(&self, request: &Request) -> Result<Reply, String> {
        let mut inner = self.inner.lock().await;
        let reply = match inner.conn.as_mut() {
            Some(conn) => exchange(conn, request).await,
            None => Err(Lost("not connected".into())),
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(Lost(why)) => {
                inner.conn = None;
                let conn =
                    self.reconnect(&inner.sessions).await.map_err(|e| format!("lost the Broker ({why}): {e}"))?;
                let conn = inner.conn.insert(conn);
                exchange(conn, request).await.map_err(|Lost(e)| format!("lost the Broker: {e}"))?
            }
        };
        match reply {
            Response::Ok { ok } => {
                if let Request::Register { session } = request {
                    inner.sessions.push(session.clone());
                }
                Ok(ok)
            }
            Response::Err { error } => Err(error),
        }
    }

    async fn reconnect(&self, sessions: &[String]) -> Result<Conn, String> {
        let mut conn = connect_or_start(&self.settings).await.map_err(|e| format!("{e:#}"))?;
        for session in sessions {
            let register = Request::Register { session: session.clone() };
            match exchange(&mut conn, &register).await.map_err(|Lost(e)| e)? {
                Response::Ok { .. } => {}
                Response::Err { error } => return Err(format!("could not register `{session}` again: {error}")),
            }
        }
        Ok(conn)
    }
}

/// The connection broke before a whole reply came back.
struct Lost(String);

/// Write one request line and read one response line.
async fn exchange(conn: &mut Conn, request: &Request) -> Result<Response, Lost> {
    let (lines, writer) = conn;
    let mut line = serde_json::to_string(request).expect("requests always serialize");
    line.push('\n');
    writer.write_all(line.as_bytes()).await.map_err(|e| Lost(e.to_string()))?;
    let reply =
        lines.next_line().await.map_err(|e| Lost(e.to_string()))?.ok_or_else(|| Lost("connection closed".into()))?;
    serde_json::from_str::<Response>(&reply).map_err(|e| Lost(format!("bad Broker reply: {e}")))
}

/// Connect to the running Broker, or start one and wait until it answers.
/// Clients that find no Broker at the same moment queue on the start lock,
/// and all but the first find the first one's Broker answering.
async fn connect_or_start(settings: &Settings) -> Result<Conn> {
    let stream = match UnixStream::connect(settings.socket_path()).await {
        Ok(stream) => stream,
        Err(_) => {
            private_fs::ensure_dir(&settings.runtime_dir)?;
            let lock_path = settings.start_lock_path();
            let start_lock = tokio::task::spawn_blocking(move || private_fs::lock_file(&lock_path, START_TIMEOUT))
                .await
                .context("start lock task failed")??;
            if start_lock.is_none() {
                bail!(
                    "another client has been starting the Broker for too long ({})",
                    settings.start_lock_path().display()
                );
            }
            match UnixStream::connect(settings.socket_path()).await {
                Ok(stream) => stream,
                Err(_) => {
                    start_broker(settings)?;
                    wait_for_broker(settings).await?
                }
            }
        }
    };
    let (r, w) = stream.into_split();
    Ok((BufReader::new(r).lines(), w))
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

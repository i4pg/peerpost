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
/// How long a client waits for another client that is starting the Broker:
/// longer than that client's own wait, so the queue never gives up first.
const START_LOCK_WAIT: Duration = Duration::from_secs(10);
/// How many Brokers a request that is safe to repeat tries before giving up.
const ATTEMPTS: usize = 3;

pub struct BrokerClient {
    settings: Settings,
    /// One request in flight at a time; responses come back in order.
    inner: Mutex<Inner>,
}

struct Inner {
    /// `None` after the Broker went away, until the next request reconnects.
    conn: Option<Conn>,
    /// Registrations made through this client, made again on every reconnect
    /// so a new Broker knows the same Sessions under the same Sender names.
    registrations: Vec<Request>,
}

impl BrokerClient {
    /// Connect to the running Broker, or start one in the background and
    /// wait until it answers.
    pub async fn connect_or_start(settings: &Settings) -> Result<Self> {
        let conn = Conn::open(settings).await?;
        Ok(Self {
            settings: settings.clone(),
            inner: Mutex::new(Inner { conn: Some(conn), registrations: Vec::new() }),
        })
    }

    /// Send one request; `Err` carries the Broker's error text (meant for the
    /// calling model) or a transport failure.
    ///
    /// If the Broker has gone away (it stopped, crashed, or lost a start
    /// race), this connects to a new one, starting it if need be, and
    /// registers this client's Sessions again first. A request that went out
    /// and got no reply is repeated only if repeating it is harmless (not
    /// `send` or `read`, which could duplicate or lose a Peer message).
    pub async fn request(&self, request: &Request) -> Result<Reply, String> {
        let mut inner = self.inner.lock().await;
        let mut lost = String::new();
        for _ in 0..ATTEMPTS {
            let inner = &mut *inner;
            let conn = match inner.conn.take().and_then(|mut c| (!c.is_closed()).then_some(c)) {
                Some(conn) => conn,
                None => {
                    let (conn, refused) =
                        self.reconnect(&mut inner.registrations).await.map_err(|e| format!("lost the Broker: {e}"))?;
                    // Only a call made for a refused Session fails here; the
                    // client's other Sessions carry on.
                    if let Some((_, error)) =
                        refused.into_iter().find(|(name, _)| Some(name.as_str()) == request.session())
                    {
                        inner.conn = Some(conn);
                        return Err(format!("the Broker restarted and refused this Session: {error}"));
                    }
                    conn
                }
            };
            match inner.conn.insert(conn).exchange(request).await {
                Ok(Response::Ok { ok }) => {
                    if let Request::Register { .. } = request {
                        inner.registrations.push(request.clone());
                    }
                    return Ok(ok);
                }
                Ok(Response::Err { error }) => return Err(error),
                Err(Failure::BadReply(e)) => return Err(format!("bad Broker reply: {e}")),
                Err(Failure::Lost(e)) => {
                    inner.conn = None;
                    if matches!(request, Request::Send { .. } | Request::Read { .. }) {
                        return Err(format!(
                            "lost the Broker during this call ({e}); it may or may not have been handled, \
                             so it was not repeated; the next call reconnects"
                        ));
                    }
                    lost = e;
                }
            }
        }
        Err(format!("lost the Broker: {lost}"))
    }

    /// Whether `name` is registered through this client: false once a
    /// restarted Broker refused it (see [`Self::request`]).
    pub async fn is_registered(&self, name: &str) -> bool {
        self.inner.lock().await.registrations.iter().any(|r| r.session() == Some(name))
    }

    /// Open a connection and register `registrations` on it again. A Sender
    /// name a new Session took in the meantime is dropped from the list and
    /// returned with the Broker's error.
    async fn reconnect(&self, registrations: &mut Vec<Request>) -> Result<(Conn, Vec<(String, String)>), String> {
        let mut conn = Conn::open(&self.settings).await.map_err(|e| format!("{e:#}"))?;
        let mut refused = Vec::new();
        let mut kept = Vec::new();
        for register in registrations.drain(..) {
            match conn.exchange(&register).await {
                Ok(Response::Ok { .. }) => kept.push(register),
                Ok(Response::Err { error }) => {
                    refused.push((register.session().unwrap_or_default().to_string(), error))
                }
                Err(Failure::Lost(e) | Failure::BadReply(e)) => return Err(e),
            }
        }
        *registrations = kept;
        Ok((conn, refused))
    }
}

/// Why a request got no usable reply.
enum Failure {
    /// The connection broke; the Broker may or may not have handled it.
    Lost(String),
    /// A reply came back but was not a protocol response.
    BadReply(String),
}

/// One open connection to a Broker.
struct Conn {
    lines: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl Conn {
    /// Connect to the running Broker, or start one and wait until it
    /// answers. Clients that find no Broker at the same moment queue on the
    /// start lock, and all but the first find the first one's Broker
    /// answering.
    async fn open(settings: &Settings) -> Result<Conn> {
        let stream = match UnixStream::connect(settings.socket_path()).await {
            Ok(stream) => stream,
            Err(_) => {
                private_fs::ensure_dir(&settings.runtime_dir)?;
                let lock_path = settings.start_lock_path();
                let _start_lock =
                    tokio::task::spawn_blocking(move || private_fs::lock_file(&lock_path, START_LOCK_WAIT))
                        .await
                        .context("start lock task failed")??
                        .context("another client has been starting the Broker for too long")?;
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
        Ok(Conn { lines: BufReader::new(r).lines(), writer: w })
    }

    /// Whether the Broker has already closed this idle connection. Checked
    /// before a request goes out, so a Broker that went away between calls
    /// never leaves a `send` or `read` in doubt.
    fn is_closed(&mut self) -> bool {
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        // Pending: open and quiet, as it should be between requests. Anything
        // else (end of stream, an error, or an unasked-for line) means this
        // connection can no longer be trusted.
        !std::pin::Pin::new(&mut self.lines).poll_next_line(&mut cx).is_pending()
    }

    /// Write one request line and read one response line.
    async fn exchange(&mut self, request: &Request) -> Result<Response, Failure> {
        let mut line = serde_json::to_string(request).expect("requests always serialize");
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.map_err(|e| Failure::Lost(e.to_string()))?;
        let reply = self
            .lines
            .next_line()
            .await
            .map_err(|e| Failure::Lost(e.to_string()))?
            .ok_or_else(|| Failure::Lost("connection closed".into()))?;
        serde_json::from_str::<Response>(&reply).map_err(|e| Failure::BadReply(e.to_string()))
    }
}

/// Start `<this binary> broker` detached in its own process group, so it
/// outlives the Session that started it. Its stderr goes to the broker log.
fn start_broker(settings: &Settings) -> Result<()> {
    use std::os::unix::process::CommandExt;
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

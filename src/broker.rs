//! The Broker: one local process per OS user that owns the Unix socket and
//! the Message log, registers Sessions, stamps Sender names and hands out
//! Peer messages. The MCP servers (and later the Watcher) are its clients.
//!
//! Lifecycle: the Broker holds a lock file in the runtime directory and one
//! in the data directory for its whole life, so only one runs per socket and
//! per Message log. It stops once it can serve no one: when no client has
//! been connected for the grace period, or at once when no client can reach
//! it any more (its socket file was removed or replaced; this is also how a
//! test's Broker goes when the test's directories are deleted). Clients
//! reconnect to, or start, a new Broker when theirs goes.

use std::collections::HashMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::message_log::MessageLog;
use crate::private_fs;
use crate::protocol::{Reply, Request, Response, NAME_TAKEN};
use crate::settings::Settings;

/// The longest the Broker waits between checks of its socket and its idle
/// time; shorter when the grace period is shorter.
const MAX_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// How long a starting Broker waits for the Broker locks. A Broker on its
/// way out holds them briefly; a live one holds them for good, and then the
/// new one gives way.
const LOCK_WAIT: Duration = Duration::from_secs(3);

type ConnId = u64;

struct State {
    log: MessageLog,
    /// Live Sessions by Sender name.
    sessions: HashMap<String, LiveSession>,
    /// Open client connections.
    clients: usize,
    /// When the last client left (or the Broker started, if none came yet).
    idle_since: Instant,
}

struct LiveSession {
    /// The connection that registered it; it may act only for its own.
    conn: ConnId,
    /// A Codex Session's thread id (ADR 0006), for `codex queue --thread`.
    codex_thread: Option<String>,
}

/// Why a running Broker stops.
enum Stop {
    /// Its socket file was removed or replaced: no new client can reach it.
    Unreachable,
    /// No client has been connected for the grace period.
    Idle,
}

pub async fn run(settings: Settings) -> Result<()> {
    private_fs::ensure_dir(&settings.runtime_dir)?;
    private_fs::ensure_dir(&settings.data_dir)?;
    let socket = settings.socket_path();

    // The Broker locks make this the only Broker: whoever holds them owns
    // the socket and the Message log. A crashed Broker's locks are released
    // by the kernel, so only its socket file can be left behind, and that is
    // removed below.
    let lock_paths = settings.broker_lock_paths();
    let locks = tokio::task::spawn_blocking(move || {
        lock_paths.iter().map(|path| private_fs::lock_file(path, LOCK_WAIT)).collect::<Result<Option<Vec<_>>>>()
    })
    .await??;
    let Some(locks) = locks else {
        eprintln!("peerpost broker: another Broker holds the lock; giving way");
        return Ok(());
    };

    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).with_context(|| format!("cannot bind {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let socket_ino = std::fs::metadata(&socket)?.ino();

    let state = Arc::new(Mutex::new(State {
        log: MessageLog::open(&settings.message_log_path(), settings.log_keep)?,
        sessions: HashMap::new(),
        clients: 0,
        idle_since: Instant::now(),
    }));

    let mut next_conn: ConnId = 0;
    let mut check = tokio::time::interval((settings.grace / 2).clamp(Duration::from_millis(10), MAX_CHECK_INTERVAL));
    let stop = loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    next_conn += 1;
                    let client = Client::arrive(next_conn, state.clone());
                    tokio::spawn(serve_connection(stream, client));
                }
                // A passing failure (such as running out of file
                // descriptors) must not stop the Broker for everyone.
                Err(e) => {
                    eprintln!("peerpost broker: accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            },
            _ = check.tick() => {
                if !still_owns_socket(&socket, socket_ino) {
                    break Stop::Unreachable;
                }
                let state = state.lock().expect("broker state poisoned");
                if state.clients == 0 && state.idle_since.elapsed() >= settings.grace {
                    break Stop::Idle;
                }
            }
        }
    };
    if let Stop::Idle = stop {
        // Remove the socket while still holding the lock, so the next client
        // finds no socket and starts a fresh Broker, which takes the lock
        // once this one has exited. A client that connected just now sees its
        // connection close and reconnects to that fresh Broker.
        let _ = std::fs::remove_file(&socket);
    }
    drop(listener);
    drop(locks);
    Ok(())
}

fn still_owns_socket(socket: &Path, ino: u64) -> bool {
    std::fs::metadata(socket).is_ok_and(|m| m.ino() == ino)
}

/// One open client connection, counted while it lives, so the Broker knows
/// when it is idle. Dropping it (even when its task panics) ends the
/// connection's Sessions and may start the grace period.
struct Client {
    conn: ConnId,
    state: Arc<Mutex<State>>,
}

impl Client {
    fn arrive(conn: ConnId, state: Arc<Mutex<State>>) -> Self {
        state.lock().expect("broker state poisoned").clients += 1;
        Self { conn, state }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.sessions.retain(|_, live| live.conn != self.conn);
        state.clients -= 1;
        if state.clients == 0 {
            state.idle_since = Instant::now();
        }
    }
}

async fn serve_connection(stream: UnixStream, client: Client) {
    let (conn, state) = (client.conn, &client.state);
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(&mut state.lock().expect("broker state poisoned"), conn, request),
            Err(e) => Response::Err { error: format!("malformed request: {e}") },
        };
        let mut out = serde_json::to_string(&response).expect("responses always serialize");
        out.push('\n');
        if write_half.write_all(out.as_bytes()).await.is_err() {
            break;
        }
    }
}

fn handle(state: &mut State, conn: ConnId, request: Request) -> Response {
    match handle_inner(state, conn, request) {
        Ok(ok) => Response::Ok { ok },
        Err(error) => Response::Err { error },
    }
}

fn handle_inner(state: &mut State, conn: ConnId, request: Request) -> Result<Reply, String> {
    match request {
        Request::Register { session, codex_thread } => {
            if state.sessions.contains_key(&session) {
                return Err(format!("the Sender name `{session}` {NAME_TAKEN}"));
            }
            if let Some(thread) = &codex_thread {
                // One Session per Codex thread, so a Nudge has one target.
                if let Some((name, _)) = state.sessions.iter().find(|(_, s)| s.codex_thread.as_ref() == Some(thread)) {
                    return Err(format!("Codex thread {thread} is already live as `{name}` on another connection"));
                }
            }
            state.sessions.insert(session, LiveSession { conn, codex_thread });
            Ok(Reply::Registered)
        }
        Request::Who => {
            let mut names: Vec<String> = state.sessions.keys().cloned().collect();
            names.sort();
            Ok(Reply::Sessions(names))
        }
        Request::Send { session, recipient, text } => {
            check_owner(state, conn, &session)?;
            let id = state.log.append(&session, &recipient, &text).map_err(|e| format!("{e:#}"))?;
            Ok(Reply::Accepted { id })
        }
        Request::Read { session } => {
            check_owner(state, conn, &session)?;
            let messages = state.log.take_unread(&session).map_err(|e| format!("{e:#}"))?;
            Ok(Reply::Messages(messages))
        }
        Request::Prune => {
            let removed = state.log.clear().map_err(|e| format!("{e:#}"))?;
            Ok(Reply::Pruned { removed })
        }
    }
}

/// A connection may only act for Sessions it registered itself.
fn check_owner(state: &State, conn: ConnId, session: &str) -> Result<(), String> {
    match state.sessions.get(session) {
        Some(live) if live.conn == conn => Ok(()),
        _ => Err(format!("`{session}` is not a Session registered on this connection")),
    }
}

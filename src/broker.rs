//! The Broker: one local process per OS user that owns the Unix socket and
//! the Message log, registers Sessions, stamps Sender names and hands out
//! Peer messages. The MCP servers (and later the Watcher) are its clients.
//!
//! Lifecycle: the Broker holds the Broker lock for its whole life, so only
//! one runs per runtime directory. It stops when nobody can reach it any more
//! (its socket file was removed or replaced) or when no client has been
//! connected for the grace period.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::message_log::MessageLog;
use crate::private_fs;
use crate::protocol::{Reply, Request, Response};
use crate::settings::Settings;

/// The longest the Broker waits between checks of its socket and its idle
/// time; shorter when the grace period is shorter.
const MAX_CHECK_INTERVAL: Duration = Duration::from_millis(500);

/// How long a starting Broker waits for the Broker lock. Only a Broker that
/// is on its way out holds it briefly; a live one holds it for good.
const LOCK_WAIT: Duration = Duration::from_secs(1);

type ConnId = u64;

struct State {
    log: MessageLog,
    /// Live Sessions: Sender name -> the connection that registered it.
    sessions: HashMap<String, ConnId>,
    /// Open client connections.
    clients: usize,
    /// When the last client left (or the Broker started, if none came yet).
    idle_since: Instant,
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

    // The Broker lock makes this the only Broker: whoever holds it owns the
    // socket. A crashed Broker's lock is released by the kernel, so only its
    // socket file can be left behind, and that is removed below.
    let Some(mut lock) = private_fs::lock_file(&settings.broker_lock_path(), LOCK_WAIT)? else {
        return Ok(());
    };
    lock.set_len(0)?;
    writeln!(lock, "{}", std::process::id())?;

    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).with_context(|| format!("cannot bind {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let socket_ino = std::fs::metadata(&socket)?.ino();

    let state = Arc::new(Mutex::new(State {
        log: MessageLog::open(&settings.message_log_path())?,
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
                    state.lock().expect("broker state poisoned").clients += 1;
                    tokio::spawn(serve_connection(stream, next_conn, state.clone()));
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
    drop(lock);
    Ok(())
}

fn still_owns_socket(socket: &Path, ino: u64) -> bool {
    std::fs::metadata(socket).is_ok_and(|m| m.ino() == ino)
}

async fn serve_connection(stream: UnixStream, conn: ConnId, state: Arc<Mutex<State>>) {
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
    let mut state = state.lock().expect("broker state poisoned");
    state.sessions.retain(|_, owner| *owner != conn);
    state.clients -= 1;
    if state.clients == 0 {
        state.idle_since = Instant::now();
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
        Request::Register { session } => {
            if state.sessions.contains_key(&session) {
                return Err(format!("the Sender name `{session}` is already used by a live Session"));
            }
            state.sessions.insert(session, conn);
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
    }
}

/// A connection may only act for Sessions it registered itself.
fn check_owner(state: &State, conn: ConnId, session: &str) -> Result<(), String> {
    match state.sessions.get(session) {
        Some(owner) if *owner == conn => Ok(()),
        _ => Err(format!("`{session}` is not a Session registered on this connection")),
    }
}

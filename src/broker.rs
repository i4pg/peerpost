//! The Broker: one local process per OS user that owns the Unix socket and
//! the Message log, registers Sessions, stamps Sender names and hands out
//! Peer messages. The MCP servers (and later the Watcher) are its clients.

use std::collections::HashMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::message_log::MessageLog;
use crate::private_fs;
use crate::protocol::{Reply, Request, Response};
use crate::settings::Settings;

/// How often the Broker checks that its socket file is still in place.
const SOCKET_CHECK_INTERVAL: Duration = Duration::from_millis(500);

type ConnId = u64;

struct State {
    log: MessageLog,
    /// Live Sessions by Sender name.
    sessions: HashMap<String, LiveSession>,
}

struct LiveSession {
    /// The connection that registered it; it may act only for its own.
    conn: ConnId,
    /// A Codex Session's thread id (ADR 0006), for `codex queue --thread`.
    #[allow(dead_code)] // read by the Nudge (ticket 07)
    codex_thread: Option<String>,
}

pub async fn run(settings: Settings) -> Result<()> {
    private_fs::ensure_dir(&settings.runtime_dir)?;
    private_fs::ensure_dir(&settings.data_dir)?;
    let socket = settings.socket_path();

    if UnixStream::connect(&socket).await.is_ok() {
        // Another Broker already answers; there must be only one.
        return Ok(());
    }
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).with_context(|| format!("cannot bind {}", socket.display()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let socket_ino = std::fs::metadata(&socket)?.ino();

    let state =
        Arc::new(Mutex::new(State { log: MessageLog::open(&settings.message_log_path())?, sessions: HashMap::new() }));

    let mut next_conn: ConnId = 0;
    let mut check = tokio::time::interval(SOCKET_CHECK_INTERVAL);
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    next_conn += 1;
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
                    // The socket was removed or replaced: nobody can reach
                    // this Broker any more, so it stops.
                    return Ok(());
                }
            }
        }
    }
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
    state.sessions.retain(|_, live| live.conn != conn);
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
                return Err(format!("the Sender name `{session}` is already used by a live Session"));
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
    }
}

/// A connection may only act for Sessions it registered itself.
fn check_owner(state: &State, conn: ConnId, session: &str) -> Result<(), String> {
    match state.sessions.get(session) {
        Some(live) if live.conn == conn => Ok(()),
        _ => Err(format!("`{session}` is not a Session registered on this connection")),
    }
}

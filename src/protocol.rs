//! The Broker protocol: one JSON object per line over the Unix socket, one
//! response line per request line, in order.
//!
//! A connection may register several Sessions (one Claude Session per MCP
//! server, or many Codex threads over one MCP server), so every request that
//! acts for a Session names it in `session`; the Broker only accepts a
//! `session` this connection registered. Registrations end when the
//! connection closes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register a Session under `session` on this connection. A Codex
    /// Session also gives its Codex thread id, which the Broker keeps so it
    /// can Nudge that thread.
    Register {
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        codex_thread: Option<String>,
    },
    /// List the Sender names of live Sessions.
    Who,
    /// Accept a Peer message from `session` (stamped as its Sender name) to
    /// `recipient`.
    Send { session: String, recipient: String, text: String },
    /// Return `session`'s unread Peer messages, oldest first, and mark them read.
    Read { session: String },
    /// Empty the Message log now, read and unread alike (`peerpost prune`).
    /// Acts for no Session; anyone who can reach the socket may ask.
    Prune,
    /// How many unread Peer messages wait for `recipient`, per Sender name
    /// (for the Watcher). Never returns text and marks nothing read. Acts
    /// for no Session: the Watcher names the Session it watches.
    Unread { recipient: String },
}

impl Request {
    /// The Sender name of the Session this request acts for, if any.
    pub fn session(&self) -> Option<&str> {
        match self {
            Request::Register { session, .. } | Request::Send { session, .. } | Request::Read { session } => {
                Some(session)
            }
            Request::Who | Request::Prune | Request::Unread { .. } => None,
        }
    }
}

/// The end of the Broker's error when a Sender name is already live; a
/// client that picks names itself tries another name only on this error.
pub const NAME_TAKEN: &str = "is already used by a live Session";

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok { ok: Reply },
    Err { error: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Registered,
    Sessions(Vec<String>),
    Accepted { id: u64 },
    Messages(Vec<PeerMessage>),
    Pruned { removed: usize },
    Unread(Vec<UnreadCount>),
}

/// How many unread Peer messages one Sender name has waiting for a
/// Recipient, and the id of the newest (so a message read and replaced by a
/// new one between two looks still shows as a change). Carries no text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreadCount {
    pub sender: String,
    pub count: usize,
    pub newest_id: u64,
}

/// A Peer message as handed to its Recipient.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerMessage {
    pub id: u64,
    /// The Sender name the Broker stamped; never taken from the message.
    pub sender: String,
    pub text: String,
    /// Seconds since the Unix epoch when the Broker accepted the message.
    pub accepted_at: u64,
}

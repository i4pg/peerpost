//! The Broker protocol: one JSON object per line over the Unix socket, one
//! response line per request line, in order.
//!
//! A connection may register several Sessions (one Claude Session per MCP
//! server, or many Codex threads over one MCP server), so every request that
//! acts for a Session names it in `session`; the Broker only accepts a
//! `session` this connection registered. Registrations end when the
//! connection closes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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
}

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

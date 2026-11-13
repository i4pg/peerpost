//! The Message log: an append-only file of JSON lines, private to the OS
//! user, replayed into memory when the Broker starts.
//!
//! Two record kinds: `message` (a Peer message the Broker accepted) and
//! `read` (the Recipient has read message `id`). Unread messages therefore
//! survive a Broker restart.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::private_fs;
use crate::protocol::PeerMessage;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Record {
    Message { id: u64, from: String, to: String, text: String, accepted_at: u64 },
    Read { id: u64 },
}

struct Stored {
    to: String,
    message: PeerMessage,
    read: bool,
}

pub struct MessageLog {
    file: File,
    /// Keyed by id, so iteration is oldest first.
    messages: BTreeMap<u64, Stored>,
    next_id: u64,
}

impl MessageLog {
    pub fn open(path: &Path) -> Result<Self> {
        let mut messages = BTreeMap::new();
        if path.exists() {
            let reader = BufReader::new(File::open(path).with_context(|| format!("cannot read {}", path.display()))?);
            for line in reader.lines() {
                let line = line?;
                // A torn last line (crash mid-write) is skipped, not fatal.
                let Ok(record) = serde_json::from_str::<Record>(&line) else { continue };
                match record {
                    Record::Message { id, from, to, text, accepted_at } => {
                        messages.insert(
                            id,
                            Stored { to, message: PeerMessage { id, from, text, accepted_at }, read: false },
                        );
                    }
                    Record::Read { id } => {
                        if let Some(m) = messages.get_mut(&id) {
                            m.read = true;
                        }
                    }
                }
            }
        }
        let next_id = messages.keys().next_back().map_or(1, |id| id + 1);
        Ok(Self { file: private_fs::open_append(path)?, messages, next_id })
    }

    /// Accept a Peer message and return its id.
    pub fn append(&mut self, from: &str, to: &str, text: &str) -> Result<u64> {
        let id = self.next_id;
        let accepted_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.write(&Record::Message { id, from: from.into(), to: to.into(), text: text.into(), accepted_at })?;
        self.next_id += 1;
        self.messages.insert(
            id,
            Stored {
                to: to.into(),
                message: PeerMessage { id, from: from.into(), text: text.into(), accepted_at },
                read: false,
            },
        );
        Ok(id)
    }

    /// Return `recipient`'s unread messages, oldest first, and mark them read.
    pub fn take_unread(&mut self, recipient: &str) -> Result<Vec<PeerMessage>> {
        let ids: Vec<u64> =
            self.messages.iter().filter(|(_, m)| !m.read && m.to == recipient).map(|(id, _)| *id).collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            self.write(&Record::Read { id })?;
            let stored = self.messages.get_mut(&id).expect("id just listed");
            stored.read = true;
            out.push(stored.message.clone());
        }
        Ok(out)
    }

    fn write(&mut self, record: &Record) -> Result<()> {
        let mut line = serde_json::to_string(record)?;
        line.push('\n');
        self.file.write_all(line.as_bytes()).context("cannot write the Message log")?;
        self.file.flush()?;
        Ok(())
    }
}

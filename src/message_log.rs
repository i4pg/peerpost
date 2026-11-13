//! The Message log: an append-only file of JSON lines, private to the OS
//! user, replayed into memory when the Broker starts.
//!
//! Two record kinds: `message` (a Peer message the Broker accepted) and
//! `read` (the Recipient has read message `id`). Unread messages therefore
//! survive a Broker restart.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::private_fs;
use crate::protocol::PeerMessage;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Record {
    Message { id: u64, sender: String, recipient: String, text: String, accepted_at: u64 },
    Read { id: u64 },
}

struct Stored {
    recipient: String,
    message: PeerMessage,
    read: bool,
}

impl Stored {
    fn unread(id: u64, sender: String, recipient: String, text: String, accepted_at: u64) -> Self {
        Self { recipient, message: PeerMessage { id, sender, text, accepted_at }, read: false }
    }
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
                    Record::Message { id, sender, recipient, text, accepted_at } => {
                        messages.insert(id, Stored::unread(id, sender, recipient, text, accepted_at));
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
        let mut file = private_fs::open_append(path)?;
        end_with_newline(path, &mut file)?;
        Ok(Self { file, messages, next_id })
    }

    /// Accept a Peer message and return its id.
    pub fn append(&mut self, sender: &str, recipient: &str, text: &str) -> Result<u64> {
        let id = self.next_id;
        let accepted_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let stored = Stored::unread(id, sender.into(), recipient.into(), text.into(), accepted_at);
        self.write(&[Record::Message {
            id,
            sender: stored.message.sender.clone(),
            recipient: stored.recipient.clone(),
            text: stored.message.text.clone(),
            accepted_at,
        }])?;
        self.next_id += 1;
        self.messages.insert(id, stored);
        Ok(id)
    }

    /// Return `recipient`'s unread messages, oldest first, and mark them read.
    /// The read markers are written in one go before anything is marked, so
    /// a failed write leaves every message unread.
    pub fn take_unread(&mut self, recipient: &str) -> Result<Vec<PeerMessage>> {
        let ids: Vec<u64> =
            self.messages.iter().filter(|(_, m)| !m.read && m.recipient == recipient).map(|(id, _)| *id).collect();
        self.write(&ids.iter().map(|&id| Record::Read { id }).collect::<Vec<_>>())?;
        Ok(ids
            .iter()
            .map(|id| {
                let stored = self.messages.get_mut(id).expect("id just listed");
                stored.read = true;
                stored.message.clone()
            })
            .collect())
    }

    fn write(&mut self, records: &[Record]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let mut lines = String::new();
        for record in records {
            lines.push_str(&serde_json::to_string(record)?);
            lines.push('\n');
        }
        self.file.write_all(lines.as_bytes()).context("cannot write the Message log")?;
        self.file.flush()?;
        Ok(())
    }
}

/// If a crash left a torn last line, end it so the next record starts on a
/// line of its own (the torn line is skipped on replay).
fn end_with_newline(path: &Path, file: &mut File) -> Result<()> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(());
    }
    let mut last = [0u8; 1];
    let mut reader = File::open(path)?;
    reader.seek(SeekFrom::Start(len - 1))?;
    reader.read_exact(&mut last)?;
    if last[0] != b'\n' {
        file.write_all(b"\n")?;
    }
    Ok(())
}

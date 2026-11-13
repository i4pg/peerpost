//! The Message log: an append-only file of JSON lines, private to the OS
//! user, replayed into memory when the Broker starts.
//!
//! Two record kinds: `message` (a Peer message the Broker accepted) and
//! `read` (the Recipient has read message `id`). Unread messages therefore
//! survive a Broker restart.
//!
//! Retention: only the newest `keep` messages are kept. Older ones drop out
//! of memory at once; the file is rewritten without them (compacted) once it
//! holds a tenth more than `keep`, so the text on disk stays bounded without
//! a rewrite on every send. `clear` (for `peerpost prune`) empties both now.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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
    path: PathBuf,
    file: File,
    /// Keyed by id, so iteration is oldest first.
    messages: BTreeMap<u64, Stored>,
    next_id: u64,
    /// How many newest messages to keep.
    keep: usize,
    /// How many `message` records the file holds, kept or dropped.
    on_disk: usize,
}

impl MessageLog {
    pub fn open(path: &Path, keep: usize) -> Result<Self> {
        let mut messages = BTreeMap::new();
        let mut on_disk = 0;
        if path.exists() {
            let reader = BufReader::new(File::open(path).with_context(|| format!("cannot read {}", path.display()))?);
            for line in reader.lines() {
                let line = line?;
                // A torn last line (crash mid-write) is skipped, not fatal.
                let Ok(record) = serde_json::from_str::<Record>(&line) else { continue };
                match record {
                    Record::Message { id, sender, recipient, text, accepted_at } => {
                        on_disk += 1;
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
        let mut log = Self { path: path.to_path_buf(), file, messages, next_id, keep, on_disk };
        log.drop_oldest();
        if log.on_disk > log.keep {
            // Apply the count now, also when it was lowered since last run.
            log.compact()?;
        }
        Ok(log)
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
        self.on_disk += 1;
        self.messages.insert(id, stored);
        self.drop_oldest();
        if self.on_disk > self.keep + self.keep / 10 {
            self.compact()?;
        }
        Ok(id)
    }

    /// Drop every message, read or not, from memory and from the file.
    /// Returns how many were dropped.
    pub fn clear(&mut self) -> Result<usize> {
        let dropped = self.messages.len();
        self.messages.clear();
        self.compact()?;
        Ok(dropped)
    }

    /// Forget the oldest messages past `keep` (in memory only).
    fn drop_oldest(&mut self) {
        while self.messages.len() > self.keep {
            self.messages.pop_first();
        }
    }

    /// Rewrite the file to hold exactly the kept messages (and read markers
    /// for those already read): write a private temporary file, then rename
    /// it over the log, so a crash leaves either the old or the new file.
    fn compact(&mut self) -> Result<()> {
        let mut records = Vec::new();
        for (&id, stored) in &self.messages {
            let m = &stored.message;
            records.push(Record::Message {
                id,
                sender: m.sender.clone(),
                recipient: stored.recipient.clone(),
                text: m.text.clone(),
                accepted_at: m.accepted_at,
            });
            if stored.read {
                records.push(Record::Read { id });
            }
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        let mut out = private_fs::create_truncate(&tmp)?;
        out.write_all(lines(&records)?.as_bytes()).context("cannot write the Message log")?;
        out.sync_all()?;
        std::fs::rename(&tmp, &self.path).context("cannot replace the Message log")?;
        self.file = private_fs::open_append(&self.path)?;
        self.on_disk = self.messages.len();
        Ok(())
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
        self.file.write_all(lines(records)?.as_bytes()).context("cannot write the Message log")?;
        self.file.flush()?;
        Ok(())
    }
}

/// One JSON line per record.
fn lines(records: &[Record]) -> Result<String> {
    let mut lines = String::new();
    for record in records {
        lines.push_str(&serde_json::to_string(record)?);
        lines.push('\n');
    }
    Ok(lines)
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

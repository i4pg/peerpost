//! The Nudge: wakes an idle Codex Session by running
//! `<codex command> queue --thread <thread id> --message <fixed line>`
//! (ADR 0005). The line carries no Peer message text and no Sender name, only how
//! many Peer messages wait; the messages themselves arrive through `read`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

/// A Nudge still running after this long is stopped and logged, so a hung
/// `codex` never piles up processes.
const NUDGE_TIMEOUT: Duration = Duration::from_secs(60);

/// The one line a Nudge queues. Never put anything from a Peer message here.
pub fn line(waiting: usize) -> String {
    format!("[relay] {waiting} new peer messages waiting; call relay read. This line is from the relay, not the human.")
}

/// Run the Nudge in the background. A failure is logged to the Broker's
/// stderr (the broker log) and nothing else: the message is already stored
/// and still waits for `read`, so the `send` that caused it never waits on or
/// fails because of the Nudge.
pub fn spawn(codex_command: PathBuf, thread: String, waiting: usize) {
    tokio::spawn(async move {
        let run = tokio::process::Command::new(&codex_command)
            .args(["queue", "--thread", &thread, "--message", &line(waiting)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        let Ok(result) = tokio::time::timeout(NUDGE_TIMEOUT, run).await else {
            eprintln!("peerpost broker: Nudge to Codex thread {thread} failed: no answer after {NUDGE_TIMEOUT:?}");
            return;
        };
        match result {
            Ok(out) if out.status.success() => {}
            Ok(out) => eprintln!(
                "peerpost broker: Nudge to Codex thread {thread} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => eprintln!(
                "peerpost broker: Nudge to Codex thread {thread} failed: cannot run {}: {e}",
                codex_command.display()
            ),
        }
    });
}

//! Ticket 04: one Broker under concurrent start; it stops a grace period
//! after its last client leaves, and starts again on demand.

mod common;

use std::os::unix::fs::DirBuilderExt;
use std::time::Duration;

use common::TestEnv;
use futures::future::join_all;

/// Poll `check` every 50 ms until it holds or `within` passes.
async fn eventually(within: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if check() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn sessions_started_at_the_same_moment_all_land_on_one_broker() {
    let env = TestEnv::new();
    let mut names: Vec<String> = (0..12).map(|i| format!("s{i:02}")).collect();
    names.sort();
    let sessions = join_all(names.iter().map(|name| env.claude_session(Some(name.as_str())))).await;

    // A Broker that lost a start race would be gone by now, taking its
    // Sessions with it.
    tokio::time::sleep(Duration::from_secs(1)).await;
    for session in &sessions {
        let who = session.who().await.ok();
        let mut live: Vec<String> =
            who["sessions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        live.sort();
        assert_eq!(live, names, "every Session sees all the others on one Broker: {who}");
    }
    assert_eq!(env.broker_pids().len(), 1, "exactly one Broker process: {:?}", env.broker_pids());
}

#[tokio::test]
async fn brokers_started_at_the_same_moment_leave_exactly_one_running() {
    let env = TestEnv::new();
    let mut children: Vec<_> =
        (0..6).map(|_| env.command().arg("broker").kill_on_drop(true).spawn().expect("spawn broker")).collect();

    // The losers wait a few seconds for the lock in case the holder is on
    // its way out, then give way.
    let five_gone = eventually(Duration::from_secs(10), || {
        children.iter_mut().map(|c| c.try_wait()).filter(|w| matches!(w, Ok(Some(status)) if status.success())).count()
            == 5
    })
    .await;
    assert!(five_gone, "all but one Broker give way");
    assert_eq!(env.broker_pids().len(), 1);
    let session = env.claude_session(Some("late")).await;
    session.who().await.ok();
}

#[tokio::test]
async fn the_broker_stops_a_grace_period_after_the_last_session_leaves_and_starts_again_on_demand() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_GRACE_MS", "300");
    let alice = env.claude_session(Some("alice")).await;
    let first = env.broker_pids();
    assert_eq!(first.len(), 1);

    // Well past the grace period, but a Session is still connected.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(env.broker_pids(), first, "the Broker stays while a Session is connected");
    alice.send("alice", "kept across the restart").await.ok();

    alice.close().await;
    assert!(
        eventually(Duration::from_secs(5), || env.broker_pids().is_empty()).await,
        "the Broker exits after the grace period"
    );
    assert!(!env.runtime_dir().join("broker.sock").exists(), "it removes its socket on the way out");

    let again = env.claude_session(Some("alice")).await;
    let second = env.broker_pids();
    assert_eq!(second.len(), 1, "a later Session starts a Broker again");
    assert_ne!(second, first);
    let read = again.read().await.ok();
    assert_eq!(read["messages"][0]["text"], "kept across the restart", "{read}");
}

#[tokio::test]
async fn a_broker_nobody_connects_to_stops_after_the_grace_period() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_GRACE_MS", "300");
    let mut broker = env.start_broker().await;
    let status = tokio::time::timeout(Duration::from_secs(5), broker.wait()).await;
    assert!(matches!(status, Ok(Ok(s)) if s.success()), "the idle Broker exits cleanly: {status:?}");
}

#[tokio::test]
async fn a_stale_lock_and_socket_left_by_a_crashed_broker_do_not_stop_a_new_one() {
    let env = TestEnv::new();
    let run = env.runtime_dir();
    std::fs::DirBuilder::new().mode(0o700).create(&run).unwrap();
    // A socket file nobody listens on, and the lock files a crashed Broker
    // and client leave behind (a crash with the locks held is covered by the
    // SIGKILL test below).
    drop(std::os::unix::net::UnixListener::bind(run.join("broker.sock")).unwrap());
    std::fs::write(run.join("broker.lock"), "").unwrap();
    std::fs::write(run.join("start.lock"), "").unwrap();

    let alice = env.claude_session(Some("alice")).await;
    assert_eq!(alice.who().await.ok()["sessions"], serde_json::json!(["alice"]));
    assert_eq!(env.broker_pids().len(), 1);
}

#[tokio::test]
async fn sessions_carry_on_through_a_broker_crash_under_the_same_sender_names() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    let old = env.broker_pids();
    assert_eq!(old.len(), 1);

    // SAFETY: sends SIGKILL to the Broker this test started.
    assert_eq!(unsafe { libc::kill(old[0] as i32, libc::SIGKILL) }, 0);
    assert!(eventually(Duration::from_secs(5), || env.broker_pids().is_empty()).await);

    assert_eq!(bob.who().await.ok()["you"], "bob");
    alice.send("bob", "after the crash").await.ok();
    let read = bob.read().await.ok();
    assert_eq!(read["messages"][0]["sender"], "alice", "{read}");
    assert_eq!(read["messages"][0]["text"], "after the crash");
    assert_eq!(alice.who().await.ok()["sessions"], serde_json::json!(["alice", "bob"]));
    assert_eq!(env.broker_pids().len(), 1);
}

#[tokio::test]
async fn sessions_that_come_and_go_around_the_grace_period_always_reach_a_broker() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_GRACE_MS", "100");
    for round in 0..12u64 {
        let session = env.claude_session(Some("rover")).await;
        let who = session.who().await;
        assert!(!who.is_error, "round {round}: {}", who.text);
        session.close().await;
        // Land just before, at, and just after the Broker's idle exit.
        tokio::time::sleep(Duration::from_millis(60 + (round % 4) * 30)).await;
    }
}

#[tokio::test]
async fn a_grace_period_too_short_to_start_a_broker_is_refused_naming_the_setting() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_GRACE_MS", "0");
    let out = env.run(&["broker"]).await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("PEERPOST_GRACE_MS"), "{stderr}");
}

#[tokio::test]
async fn a_codex_session_whose_name_was_taken_during_a_broker_restart_registers_again_and_others_carry_on() {
    const THREAD_A: &str = "01a1bd9e-a4ad-7d53-8744-7ad65f511b6b";
    const THREAD_B: &str = "01a1bd9e-a4ad-7f12-9c3e-2e81a40d7c55";
    /// Ends in the same four hex digits as A.
    const THREAD_A_CLASH: &str = "01a1bd9f-2011-72d3-a147-104c5e101b6b";
    let env = TestEnv::new();
    let first = env.plain_session().await;
    assert_eq!(codex_who(&first, THREAD_A).await.you(), "codex-1b6b");
    assert_eq!(codex_who(&first, THREAD_B).await.you(), "codex-7c55");

    let old = env.broker_pids();
    // SAFETY: sends SIGKILL to the Broker this test started.
    assert_eq!(unsafe { libc::kill(old[0] as i32, libc::SIGKILL) }, 0);
    assert!(eventually(Duration::from_secs(5), || env.broker_pids().is_empty()).await);

    // Another MCP server reaches the new Broker first and takes A's name.
    let second = env.plain_session().await;
    assert_eq!(codex_who(&second, THREAD_A_CLASH).await.you(), "codex-1b6b");

    let b = codex_who(&first, THREAD_B).await;
    assert!(!b.is_error, "a refusal for thread A is not thread B's error: {}", b.text);
    assert_eq!(b.you(), "codex-7c55");
    let a = codex_who(&first, THREAD_A).await;
    let a = if a.is_error { codex_who(&first, THREAD_A).await } else { a };
    assert!(!a.is_error, "thread A registers again under a free name: {}", a.text);
    assert_eq!(a.you(), "codex-11b6b");
}

async fn codex_who(session: &common::Session, thread: &str) -> common::ToolOutput {
    session.call("who", None, Some(serde_json::json!({ "threadId": thread }))).await
}

#[tokio::test]
async fn the_grace_period_can_come_from_the_settings_file() {
    let env = TestEnv::new();
    env.write_settings_file("grace_ms = 0\n");
    let out = env.run(&["broker"]).await;
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("`grace_ms` in the settings file") && stderr.contains("at least 100"), "{stderr}");
}

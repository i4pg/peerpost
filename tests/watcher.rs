//! Ticket 05: `peerpost watch <name>` wakes an idle Claude Session by
//! printing who has written and how many, never the message text.

mod common;

use common::TestEnv;

#[tokio::test]
async fn the_watcher_prints_the_sender_and_count_of_unread_messages_but_never_their_text() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;
    let mut watcher = env.watch("bob");

    alice.send("bob", "secret one").await.ok();
    assert_eq!(watcher.next_line().await, "1 new message from alice, call read");
    alice.send("bob", "secret two").await.ok();
    assert_eq!(watcher.next_line().await, "2 new messages from alice, call read");
}

#[tokio::test]
async fn with_several_senders_the_line_names_each_with_its_count() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let carol = env.claude_session(Some("carol")).await;
    let _bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "a1").await.ok();
    alice.send("bob", "a2").await.ok();
    carol.send("bob", "c1").await.ok();

    let mut watcher = env.watch("bob");
    assert_eq!(watcher.next_line().await, "3 new messages from alice (2) and carol (1), call read");
}

#[tokio::test]
async fn after_the_session_reads_the_watcher_stays_quiet_until_a_new_message_arrives() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    let mut watcher = env.watch("bob");

    alice.send("bob", "first").await.ok();
    assert_eq!(watcher.next_line().await, "1 new message from alice, call read");
    bob.read().await.ok();
    watcher.assert_quiet_for(1000).await;

    alice.send("bob", "second").await.ok();
    assert_eq!(watcher.next_line().await, "1 new message from alice, call read");
}

#[tokio::test]
async fn messages_for_other_sessions_do_not_wake_the_watcher() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;
    let _carol = env.claude_session(Some("carol")).await;
    let mut watcher = env.watch("bob");

    alice.send("carol", "not for bob").await.ok();
    watcher.assert_quiet_for(1000).await;
}

#[tokio::test]
async fn the_watcher_starts_the_broker_and_reports_messages_already_waiting() {
    let env = TestEnv::new();
    let broker = env.start_broker().await;
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "sent before the watcher ran").await.ok();
    alice.close().await;
    bob.close().await;
    let mut broker = broker;
    broker.kill().await.expect("kill broker");

    // No Broker runs now; the Watcher starts one, which replays the log.
    let mut watcher = env.watch("bob");
    assert_eq!(watcher.next_line().await, "1 new message from alice, call read");
}

#[tokio::test]
async fn the_watcher_exits_cleanly_when_stopped() {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        let env = TestEnv::new();
        let mut watcher = env.watch("bob");
        watcher.assert_quiet_for(300).await;
        let (status, rest) = watcher.stop_with(signal).await;
        assert!(status.success(), "signal {signal}: {status}");
        assert!(rest.is_empty(), "printed on the way out: {rest:?}");
    }
}

#[tokio::test]
async fn a_sender_name_with_a_newline_still_makes_exactly_one_line() {
    let env = TestEnv::new();
    let odd = env.claude_session(Some("odd\nname")).await;
    let _bob = env.claude_session(Some("bob")).await;
    let mut watcher = env.watch("bob");

    odd.send("bob", "hi").await.ok();
    assert_eq!(watcher.next_line().await, "1 new message from odd?name, call read");
    watcher.assert_quiet_for(500).await;
}

#[tokio::test]
async fn watch_without_a_sender_name_fails_with_usage() {
    let env = TestEnv::new();
    let out = env.run(&["watch"]).await;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("peerpost watch <sender-name>"));
}

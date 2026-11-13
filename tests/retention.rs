//! Ticket 03: the Message log keeps the newest N Peer messages, `peerpost
//! prune` clears it, and unread messages survive a Broker restart.

mod common;

use common::TestEnv;

fn texts(read: &serde_json::Value) -> Vec<String> {
    read["messages"].as_array().unwrap().iter().map(|m| m["text"].as_str().unwrap().to_string()).collect()
}

/// Stop a Broker the test started (as a crash would) and wait until it is gone.
async fn stop(mut broker: tokio::process::Child) {
    broker.kill().await.expect("kill broker");
}

#[tokio::test]
async fn unread_messages_are_still_returned_after_the_broker_stops_and_starts_again() {
    let env = TestEnv::new();
    let broker = env.start_broker().await;
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "read before the restart").await.ok();
    assert_eq!(texts(&bob.read().await.ok()), ["read before the restart"]);
    alice.send("bob", "survives the restart").await.ok();
    alice.close().await;
    bob.close().await;
    stop(broker).await;

    // A new Session finds no Broker and starts a fresh one.
    let bob = env.claude_session(Some("bob")).await;
    assert_eq!(texts(&bob.read().await.ok()), ["survives the restart"], "only the unread message comes back");
}

#[tokio::test]
async fn past_the_set_count_the_oldest_messages_are_dropped_from_the_message_log() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_LOG_KEEP", "3");
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    for n in 1..=10 {
        alice.send("bob", &format!("message {n:02}")).await.ok();
    }

    assert_eq!(texts(&bob.read().await.ok()), ["message 08", "message 09", "message 10"]);
    let on_disk = std::fs::read_to_string(env.message_log()).unwrap();
    for n in 1..=5 {
        assert!(!on_disk.contains(&format!("message {n:02}")), "message {n:02} is still on disk:\n{on_disk}");
    }
}

#[tokio::test]
async fn the_oldest_messages_stay_dropped_after_a_restart() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_LOG_KEEP", "2");
    let broker = env.start_broker().await;
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    for n in 1..=5 {
        alice.send("bob", &format!("message {n}")).await.ok();
    }
    alice.close().await;
    bob.close().await;
    stop(broker).await;

    let bob = env.claude_session(Some("bob")).await;
    assert_eq!(texts(&bob.read().await.ok()), ["message 4", "message 5"]);
}

#[tokio::test]
async fn the_retention_count_must_be_a_positive_whole_number() {
    let mut env = TestEnv::new();
    env.set_var("PEERPOST_LOG_KEEP", "0");
    let out = env.run(&["broker"]).await;
    assert!(!out.status.success(), "a keep count of 0 is refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("PEERPOST_LOG_KEEP"), "the error names the setting: {stderr}");
}

#[tokio::test]
async fn prune_empties_the_message_log_of_a_running_broker_which_keeps_working() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "private one").await.ok();
    alice.send("bob", "private two").await.ok();

    let out = env.run(&["prune"]).await;
    assert!(out.status.success(), "prune failed: {}", String::from_utf8_lossy(&out.stderr));
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("2"), "prune says how many messages it removed: {said}");

    assert_eq!(texts(&bob.read().await.ok()), Vec::<String>::new());
    assert!(!std::fs::read_to_string(env.message_log()).unwrap().contains("private"), "text left on disk");

    alice.send("bob", "after the prune").await.ok();
    assert_eq!(texts(&bob.read().await.ok()), ["after the prune"]);
}

#[tokio::test]
async fn prune_empties_the_message_log_when_no_broker_is_running() {
    let env = TestEnv::new();
    let broker = env.start_broker().await;
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "private").await.ok();
    alice.close().await;
    bob.close().await;
    stop(broker).await;

    let out = env.run(&["prune"]).await;
    assert!(out.status.success(), "prune failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!std::fs::read_to_string(env.message_log()).unwrap().contains("private"), "text left on disk");

    let bob = env.claude_session(Some("bob")).await;
    assert_eq!(texts(&bob.read().await.ok()), Vec::<String>::new(), "pruned messages never come back");
}

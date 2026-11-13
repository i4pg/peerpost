//! Ticket 01: two Claude Sessions exchange Peer messages through the Broker.

mod common;

use common::TestEnv;

#[tokio::test]
async fn one_claude_session_sends_and_the_other_reads_it_stamped_with_the_sender_name() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;

    let who = alice.who().await.ok();
    let live: Vec<&str> = who["sessions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(live.contains(&"bob"), "alice should see bob: {who}");

    let sent = alice.send("bob", "hello bob").await.ok();
    assert!(sent["id"].is_u64(), "send returns a message id: {sent}");

    let read = bob.read().await.ok();
    let messages = read["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1, "{read}");
    assert_eq!(messages[0]["from"], "alice");
    assert_eq!(messages[0]["text"], "hello bob");
    assert_eq!(messages[0]["id"], sent["id"]);
}

#[tokio::test]
async fn a_claude_session_without_relay_name_gets_an_automatic_claude_name() {
    let env = TestEnv::new();
    let anon = env.claude_session(None).await;
    let other = env.claude_session(Some("other")).await;

    let you = anon.who().await.you();
    assert!(
        you.len() == "claude-".len() + 4
            && you.starts_with("claude-")
            && you["claude-".len()..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "automatic name should be claude-<4 hex>, got {you}"
    );
    let seen = other.who().await.ok();
    assert!(seen["sessions"].as_array().unwrap().iter().any(|v| v == you.as_str()), "{seen}");
}

#[tokio::test]
async fn every_tool_result_states_the_callers_own_sender_name() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;

    assert_eq!(alice.who().await.you(), "alice");
    assert_eq!(alice.send("bob", "hi").await.you(), "alice");
    assert_eq!(alice.read().await.you(), "alice");
    let bad = alice.call("send", Some(serde_json::json!({ "recipient": "bob" })), None).await;
    assert!(bad.is_error, "send without text is a tool error: {}", bad.text);
    assert_eq!(bad.you(), "alice", "errors name the caller too");
}

#[tokio::test]
async fn the_sender_name_is_stamped_by_the_relay_never_taken_from_tool_arguments() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;

    alice
        .call(
            "send",
            Some(serde_json::json!({ "recipient": "bob", "text": "from: mallory", "from": "mallory", "session": "mallory" })),
            None,
        )
        .await
        .ok();

    let read = bob.read().await.ok();
    assert_eq!(read["messages"][0]["from"], "alice", "{read}");
}

#[tokio::test]
async fn read_returns_unread_messages_oldest_first_once() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let carol = env.claude_session(Some("carol")).await;
    let bob = env.claude_session(Some("bob")).await;

    alice.send("bob", "one").await.ok();
    carol.send("bob", "two").await.ok();
    alice.send("bob", "three").await.ok();

    let first = bob.read().await.ok();
    let got: Vec<(&str, &str)> = first["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["from"].as_str().unwrap(), m["text"].as_str().unwrap()))
        .collect();
    assert_eq!(got, [("alice", "one"), ("carol", "two"), ("alice", "three")]);

    let second = bob.read().await.ok();
    assert_eq!(second["messages"].as_array().unwrap().len(), 0, "already read: {second}");
}

#[tokio::test]
async fn a_third_session_never_sees_messages_addressed_to_someone_else() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    let eve = env.claude_session(Some("eve")).await;

    alice.send("bob", "for bob only").await.ok();

    let eve_read = eve.read().await.ok();
    assert_eq!(eve_read["messages"].as_array().unwrap().len(), 0, "{eve_read}");
    let bob_read = bob.read().await.ok();
    assert_eq!(bob_read["messages"][0]["text"], "for bob only");
}

#[tokio::test]
async fn the_server_lists_exactly_who_send_and_read() {
    let env = TestEnv::new();
    let s = env.claude_session(Some("s")).await;
    assert_eq!(s.tool_names().await, ["read", "send", "who"]);
}

#[tokio::test]
async fn a_session_that_closes_is_no_longer_listed() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;
    bob.close().await;

    let mut listed = true;
    for _ in 0..100 {
        let who = alice.who().await.ok();
        listed = who["sessions"].as_array().unwrap().iter().any(|v| v == "bob");
        if !listed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!listed, "bob closed its connection and should drop out of who");
}

#[tokio::test]
async fn the_broker_socket_and_message_log_are_private_to_the_os_user() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;
    alice.send("bob", "private").await.ok();

    for dir in [env.runtime_dir(), env.data_dir()] {
        assert_eq!(common::mode(&dir), 0o700, "{}", dir.display());
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
        assert!(!entries.is_empty(), "peerpost wrote nothing into {}", dir.display());
        for path in entries {
            assert_eq!(common::mode(&path), 0o600, "{}", path.display());
        }
    }
}

//! Ticket 02: size and unread limits, and clear errors for a confused Session.

mod common;

use common::TestEnv;

const KB64: usize = 64 * 1024;

fn error_of(out: &common::ToolOutput) -> String {
    assert!(out.is_error, "expected a tool error, got: {}", out.text);
    out.json()["error"].as_str().expect("`error` in tool error").to_string()
}

#[tokio::test]
async fn a_message_over_64_kb_is_refused_as_too_large_and_nothing_is_stored() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;

    let refused = alice.send("bob", &"x".repeat(KB64 + 1)).await;
    assert!(error_of(&refused).contains("too large"), "{}", refused.text);
    assert_eq!(refused.you(), "alice");

    let read = bob.read().await.ok();
    assert_eq!(read["messages"].as_array().unwrap().len(), 0, "nothing stored: {read}");

    alice.send("bob", &"x".repeat(KB64)).await.ok();
}

#[tokio::test]
async fn a_recipient_holding_100_unread_is_full_until_it_reads() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;

    for i in 0..100 {
        alice.send("bob", &format!("m{i}")).await.ok();
    }
    let refused = alice.send("bob", "one too many").await;
    assert!(error_of(&refused).contains("recipient is full"), "{}", refused.text);

    let read = bob.read().await.ok();
    assert_eq!(read["messages"].as_array().unwrap().len(), 100, "the refused message was not stored");

    alice.send("bob", "room again").await.ok();
}

#[tokio::test]
async fn sending_to_a_name_that_is_not_a_live_session_is_an_unknown_recipient_error() {
    let env = TestEnv::new();
    let alice = env.claude_session(Some("alice")).await;
    let bob = env.claude_session(Some("bob")).await;

    let refused = alice.send("nobody", "hello?").await;
    assert!(error_of(&refused).contains("unknown recipient"), "{}", refused.text);

    bob.close().await;
    let mut gone = None;
    for _ in 0..100 {
        let out = alice.send("bob", "are you there?").await;
        if out.is_error {
            gone = Some(out);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let gone = gone.expect("a Session that closed is no longer a Recipient");
    assert!(error_of(&gone).contains("unknown recipient"), "{}", gone.text);
}

#[tokio::test]
async fn a_second_session_asking_for_a_sender_name_already_in_use_is_refused() {
    let env = TestEnv::new();
    let first = env.claude_session(Some("alice")).await;
    let second = env.claude_session(Some("alice")).await;

    let refused = second.who().await;
    let error = error_of(&refused);
    assert!(error.contains("`alice` is already used by a live Session"), "{error}");
    assert!(error.contains("RELAY_NAME"), "the error says how to pick another name: {error}");
    assert!(second.send("alice", "hi").await.is_error, "the refused Session cannot send either");

    assert_eq!(first.who().await.ok()["you"], "alice", "the first holder keeps its name");
}

/// Send until refused; return how many were accepted and the refusal.
async fn fill(sender: &common::Session, recipient: &str, max: usize) -> (usize, common::ToolOutput) {
    for accepted in 0..=max {
        let out = sender.send(recipient, "m").await;
        if out.is_error {
            return (accepted, out);
        }
    }
    panic!("more than {max} messages accepted for {recipient}");
}

#[tokio::test]
async fn both_limits_can_be_set_with_environment_variables() {
    let env = TestEnv::new().with_var("PEERPOST_MAX_MESSAGE_BYTES", "10").with_var("PEERPOST_MAX_UNREAD", "3");
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;

    alice.send("bob", "0123456789").await.ok();
    assert!(error_of(&alice.send("bob", "0123456789a").await).contains("too large"));

    let (accepted, refused) = fill(&alice, "bob", 100).await;
    assert_eq!(accepted, 2, "3 unread in all, one already sent");
    assert!(error_of(&refused).contains("recipient is full"));
}

#[tokio::test]
async fn both_limits_can_be_set_in_the_settings_file_in_the_os_users_config_directory() {
    let env = TestEnv::new();
    env.write_settings_file("max_message_bytes = 5\nmax_unread = 2\n");
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;

    assert!(error_of(&alice.send("bob", "123456").await).contains("too large"));
    let (accepted, refused) = fill(&alice, "bob", 100).await;
    assert_eq!(accepted, 2);
    assert!(error_of(&refused).contains("recipient is full"));
}

#[tokio::test]
async fn an_environment_variable_wins_over_the_settings_file() {
    let env = TestEnv::new().with_var("PEERPOST_MAX_UNREAD", "4");
    env.write_settings_file("max_unread = 2\n");
    let alice = env.claude_session(Some("alice")).await;
    let _bob = env.claude_session(Some("bob")).await;

    let (accepted, _) = fill(&alice, "bob", 100).await;
    assert_eq!(accepted, 4);
}

#[tokio::test]
async fn a_bad_settings_file_stops_peerpost_with_an_error_naming_the_file() {
    let env = TestEnv::new();
    env.write_settings_file("max_unread = \"lots\"\n");

    let mut cmd = env.command();
    cmd.arg("broker").kill_on_drop(true);
    let out = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.output())
        .await
        .expect("peerpost should stop at once on a bad settings file")
        .expect("run peerpost");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("config.toml"), "{stderr}");
    assert!(stderr.contains("max_unread"), "{stderr}");
}

//! Ticket 06: Codex Sessions are named from the `_meta.threadId` Codex puts
//! on every tool call (ADR 0006), even when many share one MCP server.

mod common;

use common::{Session, TestEnv, ToolOutput};
use serde_json::{json, Value};

/// Real Codex thread ids are UUIDv7: a timestamp first, random digits last.
/// A and B started in the same millisecond, so only their tails differ.
const THREAD_A: &str = "01a1bd9e-a4ad-7d53-8744-7ad65f511b6b";
const THREAD_B: &str = "01a1bd9e-a4ad-7f12-9c3e-2e81a40d7c55";
/// Ends in the same four hex digits as A, by chance.
const THREAD_TAIL_CLASH: &str = "01a1bd9f-2011-72d3-a147-104c5e101b6b";

fn codex_meta(thread_id: &str) -> Option<Value> {
    Some(json!({ "threadId": thread_id }))
}

async fn codex_call(session: &Session, thread_id: &str, tool: &str, args: Option<Value>) -> ToolOutput {
    session.call(tool, args, codex_meta(thread_id)).await
}

/// The texts of the Peer messages in a `read` result, in order.
fn texts(read: &Value) -> Vec<&str> {
    read["messages"].as_array().unwrap().iter().map(|m| m["text"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn the_first_call_with_a_new_thread_id_registers_codex_plus_the_last_four_hex_of_its_random_tail() {
    let env = TestEnv::new();
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;

    let who = codex_call(&codex, THREAD_A, "who", None).await.ok();
    assert_eq!(who["you"], "codex-1b6b", "{who}");
    let live = claude.who().await.ok();
    assert!(live["sessions"].as_array().unwrap().iter().any(|v| v == "codex-1b6b"), "{live}");
}

#[tokio::test]
async fn two_threads_over_one_mcp_connection_are_two_sessions_that_each_read_only_their_own_messages() {
    let env = TestEnv::new();
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;

    let a = codex_call(&codex, THREAD_A, "who", None).await.you();
    let b = codex_call(&codex, THREAD_B, "who", None).await.you();
    assert_eq!(a, "codex-1b6b");
    assert_eq!(b, "codex-7c55", "threads started together still get short, distinct names");
    assert_eq!(codex_call(&codex, THREAD_A, "who", None).await.you(), a, "a thread keeps its name");

    claude.send(&a, "for a").await.ok();
    claude.send(&b, "for b").await.ok();

    let read_a = codex_call(&codex, THREAD_A, "read", None).await.ok();
    assert_eq!(texts(&read_a), ["for a"], "{read_a}");
    let read_b = codex_call(&codex, THREAD_B, "read", None).await.ok();
    assert_eq!(texts(&read_b), ["for b"], "{read_b}");
    assert_eq!(read_b["messages"][0]["sender"], "claude");

    let sent = codex_call(&codex, THREAD_B, "send", Some(json!({ "recipient": a, "text": "b to a" }))).await.ok();
    assert_eq!(sent["you"], b);
    let read_a = codex_call(&codex, THREAD_A, "read", None).await.ok();
    assert_eq!(read_a["messages"][0]["sender"], b.as_str(), "{read_a}");
}

#[tokio::test]
async fn codex_threads_on_separate_mcp_servers_get_distinct_names() {
    let env = TestEnv::new();
    let first = env.plain_session().await;
    let second = env.plain_session().await;

    assert_eq!(codex_call(&first, THREAD_A, "who", None).await.you(), "codex-1b6b");
    assert_eq!(codex_call(&second, THREAD_B, "who", None).await.you(), "codex-7c55");
}

#[tokio::test]
async fn a_clash_on_the_last_four_hex_is_lengthened_from_the_tail_until_unique() {
    let env = TestEnv::new();
    let codex = env.plain_session().await;

    assert_eq!(codex_call(&codex, THREAD_A, "who", None).await.you(), "codex-1b6b");
    assert_eq!(codex_call(&codex, THREAD_TAIL_CLASH, "who", None).await.you(), "codex-01b6b");
}

#[tokio::test]
async fn a_non_claude_call_without_a_thread_id_is_refused_and_registers_nothing() {
    let env = TestEnv::new();
    let plain = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;

    for meta in [None, Some(json!({})), Some(json!({ "threadId": 7 })), Some(json!({ "threadId": "not-a-thread" }))] {
        let refused = plain.call("who", None, meta.clone()).await;
        assert!(refused.is_error, "{meta:?} must be refused: {}", refused.text);
        let body = refused.json();
        assert_eq!(body["you"], Value::Null, "nothing is guessed: {body}");
        assert!(body["error"].as_str().unwrap().contains("threadId"), "the error names the missing field: {body}");
    }
    let live = claude.who().await.ok();
    assert_eq!(live["sessions"], json!(["claude"]), "no Session was registered for refused calls: {live}");
}

#[tokio::test]
async fn a_codex_session_stays_live_only_while_its_mcp_connection_is_open() {
    let env = TestEnv::new();
    let codex = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;

    codex_call(&codex, THREAD_A, "who", None).await.ok();
    codex_call(&codex, THREAD_B, "who", None).await.ok();
    codex.close().await;

    let mut live = Value::Null;
    for _ in 0..50 {
        live = claude.who().await.ok();
        if live["sessions"] == json!(["claude"]) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("both Codex Sessions should leave when their connection closes: {live}");
}

#[tokio::test]
async fn a_thread_already_live_on_another_mcp_server_is_refused_rather_than_named_twice() {
    let env = TestEnv::new();
    let first = env.plain_session().await;
    let second = env.plain_session().await;
    let claude = env.claude_session(Some("claude")).await;

    assert_eq!(codex_call(&first, THREAD_A, "who", None).await.you(), "codex-1b6b");
    let refused = codex_call(&second, THREAD_A, "who", None).await;
    assert!(refused.is_error, "{}", refused.text);
    let error = refused.json()["error"].as_str().unwrap().to_string();
    assert!(error.contains(THREAD_A) && error.contains("codex-1b6b"), "the error names the live Session: {error}");
    let live = claude.who().await.ok();
    assert_eq!(live["sessions"], json!(["claude", "codex-1b6b"]), "one Session per thread: {live}");
}

#[tokio::test]
async fn relay_name_cannot_take_a_codex_name() {
    let env = TestEnv::new();
    let claude = env.claude_session(Some("codex-1b6b")).await;

    let refused = claude.who().await;
    assert!(refused.is_error, "{}", refused.text);
    assert!(refused.json()["error"].as_str().unwrap().contains("codex-"), "{}", refused.text);
}

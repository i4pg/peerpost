//! Ticket 06: pins `_meta.threadId` (ADR 0006) to the Codex versions peerpost
//! is tested against. Each supported version has a fixture in
//! `tests/fixtures/codex/<version>.json` recording the `tools/call` shape that
//! version sends, with the Codex source location that defines it. The README
//! must list exactly these versions.
//!
//! The fixtures are recorded by hand from Codex's source. To check a Codex
//! source checkout directly, set `PEERPOST_CODEX_SOURCE` to its root and run
//! `cargo test --test codex_compat`; without it that one check is skipped.

mod common;

use std::path::{Path, PathBuf};

use common::TestEnv;
use serde_json::Value;

/// Where Codex defines the per-call `_meta` it puts on MCP tool calls.
const CODEX_META_SOURCE: &str = "codex-rs/core/src/mcp_tool_call.rs";

fn fixtures() -> Vec<(String, Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex");
    let mut found: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .map(|entry| {
            let path = entry.expect("dir entry").path();
            let version = path.file_stem().unwrap().to_str().unwrap().to_string();
            let fixture = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).expect("fixture JSON");
            (version, fixture)
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!found.is_empty(), "at least one supported Codex version");
    found
}

#[tokio::test]
async fn every_supported_codex_versions_recorded_tool_call_registers_a_codex_session() {
    for (version, fixture) in fixtures() {
        assert_eq!(fixture["codexVersion"], version.as_str(), "fixture file name matches its version");
        assert!(
            fixture["source"].as_str().is_some_and(|s| s.contains(CODEX_META_SOURCE)),
            "{version}: cite the source"
        );
        let params = &fixture["toolCall"]["params"];
        let meta = params["_meta"].clone();
        let thread =
            params["_meta"]["threadId"].as_str().unwrap_or_else(|| panic!("Codex {version} no longer sends threadId"));

        let env = TestEnv::new();
        let codex = env.plain_session().await;
        let out = codex.call(params["name"].as_str().unwrap(), Some(params["arguments"].clone()), Some(meta)).await;
        let expected = format!("codex-{}", &thread[..4]);
        assert_eq!(out.ok()["you"], expected.as_str(), "Codex {version}");
    }
}

#[test]
fn the_readme_lists_exactly_the_tested_codex_versions_and_labels_the_field_undocumented() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md")).unwrap();
    let line = readme
        .lines()
        .find(|l| l.starts_with("Tested Codex CLI versions:"))
        .expect("README has a `Tested Codex CLI versions:` line");
    let mut listed: Vec<&str> = line["Tested Codex CLI versions:".len()..]
        .split(',')
        .map(|v| v.trim().trim_matches('`').trim_end_matches('.'))
        .collect();
    listed.sort();
    let tested: Vec<String> = fixtures().into_iter().map(|(v, _)| v).collect();
    assert_eq!(listed, tested, "README versions must match tests/fixtures/codex");
    assert!(readme.contains("`_meta.threadId`") && readme.contains("undocumented"), "README labels the field");
}

/// Opt-in: checks a Codex source checkout still defines the field.
#[test]
fn a_codex_source_checkout_still_sets_thread_id_on_tool_calls() {
    let Some(root) = std::env::var_os("PEERPOST_CODEX_SOURCE").map(PathBuf::from) else {
        eprintln!("skipped: set PEERPOST_CODEX_SOURCE to a Codex source checkout to run this check");
        return;
    };
    let file = root.join(CODEX_META_SOURCE);
    let source = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
    assert!(source.contains("\"threadId\""), "{} no longer sets `threadId`: see ADR 0006", file.display());
}

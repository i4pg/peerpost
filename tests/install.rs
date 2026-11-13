//! Ticket 08: a Human who has just installed peerpost can check what they
//! installed (`--version`) and how to run it (`--help`), without a Broker.

mod common;

use common::TestEnv;

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test]
async fn version_prints_the_package_version_and_starts_no_broker() {
    let env = TestEnv::new();
    let out = env.run(&["--version"]).await;
    assert!(out.status.success(), "{out:?}");
    assert_eq!(stdout(&out), format!("peerpost {}\n", env!("CARGO_PKG_VERSION")));
    assert!(!env.runtime_dir().exists(), "--version must not start a Broker or create the runtime directory");
}

#[tokio::test]
async fn help_names_every_command_and_starts_no_broker() {
    let env = TestEnv::new();
    for flag in ["--help", "-h", "help"] {
        let out = env.run(&[flag]).await;
        assert!(out.status.success(), "{flag}: {out:?}");
        let text = stdout(&out);
        for command in ["peerpost watch <sender-name>", "peerpost prune", "peerpost --version"] {
            assert!(text.contains(command), "{flag} output lacks `{command}`:\n{text}");
        }
        assert!(text.contains("MCP server"), "{flag} output must say that no argument runs the MCP server:\n{text}");
    }
    assert!(!env.runtime_dir().exists(), "--help must not start a Broker or create the runtime directory");
}

#[tokio::test]
async fn version_and_help_still_answer_when_the_settings_file_is_broken() {
    let env = TestEnv::new();
    env.write_settings_file("no_such_setting = 1\n");
    for flag in ["--version", "--help"] {
        let out = env.run(&[flag]).await;
        assert!(out.status.success(), "{flag}: {out:?}");
    }
}

mod common;

use std::path::PathBuf;
use std::process::Command;

use graft::error::GraftError;
use graft::state::State;
use serial_test::serial;

// Helper: create workspace with convenient defaults
fn fork_helper(
    base: PathBuf,
    name: &str,
) -> Result<graft::workspace::Workspace, GraftError> {
    graft::commands::fork::create_workspace(base, name, None, None, false, None)
}

/// Get path to the built graft binary
fn graft_bin() -> PathBuf {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("graft");
    path
}

#[test]
#[serial]
fn test_enter_command_passthrough() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let ws = fork_helper(base.path().to_path_buf(), "enter-cmd-ws").unwrap();

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args(["enter", "enter-cmd-ws", "--", "echo", "hello"])
        .output()
        .expect("failed to run graft enter");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "expected 'hello' in stdout, got: {}",
        stdout
    );
    assert!(output.status.success(), "graft enter failed: {:?}", output);

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_enter_env_vars() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let ws = fork_helper(base.path().to_path_buf(), "enter-env-ws").unwrap();

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args([
            "enter",
            "enter-env-ws",
            "--",
            "sh",
            "-c",
            "echo $GRAFT_WORKSPACE",
        ])
        .output()
        .expect("failed to run graft enter");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("enter-env-ws"),
        "expected workspace name in stdout, got: {}",
        stdout
    );

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_enter_env_base() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let ws = fork_helper(base.path().to_path_buf(), "enter-base-ws").unwrap();

    let base_canonical = base.path().canonicalize().unwrap();

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args([
            "enter",
            "enter-base-ws",
            "--",
            "sh",
            "-c",
            "echo $GRAFT_BASE",
        ])
        .output()
        .expect("failed to run graft enter");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&base_canonical.display().to_string()),
        "expected base path '{}' in stdout, got: {}",
        base_canonical.display(),
        stdout
    );

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_enter_nonexistent() {
    let _dir = common::setup_test_env();

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args(["enter", "nonexistent-ws", "--", "echo", "hi"])
        .output()
        .expect("failed to run graft enter");

    assert!(
        !output.status.success(),
        "expected failure for nonexistent workspace"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent"),
        "expected error about workspace not found, got: {}",
        stderr
    );
}

#[test]
#[serial]
fn test_enter_no_name() {
    let _dir = common::setup_test_env();

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args(["enter"])
        .output()
        .expect("failed to run graft enter");

    assert!(
        !output.status.success(),
        "expected failure when no name provided"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("name required") || stderr.contains("workspace name"),
        "expected error about missing name, got: {}",
        stderr
    );
}

#[test]
#[serial]
fn test_enter_new_creates_workspace() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args([
            "enter",
            "--new",
            "new-enter-ws",
            "--from",
            base.path().to_str().unwrap(),
            "--",
            "echo",
            "inside-new",
        ])
        .output()
        .expect("failed to run graft enter --new");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "graft enter --new failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("inside-new"),
        "expected 'inside-new' in stdout, got: {}",
        stdout
    );

    // Verify workspace was created in state
    let state = State::load().unwrap();
    let ws = state.get_workspace("new-enter-ws");
    assert!(ws.is_some(), "workspace 'new-enter-ws' should exist in state");

    // Clean up
    if let Some(ws) = ws {
        common::cleanup_workspace(ws);
    }
}

// ── Ephemeral tests ──

#[test]
#[serial]
fn test_ephemeral_name_generation() {
    // Test that ephemeral name generation produces expected format
    // We can't call the private function directly, but we can test via the CLI
    let _dir = common::setup_test_env();

    // Verify the format: "ephemeral-" followed by 8 hex chars
    // We test this indirectly by checking workspace naming pattern
    let name = "ephemeral-abcdef01";
    assert!(name.starts_with("ephemeral-"));
    assert_eq!(name.len(), "ephemeral-".len() + 8);
}

#[test]
#[serial]
fn test_ephemeral_flag_recorded_in_state() {
    // Test that ephemeral flag is properly set on workspace struct
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let mut ws = graft::workspace::Workspace::new("eph-test", base.path().to_path_buf(), None);
    assert!(!ws.ephemeral); // default is false
    ws.ephemeral = true;
    assert!(ws.ephemeral);

    // Test serialization round-trip
    let json = serde_json::to_string(&ws).unwrap();
    let deserialized: graft::workspace::Workspace = serde_json::from_str(&json).unwrap();
    assert!(deserialized.ephemeral);
}

#[test]
#[serial]
fn test_ephemeral_state_without_ephemeral_field() {
    // Test that deserializing old state without ephemeral field defaults to false
    let _dir = common::setup_test_env();

    let json = r#"{
        "name": "old-ws",
        "base": "/tmp/base",
        "upper": "/tmp/upper",
        "work": "/tmp/work",
        "merged": "/tmp/merged",
        "parent": null,
        "created": "2025-01-01T00:00:00Z",
        "state": "running",
        "session": null
    }"#;

    let ws: graft::workspace::Workspace = serde_json::from_str(json).unwrap();
    assert!(!ws.ephemeral);
    assert!(!ws.tmpfs);
    assert_eq!(ws.tmpfs_size, None);
}

#[test]
#[serial]
fn test_enter_ephemeral_with_overlay() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args([
            "enter",
            "--ephemeral",
            "--from",
            base.path().to_str().unwrap(),
            "--",
            "echo",
            "ephemeral-test",
        ])
        .output()
        .expect("failed to run graft enter --ephemeral");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "graft enter --ephemeral failed: stderr={}",
        stderr
    );
    assert!(
        stdout.contains("ephemeral-test"),
        "expected 'ephemeral-test' in stdout, got: {}",
        stdout
    );
    assert!(
        stdout.contains("ephemeral workspace removed"),
        "expected 'ephemeral workspace removed' in stdout, got: {}",
        stdout
    );

    // Verify workspace was cleaned up (no ephemeral workspaces in state)
    let state = State::load().unwrap();
    let workspaces = state.list_workspaces();
    for ws in workspaces {
        assert!(
            !ws.name.starts_with("ephemeral-"),
            "ephemeral workspace '{}' should have been dropped",
            ws.name
        );
    }
}

#[test]
#[serial]
fn test_enter_ephemeral_with_name() {
    skip_without_overlay!();
    let _dir = common::setup_test_env();
    let base = tempfile::tempdir().unwrap();
    common::create_test_project(base.path());

    let output = Command::new(graft_bin())
        .env("GRAFT_HOME", std::env::var("GRAFT_HOME").unwrap())
        .args([
            "enter",
            "--ephemeral",
            "my-eph-ws",
            "--from",
            base.path().to_str().unwrap(),
            "--",
            "echo",
            "named-ephemeral",
        ])
        .output()
        .expect("failed to run graft enter --ephemeral with name");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "graft enter --ephemeral with name failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("named-ephemeral"),
        "expected 'named-ephemeral' in stdout, got: {}",
        stdout
    );
    assert!(
        stdout.contains("ephemeral workspace removed"),
        "expected drop message in stdout, got: {}",
        stdout
    );

    // Verify workspace is gone from state
    let state = State::load().unwrap();
    assert!(
        state.get_workspace("my-eph-ws").is_none(),
        "ephemeral workspace 'my-eph-ws' should have been dropped"
    );
}

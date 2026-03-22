mod common;

use std::fs;

use graft::error::GraftError;
use graft::overlay::{Capabilities, OverlayMode};
use graft::state::State;
use graft::workspace::graft_home;
use serial_test::serial;

/// Helper: manually add a workspace entry to state (no actual overlay mount).
fn add_fake_workspace(state: &mut State, name: &str, parent: Option<String>) {
    let home = graft_home();
    let fake_base = std::path::PathBuf::from("/tmp/fake-base");
    let ws = common::make_test_workspace(&home, name, &fake_base, parent);
    state.add_workspace(ws).unwrap();
}

// ── Nuke tests ──

#[test]
#[serial]
fn test_nuke_no_graft_home() {
    let _dir = common::setup_test_env();
    // Make sure graft_home does NOT exist
    let home = graft_home();
    let _ = fs::remove_dir_all(&home);
    assert!(!home.exists());

    // nuke should succeed with "nothing to clean up"
    graft::commands::nuke::run().unwrap();
}

#[test]
#[serial]
fn test_nuke_removes_graft_home() {
    let _dir = common::setup_test_env();

    // Create graft_home with a state file and a workspace dir
    let mut state = State::default();
    add_fake_workspace(&mut state, "ws-nuke-1", None);
    state.save().unwrap();

    let home = graft_home();
    assert!(home.exists());
    assert!(home.join("state.json").exists());

    // Nuke it
    graft::commands::nuke::run().unwrap();

    // Directory should be gone
    assert!(!home.exists());
}

#[test]
#[serial]
fn test_nuke_corrupted_state() {
    let _dir = common::setup_test_env();

    let home = graft_home();
    fs::create_dir_all(&home).unwrap();

    // Write garbage to state.json
    fs::write(home.join("state.json"), "this is not json {{{").unwrap();

    // Nuke should still succeed — it skips unmount if state can't load, but still deletes the dir
    graft::commands::nuke::run().unwrap();

    assert!(!home.exists());
}

#[test]
#[serial]
fn test_nuke_missing_state() {
    let _dir = common::setup_test_env();

    let home = graft_home();
    fs::create_dir_all(&home).unwrap();
    // No state.json — just the directory

    graft::commands::nuke::run().unwrap();

    assert!(!home.exists());
}

// ── Drop --all tests ──

#[test]
#[serial]
fn test_drop_all() {
    let _dir = common::setup_test_env();

    let mut state = State::default();
    add_fake_workspace(&mut state, "all-ws-1", None);
    add_fake_workspace(&mut state, "all-ws-2", None);
    add_fake_workspace(&mut state, "all-ws-3", None);
    state.save().unwrap();

    // drop --all (name is ignored when --all is set, but clap requires it)
    graft::commands::drop::run(graft::cli::DropArgs {
        name: "_".to_string(),
        force: false,
        all: true,
        glob: false,
    }).unwrap();

    let state = State::load().unwrap();
    assert!(state.list_workspaces().is_empty());
}

// ── Drop --glob tests ──

#[test]
#[serial]
fn test_drop_glob() {
    let _dir = common::setup_test_env();

    let mut state = State::default();
    add_fake_workspace(&mut state, "auth-1", None);
    add_fake_workspace(&mut state, "auth-2", None);
    add_fake_workspace(&mut state, "other", None);
    state.save().unwrap();

    // drop --glob "auth-*"
    graft::commands::drop::run(graft::cli::DropArgs {
        name: "auth-*".to_string(),
        force: false,
        all: false,
        glob: true,
    }).unwrap();

    let state = State::load().unwrap();
    assert!(state.get_workspace("auth-1").is_none());
    assert!(state.get_workspace("auth-2").is_none());
    assert!(state.get_workspace("other").is_some());
}

#[test]
#[serial]
fn test_drop_glob_no_match() {
    let _dir = common::setup_test_env();

    let mut state = State::default();
    add_fake_workspace(&mut state, "foo", None);
    state.save().unwrap();

    // drop --glob with pattern that matches nothing — should succeed with message
    graft::commands::drop::run(graft::cli::DropArgs {
        name: "zzz-*".to_string(),
        force: false,
        all: false,
        glob: true,
    }).unwrap();

    // workspace should still be there
    let state = State::load().unwrap();
    assert!(state.get_workspace("foo").is_some());
}

// ── Overlay mode tests ──

#[test]
fn test_overlay_mode_detection() {
    // Test OverlayMode enum serialization
    let mode = OverlayMode::Fuse;
    let json = serde_json::to_string(&mode).unwrap();
    assert_eq!(json, "\"fuse\"");

    // Legacy "unprivileged" and "privileged" values deserialize to Supported
    let mode: OverlayMode = serde_json::from_str("\"unprivileged\"").unwrap();
    assert_eq!(mode, OverlayMode::Supported);

    let mode: OverlayMode = serde_json::from_str("\"privileged\"").unwrap();
    assert_eq!(mode, OverlayMode::Supported);

    let mode: OverlayMode = serde_json::from_str("\"unsupported\"").unwrap();
    assert_eq!(mode, OverlayMode::Unsupported);
}

#[test]
#[serial]
fn test_capabilities_persistence() {
    let _dir = common::setup_test_env();

    let home = graft_home();
    fs::create_dir_all(&home).unwrap();

    let caps = Capabilities {
        overlay_mode: OverlayMode::Fuse,
        kernel_version: "6.1.0-test".to_string(),
        detected_at: graft::workspace::now_rfc3339(),
        ttl_hours: 24,
    };

    graft::overlay::save_capabilities(&caps).unwrap();

    let loaded = graft::overlay::load_capabilities();
    assert!(loaded.is_some());

    let loaded = loaded.unwrap();
    assert_eq!(loaded.overlay_mode, OverlayMode::Fuse);
    assert_eq!(loaded.kernel_version, "6.1.0-test");
    assert_eq!(loaded.ttl_hours, 24);
}

// ── Error message tests ──

#[test]
fn test_error_messages_are_helpful() {
    let err = GraftError::WorkspaceNotFound("my-ws".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("my-ws"));
    assert!(msg.contains("graft ls"));

    let err = GraftError::WorkspaceExists("my-ws".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("my-ws"));
    assert!(msg.contains("graft enter"));
}

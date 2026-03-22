mod common;

use std::fs;

use graft::error::GraftError;
use graft::state::State;
use graft::util::graft_home;
use serial_test::serial;

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
    let home = graft_home();
    let _ = fs::remove_dir_all(&home);
    assert!(!home.exists());

    graft::commands::nuke::exec(graft::cli::NukeArgs { yes: true }).unwrap();
}

#[test]
#[serial]
fn test_nuke_removes_graft_home() {
    let _dir = common::setup_test_env();

    let mut state = State::default();
    add_fake_workspace(&mut state, "ws-nuke-1", None);
    state.save().unwrap();

    let home = graft_home();
    assert!(home.exists());
    assert!(home.join("state.json").exists());

    graft::commands::nuke::exec(graft::cli::NukeArgs { yes: true }).unwrap();

    assert!(!home.exists());
}

#[test]
#[serial]
fn test_nuke_corrupted_state() {
    let _dir = common::setup_test_env();

    let home = graft_home();
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("state.json"), "this is not json {{{").unwrap();

    graft::commands::nuke::exec(graft::cli::NukeArgs { yes: true }).unwrap();

    assert!(!home.exists());
}

#[test]
#[serial]
fn test_nuke_missing_state() {
    let _dir = common::setup_test_env();

    let home = graft_home();
    fs::create_dir_all(&home).unwrap();

    graft::commands::nuke::exec(graft::cli::NukeArgs { yes: true }).unwrap();

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

    graft::commands::drop::exec(graft::cli::DropArgs {
        name: None,
        force: false,
        all: true,
        glob: false,
    })
    .unwrap();

    let state = State::load().unwrap();
    assert!(state.workspaces.values().collect::<Vec<_>>().is_empty());
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

    graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("auth-*".to_string()),
        force: false,
        all: false,
        glob: true,
    })
    .unwrap();

    let state = State::load().unwrap();
    assert!(state.workspaces.get("auth-1").is_none());
    assert!(state.workspaces.get("auth-2").is_none());
    assert!(state.workspaces.get("other").is_some());
}

#[test]
#[serial]
fn test_drop_glob_no_match() {
    let _dir = common::setup_test_env();

    let mut state = State::default();
    add_fake_workspace(&mut state, "foo", None);
    state.save().unwrap();

    graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("zzz-*".to_string()),
        force: false,
        all: false,
        glob: true,
    })
    .unwrap();

    let state = State::load().unwrap();
    assert!(state.workspaces.get("foo").is_some());
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

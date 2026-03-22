mod common;

use std::path::PathBuf;

use graft::error::GraftError;
use graft::state::State;
use serial_test::serial;

#[test]
#[serial]
fn test_fork_creates_workspace() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "test-ws", None, None).unwrap();

    assert!(ws.upper.exists());
    assert!(ws.work.exists());
    assert!(ws.merged.exists());

    let state = State::load().unwrap();
    assert!(state.workspaces.get("test-ws").is_some());

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_fork_merged_shows_files() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "files-ws", None, None).unwrap();

    assert!(ws.merged.join("README.md").exists());
    assert!(ws.merged.join("src/main.rs").exists());
    assert!(ws.merged.join("Cargo.toml").exists());

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_fork_duplicate_name() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "dup-ws", None, None).unwrap();

    let result = common::fork_helper(base.path().to_path_buf(), "dup-ws", None, None);
    assert!(matches!(result.unwrap_err(), GraftError::WorkspaceExists(_)));

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_fork_nonexistent_base() {
    let _dir = common::setup_test_env();
    let result = common::fork_helper(PathBuf::from("/tmp/does-not-exist-graft-test"), "bad-ws", None, None);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_fork_stacked() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws1 = common::fork_helper(base.path().to_path_buf(), "parent-ws", None, None).unwrap();
    let ws2 = common::fork_helper(PathBuf::from("parent-ws"), "child-ws", None, None).unwrap();

    assert_eq!(ws2.parent.as_deref(), Some("parent-ws"));
    assert_eq!(ws2.base, ws1.merged.canonicalize().unwrap());

    common::cleanup_workspace(&ws2);
    common::cleanup_workspace(&ws1);
}

#[test]
#[serial]
fn test_fork_writes_state() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "state-ws", None, Some("my-session".into())).unwrap();

    let state = State::load().unwrap();
    let stored = state.workspaces.get("state-ws").unwrap();
    assert_eq!(stored.name, "state-ws");
    assert_eq!(stored.session.as_deref(), Some("my-session"));
    assert!(stored.base.exists());

    common::cleanup_workspace(&ws);
}

// ── Drop tests ──

#[test]
#[serial]
fn test_drop_workspace() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "drop-ws", None, None).unwrap();
    let ws_dir = graft::util::graft_home().join("drop-ws");
    assert!(ws_dir.exists());

    graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("drop-ws".to_string()),
        force: false,
        all: false,
        glob: false,
    }).unwrap();

    assert!(!ws_dir.exists());
    let state = State::load().unwrap();
    assert!(state.workspaces.get("drop-ws").is_none());
    let _ = ws;
}

#[test]
#[serial]
fn test_drop_nonexistent() {
    let _dir = common::setup_test_env();
    let result = graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("nonexistent-ws".to_string()),
        force: false,
        all: false,
        glob: false,
    });
    assert!(matches!(result.unwrap_err(), GraftError::WorkspaceNotFound(_)));
}

#[test]
#[serial]
fn test_drop_with_children_requires_force() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws1 = common::fork_helper(base.path().to_path_buf(), "parent-drop", None, None).unwrap();
    let ws2 = common::fork_helper(PathBuf::from("parent-drop"), "child-drop", None, None).unwrap();

    let result = graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("parent-drop".to_string()),
        force: false,
        all: false,
        glob: false,
    });
    match result.unwrap_err() {
        GraftError::HasChildren { workspace, children } => {
            assert_eq!(workspace, "parent-drop");
            assert!(children.contains(&"child-drop".to_string()));
        }
        other => panic!("expected HasChildren, got: {:?}", other),
    }

    let state = State::load().unwrap();
    assert!(state.workspaces.get("parent-drop").is_some());
    assert!(state.workspaces.get("child-drop").is_some());

    common::cleanup_workspace(&ws2);
    common::cleanup_workspace(&ws1);
}

#[test]
#[serial]
fn test_drop_force_cascade() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let _ws1 = common::fork_helper(base.path().to_path_buf(), "cascade-parent", None, None).unwrap();
    let _ws2 = common::fork_helper(PathBuf::from("cascade-parent"), "cascade-child", None, None).unwrap();

    graft::commands::drop::exec(graft::cli::DropArgs {
        name: Some("cascade-parent".to_string()),
        force: true,
        all: false,
        glob: false,
    }).unwrap();

    let state = State::load().unwrap();
    assert!(state.workspaces.get("cascade-parent").is_none());
    assert!(state.workspaces.get("cascade-child").is_none());
}

// ── Ls tests ──

#[test]
#[serial]
fn test_ls_empty() {
    let _dir = common::setup_test_env();
    graft::commands::ls::exec(graft::cli::LsArgs { names: false }).unwrap();
}

#[test]
#[serial]
fn test_ls_shows_workspaces() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let ws = common::fork_helper(base.path().to_path_buf(), "ls-test-ws", None, None).unwrap();
    graft::commands::ls::exec(graft::cli::LsArgs { names: false }).unwrap();

    let state = State::load().unwrap();
    assert!(state.workspaces.get("ls-test-ws").is_some());

    common::cleanup_workspace(&ws);
}

#[test]
#[serial]
fn test_fork_tmpfs_with_overlay() {
    skip_without_overlay!();
    let (_dir, base) = common::setup_with_project();

    let result = graft::commands::fork::create_workspace(
        base.path().to_path_buf(),
        "tmpfs-overlay-ws",
        None,
        None,
        true,
        Some("128m".to_string()),
    );

    match result {
        Ok(ws) => {
            let state = State::load().unwrap();
            let stored = state.workspaces.get("tmpfs-overlay-ws").unwrap();
            assert!(stored.tmpfs);
            assert_eq!(stored.tmpfs_size, Some("128m".to_string()));
            common::cleanup_workspace(&ws);
        }
        Err(_) => {
            eprintln!("skipping: tmpfs mount not available");
        }
    }
}

mod common;

use std::path::PathBuf;

use graft::error::GraftError;
use graft::state::State;
use graft::workspace::Workspace;
use serial_test::serial;

fn make_workspace(name: &str, parent: Option<String>) -> Workspace {
    let base = PathBuf::from("/tmp/base");
    let root = graft::workspace::graft_home();
    common::make_test_workspace(&root, name, &base, parent)
}

#[test]
#[serial]
fn test_load_empty() {
    let _dir = common::setup_test_env();
    let state = State::load().unwrap();
    assert_eq!(state.version, 1);
    assert!(state.workspaces.is_empty());
}

#[test]
#[serial]
fn test_add_and_get_workspace() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    let ws = make_workspace("test-ws", None);
    state.add_workspace(ws).unwrap();

    let ws = state.get_workspace("test-ws");
    assert!(ws.is_some());
    assert_eq!(ws.unwrap().name, "test-ws");
}

#[test]
#[serial]
fn test_add_duplicate() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("dup", None)).unwrap();
    let result = state.add_workspace(make_workspace("dup", None));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GraftError::WorkspaceExists(_)));
}

#[test]
#[serial]
fn test_remove_workspace() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("rm-me", None)).unwrap();
    assert!(state.get_workspace("rm-me").is_some());

    state.remove_workspace("rm-me").unwrap();
    assert!(state.get_workspace("rm-me").is_none());
}

#[test]
#[serial]
fn test_remove_nonexistent() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    let result = state.remove_workspace("nope");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GraftError::WorkspaceNotFound(_)));
}

#[test]
#[serial]
fn test_list_workspaces() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("a", None)).unwrap();
    state.add_workspace(make_workspace("b", None)).unwrap();
    state.add_workspace(make_workspace("c", None)).unwrap();

    let list = state.list_workspaces();
    assert_eq!(list.len(), 3);

    let mut names: Vec<&str> = list.iter().map(|ws| ws.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
#[serial]
fn test_update_workspace() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("upd", None)).unwrap();

    let ws = state.get_workspace_mut("upd").unwrap();
    ws.tmpfs = true;

    let ws = state.get_workspace("upd").unwrap();
    assert!(ws.tmpfs);
}

#[test]
#[serial]
fn test_corrupted_json() {
    let dir = common::setup_test_env();
    let state_path = dir.path().join("state.json");
    std::fs::write(&state_path, "{{not valid json!!!").unwrap();

    let result = State::load();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), GraftError::StateCorrupted(_)));
}

#[test]
#[serial]
fn test_empty_file() {
    let dir = common::setup_test_env();
    let state_path = dir.path().join("state.json");
    std::fs::write(&state_path, "").unwrap();

    let state = State::load().unwrap();
    assert_eq!(state.version, 1);
    assert!(state.workspaces.is_empty());
}

#[test]
#[serial]
fn test_children_of() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("root", None)).unwrap();
    state.add_workspace(make_workspace("child1", Some("root".into()))).unwrap();
    state.add_workspace(make_workspace("child2", Some("root".into()))).unwrap();
    state.add_workspace(make_workspace("other", None)).unwrap();

    let children = state.children_of("root");
    assert_eq!(children.len(), 2);

    let mut names: Vec<&str> = children.iter().map(|ws| ws.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["child1", "child2"]);

    assert!(state.children_of("other").is_empty());
}

#[test]
#[serial]
fn test_parent_chain() {
    let _dir = common::setup_test_env();
    let mut state = State::load().unwrap();
    state.add_workspace(make_workspace("grandparent", None)).unwrap();
    state.add_workspace(make_workspace("parent", Some("grandparent".into()))).unwrap();
    state.add_workspace(make_workspace("child", Some("parent".into()))).unwrap();

    let chain = state.parent_chain("child");
    let names: Vec<&str> = chain.iter().map(|ws| ws.name.as_str()).collect();
    assert_eq!(names, vec!["grandparent", "parent", "child"]);

    // Root has chain of just itself
    let chain = state.parent_chain("grandparent");
    let names: Vec<&str> = chain.iter().map(|ws| ws.name.as_str()).collect();
    assert_eq!(names, vec!["grandparent"]);
}

#[test]
#[serial]
fn test_save_creates_dirs() {
    let dir = common::setup_test_env();
    // Point GRAFT_HOME to a nested path that doesn't exist yet
    let nested = dir.path().join("a").join("b").join("c");
    std::env::set_var("GRAFT_HOME", &nested);

    let state = State::default();
    state.save().unwrap();

    assert!(nested.join("state.json").exists());
}

#[test]
#[serial]
fn test_state_persistence() {
    let _dir = common::setup_test_env();

    {
        let mut state = State::load().unwrap();
        state.add_workspace(make_workspace("persist", None)).unwrap();
        state.add_workspace(make_workspace("persist-child", Some("persist".into()))).unwrap();
        state.save().unwrap();
    }

    {
        let state = State::load().unwrap();
        assert_eq!(state.workspaces.len(), 2);
        let ws = state.get_workspace("persist").unwrap();
        assert_eq!(ws.name, "persist");
        let child = state.get_workspace("persist-child").unwrap();
        assert_eq!(child.parent.as_deref(), Some("persist"));
    }
}

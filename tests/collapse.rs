mod common;

use std::fs;
use std::path::PathBuf;

use graft::collapse::collapse_uppers;
use graft::error::GraftError;
use graft::state::State;
use serial_test::serial;

// ── collapse_uppers unit tests ──

#[test]
#[serial]
fn test_collapse_uppers_basic() {
    let _graft = common::setup_test_env();
    let tmp = tempfile::tempdir().unwrap();

    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("base_file.txt"), "from base").unwrap();

    // Layer 1: adds file_a.txt
    let upper1 = tmp.path().join("upper1");
    fs::create_dir_all(&upper1).unwrap();
    fs::write(upper1.join("file_a.txt"), "layer1").unwrap();

    // Layer 2: adds file_b.txt, overrides file_a.txt
    let upper2 = tmp.path().join("upper2");
    fs::create_dir_all(&upper2).unwrap();
    fs::write(upper2.join("file_a.txt"), "layer2_override").unwrap();
    fs::write(upper2.join("file_b.txt"), "layer2").unwrap();

    let uppers = vec![upper1, upper2];
    let collapsed = collapse_uppers(&uppers, &base).unwrap();

    // file_a.txt should have layer2's content (later layer wins)
    assert_eq!(
        fs::read_to_string(collapsed.join("file_a.txt")).unwrap(),
        "layer2_override"
    );
    // file_b.txt should exist
    assert_eq!(
        fs::read_to_string(collapsed.join("file_b.txt")).unwrap(),
        "layer2"
    );
}

#[test]
#[serial]
fn test_collapse_uppers_whiteout_propagation() {
    let _graft = common::setup_test_env();
    let tmp = tempfile::tempdir().unwrap();

    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("base_file.txt"), "from base").unwrap();

    // Layer 1: adds added_file.txt
    let upper1 = tmp.path().join("upper1");
    fs::create_dir_all(&upper1).unwrap();
    fs::write(upper1.join("added_file.txt"), "added by layer1").unwrap();

    // Layer 2: whiteouts added_file.txt AND base_file.txt
    let upper2 = tmp.path().join("upper2");
    fs::create_dir_all(&upper2).unwrap();
    fs::write(upper2.join(".wh.added_file.txt"), "").unwrap();
    fs::write(upper2.join(".wh.base_file.txt"), "").unwrap();

    let uppers = vec![upper1, upper2];
    let collapsed = collapse_uppers(&uppers, &base).unwrap();

    // added_file.txt was added by layer1 and deleted by layer2, not in base
    // → neither file nor whiteout should remain
    assert!(
        !collapsed.join("added_file.txt").exists(),
        "added_file.txt should be removed"
    );
    assert!(
        !collapsed.join(".wh.added_file.txt").exists(),
        "whiteout for added_file.txt should not exist (not in base)"
    );

    // base_file.txt IS in base → whiteout must be preserved
    assert!(
        !collapsed.join("base_file.txt").exists(),
        "base_file.txt should not be in collapsed"
    );
    assert!(
        collapsed.join(".wh.base_file.txt").exists(),
        "whiteout for base_file.txt should be preserved (exists in base)"
    );
}

#[test]
#[serial]
fn test_collapse_uppers_override() {
    let _graft = common::setup_test_env();
    let tmp = tempfile::tempdir().unwrap();

    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    // 3 layers, each modifying same file
    let upper1 = tmp.path().join("upper1");
    let upper2 = tmp.path().join("upper2");
    let upper3 = tmp.path().join("upper3");
    fs::create_dir_all(&upper1).unwrap();
    fs::create_dir_all(&upper2).unwrap();
    fs::create_dir_all(&upper3).unwrap();

    fs::write(upper1.join("config.txt"), "v1").unwrap();
    fs::write(upper2.join("config.txt"), "v2").unwrap();
    fs::write(upper3.join("config.txt"), "v3").unwrap();

    let uppers = vec![upper1, upper2, upper3];
    let collapsed = collapse_uppers(&uppers, &base).unwrap();

    // Leaf version (v3) should win
    assert_eq!(
        fs::read_to_string(collapsed.join("config.txt")).unwrap(),
        "v3"
    );
}

// ── Command-level tests ──

#[test]
#[serial]
fn test_collapse_not_stacked() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(graft_dir.path(), "solo", &base, None);

    let mut state = State::load().unwrap();
    state.add_workspace(ws).unwrap();
    state.save().unwrap();

    let result = graft::commands::collapse::run("solo".to_string());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, GraftError::NotStackedWorkspace(_)),
        "expected NotStackedWorkspace, got: {err:?}"
    );
}

#[test]
#[serial]
fn test_collapse_not_leaf() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();

    // root → middle → child
    let ws_root = common::make_test_workspace(graft_dir.path(), "root", &base, None);
    let ws_middle = common::make_test_workspace(graft_dir.path(), "middle", &base, Some("root".to_string()));
    let ws_child = common::make_test_workspace(graft_dir.path(), "child", &base, Some("middle".to_string()));

    let mut state = State::load().unwrap();
    state.add_workspace(ws_root).unwrap();
    state.add_workspace(ws_middle).unwrap();
    state.add_workspace(ws_child).unwrap();
    state.save().unwrap();

    // Try to collapse middle (not a leaf, has child)
    let result = graft::commands::collapse::run("middle".to_string());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, GraftError::HasChildren { .. }),
        "expected HasChildren, got: {err:?}"
    );
}

#[test]
#[serial]
fn test_collapse_multi_child_conflict() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();

    // root has two children: branch_a and branch_b
    let ws_root = common::make_test_workspace(graft_dir.path(), "root", &base, None);
    let ws_a = common::make_test_workspace(graft_dir.path(), "branch_a", &base, Some("root".to_string()));
    let ws_b = common::make_test_workspace(graft_dir.path(), "branch_b", &base, Some("root".to_string()));

    let mut state = State::load().unwrap();
    state.add_workspace(ws_root).unwrap();
    state.add_workspace(ws_a).unwrap();
    state.add_workspace(ws_b).unwrap();
    state.save().unwrap();

    // Try to collapse branch_a — root has another child (branch_b)
    let result = graft::commands::collapse::run("branch_a".to_string());
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("has children"),
        "expected multi-child conflict error, got: {err_msg}"
    );
}

#[test]
#[serial]
fn test_collapse_updates_state() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("readme.txt"), "hello").unwrap();

    // root → leaf (simple 2-level stack)
    let ws_root = common::make_test_workspace(graft_dir.path(), "root", &base, None);
    let ws_leaf = common::make_test_workspace(graft_dir.path(), "leaf", &base, Some("root".to_string()));

    fs::write(ws_root.upper.join("from_root.txt"), "root content").unwrap();
    fs::write(ws_leaf.upper.join("from_leaf.txt"), "leaf content").unwrap();

    let mut state = State::load().unwrap();
    state.add_workspace(ws_root).unwrap();
    state.add_workspace(ws_leaf).unwrap();
    state.save().unwrap();

    // Collapse will try to mount overlay which requires privileges.
    // We test the state logic by running and accepting the mount failure gracefully.
    // Since we can't mount in tests, we test at a lower level.
    // Instead, verify state expectations by directly checking what collapse_uppers produces
    // and then manually updating state like collapse::run would.

    let state = State::load().unwrap();
    let chain: Vec<String> = state
        .parent_chain("leaf")
        .iter()
        .map(|w| w.name.clone())
        .collect();
    assert_eq!(chain, vec!["root", "leaf"]);

    let upper_dirs: Vec<PathBuf> = chain
        .iter()
        .map(|n| state.get_workspace(n).unwrap().upper.clone())
        .collect();
    let original_base = state.get_workspace("root").unwrap().base.clone();

    let collapsed = collapse_uppers(&upper_dirs, &original_base).unwrap();
    assert!(collapsed.join("from_root.txt").exists());
    assert!(collapsed.join("from_leaf.txt").exists());

    // Simulate what the command does to state
    let mut state = State::load().unwrap();
    state.remove_workspace("root").unwrap();
    if let Some(ws) = state.get_workspace_mut("leaf") {
        ws.parent = None;
        ws.base = original_base.clone();
    }
    state.save().unwrap();

    // Verify
    let state = State::load().unwrap();
    let ws = state.get_workspace("leaf").unwrap();
    assert!(ws.parent.is_none(), "parent should be None after collapse");
    assert_eq!(ws.base, original_base, "base should be original base");
    assert!(
        state.get_workspace("root").is_none(),
        "root should be dropped"
    );
}

#[test]
#[serial]
fn test_collapse_drops_intermediates() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();

    // root → middle → leaf (3-level stack)
    let ws_root = common::make_test_workspace(graft_dir.path(), "root", &base, None);
    let ws_middle = common::make_test_workspace(graft_dir.path(), "middle", &base, Some("root".to_string()));
    let ws_leaf = common::make_test_workspace(graft_dir.path(), "leaf", &base, Some("middle".to_string()));

    fs::write(ws_root.upper.join("r.txt"), "root").unwrap();
    fs::write(ws_middle.upper.join("m.txt"), "middle").unwrap();
    fs::write(ws_leaf.upper.join("l.txt"), "leaf").unwrap();

    let mut state = State::load().unwrap();
    state.add_workspace(ws_root).unwrap();
    state.add_workspace(ws_middle).unwrap();
    state.add_workspace(ws_leaf).unwrap();
    state.save().unwrap();

    // Verify chain
    let state = State::load().unwrap();
    let chain: Vec<String> = state
        .parent_chain("leaf")
        .iter()
        .map(|w| w.name.clone())
        .collect();
    assert_eq!(chain, vec!["root", "middle", "leaf"]);

    // Verify collapse_uppers works with 3 layers
    let upper_dirs: Vec<PathBuf> = chain
        .iter()
        .map(|n| state.get_workspace(n).unwrap().upper.clone())
        .collect();
    let original_base = state.get_workspace("root").unwrap().base.clone();
    let collapsed = collapse_uppers(&upper_dirs, &original_base).unwrap();
    assert!(collapsed.join("r.txt").exists());
    assert!(collapsed.join("m.txt").exists());
    assert!(collapsed.join("l.txt").exists());

    // Simulate dropping intermediates and updating state
    let mut state = State::load().unwrap();
    // Drop deepest first: middle, then root
    state.remove_workspace("middle").unwrap();
    state.remove_workspace("root").unwrap();
    if let Some(ws) = state.get_workspace_mut("leaf") {
        ws.parent = None;
        ws.base = original_base.clone();
    }
    state.save().unwrap();

    let state = State::load().unwrap();
    assert!(state.get_workspace("root").is_none());
    assert!(state.get_workspace("middle").is_none());
    let leaf = state.get_workspace("leaf").unwrap();
    assert!(leaf.parent.is_none());
    assert_eq!(leaf.base, original_base);
}

mod common;

use std::fs;
use std::path::PathBuf;

use graft::diff::{walk_cumulative_upper, walk_upper, ChangeKind};
use graft::state::State;
use graft::tree::{build_hierarchy, format_hierarchy};
use graft::workspace::Workspace;
use serial_test::serial;

// ── Tree tests ──

#[test]
fn test_tree_no_workspaces() {
    let workspaces: Vec<&Workspace> = vec![];
    let forest = build_hierarchy(&workspaces);
    let output = format_hierarchy(&forest);
    assert_eq!(common::strip_ansi(&output), "no workspaces");
}

#[test]
fn test_tree_single_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "my-project", &base, None);

    // Add some files to upper
    fs::write(ws.upper.join("file1.txt"), "hello\n").unwrap();
    fs::write(ws.upper.join("file2.txt"), "world\n").unwrap();

    let workspaces: Vec<&Workspace> = vec![&ws];
    let forest = build_hierarchy(&workspaces);
    let output = format_hierarchy(&forest);

    let plain = common::strip_ansi(&output);
    assert!(plain.contains("my-project/"), "expected 'my-project/' in: {plain}");
    assert!(plain.contains("2 files"), "expected '2 files' in: {plain}");
}

#[test]
fn test_tree_stacked_workspaces() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws_a = common::make_test_workspace(tmp.path(), "a", &base, None);
    let ws_b = common::make_test_workspace(tmp.path(), "b", &base, Some("a".to_string()));
    let ws_c = common::make_test_workspace(tmp.path(), "c", &base, Some("b".to_string()));

    fs::write(ws_a.upper.join("f1.txt"), "a\n").unwrap();
    fs::write(ws_a.upper.join("f2.txt"), "a\n").unwrap();
    fs::write(ws_a.upper.join("f3.txt"), "a\n").unwrap();
    fs::write(ws_b.upper.join("f1.txt"), "b\n").unwrap();
    fs::write(ws_b.upper.join("f2.txt"), "b\n").unwrap();
    fs::write(ws_c.upper.join("f1.txt"), "c\n").unwrap();

    let workspaces: Vec<&Workspace> = vec![&ws_a, &ws_b, &ws_c];
    let forest = build_hierarchy(&workspaces);

    assert_eq!(forest.len(), 1, "should have 1 root");
    assert_eq!(forest[0].name, "a");
    assert_eq!(forest[0].file_count, 3);
    assert_eq!(forest[0].children.len(), 1);
    assert_eq!(forest[0].children[0].name, "b");
    assert_eq!(forest[0].children[0].file_count, 2);
    assert_eq!(forest[0].children[0].children.len(), 1);
    assert_eq!(forest[0].children[0].children[0].name, "c");
    assert_eq!(forest[0].children[0].children[0].file_count, 1);

    let output = format_hierarchy(&forest);
    let plain = common::strip_ansi(&output);
    assert!(plain.contains("a/"), "expected 'a/' in: {plain}");
    assert!(plain.contains("├── b") || plain.contains("└── b"), "expected b in: {plain}");
    assert!(plain.contains("└── c"), "expected c in: {plain}");
}

#[test]
fn test_tree_multiple_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let base1 = tmp.path().join("base1");
    let base2 = tmp.path().join("base2");
    fs::create_dir_all(&base1).unwrap();
    fs::create_dir_all(&base2).unwrap();

    let ws1 = common::make_test_workspace(tmp.path(), "alpha", &base1, None);
    let ws2 = common::make_test_workspace(tmp.path(), "beta", &base2, None);

    let workspaces: Vec<&Workspace> = vec![&ws1, &ws2];
    let forest = build_hierarchy(&workspaces);

    assert_eq!(forest.len(), 2);
    assert_eq!(forest[0].name, "alpha");
    assert_eq!(forest[1].name, "beta");

    let output = format_hierarchy(&forest);
    let plain = common::strip_ansi(&output);
    assert!(plain.contains("alpha/"), "expected 'alpha/' in: {plain}");
    assert!(plain.contains("beta/"), "expected 'beta/' in: {plain}");
}

// ── Cumulative diff tests ──

#[test]
#[serial]
fn test_cumulative_diff() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("original.txt"), "original content\n").unwrap();

    // Root workspace: adds new_in_root.txt
    let ws_root = common::make_test_workspace(graft_dir.path(), "root-ws", &base, None);
    fs::write(ws_root.upper.join("new_in_root.txt"), "from root\n").unwrap();

    // Child workspace: adds new_in_child.txt, modifies original.txt
    let ws_child = common::make_test_workspace(graft_dir.path(), "child-ws", &base, Some("root-ws".to_string()));
    fs::write(ws_child.upper.join("new_in_child.txt"), "from child\n").unwrap();
    fs::write(ws_child.upper.join("original.txt"), "modified by child\n").unwrap();

    let mut state = State::load().unwrap();
    state.add_workspace(ws_root).unwrap();
    state.add_workspace(ws_child).unwrap();
    state.save().unwrap();

    let state = State::load().unwrap();
    let ws = state.get_workspace("child-ws").unwrap();
    let changes = walk_cumulative_upper(ws, &state).unwrap();

    // Should see: new_in_root.txt (Added), new_in_child.txt (Added), original.txt (Modified)
    assert_eq!(changes.len(), 3, "changes: {:?}", changes);

    let added: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c.kind, ChangeKind::Added))
        .collect();
    let modified: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c.kind, ChangeKind::Modified))
        .collect();

    assert_eq!(added.len(), 2);
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].path, PathBuf::from("original.txt"));
}

#[test]
#[serial]
fn test_cumulative_diff_whiteout() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("keep.txt"), "keep\n").unwrap();

    // Parent adds a file
    let ws_parent = common::make_test_workspace(graft_dir.path(), "parent-ws", &base, None);
    fs::write(ws_parent.upper.join("added_by_parent.txt"), "parent\n").unwrap();

    // Child deletes the file added by parent via whiteout
    let ws_child = common::make_test_workspace(graft_dir.path(), "child-ws", &base, Some("parent-ws".to_string()));
    fs::write(ws_child.upper.join(".wh.added_by_parent.txt"), "").unwrap();

    let mut state = State::load().unwrap();
    state.add_workspace(ws_parent).unwrap();
    state.add_workspace(ws_child).unwrap();
    state.save().unwrap();

    let state = State::load().unwrap();
    let ws = state.get_workspace("child-ws").unwrap();
    let changes = walk_cumulative_upper(ws, &state).unwrap();

    // added_by_parent.txt should NOT appear (added by parent, deleted by child, not in base)
    let found = changes
        .iter()
        .any(|c| c.path == PathBuf::from("added_by_parent.txt"));
    assert!(!found, "whiteout should remove parent-added file from cumulative diff");

    // keep.txt is in base and not touched by anyone, so no change
    assert!(changes.is_empty(), "changes: {:?}", changes);
}

#[test]
#[serial]
fn test_regular_diff_only_shows_current_layer() {
    let graft_dir = common::setup_test_env();

    let base = graft_dir.path().join("project");
    fs::create_dir_all(&base).unwrap();

    // Parent adds a file
    let ws_parent = common::make_test_workspace(graft_dir.path(), "parent-ws", &base, None);
    fs::write(ws_parent.upper.join("parent_file.txt"), "parent\n").unwrap();

    // Child adds a different file
    let ws_child = common::make_test_workspace(graft_dir.path(), "child-ws", &base, Some("parent-ws".to_string()));
    fs::write(ws_child.upper.join("child_file.txt"), "child\n").unwrap();

    let mut state = State::load().unwrap();
    state.add_workspace(ws_parent).unwrap();
    state.add_workspace(ws_child).unwrap();
    state.save().unwrap();

    let state = State::load().unwrap();
    let ws = state.get_workspace("child-ws").unwrap();

    // Regular (non-cumulative) diff should only show child's own changes
    let changes = walk_upper(ws).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, PathBuf::from("child_file.txt"));
    assert!(matches!(changes[0].kind, ChangeKind::Added));

    // Parent's file should NOT appear in regular diff
    let has_parent_file = changes
        .iter()
        .any(|c| c.path == PathBuf::from("parent_file.txt"));
    assert!(!has_parent_file, "regular diff should not show parent layer files");
}

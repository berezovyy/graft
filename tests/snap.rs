mod common;

use std::fs;
use std::path::PathBuf;

use graft::diff::ChangeKind;
use graft::snap;
use graft::state::State;
use graft::workspace::Workspace;
use serial_test::serial;

/// Create a workspace struct and register it in state, with upper dir populated.
fn setup_workspace(graft_home: &std::path::Path, name: &str) -> Workspace {
    let base = graft_home.join(format!("{name}-base"));
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(graft_home, name, &base, None);

    let mut state = State::load().unwrap();
    state.add_workspace(ws.clone()).unwrap();
    state.save().unwrap();

    ws
}

#[test]
#[serial]
fn test_create_snapshot_auto_name() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    // Put some files in upper
    fs::write(ws.upper.join("file1.txt"), "hello\n").unwrap();
    fs::write(ws.upper.join("file2.txt"), "world\n").unwrap();

    let info = snap::create_snapshot(&ws, None).unwrap();
    assert_eq!(info.name, "snap-001");
    assert_eq!(info.file_count, 2);
    assert!(info.total_size > 0);
    assert_eq!(info.trigger, "manual");

    // Verify snapshot directory exists with upper copy
    let snap_upper = _dir.path().join("test-ws/snapshots/snap-001/upper");
    assert!(snap_upper.exists());
    assert_eq!(
        fs::read_to_string(snap_upper.join("file1.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
#[serial]
fn test_create_snapshot_custom_name() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    fs::write(ws.upper.join("code.rs"), "fn main() {}\n").unwrap();

    let info = snap::create_snapshot(&ws, Some("before-refactor")).unwrap();
    assert_eq!(info.name, "before-refactor");
    assert_eq!(info.file_count, 1);
}

#[test]
#[serial]
fn test_create_multiple_auto_increment() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    fs::write(ws.upper.join("file.txt"), "content\n").unwrap();

    let s1 = snap::create_snapshot(&ws, None).unwrap();
    let s2 = snap::create_snapshot(&ws, None).unwrap();
    let s3 = snap::create_snapshot(&ws, None).unwrap();

    assert_eq!(s1.name, "snap-001");
    assert_eq!(s2.name, "snap-002");
    assert_eq!(s3.name, "snap-003");
}

#[test]
#[serial]
fn test_list_snapshots() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    fs::write(ws.upper.join("file.txt"), "content\n").unwrap();

    snap::create_snapshot(&ws, Some("first")).unwrap();
    // Small delay to ensure different timestamps
    snap::create_snapshot(&ws, Some("second")).unwrap();

    let list = snap::list_snapshots(&ws).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].name, "first");
    assert_eq!(list[1].name, "second");
}

#[test]
#[serial]
fn test_list_empty() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    let list = snap::list_snapshots(&ws).unwrap();
    assert!(list.is_empty());
}

#[test]
#[serial]
fn test_restore_snapshot() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    // Create initial files
    fs::write(ws.upper.join("file.txt"), "original\n").unwrap();

    // Take snapshot
    snap::create_snapshot(&ws, Some("checkpoint")).unwrap();

    // Modify the upper
    fs::write(ws.upper.join("file.txt"), "modified\n").unwrap();
    fs::write(ws.upper.join("new_file.txt"), "new content\n").unwrap();

    // Verify modification
    assert_eq!(
        fs::read_to_string(ws.upper.join("file.txt")).unwrap(),
        "modified\n"
    );
    assert!(ws.upper.join("new_file.txt").exists());

    // Restore
    snap::restore_snapshot(&ws, "checkpoint").unwrap();

    // Verify restoration
    assert_eq!(
        fs::read_to_string(ws.upper.join("file.txt")).unwrap(),
        "original\n"
    );
    assert!(
        !ws.upper.join("new_file.txt").exists(),
        "new_file.txt should not exist after restore"
    );
}

#[test]
#[serial]
fn test_restore_nonexistent() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    let result = snap::restore_snapshot(&ws, "nonexistent");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        graft::error::GraftError::SnapshotNotFound { .. }
    ));
}

#[test]
#[serial]
fn test_diff_snapshot() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    // Create initial files
    fs::write(ws.upper.join("keep.txt"), "unchanged\n").unwrap();
    fs::write(ws.upper.join("modify.txt"), "original\n").unwrap();
    fs::write(ws.upper.join("remove.txt"), "to be deleted\n").unwrap();

    // Take snapshot
    snap::create_snapshot(&ws, Some("base")).unwrap();

    // Make changes
    fs::write(ws.upper.join("modify.txt"), "changed\n").unwrap();
    fs::remove_file(ws.upper.join("remove.txt")).unwrap();
    fs::write(ws.upper.join("added.txt"), "new file\n").unwrap();

    // Diff against snapshot
    let changes = snap::diff_snapshot(&ws, "base").unwrap();

    assert_eq!(changes.len(), 3);

    let added = changes.iter().find(|c| c.path == PathBuf::from("added.txt")).unwrap();
    assert!(matches!(added.kind, ChangeKind::Added));

    let modified = changes.iter().find(|c| c.path == PathBuf::from("modify.txt")).unwrap();
    assert!(matches!(modified.kind, ChangeKind::Modified));

    let deleted = changes.iter().find(|c| c.path == PathBuf::from("remove.txt")).unwrap();
    assert!(matches!(deleted.kind, ChangeKind::Deleted));
}

#[test]
#[serial]
fn test_delete_snapshot() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    fs::write(ws.upper.join("file.txt"), "content\n").unwrap();
    snap::create_snapshot(&ws, Some("to-delete")).unwrap();

    let snap_dir = _dir.path().join("test-ws/snapshots/to-delete");
    assert!(snap_dir.exists());

    snap::delete_snapshot(&ws, "to-delete").unwrap();
    assert!(!snap_dir.exists());
}

#[test]
#[serial]
fn test_delete_nonexistent() {
    let _dir = common::setup_test_env();
    let ws = setup_workspace(_dir.path(), "test-ws");

    let result = snap::delete_snapshot(&ws, "nonexistent");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        graft::error::GraftError::SnapshotNotFound { .. }
    ));
}

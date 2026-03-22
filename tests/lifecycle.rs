mod common;

use std::fs;

use graft::diff;
use graft::merge::{self, MergeOpts};

/// Full lifecycle test: simulates fork → create files → diff → merge → verify → cleanup.
/// Does not require overlay mounts; tests the pure merge/diff logic.
#[test]
fn test_full_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    // Step 1: Set up base (simulates the project being forked)
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("README.md"), "# My Project\n").unwrap();
    fs::write(base.join("src/main.rs"), "fn main() {}\n").unwrap();

    // Step 2: Create workspace
    let ws = common::make_test_workspace(tmp.path(), "lifecycle-ws", &base, None);

    // Step 3: "Work" in the workspace — add, modify, delete files
    fs::create_dir_all(ws.upper.join("src")).unwrap();

    // Add a new file
    fs::write(ws.upper.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    // Modify an existing file
    fs::write(
        ws.upper.join("src/main.rs"),
        "use crate::lib::hello;\nfn main() { hello(); }\n",
    )
    .unwrap();

    // Delete a file via whiteout
    fs::write(ws.upper.join(".wh.README.md"), "").unwrap();

    // Step 4: Diff — verify we see the changes
    let changes = diff::walk_upper(&ws).unwrap();
    assert_eq!(changes.len(), 3, "expected 3 changes, got: {changes:?}");

    let added: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c.kind, diff::ChangeKind::Added))
        .collect();
    let modified: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c.kind, diff::ChangeKind::Modified))
        .collect();
    let deleted: Vec<_> = changes
        .iter()
        .filter(|c| matches!(c.kind, diff::ChangeKind::Deleted))
        .collect();

    assert_eq!(added.len(), 1);
    assert_eq!(modified.len(), 1);
    assert_eq!(deleted.len(), 1);

    // Step 5: Generate patch and verify it looks right
    let patch = merge::generate_patch(&ws, &changes).unwrap();
    assert!(!patch.is_empty());
    assert!(patch.contains("src/lib.rs"));
    assert!(patch.contains("src/main.rs"));
    assert!(patch.contains("README.md"));

    // Step 6: Merge
    let opts = MergeOpts {
        commit: false,
        message: None,
        drop: false,
        patch: false,
        no_install: true,
    };
    let result = merge::merge_workspace(&ws, &opts).unwrap();

    assert_eq!(result.added, 1);
    assert_eq!(result.modified, 1);
    assert_eq!(result.deleted, 1);

    // Step 7: Verify base is updated
    assert!(base.join("src/lib.rs").exists());
    assert_eq!(
        fs::read_to_string(base.join("src/lib.rs")).unwrap(),
        "pub fn hello() {}\n"
    );
    assert_eq!(
        fs::read_to_string(base.join("src/main.rs")).unwrap(),
        "use crate::lib::hello;\nfn main() { hello(); }\n"
    );
    assert!(!base.join("README.md").exists());

    // Step 8: Cleanup (simulates drop — just remove workspace dirs)
    fs::remove_dir_all(&ws.upper).unwrap();
    fs::remove_dir_all(&ws.work).unwrap();
    fs::remove_dir_all(&ws.merged).unwrap();
    assert!(!ws.upper.exists());
    assert!(!ws.work.exists());
    assert!(!ws.merged.exists());

    // Base remains intact
    assert!(base.join("src/lib.rs").exists());
    assert!(base.join("src/main.rs").exists());
}

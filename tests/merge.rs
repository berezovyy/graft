mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use graft::diff;
use graft::merge;

#[test]
fn test_merge_copies_added_files() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(ws.upper.join("new_file.txt"), "hello world\n").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(result.modified, 0);
    assert_eq!(result.deleted, 0);
    assert_eq!(fs::read_to_string(base.join("new_file.txt")).unwrap(), "hello world\n");
}

#[test]
fn test_merge_copies_modified_files() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(base.join("file.txt"), "original\n").unwrap();
    fs::write(ws.upper.join("file.txt"), "modified content\n").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 0);
    assert_eq!(result.modified, 1);
    assert_eq!(result.deleted, 0);
    assert_eq!(fs::read_to_string(base.join("file.txt")).unwrap(), "modified content\n");
}

#[test]
fn test_merge_handles_deletions() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(base.join("to_delete.txt"), "will be deleted\n").unwrap();
    fs::write(ws.upper.join(".wh.to_delete.txt"), "").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.deleted, 1);
    assert!(!base.join("to_delete.txt").exists());
}

#[test]
fn test_merge_skips_node_modules() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::create_dir_all(ws.upper.join("node_modules/pkg")).unwrap();
    fs::write(ws.upper.join("node_modules/pkg/index.js"), "module.exports = {}\n").unwrap();
    fs::write(ws.upper.join("app.js"), "console.log('hi')\n").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(result.skipped, 1);
    assert!(base.join("app.js").exists());
    assert!(!base.join("node_modules/pkg/index.js").exists());
}

#[test]
fn test_merge_preserves_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    let script_path = ws.upper.join("run.sh");
    fs::write(&script_path, "#!/bin/bash\necho hello\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 1);

    let perms = fs::metadata(base.join("run.sh")).unwrap().permissions();
    assert!(perms.mode() & 0o111 != 0, "executable bit should be preserved");
}

#[test]
fn test_merge_no_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(base.join("existing.txt"), "content\n").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 0);
    assert_eq!(result.modified, 0);
    assert_eq!(result.deleted, 0);
    assert_eq!(result.skipped, 0);
}

#[test]
fn test_detect_package_manager() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // No package.json → no PM even with lockfile
    fs::write(ws.upper.join("bun.lockb"), "").unwrap();
    assert!(merge::detect_package_manager(&ws).is_none());

    fs::write(ws.upper.join("package.json"), "{}").unwrap();
    assert!(matches!(merge::detect_package_manager(&ws), Some(merge::PackageManager::Bun)));
}

#[test]
fn test_detect_package_manager_variants() {
    let cases: &[(&str, &str)] = &[
        ("pnpm-lock.yaml", "Pnpm"),
        ("package-lock.json", "Npm"),
        ("yarn.lock", "Yarn"),
    ];

    for (lockfile, _expected) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("base");
        fs::create_dir_all(&base).unwrap();
        let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

        fs::write(ws.upper.join("package.json"), "{}").unwrap();
        fs::write(ws.upper.join(lockfile), "").unwrap();
        assert!(merge::detect_package_manager(&ws).is_some(), "expected PM for {lockfile}");
    }
}

#[test]
fn test_merge_patch_output() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(ws.upper.join("new.txt"), "line1\nline2\n").unwrap();

    let changes = diff::collect_changes(&ws).unwrap();
    let patch = merge::generate_patch(&ws, &changes).unwrap();
    assert!(patch.contains("--- /dev/null"));
    assert!(patch.contains("+++ b/new.txt"));
    assert!(patch.contains("+line1"));
    assert!(patch.contains("+line2"));
}

#[test]
fn test_merge_creates_parent_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();
    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::create_dir_all(ws.upper.join("src/deep/nested")).unwrap();
    fs::write(ws.upper.join("src/deep/nested/file.rs"), "fn nested() {}\n").unwrap();

    let result = merge::merge_workspace(&ws).unwrap();
    assert_eq!(result.added, 1);
    assert!(base.join("src/deep/nested/file.rs").exists());
}

#[test]
fn test_merge_git_commit_not_a_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let result = merge::git_commit(&base, "test commit");
    assert!(result.is_err());
    match result.unwrap_err() {
        graft::error::GraftError::GitFailed(msg) => {
            assert!(msg.contains("not a git repository"));
        }
        other => panic!("expected GitFailed, got: {other:?}"),
    }
}

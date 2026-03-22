mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use graft::diff;
use graft::merge::{self, MergeOpts};

fn default_opts() -> MergeOpts {
    MergeOpts {
        commit: false,
        message: None,
        drop: false,
        patch: false,
        no_install: true,
    }
}

#[test]
fn test_merge_copies_added_files() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Add a new file in upper
    fs::write(ws.upper.join("new_file.txt"), "hello world\n").unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 1);
    assert_eq!(result.modified, 0);
    assert_eq!(result.deleted, 0);

    // Verify file exists in base
    let base_file = base.join("new_file.txt");
    assert!(base_file.exists());
    assert_eq!(fs::read_to_string(&base_file).unwrap(), "hello world\n");
}

#[test]
fn test_merge_copies_modified_files() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Original file in base
    fs::write(base.join("file.txt"), "original\n").unwrap();
    // Modified version in upper
    fs::write(ws.upper.join("file.txt"), "modified content\n").unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 0);
    assert_eq!(result.modified, 1);
    assert_eq!(result.deleted, 0);

    // Verify base has new content
    assert_eq!(
        fs::read_to_string(base.join("file.txt")).unwrap(),
        "modified content\n"
    );
}

#[test]
fn test_merge_handles_deletions() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // File exists in base
    fs::write(base.join("to_delete.txt"), "will be deleted\n").unwrap();
    // Whiteout in upper signals deletion
    fs::write(ws.upper.join(".wh.to_delete.txt"), "").unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

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

    // Add a file under node_modules in upper
    fs::write(
        ws.upper.join("node_modules/pkg/index.js"),
        "module.exports = {}\n",
    )
    .unwrap();
    // Also add a normal file
    fs::write(ws.upper.join("app.js"), "console.log('hi')\n").unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 1); // only app.js
    assert_eq!(result.skipped, 1); // node_modules file skipped
    assert!(base.join("app.js").exists());
    assert!(!base.join("node_modules/pkg/index.js").exists());
}

#[test]
fn test_merge_preserves_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Create an executable file in upper
    let script_path = ws.upper.join("run.sh");
    fs::write(&script_path, "#!/bin/bash\necho hello\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 1);

    let base_script = base.join("run.sh");
    assert!(base_script.exists());
    let perms = fs::metadata(&base_script).unwrap().permissions();
    // fs::copy preserves permissions; check executable bit is set
    assert!(perms.mode() & 0o111 != 0, "executable bit should be preserved");
}

#[test]
fn test_merge_no_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Base has a file, upper is empty → no changes
    fs::write(base.join("existing.txt"), "content\n").unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 0);
    assert_eq!(result.modified, 0);
    assert_eq!(result.deleted, 0);
    assert_eq!(result.skipped, 0);
}

#[test]
fn test_merge_no_install_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Set up package.json and a lockfile in upper
    fs::write(ws.upper.join("package.json"), r#"{"name":"test"}"#).unwrap();
    fs::write(ws.upper.join("package-lock.json"), "{}").unwrap();

    // With no_install = true, detect_package_manager still works but run_install should not be called
    // We just verify the detection works and no_install flag is respected
    let pm = merge::detect_package_manager(&ws);
    assert!(pm.is_some()); // PM is detected

    let opts = MergeOpts {
        commit: false,
        message: None,
        drop: false,
        patch: false,
        no_install: true,
    };
    // merge_workspace itself doesn't call install, the command layer does
    let result = merge::merge_workspace(&ws, &opts).unwrap();
    assert!(!result.install_ran);
}

#[test]
fn test_merge_patch_output() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // Add a file
    fs::write(ws.upper.join("new.txt"), "line1\nline2\n").unwrap();

    let changes = diff::walk_upper(&ws).unwrap();
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
    fs::write(
        ws.upper.join("src/deep/nested/file.rs"),
        "fn nested() {}\n",
    )
    .unwrap();

    let result = merge::merge_workspace(&ws, &default_opts()).unwrap();

    assert_eq!(result.added, 1);
    assert!(base.join("src/deep/nested/file.rs").exists());
    assert_eq!(
        fs::read_to_string(base.join("src/deep/nested/file.rs")).unwrap(),
        "fn nested() {}\n"
    );
}

#[test]
fn test_merge_detect_package_manager_bun() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    // No package.json → no PM
    fs::write(ws.upper.join("bun.lockb"), "").unwrap();
    assert!(merge::detect_package_manager(&ws).is_none());

    // Add package.json → Bun detected
    fs::write(ws.upper.join("package.json"), "{}").unwrap();
    assert!(matches!(
        merge::detect_package_manager(&ws),
        Some(merge::PackageManager::Bun)
    ));
}

#[test]
fn test_merge_detect_package_manager_pnpm() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(ws.upper.join("package.json"), "{}").unwrap();
    fs::write(ws.upper.join("pnpm-lock.yaml"), "").unwrap();
    assert!(matches!(
        merge::detect_package_manager(&ws),
        Some(merge::PackageManager::Pnpm)
    ));
}

#[test]
fn test_merge_detect_package_manager_npm() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(ws.upper.join("package.json"), "{}").unwrap();
    fs::write(ws.upper.join("package-lock.json"), "{}").unwrap();
    assert!(matches!(
        merge::detect_package_manager(&ws),
        Some(merge::PackageManager::Npm)
    ));
}

#[test]
fn test_merge_detect_package_manager_yarn() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().join("base");
    fs::create_dir_all(&base).unwrap();

    let ws = common::make_test_workspace(tmp.path(), "test-ws", &base, None);

    fs::write(ws.upper.join("package.json"), "{}").unwrap();
    fs::write(ws.upper.join("yarn.lock"), "").unwrap();
    assert!(matches!(
        merge::detect_package_manager(&ws),
        Some(merge::PackageManager::Yarn)
    ));
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

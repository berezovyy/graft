use std::fs;
use std::path::Path;
use std::process::Command;

use crate::diff::{self, ChangeKind, FileChange};
use crate::error::{GraftError, Result};
use crate::util::{ensure_parent_dir, IoContext};
use crate::workspace::Workspace;

pub struct MergeResult {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub skipped: usize,
}

pub enum PackageManager {
    Bun,
    Pnpm,
    Npm,
    Yarn,
}

impl PackageManager {
    fn command(&self) -> &str {
        match self {
            PackageManager::Bun => "bun",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Npm => "npm",
            PackageManager::Yarn => "yarn",
        }
    }
}

pub fn merge_workspace(workspace: &Workspace) -> Result<MergeResult> {
    let changes = diff::collect_changes_fast(workspace)?;

    if changes.is_empty() {
        return Ok(MergeResult {
            added: 0,
            modified: 0,
            deleted: 0,
            skipped: 0,
        });
    }

    let mut added = 0usize;
    let mut modified = 0usize;
    let mut deleted = 0usize;
    let mut skipped = 0usize;

    for change in &changes {
        let path_str = change.path.to_string_lossy();
        if path_str.starts_with("node_modules/") || path_str == "node_modules" {
            skipped += 1;
            continue;
        }

        match change.kind {
            ChangeKind::Added | ChangeKind::Modified => {
                let src = workspace.upper.join(&change.path);
                let dst = workspace.base.join(&change.path);

                ensure_parent_dir(&dst)?;

                fs::copy(&src, &dst).io_context(|| {
                    format!("copy {} -> {}", src.display(), dst.display())
                })?;

                if matches!(change.kind, ChangeKind::Added) {
                    added += 1;
                } else {
                    modified += 1;
                }
            }
            ChangeKind::Deleted => {
                let target = workspace.base.join(&change.path);
                if target.exists() {
                    if target.is_dir() {
                        fs::remove_dir_all(&target).io_context(|| {
                            format!("remove directory {}", target.display())
                        })?;
                    } else {
                        fs::remove_file(&target).io_context(|| {
                            format!("remove {}", target.display())
                        })?;
                    }
                }
                deleted += 1;
            }
        }
    }

    Ok(MergeResult {
        added,
        modified,
        deleted,
        skipped,
    })
}

pub fn detect_package_manager(workspace: &Workspace) -> Option<PackageManager> {
    let upper = &workspace.upper;

    if !upper.join("package.json").exists() {
        return None;
    }

    if upper.join("bun.lockb").exists() || upper.join("bun.lock").exists() {
        return Some(PackageManager::Bun);
    }
    if upper.join("pnpm-lock.yaml").exists() {
        return Some(PackageManager::Pnpm);
    }
    if upper.join("package-lock.json").exists() {
        return Some(PackageManager::Npm);
    }
    if upper.join("yarn.lock").exists() {
        return Some(PackageManager::Yarn);
    }

    None
}

pub(crate) fn run_install(base: &Path, pm: &PackageManager) -> Result {
    let cmd = pm.command();
    let status = Command::new(cmd)
        .arg("install")
        .current_dir(base)
        .status()
        .map_err(|e| {
            GraftError::PackageManagerFailed(format!(
                "failed to run '{cmd} install': {e}. Is {cmd} installed and on your PATH?"
            ))
        })?;

    if !status.success() {
        return Err(GraftError::PackageManagerFailed(format!(
            "'{cmd} install' exited with status {status}"
        )));
    }

    Ok(())
}

pub fn generate_patch(workspace: &Workspace, changes: &[FileChange]) -> Result<String> {
    let mut patch = String::new();
    for change in changes {
        let diff_text = diff::generate_unified_diff(workspace, change);
        patch.push_str(&diff_text);
        if !diff_text.ends_with('\n') {
            patch.push('\n');
        }
    }
    Ok(patch)
}

fn run_git(base: &Path, args: &[&str]) -> Result {
    let cmd_str = format!("git {}", args.join(" "));
    let status = Command::new("git")
        .args(args)
        .current_dir(base)
        .status()
        .map_err(|e| GraftError::GitFailed(format!("failed to run '{cmd_str}': {e}")))?;

    if !status.success() {
        return Err(GraftError::GitFailed(format!(
            "'{cmd_str}' exited with status {status}"
        )));
    }

    Ok(())
}

pub fn git_commit(base: &Path, message: &str) -> Result {
    if !base.join(".git").exists() {
        return Err(GraftError::GitFailed(
            "base directory is not a git repository".to_string(),
        ));
    }

    run_git(base, &["add", "-A"])?;
    run_git(base, &["commit", "-m", message])?;

    Ok(())
}

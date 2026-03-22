use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::diff::{self, ChangeKind, FileChange};
use crate::error::GraftError;
use crate::util::{recursive_copy, IoContext};
use crate::workspace::{graft_home, Workspace};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub created: String,
    pub file_count: usize,
    pub total_size: u64,
    pub trigger: String,
}

fn snapshots_dir(workspace: &Workspace) -> PathBuf {
    graft_home().join(&workspace.name).join("snapshots")
}

fn snapshot_dir(workspace: &Workspace, snap_name: &str) -> PathBuf {
    snapshots_dir(workspace).join(snap_name)
}

/// Copy upper dir to snapshot, trying cp -a --reflink=auto first, falling back to manual copy.
fn cow_copy(upper: &Path, snapshot_upper: &Path) -> Result<(), GraftError> {
    // Try cp -a --reflink=auto for efficiency (CoW on btrfs/xfs)
    let result = std::process::Command::new("cp")
        .args(["-a", "--reflink=auto"])
        .arg(upper)
        .arg(snapshot_upper)
        .status();

    match result {
        Ok(status) if status.success() => Ok(()),
        _ => {
            // Fall back to manual recursive copy
            recursive_copy(upper, snapshot_upper, |_| false)
        }
    }
}

/// Count files and total size in a directory tree.
fn dir_stats(dir: &Path) -> (usize, u64) {
    let mut file_count = 0usize;
    let mut total_size = 0u64;

    if !dir.exists() {
        return (0, 0);
    }

    for entry in WalkDir::new(dir).min_depth(1).into_iter().flatten() {
        if entry.file_type().is_file() {
            file_count += 1;
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
            }
        }
    }

    (file_count, total_size)
}

/// Auto-generate a snapshot name by scanning existing snapshots for the highest snap-NNN.
fn auto_name(workspace: &Workspace) -> String {
    let snaps_dir = snapshots_dir(workspace);
    let mut max = 0u32;

    if let Ok(entries) = fs::read_dir(&snaps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(num_str) = name.strip_prefix("snap-") {
                if let Ok(n) = num_str.parse::<u32>() {
                    if n > max {
                        max = n;
                    }
                }
            }
        }
    }

    format!("snap-{:03}", max + 1)
}

/// Freeze the current upper layer state into a named snapshot.
/// Uses cp --reflink=auto for instant snapshots on CoW filesystems.
pub fn create_snapshot(
    workspace: &Workspace,
    name: Option<&str>,
) -> Result<Snapshot, GraftError> {
    let snap_name = match name {
        Some(n) => n.to_string(),
        None => auto_name(workspace),
    };

    let snap_dir = snapshot_dir(workspace, &snap_name);
    let snap_upper = snap_dir.join("upper");

    fs::create_dir_all(&snap_dir)
        .io_context(|| format!("create snapshot directory {}", snap_dir.display()))?;

    cow_copy(&workspace.upper, &snap_upper)?;

    let (file_count, total_size) = dir_stats(&snap_upper);

    let info = Snapshot {
        name: snap_name,
        created: crate::workspace::now_rfc3339(),
        file_count,
        total_size,
        trigger: "manual".to_string(),
    };

    let meta_path = snap_dir.join(".meta.json");
    let json = serde_json::to_string_pretty(&info).map_err(|e| GraftError::StateFailed(
        format!("failed to serialize snapshot metadata: {e}"),
    ))?;
    fs::write(&meta_path, json)
        .io_context(|| format!("write snapshot metadata {}", meta_path.display()))?;

    Ok(info)
}

pub fn list_snapshots(workspace: &Workspace) -> Result<Vec<Snapshot>, GraftError> {
    let snaps_dir = snapshots_dir(workspace);

    if !snaps_dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();

    let entries = fs::read_dir(&snaps_dir)
        .io_context(|| format!("read snapshots directory {}", snaps_dir.display()))?;

    for entry in entries {
        let entry = entry
            .io_context(|| "read directory entry".to_string())?;

        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let meta_path = entry.path().join(".meta.json");
        if !meta_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&meta_path)
            .io_context(|| format!("read snapshot metadata {}", meta_path.display()))?;

        let info: Snapshot = serde_json::from_str(&content).map_err(|e| {
            GraftError::StateCorrupted(format!("invalid snapshot metadata: {e}"))
        })?;

        snapshots.push(info);
    }

    snapshots.sort_by(|a, b| a.created.cmp(&b.created).then(a.name.cmp(&b.name)));
    Ok(snapshots)
}

pub fn restore_snapshot(workspace: &Workspace, snap_name: &str) -> Result<(), GraftError> {
    let snap_dir = snapshot_dir(workspace, snap_name);

    if !snap_dir.exists() {
        return Err(GraftError::SnapshotNotFound {
            workspace: workspace.name.clone(),
            snapshot: snap_name.to_string(),
        });
    }

    let snap_upper = snap_dir.join("upper");

    if workspace.upper.exists() {
        let entries = fs::read_dir(&workspace.upper)
            .io_context(|| format!("read upper directory {}", workspace.upper.display()))?;

        for entry in entries {
            let entry = entry
                .io_context(|| "read directory entry".to_string())?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)
                    .io_context(|| format!("remove {}", path.display()))?;
            } else {
                fs::remove_file(&path)
                    .io_context(|| format!("remove {}", path.display()))?;
            }
        }
    } else {
        fs::create_dir_all(&workspace.upper)
            .io_context(|| format!("create upper directory {}", workspace.upper.display()))?;
    }

    if snap_upper.exists() {
        let entries = fs::read_dir(&snap_upper)
            .io_context(|| format!("read snapshot upper directory {}", snap_upper.display()))?;

        for entry in entries {
            let entry = entry
                .io_context(|| "read directory entry".to_string())?;
            let src = entry.path();
            let dst = workspace.upper.join(entry.file_name());

            if src.is_dir() {
                recursive_copy(&src, &dst, |_| false)?;
            } else {
                fs::copy(&src, &dst).io_context(|| {
                    format!("copy {} -> {}", src.display(), dst.display())
                })?;
            }
        }
    }

    println!("restored snapshot '{snap_name}'");
    Ok(())
}

pub fn diff_snapshot(
    workspace: &Workspace,
    snap_name: &str,
) -> Result<Vec<FileChange>, GraftError> {
    let snap_dir = snapshot_dir(workspace, snap_name);

    if !snap_dir.exists() {
        return Err(GraftError::SnapshotNotFound {
            workspace: workspace.name.clone(),
            snapshot: snap_name.to_string(),
        });
    }

    let snap_upper = snap_dir.join("upper");
    let current_upper = &workspace.upper;

    let mut changes = Vec::new();

    let snap_files = collect_files(&snap_upper);
    let current_files = collect_files(current_upper);

    let snap_set: HashSet<&PathBuf> = snap_files.iter().collect();
    let current_set: HashSet<&PathBuf> = current_files.iter().collect();

    for rel in &current_files {
        if !snap_set.contains(rel) {
            let full_path = current_upper.join(rel);
            let binary = diff::is_binary(&full_path);
            let additions = if binary {
                None
            } else {
                let content = fs::read_to_string(&full_path).unwrap_or_default();
                Some(content.lines().count())
            };
            changes.push(FileChange {
                path: rel.clone(),
                kind: ChangeKind::Added,
                additions,
                deletions: None,
                is_binary: binary,
            });
        }
    }

    for rel in &current_files {
        if snap_set.contains(rel) {
            let current_path = current_upper.join(rel);
            let snap_path = snap_upper.join(rel);
            let binary = diff::is_binary(&current_path);

            if binary {
                // Compare raw bytes
                let current_bytes = fs::read(&current_path).unwrap_or_default();
                let snap_bytes = fs::read(&snap_path).unwrap_or_default();
                if current_bytes != snap_bytes {
                    changes.push(FileChange {
                        path: rel.clone(),
                        kind: ChangeKind::Modified,
                        additions: None,
                        deletions: None,
                        is_binary: true,
                    });
                }
            } else {
                let (additions, deletions) = diff::compute_line_diff(&snap_path, &current_path);
                if additions > 0 || deletions > 0 {
                    changes.push(FileChange {
                        path: rel.clone(),
                        kind: ChangeKind::Modified,
                        additions: Some(additions),
                        deletions: Some(deletions),
                        is_binary: false,
                    });
                }
            }
        }
    }

    for rel in &snap_files {
        if !current_set.contains(rel) {
            changes.push(FileChange {
                path: rel.clone(),
                kind: ChangeKind::Deleted,
                additions: None,
                deletions: None,
                is_binary: false,
            });
        }
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

pub fn delete_snapshot(workspace: &Workspace, snap_name: &str) -> Result<(), GraftError> {
    let snap_dir = snapshot_dir(workspace, snap_name);

    if !snap_dir.exists() {
        return Err(GraftError::SnapshotNotFound {
            workspace: workspace.name.clone(),
            snapshot: snap_name.to_string(),
        });
    }

    fs::remove_dir_all(&snap_dir)
        .io_context(|| format!("delete snapshot '{}'", snap_name))?;

    println!("deleted snapshot '{snap_name}'");
    Ok(())
}

/// Collect all file relative paths under a directory.
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }

    for entry in WalkDir::new(dir).min_depth(1).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(rel) = entry.path().strip_prefix(dir) {
                files.push(rel.to_path_buf());
            }
        }
    }

    files.sort();
    files
}

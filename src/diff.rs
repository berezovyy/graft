use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use walkdir::WalkDir;

use crate::error::GraftError;
use crate::state::State;
use crate::util::{WalkDirExt, Whiteout};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
    pub is_binary: bool,
}

#[derive(Debug, Serialize)]
pub struct DiffOutput {
    pub workspace: String,
    pub base: PathBuf,
    pub changes: Vec<FileChange>,
}

pub enum DiffMode {
    Default,
    Files,
    Full,
    Stat,
    Json,
}

/// Check if a file is binary by looking for null bytes in the first 8KB.
pub fn is_binary(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

/// Compute line-level additions and deletions between two text files.
pub fn compute_line_diff(old: &Path, new: &Path) -> (usize, usize) {
    let old_content = fs::read_to_string(old).unwrap_or_default();
    let new_content = fs::read_to_string(new).unwrap_or_default();

    let diff = TextDiff::from_lines(&old_content, &new_content);
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => additions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }

    (additions, deletions)
}

/// Scan the overlay's upper layer for files that diverged from the base.
/// Whiteout files (.wh.*) are treated as deletions per the OverlayFS protocol.
pub fn walk_upper(workspace: &Workspace) -> Result<Vec<FileChange>, GraftError> {
    let upper = &workspace.upper;
    if !upper.exists() {
        return Ok(Vec::new());
    }

    let mut changes = Vec::new();

    for entry in WalkDir::new(upper).min_depth(1) {
        let entry = entry.to_graft_err(upper)?;

        let rel = entry
            .path()
            .strip_prefix(upper)
            .expect("entry must be under upper dir");

        let file_name = match entry.file_name().to_str() {
            Some(n) => n,
            None => continue,
        };

        if let Some(whiteout) = Whiteout::parse(file_name) {
            match whiteout {
                Whiteout::Opaque => continue,
                Whiteout::Deletion(original_name) => {
                    let deleted_path = if let Some(parent) = rel.parent() {
                        parent.join(&original_name)
                    } else {
                        PathBuf::from(&original_name)
                    };
                    changes.push(FileChange {
                        path: deleted_path,
                        kind: ChangeKind::Deleted,
                        additions: None,
                        deletions: None,
                        is_binary: false,
                    });
                    continue;
                }
            }
        }

        if entry.file_type().is_dir() {
            continue;
        }

        let rel_path = rel.to_path_buf();
        let base_path = workspace.base.join(&rel_path);
        let upper_path = entry.path().to_path_buf();

        let binary = is_binary(&upper_path);

        if base_path.exists() {
            let (additions, deletions) = if binary {
                (None, None)
            } else {
                let (a, d) = compute_line_diff(&base_path, &upper_path);
                // If no actual line changes, skip this file (metadata-only change)
                if a == 0 && d == 0 {
                    continue;
                }
                (Some(a), Some(d))
            };
            changes.push(FileChange {
                path: rel_path,
                kind: ChangeKind::Modified,
                additions,
                deletions,
                is_binary: binary,
            });
        } else {
            let additions = if binary {
                None
            } else {
                let content = fs::read_to_string(&upper_path).unwrap_or_default();
                Some(content.lines().count())
            };
            changes.push(FileChange {
                path: rel_path,
                kind: ChangeKind::Added,
                additions,
                deletions: None,
                is_binary: binary,
            });
        }
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// Format the list of changes according to the given mode.
pub fn format_diff(changes: &[FileChange], mode: &DiffMode) -> String {
    match mode {
        DiffMode::Default => {
            let mut lines = Vec::new();
            for c in changes {
                let icon = match c.kind {
                    ChangeKind::Added => "A".green().to_string(),
                    ChangeKind::Modified => "M".yellow().to_string(),
                    ChangeKind::Deleted => "D".red().to_string(),
                };
                let counts = match (&c.kind, c.additions, c.deletions) {
                    (ChangeKind::Added, Some(a), _) => {
                        format!(" {}", format!("(+{a})").green())
                    }
                    (ChangeKind::Modified, Some(a), Some(d)) => {
                        format!(
                            " {} {}",
                            format!("(+{a})").green(),
                            format!("(-{d})").red()
                        )
                    }
                    (ChangeKind::Deleted, _, _) => String::new(),
                    _ => {
                        if c.is_binary {
                            " [binary]".to_string()
                        } else {
                            String::new()
                        }
                    }
                };
                lines.push(format!("{}  {}{}", icon, c.path.display(), counts));
            }
            lines.join("\n")
        }
        DiffMode::Files => changes
            .iter()
            .map(|c| c.path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        DiffMode::Stat => {
            let total = changes.len();
            let added = changes
                .iter()
                .filter(|c| matches!(c.kind, ChangeKind::Added))
                .count();
            let modified = changes
                .iter()
                .filter(|c| matches!(c.kind, ChangeKind::Modified))
                .count();
            let deleted = changes
                .iter()
                .filter(|c| matches!(c.kind, ChangeKind::Deleted))
                .count();
            format!("{total} files changed, {added} added, {modified} modified, {deleted} deleted")
        }
        DiffMode::Full | DiffMode::Json => {
            // Full and Json are handled separately in the command runner
            String::new()
        }
    }
}

/// Colorize a unified diff string line by line.
fn colorize_unified_diff(plain: &str) -> String {
    let mut out = String::with_capacity(plain.len() + 128);
    for line in plain.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            out.push_str(&format!("{}\n", line.bold()));
        } else if line.starts_with("@@") {
            out.push_str(&format!("{}\n", line.cyan()));
        } else if line.starts_with('+') {
            out.push_str(&format!("{}\n", line.green()));
        } else if line.starts_with('-') {
            out.push_str(&format!("{}\n", line.red()));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Generate a unified diff for a single file change.
pub fn generate_unified_diff(workspace: &Workspace, change: &FileChange) -> String {
    let upper_path = workspace.upper.join(&change.path);
    let base_path = workspace.base.join(&change.path);

    let a_label = format!("a/{}", change.path.display());
    let b_label = format!("b/{}", change.path.display());

    match change.kind {
        ChangeKind::Added => {
            if change.is_binary {
                return format!("Binary file {} added\n", change.path.display());
            }
            let content = fs::read_to_string(&upper_path).unwrap_or_default();
            let mut out = String::new();
            out.push_str(&format!("{}\n", "--- /dev/null".bold()));
            out.push_str(&format!("{}\n", format!("+++ {b_label}").bold()));
            if !content.is_empty() {
                let lines: Vec<&str> = content.lines().collect();
                out.push_str(&format!(
                    "{}\n",
                    format!("@@ -0,0 +1,{} @@", lines.len()).cyan()
                ));
                for line in &lines {
                    out.push_str(&format!("{}\n", format!("+{line}").green()));
                }
            }
            out
        }
        ChangeKind::Deleted => {
            if change.is_binary {
                return format!("Binary file {} deleted\n", change.path.display());
            }
            let content = fs::read_to_string(&base_path).unwrap_or_default();
            let mut out = String::new();
            out.push_str(&format!("{}\n", format!("--- {a_label}").bold()));
            out.push_str(&format!("{}\n", "+++ /dev/null".bold()));
            if !content.is_empty() {
                let lines: Vec<&str> = content.lines().collect();
                out.push_str(&format!(
                    "{}\n",
                    format!("@@ -1,{} +0,0 @@", lines.len()).cyan()
                ));
                for line in &lines {
                    out.push_str(&format!("{}\n", format!("-{line}").red()));
                }
            }
            out
        }
        ChangeKind::Modified => {
            if change.is_binary {
                return format!("Binary file {} modified\n", change.path.display());
            }
            let old_content = fs::read_to_string(&base_path).unwrap_or_default();
            let new_content = fs::read_to_string(&upper_path).unwrap_or_default();

            let diff = TextDiff::from_lines(&old_content, &new_content);
            let udiff = diff
                .unified_diff()
                .context_radius(3)
                .header(&a_label, &b_label)
                .to_string();
            // similar's unified_diff already includes --- / +++ headers
            if !udiff.is_empty() {
                return colorize_unified_diff(&udiff);
            }
            format!(
                "{}\n{}\n",
                format!("--- {a_label}").bold(),
                format!("+++ {b_label}").bold()
            )
        }
    }
}

/// Walk the cumulative upper directories of a stacked workspace chain and detect
/// all file changes relative to the original base (root's base directory).
pub fn walk_cumulative_upper(
    workspace: &Workspace,
    state: &State,
) -> Result<Vec<FileChange>, GraftError> {
    let chain = state.parent_chain(&workspace.name);
    if chain.is_empty() {
        return Ok(Vec::new());
    }

    // The original base is the root workspace's base
    let original_base = &chain[0].base;

    // Collect all upper dirs from root to leaf
    let uppers: Vec<&PathBuf> = chain.iter().map(|ws| &ws.upper).collect();

    // Build cumulative file map: relative path -> absolute path in whichever upper has it last
    // Also track deletions
    let mut file_map: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut deleted: BTreeSet<PathBuf> = BTreeSet::new();

    for upper in &uppers {
        if !upper.exists() {
            continue;
        }

        for entry in WalkDir::new(upper).min_depth(1) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let rel = match entry.path().strip_prefix(upper) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };

            let file_name = match entry.file_name().to_str() {
                Some(n) => n,
                None => continue,
            };

            if let Some(whiteout) = Whiteout::parse(file_name) {
                match whiteout {
                    Whiteout::Opaque => {
                        let opaque_dir = rel.parent().unwrap_or(Path::new(""));
                        let to_remove: Vec<PathBuf> = file_map
                            .keys()
                            .filter(|p| p.starts_with(opaque_dir))
                            .cloned()
                            .collect();
                        for p in to_remove {
                            file_map.remove(&p);
                        }
                        continue;
                    }
                    Whiteout::Deletion(original_name) => {
                        let deleted_path = if let Some(parent) = rel.parent() {
                            parent.join(&original_name)
                        } else {
                            PathBuf::from(&original_name)
                        };
                        file_map.remove(&deleted_path);
                        // Mark as deleted if it exists in the original base
                        if original_base.join(&deleted_path).exists() {
                            deleted.insert(deleted_path);
                        }
                        continue;
                    }
                }
            }

            if entry.file_type().is_dir() {
                continue;
            }

            let abs_path = entry.path().to_path_buf();
            // If this file was previously marked deleted, un-delete it
            deleted.remove(&rel);
            file_map.insert(rel, abs_path);
        }
    }

    let mut changes = Vec::new();

    for (rel_path, upper_path) in &file_map {
        let base_path = original_base.join(rel_path);
        let binary = is_binary(upper_path);

        if base_path.exists() {
            let (additions, deletions) = if binary {
                (None, None)
            } else {
                let (a, d) = compute_line_diff(&base_path, upper_path);
                if a == 0 && d == 0 {
                    continue;
                }
                (Some(a), Some(d))
            };
            changes.push(FileChange {
                path: rel_path.clone(),
                kind: ChangeKind::Modified,
                additions,
                deletions,
                is_binary: binary,
            });
        } else {
            let additions = if binary {
                None
            } else {
                let content = fs::read_to_string(upper_path).unwrap_or_default();
                Some(content.lines().count())
            };
            changes.push(FileChange {
                path: rel_path.clone(),
                kind: ChangeKind::Added,
                additions,
                deletions: None,
                is_binary: binary,
            });
        }
    }

    for del_path in &deleted {
        changes.push(FileChange {
            path: del_path.clone(),
            kind: ChangeKind::Deleted,
            additions: None,
            deletions: None,
            is_binary: false,
        });
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

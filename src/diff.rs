use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use walkdir::WalkDir;

use crate::error::{GraftError, Result};
use crate::state::State;
use crate::overlay::Whiteout;
use crate::util::WalkDirExt;
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

impl FileChange {
    pub(crate) fn deletion(path: PathBuf) -> Self {
        Self {
            path,
            kind: ChangeKind::Deleted,
            additions: None,
            deletions: None,
            is_binary: false,
        }
    }
}

fn whiteout_target_path(rel: &Path, original_name: &str) -> PathBuf {
    if let Some(parent) = rel.parent() {
        parent.join(original_name)
    } else {
        PathBuf::from(original_name)
    }
}

#[derive(Debug, Serialize)]
pub struct DiffOutput {
    pub workspace: String,
    pub base: PathBuf,
    pub changes: Vec<FileChange>,
}

pub enum DiffFormat {
    Default,
    Files,
    Stat,
}

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

fn compute_line_diff(old: &Path, new: &Path) -> (usize, usize) {
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

fn files_differ(a: &Path, b: &Path) -> bool {
    let (Ok(meta_a), Ok(meta_b)) = (fs::metadata(a), fs::metadata(b)) else {
        return true;
    };
    if meta_a.len() != meta_b.len() {
        return true;
    }

    let (Ok(mut fa), Ok(mut fb)) = (fs::File::open(a), fs::File::open(b)) else {
        return true;
    };
    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];
    loop {
        let na = match fa.read(&mut buf_a) {
            Ok(n) => n,
            Err(_) => return true,
        };
        let nb = match fb.read(&mut buf_b) {
            Ok(n) => n,
            Err(_) => return true,
        };
        if na != nb || buf_a[..na] != buf_b[..nb] {
            return true;
        }
        if na == 0 {
            return false;
        }
    }
}

fn classify_file(
    rel_path: PathBuf,
    upper_path: &Path,
    base_path: &Path,
    need_line_counts: bool,
) -> Option<FileChange> {
    if base_path.exists() {
        if need_line_counts {
            let binary = is_binary(upper_path);
            let (additions, deletions) = if binary {
                (None, None)
            } else {
                let (a, d) = compute_line_diff(base_path, upper_path);
                if a == 0 && d == 0 {
                    return None;
                }
                (Some(a), Some(d))
            };
            Some(FileChange {
                path: rel_path,
                kind: ChangeKind::Modified,
                additions,
                deletions,
                is_binary: binary,
            })
        } else {
            if !files_differ(base_path, upper_path) {
                return None;
            }
            Some(FileChange {
                path: rel_path,
                kind: ChangeKind::Modified,
                additions: None,
                deletions: None,
                is_binary: false,
            })
        }
    } else if need_line_counts {
        let binary = is_binary(upper_path);
        let additions = if binary {
            None
        } else {
            Some(fs::read_to_string(upper_path).unwrap_or_default().lines().count())
        };
        Some(FileChange {
            path: rel_path,
            kind: ChangeKind::Added,
            additions,
            deletions: None,
            is_binary: binary,
        })
    } else {
        Some(FileChange {
            path: rel_path,
            kind: ChangeKind::Added,
            additions: None,
            deletions: None,
            is_binary: false,
        })
    }
}

pub fn collect_changes(workspace: &Workspace) -> Result<Vec<FileChange>> {
    collect_changes_inner(workspace, true)
}

pub fn collect_changes_fast(workspace: &Workspace) -> Result<Vec<FileChange>> {
    collect_changes_inner(workspace, false)
}

fn collect_changes_inner(
    workspace: &Workspace,
    need_line_counts: bool,
) -> Result<Vec<FileChange>> {
    let upper = &workspace.upper;
    if !upper.exists() {
        return Ok(Vec::new());
    }

    let mut changes = Vec::new();

    for entry in WalkDir::new(upper).min_depth(1) {
        let entry = entry.walk_context(upper)?;

        let rel = entry
            .path()
            .strip_prefix(upper)
            .map_err(|_| {
                GraftError::Io {
                    context: format!(
                        "entry {} is not under upper dir {}",
                        entry.path().display(),
                        upper.display()
                    ),
                    source: std::io::Error::new(std::io::ErrorKind::Other, "prefix mismatch"),
                }
            })?;

        let file_name = match entry.file_name().to_str() {
            Some(n) => n,
            None => continue,
        };

        if let Some(whiteout) = Whiteout::parse(file_name) {
            match whiteout {
                Whiteout::Opaque => continue,
                Whiteout::Deletion(original_name) => {
                    changes.push(FileChange::deletion(whiteout_target_path(rel, &original_name)));
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

        if let Some(change) = classify_file(rel_path, &upper_path, &base_path, need_line_counts) {
            changes.push(change);
        }
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

pub fn format_diff(changes: &[FileChange], mode: &DiffFormat) -> String {
    match mode {
        DiffFormat::Default => {
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
        DiffFormat::Files => changes
            .iter()
            .map(|c| c.path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        DiffFormat::Stat => {
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
    }
}

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

/// Replays layer stacking across the parent chain to compute cumulative changes
/// relative to the root base directory.
pub(crate) fn collect_cumulative_changes_fast(
    workspace: &Workspace,
    state: &State,
) -> Result<Vec<FileChange>> {
    collect_cumulative_changes_inner(workspace, state, false)
}

pub(crate) fn collect_cumulative_changes(
    workspace: &Workspace,
    state: &State,
) -> Result<Vec<FileChange>> {
    collect_cumulative_changes_inner(workspace, state, true)
}

fn collect_cumulative_changes_inner(
    workspace: &Workspace,
    state: &State,
    need_line_counts: bool,
) -> Result<Vec<FileChange>> {
    let chain = state.parent_chain(&workspace.name);
    if chain.is_empty() {
        return Ok(Vec::new());
    }

    let original_base = &chain[0].base;

    let uppers: Vec<&PathBuf> = chain.iter().map(|ws| &ws.upper).collect();

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
                        let deleted_path = whiteout_target_path(&rel, &original_name);
                        file_map.remove(&deleted_path);
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
            deleted.remove(&rel);
            file_map.insert(rel, abs_path);
        }
    }

    let mut changes = Vec::new();

    for (rel_path, upper_path) in &file_map {
        let base_path = original_base.join(rel_path);
        if let Some(change) = classify_file(rel_path.clone(), upper_path, &base_path, need_line_counts) {
            changes.push(change);
        }
    }

    for del_path in &deleted {
        changes.push(FileChange::deletion(del_path.clone()));
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

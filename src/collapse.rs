use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::GraftError;
use crate::util::{ensure_parent_dir, recursive_copy, IoContext, WalkDirExt, Whiteout};
use crate::workspace::graft_home;

/// Flatten a stack of overlay upper dirs into a single layer.
/// Later layers win. Whiteouts are resolved against the original base.
pub fn collapse_uppers(upper_dirs: &[PathBuf], original_base: &Path) -> Result<PathBuf, GraftError> {
    // Create temp dir inside graft_home for same-filesystem moves
    let tmp_dir = tempfile::tempdir_in(graft_home())
        .io_context(|| "create temp dir for collapse".to_string())?;
    let tmp_path = tmp_dir.path().to_path_buf();
    // Keep the temp dir so it's not deleted on drop
    let _ = tmp_dir.keep();
    let collapsed = tmp_path.join("collapsed_upper");
    fs::create_dir_all(&collapsed)
        .io_context(|| format!("create collapsed_upper dir {}", collapsed.display()))?;

    for upper in upper_dirs {
        if !upper.exists() {
            continue;
        }

        for entry in WalkDir::new(upper).min_depth(1) {
            let entry = entry.to_graft_err(upper)?;

            let rel = entry.path().strip_prefix(upper).unwrap();
            let file_name = match rel.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };

            if let Some(whiteout) = Whiteout::parse(&file_name) {
                match whiteout {
                    Whiteout::Opaque => {
                        let dir_rel = rel.parent().unwrap_or(Path::new(""));
                        let collapsed_dir = collapsed.join(dir_rel);

                        if collapsed_dir.exists() {
                            for child in fs::read_dir(&collapsed_dir)
                                .io_context(|| format!("read dir {}", collapsed_dir.display()))?
                            {
                                let child = child
                                    .io_context(|| "read dir entry".to_string())?;
                                let child_path = child.path();
                                if child_path.is_dir() {
                                    let _ = fs::remove_dir_all(&child_path);
                                } else {
                                    let _ = fs::remove_file(&child_path);
                                }
                            }
                        }

                        let source_dir = upper.join(dir_rel);
                        recursive_copy(&source_dir, &collapsed_dir, |name| name == ".wh..wh..opq")?;

                        // Keep opaque marker if dir exists in base
                        let base_dir = original_base.join(dir_rel);
                        if base_dir.exists() {
                            fs::write(collapsed_dir.join(".wh..wh..opq"), "")
                                .io_context(|| "write opaque whiteout".to_string())?;
                        }

                        continue;
                    }
                    Whiteout::Deletion(target_name) => {
                        let dir_rel = rel.parent().unwrap_or(Path::new(""));
                        let target_in_collapsed = collapsed.join(dir_rel).join(&target_name);
                        let target_in_base = original_base.join(dir_rel).join(&target_name);

                        if target_in_collapsed.exists() {
                            // Layer deletes something a prior layer added
                            if target_in_collapsed.is_dir() {
                                let _ = fs::remove_dir_all(&target_in_collapsed);
                            } else {
                                let _ = fs::remove_file(&target_in_collapsed);
                            }

                            // Also keep whiteout if it exists in base
                            if target_in_base.exists() {
                                let whiteout_dest = collapsed.join(rel);
                                ensure_parent_dir(&whiteout_dest)?;
                                fs::write(&whiteout_dest, "")
                                    .io_context(|| "write whiteout".to_string())?;
                            }
                        } else if target_in_base.exists() {
                            // Need to hide base file
                            let whiteout_dest = collapsed.join(rel);
                            ensure_parent_dir(&whiteout_dest)?;
                            fs::write(&whiteout_dest, "")
                                .io_context(|| "write whiteout".to_string())?;
                        }
                        // else: nothing to hide, skip

                        continue;
                    }
                }
            }

            let dest = collapsed.join(rel);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&dest)
                    .io_context(|| format!("create dir {}", dest.display()))?;
            } else {
                ensure_parent_dir(&dest)?;
                fs::copy(entry.path(), &dest).io_context(|| {
                    format!("copy {} to {}", entry.path().display(), dest.display())
                })?;
                let metadata = entry.metadata().map_err(|e| {
                    let source = e.into_io_error().unwrap_or_else(|| {
                        std::io::Error::other("metadata error")
                    });
                    GraftError::Io {
                        context: "read metadata".to_string(),
                        source,
                    }
                })?;
                fs::set_permissions(&dest, metadata.permissions())
                    .io_context(|| format!("set permissions on {}", dest.display()))?;
            }
        }
    }

    Ok(collapsed)
}

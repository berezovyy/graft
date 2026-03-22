use crate::error::GraftError;
use std::path::Path;

/// Extension trait for adding context to IO errors.
pub trait IoContext<T> {
    fn io_context(self, ctx: impl FnOnce() -> String) -> Result<T, GraftError>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn io_context(self, ctx: impl FnOnce() -> String) -> Result<T, GraftError> {
        self.map_err(|e| GraftError::Io {
            context: ctx(),
            source: e,
        })
    }
}

/// Extension trait for converting walkdir errors into GraftError.
pub trait WalkDirExt {
    fn to_graft_err(self, dir: &Path) -> Result<walkdir::DirEntry, GraftError>;
}

impl WalkDirExt for Result<walkdir::DirEntry, walkdir::Error> {
    fn to_graft_err(self, dir: &Path) -> Result<walkdir::DirEntry, GraftError> {
        self.map_err(|e| {
            let source = e.into_io_error().unwrap_or_else(|| {
                std::io::Error::other("directory walk error")
            });
            GraftError::Io {
                context: format!("failed to walk {}", dir.display()),
                source,
            }
        })
    }
}

/// Create parent directories for a path if they don't exist.
pub fn ensure_parent_dir(path: &Path) -> Result<(), GraftError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).io_context(|| {
            format!("create parent directory {}", parent.display())
        })?;
    }
    Ok(())
}

/// Recursively copy a directory tree from src to dst, with an optional filename filter.
/// If `skip` returns true for a filename, that entry is skipped.
pub fn recursive_copy(
    src: &Path,
    dst: &Path,
    skip: impl Fn(&str) -> bool,
) -> Result<(), GraftError> {
    std::fs::create_dir_all(dst).io_context(|| format!("create directory {}", dst.display()))?;

    for entry in walkdir::WalkDir::new(src).min_depth(1) {
        let entry = entry.to_graft_err(src)?;
        let rel = entry.path().strip_prefix(src).expect("entry must be under src");

        if let Some(name) = rel.file_name().and_then(|n| n.to_str()) {
            if skip(name) {
                continue;
            }
        }

        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .io_context(|| format!("create directory {}", target.display()))?;
        } else {
            ensure_parent_dir(&target)?;
            std::fs::copy(entry.path(), &target).io_context(|| {
                format!("copy {} -> {}", entry.path().display(), target.display())
            })?;
        }
    }
    Ok(())
}

/// Parsed OverlayFS whiteout entry.
pub enum Whiteout {
    /// Opaque directory marker (.wh..wh..opq) — hides all lower entries in this dir
    Opaque,
    /// File/dir deletion marker (.wh.<name>) — hides a specific lower entry
    Deletion(String),
}

impl Whiteout {
    /// Parse a filename into a whiteout type, if it is one.
    pub fn parse(filename: &str) -> Option<Whiteout> {
        if filename == ".wh..wh..opq" {
            Some(Whiteout::Opaque)
        } else { filename.strip_prefix(".wh.").map(|original| Whiteout::Deletion(original.to_string())) }
    }

}

use crate::error::{GraftError, Result};
use std::path::{Path, PathBuf};

pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub fn graft_home() -> PathBuf {
    if let Ok(home) = std::env::var("GRAFT_HOME") {
        PathBuf::from(home)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".graft")
    }
}

pub(crate) trait IoContext<T> {
    fn io_context(self, ctx: impl FnOnce() -> String) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn io_context(self, ctx: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| GraftError::Io {
            context: ctx(),
            source: e,
        })
    }
}

pub(crate) trait WalkDirExt {
    fn walk_context(self, dir: &Path) -> Result<walkdir::DirEntry>;
}

impl WalkDirExt for std::result::Result<walkdir::DirEntry, walkdir::Error> {
    fn walk_context(self, dir: &Path) -> Result<walkdir::DirEntry> {
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

pub(crate) fn ensure_parent_dir(path: &Path) -> Result {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).io_context(|| {
            format!("create parent directory {}", parent.display())
        })?;
    }
    Ok(())
}

pub(crate) fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 || pid == 1 || pid > i32::MAX as u32 {
        return false;
    }
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub(crate) fn glob_matches(pattern: &str, text: &str) -> bool {
    fn do_match(pat: &[char], txt: &[char]) -> bool {
        let (mut p, mut t) = (0, 0);
        let (mut star_p, mut star_t) = (usize::MAX, 0);

        while t < txt.len() {
            if p < pat.len() && (pat[p] == '?' || pat[p] == txt[t]) {
                p += 1;
                t += 1;
            } else if p < pat.len() && pat[p] == '*' {
                star_p = p;
                star_t = t;
                p += 1;
            } else if star_p != usize::MAX {
                p = star_p + 1;
                star_t += 1;
                t = star_t;
            } else {
                return false;
            }
        }
        while p < pat.len() && pat[p] == '*' {
            p += 1;
        }
        p == pat.len()
    }

    let pat_chars: Vec<char> = pattern.chars().collect();
    let txt_chars: Vec<char> = text.chars().collect();
    do_match(&pat_chars, &txt_chars)
}

pub(crate) fn kill_process(pid: u32) {
    if pid == 0 || pid == 1 || pid > i32::MAX as u32 {
        return;
    }
    // Negate to signal the entire process group (kills sh -c wrapper children too)
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

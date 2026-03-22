#![allow(dead_code)]

use std::path::Path;

use graft::workspace::{Workspace, now_rfc3339};

pub fn setup_test_env() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::env::set_var("GRAFT_HOME", dir.path());
    dir
}

pub fn create_test_project(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).expect("failed to create src dir");
    std::fs::write(dir.join("README.md"), "# Test Project\n").expect("failed to write README");
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").expect("failed to write main.rs");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"test-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("failed to write Cargo.toml");
}

/// Create a test workspace with sensible defaults. Pass the graft_home dir as root.
pub fn make_test_workspace(
    root: &Path,
    name: &str,
    base: &Path,
    parent: Option<String>,
) -> Workspace {
    let ws_dir = root.join(name);
    std::fs::create_dir_all(ws_dir.join("upper")).expect("create upper");
    std::fs::create_dir_all(ws_dir.join("work")).expect("create work");
    std::fs::create_dir_all(ws_dir.join("merged")).expect("create merged");

    Workspace {
        name: name.to_string(),
        base: base.to_path_buf(),
        upper: ws_dir.join("upper"),
        work: ws_dir.join("work"),
        merged: ws_dir.join("merged"),
        parent,
        created: now_rfc3339(),
        session: None,
        tmpfs: false,
        tmpfs_size: None,
        ephemeral: false,
    }
}

/// Find fuse-overlayfs, preferring system path.
fn find_fuse_overlayfs() -> &'static str {
    if std::path::Path::new("/usr/bin/fuse-overlayfs").exists() {
        "/usr/bin/fuse-overlayfs"
    } else {
        "fuse-overlayfs"
    }
}

/// Strip ANSI escape codes for test assertions on colorized output.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c2 in chars.by_ref() {
                if c2 == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Unmount and clean up a workspace (best-effort).
pub fn cleanup_workspace(ws: &graft::workspace::Workspace) {
    let _ = graft::overlay::unmount_overlay(&ws.merged);
}

/// Find fusermount, preferring system path (setuid root).
fn find_fusermount() -> &'static str {
    if std::path::Path::new("/usr/bin/fusermount3").exists() {
        "/usr/bin/fusermount3"
    } else if std::path::Path::new("/usr/bin/fusermount").exists() {
        "/usr/bin/fusermount"
    } else {
        "fusermount3"
    }
}

#[macro_export]
macro_rules! skip_without_overlay {
    () => {
        use std::process::Command;
        let fuse_bin = if std::path::Path::new("/usr/bin/fuse-overlayfs").exists() {
            "/usr/bin/fuse-overlayfs"
        } else {
            "fuse-overlayfs"
        };
        let fusermount_bin = if std::path::Path::new("/usr/bin/fusermount3").exists() {
            "/usr/bin/fusermount3"
        } else if std::path::Path::new("/usr/bin/fusermount").exists() {
            "/usr/bin/fusermount"
        } else {
            "fusermount3"
        };
        let tmp = tempfile::tempdir().unwrap();
        let lower = tmp.path().join("lower");
        let upper = tmp.path().join("upper");
        let work = tmp.path().join("work");
        let merged = tmp.path().join("merged");
        std::fs::create_dir_all(&lower).unwrap();
        std::fs::create_dir_all(&upper).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&merged).unwrap();
        let status = Command::new(fuse_bin)
            .arg("-o")
            .arg(&format!(
                "lowerdir={},upperdir={},workdir={}",
                lower.display(),
                upper.display(),
                work.display()
            ))
            .arg(&merged.display().to_string())
            .status();
        match status {
            Ok(s) if s.success() => {
                let _ = Command::new(fusermount_bin)
                    .arg("-u")
                    .arg(&merged)
                    .status();
            }
            _ => {
                eprintln!("skipping test: fuse-overlayfs not available or cannot mount");
                return;
            }
        }
    };
}

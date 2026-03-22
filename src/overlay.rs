use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::{GraftError, Result};

pub(crate) enum Whiteout {
    Opaque,
    Deletion(String),
}

impl Whiteout {
    pub(crate) fn parse(filename: &str) -> Option<Whiteout> {
        if filename == ".wh..wh..opq" {
            Some(Whiteout::Opaque)
        } else {
            filename.strip_prefix(".wh.").map(|name| Whiteout::Deletion(name.to_string()))
        }
    }
}

pub fn find_fuse_overlayfs() -> &'static str {
    if Path::new("/usr/bin/fuse-overlayfs").exists() {
        "/usr/bin/fuse-overlayfs"
    } else {
        "fuse-overlayfs"
    }
}

pub fn find_fusermount() -> &'static str {
    if Path::new("/usr/bin/fusermount3").exists() {
        "/usr/bin/fusermount3"
    } else if Path::new("/usr/bin/fusermount").exists() {
        "/usr/bin/fusermount"
    } else {
        "fusermount3"
    }
}

pub(crate) struct OverlayOpts {
    pub tmpfs: bool,
    pub tmpfs_size: String,
}

impl Default for OverlayOpts {
    fn default() -> Self {
        Self {
            tmpfs: false,
            tmpfs_size: "256m".to_string(),
        }
    }
}

pub(crate) fn mount_tmpfs(path: &Path, size: &str) -> Result {
    if !Path::new("/dev/shm").exists() {
        eprintln!(
            "warning: /dev/shm not available, using regular directory for tmpfs (size={}) at {}",
            size,
            path.display()
        );
    }
    fs::create_dir_all(path).map_err(|e| {
        GraftError::MountFailed(format!("failed to create dir at {}: {}", path.display(), e))
    })
}

pub(crate) fn unmount_tmpfs(path: &Path) -> Result {
    if let Err(e) = fuse_unmount(path) {
        eprintln!("warning: failed to unmount {}: {e}", path.display());
    }
    if path.exists() {
        if let Err(e) = fs::remove_dir_all(path) {
            eprintln!("warning: failed to remove directory {}: {e}", path.display());
        }
    }
    Ok(())
}

pub(crate) fn mount_overlay(
    base: &Path,
    upper: &Path,
    work: &Path,
    merged: &Path,
    opts: &OverlayOpts,
) -> Result {
    if opts.tmpfs {
        mount_tmpfs(upper, &opts.tmpfs_size)?;
        if let Err(e) = mount_tmpfs(work, &opts.tmpfs_size) {
            let _ = unmount_tmpfs(upper);
            return Err(e);
        }
    }

    let opts_str = format!(
        "lowerdir={},upperdir={},workdir={}",
        base.display(),
        upper.display(),
        work.display()
    );

    let result = Command::new(find_fuse_overlayfs())
        .arg("-o")
        .arg(&opts_str)
        .arg(merged.as_os_str())
        .status();

    let err = match result {
        Ok(status) if status.success() => return Ok(()),
        Ok(_status) => GraftError::MountFailed(diagnose_mount_failure(merged)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            GraftError::MountFailed(
                "fuse-overlayfs not found. Install it:\n  \
                 Ubuntu/Debian: sudo apt install fuse-overlayfs\n  \
                 Fedora:        sudo dnf install fuse-overlayfs\n  \
                 Arch:          sudo pacman -S fuse-overlayfs\n  \
                 macOS:         brew install fuse-overlayfs"
                    .to_string(),
            )
        }
        Err(e) => {
            GraftError::MountFailed(format!("failed to run fuse-overlayfs: {}", e))
        }
    };

    if opts.tmpfs {
        let _ = unmount_tmpfs(work);
        let _ = unmount_tmpfs(upper);
    }

    Err(err)
}

fn diagnose_mount_failure(merged: &Path) -> String {
    let mut hints = vec![format!(
        "fuse-overlayfs failed to mount overlay at {}",
        merged.display()
    )];

    if !Path::new("/dev/fuse").exists() {
        hints.push(
            "  /dev/fuse not found. Load the FUSE kernel module:\n    sudo modprobe fuse"
                .to_string(),
        );
        return hints.join("\n");
    }
    if fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fuse")
        .is_err()
    {
        hints.push(
            "  /dev/fuse is not accessible. Fix permissions:\n    \
             sudo chmod 666 /dev/fuse\n  \
             or add yourself to the 'fuse' group:\n    \
             sudo usermod -aG fuse $USER && newgrp fuse"
                .to_string(),
        );
        return hints.join("\n");
    }

    let fm = find_fusermount();
    if let Ok(meta) = fs::metadata(fm) {
        use std::os::unix::fs::MetadataExt;
        let has_setuid = meta.mode() & 0o4000 != 0;
        if !has_setuid {
            hints.push(format!(
                "  {} is not setuid root (required for FUSE mounts).\n  \
                 Fix: use the system fusermount3 instead:\n    \
                 sudo apt install fuse3   # installs setuid /usr/bin/fusermount3",
                fm
            ));
            return hints.join("\n");
        }
    }

    if let Ok(val) = fs::read_to_string(
        "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
    ) {
        if val.trim() == "1" {
            hints.push(
                "  AppArmor is blocking unprivileged user namespaces.\n  \
                 Fix:\n    \
                 sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0"
                    .to_string(),
            );
            return hints.join("\n");
        }
    }

    if let Ok(val) = fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone") {
        if val.trim() == "0" {
            hints.push(
                "  Unprivileged user namespaces are disabled.\n  \
                 Fix:\n    \
                 sudo sysctl -w kernel.unprivileged_userns_clone=1"
                    .to_string(),
            );
            return hints.join("\n");
        }
    }

    hints.push(
        "  Run with RUST_LOG=debug for more details, or try:\n    \
         fuse-overlayfs -o lowerdir=/tmp/test,upperdir=/tmp/test2,workdir=/tmp/test3 /tmp/test4\n  \
         to test manually."
            .to_string(),
    );
    hints.join("\n")
}

pub fn unmount_overlay(merged: &Path) -> Result {
    match fuse_unmount(merged) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not mounted") || msg.contains("not found") {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

fn fuse_unmount(path: &Path) -> Result {
    let fusermount = find_fusermount();
    let result = Command::new(fusermount)
        .arg("-u")
        .arg(path.as_os_str())
        .output();

    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not mounted") || stderr.contains("No such file") {
                return Ok(());
            }
            Err(GraftError::UnmountFailed(format!(
                "{} -u {} failed: {}",
                fusermount,
                path.display(),
                stderr.trim()
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(GraftError::UnmountFailed(
                "fusermount3 not found — install fuse3".to_string(),
            ))
        }
        Err(e) => Err(GraftError::UnmountFailed(format!(
            "failed to run {}: {}",
            fusermount, e
        ))),
    }
}


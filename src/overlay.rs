use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::GraftError;
use crate::util::IoContext;
use crate::workspace::graft_home;

/// Find fuse-overlayfs binary, preferring system path (for setuid fusermount3 compatibility).
fn find_fuse_overlayfs() -> &'static str {
    if Path::new("/usr/bin/fuse-overlayfs").exists() {
        "/usr/bin/fuse-overlayfs"
    } else {
        "fuse-overlayfs"
    }
}

fn find_fusermount() -> &'static str {
    if Path::new("/usr/bin/fusermount3").exists() {
        "/usr/bin/fusermount3"
    } else if Path::new("/usr/bin/fusermount").exists() {
        "/usr/bin/fusermount"
    } else {
        "fusermount3"
    }
}

/// Options for overlay mount configuration.
pub struct OverlayOpts {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OverlayMode {
    Fuse,
    #[serde(alias = "unprivileged", alias = "privileged")]
    Supported,
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub overlay_mode: OverlayMode,
    pub kernel_version: String,
    pub detected_at: String,
    #[serde(default = "default_ttl")]
    pub ttl_hours: u32,
}

fn default_ttl() -> u32 {
    24
}

pub fn mount_tmpfs(path: &Path, _size: &str) -> Result<(), GraftError> {
    // /dev/shm avoids needing root mount privileges
    if !Path::new("/dev/shm").exists() {
        eprintln!(
            "warning: /dev/shm not available, using regular directory for tmpfs at {}",
            path.display()
        );
    }
    fs::create_dir_all(path).map_err(|e| {
        GraftError::MountFailed(format!(
            "failed to create dir at {}: {}",
            path.display(),
            e
        ))
    })
}

pub fn unmount_tmpfs(path: &Path) -> Result<(), GraftError> {
    // try fusermount first in case this was a real FUSE mount from an older version
    let _ = fuse_unmount(path);
    // Then clean up the directory
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }
    Ok(())
}

pub fn mount_overlay(
    base: &Path,
    upper: &Path,
    work: &Path,
    merged: &Path,
    opts: &OverlayOpts,
) -> Result<(), GraftError> {
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
        Ok(_status) => {
            GraftError::MountFailed(diagnose_mount_failure(merged))
        }
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
            GraftError::MountFailed(format!(
                "failed to run fuse-overlayfs: {}",
                e
            ))
        }
    };

    if opts.tmpfs {
        let _ = unmount_tmpfs(work);
        let _ = unmount_tmpfs(upper);
    }

    Err(err)
}

/// Run diagnostics to produce an actionable error message when fuse-overlayfs fails.
fn diagnose_mount_failure(merged: &Path) -> String {
    let mut hints = vec![format!(
        "fuse-overlayfs failed to mount overlay at {}",
        merged.display()
    )];

    // Check 1: /dev/fuse accessible?
    if !Path::new("/dev/fuse").exists() {
        hints.push(
            "  /dev/fuse not found. Load the FUSE kernel module:\n    sudo modprobe fuse"
                .to_string(),
        );
        return hints.join("\n");
    }
    if std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fuse").is_err() {
        hints.push(
            "  /dev/fuse is not accessible. Fix permissions:\n    \
             sudo chmod 666 /dev/fuse\n  \
             or add yourself to the 'fuse' group:\n    \
             sudo usermod -aG fuse $USER && newgrp fuse"
                .to_string(),
        );
        return hints.join("\n");
    }

    // Check 2: fusermount3 setuid?
    let fm = find_fusermount();
    if let Ok(meta) = std::fs::metadata(fm) {
        use std::os::unix::fs::MetadataExt;
        let has_setuid = meta.mode() & 0o4000 != 0;
        if !has_setuid {
            hints.push(format!(
                "  {} is not setuid root (required for FUSE mounts).\n  \
                 Fix: use the system fusermount3 instead:\n    \
                 sudo apt install fuse3   # installs setuid /usr/bin/fusermount3\n  \
                 If you installed fuse via Homebrew, the system version at\n  \
                 /usr/bin/fusermount3 takes priority — make sure it exists.",
                fm
            ));
            return hints.join("\n");
        }
    }

    // Check 3: AppArmor blocking user namespaces?
    if let Ok(val) = std::fs::read_to_string(
        "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
    ) {
        if val.trim() == "1" {
            hints.push(
                "  AppArmor is blocking unprivileged user namespaces.\n  \
                 Fix (allow user namespaces):\n    \
                 sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0\n  \
                 To make permanent:\n    \
                 echo 'kernel.apparmor_restrict_unprivileged_userns=0' | \
                 sudo tee /etc/sysctl.d/99-userns.conf"
                    .to_string(),
            );
            return hints.join("\n");
        }
    }

    // Check 4: unprivileged user namespaces disabled?
    if let Ok(val) =
        std::fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
    {
        if val.trim() == "0" {
            hints.push(
                "  Unprivileged user namespaces are disabled.\n  \
                 Fix:\n    \
                 sudo sysctl -w kernel.unprivileged_userns_clone=1\n  \
                 To make permanent:\n    \
                 echo 'kernel.unprivileged_userns_clone=1' | \
                 sudo tee /etc/sysctl.d/99-userns.conf"
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

pub fn unmount_overlay(merged: &Path) -> Result<(), GraftError> {
    match fuse_unmount(merged) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Check if the error indicates "not mounted" — that's fine
            let msg = e.to_string();
            if msg.contains("not mounted") || msg.contains("not found") {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Run fusermount to unmount a FUSE mount.
/// Prefers system /usr/bin/fusermount3 (setuid root) over Homebrew version.
fn fuse_unmount(path: &Path) -> Result<(), GraftError> {
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

/// Detect the overlay support mode on this system.
pub fn detect_overlay_support() -> OverlayMode {
    // Check for cached capabilities
    if let Some(caps) = load_capabilities() {
        // Check if cached result is still valid (within ttl)
        if let Some(age_secs) = crate::workspace::age_seconds(&caps.detected_at) {
            let age_hours = (age_secs / 3600) as u32;
            if age_hours < caps.ttl_hours {
                return caps.overlay_mode;
            }
        }
    }

    // Check if fuse-overlayfs is available on PATH
    let mode = if fuse_overlayfs_available() {
        // Verify it actually works with a test mount
        match try_test_overlay() {
            Ok(()) => OverlayMode::Fuse,
            Err(_) => OverlayMode::Unsupported,
        }
    } else {
        OverlayMode::Unsupported
    };

    // Cache result
    let caps = Capabilities {
        overlay_mode: mode.clone(),
        kernel_version: get_kernel_version(),
        detected_at: crate::workspace::now_rfc3339(),
        ttl_hours: default_ttl(),
    };
    let _ = save_capabilities(&caps);

    mode
}

fn fuse_overlayfs_available() -> bool {
    Command::new(find_fuse_overlayfs())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Load cached capabilities from graft_home/capabilities.json.
pub fn load_capabilities() -> Option<Capabilities> {
    let path = graft_home().join("capabilities.json");
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save capabilities to graft_home/capabilities.json.
pub fn save_capabilities(caps: &Capabilities) -> Result<(), GraftError> {
    let home = graft_home();
    fs::create_dir_all(&home)
        .io_context(|| format!("create graft home {}", home.display()))?;

    let path = home.join("capabilities.json");
    let json = serde_json::to_string_pretty(caps).map_err(|e| GraftError::StateFailed(format!("failed to serialize capabilities: {e}")))?;
    fs::write(&path, json)
        .io_context(|| format!("write capabilities file {}", path.display()))?;
    Ok(())
}

/// Try a test fuse-overlayfs mount to verify it works.
fn try_test_overlay() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let lower = tmp.path().join("lower");
    let upper = tmp.path().join("upper");
    let work = tmp.path().join("work");
    let merged = tmp.path().join("merged");

    fs::create_dir_all(&lower)?;
    fs::create_dir_all(&upper)?;
    fs::create_dir_all(&work)?;
    fs::create_dir_all(&merged)?;

    let opts = format!(
        "lowerdir={},upperdir={},workdir={}",
        lower.display(),
        upper.display(),
        work.display()
    );

    let status = Command::new(find_fuse_overlayfs())
        .arg("-o")
        .arg(&opts)
        .arg(&merged)
        .status()?;

    if !status.success() {
        return Err("fuse-overlayfs test mount failed".into());
    }

    let _ = Command::new(find_fusermount())
        .arg("-u")
        .arg(&merged)
        .status();

    Ok(())
}

/// Get the kernel version string.
fn get_kernel_version() -> String {
    std::fs::read_to_string("/proc/version")
        .ok()
        .and_then(|s| s.split_whitespace().nth(2).map(String::from))
        .unwrap_or_else(|| "unknown".to_string())
}

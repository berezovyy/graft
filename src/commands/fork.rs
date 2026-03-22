use std::fs;
use std::path::PathBuf;

use crate::error::GraftError;
use crate::overlay::{mount_overlay, OverlayOpts};
use crate::state::State;
use crate::util::IoContext;
use crate::workspace::Workspace;

/// Core fork logic: creates a new workspace with an overlay mount.
/// Reusable by `enter --new`.
pub fn create_workspace(
    base: PathBuf,
    name: &str,
    parent: Option<String>,
    session: Option<String>,
    tmpfs: bool,
    size: Option<String>,
) -> Result<Workspace, GraftError> {
    let (resolved_base, resolved_parent) = {
        let state = State::load()?;
        if let Some(ws) = state.get_workspace(base.to_str().unwrap_or("")) {
            (ws.merged.clone(), Some(ws.name.clone()))
        } else {
            (base.clone(), parent)
        }
    };

    if !resolved_base.exists() {
        return Err(GraftError::Io {
            context: format!("base path does not exist: {}", resolved_base.display()),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "base path not found"),
        });
    }
    if !resolved_base.is_dir() {
        return Err(GraftError::Io {
            context: format!("base path is not a directory: {}", resolved_base.display()),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a directory"),
        });
    }

    let resolved_base = resolved_base.canonicalize().io_context(|| {
        format!("failed to canonicalize base path: {}", resolved_base.display())
    })?;

    let mut ws = Workspace::new(name, resolved_base.clone(), resolved_parent);
    if let Some(ref s) = session {
        ws.session = Some(s.clone());
    }

    fs::create_dir_all(&ws.upper).io_context(|| {
        format!("failed to create upper dir: {}", ws.upper.display())
    })?;
    fs::create_dir_all(&ws.work).io_context(|| {
        format!("failed to create work dir: {}", ws.work.display())
    })?;
    fs::create_dir_all(&ws.merged).io_context(|| {
        format!("failed to create merged dir: {}", ws.merged.display())
    })?;

    let opts = if tmpfs {
        OverlayOpts {
            tmpfs: true,
            tmpfs_size: size.unwrap_or_else(|| "256m".to_string()),
        }
    } else {
        OverlayOpts::default()
    };

    ws.tmpfs = opts.tmpfs;
    if opts.tmpfs {
        ws.tmpfs_size = Some(opts.tmpfs_size.clone());
    }

    if let Err(e) = mount_overlay(&resolved_base, &ws.upper, &ws.work, &ws.merged, &opts) {
        let _ = fs::remove_dir_all(&ws.merged);
        let _ = fs::remove_dir_all(&ws.work);
        let _ = fs::remove_dir_all(&ws.upper);
        // Also try to remove the workspace root dir if empty
        let root = ws.upper.parent().unwrap();
        let _ = fs::remove_dir(root);
        return Err(e);
    }

    State::with_state(|state| {
        state.add_workspace(ws.clone())?;
        Ok(())
    })?;

    Ok(ws)
}

pub fn run(args: crate::cli::ForkArgs) -> Result<(), GraftError> {
    let ws = create_workspace(args.base, &args.name, None, args.session, args.tmpfs, args.size)?;
    println!("created workspace '{}' from {}", ws.name, ws.merged.display());
    Ok(())
}

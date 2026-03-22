use std::fs;
use std::io::BufRead;
use std::path::PathBuf;

use crate::error::{GraftError, Result};
use crate::overlay::{mount_overlay, OverlayOpts};
use crate::state::State;
use crate::util::IoContext;
use crate::workspace::Workspace;

fn apply_graftclean(merged: &std::path::Path) {
    let cleanfile = merged.join(".graftclean");
    let file = match fs::File::open(&cleanfile) {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut cleaned = 0usize;
    for line in std::io::BufReader::new(file).lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let target = merged.join(&line);
        if target.exists() {
            let _ = fs::remove_file(&target);
            cleaned += 1;
        }
    }
    if cleaned > 0 {
        eprintln!("graftclean: removed {} overlay-incompatible file(s)", cleaned);
    }
}

pub fn create_workspace(
    base: PathBuf,
    name: &str,
    parent: Option<String>,
    session: Option<String>,
    tmpfs: bool,
    size: Option<String>,
) -> Result<Workspace> {
    let (resolved_base, resolved_parent) = {
        let state = State::load()?;
        if state.workspaces.contains_key(name) {
            return Err(GraftError::WorkspaceExists(name.to_string()));
        }
        if let Some(ws) = state.workspaces.get(base.to_str().unwrap_or("")) {
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
        if let Some(root) = ws.upper.parent() {
            let _ = fs::remove_dir(root);
        }
        return Err(e);
    }

    apply_graftclean(&ws.merged);

    State::with_state(|state| {
        state.add_workspace(ws.clone())?;
        Ok(())
    })?;

    Ok(ws)
}

pub fn exec(args: crate::cli::ForkArgs) -> Result {
    let name = match args.name {
        Some(n) => n,
        None => args
            .base
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| GraftError::InvalidArgument(
                "cannot derive workspace name from path; use --name".to_string(),
            ))?,
    };
    let ws = create_workspace(args.base, &name, None, args.session, args.tmpfs, args.size)?;
    println!("created workspace '{}' from {}", ws.name, ws.merged.display());
    Ok(())
}

use std::io::{self, Read as _};
use std::path::PathBuf;
use std::process::Command;

use crate::commands::{drop as drop_cmd, fork};
use crate::diff;
use crate::error::GraftError;
use crate::merge::{self, MergeOpts};
use crate::state::State;
use crate::util::IoContext;

/// Generate an ephemeral workspace name: "ephemeral-" + 8 hex chars.
fn generate_ephemeral_name() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    let hash = hasher.finish();
    format!("ephemeral-{:08x}", hash as u32)
}

/// Read a single character from stdin (blocking).
fn read_single_char() -> Option<char> {
    let mut buf = [0u8; 1];
    match io::stdin().read_exact(&mut buf) {
        Ok(()) => Some(buf[0] as char),
        Err(_) => None,
    }
}

pub fn run(args: crate::cli::EnterArgs) -> Result<(), GraftError> {
    let ws_name = if args.ephemeral {
        let eph_name = args.name.unwrap_or_else(generate_ephemeral_name);

        let base = match args.from {
            Some(ref f) => PathBuf::from(f),
            None => PathBuf::from("."),
        };

        let mut ws = fork::create_workspace(base, &eph_name, None, None, args.tmpfs, None)?;
        ws.ephemeral = true;

        State::with_state(|state| {
            if let Some(stored) = state.get_workspace_mut(&eph_name) {
                stored.ephemeral = true;
            }
            Ok(())
        })?;

        println!(
            "created ephemeral workspace '{}' from {}",
            ws.name,
            ws.merged.display()
        );

        eph_name
    } else if args.create {
        let ws_name = args.name.ok_or_else(|| {
            GraftError::InvalidArgument("workspace name required when using --new".to_string())
        })?;

        let base = match args.from {
            Some(ref f) => PathBuf::from(f),
            None => PathBuf::from("."),
        };

        let ws = fork::create_workspace(base, &ws_name, None, None, args.tmpfs, None)?;
        println!(
            "created workspace '{}' from {}",
            ws.name,
            ws.merged.display()
        );

        ws_name
    } else {
        args.name.ok_or_else(|| {
            GraftError::InvalidArgument(
                "workspace name required — use --new to create one".to_string(),
            )
        })?
    };

    let state = State::load()?;
    let workspace = state.require_workspace(&ws_name)?;

    let mut cmd = if !args.command.is_empty() {
        let mut c = Command::new(&args.command[0]);
        c.args(&args.command[1..]);
        c
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        Command::new(shell)
    };

    cmd.current_dir(&workspace.merged)
        .env("GRAFT_WORKSPACE", &workspace.name)
        .env("GRAFT_BASE", &workspace.base)
        .env("GRAFT_UPPER", &workspace.upper);

    let status = cmd.status().io_context(|| "failed to spawn command".to_string())?;

    if status.success() {
        println!("left '{}'", ws_name);
    } else {
        let code = status.code().unwrap_or(-1);
        eprintln!("left '{}' (exit code {})", ws_name, code);
    }

    // Handle merge-on-exit before ephemeral drop
    if args.merge_on_exit {
        // Re-load workspace from state (fresh read)
        let state = State::load()?;
        let workspace = state.require_workspace(&ws_name)?;

        let changes = diff::walk_upper(workspace)?;

        if changes.is_empty() {
            println!("clean — no changes to merge");
        } else {
            // Print summary
            let total_add: usize = changes.iter().filter_map(|c| c.additions).sum();
            let total_del: usize = changes.iter().filter_map(|c| c.deletions).sum();
            println!(
                "{} files changed (+{} -{})",
                changes.len(),
                total_add,
                total_del
            );
            eprint!("[m]erge  [d]rop  [k]eep workspace? ");

            let choice = read_single_char().unwrap_or('k');
            eprintln!(); // newline after single-char read

            match choice {
                'm' => {
                    let opts = MergeOpts {
                        commit: false,
                        message: None,
                        drop: false,
                        patch: false,
                        no_install: true,
                    };
                    merge::merge_workspace(workspace, &opts)?;
                    println!("merged changes into base");
                    State::with_state(|state| {
                        drop_cmd::remove_workspace(state, &ws_name)?;
                        Ok(())
                    })?;
                    println!("removed '{}' after merge", ws_name);
                    return Ok(());
                }
                'd' => {
                    State::with_state(|state| {
                        drop_cmd::remove_workspace(state, &ws_name)?;
                        Ok(())
                    })?;
                    println!("removed");
                    return Ok(());
                }
                _ => {
                    println!("keeping workspace");
                }
            }
        }
    }

    // If ephemeral, automatically drop the workspace
    if args.ephemeral {
        State::with_state(|state| {
            drop_cmd::remove_workspace(state, &ws_name)?;
            Ok(())
        })?;
        println!("ephemeral workspace removed");
    }

    Ok(())
}

use std::fs;

use super::drop;
use crate::error::{GraftError, Result};
use crate::state::State;
use crate::util::graft_home;

pub fn exec(args: crate::cli::NukeArgs) -> Result {
    if !args.yes {
        eprint!("this will destroy ALL workspaces and graft state. type 'yes' to confirm: ");
        use std::io::{self, BufRead};
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).map_err(|e| GraftError::StateFailed(format!("failed to read confirmation: {e}")))?;
        if line.trim() != "yes" {
            println!("aborted");
            return Ok(());
        }
    }

    let home = graft_home();

    if !home.exists() {
        println!("nothing to nuke");
        return Ok(());
    }

    if let Ok(mut state) = State::load() {
        let mut names = state.workspace_names();
        state.sorted_deepest_first(&mut names);

        for name in &names {
            if let Err(e) = drop::remove_workspace(&mut state, name) {
                eprintln!("warning: failed to remove workspace '{}': {e}", name);
            }
        }

        if let Some(ref proxy) = state.proxy {
            if let Some(pid) = proxy.proxy_pid {
                crate::util::kill_process(pid);
            }
        }
    }

    if fs::remove_dir_all(&home).is_err() {
        std::thread::sleep(std::time::Duration::from_millis(100));
        fs::remove_dir_all(&home).map_err(|e| GraftError::Io {
            context: format!("remove graft home {}", home.display()),
            source: e,
        })?;
    }

    println!("nuked everything — {}", home.display());
    Ok(())
}

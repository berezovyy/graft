use std::fs;

use crate::error::GraftError;
use crate::overlay::{unmount_overlay, unmount_tmpfs};
use crate::state::State;
use crate::workspace::graft_home;

pub fn run() -> Result<(), GraftError> {
    let home = graft_home();

    if !home.exists() {
        println!("nothing to nuke");
        return Ok(());
    }

    // Try to load state and cleanly unmount everything.
    // If state is corrupted or missing, skip unmount and just nuke the directory.
    if let Ok(state) = State::load() {
        let mut names: Vec<String> = state.list_workspaces().iter().map(|ws| ws.name.clone()).collect();
        state.sorted_deepest_first(&mut names);

        for name in &names {
            if let Some(ws) = state.get_workspace(name) {
                let _ = unmount_overlay(&ws.merged);
                if ws.tmpfs {
                    let _ = unmount_tmpfs(&ws.upper);
                    let _ = unmount_tmpfs(&ws.work);
                }
            }
        }
    }

    if fs::remove_dir_all(&home).is_err() {
        // unmount may not release immediately; brief delay before retry
        std::thread::sleep(std::time::Duration::from_millis(100));
        fs::remove_dir_all(&home).map_err(|e| GraftError::Io {
            context: format!("failed to remove graft home {}: {e}", home.display()),
            source: e,
        })?;
    }

    println!("nuked everything — {}", home.display());
    Ok(())
}

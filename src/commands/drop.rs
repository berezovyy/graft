use std::fs;

use crate::error::GraftError;
use crate::overlay::{unmount_overlay, unmount_tmpfs};
use crate::state::State;
use crate::workspace::graft_home;

/// Core drop logic for a single workspace. Unmounts overlay, removes dirs, removes from state.
/// The caller is responsible for saving state afterward.
pub fn remove_workspace(state: &mut State, name: &str) -> Result<(), GraftError> {
    let ws = state.require_workspace(name)?.clone();

    // may already be unmounted after reboot
    let _ = unmount_overlay(&ws.merged);

    // If tmpfs was used, unmount tmpfs from upper and work dirs
    if ws.tmpfs {
        let _ = unmount_tmpfs(&ws.upper);
        let _ = unmount_tmpfs(&ws.work);
    }

    // dirs may already be gone after reboot
    let ws_dir = graft_home().join(name);
    let _ = fs::remove_dir_all(&ws_dir);

    state.remove_workspace(name)?;

    Ok(())
}

/// Collect all descendants of a workspace recursively.
fn collect_descendants(state: &State, name: &str) -> Vec<String> {
    let mut descendants = Vec::new();
    let mut stack = vec![name.to_string()];

    while let Some(current) = stack.pop() {
        let children: Vec<String> = state
            .children_of(&current)
            .iter()
            .map(|ws| ws.name.clone())
            .collect();
        for child in children {
            stack.push(child.clone());
            descendants.push(child);
        }
    }

    descendants
}

pub fn run(args: crate::cli::DropArgs) -> Result<(), GraftError> {
    if args.all {
        return run_drop_all();
    }

    if args.glob {
        return run_drop_glob(&args.name);
    }

    State::with_state(|state| {
        state.require_workspace(&args.name)?;

        let children: Vec<String> = state
            .children_of(&args.name)
            .iter()
            .map(|ws| ws.name.clone())
            .collect();

        if !children.is_empty() && !args.force {
            return Err(GraftError::HasChildren {
                workspace: args.name.clone(),
                children,
            });
        }

        let mut to_drop = Vec::new();
        if args.force {
            let descendants = collect_descendants(state, &args.name);
            to_drop.extend(descendants);
        }
        to_drop.push(args.name.clone());

        state.sorted_deepest_first(&mut to_drop);

        let count = to_drop.len();
        for ws_name in &to_drop {
            remove_workspace(state, ws_name)?;
        }

        if count == 1 {
            println!("removed '{}'", args.name);
        } else {
            println!("removed {} workspaces (including '{}')", count, args.name);
        }

        Ok(())
    })
}

fn run_drop_all() -> Result<(), GraftError> {
    State::with_state(|state| {
        let mut names: Vec<String> = state.list_workspaces().iter().map(|ws| ws.name.clone()).collect();

        if names.is_empty() {
            println!("no workspaces to remove");
            return Ok(());
        }

        state.sorted_deepest_first(&mut names);

        let count = names.len();
        for ws_name in &names {
            remove_workspace(state, ws_name)?;
        }

        println!("removed all {} workspaces", count);
        Ok(())
    })
}

/// Simple glob matching supporting `*` (any chars) and `?` (single char).
fn glob_matches(pattern: &str, text: &str) -> bool {
    fn do_match(
        pat: &[char],
        txt: &[char],
    ) -> bool {
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

fn run_drop_glob(pattern: &str) -> Result<(), GraftError> {
    State::with_state(|state| {
        let mut matches: Vec<String> = state
            .list_workspaces()
            .iter()
            .filter(|ws| glob_matches(pattern, &ws.name))
            .map(|ws| ws.name.clone())
            .collect();

        if matches.is_empty() {
            println!("no workspaces match '{}'", pattern);
            return Ok(());
        }

        state.sorted_deepest_first(&mut matches);

        let count = matches.len();
        for ws_name in &matches {
            remove_workspace(state, ws_name)?;
        }

        println!("removed {} workspaces matching '{}'", count, pattern);
        Ok(())
    })
}

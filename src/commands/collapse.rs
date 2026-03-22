use std::fs;

use crate::collapse::collapse_uppers;
use crate::commands::drop::remove_workspace;
use crate::error::GraftError;
use crate::overlay::{mount_overlay, unmount_overlay, OverlayOpts};
use crate::state::State;
use crate::util::IoContext;

pub fn run(name: String) -> Result<(), GraftError> {
    State::with_state(|state| {
        let ws = state.require_workspace(&name)?.clone();

        let chain: Vec<String> = state
            .parent_chain(&name)
            .iter()
            .map(|w| w.name.clone())
            .collect();

        if chain.len() <= 1 {
            return Err(GraftError::NotStackedWorkspace(name.clone()));
        }

        let children: Vec<String> = state
            .children_of(&name)
            .iter()
            .map(|w| w.name.clone())
            .collect();
        if !children.is_empty() {
            return Err(GraftError::HasChildren {
                workspace: name.clone(),
                children,
            });
        }

        // collapsing would orphan sibling workspaces that depend on intermediate layers
        let chain_set: std::collections::HashSet<&str> =
            chain.iter().map(|s| s.as_str()).collect();
        for parent_name in &chain[..chain.len() - 1] {
            let parent_children: Vec<String> = state
                .children_of(parent_name)
                .iter()
                .map(|w| w.name.clone())
                .filter(|n| !chain_set.contains(n.as_str()))
                .collect();
            if !parent_children.is_empty() {
                return Err(GraftError::HasChildren {
                    workspace: parent_name.clone(),
                    children: parent_children,
                });
            }
        }

        let upper_dirs: Vec<std::path::PathBuf> = chain
            .iter()
            .map(|n| state.require_workspace(n).unwrap().upper.clone())
            .collect();

        let root_name = &chain[0];
        let original_base = state.require_workspace(root_name).unwrap().base.clone();

        let collapsed_upper = collapse_uppers(&upper_dirs, &original_base)?;

        let _ = unmount_overlay(&ws.merged);

        let upper_bak = ws.upper.with_extension("bak");
        if upper_bak.exists() {
            let _ = fs::remove_dir_all(&upper_bak);
        }
        fs::rename(&ws.upper, &upper_bak).io_context(|| {
            "failed to back up upper dir".to_string()
        })?;

        fs::rename(&collapsed_upper, &ws.upper).io_context(|| {
            "failed to move collapsed upper".to_string()
        })?;

        if let Some(tmp_parent) = collapsed_upper.parent() {
            let _ = fs::remove_dir_all(tmp_parent);
        }

        // overlayfs requires an empty workdir for a fresh mount
        let work_dir = &ws.work;
        if work_dir.exists() {
            let _ = fs::remove_dir_all(work_dir);
        }
        fs::create_dir_all(work_dir).io_context(|| {
            "failed to recreate work dir".to_string()
        })?;

        let opts = OverlayOpts {
            tmpfs: ws.tmpfs,
            tmpfs_size: ws.tmpfs_size.clone().unwrap_or_else(|| "256m".to_string()),
        };
        if let Err(mount_err) = mount_overlay(&original_base, &ws.upper, work_dir, &ws.merged, &opts) {
            let _ = fs::remove_dir_all(&ws.upper);
            let _ = fs::rename(&upper_bak, &ws.upper);
            // Try to remount with original config
            let _ = mount_overlay(&ws.base, &ws.upper, work_dir, &ws.merged, &opts);
            return Err(mount_err);
        }

        let _ = fs::remove_dir_all(&upper_bak);

        // deepest first: unmounting a parent before children would fail
        let intermediates: Vec<String> = chain[..chain.len() - 1].iter().rev().cloned().collect();
        let intermediate_count = intermediates.len();
        for intermediate in &intermediates {
            remove_workspace(state, intermediate)?;
        }

        let ws_mut = state.require_workspace_mut(&name)?;
        ws_mut.parent = None;
        ws_mut.base = original_base.clone();

        println!(
            "collapsed {} layers into '{}' (base: {})",
            intermediate_count + 1,
            name,
            original_base.display()
        );

        Ok(())
    })
}

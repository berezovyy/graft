use crate::cli::DiffArgs;
use crate::diff::{
    collect_changes, collect_changes_fast, collect_cumulative_changes,
    collect_cumulative_changes_fast, format_diff, generate_unified_diff, DiffFormat, DiffOutput,
};
use crate::error::{GraftError, Result};
use crate::state::State;

pub fn exec(args: DiffArgs) -> Result {
    let state = State::load()?;
    let workspace = state.require_workspace(&args.name)?;

    let need_line_counts = !args.stat && !args.files;
    let changes = if args.cumulative {
        if need_line_counts {
            collect_cumulative_changes(workspace, &state)?
        } else {
            collect_cumulative_changes_fast(workspace, &state)?
        }
    } else if need_line_counts {
        collect_changes(workspace)?
    } else {
        collect_changes_fast(workspace)?
    };

    if changes.is_empty() {
        if args.json {
            let output = DiffOutput {
                workspace: args.name.clone(),
                base: workspace.base.clone(),
                changes: Vec::new(),
            };
            println!("{}", serde_json::to_string_pretty(&output)
                .map_err(|e| GraftError::Serialization(e.to_string()))?);
        } else {
            println!("clean — no changes");
        }
        return Ok(());
    }

    if args.json {
        let output = DiffOutput {
            workspace: args.name.clone(),
            base: workspace.base.clone(),
            changes,
        };
        println!("{}", serde_json::to_string_pretty(&output)
                .map_err(|e| GraftError::Serialization(e.to_string()))?);
    } else if args.full {
        for change in &changes {
            print!("{}", generate_unified_diff(workspace, change));
        }
    } else {
        let fmt = if args.files {
            DiffFormat::Files
        } else if args.stat {
            DiffFormat::Stat
        } else {
            DiffFormat::Default
        };
        println!("{}", format_diff(&changes, &fmt));
    }

    Ok(())
}

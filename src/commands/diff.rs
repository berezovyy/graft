use crate::cli::DiffArgs;
use crate::diff::{
    format_diff, generate_unified_diff, walk_cumulative_upper, walk_upper, DiffMode, DiffOutput,
};
use crate::error::GraftError;
use crate::state::State;

pub fn run(args: DiffArgs) -> Result<(), GraftError> {
    let state = State::load()?;
    let workspace = state.require_workspace(&args.name)?;

    if args.other.is_some() {
        return Err(GraftError::NotImplemented);
    }

    let changes = if args.cumulative {
        walk_cumulative_upper(workspace, &state)?
    } else {
        walk_upper(workspace)?
    };

    if changes.is_empty() {
        if args.json {
            let output = DiffOutput {
                workspace: args.name.clone(),
                base: workspace.base.clone(),
                changes: Vec::new(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        } else {
            println!("clean — no changes");
        }
        return Ok(());
    }

    let mode = if args.json {
        DiffMode::Json
    } else if args.full {
        DiffMode::Full
    } else if args.files {
        DiffMode::Files
    } else if args.stat {
        DiffMode::Stat
    } else {
        DiffMode::Default
    };

    match mode {
        DiffMode::Json => {
            let output = DiffOutput {
                workspace: args.name.clone(),
                base: workspace.base.clone(),
                changes,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
        }
        DiffMode::Full => {
            for change in &changes {
                let udiff = generate_unified_diff(workspace, change);
                print!("{udiff}");
            }
        }
        _ => {
            let output = format_diff(&changes, &mode);
            println!("{output}");
        }
    }

    Ok(())
}

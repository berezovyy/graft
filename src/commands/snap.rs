use crate::cli::SnapAction;
use crate::diff::{format_diff, DiffMode};
use crate::error::GraftError;
use crate::snap;
use crate::state::State;

pub fn run(action: SnapAction) -> Result<(), GraftError> {
    match action {
        SnapAction::Create { workspace, name } => {
            let state = State::load()?;
            let ws = state.require_workspace(&workspace)?;

            let info = snap::create_snapshot(ws, name.as_deref())?;
            println!(
                "created snapshot '{}' ({} files, {})",
                info.name,
                info.file_count,
                format_size(info.total_size),
            );
            Ok(())
        }
        SnapAction::List { workspace } => {
            let state = State::load()?;
            let ws = state.require_workspace(&workspace)?;

            let snapshots = snap::list_snapshots(ws)?;

            if snapshots.is_empty() {
                println!("no snapshots for '{workspace}'");
                return Ok(());
            }

            println!(
                "{:<20} {:>5}  {:>10}  AGE",
                "NAME", "FILES", "SIZE"
            );
            for s in &snapshots {
                println!(
                    "{:<20} {:>5}  {:>10}  {}",
                    s.name,
                    s.file_count,
                    format_size(s.total_size),
                    format_age(&s.created),
                );
            }
            Ok(())
        }
        SnapAction::Restore {
            workspace,
            snap_name,
        } => {
            let state = State::load()?;
            let ws = state.require_workspace(&workspace)?;

            snap::restore_snapshot(ws, &snap_name)
        }
        SnapAction::Diff {
            workspace,
            snap_name,
        } => {
            let state = State::load()?;
            let ws = state.require_workspace(&workspace)?;

            let changes = snap::diff_snapshot(ws, &snap_name)?;

            if changes.is_empty() {
                println!("clean since '{snap_name}'");
            } else {
                let output = format_diff(&changes, &DiffMode::Default);
                println!("{output}");
            }
            Ok(())
        }
        SnapAction::Delete {
            workspace,
            snap_name,
        } => {
            let state = State::load()?;
            let ws = state.require_workspace(&workspace)?;

            snap::delete_snapshot(ws, &snap_name)
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_age(created: &str) -> String {
    let Some(secs) = crate::workspace::age_seconds(created) else {
        return "unknown".to_string();
    };

    if secs < 60 {
        return "just now".to_string();
    }

    let minutes = secs / 60;
    if minutes < 60 {
        return if minutes == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{minutes} minutes ago")
        };
    }

    let hours = secs / 3600;
    if hours < 24 {
        return if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{hours} hours ago")
        };
    }

    let days = secs / 86400;
    if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{days} days ago")
    }
}

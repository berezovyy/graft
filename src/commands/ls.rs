use std::path::Path;

use owo_colors::OwoColorize;

use crate::error::GraftError;
use crate::state::State;

/// Shorten a path to be relative to CWD if possible.
fn shorten_path(path: &Path) -> String {
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path.strip_prefix(&cwd) {
            let rel_str = rel.display().to_string();
            if rel_str.is_empty() {
                return ".".to_string();
            }
            return format!("./{}", rel_str);
        }
    }
    path.display().to_string()
}

pub fn run() -> Result<(), GraftError> {
    let state = State::load()?;
    let mut workspaces: Vec<_> = state.list_workspaces();

    if workspaces.is_empty() {
        println!("no workspaces");
        return Ok(());
    }

    // Sort by name for consistent output
    workspaces.sort_by(|a, b| a.name.cmp(&b.name));

    // Compute column values
    let rows: Vec<(String, String, String, String)> = workspaces
        .iter()
        .map(|ws| {
            let origin = if let Some(ref parent) = ws.parent {
                parent.clone()
            } else {
                shorten_path(&ws.base)
            };

            let file_count = ws.count_upper_files();
            let changed = if file_count == 1 {
                "1 file".to_string()
            } else {
                format!("{} files", file_count)
            };

            let session = ws
                .session
                .as_deref()
                .unwrap_or("-")
                .to_string();

            (ws.name.clone(), origin, changed, session)
        })
        .collect();

    // Calculate column widths
    let w_name = rows.iter().map(|r| r.0.len()).max().unwrap_or(0).max(4);
    let w_base = rows.iter().map(|r| r.1.len()).max().unwrap_or(0).max(4);
    let w_changed = rows.iter().map(|r| r.2.len()).max().unwrap_or(0).max(7);

    // Print header
    println!(
        "{:<w_name$}  {:<w_base$}  {:<w_changed$}  {}",
        "NAME".bold(),
        "BASE".bold(),
        "CHANGED".bold(),
        "SESSION".bold(),
    );

    // Print rows
    for (name, base, changed, session) in &rows {
        println!(
            "{:<w_name$}  {:<w_base$}  {:<w_changed$}  {}",
            name.bold().bright_white(),
            base,
            changed,
            session,
        );
    }

    Ok(())
}

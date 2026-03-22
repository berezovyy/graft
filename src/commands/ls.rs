use std::path::Path;

use owo_colors::OwoColorize;

use crate::error::Result;
use crate::state::State;
use crate::util::is_pid_alive;

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

pub fn exec(args: crate::cli::LsArgs) -> Result {
    let state = State::load()?;

    if args.names {
        let mut names: Vec<_> = state.workspaces.keys().collect();
        names.sort();
        for name in names {
            println!("{}", name);
        }
        return Ok(());
    }

    let mut workspaces: Vec<_> = state.workspaces.values().collect();

    if workspaces.is_empty() {
        println!("no workspaces");
        return Ok(());
    }

    workspaces.sort_by(|a, b| a.name.cmp(&b.name));

    let rows: Vec<(String, String, String, String)> = workspaces
        .iter()
        .map(|ws| {
            let origin = if let Some(ref parent) = ws.parent {
                parent.clone()
            } else {
                shorten_path(&ws.base)
            };

            let file_count = ws.changed_file_count();
            let changed = if file_count == 1 {
                "1 file".to_string()
            } else {
                format!("{} files", file_count)
            };

            let proc_info = match &ws.process {
                Some(p) if is_pid_alive(p.pid) => {
                    format!(":{} ({})", p.port, p.command)
                }
                Some(_) => "stopped".to_string(),
                None => "-".to_string(),
            };

            (ws.name.clone(), origin, changed, proc_info)
        })
        .collect();

    let w_name = rows.iter().map(|r| r.0.len()).max().unwrap_or(0).max(4);
    let w_base = rows.iter().map(|r| r.1.len()).max().unwrap_or(0).max(4);
    let w_changed = rows.iter().map(|r| r.2.len()).max().unwrap_or(0).max(7);

    println!(
        "{:<w_name$}  {:<w_base$}  {:<w_changed$}  {}",
        "NAME".bold(),
        "BASE".bold(),
        "CHANGED".bold(),
        "PROCESS".bold(),
    );

    for (name, base, changed, proc_info) in &rows {
        println!(
            "{:<w_name$}  {:<w_base$}  {:<w_changed$}  {}",
            name.bold().bright_white(),
            base,
            changed,
            proc_info,
        );
    }

    Ok(())
}

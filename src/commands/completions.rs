use std::io;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::Cli;
use crate::error::Result;

pub fn exec(shell: Shell) -> Result {
    match shell {
        Shell::Zsh => print!("{}", ZSH_COMPLETIONS),
        _ => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "graft", &mut io::stdout());
        }
    }
    Ok(())
}

const ZSH_COMPLETIONS: &str = r#"#compdef graft

_graft_workspaces() {
    local -a names
    names=(${(f)"$(graft ls --names 2>/dev/null)"})
    _describe 'workspace' names
}

_graft() {
    local -a commands
    commands=(
        'fork:Create a new workspace from a directory'
        'drop:Remove a workspace and unmount its overlay'
        'enter:Open a shell (or run a command) inside a workspace'
        'run:Start a process in a workspace'
        'switch:Switch the active workspace'
        'ls:List all active workspaces'
        'diff:Show what changed in a workspace'
        'merge:Apply workspace changes back to the base directory'
        'nuke:Remove all workspaces and state'
    )

    _arguments -C \
        '--help[Show help]' \
        '--version[Show version]' \
        '1:command:->cmd' \
        '*::arg:->args'

    case "$state" in
        cmd)
            _describe 'command' commands
            ;;
        args)
            case "$words[1]" in
                fork)
                    _arguments \
                        '1:base directory:_directories' \
                        '--name=[Workspace name]:name:' \
                        '--session=[Session identifier]:session:' \
                        '--tmpfs[Back with tmpfs]' \
                        '--size=[tmpfs size limit]:size:' \
                        '--help[Show help]'
                    ;;
                drop)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--force[Force removal]' \
                        '--all[Remove all workspaces]' \
                        '--glob[Treat name as glob pattern]' \
                        '--help[Show help]'
                    ;;
                enter)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--create[Create if not exists]' \
                        '--from=[Base directory]:directory:_directories' \
                        '--ephemeral[Destroy on exit]' \
                        '--tmpfs[Back with tmpfs]' \
                        '--merge-on-exit[Merge changes on exit]' \
                        '--session=[Session identifier]:session:' \
                        '--help[Show help]'
                    ;;
                run)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--port=[App port]:port:' \
                        '--stop[Stop running process]' \
                        '--help[Show help]'
                    ;;
                switch)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--help[Show help]'
                    ;;
                ls)
                    _arguments \
                        '--names[Output only workspace names]' \
                        '--help[Show help]'
                    ;;
                diff)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--stat[Show compact stat summary]' \
                        '--full[Show full unified diff]' \
                        '--files[List changed file paths]' \
                        '--cumulative[Include ancestor layers]' \
                        '--json[Output as JSON]' \
                        '--help[Show help]'
                    ;;
                merge)
                    _arguments \
                        '1:workspace:_graft_workspaces' \
                        '--commit[Create a git commit]' \
                        '-m=[Commit message]:message:' \
                        '--message=[Commit message]:message:' \
                        '--patch[Generate unified patch]' \
                        '--drop[Drop workspace after merge]' \
                        '--no-install[Skip dependency installation]' \
                        '--help[Show help]'
                    ;;
                nuke)
                    _arguments \
                        '-y[Skip confirmation]' \
                        '--yes[Skip confirmation]' \
                        '--help[Show help]'
                    ;;
            esac
            ;;
    esac
}

_graft "$@"
"#;

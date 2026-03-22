use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "fork() for your project directory — instant, zero-copy workspaces",
    long_about = "Graft creates lightweight, copy-on-write workspaces using overlayfs.\n\n\
        Each workspace shares the original directory as a read-only base while capturing \
        all changes in a separate layer. Workspaces are disposable and switch in milliseconds."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new workspace from a directory
    Fork(ForkArgs),

    /// Remove a workspace and unmount its overlay
    Drop(DropArgs),

    /// Open a shell (or run a command) inside a workspace
    Enter(EnterArgs),

    /// Start a process in a workspace (dev server, Electron, etc.)
    Run(RunArgs),

    /// Switch the active workspace (hot-swap via proxy)
    Switch(SwitchArgs),

    /// List all active workspaces
    Ls(LsArgs),

    /// Show what changed in a workspace
    Diff(DiffArgs),

    /// Apply workspace changes back to the base directory
    Merge(MergeArgs),

    /// Remove all workspaces and state
    Nuke(NukeArgs),

    /// Generate shell completions
    #[command(hide = true)]
    Completions(CompletionsArgs),

    /// Internal: run the TCP proxy daemon
    #[command(name = "proxy-daemon", hide = true)]
    ProxyDaemon(ProxyDaemonArgs),
}

#[derive(Args)]
pub struct ForkArgs {
    #[arg(default_value = ".", help = "Directory to fork (defaults to current directory)")]
    pub base: PathBuf,

    #[arg(long, help = "Name for the new workspace (defaults to directory basename)")]
    pub name: Option<String>,

    #[arg(long, help = "Session identifier for grouping related workspaces")]
    pub session: Option<String>,

    #[arg(long, help = "Back the workspace with tmpfs (RAM-only, vanishes on reboot)")]
    pub tmpfs: bool,

    #[arg(long, help = "Size limit for the tmpfs backing store (e.g. 512M, 2G)")]
    pub size: Option<String>,
}

#[derive(Args)]
pub struct DropArgs {
    #[arg(required_unless_present = "all", help = "Name of the workspace to remove")]
    pub name: Option<String>,

    #[arg(long, help = "Force removal even if the workspace has children")]
    pub force: bool,

    #[arg(long, help = "Remove all workspaces")]
    pub all: bool,

    #[arg(long, help = "Treat name as a glob pattern to match multiple workspaces")]
    pub glob: bool,
}

#[derive(Args)]
pub struct EnterArgs {
    #[arg(help = "Name of the workspace to enter (omit to auto-detect)")]
    pub name: Option<String>,

    #[arg(long, alias = "new", help = "Create the workspace if it does not exist")]
    pub create: bool,

    #[arg(long, help = "Base directory when creating a new workspace on entry")]
    pub from: Option<String>,

    #[arg(long, help = "Destroy the workspace automatically on exit")]
    pub ephemeral: bool,

    #[arg(long, help = "Back the workspace with tmpfs (RAM-only, vanishes on reboot)")]
    pub tmpfs: bool,

    #[arg(long, help = "Merge changes back to the base directory when exiting the shell")]
    pub merge_on_exit: bool,

    #[arg(long, help = "Session identifier to set in the spawned shell")]
    pub session: Option<String>,

    #[arg(last = true, help = "Command to run inside the workspace instead of a shell")]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct RunArgs {
    #[arg(help = "Name of the workspace")]
    pub name: String,

    #[arg(long, default_value_t = 3000, help = "Port the app listens on inside the workspace")]
    pub port: u16,

    #[arg(long, help = "Stop the running process instead of starting one")]
    pub stop: bool,

    #[arg(last = true, help = "Command to run (e.g. npm run dev)")]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct SwitchArgs {
    #[arg(help = "Name of the workspace to switch to")]
    pub name: String,
}

#[derive(Args)]
pub struct LsArgs {
    #[arg(long, help = "Output only workspace names, one per line")]
    pub names: bool,
}

#[derive(Args)]
pub struct CompletionsArgs {
    #[arg(help = "Shell to generate completions for (bash, zsh, fish, elvish, powershell)")]
    pub shell: Shell,
}

#[derive(Args)]
pub struct DiffArgs {
    #[arg(help = "Name of the workspace to diff")]
    pub name: String,

    #[arg(long, conflicts_with_all = ["full", "files", "json"], help = "Show a compact stat summary (files changed, insertions, deletions)")]
    pub stat: bool,

    #[arg(long, conflicts_with_all = ["stat", "files", "json"], help = "Show the full unified diff output")]
    pub full: bool,

    #[arg(long, conflicts_with_all = ["stat", "full", "json"], help = "List only the changed file paths")]
    pub files: bool,

    #[arg(long, help = "Include changes from all ancestor layers")]
    pub cumulative: bool,

    #[arg(long, conflicts_with_all = ["stat", "full", "files"], help = "Output diff information as JSON")]
    pub json: bool,
}

#[derive(Args)]
pub struct MergeArgs {
    #[arg(help = "Name of the workspace to merge")]
    pub name: String,

    #[arg(long, help = "Create a git commit with the merged changes")]
    pub commit: bool,

    #[arg(short, long, help = "Commit message (implies --commit)")]
    pub message: Option<String>,

    #[arg(long, help = "Generate a unified patch instead of merging")]
    pub patch: bool,

    #[arg(long, help = "Drop the workspace after a successful merge")]
    pub drop: bool,

    #[arg(long, help = "Skip automatic dependency installation after merge")]
    pub no_install: bool,
}

#[derive(Args)]
pub struct NukeArgs {
    #[arg(long, short = 'y', help = "Skip confirmation prompt")]
    pub yes: bool,
}

#[derive(Args)]
pub struct ProxyDaemonArgs {
    #[arg(long, default_value_t = 4000, help = "Port to listen on")]
    pub port: u16,
}

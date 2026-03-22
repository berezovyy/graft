use clap::{Args, Parser, Subcommand};
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

    /// List all active workspaces
    Ls(LsArgs),

    /// Print the merged directory path of a workspace
    Path(PathArgs),

    /// Show what changed in a workspace
    Diff(DiffArgs),

    /// Open a shell (or run a command) inside a workspace
    Enter(EnterArgs),

    /// Apply workspace changes back to the base directory
    Merge(MergeArgs),

    /// Manage workspace snapshots
    Snap(SnapArgs),

    /// Show workspace hierarchy as a tree
    Tree(TreeArgs),

    /// Flatten a stacked workspace chain into a single layer
    Collapse(CollapseArgs),

    /// Remove all workspaces and state
    Nuke(NukeArgs),
}

#[derive(Args)]
pub struct ForkArgs {
    #[arg(default_value = ".", help = "Base directory to create the workspace from")]
    pub base: PathBuf,

    #[arg(short, long, help = "Name for the new workspace")]
    pub name: String,

    #[arg(long, help = "Session identifier for grouping related workspaces")]
    pub session: Option<String>,

    #[arg(long, help = "Back the workspace with tmpfs (RAM-only, vanishes on reboot)")]
    pub tmpfs: bool,

    #[arg(long, help = "Size limit for the tmpfs backing store (e.g. 512M, 2G)")]
    pub size: Option<String>,
}

#[derive(Args)]
pub struct DropArgs {
    #[arg(help = "Name of the workspace to remove")]
    pub name: String,

    #[arg(long, help = "Force removal even if the workspace is in use")]
    pub force: bool,

    #[arg(long, help = "Remove all workspaces")]
    pub all: bool,

    #[arg(long, help = "Treat name as a glob pattern to match multiple workspaces")]
    pub glob: bool,
}

#[derive(Args)]
pub struct LsArgs;

#[derive(Args)]
pub struct PathArgs {
    #[arg(help = "Name of the workspace")]
    pub name: String,
}

#[derive(Args)]
pub struct DiffArgs {
    #[arg(help = "Name of the workspace to diff")]
    pub name: String,

    #[arg(help = "Optional second workspace to diff against")]
    pub other: Option<String>,

    #[arg(long, help = "Show a compact stat summary (files changed, insertions, deletions)")]
    pub stat: bool,

    #[arg(long, help = "Show the full unified diff output")]
    pub full: bool,

    #[arg(long, help = "List only the changed file paths")]
    pub files: bool,

    #[arg(long, help = "Include changes from all ancestor layers")]
    pub cumulative: bool,

    #[arg(long, help = "Output diff information as JSON")]
    pub json: bool,
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

    #[arg(last = true, help = "Command to run inside the workspace instead of a shell")]
    pub command: Vec<String>,
}

#[derive(Args)]
pub struct MergeArgs {
    #[arg(help = "Name of the workspace to merge")]
    pub name: String,

    #[arg(long, help = "Create a git commit with the merged changes")]
    pub commit: bool,

    #[arg(short, long, help = "Commit message (implies --commit)")]
    pub message: Option<String>,

    #[arg(long, help = "Interactively select hunks to merge (patch mode)")]
    pub patch: bool,

    #[arg(long, help = "Drop the workspace after a successful merge")]
    pub drop: bool,

    #[arg(long, help = "Skip automatic dependency installation after merge")]
    pub no_install: bool,
}

#[derive(Args)]
pub struct SnapArgs {
    #[command(subcommand)]
    pub action: SnapAction,
}

#[derive(Args)]
pub struct TreeArgs;

#[derive(Args)]
pub struct CollapseArgs {
    #[arg(help = "Name of the workspace to collapse into a single layer")]
    pub name: String,
}

#[derive(Args)]
pub struct NukeArgs;

#[derive(Subcommand)]
pub enum SnapAction {
    /// Take a snapshot of the current workspace state
    Create {
        #[arg(help = "Workspace to snapshot")]
        workspace: String,

        #[arg(long, help = "Optional name for the snapshot (auto-generated if omitted)")]
        name: Option<String>,
    },

    /// List all snapshots of a workspace
    List {
        #[arg(help = "Workspace whose snapshots to list")]
        workspace: String,
    },

    /// Restore a workspace to a previous snapshot
    Restore {
        #[arg(help = "Workspace to restore")]
        workspace: String,

        #[arg(help = "Name of the snapshot to restore")]
        snap_name: String,
    },

    /// Show what changed since a snapshot
    Diff {
        #[arg(help = "Workspace to diff")]
        workspace: String,

        #[arg(help = "Snapshot to diff against")]
        snap_name: String,
    },

    /// Delete a snapshot
    Delete {
        #[arg(help = "Workspace that owns the snapshot")]
        workspace: String,

        #[arg(help = "Name of the snapshot to delete")]
        snap_name: String,
    },
}

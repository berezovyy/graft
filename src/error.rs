use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraftError {
    #[error("not yet implemented — coming soon")]
    NotImplemented,

    #[error("workspace '{0}' not found (run 'graft ls' to see available workspaces)")]
    WorkspaceNotFound(String),

    #[error("workspace '{0}' already exists (use 'graft enter {0}' to enter it)")]
    WorkspaceExists(String),

    #[error("mount failed: {0}")]
    MountFailed(String),

    #[error("unmount failed: {0}")]
    UnmountFailed(String),

    #[error("state operation failed: {0}")]
    StateFailed(String),

    #[error("state corrupted: {0}")]
    StateCorrupted(String),

    #[error("lock failed: {0}")]
    LockFailed(String),

    #[error("workspace '{workspace}' has children: {children:?} — use --force to remove all, or drop children first")]
    HasChildren {
        workspace: String,
        children: Vec<String>,
    },

    #[error("snapshot not found: workspace={workspace}, snapshot={snapshot}")]
    SnapshotNotFound {
        workspace: String,
        snapshot: String,
    },

    #[error("package manager failed: {0}")]
    PackageManagerFailed(String),

    #[error("git operation failed: {0}")]
    GitFailed(String),

    #[error("'{0}' has no parent layers to collapse")]
    NotStackedWorkspace(String),

    #[error("{0}")]
    InvalidArgument(String),

    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

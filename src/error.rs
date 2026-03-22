use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraftError {
    #[error("workspace '{0}' not found (run 'graft ls' to see available workspaces)")]
    WorkspaceNotFound(String),

    #[error("workspace '{0}' already exists (use 'graft enter {0}' to enter it)")]
    WorkspaceExists(String),

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

    #[error("mount failed: {0}")]
    MountFailed(String),

    #[error("unmount failed: {0}")]
    UnmountFailed(String),

    #[error("process failed: {0}")]
    ProcessFailed(String),

    #[error("proxy failed: {0}")]
    ProxyFailed(String),

    #[error("no available ports in range 5501-5600")]
    PortRangeExhausted,

    #[error("process already running in workspace '{0}' — stop it first with `graft run {0} --stop`")]
    ProcessAlreadyRunning(String),

    #[error("no process running in workspace '{0}'")]
    NoProcessRunning(String),

    #[error("package manager failed: {0}")]
    PackageManagerFailed(String),

    #[error("git operation failed: {0}")]
    GitFailed(String),

    #[error("{0}")]
    InvalidArgument(String),

    #[error("serialization failed: {0}")]
    Serialization(String),

    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T = ()> = std::result::Result<T, GraftError>;

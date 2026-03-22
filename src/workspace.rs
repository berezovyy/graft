use crate::util::{graft_home, now_rfc3339};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningProcess {
    pub pid: u32,
    pub command: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub base: PathBuf,
    pub upper: PathBuf,
    pub work: PathBuf,
    pub merged: PathBuf,
    pub parent: Option<String>,
    pub created: String,
    pub session: Option<String>,
    #[serde(default)]
    pub tmpfs: bool,
    #[serde(default)]
    pub tmpfs_size: Option<String>,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<RunningProcess>,
}

impl Workspace {
    pub fn new(name: &str, base: PathBuf, parent: Option<String>) -> Self {
        let root = graft_home().join(name);
        Self {
            name: name.to_string(),
            base,
            upper: root.join("upper"),
            work: root.join("work"),
            merged: root.join("merged"),
            parent,
            created: now_rfc3339(),
            session: None,
            tmpfs: false,
            tmpfs_size: None,
            ephemeral: false,
            process: None,
        }
    }

    pub(crate) fn changed_file_count(&self) -> usize {
        if !self.upper.exists() {
            return 0;
        }
        WalkDir::new(&self.upper)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|name| !name.starts_with(".wh."))
            })
            .count()
    }
}

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;

use serde::{Deserialize, Serialize};

use crate::error::{GraftError, Result};
use crate::util::graft_home;
use crate::workspace::Workspace;

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_port: u16,
    pub active_workspace: Option<String>,
    pub target_port: Option<u16>,
    #[serde(default)]
    pub proxy_pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub workspaces: HashMap<String, Workspace>,
    #[serde(default)]
    pub proxy: Option<ProxyConfig>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            workspaces: HashMap::new(),
            proxy: None,
        }
    }
}

impl State {
    pub fn load() -> Result<State> {
        let path = graft_home().join("state.json");
        if !path.exists() {
            return Ok(State::default());
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| GraftError::Io {
                context: format!("read state file {}", path.display()),
                source: e,
            })?;

        if content.trim().is_empty() {
            return Ok(State::default());
        }

        let state: State = serde_json::from_str(&content)
            .map_err(|e| GraftError::StateCorrupted(format!("invalid JSON in state file: {e}")))?;

        if state.version > 1 {
            eprintln!(
                "warning: state file has version {}, this binary only knows version 1",
                state.version
            );
        }

        Ok(state)
    }

    pub fn save(&self) -> Result {
        let path = graft_home().join("state.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| GraftError::Io {
                    context: format!("create state directory {}", parent.display()),
                    source: e,
                })?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| GraftError::StateFailed(format!("failed to serialize state: {e}")))?;

        let tmp_path = path.with_extension("json.tmp");
        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| GraftError::Io {
                context: format!("create temp state file {}", tmp_path.display()),
                source: e,
            })?;
        file.write_all(json.as_bytes())
            .map_err(|e| GraftError::Io {
                context: format!("write state file {}", tmp_path.display()),
                source: e,
            })?;
        file.sync_all()
            .map_err(|e| GraftError::Io {
                context: format!("sync state file {}", tmp_path.display()),
                source: e,
            })?;
        fs::rename(&tmp_path, &path)
            .map_err(|e| GraftError::Io {
                context: format!("rename {} to {}", tmp_path.display(), path.display()),
                source: e,
            })?;

        Ok(())
    }

    pub fn add_workspace(&mut self, ws: Workspace) -> Result {
        if self.workspaces.contains_key(&ws.name) {
            return Err(GraftError::WorkspaceExists(ws.name.clone()));
        }
        self.workspaces.insert(ws.name.clone(), ws);
        Ok(())
    }

    pub fn remove_workspace(&mut self, name: &str) -> Result {
        if self.workspaces.remove(name).is_none() {
            return Err(GraftError::WorkspaceNotFound(name.to_string()));
        }
        Ok(())
    }

    pub fn children_of(&self, name: &str) -> Vec<&Workspace> {
        self.workspaces
            .values()
            .filter(|ws| ws.parent.as_deref() == Some(name))
            .collect()
    }

    pub fn parent_chain(&self, name: &str) -> Vec<&Workspace> {
        let mut chain = Vec::new();
        let mut current = name;

        while let Some(ws) = self.workspaces.get(current) {
            chain.push(ws);
            match &ws.parent {
                Some(parent) => current = parent,
                None => break,
            }
        }

        chain.reverse();
        chain
    }

    pub fn with_lock<F, R>(f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        let lock_path = graft_home().join("state.lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| GraftError::Io {
                    context: format!("create lock directory {}", parent.display()),
                    source: e,
                })?;
        }

        let lock_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| GraftError::Io {
                context: format!("open lock file {}", lock_path.display()),
                source: e,
            })?;

        let ret = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX) };
        if ret != 0 {
            return Err(GraftError::LockFailed(format!(
                "failed to acquire lock: {}",
                std::io::Error::last_os_error()
            )));
        }

        let result = f();

        drop(lock_file);

        result
    }

    pub fn with_state<F, R>(f: F) -> Result<R>
    where
        F: FnOnce(&mut State) -> Result<R>,
    {
        State::with_lock(|| {
            let mut state = State::load()?;
            let result = f(&mut state)?;
            state.save()?;
            Ok(result)
        })
    }

    pub fn require_workspace(&self, name: &str) -> Result<&Workspace> {
        self.workspaces
            .get(name)
            .ok_or_else(|| GraftError::WorkspaceNotFound(name.to_string()))
    }

    pub fn require_workspace_mut(&mut self, name: &str) -> Result<&mut Workspace> {
        self.workspaces
            .get_mut(name)
            .ok_or_else(|| GraftError::WorkspaceNotFound(name.to_string()))
    }

    pub fn depth_of(&self, name: &str) -> usize {
        let mut depth = 0;
        let mut current = name;
        while let Some(ws) = self.workspaces.get(current) {
            match ws.parent.as_deref() {
                Some(parent) => {
                    depth += 1;
                    current = parent;
                }
                None => break,
            }
        }
        depth
    }

    pub fn workspace_names(&self) -> Vec<String> {
        self.workspaces.iter().map(|(name, _)| name.clone()).collect()
    }

    pub fn sorted_deepest_first(&self, names: &mut [String]) {
        names.sort_by_key(|name| std::cmp::Reverse(self.depth_of(name)));
    }
}

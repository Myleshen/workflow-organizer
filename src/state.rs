use std::{collections::HashMap, env, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    default_config_editor, default_editor, default_raycast_terminal, default_terminal, default_vcs,
};

pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub roots: Vec<ScanRoot>,
    #[serde(default)]
    pub usage: HashMap<String, Usage>,
    #[serde(default)]
    pub launchers: Launchers,
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub managed_files: Vec<ManagedFile>,
    #[serde(default)]
    pub cached_projects: Vec<Project>,
    #[serde(default)]
    pub cache_initialized: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Workspace {
    #[serde(default = "default_workspace_enabled")]
    pub enabled: bool,
    #[serde(default = "default_vcs")]
    pub vcs: Vec<String>,
}

fn default_workspace_enabled() -> bool {
    true
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            enabled: default_workspace_enabled(),
            vcs: default_vcs(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    pub opens: u64,
    pub last_opened: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScanRoot {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_project: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub is_worktree: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManagedFile {
    pub project: String,
    pub destination: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Launchers {
    #[serde(default = "default_editor")]
    pub editor: Vec<String>,
    #[serde(default = "default_terminal")]
    pub terminal: Vec<String>,
    #[serde(default = "default_config_editor")]
    pub config_editor: Vec<String>,
    #[serde(default = "default_raycast_terminal")]
    pub raycast_terminal: Vec<String>,
}

impl Default for Launchers {
    fn default() -> Self {
        Self {
            editor: default_editor(),
            terminal: default_terminal(),
            config_editor: default_config_editor(),
            raycast_terminal: default_raycast_terminal(),
        }
    }
}

pub struct Paths {
    pub config_dir: PathBuf,
}

impl Paths {
    pub fn from_environment() -> Result<Self> {
        let config_dir = match env::var_os("XDG_CONFIG_HOME") {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(env::var_os("HOME").context("HOME is not set")?).join(".config"),
        };
        Ok(Self {
            config_dir: config_dir.join("devx"),
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE)
    }

    pub fn overlays_dir(&self, project: &str) -> PathBuf {
        self.config_dir.join("configs").join(project)
    }

    pub fn global_overlays_dir(&self) -> PathBuf {
        self.overlays_dir("global")
    }
}

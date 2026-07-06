//! Named, saved sets of repos (`<data_dir>/workspaces.toml`) — distinct from
//! `config::AppConfig.workspace_root` (the single directory `vulp` scans);
//! a `Workspace` is a user-curated repo list, used by the settings-profile
//! "structure" snapshot (`profile::WorkspaceStructure`) to carry saved
//! groupings between machines.

use crate::config::AppConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One named, saved collection of repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name:       String,
    pub repos:      Vec<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct WorkspacesFile {
    workspaces: Vec<Workspace>,
}

/// Path to the workspaces file, `<data_dir>/workspaces.toml`.
pub fn workspaces_path(cfg: &AppConfig) -> PathBuf {
    cfg.data_dir.join("workspaces.toml")
}

/// Loads every saved workspace (empty if none have been saved yet).
pub fn load_workspaces(cfg: &AppConfig) -> Result<Vec<Workspace>> {
    let path = workspaces_path(cfg);
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        let file: WorkspacesFile = toml::from_str(&raw)?;
        Ok(file.workspaces)
    } else {
        Ok(vec![])
    }
}

/// Overwrites the workspaces file with `workspaces`.
pub fn save_workspaces(workspaces: &[Workspace], cfg: &AppConfig) -> Result<()> {
    let path = workspaces_path(cfg);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = WorkspacesFile { workspaces: workspaces.to_vec() };
    std::fs::write(path, toml::to_string_pretty(&file)?)?;
    Ok(())
}


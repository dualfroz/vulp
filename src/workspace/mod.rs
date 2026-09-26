use crate::config::AppConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

pub fn workspaces_path(cfg: &AppConfig) -> PathBuf {
    cfg.data_dir.join("workspaces.toml")
}

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

pub fn save_workspaces(workspaces: &[Workspace], cfg: &AppConfig) -> Result<()> {
    let path = workspaces_path(cfg);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = WorkspacesFile { workspaces: workspaces.to_vec() };
    std::fs::write(path, toml::to_string_pretty(&file)?)?;
    Ok(())
}

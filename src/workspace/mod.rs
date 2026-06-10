use crate::cli::{WorkspaceAction, WorkspaceArgs};
use crate::config::AppConfig;
use anyhow::Result;
use chrono::Utc;
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

pub fn create_workspace(name: &str, workspaces: &mut Vec<Workspace>, cfg: &AppConfig) -> Result<()> {
    anyhow::ensure!(!workspaces.iter().any(|w| w.name == name), "workspace '{}' already exists", name);
    workspaces.push(Workspace {
        name: name.to_string(),
        repos: vec![],
        created_at: Utc::now().to_rfc3339(),
    });
    save_workspaces(workspaces, cfg)?;
    Ok(())
}

pub fn add_repos_to_workspace(
    name: &str,
    repos: &[String],
    workspaces: &mut Vec<Workspace>,
    cfg: &AppConfig,
) -> Result<()> {
    let ws = workspaces
        .iter_mut()
        .find(|w| w.name == name)
        .ok_or_else(|| anyhow::anyhow!("workspace '{}' not found", name))?;
    for repo in repos {
        if !ws.repos.contains(repo) {
            ws.repos.push(repo.clone());
        }
    }
    save_workspaces(workspaces, cfg)?;
    Ok(())
}

pub fn remove_workspace(name: &str, workspaces: &mut Vec<Workspace>, cfg: &AppConfig) -> Result<()> {
    let before = workspaces.len();
    workspaces.retain(|w| w.name != name);
    anyhow::ensure!(workspaces.len() < before, "workspace '{}' not found", name);
    save_workspaces(workspaces, cfg)?;
    Ok(())
}

/// Map repo names to absolute paths under workspace_root.
pub fn resolve_paths(ws: &Workspace, cfg: &AppConfig) -> Vec<PathBuf> {
    ws.repos.iter()
        .map(|r| cfg.workspace_root.join(r))
        .filter(|p| p.exists())
        .collect()
}

fn print_workspace_table(workspaces: &[Workspace]) {
    println!("{:<20} {:<6}  {}", "NAME", "REPOS", "CREATED");
    println!("{}", "─".repeat(50));
    for ws in workspaces {
        println!("{:<20} {:<6}  {}", ws.name, ws.repos.len(), &ws.created_at[..10]);
    }
}

pub async fn run(args: WorkspaceArgs, cfg: &AppConfig) -> Result<()> {
    let mut workspaces = load_workspaces(cfg)?;

    match args.action {
        None | Some(WorkspaceAction::List) => {
            if workspaces.is_empty() {
                println!("No workspaces yet. Use: fxg workspace create <name>");
            } else {
                print_workspace_table(&workspaces);
            }
        }
        Some(WorkspaceAction::Create { name }) => {
            create_workspace(&name, &mut workspaces, cfg)?;
            println!("✓ Workspace '{}' created", name);
        }
        Some(WorkspaceAction::Add { workspace, repos }) => {
            add_repos_to_workspace(&workspace, &repos, &mut workspaces, cfg)?;
            println!("✓ Added {} repos to workspace '{}'", repos.len(), workspace);
        }
        Some(WorkspaceAction::Remove { name }) => {
            remove_workspace(&name, &mut workspaces, cfg)?;
            println!("✓ Workspace '{}' removed", name);
        }
    }
    Ok(())
}

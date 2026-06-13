use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name:  String,
    pub color: String,        // hex e.g. "#e74c3c"
    #[serde(default)]
    pub repos: Vec<String>,   // relative paths from workspace_root
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectsFile {
    #[serde(default)]
    project: Vec<Project>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".fxg").join("projects.toml")
}

pub fn load() -> Vec<Project> {
    let p = file_path();
    if !p.exists() { return Vec::new(); }
    toml::from_str::<ProjectsFile>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|f| f.project)
        .unwrap_or_default()
}

pub fn save(projects: &[Project]) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    let file = ProjectsFile { project: projects.to_vec() };
    std::fs::write(p, toml::to_string_pretty(&file)?)?;
    Ok(())
}

/// Returns map of relative_repo_path → project index for O(1) lookup.
pub fn index_by_repo(projects: &[Project]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, p) in projects.iter().enumerate() {
        for r in &p.repos {
            m.insert(r.clone(), i);
        }
    }
    m
}

/// Relative path of repo_path from workspace_root, as the key used in projects.
pub fn rel_key(repo_path: &Path, workspace_root: &Path) -> String {
    repo_path.strip_prefix(workspace_root)
        .unwrap_or(repo_path)
        .to_string_lossy()
        .to_string()
}

/// Parse "#rrggbb" → (u8, u8, u8). Falls back to white.
pub fn hex_to_rgb(color: &str) -> (u8, u8, u8) {
    let s = color.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return (r, g, b);
        }
    }
    (180, 180, 180)
}

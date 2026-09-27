use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name:  String,
    pub color: String,
    #[serde(default)]
    pub repos: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectsFile {
    #[serde(default)]
    project: Vec<Project>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("projects.toml")
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

pub fn index_by_repo(projects: &[Project]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, p) in projects.iter().enumerate() {
        for r in &p.repos {
            m.insert(r.clone(), i);
        }
    }
    m
}

pub fn rel_key(repo_path: &Path, workspace_root: &Path) -> String {
    repo_path.strip_prefix(workspace_root)
        .unwrap_or(repo_path)
        .to_string_lossy()
        .to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_parse_with_a_grey_fallback() {
        assert_eq!(hex_to_rgb("#e74c3c"), (0xe7, 0x4c, 0x3c));
        assert_eq!(hex_to_rgb("3DA7FF"), (0x3d, 0xa7, 0xff));
        assert_eq!(hex_to_rgb("#fff"), (180, 180, 180));
        assert_eq!(hex_to_rgb("#zzzzzz"), (180, 180, 180));
    }

    #[test]
    fn repo_keys_are_workspace_relative() {
        let root = Path::new("/ws");
        assert_eq!(rel_key(Path::new("/ws/clients/acme"), root), "clients/acme");
        assert_eq!(rel_key(Path::new("/elsewhere/x"), root), "/elsewhere/x");
    }

    #[test]
    fn index_maps_each_repo_to_its_project() {
        let projects = vec![
            Project { name: "a".into(), color: "#000000".into(), repos: vec!["x".into(), "y".into()] },
            Project { name: "b".into(), color: "#ffffff".into(), repos: vec!["z".into()] },
        ];
        let idx = index_by_repo(&projects);
        assert_eq!(idx.len(), 3);
        assert_eq!(idx["y"], 0);
        assert_eq!(idx["z"], 1);
    }
}

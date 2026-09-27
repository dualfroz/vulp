use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default)]
struct LabelsFile {
    #[serde(default)]
    labels: HashMap<String, String>,
}

fn labels_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".vulp")
        .join("labels.toml")
}

pub fn load_labels(_workspace_root: &Path) -> HashMap<PathBuf, String> {
    let path = labels_path();
    if !path.exists() { return HashMap::new(); }
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let file: LabelsFile = toml::from_str(&raw).unwrap_or_default();
    file.labels.into_iter().map(|(k, v)| (PathBuf::from(k), v)).collect()
}

pub fn save_label(repo_path: &Path, label: &str, _workspace_root: &Path) -> anyhow::Result<()> {
    let path = labels_path();
    let raw = if path.exists() { std::fs::read_to_string(&path)? } else { String::new() };
    let mut file: LabelsFile = toml::from_str(&raw).unwrap_or_default();

    let key = repo_path.to_string_lossy().to_string();
    if label.is_empty() {
        file.labels.remove(&key);
    } else {
        file.labels.insert(key, label.to_string());
    }

    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    std::fs::write(path, toml::to_string_pretty(&file)?)?;
    Ok(())
}

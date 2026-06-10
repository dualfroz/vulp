pub mod ui;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub workspace_root:      PathBuf,
    pub data_dir:            PathBuf,
    pub default_provider:    String,
    pub default_branch:      String,
    pub default_visibility:  String,
    pub editor:              String,
    pub username:            String,
    pub email:               String,
    pub gitignore_template:  String,
    pub readme_template:     String,
    pub share_stats:         bool,
    pub timezone:            String,
    pub vsc_data_dir:        PathBuf,
    /// Folder names inside workspace_root treated as organisational groups.
    /// Their direct children are scanned as repos, not the group folder itself.
    /// Example: ["clients", "internal", "opensource"]
    #[serde(default)]
    pub workspace_groups:    Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
        Self {
            workspace_root:     home.join("projects"),
            data_dir:           home.join(".fxg"),
            default_provider:   "github".into(),
            default_branch:     "main".into(),
            default_visibility: "private".into(),
            editor:             std::env::var("EDITOR").unwrap_or_else(|_| "vim".into()),
            username:           String::new(),
            email:              String::new(),
            gitignore_template: "node".into(),
            readme_template:    "heartbyte-default".into(),
            share_stats:        false,
            timezone:           "Europe/Warsaw".into(),
            vsc_data_dir:       home.join(".fxg/vsc-data"),
            workspace_groups:   vec![],
        }
    }
}

impl AppConfig {
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("workspace_root",     self.workspace_root.to_string_lossy().into()),
            ("data_dir",           self.data_dir.to_string_lossy().into()),
            ("default_provider",   self.default_provider.clone()),
            ("default_branch",     self.default_branch.clone()),
            ("default_visibility", self.default_visibility.clone()),
            ("editor",             self.editor.clone()),
            ("username",           self.username.clone()),
            ("email",              self.email.clone()),
            ("gitignore_template", self.gitignore_template.clone()),
            ("readme_template",    self.readme_template.clone()),
            ("share_stats",        self.share_stats.to_string()),
            ("timezone",           self.timezone.clone()),
            ("vsc_data_dir",       self.vsc_data_dir.to_string_lossy().into()),
            ("workspace_groups",   self.workspace_groups.join(", ")),
        ]
    }

    pub fn set_field(&mut self, key: &str, value: String) -> Result<()> {
        match key {
            "workspace_root"     => self.workspace_root     = PathBuf::from(&value),
            "data_dir"           => self.data_dir           = PathBuf::from(&value),
            "default_provider"   => self.default_provider   = value,
            "default_branch"     => self.default_branch     = value,
            "default_visibility" => self.default_visibility = value,
            "editor"             => self.editor             = value,
            "username"           => self.username           = value,
            "email"              => self.email              = value,
            "gitignore_template" => self.gitignore_template = value,
            "readme_template"    => self.readme_template    = value,
            "share_stats"        => self.share_stats        = value == "true",
            "timezone"           => self.timezone           = value,
            "vsc_data_dir"       => self.vsc_data_dir       = PathBuf::from(&value),
            "workspace_groups"   => {
                self.workspace_groups = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            other => anyhow::bail!("unknown config key: {}", other),
        }
        Ok(())
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".fxg")
        .join("config.toml")
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path();
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&raw)?)
    } else {
        Ok(AppConfig::default())
    }
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let path = config_path();
    ensure_parent(&path)?;
    std::fs::write(path, toml::to_string_pretty(cfg)?)?;
    Ok(())
}

#[allow(dead_code)]
pub fn ensure_data_dir(cfg: &AppConfig) -> Result<()> {
    std::fs::create_dir_all(&cfg.data_dir)?;
    std::fs::create_dir_all(&cfg.vsc_data_dir)?;
    Ok(())
}

fn ensure_parent(p: &Path) -> Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

pub async fn run(cfg: AppConfig) -> Result<()> {
    let updated = ui::run_config_tui(cfg)?;
    save_config(&updated)?;
    Ok(())
}

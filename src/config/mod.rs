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
    #[serde(default = "default_clipboard_cmd")]
    pub clipboard_cmd:       String,
    #[serde(default)]
    pub show_locs:           bool,
    #[serde(default)]
    pub license_holder:      String,
    #[serde(default = "default_license_kind")]
    pub default_license:     String,
    #[serde(default)]
    pub protected_repos:     Vec<String>,
    #[serde(default)]
    pub stats_author_emails: Vec<String>,
    #[serde(default)]
    pub deploy_ssh_cmd:      String,
    #[serde(default)]
    pub deploy_root:         String,
    #[serde(default)]
    pub hide_external_clones: bool,
}

fn default_license_kind() -> String { "mit".into() }

fn default_clipboard_cmd() -> String {
    for cmd in &["wl-copy", "xclip", "pbcopy"] {
        if std::process::Command::new("which").arg(cmd).output().map(|o| o.status.success()).unwrap_or(false) {
            return if *cmd == "xclip" { "xclip -selection clipboard".into() } else { cmd.to_string() };
        }
    }
    String::new()
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
        Self {
            workspace_root:     home.join("projects"),
            data_dir:           home.join(".vulp"),
            default_provider:   "github".into(),
            default_branch:     "main".into(),
            default_visibility: "private".into(),
            editor:             std::env::var("EDITOR").unwrap_or_else(|_| "vim".into()),
            username:           String::new(),
            email:              String::new(),
            gitignore_template: "node".into(),
            readme_template:    "default".into(),
            share_stats:        false,
            timezone:           "Europe/Warsaw".into(),
            vsc_data_dir:       home.join(".vulp/vsc-data"),
            clipboard_cmd:      default_clipboard_cmd(),
            show_locs:          false,
            license_holder:     String::new(),
            default_license:    default_license_kind(),
            protected_repos:    vec![],
            stats_author_emails: vec![],
            deploy_ssh_cmd:     String::new(),
            deploy_root:        String::new(),
            hide_external_clones: false,
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
            ("license_holder",     self.license_holder.clone()),
            ("default_license",    self.default_license.clone()),
            ("protected_repos",    self.protected_repos.join(", ")),
            ("stats_author_emails", self.stats_author_emails.join(", ")),
            ("deploy_ssh_cmd",     self.deploy_ssh_cmd.clone()),
            ("deploy_root",        self.deploy_root.clone()),
            ("hide_external_clones", self.hide_external_clones.to_string()),
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
            "license_holder"     => self.license_holder     = value,
            "default_license"    => self.default_license    = value,
            "protected_repos"    => {
                self.protected_repos = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "stats_author_emails" => {
                self.stats_author_emails = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "deploy_ssh_cmd"     => self.deploy_ssh_cmd     = value,
            "deploy_root"        => self.deploy_root        = value,
            "hide_external_clones" => self.hide_external_clones = value == "true",
            other => anyhow::bail!("unknown config key: {}", other),
        }
        Ok(())
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".vulp")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_field_can_be_set_back() {
        let mut cfg = AppConfig::default();
        for (key, value) in cfg.clone().fields() {
            cfg.set_field(key, value.clone()).unwrap_or_else(|e| panic!("{key}: {e}"));
            let after = cfg.fields().into_iter().find(|(k, _)| *k == key).unwrap().1;
            assert_eq!(after, value, "{key}");
        }
    }

    #[test]
    fn list_and_bool_fields_are_parsed() {
        let mut cfg = AppConfig::default();
        cfg.set_field("protected_repos", " api, ,web ".into()).unwrap();
        assert_eq!(cfg.protected_repos, ["api", "web"]);
        cfg.set_field("stats_author_emails", "me@example.com,alt@example.com".into()).unwrap();
        assert_eq!(cfg.stats_author_emails.len(), 2);
        cfg.set_field("hide_external_clones", "true".into()).unwrap();
        assert!(cfg.hide_external_clones);
        cfg.set_field("share_stats", "yes".into()).unwrap();
        assert!(!cfg.share_stats);
        cfg.set_field("workspace_root", "/tmp/ws".into()).unwrap();
        assert_eq!(cfg.workspace_root, PathBuf::from("/tmp/ws"));
    }

    #[test]
    fn unknown_keys_are_errors() {
        let err = AppConfig::default().set_field("workspace_rot", "x".into()).unwrap_err();
        assert!(err.to_string().contains("workspace_rot"));
    }

    #[test]
    fn minimal_config_file_fills_optional_fields_with_defaults() {
        let raw = r#"
            workspace_root = "/ws"
            data_dir = "/home/u/.vulp"
            default_provider = "gitea"
            default_branch = "main"
            default_visibility = "public"
            editor = "nvim"
            username = "u"
            email = "u@example.com"
            gitignore_template = "rust"
            readme_template = "default"
            share_stats = false
            timezone = "UTC"
            vsc_data_dir = "/home/u/.vulp/vsc-data"
        "#;
        let cfg: AppConfig = toml::from_str(raw).unwrap();
        assert_eq!(cfg.default_provider, "gitea");
        assert_eq!(cfg.default_license, "mit");
        assert!(cfg.protected_repos.is_empty());
        assert!(cfg.deploy_ssh_cmd.is_empty() && cfg.deploy_root.is_empty());
        assert!(!cfg.hide_external_clones);

        let reparsed: AppConfig = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(reparsed.fields(), cfg.fields());
    }

    #[test]
    fn config_with_removed_keys_still_loads() {
        let raw = r#"
            workspace_root = "/ws"
            data_dir = "/home/u/.vulp"
            default_provider = "github"
            default_branch = "main"
            default_visibility = "private"
            editor = "vim"
            username = "u"
            email = "u@example.com"
            gitignore_template = "node"
            readme_template = "default"
            share_stats = false
            timezone = "UTC"
            vsc_data_dir = "/home/u/.vulp/vsc-data"
            workspace_groups = ["clients"]
        "#;
        let cfg: AppConfig = toml::from_str(raw).unwrap();
        assert_eq!(cfg.workspace_root, PathBuf::from("/ws"));
        assert!(!toml::to_string_pretty(&cfg).unwrap().contains("workspace_groups"));
    }
}

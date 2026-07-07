//! Persistent application settings (`~/.vulp/config.toml`): workspace
//! location, defaults for new repos, editor/clipboard preferences, and the
//! one-time migration from the legacy `~/.fxg` data directory.

pub mod ui;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// All persisted user settings. Loaded once at startup and threaded through
/// the rest of the app; the hub's HubSettings tab edits a copy and calls
/// `save_config` to persist changes.
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
    /// Shell command used to copy text to clipboard (e.g. "wl-copy" or "xclip -selection clipboard").
    #[serde(default = "default_clipboard_cmd")]
    pub clipboard_cmd:       String,
    /// Count lines-of-code during workspace scan (slow for large repos; off by default).
    #[serde(default)]
    pub show_locs:           bool,
    /// Copyright holder written into generated LICENSE files
    /// (empty = fall back to `git config user.name`, then `username`).
    #[serde(default)]
    pub license_holder:      String,
    /// Default template for the add-LICENSE action: "mit" or "apache-2.0".
    #[serde(default = "default_license_kind")]
    pub default_license:     String,
    /// Repos (display name or dir basename, case-insensitive) that mutating
    /// CLI commands refuse to touch unless `--force` is passed.
    #[serde(default)]
    pub protected_repos:     Vec<String>,
    /// Author emails counted as "you" for streak/heatmap/digest stats.
    /// Empty = fall back to `git config user.email`, then `email`. GitHub's
    /// no-reply address for each configured account is always matched too.
    #[serde(default)]
    pub stats_author_emails: Vec<String>,
    /// Command (program + args, split on whitespace) that runs its final
    /// argument as a shell command on the deploy host and prints stdout —
    /// e.g. a wrapper around `ssh`. Empty = deploy-drift feature disabled.
    #[serde(default)]
    pub deploy_ssh_cmd:      String,
    /// Root on the deploy host that mirrors `workspace_root` 1:1 (relative
    /// paths past these two roots are identical). Empty = feature disabled.
    #[serde(default)]
    pub deploy_root:         String,
}

fn default_license_kind() -> String { "mit".into() }

/// Picks the first available clipboard tool on PATH as the default
/// `clipboard_cmd`, so a fresh install works without manual configuration
/// on most systems.
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
            readme_template:    "heartbyte-default".into(),
            share_stats:        false,
            timezone:           "Europe/Warsaw".into(),
            vsc_data_dir:       home.join(".vulp/vsc-data"),
            workspace_groups:   vec![],
            clipboard_cmd:      default_clipboard_cmd(),
            show_locs:          false,
            license_holder:     String::new(),
            default_license:    default_license_kind(),
            protected_repos:    vec![],
            stats_author_emails: vec![],
            deploy_ssh_cmd:     String::new(),
            deploy_root:        String::new(),
        }
    }
}

impl AppConfig {
    /// Lists every editable setting as a `(key, display value)` pair, in
    /// the order shown by the Config TUI's field list.
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
            ("license_holder",     self.license_holder.clone()),
            ("default_license",    self.default_license.clone()),
            ("protected_repos",    self.protected_repos.join(", ")),
            ("stats_author_emails", self.stats_author_emails.join(", ")),
            ("deploy_ssh_cmd",     self.deploy_ssh_cmd.clone()),
            ("deploy_root",        self.deploy_root.clone()),
        ]
    }

    /// Sets one field by its `fields()` key name from a freeform string
    /// (as typed into the Config TUI's edit prompt). Errors on an unknown
    /// key rather than silently ignoring a typo.
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

/// Path to the config file, `~/.vulp/config.toml`.
pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".vulp")
        .join("config.toml")
}

/// The pre-rename data directory, kept only so `migrate_legacy_config` can
/// detect and import it once.
fn legacy_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".fxg")
}

/// One-time migration from the old `fxg` data directory: if `~/.vulp` doesn't
/// exist yet but `~/.fxg` does, copy it over wholesale (config, accounts,
/// tokens, projects, workspaces, cache) so upgrading users keep their setup.
/// The old directory is left untouched as a safety fallback.
pub fn migrate_legacy_config() -> Result<Option<PathBuf>> {
    let new_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".vulp");
    let old_dir = legacy_data_dir();
    if new_dir.exists() || !old_dir.exists() {
        return Ok(None);
    }
    copy_dir_recursive(&old_dir, &new_dir)?;

    // The copied config.toml still has data_dir/vsc_data_dir pointing at the
    // old ~/.fxg paths — repoint them at the new location so accounts.toml
    // and friends (which read cfg.data_dir) don't silently keep writing there.
    let cfg_path = new_dir.join("config.toml");
    if cfg_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&cfg_path) {
            if let Ok(mut cfg) = toml::from_str::<AppConfig>(&raw) {
                if cfg.data_dir == old_dir {
                    cfg.data_dir = new_dir.clone();
                }
                if cfg.vsc_data_dir == old_dir.join("vsc-data") {
                    cfg.vsc_data_dir = new_dir.join("vsc-data");
                }
                let _ = std::fs::write(&cfg_path, toml::to_string_pretty(&cfg)?);
            }
        }
    }
    Ok(Some(old_dir))
}

/// Recursively copies every file and subdirectory from `src` into `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Loads the config file, or `AppConfig::default()` if it doesn't exist yet
/// (first run).
pub fn load_config() -> Result<AppConfig> {
    let path = config_path();
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&raw)?)
    } else {
        Ok(AppConfig::default())
    }
}

/// Writes `cfg` to the config file, creating `~/.vulp/` if needed.
pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let path = config_path();
    ensure_parent(&path)?;
    std::fs::write(path, toml::to_string_pretty(cfg)?)?;
    Ok(())
}

/// Creates `cfg.data_dir` and `cfg.vsc_data_dir` if they don't already exist.
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

/// Entry point for `vulp config`: opens the interactive settings TUI and
/// persists whatever the user changed.
pub async fn run(cfg: AppConfig) -> Result<()> {
    let updated = ui::run_config_tui(cfg)?;
    save_config(&updated)?;
    Ok(())
}

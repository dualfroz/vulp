//! LICENSE file generation: embedded MIT / Apache-2.0 templates, holder
//! resolution (config override → git user.name → account username), and the
//! shared write path used by both the hub's [m] action and `vulp repo
//! license`. Writing never overwrites an existing license and never commits.

use crate::config::AppConfig;
use anyhow::Result;
use std::path::Path;

/// Which license template to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseKind { Mit, Apache2 }

impl LicenseKind {
    /// Display / SPDX-style label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mit     => "MIT",
            Self::Apache2 => "Apache-2.0",
        }
    }

    /// Parses a user-supplied kind string ("mit", "apache-2.0", "apache2"...).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "mit"                                   => Some(Self::Mit),
            "apache-2.0" | "apache2" | "apache-2" | "apache" => Some(Self::Apache2),
            _ => None,
        }
    }

    fn template(&self) -> &'static str {
        match self {
            Self::Mit     => include_str!("MIT.txt"),
            Self::Apache2 => include_str!("APACHE-2.0.txt"),
        }
    }

    /// The full LICENSE file body with `{year}`/`{holder}` filled in.
    pub fn render(&self, holder: &str, year: i32) -> String {
        self.template()
            .replace("{year}", &year.to_string())
            .replace("{holder}", holder)
    }
}

/// Outcome of one `write_license` call, so callers can report per-repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome { Written, AlreadyLicensed }

/// Writes a `LICENSE` file at `repo`'s root. Skips (never overwrites) when
/// the repo already has a recognizable license file. Does NOT commit —
/// the user reviews and commits the file like any other change.
pub fn write_license(repo: &Path, kind: LicenseKind, holder: &str, year: i32) -> Result<WriteOutcome> {
    if crate::status::detect_license(repo) {
        return Ok(WriteOutcome::AlreadyLicensed);
    }
    std::fs::write(repo.join("LICENSE"), kind.render(holder, year))?;
    Ok(WriteOutcome::Written)
}

/// Resolves the copyright holder: `license_holder` from config when set,
/// otherwise global `git config user.name`, otherwise the configured
/// account username.
pub fn holder_for(cfg: &AppConfig) -> String {
    let configured = cfg.license_holder.trim();
    if !configured.is_empty() {
        return configured.to_string();
    }
    if let Ok(out) = std::process::Command::new("git")
        .args(["config", "--get", "user.name"])
        .output()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    cfg.username.clone()
}

/// The configured default kind ("mit" unless overridden), falling back to
/// MIT on an unparsable config value rather than erroring.
pub fn default_kind(cfg: &AppConfig) -> LicenseKind {
    LicenseKind::parse(&cfg.default_license).unwrap_or(LicenseKind::Mit)
}

use crate::config::AppConfig;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseKind { Mit, Apache2 }

impl LicenseKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mit     => "MIT",
            Self::Apache2 => "Apache-2.0",
        }
    }

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

    pub fn render(&self, holder: &str, year: i32) -> String {
        self.template()
            .replace("{year}", &year.to_string())
            .replace("{holder}", holder)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome { Written, AlreadyLicensed }

pub fn write_license(repo: &Path, kind: LicenseKind, holder: &str, year: i32) -> Result<WriteOutcome> {
    if crate::status::detect_license(repo) {
        return Ok(WriteOutcome::AlreadyLicensed);
    }
    std::fs::write(repo.join("LICENSE"), kind.render(holder, year))?;
    Ok(WriteOutcome::Written)
}

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

pub fn default_kind(cfg: &AppConfig) -> LicenseKind {
    LicenseKind::parse(&cfg.default_license).unwrap_or(LicenseKind::Mit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_parsing_accepts_common_spellings() {
        for s in ["mit", " MIT "] { assert_eq!(LicenseKind::parse(s), Some(LicenseKind::Mit)); }
        for s in ["apache-2.0", "Apache2", "apache-2", "APACHE"] {
            assert_eq!(LicenseKind::parse(s), Some(LicenseKind::Apache2));
        }
        assert_eq!(LicenseKind::parse("gpl-3.0"), None);
    }

    #[test]
    fn templates_are_filled_in() {
        let mit = LicenseKind::Mit.render("Jane Doe", 2031);
        assert!(mit.starts_with("MIT License"));
        assert!(mit.contains("Copyright (c) 2031 Jane Doe"));
        let apache = LicenseKind::Apache2.render("Jane Doe", 2031);
        assert!(apache.contains("Apache License"));
        assert!(apache.contains("2031") && apache.contains("Jane Doe"));
        assert!(!mit.contains("{year}") && !apache.contains("{holder}"));
    }

    #[test]
    fn writing_never_overwrites_an_existing_license() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(write_license(dir.path(), LicenseKind::Mit, "Jane", 2030).unwrap(), WriteOutcome::Written);
        let written = std::fs::read_to_string(dir.path().join("LICENSE")).unwrap();
        assert!(written.contains("2030 Jane"));
        assert_eq!(write_license(dir.path(), LicenseKind::Apache2, "Other", 2031).unwrap(), WriteOutcome::AlreadyLicensed);
        assert_eq!(std::fs::read_to_string(dir.path().join("LICENSE")).unwrap(), written);

        let copying = tempfile::tempdir().unwrap();
        std::fs::write(copying.path().join("COPYING"), "GPL").unwrap();
        assert_eq!(write_license(copying.path(), LicenseKind::Mit, "Jane", 2030).unwrap(), WriteOutcome::AlreadyLicensed);
        assert!(!copying.path().join("LICENSE").exists());
    }

    #[test]
    fn configured_holder_and_kind_win() {
        let cfg = AppConfig { license_holder: "  Acme Ltd ".into(), default_license: "apache".into(), ..AppConfig::default() };
        assert_eq!(holder_for(&cfg), "Acme Ltd");
        assert_eq!(default_kind(&cfg), LicenseKind::Apache2);
        let cfg = AppConfig { default_license: "unknown".into(), ..AppConfig::default() };
        assert_eq!(default_kind(&cfg), LicenseKind::Mit);
    }
}

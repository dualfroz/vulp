//! Persisted Tier-B stats (`~/.vulp/stats_cache.toml`): daily size samples,
//! kept separate from `meta_cache.toml` on purpose — that file is rewritten
//! wholesale on every save and read fully at startup, so growing it with
//! daily history would slow down every launch for data only the Stats tab
//! needs. Updated only during `[R]`/`vulp refresh`, never on tab entry.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeSample {
    pub date:    chrono::NaiveDate,
    pub git_kb:  u64,
    pub tree_kb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoStatsEntry {
    #[serde(default)]
    pub size_samples: Vec<SizeSample>,
    /// Whether you've ever committed here (`stats::has_own_commit`), measured
    /// on `[R]`. `Some(false)` = a vendored/reference clone you've never
    /// touched; `Some(true)` = yours; `None` = not yet measured or no identity
    /// configured. Drives the external-clone marker and the hide-clones filter.
    #[serde(default)]
    pub authored: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StatsCache {
    #[serde(default)]
    pub repos: HashMap<String, RepoStatsEntry>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("stats_cache.toml")
}

pub fn load() -> StatsCache {
    let p = file_path();
    if !p.exists() { return StatsCache::default(); }
    toml::from_str(&std::fs::read_to_string(p).unwrap_or_default()).unwrap_or_default()
}

pub fn save(cache: &StatsCache) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(cache)?)?;
    Ok(())
}

impl StatsCache {
    /// Records today's size sample for `repo`, replacing any sample already
    /// recorded today rather than appending a duplicate — max 1/day even if
    /// `[R]` runs multiple times in one day.
    pub fn record_size(&mut self, repo: &str, git_kb: u64, tree_kb: u64) {
        let today = chrono::Local::now().date_naive();
        let entry = self.repos.entry(repo.to_string()).or_default();
        entry.size_samples.retain(|s| s.date != today);
        entry.size_samples.push(SizeSample { date: today, git_kb, tree_kb });
        entry.size_samples.sort_by_key(|s| s.date);
    }

    // kept: Sprint D Stats-tab redesign (size/trend display)
    #[allow(dead_code)]
    pub fn latest_size(&self, repo: &str) -> Option<&SizeSample> {
        self.repos.get(repo).and_then(|e| e.size_samples.last())
    }

    /// Records whether `repo` has any of your own commits.
    pub fn record_authored(&mut self, repo: &str, authored: Option<bool>) {
        self.repos.entry(repo.to_string()).or_default().authored = authored;
    }

    /// Repo names classified as external/vendored clones — ones measured on a
    /// past `[R]` to have zero of your commits.
    pub fn external_repo_names(&self) -> std::collections::HashSet<String> {
        self.repos.iter()
            .filter(|(_, e)| e.authored == Some(false))
            .map(|(n, _)| n.clone())
            .collect()
    }
}

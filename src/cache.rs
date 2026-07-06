//! On-disk cache (`~/.vulp/meta_cache.toml`) of provider-API metadata
//! (stars, issue/PR counts, visibility) and the PRs/issues lists themselves,
//! keyed by repo name. Lets the hub show this data immediately on startup
//! without blocking on a fresh API round-trip for every repo.

use crate::providers::{IssueDetail, PrDetail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Cached provider-API fields for one repo. All `Option` since a repo with
/// no configured provider account (or an unauthenticated one) simply has
/// nothing to cache yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoMeta {
    pub stars:       Option<u32>,
    pub open_issues: Option<u32>,
    pub open_prs:    Option<u32>,
    pub visibility:  Option<String>,
    /// "success" | "failure" | "running" (`CiStatus::as_str`), None = unknown.
    pub ci_status:   Option<String>,
    /// Open Dependabot alerts; None = endpoint unavailable for this repo.
    pub dependabot_open: Option<u32>,
    pub fetched_at:  i64,
}

/// Everything cached for one repo: its metadata plus its last-fetched PRs
/// and issues lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheEntry {
    #[serde(default)]
    pub meta:   RepoMeta,
    #[serde(default)]
    pub prs:    Vec<PrDetail>,
    #[serde(default)]
    pub issues: Vec<IssueDetail>,
}

/// The full on-disk cache, keyed by repo name.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetaCache {
    #[serde(default)]
    pub repos: HashMap<String, CacheEntry>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("meta_cache.toml")
}

/// Loads the cache from disk, or an empty one if the file doesn't exist or
/// fails to parse — a corrupt or missing cache is never fatal, since
/// everything in it can be refetched from the provider API.
pub fn load() -> MetaCache {
    let p = file_path();
    if !p.exists() { return MetaCache::default(); }
    toml::from_str(&std::fs::read_to_string(p).unwrap_or_default()).unwrap_or_default()
}

/// Writes the cache to disk, creating `~/.vulp/` if needed.
pub fn save(cache: &MetaCache) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(cache)?)?;
    Ok(())
}

impl MetaCache {
    /// Looks up the cache entry for one repo by name.
    pub fn get(&self, repo_name: &str) -> Option<&CacheEntry> {
        self.repos.get(repo_name)
    }

    /// Replaces the cached metadata for one repo, creating its entry if needed.
    pub fn set_meta(&mut self, repo_name: &str, meta: RepoMeta) {
        self.repos.entry(repo_name.to_string()).or_default().meta = meta;
    }

    /// Replaces the cached PR list for one repo, creating its entry if needed.
    pub fn set_prs(&mut self, repo_name: &str, prs: Vec<PrDetail>) {
        self.repos.entry(repo_name.to_string()).or_default().prs = prs;
    }

    /// Replaces the cached issue list for one repo, creating its entry if needed.
    pub fn set_issues(&mut self, repo_name: &str, issues: Vec<IssueDetail>) {
        self.repos.entry(repo_name.to_string()).or_default().issues = issues;
    }

    /// All PRs across all repos, sorted by updated_ts desc.
    pub fn all_prs(&self) -> Vec<&PrDetail> {
        let mut v: Vec<&PrDetail> = self.repos.values().flat_map(|e| &e.prs).collect();
        v.sort_by(|a, b| b.updated_ts.cmp(&a.updated_ts));
        v
    }

    /// All issues across all repos, sorted by updated_ts desc.
    pub fn all_issues(&self) -> Vec<&IssueDetail> {
        let mut v: Vec<&IssueDetail> = self.repos.values().flat_map(|e| &e.issues).collect();
        v.sort_by(|a, b| b.updated_ts.cmp(&a.updated_ts));
        v
    }
}

use crate::providers::{IssueDetail, PrDetail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoMeta {
    pub stars:       Option<u32>,
    pub open_issues: Option<u32>,
    pub open_prs:    Option<u32>,
    pub visibility:  Option<String>,
    pub ci_status:   Option<String>,
    pub dependabot_open: Option<u32>,
    #[serde(default)]
    pub deploy_status: Option<String>,
    pub fetched_at:  i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheEntry {
    #[serde(default)]
    pub meta:   RepoMeta,
    #[serde(default)]
    pub prs:    Vec<PrDetail>,
    #[serde(default)]
    pub issues: Vec<IssueDetail>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetaCache {
    #[serde(default)]
    pub repos: HashMap<String, CacheEntry>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("meta_cache.toml")
}

pub fn load() -> MetaCache {
    let p = file_path();
    if !p.exists() { return MetaCache::default(); }
    toml::from_str(&std::fs::read_to_string(p).unwrap_or_default()).unwrap_or_default()
}

pub fn save(cache: &MetaCache) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(cache)?)?;
    Ok(())
}

impl MetaCache {
    pub fn get(&self, repo_name: &str) -> Option<&CacheEntry> {
        self.repos.get(repo_name)
    }

    pub fn set_meta(&mut self, repo_name: &str, meta: RepoMeta) {
        self.repos.entry(repo_name.to_string()).or_default().meta = meta;
    }

    pub fn set_prs(&mut self, repo_name: &str, prs: Vec<PrDetail>) {
        self.repos.entry(repo_name.to_string()).or_default().prs = prs;
    }

    pub fn set_issues(&mut self, repo_name: &str, issues: Vec<IssueDetail>) {
        self.repos.entry(repo_name.to_string()).or_default().issues = issues;
    }

    pub fn all_prs(&self) -> Vec<&PrDetail> {
        let mut v: Vec<&PrDetail> = self.repos.values().flat_map(|e| &e.prs).collect();
        v.sort_by_key(|pr| std::cmp::Reverse(pr.updated_ts));
        v
    }

    pub fn all_issues(&self) -> Vec<&IssueDetail> {
        let mut v: Vec<&IssueDetail> = self.repos.values().flat_map(|e| &e.issues).collect();
        v.sort_by_key(|issue| std::cmp::Reverse(issue.updated_ts));
        v
    }
}

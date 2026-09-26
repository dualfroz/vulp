mod gitea;
mod github;
mod gitlab;

pub use gitea::GiteaProvider;
pub use github::GitHubProvider;
pub use gitlab::GitLabProvider;

use crate::auth::Account;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name:    String,
    pub url:     String,
    pub ssh_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct RepoAffiliation {
    pub owner:         bool,
    pub collaborator:  bool,
    pub org_member:    bool,
}

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub url:    String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RepoMeta {
    pub open_issues:  u32,
    pub open_prs:     u32,
    pub stars:        u32,
    pub description:  Option<String>,
    pub topics:       Vec<String>,
    pub viewer_can_merge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrDetail {
    pub repo:        String,
    pub number:      u64,
    pub title:       String,
    pub author:      String,
    pub head_branch: String,
    pub base_branch: String,
    pub created_ts:  i64,
    pub updated_ts:  i64,
    pub url:         String,
    pub draft:       bool,
    pub labels:      Vec<String>,
    #[serde(default)]
    pub viewer_can_merge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssueDetail {
    pub repo:       String,
    pub number:     u64,
    pub title:      String,
    pub author:     String,
    pub labels:     Vec<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub url:        String,
    pub comments:   u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiStatus { Success, Failure, Running }

impl CiStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Running => "running",
        }
    }
}

#[async_trait]
#[allow(dead_code)]
pub trait Provider: Send + Sync {
    async fn create_repo(
        &self, name: &str, private: bool, description: &str, owner: Option<&str>,
    ) -> Result<RepoInfo>;

    async fn list_orgs(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn list_repos(&self, _affiliation: &RepoAffiliation) -> Result<Vec<RepoInfo>> {
        Ok(vec![])
    }

    async fn delete_repo(&self, name: &str) -> Result<()>;

    async fn set_topics(&self, repo: &str, topics: &[String]) -> Result<()>;

    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()>;

    async fn create_pull_request(
        &self, repo: &str, title: &str, body: &str, head: &str, base: &str,
    ) -> Result<PrInfo>;

    async fn create_issue(&self, _repo: &str, _title: &str, _body: &str) -> Result<PrInfo> {
        anyhow::bail!("create_issue not supported by this provider")
    }

    async fn request_reviewers(
        &self, _repo: &str, _pr_number: u64, _reviewers: &[String],
    ) -> Result<()> {
        Ok(())
    }

    async fn add_labels(
        &self, _repo: &str, _issue_number: u64, _labels: &[String],
    ) -> Result<()> {
        Ok(())
    }

    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()>;

    async fn create_release(
        &self, repo: &str, tag: &str, title: &str, body: &str, draft: bool,
    ) -> Result<String>;

    async fn repo_meta(&self, repo: &str) -> Result<RepoMeta>;

    async fn list_pull_requests(&self, _repo: &str) -> Result<Vec<PrDetail>> {
        Ok(vec![])
    }

    async fn list_issues(&self, _repo: &str) -> Result<Vec<IssueDetail>> {
        Ok(vec![])
    }

    async fn count_search(&self, _query: &str) -> Result<u64> {
        anyhow::bail!("count_search not supported by this provider")
    }

    async fn org_dependabot_counts(
        &self, _org: &str,
    ) -> Result<Option<std::collections::HashMap<String, u32>>> {
        Ok(None)
    }

    async fn latest_ci_status(&self, _repo: &str, _branch: &str) -> Result<Option<CiStatus>> {
        Ok(None)
    }

    async fn whoami(&self) -> Result<String>;

    async fn update_visibility(&self, repo: &str, private: bool) -> Result<()> {
        let _ = (repo, private);
        anyhow::bail!("update_visibility not supported by this provider")
    }

    async fn rename_repo(&self, repo: &str, new_name: &str) -> Result<()> {
        let _ = (repo, new_name);
        anyhow::bail!("rename_repo not supported by this provider")
    }

    async fn archive_repo(&self, repo: &str) -> Result<()> {
        let _ = repo;
        anyhow::bail!("archive_repo not supported by this provider")
    }

    async fn update_description(&self, repo: &str, description: &str) -> Result<()> {
        let _ = (repo, description);
        anyhow::bail!("update_description not supported by this provider")
    }
}

pub fn make_provider(account: &Account) -> Result<Box<dyn Provider>> {
    let token = crate::auth::get_token(&account.id())?;
    match account.provider.as_str() {
        "github" => {
            let base = account.base_url.as_deref()
                .unwrap_or("https://api.github.com");
            Ok(Box::new(GitHubProvider::new(base, &account.username, &token)?))
        }
        "gitlab" => {
            let base = account.base_url.as_deref()
                .unwrap_or("https://gitlab.com/api/v4");
            Ok(Box::new(GitLabProvider::new(base, &account.username, &token)?))
        }
        "gitea" | "forgejo" => {
            let base = account.base_url.as_deref()
                .ok_or_else(|| anyhow::anyhow!("gitea/forgejo requires base_url in account config"))?;
            let api_base = if base.ends_with("/api/v1") { base.to_string() } else { format!("{}/api/v1", base.trim_end_matches('/')) };
            Ok(Box::new(GiteaProvider::new(&api_base, &account.username, &token)?))
        }
        other => anyhow::bail!("unsupported provider: {}", other),
    }
}

pub fn default_account<'a>(
    accounts: &'a [crate::auth::Account],
    provider_hint: Option<&str>,
) -> Option<&'a crate::auth::Account> {
    if let Some(p) = provider_hint {
        accounts.iter().find(|a| a.provider == p && a.is_default)
            .or_else(|| accounts.iter().find(|a| a.provider == p))
    } else {
        accounts.iter().find(|a| a.is_default)
            .or_else(|| accounts.first())
    }
}

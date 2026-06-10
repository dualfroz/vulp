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

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub url:    String,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RepoMeta {
    pub open_issues: u32,
    pub open_prs:    u32,
    pub stars:       u32,
}

#[async_trait]
#[allow(dead_code)]
pub trait Provider: Send + Sync {
    /// Create a new remote repository.
    async fn create_repo(
        &self, name: &str, private: bool, description: &str,
    ) -> Result<RepoInfo>;

    /// Delete a remote repository.
    async fn delete_repo(&self, name: &str) -> Result<()>;

    /// Replace the topic list on a repository.
    async fn set_topics(&self, repo: &str, topics: &[String]) -> Result<()>;

    /// Create a branch from an existing ref.
    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()>;

    /// Open a pull-request / merge-request.
    async fn create_pull_request(
        &self, repo: &str, title: &str, body: &str, head: &str, base: &str,
    ) -> Result<PrInfo>;

    /// Request reviewers for an open pull request / merge request.
    async fn request_reviewers(
        &self, _repo: &str, _pr_number: u64, _reviewers: &[String],
    ) -> Result<()> {
        Ok(())
    }

    /// Add labels to an open pull request / issue.
    async fn add_labels(
        &self, _repo: &str, _issue_number: u64, _labels: &[String],
    ) -> Result<()> {
        Ok(())
    }

    /// Add a collaborator to a repository.
    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()>;

    /// Create a release tag with release notes.
    async fn create_release(
        &self, repo: &str, tag: &str, title: &str, body: &str, draft: bool,
    ) -> Result<String>;

    /// Fetch basic metadata (issues, PRs, stars).
    async fn repo_meta(&self, repo: &str) -> Result<RepoMeta>;

    /// Verify the token works, return the authenticated username.
    async fn whoami(&self) -> Result<String>;
}

/// Build a boxed `Provider` from an `Account`.
/// The token must already be present in the keyring.
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

/// Resolve the default account for a given provider name (or the global default).
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

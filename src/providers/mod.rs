//! `Provider` — the trait every hosting backend (GitHub, GitLab, Gitea/
//! Forgejo) implements, plus the data types its methods exchange and the
//! `make_provider`/`default_account` helpers that pick and construct the
//! right implementation for a given `Account`.

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

/// Result of creating a remote repository: its clone URLs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name:    String,
    pub url:     String,
    pub ssh_url: String,
}

/// Which categories of remote repos to include in a bulk import. `owner` is
/// always true in practice (the user's own repos) — kept as a field rather
/// than implied so all three read the same way at the call site.
#[derive(Debug, Clone, Default)]
pub struct RepoAffiliation {
    pub owner:         bool,
    pub collaborator:  bool,
    pub org_member:    bool,
}

/// Result of creating a pull/merge request or issue: just enough to link
/// back to it (full detail, when needed, comes from `list_pull_requests`).
#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub url:    String,
}

/// Basic repo metadata fetched for the Settings tab and the hub's cached
/// stars/issues/PRs counts.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RepoMeta {
    pub open_issues:  u32,
    pub open_prs:     u32,
    pub stars:        u32,
    pub description:  Option<String>,
    pub topics:       Vec<String>,
}

/// One pull/merge request, as shown in the hub's PRs tab and the repo
/// detail view's PRs tab.
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
}

/// One issue, as shown in the hub's Issues tab and the repo detail view's
/// Issues tab.
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

/// A hosting backend's API surface. Methods that not every provider
/// implements (`list_orgs`, `list_repos`, optional repo settings) have a
/// default that either returns an empty result or bails with a clear
/// "not supported" error, so callers can call any method on any provider
/// without matching on which one it is.
#[async_trait]
#[allow(dead_code)]
pub trait Provider: Send + Sync {
    /// Create a new remote repository. `owner` is `Some(org)` to create it
    /// under an organization/group instead of the authenticated user's
    /// personal account.
    async fn create_repo(
        &self, name: &str, private: bool, description: &str, owner: Option<&str>,
    ) -> Result<RepoInfo>;

    /// Organizations/groups the authenticated user can create repos under.
    /// Empty by default — providers without a sensible equivalent just don't
    /// offer the picker.
    async fn list_orgs(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    /// List repos visible to the authenticated user matching the given
    /// affiliation flags. Empty by default — providers without a sensible
    /// equivalent just don't offer bulk import.
    async fn list_repos(&self, _affiliation: &RepoAffiliation) -> Result<Vec<RepoInfo>> {
        Ok(vec![])
    }

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

    /// Open a new issue. Not every provider's API is wired up yet — default
    /// bails the same way other not-yet-implemented methods on this trait do.
    async fn create_issue(&self, _repo: &str, _title: &str, _body: &str) -> Result<PrInfo> {
        anyhow::bail!("create_issue not supported by this provider")
    }

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

    /// List open pull requests for a repository.
    async fn list_pull_requests(&self, _repo: &str) -> Result<Vec<PrDetail>> {
        Ok(vec![])
    }

    /// List open issues for a repository (excludes PRs).
    async fn list_issues(&self, _repo: &str) -> Result<Vec<IssueDetail>> {
        Ok(vec![])
    }

    /// Total hit count for a provider-side issue/PR search query (e.g.
    /// "is:pr is:merged user:X") — used by the Overview tab's merged/closed
    /// counters. Providers without a search API report nothing.
    async fn count_search(&self, _query: &str) -> Result<u64> {
        anyhow::bail!("count_search not supported by this provider")
    }

    /// Verify the token works, return the authenticated username.
    async fn whoami(&self) -> Result<String>;

    /// Change a repository's visibility.
    async fn update_visibility(&self, repo: &str, private: bool) -> Result<()> {
        let _ = (repo, private);
        anyhow::bail!("update_visibility not supported by this provider")
    }

    /// Rename a repository on the remote.
    async fn rename_repo(&self, repo: &str, new_name: &str) -> Result<()> {
        let _ = (repo, new_name);
        anyhow::bail!("rename_repo not supported by this provider")
    }

    /// Archive a repository (make it read-only).
    async fn archive_repo(&self, repo: &str) -> Result<()> {
        let _ = repo;
        anyhow::bail!("archive_repo not supported by this provider")
    }

    /// Update the description of a repository.
    async fn update_description(&self, repo: &str, description: &str) -> Result<()> {
        let _ = (repo, description);
        anyhow::bail!("update_description not supported by this provider")
    }
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

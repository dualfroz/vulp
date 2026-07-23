//! `Provider` implementation for GitLab (gitlab.com or self-hosted, via a
//! custom `base_url`), talking to the REST API v4 directly over `reqwest`.

use super::{PrInfo, Provider, RepoInfo, RepoMeta};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client, Response};
use serde_json::{json, Value};

pub struct GitLabProvider {
    client:    Client,
    namespace: String,
    base_url:  String,
}

impl GitLabProvider {
    /// Builds a client authenticated with `token`, scoped to `namespace`'s
    /// projects by default. `base_url` lets this point at a self-hosted
    /// GitLab instance instead of `gitlab.com`.
    pub fn new(base_url: &str, namespace: &str, token: &str) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            header::HeaderValue::from_str(token)?,
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("vulp/0.1"),
        );
        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self {
            client,
            namespace: namespace.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Joins `path` onto `base_url`, normalizing the slash between them.
    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// Builds GitLab's URL-encoded project identifier (`namespace%2Frepo`),
    /// since the v4 API addresses projects by encoded path rather than a
    /// separate numeric id field this codebase tracks. `repo` may already be
    /// a full `group/subgroup/repo` path (mirrors `GitHubProvider::slug`) —
    /// in that case it's used verbatim instead of prefixing `self.namespace`
    /// again, which would otherwise mangle any group- or subgroup-owned
    /// project into a nonexistent `namespace/group/repo` path.
    fn repo_id(&self, repo: &str) -> String {
        let raw = if repo.contains('/') { repo.to_string() } else { format!("{}/{}", self.namespace, repo) };
        raw.replace('/', "%2F")
    }

    /// Parses a response as JSON if the status was successful, otherwise
    /// turns the non-2xx status and body into an error.
    async fn check(resp: Response) -> Result<Value> {
        let status = resp.status();
        let text = resp.text().await?;
        if status.is_success() {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        } else {
            anyhow::bail!("GitLab API {} — {}", status, text)
        }
    }
}

#[async_trait]
impl Provider for GitLabProvider {
    async fn create_repo(&self, name: &str, private: bool, description: &str, _owner: Option<&str>) -> Result<RepoInfo> {
        let visibility = if private { "private" } else { "public" };
        let body = json!({
            "name": name,
            "visibility": visibility,
            "description": description,
            "namespace_path": self.namespace,
        });
        let resp = self.client.post(self.url("projects")).json(&body).send().await?;
        let v = Self::check(resp).await.context("create_repo")?;
        Ok(RepoInfo {
            name:    v["name"].as_str().unwrap_or(name).to_string(),
            url:     v["web_url"].as_str().unwrap_or("").to_string(),
            ssh_url: v["ssh_url_to_repo"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn delete_repo(&self, name: &str) -> Result<()> {
        let resp = self.client
            .delete(self.url(&format!("projects/{}", self.repo_id(name))))
            .send().await?;
        Self::check(resp).await.context("delete_repo")?;
        Ok(())
    }

    async fn set_topics(&self, repo: &str, topics: &[String]) -> Result<()> {
        let body = json!({ "topics": topics });
        let resp = self.client
            .put(self.url(&format!("projects/{}", self.repo_id(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("set_topics")?;
        Ok(())
    }

    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()> {
        let body = json!({ "branch": branch, "ref": from });
        let resp = self.client
            .post(self.url(&format!("projects/{}/repository/branches", self.repo_id(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("create_branch")?;
        Ok(())
    }

    async fn create_pull_request(
        &self, repo: &str, title: &str, body: &str, head: &str, base: &str,
    ) -> Result<PrInfo> {
        let payload = json!({
            "title": title, "description": body,
            "source_branch": head, "target_branch": base,
        });
        let resp = self.client
            .post(self.url(&format!("projects/{}/merge_requests", self.repo_id(repo))))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_pull_request")?;
        Ok(PrInfo {
            number: v["iid"].as_u64().unwrap_or(0),
            url:    v["web_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn request_reviewers(
        &self, repo: &str, pr_number: u64, reviewers: &[String],
    ) -> Result<()> {
        // Resolve usernames to GitLab user IDs
        let mut reviewer_ids: Vec<u64> = Vec::new();
        for username in reviewers {
            let resp = self.client
                .get(self.url(&format!("users?username={}", username)))
                .send().await?;
            if let Ok(v) = Self::check(resp).await {
                if let Some(id) = v.as_array()
                    .and_then(|a| a.first())
                    .and_then(|u| u["id"].as_u64())
                {
                    reviewer_ids.push(id);
                }
            }
        }
        if !reviewer_ids.is_empty() {
            let body = json!({ "reviewer_ids": reviewer_ids });
            let resp = self.client
                .put(self.url(&format!("projects/{}/merge_requests/{}", self.repo_id(repo), pr_number)))
                .json(&body).send().await?;
            Self::check(resp).await.context("request_reviewers")?;
        }
        Ok(())
    }

    async fn add_labels(
        &self, repo: &str, issue_number: u64, labels: &[String],
    ) -> Result<()> {
        let body = json!({ "add_labels": labels.join(",") });
        let resp = self.client
            .put(self.url(&format!("projects/{}/merge_requests/{}", self.repo_id(repo), issue_number)))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_labels")?;
        Ok(())
    }

    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()> {
        // GitLab access levels: 30=developer, 40=maintainer, 50=owner
        let level: u8 = match role { "read" => 20, "write" => 30, _ => 40 };
        let body = json!({ "user_id": user, "access_level": level });
        let resp = self.client
            .post(self.url(&format!("projects/{}/members", self.repo_id(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_collaborator")?;
        Ok(())
    }

    async fn create_release(
        &self, repo: &str, tag: &str, title: &str, body: &str, _draft: bool,
    ) -> Result<String> {
        let payload = json!({ "tag_name": tag, "name": title, "description": body });
        let resp = self.client
            .post(self.url(&format!("projects/{}/releases", self.repo_id(repo))))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_release")?;
        Ok(v["_links"]["self"].as_str().unwrap_or("").to_string())
    }

    async fn repo_meta(&self, repo: &str) -> Result<RepoMeta> {
        let resp = self.client
            .get(self.url(&format!("projects/{}", self.repo_id(repo))))
            .send().await?;
        let v = Self::check(resp).await.context("repo_meta")?;
        Ok(RepoMeta {
            open_issues:  v["open_issues_count"].as_u64().unwrap_or(0) as u32,
            open_prs:     v["merge_requests_count"].as_u64().unwrap_or(0) as u32,
            stars:        v["star_count"].as_u64().unwrap_or(0) as u32,
            description:  v["description"].as_str().filter(|s| !s.is_empty()).map(String::from),
            topics:       v["topics"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default(),
            viewer_can_merge: v["permissions"]["project_access"]["access_level"].as_u64().unwrap_or(0) >= 30,
        })
    }

    async fn whoami(&self) -> Result<String> {
        let resp = self.client.get(self.url("user")).send().await?;
        let v = Self::check(resp).await.context("whoami")?;
        Ok(v["username"].as_str().unwrap_or("unknown").to_string())
    }

    async fn update_visibility(&self, repo: &str, private: bool) -> Result<()> {
        let visibility = if private { "private" } else { "public" };
        let body = json!({ "visibility": visibility });
        let resp = self.client
            .put(self.url(&format!("projects/{}", self.repo_id(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("update_visibility")?;
        Ok(())
    }

    async fn rename_repo(&self, repo: &str, new_name: &str) -> Result<()> {
        let body = json!({ "name": new_name, "path": new_name });
        let resp = self.client
            .put(self.url(&format!("projects/{}", self.repo_id(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("rename_repo")?;
        Ok(())
    }

    async fn archive_repo(&self, repo: &str) -> Result<()> {
        let resp = self.client
            .post(self.url(&format!("projects/{}/archive", self.repo_id(repo))))
            .send().await?;
        Self::check(resp).await.context("archive_repo")?;
        Ok(())
    }
}

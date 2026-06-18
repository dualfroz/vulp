use super::{PrInfo, Provider, RepoInfo, RepoMeta};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client, Response};
use serde_json::{json, Value};

/// Gitea / Forgejo REST API v1 provider.
pub struct GiteaProvider {
    client:   Client,
    owner:    String,
    base_url: String,   // e.g. "https://gitea.example.com/api/v1"
}

impl GiteaProvider {
    pub fn new(base_url: &str, owner: &str, token: &str) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("token {}", token))?,
        );
        headers.insert(header::USER_AGENT, header::HeaderValue::from_static("fxg/0.1"));
        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self {
            client,
            owner: owner.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    async fn check(resp: Response) -> Result<Value> {
        let status = resp.status();
        let text   = resp.text().await?;
        if status.is_success() {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        } else {
            anyhow::bail!("Gitea API {} — {}", status, text)
        }
    }
}

#[async_trait]
impl Provider for GiteaProvider {
    async fn create_repo(&self, name: &str, private: bool, description: &str) -> Result<RepoInfo> {
        let body = json!({ "name": name, "private": private, "description": description, "auto_init": false });
        let resp = self.client.post(self.url("user/repos")).json(&body).send().await?;
        let v    = Self::check(resp).await.context("create_repo")?;
        Ok(RepoInfo {
            name:    v["name"].as_str().unwrap_or(name).to_string(),
            url:     v["html_url"].as_str().unwrap_or("").to_string(),
            ssh_url: v["ssh_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn delete_repo(&self, name: &str) -> Result<()> {
        let resp = self.client
            .delete(self.url(&format!("repos/{}/{}", self.owner, name)))
            .send().await?;
        Self::check(resp).await.context("delete_repo")?;
        Ok(())
    }

    async fn set_topics(&self, repo: &str, topics: &[String]) -> Result<()> {
        let body = json!({ "topics": topics });
        let resp = self.client
            .put(self.url(&format!("repos/{}/{}/topics", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("set_topics")?;
        Ok(())
    }

    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()> {
        let body = json!({ "new_branch_name": branch, "old_branch_name": from });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/branches", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("create_branch")?;
        Ok(())
    }

    async fn create_pull_request(
        &self, repo: &str, title: &str, body: &str, head: &str, base: &str,
    ) -> Result<PrInfo> {
        let payload = json!({ "title": title, "body": body, "head": head, "base": base });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/pulls", self.owner, repo)))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_pull_request")?;
        Ok(PrInfo {
            number: v["number"].as_u64().unwrap_or(0),
            url:    v["html_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn request_reviewers(
        &self, repo: &str, pr_number: u64, reviewers: &[String],
    ) -> Result<()> {
        let body = json!({ "reviewers": reviewers });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/pulls/{}/requested_reviewers", self.owner, repo, pr_number)))
            .json(&body).send().await?;
        Self::check(resp).await.context("request_reviewers")?;
        Ok(())
    }

    async fn add_labels(
        &self, repo: &str, issue_number: u64, labels: &[String],
    ) -> Result<()> {
        // Fetch existing labels to find IDs; create any that are missing
        let list_resp = self.client
            .get(self.url(&format!("repos/{}/{}/labels", self.owner, repo)))
            .send().await?;
        let existing: Vec<(u64, String)> = if list_resp.status().is_success() {
            list_resp.json::<serde_json::Value>().await
                .ok()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|l| l["id"].as_u64().zip(l["name"].as_str().map(|s| s.to_string())))
                .collect()
        } else {
            vec![]
        };

        let mut ids: Vec<u64> = Vec::new();
        for label in labels {
            if let Some((id, _)) = existing.iter().find(|(_, n)| n == label) {
                ids.push(*id);
            } else {
                let body = json!({ "name": label, "color": "#0075ca" });
                let resp = self.client
                    .post(self.url(&format!("repos/{}/{}/labels", self.owner, repo)))
                    .json(&body).send().await?;
                if let Ok(v) = Self::check(resp).await {
                    if let Some(id) = v["id"].as_u64() { ids.push(id); }
                }
            }
        }

        if !ids.is_empty() {
            let body = json!({ "labels": ids });
            let resp = self.client
                .post(self.url(&format!("repos/{}/{}/issues/{}/labels", self.owner, repo, issue_number)))
                .json(&body).send().await?;
            Self::check(resp).await.context("add_labels")?;
        }
        Ok(())
    }

    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()> {
        let permission = match role { "read" => "read", "admin" => "admin", _ => "write" };
        let body = json!({ "permission": permission });
        let resp = self.client
            .put(self.url(&format!("repos/{}/{}/collaborators/{}", self.owner, repo, user)))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_collaborator")?;
        Ok(())
    }

    async fn create_release(
        &self, repo: &str, tag: &str, title: &str, body: &str, draft: bool,
    ) -> Result<String> {
        let payload = json!({ "tag_name": tag, "name": title, "body": body, "draft": draft });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/releases", self.owner, repo)))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_release")?;
        Ok(v["html_url"].as_str().unwrap_or("").to_string())
    }

    async fn repo_meta(&self, repo: &str) -> Result<RepoMeta> {
        let resp = self.client
            .get(self.url(&format!("repos/{}/{}", self.owner, repo)))
            .send().await?;
        let v = Self::check(resp).await.context("repo_meta")?;
        Ok(RepoMeta {
            open_issues:  v["open_issues_count"].as_u64().unwrap_or(0) as u32,
            open_prs:     v["open_pr_counter"].as_u64().unwrap_or(0) as u32,
            stars:        v["stars_count"].as_u64().unwrap_or(0) as u32,
            description:  v["description"].as_str().filter(|s| !s.is_empty()).map(String::from),
            topics:       vec![],
        })
    }

    async fn whoami(&self) -> Result<String> {
        let resp = self.client.get(self.url("user")).send().await?;
        let v    = Self::check(resp).await.context("whoami")?;
        Ok(v["login"].as_str().unwrap_or("unknown").to_string())
    }

    async fn update_visibility(&self, repo: &str, private: bool) -> Result<()> {
        let body = json!({ "private": private });
        let resp = self.client
            .patch(self.url(&format!("repos/{}/{}", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("update_visibility")?;
        Ok(())
    }

    async fn rename_repo(&self, repo: &str, new_name: &str) -> Result<()> {
        let body = json!({ "name": new_name });
        let resp = self.client
            .patch(self.url(&format!("repos/{}/{}", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("rename_repo")?;
        Ok(())
    }

    async fn archive_repo(&self, repo: &str) -> Result<()> {
        let body = json!({ "archived": true });
        let resp = self.client
            .patch(self.url(&format!("repos/{}/{}", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("archive_repo")?;
        Ok(())
    }
}

use super::{PrInfo, Provider, RepoInfo, RepoMeta};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client, Response};
use serde_json::{json, Value};

pub struct GitHubProvider {
    client:   Client,
    owner:    String,
    base_url: String,
}

impl GitHubProvider {
    pub fn new(base_url: &str, owner: &str, token: &str) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {}", token))?,
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("fxg/0.1"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            header::HeaderValue::from_static("2022-11-28"),
        );
        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self { client, owner: owner.to_string(), base_url: base_url.trim_end_matches('/').to_string() })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    async fn check(resp: Response) -> Result<Value> {
        let status = resp.status();
        let text = resp.text().await?;
        if status.is_success() {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        } else {
            anyhow::bail!("GitHub API {} — {}", status, text)
        }
    }
}

#[async_trait]
impl Provider for GitHubProvider {
    async fn create_repo(&self, name: &str, private: bool, description: &str) -> Result<RepoInfo> {
        let body = json!({ "name": name, "private": private, "description": description });
        let resp = self.client.post(self.url("user/repos")).json(&body).send().await?;
        let v = Self::check(resp).await.context("create_repo")?;
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
        let body = json!({ "names": topics });
        let resp = self.client
            .put(self.url(&format!("repos/{}/{}/topics", self.owner, repo)))
            .json(&body).send().await?;
        Self::check(resp).await.context("set_topics")?;
        Ok(())
    }

    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()> {
        // Resolve 'from' to a SHA first
        let ref_url = self.url(&format!("repos/{}/{}/git/ref/heads/{}", self.owner, repo, from));
        let resp = self.client.get(&ref_url).send().await?;
        let v = Self::check(resp).await.context("resolve ref")?;
        let sha = v["object"]["sha"].as_str()
            .ok_or_else(|| anyhow::anyhow!("could not resolve SHA for {}", from))?
            .to_string();

        let body = json!({ "ref": format!("refs/heads/{}", branch), "sha": sha });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/git/refs", self.owner, repo)))
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

    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()> {
        let body = json!({ "permission": role });
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

        let pr_resp = self.client
            .get(self.url(&format!("repos/{}/{}/pulls?state=open&per_page=100", self.owner, repo)))
            .send().await?;
        let open_prs = if pr_resp.status().is_success() {
            let pr_list: serde_json::Value = pr_resp.json().await.unwrap_or(serde_json::Value::Array(vec![]));
            pr_list.as_array().map(|a| a.len() as u32).unwrap_or(0)
        } else {
            0
        };

        Ok(RepoMeta {
            open_issues: v["open_issues_count"].as_u64().unwrap_or(0) as u32,
            open_prs,
            stars:       v["stargazers_count"].as_u64().unwrap_or(0) as u32,
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
        let body = json!({ "labels": labels });
        let resp = self.client
            .post(self.url(&format!("repos/{}/{}/issues/{}/labels", self.owner, repo, issue_number)))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_labels")?;
        Ok(())
    }

    async fn whoami(&self) -> Result<String> {
        let resp = self.client.get(self.url("user")).send().await?;
        let v = Self::check(resp).await.context("whoami")?;
        Ok(v["login"].as_str().unwrap_or("unknown").to_string())
    }
}

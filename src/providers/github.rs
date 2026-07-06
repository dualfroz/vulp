//! `Provider` implementation for GitHub (and GitHub Enterprise, via a
//! custom `base_url`). Talks to the REST API directly over `reqwest` rather
//! than the `gh` CLI, so it works the same whether or not `gh` is installed.

use super::{CiStatus, IssueDetail, PrDetail, PrInfo, Provider, RepoAffiliation, RepoInfo, RepoMeta};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::{header, Client, Response};
use serde_json::{json, Value};

pub struct GitHubProvider {
    client:   Client,
    owner:    String,
    base_url: String,
}

/// Extracts the `rel="next"` URL from an RFC-5988 `Link` header, if present.
fn link_next_url(link: &str) -> Option<String> {
    link.split(',')
        .find(|part| part.contains("rel=\"next\""))
        .and_then(|part| {
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            Some(part[start..end].to_string())
        })
}

impl GitHubProvider {
    /// Builds a client authenticated with `token`, scoped to `owner`'s repos
    /// by default. `base_url` lets this point at a GitHub Enterprise
    /// instance instead of `api.github.com`.
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
            header::HeaderValue::from_static("vulp/0.1"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            header::HeaderValue::from_static("2022-11-28"),
        );
        // Without a timeout, a stalled connection (flaky network, secondary
        // rate-limiting) hangs the background worker forever with no way for
        // the UI to surface an error — better to fail loud than hang silent.
        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { client, owner: owner.to_string(), base_url: base_url.trim_end_matches('/').to_string() })
    }

    /// Joins `path` onto `base_url`, normalizing the slash between them.
    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// Repo path segment: names may arrive as bare ("vulp" → account owner)
    /// or fully qualified ("VulCodeCom/vulp" → used verbatim), so org-owned
    /// repos work with a personal account's token.
    fn slug(&self, repo: &str) -> String {
        if repo.contains('/') { repo.to_string() } else { format!("{}/{}", self.owner, repo) }
    }

    /// Parses a response as JSON if the status was successful, otherwise
    /// turns the non-2xx status and body into an error — the single choke
    /// point every API call routes through so error messages stay consistent.
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
    async fn create_repo(&self, name: &str, private: bool, description: &str, owner: Option<&str>) -> Result<RepoInfo> {
        let body = json!({ "name": name, "private": private, "description": description });
        let path = match owner {
            Some(org) => format!("orgs/{}/repos", org),
            None       => "user/repos".to_string(),
        };
        let resp = self.client.post(self.url(&path)).json(&body).send().await?;
        let v = Self::check(resp).await.context("create_repo")?;
        Ok(RepoInfo {
            name:    v["name"].as_str().unwrap_or(name).to_string(),
            url:     v["html_url"].as_str().unwrap_or("").to_string(),
            ssh_url: v["ssh_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn list_orgs(&self) -> Result<Vec<String>> {
        let resp = self.client.get(self.url("user/orgs")).send().await?;
        let v = Self::check(resp).await.context("list_orgs")?;
        Ok(v.as_array().map(|arr| {
            arr.iter().filter_map(|o| o["login"].as_str().map(str::to_string)).collect()
        }).unwrap_or_default())
    }

    async fn list_repos(&self, affiliation: &RepoAffiliation) -> Result<Vec<RepoInfo>> {
        let mut affs = Vec::new();
        if affiliation.owner        { affs.push("owner"); }
        if affiliation.collaborator { affs.push("collaborator"); }
        if affiliation.org_member   { affs.push("organization_member"); }
        if affs.is_empty() { return Ok(vec![]); }

        let mut repos = Vec::new();
        let mut url = self.url(&format!("user/repos?affiliation={}&per_page=100", affs.join(",")));
        loop {
            let resp = self.client.get(&url).send().await?;
            let next = resp.headers().get(header::LINK)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_next_link);
            let v = Self::check(resp).await.context("list_repos")?;
            if let Some(arr) = v.as_array() {
                for o in arr {
                    repos.push(RepoInfo {
                        name:    o["name"].as_str().unwrap_or_default().to_string(),
                        url:     o["html_url"].as_str().unwrap_or_default().to_string(),
                        ssh_url: o["ssh_url"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            match next {
                Some(next_url) => url = next_url,
                None => break,
            }
        }
        Ok(repos)
    }

    async fn delete_repo(&self, name: &str) -> Result<()> {
        let resp = self.client
            .delete(self.url(&format!("repos/{}", self.slug(name))))
            .send().await?;
        Self::check(resp).await.context("delete_repo")?;
        Ok(())
    }

    async fn set_topics(&self, repo: &str, topics: &[String]) -> Result<()> {
        let body = json!({ "names": topics });
        let resp = self.client
            .put(self.url(&format!("repos/{}/topics", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("set_topics")?;
        Ok(())
    }

    async fn create_branch(&self, repo: &str, branch: &str, from: &str) -> Result<()> {
        // Resolve 'from' to a SHA first
        let ref_url = self.url(&format!("repos/{}/git/ref/heads/{}", self.slug(repo), from));
        let resp = self.client.get(&ref_url).send().await?;
        let v = Self::check(resp).await.context("resolve ref")?;
        let sha = v["object"]["sha"].as_str()
            .ok_or_else(|| anyhow::anyhow!("could not resolve SHA for {}", from))?
            .to_string();

        let body = json!({ "ref": format!("refs/heads/{}", branch), "sha": sha });
        let resp = self.client
            .post(self.url(&format!("repos/{}/git/refs", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("create_branch")?;
        Ok(())
    }

    async fn create_pull_request(
        &self, repo: &str, title: &str, body: &str, head: &str, base: &str,
    ) -> Result<PrInfo> {
        let payload = json!({ "title": title, "body": body, "head": head, "base": base });
        let resp = self.client
            .post(self.url(&format!("repos/{}/pulls", self.slug(repo))))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_pull_request")?;
        Ok(PrInfo {
            number: v["number"].as_u64().unwrap_or(0),
            url:    v["html_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn create_issue(&self, repo: &str, title: &str, body: &str) -> Result<PrInfo> {
        let payload = json!({ "title": title, "body": body });
        let resp = self.client
            .post(self.url(&format!("repos/{}/issues", self.slug(repo))))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_issue")?;
        Ok(PrInfo {
            number: v["number"].as_u64().unwrap_or(0),
            url:    v["html_url"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn add_collaborator(&self, repo: &str, user: &str, role: &str) -> Result<()> {
        let body = json!({ "permission": role });
        let resp = self.client
            .put(self.url(&format!("repos/{}/collaborators/{}", self.slug(repo), user)))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_collaborator")?;
        Ok(())
    }

    async fn create_release(
        &self, repo: &str, tag: &str, title: &str, body: &str, draft: bool,
    ) -> Result<String> {
        let payload = json!({ "tag_name": tag, "name": title, "body": body, "draft": draft });
        let resp = self.client
            .post(self.url(&format!("repos/{}/releases", self.slug(repo))))
            .json(&payload).send().await?;
        let v = Self::check(resp).await.context("create_release")?;
        Ok(v["html_url"].as_str().unwrap_or("").to_string())
    }

    async fn repo_meta(&self, repo: &str) -> Result<RepoMeta> {
        let resp = self.client
            .get(self.url(&format!("repos/{}", self.slug(repo))))
            .send().await?;
        let v = Self::check(resp).await.context("repo_meta")?;

        let pr_resp = self.client
            .get(self.url(&format!("repos/{}/pulls?state=open&per_page=100", self.slug(repo))))
            .send().await?;
        let open_prs = if pr_resp.status().is_success() {
            let pr_list: serde_json::Value = pr_resp.json().await.unwrap_or(serde_json::Value::Array(vec![]));
            pr_list.as_array().map(|a| a.len() as u32).unwrap_or(0)
        } else {
            0
        };

        let description = v["description"].as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let topics = v["topics"].as_array()
            .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();

        Ok(RepoMeta {
            open_issues:  v["open_issues_count"].as_u64().unwrap_or(0) as u32,
            open_prs,
            stars:        v["stargazers_count"].as_u64().unwrap_or(0) as u32,
            description,
            topics,
        })
    }

    async fn request_reviewers(
        &self, repo: &str, pr_number: u64, reviewers: &[String],
    ) -> Result<()> {
        let body = json!({ "reviewers": reviewers });
        let resp = self.client
            .post(self.url(&format!("repos/{}/pulls/{}/requested_reviewers", self.slug(repo), pr_number)))
            .json(&body).send().await?;
        Self::check(resp).await.context("request_reviewers")?;
        Ok(())
    }

    async fn add_labels(
        &self, repo: &str, issue_number: u64, labels: &[String],
    ) -> Result<()> {
        let body = json!({ "labels": labels });
        let resp = self.client
            .post(self.url(&format!("repos/{}/issues/{}/labels", self.slug(repo), issue_number)))
            .json(&body).send().await?;
        Self::check(resp).await.context("add_labels")?;
        Ok(())
    }

    async fn org_dependabot_counts(
        &self, org: &str,
    ) -> Result<Option<std::collections::HashMap<String, u32>>> {
        let mut counts = std::collections::HashMap::new();
        // One request covers up to 100 open alerts org-wide. The endpoint is
        // cursor-paginated (`page=` returns 400), so past that we follow the
        // Link rel="next" header — capped so a pathological org can't stall
        // the refresh.
        let mut next = Some(format!(
            "{}?state=open&per_page=100",
            self.url(&format!("orgs/{}/dependabot/alerts", org)),
        ));
        for _ in 0..10 {
            let Some(url) = next.take() else { break };
            let resp = self.client.get(&url).send().await?;
            if !resp.status().is_success() {
                // 403 (scope/feature) / 404 (not an org) — unavailable, not an error.
                return Ok(None);
            }
            next = resp.headers().get(header::LINK)
                .and_then(|l| l.to_str().ok())
                .and_then(link_next_url);
            let v: Value = resp.json().await.unwrap_or(Value::Null);
            let Some(alerts) = v.as_array() else { return Ok(None) };
            for alert in alerts {
                if let Some(full) = alert["repository"]["full_name"].as_str() {
                    *counts.entry(full.to_lowercase()).or_insert(0) += 1;
                }
            }
        }
        Ok(Some(counts))
    }

    async fn latest_ci_status(&self, repo: &str, branch: &str) -> Result<Option<CiStatus>> {
        let resp = self.client
            .get(self.url(&format!("repos/{}/actions/runs", self.slug(repo))))
            .query(&[("branch", branch), ("per_page", "1")])
            .send().await?;
        if !resp.status().is_success() { return Ok(None); }
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        let Some(run) = v["workflow_runs"].as_array().and_then(|a| a.first()) else {
            return Ok(None);
        };
        let status = run["status"].as_str().unwrap_or("");
        let conclusion = run["conclusion"].as_str();
        Ok(Some(match (status, conclusion) {
            ("completed", Some("success")) => CiStatus::Success,
            ("completed", _)               => CiStatus::Failure,
            _                              => CiStatus::Running,
        }))
    }

    async fn whoami(&self) -> Result<String> {
        let resp = self.client.get(self.url("user")).send().await?;
        let v = Self::check(resp).await.context("whoami")?;
        Ok(v["login"].as_str().unwrap_or("unknown").to_string())
    }

    async fn count_search(&self, query: &str) -> Result<u64> {
        let resp = self.client
            .get(self.url("search/issues"))
            .query(&[("q", query), ("per_page", "1")])
            .send().await?;
        let v = Self::check(resp).await.context("count_search")?;
        Ok(v["total_count"].as_u64().unwrap_or(0))
    }

    async fn update_visibility(&self, repo: &str, private: bool) -> Result<()> {
        let body = serde_json::json!({ "private": private });
        let resp = self.client
            .patch(self.url(&format!("repos/{}", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("update_visibility")?;
        Ok(())
    }

    async fn rename_repo(&self, repo: &str, new_name: &str) -> Result<()> {
        let body = serde_json::json!({ "name": new_name });
        let resp = self.client
            .patch(self.url(&format!("repos/{}", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("rename_repo")?;
        Ok(())
    }

    async fn archive_repo(&self, repo: &str) -> Result<()> {
        let body = serde_json::json!({ "archived": true });
        let resp = self.client
            .patch(self.url(&format!("repos/{}", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("archive_repo")?;
        Ok(())
    }

    async fn update_description(&self, repo: &str, description: &str) -> Result<()> {
        let body = serde_json::json!({ "description": description });
        let resp = self.client
            .patch(self.url(&format!("repos/{}", self.slug(repo))))
            .json(&body).send().await?;
        Self::check(resp).await.context("update_description")?;
        Ok(())
    }

    async fn list_pull_requests(&self, repo: &str) -> Result<Vec<PrDetail>> {
        let resp = self.client
            .get(self.url(&format!("repos/{}/pulls?state=open&per_page=100", self.slug(repo))))
            .send().await?;
        if !resp.status().is_success() { return Ok(vec![]); }
        let list: Value = resp.json().await.unwrap_or(Value::Array(vec![]));
        let prs = list.as_array().map(|arr| arr.iter().map(|pr| {
            let labels: Vec<String> = pr["labels"].as_array()
                .map(|l| l.iter().filter_map(|x| x["name"].as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            PrDetail {
                repo:        repo.to_string(),
                number:      pr["number"].as_u64().unwrap_or(0),
                title:       pr["title"].as_str().unwrap_or("").to_string(),
                author:      pr["user"]["login"].as_str().unwrap_or("").to_string(),
                head_branch: pr["head"]["ref"].as_str().unwrap_or("").to_string(),
                base_branch: pr["base"]["ref"].as_str().unwrap_or("").to_string(),
                created_ts:  parse_gh_ts(pr["created_at"].as_str().unwrap_or("")),
                updated_ts:  parse_gh_ts(pr["updated_at"].as_str().unwrap_or("")),
                url:         pr["html_url"].as_str().unwrap_or("").to_string(),
                draft:       pr["draft"].as_bool().unwrap_or(false),
                labels,
            }
        }).collect()).unwrap_or_default();
        Ok(prs)
    }

    async fn list_issues(&self, repo: &str) -> Result<Vec<IssueDetail>> {
        let resp = self.client
            .get(self.url(&format!(
                "repos/{}/{}/issues?state=open&per_page=100&pulls=false",
                self.owner, repo
            )))
            .send().await?;
        if !resp.status().is_success() { return Ok(vec![]); }
        let list: Value = resp.json().await.unwrap_or(Value::Array(vec![]));
        let issues = list.as_array().map(|arr| arr.iter()
            // GitHub /issues returns PRs too — filter them out
            .filter(|i| i["pull_request"].is_null())
            .map(|i| {
                let labels: Vec<String> = i["labels"].as_array()
                    .map(|l| l.iter().filter_map(|x| x["name"].as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                IssueDetail {
                    repo:       repo.to_string(),
                    number:     i["number"].as_u64().unwrap_or(0),
                    title:      i["title"].as_str().unwrap_or("").to_string(),
                    author:     i["user"]["login"].as_str().unwrap_or("").to_string(),
                    labels,
                    created_ts: parse_gh_ts(i["created_at"].as_str().unwrap_or("")),
                    updated_ts: parse_gh_ts(i["updated_at"].as_str().unwrap_or("")),
                    url:        i["html_url"].as_str().unwrap_or("").to_string(),
                    comments:   i["comments"].as_u64().unwrap_or(0) as u32,
                }
            }).collect()
        ).unwrap_or_default();
        Ok(issues)
    }
}

/// Parses a GitHub API RFC3339 timestamp into a Unix timestamp, defaulting
/// to 0 on a parse failure rather than propagating an error for what's
/// always a secondary/display-only field.
fn parse_gh_ts(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

/// Pulls the `rel="next"` URL out of a GitHub pagination `Link` header, e.g.
/// `<https://api.github.com/user/repos?page=2>; rel="next", <...>; rel="last"`.
fn parse_next_link(link: &str) -> Option<String> {
    link.split(',').find_map(|part| {
        let part = part.trim();
        if !part.contains("rel=\"next\"") { return None; }
        let start = part.find('<')? + 1;
        let end = part.find('>')?;
        Some(part[start..end].to_string())
    })
}

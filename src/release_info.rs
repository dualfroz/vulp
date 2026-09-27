use anyhow::{Context, Result};
use serde_json::Value;

fn vulp_repo() -> String {
    std::env::var("VULP_REPO").unwrap_or_else(|_| "dualfroz/vulp".to_string())
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag:          String,
    pub author:       String,
    pub published_at: String,
    #[allow(dead_code)] pub body: String,
    pub html_url:     String,
}

async fn fetch_release(url: &str) -> Result<ReleaseInfo> {
    let client = reqwest::Client::builder()
        .user_agent("vulp/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(url).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("GitHub API {} — {}", status, text);
    }
    let v: Value = serde_json::from_str(&text).context("parsing release JSON")?;
    Ok(ReleaseInfo {
        tag:          v["tag_name"].as_str().unwrap_or_default().to_string(),
        author:       v["author"]["login"].as_str().unwrap_or("unknown").to_string(),
        published_at: v["published_at"].as_str().unwrap_or_default().to_string(),
        body:         v["body"].as_str().unwrap_or_default().chars().take(200).collect(),
        html_url:     v["html_url"].as_str().unwrap_or_default().to_string(),
    })
}

pub async fn fetch_latest() -> Result<ReleaseInfo> {
    fetch_release(&format!("https://api.github.com/repos/{}/releases/latest", vulp_repo())).await
}

pub async fn fetch_current(version: &str) -> Result<ReleaseInfo> {
    fetch_release(&format!("https://api.github.com/repos/{}/releases/tags/v{version}", vulp_repo())).await
}

pub async fn fetch_user_since(username: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent("vulp/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(format!("https://api.github.com/users/{username}")).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("GitHub API {} — {}", status, text);
    }
    let v: Value = serde_json::from_str(&text).context("parsing user JSON")?;
    let created = v["created_at"].as_str().unwrap_or_default();
    Ok(created.split('T').next().unwrap_or_default().to_string())
}

pub fn is_outdated(current_version: &str, latest_tag: &str) -> bool {
    let latest = latest_tag.trim_start_matches('v');
    !latest.is_empty() && latest != current_version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outdated_compares_against_the_latest_tag() {
        assert!(!is_outdated("0.1.0", "v0.1.0"));
        assert!(!is_outdated("0.1.0", "0.1.0"));
        assert!(is_outdated("0.1.0", "v0.2.0"));
        assert!(!is_outdated("0.1.0", ""));
        assert!(!is_outdated("0.1.0", "v"));
    }

    #[test]
    fn repo_defaults_to_the_published_one() {
        if std::env::var_os("VULP_REPO").is_none() {
            assert_eq!(vulp_repo(), "dualfroz/vulp");
        }
    }
}

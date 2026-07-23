//! Fetches GitHub release metadata for the hub's General Settings update
//! tile: the latest published release, and (separately) the release that
//! matches the version currently running.

use anyhow::{Context, Result};
use serde_json::Value;

/// The GitHub repo this binary is published from. Shared with the installer
/// scripts (`install/vulp-installer.sh`/`.ps1`), which hit the same
/// `releases/latest` endpoint to fetch the newest build. Overridable via
/// `$VULP_REPO` (same env var the installers already honor), so a fork or a
/// future rename doesn't require a rebuild just to fix the update-check tile.
fn vulp_repo() -> String {
    std::env::var("VULP_REPO").unwrap_or_else(|_| "dualfroz/fxg".to_string())
}

/// One GitHub release's metadata, trimmed to what the update tile shows.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag:          String,
    pub author:       String,
    pub published_at: String,
    #[allow(dead_code)] pub body: String, // kept: release-notes preview (roadmap)
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

/// Info about the very latest release on GitHub, regardless of what's running.
pub async fn fetch_latest() -> Result<ReleaseInfo> {
    fetch_release(&format!("https://api.github.com/repos/{}/releases/latest", vulp_repo())).await
}

/// Info about the release matching the version actually running, if one was
/// tagged (`v{CARGO_PKG_VERSION}`) — lets the tile show who shipped *this*
/// build and when, not just what's newest.
pub async fn fetch_current(version: &str) -> Result<ReleaseInfo> {
    fetch_release(&format!("https://api.github.com/repos/{}/releases/tags/v{version}", vulp_repo())).await
}

/// The `created_at` date (YYYY-MM-DD) of a GitHub user/org — for the General
/// Settings tile's account "since" line. Unauthenticated public endpoint.
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

/// `true` when the latest published tag differs from the version running.
pub fn is_outdated(current_version: &str, latest_tag: &str) -> bool {
    let latest = latest_tag.trim_start_matches('v');
    !latest.is_empty() && latest != current_version
}

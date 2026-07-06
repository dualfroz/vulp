//! Converts git remote URLs (SSH or HTTPS) into the browser-facing web URL
//! for the same repo, used by every "open in browser" action.

/// Rewrites `git_url` (`git@host:owner/repo.git` or an HTTPS clone URL) into
/// `https://host/owner/repo`. Already-HTTPS URLs are returned with just the
/// `.git` suffix trimmed; anything that doesn't look like either form is
/// returned unchanged rather than guessed at.
pub fn to_web_url(git_url: &str) -> String {
    if git_url.starts_with("http") { return git_url.trim_end_matches(".git").to_string(); }
    if let Some(colon) = git_url.rfind(':') {
        let host = git_url[..colon].split('@').next_back().unwrap_or("");
        let path = git_url[colon + 1..].trim_end_matches(".git");
        return format!("https://{}/{}", host, path);
    }
    git_url.to_string()
}

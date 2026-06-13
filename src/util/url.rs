pub fn to_web_url(git_url: &str) -> String {
    if git_url.starts_with("http") { return git_url.trim_end_matches(".git").to_string(); }
    if let Some(colon) = git_url.rfind(':') {
        let host = git_url[..colon].split('@').last().unwrap_or("");
        let path = git_url[colon + 1..].trim_end_matches(".git");
        return format!("https://{}/{}", host, path);
    }
    git_url.to_string()
}

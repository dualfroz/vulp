pub fn to_web_url(git_url: &str) -> String {
    if git_url.starts_with("http") { return git_url.trim_end_matches(".git").to_string(); }
    if let Some(colon) = git_url.rfind(':') {
        let host = git_url[..colon].split('@').next_back().unwrap_or("");
        let path = git_url[colon + 1..].trim_end_matches(".git");
        return format!("https://{}/{}", host, path);
    }
    git_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::to_web_url;

    #[test]
    fn remote_urls_become_browser_urls() {
        assert_eq!(to_web_url("git@github.com:octo/hello.git"), "https://github.com/octo/hello");
        assert_eq!(to_web_url("git@gitlab.example.com:group/sub/proj.git"), "https://gitlab.example.com/group/sub/proj");
        assert_eq!(to_web_url("https://github.com/octo/hello.git"), "https://github.com/octo/hello");
        assert_eq!(to_web_url("http://gitea.local/team/tool"), "http://gitea.local/team/tool");
        assert_eq!(to_web_url("/srv/git/hello.git"), "/srv/git/hello.git");
    }
}

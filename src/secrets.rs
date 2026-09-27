use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
    pub file:    String,
    pub pattern: &'static str,
}

pub fn scan_staged(repo: &Path) -> Vec<SecretHit> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(repo)
        .args(["diff", "--cached", "--no-color", "--unified=0"])
        .output();
    let Ok(out) = out else { return vec![] };
    scan_diff(&String::from_utf8_lossy(&out.stdout))
}

pub fn scan_diff(diff: &str) -> Vec<SecretHit> {
    let mut hits: Vec<SecretHit> = Vec::new();
    let mut file = String::from("?");
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            file = rest.to_string();
            continue;
        }
        if line.starts_with("+++") { continue; }
        let Some(added) = line.strip_prefix('+') else { continue };
        if let Some(pattern) = line_secret(added) {
            let hit = SecretHit { file: file.clone(), pattern };
            if !hits.contains(&hit) { hits.push(hit); }
        }
    }
    hits
}

fn line_secret(line: &str) -> Option<&'static str> {
    if token_after_prefix(line, "ghp_", 20)        { return Some("GitHub token (ghp_…)"); }
    if token_after_prefix(line, "github_pat_", 20) { return Some("GitHub fine-grained token"); }
    if token_after_prefix(line, "glpat-", 10)
        || token_after_prefix(line, "glpat_", 10)  { return Some("GitLab token (glpat…)"); }
    if aws_access_key(line)                        { return Some("AWS access key (AKIA…)"); }
    if discord_token_assignment(line)              { return Some("Discord token assignment"); }
    if line.contains("-----BEGIN") && line.contains("PRIVATE KEY") {
        return Some("private key block");
    }
    None
}

fn token_after_prefix(line: &str, prefix: &str, min_tail: usize) -> bool {
    let mut search = 0;
    while let Some(i) = line[search..].find(prefix) {
        let after = &line[search + i + prefix.len()..];
        let tail = after.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .count();
        if tail >= min_tail { return true; }
        search += i + prefix.len();
    }
    false
}

fn aws_access_key(line: &str) -> bool {
    let mut search = 0;
    while let Some(i) = line[search..].find("AKIA") {
        let abs = search + i;
        let tail = line[abs + 4..].chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            .count();
        if tail >= 16 { return true; }
        search = abs + 4;
    }
    false
}

fn discord_token_assignment(line: &str) -> bool {
    line.find("DISCORD_TOKEN")
        .map(|i| line[i + "DISCORD_TOKEN".len()..].trim_start().starts_with('='))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testgit::{commit_file, git, init_repo};

    fn github_token() -> String { format!("ghp_{}", "a1B2".repeat(9)) }
    fn aws_key() -> String { format!("AKIA{}", "Q7RZ4XK2M9PLW3VN") }
    fn diff_adding(file: &str, line: &str) -> String {
        format!("diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -0,0 +1 @@\n+{line}\n")
    }

    #[test]
    fn detects_every_credential_shape() {
        let cases = [
            (format!("token = \"{}\"", github_token()), "GitHub token (ghp_…)"),
            (format!("auth: github_pat_{}", "x".repeat(30)), "GitHub fine-grained token"),
            (format!("GITLAB=glpat-{}", "z9".repeat(10)), "GitLab token (glpat…)"),
            (format!("GITLAB=glpat_{}", "z9".repeat(10)), "GitLab token (glpat…)"),
            (format!("key_id = {}", aws_key()), "AWS access key (AKIA…)"),
            ("DISCORD_TOKEN   =  abc".to_string(), "Discord token assignment"),
            (format!("{} RSA PRIVATE KEY-----", "-----BEGIN"), "private key block"),
        ];
        for (line, expected) in cases {
            let hits = scan_diff(&diff_adding("conf.env", &line));
            assert_eq!(hits, vec![SecretHit { file: "conf.env".into(), pattern: expected }], "line: {line}");
        }
    }

    #[test]
    fn ignores_prose_that_only_mentions_a_prefix() {
        let lines = [
            "tokens start with ghp_ and are 40 chars long",
            "glpat- prefix is used by GitLab",
            "AKIA alone is not a key",
            "let discord_token = read_env();",
            "DISCORD_TOKEN is read from the environment",
            "-----BEGIN CERTIFICATE-----",
        ];
        for line in lines {
            assert!(scan_diff(&diff_adding("notes.md", line)).is_empty(), "false positive: {line}");
        }
    }

    #[test]
    fn only_added_lines_count_and_hits_are_per_file_and_deduplicated() {
        let token = github_token();
        let diff = format!(
            "+++ b/a.txt\n+{token}\n+again {token}\n-{key}\n+++ /dev/null\n+++ b/b.txt\n context {token}\n+{key}\n",
            key = aws_key(),
        );
        let hits = scan_diff(&diff);
        assert_eq!(hits, vec![
            SecretHit { file: "a.txt".into(), pattern: "GitHub token (ghp_…)" },
            SecretHit { file: "b.txt".into(), pattern: "AWS access key (AKIA…)" },
        ]);
    }

    #[test]
    fn hits_never_carry_the_secret_value() {
        let token = github_token();
        let hits = scan_diff(&diff_adding("x", &token));
        assert_eq!(hits.len(), 1);
        assert!(!format!("{hits:?}").contains(&token));
    }

    #[test]
    fn scan_staged_reads_the_index_not_the_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(&dir.path().join("r"));
        commit_file(&repo, "README.md", "hello\n", "init");
        assert!(scan_staged(&repo).is_empty());

        std::fs::write(repo.join("unstaged.env"), github_token()).unwrap();
        assert!(scan_staged(&repo).is_empty());

        std::fs::write(repo.join("staged.env"), format!("AWS={}\n", aws_key())).unwrap();
        git(&repo, &["add", "staged.env"]);
        assert_eq!(scan_staged(&repo), vec![
            SecretHit { file: "staged.env".into(), pattern: "AWS access key (AKIA…)" },
        ]);
    }

    #[test]
    fn scan_staged_outside_a_repo_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan_staged(dir.path()).is_empty());
    }
}

//! Staged-changes secret scanning, run before every vulp-driven commit+push
//! (TUI sync flow and `vulp sync --action commit-push`). Matches the added
//! lines of `git diff --cached` against known credential shapes; a hit
//! blocks the commit (CLI override: `--allow-secrets`). Hand-rolled
//! matchers — the shapes are simple prefixes, not worth a regex dependency.

use std::path::Path;

/// One detected secret: which file and which credential shape. The matched
/// text itself is deliberately not carried anywhere — reporting a secret
/// must not re-leak it into terminals or logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHit {
    pub file:    String,
    pub pattern: &'static str,
}

/// Scans the staged diff of `repo` (added lines only). Returns no hits when
/// git itself fails — sync's own git calls will surface that error instead.
pub fn scan_staged(repo: &Path) -> Vec<SecretHit> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(repo)
        .args(["diff", "--cached", "--no-color", "--unified=0"])
        .output();
    let Ok(out) = out else { return vec![] };
    scan_diff(&String::from_utf8_lossy(&out.stdout))
}

/// Scans unified-diff text, reporting each (file, pattern) pair once.
pub fn scan_diff(diff: &str) -> Vec<SecretHit> {
    let mut hits: Vec<SecretHit> = Vec::new();
    let mut file = String::from("?");
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            file = rest.to_string();
            continue;
        }
        if line.starts_with("+++") { continue; } // "+++ /dev/null"
        let Some(added) = line.strip_prefix('+') else { continue };
        if let Some(pattern) = line_secret(added) {
            let hit = SecretHit { file: file.clone(), pattern };
            if !hits.contains(&hit) { hits.push(hit); }
        }
    }
    hits
}

/// The credential shape found in `line`, if any.
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

/// Whether `prefix` occurs followed by at least `min_tail` token characters
/// — filters out prose/code that merely mentions the prefix.
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

/// `AKIA` followed by 16 uppercase alphanumerics (AWS access key ID shape).
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

/// A `DISCORD_TOKEN = <value>` style assignment (any spacing around `=`).
fn discord_token_assignment(line: &str) -> bool {
    line.find("DISCORD_TOKEN")
        .map(|i| line[i + "DISCORD_TOKEN".len()..].trim_start().starts_with('='))
        .unwrap_or(false)
}

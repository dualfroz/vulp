use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployState {
    InSync,
    Behind(u32),
    Diverged,
}

impl DeployState {
    pub fn as_str(&self) -> String {
        match self {
            Self::InSync    => "in_sync".into(),
            Self::Behind(n) => format!("behind:{n}"),
            Self::Diverged  => "diverged".into(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        if s == "in_sync"   { return Some(Self::InSync); }
        if s == "diverged"  { return Some(Self::Diverged); }
        s.strip_prefix("behind:")?.parse().ok().map(Self::Behind)
    }
}

pub fn is_configured(cfg: &AppConfig) -> bool {
    !cfg.deploy_ssh_cmd.trim().is_empty() && !cfg.deploy_root.trim().is_empty()
}

pub fn scan_deploy_heads(cfg: &AppConfig) -> Option<HashMap<String, String>> {
    if !is_configured(cfg) { return None; }
    let root = cfg.deploy_root.trim().trim_end_matches('/');
    let remote_script = format!(
        "find {root} -maxdepth 10 -type d -name .git 2>/dev/null | while IFS= read -r g; do \
         d=$(dirname \"$g\"); sha=$(git -C \"$d\" rev-parse --verify -q HEAD 2>/dev/null); \
         printf '%s\\037%s\\n' \"$d\" \"$sha\"; done"
    );

    let mut parts = cfg.deploy_ssh_cmd.split_whitespace();
    let program = parts.next()?;
    let base_args: Vec<&str> = parts.collect();

    let run = || -> Option<String> {
        let out = std::process::Command::new(program)
            .args(&base_args)
            .arg(&remote_script)
            .output()
            .ok()?;
        if !out.status.success() { return None; }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let output = run().or_else(run)?;

    let mut heads = HashMap::new();
    for line in output.lines() {
        let Some((path, sha)) = line.split_once('\u{1f}') else { continue };
        let sha = sha.trim();
        if sha.is_empty() { continue; }
        let Some(rel) = path.strip_prefix(root) else { continue };
        heads.insert(rel.trim_start_matches('/').to_string(), sha.to_string());
    }
    Some(heads)
}

fn local_head_sha(path: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path).args(["rev-parse", "HEAD"])
        .output().ok()?;
    if !out.status.success() { return None; }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

fn commits_behind(path: &Path, deployed_sha: &str) -> Option<u32> {
    let is_ancestor = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["merge-base", "--is-ancestor", deployed_sha, "HEAD"])
        .output().ok()?;
    if !is_ancestor.status.success() { return None; }
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["rev-list", "--count", &format!("{deployed_sha}..HEAD")])
        .output().ok()?;
    if !out.status.success() { return None; }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

pub fn drift_for(
    repo_path: &Path, workspace_root: &Path, deploy_heads: &HashMap<String, String>,
) -> Option<DeployState> {
    let rel = repo_path.strip_prefix(workspace_root).ok()?;
    let deployed_sha = deploy_heads.get(&rel.to_string_lossy().replace('\\', "/"))?;
    let local_sha = local_head_sha(repo_path)?;
    if &local_sha == deployed_sha {
        return Some(DeployState::InSync);
    }
    match commits_behind(repo_path, deployed_sha) {
        Some(n) => Some(DeployState::Behind(n)),
        None    => Some(DeployState::Diverged),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testgit::{commit_file, git, init_repo};

    #[test]
    fn state_string_form_round_trips() {
        for state in [DeployState::InSync, DeployState::Behind(0), DeployState::Behind(42), DeployState::Diverged] {
            assert_eq!(DeployState::parse(&state.as_str()), Some(state));
        }
        assert_eq!(DeployState::Behind(3).as_str(), "behind:3");
        for junk in ["", "behind:", "behind:x", "behind:-1", "ahead:2", "IN_SYNC"] {
            assert_eq!(DeployState::parse(junk), None, "{junk}");
        }
    }

    #[test]
    fn feature_needs_both_command_and_root() {
        let mut cfg = AppConfig::default();
        assert!(!is_configured(&cfg));
        cfg.deploy_ssh_cmd = "ssh host".into();
        assert!(!is_configured(&cfg));
        cfg.deploy_root = "   ".into();
        assert!(!is_configured(&cfg));
        cfg.deploy_root = "/srv".into();
        assert!(is_configured(&cfg));
        assert_eq!(scan_deploy_heads(&AppConfig::default()), None);
    }

    #[test]
    fn drift_against_a_deployed_sha() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("ws");
        let repo = init_repo(&workspace.join("group/app"));
        let first = commit_file(&repo, "a.txt", "1", "one");
        let mut heads = HashMap::new();

        assert_eq!(drift_for(&repo, &workspace, &heads), None);

        heads.insert("group/app".to_string(), first.clone());
        assert_eq!(drift_for(&repo, &workspace, &heads), Some(DeployState::InSync));

        commit_file(&repo, "a.txt", "2", "two");
        commit_file(&repo, "a.txt", "3", "three");
        assert_eq!(drift_for(&repo, &workspace, &heads), Some(DeployState::Behind(2)));

        git(&repo, &["checkout", "-q", "--orphan", "rewritten"]);
        commit_file(&repo, "b.txt", "x", "rewritten history");
        assert_eq!(drift_for(&repo, &workspace, &heads), Some(DeployState::Diverged));

        heads.insert("group/app".to_string(), "0".repeat(40));
        assert_eq!(drift_for(&repo, &workspace, &heads), Some(DeployState::Diverged));

        let outside = init_repo(&dir.path().join("elsewhere"));
        commit_file(&outside, "c.txt", "c", "c");
        assert_eq!(drift_for(&outside, &workspace, &heads), None);
    }

    #[cfg(unix)]
    #[test]
    fn scans_mirrored_heads_with_one_command() {
        let dir = tempfile::tempdir().unwrap();
        let deploy_root = dir.path().join("srv");
        let api = init_repo(&deploy_root.join("clients/api"));
        let api_head = commit_file(&api, "main.rs", "fn main() {}", "api");
        let site = init_repo(&deploy_root.join("site"));
        let site_head = commit_file(&site, "index.html", "<html>", "site");
        init_repo(&deploy_root.join("empty"));
        std::fs::create_dir_all(deploy_root.join("not-a-repo")).unwrap();

        let cfg = AppConfig {
            deploy_ssh_cmd: "sh -c".into(),
            deploy_root: format!("{}/", deploy_root.display()),
            ..AppConfig::default()
        };
        let heads = scan_deploy_heads(&cfg).expect("command succeeds");
        let expected: HashMap<String, String> = [
            ("clients/api".to_string(), api_head),
            ("site".to_string(), site_head),
        ].into_iter().collect();
        assert_eq!(heads, expected);
    }

    #[cfg(unix)]
    #[test]
    fn failing_remote_command_disables_drift() {
        let cfg = AppConfig {
            deploy_ssh_cmd: "false".into(),
            deploy_root: "/srv".into(),
            ..AppConfig::default()
        };
        assert_eq!(scan_deploy_heads(&cfg), None);
    }
}

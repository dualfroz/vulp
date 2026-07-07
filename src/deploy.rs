//! Deploy drift: is what's on the deploy host actually what's in the repo?
//! Auto-discovered from a 1:1 mirror convention (`deploy_root` on the host
//! mirrors `workspace_root` locally past those two roots) rather than
//! per-repo metadata the user would have to maintain by hand — one batched
//! remote command lists every mirrored repo's HEAD sha, and repos with no
//! mirror (never deployed there — e.g. this very tool, which is a pure
//! open-source dev CLI) simply have no entry, not an error.

use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::Path;

/// One repo's drift relative to the deploy host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployState {
    InSync,
    Behind(u32),
    /// The deployed sha isn't reachable from local HEAD — a rebase, a
    /// force-push, or (as happened to this very repo) a history rewrite.
    Diverged,
}

impl DeployState {
    /// Stable string form for the on-disk cache and `--json` output.
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

/// Whether the feature is configured at all (`deploy_ssh_cmd` + `deploy_root`
/// both set). Callers should skip the SSH round-trip entirely when this is
/// false rather than attempting it and failing.
pub fn is_configured(cfg: &AppConfig) -> bool {
    !cfg.deploy_ssh_cmd.trim().is_empty() && !cfg.deploy_root.trim().is_empty()
}

/// Runs `deploy_ssh_cmd` once to list every git repo under `deploy_root` on
/// the deploy host with its current HEAD sha, keyed by path relative to
/// `deploy_root`. One round-trip covers every repo — the whole point of the
/// mirror convention is never needing a request per repo. `None` when the
/// feature isn't configured or the command fails outright (transient SSH
/// drops are retried once, per the known flakiness of this specific setup).
pub fn scan_deploy_heads(cfg: &AppConfig) -> Option<HashMap<String, String>> {
    if !is_configured(cfg) { return None; }
    let root = cfg.deploy_root.trim().trim_end_matches('/');
    let remote_script = format!(
        "find {root} -maxdepth 10 -type d -name .git 2>/dev/null | while IFS= read -r g; do \
         d=$(dirname \"$g\"); sha=$(git -C \"$d\" rev-parse HEAD 2>/dev/null); \
         printf '%s\\x1f%s\\n' \"$d\" \"$sha\"; done"
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
    // One retry — this VPS's SSH is known to drop intermittently.
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

/// The local repo's own HEAD sha.
fn local_head_sha(path: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path).args(["rev-parse", "HEAD"])
        .output().ok()?;
    if !out.status.success() { return None; }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

/// How many commits `deployed_sha` is behind local HEAD, or `None` if
/// `deployed_sha` isn't an ancestor reachable from HEAD at all (rebase,
/// force-push, or a history rewrite like this repo's own `filter-repo` run —
/// in that case there's no meaningful count, only "diverged").
fn commits_behind(path: &Path, deployed_sha: &str) -> Option<u32> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["rev-list", "--count", &format!("{deployed_sha}..HEAD")])
        .output().ok()?;
    if !out.status.success() { return None; }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Computes drift for one repo. `None` = no mirror found on the deploy host
/// (not applicable — e.g. a pure open-source component that never runs
/// live) or the feature isn't configured; distinguishing "not deployed"
/// from "checked, in sync" is the whole reason this returns `Option`.
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

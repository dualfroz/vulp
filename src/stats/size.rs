//! Repo size, measured as two separate signals rather than one sum: git
//! object-store size (a growing number usually means bloating history —
//! binaries, accidentally committed build output) and working-tree size
//! (usually just `node_modules`/`target`, unrelated to the git history
//! problem). Conflating them would have hidden exactly the pathology that
//! motivated adding this metric: vulp's own `target/` growing to 539 MB in
//! its committed history.

/// One repo's size, in KiB.
#[derive(Debug, Clone, Copy, Default)]
pub struct RepoSize {
    /// Loose + packed git objects (`git count-objects -v`).
    pub git_kb:  u64,
    /// Working tree excluding `.git` (`du -sk --exclude=.git`).
    pub tree_kb: u64,
}

pub fn repo_size(path: &std::path::Path) -> RepoSize {
    RepoSize { git_kb: count_objects_kb(path), tree_kb: du_kb_excluding_git(path) }
}

fn count_objects_kb(path: &std::path::Path) -> u64 {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path).args(["count-objects", "-v"])
        .output();
    let Ok(out) = out else { return 0 };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut size = 0u64;
    let mut size_pack = 0u64;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("size: ")      { size      = v.trim().parse().unwrap_or(0); }
        if let Some(v) = line.strip_prefix("size-pack: ")  { size_pack = v.trim().parse().unwrap_or(0); }
    }
    size + size_pack
}

fn du_kb_excluding_git(path: &std::path::Path) -> u64 {
    let out = std::process::Command::new("du")
        .args(["-sk", "--exclude=.git"])
        .arg(path)
        .output();
    let Ok(out) = out else { return 0 };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

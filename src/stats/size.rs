#[derive(Debug, Clone, Copy, Default)]
pub struct RepoSize {
    pub git_kb:  u64,
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

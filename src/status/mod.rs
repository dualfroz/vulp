pub mod detail;

use crate::cli::StatusArgs;
use crate::config::AppConfig;
use crate::workspace::load_workspaces;
use anyhow::Result;
use chrono::Utc;
use git2::{Repository, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub name:            String,
    pub path:            PathBuf,
    pub branch:          String,
    pub ahead:           usize,
    pub behind:          usize,
    pub is_dirty:        bool,
    pub last_commit_msg: String,
    pub last_commit_age: String,
    pub last_commit_ts:  i64,
    pub last_tag:        Option<String>,
    pub visibility:      Option<String>,
    pub owner:           Option<String>,  // parsed from git remote URL
    pub license:         bool,
    pub lang:            Option<String>,
    pub bookmarked:      bool,
    pub stars:           Option<u32>,
    pub open_issues:     Option<u32>,
    pub open_prs:        Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CommitLine {
    pub sha:    String,
    pub msg:    String,
    pub age:    String,
    pub author: String,
}

#[derive(Debug, Clone)]
pub struct RepoDetail {
    pub summary:        RepoSummary,
    pub modified_files: Vec<(char, String)>,
    pub recent_commits: Vec<CommitLine>,
    #[allow(dead_code)] pub meta_issues: Option<u32>,
    #[allow(dead_code)] pub meta_prs:    Option<u32>,
    #[allow(dead_code)] pub meta_stars:  Option<u32>,
}

// ── owner parsing ─────────────────────────────────────────────────────────────

fn parse_owner_from_url(url: &str) -> Option<String> {
    // https://github.com/owner/repo[.git]
    if url.starts_with("http") {
        let parts: Vec<&str> = url.trim_end_matches('/').trim_end_matches(".git").split('/').collect();
        let n = parts.len();
        if n >= 2 { return Some(parts[n - 2].to_string()); }
    }
    // git@github.com:owner/repo.git
    if let Some(colon) = url.rfind(':') {
        let after = url[colon + 1..].trim_start_matches('/');
        if let Some(slash) = after.find('/') {
            return Some(after[..slash].to_string());
        }
    }
    None
}

fn extract_owner(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    parse_owner_from_url(remote.url()?)
}

// ── license + language detection ──────────────────────────────────────────────

pub fn detect_license(path: &Path) -> bool {
    ["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENSE-MIT", "LICENSE-APACHE", "COPYING", "COPYING.md"]
        .iter().any(|f| path.join(f).exists())
}

pub fn detect_language(path: &Path) -> Option<String> {
    // Fast path: check indicator files instead of scanning the full git index.
    const INDICATORS: &[(&str, &str)] = &[
        ("Cargo.toml",       "Rust"),
        ("go.mod",           "Go"),
        ("pyproject.toml",   "Python"),
        ("requirements.txt", "Python"),
        ("setup.py",         "Python"),
        ("pom.xml",          "Java"),
        ("build.gradle",     "Java"),
        ("build.gradle.kts", "Kotlin"),
        ("Gemfile",          "Ruby"),
        ("composer.json",    "PHP"),
        ("Package.swift",    "Swift"),
        ("mix.exs",          "Elixir"),
        ("*.cabal",          "Haskell"),
    ];
    for (file, lang) in INDICATORS {
        if path.join(file).exists() { return Some(lang.to_string()); }
    }
    if path.join("package.json").exists() {
        let lang = if path.join("tsconfig.json").exists() { "TypeScript" } else { "JavaScript" };
        return Some(lang.to_string());
    }
    None
}

// ── lazy per-repo extras ───────────────────────────────────────────────────────

pub fn count_tracked_files(path: &Path) -> usize {
    let repo = match Repository::open(path) { Ok(r) => r, Err(_) => return 0 };
    repo.index().map(|i| i.len()).unwrap_or(0)
}

pub fn repo_recent_commits(path: &Path, n: usize) -> Vec<(String, String)> {
    let repo = match Repository::open(path) { Ok(r) => r, Err(_) => return vec![] };
    let mut revwalk = match repo.revwalk() { Ok(r) => r, Err(_) => return vec![] };
    let _ = revwalk.push_head();
    revwalk.take(n)
        .filter_map(|id| {
            let id = id.ok()?;
            let commit = repo.find_commit(id).ok()?;
            let msg = commit.summary().unwrap_or("").chars().take(45).collect::<String>();
            let age = format_age(commit.time().seconds());
            Some((age, msg))
        })
        .collect()
}

// ── git helpers ───────────────────────────────────────────────────────────────

pub fn format_age_pub(secs: i64) -> String { format_age(secs) }

fn format_age(secs: i64) -> String {
    let diff = Utc::now().timestamp() - secs;
    match diff {
        d if d < 3600       => format!("{}m ago", d / 60),
        d if d < 86400      => format!("{}h ago", d / 3600),
        d if d < 86400 * 2  => "yesterday".into(),
        d                   => format!("{}d ago", d / 86400),
    }
}

fn ahead_behind(repo: &Repository, branch: &str) -> (usize, usize) {
    let local = match repo.head().and_then(|h| h.peel_to_commit()) {
        Ok(c)  => c.id(),
        Err(_) => return (0, 0),
    };
    let upstream_ref = format!("refs/remotes/origin/{}", branch);
    match repo.refname_to_id(&upstream_ref) {
        Ok(upstream) => repo.graph_ahead_behind(local, upstream).unwrap_or((0, 0)),
        Err(_)       => (0, 0),
    }
}

fn latest_tag(repo: &Repository) -> Option<String> {
    let mut best: Option<(i64, String)> = None;
    if let Ok(names) = repo.tag_names(None) {
        for name in names.iter().flatten() {
            let refname = format!("refs/tags/{}", name);
            let ts = repo.revparse_single(&refname).ok()
                .and_then(|obj| obj.peel_to_commit().ok())
                .map(|c| c.time().seconds());
            if let Some(ts) = ts {
                if best.as_ref().map_or(true, |(t, _)| ts > *t) {
                    best = Some((ts, name.to_string()));
                }
            }
        }
    }
    best.map(|(_, n)| n)
}

pub fn repo_summary(path: &Path) -> Result<RepoSummary> {
    let t_total = std::time::Instant::now();

    let repo   = Repository::open(path)?;
    let head   = repo.head()?;
    let branch = head.shorthand().unwrap_or("HEAD").to_string();

    let commit = head.peel_to_commit()?;
    let msg    = commit.summary().unwrap_or("").chars().take(60).collect::<String>();
    let age    = format_age(commit.time().seconds());
    let ts     = commit.time().seconds();

    let t_tag = std::time::Instant::now();
    let last_tag = latest_tag(&repo);
    let tag_ms   = t_tag.elapsed().as_millis();

    let owner   = extract_owner(&repo);
    let license = detect_license(path);

    let t_lang = std::time::Instant::now();
    let lang   = detect_language(path);
    let lang_ms = t_lang.elapsed().as_millis();

    let t_status = std::time::Instant::now();
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    let is_dirty = repo.statuses(Some(&mut opts)).map(|s| !s.is_empty()).unwrap_or(false);
    let status_ms = t_status.elapsed().as_millis();

    let (ahead, behind) = ahead_behind(&repo, &branch);
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    crate::debug::log(format!(
        "repo[{name}] total={}ms  tag={tag_ms}ms  lang={lang_ms}ms  status={status_ms}ms",
        t_total.elapsed().as_millis()
    ));

    Ok(RepoSummary {
        name, path: path.to_path_buf(), branch, ahead, behind, is_dirty,
        last_commit_msg: msg, last_commit_age: age, last_commit_ts: ts,
        last_tag, visibility: None, owner, license, lang, bookmarked: false,
        stars: None, open_issues: None, open_prs: None,
    })
}

pub fn repo_detail(path: &Path) -> Result<RepoDetail> {
    let repo = Repository::open(path)?;
    let summary = repo_summary(path)?;

    // Modified files
    let mut sopts = StatusOptions::new();
    sopts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut sopts))?;
    let modified_files = statuses.iter().map(|e| {
        let st = e.status();
        let ch = if st.contains(git2::Status::INDEX_NEW) || st.contains(git2::Status::WT_NEW) { '?' }
                 else if st.contains(git2::Status::INDEX_DELETED) || st.contains(git2::Status::WT_DELETED) { 'D' }
                 else { 'M' };
        (ch, e.path().unwrap_or("").to_string())
    }).collect();

    // Recent commits
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    let recent_commits: Vec<CommitLine> = revwalk.take(5).filter_map(|id| {
        let id = id.ok()?;
        let commit = repo.find_commit(id).ok()?;
        let sha    = id.to_string()[..7].to_string();
        let msg    = commit.summary().unwrap_or("").chars().take(50).collect::<String>();
        let age    = format_age(commit.time().seconds());
        let author = commit.author().name().unwrap_or("").to_string();
        Some(CommitLine { sha, msg, age, author })
    }).collect();

    Ok(RepoDetail { summary, modified_files, recent_commits, meta_issues: None, meta_prs: None, meta_stars: None })
}

/// Background streaming scan — sends repos to the TUI as they are found.
/// The channel closes when the scan finishes; the receiver side triggers a final sort.
pub fn scan_workspace_stream(root: PathBuf) -> mpsc::Receiver<RepoSummary> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let labels = crate::labels::load_labels(&root);
        scan_recursive_tx(&root, &root, &tx, 5, &labels);
    });
    rx
}

fn scan_recursive_tx(
    dir: &Path, root: &Path,
    tx: &mpsc::Sender<RepoSummary>,
    depth: u8,
    labels: &HashMap<PathBuf, String>,
) {
    if depth == 0 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        if SKIP_DIRS.iter().any(|s| *s == fname.as_ref()) { continue; }

        if path.join(".git").exists() {
            if let Ok(mut s) = repo_summary(&path) {
                if let Ok(rel) = path.strip_prefix(root) {
                    s.name = rel.to_string_lossy().to_string();
                }
                if let Some(label) = labels.get(&path) {
                    s.name = label.clone();
                }
                let _ = tx.send(s);
            }
        } else {
            scan_recursive_tx(&path, root, tx, depth - 1, labels);
        }
    }
}

pub fn scan_workspace(root: &Path, _groups: &[String]) -> Result<Vec<RepoSummary>> {
    crate::debug::log(format!("scan_workspace: {}", root.display()));
    let mut repos = Vec::new();
    let labels = crate::labels::load_labels(root);
    scan_recursive(root, root, &mut repos, 5);
    crate::debug::log(format!("scan done: {} repos", repos.len()));

    // Apply labels: if a repo has an assigned display name, use it
    for r in &mut repos {
        if let Some(label) = labels.get(&r.path) {
            r.name = label.clone();
        }
    }

    // Sort: active repos (dirty or behind) first, then by last commit timestamp desc
    repos.sort_by(|a, b| {
        let a_active = a.is_dirty || a.behind > 0;
        let b_active = b.is_dirty || b.behind > 0;
        b_active.cmp(&a_active).then(b.last_commit_ts.cmp(&a.last_commit_ts))
    });
    repos.dedup_by(|a, b| a.path == b.path);
    Ok(repos)
}

const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", ".cache", "vendor",
    ".npm", ".cargo", "__pycache__", ".venv", "venv", "dist", "build",
];

fn scan_recursive(dir: &Path, root: &Path, repos: &mut Vec<RepoSummary>, depth: u8) {
    if depth == 0 { return; }
    let entries = match std::fs::read_dir(dir) {
        Ok(e)  => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        if SKIP_DIRS.iter().any(|s| *s == fname.as_ref()) { continue; }

        if path.join(".git").exists() {
            if let Ok(mut s) = repo_summary(&path) {
                // Show relative path from workspace_root so user knows context
                if let Ok(rel) = path.strip_prefix(root) {
                    s.name = rel.to_string_lossy().to_string();
                }
                repos.push(s);
            }
        } else {
            scan_recursive(&path, root, repos, depth - 1);
        }
    }
}

pub fn apply_filter(mut repos: Vec<RepoSummary>, filter: &str) -> Vec<RepoSummary> {
    repos.retain(|r| match filter {
        "dirty"  => r.is_dirty,
        "behind" => r.behind > 0,
        "ahead"  => r.ahead > 0,
        "clean"  => !r.is_dirty && r.behind == 0,
        _        => true,
    });
    repos
}

pub async fn run(args: StatusArgs, cfg: &AppConfig) -> Result<()> {
    if let Some(repo_name) = &args.repo {
        let path = cfg.workspace_root.join(repo_name);
        let detail = repo_detail(&path)?;
        return crate::hub::run_detail_tui(detail);
    }

    if let Some(ws_name) = &args.workspace {
        let wss = load_workspaces(cfg)?;
        let ws  = wss.iter().find(|w| &w.name == ws_name)
            .ok_or_else(|| anyhow::anyhow!("workspace '{}' not found", ws_name))?;
        let mut repos = Vec::new();
        for path in crate::workspace::resolve_paths(ws, cfg) {
            if let Ok(s) = repo_summary(&path) { repos.push(s); }
        }
        if let Some(f) = &args.filter { repos = apply_filter(repos, f); }
        return crate::hub::run_overview_tui(repos, None, cfg).await;
    }

    let rx = scan_workspace_stream(cfg.workspace_root.clone());
    crate::hub::run_overview_tui(vec![], Some(rx), cfg).await
}

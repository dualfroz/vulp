pub mod ui;

use crate::cli::StatusArgs;
use crate::config::AppConfig;
use crate::workspace::load_workspaces;
use anyhow::Result;
use chrono::Utc;
use git2::{Repository, StatusOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub name:            String,
    pub path:            PathBuf,
    pub branch:          String,
    pub ahead:           usize,
    pub behind:          usize,
    pub is_dirty:        bool,
    #[allow(dead_code)]
    pub last_commit_msg: String,
    pub last_commit_age: String,
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

// ── git helpers ───────────────────────────────────────────────────────────────

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

pub fn repo_summary(path: &Path) -> Result<RepoSummary> {
    let repo = Repository::open(path)?;
    let head = repo.head()?;
    let branch = head.shorthand().unwrap_or("HEAD").to_string();

    let commit = head.peel_to_commit()?;
    let msg = commit.summary().unwrap_or("").chars().take(60).collect::<String>();
    let age = format_age(commit.time().seconds());

    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    let is_dirty = repo.statuses(Some(&mut opts)).map(|s| !s.is_empty()).unwrap_or(false);

    let (ahead, behind) = ahead_behind(&repo, &branch);
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    Ok(RepoSummary { name, path: path.to_path_buf(), branch, ahead, behind, is_dirty, last_commit_msg: msg, last_commit_age: age })
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

pub fn scan_workspace(root: &Path) -> Result<Vec<RepoSummary>> {
    let entries = match std::fs::read_dir(root) {
        Ok(e)  => e,
        Err(_) => return Ok(vec![]),
    };
    let mut repos = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join(".git").exists() {
            if let Ok(s) = repo_summary(&path) {
                repos.push(s);
            }
        }
    }
    repos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
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
        return ui::run_detail_tui(detail);
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
        return ui::run_overview_tui(repos);
    }

    let mut repos = scan_workspace(&cfg.workspace_root)?;
    if let Some(f) = &args.filter { repos = apply_filter(repos, f); }
    ui::run_overview_tui(repos)
}

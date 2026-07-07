//! Workspace scanning and per-repo summary computation: walks the workspace
//! root for git repos, computes each one's branch/ahead-behind/dirty/license/
//! language state, and streams results to the hub as they're found rather
//! than waiting for the whole workspace to finish scanning.

pub mod detail;

use crate::config::AppConfig;
use anyhow::Result;
use chrono::Utc;
use git2::{Repository, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Everything the hub's Repos table and topbar need for one repo, computed
/// once per scan.
#[derive(Debug, Clone, serde::Serialize)]
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
    /// The provider-side repo name parsed from the git remote URL — not
    /// necessarily the local folder name (see `parse_repo_name_from_url`).
    /// Provider-API slugs must use this, never `path.file_name()`.
    pub remote_repo_name: Option<String>,
    pub has_license:     bool,
    pub lang:            Option<String>,
    pub bookmarked:      bool,
    pub stars:           Option<u32>,
    pub open_issues:     Option<u32>,
    pub open_prs:        Option<u32>,
    /// Stash entries, counted from `.git/logs/refs/stash` (cheap file read,
    /// no subprocess — scan-path safe).
    pub stash_count:     usize,
    /// Latest CI run on the current branch: "success"|"failure"|"running"
    /// (from the meta cache / API refresh; None = unknown).
    pub ci_status:       Option<String>,
    /// Open Dependabot alerts (from the meta cache / API refresh).
    pub dependabot_open: Option<u32>,
    /// Deploy-drift state string (`deploy::DeployState::as_str`), from the
    /// meta cache / `[R]` refresh; None = not deployed anywhere the mirror
    /// convention can see, or the feature isn't configured.
    pub deploy_status: Option<String>,
}

/// One commit's display fields for a recent-commits list.
/// Fields currently populated but unread — kept as the seed-data model for
/// the detail view (see roadmap: per-commit actions).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CommitLine {
    pub sha:    String,
    pub msg:    String,
    pub age:    String,
    pub author: String,
}

/// Richer per-repo detail beyond `RepoSummary`, used as the seed data for
/// opening the repo detail view.
#[allow(dead_code)]
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

/// Extracts the `owner` segment from an HTTPS or SSH git remote URL.
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

/// Extracts the `repo` segment from an HTTPS or SSH git remote URL — the
/// name the provider actually knows the repo by, which is not necessarily
/// the local folder's name (a repo can be cloned into, or renamed to, any
/// directory name locally; `vulp`'s own local folder is "vulp" but its
/// GitHub repo is "fxg"). Callers building a provider-API slug must prefer
/// this over `path.file_name()`.
fn parse_repo_name_from_url(url: &str) -> Option<String> {
    if url.starts_with("http") {
        let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
        return trimmed.rsplit('/').next().map(|s| s.to_string());
    }
    // git@github.com:owner/repo.git
    if let Some(colon) = url.rfind(':') {
        let after = url[colon + 1..].trim_start_matches('/').trim_end_matches(".git");
        return after.rsplit('/').next().map(|s| s.to_string());
    }
    None
}

/// Parses the owner from `repo`'s `origin` remote, if it has one.
fn extract_owner(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    parse_owner_from_url(remote.url()?)
}

/// Parses the provider-side repo name from `repo`'s `origin` remote, if it
/// has one — see `parse_repo_name_from_url` for why this can differ from
/// the local folder name.
fn extract_remote_repo_name(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    parse_repo_name_from_url(remote.url()?)
}

// ── license + language detection ──────────────────────────────────────────────

/// Whether `path` has a recognizable license file at its root.
pub fn detect_license(path: &Path) -> bool {
    ["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENSE-MIT", "LICENSE-APACHE", "COPYING", "COPYING.md"]
        .iter().any(|f| path.join(f).exists())
}

// Categories treated as media/binary assets — only displayed when they dominate (>80% of files).
const ASSET_CATEGORIES: &[&str] = &[
    "Images", "Textures", "Audio", "Video", "3D", "Fonts",
    "PDF", "Docs", "Spreadsheets", "Slides", "eBook",
];

/// Detects the repo's dominant language(s) by file-extension counts,
/// returning up to 4 as a "`Lang NN%`" display string. Asset extensions
/// (images, audio, fonts, etc.) are excluded from the percentage base unless
/// they make up over 80% of the repo, in which case they're shown instead —
/// otherwise a repo that's mostly a code project with some bundled assets
/// would have its real language diluted by file count alone.
pub fn detect_language(path: &Path) -> Option<String> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    let mut total = 0usize;
    lang_walk(path, &mut counts, &mut total, 6);
    if total == 0 { return None; }

    let asset_total: usize = counts.iter()
        .filter(|(k, _)| ASSET_CATEGORIES.contains(k))
        .map(|(_, v)| *v)
        .sum();
    let assets_dominate = asset_total * 100 / total > 80;

    // When assets don't dominate, exclude them from both the display and the percentage base.
    let base = if assets_dominate { total } else { total - asset_total };
    if base == 0 { return None; }

    let threshold = ((base as f64 * 0.05) as usize).max(1);
    let mut langs: Vec<(&str, usize)> = counts.into_iter()
        .filter(|(k, c)| *c >= threshold && (assets_dominate || !ASSET_CATEGORIES.contains(k)))
        .collect();

    if langs.is_empty() { return None; }
    langs.sort_by(|a, b| b.1.cmp(&a.1));
    Some(langs.iter().take(4)
        .map(|(lang, c)| format!("{} {}%", lang, c * 100 / base))
        .collect::<Vec<_>>()
        .join("  "))
}

/// Maps a file extension to a display language/category name.
fn ext_to_lang(ext: &str) -> Option<&'static str> {
    Some(match ext {
        // ── code ──────────────────────────────────────────────────────────────
        "rs"                                          => "Rust",
        "go"                                          => "Go",
        "py" | "pyw"                                  => "Python",
        "js" | "mjs" | "cjs" | "jsx"                 => "JavaScript",
        "ts" | "tsx"                                  => "TypeScript",
        "java"                                        => "Java",
        "kt" | "kts"                                  => "Kotlin",
        "rb"                                          => "Ruby",
        "php"                                         => "PHP",
        "swift"                                       => "Swift",
        "ex" | "exs"                                  => "Elixir",
        "hs"                                          => "Haskell",
        "c" | "h"                                     => "C",
        "cpp" | "cc" | "cxx" | "hxx" | "hpp"         => "C++",
        "cs"                                          => "C#",
        "sh" | "bash" | "zsh" | "fish"                => "Shell",
        "html" | "htm"                                => "HTML",
        "css" | "scss" | "sass" | "less"              => "CSS",
        "lua"                                         => "Lua",
        "r"                                           => "R",
        "dart"                                        => "Dart",
        "scala"                                       => "Scala",
        "clj" | "cljs"                                => "Clojure",
        "ml" | "mli"                                  => "OCaml",
        "nim"                                         => "Nim",
        "zig"                                         => "Zig",
        "vue"                                         => "Vue",
        "svelte"                                      => "Svelte",
        "tf" | "tfvars"                               => "Terraform",
        "sql"                                         => "SQL",

        // ── docs & text ───────────────────────────────────────────────────────
        "md" | "mdx" | "rst"                          => "Markdown",
        "txt" | "rtf" | "log"                         => "Text",
        "pdf"                                         => "PDF",
        "docx" | "doc" | "odt" | "pages"              => "Docs",
        "xlsx" | "xls" | "ods" | "numbers" | "csv" | "tsv" => "Spreadsheets",
        "pptx" | "ppt" | "odp" | "keynote"           => "Slides",
        "epub"                                        => "eBook",

        // ── images ────────────────────────────────────────────────────────────
        "png" | "jpg" | "jpeg" | "gif" | "webp"
        | "svg" | "ico" | "bmp" | "tiff" | "tif"
        | "tga" | "psd" | "xcf" | "raw" | "dng"
        | "cr2" | "nef" | "heic" | "heif" | "avif"
        | "apng" | "jxl"                              => "Images",

        // ── textures (GPU / game engine specific) ─────────────────────────────
        "dds" | "ktx" | "ktx2" | "exr" | "hdr"
        | "basis" | "bc7" | "astc"                    => "Textures",

        // ── audio ─────────────────────────────────────────────────────────────
        "mp3" | "wav" | "ogg" | "flac" | "aac"
        | "m4a" | "wma" | "opus" | "mid" | "midi"
        | "aif" | "aiff" | "caf" | "ra" | "amr"      => "Audio",

        // ── video ─────────────────────────────────────────────────────────────
        "mp4" | "mkv" | "avi" | "mov" | "webm"
        | "m4v" | "wmv" | "flv" | "ogv" | "mpg"
        | "mpeg" | "m2ts" | "mts" | "vob" | "rm"     => "Video",

        // ── 3d models ─────────────────────────────────────────────────────────
        "obj" | "fbx" | "blend" | "gltf" | "glb"
        | "stl" | "dae" | "3ds" | "ply" | "abc"
        | "usd" | "usda" | "usdc" | "usdz"
        | "max" | "ma" | "mb" | "c4d" | "lwo"        => "3D",

        // ── fonts ─────────────────────────────────────────────────────────────
        "ttf" | "otf" | "woff" | "woff2" | "eot"     => "Fonts",

        // ── data / ml ─────────────────────────────────────────────────────────
        "parquet" | "arrow" | "hdf5" | "h5"
        | "npy" | "npz" | "pkl" | "mat" | "nc"       => "Data",

        _ => return None,
    })
}

/// Recursively tallies file extensions under `dir` into `counts`, stopping
/// at `depth` or once `total` hits 5000 files — a repo-wide walk only needs
/// a representative sample, not an exhaustive count, and the cap keeps huge
/// repos from making every scan slow.
fn lang_walk(
    dir: &Path,
    counts: &mut HashMap<&'static str, usize>,
    total: &mut usize,
    depth: u8,
) {
    if depth == 0 || *total >= 5000 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if SKIP_DIRS.iter().any(|s| *s == name.as_ref()) { continue; }
            lang_walk(&path, counts, total, depth - 1);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(lang) = ext_to_lang(ext) {
                *counts.entry(lang).or_insert(0) += 1;
                *total += 1;
            }
        }
    }
}

// ── lazy per-repo extras ───────────────────────────────────────────────────────

/// Number of files tracked in the git index — cheaper than walking the
/// filesystem since it's served straight from git's own index.
pub fn count_tracked_files(path: &Path) -> usize {
    let repo = match Repository::open(path) { Ok(r) => r, Err(_) => return 0 };
    repo.index().map(|i| i.len()).unwrap_or(0)
}

/// The `n` most recent commits as `(age, message)` pairs, for the Overview
/// tab's recent-activity list.
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

/// Public entry point for `format_age`, for callers outside this module
/// (detail/project-detail rendering) that need the same "Nm/h/d ago"
/// formatting without duplicating it.
pub fn format_age_pub(secs: i64) -> String { format_age(secs) }

/// Formats a Unix timestamp as a short relative-age string.
fn format_age(secs: i64) -> String {
    let diff = Utc::now().timestamp() - secs;
    match diff {
        d if d < 3600       => format!("{}m ago", d / 60),
        d if d < 86400      => format!("{}h ago", d / 3600),
        d if d < 86400 * 2  => "yesterday".into(),
        d                   => format!("{}d ago", d / 86400),
    }
}

/// Commits ahead/behind the matching `origin/<branch>`, or `(0, 0)` if
/// there's no such upstream (a purely local branch, or detached HEAD).
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

/// The name of the most recently created annotated or lightweight tag,
/// determined by the tagged commit's timestamp rather than tag creation
/// order (which git doesn't expose directly).
fn latest_tag(repo: &Repository) -> Option<String> {
    let mut best: Option<(i64, String)> = None;
    if let Ok(names) = repo.tag_names(None) {
        for name in names.iter().flatten() {
            let refname = format!("refs/tags/{}", name);
            let ts = repo.revparse_single(&refname).ok()
                .and_then(|obj| obj.peel_to_commit().ok())
                .map(|c| c.time().seconds());
            if let Some(ts) = ts {
                if best.as_ref().is_none_or(|(t, _)| ts > *t) {
                    best = Some((ts, name.to_string()));
                }
            }
        }
    }
    best.map(|(_, n)| n)
}

/// Computes a full `RepoSummary` for the repo at `path`. Treats a freshly
/// `git init`'d repo with zero commits as a valid (empty) result rather than
/// an error, so brand-new repos still show up in the hub instead of being
/// silently dropped from scan results.
pub fn repo_summary(path: &Path) -> Result<RepoSummary> {
    let t_total = std::time::Instant::now();

    let repo = Repository::open(path)?;

    // A freshly `git init`'d repo has no commits yet, so `repo.head()` returns
    // an UnbornBranch error. Treat that as a valid (empty) repo instead of
    // silently dropping it from the scan results.
    let (branch, msg, age, ts) = match repo.head() {
        Ok(head) => {
            let branch = head.shorthand().unwrap_or("HEAD").to_string();
            let commit = head.peel_to_commit()?;
            let msg    = commit.summary().unwrap_or("").chars().take(160).collect::<String>();
            let age    = format_age(commit.time().seconds());
            let ts     = commit.time().seconds();
            (branch, msg, age, ts)
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            let branch = std::fs::read_to_string(path.join(".git/HEAD")).ok()
                .and_then(|s| s.trim().strip_prefix("ref: refs/heads/").map(str::to_string))
                .unwrap_or_else(|| "HEAD".to_string());
            (branch, "(no commits yet)".to_string(), "—".to_string(), 0)
        }
        Err(e) => return Err(e.into()),
    };

    let t_tag = std::time::Instant::now();
    let last_tag = latest_tag(&repo);
    let tag_ms   = t_tag.elapsed().as_millis();

    let owner   = extract_owner(&repo);
    let remote_repo_name = extract_remote_repo_name(&repo);
    let has_license = detect_license(path);

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

    let stash_count = std::fs::read_to_string(path.join(".git/logs/refs/stash"))
        .map(|s| s.lines().count())
        .unwrap_or(0);

    Ok(RepoSummary {
        name, path: path.to_path_buf(), branch, ahead, behind, is_dirty,
        last_commit_msg: msg, last_commit_age: age, last_commit_ts: ts,
        last_tag, visibility: None, owner, remote_repo_name, has_license, lang, bookmarked: false,
        stars: None, open_issues: None, open_prs: None,
        stash_count,
        ci_status: None, dependabot_open: None, deploy_status: None,
    })
}

/// Computes a `RepoDetail` (summary plus modified-files and recent-commits
/// snapshots) for the repo at `path`.
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

/// Recursive worker for `scan_workspace_stream`: walks `dir`, sending each
/// found repo's summary to `tx` immediately rather than collecting into a
/// `Vec` first, so the hub can start showing results before the scan finishes.
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

/// Synchronous (non-streaming) full workspace scan, sorted with
/// dirty/behind repos first and otherwise by most-recently-committed.
/// `_groups` is currently unused — see `config::AppConfig::workspace_groups`'s
/// doc comment for the intended (not yet wired up) grouping behavior.
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

/// Recursive worker for `scan_workspace`, collecting results into `repos`.
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

/// Entry point for the bare `vulp` command (no subcommand): starts a
/// streaming workspace scan and opens the hub TUI against it.
pub async fn run(cfg: &AppConfig) -> Result<()> {
    let rx = scan_workspace_stream(cfg.workspace_root.clone());
    crate::hub::run_overview_tui(vec![], Some(rx), cfg).await
}

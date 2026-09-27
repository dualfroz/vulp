pub mod detail;

use crate::config::AppConfig;
use anyhow::Result;
use chrono::Utc;
use git2::{Repository, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

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
    pub branch_count:    usize,
    pub owner:           Option<String>,
    pub remote_repo_name: Option<String>,
    pub has_license:     bool,
    pub lang:            Option<String>,
    pub bookmarked:      bool,
    pub stars:           Option<u32>,
    pub open_issues:     Option<u32>,
    pub open_prs:        Option<u32>,
    pub stash_count:     usize,
    pub ci_status:       Option<String>,
    pub dependabot_open: Option<u32>,
    pub deploy_status: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CommitLine {
    pub sha:    String,
    pub msg:    String,
    pub age:    String,
    pub author: String,
}

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

fn remote_path_segments(url: &str) -> Option<Vec<&str>> {
    let path = match url.split_once("://") {
        Some((_, rest)) => &rest[rest.find('/')?..],
        None => {
            let (host, path) = url.split_once(':')?;
            if host.contains('/') { return None; }
            path
        }
    };
    let path = path.trim_matches('/').trim_end_matches(".git");
    Some(path.split('/').filter(|s| !s.is_empty()).collect())
}

fn parse_owner_from_url(url: &str) -> Option<String> {
    let segments = remote_path_segments(url)?;
    let n = segments.len();
    (n >= 2).then(|| segments[n - 2].to_string())
}

fn parse_repo_name_from_url(url: &str) -> Option<String> {
    remote_path_segments(url)?.last().map(|s| s.to_string())
}

fn extract_owner(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    parse_owner_from_url(remote.url()?)
}

fn extract_remote_repo_name(repo: &Repository) -> Option<String> {
    let remote = repo.find_remote("origin").ok()?;
    parse_repo_name_from_url(remote.url()?)
}

pub fn detect_license(path: &Path) -> bool {
    ["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENSE-MIT", "LICENSE-APACHE", "COPYING", "COPYING.md"]
        .iter().any(|f| path.join(f).exists())
}

const ASSET_CATEGORIES: &[&str] = &[
    "Images", "Textures", "Audio", "Video", "3D", "Fonts",
    "PDF", "Docs", "Spreadsheets", "Slides", "eBook",
];

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

    let base = if assets_dominate { total } else { total - asset_total };
    if base == 0 { return None; }

    let threshold = ((base as f64 * 0.05) as usize).max(1);
    let mut langs: Vec<(&str, usize)> = counts.into_iter()
        .filter(|(k, c)| *c >= threshold && (assets_dominate || !ASSET_CATEGORIES.contains(k)))
        .collect();

    if langs.is_empty() { return None; }
    langs.sort_by_key(|(_, files)| std::cmp::Reverse(*files));
    Some(langs.iter().take(4)
        .map(|(lang, c)| format!("{} {}%", lang, c * 100 / base))
        .collect::<Vec<_>>()
        .join("  "))
}

fn ext_to_lang(ext: &str) -> Option<&'static str> {
    Some(match ext {
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

        "md" | "mdx" | "rst"                          => "Markdown",
        "txt" | "rtf" | "log"                         => "Text",
        "pdf"                                         => "PDF",
        "docx" | "doc" | "odt" | "pages"              => "Docs",
        "xlsx" | "xls" | "ods" | "numbers" | "csv" | "tsv" => "Spreadsheets",
        "pptx" | "ppt" | "odp" | "keynote"           => "Slides",
        "epub"                                        => "eBook",

        "png" | "jpg" | "jpeg" | "gif" | "webp"
        | "svg" | "ico" | "bmp" | "tiff" | "tif"
        | "tga" | "psd" | "xcf" | "raw" | "dng"
        | "cr2" | "nef" | "heic" | "heif" | "avif"
        | "apng" | "jxl"                              => "Images",

        "dds" | "ktx" | "ktx2" | "exr" | "hdr"
        | "basis" | "bc7" | "astc"                    => "Textures",

        "mp3" | "wav" | "ogg" | "flac" | "aac"
        | "m4a" | "wma" | "opus" | "mid" | "midi"
        | "aif" | "aiff" | "caf" | "ra" | "amr"      => "Audio",

        "mp4" | "mkv" | "avi" | "mov" | "webm"
        | "m4v" | "wmv" | "flv" | "ogv" | "mpg"
        | "mpeg" | "m2ts" | "mts" | "vob" | "rm"     => "Video",

        "obj" | "fbx" | "blend" | "gltf" | "glb"
        | "stl" | "dae" | "3ds" | "ply" | "abc"
        | "usd" | "usda" | "usdc" | "usdz"
        | "max" | "ma" | "mb" | "c4d" | "lwo"        => "3D",

        "ttf" | "otf" | "woff" | "woff2" | "eot"     => "Fonts",

        "parquet" | "arrow" | "hdf5" | "h5"
        | "npy" | "npz" | "pkl" | "mat" | "nc"       => "Data",

        _ => return None,
    })
}

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
                if best.as_ref().is_none_or(|(t, _)| ts > *t) {
                    best = Some((ts, name.to_string()));
                }
            }
        }
    }
    best.map(|(_, n)| n)
}

pub fn repo_summary(path: &Path) -> Result<RepoSummary> {
    let t_total = std::time::Instant::now();

    let repo = Repository::open(path)?;

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
    let branch_count = repo.branches(Some(git2::BranchType::Local))
        .map(|bs| bs.count()).unwrap_or(0);
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
        last_tag, visibility: None, branch_count, owner, remote_repo_name, has_license, lang, bookmarked: false,
        stars: None, open_issues: None, open_prs: None,
        stash_count,
        ci_status: None, dependabot_open: None, deploy_status: None,
    })
}

pub fn repo_detail(path: &Path) -> Result<RepoDetail> {
    let repo = Repository::open(path)?;
    let summary = repo_summary(path)?;

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

    for r in &mut repos {
        if let Some(label) = labels.get(&r.path) {
            r.name = label.clone();
        }
    }

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

pub async fn run(cfg: &AppConfig) -> Result<()> {
    let rx = scan_workspace_stream(cfg.workspace_root.clone());
    crate::hub::run_overview_tui(vec![], Some(rx), cfg).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testgit::{commit_at, commit_file, git, init_repo};

    fn touch(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    #[test]
    fn owner_and_repo_name_from_remote_urls() {
        let cases = [
            ("https://github.com/octo/hello.git", Some("octo"), Some("hello")),
            ("https://github.com/octo/hello/", Some("octo"), Some("hello")),
            ("https://gitlab.example.com/group/sub/proj", Some("sub"), Some("proj")),
            ("git@github.com:octo/hello.git", Some("octo"), Some("hello")),
            ("git@gitea.example.com:/team/tool", Some("team"), Some("tool")),
            ("https://git.example.com:8443/octo/hello.git", Some("octo"), Some("hello")),
            ("https://user@git.example.com/octo/hello", Some("octo"), Some("hello")),
            ("github.com:octo/hello.git", Some("octo"), Some("hello")),
            ("git@gitlab.example.com:group/sub/proj.git", Some("sub"), Some("proj")),
            ("ssh://git@github.com/octo/hello.git", Some("octo"), Some("hello")),
            ("ssh://git@git.example.com:2222/octo/hello.git", Some("octo"), Some("hello")),
            ("ssh://git.example.com/octo/hello", Some("octo"), Some("hello")),
            ("ssh://git.example.com:22/octo/hello.git/", Some("octo"), Some("hello")),
            ("ssh://git@gitlab.example.com/group/sub/proj.git", Some("sub"), Some("proj")),
            ("ssh://git@git.example.com:2222/hello.git", None, Some("hello")),
        ];
        for (url, owner, name) in cases {
            assert_eq!(parse_owner_from_url(url).as_deref(), owner, "{url}");
            assert_eq!(parse_repo_name_from_url(url).as_deref(), name, "{url}");
        }
        assert_eq!(parse_owner_from_url("/srv/git/hello.git"), None);
        assert_eq!(parse_repo_name_from_url("/srv/git/hello.git"), None);
    }

    #[test]
    fn license_files_are_recognized_by_name() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!detect_license(dir.path()));
        touch(dir.path(), "docs/LICENSE");
        assert!(!detect_license(dir.path()));
        for name in ["LICENSE", "LICENSE.md", "COPYING", "LICENSE-APACHE"] {
            let d = tempfile::tempdir().unwrap();
            touch(d.path(), name);
            assert!(detect_license(d.path()), "{name}");
        }
    }

    #[test]
    fn language_is_ranked_by_file_count() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_language(dir.path()), None);
        for f in ["src/a.rs", "src/b.rs", "README.md", "node_modules/x/y.js", "target/z.ts", "Makefile"] {
            touch(dir.path(), f);
        }
        assert_eq!(detect_language(dir.path()).as_deref(), Some("Rust 66%  Markdown 33%"));
    }

    #[test]
    fn assets_only_show_when_they_dominate() {
        let mixed = tempfile::tempdir().unwrap();
        for f in ["a.png", "b.png", "c.jpg", "a.rs", "b.rs", "c.rs", "d.rs", "README.md"] {
            touch(mixed.path(), f);
        }
        assert_eq!(detect_language(mixed.path()).as_deref(), Some("Rust 80%  Markdown 20%"));

        let art = tempfile::tempdir().unwrap();
        for i in 0..9 { touch(art.path(), &format!("img{i}.png")); }
        touch(art.path(), "tool.py");
        assert_eq!(detect_language(art.path()).as_deref(), Some("Images 90%  Python 10%"));
    }

    #[test]
    fn minor_languages_below_five_percent_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..40 { touch(dir.path(), &format!("m{i}.go")); }
        touch(dir.path(), "script.sh");
        assert_eq!(detect_language(dir.path()).as_deref(), Some("Go 97%"));
    }

    #[test]
    fn extension_mapping() {
        assert_eq!(ext_to_lang("tsx"), Some("TypeScript"));
        assert_eq!(ext_to_lang("hpp"), Some("C++"));
        assert_eq!(ext_to_lang("woff2"), Some("Fonts"));
        assert_eq!(ext_to_lang("lock"), None);
    }

    #[test]
    fn relative_age_formatting() {
        let now = Utc::now().timestamp();
        assert_eq!(format_age(now - 125), "2m ago");
        assert_eq!(format_age(now - 3 * 3600 - 5), "3h ago");
        assert_eq!(format_age(now - 86400 - 60), "yesterday");
        assert_eq!(format_age(now - 5 * 86400 - 60), "5d ago");
    }

    #[test]
    fn summary_of_an_empty_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(&dir.path().join("fresh"));
        let s = repo_summary(&repo).unwrap();
        assert_eq!(s.name, "fresh");
        assert_eq!(s.branch, "main");
        assert_eq!(s.last_commit_msg, "(no commits yet)");
        assert_eq!(s.last_commit_ts, 0);
        assert_eq!((s.ahead, s.behind), (0, 0));
        assert!(!s.is_dirty);
        assert_eq!(s.last_tag, None);
    }

    #[test]
    fn summary_reports_git_state() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(&dir.path().join("app"));
        let first = commit_at(&repo, "main.rs", "fn main() {}", "first", 1_700_000_000);
        git(&repo, &["tag", "v0.1.0"]);
        commit_at(&repo, "lib.rs", "pub fn x() {}", "second commit", 1_700_100_000);
        git(&repo, &["tag", "-a", "v0.2.0", "-m", "release"]);
        git(&repo, &["remote", "add", "origin", "git@github.com:octo/app-remote.git"]);
        git(&repo, &["update-ref", "refs/remotes/origin/main", &first]);
        git(&repo, &["branch", "feature"]);

        let s = repo_summary(&repo).unwrap();
        assert_eq!(s.branch, "main");
        assert_eq!(s.last_commit_msg, "second commit");
        assert_eq!(s.last_commit_ts, 1_700_100_000);
        assert_eq!(s.last_tag.as_deref(), Some("v0.2.0"));
        assert_eq!((s.ahead, s.behind), (1, 0));
        assert_eq!(s.branch_count, 2);
        assert_eq!(s.owner.as_deref(), Some("octo"));
        assert_eq!(s.remote_repo_name.as_deref(), Some("app-remote"));
        assert_eq!(s.lang.as_deref(), Some("Rust 100%"));
        assert!(!s.is_dirty);
        assert!(!s.has_license);
        assert_eq!(s.stash_count, 0);

        std::fs::write(repo.join("lib.rs"), "pub fn y() {}").unwrap();
        git(&repo, &["stash", "-q"]);
        std::fs::write(repo.join("new.txt"), "untracked").unwrap();
        touch(&repo, "LICENSE");
        let s = repo_summary(&repo).unwrap();
        assert!(s.is_dirty);
        assert!(s.has_license);
        assert_eq!(s.stash_count, 1);

        git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&repo, &["reset", "-q", "--hard", &first]);
        let s = repo_summary(&repo).unwrap();
        assert_eq!((s.ahead, s.behind), (0, 1));
    }

    #[test]
    fn workspace_scan_finds_nested_repos_and_sorts_active_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let old = init_repo(&root.join("old"));
        commit_at(&old, "a.txt", "a", "old", 1_600_000_000);
        let recent = init_repo(&root.join("recent"));
        commit_at(&recent, "a.txt", "a", "recent", 1_700_000_000);
        let nested = init_repo(&root.join("clients/acme/site"));
        commit_at(&nested, "a.txt", "a", "nested", 1_650_000_000);
        std::fs::write(nested.join("wip.txt"), "dirty").unwrap();
        let hidden = init_repo(&root.join("node_modules/dep"));
        commit_file(&hidden, "a.txt", "a", "dep");
        std::fs::create_dir_all(root.join("notes")).unwrap();

        let names: Vec<String> = scan_workspace(root, &[]).unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["clients/acme/site", "recent", "old"]);
    }

    #[test]
    fn recent_commits_are_newest_first_and_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(&dir.path().join("r"));
        for i in 0..4 {
            commit_at(&repo, "f.txt", &i.to_string(), &format!("commit {i}"), 1_700_000_000 + i * 60);
        }
        let msgs: Vec<String> = repo_recent_commits(&repo, 3).into_iter().map(|(_, m)| m).collect();
        assert_eq!(msgs, ["commit 3", "commit 2", "commit 1"]);
        assert!(repo_recent_commits(dir.path(), 3).is_empty());
        assert_eq!(count_tracked_files(&repo), 1);
    }
}

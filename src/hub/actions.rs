//! Hub actions that fall outside the TUI's draw/key-handling split: the
//! blocking sync flow (which drops out of the TUI to plain stdout while it
//! runs), API refresh against configured provider accounts, the TODO/FIXME
//! scanner, clipboard/URL re-exports, and the `~/.vulp/*.toml` persistence
//! for bookmarks, pins, and repo-creation templates.

use anyhow::Result;
use crate::status::RepoSummary;
use crate::todos::{self, TodoSource};
use std::collections::HashSet;
use std::path::PathBuf;
use super::state::HubState;

// ── sync flow ─────────────────────────────────────────────────────────────────

/// Runs `action` ("pull", "commit_push", or anything else meaning "push")
/// against `targets` (indices into `repos`), printing progress straight to
/// stdout — the caller suspends the TUI for the duration so this output is
/// visible, then waits for Enter before restoring the screen.
pub async fn run_sync_flow(repos: &[RepoSummary], targets: &[usize], action: &str, commit_msg: &str) -> Result<()> {
    let sel: Vec<&RepoSummary> = targets.iter().filter_map(|&i| repos.get(i)).collect();
    if sel.is_empty() { return Ok(()); }

    println!("\n  Syncing {} repo(s)  [{}]:", sel.len(), action);
    for r in &sel { println!("    · {}", r.name); }
    println!();

    let run_logged = |path: &std::path::Path, args: &[&str], kind: crate::activity::ActivityKind, repo: &str| {
        let ok = crate::util::git::git_run_ok(path, args);
        if !ok {
            eprintln!("  git {} → failed", args.join(" "));
        } else {
            crate::activity::log(&crate::activity::ActivityEntry {
                ts: chrono::Utc::now().timestamp(), kind, repo: repo.to_string(),
                detail: format!("vulp sync ({})", args.join(" ")),
            });
        }
    };
    for r in &sel {
        println!("\n  ── {} ──", r.name);
        match action {
            "pull" => run_logged(&r.path, &["pull"], crate::activity::ActivityKind::Pull, &r.name),
            "commit_push" => {
                git_run(&r.path, &["add", "-A"]);
                git_run(&r.path, &["commit", "-m", commit_msg]);
                run_logged(&r.path, &["push"], crate::activity::ActivityKind::Push, &r.name);
            }
            _ => run_logged(&r.path, &["push"], crate::activity::ActivityKind::Push, &r.name),
        }
    }
    println!("\n  ✓ Done. Press Enter…");
    let _ = std::io::stdin().lines().next();
    Ok(())
}

pub use crate::util::git::git_run;

/// Re-scans the workspace from disk, reapplies bookmark state, and rebuilds
/// the display rows — used after operations that change repo state on disk
/// outside of `HubState` (e.g. after a sync or a new repo creation).
pub fn refresh_repos(state: &mut HubState) {
    if let Ok(fresh) = crate::status::scan_workspace(&state.workspace_root, &[]) {
        state.repos = fresh;
        for r in &mut state.repos {
            r.bookmarked = state.bookmarks.contains(&r.path);
        }
        state.focused_cache.clear();
        // `selected` holds indices into the old repo order — after a rescan
        // they'd silently point at different repos (batch sync/release would
        // hit wrong targets), so drop the selection entirely.
        state.selected.clear();
        state.rebuild_display();
    }
    // Actions that trigger a rescan (sync, release, repo creation) also write
    // to the activity log — reload it so the Overview vulp-log tile stays live.
    state.vulp_log = {
        let mut log = crate::activity::load_logged();
        log.sort_by(|a, b| b.ts.cmp(&a.ts));
        log.truncate(50);
        log
    };
}

// ── TODO scanner ──────────────────────────────────────────────────────────────

/// Scans every repo for `TODO`/`FIXME`/`HACK` markers (via
/// [`crate::legacy::refactor`]) and imports each unseen one as a new todo,
/// skipping any whose generated title already matches an existing todo so
/// re-running the scan doesn't create duplicates. Returns the count added.
pub fn scan_todos_from_repos(state: &mut HubState) -> usize {
    let mut added = 0usize;
    let existing_titles: HashSet<String> =
        state.todos.iter().map(|t| t.title.clone()).collect();

    for r in &state.repos {
        for pattern in &["TODO", "FIXME", "HACK"] {
            let mut results = crate::legacy::refactor::find_in_doc_files(pattern, &r.path, &r.name);
            results.extend(crate::legacy::refactor::find_in_source_files(pattern, &r.path, &r.name));
            for fr in results.iter().take(50) {
                let short_file = std::path::Path::new(&fr.file)
                    .file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| fr.file.clone());
                let title = format!("{}:{}  {}", short_file, fr.line,
                    fr.text.trim().chars().take(60).collect::<String>());
                if existing_titles.contains(&title) { continue; }
                let id = todos::next_id(&state.todos);
                let mut t = todos::Todo::new_scan(id, title.clone(), r.name.clone());
                t.source = TodoSource::CodeScan;
                state.todos.push(t);
                added += 1;
                crate::activity::log(&crate::activity::ActivityEntry {
                    ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::Todo,
                    repo: r.name.clone(), detail: title,
                });
            }
        }
    }
    if added > 0 { let _ = todos::save(&state.todos); }
    added
}

// ── clipboard ─────────────────────────────────────────────────────────────────

pub use crate::util::clipboard::copy_to_clipboard;

/// Reads the `origin` remote URL straight from the repo's git config.
pub fn get_remote_url(path: &PathBuf) -> Option<String> {
    let repo   = git2::Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    Some(remote.url()?.to_string())
}

pub use crate::util::url::to_web_url;

// ── bookmarks persistence ─────────────────────────────────────────────────────

fn bookmarks_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("bookmarks.toml")
}

/// Loads bookmarked repo paths from `~/.vulp/bookmarks.toml`, or an empty
/// set if the file doesn't exist or fails to parse.
pub fn load_bookmarks() -> HashSet<PathBuf> {
    let p = bookmarks_path();
    if !p.exists() { return HashSet::new(); }
    #[derive(serde::Deserialize, Default)]
    struct Bk { #[serde(default)] paths: Vec<String> }
    toml::from_str::<Bk>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|b| b.paths.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// Writes `bk` to `~/.vulp/bookmarks.toml`, creating the directory if needed.
pub fn save_bookmarks(bk: &HashSet<PathBuf>) {
    #[derive(serde::Serialize)]
    struct Bk { paths: Vec<String> }
    let p = bookmarks_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let data = Bk { paths: bk.iter().map(|p| p.to_string_lossy().to_string()).collect() };
    if let Ok(s) = toml::to_string_pretty(&data) { let _ = std::fs::write(p, s); }
}

// ── templates persistence ─────────────────────────────────────────────────────

fn templates_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("templates.toml")
}

/// On-disk shape of `~/.vulp/templates.toml`.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct TemplatesFile {
    #[serde(default)]
    templates: Vec<super::state::RepoTemplate>,
}

/// Loads repo-creation templates from `~/.vulp/templates.toml`. On first run
/// (file missing or empty), seeds and saves the built-in stack templates
/// from [`default_templates`] so the picker is never empty.
pub fn load_templates() -> Vec<super::state::RepoTemplate> {
    let p = templates_path();
    if p.exists() {
        let loaded = toml::from_str::<TemplatesFile>(&std::fs::read_to_string(&p).unwrap_or_default())
            .map(|f| f.templates)
            .unwrap_or_default();
        if !loaded.is_empty() { return loaded; }
    }
    // First run — seed built-in stacks
    let defaults = default_templates();
    save_templates(&defaults);
    defaults
}

/// Built-in repo-creation templates (Rust, Node.js, React, Python, Go,
/// Laravel) used to seed `templates.toml` on first run.
fn default_templates() -> Vec<super::state::RepoTemplate> {
    use super::state::RepoTemplate;
    vec![
        RepoTemplate {
            name: "Rust".into(),
            description: "Rust project with Cargo".into(),
            commands: vec![
                "cargo init".into(),
                "echo '.env' >> .gitignore".into(),
                "printf '# TODO\\n\\n- [ ] Initial setup\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
        RepoTemplate {
            name: "Node.js".into(),
            description: "Node.js project with npm".into(),
            commands: vec![
                "npm init -y".into(),
                "printf 'node_modules/\\n.env\\ndist/\\nbuild/\\n' > .gitignore".into(),
                "printf '# TODO\\n\\n- [ ] Initial setup\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
        RepoTemplate {
            name: "React".into(),
            description: "React + Vite app".into(),
            commands: vec![
                "npm init -y".into(),
                "npm install react react-dom".into(),
                "npm install -D vite @vitejs/plugin-react".into(),
                "printf 'node_modules/\\n.env\\ndist/\\n' > .gitignore".into(),
                "mkdir -p src public".into(),
                "printf '# TODO\\n\\n- [ ] Build React components\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
        RepoTemplate {
            name: "Python".into(),
            description: "Python project with venv".into(),
            commands: vec![
                "python3 -m venv .venv".into(),
                "printf '.venv/\\n__pycache__/\\n*.pyc\\n.env\\ndist/\\nbuild/\\n*.egg-info/\\n' > .gitignore".into(),
                "touch main.py requirements.txt".into(),
                "printf '# TODO\\n\\n- [ ] Initial setup\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
        RepoTemplate {
            name: "Go".into(),
            description: "Go module".into(),
            commands: vec![
                "go mod init $(basename $PWD)".into(),
                "touch main.go".into(),
                "printf '# TODO\\n\\n- [ ] Initial setup\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
        RepoTemplate {
            name: "Laravel".into(),
            description: "Laravel PHP project (requires Composer)".into(),
            commands: vec![
                "composer create-project laravel/laravel .".into(),
                "printf '# TODO\\n\\n- [ ] Configure application\\n' > TODO.md".into(),
                "printf '# Changelog\\n\\n## [Unreleased]\\n' > CHANGELOG.md".into(),
            ],
        },
    ]
}

/// Writes `templates` to `~/.vulp/templates.toml`, creating the directory if
/// needed.
pub fn save_templates(templates: &[super::state::RepoTemplate]) {
    let p = templates_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let file = TemplatesFile { templates: templates.to_vec() };
    if let Ok(s) = toml::to_string_pretty(&file) { let _ = std::fs::write(p, s); }
}

// ── pins persistence ──────────────────────────────────────────────────────────

fn pins_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("pins.toml")
}

/// Loads pinned repo paths from `~/.vulp/pins.toml`, or an empty set if the
/// file doesn't exist or fails to parse.
pub fn load_pins() -> HashSet<PathBuf> {
    let p = pins_path();
    if !p.exists() { return HashSet::new(); }
    #[derive(serde::Deserialize, Default)]
    struct Pins { #[serde(default)] paths: Vec<String> }
    toml::from_str::<Pins>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|b| b.paths.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// Writes `pins` to `~/.vulp/pins.toml`, creating the directory if needed.
pub fn save_pins(pins: &HashSet<PathBuf>) {
    #[derive(serde::Serialize)]
    struct Pins { paths: Vec<String> }
    let p = pins_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let data = Pins { paths: pins.iter().map(|p| p.to_string_lossy().to_string()).collect() };
    if let Ok(s) = toml::to_string_pretty(&data) { let _ = std::fs::write(p, s); }
}

use anyhow::Result;
use crate::config::AppConfig;
use crate::status::RepoSummary;
use crate::todos::{self, TodoSource};
use std::collections::HashSet;
use std::path::PathBuf;
use super::state::HubState;

// ── sync flow ─────────────────────────────────────────────────────────────────

pub async fn run_sync_flow(repos: &[RepoSummary], targets: &[usize]) -> Result<()> {
    let sel: Vec<&RepoSummary> = targets.iter().filter_map(|&i| repos.get(i)).collect();
    if sel.is_empty() { return Ok(()); }
    println!("\n  Syncing {} repo(s):", sel.len());
    for r in &sel { println!("    · {}", r.name); }
    println!();

    let mode = match inquire::Select::new("Action:", vec!["pull", "commit + push", "push only"])
        .with_page_size(3).prompt_skippable()? { Some(m) => m, None => return Ok(()) };

    let global_msg: Option<String> = if mode == "commit + push" {
        let m = inquire::Select::new("Commit message:", vec!["same for all", "per repo"]).with_page_size(2).prompt()?;
        if m == "same for all" { Some(inquire::Text::new("Message:").with_default("chore: update").prompt()?) } else { None }
    } else { None };

    for r in &sel {
        println!("\n  ── {} ──", r.name);
        match mode {
            "pull" => git_run(&r.path, &["pull"]),
            "commit + push" => {
                let msg = match &global_msg { Some(m) => m.clone(), None => inquire::Text::new(&format!("Message for '{}':", r.name)).with_default("chore: update").prompt()? };
                git_run(&r.path, &["add", "-A"]);
                git_run(&r.path, &["commit", "-m", &msg]);
                git_run(&r.path, &["push"]);
            }
            _ => git_run(&r.path, &["push"]),
        }
    }
    println!("\n  ✓ Done. Press Enter...");
    let _ = std::io::stdin().lines().next();
    Ok(())
}

pub use crate::util::git::git_run;

// ── API refresh ───────────────────────────────────────────────────────────────

pub async fn exec_api_refresh(state: &mut HubState, cfg: &AppConfig) {
    use crate::auth;
    use crate::providers;

    let accounts = match auth::load_accounts(cfg) {
        Ok(a) => a,
        Err(_) => { state.status_msg = Some("No accounts configured — fxg auth add".into()); return; }
    };
    if accounts.is_empty() {
        state.status_msg = Some("No accounts configured — fxg auth add".into());
        return;
    }

    let mut cache = crate::cache::load();
    let mut fetched_meta = 0usize;
    let mut fetched_prs  = 0usize;
    let mut fetched_iss  = 0usize;

    for account in &accounts {
        let provider = match providers::make_provider(account) {
            Ok(p)  => p,
            Err(e) => {
                state.status_msg = Some(format!("Auth error ({}): {e}", account.provider));
                continue;
            }
        };

        for r in &mut state.repos {
            let is_mine = r.owner.as_deref() == Some(account.username.as_str());
            if !is_mine { continue; }

            if let Ok(meta) = provider.repo_meta(&r.name).await {
                let cache_meta = crate::cache::RepoMeta {
                    stars:       Some(meta.stars),
                    open_issues: Some(meta.open_issues),
                    open_prs:    Some(meta.open_prs),
                    visibility:  r.visibility.clone(),
                    ci_ok:       None,
                    fetched_at:  chrono::Utc::now().timestamp(),
                };
                r.stars       = Some(meta.stars);
                r.open_issues = Some(meta.open_issues);
                r.open_prs    = Some(meta.open_prs);
                cache.set_meta(&r.name, cache_meta);
                fetched_meta += 1;
            }

            if let Ok(prs) = provider.list_pull_requests(&r.name).await {
                fetched_prs += prs.len();
                cache.set_prs(&r.name, prs);
            }

            if let Ok(issues) = provider.list_issues(&r.name).await {
                fetched_iss += issues.len();
                cache.set_issues(&r.name, issues);
            }
        }
    }

    let _ = crate::cache::save(&cache);
    state.prs    = cache.all_prs().into_iter().cloned().collect();
    state.issues = cache.all_issues().into_iter().cloned().collect();

    state.status_msg = Some(format!(
        "Refreshed {} repos: {} PRs, {} issues",
        fetched_meta, fetched_prs, fetched_iss
    ));
    state.rebuild_display();
}

pub fn refresh_repos(state: &mut HubState) {
    if let Ok(fresh) = crate::status::scan_workspace(&state.workspace_root, &[]) {
        state.repos = fresh;
        for r in &mut state.repos {
            r.bookmarked = state.bookmarks.contains(&r.path);
        }
        state.focused_cache.clear();
        state.rebuild_display();
    }
}

// ── TODO scanner ──────────────────────────────────────────────────────────────

pub fn scan_todos_from_repos(state: &mut HubState) -> usize {
    let mut added = 0usize;
    let existing_titles: HashSet<String> =
        state.todos.iter().map(|t| t.title.clone()).collect();

    for r in &state.repos {
        for pattern in &["TODO", "FIXME", "HACK"] {
            let results = crate::legacy::refactor::find_in_dir(pattern, &r.path, &r.name);
            for fr in results.iter().take(50) {
                let short_file = std::path::Path::new(&fr.file)
                    .file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| fr.file.clone());
                let title = format!("{}:{}  {}", short_file, fr.line,
                    fr.text.trim().chars().take(60).collect::<String>());
                if existing_titles.contains(&title) { continue; }
                let id = todos::next_id(&state.todos);
                let mut t = todos::Todo::new_scan(id, title, r.name.clone());
                t.source = TodoSource::CodeScan;
                state.todos.push(t);
                added += 1;
            }
        }
    }
    if added > 0 { let _ = todos::save(&state.todos); }
    added
}

// ── clipboard ─────────────────────────────────────────────────────────────────

pub use crate::util::clipboard::copy_to_clipboard;

pub fn get_remote_url(path: &PathBuf) -> Option<String> {
    let repo   = git2::Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    Some(remote.url()?.to_string())
}

pub use crate::util::url::to_web_url;

// ── bookmarks persistence ─────────────────────────────────────────────────────

fn bookmarks_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".fxg").join("bookmarks.toml")
}

pub fn load_bookmarks() -> HashSet<PathBuf> {
    let p = bookmarks_path();
    if !p.exists() { return HashSet::new(); }
    #[derive(serde::Deserialize, Default)]
    struct Bk { #[serde(default)] paths: Vec<String> }
    toml::from_str::<Bk>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|b| b.paths.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

pub fn save_bookmarks(bk: &HashSet<PathBuf>) {
    #[derive(serde::Serialize)]
    struct Bk { paths: Vec<String> }
    let p = bookmarks_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let data = Bk { paths: bk.iter().map(|p| p.to_string_lossy().to_string()).collect() };
    if let Ok(s) = toml::to_string_pretty(&data) { let _ = std::fs::write(p, s); }
}

// ── pins persistence ──────────────────────────────────────────────────────────

fn pins_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".fxg").join("pins.toml")
}

pub fn load_pins() -> HashSet<PathBuf> {
    let p = pins_path();
    if !p.exists() { return HashSet::new(); }
    #[derive(serde::Deserialize, Default)]
    struct Pins { #[serde(default)] paths: Vec<String> }
    toml::from_str::<Pins>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|b| b.paths.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

pub fn save_pins(pins: &HashSet<PathBuf>) {
    #[derive(serde::Serialize)]
    struct Pins { paths: Vec<String> }
    let p = pins_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let data = Pins { paths: pins.iter().map(|p| p.to_string_lossy().to_string()).collect() };
    if let Ok(s) = toml::to_string_pretty(&data) { let _ = std::fs::write(p, s); }
}

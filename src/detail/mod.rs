mod git;
mod render;

use crate::config::AppConfig;
use crate::status::{count_tracked_files, repo_detail, repo_recent_commits, RepoSummary};
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use git2::Repository;
use git::{
    build_history, copy_to_clipboard, count_contributors, count_local_branches,
    count_stashes, count_tags, git_run, git_run_ok, list_all_files, to_web_url,
};
use render::draw;

// ── history entry (commits + release tags interleaved) ────────────────────────

#[derive(Clone)]
pub enum HistEntry {
    Commit { sha: String, age: String, ts: i64, author: String, msg: String },
    Tag    { name: String, age: String, ts: i64, msg: String },
}

impl HistEntry {
    pub fn ts(&self) -> i64 { match self { Self::Commit { ts, .. } | Self::Tag { ts, .. } => *ts } }
}

// ── tabs ──────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum Tab { Overview, History, Files }

// ── state ─────────────────────────────────────────────────────────────────────

pub struct St {
    pub summary:        RepoSummary,
    pub tab:            Tab,
    pub files:          Vec<(char, String)>,
    pub file_count:     usize,
    pub contributors:   usize,
    pub total_tags:     usize,
    pub recent_commits: Vec<(String, String)>,
    pub stash_count:    usize,
    pub branch_count:   usize,
    pub history:        Vec<HistEntry>,
    pub hist_off:       usize,
    pub file_off:       usize,
    pub show_all_files: bool,
    pub all_files:      Vec<String>,
    pub ov_scroll:      usize,
    pub quit:           bool,
    pub run_action:     Option<usize>,
    pub status_msg:     Option<String>,
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(summary: RepoSummary, cfg: &AppConfig) -> Result<()> {
    let detail = repo_detail(&summary.path)?;
    let path   = summary.path.clone();

    let mut st = St {
        files:          detail.modified_files,
        file_count:     count_tracked_files(&path),
        contributors:   count_contributors(&path),
        total_tags:     count_tags(&path),
        recent_commits: repo_recent_commits(&path, 8),
        stash_count:    count_stashes(&path),
        branch_count:   count_local_branches(&path),
        history:        build_history(&path, 60),
        hist_off:       0,
        file_off:       0,
        show_all_files: false,
        all_files:      list_all_files(&path),
        ov_scroll:      0,
        quit:           false,
        run_action:     None,
        status_msg:     None,
        tab:            Tab::Overview,
        summary,
    };

    loop {
        let action = tui_phase(&mut st)?;
        if st.quit { break; }
        if let Some(a) = action {
            exec_action(&mut st, a).await?;
        }
    }
    let _ = cfg;
    Ok(())
}

fn tui_phase(st: &mut St) -> Result<Option<usize>> {
    let mut terminal = crate::tui::setup()?;
    let result = (|| -> Result<Option<usize>> {
        loop {
            terminal.draw(|f| draw(f, st))?;
            let event = crate::tui::next_event(250)?;
            let key = match event { Some(Event::Key(k)) => k, _ => continue };
            handle_key(key.code, st);
            if st.quit { return Ok(None); }
            if let Some(a) = st.run_action.take() { return Ok(Some(a)); }
        }
    })();
    crate::tui::teardown(terminal)?;
    result
}

fn handle_key(code: KeyCode, st: &mut St) {
    st.status_msg = None;
    match code {
        KeyCode::Char('q') | KeyCode::Esc => st.quit = true,

        KeyCode::Tab => {
            st.tab = match st.tab { Tab::Overview => Tab::History, Tab::History => Tab::Files, Tab::Files => Tab::Overview };
        }
        KeyCode::Char('1') => st.tab = Tab::Overview,
        KeyCode::Char('2') => st.tab = Tab::History,
        KeyCode::Char('3') => st.tab = Tab::Files,

        KeyCode::Down | KeyCode::Char('j') => match st.tab {
            Tab::History  => st.hist_off += 1,
            Tab::Files    => st.file_off  += 1,
            Tab::Overview => st.ov_scroll += 1,
        },
        KeyCode::Up | KeyCode::Char('k') => match st.tab {
            Tab::History  => st.hist_off  = st.hist_off.saturating_sub(1),
            Tab::Files    => st.file_off  = st.file_off.saturating_sub(1),
            Tab::Overview => st.ov_scroll = st.ov_scroll.saturating_sub(1),
        },
        KeyCode::PageDown => match st.tab {
            Tab::History  => st.hist_off  = (st.hist_off + 10).min(st.history.len().saturating_sub(1)),
            Tab::Files    => st.file_off  += 10,
            Tab::Overview => st.ov_scroll += 5,
        },
        KeyCode::PageUp => match st.tab {
            Tab::History  => st.hist_off  = st.hist_off.saturating_sub(10),
            Tab::Files    => st.file_off  = st.file_off.saturating_sub(10),
            Tab::Overview => st.ov_scroll = st.ov_scroll.saturating_sub(5),
        },

        KeyCode::Char('s') => { st.run_action = Some(0); }
        KeyCode::Char('r') => { st.run_action = Some(1); }
        KeyCode::Char('W') => {
            if let Ok(repo) = Repository::open(&st.summary.path) {
                if let Ok(remote) = repo.find_remote("origin") {
                    if let Some(url) = remote.url() {
                        let web = to_web_url(url);
                        let _ = std::process::Command::new("xdg-open").arg(&web).spawn();
                        st.status_msg = Some("Opening in browser...".into());
                    }
                }
            }
        }
        KeyCode::Char('o') => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let _ = std::process::Command::new(&editor).arg(&st.summary.path).spawn();
            st.status_msg = Some(format!("Opened in {}", editor));
        }
        KeyCode::Char('y') => {
            let path = st.summary.path.to_string_lossy().to_string();
            if copy_to_clipboard(&path) { st.status_msg = Some("Path copied!".into()); }
        }
        KeyCode::Char('u') => {
            if let Ok(repo) = Repository::open(&st.summary.path) {
                if let Ok(remote) = repo.find_remote("origin") {
                    if let Some(url) = remote.url() {
                        if copy_to_clipboard(url) { st.status_msg = Some("URL copied!".into()); }
                    }
                }
            }
        }
        KeyCode::Char('g') => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let gi = st.summary.path.join(".gitignore");
            let _ = std::process::Command::new(&editor).arg(&gi).spawn();
            st.status_msg = Some("Opening .gitignore...".into());
        }
        KeyCode::Char('c') => { st.run_action = Some(10); }
        KeyCode::Char('S') => {
            let ok = git_run_ok(&st.summary.path, &["stash"]);
            st.status_msg = Some(if ok { "Stashed.".into() } else { "Stash failed.".into() });
            if ok { st.stash_count += 1; }
        }
        KeyCode::Char('A') => {
            let ok = git_run_ok(&st.summary.path, &["stash", "pop"]);
            st.status_msg = Some(if ok { "Stash applied.".into() } else { "No stash / error.".into() });
            if ok { st.stash_count = st.stash_count.saturating_sub(1); }
        }
        KeyCode::Char('a') if st.tab == Tab::Files => {
            st.show_all_files = !st.show_all_files;
            st.file_off = 0;
        }

        _ => {}
    }
}

async fn exec_action(st: &mut St, idx: usize) -> Result<()> {
    let path = &st.summary.path;
    match idx {
        0 => {
            println!("\n── Sync: {} ──\n", st.summary.name);
            let mode = inquire::Select::new("Action:", vec!["pull", "commit + push", "push only"])
                .with_page_size(3).prompt_skippable()?;
            if let Some(m) = mode {
                match m {
                    "pull" => git_run(path, &["pull"]),
                    "commit + push" => {
                        let msg = inquire::Text::new("Commit message:").with_default("chore: update").prompt()?;
                        git_run(path, &["add", "-A"]);
                        git_run(path, &["commit", "-m", &msg]);
                        git_run(path, &["push"]);
                    }
                    _ => git_run(path, &["push"]),
                }
            }
        }
        1 => {
            println!("\n── Release: {} ──\n", st.summary.name);
            let tag = inquire::Text::new("Tag name (e.g. v1.0.0):").prompt()?;
            let msg = inquire::Text::new("Tag message:").with_default(&tag).prompt()?;
            git_run(path, &["tag", "-a", &tag, "-m", &msg]);
            if inquire::Confirm::new("Push tag?").with_default(true).prompt()? {
                git_run(path, &["push", "origin", &tag]);
            }
            st.history = build_history(path, 60);
            st.total_tags += 1;
        }
        10 => {
            println!("\n── New branch in: {} ──\n", st.summary.name);
            let name = inquire::Text::new("Branch name:").prompt()?;
            if !name.trim().is_empty() {
                git_run(path, &["checkout", "-b", &name]);
                st.branch_count += 1;
                st.summary.branch = name;
            }
        }
        _ => {}
    }
    println!("\n  Press Enter to return...");
    let _ = std::io::stdin().lines().next();
    Ok(())
}

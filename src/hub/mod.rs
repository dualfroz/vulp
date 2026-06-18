mod actions;
mod keys;
mod modals;
mod project_detail;
mod render;
mod right_panel;
mod state;
mod tabs;

use actions::{exec_api_refresh, load_bookmarks, load_pins, load_templates, refresh_repos, run_sync_flow};
use keys::tui_phase;
use state::{HubState, Modal, PhaseResult, SortMode, Tab};

use anyhow::Result;
use crate::config::AppConfig;
use crate::projects;
use crate::providers::{IssueDetail, PrDetail};
use crate::status::RepoSummary;
use crossterm::event::KeyCode;
use ratatui::text::Line;
use std::collections::{HashMap, HashSet};

pub async fn run_overview_tui(
    mut repos: Vec<RepoSummary>,
    scan_rx: Option<std::sync::mpsc::Receiver<RepoSummary>>,
    cfg: &AppConfig,
) -> Result<()> {
    let workspace_root = cfg.workspace_root.clone();
    let projects       = projects::load();
    let project_map    = projects::index_by_repo(&projects);
    let todos          = crate::todos::load_manual();
    let bookmarks      = load_bookmarks();
    let pins           = load_pins();

    let meta_cache = crate::cache::load();
    for r in &mut repos {
        if let Some(entry) = meta_cache.get(&r.name) {
            let m = &entry.meta;
            r.stars       = m.stars;
            r.open_issues = m.open_issues;
            r.open_prs    = m.open_prs;
            if r.visibility.is_none() { r.visibility = m.visibility.clone(); }
        }
    }
    let cached_prs:    Vec<PrDetail>    = meta_cache.all_prs().into_iter().cloned().collect();
    let cached_issues: Vec<IssueDetail> = meta_cache.all_issues().into_iter().cloned().collect();

    let debug_mode = crate::debug::is_enabled();
    let debug_log  = crate::debug::take();
    let accounts   = crate::auth::load_accounts_path(&cfg.data_dir).unwrap_or_default();
    let templates  = load_templates();

    let mut state = HubState {
        repos, projects, project_map, todos,
        prs:    cached_prs,
        issues: cached_issues,
        tab: Tab::Overview,
        display: Vec::new(), repo_pos: Vec::new(),
        cursor: 0, offset: 0, visible_rows: 0,
        selected: HashSet::new(),
        filter_dirty: false, filter_behind: false, filter_ahead: false,
        filter_archived: false,
        filter_text: String::new(), filtering: false,
        sort_mode: SortMode::Default,
        bookmarks,
        pins,
        status_msg: None,
        pending_clone: None,
        modal: Modal::None, confirming: None, exec_action: false,
        open_detail: false, quit: false,
        workspace_root,
        focused_cache: HashMap::new(),
        editor: cfg.editor.clone(),
        clipboard_cmd: cfg.clipboard_cmd.clone(),
        show_code_todos: false,
        debug_mode,
        debug_log,
        scan_rx,
        meta_cache,
        diff_rx: None,
        global_stats: None,
        global_stats_rx: None,
        global_stats_period: crate::detail::StatsPeriod::Day30,
        hub_settings_cur: 0,
        accounts,
        templates,
        pending_sync_action: None,
        pending_sync_msg:    None,
        pending_phase:       None,
        pull_all_rx:         None,
    };
    for r in &mut state.repos {
        r.bookmarked = state.bookmarks.contains(&r.path);
    }
    state.rebuild_display();

    loop {
        let (phase_result, handed_term) = tui_phase(&mut state)?;

        if state.quit { break; }

        match phase_result {
            PhaseResult::OpenDetail => {
                state.open_detail = false;
                if let Some(ri) = state.cur_repo_idx() {
                    let _ = crate::status::detail::run(state.repos[ri].clone(), cfg, handed_term).await;
                } else if let Some(pi) = state.cur_proj_idx() {
                    let repo_indices = state.project_repos(pi);
                    let proj_repos: Vec<RepoSummary> =
                        repo_indices.iter().filter_map(|&ri| state.repos.get(ri)).cloned().collect();
                    project_detail::run(state.projects[pi].clone(), proj_repos, cfg, handed_term).await?;
                }
            }
            PhaseResult::ExecSync => {
                state.exec_action = false;
                state.confirming  = None;
                let targets = state.action_targets();
                let action  = state.pending_sync_action.take().unwrap_or_else(|| "pull".into());
                let msg     = state.pending_sync_msg.take().unwrap_or_default();
                run_sync_flow(&state.repos, &targets, &action, &msg).await?;
                state.selected.clear();
                refresh_repos(&mut state);
            }
            PhaseResult::ExecRelease => {
                state.exec_action = false;
                state.confirming  = None;
                println!("\n  Use [Enter] → repo detail → Actions → Release");
                println!("  Press Enter...");
                let _ = std::io::stdin().lines().next();
            }
            PhaseResult::ExecClone(url) => {
                println!("\n  Cloning: {}", url);
                println!("  Into: {}\n", state.workspace_root.display());
                let _ = std::process::Command::new("git")
                    .arg("clone").arg(&url)
                    .current_dir(&state.workspace_root)
                    .status();
                println!("\n  Done. Press Enter...");
                let _ = std::io::stdin().lines().next();
                refresh_repos(&mut state);
            }
            PhaseResult::ExecRefresh => {
                exec_api_refresh(&mut state, cfg).await;
            }
            PhaseResult::ExecTemplateInit { path, commands } => {
                println!("\n── Running template commands ──\n");
                for cmd in &commands {
                    println!("  $ {}", cmd);
                    let _ = std::process::Command::new("sh")
                        .args(["-c", cmd.as_str()])
                        .current_dir(&path)
                        .status();
                }
                println!("\n  ✓ Done. Press Enter...");
                let _ = std::io::stdin().lines().next();
                refresh_repos(&mut state);
            }
            PhaseResult::ExecOpenEditor(path) => {
                let editor = state.editor.clone();
                let _ = std::process::Command::new(&editor).arg(&path).status();
            }
            PhaseResult::None => {}
        }
    }
    Ok(())
}


pub fn run_detail_tui(detail: crate::status::RepoDetail) -> Result<()> {
    use crossterm::event::Event;
    use ratatui::widgets::{Block, Borders, Paragraph};
    let mut quit = false;
    crate::tui::run_loop(|terminal| {
        terminal.draw(|f| {
            let area = f.area();
            let blk  = Block::default().title(format!(" {} · {} ", detail.summary.name, detail.summary.branch)).borders(Borders::ALL);
            let inner = blk.inner(area);
            f.render_widget(blk, area);
            let lines = vec![
                Line::from(format!("  Branch: {}  +{} ahead  -{} behind  dirty: {}", detail.summary.branch, detail.summary.ahead, detail.summary.behind, detail.summary.is_dirty)),
                Line::from(format!("  Path: {}", detail.summary.path.display())),
                Line::from(""),
                Line::from("  Modified files:"),
            ];
            f.render_widget(Paragraph::new(lines), inner);
        })?;
        if let Some(Event::Key(key)) = crate::tui::next_event(250)? {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) { quit = true; }
        }
        Ok(quit)
    })
}

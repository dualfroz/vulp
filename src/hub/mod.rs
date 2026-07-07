// pub: the CLI (`cli_cmds`) reuses the bookmark/pin persistence and
// remote-url helpers instead of duplicating them.
pub mod actions;
mod keys;
mod keys_dispatch;
mod keys_poll;
mod modals;
mod modals_confirm_project;
mod modals_confirm_repo;
mod modals_confirm_settings;
mod modals_confirm_sync;
mod modals_spawn;
mod project_detail;
mod project_detail_render;
mod render;
mod right_panel;
mod session;
mod state;
mod state_bg;
mod state_misc;
mod state_modal;
mod state_rows;
mod tabs;
mod tabs_overview;
mod tabs_settings;
mod tabs_stats;
mod tabs_tables;

use actions::{load_bookmarks, load_pins, load_templates, refresh_repos, run_sync_flow};
use keys::tui_phase;
use state::{HubState, Modal, PhaseResult, SortMode, Tab};

use anyhow::Result;
use crate::config::AppConfig;
use crate::projects;
use crate::providers::{IssueDetail, PrDetail};
use crate::status::RepoSummary;
use std::collections::{HashMap, HashSet};

/// Kicks off a one-time, best-effort fetch of release info for the General
/// Settings tile: the very latest GitHub release (for the update check) and
/// the release matching the version actually running (for "shipped by X on
/// Y"). Unauthenticated, so it works with zero accounts configured.
fn spawn_release_info_fetch(state: &mut HubState) {
    let (tx, rx) = std::sync::mpsc::channel();
    let version = env!("CARGO_PKG_VERSION").to_string();
    let gh_users: Vec<String> = state.accounts.iter()
        .filter(|a| a.provider == "github" && a.base_url.is_none())
        .map(|a| a.username.clone())
        .collect();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() { Ok(rt) => rt, Err(_) => return };
        let (latest, current, since) = rt.block_on(async {
            let latest  = crate::release_info::fetch_latest().await.map_err(|e| format!("{e:#}"));
            let current = crate::release_info::fetch_current(&version).await.map_err(|e| format!("{e:#}"));
            let mut since = Vec::new();
            for user in &gh_users {
                if let Ok(date) = crate::release_info::fetch_user_since(user).await {
                    if !date.is_empty() { since.push((user.clone(), date)); }
                }
            }
            (latest, current, since)
        });
        let _ = tx.send((latest, current, since));
    });
    state.release_rx = Some(rx);
}

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
            r.stars           = m.stars;
            r.open_issues     = m.open_issues;
            r.open_prs        = m.open_prs;
            r.ci_status       = m.ci_status.clone();
            r.dependabot_open = m.dependabot_open;
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
        display: Vec::new(),
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
        activity_feed: None,
        activity_rx: None,
        activity_filter: None,
        vulp_log: Vec::new(),
        pending_cursor_restore: None,
        hub_settings_cur: 0,
        accounts,
        templates,
        pending_sync_action: None,
        pending_sync_msg:    None,
        pending_phase:       None,
        pull_all_rx:         None,
        refresh_rx:          None,
        create_repo_rx:      None,
        profile_pull_rx:     None,
        codeintel_rx:        None,
        digest_rx:           None,
        latest_release:      None,
        current_release:     None,
        account_since:       HashMap::new(),
        closed_counts:       None,
        release_rx:          None,
        cfg:                 cfg.clone(),
    };
    for r in &mut state.repos {
        r.bookmarked = state.bookmarks.contains(&r.path);
    }
    // Restore last session's tab + remember which repo to re-focus once the
    // scan finishes (cursor can't be set yet — repos stream in unsorted).
    let session = session::load();
    state.tab = session::u8_to_tab(session.tab);
    state.pending_cursor_restore = session.cursor_repo_path;
    state.rebuild_display();
    spawn_release_info_fetch(&mut state);
    // Tab starts on Overview, which now needs global stats (top repos/projects)
    // and the local vulp action log for its tiles.
    keys::maybe_load_global_stats(&mut state);
    crate::activity::compact(5000);
    state.vulp_log = {
        let mut log = crate::activity::load_logged();
        log.sort_by(|a, b| b.ts.cmp(&a.ts));
        log.truncate(50);
        log
    };

    // Carried across loop iterations so a terminal handed back by a sub-view
    // (repo/project detail) can be reused by the next `tui_phase` call
    // instead of tearing down and setting a fresh one up — see `tui_phase`'s
    // doc comment for the flash this avoids.
    let mut carried_term: Option<crate::tui::Tty> = None;
    loop {
        let (phase_result, handed_term) = tui_phase(&mut state, carried_term.take())?;

        if state.quit {
            session::save(state.tab, state.cur_repo_idx().map(|ri| state.repos[ri].path.clone()));
            break;
        }

        match phase_result {
            PhaseResult::OpenDetail => {
                state.open_detail = false;
                if let Some(ri) = state.cur_repo_idx() {
                    if let Ok(term) = crate::status::detail::run(state.repos[ri].clone(), cfg, handed_term).await {
                        carried_term = Some(term);
                    }
                } else if let Some(pi) = state.cur_proj_idx() {
                    let repo_indices = state.project_repos(pi);
                    let proj_repos: Vec<RepoSummary> =
                        repo_indices.iter().filter_map(|&ri| state.repos.get(ri)).cloned().collect();
                    carried_term = Some(project_detail::run(state.projects[pi].clone(), proj_repos, cfg, handed_term).await?);
                }
                // Detail view can rename/delete/archive repos or change branches —
                // rescan so the hub's list reflects whatever just happened, instead
                // of silently showing stale data until some other action refreshes it.
                refresh_repos(&mut state);
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
            PhaseResult::ExecOpenEditor(path) => {
                let editor = state.editor.clone();
                let _ = std::process::Command::new(&editor).arg(&path).status();
            }
            PhaseResult::None => {}
        }
    }
    Ok(())
}

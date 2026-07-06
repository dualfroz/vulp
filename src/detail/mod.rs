//! Repo-detail view: the full-screen TUI shown after selecting a single
//! repo from the hub. Owns the loop that polls background fetches, draws
//! the current tab, and routes key events — the actual state shape lives in
//! `types.rs`, key handling in `keys.rs`, background I/O in `settings_io.rs`,
//! out-of-TUI actions in `actions.rs`, and rendering in `render.rs`.

mod actions;
mod git;
mod keys;
mod render;
mod settings_io;
mod types;

pub use git::{activity_1d, activity_7d, activity_30d, activity_90d, activity_1y, activity_all};
pub use types::*;

/// Builds the full commit/tag history for `path`, exposed at the crate root
/// so the project-detail view can reuse the same history-building logic
/// without depending on `detail`'s private `git` submodule directly.
pub fn build_history_pub(path: &std::path::Path, limit: usize) -> Vec<HistEntry> {
    git::build_history(path, limit)
}

use crate::config::AppConfig;
use crate::status::{count_tracked_files, repo_detail, repo_recent_commits, RepoSummary};
use actions::exec_action;
use anyhow::Result;
use crossterm::event::Event;
use git::{
    build_history, build_tree, count_lines_of_code, count_stashes, count_tags, largest_files,
    list_all_files, list_branches, list_contributors, read_file_preview, remote_url,
    repo_first_commit_ts, total_commits,
};
use keys::{handle_key, handle_modal_key};
use settings_io::{compute_activity, poll_api_rx, poll_prs_issues_rx, poll_settings_rx, spawn_prs_issues_fetch, spawn_settings_fetch};

// ── entry point ───────────────────────────────────────────────────────────────

/// Opens the detail view for `summary`. Reuses an already-live terminal
/// handed over by the hub when given one (avoids a flash of the alternate
/// screen toggling off and back on), otherwise sets up a fresh one.
pub async fn run(
    summary:   RepoSummary,
    cfg:       &AppConfig,
    maybe_term: Option<crate::tui::Tty>,
) -> Result<()> {
    let path = summary.path.clone();
    let name = summary.name.clone();

    // Reuse the handed terminal (no LeaveAlternateScreen flash) or create a
    // new one. Either way, TUI is live before any blocking git calls.
    let mut terminal = match maybe_term {
        Some(t) => t,
        None    => crate::tui::setup()?,
    };

    // Show "Loading…" while we run the blocking git subprocesses.
    terminal.draw(|f| {
        use ratatui::widgets::Paragraph;
        f.render_widget(
            Paragraph::new(format!("  Loading {}…", name))
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)),
            f.area(),
        );
    })?;

    let detail        = repo_detail(&path).unwrap_or_else(|_| crate::status::RepoDetail {
        summary:        summary.clone(),
        modified_files: vec![],
        recent_commits: vec![],
        meta_issues:    None, meta_prs: None, meta_stars: None,
    });
    let all_files    = list_all_files(&path);
    let tree_nodes   = build_tree(&all_files);
    let tree_visible = rebuild_tree_visible(&tree_nodes);
    let contributors = list_contributors(&path);
    let branches     = list_branches(&path);
    let branch_count = branches.len().max(1);
    let first_ts     = repo_first_commit_ts(&path).unwrap_or(0);
    let repo_age_days = ((chrono::Utc::now().timestamp() - first_ts) / 86400).max(0) as u64;
    let total        = total_commits(&path);
    let url_str      = remote_url(&path);
    let loc          = count_lines_of_code(&path);
    let top_f        = largest_files(&path, 5);
    let activity     = activity_1y(&path);

    let mut st = St {
        tab:            Tab::Overview,
        files:          detail.modified_files,
        file_count:     count_tracked_files(&path),
        total_tags:     count_tags(&path),
        stash_count:    count_stashes(&path),
        branch_count,
        history:        build_history(&path, 200),
        hist_off:       0,
        total_commits:  total,
        branches,
        branch_cursor:  0,
        all_files,
        tree_nodes,
        tree_visible,
        tree_cursor:    0,
        file_preview:   vec![],
        preview_off:    0,
        contributors,
        repo_age_days,
        stats_off:      0,
        stats_period:   StatsPeriod::Year1,
        activity_data:  activity,
        loc_total:      loc,
        top_files:      top_f,
        ov_recent:      repo_recent_commits(&path, 6),
        ov_off:         0,
        remote_url_str: url_str,
        settings:       None,
        settings_cursor: 0,
        settings_rx:    None,
        settings_err:   None,
        api_rx:         None,
        repo_prs:       vec![],
        repo_issues:    vec![],
        repo_todos:     {
            let rname = name.clone();
            crate::todos::load_manual().into_iter()
                .filter(|t| t.repo.as_deref() == Some(&rname))
                .collect()
        },
        pr_cursor:      0,
        issue_cursor:   0,
        todo_cursor:    0,
        prs_loaded:     false,
        prs_issues_rx:  None,
        quit:           false,
        quit_after_op:  false,
        run_action:     None,
        status_msg:     None,
        cfg_data_dir:   cfg.data_dir.clone(),
        cfg_editor:     cfg.editor.clone(),
        pending_sync_mode:  None,
        pending_commit_msg: None,
        pending_tag_name:   None,
        pending_tag_msg:    None,
        pending_push_tag:   false,
        modal:          DetailModal::None,
        pending_file:   None,
        summary,
    };

    // Main loop — terminal stays alive the whole time.
    let result = run_detail_loop(&mut st, &mut terminal).await;
    crate::tui::teardown(terminal)?;
    result
}

async fn run_detail_loop(st: &mut St, terminal: &mut crate::tui::Tty) -> Result<()> {
    loop {
        let action = tui_phase_inner(st, terminal)?;
        if st.quit { break; }
        if let Some(a) = action {
            // Leave alternate screen for interactive prompts, then return.
            crate::tui::leave_tui(terminal)?;
            let r = exec_action(st, a).await;
            crate::tui::enter_tui(terminal)?;
            r?;
        }
    }
    Ok(())
}

/// Runs draw/poll/input frames until a key handler requests an out-of-TUI
/// action (returned as `Some(idx)`) or the view quits (returned as `None`).
/// `idx` values 40/41/50/80 are handled here rather than handed up to
/// `exec_action`, since they're cheap, non-interactive follow-ups (refresh a
/// file preview, recompute a stats chart, kick off a background fetch) that
/// don't need to leave the alternate screen.
fn tui_phase_inner(st: &mut St, terminal: &mut crate::tui::Tty) -> Result<Option<usize>> {
    let path = st.summary.path.clone();
    loop {
        poll_settings_rx(st);
        poll_api_rx(st);
        poll_prs_issues_rx(st);
        terminal.draw(|f| render::draw(f, st))?;
        let event = crate::tui::next_event(250)?;
        let key = match event { Some(Event::Key(k)) => k, _ => continue };

        if handle_modal_key(key.code, st) {
            if st.quit { return Ok(None); }
            if let Some(a) = st.run_action.take() {
                if a == 40 {
                    if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                        let node = &st.tree_nodes[ni];
                        if !node.is_dir {
                            st.file_preview = read_file_preview(&path, &node.rel_path, 200);
                        } else {
                            st.file_preview = vec![];
                        }
                        st.preview_off = 0;
                    }
                } else if a == 41 {
                    st.activity_data = compute_activity(&path, st.stats_period);
                } else if a == 50 {
                    spawn_settings_fetch(st);
                } else if a == 80 {
                    spawn_prs_issues_fetch(st);
                } else {
                    return Ok(Some(a));
                }
            }
            continue;
        }

        handle_key(key.code, st);
        if st.quit { return Ok(None); }

        if let Some(a) = st.run_action.take() {
            if a == 40 {
                if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                    let node = &st.tree_nodes[ni];
                    if !node.is_dir {
                        st.file_preview = read_file_preview(&path, &node.rel_path, 200);
                    } else {
                        st.file_preview = vec![];
                    }
                    st.preview_off = 0;
                }
                continue;
            }
            if a == 41 {
                st.activity_data = compute_activity(&path, st.stats_period);
                continue;
            }
            if a == 50 {
                spawn_settings_fetch(st);
                continue;
            }
            if a == 80 {
                spawn_prs_issues_fetch(st);
                continue;
            }
            return Ok(Some(a));
        }
    }
}

/// Triggers a Settings-tab fetch the first time that tab is visited this
/// session, so switching tabs repeatedly doesn't refetch on every visit.
fn maybe_load_settings(st: &mut St) {
    if st.tab == Tab::Settings && st.settings.is_none() {
        st.run_action = Some(50);
    }
}

/// Triggers a PRs/Issues fetch the first time either of those tabs is
/// visited this session, mirroring `maybe_load_settings`.
fn maybe_load_prs_issues(st: &mut St) {
    if (st.tab == Tab::PRs || st.tab == Tab::Issues) && !st.prs_loaded && st.prs_issues_rx.is_none() {
        st.run_action = Some(80);
    }
}

//! The hub's main event loop: one terminal frame, one input event, dispatched
//! to whichever input mode is active (modal open, confirm prompt, filter
//! typing, or the plain global keymap in `keys_dispatch`), plus the two
//! lazy-loaders that kick off a background fetch the first time their tab
//! is opened.

use anyhow::Result;
use crossterm::event::{Event, KeyCode, MouseEventKind};
use super::modals::{handle_modal_key, process_modal_confirm};
use super::render::render_hub;
use super::state::{HubState, Modal, ModalAction, PendingAction, PhaseResult, Tab};
use super::{keys_dispatch, keys_poll};

pub fn maybe_load_global_stats(state: &mut HubState) {
    if !matches!(state.tab, Tab::Stats | Tab::Overview) { return; }
    // Wait for the workspace scan to finish — computing stats from a
    // partially-streamed repo list bakes in an empty/incomplete snapshot.
    if state.scan_rx.is_some() { return; }
    if state.global_stats.is_some() || state.global_stats_rx.is_some() { return; }

    let repos_snapshot: Vec<(String, std::path::PathBuf, Option<String>, bool, usize)> = state.repos.iter()
        .map(|r| (r.name.clone(), r.path.clone(), r.lang.clone(), r.is_dirty, r.behind))
        .collect();
    let dirty_count  = state.repos.iter().filter(|r| r.is_dirty).count();
    let behind_count = state.repos.iter().filter(|r| r.behind > 0).count();
    let period = state.global_stats_period;
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        use super::state::GlobalStats;
        let mut all_dates: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut top_repos: Vec<(String, usize)> = vec![];
        let mut total_commits: usize = 0;

        for (name, path, _lang, _dirty, _behind) in &repos_snapshot {
            let activity = match period {
                crate::detail::StatsPeriod::Day1    => crate::detail::activity_1d(path),
                crate::detail::StatsPeriod::Day7    => crate::detail::activity_7d(path),
                crate::detail::StatsPeriod::Day30   => crate::detail::activity_30d(path),
                crate::detail::StatsPeriod::Day90   => crate::detail::activity_90d(path),
                crate::detail::StatsPeriod::Year1   => crate::detail::activity_1y(path),
                crate::detail::StatsPeriod::AllTime => crate::detail::activity_all(path),
            };
            let repo_total: usize = activity.iter().map(|(_, c)| c).sum();
            total_commits += repo_total;
            if repo_total > 0 { top_repos.push((name.clone(), repo_total)); }
            for (date, count) in activity {
                *all_dates.entry(date).or_default() += count;
            }
        }

        let mut day_pattern = [0usize; 7];
        for (date_str, count) in &all_dates {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                use chrono::Datelike;
                let wd = d.weekday().num_days_from_monday() as usize;
                day_pattern[wd] += count;
            }
        }

        let mut activity: Vec<(String, usize)> = all_dates.into_iter().collect();
        activity.sort_by(|a, b| a.0.cmp(&b.0));
        top_repos.sort_by(|a, b| b.1.cmp(&a.1));
        let repo_commits = top_repos.clone();
        top_repos.truncate(5);

        let mut lang_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_, _, lang, _, _) in &repos_snapshot {
            // Repo lang strings can carry a percentage suffix ("TypeScript 92%") —
            // aggregate on the bare language name so they count as one language.
            if let Some(l) = lang {
                let name = l.split_whitespace().next().unwrap_or(l).to_string();
                *lang_map.entry(name).or_default() += 1;
            }
        }
        let mut lang_dist: Vec<(String, usize)> = lang_map.into_iter().collect();
        lang_dist.sort_by(|a, b| b.1.cmp(&a.1));
        lang_dist.truncate(8);

        let _ = tx.send(GlobalStats {
            activity, lang_dist, top_repos, repo_commits, total_commits,
            dirty_count, behind_count,
            day_pattern,
        });
    });

    state.global_stats_rx = Some(rx);
    state.status_msg = Some("Computing global stats...".into());
}

/// Builds the Activity tab's feed: git commits + tags (ground truth, sourced
/// straight from each repo's history, regardless of how the activity
/// happened) merged with PR/issue timestamps already in memory, merged with
/// the locally-logged push/pull/release/new-repo/todo events that only vulp
/// itself can see (see `crate::activity`).
pub fn maybe_load_activity(state: &mut HubState) {
    if state.tab != Tab::Activity { return; }
    if state.activity_feed.is_some() || state.activity_rx.is_some() { return; }

    let repos_snapshot: Vec<(String, std::path::PathBuf)> = state.repos.iter()
        .map(|r| (r.name.clone(), r.path.clone())).collect();
    let mut entries: Vec<crate::activity::ActivityEntry> = state.prs.iter().map(|pr| {
        crate::activity::ActivityEntry {
            ts: pr.created_ts, kind: crate::activity::ActivityKind::Pr,
            repo: pr.repo.clone(), detail: format!("#{} {} (@{})", pr.number, pr.title, pr.author),
        }
    }).collect();
    entries.extend(state.issues.iter().map(|iss| {
        crate::activity::ActivityEntry {
            ts: iss.created_ts, kind: crate::activity::ActivityKind::Issue,
            repo: iss.repo.clone(), detail: format!("#{} {} (@{})", iss.number, iss.title, iss.author),
        }
    }));

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut entries = entries;
        entries.extend(crate::activity::load_logged());
        for (name, path) in &repos_snapshot {
            for h in crate::detail::build_history_pub(path, 200) {
                match h {
                    crate::detail::HistEntry::Commit { ts, author, msg, .. } => {
                        entries.push(crate::activity::ActivityEntry {
                            ts, kind: crate::activity::ActivityKind::Commit,
                            repo: name.clone(), detail: format!("{author}: {msg}"),
                        });
                    }
                    crate::detail::HistEntry::Tag { ts, name: tag, msg, .. } => {
                        entries.push(crate::activity::ActivityEntry {
                            ts, kind: crate::activity::ActivityKind::Release,
                            repo: name.clone(), detail: if msg.is_empty() { tag } else { format!("{tag}: {msg}") },
                        });
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.ts.cmp(&a.ts));
        entries.truncate(2000);
        let _ = tx.send(entries);
    });

    state.activity_rx = Some(rx);
    state.status_msg = Some("Loading activity...".into());
}

/// Runs the hub's TUI for one or more frames until something needs to leave
/// the loop: draws a frame, polls every background channel, reads one input
/// event, and routes it to whichever input mode is currently active. Modal/
/// confirm/filter input short-circuit here (they're loop control flow, not
/// part of the global keymap); everything else goes to `keys_dispatch`.
///
/// `existing_term` lets the caller hand back a terminal a sub-view (repo/
/// project detail) just returned instead of tearing down and setting up a
/// fresh one — the previous "leave alt screen, blink the real terminal,
/// re-enter" was visible for a frame every time detail handed control back.
pub fn tui_phase(state: &mut HubState, existing_term: Option<crate::tui::Tty>) -> Result<(PhaseResult, Option<crate::tui::Tty>)> {
    let mut terminal = match existing_term {
        Some(t) => t,
        None    => crate::tui::setup()?,
    };
    let result = (|| -> Result<PhaseResult> {
        loop {
            state.poll_incoming();
            // No-op until the scan completes and only on Overview/Stats —
            // kicks off the first stats computation once repos are final.
            maybe_load_global_stats(state);
            state.poll_diff();
            state.poll_global_stats();
            state.poll_activity();
            state.poll_release_info();
            state.poll_codeintel();
            state.poll_digest();
            keys_poll::poll_pull_all(state);
            keys_poll::poll_create_repo(state);
            keys_poll::poll_profile_pull(state);
            keys_poll::poll_refresh(state);

            terminal.draw(|f| render_hub(f, state))?;
            state.update_focused_extra();

            let timeout_ms = if state.scan_rx.is_some() { 50 } else { 250 };
            let event = crate::tui::next_event(timeout_ms)?;

            if let Some(Event::Resize(_, _)) = &event {
                let _ = terminal.clear();
                continue;
            }

            if let Some(Event::Mouse(m)) = &event {
                match m.kind {
                    MouseEventKind::ScrollDown => { state.scroll_down(); }
                    MouseEventKind::ScrollUp   => { state.scroll_up(); }
                    _ => {}
                }
                continue;
            }
            let key = match event { Some(Event::Key(k)) => k, _ => continue };

            if state.modal.is_some() {
                match handle_modal_key(&mut state.modal, key.code) {
                    ModalAction::Confirm => process_modal_confirm(state),
                    ModalAction::Cancel  => state.modal = Modal::None,
                    ModalAction::Pending => {}
                }
                if let Some(url) = state.pending_clone.take() {
                    return Ok(PhaseResult::ExecClone(url));
                }
                if let Some(phase) = state.pending_phase.take() {
                    return Ok(phase);
                }
                continue;
            }

            if state.confirming.is_some() {
                match key.code {
                    KeyCode::Char('y') => { state.exec_action = true; }
                    KeyCode::Esc       => { state.confirming = None; }
                    _ => {}
                }
                if state.exec_action {
                    let PendingAction::Release = state.confirming.as_ref().unwrap();
                    return Ok(PhaseResult::ExecRelease);
                }
                continue;
            }

            if state.filtering {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('/') => {
                        state.filtering = false;
                        state.filter_text.clear();
                        state.rebuild_display();
                    }
                    KeyCode::Enter => { state.filtering = false; }
                    KeyCode::Backspace => {
                        state.filter_text.pop();
                        state.rebuild_display();
                    }
                    KeyCode::Char(c) => {
                        state.filter_text.push(c);
                        state.rebuild_display();
                    }
                    _ => {}
                }
                continue;
            }

            state.status_msg = None;
            if let Some(phase) = keys_dispatch::handle_global_key(state, key) {
                return Ok(phase);
            }
        }
    })();
    let phase = result?;
    // When opening detail, hand the terminal over instead of tearing down —
    // detail will reuse it, eliminating the LeaveAlternateScreen flash.
    if matches!(phase, PhaseResult::OpenDetail) {
        return Ok((phase, Some(terminal)));
    }
    crate::tui::teardown(terminal)?;
    Ok((phase, None))
}

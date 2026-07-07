//! Per-frame polling of hub background-thread channels that don't have a
//! dedicated `HubState::poll_*` method: pull-all, repo creation, and
//! settings-profile pull. Each function is a no-op if its channel isn't
//! open, so all three are safe to call unconditionally every loop iteration.

use super::actions::refresh_repos;
use super::state::{CreateRepoMsg, HubState, Modal, ProfilePullMsg};

/// Drains progress messages from a "pull all repos for this account" job
/// and, once it finishes, reloads settings (if the job imported any) and
/// rescans the workspace so the newly-cloned repos show up.
pub fn poll_pull_all(state: &mut HubState) {
    let done = if let Some(rx) = &state.pull_all_rx {
        let mut finished = false;
        let mut settings_imported = false;
        while let Ok(msg) = rx.try_recv() {
            if msg == "DONE" { finished = true; }
            else if msg == "SETTINGS_IMPORTED" { settings_imported = true; }
            else { state.status_msg = Some(msg); }
        }
        if settings_imported {
            if let Ok(cfg) = crate::config::load_config() {
                state.editor        = cfg.editor.clone();
                state.clipboard_cmd = cfg.clipboard_cmd.clone();
                state.cfg            = cfg;
            }
        }
        finished
    } else { false };
    if done {
        state.pull_all_rx = None;
        state.status_msg  = Some("Pull-all complete! Refreshing...".into());
        refresh_repos(state);
    }
}

/// Drains progress from a background "create repo" job, appending lines to
/// the open `Modal::Info` (if any) so the user sees live progress, then
/// replaces it with the final result once the job reports `Done`.
pub fn poll_create_repo(state: &mut HubState) {
    let outcome = if let Some(rx) = &state.create_repo_rx {
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                CreateRepoMsg::Progress(s) => {
                    match &mut state.modal {
                        Modal::Info { lines, .. } => lines.push(s),
                        _ => state.status_msg = Some(s),
                    }
                }
                CreateRepoMsg::Done { success, name, detail } => done = Some((success, name, detail)),
            }
        }
        done
    } else { None };
    if let Some((success, name, detail)) = outcome {
        state.create_repo_rx = None;
        let title = if success { format!("✓ '{}' created", name) } else { format!("✗ '{}' failed", name) };
        let mut lines = match std::mem::replace(&mut state.modal, Modal::None) {
            Modal::Info { lines, .. } => lines,
            _ => vec![],
        };
        lines.push(detail);
        state.modal = Modal::Info { title, lines, scroll: 0 };
        refresh_repos(state);
    }
}

/// Drains progress from a background "pull settings profile from the sync
/// repo" job and, once it finishes, applies the fetched snapshot to `cfg`
/// (rescanning the workspace too, if the snapshot included repo structure).
pub fn poll_profile_pull(state: &mut HubState) {
    let outcome = if let Some(rx) = &state.profile_pull_rx {
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            let ProfilePullMsg::Done(result) = msg;
            done = Some(result);
        }
        done
    } else { None };
    if let Some(result) = outcome {
        state.profile_pull_rx = None;
        match result {
            Ok(snap) => {
                let has_structure = snap.structure.is_some();
                match crate::profile::apply(&snap, &mut state.cfg) {
                    Ok(()) => {
                        state.editor = state.cfg.editor.clone();
                        state.clipboard_cmd = state.cfg.clipboard_cmd.clone();
                        if has_structure { refresh_repos(state); }
                        state.modal = Modal::Info {
                            title: "✓ Settings pulled".into(),
                            lines: vec!["Applied successfully".into()],
                            scroll: 0,
                        };
                    }
                    Err(e) => {
                        state.modal = Modal::Info {
                            title: "✗ Pull failed".into(),
                            lines: vec![format!("Fetched but failed to apply: {e:#}")],
                            scroll: 0,
                        };
                    }
                }
            }
            Err(e) => {
                state.modal = Modal::Info {
                    title: "✗ Pull failed".into(),
                    lines: vec![e],
                    scroll: 0,
                };
            }
        }
    }
}

/// Drains the background API-refresh channel: progress goes to the status
/// line, `Done` applies fetched metadata/PRs/issues to state + cache and
/// rebuilds the display.
pub fn poll_refresh(state: &mut HubState) {
    use super::state::RefreshMsg;
    let mut done = None;
    if let Some(rx) = &state.refresh_rx {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                RefreshMsg::Progress(s) => state.status_msg = Some(s),
                RefreshMsg::Done { .. } => { done = Some(msg); }
            }
        }
        // Thread gone without Done — drop the receiver so [R] works again.
        if done.is_none() {
            if let Err(std::sync::mpsc::TryRecvError::Disconnected) = rx.try_recv() {
                state.refresh_rx = None;
                state.status_msg = Some("Refresh failed (thread died)".into());
            }
        }
    }
    let Some(RefreshMsg::Done { metas, prs, issues, closed_counts }) = done else { return };

    let mut cache = crate::cache::load();
    let mut fetched_meta = 0usize;
    for (name, meta) in metas {
        if let Some(r) = state.repos.iter_mut().find(|r| r.name == name) {
            r.stars           = meta.stars;
            r.open_issues     = meta.open_issues;
            r.open_prs        = meta.open_prs;
            r.ci_status       = meta.ci_status.clone();
            r.dependabot_open = meta.dependabot_open;
            r.deploy_status   = meta.deploy_status.clone();
        }
        cache.set_meta(&name, meta);
        fetched_meta += 1;
    }
    let mut fetched_prs = 0usize;
    for (name, p) in prs { fetched_prs += p.len(); cache.set_prs(&name, p); }
    let mut fetched_iss = 0usize;
    for (name, i) in issues { fetched_iss += i.len(); cache.set_issues(&name, i); }
    let _ = crate::cache::save(&cache);

    state.prs    = cache.all_prs().into_iter().cloned().collect();
    state.issues = cache.all_issues().into_iter().cloned().collect();
    if closed_counts.is_some() { state.closed_counts = closed_counts; }

    state.refresh_rx = None;
    state.status_msg = Some(format!(
        "Refreshed {} repos: {} PRs, {} issues", fetched_meta, fetched_prs, fetched_iss
    ));
    state.rebuild_display();
}

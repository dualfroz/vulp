use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind};
use super::actions::{copy_to_clipboard, get_remote_url, refresh_repos, scan_todos_from_repos, save_bookmarks, save_pins, to_web_url};
use super::modals::{handle_modal_key, process_modal_confirm};
use super::render::{render_hub, HELP_LINES};
use super::state::{
    ConfirmTag, DisplayRow, HubState, Modal, ModalAction, PendingAction,
    PhaseResult, SelectTag, Tab, TextTag,
};
use crate::todos;

pub fn maybe_load_global_stats(state: &mut HubState) {
    if state.tab != Tab::Stats { return; }
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
                let wd = d.weekday().num_days_from_monday() as usize;
                day_pattern[wd] += count;
            }
        }

        let mut activity: Vec<(String, usize)> = all_dates.into_iter().collect();
        activity.sort_by(|a, b| a.0.cmp(&b.0));
        top_repos.sort_by(|a, b| b.1.cmp(&a.1));
        top_repos.truncate(5);

        let mut lang_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_, _, lang, _, _) in &repos_snapshot {
            if let Some(l) = lang { *lang_map.entry(l.clone()).or_default() += 1; }
        }
        let mut lang_dist: Vec<(String, usize)> = lang_map.into_iter().collect();
        lang_dist.sort_by(|a, b| b.1.cmp(&a.1));
        lang_dist.truncate(8);

        let _ = tx.send(GlobalStats {
            activity, lang_dist, top_repos, total_commits,
            dirty_count, behind_count,
            day_pattern,
        });
    });

    state.global_stats_rx = Some(rx);
    state.status_msg = Some("Computing global stats...".into());
}

pub fn tui_phase(state: &mut HubState) -> Result<(PhaseResult, Option<crate::tui::Tty>)> {
    let mut terminal = crate::tui::setup()?;
    let result = (|| -> Result<PhaseResult> {
        loop {
            state.poll_incoming();
            state.poll_diff();
            state.poll_global_stats();
            // poll pull-all background worker
            {
                let done = if let Some(rx) = &state.pull_all_rx {
                    let mut finished = false;
                    while let Ok(msg) = rx.try_recv() {
                        if msg == "DONE" { finished = true; }
                        else { state.status_msg = Some(msg); }
                    }
                    finished
                } else { false };
                if done {
                    state.pull_all_rx = None;
                    state.status_msg  = Some("Pull-all complete! Refreshing...".into());
                    refresh_repos(state);
                }
            }
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
            {
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
                        let r = match state.confirming.as_ref().unwrap() {
                            PendingAction::Sync    => PhaseResult::ExecSync,
                            PendingAction::Release => PhaseResult::ExecRelease,
                        };
                        return Ok(r);
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
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if !state.filter_text.is_empty() {
                            state.filter_text.clear(); state.rebuild_display();
                        } else {
                            state.quit = true; return Ok(PhaseResult::None);
                        }
                    }
                    KeyCode::Up   | KeyCode::Char('k') => {
                        if state.tab == Tab::HubSettings {
                            state.hub_settings_cur = state.hub_settings_cur.saturating_sub(1);
                        } else {
                            state.scroll_up();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.tab == Tab::HubSettings {
                            let max = 10 + 1 + state.templates.len();
                            if state.hub_settings_cur + 1 < max { state.hub_settings_cur += 1; }
                        } else {
                            state.scroll_down();
                        }
                    }
                    KeyCode::Char('A') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let visible: Vec<usize> = state.display.iter().filter_map(|row| {
                            if let DisplayRow::Repo { repo_idx } = row { Some(*repo_idx) } else { None }
                        }).collect();
                        let all_sel = visible.iter().all(|ri| state.selected.contains(ri));
                        if all_sel {
                            state.selected.clear();
                            state.status_msg = Some("All deselected".into());
                        } else {
                            for ri in visible { state.selected.insert(ri); }
                            state.status_msg = Some(format!("Selected {} repos", state.selected.len()));
                        }
                    }
                    KeyCode::Char('*') => {
                        state.modal = Modal::Select {
                            title: "Select by condition".into(),
                            options: vec![
                                "All visible".into(),
                                "Dirty (uncommitted changes)".into(),
                                "Behind remote".into(),
                                "Ahead of remote".into(),
                                "No license".into(),
                                "Language: Rust".into(),
                                "Language: TypeScript".into(),
                                "Language: Python".into(),
                                "Language: Go".into(),
                            ],
                            cursor: 0,
                            tag: SelectTag::SelectByCondition,
                        };
                    }
                    KeyCode::Char(' ') => {
                        match state.tab {
                            Tab::Todos => {
                                if let Some(ti) = state.cur_todo_idx() {
                                    state.todos[ti].status = state.todos[ti].status.cycle();
                                    state.todos[ti].touch();
                                    let _ = todos::save(&state.todos);
                                }
                            }
                            _ => state.toggle_select(),
                        }
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        state.tab = match state.tab {
                            Tab::Overview    => Tab::Repos,
                            Tab::Repos       => Tab::Projects,
                            Tab::Projects    => Tab::Todos,
                            Tab::Todos       => Tab::PRs,
                            Tab::PRs         => Tab::Issues,
                            Tab::Issues      => Tab::Stats,
                            Tab::Stats       => Tab::HubSettings,
                            Tab::HubSettings => Tab::Overview,
                        };
                        state.cursor = 0; state.offset = 0; state.selected.clear();
                        state.rebuild_display();
                        maybe_load_global_stats(state);
                    }
                    KeyCode::Left => {
                        state.tab = match state.tab {
                            Tab::Overview    => Tab::HubSettings,
                            Tab::Repos       => Tab::Overview,
                            Tab::Projects    => Tab::Repos,
                            Tab::Todos       => Tab::Projects,
                            Tab::PRs         => Tab::Todos,
                            Tab::Issues      => Tab::PRs,
                            Tab::Stats       => Tab::Issues,
                            Tab::HubSettings => Tab::Stats,
                        };
                        state.cursor = 0; state.offset = 0; state.selected.clear();
                        state.rebuild_display();
                        maybe_load_global_stats(state);
                    }
                    KeyCode::Enter => {
                        match state.tab {
                            Tab::Repos | Tab::Projects => {
                                if state.cur_repo_idx().is_some() || state.cur_proj_idx().is_some() {
                                    state.open_detail = true;
                                    return Ok(PhaseResult::OpenDetail);
                                }
                            }
                            Tab::PRs => {
                                let di = state.cur_di();
                                if let Some(di) = di {
                                    if let DisplayRow::Pr { idx } = &state.display[di] {
                                        let url = state.prs[*idx].url.clone();
                                        if !url.is_empty() {
                                            crate::util::open_path(&url);
                                            state.status_msg = Some("Opening PR in browser...".into());
                                        }
                                    }
                                }
                            }
                            Tab::Issues => {
                                let di = state.cur_di();
                                if let Some(di) = di {
                                    if let DisplayRow::Issue { idx } = &state.display[di] {
                                        let url = state.issues[*idx].url.clone();
                                        if !url.is_empty() {
                                            crate::util::open_path(&url);
                                            state.status_msg = Some("Opening issue in browser...".into());
                                        }
                                    }
                                }
                            }
                            Tab::Todos | Tab::Overview | Tab::Stats => {}
                            Tab::HubSettings => {
                                match state.hub_settings_cur {
                                    0 => {
                                        state.modal = Modal::Text {
                                            title: "Workspace Root".into(),
                                            prompt: "Absolute path (~ supported):".into(),
                                            value: state.workspace_root.to_string_lossy().to_string(),
                                            tag: TextTag::SetWorkspace,
                                            max_len: None,
                                        };
                                    }
                                    1 => {
                                        state.modal = Modal::Text {
                                            title: "Editor".into(),
                                            prompt: "Command (e.g. nvim, code):".into(),
                                            value: state.editor.clone(),
                                            tag: TextTag::SetEditor,
                                            max_len: Some(60),
                                        };
                                    }
                                    2 => {
                                        state.modal = Modal::Text {
                                            title: "Clipboard Command".into(),
                                            prompt: "Command (e.g. xclip -sel clip):".into(),
                                            value: state.clipboard_cmd.clone(),
                                            tag: TextTag::SetClipboard,
                                            max_len: Some(60),
                                        };
                                    }
                                    7 => {
                                        if state.accounts.is_empty() {
                                            state.status_msg = Some("No accounts — run: fxg auth import".into());
                                        } else if state.accounts.len() == 1 {
                                            let name = format!("@{} ({})", state.accounts[0].username, state.accounts[0].provider);
                                            state.modal = Modal::Confirm {
                                                message: format!("Pull all repos for {}?", name),
                                                tag: ConfirmTag::PullAll(0),
                                            };
                                        } else {
                                            let opts: Vec<String> = state.accounts.iter()
                                                .map(|a| format!("@{} ({})", a.username, a.provider))
                                                .collect();
                                            state.modal = Modal::Select {
                                                title: "Pull all repos — Account".into(),
                                                options: opts,
                                                cursor: 0,
                                                tag: SelectTag::PullAllAccount,
                                            };
                                        }
                                    }
                                    8 => {
                                        state.modal = Modal::Text {
                                            title: "Import Directory as Repos".into(),
                                            prompt: "Directory to scan (git init + link each subfolder):".into(),
                                            value: String::new(),
                                            tag: TextTag::ImportDir,
                                            max_len: None,
                                        };
                                    }
                                    9 => {
                                        let n = state.repos.len();
                                        state.modal = Modal::Confirm {
                                            message: format!("DANGER: Delete .git from all {} repos in workspace? Cannot be undone!", n),
                                            tag: ConfirmTag::RemoveGitAll,
                                        };
                                    }
                                    10 => {
                                        state.modal = Modal::Text {
                                            title: "New Template — Name".into(),
                                            prompt: "Template name:".into(),
                                            value: String::new(),
                                            tag: TextTag::NewTemplateName,
                                            max_len: Some(50),
                                        };
                                    }
                                    n if n >= 11 => {
                                        let ti = n - 11;
                                        if ti < state.templates.len() {
                                            let t = &state.templates[ti];
                                            let cmd_preview = t.commands.iter().take(2).cloned().collect::<Vec<_>>().join("; ");
                                            let cmd_str = if cmd_preview.is_empty() { "(no commands)".into() } else { cmd_preview };
                                            state.modal = Modal::Select {
                                                title: format!("Template: {}", t.name),
                                                options: vec![
                                                    format!("Commands: {}", cmd_str.chars().take(40).collect::<String>()),
                                                    "Delete template".into(),
                                                ],
                                                cursor: 0,
                                                tag: SelectTag::TemplateMenu(ti),
                                            };
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        if !state.action_targets().is_empty() {
                            state.modal = Modal::Select {
                                title: "Sync — Action".into(),
                                options: vec!["Pull".into(), "Commit+Push".into(), "Push only".into()],
                                cursor: 0,
                                tag: SelectTag::SyncAction,
                            };
                        }
                    }
                    KeyCode::Char('r') => {
                        if !state.action_targets().is_empty() { state.confirming = Some(PendingAction::Release); }
                    }
                    KeyCode::Char('n') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let cur = state.repos[ri].name.clone();
                            state.modal = Modal::Text { title: "Set label".into(), prompt: "Display name (blank = reset):".into(), value: cur, tag: TextTag::Label(ri), max_len: Some(60) };
                        }
                    }
                    KeyCode::Char('p') if state.tab == Tab::Stats => {
                        state.global_stats        = None;
                        state.global_stats_rx     = None;
                        state.global_stats_period = state.global_stats_period.next();
                        maybe_load_global_stats(state);
                    }
                    KeyCode::Char('<') if state.tab == Tab::Stats => {
                        state.global_stats        = None;
                        state.global_stats_rx     = None;
                        state.global_stats_period = state.global_stats_period.prev();
                        maybe_load_global_stats(state);
                    }
                    KeyCode::Char('p') => {
                        let mut opts = vec!["Create project".to_string()];
                        if !state.projects.is_empty() {
                            opts.push("Assign repos".into());
                            opts.push("Rename / recolor".into());
                            opts.push("Delete project".into());
                        }
                        state.modal = Modal::Select { title: "Projects".into(), options: opts, cursor: 0, tag: SelectTag::ProjMenu };
                    }
                    KeyCode::Char('+') => {
                        state.modal = Modal::Text { title: "New todo — Title".into(), prompt: "Title:".into(), value: String::new(), tag: TextTag::TodoTitle, max_len: Some(100) };
                    }
                    KeyCode::Char('e') => {
                        if let Some(ti) = state.cur_todo_idx() {
                            let cur = state.todos[ti].title.clone();
                            state.modal = Modal::Text { title: "Edit todo — Title".into(), prompt: "Title:".into(), value: cur, tag: TextTag::TodoEditTitle(ti), max_len: Some(100) };
                        }
                    }
                    KeyCode::Delete | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::SHIFT) => {
                        if state.tab == Tab::Todos {
                            if let Some(ti) = state.cur_todo_idx() {
                                let title = state.todos[ti].title.clone();
                                state.modal = Modal::Confirm { message: format!("Delete '{}'?", title), tag: ConfirmTag::DeleteTodo(ti) };
                            }
                        } else if state.tab == Tab::Projects {
                            if let Some(pi) = state.cur_proj_idx() {
                                let name = state.projects[pi].name.clone();
                                state.modal = Modal::Confirm { message: format!("Delete project '{}'?", name), tag: ConfirmTag::DeleteProject(pi) };
                            }
                        }
                    }
                    KeyCode::Delete => {
                        if state.tab == Tab::Todos {
                            if let Some(ti) = state.cur_todo_idx() {
                                let title = state.todos[ti].title.clone();
                                state.modal = Modal::Confirm { message: format!("Delete '{}'?", title), tag: ConfirmTag::DeleteTodo(ti) };
                            }
                        } else if state.tab == Tab::Projects {
                            if let Some(pi) = state.cur_proj_idx() {
                                let name = state.projects[pi].name.clone();
                                state.modal = Modal::Confirm { message: format!("Delete project '{}'?", name), tag: ConfirmTag::DeleteProject(pi) };
                            }
                        }
                    }
                    KeyCode::Char('d') => { state.filter_dirty  = !state.filter_dirty;  state.rebuild_display(); }
                    KeyCode::Char('b') => { state.filter_behind = !state.filter_behind; state.rebuild_display(); }
                    KeyCode::Char('f') => { state.filter_ahead  = !state.filter_ahead;  state.rebuild_display(); }
                    KeyCode::Char('/') => {
                        state.filtering = true;
                        state.filter_text.clear();
                        state.rebuild_display();
                    }
                    KeyCode::Char('?') => {
                        state.modal = Modal::Info {
                            title: "Keybindings".into(),
                            lines: HELP_LINES.iter().map(|s| s.to_string()).collect(),
                            scroll: 0,
                        };
                    }
                    KeyCode::Char('S') => {
                        let sm = state.sort_mode;
                        let dot = |m: super::state::SortMode| if sm == m { "●" } else { "○" };
                        let chk = |b: bool| if b { "✓" } else { " " };
                        state.modal = Modal::Select {
                            title: "Sort & Filter".into(),
                            options: vec![
                                format!("{}  Default order",     dot(super::state::SortMode::Default)),
                                format!("{}  By name",           dot(super::state::SortMode::Name)),
                                format!("{}  By recent commit",  dot(super::state::SortMode::Age)),
                                format!("{}  Dirty first",       dot(super::state::SortMode::DirtyFirst)),
                                format!("{}  Behind first",      dot(super::state::SortMode::BehindFirst)),
                                format!("[{}] Show dirty only",  chk(state.filter_dirty)),
                                format!("[{}] Show behind only", chk(state.filter_behind)),
                                format!("[{}] Show ahead only",  chk(state.filter_ahead)),
                                format!("[{}] Show archived",    chk(state.filter_archived)),
                                "    Reset all".into(),
                            ],
                            cursor: 0,
                            tag: SelectTag::SortFilter,
                        };
                    }
                    KeyCode::Char('o') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            state.pending_phase = Some(PhaseResult::ExecOpenEditor(path));
                        }
                    }
                    KeyCode::Char('y') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.to_string_lossy().to_string();
                            let cmd  = state.clipboard_cmd.clone();
                            if copy_to_clipboard(&path, &cmd) {
                                state.status_msg = Some("Path copied!".into());
                            } else {
                                state.status_msg = Some("No clipboard cmd".into());
                            }
                        }
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            state.modal = Modal::Select {
                                title: "Copy URL".into(),
                                options: vec!["SSH URL (git@…)".into(), "HTTPS URL (https://…)".into()],
                                cursor: 0,
                                tag: SelectTag::CopyUrl(path),
                            };
                        }
                    }
                    KeyCode::Char('u') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            let cmd  = state.clipboard_cmd.clone();
                            if let Some(url) = get_remote_url(&path) {
                                if copy_to_clipboard(&url, &cmd) {
                                    state.status_msg = Some("SSH URL copied!".into());
                                } else {
                                    state.status_msg = Some(format!("  {}", url));
                                }
                            } else {
                                state.status_msg = Some("No remote origin".into());
                            }
                        }
                    }
                    KeyCode::Char('B') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            if state.bookmarks.contains(&path) {
                                state.bookmarks.remove(&path);
                                state.repos[ri].bookmarked = false;
                                state.status_msg = Some("Bookmark removed".into());
                            } else {
                                state.bookmarks.insert(path.clone());
                                state.repos[ri].bookmarked = true;
                                state.status_msg = Some("Bookmarked!".into());
                            }
                            save_bookmarks(&state.bookmarks);
                            state.rebuild_display();
                        }
                    }
                    KeyCode::Char('T') => {
                        let count = scan_todos_from_repos(state);
                        state.status_msg = Some(format!("Scanned: {} new code todos", count));
                        state.tab = Tab::Todos;
                        state.show_code_todos = true;
                        state.rebuild_display();
                    }
                    KeyCode::Char('X') => {
                        if state.tab == Tab::Todos {
                            state.show_code_todos = !state.show_code_todos;
                            state.rebuild_display();
                            state.status_msg = Some(if state.show_code_todos {
                                "Showing code scan todos".into()
                            } else {
                                "Showing manual todos only".into()
                            });
                        }
                    }
                    KeyCode::Char('g') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let gitignore = state.repos[ri].path.join(".gitignore");
                            state.pending_phase = Some(PhaseResult::ExecOpenEditor(gitignore));
                            state.status_msg = Some("Opening .gitignore...".into());
                        }
                    }
                    KeyCode::Char('O') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.to_string_lossy().to_string();
                            crate::util::open_path(&path);
                            state.status_msg = Some("Opening in file manager...".into());
                        }
                    }
                    KeyCode::Char('W') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            if let Some(url) = get_remote_url(&path) {
                                let web = to_web_url(&url);
                                crate::util::open_path(&web);
                                state.status_msg = Some("Opening in browser...".into());
                            } else {
                                state.status_msg = Some("No remote origin".into());
                            }
                        }
                    }
                    KeyCode::Char('P') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            if state.pins.contains(&path) {
                                state.pins.remove(&path);
                                state.status_msg = Some("Unpinned".into());
                            } else {
                                state.pins.insert(path.clone());
                                state.status_msg = Some("Pinned to top! ◆".into());
                            }
                            save_pins(&state.pins);
                            state.rebuild_display();
                        }
                    }
                    KeyCode::Char('a') => {
                        state.filter_archived = !state.filter_archived;
                        state.rebuild_display();
                    }
                    KeyCode::Char('N') => {
                        state.modal = Modal::Select {
                            title: "New repository".into(),
                            options: vec![
                                "Clone from URL".into(),
                                "Clone local folder (path)".into(),
                                "Create new empty repo".into(),
                                "Link existing folder".into(),
                            ],
                            cursor: 0,
                            tag: SelectTag::NewRepo,
                        };
                    }
                    KeyCode::Char('R') => {
                        state.status_msg = Some("Fetching from GitHub API...".into());
                        return Ok(PhaseResult::ExecRefresh);
                    }

                    // Direct tab switching (number keys)
                    KeyCode::Char('1') => { state.tab = Tab::Overview;    state.rebuild_display(); }
                    KeyCode::Char('2') => { state.tab = Tab::Repos;       state.rebuild_display(); }
                    KeyCode::Char('3') => { state.tab = Tab::Projects;    state.rebuild_display(); }
                    KeyCode::Char('4') => { state.tab = Tab::Todos;       state.rebuild_display(); }
                    KeyCode::Char('5') => { state.tab = Tab::PRs;         state.rebuild_display(); }
                    KeyCode::Char('6') => { state.tab = Tab::Issues;      state.rebuild_display(); }
                    KeyCode::Char('7') => {
                        state.tab = Tab::Stats;
                        state.rebuild_display();
                        maybe_load_global_stats(state);
                    }
                    KeyCode::Char('8') => { state.tab = Tab::HubSettings; state.rebuild_display(); }

                    _ => {}
                }
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

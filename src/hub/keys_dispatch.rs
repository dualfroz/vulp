//! The hub's global keymap: one key event in, an optional `PhaseResult` out.
//! This only runs once `tui_phase` has ruled out a modal/confirm/filter input
//! being open — every arm here is a key that means something on the main
//! repo/project/todo/PR/issue list itself.

use super::actions::{copy_to_clipboard, get_remote_url, scan_todos_from_repos, save_bookmarks, save_pins, to_web_url};
use super::modals::open_bulk_import_picker;
use super::render::HELP_LINES;
use super::state::{
    ConfirmTag, DisplayRow, HubState, Modal, PendingAction, PhaseResult, SelectTag, Tab, TextTag,
};
use crate::todos;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handles one key event against the hub's global keymap. Returns `Some` only
/// for the handful of keys that need `tui_phase` to leave its loop (quit, open
/// a sub-view, or run something that needs the TUI torn down first) — every
/// other key is fully handled here and the caller should just keep looping.
pub fn handle_global_key(state: &mut HubState, key: KeyEvent) -> Option<PhaseResult> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if !state.filter_text.is_empty() {
                state.filter_text.clear(); state.rebuild_display();
            } else {
                state.quit = true; return Some(PhaseResult::None);
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
        // Terminals deliver Ctrl+A as lowercase 'a' + CONTROL (byte 0x01);
        // match both cases so the binding works everywhere.
        KeyCode::Char('a') | KeyCode::Char('A') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
                        state.rebuild_display();
                    } else if let Some(key) = state.cur_todo_header_key() {
                        toggle_todo_group(state, key);
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
                Tab::Stats       => Tab::Activity,
                Tab::Activity    => Tab::HubSettings,
                Tab::HubSettings => Tab::Overview,
            };
            state.cursor = 0; state.offset = 0; state.selected.clear();
            state.rebuild_display();
            super::keys::maybe_load_global_stats(state);
            super::keys::maybe_load_activity(state);
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
                Tab::Activity    => Tab::Stats,
                Tab::HubSettings => Tab::Activity,
            };
            state.cursor = 0; state.offset = 0; state.selected.clear();
            state.rebuild_display();
            super::keys::maybe_load_global_stats(state);
            super::keys::maybe_load_activity(state);
        }
        KeyCode::Enter => {
            match state.tab {
                Tab::Repos | Tab::Projects => {
                    if state.cur_repo_idx().is_some() || state.cur_proj_idx().is_some() {
                        state.open_detail = true;
                        return Some(PhaseResult::OpenDetail);
                    }
                }
                Tab::PRs => {
                    let di = state.cur_di();
                    if let Some(di) = di {
                        if let DisplayRow::Pr { pr_idx } = &state.display[di] {
                            let url = state.prs[*pr_idx].url.clone();
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
                        if let DisplayRow::Issue { issue_idx } = &state.display[di] {
                            let url = state.issues[*issue_idx].url.clone();
                            if !url.is_empty() {
                                crate::util::open_path(&url);
                                state.status_msg = Some("Opening issue in browser...".into());
                            }
                        }
                    }
                }
                Tab::Todos => {
                    if let Some(key) = state.cur_todo_header_key() {
                        toggle_todo_group(state, key);
                    }
                }
                Tab::Overview | Tab::Stats | Tab::Activity => {}
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
                            let mut opts = crate::util::detect_editors();
                            opts.push("Custom...".into());
                            let cursor = opts.iter().position(|o| o == &state.editor).unwrap_or(0);
                            state.modal = Modal::Select {
                                title: "Editor".into(),
                                options: opts,
                                cursor,
                                tag: SelectTag::SetEditor,
                            };
                        }
                        2 => {
                            let mut opts = crate::util::detect_clipboard_tools();
                            opts.push("Custom...".into());
                            let cursor = opts.iter().position(|o| o == &state.clipboard_cmd).unwrap_or(0);
                            state.modal = Modal::Select {
                                title: "Clipboard Command".into(),
                                options: opts,
                                cursor,
                                tag: SelectTag::SetClipboard,
                            };
                        }
                        7 => {
                            if state.accounts.is_empty() {
                                state.status_msg = Some("No accounts — run: vulp auth import".into());
                            } else if state.accounts.len() == 1 {
                                open_bulk_import_picker(state, 0);
                            } else {
                                let opts: Vec<String> = state.accounts.iter()
                                    .map(|a| format!("@{} ({})", a.username, a.provider))
                                    .collect();
                                state.modal = Modal::Select {
                                    title: "Bulk Import — Account".into(),
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
        KeyCode::Char('s') if state.tab == Tab::Todos => {
            state.todo_status_view = state.todo_status_view.next();
            state.rebuild_display();
        }
        KeyCode::Char('g') if state.tab == Tab::Todos => {
            state.todo_group_by = state.todo_group_by.next();
            state.rebuild_display();
        }
        KeyCode::Char('c') if state.tab == Tab::Todos => {
            state.show_code_todos = !state.show_code_todos;
            state.rebuild_display();
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
            super::keys::maybe_load_global_stats(state);
        }
        KeyCode::Char('<') if state.tab == Tab::Stats => {
            state.global_stats        = None;
            state.global_stats_rx     = None;
            state.global_stats_period = state.global_stats_period.prev();
            super::keys::maybe_load_global_stats(state);
        }
        KeyCode::Char('d') if state.tab == Tab::Stats => {
            super::modals_spawn::spawn_digest_preview(state);
        }
        KeyCode::Char('v') if state.tab == Tab::Overview => {
            if let Some(Ok(latest)) = &state.latest_release {
                if !latest.html_url.is_empty() {
                    crate::util::open_path(&latest.html_url);
                    state.status_msg = Some("Opening release notes in browser...".into());
                }
            }
        }
        KeyCode::Char('c') if state.tab == Tab::Activity => {
            use crate::activity::ActivityKind;
            let cycle = ActivityKind::cycle();
            state.activity_filter = match state.activity_filter {
                None => Some(cycle[0]),
                Some(k) => {
                    let idx = cycle.iter().position(|&c| c == k).unwrap_or(0);
                    if idx + 1 < cycle.len() { Some(cycle[idx + 1]) } else { None }
                }
            };
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
        KeyCode::Char('M') if state.tab == Tab::HubSettings => {
            let profiles = crate::profile::list_profiles(&state.cfg);
            let profile_count = profiles.len();
            let mut opts: Vec<String> = profiles.into_iter().map(|n| format!("Switch to '{}'", n)).collect();
            opts.push("+ Save current settings as profile".into());
            opts.push("Copy current settings as JSON".into());
            opts.push("Import settings from JSON (paste)".into());
            opts.push(format!("Push settings to '{}' (private repo)", crate::profile::SETTINGS_REPO_NAME));
            opts.push(format!("Pull settings from '{}' (private repo)", crate::profile::SETTINGS_REPO_NAME));
            state.modal = Modal::Select {
                title: "Profiles & Sync".into(),
                options: opts,
                cursor: 0,
                tag: SelectTag::ProfileMenu { profile_count },
            };
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
            super::modals_spawn::spawn_api_refresh(state);
        }
        KeyCode::Char('m') => super::modals_spawn::open_action_menu(state),

        // Direct tab switching (number keys)
        KeyCode::Char('1') => {
            state.tab = Tab::Overview;
            state.rebuild_display();
            super::keys::maybe_load_global_stats(state);
        }
        KeyCode::Char('2') => { state.tab = Tab::Repos;       state.rebuild_display(); }
        KeyCode::Char('3') => { state.tab = Tab::Projects;    state.rebuild_display(); }
        KeyCode::Char('4') => { state.tab = Tab::Todos;       state.rebuild_display(); }
        KeyCode::Char('5') => { state.tab = Tab::PRs;         state.rebuild_display(); }
        KeyCode::Char('6') => { state.tab = Tab::Issues;      state.rebuild_display(); }
        KeyCode::Char('7') => {
            state.tab = Tab::Stats;
            state.rebuild_display();
            super::keys::maybe_load_global_stats(state);
        }
        KeyCode::Char('8') => {
            state.tab = Tab::Activity;
            state.rebuild_display();
            super::keys::maybe_load_activity(state);
        }
        KeyCode::Char('9') => { state.tab = Tab::HubSettings; state.rebuild_display(); }

        _ => {}
    }
    None
}

/// Collapses/expands one Todos-tab group header.
fn toggle_todo_group(state: &mut HubState, key: String) {
    if !state.todo_group_collapsed.remove(&key) {
        state.todo_group_collapsed.insert(key);
    }
    state.rebuild_display();
}

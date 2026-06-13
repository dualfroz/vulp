use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseEventKind};
use super::actions::{copy_to_clipboard, get_remote_url, scan_todos_from_repos, save_bookmarks, save_pins, to_web_url};
use super::modals::{handle_modal_key, process_modal_confirm};
use super::render::{render_hub, HELP_LINES};
use super::state::{
    ConfirmTag, DisplayRow, HubState, Modal, ModalAction, PendingAction,
    PhaseResult, SelectTag, Tab, TextTag,
};
use crate::todos;

pub fn tui_phase(state: &mut HubState) -> Result<PhaseResult> {
    let mut terminal = crate::tui::setup()?;
    let result = (|| -> Result<PhaseResult> {
        loop {
            state.poll_incoming();
            state.poll_diff();
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
                        KeyCode::Esc => {
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
                    KeyCode::Up   | KeyCode::Char('k') => state.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => state.scroll_down(),
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
                            Tab::Repos    => Tab::Projects,
                            Tab::Projects => Tab::Todos,
                            Tab::Todos    => Tab::PRs,
                            Tab::PRs      => Tab::Issues,
                            Tab::Issues   => Tab::Repos,
                        };
                        state.cursor = 0; state.offset = 0; state.selected.clear();
                        state.rebuild_display();
                    }
                    KeyCode::Left => {
                        state.tab = match state.tab {
                            Tab::Repos    => Tab::Issues,
                            Tab::Projects => Tab::Repos,
                            Tab::Todos    => Tab::Projects,
                            Tab::PRs      => Tab::Todos,
                            Tab::Issues   => Tab::PRs,
                        };
                        state.cursor = 0; state.offset = 0; state.selected.clear();
                        state.rebuild_display();
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
                                            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
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
                                            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                                            state.status_msg = Some("Opening issue in browser...".into());
                                        }
                                    }
                                }
                            }
                            Tab::Todos => {}
                        }
                    }
                    KeyCode::Char('s') => {
                        if !state.action_targets().is_empty() { state.confirming = Some(PendingAction::Sync); }
                    }
                    KeyCode::Char('r') => {
                        if !state.action_targets().is_empty() { state.confirming = Some(PendingAction::Release); }
                    }
                    KeyCode::Char('n') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let cur = state.repos[ri].name.clone();
                            state.modal = Modal::Text { title: "Set label".into(), prompt: "Display name (blank = reset):".into(), value: cur, tag: TextTag::Label(ri) };
                        }
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
                        state.modal = Modal::Text { title: "New todo — Title".into(), prompt: "Title:".into(), value: String::new(), tag: TextTag::TodoTitle };
                    }
                    KeyCode::Char('e') => {
                        if let Some(ti) = state.cur_todo_idx() {
                            let cur = state.todos[ti].title.clone();
                            state.modal = Modal::Text { title: "Edit todo — Title".into(), prompt: "Title:".into(), value: cur, tag: TextTag::TodoEditTitle(ti) };
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
                        state.sort_mode = state.sort_mode.next();
                        state.rebuild_display();
                    }
                    KeyCode::Char('o') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            let editor = state.editor.clone();
                            let _ = std::process::Command::new(&editor).arg(&path).spawn();
                            state.status_msg = Some(format!("Opened in {}", editor));
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
                    KeyCode::Char('u') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            let cmd  = state.clipboard_cmd.clone();
                            if let Some(url) = get_remote_url(&path) {
                                if copy_to_clipboard(&url, &cmd) {
                                    state.status_msg = Some("URL copied!".into());
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
                            let editor = state.editor.clone();
                            let _ = std::process::Command::new(&editor).arg(&gitignore).spawn();
                            state.status_msg = Some("Opening .gitignore...".into());
                        }
                    }
                    KeyCode::Char('O') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                            state.status_msg = Some("Opening in file manager...".into());
                        }
                    }
                    KeyCode::Char('W') => {
                        if let Some(ri) = state.cur_repo_idx() {
                            let path = state.repos[ri].path.clone();
                            if let Some(url) = get_remote_url(&path) {
                                let web = to_web_url(&url);
                                let _ = std::process::Command::new("xdg-open").arg(&web).spawn();
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
                    KeyCode::Char('C') => {
                        let prompt = format!("Git URL (clones into {}):", state.workspace_root.display());
                        state.modal = Modal::Text {
                            title: "Clone repository".into(),
                            prompt,
                            value: String::new(),
                            tag: TextTag::CloneUrl,
                        };
                    }
                    KeyCode::Char('I') => {
                        let prompt = format!("Folder name (created in {}):", state.workspace_root.display());
                        state.modal = Modal::Text {
                            title: "Init new repository".into(),
                            prompt,
                            value: String::new(),
                            tag: TextTag::InitName,
                        };
                    }
                    KeyCode::Char('L') => {
                        state.modal = Modal::Text {
                            title: "Link existing folder".into(),
                            prompt: "Absolute path to folder:".into(),
                            value: String::new(),
                            tag: TextTag::LinkPath,
                        };
                    }
                    KeyCode::Char('R') => {
                        state.status_msg = Some("Fetching from GitHub API...".into());
                        return Ok(PhaseResult::ExecRefresh);
                    }

                    _ => {}
                }
            }
        }
    })();
    crate::tui::teardown(terminal)?;
    result
}

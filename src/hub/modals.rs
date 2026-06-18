use crate::projects;
use crate::todos::{self, Todo, TodoStatus};
use crossterm::event::KeyCode;

fn expand_tilde(s: &str) -> std::path::PathBuf {
    let s = s.trim();
    if s == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(s))
    } else if s.starts_with("~/") {
        dirs::home_dir().map(|h| h.join(&s[2..])).unwrap_or_else(|| std::path::PathBuf::from(s))
    } else {
        std::path::PathBuf::from(s)
    }
}
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use super::state::{
    ConfirmTag, HubState, Modal, ModalAction, MultiTag,
    ProjAction, SelectTag, TextTag,
};
use super::actions::save_templates;

pub fn render_modal(f: &mut Frame, modal: &Modal, area: Rect) {
    match modal {
        Modal::Text { title, prompt, value, max_len, .. } => {
            let rect = centered_rect(60, 8, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL).style(Style::default().bg(Color::Reset));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let counter_line = if let Some(max) = max_len {
                let n = value.len();
                let style = if n >= *max { Style::default().fg(Color::Red) }
                    else if n * 10 >= max * 7 { Style::default().fg(Color::Yellow) }
                    else { Style::default().fg(Color::DarkGray) };
                Line::from(Span::styled(format!("  ({}/{})", n, max), style))
            } else {
                Line::from("")
            };
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", prompt), Style::default().fg(Color::White))),
                counter_line,
                Line::from(vec![Span::styled("  > ", Style::default().fg(Color::Cyan)), Span::raw(value.clone()), Span::styled("█", Style::default().fg(Color::Cyan))]),
                Line::from(""),
                Line::from(Span::styled("  Enter = confirm   Esc = cancel", Style::default().fg(Color::DarkGray))),
            ];
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Select { title, options, cursor, .. } => {
            let h = (options.len() + 5).min(20) as u16;
            let rect = centered_rect(60, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let items: Vec<ListItem> = options.iter().enumerate().map(|(i, opt)| {
                let on = i == *cursor;
                ListItem::new(Line::from(vec![
                    Span::styled(if on { " ▶ " } else { "   " }, Style::default().fg(Color::Cyan)),
                    Span::styled(opt.clone(), if on { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default() }),
                ])).style(if on { Style::default().bg(Color::DarkGray) } else { Style::default() })
            }).collect();
            f.render_widget(List::new(items).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Multi { title, options, checks, cursor, .. } => {
            let h = (options.len() + 5).min(24) as u16;
            let rect = centered_rect(70, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let items: Vec<ListItem> = options.iter().enumerate().map(|(i, opt)| {
                let on      = i == *cursor;
                let checked = checks[i];
                ListItem::new(Line::from(vec![
                    Span::styled(if on { " ▶ " } else { "   " }, Style::default().fg(Color::Cyan)),
                    Span::styled(if checked { "[✓] " } else { "[ ] " }, if checked { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) }),
                    Span::styled(opt.clone(), if on { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default() }),
                ])).style(if on { Style::default().bg(Color::DarkGray) } else { Style::default() })
            }).collect();
            let hint = ListItem::new(Line::from(Span::styled("  Space = toggle  Enter = confirm  Esc = cancel", Style::default().fg(Color::DarkGray))));
            let mut all_items = items;
            all_items.push(hint);
            f.render_widget(List::new(all_items).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Confirm { message, .. } => {
            let rect = centered_rect(50, 6, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(" Confirm ").borders(Borders::ALL).style(Style::default().bg(Color::Black));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", message), Style::default().fg(Color::White))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  [y] Yes  ", Style::default().fg(Color::Green)),
                    Span::styled("[n/Esc] No", Style::default().fg(Color::Red)),
                ]),
            ];
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Info { title, lines, scroll } => {
            let h = (lines.len() + 4).min(30) as u16;
            let rect = centered_rect(70, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let avail  = inner.height.saturating_sub(1) as usize;
            let items: Vec<ListItem> = lines.iter().skip(*scroll).take(avail)
                .map(|l| ListItem::new(Line::from(Span::styled(format!("  {}", l), Style::default()))))
                .collect();
            let hint = ListItem::new(Line::from(Span::styled("  [↑↓] scroll  [Esc/q] close", Style::default().fg(Color::DarkGray))));
            let mut all = items;
            all.push(hint);
            f.render_widget(List::new(all).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::None => {}
    }
}

// ── Project color palette ─────────────────────────────────────────────────────

const PALETTE: &[(&str, &str)] = &[
    ("#808080", "Gray    (default)"),
    ("#3b82f6", "Blue"),
    ("#22c55e", "Green"),
    ("#ef4444", "Red"),
    ("#eab308", "Yellow"),
    ("#a855f7", "Purple"),
    ("#f97316", "Orange"),
    ("#ec4899", "Pink"),
    ("#06b6d4", "Cyan"),
    ("#14b8a6", "Teal"),
    ("#f43f5e", "Rose"),
];

pub fn color_palette() -> Vec<String> {
    PALETTE.iter().map(|(hex, name)| format!("{}  {}", hex, name)).collect()
}

pub fn palette_hex(cursor: usize) -> String {
    PALETTE.get(cursor).map(|(h, _)| h.to_string()).unwrap_or_else(|| "#808080".into())
}

pub fn palette_cursor_for(hex: &str) -> usize {
    PALETTE.iter().position(|(h, _)| *h == hex).unwrap_or(0)
}

pub fn centered_rect(pct_x: u16, height: u16, area: Rect) -> Rect {
    let w = area.width * pct_x / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}

pub fn handle_modal_key(modal: &mut Modal, code: KeyCode) -> ModalAction {
    match modal {
        Modal::Text { value, max_len, .. } => match code {
            KeyCode::Char(c) => {
                if max_len.map_or(true, |m| value.len() < m) { value.push(c); }
                ModalAction::Pending
            }
            KeyCode::Backspace => { value.pop(); ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Select { options, cursor, .. } => match code {
            KeyCode::Up   | KeyCode::Char('k') => { *cursor = cursor.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => { if *cursor + 1 < options.len() { *cursor += 1; } ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Multi { options, checks, cursor, .. } => match code {
            KeyCode::Up   | KeyCode::Char('k') => { *cursor = cursor.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => { if *cursor + 1 < options.len() { *cursor += 1; } ModalAction::Pending }
            KeyCode::Char(' ') => { let c = *cursor; checks[c] = !checks[c]; ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Confirm { .. } => match code {
            KeyCode::Char('y') | KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Char('n') | KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Info { lines, scroll, .. } => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => ModalAction::Cancel,
            KeyCode::Up   | KeyCode::Char('k') => { *scroll = scroll.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => {
                if *scroll + 1 < lines.len() { *scroll += 1; }
                ModalAction::Pending
            }
            _ => ModalAction::Pending,
        },
        Modal::None => ModalAction::Cancel,
    }
}

pub fn process_modal_confirm(state: &mut HubState) {
    let modal = std::mem::replace(&mut state.modal, Modal::None);
    match modal {
        Modal::Text { value, tag: TextTag::Label(ri), .. } => {
            let display = if value.trim().is_empty() {
                state.repos[ri].path.file_name().unwrap_or_default().to_string_lossy().to_string()
            } else { value.clone() };
            state.repos[ri].name = display.clone();
            let _ = crate::labels::save_label(&state.repos[ri].path.clone(), &value, &state.workspace_root);
            state.rebuild_display();
        }

        Modal::Text { value, tag: TextTag::ProjName, .. } => {
            if !value.trim().is_empty() {
                state.modal = Modal::Select {
                    title: "Create project — Color".into(),
                    options: color_palette(),
                    cursor: 0,
                    tag: SelectTag::ProjColorCreate(value),
                };
            }
        }
        Modal::Select { cursor, tag: SelectTag::ProjColorCreate(name), .. } => {
            let color = palette_hex(cursor);
            state.projects.push(crate::projects::Project { name, color, repos: vec![] });
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }

        Modal::Text { value, tag: TextTag::ProjRenameName(pi), .. } => {
            let cur_cursor = palette_cursor_for(&state.projects[pi].color);
            state.modal = Modal::Select {
                title: format!("Rename '{}' — Color", state.projects[pi].name),
                options: color_palette(),
                cursor: cur_cursor,
                tag: SelectTag::ProjColorRename(pi, value),
            };
        }
        Modal::Select { cursor, tag: SelectTag::ProjColorRename(pi, name), .. } => {
            let color = palette_hex(cursor);
            state.projects[pi].name  = name;
            state.projects[pi].color = color;
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }

        Modal::Select { cursor, tag: SelectTag::ProjMenu, .. } => {
            if state.projects.is_empty() && cursor == 0 {
                state.modal = Modal::Text { title: "Create project — Name".into(), prompt: "Project name:".into(), value: String::new(), tag: TextTag::ProjName, max_len: Some(50) };
                return;
            }
            match cursor {
                0 => {
                    state.modal = Modal::Text { title: "Create project — Name".into(), prompt: "Project name:".into(), value: String::new(), tag: TextTag::ProjName, max_len: Some(50) };
                }
                1 => {
                    let names: Vec<String> = state.projects.iter().map(|p| p.name.clone()).collect();
                    state.modal = Modal::Select { title: "Assign repos to…".into(), options: names, cursor: 0, tag: SelectTag::ProjForAction(ProjAction::Assign) };
                }
                2 => {
                    let names: Vec<String> = state.projects.iter().map(|p| p.name.clone()).collect();
                    state.modal = Modal::Select { title: "Rename…".into(), options: names, cursor: 0, tag: SelectTag::ProjForAction(ProjAction::Rename) };
                }
                3 => {
                    let names: Vec<String> = state.projects.iter().map(|p| p.name.clone()).collect();
                    state.modal = Modal::Select { title: "Delete…".into(), options: names, cursor: 0, tag: SelectTag::ProjForAction(ProjAction::Delete) };
                }
                _ => {}
            }
        }
        Modal::Select { cursor, tag: SelectTag::ProjForAction(action), .. } => {
            let pi = cursor;
            match action {
                ProjAction::Assign => {
                    let rel_paths: Vec<String> = state.repos.iter()
                        .map(|r| projects::rel_key(&r.path, &state.workspace_root))
                        .collect();
                    let options: Vec<String> = state.repos.iter().zip(&rel_paths)
                        .map(|(r, rel)| format!("{:<18}  {}", r.name, rel))
                        .collect();
                    let checks: Vec<bool> = rel_paths.iter()
                        .map(|rel| state.projects[pi].repos.contains(rel))
                        .collect();
                    state.modal = Modal::Multi { title: format!("Assign repos to '{}'", state.projects[pi].name), options, checks, cursor: 0, tag: MultiTag::ProjAssign(pi) };
                }
                ProjAction::Rename => {
                    let cur_name = state.projects[pi].name.clone();
                    state.modal = Modal::Text { title: "Rename project".into(), prompt: "New name:".into(), value: cur_name, tag: TextTag::ProjRenameName(pi), max_len: Some(50) };
                }
                ProjAction::Delete => {
                    let name = state.projects[pi].name.clone();
                    state.modal = Modal::Confirm { message: format!("Delete project '{}'?", name), tag: ConfirmTag::DeleteProject(pi) };
                }
            }
        }
        Modal::Multi { checks, tag: MultiTag::ProjAssign(pi), .. } => {
            let rel_paths: Vec<String> = state.repos.iter()
                .map(|r| projects::rel_key(&r.path, &state.workspace_root))
                .collect();
            state.projects[pi].repos = rel_paths.into_iter().zip(checks.iter()).filter(|(_, &c)| c).map(|(p, _)| p).collect();
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }
        Modal::Confirm { tag: ConfirmTag::DeleteProject(pi), .. } => {
            state.projects.remove(pi);
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }

        Modal::Text { value, tag: TextTag::TodoTitle, .. } => {
            if !value.trim().is_empty() {
                state.modal = Modal::Text { title: "New todo — Description".into(), prompt: "Description (optional):".into(), value: String::new(), tag: TextTag::TodoDesc(value), max_len: None };
            }
        }
        Modal::Text { value: desc, tag: TextTag::TodoDesc(title), .. } => {
            let options = TodoStatus::all_labels().iter().map(|s| s.to_string()).collect();
            state.modal = Modal::Select { title: "New todo — Status".into(), options, cursor: 0, tag: SelectTag::TodoStatus { title, desc } };
        }
        Modal::Select { cursor, tag: SelectTag::TodoStatus { title, desc }, .. } => {
            let mut opts: Vec<String> = state.projects.iter().map(|p| p.name.clone()).collect();
            opts.insert(0, "— no project —".into());
            state.modal = Modal::Select { title: "New todo — Project".into(), options: opts, cursor: 0, tag: SelectTag::TodoProject { title, desc, status_idx: cursor } };
        }
        Modal::Select { cursor, tag: SelectTag::TodoProject { title, desc, status_idx }, .. } => {
            let project = if cursor == 0 { None } else { Some(state.projects[cursor - 1].name.clone()) };
            let id  = todos::next_id(&state.todos);
            let todo = Todo::new(id, title, desc, TodoStatus::from_idx(status_idx), project);
            state.todos.push(todo);
            let _ = todos::save(&state.todos);
            state.rebuild_display();
        }

        Modal::Text { value, tag: TextTag::TodoEditTitle(ti), .. } => {
            state.todos[ti].title = value.clone();
            let cur_desc = state.todos[ti].description.clone();
            state.todos[ti].touch();
            state.modal = Modal::Text { title: "Edit todo — Description".into(), prompt: "Description:".into(), value: cur_desc, tag: TextTag::TodoEditDesc(ti, value), max_len: None };
        }
        Modal::Text { value: desc, tag: TextTag::TodoEditDesc(ti, _), .. } => {
            state.todos[ti].description = desc;
            state.todos[ti].touch();
            let _ = todos::save(&state.todos);
            state.rebuild_display();
        }

        Modal::Select { cursor, tag: SelectTag::TodoChangeStatus(ti), .. } => {
            state.todos[ti].status = TodoStatus::from_idx(cursor);
            state.todos[ti].touch();
            let _ = todos::save(&state.todos);
        }

        Modal::Select { cursor, tag: SelectTag::SortFilter, .. } => {
            use super::state::SortMode;
            match cursor {
                0 => state.sort_mode = SortMode::Default,
                1 => state.sort_mode = SortMode::Name,
                2 => state.sort_mode = SortMode::Age,
                3 => state.sort_mode = SortMode::DirtyFirst,
                4 => state.sort_mode = SortMode::BehindFirst,
                5 => state.filter_dirty    = !state.filter_dirty,
                6 => state.filter_behind   = !state.filter_behind,
                7 => state.filter_ahead    = !state.filter_ahead,
                8 => state.filter_archived = !state.filter_archived,
                9 => {
                    state.sort_mode      = SortMode::Default;
                    state.filter_dirty   = false;
                    state.filter_behind  = false;
                    state.filter_ahead   = false;
                    state.filter_archived = false;
                }
                _ => {}
            }
            state.rebuild_display();
        }

        Modal::Select { cursor, tag: SelectTag::NewRepo, .. } => {
            match cursor {
                0 => {
                    let prompt = format!("Git URL (clones into {}):", state.workspace_root.display());
                    state.modal = Modal::Text { title: "Clone from URL".into(), prompt, value: String::new(), tag: TextTag::CloneUrl, max_len: None };
                }
                1 => {
                    state.modal = Modal::Text {
                        title: "Link local folder".into(),
                        prompt: "Absolute path to folder (~ supported):".into(),
                        value: String::new(),
                        tag: TextTag::LinkPath,
                        max_len: None,
                    };
                }
                2 => {
                    let prompt = format!("Folder name (in {}):", state.workspace_root.display());
                    state.modal = Modal::Text { title: "Create new repo".into(), prompt, value: String::new(), tag: TextTag::InitName, max_len: Some(60) };
                }
                3 => {
                    state.modal = Modal::Text { title: "Link existing folder".into(), prompt: "Absolute path:".into(), value: String::new(), tag: TextTag::LinkPath, max_len: None };
                }
                _ => {}
            }
        }

        Modal::Select { cursor, tag: SelectTag::SelectByCondition, .. } => {
            let repos = &state.repos;
            let candidates: Vec<usize> = (0..repos.len()).filter(|&ri| state.passes_filter(ri)).collect();
            state.selected.clear();
            match cursor {
                0 => { for ri in candidates { state.selected.insert(ri); } }
                1 => { for ri in candidates { if repos[ri].is_dirty      { state.selected.insert(ri); } } }
                2 => { for ri in candidates { if repos[ri].behind > 0    { state.selected.insert(ri); } } }
                3 => { for ri in candidates { if repos[ri].ahead  > 0    { state.selected.insert(ri); } } }
                4 => { for ri in candidates { if !repos[ri].license      { state.selected.insert(ri); } } }
                5 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Rust")       { state.selected.insert(ri); } } }
                6 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("TypeScript") { state.selected.insert(ri); } } }
                7 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Python")     { state.selected.insert(ri); } } }
                8 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Go")         { state.selected.insert(ri); } } }
                _ => {}
            }
            state.status_msg = Some(format!("Selected {} repos", state.selected.len()));
        }

        Modal::Confirm { tag: ConfirmTag::DeleteTodo(ti), .. } => {
            state.todos.remove(ti);
            let _ = todos::save(&state.todos);
            state.rebuild_display();
        }

        // ── Hub sync via in-TUI modals ────────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::SyncAction, .. } => {
            use super::state::PhaseResult;
            match cursor {
                0 => { // Pull
                    state.pending_sync_action = Some("pull".into());
                    state.pending_phase = Some(PhaseResult::ExecSync);
                }
                1 => { // Commit+Push
                    state.pending_sync_action = Some("commit_push".into());
                    state.modal = Modal::Text {
                        title: "Sync — Commit Message".into(),
                        prompt: "Message (stages all changes):".into(),
                        value: "chore: update".into(),
                        tag: TextTag::SyncMsg,
                        max_len: None,
                    };
                }
                _ => { // Push only
                    state.pending_sync_action = Some("push_only".into());
                    state.pending_phase = Some(PhaseResult::ExecSync);
                }
            }
        }
        Modal::Text { value, tag: TextTag::SyncMsg, .. } => {
            use super::state::PhaseResult;
            state.pending_sync_msg = Some(value);
            state.pending_phase    = Some(PhaseResult::ExecSync);
        }

        // ── Hub Settings editing ──────────────────────────────────────────────
        Modal::Text { value, tag: TextTag::SetWorkspace, .. } => {
            let path = expand_tilde(&value);
            if !path.exists() {
                state.status_msg = Some(format!("Directory does not exist: {}", path.display()));
                return;
            }
            if !path.is_dir() {
                state.status_msg = Some("Not a directory".into());
                return;
            }
            state.workspace_root = path;
            super::actions::refresh_repos(state);
            state.status_msg = Some(format!(
                "Workspace changed to {}  (to persist: edit ~/.fxg/config.toml)",
                state.workspace_root.display(),
            ));
        }
        Modal::Text { value, tag: TextTag::SetEditor, .. } => {
            if !value.trim().is_empty() {
                state.editor = value.trim().to_string();
                state.status_msg = Some(format!("Editor set to: {}", state.editor));
            }
        }
        Modal::Text { value, tag: TextTag::SetClipboard, .. } => {
            state.clipboard_cmd = value.trim().to_string();
            state.status_msg = Some("Clipboard command updated".into());
        }

        // ── Pull-all repos ────────────────────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::PullAllAccount, .. } => {
            let acc_name = state.accounts.get(cursor).map(|a| format!("@{} ({})", a.username, a.provider)).unwrap_or_default();
            state.modal = Modal::Confirm {
                message: format!("Pull all repos for {}?", acc_name),
                tag: ConfirmTag::PullAll(cursor),
            };
        }
        Modal::Confirm { tag: ConfirmTag::PullAll(acc_idx), .. } => {
            if let Some(acc) = state.accounts.get(acc_idx) {
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                let workspace = state.workspace_root.clone();
                let provider  = acc.provider.clone();
                let username  = acc.username.clone();
                state.pull_all_rx = Some(rx);
                state.status_msg  = Some(format!("Fetching repos for @{}...", username));
                std::thread::spawn(move || {
                    let out = std::process::Command::new("gh")
                        .args(["api", "user/repos", "--paginate", "--jq", ".[].clone_url"])
                        .output();
                    let Ok(o) = out else {
                        let _ = tx.send("ERROR: gh CLI failed — is it installed and authenticated?".into());
                        return;
                    };
                    let urls: Vec<String> = String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    let total = urls.len();
                    let _ = tx.send(format!("Found {} repos for @{} ({})", total, username, provider));
                    for (i, url) in urls.iter().enumerate() {
                        let repo_name = url.rsplitn(2, '/').next().unwrap_or("").trim_end_matches(".git");
                        let target = workspace.join(repo_name);
                        let action = if target.exists() {
                            let _ = std::process::Command::new("git")
                                .args(["pull"]).current_dir(&target).status();
                            "pulled"
                        } else {
                            let _ = std::process::Command::new("git")
                                .args(["clone", url.as_str()]).current_dir(&workspace).status();
                            "cloned"
                        };
                        let _ = tx.send(format!("[{}/{}] {} {}", i + 1, total, action, repo_name));
                    }
                    let _ = tx.send("DONE".into());
                });
            }
        }

        Modal::Text { value, tag: TextTag::CloneUrl, .. } => {
            if !value.trim().is_empty() {
                state.pending_clone = Some(value.trim().to_string());
            }
        }

        Modal::Text { value, tag: TextTag::InitName, .. } => {
            let name = value.trim().to_string();
            if name.is_empty() { return; }
            let mut opts = vec!["Blank (no template)".to_string()];
            for t in &state.templates {
                let n = t.commands.len();
                opts.push(format!("{:<20} ({} cmd{})", t.name, n, if n == 1 { "" } else { "s" }));
            }
            state.modal = Modal::Select {
                title: format!("Create '{}' — Template", name),
                options: opts,
                cursor: 0,
                tag: SelectTag::InitTemplate(name),
            };
        }

        // ── Repo creation with template ───────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::InitTemplate(repo_name), .. } => {
            use super::state::PhaseResult;
            let template = if cursor == 0 { None } else { state.templates.get(cursor - 1).cloned() };
            let target = state.workspace_root.join(&repo_name);
            if let Err(e) = std::fs::create_dir_all(&target) {
                state.status_msg = Some(format!("mkdir failed: {e}"));
                return;
            }
            let git_ok = std::process::Command::new("git")
                .arg("init").current_dir(&target).output()
                .map(|o| o.status.success()).unwrap_or(false);
            if !git_ok {
                state.status_msg = Some("git init failed".into());
                return;
            }
            // create .fxg/meta.toml
            let fxg_dir = target.join(".fxg");
            let _ = std::fs::create_dir_all(&fxg_dir);
            let tmpl_name = template.as_ref().map(|t| t.name.as_str()).unwrap_or("blank");
            let meta = format!("created_at = \"{}\"\ntemplate = \"{}\"\n",
                chrono::Utc::now().format("%Y-%m-%d"), tmpl_name);
            let _ = std::fs::write(fxg_dir.join("meta.toml"), meta);

            if let Some(t) = &template {
                if !t.commands.is_empty() {
                    state.pending_phase = Some(PhaseResult::ExecTemplateInit {
                        path: target.clone(),
                        commands: t.commands.clone(),
                    });
                }
            }
            super::actions::refresh_repos(state);
            state.status_msg = Some(format!("Created '{}'{}",
                repo_name,
                template.as_ref().map(|t| format!(" using '{}'", t.name)).unwrap_or_default(),
            ));
        }

        // ── Import directory as repos ─────────────────────────────────────────
        Modal::Text { value, tag: TextTag::ImportDir, .. } => {
            let src = expand_tilde(&value);
            if !src.exists() {
                state.status_msg = Some(format!("Path does not exist: {}", src.display()));
                return;
            }
            let workspace = state.workspace_root.clone();
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            state.pull_all_rx = Some(rx);
            state.status_msg  = Some(format!("Importing from {}...", src.display()));
            std::thread::spawn(move || {
                let Ok(entries) = std::fs::read_dir(&src) else {
                    let _ = tx.send("ERROR: Cannot read directory".into());
                    let _ = tx.send("DONE".into());
                    return;
                };
                let mut linked = 0usize;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() { continue; }
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if name.starts_with('.') { continue; }
                    if !path.join(".git").exists() {
                        let _ = std::process::Command::new("git")
                            .arg("init").current_dir(&path).status();
                        let _ = tx.send(format!("git init: {}", name));
                    }
                    let link = workspace.join(&name);
                    if !link.exists() {
                        if std::os::unix::fs::symlink(&path, &link).is_ok() {
                            linked += 1;
                            let _ = tx.send(format!("linked: {}", name));
                        }
                    } else {
                        let _ = tx.send(format!("skip (exists): {}", name));
                    }
                }
                let _ = tx.send(format!("Done: {} new repos linked", linked));
                let _ = tx.send("DONE".into());
            });
        }

        // ── Template management ───────────────────────────────────────────────
        Modal::Text { value, tag: TextTag::NewTemplateName, .. } => {
            let name = value.trim().to_string();
            if !name.is_empty() {
                state.modal = Modal::Text {
                    title: format!("Template '{}' — Commands", name),
                    prompt: "Commands (semicolon-separated, e.g. cargo init; echo ok):".into(),
                    value: String::new(),
                    tag: TextTag::NewTemplateCmds(name),
                    max_len: None,
                };
            }
        }
        Modal::Text { value, tag: TextTag::NewTemplateCmds(name), .. } => {
            let commands: Vec<String> = value.split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let t = super::state::RepoTemplate { name: name.clone(), description: String::new(), commands };
            state.templates.push(t);
            save_templates(&state.templates);
            state.status_msg = Some(format!("Template '{}' saved", name));
        }
        Modal::Select { cursor, tag: SelectTag::TemplateMenu(ti), .. } => {
            if cursor == 1 { // Delete
                let name = state.templates[ti].name.clone();
                state.modal = Modal::Confirm {
                    message: format!("Delete template '{}'?", name),
                    tag: ConfirmTag::DeleteTemplate(ti),
                };
            }
            // cursor 0 is "view commands" — just dismiss
        }
        Modal::Confirm { tag: ConfirmTag::DeleteTemplate(ti), .. } => {
            let name = state.templates[ti].name.clone();
            state.templates.remove(ti);
            save_templates(&state.templates);
            let max = 10 + 1 + state.templates.len();
            if state.hub_settings_cur >= max { state.hub_settings_cur = max.saturating_sub(1); }
            state.status_msg = Some(format!("Template '{}' deleted", name));
        }

        // ── Remove .git from all workspace repos ──────────────────────────────
        Modal::Confirm { tag: ConfirmTag::RemoveGitAll, .. } => {
            let workspace = state.workspace_root.clone();
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            state.pull_all_rx = Some(rx);
            state.status_msg  = Some("Removing .git directories...".into());
            std::thread::spawn(move || {
                let Ok(entries) = std::fs::read_dir(&workspace) else {
                    let _ = tx.send("DONE".into());
                    return;
                };
                let mut count = 0usize;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() { continue; }
                    let git_dir = path.join(".git");
                    if git_dir.exists() {
                        if std::fs::remove_dir_all(&git_dir).is_ok() {
                            count += 1;
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let _ = tx.send(format!("Removed .git: {}", name));
                        }
                    }
                }
                let _ = tx.send(format!("Done: removed .git from {} dirs", count));
                let _ = tx.send("DONE".into());
            });
        }

        Modal::Text { value, tag: TextTag::LinkPath, .. } => {
            let src = expand_tilde(&value);
            if !src.exists() {
                state.status_msg = Some(format!("Path does not exist: {}", src.display()));
                return;
            }
            let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.is_empty() { state.status_msg = Some("Invalid path".into()); return; }
            let link = state.workspace_root.join(&name);
            if link.exists() {
                state.status_msg = Some(format!("{name} already exists in workspace"));
                return;
            }
            if let Err(e) = std::os::unix::fs::symlink(&src, &link) {
                state.status_msg = Some(format!("symlink failed: {e}"));
                return;
            }
            // Ensure it's a git repo
            if !src.join(".git").exists() {
                let _ = std::process::Command::new("git").arg("init").current_dir(&src).output();
            }
            super::actions::refresh_repos(state);
            state.status_msg = Some(format!("Linked {name}"));
        }

        Modal::Select { cursor, tag: SelectTag::CopyUrl(path), .. } => {
            use super::actions::{copy_to_clipboard, get_remote_url};
            use super::actions::to_web_url;
            let cmd = state.clipboard_cmd.clone();
            if let Some(raw_url) = get_remote_url(&path) {
                let url = if cursor == 0 { raw_url } else { to_web_url(&raw_url) };
                if copy_to_clipboard(&url, &cmd) {
                    let label = if cursor == 0 { "SSH URL" } else { "HTTPS URL" };
                    state.status_msg = Some(format!("{} copied!", label));
                } else {
                    state.status_msg = Some(format!("  {}", url));
                }
            } else {
                state.status_msg = Some("No remote origin".into());
            }
        }

        _ => {}
    }
}

use crate::projects;
use crate::todos::{self, Todo, TodoStatus};
use crossterm::event::KeyCode;
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

pub fn render_modal(f: &mut Frame, modal: &Modal, area: Rect) {
    match modal {
        Modal::Text { title, prompt, value, .. } => {
            let rect = centered_rect(60, 8, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL).style(Style::default().bg(Color::Reset));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", prompt), Style::default().fg(Color::White))),
                Line::from(""),
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

pub fn centered_rect(pct_x: u16, height: u16, area: Rect) -> Rect {
    let w = area.width * pct_x / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}

pub fn handle_modal_key(modal: &mut Modal, code: KeyCode) -> ModalAction {
    match modal {
        Modal::Text { value, .. } => match code {
            KeyCode::Char(c) => { value.push(c); ModalAction::Pending }
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
                state.modal = Modal::Text {
                    title: "Create project — Color".into(),
                    prompt: "Hex color (#rrggbb):".into(),
                    value: "#3498db".into(),
                    tag: TextTag::ProjColor(value),
                };
            }
        }
        Modal::Text { value: color, tag: TextTag::ProjColor(name), .. } => {
            let color = if color.starts_with('#') && color.len() == 7 { color } else { "#3498db".into() };
            state.projects.push(crate::projects::Project { name, color, repos: vec![] });
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }

        Modal::Text { value, tag: TextTag::ProjRenameName(pi), .. } => {
            let cur_color = state.projects[pi].color.clone();
            state.modal = Modal::Text {
                title: format!("Rename '{}' — Color", state.projects[pi].name),
                prompt: "New color (#rrggbb):".into(),
                value: cur_color,
                tag: TextTag::ProjRenameColor(pi, value),
            };
        }
        Modal::Text { value: color, tag: TextTag::ProjRenameColor(pi, name), .. } => {
            let color = if color.starts_with('#') && color.len() == 7 { color } else { state.projects[pi].color.clone() };
            state.projects[pi].name  = name;
            state.projects[pi].color = color;
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
        }

        Modal::Select { cursor, tag: SelectTag::ProjMenu, .. } => {
            if state.projects.is_empty() && cursor == 0 {
                state.modal = Modal::Text { title: "Create project — Name".into(), prompt: "Project name:".into(), value: String::new(), tag: TextTag::ProjName };
                return;
            }
            match cursor {
                0 => {
                    state.modal = Modal::Text { title: "Create project — Name".into(), prompt: "Project name:".into(), value: String::new(), tag: TextTag::ProjName };
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
                    state.modal = Modal::Text { title: "Rename project".into(), prompt: "New name:".into(), value: cur_name, tag: TextTag::ProjRenameName(pi) };
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
                state.modal = Modal::Text { title: "New todo — Description".into(), prompt: "Description (optional):".into(), value: String::new(), tag: TextTag::TodoDesc(value) };
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
            state.modal = Modal::Text { title: "Edit todo — Description".into(), prompt: "Description:".into(), value: cur_desc, tag: TextTag::TodoEditDesc(ti, value) };
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

        Modal::Text { value, tag: TextTag::CloneUrl, .. } => {
            if !value.trim().is_empty() {
                state.pending_clone = Some(value.trim().to_string());
            }
        }

        Modal::Text { value, tag: TextTag::InitName, .. } => {
            let name = value.trim().to_string();
            if name.is_empty() { return; }
            let target = state.workspace_root.join(&name);
            if let Err(e) = std::fs::create_dir_all(&target) {
                state.status_msg = Some(format!("mkdir failed: {e}"));
                return;
            }
            let out = std::process::Command::new("git")
                .arg("init").current_dir(&target).output();
            match out {
                Ok(o) if o.status.success() => {
                    super::actions::refresh_repos(state);
                    state.status_msg = Some(format!("Initialized {name}"));
                }
                Ok(o) => {
                    state.status_msg = Some(format!("git init failed: {}", String::from_utf8_lossy(&o.stderr).trim()));
                }
                Err(e) => { state.status_msg = Some(format!("git init error: {e}")); }
            }
        }

        Modal::Text { value, tag: TextTag::LinkPath, .. } => {
            let src = std::path::PathBuf::from(value.trim());
            if !src.exists() {
                state.status_msg = Some("Path does not exist".into());
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

        _ => {}
    }
}

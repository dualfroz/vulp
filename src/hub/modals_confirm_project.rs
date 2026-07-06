//! `process_modal_confirm` arms for the Projects and Todos features: naming/
//! labeling repos, creating/renaming/assigning/deleting projects, and the
//! multi-step "new todo" / "edit todo" modal chains.

use super::state::{ConfirmTag, HubState, Modal, ProjAction, SelectTag, TextTag};
use crate::projects;
use crate::todos::{self, Todo, TodoStatus};

/// Handles one step of a Projects/Todos modal chain. Returns `None` once
/// `modal` has been consumed by one of this file's arms, or `Some(modal)`
/// unchanged so the next file in the chain (see `modals::process_modal_confirm`)
/// can try it.
pub fn try_confirm(state: &mut HubState, modal: Modal) -> Option<Modal> {
    match modal {
        Modal::Text { value, tag: TextTag::Label(ri), .. } => {
            let display = if value.trim().is_empty() {
                state.repos[ri].path.file_name().unwrap_or_default().to_string_lossy().to_string()
            } else { value.clone() };
            state.repos[ri].name = display.clone();
            let _ = crate::labels::save_label(&state.repos[ri].path.clone(), &value, &state.workspace_root);
            state.rebuild_display();
            None
        }

        Modal::Text { value, tag: TextTag::ProjName, .. } => {
            if !value.trim().is_empty() {
                state.modal = Modal::Select {
                    title: "Create project — Color".into(),
                    options: super::modals::color_palette(),
                    cursor: 0,
                    tag: SelectTag::ProjColorCreate(value),
                };
            }
            None
        }
        Modal::Select { cursor, tag: SelectTag::ProjColorCreate(name), .. } => {
            let color = super::modals::palette_hex(cursor);
            state.projects.push(crate::projects::Project { name, color, repos: vec![] });
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
            None
        }

        Modal::Text { value, tag: TextTag::ProjRenameName(pi), .. } => {
            let cur_cursor = super::modals::palette_cursor_for(&state.projects[pi].color);
            state.modal = Modal::Select {
                title: format!("Rename '{}' — Color", state.projects[pi].name),
                options: super::modals::color_palette(),
                cursor: cur_cursor,
                tag: SelectTag::ProjColorRename(pi, value),
            };
            None
        }
        Modal::Select { cursor, tag: SelectTag::ProjColorRename(pi, name), .. } => {
            let color = super::modals::palette_hex(cursor);
            state.projects[pi].name  = name;
            state.projects[pi].color = color;
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
            None
        }

        Modal::Select { cursor, tag: SelectTag::ProjMenu, .. } => {
            if state.projects.is_empty() && cursor == 0 {
                state.modal = Modal::Text { title: "Create project — Name".into(), prompt: "Project name:".into(), value: String::new(), tag: TextTag::ProjName, max_len: Some(50) };
                return None;
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
            None
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
                    state.modal = Modal::Multi { title: format!("Assign repos to '{}'", state.projects[pi].name), options, checks, cursor: 0, tag: super::state::MultiTag::ProjAssign(pi) };
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
            None
        }
        Modal::Multi { checks, tag: super::state::MultiTag::ProjAssign(pi), .. } => {
            let rel_paths: Vec<String> = state.repos.iter()
                .map(|r| projects::rel_key(&r.path, &state.workspace_root))
                .collect();
            state.projects[pi].repos = rel_paths.into_iter().zip(checks.iter()).filter(|(_, &c)| c).map(|(p, _)| p).collect();
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
            None
        }
        Modal::Confirm { tag: ConfirmTag::DeleteProject(pi), .. } => {
            state.projects.remove(pi);
            let _ = projects::save(&state.projects);
            state.rebuild_project_map();
            state.rebuild_display();
            None
        }

        Modal::Text { value, tag: TextTag::TodoTitle, .. } => {
            if !value.trim().is_empty() {
                state.modal = Modal::Text { title: "New todo — Description".into(), prompt: "Description (optional):".into(), value: String::new(), tag: TextTag::TodoDesc(value), max_len: None };
            }
            None
        }
        Modal::Text { value: desc, tag: TextTag::TodoDesc(title), .. } => {
            let options = TodoStatus::all_labels().iter().map(|s| s.to_string()).collect();
            state.modal = Modal::Select { title: "New todo — Status".into(), options, cursor: 0, tag: SelectTag::TodoStatus { title, desc } };
            None
        }
        Modal::Select { cursor, tag: SelectTag::TodoStatus { title, desc }, .. } => {
            let mut opts: Vec<String> = state.projects.iter().map(|p| p.name.clone()).collect();
            opts.insert(0, "— no project —".into());
            state.modal = Modal::Select { title: "New todo — Project".into(), options: opts, cursor: 0, tag: SelectTag::TodoProject { title, desc, status_idx: cursor } };
            None
        }
        Modal::Select { cursor, tag: SelectTag::TodoProject { title, desc, status_idx }, .. } => {
            let project = if cursor == 0 { None } else { Some(state.projects[cursor - 1].name.clone()) };
            let id  = todos::next_id(&state.todos);
            let todo = Todo::new(id, title, desc, TodoStatus::from_idx(status_idx), project);
            state.todos.push(todo);
            let _ = todos::save(&state.todos);
            state.rebuild_display();
            state.cursor = state.display.len().saturating_sub(1);
            None
        }

        Modal::Text { value, tag: TextTag::TodoEditTitle(ti), .. } => {
            state.todos[ti].title = value.clone();
            let cur_desc = state.todos[ti].description.clone();
            state.todos[ti].touch();
            state.modal = Modal::Text { title: "Edit todo — Description".into(), prompt: "Description:".into(), value: cur_desc, tag: TextTag::TodoEditDesc(ti), max_len: None };
            None
        }
        Modal::Text { value: desc, tag: TextTag::TodoEditDesc(ti), .. } => {
            state.todos[ti].description = desc;
            state.todos[ti].touch();
            let _ = todos::save(&state.todos);
            state.rebuild_display();
            None
        }

        Modal::Confirm { tag: ConfirmTag::DeleteTodo(ti), .. } => {
            state.todos.remove(ti);
            let _ = todos::save(&state.todos);
            state.rebuild_display();
            None
        }

        other => Some(other),
    }
}

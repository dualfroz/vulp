use super::git::{copy_to_clipboard, delete_branch, git_run_ok, list_branches, to_web_url};
use super::settings_io::{open_settings_modal, spawn_settings_op};
use super::types::{DetailModal, SettingOp, St, Tab};
use super::{maybe_load_prs_issues, maybe_load_settings, rebuild_tree_visible};
use crossterm::event::KeyCode;
use git2::Repository;

pub fn validate_branch_name(name: &str) -> Option<String> {
    if name.is_empty() { return Some("Name cannot be empty".into()); }
    if name.contains(' ') { return Some("No spaces allowed".into()); }
    if name.contains("..") { return Some("Cannot contain '..'".into()); }
    if name.starts_with('/') || name.ends_with('/') { return Some("Cannot start or end with '/'".into()); }
    if name.chars().any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')) {
        return Some("Invalid character (~^:?*[\\)".into());
    }
    None
}

pub fn handle_modal_key(code: KeyCode, st: &mut St) -> bool {
    match &mut st.modal {
        DetailModal::None => false,

        DetailModal::NewBranch { input, error } => {
            match code {
                KeyCode::Esc => { st.modal = DetailModal::None; }
                KeyCode::Enter => {
                    let name = input.trim().to_string();
                    if let Some(err) = validate_branch_name(&name) {
                        *error = Some(err);
                    } else {
                        let ok = git_run_ok(&st.summary.path, &["checkout", "-b", &name]);
                        st.status_msg = Some(if ok {
                            st.summary.branch = name.clone();
                            st.branches = list_branches(&st.summary.path);
                            st.branch_count = st.branches.len();
                            st.branch_cursor = st.branches.iter().position(|b| b.is_current).unwrap_or(0);
                            format!("Created & switched to '{}'", name)
                        } else {
                            "Failed to create branch".into()
                        });
                        st.modal = DetailModal::None;
                    }
                }
                KeyCode::Backspace => { input.pop(); *error = None; }
                KeyCode::Char(c) if input.len() < 80 => { input.push(c); *error = None; }
                _ => {}
            }
            true
        }

        DetailModal::SelectOption { options, cursor, tag, .. } => {
            let max = options.len().saturating_sub(1);
            match code {
                KeyCode::Esc => { st.modal = DetailModal::None; }
                KeyCode::Up   => { *cursor = cursor.saturating_sub(1); }
                KeyCode::Down => { *cursor = (*cursor + 1).min(max); }
                KeyCode::Enter => {
                    let sel = options[*cursor].clone();
                    let tag = *tag;
                    st.modal = DetailModal::None;
                    match tag {
                        60 => {
                            let private = sel == "private";
                            let label = sel.clone();
                            if let Some(s) = &mut st.settings { s.visibility = label.clone(); }
                            st.summary.visibility = Some(label);
                            spawn_settings_op(st, SettingOp::Visibility { private });
                        }
                        90 => {
                            match sel.as_str() {
                                "Pull" => {
                                    st.pending_sync_mode = Some("pull".into());
                                    st.run_action = Some(0);
                                }
                                "Commit + Push" => {
                                    st.modal = DetailModal::EditText {
                                        title: "Commit Message".into(),
                                        prompt: "Message (stages all changes):".into(),
                                        input: "chore: update".into(),
                                        tag: 91,
                                    };
                                }
                                _ => {
                                    st.pending_sync_mode = Some("push_only".into());
                                    st.run_action = Some(0);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            true
        }

        DetailModal::EditText { input, tag, .. } => {
            match code {
                KeyCode::Esc => { st.modal = DetailModal::None; }
                KeyCode::Backspace => { input.pop(); }
                KeyCode::Char(c) if input.len() < 200 => { input.push(c); }
                KeyCode::Enter => {
                    let text = input.trim().to_string();
                    let tag  = *tag;
                    st.modal = DetailModal::None;
                    match tag {
                        61 => {
                            if let Some(s) = &mut st.settings { s.description = text.clone(); }
                            spawn_settings_op(st, SettingOp::Description { text });
                        }
                        62 => {
                            let topics: Vec<String> = text.split(',')
                                .map(|s| s.trim().to_lowercase())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let invalid = topics.iter().find(|t| t.contains(' '));
                            if let Some(bad) = invalid {
                                st.status_msg = Some(format!("Invalid topic '{}': no spaces within words", bad));
                            } else {
                                if let Some(s) = &mut st.settings { s.topics = topics.clone(); }
                                spawn_settings_op(st, SettingOp::Topics { topics });
                            }
                        }
                        63 => {
                            if !text.is_empty() {
                                st.modal = DetailModal::SelectRole { collab_name: text, cursor: 0 };
                            }
                        }
                        65 => {
                            if !text.is_empty() {
                                spawn_settings_op(st, SettingOp::Rename { new_name: text });
                            }
                        }
                        70 => {
                            if !text.is_empty() {
                                let path = st.summary.path.clone();
                                let add_ok = git_run_ok(&path, &["add", "-A"]);
                                let commit_ok = add_ok && git_run_ok(&path, &["commit", "-m", &text]);
                                st.status_msg = Some(if commit_ok {
                                    st.summary.ahead += 1;
                                    st.summary.is_dirty = false;
                                    format!("Committed: {}", text.chars().take(50).collect::<String>())
                                } else if !add_ok {
                                    "git add -A failed".into()
                                } else {
                                    "Commit failed — nothing staged?".into()
                                });
                            }
                        }
                        91 => {
                            st.pending_sync_mode  = Some("commit_push".into());
                            st.pending_commit_msg = Some(text);
                            st.run_action         = Some(0);
                        }
                        92 => {
                            if !text.is_empty() {
                                let default_msg = text.clone();
                                st.pending_tag_name = Some(text);
                                st.modal = DetailModal::EditText {
                                    title: "Release — Tag Message".into(),
                                    prompt: "Tag message (blank = tag name):".into(),
                                    input: default_msg,
                                    tag: 93,
                                };
                            }
                        }
                        93 => {
                            let tag_msg = if text.is_empty() {
                                st.pending_tag_name.clone().unwrap_or_default()
                            } else { text };
                            st.pending_tag_msg = Some(tag_msg);
                            let tag_display = st.pending_tag_name.as_deref().unwrap_or("").to_string();
                            st.modal = DetailModal::ConfirmAction {
                                message: format!("Push tag '{}' to remote?", tag_display),
                                tag: 94,
                            };
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            true
        }

        DetailModal::SelectRole { collab_name, cursor } => {
            const ROLES: &[&str] = &["pull", "push", "maintain", "admin"];
            match code {
                KeyCode::Esc => { st.modal = DetailModal::None; }
                KeyCode::Up   => { *cursor = cursor.saturating_sub(1); }
                KeyCode::Down => { *cursor = (*cursor + 1).min(ROLES.len() - 1); }
                KeyCode::Enter => {
                    let name = collab_name.clone();
                    let role = ROLES[*cursor].to_string();
                    st.modal = DetailModal::None;
                    spawn_settings_op(st, SettingOp::AddCollaborator { name, role });
                }
                _ => {}
            }
            true
        }

        DetailModal::ConfirmAction { tag, .. } => {
            match code {
                KeyCode::Esc | KeyCode::Char('n') => { st.modal = DetailModal::None; }
                KeyCode::Enter | KeyCode::Char('y') => {
                    let tag = *tag;
                    st.modal = DetailModal::None;
                    match tag {
                        31 => {
                            let path = st.summary.path.clone();
                            if let Some(b) = st.branches.get(st.branch_cursor) {
                                let name = b.name.clone();
                                let ok = delete_branch(&path, &name);
                                if ok {
                                    st.branches = list_branches(&path);
                                    st.branch_count = st.branches.len();
                                    st.branch_cursor = st.branch_cursor.min(st.branches.len().saturating_sub(1));
                                    st.status_msg = Some(format!("Deleted '{}'", name));
                                } else {
                                    st.status_msg = Some("Delete failed — unmerged changes?".into());
                                }
                            }
                        }
                        64 => { spawn_settings_op(st, SettingOp::Archive); }
                        66 => { spawn_settings_op(st, SettingOp::Delete); }
                        94 => {
                            st.pending_push_tag = true;
                            st.run_action = Some(1);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            true
        }
    }
}

pub fn handle_key(code: KeyCode, st: &mut St) {
    st.status_msg = None;
    match code {
        KeyCode::Char('q') | KeyCode::Esc => st.quit = true,

        KeyCode::Tab | KeyCode::Right => {
            st.tab = st.tab.next();
            maybe_load_settings(st);
            maybe_load_prs_issues(st);
        }
        KeyCode::Left => {
            st.tab = st.tab.prev();
            maybe_load_settings(st);
            maybe_load_prs_issues(st);
        }
        KeyCode::Char('1') => { st.tab = Tab::Overview; }
        KeyCode::Char('2') => { st.tab = Tab::Commits; }
        KeyCode::Char('3') => { st.tab = Tab::Branches; }
        KeyCode::Char('4') => { st.tab = Tab::Files; }
        KeyCode::Char('5') => { st.tab = Tab::Stats; }
        KeyCode::Char('6') => {
            st.tab = Tab::Settings;
            maybe_load_settings(st);
        }
        KeyCode::Char('7') => {
            st.tab = Tab::PRs;
            maybe_load_prs_issues(st);
        }
        KeyCode::Char('8') => {
            st.tab = Tab::Issues;
            maybe_load_prs_issues(st);
        }
        KeyCode::Char('9') => { st.tab = Tab::Todos; }

        KeyCode::Down | KeyCode::Char('j') => match st.tab {
            Tab::Overview  => st.ov_off  = st.ov_off.saturating_add(1),
            Tab::Commits   => {
                let max = st.history.len().saturating_sub(1);
                st.hist_off = (st.hist_off + 1).min(max);
            }
            Tab::Branches  => {
                let max = st.branches.len().saturating_sub(1);
                st.branch_cursor = (st.branch_cursor + 1).min(max);
            }
            Tab::Files     => {
                if st.tree_cursor + 1 < st.tree_visible.len() {
                    st.tree_cursor += 1;
                    st.run_action = Some(40);
                }
            }
            Tab::Stats     => st.stats_off  = st.stats_off.saturating_add(1),
            Tab::Settings  => st.settings_cursor = (st.settings_cursor + 1).min(14),
            Tab::PRs       => st.pr_cursor    = (st.pr_cursor + 1).min(st.repo_prs.len().saturating_sub(1)),
            Tab::Issues    => st.issue_cursor = (st.issue_cursor + 1).min(st.repo_issues.len().saturating_sub(1)),
            Tab::Todos     => st.todo_cursor  = (st.todo_cursor + 1).min(st.repo_todos.len().saturating_sub(1)),
        },
        KeyCode::Up | KeyCode::Char('k') => match st.tab {
            Tab::Overview  => st.ov_off   = st.ov_off.saturating_sub(1),
            Tab::Commits   => st.hist_off    = st.hist_off.saturating_sub(1),
            Tab::Branches  => { st.branch_cursor = st.branch_cursor.saturating_sub(1); }
            Tab::Files     => {
                if st.tree_cursor > 0 {
                    st.tree_cursor -= 1;
                    st.run_action = Some(40);
                }
            }
            Tab::Stats     => st.stats_off   = st.stats_off.saturating_sub(1),
            Tab::Settings  => st.settings_cursor = st.settings_cursor.saturating_sub(1),
            Tab::PRs       => st.pr_cursor    = st.pr_cursor.saturating_sub(1),
            Tab::Issues    => st.issue_cursor = st.issue_cursor.saturating_sub(1),
            Tab::Todos     => st.todo_cursor  = st.todo_cursor.saturating_sub(1),
        },
        KeyCode::PageDown => match st.tab {
            Tab::Commits => {
                let max = st.history.len().saturating_sub(1);
                st.hist_off = (st.hist_off + 10).min(max);
            }
            Tab::Files => {
                let max = st.tree_visible.len().saturating_sub(1);
                st.tree_cursor = (st.tree_cursor + 10).min(max);
                st.run_action = Some(40);
            }
            Tab::PRs    => st.pr_cursor    = (st.pr_cursor + 10).min(st.repo_prs.len().saturating_sub(1)),
            Tab::Issues => st.issue_cursor = (st.issue_cursor + 10).min(st.repo_issues.len().saturating_sub(1)),
            Tab::Todos  => st.todo_cursor  = (st.todo_cursor + 10).min(st.repo_todos.len().saturating_sub(1)),
            _ => {}
        },
        KeyCode::PageUp => match st.tab {
            Tab::Commits => st.hist_off    = st.hist_off.saturating_sub(10),
            Tab::Files   => {
                st.tree_cursor = st.tree_cursor.saturating_sub(10);
                st.run_action = Some(40);
            }
            Tab::PRs    => st.pr_cursor    = st.pr_cursor.saturating_sub(10),
            Tab::Issues => st.issue_cursor = st.issue_cursor.saturating_sub(10),
            Tab::Todos  => st.todo_cursor  = st.todo_cursor.saturating_sub(10),
            _ => {}
        },

        KeyCode::Enter => match st.tab {
            Tab::Files => {
                if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                    let node = &mut st.tree_nodes[ni];
                    if node.is_dir {
                        node.expanded = !node.expanded;
                        let new_vis = rebuild_tree_visible(&st.tree_nodes);
                        st.tree_visible = new_vis;
                        st.tree_cursor = st.tree_cursor.min(st.tree_visible.len().saturating_sub(1));
                        st.file_preview = vec![];
                    }
                }
            }
            Tab::Branches => { st.run_action = Some(30); }
            Tab::Settings => open_settings_modal(st),
            Tab::PRs => {
                if let Some(pr) = st.repo_prs.get(st.pr_cursor) {
                    let url = pr.url.clone();
                    crate::util::open_path(&url);
                    st.status_msg = Some("Opening PR in browser...".into());
                }
            }
            Tab::Issues => {
                if let Some(issue) = st.repo_issues.get(st.issue_cursor) {
                    let url = issue.url.clone();
                    crate::util::open_path(&url);
                    st.status_msg = Some("Opening issue in browser...".into());
                }
            }
            Tab::Todos => {
                use crate::todos::TodoStatus;
                if let Some(t) = st.repo_todos.get_mut(st.todo_cursor) {
                    t.status = match t.status {
                        TodoStatus::Open       => TodoStatus::InProgress,
                        TodoStatus::InProgress => TodoStatus::Done,
                        TodoStatus::Done       => TodoStatus::Cancelled,
                        TodoStatus::Cancelled  => TodoStatus::Open,
                    };
                    t.touch();
                    let all = crate::todos::load_manual();
                    let updated_t = t.clone();
                    let new_todos: Vec<_> = all.into_iter()
                        .map(|x| if x.id == updated_t.id { updated_t.clone() } else { x })
                        .collect();
                    let _ = crate::todos::save(&new_todos);
                }
            }
            _ => {}
        },

        KeyCode::Char('D') if st.tab == Tab::Branches => {
            if let Some(b) = st.branches.get(st.branch_cursor) {
                if b.is_current {
                    st.status_msg = Some("Cannot delete the current branch".into());
                } else {
                    let name = b.name.clone();
                    st.modal = DetailModal::ConfirmAction {
                        message: format!("Delete branch '{}'?", name),
                        tag: 31,
                    };
                }
            }
        }
        KeyCode::Char('P') if st.tab == Tab::Branches => { st.run_action = Some(32); }
        KeyCode::Char('n') if st.tab == Tab::Branches => {
            st.modal = DetailModal::NewBranch { input: String::new(), error: None };
        }

        KeyCode::Char('p') if st.tab == Tab::Stats => {
            st.stats_period = st.stats_period.next();
            st.stats_off = 0;
            st.run_action = Some(41);
        }
        KeyCode::Char('<') if st.tab == Tab::Stats => {
            st.stats_period = st.stats_period.prev();
            st.stats_off = 0;
            st.run_action = Some(41);
        }
        KeyCode::Char('>') if st.tab == Tab::Stats => {
            st.stats_period = st.stats_period.next();
            st.stats_off = 0;
            st.run_action = Some(41);
        }

        KeyCode::Char(']') if st.tab == Tab::Files => {
            st.preview_off = st.preview_off.saturating_add(5);
        }
        KeyCode::Char('[') if st.tab == Tab::Files => {
            st.preview_off = st.preview_off.saturating_sub(5);
        }
        KeyCode::Char('e') if st.tab == Tab::Files => {
            if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                let node = &st.tree_nodes[ni];
                if !node.is_dir {
                    st.pending_file = Some(st.summary.path.join(&node.rel_path));
                    st.run_action = Some(110);
                    st.status_msg = Some("Opening file...".into());
                }
            }
        }

        KeyCode::Char('s') => {
            st.modal = DetailModal::SelectOption {
                title: "Sync".into(),
                options: vec!["Pull".into(), "Commit + Push".into(), "Push only".into()],
                cursor: 0,
                tag: 90,
            };
        }
        KeyCode::Char('r') => {
            st.modal = DetailModal::EditText {
                title: "Release — Tag Name".into(),
                prompt: "Tag name (e.g. v1.0.0):".into(),
                input: String::new(),
                tag: 92,
            };
        }
        KeyCode::Char('W') => {
            if let Ok(repo) = Repository::open(&st.summary.path) {
                if let Ok(remote) = repo.find_remote("origin") {
                    if let Some(url) = remote.url() {
                        let web = to_web_url(url);
                        crate::util::open_path(&web);
                        st.status_msg = Some("Opening in browser...".into());
                    }
                }
            }
        }
        KeyCode::Char('o') => { st.run_action = Some(106); }
        KeyCode::Char('y') => {
            let path = st.summary.path.to_string_lossy().to_string();
            st.status_msg = Some(if copy_to_clipboard(&path) {
                "Path copied!".into()
            } else {
                format!("No clipboard tool found — path: {}", path)
            });
        }
        KeyCode::Char('u') => {
            if let Some(ref url) = st.remote_url_str.clone() {
                st.status_msg = Some(if copy_to_clipboard(url) {
                    "URL copied!".into()
                } else {
                    format!("No clipboard tool found — URL: {}", url)
                });
            }
        }
        KeyCode::Char('g') => {
            st.run_action = Some(107);
            st.status_msg = Some("Opening .gitignore...".into());
        }
        KeyCode::Char('S') => {
            let ok = git_run_ok(&st.summary.path, &["stash"]);
            st.status_msg = Some(if ok { "Stashed.".into() } else { "Stash failed.".into() });
            if ok { st.stash_count += 1; }
        }
        KeyCode::Char('A') => {
            let ok = git_run_ok(&st.summary.path, &["stash", "pop"]);
            st.status_msg = Some(if ok { "Stash applied.".into() } else { "No stash / error.".into() });
            if ok { st.stash_count = st.stash_count.saturating_sub(1); }
        }

        _ => {}
    }
}

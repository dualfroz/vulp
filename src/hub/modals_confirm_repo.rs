//! `process_modal_confirm` arms for repo-creation flows: the bulk-import
//! affiliation picker, clone/init/link/import-directory, repo templates,
//! the workspace-wide "remove .git" action, and the copy-URL picker.

use super::modals_spawn::{
    fetch_orgs_blocking, open_template_picker, spawn_bulk_import, spawn_create_repo,
    spawn_publish_existing,
};
use super::state::{ConfirmTag, HubState, Modal, SelectTag, TextTag};

/// Handles one step of a repo-creation, template-management, or workspace
/// bulk-action modal chain. Returns `None` once `modal` has been consumed by
/// one of this file's arms, or `Some(modal)` unchanged — this is the last
/// file in `modals::process_modal_confirm`'s chain, so an unmatched modal is
/// simply discarded by the caller either way.
pub fn try_confirm(state: &mut HubState, modal: Modal) -> Option<Modal> {
    match modal {
        Modal::Multi { checks, tag: super::state::MultiTag::BulkImportAffiliation(acc_idx), .. } => {
            // "Repos I own" is always included regardless of how it's left
            // checked — bulk import without your own repos doesn't make sense.
            let affiliation = crate::providers::RepoAffiliation {
                owner:        true,
                collaborator: checks.get(1).copied().unwrap_or(false),
                org_member:   checks.get(2).copied().unwrap_or(false),
            };
            spawn_bulk_import(state, acc_idx, affiliation);
            None
        }

        Modal::Text { value, tag: TextTag::CloneUrl, .. } => {
            if !value.trim().is_empty() {
                state.pending_clone = Some(value.trim().to_string());
            }
            None
        }

        Modal::Text { value, tag: TextTag::InitName, .. } => {
            let name = value.trim().to_string();
            if name.is_empty() { return None; }

            // Repos can be created under an org instead of the personal account —
            // ask first if the default account actually belongs to any.
            let orgs = fetch_orgs_blocking(state);
            if !orgs.is_empty() {
                let mut opts = vec!["@me (personal account)".to_string()];
                opts.extend(orgs);
                state.modal = Modal::Select {
                    title: format!("Create '{}' — Owner", name),
                    options: opts,
                    cursor: 0,
                    tag: SelectTag::InitOwner(name),
                };
            } else {
                open_template_picker(state, name, None);
            }
            None
        }

        Modal::Select { cursor, options, tag: SelectTag::InitOwner(name), .. } => {
            let owner = if cursor == 0 { None } else { options.get(cursor).cloned() };
            open_template_picker(state, name, owner);
            None
        }

        // ── Repo creation with template ───────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::InitTemplate { name: repo_name, owner }, .. } => {
            let template = if cursor == 0 { None } else { state.templates.get(cursor - 1).cloned() };
            let target = state.workspace_root.join(&repo_name);
            if let Err(e) = std::fs::create_dir_all(&target) {
                state.status_msg = Some(format!("mkdir failed: {e}"));
                return None;
            }
            let git_ok = std::process::Command::new("git")
                .arg("init").current_dir(&target).output()
                .map(|o| o.status.success()).unwrap_or(false);
            if !git_ok {
                state.status_msg = Some("git init failed".into());
                return None;
            }
            // create .vulp/meta.toml
            let vulp_dir = target.join(".vulp");
            let _ = std::fs::create_dir_all(&vulp_dir);
            let tmpl_name = template.as_ref().map(|t| t.name.as_str()).unwrap_or("blank");
            let meta = format!("created_at = \"{}\"\ntemplate = \"{}\"\n",
                chrono::Utc::now().format("%Y-%m-%d"), tmpl_name);
            let _ = std::fs::write(vulp_dir.join("meta.toml"), meta);

            let commands = template.as_ref().map(|t| t.commands.clone()).unwrap_or_default();
            spawn_create_repo(
                state, target, repo_name.clone(),
                vec!["mkdir + git init  ✓".into(), ".vulp/meta.toml  ✓".into()],
                commands, owner,
            );
            None
        }

        // ── Import directory as repos ─────────────────────────────────────────
        Modal::Text { value, tag: TextTag::ImportDir, .. } => {
            let src = super::modals::expand_tilde(&value);
            if !src.exists() {
                state.status_msg = Some(format!("Path does not exist: {}", src.display()));
                return None;
            }
            if state.pull_all_rx.is_some() {
                state.status_msg = Some("A workspace job is already running — wait for it to finish.".into());
                return None;
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
            None
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
            None
        }
        Modal::Text { value, tag: TextTag::NewTemplateCmds(name), .. } => {
            let commands: Vec<String> = value.split(';')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let t = super::state::RepoTemplate { name: name.clone(), description: String::new(), commands };
            state.templates.push(t);
            super::actions::save_templates(&state.templates);
            state.status_msg = Some(format!("Template '{}' saved", name));
            None
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
            None
        }
        Modal::Confirm { tag: ConfirmTag::DeleteTemplate(ti), .. } => {
            let name = state.templates[ti].name.clone();
            state.templates.remove(ti);
            super::actions::save_templates(&state.templates);
            let max = 10 + 1 + state.templates.len();
            if state.hub_settings_cur >= max { state.hub_settings_cur = max.saturating_sub(1); }
            state.status_msg = Some(format!("Template '{}' deleted", name));
            None
        }

        // ── Remove .git from all workspace repos ──────────────────────────────
        Modal::Confirm { tag: ConfirmTag::RemoveGitAll, .. } => {
            if state.pull_all_rx.is_some() {
                state.status_msg = Some("A workspace job is already running — wait for it to finish.".into());
                return None;
            }
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
                    if git_dir.exists()
                        && std::fs::remove_dir_all(&git_dir).is_ok() {
                            count += 1;
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let _ = tx.send(format!("Removed .git: {}", name));
                        }
                }
                let _ = tx.send(format!("Done: removed .git from {} dirs", count));
                let _ = tx.send("DONE".into());
            });
            None
        }

        Modal::Text { value, tag: TextTag::LinkPath, .. } => {
            let src = super::modals::expand_tilde(&value);
            if !src.exists() {
                state.status_msg = Some(format!("Path does not exist: {}", src.display()));
                return None;
            }
            let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.is_empty() { state.status_msg = Some("Invalid path".into()); return None; }
            let link = state.workspace_root.join(&name);
            if link.exists() {
                state.status_msg = Some(format!("{name} already exists in workspace"));
                return None;
            }
            if let Err(e) = std::os::unix::fs::symlink(&src, &link) {
                state.status_msg = Some(format!("symlink failed: {e}"));
                return None;
            }
            if !src.join(".git").exists() {
                // No git history at all: fully onboard it, same as "Create new empty
                // repo" — stage everything that's already there, commit, create the
                // remote, push.
                spawn_create_repo(
                    state, src, name,
                    vec!["Linked into workspace  ✓".into()],
                    vec![], None,
                );
                return None;
            }
            // Already has a .git — ask what to do with the existing history instead
            // of silently guessing. A `.git` dir with zero commits (e.g. a bare
            // `git init` with nothing committed yet) still offers "discard" as a
            // no-op-equivalent option, which is harmless.
            state.modal = Modal::Select {
                title: format!("'{}' already has a .git", name),
                options: vec![
                    "Keep history — link only (push yourself later)".into(),
                    "Keep history — link & publish to GitHub now".into(),
                    "Discard history — start a fresh repo".into(),
                ],
                cursor: 0,
                tag: SelectTag::LinkExisting { src, name },
            };
            None
        }

        Modal::Select { cursor, tag: SelectTag::LinkExisting { src, name }, .. } => {
            match cursor {
                0 => {
                    super::actions::refresh_repos(state);
                    state.status_msg = Some(format!("Linked {name}"));
                }
                1 => {
                    spawn_publish_existing(state, src, name, None);
                }
                2 => {
                    let _ = std::fs::remove_dir_all(src.join(".git"));
                    spawn_create_repo(
                        state, src, name,
                        vec!["Old .git removed  ✓".into(), "Linked into workspace  ✓".into()],
                        vec![], None,
                    );
                }
                _ => {}
            }
            None
        }

        Modal::Select { cursor, tag: SelectTag::CopyUrl(path), .. } => {
            use super::actions::{copy_to_clipboard, get_remote_url, to_web_url};
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
            None
        }

        other => Some(other),
    }
}

use super::modals_spawn::spawn_profile_pull;
use super::state::{HubState, Modal, ProfileScopeAction, SelectTag, TextTag};

pub fn try_confirm(state: &mut HubState, modal: Modal) -> Option<Modal> {
    match modal {
        Modal::Text { value, tag: TextTag::SetWorkspace, .. } => {
            let path = super::modals::expand_tilde(&value);
            if !path.exists() {
                state.status_msg = Some(format!("Directory does not exist: {}", path.display()));
                return None;
            }
            if !path.is_dir() {
                state.status_msg = Some("Not a directory".into());
                return None;
            }
            state.workspace_root = path;
            super::actions::refresh_repos(state);
            state.status_msg = Some(format!(
                "Workspace changed to {}  (to persist: edit ~/.vulp/config.toml)",
                state.workspace_root.display(),
            ));
            None
        }
        Modal::Text { value, tag: TextTag::SetEditor, .. } => {
            if !value.trim().is_empty() {
                state.editor = value.trim().to_string();
                state.status_msg = Some(format!("Editor set to: {}", state.editor));
            }
            None
        }
        Modal::Text { value, tag: TextTag::SetClipboard, .. } => {
            state.clipboard_cmd = value.trim().to_string();
            state.status_msg = Some("Clipboard command updated".into());
            None
        }
        Modal::Select { cursor, options, tag: SelectTag::SetEditor, .. } => {
            let choice = options.get(cursor).cloned().unwrap_or_default();
            if choice == "Custom..." {
                state.modal = Modal::Text {
                    title: "Editor".into(),
                    prompt: "Command (e.g. nvim, code):".into(),
                    value: state.editor.clone(),
                    tag: TextTag::SetEditor,
                    max_len: Some(60),
                };
            } else {
                state.editor = choice;
                state.status_msg = Some(format!("Editor set to: {}", state.editor));
            }
            None
        }
        Modal::Select { cursor, options, tag: SelectTag::SetClipboard, .. } => {
            let choice = options.get(cursor).cloned().unwrap_or_default();
            if choice == "Custom..." {
                state.modal = Modal::Text {
                    title: "Clipboard Command".into(),
                    prompt: "Command (e.g. xclip -sel clip):".into(),
                    value: state.clipboard_cmd.clone(),
                    tag: TextTag::SetClipboard,
                    max_len: Some(60),
                };
            } else {
                state.clipboard_cmd = choice;
                state.status_msg = Some("Clipboard command updated".into());
            }
            None
        }

        Modal::Select { cursor, tag: SelectTag::ProfileMenu { profile_count }, .. } => {
            if cursor < profile_count {
                let profiles = crate::profile::list_profiles(&state.cfg);
                if let Some(name) = profiles.get(cursor) {
                    match crate::profile::load_profile(&state.cfg, name) {
                        Ok(snap) => {
                            if let Err(e) = crate::profile::apply(&snap, &mut state.cfg) {
                                state.status_msg = Some(format!("Failed to apply profile: {e:#}"));
                            } else {
                                state.editor = state.cfg.editor.clone();
                                state.clipboard_cmd = state.cfg.clipboard_cmd.clone();
                                super::actions::refresh_repos(state);
                                state.status_msg = Some(format!("Switched to profile '{}'", name));
                            }
                        }
                        Err(e) => state.status_msg = Some(format!("Failed to load profile '{}': {e:#}", name)),
                    }
                }
                return None;
            }
            let scope_select = |title: String, tag_action: ProfileScopeAction| Modal::Select {
                title,
                options: vec!["Settings only".into(), "Settings + workspace structure".into()],
                cursor: 0,
                tag: SelectTag::ProfileScope(tag_action),
            };
            match cursor - profile_count {
                0 => {
                    state.modal = Modal::Text {
                        title: "Save Profile".into(),
                        prompt: "Profile name:".into(),
                        value: String::new(),
                        tag: TextTag::ProfileSaveName,
                        max_len: Some(40),
                    };
                }
                1 => {
                    state.modal = scope_select("Copy as JSON — scope".into(), ProfileScopeAction::CopyJson);
                }
                2 => {
                    state.modal = Modal::Text {
                        title: "Import Settings".into(),
                        prompt: "Paste JSON:".into(),
                        value: String::new(),
                        tag: TextTag::ProfileImportJson,
                        max_len: None,
                    };
                }
                3 => {
                    state.modal = scope_select(
                        format!("Push to '{}' — scope", crate::profile::SETTINGS_REPO_NAME),
                        ProfileScopeAction::PushRepo,
                    );
                }
                4 => spawn_profile_pull(state),
                _ => {}
            }
            None
        }
        Modal::Text { value, tag: TextTag::ProfileSaveName, .. } => {
            let name = value.trim().to_string();
            if name.is_empty() { return None; }
            state.modal = Modal::Select {
                title: format!("Save '{}' — scope", name),
                options: vec!["Settings only".into(), "Settings + workspace structure".into()],
                cursor: 0,
                tag: SelectTag::ProfileScope(ProfileScopeAction::SaveProfile(name)),
            };
            None
        }
        Modal::Text { value, tag: TextTag::ProfileImportJson, .. } => {
            match crate::profile::from_json(value.trim()) {
                Ok(snap) => {
                    let has_structure = snap.structure.is_some();
                    if let Err(e) = crate::profile::apply(&snap, &mut state.cfg) {
                        state.status_msg = Some(format!("Failed to apply settings: {e:#}"));
                    } else {
                        state.editor = state.cfg.editor.clone();
                        state.clipboard_cmd = state.cfg.clipboard_cmd.clone();
                        if has_structure { super::actions::refresh_repos(state); }
                        state.status_msg = Some("Settings imported".into());
                    }
                }
                Err(e) => state.status_msg = Some(format!("Invalid settings JSON: {e:#}")),
            }
            None
        }
        Modal::Select { cursor, tag: SelectTag::ProfileScope(action), .. } => {
            let include_structure = cursor == 1;
            state.cfg.editor = state.editor.clone();
            state.cfg.clipboard_cmd = state.clipboard_cmd.clone();
            let snap = crate::profile::snapshot(&state.cfg, include_structure);
            match action {
                ProfileScopeAction::SaveProfile(name) => {
                    match crate::profile::save_profile(&state.cfg, &name, &snap) {
                        Ok(()) => state.status_msg = Some(format!("Profile '{}' saved", name)),
                        Err(e) => state.status_msg = Some(format!("Failed to save profile: {e:#}")),
                    }
                }
                ProfileScopeAction::CopyJson => {
                    match crate::profile::to_json(&snap) {
                        Ok(json) => {
                            if crate::util::clipboard::copy_to_clipboard(&json, &state.clipboard_cmd) {
                                state.status_msg = Some("Settings copied to clipboard as JSON".into());
                            } else {
                                state.status_msg = Some("Clipboard copy failed — check clipboard command in Settings".into());
                            }
                        }
                        Err(e) => state.status_msg = Some(format!("Failed to serialize settings: {e:#}")),
                    }
                }
                ProfileScopeAction::PushRepo => super::modals_spawn::spawn_profile_push(state, snap),
            }
            None
        }

        Modal::Select { cursor, tag: SelectTag::PullAllAccount, .. } => {
            super::modals_spawn::open_bulk_import_picker(state, cursor);
            None
        }

        other => Some(other),
    }
}

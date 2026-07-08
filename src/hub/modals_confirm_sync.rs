//! `process_modal_confirm` arms for sort/filter, repo creation entry points,
//! select-by-condition, the in-TUI sync flow, the contextual [m] action menu
//! (incl. PR/Issue creation), and the code-intelligence action chain.

use super::modals_spawn::{
    spawn_convention_preview, spawn_create_issue, spawn_create_pr, spawn_find_usages,
    spawn_list_functions, spawn_repo_stats, spawn_rename_preview,
};
use super::state::{ConfirmTag, HubState, MenuAction, Modal, PhaseResult, SelectTag, SortMode, TextTag};

/// Handles one step of a sort/filter, repo-creation, sync, Quick Actions, or
/// code-intel modal chain. Returns `None` once `modal` has been consumed by
/// one of this file's arms, or `Some(modal)` unchanged so the next file in
/// the chain (see `modals::process_modal_confirm`) can try it.
pub fn try_confirm(state: &mut HubState, modal: Modal) -> Option<Modal> {
    match modal {
        Modal::Select { cursor, tag: SelectTag::SortFilter, .. } => {
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
                    state.cfg.hide_external_clones = !state.cfg.hide_external_clones;
                    let _ = crate::config::save_config(&state.cfg);
                }
                10 => {
                    state.sort_mode      = SortMode::Default;
                    state.filter_dirty   = false;
                    state.filter_behind  = false;
                    state.filter_ahead   = false;
                    state.filter_archived = false;
                }
                _ => {}
            }
            state.rebuild_display();
            None
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
            None
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
                4 => { for ri in candidates { if !repos[ri].has_license   { state.selected.insert(ri); } } }
                5 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Rust")       { state.selected.insert(ri); } } }
                6 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("TypeScript") { state.selected.insert(ri); } } }
                7 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Python")     { state.selected.insert(ri); } } }
                8 => { for ri in candidates { if repos[ri].lang.as_deref() == Some("Go")         { state.selected.insert(ri); } } }
                _ => {}
            }
            state.status_msg = Some(format!("Selected {} repos", state.selected.len()));
            None
        }

        // ── Hub sync via in-TUI modals ────────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::SyncAction, .. } => {
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
            None
        }
        Modal::Text { value, tag: TextTag::SyncMsg, .. } => {
            state.pending_sync_msg = Some(value);
            state.pending_phase    = Some(PhaseResult::ExecSync);
            None
        }

        // ── Contextual [m] action menu ─────────────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::ActionMenu(actions), .. } => {
            match actions.get(cursor) {
                Some(MenuAction::Key(k)) => {
                    // Re-dispatch the entry's key through the global keymap.
                    // `state.modal` is already None here (mem::replace in
                    // process_modal_confirm), so a follow-up modal opened by
                    // the handler lands cleanly.
                    if let Some(phase) = super::keys_dispatch::handle_global_key(state, *k) {
                        state.pending_phase = Some(phase);
                    }
                }
                Some(MenuAction::NewPr) => {
                    state.modal = Modal::Text {
                        title: "New PR — Repository".into(),
                        prompt: "Repo name:".into(),
                        value: String::new(),
                        tag: TextTag::PrRepo,
                        max_len: Some(80),
                    };
                }
                Some(MenuAction::NewIssue) => {
                    state.modal = Modal::Text {
                        title: "New Issue — Repository".into(),
                        prompt: "Repo name:".into(),
                        value: String::new(),
                        tag: TextTag::IssueRepo,
                        max_len: Some(80),
                    };
                }
                Some(MenuAction::OpenAccountProfile) => {
                    if let Some(acc) = state.accounts.first() {
                        let host = acc.base_url.clone().unwrap_or_else(|| match acc.provider.as_str() {
                            "gitlab" => "https://gitlab.com".into(),
                            _        => "https://github.com".into(),
                        });
                        let url = format!("{}/{}", host.trim_end_matches('/'), acc.username);
                        crate::util::open_path(&url);
                        state.status_msg = Some("Opening profile in browser...".into());
                    }
                }
                Some(MenuAction::CodeIntelPicker) => super::modals_spawn::open_codeintel_picker(state),
                Some(MenuAction::AddLicense) => {
                    let targets: Vec<usize> = state.action_targets().into_iter()
                        .filter(|&ri| !state.repos[ri].has_license)
                        .collect();
                    if targets.is_empty() {
                        state.status_msg = Some("All targeted repos already have a LICENSE".into());
                    } else {
                        let cursor = match crate::license::default_kind(&state.cfg) {
                            crate::license::LicenseKind::Apache2 => 1,
                            crate::license::LicenseKind::Mit     => 0,
                        };
                        state.modal = Modal::Select {
                            title: format!("Add LICENSE — {} repo{}", targets.len(), if targets.len() == 1 { "" } else { "s" }),
                            options: vec!["MIT".into(), "Apache-2.0".into()],
                            cursor,
                            tag: SelectTag::LicenseKind { targets },
                        };
                    }
                }
                Some(MenuAction::StashSave(ri)) | Some(MenuAction::StashPop(ri)) => {
                    let ri = *ri;
                    let repo = state.repos.get(ri)?;
                    let (args, verb): (&[&str], _) = match actions.get(cursor) {
                        Some(MenuAction::StashPop(_)) => (&["stash", "pop"], "popped"),
                        _                             => (&["stash"], "stashed"),
                    };
                    let path = repo.path.clone();
                    let name = repo.name.clone();
                    if crate::util::git::git_run_ok(&path, args) {
                        // Point-refresh just this repo — no full rescan needed.
                        if let Ok(fresh) = crate::status::repo_summary(&path) {
                            let bookmarked = state.repos[ri].bookmarked;
                            let (stars, oi, op) = (state.repos[ri].stars, state.repos[ri].open_issues, state.repos[ri].open_prs);
                            state.repos[ri] = fresh;
                            state.repos[ri].bookmarked = bookmarked;
                            state.repos[ri].stars = stars;
                            state.repos[ri].open_issues = oi;
                            state.repos[ri].open_prs = op;
                        }
                        state.status_msg = Some(format!("{}: {}", name, verb));
                    } else {
                        state.status_msg = Some(format!("git stash failed on {} (conflict?)", name));
                    }
                }
                None => {}
            }
            None
        }

        // ── Add LICENSE ([m] action, bulk over action_targets) ────────────────
        Modal::Select { cursor, tag: SelectTag::LicenseKind { targets }, .. } => {
            let kind = if cursor == 1 { crate::license::LicenseKind::Apache2 } else { crate::license::LicenseKind::Mit };
            let names: Vec<&str> = targets.iter().take(6)
                .filter_map(|&ri| state.repos.get(ri).map(|r| r.name.as_str()))
                .collect();
            let more = targets.len().saturating_sub(names.len());
            let list = if more > 0 {
                format!("{} +{} more", names.join(", "), more)
            } else {
                names.join(", ")
            };
            state.modal = Modal::Confirm {
                message: format!(
                    "Write {} LICENSE to {} repo{}? ({}) — not committed",
                    kind.label(), targets.len(), if targets.len() == 1 { "" } else { "s" }, list,
                ),
                tag: ConfirmTag::WriteLicense { kind, targets },
            };
            None
        }
        Modal::Confirm { tag: ConfirmTag::WriteLicense { kind, targets }, .. } => {
            let holder = crate::license::holder_for(&state.cfg);
            let year   = chrono::Datelike::year(&chrono::Utc::now());
            let (mut written, mut skipped, mut failed) = (0usize, 0usize, 0usize);
            for ri in targets {
                let Some(repo) = state.repos.get(ri) else { continue };
                let (path, name) = (repo.path.clone(), repo.name.clone());
                match crate::license::write_license(&path, kind, &holder, year) {
                    Ok(crate::license::WriteOutcome::Written) => {
                        written += 1;
                        state.repos[ri].has_license = true;
                        // New untracked LICENSE file — the repo is dirty now.
                        state.repos[ri].is_dirty = true;
                        crate::activity::log(&crate::activity::ActivityEntry {
                            ts:     chrono::Utc::now().timestamp(),
                            kind:   crate::activity::ActivityKind::License,
                            repo:   name,
                            detail: format!("{} ({})", kind.label(), holder),
                        });
                    }
                    Ok(crate::license::WriteOutcome::AlreadyLicensed) => {
                        skipped += 1;
                        state.repos[ri].has_license = true;
                    }
                    Err(_) => failed += 1,
                }
            }
            let mut msg = format!("LICENSE ({}): {} written", kind.label(), written);
            if skipped > 0 { msg.push_str(&format!(", {} already licensed", skipped)); }
            if failed  > 0 { msg.push_str(&format!(", {} FAILED", failed)); }
            state.status_msg = Some(msg);
            None
        }

        Modal::Text { value, tag: TextTag::PrRepo, .. } => {
            let repo = value.trim().to_string();
            if repo.is_empty() { return None; }
            state.modal = Modal::Text {
                title: format!("New PR — '{}'", repo),
                prompt: "Title:".into(),
                value: String::new(),
                tag: TextTag::PrTitle(repo),
                max_len: None,
            };
            None
        }
        Modal::Text { value, tag: TextTag::PrTitle(repo), .. } => {
            let title = value.trim().to_string();
            if title.is_empty() { return None; }
            state.modal = Modal::Text {
                title: format!("New PR — '{}'", repo),
                prompt: "Head branch:".into(),
                value: String::new(),
                tag: TextTag::PrHead(repo, title),
                max_len: None,
            };
            None
        }
        Modal::Text { value, tag: TextTag::PrHead(repo, title), .. } => {
            let head = value.trim().to_string();
            if head.is_empty() { return None; }
            state.modal = Modal::Text {
                title: format!("New PR — '{}'", repo),
                prompt: "Base branch:".into(),
                value: "main".into(),
                tag: TextTag::PrBase(repo, title, head),
                max_len: None,
            };
            None
        }
        Modal::Text { value, tag: TextTag::PrBase(repo, title, head), .. } => {
            let base = if value.trim().is_empty() { "main".to_string() } else { value.trim().to_string() };
            spawn_create_pr(state, repo, title, head, base);
            None
        }

        Modal::Text { value, tag: TextTag::IssueRepo, .. } => {
            let repo = value.trim().to_string();
            if repo.is_empty() { return None; }
            state.modal = Modal::Text {
                title: format!("New Issue — '{}'", repo),
                prompt: "Title:".into(),
                value: String::new(),
                tag: TextTag::IssueTitle(repo),
                max_len: None,
            };
            None
        }
        Modal::Text { value, tag: TextTag::IssueTitle(repo), .. } => {
            let title = value.trim().to_string();
            if title.is_empty() { return None; }
            spawn_create_issue(state, repo, title);
            None
        }

        // ── Code intelligence (Stats tab, [m]) ─────────────────────────────────
        Modal::Select { cursor, tag: SelectTag::CodeIntelRepoPick, .. } => {
            if let Some(r) = state.repos.get(cursor) {
                let repo = r.path.clone();
                let name = r.name.clone();
                state.modal = Modal::Select {
                    title: format!("Code Intel — '{}'", name),
                    options: vec![
                        "List functions — names only".into(),
                        "List functions — names + args".into(),
                        "List functions — names + args + meta (length, refs)".into(),
                        "Find usages of an identifier...".into(),
                        "Rename an identifier...".into(),
                        "Convert naming convention...".into(),
                        "Repo code stats".into(),
                    ],
                    cursor: 0,
                    tag: SelectTag::CodeIntelAction { repo, name },
                };
            }
            None
        }
        Modal::Select { cursor, tag: SelectTag::CodeIntelAction { repo, name }, .. } => {
            match cursor {
                0 => spawn_list_functions(state, repo, name, crate::codeintel::FuncFormat::NamesOnly),
                1 => spawn_list_functions(state, repo, name, crate::codeintel::FuncFormat::WithArgs),
                2 => spawn_list_functions(state, repo, name, crate::codeintel::FuncFormat::WithMeta),
                3 => {
                    state.modal = Modal::Text {
                        title: format!("Find usages — '{}'", name),
                        prompt: "Identifier:".into(),
                        value: String::new(),
                        tag: TextTag::CodeIntelFindUsages { repo, name },
                        max_len: Some(80),
                    };
                }
                4 => {
                    state.modal = Modal::Text {
                        title: format!("Rename — '{}'", name),
                        prompt: "Current identifier name:".into(),
                        value: String::new(),
                        tag: TextTag::CodeIntelRenameOld { repo, name },
                        max_len: Some(80),
                    };
                }
                5 => {
                    state.modal = Modal::Select {
                        title: "Convert naming convention — target".into(),
                        options: vec!["snake_case".into(), "camelCase".into(), "PascalCase".into()],
                        cursor: 0,
                        tag: SelectTag::CodeIntelConvention { repo, name },
                    };
                }
                _ => spawn_repo_stats(state, repo, name),
            }
            None
        }
        Modal::Text { value, tag: TextTag::CodeIntelFindUsages { repo, name }, .. } => {
            let ident = value.trim().to_string();
            if ident.is_empty() { return None; }
            spawn_find_usages(state, repo, name, ident);
            None
        }
        Modal::Text { value, tag: TextTag::CodeIntelRenameOld { repo, name }, .. } => {
            let old = value.trim().to_string();
            if old.is_empty() { return None; }
            state.modal = Modal::Text {
                title: format!("Rename '{}' to...", old),
                prompt: "New name:".into(),
                value: String::new(),
                tag: TextTag::CodeIntelRenameNew { repo, name, old },
                max_len: Some(80),
            };
            None
        }
        Modal::Text { value, tag: TextTag::CodeIntelRenameNew { repo, name, old }, .. } => {
            let new = value.trim().to_string();
            if new.is_empty() || new == old { return None; }
            spawn_rename_preview(state, repo, name, old, new);
            None
        }
        Modal::Select { cursor, tag: SelectTag::CodeIntelConvention { repo, name }, .. } => {
            let target = match cursor {
                0 => crate::codeintel::NamingConvention::Snake,
                1 => crate::codeintel::NamingConvention::Camel,
                _ => crate::codeintel::NamingConvention::Pascal,
            };
            spawn_convention_preview(state, repo, name, target);
            None
        }
        Modal::RenamePreview { plan, .. } => {
            let n_files = plan.file_count();
            let n_edits = plan.edits.len();
            match crate::codeintel::apply_rename_plan(&plan) {
                Ok(applied) => {
                    state.modal = Modal::Info {
                        title: "✓ Rename applied".into(),
                        lines: vec![format!("{} edit{} across {} file{}", applied, if applied == 1 {""} else {"s"}, n_files, if n_files == 1 {""} else {"s"})],
                        scroll: 0,
                    };
                }
                Err(e) => {
                    state.modal = Modal::Info {
                        title: "✗ Rename failed".into(),
                        lines: vec![format!("{} of {} edits may be applied — error: {e}", n_edits, n_edits)],
                        scroll: 0,
                    };
                }
            }
            None
        }

        other => Some(other),
    }
}

//! `exec_action` — the repo-detail view's dispatcher for actions that need
//! to leave the TUI's alternate screen (interactive `inquire` prompts,
//! launching `$EDITOR`, blocking git subprocesses with visible output).
//! `run_detail_loop` in `mod.rs` calls this between `leave_tui`/`enter_tui`,
//! so everything here runs with the terminal back in normal/cooked mode.

use super::git::{build_history, checkout_branch, git_run, list_branches, push_branch};
use super::settings_io::{try_load_settings, try_make_provider};
use super::types::{RepoSettings, St};
use anyhow::Result;

/// Runs the action identified by `idx` (one of the numeric codes set on
/// `st.run_action`/passed in from a modal). Numeric codes rather than an enum
/// because they're produced from several different call sites (key handlers,
/// modal confirms, settings-tab rows) that don't otherwise share a type.
pub async fn exec_action(st: &mut St, idx: usize) -> Result<()> {
    let path = st.summary.path.clone();
    match idx {
        // ── 0: sync ──────────────────────────────────────────────────────────
        0 => {
            println!("\n── Sync: {} ──\n", st.summary.name);
            let mode       = st.pending_sync_mode.take().unwrap_or_else(|| "pull".into());
            let commit_msg = st.pending_commit_msg.take().unwrap_or_default();
            match mode.as_str() {
                "pull" => { git_run(&path, &["pull"]); }
                "commit_push" => {
                    git_run(&path, &["add", "-A"]);
                    git_run(&path, &["commit", "-m", &commit_msg]);
                    git_run(&path, &["push"]);
                }
                _ => { git_run(&path, &["push"]); } // push_only
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 1: release ───────────────────────────────────────────────────────
        1 => {
            println!("\n── Release: {} ──\n", st.summary.name);
            let tag_name = st.pending_tag_name.take().unwrap_or_default();
            let tag_msg  = st.pending_tag_msg.take().unwrap_or_else(|| tag_name.clone());
            let do_push  = std::mem::replace(&mut st.pending_push_tag, false);
            if !tag_name.is_empty() {
                git_run(&path, &["tag", "-a", &tag_name, "-m", &tag_msg]);
                if do_push {
                    git_run(&path, &["push", "origin", &tag_name]);
                }
                st.history = build_history(&path, 200);
                st.total_tags += 1;
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 30: checkout branch ──────────────────────────────────────────────
        30 => {
            if let Some(b) = st.branches.get(st.branch_cursor) {
                let name = b.name.clone();
                if b.is_current {
                    st.status_msg = Some(format!("Already on '{}'", name));
                    return Ok(());
                }
                let ok = checkout_branch(&path, &name);
                if ok {
                    st.summary.branch = name.clone();
                    st.branches = list_branches(&path);
                    st.branch_cursor = st.branches.iter().position(|b| b.is_current).unwrap_or(0);
                    st.status_msg = Some(format!("Switched to '{}'", name));
                } else {
                    st.status_msg = Some("Checkout failed (uncommitted changes?)".into());
                }
            }
        }

        // ── 31: delete branch (handled in-TUI via ConfirmAction modal) ──────
        31 => {}

        // ── 106-109: open in editor (leave_tui is handled by run_detail_loop) ─
        106 => {
            let editor = st.cfg_editor.clone();
            if let Err(e) = std::process::Command::new(&editor).arg(&path).status() {
                st.status_msg = Some(format!("Couldn't launch editor '{}': {}", editor, e));
            }
        }
        107 => {
            let editor = st.cfg_editor.clone();
            if let Err(e) = std::process::Command::new(&editor).arg(path.join(".gitignore")).status() {
                st.status_msg = Some(format!("Couldn't launch editor '{}': {}", editor, e));
            }
        }
        108 => {
            let editor = st.cfg_editor.clone();
            if let Err(e) = std::process::Command::new(&editor).arg(path.join("LICENSE")).status() {
                st.status_msg = Some(format!("Couldn't launch editor '{}': {}", editor, e));
            }
        }
        109 => {
            let editor = st.cfg_editor.clone();
            let readme = ["README.md", "README.rst", "README"].iter()
                .map(|f| path.join(f)).find(|p| p.exists())
                .unwrap_or_else(|| path.join("README.md"));
            if let Err(e) = std::process::Command::new(&editor).arg(&readme).status() {
                st.status_msg = Some(format!("Couldn't launch editor '{}': {}", editor, e));
            }
        }

        // ── 110: open pending_file in editor ────────────────────────────────
        110 => {
            if let Some(file) = st.pending_file.take() {
                let editor = st.cfg_editor.clone();
                if let Err(e) = std::process::Command::new(&editor).arg(&file).status() {
                    st.status_msg = Some(format!("Couldn't launch editor '{}': {}", editor, e));
                }
            }
        }

        // ── 32: push branch ──────────────────────────────────────────────────
        32 => {
            if let Some(b) = st.branches.get(st.branch_cursor) {
                let name = b.name.clone();
                println!("\n── Push '{}' ──\n", name);
                let ok = push_branch(&path, &name);
                if ok {
                    st.branches = list_branches(&path);
                    st.status_msg = Some(format!("Pushed '{}'", name));
                } else {
                    st.status_msg = Some("Push failed".into());
                }
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 50: load settings from GitHub API ───────────────────────────────
        50 => {
            let local_visibility = st.summary.visibility.clone().unwrap_or_else(|| "unknown".into());
            match try_load_settings(st).await {
                Ok(_) => {}
                Err(_) => {
                    st.settings = Some(RepoSettings {
                        visibility:  local_visibility,
                        description: String::new(),
                        topics:      vec![],
                        gh_auth_ok:  false,
                    });
                }
            }
        }

        // ── 60: toggle visibility ────────────────────────────────────────────
        60 => {
            println!("\n── Visibility: {} ──\n", st.summary.name);
            let choice = inquire::Select::new("Set visibility:", vec!["public", "private"])
                .prompt_skippable()?;
            if let Some(vis) = choice {
                let private = vis == "private";
                match try_make_provider(st).await {
                    Ok(provider) => match provider.update_visibility(&st.summary.name, private).await {
                        Ok(_) => {
                            let v = vis.to_string();
                            st.summary.visibility = Some(v.clone());
                            if let Some(s) = &mut st.settings { s.visibility = v; }
                            st.status_msg = Some(format!("Visibility → {}", vis));
                        }
                        Err(e) => { println!("API error: {}", e); }
                    }
                    Err(e) => { println!("Auth error: {}", e); }
                }
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 61: edit description ─────────────────────────────────────────────
        61 => {
            println!("\n── Description ──\n");
            let current = st.settings.as_ref().map(|s| s.description.as_str()).unwrap_or("").to_string();
            if let Some(desc) = inquire::Text::new("Description:").with_default(&current).prompt_skippable()? {
                match try_make_provider(st).await {
                    Ok(provider) => match provider.update_description(&st.summary.name, &desc).await {
                        Ok(_) => {
                            if let Some(s) = &mut st.settings { s.description = desc; }
                            st.status_msg = Some("Description updated".into());
                        }
                        Err(e) => { println!("API error: {}", e); }
                    }
                    Err(e) => { println!("Auth error: {}", e); }
                }
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 62: edit topics ──────────────────────────────────────────────────
        62 => {
            println!("\n── Topics ──\n");
            let current = st.settings.as_ref().map(|s| s.topics.join(", ")).unwrap_or_default();
            if let Some(raw) = inquire::Text::new("Topics (comma-separated):").with_default(&current).prompt_skippable()? {
                let topics: Vec<String> = raw.split(',').map(|s| s.trim().to_lowercase().to_string())
                    .filter(|s| !s.is_empty()).collect();
                match try_make_provider(st).await {
                    Ok(provider) => match provider.set_topics(&st.summary.name, &topics).await {
                        Ok(_) => {
                            if let Some(s) = &mut st.settings { s.topics = topics; }
                            st.status_msg = Some("Topics updated".into());
                        }
                        Err(e) => { println!("API error: {}", e); }
                    }
                    Err(e) => { println!("Auth error: {}", e); }
                }
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        // ── 63: add collaborator ─────────────────────────────────────────────
        63 => {
            println!("\n── Add Collaborator ──\n");
            if let Some(user) = inquire::Text::new("GitHub username:").prompt_skippable()? {
                if !user.trim().is_empty() {
                    let role = inquire::Select::new("Role:", vec!["pull", "push", "admin"])
                        .prompt_skippable()?;
                    if let Some(role) = role {
                        match try_make_provider(st).await {
                            Ok(provider) => match provider.add_collaborator(&st.summary.name, user.trim(), role).await {
                                Ok(_) => { st.status_msg = Some(format!("Invited {}", user.trim())); }
                                Err(e) => { println!("API error: {}", e); }
                            }
                            Err(e) => { println!("Auth error: {}", e); }
                        }
                    }
                }
            }
            println!("\n  Press Enter to return...");
            let _ = std::io::stdin().lines().next();
        }

        _ => {}
    }
    Ok(())
}

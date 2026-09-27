use super::git::git_run_ok;
use super::types::{DetailModal, RepoSettings, SettingOp, St, StatsPeriod};
use super::{activity_1d, activity_30d, activity_7d, activity_90d, activity_1y, activity_all};
use anyhow::Result;

pub fn compute_activity(path: &std::path::Path, period: StatsPeriod) -> Vec<(String, usize)> {
    match period {
        StatsPeriod::Day1    => activity_1d(path),
        StatsPeriod::Day7    => activity_7d(path),
        StatsPeriod::Day30   => activity_30d(path),
        StatsPeriod::Day90   => activity_90d(path),
        StatsPeriod::Year1   => activity_1y(path),
        StatsPeriod::AllTime => activity_all(path),
    }
}

pub fn settings_fallback(st: &St) -> RepoSettings {
    RepoSettings {
        visibility:  st.summary.visibility.clone().unwrap_or_else(|| "unknown".into()),
        description: String::new(),
        topics:      vec![],
        gh_auth_ok:  false,
    }
}

pub fn settings_fallback_auth(st: &St) -> RepoSettings {
    RepoSettings {
        visibility:  st.summary.visibility.clone().unwrap_or_else(|| "unknown".into()),
        description: String::new(),
        topics:      vec![],
        gh_auth_ok:  true,
    }
}

pub fn poll_settings_rx(st: &mut St) {
    let done = match &st.settings_rx {
        None => return,
        Some(rx) => match rx.try_recv() {
            Ok(r)  => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Channel disconnected".into())),
        },
    };
    st.settings_rx = None;
    match done {
        Some(Ok(s)) => {
            st.settings     = Some(s);
            st.settings_err = None;
        }
        Some(Err(e)) => {
            let lower = e.to_lowercase();
            let auth_fail = e.contains("No GitHub account") || e.contains("no account")
                || e.contains("no github") || e.contains("authentication")
                || lower.contains("bad credentials") || lower.contains(" 401 ");
            if auth_fail {
                st.settings     = Some(settings_fallback(st));
                st.settings_err = Some(e);
            } else {
                if st.settings.is_none() { st.settings = Some(settings_fallback_auth(st)); }
                st.settings_err = Some(format!("API error: {}", e));
            }
        }
        None => {}
    }
}

pub fn spawn_settings_fetch(st: &mut St) {
    if st.settings_rx.is_some() { return; }
    let data_dir  = st.cfg_data_dir.clone();
    let repo_name = st.summary.name.clone();
    let vis       = st.summary.visibility.clone().unwrap_or_else(|| "unknown".into());
    let (tx, rx)  = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        let result: Result<RepoSettings, String> = rt.block_on(async move {
            let accounts = crate::auth::load_accounts_path(&data_dir)
                .map_err(|e| e.to_string())?;
            let account = crate::providers::default_account(&accounts, None)
                .ok_or_else(|| "No GitHub account — run: vulp auth import".to_string())?;
            let provider = crate::providers::make_provider(account)
                .map_err(|e| e.to_string())?;
            let meta = provider.repo_meta(&repo_name).await
                .map_err(|e| e.to_string())?;
            Ok(RepoSettings {
                visibility:  vis,
                description: meta.description.unwrap_or_default(),
                topics:      meta.topics,
                gh_auth_ok:  true,
            })
        });
        let _ = tx.send(result);
    });

    st.settings_rx = Some(rx);
}

pub fn open_settings_modal(st: &mut St) {
    let cur_vis  = st.settings.as_ref().map(|s| s.visibility.clone()).or_else(|| st.summary.visibility.clone()).unwrap_or_default();
    let cur_desc = st.settings.as_ref().map(|s| s.description.clone()).unwrap_or_default();
    let cur_top  = st.settings.as_ref().map(|s| s.topics.join(", ")).unwrap_or_default();
    let repo     = st.summary.name.clone();
    match st.settings_cursor {
        0 => {
            let cursor = if cur_vis == "private" { 1 } else { 0 };
            st.modal = DetailModal::SelectOption {
                title: "Visibility".into(),
                options: vec!["public".into(), "private".into()],
                cursor, tag: 60,
            };
        }
        1 => {
            st.modal = DetailModal::EditText {
                title: "Description".into(),
                prompt: "New description (blank to clear):".into(),
                input: cur_desc, tag: 61,
            };
        }
        2 => {
            st.modal = DetailModal::EditText {
                title: "Topics".into(),
                prompt: "Comma-separated (e.g. rust, cli, tool). No spaces within words.".into(),
                input: cur_top, tag: 62,
            };
        }
        3 => {
            st.modal = DetailModal::EditText {
                title: "Add Collaborator".into(),
                prompt: "GitHub username:".into(),
                input: String::new(), tag: 63,
            };
        }
        4 => {
            st.modal = DetailModal::ConfirmAction {
                message: format!("Archive '{}' on GitHub?", repo),
                tag: 64,
            };
        }
        5 => {
            st.modal = DetailModal::EditText {
                title: "Rename Repository".into(),
                prompt: "New name:".into(),
                input: repo, tag: 65,
            };
        }
        6 => {
            let name = st.summary.name.clone();
            st.modal = DetailModal::ConfirmAction {
                message: format!("Permanently delete '{}' on GitHub AND locally?\nThis cannot be undone.", name),
                tag: 66,
            };
        }
        7 => { st.run_action = Some(107); }
        8 => { st.run_action = Some(108); }
        9 => { st.run_action = Some(109); }
        10 => {
            let ok = git_run_ok(&st.summary.path, &["pull"]);
            st.status_msg = Some(if ok { "Pulled successfully".into() } else { "Pull failed — check remote".into() });
            if ok { st.summary.behind = 0; }
        }
        11 => {
            st.modal = DetailModal::EditText {
                title:  "Commit".into(),
                prompt: "Commit message (stages all changes):".into(),
                input:  String::new(),
                tag:    70,
            };
        }
        12 => {
            let ok = git_run_ok(&st.summary.path, &["push"]);
            st.status_msg = Some(if ok { "Pushed successfully".into() } else { "Push failed — check remote".into() });
            if ok { st.summary.ahead = 0; }
        }
        13 => {
            let ok = git_run_ok(&st.summary.path, &["stash"]);
            st.status_msg = Some(if ok { "Stash saved".into() } else { "Stash failed".into() });
            if ok { st.summary.is_dirty = false; st.stash_count += 1; }
        }
        14 => {
            let ok = git_run_ok(&st.summary.path, &["stash", "pop"]);
            st.status_msg = Some(if ok { "Stash popped".into() } else { "Stash pop failed — no stash?".into() });
            if ok {
                if st.stash_count > 0 { st.stash_count -= 1; }
                let out = std::process::Command::new("git")
                    .arg("-C").arg(&st.summary.path).args(["status", "--porcelain"])
                    .output();
                if let Ok(o) = out {
                    st.summary.is_dirty = !o.stdout.is_empty();
                }
            }
        }
        _ => {}
    }
}

pub fn spawn_settings_op(st: &mut St, op: SettingOp) {
    if st.api_rx.is_some() {
        st.status_msg = Some("Another operation in progress...".into());
        return;
    }
    let data_dir  = st.cfg_data_dir.clone();
    let repo_name = st.summary.name.clone();
    let repo_path = st.summary.path.clone();
    let (tx, rx)  = std::sync::mpsc::channel();
    st.quit_after_op = matches!(op, SettingOp::Delete);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        let result: Result<String, String> = rt.block_on(async move {
            let accounts = crate::auth::load_accounts_path(&data_dir)
                .map_err(|e| e.to_string())?;
            let account = crate::providers::default_account(&accounts, Some("github"))
                .ok_or_else(|| "No GitHub account — run: vulp auth import".to_string())?;
            let provider = crate::providers::make_provider(account)
                .map_err(|e| e.to_string())?;
            match op {
                SettingOp::Visibility { private } => {
                    provider.update_visibility(&repo_name, private).await
                        .map(|_| format!("Visibility → {}", if private { "private" } else { "public" }))
                        .map_err(|e| e.to_string())
                }
                SettingOp::Description { text } => {
                    provider.update_description(&repo_name, &text).await
                        .map(|_| "Description updated".to_string())
                        .map_err(|e| e.to_string())
                }
                SettingOp::Topics { topics } => {
                    provider.set_topics(&repo_name, &topics).await
                        .map(|_| "Topics updated".to_string())
                        .map_err(|e| e.to_string())
                }
                SettingOp::AddCollaborator { name, role } => {
                    provider.add_collaborator(&repo_name, &name, &role).await
                        .map(|_| format!("Invited @{} as {}", name, role))
                        .map_err(|e| e.to_string())
                }
                SettingOp::Archive => {
                    provider.archive_repo(&repo_name).await
                        .map(|_| "Repository archived".to_string())
                        .map_err(|e| e.to_string())
                }
                SettingOp::Rename { new_name } => {
                    provider.rename_repo(&repo_name, &new_name).await
                        .map(|_| format!("Renamed to '{}'", new_name))
                        .map_err(|e| e.to_string())
                }
                SettingOp::Delete => {
                    provider.delete_repo(&repo_name).await.map_err(|e| e.to_string())?;
                    let is_symlink = repo_path.symlink_metadata()
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false);
                    let local_result = if is_symlink {
                        std::fs::remove_file(&repo_path)
                    } else {
                        std::fs::remove_dir_all(&repo_path)
                    };
                    match local_result {
                        Ok(())   => Ok("Deleted on GitHub and locally".to_string()),
                        Err(e)   => Err(format!("Deleted on GitHub, but local delete failed: {}", e)),
                    }
                }
            }
        });
        let _ = tx.send(result);
    });

    st.api_rx = Some(rx);
    st.status_msg = Some("Updating GitHub...".into());
}

pub fn poll_api_rx(st: &mut St) {
    let done = match &st.api_rx {
        None     => return,
        Some(rx) => match rx.try_recv() {
            Ok(r)                                            => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty)        => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Connection lost".into())),
        },
    };
    if let Some(result) = done {
        let success = result.is_ok();
        st.status_msg = Some(match result {
            Ok(msg)  => msg,
            Err(msg) => format!("⚠ {}", msg),
        });
        st.api_rx = None;
        if success && st.quit_after_op {
            st.quit = true;
        }
        st.quit_after_op = false;
    }
}

pub fn poll_prs_issues_rx(st: &mut St) {
    let done = match &st.prs_issues_rx {
        None     => return,
        Some(rx) => match rx.try_recv() {
            Ok(r)                                            => Some(Ok(r)),
            Err(std::sync::mpsc::TryRecvError::Empty)        => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(())),
        },
    };
    st.prs_issues_rx = None;
    st.prs_loaded = true;
    if let Some(Ok((prs, issues))) = done {
        st.repo_prs    = prs;
        st.repo_issues = issues;
    }
}

pub fn spawn_prs_issues_fetch(st: &mut St) {
    if st.prs_issues_rx.is_some() || st.prs_loaded { return; }
    let data_dir  = st.cfg_data_dir.clone();
    let repo_name = st.summary.name.clone();
    let (tx, rx)  = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        let result = rt.block_on(async move {
            let accounts = crate::auth::load_accounts_path(&data_dir).ok()?;
            let account  = crate::providers::default_account(&accounts, None)?;
            let provider = crate::providers::make_provider(account).ok()?;
            let prs    = provider.list_pull_requests(&repo_name).await.unwrap_or_default();
            let issues = provider.list_issues(&repo_name).await.unwrap_or_default();
            Some((prs, issues))
        });
        let _ = tx.send(result.unwrap_or_default());
    });

    st.prs_issues_rx = Some(rx);
    st.status_msg = Some("Fetching PRs and issues...".into());
}

pub async fn try_make_provider(st: &St) -> Result<Box<dyn crate::providers::Provider>> {
    let accounts = crate::auth::load_accounts_path(&st.cfg_data_dir)?;
    let account  = crate::providers::default_account(&accounts, Some("github"))
        .ok_or_else(|| anyhow::anyhow!("no github account — run: vulp auth import"))?;
    crate::providers::make_provider(account)
}

pub async fn try_load_settings(st: &mut St) -> Result<()> {
    let provider = try_make_provider(st).await?;
    let meta = provider.repo_meta(&st.summary.name).await?;
    st.settings = Some(RepoSettings {
        visibility:  st.summary.visibility.clone().unwrap_or_else(|| "unknown".into()),
        description: meta.description.unwrap_or_default(),
        topics:      meta.topics,
        gh_auth_ok:  true,
    });
    Ok(())
}

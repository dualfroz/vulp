use super::state::{CodeIntelMsg, CreateRepoMsg, HubState, Modal, SelectTag};

pub fn spawn_create_repo(
    state: &mut HubState,
    path: std::path::PathBuf,
    display_name: String,
    initial_lines: Vec<String>,
    commands: Vec<String>,
    owner: Option<String>,
) {
    if state.create_repo_rx.is_some() {
        state.status_msg = Some("A create/publish/PR/issue job is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<CreateRepoMsg>();
    state.create_repo_rx = Some(rx);
    state.modal = Modal::Info {
        title: format!("Creating '{}'", display_name),
        lines: initial_lines,
        scroll: 0,
    };

    let cfg = state.cfg.clone();
    let fallback_name = display_name;
    let private = cfg.default_visibility != "public";

    std::thread::spawn(move || {
        for cmd in &commands {
            let _ = tx.send(CreateRepoMsg::Progress(format!("$ {}", cmd)));
            let _ = std::process::Command::new("sh")
                .args(["-c", cmd.as_str()])
                .current_dir(&path)
                .output();
        }
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(CreateRepoMsg::Done {
                    success: false, name: fallback_name, detail: format!("runtime error: {e}"),
                });
                return;
            }
        };
        let tx_progress = tx.clone();
        let result = rt.block_on(crate::legacy::adopt::adopt_folder(
            &path, &cfg, private, "Initial commit", None, owner.as_deref(),
            move |s: &str| { let _ = tx_progress.send(CreateRepoMsg::Progress(s.to_string())); },
        ));
        let detail = if result.success {
            format!("Pushed to {}", result.url.unwrap_or_default())
        } else {
            result.detail
        };
        if result.success {
            crate::activity::log(&crate::activity::ActivityEntry {
                ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::NewRepo,
                repo: result.name.clone(), detail: detail.clone(),
            });
        }
        let _ = tx.send(CreateRepoMsg::Done { success: result.success, name: result.name, detail });
    });
}

pub fn spawn_publish_existing(state: &mut HubState, path: std::path::PathBuf, display_name: String, owner: Option<String>) {
    if state.create_repo_rx.is_some() {
        state.status_msg = Some("A create/publish/PR/issue job is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<CreateRepoMsg>();
    state.create_repo_rx = Some(rx);
    state.modal = Modal::Info {
        title: format!("Publishing '{}'", display_name),
        lines: vec!["Linked into workspace  ✓".into()],
        scroll: 0,
    };

    let cfg = state.cfg.clone();
    let fallback_name = display_name;
    let private = cfg.default_visibility != "public";

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(CreateRepoMsg::Done {
                    success: false, name: fallback_name, detail: format!("runtime error: {e}"),
                });
                return;
            }
        };
        let tx_progress = tx.clone();
        let result = rt.block_on(crate::legacy::adopt::publish_existing(
            &path, &cfg, private, None, owner.as_deref(),
            move |s: &str| { let _ = tx_progress.send(CreateRepoMsg::Progress(s.to_string())); },
        ));
        let detail = if result.success {
            format!("Pushed to {}", result.url.unwrap_or_default())
        } else {
            result.detail
        };
        if result.success {
            crate::activity::log(&crate::activity::ActivityEntry {
                ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::NewRepo,
                repo: result.name.clone(), detail: detail.clone(),
            });
        }
        let _ = tx.send(CreateRepoMsg::Done { success: result.success, name: result.name, detail });
    });
}

pub fn spawn_create_pr(state: &mut HubState, repo: String, title: String, head: String, base: String) {
    if state.create_repo_rx.is_some() {
        state.status_msg = Some("A create/publish/PR/issue job is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<CreateRepoMsg>();
    state.create_repo_rx = Some(rx);
    state.modal = Modal::Info {
        title: format!("Opening PR on '{}'", repo),
        lines: vec![format!("{} → {}", head, base)],
        scroll: 0,
    };

    let Some(account) = crate::providers::default_account(&state.accounts, None).cloned() else {
        state.modal = Modal::Info { title: "No account configured".into(), lines: vec!["Run: vulp auth import".into()], scroll: 0 };
        state.create_repo_rx = None;
        return;
    };

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => { let _ = tx.send(CreateRepoMsg::Done { success: false, name: repo, detail: format!("runtime error: {e}") }); return; }
        };
        let result = rt.block_on(async {
            let provider = crate::providers::make_provider(&account)?;
            provider.create_pull_request(&repo, &title, "", &head, &base).await
        });
        match result {
            Ok(pr) => {
                crate::activity::log(&crate::activity::ActivityEntry {
                    ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::Pr,
                    repo: repo.clone(), detail: format!("#{} {} (opened via vulp)", pr.number, title),
                });
                let _ = tx.send(CreateRepoMsg::Done { success: true, name: format!("PR #{}", pr.number), detail: pr.url });
            }
            Err(e) => { let _ = tx.send(CreateRepoMsg::Done { success: false, name: repo, detail: format!("{e:#}") }); }
        }
    });
}

pub fn spawn_create_issue(state: &mut HubState, repo: String, title: String) {
    if state.create_repo_rx.is_some() {
        state.status_msg = Some("A create/publish/PR/issue job is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<CreateRepoMsg>();
    state.create_repo_rx = Some(rx);
    state.modal = Modal::Info {
        title: format!("Opening issue on '{}'", repo),
        lines: vec![title.clone()],
        scroll: 0,
    };

    let Some(account) = crate::providers::default_account(&state.accounts, None).cloned() else {
        state.modal = Modal::Info { title: "No account configured".into(), lines: vec!["Run: vulp auth import".into()], scroll: 0 };
        state.create_repo_rx = None;
        return;
    };

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => { let _ = tx.send(CreateRepoMsg::Done { success: false, name: repo, detail: format!("runtime error: {e}") }); return; }
        };
        let result = rt.block_on(async {
            let provider = crate::providers::make_provider(&account)?;
            provider.create_issue(&repo, &title, "").await
        });
        match result {
            Ok(iss) => {
                crate::activity::log(&crate::activity::ActivityEntry {
                    ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::Issue,
                    repo: repo.clone(), detail: format!("#{} {} (opened via vulp)", iss.number, title),
                });
                let _ = tx.send(CreateRepoMsg::Done { success: true, name: format!("Issue #{}", iss.number), detail: iss.url });
            }
            Err(e) => { let _ = tx.send(CreateRepoMsg::Done { success: false, name: repo, detail: format!("{e:#}") }); }
        }
    });
}

fn spawn_codeintel<F>(state: &mut HubState, busy_msg: &str, work: F)
where
    F: FnOnce() -> CodeIntelMsg + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel::<CodeIntelMsg>();
    state.codeintel_rx = Some(rx);
    state.modal         = Modal::None;
    state.status_msg    = Some(busy_msg.to_string());
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
}

fn format_function_lines(funcs: &[crate::codeintel::FunctionInfo], format: crate::codeintel::FuncFormat) -> Vec<String> {
    use crate::codeintel::FuncFormat;
    funcs.iter().map(|f| match format {
        FuncFormat::NamesOnly => f.name.clone(),
        FuncFormat::WithArgs  => format!("{}({})", f.name, f.args.join(", ")),
        FuncFormat::WithMeta  => format!(
            "{}({})  [{}:{}, {} lines, {} ref{}]",
            f.name, f.args.join(", "), f.file, f.line, f.length_lines, f.ref_count,
            if f.ref_count == 1 { "" } else { "s" },
        ),
    }).collect()
}

pub fn spawn_list_functions(state: &mut HubState, repo: std::path::PathBuf, name: String, format: crate::codeintel::FuncFormat) {
    spawn_codeintel(state, "Parsing functions...", move || {
        let funcs = crate::codeintel::list_functions(&repo, format);
        let mut lines = format_function_lines(&funcs, format);
        if lines.is_empty() { lines.push("No functions found (unsupported language(s)?)".into()); }
        CodeIntelMsg::Info { title: format!("Functions — '{}' ({})", name, funcs.len()), lines }
    });
}

pub fn spawn_find_usages(state: &mut HubState, repo: std::path::PathBuf, name: String, ident: String) {
    spawn_codeintel(state, "Finding usages...", move || {
        let usages = crate::codeintel::find_usages(&repo, &ident);
        let mut lines: Vec<String> = usages.iter()
            .map(|u| format!("{}:{}:{}  {}", u.file, u.line, u.col, u.context))
            .collect();
        if lines.is_empty() { lines.push(format!("No usages of '{}' found", ident)); }
        CodeIntelMsg::Info { title: format!("Usages of '{}' in '{}' ({})", ident, name, usages.len()), lines }
    });
}

pub fn spawn_repo_stats(state: &mut HubState, repo: std::path::PathBuf, name: String) {
    spawn_codeintel(state, "Computing repo stats...", move || {
        let s = crate::codeintel::repo_stats(&repo);
        let mut lines = vec![
            format!("Functions: {}   Files: {}   Avg fn length: {:.1} lines", s.total_functions, s.total_files, s.avg_fn_len),
            String::new(),
            "Languages:".to_string(),
        ];
        for (lang, count) in &s.lang_counts {
            lines.push(format!("  {} — {} file{}", lang, count, if *count == 1 { "" } else { "s" }));
        }
        lines.push(String::new());
        lines.push("Longest functions:".to_string());
        for (file, fname, len) in &s.longest_functions {
            lines.push(format!("  {} ({}:{} lines)", fname, file, len));
        }
        lines.push(String::new());
        lines.push("Largest files:".to_string());
        for (file, loc) in &s.largest_files {
            lines.push(format!("  {} ({} lines)", file, loc));
        }
        CodeIntelMsg::Info { title: format!("Code stats — '{}'", name), lines }
    });
}

fn rename_plan_preview_lines(plan: &crate::codeintel::RenamePlan) -> Vec<String> {
    let mut lines = vec![format!("{} edit(s) across {} file(s):", plan.edits.len(), plan.file_count()), String::new()];
    for e in plan.edits.iter().take(200) {
        lines.push(format!(
            "{}:{}:{}  {} → {}",
            e.file.display(), e.line, e.col, e.old_text, e.new_text,
        ));
    }
    if plan.edits.len() > 200 {
        lines.push(format!("... and {} more", plan.edits.len() - 200));
    }
    lines
}

pub fn spawn_rename_preview(state: &mut HubState, repo: std::path::PathBuf, name: String, old: String, new: String) {
    spawn_codeintel(state, "Building rename plan...", move || {
        let plan = crate::codeintel::build_rename_plan(&repo, &old, &new);
        if plan.edits.is_empty() {
            return CodeIntelMsg::Info {
                title: format!("Rename — '{}'", name),
                lines: vec![format!("No occurrences of '{}' found in '{}'", old, name)],
            };
        }
        let lines = rename_plan_preview_lines(&plan);
        CodeIntelMsg::Preview { title: format!("Rename '{}' → '{}' in '{}' — dry run", old, new, name), lines, plan }
    });
}

pub fn spawn_convention_preview(state: &mut HubState, repo: std::path::PathBuf, name: String, target: crate::codeintel::NamingConvention) {
    spawn_codeintel(state, "Building conversion plan...", move || {
        let plan = crate::codeintel::build_convention_plan(&repo, target);
        if plan.edits.is_empty() {
            return CodeIntelMsg::Info {
                title: format!("Convert naming — '{}'", name),
                lines: vec![format!("All function names already match {}", target.label())],
            };
        }
        let lines = rename_plan_preview_lines(&plan);
        CodeIntelMsg::Preview { title: format!("Convert to {} in '{}' — dry run", target.label(), name), lines, plan }
    });
}

pub fn spawn_profile_push(state: &mut HubState, snap: crate::profile::SettingsSnapshot) {
    if state.create_repo_rx.is_some() {
        state.status_msg = Some("A create/publish/PR/issue job is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<CreateRepoMsg>();
    state.create_repo_rx = Some(rx);
    let repo_name = crate::profile::SETTINGS_REPO_NAME.to_string();
    state.modal = Modal::Info {
        title: format!("Pushing settings to '{}'", repo_name),
        lines: vec!["Preparing snapshot...".into()],
        scroll: 0,
    };

    let cfg = state.cfg.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(CreateRepoMsg::Done {
                    success: false, name: repo_name, detail: format!("runtime error: {e}"),
                });
                return;
            }
        };
        let _ = tx.send(CreateRepoMsg::Progress("Pushing...".into()));
        match rt.block_on(crate::profile::push_to_repo(&snap, &cfg, &repo_name)) {
            Ok(url)  => { let _ = tx.send(CreateRepoMsg::Done { success: true, name: repo_name, detail: format!("Pushed to {url}") }); }
            Err(e)   => { let _ = tx.send(CreateRepoMsg::Done { success: false, name: repo_name, detail: format!("{e:#}") }); }
        }
    });
}

pub fn spawn_profile_pull(state: &mut HubState) {
    use super::state::ProfilePullMsg;
    if state.profile_pull_rx.is_some() {
        state.status_msg = Some("A settings pull is already running — wait for it to finish.".into());
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel::<ProfilePullMsg>();
    state.profile_pull_rx = Some(rx);
    let repo_name = crate::profile::SETTINGS_REPO_NAME.to_string();
    state.modal = Modal::Info {
        title: format!("Pulling settings from '{}'", repo_name),
        lines: vec!["Fetching...".into()],
        scroll: 0,
    };

    let cfg = state.cfg.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(ProfilePullMsg::Done(Err(format!("runtime error: {e}"))));
                return;
            }
        };
        let result = rt.block_on(crate::profile::pull_from_repo(&cfg, &repo_name));
        let _ = tx.send(ProfilePullMsg::Done(result.map_err(|e| format!("{e:#}"))));
    });
}

pub fn open_bulk_import_picker(state: &mut HubState, acc_idx: usize) {
    state.modal = Modal::Multi {
        title: "Bulk Import — repo categories".into(),
        options: vec![
            "Repos I own".into(),
            "Repos I collaborate on".into(),
            "Repos from orgs I'm a member of".into(),
        ],
        checks: vec![true, false, false],
        cursor: 0,
        tag: super::state::MultiTag::BulkImportAffiliation(acc_idx),
    };
}

pub fn spawn_bulk_import(state: &mut HubState, acc_idx: usize, affiliation: crate::providers::RepoAffiliation) {
    if state.pull_all_rx.is_some() {
        state.status_msg = Some("A workspace job is already running — wait for it to finish.".into());
        return;
    }
    let Some(acc) = state.accounts.get(acc_idx).cloned() else { return; };
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    state.pull_all_rx = Some(rx);
    state.status_msg  = Some(format!("Fetching repos for @{}...", acc.username));

    let workspace = state.workspace_root.clone();
    let cfg = state.cfg.clone();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = tx.send(format!("ERROR: runtime error: {e}"));
                let _ = tx.send("DONE".into());
                return;
            }
        };
        rt.block_on(async {
            let provider = match crate::providers::make_provider(&acc) {
                Ok(p) => p,
                Err(e) => { let _ = tx.send(format!("ERROR: {e:#}")); let _ = tx.send("DONE".into()); return; }
            };
            let repos = match provider.list_repos(&affiliation).await {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(format!("ERROR: {e:#}")); let _ = tx.send("DONE".into()); return; }
            };
            let total = repos.len();
            let _ = tx.send(format!("Found {} repos for @{} ({})", total, acc.username, acc.provider));

            if repos.iter().any(|r| r.name == crate::profile::SETTINGS_REPO_NAME) {
                let _ = tx.send(format!("Found your '{}' on this account — importing settings...", crate::profile::SETTINGS_REPO_NAME));
                match crate::profile::pull_from_repo(&cfg, crate::profile::SETTINGS_REPO_NAME).await {
                    Ok(snap) => {
                        let mut cfg_mut = cfg.clone();
                        match crate::profile::apply(&snap, &mut cfg_mut) {
                            Ok(()) => { let _ = tx.send("SETTINGS_IMPORTED".into()); }
                            Err(e) => { let _ = tx.send(format!("Settings fetched but failed to apply: {e:#}")); }
                        }
                    }
                    Err(e) => { let _ = tx.send(format!("Settings import skipped: {e:#}")); }
                }
            }

            let no_prompt = |cmd: &mut std::process::Command| {
                cmd.env("GIT_TERMINAL_PROMPT", "0")
                   .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10")
                   .stdin(std::process::Stdio::null())
                   .stdout(std::process::Stdio::null())
                   .stderr(std::process::Stdio::null());
            };
            let run_with_timeout = |cmd: &mut std::process::Command| -> bool {
                let Ok(mut child) = cmd.spawn() else { return false };
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => return status.success(),
                        Ok(None) => {
                            if std::time::Instant::now() >= deadline {
                                let _ = child.kill();
                                let _ = child.wait();
                                return false;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                        Err(_) => return false,
                    }
                }
            };
            for (i, repo) in repos.iter().enumerate() {
                let target = workspace.join(&repo.name);
                let https_url = format!("{}.git", repo.url);
                let url = if !repo.ssh_url.is_empty() { &repo.ssh_url } else { &https_url };
                let action = if target.exists() {
                    let mut cmd = std::process::Command::new("git");
                    cmd.args(["pull"]).current_dir(&target);
                    no_prompt(&mut cmd);
                    if run_with_timeout(&mut cmd) { "pulled" } else { "pull failed" }
                } else {
                    let mut cmd = std::process::Command::new("git");
                    cmd.args(["clone", url.as_str()]).current_dir(&workspace);
                    no_prompt(&mut cmd);
                    if run_with_timeout(&mut cmd) { "cloned" } else { "clone failed" }
                };
                let _ = tx.send(format!("[{}/{}] {} {}", i + 1, total, action, repo.name));
            }
            let _ = tx.send("DONE".into());
        });
    });
}

pub fn fetch_orgs_blocking(state: &HubState) -> Vec<String> {
    let accounts = crate::auth::load_accounts(&state.cfg).unwrap_or_default();
    let Some(acc) = crate::providers::default_account(&accounts, Some(&state.cfg.default_provider)).cloned() else {
        return vec![];
    };
    let handle = std::thread::spawn(move || {
        let Ok(provider) = crate::providers::make_provider(&acc) else { return vec![]; };
        let Ok(rt) = tokio::runtime::Runtime::new() else { return vec![]; };
        rt.block_on(provider.list_orgs()).unwrap_or_default()
    });
    handle.join().unwrap_or_default()
}

pub fn open_template_picker(state: &mut HubState, name: String, owner: Option<String>) {
    let mut opts = vec!["Blank (no template)".to_string()];
    for t in &state.templates {
        let n = t.commands.len();
        opts.push(format!("{:<20} ({} cmd{})", t.name, n, if n == 1 { "" } else { "s" }));
    }
    let title = match &owner {
        Some(o) => format!("Create '{}' in @{} — Template", name, o),
        None    => format!("Create '{}' — Template", name),
    };
    state.modal = Modal::Select {
        title,
        options: opts,
        cursor: 0,
        tag: SelectTag::InitTemplate { name, owner },
    };
}

pub fn open_codeintel_picker(state: &mut HubState) {
    if state.repos.is_empty() {
        state.status_msg = Some("No repos in workspace".into());
        return;
    }
    let options = state.repos.iter().map(|r| r.name.clone()).collect();
    state.modal = Modal::Select {
        title: "Code Intelligence — pick repo".into(),
        options,
        cursor: 0,
        tag: SelectTag::CodeIntelRepoPick,
    };
}

pub fn open_action_menu(state: &mut HubState) {
    use super::state::{MenuAction, Tab};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let k  = |c: char| MenuAction::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    let kc = |code: KeyCode| MenuAction::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let entry = |label: &str, hint: &str, action: MenuAction| {
        (format!("{:<34}[{}]", label, hint), action)
    };

    let mut entries: Vec<(String, MenuAction)> = Vec::new();

    if state.cur_repo_idx().is_some() {
        entries.push(entry("Open repo detail",       "↵",     kc(KeyCode::Enter)));
        entries.push(entry("Sync (pull / push)",     "s",     k('s')));
        entries.push(entry("Create release",         "r",     k('r')));
        entries.push(entry("Open in editor",         "o",     k('o')));
        entries.push(entry("Open in file manager",   "O",     k('O')));
        entries.push(entry("Open in browser",        "W",     k('W')));
        entries.push(entry("Copy path",              "y",     k('y')));
        entries.push(entry("Copy remote URL",        "u",     k('u')));
        entries.push(entry("Copy URL (SSH/HTTPS)",   "^u",
            MenuAction::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))));
        entries.push(entry("Toggle bookmark",        "B",     k('B')));
        entries.push(entry("Toggle pin",             "P",     k('P')));
        entries.push(entry("Set display label",      "n",     k('n')));
        entries.push(entry("Edit .gitignore",        "g",     k('g')));
        entries.push(entry("Select for batch",       "Space", kc(KeyCode::Char(' '))));
        if let Some(ri) = state.cur_repo_idx() {
            if state.repos[ri].is_dirty {
                entries.push(entry("Stash changes",   "", MenuAction::StashSave(ri)));
            }
            if state.repos[ri].stash_count > 0 {
                entries.push(entry("Pop stash",       "", MenuAction::StashPop(ri)));
            }
        }
    } else if state.tab == Tab::Projects && state.cur_proj_idx().is_some() {
        entries.push(entry("Open project detail",    "↵",     kc(KeyCode::Enter)));
        entries.push(entry("Delete project",         "Del",   kc(KeyCode::Delete)));
    }

    if state.action_targets().iter().any(|&ri| !state.repos[ri].has_license) {
        entries.push(entry("Add LICENSE (MIT/Apache)", "", MenuAction::AddLicense));
    }

    match state.tab {
        Tab::Todos => {
            entries.push(entry("Cycle todo status",   "Space", kc(KeyCode::Char(' '))));
            entries.push(entry("Expand/collapse group","↵",    kc(KeyCode::Enter)));
            entries.push(entry("Edit todo",           "e",     k('e')));
            entries.push(entry("Delete todo",         "Del",   kc(KeyCode::Delete)));
            entries.push(entry("New todo",            "+",     k('+')));
            entries.push(entry("Cycle status view",   "s",     k('s')));
            entries.push(entry("Cycle grouping",      "g",     k('g')));
            entries.push(entry("Toggle code todos",   "c",     k('c')));
            entries.push(entry("Scan repos for TODOs","T",     k('T')));
        }
        Tab::PRs => {
            entries.push(entry("Open PR in browser",  "↵",     kc(KeyCode::Enter)));
            entries.push(entry("New PR",              "",      MenuAction::NewPr));
        }
        Tab::Issues => {
            entries.push(entry("Open issue in browser","↵",    kc(KeyCode::Enter)));
            entries.push(entry("New issue",           "",      MenuAction::NewIssue));
        }
        Tab::Overview => {
            entries.push(entry("New PR",              "",      MenuAction::NewPr));
            entries.push(entry("New issue",           "",      MenuAction::NewIssue));
            entries.push(entry("Open account profile","",      MenuAction::OpenAccountProfile));
            entries.push(entry("View release notes",  "v",     k('v')));
        }
        Tab::Stats => {
            entries.push(entry("Code intelligence",   "",      MenuAction::CodeIntelPicker));
            entries.push(entry("Cycle stats period",  "p",     k('p')));
        }
        _ => {}
    }

    entries.push(entry("New repository",        "N", k('N')));
    if state.tab != Tab::Stats {
        entries.push(entry("Project menu",      "p", k('p')));
    }
    entries.push(entry("Sort & filter",         "S", k('S')));
    entries.push(entry("Select by condition",   "*", k('*')));
    entries.push(entry("Refresh from API",      "R", k('R')));
    entries.push(entry("Help / keybindings",    "?", k('?')));

    let (options, actions): (Vec<String>, Vec<MenuAction>) = entries.into_iter().unzip();
    state.modal = Modal::Select {
        title: "Actions".into(),
        options,
        cursor: 0,
        tag: SelectTag::ActionMenu(actions),
    };
}

pub fn spawn_digest_preview(state: &mut HubState) {
    use super::state::DigestMsg;

    if state.digest_rx.is_some() {
        state.status_msg = Some("Digest already loading...".into());
        return;
    }
    let repos = state.repos.clone();
    let cfg = state.cfg.clone();
    let accounts = crate::auth::load_accounts(&cfg).unwrap_or_default();

    let (tx, rx) = std::sync::mpsc::channel::<DigestMsg>();
    state.digest_rx = Some(rx);
    state.status_msg = Some("Building weekly digest...".into());

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() { Ok(rt) => rt, Err(_) => return };
        rt.block_on(async move {
            let matcher = crate::stats::OwnEmailMatcher::resolve(&cfg, &accounts);
            let default_account = accounts.iter().find(|a| a.is_default).or(accounts.first());

            let mut owners: std::collections::HashSet<String> =
                repos.iter().filter_map(|r| r.owner.clone()).collect();
            if let Some(acc) = default_account { owners.insert(acc.username.clone()); }
            let scope = owners.iter().map(|o| format!("user:{o}")).collect::<Vec<_>>().join(" ");

            let provider = default_account.and_then(|acc| crate::providers::make_provider(acc).ok());
            let d = crate::digest::build(&repos, &matcher, provider.as_deref(), &scope).await;
            let lines: Vec<String> = crate::digest::render_plaintext(&d)
                .lines().map(|s| s.to_string()).collect();
            let _ = tx.send(DigestMsg { lines });
        });
    });
}

pub fn spawn_api_refresh(state: &mut HubState) {
    use super::state::RefreshMsg;

    if state.refresh_rx.is_some() {
        state.status_msg = Some("Refresh already running...".into());
        return;
    }
    let accounts = match crate::auth::load_accounts(&state.cfg) {
        Ok(a) if !a.is_empty() => a,
        _ => { state.status_msg = Some("No accounts configured — vulp auth add".into()); return; }
    };
    let repos: Vec<(String, Option<String>, String, String)> = state.repos.iter()
        .map(|r| {
            let basename = r.remote_repo_name.clone().unwrap_or_else(|| {
                r.path.file_name().map(|b| b.to_string_lossy().to_string()).unwrap_or_else(|| r.name.clone())
            });
            (r.name.clone(), r.owner.clone(), basename, r.branch.clone())
        })
        .collect();
    let repo_paths: Vec<(String, std::path::PathBuf)> = state.repos.iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect();
    let cfg = state.cfg.clone();

    let (tx, rx) = std::sync::mpsc::channel::<RefreshMsg>();
    state.refresh_rx = Some(rx);
    state.status_msg = Some("Refreshing from API...".into());

    std::thread::spawn(move || {
        use futures::stream::{self, StreamExt};
        let rt = match tokio::runtime::Runtime::new() { Ok(rt) => rt, Err(_) => return };
        rt.block_on(async move {
            let mut metas   = Vec::new();
            let mut prs     = Vec::new();
            let mut issues  = Vec::new();
            let mut closed_counts = None;

            let account = accounts.iter().find(|a| a.is_default).or(accounts.first()).cloned();
            let Some(account) = account else { return };
            let Ok(provider) = crate::providers::make_provider(&account) else {
                let _ = tx.send(RefreshMsg::Done { metas, prs, issues, closed_counts });
                return;
            };
            let provider: std::sync::Arc<dyn crate::providers::Provider> = std::sync::Arc::from(provider);

            let targets: Vec<(String, String, String)> = repos.iter()
                .filter_map(|(name, owner, basename, branch)| {
                    owner.as_ref().map(|o| (name.clone(), format!("{}/{}", o, basename), branch.clone()))
                })
                .collect();
            let total = targets.len();
            let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

            let mut dependabot: std::collections::HashMap<String, Option<std::collections::HashMap<String, u32>>> =
                std::collections::HashMap::new();
            {
                let owners: std::collections::HashSet<String> =
                    repos.iter().filter_map(|(_, o, _, _)| o.clone()).collect();
                for owner in owners {
                    let _ = tx.send(RefreshMsg::Progress(format!("Dependabot alerts: {}", owner)));
                    let counts = provider.org_dependabot_counts(&owner).await.ok().flatten();
                    dependabot.insert(owner.to_lowercase(), counts);
                }
            }

            let deploy_heads = if crate::deploy::is_configured(&cfg) {
                let _ = tx.send(RefreshMsg::Progress("Checking deploy drift...".into()));
                crate::deploy::scan_deploy_heads(&cfg)
            } else {
                None
            };

            let results: Vec<_> = stream::iter(targets.into_iter().map(|(name, slug, branch)| {
                let provider = provider.clone();
                let tx = tx.clone();
                let done = done.clone();
                async move {
                    let meta = provider.repo_meta(&slug).await.ok();
                    let p    = provider.list_pull_requests(&slug).await.ok();
                    let i    = provider.list_issues(&slug).await.ok();
                    let ci   = provider.latest_ci_status(&slug, &branch).await.ok().flatten();
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let _ = tx.send(RefreshMsg::Progress(format!("Refreshed {}/{}: {}", n, total, name)));
                    (name, slug, meta, p, i, ci)
                }
            }))
            .buffer_unordered(6)
            .collect()
            .await;

            for (name, slug, meta, p, i, ci) in results {
                let can_merge = meta.as_ref().map(|m| m.viewer_can_merge).unwrap_or(false);
                if let Some(meta) = meta {
                    let slug_lc = slug.to_lowercase();
                    let dep = dependabot
                        .get(slug_lc.split('/').next().unwrap_or(""))
                        .and_then(|per_org| per_org.as_ref()
                            .map(|m| m.get(&slug_lc).copied().unwrap_or(0)));
                    let deploy_status = deploy_heads.as_ref().and_then(|heads| {
                        let path = &repo_paths.iter().find(|(n, _)| n == &name)?.1;
                        crate::deploy::drift_for(path, &cfg.workspace_root, heads)
                    }).map(|s| s.as_str());
                    metas.push((name.clone(), crate::cache::RepoMeta {
                        stars:           Some(meta.stars),
                        open_issues:     Some(meta.open_issues),
                        open_prs:        Some(meta.open_prs),
                        visibility:      None,
                        ci_status:       ci.map(|c| c.as_str().to_string()),
                        dependabot_open: dep,
                        deploy_status,
                        fetched_at:      chrono::Utc::now().timestamp(),
                    }));
                }
                if let Some(mut p) = p {
                    for pr in &mut p { pr.viewer_can_merge = can_merge; }
                    prs.push((name.clone(), p));
                }
                if let Some(i) = i { issues.push((name, i)); }
            }

            {
                let mut owners: std::collections::HashSet<String> =
                    repos.iter().filter_map(|(_, o, _, _)| o.clone()).collect();
                owners.insert(account.username.clone());
                let scope = owners.iter().map(|o| format!("user:{}", o)).collect::<Vec<_>>().join(" ");
                let _ = tx.send(RefreshMsg::Progress("Counting merged/closed...".into()));
                let merged = provider.count_search(&format!("is:pr is:merged {}", scope)).await.unwrap_or(0);
                let closed = provider.count_search(&format!("is:pr is:closed is:unmerged {}", scope)).await.unwrap_or(0);
                let ciss   = provider.count_search(&format!("is:issue is:closed {}", scope)).await.unwrap_or(0);
                closed_counts = Some((merged, closed, ciss));
            }

            let _ = tx.send(RefreshMsg::Progress("Measuring release cadence + repo size...".into()));
            let matcher = crate::stats::OwnEmailMatcher::resolve(&cfg, &accounts);
            let mut stats_cache = crate::stats::cache::load();
            for (name, path) in &repo_paths {
                let size = crate::stats::size::repo_size(path);
                stats_cache.record_size(name, size.git_kb, size.tree_kb);
                stats_cache.record_authored(name, crate::stats::has_own_commit(path, &matcher));
            }
            let _ = crate::stats::cache::save(&stats_cache);

            let _ = tx.send(RefreshMsg::Done { metas, prs, issues, closed_counts });
        });
    });
}

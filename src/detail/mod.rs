mod git;
mod render;

pub use git::{activity_1d, activity_7d, activity_30d, activity_90d, activity_1y, activity_all};

pub fn build_history_pub(path: &std::path::Path, limit: usize) -> Vec<HistEntry> {
    git::build_history(path, limit)
}

use crate::config::AppConfig;
use crate::status::{count_tracked_files, repo_detail, repo_recent_commits, RepoSummary};
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use git2::Repository;
use git::{
    build_history, build_tree, checkout_branch, copy_to_clipboard, count_lines_of_code,
    count_stashes, count_tags, delete_branch, git_run, git_run_ok, largest_files,
    list_all_files, list_branches, list_contributors, push_branch, read_file_preview,
    remote_url, repo_first_commit_ts, to_web_url, total_commits,
};
use render::draw;
use std::path::PathBuf;

// ── history entry ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum HistEntry {
    Commit { sha: String, age: String, ts: i64, author: String, msg: String },
    Tag    { name: String, age: String, ts: i64, msg: String },
}

impl HistEntry {
    pub fn ts(&self) -> i64 { match self { Self::Commit { ts, .. } | Self::Tag { ts, .. } => *ts } }
}

// ── new data structures ───────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BranchEntry {
    pub name:             String,
    pub is_current:       bool,
    pub upstream:         Option<String>,
    pub ahead:            usize,
    pub behind:           usize,
    pub last_commit_age:  String,
    pub last_commit_msg:  String,
}

#[derive(Clone)]
pub struct TreeNode {
    pub name:     String,
    pub rel_path: String,
    pub is_dir:   bool,
    pub depth:    usize,
    pub expanded: bool,
}

#[derive(Clone)]
pub struct ContribEntry {
    pub name:  String,
    pub count: usize,
}

#[derive(Clone)]
pub struct MonthlyCommits {
    pub label: String,
    pub count: usize,
}

pub struct RepoSettings {
    pub visibility:  String,
    pub description: String,
    pub topics:      Vec<String>,
    pub gh_auth_ok:  bool,
}

// ── stats period ──────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum StatsPeriod { Day1, Day7, Day30, Day90, Year1, AllTime }

impl StatsPeriod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Day1    => "1d",
            Self::Day7    => "7d",
            Self::Day30   => "30d",
            Self::Day90   => "90d",
            Self::Year1   => "1y",
            Self::AllTime => "all",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Day1    => Self::Day7,
            Self::Day7    => Self::Day30,
            Self::Day30   => Self::Day90,
            Self::Day90   => Self::Year1,
            Self::Year1   => Self::AllTime,
            Self::AllTime => Self::Day1,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Day1    => Self::AllTime,
            Self::Day7    => Self::Day1,
            Self::Day30   => Self::Day7,
            Self::Day90   => Self::Day30,
            Self::Year1   => Self::Day90,
            Self::AllTime => Self::Year1,
        }
    }
}

// ── Settings API operations (sent to background thread) ──────────────────────

pub enum SettingOp {
    Visibility { private: bool },
    Description { text: String },
    Topics { topics: Vec<String> },
    AddCollaborator { name: String, role: String },
    Archive,
    Rename { new_name: String },
    Delete,
}

// ── TUI modals ────────────────────────────────────────────────────────────────

pub enum DetailModal {
    None,
    NewBranch { input: String, error: Option<String> },
    // generic option selector (tag identifies the action)
    SelectOption { title: String, options: Vec<String>, cursor: usize, tag: u8 },
    // generic text input (tag identifies the action)
    EditText { title: String, prompt: String, input: String, tag: u8 },
    // add-collaborator step 2: name collected, now pick role
    SelectRole { collab_name: String, cursor: usize },
    // dangerous confirm (archive, delete)
    ConfirmAction { message: String, tag: u8 },
}

// ── tabs ──────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum Tab { Overview, Commits, Branches, Files, Stats, Settings, PRs, Issues, Todos }

impl Tab {
    fn next(self) -> Self {
        match self {
            Self::Overview  => Self::Commits,
            Self::Commits   => Self::Branches,
            Self::Branches  => Self::Files,
            Self::Files     => Self::Stats,
            Self::Stats     => Self::Settings,
            Self::Settings  => Self::PRs,
            Self::PRs       => Self::Issues,
            Self::Issues    => Self::Todos,
            Self::Todos     => Self::Overview,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::Overview  => Self::Todos,
            Self::Commits   => Self::Overview,
            Self::Branches  => Self::Commits,
            Self::Files     => Self::Branches,
            Self::Stats     => Self::Files,
            Self::Settings  => Self::Stats,
            Self::PRs       => Self::Settings,
            Self::Issues    => Self::PRs,
            Self::Todos     => Self::Issues,
        }
    }
}

// ── state ─────────────────────────────────────────────────────────────────────

pub struct St {
    pub summary:        RepoSummary,
    pub tab:            Tab,
    pub pending_file:   Option<PathBuf>,

    // Commits tab
    pub history:        Vec<HistEntry>,
    pub hist_off:       usize,
    pub total_commits:  usize,

    // Files (changed)
    pub files:          Vec<(char, String)>,
    pub file_count:     usize,

    // Branches tab
    pub branches:       Vec<BranchEntry>,
    pub branch_cursor:  usize,
    pub branch_off:     usize,

    // Files tab (tree)
    pub all_files:      Vec<String>,
    pub tree_nodes:     Vec<TreeNode>,
    pub tree_visible:   Vec<usize>,
    pub tree_cursor:    usize,
    pub tree_off:       usize,
    pub file_preview:   Vec<String>,
    pub preview_off:    usize,

    // Stats tab
    pub contributors:   Vec<ContribEntry>,
    pub repo_age_days:  u64,
    pub stats_off:      usize,
    pub stats_period:   StatsPeriod,
    pub activity_data:  Vec<(String, usize)>,   // current period's activity
    pub loc_total:      usize,
    pub top_files:      Vec<(String, usize)>,   // largest files by LOC

    // Tags/stashes (shared)
    pub total_tags:     usize,
    pub stash_count:    usize,

    // Overview tab
    pub ov_recent:      Vec<(String, String)>,
    pub ov_scroll:      usize,
    pub remote_url_str: Option<String>,

    // Settings tab
    pub settings:       Option<RepoSettings>,
    pub settings_cur:   usize,
    pub settings_rx:    Option<std::sync::mpsc::Receiver<Result<RepoSettings, String>>>,
    pub settings_err:   Option<String>,
    pub api_rx:         Option<std::sync::mpsc::Receiver<Result<String, String>>>,

    // PRs / Issues / Todos tabs
    pub repo_prs:       Vec<crate::providers::PrDetail>,
    pub repo_issues:    Vec<crate::providers::IssueDetail>,
    pub repo_todos:     Vec<crate::todos::Todo>,
    pub pr_cursor:      usize,
    pub issue_cursor:   usize,
    pub todo_cursor:    usize,
    pub prs_loaded:     bool,
    pub prs_issues_rx:  Option<std::sync::mpsc::Receiver<(Vec<crate::providers::PrDetail>, Vec<crate::providers::IssueDetail>)>>,

    // Shared
    pub branch_count:   usize,
    pub quit:           bool,
    pub run_action:     Option<usize>,
    pub status_msg:     Option<String>,
    pub cfg_data_dir:   PathBuf,

    // Pending values collected from in-TUI modals (sync / release)
    pub pending_sync_mode:   Option<String>,
    pub pending_commit_msg:  Option<String>,
    pub pending_tag_name:    Option<String>,
    pub pending_tag_msg:     Option<String>,
    pub pending_push_tag:    bool,

    // Modal
    pub modal:          DetailModal,
}

// ── tree visibility rebuild ───────────────────────────────────────────────────

pub fn rebuild_tree_visible(nodes: &[TreeNode]) -> Vec<usize> {
    let mut visible = vec![];
    let mut collapsed_depth: Option<usize> = None;
    for (i, node) in nodes.iter().enumerate() {
        if let Some(cd) = collapsed_depth {
            if node.depth > cd { continue; }
            else { collapsed_depth = None; }
        }
        visible.push(i);
        if node.is_dir && !node.expanded {
            collapsed_depth = Some(node.depth);
        }
    }
    visible
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(
    summary:   RepoSummary,
    cfg:       &AppConfig,
    maybe_term: Option<crate::tui::Tty>,
) -> Result<()> {
    let path = summary.path.clone();
    let name = summary.name.clone();

    // Reuse the handed terminal (no LeaveAlternateScreen flash) or create a
    // new one. Either way, TUI is live before any blocking git calls.
    let mut terminal = match maybe_term {
        Some(t) => t,
        None    => crate::tui::setup()?,
    };

    // Show "Loading…" while we run the blocking git subprocesses.
    terminal.draw(|f| {
        use ratatui::widgets::Paragraph;
        f.render_widget(
            Paragraph::new(format!("  Loading {}…", name))
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)),
            f.area(),
        );
    })?;

    let detail        = repo_detail(&path).unwrap_or_else(|_| crate::status::RepoDetail {
        summary:        summary.clone(),
        modified_files: vec![],
        recent_commits: vec![],
        meta_issues:    None, meta_prs: None, meta_stars: None,
    });
    let all_files    = list_all_files(&path);
    let tree_nodes   = build_tree(&all_files);
    let tree_visible = rebuild_tree_visible(&tree_nodes);
    let contributors = list_contributors(&path);
    let branches     = list_branches(&path);
    let branch_count = branches.len().max(1);
    let first_ts     = repo_first_commit_ts(&path).unwrap_or(0);
    let repo_age_days = ((chrono::Utc::now().timestamp() - first_ts) / 86400).max(0) as u64;
    let total        = total_commits(&path);
    let url_str      = remote_url(&path);
    let loc          = count_lines_of_code(&path);
    let top_f        = largest_files(&path, 5);
    let activity     = activity_1y(&path);

    let mut st = St {
        tab:            Tab::Overview,
        files:          detail.modified_files,
        file_count:     count_tracked_files(&path),
        total_tags:     count_tags(&path),
        stash_count:    count_stashes(&path),
        branch_count,
        history:        build_history(&path, 200),
        hist_off:       0,
        total_commits:  total,
        branches,
        branch_cursor:  0,
        branch_off:     0,
        all_files,
        tree_nodes,
        tree_visible,
        tree_cursor:    0,
        tree_off:       0,
        file_preview:   vec![],
        preview_off:    0,
        contributors,
        repo_age_days,
        stats_off:      0,
        stats_period:   StatsPeriod::Year1,
        activity_data:  activity,
        loc_total:      loc,
        top_files:      top_f,
        ov_recent:      repo_recent_commits(&path, 6),
        ov_scroll:      0,
        remote_url_str: url_str,
        settings:       None,
        settings_cur:   0,
        settings_rx:    None,
        settings_err:   None,
        api_rx:         None,
        repo_prs:       vec![],
        repo_issues:    vec![],
        repo_todos:     {
            let rname = name.clone();
            crate::todos::load_manual().into_iter()
                .filter(|t| t.repo.as_deref() == Some(&rname))
                .collect()
        },
        pr_cursor:      0,
        issue_cursor:   0,
        todo_cursor:    0,
        prs_loaded:     false,
        prs_issues_rx:  None,
        quit:           false,
        run_action:     None,
        status_msg:     None,
        cfg_data_dir:   cfg.data_dir.clone(),
        pending_sync_mode:  None,
        pending_commit_msg: None,
        pending_tag_name:   None,
        pending_tag_msg:    None,
        pending_push_tag:   false,
        modal:          DetailModal::None,
        pending_file:   None,
        summary,
    };

    // Main loop — terminal stays alive the whole time.
    let result = run_detail_loop(&mut st, &mut terminal).await;
    crate::tui::teardown(terminal)?;
    result
}

async fn run_detail_loop(st: &mut St, terminal: &mut crate::tui::Tty) -> Result<()> {
    loop {
        let action = tui_phase_inner(st, terminal)?;
        if st.quit { break; }
        if let Some(a) = action {
            // Leave alternate screen for interactive prompts, then return.
            crate::tui::leave_tui(terminal)?;
            let r = exec_action(st, a).await;
            crate::tui::enter_tui(terminal)?;
            r?;
        }
    }
    Ok(())
}

fn tui_phase_inner(st: &mut St, terminal: &mut crate::tui::Tty) -> Result<Option<usize>> {
    let path = st.summary.path.clone();
    loop {
        poll_settings_rx(st);
        poll_api_rx(st);
        poll_prs_issues_rx(st);
        terminal.draw(|f| draw(f, st))?;
        let event = crate::tui::next_event(250)?;
        let key = match event { Some(Event::Key(k)) => k, _ => continue };

        if handle_modal_key(key.code, st) {
            if st.quit { return Ok(None); }
            if let Some(a) = st.run_action.take() {
                if a == 40 {
                    if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                        let node = &st.tree_nodes[ni];
                        if !node.is_dir {
                            st.file_preview = read_file_preview(&path, &node.rel_path, 200);
                        } else {
                            st.file_preview = vec![];
                        }
                        st.preview_off = 0;
                    }
                } else if a == 41 {
                    st.activity_data = compute_activity(&path, st.stats_period);
                } else if a == 50 {
                    spawn_settings_fetch(st);
                } else if a == 80 {
                    spawn_prs_issues_fetch(st);
                } else {
                    return Ok(Some(a));
                }
            }
            continue;
        }

        handle_key(key.code, st);
        if st.quit { return Ok(None); }

        if let Some(a) = st.run_action.take() {
            if a == 40 {
                if let Some(&ni) = st.tree_visible.get(st.tree_cursor) {
                    let node = &st.tree_nodes[ni];
                    if !node.is_dir {
                        st.file_preview = read_file_preview(&path, &node.rel_path, 200);
                    } else {
                        st.file_preview = vec![];
                    }
                    st.preview_off = 0;
                }
                continue;
            }
            if a == 41 {
                st.activity_data = compute_activity(&path, st.stats_period);
                continue;
            }
            if a == 50 {
                spawn_settings_fetch(st);
                continue;
            }
            if a == 80 {
                spawn_prs_issues_fetch(st);
                continue;
            }
            return Ok(Some(a));
        }
    }
}

fn compute_activity(path: &std::path::Path, period: StatsPeriod) -> Vec<(String, usize)> {
    match period {
        StatsPeriod::Day1    => activity_1d(path),
        StatsPeriod::Day7    => activity_7d(path),
        StatsPeriod::Day30   => activity_30d(path),
        StatsPeriod::Day90   => activity_90d(path),
        StatsPeriod::Year1   => activity_1y(path),
        StatsPeriod::AllTime => activity_all(path),
    }
}

fn settings_fallback(st: &St) -> RepoSettings {
    RepoSettings {
        visibility:  st.summary.visibility.clone().unwrap_or_else(|| "unknown".into()),
        description: String::new(),
        topics:      vec![],
        gh_auth_ok:  false,
    }
}

fn settings_fallback_auth(st: &St) -> RepoSettings {
    RepoSettings {
        visibility:  st.summary.visibility.clone().unwrap_or_else(|| "unknown".into()),
        description: String::new(),
        topics:      vec![],
        gh_auth_ok:  true,
    }
}

fn poll_settings_rx(st: &mut St) {
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
            let auth_fail = e.contains("No GitHub account") || e.contains("no account")
                || e.contains("no github") || e.contains("authentication");
            if auth_fail {
                st.settings     = Some(settings_fallback(st));
                st.settings_err = Some(e);
            } else {
                // API error, but we are authenticated — keep showing stale data, show error
                if st.settings.is_none() { st.settings = Some(settings_fallback_auth(st)); }
                st.settings_err = Some(format!("API error: {}", e));
            }
        }
        None => {}
    }
}

fn spawn_settings_fetch(st: &mut St) {
    if st.settings_rx.is_some() { return; } // already in flight
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
                .ok_or_else(|| "No GitHub account — run: fxg auth import".to_string())?;
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

// ── settings modal opener ─────────────────────────────────────────────────────

fn open_settings_modal(st: &mut St) {
    let cur_vis  = st.settings.as_ref().map(|s| s.visibility.clone()).or_else(|| st.summary.visibility.clone()).unwrap_or_default();
    let cur_desc = st.settings.as_ref().map(|s| s.description.clone()).unwrap_or_default();
    let cur_top  = st.settings.as_ref().map(|s| s.topics.join(", ")).unwrap_or_default();
    let repo     = st.summary.name.clone();
    match st.settings_cur {
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
                message: format!("Permanently delete '{}' on GitHub?\nThis cannot be undone.", name),
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
            if ok && st.stash_count > 0 { st.stash_count -= 1; }
        }
        _ => {}
    }
}

// ── settings API background operations ───────────────────────────────────────

fn spawn_settings_op(st: &mut St, op: SettingOp) {
    if st.api_rx.is_some() {
        st.status_msg = Some("Another operation in progress...".into());
        return;
    }
    let data_dir  = st.cfg_data_dir.clone();
    let repo_name = st.summary.name.clone();
    let (tx, rx)  = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        let result: Result<String, String> = rt.block_on(async move {
            let accounts = crate::auth::load_accounts_path(&data_dir)
                .map_err(|e| e.to_string())?;
            let account = crate::providers::default_account(&accounts, Some("github"))
                .ok_or_else(|| "No GitHub account — run: fxg auth import".to_string())?;
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
                    provider.delete_repo(&repo_name).await
                        .map(|_| "Repository deleted on GitHub (local copy still exists)".to_string())
                        .map_err(|e| e.to_string())
                }
            }
        });
        let _ = tx.send(result);
    });

    st.api_rx = Some(rx);
    st.status_msg = Some("Updating GitHub...".into());
}

fn poll_api_rx(st: &mut St) {
    let done = match &st.api_rx {
        None     => return,
        Some(rx) => match rx.try_recv() {
            Ok(r)                                            => Some(r),
            Err(std::sync::mpsc::TryRecvError::Empty)        => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Connection lost".into())),
        },
    };
    if let Some(result) = done {
        st.status_msg = Some(match result {
            Ok(msg)  => msg,
            Err(msg) => format!("⚠ {}", msg),
        });
        st.api_rx = None;
    }
}

fn poll_prs_issues_rx(st: &mut St) {
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

fn spawn_prs_issues_fetch(st: &mut St) {
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

// ── branch name validation ────────────────────────────────────────────────────

fn validate_branch_name(name: &str) -> Option<String> {
    if name.is_empty() { return Some("Name cannot be empty".into()); }
    if name.contains(' ') { return Some("No spaces allowed".into()); }
    if name.contains("..") { return Some("Cannot contain '..'".into()); }
    if name.starts_with('/') || name.ends_with('/') { return Some("Cannot start or end with '/'".into()); }
    if name.chars().any(|c| matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')) {
        return Some("Invalid character (~^:?*[\\)".into());
    }
    None
}

// ── modal key handling ────────────────────────────────────────────────────────

fn handle_modal_key(code: KeyCode, st: &mut St) -> bool {
    match &mut st.modal {
        DetailModal::None => return false,

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
                        63 => { // collab name → step 2: pick role
                            if !text.is_empty() {
                                st.modal = DetailModal::SelectRole { collab_name: text, cursor: 0 };
                            }
                        }
                        65 => { // rename
                            if !text.is_empty() {
                                spawn_settings_op(st, SettingOp::Rename { new_name: text });
                            }
                        }
                        70 => { // commit — stage all and commit with message
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
                        91 => { // commit message for commit+push → trigger exec_action(0)
                            st.pending_sync_mode  = Some("commit_push".into());
                            st.pending_commit_msg = Some(text);
                            st.run_action         = Some(0);
                        }
                        92 => { // tag name → ask for tag message
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
                        93 => { // tag message → confirm push?
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
                        31 => { // delete branch
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
                        94 => { // push tag confirmed
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

// ── main key handling ─────────────────────────────────────────────────────────

fn handle_key(code: KeyCode, st: &mut St) {
    st.status_msg = None;
    match code {
        KeyCode::Char('q') | KeyCode::Esc => st.quit = true,

        // Tab switching: Tab, arrows, number keys
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

        // Vertical navigation
        KeyCode::Down | KeyCode::Char('j') => match st.tab {
            Tab::Overview  => st.ov_scroll  = st.ov_scroll.saturating_add(1),
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
            Tab::Settings  => st.settings_cur = (st.settings_cur + 1).min(14),
            Tab::PRs       => st.pr_cursor    = (st.pr_cursor + 1).min(st.repo_prs.len().saturating_sub(1)),
            Tab::Issues    => st.issue_cursor = (st.issue_cursor + 1).min(st.repo_issues.len().saturating_sub(1)),
            Tab::Todos     => st.todo_cursor  = (st.todo_cursor + 1).min(st.repo_todos.len().saturating_sub(1)),
        },
        KeyCode::Up | KeyCode::Char('k') => match st.tab {
            Tab::Overview  => st.ov_scroll   = st.ov_scroll.saturating_sub(1),
            Tab::Commits   => st.hist_off    = st.hist_off.saturating_sub(1),
            Tab::Branches  => { st.branch_cursor = st.branch_cursor.saturating_sub(1); }
            Tab::Files     => {
                if st.tree_cursor > 0 {
                    st.tree_cursor -= 1;
                    st.run_action = Some(40);
                }
            }
            Tab::Stats     => st.stats_off   = st.stats_off.saturating_sub(1),
            Tab::Settings  => st.settings_cur = st.settings_cur.saturating_sub(1),
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

        // Enter
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

        // Branch-specific keys
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

        // Stats period change
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

        // Preview scroll (Files tab, right panel)
        KeyCode::Char(']') if st.tab == Tab::Files => {
            st.preview_off = st.preview_off.saturating_add(5);
        }
        KeyCode::Char('[') if st.tab == Tab::Files => {
            st.preview_off = st.preview_off.saturating_sub(5);
        }
        // Open selected file in editor (Files tab)
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

        // Global actions
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
            if copy_to_clipboard(&path) { st.status_msg = Some("Path copied!".into()); }
        }
        KeyCode::Char('u') => {
            if let Some(ref url) = st.remote_url_str.clone() {
                if copy_to_clipboard(url) { st.status_msg = Some("URL copied!".into()); }
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

fn maybe_load_settings(st: &mut St) {
    if st.tab == Tab::Settings && st.settings.is_none() {
        st.run_action = Some(50);
    }
}

fn maybe_load_prs_issues(st: &mut St) {
    if (st.tab == Tab::PRs || st.tab == Tab::Issues) && !st.prs_loaded && st.prs_issues_rx.is_none() {
        st.run_action = Some(80);
    }
}

// ── exec_action ───────────────────────────────────────────────────────────────

async fn exec_action(st: &mut St, idx: usize) -> Result<()> {
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
                    st.status_msg = Some(format!("Switched to '{}'", name));
                } else {
                    st.status_msg = Some("Checkout failed (uncommitted changes?)".into());
                }
            }
        }

        // ── 31: delete branch (now handled in-TUI via ConfirmAction modal) ──
        31 => {}

        // ── 106-109: open in editor (leave_tui is handled by run_detail_loop) ─
        106 => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let _ = std::process::Command::new(&editor).arg(&path).status();
        }
        107 => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let _ = std::process::Command::new(&editor).arg(path.join(".gitignore")).status();
        }
        108 => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let _ = std::process::Command::new(&editor).arg(path.join("LICENSE")).status();
        }
        109 => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let readme = ["README.md", "README.rst", "README"].iter()
                .map(|f| path.join(f)).find(|p| p.exists())
                .unwrap_or_else(|| path.join("README.md"));
            let _ = std::process::Command::new(&editor).arg(&readme).status();
        }

        // ── 110: open pending_file in editor ────────────────────────────────
        110 => {
            if let Some(file) = st.pending_file.take() {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
                let _ = std::process::Command::new(&editor).arg(&file).status();
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

// ── provider helpers ──────────────────────────────────────────────────────────

async fn try_make_provider(st: &St) -> Result<Box<dyn crate::providers::Provider>> {
    let accounts = crate::auth::load_accounts_path(&st.cfg_data_dir)?;
    let account  = crate::providers::default_account(&accounts, Some("github"))
        .ok_or_else(|| anyhow::anyhow!("no github account — run: fxg auth import"))?;
    crate::providers::make_provider(account)
}

async fn try_load_settings(st: &mut St) -> Result<()> {
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

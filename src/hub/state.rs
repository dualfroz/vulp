//! The hub's central state machine: `HubState` (everything the main TUI loop
//! reads and mutates) plus the `Tab` enum selecting which view is active.
//! The supporting type definitions this struct is built from — modal types,
//! display-row addressing, background-thread messages, and a couple of
//! standalone enums — live in the sibling `state_*` files and are re-exported
//! below so the rest of `hub/` can keep importing everything from `state::`.

pub use super::state_bg::*;
pub use super::state_misc::*;
pub use super::state_modal::*;
pub use super::state_rows::*;

use crate::projects::{self, Project};
use crate::providers::{IssueDetail, PrDetail};
use crate::status::{count_tracked_files, repo_recent_commits, RepoSummary};
use crate::status::format_age_pub;
use crate::todos::{Todo, TodoSource, TodoStatus};
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Payload of the startup release-info fetch: latest release, the release
/// matching the running version, and per-account GitHub creation dates.
pub type ReleaseFetch = (
    Result<crate::release_info::ReleaseInfo, String>,
    Result<crate::release_info::ReleaseInfo, String>,
    Vec<(String, String)>,
);

/// Which top-level view of the hub is currently showing. Cycled with Tab/
/// Shift-Tab or jumped to directly with the number keys shown in the tab bar.
#[derive(PartialEq, Clone, Copy)]
pub enum Tab { Overview, Repos, Projects, Todos, PRs, Issues, Stats, Activity, HubSettings }

/// Everything the hub's main event loop (`tui_phase`) reads and mutates: the
/// scanned repo/project/todo/PR/issue data, which tab and row are selected,
/// in-flight background-thread receivers, and user settings. One instance
/// lives for the lifetime of a single `vulp` TUI session.
pub struct HubState {
    pub repos:        Vec<RepoSummary>,
    pub projects:     Vec<Project>,
    pub project_map:  HashMap<String, usize>,
    pub todos:        Vec<Todo>,
    pub prs:          Vec<PrDetail>,
    pub issues:       Vec<IssueDetail>,
    pub tab:          Tab,
    pub display:      Vec<DisplayRow>,
    pub cursor:       usize,
    pub offset:       usize,
    pub visible_rows: usize,
    pub selected:     HashSet<usize>,
    pub filter_dirty:   bool,
    pub filter_behind:  bool,
    pub filter_ahead:   bool,
    pub filter_text:    String,
    pub filtering:      bool,
    pub filter_archived:bool,
    pub sort_mode:      SortMode,
    pub bookmarks:      HashSet<PathBuf>,
    pub pins:           HashSet<PathBuf>,
    pub status_msg:     Option<String>,
    pub pending_clone:  Option<String>,
    pub modal:        Modal,
    pub confirming:   Option<PendingAction>,
    pub exec_action:  bool,
    pub open_detail:  bool,
    pub quit:         bool,
    pub workspace_root: PathBuf,
    /// Per-repo cache of expensive extras; never cleared on scroll, only on refresh.
    pub focused_cache:  HashMap<usize, FocusedExtra>,
    pub editor:         String,
    pub clipboard_cmd:  String,
    pub show_code_todos:bool,
    pub todo_status_view: TodoStatusView,
    pub todo_group_by:    TodoGroupBy,
    /// Group keys currently collapsed on the Todos tab (e.g. "Done",
    /// "Cancelled", a project name, a repo name — whatever the current
    /// `todo_group_by` mode keys groups by). Toggled by Enter/Space on a
    /// header row; not persisted across restarts.
    pub todo_group_collapsed: HashSet<String>,
    pub debug_mode:   bool,
    pub debug_log:    Vec<String>,
    /// Live receiver from the background scan thread; None when scan is complete.
    pub scan_rx:      Option<std::sync::mpsc::Receiver<crate::status::RepoSummary>>,
    pub meta_cache:   crate::cache::MetaCache,
    /// Background diff computation in progress: (repo_idx, receiver).
    pub diff_rx:      Option<(usize, std::sync::mpsc::Receiver<Vec<String>>)>,
    // Global Stats tab
    pub global_stats:        Option<GlobalStats>,
    pub global_stats_rx:     Option<std::sync::mpsc::Receiver<GlobalStats>>,
    pub global_stats_period: crate::detail::StatsPeriod,
    // Activity tab
    pub activity_feed:       Option<Vec<crate::activity::ActivityEntry>>,
    pub activity_rx:         Option<std::sync::mpsc::Receiver<Vec<crate::activity::ActivityEntry>>>,
    pub activity_filter:     Option<crate::activity::ActivityKind>,
    // Overview "vulp log" tile: recent vulp-performed actions, newest first
    pub vulp_log:            Vec<crate::activity::ActivityEntry>,
    /// Repo path to re-focus once the startup scan completes (session restore).
    pub pending_cursor_restore: Option<std::path::PathBuf>,
    // General Settings tile: release/update info, fetched once at startup
    pub latest_release:      Option<Result<crate::release_info::ReleaseInfo, String>>,
    pub current_release:     Option<Result<crate::release_info::ReleaseInfo, String>>,
    /// GitHub account-creation dates per account username ("since ..."),
    /// fetched in the same startup thread as the release info.
    pub account_since:       HashMap<String, String>,
    /// (merged PRs, closed-unmerged PRs, closed issues) across all owners,
    /// from the provider search API during [R] refresh. None until fetched.
    pub closed_counts:       Option<(u64, u64, u64)>,
    pub release_rx:          Option<std::sync::mpsc::Receiver<ReleaseFetch>>,
    // Hub Settings tab
    pub hub_settings_cur:    usize,
    // Loaded once at startup from auth data dir
    pub accounts:            Vec<crate::auth::Account>,
    // User-defined repo templates
    pub templates:           Vec<RepoTemplate>,
    // Pending sync input collected from in-TUI modals
    pub pending_sync_action: Option<String>,  // "pull" | "commit_push" | "push_only"
    pub pending_sync_msg:    Option<String>,
    // PhaseResult deferred from modal confirm
    pub pending_phase:       Option<PhaseResult>,
    // Pull-all background receiver: sends progress strings
    pub pull_all_rx:         Option<std::sync::mpsc::Receiver<String>>,
    // API refresh ([R]) background receiver
    pub refresh_rx:          Option<std::sync::mpsc::Receiver<RefreshMsg>>,
    // Repo creation background receiver: progress + final result, without leaving the TUI
    pub create_repo_rx:      Option<std::sync::mpsc::Receiver<CreateRepoMsg>>,
    // Settings-profile pull-from-repo background receiver
    pub profile_pull_rx:     Option<std::sync::mpsc::Receiver<ProfilePullMsg>>,
    // Code intelligence background receiver (tree-sitter parsing — CPU-bound, plain thread)
    pub codeintel_rx:        Option<std::sync::mpsc::Receiver<CodeIntelMsg>>,
    // Weekly digest preview background receiver ([d] on Stats)
    pub digest_rx:           Option<std::sync::mpsc::Receiver<DigestMsg>>,
    // Full app config, needed by background threads that create/push repos via providers
    pub cfg:                 crate::config::AppConfig,
}

impl HubState {
    pub fn passes_filter(&self, ri: usize) -> bool {
        let r = &self.repos[ri];
        if    (self.filter_dirty  && !r.is_dirty)
           || (self.filter_behind && r.behind == 0)
           || (self.filter_ahead  && r.ahead  == 0) { return false; }
        if self.filter_archived {
            let days = (chrono::Utc::now().timestamp() - r.last_commit_ts) / 86400;
            if days < 90 { return false; }
        }
        if !self.filter_text.is_empty() {
            let q = self.filter_text.to_lowercase();
            let in_name   = r.name.to_lowercase().contains(&q);
            let in_owner  = r.owner.as_deref().map(|o| o.to_lowercase().contains(&q)).unwrap_or(false);
            let in_branch = r.branch.to_lowercase().contains(&q);
            let in_lang   = r.lang.as_deref().map(|l| l.to_lowercase().contains(&q)).unwrap_or(false);
            if !(in_name || in_owner || in_branch || in_lang) { return false; }
        }
        true
    }

    /// Recomputes `display` for whichever tab is active. Must be
    /// called after anything that changes what rows should be visible
    /// (filter/sort changed, data refreshed, tab switched) — `display` is not
    /// kept in sync automatically.
    pub fn rebuild_display(&mut self) {
        let mut d = Vec::new();
        match self.tab {
            Tab::Repos => {
                let all_idx: Vec<usize> = (0..self.repos.len())
                    .filter(|&ri| self.passes_filter(ri))
                    .collect();
                let mut pinned:  Vec<usize> = all_idx.iter().copied().filter(|&ri|  self.pins.contains(&self.repos[ri].path)).collect();
                let mut regular: Vec<usize> = all_idx.iter().copied().filter(|&ri| !self.pins.contains(&self.repos[ri].path)).collect();
                let repos = &self.repos;
                match self.sort_mode {
                    SortMode::Default    => {}
                    SortMode::Name       => regular.sort_by(|&a, &b| repos[a].name.cmp(&repos[b].name)),
                    SortMode::Age        => regular.sort_by(|&a, &b| repos[b].last_commit_ts.cmp(&repos[a].last_commit_ts)),
                    SortMode::DirtyFirst => regular.sort_by(|&a, &b| repos[b].is_dirty.cmp(&repos[a].is_dirty)),
                    SortMode::BehindFirst=> regular.sort_by(|&a, &b| repos[b].behind.cmp(&repos[a].behind)),
                }
                pinned.sort_by(|&a, &b| repos[a].name.cmp(&repos[b].name));
                for ri in pinned.into_iter().chain(regular) {
                    d.push(DisplayRow::Repo { repo_idx: ri });
                }
            }
            Tab::Projects => {
                for pi in 0..self.projects.len() {
                    d.push(DisplayRow::Project { proj_idx: pi });
                }
            }
            Tab::Todos => self.build_todos_display(&mut d),
            Tab::PRs => {
                for i in 0..self.prs.len() {
                    d.push(DisplayRow::Pr { pr_idx: i });
                }
            }
            Tab::Issues => {
                for i in 0..self.issues.len() {
                    d.push(DisplayRow::Issue { issue_idx: i });
                }
            }
            Tab::Overview | Tab::Stats | Tab::Activity | Tab::HubSettings => {
                // These tabs render their own content; display list is empty
            }
        }
        if self.cursor >= d.len().max(1) {
            self.cursor = d.len().saturating_sub(1);
        }
        // Keep the (possibly clamped) cursor inside the visible window —
        // otherwise a filter change can leave it off-screen until the first
        // arrow key.
        self.offset = if self.visible_rows > 0 && self.cursor >= self.visible_rows {
            self.cursor - self.visible_rows + 1
        } else {
            0
        };
        self.display = d;
    }

    /// Builds the Todos tab's display rows: status-view-filters the todo
    /// list, then groups what survives per `todo_group_by` — `None` just
    /// sorts by recency, the other modes interleave a collapsible
    /// `TodoHeader` before each group's rows (omitted entirely when that
    /// group's key is in `todo_group_collapsed`).
    fn build_todos_display(&self, d: &mut Vec<DisplayRow>) {
        let filtered: Vec<usize> = (0..self.todos.len())
            .filter(|&ti| {
                let t = &self.todos[ti];
                if !self.show_code_todos && t.source == TodoSource::CodeScan { return false; }
                self.todo_status_view.allows(&t.status)
            })
            .collect();

        let by_recency = |a: &usize, b: &usize| self.todos[*b].updated_at.cmp(&self.todos[*a].updated_at);

        match self.todo_group_by {
            TodoGroupBy::None => {
                let mut sorted = filtered;
                sorted.sort_by(by_recency);
                for ti in sorted { d.push(DisplayRow::Todo { todo_idx: ti }); }
            }
            TodoGroupBy::Status => {
                for status in [TodoStatus::InProgress, TodoStatus::Open, TodoStatus::Done, TodoStatus::Cancelled] {
                    let mut group: Vec<usize> = filtered.iter().copied()
                        .filter(|&ti| self.todos[ti].status == status).collect();
                    if group.is_empty() { continue; }
                    group.sort_by(by_recency);
                    let key = status.label().to_string();
                    let extra = if status == TodoStatus::Done {
                        format!(" · ostatni {}", format_age_pub(self.todos[group[0]].updated_at))
                    } else {
                        String::new()
                    };
                    self.push_todo_group(d, key.clone(), status.label().to_string(), group, extra);
                }
            }
            TodoGroupBy::Project => {
                let mut keys: Vec<String> = filtered.iter()
                    .map(|&ti| self.todos[ti].project.clone().unwrap_or_else(|| "— brak projektu —".into()))
                    .collect();
                keys.sort(); keys.dedup();
                for key in keys {
                    let mut group: Vec<usize> = filtered.iter().copied()
                        .filter(|&ti| self.todos[ti].project.as_deref().unwrap_or("— brak projektu —") == key)
                        .collect();
                    group.sort_by(by_recency);
                    self.push_todo_group(d, key.clone(), key, group, String::new());
                }
            }
            TodoGroupBy::Repo => {
                let mut keys: Vec<String> = filtered.iter()
                    .map(|&ti| self.todos[ti].repo.clone().unwrap_or_else(|| "— brak repo —".into()))
                    .collect();
                keys.sort(); keys.dedup();
                for key in keys {
                    let mut group: Vec<usize> = filtered.iter().copied()
                        .filter(|&ti| self.todos[ti].repo.as_deref().unwrap_or("— brak repo —") == key)
                        .collect();
                    group.sort_by(by_recency);
                    self.push_todo_group(d, key.clone(), key, group, String::new());
                }
            }
        }
    }

    /// Pushes one group's header row, then its todo rows unless the group
    /// is collapsed.
    fn push_todo_group(&self, d: &mut Vec<DisplayRow>, key: String, label: String, group: Vec<usize>, extra: String) {
        let count = group.len();
        let collapsed = self.todo_group_collapsed.contains(&key);
        d.push(DisplayRow::TodoHeader { key, label, count, extra });
        if !collapsed {
            for ti in group { d.push(DisplayRow::Todo { todo_idx: ti }); }
        }
    }

    pub fn rebuild_project_map(&mut self) {
        self.project_map = projects::index_by_repo(&self.projects);
    }

    pub fn cur_di(&self) -> Option<usize> {
        if self.cursor < self.display.len() { Some(self.cursor) } else { None }
    }
    pub fn cur_repo_idx(&self) -> Option<usize> {
        match self.display.get(self.cur_di()?)? {
            DisplayRow::Repo { repo_idx } => Some(*repo_idx),
            _ => None,
        }
    }
    pub fn cur_proj_idx(&self) -> Option<usize> {
        match self.display.get(self.cur_di()?)? {
            DisplayRow::Project { proj_idx } => Some(*proj_idx),
            _ => None,
        }
    }
    pub fn cur_todo_idx(&self) -> Option<usize> {
        match self.display.get(self.cur_di()?)? {
            DisplayRow::Todo { todo_idx } => Some(*todo_idx),
            _ => None,
        }
    }
    /// The group key of the `TodoHeader` row under the cursor, if any —
    /// used to toggle that group's collapsed state on Enter/Space.
    pub fn cur_todo_header_key(&self) -> Option<String> {
        match self.display.get(self.cur_di()?)? {
            DisplayRow::TodoHeader { key, .. } => Some(key.clone()),
            _ => None,
        }
    }

    pub fn project_repos(&self, pi: usize) -> Vec<usize> {
        self.projects[pi].repos.iter()
            .filter_map(|rel| {
                let abs = self.workspace_root.join(rel);
                self.repos.iter().position(|r| r.path == abs)
            })
            .collect()
    }

    pub fn repo_project(&self, ri: usize) -> Option<&Project> {
        let rel = projects::rel_key(&self.repos[ri].path, &self.workspace_root);
        self.project_map.get(&rel).map(|&pi| &self.projects[pi])
    }
    pub fn repo_proj_color(&self, ri: usize) -> Option<Color> {
        let p = self.repo_project(ri)?;
        let (r, g, b) = projects::hex_to_rgb(&p.color);
        Some(Color::Rgb(r, g, b))
    }
    pub fn proj_color(&self, pi: usize) -> Color {
        let (r, g, b) = projects::hex_to_rgb(&self.projects[pi].color);
        Color::Rgb(r, g, b)
    }

    /// Repos a bulk action (sync, label, etc.) should apply to: the explicit
    /// multi-select if there is one, else the single repo under the cursor,
    /// else every repo in the project under the cursor.
    pub fn action_targets(&self) -> Vec<usize> {
        if self.selected.len() > 1 {
            let mut v: Vec<usize> = self.selected.iter().copied().collect();
            v.sort_unstable();
            v
        } else if let Some(ri) = self.cur_repo_idx() {
            vec![ri]
        } else if let Some(pi) = self.cur_proj_idx() {
            let mut v = self.project_repos(pi);
            v.retain(|&ri| self.passes_filter(ri));
            v
        } else {
            vec![]
        }
    }

    pub fn scroll_down(&mut self) {
        let n = self.display.len();
        if n == 0 || self.cursor + 1 >= n { return; }
        self.cursor += 1;
        if self.visible_rows > 0 && self.cursor >= self.offset + self.visible_rows {
            self.offset = self.cursor - self.visible_rows + 1;
        }
    }
    pub fn scroll_up(&mut self) {
        if self.cursor == 0 { return; }
        self.cursor -= 1;
        if self.cursor < self.offset { self.offset = self.cursor; }
    }
    pub fn toggle_select(&mut self) {
        if let Some(ri) = self.cur_repo_idx() {
            if self.selected.contains(&ri) { self.selected.remove(&ri); } else { self.selected.insert(ri); }
        } else if let Some(pi) = self.cur_proj_idx() {
            let repos = self.project_repos(pi);
            let all = repos.iter().all(|ri| self.selected.contains(ri));
            if all { for ri in repos { self.selected.remove(&ri); } } else { for ri in repos { self.selected.insert(ri); } }
        }
    }

    /// Drain all pending repos from the background scan channel.
    /// Non-blocking: returns immediately if nothing is available.
    pub fn poll_incoming(&mut self) {
        if self.scan_rx.is_none() { return; }
        let mut any_new = false;
        let mut scan_done = false;
        loop {
            let result = self.scan_rx.as_ref().unwrap().try_recv();
            match result {
                Ok(mut repo) => {
                    repo.bookmarked = self.bookmarks.contains(&repo.path);
                    // Apply any cached API metadata (stars, PRs, issues, CI, alerts, deploy)
                    let cache_vals = self.meta_cache.get(&repo.name).map(|e| {
                        (e.meta.stars, e.meta.open_issues, e.meta.open_prs, e.meta.visibility.clone(),
                         e.meta.ci_status.clone(), e.meta.dependabot_open, e.meta.deploy_status.clone())
                    });
                    if let Some((stars, issues, prs, vis, ci, dep, deploy)) = cache_vals {
                        repo.stars           = stars;
                        repo.open_issues     = issues;
                        repo.open_prs        = prs;
                        repo.ci_status       = ci;
                        repo.dependabot_open = dep;
                        repo.deploy_status   = deploy;
                        if repo.visibility.is_none() { repo.visibility = vis; }
                    }
                    self.repos.push(repo);
                    any_new = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Scan finished — apply final sort matching scan_workspace order
                    self.repos.sort_by(|a, b| {
                        let a_active = a.is_dirty || a.behind > 0;
                        let b_active = b.is_dirty || b.behind > 0;
                        b_active.cmp(&a_active).then(b.last_commit_ts.cmp(&a.last_commit_ts))
                    });
                    self.repos.dedup_by(|a, b| a.path == b.path);
                    self.scan_rx = None;
                    scan_done = true;
                    // Session restore: put the cursor back on the repo it was
                    // on last time, now that the final sort order is known.
                    if let Some(want) = self.pending_cursor_restore.take() {
                        self.rebuild_display();
                        if let Some(pos) = self.display.iter().position(|row| {
                            matches!(row, DisplayRow::Repo { repo_idx } if self.repos[*repo_idx].path == want)
                        }) {
                            self.cursor = pos;
                        }
                    }
                    any_new  = true;
                    break;
                }
            }
        }
        if any_new { self.rebuild_display(); }
        if scan_done {
            crate::debug::log("scan channel closed — final sort applied".to_string());
            if self.debug_mode {
                self.debug_log.push(format!("scan complete: {} repos", self.repos.len()));
            }
        }
    }

    /// Compute and cache focused extras for the current repo.
    /// Commits + file count run synchronously; diff is spawned in the background.
    /// Safe to call every frame — skips if already cached or diff already in flight.
    pub fn update_focused_extra(&mut self) {
        let Some(ri) = self.cur_repo_idx() else { return; };
        if self.focused_cache.contains_key(&ri) { return; }
        // Don't spawn a second diff for the same repo while one is already in flight.
        if self.diff_rx.as_ref().map(|(idx, _)| *idx == ri).unwrap_or(false) { return; }

        let t0   = std::time::Instant::now();
        let path = self.repos[ri].path.clone();

        let t1 = std::time::Instant::now();
        let commits    = repo_recent_commits(&path, 4);
        let commits_ms = t1.elapsed().as_millis();

        let t2 = std::time::Instant::now();
        let file_count  = count_tracked_files(&path);
        let files_ms    = t2.elapsed().as_millis();

        let diff_preview = if self.repos[ri].is_dirty {
            let path2 = path.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || { let _ = tx.send(diff_inner(&path2)); });
            self.diff_rx = Some((ri, rx));
            DiffState::Loading
        } else {
            DiffState::Ready(vec![])
        };

        if self.debug_mode {
            let total_ms = t0.elapsed().as_millis();
            self.debug_log.push(format!(
                "{total_ms:>5}ms  hover[{ri}] {}  commits={commits_ms}ms files={files_ms}ms diff=bg",
                self.repos[ri].name,
            ));
            if self.debug_log.len() > 200 { self.debug_log.drain(..100); }
        }

        self.focused_cache.insert(ri, FocusedExtra { commits, file_count, diff_preview });
    }

    /// Check if the background diff thread finished; if so, update the cache entry.
    pub fn poll_global_stats(&mut self) {
        let done = match &self.global_stats_rx {
            None     => return,
            Some(rx) => match rx.try_recv() {
                Ok(s)                                            => Some(s),
                Err(std::sync::mpsc::TryRecvError::Empty)        => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            },
        };
        if let Some(s) = done {
            self.global_stats    = Some(s);
            self.global_stats_rx = None;
            self.status_msg      = None;
        }
    }

    /// Check if the background activity-feed thread finished; if so, store the result.
    pub fn poll_activity(&mut self) {
        let done = match &self.activity_rx {
            None     => return,
            Some(rx) => match rx.try_recv() {
                Ok(s)                                            => Some(s),
                Err(std::sync::mpsc::TryRecvError::Empty)        => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            },
        };
        if let Some(s) = done {
            self.activity_feed = Some(s);
            self.activity_rx   = None;
            self.status_msg    = None;
        }
    }

    /// Check if the background release-info fetch finished; if so, store the result.
    pub fn poll_release_info(&mut self) {
        let done = match &self.release_rx {
            None     => return,
            Some(rx) => match rx.try_recv() {
                Ok(pair)                                          => Some(pair),
                Err(std::sync::mpsc::TryRecvError::Empty)        => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            },
        };
        if let Some((latest, current, since)) = done {
            self.latest_release  = Some(latest);
            self.current_release = Some(current);
            self.account_since   = since.into_iter().collect();
            self.release_rx      = None;
        }
    }

    /// Check if a background codeintel job (list functions / find usages /
    /// build rename plan / repo stats) finished, and if so, show its result.
    pub fn poll_codeintel(&mut self) {
        let done = match &self.codeintel_rx {
            None     => return,
            Some(rx) => match rx.try_recv() {
                Ok(msg)                                           => Some(msg),
                Err(std::sync::mpsc::TryRecvError::Empty)        => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            },
        };
        if let Some(msg) = done {
            self.codeintel_rx = None;
            self.status_msg   = None;
            match msg {
                CodeIntelMsg::Info { title, lines } => {
                    self.modal = Modal::Info { title, lines, scroll: 0 };
                }
                CodeIntelMsg::Preview { title, lines, plan } => {
                    self.modal = Modal::RenamePreview { title, lines, scroll: 0, plan };
                }
            }
        }
    }

    /// Check if the background digest-preview job ([d] on Stats) finished,
    /// and if so, show it as a `Modal::Info`.
    pub fn poll_digest(&mut self) {
        let done = match &self.digest_rx {
            None     => return,
            Some(rx) => match rx.try_recv() {
                Ok(msg)                                          => Some(msg),
                Err(std::sync::mpsc::TryRecvError::Empty)        => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            },
        };
        if let Some(msg) = done {
            self.digest_rx  = None;
            self.status_msg = None;
            self.modal = Modal::Info { title: "Weekly digest (preview — nothing sent)".into(), lines: msg.lines, scroll: 0 };
        }
    }

    pub fn poll_diff(&mut self) {
        let rx_ready = if let Some((ri, rx)) = &self.diff_rx {
            match rx.try_recv() {
                Ok(lines)                                         => Some((*ri, lines)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some((*ri, vec![])),
                Err(std::sync::mpsc::TryRecvError::Empty)        => None,
            }
        } else {
            None
        };
        if let Some((ri, lines)) = rx_ready {
            self.diff_rx = None;
            if let Some(ex) = self.focused_cache.get_mut(&ri) {
                ex.diff_preview = DiffState::Ready(lines);
            }
        }
    }
}

/// Working-tree diff (vs. HEAD) for `path`, capped at 20 changed-file lines
/// plus an insertions/deletions summary — enough for the focused-repo
/// preview panel without rendering a full unbounded diff.
fn diff_inner(path: &PathBuf) -> Vec<String> {
    let Ok(repo)  = git2::Repository::open(path)                  else { return vec![]; };
    let Ok(head)  = repo.head()                                    else { return vec![]; };
    let Ok(tree)  = head.peel_to_commit().and_then(|c| c.tree())  else { return vec![]; };

    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(false)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .recurse_ignored_dirs(false)
        .ignore_submodules(true)
        .skip_binary_check(true);

    let Ok(diff)  = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts)) else { return vec![]; };
    let Ok(stats) = diff.stats() else { return vec![]; };

    let mut lines = Vec::new();
    let _ = diff.foreach(
        &mut |delta, _| {
            if lines.len() >= 20 { return false; }
            let marker = match delta.status() {
                git2::Delta::Added   => '+',
                git2::Delta::Deleted => '-',
                _                    => ' ',
            };
            let p = delta.new_file().path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            lines.push(format!("{}  {}", marker, p));
            true
        },
        None, None, None,
    );

    let (ins, del) = (stats.insertions(), stats.deletions());
    if ins > 0 || del > 0 { lines.push(format!("+{ins} -{del}")); }
    lines
}

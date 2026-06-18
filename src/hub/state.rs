use crate::projects::{self, Project};
use crate::providers::{IssueDetail, PrDetail};
use crate::status::{count_tracked_files, repo_recent_commits, RepoSummary};
use crate::todos::{Todo, TodoSource};
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ── tabs ──────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
pub enum Tab { Overview, Repos, Projects, Todos, PRs, Issues, Stats, HubSettings }

// ── global stats ─────────────────────────────────────────────────────────────

pub struct GlobalStats {
    pub activity:      Vec<(String, usize)>,  // date → aggregated commits
    pub lang_dist:     Vec<(String, usize)>,  // lang → repo count
    pub top_repos:     Vec<(String, usize)>,  // name → commit count
    pub total_commits: usize,
    pub dirty_count:   usize,
    pub behind_count:  usize,
    pub day_pattern:   [usize; 7],            // Mon–Sun commit totals
}

// ── sort mode ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum SortMode { Default, Name, Age, DirtyFirst, BehindFirst }

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            Self::Default    => Self::Name,
            Self::Name       => Self::Age,
            Self::Age        => Self::DirtyFirst,
            Self::DirtyFirst => Self::BehindFirst,
            Self::BehindFirst=> Self::Default,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Default    => "Default",
            Self::Name       => "Name",
            Self::Age        => "Recent",
            Self::DirtyFirst => "Dirty↑",
            Self::BehindFirst=> "Behind↑",
        }
    }
}

// ── modal system ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum Modal {
    None,
    Text    { title: String, prompt: String, value: String, tag: TextTag, max_len: Option<usize> },
    Select  { title: String, options: Vec<String>, cursor: usize, tag: SelectTag },
    Multi   { title: String, options: Vec<String>, checks: Vec<bool>, cursor: usize, tag: MultiTag },
    Confirm { message: String, tag: ConfirmTag },
    Info    { title: String, lines: Vec<String>, scroll: usize },
}

impl Modal {
    pub fn is_some(&self) -> bool { !matches!(self, Modal::None) }
}

#[derive(Clone)]
pub enum TextTag {
    Label(usize),
    ProjName,
    ProjColor(String),
    ProjRenameName(usize),
    ProjRenameColor(usize, String),
    TodoTitle,
    TodoDesc(String),
    TodoEditTitle(usize),
    TodoEditDesc(usize, String),
    CloneUrl,
    InitName,
    LinkPath,
    // hub sync
    SyncMsg,
    // hub settings editing
    SetEditor,
    SetClipboard,
    SetWorkspace,
    // import directory
    ImportDir,
    // template creation
    NewTemplateName,
    NewTemplateCmds(String), // template name
}

#[derive(Clone)]
pub enum SelectTag {
    ProjMenu,
    ProjForAction(ProjAction),
    TodoStatus { title: String, desc: String },
    TodoProject { title: String, desc: String, status_idx: usize },
    TodoChangeStatus(usize),
    SelectByCondition,
    SortFilter,
    NewRepo,
    // color palette for project create / rename
    ProjColorCreate(String),
    ProjColorRename(usize, String),
    // hub sync action picker
    SyncAction,
    // pull-all account picker
    PullAllAccount,
    // repo creation: pick template
    InitTemplate(String),
    // template management
    TemplateMenu(usize),
    // copy URL: SSH vs HTTPS
    CopyUrl(std::path::PathBuf),
}

#[derive(Clone, Copy, PartialEq)]
pub enum ProjAction { Assign, Rename, Delete }

#[derive(Clone)]
pub enum MultiTag {
    ProjAssign(usize),
}

#[derive(Clone)]
pub enum ConfirmTag {
    DeleteProject(usize),
    DeleteTodo(usize),
    PullAll(usize),          // index into state.accounts
    RemoveGitAll,
    DeleteTemplate(usize),
}

// ── display rows ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum DisplayRow {
    Repo    { repo_idx: usize },
    Project { proj_idx: usize },
    Todo    { todo_idx: usize },
    Pr      { idx: usize },
    Issue   { idx: usize },
}

// ── pending action ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum PendingAction { Sync, Release }

// ── focused extras (keyed by repo_idx, never cleared on scroll) ───────────────

pub enum DiffState {
    Loading,
    Ready(Vec<String>),
}

pub struct FocusedExtra {
    pub commits:      Vec<(String, String)>,
    pub file_count:   usize,
    pub diff_preview: DiffState,
}

// ── repo template ─────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoTemplate {
    pub name:        String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commands:    Vec<String>,
}

// ── phase result ──────────────────────────────────────────────────────────────

#[derive(PartialEq)]
pub enum PhaseResult {
    None,
    OpenDetail,
    ExecSync,
    ExecRelease,
    ExecClone(String),
    ExecRefresh,
    ExecTemplateInit { path: PathBuf, commands: Vec<String> },
    ExecOpenEditor(PathBuf),
}

// ── modal action ──────────────────────────────────────────────────────────────

pub enum ModalAction { Pending, Confirm, Cancel }

// ── hub state ─────────────────────────────────────────────────────────────────

pub struct HubState {
    pub repos:        Vec<RepoSummary>,
    pub projects:     Vec<Project>,
    pub project_map:  HashMap<String, usize>,
    pub todos:        Vec<Todo>,
    pub prs:          Vec<PrDetail>,
    pub issues:       Vec<IssueDetail>,
    pub tab:          Tab,
    pub display:      Vec<DisplayRow>,
    pub repo_pos:     Vec<usize>,
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
            Tab::Todos => {
                for (ti, t) in self.todos.iter().enumerate() {
                    if !self.show_code_todos && t.source == TodoSource::CodeScan { continue; }
                    d.push(DisplayRow::Todo { todo_idx: ti });
                }
            }
            Tab::PRs => {
                for i in 0..self.prs.len() {
                    d.push(DisplayRow::Pr { idx: i });
                }
            }
            Tab::Issues => {
                for i in 0..self.issues.len() {
                    d.push(DisplayRow::Issue { idx: i });
                }
            }
            Tab::Overview | Tab::Stats | Tab::HubSettings => {
                // These tabs render their own content; display list is empty
            }
        }
        self.repo_pos = (0..d.len()).collect();
        if self.cursor >= self.repo_pos.len().max(1) {
            self.cursor = self.repo_pos.len().saturating_sub(1);
        }
        self.offset = 0;
        self.display = d;
    }

    pub fn rebuild_project_map(&mut self) {
        self.project_map = projects::index_by_repo(&self.projects);
    }

    pub fn cur_di(&self) -> Option<usize> {
        self.repo_pos.get(self.cursor).copied()
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
        let n = self.repo_pos.len();
        if n == 0 || self.cursor + 1 >= n { return; }
        self.cursor += 1;
        let di = self.repo_pos[self.cursor];
        if self.visible_rows > 0 && di >= self.offset + self.visible_rows {
            self.offset = di - self.visible_rows + 1;
        }
    }
    pub fn scroll_up(&mut self) {
        if self.cursor == 0 { return; }
        self.cursor -= 1;
        let di = self.repo_pos[self.cursor];
        if di < self.offset { self.offset = di; }
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
                    // Apply any cached API metadata (stars, PRs, issues)
                    let cache_vals = self.meta_cache.get(&repo.name).map(|e| {
                        (e.meta.stars, e.meta.open_issues, e.meta.open_prs, e.meta.visibility.clone())
                    });
                    if let Some((stars, issues, prs, vis)) = cache_vals {
                        repo.stars       = stars;
                        repo.open_issues = issues;
                        repo.open_prs    = prs;
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

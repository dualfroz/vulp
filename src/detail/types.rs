//! Data structures shared across the repo-detail view: the tab enum, the
//! `St` struct holding all per-session state, and the small value types that
//! populate it (history entries, branches, the file tree, settings).
//!
//! Kept separate from the logic that fills these fields (`settings_io.rs`,
//! `keys.rs`, `actions.rs`) and the logic that draws them (`render.rs`) so
//! that anyone trying to understand "what does the detail view know about a
//! repo" can read one file without wading through control flow.

use crate::status::RepoSummary;
use std::path::PathBuf;

// ── history entry ─────────────────────────────────────────────────────────────

/// One row in the Commits tab: either a real commit or an annotated tag,
/// merged into a single timeline sorted by `ts`.
#[derive(Clone)]
pub enum HistEntry {
    Commit { sha: String, age: String, ts: i64, author: String, msg: String },
    Tag    { name: String, age: String, ts: i64, msg: String },
}

impl HistEntry {
    pub fn ts(&self) -> i64 { match self { Self::Commit { ts, .. } | Self::Tag { ts, .. } => *ts } }
}

// ── new data structures ───────────────────────────────────────────────────────

/// One row in the Branches tab, combining local branch state with its
/// upstream tracking info (ahead/behind counts, last commit summary).
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

/// One row in the Files tab's flattened tree view. `depth` and `expanded`
/// drive `rebuild_tree_visible`'s collapse logic rather than a real nested
/// tree structure, since a flat `Vec` is simpler to scroll and index into.
#[derive(Clone)]
pub struct TreeNode {
    pub name:     String,
    pub rel_path: String,
    pub is_dir:   bool,
    pub depth:    usize,
    pub expanded: bool,
}

/// One row in the Stats tab's contributor breakdown.
#[derive(Clone)]
pub struct ContribEntry {
    pub name:  String,
    pub count: usize,
}


/// Repo metadata fetched from the hosting provider's API for the Settings
/// tab. `gh_auth_ok` distinguishes "we asked the API and it said no" from
/// "we never had credentials to ask in the first place", since those need
/// different fallback messaging.
pub struct RepoSettings {
    pub visibility:  String,
    pub description: String,
    pub topics:      Vec<String>,
    pub gh_auth_ok:  bool,
}

// ── stats period ──────────────────────────────────────────────────────────────

/// Selectable time window for the Stats tab's activity chart.
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

/// One mutating Settings-tab action, captured as data so it can be handed
/// off to a background thread (`spawn_settings_op`) that owns the actual
/// network call.
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

/// All modal prompts the detail view can show. The `tag: u8` fields on the
/// generic variants identify which logical action is in flight, since the
/// same `SelectOption`/`EditText` shapes are reused for several different
/// settings/sync/release flows.
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

/// The detail view's tab bar, in display order.
#[derive(PartialEq, Clone, Copy)]
pub enum Tab { Overview, Commits, Branches, Files, Stats, Settings, PRs, Issues, Todos }

impl Tab {
    pub fn next(self) -> Self {
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
    pub fn prev(self) -> Self {
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

/// All state for one open repo-detail session: a snapshot of every tab's
/// data plus cursors/scroll offsets, loaded eagerly in `run()` (git data) or
/// lazily on first tab visit (provider API data behind `*_rx` channels).
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

    // Files tab (tree)
    #[allow(dead_code)] pub all_files: Vec<String>, // kept: files-tab data model
    pub tree_nodes:     Vec<TreeNode>,
    pub tree_visible:   Vec<usize>,
    pub tree_cursor:    usize,
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
    pub ov_off:         usize,
    pub remote_url_str: Option<String>,

    // Settings tab
    pub settings:       Option<RepoSettings>,
    pub settings_cursor: usize,
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
    pub quit_after_op:  bool,
    pub run_action:     Option<usize>,
    pub status_msg:     Option<String>,
    pub cfg_data_dir:   PathBuf,
    pub cfg_editor:     String,

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

/// Flattens `nodes` into the indices that should actually be drawn, skipping
/// children of any collapsed directory. Recomputed whenever a directory is
/// toggled rather than stored incrementally, since the file tree is small
/// enough that a full rebuild is cheap.
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

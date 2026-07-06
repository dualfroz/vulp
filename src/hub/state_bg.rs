//! Messages and snapshots produced by hub background threads: aggregated
//! workspace stats, the result of long-running repo-creation / settings-sync
//! / code-intelligence jobs, and the loop-control signal (`PhaseResult`) that
//! `tui_phase` uses to know when it needs to hand control back to its caller.

use std::path::PathBuf;

/// Workspace-wide numbers for the Stats and Overview tabs, computed once in
/// the background (scanning every repo's git history) rather than recomputed
/// every frame.
pub struct GlobalStats {
    pub activity:      Vec<(String, usize)>,  // date → aggregated commits
    pub lang_dist:     Vec<(String, usize)>,  // lang → repo count
    pub top_repos:     Vec<(String, usize)>,  // name → commit count, top 5
    pub repo_commits:  Vec<(String, usize)>,  // name → commit count, every repo (sorted desc)
    pub total_commits: usize,
    pub dirty_count:   usize,
    pub behind_count:  usize,
    pub day_pattern:   [usize; 7],            // Mon–Sun commit totals
}

/// Progress/result of the background API refresh ([R]): per-repo metadata,
/// PR/issue lists and workspace-wide closed counters, fetched off-thread so
/// the UI keeps drawing.
pub enum RefreshMsg {
    Progress(String),
    Done {
        metas:         Vec<(String, crate::cache::RepoMeta)>,
        prs:           Vec<(String, Vec<crate::providers::PrDetail>)>,
        issues:        Vec<(String, Vec<crate::providers::IssueDetail>)>,
        closed_counts: Option<(u64, u64, u64)>,
    },
}

/// What `tui_phase` should do once the current event-loop iteration ends.
/// Most keys are handled in place and report `None`; the rest need the
/// caller to take control away from the raw-mode TUI loop (e.g. to shell out
/// for a sync, or to switch into the repo detail sub-view).
#[derive(PartialEq)]
pub enum PhaseResult {
    None,
    OpenDetail,
    ExecSync,
    ExecRelease,
    ExecClone(String),
    ExecOpenEditor(PathBuf),
}

/// Progress/result of a background "create repo" job (git init, commit,
/// push, create-on-provider). `Progress` lines accumulate in a `Modal::Info`
/// while the job runs; `Done` replaces the modal with the final outcome.
pub enum CreateRepoMsg {
    Progress(String),
    Done { success: bool, name: String, detail: String },
}

// ── settings-profile pull-from-repo background messages ─────────────────────
// Separate from CreateRepoMsg because a successful pull needs to hand the
// fetched snapshot back to the main thread to be applied.

/// Progress/result of pulling a settings snapshot from the profile-sync repo.
pub enum ProfilePullMsg {
    Done(Result<crate::profile::SettingsSnapshot, String>),
}

// ── code intelligence background messages ───────────────────────────────────
// Tree-sitter parsing a whole repo is CPU-bound, not I/O — runs on a plain
// std::thread (no tokio runtime needed) and reports back over this channel.

/// Result of a background code-intelligence job (list functions / find
/// usages / build a rename plan / repo stats).
pub enum CodeIntelMsg {
    /// Read-only result: rendered as a `Modal::Info` (function list, usages, stats).
    Info { title: String, lines: Vec<String> },
    /// A rename/convention-conversion dry-run: rendered as `Modal::RenamePreview`,
    /// which requires an explicit Enter/y before `apply_rename_plan` ever runs.
    Preview { title: String, lines: Vec<String>, plan: crate::codeintel::RenamePlan },
}

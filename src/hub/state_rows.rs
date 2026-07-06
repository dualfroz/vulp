//! Row-addressing types: which concrete item a position in the hub's visible
//! list (`DisplayRow`) or a queued confirmation (`PendingAction`) refers to,
//! plus the per-repo "focused extra" data shown for whichever row the cursor
//! is currently on.

/// One row in the currently-displayed list (`HubState::display`). Which tab
/// is active determines which variant appears — a given `display` list is
/// always one variant, never mixed.
#[derive(Clone)]
pub enum DisplayRow {
    Repo    { repo_idx: usize },
    Project { proj_idx: usize },
    Todo    { todo_idx: usize },
    Pr      { pr_idx: usize },
    Issue   { issue_idx: usize },
}

/// A sync/release action the user confirmed but that needs the confirm modal
/// closed first before `tui_phase` actually runs it, so the dialog isn't
/// still on screen while the (potentially slow) operation executes.
#[derive(Clone)]
pub enum PendingAction { Release }

/// Background diff fetch for the currently-focused repo: `Loading` while the
/// thread spawned by `HubState::update_focused_extra` is still running,
/// `Ready` once it reports back.
pub enum DiffState {
    Loading,
    Ready(Vec<String>),
}

/// Expensive per-repo data (recent commits, file count, working-tree diff)
/// computed lazily for whichever repo the cursor is hovering. Cached by
/// `repo_idx` in `HubState::focused_cache` and never cleared on scroll —
/// only a manual refresh recomputes it, so revisiting a repo is instant.
pub struct FocusedExtra {
    #[allow(dead_code)] pub commits: Vec<(String, String)>, // kept: focused-repo data model
    pub file_count:   usize,
    pub diff_preview: DiffState,
}

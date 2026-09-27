#[derive(Clone)]
pub enum DisplayRow {
    Repo    { repo_idx: usize },
    Project { proj_idx: usize },
    Todo    { todo_idx: usize },
    TodoHeader { key: String, label: String, count: usize, extra: String },
    Pr      { pr_idx: usize },
    Issue   { issue_idx: usize },
}

#[derive(Clone)]
pub enum PendingAction { Release }

pub enum DiffState {
    Loading,
    Ready(Vec<String>),
}

pub struct FocusedExtra {
    #[allow(dead_code)] pub commits: Vec<(String, String)>,
    pub file_count:   usize,
    pub diff_preview: DiffState,
}

//! Cross-repo todo list (`~/.vulp/todos.toml`), shared by the hub's Todos
//! tab and each repo detail view's per-repo Todos tab. Todos are either
//! manually created or scanned from `TODO`/`FIXME`/`HACK` comments in
//! source (`TodoSource`) — the hub's default Todos view shows only manual
//! ones, since scanned todos are meant to be reviewed and promoted
//! deliberately rather than flooding the list automatically.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A todo's workflow state, cycled in a fixed order by the `Space` key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TodoStatus {
    Open,
    InProgress,
    Done,
    Cancelled,
}

impl TodoStatus {
    /// Display label shown in todo tables and the status-picker modal.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open       => "Open",
            Self::InProgress => "In Progress",
            Self::Done       => "Done",
            Self::Cancelled  => "Cancelled",
        }
    }
    /// Status color used in todo tables.
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            Self::Open       => Color::Cyan,
            Self::InProgress => Color::Yellow,
            Self::Done       => Color::Green,
            Self::Cancelled  => Color::DarkGray,
        }
    }
    /// Advances to the next status in the fixed Open → InProgress → Done →
    /// Cancelled → Open cycle, bound to the `Space` key on the Todos tab.
    pub fn cycle(&self) -> Self {
        match self {
            Self::Open       => Self::InProgress,
            Self::InProgress => Self::Done,
            Self::Done       => Self::Cancelled,
            Self::Cancelled  => Self::Open,
        }
    }
    /// Maps the status-picker modal's selected index back to a `TodoStatus`,
    /// defaulting to `Open` for any out-of-range index.
    pub fn from_idx(i: usize) -> Self {
        match i { 1 => Self::InProgress, 2 => Self::Done, 3 => Self::Cancelled, _ => Self::Open }
    }
    /// One-character glyph shown next to the status label in todo tables.
    pub fn icon(&self) -> &'static str {
        match self { Self::Open => "○", Self::InProgress => "◎", Self::Done => "●", Self::Cancelled => "×" }
    }
    /// All status labels in display order, for the status-picker modal.
    pub fn all_labels() -> Vec<&'static str> {
        vec!["Open", "In Progress", "Done", "Cancelled"]
    }
}

/// Where a todo came from — user-created or auto-scanned from code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TodoSource {
    #[default]
    Manual,
    CodeScan,
}

/// One todo item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id:          u64,
    pub title:       String,
    pub description: String,
    pub status:      TodoStatus,
    pub project:     Option<String>,
    /// Which repo this todo belongs to (repo folder name).
    #[serde(default)]
    pub repo:        Option<String>,
    /// Manual (user-created) or CodeScan (found in source).
    #[serde(default)]
    pub source:      TodoSource,
    #[serde(default)]
    pub estimate:    Option<String>,
    #[serde(default)]
    pub difficulty:  Option<u8>,
    pub created_at:  i64,
    pub updated_at:  i64,
}

impl Todo {
    /// Creates a manually-entered todo, timestamped now.
    pub fn new(
        id: u64, title: String, description: String,
        status: TodoStatus, project: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self { id, title, description, status, project,
               repo: None, source: TodoSource::Manual,
               estimate: None, difficulty: None,
               created_at: now, updated_at: now }
    }

    /// Creates a todo discovered by the source-code `TODO`/`FIXME`/`HACK`
    /// scanner, tagged with the repo it came from and `TodoSource::CodeScan`.
    pub fn new_scan(
        id: u64, title: String, repo: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self { id, title, description: String::new(),
               status: TodoStatus::Open, project: None,
               repo: Some(repo), source: TodoSource::CodeScan,
               estimate: None, difficulty: None,
               created_at: now, updated_at: now }
    }

    /// Updates `updated_at` to now — called on every edit so "last touched"
    /// stays meaningful.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TodosFile {
    #[serde(default)]
    todo: Vec<Todo>,
}

fn file_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".vulp").join("todos.toml")
}

/// Loads every todo (manual and code-scanned) from disk, or an empty list
/// if the file doesn't exist or fails to parse.
pub fn load() -> Vec<Todo> {
    let p = file_path();
    if !p.exists() { return Vec::new(); }
    toml::from_str::<TodosFile>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|f| f.todo)
        .unwrap_or_default()
}

/// Load only manual todos (default view).
pub fn load_manual() -> Vec<Todo> {
    load().into_iter().filter(|t| t.source == TodoSource::Manual).collect()
}

/// Overwrites the todos file with `todos`.
pub fn save(todos: &[Todo]) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(&TodosFile { todo: todos.to_vec() })?)?;
    Ok(())
}

/// The next unused todo id — one past the current maximum, or 1 if the list
/// is empty.
pub fn next_id(todos: &[Todo]) -> u64 {
    todos.iter().map(|t| t.id).max().unwrap_or(0) + 1
}

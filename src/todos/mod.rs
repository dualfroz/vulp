use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TodoStatus {
    Open,
    InProgress,
    Done,
    Cancelled,
}

impl TodoStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open       => "Open",
            Self::InProgress => "In Progress",
            Self::Done       => "Done",
            Self::Cancelled  => "Cancelled",
        }
    }
    pub fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            Self::Open       => Color::Cyan,
            Self::InProgress => Color::Yellow,
            Self::Done       => Color::Green,
            Self::Cancelled  => Color::DarkGray,
        }
    }
    pub fn cycle(&self) -> Self {
        match self {
            Self::Open       => Self::InProgress,
            Self::InProgress => Self::Done,
            Self::Done       => Self::Cancelled,
            Self::Cancelled  => Self::Open,
        }
    }
    pub fn idx(&self) -> usize {
        match self {
            Self::Open => 0, Self::InProgress => 1,
            Self::Done => 2, Self::Cancelled  => 3,
        }
    }
    pub fn from_idx(i: usize) -> Self {
        match i { 1 => Self::InProgress, 2 => Self::Done, 3 => Self::Cancelled, _ => Self::Open }
    }
    pub fn icon(&self) -> &'static str {
        match self { Self::Open => "○", Self::InProgress => "◎", Self::Done => "●", Self::Cancelled => "×" }
    }
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

impl TodoSource {
    pub fn icon(&self) -> &'static str {
        match self { Self::Manual => "✎", Self::CodeScan => "⌂" }
    }
}

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
    dirs::home_dir().unwrap_or_default().join(".fxg").join("todos.toml")
}

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

pub fn save(todos: &[Todo]) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(&TodosFile { todo: todos.to_vec() })?)?;
    Ok(())
}

/// Purge all CodeScan todos from the saved file.
pub fn purge_code_scan() -> anyhow::Result<usize> {
    let all = load();
    let before = all.len();
    let manual: Vec<Todo> = all.into_iter().filter(|t| t.source == TodoSource::Manual).collect();
    let removed = before - manual.len();
    save(&manual)?;
    Ok(removed)
}

pub fn next_id(todos: &[Todo]) -> u64 {
    todos.iter().map(|t| t.id).max().unwrap_or(0) + 1
}

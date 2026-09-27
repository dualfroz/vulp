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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TodoSource {
    #[default]
    Manual,
    CodeScan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id:          u64,
    pub title:       String,
    pub description: String,
    pub status:      TodoStatus,
    pub project:     Option<String>,
    #[serde(default)]
    pub repo:        Option<String>,
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
    dirs::home_dir().unwrap_or_default().join(".vulp").join("todos.toml")
}

pub fn load() -> Vec<Todo> {
    let p = file_path();
    if !p.exists() { return Vec::new(); }
    toml::from_str::<TodosFile>(&std::fs::read_to_string(p).unwrap_or_default())
        .map(|f| f.todo)
        .unwrap_or_default()
}

pub fn load_manual() -> Vec<Todo> {
    load().into_iter().filter(|t| t.source == TodoSource::Manual).collect()
}

pub fn save(todos: &[Todo]) -> anyhow::Result<()> {
    let p = file_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(p, toml::to_string_pretty(&TodosFile { todo: todos.to_vec() })?)?;
    Ok(())
}

pub fn next_id(todos: &[Todo]) -> u64 {
    todos.iter().map(|t| t.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_continue_after_the_highest() {
        assert_eq!(next_id(&[]), 1);
        let todos = vec![
            Todo::new(7, "a".into(), String::new(), TodoStatus::Open, None),
            Todo::new(3, "b".into(), String::new(), TodoStatus::Done, None),
        ];
        assert_eq!(next_id(&todos), 8);
    }

    #[test]
    fn status_cycle_visits_every_state_and_matches_the_picker() {
        let mut s = TodoStatus::Open;
        let mut seen = vec![];
        for _ in 0..4 {
            seen.push(s.label());
            s = s.cycle();
        }
        assert_eq!(s, TodoStatus::Open);
        assert_eq!(seen, TodoStatus::all_labels());
        for (i, label) in TodoStatus::all_labels().into_iter().enumerate() {
            assert_eq!(TodoStatus::from_idx(i).label(), label);
        }
        assert_eq!(TodoStatus::from_idx(99), TodoStatus::Open);
    }

    #[test]
    fn scanned_todos_are_tagged_and_old_files_still_load() {
        let t = Todo::new_scan(1, "fix parser".into(), "core".into());
        assert_eq!(t.source, TodoSource::CodeScan);
        assert_eq!(t.repo.as_deref(), Some("core"));

        let legacy = r#"
            [[todo]]
            id = 4
            title = "old entry"
            description = ""
            status = "InProgress"
            created_at = 1
            updated_at = 2
        "#;
        let file: TodosFile = toml::from_str(legacy).unwrap();
        assert_eq!(file.todo.len(), 1);
        assert_eq!(file.todo[0].source, TodoSource::Manual);
        assert_eq!(file.todo[0].status, TodoStatus::InProgress);
        assert_eq!(file.todo[0].repo, None);
    }
}

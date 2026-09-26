use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityKind { Commit, Push, Pull, Release, NewRepo, Todo, Pr, Issue, License }

impl ActivityKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Commit  => "commit",
            Self::Push    => "push",
            Self::Pull    => "pull",
            Self::Release => "release",
            Self::NewRepo => "new repo",
            Self::Todo    => "todo",
            Self::Pr      => "pr",
            Self::Issue   => "issue",
            Self::License => "license",
        }
    }

    pub fn cycle() -> &'static [ActivityKind] {
        &[
            Self::Commit, Self::Push, Self::Pull, Self::Release,
            Self::NewRepo, Self::Todo, Self::Pr, Self::Issue, Self::License,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub ts:     i64,
    pub kind:   ActivityKind,
    pub repo:   String,
    pub detail: String,
}

fn log_path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".vulp").join("activity.log")
}

pub fn log(entry: &ActivityEntry) {
    let mut entry = entry.clone();
    if std::env::var("VULP_AGENT").map(|v| !v.is_empty()).unwrap_or(false) {
        entry.detail = if entry.detail.is_empty() {
            "via agent".into()
        } else {
            format!("{} · via agent", entry.detail)
        };
    }
    let Ok(line) = serde_json::to_string(&entry) else { return };
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

pub fn load_logged() -> Vec<ActivityEntry> {
    let Ok(content) = std::fs::read_to_string(log_path()) else { return vec![] };
    content.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

pub fn compact(max_entries: usize) {
    let entries = load_logged();
    if entries.len() <= max_entries { return; }
    let keep = &entries[entries.len() - max_entries..];
    let body: String = keep.iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .map(|l| l + "\n")
        .collect();
    let path = log_path();
    let tmp  = path.with_extension("log.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

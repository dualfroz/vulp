//! Local activity log (`~/.vulp/activity.log`, one JSON line per event) for
//! actions vulp itself performed — feeds the hub's Activity tab alongside
//! (but separately from) the git-log-sourced commit feed, which sees real
//! commit history regardless of which tool made it.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The kind of action recorded in one activity log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityKind { Commit, Push, Pull, Release, NewRepo, Todo, Pr, Issue }

impl ActivityKind {
    /// Display label shown in the Activity tab's table.
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
        }
    }

    /// Cycle order for the Activity tab's kind filter (`All` is handled
    /// separately as `None`).
    pub fn cycle() -> &'static [ActivityKind] {
        &[
            Self::Commit, Self::Push, Self::Pull, Self::Release,
            Self::NewRepo, Self::Todo, Self::Pr, Self::Issue,
        ]
    }
}

/// One recorded activity-log event.
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

/// Appends one event to the local activity log. Only called from code paths
/// where vulp itself performs the action (push/pull/release/repo-create/
/// todo-scan) — it has no way to see git activity done outside vulp, which
/// the Activity tab's commit feed (sourced straight from git log) covers
/// separately and accurately regardless of how it happened.
pub fn log(entry: &ActivityEntry) {
    let Ok(line) = serde_json::to_string(entry) else { return };
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

/// Loads every logged entry, skipping any line that fails to parse (a
/// truncated last line from a crash, for instance) rather than discarding
/// the whole log.
pub fn load_logged() -> Vec<ActivityEntry> {
    let Ok(content) = std::fs::read_to_string(log_path()) else { return vec![] };
    content.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Keeps the append-only log from growing forever: when it exceeds
/// `max_entries`, rewrites the file with only the newest `max_entries`.
/// Called once at hub startup — cheap no-op below the threshold.
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

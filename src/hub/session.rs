//! Session persistence (`~/.vulp/session.toml`): remembers the active tab
//! and the repo under the cursor between runs. Deliberately tiny — filters
//! and text search are NOT restored (surprising UX), only "where was I".

use super::state::Tab;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct Session {
    #[serde(default)]
    pub tab: u8,
    #[serde(default)]
    pub cursor_repo_path: Option<PathBuf>,
}

fn path() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".vulp").join("session.toml")
}

pub fn tab_to_u8(tab: Tab) -> u8 {
    match tab {
        Tab::Overview => 0, Tab::Repos => 1, Tab::Projects => 2, Tab::Todos => 3,
        Tab::PRs => 4, Tab::Issues => 5, Tab::Stats => 6, Tab::Activity => 7,
        Tab::HubSettings => 8,
    }
}

pub fn u8_to_tab(v: u8) -> Tab {
    match v {
        1 => Tab::Repos, 2 => Tab::Projects, 3 => Tab::Todos, 4 => Tab::PRs,
        5 => Tab::Issues, 6 => Tab::Stats, 7 => Tab::Activity, 8 => Tab::HubSettings,
        _ => Tab::Overview,
    }
}

pub fn load() -> Session {
    let p = path();
    if !p.exists() { return Session::default(); }
    toml::from_str(&std::fs::read_to_string(p).unwrap_or_default()).unwrap_or_default()
}

pub fn save(tab: Tab, cursor_repo_path: Option<PathBuf>) {
    let s = Session { tab: tab_to_u8(tab), cursor_repo_path };
    let p = path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    if let Ok(body) = toml::to_string_pretty(&s) { let _ = std::fs::write(p, body); }
}

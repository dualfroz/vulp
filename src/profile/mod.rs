//! Settings portability: snapshotting `AppConfig` (and optionally workspace/
//! project structure) into a `SettingsSnapshot`, then saving/loading it as a
//! named local profile, a copy-pasteable JSON blob, or a synced private git
//! repo (`vulp-settings`) for moving settings between machines.

use crate::config::AppConfig;
use crate::projects::Project;
use crate::workspace::Workspace;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default name of the optional private git repo used to sync settings
/// between machines. Also doubles as the marker bulk-import looks for to
/// offer auto-importing settings from a freshly-connected account.
pub const SETTINGS_REPO_NAME: &str = "vulp-settings";

/// The optional, opt-in part of a snapshot: workspace layout and project
/// groupings, kept separate from `SettingsSnapshot.config` since it leaks
/// repo names and is meaningful to copy only when moving to a genuinely new
/// machine, not just syncing preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStructure {
    pub workspaces: Vec<Workspace>,
    pub projects:   Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSnapshot {
    /// Config with machine-specific paths (workspace_root, data_dir,
    /// vsc_data_dir) zeroed out — those never travel between machines.
    pub config: AppConfig,
    /// `None` = settings-only snapshot. `Some` additionally carries the
    /// workspace/project layout, which leaks repo names — opt-in only.
    pub structure: Option<WorkspaceStructure>,
}

/// Build a snapshot of the current settings (and optionally workspace
/// structure) for export/profile-save/git-sync. Never includes tokens —
/// those live only in the OS keyring / local `.tokens` file.
pub fn snapshot(cfg: &AppConfig, include_structure: bool) -> SettingsSnapshot {
    let mut config = cfg.clone();
    config.workspace_root = PathBuf::new();
    config.data_dir       = PathBuf::new();
    config.vsc_data_dir   = PathBuf::new();

    let structure = if include_structure {
        Some(WorkspaceStructure {
            workspaces: crate::workspace::load_workspaces(cfg).unwrap_or_default(),
            projects:   crate::projects::load(),
        })
    } else {
        None
    };
    SettingsSnapshot { config, structure }
}

/// Compact (single-line) JSON — deliberately not pretty-printed, since this
/// also has to round-trip through the single-line copy/paste Text modal.
pub fn to_json(snap: &SettingsSnapshot) -> Result<String> {
    Ok(serde_json::to_string(snap)?)
}

/// Parses a snapshot from JSON (the inverse of `to_json`).
pub fn from_json(s: &str) -> Result<SettingsSnapshot> {
    Ok(serde_json::from_str(s)?)
}

/// Apply a snapshot on top of the current config: machine-specific paths are
/// preserved (not overwritten with whatever was in the snapshot), everything
/// else is replaced. Saves config.toml (and workspaces/projects.toml if the
/// snapshot carries structure) immediately.
pub fn apply(snap: &SettingsSnapshot, cfg: &mut AppConfig) -> Result<()> {
    let workspace_root = cfg.workspace_root.clone();
    let data_dir        = cfg.data_dir.clone();
    let vsc_data_dir     = cfg.vsc_data_dir.clone();

    *cfg = snap.config.clone();
    cfg.workspace_root = workspace_root;
    cfg.data_dir        = data_dir;
    cfg.vsc_data_dir     = vsc_data_dir;
    crate::config::save_config(cfg)?;

    if let Some(structure) = &snap.structure {
        crate::workspace::save_workspaces(&structure.workspaces, cfg)?;
        crate::projects::save(&structure.projects)?;
    }
    Ok(())
}

// ── local profiles: named snapshots for instant switching ──────────────────

fn profiles_dir(cfg: &AppConfig) -> PathBuf {
    cfg.data_dir.join("profiles")
}

/// Saves `snap` as a named local profile (`<data_dir>/profiles/<name>.json`)
/// for instant switching later via `load_profile`.
pub fn save_profile(cfg: &AppConfig, name: &str, snap: &SettingsSnapshot) -> Result<()> {
    let dir = profiles_dir(cfg);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{name}.json")), to_json(snap)?)?;
    Ok(())
}

/// Loads a previously saved local profile by name.
pub fn load_profile(cfg: &AppConfig, name: &str) -> Result<SettingsSnapshot> {
    let raw = std::fs::read_to_string(profiles_dir(cfg).join(format!("{name}.json")))?;
    from_json(&raw)
}

/// Lists every saved local profile's name.
pub fn list_profiles(cfg: &AppConfig) -> Vec<String> {
    let dir = profiles_dir(cfg);
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    entries.flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                p.file_stem().map(|s| s.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect()
}

// ── optional git-repo sync ──────────────────────────────────────────────────

const SNAPSHOT_FILE: &str = "vulp-settings.json";

fn local_sync_dir(cfg: &AppConfig, repo_name: &str) -> PathBuf {
    cfg.data_dir.join(repo_name)
}

/// Builds git2 push/fetch credentials trying the SSH agent first, falling
/// back to the provider token as an HTTPS password — covers both SSH- and
/// HTTPS-configured remotes without needing to know which one is in use.
fn push_credentials(token: String) -> git2::RemoteCallbacks<'static> {
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |_url, user, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            git2::Cred::ssh_key_from_agent(user.unwrap_or("git"))
        } else {
            git2::Cred::userpass_plaintext(user.unwrap_or("git"), &token)
        }
    });
    cb
}

/// Push the snapshot to a private repo (created on first use). Reuses the
/// same create-and-push path as regular repo creation, just for a folder
/// that holds one JSON file instead of a project.
pub async fn push_to_repo(snap: &SettingsSnapshot, cfg: &AppConfig, repo_name: &str) -> Result<String> {
    let dir = local_sync_dir(cfg, repo_name);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(SNAPSHOT_FILE), to_json(snap)?)?;

    if dir.join(".git").exists() {
        // Already linked to a remote from a previous push — commit + push.
        let repo = git2::Repository::open(&dir)?;
        let mut index = repo.index()?;
        index.add_path(std::path::Path::new(SNAPSHOT_FILE))?;
        index.write()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = git2::Signature::now(&cfg.username, &cfg.email)?;
        let parent = repo.head()?.peel_to_commit()?;
        repo.commit(Some("HEAD"), &sig, &sig, "Update vulp settings", &tree, &[&parent])?;

        let accounts = crate::auth::load_accounts(cfg)?;
        let acc = crate::providers::default_account(&accounts, Some(&cfg.default_provider))
            .ok_or_else(|| anyhow::anyhow!("no provider account configured"))?;
        let token = crate::auth::get_token(&acc.id())?;
        let mut remote = repo.find_remote("origin")?;
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(push_credentials(token));
        remote.push(&["refs/heads/master:refs/heads/master", "refs/heads/main:refs/heads/main"], Some(&mut push_opts)).ok();
        let url = remote.url().unwrap_or_default().to_string();
        crate::activity::log(&crate::activity::ActivityEntry {
            ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::Push,
            repo: repo_name.to_string(), detail: "settings sync".into(),
        });
        Ok(url)
    } else {
        // First push ever: create the remote (private) and publish.
        let result = crate::legacy::adopt::adopt_folder(
            &dir, cfg, true, "Initial vulp settings sync", None, None, |_| {},
        ).await;
        if result.success {
            crate::activity::log(&crate::activity::ActivityEntry {
                ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::NewRepo,
                repo: repo_name.to_string(), detail: "settings sync (first push)".into(),
            });
            Ok(result.url.unwrap_or_default())
        } else {
            Err(anyhow::anyhow!(result.detail))
        }
    }
}

/// Pull the snapshot from the sync repo, cloning it locally first if needed.
pub async fn pull_from_repo(cfg: &AppConfig, repo_name: &str) -> Result<SettingsSnapshot> {
    let dir = local_sync_dir(cfg, repo_name);

    if dir.join(".git").exists() {
        let repo = git2::Repository::open(&dir)?;
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
        if let Ok(oid) = repo.refname_to_id("refs/remotes/origin/HEAD")
            .or_else(|_| repo.refname_to_id("refs/remotes/origin/master"))
            .or_else(|_| repo.refname_to_id("refs/remotes/origin/main"))
        {
            let obj = repo.find_object(oid, None)?;
            repo.reset(&obj, git2::ResetType::Hard, None)?;
        }
    } else {
        let accounts = crate::auth::load_accounts(cfg)?;
        let acc = crate::providers::default_account(&accounts, Some(&cfg.default_provider))
            .ok_or_else(|| anyhow::anyhow!("no provider account configured"))?;
        let token = crate::auth::get_token(&acc.id())?;
        let host = acc.base_url.clone().unwrap_or_else(|| match acc.provider.as_str() {
            "gitlab" => "https://gitlab.com".to_string(),
            _        => "https://github.com".to_string(),
        });
        let url = format!("{}/{}/{}.git", host.trim_end_matches('/'), acc.username, repo_name);
        std::fs::create_dir_all(dir.parent().unwrap_or(&dir))?;
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(push_credentials(token));
        git2::build::RepoBuilder::new().fetch_options(fo).clone(&url, &dir)?;
    }

    let raw = std::fs::read_to_string(dir.join(SNAPSHOT_FILE))?;
    let snap = from_json(&raw)?;
    crate::activity::log(&crate::activity::ActivityEntry {
        ts: chrono::Utc::now().timestamp(), kind: crate::activity::ActivityKind::Pull,
        repo: repo_name.to_string(), detail: "settings sync".into(),
    });
    Ok(snap)
}

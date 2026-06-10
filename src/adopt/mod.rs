pub mod ui;

use crate::auth::load_accounts;
use crate::cli::AdoptArgs;
use crate::config::AppConfig;
use crate::providers::{default_account, make_provider};
use anyhow::Result;
use git2::{IndexAddOption, Repository, Signature};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AdoptResult {
    pub name:    String,
    pub success: bool,
    pub detail:  String,
    pub url:     Option<String>,
}

impl AdoptResult {
    fn ok(name: &str, detail: impl Into<String>, url: impl Into<String>) -> Self {
        Self { name: name.into(), success: true, detail: detail.into(), url: Some(url.into()) }
    }
    fn err(name: &str, detail: impl Into<String>) -> Self {
        Self { name: name.into(), success: false, detail: detail.into(), url: None }
    }
}

/// Scan for adoptable folders using the two-tier model:
///
/// Tier 1 — direct children of `root` that are NOT in `groups` and have no `.git`.
/// Tier 2 — direct children of each `root/<group>` folder that have no `.git`.
///
/// Never recurses further. Hidden dirs and the group folders themselves are excluded.
pub fn scan_non_git(root: &Path, groups: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let group_set: std::collections::HashSet<&str> = groups.iter().map(|s| s.as_str()).collect();

    // Tier 1: direct children of workspace_root
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') { continue; }
            if group_set.contains(name.as_ref()) { continue; } // group container, not a project
            if path.join(".git").exists() { continue; }
            out.push(path);
        }
    }

    // Tier 2: direct children of each group folder
    for group in groups {
        let group_dir = root.join(group);
        if !group_dir.is_dir() { continue; }
        if let Ok(entries) = std::fs::read_dir(&group_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with('.') { continue; }
                if path.join(".git").exists() { continue; }
                out.push(path);
            }
        }
    }

    out.sort();
    out
}

/// Core adopt operation for a single folder:
/// git init → stage all → initial commit → create remote repo → add remote → push
pub async fn adopt_folder(
    path: &Path,
    cfg: &AppConfig,
    private: bool,
    commit_msg: &str,
    provider_hint: Option<&str>,
) -> AdoptResult {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    // ── git init ─────────────────────────────────────────────────────────────
    let repo = match Repository::init(path) {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("git init: {}", e)),
    };

    // ── stage everything ─────────────────────────────────────────────────────
    let mut index = match repo.index() {
        Ok(i)  => i,
        Err(e) => return AdoptResult::err(&name, format!("index: {}", e)),
    };
    if let Err(e) = index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None) {
        return AdoptResult::err(&name, format!("git add: {}", e));
    }
    if let Err(e) = index.write() {
        return AdoptResult::err(&name, format!("index write: {}", e));
    }

    // ── initial commit ────────────────────────────────────────────────────────
    let tree_id = match index.write_tree() {
        Ok(id) => id,
        Err(e) => return AdoptResult::err(&name, format!("write tree: {}", e)),
    };
    let tree = match repo.find_tree(tree_id) {
        Ok(t)  => t,
        Err(e) => return AdoptResult::err(&name, format!("find tree: {}", e)),
    };
    let sig = match Signature::now(&cfg.username, &cfg.email) {
        Ok(s)  => s,
        Err(e) => return AdoptResult::err(&name, format!("signature: {}", e)),
    };
    if let Err(e) = repo.commit(Some("HEAD"), &sig, &sig, commit_msg, &tree, &[]) {
        return AdoptResult::err(&name, format!("commit: {}", e));
    }

    // ── create remote repo via API ────────────────────────────────────────────
    let accounts = load_accounts(cfg).unwrap_or_default();
    let acc = match default_account(&accounts, provider_hint.or(Some(&cfg.default_provider))) {
        Some(a) => a,
        None    => return AdoptResult::err(&name, "no provider account configured — run: fxg auth add"),
    };
    let provider = match make_provider(acc) {
        Ok(p)  => p,
        Err(e) => return AdoptResult::err(&name, format!("provider: {}", e)),
    };
    let repo_info = match provider.create_repo(&name, private, "").await {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("create remote repo: {}", e)),
    };

    // ── git remote add origin ─────────────────────────────────────────────────
    // Prefer SSH URL; fall back to HTTPS clone URL
    let remote_url = if !repo_info.ssh_url.is_empty() {
        repo_info.ssh_url.clone()
    } else {
        repo_info.url.clone()
    };

    if let Err(e) = repo.remote("origin", &remote_url) {
        return AdoptResult::err(&name, format!("remote add: {}", e));
    }

    // ── push ──────────────────────────────────────────────────────────────────
    let token = crate::auth::get_token(&acc.id()).unwrap_or_default();
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |_url, user, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            git2::Cred::ssh_key_from_agent(user.unwrap_or("git"))
        } else {
            git2::Cred::userpass_plaintext(user.unwrap_or("git"), &token)
        }
    });
    let mut push_opts = git2::PushOptions::new();
    push_opts.remote_callbacks(cb);

    let mut remote = match repo.find_remote("origin") {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("find remote: {}", e)),
    };

    let branch = repo.head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .unwrap_or_else(|| cfg.default_branch.clone());

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
    if let Err(e) = remote.push(&[&refspec], Some(&mut push_opts)) {
        return AdoptResult::err(&name, format!("push: {}", e));
    }

    AdoptResult::ok(&name, format!("pushed {}", branch), repo_info.url)
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(args: AdoptArgs, cfg: &AppConfig) -> Result<()> {
    let private = args.visibility != "public";
    let provider_hint = args.provider.as_deref();

    // Single named folder
    if let Some(name) = &args.name {
        let path = cfg.workspace_root.join(name);
        if !path.is_dir() {
            anyhow::bail!("folder '{}' not found in workspace_root", name);
        }
        if path.join(".git").exists() {
            anyhow::bail!("'{}' already has a git repo", name);
        }
        let result = adopt_folder(&path, cfg, private, &args.message, provider_hint).await;
        print_result(&result);
        return Ok(());
    }

    let candidates = scan_non_git(&cfg.workspace_root, &cfg.workspace_groups);
    if candidates.is_empty() {
        println!("Wszystkie foldery w workspace_root już mają .git. Nic do adoptowania.");
        return Ok(());
    }

    if args.all {
        // Batch — no TUI
        println!("Adoptuję {} folderów...\n", candidates.len());
        for path in &candidates {
            let result = adopt_folder(path, cfg, private, &args.message, provider_hint).await;
            print_result(&result);
        }
    } else {
        // Interactive TUI
        ui::run_adopt_tui(candidates, &cfg.workspace_root, cfg, private, &args.message, provider_hint).await?;
    }

    Ok(())
}

fn print_result(r: &AdoptResult) {
    if r.success {
        let url = r.url.as_deref().unwrap_or("");
        println!("  ✓ {}  →  {}", r.name, url);
    } else {
        println!("  ✗ {}  —  {}", r.name, r.detail);
    }
}

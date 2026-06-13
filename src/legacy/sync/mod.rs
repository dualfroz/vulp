pub mod pr;
pub mod ui;

use crate::cli::{BranchAction, SyncAction, SyncArgs};
use crate::config::AppConfig;
use crate::workspace::{load_workspaces, resolve_paths};
use anyhow::Result;
use git2::{build::CheckoutBuilder, BranchType, IndexAddOption, Repository, Signature};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub repo:    String,
    pub action:  String,
    pub success: bool,
    pub detail:  String,
}

impl SyncResult {
    pub fn ok(repo: &str, action: &str, detail: impl Into<String>) -> Self {
        Self { repo: repo.into(), action: action.into(), success: true, detail: detail.into() }
    }
    pub fn err(repo: &str, action: &str, detail: impl Into<String>) -> Self {
        Self { repo: repo.into(), action: action.into(), success: false, detail: detail.into() }
    }
}

fn make_callbacks(token: String) -> git2::RemoteCallbacks<'static> {
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |_url, user, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) { git2::Cred::ssh_key_from_agent(user.unwrap_or("git")) }
        else { git2::Cred::userpass_plaintext(user.unwrap_or("git"), &token) }
    });
    cb
}

pub fn pull_repo(path: &Path, token: &str) -> Result<SyncResult> {
    let repo   = Repository::open(path)?;
    let name   = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let branch = repo.head()?.shorthand().unwrap_or("main").to_string();

    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.remote_callbacks(make_callbacks(token.to_string()));
    let mut remote = repo.find_remote("origin")?;
    remote.fetch(&[&branch], Some(&mut fetch_opts), None)?;

    let fetch_head   = repo.find_reference("FETCH_HEAD")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

    if analysis.is_up_to_date() { return Ok(SyncResult::ok(&name, "pull", "Already up to date")); }
    if analysis.is_fast_forward() {
        let refname = format!("refs/heads/{}", branch);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(fetch_commit.id(), "Fast-forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(CheckoutBuilder::default().force()))?;
        return Ok(SyncResult::ok(&name, "pull", "Fast-forwarded"));
    }
    Ok(SyncResult::err(&name, "pull", "Merge required — not fast-forward"))
}

pub fn commit_repo(path: &Path, message: &str, username: &str, email: &str) -> Result<SyncResult> {
    let repo  = Repository::open(path)?;
    let name  = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;
    let sig  = Signature::now(username, email)?;
    let parent = repo.head()?.peel_to_commit()?;
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;
    Ok(SyncResult::ok(&name, "commit", message))
}

pub fn push_repo(path: &Path, token: &str, force_with_lease: bool) -> Result<SyncResult> {
    let repo   = Repository::open(path)?;
    let name   = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let branch = repo.head()?.shorthand().unwrap_or("main").to_string();
    let mut remote = repo.find_remote("origin")?;
    let mut opts   = git2::PushOptions::new();
    opts.remote_callbacks(make_callbacks(token.to_string()));
    let prefix = if force_with_lease { "+" } else { "" };
    let refspec = format!("{}refs/heads/{1}:refs/heads/{1}", prefix, branch);
    remote.push(&[&refspec], Some(&mut opts))?;
    Ok(SyncResult::ok(&name, "push", format!("Pushed {}", branch)))
}

pub fn create_branch(path: &Path, name: &str) -> Result<()> {
    let repo   = Repository::open(path)?;
    let commit = repo.head()?.peel_to_commit()?;
    repo.branch(name, &commit, false)?;
    Ok(())
}

pub fn delete_branch(path: &Path, name: &str) -> Result<()> {
    let repo = Repository::open(path)?;
    repo.find_branch(name, BranchType::Local)?.delete()?;
    Ok(())
}

pub fn switch_branch(path: &Path, name: &str) -> Result<()> {
    let repo = Repository::open(path)?;
    let obj  = repo.revparse_single(&format!("refs/heads/{}", name))?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&format!("refs/heads/{}", name))?;
    Ok(())
}

fn repos_for_workspace(workspace: &str, cfg: &AppConfig) -> Result<Vec<crate::status::RepoSummary>> {
    let wss = load_workspaces(cfg)?;
    let ws  = wss.iter().find(|w| w.name == workspace)
        .ok_or_else(|| anyhow::anyhow!("workspace '{}' not found", workspace))?;
    let paths = resolve_paths(ws, cfg);
    let mut repos = Vec::new();
    for path in paths {
        if let Ok(s) = crate::status::repo_summary(&path) { repos.push(s); }
    }
    Ok(repos)
}

fn print_result(sr: &SyncResult) {
    println!("  {} {}: {}", if sr.success { "✓" } else { "✗" }, sr.repo, sr.detail);
}

pub async fn run(args: SyncArgs, cfg: &AppConfig) -> Result<()> {
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    let token = crate::providers::default_account(&accounts, Some(&cfg.default_provider))
        .and_then(|a| crate::auth::get_token(&a.id()).ok())
        .unwrap_or_default();

    match args.action {
        None => {
            let repos = crate::status::scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;
            ui::run_sync_tui(repos, cfg)
        }
        Some(SyncAction::Pull { all, workspace }) => {
            let repos = if let Some(ws) = workspace {
                repos_for_workspace(&ws, cfg)?
            } else if all {
                crate::status::scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?
            } else {
                let cwd = std::env::current_dir()?;
                vec![crate::status::repo_summary(&cwd)?]
            };
            for r in &repos {
                print_result(&pull_repo(&r.path, &token).unwrap_or_else(|e| SyncResult::err(&r.name, "pull", e.to_string())));
            }
            Ok(())
        }
        Some(SyncAction::Commit { message, all }) => {
            let msg = message.unwrap_or_else(|| "chore: sync".into());
            let repos = if all { crate::status::scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)? }
                        else { vec![crate::status::repo_summary(&std::env::current_dir()?)? ] };
            for r in repos.iter().filter(|r| r.is_dirty) {
                print_result(&commit_repo(&r.path, &msg, &cfg.username, &cfg.email)
                    .unwrap_or_else(|e| SyncResult::err(&r.name, "commit", e.to_string())));
            }
            Ok(())
        }
        Some(SyncAction::Push { all, force_with_lease, message }) => {
            let repos = if all { crate::status::scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)? }
                        else { vec![crate::status::repo_summary(&std::env::current_dir()?)? ] };
            // If -m given, commit dirty repos first
            if let Some(msg) = &message {
                for r in repos.iter().filter(|r| r.is_dirty) {
                    let _ = commit_repo(&r.path, msg, &cfg.username, &cfg.email);
                }
            }
            for r in &repos {
                print_result(&push_repo(&r.path, &token, force_with_lease)
                    .unwrap_or_else(|e| SyncResult::err(&r.name, "push", e.to_string())));
            }
            Ok(())
        }
        Some(SyncAction::Branch(bargs)) => {
            match bargs.action {
                BranchAction::Create { name, repos } => {
                    let paths: Vec<_> = if repos.is_empty() {
                        vec![std::env::current_dir()?]
                    } else {
                        repos.iter().map(|r| cfg.workspace_root.join(r)).collect()
                    };
                    for path in &paths {
                        let rname = path.file_name().unwrap_or_default().to_string_lossy();
                        match create_branch(path, &name) {
                            Ok(())  => println!("  ✓ {} — created branch {}", rname, name),
                            Err(e)  => println!("  ✗ {}: {}", rname, e),
                        }
                    }
                }
                BranchAction::Delete { name } => delete_branch(&std::env::current_dir()?, &name)?,
                BranchAction::Switch { name, all } => {
                    let repos = if all { crate::status::scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)? }
                                else { vec![crate::status::repo_summary(&std::env::current_dir()?)? ] };
                    for r in &repos { let _ = switch_branch(&r.path, &name); }
                }
            }
            Ok(())
        }
        Some(SyncAction::Pr(pr_args)) => pr::run(pr_args, cfg).await,
    }
}

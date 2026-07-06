//! Onboards a local folder onto a remote git provider: either from scratch
//! (`adopt_folder` — git init, stage, commit, create remote, push) or for a
//! folder that already has commits (`publish_existing` — just create the
//! remote and push). Called by the hub's "Create new repo" and "Link
//! existing folder" flows.

use crate::auth::load_accounts;
use crate::config::AppConfig;
use crate::providers::{default_account, make_provider};
use git2::{IndexAddOption, Repository, Signature};
use std::path::Path;

/// Outcome of an adopt/publish operation, reported back to the TUI's
/// progress modal.
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

/// Core adopt operation for a single folder:
/// git init → stage all → initial commit → create remote repo → add remote → push
pub async fn adopt_folder(
    path: &Path,
    cfg: &AppConfig,
    private: bool,
    commit_msg: &str,
    provider_hint: Option<&str>,
    owner: Option<&str>,
    progress: impl Fn(&str),
) -> AdoptResult {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    // ── git init ─────────────────────────────────────────────────────────────
    progress("git init");
    let repo = match Repository::init(path) {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("git init: {}", e)),
    };

    // ── stage everything ─────────────────────────────────────────────────────
    progress("staging files");
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
    progress("creating initial commit");
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
    progress("resolving provider account");
    let accounts = load_accounts(cfg).unwrap_or_default();
    let acc = match default_account(&accounts, provider_hint.or(Some(&cfg.default_provider))) {
        Some(a) => a,
        None    => return AdoptResult::err(&name, "no provider account configured — run: vulp auth add"),
    };
    let provider = match make_provider(acc) {
        Ok(p)  => p,
        Err(e) => return AdoptResult::err(&name, format!("provider: {}", e)),
    };
    progress(&format!("creating repo on {} via API", acc.provider));
    let repo_info = match provider.create_repo(&name, private, "", owner).await {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("create remote repo: {}", e)),
    };

    // ── git remote add origin ─────────────────────────────────────────────────
    // Prefer HTTPS: push auth below uses the stored token, which works over HTTPS
    // without needing the remote's SSH host key to already be trusted locally.
    let remote_url = if !repo_info.url.is_empty() {
        repo_info.url.clone()
    } else {
        repo_info.ssh_url.clone()
    };

    progress(&format!("git remote add origin {}", remote_url));
    if let Err(e) = repo.remote("origin", &remote_url) {
        return AdoptResult::err(&name, format!("remote add: {}", e));
    }

    // ── push ──────────────────────────────────────────────────────────────────
    progress("pushing to origin");
    let token  = crate::auth::get_token(&acc.id()).unwrap_or_default();
    let branch = match push_branch(&repo, cfg, token) {
        Ok(b)  => b,
        Err(e) => return AdoptResult::err(&name, format!("push: {}", e)),
    };

    AdoptResult::ok(&name, format!("pushed {}", branch), repo_info.url)
}

/// Pushes the current HEAD branch to `origin`, returning the branch name.
/// Shared by `adopt_folder` (brand-new repo) and `publish_existing` (folder
/// that already has commits, just needs a remote).
fn push_branch(repo: &Repository, cfg: &AppConfig, token: String) -> std::result::Result<String, git2::Error> {
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

    let mut remote = repo.find_remote("origin")?;
    let branch = repo.head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .unwrap_or_else(|| cfg.default_branch.clone());

    let refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
    remote.push(&[&refspec], Some(&mut push_opts))?;
    Ok(branch)
}

/// For a folder that already has git commits but isn't on a remote yet:
/// create the remote repo (if `origin` doesn't already exist) and push,
/// without touching any existing history.
pub async fn publish_existing(
    path: &Path,
    cfg: &AppConfig,
    private: bool,
    provider_hint: Option<&str>,
    owner: Option<&str>,
    progress: impl Fn(&str),
) -> AdoptResult {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

    progress("opening existing repo");
    let repo = match Repository::open(path) {
        Ok(r)  => r,
        Err(e) => return AdoptResult::err(&name, format!("open repo: {}", e)),
    };

    progress("resolving provider account");
    let accounts = load_accounts(cfg).unwrap_or_default();
    let acc = match default_account(&accounts, provider_hint.or(Some(&cfg.default_provider))) {
        Some(a) => a,
        None    => return AdoptResult::err(&name, "no provider account configured — run: vulp auth add"),
    };

    let remote_url = match repo.find_remote("origin") {
        Ok(r) => r.url().unwrap_or_default().to_string(),
        Err(_) => {
            let provider = match make_provider(acc) {
                Ok(p)  => p,
                Err(e) => return AdoptResult::err(&name, format!("provider: {}", e)),
            };
            progress(&format!("creating repo on {} via API", acc.provider));
            let repo_info = match provider.create_repo(&name, private, "", owner).await {
                Ok(r)  => r,
                Err(e) => return AdoptResult::err(&name, format!("create remote repo: {}", e)),
            };
            let url = if !repo_info.url.is_empty() { repo_info.url.clone() } else { repo_info.ssh_url.clone() };
            progress(&format!("git remote add origin {}", url));
            if let Err(e) = repo.remote("origin", &url) {
                return AdoptResult::err(&name, format!("remote add: {}", e));
            }
            url
        }
    };

    progress("pushing to origin");
    let token  = crate::auth::get_token(&acc.id()).unwrap_or_default();
    let branch = match push_branch(&repo, cfg, token) {
        Ok(b)  => b,
        Err(e) => return AdoptResult::err(&name, format!("push: {}", e)),
    };

    AdoptResult::ok(&name, format!("pushed {}", branch), remote_url)
}


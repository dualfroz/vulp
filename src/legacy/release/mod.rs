pub mod ui;

use crate::auth::load_accounts;
use crate::cli::{ReleaseAction, ReleaseArgs};
use crate::config::AppConfig;
use crate::providers::make_provider;
use anyhow::Result;
use git2::{Repository, Signature};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ChangelogEntry {
    pub kind:    String,
    pub scope:   Option<String>,
    pub message: String,
    pub sha:     String,
}

fn parse_cc(subject: &str) -> Option<(String, Option<String>, String)> {
    let (head, tail) = subject.split_once(':')?;
    let msg = tail.trim().to_string();
    if let Some((kind, scope_raw)) = head.split_once('(') {
        Some((kind.to_string(), Some(scope_raw.trim_end_matches(')').to_string()), msg))
    } else {
        Some((head.to_string(), None, msg))
    }
}

pub fn generate_changelog(repo: &Repository, since_tag: Option<&str>) -> Result<Vec<ChangelogEntry>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    let stop_id = since_tag.and_then(|t| repo.revparse_single(t).ok().map(|o| o.id()));
    let mut entries = Vec::new();
    for id in revwalk {
        let id = id?;
        if stop_id.map(|s| s == id).unwrap_or(false) { break; }
        let commit = repo.find_commit(id)?;
        if let Some((kind, scope, message)) = parse_cc(commit.summary().unwrap_or("")) {
            entries.push(ChangelogEntry { kind, scope, message, sha: id.to_string()[..7].to_string() });
        }
    }
    Ok(entries)
}

pub fn format_changelog_md(entries: &[ChangelogEntry], version: &str) -> String {
    let mut out = format!("## {}\n\n", version);
    for (section, header) in &[("feat","### Features"),("fix","### Bug Fixes"),("refactor","### Refactoring"),("chore","### Chores"),("docs","### Documentation")] {
        let group: Vec<_> = entries.iter().filter(|e| e.kind == *section).collect();
        if group.is_empty() { continue; }
        out.push_str(&format!("{}\n\n", header));
        for e in group {
            let scope_str = e.scope.as_ref().map(|s| format!("**{}**: ", s)).unwrap_or_default();
            out.push_str(&format!("- {}{} ({})\n", scope_str, e.message, e.sha));
        }
        out.push('\n');
    }
    out
}

fn latest_tag(repo: &Repository) -> Option<String> {
    repo.tag_names(None).ok()?.iter().flatten().map(|s| s.to_string()).last()
}

fn bump_version(tag: Option<&str>, release_type: &str) -> String {
    let v = tag.unwrap_or("v0.0.0").trim_start_matches('v');
    let p: Vec<u64> = v.split('.').filter_map(|x| x.parse().ok()).collect();
    let (mj, mn, pt) = (p.first().copied().unwrap_or(0), p.get(1).copied().unwrap_or(0), p.get(2).copied().unwrap_or(0));
    match release_type { "major" => format!("v{}.0.0", mj+1), "minor" => format!("v{}.{}.0", mj, mn+1), _ => format!("v{}.{}.{}", mj, mn, pt+1) }
}

fn create_local_tag(repo: &Repository, version: &str, cfg: &AppConfig) -> Result<()> {
    let head = repo.head()?.peel_to_commit()?;
    let sig  = Signature::now(&cfg.username, &cfg.email)?;
    repo.tag(version, head.as_object(), &sig, &format!("Release {}", version), false)?;
    Ok(())
}

fn push_tag(repo: &Repository, token: &str, version: &str) -> Result<()> {
    let t = token.to_string();
    let mut remote = repo.find_remote("origin")?;
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |_url, user, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) { git2::Cred::ssh_key_from_agent(user.unwrap_or("git")) }
        else { git2::Cred::userpass_plaintext(user.unwrap_or("git"), &t) }
    });
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(cb);
    let refspec = format!("refs/tags/{0}:refs/tags/{0}", version);
    remote.push(&[&refspec], Some(&mut opts))?;
    Ok(())
}

fn tag_one_repo(path: &Path, version: &str, token: &str, cfg: &AppConfig) -> Result<()> {
    let repo = Repository::open(path)?;
    create_local_tag(&repo, version, cfg)?;
    push_tag(&repo, token, version)?;
    Ok(())
}

fn find_repo_upward() -> Result<(Repository, std::path::PathBuf)> {
    let mut current = std::env::current_dir()?;
    loop {
        if current.join(".git").exists() {
            let repo = Repository::open(&current)?;
            return Ok((repo, current));
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None    => anyhow::bail!(
                "not inside a git repository — run from within a repo, or cd into one first"
            ),
        }
    }
}

pub async fn run(args: ReleaseArgs, cfg: &AppConfig) -> Result<()> {
    let (repo, cwd) = find_repo_upward()?;
    let accounts = load_accounts(cfg)?;
    let account  = crate::providers::default_account(&accounts, None)
        .ok_or_else(|| anyhow::anyhow!("no account configured — run: fxg auth add"))?;
    let token    = crate::auth::get_token(&account.id())?;

    match args.action {
        None => {
            let entries = generate_changelog(&repo, latest_tag(&repo).as_deref())?;
            if let Some((version, title, body, draft)) = ui::run_release_tui(&repo, entries)? {
                println!("  Publishing {}…", version);
                create_local_tag(&repo, &version, cfg)?;
                push_tag(&repo, &token, &version)?;
                println!("  ✓ Tag pushed");
                let provider = make_provider(account)?;
                let repo_name = cwd.file_name().unwrap_or_default().to_string_lossy();
                match provider.create_release(&repo_name, &version, &title, &body, draft).await {
                    Ok(url) => println!("  ✓ Release created: {}", url),
                    Err(e)  => println!("  ✗ Release API: {}", e),
                }
            }
        }
        Some(ReleaseAction::Create { version, release_type, generate_changelog: gc }) => {
            let last = latest_tag(&repo);
            let ver  = version.unwrap_or_else(|| bump_version(last.as_deref(), &release_type));
            let body = if gc { let e = generate_changelog(&repo, last.as_deref())?; format_changelog_md(&e, &ver) } else { String::new() };
            create_local_tag(&repo, &ver, cfg)?;
            println!("✓ Created tag {}", ver);
            if let Err(e) = push_tag(&repo, &token, &ver) { println!("  ⚠ push tag: {}", e); } else { println!("✓ Pushed tag"); }
            if !body.is_empty() {
                let provider = make_provider(account)?;
                let repo_name = cwd.file_name().unwrap_or_default().to_string_lossy();
                match provider.create_release(&repo_name, &ver, &ver, &body, false).await {
                    Ok(url) => println!("✓ Release: {}", url),
                    Err(e)  => println!("⚠ Release API: {}", e),
                }
            }
        }
        Some(ReleaseAction::List) => {
            let names = repo.tag_names(None)?;
            for name in names.iter().flatten() { println!("  {}", name); }
        }
        Some(ReleaseAction::Tag { version, repos }) => {
            let targets: Vec<_> = if repos.is_empty() {
                vec![cwd.clone()]
            } else {
                repos.iter().map(|r| cfg.workspace_root.join(r)).collect()
            };
            for path in targets {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                match tag_one_repo(&path, &version, &token, cfg) {
                    Ok(())  => println!("  ✓ {} tagged {}", name, version),
                    Err(e)  => println!("  ✗ {}: {}", name, e),
                }
            }
        }
    }
    Ok(())
}

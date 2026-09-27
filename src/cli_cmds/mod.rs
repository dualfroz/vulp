mod digest;
mod items;
mod remote;
mod workspace;

use crate::cli::Commands;
use crate::config::AppConfig;
use anyhow::Result;

#[derive(Clone, Copy)]
pub(crate) struct Guard {
    pub dry_run: bool,
    pub force:   bool,
}

pub async fn run(cmd: Commands, json: bool, guard: Guard, cfg: &AppConfig) -> Result<()> {
    match cmd {
        Commands::List { dirty, behind, ahead, lang, project, alerts, stale_deploy, no_external, external } =>
            workspace::list(cfg, json, dirty, behind, ahead, lang, project, alerts, stale_deploy, no_external, external),
        Commands::Status { repo }   => workspace::status(cfg, json, &repo),
        Commands::Scan              => workspace::scan_cmd(cfg, json),
        Commands::Sync { repos, all, project, action, message, allow_secrets } =>
            workspace::sync(cfg, guard, repos, all, project, &action, message, allow_secrets),
        Commands::Release { repo, tag, notes } =>
            remote::release(cfg, guard, &repo, &tag, &notes).await,
        Commands::Brief { repo }    => workspace::brief(cfg, json, repo),
        Commands::Stats { period }  => workspace::stats(cfg, json, &period),
        Commands::Activity { limit, kind } => workspace::activity(json, limit, kind),

        Commands::Todo { action }    => items::todo(json, action),
        Commands::Project { action } => items::project(cfg, json, action),

        Commands::Pr { action }      => remote::pr(cfg, json, action).await,
        Commands::Issue { action }   => remote::issue(cfg, json, action).await,
        Commands::Repo { action }    => remote::repo_cmd(cfg, json, guard, action).await,
        Commands::Init { name, private, description } =>
            remote::init(cfg, guard, &name, private, &description).await,
        Commands::Accounts           => remote::accounts(cfg, json),
        Commands::Refresh            => remote::refresh(cfg, json).await,
        Commands::Digest { action }  => digest::digest(cfg, guard, json, action).await,

        Commands::Config | Commands::Auth(_) => unreachable!("handled in app.rs"),
    }
}

pub(crate) fn die(code: i32, msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(code);
}

pub(crate) fn out_json<T: serde::Serialize>(v: &T) {
    match serde_json::to_string_pretty(v) {
        Ok(s) => println!("{s}"),
        Err(e) => die(1, &format!("JSON serialization failed: {e}")),
    }
}

pub(crate) fn scan_repos(cfg: &AppConfig) -> Vec<crate::status::RepoSummary> {
    let mut repos = match crate::status::scan_workspace(&cfg.workspace_root, &[]) {
        Ok(r) => r,
        Err(e) => die(1, &format!("workspace scan failed: {e:#}")),
    };
    let cache = crate::cache::load();
    for r in &mut repos {
        if let Some(entry) = cache.get(&r.name) {
            let m = &entry.meta;
            r.stars           = m.stars;
            r.open_issues     = m.open_issues;
            r.open_prs        = m.open_prs;
            r.ci_status       = m.ci_status.clone();
            r.dependabot_open = m.dependabot_open;
            r.deploy_status   = m.deploy_status.clone();
            if r.visibility.is_none() { r.visibility = m.visibility.clone(); }
        }
    }
    repos
}

pub(crate) fn find_repo(cfg: &AppConfig, name: &str) -> crate::status::RepoSummary {
    let repos = scan_repos(cfg);
    let needle = name.to_lowercase();
    let hit = repos.iter().find(|r| {
        r.name.to_lowercase() == needle
            || r.path.file_name().map(|b| b.to_string_lossy().to_lowercase() == needle).unwrap_or(false)
    });
    match hit {
        Some(r) => r.clone(),
        None => die(4, &format!("repo '{name}' not found in {}", cfg.workspace_root.display())),
    }
}

pub(crate) fn default_provider(cfg: &AppConfig) -> (crate::auth::Account, Box<dyn crate::providers::Provider>) {
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    let Some(acc) = accounts.iter().find(|a| a.is_default).or(accounts.first()).cloned() else {
        die(3, "no accounts configured — run: vulp auth import");
    };
    match crate::providers::make_provider(&acc) {
        Ok(p) => (acc, p),
        Err(e) => die(3, &format!("provider auth failed: {e:#}")),
    }
}

pub(crate) fn is_protected(cfg: &AppConfig, r: &crate::status::RepoSummary) -> bool {
    let name = r.name.to_lowercase();
    let base = r.path.file_name().map(|b| b.to_string_lossy().to_lowercase()).unwrap_or_default();
    cfg.protected_repos.iter().any(|p| {
        let p = p.trim().to_lowercase();
        p == name || p == base
    })
}

pub(crate) fn guard_protected(cfg: &AppConfig, force: bool, targets: &[&crate::status::RepoSummary]) {
    if force { return; }
    let blocked: Vec<&str> = targets.iter()
        .filter(|r| is_protected(cfg, r))
        .map(|r| r.name.as_str())
        .collect();
    if !blocked.is_empty() {
        die(1, &format!("protected repos: {} — pass --force to proceed", blocked.join(", ")));
    }
}

pub(crate) fn log_activity(kind: crate::activity::ActivityKind, repo: &str, detail: String) {
    crate::activity::log(&crate::activity::ActivityEntry {
        ts: chrono::Utc::now().timestamp(),
        kind,
        repo: repo.to_string(),
        detail,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, path: &str) -> crate::status::RepoSummary {
        crate::status::RepoSummary {
            name: name.into(), path: path.into(), branch: "main".into(),
            ahead: 0, behind: 0, is_dirty: false,
            last_commit_msg: String::new(), last_commit_age: String::new(), last_commit_ts: 0,
            last_tag: None, visibility: None, branch_count: 1, owner: None, remote_repo_name: None,
            has_license: false, lang: None, bookmarked: false,
            stars: None, open_issues: None, open_prs: None, stash_count: 0,
            ci_status: None, dependabot_open: None, deploy_status: None,
        }
    }

    #[test]
    fn protection_matches_label_or_directory_case_insensitively() {
        let cfg = AppConfig { protected_repos: vec![" Billing ".into(), "infra".into()], ..AppConfig::default() };
        assert!(is_protected(&cfg, &repo("billing", "/ws/billing")));
        assert!(is_protected(&cfg, &repo("Production Infra", "/ws/ops/INFRA")));
        assert!(!is_protected(&cfg, &repo("billing-ui", "/ws/billing-ui")));
        assert!(!is_protected(&AppConfig::default(), &repo("billing", "/ws/billing")));
    }

    #[test]
    fn force_skips_the_protection_check() {
        let cfg = AppConfig { protected_repos: vec!["billing".into()], ..AppConfig::default() };
        let target = repo("billing", "/ws/billing");
        guard_protected(&cfg, true, &[&target]);
        guard_protected(&cfg, false, &[&repo("other", "/ws/other")]);
    }
}

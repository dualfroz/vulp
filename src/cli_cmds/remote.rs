//! Provider-API CLI commands: PRs, issues, releases, repo utilities,
//! `vulp init`, account listing and the blocking `vulp refresh`.

use super::{die, default_provider, find_repo, log_activity, out_json};
use crate::activity::ActivityKind;
use crate::cli::{IssueCmd, PrCmd, RepoCmd};
use crate::config::AppConfig;
use crate::util::git::{git_run, git_run_ok};
use anyhow::Result;

pub async fn pr(cfg: &AppConfig, json: bool, cmd: PrCmd) -> Result<()> {
    match cmd {
        PrCmd::List { repo } => {
            let cache = crate::cache::load();
            let mut prs: Vec<crate::providers::PrDetail> =
                cache.all_prs().into_iter().cloned().collect();
            if let Some(r) = repo {
                prs.retain(|p| p.repo.eq_ignore_ascii_case(&r));
            }
            prs.sort_by_key(|p| std::cmp::Reverse(p.created_ts));
            if json {
                out_json(&prs);
            } else if prs.is_empty() {
                println!("no cached open PRs (run: vulp refresh)");
            } else {
                for p in &prs {
                    println!("#{:<5} {:<24} {} ({} → {})", p.number, p.repo, p.title, p.head_branch, p.base_branch);
                }
            }
            Ok(())
        }
        PrCmd::Create { repo, title, head, base } => {
            let (_, provider) = default_provider(cfg);
            match provider.create_pull_request(&repo, &title, "", &head, &base).await {
                Ok(info) => {
                    log_activity(ActivityKind::Pr, &repo, format!("#{} {}", info.number, title));
                    println!("{}", info.url);
                    Ok(())
                }
                Err(e) => die(1, &format!("PR creation failed: {e:#}")),
            }
        }
    }
}

pub async fn issue(cfg: &AppConfig, json: bool, cmd: IssueCmd) -> Result<()> {
    match cmd {
        IssueCmd::List { repo } => {
            let cache = crate::cache::load();
            let mut issues: Vec<crate::providers::IssueDetail> =
                cache.all_issues().into_iter().cloned().collect();
            if let Some(r) = repo {
                issues.retain(|i| i.repo.eq_ignore_ascii_case(&r));
            }
            issues.sort_by_key(|i| std::cmp::Reverse(i.created_ts));
            if json {
                out_json(&issues);
            } else if issues.is_empty() {
                println!("no cached open issues (run: vulp refresh)");
            } else {
                for i in &issues {
                    println!("#{:<5} {:<24} {} ({} comments)", i.number, i.repo, i.title, i.comments);
                }
            }
            Ok(())
        }
        IssueCmd::Create { repo, title, body } => {
            let (_, provider) = default_provider(cfg);
            match provider.create_issue(&repo, &title, &body).await {
                Ok(info) => {
                    log_activity(ActivityKind::Issue, &repo, format!("#{} {}", info.number, title));
                    println!("{}", info.url);
                    Ok(())
                }
                Err(e) => die(1, &format!("issue creation failed: {e:#}")),
            }
        }
    }
}

pub async fn release(cfg: &AppConfig, guard: super::Guard, repo: &str, tag: &str, notes: &str) -> Result<()> {
    let r = find_repo(cfg, repo);
    super::guard_protected(cfg, guard.force, &[&r]);
    if guard.dry_run {
        println!("DRY RUN: would tag {} as {tag}, push the tag, and publish a provider release", r.name);
        return Ok(());
    }
    let msg = if notes.is_empty() { tag } else { notes };
    git_run(&r.path, &["tag", "-a", tag, "-m", msg]);
    if !git_run_ok(&r.path, &["push", "origin", tag]) {
        die(1, &format!("pushing tag '{tag}' failed"));
    }
    // Publikacja release'a na providerze — best effort: tag już wypchnięty,
    // brak konta/tokena nie powinien cofać release'a gitowego.
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    if let Some(acc) = accounts.iter().find(|a| a.is_default).or(accounts.first()) {
        if let Ok(provider) = crate::providers::make_provider(acc) {
            let repo_basename = r.path.file_name().map(|b| b.to_string_lossy().to_string())
                .unwrap_or_else(|| r.name.clone());
            match provider.create_release(&repo_basename, tag, tag, notes, false).await {
                Ok(url) => println!("{url}"),
                Err(e)  => eprintln!("warning: tag pushed, but provider release failed: {e:#}"),
            }
        }
    }
    log_activity(ActivityKind::Release, &r.name, tag.to_string());
    println!("released {tag} on {}", r.name);
    Ok(())
}

pub async fn repo_cmd(cfg: &AppConfig, json: bool, guard: super::Guard, cmd: RepoCmd) -> Result<()> {
    match cmd {
        RepoCmd::Url { repo } => {
            let r = find_repo(cfg, &repo);
            match crate::hub::actions::get_remote_url(&r.path) {
                Some(url) => { println!("{url}"); Ok(()) }
                None => die(4, "repo has no 'origin' remote"),
            }
        }
        RepoCmd::Path { repo } => {
            let r = find_repo(cfg, &repo);
            println!("{}", r.path.display());
            Ok(())
        }
        RepoCmd::Open { repo } => {
            let r = find_repo(cfg, &repo);
            match crate::hub::actions::get_remote_url(&r.path) {
                Some(url) => {
                    let web = crate::util::url::to_web_url(&url);
                    crate::util::open_path(&web);
                    println!("{web}");
                    Ok(())
                }
                None => die(4, "repo has no 'origin' remote"),
            }
        }
        RepoCmd::Label { repo, name } => {
            let r = find_repo(cfg, &repo);
            crate::labels::save_label(&r.path, &name, &cfg.workspace_root)?;
            if name.is_empty() {
                println!("label cleared for {}", r.path.display());
            } else {
                println!("label '{name}' set");
            }
            Ok(())
        }
        RepoCmd::Bookmark { repo } => {
            let r = find_repo(cfg, &repo);
            let mut bk = crate::hub::actions::load_bookmarks();
            let added = bk.insert(r.path.clone());
            if !added { bk.remove(&r.path); }
            crate::hub::actions::save_bookmarks(&bk);
            println!("{} {}", if added { "bookmarked" } else { "unbookmarked" }, r.name);
            Ok(())
        }
        RepoCmd::Pin { repo } => {
            let r = find_repo(cfg, &repo);
            let mut pins = crate::hub::actions::load_pins();
            let added = pins.insert(r.path.clone());
            if !added { pins.remove(&r.path); }
            crate::hub::actions::save_pins(&pins);
            println!("{} {}", if added { "pinned" } else { "unpinned" }, r.name);
            Ok(())
        }
        RepoCmd::Collab { repo, user, role } => {
            if !["pull", "push", "admin"].contains(&role.as_str()) {
                die(2, "role must be pull | push | admin");
            }
            let r = find_repo(cfg, &repo);
            super::guard_protected(cfg, guard.force, &[&r]);
            let repo_basename = r.path.file_name().map(|b| b.to_string_lossy().to_string())
                .unwrap_or_else(|| r.name.clone());
            if guard.dry_run {
                println!("DRY RUN: would invite {user} to {repo_basename} ({role})");
                return Ok(());
            }
            let (_, provider) = default_provider(cfg);
            match provider.add_collaborator(&repo_basename, &user, &role).await {
                Ok(()) => { println!("invited {user} to {repo_basename} ({role})"); Ok(()) }
                Err(e) => die(1, &format!("invite failed: {e:#}")),
            }
        }
        RepoCmd::License { repos, all, kind, holder } => {
            license(cfg, json, guard, repos, all, kind, holder)
        }
    }
}

/// `vulp repo license` — writes a LICENSE into each target repo that lacks
/// one. Explicit names that already have a license are reported as skipped,
/// not errors; `--all` targets every unlicensed workspace repo.
fn license(cfg: &AppConfig, json: bool, guard: super::Guard, repos: Vec<String>, all: bool,
           kind: Option<String>, holder: Option<String>) -> Result<()> {
    use crate::license::{LicenseKind, WriteOutcome};

    if repos.is_empty() && !all {
        die(2, "pass repo names or --all");
    }
    let kind = match kind {
        Some(s) => match LicenseKind::parse(&s) {
            Some(k) => k,
            None    => die(2, "kind must be mit | apache-2.0"),
        },
        None => crate::license::default_kind(cfg),
    };
    let holder = holder.unwrap_or_else(|| crate::license::holder_for(cfg));
    if holder.trim().is_empty() {
        die(2, "no copyright holder — pass --holder or set license_holder/username in config");
    }

    let workspace = super::scan_repos(cfg);
    let targets: Vec<&crate::status::RepoSummary> = if all {
        workspace.iter().filter(|r| !r.has_license).collect()
    } else {
        repos.iter().map(|name| {
            let needle = name.to_lowercase();
            workspace.iter().find(|r| {
                r.name.to_lowercase() == needle
                    || r.path.file_name().map(|b| b.to_string_lossy().to_lowercase() == needle).unwrap_or(false)
            }).unwrap_or_else(|| die(4, &format!("repo '{name}' not found in {}", cfg.workspace_root.display())))
        }).collect()
    };

    super::guard_protected(cfg, guard.force, &targets);
    if guard.dry_run {
        for r in &targets {
            if r.has_license {
                println!("DRY RUN: would skip {} (already has a license)", r.name);
            } else {
                println!("DRY RUN: would write {} LICENSE to {} (holder: {holder})", kind.label(), r.name);
            }
        }
        return Ok(());
    }

    let year = chrono::Datelike::year(&chrono::Utc::now());
    let (mut written, mut skipped, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for r in targets {
        match crate::license::write_license(&r.path, kind, &holder, year) {
            Ok(WriteOutcome::Written) => {
                log_activity(ActivityKind::License, &r.name, format!("{} ({})", kind.label(), holder));
                written.push(r.name.clone());
            }
            Ok(WriteOutcome::AlreadyLicensed) => skipped.push(r.name.clone()),
            Err(e) => {
                eprintln!("error: writing LICENSE to {} failed: {e:#}", r.name);
                failed.push(r.name.clone());
            }
        }
    }

    if json {
        #[derive(serde::Serialize)]
        struct Out { kind: String, holder: String, written: Vec<String>, skipped: Vec<String>, failed: Vec<String> }
        out_json(&Out { kind: kind.label().into(), holder, written, skipped, failed: failed.clone() });
    } else {
        for n in &written { println!("LICENSE ({}) written to {n}", kind.label()); }
        for n in &skipped { println!("skipped {n} (already has a license)"); }
        if written.is_empty() && skipped.is_empty() { println!("nothing to do"); }
    }
    if !failed.is_empty() { std::process::exit(1); }
    Ok(())
}

pub async fn init(cfg: &AppConfig, guard: super::Guard, name: &str, private: bool, description: &str) -> Result<()> {
    let path = cfg.workspace_root.join(name);
    if path.exists() {
        die(1, &format!("{} already exists", path.display()));
    }
    if guard.dry_run {
        println!("DRY RUN: would init {}, create a {} repo on the provider, and push",
                 path.display(), if private { "private" } else { "public" });
        return Ok(());
    }
    let (_, provider) = default_provider(cfg);

    std::fs::create_dir_all(&path)?;
    git_run(&path, &["init", "-b", "main"]);
    std::fs::write(path.join("README.md"), format!("# {name}\n"))?;
    git_run(&path, &["add", "-A"]);
    git_run(&path, &["commit", "-m", "Initial commit"]);

    let info = match provider.create_repo(name, private, description, None).await {
        Ok(i) => i,
        Err(e) => die(1, &format!("provider repo creation failed: {e:#}")),
    };
    git_run(&path, &["remote", "add", "origin", &info.ssh_url]);
    if !git_run_ok(&path, &["push", "-u", "origin", "main"]) {
        die(1, "initial push failed (repo created on provider, local push didn't go through)");
    }
    log_activity(ActivityKind::NewRepo, name, format!("Pushed to {}", info.url));
    println!("{}", info.url);
    Ok(())
}

pub fn accounts(cfg: &AppConfig, json: bool) -> Result<()> {
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    if json {
        out_json(&accounts);
    } else if accounts.is_empty() {
        println!("no accounts (run: vulp auth import)");
    } else {
        for a in &accounts {
            println!("{}{}  @{}  ({})", a.id(), if a.is_default { " *" } else { "" }, a.username, a.provider);
        }
    }
    Ok(())
}

pub async fn refresh(cfg: &AppConfig, json: bool) -> Result<()> {
    use futures::stream::{self, StreamExt};

    let (account, provider) = default_provider(cfg);
    let provider: std::sync::Arc<dyn crate::providers::Provider> = std::sync::Arc::from(provider);
    let _ = account;
    let repos = super::scan_repos(cfg);

    // Org-owned repos are reachable through the personal token — refresh
    // everything with an owner, addressed by full "owner/basename" slug.
    let targets: Vec<(String, String, String)> = repos.iter()
        .filter_map(|r| {
            let basename = r.path.file_name().map(|b| b.to_string_lossy().to_string())
                .unwrap_or_else(|| r.name.clone());
            r.owner.as_ref().map(|o| (r.name.clone(), format!("{}/{}", o, basename), r.branch.clone()))
        })
        .collect();

    // Dependabot alert counts: one org-level request per distinct owner
    // (personal owners 404 → None → unknown).
    let mut dependabot: std::collections::HashMap<String, Option<std::collections::HashMap<String, u32>>> =
        std::collections::HashMap::new();
    {
        let owners: std::collections::HashSet<String> =
            repos.iter().filter_map(|r| r.owner.clone()).collect();
        for owner in owners {
            eprintln!("dependabot alerts: {owner}...");
            let counts = provider.org_dependabot_counts(&owner).await.ok().flatten();
            dependabot.insert(owner.to_lowercase(), counts);
        }
    }

    // Deploy drift: one batched remote command (if configured) lists every
    // mirrored repo's HEAD sha; comparing against local HEAD is then a cheap
    // per-repo git call, folded into the metas loop below.
    let deploy_heads = if crate::deploy::is_configured(cfg) {
        eprintln!("checking deploy drift...");
        crate::deploy::scan_deploy_heads(cfg)
    } else {
        None
    };

    let results: Vec<_> = stream::iter(targets.into_iter().map(|(name, slug, branch)| {
        let provider = provider.clone();
        async move {
            eprintln!("refreshing {slug}...");
            let meta = provider.repo_meta(&slug).await.ok();
            let p    = provider.list_pull_requests(&slug).await.ok();
            let i    = provider.list_issues(&slug).await.ok();
            let ci   = provider.latest_ci_status(&slug, &branch).await.ok().flatten();
            (name, slug, meta, p, i, ci)
        }
    })).buffer_unordered(6).collect().await;

    let mut cache = crate::cache::load();
    let mut fetched_meta = 0usize;
    let mut fetched_prs  = 0usize;
    let mut fetched_iss  = 0usize;
    for (name, slug, meta, p, i, ci) in results {
        if let Some(meta) = meta {
            let slug_lc = slug.to_lowercase();
            let dep = dependabot
                .get(slug_lc.split('/').next().unwrap_or(""))
                .and_then(|per_org| per_org.as_ref()
                    .map(|m| m.get(&slug_lc).copied().unwrap_or(0)));
            let deploy_status = deploy_heads.as_ref().and_then(|heads| {
                let path = &repos.iter().find(|r| r.name == name)?.path;
                crate::deploy::drift_for(path, &cfg.workspace_root, heads)
            }).map(|s| s.as_str());
            cache.set_meta(&name, crate::cache::RepoMeta {
                stars:           Some(meta.stars),
                open_issues:     Some(meta.open_issues),
                open_prs:        Some(meta.open_prs),
                visibility:      None,
                ci_status:       ci.map(|c| c.as_str().to_string()),
                dependabot_open: dep,
                deploy_status,
                fetched_at:      chrono::Utc::now().timestamp(),
            });
            fetched_meta += 1;
        }
        if let Some(p) = p { fetched_prs += p.len(); cache.set_prs(&name, p); }
        if let Some(i) = i { fetched_iss += i.len(); cache.set_issues(&name, i); }
    }
    let _ = crate::cache::save(&cache);

    // Tier-B local measurement (repo size trend) — no network, cheap, only
    // ever recorded during a refresh (never on a plain read like `vulp status`).
    let mut stats_cache = crate::stats::cache::load();
    for r in &repos {
        let size = crate::stats::size::repo_size(&r.path);
        stats_cache.record_size(&r.name, size.git_kb, size.tree_kb);
    }
    let _ = crate::stats::cache::save(&stats_cache);

    if json {
        #[derive(serde::Serialize)]
        struct Out { repos_refreshed: usize, open_prs: usize, open_issues: usize }
        out_json(&Out { repos_refreshed: fetched_meta, open_prs: fetched_prs, open_issues: fetched_iss });
    } else {
        println!("refreshed {fetched_meta} repos: {fetched_prs} PRs, {fetched_iss} issues");
    }
    Ok(())
}

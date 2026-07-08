//! Local-workspace CLI commands: list/status/scan, sync, stats, activity —
//! everything that works off the on-disk repos without a provider API.

use super::{die, find_repo, log_activity, out_json, scan_repos};
use crate::config::AppConfig;
use anyhow::Result;

#[allow(clippy::too_many_arguments)]
pub fn list(
    cfg: &AppConfig, json: bool,
    dirty: bool, behind: bool, ahead: bool,
    lang: Option<String>, project: Option<String>, alerts: bool, stale_deploy: bool,
    no_external: bool, external: bool,
) -> Result<()> {
    let mut repos = scan_repos(cfg);

    if dirty  { repos.retain(|r| r.is_dirty); }
    if behind { repos.retain(|r| r.behind > 0); }
    if ahead  { repos.retain(|r| r.ahead > 0); }
    if alerts { repos.retain(|r| r.dependabot_open.unwrap_or(0) > 0); }
    if no_external || external {
        let ext = crate::stats::cache::load().external_repo_names();
        if external    { repos.retain(|r|  ext.contains(&r.name)); }
        if no_external { repos.retain(|r| !ext.contains(&r.name)); }
    }
    if stale_deploy {
        repos.retain(|r| {
            !matches!(
                r.deploy_status.as_deref().and_then(crate::deploy::DeployState::parse),
                None | Some(crate::deploy::DeployState::InSync)
            )
        });
    }
    if let Some(l) = lang {
        let l = l.to_lowercase();
        repos.retain(|r| r.lang.as_deref().map(|x| x.to_lowercase().contains(&l)).unwrap_or(false));
    }
    if let Some(p) = &project {
        let projects = crate::projects::load();
        let Some(proj) = projects.iter().find(|x| x.name.eq_ignore_ascii_case(p)) else {
            die(4, &format!("project '{p}' not found"));
        };
        let members: Vec<std::path::PathBuf> =
            proj.repos.iter().map(|rel| cfg.workspace_root.join(rel)).collect();
        repos.retain(|r| members.contains(&r.path));
    }

    if json {
        out_json(&repos);
        return Ok(());
    }
    if repos.is_empty() {
        println!("no repos match");
        return Ok(());
    }
    let name_w = repos.iter().map(|r| r.name.chars().count()).max().unwrap_or(4).max(4);
    for r in &repos {
        let mut flags = String::new();
        if r.is_dirty   { flags.push('⚠'); }
        if r.behind > 0 { flags.push('↓'); }
        if r.ahead > 0  { flags.push('↑'); }
        if stale_deploy {
            let state = r.deploy_status.as_deref().and_then(crate::deploy::DeployState::parse);
            let label = match state {
                Some(crate::deploy::DeployState::Behind(n)) => format!("{n} behind"),
                Some(crate::deploy::DeployState::Diverged)  => "diverged".into(),
                _ => "-".into(),
            };
            println!("{:<name_w$}  {:<12}  {:<10}  {}", r.name, r.branch, label, r.lang.as_deref().unwrap_or("-"));
        } else if alerts {
            println!(
                "{:<name_w$}  {:<12}  {:>3} alerts  {}",
                r.name, r.branch, r.dependabot_open.unwrap_or(0),
                r.lang.as_deref().unwrap_or("-"),
            );
        } else {
            println!(
                "{:<name_w$}  {:<12}  {:<3}  {}",
                r.name, r.branch, flags,
                r.lang.as_deref().unwrap_or("-"),
            );
        }
    }
    Ok(())
}

pub fn status(cfg: &AppConfig, json: bool, repo: &str) -> Result<()> {
    let r = find_repo(cfg, repo);
    if json {
        out_json(&r);
        return Ok(());
    }
    println!("name:        {}", r.name);
    println!("path:        {}", r.path.display());
    println!("branch:      {}", r.branch);
    println!("dirty:       {}", r.is_dirty);
    println!("ahead:       {}", r.ahead);
    println!("behind:      {}", r.behind);
    println!("owner:       {}", r.owner.as_deref().unwrap_or("-"));
    println!("language:    {}", r.lang.as_deref().unwrap_or("-"));
    println!("license:     {}", if r.has_license { "yes" } else { "no" });
    println!("last commit: {} ({})", r.last_commit_msg, r.last_commit_age);
    if let Some(tag) = &r.last_tag { println!("last tag:    {tag}"); }
    Ok(())
}

pub fn scan_cmd(cfg: &AppConfig, json: bool) -> Result<()> {
    let repos = scan_repos(cfg);
    let dirty  = repos.iter().filter(|r| r.is_dirty).count();
    let behind = repos.iter().filter(|r| r.behind > 0).count();
    let ahead  = repos.iter().filter(|r| r.ahead > 0).count();
    if json {
        #[derive(serde::Serialize)]
        struct Summary { total: usize, dirty: usize, behind: usize, ahead: usize }
        out_json(&Summary { total: repos.len(), dirty, behind, ahead });
    } else {
        println!("{} repos  ·  {} dirty  ·  {} behind  ·  {} ahead", repos.len(), dirty, behind, ahead);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn sync(
    cfg: &AppConfig, guard: super::Guard,
    repos: Vec<String>, all: bool, project: Option<String>,
    action: &str, message: Option<String>, allow_secrets: bool,
) -> Result<()> {
    use crate::activity::ActivityKind;
    use crate::util::git::{git_run, git_run_ok};

    let scanned = scan_repos(cfg);
    let targets: Vec<&crate::status::RepoSummary> = if all {
        scanned.iter().collect()
    } else if let Some(p) = &project {
        let projects = crate::projects::load();
        let Some(proj) = projects.iter().find(|x| x.name.eq_ignore_ascii_case(p)) else {
            die(4, &format!("project '{p}' not found"));
        };
        let members: Vec<std::path::PathBuf> =
            proj.repos.iter().map(|rel| cfg.workspace_root.join(rel)).collect();
        scanned.iter().filter(|r| members.contains(&r.path)).collect()
    } else if !repos.is_empty() {
        repos.iter().map(|name| {
            let needle = name.to_lowercase();
            scanned.iter()
                .find(|r| r.name.to_lowercase() == needle
                    || r.path.file_name().map(|b| b.to_string_lossy().to_lowercase() == needle).unwrap_or(false))
                .unwrap_or_else(|| die(4, &format!("repo '{name}' not found")))
        }).collect()
    } else {
        die(2, "specify repo names, --all, or --project");
    };

    let action = action.replace('-', "_"); // commit-push → commit_push
    if !["pull", "push", "commit_push"].contains(&action.as_str()) {
        die(2, &format!("unknown action '{action}' (pull | push | commit-push)"));
    }
    if action == "commit_push" && message.is_none() {
        die(2, "--message is required for commit-push");
    }
    let msg = message.unwrap_or_default();

    super::guard_protected(cfg, guard.force, &targets);
    if guard.dry_run {
        for r in &targets {
            match action.as_str() {
                "commit_push" => println!("DRY RUN: would commit+push {} (message: {msg})", r.name),
                a             => println!("DRY RUN: would {a} {}", r.name),
            }
        }
        return Ok(());
    }

    let mut failed = 0usize;
    for r in &targets {
        eprintln!("── {} [{}]", r.name, action);
        let ok = match action.as_str() {
            "pull" => {
                let ok = git_run_ok(&r.path, &["pull"]);
                if ok { log_activity(ActivityKind::Pull, &r.name, "vulp sync (pull)".into()); }
                ok
            }
            "commit_push" => {
                git_run(&r.path, &["add", "-A"]);
                let hits = if allow_secrets { vec![] } else { crate::secrets::scan_staged(&r.path) };
                if !hits.is_empty() {
                    eprintln!("   SECRETS in staged changes — commit blocked:");
                    for h in &hits { eprintln!("     {} — {}", h.file, h.pattern); }
                    eprintln!("   changes unstaged; re-run with --allow-secrets to override");
                    git_run_ok(&r.path, &["reset", "-q"]);
                    false
                } else {
                    git_run(&r.path, &["commit", "-m", &msg]);
                    let ok = git_run_ok(&r.path, &["push"]);
                    if ok { log_activity(ActivityKind::Push, &r.name, format!("vulp sync (commit+push: {msg})")); }
                    ok
                }
            }
            "push" => {
                let ok = git_run_ok(&r.path, &["push"]);
                if ok { log_activity(ActivityKind::Push, &r.name, "vulp sync (push)".into()); }
                ok
            }
            other => die(2, &format!("unknown action '{other}' (pull | push | commit-push)")),
        };
        if !ok { eprintln!("   FAILED"); failed += 1; }
    }
    println!("synced {}/{} repos", targets.len() - failed, targets.len());
    if failed > 0 { std::process::exit(1); }
    Ok(())
}

/// `vulp brief` — one-shot workspace or per-repo picture for scripts and AI
/// agents: local git state plus todos and the cached PR/issue lists (run
/// `vulp refresh` first when freshness matters). Read-only.
pub fn brief(cfg: &AppConfig, json: bool, repo: Option<String>) -> Result<()> {
    use crate::todos::TodoStatus;

    let cache = crate::cache::load();

    if let Some(name) = repo {
        let r = find_repo(cfg, &name);
        let basename = r.path.file_name().map(|b| b.to_string_lossy().to_string())
            .unwrap_or_else(|| r.name.clone());
        let matches_repo = |candidate: &str| {
            candidate.eq_ignore_ascii_case(&r.name) || candidate.eq_ignore_ascii_case(&basename)
        };

        let commits: Vec<(String, String)> = crate::status::repo_recent_commits(&r.path, 10);
        let todos: Vec<crate::todos::Todo> = crate::todos::load().into_iter()
            .filter(|t| t.status == TodoStatus::Open || t.status == TodoStatus::InProgress)
            .filter(|t| t.repo.as_deref().map(matches_repo).unwrap_or(false))
            .collect();
        let prs: Vec<crate::providers::PrDetail> = cache.all_prs().into_iter()
            .filter(|p| matches_repo(&p.repo)).cloned().collect();
        let issues: Vec<crate::providers::IssueDetail> = cache.all_issues().into_iter()
            .filter(|i| matches_repo(&i.repo)).cloned().collect();

        if json {
            #[derive(serde::Serialize)]
            struct Commit { age: String, message: String }
            #[derive(serde::Serialize)]
            struct Out {
                repo:           crate::status::RepoSummary,
                recent_commits: Vec<Commit>,
                todos:          Vec<crate::todos::Todo>,
                open_prs:       Vec<crate::providers::PrDetail>,
                open_issues:    Vec<crate::providers::IssueDetail>,
            }
            out_json(&Out {
                repo: r,
                recent_commits: commits.into_iter().map(|(age, message)| Commit { age, message }).collect(),
                todos, open_prs: prs, open_issues: issues,
            });
        } else {
            println!("{} · {} · {}{}{}", r.name, r.branch,
                if r.is_dirty { "dirty " } else { "clean " },
                if r.ahead  > 0 { format!("↑{} ", r.ahead)  } else { String::new() },
                if r.behind > 0 { format!("↓{} ", r.behind) } else { String::new() });
            println!("\nrecent commits:");
            for (age, msg) in &commits { println!("  {:<9} {}", age, msg); }
            if !todos.is_empty() {
                println!("\ntodos:");
                for t in &todos { println!("  #{:<4} [{}] {}", t.id, t.status.label(), t.title); }
            }
            if !prs.is_empty() {
                println!("\nopen PRs (cached):");
                for p in &prs { println!("  #{:<5} {} ({} → {})", p.number, p.title, p.head_branch, p.base_branch); }
            }
            if !issues.is_empty() {
                println!("\nopen issues (cached):");
                for i in &issues { println!("  #{:<5} {}", i.number, i.title); }
            }
        }
        return Ok(());
    }

    // ── workspace-wide brief ──────────────────────────────────────────────
    let repos = scan_repos(cfg);
    let dirty  = repos.iter().filter(|r| r.is_dirty).count();
    let behind = repos.iter().filter(|r| r.behind > 0).count();
    let ahead  = repos.iter().filter(|r| r.ahead > 0).count();
    let unlicensed = repos.iter().filter(|r| !r.has_license).count();
    let needs_attention: Vec<&crate::status::RepoSummary> = repos.iter()
        .filter(|r| r.is_dirty || r.ahead > 0 || r.behind > 0)
        .collect();
    let todos: Vec<crate::todos::Todo> = crate::todos::load().into_iter()
        .filter(|t| t.status == TodoStatus::Open || t.status == TodoStatus::InProgress)
        .collect();
    let prs: Vec<&crate::providers::PrDetail> = cache.all_prs();
    let issues: Vec<&crate::providers::IssueDetail> = cache.all_issues();

    if json {
        #[derive(serde::Serialize)]
        struct Summary { total: usize, dirty: usize, ahead: usize, behind: usize, missing_license: usize }
        #[derive(serde::Serialize)]
        struct Out<'a> {
            summary:         Summary,
            needs_attention: Vec<&'a crate::status::RepoSummary>,
            todos:           &'a [crate::todos::Todo],
            open_prs:        &'a [&'a crate::providers::PrDetail],
            open_issues:     &'a [&'a crate::providers::IssueDetail],
        }
        out_json(&Out {
            summary: Summary { total: repos.len(), dirty, ahead, behind, missing_license: unlicensed },
            needs_attention, todos: &todos, open_prs: &prs, open_issues: &issues,
        });
    } else {
        println!("{} repos · {} dirty · {} ahead · {} behind · {} without license",
                 repos.len(), dirty, ahead, behind, unlicensed);
        if !needs_attention.is_empty() {
            println!("\nneeds attention:");
            for r in &needs_attention {
                let mut flags = String::new();
                if r.is_dirty   { flags.push('⚠'); }
                if r.ahead > 0  { flags.push('↑'); }
                if r.behind > 0 { flags.push('↓'); }
                println!("  {:<28} {:<12} {}", r.name, r.branch, flags);
            }
        }
        println!("\ntodos: {} open/in-progress · PRs (cached): {} · issues (cached): {}",
                 todos.len(), prs.len(), issues.len());
    }
    Ok(())
}

pub fn stats(cfg: &AppConfig, json: bool, period: &str) -> Result<()> {
    use crate::detail::StatsPeriod;
    let p = match period {
        "1d" => StatsPeriod::Day1,
        "7d" => StatsPeriod::Day7,
        "30d" => StatsPeriod::Day30,
        "90d" => StatsPeriod::Day90,
        "1y" => StatsPeriod::Year1,
        "all" => StatsPeriod::AllTime,
        other => die(2, &format!("unknown period '{other}' (1d|7d|30d|90d|1y|all)")),
    };
    let repos = scan_repos(cfg);
    let mut per_repo: Vec<(String, usize)> = Vec::new();
    let mut total = 0usize;
    for r in &repos {
        let activity = match p {
            StatsPeriod::Day1    => crate::detail::activity_1d(&r.path),
            StatsPeriod::Day7    => crate::detail::activity_7d(&r.path),
            StatsPeriod::Day30   => crate::detail::activity_30d(&r.path),
            StatsPeriod::Day90   => crate::detail::activity_90d(&r.path),
            StatsPeriod::Year1   => crate::detail::activity_1y(&r.path),
            StatsPeriod::AllTime => crate::detail::activity_all(&r.path),
        };
        let n: usize = activity.iter().map(|(_, c)| c).sum();
        total += n;
        if n > 0 { per_repo.push((r.name.clone(), n)); }
    }
    per_repo.sort_by(|a, b| b.1.cmp(&a.1));

    if json {
        #[derive(serde::Serialize)]
        struct Stats<'a> {
            period: &'a str,
            total_commits: usize,
            repos_active: usize,
            per_repo: &'a [(String, usize)],
        }
        out_json(&Stats { period, total_commits: total, repos_active: per_repo.len(), per_repo: &per_repo });
    } else {
        println!("period: {period}   commits: {total}   active repos: {}", per_repo.len());
        for (name, n) in per_repo.iter().take(15) {
            println!("  {:>5}  {}", n, name);
        }
    }
    Ok(())
}

pub fn activity(json: bool, limit: usize, kind: Option<String>) -> Result<()> {
    let mut entries = crate::activity::load_logged();
    entries.sort_by(|a, b| b.ts.cmp(&a.ts));
    if let Some(k) = &kind {
        entries.retain(|e| e.kind.label().eq_ignore_ascii_case(k));
    }
    entries.truncate(limit);

    if json {
        out_json(&entries);
    } else if entries.is_empty() {
        println!("no matching activity");
    } else {
        for e in &entries {
            println!("{}  {:<8}  {:<24}  {}", crate::status::format_age_pub(e.ts), e.kind.label(), e.repo, e.detail);
        }
    }
    Ok(())
}

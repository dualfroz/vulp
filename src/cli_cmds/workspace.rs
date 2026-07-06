//! Local-workspace CLI commands: list/status/scan, sync, stats, activity —
//! everything that works off the on-disk repos without a provider API.

use super::{die, find_repo, log_activity, out_json, scan_repos};
use crate::config::AppConfig;
use anyhow::Result;

#[allow(clippy::too_many_arguments)]
pub fn list(
    cfg: &AppConfig, json: bool,
    dirty: bool, behind: bool, ahead: bool,
    lang: Option<String>, project: Option<String>,
) -> Result<()> {
    let mut repos = scan_repos(cfg);

    if dirty  { repos.retain(|r| r.is_dirty); }
    if behind { repos.retain(|r| r.behind > 0); }
    if ahead  { repos.retain(|r| r.ahead > 0); }
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
        println!(
            "{:<name_w$}  {:<12}  {:<3}  {}",
            r.name, r.branch, flags,
            r.lang.as_deref().unwrap_or("-"),
        );
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

pub fn sync(
    cfg: &AppConfig, guard: super::Guard,
    repos: Vec<String>, all: bool, project: Option<String>,
    action: &str, message: Option<String>,
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
                git_run(&r.path, &["commit", "-m", &msg]);
                let ok = git_run_ok(&r.path, &["push"]);
                if ok { log_activity(ActivityKind::Push, &r.name, format!("vulp sync (commit+push: {msg})")); }
                ok
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

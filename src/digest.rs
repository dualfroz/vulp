use crate::providers::Provider;
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};

pub const WEBHOOK_ID: &str = "discord_webhook_default";

pub struct WeeklyDigest {
    pub iso_week:          (i32, u32),
    pub range:              (NaiveDate, NaiveDate),
    pub own_commits:        usize,
    pub prev_week_commits:  usize,
    pub active_days:        u8,
    pub streak:             u32,
    pub top_repos:          Vec<(String, usize)>,
    pub releases:           Vec<(String, String)>,
    pub prs_opened:         u64,
    pub prs_merged:         u64,
    pub issues_opened:      u64,
    pub issues_closed:      u64,
    pub todos_done:         usize,
    pub todos_added:        usize,
}

pub async fn build(
    repos: &[crate::status::RepoSummary],
    matcher: &crate::stats::OwnEmailMatcher,
    provider: Option<&dyn Provider>,
    search_scope: &str,
) -> WeeklyDigest {
    let today = Local::now().date_naive();
    let this_monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let this_sunday = this_monday + chrono::Duration::days(6);
    let prev_monday = this_monday - chrono::Duration::days(7);
    let prev_sunday = this_monday - chrono::Duration::days(1);

    let all_commits  = crate::stats::scan_workspace_own_commits(repos, matcher);
    let week_commits = crate::stats::commits_in_range(&all_commits, this_monday, this_sunday);
    let prev_commits = crate::stats::commits_in_range(&all_commits, prev_monday, prev_sunday);
    let (streak_cur, _) = crate::stats::streak(&all_commits);
    let active_days = week_commits.iter().map(|c| c.date)
        .collect::<std::collections::HashSet<_>>().len() as u8;

    let top_repos = crate::stats::top_repos_in_range(repos, matcher, this_monday, this_sunday)
        .into_iter().take(3).collect();
    let releases = local_releases_in_range(repos, this_monday, this_sunday);

    let (prs_opened, prs_merged, issues_opened, issues_closed) = match provider {
        Some(p) if !search_scope.is_empty() => {
            let range_q = format!("{}..{}", this_monday, this_sunday);
            let po = p.count_search(&format!("is:pr created:{range_q} {search_scope}")).await.unwrap_or(0);
            let pm = p.count_search(&format!("is:pr is:merged merged:{range_q} {search_scope}")).await.unwrap_or(0);
            let io = p.count_search(&format!("is:issue created:{range_q} {search_scope}")).await.unwrap_or(0);
            let ic = p.count_search(&format!("is:issue closed:{range_q} {search_scope}")).await.unwrap_or(0);
            (po, pm, io, ic)
        }
        _ => (0, 0, 0, 0),
    };

    let todos = crate::todos::load();
    let todos_added = todos.iter().filter(|t| ts_in_range(t.created_at, this_monday, this_sunday)).count();
    let todos_done  = todos.iter()
        .filter(|t| t.status == crate::todos::TodoStatus::Done && ts_in_range(t.updated_at, this_monday, this_sunday))
        .count();

    WeeklyDigest {
        iso_week: (today.iso_week().year(), today.iso_week().week()),
        range: (this_monday, this_sunday),
        own_commits: week_commits.len(),
        prev_week_commits: prev_commits.len(),
        active_days,
        streak: streak_cur,
        top_repos, releases,
        prs_opened, prs_merged, issues_opened, issues_closed,
        todos_done, todos_added,
    }
}

fn ts_in_range(ts: i64, from: NaiveDate, to: NaiveDate) -> bool {
    let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) else { return false };
    let date = dt.with_timezone(&Local).date_naive();
    date >= from && date <= to
}

fn local_releases_in_range(
    repos: &[crate::status::RepoSummary], from: NaiveDate, to: NaiveDate,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in repos {
        let output = std::process::Command::new("git")
            .arg("-C").arg(&r.path)
            .args(["for-each-ref", "refs/tags", "--format=%(creatordate:unix) %(refname:short)"])
            .output();
        let Ok(output) = output else { continue };
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Some((ts_s, tag)) = line.split_once(' ') else { continue };
            let Ok(ts) = ts_s.parse::<i64>() else { continue };
            if ts_in_range(ts, from, to) {
                out.push((r.name.clone(), tag.to_string()));
            }
        }
    }
    out
}

fn delta_label(delta: i64) -> String {
    match delta.cmp(&0) {
        std::cmp::Ordering::Greater => format!("▲ +{delta}"),
        std::cmp::Ordering::Less    => format!("▼ {delta}"),
        std::cmp::Ordering::Equal   => "=".into(),
    }
}

pub fn render_plaintext(d: &WeeklyDigest) -> String {
    let delta = delta_label(d.own_commits as i64 - d.prev_week_commits as i64);
    let mut out = String::new();
    out.push_str(&format!(
        "Weekly digest {}-W{:02} · {} – {}\n",
        d.iso_week.0, d.iso_week.1, d.range.0.format("%d.%m"), d.range.1.format("%d.%m"),
    ));
    out.push_str(&format!(
        "{} commits ({delta} vs last week) · {} active days · streak {} days\n",
        d.own_commits, d.active_days, d.streak,
    ));
    if !d.top_repos.is_empty() {
        let s: Vec<String> = d.top_repos.iter().map(|(n, c)| format!("{n} {c}")).collect();
        out.push_str(&format!("Top repos: {}\n", s.join(" · ")));
    }
    if !d.releases.is_empty() {
        let s: Vec<String> = d.releases.iter().map(|(r, t)| format!("{r} {t}")).collect();
        out.push_str(&format!("Releases: {}\n", s.join(", ")));
    }
    out.push_str(&format!(
        "PRs: {} opened, {} merged · Issues: {} opened, {} closed\n",
        d.prs_opened, d.prs_merged, d.issues_opened, d.issues_closed,
    ));
    out.push_str(&format!("Todos: {} done, {} added\n", d.todos_done, d.todos_added));
    out
}

pub fn render_markdown(d: &WeeklyDigest) -> String {
    let delta = delta_label(d.own_commits as i64 - d.prev_week_commits as i64);
    let mut out = String::new();
    out.push_str(&format!("# Weekly digest {}-W{:02} · {} – {}\n\n", d.iso_week.0, d.iso_week.1, d.range.0, d.range.1));
    out.push_str(&format!(
        "**{} commits** ({delta} vs last week) · {} active days · streak **{} days**\n\n",
        d.own_commits, d.active_days, d.streak,
    ));
    if !d.top_repos.is_empty() {
        out.push_str("## Top repos\n\n");
        for (n, c) in &d.top_repos { out.push_str(&format!("- `{n}` — {c}\n")); }
        out.push('\n');
    }
    if !d.releases.is_empty() {
        out.push_str("## Releases\n\n");
        for (r, t) in &d.releases { out.push_str(&format!("- `{r}` **{t}**\n")); }
        out.push('\n');
    }
    out.push_str("## PRs / Issues\n\n");
    out.push_str(&format!(
        "PRs: {} opened, {} merged  \nIssues: {} opened, {} closed\n\n",
        d.prs_opened, d.prs_merged, d.issues_opened, d.issues_closed,
    ));
    out.push_str("## Todos\n\n");
    out.push_str(&format!("✔ {} done · ＋{} added\n", d.todos_done, d.todos_added));
    out
}

pub fn render_discord_embed(d: &WeeklyDigest) -> serde_json::Value {
    let delta = delta_label(d.own_commits as i64 - d.prev_week_commits as i64);
    let title = format!(
        "Weekly digest {}-W{:02} · {} – {}",
        d.iso_week.0, d.iso_week.1, d.range.0.format("%d.%m"), d.range.1.format("%d.%m"),
    );
    let description = format!(
        "**{} commits** ({delta} vs last week) · {} active days · streak **{} days**",
        d.own_commits, d.active_days, d.streak,
    );

    let mut fields = Vec::new();
    if !d.top_repos.is_empty() {
        let val = d.top_repos.iter().map(|(n, c)| format!("`{n}` {c}")).collect::<Vec<_>>().join(" · ");
        fields.push(serde_json::json!({ "name": "Top repos", "value": val, "inline": false }));
    }
    if !d.releases.is_empty() {
        let val = d.releases.iter().map(|(r, t)| format!("`{r}` **{t}**")).collect::<Vec<_>>().join("\n");
        fields.push(serde_json::json!({ "name": "Releases", "value": val, "inline": true }));
    }
    fields.push(serde_json::json!({
        "name": "PRs / Issues", "inline": false,
        "value": format!("PRs: {} opened, {} merged · Issues: {} opened, {} closed",
            d.prs_opened, d.prs_merged, d.issues_opened, d.issues_closed),
    }));
    fields.push(serde_json::json!({
        "name": "Todos", "inline": true,
        "value": format!("✔ {} done · ＋{} added", d.todos_done, d.todos_added),
    }));

    serde_json::json!({
        "username": "vulp",
        "embeds": [{
            "title": title,
            "color": 15105570,
            "description": description,
            "fields": fields,
            "footer": { "text": format!("vulp digest · sent {}", Local::now().format("%a %H:%M")) },
        }],
    })
}

pub fn archive_path(d: &WeeklyDigest) -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_default()
        .join(".vulp").join("digests")
        .join(format!("{}-W{:02}.md", d.iso_week.0, d.iso_week.1))
}

pub fn archive(d: &WeeklyDigest) -> Result<std::path::PathBuf> {
    let path = archive_path(d);
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(&path, render_markdown(d))?;
    Ok(path)
}

pub async fn send_discord(embed: &serde_json::Value, webhook_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let resp = client.post(format!("{webhook_url}?wait=true")).json(embed).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Discord webhook POST failed: {status} — {text}");
    }
    Ok(())
}

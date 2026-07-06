//! Author-filtered activity analytics: the existing `GlobalStats` (Overview/
//! old Stats tab) counts *every* commit on HEAD, which for a workspace with
//! big forks (95%+ of this workspace's commits are upstream history) makes
//! "your activity" fictional. This module scans only commits whose author
//! email is actually yours — streak, 52-week heatmap, hour/day histograms —
//! sharing one git invocation per repo so adding a metric never costs a
//! second scan.

pub mod cache;
pub mod cadence;
pub mod size;

use crate::config::AppConfig;
use chrono::{Datelike, Local, NaiveDate};
use std::collections::{HashMap, HashSet};

/// One of your own commits: the local calendar date and hour it was made.
#[derive(Debug, Clone, Copy)]
pub struct OwnCommit {
    pub date: NaiveDate,
    // kept: hour_pattern (Sprint D Stats-tab redesign)
    #[allow(dead_code)]
    pub hour: u32,
}

/// Resolves which author identities count as "you": configured
/// `stats_author_emails`, falling back to `git config user.email`, plus
/// every configured account's GitHub no-reply address (`{id+}username@
/// users.noreply.github.com`) — the most common gap, since squash-merges
/// done from the GitHub web UI carry that address instead of your real one.
pub struct OwnEmailMatcher {
    emails:    HashSet<String>,
    usernames: Vec<String>,
}

impl OwnEmailMatcher {
    pub fn resolve(cfg: &AppConfig, accounts: &[crate::auth::Account]) -> Self {
        let mut emails: HashSet<String> = cfg.stats_author_emails.iter()
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty())
            .collect();
        if emails.is_empty() {
            if let Ok(out) = std::process::Command::new("git")
                .args(["config", "--get", "user.email"]).output()
            {
                let e = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
                if !e.is_empty() { emails.insert(e); }
            }
        }
        if emails.is_empty() && !cfg.email.trim().is_empty() {
            emails.insert(cfg.email.trim().to_lowercase());
        }
        let usernames: Vec<String> = accounts.iter()
            .map(|a| a.username.trim().to_lowercase())
            .filter(|u| !u.is_empty())
            .collect();
        Self { emails, usernames }
    }

    /// Whether `email` belongs to you.
    pub fn is_own(&self, email: &str) -> bool {
        let email = email.trim().to_lowercase();
        if self.emails.contains(&email) { return true; }
        let Some(at) = email.rfind('@') else { return false };
        if &email[at..] != "@users.noreply.github.com" { return false; }
        let local = &email[..at];
        let uname = local.rsplit('+').next().unwrap_or(local);
        self.usernames.iter().any(|u| u == uname)
    }

    /// A combined `--author` extended-regex matching every known identity —
    /// lets `git log` do the filtering instead of materializing every
    /// commit (some forks in this workspace have thousands) just to throw
    /// most of them away in Rust. `None` when nothing is configured at all.
    fn author_regex(&self) -> Option<String> {
        let mut alts: Vec<String> = self.emails.iter()
            .map(|e| format!("<{}>", escape_ere(e)))
            .collect();
        alts.extend(self.usernames.iter().map(|u| {
            format!(r"<[0-9]*\+?{}@users\.noreply\.github\.com>", escape_ere(u))
        }));
        if alts.is_empty() { None } else { Some(alts.join("|")) }
    }
}

fn escape_ere(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.^$|()[]{}*+?".contains(c) { out.push('\\'); }
        out.push(c);
    }
    out
}

/// Scans one repo's local branches for your own commits. Filters at the git
/// level via `--author` (cheap even on huge histories), then re-checks each
/// result against `matcher.is_own` as a safety net against regex quirks.
pub fn scan_repo_own_commits(path: &std::path::Path, matcher: &OwnEmailMatcher) -> Vec<OwnCommit> {
    let Some(regex) = matcher.author_regex() else { return vec![] };
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["log", "--branches", "-i", &format!("--author={regex}")])
        .args(["--date=format-local:%Y-%m-%dT%H", "--pretty=format:%ae\x1f%ad"])
        .output();
    let Ok(out) = out else { return vec![] };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|line| {
        let (email, ad) = line.split_once('\x1f')?;
        if !matcher.is_own(email) { return None; }
        let (date_s, hour_s) = ad.split_once('T')?;
        let date = NaiveDate::parse_from_str(date_s, "%Y-%m-%d").ok()?;
        let hour: u32 = hour_s.parse().ok()?;
        Some(OwnCommit { date, hour })
    }).collect()
}

/// Scans every repo and merges the result into one date-sorted list.
pub fn scan_workspace_own_commits(
    repos: &[crate::status::RepoSummary], matcher: &OwnEmailMatcher,
) -> Vec<OwnCommit> {
    let mut all: Vec<OwnCommit> = repos.iter()
        .flat_map(|r| scan_repo_own_commits(&r.path, matcher))
        .collect();
    all.sort_by_key(|c| c.date);
    all
}

/// Current and longest streaks of consecutive local-calendar days with at
/// least one commit. Grace rule: an empty *today* doesn't break the current
/// streak until local midnight passes (checking at 8am shouldn't show a
/// streak of yesterday's length as already broken).
pub fn streak(commits: &[OwnCommit]) -> (u32, u32) {
    let days: std::collections::BTreeSet<NaiveDate> = commits.iter().map(|c| c.date).collect();
    if days.is_empty() { return (0, 0); }

    let day_list: Vec<NaiveDate> = days.iter().copied().collect();
    let mut longest = 1u32;
    let mut run = 1u32;
    for w in day_list.windows(2) {
        if w[1] - w[0] == chrono::Duration::days(1) {
            run += 1;
        } else {
            longest = longest.max(run);
            run = 1;
        }
    }
    longest = longest.max(run);

    let today = Local::now().date_naive();
    let mut cursor = today;
    if !days.contains(&cursor) { cursor -= chrono::Duration::days(1); }
    let mut current = 0u32;
    while days.contains(&cursor) {
        current += 1;
        cursor -= chrono::Duration::days(1);
    }
    (current, longest)
}

/// 364 days ending today, Monday-aligned, as `(date, count)` — a fixed 52×7
/// grid a heatmap can render directly (missing days are `0`, not absent).
// kept: Sprint D Stats-tab redesign (heatmap mockup in debata-2026-07-06/02-ux.md)
#[allow(dead_code)]
pub fn heatmap_52w(commits: &[OwnCommit]) -> Vec<(NaiveDate, u32)> {
    let mut counts: HashMap<NaiveDate, u32> = HashMap::new();
    for c in commits { *counts.entry(c.date).or_default() += 1; }

    let today = Local::now().date_naive();
    let rough_start = today - chrono::Duration::days(363);
    let start = rough_start - chrono::Duration::days(rough_start.weekday().num_days_from_monday() as i64);

    let mut out = Vec::new();
    let mut d = start;
    while d <= today {
        out.push((d, counts.get(&d).copied().unwrap_or(0)));
        d += chrono::Duration::days(1);
    }
    out
}

/// Weekday histogram (Mon=0..Sun=6) over the last `since_days` days.
// kept: Sprint D Stats-tab redesign
#[allow(dead_code)]
pub fn day_pattern(commits: &[OwnCommit], since_days: i64) -> [u32; 7] {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    let mut out = [0u32; 7];
    for c in commits {
        if c.date > cutoff { out[c.date.weekday().num_days_from_monday() as usize] += 1; }
    }
    out
}

/// Local-hour histogram (0..23) over the last `since_days` days.
// kept: Sprint D Stats-tab redesign
#[allow(dead_code)]
pub fn hour_pattern(commits: &[OwnCommit], since_days: i64) -> [u32; 24] {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    let mut out = [0u32; 24];
    for c in commits {
        if c.date > cutoff && (c.hour as usize) < 24 { out[c.hour as usize] += 1; }
    }
    out
}

/// Number of own commits in the last `since_days` days.
// kept: Sprint D Stats-tab redesign (tile: "commits this period")
#[allow(dead_code)]
pub fn commits_since(commits: &[OwnCommit], since_days: i64) -> usize {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    commits.iter().filter(|c| c.date > cutoff).count()
}

/// Number of distinct calendar days with >=1 own commit in the last
/// `since_days` days.
// kept: Sprint D Stats-tab redesign (tile: "active days")
#[allow(dead_code)]
pub fn active_days_since(commits: &[OwnCommit], since_days: i64) -> usize {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    commits.iter().filter(|c| c.date > cutoff)
        .map(|c| c.date)
        .collect::<HashSet<_>>()
        .len()
}

/// Own commits with a date in `[from, to]` inclusive.
pub fn commits_in_range(commits: &[OwnCommit], from: NaiveDate, to: NaiveDate) -> Vec<OwnCommit> {
    commits.iter().copied().filter(|c| c.date >= from && c.date <= to).collect()
}

/// Per-repo own-commit counts within `[from, to]`, sorted descending.
pub fn top_repos_in_range(
    repos: &[crate::status::RepoSummary], matcher: &OwnEmailMatcher,
    from: NaiveDate, to: NaiveDate,
) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = repos.iter().filter_map(|r| {
        let n = scan_repo_own_commits(&r.path, matcher).into_iter()
            .filter(|c| c.date >= from && c.date <= to)
            .count();
        if n > 0 { Some((r.name.clone(), n)) } else { None }
    }).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

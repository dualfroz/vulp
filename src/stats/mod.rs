pub mod cache;
pub mod cadence;
pub mod size;

use crate::config::AppConfig;
use chrono::{Datelike, Local, NaiveDate};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct OwnCommit {
    pub date: NaiveDate,
    #[allow(dead_code)]
    pub hour: u32,
}

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

    pub fn is_own(&self, email: &str) -> bool {
        let email = email.trim().to_lowercase();
        if self.emails.contains(&email) { return true; }
        let Some(at) = email.rfind('@') else { return false };
        if &email[at..] != "@users.noreply.github.com" { return false; }
        let local = &email[..at];
        let uname = local.rsplit('+').next().unwrap_or(local);
        self.usernames.iter().any(|u| u == uname)
    }

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

pub fn scan_repo_own_commits(path: &std::path::Path, matcher: &OwnEmailMatcher) -> Vec<OwnCommit> {
    let Some(regex) = matcher.author_regex() else { return vec![] };
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["log", "--branches", "-E", "-i", &format!("--author={regex}")])
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

pub fn has_own_commit(path: &std::path::Path, matcher: &OwnEmailMatcher) -> Option<bool> {
    let regex = matcher.author_regex()?;
    let out = std::process::Command::new("git")
        .arg("-C").arg(path)
        .args(["log", "--branches", "-E", "-i", &format!("--author={regex}"), "--max-count=1", "--format=%H"])
        .output()
        .ok()?;
    Some(out.stdout.iter().any(|b| !b.is_ascii_whitespace()))
}

pub fn scan_workspace_own_commits(
    repos: &[crate::status::RepoSummary], matcher: &OwnEmailMatcher,
) -> Vec<OwnCommit> {
    let mut all: Vec<OwnCommit> = repos.iter()
        .flat_map(|r| scan_repo_own_commits(&r.path, matcher))
        .collect();
    all.sort_by_key(|c| c.date);
    all
}

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

#[allow(dead_code)]
pub fn day_pattern(commits: &[OwnCommit], since_days: i64) -> [u32; 7] {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    let mut out = [0u32; 7];
    for c in commits {
        if c.date > cutoff { out[c.date.weekday().num_days_from_monday() as usize] += 1; }
    }
    out
}

#[allow(dead_code)]
pub fn hour_pattern(commits: &[OwnCommit], since_days: i64) -> [u32; 24] {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    let mut out = [0u32; 24];
    for c in commits {
        if c.date > cutoff && (c.hour as usize) < 24 { out[c.hour as usize] += 1; }
    }
    out
}

#[allow(dead_code)]
pub fn commits_since(commits: &[OwnCommit], since_days: i64) -> usize {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    commits.iter().filter(|c| c.date > cutoff).count()
}

#[allow(dead_code)]
pub fn active_days_since(commits: &[OwnCommit], since_days: i64) -> usize {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(since_days);
    commits.iter().filter(|c| c.date > cutoff)
        .map(|c| c.date)
        .collect::<HashSet<_>>()
        .len()
}

pub fn commits_in_range(commits: &[OwnCommit], from: NaiveDate, to: NaiveDate) -> Vec<OwnCommit> {
    commits.iter().copied().filter(|c| c.date >= from && c.date <= to).collect()
}

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
    out.sort_by_key(|(_, commits)| std::cmp::Reverse(*commits));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testgit::{commit_at, git, git_with_env, init_repo};
    use chrono::Timelike;

    fn matcher(emails: &[&str], usernames: &[&str]) -> OwnEmailMatcher {
        OwnEmailMatcher {
            emails: emails.iter().map(|e| e.to_string()).collect(),
            usernames: usernames.iter().map(|u| u.to_string()).collect(),
        }
    }

    fn day(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }

    fn commit(date: NaiveDate) -> OwnCommit { OwnCommit { date, hour: 12 } }

    #[test]
    fn own_identity_includes_github_noreply_addresses() {
        let m = matcher(&["me@example.com"], &["octo"]);
        assert!(m.is_own(" ME@example.com "));
        assert!(m.is_own("octo@users.noreply.github.com"));
        assert!(m.is_own("12345+octo@users.noreply.github.com"));
        assert!(!m.is_own("12345+someone@users.noreply.github.com"));
        assert!(!m.is_own("octo@example.com"));
        assert!(!m.is_own("not-an-email"));
    }

    #[test]
    fn author_regex_escapes_metacharacters() {
        assert_eq!(matcher(&[], &[]).author_regex(), None);
        assert_eq!(escape_ere("a.b+c@x(y)"), r"a\.b\+c@x\(y\)");
        let regex = matcher(&["me+tag@example.com"], &["octo"]).author_regex().unwrap();
        assert!(regex.contains(r"<me\+tag@example\.com>"));
        assert!(regex.contains(r"octo@users\.noreply\.github\.com>"));
    }

    #[test]
    fn longest_streak_counts_consecutive_days() {
        assert_eq!(streak(&[]), (0, 0));
        let commits: Vec<OwnCommit> = ["2020-01-01", "2020-01-02", "2020-01-02", "2020-01-03", "2020-02-10"]
            .into_iter().map(|d| commit(day(d))).collect();
        assert_eq!(streak(&commits), (0, 3));
    }

    #[test]
    fn current_streak_survives_an_empty_today() {
        let today = Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let commits = vec![commit(yesterday - chrono::Duration::days(1)), commit(yesterday)];
        assert_eq!(streak(&commits), (2, 2));
        let mut with_today = commits.clone();
        with_today.push(commit(today));
        assert_eq!(streak(&with_today), (3, 3));
        let stale = vec![commit(today - chrono::Duration::days(3))];
        assert_eq!(streak(&stale).0, 0);
    }

    #[test]
    fn range_filter_is_inclusive() {
        let commits: Vec<OwnCommit> = ["2030-03-01", "2030-03-05", "2030-03-08", "2030-03-09"]
            .into_iter().map(|d| commit(day(d))).collect();
        let picked: Vec<NaiveDate> = commits_in_range(&commits, day("2030-03-05"), day("2030-03-08"))
            .into_iter().map(|c| c.date).collect();
        assert_eq!(picked, [day("2030-03-05"), day("2030-03-08")]);
    }

    #[test]
    fn only_own_commits_are_scanned() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(&dir.path().join("fork"));
        commit_at(&repo, "a.txt", "1", "mine", 1_700_000_000);
        std::fs::write(repo.join("b.txt"), "2").unwrap();
        git(&repo, &["add", "-A"]);
        git_with_env(&repo, &["commit", "-q", "-m", "upstream"], &[
            ("GIT_AUTHOR_EMAIL", "upstream@example.org".to_string()),
        ]);

        let mine = matcher(&["test@example.com"], &[]);
        let found = scan_repo_own_commits(&repo, &mine);
        assert_eq!(found.len(), 1);
        let expected = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap().with_timezone(&Local);
        assert_eq!(found[0].date, expected.date_naive());
        assert_eq!(found[0].hour, expected.hour());
        assert_eq!(has_own_commit(&repo, &mine), Some(true));

        let stranger = matcher(&["nobody@example.net"], &[]);
        assert!(scan_repo_own_commits(&repo, &stranger).is_empty());
        assert_eq!(has_own_commit(&repo, &stranger), Some(false));
        assert_eq!(has_own_commit(&repo, &matcher(&[], &[])), None);
    }
}

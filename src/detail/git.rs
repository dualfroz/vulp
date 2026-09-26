use super::{BranchEntry, ContribEntry, HistEntry, TreeNode};
use crate::status::format_age_pub;
use git2::{BranchType, Repository};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub use crate::util::git::{git_run, git_run_ok};
pub use crate::util::url::to_web_url;
pub use crate::util::clipboard::copy_auto as copy_to_clipboard;

pub fn build_history(path: &std::path::Path, limit: usize) -> Vec<HistEntry> {
    let repo = match Repository::open(path) { Ok(r) => r, Err(_) => return vec![] };

    let mut commits: Vec<HistEntry> = {
        let mut rw = match repo.revwalk() { Ok(r) => r, Err(_) => return vec![] };
        let _ = rw.push_head();
        rw.take(limit).filter_map(|id| {
            let id = id.ok()?;
            let c  = repo.find_commit(id).ok()?;
            let sha    = id.to_string()[..7].to_string();
            let ts     = c.time().seconds();
            let age    = format_age_pub(ts);
            let author = c.author().name().unwrap_or("").to_string();
            let msg    = c.summary().unwrap_or("").chars().take(72).collect();
            Some(HistEntry::Commit { sha, age, ts, author, msg })
        }).collect()
    };

    let mut tags: Vec<HistEntry> = vec![];
    if let Ok(tag_names) = repo.tag_names(None) {
        for name in tag_names.iter().flatten() {
            let refname = format!("refs/tags/{}", name);
            if let Ok(obj) = repo.revparse_single(&refname) {
                let (ts, tag_msg) = if let Ok(tag) = obj.clone().into_tag() {
                    let ts  = tag.tagger().map(|s| s.when().seconds()).unwrap_or(0);
                    let msg = tag.message().unwrap_or("").chars().take(72).collect();
                    (ts, msg)
                } else if let Ok(c) = obj.peel_to_commit() {
                    (c.time().seconds(), String::new())
                } else { continue };
                tags.push(HistEntry::Tag {
                    name: name.to_string(),
                    age:  format_age_pub(ts),
                    ts,
                    msg:  tag_msg,
                });
            }
        }
    }

    commits.extend(tags);
    commits.sort_by_key(|c| std::cmp::Reverse(c.ts()));
    commits.truncate(limit + 20);
    commits
}

pub fn total_commits(path: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C").arg(path).args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn count_tags(path: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C").arg(path).args(["tag", "-l"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

pub fn count_stashes(path: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C").arg(path).args(["stash", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0)
}

pub fn list_all_files(path: &std::path::Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path).args(["ls-files"])
        .output()
        .unwrap_or_else(|_| std::process::Output {
            status: std::process::ExitStatus::default(), stdout: vec![], stderr: vec![]
        });
    String::from_utf8_lossy(&out.stdout)
        .lines().map(|l| l.to_string()).collect()
}

pub fn count_lines_of_code(path: &std::path::Path) -> usize {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("git ls-files | head -5000 | xargs wc -l 2>/dev/null | tail -1")
        .current_dir(path)
        .output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0)
        }
        Err(_) => 0,
    }
}

pub fn largest_files(path: &std::path::Path, n: usize) -> Vec<(String, usize)> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("git ls-files | head -5000 | xargs wc -l 2>/dev/null | sort -rn")
        .current_dir(path)
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines()
            .filter_map(|line| {
                let line = line.trim();
                let mut parts = line.splitn(2, ' ');
                let count_str = parts.next()?.trim();
                let file = parts.next()?.trim();
                if file == "total" || file.is_empty() { return None; }
                let count = count_str.parse::<usize>().ok()?;
                Some((file.to_string(), count))
            })
            .take(n)
            .collect(),
        Err(_) => vec![],
    }
}

pub fn remote_url(path: &std::path::Path) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    let url = remote.url()?;
    Some(to_web_url(url))
}

pub fn list_branches(path: &std::path::Path) -> Vec<BranchEntry> {
    let repo = match Repository::open(path) { Ok(r) => r, Err(_) => return vec![] };
    let mut out = vec![];
    let branches = match repo.branches(Some(BranchType::Local)) {
        Ok(b) => b, Err(_) => return vec![],
    };
    for b in branches.flatten() {
        let (branch, _) = b;
        let name = match branch.name() {
            Ok(Some(n)) => n.to_string(),
            _ => continue,
        };
        let is_current = branch.is_head();

        let upstream = branch.upstream().ok()
            .and_then(|u| u.name().ok().flatten().map(|n| n.to_string()));

        let (ahead, behind) = if let Some(ref up) = upstream {
            if let (Ok(local_ref), Ok(up_ref)) = (
                repo.find_branch(&name, BranchType::Local).and_then(|b| b.into_reference().resolve()),
                repo.find_reference(&format!("refs/remotes/{}", up)).and_then(|r| r.resolve()),
            ) {
                if let (Some(local_oid), Some(up_oid)) = (local_ref.target(), up_ref.target()) {
                    repo.graph_ahead_behind(local_oid, up_oid).unwrap_or((0, 0))
                } else { (0, 0) }
            } else { (0, 0) }
        } else { (0, 0) };

        let (last_commit_age, last_commit_msg) = branch.get().peel_to_commit().ok()
            .map(|c| (format_age_pub(c.time().seconds()), c.summary().unwrap_or("").chars().take(60).collect::<String>()))
            .unwrap_or_default();

        out.push(BranchEntry { name, is_current, upstream, ahead, behind, last_commit_age, last_commit_msg });
    }
    out.sort_by(|a, b| b.is_current.cmp(&a.is_current).then(a.name.cmp(&b.name)));
    out
}

pub fn list_contributors(path: &std::path::Path) -> Vec<ContribEntry> {
    let out = std::process::Command::new("git")
        .arg("-C").arg(path).args(["shortlog", "-sn", "--branches"])
        .output()
        .unwrap_or_else(|_| std::process::Output {
            status: std::process::ExitStatus::default(), stdout: vec![], stderr: vec![]
        });
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (count_str, name) = line.split_once('\t')?;
            let count = count_str.trim().parse::<usize>().ok()?;
            Some(ContribEntry { name: name.to_string(), count })
        })
        .collect()
}

fn git_activity_raw(path: &std::path::Path, since_days: Option<u64>, date_fmt: &str) -> HashMap<String, usize> {
    let since_arg = since_days.map(|d| format!("--since={} days ago", d));
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(path).arg("log");
    if let Some(ref s) = since_arg { cmd.arg(s); }
    cmd.args([&format!("--date=format-local:{}", date_fmt), "--pretty=format:%ad"]);
    let out = cmd.output().unwrap_or_else(|_| std::process::Output {
        status: std::process::ExitStatus::default(), stdout: vec![], stderr: vec![]
    });
    let mut counts = HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let label = line.trim().to_string();
        if !label.is_empty() { *counts.entry(label).or_insert(0usize) += 1; }
    }
    counts
}

pub fn activity_1d(path: &std::path::Path) -> Vec<(String, usize)> {
    let raw = git_activity_raw(path, Some(1), "%H");
    (0u32..24).map(|h| {
        let label = format!("{:02}", h);
        let count = raw.get(&label).copied().unwrap_or(0);
        (label, count)
    }).collect()
}

pub fn activity_7d(path: &std::path::Path) -> Vec<(String, usize)> {
    let raw = git_activity_raw(path, Some(7), "%Y-%m-%d");
    let now = chrono::Local::now();
    (0i64..7).rev().map(|d| {
        let dt    = now - chrono::Duration::days(d);
        let label = dt.format("%Y-%m-%d").to_string();
        let short = dt.format("%m-%d").to_string();
        let count = raw.get(&label).copied().unwrap_or(0);
        (short, count)
    }).collect()
}

pub fn activity_30d(path: &std::path::Path) -> Vec<(String, usize)> {
    let raw = git_activity_raw(path, Some(30), "%Y-%m-%d");
    let now = chrono::Local::now();
    (0i64..30).rev().map(|d| {
        let dt    = now - chrono::Duration::days(d);
        let label = dt.format("%Y-%m-%d").to_string();
        let short = dt.format("%d").to_string();
        let count = raw.get(&label).copied().unwrap_or(0);
        (short, count)
    }).collect()
}

pub fn activity_90d(path: &std::path::Path) -> Vec<(String, usize)> {
    let raw = git_activity_raw(path, Some(90), "%Y-%m-%d");
    let now = chrono::Local::now();
    (0usize..13).rev().map(|w| {
        let end_d   = (w * 7) as i64;
        let start_d = end_d + 7;
        let dt_end   = now - chrono::Duration::days(end_d);
        let dt_start = now - chrono::Duration::days(start_d - 1);
        let label  = format!("W{:02}", 13 - w);
        let short  = dt_start.format("%m-%d").to_string();
        let count: usize = (0..7).map(|i| {
            let dt = now - chrono::Duration::days(end_d + i);
            let k  = dt.format("%Y-%m-%d").to_string();
            raw.get(&k).copied().unwrap_or(0)
        }).sum();
        let _ = (label, dt_end);
        (short, count)
    }).collect()
}

pub fn activity_1y(path: &std::path::Path) -> Vec<(String, usize)> {
    use chrono::Datelike;
    let raw = git_activity_raw(path, Some(365), "%Y-%m");
    let now = chrono::Local::now();
    let (y0, m0) = (now.year(), now.month() as i32);
    (0i32..12).rev().map(|m| {
        let total = y0 * 12 + (m0 - 1) - m;
        let (y, mon) = (total.div_euclid(12), total.rem_euclid(12) + 1);
        let label = format!("{:04}-{:02}", y, mon);
        const MONTHS: [&str; 12] = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
        let short = MONTHS[(mon - 1) as usize].to_string();
        let count = raw.get(&label).copied().unwrap_or(0);
        (short, count)
    }).collect()
}

pub fn activity_all(path: &std::path::Path) -> Vec<(String, usize)> {
    let raw = git_activity_raw(path, None, "%Y");
    let mut result: Vec<(String, usize)> = raw.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

pub fn repo_first_commit_ts(path: &std::path::Path) -> Option<i64> {
    let repo = Repository::open(path).ok()?;
    let mut rw = repo.revwalk().ok()?;
    rw.push_head().ok()?;
    let oids: Vec<_> = rw.filter_map(|r| r.ok()).collect();
    let last_oid = oids.last()?;
    let commit = repo.find_commit(*last_oid).ok()?;
    Some(commit.time().seconds())
}

pub fn build_tree(all_files: &[String]) -> Vec<TreeNode> {
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for f in all_files {
        let parts: Vec<&str> = f.split('/').collect();
        for i in 1..parts.len() {
            dirs.insert(parts[..i].join("/"));
        }
    }

    let mut dir_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    dir_children.entry(String::new()).or_default();

    for dir in &dirs {
        let parent = dir.rfind('/').map(|i| &dir[..i]).unwrap_or("").to_string();
        dir_children.entry(parent).or_default().push(dir.clone());
        dir_children.entry(dir.clone()).or_default();
    }
    for f in all_files {
        let parent = f.rfind('/').map(|i| &f[..i]).unwrap_or("").to_string();
        dir_children.entry(parent).or_default().push(f.clone());
    }

    for children in dir_children.values_mut() {
        children.sort_by(|a, b| {
            let a_is_dir = dirs.contains(a.as_str());
            let b_is_dir = dirs.contains(b.as_str());
            b_is_dir.cmp(&a_is_dir).then(a.cmp(b))
        });
        children.dedup();
    }

    let mut result = Vec::new();
    flatten_tree("", &dir_children, &dirs, 0, &mut result);
    result
}

fn flatten_tree(
    prefix: &str,
    dir_children: &BTreeMap<String, Vec<String>>,
    dirs: &BTreeSet<String>,
    depth: usize,
    out: &mut Vec<TreeNode>,
) {
    let children = match dir_children.get(prefix) {
        Some(c) => c,
        None => return,
    };
    for child in children {
        let is_dir = dirs.contains(child.as_str());
        let name = child.rfind('/').map(|i| &child[i+1..]).unwrap_or(child).to_string();
        out.push(TreeNode { name, rel_path: child.clone(), is_dir, depth, expanded: true });
        if is_dir {
            flatten_tree(child, dir_children, dirs, depth + 1, out);
        }
    }
}

pub fn read_file_preview(repo_path: &std::path::Path, rel: &str, max_lines: usize) -> Vec<String> {
    let full = repo_path.join(rel);
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(_) => return vec!["[cannot read file]".into()],
    };
    if bytes.iter().take(512).any(|&b| b == 0) {
        return vec!["[binary file]".into()];
    }
    let text = String::from_utf8_lossy(&bytes);
    text.lines().take(max_lines).map(|l| l.to_string()).collect()
}

pub fn checkout_branch(path: &std::path::Path, name: &str) -> bool {
    git_run_ok(path, &["checkout", name])
}

pub fn delete_branch(path: &std::path::Path, name: &str) -> bool {
    git_run_ok(path, &["branch", "-d", name])
}

pub fn push_branch(path: &std::path::Path, name: &str) -> bool {
    git_run_ok(path, &["push", "-u", "origin", name])
}

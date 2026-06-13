use super::HistEntry;
use crate::status::format_age_pub;
use git2::Repository;

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
    commits.sort_by(|a, b| b.ts().cmp(&a.ts()));
    commits.truncate(limit + 20);
    commits
}

pub fn count_contributors(path: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C").arg(path).args(["shortlog", "-sn", "--all"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
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

pub fn count_local_branches(path: &std::path::Path) -> usize {
    std::process::Command::new("git")
        .arg("-C").arg(path).args(["branch"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(1)
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

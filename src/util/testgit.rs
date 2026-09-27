use std::path::{Path, PathBuf};
use std::process::Command;

pub fn git(dir: &Path, args: &[&str]) -> String {
    git_with_env(dir, args, &[])
}

pub fn git_with_env(dir: &Path, args: &[&str], env: &[(&str, String)]) -> String {
    let out = Command::new("git")
        .arg("-C").arg(dir)
        .args(["-c", "user.name=Test User", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".missing-global-gitconfig"))
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .output()
        .expect("git binary must be on PATH");
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn init_repo(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    path.to_path_buf()
}

pub fn commit_file(repo: &Path, file: &str, content: &str, message: &str) -> String {
    commit_with_env(repo, file, content, message, &[])
}

pub fn commit_at(repo: &Path, file: &str, content: &str, message: &str, unix_ts: i64) -> String {
    let date = format!("@{unix_ts} +0000");
    commit_with_env(repo, file, content, message, &[
        ("GIT_AUTHOR_DATE", date.clone()),
        ("GIT_COMMITTER_DATE", date),
    ])
}

fn commit_with_env(repo: &Path, file: &str, content: &str, message: &str, env: &[(&str, String)]) -> String {
    let target = repo.join(file);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(target, content).unwrap();
    git(repo, &["add", "-A"]);
    git_with_env(repo, &["commit", "-q", "-m", message], env);
    git(repo, &["rev-parse", "HEAD"])
}

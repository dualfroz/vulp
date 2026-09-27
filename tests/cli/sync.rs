use crate::support::Sandbox;

fn fake_github_token() -> String {
    format!("ghp_{}", "Zx81".repeat(9))
}

#[test]
fn pull_brings_in_commits_from_the_remote() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");
    let teammate = sb.clone_outside(&bare, "teammate");
    let upstream_head = sb.commit(&teammate, "feature.txt", "x", "teammate work");
    sb.git(&teammate, &["push", "-q"]);

    let run = sb.vulp(&["sync", "alpha"]).success();
    assert!(run.stdout.contains("synced 1/1 repos"), "{}", run.stdout);
    assert_eq!(sb.git(&alpha, &["rev-parse", "HEAD"]), upstream_head);
    assert!(alpha.join("feature.txt").exists());

    let log = sb.vulp(&["activity", "--json"]).success().json();
    assert_eq!(log[0]["kind"], "Pull");
    assert_eq!(log[0]["repo"], "alpha");
}

#[test]
fn push_and_commit_push_publish_local_work() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");

    let local = sb.commit(&alpha, "a.txt", "1", "local commit");
    sb.vulp(&["sync", "alpha", "--action", "push"]).success();
    assert_eq!(sb.git(&bare, &["rev-parse", "main"]), local);
    assert_eq!(sb.vulp(&["status", "alpha", "--json"]).success().json()["ahead"], 0);

    std::fs::write(alpha.join("b.txt"), "2").unwrap();
    std::fs::write(alpha.join("a.txt"), "changed").unwrap();
    let run = sb.vulp(&["sync", "alpha", "--action", "commit-push", "-m", "feat: batch update"]).success();
    assert!(run.stdout.contains("synced 1/1 repos"));
    assert_eq!(sb.git(&bare, &["log", "-1", "--format=%s", "main"]), "feat: batch update");
    assert_eq!(sb.git(&bare, &["show", "--name-only", "--format=", "main"]), "a.txt\nb.txt");
    let status = sb.vulp(&["status", "alpha", "--json"]).success().json();
    assert_eq!(status["is_dirty"], false);
    assert_eq!(status["last_commit_msg"], "feat: batch update");

    let kinds: Vec<String> = sb.vulp(&["activity", "--json"]).success().json().as_array().unwrap()
        .iter().map(|e| e["detail"].as_str().unwrap().to_string()).collect();
    assert!(kinds.contains(&"vulp sync (push)".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"vulp sync (commit+push: feat: batch update)".to_string()), "{kinds:?}");
}

#[test]
fn secret_scan_blocks_commit_push_until_overridden() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");
    let before = sb.git(&bare, &["rev-parse", "main"]);
    let token = fake_github_token();
    std::fs::write(alpha.join(".env"), format!("GITHUB_TOKEN={token}\n")).unwrap();
    std::fs::write(alpha.join("app.txt"), "harmless").unwrap();

    let blocked = sb.vulp(&["sync", "alpha", "--action", "commit-push", "-m", "add env"]).failure(1);
    assert!(blocked.stderr.contains("SECRETS in staged changes"), "{}", blocked.stderr);
    assert!(blocked.stderr.contains(".env — GitHub token (ghp_…)"), "{}", blocked.stderr);
    assert!(!blocked.stderr.contains(&token) && !blocked.stdout.contains(&token));
    assert!(blocked.stdout.contains("synced 0/1 repos"));

    assert_eq!(sb.git(&bare, &["rev-parse", "main"]), before);
    assert_eq!(sb.git(&alpha, &["log", "-1", "--format=%s"]), "initial commit");
    assert_eq!(sb.git(&alpha, &["diff", "--cached", "--name-only"]), "");
    assert!(alpha.join(".env").exists());

    sb.vulp(&["sync", "alpha", "--action", "commit-push", "-m", "add env", "--allow-secrets"]).success();
    assert_eq!(sb.git(&bare, &["log", "-1", "--format=%s", "main"]), "add env");
}

#[test]
fn dry_run_changes_nothing() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");
    sb.repo_with_remote("beta");
    let before = sb.git(&bare, &["rev-parse", "main"]);
    std::fs::write(alpha.join("x.txt"), "x").unwrap();

    let run = sb.vulp(&["sync", "--all", "--action", "commit-push", "-m", "msg", "--dry-run"]).success();
    assert!(run.stdout.contains("DRY RUN: would commit+push alpha (message: msg)"), "{}", run.stdout);
    assert!(run.stdout.contains("DRY RUN: would commit+push beta"), "{}", run.stdout);
    assert_eq!(sb.git(&bare, &["rev-parse", "main"]), before);
    assert_eq!(sb.git(&alpha, &["status", "--porcelain"]), "?? x.txt");
    assert!(sb.read_data("activity.log").is_empty());

    let pull = sb.vulp(&["sync", "beta", "--dry-run"]).success();
    assert_eq!(pull.stdout.trim(), "DRY RUN: would pull beta");
}

#[test]
fn protected_repos_need_force() {
    let sb = Sandbox::with_config("protected_repos = [\"Alpha\"]");
    let (alpha, bare) = sb.repo_with_remote("alpha");
    let local = sb.commit(&alpha, "a.txt", "1", "local");

    let blocked = sb.vulp(&["sync", "alpha", "--action", "push"]).failure(1);
    assert!(blocked.stderr.contains("protected repos: alpha"), "{}", blocked.stderr);
    sb.vulp(&["sync", "alpha", "--action", "push", "--dry-run"]).failure(1);
    assert_ne!(sb.git(&bare, &["rev-parse", "main"]), local);

    sb.vulp(&["sync", "alpha", "--action", "push", "--force"]).success();
    assert_eq!(sb.git(&bare, &["rev-parse", "main"]), local);
}

#[test]
fn partial_failure_exits_non_zero_but_syncs_the_rest() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");
    let lonely = sb.repo("lonely");
    let local = sb.commit(&alpha, "a.txt", "1", "local");
    sb.commit(&lonely, "b.txt", "1", "no remote");

    let run = sb.vulp(&["sync", "--all", "--action", "push"]).failure(1);
    assert!(run.stdout.contains("synced 1/2 repos"), "{}", run.stdout);
    assert!(run.stderr.contains("── lonely [push]") && run.stderr.contains("FAILED"), "{}", run.stderr);
    assert_eq!(sb.git(&bare, &["rev-parse", "main"]), local);
}

#[test]
fn project_sync_targets_only_members() {
    let sb = Sandbox::new();
    let (api, api_bare) = sb.repo_with_remote("api");
    let (web, web_bare) = sb.repo_with_remote("web");
    let api_head = sb.commit(&api, "a.txt", "1", "api change");
    let web_before = sb.git(&web_bare, &["rev-parse", "main"]);
    sb.commit(&web, "w.txt", "1", "web change");

    sb.vulp(&["project", "create", "backend"]).success();
    sb.vulp(&["project", "assign", "backend", "api"]).success();
    sb.vulp(&["sync", "--project", "backend", "--action", "push"]).success();
    assert_eq!(sb.git(&api_bare, &["rev-parse", "main"]), api_head);
    assert_eq!(sb.git(&web_bare, &["rev-parse", "main"]), web_before);
    sb.vulp(&["sync", "--project", "frontend"]).failure(4);
}

#[test]
fn sync_argument_errors() {
    let sb = Sandbox::new();
    sb.repo("alpha");
    assert!(sb.vulp(&["sync"]).failure(2).stderr.contains("specify repo names"));
    assert!(sb.vulp(&["sync", "alpha", "--action", "merge"]).failure(2).stderr.contains("unknown action"));
    assert!(sb.vulp(&["sync", "alpha", "--action", "commit-push"]).failure(2).stderr.contains("--message is required"));
    assert!(sb.vulp(&["sync", "ghost"]).failure(4).stderr.contains("repo 'ghost' not found"));
}

#[test]
fn release_tags_and_pushes_without_a_provider_account() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");

    let dry = sb.vulp(&["release", "alpha", "--tag", "v1.0.0", "--dry-run"]).success();
    assert!(dry.stdout.contains("DRY RUN: would tag alpha as v1.0.0"), "{}", dry.stdout);
    assert_eq!(sb.git(&alpha, &["tag", "--list"]), "");

    let run = sb.vulp(&["release", "alpha", "--tag", "v1.0.0", "--notes", "First stable release"]).success();
    assert!(run.stdout.contains("released v1.0.0 on alpha"), "{}", run.stdout);
    assert_eq!(sb.git(&alpha, &["tag", "-l", "--format=%(contents:subject)", "v1.0.0"]), "First stable release");
    assert!(sb.git(&bare, &["tag", "--list"]).contains("v1.0.0"));
    assert_eq!(sb.vulp(&["status", "alpha", "--json"]).success().json()["last_tag"], "v1.0.0");
    let log = sb.vulp(&["activity", "--kind", "release", "--json"]).success().json();
    assert_eq!(log[0]["detail"], "v1.0.0");

    let lonely = sb.repo("lonely");
    let failed = sb.vulp(&["release", "lonely", "--tag", "v0.1.0"]).failure(1);
    assert!(failed.stderr.contains("pushing tag 'v0.1.0' failed"), "{}", failed.stderr);
    assert_eq!(sb.git(&lonely, &["tag", "--list"]), "v0.1.0");
}

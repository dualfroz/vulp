use crate::support::{find, names, Sandbox};

#[test]
fn list_reports_git_state_for_every_repo() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.commit(&alpha, "src/main.rs", "fn main() {}", "add main");
    sb.commit(&alpha, "src/lib.rs", "pub fn f() {}", "add lib");
    let beta = sb.repo("beta");
    std::fs::write(beta.join("notes.txt"), "wip").unwrap();
    sb.repo("clients/gamma");
    std::fs::create_dir_all(sb.workspace.join("node_modules/pkg/.git")).unwrap();

    let list = sb.vulp(&["list", "--json"]).success().json();
    assert_eq!(names(&list).len(), 3);
    assert_eq!(names(&list)[0], "beta");

    let a = find(&list, "alpha");
    assert_eq!(a["branch"], "main");
    assert_eq!(a["is_dirty"], false);
    assert_eq!(a["ahead"], 0);
    assert_eq!(a["behind"], 0);
    assert_eq!(a["last_commit_msg"], "add lib");
    assert_eq!(a["lang"], "Rust 66%  Markdown 33%");
    assert_eq!(a["path"], alpha.to_str().unwrap());
    assert_eq!(a["has_license"], false);
    assert_eq!(a["owner"], serde_json::Value::Null);
    assert_eq!(find(&list, "beta")["is_dirty"], true);
    assert_eq!(find(&list, "clients/gamma")["branch_count"], 1);

    assert_eq!(names(&sb.vulp(&["list", "--dirty", "--json"]).success().json()), ["beta"]);
    assert_eq!(names(&sb.vulp(&["list", "--lang", "RUST", "--json"]).success().json()), ["alpha"]);
    assert!(names(&sb.vulp(&["list", "--behind", "--json"]).success().json()).is_empty());

    let human = sb.vulp(&["list"]).success();
    assert!(human.stdout.lines().any(|l| l.starts_with("beta") && l.contains('⚠')), "{}", human.stdout);
}

#[test]
fn empty_workspace_is_not_an_error() {
    let sb = Sandbox::new();
    assert_eq!(sb.vulp(&["list", "--json"]).success().json(), serde_json::json!([]));
    assert!(sb.vulp(&["list"]).success().stdout.contains("no repos match"));
    let summary = sb.vulp(&["scan", "--json"]).success().json();
    assert_eq!(summary, serde_json::json!({ "total": 0, "dirty": 0, "behind": 0, "ahead": 0 }));
}

#[test]
fn conflicting_external_filters_are_bad_usage() {
    let sb = Sandbox::new();
    sb.vulp(&["list", "--external", "--no-external"]).failure(2);
}

#[test]
fn status_matches_label_or_directory_name() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.git(&alpha, &["remote", "add", "origin", "git@github.com:octo/alpha-site.git"]);
    sb.git(&alpha, &["tag", "v1.0.0"]);

    let status = sb.vulp(&["status", "ALPHA", "--json"]).success().json();
    assert_eq!(status["name"], "alpha");
    assert_eq!(status["owner"], "octo");
    assert_eq!(status["remote_repo_name"], "alpha-site");
    assert_eq!(status["last_tag"], "v1.0.0");

    let human = sb.vulp(&["status", "alpha"]).success().stdout;
    assert!(human.contains("branch:      main"), "{human}");
    assert!(human.contains("last tag:    v1.0.0"), "{human}");
    assert!(human.contains("owner:       octo"), "{human}");

    let missing = sb.vulp(&["status", "nope"]).failure(4);
    assert!(missing.stderr.contains("repo 'nope' not found"), "{}", missing.stderr);
    assert!(missing.stdout.is_empty());
}

#[test]
fn ahead_and_behind_follow_the_remote_tracking_branch() {
    let sb = Sandbox::new();
    let (alpha, bare) = sb.repo_with_remote("alpha");
    let teammate = sb.clone_outside(&bare, "teammate");
    sb.commit(&teammate, "feature.txt", "x", "teammate work");
    sb.git(&teammate, &["push", "-q"]);

    let status = sb.vulp(&["status", "alpha", "--json"]).success().json();
    assert_eq!(status["behind"], 0);

    sb.git(&alpha, &["fetch", "-q"]);
    let status = sb.vulp(&["status", "alpha", "--json"]).success().json();
    assert_eq!((status["ahead"].as_u64(), status["behind"].as_u64()), (Some(0), Some(1)));
    assert_eq!(names(&sb.vulp(&["list", "--behind", "--json"]).success().json()), ["alpha"]);

    sb.commit(&alpha, "local.txt", "y", "local work");
    let status = sb.vulp(&["status", "alpha", "--json"]).success().json();
    assert_eq!((status["ahead"].as_u64(), status["behind"].as_u64()), (Some(1), Some(1)));
    assert_eq!(names(&sb.vulp(&["list", "--ahead", "--json"]).success().json()), ["alpha"]);

    let summary = sb.vulp(&["scan", "--json"]).success().json();
    assert_eq!(summary, serde_json::json!({ "total": 1, "dirty": 0, "behind": 1, "ahead": 1 }));
    assert!(sb.vulp(&["scan"]).success().stdout.contains("1 repos"));
}

#[test]
fn brief_combines_state_todos_and_cached_items() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.commit(&alpha, "a.txt", "1", "second commit");
    let beta = sb.repo("beta");
    std::fs::write(beta.join("LICENSE"), "MIT").unwrap();
    sb.write_data("todos.toml", r#"
        [[todo]]
        id = 1
        title = "alpha task"
        description = ""
        status = "Open"
        repo = "alpha"
        created_at = 1
        updated_at = 1

        [[todo]]
        id = 2
        title = "finished"
        description = ""
        status = "Done"
        repo = "alpha"
        created_at = 1
        updated_at = 1
    "#);
    sb.write_data("meta_cache.toml", &crate::remote::cache_with_items());

    let ws = sb.vulp(&["brief", "--json"]).success().json();
    assert_eq!(ws["summary"]["total"], 2);
    assert_eq!(ws["summary"]["dirty"], 1);
    assert_eq!(ws["summary"]["missing_license"], 1);
    assert_eq!(names(&ws["needs_attention"]), ["beta"]);
    assert_eq!(ws["todos"].as_array().unwrap().len(), 1);
    assert_eq!(ws["open_prs"].as_array().unwrap().len(), 2);
    assert_eq!(ws["open_issues"].as_array().unwrap().len(), 1);

    let one = sb.vulp(&["brief", "alpha", "--json"]).success().json();
    assert_eq!(one["repo"]["name"], "alpha");
    let messages: Vec<&str> = one["recent_commits"].as_array().unwrap().iter()
        .map(|c| c["message"].as_str().unwrap()).collect();
    assert_eq!(messages, ["second commit", "initial commit"]);
    assert_eq!(one["todos"][0]["title"], "alpha task");
    assert_eq!(one["todos"].as_array().unwrap().len(), 1);
    assert_eq!(one["open_prs"].as_array().unwrap().len(), 1);
    assert_eq!(one["open_prs"][0]["number"], 7);
    assert_eq!(one["open_issues"].as_array().unwrap().len(), 0);

    let human = sb.vulp(&["brief"]).success().stdout;
    assert!(human.contains("2 repos · 1 dirty"), "{human}");
}

#[test]
fn stats_count_commits_per_repo_in_the_period() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.commit(&alpha, "a.txt", "1", "two");
    sb.commit(&alpha, "a.txt", "2", "three");
    sb.repo("beta");

    let stats = sb.vulp(&["stats", "--period", "7d", "--json"]).success().json();
    assert_eq!(stats["period"], "7d");
    assert_eq!(stats["total_commits"], 4);
    assert_eq!(stats["repos_active"], 2);
    assert_eq!(stats["per_repo"], serde_json::json!([["alpha", 3], ["beta", 1]]));

    for period in ["1d", "30d", "90d", "1y", "all"] {
        assert_eq!(sb.vulp(&["stats", "--period", period, "--json"]).success().json()["total_commits"], 4, "{period}");
    }
    let bad = sb.vulp(&["stats", "--period", "2w"]).failure(2);
    assert!(bad.stderr.contains("unknown period"));
}

#[test]
fn activity_log_is_filtered_and_limited() {
    let sb = Sandbox::new();
    sb.vulp(&["todo", "add", "first"]).success();
    sb.vulp_with_env(&["todo", "add", "second"], &[("VULP_AGENT", "1")]).success();
    sb.write_data("activity.log", &format!(
        "{}{{\"ts\":5,\"kind\":\"Push\",\"repo\":\"alpha\",\"detail\":\"old push\"}}\nnot json\n",
        sb.read_data("activity.log"),
    ));

    let all = sb.vulp(&["activity", "--json"]).success().json();
    let details: Vec<&str> = all.as_array().unwrap().iter().map(|e| e["detail"].as_str().unwrap()).collect();
    assert_eq!(details.len(), 3);
    assert!(details.contains(&"added: second · via agent"), "{details:?}");
    assert_eq!(details[2], "old push");

    let todos = sb.vulp(&["activity", "--kind", "todo", "--json"]).success().json();
    assert_eq!(todos.as_array().unwrap().len(), 2);
    assert!(todos.as_array().unwrap().iter().all(|e| e["kind"] == "Todo"));

    assert_eq!(sb.vulp(&["activity", "--limit", "1", "--json"]).success().json().as_array().unwrap().len(), 1);
    assert!(sb.vulp(&["activity", "--kind", "release"]).success().stdout.contains("no matching activity"));
}

#[test]
fn cached_metadata_drives_alert_drift_and_external_filters() {
    let sb = Sandbox::new();
    for name in ["api", "web", "worker", "vendored"] {
        sb.repo(name);
    }
    sb.write_data("meta_cache.toml", r#"
        [repos.api.meta]
        stars = 12
        dependabot_open = 3
        deploy_status = "behind:2"
        ci_status = "failure"
        fetched_at = 1

        [repos.web.meta]
        dependabot_open = 0
        deploy_status = "in_sync"
        fetched_at = 1

        [repos.worker.meta]
        deploy_status = "diverged"
        fetched_at = 1
    "#);
    sb.write_data("stats_cache.toml", r#"
        [repos.vendored]
        authored = false

        [repos.api]
        authored = true
    "#);

    let list = sb.vulp(&["list", "--json"]).success().json();
    assert_eq!(find(&list, "api")["stars"], 12);
    assert_eq!(find(&list, "api")["ci_status"], "failure");
    assert_eq!(find(&list, "web")["deploy_status"], "in_sync");
    assert_eq!(find(&list, "vendored")["deploy_status"], serde_json::Value::Null);

    let mut stale = names(&sb.vulp(&["list", "--stale-deploy", "--json"]).success().json());
    stale.sort();
    assert_eq!(stale, ["api", "worker"]);
    let human = sb.vulp(&["list", "--stale-deploy"]).success().stdout;
    assert!(human.lines().any(|l| l.starts_with("api") && l.contains("2 behind")), "{human}");
    assert!(human.lines().any(|l| l.starts_with("worker") && l.contains("diverged")), "{human}");

    assert_eq!(names(&sb.vulp(&["list", "--alerts", "--json"]).success().json()), ["api"]);
    assert_eq!(names(&sb.vulp(&["list", "--external", "--json"]).success().json()), ["vendored"]);
    let mut own = names(&sb.vulp(&["list", "--no-external", "--json"]).success().json());
    own.sort();
    assert_eq!(own, ["api", "web", "worker"]);
}

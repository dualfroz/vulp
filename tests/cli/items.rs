use crate::support::{find, names, Sandbox};

#[test]
fn todo_lifecycle() {
    let sb = Sandbox::new();
    assert!(sb.vulp(&["todo", "list"]).success().stdout.contains("no todos"));

    assert!(sb.vulp(&["todo", "add", "write tests", "--desc", "cover the CLI", "--project", "core"])
        .success().stdout.contains("todo #1 added"));
    sb.vulp(&["todo", "add", "ship release"]).success();
    sb.vulp(&["todo", "add", "drop legacy code"]).success();

    sb.vulp(&["todo", "start", "1"]).success();
    assert!(sb.vulp(&["todo", "done", "2"]).success().stdout.contains("todo #2: Done"));
    sb.vulp(&["todo", "cancel", "3"]).success();

    let all = sb.vulp(&["todo", "list", "--json"]).success().json();
    assert_eq!(all.as_array().unwrap().len(), 3);
    let first = all.as_array().unwrap().iter().find(|t| t["id"] == 1).unwrap();
    assert_eq!(first["status"], "In Progress");
    assert_eq!(first["description"], "cover the CLI");
    assert_eq!(first["project"], "core");

    let done = sb.vulp(&["todo", "list", "--status", "done", "--json"]).success().json();
    assert_eq!(done.as_array().unwrap().len(), 1);
    assert_eq!(done[0]["title"], "ship release");
    let in_progress = sb.vulp(&["todo", "list", "--status", "in-progress", "--json"]).success().json();
    assert_eq!(in_progress[0]["id"], 1);

    assert!(sb.vulp(&["todo", "rm", "3"]).success().stdout.contains("removed (drop legacy code)"));
    assert_eq!(sb.vulp(&["todo", "list", "--json"]).success().json().as_array().unwrap().len(), 2);
    assert!(sb.vulp(&["todo", "add", "next"]).success().stdout.contains("todo #3 added"));

    assert!(sb.read_data("todos.toml").contains("write tests"));
    assert!(sb.vulp(&["todo", "done", "99"]).failure(4).stderr.contains("todo #99 not found"));
    sb.vulp(&["todo", "rm", "99"]).failure(4);
    sb.vulp(&["todo", "list", "--status", "someday"]).failure(2);
}

#[test]
fn project_lifecycle_and_filtering() {
    let sb = Sandbox::new();
    sb.repo("api");
    sb.repo("clients/web");
    sb.repo("tools");

    sb.vulp(&["project", "create", "Shop", "--color", "#e74c3c"]).success();
    assert!(sb.vulp(&["project", "create", "shop"]).failure(1).stderr.contains("already exists"));
    assert!(sb.vulp(&["project", "assign", "shop", "api", "WEB"]).success().stdout.contains("2 repos"));
    sb.vulp(&["project", "assign", "shop", "api"]).success();

    let projects = sb.vulp(&["project", "list", "--json"]).success().json();
    assert_eq!(projects[0]["name"], "Shop");
    assert_eq!(projects[0]["color"], "#e74c3c");
    assert_eq!(projects[0]["repos"], serde_json::json!(["api", "clients/web"]));

    let mut members = names(&sb.vulp(&["list", "--project", "SHOP", "--json"]).success().json());
    members.sort();
    assert_eq!(members, ["api", "clients/web"]);

    sb.vulp(&["project", "assign", "shop", "ghost"]).failure(4);
    sb.vulp(&["project", "assign", "nope", "api"]).failure(4);
    sb.vulp(&["project", "assign", "shop"]).failure(2);
    sb.vulp(&["list", "--project", "nope"]).failure(4);

    assert!(sb.vulp(&["project", "list"]).success().stdout.contains("Shop  (2 repos)"));
    sb.vulp(&["project", "rm", "shop"]).success();
    assert_eq!(sb.vulp(&["project", "list", "--json"]).success().json(), serde_json::json!([]));
    sb.vulp(&["project", "rm", "shop"]).failure(4);
    assert!(sb.workspace.join("api").exists());
}

#[test]
fn repo_url_path_label_bookmark_and_pin() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.repo("beta");
    sb.git(&alpha, &["remote", "add", "origin", "git@github.com:octo/alpha.git"]);

    assert_eq!(sb.vulp(&["repo", "url", "alpha"]).success().stdout.trim(), "git@github.com:octo/alpha.git");
    assert!(sb.vulp(&["repo", "url", "beta"]).failure(4).stderr.contains("no 'origin' remote"));
    assert_eq!(sb.vulp(&["repo", "path", "beta"]).success().stdout.trim(), sb.workspace.join("beta").to_str().unwrap());
    sb.vulp(&["repo", "path", "gamma"]).failure(4);

    sb.vulp(&["repo", "label", "alpha", "Alpha Service"]).success();
    let list = sb.vulp(&["list", "--json"]).success().json();
    assert_eq!(find(&list, "Alpha Service")["path"], alpha.to_str().unwrap());
    assert_eq!(sb.vulp(&["status", "alpha service", "--json"]).success().json()["name"], "Alpha Service");
    assert_eq!(sb.vulp(&["status", "alpha", "--json"]).success().json()["name"], "Alpha Service");
    assert!(sb.vulp(&["repo", "label", "alpha"]).success().stdout.contains("label cleared"));
    assert!(names(&sb.vulp(&["list", "--json"]).success().json()).contains(&"alpha".to_string()));

    assert!(sb.vulp(&["repo", "bookmark", "alpha"]).success().stdout.contains("bookmarked alpha"));
    assert!(sb.read_data("bookmarks.toml").contains(alpha.to_str().unwrap()));
    assert!(sb.vulp(&["repo", "bookmark", "alpha"]).success().stdout.contains("unbookmarked alpha"));
    assert!(!sb.read_data("bookmarks.toml").contains(alpha.to_str().unwrap()));

    assert!(sb.vulp(&["repo", "pin", "beta"]).success().stdout.contains("pinned beta"));
    assert!(sb.read_data("pins.toml").contains("beta"));
    assert!(sb.vulp(&["repo", "pin", "beta"]).success().stdout.contains("unpinned beta"));
}

#[test]
fn repo_license_writes_only_where_missing() {
    let sb = Sandbox::with_config("protected_repos = [\"locked\"]");
    let alpha = sb.repo("alpha");
    let beta = sb.repo("beta");
    std::fs::write(beta.join("LICENSE.md"), "custom").unwrap();
    sb.repo("gamma");

    sb.vulp(&["repo", "license"]).failure(2);
    sb.vulp(&["repo", "license", "alpha", "--kind", "gpl"]).failure(2);
    sb.vulp(&["repo", "license", "ghost", "--holder", "X"]).failure(4);

    let dry = sb.vulp(&["repo", "license", "alpha", "beta", "--holder", "Jane Doe", "--dry-run"]).success();
    assert!(dry.stdout.contains("would write MIT LICENSE to alpha (holder: Jane Doe)"), "{}", dry.stdout);
    assert!(dry.stdout.contains("would skip beta"), "{}", dry.stdout);
    assert!(!alpha.join("LICENSE").exists());

    let out = sb.vulp(&["repo", "license", "alpha", "beta", "--kind", "apache-2.0", "--holder", "Jane Doe", "--json"])
        .success().json();
    assert_eq!(out, serde_json::json!({
        "kind": "Apache-2.0", "holder": "Jane Doe",
        "written": ["alpha"], "skipped": ["beta"], "failed": [],
    }));
    let text = std::fs::read_to_string(alpha.join("LICENSE")).unwrap();
    assert!(text.contains("Apache License") && text.contains("Jane Doe"));
    assert_eq!(std::fs::read_to_string(beta.join("LICENSE.md")).unwrap(), "custom");

    let all = sb.vulp(&["repo", "license", "--all", "--json"]).success().json();
    assert_eq!(all["written"], serde_json::json!(["gamma"]));
    assert_eq!(all["holder"], "Test User");
    assert!(std::fs::read_to_string(sb.workspace.join("gamma/LICENSE")).unwrap().contains("MIT License"));
    assert!(sb.vulp(&["repo", "license", "--all"]).success().stdout.contains("nothing to do"));

    let activity = sb.vulp(&["activity", "--kind", "license", "--json"]).success().json();
    assert_eq!(activity.as_array().unwrap().len(), 2);

    sb.repo("locked");
    let blocked = sb.vulp(&["repo", "license", "locked", "--holder", "X"]).failure(1);
    assert!(blocked.stderr.contains("protected repos: locked"), "{}", blocked.stderr);
    assert!(!sb.workspace.join("locked/LICENSE").exists());
    sb.vulp(&["repo", "license", "locked", "--holder", "X", "--force"]).success();
    assert!(sb.workspace.join("locked/LICENSE").exists());
}

#[test]
fn unknown_subcommands_and_flags_are_usage_errors() {
    let sb = Sandbox::new();
    sb.vulp(&["frobnicate"]).failure(2);
    sb.vulp(&["list", "--bogus"]).failure(2);
    sb.vulp(&["todo", "done", "abc"]).failure(2);
    let version = sb.vulp(&["--version"]).success();
    assert!(version.stdout.starts_with("vulp "), "{}", version.stdout);
}

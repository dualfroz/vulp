use crate::support::Sandbox;

pub fn cache_with_items() -> String {
    r#"
        [repos.alpha.meta]
        stars = 4
        fetched_at = 1

        [[repos.alpha.prs]]
        repo = "alpha"
        number = 7
        title = "Add parser"
        author = "octo"
        head_branch = "feature/parser"
        base_branch = "main"
        created_ts = 200
        updated_ts = 300
        url = "https://github.com/octo/alpha/pull/7"
        draft = false
        labels = []

        [[repos.beta.prs]]
        repo = "beta"
        number = 3
        title = "Fix typo"
        author = "someone"
        head_branch = "typo"
        base_branch = "main"
        created_ts = 100
        updated_ts = 100
        url = "https://github.com/octo/beta/pull/3"
        draft = true
        labels = ["docs"]

        [[repos.beta.issues]]
        repo = "beta"
        number = 11
        title = "Crash on start"
        author = "user"
        labels = ["bug"]
        created_ts = 150
        updated_ts = 150
        url = "https://github.com/octo/beta/issues/11"
        comments = 2
    "#.to_string()
}

#[test]
fn pr_and_issue_lists_read_the_cache() {
    let sb = Sandbox::new();
    assert_eq!(sb.vulp(&["pr", "list", "--json"]).success().json(), serde_json::json!([]));
    assert!(sb.vulp(&["pr", "list"]).success().stdout.contains("run: vulp refresh"));
    assert!(sb.vulp(&["issue", "list"]).success().stdout.contains("run: vulp refresh"));

    sb.write_data("meta_cache.toml", &cache_with_items());
    let prs = sb.vulp(&["pr", "list", "--json"]).success().json();
    let numbers: Vec<u64> = prs.as_array().unwrap().iter().map(|p| p["number"].as_u64().unwrap()).collect();
    assert_eq!(numbers, [7, 3]);
    assert_eq!(prs[1]["draft"], true);

    let beta = sb.vulp(&["pr", "list", "--repo", "BETA", "--json"]).success().json();
    assert_eq!(beta.as_array().unwrap().len(), 1);
    assert_eq!(beta[0]["title"], "Fix typo");

    let human = sb.vulp(&["pr", "list"]).success().stdout;
    assert!(human.contains("Add parser (feature/parser → main)"), "{human}");

    let issues = sb.vulp(&["issue", "list", "--json"]).success().json();
    assert_eq!(issues[0]["number"], 11);
    assert_eq!(issues[0]["comments"], 2);
    assert_eq!(sb.vulp(&["issue", "list", "--repo", "alpha", "--json"]).success().json(), serde_json::json!([]));
    assert!(sb.vulp(&["issue", "list"]).success().stdout.contains("Crash on start (2 comments)"));
}

#[test]
fn accounts_are_listed_from_the_accounts_file() {
    let sb = Sandbox::new();
    assert_eq!(sb.vulp(&["accounts", "--json"]).success().json(), serde_json::json!([]));
    assert!(sb.vulp(&["accounts"]).success().stdout.contains("no accounts"));

    sb.write_data("accounts.toml", r#"
        [[accounts]]
        provider = "github"
        alias = "work"
        username = "octo"
        email = "octo@example.com"
        is_default = true

        [[accounts]]
        provider = "gitea"
        alias = "home"
        username = "me"
        email = "me@example.com"
        base_url = "https://git.example.com"
        is_default = false
    "#);
    let accounts = sb.vulp(&["accounts", "--json"]).success().json();
    assert_eq!(accounts.as_array().unwrap().len(), 2);
    assert_eq!(accounts[1]["base_url"], "https://git.example.com");
    let human = sb.vulp(&["accounts"]).success().stdout;
    assert!(human.contains("github/work *  @octo  (github)"), "{human}");
    assert!(human.contains("gitea/home  @me  (gitea)"), "{human}");
}

#[test]
fn provider_commands_without_an_account_exit_with_auth_error() {
    let sb = Sandbox::new();
    sb.repo("alpha");
    let cases: [&[&str]; 5] = [
        &["pr", "create", "--repo", "alpha", "--title", "t", "--head", "feature"],
        &["issue", "create", "--repo", "alpha", "--title", "t"],
        &["refresh"],
        &["repo", "collab", "alpha", "someone"],
        &["init", "fresh-project"],
    ];
    for args in cases {
        let run = sb.vulp(args).failure(3);
        assert!(run.stderr.contains("no accounts configured"), "{args:?}: {}", run.stderr);
    }
    assert!(!sb.workspace.join("fresh-project").exists());
}

#[test]
fn init_dry_run_and_existing_target() {
    let sb = Sandbox::new();
    let dry = sb.vulp(&["init", "new-tool", "--private", "--dry-run"]).success();
    assert!(dry.stdout.contains("DRY RUN: would init"), "{}", dry.stdout);
    assert!(dry.stdout.contains("private repo"), "{}", dry.stdout);
    assert!(!sb.workspace.join("new-tool").exists());

    sb.repo("taken");
    let run = sb.vulp(&["init", "taken"]).failure(1);
    assert!(run.stderr.contains("already exists"), "{}", run.stderr);
}

#[test]
fn collab_validates_role_and_supports_dry_run() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.git(&alpha, &["remote", "add", "origin", "git@github.com:octo/alpha-remote.git"]);
    sb.vulp(&["repo", "collab", "alpha", "someone", "--role", "owner"]).failure(2);
    let dry = sb.vulp(&["repo", "collab", "alpha", "someone", "--role", "admin", "--dry-run"]).success();
    assert!(dry.stdout.contains("would invite someone to alpha-remote (admin)"), "{}", dry.stdout);
}

#[test]
fn digest_reports_own_commits_and_archives_them() {
    let sb = Sandbox::new();
    let alpha = sb.repo("alpha");
    sb.commit(&alpha, "a.txt", "1", "more work");
    sb.git(&alpha, &["tag", "-a", "v0.1.0", "-m", "first release"]);
    sb.repo("beta");
    sb.vulp(&["todo", "add", "write docs"]).success();

    let digest = sb.vulp(&["digest", "--json"]).success().json();
    let text = digest["plaintext"].as_str().unwrap();
    assert!(text.starts_with("Weekly digest "), "{text}");
    assert!(text.contains("3 commits"), "{text}");
    assert!(text.contains("Top repos: alpha 2 · beta 1"), "{text}");
    assert!(text.contains("Releases: alpha v0.1.0"), "{text}");
    assert!(text.contains("Todos: 0 done, 1 added"), "{text}");
    let archived = std::path::Path::new(digest["archived"].as_str().unwrap());
    assert!(archived.starts_with(sb.home.join(".vulp/digests")));
    let markdown = std::fs::read_to_string(archived).unwrap();
    assert!(markdown.starts_with("# Weekly digest"));
    assert!(markdown.contains("**3 commits**"));

    let send = sb.vulp(&["digest", "send", "discord", "--dry-run"]).success();
    assert!(send.stdout.contains("DRY RUN: would POST"), "{}", send.stdout);
    sb.vulp(&["digest", "send", "slack"]).failure(2);
    sb.vulp(&["digest", "webhook", "https://example.com/hook"]).failure(2);
}

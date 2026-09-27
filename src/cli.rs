use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vulp", about = "Multi-repo git workflow manager", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    /// Show timing debug panel at bottom of TUI
    #[arg(long, hide = true)]
    pub debug: bool,
    /// Machine-readable JSON output on stdout (list/status/stats/... commands)
    #[arg(long, global = true)]
    pub json: bool,
    /// Print what a mutating command (sync/release/init/collab/license)
    /// would do without doing it
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Proceed on repos listed in config `protected_repos`
    #[arg(long, global = true)]
    pub force: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configure vulp settings
    Config,
    /// Manage provider accounts
    Auth(AuthArgs),

    /// List workspace repos (name, branch, dirty/ahead/behind, language)
    List {
        /// Only repos with uncommitted changes
        #[arg(long)] dirty: bool,
        /// Only repos behind their remote
        #[arg(long)] behind: bool,
        /// Only repos ahead of their remote
        #[arg(long)] ahead: bool,
        /// Filter by language (substring, case-insensitive)
        #[arg(long)] lang: Option<String>,
        /// Filter by vulp project name
        #[arg(long)] project: Option<String>,
        /// Only repos with open Dependabot alerts (cached — run refresh first)
        #[arg(long)] alerts: bool,
        /// Only repos whose deploy host is behind or diverged (needs deploy_ssh_cmd/deploy_root configured)
        #[arg(long)] stale_deploy: bool,
        /// Hide vendored/reference clones — repos with zero of your own commits (classified on refresh)
        #[arg(long)] no_external: bool,
        /// Only vendored/reference clones — repos with zero of your own commits
        #[arg(long)] external: bool,
    },
    /// Detailed status of one repo
    Status { repo: String },
    /// Rescan the workspace and print a summary
    Scan,

    /// Pull/push/commit+push one or more repos
    Sync {
        /// Repo names (omit with --all or --project)
        repos: Vec<String>,
        /// Sync every repo in the workspace
        #[arg(long)] all: bool,
        /// Sync every repo in a vulp project
        #[arg(long)] project: Option<String>,
        /// pull | push | commit-push
        #[arg(long, default_value = "pull")] action: String,
        /// Commit message (required for commit-push)
        #[arg(short, long)] message: Option<String>,
        /// Commit even when staged changes look like secrets
        #[arg(long)] allow_secrets: bool,
    },
    /// Tag a release and publish it on the provider
    Release {
        repo: String,
        /// Tag, e.g. v1.2.0
        #[arg(long)] tag: String,
        /// Release notes body
        #[arg(long, default_value = "")] notes: String,
    },

    /// Manage todos
    Todo {
        #[command(subcommand)]
        action: TodoCmd,
    },
    /// List / create pull requests
    Pr {
        #[command(subcommand)]
        action: PrCmd,
    },
    /// List / create issues
    Issue {
        #[command(subcommand)]
        action: IssueCmd,
    },
    /// Manage vulp projects (repo groupings)
    Project {
        #[command(subcommand)]
        action: ProjectCmd,
    },
    /// Per-repo utilities (url/path/open/label/bookmark/pin/collab)
    Repo {
        #[command(subcommand)]
        action: RepoCmd,
    },
    /// Create a new repo: local init + create on provider + push
    Init {
        name: String,
        /// Create as private on the provider
        #[arg(long)] private: bool,
        /// Repo description
        #[arg(long, default_value = "")] description: String,
    },

    /// One-shot picture: repo state + commits + todos + PRs/issues (from cache)
    Brief {
        /// Repo name (omit for the whole workspace)
        repo: Option<String>,
    },
    /// Workspace-wide commit statistics
    Stats {
        /// 1d | 7d | 30d | 90d | 1y | all
        #[arg(long, default_value = "30d")] period: String,
    },
    /// Recent vulp-performed actions (activity log)
    Activity {
        #[arg(long, default_value_t = 20)] limit: usize,
        /// commit | push | pull | release | new repo | todo | pr | issue
        #[arg(long)] kind: Option<String>,
    },
    /// List configured provider accounts
    Accounts,
    /// Fetch metadata/PRs/issues + merged/closed counters from provider APIs
    Refresh,
    /// Weekly digest of your own activity (commits/releases/PRs/issues/todos)
    Digest {
        #[command(subcommand)]
        action: Option<DigestCmd>,
    },
}

#[derive(Subcommand)]
pub enum DigestCmd {
    /// Post this week's digest to a saved destination
    Send {
        /// Currently only "discord"
        target: String,
    },
    /// Store/replace the Discord webhook URL used by `digest send discord`
    Webhook { url: String },
}

#[derive(Subcommand)]
pub enum TodoCmd {
    /// List todos
    List {
        /// open | in-progress | done | cancelled
        #[arg(long)] status: Option<String>,
    },
    /// Add a todo
    Add {
        title: String,
        #[arg(long, default_value = "")] desc: String,
        #[arg(long)] project: Option<String>,
    },
    /// Mark a todo done
    Done { id: u64 },
    /// Mark a todo in-progress
    Start { id: u64 },
    /// Mark a todo cancelled
    Cancel { id: u64 },
    /// Delete a todo
    Rm { id: u64 },
}

#[derive(Subcommand)]
pub enum PrCmd {
    /// List cached open PRs (run `vulp refresh` first for fresh data)
    List {
        #[arg(long)] repo: Option<String>,
    },
    /// Open a pull request on the provider
    Create {
        #[arg(long)] repo: String,
        #[arg(long)] title: String,
        #[arg(long)] head: String,
        #[arg(long, default_value = "main")] base: String,
    },
}

#[derive(Subcommand)]
pub enum IssueCmd {
    /// List cached open issues (run `vulp refresh` first for fresh data)
    List {
        #[arg(long)] repo: Option<String>,
    },
    /// Open an issue on the provider
    Create {
        #[arg(long)] repo: String,
        #[arg(long)] title: String,
        #[arg(long, default_value = "")] body: String,
    },
}

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects with their repos
    List,
    /// Create a project
    Create {
        name: String,
        /// Hex color, e.g. "#e74c3c"
        #[arg(long, default_value = "#3da7ff")] color: String,
    },
    /// Assign repos to a project
    Assign { name: String, repos: Vec<String> },
    /// Delete a project (repos stay on disk)
    Rm { name: String },
}

#[derive(Subcommand)]
pub enum RepoCmd {
    /// Print the repo's remote URL
    Url { repo: String },
    /// Print the repo's absolute path
    Path { repo: String },
    /// Open the repo page in the browser
    Open { repo: String },
    /// Set/clear the repo's display label (empty = reset)
    Label { repo: String, #[arg(default_value = "")] name: String },
    /// Toggle bookmark
    Bookmark { repo: String },
    /// Toggle pin
    Pin { repo: String },
    /// Invite a collaborator
    Collab {
        repo: String,
        user: String,
        /// pull | push | admin
        #[arg(long, default_value = "push")] role: String,
    },
    /// Write a LICENSE file into repos that lack one (never overwrites, never commits)
    License {
        /// Repo names (omit with --all)
        repos: Vec<String>,
        /// Every workspace repo missing a license
        #[arg(long)] all: bool,
        /// mit | apache-2.0 (default: config `default_license`)
        #[arg(long)] kind: Option<String>,
        /// Copyright holder (default: config `license_holder`, then git user.name)
        #[arg(long)] holder: Option<String>,
    },
}

#[derive(Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: Option<AuthAction>,
}

#[derive(Subcommand)]
pub enum AuthAction {
    Add {
        #[arg(long)] provider: Option<String>,
        #[arg(long)] token:    Option<String>,
        #[arg(long)] alias:    Option<String>,
        #[arg(long)] url:      Option<String>,
    },
    /// Auto-import credentials from the gh CLI (no token needed)
    Import,
    Remove  { account: String },
    Switch  { account: String },
    Test    { account: Option<String> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("vulp").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn command_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn bare_invocation_opens_the_tui() {
        let cli = parse(&[]);
        assert!(cli.command.is_none());
        assert!(!cli.json && !cli.dry_run && !cli.force);
    }

    #[test]
    fn global_flags_work_after_the_subcommand() {
        let cli = parse(&["sync", "a", "b", "--action", "commit-push", "-m", "msg", "--json", "--dry-run", "--force"]);
        assert!(cli.json && cli.dry_run && cli.force);
        match cli.command {
            Some(Commands::Sync { repos, all, project, action, message, allow_secrets }) => {
                assert_eq!(repos, ["a", "b"]);
                assert!(!all && project.is_none() && !allow_secrets);
                assert_eq!(action, "commit-push");
                assert_eq!(message.as_deref(), Some("msg"));
            }
            _ => panic!("expected sync"),
        }
    }

    #[test]
    fn defaults_are_applied() {
        match parse(&["sync", "--all"]).command {
            Some(Commands::Sync { action, all, .. }) => { assert_eq!(action, "pull"); assert!(all); }
            _ => panic!("expected sync"),
        }
        match parse(&["stats"]).command {
            Some(Commands::Stats { period }) => assert_eq!(period, "30d"),
            _ => panic!("expected stats"),
        }
        match parse(&["activity"]).command {
            Some(Commands::Activity { limit, kind }) => { assert_eq!(limit, 20); assert!(kind.is_none()); }
            _ => panic!("expected activity"),
        }
        match parse(&["pr", "create", "--repo", "r", "--title", "t", "--head", "h"]).command {
            Some(Commands::Pr { action: PrCmd::Create { base, .. } }) => assert_eq!(base, "main"),
            _ => panic!("expected pr create"),
        }
        match parse(&["repo", "collab", "r", "u"]).command {
            Some(Commands::Repo { action: RepoCmd::Collab { role, .. } }) => assert_eq!(role, "push"),
            _ => panic!("expected repo collab"),
        }
        match parse(&["repo", "label", "r"]).command {
            Some(Commands::Repo { action: RepoCmd::Label { name, .. } }) => assert_eq!(name, ""),
            _ => panic!("expected repo label"),
        }
        match parse(&["project", "create", "p"]).command {
            Some(Commands::Project { action: ProjectCmd::Create { color, .. } }) => assert_eq!(color, "#3da7ff"),
            _ => panic!("expected project create"),
        }
    }

    #[test]
    fn invalid_usage_is_rejected() {
        for args in [
            vec!["status"],
            vec!["release", "r"],
            vec!["todo", "done", "not-a-number"],
            vec!["list", "--bogus"],
            vec!["frobnicate"],
        ] {
            let err = Cli::try_parse_from(std::iter::once("vulp").chain(args.iter().copied()))
                .err()
                .unwrap_or_else(|| panic!("{args:?} should fail"));
            assert_eq!(err.exit_code(), 2, "{args:?}");
        }
    }
}

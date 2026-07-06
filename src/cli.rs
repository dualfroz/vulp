//! Command-line surface: `clap` argument definitions. `vulp` with no
//! subcommand launches the TUI hub; every hub operation is also reachable
//! through a subcommand here so scripts and AI agents can drive vulp without
//! a terminal UI. Machine consumers: pass `--json` and see `docs/cli.md`.
//!
//! Exit codes: 0 ok · 1 execution error · 2 bad usage (clap) ·
//! 3 auth missing/invalid · 4 not found (repo/todo/project).

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

/// `vulp auth` subcommands. Account state is a flat list of credentials
/// (one per provider account); `Switch` changes which one is the default
/// rather than removing the others, so multiple accounts can stay configured
/// at once.
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

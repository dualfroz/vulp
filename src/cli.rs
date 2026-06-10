use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fxg", about = "Multi-repo git workflow manager", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configure fxg settings
    Config,
    /// Manage provider accounts
    Auth(AuthArgs),
    /// Initialize new repositories
    Init(InitArgs),
    /// View repository status
    Status(StatusArgs),
    /// Sync repositories (pull / commit / push / branch / PR)
    Sync(SyncArgs),
    /// Manage releases, tags and changelogs
    Release(ReleaseArgs),
    /// Rename, find and clean code
    Refactor(RefactorArgs),
    /// Code statistics and metrics
    Stats(StatsArgs),
    /// Manage repo workspaces
    Workspace(WorkspaceArgs),
    /// Summarize repository structure, languages, git history and dependencies
    Summary(SummaryArgs),
    /// Import existing local folders into git + create remote repo
    Adopt(AdoptArgs),
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
    Remove  { account: String },
    Switch  { account: String },
    Test    { account: Option<String> },
}


#[derive(Args)]
pub struct InitArgs {
    pub name: Option<String>,
    #[arg(long)] pub provider: Option<String>,
    #[arg(long, value_delimiter = ',')] pub tags: Vec<String>,
    #[arg(long, default_value = "private")] pub status: String,
    #[arg(long, default_value = "main")]    pub branch: String,
    #[arg(long)] pub batch: Option<String>,
}


#[derive(Args)]
pub struct StatusArgs {
    pub repo: Option<String>,
    #[arg(long)] pub workspace: Option<String>,
    #[arg(long)] pub filter:    Option<String>,
}


#[derive(Args)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub action: Option<SyncAction>,
}

#[derive(Subcommand)]
pub enum SyncAction {
    Pull {
        #[arg(long)] all:       bool,
        #[arg(long)] workspace: Option<String>,
    },
    Commit {
        #[arg(short = 'm', long)] message: Option<String>,
        #[arg(long)]              all:     bool,
    },
    Push {
        #[arg(long)] all:             bool,
        #[arg(long)] force_with_lease: bool,
        /// If set, also stage+commit dirty repos with this message before pushing
        #[arg(short = 'm', long)] message: Option<String>,
    },
    Branch(BranchArgs),
    Pr(PrArgs),
}

#[derive(Args)]
pub struct PrArgs {
    #[command(subcommand)]
    pub action: PrAction,
}

#[derive(Subcommand)]
pub enum PrAction {
    Create {
        #[arg(long)] title:     Option<String>,
        #[arg(long)] base:      Option<String>,
        #[arg(long)] head:      Option<String>,
        #[arg(long, value_delimiter = ',')] reviewers: Vec<String>,
        #[arg(long, value_delimiter = ',')] labels:    Vec<String>,
        #[arg(long)] repo:      Option<String>,
    },
}

#[derive(Args)]
pub struct BranchArgs {
    #[command(subcommand)]
    pub action: BranchAction,
}

#[derive(Subcommand)]
pub enum BranchAction {
    Create { name: String, #[arg(long, value_delimiter = ',')] repos: Vec<String> },
    Delete { name: String },
    Switch { name: String, #[arg(long)] all: bool },
}


#[derive(Args)]
pub struct ReleaseArgs {
    #[command(subcommand)]
    pub action: Option<ReleaseAction>,
}

#[derive(Subcommand)]
pub enum ReleaseAction {
    Create {
        version: Option<String>,
        #[arg(long, default_value = "patch")] release_type: String,
        #[arg(long)] generate_changelog: bool,
    },
    List,
    Tag { version: String, #[arg(long, value_delimiter = ',')] repos: Vec<String> },
}


#[derive(Args)]
pub struct RefactorArgs {
    #[command(subcommand)]
    pub action: Option<RefactorAction>,
}

#[derive(Subcommand)]
pub enum RefactorAction {
    Rename {
        kind: String, from: String, to: String,
        #[arg(long, value_delimiter = ',')] repos: Vec<String>,
        #[arg(long)] dry_run: bool,
    },
    Clean    { target: String },
    Find {
        pattern: String,
        #[arg(long)]          repos:     Option<String>,
        #[arg(long = "type")] find_type: Option<String>,
    },
    Generate { target: String },
}


#[derive(Args)]
pub struct StatsArgs {
    #[command(subcommand)]
    pub action: Option<StatsAction>,
    #[arg(long)] pub repo:      Option<String>,
    #[arg(long)] pub workspace: Option<String>,
    /// Save output to <repo>/.fxg/stats.md
    #[arg(long)] pub save: bool,
}

#[derive(Subcommand)]
pub enum StatsAction {
    Time {
        #[arg(long)] repo:  Option<String>,
        #[arg(long)] since: Option<String>,
    },
    Lines, Languages, Functions, Security,
}

#[derive(Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub action: Option<WorkspaceAction>,
}

#[derive(Subcommand)]
pub enum WorkspaceAction {
    Create { name: String },
    Add    { workspace: String, repos: Vec<String> },
    Remove { name: String },
    List,
}

#[derive(Args)]
pub struct SummaryArgs {
    /// Repository name (defaults to current directory)
    pub repo: Option<String>,
    #[arg(long)] pub workspace: Option<String>,
    /// Print to stdout instead of saving to <repo>/.fxg/summary.md
    #[arg(long)] pub print: bool,
}

#[derive(Args)]
pub struct AdoptArgs {
    /// Folder name inside workspace_root to adopt (omit for interactive TUI)
    pub name: Option<String>,
    /// Adopt all non-git folders without prompting
    #[arg(long)] pub all: bool,
    /// Provider to use (default: from config)
    #[arg(long)] pub provider: Option<String>,
    /// Visibility: private (default) or public
    #[arg(long, default_value = "private")] pub visibility: String,
    /// Initial commit message
    #[arg(long, short = 'm', default_value = "chore: init")] pub message: String,
}

//! Implementations of every non-TUI subcommand (`vulp list/sync/todo/...`).
//! Shared contract (see `docs/cli.md`): `--json` prints machine-readable
//! JSON on stdout; human output otherwise; progress/errors go to stderr.
//! Nothing here ever prompts interactively — a missing argument is an error.
//!
//! Exit codes: 0 ok · 1 execution error · 2 bad usage (clap) ·
//! 3 auth missing/invalid · 4 not found.

mod items;
mod remote;
mod workspace;

use crate::cli::Commands;
use crate::config::AppConfig;
use anyhow::Result;

/// Dispatches one parsed subcommand. Only called for commands that aren't
/// `Config`/`Auth`/the bare TUI (those keep their existing paths in `app.rs`).
pub async fn run(cmd: Commands, json: bool, cfg: &AppConfig) -> Result<()> {
    match cmd {
        Commands::List { dirty, behind, ahead, lang, project } =>
            workspace::list(cfg, json, dirty, behind, ahead, lang, project),
        Commands::Status { repo }   => workspace::status(cfg, json, &repo),
        Commands::Scan              => workspace::scan_cmd(cfg, json),
        Commands::Sync { repos, all, project, action, message } =>
            workspace::sync(cfg, repos, all, project, &action, message),
        Commands::Release { repo, tag, notes } =>
            remote::release(cfg, &repo, &tag, &notes).await,
        Commands::Stats { period }  => workspace::stats(cfg, json, &period),
        Commands::Activity { limit, kind } => workspace::activity(json, limit, kind),

        Commands::Todo { action }    => items::todo(json, action),
        Commands::Project { action } => items::project(cfg, json, action),

        Commands::Pr { action }      => remote::pr(cfg, json, action).await,
        Commands::Issue { action }   => remote::issue(cfg, json, action).await,
        Commands::Repo { action }    => remote::repo_cmd(cfg, json, action).await,
        Commands::Init { name, private, description } =>
            remote::init(cfg, &name, private, &description).await,
        Commands::Accounts           => remote::accounts(cfg, json),
        Commands::Refresh            => remote::refresh(cfg, json).await,

        Commands::Config | Commands::Auth(_) => unreachable!("handled in app.rs"),
    }
}

// ── shared helpers ────────────────────────────────────────────────────────────

/// Prints `msg` to stderr and exits with `code` (1 error / 3 auth / 4 not found).
pub(crate) fn die(code: i32, msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(code);
}

/// Serializes `v` as pretty JSON on stdout.
pub(crate) fn out_json<T: serde::Serialize>(v: &T) {
    match serde_json::to_string_pretty(v) {
        Ok(s) => println!("{s}"),
        Err(e) => die(1, &format!("JSON serialization failed: {e}")),
    }
}

/// Scans the workspace synchronously (labels already applied by the scan).
pub(crate) fn scan_repos(cfg: &AppConfig) -> Vec<crate::status::RepoSummary> {
    match crate::status::scan_workspace(&cfg.workspace_root, &[]) {
        Ok(r) => r,
        Err(e) => die(1, &format!("workspace scan failed: {e:#}")),
    }
}

/// Finds one repo by (case-insensitive) display name or directory basename;
/// exits 4 when nothing matches.
pub(crate) fn find_repo(cfg: &AppConfig, name: &str) -> crate::status::RepoSummary {
    let repos = scan_repos(cfg);
    let needle = name.to_lowercase();
    let hit = repos.iter().find(|r| {
        r.name.to_lowercase() == needle
            || r.path.file_name().map(|b| b.to_string_lossy().to_lowercase() == needle).unwrap_or(false)
    });
    match hit {
        Some(r) => r.clone(),
        None => die(4, &format!("repo '{name}' not found in {}", cfg.workspace_root.display())),
    }
}

/// The default (or first) configured account plus its provider client.
/// Exits 3 when no account or token is usable.
pub(crate) fn default_provider(cfg: &AppConfig) -> (crate::auth::Account, Box<dyn crate::providers::Provider>) {
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    let Some(acc) = accounts.iter().find(|a| a.is_default).or(accounts.first()).cloned() else {
        die(3, "no accounts configured — run: vulp auth import");
    };
    match crate::providers::make_provider(&acc) {
        Ok(p) => (acc, p),
        Err(e) => die(3, &format!("provider auth failed: {e:#}")),
    }
}

/// Logs one vulp-performed action to `~/.vulp/activity.log` (same log the
/// TUI writes, so CLI actions show up in the Overview feed too).
pub(crate) fn log_activity(kind: crate::activity::ActivityKind, repo: &str, detail: String) {
    crate::activity::log(&crate::activity::ActivityEntry {
        ts: chrono::Utc::now().timestamp(),
        kind,
        repo: repo.to_string(),
        detail,
    });
}

//! Top-level command dispatch: loads config, then routes to the TUI hub,
//! one of the setup subcommands, or the non-interactive CLI surface.

use crate::cli::{Cli, Commands};
use anyhow::Result;

/// Dispatches a parsed `Cli` invocation. With no subcommand, this launches
/// straight into the TUI hub — `vulp` is meant to be used interactively, so
/// the bare command is the primary entry point rather than a help screen.
/// Every other subcommand (besides `config`/`auth`) goes to `cli_cmds`,
/// the machine-friendly non-interactive surface.
pub async fn run(cli: Cli) -> Result<()> {
    if cli.debug { crate::debug::enable(); }
    let cfg = crate::config::load_config()?;

    match cli.command {
        // ── no subcommand → TUI hub ──
        None => crate::status::run(&cfg).await,

        // ── setup, needed before the TUI has anything to show ──
        Some(Commands::Config)     => crate::config::run(cfg).await,
        Some(Commands::Auth(args)) => crate::auth::run(args, &cfg).await,

        // ── everything else: non-interactive CLI ──
        Some(cmd) => {
            let guard = crate::cli_cmds::Guard { dry_run: cli.dry_run, force: cli.force };
            crate::cli_cmds::run(cmd, cli.json, guard, &cfg).await
        }
    }
}

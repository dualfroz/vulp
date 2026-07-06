//! `vulp` — a TUI multi-repo git workflow manager. Entry point: install the
//! Ctrl-Z suspend handler, migrate any legacy on-disk config, parse CLI
//! args, and hand off to `app::run`, which dispatches to either the TUI hub
//! (no subcommand) or one of the setup subcommands (`config`, `auth`).

mod activity;
mod app;
mod auth;
mod cache;
mod cli;
mod cli_cmds;
mod codeintel;
mod config;
mod debug;
mod detail;
mod hub;
mod labels;
mod legacy;
mod profile;
mod projects;
mod providers;
mod release_info;
mod status;
mod todos;
mod tui;
mod util;
mod workspace;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    tui::install_suspend_handler();
    if let Ok(Some(old)) = config::migrate_legacy_config() {
        eprintln!("vulp: imported existing config from {}", old.display());
    }
    let cli = cli::Cli::parse();
    if let Err(e) = app::run(cli).await {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
    Ok(())
}

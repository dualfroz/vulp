mod activity;
mod app;
mod auth;
mod cache;
mod cli;
mod cli_cmds;
mod codeintel;
mod config;
mod debug;
mod deploy;
mod detail;
mod digest;
mod hub;
mod labels;
mod legacy;
mod license;
mod profile;
mod projects;
mod providers;
mod release_info;
mod secrets;
mod stats;
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
    let cli = cli::Cli::parse();
    if let Err(e) = app::run(cli).await {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
    Ok(())
}

mod app;
mod auth;
mod cache;
mod cli;
mod config;
mod debug;
mod detail;
mod hub;
mod labels;
mod legacy;
mod projects;
mod providers;
mod status;
mod todos;
mod tui;
mod util;
mod workspace;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    if let Err(e) = app::run(cli).await {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
    Ok(())
}

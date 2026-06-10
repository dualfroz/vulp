mod adopt;
mod app;
mod auth;
mod cli;
mod config;
mod init;
mod providers;
mod refactor;
mod release;
mod stats;
mod summary;
mod status;
mod sync;
mod tui;
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

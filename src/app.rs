use crate::cli::{Cli, Commands};
use anyhow::Result;

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Config => {
            let cfg = crate::config::load_config()?;
            crate::config::run(cfg).await
        }
        Commands::Auth(args) => {
            let cfg = crate::config::load_config()?;
            crate::auth::run(args, &cfg).await
        }
        Commands::Init(args) => {
            let cfg = crate::config::load_config()?;
            crate::init::run(args, &cfg).await
        }
        Commands::Status(args) => {
            let cfg = crate::config::load_config()?;
            crate::status::run(args, &cfg).await
        }
        Commands::Sync(args) => {
            let cfg = crate::config::load_config()?;
            crate::sync::run(args, &cfg).await
        }
        Commands::Release(args) => {
            let cfg = crate::config::load_config()?;
            crate::release::run(args, &cfg).await
        }
        Commands::Refactor(args) => {
            let cfg = crate::config::load_config()?;
            crate::refactor::run(args, &cfg).await
        }
        Commands::Stats(args) => {
            let cfg = crate::config::load_config()?;
            crate::stats::run(args, &cfg).await
        }
        Commands::Workspace(args) => {
            let cfg = crate::config::load_config()?;
            crate::workspace::run(args, &cfg).await
        }
        Commands::Summary(args) => {
            let cfg = crate::config::load_config()?;
            crate::summary::run(args, &cfg).await
        }
        Commands::Adopt(args) => {
            let cfg = crate::config::load_config()?;
            crate::adopt::run(args, &cfg).await
        }
    }
}

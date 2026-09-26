use crate::cli::{Cli, Commands};
use anyhow::Result;

pub async fn run(cli: Cli) -> Result<()> {
    if cli.debug { crate::debug::enable(); }
    let cfg = crate::config::load_config()?;

    match cli.command {
        None => crate::status::run(&cfg).await,

        Some(Commands::Config)     => crate::config::run(cfg).await,
        Some(Commands::Auth(args)) => crate::auth::run(args, &cfg).await,

        Some(cmd) => {
            let guard = crate::cli_cmds::Guard { dry_run: cli.dry_run, force: cli.force };
            crate::cli_cmds::run(cmd, cli.json, guard, &cfg).await
        }
    }
}

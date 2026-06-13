use crate::cli::{Cli, Commands};
use anyhow::Result;

const NOT_YET: &str = "\
  This CLI command is not yet implemented.
  Launch the TUI with: fxg
";

pub async fn run(cli: Cli) -> Result<()> {
    if cli.debug { crate::debug::enable(); }
    let cfg = crate::config::load_config()?;

    match cli.command {
        // ── no subcommand → TUI hub ──
        None => crate::status::run(
            crate::cli::StatusArgs { repo: None, workspace: None, filter: None },
            &cfg,
        ).await,

        // ── always-on: config + auth ──
        Some(Commands::Config)       => crate::config::run(cfg).await,
        Some(Commands::Auth(args))   => crate::auth::run(args, &cfg).await,

        // ── everything else: stub ──
        Some(Commands::Init(_))      => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Status(_))    => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Sync(_))      => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Release(_))   => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Refactor(_))  => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Stats(_))     => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Workspace(_)) => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Summary(_))   => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Adopt(_))     => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Manage(_))    => { eprintln!("{}", NOT_YET); Ok(()) }
        Some(Commands::Util(_))      => { eprintln!("{}", NOT_YET); Ok(()) }
    }
}

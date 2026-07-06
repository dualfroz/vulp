//! `vulp digest` — build/show/send the weekly digest of own activity.

use super::{die, out_json};
use crate::cli::DigestCmd;
use crate::config::AppConfig;
use anyhow::Result;

pub async fn digest(cfg: &AppConfig, guard: super::Guard, json: bool, action: Option<DigestCmd>) -> Result<()> {
    match action {
        Some(DigestCmd::Webhook { url }) => {
            if !url.starts_with("https://discord.com/api/webhooks/")
                && !url.starts_with("https://discordapp.com/api/webhooks/")
            {
                die(2, "expected a Discord webhook URL (https://discord.com/api/webhooks/...)");
            }
            crate::auth::store_token(crate::digest::WEBHOOK_ID, &url)?;
            println!("✓ webhook stored");
            Ok(())
        }
        Some(DigestCmd::Send { target }) => {
            if target != "discord" {
                die(2, "only 'discord' is supported as a send target");
            }
            let d = build(cfg).await;
            let path = crate::digest::archive(&d)?;
            if guard.dry_run {
                println!("DRY RUN: would POST digest embed to the saved Discord webhook");
                println!("(archived: {})", path.display());
                return Ok(());
            }
            let webhook = crate::auth::get_token(crate::digest::WEBHOOK_ID)
                .map_err(|_| anyhow::anyhow!("no webhook stored — run: vulp digest webhook <url>"))?;
            let embed = crate::digest::render_discord_embed(&d);
            crate::digest::send_discord(&embed, &webhook).await?;
            println!("✓ sent to Discord (archived: {})", path.display());
            Ok(())
        }
        None => {
            let d = build(cfg).await;
            let path = crate::digest::archive(&d)?;
            if json {
                #[derive(serde::Serialize)]
                struct Out { plaintext: String, archived: String }
                out_json(&Out {
                    plaintext: crate::digest::render_plaintext(&d),
                    archived: path.display().to_string(),
                });
            } else {
                print!("{}", crate::digest::render_plaintext(&d));
                eprintln!("(archived: {})", path.display());
            }
            Ok(())
        }
    }
}

/// Gathers workspace state and builds the digest — shared by `show` and
/// `send` so both report on exactly the same numbers.
async fn build(cfg: &AppConfig) -> crate::digest::WeeklyDigest {
    let repos = super::scan_repos(cfg);
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    let matcher = crate::stats::OwnEmailMatcher::resolve(cfg, &accounts);

    let mut owners: std::collections::HashSet<String> =
        repos.iter().filter_map(|r| r.owner.clone()).collect();
    let default_account = accounts.iter().find(|a| a.is_default).or(accounts.first());
    if let Some(acc) = default_account { owners.insert(acc.username.clone()); }
    let scope = owners.iter().map(|o| format!("user:{o}")).collect::<Vec<_>>().join(" ");

    let provider = default_account.and_then(|acc| crate::providers::make_provider(acc).ok());
    crate::digest::build(&repos, &matcher, provider.as_deref(), &scope).await
}

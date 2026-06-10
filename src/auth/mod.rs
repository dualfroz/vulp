pub mod ui;

use crate::cli::{AuthAction, AuthArgs};
use crate::config::AppConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub provider:   String,
    pub alias:      String,
    pub username:   String,
    pub email:      String,
    pub base_url:   Option<String>,
    pub is_default: bool,
}

impl Account {
    pub fn id(&self) -> String {
        format!("{}/{}", self.provider, self.alias)
    }
}

// ── persistence ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct AccountsFile {
    accounts: Vec<Account>,
}

pub fn accounts_path(cfg: &AppConfig) -> PathBuf {
    cfg.data_dir.join("accounts.toml")
}

pub fn load_accounts(cfg: &AppConfig) -> Result<Vec<Account>> {
    let path = accounts_path(cfg);
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        let file: AccountsFile = toml::from_str(&raw)?;
        Ok(file.accounts)
    } else {
        Ok(vec![])
    }
}

pub fn save_accounts(accounts: &[Account], cfg: &AppConfig) -> Result<()> {
    let path = accounts_path(cfg);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = AccountsFile { accounts: accounts.to_vec() };
    std::fs::write(path, toml::to_string_pretty(&file)?)?;
    Ok(())
}

// ── keyring ───────────────────────────────────────────────────────────────────

const SERVICE: &str = "fxg";

pub fn store_token(id: &str, token: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, id)?.set_password(token)?;
    Ok(())
}

pub fn get_token(id: &str) -> Result<String> {
    Ok(keyring::Entry::new(SERVICE, id)?.get_password()?)
}

pub fn delete_token(id: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, id)?.delete_credential()?;
    Ok(())
}

// ── mutation helpers ──────────────────────────────────────────────────────────

fn add_account(
    accounts: &mut Vec<Account>,
    account: Account,
    token: &str,
    cfg: &AppConfig,
) -> Result<()> {
    let id = account.id();
    accounts.retain(|a| a.id() != id);
    accounts.push(account);
    save_accounts(accounts, cfg)?;
    store_token(&id, token)?;
    Ok(())
}

fn remove_account(accounts: &mut Vec<Account>, id: &str, cfg: &AppConfig) -> Result<()> {
    accounts.retain(|a| a.id() != id);
    save_accounts(accounts, cfg)?;
    let _ = delete_token(id);
    Ok(())
}

fn switch_default(accounts: &mut Vec<Account>, id: &str, cfg: &AppConfig) -> Result<()> {
    let exists = accounts.iter().any(|a| a.id() == id);
    anyhow::ensure!(exists, "account '{}' not found", id);
    for a in accounts.iter_mut() {
        a.is_default = a.id() == id;
    }
    save_accounts(accounts, cfg)?;
    Ok(())
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(args: AuthArgs, cfg: &AppConfig) -> Result<()> {
    let mut accounts = load_accounts(cfg)?;

    match args.action {
        None => {
            let updated = ui::run_auth_tui(accounts, cfg)?;
            save_accounts(&updated, cfg)?;
        }
        Some(AuthAction::Add { provider, token, alias, url }) => {
            let provider = provider.unwrap_or_else(|| cfg.default_provider.clone());
            let alias    = alias.unwrap_or_else(|| "personal".into());
            let token    = token.ok_or_else(|| anyhow::anyhow!("--token is required"))?;
            let account  = Account {
                provider, alias, username: cfg.username.clone(),
                email: cfg.email.clone(), base_url: url, is_default: accounts.is_empty(),
            };
            add_account(&mut accounts, account, &token, cfg)?;
            println!("✓ Account added");
        }
        Some(AuthAction::Remove { account }) => {
            remove_account(&mut accounts, &account, cfg)?;
            println!("✓ Account removed");
        }
        Some(AuthAction::Switch { account }) => {
            switch_default(&mut accounts, &account, cfg)?;
            println!("✓ Default switched to {}", account);
        }
        Some(AuthAction::Test { account }) => {
            let id = account.unwrap_or_else(|| {
                accounts.iter().find(|a| a.is_default).map(|a| a.id()).unwrap_or_default()
            });
            let token = get_token(&id)?;
            println!("Token for {}: {}…", id, &token[..token.len().min(8)]);
        }
    }
    Ok(())
}

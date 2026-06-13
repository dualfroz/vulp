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

// ── token storage (keyring with file fallback) ────────────────────────────────

const SERVICE: &str = "fxg";

fn tokens_file() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~"))
        .join(".fxg")
        .join(".tokens")
}

fn file_read_token(id: &str) -> Option<String> {
    let raw = std::fs::read_to_string(tokens_file()).ok()?;
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == id {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn file_write_token(id: &str, token: &str) -> Result<()> {
    let path = tokens_file();
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|l| !l.starts_with(&format!("{}=", id)))
        .map(|l| l.to_string())
        .collect();
    lines.push(format!("{}={}", id, token));

    std::fs::write(&path, lines.join("\n") + "\n")?;

    // 600 permissions — owner read/write only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn file_delete_token(id: &str) -> Result<()> {
    let path = tokens_file();
    if !path.exists() { return Ok(()); }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = existing
        .lines()
        .filter(|l| !l.starts_with(&format!("{}=", id)))
        .collect();
    std::fs::write(&path, lines.join("\n") + "\n")?;
    Ok(())
}

pub fn store_token(id: &str, token: &str) -> Result<()> {
    // File is always the source of truth — keyring is best-effort only
    file_write_token(id, token)?;
    if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
        let _ = entry.set_password(token);
    }
    Ok(())
}

pub fn get_token(id: &str) -> Result<String> {
    // Read from file first (reliable); keyring as fallback for tokens stored before this fix
    if let Some(t) = file_read_token(id) {
        return Ok(t);
    }
    if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
        if let Ok(t) = entry.get_password() {
            // Migrate to file so future reads don't need keyring
            let _ = file_write_token(id, &t);
            return Ok(t);
        }
    }
    Err(anyhow::anyhow!(
        "no token for '{}' — run: fxg auth import  (or fxg auth add --token <PAT>)", id
    ))
}

pub fn delete_token(id: &str) -> Result<()> {
    file_delete_token(id)?;
    if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
        let _ = entry.delete_credential();
    }
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

// ── gh CLI import ─────────────────────────────────────────────────────────────

fn gh_output(args: &[&str]) -> Option<String> {
    std::process::Command::new("gh")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "null")
}

fn import_from_gh(accounts: &mut Vec<Account>, cfg: &AppConfig) -> Result<()> {
    let token = gh_output(&["auth", "token"])
        .ok_or_else(|| anyhow::anyhow!(
            "gh auth token failed — run: gh auth login"
        ))?;

    let username = gh_output(&["api", "user", "--jq", ".login"])
        .unwrap_or_else(|| cfg.username.clone());

    let email = gh_output(&["api", "user", "--jq", ".email"])
        .unwrap_or_else(|| cfg.email.clone());

    let account = Account {
        provider:   "github".into(),
        alias:      "personal".into(),
        username:   username.clone(),
        email,
        base_url:   None,
        is_default: true,
    };

    // mark all others as non-default
    for a in accounts.iter_mut() { a.is_default = false; }

    add_account(accounts, account, &token, cfg)?;
    println!("✓ Imported GitHub account for '{}'", username);
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
        Some(AuthAction::Import) => {
            import_from_gh(&mut accounts, cfg)?;
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

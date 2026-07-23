//! Provider-account management: storing account metadata in
//! `~/.vulp/accounts.toml` and the matching access tokens via the OS keyring
//! (with an encrypted-permission file fallback), plus the `vulp auth` CLI
//! subcommand and importing credentials straight from the `gh` CLI.

pub mod ui;

use crate::cli::{AuthAction, AuthArgs};
use crate::config::AppConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One configured provider account. `alias` distinguishes multiple accounts
/// on the same provider (e.g. a personal and a work GitHub account);
/// `id()` (provider/alias) is the key used everywhere else to reference one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub provider:   String,
    pub alias:      String,
    pub username:   String,
    pub email:      String,
    pub base_url:   Option<String>,
    pub is_default: bool,
    /// Date (YYYY-MM-DD) this account was added to vulp. Absent on accounts
    /// saved before the field existed.
    #[serde(default)]
    pub added_at:   Option<String>,
}

/// Today as YYYY-MM-DD, for stamping `Account::added_at`.
fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

impl Account {
    /// The stable key used to look up this account's token and to reference
    /// it from CLI args (`provider/alias`).
    pub fn id(&self) -> String {
        format!("{}/{}", self.provider, self.alias)
    }
}

// ── persistence ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct AccountsFile {
    accounts: Vec<Account>,
}

/// Path to the accounts file, `<data_dir>/accounts.toml`.
pub fn accounts_path(cfg: &AppConfig) -> PathBuf {
    cfg.data_dir.join("accounts.toml")
}

/// Loads all configured accounts (empty if none have been added yet).
pub fn load_accounts(cfg: &AppConfig) -> Result<Vec<Account>> {
    load_accounts_path(&cfg.data_dir)
}

/// Like `load_accounts`, but takes the data directory directly — used by
/// callers (like the detail view) that only have a `PathBuf` on hand, not a
/// full `AppConfig`.
pub fn load_accounts_path(data_dir: &std::path::Path) -> Result<Vec<Account>> {
    let path = data_dir.join("accounts.toml");
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        let file: AccountsFile = toml::from_str(&raw)?;
        Ok(file.accounts)
    } else {
        Ok(vec![])
    }
}

/// Overwrites the accounts file with `accounts`.
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
//
// The file at `.tokens` (mode 0600) is the actual source of truth — the OS
// keyring is written to as a convenience but never required to be present,
// since headless/CI environments and some Linux setups don't have a working
// keyring backend at all.

const SERVICE: &str = "vulp";

fn tokens_file() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~"))
        .join(".vulp")
        .join(".tokens")
}

/// Reads one token by id from the plaintext-but-0600 tokens file.
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

/// Writes or replaces one token's line in the tokens file, then restricts
/// the file to owner read/write (0600) so other local users can't read it.
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
    let contents = lines.join("\n") + "\n";

    // On Unix, open with mode 0600 from the start (`OpenOptions::mode` sets
    // the mode atomically at creation) instead of `fs::write` + a follow-up
    // `set_permissions` — the latter leaves a real window, between the file
    // hitting disk at the umask-derived default (typically world-readable
    // 0644) and the chmod landing, where another local user could read the
    // plaintext token.
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(contents.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, contents)?;
    }
    Ok(())
}

/// Removes one token's line from the tokens file, if present.
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

/// Stores a token for `id` in the tokens file, and best-effort mirrors it
/// into the OS keyring.
pub fn store_token(id: &str, token: &str) -> Result<()> {
    // File is always the source of truth — keyring is best-effort only
    file_write_token(id, token)?;
    if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
        let _ = entry.set_password(token);
    }
    Ok(())
}

/// Retrieves the token for `id`, checking the tokens file first and falling
/// back to the OS keyring (migrating it into the file on a keyring hit, so
/// the fallback path is only ever needed once per token).
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
    // Last resort for GitHub accounts: borrow the gh CLI's token. Not
    // persisted — gh rotates oauth tokens, so ask it fresh each time.
    if id.starts_with("github/") {
        if let Ok(out) = std::process::Command::new("gh").args(["auth", "token"]).output() {
            let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && !t.is_empty() {
                return Ok(t);
            }
        }
    }
    Err(anyhow::anyhow!(
        "no token for '{}' — run: vulp auth import  (or vulp auth add --token <PAT>)", id
    ))
}

/// Removes the token for `id` from both the tokens file and the OS keyring.
pub fn delete_token(id: &str) -> Result<()> {
    file_delete_token(id)?;
    if let Ok(entry) = keyring::Entry::new(SERVICE, id) {
        let _ = entry.delete_credential();
    }
    Ok(())
}

// ── mutation helpers ──────────────────────────────────────────────────────────

/// Inserts or replaces `account` (matched by id) and stores its token,
/// persisting both in one call so the two never drift out of sync.
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

/// Removes the account matching `id` and deletes its stored token.
fn remove_account(accounts: &mut Vec<Account>, id: &str, cfg: &AppConfig) -> Result<()> {
    accounts.retain(|a| a.id() != id);
    save_accounts(accounts, cfg)?;
    let _ = delete_token(id);
    Ok(())
}

/// Marks the account matching `id` as the default and every other account
/// as non-default. Errors if `id` doesn't match any configured account.
fn switch_default(accounts: &mut [Account], id: &str, cfg: &AppConfig) -> Result<()> {
    let exists = accounts.iter().any(|a| a.id() == id);
    anyhow::ensure!(exists, "account '{}' not found", id);
    for a in accounts.iter_mut() {
        a.is_default = a.id() == id;
    }
    save_accounts(accounts, cfg)?;
    Ok(())
}

// ── gh CLI import ─────────────────────────────────────────────────────────────

/// Runs `gh <args>`, returning trimmed stdout on success — `None` on any
/// failure or empty/"null" output, so callers can use `?`/`ok_or_else`
/// without separately checking exit status.
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

/// Imports a GitHub account using credentials already authenticated via the
/// `gh` CLI — no token entry needed from the user. Becomes the new default
/// account.
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
        added_at:   Some(today()),
    };

    // mark all others as non-default
    for a in accounts.iter_mut() { a.is_default = false; }

    add_account(accounts, account, &token, cfg)?;
    println!("✓ Imported GitHub account for '{}'", username);
    Ok(())
}

// ── entry point ───────────────────────────────────────────────────────────────

/// Entry point for `vulp auth`. With no subcommand, opens the interactive
/// account-management TUI; otherwise runs the requested action directly and
/// prints a one-line confirmation.
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
                added_at: Some(today()),
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
            // Byte-safe truncation: a raw `&token[..8]` would panic if byte 8
            // fell mid-character for any non-ASCII token (tokens are ASCII
            // today, but nothing enforces that on `auth add --token`).
            let preview: String = token.chars().take(8).collect();
            println!("Token for {}: {}…", id, preview);
        }
    }
    Ok(())
}

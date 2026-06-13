use crate::config::AppConfig;
use anyhow::Result;
use inquire::Select;

// Top-level menu entries
const MAIN: &[(&str, &str)] = &[
    ("status",    "View all repositories"),
    ("sync",      "Pull / commit / push / branch / PR"),
    ("manage",    "Rename · visibility · topics · delete · archive · categorize"),
    ("util",      "Unified log · secret scan · new doc"),
    ("release",   "Create tags & GitHub releases"),
    ("init",      "Initialize new repository"),
    ("adopt",     "Import local folder → git + remote"),
    ("auth",      "Manage provider accounts"),
    ("config",    "Settings"),
];

const MANAGE: &[(&str, &str)] = &[
    ("categorize", "Organize repos into categories (local + GitHub topics)"),
    ("visibility", "Change visibility (private / public)"),
    ("topics",     "Set GitHub topics"),
    ("rename",     "Rename repo on remote + locally"),
    ("delete",     "Delete repo (remote + optional local)"),
    ("archive",    "Archive repo (read-only on remote)"),
];

const UTIL: &[(&str, &str)] = &[
    ("log",          "Unified chronological log across all repos"),
    ("find-secrets", "Scan git history for leaked credentials"),
    ("new-doc",      "Create CHANGELOG / README / TODO in a repo"),
];

fn make_choices(items: &[(&str, &str)]) -> Vec<String> {
    let mut v: Vec<String> = items.iter().enumerate()
        .map(|(i, (cmd, desc))| format!(" {}  {:<14} {}", i + 1, cmd, desc))
        .collect();
    v.push(" 0  back / exit".to_string());
    v
}

fn pick(title: &str, items: &[(&str, &str)]) -> Result<Option<usize>> {
    let choices = make_choices(items);
    let page = choices.len();
    match Select::new(title, choices.iter().map(|s| s.as_str()).collect())
        .with_page_size(page)
        .prompt_skippable()?
    {
        None    => Ok(None),
        Some(s) => {
            // Parse the leading number
            let num: usize = s.trim_start().chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            Ok(Some(num))
        }
    }
}

pub async fn run(cfg: &AppConfig) -> Result<()> {
    loop {
        println!(); // breathing room after alternate-screen exit
        let choice = pick("fxg — Main Menu", MAIN)?;

        match choice {
            None | Some(0) => break,

            Some(1) => {
                crate::status::run(
                    crate::cli::StatusArgs { repo: None, workspace: None, filter: None },
                    cfg,
                ).await?;
            }

            Some(2) => {
                crate::legacy::sync::run(
                    crate::cli::SyncArgs { action: None },
                    cfg,
                ).await?;
            }

            Some(3) => {
                // Manage sub-menu
                loop {
                    let sub = pick("fxg manage", MANAGE)?;
                    match sub {
                        None | Some(0) => break,
                        Some(1) => { crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Categorize) }, cfg).await?; }
                        Some(2) => { crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Visibility { pub_repo: None }) }, cfg).await?; }
                        Some(3) => { crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Topics { pub_repo: None, set: vec![] }) }, cfg).await?; }
                        Some(4) => {
                            if let Ok(name) = inquire::Text::new("Repo name to rename:").prompt() {
                                if let Ok(new) = inquire::Text::new("New name:").prompt() {
                                    crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Rename { pub_repo: name, new_name: new }) }, cfg).await?;
                                }
                            }
                        }
                        Some(5) => {
                            if let Ok(name) = inquire::Text::new("Repo name to delete:").prompt() {
                                crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Delete { pub_repo: name, local: false }) }, cfg).await?;
                            }
                        }
                        Some(6) => {
                            if let Ok(name) = inquire::Text::new("Repo name to archive:").prompt() {
                                crate::legacy::manage::run(crate::cli::ManageArgs { action: Some(crate::cli::ManageAction::Archive { pub_repo: name }) }, cfg).await?;
                            }
                        }
                        _ => {}
                    }
                }
            }

            Some(4) => {
                // Util sub-menu
                loop {
                    let sub = pick("fxg util", UTIL)?;
                    match sub {
                        None | Some(0) => break,
                        Some(1) => { crate::legacy::util::run(crate::cli::UtilArgs { action: Some(crate::cli::UtilAction::Log { limit: 30 }) }, cfg).await?; }
                        Some(2) => { crate::legacy::util::run(crate::cli::UtilArgs { action: Some(crate::cli::UtilAction::FindSecrets { repos: vec![] }) }, cfg).await?; }
                        Some(3) => { crate::legacy::util::run(crate::cli::UtilArgs { action: Some(crate::cli::UtilAction::NewDoc { pub_repo: None, doc_type: None }) }, cfg).await?; }
                        _ => {}
                    }
                }
            }

            Some(5) => { crate::legacy::release::run(crate::cli::ReleaseArgs { action: None }, cfg).await?; }
            Some(6) => { crate::legacy::init::run(crate::cli::InitArgs { name: None, provider: None, tags: vec![], status: "private".into(), branch: "main".into(), batch: None }, cfg).await?; }
            Some(7) => { crate::legacy::adopt::run(crate::cli::AdoptArgs { name: None, all: false, provider: None, visibility: "private".into(), message: "chore: init".into() }, cfg).await?; }
            Some(8) => { crate::auth::run(crate::cli::AuthArgs { action: None }, cfg).await?; }
            Some(9) => { crate::config::run(cfg.clone()).await?; }
            _ => {}
        }
    }

    Ok(())
}

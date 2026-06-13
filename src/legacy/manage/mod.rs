use crate::auth::load_accounts;
use crate::cli::{ManageAction, ManageArgs};
use crate::config::AppConfig;
use crate::providers::{default_account, make_provider};
use crate::status::scan_workspace;
use anyhow::Result;
use inquire::{Confirm, Select, Text};
use std::path::PathBuf;

pub async fn run(args: ManageArgs, cfg: &AppConfig) -> Result<()> {
    let accounts = load_accounts(cfg)?;
    let account = default_account(&accounts, None)
        .ok_or_else(|| anyhow::anyhow!(
            "No accounts configured. Run: fxg auth add --provider github --token <PAT>"
        ))?;
    let provider = make_provider(account)?;

    match args.action {
        None => {
            println!("Usage: fxg manage <subcommand>");
            println!();
            println!("  delete   <repo> [--local]   Delete remote (+ optionally local) repo");
            println!("  rename   <repo> <new-name>  Rename repo on remote + locally");
            println!("  visibility [<repo>]         Change visibility (picker)");
            println!("  topics   [<repo>]           Set topics (picker)");
            println!("  archive  <repo>             Archive repo (read-only)");
        }

        Some(ManageAction::Delete { pub_repo, local }) => {
            let confirmed = Confirm::new(&format!(
                "Delete remote repo '{}'? This cannot be undone.", pub_repo
            ))
            .with_default(false)
            .prompt()?;
            if !confirmed {
                println!("Cancelled.");
                return Ok(());
            }

            provider.delete_repo(&pub_repo).await?;
            println!("✓ Remote repo '{}' deleted", pub_repo);

            if local {
                let path = cfg.workspace_root.join(&pub_repo);
                if path.exists() {
                    let confirmed2 = Confirm::new(&format!(
                        "Also delete local directory '{}'?", path.display()
                    ))
                    .with_default(false)
                    .prompt()?;
                    if confirmed2 {
                        std::fs::remove_dir_all(&path)?;
                        println!("✓ Local directory removed");
                    }
                } else {
                    println!("  (local directory not found, skipping)");
                }
            }
        }

        Some(ManageAction::Rename { pub_repo, new_name }) => {
            let confirmed = Confirm::new(&format!(
                "Rename '{}' → '{}'?", pub_repo, new_name
            ))
            .with_default(true)
            .prompt()?;
            if !confirmed {
                println!("Cancelled.");
                return Ok(());
            }

            provider.rename_repo(&pub_repo, &new_name).await?;
            println!("✓ Remote repo renamed to '{}'", new_name);

            // Rename local dir if it exists under workspace_root or any group
            let candidates: Vec<std::path::PathBuf> = {
                let mut v = vec![cfg.workspace_root.join(&pub_repo)];
                for g in &cfg.workspace_groups {
                    v.push(cfg.workspace_root.join(g).join(&pub_repo));
                }
                v
            };
            for old_path in candidates {
                if old_path.exists() {
                    let new_path = old_path.parent().unwrap().join(&new_name);
                    std::fs::rename(&old_path, &new_path)?;
                    println!("✓ Local directory renamed: {} → {}", old_path.display(), new_path.display());
                    break;
                }
            }
        }

        Some(ManageAction::Visibility { pub_repo }) => {
            let repo = resolve_repo(pub_repo, cfg)?;
            let choice = Select::new(
                &format!("New visibility for '{}':", repo),
                vec!["private", "public"],
            )
            .with_help_message("Use ↑/↓ to select, Enter to confirm")
            .prompt()?;
            let private = choice == "private";
            provider.update_visibility(&repo, private).await?;
            println!("✓ Repo '{}' is now {}", repo, choice);
        }

        Some(ManageAction::Topics { pub_repo, set }) => {
            let repo = resolve_repo(pub_repo, cfg)?;
            let topics: Vec<String> = if set.is_empty() {
                let raw = Text::new(&format!("Topics for '{}' (comma-separated):", repo))
                    .with_placeholder("rust, cli, tools")
                    .with_default("")
                    .prompt()?;
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                set
            };
            provider.set_topics(&repo, &topics).await?;
            if topics.is_empty() {
                println!("✓ Topics cleared for '{}'", repo);
            } else {
                println!("✓ Topics set for '{}': {}", repo, topics.join(", "));
            }
        }

        Some(ManageAction::Archive { pub_repo }) => {
            let confirmed = Confirm::new(&format!(
                "Archive '{}' on remote? It will become read-only.", pub_repo
            ))
            .with_default(false)
            .prompt()?;
            if !confirmed {
                println!("Cancelled.");
                return Ok(());
            }
            provider.archive_repo(&pub_repo).await?;
            println!("✓ Repo '{}' archived (read-only)", pub_repo);
        }

        Some(ManageAction::Categorize) => {
            // provider is optional for categorize — topics need auth, moves don't
            drop(provider);
            return run_categorize(cfg).await;
        }
    }

    Ok(())
}

fn resolve_repo(name: Option<String>, cfg: &AppConfig) -> Result<String> {
    if let Some(n) = name {
        return Ok(n);
    }
    let repos = scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;
    if repos.is_empty() {
        anyhow::bail!("No repos found in workspace. Check workspace_root in config.");
    }
    let names: Vec<String> = repos.iter().map(|r| r.name.clone()).collect();
    Ok(Select::new("Select repository:", names)
        .with_help_message("Use ↑/↓ to select, Enter to confirm")
        .prompt()?)
}

// ── categorize ────────────────────────────────────────────────────────────────

const CATEGORIES: &[&str] = &[
    "webdev",
    "software",
    "gamedev",
    "minecraft",
    "scpsl",
    "pterodactyl",
    "tools",
    "libs",
    "bots",
    "experiments",
];

const PROJECT_TYPES: &[&str] = &[
    "fullstack",
    "frontend",
    "backend",
    "plugin",
    "mod",
    "cli",
    "library",
    "bot",
    "game",
    "api",
    "script",
    "template",
];

const TECHNOLOGIES: &[&str] = &[
    "nodejs",
    "typescript",
    "rust",
    "java",
    "csharp",
    "go",
    "python",
    "lua",
    "php",
    "mixed",
    "other",
];

struct RepoPlan {
    folder_name: String,
    current_path: PathBuf,
    target_path: Option<PathBuf>,
    topics: Vec<String>,
}

/// Collect all existing project-group subfolder names under workspace_root/category.
fn existing_groups(workspace_root: &std::path::Path, category: &str) -> Vec<String> {
    let dir = workspace_root.join(category);
    std::fs::read_dir(&dir).ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && !e.path().join(".git").exists())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

async fn run_categorize(cfg: &AppConfig) -> Result<()> {
    let repos = scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;
    if repos.is_empty() {
        println!("No repos found. Check workspace_root in fxg config.");
        return Ok(());
    }

    let total = repos.len();
    println!("\n══════════════════════════════════════════════════════");
    println!("  fxg categorize — {} repos", total);
    println!("  Structure: workspace/category/project/repo");
    println!("  Skip any repo with Esc on the first prompt");
    println!("══════════════════════════════════════════════════════\n");

    let mut plans: Vec<RepoPlan> = Vec::new();

    for (idx, repo) in repos.iter().enumerate() {
        let folder_name = repo.path
            .file_name().unwrap_or_default()
            .to_string_lossy().to_string();

        println!("─────────────────────────────────────────────────────");
        println!("[{}/{}]  {}",
            idx + 1, total,
            repo.path.strip_prefix(&cfg.workspace_root)
                .unwrap_or(&repo.path)
                .display());
        println!("  branch: {}  dirty: {}  last commit: {}",
            repo.branch,
            if repo.is_dirty { "yes" } else { "no" },
            repo.last_commit_age);

        // Step 1 — category (Esc = skip this repo)
        let category = match Select::new("  Category:", CATEGORIES.to_vec())
            .with_page_size(CATEGORIES.len())
            .with_help_message("Esc to skip this repo")
            .prompt_skippable()?
        {
            Some(c) => c,
            None    => { println!("  skipped\n"); continue; }
        };

        // Step 2 — project group (blank = standalone, no subfolder)
        let mut existing = existing_groups(&cfg.workspace_root, category);
        existing.sort();
        existing.dedup();

        let project_group: String = if existing.is_empty() {
            Text::new("  Project group (blank = standalone):")
                .with_help_message("e.g. 'vulcode', 'platformaHostingowa' — groups repos under one project")
                .with_default("")
                .prompt()?
                .trim()
                .to_string()
        } else {
            // Show existing groups + option to type a new one
            let mut choices: Vec<String> = existing.clone();
            choices.push("[ new group... ]".into());
            choices.push("[ standalone — no group ]".into());

            let picked = Select::new("  Project group:", choices.iter().map(|s| s.as_str()).collect())
                .with_page_size(choices.len().min(15))
                .with_help_message("Pick existing group, create new, or leave standalone")
                .prompt()?;

            if picked == "[ new group... ]" {
                Text::new("  New group name:").prompt()?.trim().to_string()
            } else if picked == "[ standalone — no group ]" {
                String::new()
            } else {
                picked.to_string()
            }
        };

        // Step 3 — project type + tech (GitHub topics)
        let proj_type = Select::new("  Project type (GitHub topic):", PROJECT_TYPES.to_vec())
            .with_page_size(PROJECT_TYPES.len())
            .prompt()?;

        let tech = Select::new("  Primary tech (GitHub topic):", TECHNOLOGIES.to_vec())
            .with_page_size(TECHNOLOGIES.len())
            .prompt()?;

        let topics = vec![proj_type.to_string(), tech.to_string()];

        // Build target path: workspace/category[/group]/repo
        let target_dir = if project_group.is_empty() {
            cfg.workspace_root.join(category)
        } else {
            cfg.workspace_root.join(category).join(&project_group)
        };
        let target_path_full = target_dir.join(&folder_name);
        let already_there = repo.path == target_path_full;

        let do_move = if already_there {
            println!("  (already in correct location)");
            false
        } else {
            let dest_display = target_path_full
                .strip_prefix(&cfg.workspace_root)
                .map(|r| r.display().to_string())
                .unwrap_or_else(|_| target_path_full.display().to_string());
            Confirm::new(&format!("  Move to workspace/{}?", dest_display))
                .with_default(true)
                .prompt()?
        };

        plans.push(RepoPlan {
            folder_name,
            current_path: repo.path.clone(),
            target_path: if do_move { Some(target_path_full) } else { None },
            topics,
        });
        println!();
    }

    if plans.is_empty() {
        println!("Nothing to do.");
        return Ok(());
    }

    // ── Summary ───────────────────────────────────────────────────────────────
    println!("\n══════════════════════════════════════════════════════");
    println!("  Summary — {} repos", plans.len());
    println!("══════════════════════════════════════════════════════");
    for p in &plans {
        let dest = match &p.target_path {
            Some(t) => t.strip_prefix(&cfg.workspace_root)
                .map(|r| format!("workspace/{}", r.display()))
                .unwrap_or_else(|_| t.display().to_string()),
            None    => "(no move)".to_string(),
        };
        println!("  {:<30}  →  {:<40}  [{}]",
            p.folder_name, dest, p.topics.join(", "));
    }

    let proceed = Confirm::new("\nApply all changes?")
        .with_default(true)
        .prompt()?;
    if !proceed { println!("Cancelled."); return Ok(()); }

    // ── Execute ───────────────────────────────────────────────────────────────
    let accounts = load_accounts(cfg)?;
    let provider_opt = default_account(&accounts, None)
        .and_then(|a| make_provider(a).ok());

    if provider_opt.is_none() {
        println!("  (no fxg account — GitHub topics skipped; run: fxg auth import)");
    }

    for p in &plans {
        if let Some(ref provider) = provider_opt {
            match provider.set_topics(&p.folder_name, &p.topics).await {
                Ok(_)  => println!("  ✓ [{}] topics: {}", p.folder_name, p.topics.join(", ")),
                Err(e) => println!("  ✗ [{}] topics: {}", p.folder_name, e),
            }
        }
        if let Some(ref dest) = p.target_path {
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    println!("  ✗ [{}] mkdir: {}", p.folder_name, e);
                    continue;
                }
            }
            match std::fs::rename(&p.current_path, dest) {
                Ok(_)  => {
                    let rel = dest.strip_prefix(&cfg.workspace_root)
                        .map(|r| r.display().to_string())
                        .unwrap_or_else(|_| dest.display().to_string());
                    println!("  ✓ [{}] moved → workspace/{}", p.folder_name, rel);
                }
                Err(e) => println!("  ✗ [{}] move: {}", p.folder_name, e),
            }
        }
    }

    println!("\n✓ Done.");
    Ok(())
}

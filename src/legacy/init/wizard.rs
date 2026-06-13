use crate::auth::Account;
use super::InitConfig;
use anyhow::Result;
use inquire::{Confirm, MultiSelect, Select, Text};

pub fn run_wizard(prefill_name: Option<&str>, accounts: &[Account]) -> Result<InitConfig> {
    println!("\n══════════════════════════════════════════");
    println!("  fxg — New Repository");
    println!("══════════════════════════════════════════\n");

    // Step 1 — Name
    let name = {
        let mut t = Text::new("Step 1/7 · Repository name:");
        if let Some(n) = prefill_name {
            t = t.with_default(n);
        }
        t.with_validator(|v: &str| {
            use inquire::validator::Validation;
            if v.is_empty() {
                Ok(Validation::Invalid("Name cannot be empty".into()))
            } else if v.contains(' ') {
                Ok(Validation::Invalid("Name cannot contain spaces".into()))
            } else {
                Ok(Validation::Valid)
            }
        }).prompt()?
    };

    // Step 2 — Provider & Account
    let account_id = if accounts.is_empty() {
        anyhow::bail!("No accounts configured. Run: fxg auth add");
    } else {
        let choices: Vec<String> = accounts.iter().map(|a| a.id()).collect();
        let default_idx = accounts.iter().position(|a| a.is_default).unwrap_or(0);
        Select::new("Step 2/7 · Provider & Account:", choices.clone())
            .with_starting_cursor(default_idx)
            .prompt()?
    };

    // Step 3 — Visibility
    let visibility = Select::new(
        "Step 3/7 · Visibility:",
        vec!["private", "public", "internal"],
    ).prompt()?;

    // Step 4 — Tags
    let tags_raw = Text::new("Step 4/7 · Tags (comma-separated):")
        .with_default("")
        .prompt()?;
    let tags: Vec<String> = tags_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Step 5 — Files
    let file_options = vec![
        "README.md", ".gitignore", "CHANGELOG.md", ".env.example", "LICENSE", "CONTRIBUTING.md",
    ];
    let selected_files = MultiSelect::new("Step 5/7 · Initial files:", file_options.clone())
        .with_default(&[0, 1, 2, 3])
        .prompt()?;

    let template = Select::new(
        "  Template:",
        vec!["node", "rust", "python", "blank"],
    ).prompt()?;

    // Step 6 — Branches
    let extra_raw = Text::new("Step 6/7 · Extra branches (comma-separated):")
        .with_default("dev")
        .prompt()?;
    let extra_branches: Vec<String> = extra_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Step 7 — Collaborators
    let add_collabs = Confirm::new("Step 7/7 · Add collaborators?")
        .with_default(false)
        .prompt()?;

    let collaborators = if add_collabs {
        collect_collaborators()?
    } else {
        vec![]
    };

    println!("\n──────────────────────────────────────────");
    let confirmed = Confirm::new("Proceed with creation?")
        .with_default(true)
        .prompt()?;

    if !confirmed {
        anyhow::bail!("Cancelled by user");
    }

    Ok(InitConfig {
        name,
        account_id,
        private: visibility == "private" || visibility == "internal",
        tags,
        branch: "main".into(),
        extra_branches,
        readme:      selected_files.contains(&"README.md"),
        gitignore:   selected_files.contains(&".gitignore"),
        changelog:   selected_files.contains(&"CHANGELOG.md"),
        env_example: selected_files.contains(&".env.example"),
        license:     selected_files.contains(&"LICENSE"),
        contributing: selected_files.contains(&"CONTRIBUTING.md"),
        template: template.to_string(),
        collaborators,
    })
}

fn collect_collaborators() -> Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    loop {
        let username = Text::new("  Collaborator username (blank to finish):")
            .prompt()?;
        if username.is_empty() { break; }

        let role = Select::new("  Role:", vec!["read", "write", "admin"])
            .with_starting_cursor(1)
            .prompt()?;

        result.push((username, role.to_string()));

        let more = Confirm::new("  Add another?").with_default(false).prompt()?;
        if !more { break; }
    }
    Ok(result)
}

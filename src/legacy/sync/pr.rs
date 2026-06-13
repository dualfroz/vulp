use crate::auth::load_accounts;
use crate::cli::{PrAction, PrArgs};
use crate::config::AppConfig;
use crate::providers::make_provider;
use anyhow::Result;
use inquire::Text;
fn open_editor_for_body() -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "notepad".into());
    let tmp    = std::env::temp_dir().join("fxg_pr_body.md");
    std::fs::write(&tmp, "# PR Description\n\n")?;
    std::process::Command::new(&editor).arg(&tmp).status()?;
    Ok(std::fs::read_to_string(&tmp).unwrap_or_default())
}

fn current_branch() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let Ok(repo) = git2::Repository::open(&cwd) else { return "main".into() };
    let Ok(head) = repo.head() else { return "main".into() };
    head.shorthand().unwrap_or("main").to_string()
}

pub async fn run(args: PrArgs, cfg: &AppConfig) -> Result<()> {
    match args.action {
        PrAction::Create { title, base, head, reviewers, labels, repo } => {
            // Resolve fields — prompt for any that are missing
            let title = match title {
                Some(t) => t,
                None    => Text::new("PR title:").prompt()?,
            };
            let head = match head {
                Some(h) => h,
                None    => {
                    let cur = current_branch();
                    Text::new("Head branch:").with_default(&cur).prompt()?
                }
            };
            let base = match base {
                Some(b) => b,
                None    => Text::new("Base branch:").with_default("main").prompt()?,
            };
            let body = open_editor_for_body()?;
            let repo_name = match repo {
                Some(r) => r,
                None    => std::env::current_dir()?
                    .file_name().unwrap_or_default()
                    .to_string_lossy().to_string(),
            };

            let accounts = load_accounts(cfg)?;
            let account  = crate::providers::default_account(&accounts, None)
                .ok_or_else(|| anyhow::anyhow!("no account configured"))?;
            let provider = make_provider(account)?;

            let pr = provider.create_pull_request(&repo_name, &title, &body, &head, &base).await?;

            println!("  ✓ PR #{} created: {}", pr.number, pr.url);

            if !reviewers.is_empty() {
                match provider.request_reviewers(&repo_name, pr.number, &reviewers).await {
                    Ok(()) => println!("  ✓ Reviewers requested: {}", reviewers.join(", ")),
                    Err(e) => println!("  ⚠ Could not request reviewers: {}", e),
                }
            }
            if !labels.is_empty() {
                match provider.add_labels(&repo_name, pr.number, &labels).await {
                    Ok(()) => println!("  ✓ Labels added: {}", labels.join(", ")),
                    Err(e) => println!("  ⚠ Could not add labels: {}", e),
                }
            }
            Ok(())
        }
    }
}

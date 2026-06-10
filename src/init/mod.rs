pub mod wizard;

use crate::auth::load_accounts;
use crate::cli::InitArgs;
use crate::config::AppConfig;
use crate::providers::make_provider;
use anyhow::Result;
use git2::{IndexAddOption, Repository, Signature};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct InitConfig {
    pub name:           String,
    pub account_id:     String,
    pub private:        bool,
    pub tags:           Vec<String>,
    #[allow(dead_code)]
    pub branch:         String,
    pub extra_branches: Vec<String>,
    pub readme:         bool,
    pub gitignore:      bool,
    pub changelog:      bool,
    pub env_example:    bool,
    pub license:        bool,
    pub contributing:   bool,
    pub template:       String,
    pub collaborators:  Vec<(String, String)>,
}

// ── git helpers ───────────────────────────────────────────────────────────────

pub fn git_init(path: &Path) -> Result<Repository> { Ok(Repository::init(path)?) }

pub fn write_files(path: &Path, ic: &InitConfig) -> Result<()> {
    if ic.readme    { std::fs::write(path.join("README.md"), format!("# {}\n", ic.name))?; }
    if ic.gitignore { std::fs::write(path.join(".gitignore"), gitignore_content(&ic.template))?; }
    if ic.changelog { std::fs::write(path.join("CHANGELOG.md"), "# Changelog\n\nAll notable changes...\n")?; }
    if ic.env_example { std::fs::write(path.join(".env.example"), "# Environment variables\n")?; }
    if ic.license {
        std::fs::write(path.join("LICENSE"), license_mit())?;
    }
    if ic.contributing {
        std::fs::write(path.join("CONTRIBUTING.md"), contributing_md())?;
    }
    Ok(())
}

fn gitignore_content(t: &str) -> &'static str {
    match t { "rust" => "/target\nCargo.lock\n", "python" => "__pycache__/\n*.pyc\n.venv/\n", _ => "/node_modules\ndist/\n.env\n" }
}

fn license_mit() -> &'static str {
    "MIT License\n\nCopyright (c) [year] [author]\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the \"Software\"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n"
}

fn contributing_md() -> &'static str {
    "# Contributing\n\nThank you for considering contributing!\n\n## How to contribute\n\n1. Fork the repository\n2. Create a feature branch: `git checkout -b feat/my-feature`\n3. Commit your changes: `git commit -m 'feat: add my feature'`\n4. Push to the branch: `git push origin feat/my-feature`\n5. Open a pull request\n\n## Code style\n\n- Follow existing conventions\n- Write tests for new functionality\n- Update documentation as needed\n"
}

pub fn initial_commit(repo: &Repository, username: &str, email: &str) -> Result<()> {
    let mut idx = repo.index()?;
    idx.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    idx.write()?;
    let tree = repo.find_tree(idx.write_tree()?)?;
    let sig  = Signature::now(username, email)?;
    repo.commit(Some("HEAD"), &sig, &sig, "chore: init", &tree, &[])?;
    Ok(())
}

pub fn push_to_remote(repo: &Repository, token: &str, refspec: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let t = token.to_string();
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |_url, user, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) { git2::Cred::ssh_key_from_agent(user.unwrap_or("git")) }
        else { git2::Cred::userpass_plaintext(user.unwrap_or("git"), &t) }
    });
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(cb);
    remote.push(&[refspec], Some(&mut opts))?;
    Ok(())
}

// ── orchestration ─────────────────────────────────────────────────────────────

pub async fn execute(ic: &InitConfig, cfg: &AppConfig) -> Result<()> {
    let accounts = load_accounts(cfg)?;
    let account  = accounts.iter().find(|a| a.id() == ic.account_id)
        .ok_or_else(|| anyhow::anyhow!("account '{}' not found — run: fxg auth add", ic.account_id))?;
    let token    = crate::auth::get_token(&account.id())?;
    let provider = make_provider(account)?;
    let t0       = Instant::now();

    let info = provider.create_repo(&ic.name, ic.private, "").await?;
    println!("  ✓ Created remote repo {} ({})", ic.name, ic.account_id);

    if !ic.tags.is_empty() {
        if let Err(e) = provider.set_topics(&ic.name, &ic.tags).await {
            println!("  ⚠ set_topics: {}", e);
        } else {
            println!("  ✓ Applied tags: {}", ic.tags.join(", "));
        }
    }

    let repo_path: PathBuf = cfg.workspace_root.join(&ic.name);
    std::fs::create_dir_all(&repo_path)?;
    println!("  ✓ mkdir {}", repo_path.display());

    let repo = git_init(&repo_path)?;
    println!("  ✓ git init");

    write_files(&repo_path, ic)?;
    println!("  ✓ Created initial files");

    if let Err(e) = initial_commit(&repo, &cfg.username, &cfg.email) {
        println!("  ✗ initial commit: {}", e);
    } else {
        println!("  ✓ Initial commit: \"chore: init\"");
    }

    repo.remote("origin", &info.ssh_url)?;
    println!("  ✓ git remote add origin {}", info.ssh_url);

    if let Err(e) = push_to_remote(&repo, &token, "refs/heads/main:refs/heads/main") {
        println!("  ✗ git push: {}", e);
    } else {
        println!("  ✓ git push -u origin main");
    }

    for branch in &ic.extra_branches {
        let head = repo.head()?.peel_to_commit()?;
        let _ = repo.branch(branch, &head, false);
        println!("  ✓ Created branch: {}", branch);
    }

    for (user, role) in &ic.collaborators {
        if let Err(e) = provider.add_collaborator(&ic.name, user, role).await {
            println!("  ✗ collaborator {}: {}", user, e);
        } else {
            println!("  ✓ Added collaborator: {} ({})", user, role);
        }
    }

    println!("\n  Done in {:.1}s · {}", t0.elapsed().as_secs_f64(), info.url);
    Ok(())
}

// ── entry point ───────────────────────────────────────────────────────────────

fn build_noninteractive(args: &InitArgs, accounts: &[crate::auth::Account]) -> Option<InitConfig> {
    let name = args.name.clone()?;
    let account = crate::providers::default_account(accounts, args.provider.as_deref())?;
    Some(InitConfig {
        name, account_id: account.id(),
        private:    args.status != "public",
        tags:       args.tags.clone(),
        branch:     args.branch.clone(),
        extra_branches: vec![],
        readme: true, gitignore: true, changelog: true, env_example: true,
        license: false, contributing: false,
        template: "blank".into(), collaborators: vec![],
    })
}

pub async fn run(args: InitArgs, cfg: &AppConfig) -> Result<()> {
    let accounts = load_accounts(cfg)?;

    // --batch mode
    if let Some(batch_input) = &args.batch {
        let names: Vec<String> = if std::path::Path::new(batch_input).exists() {
            std::fs::read_to_string(batch_input)?
                .lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
        } else {
            batch_input.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        let account = crate::providers::default_account(&accounts, args.provider.as_deref())
            .ok_or_else(|| anyhow::anyhow!("no account configured"))?;
        for name in names {
            let ic = InitConfig {
                name: name.clone(), account_id: account.id(),
                private: args.status != "public", tags: args.tags.clone(),
                branch: args.branch.clone(), extra_branches: vec![],
                readme: true, gitignore: true, changelog: false, env_example: false,
                license: false, contributing: false,
                template: "blank".into(), collaborators: vec![],
            };
            println!("\n══ {} ══", name);
            if let Err(e) = execute(&ic, cfg).await { println!("  ✗ {}", e); }
        }
        return Ok(());
    }

    // flags-only mode: name + provider both given → skip wizard
    if args.name.is_some() && args.provider.is_some() {
        let ic = build_noninteractive(&args, &accounts)
            .ok_or_else(|| anyhow::anyhow!("no matching account for provider '{}'", args.provider.as_deref().unwrap_or("")))?;
        return execute(&ic, cfg).await;
    }

    // interactive wizard
    let ic = wizard::run_wizard(args.name.as_deref(), &accounts)?;
    execute(&ic, cfg).await
}

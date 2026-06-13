use crate::cli::{UtilAction, UtilArgs};
use crate::config::AppConfig;
use crate::status::scan_workspace;
use anyhow::Result;
use chrono::Utc;
use git2::Repository;
use inquire::Select;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub async fn run(args: UtilArgs, cfg: &AppConfig) -> Result<()> {
    match args.action {
        None => {
            println!("Usage: fxg util <subcommand>");
            println!();
            println!("  log [--limit N]         Unified chronological log across all repos");
            println!("  find-secrets [<repos>]  Scan git history for leaked credentials");
            println!("  new-doc [<repo>]        Create CHANGELOG / README / TODO in a repo");
        }
        Some(UtilAction::Log { limit })            => run_log(limit, cfg)?,
        Some(UtilAction::FindSecrets { repos })    => run_find_secrets(repos, cfg)?,
        Some(UtilAction::NewDoc { pub_repo, doc_type }) => run_new_doc(pub_repo, doc_type, cfg)?,
    }
    Ok(())
}

// ── log ───────────────────────────────────────────────────────────────────────

struct LogEntry {
    ts:     i64,
    repo:   String,
    sha:    String,
    author: String,
    msg:    String,
}

fn run_log(limit: usize, cfg: &AppConfig) -> Result<()> {
    let repos = scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;

    let mut entries: Vec<LogEntry> = Vec::new();

    for rs in &repos {
        let Ok(repo) = Repository::open(&rs.path) else { continue };
        let mut revwalk = match repo.revwalk() {
            Ok(r)  => r,
            Err(_) => continue,
        };
        let _ = revwalk.push_head();
        for id in revwalk.take(limit) {
            let Ok(id)     = id else { continue };
            let Ok(commit) = repo.find_commit(id) else { continue };
            entries.push(LogEntry {
                ts:     commit.time().seconds(),
                repo:   rs.name.clone(),
                sha:    id.to_string()[..7].to_string(),
                author: commit.author().name().unwrap_or("").chars().take(16).collect(),
                msg:    commit.summary().unwrap_or("").chars().take(60).collect(),
            });
        }
    }

    entries.sort_by(|a, b| b.ts.cmp(&a.ts));
    entries.truncate(limit);

    if entries.is_empty() {
        println!("No commits found.");
        return Ok(());
    }

    println!("{:<16} {:<7} {:<16} {:<16} {}", "REPO", "SHA", "AUTHOR", "AGE", "MESSAGE");
    println!("{}", "─".repeat(90));
    for e in &entries {
        println!(
            "{:<16} {:<7} {:<16} {:<16} {}",
            truncate(&e.repo, 15), e.sha,
            truncate(&e.author, 15),
            format_age(e.ts),
            e.msg
        );
    }
    Ok(())
}

// ── find-secrets ──────────────────────────────────────────────────────────────

const PATTERNS: &[(&str, &str)] = &[
    ("Stripe live key",   "sk_live_"),
    ("Stripe test key",   "sk_test_"),
    ("AWS access key",    "AKIA"),
    ("RSA private key",   "BEGIN RSA PRIVATE KEY"),
    ("OpenSSH private key","BEGIN OPENSSH PRIVATE KEY"),
    ("GitHub PAT",        "ghp_"),
    ("Generic API key",   "api_key"),
    ("Generic secret",    "client_secret"),
];

fn run_find_secrets(repos: Vec<String>, cfg: &AppConfig) -> Result<()> {
    let all_repos = scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;

    let paths: Vec<PathBuf> = if repos.is_empty() {
        all_repos.iter().map(|r| r.path.clone()).collect()
    } else {
        all_repos.iter()
            .filter(|r| repos.contains(&r.name))
            .map(|r| r.path.clone())
            .collect()
    };

    if paths.is_empty() {
        println!("No repos found to scan.");
        return Ok(());
    }

    let mut total_hits = 0usize;

    for path in &paths {
        let repo_name = path.file_name().unwrap_or_default().to_string_lossy();
        println!("\nScanning {} ...", repo_name);

        let mut child = match Command::new("git")
            .args(["-C", path.to_str().unwrap_or("."), "log", "-p",
                   "--all", "--max-count=500", "--format=COMMIT:%H"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c)  => c,
            Err(e) => { eprintln!("  Could not run git: {}", e); continue; }
        };

        let stdout   = child.stdout.take().unwrap();
        let reader   = BufReader::new(stdout);
        let mut cur_commit = String::new();
        let mut hits_this  = 0usize;

        for line in reader.lines().flatten() {
            if let Some(sha) = line.strip_prefix("COMMIT:") {
                cur_commit = sha[..sha.len().min(7)].to_string();
                continue;
            }
            if !line.starts_with('+') || line.starts_with("+++") {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            for (label, pattern) in PATTERNS {
                if lower.contains(&pattern.to_ascii_lowercase()) {
                    println!("  [{}] commit {} — {}", label, cur_commit,
                        line.chars().take(80).collect::<String>().trim());
                    hits_this += 1;
                    total_hits += 1;
                    break;
                }
            }
        }
        let _ = child.wait();

        if hits_this == 0 {
            println!("  No secrets found.");
        }
    }

    println!("\n─────────────────────────────────────────────");
    if total_hits == 0 {
        println!("All clear — no secret patterns detected.");
    } else {
        println!("Found {} potential secret(s). Review the above lines.", total_hits);
    }
    Ok(())
}

// ── new-doc ───────────────────────────────────────────────────────────────────

const DOC_TYPES: &[&str] = &["CHANGELOG.md", "README.md", "TODO.md", "CONTRIBUTING.md", ".env.example"];

fn run_new_doc(pub_repo: Option<String>, doc_type: Option<String>, cfg: &AppConfig) -> Result<()> {
    // Resolve repo path
    let repo_path = resolve_repo_path(pub_repo, cfg)?;
    let repo_name = repo_path.file_name().unwrap_or_default().to_string_lossy().to_string();

    // Resolve doc type via picker if not given
    let doc = if let Some(t) = doc_type {
        if !DOC_TYPES.contains(&t.as_str()) {
            anyhow::bail!("Unknown doc type '{}'. Choose from: {}", t, DOC_TYPES.join(", "));
        }
        t
    } else {
        Select::new("Document to create:", DOC_TYPES.to_vec())
            .with_help_message("Use ↑/↓ to select, Enter to confirm")
            .prompt()?
            .to_string()
    };

    let dest = repo_path.join(&doc);
    if dest.exists() {
        anyhow::bail!("'{}' already exists in '{}'", doc, repo_name);
    }

    let content = template_content(&doc, &repo_name);
    std::fs::write(&dest, content)?;
    println!("✓ Created {}", dest.display());
    Ok(())
}

fn template_content(doc: &str, repo_name: &str) -> String {
    match doc {
        "CHANGELOG.md" => format!(
"# Changelog

All notable changes to **{}** are documented here.
Format based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

### Changed

### Fixed
", repo_name),

        "README.md" => format!(
"# {}

## Description

## Installation

```bash
# add install steps here
```

## Usage

```bash
# add usage examples here
```

## License

MIT
", repo_name),

        "TODO.md" => format!(
"# TODO — {}

## High Priority

- [ ]

## Medium Priority

- [ ]

## Low Priority

- [ ]
", repo_name),

        "CONTRIBUTING.md" => format!(
"# Contributing to {}

## Getting Started

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Commit your changes: `git commit -m 'feat: add my feature'`
4. Push to the branch: `git push origin feature/my-feature`
5. Open a Pull Request

## Code Style

Follow the existing conventions in the codebase.

## Reporting Issues

Open an issue with a clear description and steps to reproduce.
", repo_name),

        ".env.example" => String::from(
"# Copy this file to .env and fill in your values
# Never commit .env to version control

# Application
APP_ENV=development
APP_PORT=3000

# Database
DB_HOST=localhost
DB_PORT=5432
DB_NAME=
DB_USER=
DB_PASS=

# Auth / Secrets
SECRET_KEY=
"),
        _ => String::new(),
    }
}

fn resolve_repo_path(name: Option<String>, cfg: &AppConfig) -> Result<PathBuf> {
    if let Some(n) = name {
        // Search workspace_root and groups
        let candidates: Vec<PathBuf> = {
            let mut v = vec![cfg.workspace_root.join(&n)];
            for g in &cfg.workspace_groups {
                v.push(cfg.workspace_root.join(g).join(&n));
            }
            v
        };
        for p in candidates {
            if p.join(".git").exists() { return Ok(p); }
        }
        anyhow::bail!("Repo '{}' not found in workspace", n);
    }
    let repos = scan_workspace(&cfg.workspace_root, &cfg.workspace_groups)?;
    if repos.is_empty() {
        anyhow::bail!("No repos found in workspace. Check workspace_root in config.");
    }
    let names: Vec<String> = repos.iter().map(|r| r.name.clone()).collect();
    let chosen = Select::new("Select repository:", names)
        .with_help_message("Use ↑/↓ to select, Enter to confirm")
        .prompt()?;
    repos.iter()
        .find(|r| r.name == chosen)
        .map(|r| r.path.clone())
        .ok_or_else(|| anyhow::anyhow!("repo not found"))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn format_age(secs: i64) -> String {
    let diff = Utc::now().timestamp() - secs;
    match diff {
        d if d < 60         => "just now".into(),
        d if d < 3600       => format!("{}m ago", d / 60),
        d if d < 86400      => format!("{}h ago", d / 3600),
        d if d < 86400 * 2  => "yesterday".into(),
        d                   => format!("{}d ago", d / 86400),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}…", &s[..max - 1]) }
}

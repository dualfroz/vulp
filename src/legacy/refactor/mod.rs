pub mod ui;

use crate::cli::{RefactorAction, RefactorArgs};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FindResult {
    pub repo: String,
    pub file: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct CleanResult {
    pub file:   String,
    pub kind:   String,
    pub line:   usize,
    pub detail: String,
}

// ── find ──────────────────────────────────────────────────────────────────────

pub fn find_in_dir(pattern: &str, root: &Path, repo_name: &str) -> Vec<FindResult> {
    let mut results = Vec::new();
    let rx = match regex_simple(pattern) {
        Some(r) => r,
        None    => return results,
    };
    walk_files(root, &mut |file_path, rel| {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            for (i, line) in content.lines().enumerate() {
                if rx(line) {
                    results.push(FindResult {
                        repo: repo_name.to_string(),
                        file: rel.clone(),
                        line: i + 1,
                        text: line.trim().chars().take(120).collect(),
                    });
                }
            }
        }
    });
    results
}

// ── rename ────────────────────────────────────────────────────────────────────

pub fn rename_in_dir(from: &str, to: &str, root: &Path, dry_run: bool) -> Vec<FindResult> {
    let mut hits = Vec::new();
    walk_files(root, &mut |file_path, rel| {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            if content.contains(from) {
                let count = content.matches(from).count();
                hits.push(FindResult {
                    repo: String::new(),
                    file: rel.clone(),
                    line: count,
                    text: format!("{} occurrence(s)", count),
                });
                if !dry_run {
                    let _ = std::fs::write(&file_path, content.replace(from, to));
                }
            }
        }
    });
    hits
}

// ── clean ─────────────────────────────────────────────────────────────────────

pub fn find_unused(kind: &str, root: &Path) -> Vec<CleanResult> {
    let mut results = Vec::new();
    let pattern: &dyn Fn(&str) -> bool = match kind {
        "unused-imports"   => &|l: &str| l.trim_start().starts_with("use ") && l.contains("// unused"),
        "unused-vars"      => &|l: &str| l.contains("_unused") || l.contains("let _"),
        "dead-code"        => &|l: &str| l.contains("#[allow(dead_code)]"),
        "unused-functions" => &|l: &str| l.contains("fn ") && l.contains("// dead"),
        _                  => return results,
    };
    walk_files(root, &mut |file_path, rel| {
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            for (i, line) in content.lines().enumerate() {
                if pattern(line) {
                    results.push(CleanResult {
                        file: rel.clone(), kind: kind.to_string(),
                        line: i + 1, detail: line.trim().chars().take(80).collect(),
                    });
                }
            }
        }
    });
    results
}

// ── generate ──────────────────────────────────────────────────────────────────

pub fn generate_todo_list(root: &Path) -> Vec<FindResult> {
    find_in_dir("TODO|FIXME|HACK|XXX", root, "")
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn walk_files<F: FnMut(std::path::PathBuf, String)>(root: &Path, cb: &mut F) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !matches!(name.as_ref(), ".git" | "target" | "node_modules" | ".next") {
                walk_files(&path, cb);
            }
        } else if let Some(ext) = path.extension() {
            if matches!(ext.to_string_lossy().as_ref(), "rs" | "ts" | "js" | "py" | "go" | "md") {
                let rel = path.to_string_lossy().to_string();
                cb(path, rel);
            }
        }
    }
}

fn regex_simple(pattern: &str) -> Option<Box<dyn Fn(&str) -> bool>> {
    // Simple substring/literal match (not full regex to avoid extra dep)
    let p = pattern.to_string();
    Some(Box::new(move |line: &str| line.contains(p.as_str())))
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(args: RefactorArgs, cfg: &AppConfig) -> Result<()> {
    let root = &cfg.workspace_root;

    match args.action {
        None => ui::run_refactor_tui(cfg),
        Some(RefactorAction::Find { pattern, repos, .. }) => {
            let search_root = repos.as_ref()
                .map(|r| root.join(r))
                .unwrap_or_else(|| root.clone());
            let results = find_in_dir(&pattern, &search_root, "");
            if results.is_empty() {
                println!("No matches found for '{}'", pattern);
            } else {
                for r in &results {
                    println!("  {}:{} — {}", r.file, r.line, r.text);
                }
                println!("\n  {} match(es)", results.len());
            }
            Ok(())
        }
        Some(RefactorAction::Rename { from, to, repos, dry_run, kind: _ }) => {
            let search_root = repos.first()
                .map(|r| root.join(r))
                .unwrap_or_else(|| root.clone());
            let hits = rename_in_dir(&from, &to, &search_root, dry_run);
            let label = if dry_run { "Would rename" } else { "Renamed" };
            for h in &hits {
                println!("  {} {} occurrence(s) in {}", label, h.line, h.file);
            }
            println!("\n  {} file(s) affected", hits.len());
            Ok(())
        }
        Some(RefactorAction::Clean { target }) => {
            let results = find_unused(&target, root);
            for r in &results {
                println!("  {}:{} — {}", r.file, r.line, r.detail);
            }
            println!("\n  {} item(s) flagged", results.len());
            Ok(())
        }
        Some(RefactorAction::Generate { target }) => {
            match target.as_str() {
                "todo" => {
                    for r in generate_todo_list(root) {
                        println!("  {}:{} — {}", r.file, r.line, r.text);
                    }
                }
                _ => println!("Unknown generate target: {}", target),
            }
            Ok(())
        }
    }
}

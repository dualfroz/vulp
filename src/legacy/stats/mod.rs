pub mod ui;

use crate::cli::{StatsAction, StatsArgs};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct CodeStats {
    pub total_lines:    usize,
    pub file_count:     usize,
    pub function_count: usize,
    pub avg_fn_length:  f64,
    pub languages:      Vec<(String, f64)>,
    pub todos:          Vec<TodoItem>,
    pub sec_warnings:   Vec<SecWarning>,
    pub time_entries:   Vec<TimeEntry>,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub file: String,
    pub line: usize,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct SecWarning {
    pub file:   String,
    pub line:   usize,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct TimeEntry {
    pub file:          String,
    pub duration_mins: u64,   // stored as minutes
}

// ── VSC time data ─────────────────────────────────────────────────────────────

/// Load time-per-file data from ~/.fxg/vsc-data/<repo_name>.json
/// Expected format: `{ "files": { "src/main.rs": 3600 } }` (seconds)
pub fn load_vsc_data(vsc_dir: &Path, repo_name: &str) -> Vec<TimeEntry> {
    let path = vsc_dir.join(format!("{}.json", repo_name));
    let Ok(raw) = std::fs::read_to_string(&path) else { return vec![] };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else { return vec![] };
    let Some(files) = val.get("files").and_then(|f| f.as_object()) else { return vec![] };
    let mut entries: Vec<TimeEntry> = files.iter()
        .filter_map(|(file, secs)| {
            let s = secs.as_u64()?;
            Some(TimeEntry { file: file.clone(), duration_mins: s / 60 })
        }).collect();
    entries.sort_by(|a, b| b.duration_mins.cmp(&a.duration_mins));
    entries
}

// ── language detection ────────────────────────────────────────────────────────

fn detect_lang(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs"         => "Rust",
        "ts" | "tsx" => "TypeScript",
        "js" | "jsx" => "JavaScript",
        "py"         => "Python",
        "go"         => "Go",
        "java"       => "Java",
        "sql"        => "SQL",
        "sh"         => "Shell",
        "toml"       => "TOML",
        "json"       => "JSON",
        "md"         => "Markdown",
        _            => return None,
    })
}

fn count_fns(content: &str, lang: &str) -> usize {
    let pat = match lang { "Rust" => " fn ", "Python" => "def ", "Go" => "func ", _ => "function " };
    content.matches(pat).count()
}

// ── secret heuristics ─────────────────────────────────────────────────────────

fn scan_secrets(content: &str, file: &str) -> Vec<SecWarning> {
    let mut warnings = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let l = line.to_lowercase();
        if (l.contains("secret") || l.contains("api_key") || l.contains("password") || l.contains("jwt_secret"))
            && line.contains('=') && !line.trim().starts_with("//") && !line.trim().starts_with('#')
            && !line.ends_with("\"\"") && !line.ends_with("''")
        {
            warnings.push(SecWarning { file: file.to_string(), line: i + 1, reason: "Possible hardcoded secret".into() });
        }
    }
    warnings
}

// ── file walker ───────────────────────────────────────────────────────────────

fn walk_collect(root: &Path, stats: &mut CodeStats) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut lang_lines: std::collections::HashMap<String, usize> = Default::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !matches!(name.as_ref(), ".git" | "target" | "node_modules") { walk_collect(&path, stats); }
            continue;
        }
        let ext  = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let lang = match detect_lang(ext) { Some(l) => l, None => continue };
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        let line_count  = content.lines().count();
        stats.total_lines  += line_count;
        stats.file_count   += 1;
        *lang_lines.entry(lang.to_string()).or_default() += line_count;
        stats.function_count += count_fns(&content, lang);
        let rel = path.to_string_lossy().to_string();
        for (i, line) in content.lines().enumerate() {
            for kw in &["TODO", "FIXME", "HACK", "XXX"] {
                if line.contains(kw) {
                    stats.todos.push(TodoItem { file: rel.clone(), line: i + 1, kind: kw.to_string(), text: line.trim().chars().take(80).collect() });
                    break;
                }
            }
        }
        stats.sec_warnings.extend(scan_secrets(&content, &rel));
    }
    let total: usize = lang_lines.values().sum();
    stats.languages = lang_lines.iter()
        .map(|(l, c)| (l.clone(), if total > 0 { *c as f64 / total as f64 * 100.0 } else { 0.0 }))
        .collect();
    stats.languages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
}

pub fn collect_stats(root: &Path, vsc_dir: Option<&Path>) -> Result<CodeStats> {
    let mut stats = CodeStats::default();
    walk_collect(root, &mut stats);
    if stats.function_count > 0 {
        stats.avg_fn_length = stats.total_lines as f64 / stats.function_count as f64;
    }
    if let Some(vsc) = vsc_dir {
        let repo_name = root.file_name().unwrap_or_default().to_string_lossy();
        stats.time_entries = load_vsc_data(vsc, &repo_name);
    }
    Ok(stats)
}

pub async fn run(args: StatsArgs, cfg: &AppConfig) -> Result<()> {
    let root    = args.repo.as_ref().map(|r| cfg.workspace_root.join(r)).unwrap_or_else(|| cfg.workspace_root.clone());
    let vsc_dir = cfg.vsc_data_dir.exists().then(|| cfg.vsc_data_dir.as_path());
    let stats   = collect_stats(&root, vsc_dir)?;
    let save    = args.save;

    let mut output = String::new();

    match args.action {
        None => {
            if save {
                let md = stats_to_md(&stats, &root);
                save_stats_md(&root, &md)?;
                println!("  ✓ Saved to {}/.fxg/stats.md", root.file_name().unwrap_or_default().to_string_lossy());
            } else {
                return ui::run_stats_tui(stats);
            }
        }
        Some(StatsAction::Security) => {
            if stats.sec_warnings.is_empty() {
                output.push_str("  ✓ No security warnings found\n");
            } else {
                for w in &stats.sec_warnings {
                    output.push_str(&format!("  ⚠  {}:{} — {}\n", w.file, w.line, w.reason));
                }
            }
        }
        Some(StatsAction::Lines) => {
            output.push_str(&format!("  Lines: {}   Files: {}\n", stats.total_lines, stats.file_count));
        }
        Some(StatsAction::Languages) => {
            for (l, p) in &stats.languages {
                output.push_str(&format!("  {:15} {:.1}%\n", l, p));
            }
        }
        Some(StatsAction::Functions) => {
            output.push_str(&format!("  Functions: {}   Avg length: {:.1} lines\n", stats.function_count, stats.avg_fn_length));
        }
        Some(StatsAction::Time { repo, since }) => {
            let r    = repo.as_ref().map(|r| cfg.workspace_root.join(r)).unwrap_or_else(|| root.clone());
            let name = r.file_name().unwrap_or_default().to_string_lossy();
            let mut entries = load_vsc_data(&cfg.vsc_data_dir, &name);

            if let Some(since_str) = &since {
                use chrono::NaiveDate;
                if let Ok(date) = NaiveDate::parse_from_str(since_str, "%Y-%m-%d") {
                    let since_ts = date.and_hms_opt(0, 0, 0)
                        .map(|dt| dt.and_utc().timestamp())
                        .unwrap_or(0);
                    entries.retain(|e| {
                        let full_path = r.join(&e.file);
                        std::fs::metadata(&full_path)
                            .and_then(|m| m.modified())
                            .map(|t| {
                                t.duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs() as i64 >= since_ts)
                                    .unwrap_or(true)
                            })
                            .unwrap_or(true)
                    });
                }
            }

            if entries.is_empty() {
                output.push_str(&format!("  No VSC data at {}/{}.json\n", cfg.vsc_data_dir.display(), name));
            } else {
                let max = entries.first().map(|e| e.duration_mins).unwrap_or(1);
                output.push_str(&format!("  {:40} {:>8}  BAR\n", "FILE", "TIME"));
                output.push_str(&format!("  {}\n", "─".repeat(70)));
                for e in &entries {
                    let bar_w = (e.duration_mins as f64 / max as f64 * 20.0).round() as usize;
                    output.push_str(&format!("  {:40} {:5}m  {}\n",
                        e.file.split('/').last().unwrap_or(&e.file),
                        e.duration_mins,
                        "█".repeat(bar_w)));
                }
                let total_mins: u64 = entries.iter().map(|e| e.duration_mins).sum();
                output.push_str(&format!("\n  Total: {}h {}m\n", total_mins / 60, total_mins % 60));
            }
        }
    }

    if !output.is_empty() {
        print!("{}", output);
        if save {
            let md = format!("# Stats\n\n```\n{}```\n", output);
            save_stats_md(&root, &md)?;
            println!("  ✓ Saved to {}/.fxg/stats.md", root.file_name().unwrap_or_default().to_string_lossy());
        }
    }

    Ok(())
}

fn save_stats_md(root: &Path, content: &str) -> Result<()> {
    let dir = root.join(".fxg");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("stats.md"), content)?;
    Ok(())
}

fn stats_to_md(stats: &CodeStats, root: &Path) -> String {
    let name = root.file_name().unwrap_or_default().to_string_lossy();
    let mut md = format!("# Stats — {}\n\n", name);
    md.push_str(&format!("> Generated {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")));

    md.push_str("## Summary\n\n");
    md.push_str(&format!("| Metric | Value |\n|---|---|\n"));
    md.push_str(&format!("| Total lines | {} |\n", stats.total_lines));
    md.push_str(&format!("| Files | {} |\n", stats.file_count));
    md.push_str(&format!("| Functions | {} |\n", stats.function_count));
    md.push_str(&format!("| Avg fn length | {:.1} lines |\n", stats.avg_fn_length));
    md.push_str(&format!("| TODOs | {} |\n", stats.todos.len()));
    md.push_str(&format!("| Security warnings | {} |\n\n", stats.sec_warnings.len()));

    if !stats.languages.is_empty() {
        md.push_str("## Languages\n\n| Language | Share |\n|---|---|\n");
        for (lang, pct) in &stats.languages {
            md.push_str(&format!("| {} | {:.1}% |\n", lang, pct));
        }
        md.push('\n');
    }

    if !stats.todos.is_empty() {
        md.push_str("## TODOs\n\n| Kind | File | Line |\n|---|---|---|\n");
        for t in &stats.todos {
            let f = t.file.split(['/', '\\']).last().unwrap_or(&t.file);
            md.push_str(&format!("| `{}` | `{}` | {} |\n", t.kind, f, t.line));
        }
        md.push('\n');
    }

    if !stats.sec_warnings.is_empty() {
        md.push_str("## Security Warnings\n\n| File | Line | Reason |\n|---|---|---|\n");
        for w in &stats.sec_warnings {
            let f = w.file.split(['/', '\\']).last().unwrap_or(&w.file);
            md.push_str(&format!("| `{}` | {} | {} |\n", f, w.line, w.reason));
        }
        md.push('\n');
    }

    md
}

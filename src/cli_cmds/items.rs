//! Todo and project CLI commands — pure `~/.vulp/*.toml` state, no network.

use super::{die, out_json};
use crate::cli::{ProjectCmd, TodoCmd};
use crate::config::AppConfig;
use crate::todos::{self, Todo, TodoStatus};
use anyhow::Result;

fn parse_status(s: &str) -> TodoStatus {
    match s.to_lowercase().as_str() {
        "open" => TodoStatus::Open,
        "in-progress" | "inprogress" | "in_progress" => TodoStatus::InProgress,
        "done" => TodoStatus::Done,
        "cancelled" | "canceled" => TodoStatus::Cancelled,
        other => die(2, &format!("unknown status '{other}' (open|in-progress|done|cancelled)")),
    }
}

fn find_todo(all: &[Todo], id: u64) -> usize {
    all.iter().position(|t| t.id == id)
        .unwrap_or_else(|| die(4, &format!("todo #{id} not found")))
}

fn set_status(id: u64, status: TodoStatus) -> Result<()> {
    let mut all = todos::load();
    let i = find_todo(&all, id);
    all[i].status = status;
    all[i].touch();
    todos::save(&all)?;
    println!("todo #{id}: {}", all[i].status.label());
    Ok(())
}

pub fn todo(json: bool, cmd: TodoCmd) -> Result<()> {
    match cmd {
        TodoCmd::List { status } => {
            let mut all = todos::load();
            if let Some(s) = status {
                let want = parse_status(&s);
                all.retain(|t| t.status == want);
            }
            all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            if json {
                // Todo nie deriviuje Serialize (kolory ratatui w impl) — ręczna projekcja.
                #[derive(serde::Serialize)]
                struct T<'a> {
                    id: u64, title: &'a str, description: &'a str,
                    status: &'a str, project: Option<&'a str>, repo: Option<&'a str>,
                    created_at: i64, updated_at: i64,
                }
                let view: Vec<T> = all.iter().map(|t| T {
                    id: t.id, title: &t.title, description: &t.description,
                    status: t.status.label(), project: t.project.as_deref(), repo: t.repo.as_deref(),
                    created_at: t.created_at, updated_at: t.updated_at,
                }).collect();
                out_json(&view);
            } else if all.is_empty() {
                println!("no todos");
            } else {
                for t in &all {
                    println!("#{:<4} {:<12} {}", t.id, t.status.label(), t.title);
                }
            }
            Ok(())
        }
        TodoCmd::Add { title, desc, project } => {
            let mut all = todos::load();
            let id = todos::next_id(&all);
            all.push(Todo::new(id, title.clone(), desc, TodoStatus::Open, project));
            todos::save(&all)?;
            super::log_activity(crate::activity::ActivityKind::Todo, "", format!("added: {title}"));
            println!("todo #{id} added");
            Ok(())
        }
        TodoCmd::Done { id }   => set_status(id, TodoStatus::Done),
        TodoCmd::Start { id }  => set_status(id, TodoStatus::InProgress),
        TodoCmd::Cancel { id } => set_status(id, TodoStatus::Cancelled),
        TodoCmd::Rm { id } => {
            let mut all = todos::load();
            let i = find_todo(&all, id);
            let t = all.remove(i);
            todos::save(&all)?;
            println!("todo #{} removed ({})", id, t.title);
            Ok(())
        }
    }
}

pub fn project(cfg: &AppConfig, json: bool, cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::List => {
            let projects = crate::projects::load();
            if json {
                out_json(&projects);
            } else if projects.is_empty() {
                println!("no projects");
            } else {
                for p in &projects {
                    println!("{}  ({} repos)", p.name, p.repos.len());
                    for r in &p.repos { println!("  · {r}"); }
                }
            }
            Ok(())
        }
        ProjectCmd::Create { name, color } => {
            let mut projects = crate::projects::load();
            if projects.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
                die(1, &format!("project '{name}' already exists"));
            }
            projects.push(crate::projects::Project { name: name.clone(), color, repos: vec![] });
            crate::projects::save(&projects)?;
            println!("project '{name}' created");
            Ok(())
        }
        ProjectCmd::Assign { name, repos } => {
            if repos.is_empty() { die(2, "specify at least one repo"); }
            let mut projects = crate::projects::load();
            let Some(proj) = projects.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&name)) else {
                die(4, &format!("project '{name}' not found"));
            };
            // Zamień nazwy na ścieżki względne od workspace_root — tak trzyma je TUI.
            let scanned = super::scan_repos(cfg);
            for repo_name in &repos {
                let r = scanned.iter().find(|r| {
                    let needle = repo_name.to_lowercase();
                    r.name.to_lowercase() == needle
                        || r.path.file_name().map(|b| b.to_string_lossy().to_lowercase() == needle).unwrap_or(false)
                }).unwrap_or_else(|| die(4, &format!("repo '{repo_name}' not found")));
                let rel = r.path.strip_prefix(&cfg.workspace_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| r.path.to_string_lossy().to_string());
                if !proj.repos.contains(&rel) { proj.repos.push(rel); }
            }
            let count = proj.repos.len();
            crate::projects::save(&projects)?;
            println!("project '{name}': {count} repos");
            Ok(())
        }
        ProjectCmd::Rm { name } => {
            let mut projects = crate::projects::load();
            let before = projects.len();
            projects.retain(|p| !p.name.eq_ignore_ascii_case(&name));
            if projects.len() == before { die(4, &format!("project '{name}' not found")); }
            crate::projects::save(&projects)?;
            println!("project '{name}' removed");
            Ok(())
        }
    }
}

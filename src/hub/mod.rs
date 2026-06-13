mod actions;
mod keys;
mod modals;
mod render;
mod right_panel;
mod state;
mod tabs;

use actions::{exec_api_refresh, load_bookmarks, load_pins, refresh_repos, run_sync_flow};
use keys::tui_phase;
use state::{HubState, Modal, PhaseResult, SortMode, Tab};

use anyhow::Result;
use crate::config::AppConfig;
use crate::projects;
use crate::providers::{IssueDetail, PrDetail};
use crate::status::RepoSummary;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};
use std::collections::{HashMap, HashSet};

pub async fn run_overview_tui(
    mut repos: Vec<RepoSummary>,
    scan_rx: Option<std::sync::mpsc::Receiver<RepoSummary>>,
    cfg: &AppConfig,
) -> Result<()> {
    let workspace_root = cfg.workspace_root.clone();
    let projects       = projects::load();
    let project_map    = projects::index_by_repo(&projects);
    let todos          = crate::todos::load_manual();
    let bookmarks      = load_bookmarks();
    let pins           = load_pins();

    let meta_cache = crate::cache::load();
    for r in &mut repos {
        if let Some(entry) = meta_cache.get(&r.name) {
            let m = &entry.meta;
            r.stars       = m.stars;
            r.open_issues = m.open_issues;
            r.open_prs    = m.open_prs;
            if r.visibility.is_none() { r.visibility = m.visibility.clone(); }
        }
    }
    let cached_prs:    Vec<PrDetail>    = meta_cache.all_prs().into_iter().cloned().collect();
    let cached_issues: Vec<IssueDetail> = meta_cache.all_issues().into_iter().cloned().collect();

    let debug_mode = crate::debug::is_enabled();
    let debug_log  = crate::debug::take();

    let mut state = HubState {
        repos, projects, project_map, todos,
        prs:    cached_prs,
        issues: cached_issues,
        tab: Tab::Repos,
        display: Vec::new(), repo_pos: Vec::new(),
        cursor: 0, offset: 0, visible_rows: 0,
        selected: HashSet::new(),
        filter_dirty: false, filter_behind: false, filter_ahead: false,
        filter_archived: false,
        filter_text: String::new(), filtering: false,
        sort_mode: SortMode::Default,
        bookmarks,
        pins,
        status_msg: None,
        pending_clone: None,
        modal: Modal::None, confirming: None, exec_action: false,
        open_detail: false, quit: false,
        workspace_root,
        focused_cache: HashMap::new(),
        editor: cfg.editor.clone(),
        clipboard_cmd: cfg.clipboard_cmd.clone(),
        show_code_todos: false,
        debug_mode,
        debug_log,
        scan_rx,
        meta_cache,
        diff_rx: None,
    };
    for r in &mut state.repos {
        r.bookmarked = state.bookmarks.contains(&r.path);
    }
    state.rebuild_display();

    loop {
        let phase_result = tui_phase(&mut state)?;

        if state.quit { break; }

        match phase_result {
            PhaseResult::OpenDetail => {
                state.open_detail = false;
                if let Some(ri) = state.cur_repo_idx() {
                    let _ = crate::status::detail::run(state.repos[ri].clone(), cfg).await;
                } else if let Some(pi) = state.cur_proj_idx() {
                    let repo_indices = state.project_repos(pi);
                    let proj_repos: Vec<RepoSummary> =
                        repo_indices.iter().filter_map(|&ri| state.repos.get(ri)).cloned().collect();
                    run_project_detail(&state.projects[pi], proj_repos, cfg).await?;
                }
            }
            PhaseResult::ExecSync => {
                state.exec_action = false;
                state.confirming  = None;
                let targets = state.action_targets();
                run_sync_flow(&state.repos, &targets).await?;
                state.selected.clear();
                refresh_repos(&mut state);
            }
            PhaseResult::ExecRelease => {
                state.exec_action = false;
                state.confirming  = None;
                println!("\n  Use [Enter] → repo detail → Actions → Release");
                println!("  Press Enter...");
                let _ = std::io::stdin().lines().next();
            }
            PhaseResult::ExecClone(url) => {
                println!("\n  Cloning: {}", url);
                println!("  Into: {}\n", state.workspace_root.display());
                let _ = std::process::Command::new("git")
                    .arg("clone").arg(&url)
                    .current_dir(&state.workspace_root)
                    .status();
                println!("\n  Done. Press Enter...");
                let _ = std::io::stdin().lines().next();
                refresh_repos(&mut state);
            }
            PhaseResult::ExecRefresh => {
                exec_api_refresh(&mut state, cfg).await;
            }
            PhaseResult::None => {}
        }
    }
    Ok(())
}

async fn run_project_detail(proj: &crate::projects::Project, repos: Vec<RepoSummary>, _cfg: &AppConfig) -> Result<()> {
    use tabs::age_color;

    let (pr, pg, pb) = crate::projects::hex_to_rgb(&proj.color);
    let proj_color = ratatui::style::Color::Rgb(pr, pg, pb);

    let mut terminal = crate::tui::setup()?;
    let result = (|| -> Result<()> {
        loop {
            let proj_name  = proj.name.clone();
            let proj_hex   = proj.color.clone();
            let repos_snap = repos.clone();
            terminal.draw(|f| {
                let area = f.area();
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Fill(1), Constraint::Length(1)])
                    .split(area);

                f.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {} ", proj_name), Style::default().fg(proj_color).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("({})", proj_hex), Style::default().fg(proj_color)),
                    Span::styled(format!("  {} repos", repos_snap.len()), Style::default().fg(Color::DarkGray)),
                ])), layout[0]);

                let header = Row::new(vec!["NAME", "BRANCH", "STATUS", "AGE", "LAST COMMIT"])
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
                let rows: Vec<Row> = repos_snap.iter().map(|r| {
                    let status: String = if r.is_dirty { "⚠ dirty".into() } else if r.behind > 0 { format!("↓{}", r.behind) } else { "✓".into() };
                    let name_col = if r.is_dirty || r.ahead > 0 || r.behind > 0 { Color::Yellow } else { Color::White };
                    Row::new(vec![
                        Cell::from(r.name.clone()).style(Style::default().fg(name_col)),
                        Cell::from(r.branch.chars().take(12).collect::<String>()),
                        Cell::from(status),
                        Cell::from(r.last_commit_age.clone()).style(Style::default().fg(age_color(r.last_commit_ts))),
                        Cell::from(r.last_commit_msg.chars().take(40).collect::<String>()).style(Style::default().fg(Color::DarkGray)),
                    ])
                }).collect();
                f.render_widget(
                    Table::new(rows, [Constraint::Fill(1), Constraint::Length(13), Constraint::Length(10), Constraint::Length(10), Constraint::Fill(2)])
                        .header(header),
                    layout[1],
                );
                f.render_widget(
                    Paragraph::new("  [q/Esc] back").style(Style::default().fg(Color::DarkGray)),
                    layout[2],
                );
            })?;

            if let Some(Event::Key(k)) = crate::tui::next_event(250)? {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    crate::tui::teardown(terminal)?;
    result
}

pub fn run_detail_tui(detail: crate::status::RepoDetail) -> Result<()> {
    use crossterm::event::Event;
    use ratatui::widgets::{Block, Borders, Paragraph};
    let mut quit = false;
    crate::tui::run_loop(|terminal| {
        terminal.draw(|f| {
            let area = f.area();
            let blk  = Block::default().title(format!(" {} · {} ", detail.summary.name, detail.summary.branch)).borders(Borders::ALL);
            let inner = blk.inner(area);
            f.render_widget(blk, area);
            let lines = vec![
                Line::from(format!("  Branch: {}  +{} ahead  -{} behind  dirty: {}", detail.summary.branch, detail.summary.ahead, detail.summary.behind, detail.summary.is_dirty)),
                Line::from(format!("  Path: {}", detail.summary.path.display())),
                Line::from(""),
                Line::from("  Modified files:"),
            ];
            f.render_widget(Paragraph::new(lines), inner);
        })?;
        if let Some(Event::Key(key)) = crate::tui::next_event(250)? {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) { quit = true; }
        }
        Ok(quit)
    })
}

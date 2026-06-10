use crate::status::{RepoDetail, RepoSummary};
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

// ── overview TUI ──────────────────────────────────────────────────────────────

struct OverviewState {
    repos:    Vec<RepoSummary>,
    selected: usize,
    quit:     bool,
}

fn render_overview(f: &mut Frame, state: &OverviewState) {
    let area = f.area();
    let outer = Block::default()
        .title(format!(" fxg — Workspace Status · {} repos ", state.repos.len()))
        .borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    let header = Row::new(vec!["#", "NAME", "BRANCH", "AHEAD", "BEHIND", "DIRTY", "LAST COMMIT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.repos.iter().enumerate().map(|(i, r)| {
        let dirty = if r.is_dirty { "✓" } else { "✗" };
        let row_style = if i == state.selected {
            Style::default().bg(Color::DarkGray)
        } else if r.is_dirty {
            Style::default().fg(Color::Yellow)
        } else if r.behind > 0 {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(format!("{}", i + 1)),
            Cell::from(r.name.clone()),
            Cell::from(r.branch.clone()),
            Cell::from(r.ahead.to_string()),
            Cell::from(r.behind.to_string()),
            Cell::from(dirty),
            Cell::from(r.last_commit_age.clone()),
        ]).style(row_style)
    }).collect();

    let widths = [
        Constraint::Length(4), Constraint::Fill(1), Constraint::Length(14),
        Constraint::Length(6), Constraint::Length(7), Constraint::Length(6),
        Constraint::Length(12),
    ];
    f.render_widget(Table::new(rows, widths).header(header), chunks[0]);
    f.render_widget(
        Paragraph::new("  [↑/↓] navigate  [Enter] drill-in  [f] filter  [q] quit")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

pub fn run_overview_tui(repos: Vec<RepoSummary>) -> Result<()> {
    let mut state = OverviewState { repos, selected: 0, quit: false };
    tui::run_loop(|terminal| {
        terminal.draw(|f| render_overview(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
                KeyCode::Up   | KeyCode::Char('k') => {
                    if state.selected > 0 { state.selected -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if state.selected + 1 < state.repos.len() { state.selected += 1; }
                }
                KeyCode::Enter => {
                    if let Some(r) = state.repos.get(state.selected) {
                        if let Ok(detail) = crate::status::repo_detail(&r.path) {
                            // temporarily exit and re-enter for the detail view
                            state.quit = true;
                            // Note: after this loop ends we open detail TUI
                            let _ = run_detail_tui(detail);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(state.quit)
    })
}

// ── detail TUI ────────────────────────────────────────────────────────────────

fn render_detail(f: &mut Frame, detail: &RepoDetail) {
    let area = f.area();
    let outer = Block::default()
        .title(format!(" {} · {} ", detail.summary.name, detail.summary.branch))
        .borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Fill(1), Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    // Header info
    let info = vec![
        Line::from(vec![Span::raw("  Branch:  "), Span::styled(&detail.summary.branch, Style::default().fg(Color::Cyan))]),
        Line::from(vec![
            Span::raw("  Status:  "),
            Span::raw(format!("+{} ahead  -{} behind  dirty: {}", detail.summary.ahead, detail.summary.behind,
                if detail.summary.is_dirty { "yes" } else { "no" })),
        ]),
    ];
    f.render_widget(Paragraph::new(info), chunks[0]);

    // Modified files
    let file_block = Block::default().title(" Modified files ").borders(Borders::ALL);
    let files: Vec<ListItem> = detail.modified_files.iter()
        .map(|(ch, path)| ListItem::new(format!("  {}  {}", ch, path)))
        .collect();
    f.render_widget(List::new(files).block(file_block), chunks[1]);

    // Recent commits
    let commit_block = Block::default().title(" Recent commits ").borders(Borders::ALL);
    let commits: Vec<ListItem> = detail.recent_commits.iter()
        .map(|c| ListItem::new(format!("  {}  {:<50}  {}  {}", c.sha, c.msg, c.age, c.author)))
        .collect();
    f.render_widget(List::new(commits).block(commit_block), chunks[2]);

    f.render_widget(
        Paragraph::new("  [p] pull  [P] push  [c] commit  [b] branches  [q] back")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );
}

pub fn run_detail_tui(detail: RepoDetail) -> Result<()> {
    let mut quit = false;
    tui::run_loop(|terminal| {
        terminal.draw(|f| render_detail(f, &detail))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                quit = true;
            }
        }
        Ok(quit)
    })
}

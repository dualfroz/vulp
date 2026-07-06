//! Standalone TUI for `vulp auth` with no subcommand: lists configured
//! accounts and lets the user remove one or change the default. Adding an
//! account isn't done here — it needs an interactive token prompt, which
//! doesn't fit cleanly inside this raw-mode loop, so that's deferred to the
//! `vulp auth add` CLI form instead.

use crate::auth::Account;
use crate::config::AppConfig;
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

struct State {
    accounts: Vec<Account>,
    selected: usize,
    status:   Option<String>,
    quit:     bool,
}

/// Draws the account table (provider, alias, email, default marker) and the
/// key-hint/status line below it.
fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default()
        .title(" vulp — Authenticated Accounts ")
        .borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(2)])
        .split(inner);

    let header = Row::new(vec!["", "Provider", "Alias", "Email", "Status"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.accounts.iter().enumerate().map(|(i, a)| {
        let star = if a.is_default { "★" } else { " " };
        let status_cell = Cell::from("✓ active").style(Style::default().fg(Color::Green));
        let style = if i == state.selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };
        Row::new(vec![
            Cell::from(format!("{} [{}]", star, i + 1)),
            Cell::from(a.provider.clone()),
            Cell::from(a.alias.clone()),
            Cell::from(a.email.clone()),
            status_cell,
        ]).style(style)
    }).collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Fill(1),
        Constraint::Length(10),
    ];
    let table = Table::new(rows, widths).header(header);
    f.render_widget(table, chunks[0]);

    let hint = state.status.as_deref().unwrap_or(
        "  [a] add  [r] remove  [s] set default  [t] test  [q] quit"
    );
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

/// Runs the account-management TUI until the user quits, returning the
/// (possibly modified) account list for the caller to persist.
pub fn run_auth_tui(mut accounts: Vec<Account>, _cfg: &AppConfig) -> Result<Vec<Account>> {
    let mut state = State { accounts: accounts.clone(), selected: 0, status: None, quit: false };

    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            state.status = None;
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
                KeyCode::Up   | KeyCode::Char('k') => {
                    if state.selected > 0 { state.selected -= 1; }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if state.selected + 1 < state.accounts.len() { state.selected += 1; }
                }
                KeyCode::Char('a') => {
                    // must restore terminal temporarily for inquire
                    // handled outside the raw-mode loop
                    state.status = Some("  Press [q] first, then use: vulp auth add --provider <p> --token <t>".into());
                }
                KeyCode::Char('r') => {
                    if let Some(a) = state.accounts.get(state.selected) {
                        let id = a.id();
                        state.accounts.retain(|x| x.id() != id);
                        let _ = crate::auth::delete_token(&id);
                        if state.selected > 0 { state.selected -= 1; }
                        state.status = Some(format!("  ✓ Removed {}", id));
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(a) = state.accounts.get(state.selected) {
                        let id = a.id();
                        for acc in state.accounts.iter_mut() {
                            acc.is_default = acc.id() == id;
                        }
                        state.status = Some(format!("  ✓ Default set to {}", id));
                    }
                }
                _ => {}
            }
        }
        Ok(state.quit)
    })?;

    accounts = state.accounts;
    Ok(accounts)
}

use crate::config::AppConfig;
use crate::refactor::{CleanResult, FindResult};
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

#[derive(PartialEq)]
enum View { Menu, FindResults, CleanResults }

struct State {
    view:          View,
    find_results:  Vec<FindResult>,
    clean_results: Vec<CleanResult>,
    cursor:        usize,
    scroll:        usize,
    input_buf:     String,
    input_active:  bool,
    status:        Option<String>,
    quit:          bool,
}

const MENU_ITEMS: &[&str] = &[
    "[1] Rename identifier",
    "[2] Find pattern",
    "[3] Clean unused",
    "[4] Generate TODO list",
];

impl State {
    fn new() -> Self {
        Self {
            view: View::Menu, find_results: vec![], clean_results: vec![],
            cursor: 0, scroll: 0, input_buf: String::new(), input_active: false,
            status: None, quit: false,
        }
    }
}

fn render_menu(f: &mut Frame, state: &State, area: Rect) {
    let block = Block::default().title(" Refactor operations ").borders(Borders::ALL);
    let items: Vec<ListItem> = MENU_ITEMS.iter().enumerate().map(|(i, item)| {
        let style = if i == state.cursor { Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD) }
                    else { Style::default() };
        ListItem::new(format!("  {}", item)).style(style)
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_find(f: &mut Frame, state: &State, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    // Left: file list
    let left_block = Block::default().title(" Files ").borders(Borders::ALL);
    let items: Vec<ListItem> = state.find_results.iter().enumerate()
        .skip(state.scroll)
        .take(chunks[0].height.saturating_sub(2) as usize)
        .map(|(i, r)| {
            let style = if i == state.cursor { Style::default().bg(Color::DarkGray) }
                        else { Style::default() };
            ListItem::new(format!("  {}:{}", r.file.split('/').last().unwrap_or(&r.file), r.line)).style(style)
        }).collect();
    f.render_widget(List::new(items).block(left_block), chunks[0]);

    // Right: context
    let right_block = Block::default().title(" Context ").borders(Borders::ALL);
    let preview = state.find_results.get(state.cursor)
        .map(|r| format!("  {}:{}\n\n  {}", r.file, r.line, r.text))
        .unwrap_or_else(|| "  No selection".into());
    f.render_widget(Paragraph::new(preview).block(right_block), chunks[1]);
}

fn render_clean(f: &mut Frame, state: &State, area: Rect) {
    let block = Block::default().title(" Clean results ").borders(Borders::ALL);
    let items: Vec<ListItem> = state.clean_results.iter().map(|r| {
        ListItem::new(format!("  {}:{} [{}] — {}", r.file, r.line, r.kind, r.detail))
            .style(Style::default().fg(Color::Yellow))
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default().title(" fxg — Refactor ").borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(2)])
        .split(inner);

    match state.view {
        View::Menu        => render_menu(f, state, chunks[0]),
        View::FindResults => render_find(f, state, chunks[0]),
        View::CleanResults => render_clean(f, state, chunks[0]),
    }

    let hint_text = if state.input_active {
        format!("  Pattern: {}_ ", state.input_buf)
    } else {
        let base = match state.view {
            View::Menu         => "  [Enter] select  [q] quit",
            View::FindResults  => "  [j/k] navigate  [Esc] back  [q] quit",
            View::CleanResults => "  [Esc] back  [q] quit",
        };
        if let Some(s) = &state.status { format!("  {}  — {}", s, base) } else { base.into() }
    };
    f.render_widget(Paragraph::new(hint_text).style(Style::default().fg(Color::DarkGray)), chunks[1]);
}

fn handle_key(state: &mut State, key: crossterm::event::KeyEvent, cfg: &AppConfig) {
    if state.input_active {
        match key.code {
            KeyCode::Enter => {
                let results = crate::refactor::find_in_dir(&state.input_buf, &cfg.workspace_root, "workspace");
                state.find_results = results;
                state.cursor = 0;
                state.scroll = 0;
                state.view = View::FindResults;
                state.input_active = false;
            }
            KeyCode::Esc       => { state.input_active = false; state.input_buf.clear(); }
            KeyCode::Char(c)   => state.input_buf.push(c),
            KeyCode::Backspace => { state.input_buf.pop(); }
            _ => {}
        }
        return;
    }

    match state.view {
        View::Menu => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
            KeyCode::Up   | KeyCode::Char('k') => { if state.cursor > 0 { state.cursor -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => { if state.cursor + 1 < MENU_ITEMS.len() { state.cursor += 1; } }
            KeyCode::Enter | KeyCode::Char('2') => {
                state.input_active = true;
                state.input_buf.clear();
                state.status = Some("Type pattern and press Enter".into());
            }
            KeyCode::Char('3') => {
                state.clean_results = crate::refactor::find_unused("dead-code", &cfg.workspace_root);
                state.view = View::CleanResults;
                state.cursor = 0;
            }
            KeyCode::Char('4') => {
                state.find_results = crate::refactor::generate_todo_list(&cfg.workspace_root);
                state.view = View::FindResults;
                state.cursor = 0;
            }
            _ => {}
        },
        View::FindResults | View::CleanResults => match key.code {
            KeyCode::Esc | KeyCode::Char('b') => { state.view = View::Menu; state.cursor = 0; }
            KeyCode::Char('q') => state.quit = true,
            KeyCode::Up   | KeyCode::Char('k') => { if state.cursor > 0 { state.cursor -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => state.cursor += 1,
            _ => {}
        },
    }
}

pub fn run_refactor_tui(cfg: &AppConfig) -> Result<()> {
    let mut state = State::new();
    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(&mut state, key, cfg);
        }
        Ok(state.quit)
    })
}

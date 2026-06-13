use super::{adopt_folder, AdoptResult};
use crate::config::AppConfig;
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::path::{Path, PathBuf};

#[derive(PartialEq)]
enum Mode { Select, Progress }

struct State {
    folders:        Vec<PathBuf>,
    workspace_root: PathBuf,
    selected:       Vec<bool>,
    cursor:         usize,
    mode:           Mode,
    results:        Vec<AdoptResult>,
    quit:           bool,
}

impl State {
    fn new(folders: Vec<PathBuf>, workspace_root: PathBuf) -> Self {
        let len = folders.len();
        Self {
            folders, workspace_root,
            selected: vec![false; len],
            cursor: 0, mode: Mode::Select,
            results: vec![], quit: false,
        }
    }
    fn toggle(&mut self) {
        if let Some(v) = self.selected.get_mut(self.cursor) { *v = !*v; }
    }
    fn selected_paths(&self) -> Vec<PathBuf> {
        self.folders.iter().enumerate()
            .filter(|(i, _)| self.selected[*i])
            .map(|(_, p)| p.clone())
            .collect()
    }
}

/// Show path relative to workspace_root, e.g. "clients/client-a"
fn rel_name(p: &Path, root: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_else(|_| p.file_name().unwrap_or_default().to_string_lossy().to_string())
}

fn render_select(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let selected_count = state.selected.iter().filter(|&&v| v).count();
    let title = format!(
        " Foldery bez .git ({} zaznaczonych — Enter aby adoptować) ",
        selected_count
    );
    let block = Block::default().title(title).borders(Borders::ALL);
    let items: Vec<ListItem> = state.folders.iter().enumerate().map(|(i, p)| {
        let check = if state.selected[i] { "[x]" } else { "[ ]" };
        let style = if i == state.cursor {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let label = rel_name(p, &state.workspace_root);
        ListItem::new(format!("  {} {}", check, label)).style(style)
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_progress(f: &mut Frame, results: &[AdoptResult], total: usize, area: ratatui::layout::Rect) {
    let done = results.len();
    let title = format!(" Adoptowanie ({}/{}) ", done, total);
    let block = Block::default().title(title).borders(Borders::ALL);
    let items: Vec<ListItem> = results.iter().map(|r| {
        if r.success {
            let url = r.url.as_deref().unwrap_or("");
            ListItem::new(format!("  ✓ {}  →  {}", r.name, url))
                .style(Style::default().fg(Color::Green))
        } else {
            ListItem::new(format!("  ✗ {}  —  {}", r.name, r.detail))
                .style(Style::default().fg(Color::Red))
        }
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render(f: &mut Frame, state: &State, total_to_adopt: usize) {
    let area = f.area();
    let outer = Block::default().title(" fxg — Adopt ").borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    match state.mode {
        Mode::Select   => render_select(f, state, chunks[0]),
        Mode::Progress => render_progress(f, &state.results, total_to_adopt, chunks[0]),
    }

    let hint = match state.mode {
        Mode::Select   => "  [Space] zaznacz  [a] wszystkie  [Enter] adoptuj zaznaczone  [q] wyjdź",
        Mode::Progress => "  [q] wyjdź",
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

fn handle_key(state: &mut State, key: crossterm::event::KeyEvent) {
    match state.mode {
        Mode::Select => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
            KeyCode::Char('a') => state.selected.iter_mut().for_each(|v| *v = true),
            KeyCode::Char(' ') => state.toggle(),
            KeyCode::Up   | KeyCode::Char('k') => { if state.cursor > 0 { state.cursor -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.cursor + 1 < state.folders.len() { state.cursor += 1; }
            }
            KeyCode::Enter => {
                if state.selected.iter().any(|&v| v) {
                    state.mode = Mode::Progress;
                }
            }
            _ => {}
        },
        Mode::Progress => {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                state.quit = true;
            }
        }
    }
}

pub async fn run_adopt_tui(
    folders: Vec<PathBuf>,
    workspace_root: &Path,
    cfg: &AppConfig,
    private: bool,
    commit_msg: &str,
    provider_hint: Option<&str>,
) -> Result<()> {
    let mut state = State::new(folders, workspace_root.to_path_buf());
    let mut pending: Option<Vec<PathBuf>> = None;
    let mut total_to_adopt = 0usize;

    // We need to leave TUI to run async adopt (git2 + HTTP) then re-enter for results.
    // Strategy: collect selection in TUI, exit TUI, run adopts, show summary in plain text.
    // This avoids threading git2/reqwest into the ratatui event loop.

    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state, total_to_adopt))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(&mut state, key);
            // When user confirms (mode flipped to Progress), capture selection and break
            if state.mode == Mode::Progress && pending.is_none() {
                pending = Some(state.selected_paths());
                total_to_adopt = pending.as_ref().unwrap().len();
                state.quit = true; // exit TUI loop, run adopts outside
            }
        }
        Ok(state.quit)
    })?;

    let paths = match pending {
        Some(p) if !p.is_empty() => p,
        _ => return Ok(()),
    };

    // Run adopts outside TUI (async + network)
    println!("\nAdoptuję {} projekt(ów)...\n", paths.len());
    for path in &paths {
        let label = rel_name(path, cfg.workspace_root.as_path());
        let result = adopt_folder(path, cfg, private, commit_msg, provider_hint).await;
        if result.success {
            println!("  ✓ {}  →  {}", label, result.url.as_deref().unwrap_or(""));
        } else {
            println!("  ✗ {}  —  {}", label, result.detail);
        }
    }
    println!();

    Ok(())
}

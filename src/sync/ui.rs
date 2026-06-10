use crate::config::AppConfig;
use crate::status::RepoSummary;
use crate::sync::SyncResult;
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

#[derive(PartialEq)]
enum Mode { RepoSelect, ActionMenu, CommitInput, Progress }

struct State {
    repos:          Vec<RepoSummary>,
    selected_repos: Vec<bool>,
    cursor:         usize,
    mode:           Mode,
    action_cursor:  usize,
    commit_msg:     String,
    results:        Vec<SyncResult>,
    quit:           bool,
}

const ACTIONS: &[&str] = &["Pull", "Commit", "Push", "New branch", "Create PR"];

impl State {
    fn new(repos: Vec<RepoSummary>) -> Self {
        let len = repos.len();
        Self {
            repos, selected_repos: vec![false; len],
            cursor: 0, mode: Mode::RepoSelect,
            action_cursor: 0, commit_msg: String::new(),
            results: vec![], quit: false,
        }
    }
    fn toggle(&mut self) {
        if let Some(v) = self.selected_repos.get_mut(self.cursor) { *v = !*v; }
    }
    fn selected_names(&self) -> Vec<String> {
        self.repos.iter().enumerate()
            .filter(|(i, _)| self.selected_repos[*i])
            .map(|(_, r)| r.name.clone())
            .collect()
    }
}

fn render_repo_select(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Select repositories (Space = toggle, Enter = confirm) ").borders(Borders::ALL);
    let items: Vec<ListItem> = state.repos.iter().enumerate().map(|(i, r)| {
        let check = if state.selected_repos[i] { "[x]" } else { "[ ]" };
        let dirty = if r.is_dirty { " ✓" } else { "" };
        let style = if i == state.cursor { Style::default().bg(Color::DarkGray) } else { Style::default() };
        ListItem::new(format!("  {} {:30} {}{}", check, r.name, r.branch, dirty)).style(style)
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_action_menu(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let repos = state.selected_names();
    let title = format!(" Action for: {} ", repos.join(", "));
    let block = Block::default().title(title).borders(Borders::ALL);
    let items: Vec<ListItem> = ACTIONS.iter().enumerate().map(|(i, a)| {
        let style = if i == state.action_cursor { Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD) }
                    else { Style::default() };
        ListItem::new(format!("  {}", a)).style(style)
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_commit_input(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let repos = state.selected_names();
    let title = format!(" Commit message for: {} ", repos.join(", "));
    let block = Block::default().title(title).borders(Borders::ALL);
    let display = format!("  > {}_", state.commit_msg);
    f.render_widget(Paragraph::new(display).block(block), area);
}

fn render_progress(f: &mut Frame, results: &[SyncResult], area: ratatui::layout::Rect) {
    let block = Block::default().title(" Progress ").borders(Borders::ALL);
    let items: Vec<ListItem> = results.iter().map(|r| {
        let (icon, color) = if r.success { ("✓", Color::Green) } else { ("✗", Color::Red) };
        ListItem::new(format!("  {} {} — {} — {}", icon, r.repo, r.action, r.detail))
            .style(Style::default().fg(color))
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default().title(" fxg — Sync ").borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    match state.mode {
        Mode::RepoSelect  => render_repo_select(f, state, chunks[0]),
        Mode::ActionMenu  => render_action_menu(f, state, chunks[0]),
        Mode::CommitInput => render_commit_input(f, state, chunks[0]),
        Mode::Progress    => render_progress(f, &state.results, chunks[0]),
    }

    let hint = match state.mode {
        Mode::RepoSelect  => "  [Space] toggle  [a] select all  [Enter] confirm  [q] quit",
        Mode::ActionMenu  => "  [↑/↓] navigate  [Enter] execute  [Esc] back",
        Mode::CommitInput => "  [Type] commit message  [Enter] commit  [Esc] back",
        Mode::Progress    => "  [q] quit",
    };
    f.render_widget(Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)), chunks[1]);
}

fn handle_key(state: &mut State, key: crossterm::event::KeyEvent, cfg: &AppConfig) {
    match state.mode {
        Mode::RepoSelect => match key.code {
            KeyCode::Char('q') | KeyCode::Esc  => state.quit = true,
            KeyCode::Char('a') => state.selected_repos.iter_mut().for_each(|v| *v = true),
            KeyCode::Char(' ') => state.toggle(),
            KeyCode::Up   | KeyCode::Char('k') => { if state.cursor > 0 { state.cursor -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => { if state.cursor + 1 < state.repos.len() { state.cursor += 1; } }
            KeyCode::Enter => {
                if state.selected_repos.iter().any(|&v| v) { state.mode = Mode::ActionMenu; }
            }
            _ => {}
        },
        Mode::ActionMenu => match key.code {
            KeyCode::Esc => state.mode = Mode::RepoSelect,
            KeyCode::Up   | KeyCode::Char('k') => { if state.action_cursor > 0 { state.action_cursor -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => { if state.action_cursor + 1 < ACTIONS.len() { state.action_cursor += 1; } }
            KeyCode::Enter => {
                if state.action_cursor == 1 {
                    state.commit_msg.clear();
                    state.mode = Mode::CommitInput;
                } else {
                    execute_action(state, cfg);
                    state.mode = Mode::Progress;
                }
            }
            _ => {}
        },
        Mode::CommitInput => match key.code {
            KeyCode::Esc => state.mode = Mode::ActionMenu,
            KeyCode::Enter => {
                if !state.commit_msg.is_empty() {
                    execute_action(state, cfg);
                    state.mode = Mode::Progress;
                }
            }
            KeyCode::Backspace => { state.commit_msg.pop(); }
            KeyCode::Char(c)   => state.commit_msg.push(c),
            _ => {}
        },
        Mode::Progress => {
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) { state.quit = true; }
        }
    }
}

fn execute_action(state: &mut State, cfg: &AppConfig) {
    let accounts = crate::auth::load_accounts(cfg).unwrap_or_default();
    let token = crate::providers::default_account(&accounts, Some(&cfg.default_provider))
        .and_then(|a| crate::auth::get_token(&a.id()).ok())
        .unwrap_or_default();

    let selected: Vec<_> = state.repos.iter()
        .enumerate()
        .filter(|(i, _)| state.selected_repos[*i])
        .map(|(_, r)| r.clone())
        .collect();

    for repo in &selected {
        let result = match state.action_cursor {
            0 => crate::sync::pull_repo(&repo.path, &token)
                    .unwrap_or_else(|e| SyncResult::err(&repo.name, "pull", e.to_string())),
            1 => crate::sync::commit_repo(&repo.path, &state.commit_msg, &cfg.username, &cfg.email)
                    .unwrap_or_else(|e| SyncResult::err(&repo.name, "commit", e.to_string())),
            2 => crate::sync::push_repo(&repo.path, &token, false)
                    .unwrap_or_else(|e| SyncResult::err(&repo.name, "push", e.to_string())),
            _ => SyncResult::err(&repo.name, ACTIONS[state.action_cursor],
                     "niedostepne w TUI — uzyj: fxg sync branch create <name> / fxg sync pr create"),
        };
        state.results.push(result);
    }
}

pub fn run_sync_tui(repos: Vec<RepoSummary>, cfg: &AppConfig) -> Result<()> {
    let mut state = State::new(repos);
    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(&mut state, key, cfg);
        }
        Ok(state.quit)
    })
}

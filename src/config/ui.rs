use crate::config::AppConfig;
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use inquire::Select;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

// Fields that must be chosen from a fixed list — no free text allowed.
const ENUM_FIELDS: &[(&str, &[&str])] = &[
    ("default_provider",   &["github", "gitlab", "gitea", "forgejo"]),
    ("default_visibility", &["private", "public"]),
    ("share_stats",        &["false", "true"]),
    ("gitignore_template", &["node", "rust", "python", "blank"]),
];

fn enum_options(key: &str) -> Option<&'static [&'static str]> {
    ENUM_FIELDS.iter().find(|(k, _)| *k == key).map(|(_, opts)| *opts)
}

// ── TUI state ─────────────────────────────────────────────────────────────────

enum Action {
    Quit,
    OpenPicker { field_idx: usize, options: Vec<&'static str> },
}

struct State {
    fields:   Vec<(&'static str, String)>,
    selected: usize,
    editing:  bool,
    edit_buf: String,
    action:   Option<Action>,
}

impl State {
    fn new(cfg: &AppConfig) -> Self {
        Self {
            fields:   cfg.fields(),
            selected: 0,
            editing:  false,
            edit_buf: String::new(),
            action:   None,
        }
    }

    fn to_config(&self, base: AppConfig) -> AppConfig {
        let mut cfg = base;
        let labels: Vec<&str> = self.fields.iter().map(|(k, _)| *k).collect();
        for (i, (_, val)) in self.fields.iter().enumerate() {
            let _ = cfg.set_field(labels[i], val.clone());
        }
        cfg
    }
}

// ── render ────────────────────────────────────────────────────────────────────

fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default()
        .title(" fxg — Configuration ")
        .borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = state.fields.iter().enumerate().map(|(i, (key, val))| {
        let active = i == state.selected;
        let is_enum = enum_options(key).is_some();

        let value_str = if active && state.editing {
            format!("{}_", state.edit_buf)
        } else {
            val.clone()
        };

        let value_style = if is_enum {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };

        let key_label = if is_enum {
            format!("{:<22} [pick]", key)
        } else {
            format!("{:<22}      ", key)
        };

        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(key_label, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(value_str, value_style),
        ]);

        let style = if active {
            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(line).style(style)
    }).collect();

    f.render_widget(List::new(items), chunks[0]);

    let hint = if state.editing {
        "  [Enter] confirm  [Esc] cancel  [Backspace] delete"
    } else {
        "  [↑/↓] navigate  [Enter] edit / pick  [r] reset defaults  [q] save & quit"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

// ── key handling ──────────────────────────────────────────────────────────────

fn handle_key(state: &mut State, key: KeyEvent) {
    if state.editing {
        match key.code {
            KeyCode::Enter => {
                state.fields[state.selected].1 = state.edit_buf.clone();
                state.editing = false;
                state.edit_buf.clear();
            }
            KeyCode::Esc => {
                state.editing = false;
                state.edit_buf.clear();
            }
            KeyCode::Char(c) => state.edit_buf.push(c),
            KeyCode::Backspace => { state.edit_buf.pop(); }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.action = Some(Action::Quit);
        }
        KeyCode::Char('r') => {
            let defaults = AppConfig::default().fields();
            state.fields = defaults;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if state.selected > 0 { state.selected -= 1; }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if state.selected + 1 < state.fields.len() { state.selected += 1; }
        }
        KeyCode::Enter => {
            let key_name = state.fields[state.selected].0;
            if let Some(opts) = enum_options(key_name) {
                // Signal the outer loop to open a picker
                state.action = Some(Action::OpenPicker {
                    field_idx: state.selected,
                    options:   opts.to_vec(),
                });
            } else {
                state.edit_buf = state.fields[state.selected].1.clone();
                state.editing = true;
            }
        }
        _ => {}
    }
}

// ── ratatui inner loop ────────────────────────────────────────────────────────

fn run_ratatui_loop(state: &mut State) -> Result<Action> {
    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(state, key);
        }
        Ok(state.action.is_some())
    })?;
    Ok(state.action.take().unwrap_or(Action::Quit))
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn run_config_tui(cfg: AppConfig) -> Result<AppConfig> {
    let mut state = State::new(&cfg);

    loop {
        match run_ratatui_loop(&mut state)? {
            Action::Quit => break,

            Action::OpenPicker { field_idx, options } => {
                // We are now back in normal terminal mode — inquire can draw.
                let current = &state.fields[field_idx].1;
                let start = options.iter().position(|o| *o == current.as_str()).unwrap_or(0);
                let key_name = state.fields[field_idx].0;
                match Select::new(&format!("{}:", key_name), options)
                    .with_starting_cursor(start)
                    .with_help_message("Use ↑/↓ to select, Enter to confirm, Esc to cancel")
                    .prompt_skippable()?
                {
                    Some(chosen) => { state.fields[field_idx].1 = chosen.to_string(); }
                    None         => {} // Esc — keep old value
                }
                // Loop continues — re-enter ratatui
            }
        }
    }

    Ok(state.to_config(cfg))
}

use crate::config::AppConfig;
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

struct State {
    fields:    Vec<(&'static str, String)>,
    selected:  usize,
    editing:   bool,
    edit_buf:  String,
    quit:      bool,
}

impl State {
    fn new(cfg: &AppConfig) -> Self {
        Self {
            fields:   cfg.fields(),
            selected: 0,
            editing:  false,
            edit_buf: String::new(),
            quit:     false,
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
        let value_str = if active && state.editing {
            format!("{}_", state.edit_buf)
        } else {
            val.clone()
        };
        let line = Line::from(vec![
            Span::raw(format!("  [{:2}]  ", i + 1)),
            Span::styled(format!("{:<22}", key), Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::raw(value_str),
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
        "  [↑/↓] navigate  [Enter] edit  [r] reset  [q] save & quit"
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        chunks[1],
    );
}

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
        KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
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
            state.edit_buf = state.fields[state.selected].1.clone();
            state.editing = true;
        }
        _ => {}
    }
}

pub fn run_config_tui(cfg: AppConfig) -> Result<AppConfig> {
    let mut state = State::new(&cfg);
    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(&mut state, key);
        }
        Ok(state.quit)
    })?;
    Ok(state.to_config(cfg))
}

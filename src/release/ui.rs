use crate::release::{format_changelog_md, ChangelogEntry};
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use git2::Repository;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

#[derive(PartialEq, Clone, Copy)]
enum Step { Version, ReleaseType, ChangelogReview, Confirm }

struct State {
    step:       Step,
    version:    String,
    rtype:      usize,       // index into RELEASE_TYPES
    changelog:  String,
    draft:      bool,
    input_buf:  String,
    quit:       bool,
    confirmed:  bool,
    scroll:     u16,
}

const RELEASE_TYPES: &[&str] = &["patch", "minor", "major"];
const STEPS: &[&str] = &["Version", "Type", "Changelog", "Confirm"];

impl State {
    fn new(version: String, changelog: String) -> Self {
        Self {
            step: Step::Version, version: version.clone(), rtype: 0,
            changelog, draft: false, input_buf: version,
            quit: false, confirmed: false, scroll: 0,
        }
    }
    fn next(&mut self) {
        self.step = match self.step {
            Step::Version       => { self.version = self.input_buf.clone(); Step::ReleaseType }
            Step::ReleaseType   => Step::ChangelogReview,
            Step::ChangelogReview => Step::Confirm,
            Step::Confirm       => { self.confirmed = true; self.quit = true; return; }
        };
    }
    fn prev(&mut self) {
        self.step = match self.step {
            Step::ReleaseType     => Step::Version,
            Step::ChangelogReview => Step::ReleaseType,
            Step::Confirm         => Step::ChangelogReview,
            _                     => return,
        };
    }
}

fn step_bar(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let idx = match state.step { Step::Version => 0, Step::ReleaseType => 1, Step::ChangelogReview => 2, Step::Confirm => 3 };
    let text = STEPS.iter().enumerate().map(|(i, s)| {
        if i == idx { format!(" [{}] ", s) } else { format!("  {}  ", s) }
    }).collect::<Vec<_>>().join("→");
    f.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        area,
    );
}

fn render_version(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Version tag ").borders(Borders::ALL);
    let text = format!("  > {}_ \n\n  e.g. v2.1.0", state.input_buf);
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn render_rtype(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Release type ").borders(Borders::ALL);
    let items: Vec<ListItem> = RELEASE_TYPES.iter().enumerate().map(|(i, t)| {
        let style = if i == state.rtype { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) }
                    else { Style::default() };
        ListItem::new(format!("  {}", t)).style(style)
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_changelog(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Changelog preview (↑/↓ scroll) ").borders(Borders::ALL);
    let lines: Vec<Line> = state.changelog.lines().map(|l| Line::from(l.to_string())).collect();
    f.render_widget(Paragraph::new(lines).block(block).scroll((state.scroll, 0)), area);
}

fn render_confirm(f: &mut Frame, state: &State, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Confirm release ").borders(Borders::ALL);
    let text = format!(
        "\n  Version : {}\n  Type    : {}\n  Draft   : {}\n\n  [Enter] publish   [d] toggle draft   [Esc] cancel",
        state.version, RELEASE_TYPES[state.rtype], state.draft
    );
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default().title(" fxg — New Release ").borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    step_bar(f, state, chunks[0]);

    match state.step {
        Step::Version         => render_version(f, state, chunks[1]),
        Step::ReleaseType     => render_rtype(f, state, chunks[1]),
        Step::ChangelogReview => render_changelog(f, state, chunks[1]),
        Step::Confirm         => render_confirm(f, state, chunks[1]),
    }

    f.render_widget(
        Paragraph::new("  [Enter] next  [b] back  [Esc] cancel")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn handle_key(state: &mut State, key: crossterm::event::KeyEvent) {
    match state.step {
        Step::Version => match key.code {
            KeyCode::Enter     => state.next(),
            KeyCode::Esc       => state.quit = true,
            KeyCode::Char('b') => state.prev(),
            KeyCode::Char(c)   => state.input_buf.push(c),
            KeyCode::Backspace => { state.input_buf.pop(); }
            _ => {}
        },
        Step::ReleaseType => match key.code {
            KeyCode::Enter     => state.next(),
            KeyCode::Esc       => state.quit = true,
            KeyCode::Char('b') => state.prev(),
            KeyCode::Up | KeyCode::Char('k') => { if state.rtype > 0 { state.rtype -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => { if state.rtype + 1 < RELEASE_TYPES.len() { state.rtype += 1; } }
            _ => {}
        },
        Step::ChangelogReview => match key.code {
            KeyCode::Enter     => state.next(),
            KeyCode::Esc       => state.quit = true,
            KeyCode::Char('b') => state.prev(),
            KeyCode::Up | KeyCode::Char('k')   => { if state.scroll > 0 { state.scroll -= 1; } }
            KeyCode::Down | KeyCode::Char('j') => state.scroll += 1,
            _ => {}
        },
        Step::Confirm => match key.code {
            KeyCode::Enter     => state.next(),
            KeyCode::Esc       => state.quit = true,
            KeyCode::Char('d') => state.draft = !state.draft,
            KeyCode::Char('b') => state.prev(),
            _ => {}
        },
    }
}

pub fn run_release_tui(
    _repo: &Repository,
    entries: Vec<ChangelogEntry>,
) -> Result<Option<(String, String, String, bool)>> {
    let version   = "v0.1.0".to_string();
    let changelog = format_changelog_md(&entries, &version);
    let mut state = State::new(version, changelog);

    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            handle_key(&mut state, key);
        }
        Ok(state.quit)
    })?;

    if state.confirmed {
        let body = format_changelog_md(&entries, &state.version);
        Ok(Some((state.version.clone(), state.version, body, state.draft)))
    } else {
        Ok(None)
    }
}

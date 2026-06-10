use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    Terminal,
};
use std::{io, time::Duration};

pub type Tty = Terminal<CrosstermBackend<io::Stdout>>;

/// Set up raw-mode alternate-screen terminal.
pub fn setup() -> Result<Tty> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

/// Restore terminal to normal mode.
pub fn teardown(mut terminal: Tty) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Poll for the next crossterm event with a timeout.
pub fn next_event(timeout_ms: u64) -> Result<Option<Event>> {
    if event::poll(Duration::from_millis(timeout_ms))? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Return a centred `Rect` inside `area` using percentage sizes.
#[allow(dead_code)]
pub fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}

/// Run `draw_fn` inside a properly-torn-down terminal.
/// The closure returns `true` to quit, `false` to keep running.
pub fn run_loop<F>(mut draw_fn: F) -> Result<()>
where
    F: FnMut(&mut Tty) -> Result<bool>,
{
    let mut terminal = setup()?;
    let result = (|| -> Result<()> {
        loop {
            if draw_fn(&mut terminal)? {
                break;
            }
        }
        Ok(())
    })();
    teardown(terminal)?;
    result
}

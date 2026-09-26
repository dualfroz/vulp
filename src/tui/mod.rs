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

pub fn setup() -> Result<Tty> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

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

pub fn leave_tui(terminal: &mut Tty) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn enter_tui(terminal: &mut Tty) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.hide_cursor()?;
    Ok(())
}

pub fn install_suspend_handler() {
    use signal_hook::consts::{SIGCONT, SIGTSTP};
    use signal_hook::iterator::Signals;

    let mut signals = match Signals::new([SIGTSTP, SIGCONT]) {
        Ok(s) => s,
        Err(_) => return,
    };
    std::thread::spawn(move || {
        for sig in signals.forever() {
            match sig {
                SIGTSTP => {
                    let mut stdout = io::stdout();
                    let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen);
                    let _ = disable_raw_mode();
                    unsafe { libc::raise(libc::SIGSTOP) };
                }
                SIGCONT => {
                    let _ = enable_raw_mode();
                    let mut stdout = io::stdout();
                    let _ = execute!(stdout, EnterAlternateScreen, EnableMouseCapture);
                }
                _ => {}
            }
        }
    });
}

pub fn next_event(timeout_ms: u64) -> Result<Option<Event>> {
    if event::poll(Duration::from_millis(timeout_ms))? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

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

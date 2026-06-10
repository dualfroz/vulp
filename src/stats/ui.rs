use crate::stats::{CodeStats, SecWarning, TodoItem};
use crate::tui;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame,
};

#[derive(Clone, Copy, PartialEq)]
enum Tab { Overview, Security, Todos, Languages }

struct State {
    stats:  CodeStats,
    tab:    Tab,
    scroll: usize,
    quit:   bool,
}

const TABS: &[&str] = &["Overview", "Security", "TODOs", "Languages"];

fn tab_index(t: Tab) -> usize {
    match t { Tab::Overview => 0, Tab::Security => 1, Tab::Todos => 2, Tab::Languages => 3 }
}

fn make_bar(value: f64, max: f64, width: usize) -> String {
    if max == 0.0 { return " ".repeat(width); }
    let filled = ((value / max) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn render_overview(f: &mut Frame, stats: &CodeStats, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Fill(1)])
        .split(area);

    // Summary block
    let summary = vec![
        Line::from(vec![Span::raw("  Lines of code : "), Span::styled(stats.total_lines.to_string(), Style::default().fg(Color::Cyan))]),
        Line::from(vec![Span::raw("  Files         : "), Span::raw(stats.file_count.to_string())]),
        Line::from(vec![Span::raw("  Functions     : "), Span::raw(format!("{}  avg length: {:.1} lines", stats.function_count, stats.avg_fn_length))]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  TODOs: "),
            Span::styled(stats.todos.len().to_string(), Style::default().fg(Color::Yellow)),
            Span::raw("    Security warnings: "),
            Span::styled(stats.sec_warnings.len().to_string(),
                if stats.sec_warnings.is_empty() { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) }),
        ]),
    ];
    f.render_widget(Paragraph::new(summary).block(Block::default().title(" Summary ").borders(Borders::ALL)), chunks[0]);

    // Language bars
    let lang_block = Block::default().title(" Languages ").borders(Borders::ALL);
    let max_pct = stats.languages.iter().map(|(_, p)| *p).fold(0.0_f64, f64::max);
    let items: Vec<ListItem> = stats.languages.iter().take(8).map(|(lang, pct)| {
        let bar = make_bar(*pct, max_pct, 20);
        ListItem::new(format!("  {:15} {:5.1}%  {}", lang, pct, bar))
    }).collect();
    f.render_widget(List::new(items).block(lang_block), chunks[1]);
}

fn render_security(f: &mut Frame, warnings: &[SecWarning], scroll: usize, area: Rect) {
    let block = Block::default().title(format!(" Security — {} warning(s) ", warnings.len())).borders(Borders::ALL);
    let items: Vec<ListItem> = warnings.iter().skip(scroll).map(|w| {
        ListItem::new(format!("  ⚠  {}:{}  — {}", w.file.split('/').last().unwrap_or(&w.file), w.line, w.reason))
            .style(Style::default().fg(Color::Yellow))
    }).collect();
    f.render_widget(List::new(items).block(block), area);
}

fn render_todos(f: &mut Frame, todos: &[TodoItem], scroll: usize, area: Rect) {
    let block = Block::default().title(format!(" TODOs / FIXMEs — {} item(s) ", todos.len())).borders(Borders::ALL);
    let rows: Vec<Row> = todos.iter().skip(scroll).map(|t| {
        let kind_color = match t.kind.as_str() {
            "FIXME" => Color::Red, "HACK" => Color::Magenta, _ => Color::Yellow,
        };
        Row::new(vec![
            Cell::from(t.kind.clone()).style(Style::default().fg(kind_color)),
            Cell::from(t.file.split('/').last().unwrap_or(&t.file).to_string()),
            Cell::from(t.line.to_string()),
            Cell::from(t.text.clone()),
        ])
    }).collect();
    let widths = [Constraint::Length(7), Constraint::Length(22), Constraint::Length(6), Constraint::Fill(1)];
    let header = Row::new(["KIND", "FILE", "LINE", "TEXT"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(Table::new(rows, widths).header(header).block(block), area);
}

fn render_languages(f: &mut Frame, stats: &CodeStats, area: Rect) {
    let block = Block::default().title(" Language breakdown ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let _bar_height: u16 = 3;
    let bars: Vec<_> = stats.languages.iter().take(6).collect();
    if bars.is_empty() { return; }

    let constraints: Vec<Constraint> = bars.iter().map(|_| Constraint::Fill(1)).collect();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(inner);

    let max = bars.iter().map(|(_, p)| *p).fold(0.0_f64, f64::max);
    for (i, (lang, pct)) in bars.iter().enumerate() {
        let gauge = Gauge::default()
            .block(Block::default().title(lang.as_str()))
            .gauge_style(Style::default().fg(Color::Cyan))
            .ratio((pct / max).min(1.0));
        f.render_widget(gauge, cols[i]);
    }
}

fn render(f: &mut Frame, state: &State) {
    let area = f.area();
    let outer = Block::default().title(" fxg stats ").borders(Borders::ALL);
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Fill(1), Constraint::Length(1)])
        .split(inner);

    // Tab bar
    let tab_titles: Vec<Line> = TABS.iter().map(|t| Line::from(*t)).collect();
    f.render_widget(
        Tabs::new(tab_titles).select(tab_index(state.tab))
            .style(Style::default())
            .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );

    match state.tab {
        Tab::Overview  => render_overview(f, &state.stats, chunks[1]),
        Tab::Security  => render_security(f, &state.stats.sec_warnings, state.scroll, chunks[1]),
        Tab::Todos     => render_todos(f, &state.stats.todos, state.scroll, chunks[1]),
        Tab::Languages => render_languages(f, &state.stats, chunks[1]),
    }

    f.render_widget(
        Paragraph::new("  [1-4] tabs  [j/k] scroll  [e] export  [q] quit")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

pub fn run_stats_tui(stats: CodeStats) -> Result<()> {
    let mut state = State { stats, tab: Tab::Overview, scroll: 0, quit: false };
    tui::run_loop(|terminal| {
        terminal.draw(|f| render(f, &state))?;
        if let Some(Event::Key(key)) = tui::next_event(250)? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => state.quit = true,
                KeyCode::Tab | KeyCode::Right      => state.tab = match state.tab { Tab::Overview => Tab::Security, Tab::Security => Tab::Todos, Tab::Todos => Tab::Languages, Tab::Languages => Tab::Overview },
                KeyCode::BackTab | KeyCode::Left   => state.tab = match state.tab { Tab::Overview => Tab::Languages, Tab::Security => Tab::Overview, Tab::Todos => Tab::Security, Tab::Languages => Tab::Todos },
                KeyCode::Char('1') => { state.tab = Tab::Overview; state.scroll = 0; }
                KeyCode::Char('2') => { state.tab = Tab::Security; state.scroll = 0; }
                KeyCode::Char('3') => { state.tab = Tab::Todos;    state.scroll = 0; }
                KeyCode::Char('4') => { state.tab = Tab::Languages; state.scroll = 0; }
                KeyCode::Down | KeyCode::Char('j') => state.scroll += 1,
                KeyCode::Up   | KeyCode::Char('k') => { if state.scroll > 0 { state.scroll -= 1; } }
                _ => {}
            }
        }
        Ok(state.quit)
    })
}

mod render_modal;
mod render_provider_tabs;
mod render_side;
mod render_tabs;

use super::{St, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, st: &St) {
    let area = f.area();

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(3)])
        .split(area);

    render_topbar(f, st, vert[0]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(73), Constraint::Percentage(27)])
        .split(vert[1]);

    let left_blk  = Block::default().borders(Borders::ALL);
    let left_inner = left_blk.inner(cols[0]);
    f.render_widget(left_blk, cols[0]);

    match st.tab {
        Tab::Overview  => render_tabs::render_overview(f, st, left_inner),
        Tab::Commits   => render_tabs::render_commits(f, st, left_inner),
        Tab::Branches  => render_tabs::render_branches(f, st, left_inner),
        Tab::Files     => render_tabs::render_file_tree(f, st, left_inner),
        Tab::Stats     => render_tabs::render_stats(f, st, left_inner),
        Tab::Settings  => render_tabs::render_settings(f, st, left_inner),
        Tab::PRs       => render_provider_tabs::render_prs(f, st, left_inner),
        Tab::Issues    => render_provider_tabs::render_issues(f, st, left_inner),
        Tab::Todos     => render_provider_tabs::render_repo_todos(f, st, left_inner),
    }

    render_side::render_right(f, st, cols[1]);
    render_bottombar(f, st, vert[2]);

    render_modal::render_modal(f, st, area);
}

fn render_topbar(f: &mut Frame, st: &St, area: Rect) {
    let r = &st.summary;

    let tab_s = |label: &str, on: bool| -> Span<'static> {
        let label = label.to_string();
        if on {
            Span::styled(format!(" {} ", label), Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(format!(" {} ", label), Style::default().fg(Color::DarkGray))
        }
    };

    let sync_span = match (r.ahead, r.behind) {
        (0, 0) => Span::styled(" ✓", Style::default().fg(Color::DarkGray)),
        (a, 0) => Span::styled(format!(" ↑{}", a), Style::default().fg(Color::Cyan)),
        (0, b) => Span::styled(format!(" ↓{}", b), Style::default().fg(Color::Red)),
        (a, b) => Span::styled(format!(" ↑{} ↓{}", a, b), Style::default().fg(Color::Yellow)),
    };
    let dirty_span = if r.is_dirty {
        Span::styled("  ⚠ dirty", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("  ✓ clean", Style::default().fg(Color::DarkGray))
    };

    let mut spans = vec![
        Span::styled(" ← ", Style::default().fg(Color::DarkGray)),
        Span::styled(r.name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  ⎇ {}", r.branch), Style::default().fg(Color::Cyan)),
        sync_span,
        dirty_span,
        Span::raw("   "),
        tab_s("1 Overview",  st.tab == Tab::Overview),
        Span::raw(" "),
        tab_s("2 Commits",   st.tab == Tab::Commits),
        Span::raw(" "),
        tab_s("3 Branches",  st.tab == Tab::Branches),
        Span::raw(" "),
        tab_s("4 Files",     st.tab == Tab::Files),
        Span::raw(" "),
        tab_s("5 Stats",     st.tab == Tab::Stats),
        Span::raw(" "),
        tab_s("6 Settings",  st.tab == Tab::Settings),
        Span::raw(" "),
        tab_s("7 PRs",       st.tab == Tab::PRs),
        Span::raw(" "),
        tab_s("8 Issues",    st.tab == Tab::Issues),
        Span::raw(" "),
        tab_s("9 Todos",     st.tab == Tab::Todos),
    ];

    if let Some(ref msg) = st.status_msg {
        spans.push(Span::styled(
            format!("   {}", msg),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_bottombar(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let key = |k: &str| Span::styled(format!("[{}] ", k), dim);

    let line2: Line = match st.tab {
        Tab::Overview  => Line::from(vec![key("↑↓"), Span::styled("scroll", dim)]),
        Tab::Commits   => Line::from(vec![
            key("↑↓/jk"), Span::styled("scroll  ", dim),
            key("PgUp/Dn"), Span::styled("fast", dim),
        ]),
        Tab::Branches  => Line::from(vec![
            key("↑↓"), Span::styled("select  ", dim),
            key("Enter"), Span::styled("checkout  ", dim),
            key("n"), Span::styled("new (modal)  ", dim),
            key("P"), Span::styled("push  ", dim),
            key("D"), Span::styled("delete", dim),
        ]),
        Tab::Files     => Line::from(vec![
            key("↑↓"), Span::styled("navigate  ", dim),
            key("Enter"), Span::styled("expand/collapse  ", dim),
            key("e"), Span::styled("edit in $EDITOR  ", dim),
            key("[/]"), Span::styled("scroll preview", dim),
        ]),
        Tab::Stats     => Line::from(vec![
            key("p"), Span::styled("cycle period  ", dim),
            key("</>"), Span::styled("prev/next period  ", dim),
            key("↑↓"), Span::styled("scroll contributors", dim),
        ]),
        Tab::Settings  => Line::from(vec![
            key("↑↓"), Span::styled("select  ", dim),
            key("Enter"), Span::styled("edit/action", dim),
        ]),
        Tab::PRs | Tab::Issues => Line::from(vec![
            key("↑↓"), Span::styled("select  ", dim),
            key("Enter"), Span::styled("open in browser", dim),
        ]),
        Tab::Todos => Line::from(vec![
            key("↑↓"), Span::styled("select  ", dim),
            key("Enter"), Span::styled("cycle status", dim),
        ]),
    };

    f.render_widget(Paragraph::new(vec![
        Line::from(vec![
            key("←→/Tab"), Span::styled("switch tab  ", dim),
            key("1-9"),     Span::styled("direct  ", dim),
            key("q/Esc"),   Span::styled("back  ", dim),
            key("s"),       Span::styled("sync  ", dim),
            key("r"),       Span::styled("release  ", dim),
            key("W"),       Span::styled("browser", dim),
        ]),
        line2,
        Line::from(""),
    ]), area);
}

pub fn dsep(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        " ".to_string() + &"─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    ))
}

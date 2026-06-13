use super::{HistEntry, St, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

pub fn draw(f: &mut Frame, st: &St) {
    let area = f.area();

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Fill(1), Constraint::Length(1)])
        .split(area);

    render_header(f, st, vert[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(24)])
        .split(vert[1]);

    match st.tab {
        Tab::Overview => render_overview(f, st, body[0]),
        Tab::History  => render_history(f, st, body[0]),
        Tab::Files    => render_files(f, st, body[0]),
    }
    render_sidebar(f, st, body[1]);

    let hint = if let Some(ref msg) = st.status_msg {
        format!("  {}  — press any key to continue", msg)
    } else {
        match st.tab {
            Tab::Overview => "  [↑↓/jk] scroll  [Tab/1-3] tab  [q] back to hub".into(),
            Tab::History  => "  [↑↓/jk] scroll  [PgUp/Dn] fast  [Tab/1-3] tab  [q] back".into(),
            Tab::Files    => "  [a] toggle all/changed  [↑↓] scroll  [g] edit .gitignore  [q] back".into(),
        }
    };
    f.render_widget(
        Paragraph::new(hint.as_str()).style(Style::default().fg(Color::DarkGray)),
        vert[2],
    );
}

fn render_header(f: &mut Frame, st: &St, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let r = &st.summary;
    let owner_s  = r.owner.as_deref().map(|o| format!("  @{}", o)).unwrap_or_default();
    let stars_s  = r.stars.map(|s| format!("  ★ {}", s)).unwrap_or_default();
    let issues_s = r.open_issues.map(|i| if i > 0 { format!("  ⚠ {} issues", i) } else { String::new() }).unwrap_or_default();
    let prs_s    = r.open_prs.map(|p| if p > 0 { format!("  ↔ {} PRs", p) } else { String::new() }).unwrap_or_default();

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {}", r.name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(owner_s, Style::default().fg(Color::DarkGray)),
            Span::styled(format!("  ⎇ {}", r.branch), Style::default().fg(Color::Cyan)),
            if r.is_dirty { Span::styled("  ⚠ dirty", Style::default().fg(Color::Yellow)) }
            else          { Span::styled("  ✓ clean", Style::default().fg(Color::DarkGray)) },
            Span::styled(stars_s,  Style::default().fg(Color::Yellow)),
            Span::styled(issues_s, Style::default().fg(Color::Red)),
            Span::styled(prs_s,    Style::default().fg(Color::Cyan)),
        ])),
        rows[0],
    );

    let tab_s = |label: &str, on: bool| {
        if on { Span::styled(format!(" {} ", label), Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)) }
        else  { Span::styled(format!(" {} ", label), Style::default().fg(Color::DarkGray)) }
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            tab_s("[1] Overview", st.tab == Tab::Overview),
            Span::raw("  "),
            tab_s("[2] History",  st.tab == Tab::History),
            Span::raw("  "),
            tab_s("[3] Files",    st.tab == Tab::Files),
        ])),
        rows[1],
    );
}

fn render_sidebar(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default().title(" Actions ").borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let act = |key: &'static str, label: &'static str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" [{}]", key), Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", label), Style::default().fg(Color::White)),
        ])
    };
    let sep = || Line::from(Span::styled(" ─────────────────", Style::default().fg(Color::DarkGray)));
    let stat_line = |label: &str, val: String, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" {:<10}", label), Style::default().fg(Color::DarkGray)),
            Span::styled(val, Style::default().fg(col)),
        ])
    };

    let mut lines: Vec<Line> = vec![
        stat_line("Branch",   st.summary.branch.chars().take(11).collect(), Color::Cyan),
        stat_line("Tags",     st.total_tags.to_string(), Color::Yellow),
        stat_line("Stashes",  st.stash_count.to_string(), Color::DarkGray),
        stat_line("Branches", st.branch_count.to_string(), Color::DarkGray),
        sep(),
        act("s", "Sync",       Color::Green),
        act("r", "Release",    Color::Green),
        sep(),
        act("o", "Editor",     Color::Cyan),
        act("W", "Browser",    Color::Cyan),
        act("y", "Copy path",  Color::Cyan),
        act("u", "Copy URL",   Color::Cyan),
        act("g", ".gitignore", Color::Cyan),
        sep(),
        act("c", "New branch", Color::White),
        act("S", "Stash",      Color::White),
        act("A", "Pop stash",  Color::White),
        sep(),
        act("q", "← Back",    Color::DarkGray),
    ];

    if let Some(ref msg) = st.status_msg {
        lines.push(sep());
        let trunc: String = msg.chars().take(20).collect();
        lines.push(Line::from(Span::styled(format!(" {}", trunc), Style::default().fg(Color::Green))));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_overview(f: &mut Frame, st: &St, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);

    let r = &st.summary;

    let mut all_lines: Vec<Line> = vec![section("Identity"), Line::from("")];
    all_lines.push(kv("Path",        &r.path.to_string_lossy()));
    all_lines.push(kv("Owner",       r.owner.as_deref().unwrap_or("—")));
    all_lines.push(kv("Visibility",  r.visibility.as_deref().unwrap_or("unknown")));
    all_lines.push(kv("Language",    r.lang.as_deref().unwrap_or("—")));
    all_lines.push(kv("License",     if r.license { "✓ present" } else { "⚠ not found" }));
    if let Some(s) = r.stars       { all_lines.push(kv("Stars",     &format!("★ {}", s))); }
    if let Some(i) = r.open_issues { all_lines.push(kv("Issues",    &format!("{} open", i))); }
    if let Some(p) = r.open_prs    { all_lines.push(kv("Open PRs",  &format!("{}", p))); }
    all_lines.push(Line::from(""));

    all_lines.push(section("Status"));
    all_lines.push(Line::from(""));
    let branch_detail = match (r.ahead, r.behind) {
        (0, 0) => " (in sync)".to_string(),
        (a, 0) => format!(" ↑{}", a),
        (0, b) => format!(" ↓{}", b),
        (a, b) => format!(" ↑{} ↓{}", a, b),
    };
    all_lines.push(Line::from(vec![
        Span::styled("  Branch       ", Style::default().fg(Color::DarkGray)),
        Span::styled(r.branch.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(branch_detail, Style::default().fg(Color::DarkGray)),
    ]));
    let (dirty_txt, dirty_col) = if r.is_dirty {
        (format!("⚠ {} file(s)", st.files.len()), Color::Yellow)
    } else { ("✓ clean".into(), Color::DarkGray) };
    all_lines.push(Line::from(vec![
        Span::styled("  Changes      ", Style::default().fg(Color::DarkGray)),
        Span::styled(dirty_txt, Style::default().fg(dirty_col)),
    ]));
    all_lines.push(Line::from(""));

    all_lines.push(section("Repository"));
    all_lines.push(Line::from(""));
    all_lines.push(kv("Files",        &st.file_count.to_string()));
    all_lines.push(kv("Branches",     &st.branch_count.to_string()));
    all_lines.push(kv("Contributors", &st.contributors.to_string()));
    all_lines.push(kv("Stashes",      &st.stash_count.to_string()));
    all_lines.push(kv("Tags",         &st.total_tags.to_string()));
    all_lines.push(Line::from(""));

    all_lines.push(section("Last commit"));
    all_lines.push(Line::from(""));
    all_lines.push(kv("When",    &r.last_commit_age));
    let msg_trunc: String = r.last_commit_msg.chars().take(46).collect();
    all_lines.push(Line::from(vec![
        Span::styled("  Message      ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("\"{}\"", msg_trunc), Style::default().fg(Color::White)),
    ]));

    let skip = st.ov_scroll.min(all_lines.len().saturating_sub(1));
    let visible: Vec<Line> = all_lines.into_iter().skip(skip).collect();

    f.render_widget(
        Paragraph::new(visible).block(Block::default().borders(Borders::RIGHT)),
        cols[0],
    );

    let mut right: Vec<Line> = vec![section("Recent commits"), Line::from("")];
    for (age, msg) in &st.recent_commits {
        right.push(Line::from(vec![
            Span::styled(format!("  {:>8}  ", age), Style::default().fg(Color::DarkGray)),
            Span::raw(msg.chars().take(cols[1].width.saturating_sub(14) as usize).collect::<String>()),
        ]));
    }
    if !st.files.is_empty() {
        right.push(Line::from(""));
        right.push(section("Changed files"));
        right.push(Line::from(""));
        if st.file_count > 500 {
            right.push(Line::from(Span::styled(
                format!("  ⚠ {} tracked — check .gitignore!", st.file_count),
                Style::default().fg(Color::Yellow),
            )));
        }
        for (ch, path) in st.files.iter().take(10) {
            let col = match ch { 'M' => Color::Yellow, 'D' => Color::Red, _ => Color::Green };
            right.push(Line::from(vec![
                Span::styled(format!("  {} ", ch), Style::default().fg(col)),
                Span::styled(
                    path.chars().take(cols[1].width.saturating_sub(6) as usize).collect::<String>(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        if st.files.len() > 10 {
            right.push(Line::from(Span::styled(
                format!("  … {} more", st.files.len() - 10),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    f.render_widget(Paragraph::new(right), cols[1]);
}

fn render_history(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default()
        .title(format!(" History ({} entries) ", st.history.len()))
        .borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let avail  = inner.height.saturating_sub(1) as usize;
    let header = Row::new(vec!["TYPE", "REF", "AGE", "AUTHOR / TAG MSG", "MESSAGE"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = st.history.iter()
        .skip(st.hist_off).take(avail)
        .map(|entry| match entry {
            HistEntry::Tag { name, age, msg, .. } => {
                Row::new(vec![
                    Cell::from("🔖 tag").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Cell::from(name.clone()).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Cell::from(age.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(""),
                    Cell::from(if msg.is_empty() { "(lightweight tag)".to_string() } else { msg.clone() })
                        .style(Style::default().fg(Color::DarkGray)),
                ]).style(Style::default().bg(Color::Reset))
            }
            HistEntry::Commit { sha, age, author, msg, .. } => {
                Row::new(vec![
                    Cell::from("●").style(Style::default().fg(Color::DarkGray)),
                    Cell::from(sha.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(age.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(format!("@{}", author)).style(Style::default().fg(Color::Cyan)),
                    Cell::from(msg.clone()),
                ])
            }
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Fill(1),
        ]).header(header),
        inner,
    );
}

fn render_files(f: &mut Frame, st: &St, area: Rect) {
    let title: &str = if st.show_all_files {
        " All tracked files — [a] show changed "
    } else {
        " Changed files — [a] show all tracked "
    };

    let blk   = Block::default().title(title).borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let mut warn_rows = 0usize;
    if st.file_count > 500 && !st.show_all_files {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  ⚠ {} tracked files — large repo, consider .gitignore  [g] edit it", st.file_count),
                Style::default().fg(Color::Yellow),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        warn_rows = 1;
    }

    let usable = Rect::new(inner.x, inner.y + warn_rows as u16, inner.width, inner.height.saturating_sub(warn_rows as u16));

    if st.show_all_files {
        let avail = usable.height as usize;
        let items: Vec<ListItem> = st.all_files.iter()
            .skip(st.file_off).take(avail)
            .map(|p| ListItem::new(Line::from(Span::styled(format!("  {}", p), Style::default().fg(Color::DarkGray)))))
            .collect();
        let count = Line::from(Span::styled(
            format!("  {} files total  (skip: {})", st.all_files.len(), st.file_off),
            Style::default().fg(Color::DarkGray),
        ));
        let mut all_items = vec![ListItem::new(count)];
        all_items.extend(items);
        f.render_widget(List::new(all_items), usable);
    } else {
        if st.files.is_empty() {
            f.render_widget(
                Paragraph::new("  (working tree clean)").style(Style::default().fg(Color::DarkGray)),
                usable,
            );
            return;
        }
        let avail = usable.height.saturating_sub(1) as usize;
        let rows: Vec<Row> = st.files.iter()
            .skip(st.file_off).take(avail)
            .map(|(ch, p)| {
                let col = match ch { 'M' => Color::Yellow, 'D' => Color::Red, _ => Color::Green };
                Row::new(vec![
                    Cell::from(ch.to_string()).style(Style::default().fg(col)),
                    Cell::from(p.clone()),
                ])
            })
            .collect();
        f.render_widget(Table::new(rows, [Constraint::Length(3), Constraint::Fill(1)]), usable);
    }
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {}", title),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ))
}

fn kv(key: &str, val: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<15}", key), Style::default().fg(Color::DarkGray)),
        Span::raw(val.to_string()),
    ])
}

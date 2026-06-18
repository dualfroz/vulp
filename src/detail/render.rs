use super::{DetailModal, HistEntry, St, StatsPeriod, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table},
    Frame,
};

// ── top-level draw ────────────────────────────────────────────────────────────

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
        Tab::Overview  => render_overview(f, st, left_inner),
        Tab::Commits   => render_commits(f, st, left_inner),
        Tab::Branches  => render_branches(f, st, left_inner),
        Tab::Files     => render_file_tree(f, st, left_inner),
        Tab::Stats     => render_stats(f, st, left_inner),
        Tab::Settings  => render_settings(f, st, left_inner),
        Tab::PRs       => render_prs(f, st, left_inner),
        Tab::Issues    => render_issues(f, st, left_inner),
        Tab::Todos     => render_repo_todos(f, st, left_inner),
    }

    render_right(f, st, cols[1]);
    render_bottombar(f, st, vert[2]);

    // Modal overlay — rendered last so it's on top
    render_modal(f, st, area);
}

// ── top bar ───────────────────────────────────────────────────────────────────

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

// ── 1. Overview tab ───────────────────────────────────────────────────────────

fn render_overview(f: &mut Frame, st: &St, area: Rect) {
    let r   = &st.summary;
    let dim = Style::default().fg(Color::DarkGray);
    let wht = Style::default().fg(Color::White);
    let cyn = Style::default().fg(Color::Cyan);
    let yel = Style::default().fg(Color::Yellow);
    let w   = area.width.saturating_sub(2) as usize;

    let kv = |k: &str, v: &str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {:<16}", k), dim),
            Span::styled(v.chars().take(w.saturating_sub(18)).collect::<String>(), Style::default().fg(col)),
        ])
    };
    let sep = || dsep(area.width);

    let mut lines: Vec<Line> = vec![];

    // ── location & links ──
    lines.push(Line::from(Span::styled("  Location & Links", yel.add_modifier(Modifier::BOLD))));
    lines.push(sep());
    lines.push(kv("Path", &st.summary.path.to_string_lossy(), Color::DarkGray));
    if let Some(ref url) = st.remote_url_str {
        lines.push(kv("GitHub URL", url, Color::Cyan));
    } else {
        lines.push(kv("GitHub URL", "(no remote)", Color::DarkGray));
    }
    lines.push(Line::from(""));

    // ── repository stats ──
    lines.push(Line::from(Span::styled("  Repository", yel.add_modifier(Modifier::BOLD))));
    lines.push(sep());

    let dirty_txt = if r.is_dirty { format!("⚠ {} changed", st.files.len()) } else { "✓ clean".into() };
    let sync_txt  = match (r.ahead, r.behind) {
        (0, 0) => "in sync".to_string(),
        (a, 0) => format!("↑{} ahead", a),
        (0, b) => format!("↓{} behind", b),
        (a, b) => format!("↑{} ↓{}", a, b),
    };
    let age_txt = if st.repo_age_days > 365 {
        format!("{} years old", st.repo_age_days / 365)
    } else if st.repo_age_days > 30 {
        format!("{} months old", st.repo_age_days / 30)
    } else {
        format!("{} days old", st.repo_age_days)
    };

    lines.push(kv("Total commits", &st.total_commits.to_string(), Color::White));
    lines.push(kv("Total releases", &st.total_tags.to_string(), Color::Yellow));
    lines.push(kv("Branches", &st.branch_count.to_string(), Color::White));
    lines.push(kv("Tracked files", &st.file_count.to_string(), Color::DarkGray));
    lines.push(kv("Working tree", &dirty_txt, if r.is_dirty { Color::Yellow } else { Color::DarkGray }));
    lines.push(kv("Remote", &sync_txt, if r.behind > 0 { Color::Red } else if r.ahead > 0 { Color::Cyan } else { Color::DarkGray }));
    lines.push(kv("Age", &age_txt, Color::DarkGray));
    if let Some(lang) = &r.lang {
        lines.push(kv("Languages", lang, Color::DarkGray));
    }
    lines.push(Line::from(""));

    // ── latest version + recent releases ──
    let tags: Vec<_> = st.history.iter().filter_map(|h| {
        if let HistEntry::Tag { name, age, .. } = h { Some((name.clone(), age.clone())) } else { None }
    }).collect();

    if !tags.is_empty() {
        lines.push(Line::from(Span::styled("  Releases", yel.add_modifier(Modifier::BOLD))));
        lines.push(sep());
        for (name, age) in tags.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<11}  ", age), dim),
                Span::styled(name.clone(), cyn),
            ]));
        }
        lines.push(Line::from(""));
    }

    // ── recent commits ──
    if !st.ov_recent.is_empty() {
        lines.push(Line::from(Span::styled("  Recent Commits", yel.add_modifier(Modifier::BOLD))));
        lines.push(sep());
        let skip = st.ov_scroll.min(st.ov_recent.len().saturating_sub(1));
        for (age, msg) in st.ov_recent.iter().skip(skip).take(5) {
            let msg_t: String = msg.chars().take(w.saturating_sub(14)).collect();
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<11}  ", age), dim),
                Span::styled(msg_t, wht),
            ]));
        }
        lines.push(Line::from(""));
    }

    // ── top contributors ──
    if !st.contributors.is_empty() {
        lines.push(Line::from(Span::styled("  Top Contributors", yel.add_modifier(Modifier::BOLD))));
        lines.push(sep());
        for c in st.contributors.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<25}", c.name.chars().take(25).collect::<String>()), wht),
                Span::styled(format!("{} commits", c.count), dim),
            ]));
        }
        lines.push(Line::from(""));
    }

    // ── active branches ──
    if !st.branches.is_empty() {
        lines.push(Line::from(Span::styled("  Active Branches", yel.add_modifier(Modifier::BOLD))));
        lines.push(sep());
        for b in st.branches.iter().take(4) {
            let marker = if b.is_current { "▶ " } else { "  " };
            let sync = match (b.ahead, b.behind) {
                (0, 0) => String::new(),
                (a, 0) => format!(" ↑{}", a),
                (0, b) => format!(" ↓{}", b),
                (a, b) => format!(" ↑{}↓{}", a, b),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", marker), if b.is_current { cyn } else { dim }),
                Span::styled(b.name.chars().take(25).collect::<String>(), if b.is_current { cyn } else { wht }),
                Span::styled(sync, dim),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), area);
}

// ── 2. Commits tab ───────────────────────────────────────────────────────────

fn render_commits(f: &mut Frame, st: &St, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let ahead  = st.summary.ahead; // first N commits are local-only
    let header = Row::new(vec!["", "SHA", "AGE", "AUTHOR", "MESSAGE"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let mut commit_idx = 0usize; // count Commit entries seen so far (for ahead marker)
    let rows: Vec<Row> = st.history.iter()
        .skip(st.hist_off)
        .take(avail)
        .map(|entry| {
            match entry {
                HistEntry::Tag { name, age, msg, .. } => Row::new(vec![
                    Cell::from("tag").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Cell::from(name.clone()).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Cell::from(age.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(""),
                    Cell::from(if msg.is_empty() { "(lightweight tag)".into() } else { msg.clone() })
                        .style(Style::default().fg(Color::DarkGray)),
                ]),
                HistEntry::Commit { sha, age, author, msg, .. } => {
                    let is_local = commit_idx < ahead;
                    commit_idx += 1;
                    let marker = if is_local { "↑" } else { "●" };
                    let marker_col = if is_local { Color::Cyan } else { Color::DarkGray };
                    let msg_col    = if is_local { Color::White } else { Color::Reset };
                    Row::new(vec![
                        Cell::from(marker).style(Style::default().fg(marker_col)),
                        Cell::from(sha.clone()).style(Style::default().fg(Color::DarkGray)),
                        Cell::from(age.clone()).style(Style::default().fg(Color::DarkGray)),
                        Cell::from(format!("@{}", author)).style(Style::default().fg(Color::Cyan)),
                        Cell::from(msg.clone()).style(Style::default().fg(msg_col)),
                    ])
                }
            }
        })
        .collect();

    // Legend for local-only indicator
    let legend = if ahead > 0 {
        Rect::new(area.x, area.y + area.height.saturating_sub(1), area.width, 1)
    } else {
        Rect::new(0, 0, 0, 0)
    };
    let table_area = if ahead > 0 {
        Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1))
    } else {
        area
    };

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(2),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Fill(1),
        ]).header(header),
        table_area,
    );

    if ahead > 0 {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ↑ ", Style::default().fg(Color::Cyan)),
                Span::styled(format!("{} local-only commit(s) not yet pushed", ahead), Style::default().fg(Color::DarkGray)),
            ])),
            legend,
        );
    }
}

// ── 3. Branches tab ──────────────────────────────────────────────────────────

fn render_branches(f: &mut Frame, st: &St, area: Rect) {
    if st.branches.is_empty() {
        f.render_widget(
            Paragraph::new("  No branches found").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let avail  = area.height.saturating_sub(1) as usize;
    let header = Row::new(vec!["", "BRANCH", "UPSTREAM", "+", "-", "AGE", "LAST COMMIT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let scroll = st.branch_cursor.saturating_sub(avail.saturating_sub(1));

    let rows: Vec<Row> = st.branches.iter()
        .enumerate()
        .skip(scroll)
        .take(avail)
        .map(|(i, b)| {
            let is_sel  = i + scroll == st.branch_cursor;
            let sel_bg  = if is_sel { Color::DarkGray } else { Color::Reset };
            let name_col= if b.is_current { Color::Cyan } else { Color::White };
            let cur_mark= if b.is_current { "▶" } else { " " };
            let ahead_col = if b.ahead  > 0 { Color::Green } else { Color::DarkGray };
            let behind_col= if b.behind > 0 { Color::Red }   else { Color::DarkGray };

            let msg: String = b.last_commit_msg.chars()
                .take(area.width.saturating_sub(70).max(10) as usize).collect();

            Row::new(vec![
                Cell::from(cur_mark).style(Style::default().fg(Color::Cyan).bg(sel_bg)),
                Cell::from(b.name.clone()).style(Style::default().fg(name_col).bg(sel_bg).add_modifier(if b.is_current { Modifier::BOLD } else { Modifier::empty() })),
                Cell::from(b.upstream.as_deref().unwrap_or("—")).style(Style::default().fg(Color::DarkGray).bg(sel_bg)),
                Cell::from(if b.ahead  > 0 { b.ahead.to_string()  } else { "—".into() }).style(Style::default().fg(ahead_col).bg(sel_bg)),
                Cell::from(if b.behind > 0 { b.behind.to_string() } else { "—".into() }).style(Style::default().fg(behind_col).bg(sel_bg)),
                Cell::from(b.last_commit_age.clone()).style(Style::default().fg(Color::DarkGray).bg(sel_bg)),
                Cell::from(msg).style(Style::default().fg(Color::DarkGray).bg(sel_bg)),
            ])
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(2),
            Constraint::Length(24),
            Constraint::Length(18),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Fill(1),
        ]).header(header),
        area,
    );
}

// ── 4. Files tab (tree) ───────────────────────────────────────────────────────

fn render_file_tree(f: &mut Frame, st: &St, area: Rect) {
    if st.tree_visible.is_empty() {
        f.render_widget(
            Paragraph::new("  No tracked files").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let avail  = area.height as usize;
    let scroll = st.tree_cursor.saturating_sub(avail.saturating_sub(1));

    let dirty: std::collections::HashSet<&str> = st.files.iter().map(|(_, p)| p.as_str()).collect();

    let items: Vec<ListItem> = st.tree_visible.iter()
        .skip(scroll)
        .take(avail)
        .enumerate()
        .map(|(row_i, &ni)| {
            let node   = &st.tree_nodes[ni];
            let is_sel = (scroll + row_i) == st.tree_cursor;
            let indent: String = "  ".repeat(node.depth);
            let sel_bg  = if is_sel { Color::DarkGray } else { Color::Reset };

            let (prefix, name_col) = if node.is_dir {
                let arrow = if node.expanded { "▼ " } else { "▶ " };
                (format!("{}{}", indent, arrow), Color::Yellow)
            } else {
                (format!("{}· ", indent), Color::White)
            };

            let name_str = if node.is_dir { format!("{}/", node.name) } else { node.name.clone() };

            let dirty_mark = if !node.is_dir && dirty.contains(node.rel_path.as_str()) {
                let status = st.files.iter().find(|(_, p)| p == &node.rel_path).map(|(c, _)| *c);
                match status {
                    Some('M') => Span::styled(" M", Style::default().fg(Color::Yellow)),
                    Some('D') => Span::styled(" D", Style::default().fg(Color::Red)),
                    _         => Span::styled(" ?", Style::default().fg(Color::Green)),
                }
            } else { Span::raw("") };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::DarkGray).bg(sel_bg)),
                Span::styled(name_str, Style::default().fg(name_col).bg(sel_bg)),
                dirty_mark,
            ]))
        })
        .collect();

    f.render_widget(List::new(items), area);
}

// ── 5. Stats tab ─────────────────────────────────────────────────────────────

fn render_stats(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let all_periods = [
        StatsPeriod::Day1, StatsPeriod::Day7, StatsPeriod::Day30,
        StatsPeriod::Day90, StatsPeriod::Year1, StatsPeriod::AllTime,
    ];

    // ── Period selector row ──
    let mut period_spans: Vec<Span> = vec![Span::styled("  Period: ", dim)];
    for p in all_periods {
        if p == st.stats_period {
            period_spans.push(Span::styled(
                format!("[{}]", p.label()),
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
        } else {
            period_spans.push(Span::styled(format!(" {} ", p.label()), dim));
        }
        period_spans.push(Span::raw(" "));
    }
    period_spans.push(Span::styled("[p] cycle  [</>] prev/next", dim));

    let period_h = 2u16;
    let period_area = Rect::new(area.x, area.y, area.width, period_h);

    // ── Chart area ──
    let chart_h = (area.height.saturating_sub(period_h + 2)).min(20).max(5); // 5..20 rows
    let chart_area = Rect::new(area.x, area.y + period_h, area.width, chart_h);

    // ── Bottom metrics ──
    let metrics_y = area.y + period_h + chart_h + 1;
    let metrics_h = area.height.saturating_sub(period_h + chart_h + 2);
    let metrics_area = if metrics_h > 0 {
        Rect::new(area.x, metrics_y, area.width, metrics_h)
    } else {
        Rect::new(0, 0, 0, 0)
    };

    // Render period selector
    f.render_widget(Paragraph::new(vec![
        Line::from(period_spans),
        Line::from(dsep(area.width)),
    ]), period_area);

    // Render vertical bar chart
    render_bar_chart(f, &st.activity_data, chart_area);

    // Render bottom metrics
    if metrics_h > 0 {
        let kv = |k: &str, v: &str, col: Color| -> Line<'static> {
            Line::from(vec![
                Span::styled(format!("  {:<18}", k), dim),
                Span::styled(v.to_string(), Style::default().fg(col)),
            ])
        };
        let avg_per_month = if st.repo_age_days > 0 {
            st.total_commits * 30 / (st.repo_age_days as usize).max(1)
        } else { 0 };
        let loc_str = if st.loc_total > 1000 {
            format!("{}k", st.loc_total / 1000)
        } else {
            st.loc_total.to_string()
        };

        let mut mlines: Vec<Line> = vec![
            Line::from(Span::styled("  Key Metrics", yel)),
            dsep(area.width),
            kv("Total commits",   &st.total_commits.to_string(),  Color::White),
            kv("Total releases",  &st.total_tags.to_string(),     Color::Yellow),
            kv("Contributors",    &st.contributors.len().to_string(), Color::White),
            kv("Lines of code",   &loc_str,                       Color::Cyan),
            kv("Avg/month",       &format!("{} commits", avg_per_month), Color::DarkGray),
        ];
        if !st.top_files.is_empty() {
            mlines.push(Line::from(""));
            mlines.push(Line::from(Span::styled("  Largest Files (lines)", yel)));
            mlines.push(dsep(area.width));
            for (name, lines) in st.top_files.iter().take(5) {
                let short: String = name.chars().rev().take(30).collect::<String>().chars().rev().collect();
                mlines.push(Line::from(vec![
                    Span::styled(format!("  {:<32}", short), dim),
                    Span::styled(format!("{:>6}", lines), Style::default().fg(Color::Cyan)),
                ]));
            }
        }
        f.render_widget(Paragraph::new(mlines), metrics_area);
    }
}

fn render_bar_chart(f: &mut Frame, data: &[(String, usize)], area: Rect) {
    if data.is_empty() || area.height < 3 || area.width < 10 {
        return;
    }

    let max_v   = data.iter().map(|d| d.1).max().unwrap_or(1).max(1);
    let axis_w  = 6usize; // "  123 │"
    let avail_w = area.width.saturating_sub(axis_w as u16) as usize;
    let n_cols  = data.len();
    // Each column: bar_chars + 1 space. bar_chars = col_w chars.
    let col_w   = ((avail_w).saturating_sub(n_cols)) / n_cols.max(1);
    let col_w   = col_w.max(1).min(4);
    let chart_h = area.height.saturating_sub(2) as usize; // -1 for axis, -1 for labels

    let mut lines: Vec<Line> = vec![];

    // Build chart rows from top to bottom
    for row in 0..chart_h {
        // row 0 = top (high values), row chart_h-1 = bottom (0)
        let mut spans: Vec<Span<'static>> = vec![];

        // Y-axis label
        let y_val = max_v * (chart_h - row) / chart_h;
        let y_lbl = if row == 0 {
            format!("{:>4} │", max_v)
        } else if row == chart_h / 2 {
            format!("{:>4} │", y_val)
        } else if row == chart_h - 1 {
            "   0 │".to_string()
        } else {
            "     │".to_string()
        };
        spans.push(Span::styled(y_lbl, Style::default().fg(Color::DarkGray)));

        for d in data.iter() {
            // bar_h = how many rows this column fills from bottom
            let bar_h = if max_v == 0 { 0 } else { (d.1 * chart_h / max_v).min(chart_h) };
            // Current row is filled if (row from bottom) < bar_h
            // row from bottom = chart_h - 1 - row
            let row_from_bottom = chart_h.saturating_sub(1).saturating_sub(row);
            let filled = row_from_bottom < bar_h;
            let bar_str = if filled {
                "█".repeat(col_w)
            } else {
                " ".repeat(col_w)
            };
            let bar_col = if filled { Color::Cyan } else { Color::Reset };
            spans.push(Span::styled(bar_str + " ", Style::default().fg(bar_col)));
        }
        lines.push(Line::from(spans));
    }

    // X-axis
    let x_axis = format!("     └{}", "─".repeat((col_w + 1) * n_cols + 1));
    lines.push(Line::from(Span::styled(x_axis, Style::default().fg(Color::DarkGray))));

    // Labels (show every Nth to avoid crowding)
    let label_every = ((n_cols as u16 * (col_w as u16 + 1)) / area.width.max(1) + 1) as usize;
    let mut label_spans: Vec<Span<'static>> = vec![Span::raw("      ".to_string())];
    for (i, d) in data.iter().enumerate() {
        let lbl = if i % label_every.max(1) == 0 {
            format!("{:<width$}", d.0.chars().take(col_w + 1).collect::<String>(), width = col_w + 1)
        } else {
            " ".repeat(col_w + 1)
        };
        label_spans.push(Span::styled(lbl, Style::default().fg(Color::DarkGray)));
    }
    lines.push(Line::from(label_spans));

    f.render_widget(Paragraph::new(lines), area);
}

// ── 6. Settings tab ───────────────────────────────────────────────────────────

fn render_settings(f: &mut Frame, st: &St, area: Rect) {
    let dim  = Style::default().fg(Color::DarkGray);
    let cyn  = Style::default().fg(Color::Cyan);
    let yel  = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let red  = Style::default().fg(Color::Red);

    let (vis, desc, topics_str, gh_ok) = match &st.settings {
        None => {
            let msg = if st.settings_rx.is_some() {
                "  Fetching from GitHub API..."
            } else {
                "  (loading...)"
            };
            f.render_widget(Paragraph::new(msg).style(dim), area);
            return;
        }
        Some(s) => (s.visibility.clone(), s.description.clone(), s.topics.join(", "), s.gh_auth_ok),
    };

    let busy = st.api_rx.is_some();

    struct Item { label: &'static str, value: String, hint: &'static str, danger: bool }
    let items: &[Item] = &[
        Item { label: "Visibility",      value: vis.clone(),                                              hint: "toggle public / private",  danger: false },
        Item { label: "Description",     value: if desc.is_empty() { "(none)".into() } else { desc.chars().take(35).collect() }, hint: "edit text",   danger: false },
        Item { label: "Topics",          value: if topics_str.is_empty() { "(none)".into() } else { topics_str.chars().take(35).collect() }, hint: "edit list",   danger: false },
        Item { label: "Collaborator",    value: String::new(),                                             hint: "invite user",              danger: false },
        Item { label: "Archive",         value: String::new(),                                             hint: "archive repo on GitHub",   danger: true  },
        Item { label: "Rename",          value: String::new(),                                             hint: "rename on GitHub",         danger: true  },
        Item { label: "Delete",          value: String::new(),                                             hint: "delete on GitHub ⚠",      danger: true  },
        Item { label: ".gitignore",      value: String::new(),                                             hint: "open in $EDITOR",          danger: false },
        Item { label: "LICENSE",         value: String::new(),                                             hint: "open in $EDITOR",          danger: false },
        Item { label: "README",          value: String::new(),                                             hint: "open in $EDITOR",          danger: false },
    ];

    let mut lines: Vec<Line> = vec![];

    if !gh_ok {
        let msg = st.settings_err.as_deref()
            .unwrap_or("No GitHub account — run: fxg auth import");
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", msg),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(dsep(area.width));
        lines.push(Line::from(""));
    } else if let Some(err) = &st.settings_err {
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", err),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(""));
    }
    if busy {
        lines.push(Line::from(Span::styled(
            "  Updating GitHub...",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    // Section: GitHub API
    lines.push(Line::from(Span::styled("  GitHub API", yel)));
    lines.push(dsep(area.width));
    for (i, item) in items[..7].iter().enumerate() {
        lines.push(settings_row(i, item.label, &item.value, item.hint, item.danger, i == st.settings_cur));
    }
    lines.push(Line::from(""));

    // Section: Local Files
    lines.push(Line::from(Span::styled("  Local Files", yel)));
    lines.push(dsep(area.width));
    for (i, item) in items[7..].iter().enumerate() {
        lines.push(settings_row(7 + i, item.label, &item.value, item.hint, false, 7 + i == st.settings_cur));
    }
    lines.push(Line::from(""));

    // Section: Git Operations
    struct GitItem { label: &'static str, value: String, hint: &'static str }
    let stash_hint = if st.stash_count > 0 { format!("{} stashed", st.stash_count) } else { String::new() };
    let git_items: &[GitItem] = &[
        GitItem { label: "Pull",       value: String::new(),   hint: "git pull (fetch + merge)" },
        GitItem { label: "Commit",     value: String::new(),   hint: "stage all & commit" },
        GitItem { label: "Push",       value: if st.summary.ahead > 0 { format!("{} ahead", st.summary.ahead) } else { String::new() }, hint: "git push" },
        GitItem { label: "Stash",      value: stash_hint.clone(), hint: "git stash" },
        GitItem { label: "Pop stash",  value: stash_hint,      hint: "git stash pop" },
    ];
    lines.push(Line::from(Span::styled("  Git Operations", yel)));
    lines.push(dsep(area.width));
    for (i, item) in git_items.iter().enumerate() {
        lines.push(settings_row(10 + i, item.label, &item.value, item.hint, false, 10 + i == st.settings_cur));
    }

    f.render_widget(Paragraph::new(lines), area);

    // Suppress unused variable warnings via inline references
    let _ = (cyn, red);
}

fn settings_row(
    idx:    usize,
    label:  &str,
    value:  &str,
    hint:   &str,
    danger: bool,
    sel:    bool,
) -> Line<'static> {
    let sel_bg   = if sel { Color::DarkGray } else { Color::Reset };
    let num_col  = if sel { Color::Cyan } else { Color::DarkGray };
    let lbl_col  = if danger && !sel { Color::Red } else if sel { Color::White } else { Color::White };
    let hint_col = if danger { Color::Red } else { Color::DarkGray };

    Line::from(vec![
        Span::styled(
            format!("  {:>2}. ", idx + 1),
            Style::default().fg(num_col).bg(sel_bg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
        ),
        Span::styled(
            format!("{:<16}", label),
            Style::default().fg(lbl_col).bg(sel_bg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
        ),
        Span::styled(
            format!("{:<30}", value.chars().take(30).collect::<String>()),
            Style::default().fg(Color::DarkGray).bg(sel_bg),
        ),
        Span::styled(
            hint.to_string(),
            Style::default().fg(hint_col).bg(sel_bg),
        ),
    ])
}

// ── right panel router ────────────────────────────────────────────────────────

fn render_right(f: &mut Frame, st: &St, area: Rect) {
    match st.tab {
        Tab::Files    => render_right_files(f, st, area),
        Tab::Branches => render_right_branch(f, st, area),
        Tab::Stats    => render_right_stats(f, st, area),
        _             => render_right_default(f, st, area),
    }
}

fn render_right_default(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let actions_h = 16u16;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(actions_h)])
        .split(inner);

    render_info_block(f, st, layout[0]);
    render_actions_block(f, layout[1]);
}

fn render_right_files(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let selected_node = st.tree_visible.get(st.tree_cursor).map(|&ni| &st.tree_nodes[ni]);

    let title = if let Some(node) = selected_node {
        if node.is_dir {
            format!(" {} {} ", if node.expanded { "▼" } else { "▶" }, node.name)
        } else {
            format!(" {} ", node.name)
        }
    } else { " Preview ".to_string() };

    let title_blk = Block::default().borders(Borders::BOTTOM).title(title.as_str())
        .border_style(Style::default().fg(Color::DarkGray));
    let title_h = 2u16;
    let title_area   = Rect::new(inner.x, inner.y, inner.width, title_h);
    let preview_area = Rect::new(inner.x, inner.y + title_h, inner.width, inner.height.saturating_sub(title_h));
    f.render_widget(title_blk, title_area);

    if let Some(node) = selected_node {
        if node.is_dir {
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("(directory)", Style::default().fg(Color::DarkGray))),
                    Line::from(Span::styled("[Enter] expand/collapse", Style::default().fg(Color::DarkGray))),
                    Line::from(Span::styled("[e] open in editor", Style::default().fg(Color::DarkGray))),
                ]),
                preview_area,
            );
        } else if st.file_preview.is_empty() {
            f.render_widget(
                Paragraph::new("(navigate to load)").style(Style::default().fg(Color::DarkGray)),
                preview_area,
            );
        } else {
            let avail = preview_area.height as usize;
            let skip  = st.preview_off.min(st.file_preview.len().saturating_sub(1));
            let items: Vec<ListItem> = st.file_preview.iter()
                .skip(skip).take(avail).enumerate()
                .map(|(i, line)| {
                    let ln = skip + i + 1;
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:>4} ", ln), Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            line.chars().take(inner.width.saturating_sub(5) as usize).collect::<String>(),
                            Style::default().fg(Color::White),
                        ),
                    ]))
                })
                .collect();
            f.render_widget(List::new(items), preview_area);
        }
    } else {
        f.render_widget(
            Paragraph::new("(no file selected)").style(Style::default().fg(Color::DarkGray)),
            preview_area,
        );
    }
}

fn render_right_branch(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let dim = Style::default().fg(Color::DarkGray);
    let kv  = |k: &str, v: &str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" {:<11}", k), dim),
            Span::styled(v.to_string(), Style::default().fg(col)),
        ])
    };
    let mut lines: Vec<Line> = vec![];

    if let Some(b) = st.branches.get(st.branch_cursor) {
        lines.push(Line::from(Span::styled(
            format!(" {}", b.name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
        lines.push(dsep(inner.width));
        lines.push(Line::from(""));
        lines.push(kv("Status", if b.is_current { "✓ current" } else { "○ other" }, if b.is_current { Color::Cyan } else { Color::DarkGray }));
        lines.push(kv("Upstream", b.upstream.as_deref().unwrap_or("(no tracking)"), Color::DarkGray));
        if b.upstream.is_some() {
            lines.push(kv("Ahead",  &b.ahead.to_string(),  if b.ahead  > 0 { Color::Green } else { Color::DarkGray }));
            lines.push(kv("Behind", &b.behind.to_string(), if b.behind > 0 { Color::Red }   else { Color::DarkGray }));
        }
        lines.push(kv("Age", &b.last_commit_age, Color::DarkGray));
        lines.push(Line::from(""));
        let msg: String = b.last_commit_msg.chars().take(inner.width.saturating_sub(3) as usize).collect();
        lines.push(kv("Last", &msg, Color::DarkGray));
    } else {
        lines.push(Line::from(Span::styled(" No branch selected", dim)));
    }

    lines.push(Line::from(""));
    lines.push(dsep(inner.width));
    lines.push(Line::from(Span::styled(" Actions", dim)));
    lines.push(Line::from(""));
    for (k, lbl) in [("Enter", "Checkout"), ("n", "New branch (modal)"), ("P", "Push"), ("D", "Delete")] {
        lines.push(Line::from(vec![
            Span::styled(format!(" [{}]", k), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", lbl), Style::default().fg(Color::White)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_right_stats(f: &mut Frame, st: &St, area: Rect) {
    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let dim = Style::default().fg(Color::DarkGray);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let kv  = |k: &str, v: &str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" {:<13}", k), dim),
            Span::styled(v.to_string(), Style::default().fg(col)),
        ])
    };

    let loc_str = if st.loc_total > 1000 {
        format!("{}k lines", st.loc_total / 1000)
    } else {
        format!("{} lines", st.loc_total)
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(" Top Contributors", yel)),
        dsep(inner.width),
        Line::from(""),
    ];
    for (i, c) in st.contributors.iter().take(8).enumerate() {
        let name: String = c.name.chars().take((inner.width.saturating_sub(8)) as usize).collect();
        lines.push(Line::from(vec![
            Span::styled(format!(" {:>2}. ", i + 1), dim),
            Span::styled(name, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("     ", dim),
            Span::styled(format!("{} commits", c.count), dim),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" Code Stats", yel)));
    lines.push(dsep(inner.width));
    lines.push(kv("LOC", &loc_str, Color::Cyan));
    lines.push(kv("Commits", &st.total_commits.to_string(), Color::White));
    lines.push(kv("Tags", &st.total_tags.to_string(), Color::Yellow));
    if let Some(lang) = &st.summary.lang {
        lines.push(kv("Languages", lang, Color::DarkGray));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── info + actions (shared default right panel) ────────────────────────────

fn render_info_block(f: &mut Frame, st: &St, area: Rect) {
    let r   = &st.summary;
    let w   = area.width.saturating_sub(2) as usize;
    let dim = Style::default().fg(Color::DarkGray);
    let kv  = |k: &str, v: &str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" {:<10}", k), dim),
            Span::styled(v.chars().take(w.saturating_sub(11)).collect::<String>(), Style::default().fg(col)),
        ])
    };
    let sync_txt = match (r.ahead, r.behind) {
        (0, 0) => "in sync".to_string(),
        (a, 0) => format!("↑{} ahead", a),
        (0, b) => format!("↓{} behind", b),
        (a, b) => format!("↑{} ↓{}", a, b),
    };

    let mut info: Vec<Line> = vec![
        Line::from(Span::styled(
            format!(" {}", r.name.chars().take(w).collect::<String>()),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        dsep(area.width.saturating_add(2)),
        Line::from(""),
        kv("Remote",  &sync_txt, if r.behind > 0 { Color::Red } else if r.ahead > 0 { Color::Cyan } else { Color::DarkGray }),
        kv("Dirty",   if r.is_dirty { "⚠ changes" } else { "✓ clean" }, if r.is_dirty { Color::Yellow } else { Color::DarkGray }),
        Line::from(""),
        kv("Owner",   r.owner.as_deref().unwrap_or("—"), Color::White),
        kv("Vis.",    r.visibility.as_deref().unwrap_or("—"), match r.visibility.as_deref() { Some("public") => Color::Green, Some("private") => Color::Yellow, _ => Color::DarkGray }),
        kv("License", if r.license { "✓" } else { "⚠ missing" }, if r.license { Color::DarkGray } else { Color::Yellow }),
    ];
    if let Some(lang) = &r.lang { info.push(kv("Lang", lang, Color::DarkGray)); }
    if let Some(s) = r.stars       { info.push(kv("Stars",  &format!("★ {}", s),  Color::Yellow)); }
    if let Some(i) = r.open_issues { info.push(kv("Issues", &i.to_string(),        if i > 0 { Color::Red }  else { Color::DarkGray })); }
    if let Some(p) = r.open_prs    { info.push(kv("PRs",    &p.to_string(),        if p > 0 { Color::Cyan } else { Color::DarkGray })); }
    info.push(Line::from(""));
    info.push(kv("Commits",  &st.total_commits.to_string(),      Color::White));
    info.push(kv("Files",    &st.file_count.to_string(),          Color::DarkGray));
    info.push(kv("Branches", &st.branch_count.to_string(),        Color::DarkGray));
    info.push(kv("Tags",     &st.total_tags.to_string(),           Color::Yellow));
    info.push(kv("Stashes",  &st.stash_count.to_string(),         Color::DarkGray));
    if let Some(tag) = &r.last_tag { info.push(kv("Last tag", tag, Color::Cyan)); }

    f.render_widget(Paragraph::new(info), area);
}

fn render_actions_block(f: &mut Frame, area: Rect) {
    let act_blk   = Block::default().borders(Borders::TOP).title(" Actions ").border_style(Style::default().fg(Color::DarkGray));
    let act_inner = act_blk.inner(area);
    f.render_widget(act_blk, area);

    let act = |k: &'static str, label: &'static str, col: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!(" [{}]", k), Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", label), Style::default().fg(Color::White)),
        ])
    };
    let div = || Line::from(Span::styled(" ─────────────────", Style::default().fg(Color::DarkGray)));

    f.render_widget(Paragraph::new(vec![
        act("s", "Sync",           Color::Green),
        act("r", "Release tag",    Color::Green),
        div(),
        act("o", "Open in editor", Color::Cyan),
        act("W", "Open browser",   Color::Cyan),
        act("y", "Copy path",      Color::Cyan),
        act("u", "Copy URL",       Color::Cyan),
        act("g", ".gitignore",     Color::Cyan),
        div(),
        act("S", "Stash",          Color::White),
        act("A", "Pop stash",      Color::White),
        div(),
        act("q", "← Back",        Color::DarkGray),
    ]), act_inner);
}

// ── bottom bar ────────────────────────────────────────────────────────────────

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

// ── modal overlay ─────────────────────────────────────────────────────────────

fn render_modal(f: &mut Frame, st: &St, area: Rect) {
    match &st.modal {
        DetailModal::None => {}

        DetailModal::NewBranch { input, error } => {
            draw_text_modal_with_error(f, area, " New Branch ", input, "branch name", error.as_deref(), Color::Cyan);
        }

        DetailModal::EditText { title, prompt, input, .. } => {
            let prompt_owned = prompt.clone();
            draw_text_modal(f, area, &format!(" {} ", title), input, &prompt_owned, Color::Cyan);
        }

        DetailModal::SelectOption { title, options, cursor, .. } => {
            let h = (options.len() as u16 + 4).min(area.height.saturating_sub(4));
            let w = 44u16.min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(format!(" {} ", title))
                .border_style(Style::default().fg(Color::Cyan));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let mut lines: Vec<Line> = vec![Line::from("")];
            for (i, opt) in options.iter().enumerate() {
                let sel = i == *cursor;
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} {}", if sel { "▶" } else { " " }, opt),
                        Style::default()
                            .fg(if sel { Color::Cyan } else { Color::White })
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [↑↓] select  [Enter] confirm  [Esc] cancel", Style::default().fg(Color::DarkGray))));
            f.render_widget(Paragraph::new(lines), inner);
        }

        DetailModal::SelectRole { collab_name, cursor } => {
            const ROLES: &[&str] = &["pull", "push", "maintain", "admin"];
            let h = (ROLES.len() as u16 + 5).min(area.height.saturating_sub(4));
            let w = 44u16.min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(format!(" Role for @{} ", collab_name))
                .border_style(Style::default().fg(Color::Cyan));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let mut lines: Vec<Line> = vec![Line::from("")];
            for (i, role) in ROLES.iter().enumerate() {
                let sel = i == *cursor;
                lines.push(Line::from(Span::styled(
                    format!("  {} {}", if sel { "▶" } else { " " }, role),
                    Style::default().fg(if sel { Color::Cyan } else { Color::White })
                        .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [↑↓] select  [Enter] invite  [Esc] cancel", Style::default().fg(Color::DarkGray))));
            f.render_widget(Paragraph::new(lines), inner);
        }

        DetailModal::ConfirmAction { message, .. } => {
            let w = 52u16.min(area.width.saturating_sub(4));
            let h = 7u16;
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(" Confirm ")
                .border_style(Style::default().fg(Color::Red));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let msg_clone = message.clone();
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled(format!("  {}", msg_clone), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                    Line::from(""),
                    Line::from(Span::styled("  [y / Enter] confirm", Style::default().fg(Color::Red))),
                    Line::from(Span::styled("  [n / Esc]   cancel",  Style::default().fg(Color::DarkGray))),
                ]),
                inner,
            );
        }
    }
}

fn draw_text_modal(f: &mut Frame, area: Rect, title: &str, input: &str, hint: &str, border_col: Color) {
    draw_text_modal_with_error(f, area, title, input, hint, None, border_col);
}

fn draw_text_modal_with_error(f: &mut Frame, area: Rect, title: &str, input: &str, hint: &str, error: Option<&str>, border_col: Color) {
    let w = 60u16.min(area.width.saturating_sub(4));
    let h = if error.is_some() { 7u16 } else { 6u16 };
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let modal_area = Rect::new(x, y, w, h);
    f.render_widget(Clear, modal_area);

    let border_col = if error.is_some() { Color::Red } else { border_col };
    let blk = Block::default().borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(border_col));
    let inner = blk.inner(modal_area);
    f.render_widget(blk, modal_area);

    let mut lines = vec![
        Line::from(Span::styled(format!("  {}", hint), Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            format!("  > {}█", input.chars().take((w as usize).saturating_sub(6)).collect::<String>()),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("  [Enter] confirm  [Esc] cancel", Style::default().fg(Color::DarkGray))),
    ];
    if let Some(err) = error {
        lines.insert(2, Line::from(Span::styled(format!("  ⚠ {}", err), Style::default().fg(Color::Red))));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ── PRs tab ──────────────────────────────────────────────────────────────────

fn render_prs(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let header = Row::new(vec!["#", "TITLE", "AUTHOR", "BRANCH", "AGE", "DRAFT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    if st.prs_issues_rx.is_some() {
        f.render_widget(Paragraph::new("  Fetching PRs...").style(dim), area);
        return;
    }
    if st.repo_prs.is_empty() {
        let msg = if st.prs_loaded { "  No open PRs" } else { "  PRs not loaded — switch to this tab to load" };
        f.render_widget(Paragraph::new(msg).style(dim), area);
        return;
    }

    let avail  = area.height.saturating_sub(1) as usize;
    let pr_off = st.pr_cursor.saturating_sub(avail / 2);
    let rows: Vec<Row> = st.repo_prs.iter().enumerate()
        .skip(pr_off).take(avail)
        .map(|(i, pr)| {
            let is_cur = i + pr_off == st.pr_cursor;
            let age = crate::status::format_age_pub(pr.created_ts);
            let draft_str = if pr.draft { "draft" } else { "" };
            let branch_str = format!("{} → {}", pr.head_branch.chars().take(10).collect::<String>(), pr.base_branch.chars().take(8).collect::<String>());
            Row::new(vec![
                Cell::from(format!("#{}", pr.number)).style(Style::default().fg(Color::Cyan)),
                Cell::from(pr.title.chars().take(45).collect::<String>()),
                Cell::from(pr.author.chars().take(14).collect::<String>()).style(dim),
                Cell::from(branch_str).style(dim),
                Cell::from(age).style(dim),
                Cell::from(draft_str).style(Style::default().fg(Color::Yellow)),
            ]).style(if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() })
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(7),
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Length(22),
            Constraint::Length(10),
            Constraint::Length(6),
        ]).header(header),
        area,
    );
}

// ── Issues tab ────────────────────────────────────────────────────────────────

fn render_issues(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let header = Row::new(vec!["#", "TITLE", "AUTHOR", "LABELS", "AGE", "CMT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    if st.prs_issues_rx.is_some() {
        f.render_widget(Paragraph::new("  Fetching issues...").style(dim), area);
        return;
    }
    if st.repo_issues.is_empty() {
        let msg = if st.prs_loaded { "  No open issues" } else { "  Issues not loaded — switch to this tab to load" };
        f.render_widget(Paragraph::new(msg).style(dim), area);
        return;
    }

    let avail     = area.height.saturating_sub(1) as usize;
    let issue_off = st.issue_cursor.saturating_sub(avail / 2);
    let rows: Vec<Row> = st.repo_issues.iter().enumerate()
        .skip(issue_off).take(avail)
        .map(|(i, issue)| {
            let is_cur = i + issue_off == st.issue_cursor;
            let age    = crate::status::format_age_pub(issue.created_ts);
            let labels = issue.labels.join(", ").chars().take(18).collect::<String>();
            Row::new(vec![
                Cell::from(format!("#{}", issue.number)).style(Style::default().fg(Color::Cyan)),
                Cell::from(issue.title.chars().take(45).collect::<String>()),
                Cell::from(issue.author.chars().take(14).collect::<String>()).style(dim),
                Cell::from(labels).style(Style::default().fg(Color::Yellow)),
                Cell::from(age).style(dim),
                Cell::from(issue.comments.to_string()).style(dim),
            ]).style(if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() })
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(7),
            Constraint::Fill(1),
            Constraint::Length(14),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(5),
        ]).header(header),
        area,
    );
}

// ── Todos tab (per-repo) ──────────────────────────────────────────────────────

fn render_repo_todos(f: &mut Frame, st: &St, area: Rect) {
    use crate::todos::TodoStatus;
    let dim = Style::default().fg(Color::DarkGray);
    let header = Row::new(vec!["STATUS", "TITLE", "EST", "DIFF", "CREATED"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    if st.repo_todos.is_empty() {
        f.render_widget(Paragraph::new("  No todos for this repo").style(dim), area);
        return;
    }

    let avail    = area.height.saturating_sub(1) as usize;
    let todo_off = st.todo_cursor.saturating_sub(avail / 2);
    let rows: Vec<Row> = st.repo_todos.iter().enumerate()
        .skip(todo_off).take(avail)
        .map(|(i, t)| {
            let is_cur  = i + todo_off == st.todo_cursor;
            let est_str = t.estimate.as_deref().unwrap_or("—").chars().take(10).collect::<String>();
            let diff_str = match t.difficulty {
                Some(d) => format!("{}{}", "★".repeat(d.min(5) as usize), "☆".repeat(5usize.saturating_sub(d as usize))),
                None    => "—".into(),
            };
            let status_col = t.status.color();
            let status_str = format!("{} {}", t.status.icon(), t.status.label());
            let created    = crate::status::format_age_pub(t.created_at);
            Row::new(vec![
                Cell::from(status_str).style(Style::default().fg(status_col)),
                Cell::from(t.title.chars().take(50).collect::<String>()),
                Cell::from(est_str).style(dim),
                Cell::from(diff_str).style(Style::default().fg(Color::Yellow)),
                Cell::from(created).style(dim),
            ]).style(if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() })
        })
        .collect();

    let _ = TodoStatus::Open; // suppress unused import warning
    f.render_widget(
        Table::new(rows, [
            Constraint::Length(14),
            Constraint::Fill(1),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(10),
        ]).header(header),
        area,
    );
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn dsep(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        " ".to_string() + &"─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    ))
}

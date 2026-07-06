//! Renderers for the detail view's git-native tabs (Overview, Commits,
//! Branches, Files, Stats, Settings) — everything drawn into the left
//! 73%-width pane that doesn't depend on a provider API response.

use super::super::{HistEntry, St, StatsPeriod};
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

// ── 1. Overview tab ───────────────────────────────────────────────────────────

/// Draws a one-screen summary: location/links, repo-level stats, recent
/// releases, recent commits, top contributors, and active branches — the
/// "at a glance" tab shown by default when opening a repo.
pub fn render_overview(f: &mut Frame, st: &St, area: Rect) {
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
    let sep = || super::dsep(area.width);

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
        let skip = st.ov_off.min(st.ov_recent.len().saturating_sub(1));
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

/// Draws the merged commit/tag timeline. Local-only commits (not yet
/// pushed) are marked with `↑` and listed first, so the "ahead" count from
/// the topbar has a visible explanation here.
pub fn render_commits(f: &mut Frame, st: &St, area: Rect) {
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
                    Cell::from("T").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
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

/// Draws the branch table, scrolled to keep `branch_cursor` in view.
pub fn render_branches(f: &mut Frame, st: &St, area: Rect) {
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

/// Draws the flattened file tree, with a one-character dirty-status marker
/// (`M`/`D`/`?`) next to any file that also appears in `st.files`.
pub fn render_file_tree(f: &mut Frame, st: &St, area: Rect) {
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

/// Draws the period selector, activity bar chart, and a key-metrics summary
/// (avg commits/month, LOC, largest files) for the selected `stats_period`.
pub fn render_stats(f: &mut Frame, st: &St, area: Rect) {
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
    let chart_h = (area.height.saturating_sub(period_h + 2)).clamp(5, 20);
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
        super::dsep(area.width),
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
            super::dsep(area.width),
            kv("Total commits",   &st.total_commits.to_string(),  Color::White),
            kv("Total releases",  &st.total_tags.to_string(),     Color::Yellow),
            kv("Contributors",    &st.contributors.len().to_string(), Color::White),
            kv("Lines of code",   &loc_str,                       Color::Cyan),
            kv("Avg/month",       &format!("{} commits", avg_per_month), Color::DarkGray),
        ];
        if !st.top_files.is_empty() {
            mlines.push(Line::from(""));
            mlines.push(Line::from(Span::styled("  Largest Files (lines)", yel)));
            mlines.push(super::dsep(area.width));
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

/// Draws a vertical bar chart of `data` (label, count) pairs, sized to fit
/// `area` exactly. Bar width and label sampling both scale down gracefully
/// as the chart gets narrower, rather than overflowing or panicking.
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
    let col_w   = col_w.clamp(1, 4);
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

/// Draws the Settings tab: GitHub API actions, local-file shortcuts
/// (.gitignore/LICENSE/README), and git operations, each as a numbered,
/// cursor-selectable row sharing layout with `settings_row`.
pub fn render_settings(f: &mut Frame, st: &St, area: Rect) {
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
            .unwrap_or("No GitHub account — run: vulp auth import");
        lines.push(Line::from(Span::styled(
            format!("  ⚠ {}", msg),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(super::dsep(area.width));
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
    lines.push(super::dsep(area.width));
    for (i, item) in items[..7].iter().enumerate() {
        lines.push(settings_row(i, item.label, &item.value, item.hint, item.danger, i == st.settings_cursor));
    }
    lines.push(Line::from(""));

    // Section: Local Files
    lines.push(Line::from(Span::styled("  Local Files", yel)));
    lines.push(super::dsep(area.width));
    for (i, item) in items[7..].iter().enumerate() {
        lines.push(settings_row(7 + i, item.label, &item.value, item.hint, false, 7 + i == st.settings_cursor));
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
    lines.push(super::dsep(area.width));
    for (i, item) in git_items.iter().enumerate() {
        lines.push(settings_row(10 + i, item.label, &item.value, item.hint, false, 10 + i == st.settings_cursor));
    }

    f.render_widget(Paragraph::new(lines), area);

    // Suppress unused variable warnings via inline references
    let _ = (cyn, red);
}

/// Draws one numbered row of the Settings tab, sharing column widths across
/// all three sections so the cursor highlight lines up consistently.
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
    let lbl_col  = if danger && !sel { Color::Red } else { Color::White };
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

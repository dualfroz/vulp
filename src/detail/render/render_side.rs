//! Renderers for the detail view's right-hand panel (27% width column).
//! Most tabs show the default info+actions panel; Files/Branches/Stats
//! replace it with a tab-specific panel showing the selected item's detail.

use super::super::{St, Tab};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Picks which right-panel variant to draw based on the active tab.
pub fn render_right(f: &mut Frame, st: &St, area: Rect) {
    match st.tab {
        Tab::Files    => render_right_files(f, st, area),
        Tab::Branches => render_right_branch(f, st, area),
        Tab::Stats    => render_right_stats(f, st, area),
        _             => render_right_default(f, st, area),
    }
}

/// Default right panel: repo info on top, a fixed list of global actions on
/// the bottom — shown for every tab without a more specific panel.
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

/// Files-tab right panel: a preview of the selected file's first 200 lines
/// (or a directory hint), loaded on demand by `tui_phase_inner`'s `run_action`
/// 40 handling rather than eagerly for every node.
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

/// Branches-tab right panel: detail (upstream, ahead/behind, last commit)
/// for the cursor-selected branch, plus the branch action shortcuts.
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
        lines.push(super::dsep(inner.width));
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
    lines.push(super::dsep(inner.width));
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

/// Stats-tab right panel: top contributors and a code-size summary,
/// complementing the activity chart drawn in the left pane.
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
        super::dsep(inner.width),
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
    lines.push(super::dsep(inner.width));
    lines.push(kv("LOC", &loc_str, Color::Cyan));
    lines.push(kv("Commits", &st.total_commits.to_string(), Color::White));
    lines.push(kv("Tags", &st.total_tags.to_string(), Color::Yellow));
    if let Some(lang) = &st.summary.lang {
        lines.push(kv("Languages", lang, Color::DarkGray));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── info + actions (shared default right panel) ────────────────────────────

/// Draws the repo metadata block (sync status, owner, visibility, license,
/// language, stars/issues/PRs when known, and local counters) used by the
/// default right panel.
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
        super::dsep(area.width.saturating_add(2)),
        Line::from(""),
        kv("Remote",  &sync_txt, if r.behind > 0 { Color::Red } else if r.ahead > 0 { Color::Cyan } else { Color::DarkGray }),
        kv("Dirty",   if r.is_dirty { "⚠ changes" } else { "✓ clean" }, if r.is_dirty { Color::Yellow } else { Color::DarkGray }),
        Line::from(""),
        kv("Owner",   r.owner.as_deref().unwrap_or("—"), Color::White),
        kv("Vis.",    r.visibility.as_deref().unwrap_or("—"), match r.visibility.as_deref() { Some("public") => Color::Green, Some("private") => Color::Yellow, _ => Color::DarkGray }),
        kv("License", if r.has_license { "✓" } else { "⚠ missing" }, if r.has_license { Color::DarkGray } else { Color::Yellow }),
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

/// Draws the fixed list of global action shortcuts used by the default
/// right panel (sync, release, open-in-editor/browser, clipboard, stash).
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

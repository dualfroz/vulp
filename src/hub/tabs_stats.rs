//! The hub's Stats and Activity tabs: a workspace-wide commit bar chart plus
//! language/top-repo/day-of-week breakdowns (Stats), and the merged
//! commit/PR/issue/local-event feed (Activity).

use crate::status::format_age_pub;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
    Frame,
};
use super::state::HubState;

pub fn render_stats_global(f: &mut Frame, state: &HubState, area: Rect) {
    let dim  = Style::default().fg(Color::DarkGray);
    let yel  = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let cyn  = Style::default().fg(Color::Cyan);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // LEFT: bar chart or loading
    let mut left_lines: Vec<Line> = vec![];

    left_lines.push(Line::from(Span::styled(
        format!("  Global Statistics  [7:{}]  [p] change period  [d] weekly digest preview", state.global_stats_period.label()),
        yel,
    )));
    left_lines.push(Line::from(Span::styled("  ─────────────────────────────────────────────────────────", dim)));

    match &state.global_stats {
        None if state.global_stats_rx.is_some() => {
            left_lines.push(Line::from(Span::styled("  Computing stats from all repos (may take a moment)...", cyn)));
        }
        None => {
            left_lines.push(Line::from(Span::styled("  Press [p] or switch to this tab to load stats", dim)));
        }
        Some(gs) => {
            left_lines.push(Line::from(vec![
                Span::styled(format!("  Total commits this period: {}", gs.total_commits), Style::default().fg(Color::White)),
            ]));
            left_lines.push(Line::from(""));

            // Simple text bar chart
            let max_count: usize = gs.activity.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
            let bar_h = (area.height as usize).saturating_sub(10).max(5);
            let data = &gs.activity;
            let n = data.len();
            let bar_w = (cols[0].width as usize).saturating_sub(8).max(1);
            let step = if n > bar_w { (n / bar_w).max(1) } else { 1 };
            let sampled: Vec<&(String, usize)> = data.iter().step_by(step).collect();
            let bar_count = sampled.len().min(bar_w);
            // Stretch bars to fill available width when few data points
            let bar_width = if bar_count == 0 { 1 } else { (bar_w / bar_count).max(1) };

            for row in (0..bar_h).rev() {
                let threshold = (row * max_count) / bar_h;
                let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
                let y_label = if row == bar_h - 1 { format!("{:>4} ", max_count) } else if row == 0 { "   0 ".to_string() } else { "     ".to_string() };
                spans.push(Span::styled(y_label, dim));
                for j in 0..bar_count {
                    let count = sampled.get(j).map(|(_, c)| *c).unwrap_or(0);
                    let filled = count > threshold;
                    for _ in 0..bar_width {
                        if filled { spans.push(Span::styled("█", cyn)); }
                        else      { spans.push(Span::styled("░", dim)); }
                    }
                }
                left_lines.push(Line::from(spans));
            }
            // X labels: one per bar, each bar_width chars wide
            let nth = (bar_count / 5).max(1);
            let mut x_spans: Vec<Span> = vec![Span::raw("       ")];
            for j in 0..bar_count {
                let label = if j % nth == 0 {
                    let raw = sampled.get(j)
                        .map(|(d, _)| d.chars().skip(5).take(bar_width.min(5)).collect::<String>())
                        .unwrap_or_default();
                    format!("{:<width$}", raw, width = bar_width)
                } else {
                    " ".repeat(bar_width)
                };
                x_spans.push(Span::styled(label, dim));
            }
            left_lines.push(Line::from(x_spans));
        }
    }

    f.render_widget(Paragraph::new(left_lines), cols[0]);

    // RIGHT: language breakdown + top repos
    let mut right_lines: Vec<Line> = vec![];

    right_lines.push(Line::from(Span::styled("  Languages", yel)));
    right_lines.push(Line::from(Span::styled("  ────────────────────────────────", dim)));
    match &state.global_stats {
        None => { right_lines.push(Line::from(Span::styled("  Loading...", dim))); }
        Some(gs) => {
            let total_repos = state.repos.len().max(1);
            for (lang, cnt) in gs.lang_dist.iter().take(8) {
                let bar_len = (cnt * 16) / total_repos;
                let bar = "█".repeat(bar_len) + &"░".repeat(16usize.saturating_sub(bar_len));
                right_lines.push(Line::from(vec![
                    Span::styled(format!("  {:<8} ", lang.chars().take(8).collect::<String>()), Style::default().fg(Color::White)),
                    Span::styled(bar, cyn),
                    Span::styled(format!(" {}", cnt), dim),
                ]));
            }
            right_lines.push(Line::from(""));

            right_lines.push(Line::from(Span::styled("  Top Repos (commits)", yel)));
            right_lines.push(Line::from(Span::styled("  ────────────────────────────────", dim)));
            for (name, cnt) in gs.top_repos.iter().take(5) {
                right_lines.push(Line::from(vec![
                    Span::styled(format!("  {:<18} ", name.chars().take(18).collect::<String>()), Style::default().fg(Color::White)),
                    Span::styled(format!("{}", cnt), cyn),
                ]));
            }
            right_lines.push(Line::from(""));
            right_lines.push(Line::from(vec![
                Span::styled("  Health: ", dim),
                Span::styled(format!("{}↓ {}⚠", gs.behind_count, gs.dirty_count), Style::default().fg(if gs.behind_count > 0 || gs.dirty_count > 0 { Color::Yellow } else { Color::Green })),
            ]));
            right_lines.push(Line::from(""));

            right_lines.push(Line::from(Span::styled("  Day of Week", yel)));
            right_lines.push(Line::from(Span::styled("  ────────────────────────────────", dim)));
            let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
            let max_day = gs.day_pattern.iter().copied().max().unwrap_or(1).max(1);
            for (i, &cnt) in gs.day_pattern.iter().enumerate() {
                let bar_len = (cnt * 16) / max_day;
                let bar: String = "█".repeat(bar_len) + &"░".repeat(16usize.saturating_sub(bar_len));
                right_lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", day_names[i]), Style::default().fg(Color::White)),
                    Span::styled(bar, cyn),
                    Span::styled(format!(" {}", cnt), dim),
                ]));
            }
        }
    }

    f.render_widget(Paragraph::new(right_lines), cols[1]);
}

pub fn render_activity_global(f: &mut Frame, state: &HubState, area: Rect) {
    use ratatui::widgets::{Block, Borders};
    let dim = Style::default().fg(Color::DarkGray);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let filter_label = state.activity_filter.map(|k| k.label()).unwrap_or("all");
    let title = format!(" activity — filter: {}  ·  [↑↓] move  [Enter] open repo  [f] filter ", filter_label);
    let blk = Block::default().borders(Borders::ALL)
        .title(Span::styled(title, yel))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    if state.activity_feed.is_none() {
        let msg = if state.activity_rx.is_some() { " Loading activity..." } else { " Press [Tab] or re-enter this tab to load activity" };
        f.render_widget(Paragraph::new(Line::from(Span::styled(msg, dim))), inner);
        return;
    }

    let filtered = state.activity_filtered();
    if filtered.is_empty() {
        f.render_widget(Paragraph::new(Line::from(Span::styled(" no activity recorded yet", dim))), inner);
        return;
    }

    // Keep the cursor in range and scroll a window around it.
    let max_rows = (inner.height.saturating_sub(1) as usize).max(1);
    let cursor   = state.activity_cursor.min(filtered.len() - 1);
    let offset   = if cursor >= max_rows { cursor - max_rows + 1 } else { 0 };
    let repo_w   = 34usize;
    let detail_w = (inner.width as usize).saturating_sub(20 + 10 + repo_w + 4).max(10);

    let header = Row::new(vec![
        Cell::from(" WHEN").style(dim), Cell::from("KIND").style(dim),
        Cell::from("REPO").style(dim), Cell::from("DETAIL").style(dim),
    ]);
    let rows: Vec<Row> = filtered.iter().enumerate().skip(offset).take(max_rows).map(|(i, e)| {
        let is_cur = i == cursor;
        let kind_col = match e.kind {
            crate::activity::ActivityKind::Commit  => Color::White,
            crate::activity::ActivityKind::Push    => Color::Green,
            crate::activity::ActivityKind::Pull    => Color::Cyan,
            crate::activity::ActivityKind::Release  => Color::LightGreen,
            crate::activity::ActivityKind::NewRepo  => Color::Magenta,
            crate::activity::ActivityKind::Todo     => Color::Yellow,
            crate::activity::ActivityKind::Pr       => Color::LightCyan,
            crate::activity::ActivityKind::Issue    => Color::LightYellow,
            crate::activity::ActivityKind::License  => Color::Blue,
        };
        // Precise local timestamp + relative age, instead of age alone.
        let when = chrono::DateTime::from_timestamp(e.ts, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".into());
        // Colour the repo by its project, matching the Repos table; the ›
        // marks it as a jump target.
        let repo_ri  = state.repos.iter().position(|r| r.name == e.repo);
        let repo_col = repo_ri.and_then(|ri| state.repo_proj_color(ri)).unwrap_or(Color::White);
        let repo_txt = format!("{}{}", if repo_ri.is_some() { "› " } else { "  " },
            e.repo.chars().take(repo_w - 2).collect::<String>());
        let detail = e.detail.chars().take(detail_w).collect::<String>();
        let mark = if is_cur { "▶ " } else { "  " };
        let row = Row::new(vec![
            Cell::from(format!("{}{}", mark, when)).style(Style::default().fg(if is_cur { Color::White } else { Color::Gray })),
            Cell::from(e.kind.label()).style(Style::default().fg(kind_col).add_modifier(Modifier::BOLD)),
            Cell::from(repo_txt).style(Style::default().fg(repo_col)),
            Cell::from(format!("{}  {}", detail, format_age_pub(e.ts))).style(dim),
        ]);
        if is_cur { row.style(Style::default().bg(Color::Rgb(40, 40, 55))) } else { row }
    }).collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(20), Constraint::Length(10),
            Constraint::Length(repo_w as u16), Constraint::Fill(1),
        ]).header(header),
        inner,
    );
}

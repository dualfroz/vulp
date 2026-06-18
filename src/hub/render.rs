use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use super::modals::render_modal;
use super::right_panel::render_right;
use super::state::{HubState, SortMode, Tab};
use super::tabs::render_list;

pub static HELP_LINES: &[&str] = &[
    "── Navigation ───────────────────────────────",
    " ↑↓ / jk     Move cursor up/down",
    " Tab          Switch tab (Repos → Projects → Todos → PRs → Issues)",
    " Enter        Open repo/project detail view",
    " q / Esc      Quit (or clear active filter)",
    "",
    "── Filtering & Sorting ───────────────────────",
    " /            Fuzzy filter repos (name, branch, owner, lang)",
    " S            Cycle sort: Default → Name → Recent → Dirty↑ → Behind↑",
    " d            Toggle: show only dirty repos",
    " b            Toggle: show only behind repos",
    " f            Toggle: show only ahead repos",
    " a            Toggle: show only archived repos (no commits >90d)",
    "",
    "── Open / Navigate ───────────────────────────",
    " o            Open repo in $EDITOR",
    " O            Open repo in file manager (xdg-open)",
    " W            Open repo in browser (GitHub/GitLab)",
    " y            Copy repo path to clipboard",
    " u            Copy remote URL to clipboard",
    " g            Open .gitignore in $EDITOR",
    " C            Clone a repository into workspace",
    "",
    "── Repo Actions ──────────────────────────────",
    " s            Sync (pull / commit+push) selected repos",
    " r            Create release tag",
    " B            Toggle bookmark (★) — visible in table",
    " P            Toggle pin (◆) — pinned repos shown at top",
    " n            Rename/label repo display name",
    " Space        Select repo for batch action",
    "",
    "── Projects & Todos ──────────────────────────",
    " p            Project menu (create / assign / rename / delete)",
    " +            New todo",
    " e            Edit todo (Todos tab)",
    " Space        (Todos tab) Cycle todo status",
    " D / Del      Delete todo or project",
    " T            Scan repos for TODO/FIXME/HACK → import to Todos",
    "",
    "── Help ──────────────────────────────────────",
    " ?            Show this help",
    " Esc / q      Close modal or quit",
];

pub fn render_hub(f: &mut Frame, state: &mut HubState) {
    let area = f.area();
    let bottom_h: u16 = if state.debug_mode { 11 } else { 3 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(bottom_h)])
        .split(area);

    let tab_span = |label: &str, on: bool| {
        if on {
            Span::styled(format!(" {} ", label), Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(format!(" {} ", label), Style::default().fg(Color::DarkGray))
        }
    };
    let n = state.repo_pos.len();
    let pos_str = format!("  {}/{}", if n == 0 { 0 } else { state.cursor + 1 }, n);

    let behind_c = state.repos.iter().filter(|r| r.behind > 0).count();
    let dirty_c  = state.repos.iter().filter(|r| r.is_dirty).count();
    let health_str = if behind_c > 0 || dirty_c > 0 {
        format!("  ↓{} ⚠{} ", behind_c, dirty_c)
    } else {
        "  ✓ ".into()
    };
    let health_col = if behind_c > 0 { Color::Red } else if dirty_c > 0 { Color::Yellow } else { Color::Green };

    let pr_badge    = if !state.prs.is_empty()    { format!("5 PRs({})",    state.prs.len())    } else { "5 PRs".into() };
    let issue_badge = if !state.issues.is_empty() { format!("6 Issues({})", state.issues.len()) } else { "6 Issues".into() };

    let mut top_spans = vec![
        Span::styled(" fxg  ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        tab_span("1 Overview",    state.tab == Tab::Overview),
        Span::raw(" "),
        tab_span("2 Repos",       state.tab == Tab::Repos),
        Span::raw(" "),
        tab_span("3 Projects",    state.tab == Tab::Projects),
        Span::raw(" "),
        tab_span("4 Todos",       state.tab == Tab::Todos),
        Span::raw(" "),
        tab_span(&pr_badge,       state.tab == Tab::PRs),
        Span::raw(" "),
        tab_span(&issue_badge,    state.tab == Tab::Issues),
        Span::raw(" "),
        tab_span("7 Stats",       state.tab == Tab::Stats),
        Span::raw(" "),
        tab_span("8 Settings",    state.tab == Tab::HubSettings),
        Span::raw("   "),
    ];

    // Active filter indicators — only shown when on, so they don't crowd the bar.
    let active_filter_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    if state.filter_dirty    { top_spans.push(Span::styled("[d:dirty] ",    active_filter_style)); }
    if state.filter_behind   { top_spans.push(Span::styled("[b:behind] ",   active_filter_style)); }
    if state.filter_ahead    { top_spans.push(Span::styled("[f:ahead] ",    active_filter_style)); }
    if state.filter_archived { top_spans.push(Span::styled("[a:archived] ", active_filter_style)); }

    if state.sort_mode != SortMode::Default {
        top_spans.push(Span::styled(
            format!("[S:{}] ", state.sort_mode.label()),
            Style::default().fg(Color::Cyan),
        ));
    }
    if !state.filter_text.is_empty() {
        top_spans.push(Span::styled(
            format!("[/:{}_] ", state.filter_text),
            active_filter_style,
        ));
    } else if state.filtering {
        top_spans.push(Span::styled("[/: _] ", active_filter_style));
    }

    top_spans.push(Span::styled(health_str, Style::default().fg(health_col)));
    top_spans.push(Span::styled(pos_str, Style::default().fg(Color::DarkGray)));

    if let Some(ref msg) = state.status_msg {
        top_spans.push(Span::styled(format!("  {}", msg), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
    }

    f.render_widget(Paragraph::new(Line::from(top_spans)), outer[0]);

    if state.tab == Tab::Overview {
        super::tabs::render_overview_global(f, state, outer[1]);
    } else {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(73), Constraint::Percentage(27)])
            .split(outer[1]);

        let left_blk  = Block::default().borders(Borders::ALL);
        let left_inner = left_blk.inner(cols[0]);
        f.render_widget(left_blk, cols[0]);
        state.visible_rows = left_inner.height.saturating_sub(1) as usize;
        render_list(f, state, left_inner);
        render_right(f, state, cols[1]);
    }

    if state.debug_mode {
        let blk   = Block::default().borders(Borders::TOP).title(" debug ")
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = blk.inner(outer[2]);
        f.render_widget(blk, outer[2]);
        let h     = inner.height as usize;
        let n     = state.debug_log.len();
        let start = n.saturating_sub(h);
        let lines: Vec<Line> = state.debug_log[start..]
            .iter()
            .map(|s| Line::from(Span::styled(s.clone(), Style::default().fg(Color::DarkGray))))
            .collect();
        f.render_widget(Paragraph::new(lines), inner);
    } else {
        let dim = Style::default().fg(Color::DarkGray);
        let key = |k: &str| Span::styled(format!("[{}] ", k), dim);
        let sep = || Span::styled("  ", dim);

        // Contextual line 3: scan progress or selection hint.
        let line3 = if state.scan_rx.is_some() {
            Line::from(Span::styled("  scanning repositories…", dim))
        } else if !state.selected.is_empty() {
            Line::from(vec![
                Span::styled(format!("  {} selected", state.selected.len()), Style::default().fg(Color::Cyan)),
                Span::styled("  — ", dim),
                key("s"), Span::styled("sync  ", dim),
                key("r"), Span::styled("release", dim),
            ])
        } else {
            Line::from("")
        };

        let hint_lines: Vec<Line> = if state.modal.is_some() {
            vec![
                Line::from(vec![key("↑↓/jk"), Span::styled("navigate  ", dim), key("Enter"), Span::styled("confirm  ", dim), key("Esc"), Span::styled("cancel", dim)]),
                Line::from(""),
                Line::from(""),
            ]
        } else if state.filtering {
            vec![
                Line::from(vec![Span::styled("  type to filter", dim), sep(), key("Enter"), Span::styled("lock  ", dim), key("Esc"), Span::styled("clear", dim)]),
                Line::from(""),
                Line::from(""),
            ]
        } else if state.confirming.is_some() {
            vec![
                Line::from(vec![key("y"), Span::styled("confirm  ", dim), key("Esc"), Span::styled("cancel", dim)]),
                Line::from(""),
                Line::from(""),
            ]
        } else {
            match state.tab {
                Tab::PRs | Tab::Issues => vec![
                    Line::from(vec![key("↑↓/jk"), Span::styled("navigate  ", dim), key("Tab/←→"), Span::styled("switch tab  ", dim), key("↵"), Span::styled("open in browser  ", dim), key("R"), Span::styled("refresh API  ", dim), key("?"), Span::styled("help  ", dim), key("q"), Span::styled("quit", dim)]),
                    Line::from(""),
                    line3,
                ],
                _ => vec![
                    Line::from(vec![key("↑↓/jk"), Span::styled("navigate  ", dim), key("Tab/←→"), Span::styled("tabs  ", dim), key("/"), Span::styled("filter  ", dim), key("S"), Span::styled("sort & filter  ", dim), key("?"), Span::styled("help  ", dim), key("q"), Span::styled("quit", dim)]),
                    Line::from(vec![key("Space"), Span::styled("select  ", dim), key("R"), Span::styled("API refresh  ", dim), key("N"), Span::styled("new repo", dim)]),
                    line3,
                ],
            }
        };
        f.render_widget(Paragraph::new(hint_lines), outer[2]);
    }

    if state.modal.is_some() {
        render_modal(f, &state.modal, area);
    }
}

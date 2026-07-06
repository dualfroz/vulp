//! The hub's HubSettings tab: workspace-level settings (root, editor,
//! clipboard command, bulk import/de-git actions) and the user-defined
//! repo template list, navigated with the same up/down + Enter pattern as
//! every other list in the hub.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use super::state::HubState;

pub fn render_settings_global(f: &mut Frame, state: &HubState, area: Rect) {
    let dim  = Style::default().fg(Color::DarkGray);
    let yel  = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let mut left_lines: Vec<Line> = vec![];
    left_lines.push(Line::from(Span::styled("  Workspace Settings", yel)));
    left_lines.push(Line::from(Span::styled("  ─────────────────────────────────────────────", dim)));

    struct Item { label: &'static str, value: String }
    let editor = state.editor.clone();
    let clip   = state.clipboard_cmd.clone();
    let workspace = state.workspace_root.to_string_lossy().to_string();

    let acc_count = state.accounts.len().to_string();
    let items: &[Item] = &[
        Item { label: "Workspace root",        value: workspace },
        Item { label: "Editor",                value: editor },
        Item { label: "Clipboard cmd",         value: clip },
        Item { label: "Total repos",           value: state.repos.len().to_string() },
        Item { label: "Total projects",        value: state.projects.len().to_string() },
        Item { label: "Total todos",           value: state.todos.len().to_string() },
        Item { label: "Accounts",              value: acc_count },
        Item { label: "Bulk import repos",     value: "[Enter] clone/pull all from account".into() },
        Item { label: "Import directory",      value: "[Enter] git init + link all subfolders".into() },
        Item { label: "Remove .git (all)",     value: "[Enter] ⚠ de-git all repos in workspace".into() },
    ];

    for (i, item) in items.iter().enumerate() {
        let sel = i == state.hub_settings_cur;
        let bg = if sel { Color::DarkGray } else { Color::Reset };
        left_lines.push(Line::from(vec![
            Span::styled(
                format!("  {:>2}. ", i + 1),
                Style::default().fg(if sel { Color::Cyan } else { Color::DarkGray }).bg(bg),
            ),
            Span::styled(
                format!("{:<20}", item.label),
                Style::default().fg(Color::White).bg(bg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled(
                item.value.chars().take(35).collect::<String>(),
                Style::default().fg(Color::DarkGray).bg(bg),
            ),
        ]));
    }
    left_lines.push(Line::from(""));

    // Templates section
    left_lines.push(Line::from(Span::styled("  Templates", yel)));
    left_lines.push(Line::from(Span::styled("  ─────────────────────────────────────────────", dim)));

    let new_tmpl_sel = state.hub_settings_cur == 10;
    let new_bg = if new_tmpl_sel { Color::DarkGray } else { Color::Reset };
    left_lines.push(Line::from(vec![
        Span::styled(
            format!("  {:>2}. ", 11),
            Style::default().fg(if new_tmpl_sel { Color::Cyan } else { Color::DarkGray }).bg(new_bg),
        ),
        Span::styled(
            format!("{:<20}", "[+] New template"),
            Style::default().fg(Color::Green).bg(new_bg).add_modifier(if new_tmpl_sel { Modifier::BOLD } else { Modifier::empty() }),
        ),
    ]));

    for (ti, t) in state.templates.iter().enumerate() {
        let idx = 11 + ti;
        let sel = idx == state.hub_settings_cur;
        let bg  = if sel { Color::DarkGray } else { Color::Reset };
        let cmd_n = t.commands.len();
        let cmd_s = match cmd_n {
            0 => "(no commands)".to_string(),
            1 => "1 command".to_string(),
            n => format!("{} commands", n),
        };
        left_lines.push(Line::from(vec![
            Span::styled(
                format!("  {:>2}. ", 12 + ti),
                Style::default().fg(if sel { Color::Cyan } else { Color::DarkGray }).bg(bg),
            ),
            Span::styled(
                format!("{:<20}", t.name.chars().take(20).collect::<String>()),
                Style::default().fg(Color::White).bg(bg).add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled(
                cmd_s,
                Style::default().fg(Color::DarkGray).bg(bg),
            ),
        ]));
    }
    if state.templates.is_empty() {
        left_lines.push(Line::from(Span::styled("       (no templates yet)", dim)));
    }

    f.render_widget(Paragraph::new(left_lines), cols[0]);

    let mut right_lines: Vec<Line> = vec![
        Line::from(Span::styled("  Hints", yel)),
        Line::from(Span::styled("  ──────────────────────────────", dim)),
    ];
    right_lines.push(Line::from(Span::styled("  ↑↓ / jk   navigate", dim)));
    right_lines.push(Line::from(Span::styled("  Enter     edit / run item", dim)));
    right_lines.push(Line::from(Span::styled("  D         delete template", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Items 2-3: editable in session.", dim)));
    right_lines.push(Line::from(Span::styled("  Item  8: bulk import from account.", dim)));
    right_lines.push(Line::from(Span::styled("  Item  9: git init + link dir.", dim)));
    right_lines.push(Line::from(Span::styled("  Item 10: remove .git  ⚠", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Templates run on repo create.", dim)));
    right_lines.push(Line::from(Span::styled("  Cmds: semicolon-separated.", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Auth: vulp auth import", dim)));

    f.render_widget(Paragraph::new(right_lines), cols[1]);
}

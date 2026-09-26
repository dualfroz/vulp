use crate::status::format_age_pub;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use super::state::{DiffState, DisplayRow, HubState, PendingAction, Tab};
use super::tabs::age_color;

pub fn r_kv<'a>(key: &'a str, val: &str, val_col: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {:<11}", key), Style::default().fg(Color::DarkGray)),
        Span::styled(val.to_string(), Style::default().fg(val_col)),
    ])
}
pub fn r_kv_color<'a>(key: &'a str, val: &str, val_col: Color) -> Line<'a> {
    r_kv(key, val, val_col)
}
pub fn r_section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {}", title),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ))
}
pub fn r_sep(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        " ".to_string() + &"─".repeat(width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    ))
}
pub fn r_dim(s: String) -> Line<'static> {
    Line::from(Span::styled(s, Style::default().fg(Color::DarkGray)))
}
pub fn r_action_line<'a>(key: char, desc: &'a str, col: Color) -> Line<'a> {
    if key == '\n' {
        return Line::from(Span::styled(desc, Style::default().fg(col)));
    }
    Line::from(vec![
        Span::styled(format!(" [{}]", key), Style::default().fg(col).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", desc), Style::default().fg(Color::White)),
    ])
}
pub fn r_action_key<'a>(key: &'a str, desc: &'a str, col: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" [{}]", key), Style::default().fg(col).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}", desc), Style::default().fg(Color::White)),
    ])
}

pub fn render_right(f: &mut Frame, state: &HubState, area: Rect) {
    if let Some(ref action) = state.confirming {
        render_confirm_overlay(f, state, area, action);
        return;
    }

    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    match state.tab {
        Tab::Repos       => render_right_repo(f, state, inner),
        Tab::Projects    => render_right_project(f, state, inner),
        Tab::Todos       => render_right_todo(f, state, inner),
        Tab::PRs         => render_right_pr(f, state, inner),
        Tab::Issues      => render_right_issue(f, state, inner),
        Tab::Overview | Tab::Stats | Tab::Activity | Tab::HubSettings => {}
    }
}

fn render_confirm_overlay(f: &mut Frame, state: &HubState, area: Rect, action: &PendingAction) {
    let targets = state.action_targets();
    let PendingAction::Release = action;
    let (title, verb) = (" Release ", "Release");
    let blk   = Block::default().borders(Borders::ALL).title(title)
        .style(Style::default().fg(Color::Yellow));
    let inner = blk.inner(area);
    f.render_widget(blk, area);

    let mut lines = vec![
        Line::from(""),
        r_section(verb),
        Line::from(""),
    ];
    for &ri in targets.iter().take(12) {
        if let Some(r) = state.repos.get(ri) {
            lines.push(r_kv(" ·", &r.name, Color::Cyan));
        }
    }
    if targets.len() > 12 {
        lines.push(r_dim(format!("   … and {} more", targets.len() - 12)));
    }
    lines.extend([
        Line::from(""),
        r_kv("Targets", &format!("{} repo(s)", targets.len()), Color::White),
        Line::from(""),
        r_action_line('y', "Confirm", Color::Green),
        r_action_line('n', "Cancel",  Color::Red),
    ]);
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_right_repo(f: &mut Frame, state: &HubState, area: Rect) {
    let actions_h = 20u16;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(actions_h)])
        .split(area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(ri) = state.cur_repo_idx() {
        let r = &state.repos[ri];

        lines.push(Line::from(Span::styled(
            format!(" {}", r.name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
        lines.push(r_sep(area.width));
        lines.push(Line::from(""));

        lines.push(r_kv("Branch",  &r.branch,                        Color::Cyan));
        lines.push(r_kv("Owner",   r.owner.as_deref().unwrap_or("—"), Color::White));
        let proj_name  = state.repo_project(ri).map(|p| p.name.as_str()).unwrap_or("—");
        let proj_color = state.repo_proj_color(ri).unwrap_or(Color::DarkGray);
        lines.push(r_kv_color("Project", proj_name, proj_color));
        lines.push(r_kv("Vis.",  r.visibility.as_deref().unwrap_or("unknown"), match r.visibility.as_deref() { Some("public") => Color::Green, Some("private") => Color::Yellow, _ => Color::DarkGray }));
        lines.push(Line::from(""));

        let (sync_txt, sync_col) = match (r.ahead, r.behind) {
            (0, 0) => ("✓ in sync with remote", Color::Green),
            (a, 0) => (Box::leak(format!("↑ {} commit(s) ahead", a).into_boxed_str()) as &str, Color::Cyan),
            (0, b) => (Box::leak(format!("↓ {} commit(s) behind", b).into_boxed_str()) as &str, Color::Red),
            (a, b) => (Box::leak(format!("↑{} ahead  ↓{} behind", a, b).into_boxed_str()) as &str, Color::Yellow),
        };
        let (dirty_txt, dirty_col) = if r.is_dirty {
            ("⚠ uncommitted changes", Color::Yellow)
        } else {
            ("✓ working tree clean", Color::DarkGray)
        };
        lines.push(r_kv("Changes", dirty_txt,  dirty_col));
        lines.push(r_kv("Remote",  sync_txt,   sync_col));
        if r.stash_count > 0 {
            lines.push(r_kv("Stash", &format!("⌂ {} entr{}", r.stash_count, if r.stash_count == 1 { "y" } else { "ies" }), Color::Magenta));
        }

        if let Some(ex) = state.focused_cache.get(&ri) {
            lines.push(r_kv("Files", &format!("{} tracked", ex.file_count), Color::DarkGray));
        }
        if let Some(tag) = &r.last_tag { lines.push(r_kv("Release", tag, Color::Cyan)); }
        if let Some(lang) = &r.lang    { lines.push(r_kv("Lang", lang, Color::DarkGray)); }
        lines.push(Line::from(""));

        lines.push(r_section("Last commit"));
        lines.push(r_kv("When", &r.last_commit_age, age_color(r.last_commit_ts)));
        if !r.last_commit_msg.is_empty() {
            let msg = r.last_commit_msg.chars().take(area.width.saturating_sub(14) as usize).collect::<String>();
            lines.push(r_kv("Message", &msg, Color::DarkGray));
        }

        if r.is_dirty {
            lines.push(Line::from(""));
            lines.push(r_section("Diff preview"));
            if let Some(ex) = state.focused_cache.get(&ri) {
                match &ex.diff_preview {
                    DiffState::Loading => {
                        lines.push(r_dim(" loading...".into()));
                    }
                    DiffState::Ready(diff_lines) => {
                        if diff_lines.is_empty() {
                            lines.push(r_dim(" (no tracked changes)".into()));
                        } else {
                            for dl in diff_lines.iter().take(8) {
                                let col = if dl.starts_with('+') { Color::Green }
                                          else if dl.starts_with('-') { Color::Red }
                                          else { Color::DarkGray };
                                let max_w = area.width.saturating_sub(2) as usize;
                                let truncated: String = dl.chars().take(max_w).collect();
                                lines.push(Line::from(Span::styled(truncated, Style::default().fg(col))));
                            }
                        }
                    }
                }
            } else {
                lines.push(r_dim(" loading...".into()));
            }
        }

        if !state.selected.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(" {} repos selected (actions apply to all)", state.selected.len()),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
        }
    } else {
        lines.push(r_dim(" ← select a repository".into()));
    }

    f.render_widget(Paragraph::new(lines), layout[0]);

    let action_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Actions ");
    let action_inner = action_block.inner(layout[1]);
    f.render_widget(action_block, layout[1]);
    f.render_widget(Paragraph::new(vec![
        r_action_key("↵ ", "Open full detail",   Color::White),
        r_action_line('\n', "─────────────────", Color::DarkGray),
        r_action_line('o', "Open in editor",     Color::Cyan),
        r_action_line('W', "Open in browser",    Color::Cyan),
        r_action_line('O', "Open file manager",  Color::Cyan),
        r_action_line('y', "Copy path",          Color::Cyan),
        r_action_line('u', "Copy remote URL",    Color::Cyan),
        r_action_line('g', "Edit .gitignore",    Color::Cyan),
        r_action_line('\n', "─────────────────", Color::DarkGray),
        r_action_line('s', "Sync with remote",   Color::Green),
        r_action_line('r', "Create release tag", Color::Green),
        r_action_line('\n', "─────────────────", Color::DarkGray),
        r_action_key("Spc", "Toggle select",     Color::Yellow),
        r_action_line('*', "Select by condition",Color::Yellow),
        r_action_key("^A", "Select all / none",  Color::Yellow),
        r_action_line('\n', "─────────────────", Color::DarkGray),
        r_action_line('B', "Bookmark toggle",    Color::White),
        r_action_line('P', "Pin to top",         Color::White),
        r_action_line('n', "Rename label",       Color::White),
        r_action_line('p', "Manage projects",    Color::White),
        r_action_line('+', "New todo",           Color::White),
        r_action_line('\n', "─────────────────", Color::DarkGray),
        r_action_line('N', "New repo…",          Color::DarkGray),
    ]), action_inner);
}

fn render_right_project(f: &mut Frame, state: &HubState, area: Rect) {
    let actions_h = 7u16;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(actions_h)])
        .split(area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(pi) = state.cur_proj_idx() {
        let p   = &state.projects[pi];
        let col = state.proj_color(pi);
        let all_ri = state.project_repos(pi);
        let dirty_n  = all_ri.iter().filter(|&&ri| state.repos[ri].is_dirty).count();
        let behind_n = all_ri.iter().filter(|&&ri| state.repos[ri].behind > 0).count();
        let open_todos = state.todos.iter()
            .filter(|t| t.project.as_deref() == Some(&p.name)
                && t.status != crate::todos::TodoStatus::Done
                && t.status != crate::todos::TodoStatus::Cancelled)
            .count();

        lines.push(Line::from(Span::styled(
            format!(" ◼  {}", p.name),
            Style::default().fg(col).add_modifier(Modifier::BOLD),
        )));
        lines.push(r_sep(area.width));
        lines.push(Line::from(""));

        lines.push(r_kv("Color",  &p.color, col));
        lines.push(r_kv("Repos",  &format!("{} repositories", all_ri.len()), Color::White));
        lines.push(Line::from(""));

        lines.push(r_section("Health"));
        let health = match (dirty_n, behind_n) {
            (0, 0) => ("✓ all clean", Color::Green),
            (d, 0) => (Box::leak(format!("⚠ {} with changes", d).into_boxed_str()) as &str, Color::Yellow),
            (0, b) => (Box::leak(format!("↓ {} behind remote", b).into_boxed_str()) as &str, Color::Red),
            (d, b) => (Box::leak(format!("⚠ {} changed  ↓ {} behind", d, b).into_boxed_str()) as &str, Color::Yellow),
        };
        lines.push(r_kv("Status", health.0, health.1));
        lines.push(r_kv("Todos",
            if open_todos == 0 { "no open todos" }
            else { Box::leak(format!("{} open", open_todos).into_boxed_str()) },
            if open_todos == 0 { Color::DarkGray } else { Color::Yellow },
        ));
        lines.push(Line::from(""));

        lines.push(r_section("Repositories"));
        for &ri in all_ri.iter().take(8) {
            let r = &state.repos[ri];
            let suf = if r.is_dirty { " ⚠" } else if r.behind > 0 { " ↓" } else { "" };
            lines.push(Line::from(Span::styled(
                format!("  · {}{}", r.name, suf),
                Style::default().fg(if r.behind > 0 { Color::Red } else if r.is_dirty { Color::Yellow } else { Color::DarkGray }),
            )));
        }
        if all_ri.len() > 8 {
            lines.push(r_dim(format!("  … and {} more", all_ri.len() - 8)));
        }
    } else {
        lines.push(r_dim(" ← select a project".into()));
    }

    f.render_widget(Paragraph::new(lines), layout[0]);

    let action_block = Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)).title(" Actions ");
    let action_inner = action_block.inner(layout[1]);
    f.render_widget(action_block, layout[1]);
    f.render_widget(Paragraph::new(vec![
        r_action_line('s', "Sync all repos",     Color::Green),
        r_action_line('p', "Edit this project",  Color::White),
        r_action_key("↵ ", "Open detail view",   Color::White),
        r_action_line('+', "New todo",            Color::White),
        r_action_line('D', "Delete project",      Color::DarkGray),
    ]), action_inner);
}

fn render_right_todo(f: &mut Frame, state: &HubState, area: Rect) {
    let actions_h = 6u16;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(actions_h)])
        .split(area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(ti) = state.cur_todo_idx() {
        let t = &state.todos[ti];

        lines.push(Line::from(Span::styled(
            format!(" {}", t.title),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
        lines.push(r_sep(area.width));
        lines.push(Line::from(""));

        lines.push(r_kv_color("Status", &format!("{} {}", t.status.icon(), t.status.label()), t.status.color()));
        lines.push(r_kv("Project", t.project.as_deref().unwrap_or("— not assigned"), Color::DarkGray));
        lines.push(r_kv("Repo", t.repo.as_deref().unwrap_or("— not assigned"), Color::DarkGray));
        let diff_str = match t.difficulty {
            Some(d) => "★".repeat(d.min(5) as usize) + &"☆".repeat(5usize.saturating_sub(d as usize)),
            None    => "—".into(),
        };
        lines.push(r_kv("Difficulty", &diff_str, Color::Yellow));
        lines.push(r_kv("Estimate", t.estimate.as_deref().unwrap_or("—"), Color::DarkGray));
        lines.push(Line::from(""));

        if !t.description.is_empty() {
            lines.push(r_section("Description"));
            let max_w = (area.width.saturating_sub(3) as usize).max(1);
            for raw_line in t.description.lines().take(6) {
                for chunk in raw_line.chars().collect::<Vec<_>>().chunks(max_w).map(|c| c.iter().collect::<String>()) {
                    lines.push(Line::from(Span::styled(format!("  {}", chunk), Style::default().fg(Color::DarkGray))));
                }
            }
            lines.push(Line::from(""));
        }

        lines.push(r_section("Timeline"));
        lines.push(r_kv("Created", &format_age_pub(t.created_at), Color::DarkGray));
        lines.push(r_kv("Updated", &format_age_pub(t.updated_at), Color::DarkGray));
    } else if state.cur_todo_header_key().is_some() {
        lines.push(r_dim(" Group header — [↵/Space] expand/collapse".into()));
    } else {
        lines.push(r_dim(" ← select a todo".into()));
    }

    f.render_widget(Paragraph::new(lines), layout[0]);

    let action_block = Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)).title(" Actions ");
    let action_inner = action_block.inner(layout[1]);
    f.render_widget(action_block, layout[1]);
    f.render_widget(Paragraph::new(vec![
        r_action_key("Space", "Cycle status",        Color::Green),
        r_action_line('e', "Edit title & description", Color::White),
        r_action_line('+', "New todo",               Color::White),
        r_action_line('D', "Delete",                 Color::Red),
    ]), action_inner);
}

fn render_right_pr(f: &mut Frame, state: &HubState, area: Rect) {
    let di = state.cur_di();
    let pr = di.and_then(|d| {
        if let DisplayRow::Pr { pr_idx } = &state.display[d] { state.prs.get(*pr_idx) } else { None }
    });

    let mut lines: Vec<Line> = vec![r_section("Pull Request"), Line::from("")];
    if let Some(pr) = pr {
        let age = format_age_pub(pr.updated_ts);
        lines.push(r_kv("Repo",   &pr.repo, Color::Cyan));
        lines.push(r_kv("Number", &format!("#{}", pr.number), Color::DarkGray));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!(" \"{}\"", pr.title.chars().take(30).collect::<String>()), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))));
        if pr.title.chars().count() > 30 {
            lines.push(Line::from(Span::styled(format!("  {}", pr.title.chars().skip(30).take(30).collect::<String>()), Style::default().fg(Color::White))));
        }
        lines.push(Line::from(""));
        lines.push(r_kv("Author",  &format!("@{}", pr.author), Color::Green));
        lines.push(r_kv("Updated", &age, Color::DarkGray));
        lines.push(r_kv("Branch",  &format!("{} → {}", pr.head_branch, pr.base_branch), Color::Cyan));
        if pr.draft { lines.push(r_dim(" ⚠ DRAFT".into())); }
        if !pr.labels.is_empty() {
            lines.push(Line::from(""));
            lines.push(r_dim(format!(" Labels: {}", pr.labels.join(", "))));
        }
        lines.push(Line::from(""));
        lines.push(r_dim(" [↵] open in browser".into()));
    } else {
        lines.push(r_dim(" ← select a PR".into()));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn render_right_issue(f: &mut Frame, state: &HubState, area: Rect) {
    let di = state.cur_di();
    let issue = di.and_then(|d| {
        if let DisplayRow::Issue { issue_idx } = &state.display[d] { state.issues.get(*issue_idx) } else { None }
    });

    let mut lines: Vec<Line> = vec![r_section("Issue"), Line::from("")];
    if let Some(issue) = issue {
        let age = format_age_pub(issue.updated_ts);
        lines.push(r_kv("Repo",     &issue.repo, Color::Cyan));
        lines.push(r_kv("Number",   &format!("#{}", issue.number), Color::DarkGray));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(format!(" \"{}\"", issue.title.chars().take(30).collect::<String>()), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))));
        if issue.title.chars().count() > 30 {
            lines.push(Line::from(Span::styled(format!("  {}", issue.title.chars().skip(30).take(30).collect::<String>()), Style::default().fg(Color::White))));
        }
        lines.push(Line::from(""));
        lines.push(r_kv("Author",   &format!("@{}", issue.author), Color::Yellow));
        lines.push(r_kv("Updated",  &age, Color::DarkGray));
        lines.push(r_kv("Comments", &format!("{}", issue.comments), Color::DarkGray));
        if !issue.labels.is_empty() {
            lines.push(Line::from(""));
            lines.push(r_dim(format!(" Labels: {}", issue.labels.join(", "))));
        }
        lines.push(Line::from(""));
        lines.push(r_dim(" [↵] open in browser".into()));
    } else {
        lines.push(r_dim(" ← select an issue".into()));
    }
    f.render_widget(Paragraph::new(lines), area);
}

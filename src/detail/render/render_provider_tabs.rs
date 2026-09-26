use super::super::St;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Cell, Paragraph, Row, Table},
    Frame,
};

pub fn render_prs(f: &mut Frame, st: &St, area: Rect) {
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

pub fn render_issues(f: &mut Frame, st: &St, area: Rect) {
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

pub fn render_repo_todos(f: &mut Frame, st: &St, area: Rect) {
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

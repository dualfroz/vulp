//! Table renderers for the hub's list-style tabs (Repos, Projects, Todos,
//! PRs, Issues) — one row per visible `DisplayRow`, scrolled by
//! `HubState::offset`. The full-screen tabs (Overview, Stats, Activity,
//! HubSettings) have their own renderers in sibling `tabs_*` files.

use crate::status::format_age_pub;
use crate::todos::TodoSource;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Row, Table},
    Frame,
};
use super::state::{DisplayRow, HubState};
use super::tabs::age_color;

pub fn render_repos_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec![
        "    ", "NAME", "BRANCH", "PROJECT", "VISIB", "AGE", "STATUS", "RELEASE", "LICENSE", "CI", "ALERT", "DEPLOY", "STARS",
    ]).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.display.iter().enumerate()
        .skip(state.offset).take(avail)
        .filter_map(|(di, row)| {
            let DisplayRow::Repo { repo_idx: ri } = row else { return None; };
            let r      = &state.repos[*ri];
            let is_cur = di == cur_di;
            let is_sel = state.selected.contains(ri);

            let check_sty = if is_sel { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
            let row_sty   = if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() };

            let proj_name  = state.repo_project(*ri).map(|p| p.name.as_str()).unwrap_or("—");
            let proj_color = state.repo_proj_color(*ri).unwrap_or(Color::DarkGray);

            let vis   = r.visibility.as_deref().unwrap_or("?");
            let rel   = r.last_tag.as_deref().unwrap_or("—");
            let now_ts = chrono::Utc::now().timestamp();
            let is_stale = (now_ts - r.last_commit_ts) / 86400 > 90;
            let d_sym = if r.is_dirty { "⚠" } else if is_stale { "~" } else { "·" };
            let a_str = if r.ahead  > 0 { format!("+{}", r.ahead)  } else { String::new() };
            let b_str = if r.behind > 0 { format!("-{}", r.behind) } else { String::new() };
            let status_str = format!("{} {}{}", d_sym, a_str, b_str);
            let name_col = if is_cur || (!r.is_dirty && r.ahead == 0 && r.behind == 0) {
                Color::White
            } else {
                Color::Yellow
            };
            let lic_str   = if r.has_license { "✓" } else { "—" };
            let (ci_str, ci_col) = match r.ci_status.as_deref() {
                Some("success") => ("✓", Color::Green),
                Some("failure") => ("✗", Color::Red),
                Some("running") => ("●", Color::Yellow),
                _               => ("—", Color::DarkGray),
            };
            let dep_str = match r.dependabot_open {
                Some(0) => "0".to_string(),
                Some(n) => format!("{}", n),
                None    => "—".into(),
            };
            let dep_col = match r.dependabot_open {
                Some(0) | None => Color::DarkGray,
                Some(_)        => Color::Red,
            };
            let deploy_state = r.deploy_status.as_deref().and_then(crate::deploy::DeployState::parse);
            let (deploy_str, deploy_col) = match deploy_state {
                Some(crate::deploy::DeployState::InSync)     => ("✓".to_string(), Color::Green),
                Some(crate::deploy::DeployState::Behind(n))  => (format!("-{n}"), Color::Yellow),
                Some(crate::deploy::DeployState::Diverged)   => ("≠".to_string(), Color::Red),
                None                                          => ("—".to_string(), Color::DarkGray),
            };
            let stars_str = r.stars.map(|s| format!("{}", s)).unwrap_or_else(|| "—".into());
            let stars_col = if r.stars.is_some() { Color::Yellow } else { Color::DarkGray };
            let is_pinned = state.pins.contains(&r.path);
            let prefix = match (is_pinned, r.bookmarked) {
                (true, true)  => "◆★",
                (true, false) => "◆ ",
                (false, true) => "★ ",
                (false, false)=> "  ",
            };

            Some(Row::new(vec![
                Cell::from(format!("{} {}", if is_cur { "▶" } else { " " }, if is_sel { "[✓]" } else { "[ ]" })).style(check_sty),
                Cell::from(format!("{}{}", prefix, r.name)).style(Style::default().fg(name_col)),
                Cell::from(r.branch.chars().take(9).collect::<String>()),
                Cell::from(proj_name.chars().take(21).collect::<String>()).style(Style::default().fg(proj_color)),
                Cell::from(vis).style(match vis { "public" => Style::default().fg(Color::Green), "private" => Style::default().fg(Color::Yellow), _ => Style::default().fg(Color::DarkGray) }),
                Cell::from(r.last_commit_age.clone()).style(Style::default().fg(age_color(r.last_commit_ts))),
                Cell::from(status_str).style(if r.is_dirty || r.behind > 0 { Style::default().fg(Color::Yellow) } else if is_stale { Style::default().fg(Color::Magenta).add_modifier(Modifier::DIM) } else { Style::default().fg(Color::DarkGray) }),
                Cell::from(rel.chars().take(7).collect::<String>()).style(Style::default().fg(Color::Cyan)),
                Cell::from(lic_str).style(Style::default().fg(if r.has_license { Color::Green } else { Color::DarkGray })),
                Cell::from(ci_str).style(Style::default().fg(ci_col)),
                Cell::from(dep_str).style(Style::default().fg(dep_col)),
                Cell::from(deploy_str).style(Style::default().fg(deploy_col)),
                Cell::from(stars_str).style(Style::default().fg(stars_col)),
            ]).style(row_sty))
        })
        .collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Fill(1),
        Constraint::Length(10),
        Constraint::Length(22),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(9),
        Constraint::Length(4),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(8),
    ];
    f.render_widget(Table::new(rows, widths).header(header), area);
}

pub fn render_projects_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec!["    ", "PROJECT", "COLOR", "REPOS", "DIRTY", "BEHIND"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.display.iter().enumerate()
        .skip(state.offset).take(avail)
        .filter_map(|(di, row)| {
            let DisplayRow::Project { proj_idx: pi } = row else { return None; };
            let p       = &state.projects[*pi];
            let all_ri  = state.project_repos(*pi);
            let dirty_n  = all_ri.iter().filter(|&&ri| state.repos[ri].is_dirty).count();
            let behind_n = all_ri.iter().filter(|&&ri| state.repos[ri].behind > 0).count();
            let all_sel  = !all_ri.is_empty() && all_ri.iter().all(|ri| state.selected.contains(ri));
            let any_sel  = all_ri.iter().any(|ri| state.selected.contains(ri));
            let is_cur   = di == cur_di;
            let color    = state.proj_color(*pi);

            let check_sty = if all_sel || any_sel { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
            let row_sty   = if is_cur { Style::default().bg(Color::DarkGray) } else if behind_n > 0 { Style::default().fg(Color::Red) } else if dirty_n > 0 { Style::default().fg(Color::Yellow) } else { Style::default() };

            Some(Row::new(vec![
                Cell::from(format!("{} {}", if is_cur { "▶" } else { " " }, if all_sel { "[✓]" } else if any_sel { "[◈]" } else { "[ ]" })).style(check_sty),
                Cell::from(p.name.clone()).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Cell::from(Line::from(vec![
                    Span::styled("██ ", Style::default().fg(color)),
                    Span::styled(p.color.clone(), Style::default().fg(color)),
                ])),
                Cell::from(all_ri.len().to_string()),
                Cell::from(if dirty_n  > 0 { dirty_n.to_string()  } else { "·".into() }).style(if dirty_n  > 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) }),
                Cell::from(if behind_n > 0 { behind_n.to_string() } else { "·".into() }).style(if behind_n > 0 { Style::default().fg(Color::Red)    } else { Style::default().fg(Color::DarkGray) }),
            ]).style(row_sty))
        })
        .collect();

    f.render_widget(Table::new(rows, [Constraint::Length(6), Constraint::Fill(1), Constraint::Length(9), Constraint::Length(7), Constraint::Length(6), Constraint::Length(7)]).header(header), area);
}

pub fn render_todos_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec![" SRC", " STATUS", "TITLE", "REPO", "PROJECT", "EST", "DIFF", "CREATED"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.display.iter().enumerate()
        .skip(state.offset).take(avail)
        .filter_map(|(di, row)| {
            let DisplayRow::Todo { todo_idx: ti } = row else { return None; };
            let t      = &state.todos[*ti];
            let is_cur = di == cur_di;
            let status_color = t.status.color();
            let proj_name = t.project.as_deref().unwrap_or("—");
            let repo_name = t.repo.as_deref().unwrap_or("—");
            let created   = format_age_pub(t.created_at);
            let est_str   = t.estimate.as_deref().unwrap_or("—").chars().take(10).collect::<String>();
            let diff_str  = match t.difficulty {
                Some(d) => format!("{}{}", "★".repeat(d.min(5) as usize), "☆".repeat(5usize.saturating_sub(d as usize))),
                None    => "—".into(),
            };
            let title_short: String = if t.source == TodoSource::CodeScan {
                let raw = &t.title;
                let trimmed = if let Some(end) = raw.find(']') { raw[end+1..].trim() } else { raw.as_str() };
                trimmed.chars().take(55).collect()
            } else {
                t.title.chars().take(55).collect()
            };
            let src_icon = t.source.icon();
            let src_col  = match t.source { TodoSource::Manual => Color::Cyan, TodoSource::CodeScan => Color::DarkGray };
            let row_sty  = if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() };
            Some(Row::new(vec![
                Cell::from(format!(" {}", src_icon)).style(Style::default().fg(src_col)),
                Cell::from(format!(" {} {}", t.status.icon(), t.status.label()))
                    .style(Style::default().fg(status_color)),
                Cell::from(title_short),
                Cell::from(repo_name).style(Style::default().fg(Color::DarkGray)),
                Cell::from(proj_name).style(Style::default().fg(Color::DarkGray)),
                Cell::from(est_str).style(Style::default().fg(Color::DarkGray)),
                Cell::from(diff_str).style(Style::default().fg(Color::Yellow)),
                Cell::from(created).style(Style::default().fg(Color::DarkGray)),
            ]).style(row_sty))
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(10),
        ]).header(header),
        area,
    );
}

pub fn render_prs_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec!["  REPO", "#", "AGE", "AUTHOR", "TITLE", "DRAFT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.display.iter().enumerate()
        .skip(state.offset).take(avail)
        .filter_map(|(di, row)| {
            let DisplayRow::Pr { pr_idx } = row else { return None; };
            let pr     = &state.prs[*pr_idx];
            let is_cur = di == cur_di;
            let age    = format_age_pub(pr.updated_ts);
            let draft_s = if pr.draft { "draft" } else { "" };
            let row_sty = if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() };
            Some(Row::new(vec![
                Cell::from(format!("  {}", pr.repo.chars().take(14).collect::<String>()))
                    .style(Style::default().fg(Color::Cyan)),
                Cell::from(format!("#{}", pr.number)).style(Style::default().fg(Color::DarkGray)),
                Cell::from(age).style(Style::default().fg(Color::DarkGray)),
                Cell::from(format!("@{}", pr.author.chars().take(12).collect::<String>()))
                    .style(Style::default().fg(Color::Green)),
                Cell::from(pr.title.chars().take(60).collect::<String>()),
                Cell::from(draft_s).style(Style::default().fg(Color::DarkGray)),
            ]).style(row_sty))
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(16), Constraint::Length(7), Constraint::Length(10),
            Constraint::Length(14), Constraint::Fill(1), Constraint::Length(6),
        ]).header(header),
        area,
    );
}

pub fn render_issues_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec!["  REPO", "#", "AGE", "AUTHOR", "TITLE", "CMT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = state.display.iter().enumerate()
        .skip(state.offset).take(avail)
        .filter_map(|(di, row)| {
            let DisplayRow::Issue { issue_idx } = row else { return None; };
            let issue  = &state.issues[*issue_idx];
            let is_cur = di == cur_di;
            let age    = format_age_pub(issue.updated_ts);
            let row_sty = if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() };
            Some(Row::new(vec![
                Cell::from(format!("  {}", issue.repo.chars().take(14).collect::<String>()))
                    .style(Style::default().fg(Color::Cyan)),
                Cell::from(format!("#{}", issue.number)).style(Style::default().fg(Color::DarkGray)),
                Cell::from(age).style(Style::default().fg(Color::DarkGray)),
                Cell::from(format!("@{}", issue.author.chars().take(12).collect::<String>()))
                    .style(Style::default().fg(Color::Yellow)),
                Cell::from(issue.title.chars().take(60).collect::<String>()),
                Cell::from(issue.comments.to_string()).style(Style::default().fg(Color::DarkGray)),
            ]).style(row_sty))
        })
        .collect();

    f.render_widget(
        Table::new(rows, [
            Constraint::Length(16), Constraint::Length(7), Constraint::Length(10),
            Constraint::Length(14), Constraint::Fill(1), Constraint::Length(5),
        ]).header(header),
        area,
    );
}

use crate::status::format_age_pub;
use crate::todos::TodoSource;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
    Frame,
};
use super::state::{DisplayRow, HubState, Tab};

pub fn age_color(ts: i64) -> Color {
    let days = (chrono::Utc::now().timestamp() - ts) / 86400;
    if days < 7 { Color::Green } else if days < 30 { Color::Yellow } else { Color::Red }
}

pub fn render_repos_table(f: &mut Frame, state: &HubState, area: Rect) {
    let avail  = area.height.saturating_sub(1) as usize;
    let cur_di = state.cur_di().unwrap_or(usize::MAX);

    let header = Row::new(vec![
        "    ", "NAME", "BRANCH", "PROJECT", "VISIB", "AGE", "STATUS", "RELEASE", "LICENSE", "STARS",
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
            let lic_str   = if r.license { "✓" } else { "—" };
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
                Cell::from(lic_str).style(Style::default().fg(if r.license { Color::Green } else { Color::DarkGray })),
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
            let DisplayRow::Pr { idx } = row else { return None; };
            let pr     = &state.prs[*idx];
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
            let DisplayRow::Issue { idx } = row else { return None; };
            let issue  = &state.issues[*idx];
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

pub fn render_list(f: &mut Frame, state: &HubState, area: Rect) {
    // New full-screen tabs don't use the list
    match state.tab {
        Tab::Overview    => { render_overview_global(f, state, area); return; }
        Tab::Stats       => { render_stats_global(f, state, area);    return; }
        Tab::HubSettings => { render_settings_global(f, state, area); return; }
        _ => {}
    }

    if state.display.is_empty() {
        let msg = match state.tab {
            Tab::Repos    => "  (no repos match current filters)",
            Tab::Projects => "  No projects — press [p] to create one",
            Tab::Todos    => "  No todos — press [+] to create one",
            Tab::PRs      => "  No PRs cached — press [R] to fetch from GitHub",
            Tab::Issues   => "  No issues cached — press [R] to fetch from GitHub",
            _             => "",
        };
        f.render_widget(ratatui::widgets::Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)), area);
        return;
    }
    match state.tab {
        Tab::Repos       => render_repos_table(f, state, area),
        Tab::Projects    => render_projects_table(f, state, area),
        Tab::Todos       => render_todos_table(f, state, area),
        Tab::PRs         => render_prs_table(f, state, area),
        Tab::Issues      => render_issues_table(f, state, area),
        _                => {}
    }
}

// ── Overview global tab ───────────────────────────────────────────────────────

pub fn render_overview_global(f: &mut Frame, state: &HubState, area: Rect) {
    use ratatui::widgets::{Block, Borders};

    let dim  = Style::default().fg(Color::DarkGray);
    let yel  = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let cyn  = Style::default().fg(Color::Cyan);
    let grn  = Style::default().fg(Color::Green);
    let red  = Style::default().fg(Color::Red);
    let wht  = Style::default().fg(Color::White);

    // ── Shared stats ─────────────────────────────────────────────────────────
    let total_repos  = state.repos.len();
    let dirty_c      = state.repos.iter().filter(|r| r.is_dirty).count();
    let behind_c     = state.repos.iter().filter(|r| r.behind > 0).count();
    let ahead_c      = state.repos.iter().filter(|r| r.ahead > 0).count();
    let no_lic_c     = state.repos.iter().filter(|r| !r.license).count();
    let open_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Open)).count();
    let inprog_todos = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::InProgress)).count();
    let done_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Done)).count();
    let canc_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Cancelled)).count();

    // ── Bento rows: 32% / 42% / 26% ─────────────────────────────────────────
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(42), Constraint::Percentage(26)])
        .split(area);

    // ── Row 1: App/Workspace (75%) + Account (25%) ───────────────────────────
    let row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(rows[0]);

    // Cell 1 — App / Workspace
    let app_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" workspace ", yel))
        .border_style(dim);
    let app_inner = app_blk.inner(row1[0]);
    f.render_widget(app_blk, row1[0]);
    {
        let version    = env!("CARGO_PKG_VERSION");
        let debug_flag = if state.debug_mode { " [debug]" } else { "" };
        let ws         = state.workspace_root.display().to_string();
        let ws_short   = if ws.starts_with("/home/") {
            ws.splitn(4, '/').nth(3).map(|s| format!("~/{}", s)).unwrap_or(ws.clone())
        } else { ws };
        let proj_c     = state.projects.len();
        let prs_c      = state.prs.len();
        let iss_c      = state.issues.len();

        let clean_c   = total_repos.saturating_sub(dirty_c + behind_c);
        let bar_total = 20usize;
        let clean_n   = if total_repos > 0 { bar_total * clean_c   / total_repos } else { bar_total };
        let dirty_n   = if total_repos > 0 { bar_total * dirty_c   / total_repos } else { 0 };
        let behind_n  = bar_total.saturating_sub(clean_n + dirty_n);
        let health_bar = format!("{}{}{}",
            "█".repeat(clean_n),
            "▓".repeat(dirty_n),
            "░".repeat(behind_n),
        );
        let health_col = if behind_c > 0 { Color::Red } else if dirty_c > 0 { Color::Yellow } else { Color::Green };

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(format!(" fxg v{}{}", version, debug_flag), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled("  workspace ", dim),
                Span::styled(ws_short, wht),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(format!(" Repos: {:>3}", total_repos), wht),
                Span::styled(format!("   Projects: {:>2}", proj_c), wht),
                Span::styled(format!("   Todos: {:>2} open", open_todos), if open_todos > 0 { cyn } else { dim }),
                Span::styled(format!("   PRs: {:>2}", prs_c), if prs_c > 0 { cyn } else { dim }),
                Span::styled(format!("  Issues: {:>2}", iss_c), if iss_c > 0 { Style::default().fg(Color::Yellow) } else { dim }),
            ]),
            Line::from(vec![
                Span::styled(" Health  ", dim),
                Span::styled(health_bar, Style::default().fg(health_col)),
                Span::styled(format!("  clean {}/{}", clean_c, total_repos), Style::default().fg(health_col)),
            ]),
            Line::from(vec![
                Span::styled(format!("  ↓ behind: {}", behind_c), if behind_c > 0 { red } else { dim }),
                Span::styled(format!("   ⚠ dirty: {}", dirty_c), if dirty_c > 0 { Style::default().fg(Color::Yellow) } else { dim }),
                Span::styled(format!("   ↑ ahead: {}", ahead_c), if ahead_c > 0 { grn } else { dim }),
            ]),
        ];

        // Pad remaining space
        let used = lines.len();
        let avail = app_inner.height as usize;
        for _ in used..avail { lines.push(Line::from("")); }

        f.render_widget(Paragraph::new(lines), app_inner);
    }

    // Cell 2 — Account
    let acc_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" account ", yel))
        .border_style(dim);
    let acc_inner = acc_blk.inner(row1[1]);
    f.render_widget(acc_blk, row1[1]);
    {
        let mut lines: Vec<Line> = vec![];
        if state.accounts.is_empty() {
            lines.push(Line::from(Span::styled(" no accounts", dim)));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(" run:", dim)));
            lines.push(Line::from(Span::styled(" fxg auth import", Style::default().fg(Color::Cyan))));
        } else {
            for acc in &state.accounts {
                let token_ok  = crate::auth::get_token(&acc.id()).is_ok();
                let tok_icon  = if token_ok { "✓" } else { "✗" };
                let tok_sty   = if token_ok { grn } else { red };
                let def_mark  = if acc.is_default { " ★" } else { "" };
                let repo_cnt  = state.repos.iter().filter(|r| r.owner.as_deref() == Some(acc.username.as_str())).count();
                let dirty_cnt = state.repos.iter().filter(|r| r.owner.as_deref() == Some(acc.username.as_str()) && r.is_dirty).count();
                let beh_cnt   = state.repos.iter().filter(|r| r.owner.as_deref() == Some(acc.username.as_str()) && r.behind > 0).count();
                let pr_cnt    = state.prs.iter().filter(|pr| pr.author == acc.username).count();
                let iss_cnt   = state.issues.iter().filter(|i| i.author == acc.username).count();
                lines.push(Line::from(vec![
                    Span::styled(format!(" @{}{}", acc.username, def_mark), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(format!("  {:<8}", acc.provider), dim),
                    Span::styled(tok_icon, tok_sty),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(format!("  {} repo{}", repo_cnt, if repo_cnt == 1 { "" } else { "s" }), wht),
                    if dirty_cnt > 0 || beh_cnt > 0 {
                        Span::styled(format!("  ↓{} ⚠{}", beh_cnt, dirty_cnt), Style::default().fg(Color::Yellow))
                    } else {
                        Span::styled("  ✓", grn)
                    },
                ]));
                if pr_cnt > 0 || iss_cnt > 0 {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} PR{}  {} issue{}", pr_cnt, if pr_cnt == 1 { "" } else { "s" }, iss_cnt, if iss_cnt == 1 { "" } else { "s" }), dim),
                    ]));
                }
                lines.push(Line::from(""));
            }
        }
        f.render_widget(Paragraph::new(lines), acc_inner);
    }

    // ── Row 2: Health (50%) + Recent Activity (50%) ──────────────────────────
    let row2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    // Cell 3 — Health Stats
    let health_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" health ", yel))
        .border_style(dim);
    let health_inner = health_blk.inner(row2[0]);
    f.render_widget(health_blk, row2[0]);
    {
        // Language breakdown from global stats (if available)
        let top_langs: Vec<(String, usize)> = if let Some(gs) = &state.global_stats {
            gs.lang_dist.iter().take(3).map(|(l, c)| (l.clone(), *c)).collect()
        } else { vec![] };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" Repos:     {:>4}", total_repos), wht),
                Span::styled(format!("   Projects:{:>3}", state.projects.len()), wht),
            ]),
            Line::from(vec![
                Span::styled(format!(" Dirty:     {:>4}", dirty_c),  if dirty_c  > 0 { Style::default().fg(Color::Yellow) } else { dim }),
                Span::styled(format!("   Behind:  {:>3}", behind_c), if behind_c > 0 { red } else { dim }),
            ]),
            Line::from(vec![
                Span::styled(format!(" Ahead:     {:>4}", ahead_c),  if ahead_c  > 0 { grn } else { dim }),
                Span::styled(format!("   No lic:  {:>3}", no_lic_c), if no_lic_c > 0 { Style::default().fg(Color::Yellow) } else { dim }),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(format!(" Todos open:{:>4}", open_todos), cyn),
                Span::styled(format!("   In prog:{:>3}", inprog_todos), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled(format!(" Done:      {:>4}", done_todos), grn),
                Span::styled(format!("   Cancld:  {:>3}", canc_todos), dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" PRs:       {:>4}", state.prs.len()), cyn),
                Span::styled(format!("   Issues:  {:>3}", state.issues.len()), Style::default().fg(Color::Yellow)),
            ]),
        ];
        if !top_langs.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(" Languages (top 3)", dim)));
            for (lang, cnt) in &top_langs {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {:<10}", lang.chars().take(10).collect::<String>()), wht),
                    Span::styled(format!("{} repo{}", cnt, if *cnt == 1 { "" } else { "s" }), dim),
                ]));
            }
        }
        f.render_widget(Paragraph::new(lines), health_inner);
    }

    // Cell 4 — Recent Activity
    let act_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" recent activity ", yel))
        .border_style(dim);
    let act_inner = act_blk.inner(row2[1]);
    f.render_widget(act_blk, row2[1]);
    {
        let mut sorted: Vec<&crate::status::RepoSummary> = state.repos.iter().collect();
        sorted.sort_by(|a, b| b.last_commit_ts.cmp(&a.last_commit_ts));
        let max_rows  = act_inner.height as usize;
        let name_w    = (act_inner.width as usize).saturating_sub(22).min(20).max(8);
        let msg_w     = (act_inner.width as usize).saturating_sub(name_w + 10).max(5);
        let mut lines: Vec<Line> = sorted.iter().take(max_rows).map(|r| {
            let age_col = age_color(r.last_commit_ts);
            let name    = r.name.chars().take(name_w).collect::<String>();
            let msg     = r.last_commit_msg.chars().take(msg_w).collect::<String>();
            Line::from(vec![
                Span::styled(format!("{:>5}  ", format_age_pub(r.last_commit_ts)), Style::default().fg(age_col)),
                Span::styled(format!("{:<width$} ", name, width = name_w), wht),
                Span::styled(msg, dim),
            ])
        }).collect();
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(" no commits yet", dim)));
        }
        f.render_widget(Paragraph::new(lines), act_inner);
    }

    // ── Row 3: Todos (25%) + PRs/Issues (75%) ────────────────────────────────
    let row3 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(rows[2]);

    // Cell 5 — Todos summary
    let todo_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" todos ", yel))
        .border_style(dim);
    let todo_inner = todo_blk.inner(row3[0]);
    f.render_widget(todo_blk, row3[0]);
    {
        let name_w = (todo_inner.width as usize).saturating_sub(4).max(5);
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("{:>3} ", open_todos),     cyn),
                Span::styled("open  ",  dim),
                Span::styled(format!("{:>3} ", inprog_todos), Style::default().fg(Color::Yellow)),
                Span::styled("prog", dim),
            ]),
            Line::from(vec![
                Span::styled(format!("{:>3} ", done_todos),  grn),
                Span::styled("done  ", dim),
                Span::styled(format!("{:>3} ", canc_todos),  dim),
                Span::styled("canc", dim),
            ]),
        ];
        // Recent open/in-progress todos
        let urgent: Vec<&crate::todos::Todo> = state.todos.iter()
            .filter(|t| matches!(t.status, crate::todos::TodoStatus::Open | crate::todos::TodoStatus::InProgress)
                && t.source == TodoSource::Manual)
            .take((todo_inner.height as usize).saturating_sub(4))
            .collect();
        if !urgent.is_empty() {
            lines.push(Line::from(""));
            for t in urgent {
                let icon = t.status.icon();
                let title = t.title.chars().take(name_w).collect::<String>();
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(t.status.color())),
                    Span::styled(title, wht),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(" [4] Todos", dim)));
        f.render_widget(Paragraph::new(lines), todo_inner);
    }

    // Cell 6 — PRs / Issues notifications
    let notif_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" prs & issues ", yel))
        .border_style(dim);
    let notif_inner = notif_blk.inner(row3[1]);
    f.render_widget(notif_blk, row3[1]);
    {
        let max_rows = notif_inner.height.saturating_sub(1) as usize;
        let title_w  = (notif_inner.width as usize).saturating_sub(40).min(40).max(10);

        // Merge PRs and Issues into a unified notification stream, newest first.
        struct Notif<'a> {
            kind:   &'a str,
            number: u64,
            repo:   &'a str,
            title:  &'a str,
            author: &'a str,
            ts:     i64,
        }
        let mut notifs: Vec<Notif> = vec![];
        for pr in &state.prs {
            notifs.push(Notif { kind: "PR ", number: pr.number as u64, repo: &pr.repo, title: &pr.title, author: &pr.author, ts: pr.created_ts });
        }
        for iss in &state.issues {
            notifs.push(Notif { kind: "ISS", number: iss.number as u64, repo: &iss.repo, title: &iss.title, author: &iss.author, ts: iss.created_ts });
        }
        notifs.sort_by(|a, b| b.ts.cmp(&a.ts));

        let mut lines: Vec<Line> = if notifs.is_empty() {
            vec![Line::from(Span::styled(" no open PRs or issues", dim))]
        } else {
            notifs.iter().take(max_rows).map(|n| {
                let age = format_age_pub(n.ts);
                let kind_col = if n.kind == "PR " { cyn } else { Style::default().fg(Color::Yellow) };
                let repo_w = 12usize;
                let repo = n.repo.chars().take(repo_w).collect::<String>();
                let title = n.title.chars().take(title_w).collect::<String>();
                let auth  = n.author.chars().take(10).collect::<String>();
                Line::from(vec![
                    Span::styled(format!(" {} #{:<4} ", n.kind, n.number), kind_col),
                    Span::styled(format!("{:<width$} ", repo, width = repo_w), wht),
                    Span::styled(format!("{:<width$} ", title, width = title_w), dim),
                    Span::styled(format!("@{:<10} ", auth), dim),
                    Span::styled(age, dim),
                ])
            }).collect()
        };
        lines.push(Line::from(vec![
            Span::styled("  [5] PRs  [6] Issues", dim),
        ]));
        f.render_widget(Paragraph::new(lines), notif_inner);
    }
}

// ── Stats global tab ──────────────────────────────────────────────────────────

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
        format!("  Global Statistics  [7:{}]  [p] change period", state.global_stats_period.label()),
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

// ── Settings global tab ───────────────────────────────────────────────────────

pub fn render_settings_global(f: &mut Frame, state: &HubState, area: Rect) {
    let dim  = Style::default().fg(Color::DarkGray);
    let yel  = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let wht  = Style::default().fg(Color::White);

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
        Item { label: "Pull all GitHub repos", value: "[Enter] clone/pull all from account".into() },
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

    let mut right_lines: Vec<Line> = vec![];
    right_lines.push(Line::from(Span::styled("  Hints", yel)));
    right_lines.push(Line::from(Span::styled("  ──────────────────────────────", dim)));
    right_lines.push(Line::from(Span::styled("  ↑↓ / jk   navigate", dim)));
    right_lines.push(Line::from(Span::styled("  Enter     edit / run item", dim)));
    right_lines.push(Line::from(Span::styled("  D         delete template", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Items 2-3: editable in session.", dim)));
    right_lines.push(Line::from(Span::styled("  Item  8: pull all from GitHub.", dim)));
    right_lines.push(Line::from(Span::styled("  Item  9: git init + link dir.", dim)));
    right_lines.push(Line::from(Span::styled("  Item 10: remove .git  ⚠", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Templates run on repo create.", dim)));
    right_lines.push(Line::from(Span::styled("  Cmds: semicolon-separated.", dim)));
    right_lines.push(Line::from(""));
    right_lines.push(Line::from(Span::styled("  Auth: fxg auth import", dim)));

    f.render_widget(Paragraph::new(right_lines), cols[1]);
}

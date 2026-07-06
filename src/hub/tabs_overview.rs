//! The hub's Overview tab: a bento-box dashboard (workspace info in columns,
//! general settings, a merged activity feed, then todos/PRs/issues)
//! summarizing the whole workspace at a glance, rather than listing
//! individual rows like the other tabs.

use crate::status::format_age_pub;
use crate::todos::TodoSource;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use super::state::HubState;
use super::tabs::age_color;

/// Color used for an activity-kind label in the merged feed.
fn kind_color(kind: crate::activity::ActivityKind) -> Color {
    use crate::activity::ActivityKind as K;
    match kind {
        K::Commit  => Color::Green,
        K::Push    => Color::Cyan,
        K::Pull    => Color::Blue,
        K::Release => Color::Magenta,
        K::NewRepo => Color::Green,
        K::Todo    => Color::Yellow,
        K::Pr      => Color::Cyan,
        K::Issue   => Color::Yellow,
        K::License => Color::Magenta,
    }
}

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
    let no_lic_c     = state.repos.iter().filter(|r| !r.has_license).count();
    let open_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Open)).count();
    let inprog_todos = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::InProgress)).count();
    let done_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Done)).count();
    let canc_todos   = state.todos.iter().filter(|t| matches!(t.status, crate::todos::TodoStatus::Cancelled)).count();

    // ── Bento rows: 36% / 38% / 26% ─────────────────────────────────────────
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(38), Constraint::Percentage(26)])
        .split(area);

    // ── Row 1: Workspace (75%) + General Settings (25%) ──────────────────────
    let row1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(rows[0]);

    // Cell 1 — Workspace: header line, then three inner columns
    let app_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" workspace ", yel))
        .title_alignment(Alignment::Center)
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

        // Header line spans the whole tile, columns start below it.
        let header_area = Rect { height: 1, ..app_inner };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!(" vulp v{}{}", version, debug_flag), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("  workspace ", dim),
            Span::styled(ws_short, wht),
        ])), header_area);

        let body = Rect {
            y: app_inner.y + 2,
            height: app_inner.height.saturating_sub(2),
            ..app_inner
        };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), Constraint::Percentage(35), Constraint::Percentage(35),
            ])
            .split(body);

        // Col A — general counts, one stat per line, then health
        {
            let acct_names: std::collections::HashSet<&str> =
                state.accounts.iter().map(|a| a.username.as_str()).collect();
            let orgs: std::collections::HashSet<&str> = state.repos.iter()
                .filter_map(|r| r.owner.as_deref())
                .filter(|o| !acct_names.contains(o))
                .collect();
            let vulp_pushes = state.vulp_log.iter()
                .filter(|e| e.kind == crate::activity::ActivityKind::Push)
                .count();

            let clean_c   = total_repos.saturating_sub(dirty_c + behind_c);
            let bar_total = 12usize;
            let clean_n   = if total_repos > 0 { bar_total * clean_c   / total_repos } else { bar_total };
            let dirty_n   = if total_repos > 0 { bar_total * dirty_c   / total_repos } else { 0 };
            let behind_n  = bar_total.saturating_sub(clean_n + dirty_n);
            let health_bar = format!("{}{}{}",
                "█".repeat(clean_n), "▓".repeat(dirty_n), "░".repeat(behind_n));
            let health_col = if behind_c > 0 { Color::Red } else if dirty_c > 0 { Color::Yellow } else { Color::Green };

            let stat = |label: &str, value: String, style: Style| Line::from(vec![
                Span::styled(format!(" {:<14}", label), dim),
                Span::styled(value, style),
            ]);

            let commits_val = match &state.global_stats {
                Some(gs) => format!("{} ({})", gs.total_commits, state.global_stats_period.label()),
                None => "computing...".into(),
            };

            let lines: Vec<Line> = vec![
                stat("Repositories", format!("{}", total_repos), wht),
                stat("Projects",     format!("{}", state.projects.len()), wht),
                stat("Accounts",     format!("{}", state.accounts.len()), wht),
                stat("Organizations",format!("{}", orgs.len()), wht),
                stat("Commits",      commits_val, wht),
                stat("Pushes (vulp)",format!("{}", vulp_pushes), wht),
                stat("PRs",          format!("{}", state.prs.len()), if state.prs.is_empty() { dim } else { cyn }),
                stat("Issues",       format!("{}", state.issues.len()), if state.issues.is_empty() { dim } else { Style::default().fg(Color::Yellow) }),
                stat("Todos",        format!("{}", open_todos), if open_todos == 0 { dim } else { cyn }),
                Line::from(vec![
                    Span::styled(" Health ", dim),
                    Span::styled(health_bar, Style::default().fg(health_col)),
                    Span::styled(format!(" {}/{}", clean_c, total_repos), Style::default().fg(health_col)),
                ]),
                Line::from(vec![
                    Span::styled(format!(" ↓{}", behind_c), if behind_c > 0 { red } else { dim }),
                    Span::styled(format!("  ⚠{}", dirty_c), if dirty_c > 0 { Style::default().fg(Color::Yellow) } else { dim }),
                    Span::styled(format!("  ↑{}", ahead_c), if ahead_c > 0 { grn } else { dim }),
                    Span::styled(format!("  no-lic {}", no_lic_c), if no_lic_c > 0 { Style::default().fg(Color::Yellow) } else { dim }),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), cols[0]);
        }

        // Col B — project list
        {
            let max_rows = (cols[1].height as usize).saturating_sub(1);
            let name_w   = (cols[1].width as usize).saturating_sub(8).max(5);
            let mut lines: Vec<Line> = vec![Line::from(Span::styled(" projects", dim))];
            if state.projects.is_empty() {
                lines.push(Line::from(Span::styled(" no projects yet", dim)));
                lines.push(Line::from(Span::styled(" [p] create one", dim)));
            } else {
                for (pi, p) in state.projects.iter().take(max_rows).enumerate() {
                    let repo_cnt = state.project_repos(pi).len();
                    let name = p.name.chars().take(name_w).collect::<String>();
                    lines.push(Line::from(vec![
                        Span::styled(" ● ", Style::default().fg(state.proj_color(pi))),
                        Span::styled(format!("{:<width$} ", name, width = name_w), wht),
                        Span::styled(format!("{} repo{}", repo_cnt, if repo_cnt == 1 { "" } else { "s" }), dim),
                    ]));
                }
            }
            f.render_widget(Paragraph::new(lines), cols[1]);
        }

        // Col C — top repos (and projects) by commits in the current period,
        // as many as fit
        {
            let period = state.global_stats_period.label();
            let name_w = (cols[2].width as usize).saturating_sub(10).max(5);
            let mut lines: Vec<Line> = vec![Line::from(Span::styled(format!(" top repos ({}) by commits", period), dim))];
            match &state.global_stats {
                None => lines.push(Line::from(Span::styled(" computing...", dim))),
                Some(gs) => {
                    if gs.repo_commits.is_empty() {
                        lines.push(Line::from(Span::styled(" no commits in period", dim)));
                    }
                    let height = cols[2].height as usize;
                    // Leave room for a top-projects section when projects exist.
                    let proj_section = if state.projects.is_empty() { 0 } else { 4 };
                    let repo_rows = height.saturating_sub(1 + proj_section).max(3);
                    for (name, cnt) in gs.repo_commits.iter().take(repo_rows) {
                        let n = name.chars().take(name_w).collect::<String>();
                        lines.push(Line::from(vec![
                            Span::styled(format!(" {:<width$} ", n, width = name_w), wht),
                            Span::styled(format!("{}", cnt), cyn),
                        ]));
                    }
                    if !state.projects.is_empty() {
                        let commit_map: std::collections::HashMap<&str, usize> =
                            gs.repo_commits.iter().map(|(n, c)| (n.as_str(), *c)).collect();
                        let mut proj_commits: Vec<(&str, usize)> = state.projects.iter().enumerate()
                            .map(|(pi, p)| {
                                let sum = state.project_repos(pi).iter()
                                    .filter_map(|&ri| state.repos.get(ri))
                                    .filter_map(|r| commit_map.get(r.name.as_str()))
                                    .sum();
                                (p.name.as_str(), sum)
                            })
                            .filter(|(_, c)| *c > 0)
                            .collect();
                        proj_commits.sort_by(|a, b| b.1.cmp(&a.1));
                        if !proj_commits.is_empty() {
                            lines.push(Line::from(Span::styled(format!(" top projects ({})", period), dim)));
                            for (name, cnt) in proj_commits.iter().take(3) {
                                let n = name.chars().take(name_w).collect::<String>();
                                lines.push(Line::from(vec![
                                    Span::styled(format!(" {:<width$} ", n, width = name_w), wht),
                                    Span::styled(format!("{}", cnt), cyn),
                                ]));
                            }
                        }
                    }
                }
            }
            f.render_widget(Paragraph::new(lines), cols[2]);
        }
    }

    // Cell 2 — General Settings
    let acc_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" general settings ", yel))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let acc_inner = acc_blk.inner(row1[1]);
    f.render_widget(acc_blk, row1[1]);
    {
        let mut lines: Vec<Line> = vec![];
        let version = env!("CARGO_PKG_VERSION");

        // Version + provenance. Prefer real release metadata; fall back to
        // the compile-time build date when no GitHub release exists.
        lines.push(Line::from(Span::styled(format!(" vulp v{}", version), cyn)));
        if let Some(Ok(cur)) = &state.current_release {
            let date = cur.published_at.split('T').next().unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(" released ", dim),
                Span::styled(date.to_string(), wht),
                Span::styled(" by ", dim),
                Span::styled(format!("@{}", cur.author), wht),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(" built ", dim),
                Span::styled(env!("VULP_BUILD_DATE"), wht),
                Span::styled(" by ", dim),
                Span::styled(env!("CARGO_PKG_AUTHORS"), wht),
            ]));
        }
        match &state.latest_release {
            Some(Ok(latest)) => {
                if crate::release_info::is_outdated(version, &latest.tag) {
                    lines.push(Line::from(vec![
                        Span::styled(format!(" ↑ {} available", latest.tag), Style::default().fg(Color::Yellow)),
                        Span::styled(" — [v] notes", cyn),
                    ]));
                } else {
                    lines.push(Line::from(Span::styled(" ✓ no new updates available", grn)));
                }
            }
            Some(Err(e)) => {
                let msg = if e.contains("404") { "no releases published yet" } else { "update check failed (offline?)" };
                lines.push(Line::from(Span::styled(format!(" {}", msg), dim)));
            }
            None => {
                lines.push(Line::from(Span::styled(" checking for updates...", dim)));
            }
        }
        lines.push(Line::from(""));

        if state.accounts.is_empty() {
            lines.push(Line::from(Span::styled(" no accounts", dim)));
            lines.push(Line::from(Span::styled(" run: vulp auth import", Style::default().fg(Color::Cyan))));
        } else {
            for acc in &state.accounts {
                let token_ok = crate::auth::get_token(&acc.id()).is_ok();
                let def_mark = if acc.is_default { " ★" } else { "" };
                lines.push(Line::from(vec![
                    Span::styled(" account ", dim),
                    Span::styled(format!("@{}{}", acc.username, def_mark), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" · {}", acc.provider), dim),
                    if token_ok { Span::styled(" ✓", grn) } else { Span::styled(" ✗ token", red) },
                ]));
                let mut details: Vec<String> = Vec::new();
                if let Some(since) = state.account_since.get(&acc.username) {
                    details.push(format!("since {}", since));
                }
                if let Some(added) = &acc.added_at {
                    details.push(format!("added to vulp {}", added));
                }
                if !details.is_empty() {
                    lines.push(Line::from(Span::styled(format!("   {}", details.join(" · ")), dim)));
                }
            }
        }
        f.render_widget(Paragraph::new(lines), acc_inner);
    }

    // ── Row 2: Recent Activity — merged feed: latest git commit per repo +
    // vulp-performed actions (push/pull/release/new repo/todo/PR/issue) ─────
    let act_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" recent activity ", yel))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let act_inner = act_blk.inner(rows[1]);
    f.render_widget(act_blk, rows[1]);
    {
        use crate::activity::ActivityKind;
        struct FeedRow<'a> { ts: i64, kind: ActivityKind, repo: &'a str, detail: &'a str }

        let mut feed: Vec<FeedRow> = state.repos.iter()
            .filter(|r| r.last_commit_ts > 0)
            .map(|r| FeedRow { ts: r.last_commit_ts, kind: ActivityKind::Commit, repo: &r.name, detail: &r.last_commit_msg })
            .collect();
        // Commits come from git itself (authoritative, sees work done outside
        // vulp too) — take everything else from vulp's own action log.
        feed.extend(state.vulp_log.iter()
            .filter(|e| e.kind != ActivityKind::Commit)
            .map(|e| FeedRow { ts: e.ts, kind: e.kind, repo: &e.repo, detail: &e.detail }));
        feed.sort_by(|a, b| b.ts.cmp(&a.ts));

        let max_rows = act_inner.height as usize;
        let age_w    = 12usize;
        let kind_w   = 10usize;
        let name_w   = feed.iter().take(max_rows)
            .map(|r| r.repo.chars().count())
            .max()
            .unwrap_or(8)
            .min((act_inner.width as usize) * 2 / 5)
            .max(8);
        let msg_w    = (act_inner.width as usize).saturating_sub(age_w + kind_w + name_w + 4).max(5);

        let mut lines: Vec<Line> = feed.iter().take(max_rows).map(|r| {
            let name   = r.repo.chars().take(name_w).collect::<String>();
            let detail = r.detail.chars().take(msg_w).collect::<String>();
            Line::from(vec![
                Span::styled(format!(" {:<width$} ", format_age_pub(r.ts), width = age_w), Style::default().fg(age_color(r.ts))),
                Span::styled(format!("{:<width$} ", r.kind.label(), width = kind_w), Style::default().fg(kind_color(r.kind))),
                Span::styled(format!("{:<width$} ", name, width = name_w), wht),
                Span::styled(detail, dim),
            ])
        }).collect();
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(" no activity yet", dim)));
        }
        f.render_widget(Paragraph::new(lines), act_inner);
    }

    // ── Row 3: Todos (33%) + PRs (33%) + Issues (34%) ────────────────────────
    let row3 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34),
        ])
        .split(rows[2]);

    // Todos — stacked status counts, then most recent todos
    let todo_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" todos ", yel))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let todo_inner = todo_blk.inner(row3[0]);
    f.render_widget(todo_blk, row3[0]);
    {
        let name_w = (todo_inner.width as usize).saturating_sub(4).max(5);
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" {:>3} ", open_todos), cyn),
                Span::styled("open", dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" {:>3} ", inprog_todos), Style::default().fg(Color::Yellow)),
                Span::styled("in progress", dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" {:>3} ", done_todos), grn),
                Span::styled("done", dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" {:>3} ", canc_todos), dim),
                Span::styled("cancelled", dim),
            ]),
        ];
        let mut recent: Vec<&crate::todos::Todo> = state.todos.iter()
            .filter(|t| t.source == TodoSource::Manual)
            .collect();
        recent.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let slots = (todo_inner.height as usize).saturating_sub(lines.len() + 1);
        if slots > 0 && !recent.is_empty() {
            lines.push(Line::from(""));
            for t in recent.iter().take(slots) {
                let icon  = t.status.icon();
                let title = t.title.chars().take(name_w).collect::<String>();
                lines.push(Line::from(vec![
                    Span::styled(format!(" {} ", icon), Style::default().fg(t.status.color())),
                    Span::styled(title, wht),
                ]));
            }
        }
        f.render_widget(Paragraph::new(lines), todo_inner);
    }

    // PRs — status breakdown (incl. merged/closed from the search API when
    // refreshed), then recent titles
    let pr_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" prs ", cyn))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let pr_inner = pr_blk.inner(row3[1]);
    f.render_widget(pr_blk, row3[1]);
    {
        let title_w = (pr_inner.width as usize).saturating_sub(9).max(6);
        let ready_c = state.prs.iter().filter(|p| !p.draft).count();
        let draft_c = state.prs.iter().filter(|p| p.draft).count();
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" {:>3} ", ready_c), if ready_c == 0 { dim } else { cyn }),
                Span::styled("open", dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" {:>3} ", draft_c), if draft_c == 0 { dim } else { Style::default().fg(Color::Yellow) }),
                Span::styled("draft", dim),
            ]),
        ];
        match state.closed_counts {
            Some((merged, closed, _)) => {
                lines.push(Line::from(vec![
                    Span::styled(format!(" {:>3} ", merged), if merged == 0 { dim } else { Style::default().fg(Color::Magenta) }),
                    Span::styled("merged", dim),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(format!(" {:>3} ", closed), dim),
                    Span::styled("closed", dim),
                ]));
            }
            None => lines.push(Line::from(Span::styled("   – merged/closed — [R] refresh", dim))),
        }
        let mut sorted: Vec<&crate::providers::PrDetail> = state.prs.iter().collect();
        sorted.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        let slots = (pr_inner.height as usize).saturating_sub(lines.len() + 1);
        if slots > 0 && !sorted.is_empty() {
            lines.push(Line::from(""));
            for pr in sorted.iter().take(slots) {
                let title = pr.title.chars().take(title_w).collect::<String>();
                lines.push(Line::from(vec![
                    Span::styled(format!(" #{:<4} ", pr.number), cyn),
                    Span::styled(title, wht),
                ]));
            }
        }
        f.render_widget(Paragraph::new(lines), pr_inner);
    }

    // Issues — status breakdown, then recent titles
    let iss_yel = Style::default().fg(Color::Yellow);
    let iss_blk   = Block::default().borders(Borders::ALL)
        .title(Span::styled(" issues ", iss_yel))
        .title_alignment(Alignment::Center)
        .border_style(dim);
    let iss_inner = iss_blk.inner(row3[2]);
    f.render_widget(iss_blk, row3[2]);
    {
        let title_w = (iss_inner.width as usize).saturating_sub(9).max(6);
        let unanswered_c = state.issues.iter().filter(|i| i.comments == 0).count();
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" {:>3} ", state.issues.len()), if state.issues.is_empty() { dim } else { iss_yel }),
                Span::styled("open", dim),
            ]),
            Line::from(vec![
                Span::styled(format!(" {:>3} ", unanswered_c), if unanswered_c == 0 { dim } else { red }),
                Span::styled("unanswered", dim),
            ]),
        ];
        match state.closed_counts {
            Some((_, _, closed_iss)) => {
                lines.push(Line::from(vec![
                    Span::styled(format!(" {:>3} ", closed_iss), if closed_iss == 0 { dim } else { grn }),
                    Span::styled("closed", dim),
                ]));
            }
            None => lines.push(Line::from(Span::styled("   – closed — [R] refresh", dim))),
        }
        let mut sorted: Vec<&crate::providers::IssueDetail> = state.issues.iter().collect();
        sorted.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        let slots = (iss_inner.height as usize).saturating_sub(lines.len() + 1);
        if slots > 0 && !sorted.is_empty() {
            lines.push(Line::from(""));
            for iss in sorted.iter().take(slots) {
                let title = iss.title.chars().take(title_w).collect::<String>();
                lines.push(Line::from(vec![
                    Span::styled(format!(" #{:<4} ", iss.number), iss_yel),
                    Span::styled(title, wht),
                ]));
            }
        }
        f.render_widget(Paragraph::new(lines), iss_inner);
    }
}

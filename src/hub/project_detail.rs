use crate::config::AppConfig;
use crate::projects::Project;
use crate::status::RepoSummary;
use crate::todos::{Todo, TodoStatus};
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

// ── tabs ──────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum ProjectTab { Overview, Commits, Repos, Stats, Todos, Settings }

impl ProjectTab {
    fn next(self) -> Self {
        match self {
            Self::Overview  => Self::Commits,
            Self::Commits   => Self::Repos,
            Self::Repos     => Self::Stats,
            Self::Stats     => Self::Todos,
            Self::Todos     => Self::Settings,
            Self::Settings  => Self::Overview,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::Overview  => Self::Settings,
            Self::Commits   => Self::Overview,
            Self::Repos     => Self::Commits,
            Self::Stats     => Self::Repos,
            Self::Todos     => Self::Stats,
            Self::Settings  => Self::Todos,
        }
    }
}

// ── color palette ─────────────────────────────────────────────────────────────

const PROJ_PALETTE: &[(&str, &str)] = &[
    ("#808080", "Gray"), ("#3b82f6", "Blue"),   ("#22c55e", "Green"),
    ("#ef4444", "Red"),  ("#eab308", "Yellow"),  ("#a855f7", "Purple"),
    ("#f97316", "Orange"),("#ec4899", "Pink"),   ("#06b6d4", "Cyan"),
    ("#14b8a6", "Teal"), ("#f43f5e", "Rose"),    ("#84cc16", "Lime"),
];

// ── aggregated commit ─────────────────────────────────────────────────────────

struct AggCommit {
    repo:   String,
    sha:    String,
    ts:     i64,
    age:    String,
    author: String,
    msg:    String,
}

// ── state ─────────────────────────────────────────────────────────────────────

struct St {
    project:       Project,
    repos:         Vec<RepoSummary>,
    tab:           ProjectTab,
    commits:       Vec<AggCommit>,
    commits_rx:    Option<std::sync::mpsc::Receiver<Vec<AggCommit>>>,
    repo_cursor:   usize,
    commit_off:    usize,
    todo_cursor:   usize,
    stats_cursor:  usize,
    todos:         Vec<Todo>,
    activity:      Vec<(String, usize)>,
    activity_rx:   Option<std::sync::mpsc::Receiver<Vec<(String, usize)>>>,
    status_msg:    Option<String>,
    quit:          bool,
    cfg:           AppConfig,
    rename_input:  Option<String>,
    color_cursor:  Option<usize>,  // palette picker cursor (replaces hex text input)
}

impl St {
    fn new(project: Project, repos: Vec<RepoSummary>, cfg: AppConfig) -> Self {
        let pname = project.name.clone();
        let all_todos = crate::todos::load_manual();
        let todos: Vec<Todo> = all_todos.into_iter()
            .filter(|t| t.project.as_deref() == Some(&pname))
            .collect();
        Self {
            project, repos, cfg,
            tab:          ProjectTab::Overview,
            commits:      vec![],
            commits_rx:   None,
            repo_cursor:  0,
            commit_off:   0,
            todo_cursor:  0,
            stats_cursor: 0,
            todos,
            activity:     vec![],
            activity_rx:  None,
            status_msg:   None,
            quit:         false,
            rename_input: None,
            color_cursor: None,
        }
    }
}

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run(project: Project, repos: Vec<RepoSummary>, cfg: &AppConfig, maybe_term: Option<crate::tui::Tty>) -> Result<()> {
    let mut terminal = match maybe_term {
        Some(t) => t,
        None    => crate::tui::setup()?,
    };
    let mut st = St::new(project, repos, cfg.clone());

    // Spawn commits aggregation immediately
    spawn_commits(&mut st);

    let result = run_loop(&mut st, &mut terminal).await;
    crate::tui::teardown(terminal)?;
    result
}

async fn run_loop(st: &mut St, terminal: &mut crate::tui::Tty) -> Result<()> {
    loop {
        poll_commits(st);
        poll_activity(st);

        terminal.draw(|f| draw(f, st))?;

        let event = crate::tui::next_event(250)?;
        let key = match event { Some(Event::Key(k)) => k, _ => continue };

        // Modal inputs (rename / color-palette)
        if let Some(ref mut input) = st.rename_input {
            match key.code {
                KeyCode::Esc => { st.rename_input = None; }
                KeyCode::Enter => {
                    let name = input.trim().to_string();
                    if !name.is_empty() {
                        let mut projects = crate::projects::load();
                        if let Some(p) = projects.iter_mut().find(|p| p.name == st.project.name) {
                            p.name = name.clone();
                        }
                        let _ = crate::projects::save(&projects);
                        st.project.name = name;
                        st.status_msg = Some("Renamed project".into());
                    }
                    st.rename_input = None;
                }
                KeyCode::Backspace => { input.pop(); }
                KeyCode::Char(c) if input.len() < 80 => { input.push(c); }
                _ => {}
            }
            continue;
        }
        if let Some(ref mut cur) = st.color_cursor {
            let max = PROJ_PALETTE.len().saturating_sub(1);
            match key.code {
                KeyCode::Esc => { st.color_cursor = None; }
                KeyCode::Left  | KeyCode::Char('h') => { *cur = cur.saturating_sub(1); }
                KeyCode::Right | KeyCode::Char('l') => { *cur = (*cur + 1).min(max); }
                KeyCode::Up    | KeyCode::Char('k') => { *cur = cur.saturating_sub(6); }
                KeyCode::Down  | KeyCode::Char('j') => { *cur = (*cur + 6).min(max); }
                KeyCode::Enter => {
                    let (hex, _) = PROJ_PALETTE[*cur];
                    let mut projects = crate::projects::load();
                    if let Some(p) = projects.iter_mut().find(|p| p.name == st.project.name) {
                        p.color = hex.to_string();
                    }
                    let _ = crate::projects::save(&projects);
                    st.project.color = hex.to_string();
                    st.color_cursor = None;
                    st.status_msg = Some("Color updated".into());
                }
                _ => {}
            }
            continue;
        }

        handle_key(key.code, st).await;
        if st.quit { break; }
    }
    Ok(())
}

async fn handle_key(code: KeyCode, st: &mut St) {
    st.status_msg = None;
    match code {
        KeyCode::Char('q') | KeyCode::Esc => { st.quit = true; }

        KeyCode::Tab | KeyCode::Right => {
            st.tab = st.tab.next();
            if st.tab == ProjectTab::Stats && st.activity.is_empty() && st.activity_rx.is_none() {
                spawn_activity(st);
            }
        }
        KeyCode::Left => {
            st.tab = st.tab.prev();
            if st.tab == ProjectTab::Stats && st.activity.is_empty() && st.activity_rx.is_none() {
                spawn_activity(st);
            }
        }
        KeyCode::Char('1') => { st.tab = ProjectTab::Overview; }
        KeyCode::Char('2') => { st.tab = ProjectTab::Commits; }
        KeyCode::Char('3') => { st.tab = ProjectTab::Repos; }
        KeyCode::Char('4') => {
            st.tab = ProjectTab::Stats;
            if st.activity.is_empty() && st.activity_rx.is_none() { spawn_activity(st); }
        }
        KeyCode::Char('5') => { st.tab = ProjectTab::Todos; }
        KeyCode::Char('6') => { st.tab = ProjectTab::Settings; }

        KeyCode::Down | KeyCode::Char('j') => match st.tab {
            ProjectTab::Repos    => st.repo_cursor  = (st.repo_cursor + 1).min(st.repos.len().saturating_sub(1)),
            ProjectTab::Commits  => { let max = st.commits.len().saturating_sub(1); st.commit_off = (st.commit_off + 1).min(max); }
            ProjectTab::Todos    => st.todo_cursor  = (st.todo_cursor + 1).min(st.todos.len().saturating_sub(1)),
            ProjectTab::Stats    => st.stats_cursor = (st.stats_cursor + 1).min(st.repos.len().saturating_sub(1)),
            _ => {}
        },
        KeyCode::Up | KeyCode::Char('k') => match st.tab {
            ProjectTab::Repos    => st.repo_cursor  = st.repo_cursor.saturating_sub(1),
            ProjectTab::Commits  => st.commit_off   = st.commit_off.saturating_sub(1),
            ProjectTab::Todos    => st.todo_cursor  = st.todo_cursor.saturating_sub(1),
            ProjectTab::Stats    => st.stats_cursor = st.stats_cursor.saturating_sub(1),
            _ => {}
        },

        KeyCode::Enter => match st.tab {
            ProjectTab::Repos => {
                if let Some(r) = st.repos.get(st.repo_cursor).cloned() {
                    let _ = crate::detail::run(r, &st.cfg, None).await;
                }
            }
            ProjectTab::Todos => {
                if let Some(t) = st.todos.get_mut(st.todo_cursor) {
                    t.status = match t.status {
                        TodoStatus::Open       => TodoStatus::InProgress,
                        TodoStatus::InProgress => TodoStatus::Done,
                        TodoStatus::Done       => TodoStatus::Cancelled,
                        TodoStatus::Cancelled  => TodoStatus::Open,
                    };
                    t.touch();
                    let all = crate::todos::load_manual();
                    let updated = t.clone();
                    let new_all: Vec<_> = all.into_iter().map(|x| if x.id == updated.id { updated.clone() } else { x }).collect();
                    let _ = crate::todos::save(&new_all);
                }
            }
            ProjectTab::Settings => {
                if st.rename_input.is_none() && st.color_cursor.is_none() {
                    st.rename_input = Some(st.project.name.clone());
                }
            }
            _ => {}
        },

        KeyCode::Char('c') if st.tab == ProjectTab::Settings => {
            let cur = PROJ_PALETTE.iter().position(|(h, _)| *h == st.project.color.as_str()).unwrap_or(0);
            st.color_cursor = Some(cur);
        }

        _ => {}
    }
}

// ── background loaders ────────────────────────────────────────────────────────

fn spawn_commits(st: &mut St) {
    if st.commits_rx.is_some() { return; }
    let paths: Vec<(String, std::path::PathBuf)> = st.repos.iter()
        .map(|r| (r.name.clone(), r.path.clone()))
        .collect();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut all: Vec<AggCommit> = vec![];
        for (name, path) in &paths {
            let hist = crate::detail::build_history_pub(path, 100);
            for h in hist {
                if let crate::detail::HistEntry::Commit { sha, ts, age, author, msg } = h {
                    all.push(AggCommit { repo: name.clone(), sha, ts, age, author, msg });
                }
            }
        }
        all.sort_by(|a, b| b.ts.cmp(&a.ts));
        all.truncate(500);
        let _ = tx.send(all);
    });

    st.commits_rx = Some(rx);
}

fn spawn_activity(st: &mut St) {
    if st.activity_rx.is_some() { return; }
    let paths: Vec<std::path::PathBuf> = st.repos.iter().map(|r| r.path.clone()).collect();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let mut date_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for path in &paths {
            for (date, count) in crate::detail::activity_1y(path) {
                *date_map.entry(date).or_default() += count;
            }
        }
        let mut activity: Vec<(String, usize)> = date_map.into_iter().collect();
        activity.sort_by(|a, b| a.0.cmp(&b.0));
        let _ = tx.send(activity);
    });

    st.activity_rx = Some(rx);
}

fn poll_commits(st: &mut St) {
    if let Some(rx) = &st.commits_rx {
        match rx.try_recv() {
            Ok(c) => { st.commits = c; st.commits_rx = None; }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => { st.commits_rx = None; }
            _ => {}
        }
    }
}

fn poll_activity(st: &mut St) {
    if let Some(rx) = &st.activity_rx {
        match rx.try_recv() {
            Ok(a) => { st.activity = a; st.activity_rx = None; }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => { st.activity_rx = None; }
            _ => {}
        }
    }
}

// ── rendering ─────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, st: &St) {
    let area = f.area();
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(2)])
        .split(area);

    render_topbar(f, st, vert[0]);

    let blk   = Block::default().borders(Borders::ALL);
    let inner = blk.inner(vert[1]);
    f.render_widget(blk, vert[1]);

    match st.tab {
        ProjectTab::Overview  => render_overview(f, st, inner),
        ProjectTab::Commits   => render_commits(f, st, inner),
        ProjectTab::Repos     => render_repos(f, st, inner),
        ProjectTab::Stats     => render_stats(f, st, inner),
        ProjectTab::Todos     => render_todos(f, st, inner),
        ProjectTab::Settings  => render_settings(f, st, inner),
    }

    render_bottombar(f, st, vert[2]);

    // Modal overlays
    if let Some(ref input) = st.rename_input {
        draw_text_modal(f, area, "Rename Project", input, "Enter new name | Esc cancel");
    }
    if let Some(cursor) = st.color_cursor {
        draw_color_pick_modal(f, area, cursor, &st.project.color);
    }
}

fn render_topbar(f: &mut Frame, st: &St, area: Rect) {
    let (pr, pg, pb) = crate::projects::hex_to_rgb(&st.project.color);
    let proj_col = Color::Rgb(pr, pg, pb);
    let dim = Style::default().fg(Color::DarkGray);

    let tab_s = |label: &str, on: bool| -> Span<'static> {
        let label = label.to_string();
        if on { Span::styled(format!(" {} ", label), Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)) }
        else   { Span::styled(format!(" {} ", label), dim) }
    };

    let dirty_c  = st.repos.iter().filter(|r| r.is_dirty).count();
    let behind_c = st.repos.iter().filter(|r| r.behind > 0).count();
    let open_t   = st.todos.iter().filter(|t| matches!(t.status, TodoStatus::Open)).count();
    let (health_icon, health_col) = if behind_c > 0 { ("↓", Color::Red) }
        else if dirty_c > 0 { ("⚠", Color::Yellow) }
        else { ("✓", Color::Green) };

    let mut spans = vec![
        Span::styled(" ◀ ", dim),
        Span::styled("■ ", Style::default().fg(proj_col)),
        Span::styled(st.project.name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" · {} repo{}  ", st.repos.len(), if st.repos.len() == 1 { "" } else { "s" }),
            dim,
        ),
        tab_s("1 Overview",  st.tab == ProjectTab::Overview),
        Span::raw(" "),
        tab_s("2 Commits",   st.tab == ProjectTab::Commits),
        Span::raw(" "),
        tab_s("3 Repos",     st.tab == ProjectTab::Repos),
        Span::raw(" "),
        tab_s("4 Stats",     st.tab == ProjectTab::Stats),
        Span::raw(" "),
        tab_s("5 Todos",     st.tab == ProjectTab::Todos),
        Span::raw(" "),
        tab_s("6 Settings",  st.tab == ProjectTab::Settings),
        Span::styled(format!("   {} ", health_icon), Style::default().fg(health_col)),
    ];
    if open_t > 0 {
        spans.push(Span::styled(
            format!("{} todo{}", open_t, if open_t == 1 { "" } else { "s" }),
            Style::default().fg(Color::Cyan),
        ));
    }
    if let Some(ref msg) = st.status_msg {
        spans.push(Span::styled(format!("   {}", msg), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_bottombar(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let key = |k: &str| Span::styled(format!("[{}] ", k), dim);
    let hint = match st.tab {
        ProjectTab::Repos    => Line::from(vec![key("↑↓"), Span::styled("select  ", dim), key("Enter"), Span::styled("open repo detail", dim)]),
        ProjectTab::Todos    => Line::from(vec![key("↑↓"), Span::styled("select  ", dim), key("Enter"), Span::styled("cycle status", dim)]),
        ProjectTab::Settings => Line::from(vec![key("Enter"), Span::styled("rename  ", dim), key("c"), Span::styled("change color", dim)]),
        _                    => Line::from(vec![key("↑↓"), Span::styled("scroll", dim)]),
    };
    f.render_widget(Paragraph::new(vec![
        Line::from(vec![key("←→/Tab"), Span::styled("tabs  ", dim), key("1-6"), Span::styled("direct  ", dim), key("q/Esc"), Span::styled("back", dim)]),
        hint,
    ]), area);
}

// ── tab renderers ─────────────────────────────────────────────────────────────

fn render_overview(f: &mut Frame, st: &St, area: Rect) {
    let dim   = Style::default().fg(Color::DarkGray);
    let yel   = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let wht   = Style::default().fg(Color::White);
    let (pr, pg, pb) = crate::projects::hex_to_rgb(&st.project.color);
    let proj_col = Color::Rgb(pr, pg, pb);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // LEFT: project info + health bar + repo list
    {
        let dirty_c  = st.repos.iter().filter(|r| r.is_dirty).count();
        let behind_c = st.repos.iter().filter(|r| r.behind > 0).count();
        let ahead_c  = st.repos.iter().filter(|r| r.ahead > 0).count();
        let total    = st.repos.len();

        let clean_n  = if total > 0 { (20 * total.saturating_sub(dirty_c + behind_c)) / total } else { 20 };
        let dirty_n  = if total > 0 { (20 * dirty_c)  / total } else { 0 };
        let behind_n = 20usize.saturating_sub(clean_n + dirty_n);
        let health_bar = format!("{}{}{}", "█".repeat(clean_n), "▓".repeat(dirty_n), "░".repeat(behind_n));
        let health_col = if behind_c > 0 { Color::Red } else if dirty_c > 0 { Color::Yellow } else { Color::Green };

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("  ■ ", Style::default().fg(proj_col)),
                Span::styled(st.project.name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  {} repo{}", total, if total == 1 { "" } else { "s" }),
                    dim,
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Health  ", dim),
                Span::styled(health_bar, Style::default().fg(health_col)),
                Span::styled(format!("  ↓{}  ⚠{}  ↑{}", behind_c, dirty_c, ahead_c), dim),
            ]),
            Line::from(""),
            Line::from(Span::styled("  Repos", yel)),
            Line::from(Span::styled("  ─────────────────────────────────────────", dim)),
        ];

        let avail = area.height.saturating_sub(6) as usize;
        for r in st.repos.iter().take(avail) {
            let status = if r.is_dirty { "⚠" } else if r.ahead > 0 { "↑" } else if r.behind > 0 { "↓" } else { "✓" };
            let col = if r.is_dirty { Color::Yellow } else if r.behind > 0 { Color::Red } else { Color::Green };
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", status), Style::default().fg(col)),
                Span::styled(format!("{:<18} ", r.name.chars().take(18).collect::<String>()), wht),
                Span::styled(format!("{:<9} ", r.branch.chars().take(9).collect::<String>()), dim),
                Span::styled(crate::status::format_age_pub(r.last_commit_ts), Style::default().fg(super::tabs::age_color(r.last_commit_ts))),
            ]));
        }
        f.render_widget(Paragraph::new(lines), cols[0]);
    }

    // RIGHT: open todos + recent commits
    {
        let open_t: Vec<&Todo> = st.todos.iter()
            .filter(|t| matches!(t.status, TodoStatus::Open | TodoStatus::InProgress))
            .collect();

        let mut lines: Vec<Line> = vec![];

        lines.push(Line::from(Span::styled("  Todos", yel)));
        lines.push(Line::from(Span::styled("  ─────────────────────────────────", dim)));
        if open_t.is_empty() {
            lines.push(Line::from(Span::styled("  No open todos", dim)));
        } else {
            let half_h = (area.height as usize / 2).saturating_sub(2);
            for t in open_t.iter().take(half_h) {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {} ", t.status.icon()), Style::default().fg(t.status.color())),
                    Span::styled(t.title.chars().take(28).collect::<String>(), wht),
                ]));
            }
        }
        lines.push(Line::from(""));

        if !st.commits.is_empty() {
            lines.push(Line::from(Span::styled("  Recent Commits", yel)));
            lines.push(Line::from(Span::styled("  ─────────────────────────────────", dim)));
            let rem_h = (area.height as usize).saturating_sub(lines.len() + 1);
            for c in st.commits.iter().take(rem_h) {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {:>5}  ", c.age.chars().take(5).collect::<String>()), dim),
                    Span::styled(format!("{:<10} ", c.repo.chars().take(10).collect::<String>()), Style::default().fg(Color::Cyan)),
                    Span::styled(c.msg.chars().take(22).collect::<String>(), dim),
                ]));
            }
        } else if st.commits_rx.is_some() {
            lines.push(Line::from(Span::styled("  Loading commits...", dim)));
        }

        f.render_widget(Paragraph::new(lines), cols[1]);
    }
}

fn render_commits(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    if st.commits_rx.is_some() {
        f.render_widget(Paragraph::new("  Loading commits from all repos...").style(dim), area);
        return;
    }
    if st.commits.is_empty() {
        f.render_widget(Paragraph::new("  No commits found").style(dim), area);
        return;
    }

    let header = Row::new(vec!["REPO", "AGE", "AUTHOR", "MESSAGE"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let avail = area.height.saturating_sub(1) as usize;
    let rows: Vec<Row> = st.commits.iter().skip(st.commit_off).take(avail)
        .map(|c| {
            Row::new(vec![
                Cell::from(c.repo.chars().take(14).collect::<String>()).style(yel),
                Cell::from(c.age.chars().take(8).collect::<String>()).style(dim),
                Cell::from(c.author.chars().take(12).collect::<String>()).style(dim),
                Cell::from(c.msg.chars().take(60).collect::<String>()),
            ])
        })
        .collect();

    f.render_widget(
        Table::new(rows, [Constraint::Length(14), Constraint::Length(8), Constraint::Length(12), Constraint::Fill(1)]).header(header),
        area,
    );
}

fn render_repos(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let header = Row::new(vec!["NAME", "BRANCH", "STATUS", "AGE", "LAST COMMIT"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let avail = area.height.saturating_sub(1) as usize;
    let rows: Vec<Row> = st.repos.iter().enumerate().skip(st.repo_cursor.saturating_sub(avail / 2)).take(avail)
        .map(|(i, r)| {
            let is_cur = i == st.repo_cursor;
            let status = if r.is_dirty { format!("⚠ +{}-{}", r.ahead, r.behind) } else { format!("+{}-{}", r.ahead, r.behind) };
            let col = if r.is_dirty || r.behind > 0 { Color::Yellow } else { Color::White };
            Row::new(vec![
                Cell::from(r.name.chars().take(20).collect::<String>()).style(Style::default().fg(col)),
                Cell::from(r.branch.chars().take(12).collect::<String>()).style(dim),
                Cell::from(status).style(dim),
                Cell::from(crate::status::format_age_pub(r.last_commit_ts)).style(Style::default().fg(super::tabs::age_color(r.last_commit_ts))),
                Cell::from(r.last_commit_msg.chars().take(35).collect::<String>()).style(dim),
            ]).style(if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() })
        })
        .collect();

    f.render_widget(
        Table::new(rows, [Constraint::Length(20), Constraint::Length(12), Constraint::Length(12), Constraint::Length(10), Constraint::Fill(1)]).header(header),
        area,
    );
}

fn render_stats(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let cyn = Style::default().fg(Color::Cyan);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = vec![];
    lines.push(Line::from(Span::styled("  Activity (last 1y, all repos combined)", yel)));
    lines.push(Line::from(Span::styled("  ──────────────────────────────────────────────────────", dim)));

    if st.activity_rx.is_some() {
        lines.push(Line::from(Span::styled("  Computing activity...", cyn)));
        f.render_widget(Paragraph::new(lines), area);
        return;
    }
    if st.activity.is_empty() {
        lines.push(Line::from(Span::styled("  No activity data", dim)));
        f.render_widget(Paragraph::new(lines), area);
        return;
    }

    let max_count: usize = st.activity.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);
    let bar_h = (area.height as usize).saturating_sub(6).max(3);
    let bar_w = (area.width as usize).saturating_sub(10).max(1);
    let n = st.activity.len();
    let step = if n > bar_w { (n / bar_w).max(1) } else { 1 };
    let sampled: Vec<&(String, usize)> = st.activity.iter().step_by(step).collect();
    let bar_count = sampled.len().min(bar_w);
    let bar_width = if bar_count == 0 { 1 } else { (bar_w / bar_count).max(1) };

    for row in (0..bar_h).rev() {
        let threshold = (row * max_count) / bar_h;
        let mut spans: Vec<Span<'static>> = vec![];
        let y_label = if row == bar_h - 1 { format!("  {:>4} ", max_count) } else if row == 0 { "     0 ".to_string() } else { "       ".to_string() };
        spans.push(Span::styled(y_label, dim));
        for j in 0..bar_count {
            let count = sampled.get(j).map(|(_, c)| *c).unwrap_or(0);
            let filled = count > threshold;
            for _ in 0..bar_width {
                if filled { spans.push(Span::styled("█", cyn)); }
                else      { spans.push(Span::styled("░", dim)); }
            }
        }
        lines.push(Line::from(spans));
    }
    // X axis labels
    let nth = (bar_count / 6).max(1);
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
    lines.push(Line::from(x_spans));

    f.render_widget(Paragraph::new(lines), area);
}

fn render_todos(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let header = Row::new(vec!["STATUS", "TITLE", "EST", "CREATED"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    if st.todos.is_empty() {
        f.render_widget(Paragraph::new("  No todos for this project").style(dim), area);
        return;
    }

    let avail = area.height.saturating_sub(1) as usize;
    let rows: Vec<Row> = st.todos.iter().enumerate()
        .skip(st.todo_cursor.saturating_sub(avail / 2)).take(avail)
        .map(|(i, t)| {
            let is_cur = i == st.todo_cursor;
            let status_str = format!("{} {}", t.status.icon(), t.status.label());
            let est_str = t.estimate.as_deref().unwrap_or("—").chars().take(10).collect::<String>();
            let created = crate::status::format_age_pub(t.created_at);
            Row::new(vec![
                Cell::from(status_str).style(Style::default().fg(t.status.color())),
                Cell::from(t.title.chars().take(55).collect::<String>()),
                Cell::from(est_str).style(dim),
                Cell::from(created).style(dim),
            ]).style(if is_cur { Style::default().bg(Color::DarkGray) } else { Style::default() })
        })
        .collect();

    f.render_widget(
        Table::new(rows, [Constraint::Length(14), Constraint::Fill(1), Constraint::Length(10), Constraint::Length(10)]).header(header),
        area,
    );
}

fn render_settings(f: &mut Frame, st: &St, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let yel = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let cyn = Style::default().fg(Color::Cyan);
    let (pr, pg, pb) = crate::projects::hex_to_rgb(&st.project.color);
    let proj_col = Color::Rgb(pr, pg, pb);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled("  Project Settings", yel)),
        Line::from(Span::styled("  ────────────────────────────────────────────", dim)),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Name:  ", dim),
            Span::styled(st.project.name.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Color: ", dim),
            Span::styled(" ■ ", Style::default().fg(proj_col).add_modifier(Modifier::BOLD)),
            Span::styled(st.project.color.clone(), Style::default().fg(proj_col)),
        ]),
        Line::from(vec![
            Span::styled("  Repos: ", dim),
            Span::styled(st.repos.len().to_string(), Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Actions", yel)),
        Line::from(Span::styled("  ────────────────────────────────────────────", dim)),
        Line::from(vec![
            Span::styled("  [Enter]  ", cyn),
            Span::styled("Rename project", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  [c]      ", cyn),
            Span::styled("Change color (palette picker)", Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Color Palette", yel)),
        Line::from(Span::styled("  ────────────────────────────────────────────", dim)),
    ];

    // Show palette swatches
    let per_row = 4;
    for chunk_start in (0..PROJ_PALETTE.len()).step_by(per_row) {
        let mut spans: Vec<Span> = vec![Span::raw("  ")];
        for i in 0..per_row {
            if let Some((hex, name)) = PROJ_PALETTE.get(chunk_start + i) {
                let (cr, cg, cb) = crate::projects::hex_to_rgb(hex);
                let c = Color::Rgb(cr, cg, cb);
                let active = *hex == st.project.color.as_str();
                spans.push(Span::styled(if active { "▶██◀ " } else { " ██  " }, Style::default().fg(c)));
                spans.push(Span::styled(format!("{:<8}", name.chars().take(6).collect::<String>()), if active { Style::default().fg(c).add_modifier(Modifier::BOLD) } else { dim }));
            }
        }
        lines.push(Line::from(spans));
    }

    if st.rename_input.is_some() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  → Rename modal open — type name, Enter confirm, Esc cancel", cyn)));
    }
    if st.color_cursor.is_some() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  → Color picker open — ←→↑↓ navigate, Enter confirm, Esc cancel", cyn)));
    }

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_text_modal(f: &mut Frame, area: Rect, title: &str, input: &str, hint: &str) {
    use ratatui::widgets::Clear;
    let modal_area = crate::tui::centered_rect(50, 20, area);
    f.render_widget(Clear, modal_area);
    let blk = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = blk.inner(modal_area);
    f.render_widget(blk, modal_area);
    f.render_widget(Paragraph::new(vec![
        Line::from(Span::styled(format!("  {}_", input), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(format!("  {}", hint), Style::default().fg(Color::DarkGray))),
    ]), inner);
}

fn draw_color_pick_modal(f: &mut Frame, area: Rect, cursor: usize, current_hex: &str) {
    use ratatui::widgets::Clear;
    let modal_area = crate::tui::centered_rect(70, 50, area);
    f.render_widget(Clear, modal_area);
    let blk = Block::default()
        .borders(Borders::ALL)
        .title(" Choose Color  ←→↑↓ navigate · Enter confirm · Esc cancel ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = blk.inner(modal_area);
    f.render_widget(blk, modal_area);

    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line> = vec![Line::from("")];

    let per_row = 6usize;
    for row_start in (0..PROJ_PALETTE.len()).step_by(per_row) {
        // Swatch row
        let mut swatch_spans: Vec<Span> = vec![Span::raw("  ")];
        for i in 0..per_row {
            if let Some((hex, _)) = PROJ_PALETTE.get(row_start + i) {
                let (cr, cg, cb) = crate::projects::hex_to_rgb(hex);
                let c = Color::Rgb(cr, cg, cb);
                let is_cur = row_start + i == cursor;
                let is_active = *hex == current_hex;
                let label = if is_cur { "▶████◀ " } else if is_active { " ████* " } else { " ████  " };
                swatch_spans.push(Span::styled(label, Style::default().fg(c).add_modifier(if is_cur { Modifier::BOLD } else { Modifier::empty() })));
            }
        }
        lines.push(Line::from(swatch_spans));

        // Name row
        let mut name_spans: Vec<Span> = vec![Span::raw("  ")];
        for i in 0..per_row {
            if let Some((hex, name)) = PROJ_PALETTE.get(row_start + i) {
                let (cr, cg, cb) = crate::projects::hex_to_rgb(hex);
                let c = Color::Rgb(cr, cg, cb);
                let is_cur = row_start + i == cursor;
                name_spans.push(Span::styled(
                    format!("{:<9}", name.chars().take(7).collect::<String>()),
                    if is_cur { Style::default().fg(c).add_modifier(Modifier::BOLD) } else { dim },
                ));
            }
        }
        lines.push(Line::from(name_spans));
        lines.push(Line::from(""));
    }

    // Show selected hex
    if let Some((hex, name)) = PROJ_PALETTE.get(cursor) {
        let (cr, cg, cb) = crate::projects::hex_to_rgb(hex);
        lines.push(Line::from(vec![
            Span::raw("  Selected: "),
            Span::styled(format!("■ {} — {}", hex, name), Style::default().fg(Color::Rgb(cr, cg, cb)).add_modifier(Modifier::BOLD)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

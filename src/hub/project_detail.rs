//! The project detail sub-view: a per-project dashboard aggregating commits,
//! repos, activity stats and todos across every repo assigned to one
//! project. Rendering lives in the sibling `project_detail_render` module;
//! this file owns the state, event loop, and background data loaders.

use crate::config::AppConfig;
use crate::projects::Project;
use crate::status::RepoSummary;
use crate::todos::{Todo, TodoStatus};
use anyhow::Result;
use crossterm::event::{Event, KeyCode};

/// Which tab of the project detail view is showing.
#[derive(PartialEq, Clone, Copy)]
pub enum ProjectTab { Overview, Commits, Repos, Stats, Todos, Settings }

impl ProjectTab {
    pub fn next(self) -> Self {
        match self {
            Self::Overview  => Self::Commits,
            Self::Commits   => Self::Repos,
            Self::Repos     => Self::Stats,
            Self::Stats     => Self::Todos,
            Self::Todos     => Self::Settings,
            Self::Settings  => Self::Overview,
        }
    }
    pub fn prev(self) -> Self {
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

/// Fixed color choices offered by the rename/recolor settings picker —
/// matches the palette used for project markers throughout the hub.
pub const PROJ_PALETTE: &[(&str, &str)] = &[
    ("#808080", "Gray"), ("#3b82f6", "Blue"),   ("#22c55e", "Green"),
    ("#ef4444", "Red"),  ("#eab308", "Yellow"),  ("#a855f7", "Purple"),
    ("#f97316", "Orange"),("#ec4899", "Pink"),   ("#06b6d4", "Cyan"),
    ("#14b8a6", "Teal"), ("#f43f5e", "Rose"),    ("#84cc16", "Lime"),
];

/// One commit, tagged with which repo it came from — used to merge commit
/// history across every repo in the project into a single chronological feed.
pub struct AggCommit {
    pub repo:   String,
    #[allow(dead_code)] pub sha: String, // kept for future per-commit actions
    pub ts:     i64,
    pub age:    String,
    pub author: String,
    pub msg:    String,
}

/// State for one project-detail session: the project being viewed, its
/// repos, and whatever background data (aggregated commits, activity) has
/// loaded so far.
pub struct St {
    pub project:       Project,
    pub repos:         Vec<RepoSummary>,
    pub tab:           ProjectTab,
    pub commits:       Vec<AggCommit>,
    pub commits_rx:    Option<std::sync::mpsc::Receiver<Vec<AggCommit>>>,
    pub repo_cursor:   usize,
    pub commit_off:    usize,
    pub todo_cursor:   usize,
    pub stats_cursor:  usize,
    pub todos:         Vec<Todo>,
    pub activity:      Vec<(String, usize)>,
    pub activity_rx:   Option<std::sync::mpsc::Receiver<Vec<(String, usize)>>>,
    pub status_msg:    Option<String>,
    pub quit:          bool,
    pub cfg:           AppConfig,
    pub rename_input:  Option<String>,
    pub color_cursor:  Option<usize>,  // palette picker cursor (replaces hex text input)
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

/// Runs the project detail sub-view until the user backs out. Reuses the
/// caller's terminal handle when given one (avoids an alternate-screen
/// flash when entering from the hub), otherwise sets up its own.
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

        terminal.draw(|f| super::project_detail_render::draw(f, st))?;

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

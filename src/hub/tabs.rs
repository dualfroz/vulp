//! Dispatch for the hub's per-tab rendering: routes to either a full-screen
//! dashboard tab (Overview/Stats/Activity/HubSettings, in the sibling
//! `tabs_*` files) or a scrollable table (Repos/Projects/Todos/PRs/Issues,
//! in `tabs_tables`).

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    Frame,
};
use super::state::{HubState, Tab};
use super::tabs_overview::render_overview_global;
use super::tabs_settings::render_settings_global;
use super::tabs_stats::{render_activity_global, render_stats_global};
use super::tabs_tables::{
    render_issues_table, render_prs_table, render_projects_table, render_repos_table, render_todos_table,
};

/// Color-codes a timestamp's age for "freshness at a glance" — green/yellow/
/// red roughly matching how stale a repo's last commit looks in practice.
pub fn age_color(ts: i64) -> Color {
    let days = (chrono::Utc::now().timestamp() - ts) / 86400;
    if days < 7 { Color::Green } else if days < 30 { Color::Yellow } else { Color::Red }
}

pub fn render_list(f: &mut Frame, state: &HubState, area: Rect) {
    // New full-screen tabs don't use the list
    match state.tab {
        Tab::Overview    => { render_overview_global(f, state, area); return; }
        Tab::Stats       => { render_stats_global(f, state, area);    return; }
        Tab::Activity    => { render_activity_global(f, state, area); return; }
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

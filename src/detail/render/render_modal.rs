//! Modal overlay rendering for the detail view: dispatches on `DetailModal`
//! and draws each variant centered over the full view area, on top of
//! whatever tab is currently rendered underneath.

use super::super::{DetailModal, St};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Draws the currently active modal, if any, centered over `area`.
pub fn render_modal(f: &mut Frame, st: &St, area: Rect) {
    match &st.modal {
        DetailModal::None => {}

        DetailModal::NewBranch { input, error } => {
            draw_text_modal_with_error(f, area, " New Branch ", input, "branch name", error.as_deref(), Color::Cyan);
        }

        DetailModal::EditText { title, prompt, input, .. } => {
            let prompt_owned = prompt.clone();
            draw_text_modal(f, area, &format!(" {} ", title), input, &prompt_owned, Color::Cyan);
        }

        DetailModal::SelectOption { title, options, cursor, .. } => {
            let h = (options.len() as u16 + 4).min(area.height.saturating_sub(4));
            let w = 44u16.min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(format!(" {} ", title))
                .border_style(Style::default().fg(Color::Cyan));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let mut lines: Vec<Line> = vec![Line::from("")];
            for (i, opt) in options.iter().enumerate() {
                let sel = i == *cursor;
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} {}", if sel { "▶" } else { " " }, opt),
                        Style::default()
                            .fg(if sel { Color::Cyan } else { Color::White })
                            .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [↑↓] select  [Enter] confirm  [Esc] cancel", Style::default().fg(Color::DarkGray))));
            f.render_widget(Paragraph::new(lines), inner);
        }

        DetailModal::SelectRole { collab_name, cursor } => {
            const ROLES: &[&str] = &["pull", "push", "maintain", "admin"];
            let h = (ROLES.len() as u16 + 5).min(area.height.saturating_sub(4));
            let w = 44u16.min(area.width.saturating_sub(4));
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(format!(" Role for @{} ", collab_name))
                .border_style(Style::default().fg(Color::Cyan));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let mut lines: Vec<Line> = vec![Line::from("")];
            for (i, role) in ROLES.iter().enumerate() {
                let sel = i == *cursor;
                lines.push(Line::from(Span::styled(
                    format!("  {} {}", if sel { "▶" } else { " " }, role),
                    Style::default().fg(if sel { Color::Cyan } else { Color::White })
                        .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("  [↑↓] select  [Enter] invite  [Esc] cancel", Style::default().fg(Color::DarkGray))));
            f.render_widget(Paragraph::new(lines), inner);
        }

        DetailModal::ConfirmAction { message, .. } => {
            let w = 52u16.min(area.width.saturating_sub(4));
            let h = 7u16;
            let x = area.x + (area.width.saturating_sub(w)) / 2;
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            let modal_area = Rect::new(x, y, w, h);
            f.render_widget(Clear, modal_area);
            let blk = Block::default().borders(Borders::ALL)
                .title(" Confirm ")
                .border_style(Style::default().fg(Color::Red));
            let inner = blk.inner(modal_area);
            f.render_widget(blk, modal_area);

            let msg_clone = message.clone();
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(""),
                    Line::from(Span::styled(format!("  {}", msg_clone), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                    Line::from(""),
                    Line::from(Span::styled("  [y / Enter] confirm", Style::default().fg(Color::Red))),
                    Line::from(Span::styled("  [n / Esc]   cancel",  Style::default().fg(Color::DarkGray))),
                ]),
                inner,
            );
        }
    }
}

/// Draws a single-line text-input modal with no error message.
fn draw_text_modal(f: &mut Frame, area: Rect, title: &str, input: &str, hint: &str, border_col: Color) {
    draw_text_modal_with_error(f, area, title, input, hint, None, border_col);
}

/// Draws a single-line text-input modal, growing by one line and switching
/// to a red border when `error` is `Some` (used for the New Branch modal's
/// inline validation message).
fn draw_text_modal_with_error(f: &mut Frame, area: Rect, title: &str, input: &str, hint: &str, error: Option<&str>, border_col: Color) {
    let w = 60u16.min(area.width.saturating_sub(4));
    let h = if error.is_some() { 7u16 } else { 6u16 };
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let modal_area = Rect::new(x, y, w, h);
    f.render_widget(Clear, modal_area);

    let border_col = if error.is_some() { Color::Red } else { border_col };
    let blk = Block::default().borders(Borders::ALL)
        .title(title.to_string())
        .border_style(Style::default().fg(border_col));
    let inner = blk.inner(modal_area);
    f.render_widget(blk, modal_area);

    let mut lines = vec![
        Line::from(Span::styled(format!("  {}", hint), Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(
            format!("  > {}█", input.chars().take((w as usize).saturating_sub(6)).collect::<String>()),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("  [Enter] confirm  [Esc] cancel", Style::default().fg(Color::DarkGray))),
    ];
    if let Some(err) = error {
        lines.insert(2, Line::from(Span::styled(format!("  ⚠ {}", err), Style::default().fg(Color::Red))));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

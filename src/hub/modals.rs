//! Hub modal system: rendering (`render_modal`), per-keystroke editing
//! (`handle_modal_key`), and committing a modal once the user confirms
//! (`process_modal_confirm`). The actual confirm logic for each feature area
//! lives in the sibling `modals_confirm_*` files, chained together below;
//! background-thread launchers live in `modals_spawn`.

use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use super::state::{HubState, Modal, ModalAction};

pub use super::modals_spawn::open_bulk_import_picker;

/// Expands a leading `~` or `~/...` to the user's home directory; returns
/// `s` unchanged (as a `PathBuf`) otherwise. Used by every modal that takes
/// a free-form filesystem path, since users naturally type `~/...` paths.
pub fn expand_tilde(s: &str) -> std::path::PathBuf {
    let s = s.trim();
    if s == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(s))
    } else if s.starts_with("~/") {
        dirs::home_dir().map(|h| h.join(&s[2..])).unwrap_or_else(|| std::path::PathBuf::from(s))
    } else {
        std::path::PathBuf::from(s)
    }
}

/// Draws the currently active modal, dispatching on its variant. Each
/// variant gets a centered box sized to its content (`centered_rect`).
pub fn render_modal(f: &mut Frame, modal: &Modal, area: Rect) {
    match modal {
        Modal::Text { title, prompt, value, max_len, .. } => {
            let rect = centered_rect(60, 8, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL).style(Style::default().bg(Color::Reset));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let counter_line = if let Some(max) = max_len {
                let n = value.len();
                let style = if n >= *max { Style::default().fg(Color::Red) }
                    else if n * 10 >= max * 7 { Style::default().fg(Color::Yellow) }
                    else { Style::default().fg(Color::DarkGray) };
                Line::from(Span::styled(format!("  ({}/{})", n, max), style))
            } else {
                Line::from("")
            };
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", prompt), Style::default().fg(Color::White))),
                counter_line,
                Line::from(vec![Span::styled("  > ", Style::default().fg(Color::Cyan)), Span::raw(value.clone()), Span::styled("█", Style::default().fg(Color::Cyan))]),
                Line::from(""),
                Line::from(Span::styled("  Enter = confirm   Esc = cancel", Style::default().fg(Color::DarkGray))),
            ];
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Select { title, options, cursor, .. } => {
            let h = (options.len() + 5).min(20) as u16;
            let rect = centered_rect(60, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let items: Vec<ListItem> = options.iter().enumerate().map(|(i, opt)| {
                let on = i == *cursor;
                ListItem::new(Line::from(vec![
                    Span::styled(if on { " ▶ " } else { "   " }, Style::default().fg(Color::Cyan)),
                    Span::styled(opt.clone(), if on { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default() }),
                ])).style(if on { Style::default().bg(Color::DarkGray) } else { Style::default() })
            }).collect();
            f.render_widget(List::new(items).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Multi { title, options, checks, cursor, .. } => {
            let h = (options.len() + 5).min(24) as u16;
            let rect = centered_rect(70, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let items: Vec<ListItem> = options.iter().enumerate().map(|(i, opt)| {
                let on      = i == *cursor;
                let checked = checks[i];
                ListItem::new(Line::from(vec![
                    Span::styled(if on { " ▶ " } else { "   " }, Style::default().fg(Color::Cyan)),
                    Span::styled(if checked { "[✓] " } else { "[ ] " }, if checked { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) }),
                    Span::styled(opt.clone(), if on { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default() }),
                ])).style(if on { Style::default().bg(Color::DarkGray) } else { Style::default() })
            }).collect();
            let hint = ListItem::new(Line::from(Span::styled("  Space = toggle  Enter = confirm  Esc = cancel", Style::default().fg(Color::DarkGray))));
            let mut all_items = items;
            all_items.push(hint);
            f.render_widget(List::new(all_items).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Confirm { message, .. } => {
            let rect = centered_rect(50, 6, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(" Confirm ").borders(Borders::ALL).style(Style::default().bg(Color::Black));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(format!("  {}", message), Style::default().fg(Color::White))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  [y] Yes  ", Style::default().fg(Color::Green)),
                    Span::styled("[n/Esc] No", Style::default().fg(Color::Red)),
                ]),
            ];
            f.render_widget(Paragraph::new(lines).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::Info { title, lines, scroll } => {
            let h = (lines.len() + 4).min(30) as u16;
            let rect = centered_rect(70, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL);
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let avail  = inner.height.saturating_sub(1) as usize;
            let items: Vec<ListItem> = lines.iter().skip(*scroll).take(avail)
                .map(|l| ListItem::new(Line::from(Span::styled(format!("  {}", l), Style::default()))))
                .collect();
            let hint = ListItem::new(Line::from(Span::styled("  [↑↓] scroll  [Esc/q] close", Style::default().fg(Color::DarkGray))));
            let mut all = items;
            all.push(hint);
            f.render_widget(List::new(all).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::RenamePreview { title, lines, scroll, .. } => {
            let h = (lines.len() + 4).min(30) as u16;
            let rect = centered_rect(76, h, area);
            f.render_widget(Clear, rect);
            let blk   = Block::default().title(format!(" {} ", title)).borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));
            let inner = blk.inner(rect);
            f.render_widget(blk, rect);
            let avail  = inner.height.saturating_sub(1) as usize;
            let items: Vec<ListItem> = lines.iter().skip(*scroll).take(avail)
                .map(|l| ListItem::new(Line::from(Span::styled(format!("  {}", l), Style::default()))))
                .collect();
            let hint = ListItem::new(Line::from(vec![
                Span::styled("  [↑↓] scroll  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[y/Enter] apply  ", Style::default().fg(Color::Green)),
                Span::styled("[n/Esc] discard", Style::default().fg(Color::Red)),
            ]));
            let mut all = items;
            all.push(hint);
            f.render_widget(List::new(all).style(Style::default().bg(Color::Black)), inner);
        }
        Modal::None => {}
    }
}

// ── Project color palette ─────────────────────────────────────────────────────

const PALETTE: &[(&str, &str)] = &[
    ("#808080", "Gray    (default)"),
    ("#3b82f6", "Blue"),
    ("#22c55e", "Green"),
    ("#ef4444", "Red"),
    ("#eab308", "Yellow"),
    ("#a855f7", "Purple"),
    ("#f97316", "Orange"),
    ("#ec4899", "Pink"),
    ("#06b6d4", "Cyan"),
    ("#14b8a6", "Teal"),
    ("#f43f5e", "Rose"),
];

/// Lists every palette entry as a "`hex  Name`" display string, for the
/// project-color `Modal::Select` picker.
pub fn color_palette() -> Vec<String> {
    PALETTE.iter().map(|(hex, name)| format!("{}  {}", hex, name)).collect()
}

/// Looks up the hex string for a palette index, falling back to the default
/// gray if `cursor` is somehow out of range.
pub fn palette_hex(cursor: usize) -> String {
    PALETTE.get(cursor).map(|(h, _)| h.to_string()).unwrap_or_else(|| "#808080".into())
}

/// Finds the palette index for a stored hex value, so re-opening the color
/// picker on an existing project starts with the right entry highlighted.
pub fn palette_cursor_for(hex: &str) -> usize {
    PALETTE.iter().position(|(h, _)| *h == hex).unwrap_or(0)
}

/// Centers a `pct_x`%-wide, `height`-tall box inside `area`, clamping the
/// height so it never exceeds the available space.
pub fn centered_rect(pct_x: u16, height: u16, area: Rect) -> Rect {
    let w = area.width * pct_x / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, w, height.min(area.height))
}

/// Routes one key event to the active modal's per-character editing logic
/// (typing into a text field, moving a list cursor, toggling a checkbox) and
/// reports whether the modal should be confirmed, cancelled, or left open.
pub fn handle_modal_key(modal: &mut Modal, code: KeyCode) -> ModalAction {
    match modal {
        Modal::Text { value, max_len, .. } => match code {
            KeyCode::Char(c) => {
                if max_len.is_none_or(|m| value.len() < m) { value.push(c); }
                ModalAction::Pending
            }
            KeyCode::Backspace => { value.pop(); ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Select { options, cursor, .. } => match code {
            KeyCode::Up   | KeyCode::Char('k') => { *cursor = cursor.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => { if *cursor + 1 < options.len() { *cursor += 1; } ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Multi { options, checks, cursor, .. } => match code {
            KeyCode::Up   | KeyCode::Char('k') => { *cursor = cursor.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => { if *cursor + 1 < options.len() { *cursor += 1; } ModalAction::Pending }
            KeyCode::Char(' ') => { let c = *cursor; checks[c] = !checks[c]; ModalAction::Pending }
            KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Confirm { .. } => match code {
            KeyCode::Char('y') | KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Char('n') | KeyCode::Esc   => ModalAction::Cancel,
            _ => ModalAction::Pending,
        },
        Modal::Info { lines, scroll, .. } => match code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => ModalAction::Cancel,
            KeyCode::Up   | KeyCode::Char('k') => { *scroll = scroll.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => {
                if *scroll + 1 < lines.len() { *scroll += 1; }
                ModalAction::Pending
            }
            _ => ModalAction::Pending,
        },
        Modal::RenamePreview { lines, scroll, .. } => match code {
            KeyCode::Char('y') | KeyCode::Enter => ModalAction::Confirm,
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => ModalAction::Cancel,
            KeyCode::Up   | KeyCode::Char('k') => { *scroll = scroll.saturating_sub(1); ModalAction::Pending }
            KeyCode::Down | KeyCode::Char('j') => {
                if *scroll + 1 < lines.len() { *scroll += 1; }
                ModalAction::Pending
            }
            _ => ModalAction::Pending,
        },
        Modal::None => ModalAction::Cancel,
    }
}

/// Commits the currently active modal: takes it out of `state.modal`
/// (replacing with `Modal::None`) and routes it through each
/// `modals_confirm_*::try_confirm` in turn until one of them recognizes its
/// tag and handles it. A modal not recognized by any of them is silently
/// dropped, same as an unmatched arm in a single big `match` would be.
pub fn process_modal_confirm(state: &mut HubState) {
    let modal = std::mem::replace(&mut state.modal, Modal::None);
    let modal = match super::modals_confirm_project::try_confirm(state, modal)   { Some(m) => m, None => return };
    let modal = match super::modals_confirm_sync::try_confirm(state, modal)     { Some(m) => m, None => return };
    let modal = match super::modals_confirm_settings::try_confirm(state, modal) { Some(m) => m, None => return };
    let _     = super::modals_confirm_repo::try_confirm(state, modal);
}

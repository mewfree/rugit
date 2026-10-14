pub mod status;
pub mod log;
pub mod popup;
pub mod editor;

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{ActiveBuffer, App};
use crossterm::event::KeyCode;

/// Background of the header, footer and editor status bars.
pub const BAR_BG: Color = Color::Rgb(20, 30, 70);

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    // Layout: header(1) | main(min) | footer(1)
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    let bar = Style::new().bg(BAR_BG).fg(Color::White);
    f.render_widget(Paragraph::new(build_head_line(app)).style(bar), chunks[0]);

    // Main content
    match app.buffer {
        ActiveBuffer::Status => {
            status::render_status(f, app, chunks[1]);
        }
        ActiveBuffer::Log => {
            log::render_log(f, app, chunks[1]);
        }
        ActiveBuffer::Help => {
            // Show status underneath, then overlay help popup
            status::render_status(f, app, chunks[1]);
            popup::render_help(f, area, &app.help_scroll);
        }
        ActiveBuffer::Editor => {
            if let Some(state) = &app.editor {
                editor::render_editor(f, chunks[1], state);
            } else {
                status::render_status(f, app, chunks[1]);
            }
        }
    }

    if let Some(preview) = &app.commit_preview {
        popup::render_commit_preview(f, area, preview);
    }

    // Submenu for the first key of a chord
    match app.pending_key {
        Some(KeyCode::Char('c')) => popup::render_commit_popup(f, area),
        Some(KeyCode::Char('p')) => popup::render_push_popup(f, area),
        Some(KeyCode::Char('z')) => popup::render_stash_popup(f, area),
        Some(KeyCode::Char('b')) => popup::render_branch_popup(f, area),
        Some(KeyCode::Char('r')) => popup::render_rebase_popup(f, area),
        _ => {}
    }

    if let Some(state) = &app.commit_picker {
        popup::render_commit_picker(f, area, state);
    }
    if let Some(state) = &app.stash_list {
        popup::render_stash_list(f, area, state);
    }
    if let Some(state) = &app.branch_picker {
        popup::render_branch_picker(f, area, state);
    }
    if let Some(state) = &app.branch_name_input {
        popup::render_branch_name_input(f, area, state);
    }
    if let Some(state) = &app.palette {
        popup::render_palette(f, area, state);
    }
    if let Some(input) = &app.log_search {
        popup::render_log_search(f, area, input);
    }

    f.render_widget(Paragraph::new(build_footer(app, area.width as usize)).style(bar), chunks[2]);
}

fn build_head_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.backend.kind_name()),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ];
    if app.visual_anchor.is_some() {
        spans.push(Span::styled(
            " VISUAL ",
            Style::new().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    let status = &app.status;
    match (&status.head, &status.head_short_hash, &status.head_summary) {
        (Some(branch), hash, summary) => {
            let (head, summary) = match (hash, summary) {
                (Some(hash), Some(summary)) => (format!("head: {branch} · {hash} "), summary.clone()),
                _ => (format!("head: {branch} "), String::new()),
            };
            spans.extend([
                Span::raw("│ "),
                Span::styled(head, Style::new().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                Span::raw(summary),
            ]);
        }
        _ => spans.push(Span::raw("│ (no commits yet)")),
    }
    Line::from(spans)
}

/// A footer hint, rendered as `[key]label`. Higher `priority` values are
/// dropped first when the terminal is too narrow; 0 is never dropped.
struct Hint {
    key: &'static str,
    label: &'static str,
    priority: u8,
}

const fn hint(key: &'static str, label: &'static str, priority: u8) -> Hint {
    Hint { key, label, priority }
}

const STATUS_HINTS: &[Hint] = &[
    hint("[s/S]", "tage", 1),
    hint("[u/U]", "nstage", 2),
    hint("[x]", "discard", 5),
    hint("[Tab]", "expand", 3),
    hint("[c]", "ommit", 1),
    hint("[z]", "stash", 7),
    hint("[p]", "ush", 4),
    hint("[F]", "pull", 4),
    hint("[b]", "ranch", 6),
    hint("[l]", "og", 6),
    hint("[g]", "refresh", 8),
    hint("[:]", "commands", 5),
    hint("[?]", "help", 0),
    hint("[q]", "uit", 0),
];

const VISUAL_HINTS: &[Hint] = &[
    hint("[j/k]", " extend", 3),
    hint("[s]", "tage", 1),
    hint("[u]", "nstage", 1),
    hint("[x]", "discard", 2),
    hint("[Esc]", " cancel", 0),
];

const LOG_HINTS: &[Hint] = &[
    hint("[j/k]", " move", 2),
    hint("[/]", " filter", 1),
    hint("[Esc]", " clear filter", 3),
    hint("[?]", "help", 0),
    hint("[q]", " back", 0),
];

/// The footer, with a running push/pull's spinner at the left edge, ahead of
/// the hints (which give up width as needed).
fn build_footer(app: &App, width: usize) -> Line<'static> {
    let Some(job) = &app.remote_job else {
        return build_footer_left(app, width);
    };
    let label = job.spinner_line();
    let used = label.chars().count() + 3;
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(label, Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" │", Style::new().fg(Color::DarkGray)),
    ];
    if width > used + 10 {
        spans.extend(build_footer_left(app, width - used).spans);
    }
    Line::from(spans)
}

fn build_footer_left(app: &App, width: usize) -> Line<'static> {
    if let Some(msg) = &app.error_msg {
        return Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("✗ {msg}"),
                Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if let Some(msg) = &app.status_msg {
        return Line::from(vec![
            Span::styled(" ", Style::new()),
            Span::styled(msg.clone(), Style::new().fg(Color::Green)),
        ]);
    }

    let hints = if app.visual_anchor.is_some() {
        VISUAL_HINTS
    } else if app.buffer == ActiveBuffer::Log {
        LOG_HINTS
    } else {
        STATUS_HINTS
    };

    let used = |shown: &[&Hint]| {
        1 + shown.iter().map(|h| h.key.len() + h.label.len()).sum::<usize>()
            + 2 * shown.len().saturating_sub(1)
    };
    let mut shown: Vec<&Hint> = hints.iter().collect();
    while used(&shown) > width {
        // Drop the least important hint; among equals, the rightmost.
        let Some((i, h)) = shown.iter().enumerate().max_by_key(|(i, h)| (h.priority, *i)) else { break };
        if h.priority == 0 {
            break;
        }
        shown.remove(i);
    }

    let mut spans = vec![Span::raw(" ")];
    for (i, h) in shown.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(h.key, Style::new().fg(Color::LightCyan)));
        spans.push(Span::raw(h.label));
    }
    Line::from(spans)
}

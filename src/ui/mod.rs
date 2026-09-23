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
            popup::render_help(f, area);
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
    if let Some(input) = &app.log_search {
        popup::render_log_search(f, area, input);
    }

    f.render_widget(Paragraph::new(build_footer(app)).style(bar), chunks[2]);
}

fn build_head_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.backend.kind_name()),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ];
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

fn build_footer(app: &App) -> Line<'static> {
    if let Some(msg) = &app.status_msg {
        Line::from(vec![
            Span::styled(" ", Style::new()),
            Span::styled(msg.clone(), Style::new().fg(Color::Green)),
        ])
    } else {
        let key = |s: &'static str| Span::styled(s, Style::new().fg(Color::LightCyan));
        let sep = || Span::raw("  ");
        Line::from(vec![
            Span::raw(" "),
            key("[s/S]"), Span::raw("tage"),
            sep(),
            key("[u/U]"), Span::raw("nstage"),
            sep(),
            key("[x]"), Span::raw("discard"),
            sep(),
            key("[Tab]"), Span::raw("expand"),
            sep(),
            key("[c]"), Span::raw("ommit"),
            sep(),
            key("[z]"), Span::raw("stash"),
            sep(),
            key("[p]"), Span::raw("ush"),
            sep(),
            key("[F]"), Span::raw("pull"),
            sep(),
            key("[b]"), Span::raw("ranch"),
            sep(),
            key("[l]"), Span::raw("og"),
            sep(),
            key("[g]"), Span::raw("refresh"),
            sep(),
            key("[?]"), Span::raw("help"),
            sep(),
            key("[q]"), Span::raw("uit"),
        ])
    }
}

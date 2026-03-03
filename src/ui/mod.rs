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

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Build header line text
    let head_text = build_head_line(app);

    // Build footer text
    let footer_text = build_footer(app);

    // Layout: header(1) | main(min) | footer(1)
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    // Header
    let header = Paragraph::new(head_text)
        .style(Style::new().bg(Color::Rgb(20, 30, 70)).fg(Color::White));
    f.render_widget(header, chunks[0]);

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

    // Remote op result popup (overlays everything)
    if let Some((title, output)) = &app.remote_op_result.clone() {
        popup::render_remote_result(f, area, title, output);
    }

    // Remote op result popup (overlays everything)
    if let Some((title, output)) = &app.remote_op_result.clone() {
        popup::render_remote_result(f, area, title, output);
    }

    // Footer
    let footer = Paragraph::new(footer_text)
        .style(Style::new().bg(Color::Rgb(20, 30, 70)).fg(Color::White));
    f.render_widget(footer, chunks[2]);
}

fn build_head_line(app: &App) -> Line<'static> {
    let backend = app.backend.kind_name();
    match (&app.status.head, &app.status.head_short_hash, &app.status.head_summary) {
        (Some(branch), Some(hash), Some(summary)) => {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", backend),
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw("│ "),
                Span::styled(
                    format!("head: {} · {} ", branch, hash),
                    Style::new().fg(Color::LightCyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw(summary.clone()),
            ])
        }
        (Some(branch), _, _) => {
            Line::from(vec![
                Span::styled(
                    format!(" {} ", backend),
                    Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::raw("│ "),
                Span::styled(
                    format!("head: {} ", branch),
                    Style::new().fg(Color::LightCyan).add_modifier(Modifier::BOLD),
                ),
            ])
        }
        _ => Line::from(vec![
            Span::styled(
                format!(" {} ", backend),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("│ (no commits yet)"),
        ]),
    }
}

fn build_footer(app: &App) -> Line<'static> {
    if let Some(msg) = &app.status_msg {
        Line::from(vec![
            Span::styled(" ", Style::new()),
            Span::styled(msg.clone(), Style::new().fg(Color::Green)),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("[s]", Style::new().fg(Color::LightCyan)),
            Span::raw("tage "),
            Span::styled("[u]", Style::new().fg(Color::LightCyan)),
            Span::raw("nstage "),
            Span::styled("[c c]", Style::new().fg(Color::LightCyan)),
            Span::raw("commit "),
            Span::styled("[l]", Style::new().fg(Color::LightCyan)),
            Span::raw("og "),
            Span::styled("[g]", Style::new().fg(Color::LightCyan)),
            Span::raw("refresh "),
            Span::styled("[?]", Style::new().fg(Color::LightCyan)),
            Span::raw("help "),
            Span::styled("[q]", Style::new().fg(Color::LightCyan)),
            Span::raw("uit"),
        ])
    }
}

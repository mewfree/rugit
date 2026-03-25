use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{EditorMode, EditorState};

pub fn render_editor(f: &mut Frame, area: Rect, state: &EditorState) {
    // Layout: outer block | status line
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let border_style = match state.mode {
        EditorMode::Insert => Style::new().fg(Color::Green),
        EditorMode::Normal => Style::new().fg(Color::Yellow),
    };

    let outer_block = Block::default()
        .title(format!(" {} ", state.title))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = outer_block.inner(chunks[0]);
    f.render_widget(outer_block, chunks[0]);

    // Split inner area: textarea on top, comments below
    let comment_count = state.comments.len();
    let comments_height = if comment_count == 0 {
        0
    } else {
        (comment_count + 1).min(inner.height as usize / 2) as u16
    };

    let inner_chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(comments_height),
    ])
    .split(inner);

    // Render textarea (no block — we drew the outer block manually)
    f.render_widget(&state.textarea, inner_chunks[0]);

    // Render comments greyed out
    if comments_height > 0 {
        let comment_lines: Vec<Line> = std::iter::once(Line::from(""))
            .chain(state.comments.iter().map(|c| Line::from(Span::styled(
                format!("# {}", c),
                Style::new().fg(Color::DarkGray),
            ))))
            .collect();
        f.render_widget(Paragraph::new(comment_lines), inner_chunks[1]);
    }

    // Status line
    let status_text = match state.mode {
        EditorMode::Insert => Line::from(Span::styled(
            " -- INSERT --  (Esc: normal mode)",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        EditorMode::Normal => {
            if state.pending_colon {
                Line::from(Span::styled(
                    " -- NORMAL --  (:wq_)",
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(vec![
                    Span::styled(
                        " -- NORMAL --  ",
                        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("i", Style::new().fg(Color::Cyan)),
                    Span::raw(": insert  "),
                    Span::styled("Enter", Style::new().fg(Color::Cyan)),
                    Span::raw(" / "),
                    Span::styled(":wq", Style::new().fg(Color::Cyan)),
                    Span::raw(": save  "),
                    Span::styled("q", Style::new().fg(Color::Cyan)),
                    Span::raw(": abort  "),
                    Span::styled("u", Style::new().fg(Color::Cyan)),
                    Span::raw(": undo  "),
                    Span::styled("C-r", Style::new().fg(Color::Cyan)),
                    Span::raw(": redo"),
                ])
            }
        }
    };

    let status_paragraph = Paragraph::new(status_text)
        .style(Style::new().bg(Color::Rgb(20, 30, 70)));
    f.render_widget(status_paragraph, chunks[1]);
}

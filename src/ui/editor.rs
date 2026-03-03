use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{EditorMode, EditorState};

pub fn render_editor(f: &mut Frame, area: Rect, state: &EditorState) {
    // Split: main text area + status line at bottom
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let text_area = chunks[0];
    let status_area = chunks[1];

    // Build the block
    let border_style = match state.mode {
        EditorMode::Insert => Style::new().fg(Color::Green),
        EditorMode::Normal => Style::new().fg(Color::Yellow),
    };

    let block = Block::default()
        .title(format!(" {} ", state.title))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Inner area inside the block borders
    let inner = block.inner(text_area);

    // Render the block itself
    f.render_widget(block, text_area);

    // Build lines for the text content
    let mut lines: Vec<Line> = Vec::new();

    // Render each line of the editor content
    for (row_idx, line_text) in state.lines.iter().enumerate() {
        if row_idx == state.cursor_row {
            // This line contains the cursor — render with cursor highlight
            let col = state.cursor_col.min(line_text.len());
            let before = &line_text[..col];
            let cursor_char = if col < line_text.len() {
                line_text[col..].chars().next().unwrap_or(' ')
            } else {
                ' '
            };
            let after = if col < line_text.len() {
                let char_len = line_text[col..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                &line_text[col + char_len..]
            } else {
                ""
            };

            let mut spans = Vec::new();
            if !before.is_empty() {
                spans.push(Span::raw(before.to_string()));
            }
            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::new().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD),
            ));
            if !after.is_empty() {
                spans.push(Span::raw(after.to_string()));
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(line_text.clone()));
        }
    }

    // Blank separator before comments
    if !state.comments.is_empty() {
        lines.push(Line::from(""));
    }

    // Render comments greyed out
    for comment in &state.comments {
        lines.push(Line::from(Span::styled(
            format!("# {}", comment),
            Style::new().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

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
                    Span::styled(
                        "i",
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::raw(": insert  "),
                    Span::styled(
                        "Enter",
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::raw(" / "),
                    Span::styled(
                        ":wq",
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::raw(": save  "),
                    Span::styled(
                        "q",
                        Style::new().fg(Color::Cyan),
                    ),
                    Span::raw(": abort"),
                ])
            }
        }
    };

    let status_paragraph = Paragraph::new(status_text)
        .style(Style::new().bg(Color::Rgb(20, 30, 70)));
    f.render_widget(status_paragraph, status_area);
}

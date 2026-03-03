use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn render_help(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 70, area);

    // Clear the background
    f.render_widget(Clear, popup_area);

    let help_lines = vec![
        Line::from(Span::styled(
            "Keybindings",
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  j / ↓     ", Style::new().fg(Color::Cyan)),
            Span::raw("Move down"),
        ]),
        Line::from(vec![
            Span::styled("  k / ↑     ", Style::new().fg(Color::Cyan)),
            Span::raw("Move up"),
        ]),
        Line::from(vec![
            Span::styled("  s         ", Style::new().fg(Color::Cyan)),
            Span::raw("Stage file / section"),
        ]),
        Line::from(vec![
            Span::styled("  u         ", Style::new().fg(Color::Cyan)),
            Span::raw("Unstage file / section"),
        ]),
        Line::from(vec![
            Span::styled("  S         ", Style::new().fg(Color::Cyan)),
            Span::raw("Stage all changes"),
        ]),
        Line::from(vec![
            Span::styled("  U         ", Style::new().fg(Color::Cyan)),
            Span::raw("Unstage all changes"),
        ]),
        Line::from(vec![
            Span::styled("  Tab       ", Style::new().fg(Color::Cyan)),
            Span::raw("Toggle diff expansion"),
        ]),
        Line::from(vec![
            Span::styled("  c c       ", Style::new().fg(Color::Cyan)),
            Span::raw("Commit (opens $EDITOR)"),
        ]),
        Line::from(vec![
            Span::styled("  l         ", Style::new().fg(Color::Cyan)),
            Span::raw("Switch to log view"),
        ]),
        Line::from(vec![
            Span::styled("  b         ", Style::new().fg(Color::Cyan)),
            Span::raw("Switch to status view"),
        ]),
        Line::from(vec![
            Span::styled("  g         ", Style::new().fg(Color::Cyan)),
            Span::raw("Refresh"),
        ]),
        Line::from(vec![
            Span::styled("  ?         ", Style::new().fg(Color::Cyan)),
            Span::raw("Show this help"),
        ]),
        Line::from(vec![
            Span::styled("  p         ", Style::new().fg(Color::Cyan)),
            Span::raw("Push menu (p p: push  p f: force-push)"),
        ]),
        Line::from(vec![
            Span::styled("  F         ", Style::new().fg(Color::Cyan)),
            Span::raw("Pull / fetch"),
        ]),
        Line::from(vec![
            Span::styled("  Esc / q   ", Style::new().fg(Color::Cyan)),
            Span::raw("Quit / close help"),
        ]),
    ];

    let paragraph = Paragraph::new(help_lines)
        .block(
            Block::default()
                .title(" Help ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

pub fn render_commit_preview(f: &mut Frame, area: Rect, title: &str, content: &str, scroll: u16) {
    let popup_area = centered_rect(90, 85, area);
    f.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = content.lines().map(|l| {
        let style = if l.starts_with('+') && !l.starts_with("+++") {
            Style::new().fg(Color::Green)
        } else if l.starts_with('-') && !l.starts_with("---") {
            Style::new().fg(Color::Red)
        } else if l.starts_with("@@") {
            Style::new().fg(Color::Cyan)
        } else if l.starts_with("commit ") || l.starts_with("Author:") || l.starts_with("Date:") {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new()
        };
        Line::from(Span::styled(l.to_owned(), style))
    }).collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  q / Esc to close",
        Style::new().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Yellow)),
        )
        .scroll((scroll, 0));

    f.render_widget(paragraph, popup_area);
}

pub fn render_commit_popup(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(50, 30, area);
    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  c  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Commit"),
        ]),
        Line::from(vec![
            Span::styled("  a  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Amend"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Esc / any other key: cancel",
            Style::new().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Commit ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Green)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

pub fn render_push_popup(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(50, 30, area);
    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  p  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Push"),
        ]),
        Line::from(vec![
            Span::styled("  f  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Force push (--force-with-lease)"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Esc / any other key: cancel",
            Style::new().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Push ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

pub fn render_remote_result(f: &mut Frame, area: Rect, title: &str, output: &str) {
    let popup_area = centered_rect(70, 60, area);
    f.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = output.lines().map(|l| Line::from(l.to_owned())).collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press q or Esc to dismiss",
        Style::new().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

/// Returns a centered rect with the given percentage width/height.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

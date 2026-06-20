use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use crate::app::{CommitPickerState, FixupMode, StashListState};

pub fn render_help(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(60, 90, area);

    // Clear the background
    f.render_widget(Clear, popup_area);

    let section = |s: &'static str| Line::from(Span::styled(
        s, Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ));
    let key = |k: &'static str, desc: &'static str| Line::from(vec![
        Span::styled(k, Style::new().fg(Color::Cyan)),
        Span::raw(desc),
    ]);

    let help_lines = vec![
        section("  Navigation"),
        key("  j / ↓       ", "Move down"),
        key("  k / ↑       ", "Move up"),
        key("  Tab         ", "Expand / collapse diff"),
        key("  Enter       ", "Preview commit"),
        Line::from(""),
        section("  Staging"),
        key("  s           ", "Stage file or hunk at point"),
        key("  u           ", "Unstage file or hunk at point"),
        key("  x           ", "Discard changes at point"),
        key("  S           ", "Stage all changes"),
        key("  U           ", "Unstage all changes"),
        Line::from(""),
        section("  Commits"),
        key("  c           ", "Open commit menu"),
        key("  c c         ", "Commit staged changes"),
        key("  c a         ", "Amend last commit"),
        key("  c F         ", "Instant fixup into a commit"),
        key("  c s         ", "Instant squash into a commit"),
        Line::from(""),
        section("  Stash"),
        key("  z           ", "Open stash menu"),
        key("  z z         ", "Stash changes"),
        key("  z p         ", "Pop latest stash"),
        key("  z a         ", "Apply latest stash"),
        key("  z d         ", "Drop latest stash"),
        key("  z l         ", "List stashes"),
        Line::from(""),
        section("  Remotes"),
        key("  p           ", "Open push menu"),
        key("  p p         ", "Push to upstream"),
        key("  p f         ", "Force-push (--force-with-lease)"),
        key("  F           ", "Pull from upstream"),
        Line::from(""),
        section("  Views & misc"),
        key("  l           ", "Switch to log view"),
        key("  b           ", "Switch to status view"),
        key("  g           ", "Refresh"),
        key("  ?           ", "Show this help"),
        key("  q / Esc     ", "Quit / close"),
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

pub fn render_commit_picker(f: &mut Frame, area: Rect, state: &CommitPickerState) {
    let popup_area = centered_rect(70, 60, area);
    f.render_widget(Clear, popup_area);

    let title = match state.mode {
        FixupMode::Fixup  => " Fixup: select target commit ",
        FixupMode::Squash => " Squash: select target commit ",
    };

    let inner_height = popup_area.height.saturating_sub(4) as usize; // border + hint line
    let visible_start = if state.cursor >= inner_height {
        state.cursor - inner_height + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, commit) in state.commits.iter().enumerate().skip(visible_start).take(inner_height) {
        let selected = i == state.cursor;
        let prefix = if selected { "> " } else { "  " };
        let hash_style = if selected {
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        let msg_style = if selected {
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("{} ", commit.short_hash), hash_style),
            Span::styled(commit.summary.clone(), msg_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  j/k: navigate   Enter: confirm   Esc: cancel",
        Style::new().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

pub fn render_commit_popup(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(50, 40, area);
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
        Line::from(vec![
            Span::styled("  F  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Instant fixup"),
        ]),
        Line::from(vec![
            Span::styled("  s  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Instant squash"),
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

pub fn render_stash_popup(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(50, 40, area);
    f.render_widget(Clear, popup_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  z  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Stash changes"),
        ]),
        Line::from(vec![
            Span::styled("  p  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Pop latest stash"),
        ]),
        Line::from(vec![
            Span::styled("  a  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Apply latest stash"),
        ]),
        Line::from(vec![
            Span::styled("  d  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Drop latest stash"),
        ]),
        Line::from(vec![
            Span::styled("  l  ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("List stashes"),
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
                .title(" Stash ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Magenta)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, popup_area);
}

pub fn render_stash_list(f: &mut Frame, area: Rect, state: &StashListState) {
    let popup_area = centered_rect(70, 60, area);
    f.render_widget(Clear, popup_area);

    let inner_height = popup_area.height.saturating_sub(4) as usize;
    let visible_start = if state.cursor >= inner_height {
        state.cursor - inner_height + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, stash) in state.stashes.iter().enumerate().skip(visible_start).take(inner_height) {
        let selected = i == state.cursor;
        let prefix = if selected { "> " } else { "  " };
        let style = if selected {
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, stash.summary), style)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  j/k: navigate   a: apply   p: pop   d: drop   Esc: close",
        Style::new().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Stashes ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Magenta)),
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

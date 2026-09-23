use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use crate::app::{BranchNameInputState, BranchNameMode, BranchPickerMode, BranchPickerState, CommitPickerState, CommitPreview, FixupMode, StashListState};

/// Bordered block with a centered title.
fn popup_block<'a>(title: impl Into<Line<'a>>, color: Color) -> Block<'a> {
    Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::new().fg(color))
}

fn hint(text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::new().fg(Color::DarkGray)))
}

/// Clear a centered area of `percent_x` × `percent_y` and draw `lines` in it.
fn render_popup(f: &mut Frame, area: Rect, (percent_x, percent_y): (u16, u16), block: Block, lines: Vec<Line>) {
    let popup_area = centered_rect(percent_x, percent_y, area);
    f.render_widget(Clear, popup_area);
    f.render_widget(Paragraph::new(lines).block(block), popup_area);
}

/// A chord submenu: one line per (key, label).
fn render_menu(f: &mut Frame, area: Rect, percent_y: u16, title: &str, color: Color, entries: &[(&str, &str)]) {
    let key_style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let mut lines = vec![Line::from("")];
    lines.extend(entries.iter().map(|&(key, label)| {
        Line::from(vec![Span::styled(format!("  {key}  "), key_style), Span::raw(label)])
    }));
    lines.push(Line::from(""));
    lines.push(hint("  Esc / any other key: cancel"));
    render_popup(f, area, (50, percent_y), popup_block(format!(" {title} "), color), lines);
}

/// Rows for a scrolling list popup of `size`: the window of `items` around
/// `cursor`, then the hint. `row` renders one item, given whether it's selected.
fn list_lines<T>(
    area: Rect,
    size: (u16, u16),
    items: &[T],
    cursor: usize,
    hint_text: &str,
    row: impl Fn(&T, bool) -> Line<'static>,
) -> Vec<Line<'static>> {
    // Leave room for the borders, the blank first line and the hint.
    let height = centered_rect(size.0, size.1, area).height.saturating_sub(4) as usize;
    let start = (cursor + 1).saturating_sub(height);

    let mut lines = vec![Line::from("")];
    lines.extend(
        items
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .map(|(i, item)| row(item, i == cursor)),
    );
    lines.push(Line::from(""));
    lines.push(hint(hint_text));
    lines
}

/// A single-line text prompt.
fn render_prompt(f: &mut Frame, area: Rect, title: &str, prompt: String, hint_text: &str) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(prompt, Style::new().fg(Color::White))),
        Line::from(""),
        hint(hint_text),
    ];
    render_popup(f, area, (50, 20), popup_block(title.to_string(), Color::LightGreen), lines);
}

fn selection_prefix(selected: bool) -> &'static str {
    if selected { "> " } else { "  " }
}

pub fn render_help(f: &mut Frame, area: Rect) {
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
        key("  c w         ", "Reword a commit's message"),
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
        section("  Branches"),
        key("  b           ", "Open branch menu"),
        key("  b b         ", "Checkout branch"),
        key("  b c         ", "Create & checkout new branch"),
        key("  b d         ", "Delete branch"),
        key("  b r         ", "Rename current branch"),
        Line::from(""),
        section("  Views & misc"),
        key("  l           ", "Switch to log view"),
        key("  /           ", "Filter log by hash/author/message (in log view)"),
        key("  Esc         ", "Clear log filter"),
        key("  g           ", "Refresh"),
        key("  ?           ", "Show this help"),
        key("  q / Esc     ", "Quit / close"),
    ];

    render_popup(f, area, (60, 90), popup_block(" Help ", Color::Yellow), help_lines);
}

pub fn render_commit_preview(f: &mut Frame, area: Rect, preview: &CommitPreview) {
    let popup_area = centered_rect(90, 85, area);
    f.render_widget(Clear, popup_area);

    let mut lines: Vec<Line> = preview.content.lines().map(|l| {
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
        Line::from(Span::styled(l, style))
    }).collect();

    lines.push(Line::from(""));
    lines.push(hint("  q / Esc to close"));

    let paragraph = Paragraph::new(lines)
        .block(popup_block(format!(" {} ", preview.title), Color::Yellow))
        .scroll((preview.scroll, 0));

    f.render_widget(paragraph, popup_area);
}

pub fn render_commit_picker(f: &mut Frame, area: Rect, state: &CommitPickerState) {
    let title = match state.mode {
        FixupMode::Fixup  => " Fixup: select target commit ",
        FixupMode::Squash => " Squash: select target commit ",
        FixupMode::Reword => " Reword: select target commit ",
    };
    let size = (70, 60);
    let lines = list_lines(
        area, size, &state.commits, state.cursor,
        "  j/k: navigate   Enter: confirm   Esc: cancel",
        |commit, selected| {
            let (hash_style, msg_style) = if selected {
                (
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
                )
            } else {
                (Style::new().fg(Color::DarkGray), Style::new())
            };
            Line::from(vec![
                Span::raw(selection_prefix(selected)),
                Span::styled(format!("{} ", commit.short_hash), hash_style),
                Span::styled(commit.summary.clone(), msg_style),
            ])
        },
    );
    render_popup(f, area, size, popup_block(title, Color::Yellow), lines);
}

pub fn render_commit_popup(f: &mut Frame, area: Rect) {
    render_menu(f, area, 50, "Commit", Color::Green, &[
        ("c", "Commit"),
        ("a", "Amend"),
        ("F", "Instant fixup"),
        ("s", "Instant squash"),
        ("w", "Reword message"),
    ]);
}

pub fn render_push_popup(f: &mut Frame, area: Rect) {
    render_menu(f, area, 30, "Push", Color::Cyan, &[
        ("p", "Push"),
        ("f", "Force push (--force-with-lease)"),
    ]);
}

pub fn render_stash_popup(f: &mut Frame, area: Rect) {
    render_menu(f, area, 40, "Stash", Color::Magenta, &[
        ("z", "Stash changes"),
        ("p", "Pop latest stash"),
        ("a", "Apply latest stash"),
        ("d", "Drop latest stash"),
        ("l", "List stashes"),
    ]);
}

pub fn render_stash_list(f: &mut Frame, area: Rect, state: &StashListState) {
    let size = (70, 60);
    let lines = list_lines(
        area, size, &state.stashes, state.cursor,
        "  j/k: navigate   a: apply   p: pop   d: drop   Esc: close",
        |stash, selected| {
            let style = if selected {
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            Line::from(Span::styled(
                format!("{}stash@{{{}}}: {}", selection_prefix(selected), stash.index, stash.summary),
                style,
            ))
        },
    );
    render_popup(f, area, size, popup_block(" Stashes ", Color::Magenta), lines);
}

pub fn render_branch_popup(f: &mut Frame, area: Rect) {
    render_menu(f, area, 35, "Branch", Color::LightGreen, &[
        ("b", "Checkout branch"),
        ("c", "Create & checkout new branch"),
        ("d", "Delete branch"),
        ("r", "Rename current branch"),
    ]);
}

pub fn render_branch_picker(f: &mut Frame, area: Rect, state: &BranchPickerState) {
    let (title, hint_text) = match state.mode {
        BranchPickerMode::Checkout => (" Checkout branch ", "  j/k: navigate   Enter: checkout   Esc: cancel"),
        BranchPickerMode::Delete   => (" Delete branch ", "  j/k: navigate   Enter: delete   Esc: cancel"),
    };
    let size = (60, 60);
    let lines = list_lines(
        area, size, &state.branches, state.cursor, hint_text,
        |branch, selected| {
            let marker = if branch.is_current {
                Span::styled("* ", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::raw("  ")
            };
            let name_style = if selected {
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if branch.is_current {
                Style::new().fg(Color::Green)
            } else {
                Style::new()
            };
            Line::from(vec![
                Span::raw(selection_prefix(selected)),
                marker,
                Span::styled(branch.name.clone(), name_style),
            ])
        },
    );
    render_popup(f, area, size, popup_block(title, Color::LightGreen), lines);
}

pub fn render_branch_name_input(f: &mut Frame, area: Rect, state: &BranchNameInputState) {
    let (title, prompt) = match state.mode {
        BranchNameMode::Create => (" New branch name ", format!("  Branch name: {}_", state.input)),
        BranchNameMode::Rename => (" Rename branch ", format!("  Rename '{}' to: {}_", state.original, state.input)),
    };
    render_prompt(f, area, title, prompt, "  Enter: confirm   Esc: cancel");
}

pub fn render_log_search(f: &mut Frame, area: Rect, input: &str) {
    render_prompt(f, area, " Search log ", format!("  /{input}_"), "  Enter: keep filter   Esc: cancel");
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

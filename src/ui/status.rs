use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Section, StatusItem};
use crate::backend::FileKind;

pub fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| status_item_to_list_item(item))
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !app.items.is_empty() {
        state.select(Some(app.cursor));
    }

    f.render_stateful_widget(list, area, &mut state);
}

fn status_item_to_list_item(item: &StatusItem) -> ListItem<'static> {
    match item {
        StatusItem::Header { label, count, section } => {
            let color = section_color(section);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ({})", label, count),
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]);
            ListItem::new(line)
        }
        StatusItem::File { entry, section } => {
            let color = section_color(section);
            let kind_str = kind_prefix(&entry.kind);
            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} ", kind_str),
                    Style::new().fg(color),
                ),
                Span::raw(entry.path.clone()),
            ]);
            ListItem::new(line)
        }
        StatusItem::Diff { lines } => {
            let text: Vec<Line> = lines
                .iter()
                .map(|l| {
                    let style = if l.starts_with('+') {
                        Style::new().fg(Color::Green)
                    } else if l.starts_with('-') {
                        Style::new().fg(Color::Red)
                    } else if l.starts_with('@') {
                        Style::new().fg(Color::Cyan)
                    } else {
                        Style::new()
                    };
                    Line::from(Span::styled(format!("    {}", l), style))
                })
                .collect();
            // Combine all diff lines into a single ListItem
            // We show just the first line as summary if too large
            if text.is_empty() {
                ListItem::new(Line::from("    (empty diff)"))
            } else {
                // Use the first line to represent the diff block (ratatui ListItem takes Text)
                use ratatui::text::Text;
                let text_widget = Text::from(text);
                ListItem::new(text_widget)
            }
        }
    }
}

fn section_color(section: &Section) -> Color {
    match section {
        Section::Staged => Color::Green,
        Section::Unstaged => Color::Red,
        Section::Untracked => Color::DarkGray,
    }
}

fn kind_prefix(kind: &FileKind) -> &'static str {
    match kind {
        FileKind::Modified => "M",
        FileKind::Added => "A",
        FileKind::Deleted => "D",
        FileKind::Renamed(_) => "R",
        FileKind::Untracked => "?",
        FileKind::Conflicted => "!",
    }
}

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Section, StatusItem};
use crate::backend::FileKind;

// Palette
const COL_STAGED:   Color = Color::LightGreen;
const COL_UNSTAGED: Color = Color::LightRed;
const COL_UNTRACKED:Color = Color::Gray;
const COL_RECENT:   Color = Color::LightBlue;
const COL_HASH:     Color = Color::Cyan;
const COL_DIM:      Color = Color::DarkGray;

pub fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(status_item_to_list_item)
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(Color::Rgb(40, 60, 120))
                .fg(Color::White)
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
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ({})", label, count),
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]))
        }

        StatusItem::File { entry, section, is_expanded } => {
            let color = section_color(section);
            let kind_str = kind_prefix(&entry.kind);
            let suffix = if *is_expanded { "" } else { " …" };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} ", kind_str),
                    Style::new().fg(color),
                ),
                Span::raw(entry.path.clone()),
                Span::styled(suffix, Style::new().fg(COL_DIM)),
            ]))
        }

        StatusItem::Diff { lines } => {
            if lines.is_empty() {
                return ListItem::new(Line::from(
                    Span::styled("    (empty diff)", Style::new().fg(COL_DIM)),
                ));
            }
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
                        Style::new().fg(COL_DIM)
                    };
                    Line::from(Span::styled(format!("    {}", l), style))
                })
                .collect();
            use ratatui::text::Text;
            ListItem::new(Text::from(text))
        }

        StatusItem::RecentHeader => {
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "Recent commits",
                    Style::new().fg(COL_RECENT).add_modifier(Modifier::BOLD),
                ),
            ]))
        }

        StatusItem::Spacer => ListItem::new(Line::from("")),

        StatusItem::RecentCommit { info } => {
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} ", info.short_hash),
                    Style::new().fg(COL_HASH),
                ),
                Span::raw(info.summary.clone()),
            ]))
        }
    }
}

fn section_color(section: &Section) -> Color {
    match section {
        Section::Staged   => COL_STAGED,
        Section::Unstaged => COL_UNSTAGED,
        Section::Untracked => COL_UNTRACKED,
    }
}

fn kind_prefix(kind: &FileKind) -> &'static str {
    match kind {
        FileKind::Modified   => "M",
        FileKind::Added      => "A",
        FileKind::Deleted    => "D",
        FileKind::Renamed(_) => "R",
        FileKind::Untracked  => "?",
        FileKind::Conflicted => "!",
    }
}

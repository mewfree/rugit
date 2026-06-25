use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Section, StatusItem};
use crate::backend::{FileKind, StashInfo};

// Palette
const COL_STAGED: Color = Color::LightGreen;
const COL_UNSTAGED: Color = Color::LightRed;
const COL_UNTRACKED: Color = Color::Gray;
const COL_RECENT: Color = Color::LightBlue;
const COL_HASH: Color = Color::Cyan;
const COL_DIM: Color = Color::DarkGray;

pub fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let visual_range = app.visual_anchor.map(|anchor| {
        if anchor <= app.cursor { (anchor, app.cursor) } else { (app.cursor, anchor) }
    });

    let items: Vec<ListItem> = app.items.iter().enumerate().map(|(i, item)| {
        let in_visual = visual_range.map(|(s, e)| i >= s && i <= e && i != app.cursor).unwrap_or(false);
        status_item_to_list_item(item, in_visual)
    }).collect();

    let list = List::new(items).highlight_style(
        Style::new()
            .bg(Color::Rgb(40, 60, 120))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if !app.items.is_empty() {
        state.select(Some(app.cursor));
        // Scrolloff: keep 5 lines visible below cursor when possible
        const SCROLLOFF: usize = 5;
        let h = area.height as usize;
        let n = app.items.len();
        let offset = (app.cursor + SCROLLOFF + 1)
            .saturating_sub(h)
            .min(n.saturating_sub(h));
        *state.offset_mut() = offset;
    }

    f.render_stateful_widget(list, area, &mut state);
}

fn status_item_to_list_item(item: &StatusItem, in_visual: bool) -> ListItem<'static> {
    let visual_bg = Color::Rgb(60, 40, 100);

    let list_item = match item {
        StatusItem::Header {
            label,
            count,
            section,
        } => {
            let color = section_color(section);
            ListItem::new(Line::from(vec![Span::styled(
                format!("{} ({})", label, count),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            )]))
        }

        StatusItem::File {
            entry,
            section: _,
            is_expanded,
        } => {
            let color = kind_color(&entry.kind);
            let kind_str = kind_prefix(&entry.kind);
            let suffix = if *is_expanded { "" } else { "…" };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", kind_str), Style::new().fg(color)),
                Span::raw(entry.path.clone()),
                Span::styled(suffix, Style::new().fg(COL_DIM)),
            ]))
        }

        StatusItem::HunkHeader { line, .. } => {
            ListItem::new(Line::from(Span::styled(
                format!("    {}", line),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )))
        }

        StatusItem::DiffLine { line, .. } => {
            let style = if line.starts_with('+') {
                Style::new().fg(Color::Green)
            } else if line.starts_with('-') {
                Style::new().fg(Color::Red)
            } else if line.starts_with('@') {
                Style::new().fg(Color::Cyan)
            } else {
                Style::new().fg(COL_DIM)
            };
            ListItem::new(Line::from(Span::styled(
                format!("    {}", line),
                style,
            )))
        }

        StatusItem::UnpushedHeader { count, upstream } => ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("Unpushed to {} ({})", upstream, count),
                Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
        ])),

        StatusItem::RecentHeader => ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Recent commits",
                Style::new().fg(COL_RECENT).add_modifier(Modifier::BOLD),
            ),
        ])),

        StatusItem::StashHeader { count } => ListItem::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("Stashes ({})", count),
                Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
        ])),

        StatusItem::StashEntry { info } => ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("stash@{{{}}} ", info.index), Style::new().fg(Color::Magenta)),
            Span::raw(stash_summary(info)),
        ])),

        StatusItem::Spacer => ListItem::new(Line::from("")),

        StatusItem::RecentCommit { info } => ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{} ", info.short_hash), Style::new().fg(COL_HASH)),
            Span::raw(info.summary.clone()),
        ])),
    };

    if in_visual {
        list_item.style(Style::new().bg(visual_bg))
    } else {
        list_item
    }
}

fn stash_summary(info: &StashInfo) -> String {
    let prefix = format!("stash@{{{}}}: ", info.index);
    info.summary.strip_prefix(&prefix)
        .unwrap_or(&info.summary)
        .to_string()
}

fn section_color(section: &Section) -> Color {
    match section {
        Section::Staged => COL_STAGED,
        Section::Unstaged => COL_UNSTAGED,
        Section::Untracked => COL_UNTRACKED,
    }
}

// Orange — ratatui has no named orange.
const COL_ORANGE: Color = Color::Rgb(255, 165, 0);

fn kind_color(kind: &FileKind) -> Color {
    match kind {
        FileKind::Added | FileKind::Untracked => COL_STAGED, // green: new content
        FileKind::Modified => COL_ORANGE,                    // orange: changed
        FileKind::Renamed(_) => COL_RECENT,                  // blue: moved
        FileKind::Deleted => COL_UNSTAGED,                   // red: gone
        FileKind::Conflicted => Color::Magenta,              // conflict
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

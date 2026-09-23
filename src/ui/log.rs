use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use super::status::visible_window;
use crate::app::App;

pub fn render_log(f: &mut Frame, app: &App, area: Rect) {
    let count = app.log_visible_len();

    let block = match app.log_filter.as_deref() {
        Some(query) if !query.is_empty() => Block::default().title(format!(
            " /{} ({} match{}) — Esc to clear ",
            query,
            count,
            if count == 1 { "" } else { "es" }
        )),
        _ => Block::default(),
    };

    // Build widgets only for the rows on screen, as the status buffer does.
    let (offset, end) = visible_window(app.cursor, block.inner(area).height as usize, count);
    let items: Vec<ListItem> = app
        .log_visible()
        .skip(offset)
        .take(end - offset)
        .map(|commit| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", commit.short_hash),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{} ", commit.author), Style::new().fg(Color::Cyan)),
                Span::raw(commit.summary.as_str()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    let mut state = ListState::default();
    if count > 0 {
        state.select(Some(app.cursor - offset));
    }

    f.render_stateful_widget(list, area, &mut state);
}

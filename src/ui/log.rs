use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::app::App;

pub fn render_log(f: &mut Frame, app: &mut App, area: Rect) {
    let count = app.log_visible_len();

    let items: Vec<ListItem> = app
        .log_visible()
        .map(|commit| {
            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", commit.short_hash),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} ", commit.author),
                    Style::new().fg(Color::Cyan),
                ),
                Span::raw(commit.summary.clone()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = match app.log_filter.as_deref() {
        Some(query) if !query.is_empty() => Block::default().title(format!(
            " /{} ({} match{}) — Esc to clear ",
            query,
            count,
            if count == 1 { "" } else { "es" }
        )),
        _ => Block::default(),
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if count > 0 {
        state.select(Some(app.cursor));
    }

    f.render_stateful_widget(list, area, &mut state);
}

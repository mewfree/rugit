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
const COL_STAGED: Color = Color::LightGreen;
const COL_UNSTAGED: Color = Color::LightRed;
const COL_UNTRACKED: Color = Color::Gray;
const COL_RECENT: Color = Color::LightBlue;
const COL_HASH: Color = Color::Cyan;
const COL_DIM: Color = Color::DarkGray;

/// Lines kept visible below the cursor when possible.
const SCROLLOFF: usize = 5;

/// The `[offset, end)` slice of items to build widgets for. Items are one line
/// each, so the offset is a pure function of the cursor.
/// Guarantees `offset <= cursor < end` when `height > 0 && cursor < n`.
fn visible_window(cursor: usize, height: usize, n: usize) -> (usize, usize) {
    let offset = (cursor + SCROLLOFF + 1)
        .saturating_sub(height)
        .min(n.saturating_sub(height))
        .min(cursor); // short viewports: keep offset from passing the cursor
    let end = offset.saturating_add(height).min(n);
    (offset, end)
}

pub fn render_status(f: &mut Frame, app: &mut App, area: Rect) {
    let visual_range = app.visual_anchor.map(|anchor| {
        if anchor <= app.cursor { (anchor, app.cursor) } else { (app.cursor, anchor) }
    });

    let (offset, end) = visible_window(app.cursor, area.height as usize, app.items.len());

    let items: Vec<ListItem> = app.items[offset..end]
        .iter()
        .enumerate()
        .map(|(vis_i, item)| {
            let i = offset + vis_i;
            let in_visual = visual_range
                .map(|(s, e)| i >= s && i <= e && i != app.cursor)
                .unwrap_or(false);
            status_item_to_list_item(item, in_visual)
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::new()
            .bg(Color::Rgb(40, 60, 120))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    // Selection is relative to the window, not the full list.
    let mut state = ListState::default();
    if !app.items.is_empty() {
        state.select(Some(app.cursor - offset));
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
            Span::raw(info.summary.clone()),
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

#[cfg(test)]
mod tests {
    use super::{render_status, visible_window, SCROLLOFF};
    use crate::app::App;
    use crate::backend::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus, StashInfo};
    use crate::config::Config;
    use anyhow::{bail, Result};
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};
    use std::path::Path;

    /// In-memory backend so render paths work without a repo.
    struct MockBackend {
        status: RepoStatus,
    }

    /// Rendering touches none of the write paths.
    macro_rules! unsupported {
        ($($name:ident($($arg:ty),*) -> $ret:ty;)*) => {
            $(fn $name(&self, $(_: $arg),*) -> $ret { bail!("unsupported in tests") })*
        };
    }

    impl Backend for MockBackend {
        fn repo_root(&self) -> &Path { Path::new("/tmp") }
        fn kind_name(&self) -> &'static str { "git" }
        fn status(&self) -> Result<RepoStatus> { Ok(self.status.clone()) }
        fn log(&self, _limit: usize) -> Result<Vec<CommitInfo>> { Ok(vec![]) }
        fn stash_list(&self) -> Result<Vec<StashInfo>> { Ok(vec![]) }
        unsupported! {
            diff_file(&str, bool) -> Result<String>;
            stage_file(&str) -> Result<()>;
            unstage_file(&str) -> Result<()>;
            discard_file(&str) -> Result<()>;
            stage_all() -> Result<()>;
            unstage_all() -> Result<()>;
            commit(&str) -> Result<()>;
            amend(&str) -> Result<()>;
            head_commit_message() -> Result<String>;
            commit_message(&str) -> Result<String>;
            reword_commit(&str, &str) -> Result<()>;
            push() -> Result<String>;
            push_force_lease() -> Result<String>;
            pull() -> Result<String>;
            show_commit(&str) -> Result<String>;
            apply_patch(&str, bool) -> Result<()>;
            discard_patch(&str) -> Result<()>;
            discard_hunk(&str, usize) -> Result<()>;
            discard_all_unstaged() -> Result<()>;
            discard_staged_file(&str) -> Result<()>;
            discard_all_staged() -> Result<()>;
            fixup_commit(&str) -> Result<()>;
            squash_commit(&str) -> Result<()>;
            stash() -> Result<()>;
            stash_pop() -> Result<()>;
            stash_apply(usize) -> Result<()>;
            stash_drop(usize) -> Result<()>;
            list_branches() -> Result<Vec<crate::backend::BranchInfo>>;
            checkout_branch(&str) -> Result<()>;
            create_branch(&str) -> Result<()>;
            delete_branch(&str) -> Result<()>;
            rename_branch(&str, &str) -> Result<()>;
        }
    }

    /// An app with one expanded file whose diff has `diff_lines` lines.
    fn app_with_big_diff(diff_lines: usize) -> App {
        let status = RepoStatus {
            unstaged: vec![FileEntry { path: "big.txt".into(), kind: FileKind::Modified }],
            ..Default::default()
        };
        let backend = Box::new(MockBackend { status });
        let mut app = App::new(backend, Config::default()).unwrap();
        let mut diff = String::from("@@ -1,1 +1,1 @@\n");
        for i in 0..diff_lines {
            diff.push_str(&format!("+changed line {}\n", i));
        }
        app.diff_cache.insert("unstaged:big.txt".into(), diff);
        app.expanded.insert("unstaged:big.txt".into());
        app.rebuild_items();
        app
    }

    /// An out-of-range slice or selection index would panic here.
    #[test]
    fn renders_huge_diff_at_every_cursor_position_without_panicking() {
        let mut app = app_with_big_diff(2000);
        let n = app.items.len();
        assert!(n > 2000, "expected a large item list, got {n}");

        for &height in &[1u16, 2, 6, 24, 80] {
            let mut terminal = Terminal::new(TestBackend::new(100, height)).unwrap();
            for cursor in 0..n {
                app.cursor = cursor;
                terminal
                    .draw(|f| render_status(f, &mut app, Rect::new(0, 0, 100, height)))
                    .unwrap();
            }
        }
    }

    #[test]
    fn cursor_line_is_visible_in_the_rendered_buffer() {
        let mut app = app_with_big_diff(2000);
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();

        for &cursor in &[0usize, 1, 50, 500, 1500, app.items.len() - 1] {
            app.cursor = cursor;
            terminal
                .draw(|f| render_status(f, &mut app, Rect::new(0, 0, 100, 24)))
                .unwrap();

            let (offset, _) = visible_window(cursor, 24, app.items.len());
            let buf = terminal.backend().buffer();
            let row: String = (0..100)
                .map(|x| buf[(x, (cursor - offset) as u16)].symbol())
                .collect::<String>();
            // That row must hold the cursor's item.
            if let crate::app::StatusItem::DiffLine { line, .. } = &app.items[cursor] {
                assert!(row.contains(line.trim_end()), "cursor={cursor} row={row:?}");
            }
        }
    }

    #[test]
    fn renders_empty_list() {
        let backend = Box::new(MockBackend { status: RepoStatus::default() });
        let mut app = App::new(backend, Config::default()).unwrap();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render_status(f, &mut app, Rect::new(0, 0, 80, 24)))
            .unwrap();
    }


    /// `cursor - offset` must index the sliced list correctly.
    #[test]
    fn cursor_always_inside_window() {
        for n in [0usize, 1, 2, 7, 40, 3000] {
            for height in [0usize, 1, 2, 3, 6, 7, 24, 80] {
                for cursor in 0..n {
                    let (offset, end) = visible_window(cursor, height, n);
                    assert!(offset <= end, "offset>end n={n} h={height} c={cursor}");
                    assert!(end <= n, "end>n n={n} h={height} c={cursor}");
                    assert!(end - offset <= height, "window taller than viewport");
                    assert!(offset <= cursor, "offset>cursor n={n} h={height} c={cursor}");
                    if height > 0 {
                        assert!(cursor < end, "cursor>=end n={n} h={height} c={cursor}");
                        assert!(cursor - offset < height, "select index out of view");
                    }
                }
            }
        }
    }

    #[test]
    fn window_fills_viewport_when_content_allows() {
        for cursor in 0..3000 {
            let (offset, end) = visible_window(cursor, 80, 3000);
            assert_eq!(end - offset, 80, "cursor={cursor}");
        }
    }

    #[test]
    fn no_scrolling_when_everything_fits() {
        for cursor in 0..10 {
            assert_eq!(visible_window(cursor, 40, 10), (0, 10));
        }
    }

    #[test]
    fn keeps_scrolloff_below_cursor_until_the_end() {
        let (n, h) = (3000, 40);
        // Mid-list: cursor sits SCROLLOFF above the window bottom.
        let (offset, end) = visible_window(500, h, n);
        assert_eq!(end - 500 - 1, SCROLLOFF);
        assert_eq!(offset, 500 + SCROLLOFF + 1 - h);
        // At the end, stop rather than scroll past the last item.
        let (offset, end) = visible_window(n - 1, h, n);
        assert_eq!((offset, end), (n - h, n));
    }

    #[test]
    fn window_is_bounded_by_viewport_not_item_count() {
        // Work per frame scales with the viewport, not the list.
        let (offset, end) = visible_window(150_000, 30, 1_000_000);
        assert_eq!(end - offset, 30);
    }
}

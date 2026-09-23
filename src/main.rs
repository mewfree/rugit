use std::io;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tui_textarea::{CursorMove, TextArea};

mod app;
mod backend;
mod config;
mod diff;
mod keybindings;
mod ui;

use app::{ActiveBuffer, App, BranchNameInputState, BranchNameMode, BranchPickerMode, BranchPickerState, CommitPickerState, CommitPreview, EditorIntent, EditorMode, EditorState, FixupMode, PatchOp, StashListState, StatusItem};
use backend::{detect_backend, Backend, BackendKind};
use config::Config;
use keybindings::{key_to_action, Action};

type Term = Terminal<CrosstermBackend<io::Stdout>>;

#[derive(Parser, Debug)]
#[command(name = "rugit", about = "A Magit-inspired git TUI", version)]
struct Cli {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    path: String,

    /// Force backend: git or jj
    #[arg(long, value_enum)]
    backend: Option<BackendKind>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    let backend = match detect_backend(&cli.path, cli.backend, &config) {
        Ok(backend) => backend,
        Err(err) => {
            let cwd = std::fs::canonicalize(&cli.path).unwrap_or_else(|_| cli.path.clone().into());
            eprintln!("rugit: no git repository found in {}", cwd.display());
            eprintln!();
            eprintln!("Run rugit from inside a git repository, or create one here with:");
            eprintln!("  git init");
            if std::env::var("RUGIT_DEBUG").is_ok() {
                eprintln!();
                eprintln!("Details: {err:?}");
            }
            std::process::exit(1);
        }
    };
    let mut app = App::new(backend, config)?;

    // Set up terminal
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        SetCursorStyle::DefaultUserShape,
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

/// Draws the app and, when the commit-message editor is focused, switches the
/// real terminal cursor to a thin bar in Insert mode or a block in Normal
/// mode — matching (neo)vim's cursor behavior.
fn draw(terminal: &mut Term, app: &App) -> Result<()> {
    terminal.draw(|f| ui::render(f, app))?;
    let style = match (&app.buffer, &app.editor) {
        (ActiveBuffer::Editor, Some(editor)) => match editor.mode {
            EditorMode::Insert => SetCursorStyle::SteadyBar,
            EditorMode::Normal | EditorMode::Visual | EditorMode::VisualLine => SetCursorStyle::SteadyBlock,
        },
        _ => SetCursorStyle::DefaultUserShape,
    };
    execute!(terminal.backend_mut(), style)?;
    Ok(())
}

fn run_app(terminal: &mut Term, app: &mut App) -> Result<()> {
    // The screen only changes in response to an event, so block on the next
    // one and redraw only when it could have changed something.
    let mut needs_redraw = true;
    while !app.should_quit {
        if needs_redraw {
            draw(terminal, app)?;
        }
        needs_redraw = match event::read()? {
            // Ignore release/repeat events reported on some platforms.
            Event::Key(key) if key.kind != KeyEventKind::Press => false,
            Event::Key(key) => {
                handle_key(terminal, app, key)?;
                true
            }
            Event::Mouse(mouse) => handle_mouse(app, mouse.kind),
            _ => true, // resize
        };
    }
    Ok(())
}

/// Scroll wheel scrolls the commit preview if open, else moves the cursor.
/// Returns whether anything changed.
fn handle_mouse(app: &mut App, kind: MouseEventKind) -> bool {
    let down = match kind {
        MouseEventKind::ScrollDown => true,
        MouseEventKind::ScrollUp => false,
        _ => return false,
    };
    match app.commit_preview.as_mut() {
        Some(preview) => preview.scroll_by(if down { 3 } else { -3 }),
        None if down => app.move_down(),
        None => app.move_up(),
    }
    true
}

/// Route a keypress to the open popup, the editor, or the active buffer.
fn handle_key(terminal: &mut Term, app: &mut App, key: KeyEvent) -> Result<()> {
    let code = key.code;
    let log_filter_key = app.buffer == ActiveBuffer::Log
        && (code == KeyCode::Char('/') || (code == KeyCode::Esc && app.log_filter.is_some()));

    if app.stash_list.is_some() {
        handle_stash_list_key(app, code);
    } else if app.log_search.is_some() {
        handle_log_search_key(app, code);
    } else if log_filter_key {
        if code == KeyCode::Char('/') {
            app.log_search = Some(String::new());
            app.set_log_filter(Some(String::new()));
            app.status_msg = None;
        } else {
            app.set_log_filter(None);
        }
    } else if app.branch_name_input.is_some() {
        handle_branch_name_key(app, code);
    } else if app.branch_picker.is_some() {
        handle_branch_picker_key(app, code);
    } else if app.commit_picker.is_some() {
        handle_commit_picker_key(app, code);
    } else if app.commit_preview.is_some() {
        handle_preview_key(app, code);
    } else if app.buffer == ActiveBuffer::Editor {
        handle_editor_key(app, key);
    } else if code == KeyCode::Esc && app.visual_anchor.is_some() {
        app.visual_anchor = None;
        app.status_msg = None;
    } else {
        app.status_msg = None;
        // A chord prefix only applies to the next key.
        let action = key_to_action(key, app.pending_key.take());
        handle_action(terminal, app, action)?;
    }
    Ok(())
}

/// j/k movement in a popup list. Returns whether `code` was a movement key.
fn step_cursor(cursor: &mut usize, len: usize, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            if *cursor + 1 < len {
                *cursor += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => *cursor = cursor.saturating_sub(1),
        _ => return false,
    }
    true
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// Show an error in the footer. Git errors can span many lines; the footer has one.
fn show_error(app: &mut App, prefix: &str, err: &anyhow::Error) {
    app.status_msg = Some(format!("{prefix}: {}", first_line(&err.to_string())));
}

fn report(app: &mut App, result: Result<()>) {
    if let Err(e) = result {
        show_error(app, "Error", &e);
    }
}

/// After an operation that changes the repo: refresh and show `done`, or the error.
fn finish(app: &mut App, result: Result<()>, done: impl Into<String>, failed: &str) {
    match result {
        Ok(()) => {
            let _ = app.refresh();
            app.status_msg = Some(done.into());
        }
        Err(e) => show_error(app, failed, &e),
    }
}

fn handle_stash_list_key(app: &mut App, code: KeyCode) {
    let Some(list) = app.stash_list.as_mut() else { return };
    if step_cursor(&mut list.cursor, list.stashes.len(), code) {
        return;
    }
    type StashOp = fn(&dyn Backend, usize) -> Result<()>;
    let (op, done): (StashOp, &str) = match code {
        KeyCode::Char('a') => (|b, i| b.stash_apply(i), "Stash applied"),
        KeyCode::Char('p') => (|b, i| b.stash_pop(i), "Stash popped"),
        KeyCode::Char('d') => (|b, i| b.stash_drop(i), "Stash dropped"),
        KeyCode::Esc => {
            app.stash_list = None;
            app.status_msg = None;
            return;
        }
        _ => return,
    };
    if let Some(list) = app.stash_list.take() {
        let result = op(app.backend.as_ref(), list.stashes[list.cursor].index);
        finish(app, result, done, "Error");
    }
}

/// Typing in the log search prompt filters the log live.
fn handle_log_search_key(app: &mut App, code: KeyCode) {
    let Some(input) = app.log_search.as_mut() else { return };
    match code {
        KeyCode::Esc => {
            app.log_search = None;
            app.set_log_filter(None);
            return;
        }
        // Keep the filter applied; just close the prompt so j/k browse the filtered list.
        KeyCode::Enter => {
            app.log_search = None;
            return;
        }
        KeyCode::Backspace => { input.pop(); }
        KeyCode::Char(c) => input.push(c),
        _ => return,
    }
    let query = input.clone();
    app.set_log_filter(Some(query));
}

fn handle_branch_name_key(app: &mut App, code: KeyCode) {
    let Some(state) = app.branch_name_input.as_mut() else { return };
    match code {
        KeyCode::Esc => {
            app.branch_name_input = None;
            app.status_msg = None;
        }
        KeyCode::Backspace => { state.input.pop(); }
        KeyCode::Char(c) => state.input.push(c),
        KeyCode::Enter => {
            let Some(state) = app.branch_name_input.take() else { return };
            let name = state.input.trim();
            if name.is_empty() {
                app.status_msg = Some("Cancelled: empty name".to_string());
                return;
            }
            let (result, done) = match state.mode {
                BranchNameMode::Create => (app.backend.create_branch(name), format!("Created and switched to '{name}'")),
                BranchNameMode::Rename => (app.backend.rename_branch(&state.original, name), format!("Renamed to '{name}'")),
            };
            finish(app, result, done, "Error");
        }
        _ => {}
    }
}

fn handle_branch_picker_key(app: &mut App, code: KeyCode) {
    let Some(picker) = app.branch_picker.as_mut() else { return };
    if step_cursor(&mut picker.cursor, picker.branches.len(), code) {
        return;
    }
    match code {
        KeyCode::Esc => {
            app.branch_picker = None;
            app.status_msg = None;
        }
        KeyCode::Enter => {
            let Some(picker) = app.branch_picker.take() else { return };
            let Some(branch) = picker.branches.get(picker.cursor) else { return };
            let name = &branch.name;
            let (result, done) = match picker.mode {
                BranchPickerMode::Checkout => (app.backend.checkout_branch(name), format!("Switched to '{name}'")),
                BranchPickerMode::Delete if branch.is_current => (
                    Err(anyhow::anyhow!("Cannot delete the currently checked-out branch")),
                    String::new(),
                ),
                BranchPickerMode::Delete => (app.backend.delete_branch(name), format!("Deleted '{name}'")),
            };
            finish(app, result, done, "Error");
        }
        _ => {}
    }
}

/// Fixup/squash/reword target picker.
fn handle_commit_picker_key(app: &mut App, code: KeyCode) {
    let Some(picker) = app.commit_picker.as_mut() else { return };
    if step_cursor(&mut picker.cursor, picker.commits.len(), code) {
        return;
    }
    match code {
        KeyCode::Esc => {
            app.commit_picker = None;
            app.status_msg = None;
        }
        KeyCode::Enter => {
            let Some(picker) = app.commit_picker.take() else { return };
            let Some(commit) = picker.commits.get(picker.cursor) else { return };
            let hash = &commit.short_hash;
            match picker.mode {
                FixupMode::Reword => {
                    let result = open_reword_editor(app, hash, &commit.summary);
                    report(app, result);
                }
                FixupMode::Fixup => {
                    let result = app.backend.fixup_commit(hash);
                    finish(app, result, format!("Fixed up into {hash}"), "Error");
                }
                FixupMode::Squash => {
                    let result = app.backend.squash_commit(hash);
                    finish(app, result, format!("Squashed into {hash}"), "Error");
                }
            }
        }
        _ => {}
    }
}

fn handle_preview_key(app: &mut App, code: KeyCode) {
    let Some(preview) = app.commit_preview.as_mut() else { return };
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.commit_preview = None,
        KeyCode::Char('j') | KeyCode::Down => preview.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => preview.scroll_by(-1),
        KeyCode::Char('d') => preview.scroll_by(20),
        KeyCode::Char('u') => preview.scroll_by(-20),
        _ => {}
    }
}

fn handle_action(terminal: &mut Term, app: &mut App, action: Action) -> Result<()> {
    match action {
        Action::Quit => {
            if matches!(app.buffer, ActiveBuffer::Help | ActiveBuffer::Log) {
                app.buffer = ActiveBuffer::Status;
                app.cursor = 0;
            } else {
                app.should_quit = true;
            }
        }
        Action::HideHelp => {
            if app.buffer == ActiveBuffer::Help {
                app.buffer = ActiveBuffer::Status;
            }
        }
        Action::MoveDown => app.move_down(),
        Action::MoveUp => app.move_up(),
        Action::PageDown => app.move_page_down(half_page(terminal)?),
        Action::PageUp => app.move_page_up(half_page(terminal)?),
        Action::StageFile => apply_op(app, PatchOp::Stage, App::stage_at_cursor),
        Action::UnstageFile => apply_op(app, PatchOp::Unstage, App::unstage_at_cursor),
        Action::DiscardFile => apply_op(app, PatchOp::Discard, App::discard_at_cursor),
        Action::StageAll => {
            let result = app.stage_all();
            report(app, result);
        }
        Action::UnstageAll => {
            let result = app.unstage_all();
            report(app, result);
        }
        Action::ToggleExpand => app.toggle_expand_at_cursor(),
        Action::SwitchToLog => match app.load_log() {
            Ok(()) => {
                app.buffer = ActiveBuffer::Log;
                app.set_log_filter(None);
            }
            Err(e) => show_error(app, "Error loading log", &e),
        },
        Action::Refresh => {
            if let Err(e) = app.refresh() {
                show_error(app, "Error refreshing", &e);
            }
        }
        Action::ShowHelp => app.buffer = ActiveBuffer::Help,
        Action::Enter => open_commit_preview(app),
        Action::CommitBegin => begin_chord(app, 'c', "c-"),
        Action::CommitConfirm => open_commit_editor(app, EditorIntent::Commit),
        Action::CommitAmendConfirm => open_commit_editor(app, EditorIntent::Amend),
        Action::FixupPick => open_commit_picker(app, FixupMode::Fixup),
        Action::SquashPick => open_commit_picker(app, FixupMode::Squash),
        Action::RewordPick => open_commit_picker(app, FixupMode::Reword),
        Action::PushBegin => begin_chord(app, 'p', "P-"),
        Action::Push => run_remote(terminal, app, "Pushing…", "Pushed.", "Push failed", |b| b.push())?,
        Action::PushForce => run_remote(terminal, app, "Force-pushing…", "Force-pushed.", "Force-push failed", |b| b.push_force_lease())?,
        Action::Pull => run_remote(terminal, app, "Pulling…", "Pulled.", "Pull failed", |b| b.pull())?,
        Action::VisualMode => {
            app.visual_anchor = Some(app.cursor);
            app.status_msg = Some("-- VISUAL --".to_string());
        }
        Action::StashBegin => begin_chord(app, 'z', "z-"),
        Action::StashSave => {
            let result = app.backend.stash();
            finish(app, result, "Stashed changes", "Stash failed");
        }
        Action::StashPop => {
            let result = app.backend.stash_pop(0);
            finish(app, result, "Popped stash", "Stash pop failed");
        }
        Action::StashApply => {
            let result = app.backend.stash_apply(0);
            finish(app, result, "Applied stash", "Stash apply failed");
        }
        Action::StashDrop => {
            let result = app.backend.stash_drop(0);
            finish(app, result, "Dropped stash", "Stash drop failed");
        }
        Action::StashList => match app.backend.stash_list() {
            Ok(stashes) if stashes.is_empty() => app.status_msg = Some("No stashes".to_string()),
            Ok(stashes) => app.stash_list = Some(StashListState { stashes, cursor: 0 }),
            Err(e) => show_error(app, "Error", &e),
        },
        Action::BranchBegin => begin_chord(app, 'b', "b-"),
        Action::BranchCheckout => open_branch_picker(app, BranchPickerMode::Checkout),
        Action::BranchDelete => open_branch_picker(app, BranchPickerMode::Delete),
        Action::BranchCreate => {
            app.branch_name_input = Some(BranchNameInputState {
                input: String::new(),
                mode: BranchNameMode::Create,
                original: String::new(),
            });
        }
        Action::BranchRename => {
            let current = app.status.head.clone().unwrap_or_default();
            app.branch_name_input = Some(BranchNameInputState {
                input: current.clone(),
                mode: BranchNameMode::Rename,
                original: current,
            });
        }
        Action::None => {}
    }
    Ok(())
}

fn half_page(terminal: &Term) -> Result<usize> {
    Ok((terminal.size()?.height as usize / 2).max(1))
}

/// Stage/unstage/discard: the visual selection if active, else the item under the cursor.
fn apply_op(app: &mut App, op: PatchOp, at_cursor: fn(&mut App) -> Result<()>) {
    let result = if app.visual_anchor.is_some() {
        app.apply_visual_selection(op)
    } else {
        at_cursor(app)
    };
    report(app, result);
}

fn begin_chord(app: &mut App, key: char, label: &str) {
    app.pending_key = Some(KeyCode::Char(key));
    app.status_msg = Some(label.to_string());
}

/// Push/pull. Shows `busy` while the (blocking) command runs.
fn run_remote(
    terminal: &mut Term,
    app: &mut App,
    busy: &str,
    done: &str,
    failed: &str,
    op: impl FnOnce(&dyn Backend) -> Result<()>,
) -> Result<()> {
    app.status_msg = Some(busy.to_string());
    draw(terminal, app)?;
    let result = op(app.backend.as_ref());
    // Refresh even on failure: a failed pull can still have changed the worktree.
    let _ = app.refresh();
    match result {
        Ok(()) => app.status_msg = Some(done.to_string()),
        Err(e) => show_error(app, failed, &e),
    }
    Ok(())
}

fn open_commit_preview(app: &mut App) {
    let Some(StatusItem::RecentCommit { info }) = app.items.get(app.cursor) else { return };
    let title = format!("{} {}", info.short_hash, info.summary);
    match app.backend.show_commit(&info.short_hash) {
        Ok(content) => app.commit_preview = Some(CommitPreview { title, content, scroll: 0 }),
        Err(e) => show_error(app, "Error", &e),
    }
}

fn open_commit_picker(app: &mut App, mode: FixupMode) {
    match app.backend.log(app.config.log_limit) {
        Ok(commits) if commits.is_empty() => {
            let what = match mode {
                FixupMode::Fixup => "fixup into",
                FixupMode::Squash => "squash into",
                FixupMode::Reword => "reword",
            };
            app.status_msg = Some(format!("No commits to {what}"));
        }
        Ok(commits) => app.commit_picker = Some(CommitPickerState { commits, cursor: 0, mode }),
        Err(e) => show_error(app, "Error loading commits", &e),
    }
}

fn open_branch_picker(app: &mut App, mode: BranchPickerMode) {
    match app.backend.list_branches() {
        Ok(branches) if branches.is_empty() => app.status_msg = Some("No local branches".to_string()),
        Ok(branches) => {
            // Start on the current branch to check out, or on another one to delete.
            let cursor = branches
                .iter()
                .position(|b| b.is_current == (mode == BranchPickerMode::Checkout))
                .unwrap_or(0);
            app.branch_picker = Some(BranchPickerState { branches, cursor, mode });
        }
        Err(e) => show_error(app, "Error", &e),
    }
}

/// Open the inline editor for a new commit, or to amend HEAD.
fn open_commit_editor(app: &mut App, intent: EditorIntent) {
    let (title, message, prompt, staged_label) = if intent == EditorIntent::Amend {
        (
            "Amend Commit",
            app.backend.head_commit_message().unwrap_or_default(),
            "Amend the commit message above.",
            "Staged changes (will be included in amended commit):",
        )
    } else {
        ("Commit Message", String::new(), "Enter commit message above.", "Staged changes:")
    };
    let comments = [prompt, "Lines starting with # are ignored.", "", staged_label]
        .map(String::from)
        .into_iter()
        .chain(app.status.staged.iter().map(|e| format!("  {} {}", e.kind, e.path)))
        .collect();
    app.open_editor(EditorState::new(title.to_string(), message, comments, intent));
}

/// Open the inline editor to rewrite an earlier commit's message.
fn open_reword_editor(app: &mut App, hash: &str, summary: &str) -> Result<()> {
    if !app.status.staged.is_empty() {
        anyhow::bail!("cannot reword while the index has staged changes");
    }
    let message = app.backend.commit_message(hash)?;
    let comments = vec![
        format!("Reword {hash} {summary}."),
        "Only the message changes. Later commits are replayed.".to_string(),
        "Staged changes are not included.".to_string(),
        "Lines starting with # are ignored.".to_string(),
    ];
    app.open_editor(EditorState::new(
        format!("Reword {hash}"),
        message,
        comments,
        EditorIntent::Reword { hash: hash.to_string() },
    ));
    Ok(())
}

/// Handle a keypress when the inline editor buffer is active.
fn handle_editor_key(app: &mut App, key: KeyEvent) {
    let Some(state) = app.editor.as_mut() else { return };

    // Ctrl-C Ctrl-C (Emacs-style): first press arms, second press saves.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        state.pending_ctrl_c = !state.pending_ctrl_c;
        if !state.pending_ctrl_c {
            save_editor(app);
        }
        return;
    }
    state.pending_ctrl_c = false;

    let mut save = false;
    let mut abort = false;
    let textarea = &mut state.textarea;
    match state.mode {
        EditorMode::Insert => {
            if key.code == KeyCode::Esc {
                state.mode = EditorMode::Normal;
            } else {
                textarea.input(key);
            }
        }
        EditorMode::Normal if state.pending_colon => {
            match key.code {
                KeyCode::Char('w') => return, // wait for 'q'
                KeyCode::Char('q') => save = true,
                _ => {}
            }
            state.pending_colon = false;
        }
        EditorMode::Normal if state.pending_d => {
            state.pending_d = false;
            match key.code {
                KeyCode::Char('d') => {
                    // dd: delete entire line
                    let (row, _) = textarea.cursor();
                    delete_lines(textarea, row, row);
                    state.yank_linewise = true;
                }
                KeyCode::Char('w') => { // dw
                    textarea.delete_next_word();
                    state.yank_linewise = false;
                }
                KeyCode::Char('$') => { // d$
                    textarea.delete_line_by_end();
                    state.yank_linewise = false;
                }
                _ => {} // any other key cancels
            }
        }
        EditorMode::Normal | EditorMode::Visual | EditorMode::VisualLine if state.pending_g => {
            state.pending_g = false;
            if key.code == KeyCode::Char('g') {
                textarea.move_cursor(CursorMove::Top);
            }
        }
        EditorMode::Visual => match key.code {
            KeyCode::Esc | KeyCode::Char('v') => {
                textarea.cancel_selection();
                state.mode = EditorMode::Normal;
            }
            KeyCode::Char('g') => state.pending_g = true,
            KeyCode::Char('y') => {
                let start = select_inclusive(textarea);
                textarea.copy();
                state.yank_linewise = false;
                if let Some((row, col)) = start {
                    textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
                }
                state.mode = EditorMode::Normal;
            }
            KeyCode::Char('d') | KeyCode::Char('x') => {
                select_inclusive(textarea);
                textarea.cut();
                state.yank_linewise = false;
                state.mode = EditorMode::Normal;
            }
            KeyCode::Char('c') => {
                select_inclusive(textarea);
                textarea.cut();
                state.yank_linewise = false;
                state.mode = EditorMode::Insert;
            }
            _ => editor_motion(textarea, key.code),
        },
        EditorMode::VisualLine => {
            let (row, _) = textarea.cursor();
            let first = state.visual_line_anchor.min(row);
            let last = state.visual_line_anchor.max(row);
            match key.code {
                KeyCode::Esc | KeyCode::Char('V') => state.mode = EditorMode::Normal,
                KeyCode::Char('g') => state.pending_g = true,
                KeyCode::Char('y') => {
                    let text = textarea.lines()[first..=last].join("\n") + "\n";
                    textarea.set_yank_text(text);
                    textarea.move_cursor(CursorMove::Jump(first as u16, 0));
                    state.yank_linewise = true;
                    state.mode = EditorMode::Normal;
                }
                KeyCode::Char('d') | KeyCode::Char('x') => {
                    delete_lines(textarea, first, last);
                    state.yank_linewise = true;
                    state.mode = EditorMode::Normal;
                }
                KeyCode::Char('c') => {
                    // Replace the lines with a single empty line and start typing.
                    let text = textarea.lines()[first..=last].join("\n") + "\n";
                    let end_col = textarea.lines()[last].chars().count();
                    select_range(textarea, (first, 0), (last, end_col));
                    textarea.cut();
                    textarea.set_yank_text(text);
                    state.yank_linewise = true;
                    state.mode = EditorMode::Insert;
                }
                code => editor_motion(textarea, code),
            }
        }
        EditorMode::Normal => match key.code {
            // Mode transitions
            KeyCode::Char('i') => state.mode = EditorMode::Insert,
            KeyCode::Char('v') => {
                textarea.start_selection();
                state.mode = EditorMode::Visual;
            }
            KeyCode::Char('V') => {
                state.visual_line_anchor = textarea.cursor().0;
                state.mode = EditorMode::VisualLine;
            }
            KeyCode::Char('a') => {
                state.mode = EditorMode::Insert;
                textarea.move_cursor(CursorMove::Forward);
            }
            KeyCode::Char('A') => {
                state.mode = EditorMode::Insert;
                textarea.move_cursor(CursorMove::End);
            }
            KeyCode::Char('o') => {
                textarea.move_cursor(CursorMove::End);
                textarea.insert_newline();
                state.mode = EditorMode::Insert;
            }
            KeyCode::Char('O') => {
                textarea.move_cursor(CursorMove::Head);
                textarea.insert_newline();
                textarea.move_cursor(CursorMove::Up);
                state.mode = EditorMode::Insert;
            }
            KeyCode::Char('g') => state.pending_g = true,
            // Editing
            KeyCode::Char('x') => { textarea.delete_next_char(); }
            KeyCode::Char('d') => state.pending_d = true,
            KeyCode::Char('u') => { textarea.undo(); }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => { textarea.redo(); }
            // Save / abort
            KeyCode::Enter => save = true,
            KeyCode::Char('q') => abort = true,
            KeyCode::Char(':') => state.pending_colon = true,
            // Paste the yank buffer after (p) or at (P) the cursor
            KeyCode::Char('p') if state.yank_linewise => paste_lines(textarea, true),
            KeyCode::Char('P') if state.yank_linewise => paste_lines(textarea, false),
            KeyCode::Char('p') => {
                if !cursor_at_line_end(textarea) {
                    textarea.move_cursor(CursorMove::Forward);
                }
                textarea.paste();
            }
            KeyCode::Char('P') => { textarea.paste(); }
            code => editor_motion(textarea, code),
        },
    }

    refresh_line_highlight(state);

    if save {
        save_editor(app);
    } else if abort {
        app.editor = None;
        app.buffer = ActiveBuffer::Status;
        app.status_msg = Some("Commit aborted".to_string());
    }
}

/// Cursor motions shared by the editor's Normal and Visual modes.
fn editor_motion(textarea: &mut TextArea<'static>, code: KeyCode) {
    let motion = match code {
        KeyCode::Char('h') | KeyCode::Left => CursorMove::Back,
        KeyCode::Char('l') | KeyCode::Right => CursorMove::Forward,
        KeyCode::Char('j') | KeyCode::Down => CursorMove::Down,
        KeyCode::Char('k') | KeyCode::Up => CursorMove::Up,
        KeyCode::Char('w') => CursorMove::WordForward,
        KeyCode::Char('b') => CursorMove::WordBack,
        KeyCode::Char('e') => CursorMove::WordEnd,
        KeyCode::Char('0') => CursorMove::Head,
        KeyCode::Char('$') => CursorMove::End,
        KeyCode::Char('G') => CursorMove::Bottom,
        _ => return,
    };
    textarea.move_cursor(motion);
}

fn cursor_at_line_end(textarea: &TextArea<'static>) -> bool {
    let (row, col) = textarea.cursor();
    textarea.lines().get(row).is_none_or(|line| col >= line.chars().count())
}

/// tui-textarea selections exclude the cell at their end; Vim's visual mode
/// includes both the anchor and the cursor cell. Re-anchor the selection so it
/// covers the same span inclusively. Returns the selection start.
fn select_inclusive(textarea: &mut TextArea<'static>) -> Option<(usize, usize)> {
    let (start, end) = textarea.selection_range()?;
    textarea.cancel_selection();
    textarea.move_cursor(CursorMove::Jump(start.0 as u16, start.1 as u16));
    textarea.start_selection();
    textarea.move_cursor(CursorMove::Jump(end.0 as u16, end.1 as u16));
    if !cursor_at_line_end(textarea) {
        textarea.move_cursor(CursorMove::Forward);
    }
    Some(start)
}

/// Select the text between two (row, char column) positions.
fn select_range(textarea: &mut TextArea<'static>, start: (usize, usize), end: (usize, usize)) {
    textarea.cancel_selection();
    textarea.move_cursor(CursorMove::Jump(start.0 as u16, start.1 as u16));
    textarea.start_selection();
    textarea.move_cursor(CursorMove::Jump(end.0 as u16, end.1 as u16));
}

/// Delete rows `first..=last` (with their line breaks) into the yank buffer as
/// linewise text, leaving the cursor at the start of the line that took their place.
fn delete_lines(textarea: &mut TextArea<'static>, first: usize, last: usize) {
    let lines = textarea.lines();
    let text = lines[first..=last].join("\n") + "\n";
    let width = |row: usize| lines[row].chars().count();
    let (start, end) = if last + 1 < lines.len() {
        ((first, 0), (last + 1, 0))
    } else if first > 0 {
        // Last line of the buffer: take the preceding line break instead.
        ((first - 1, width(first - 1)), (last, width(last)))
    } else {
        ((0, 0), (last, width(last)))
    };
    select_range(textarea, start, end);
    textarea.cut();
    textarea.set_yank_text(text);
    textarea.move_cursor(CursorMove::Jump(first as u16, 0));
}

/// Paste linewise yank text as whole lines below (`p`) or above (`P`) the cursor line.
fn paste_lines(textarea: &mut TextArea<'static>, below: bool) {
    let text = textarea.yank_text();
    let body = text.strip_suffix('\n').unwrap_or(&text);
    let (row, _) = textarea.cursor();
    if below {
        textarea.move_cursor(CursorMove::End);
        textarea.insert_newline();
        textarea.insert_str(body);
        textarea.move_cursor(CursorMove::Jump((row + 1) as u16, 0));
    } else {
        textarea.move_cursor(CursorMove::Head);
        textarea.insert_str(body);
        textarea.insert_newline();
        textarea.move_cursor(CursorMove::Jump(row as u16, 0));
    }
}

/// Highlight the rows covered by linewise visual mode (tui-textarea's own
/// selection is charwise and tied to the cursor, so it can't express this).
fn refresh_line_highlight(state: &mut EditorState) {
    state.textarea.clear_custom_highlight();
    if state.mode != EditorMode::VisualLine {
        return;
    }
    let (row, _) = state.textarea.cursor();
    let first = state.visual_line_anchor.min(row);
    let last = state.visual_line_anchor.max(row);
    let end = state.textarea.lines()[last].len();
    state.textarea.custom_highlight(
        ((first, 0), (last, end)),
        ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
        1,
    );
}

fn save_editor(app: &mut App) {
    let Some(state) = app.editor.take() else { return };
    app.buffer = ActiveBuffer::Status;
    let message = state.message();
    let intent = state.intent;

    if message.is_empty() {
        let what = match intent {
            EditorIntent::Commit => "Commit",
            EditorIntent::Amend => "Amend",
            EditorIntent::Reword { .. } => "Reword",
        };
        app.status_msg = Some(format!("{what} aborted: empty message"));
        return;
    }

    let result = match &intent {
        EditorIntent::Commit => app.backend.commit(&message),
        EditorIntent::Amend => app.backend.amend(&message),
        EditorIntent::Reword { hash } => app.backend.reword_commit(hash, &message),
    };
    let done = match intent {
        EditorIntent::Commit => "Commit created".to_string(),
        EditorIntent::Amend => "Commit amended".to_string(),
        EditorIntent::Reword { hash } => format!("Reworded {hash}"),
    };
    finish(app, result, done, "Error");
}

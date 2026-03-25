use std::io;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod config;
mod keybindings;
mod backend;
mod ui;

use app::{ActiveBuffer, App, EditorState};
use backend::{detect_backend, BackendKind};
use config::Config;
use keybindings::{editor_key_to_action, key_to_action, Action};

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

    let backend = detect_backend(&cli.path, cli.backend, &config)?;
    let mut app = App::new(backend, config)?;

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend_term = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend_term)?;

    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(std::time::Duration::from_millis(250))? {
            match event::read()? {
            Event::Mouse(mouse) => {
                match mouse.kind {
                    MouseEventKind::ScrollDown => {
                        if let Some((_, _, ref mut scroll)) = app.commit_preview {
                            *scroll = scroll.saturating_add(3);
                        } else {
                            app.move_down();
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if let Some((_, _, ref mut scroll)) = app.commit_preview {
                            *scroll = scroll.saturating_sub(3);
                        } else {
                            app.move_up();
                        }
                    }
                    _ => {}
                }
                if app.should_quit { break; }
            }
            Event::Key(key) => {
                // Only process key press events (ignore release/repeat on some platforms)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Handle commit preview popup — scroll or dismiss
                if app.commit_preview.is_some() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.commit_preview = None,
                        KeyCode::Char('j') | KeyCode::Down => {
                            if let Some((_, _, ref mut scroll)) = app.commit_preview {
                                *scroll = scroll.saturating_add(1);
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if let Some((_, _, ref mut scroll)) = app.commit_preview {
                                *scroll = scroll.saturating_sub(1);
                            }
                        }
                        KeyCode::Char('d') => {
                            if let Some((_, _, ref mut scroll)) = app.commit_preview {
                                *scroll = scroll.saturating_add(20);
                            }
                        }
                        KeyCode::Char('u') => {
                            if let Some((_, _, ref mut scroll)) = app.commit_preview {
                                *scroll = scroll.saturating_sub(20);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Route to inline editor if in editor mode
                if app.buffer == ActiveBuffer::Editor {
                    handle_editor_key(app, key);
                    if app.should_quit {
                        break;
                    }
                    continue;
                }

                // Clear status message on any keypress
                app.status_msg = None;

                let action = key_to_action(key, app.pending_key);

                match action {
                    Action::Quit => {
                        if app.buffer == ActiveBuffer::Help {
                            app.buffer = ActiveBuffer::Status;
                            app.pending_key = None;
                        } else {
                            app.should_quit = true;
                        }
                    }
                    Action::HideHelp => {
                        if app.buffer == ActiveBuffer::Help {
                            app.buffer = ActiveBuffer::Status;
                        }
                        app.pending_key = None;
                    }
                    Action::MoveDown => {
                        app.move_down();
                        app.pending_key = None;
                    }
                    Action::MoveUp => {
                        app.move_up();
                        app.pending_key = None;
                    }
                    Action::StageFile => {
                        if let Err(e) = app.stage_at_cursor() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::UnstageFile => {
                        if let Err(e) = app.unstage_at_cursor() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::DiscardFile => {
                        if let Err(e) = app.discard_at_cursor() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::StageAll => {
                        if let Err(e) = app.stage_all() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::UnstageAll => {
                        if let Err(e) = app.unstage_all() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::ToggleExpand => {
                        if let Err(e) = app.toggle_expand_at_cursor() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::SwitchToLog => {
                        if let Err(e) = app.load_log() {
                            app.status_msg = Some(format!("Error loading log: {}", e));
                        } else {
                            app.buffer = ActiveBuffer::Log;
                            app.cursor = 0;
                        }
                        app.pending_key = None;
                    }
                    Action::SwitchToStatus => {
                        app.buffer = ActiveBuffer::Status;
                        app.cursor = 0;
                        app.pending_key = None;
                    }
                    Action::Refresh => {
                        if let Err(e) = app.refresh() {
                            app.status_msg = Some(format!("Error refreshing: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::ShowHelp => {
                        app.buffer = ActiveBuffer::Help;
                        app.pending_key = None;
                    }
                    Action::Enter => {
                        if let Some(crate::app::StatusItem::RecentCommit { info }) =
                            app.items.get(app.cursor).cloned()
                        {
                            match app.backend.show_commit(&info.short_hash) {
                                Ok(content) => {
                                    app.commit_preview = Some((
                                        format!("{} {}", info.short_hash, info.summary),
                                        content,
                                        0,
                                    ));
                                }
                                Err(e) => {
                                    app.status_msg = Some(format!("Error: {}", e));
                                }
                            }
                        }
                        app.pending_key = None;
                    }
                    Action::CommitBegin => {
                        app.pending_key = Some(crossterm::event::KeyCode::Char('c'));
                        app.status_msg = Some("c-".to_string());
                    }
                    Action::CommitConfirm => {
                        app.pending_key = None;
                        do_commit(app);
                    }
                    Action::CommitAmendConfirm => {
                        app.pending_key = None;
                        if let Err(e) = do_commit_amend(app) {
                            app.status_msg = Some(format!("Amend error: {}", e));
                        }
                    }
                    Action::PushBegin => {
                        app.pending_key = Some(crossterm::event::KeyCode::Char('p'));
                        app.status_msg = Some("P-".to_string());
                    }
                    Action::Push => {
                        app.pending_key = None;
                        app.status_msg = Some("Pushing…".to_string());
                        terminal.draw(|f| ui::render(f, app))?;
                        match app.backend.push() {
                            Ok(_)  => app.status_msg = Some("Pushed.".to_string()),
                            Err(e) => app.status_msg = Some(format!("Push failed: {}", first_line(&e.to_string()))),
                        }
                        let _ = app.refresh();
                    }
                    Action::PushForce => {
                        app.pending_key = None;
                        app.status_msg = Some("Force-pushing…".to_string());
                        terminal.draw(|f| ui::render(f, app))?;
                        match app.backend.push_force_lease() {
                            Ok(_)  => app.status_msg = Some("Force-pushed.".to_string()),
                            Err(e) => app.status_msg = Some(format!("Force-push failed: {}", first_line(&e.to_string()))),
                        }
                        let _ = app.refresh();
                    }
                    Action::Pull => {
                        app.pending_key = None;
                        app.status_msg = Some("Pulling…".to_string());
                        terminal.draw(|f| ui::render(f, app))?;
                        match app.backend.pull() {
                            Ok(_)  => app.status_msg = Some("Pulled.".to_string()),
                            Err(e) => app.status_msg = Some(format!("Pull failed: {}", first_line(&e.to_string()))),
                        }
                        let _ = app.refresh();
                    }
                    Action::None => {
                        // Clear pending key if it doesn't form a valid chord
                        app.pending_key = None;
                    }
                    // Editor actions are handled by the editor subsystem, not here
                    Action::EditorChar(_)
                    | Action::EditorBackspace
                    | Action::EditorNewline
                    | Action::EditorSave
                    | Action::EditorAbort
                    | Action::EditorInsertMode
                    | Action::EditorNormalMode
                    | Action::EditorMoveLeft
                    | Action::EditorMoveRight
                    | Action::EditorMoveUp
                    | Action::EditorMoveDown
                    | Action::EditorDeleteBegin
                    | Action::EditorDeleteLine
                    | Action::EditorDeleteChar
                    | Action::EditorLineStart
                    | Action::EditorWordForward
                    | Action::EditorAppend
                    | Action::EditorAppendEnd => {}
                }

                if app.should_quit {
                    break;
                }
            }
            _ => {}
            }
        }
    }
    Ok(())
}

/// Open the inline TUI commit editor for a new commit.
fn do_commit(app: &mut App) {
    let comments = get_staged_summary(app);
    let mut comment_lines = vec![
        "Enter commit message above.".to_string(),
        "Lines starting with # are ignored.".to_string(),
        String::new(),
        "Staged changes:".to_string(),
    ];
    comment_lines.extend(comments.into_iter().map(|l| format!("  {}", l)));
    app.editor = Some(EditorState::new(
        "Commit Message".to_string(),
        String::new(),
        comment_lines,
        false,
    ));
    app.buffer = ActiveBuffer::Editor;
}

/// Open the inline TUI commit editor for an amend.
fn do_commit_amend(app: &mut App) -> Result<()> {
    let last_message = app.backend.log(1)?
        .into_iter()
        .next()
        .map(|c| c.summary)
        .unwrap_or_default();

    let comments = get_staged_summary(app);
    let mut comment_lines = vec![
        "Amend the commit message above.".to_string(),
        "Lines starting with # are ignored.".to_string(),
        String::new(),
        "Staged changes (will be included in amended commit):".to_string(),
    ];
    comment_lines.extend(comments.into_iter().map(|l| format!("  {}", l)));
    app.editor = Some(EditorState::new(
        "Amend Commit".to_string(),
        last_message,
        comment_lines,
        true,
    ));
    app.buffer = ActiveBuffer::Editor;
    Ok(())
}

/// Handle a keypress when the inline editor buffer is active.
fn handle_editor_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use app::EditorMode;
    use crossterm::event::KeyModifiers;

    // Ctrl-C Ctrl-C (Emacs-style): first press arms, second press saves.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let crossterm::event::KeyCode::Char('c') = key.code {
            if let Some(state) = app.editor.as_mut() {
                if state.pending_ctrl_c {
                    // Second C-c: save/commit
                    state.pending_ctrl_c = false;
                } else {
                    state.pending_ctrl_c = true;
                    return;
                }
            }
            // Fall through to EditorSave logic below
            editor_do_save(app);
            return;
        }
    }

    // Clear transient pending flags on any non-ctrl key
    if let Some(state) = app.editor.as_mut() {
        state.pending_ctrl_c = false;
        // pending_d is reset per-action below (EditorDeleteBegin sets it, all others clear it)
    }

    let action = {
        let state = match app.editor.as_ref() {
            Some(s) => s,
            None => return,
        };
        editor_key_to_action(key, &state.mode, state.pending_colon, state.pending_d)
    };

    match action {
        Action::EditorInsertMode => {
            if let Some(state) = app.editor.as_mut() {
                state.mode = EditorMode::Insert;
                state.pending_colon = false;
            }
        }
        Action::EditorNormalMode => {
            if let Some(state) = app.editor.as_mut() {
                state.mode = EditorMode::Normal;
                state.pending_colon = false;
                state.pending_d = false;
            }
        }
        Action::EditorChar(c) => {
            if let Some(state) = app.editor.as_mut() {
                if state.mode == EditorMode::Normal && c == ':' {
                    state.pending_colon = true;
                } else {
                    let row = state.cursor_row;
                    let col = state.cursor_col;
                    state.lines[row].insert(col, c);
                    state.cursor_col += 1;
                    state.pending_colon = false;
                }
            }
        }
        Action::EditorBackspace => {
            if let Some(state) = app.editor.as_mut() {
                let row = state.cursor_row;
                let col = state.cursor_col;
                if col > 0 {
                    state.lines[row].remove(col - 1);
                    state.cursor_col -= 1;
                } else if row > 0 {
                    let current_line = state.lines.remove(row);
                    let prev_len = state.lines[row - 1].len();
                    state.lines[row - 1].push_str(&current_line);
                    state.cursor_row -= 1;
                    state.cursor_col = prev_len;
                }
            }
        }
        Action::EditorNewline => {
            if let Some(state) = app.editor.as_mut() {
                let row = state.cursor_row;
                let col = state.cursor_col;
                let rest = state.lines[row].split_off(col);
                state.lines.insert(row + 1, rest);
                state.cursor_row += 1;
                state.cursor_col = 0;
            }
        }
        Action::EditorMoveLeft => {
            if let Some(state) = app.editor.as_mut() {
                if state.cursor_col > 0 {
                    state.cursor_col -= 1;
                }
                state.pending_d = false;
            }
        }
        Action::EditorMoveRight => {
            if let Some(state) = app.editor.as_mut() {
                let line_len = state.lines[state.cursor_row].len();
                if state.cursor_col < line_len {
                    state.cursor_col += 1;
                }
                state.pending_d = false;
            }
        }
        Action::EditorMoveUp => {
            if let Some(state) = app.editor.as_mut() {
                if state.cursor_row > 0 {
                    state.cursor_row -= 1;
                    let line_len = state.lines[state.cursor_row].len();
                    state.cursor_col = state.cursor_col.min(line_len);
                }
                state.pending_d = false;
            }
        }
        Action::EditorMoveDown => {
            if let Some(state) = app.editor.as_mut() {
                if state.cursor_row + 1 < state.lines.len() {
                    state.cursor_row += 1;
                    let line_len = state.lines[state.cursor_row].len();
                    state.cursor_col = state.cursor_col.min(line_len);
                }
                state.pending_d = false;
            }
        }
        Action::EditorLineStart => {
            if let Some(state) = app.editor.as_mut() {
                state.cursor_col = 0;
                state.pending_d = false;
            }
        }
        Action::EditorWordForward => {
            if let Some(state) = app.editor.as_mut() {
                let line = &state.lines[state.cursor_row];
                let chars: Vec<char> = line.chars().collect();
                let mut col = state.cursor_col;
                // Skip current word (non-whitespace)
                while col < chars.len() && !chars[col].is_whitespace() {
                    col += 1;
                }
                // Skip whitespace
                while col < chars.len() && chars[col].is_whitespace() {
                    col += 1;
                }
                if col < chars.len() {
                    state.cursor_col = col;
                } else if state.cursor_row + 1 < state.lines.len() {
                    // Move to start of next line
                    state.cursor_row += 1;
                    state.cursor_col = 0;
                }
                state.pending_d = false;
            }
        }
        Action::EditorDeleteChar => {
            if let Some(state) = app.editor.as_mut() {
                let row = state.cursor_row;
                let col = state.cursor_col;
                if col < state.lines[row].len() {
                    state.lines[row].remove(col);
                    // Keep col valid
                    let line_len = state.lines[row].len();
                    if state.cursor_col > 0 && state.cursor_col >= line_len {
                        state.cursor_col = line_len.saturating_sub(1);
                    }
                }
                state.pending_d = false;
            }
        }
        Action::EditorDeleteBegin => {
            if let Some(state) = app.editor.as_mut() {
                state.pending_d = true;
            }
        }
        Action::EditorDeleteLine => {
            if let Some(state) = app.editor.as_mut() {
                if state.lines.len() == 1 {
                    state.lines[0].clear();
                    state.cursor_col = 0;
                } else {
                    state.lines.remove(state.cursor_row);
                    if state.cursor_row >= state.lines.len() {
                        state.cursor_row = state.lines.len() - 1;
                    }
                    let line_len = state.lines[state.cursor_row].len();
                    state.cursor_col = state.cursor_col.min(line_len);
                }
                state.pending_d = false;
            }
        }
        Action::EditorAppend => {
            if let Some(state) = app.editor.as_mut() {
                let line_len = state.lines[state.cursor_row].len();
                if state.cursor_col < line_len {
                    state.cursor_col += 1;
                }
                state.mode = EditorMode::Insert;
                state.pending_d = false;
            }
        }
        Action::EditorAppendEnd => {
            if let Some(state) = app.editor.as_mut() {
                state.cursor_col = state.lines[state.cursor_row].len();
                state.mode = EditorMode::Insert;
                state.pending_d = false;
            }
        }
        Action::EditorSave => {
            editor_do_save(app);
        }
        Action::EditorAbort => {
            app.editor = None;
            app.buffer = ActiveBuffer::Status;
            app.status_msg = Some("Commit aborted".to_string());
        }
        _ => {}
    }
}

fn editor_do_save(app: &mut App) {
    let (message, is_amend) = match app.editor.as_ref() {
        Some(state) => (state.message(), state.is_amend),
        None => return,
    };

    app.editor = None;
    app.buffer = ActiveBuffer::Status;

    if message.is_empty() {
        app.status_msg = Some(if is_amend {
            "Amend aborted: empty message".to_string()
        } else {
            "Commit aborted: empty message".to_string()
        });
        return;
    }

    let result = if is_amend {
        app.backend.amend(&message)
    } else {
        app.backend.commit(&message)
    };

    match result {
        Ok(_) => {
            let _ = app.refresh();
            app.status_msg = Some(if is_amend {
                "Commit amended".to_string()
            } else {
                "Commit created".to_string()
            });
        }
        Err(e) => {
            app.status_msg = Some(format!("Error: {}", e));
        }
    }
}

fn get_staged_summary(app: &App) -> Vec<String> {
    app.status
        .staged
        .iter()
        .map(|e| format!("{} {}", e.kind, e.path))
        .collect()
}


fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

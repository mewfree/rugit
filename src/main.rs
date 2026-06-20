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

use app::{ActiveBuffer, App, CommitPickerState, EditorState, FixupMode, StashListState};
use backend::{detect_backend, BackendKind};
use config::Config;
use keybindings::{key_to_action, Action};

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

                // Handle stash list popup
                if app.stash_list.is_some() {
                    match key.code {
                        KeyCode::Esc => {
                            app.stash_list = None;
                            app.status_msg = None;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if let Some(ref mut sl) = app.stash_list {
                                if sl.cursor + 1 < sl.stashes.len() {
                                    sl.cursor += 1;
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if let Some(ref mut sl) = app.stash_list {
                                if sl.cursor > 0 {
                                    sl.cursor -= 1;
                                }
                            }
                        }
                        KeyCode::Char('a') => {
                            if let Some(sl) = app.stash_list.take() {
                                match app.backend.stash_apply(sl.stashes[sl.cursor].index) {
                                    Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Stash applied".to_string()); }
                                    Err(e) => { app.status_msg = Some(format!("Error: {}", e)); }
                                }
                            }
                        }
                        KeyCode::Char('p') => {
                            if let Some(sl) = app.stash_list.take() {
                                match app.backend.stash_apply(sl.stashes[sl.cursor].index) {
                                    Ok(_) => {
                                        let idx = sl.stashes[sl.cursor].index;
                                        let _ = app.backend.stash_drop(idx);
                                        let _ = app.refresh();
                                        app.status_msg = Some("Stash popped".to_string());
                                    }
                                    Err(e) => { app.status_msg = Some(format!("Error: {}", e)); }
                                }
                            }
                        }
                        KeyCode::Char('d') => {
                            if let Some(sl) = app.stash_list.take() {
                                match app.backend.stash_drop(sl.stashes[sl.cursor].index) {
                                    Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Stash dropped".to_string()); }
                                    Err(e) => { app.status_msg = Some(format!("Error: {}", e)); }
                                }
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

                // Handle commit picker popup (fixup/squash)
                if app.commit_picker.is_some() {
                    match key.code {
                        KeyCode::Esc => {
                            app.commit_picker = None;
                            app.status_msg = None;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if let Some(ref mut picker) = app.commit_picker {
                                if picker.cursor + 1 < picker.commits.len() {
                                    picker.cursor += 1;
                                }
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if let Some(ref mut picker) = app.commit_picker {
                                if picker.cursor > 0 {
                                    picker.cursor -= 1;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(picker) = app.commit_picker.take() {
                                if let Some(commit) = picker.commits.get(picker.cursor) {
                                    let hash = commit.short_hash.clone();
                                    let result = match picker.mode {
                                        FixupMode::Fixup  => app.backend.fixup_commit(&hash),
                                        FixupMode::Squash => app.backend.squash_commit(&hash),
                                    };
                                    match result {
                                        Ok(_) => {
                                            let _ = app.refresh();
                                            app.status_msg = Some(match picker.mode {
                                                FixupMode::Fixup  => format!("Fixup commit created targeting {}", hash),
                                                FixupMode::Squash => format!("Squash commit created targeting {}", hash),
                                            });
                                        }
                                        Err(e) => {
                                            app.status_msg = Some(format!("Error: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
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

                // Esc exits visual mode if active
                if key.code == KeyCode::Esc && app.visual_anchor.is_some() {
                    app.visual_anchor = None;
                    app.status_msg = None;
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
                        if app.visual_anchor.is_some() {
                            if let Err(e) = app.stage_visual_selection() {
                                app.status_msg = Some(format!("Error: {}", e));
                            }
                        } else if let Err(e) = app.stage_at_cursor() {
                            app.status_msg = Some(format!("Error: {}", e));
                        }
                        app.pending_key = None;
                    }
                    Action::UnstageFile => {
                        if app.visual_anchor.is_some() {
                            if let Err(e) = app.unstage_visual_selection() {
                                app.status_msg = Some(format!("Error: {}", e));
                            }
                        } else if let Err(e) = app.unstage_at_cursor() {
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
                    Action::FixupPick => {
                        app.pending_key = None;
                        match app.backend.log(app.config.log_limit) {
                            Ok(commits) if !commits.is_empty() => {
                                app.commit_picker = Some(CommitPickerState {
                                    commits,
                                    cursor: 0,
                                    mode: FixupMode::Fixup,
                                });
                            }
                            Ok(_) => {
                                app.status_msg = Some("No commits to fixup into".to_string());
                            }
                            Err(e) => {
                                app.status_msg = Some(format!("Error loading commits: {}", e));
                            }
                        }
                    }
                    Action::SquashPick => {
                        app.pending_key = None;
                        match app.backend.log(app.config.log_limit) {
                            Ok(commits) if !commits.is_empty() => {
                                app.commit_picker = Some(CommitPickerState {
                                    commits,
                                    cursor: 0,
                                    mode: FixupMode::Squash,
                                });
                            }
                            Ok(_) => {
                                app.status_msg = Some("No commits to squash into".to_string());
                            }
                            Err(e) => {
                                app.status_msg = Some(format!("Error loading commits: {}", e));
                            }
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
                    Action::VisualMode => {
                        app.visual_anchor = Some(app.cursor);
                        app.status_msg = Some("-- VISUAL --".to_string());
                        app.pending_key = None;
                    }
                    Action::StashBegin => {
                        app.pending_key = Some(crossterm::event::KeyCode::Char('z'));
                        app.status_msg = Some("z-".to_string());
                    }
                    Action::StashSave => {
                        app.pending_key = None;
                        match app.backend.stash() {
                            Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Stashed changes".to_string()); }
                            Err(e) => { app.status_msg = Some(format!("Stash failed: {}", first_line(&e.to_string()))); }
                        }
                    }
                    Action::StashPop => {
                        app.pending_key = None;
                        match app.backend.stash_pop() {
                            Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Popped stash".to_string()); }
                            Err(e) => { app.status_msg = Some(format!("Stash pop failed: {}", first_line(&e.to_string()))); }
                        }
                    }
                    Action::StashApply => {
                        app.pending_key = None;
                        match app.backend.stash_apply(0) {
                            Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Applied stash".to_string()); }
                            Err(e) => { app.status_msg = Some(format!("Stash apply failed: {}", first_line(&e.to_string()))); }
                        }
                    }
                    Action::StashDrop => {
                        app.pending_key = None;
                        match app.backend.stash_drop(0) {
                            Ok(_) => { let _ = app.refresh(); app.status_msg = Some("Dropped stash".to_string()); }
                            Err(e) => { app.status_msg = Some(format!("Stash drop failed: {}", first_line(&e.to_string()))); }
                        }
                    }
                    Action::StashList => {
                        app.pending_key = None;
                        match app.backend.stash_list() {
                            Ok(stashes) if stashes.is_empty() => {
                                app.status_msg = Some("No stashes".to_string());
                            }
                            Ok(stashes) => {
                                app.stash_list = Some(StashListState { stashes, cursor: 0 });
                            }
                            Err(e) => {
                                app.status_msg = Some(format!("Error: {}", e));
                            }
                        }
                    }
                    Action::None => {
                        // Clear pending key if it doesn't form a valid chord
                        app.pending_key = None;
                    }
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    // Ctrl-C Ctrl-C (Emacs-style): first press arms, second press saves.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        let fire_save = {
            let Some(state) = app.editor.as_mut() else { return };
            if state.pending_ctrl_c {
                state.pending_ctrl_c = false;
                true
            } else {
                state.pending_ctrl_c = true;
                false
            }
        };
        if fire_save { editor_do_save(app); }
        return;
    }

    if let Some(state) = app.editor.as_mut() {
        state.pending_ctrl_c = false;
    }

    let (fire_save, fire_abort) = {
        let Some(state) = app.editor.as_mut() else { return };
        let mut fire_save = false;
        let mut fire_abort = false;

        match state.mode {
            EditorMode::Insert => {
                if key.code == KeyCode::Esc {
                    state.mode = EditorMode::Normal;
                } else {
                    state.textarea.input(key);
                }
            }
            EditorMode::Normal => {
                if state.pending_colon {
                    match key.code {
                        KeyCode::Char('w') => {} // wait for 'q'
                        KeyCode::Char('q') => { fire_save = true; }
                        _ => {}
                    }
                    if key.code != KeyCode::Char('w') {
                        state.pending_colon = false;
                    }
                } else if state.pending_d {
                    state.pending_d = false;
                    let ctrl = KeyModifiers::CONTROL;
                    let none = KeyModifiers::NONE;
                    match key.code {
                        KeyCode::Char('d') => {
                            // dd: clear line (Home + Ctrl+K)
                            state.textarea.input(KeyEvent::new(KeyCode::Home, none));
                            state.textarea.input(KeyEvent::new(KeyCode::Char('k'), ctrl));
                        }
                        KeyCode::Char('w') => {
                            // dw: Alt+D (kill word forward)
                            state.textarea.input(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
                        }
                        _ => {} // any other key cancels
                    }
                } else {
                    let none = KeyModifiers::NONE;
                    let alt  = KeyModifiers::ALT;
                    let ctrl = KeyModifiers::CONTROL;
                    match key.code {
                        // Mode transitions
                        KeyCode::Char('i') => { state.mode = EditorMode::Insert; }
                        KeyCode::Char('a') => {
                            state.mode = EditorMode::Insert;
                            state.textarea.input(KeyEvent::new(KeyCode::Right, none));
                        }
                        KeyCode::Char('A') => {
                            state.mode = EditorMode::Insert;
                            state.textarea.input(KeyEvent::new(KeyCode::End, none));
                        }
                        // Movements
                        KeyCode::Char('h') | KeyCode::Left  => { state.textarea.input(KeyEvent::new(KeyCode::Left,  none)); }
                        KeyCode::Char('l') | KeyCode::Right => { state.textarea.input(KeyEvent::new(KeyCode::Right, none)); }
                        KeyCode::Char('j') | KeyCode::Down  => { state.textarea.input(KeyEvent::new(KeyCode::Down,  none)); }
                        KeyCode::Char('k') | KeyCode::Up    => { state.textarea.input(KeyEvent::new(KeyCode::Up,    none)); }
                        KeyCode::Char('w') => { state.textarea.input(KeyEvent::new(KeyCode::Char('f'), alt)); }
                        KeyCode::Char('b') => { state.textarea.input(KeyEvent::new(KeyCode::Char('b'), alt)); }
                        KeyCode::Char('0') => { state.textarea.input(KeyEvent::new(KeyCode::Home, none)); }
                        KeyCode::Char('$') => { state.textarea.input(KeyEvent::new(KeyCode::End,  none)); }
                        // Editing
                        KeyCode::Char('x') => { state.textarea.input(KeyEvent::new(KeyCode::Delete, none)); }
                        KeyCode::Char('d') => { state.pending_d = true; }
                        KeyCode::Char('u') => { state.textarea.undo(); }
                        KeyCode::Char('r') if key.modifiers.contains(ctrl) => { state.textarea.redo(); }
                        // Save / abort
                        KeyCode::Enter      => { fire_save = true; }
                        KeyCode::Char('q')  => { fire_abort = true; }
                        KeyCode::Char(':')  => { state.pending_colon = true; }
                        _ => {}
                    }
                }
            }
        }

        (fire_save, fire_abort)
    };

    if fire_save {
        editor_do_save(app);
    } else if fire_abort {
        app.editor = None;
        app.buffer = ActiveBuffer::Status;
        app.status_msg = Some("Commit aborted".to_string());
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

use std::io;
use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod config;
mod keybindings;
mod backend;
mod ui;

use app::{ActiveBuffer, App};
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
            if let Event::Key(key) = event::read()? {
                // Only process key press events (ignore release/repeat on some platforms)
                if key.kind != KeyEventKind::Press {
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
                    Action::CommitBegin => {
                        app.pending_key = Some(crossterm::event::KeyCode::Char('c'));
                        app.status_msg = Some("c-".to_string());
                    }
                    Action::CommitConfirm => {
                        app.pending_key = None;
                        if let Err(e) = do_commit(terminal, app) {
                            app.status_msg = Some(format!("Commit error: {}", e));
                        }
                    }
                    Action::CommitAmendConfirm => {
                        app.pending_key = None;
                        app.status_msg = Some("Amend not yet implemented".to_string());
                    }
                    Action::Push => {
                        app.pending_key = None;
                        match do_git_remote_op(terminal, || app.backend.push()) {
                            Ok(msg) => app.status_msg = Some(msg),
                            Err(e) => app.status_msg = Some(format!("Push error: {}", e)),
                        }
                        let _ = app.refresh();
                    }
                    Action::Pull => {
                        app.pending_key = None;
                        match do_git_remote_op(terminal, || app.backend.pull()) {
                            Ok(msg) => app.status_msg = Some(msg),
                            Err(e) => app.status_msg = Some(format!("Pull error: {}", e)),
                        }
                        let _ = app.refresh();
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
        }
    }
    Ok(())
}

/// Suspend the TUI, run a closure (which may print to stdout/stderr or prompt
/// for credentials), then restore the TUI.  Returns the closure's Result.
fn do_git_remote_op<F>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    f: F,
) -> Result<String>
where
    F: FnOnce() -> Result<String>,
{
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    let result = f();

    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture,
    )?;
    terminal.clear()?;

    result
}

fn do_commit(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    use std::io::Write as _;

    // Build a temp file with diff summary comment
    let tmp = std::env::temp_dir().join(format!("rugit_commit_{}.txt", std::process::id()));

    // Get staged diff summary
    let diff_summary = get_staged_summary(app);

    {
        let mut f = std::fs::File::create(&tmp)?;
        writeln!(f, "")?;
        writeln!(f, "# Enter commit message above.")?;
        writeln!(f, "# Lines starting with # are ignored.")?;
        writeln!(f, "#")?;
        writeln!(f, "# Staged changes:")?;
        for line in &diff_summary {
            writeln!(f, "#   {}", line)?;
        }
    }

    // Suspend TUI
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    // Launch editor
    let editor = app.config.editor();
    let status = std::process::Command::new(&editor)
        .arg(&tmp)
        .status()?;

    // Restore TUI
    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture,
    )?;
    terminal.clear()?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Read the commit message
    let content = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);

    let message: String = content
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(String::from)
        .collect::<Vec<_>>()
        .join("\n");

    let message = message.trim().to_string();

    if message.is_empty() {
        app.status_msg = Some("Commit aborted: empty message".to_string());
        return Ok(());
    }

    app.backend.commit(&message)?;
    app.refresh()?;
    app.status_msg = Some("Commit created".to_string());

    Ok(())
}

fn get_staged_summary(app: &App) -> Vec<String> {
    app.status
        .staged
        .iter()
        .map(|e| format!("{} {}", e.kind, e.path))
        .collect()
}

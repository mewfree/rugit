use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::EditorMode;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    MoveDown,
    MoveUp,
    StageFile,
    UnstageFile,
    StageAll,
    UnstageAll,
    ToggleExpand,
    SwitchToLog,
    SwitchToStatus,
    Refresh,
    ShowHelp,
    HideHelp,
    CommitBegin,  // first 'c' of 'c c'
    CommitConfirm, // second 'c'
    CommitAmendConfirm, // 'a' after 'c'
    Push,
    PushForce,
    Pull,
    // Editor actions
    EditorChar(char),
    EditorBackspace,
    EditorNewline,
    EditorSave,
    EditorAbort,
    EditorInsertMode,
    EditorNormalMode,
    None,
}

pub fn key_to_action(key: KeyEvent, pending: Option<KeyCode>) -> Action {
    // Handle ctrl-c / ctrl-q
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => return Action::Quit,
            _ => {}
        }
    }

    if let Some(pending_key) = pending {
        // We have a pending key — resolve chord
        match pending_key {
            KeyCode::Char('c') => match key.code {
                KeyCode::Char('c') => return Action::CommitConfirm,
                KeyCode::Char('a') => return Action::CommitAmendConfirm,
                _ => return Action::None,
            },
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
        KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
        KeyCode::Char('s') => Action::StageFile,
        KeyCode::Char('u') => Action::UnstageFile,
        KeyCode::Char('S') => Action::StageAll,
        KeyCode::Char('U') => Action::UnstageAll,
        KeyCode::Tab => Action::ToggleExpand,
        KeyCode::Char('l') => Action::SwitchToLog,
        KeyCode::Char('b') => Action::SwitchToStatus,
        KeyCode::Char('g') => Action::Refresh,
        KeyCode::Char('?') => Action::ShowHelp,
        KeyCode::Esc => Action::HideHelp,
        KeyCode::Char('c') => Action::CommitBegin,
        KeyCode::Char('P') => Action::Push,
        KeyCode::Char('p') => Action::PushForce,
        KeyCode::Char('F') => Action::Pull,
        _ => Action::None,
    }
}

pub fn editor_key_to_action(key: KeyEvent, mode: &EditorMode, pending_colon: bool) -> Action {
    // Ctrl-c always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char('c') = key.code {
            return Action::EditorAbort;
        }
    }

    match mode {
        EditorMode::Insert => match key.code {
            KeyCode::Esc => Action::EditorNormalMode,
            KeyCode::Backspace => Action::EditorBackspace,
            KeyCode::Enter => Action::EditorNewline,
            KeyCode::Char(c) => Action::EditorChar(c),
            _ => Action::None,
        },
        EditorMode::Normal => {
            if pending_colon {
                // We already received ':', now waiting for 'w' then 'q'
                match key.code {
                    KeyCode::Char('w') => Action::None, // wait for 'q'
                    KeyCode::Char('q') => Action::EditorSave,
                    KeyCode::Esc => Action::EditorNormalMode, // clears pending_colon
                    _ => Action::EditorNormalMode, // reset state
                }
            } else {
                match key.code {
                    KeyCode::Char('i') => Action::EditorInsertMode,
                    KeyCode::Char('q') => Action::EditorAbort,
                    KeyCode::Enter => Action::EditorSave,
                    KeyCode::Char(':') => Action::EditorChar(':'), // handled via pending_colon in main
                    KeyCode::Esc => Action::None,
                    _ => Action::None,
                }
            }
        }
    }
}

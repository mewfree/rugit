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
    Enter,
    PushBegin,    // 'P' — opens push submenu
    Push,         // 'P p'
    PushForce,    // 'P f'
    Pull,
    DiscardFile,
    // Editor actions
    EditorChar(char),
    EditorBackspace,
    EditorNewline,
    EditorSave,
    EditorAbort,
    EditorInsertMode,
    EditorNormalMode,
    EditorMoveLeft,
    EditorMoveRight,
    EditorMoveUp,
    EditorMoveDown,
    EditorDeleteBegin,  // 'd' — first key of 'dd'
    EditorDeleteLine,   // 'dd'
    EditorDeleteWord,   // 'dw'
    EditorDeleteChar,   // 'x'
    EditorLineStart,    // '0'
    EditorWordForward,  // 'w'
    EditorAppend,       // 'a' — insert after cursor char
    EditorAppendEnd,    // 'A' — insert at end of line
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
            KeyCode::Char('p') => match key.code {
                KeyCode::Char('p') => return Action::Push,
                KeyCode::Char('f') => return Action::PushForce,
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
        KeyCode::Enter => Action::Enter,
        KeyCode::Char('c') => Action::CommitBegin,
        KeyCode::Char('p') => Action::PushBegin,
        KeyCode::Char('F') => Action::Pull,
        KeyCode::Char('x') => Action::DiscardFile,
        _ => Action::None,
    }
}

pub fn editor_key_to_action(key: KeyEvent, mode: &EditorMode, pending_colon: bool, pending_d: bool) -> Action {
    // Ctrl-C is handled by the caller (pending_ctrl_c logic for C-c C-c save).
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::None;
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
                match key.code {
                    KeyCode::Char('w') => Action::None, // wait for 'q'
                    KeyCode::Char('q') => Action::EditorSave,
                    KeyCode::Esc => Action::EditorNormalMode,
                    _ => Action::EditorNormalMode,
                }
            } else if pending_d {
                match key.code {
                    KeyCode::Char('d') => Action::EditorDeleteLine,
                    KeyCode::Char('w') => Action::EditorDeleteWord,
                    _ => Action::EditorNormalMode, // any other key cancels
                }
            } else {
                match key.code {
                    KeyCode::Char('i') => Action::EditorInsertMode,
                    KeyCode::Char('a') => Action::EditorAppend,
                    KeyCode::Char('A') => Action::EditorAppendEnd,
                    KeyCode::Char('h') | KeyCode::Left  => Action::EditorMoveLeft,
                    KeyCode::Char('j') | KeyCode::Down  => Action::EditorMoveDown,
                    KeyCode::Char('k') | KeyCode::Up    => Action::EditorMoveUp,
                    KeyCode::Char('l') | KeyCode::Right => Action::EditorMoveRight,
                    KeyCode::Char('0') => Action::EditorLineStart,
                    KeyCode::Char('w') => Action::EditorWordForward,
                    KeyCode::Char('x') => Action::EditorDeleteChar,
                    KeyCode::Char('d') => Action::EditorDeleteBegin,
                    KeyCode::Char('q') => Action::EditorAbort,
                    KeyCode::Enter => Action::EditorSave,
                    KeyCode::Char(':') => Action::EditorChar(':'),
                    KeyCode::Esc => Action::None,
                    _ => Action::None,
                }
            }
        }
    }
}

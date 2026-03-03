use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    Pull,
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
        KeyCode::Char('F') => Action::Pull,
        _ => Action::None,
    }
}

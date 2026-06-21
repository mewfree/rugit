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
    Refresh,
    ShowHelp,
    HideHelp,
    CommitBegin,  // first 'c' of 'c c'
    CommitConfirm, // second 'c'
    CommitAmendConfirm, // 'a' after 'c'
    FixupPick,    // 'c F' — open commit picker for fixup
    SquashPick,   // 'c s' — open commit picker for squash
    Enter,
    PushBegin,    // 'P' — opens push submenu
    Push,         // 'P p'
    PushForce,    // 'P f'
    Pull,
    DiscardFile,
    VisualMode,
    StashBegin,   // 'z' — opens stash submenu
    StashSave,    // 'z z'
    StashPop,     // 'z p'
    StashApply,   // 'z a'
    StashDrop,    // 'z d'
    StashList,    // 'z l'
    BranchBegin,     // 'b' — opens branch submenu
    BranchCheckout,  // 'b b'
    BranchCreate,    // 'b c'
    BranchDelete,    // 'b d'
    BranchRename,    // 'b r'
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
                KeyCode::Char('F') => return Action::FixupPick,
                KeyCode::Char('s') => return Action::SquashPick,
                _ => return Action::None,
            },
            KeyCode::Char('p') => match key.code {
                KeyCode::Char('p') => return Action::Push,
                KeyCode::Char('f') => return Action::PushForce,
                _ => return Action::None,
            },
            KeyCode::Char('z') => match key.code {
                KeyCode::Char('z') => return Action::StashSave,
                KeyCode::Char('p') => return Action::StashPop,
                KeyCode::Char('a') => return Action::StashApply,
                KeyCode::Char('d') => return Action::StashDrop,
                KeyCode::Char('l') => return Action::StashList,
                _ => return Action::None,
            },
            KeyCode::Char('b') => match key.code {
                KeyCode::Char('b') => return Action::BranchCheckout,
                KeyCode::Char('c') => return Action::BranchCreate,
                KeyCode::Char('d') => return Action::BranchDelete,
                KeyCode::Char('r') => return Action::BranchRename,
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
        KeyCode::Char('b') => Action::BranchBegin,
        KeyCode::Char('g') => Action::Refresh,
        KeyCode::Char('?') => Action::ShowHelp,
        KeyCode::Esc => Action::HideHelp,
        KeyCode::Enter => Action::Enter,
        KeyCode::Char('c') => Action::CommitBegin,
        KeyCode::Char('p') => Action::PushBegin,
        KeyCode::Char('F') => Action::Pull,
        KeyCode::Char('x') => Action::DiscardFile,
        KeyCode::Char('V') => Action::VisualMode,
        KeyCode::Char('z') => Action::StashBegin,
        _ => Action::None,
    }
}

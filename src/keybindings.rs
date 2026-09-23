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
    RewordPick,   // 'c w' — open commit picker to reword a message
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
    PageDown,        // Ctrl-d
    PageUp,          // Ctrl-u
    None,
}

pub fn key_to_action(key: KeyEvent, pending: Option<KeyCode>) -> Action {
    // Handle ctrl-c / ctrl-q / ctrl-d / ctrl-u
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('d') => return Action::PageDown,
            KeyCode::Char('u') => return Action::PageUp,
            _ => {}
        }
    }

    // Second key of a chord; anything unrecognised cancels it.
    if let Some(KeyCode::Char(prefix @ ('c' | 'p' | 'z' | 'b'))) = pending {
        let KeyCode::Char(second) = key.code else { return Action::None };
        return match (prefix, second) {
            ('c', 'c') => Action::CommitConfirm,
            ('c', 'a') => Action::CommitAmendConfirm,
            ('c', 'F') => Action::FixupPick,
            ('c', 's') => Action::SquashPick,
            ('c', 'w') => Action::RewordPick,
            ('p', 'p') => Action::Push,
            ('p', 'f') => Action::PushForce,
            ('z', 'z') => Action::StashSave,
            ('z', 'p') => Action::StashPop,
            ('z', 'a') => Action::StashApply,
            ('z', 'd') => Action::StashDrop,
            ('z', 'l') => Action::StashList,
            ('b', 'b') => Action::BranchCheckout,
            ('b', 'c') => Action::BranchCreate,
            ('b', 'd') => Action::BranchDelete,
            ('b', 'r') => Action::BranchRename,
            _ => Action::None,
        };
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

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
    SquashPick,   // 'c S' — open commit picker for squash
    RebaseBegin,  // 'r' — opens rebase submenu
    RewordPick,   // 'r w' — open commit picker to reword a message
    DropPick,     // 'r k' — open commit picker to remove a commit
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
    CommandPalette,  // ':' or Alt-x
    PageDown,        // Ctrl-d
    PageUp,          // Ctrl-u
    None,
}

/// An entry in the command palette: a runnable action and its keybinding.
pub struct Command {
    pub name: &'static str,
    pub keys: &'static str,
    pub action: Action,
}

const fn cmd(name: &'static str, keys: &'static str, action: Action) -> Command {
    Command { name, keys, action }
}

/// Every action worth running by name. Chord prefixes are left out: the palette
/// lists what a chord resolves to.
pub const COMMANDS: &[Command] = &[
    cmd("Stage file / hunk at point", "s", Action::StageFile),
    cmd("Unstage file / hunk at point", "u", Action::UnstageFile),
    cmd("Discard changes at point", "x", Action::DiscardFile),
    cmd("Stage all changes", "S", Action::StageAll),
    cmd("Unstage all changes", "U", Action::UnstageAll),
    cmd("Toggle diff expansion", "Tab", Action::ToggleExpand),
    cmd("Visual selection mode", "V", Action::VisualMode),
    cmd("Commit staged changes", "c c", Action::CommitConfirm),
    cmd("Amend last commit", "c a", Action::CommitAmendConfirm),
    cmd("Instant fixup into a commit", "c F", Action::FixupPick),
    cmd("Instant squash into a commit", "c S", Action::SquashPick),
    cmd("Reword a commit's message", "r w", Action::RewordPick),
    cmd("Remove a commit", "r k", Action::DropPick),
    cmd("Push to upstream", "p p", Action::Push),
    cmd("Force-push (--force-with-lease)", "p f", Action::PushForce),
    cmd("Pull from upstream", "F", Action::Pull),
    cmd("Stash changes", "z z", Action::StashSave),
    cmd("Pop latest stash", "z p", Action::StashPop),
    cmd("Apply latest stash", "z a", Action::StashApply),
    cmd("Drop latest stash", "z d", Action::StashDrop),
    cmd("List stashes", "z l", Action::StashList),
    cmd("Checkout branch", "b b", Action::BranchCheckout),
    cmd("Create & checkout new branch", "b c", Action::BranchCreate),
    cmd("Delete branch", "b d", Action::BranchDelete),
    cmd("Rename current branch", "b r", Action::BranchRename),
    cmd("Show log", "l", Action::SwitchToLog),
    cmd("Refresh", "g", Action::Refresh),
    cmd("Show help", "?", Action::ShowHelp),
    cmd("Quit", "q", Action::Quit),
];

pub fn key_to_action(key: KeyEvent, pending: Option<KeyCode>) -> Action {
    if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('x') {
        return Action::CommandPalette;
    }

    // Handle ctrl-c / ctrl-q / ctrl-d / ctrl-u
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c' | 'q') => return Action::Quit,
            KeyCode::Char('d') => return Action::PageDown,
            KeyCode::Char('u') => return Action::PageUp,
            _ => {}
        }
    }

    // Second key of a chord; anything unrecognised cancels it.
    if let Some(KeyCode::Char(prefix @ ('c' | 'p' | 'z' | 'b' | 'r'))) = pending {
        let KeyCode::Char(second) = key.code else { return Action::None };
        return match (prefix, second) {
            ('c', 'c') => Action::CommitConfirm,
            ('c', 'a') => Action::CommitAmendConfirm,
            ('c', 'F') => Action::FixupPick,
            ('c', 'S') => Action::SquashPick,
            ('r', 'w') => Action::RewordPick,
            ('r', 'k') => Action::DropPick,
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
        KeyCode::Char(':') => Action::CommandPalette,
        KeyCode::Char('?') => Action::ShowHelp,
        KeyCode::Esc => Action::HideHelp,
        KeyCode::Enter => Action::Enter,
        KeyCode::Char('c') => Action::CommitBegin,
        KeyCode::Char('p') => Action::PushBegin,
        KeyCode::Char('F') => Action::Pull,
        KeyCode::Char('x') => Action::DiscardFile,
        KeyCode::Char('V') => Action::VisualMode,
        KeyCode::Char('z') => Action::StashBegin,
        KeyCode::Char('r') => Action::RebaseBegin,
        _ => Action::None,
    }
}

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, Range};
use std::path::Path;
use std::rc::Rc;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui_textarea::TextArea;

use crate::backend::{Backend, BranchInfo, CommitInfo, FileEntry, RepoStatus, StashInfo};
use crate::config::Config;
use crate::diff;

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveBuffer {
    Status,
    Log,
    Help,
    Editor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FixupMode {
    Fixup,
    Squash,
    Reword,
}

/// What saving the commit editor should do.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorIntent {
    Commit,
    Amend,
    /// Rewrite this commit's message and replay the commits after it.
    Reword { hash: String },
}

pub struct CommitPickerState {
    pub commits: Vec<CommitInfo>,
    pub cursor: usize,
    pub mode: FixupMode,
}

pub struct StashListState {
    pub stashes: Vec<StashInfo>,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BranchPickerMode {
    Checkout,
    Delete,
}

pub struct BranchPickerState {
    pub branches: Vec<BranchInfo>,
    pub cursor: usize,
    pub mode: BranchPickerMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BranchNameMode {
    Create,
    Rename,
}

pub struct BranchNameInputState {
    pub input: String,
    pub mode: BranchNameMode,
    pub original: String,
}

pub struct CommitPreview {
    pub title: String,
    pub content: String,
    pub scroll: u16,
}

impl CommitPreview {
    pub fn scroll_by(&mut self, lines: i16) {
        self.scroll = self.scroll.saturating_add_signed(lines);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorMode {
    Normal,
    Insert,
}

pub struct EditorState {
    pub textarea: TextArea<'static>,
    pub mode: EditorMode,
    pub title: String,
    pub comments: Vec<String>,
    pub pending_colon: bool,
    pub pending_ctrl_c: bool,
    pub pending_d: bool,
    pub pending_g: bool,
    pub intent: EditorIntent,
}

impl EditorState {
    pub fn new(title: String, initial_message: String, comments: Vec<String>, intent: EditorIntent) -> Self {
        let lines: Vec<String> = if initial_message.is_empty() {
            vec![String::new()]
        } else {
            initial_message.lines().map(String::from).collect()
        };
        let mut textarea = TextArea::new(lines);
        // Remove default block borders and cursor-line underline
        textarea.set_block(ratatui::widgets::Block::default());
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        // We drive the real terminal cursor ourselves (see ui/editor.rs) so its
        // shape can switch between a block (Normal) and a bar (Insert), like
        // (neo)vim. Disable tui-textarea's own fake highlighted-cell cursor so
        // the two don't overlap.
        textarea.set_cursor_style(ratatui::style::Style::default());
        // Position cursor at end of first line (matching original behavior)
        textarea.move_cursor(tui_textarea::CursorMove::End);
        Self {
            textarea,
            mode: EditorMode::Insert,
            title,
            comments,
            pending_colon: false,
            pending_ctrl_c: false,
            pending_d: false,
            pending_g: false,
            intent,
        }
    }

    /// Returns the commit message (lines joined, trimmed).
    pub fn message(&self) -> String {
        self.textarea.lines().join("\n").trim().to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Staged,
    Unstaged,
    Untracked,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Staged => "Staged Changes",
            Section::Unstaged => "Unstaged Changes",
            Section::Untracked => "Untracked Files",
        }
    }
}

/// Key for a file's expanded state and cached diff.
pub type FileKey = (Section, String);

fn file_key(section: Section, path: &str) -> FileKey {
    (section, path.to_string())
}

/// One line of a cached diff. Shares the diff text rather than copying it,
/// since every stage/unstage rebuilds the item list.
#[derive(Debug, Clone)]
pub struct DiffText {
    diff: Rc<str>,
    range: Range<usize>,
}

impl Deref for DiffText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.diff[self.range.clone()]
    }
}

#[derive(Debug, Clone)]
pub enum StatusItem {
    Header {
        label: &'static str,
        count: usize,
        section: Section,
    },
    File {
        entry: FileEntry,
        section: Section,
        is_expanded: bool,
    },
    HunkHeader {
        line: DiffText,
        hunk_index: usize,
        file_path: Rc<str>,
        section: Section,
    },
    DiffLine {
        line: DiffText,
        file_path: Rc<str>,
        section: Section,
        hunk_index: usize,
        line_in_hunk: usize,
    },
    UnpushedHeader {
        count: usize,
        upstream: String,
    },
    RecentHeader,
    RecentCommit {
        info: CommitInfo,
    },
    StashHeader {
        count: usize,
    },
    StashEntry {
        info: StashInfo,
    },
    Spacer,
}

impl StatusItem {
    /// Unchanged diff lines: the cursor skips them and they can't be staged.
    fn is_context_line(&self) -> bool {
        matches!(self, StatusItem::DiffLine { line, .. } if !diff::is_change(line))
    }
}

/// Moving a hunk or lines between index and worktree via a patch.
#[derive(Debug, Clone, Copy)]
pub enum PatchOp {
    Stage,
    Unstage,
    Discard,
}

impl PatchOp {
    /// The section whose cached diff the patch is cut from.
    fn source(self) -> Section {
        match self {
            PatchOp::Stage | PatchOp::Discard => Section::Unstaged,
            PatchOp::Unstage => Section::Staged,
        }
    }

    /// The section that receives the change, expanded afterwards.
    fn destination(self) -> Section {
        match self {
            PatchOp::Stage => Section::Staged,
            PatchOp::Unstage | PatchOp::Discard => Section::Unstaged,
        }
    }

    /// Whether the patch is applied in reverse (see `diff::lines_patch`).
    fn reverse(self) -> bool {
        !matches!(self, PatchOp::Stage)
    }

    fn verb(self) -> &'static str {
        match self {
            PatchOp::Stage => "Staged",
            PatchOp::Unstage => "Unstaged",
            PatchOp::Discard => "Discarded",
        }
    }

    fn apply(self, backend: &dyn Backend, patch: &str) -> Result<()> {
        match self {
            PatchOp::Stage | PatchOp::Unstage => backend.apply_patch(patch, self.reverse()),
            PatchOp::Discard => backend.discard_patch(patch),
        }
    }
}

/// A selected change line: (file, hunk index, index within the hunk body).
type LineRef = (Rc<str>, usize, usize);

fn remove_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

pub struct App {
    pub backend: Box<dyn Backend>,
    pub config: Config,
    pub buffer: ActiveBuffer,
    pub status: RepoStatus,
    pub log: Vec<CommitInfo>,
    pub cursor: usize,
    pub expanded: HashSet<FileKey>,
    pub diff_cache: HashMap<FileKey, Rc<str>>,
    pub pending_key: Option<KeyCode>,
    pub status_msg: Option<String>,
    pub commit_preview: Option<CommitPreview>,
    pub should_quit: bool,
    pub recent_commits: Vec<CommitInfo>,
    /// Flat list of visible status items (rebuilt on refresh)
    pub items: Vec<StatusItem>,
    pub editor: Option<EditorState>,
    /// Anchor position when visual mode is active (V key)
    pub visual_anchor: Option<usize>,
    pub commit_picker: Option<CommitPickerState>,
    pub stash_list: Option<StashListState>,
    pub stashes: Vec<StashInfo>,
    pub branch_picker: Option<BranchPickerState>,
    pub branch_name_input: Option<BranchNameInputState>,
    /// Text currently being typed into the log search prompt (Some while typing)
    pub log_search: Option<String>,
    /// Active filter query narrowing the log view; set live while typing and
    /// stays applied after confirming so j/k browse the filtered list.
    pub log_filter: Option<String>,
    /// Indices into `log` matching `log_filter`. Set via `set_log_filter`;
    /// recomputed on change rather than every keystroke and frame.
    log_filtered: Vec<usize>,
}

impl App {
    pub fn new(backend: Box<dyn Backend>, config: Config) -> Result<Self> {
        let status = backend.status()?;
        let recent_commits = backend.log(config.recent_limit).unwrap_or_default();
        let stashes = backend.stash_list().unwrap_or_default();
        let mut app = Self {
            backend,
            config,
            buffer: ActiveBuffer::Status,
            status,
            log: Vec::new(),
            cursor: 0,
            expanded: HashSet::new(),
            diff_cache: HashMap::new(),
            pending_key: None,
            status_msg: None,
            commit_preview: None,
            should_quit: false,
            recent_commits,
            items: Vec::new(),
            editor: None,
            visual_anchor: None,
            commit_picker: None,
            stash_list: None,
            stashes,
            branch_picker: None,
            branch_name_input: None,
            log_search: None,
            log_filter: None,
            log_filtered: Vec::new(),
        };
        app.rebuild_items();
        Ok(app)
    }

    pub fn open_editor(&mut self, editor: EditorState) {
        self.editor = Some(editor);
        self.buffer = ActiveBuffer::Editor;
    }

    /// Inclusive (start, end) of the visual selection, if active.
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual_anchor
            .map(|anchor| (anchor.min(self.cursor), anchor.max(self.cursor)))
    }

    fn section_entries(&self, section: Section) -> &[FileEntry] {
        match section {
            Section::Staged => &self.status.staged,
            Section::Unstaged => &self.status.unstaged,
            Section::Untracked => &self.status.untracked,
        }
    }

    /// Rebuild the flat items list from current status + expanded set.
    /// Section order matches Magit: Untracked → Unstaged → Staged.
    pub fn rebuild_items(&mut self) {
        let mut items = vec![StatusItem::Spacer];
        // Spacer between sections, but not before the first.
        let begin_section = |items: &mut Vec<StatusItem>| {
            if items.len() > 1 {
                items.push(StatusItem::Spacer);
            }
        };

        for section in [Section::Untracked, Section::Unstaged, Section::Staged] {
            let entries = self.section_entries(section);
            if entries.is_empty() {
                continue;
            }
            begin_section(&mut items);
            items.push(StatusItem::Header { label: section.label(), count: entries.len(), section });
            for entry in entries {
                let key = file_key(section, &entry.path);
                let is_expanded = self.expanded.contains(&key);
                items.push(StatusItem::File { entry: entry.clone(), section, is_expanded });
                if let Some(diff) = self.diff_cache.get(&key).filter(|_| is_expanded) {
                    push_diff_items(&mut items, diff, entry.path.as_str().into(), section);
                }
            }
        }

        if !self.stashes.is_empty() {
            begin_section(&mut items);
            items.push(StatusItem::StashHeader { count: self.stashes.len() });
            items.extend(self.stashes.iter().map(|info| StatusItem::StashEntry { info: info.clone() }));
        }

        if !self.status.unpushed.is_empty() {
            begin_section(&mut items);
            items.push(StatusItem::UnpushedHeader {
                // `unpushed` is capped; show the real total.
                count: self.status.unpushed_total.max(self.status.unpushed.len()),
                upstream: self.status.upstream.clone().unwrap_or_else(|| "upstream".to_string()),
            });
            items.extend(self.status.unpushed.iter().map(|info| StatusItem::RecentCommit { info: info.clone() }));
        }

        if !self.recent_commits.is_empty() {
            begin_section(&mut items);
            items.push(StatusItem::RecentHeader);
            items.extend(self.recent_commits.iter().map(|info| StatusItem::RecentCommit { info: info.clone() }));
        }

        self.items = items;
    }

    /// Keep the cursor on an item, and off a Spacer.
    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.items.len().saturating_sub(1));
        while self.cursor > 0 && matches!(self.items[self.cursor], StatusItem::Spacer) {
            self.cursor -= 1;
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.status = self.backend.status()?;
        self.recent_commits = self.backend.log(self.config.recent_limit).unwrap_or_default();
        self.stashes = self.backend.stash_list().unwrap_or_default();
        self.rebuild_items();
        self.clamp_cursor();
        Ok(())
    }

    /// Nearest item at or before `i` that isn't a context line; item 0 always counts.
    fn selectable_at_or_before(&self, i: usize) -> usize {
        (1..=i)
            .rev()
            .find(|&j| !self.items[j].is_context_line())
            .unwrap_or(0)
    }

    pub fn move_down(&mut self) {
        match self.buffer {
            ActiveBuffer::Status => {
                if let Some(next) = (self.cursor + 1..self.items.len()).find(|&i| !self.items[i].is_context_line()) {
                    self.cursor = next;
                }
            }
            ActiveBuffer::Log => {
                if self.cursor + 1 < self.log_visible_len() {
                    self.cursor += 1;
                }
            }
            _ => {}
        }
    }

    pub fn move_up(&mut self) {
        match self.buffer {
            ActiveBuffer::Status if self.cursor > 0 => {
                self.cursor = self.selectable_at_or_before(self.cursor - 1);
            }
            ActiveBuffer::Log => self.cursor = self.cursor.saturating_sub(1),
            _ => {}
        }
    }

    pub fn move_page_down(&mut self, amount: usize) {
        match self.buffer {
            ActiveBuffer::Status if !self.items.is_empty() => {
                let target = (self.cursor + amount).min(self.items.len() - 1);
                let selectable = |&i: &usize| !self.items[i].is_context_line();
                // Past a run of context lines at the end, fall back to the last
                // selectable item between the cursor and the target.
                self.cursor = (target..self.items.len())
                    .find(selectable)
                    .or_else(|| (self.cursor + 1..=target).rev().find(selectable))
                    .unwrap_or(self.cursor);
            }
            ActiveBuffer::Log => {
                let len = self.log_visible_len();
                if len > 0 {
                    self.cursor = (self.cursor + amount).min(len - 1);
                }
            }
            _ => {}
        }
    }

    pub fn move_page_up(&mut self, amount: usize) {
        match self.buffer {
            ActiveBuffer::Status if !self.items.is_empty() => {
                self.cursor = self.selectable_at_or_before(self.cursor.saturating_sub(amount));
            }
            ActiveBuffer::Log => self.cursor = self.cursor.saturating_sub(amount),
            _ => {}
        }
    }

    /// Refresh status and diffs for `paths` after a stage/unstage operation.
    /// `destination` is the section that just received the change — it is always
    /// expanded and re-fetched so the user can see the result immediately.
    /// The other section is re-fetched only if it was already expanded.
    fn refresh_file_diffs(&mut self, paths: &[&str], destination: Section) -> Result<()> {
        for path in paths {
            self.diff_cache.remove(&file_key(Section::Staged, path));
            self.diff_cache.remove(&file_key(Section::Unstaged, path));
        }

        // Only the index/worktree moved; no need to re-walk the commit list.
        self.status = self.backend.status()?;

        for path in paths {
            for section in [Section::Staged, Section::Unstaged] {
                self.refetch_diff(section, path, destination);
            }
        }

        self.rebuild_items();
        self.clamp_cursor();
        Ok(())
    }

    fn refetch_diff(&mut self, section: Section, path: &str, destination: Section) {
        let key = file_key(section, path);
        if !self.section_entries(section).iter().any(|e| e.path == path) {
            self.expanded.remove(&key);
            return;
        }
        if section == destination || self.expanded.contains(&key) {
            if let Ok(diff) = self.backend.diff_file(path, section == Section::Staged) {
                self.diff_cache.insert(key.clone(), diff.into());
            }
            self.expanded.insert(key);
        }
    }

    /// Apply `op` to one hunk, cut from the cached diff of its source section.
    fn apply_hunk(&mut self, op: PatchOp, file_path: &str, hunk_index: usize) -> Result<()> {
        let Some(diff) = self.diff_cache.get(&file_key(op.source(), file_path)).cloned() else {
            return Ok(());
        };
        if let Some(patch) = diff::hunk_patch(&diff, hunk_index) {
            op.apply(self.backend.as_ref(), &patch)?;
            self.refresh_file_diffs(&[file_path], op.destination())?;
            self.status_msg = Some(format!("{} hunk {}", op.verb(), hunk_index + 1));
        }
        Ok(())
    }

    /// Apply `op` to the given change lines, one patch per (file, hunk).
    /// Returns how many lines were applied.
    fn apply_lines(&mut self, op: PatchOp, lines: &[LineRef]) -> Result<usize> {
        let mut groups: HashMap<(Rc<str>, usize), HashSet<usize>> = HashMap::new();
        for (file, hunk, line) in lines {
            groups.entry((file.clone(), *hunk)).or_default().insert(*line);
        }

        let mut applied = 0;
        for ((file_path, hunk_index), line_indices) in &groups {
            let Some(diff) = self.diff_cache.get(&file_key(op.source(), file_path)).cloned() else {
                continue;
            };
            if let Some(patch) = diff::lines_patch(&diff, *hunk_index, line_indices, op.reverse()) {
                op.apply(self.backend.as_ref(), &patch)?;
                applied += line_indices.len();
            }
        }

        let mut files: Vec<&str> = groups.keys().map(|(file, _)| &**file).collect();
        files.sort_unstable();
        files.dedup();
        if !files.is_empty() {
            self.refresh_file_diffs(&files, op.destination())?;
        }
        Ok(applied)
    }

    /// Apply `op` to the hunk or change line under the cursor. Other items,
    /// and items not in `op`'s source section, are ignored.
    fn apply_at_cursor(&mut self, op: PatchOp, item: StatusItem) -> Result<()> {
        match item {
            StatusItem::HunkHeader { hunk_index, file_path, section, .. } if section == op.source() => {
                self.apply_hunk(op, &file_path, hunk_index)
            }
            StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk }
                if section == op.source() && diff::is_change(&line) =>
            {
                if self.apply_lines(op, &[(file_path, hunk_index, line_in_hunk)])? > 0 {
                    self.status_msg = Some(format!("{} line", op.verb()));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Apply `op` to every change line in the visual selection, then leave visual mode.
    pub fn apply_visual_selection(&mut self, op: PatchOp) -> Result<()> {
        let Some((start, end)) = self.visual_range() else { return Ok(()) };
        self.visual_anchor = None;

        let lines: Vec<LineRef> = self.items
            .iter()
            .take(end + 1)
            .skip(start)
            .filter_map(|item| match item {
                StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk }
                    if *section == op.source() && diff::is_change(line) =>
                {
                    Some((file_path.clone(), *hunk_index, *line_in_hunk))
                }
                _ => None,
            })
            .collect();
        if lines.is_empty() {
            return Ok(());
        }

        let applied = self.apply_lines(op, &lines)?;
        self.status_msg = Some(format!("{} {applied} line(s)", op.verb()));
        Ok(())
    }

    pub fn stage_at_cursor(&mut self) -> Result<()> {
        let Some(item) = self.items.get(self.cursor).cloned() else { return Ok(()) };
        match item {
            StatusItem::File { entry, section: Section::Staged, .. } => {
                self.status_msg = Some(format!("{} is already staged", entry.path));
            }
            StatusItem::File { entry, .. } => {
                self.backend.stage_file(&entry.path)?;
                self.refresh_file_diffs(&[&entry.path], Section::Staged)?;
                self.status_msg = Some(format!("Staged: {}", entry.path));
            }
            StatusItem::Header { section: section @ (Section::Unstaged | Section::Untracked), .. } => {
                let paths: Vec<String> = self.section_entries(section).iter().map(|e| e.path.clone()).collect();
                self.backend.stage_files(&paths)?;
                self.diff_cache.clear();
                self.refresh()?;
                self.status_msg = Some(match section {
                    Section::Untracked => "Staged all untracked files",
                    _ => "Staged all unstaged changes",
                }.to_string());
            }
            item => self.apply_at_cursor(PatchOp::Stage, item)?,
        }
        Ok(())
    }

    pub fn unstage_at_cursor(&mut self) -> Result<()> {
        let Some(item) = self.items.get(self.cursor).cloned() else { return Ok(()) };
        match item {
            StatusItem::File { entry, section: Section::Staged, .. } => {
                self.backend.unstage_file(&entry.path)?;
                self.refresh_file_diffs(&[&entry.path], Section::Unstaged)?;
                self.status_msg = Some(format!("Unstaged: {}", entry.path));
            }
            StatusItem::Header { section: Section::Staged, .. } => {
                self.backend.unstage_all()?;
                self.diff_cache.clear();
                self.refresh()?;
                self.status_msg = Some("Unstaged all changes".to_string());
            }
            item => self.apply_at_cursor(PatchOp::Unstage, item)?,
        }
        Ok(())
    }

    pub fn discard_at_cursor(&mut self) -> Result<()> {
        let Some(item) = self.items.get(self.cursor).cloned() else { return Ok(()) };
        match item {
            StatusItem::File { entry, section: Section::Untracked, .. } => {
                remove_path(&self.backend.repo_root().join(&entry.path))?;
                self.refresh()?;
                self.status_msg = Some(format!("Deleted: {}", entry.path));
            }
            StatusItem::File { entry, section, .. } => {
                if section == Section::Staged {
                    self.backend.discard_staged_file(&entry.path)?;
                } else {
                    self.backend.discard_file(&entry.path)?;
                }
                self.diff_cache.remove(&file_key(section, &entry.path));
                self.refresh()?;
                self.status_msg = Some(format!("Discarded: {}", entry.path));
            }
            StatusItem::Header { section: Section::Untracked, .. } => {
                let root = self.backend.repo_root();
                for entry in &self.status.untracked {
                    remove_path(&root.join(&entry.path))?;
                }
                self.refresh()?;
                self.status_msg = Some("Deleted all untracked files".to_string());
            }
            StatusItem::Header { section, .. } => {
                if section == Section::Staged {
                    self.backend.discard_all_staged()?;
                } else {
                    self.backend.discard_all_unstaged()?;
                }
                self.diff_cache.clear();
                self.refresh()?;
                self.status_msg = Some(format!(
                    "Discarded all {} changes",
                    if section == Section::Staged { "staged" } else { "unstaged" }
                ));
            }
            StatusItem::HunkHeader { hunk_index, file_path, section: Section::Unstaged, .. } => {
                self.backend.discard_hunk(&file_path, hunk_index)?;
                // Re-fetch the diff so remaining hunks stay visible.
                self.refresh_file_diffs(&[&file_path], Section::Unstaged)?;
                self.status_msg = Some(format!("Discarded hunk {}", hunk_index + 1));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn stage_all(&mut self) -> Result<()> {
        self.backend.stage_all()?;
        self.refresh()?;
        self.status_msg = Some("Staged all changes".to_string());
        Ok(())
    }

    pub fn unstage_all(&mut self) -> Result<()> {
        self.backend.unstage_all()?;
        self.refresh()?;
        self.status_msg = Some("Unstaged all changes".to_string());
        Ok(())
    }

    /// Show or hide the diff of the file under the cursor. Untracked files
    /// have no diff against the index, so they don't expand.
    pub fn toggle_expand_at_cursor(&mut self) {
        let Some(StatusItem::File { entry, section, .. }) = self.items.get(self.cursor) else { return };
        if *section == Section::Untracked {
            return;
        }
        let key = file_key(*section, &entry.path);
        if !self.expanded.remove(&key) {
            if !self.diff_cache.contains_key(&key) {
                match self.backend.diff_file(&key.1, key.0 == Section::Staged) {
                    Ok(diff) => { self.diff_cache.insert(key.clone(), diff.into()); }
                    Err(e) => {
                        self.status_msg = Some(format!("Diff error: {e}"));
                        return;
                    }
                }
            }
            self.expanded.insert(key);
        }
        self.rebuild_items();
    }

    pub fn load_log(&mut self) -> Result<()> {
        self.log = self.backend.log(self.config.log_limit)?;
        self.rebuild_log_filter();
        Ok(())
    }

    /// Set or clear the log filter, recomputing the match list and moving
    /// the cursor back to the top.
    pub fn set_log_filter(&mut self, filter: Option<String>) {
        self.log_filter = filter;
        self.rebuild_log_filter();
        self.cursor = 0;
    }

    /// Match on hash/author/summary, case-insensitive.
    fn rebuild_log_filter(&mut self) {
        self.log_filtered = match self.log_filter.as_deref() {
            Some(query) if !query.is_empty() => {
                let query = query.to_lowercase();
                self.log
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        c.summary.to_lowercase().contains(&query)
                            || c.author.to_lowercase().contains(&query)
                            || c.short_hash.to_lowercase().contains(&query)
                    })
                    .map(|(i, _)| i)
                    .collect()
            }
            _ => (0..self.log.len()).collect(),
        };
    }

    /// Number of commits currently visible in the log buffer.
    pub fn log_visible_len(&self) -> usize {
        self.log_filtered.len()
    }

    /// The commits currently visible in the log buffer, in display order.
    pub fn log_visible(&self) -> impl Iterator<Item = &CommitInfo> + '_ {
        self.log_filtered.iter().filter_map(move |&i| self.log.get(i))
    }
}

/// Push a hunk header or diff line item for each line of `diff`. Lines
/// before the first hunk (the file header) are skipped.
fn push_diff_items(items: &mut Vec<StatusItem>, diff: &Rc<str>, file_path: Rc<str>, section: Section) {
    let mut hunk_index: Option<usize> = None;
    let mut line_in_hunk = 0;
    let mut start = 0;
    for raw in diff.split_inclusive('\n') {
        // Same line ending handling as `str::lines`.
        let text = raw.strip_suffix('\n').map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l));
        let end = start + text.len();
        let line = DiffText { diff: diff.clone(), range: start..end };
        start += raw.len();

        if line.starts_with("@@") {
            let index = hunk_index.map_or(0, |i| i + 1);
            hunk_index = Some(index);
            line_in_hunk = 0;
            items.push(StatusItem::HunkHeader { line, hunk_index: index, file_path: file_path.clone(), section });
        } else if let Some(hunk_index) = hunk_index {
            items.push(StatusItem::DiffLine { line, file_path: file_path.clone(), section, hunk_index, line_in_hunk });
            line_in_hunk += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{App, PatchOp, Section, StatusItem};
    use crate::backend::git::tests::TestRepo;
    use crate::config::Config;
    use std::fs;

    /// A repo where `f` changed on lines 1 and 3 (one hunk), with the diff
    /// expanded and the cursor on its file line.
    fn app_with_expanded_diff() -> (TestRepo, App) {
        let repo = TestRepo::new();
        repo.commit("f", "a\nb\nc\n", "base");
        fs::write(repo.path.join("f"), "A\nb\nC\n").unwrap();
        let mut app = App::new(Box::new(repo.backend()), Config::default()).unwrap();
        app.cursor = find(&app, |item| matches!(item, StatusItem::File { section: Section::Unstaged, .. }));
        app.toggle_expand_at_cursor();
        (repo, app)
    }

    fn find(app: &App, pred: impl Fn(&StatusItem) -> bool) -> usize {
        app.items.iter().position(pred).expect("item not found")
    }

    fn diff_line(app: &App, text: &str) -> usize {
        find(app, |item| matches!(item, StatusItem::DiffLine { line, .. } if &**line == text))
    }

    #[test]
    fn cursor_skips_context_lines() {
        let (_repo, mut app) = app_with_expanded_diff();
        app.cursor = diff_line(&app, "+A");
        app.move_down();
        assert_eq!(app.cursor, diff_line(&app, "-c"), "should skip \" b\"");
        app.move_up();
        assert_eq!(app.cursor, diff_line(&app, "+A"));
    }

    #[test]
    fn visual_selection_stages_only_selected_lines() {
        let (repo, mut app) = app_with_expanded_diff();
        app.visual_anchor = Some(diff_line(&app, "-a"));
        app.cursor = diff_line(&app, "+A");

        app.apply_visual_selection(PatchOp::Stage).unwrap();

        assert_eq!(repo.stdout(&["show", ":f"]), "A\nb\nc\n");
        assert_eq!(app.status_msg.as_deref(), Some("Staged 2 line(s)"));
        assert!(app.visual_anchor.is_none());
        // The staged side is expanded to show the result.
        assert!(app.expanded.contains(&(Section::Staged, "f".to_string())));
    }

    #[test]
    fn stage_then_unstage_single_line_at_cursor() {
        let (repo, mut app) = app_with_expanded_diff();
        app.cursor = diff_line(&app, "+C");
        app.stage_at_cursor().unwrap();
        assert_eq!(repo.stdout(&["show", ":f"]), "a\nb\nc\nC\n");

        app.cursor = find(&app, |item| {
            matches!(item, StatusItem::DiffLine { line, section: Section::Staged, .. } if &**line == "+C")
        });
        app.unstage_at_cursor().unwrap();
        assert_eq!(repo.stdout(&["show", ":f"]), "a\nb\nc\n");
    }

    #[test]
    fn discarding_a_hunk_restores_the_file() {
        let (repo, mut app) = app_with_expanded_diff();
        app.cursor = find(&app, |item| matches!(item, StatusItem::HunkHeader { .. }));
        app.discard_at_cursor().unwrap();
        assert_eq!(fs::read_to_string(repo.path.join("f")).unwrap(), "a\nb\nc\n");
        assert!(app.status.unstaged.is_empty());
    }
}

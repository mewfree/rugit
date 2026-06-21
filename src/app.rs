use std::collections::{HashMap, HashSet};
use anyhow::Result;
use crossterm::event::KeyCode;
use tui_textarea::TextArea;

use crate::backend::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus, StashInfo};

use crate::config::Config;

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
    pub is_amend: bool,
}

impl EditorState {
    pub fn new(title: String, initial_message: String, comments: Vec<String>, is_amend: bool) -> Self {
        let lines: Vec<String> = if initial_message.is_empty() {
            vec![String::new()]
        } else {
            initial_message.lines().map(String::from).collect()
        };
        let mut textarea = TextArea::new(lines);
        // Remove default block borders and cursor-line underline
        textarea.set_block(ratatui::widgets::Block::default());
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        // Position cursor at end of first line (matching original behavior)
        textarea.input(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::End,
            crossterm::event::KeyModifiers::NONE,
        ));
        Self {
            textarea,
            mode: EditorMode::Insert,
            title,
            comments,
            pending_colon: false,
            pending_ctrl_c: false,
            pending_d: false,
            pending_g: false,
            is_amend,
        }
    }

    /// Returns the commit message (lines joined, trimmed).
    pub fn message(&self) -> String {
        self.textarea.lines().join("\n").trim().to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Debug, Clone)]
pub enum StatusItem {
    Header {
        label: String,
        count: usize,
        section: Section,
    },
    File {
        entry: FileEntry,
        section: Section,
        is_expanded: bool,
    },
    HunkHeader {
        line: String,
        hunk_index: usize,
        file_path: String,
        section: Section,
    },
    DiffLine {
        line: String,
        file_path: String,
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

pub struct App {
    pub backend: Box<dyn Backend>,
    pub config: Config,
    pub buffer: ActiveBuffer,
    pub status: RepoStatus,
    pub log: Vec<CommitInfo>,
    pub cursor: usize,
    pub expanded: HashSet<String>,
    pub diff_cache: HashMap<String, String>,
    pub pending_key: Option<KeyCode>,
    pub status_msg: Option<String>,
    pub commit_preview: Option<(String, String, u16)>, // (title, content, scroll)
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
        };
        app.rebuild_items();
        Ok(app)
    }

    /// Rebuild the flat items list from current status + expanded set.
    /// Section order matches Magit: Untracked → Unstaged → Staged.
    pub fn rebuild_items(&mut self) {
        let mut items = vec![StatusItem::Spacer];

        // Untracked section (top)
        if !self.status.untracked.is_empty() {
            items.push(StatusItem::Header {
                label: "Untracked Files".to_string(),
                count: self.status.untracked.len(),
                section: Section::Untracked,
            });
            for entry in &self.status.untracked {
                items.push(StatusItem::File {
                    entry: entry.clone(),
                    section: Section::Untracked,
                    is_expanded: false,
                });
            }
        }

        // Unstaged section (middle)
        if !self.status.unstaged.is_empty() {
            if items.len() > 1 { items.push(StatusItem::Spacer); }
            items.push(StatusItem::Header {
                label: "Unstaged Changes".to_string(),
                count: self.status.unstaged.len(),
                section: Section::Unstaged,
            });
            for entry in &self.status.unstaged {
                let path = entry.path.clone();
                let key = format!("unstaged:{}", path);
                let is_expanded = self.expanded.contains(&key);
                items.push(StatusItem::File {
                    entry: entry.clone(),
                    section: Section::Unstaged,
                    is_expanded,
                });
                if is_expanded {
                    if let Some(diff) = self.diff_cache.get(&format!("unstaged:{}", path)) {
                        let mut hunk_index: usize = 0;
                        let mut seen_first_hunk = false;
                        let mut line_in_hunk: usize = 0;
                        for line in diff.lines() {
                            if line.starts_with("@@") {
                                if seen_first_hunk { hunk_index += 1; }
                                seen_first_hunk = true;
                                line_in_hunk = 0;
                                items.push(StatusItem::HunkHeader {
                                    line: line.to_string(),
                                    hunk_index,
                                    file_path: path.clone(),
                                    section: Section::Unstaged,
                                });
                            } else if seen_first_hunk {
                                items.push(StatusItem::DiffLine {
                                    line: line.to_string(),
                                    file_path: path.clone(),
                                    section: Section::Unstaged,
                                    hunk_index,
                                    line_in_hunk,
                                });
                                line_in_hunk += 1;
                            }
                        }
                    }
                }
            }
        }

        // Staged section (bottom)
        if !self.status.staged.is_empty() {
            if items.len() > 1 { items.push(StatusItem::Spacer); }
            items.push(StatusItem::Header {
                label: "Staged Changes".to_string(),
                count: self.status.staged.len(),
                section: Section::Staged,
            });
            for entry in &self.status.staged {
                let path = entry.path.clone();
                let key = format!("staged:{}", path);
                let is_expanded = self.expanded.contains(&key);
                items.push(StatusItem::File {
                    entry: entry.clone(),
                    section: Section::Staged,
                    is_expanded,
                });
                if is_expanded {
                    if let Some(diff) = self.diff_cache.get(&format!("staged:{}", path)) {
                        let mut hunk_index: usize = 0;
                        let mut seen_first_hunk = false;
                        let mut line_in_hunk: usize = 0;
                        for line in diff.lines() {
                            if line.starts_with("@@") {
                                if seen_first_hunk { hunk_index += 1; }
                                seen_first_hunk = true;
                                line_in_hunk = 0;
                                items.push(StatusItem::HunkHeader {
                                    line: line.to_string(),
                                    hunk_index,
                                    file_path: path.clone(),
                                    section: Section::Staged,
                                });
                            } else if seen_first_hunk {
                                items.push(StatusItem::DiffLine {
                                    line: line.to_string(),
                                    file_path: path.clone(),
                                    section: Section::Staged,
                                    hunk_index,
                                    line_in_hunk,
                                });
                                line_in_hunk += 1;
                            }
                        }
                    }
                }
            }
        }

        // Stash section
        if !self.stashes.is_empty() {
            if items.len() > 1 { items.push(StatusItem::Spacer); }
            items.push(StatusItem::StashHeader { count: self.stashes.len() });
            for info in &self.stashes {
                items.push(StatusItem::StashEntry { info: info.clone() });
            }
        }

        // Unpushed commits section
        if !self.status.unpushed.is_empty() {
            if items.len() > 1 { items.push(StatusItem::Spacer); }
            let upstream = self.status.upstream.clone().unwrap_or_else(|| "upstream".to_string());
            items.push(StatusItem::UnpushedHeader {
                count: self.status.unpushed.len(),
                upstream,
            });
            for info in &self.status.unpushed {
                items.push(StatusItem::RecentCommit { info: info.clone() });
            }
        }

        // Recent commits section
        if !self.recent_commits.is_empty() {
            if items.len() > 1 { items.push(StatusItem::Spacer); }
            items.push(StatusItem::RecentHeader);
            for info in &self.recent_commits {
                items.push(StatusItem::RecentCommit { info: info.clone() });
            }
        }

        self.items = items;
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.status = self.backend.status()?;
        self.recent_commits = self.backend.log(self.config.recent_limit).unwrap_or_default();
        self.stashes = self.backend.stash_list().unwrap_or_default();
        self.rebuild_items();
        // Clamp cursor
        if !self.items.is_empty() && self.cursor >= self.items.len() {
            self.cursor = self.items.len() - 1;
        }
        Ok(())
    }

    pub fn move_down(&mut self) {
        if self.buffer == ActiveBuffer::Status {
            let mut next = self.cursor + 1;
            while next < self.items.len() {
                match &self.items[next] {
                    StatusItem::DiffLine { line, .. }
                        if !line.starts_with('+') && !line.starts_with('-') => { next += 1; }
                    _ => break,
                }
            }
            if next < self.items.len() {
                self.cursor = next;
            }
        } else if self.buffer == ActiveBuffer::Log {
            if !self.log.is_empty() && self.cursor + 1 < self.log.len() {
                self.cursor += 1;
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.buffer == ActiveBuffer::Status {
            if self.cursor == 0 { return; }
            let mut prev = self.cursor - 1;
            while prev > 0 {
                match &self.items[prev] {
                    StatusItem::DiffLine { line, .. }
                        if !line.starts_with('+') && !line.starts_with('-') => { prev -= 1; }
                    _ => break,
                }
            }
            self.cursor = prev;
        } else {
            if self.cursor > 0 { self.cursor -= 1; }
        }
    }

    /// Refresh status and diffs for `file_path` after a stage/unstage operation.
    /// `destination` is the section that just received the change — it is always
    /// expanded and re-fetched so the user can see the result immediately.
    /// The other section is re-fetched only if it was already expanded.
    fn refresh_file_diffs(&mut self, file_path: &str, destination: &Section) -> Result<()> {
        let staged_key   = format!("staged:{}", file_path);
        let unstaged_key = format!("unstaged:{}", file_path);
        self.diff_cache.remove(&staged_key);
        self.diff_cache.remove(&unstaged_key);

        self.status = self.backend.status()?;
        self.recent_commits = self.backend.log(self.config.recent_limit).unwrap_or_default();

        let want_staged   = *destination == Section::Staged
            || self.expanded.contains(&staged_key);
        let want_unstaged = *destination == Section::Unstaged
            || self.expanded.contains(&unstaged_key);

        if self.status.staged.iter().any(|e| e.path == file_path) {
            if want_staged {
                self.expanded.insert(staged_key.clone());
                if let Ok(diff) = self.backend.diff_file(file_path, true) {
                    self.diff_cache.insert(staged_key, diff);
                }
            }
        } else {
            self.expanded.remove(&staged_key);
        }

        if self.status.unstaged.iter().any(|e| e.path == file_path) {
            if want_unstaged {
                self.expanded.insert(unstaged_key.clone());
                if let Ok(diff) = self.backend.diff_file(file_path, false) {
                    self.diff_cache.insert(unstaged_key, diff);
                }
            }
        } else {
            self.expanded.remove(&unstaged_key);
        }

        self.rebuild_items();
        // Clamp then snap cursor off Spacers
        if !self.items.is_empty() {
            if self.cursor >= self.items.len() {
                self.cursor = self.items.len() - 1;
            }
            while self.cursor > 0 && matches!(self.items[self.cursor], StatusItem::Spacer) {
                self.cursor -= 1;
            }
        }
        Ok(())
    }

    /// Parse `@@ -old_start[,count] +new_start[,count] @@` and return (old_start, new_start).
    fn parse_hunk_starts(header: &str) -> Option<(u32, u32)> {
        let inner = header.strip_prefix("@@ ")?;
        let (old_part, rest) = inner.split_once(' ')?;
        let (new_part, _) = rest.split_once(' ')?;
        let old_start: u32 = old_part.trim_start_matches('-').split(',').next()?.parse().ok()?;
        let new_start: u32 = new_part.trim_start_matches('+').split(',').next()?.parse().ok()?;
        Some((old_start, new_start))
    }

    /// Build a minimal patch for one or more `+`/`-` lines within a hunk.
    ///
    /// `reverse=false` (staging): the INDEX contains `-` lines (they haven't been removed yet)
    /// and does NOT contain `+` lines. So `-` lines become context, `+` lines are dropped.
    ///
    /// `reverse=true` (unstaging): the INDEX contains `+` lines (they were staged) and does NOT
    /// contain `-` lines (they were staged for removal). So `+` lines become context, `-` dropped.
    fn extract_lines_patch(diff: &str, hunk_index: usize, line_indices: &HashSet<usize>, reverse: bool) -> Option<String> {
        let lines: Vec<&str> = diff.lines().collect();
        let first_hunk = lines.iter().position(|l| l.starts_with("@@"))?;
        let file_header = lines[..first_hunk].join("\n");

        let hunk_starts: Vec<usize> = lines.iter()
            .enumerate()
            .filter_map(|(i, l)| if l.starts_with("@@") { Some(i) } else { None })
            .collect();

        let hunk_start = *hunk_starts.get(hunk_index)?;
        let hunk_end = hunk_starts.get(hunk_index + 1).copied().unwrap_or(lines.len());
        let hunk_body = &lines[hunk_start + 1..hunk_end];

        let (old_start, new_start) = Self::parse_hunk_starts(lines[hunk_start])?;

        let mut new_body: Vec<String> = Vec::new();
        let mut has_selected = false;
        for (i, &body_line) in hunk_body.iter().enumerate() {
            let ch = body_line.chars().next().unwrap_or(' ');
            if line_indices.contains(&i) && (ch == '+' || ch == '-') {
                new_body.push(body_line.to_string());
                has_selected = true;
            } else if ch == '+' {
                if reverse {
                    new_body.push(format!(" {}", &body_line[1..]));
                }
            } else if ch == '-' {
                if !reverse {
                    new_body.push(format!(" {}", &body_line[1..]));
                }
            } else {
                new_body.push(body_line.to_string());
            }
        }

        if !has_selected {
            return None;
        }

        let old_count = new_body.iter()
            .filter(|l| matches!(l.chars().next(), Some(' ') | Some('-')))
            .count() as u32;
        let new_count = new_body.iter()
            .filter(|l| matches!(l.chars().next(), Some(' ') | Some('+')))
            .count() as u32;

        let mut patch = file_header;
        patch.push('\n');
        patch.push_str(&format!("@@ -{},{} +{},{} @@\n", old_start, old_count, new_start, new_count));
        patch.push_str(&new_body.join("\n"));
        patch.push('\n');
        Some(patch)
    }

    fn extract_line_patch(diff: &str, hunk_index: usize, line_in_hunk: usize, reverse: bool) -> Option<String> {
        let mut set = HashSet::new();
        set.insert(line_in_hunk);
        Self::extract_lines_patch(diff, hunk_index, &set, reverse)
    }

    /// Extract a single hunk from a diff string as a complete patch (file header + hunk body).
    fn extract_hunk_patch(diff: &str, hunk_index: usize) -> Option<String> {
        let lines: Vec<&str> = diff.lines().collect();

        // Collect file header lines (everything before the first @@)
        let first_hunk_start = lines.iter().position(|l| l.starts_with("@@"))?;
        let header: Vec<&str> = lines[..first_hunk_start].to_vec();

        // Find the start of each hunk
        let hunk_starts: Vec<usize> = lines.iter()
            .enumerate()
            .filter_map(|(i, l)| if l.starts_with("@@") { Some(i) } else { None })
            .collect();

        let start = *hunk_starts.get(hunk_index)?;
        let end = hunk_starts.get(hunk_index + 1).copied().unwrap_or(lines.len());

        let mut patch = header.join("\n");
        patch.push('\n');
        patch.push_str(&lines[start..end].join("\n"));
        patch.push('\n');
        Some(patch)
    }

    pub fn stage_at_cursor(&mut self) -> Result<()> {
        if let Some(item) = self.items.get(self.cursor).cloned() {
            match item {
                StatusItem::File { entry, section, .. } => {
                    match section {
                        Section::Unstaged | Section::Untracked => {
                            self.backend.stage_file(&entry.path)?;
                            self.refresh_file_diffs(&entry.path, &Section::Staged)?;
                            self.status_msg = Some(format!("Staged: {}", entry.path));
                        }
                        Section::Staged => {
                            self.status_msg = Some(format!("{} is already staged", entry.path));
                        }
                    }
                }
                StatusItem::Header { section, .. } => {
                    match section {
                        Section::Unstaged => {
                            let paths: Vec<String> = self.status.unstaged.iter().map(|e| e.path.clone()).collect();
                            for path in &paths {
                                self.backend.stage_file(path)?;
                            }
                            self.diff_cache.clear();
                            self.refresh()?;
                            self.status_msg = Some("Staged all unstaged changes".to_string());
                        }
                        Section::Untracked => {
                            let paths: Vec<String> = self.status.untracked.iter().map(|e| e.path.clone()).collect();
                            for path in &paths {
                                self.backend.stage_file(path)?;
                            }
                            self.diff_cache.clear();
                            self.refresh()?;
                            self.status_msg = Some("Staged all untracked files".to_string());
                        }
                        Section::Staged => {}
                    }
                }
                StatusItem::HunkHeader { hunk_index, file_path, section, .. } => {
                    if section == Section::Unstaged {
                        let key = self.file_key(&section, &file_path);
                        if let Some(diff) = self.diff_cache.get(&key).cloned() {
                            if let Some(patch) = Self::extract_hunk_patch(&diff, hunk_index) {
                                self.backend.apply_patch(&patch, false)?;
                                self.refresh_file_diffs(&file_path, &Section::Staged)?;
                                self.status_msg = Some(format!("Staged hunk {}", hunk_index + 1));
                            }
                        }
                    }
                }
                StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk } => {
                    if !line.starts_with('+') && !line.starts_with('-') {
                        return Ok(());
                    }
                    if section == Section::Unstaged {
                        let key = self.file_key(&section, &file_path);
                        if let Some(diff) = self.diff_cache.get(&key).cloned() {
                            if let Some(patch) = Self::extract_line_patch(&diff, hunk_index, line_in_hunk, false) {
                                self.backend.apply_patch(&patch, false)?;
                                self.refresh_file_diffs(&file_path, &Section::Staged)?;
                                self.status_msg = Some("Staged line".to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn unstage_at_cursor(&mut self) -> Result<()> {
        if let Some(item) = self.items.get(self.cursor).cloned() {
            match item {
                StatusItem::File { entry, section, .. } => {
                    if section == Section::Staged {
                        self.backend.unstage_file(&entry.path)?;
                        self.refresh_file_diffs(&entry.path, &Section::Unstaged)?;
                        self.status_msg = Some(format!("Unstaged: {}", entry.path));
                    }
                }
                StatusItem::Header { section, .. } => {
                    if section == Section::Staged {
                        self.backend.unstage_all()?;
                        self.diff_cache.clear();
                        self.refresh()?;
                        self.status_msg = Some("Unstaged all changes".to_string());
                    }
                }
                StatusItem::HunkHeader { hunk_index, file_path, section, .. } => {
                    if section == Section::Staged {
                        let key = self.file_key(&section, &file_path);
                        if let Some(diff) = self.diff_cache.get(&key).cloned() {
                            if let Some(patch) = Self::extract_hunk_patch(&diff, hunk_index) {
                                self.backend.apply_patch(&patch, true)?;
                                self.refresh_file_diffs(&file_path, &Section::Unstaged)?;
                                self.status_msg = Some(format!("Unstaged hunk {}", hunk_index + 1));
                            }
                        }
                    }
                }
                StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk } => {
                    if !line.starts_with('+') && !line.starts_with('-') {
                        return Ok(());
                    }
                    if section == Section::Staged {
                        let key = self.file_key(&section, &file_path);
                        if let Some(diff) = self.diff_cache.get(&key).cloned() {
                            if let Some(patch) = Self::extract_line_patch(&diff, hunk_index, line_in_hunk, true) {
                                self.backend.apply_patch(&patch, true)?;
                                self.refresh_file_diffs(&file_path, &Section::Unstaged)?;
                                self.status_msg = Some("Unstaged line".to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Stage all `+`/`-` DiffLines in the visual selection range.
    pub fn stage_visual_selection(&mut self) -> Result<()> {
        let anchor = match self.visual_anchor.take() {
            Some(a) => a,
            None => return Ok(()),
        };
        let (start, end) = if anchor <= self.cursor { (anchor, self.cursor) } else { (self.cursor, anchor) };

        // Collect selected unstaged diff lines grouped by (file_path, hunk_index)
        let mut groups: HashMap<(String, usize), HashSet<usize>> = HashMap::new();
        let mut any_file: Option<String> = None;

        for i in start..=end {
            if let Some(StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk }) = self.items.get(i) {
                if *section == Section::Unstaged && (line.starts_with('+') || line.starts_with('-')) {
                    groups.entry((file_path.clone(), *hunk_index)).or_default().insert(*line_in_hunk);
                    any_file = Some(file_path.clone());
                }
            }
        }

        if groups.is_empty() {
            return Ok(());
        }

        // Apply patches per (file, hunk) in order, reusing cached diff
        let mut total = 0usize;
        for ((file_path, hunk_index), line_indices) in &groups {
            let cache_key = format!("unstaged:{}", file_path);
            if let Some(diff) = self.diff_cache.get(&cache_key).cloned() {
                if let Some(patch) = Self::extract_lines_patch(&diff, *hunk_index, line_indices, false) {
                    self.backend.apply_patch(&patch, false)?;
                    total += line_indices.len();
                }
            }
        }

        if let Some(path) = any_file {
            self.refresh_file_diffs(&path, &Section::Staged)?;
        }
        self.status_msg = Some(format!("Staged {} line(s)", total));
        Ok(())
    }

    /// Unstage all `+`/`-` DiffLines in the visual selection range.
    pub fn unstage_visual_selection(&mut self) -> Result<()> {
        let anchor = match self.visual_anchor.take() {
            Some(a) => a,
            None => return Ok(()),
        };
        let (start, end) = if anchor <= self.cursor { (anchor, self.cursor) } else { (self.cursor, anchor) };

        let mut groups: HashMap<(String, usize), HashSet<usize>> = HashMap::new();
        let mut any_file: Option<String> = None;

        for i in start..=end {
            if let Some(StatusItem::DiffLine { line, file_path, section, hunk_index, line_in_hunk }) = self.items.get(i) {
                if *section == Section::Staged && (line.starts_with('+') || line.starts_with('-')) {
                    groups.entry((file_path.clone(), *hunk_index)).or_default().insert(*line_in_hunk);
                    any_file = Some(file_path.clone());
                }
            }
        }

        if groups.is_empty() {
            return Ok(());
        }

        let mut total = 0usize;
        for ((file_path, hunk_index), line_indices) in &groups {
            let cache_key = format!("staged:{}", file_path);
            if let Some(diff) = self.diff_cache.get(&cache_key).cloned() {
                if let Some(patch) = Self::extract_lines_patch(&diff, *hunk_index, line_indices, true) {
                    self.backend.apply_patch(&patch, true)?;
                    total += line_indices.len();
                }
            }
        }

        if let Some(path) = any_file {
            self.refresh_file_diffs(&path, &Section::Unstaged)?;
        }
        self.status_msg = Some(format!("Unstaged {} line(s)", total));
        Ok(())
    }

    pub fn discard_at_cursor(&mut self) -> Result<()> {
        if let Some(item) = self.items.get(self.cursor).cloned() {
            match item {
                StatusItem::File { entry, section, .. } => {
                    match section {
                        Section::Unstaged => {
                            self.backend.discard_file(&entry.path)?;
                            self.diff_cache.remove(&self.file_key(&section, &entry.path));
                            self.refresh()?;
                            self.status_msg = Some(format!("Discarded: {}", entry.path));
                        }
                        Section::Untracked => {
                            let full_path = self.backend.repo_root().join(&entry.path);
                            if entry.kind == FileKind::Untracked && full_path.is_dir() {
                                std::fs::remove_dir_all(&full_path)?;
                            } else {
                                std::fs::remove_file(&full_path)?;
                            }
                            self.refresh()?;
                            self.status_msg = Some(format!("Deleted: {}", entry.path));
                        }
                        Section::Staged => {
                            self.status_msg = Some(format!("{} is staged — unstage first", entry.path));
                        }
                    }
                }
                StatusItem::Header { section, .. } => {
                    match section {
                        Section::Unstaged => {
                            self.backend.discard_all_unstaged()?;
                            self.diff_cache.clear();
                            self.refresh()?;
                            self.status_msg = Some("Discarded all unstaged changes".to_string());
                        }
                        Section::Untracked => {
                            let root = self.backend.repo_root().to_path_buf();
                            for entry in self.status.untracked.clone() {
                                let full_path = root.join(&entry.path);
                                if full_path.is_dir() {
                                    std::fs::remove_dir_all(&full_path)?;
                                } else {
                                    std::fs::remove_file(&full_path)?;
                                }
                            }
                            self.refresh()?;
                            self.status_msg = Some("Deleted all untracked files".to_string());
                        }
                        Section::Staged => {
                            self.status_msg = Some("Cannot discard staged changes — unstage first".to_string());
                        }
                    }
                }
                StatusItem::HunkHeader { hunk_index, file_path, section, .. } => {
                    if section == Section::Unstaged {
                        let key = self.file_key(&section, &file_path);
                        self.backend.discard_hunk(&file_path, hunk_index)?;
                        // Re-fetch the diff so remaining hunks stay visible
                        self.status = self.backend.status()?;
                        self.recent_commits = self.backend.log(self.config.recent_limit).unwrap_or_default();
                        if self.status.unstaged.iter().any(|e| e.path == file_path) {
                            if let Ok(new_diff) = self.backend.diff_file(&file_path, false) {
                                self.diff_cache.insert(key, new_diff);
                            }
                        } else {
                            self.diff_cache.remove(&key);
                        }
                        self.rebuild_items();
                        if !self.items.is_empty() && self.cursor >= self.items.len() {
                            self.cursor = self.items.len() - 1;
                        }
                        self.status_msg = Some(format!("Discarded hunk {}", hunk_index + 1));
                    }
                }
                _ => {}
            }
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

    pub fn toggle_expand_at_cursor(&mut self) -> Result<()> {
        let item = match self.items.get(self.cursor).cloned() {
            Some(i) => i,
            None => return Ok(()),
        };

        match item {
            StatusItem::File { entry, section, .. } => {
                let key = self.file_key(&section, &entry.path);
                let staged = section == Section::Staged;
                if self.expanded.contains(&key) {
                    self.expanded.remove(&key);
                } else {
                    if !self.diff_cache.contains_key(&key) {
                        match self.backend.diff_file(&entry.path, staged) {
                            Ok(diff) => { self.diff_cache.insert(key.clone(), diff); }
                            Err(e) => {
                                self.status_msg = Some(format!("Diff error: {}", e));
                                return Ok(());
                            }
                        }
                    }
                    self.expanded.insert(key);
                }
                self.rebuild_items();
            }
            _ => {}
        }
        Ok(())
    }

    fn file_key(&self, section: &Section, path: &str) -> String {
        match section {
            Section::Staged => format!("staged:{}", path),
            Section::Unstaged => format!("unstaged:{}", path),
            Section::Untracked => format!("untracked:{}", path),
        }
    }

    pub fn load_log(&mut self) -> Result<()> {
        self.log = self.backend.log(self.config.log_limit)?;
        Ok(())
    }
}

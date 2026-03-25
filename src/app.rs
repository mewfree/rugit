use std::collections::{HashMap, HashSet};
use anyhow::Result;
use crossterm::event::KeyCode;
use tui_textarea::TextArea;

use crate::backend::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveBuffer {
    Status,
    Log,
    Help,
    Editor,
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
    },
    UnpushedHeader {
        count: usize,
        upstream: String,
    },
    RecentHeader,
    RecentCommit {
        info: CommitInfo,
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
}

impl App {
    pub fn new(backend: Box<dyn Backend>, config: Config) -> Result<Self> {
        let status = backend.status()?;
        let recent_commits = backend.log(5).unwrap_or_default();
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
                        for line in diff.lines() {
                            if line.starts_with("@@") {
                                if seen_first_hunk { hunk_index += 1; }
                                seen_first_hunk = true;
                                items.push(StatusItem::HunkHeader {
                                    line: line.to_string(),
                                    hunk_index,
                                    file_path: path.clone(),
                                    section: Section::Unstaged,
                                });
                            } else {
                                items.push(StatusItem::DiffLine { line: line.to_string() });
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
                        for line in diff.lines() {
                            if line.starts_with("@@") {
                                if seen_first_hunk { hunk_index += 1; }
                                seen_first_hunk = true;
                                items.push(StatusItem::HunkHeader {
                                    line: line.to_string(),
                                    hunk_index,
                                    file_path: path.clone(),
                                    section: Section::Staged,
                                });
                            } else {
                                items.push(StatusItem::DiffLine { line: line.to_string() });
                            }
                        }
                    }
                }
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
        self.recent_commits = self.backend.log(5).unwrap_or_default();
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
                if matches!(self.items[next], StatusItem::DiffLine { .. }) {
                    next += 1;
                } else {
                    break;
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
            while prev > 0 && matches!(self.items[prev], StatusItem::DiffLine { .. }) {
                prev -= 1;
            }
            self.cursor = prev;
        } else {
            if self.cursor > 0 { self.cursor -= 1; }
        }
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
                            self.diff_cache.remove(&self.file_key(&section, &entry.path));
                            self.refresh()?;
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
                                self.diff_cache.remove(&key);
                                self.refresh()?;
                                self.status_msg = Some(format!("Staged hunk {}", hunk_index + 1));
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
                        self.diff_cache.remove(&self.file_key(&section, &entry.path));
                        self.refresh()?;
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
                                self.diff_cache.remove(&key);
                                self.refresh()?;
                                self.status_msg = Some(format!("Unstaged hunk {}", hunk_index + 1));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
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
                        self.recent_commits = self.backend.log(5).unwrap_or_default();
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

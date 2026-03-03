use std::collections::{HashMap, HashSet};
use anyhow::Result;
use crossterm::event::KeyCode;

use crate::backend::{Backend, CommitInfo, FileEntry, RepoStatus};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveBuffer {
    Status,
    Log,
    Help,
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
    Diff {
        lines: Vec<String>,
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
    pub should_quit: bool,
    pub recent_commits: Vec<CommitInfo>,
    /// Flat list of visible status items (rebuilt on refresh)
    pub items: Vec<StatusItem>,
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
            should_quit: false,
            recent_commits,
            items: Vec::new(),
        };
        app.rebuild_items();
        Ok(app)
    }

    /// Rebuild the flat items list from current status + expanded set.
    pub fn rebuild_items(&mut self) {
        let mut items = vec![StatusItem::Spacer];

        // Staged section
        if !self.status.staged.is_empty() {
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
                        let lines: Vec<String> = diff.lines().map(String::from).collect();
                        items.push(StatusItem::Diff { lines });
                    }
                }
            }
        }

        // Unstaged section
        if !self.status.unstaged.is_empty() {
            if !items.is_empty() { items.push(StatusItem::Spacer); }
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
                        let lines: Vec<String> = diff.lines().map(String::from).collect();
                        items.push(StatusItem::Diff { lines });
                    }
                }
            }
        }

        // Untracked section
        if !self.status.untracked.is_empty() {
            if !items.is_empty() { items.push(StatusItem::Spacer); }
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

        // Recent commits section
        if !self.recent_commits.is_empty() {
            if !items.is_empty() { items.push(StatusItem::Spacer); }
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
            if !self.items.is_empty() && self.cursor + 1 < self.items.len() {
                self.cursor += 1;
            }
        } else if self.buffer == ActiveBuffer::Log {
            if !self.log.is_empty() && self.cursor + 1 < self.log.len() {
                self.cursor += 1;
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn stage_at_cursor(&mut self) -> Result<()> {
        if let Some(item) = self.items.get(self.cursor).cloned() {
            match item {
                StatusItem::File { entry, section, .. } => {
                    match section {
                        Section::Unstaged | Section::Untracked => {
                            self.backend.stage_file(&entry.path)?;
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
                        Section::Unstaged | Section::Untracked => {
                            self.backend.stage_all()?;
                            self.refresh()?;
                            self.status_msg = Some("Staged all changes".to_string());
                        }
                        Section::Staged => {}
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
                        self.refresh()?;
                        self.status_msg = Some(format!("Unstaged: {}", entry.path));
                    }
                }
                StatusItem::Header { section, .. } => {
                    if section == Section::Staged {
                        self.backend.unstage_all()?;
                        self.refresh()?;
                        self.status_msg = Some("Unstaged all changes".to_string());
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
            StatusItem::Diff { .. } => {
                // Collapse the parent File item sitting directly above this Diff
                if self.cursor > 0 {
                    if let Some(StatusItem::File { entry, section, .. }) =
                        self.items.get(self.cursor - 1).cloned()
                    {
                        let key = self.file_key(&section, &entry.path);
                        self.expanded.remove(&key);
                        self.cursor -= 1;
                        self.rebuild_items();
                    }
                }
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

use std::path::Path;
use anyhow::Result;
use clap::ValueEnum;
use crate::config::Config;

pub mod git;
pub mod jj;

#[derive(Debug, Clone, PartialEq)]
pub enum FileKind {
    Modified,
    Added,
    Deleted,
    Renamed(String),
    Untracked,
    Conflicted,
}

impl std::fmt::Display for FileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileKind::Modified => write!(f, "modified"),
            FileKind::Added => write!(f, "added"),
            FileKind::Deleted => write!(f, "deleted"),
            FileKind::Renamed(to) => write!(f, "renamed → {}", to),
            FileKind::Untracked => write!(f, "untracked"),
            FileKind::Conflicted => write!(f, "conflicted"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Default)]
pub struct RepoStatus {
    pub head: Option<String>,
    pub head_short_hash: Option<String>,
    pub head_summary: Option<String>,
    pub upstream: Option<String>,
    pub staged: Vec<FileEntry>,
    pub unstaged: Vec<FileEntry>,
    pub untracked: Vec<FileEntry>,
    /// Capped for display; see `unpushed_total` for the real count.
    pub unpushed: Vec<CommitInfo>,
    /// Real count, even when `unpushed` is truncated.
    pub unpushed_total: usize,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub short_hash: String,
    pub summary: String,
    pub author: String,
}

#[derive(Debug, Clone)]
pub struct StashInfo {
    pub index: usize,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

pub trait Backend {
    fn repo_root(&self) -> &Path;
    fn kind_name(&self) -> &'static str;
    fn status(&self) -> Result<RepoStatus>;
    fn diff_file(&self, path: &str, staged: bool) -> Result<String>;
    fn stage_file(&self, path: &str) -> Result<()>;
    /// Override to avoid rewriting the whole index once per path.
    fn stage_files(&self, paths: &[String]) -> Result<()> {
        for path in paths {
            self.stage_file(path)?;
        }
        Ok(())
    }
    fn unstage_file(&self, path: &str) -> Result<()>;
    fn discard_file(&self, path: &str) -> Result<()>;
    fn stage_all(&self) -> Result<()>;
    fn unstage_all(&self) -> Result<()>;
    fn commit(&self, message: &str) -> Result<()>;
    fn amend(&self, message: &str) -> Result<()>;
    fn head_commit_message(&self) -> Result<String>;
    /// Full message of `hash`, including the body.
    fn commit_message(&self, hash: &str) -> Result<String>;
    /// Replace the message of `hash` and replay later commits onto it.
    fn reword_commit(&self, hash: &str, message: &str) -> Result<()>;
    fn log(&self, limit: usize) -> Result<Vec<CommitInfo>>;
    fn push(&self) -> Result<()>;
    fn push_force_lease(&self) -> Result<()>;
    fn pull(&self) -> Result<()>;
    fn show_commit(&self, hash: &str) -> Result<String>;
    fn apply_patch(&self, patch: &str, reverse: bool) -> Result<()>;
    fn discard_patch(&self, patch: &str) -> Result<()>;
    fn discard_hunk(&self, path: &str, hunk_index: usize) -> Result<()>;
    fn discard_all_unstaged(&self) -> Result<()>;
    fn discard_staged_file(&self, path: &str) -> Result<()>;
    fn discard_all_staged(&self) -> Result<()>;
    fn fixup_commit(&self, hash: &str) -> Result<()>;
    fn squash_commit(&self, hash: &str) -> Result<()>;
    fn stash(&self) -> Result<()>;
    fn stash_pop(&self, index: usize) -> Result<()>;
    fn stash_apply(&self, index: usize) -> Result<()>;
    fn stash_drop(&self, index: usize) -> Result<()>;
    fn stash_list(&self) -> Result<Vec<StashInfo>>;
    fn list_branches(&self) -> Result<Vec<BranchInfo>>;
    fn checkout_branch(&self, name: &str) -> Result<()>;
    fn create_branch(&self, name: &str) -> Result<()>;
    fn delete_branch(&self, name: &str) -> Result<()>;
    fn rename_branch(&self, old: &str, new: &str) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum BackendKind {
    Git,
    Jj,
}

/// The nearest ancestor of `start` (inclusive) holding a `.jj` directory.
pub fn find_jj_root(start: &Path) -> Option<&Path> {
    start.ancestors().find(|dir| dir.join(".jj").is_dir())
}

pub fn detect_backend(
    path: &str,
    forced: Option<BackendKind>,
    config: &Config,
) -> Result<Box<dyn Backend>> {
    let path = Path::new(path);

    // CLI arg wins, then config, then auto-detect (prefer jj if a .jj dir exists)
    let kind = forced
        .or_else(|| BackendKind::from_str(config.backend.as_deref()?, true).ok())
        .unwrap_or_else(|| match find_jj_root(path) {
            Some(_) => BackendKind::Jj,
            None => BackendKind::Git,
        });

    Ok(match kind {
        BackendKind::Jj => Box::new(jj::JjBackend::new(path)?),
        BackendKind::Git => Box::new(git::GitBackend::new(path)?),
    })
}

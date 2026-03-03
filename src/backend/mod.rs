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
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub short_hash: String,
    pub summary: String,
    pub author: String,
    pub timestamp: i64,
}

pub trait Backend {
    fn repo_root(&self) -> &Path;
    fn kind_name(&self) -> &'static str;
    fn status(&self) -> Result<RepoStatus>;
    fn diff_file(&self, path: &str, staged: bool) -> Result<String>;
    fn stage_file(&self, path: &str) -> Result<()>;
    fn unstage_file(&self, path: &str) -> Result<()>;
    fn discard_file(&self, path: &str) -> Result<()>;
    fn stage_all(&self) -> Result<()>;
    fn unstage_all(&self) -> Result<()>;
    fn commit(&self, message: &str) -> Result<()>;
    fn amend(&self, message: &str) -> Result<()>;
    fn log(&self, limit: usize) -> Result<Vec<CommitInfo>>;
    fn push(&self) -> Result<String>;
    fn push_force_lease(&self) -> Result<String>;
    fn pull(&self) -> Result<String>;
}

#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum BackendKind {
    Git,
    Jj,
}

/// Walk up from `start` looking for a `.jj` directory.
pub fn has_jj_repo(start: &Path) -> bool {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join(".jj").is_dir() {
            return true;
        }
        if !cur.pop() {
            break;
        }
    }
    false
}

pub fn detect_backend(
    path: &str,
    forced: Option<BackendKind>,
    config: &Config,
) -> Result<Box<dyn Backend>> {
    let p = std::path::PathBuf::from(path);

    // Resolve forced kind (CLI arg wins, then config)
    let kind = forced.or_else(|| {
        config.backend.as_deref().and_then(|s| match s {
            "jj" => Some(BackendKind::Jj),
            "git" => Some(BackendKind::Git),
            _ => None,
        })
    });

    match kind {
        Some(BackendKind::Jj) => {
            Ok(Box::new(jj::JjBackend::new(&p)?))
        }
        Some(BackendKind::Git) => {
            Ok(Box::new(git::GitBackend::new(&p)?))
        }
        None => {
            // Auto-detect: prefer jj if .jj dir found
            if has_jj_repo(&p) {
                Ok(Box::new(jj::JjBackend::new(&p)?))
            } else {
                Ok(Box::new(git::GitBackend::new(&p)?))
            }
        }
    }
}

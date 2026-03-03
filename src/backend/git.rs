use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use git2::{DiffFormat, DiffOptions, IndexAddOption, Repository, ResetType, Sort, StatusOptions};

use super::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus};

pub struct GitBackend {
    repo: Repository,
    root: PathBuf,
}

impl GitBackend {
    pub fn new(path: &Path) -> Result<Self> {
        let repo = Repository::discover(path)
            .with_context(|| format!("Could not find git repository at {}", path.display()))?;
        let root = repo
            .workdir()
            .ok_or_else(|| anyhow::anyhow!("Bare repositories are not supported"))?
            .to_path_buf();
        Ok(Self { repo, root })
    }

    fn head_info(&self) -> (Option<String>, Option<String>, Option<String>) {
        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return (None, None, None),
        };

        let branch = if head.is_branch() {
            head.shorthand().map(String::from)
        } else {
            // detached HEAD
            head.shorthand().map(|s| format!("({})", s))
        };

        let commit = head.peel_to_commit().ok();
        let short_hash = commit.as_ref().map(|c| {
            let id = c.id();
            format!("{:.7}", id)
        });
        let summary = commit.as_ref().and_then(|c| c.summary().map(String::from));

        (branch, short_hash, summary)
    }
}

impl Backend for GitBackend {
    fn repo_root(&self) -> &Path {
        &self.root
    }

    fn kind_name(&self) -> &'static str {
        "git"
    }

    fn status(&self) -> Result<RepoStatus> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);

        let statuses = self.repo.statuses(Some(&mut opts))?;

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let s = entry.status();

            // Staged changes
            if s.contains(git2::Status::INDEX_NEW) {
                staged.push(FileEntry { path: path.clone(), kind: FileKind::Added });
            } else if s.contains(git2::Status::INDEX_MODIFIED) {
                staged.push(FileEntry { path: path.clone(), kind: FileKind::Modified });
            } else if s.contains(git2::Status::INDEX_DELETED) {
                staged.push(FileEntry { path: path.clone(), kind: FileKind::Deleted });
            } else if s.contains(git2::Status::INDEX_RENAMED) {
                let new_path = entry.head_to_index()
                    .and_then(|d| d.new_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                staged.push(FileEntry { path: path.clone(), kind: FileKind::Renamed(new_path) });
            }

            // Worktree changes
            if s.contains(git2::Status::WT_MODIFIED) {
                unstaged.push(FileEntry { path: path.clone(), kind: FileKind::Modified });
            } else if s.contains(git2::Status::WT_DELETED) {
                unstaged.push(FileEntry { path: path.clone(), kind: FileKind::Deleted });
            } else if s.contains(git2::Status::WT_RENAMED) {
                let new_path = entry.index_to_workdir()
                    .and_then(|d| d.new_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                unstaged.push(FileEntry { path: path.clone(), kind: FileKind::Renamed(new_path) });
            } else if s.contains(git2::Status::WT_NEW) {
                untracked.push(FileEntry { path: path.clone(), kind: FileKind::Untracked });
            }

            if s.contains(git2::Status::CONFLICTED) {
                unstaged.push(FileEntry { path: path.clone(), kind: FileKind::Conflicted });
            }
        }

        let (head, head_short_hash, head_summary) = self.head_info();

        Ok(RepoStatus {
            head,
            head_short_hash,
            head_summary,
            upstream: None, // TODO: fetch upstream tracking info
            staged,
            unstaged,
            untracked,
        })
    }

    fn diff_file(&self, path: &str, staged: bool) -> Result<String> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);

        let diff = if staged {
            let head_tree = self.repo.head().ok()
                .and_then(|h| h.peel_to_commit().ok())
                .and_then(|c| c.tree().ok());
            let index = self.repo.index()?;
            self.repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut diff_opts))?
        } else {
            self.repo.diff_index_to_workdir(None, Some(&mut diff_opts))?
        };

        let mut output = String::new();
        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");
            match line.origin() {
                '+' | '-' | ' ' => output.push(line.origin()),
                _ => {}
            }
            output.push_str(content);
            true
        })?;

        Ok(output)
    }

    fn stage_file(&self, path: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        let full_path = self.root.join(path);
        if full_path.exists() {
            index.add_path(Path::new(path))?;
        } else {
            index.remove_path(Path::new(path))?;
        }
        index.write()?;
        Ok(())
    }

    fn unstage_file(&self, path: &str) -> Result<()> {
        // If HEAD exists, reset that specific file
        if let Ok(head) = self.repo.head() {
            if let Ok(commit) = head.peel_to_commit() {
                let obj = commit.as_object();
                self.repo.reset_default(Some(obj), [path].iter())?;
                return Ok(());
            }
        }
        // No HEAD (initial repo): just remove from index
        let mut index = self.repo.index()?;
        index.remove_path(Path::new(path))?;
        index.write()?;
        Ok(())
    }

    fn discard_file(&self, path: &str) -> Result<()> {
        let mut cb = git2::build::CheckoutBuilder::new();
        cb.path(path).force();
        self.repo.checkout_index(None, Some(&mut cb))?;
        Ok(())
    }

    fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    fn unstage_all(&self) -> Result<()> {
        if let Ok(head) = self.repo.head() {
            if let Ok(commit) = head.peel_to_commit() {
                let obj = commit.as_object();
                self.repo.reset(obj, ResetType::Mixed, None)?;
                return Ok(());
            }
        }
        // No HEAD: clear the index
        let mut index = self.repo.index()?;
        index.clear()?;
        index.write()?;
        Ok(())
    }

    fn commit(&self, message: &str) -> Result<()> {
        let sig = self.repo.signature()?;
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let parents: Vec<git2::Commit> = if let Ok(head) = self.repo.head() {
            if let Ok(commit) = head.peel_to_commit() {
                vec![commit]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        self.repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &parent_refs,
        )?;

        Ok(())
    }

    fn log(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let mut walk = self.repo.revwalk()?;
        walk.push_head().ok(); // ok if no commits yet
        walk.set_sorting(Sort::TIME)?;

        let mut commits = Vec::new();
        for oid_result in walk.take(limit) {
            let oid = oid_result?;
            let commit = self.repo.find_commit(oid)?;
            let short_hash = format!("{:.7}", commit.id());
            let summary = commit.summary().unwrap_or("").to_string();
            let author = commit.author().name().unwrap_or("").to_string();
            let timestamp = commit.time().seconds();
            commits.push(CommitInfo {
                short_hash,
                summary,
                author,
                timestamp,
            });
        }
        Ok(commits)
    }
}


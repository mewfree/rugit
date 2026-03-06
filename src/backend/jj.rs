use std::path::{Path, PathBuf};
use anyhow::{bail, Result};

use super::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus};

pub struct JjBackend {
    root: PathBuf,
}

impl JjBackend {
    pub fn new(path: &Path) -> Result<Self> {
        // Find the .jj root
        let mut cur = path.to_path_buf();
        loop {
            if cur.join(".jj").is_dir() {
                return Ok(Self { root: cur });
            }
            if !cur.pop() {
                break;
            }
        }
        bail!("No .jj directory found from {}", path.display())
    }

    fn run_jj(&self, args: &[&str]) -> Result<String> {
        let output = std::process::Command::new("jj")
            .args(args)
            .current_dir(&self.root)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("jj {} failed: {}", args.join(" "), stderr);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Backend for JjBackend {
    fn repo_root(&self) -> &Path {
        &self.root
    }

    fn kind_name(&self) -> &'static str {
        "jj"
    }

    fn status(&self) -> Result<RepoStatus> {
        let output = self.run_jj(&["status", "--no-pager"])?;
        let mut unstaged = Vec::new();
        let mut staged = Vec::new();

        for line in output.lines() {
            if line.starts_with("M ") {
                unstaged.push(FileEntry {
                    path: line[2..].trim().to_string(),
                    kind: FileKind::Modified,
                });
            } else if line.starts_with("A ") {
                staged.push(FileEntry {
                    path: line[2..].trim().to_string(),
                    kind: FileKind::Added,
                });
            } else if line.starts_with("D ") {
                unstaged.push(FileEntry {
                    path: line[2..].trim().to_string(),
                    kind: FileKind::Deleted,
                });
            }
        }

        Ok(RepoStatus {
            head: None,
            head_short_hash: None,
            head_summary: None,
            upstream: None,
            staged,
            unstaged,
            untracked: vec![],
            unpushed: vec![],
        })
    }

    fn diff_file(&self, path: &str, _staged: bool) -> Result<String> {
        self.run_jj(&["diff", "--no-pager", path])
    }

    fn stage_file(&self, _path: &str) -> Result<()> {
        // TODO: jj doesn't have a staging area; implement when needed
        bail!("jj write ops not yet implemented")
    }

    fn unstage_file(&self, _path: &str) -> Result<()> {
        // TODO: implement
        bail!("jj write ops not yet implemented")
    }

    fn discard_file(&self, _path: &str) -> Result<()> {
        // TODO: implement
        bail!("jj write ops not yet implemented")
    }

    fn stage_all(&self) -> Result<()> {
        // TODO: implement
        bail!("jj write ops not yet implemented")
    }

    fn unstage_all(&self) -> Result<()> {
        // TODO: implement
        bail!("jj write ops not yet implemented")
    }

    fn commit(&self, _message: &str) -> Result<()> {
        // TODO: implement jj commit
        bail!("jj write ops not yet implemented")
    }

    fn amend(&self, _message: &str) -> Result<()> {
        bail!("amend not supported for jj backend")
    }

    fn push(&self) -> Result<String> {
        let out = self.run_jj(&["git", "push"])?;
        Ok(if out.trim().is_empty() { "Push successful".into() } else { out.trim().to_string() })
    }

    fn push_force_lease(&self) -> Result<String> {
        anyhow::bail!("force-with-lease not supported for jj backend")
    }

    fn pull(&self) -> Result<String> {
        let out = self.run_jj(&["git", "fetch"])?;
        Ok(if out.trim().is_empty() { "Fetch successful".into() } else { out.trim().to_string() })
    }

    fn apply_patch(&self, _patch: &str, _reverse: bool) -> Result<()> {
        bail!("jj hunk staging not yet implemented")
    }

    fn discard_hunk(&self, _path: &str, _hunk_index: usize) -> Result<()> {
        bail!("jj hunk discard not yet implemented")
    }

    fn discard_all_unstaged(&self) -> Result<()> {
        bail!("jj write ops not yet implemented")
    }

    fn show_commit(&self, hash: &str) -> Result<String> {
        self.run_jj(&["show", "--no-pager", hash])
    }

    fn log(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let template = r#"separate("\x1f", change_id.short(), description.first_line(), author.name(), author.timestamp()) ++ "\n""#;
        let limit_str = limit.to_string();
        let output = self.run_jj(&[
            "log",
            "--no-graph",
            "--no-pager",
            "--template",
            template,
            "-n",
            &limit_str,
        ])?;

        let mut commits = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.splitn(4, '\x1f').collect();
            if parts.len() >= 4 {
                commits.push(CommitInfo {
                    short_hash: parts[0].to_string(),
                    summary: parts[1].to_string(),
                    author: parts[2].to_string(),
                    timestamp: 0, // TODO: parse timestamp
                });
            }
        }
        Ok(commits)
    }
}

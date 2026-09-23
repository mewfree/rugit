use std::path::{Path, PathBuf};
use anyhow::{bail, Result};

use super::{Backend, CommitInfo, FileEntry, FileKind, RepoStatus};

pub struct JjBackend {
    root: PathBuf,
}

impl JjBackend {
    pub fn new(path: &Path) -> Result<Self> {
        match super::find_jj_root(path) {
            Some(root) => Ok(Self { root: root.to_path_buf() }),
            None => bail!("No .jj directory found from {}", path.display()),
        }
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
            let Some((code, path)) = line.split_once(' ') else { continue };
            let (list, kind) = match code {
                "M" => (&mut unstaged, FileKind::Modified),
                "A" => (&mut staged, FileKind::Added),
                "D" => (&mut unstaged, FileKind::Deleted),
                _ => continue,
            };
            list.push(FileEntry { path: path.trim().to_string(), kind });
        }

        Ok(RepoStatus { staged, unstaged, ..Default::default() })
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

    fn head_commit_message(&self) -> Result<String> {
        bail!("head_commit_message not supported for jj backend")
    }

    fn commit_message(&self, _hash: &str) -> Result<String> {
        bail!("commit_message not supported for jj backend")
    }

    fn reword_commit(&self, _hash: &str, _message: &str) -> Result<()> {
        bail!("reword not supported for jj backend")
    }

    fn push(&self) -> Result<()> {
        self.run_jj(&["git", "push"])?;
        Ok(())
    }

    fn push_force_lease(&self) -> Result<()> {
        bail!("force-with-lease not supported for jj backend")
    }

    fn pull(&self) -> Result<()> {
        self.run_jj(&["git", "fetch"])?;
        Ok(())
    }

    fn apply_patch(&self, _patch: &str, _reverse: bool) -> Result<()> {
        bail!("jj hunk staging not yet implemented")
    }

    fn discard_hunk(&self, _path: &str, _hunk_index: usize) -> Result<()> {
        bail!("jj hunk discard not yet implemented")
    }

    fn discard_patch(&self, _patch: &str) -> Result<()> {
        bail!("jj hunk discard not yet implemented")
    }

    fn discard_all_unstaged(&self) -> Result<()> {
        bail!("jj write ops not yet implemented")
    }

    fn discard_staged_file(&self, _path: &str) -> Result<()> {
        bail!("jj write ops not yet implemented")
    }

    fn discard_all_staged(&self) -> Result<()> {
        bail!("jj write ops not yet implemented")
    }

    fn fixup_commit(&self, _hash: &str) -> Result<()> {
        bail!("fixup not supported for jj backend")
    }

    fn squash_commit(&self, _hash: &str) -> Result<()> {
        bail!("squash not supported for jj backend")
    }

    fn stash(&self) -> Result<()> {
        bail!("stash not supported for jj backend")
    }

    fn stash_pop(&self, _index: usize) -> Result<()> {
        bail!("stash not supported for jj backend")
    }

    fn stash_apply(&self, _index: usize) -> Result<()> {
        bail!("stash not supported for jj backend")
    }

    fn stash_drop(&self, _index: usize) -> Result<()> {
        bail!("stash not supported for jj backend")
    }

    fn stash_list(&self) -> Result<Vec<super::StashInfo>> {
        bail!("stash not supported for jj backend")
    }

    fn list_branches(&self) -> Result<Vec<super::BranchInfo>> {
        bail!("branch management not yet supported for jj backend")
    }

    fn checkout_branch(&self, _name: &str) -> Result<()> {
        bail!("branch management not yet supported for jj backend")
    }

    fn create_branch(&self, _name: &str) -> Result<()> {
        bail!("branch management not yet supported for jj backend")
    }

    fn delete_branch(&self, _name: &str) -> Result<()> {
        bail!("branch management not yet supported for jj backend")
    }

    fn rename_branch(&self, _old: &str, _new: &str) -> Result<()> {
        bail!("branch management not yet supported for jj backend")
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

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(4, '\x1f');
                let (hash, summary, author, _timestamp) =
                    (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
                Some(CommitInfo {
                    short_hash: hash.to_string(),
                    summary: summary.to_string(),
                    author: author.to_string(),
                })
            })
            .collect())
    }
}

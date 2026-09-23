use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use anyhow::{bail, Context, Result};
use git2::{Commit, DiffFormat, DiffOptions, IndexAddOption, Oid, Repository, ResetType, Sort, Status, StatusOptions};

use super::{Backend, BranchInfo, CommitInfo, FileEntry, FileKind, RepoStatus, StashInfo};
use crate::diff;

pub struct GitBackend {
    repo: Repository,
    root: PathBuf,
}

/// Fails with git's stderr when the command exited non-zero.
fn check(out: Output) -> Result<Output> {
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out)
}

fn commit_info(commit: &Commit) -> CommitInfo {
    CommitInfo {
        short_hash: format!("{:.7}", commit.id()),
        summary: commit.summary().unwrap_or("").to_string(),
        author: commit.author().name().unwrap_or("").to_string(),
    }
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

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(&self.root);
        cmd
    }

    fn run_git(&self, args: &[&str]) -> Result<Output> {
        check(self.command(args).output()?)
    }

    /// Like `run_git`, feeding `input` on stdin.
    fn pipe_git(&self, args: &[&str], input: &str) -> Result<()> {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // Dropped at the end of the statement, closing stdin so git can finish.
        child.stdin.take().context("git stdin unavailable")?.write_all(input.as_bytes())?;
        check(child.wait_with_output()?)?;
        Ok(())
    }

    fn head_commit(&self) -> Option<Commit<'_>> {
        self.repo.head().ok()?.peel_to_commit().ok()
    }

    fn find_commit(&self, rev: &str) -> Result<Commit<'_>> {
        self.repo
            .revparse_single(rev)
            .and_then(|obj| obj.peel_to_commit())
            .with_context(|| format!("could not resolve commit {rev}"))
    }

    /// Non-interactively run `git rebase -i --autosquash` so a just-created
    /// `fixup!`/`squash!` commit is folded into its target. Targets the parent
    /// of `hash` so the rebase range includes the target commit itself.
    fn autosquash_rebase(&self, hash: &str) -> Result<()> {
        // If the target is the root commit it has no parent, so rebase onto
        // `--root` instead of `<hash>^`.
        let has_parent = self
            .find_commit(hash)
            .map(|c| c.parent_count() > 0)
            .unwrap_or(true);
        let base = if has_parent {
            format!("{hash}^")
        } else {
            "--root".to_string()
        };
        let out = self
            .command(&["rebase", "-i", "--autosquash", "--autostash", &base])
            // `:` exits 0 without touching the file, so the autosquash-ordered
            // todo list and the squash commit message are accepted as-is.
            .env("GIT_SEQUENCE_EDITOR", ":")
            .env("GIT_EDITOR", ":")
            .output()?;
        if let Err(e) = check(out) {
            // Don't leave the repo mid-rebase (e.g. on a conflict).
            let _ = self.run_git(&["rebase", "--abort"]);
            bail!("autosquash rebase failed (conflict?); aborted. {e}");
        }
        Ok(())
    }

    /// `git commit --fixup=reword:` rejects `-m`, and `--autosquash` only
    /// matches a subject of `amend! <full hash>`. The editor script writes
    /// that subject plus the user's message; everything after the first line
    /// becomes the rewritten message.
    fn create_reword_commit(&self, full_hash: &str, message: &str) -> Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rugit-reword-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let outcome = (|| -> Result<()> {
            let msg_path = dir.join("message");
            let editor_path = dir.join("editor.sh");
            std::fs::write(
                &msg_path,
                format!("amend! {full_hash}\n\n{}\n", message.trim_end()),
            )?;
            std::fs::write(
                &editor_path,
                "#!/bin/sh\ncat \"$RUGIT_REWORD_MSG\" > \"$1\"\n",
            )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&editor_path, std::fs::Permissions::from_mode(0o755))?;
            }
            check(
                self.command(&["commit", &format!("--fixup=reword:{full_hash}")])
                    .env("GIT_EDITOR", &editor_path)
                    .env("RUGIT_REWORD_MSG", &msg_path)
                    .output()?,
            )?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        outcome
    }

    fn ensure_index_clean(&self) -> Result<()> {
        let head_tree = self.repo.head().and_then(|h| h.peel_to_tree()).ok();
        let staged = self.repo.diff_tree_to_index(head_tree.as_ref(), None, None)?;
        if staged.deltas().len() > 0 {
            bail!("cannot reword while the index has staged changes");
        }
        Ok(())
    }

    /// Cap on unpushed commits we parse. The walk still counts them all.
    const UNPUSHED_DISPLAY_LIMIT: usize = 100;

    fn upstream_info(&self) -> (Option<String>, Vec<CommitInfo>, usize) {
        let Some(upstream) = self
            .repo
            .head()
            .ok()
            .and_then(|head| git2::Branch::wrap(head).upstream().ok())
        else {
            return (None, vec![], 0);
        };
        let name = upstream.name().ok().flatten().map(String::from);
        let (commits, total) = upstream
            .get()
            .peel_to_commit()
            .ok()
            .and_then(|c| self.unpushed_since(c.id()).ok())
            .unwrap_or_default();
        (name, commits, total)
    }

    /// Commits reachable from HEAD but not from `upstream`: the first
    /// `UNPUSHED_DISPLAY_LIMIT` of them, plus the full count.
    fn unpushed_since(&self, upstream: Oid) -> Result<(Vec<CommitInfo>, usize)> {
        let mut walk = self.repo.revwalk()?;
        walk.push_head()?;
        walk.hide(upstream)?;
        walk.set_sorting(Sort::TIME)?;

        let mut commits = Vec::new();
        let mut total = 0;
        for oid in walk.map_while(Result::ok) {
            total += 1;
            if commits.len() < Self::UNPUSHED_DISPLAY_LIMIT {
                let Ok(commit) = self.repo.find_commit(oid) else { break };
                commits.push(commit_info(&commit));
            }
        }
        Ok((commits, total))
    }

    fn head_info(&self) -> (Option<String>, Option<String>, Option<String>) {
        let Ok(head) = self.repo.head() else {
            return (None, None, None);
        };
        let branch = head.shorthand().map(|s| {
            if head.is_branch() { s.to_string() } else { format!("({s})") } // detached
        });
        let commit = head.peel_to_commit().ok();
        let short_hash = commit.as_ref().map(|c| format!("{:.7}", c.id()));
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
            let path = entry.path().unwrap_or("");
            let s = entry.status();
            let file = |kind| FileEntry { path: path.to_string(), kind };
            let renamed = |delta: Option<git2::DiffDelta>| {
                FileKind::Renamed(
                    delta
                        .and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned()))
                        .unwrap_or_default(),
                )
            };

            if s.contains(Status::INDEX_NEW) {
                staged.push(file(FileKind::Added));
            } else if s.contains(Status::INDEX_MODIFIED) {
                staged.push(file(FileKind::Modified));
            } else if s.contains(Status::INDEX_DELETED) {
                staged.push(file(FileKind::Deleted));
            } else if s.contains(Status::INDEX_RENAMED) {
                staged.push(file(renamed(entry.head_to_index())));
            }

            if s.contains(Status::WT_MODIFIED) {
                unstaged.push(file(FileKind::Modified));
            } else if s.contains(Status::WT_DELETED) {
                unstaged.push(file(FileKind::Deleted));
            } else if s.contains(Status::WT_RENAMED) {
                unstaged.push(file(renamed(entry.index_to_workdir())));
            } else if s.contains(Status::WT_NEW) {
                untracked.push(file(FileKind::Untracked));
            }

            if s.contains(Status::CONFLICTED) {
                unstaged.push(file(FileKind::Conflicted));
            }
        }

        let (head, head_short_hash, head_summary) = self.head_info();
        let (upstream, unpushed, unpushed_total) = self.upstream_info();

        Ok(RepoStatus {
            head,
            head_short_hash,
            head_summary,
            upstream,
            staged,
            unstaged,
            untracked,
            unpushed,
            unpushed_total,
        })
    }

    fn diff_file(&self, path: &str, staged: bool) -> Result<String> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);

        let diff = if staged {
            let head_tree = self.head_commit().and_then(|c| c.tree().ok());
            self.repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut diff_opts))?
        } else {
            self.repo.diff_index_to_workdir(None, Some(&mut diff_opts))?
        };

        let mut output = String::new();
        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            if let origin @ ('+' | '-' | ' ') = line.origin() {
                output.push(origin);
            }
            output.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;

        Ok(output)
    }

    fn stage_file(&self, path: &str) -> Result<()> {
        self.stage_files(&[path.to_string()])
    }

    /// One index read/write for all paths, vs one per path in a `stage_file` loop.
    fn stage_files(&self, paths: &[String]) -> Result<()> {
        let mut index = self.repo.index()?;
        for path in paths {
            if self.root.join(path).exists() {
                index.add_path(Path::new(path))?;
            } else {
                index.remove_path(Path::new(path))?;
            }
        }
        index.write()?;
        Ok(())
    }

    fn unstage_file(&self, path: &str) -> Result<()> {
        if let Some(head) = self.head_commit() {
            self.repo.reset_default(Some(head.as_object()), [path])?;
            return Ok(());
        }
        // No HEAD (initial repo): just remove from index
        let mut index = self.repo.index()?;
        index.remove_path(Path::new(path))?;
        index.write()?;
        Ok(())
    }

    fn discard_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", "--", path])?;
        Ok(())
    }

    fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["*"], IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    fn unstage_all(&self) -> Result<()> {
        if let Some(head) = self.head_commit() {
            self.repo.reset(head.as_object(), ResetType::Mixed, None)?;
            return Ok(());
        }
        // No HEAD: clear the index
        let mut index = self.repo.index()?;
        index.clear()?;
        index.write()?;
        Ok(())
    }

    fn commit(&self, message: &str) -> Result<()> {
        self.run_git(&["commit", "-m", message])?;
        Ok(())
    }

    fn amend(&self, message: &str) -> Result<()> {
        self.run_git(&["commit", "--amend", "-m", message])?;
        Ok(())
    }

    fn head_commit_message(&self) -> Result<String> {
        let commit = self.repo.head()?.peel_to_commit()?;
        Ok(commit.message().unwrap_or("").to_string())
    }

    fn commit_message(&self, hash: &str) -> Result<String> {
        Ok(self.find_commit(hash)?.message().unwrap_or("").to_string())
    }

    fn reword_commit(&self, hash: &str, message: &str) -> Result<()> {
        if message.trim().is_empty() {
            bail!("empty commit message");
        }
        // --autostash applies with `stash apply`, which does not restore the
        // index, so a staged change would come back unstaged.
        self.ensure_index_clean()?;
        let full = self.find_commit(hash)?.id().to_string();
        let head_before = self
            .head_commit()
            .context("reword needs an existing HEAD commit")?
            .id()
            .to_string();

        self.create_reword_commit(&full, message)?;
        if let Err(e) = self.autosquash_rebase(&full) {
            if let Err(reset_err) = self.run_git(&["reset", "--soft", &head_before]) {
                bail!("{e} (also failed to drop the amend! commit: {reset_err})");
            }
            return Err(e);
        }
        Ok(())
    }

    fn push(&self) -> Result<()> {
        let Err(e) = self.run_git(&["push"]) else { return Ok(()) };
        let msg = e.to_string();
        if !(msg.contains("no upstream branch") || msg.contains("has no upstream")) {
            return Err(e);
        }
        // No upstream configured — push with --set-upstream to origin
        let branch = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .context("Could not determine current branch")?;
        self.run_git(&["push", "--set-upstream", "origin", &branch])?;
        Ok(())
    }

    fn push_force_lease(&self) -> Result<()> {
        self.run_git(&["push", "--force-with-lease"])?;
        Ok(())
    }

    fn pull(&self) -> Result<()> {
        self.run_git(&["pull"])?;
        Ok(())
    }

    fn log(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        let mut walk = self.repo.revwalk()?;
        walk.push_head().ok(); // ok if no commits yet
        walk.set_sorting(Sort::TIME)?;
        walk.take(limit)
            .map(|oid| Ok(commit_info(&self.repo.find_commit(oid?)?)))
            .collect()
    }

    fn apply_patch(&self, patch: &str, reverse: bool) -> Result<()> {
        let args: &[&str] = if reverse {
            &["apply", "--cached", "--reverse"]
        } else {
            &["apply", "--cached"]
        };
        self.pipe_git(args, patch)
    }

    fn discard_patch(&self, patch: &str) -> Result<()> {
        self.pipe_git(&["apply", "--reverse"], patch)
    }

    fn discard_all_unstaged(&self) -> Result<()> {
        self.run_git(&["restore", "."])?;
        Ok(())
    }

    fn discard_staged_file(&self, path: &str) -> Result<()> {
        self.run_git(&["restore", "--staged", "--worktree", "--", path])?;
        Ok(())
    }

    fn discard_all_staged(&self) -> Result<()> {
        self.run_git(&["restore", "--staged", "--worktree", "."])?;
        Ok(())
    }

    fn discard_hunk(&self, path: &str, hunk_index: usize) -> Result<()> {
        // Get a fresh diff via subprocess so the patch format exactly matches
        // what `git apply` expects when operating on the working tree.
        let out = self.run_git(&["diff", "--", path])?;
        let patch = diff::hunk_patch(&String::from_utf8_lossy(&out.stdout), hunk_index)
            .with_context(|| format!("hunk {hunk_index} not found in diff"))?;
        self.discard_patch(&patch)
    }

    fn fixup_commit(&self, hash: &str) -> Result<()> {
        self.run_git(&["commit", &format!("--fixup={hash}")])?;
        self.autosquash_rebase(hash)
    }

    fn squash_commit(&self, hash: &str) -> Result<()> {
        self.run_git(&["commit", &format!("--squash={hash}")])?;
        self.autosquash_rebase(hash)
    }

    fn show_commit(&self, hash: &str) -> Result<String> {
        let out = self.run_git(&["show", "--stat", "-p", "--color=never", hash])?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn stash(&self) -> Result<()> {
        self.run_git(&["stash", "push"])?;
        Ok(())
    }

    fn stash_pop(&self, index: usize) -> Result<()> {
        self.run_git(&["stash", "pop", &format!("stash@{{{index}}}")])?;
        Ok(())
    }

    fn stash_apply(&self, index: usize) -> Result<()> {
        self.run_git(&["stash", "apply", &format!("stash@{{{index}}}")])?;
        Ok(())
    }

    fn stash_drop(&self, index: usize) -> Result<()> {
        self.run_git(&["stash", "drop", &format!("stash@{{{index}}}")])?;
        Ok(())
    }

    /// Reads `refs/stash`'s reflog; `git stash list` would fork on every refresh.
    /// Summaries omit the `stash@{N}: ` prefix; callers add it.
    fn stash_list(&self) -> Result<Vec<StashInfo>> {
        // No stash ref is the common case, not an error.
        let Ok(reflog) = self.repo.reflog("refs/stash") else {
            return Ok(Vec::new());
        };
        Ok(reflog
            .iter()
            .enumerate()
            .map(|(index, entry)| StashInfo {
                index,
                summary: entry.message().unwrap_or("").to_string(),
            })
            .collect())
    }

    fn list_branches(&self) -> Result<Vec<BranchInfo>> {
        let head_name = self.repo.head().ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_default();
        let mut result = Vec::new();
        for branch in self.repo.branches(Some(git2::BranchType::Local))? {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                result.push(BranchInfo {
                    name: name.to_string(),
                    is_current: name == head_name,
                });
            }
        }
        result.sort_by(|a, b| b.is_current.cmp(&a.is_current).then(a.name.cmp(&b.name)));
        Ok(result)
    }

    fn checkout_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["switch", name])?;
        Ok(())
    }

    fn create_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["switch", "-c", name])?;
        Ok(())
    }

    fn delete_branch(&self, name: &str) -> Result<()> {
        self.run_git(&["branch", "-d", name])?;
        Ok(())
    }

    fn rename_branch(&self, old: &str, new: &str) -> Result<()> {
        self.run_git(&["branch", "-m", old, new])?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::GitBackend;
    use crate::backend::Backend;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static N: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct TestRepo {
        pub(crate) path: PathBuf,
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    impl TestRepo {
        pub(crate) fn new() -> Self {
            let n = N.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("rugit-reword-test-{nanos}-{n}"));
            fs::create_dir_all(&path).unwrap();
            let repo = Self { path };
            repo.git(&["init", "-q"]);
            repo.git(&["config", "user.email", "t@t"]);
            repo.git(&["config", "user.name", "t"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            let hooks = repo.path.join(".git/hooks");
            fs::create_dir_all(&hooks).unwrap();
            repo.git(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
            repo
        }

        pub(crate) fn git(&self, args: &[&str]) -> std::process::Output {
            let out = Command::new("git")
                .args(args)
                .current_dir(&self.path)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            );
            out
        }

        pub(crate) fn commit(&self, path: &str, body: &str, message: &str) {
            fs::write(self.path.join(path), body).unwrap();
            self.git(&["add", path]);
            self.git(&["commit", "-qm", message]);
        }

        pub(crate) fn backend(&self) -> GitBackend {
            GitBackend::new(&self.path).unwrap()
        }

        pub(crate) fn stdout(&self, args: &[&str]) -> String {
            String::from_utf8(self.git(args).stdout).unwrap()
        }

        fn log_subjects(&self) -> Vec<String> {
            self.stdout(&["log", "--format=%s"])
                .lines()
                .map(|s| s.to_string())
                .collect()
        }

        fn head(&self) -> String {
            self.stdout(&["rev-parse", "HEAD"]).trim().to_string()
        }
    }

    #[test]
    fn reword_replaces_message_and_replays_later_commits() {
        let repo = TestRepo::new();
        repo.commit("a", "a\n", "first subject");
        fs::write(repo.path.join("b"), "b\n").unwrap();
        repo.git(&["add", "b"]);
        repo.git(&[
            "commit",
            "-qm",
            "second subject",
            "--author=Other <o@example>",
        ]);
        repo.commit("c", "c\n", "third");
        let first = repo.stdout(&["rev-parse", "HEAD~2"]);
        let target = repo.stdout(&["rev-parse", "--short", "HEAD~1"]);

        repo.backend()
            .reword_commit(target.trim(), "rewritten subject\n\nrewritten body")
            .unwrap();

        assert_eq!(
            repo.log_subjects(),
            vec!["third", "rewritten subject", "first subject"]
        );
        assert_eq!(repo.stdout(&["rev-parse", "HEAD~2"]).trim(), first.trim());
        assert_eq!(
            repo.stdout(&["log", "-1", "--format=%B", "HEAD~1"]).trim(),
            "rewritten subject\n\nrewritten body"
        );
        assert_eq!(
            repo.stdout(&["log", "-1", "--format=%an <%ae>", "HEAD~1"]).trim(),
            "Other <o@example>"
        );
    }

    #[test]
    fn reword_root_commit() {
        let repo = TestRepo::new();
        repo.commit("a", "a\n", "root subject");
        repo.commit("b", "b\n", "child");
        let root = repo.stdout(&["rev-parse", "--short", "HEAD~1"]);

        repo.backend()
            .reword_commit(root.trim(), "renamed root")
            .unwrap();

        assert_eq!(repo.log_subjects(), vec!["child", "renamed root"]);
    }

    #[test]
    fn reword_refuses_staged_changes() {
        let repo = TestRepo::new();
        repo.commit("a", "a\n", "first");
        repo.commit("b", "b\n", "second");
        fs::write(repo.path.join("a"), "changed\n").unwrap();
        repo.git(&["add", "a"]);
        let head = repo.head();

        let err = repo
            .backend()
            .reword_commit("HEAD~1", "nope")
            .unwrap_err()
            .to_string();

        assert!(err.contains("staged"), "{err}");
        assert_eq!(repo.head(), head);
        assert_eq!(repo.log_subjects(), vec!["second", "first"]);
        assert_eq!(repo.stdout(&["diff", "--cached", "--name-only"]).trim(), "a");
    }

    #[test]
    fn reword_keeps_unstaged_work() {
        let repo = TestRepo::new();
        repo.commit("a", "a\n", "first");
        repo.commit("b", "b\n", "second");
        fs::write(repo.path.join("a"), "dirty\n").unwrap();

        repo.backend()
            .reword_commit("HEAD", "new subject")
            .unwrap();

        assert_eq!(repo.log_subjects(), vec!["new subject", "first"]);
        // Leading space: the edit is unstaged, not staged. trim() would hide that.
        assert_eq!(repo.stdout(&["status", "--porcelain"]).trim_end(), " M a");
        assert_eq!(fs::read_to_string(repo.path.join("a")).unwrap(), "dirty\n");
    }

    #[test]
    fn failed_rebase_drops_amend_commit() {
        let repo = TestRepo::new();
        repo.commit("a", "a\n", "first");
        repo.commit("b", "b\n", "second");
        repo.commit("c", "c\n", "third");
        fs::write(repo.path.join("a"), "dirty\n").unwrap();
        let hook = repo.path.join(".git/hooks/pre-rebase");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let head = repo.head();

        let err = repo
            .backend()
            .reword_commit("HEAD~1", "nope")
            .unwrap_err()
            .to_string();

        assert!(
            err.contains("rebase") || err.contains("hook") || err.contains("refused"),
            "{err}"
        );
        assert_eq!(repo.head(), head);
        assert_eq!(repo.log_subjects(), vec!["third", "second", "first"]);
        assert_eq!(fs::read_to_string(repo.path.join("a")).unwrap(), "dirty\n");
        assert_eq!(repo.stdout(&["status", "--porcelain"]).trim_end(), " M a");
        assert!(repo.stdout(&["stash", "list"]).trim().is_empty());
    }

    #[test]
    fn stages_and_unstages_a_single_line() {
        use std::collections::HashSet;
        let repo = TestRepo::new();
        repo.commit("f", "a\nb\n", "base");
        fs::write(repo.path.join("f"), "a\nB\nc\n").unwrap();
        let backend = repo.backend();

        // Unstaged hunk body: " a", "-b", "+B", "+c". Stage only "+c".
        let diff = backend.diff_file("f", false).unwrap();
        let only_c: HashSet<usize> = [3].into();
        let patch = crate::diff::lines_patch(&diff, 0, &only_c, false).unwrap();
        backend.apply_patch(&patch, false).unwrap();
        assert_eq!(repo.stdout(&["show", ":f"]), "a\nb\nc\n");

        // Staged hunk body: " a", " b", "+c". Unstage it again.
        let staged = backend.diff_file("f", true).unwrap();
        let added: HashSet<usize> = [2].into();
        let patch = crate::diff::lines_patch(&staged, 0, &added, true).unwrap();
        backend.apply_patch(&patch, true).unwrap();
        assert_eq!(repo.stdout(&["show", ":f"]), "a\nb\n");
        assert_eq!(fs::read_to_string(repo.path.join("f")).unwrap(), "a\nB\nc\n");
    }
}

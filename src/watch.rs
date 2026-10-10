//! Filesystem watcher: tells the event loop when the repo changed on disk so
//! the status buffer can refresh without a manual `g`.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};

use git2::Repository;
use notify::{recommended_watcher, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub struct RepoWatcher {
    // Dropping the watcher stops it.
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

impl RepoWatcher {
    /// `None` when the watcher can't be set up; the user can still refresh by hand.
    pub fn start(root: &Path) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        // Events carry resolved paths, so a symlinked root must be resolved too.
        let root = root.canonicalize().ok()?;
        // Used to skip build output and other ignored paths. Absent for jj.
        let repo = Repository::open(&root).ok();
        let watch_root = root.clone();
        let mut watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            if event.paths.iter().any(|p| is_relevant(&root, p, repo.as_ref())) {
                let _ = tx.send(());
            }
        })
        .ok()?;
        watcher.watch(&watch_root, RecursiveMode::Recursive).ok()?;
        Some(Self { _watcher: watcher, rx })
    }

    /// Whether anything relevant changed since the last call.
    pub fn take_changed(&self) -> bool {
        self.rx.try_iter().count() > 0
    }
}

/// Inside `.git`, only the index, HEAD and refs matter; everything else there
/// (objects, lock files, logs) is churn. Outside it, skip ignored paths.
fn is_relevant(root: &Path, path: &Path, repo: Option<&Repository>) -> bool {
    let Ok(rel) = path.strip_prefix(root) else { return false };
    let mut parts = rel.components();
    match parts.next().and_then(|c| c.as_os_str().to_str()) {
        Some(".git") => {
            let rest = parts.as_path();
            rest == Path::new("index")
                || rest == Path::new("HEAD")
                || rest == Path::new("packed-refs")
                || rest.starts_with("refs")
        }
        Some(".jj") => false,
        _ => !repo.is_some_and(|r| r.status_should_ignore(rel).unwrap_or(false)),
    }
}

#[cfg(test)]
mod tests {
    use super::RepoWatcher;
    use crate::backend::git::tests::TestRepo;
    use std::time::{Duration, Instant};

    fn wait_for_change(watcher: &RepoWatcher) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if watcher.take_changed() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn reports_worktree_edits_but_not_ignored_files() {
        let repo = TestRepo::new();
        repo.commit(".gitignore", "build/\n", "ignore build");
        std::fs::create_dir(repo.path.join("build")).unwrap();
        // Let the commit's own events settle before watching.
        std::thread::sleep(Duration::from_millis(500));
        let watcher = RepoWatcher::start(&repo.path).expect("watcher starts");
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(repo.path.join("build/out.o"), "x").unwrap();
        assert!(!wait_for_change_for(&watcher, Duration::from_millis(800)), "ignored path must not notify");

        std::fs::write(repo.path.join("new.txt"), "x").unwrap();
        assert!(wait_for_change(&watcher), "tracked-area edit must notify");
    }

    fn wait_for_change_for(watcher: &RepoWatcher, wait: Duration) -> bool {
        std::thread::sleep(wait);
        watcher.take_changed()
    }
}

//! Opening on the last map at once, and remembering each scan to show what changed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tidy_core::history::{self, Summary};
use tidy_core::snapshot::Snapshot;
use tidy_core::Tree;

use crate::App;

fn dir() -> PathBuf {
    crate::assistant::support_dir().join("history")
}

fn last_path() -> PathBuf {
    crate::assistant::support_dir().join("last.json")
}

/// Filled off the UI thread at launch: the last scan's tree (from its snapshot) and history.
pub static BOOT: Mutex<Option<(Tree, Vec<Summary>)>> = Mutex::new(None);
/// Histories read in the background for a scan brought on screen.
static LOADED: Mutex<Option<(PathBuf, Vec<Summary>)>> = Mutex::new(None);

/// At launch, on a background thread: rebuilds the last map from its snapshot.
pub fn boot(waker: rune::Waker) {
    std::thread::spawn(move || {
        let Some(root) = std::fs::read(last_path()).ok().and_then(|b| serde_json::from_slice::<PathBuf>(&b).ok()) else { return };
        let Ok(snap) = Snapshot::read(&crate::assistant::snapshot_path()) else { return };
        if Path::new(&snap.root) != root {
            return;
        }
        let tree = history::tree_from_snapshot(&snap);
        let hist = history::load(&dir(), &root);
        *BOOT.lock().unwrap() = Some((tree, hist));
        waker();
    });
}

impl App {
    /// A fresh scan finished: remember it, and where to open next time.
    pub fn remember_scan(&mut self, tree: &Tree) {
        let summary = Summary::of(tree);
        let root = tree.root_path.clone();
        self.history.push(summary.clone());
        if cfg!(test) {
            return; // tests never touch the person's history
        }
        std::thread::spawn(move || {
            let _ = history::append(&dir(), &root, summary);
            if let Ok(json) = serde_json::to_vec(&root) {
                let _ = std::fs::write(last_path(), json);
            }
        });
    }

    /// Reads the history of the scan now on screen, in the background.
    pub fn load_history(&mut self, root: &Path) {
        self.history.clear();
        if cfg!(test) {
            return;
        }
        let root = root.to_path_buf();
        let waker = self.waker.clone();
        std::thread::spawn(move || {
            let h = history::load(&dir(), &root);
            *LOADED.lock().unwrap() = Some((root, h));
            if let Some(w) = waker {
                w();
            }
        });
    }

    /// Lands a history read in the background, and the launch's last map. True when changed.
    pub fn poll_history(&mut self) -> bool {
        let mut changed = false;
        if let Some((root, h)) = LOADED.lock().unwrap().take()
            && self.tree.as_ref().is_some_and(|t| t.root_path == root)
        {
            // Keep a summary this session already added on top of what was on disk.
            let mut all = h;
            for s in self.history.drain(..) {
                if !all.iter().any(|x| x.scanned_at == s.scanned_at) {
                    all.push(s);
                }
            }
            self.history = all;
            changed = true;
        }
        if self.tree.is_none()
            && self.job.is_none()
            && self.pending.is_none()
            && let Some((tree, hist)) = BOOT.lock().unwrap().take()
        {
            let root = tree.root_path.clone();
            self.from_snapshot = true;
            self.history = hist;
            self.show_tree(tree, &root, false);
            // Then a fresh scan, with the last map on screen meanwhile.
            self.start_scan(root);
            changed = true;
        }
        changed
    }
}


//! The Duplicates view's state: the search (reading files, so it runs when asked), its results,
//! and which extra copies are ticked.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use tidy_core::dupes::{Group, Progress};

use crate::App;

#[derive(Default)]
pub struct DupesState {
    pub search: Option<(Arc<Progress>, Receiver<Option<Vec<Group>>>)>,
    pub groups: Vec<Group>,
    /// The scan the results belong to; a new scan makes them stale.
    pub for_root: Option<PathBuf>,
    pub picked: BTreeSet<PathBuf>,
}

impl App {
    pub fn find_dupes(&mut self) {
        let Some(tree) = &self.tree else { return };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        // The candidates come from the tree on this thread (no disk access); reading runs aside.
        let cands = tidy_core::dupes::candidates(tree, home.as_deref());
        let progress = Arc::new(Progress::default());
        let (tx, rx) = channel();
        let (p, waker) = (progress.clone(), self.waker.clone());
        std::thread::spawn(move || {
            let r = tidy_core::dupes::find(cands, home.as_deref(), &p);
            let _ = tx.send(r);
            if let Some(w) = waker {
                w();
            }
        });
        self.dupes.search = Some((progress, rx));
        self.dupes.for_root = Some(tree.root_path.clone());
    }

    pub fn stop_dupes(&mut self) {
        if let Some((p, _)) = self.dupes.search.take() {
            p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// True when something changed; asks for a progress tick while searching.
    pub fn poll_dupes(&mut self, tx: &mut rune::Tx) -> bool {
        let Some((_, rx)) = &self.dupes.search else { return false };
        match rx.try_recv() {
            Ok(r) => {
                self.dupes.search = None;
                if let Some(groups) = r {
                    // Every copy but the one to keep comes ticked.
                    self.dupes.picked = groups.iter().flat_map(|g| g.copies.iter().skip(1).map(|c| c.path.clone())).collect();
                    self.dupes.groups = groups;
                }
                true
            }
            Err(_) => {
                tx.wake_after(std::time::Duration::from_millis(250));
                true
            }
        }
    }

    /// Ticks or unticks one copy, never leaving a group with none kept.
    pub fn toggle_copy(&mut self, group: usize, path: &PathBuf) {
        if !self.dupes.picked.remove(path) {
            let g = &self.dupes.groups[group];
            let others_kept = g.copies.iter().filter(|c| &c.path != path && !self.dupes.picked.contains(&c.path)).count();
            if others_kept == 0 {
                self.say("Keep at least one copy");
                return;
            }
            self.dupes.picked.insert(path.clone());
        }
    }

    pub fn trash_dupes(&mut self) {
        let paths: Vec<String> = self.dupes.picked.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        if paths.is_empty() {
            return;
        }
        self.trash(&paths);
        let gone: BTreeSet<PathBuf> = self.dupes.picked.iter().filter(|p| !p.exists()).cloned().collect();
        for g in &mut self.dupes.groups {
            g.copies.retain(|c| !gone.contains(&c.path));
        }
        self.dupes.groups.retain(|g| g.copies.len() > 1);
        self.dupes.picked.retain(|p| !gone.contains(p));
    }
}

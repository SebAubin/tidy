//! The Developer view's state: findings from the scan and from the tools, what's ticked, and
//! the cleaning that runs in the background once the person confirms.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, channel};

use tidy_core::dev::{Action, Finding, WorktreeDir};

use crate::App;

#[derive(Default)]
pub struct DevState {
    /// From the scan, by the scan's root folder, so going back to a kept scan has them.
    pub by_root: HashMap<PathBuf, (Vec<Finding>, Vec<WorktreeDir>)>,
    /// What the view shows: the scan's findings and the tools' answers.
    pub findings: Vec<Finding>,
    /// Answers from the tools (git, simctl, docker) as they come, while they're being asked.
    pub checking: Option<Receiver<Update>>,
    /// Background askers still running (tools, worktrees).
    pub askers: usize,
    /// Findings whose default tick was already decided, so an update doesn't undo a choice.
    seen: HashSet<String>,
    pub picked: BTreeSet<String>,
    pub cleaning: Option<Cleaning>,
    /// The last cleaning's failures: what and why.
    pub failures: Vec<(String, String)>,
}

/// One answer from the background checks.
pub enum Update {
    /// New or refreshed findings, replacing any with the same id.
    Upsert(Vec<Finding>),
    /// Not a finding after all (a submodule's `.git` file).
    Remove(String),
    /// One asker finished.
    Done,
}

pub struct Cleaning {
    pub total: usize,
    pub done: usize,
    pub freed: u64,
    rx: Receiver<(Finding, Result<(), String>)>,
}

/// The login shell's PATH, for git, xcrun, docker, brew… (a Finder-launched app has a bare one).
pub fn path_env() -> Option<&'static str> {
    static P: OnceLock<Option<String>> = OnceLock::new();
    P.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        std::process::Command::new(shell)
            .args(["-lc", "printf %s \"$PATH\""])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .filter(|p| !p.is_empty())
    })
    .as_deref()
}

/// Still waiting on git for this one.
pub fn checking(f: &Finding) -> bool {
    f.blocked.as_deref() == Some(tidy_core::dev::CHECKING)
}

impl DevState {
    /// Ticked findings that will actually be cleaned: not blocked, and not inside another
    /// ticked one (a build folder inside a worktree being removed).
    pub fn effective(&self) -> BTreeSet<String> {
        let covered: BTreeSet<&str> = tidy_core::dev::covered(&self.findings, |f| self.picked.contains(&f.id)).into_iter().collect();
        self.findings
            .iter()
            .filter(|f| self.picked.contains(&f.id) && f.blocked.is_none() && !covered.contains(f.id.as_str()))
            .map(|f| f.id.clone())
            .collect()
    }

    /// The ticked finding that already takes this one with it.
    pub fn covered_by(&self, f: &Finding) -> Option<&Finding> {
        let p = f.path.as_deref()?;
        self.findings.iter().find(|o| o.id != f.id && self.picked.contains(&o.id) && o.path.as_deref().is_some_and(|op| op != p && p.starts_with(op)))
    }

    pub fn total(&self, ids: &BTreeSet<String>) -> u64 {
        self.findings.iter().filter(|f| ids.contains(&f.id)).map(|f| f.size).sum()
    }

    fn pick_defaults(&mut self, days: u32) {
        self.picked = self.findings.iter().filter(|f| f.suggested(days)).map(|f| f.id.clone()).collect();
        self.seen = self.findings.iter().filter(|f| !checking(f)).map(|f| f.id.clone()).collect();
    }

    /// Lands findings from the checks: new ones get their default tick, known ones keep the
    /// person's choice.
    fn upsert(&mut self, fresh: Vec<Finding>, days: u32) {
        for f in fresh {
            if !checking(&f) && self.seen.insert(f.id.clone()) && f.suggested(days) {
                self.picked.insert(f.id.clone());
            }
            if f.blocked.is_some() {
                self.picked.remove(&f.id);
            }
            match self.findings.iter_mut().find(|x| x.id == f.id) {
                Some(x) => *x = f,
                None => self.findings.push(f),
            }
        }
        self.findings.sort_by(|a, b| a.category.cmp(&b.category).then(b.size.cmp(&a.size)));
    }

    /// Worktrees git hasn't answered about yet.
    pub fn pending(&self) -> (usize, usize) {
        let wts = self.findings.iter().filter(|f| f.category == tidy_core::dev::Category::Worktrees);
        let (mut waiting, mut all) = (0, 0);
        for f in wts {
            all += 1;
            if checking(f) {
                waiting += 1;
            }
        }
        (waiting, all)
    }
}

impl App {
    /// Shows the findings for the scan on screen at once (worktrees as "checking"), then asks
    /// the tools in the background and lands each answer as it comes.
    pub fn start_dev(&mut self, root: &Path) {
        let (tree_findings, worktrees) = self.dev.by_root.get(root).cloned().unwrap_or_default();
        let mut shown = tree_findings.clone();
        shown.extend(worktrees.iter().map(tidy_core::dev::worktree_placeholder));
        shown.sort_by(|a, b| a.category.cmp(&b.category).then(b.size.cmp(&a.size)));
        self.dev.findings = shown;
        self.dev.pick_defaults(self.settings.stale_days);
        let (tx, rx) = channel();
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let wake = {
            let w = self.waker.clone();
            move || {
                if let Some(w) = &w {
                    w();
                }
            }
        };
        // Xcode, Android, Docker: a few seconds.
        {
            let (tx, wake) = (tx.clone(), wake.clone());
            std::thread::spawn(move || {
                let mut tf = tree_findings;
                let mut found = tidy_core::dev::tools(&mut tf, path_env(), &home);
                found.extend(tf.into_iter().filter(|f| f.id.starts_with("sysimg:")));
                let _ = tx.send(Update::Upsert(found));
                let _ = tx.send(Update::Done);
                wake();
            });
        }
        // git, four worktrees at a time, each answer landing on its own.
        std::thread::spawn(move || {
            use rayon::prelude::*;
            let ask = || {
                worktrees.par_iter().for_each(|wt| {
                    let u = match tidy_core::dev::check_worktree(wt, path_env()) {
                        Some(f) => Update::Upsert(vec![f]),
                        None => Update::Remove(format!("wt:{}", wt.path.display())),
                    };
                    let _ = tx.send(u);
                    wake();
                })
            };
            match tidy_core::dev::git_pool() {
                Some(p) => p.install(ask),
                None => ask(),
            }
            let _ = tx.send(Update::Done);
            wake();
        });
        self.dev.askers = 2;
        self.dev.checking = Some(rx);
    }

    /// The staleness setting changed: tick again what it now suggests.
    pub fn refresh_dev(&mut self) {
        self.dev.pick_defaults(self.settings.stale_days);
    }

    /// Cleans the ticked findings, one after another, off the UI thread.
    pub fn clean_dev(&mut self) {
        if self.dev.cleaning.is_some() {
            return;
        }
        let ids = self.dev.effective();
        let todo: Vec<Finding> = self.dev.findings.iter().filter(|f| ids.contains(&f.id)).cloned().collect();
        if todo.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let waker = self.waker.clone();
        let total = todo.len();
        std::thread::spawn(move || {
            // Trash moves go in one Finder operation (one sound); tools run one by one.
            let (trash, tools): (Vec<Finding>, Vec<Finding>) = todo.into_iter().partition(|f| matches!(f.action, Action::Trash { .. }));
            let paths: Vec<PathBuf> = trash.iter().flat_map(|f| match &f.action {
                Action::Trash { paths } => paths.clone(),
                _ => Vec::new(),
            }).collect();
            let failed = tidy_core::move_to_trash(&paths);
            for f in trash {
                let err = failed.iter().find(|(p, _)| matches!(&f.action, Action::Trash { paths } if paths.contains(p))).map(|(_, e)| e.clone());
                let _ = tx.send((f, err.map_or(Ok(()), Err)));
            }
            if let Some(w) = &waker {
                w();
            }
            for f in tools {
                let r = tidy_core::dev::clean(&f, path_env());
                let _ = tx.send((f, r));
                if let Some(w) = &waker {
                    w();
                }
            }
        });
        self.dev.failures.clear();
        self.dev.cleaning = Some(Cleaning { total, done: 0, freed: 0, rx });
    }

    /// Lands the tools' answers and cleaning progress. True when something changed.
    pub fn poll_dev(&mut self) -> bool {
        let mut changed = false;
        let updates: Vec<Update> = self.dev.checking.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for u in updates {
            changed = true;
            match u {
                Update::Upsert(fs) => self.dev.upsert(fs, self.settings.stale_days),
                Update::Remove(id) => {
                    self.dev.findings.retain(|f| f.id != id);
                    self.dev.picked.remove(&id);
                }
                Update::Done => {
                    self.dev.askers = self.dev.askers.saturating_sub(1);
                    if self.dev.askers == 0 {
                        self.dev.checking = None;
                        self.write_snapshot();
                    }
                }
            }
        }
        let mut landed = Vec::new();
        if let Some(c) = &self.dev.cleaning {
            landed.extend(c.rx.try_iter());
        }
        for (f, r) in landed {
            changed = true;
            let c = self.dev.cleaning.as_mut().unwrap();
            c.done += 1;
            match r {
                Ok(()) => {
                    c.freed += f.size;
                    if matches!(f.action, Action::Trash { .. }) {
                        self.in_trash += f.size;
                    }
                    // Gone with it: the finding and anything inside its folder.
                    self.dev.findings.retain(|x| x.id != f.id && !(f.path.is_some() && x.path.as_deref().is_some_and(|p| f.path.as_deref().is_some_and(|fp| p.starts_with(fp)))));
                    self.dev.picked.remove(&f.id);
                    if let Some(p) = &f.path
                        && !p.exists()
                    {
                        self.forget(p);
                    }
                }
                Err(e) => self.dev.failures.push((f.title.clone(), e)),
            }
        }
        if let Some(c) = &self.dev.cleaning
            && c.done == c.total
        {
            let (n, freed) = (c.total - self.dev.failures.len(), c.freed);
            crate::telemetry::track(
                "developer_cleanup",
                serde_json::json!({ "items": crate::telemetry::count_bucket(n as u64), "size": crate::telemetry::size_bucket(freed), "failed": crate::telemetry::count_bucket(self.dev.failures.len() as u64) }),
            );
            self.dev.cleaning = None;
            self.space = self.place.as_deref().and_then(tidy_core::volume::space).or(self.space);
            self.write_snapshot();
            self.say(if self.dev.failures.is_empty() {
                format!("Cleaned {} · {}", crate::plural(n, "item"), tidy_core::format::bytes(freed))
            } else {
                format!("Cleaned {}, {} couldn't be", crate::plural(n, "item"), self.dev.failures.len())
            });
        }
        changed
    }

    /// Takes a removed folder off the scans in memory.
    pub fn forget(&mut self, p: &Path) {
        for t in self.tree.iter_mut().chain(self.shelf.iter_mut()) {
            if let Some(id) = t.find(p) {
                t.remove(id);
            }
        }
        // Back out of a folder that's gone.
        if let Some(t) = &self.tree
            && t.ancestry(self.focus).first() != Some(&tidy_core::Tree::ROOT)
        {
            self.focus = tidy_core::Tree::ROOT;
            self.selected = None;
        }
        for (f, w) in self.dev.by_root.values_mut() {
            f.retain(|x| x.path.as_deref() != Some(p));
            w.retain(|x| x.path != p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidy_core::dev::{WorktreeDir, worktree_placeholder};

    #[test]
    fn a_worktree_shows_at_once_then_takes_git_s_answer() {
        let wt = WorktreeDir { path: "/r/.claude/worktrees/old".into(), size: 5_000_000_000, activity: 0 };
        let mut d = DevState { findings: vec![worktree_placeholder(&wt)], ..Default::default() };
        d.pick_defaults(30);
        assert!(d.picked.is_empty(), "nothing ticked while git hasn't answered");
        assert_eq!(d.pending(), (1, 1));
        let mut answered = worktree_placeholder(&wt);
        answered.blocked = None;
        answered.detail = "r · feature/x".into();
        d.upsert(vec![answered.clone()], 30);
        assert_eq!(d.pending(), (0, 1));
        assert!(d.picked.contains(&answered.id), "stale and clean: ticked once known");
        d.picked.remove(&answered.id);
        d.upsert(vec![answered.clone()], 30);
        assert!(!d.picked.contains(&answered.id), "a later answer keeps the person's choice");
    }
}

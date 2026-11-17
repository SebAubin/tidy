//! What changed between scans: a small summary of each scan (folders of 50 MB or more, a few
//! levels deep), kept per scanned folder, and the comparison of the scan on screen with one of
//! them. Also rebuilds a browsable tree from the last snapshot, so the app opens on the last
//! map at once while a fresh scan runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::kind::Kind;
use crate::scan::{Node, NodeId, Tree, now};
use crate::snapshot::Snapshot;

const MIN: u64 = 50_000_000;
const DEPTH: usize = 4;
/// Summaries kept per scanned folder.
const KEEP: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub scanned_at: i64,
    pub total: u64,
    pub files: u64,
    /// Folder paths relative to the scanned root ("" is the root), and their sizes.
    pub folders: HashMap<String, u64>,
}

impl Summary {
    pub fn of(tree: &Tree) -> Summary {
        let mut folders = HashMap::new();
        let mut stack: Vec<(NodeId, String, usize)> = vec![(Tree::ROOT, String::new(), 0)];
        while let Some((id, rel, depth)) = stack.pop() {
            let n = tree.node(id);
            folders.insert(rel.clone(), n.size);
            if depth == DEPTH {
                continue;
            }
            for &c in &n.children {
                let cn = tree.node(c);
                if cn.is_dir && cn.size >= MIN {
                    let r = if rel.is_empty() { cn.name.to_string() } else { format!("{rel}/{}", cn.name) };
                    stack.push((c, r, depth + 1));
                }
            }
        }
        Summary { scanned_at: tree.scanned_at, total: tree.total(), files: tree.node(Tree::ROOT).files, folders }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Relative to the scanned root.
    pub rel: String,
    pub before: u64,
    pub after: u64,
}

impl Change {
    pub fn delta(&self) -> i64 {
        self.after as i64 - self.before as i64
    }
}

/// Folders whose size moved since `old`, biggest moves first. A folder whose move is mostly one
/// subfolder's is left out for that subfolder, so the list points at where the change is.
pub fn compare(old: &Summary, new: &Summary) -> Vec<Change> {
    let mut all: Vec<Change> = new
        .folders
        .iter()
        .map(|(rel, &after)| Change { rel: rel.clone(), before: old.folders.get(rel).copied().unwrap_or(0), after })
        .chain(old.folders.iter().filter(|(rel, _)| !new.folders.contains_key(*rel)).map(|(rel, &before)| Change { rel: rel.clone(), before, after: 0 }))
        // Below the summary's floor on one side only, the other side is unknown: skip unless big.
        .filter(|c| c.delta().unsigned_abs() >= 100_000_000)
        .collect();
    let deltas: HashMap<String, i64> = all.iter().map(|c| (c.rel.clone(), c.delta())).collect();
    all.retain(|c| {
        let d = c.delta();
        let prefix = if c.rel.is_empty() { String::new() } else { format!("{}/", c.rel) };
        // Leave out a folder whose largest child moved most of the same way.
        !deltas.iter().any(|(r, &cd)| {
            r.starts_with(&prefix) && r.len() > prefix.len() && !r[prefix.len()..].contains('/') && cd.signum() == d.signum() && cd.abs() * 10 >= d.abs() * 7
        })
    });
    all.sort_by(|a, b| b.delta().abs().cmp(&a.delta().abs()));
    all
}

/// The summary to compare with: the newest one at least `age` seconds older than `now_at`
/// (or simply the previous scan when `age` is 0).
pub fn baseline(history: &[Summary], current_at: i64, age: i64) -> Option<&Summary> {
    history.iter().filter(|s| s.scanned_at < current_at && current_at - s.scanned_at >= age).max_by_key(|s| s.scanned_at)
}

fn key(root: &Path) -> String {
    // FNV-1a: a stable file name per scanned folder.
    let h = root.to_string_lossy().bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}.json")
}

pub fn load(dir: &Path, root: &Path) -> Vec<Summary> {
    std::fs::read(dir.join(key(root))).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Adds `s` to the folder's history and writes it (call off the UI thread).
pub fn append(dir: &Path, root: &Path, s: Summary) -> std::io::Result<Vec<Summary>> {
    let mut all = load(dir, root);
    all.push(s);
    all.sort_by_key(|x| x.scanned_at);
    if all.len() > KEEP {
        all.drain(..all.len() - KEEP);
    }
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{}.tmp", key(root)));
    std::fs::write(&tmp, serde_json::to_vec(&all)?)?;
    std::fs::rename(tmp, dir.join(key(root)))?;
    Ok(all)
}

/// A tree rebuilt from a snapshot: every folder and file it kept (1 MB and up), with sizes as
/// scanned. Smaller items show as the gap between a folder and its children.
pub fn tree_from_snapshot(s: &Snapshot) -> Tree {
    let root = PathBuf::from(&s.root);
    let mut nodes: Vec<Node> = Vec::with_capacity(s.entries.len() + 1);
    let mut index: HashMap<String, NodeId> = HashMap::new();
    let mut entries: Vec<&crate::snapshot::Entry> = s.entries.iter().collect();
    // Parents before children.
    entries.sort_by_key(|e| e.path.len());
    for e in entries {
        let parent = if e.path == s.root { None } else { Path::new(&e.path).parent().map(|p| p.to_string_lossy().into_owned()) };
        let parent_id = match &parent {
            Some(p) => match index.get(p) {
                Some(&id) => Some(id),
                None => continue,
            },
            None => None,
        };
        if parent_id.is_none() && !nodes.is_empty() {
            continue;
        }
        let name: Box<str> = if parent_id.is_none() { s.root.clone().into() } else { Path::new(&e.path).file_name().map(|n| n.to_string_lossy().into()).unwrap_or_default() };
        let id = nodes.len() as NodeId;
        nodes.push(Node {
            name,
            parent: parent_id,
            children: Vec::new(),
            size: e.size,
            files: e.files,
            is_dir: e.dir,
            kind: Kind::parse(&e.kind).unwrap_or(Kind::Other),
            modified: e.modified,
            accessed: e.accessed,
        });
        if let Some(p) = parent_id {
            nodes[p as usize].children.push(id);
        }
        index.insert(e.path.clone(), id);
    }
    for i in 0..nodes.len() {
        let mut ch = std::mem::take(&mut nodes[i].children);
        ch.sort_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size));
        nodes[i].children = ch;
    }
    if nodes.is_empty() {
        nodes.push(Node { name: s.root.clone().into(), parent: None, children: Vec::new(), size: s.total, files: s.files, is_dir: true, kind: Kind::Folder, modified: 0, accessed: 0 });
    }
    Tree { root_path: root, nodes, elapsed_ms: 0, denied: 0, skipped: Vec::new(), scanned_at: if s.scanned_at > 0 { s.scanned_at } else { now() } }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(at: i64, f: &[(&str, u64)]) -> Summary {
        Summary { scanned_at: at, total: 0, files: 0, folders: f.iter().map(|(r, s)| ((*r).to_owned(), *s)).collect() }
    }

    #[test]
    fn points_at_the_folder_that_grew() {
        let g = 1_000_000_000;
        let old = summary(1, &[("", 100 * g), ("Library", 40 * g), ("Library/Developer", 10 * g), ("Movies", 20 * g)]);
        let new = summary(2, &[("", 125 * g), ("Library", 70 * g), ("Library/Developer", 39 * g), ("Movies", 15 * g)]);
        let c = compare(&old, &new);
        let rels: Vec<&str> = c.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(rels, vec!["Library/Developer", "Movies"], "the root and Library grew because of Developer");
        assert_eq!(c[0].delta(), 29 * g as i64);
        let hist = vec![summary(10, &[]), summary(100, &[]), summary(200, &[])];
        assert_eq!(baseline(&hist, 250, 0).unwrap().scanned_at, 200);
        assert_eq!(baseline(&hist, 250, 100).unwrap().scanned_at, 100);
        assert!(baseline(&hist, 250, 1000).is_none());
    }

    #[test]
    fn rebuilds_a_tree_from_a_snapshot() {
        let dir = std::env::temp_dir().join(format!("tidy-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/big.bin"), vec![1u8; 3_000_000]).unwrap();
        std::fs::write(dir.join("small.txt"), b"x").unwrap();
        let t = crate::scan(&dir, Default::default()).unwrap();
        let snap = Snapshot::build(&t, &[], &[]);
        let back = tree_from_snapshot(&snap);
        let id = back.find(&dir.join("a/b/big.bin")).expect("the big file is there");
        assert_eq!(back.node(id).size, t.node(t.find(&dir.join("a/b/big.bin")).unwrap()).size);
        assert_eq!(back.total(), t.total(), "folder sizes are as scanned");
        assert!(back.find(&dir.join("small.txt")).is_none(), "small items are left out");
        let h = append(&dir.join("hist"), &dir, Summary::of(&t)).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(load(&dir.join("hist"), &dir).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

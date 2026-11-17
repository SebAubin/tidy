//! Duplicate files: the same bytes stored more than once.
//!
//! Cheap to expensive, so most files are never read: files are grouped by size from the scan,
//! then by a hash of their first and last 16 KB, and only what still matches is hashed in full
//! (BLAKE3). Reading stays on this Mac. Places where copies are expected or managed by an app
//! (app bundles, Photos and Music libraries, `~/Library`, hidden folders, build output) are left
//! out, as are git checkouts, so what's found is the person's own files.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::scan::{NodeId, Tree};

#[derive(Clone, Debug)]
pub struct Copy {
    pub path: PathBuf,
    pub modified: i64,
}

#[derive(Clone, Debug)]
pub struct Group {
    /// Bytes of each copy.
    pub size: u64,
    /// The one to keep first: outside Downloads, Desktop and the Trash, then the oldest.
    pub copies: Vec<Copy>,
}

impl Group {
    /// What removing every copy but one frees.
    pub fn extra(&self) -> u64 {
        self.size * (self.copies.len() as u64).saturating_sub(1)
    }
}

#[derive(Default)]
pub struct Progress {
    /// Bytes to read in the worst case, and read so far.
    pub total: AtomicU64,
    pub done: AtomicU64,
    pub cancel: AtomicBool,
}

const MIN: u64 = 1_000_000;
const EDGE: u64 = 16 * 1024;

/// Folders whose copies aren't the person's to dedupe.
fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || matches!(name, "node_modules" | "target" | "DerivedData" | "Pods" | "bin" | "obj" | "build" | "venv" | "__pycache__")
        || [".app", ".photoslibrary", ".musiclibrary", ".tvlibrary", ".fcpbundle", ".imovielibrary", ".logicx", ".xcarchive", ".framework", ".bundle"]
            .iter()
            .any(|e| name.ends_with(e))
}

/// Files that could have a twin: same size as another, at least 1 MB, outside skipped places.
pub fn candidates(tree: &Tree, home: Option<&Path>) -> Vec<Vec<(NodeId, PathBuf, i64)>> {
    let library = home.map(|h| h.join("Library"));
    let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
    // Walk folders, not every node, so a skipped folder's whole subtree is skipped.
    let mut stack = vec![Tree::ROOT];
    while let Some(d) = stack.pop() {
        for &c in &tree.node(d).children {
            let n = tree.node(c);
            if n.is_dir {
                // A git checkout's files are the repository's, copied on purpose into each
                // worktree: never the person's duplicates to remove.
                let checkout = n.children.iter().any(|&g| &*tree.node(g).name == ".git");
                if checkout || skipped(&n.name) || library.as_ref().is_some_and(|l| tree.path(c) == *l) {
                    continue;
                }
                stack.push(c);
            } else if n.size >= MIN && !n.name.starts_with('.') {
                by_size.entry(n.size).or_default().push(c);
            }
        }
    }
    by_size
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|v| v.into_iter().map(|id| (id, tree.path(id), tree.node(id).modified)).collect())
        .collect()
}

fn edges(path: &Path, size: u64) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; EDGE.min(size) as usize];
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    if size > EDGE * 2 {
        f.seek(SeekFrom::End(-(EDGE as i64))).ok()?;
        f.read_exact(&mut buf).ok()?;
        h.update(&buf);
    }
    Some(*h.finalize().as_bytes())
}

fn full(path: &Path, progress: &Progress) -> Option<[u8; 32]> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.cancel.load(Ordering::Relaxed) {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        progress.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    Some(*h.finalize().as_bytes())
}

fn keep_order(home: Option<&Path>) -> impl Fn(&Copy) -> (u8, i64) + '_ {
    move |c: &Copy| {
        let transient = home.is_some_and(|h| ["Downloads", "Desktop", ".Trash"].iter().any(|d| c.path.starts_with(h.join(d))));
        (transient as u8, c.modified)
    }
}

/// Finds the duplicates among `candidates`. Returns None when cancelled.
pub fn find(candidates: Vec<Vec<(NodeId, PathBuf, i64)>>, home: Option<&Path>, progress: &Progress) -> Option<Vec<Group>> {
    use rayon::prelude::*;
    let total: u64 = candidates.iter().flatten().map(|(_, p, _)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
    progress.total.store(total, Ordering::Relaxed);
    // Reading is disk-bound: a few at a time.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().ok()?;
    let groups = pool.install(|| {
        candidates
            .into_par_iter()
            .flat_map_iter(|same_size| {
                if progress.cancel.load(Ordering::Relaxed) {
                    return Vec::new();
                }
                let size = std::fs::metadata(&same_size[0].1).map(|m| m.len()).unwrap_or(0);
                // First and last 16 KB.
                let mut by_edges: HashMap<[u8; 32], Vec<(PathBuf, i64)>> = HashMap::new();
                for (_, p, m) in same_size {
                    if let Some(e) = edges(&p, size) {
                        by_edges.entry(e).or_default().push((p, m));
                    }
                }
                let mut out = Vec::new();
                for (_, files) in by_edges.into_iter().filter(|(_, v)| v.len() > 1) {
                    // Whole files, only where the edges matched.
                    let mut by_hash: HashMap<[u8; 32], Vec<Copy>> = HashMap::new();
                    for (p, m) in files {
                        if let Some(h) = if size <= EDGE * 2 { edges(&p, size) } else { full(&p, progress) } {
                            by_hash.entry(h).or_default().push(Copy { path: p, modified: m });
                        }
                    }
                    for (_, mut copies) in by_hash.into_iter().filter(|(_, v)| v.len() > 1) {
                        copies.sort_by_key(keep_order(home));
                        out.push(Group { size, copies });
                    }
                }
                // Files that turned out different count as read too.
                out
            })
            .collect::<Vec<_>>()
    });
    if progress.cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut groups = groups;
    groups.sort_by(|a, b| b.extra().cmp(&a.extra()));
    progress.done.store(total, Ordering::Relaxed);
    Some(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_true_duplicates_only() {
        let dir = std::env::temp_dir().join(format!("tidy-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("Downloads")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        std::fs::create_dir_all(dir.join("repo/.git")).unwrap();
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let mut other = big.clone();
        other[1_500_000] ^= 1; // same size, same edges, different middle
        std::fs::write(dir.join("a/movie.mov"), &big).unwrap();
        std::fs::write(dir.join("Downloads/movie copy.mov"), &big).unwrap();
        std::fs::write(dir.join("a/almost.mov"), &other).unwrap();
        std::fs::write(dir.join("node_modules/movie.mov"), &big).unwrap();
        std::fs::write(dir.join("repo/movie.mov"), &big).unwrap();
        let tree = crate::scan(&dir, Default::default()).unwrap();
        let c = candidates(&tree, Some(&dir));
        let groups = find(c, Some(&dir), &Progress::default()).unwrap();
        assert_eq!(groups.len(), 1, "the near-twin and the node_modules copy aren't counted");
        let g = &groups[0];
        assert_eq!(g.copies.len(), 2);
        assert!(g.copies[0].path.ends_with("a/movie.mov"), "keeps the one outside Downloads");
        assert_eq!(g.extra(), 3_000_000);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

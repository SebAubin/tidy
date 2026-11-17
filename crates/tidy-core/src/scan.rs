//! Walks a folder in parallel and builds a size tree.
//!
//! Sizes are what the files take on disk (allocated blocks), not their logical length, so sparse
//! files and APFS clones don't inflate the picture. A hard-linked file counts once. The walk stays
//! on the volume it starts on, so `/` doesn't descend into other mounts.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::kind::Kind;

pub type NodeId = u32;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Box<str>,
    pub parent: Option<NodeId>,
    /// Sorted by size, largest first.
    pub children: Vec<NodeId>,
    /// Bytes on disk, including everything below.
    pub size: u64,
    /// Files below (1 for a file).
    pub files: u64,
    pub is_dir: bool,
    pub kind: Kind,
    /// Seconds since the Unix epoch.
    pub modified: i64,
    pub accessed: i64,
}

/// A finished scan.
#[derive(Clone, Debug)]
pub struct Tree {
    pub root_path: PathBuf,
    pub nodes: Vec<Node>,
    pub elapsed_ms: u64,
    /// Folders that couldn't be read (permissions, mostly).
    pub denied: u64,
    /// Folders left out on purpose (privacy-guarded ones, without Full Disk Access).
    pub skipped: Vec<PathBuf>,
    /// When the scan finished, seconds since the epoch.
    pub scanned_at: i64,
}

impl Tree {
    pub const ROOT: NodeId = 0;

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = Some(id);
        while let Some(i) = cur {
            if i == Self::ROOT {
                break;
            }
            let n = self.node(i);
            parts.push(&*n.name);
            cur = n.parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    /// The node at `path`, if it's inside this tree.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let rel = path.strip_prefix(&self.root_path).ok()?;
        let mut cur = Self::ROOT;
        for part in rel.components() {
            let name = part.as_os_str().to_str()?;
            cur = *self.node(cur).children.iter().find(|&&c| &*self.node(c).name == name)?;
        }
        Some(cur)
    }

    /// From the root down to `id`.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut cur = self.node(id).parent;
        while let Some(i) = cur {
            out.push(i);
            cur = self.node(i).parent;
        }
        out.reverse();
        out
    }

    pub fn total(&self) -> u64 {
        self.node(Self::ROOT).size
    }

    /// Every file, largest first, at least `min` bytes.
    pub fn largest_files(&self, min: u64, limit: usize) -> Vec<NodeId> {
        let mut v: Vec<NodeId> =
            (0..self.nodes.len() as u32).filter(|&i| !self.nodes[i as usize].is_dir && self.nodes[i as usize].size >= min).collect();
        v.sort_unstable_by(|a, b| self.node(*b).size.cmp(&self.node(*a).size));
        v.truncate(limit);
        v
    }

    /// Bytes per kind of file.
    pub fn by_kind(&self) -> Vec<(Kind, u64, u64)> {
        let mut acc = [(0u64, 0u64); Kind::ALL.len()];
        for n in &self.nodes {
            if !n.is_dir {
                let i = n.kind as usize;
                acc[i].0 += n.size;
                acc[i].1 += 1;
            }
        }
        let mut v: Vec<_> = Kind::ALL.iter().map(|&k| (k, acc[k as usize].0, acc[k as usize].1)).filter(|x| x.1 > 0).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    /// Takes a node out of the tree (after it was trashed), fixing sizes above it.
    pub fn remove(&mut self, id: NodeId) {
        if id == Self::ROOT {
            return;
        }
        let (size, files, parent) = {
            let n = self.node(id);
            (n.size, n.files, n.parent)
        };
        if let Some(p) = parent {
            self.nodes[p as usize].children.retain(|&c| c != id);
        }
        let mut cur = parent;
        while let Some(i) = cur {
            let n = &mut self.nodes[i as usize];
            n.size = n.size.saturating_sub(size);
            n.files = n.files.saturating_sub(files);
            cur = n.parent;
        }
        // The node stays in the arena, detached; ids elsewhere remain valid.
        self.nodes[id as usize].parent = None;
        self.nodes[id as usize].size = 0;
    }
}

/// A folder directly inside the scanned one, as measured so far.
#[derive(Clone, Debug)]
pub struct TopEntry {
    pub name: Box<str>,
    /// Bytes counted so far (all of it once `done`).
    pub size: u64,
    pub done: bool,
}

/// Live counters a UI reads while a scan runs.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
    pub current: Mutex<String>,
    /// The scanned folder's own subfolders, growing as their contents are measured, so a UI can
    /// draw the map as it fills in.
    pub top: Mutex<Vec<TopEntry>>,
}

impl Progress {
    fn grow_top(&self, name: &str, by: u64, done: bool) {
        let mut top = self.top.lock().unwrap();
        match top.iter_mut().find(|e| &*e.name == name) {
            Some(e) if done => {
                e.size = by;
                e.done = true;
            }
            Some(e) => e.size += by,
            None => top.push(TopEntry { name: name.into(), size: by, done }),
        }
    }
}

impl Progress {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// A subtree built by one worker; merged into the arena by its parent.
struct Sub {
    nodes: Vec<Node>,
}

struct Ctx<'a> {
    dev: u64,
    progress: &'a Progress,
    links: Mutex<HashSet<(u64, u64)>>,
    denied: AtomicU64,
    skip: &'a [PathBuf],
}

fn secs(t: std::io::Result<SystemTime>) -> i64 {
    t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs() as i64)
}

/// Scans `root`. Returns `None` when cancelled.
pub fn scan(root: &Path, progress: Arc<Progress>) -> Option<Tree> {
    scan_skipping(root, progress, Vec::new())
}

/// Scans `root` without entering the folders in `leave_out` (never even opened, so macOS asks
/// nothing about them).
pub fn scan_skipping(root: &Path, progress: Arc<Progress>, leave_out: Vec<PathBuf>) -> Option<Tree> {
    let started = Instant::now();
    let meta = fs::symlink_metadata(root).ok()?;
    // Inside a volume's data firmlinks, `/System/Volumes/Data` mirrors `/`: skip it when scanning `/`.
    let mut skip: Vec<PathBuf> = if root == Path::new("/") {
        vec!["/System/Volumes".into(), "/Volumes".into(), "/dev".into(), "/private/var/vm".into()]
    } else {
        Vec::new()
    };
    skip.extend(leave_out.iter().cloned());
    let ctx = Ctx { dev: meta.dev(), progress: &progress, links: Mutex::new(HashSet::new()), denied: AtomicU64::new(0), skip: &skip };
    let name: Box<str> = root.to_string_lossy().into();
    let sub = walk_dir(&ctx, root, name, &meta, 0, None);
    if progress.cancelled() {
        return None;
    }
    let mut nodes = sub.nodes;
    sort_children(&mut nodes);
    Some(Tree {
        root_path: root.to_path_buf(),
        nodes,
        elapsed_ms: started.elapsed().as_millis() as u64,
        denied: ctx.denied.load(Ordering::Relaxed),
        skipped: leave_out,
        scanned_at: now(),
    })
}

fn sort_children(nodes: &mut [Node]) {
    for i in 0..nodes.len() {
        let mut ch = std::mem::take(&mut nodes[i].children);
        ch.sort_unstable_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size).then_with(|| nodes[*a as usize].name.cmp(&nodes[*b as usize].name)));
        nodes[i].children = ch;
    }
}

/// Builds the subtree for `dir`; its root is `nodes[0]`, ids are local to the returned vector.
fn walk_dir(ctx: &Ctx, dir: &Path, name: Box<str>, meta: &fs::Metadata, depth: u32, top: Option<&str>) -> Sub {
    let mut me = Node {
        name,
        parent: None,
        children: Vec::new(),
        size: meta.blocks() * 512,
        files: 0,
        is_dir: true,
        kind: Kind::Folder,
        modified: meta.mtime(),
        accessed: meta.atime(),
    };
    ctx.progress.dirs.fetch_add(1, Ordering::Relaxed);
    if ctx.progress.cancelled() {
        return Sub { nodes: vec![me] };
    }
    if let Ok(mut cur) = ctx.progress.current.try_lock() {
        cur.clear();
        cur.push_str(&dir.to_string_lossy());
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            ctx.denied.fetch_add(1, Ordering::Relaxed);
            return Sub { nodes: vec![me] };
        }
    };
    let mut files: Vec<Node> = Vec::new();
    let mut subdirs: Vec<(PathBuf, Box<str>, fs::Metadata)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(m) = entry.metadata() else { continue };
        let nm: Box<str> = entry.file_name().to_string_lossy().into();
        let ft = m.file_type();
        if ft.is_dir() {
            let p = entry.path();
            if m.dev() != ctx.dev || ctx.skip.iter().any(|s| s == &p) {
                continue;
            }
            subdirs.push((p, nm, m));
            continue;
        }
        let mut size = m.blocks() * 512;
        if ft.is_file() && m.nlink() > 1 && !ctx.links.lock().unwrap().insert((m.dev(), m.ino())) {
            size = 0;
        }
        ctx.progress.files.fetch_add(1, Ordering::Relaxed);
        ctx.progress.bytes.fetch_add(size, Ordering::Relaxed);
        let kind = if ft.is_symlink() { Kind::Other } else { Kind::of(&nm) };
        files.push(Node {
            name: nm,
            parent: None,
            children: Vec::new(),
            size,
            files: 1,
            is_dir: false,
            kind,
            modified: m.mtime(),
            accessed: m.atime(),
        });
    }
    // Recurse in parallel; small folders are cheap enough that rayon's splitting handles them.
    use rayon::prelude::*;
    let subs: Vec<Sub> = subdirs
        .into_par_iter()
        .map(|(p, nm, m)| {
            // Below the root, each subfolder reports to the top-level folder it's in.
            let top_name: Option<String> = if depth == 0 { Some(nm.to_string()) } else { top.map(str::to_owned) };
            let sub = walk_dir(ctx, &p, nm, &m, depth + 1, top_name.as_deref());
            match (depth, &top_name) {
                (0, Some(t)) => ctx.progress.grow_top(t, sub.nodes[0].size, true),
                (1, Some(t)) => ctx.progress.grow_top(t, sub.nodes[0].size, false),
                _ => {}
            }
            sub
        })
        .collect();

    // Bundles (`.app`, `.photoslibrary`, …) are folders on disk but read as one thing.
    me.kind = Kind::of_folder(&me.name);
    let mut nodes = Vec::with_capacity(1 + files.len() + subs.iter().map(|s| s.nodes.len()).sum::<usize>());
    nodes.push(me);
    for mut f in files {
        f.parent = Some(0);
        nodes[0].size += f.size;
        nodes[0].files += 1;
        nodes.push(f);
        let id = (nodes.len() - 1) as u32;
        nodes[0].children.push(id);
    }
    for sub in subs {
        let off = nodes.len() as u32;
        let (s, f) = (sub.nodes[0].size, sub.nodes[0].files);
        for mut n in sub.nodes {
            n.parent = Some(n.parent.map_or(0, |p| p + off));
            for c in &mut n.children {
                *c += off;
            }
            nodes.push(n);
        }
        nodes[0].children.push(off);
        nodes[0].size += s;
        nodes[0].files += f;
    }
    nodes[0].parent = None;
    Sub { nodes }
}

/// Now, in seconds since the epoch.
pub fn now() -> i64 {
    secs(Ok(SystemTime::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_a_small_tree_and_finds_paths() {
        let dir = std::env::temp_dir().join(format!("tidy-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/big.bin"), vec![1u8; 200_000]).unwrap();
        fs::write(dir.join("a/small.txt"), b"hi").unwrap();
        fs::write(dir.join("top.mp4"), vec![2u8; 50_000]).unwrap();
        let t = scan(&dir, Arc::new(Progress::default())).unwrap();
        assert_eq!(t.node(Tree::ROOT).files, 3);
        let big = t.find(&dir.join("a/b/big.bin")).unwrap();
        assert_eq!(t.path(big), dir.join("a/b/big.bin"));
        assert!(t.node(big).size >= 200_000);
        // Largest first.
        let root = t.node(Tree::ROOT);
        assert_eq!(&*t.node(root.children[0]).name, "a");
        let before = t.total();
        let mut t2 = t.clone();
        t2.remove(big);
        assert_eq!(t2.total(), before - t.node(big).size);
        assert_eq!(t.by_kind()[0].0, Kind::Library);
        fs::remove_dir_all(&dir).unwrap();
    }
}

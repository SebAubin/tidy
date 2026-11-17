//! A compact copy of a scan on disk, for the assistant's MCP server to read in its own process.
//!
//! It holds paths, sizes, dates and kinds of everything at least `min` bytes, never file
//! contents. It stays on this Mac: the assistant sees only what its tools return.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::kind::Kind;
use crate::scan::{NodeId, Tree, now};
use crate::{Suggestion, volume};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub dir: bool,
    pub kind: String,
    pub modified: i64,
    pub accessed: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KindTotal {
    pub kind: String,
    pub label: String,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub root: String,
    pub total: u64,
    pub files: u64,
    pub volume_total: u64,
    pub volume_free: u64,
    pub scanned_at: i64,
    /// Entries smaller than this were left out.
    pub min_size: u64,
    pub by_kind: Vec<KindTotal>,
    pub suggestions: Vec<serde_json::Value>,
    /// The Developer view's findings (worktrees, simulators, build output…).
    #[serde(default)]
    pub developer: Vec<serde_json::Value>,
    pub entries: Vec<Entry>,
}

const MAX_ENTRIES: usize = 150_000;

impl Snapshot {
    pub fn build(tree: &Tree, suggestions: &[Suggestion], developer: &[crate::dev::Finding]) -> Snapshot {
        // Start at 1 MB and raise the floor until the count fits.
        let mut min = 1u64 << 20;
        let live: Vec<NodeId> = (0..tree.nodes.len() as u32).filter(|&i| i == Tree::ROOT || tree.node(i).parent.is_some()).collect();
        loop {
            let n = live.iter().filter(|&&i| tree.node(i).size >= min).count();
            if n <= MAX_ENTRIES {
                break;
            }
            min *= 2;
        }
        let entries = live
            .iter()
            .filter(|&&i| tree.node(i).size >= min)
            .map(|&i| {
                let n = tree.node(i);
                Entry {
                    path: tree.path(i).to_string_lossy().into_owned(),
                    size: n.size,
                    files: n.files,
                    dir: n.is_dir,
                    kind: kind_name(n.kind),
                    modified: n.modified,
                    accessed: n.accessed,
                }
            })
            .collect();
        let space = volume::space(&tree.root_path).unwrap_or_default();
        Snapshot {
            root: tree.root_path.to_string_lossy().into_owned(),
            total: tree.total(),
            files: tree.node(Tree::ROOT).files,
            volume_total: space.total,
            volume_free: space.free,
            scanned_at: now(),
            min_size: min,
            by_kind: tree
                .by_kind()
                .into_iter()
                .map(|(k, bytes, files)| KindTotal { kind: kind_name(k), label: k.label().into(), bytes, files })
                .collect(),
            suggestions: suggestions.iter().filter_map(|s| serde_json::to_value(s).ok()).collect(),
            developer: developer
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "category": f.category.title(),
                        "name": f.title,
                        "detail": f.detail,
                        "size": crate::format::bytes(f.size),
                        "bytes": f.size,
                        "last_used": f.last_used.map(|t| crate::format::age(now() - t)).unwrap_or_else(|| "never".into()),
                        "path": f.path,
                        "how_tidy_cleans_it": f.action.label(),
                        "blocked": f.blocked,
                        "worth_keeping": f.keep,
                    })
                })
                .collect(),
            entries,
        }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn read(path: &Path) -> std::io::Result<Snapshot> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    /// Children of each folder, by the folder's path.
    pub fn children_index(&self) -> HashMap<&str, Vec<usize>> {
        let mut idx: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, e) in self.entries.iter().enumerate() {
            if let Some(parent) = Path::new(&e.path).parent().and_then(|p| p.to_str()) {
                idx.entry(parent).or_default().push(i);
            }
        }
        for v in idx.values_mut() {
            v.sort_by(|a, b| self.entries[*b].size.cmp(&self.entries[*a].size));
        }
        idx
    }
}

fn kind_name(k: Kind) -> String {
    serde_json::to_value(k).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

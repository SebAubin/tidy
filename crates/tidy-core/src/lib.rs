//! Tidy's engine: scanning, insights, the snapshot the assistant reads, and its MCP server.
//! No UI here, so the same code serves the window, the `tidy mcp` subprocess and tests.

pub mod access;
pub mod dev;
pub mod dupes;
pub mod format;
pub mod history;
pub mod insights;
pub mod kind;
pub mod mcp;
pub mod scan;
pub mod snapshot;
pub mod volume;

pub use insights::{Risk, Suggestion, suggestions};
pub use kind::Kind;
pub use scan::{Node, NodeId, Progress, Tree, scan, scan_skipping};

/// Moves `paths` to the Trash in one Finder operation (one sound, and "Put Back" works), never
/// deleting outright. Returns the paths that couldn't be moved, with why.
pub fn move_to_trash(paths: &[std::path::PathBuf]) -> Vec<(std::path::PathBuf, String)> {
    if paths.is_empty() || trash::delete_all(paths).is_ok() {
        return Vec::new();
    }
    // The batch failed as a whole: find which ones, moving what can be moved.
    paths
        .iter()
        .filter(|p| p.exists())
        .filter_map(|p| trash::delete(p).err().map(|e| (p.clone(), e.to_string())))
        .collect()
}

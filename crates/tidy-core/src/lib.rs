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

/// The person's home folder: `$HOME`, or `%USERPROFILE%` on Windows.
pub fn home_dir() -> std::path::PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).or_else(|| std::env::var_os("HOME")).map(std::path::PathBuf::from).unwrap_or_else(system_root)
}

/// The system volume's root: `/`, or `C:\` (the system drive) on Windows.
pub fn system_root() -> std::path::PathBuf {
    if cfg!(windows) {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        std::path::PathBuf::from(format!("{drive}\\"))
    } else {
        std::path::PathBuf::from("/")
    }
}

/// What the system volume is called where people see it.
pub fn system_volume_name() -> String {
    if cfg!(windows) {
        format!("Local Disk ({})", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()))
    } else {
        "Macintosh HD".into()
    }
}

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

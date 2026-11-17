//! macOS privacy protection (TCC): which folders ask for permission, and whether Tidy has Full
//! Disk Access, the one switch that covers them all.
//!
//! Without Full Disk Access, opening a protected folder pops a separate "Allow" dialog for each
//! one (Desktop, Documents, Downloads, other apps' data, iCloud Drive, removable volumes, …).
//! Rather than ask fifteen times, Tidy skips them and offers the single switch instead.

use std::path::{Path, PathBuf};

/// Whether this process can read what only Full Disk Access unlocks. Never prompts: these
/// locations have no per-folder dialog, they just fail without the grant.
pub fn has_full_disk_access() -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    let home = crate::home_dir();
    let probes = [
        home.join("Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Safari/Bookmarks.plist"),
        home.join("Library/Safari"),
        PathBuf::from("/Library/Application Support/com.apple.TCC/TCC.db"),
    ];
    probes.iter().filter(|p| p.exists()).any(|p| if p.is_dir() { std::fs::read_dir(p).is_ok() } else { std::fs::File::open(p).is_ok() })
}

/// A folder macOS guards with its own dialog, and what to call it.
#[derive(Clone, Debug)]
pub struct Guarded {
    pub path: PathBuf,
    pub label: &'static str,
}

/// The guarded folders under `root` (or containing it), as they exist on this Mac.
pub fn guarded_under(root: &Path) -> Vec<Guarded> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let home = crate::home_dir();
    let list: [(&str, &'static str); 17] = [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures/Photos Library.photoslibrary", "Photos"),
        ("Library/Mobile Documents", "iCloud Drive"),
        ("Library/Containers", "Other apps' data"),
        ("Library/Group Containers", "Other apps' shared data"),
        ("Library/Mail", "Mail"),
        ("Library/Messages", "Messages"),
        ("Library/Calendars", "Calendars"),
        ("Library/Application Support/AddressBook", "Contacts"),
        ("Library/Reminders", "Reminders"),
        ("Library/Safari", "Safari"),
        ("Library/Application Support/com.apple.TCC", "Privacy settings"),
        ("Library/HomeKit", "Home"),
        ("Library/Cookies", "Cookies"),
        ("Library/Application Support/CallHistoryDB", "Call history"),
    ];
    let mut out: Vec<Guarded> = list
        .iter()
        .map(|(rel, label)| Guarded { path: home.join(rel), label })
        .filter(|g| g.path.exists() && g.path.starts_with(root) && g.path != root)
        .collect();
    // External and network volumes ask too.
    if root == Path::new("/") {
        out.push(Guarded { path: "/Volumes".into(), label: "Other volumes" });
    }
    out
}

/// Opens System Settings at Privacy & Security › Full Disk Access.
pub fn open_full_disk_access_settings() {
    let _ = std::process::Command::new("open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles").spawn();
}

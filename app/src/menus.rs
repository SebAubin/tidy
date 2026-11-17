//! The menu bar: Tidy's commands where a Mac user looks for them, with Finder's shortcuts where
//! Finder has the same command (⌘↑ Enclosing Folder, ⇧⌘H Home, ⌘⌫ Move to Trash).

use std::path::PathBuf;

use rune::{MenuItem, Shortcut, Tx};

use crate::{App, Section, settings};

fn key(s: &str) -> Shortcut {
    s.parse().unwrap_or_else(|_| panic!("bad shortcut {s}"))
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into())
}

fn go(s: &mut App, rel: &str) {
    let p = if rel.starts_with('/') { PathBuf::from(rel) } else { home().join(rel) };
    s.go_to(p);
}

fn section(s: &mut App, sec: Section) {
    if s.tree.is_some() {
        s.section = sec;
    }
}

fn selection(s: &App) -> Vec<String> {
    let ids: Vec<_> = if s.section == Section::LargeFiles && !s.picked.is_empty() { s.picked.iter().copied().collect() } else { s.selected.into_iter().collect() };
    ids.into_iter().filter_map(|id| s.path_of(id)).map(|p| p.to_string_lossy().into_owned()).collect()
}

pub fn items() -> Vec<MenuItem<App>> {
    vec![
        // App menu, after About.
        MenuItem::separator(),
        MenuItem::new("Settings…", |_: &mut App, _: &mut Tx| settings::open()).id("settings").key_equivalent(key("cmd+,")),
        // File
        MenuItem::new("Scan Home Folder", |s: &mut App, _: &mut Tx| go(s, "")).id("scan-home").menu("File"),
        MenuItem::new("Scan Macintosh HD", |s: &mut App, _: &mut Tx| go(s, "/")).id("scan-disk").menu("File"),
        MenuItem::new("Choose Folder to Scan…", |_: &mut App, tx: &mut Tx| crate::ui::pick_folder(tx)).id("scan-pick").menu("File").key_equivalent(key("cmd+o")),
        MenuItem::separator().menu("File"),
        MenuItem::new("Scan Again", |s: &mut App, _: &mut Tx| s.rescan()).id("rescan").menu("File").key_equivalent(key("cmd+r")),
        MenuItem::new("Stop Scanning", |s: &mut App, _: &mut Tx| s.cancel_scan()).id("stop").menu("File").key_equivalent(key("cmd+.")),
        MenuItem::separator().menu("File"),
        MenuItem::new("Show in Finder", |s: &mut App, _: &mut Tx| {
            let mut c = std::process::Command::new("open");
            c.arg("-R").args(selection(s));
            let _ = c.spawn();
        })
        .id("reveal")
        .menu("File")
        .key_equivalent(key("cmd+alt+r")),
        MenuItem::new("Move to Trash", |s: &mut App, _: &mut Tx| {
            let paths = selection(s);
            if !paths.is_empty() {
                s.trash(&paths);
                s.picked.clear();
            }
        })
        .id("trash")
        .menu("File")
        .key_equivalent(key("cmd+backspace")),
        // View
        MenuItem::new("Space Map", |s: &mut App, _: &mut Tx| section(s, Section::Map)).id("v-map").menu("View").key_equivalent(key("cmd+1")),
        MenuItem::new("Suggestions", |s: &mut App, _: &mut Tx| section(s, Section::Suggestions)).id("v-sugg").menu("View").key_equivalent(key("cmd+2")),
        MenuItem::new("Large Files", |s: &mut App, _: &mut Tx| section(s, Section::LargeFiles)).id("v-large").menu("View").key_equivalent(key("cmd+3")),
        MenuItem::new("Duplicates", |s: &mut App, _: &mut Tx| section(s, Section::Duplicates)).id("v-dupes").menu("View").key_equivalent(key("cmd+4")),
        MenuItem::new("Developer", |s: &mut App, _: &mut Tx| section(s, Section::Developer)).id("v-dev").menu("View").key_equivalent(key("cmd+5")),
        MenuItem::new("Changes", |s: &mut App, _: &mut Tx| section(s, Section::Changes)).id("v-changes").menu("View").key_equivalent(key("cmd+6")),
        MenuItem::separator().menu("View"),
        MenuItem::new("Show Assistant", |s: &mut App, _: &mut Tx| s.inspector.toggle()).id("assistant").menu("View").key_equivalent(key("cmd+j")),
        MenuItem::new("Show Sidebar", |s: &mut App, _: &mut Tx| s.sidebar.toggle()).id("sidebar").menu("View").key_equivalent(key("cmd+ctrl+s")),
        // Go
        MenuItem::new("Enclosing Folder", |s: &mut App, _: &mut Tx| s.go_up()).id("up").menu("Go").key_equivalent(key("cmd+up")),
        MenuItem::separator().menu("Go"),
        MenuItem::new("Home", |s: &mut App, _: &mut Tx| go(s, "")).id("go-home").menu("Go").key_equivalent(key("cmd+shift+h")),
        MenuItem::new("Desktop", |s: &mut App, _: &mut Tx| go(s, "Desktop")).id("go-desktop").menu("Go").key_equivalent(key("cmd+shift+d")),
        MenuItem::new("Documents", |s: &mut App, _: &mut Tx| go(s, "Documents")).id("go-docs").menu("Go").key_equivalent(key("cmd+shift+o")),
        MenuItem::new("Downloads", |s: &mut App, _: &mut Tx| go(s, "Downloads")).id("go-dl").menu("Go").key_equivalent(key("cmd+alt+l")),
        MenuItem::new("Library", |s: &mut App, _: &mut Tx| go(s, "Library")).id("go-lib").menu("Go"),
        MenuItem::new("Applications", |s: &mut App, _: &mut Tx| go(s, "/Applications")).id("go-apps").menu("Go").key_equivalent(key("cmd+shift+a")),
        // Help
        MenuItem::new("Full Disk Access…", |_: &mut App, _: &mut Tx| tidy_core::access::open_full_disk_access_settings()).id("help-fda").menu("Help"),
    ]
}

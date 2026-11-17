//! Space that is usually safe to reclaim, found offline from well-known locations and names.
//! Nothing here deletes anything: these are suggestions a person (or the assistant) reviews.

use std::path::Path;

use crate::scan::{NodeId, Tree, now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Rebuilt or re-downloaded automatically when needed.
    Safe,
    /// Probably unneeded, but worth a look first.
    Review,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Suggestion {
    #[serde(skip)]
    pub node: NodeId,
    pub path: String,
    #[serde(skip)]
    pub size: u64,
    #[serde(rename = "size")]
    pub size_text: String,
    pub title: &'static str,
    pub why: String,
    pub risk: Risk,
}

const MB: u64 = 1 << 20;
const DAY: i64 = 86_400;

/// Folders under the home folder that tools rebuild on demand.
const HOME_CACHES: &[(&str, &str, &str)] = &[
    ("Library/Caches", "App caches", "Apps rebuild their caches when they need them."),
    ("Library/Developer/Xcode/DerivedData", "Xcode build data", "Xcode rebuilds it on the next build."),
    ("Library/Developer/Xcode/iOS DeviceSupport", "iOS device support", "Symbols for devices you plugged in once; Xcode fetches them again."),
    ("Library/Developer/Xcode/watchOS DeviceSupport", "watchOS device support", "Xcode fetches them again when needed."),
    ("Library/Developer/CoreSimulator/Caches", "Simulator caches", "Rebuilt by the simulator."),
    ("Library/Logs", "Logs", "Old diagnostic logs."),
    (".npm/_cacache", "npm cache", "npm downloads packages again when needed."),
    (".cache", "Tool caches", "Command-line tools keep downloads here and recreate them."),
    (".cargo/registry/cache", "Cargo downloads", "Cargo re-downloads crates when a build needs them."),
    (".gradle/caches", "Gradle caches", "Gradle re-downloads dependencies when needed."),
    (".m2/repository", "Maven repository", "Maven re-downloads dependencies when needed."),
    (".Trash", "Trash", "Files already in the Trash still take space until it's emptied."),
    ("Library/Application Support/Code/CachedData", "VS Code cache", "Rebuilt by VS Code."),
    ("Library/Containers/com.docker.docker/Data/vms", "Docker disk", "Docker's VM disk; prune images with `docker system prune` instead of deleting it."),
    // Windows.
    ("AppData/Local/Temp", "Temporary files", "Apps leave these behind; whatever is still needed gets recreated."),
    ("AppData/Local/CrashDumps", "Crash dumps", "Copies of crashed apps' memory, kept for debugging."),
    ("AppData/Local/Microsoft/Windows/INetCache", "Internet cache", "Web content cached by Windows and Edge; fetched again when needed."),
    ("AppData/Local/D3DSCache", "Shader cache", "Compiled graphics shaders; games and apps rebuild them."),
    ("AppData/Local/npm-cache", "npm cache", "npm downloads packages again when needed."),
    ("AppData/Local/pip/Cache", "pip cache", "pip downloads packages again when needed."),
    ("AppData/Local/Yarn/Cache", "Yarn cache", "Yarn downloads packages again when needed."),
    ("AppData/Roaming/Code/Cache", "VS Code cache", "Rebuilt by VS Code."),
    ("AppData/Roaming/Code/CachedData", "VS Code cache", "Rebuilt by VS Code."),
];

pub fn suggestions(tree: &Tree, home: Option<&Path>) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut push = |node: NodeId, title: &'static str, why: String, risk: Risk| {
        let n = tree.node(node);
        if n.size >= 20 * MB {
            out.push(Suggestion { node, path: tree.path(node).to_string_lossy().into_owned(), size: n.size, size_text: crate::format::bytes(n.size), title, why, risk });
        }
    };
    if let Some(home) = home {
        for (rel, title, why) in HOME_CACHES {
            if let Some(id) = tree.find(&home.join(rel)) {
                let risk = if rel.contains("docker") { Risk::Review } else { Risk::Safe };
                push(id, title, (*why).into(), risk);
            }
        }
    }
    let t = now();
    let downloads = home.map(|h| h.join("Downloads"));
    // Project build output and package folders, anywhere.
    for (i, n) in tree.nodes.iter().enumerate() {
        let id = i as NodeId;
        if n.parent.is_none() && id != Tree::ROOT {
            continue; // removed
        }
        let parent_has = |name: &str| n.parent.is_some_and(|p| tree.node(p).children.iter().any(|&c| &*tree.node(c).name == name));
        if n.is_dir {
            match &*n.name {
                "node_modules" if parent_has("package.json") && !ancestor_named(tree, id, "node_modules") => {
                    push(id, "node_modules", "Reinstalled with `npm install` (or pnpm/yarn) from package.json.".into(), Risk::Safe)
                }
                "target" if parent_has("Cargo.toml") => push(id, "Rust build output", "Rebuilt by `cargo build`.".into(), Risk::Safe),
                ".venv" | "venv" if parent_has("pyproject.toml") || parent_has("requirements.txt") => {
                    push(id, "Python virtualenv", "Recreated from the project's requirements.".into(), Risk::Safe)
                }
                ".next" | ".turbo" | ".parcel-cache" | ".nuxt" if parent_has("package.json") => {
                    push(id, "Web build cache", "Rebuilt on the next dev or build run.".into(), Risk::Safe)
                }
                "Pods" if parent_has("Podfile") => push(id, "CocoaPods", "Restored by `pod install`.".into(), Risk::Safe),
                "build" | "bin" | "obj" if parent_has("build.gradle") || parent_has("build.gradle.kts") || has_csproj(tree, n.parent) => {
                    push(id, "Build output", "Rebuilt by the project's build.".into(), Risk::Safe)
                }
                _ => {}
            }
        } else if let Some(dl) = &downloads {
            let in_downloads = n.parent.is_some_and(|p| tree.path(p) == *dl);
            let ext = n.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
            if in_downloads && matches!(ext.as_str(), "dmg" | "pkg" | "zip" | "iso" | "xip" | "msi" | "exe") {
                let age = (t - n.modified) / DAY;
                push(id, "Old installer in Downloads", format!("Downloaded {age} days ago; the app it installs is likely already installed."), Risk::Review);
            } else if in_downloads && t - n.accessed > 180 * DAY && n.size >= 200 * MB {
                push(id, "Untouched download", format!("Not opened in {} days.", (t - n.accessed) / DAY), Risk::Review);
            }
        }
    }
    // A suggestion inside another one is already counted.
    out.sort_by_key(|a| a.path.len());
    let mut kept: Vec<Suggestion> = Vec::new();
    for s in out {
        if !kept.iter().any(|k| Path::new(&s.path).starts_with(&k.path)) {
            kept.push(s);
        }
    }
    kept.sort_by(|a, b| b.size.cmp(&a.size));
    kept
}

fn ancestor_named(tree: &Tree, id: NodeId, name: &str) -> bool {
    let mut cur = tree.node(id).parent;
    while let Some(i) = cur {
        if &*tree.node(i).name == name {
            return true;
        }
        cur = tree.node(i).parent;
    }
    false
}

fn has_csproj(tree: &Tree, parent: Option<NodeId>) -> bool {
    parent.is_some_and(|p| tree.node(p).children.iter().any(|&c| tree.node(c).name.ends_with(".csproj")))
}

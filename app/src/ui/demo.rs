//! Deterministic scenes for headless snapshots (`tidy --snap out.png --scene map`): a made-up home
//! folder, so screenshots and tests don't depend on this Mac's disk.

use tidy_core::{Kind, Node, Tree};

use crate::{App, Msg, Proposal, ProposalItem, Section};

pub static SCENES: [(&str, fn(&mut App)); 9] = [
    ("welcome", |_| {}),
    ("map", |s| load(s, Section::Map)),
    ("suggestions", |s| load(s, Section::Suggestions)),
    ("large", |s| load(s, Section::LargeFiles)),
    ("scanning", scanning),
    ("changes", |s| {
        load(s, Section::Changes);
        let tree = s.tree.as_ref().unwrap();
        let mut old = tidy_core::history::Summary::of(tree);
        old.scanned_at -= 9 * 86_400;
        let g = GB;
        let mut set = |rel: &str, f: &dyn Fn(u64) -> u64| {
            if let Some(v) = old.folders.get_mut(rel) {
                *v = f(*v);
            }
        };
        set("", &|v| v - 31 * g);
        set("Library", &|v| v - 23 * g);
        set("Library/Developer", &|v| v - 22 * g);
        set("Library/Developer/CoreSimulator", &|v| v - 12 * g);
        set("Movies", &|v| v - 6 * g);
        set("Downloads", &|v| v + 4 * g);
        s.history = vec![old];
    }),
    ("duplicates", |s| {
        load(s, Section::Duplicates);
        dupes(s);
    }),
    ("developer", |s| {
        load(s, Section::Developer);
        developer(s);
    }),
    ("chat", |s| {
        load(s, Section::Map);
        chat(s);
    }),
];

const GB: u64 = 1_000_000_000;
const MB: u64 = 1_000_000;

const FILES: &[(&str, u64)] = &[
    ("Library/Caches/com.spotify.client/Data/cache.db", 6 * GB),
    ("Library/Caches/Google/Chrome/Default/Cache/data_1", 3 * GB),
    ("Library/Caches/Homebrew/downloads/llvm.tar.gz", 2 * GB),
    ("Library/Developer/Xcode/DerivedData/Tidy-abc/Build/Products.o", 9 * GB),
    ("Library/Developer/Xcode/iOS DeviceSupport/18.2/Symbols.dylib", 6 * GB),
    ("Library/Developer/CoreSimulator/Devices/A1/data/disk.img", 14 * GB),
    ("Library/Containers/com.docker.docker/Data/vms/0/Docker.raw", 32 * GB),
    ("Library/Mail/V10/Mailboxes/INBOX.mbox/data.db", 5 * GB),
    ("Library/Application Support/Slack/Cache/blob.bin", 2 * GB),
    ("Pictures/Photos Library.photoslibrary/originals/A/IMG_0001.heic", 48 * GB),
    ("Movies/Final Cut/Trip to Gaspésie.mov", 38 * GB),
    ("Movies/Screen Recordings/demo-2024.mov", 6 * GB),
    ("Music/Music/Media/Albums.m4a", 12 * GB),
    ("Downloads/Xcode_16.2.xip", 7 * GB),
    ("Downloads/Docker.dmg", 600 * MB),
    ("Downloads/ubuntu-24.04-desktop-arm64.iso", 3 * GB),
    ("Downloads/Photos export.zip", 4 * GB),
    ("Documents/Taxes 2025.pdf", 40 * MB),
    ("Documents/Backups/iPhone backup.sparsebundle", 22 * GB),
    ("Documents/Design/Brand.psd", 900 * MB),
    ("Documents/github/rune-app/target/debug/deps/librune.rlib", 11 * GB),
    ("Documents/github/rune-app/src/main.rs", 40 * MB),
    ("Documents/github/website/node_modules/next/dist/bundle.js", 2 * GB),
    ("Documents/github/website/package.json", 20 * MB),
    ("Documents/github/website/.next/cache/webpack.pack", GB),
    ("Documents/github/ml/.venv/lib/torch/libtorch.dylib", 5 * GB),
    ("Documents/github/ml/requirements.txt", 20 * MB),
    ("Documents/github/ml/data/train.parquet", 9 * GB),
    ("Desktop/Screenshot 2026-03-02.png", 30 * MB),
    (".npm/_cacache/content-v2/sha512.bin", 3 * GB),
    (".cargo/registry/cache/index.crates.io/serde.crate", 2 * GB),
    (".Trash/Old Project.zip", 5 * GB),
];

pub fn demo_tree() -> Tree {
    let root = tidy_core::home_dir();
    let mut nodes = vec![node(&root.to_string_lossy(), None, true, 0)];
    let old = tidy_core::scan::now() - 400 * 86_400;
    for (path, size) in FILES {
        let mut cur = 0u32;
        let parts: Vec<&str> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let leaf = i + 1 == parts.len();
            let found = nodes[cur as usize].children.iter().copied().find(|&c| &*nodes[c as usize].name == *part);
            cur = match found {
                Some(c) => c,
                None => {
                    let mut n = node(part, Some(cur), !leaf, if leaf { *size } else { 0 });
                    n.accessed = old;
                    nodes.push(n);
                    let id = (nodes.len() - 1) as u32;
                    nodes[cur as usize].children.push(id);
                    id
                }
            };
        }
        // Sizes up the chain.
        let mut up = nodes[cur as usize].parent;
        while let Some(p) = up {
            nodes[p as usize].size += size;
            nodes[p as usize].files += 1;
            up = nodes[p as usize].parent;
        }
    }
    for i in 0..nodes.len() {
        let mut ch = std::mem::take(&mut nodes[i].children);
        ch.sort_by(|a, b| nodes[*b as usize].size.cmp(&nodes[*a as usize].size));
        nodes[i].children = ch;
    }
    Tree { root_path: root, nodes, elapsed_ms: 900, denied: 0, skipped: Vec::new(), scanned_at: tidy_core::scan::now() - 180 }
}

fn node(name: &str, parent: Option<u32>, dir: bool, size: u64) -> Node {
    Node {
        name: name.into(),
        parent,
        children: Vec::new(),
        size,
        files: if dir { 0 } else { 1 },
        is_dir: dir,
        kind: if dir { Kind::of_folder(name) } else { Kind::of(name) },
        modified: tidy_core::scan::now() - 200 * 86_400,
        accessed: 0,
    }
}

fn load(s: &mut App, section: Section) {
    let tree = demo_tree();
    s.place = Some(tree.root_path.clone());
    s.suggestions = tidy_core::suggestions(&tree, Some(&tree.root_path));
    s.ticked = s.suggestions.iter().filter(|x| x.risk == tidy_core::Risk::Safe).map(|x| x.path.clone()).collect();
    s.largest = tree.largest_files(10_000_000, 500);
    s.tree = Some(tree);
    s.section = section;
    s.chat.detected = Some(crate::assistant::Detected {
        providers: vec![crate::assistant::Provider { kind: crate::assistant::ProviderKind::ClaudeCode, path: "/usr/local/bin/claude".into() }],
        path_env: None,
    });
}

fn chat(s: &mut App) {
    let home = s.tree.as_ref().unwrap().root_path.clone();
    let p = |rel: &str| home.join(rel).to_string_lossy().into_owned();
    s.chat.model = "claude-opus-5-5".into();
    s.chat.messages = vec![
        Msg::User("What can I safely clean up?".into()),
        Msg::Activity("Reading the scan overview".into()),
        Msg::Activity("Looking inside ~/Library/Developer".into()),
        Msg::Assistant("About **38 GB** is easy to get back. Most of it is developer data that rebuilds itself: the iOS simulator, Xcode's build data and a Rust `target` folder.".into()),
        Msg::Proposal(0),
    ];
    let item = |rel: &str, size: u64, reason: &str, safe| ProposalItem { path: p(rel), reason: reason.into(), safe, size, checked: safe };
    s.chat.proposals = vec![Proposal {
        title: "Developer leftovers".into(),
        items: vec![
            item("Library/Developer/Xcode/DerivedData", 9 * GB, "Xcode rebuilds it on the next build.", true),
            item("Documents/github/rune-app/target", 11 * GB, "Rust build output; `cargo build` recreates it.", true),
            item("Downloads/Xcode_16.2.xip", 7 * GB, "Installer for an Xcode you already have.", false),
        ],
        done: None,
    }];
    s.chat.cost = 0.04;
}

fn developer(s: &mut App) {
    use tidy_core::dev::{Action, Category, Finding};
    let tree = s.tree.as_ref().unwrap();
    let (mut f, _) = tidy_core::dev::from_tree(tree, &tree.root_path);
    let now = tidy_core::scan::now();
    let home = tree.root_path.clone();
    let run = |label: &str| Action::Run { program: "true".into(), args: vec![], label: label.into() };
    let mk = |id: &str, category, title: &str, detail: &str, size: u64, days: Option<i64>, action, blocked: Option<&str>, keep| Finding {
        id: id.into(),
        category,
        title: title.into(),
        detail: detail.into(),
        size,
        last_used: days.map(|d| now - d * 86_400),
        path: None,
        action,
        blocked: None,
        keep,
        warning: blocked.map(str::to_owned),
    };
    let wt = |name: &str, branch: &str, size, days, blocked| {
        let mut x = mk(&format!("wt:{name}"), Category::Worktrees, name, &format!("website · {branch}"), size, Some(days), run("git worktree remove"), blocked, false);
        x.path = Some(home.join("Documents/github/website/.claude/worktrees").join(name));
        x
    };
    f.extend([
        wt("agent-3f9c21", "fix/login-redirect", 2 * GB, 41, None),
        wt("pricing-page", "feat/pricing-v2", 1_800 * MB, 63, None),
        wt("hotfix-1.4.2", "release/1.4.2", 1_200 * MB, 6, Some("3 uncommitted changes will be lost")),
        mk("rt:1", Category::Runtimes, "iOS 27.2", "Newest installed", 8_300 * MB, None, run("xcrun simctl runtime delete"), None, true),
        mk("rt:2", Category::Runtimes, "iOS 18.5", "Build 22F77", 8_800 * MB, Some(84), run("xcrun simctl runtime delete"), None, false),
        mk("rt:3", Category::Runtimes, "iOS 18.2", "Build 22C150", 8_700 * MB, None, run("xcrun simctl runtime delete"), None, false),
        mk("rt:4", Category::Runtimes, "watchOS 11.2", "Build 22S99", 4_500 * MB, None, run("xcrun simctl runtime delete"), None, false),
        mk("sim:1", Category::Simulators, "iPhone 17 Pro", "iOS 26.5", 5_800 * MB, Some(11), run("xcrun simctl delete"), None, false),
        mk("sim:2", Category::Simulators, "iPad Air 11-inch", "iOS 18.5 · runtime missing", 2_100 * MB, Some(210), run("xcrun simctl delete"), None, false),
        mk("docker:b", Category::Docker, "Build cache", "6.4GB total", 6_400 * MB, None, run("docker builder prune"), None, false),
    ]);
    f.sort_by(|a, b| a.category.cmp(&b.category).then(b.size.cmp(&a.size)));
    s.dev.findings = f;
    s.dev.picked = s.dev.findings.iter().filter(|x| x.suggested(30)).map(|x| x.id.clone()).collect();
}

fn scanning(s: &mut App) {
    use std::sync::atomic::Ordering;
    use tidy_core::scan::TopEntry;
    let p = std::sync::Arc::new(tidy_core::Progress::default());
    p.files.store(1_454_935, Ordering::Relaxed);
    p.bytes.store(205 * GB, Ordering::Relaxed);
    let e = |n: &str, size, done| TopEntry { name: n.into(), size, done };
    *p.top.lock().unwrap() = vec![
        e("Library", 96 * GB, false),
        e("Documents", 61 * GB, true),
        e("Pictures", 22 * GB, true),
        e("Movies", 12 * GB, true),
        e("Downloads", 8 * GB, true),
        e(".cargo", 3 * GB, true),
        e("Music", 2 * GB, true),
        e(".npm", GB, false),
    ];
    let home = tidy_core::home_dir();
    s.job = Some(crate::ScanJob {
        path: home,
        progress: p,
        started: std::time::Instant::now() - std::time::Duration::from_secs(37),
        result: Default::default(),
    });
}

fn dupes(s: &mut App) {
    use tidy_core::dupes::{Copy, Group};
    let home = s.tree.as_ref().unwrap().root_path.clone();
    let now = tidy_core::scan::now();
    let c = |rel: &str, days: i64| Copy { path: home.join(rel), modified: now - days * 86_400 };
    s.dupes.groups = vec![
        Group { size: 2_400 * MB, copies: vec![c("Movies/Trip to Gaspésie.mov", 400), c("Downloads/Trip to Gaspésie.mov", 30), c("Desktop/Trip to Gaspésie (1).mov", 12)] },
        Group { size: 640 * MB, copies: vec![c("Documents/Backups/photos-2023.zip", 700), c("Downloads/photos-2023.zip", 90)] },
        Group { size: 48 * MB, copies: vec![c("Documents/Taxes/2025 return.pdf", 200), c("Downloads/2025 return.pdf", 199)] },
    ];
    s.dupes.picked = s.dupes.groups.iter().flat_map(|g| g.copies.iter().skip(1).map(|c| c.path.clone())).collect();
    s.dupes.for_root = Some(home);
}

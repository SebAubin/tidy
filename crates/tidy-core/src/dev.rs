//! The Developer view's findings: what a developer's Mac accumulates and can let go of. Build
//! output, stale git worktrees, simulators and their runtimes, Android emulators, package caches,
//! old toolchains, Docker.
//!
//! Two passes. `from_tree` reads the scan (no disk access beyond a few small files) and runs on
//! the scan's thread. `from_system` asks the tools themselves (git, simctl, docker), which can
//! take seconds, on its own thread. Cleaning happens only when the person confirms, through the
//! tool that owns the thing when there is one (`git worktree remove`, `xcrun simctl delete`), else
//! by moving it to the Trash.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::scan::{NodeId, Tree, now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Worktrees,
    Builds,
    Simulators,
    Runtimes,
    Xcode,
    Android,
    Caches,
    Toolchains,
    Docker,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Worktrees,
        Category::Builds,
        Category::Runtimes,
        Category::Simulators,
        Category::Xcode,
        Category::Android,
        Category::Docker,
        Category::Caches,
        Category::Toolchains,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::Worktrees => "Git worktrees",
            Category::Builds => "Build output",
            Category::Simulators => "iOS simulators",
            Category::Runtimes => "Simulator runtimes",
            Category::Xcode => "Xcode",
            Category::Android => "Android emulators",
            Category::Caches => "Package caches",
            Category::Toolchains => "Toolchains",
            Category::Docker => "Docker",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            Category::Worktrees => {
                "Worktrees made for a branch or an agent often outlive it. Tidy removes them with `git worktree remove`, which refuses if anything is uncommitted; the branch itself stays."
            }
            Category::Builds => "Build folders of projects you haven't touched lately. Each rebuilds with the project's usual command.",
            Category::Simulators => "Simulator devices keep their apps and data. Delete the ones you no longer run; Xcode recreates a fresh one in a click.",
            Category::Runtimes => {
                "Each iOS or watchOS runtime takes 4 to 9 GB. Keep the newest; older ones re-download from Xcode › Settings › Components if a project needs them."
            }
            Category::Xcode => "Per-project build data, device symbols from old iOS versions, and archives of past builds.",
            Category::Android => "Emulators (AVDs) you haven't started lately, and system images no emulator uses.",
            Category::Caches => "Downloads package managers keep. They fetch again when a build needs them.",
            Category::Toolchains => "Toolchain versions besides your default.",
            Category::Docker => "Space Docker can reclaim itself: stopped containers, unused images and build cache.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Move these to the Trash.
    Trash { paths: Vec<PathBuf> },
    /// Run a tool that owns the thing (`git worktree remove …`).
    Run { program: String, args: Vec<String>, label: String },
}

impl Action {
    pub fn label(&self) -> String {
        match self {
            Action::Trash { .. } => if cfg!(windows) { "Move to Recycle Bin".into() } else { "Move to Trash".into() },
            Action::Run { label, .. } => label.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub id: String,
    pub category: Category,
    pub title: String,
    pub detail: String,
    /// What it occupies on disk, when known.
    pub size: u64,
    /// When it was last used (seconds since the epoch), when known.
    pub last_used: Option<i64>,
    /// The folder it is, for revealing it and taking it off the map once removed.
    pub path: Option<PathBuf>,
    pub action: Action,
    /// Why it can't be cleaned right now (uncommitted changes, in use).
    pub blocked: Option<String>,
    /// Worth keeping even if old (the newest runtime, the default toolchain).
    pub keep: bool,
    /// Cleaning it loses something (uncommitted changes): never ticked for the person, and said
    /// plainly next to it.
    pub warning: Option<String>,
}

/// Findings whose folder sits inside another picked finding's folder: cleaning the outer one
/// (a worktree) takes them too, so they aren't counted or cleaned twice.
pub fn covered(all: &[Finding], picked: impl Fn(&Finding) -> bool) -> Vec<&str> {
    let outer: Vec<&Path> = all.iter().filter(|f| picked(f)).filter_map(|f| f.path.as_deref()).collect();
    all.iter()
        .filter(|f| f.path.as_deref().is_some_and(|p| outer.iter().any(|o| p != *o && p.starts_with(o))))
        .map(|f| f.id.as_str())
        .collect()
}

impl Finding {
    /// Unused for longer than `days`, or never used.
    pub fn stale(&self, days: u32) -> bool {
        self.last_used.is_none_or(|t| now() - t > days as i64 * 86_400)
    }

    /// Ticked by default: stale, cleanable and not one to keep.
    pub fn suggested(&self, days: u32) -> bool {
        self.blocked.is_none() && self.warning.is_none() && !self.keep && self.stale(days)
    }
}

const MB: u64 = 1_000_000;

fn trash(p: &Path) -> Action {
    Action::Trash { paths: vec![p.to_path_buf()] }
}

fn run(program: &str, args: &[&str], label: &str) -> Action {
    Action::Run { program: program.into(), args: args.iter().map(|a| (*a).to_owned()).collect(), label: label.into() }
}

/// The newest change among a folder's direct children, other than `except`: when the project
/// was last worked on.
fn activity(tree: &Tree, dir: NodeId, except: NodeId) -> i64 {
    let n = tree.node(dir);
    n.children.iter().filter(|&&c| c != except).map(|&c| tree.node(c).modified).max().unwrap_or(n.modified).max(n.modified.min(0))
}

fn has_child(tree: &Tree, dir: NodeId, pred: impl Fn(&str) -> bool) -> bool {
    tree.node(dir).children.iter().any(|&c| pred(&tree.node(c).name))
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

/// A worktree found in the scan: its folder, size and last activity.
#[derive(Clone, Debug)]
pub struct WorktreeDir {
    pub path: PathBuf,
    pub size: u64,
    pub activity: i64,
}

/// What the scan alone tells: build folders, Xcode data, Android, caches, toolchains, and the
/// worktrees `from_system` then asks git about.
pub fn from_tree(tree: &Tree, home: &Path) -> (Vec<Finding>, Vec<WorktreeDir>) {
    let mut out = Vec::new();
    let mut worktrees = Vec::new();
    for (i, n) in tree.nodes.iter().enumerate() {
        let id = i as NodeId;
        let Some(parent) = n.parent else { continue };
        if !n.is_dir {
            // A `.git` *file* marks a linked worktree.
            if &*n.name == ".git" {
                let pn = tree.node(parent);
                worktrees.push(WorktreeDir { path: tree.path(parent), size: pn.size, activity: activity(tree, parent, id) });
            }
            continue;
        }
        if n.size < 50 * MB {
            continue;
        }
        let kind = match &*n.name {
            "target" if has_child(tree, parent, |c| c == "Cargo.toml") => Some(("Rust target/", "cargo build")),
            "node_modules" if has_child(tree, parent, |c| c == "package.json") && !ancestor_named(tree, id, "node_modules") => {
                Some(("node_modules", "npm install"))
            }
            ".next" | ".turbo" | ".nuxt" | ".parcel-cache" | ".svelte-kit" | ".angular" if has_child(tree, parent, |c| c == "package.json") => {
                Some(("web build cache", "the next dev run"))
            }
            ".venv" | "venv" if has_child(tree, parent, |c| c == "pyproject.toml" || c == "requirements.txt" || c == "setup.py") => {
                Some(("Python virtualenv", "pip install / uv sync"))
            }
            "Pods" if has_child(tree, parent, |c| c == "Podfile") => Some(("CocoaPods", "pod install")),
            ".build" if has_child(tree, parent, |c| c == "Package.swift") => Some(("SwiftPM .build", "swift build")),
            "build" | ".gradle" if has_child(tree, parent, |c| c == "build.gradle" || c == "build.gradle.kts" || c == "settings.gradle.kts") => {
                Some(("Gradle build", "gradle build"))
            }
            "bin" | "obj" if has_child(tree, parent, |c| c.ends_with(".csproj") || c.ends_with(".fsproj")) => Some((".NET bin/obj", "dotnet build")),
            _ => None,
        };
        if let Some((what, rebuild)) = kind {
            let project = tree.node(parent).name.to_string();
            let p = tree.path(id);
            out.push(Finding {
                id: format!("build:{}", p.display()),
                category: Category::Builds,
                title: project,
                detail: format!("{what} · rebuilt by {rebuild}"),
                size: n.size,
                last_used: Some(activity(tree, parent, id)),
                path: Some(p.clone()),
                action: trash(&p),
                blocked: None,
                keep: false,
                warning: None,
            });
        }
    }

    let child_dirs = |rel: &str| -> Vec<NodeId> {
        tree.find(&home.join(rel)).map(|d| tree.node(d).children.iter().copied().filter(|&c| tree.node(c).is_dir).collect()).unwrap_or_default()
    };
    let newest_change = |d: NodeId| tree.node(d).children.iter().map(|&c| tree.node(c).modified).max().unwrap_or(tree.node(d).modified);

    // Xcode: DerivedData per project, device symbols per iOS version, archives.
    for d in child_dirs("Library/Developer/Xcode/DerivedData") {
        let n = tree.node(d);
        if n.size < 20 * MB || &*n.name == "ModuleCache.noindex" {
            continue;
        }
        let p = tree.path(d);
        let project = n.name.rsplit_once('-').map_or(&*n.name, |(a, _)| a).to_owned();
        out.push(Finding {
            id: format!("dd:{}", p.display()),
            category: Category::Xcode,
            title: format!("DerivedData · {project}"),
            detail: "Build data; Xcode rebuilds it".into(),
            size: n.size,
            last_used: Some(newest_change(d)),
            path: Some(p.clone()),
            action: trash(&p),
            blocked: None,
            keep: false,
            warning: None,
        });
    }
    for os in ["iOS", "watchOS", "tvOS", "visionOS", "macOS"] {
        let mut versions = child_dirs(&format!("Library/Developer/Xcode/{os} DeviceSupport"));
        versions.sort_by_key(|&d| std::cmp::Reverse(tree.node(d).modified));
        for (rank, d) in versions.into_iter().enumerate() {
            let n = tree.node(d);
            let p = tree.path(d);
            out.push(Finding {
                id: format!("ds:{}", p.display()),
                category: Category::Xcode,
                title: format!("{os} device symbols · {}", n.name.split(' ').next().unwrap_or(&n.name)),
                detail: "Copied from a device you plugged in; fetched again when needed".into(),
                size: n.size,
                last_used: Some(n.modified),
                path: Some(p.clone()),
                action: trash(&p),
                blocked: None,
                keep: rank == 0,
                warning: None,
            });
        }
    }
    for d in child_dirs("Library/Developer/Xcode/Archives") {
        let n = tree.node(d);
        let p = tree.path(d);
        out.push(Finding {
            id: format!("ar:{}", p.display()),
            category: Category::Xcode,
            title: format!("Archives from {}", n.name),
            detail: "Past release builds; keep the ones you may need to symbolicate crashes".into(),
            size: n.size,
            last_used: Some(n.modified),
            path: Some(p.clone()),
            action: trash(&p),
            blocked: None,
            keep: true,
            warning: None,
        });
    }

    // Android: emulators, by when their disk last changed.
    for d in child_dirs(".android/avd") {
        let n = tree.node(d);
        if !n.name.ends_with(".avd") {
            continue;
        }
        let p = tree.path(d);
        let name = n.name.trim_end_matches(".avd").to_owned();
        let ini = p.with_extension("ini");
        let mut paths = vec![p.clone()];
        if ini.exists() {
            paths.push(ini);
        }
        out.push(Finding {
            id: format!("avd:{name}"),
            category: Category::Android,
            title: name.replace('_', " "),
            detail: "Android emulator".into(),
            size: n.size,
            last_used: Some(newest_change(d)),
            path: Some(p),
            action: Action::Trash { paths },
            blocked: None,
            keep: false,
            warning: None,
        });
    }
    for api in child_dirs("Library/Android/sdk/system-images").into_iter().chain(child_dirs("AppData/Local/Android/Sdk/system-images")) {
        for tag in tree.node(api).children.iter().copied().filter(|&c| tree.node(c).is_dir) {
            for abi in tree.node(tag).children.iter().copied().filter(|&c| tree.node(c).is_dir) {
                let p = tree.path(abi);
                out.push(Finding {
                    id: format!("sysimg:{}", p.display()),
                    category: Category::Android,
                    title: format!("System image {} {}", tree.node(api).name, tree.node(tag).name),
                    detail: tree.node(abi).name.to_string(),
                    size: tree.node(abi).size,
                    last_used: None,
                    path: Some(p.clone()),
                    action: trash(&p),
                    blocked: None,
                    // `from_system` clears this when no emulator uses the image.
                    keep: true,
                    warning: None,
                });
            }
        }
    }

    // Docker Desktop on Windows keeps its images and volumes in a WSL disk image.
    if let Some(d) = tree.find(&home.join("AppData/Local/Docker/wsl")) {
        let n = tree.node(d);
        if n.size >= 1_000_000_000 {
            out.push(Finding {
                id: "docker:wsl".into(),
                category: Category::Docker,
                title: "Docker Desktop disk".into(),
                detail: "WSL disk image: prune with `docker system prune`, then compact it from Docker Desktop".into(),
                size: n.size,
                last_used: Some(n.modified),
                path: Some(tree.path(d)),
                action: run("docker", &["system", "prune", "-f"], "docker system prune"),
                blocked: None,
                keep: true,
                warning: None,
            });
        }
    }

    // Package caches.
    const CACHES: &[(&str, &str, &str)] = &[
        (".npm/_cacache", "npm", ""),
        ("Library/pnpm/store", "pnpm store", ""),
        ("Library/Caches/Yarn", "Yarn", ""),
        (".bun/install/cache", "Bun", ""),
        ("Library/Caches/Homebrew", "Homebrew downloads", "brew"),
        ("Library/Caches/pip", "pip", ""),
        (".cache/uv", "uv", ""),
        (".cargo/registry/cache", "Cargo downloads", ""),
        (".cargo/registry/src", "Cargo sources", ""),
        (".cargo/git", "Cargo git checkouts", ""),
        (".gradle/caches", "Gradle", ""),
        (".m2/repository", "Maven", ""),
        (".nuget/packages", "NuGet", ""),
        ("Library/Caches/CocoaPods", "CocoaPods", ""),
        ("go/pkg/mod", "Go modules", "go"),
        ("Library/Caches/JetBrains", "JetBrains IDEs", ""),
        ("Library/Developer/CoreSimulator/Caches", "Simulator caches", ""),
        // Windows.
        ("AppData/Local/npm-cache", "npm", ""),
        ("AppData/Local/pnpm/store", "pnpm store", ""),
        ("AppData/Local/Yarn/Cache", "Yarn", ""),
        ("AppData/Local/pip/Cache", "pip", ""),
        ("AppData/Local/JetBrains", "JetBrains IDEs", ""),
        ("AppData/Local/NuGet/v3-cache", "NuGet HTTP cache", ""),
    ];
    for (rel, name, tool) in CACHES {
        let Some(d) = tree.find(&home.join(rel)) else { continue };
        let n = tree.node(d);
        if n.size < 100 * MB {
            continue;
        }
        let p = tree.path(d);
        let action = match *tool {
            // Homebrew keeps an index of its downloads; Go module files are read-only.
            "brew" => run("brew", &["cleanup", "--prune=all", "-s"], "brew cleanup"),
            "go" => run("go", &["clean", "-modcache"], "go clean -modcache"),
            _ => trash(&p),
        };
        out.push(Finding {
            id: format!("cache:{rel}"),
            category: Category::Caches,
            title: (*name).into(),
            detail: format!("~{}{}", std::path::MAIN_SEPARATOR, rel.replace('/', std::path::MAIN_SEPARATOR_STR)),
            size: n.size,
            last_used: Some(n.modified),
            path: Some(p),
            action,
            blocked: None,
            keep: false,
            warning: None,
        });
    }

    // Toolchains besides the default.
    let rust_default = std::fs::read_to_string(home.join(".rustup/settings.toml"))
        .ok()
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("default_toolchain = ").map(|v| v.trim_matches('"').to_owned())));
    for d in child_dirs(".rustup/toolchains") {
        let n = tree.node(d);
        let name = n.name.to_string();
        out.push(Finding {
            id: format!("rust:{name}"),
            category: Category::Toolchains,
            title: format!("Rust {name}"),
            detail: if rust_default.as_deref() == Some(name.as_str()) { "Your default toolchain".into() } else { "rustup toolchain".into() },
            size: n.size,
            last_used: Some(n.modified),
            path: Some(tree.path(d)),
            action: run("rustup", &["toolchain", "uninstall", &name], "rustup toolchain uninstall"),
            blocked: None,
            keep: rust_default.as_deref() == Some(name.as_str()),
            warning: None,
        });
    }
    let node_default = std::fs::read_to_string(home.join(".nvm/alias/default")).ok().map(|s| s.trim().trim_start_matches('v').to_owned());
    // nvm on macOS keeps versions in ~/.nvm/versions/node; nvm-windows in %APPDATA%\nvm\v*.
    let nvm_windows = child_dirs("AppData/Roaming/nvm").into_iter().filter(|&d| tree.node(d).name.starts_with('v'));
    for d in child_dirs(".nvm/versions/node").into_iter().chain(nvm_windows) {
        let n = tree.node(d);
        let v = n.name.trim_start_matches('v').to_owned();
        let is_default = node_default.as_ref().is_some_and(|dflt| v == *dflt || v.starts_with(&format!("{dflt}.")));
        let p = tree.path(d);
        out.push(Finding {
            id: format!("node:{v}"),
            category: Category::Toolchains,
            title: format!("Node.js {v}"),
            detail: if is_default { "Your nvm default".into() } else { "nvm version".into() },
            size: n.size,
            last_used: Some(n.modified),
            path: Some(p.clone()),
            action: trash(&p),
            blocked: None,
            keep: is_default,
            warning: None,
        });
    }
    (out, worktrees)
}

/// Runs a command with a time limit; stdout on success.
fn output(program: &str, args: &[&str], path_env: Option<&str>, limit: Duration) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(p) = path_env {
        cmd.env("PATH", p);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                return Err(format!("{program} took too long"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim().to_owned())
    }
}

fn iso8601(s: &str) -> Option<i64> {
    // "2026-11-06T14:47:46Z"
    let (date, time) = s.trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|x| x.split('.').next().unwrap_or("0").parse::<i64>());
    let (hh, mm, ss) = (t.next()?.ok()?, t.next()?.ok()?, t.next().and_then(|x| x.ok()).unwrap_or(0));
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

/// Shown for a worktree until git has been asked about it: its size and activity from the scan,
/// not tickable yet.
pub const CHECKING: &str = "Checking with git…";

pub fn worktree_placeholder(wt: &WorktreeDir) -> Finding {
    let wt_s = wt.path.to_string_lossy().into_owned();
    Finding {
        id: format!("wt:{wt_s}"),
        category: Category::Worktrees,
        title: wt.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        detail: "Reading its branch and changes".into(),
        size: wt.size,
        last_used: Some(wt.activity),
        path: Some(wt.path.clone()),
        action: run("git", &["worktree", "remove", &wt_s], "git worktree remove"),
        blocked: Some(CHECKING.into()),
        keep: false,
        warning: None,
    }
}

/// Asks git about one worktree: its branch, last commit, and anything removing it would lose.
/// None for what isn't a worktree after all (a submodule's `.git` file).
pub fn check_worktree(wt: &WorktreeDir, path_env: Option<&str>) -> Option<Finding> {
    let quick = Duration::from_secs(10);
    let git_wait = Duration::from_secs(120);
    let git_file = std::fs::read_to_string(wt.path.join(".git")).ok()?;
    let gitdir = git_file.lines().find_map(|l| l.strip_prefix("gitdir:")).map(|g| PathBuf::from(g.trim()))?;
    // Submodules have a `.git` file too, pointing into the parent's `.git/modules`.
    if gitdir.components().any(|c| c.as_os_str() == "modules") {
        return None;
    }
    let name = wt.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let repo = gitdir.ancestors().find(|a| a.file_name().is_some_and(|n| n == ".git")).and_then(|g| g.parent()).map(Path::to_path_buf);
    let wt_s = wt.path.to_string_lossy().into_owned();
    let (action, blocked, warning, branch, last) = if !gitdir.exists() {
        // Its repository forgot it: nothing but files left.
        (trash(&wt.path), None, None, "orphaned".to_owned(), wt.activity)
    } else {
        let branch = output("git", &["-C", &wt_s, "rev-parse", "--abbrev-ref", "HEAD"], path_env, quick).unwrap_or_default().trim().to_owned();
        let status = output("git", &["-C", &wt_s, "status", "--porcelain"], path_env, git_wait);
        let changes: Vec<String> = status.as_ref().map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect()).unwrap_or_default();
        let unreadable = status.is_err();
        let commit = output("git", &["-C", &wt_s, "log", "-1", "--format=%ct"], path_env, quick).ok().and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        let locked = gitdir.join("locked").exists();
        // If git couldn't say, removing without --force is still safe: git itself refuses a
        // worktree with changes, and Tidy reports that.
        let blocked = locked.then(|| "Locked".to_owned());
        let _ = unreadable;
        let untracked_only = !changes.is_empty() && changes.iter().all(|l| l.starts_with("??"));
        let warning = (!changes.is_empty()).then(|| {
            let n = changes.len();
            let what = if untracked_only { "untracked file" } else { "uncommitted change" };
            format!("{n} {what}{} will be lost", if n == 1 { "" } else { "s" })
        });
        let repo_s = repo.as_ref().map(|r| r.to_string_lossy().into_owned()).unwrap_or_default();
        let mut args = vec!["-C".into(), repo_s, "worktree".into(), "remove".into()];
        // git refuses a worktree with changes; the person ticked it knowing, so say --force.
        if warning.is_some() {
            args.push("--force".into());
        }
        args.push(wt_s.clone());
        let label = if warning.is_some() { "git worktree remove --force" } else { "git worktree remove" };
        let action = Action::Run { program: "git".into(), args, label: label.into() };
        (action, blocked, warning, branch, commit.max(wt.activity))
    };
    let repo_name = repo.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some(Finding {
        id: format!("wt:{wt_s}"),
        category: Category::Worktrees,
        title: name,
        detail: if repo_name.is_empty() { branch } else { format!("{repo_name} · {branch}") },
        size: wt.size,
        last_used: Some(last),
        path: Some(wt.path.clone()),
        action,
        blocked,
        keep: false,
        warning,
    })
}

/// git status on a large checkout takes seconds and reads a lot of disk: four at a time is
/// faster overall than all at once, where they starve each other into timeouts.
pub fn git_pool() -> Option<rayon::ThreadPool> {
    rayon::ThreadPoolBuilder::new().num_threads(4).build().ok()
}

/// What Xcode, the Android SDK and Docker say (seconds, not minutes): simulators, runtimes,
/// which system images an emulator uses (updating `tree_findings`), Docker's reclaimable space.
pub fn tools(tree_findings: &mut [Finding], path_env: Option<&str>, home: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    // Simulators and their runtimes.
    if let Ok(json) = output("xcrun", &["simctl", "list", "devices", "--json"], path_env, Duration::from_secs(20))
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&json)
    {
        for (runtime, devices) in v["devices"].as_object().into_iter().flatten() {
            let os = runtime.rsplit('.').next().unwrap_or(runtime).replacen('-', " ", 1).replace('-', ".");
            for d in devices.as_array().into_iter().flatten() {
                let size = d["dataPathSize"].as_u64().unwrap_or(0) + d["logPathSize"].as_u64().unwrap_or(0);
                let available = d["isAvailable"].as_bool().unwrap_or(true);
                if size < 100 * MB && available {
                    continue; // never-booted devices weigh a few MB
                }
                let udid = d["udid"].as_str().unwrap_or("").to_owned();
                let booted = d["state"] == "Booted";
                out.push(Finding {
                    id: format!("sim:{udid}"),
                    category: Category::Simulators,
                    title: d["name"].as_str().unwrap_or("Simulator").to_owned(),
                    detail: if available { os.clone() } else { format!("{os} · runtime missing") },
                    size,
                    last_used: d["lastUsedAt"].as_str().and_then(iso8601),
                    path: d["dataPath"].as_str().map(|p| PathBuf::from(p).parent().map(Path::to_path_buf).unwrap_or_default()),
                    action: run("xcrun", &["simctl", "delete", &udid], "xcrun simctl delete"),
                    blocked: booted.then(|| "Running".to_owned()),
                    keep: false,
                    warning: None,
                });
            }
        }
    }
    if let Ok(json) = output("xcrun", &["simctl", "runtime", "list", "-j"], path_env, Duration::from_secs(20))
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&json)
    {
        let runtimes: Vec<&serde_json::Value> = v.as_object().into_iter().flat_map(|m| m.values()).collect();
        let version = |r: &serde_json::Value| -> Vec<u32> { r["version"].as_str().unwrap_or("0").split('.').filter_map(|x| x.parse().ok()).collect() };
        for r in &runtimes {
            let platform = r["platformIdentifier"].as_str().unwrap_or("");
            let newest = runtimes.iter().filter(|o| o["platformIdentifier"].as_str() == Some(platform)).all(|o| version(o) <= version(r));
            let os = match platform {
                p if p.contains("iphone") => "iOS",
                p if p.contains("watch") => "watchOS",
                p if p.contains("appletv") => "tvOS",
                p if p.contains("xr") => "visionOS",
                _ => "Simulator",
            };
            let id = r["identifier"].as_str().unwrap_or("").to_owned();
            let deletable = r["deletable"].as_bool().unwrap_or(false);
            out.push(Finding {
                id: format!("rt:{id}"),
                category: Category::Runtimes,
                title: format!("{os} {}", r["version"].as_str().unwrap_or("")),
                detail: if newest { "Newest installed".into() } else { format!("Build {}", r["build"].as_str().unwrap_or("")) },
                size: r["sizeBytes"].as_u64().unwrap_or(0),
                last_used: r["lastUsedAt"].as_str().and_then(iso8601),
                path: None,
                action: run("xcrun", &["simctl", "runtime", "delete", &id], "xcrun simctl runtime delete"),
                blocked: (!deletable).then(|| "Managed by Xcode".to_owned()),
                keep: newest,
                warning: None,
            });
        }
    }

    // Android system images an emulator points at.
    let used: Vec<String> = std::fs::read_dir(home.join(".android/avd"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("config.ini")).ok())
        .flat_map(|ini| ini.lines().filter_map(|l| l.strip_prefix("image.sysdir.1=")).map(|v| v.trim().trim_end_matches('/').to_owned()).collect::<Vec<_>>())
        .collect();
    for f in tree_findings.iter_mut().filter(|f| f.id.starts_with("sysimg:")) {
        let p = f.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        f.keep = used.iter().any(|u| p.ends_with(u.as_str()));
        if !f.keep {
            f.detail = format!("{} · no emulator uses it", f.detail);
        }
    }

    // Docker, when its daemon answers.
    if let Ok(lines) = output("docker", &["system", "df", "--format", "{{json .}}"], path_env, Duration::from_secs(5)) {
        for line in lines.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let ty = v["Type"].as_str().unwrap_or("");
            let reclaim = parse_docker_size(v["Reclaimable"].as_str().unwrap_or("0"));
            if reclaim < 100 * MB {
                continue;
            }
            let (title, action, keep) = match ty {
                "Images" => ("Unused images", run("docker", &["image", "prune", "-a", "-f"], "docker image prune -a"), false),
                "Containers" => ("Stopped containers", run("docker", &["container", "prune", "-f"], "docker container prune"), false),
                "Local Volumes" => ("Unused volumes", run("docker", &["volume", "prune", "-f"], "docker volume prune"), true),
                "Build Cache" => ("Build cache", run("docker", &["builder", "prune", "-f"], "docker builder prune"), false),
                _ => continue,
            };
            out.push(Finding {
                id: format!("docker:{ty}"),
                category: Category::Docker,
                title: title.into(),
                detail: if keep { "Volumes can hold databases: check first".into() } else { format!("{} total", v["Size"].as_str().unwrap_or("")) },
                size: reclaim,
                last_used: None,
                path: None,
                action,
                blocked: None,
                keep,
                warning: None,
            });
        }
    }
    out
}

/// Everything the tools say, worktrees included, in one call (the CLI's way; the app streams).
pub fn from_system(worktrees: &[WorktreeDir], tree_findings: &mut [Finding], path_env: Option<&str>, home: &Path) -> Vec<Finding> {
    use rayon::prelude::*;
    let mut out = tools(tree_findings, path_env, home);
    let check = || -> Vec<Finding> { worktrees.par_iter().filter_map(|wt| check_worktree(wt, path_env)).collect() };
    out.extend(match git_pool() {
        Some(p) => p.install(check),
        None => check(),
    });
    out
}

/// "1.2GB (40%)" → bytes.
fn parse_docker_size(s: &str) -> u64 {
    let s = s.split_whitespace().next().unwrap_or("0");
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().unwrap_or(0.0);
    let mult = match unit {
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

/// Cleans one finding. Never forces: a tool that refuses (a dirty worktree) reports why.
pub fn clean(f: &Finding, path_env: Option<&str>) -> Result<(), String> {
    match &f.action {
        Action::Trash { paths } => match crate::move_to_trash(paths).first() {
            None => Ok(()),
            Some((_, e)) => Err(e.clone()),
        },
        Action::Run { program, args, .. } => {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            output(program, &a, path_env, Duration::from_secs(600)).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dates_and_docker_sizes() {
        assert_eq!(iso8601("1970-01-02T00:00:00Z"), Some(86_400));
        assert_eq!(iso8601("2026-11-06T14:47:46Z"), Some(1_793_976_466));
        assert_eq!(parse_docker_size("1.5GB (40%)"), 1_500_000_000);
        assert_eq!(parse_docker_size("0B"), 0);
    }

    #[test]
    fn finds_build_output_and_worktrees_in_a_scan() {
        let dir = std::env::temp_dir().join(format!("tidy-dev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("proj/target/debug")).unwrap();
        std::fs::write(dir.join("proj/Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.join("proj/target/debug/big"), vec![0u8; 60_000_000]).unwrap();
        std::fs::create_dir_all(dir.join("wt")).unwrap();
        std::fs::write(dir.join("wt/.git"), "gitdir: /nowhere/.git/worktrees/wt").unwrap();
        let t = crate::scan(&dir, Default::default()).unwrap();
        let (f, wts) = from_tree(&t, &dir);
        assert!(f.iter().any(|f| f.category == Category::Builds && f.title == "proj"));
        assert_eq!(wts.len(), 1);
        let mut none = Vec::new();
        let sys = from_system(&wts, &mut none, None, &dir);
        let wt = sys.iter().find(|f| f.category == Category::Worktrees).unwrap();
        assert!(matches!(wt.action, Action::Trash { .. }), "an orphaned worktree goes to the Trash");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

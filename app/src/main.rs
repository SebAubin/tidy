//! Tidy: see what fills your disk, and clean it up with the AI you already have.

mod assistant;
mod dev;
mod dupes;
mod history;
mod icons;
mod menus;
mod settings;
#[cfg(target_os = "macos")]
mod status_item;
mod theme;
mod treemap;
mod ui;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rune::{AppSpec, Chrome, KeyEvent, TextEdit, Tx, Waker};
use rune_kit::shell::{self, PaneState};
use tidy_core::{NodeId, Progress, Suggestion, Tree, snapshot::Snapshot};

use assistant::{Detected, Event, Session};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Map,
    Suggestions,
    LargeFiles,
    Duplicates,
    Developer,
    Changes,
}

/// A folder the sidebar offers.
#[derive(Clone, Debug)]
pub struct Place {
    pub name: String,
    pub path: PathBuf,
    pub drive: bool,
}

/// A finished scan with what the Developer view finds in it, worked out on the scan's thread.
type ScanResult = (Tree, Vec<tidy_core::dev::Finding>, Vec<tidy_core::dev::WorktreeDir>);

pub struct ScanJob {
    pub path: PathBuf,
    pub progress: Arc<Progress>,
    pub started: Instant,
    pub(crate) result: Arc<Mutex<Option<Option<ScanResult>>>>,
}

#[derive(Clone, Debug)]
pub enum Msg {
    User(String),
    Assistant(String),
    /// What the assistant is looking at ("Looking at ~/Library").
    Activity(String),
    Proposal(usize),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct ProposalItem {
    pub path: String,
    pub reason: String,
    pub safe: bool,
    pub size: u64,
    pub checked: bool,
}

#[derive(Clone, Debug)]
pub struct Proposal {
    pub title: String,
    pub items: Vec<ProposalItem>,
    /// Moved to the Trash: how many bytes.
    pub done: Option<u64>,
}

pub struct Chat {
    pub detected: Option<Detected>,
    pub provider: usize,
    pub session: Option<Session>,
    pub model: String,
    pub messages: Vec<Msg>,
    /// Text streaming into the assistant's current message.
    pub streaming: String,
    pub busy: bool,
    pub input: TextEdit,
    pub proposals: Vec<Proposal>,
    pub cost: f64,
}

pub struct App {
    pub sidebar: PaneState,
    pub inspector: PaneState,
    pub section: Section,
    pub places: Vec<Place>,
    pub place: Option<PathBuf>,
    pub job: Option<ScanJob>,
    pub tree: Option<Tree>,
    /// Earlier scans kept in memory, so going back to them is instant.
    pub shelf: Vec<Tree>,
    /// The folder the map shows.
    pub focus: NodeId,
    pub selected: Option<NodeId>,
    pub suggestions: Vec<Suggestion>,
    /// Suggestions ticked for removal, by path.
    pub ticked: BTreeSet<String>,
    pub largest: Vec<NodeId>,
    /// Large Files picked for removal (⌘-click, ⇧-click, ⌘A), and where a ⇧-click range starts.
    pub picked: BTreeSet<NodeId>,
    pub pick_anchor: Option<usize>,
    /// The scanned volume's capacity, read after scans, removals and on a slow timer (no disk
    /// calls while drawing).
    pub space: Option<tidy_core::volume::Space>,
    /// What Tidy moved to the Trash this session: still on disk until the Trash is emptied.
    pub in_trash: u64,
    /// "Empty Trash" pressed once; a second press within a few seconds empties it.
    pub confirm_empty: Option<Instant>,
    /// Read the volume's space again over the next seconds (after emptying the Trash).
    pub space_refresh_at: Option<Instant>,
    pub chat: Chat,
    pub toast: Option<(String, Instant)>,
    pub waker: Option<Waker>,
    pub framed: bool,
    /// Full Disk Access, checked at launch and while the access screen shows.
    pub full_disk: bool,
    /// A scan waiting on the access screen.
    pub pending: Option<PathBuf>,
    /// The person chose to scan without Full Disk Access this session.
    pub without_access: bool,
    pub settings: settings::Settings,
    /// The Developer view's findings, gathered in the background after each scan.
    pub dev: dev::DevState,
    pub dupes: dupes::DupesState,
    /// Earlier scans of the folder on screen, oldest first.
    pub history: Vec<tidy_core::history::Summary>,
    /// The map on screen is the last session's, rebuilt from its snapshot while a fresh scan runs.
    pub from_snapshot: bool,
    /// What Changes compares with: 0 for the previous scan, else at least this many seconds ago.
    pub changes_age: i64,
    /// The low-space notification went out this session.
    pub warned_low: bool,
    /// When the volume's free space was last read, to keep the menu bar current.
    pub space_read: Option<Instant>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into())
}

fn init() -> App {
    let h = home();
    let mut places = vec![Place { name: "Home".into(), path: h.clone(), drive: false }];
    for (n, rel) in [("Downloads", "Downloads"), ("Documents", "Documents"), ("Desktop", "Desktop"), ("Library", "Library")] {
        if h.join(rel).is_dir() {
            places.push(Place { name: n.into(), path: h.join(rel), drive: false });
        }
    }
    places.push(Place { name: "Applications".into(), path: "/Applications".into(), drive: false });
    places.push(Place { name: "Macintosh HD".into(), path: "/".into(), drive: true });
    App {
        sidebar: PaneState::new(220.0).range(180.0, 320.0),
        inspector: PaneState::new(360.0).range(300.0, 560.0),
        section: Section::Map,
        places,
        place: None,
        job: None,
        tree: None,
        shelf: Vec::new(),
        focus: Tree::ROOT,
        selected: None,
        suggestions: Vec::new(),
        ticked: BTreeSet::new(),
        largest: Vec::new(),
        picked: BTreeSet::new(),
        pick_anchor: None,
        space: tidy_core::volume::space(Path::new("/")),
        in_trash: 0,
        confirm_empty: None,
        space_refresh_at: None,
        chat: Chat {
            detected: None,
            provider: 0,
            session: None,
            model: String::new(),
            messages: Vec::new(),
            streaming: String::new(),
            busy: false,
            input: TextEdit::new(""),
            proposals: Vec::new(),
            cost: 0.0,
        },
        toast: None,
        waker: None,
        framed: false,
        full_disk: tidy_core::access::has_full_disk_access(),
        pending: None,
        without_access: false,
        settings: settings::Settings::load(),
        dev: dev::DevState::default(),
        dupes: dupes::DupesState::default(),
        history: Vec::new(),
        from_snapshot: false,
        changes_age: 0,
        warned_low: false,
        space_read: None,
    }
}

/// Scans kept besides the one on screen; each can be hundreds of MB for a large home folder.
const SHELF: usize = 2;

impl App {
    /// Shows `path`: from the scan on screen when it covers it, else from a scan kept in memory,
    /// else by scanning it.
    pub fn go_to(&mut self, path: PathBuf) {
        if let Some(id) = self.tree.as_ref().and_then(|t| t.find(&path)) {
            self.cancel_scan();
            self.pending = None;
            self.focus = id;
            self.selected = None;
            self.section = Section::Map;
            return;
        }
        if let Some(i) = self.shelf.iter().position(|t| t.find(&path).is_some()) {
            self.cancel_scan();
            self.pending = None;
            let t = self.shelf.remove(i);
            if let Some(old) = self.tree.take() {
                self.shelf.push(old);
            }
            self.activate(t, &path);
            return;
        }
        self.start_scan(path);
    }

    /// Scans again the folder the scan on screen started from.
    pub fn rescan(&mut self) {
        if let Some(p) = self.tree.as_ref().map(|t| t.root_path.clone()).or_else(|| self.place.clone()) {
            self.start_scan(p);
        }
    }

    /// Brings a kept scan back on screen.
    fn activate(&mut self, tree: Tree, path: &Path) {
        let root = tree.root_path.clone();
        self.from_snapshot = false;
        self.show_tree(tree, path, true);
        self.load_history(&root);
    }

    /// Puts `tree` on screen at `path`. `fresh` when it's a real scan, which the assistant's
    /// snapshot is then written from.
    pub fn show_tree(&mut self, tree: Tree, path: &Path, fresh: bool) {
        self.suggestions = tidy_core::suggestions(&tree, Some(&home()));
        self.ticked = self.suggestions.iter().filter(|s| s.risk == tidy_core::Risk::Safe).map(|s| s.path.clone()).collect();
        self.largest = tree.largest_files(self.settings.large_min_mb * 1_000_000, 500);
        self.focus = tree.find(path).unwrap_or(Tree::ROOT);
        self.selected = None;
        self.place = Some(tree.root_path.clone());
        self.section = Section::Map;
        self.picked.clear();
        self.pick_anchor = None;
        self.space = tidy_core::volume::space(&tree.root_path);
        let root = tree.root_path.clone();
        self.tree = Some(tree);
        if fresh {
            self.write_snapshot();
        }
        self.start_dev(&root);
    }

    /// The assistant to use: the one chosen last (or set in Settings) if it's here and on, else
    /// the first that is, Claude Code first.
    pub fn pick_provider(&mut self) {
        let Some(d) = &self.chat.detected else { return };
        let ok = |i: &usize| d.providers.get(*i).is_some_and(|p| self.settings.enabled(p.kind));
        let wanted = self.settings.assistant.as_deref().and_then(|id| d.providers.iter().position(|p| p.kind.id() == id)).filter(ok);
        let next = wanted.or_else(|| (0..d.providers.len()).find(ok)).unwrap_or(0);
        if next != self.chat.provider && self.chat.session.is_some() {
            self.new_chat();
        }
        self.chat.provider = next;
    }

    /// Remembers the assistant picked in the panel, and tells the Settings window.
    pub fn choose_provider(&mut self, i: usize) {
        let Some(kind) = self.chat.detected.as_ref().and_then(|d| d.providers.get(i)).map(|p| p.kind) else { return };
        self.settings.assistant = Some(kind.id().to_owned());
        self.settings.save();
        let next = self.settings.clone();
        if let Some(w) = settings::WINDOW.get() {
            w.update(move |ui: &mut settings::SettingsUi, _| ui.s = next);
        }
        if i != self.chat.provider {
            self.new_chat();
        }
        self.chat.provider = i;
    }

    /// Takes new settings from the Settings window.
    pub fn apply_settings(&mut self, next: settings::Settings, tx: &mut Tx) {
        // A different assistant or model starts a new conversation with it.
        let assistant_changed = next.assistant != self.settings.assistant || next.disabled != self.settings.disabled;
        let kind = self.chat.detected.as_ref().and_then(|d| d.providers.get(self.chat.provider)).map(|p| p.kind);
        let model_changed = kind.is_some_and(|k| next.model(k) != self.settings.model(k));
        if next.appearance != self.settings.appearance || next.accent != self.settings.accent {
            tx.color_motion(rune::tok::motion::CALM);
        }
        let large_changed = next.large_min_mb != self.settings.large_min_mb;
        let stale_changed = next.stale_days != self.settings.stale_days;
        self.settings = next;
        if large_changed && let Some(t) = &self.tree {
            self.largest = t.largest_files(self.settings.large_min_mb * 1_000_000, 500);
            self.picked.clear();
        }
        if stale_changed {
            self.refresh_dev();
        }
        if assistant_changed {
            self.pick_provider();
        }
        if model_changed && self.chat.session.is_some() {
            self.new_chat();
        }
        // The menu bar follows at once (its text, or showing at all).
        #[cfg(target_os = "macos")]
        self.sync_status_item();
    }

    /// Empties the Trash through the Finder (as its menu does), then reads the free space again.
    pub fn empty_trash(&mut self, tx: &mut Tx) {
        let _ = std::process::Command::new("osascript").args(["-e", "tell application \"Finder\" to empty trash"]).spawn();
        self.in_trash = 0;
        self.confirm_empty = None;
        self.say("Emptying the Trash…");
        // The Finder empties in the background; look at the volume again as it goes.
        tx.wake_after(std::time::Duration::from_secs(2));
        self.space_refresh_at = Some(Instant::now());
    }

    /// Scans `path`, first asking for Full Disk Access when the folder holds guarded places,
    /// so macOS doesn't ask about each of them.
    pub fn start_scan(&mut self, path: PathBuf) {
        let guarded = tidy_core::access::guarded_under(&path);
        if !self.full_disk && !guarded.is_empty() && !self.without_access {
            self.cancel_scan();
            self.pending = Some(path);
            return;
        }
        let leave_out = if self.full_disk { Vec::new() } else { guarded.into_iter().map(|g| g.path).collect() };
        self.pending = None;
        if let Some(j) = &self.job {
            j.progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let progress = Arc::new(Progress::default());
        let result = Arc::new(Mutex::new(None));
        let (p, r, root) = (progress.clone(), result.clone(), path.clone());
        let waker = self.waker.clone();
        std::thread::spawn(move || {
            let tree = tidy_core::scan_skipping(&root, p, leave_out).map(|t| {
                let (f, w) = tidy_core::dev::from_tree(&t, &home());
                (t, f, w)
            });
            *r.lock().unwrap() = Some(tree);
            if let Some(w) = waker {
                w();
            }
        });
        self.place = Some(path.clone());
        self.job = Some(ScanJob { path, progress, started: Instant::now(), result });
        self.selected = None;
    }

    pub fn cancel_scan(&mut self) {
        if let Some(j) = self.job.take() {
            j.progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if self.tree.is_none() {
            self.place = None;
        }
    }

    fn finish_scan(&mut self, tree: Tree) {
        // A fresh scan replaces any kept scan it covers.
        let root = tree.root_path.clone();
        // A refresh of the scan on screen keeps the folder and view the person was in.
        let same = self.tree.as_ref().is_some_and(|t| t.root_path == root);
        let keep = same.then(|| (self.tree.as_ref().unwrap().path(self.focus), self.section));
        if !same {
            self.load_history(&root);
        }
        self.remember_scan(&tree);
        self.shelf.retain(|t| !t.root_path.starts_with(&root));
        if let Some(old) = self.tree.take().filter(|t| !t.root_path.starts_with(&root)) {
            self.shelf.push(old);
        }
        while self.shelf.len() > SHELF {
            self.shelf.remove(0);
        }
        self.from_snapshot = false;
        match keep {
            Some((focus, section)) => {
                self.show_tree(tree, &focus, true);
                self.section = section;
            }
            None => self.show_tree(tree, &root, true),
        }
    }

    /// Hands the assistant's MCP server the latest picture, off the UI thread.
    pub fn write_snapshot(&self) {
        let Some(tree) = &self.tree else { return };
        if cfg!(test) {
            return;
        }
        let snap = Snapshot::build(tree, &self.suggestions, &self.dev.findings);
        std::thread::spawn(move || {
            let _ = snap.write(&assistant::snapshot_path());
        });
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub fn path_of(&self, id: NodeId) -> Option<PathBuf> {
        self.tree.as_ref().map(|t| t.path(id))
    }

    pub fn open_folder(&mut self, id: NodeId) {
        if let Some(t) = &self.tree
            && t.node(id).is_dir
            && !t.node(id).children.is_empty()
        {
            self.focus = id;
            self.selected = None;
            self.section = Section::Map;
        }
    }

    pub fn go_up(&mut self) {
        if let Some(p) = self.tree.as_ref().and_then(|t| t.node(self.focus).parent) {
            self.selected = Some(self.focus);
            self.focus = p;
        }
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    /// Moves paths to the Trash and takes them out of the tree. Returns bytes freed.
    pub fn trash(&mut self, paths: &[String]) -> u64 {
        let all: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let errors = tidy_core::move_to_trash(&all);
        let failed: Vec<String> = errors.iter().map(|(p, e)| format!("{}: {e}", tilde(&p.to_string_lossy()))).collect();
        let mut freed = 0;
        for p in all.iter().filter(|p| !errors.iter().any(|(q, _)| q == *p)) {
            if let Some(t) = &mut self.tree
                && let Some(id) = t.find(p)
            {
                freed += t.node(id).size;
                if self.focus == id || t.ancestry(self.focus).contains(&id) {
                    self.focus = t.node(id).parent.unwrap_or(Tree::ROOT);
                }
                t.remove(id);
            }
            for t in &mut self.shelf {
                if let Some(id) = t.find(p) {
                    t.remove(id);
                }
            }
        }
        self.in_trash += freed;
        self.picked.retain(|id| self.tree.as_ref().is_some_and(|t| t.node(*id).parent.is_some()));
        self.space = self.tree.as_ref().and_then(|t| tidy_core::volume::space(&t.root_path)).or(self.space);
        self.suggestions.retain(|s| !paths.contains(&s.path));
        for p in paths {
            self.ticked.remove(p);
        }
        if let Some(t) = &self.tree {
            self.largest.retain(|&id| t.node(id).parent.is_some());
        }
        self.selected = None;
        self.write_snapshot();
        if failed.is_empty() {
            self.say(format!("Moved {} to the Trash · {}", plural(paths.len(), "item"), tidy_core::format::bytes(freed)));
        } else {
            self.say(format!("Couldn't move {}: {}", plural(failed.len(), "item"), failed.join("; ")));
        }
        freed
    }

    pub fn ask(&mut self, text: String) {
        let text = text.trim().to_owned();
        if text.is_empty() || self.chat.busy {
            return;
        }
        if self.tree.is_none() {
            self.chat.messages.push(Msg::Error("Scan a folder first: the assistant reads Tidy's scan.".into()));
            return;
        }
        self.chat.messages.push(Msg::User(text.clone()));
        self.chat.input = TextEdit::new("");
        if self.chat.session.is_none() {
            let Some(d) = &self.chat.detected else { return };
            let Some(p) = d.providers.get(self.chat.provider).filter(|p| self.settings.enabled(p.kind)).cloned() else {
                self.chat.messages.push(Msg::Error("No assistant is available. Install Claude Code, Codex or Gemini CLI, or turn one on in Settings › Assistant.".into()));
                return;
            };
            let Some(w) = self.waker.clone() else { return };
            match Session::start(&p, self.settings.model(p.kind), d.path_env.as_deref(), &assistant::snapshot_path(), w) {
                Ok(s) => self.chat.session = Some(s),
                Err(e) => {
                    self.chat.messages.push(Msg::Error(e));
                    return;
                }
            }
        }
        // Which scan it's looking at (so it never talks of scanning), and where the person is
        // looking, so "this folder" means something.
        let scan = self.tree.as_ref().map(|t| {
            format!(
                "Tidy's scan of {}: {}, {} files, scanned {}.",
                t.root_path.display(),
                tidy_core::format::bytes(t.total()),
                tidy_core::format::count(t.node(Tree::ROOT).files),
                tidy_core::format::age(tidy_core::scan::now() - t.scanned_at)
            )
        });
        let here = match (&self.tree, self.selected) {
            _ if !self.settings.share_context => String::new(),
            (Some(t), Some(sel)) => format!(" Tidy is showing {}, with {} selected.", t.path(self.focus).display(), t.path(sel).display()),
            (Some(t), None) => format!(" Tidy is showing {}.", t.path(self.focus).display()),
            _ => String::new(),
        };
        let here = match scan {
            Some(scan) => format!("\n\n({scan}{here})"),
            None => here,
        };
        if let Some(s) = &mut self.chat.session {
            match s.send(&format!("{text}{here}")) {
                Ok(()) => self.chat.busy = true,
                Err(e) => {
                    self.chat.messages.push(Msg::Error(e));
                    self.chat.session = None;
                }
            }
        }
    }

    pub fn new_chat(&mut self) {
        self.chat.session = None;
        self.chat.messages.clear();
        self.chat.streaming.clear();
        self.chat.proposals.clear();
        self.chat.busy = false;
    }

    fn on_event(&mut self, e: Event) {
        let c = &mut self.chat;
        match e {
            Event::Ready { model } => c.model = model,
            Event::Delta(t) => c.streaming.push_str(&t),
            Event::Text(t) => {
                c.streaming.clear();
                if !t.trim().is_empty() {
                    c.messages.push(Msg::Assistant(t));
                }
            }
            Event::Tool { name, input } => {
                let short = assistant::tool_name(&name);
                let at = |k: &str| input[k].as_str().map(tilde);
                let line = match short.as_str() {
                    "overview" => "Reading Tidy's scan".to_owned(),
                    "list_folder" => format!("Looking inside {}", at("path").unwrap_or_default()),
                    "find_files" => match at("under") {
                        Some(u) => format!("Searching files in {u}"),
                        None => "Searching for large files".to_owned(),
                    },
                    tidy_core::mcp::PROPOSE_TOOL => {
                        self.add_proposal(&input);
                        return;
                    }
                    // The assistant steering the window.
                    tidy_core::mcp::SHOW_TOOL => {
                        if let Some(p) = input["path"].as_str().map(PathBuf::from) {
                            let target = if p.is_dir() { p.clone() } else { p.parent().map(Path::to_path_buf).unwrap_or(p.clone()) };
                            self.go_to(target);
                            self.section = Section::Map;
                            self.selected = self.tree.as_ref().and_then(|t| t.find(&p));
                            self.chat.messages.push(Msg::Activity(format!("Showing {} on the map", tilde(&p.to_string_lossy()))));
                        }
                        return;
                    }
                    tidy_core::mcp::VIEW_TOOL => {
                        let (sec, name) = match input["view"].as_str().unwrap_or("") {
                            "suggestions" => (Section::Suggestions, "Suggestions"),
                            "large_files" => (Section::LargeFiles, "Large Files"),
                            "duplicates" => (Section::Duplicates, "Duplicates"),
                            "developer" => (Section::Developer, "Developer"),
                            "changes" => (Section::Changes, "Changes"),
                            _ => (Section::Map, "the Space Map"),
                        };
                        if self.tree.is_some() {
                            self.section = sec;
                        }
                        self.chat.messages.push(Msg::Activity(format!("Opened {name}")));
                        return;
                    }
                    other => format!("Using {other}"),
                };
                c.messages.push(Msg::Activity(line));
            }
            Event::TurnDone { cost_usd, error } => {
                c.busy = false;
                c.cost = cost_usd.max(c.cost);
                if !c.streaming.is_empty() {
                    let t = std::mem::take(&mut c.streaming);
                    c.messages.push(Msg::Assistant(t));
                }
                if let Some(e) = error {
                    c.messages.push(Msg::Error(e));
                }
            }
            Event::Exited(err) => {
                if c.session.take().is_some() && c.busy {
                    c.messages.push(Msg::Error(err));
                }
                c.busy = false;
            }
        }
    }

    fn add_proposal(&mut self, input: &serde_json::Value) {
        let Some(tree) = &self.tree else { return };
        let items: Vec<ProposalItem> = input["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|it| {
                let path = it["path"].as_str()?.to_owned();
                let id = tree.find(Path::new(&path))?;
                let safe = it["risk"].as_str() == Some("safe");
                Some(ProposalItem { reason: it["reason"].as_str().unwrap_or("").to_owned(), size: tree.node(id).size, checked: safe, safe, path })
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let title = input["title"].as_str().unwrap_or("Cleanup").to_owned();
        self.chat.proposals.push(Proposal { title, items, done: None });
        self.chat.messages.push(Msg::Proposal(self.chat.proposals.len() - 1));
    }
}

pub fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{} {word}s", tidy_core::format::count(n as u64)) }
}

/// `/Users/me/Library` as `~/Library`.
pub fn tilde(p: &str) -> String {
    let h = home();
    let h = h.to_string_lossy();
    match p.strip_prefix(h.as_ref()) {
        Some(rest) if h != "/" => format!("~{rest}"),
        _ => p.to_owned(),
    }
}

fn setup(s: &mut App, waker: Waker) {
    s.waker = Some(waker.clone());
    history::boot(waker.clone());
    std::thread::spawn({
        let w = waker.clone();
        move || {
            let d = assistant::detect();
            DETECTED.lock().unwrap().replace(d);
            w();
        }
    });
}

static DETECTED: Mutex<Option<Detected>> = Mutex::new(None);

fn poll(s: &mut App, _: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if !s.framed {
        s.framed = rune_mac::remember_frame("Tidy");
    }
    if s.chat.detected.is_none()
        && let Some(d) = DETECTED.lock().unwrap().take()
    {
        *settings::INSTALLED.lock().unwrap() = d.providers.iter().map(|p| p.kind).collect();
        s.chat.detected = Some(d);
        s.pick_provider();
        changed = true;
    }
    // Waiting on the switch in System Settings: look again every second, and go when it's on.
    if s.pending.is_some() {
        if tidy_core::access::has_full_disk_access() {
            s.full_disk = true;
            if let Some(p) = s.pending.take() {
                s.start_scan(p);
            }
            changed = true;
        } else {
            tx.wake_after(std::time::Duration::from_secs(1));
        }
    }
    if let Some(job) = &s.job {
        let done = job.result.lock().unwrap().take();
        match done {
            Some(Some((tree, findings, worktrees))) => {
                s.job = None;
                s.dev.by_root.insert(tree.root_path.clone(), (findings, worktrees));
                s.finish_scan(tree);
                changed = true;
            }
            Some(None) => s.job = None,
            None => {
                // Progress ticks four times a second: enough to feel live, calm enough to read.
                tx.wake_after(std::time::Duration::from_millis(250));
                changed = true;
            }
        }
    }
    changed |= s.poll_dev();
    changed |= s.poll_dupes(tx);
    changed |= s.poll_history();
    // The menu bar's free space: read once a minute, and whenever something changed.
    let minute = std::time::Duration::from_secs(60);
    // After launch has finished (the window has its frame): AppKit may drop a status item
    // made earlier.
    if !s.framed {
    } else if s.space_read.is_none_or(|t| t.elapsed() >= minute) {
        s.space = s.place.as_deref().or(Some(Path::new("/"))).and_then(tidy_core::volume::space).or(s.space);
        s.space_read = Some(Instant::now());
        tx.wake_after(minute);
        #[cfg(target_os = "macos")]
        s.sync_status_item();
    } else if changed {
        #[cfg(target_os = "macos")]
        s.sync_status_item();
    }
    let events: Vec<Event> = s.chat.session.as_ref().map(|x| x.events.try_iter().collect()).unwrap_or_default();
    for e in events {
        s.on_event(e);
        changed = true;
    }
    if let Some(at) = s.space_refresh_at {
        let fresh = s.place.as_deref().and_then(tidy_core::volume::space).or(s.space);
        if fresh.map(|x| x.free) != s.space.map(|x| x.free) {
            s.space = fresh;
            changed = true;
        }
        if at.elapsed() < std::time::Duration::from_secs(20) {
            tx.wake_after(std::time::Duration::from_secs(2));
        } else {
            s.space_refresh_at = None;
        }
    }
    if let Some(at) = s.confirm_empty
        && at.elapsed() >= std::time::Duration::from_secs(4)
    {
        s.confirm_empty = None;
        changed = true;
    } else if s.confirm_empty.is_some() {
        tx.wake_after(std::time::Duration::from_secs(1));
    }
    if let Some((_, at)) = &s.toast {
        let left = std::time::Duration::from_secs(4).saturating_sub(at.elapsed());
        if left.is_zero() {
            s.toast = None;
            changed = true;
        } else {
            tx.wake_after(left);
        }
    }
    changed
}

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if shell::is_sidebar_toggle(ev) {
        s.sidebar.toggle();
        return true;
    }
    if ev.is_command('j') {
        s.inspector.toggle();
        return true;
    }
    if ev.is_command('r') {
        s.rescan();
        return true;
    }
    if ev.is_command('o') {
        ui::pick_folder(tx);
        return true;
    }
    if tx.focused().is_none() && s.section == Section::LargeFiles && s.job.is_none() {
        if ev.is_command('a') {
            s.picked = s.largest.iter().copied().collect();
            return true;
        }
        if matches!(ev.key, rune::Key::Backspace) && ev.mods.cmd && !s.picked.is_empty() {
            let paths: Vec<String> = s.picked.iter().filter_map(|&id| s.path_of(id)).map(|p| p.to_string_lossy().into_owned()).collect();
            s.trash(&paths);
            return true;
        }
        if matches!(ev.key, rune::Key::Escape) && !s.picked.is_empty() {
            s.picked.clear();
            return true;
        }
    }
    if tx.focused().is_none() {
        match ev.key {
            rune::Key::Escape | rune::Key::Backspace if !ev.mods.cmd => {
                s.go_up();
                return true;
            }
            rune::Key::Backspace if ev.mods.cmd => {
                if let Some(p) = s.selected.and_then(|id| s.path_of(id)) {
                    s.trash(&[p.to_string_lossy().into_owned()]);
                }
                return true;
            }
            _ => {}
        }
    }
    false
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("mcp") {
        let snap = args.get(2).map(PathBuf::from).unwrap_or_else(assistant::snapshot_path);
        if let Err(e) = tidy_core::mcp::serve(&snap) {
            eprintln!("tidy mcp: {e}");
            std::process::exit(1);
        }
        return;
    }
    rune_mac::launch();
    rune::prewarm_gpu();
    let size = rune_mac::saved_size("Tidy").unwrap_or((1180.0, 760.0));
    let settings_spec = AppSpec { min_size: Some((520.0, 460.0)), poll: Some(settings::poll), ..AppSpec::new("Settings", (580.0, 760.0), settings::init, settings::view) };
    let spec = AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        min_size: Some((820.0, 520.0)),
        families: ("System Font, Segoe UI Variable Text, Segoe UI, Inter", "ui-monospace"),
        scenes: &ui::SCENES,
        ..AppSpec::new("Tidy", size, init, ui::view)
    };
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((18.0, 18.0)), drag_regions: true };
    let mut app = rune::App::new(spec).chrome(chrome).standard_menus().modifier_keys();
    for item in menus::items() {
        app = app.menu_item(item);
    }
    let hidden = rune::WindowOptions { visible: false, resizable: false, ..Default::default() };
    // A standard titled window: macOS draws its title bar and "Settings" title.
    let settings_app = rune::App::new(settings_spec).chrome(Chrome { full_size_content: false, traffic_lights: None, drag_regions: false });
    let (settings_handle, _) = app.window(rune::WindowSpec::from_app("settings", settings_app).options(hidden));
    let _ = settings::WINDOW.set(settings_handle);
    let _ = settings::MAIN.set(app.handle());
    // A click on the low-space notification opens Tidy.
    rune_mac::notify::on_click(|_| {
        if let Some(m) = settings::MAIN.get() {
            m.open();
        }
    });
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn a_place_inside_the_scan_opens_without_scanning() {
        let mut s = init();
        (ui::SCENES[1].1)(&mut s); // the demo home folder, on the map
        let downloads = s.tree.as_ref().unwrap().root_path.join("Downloads");
        let mut rt = Runtime::new(s, ui::view, (1180.0, 760.0), Env { system_dark: false, scale: 1.0 });
        rt.frame(0.0);
        let i = rt.state.places.iter().position(|p| p.path == downloads).expect("Downloads is a place");
        let (x, y) = rt.find(&format!("place-{i}")).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.job.is_none() && rt.state.pending.is_none(), "no scan started");
        assert_eq!(rt.state.tree.as_ref().unwrap().path(rt.state.focus), downloads);
    }

    #[test]
    fn suggestions_select_and_deselect_all() {
        let mut s = init();
        (ui::SCENES.iter().find(|(n, _)| *n == "suggestions").unwrap().1)(&mut s);
        let mut rt = Runtime::new(s, ui::view, (1180.0, 900.0), Env { system_dark: false, scale: 1.0 });
        rt.frame(0.0);
        let n = rt.state.suggestions.len();
        assert!(rt.state.ticked.len() < n, "review items start unticked");
        click(&mut rt, "sugg-all");
        assert_eq!(rt.state.ticked.len(), n, "Select All ticks every suggestion");
        click(&mut rt, "sugg-all");
        assert!(rt.state.ticked.is_empty(), "then Deselect All clears them");
    }

    #[test]
    fn a_rescan_keeps_the_folder_and_view_on_screen() {
        let mut s = init();
        (ui::SCENES[1].1)(&mut s);
        let downloads = s.tree.as_ref().unwrap().root_path.join("Downloads");
        s.go_to(downloads.clone());
        s.section = Section::LargeFiles;
        s.finish_scan(ui::demo_tree());
        assert_eq!(s.tree.as_ref().unwrap().path(s.focus), downloads);
        assert_eq!(s.section, Section::LargeFiles);
        assert_eq!(s.history.len(), 1, "the scan is remembered for Changes");
    }

    #[test]
    fn the_chosen_assistant_is_remembered_unless_turned_off() {
        use assistant::{Provider, ProviderKind};
        let mut s = init();
        let p = |kind| Provider { kind, path: "/x".into() };
        s.chat.detected = Some(Detected { providers: vec![p(ProviderKind::ClaudeCode), p(ProviderKind::Codex)], path_env: None });
        s.settings.assistant = Some("codex".into());
        s.pick_provider();
        assert_eq!(s.chat.provider, 1, "the one used last");
        s.settings.disabled = vec!["codex".into()];
        s.pick_provider();
        assert_eq!(s.chat.provider, 0, "a turned-off one falls back to the next");
        assert_eq!(s.settings.model(ProviderKind::ClaudeCode), None);
        s.settings.models.insert("claude_code".into(), " sonnet ".into());
        assert_eq!(s.settings.model(ProviderKind::ClaudeCode).as_deref().map(str::trim), Some("sonnet"));
    }

    fn click(rt: &mut Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn developer_rows_and_groups_tick() {
        let mut s = init();
        let scene = ui::SCENES.iter().find(|(n, _)| *n == "developer").unwrap().1;
        scene(&mut s);
        let mut rt = Runtime::new(s, ui::view, (1180.0, 1400.0), Env { system_dark: false, scale: 1.0 });
        rt.frame(0.0);
        // A worktree with uncommitted work: not ticked for you, but you can tick it.
        let risky = "wt:hotfix-1.4.2".to_owned();
        assert!(!rt.state.dev.picked.contains(&risky));
        click(&mut rt, &format!("f-{risky}"));
        assert!(rt.state.dev.picked.contains(&risky), "a click ticks it");
        click(&mut rt, &format!("f-{risky}"));
        assert!(!rt.state.dev.picked.contains(&risky), "and unticks it");
        // The group's box ticks every safe item, never the risky one.
        let safe = "wt:agent-3f9c21".to_owned();
        assert!(rt.state.dev.picked.contains(&safe));
        click(&mut rt, "cat-Worktrees/all");
        assert!(!rt.state.dev.picked.contains(&safe), "all off");
        click(&mut rt, "cat-Worktrees/all");
        assert!(rt.state.dev.picked.contains(&safe) && !rt.state.dev.picked.contains(&risky), "all on, safe ones only");
    }
}

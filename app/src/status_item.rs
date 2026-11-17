//! Tidy in the menu bar: free space at a glance, and a menu to jump into the app. AppKit's own
//! `NSStatusItem`, made on the main thread (Rune's tray covers Linux and Windows, not macOS).

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSCellImagePosition, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::NSString;

use crate::{App, Section, settings};

/// What the item shows.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Info {
    pub free: u64,
    pub total: u64,
    /// What Tidy found to reclaim (suggestions and stale developer items).
    pub reclaim: u64,
    pub scanning: bool,
    pub text: crate::settings::MenuBarText,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "TidyStatusItemTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(pick:))]
        fn pick(&self, sender: &NSMenuItem) {
            run(sender.tag());
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

thread_local! {
    static ITEM: RefCell<Option<(Retained<NSStatusItem>, Retained<Target>, Info)>> = const { RefCell::new(None) };
}

const OPEN: isize = 1;
const MAP: isize = 2;
const SUGGESTIONS: isize = 3;
const DEVELOPER: isize = 4;
const RESCAN: isize = 5;
const SETTINGS: isize = 6;
const QUIT: isize = 7;

fn run(tag: isize) {
    let Some(main) = settings::MAIN.get() else { return };
    if let Some(mtm) = MainThreadMarker::new() {
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
    match tag {
        SETTINGS => settings::open(),
        QUIT => main.quit(),
        _ => {
            main.open();
            main.update(move |s: &mut App, _| match tag {
                MAP => s.section = Section::Map,
                SUGGESTIONS => s.section = Section::Suggestions,
                DEVELOPER => s.section = Section::Developer,
                RESCAN => s.rescan(),
                _ => {}
            });
        }
    }
}

fn item(title: &str, tag: isize, target: &Target, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let i = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), Some(sel!(pick:)), &NSString::from_str("")) };
    unsafe { i.setTarget(Some(target)) };
    i.setTag(tag);
    i
}

fn label(title: &str, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let i = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(mtm), &NSString::from_str(title), None, &NSString::from_str("")) };
    i.setEnabled(false);
    i
}

/// Shows or refreshes the item. Main thread only (call from `poll` or an update).
pub fn show(info: Info) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    ITEM.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.as_ref().is_some_and(|(_, _, last)| *last == info) {
            return;
        }
        if cell.is_none() {
            let status = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            *cell = Some((status, Target::new(mtm), Info::default()));
        }
        let (status, target, last) = cell.as_mut().unwrap();
        *last = info;
        let gb = |b: u64| tidy_core::format::bytes(b);
        if let Some(button) = status.button(mtm) {
            let symbol = if info.total > 0 && info.free * 10 < info.total { "externaldrive.badge.exclamationmark" } else { "internaldrive" };
            if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), Some(&NSString::from_str("Disk space"))) {
                img.setTemplate(true);
                button.setImage(Some(&img));
                button.setImagePosition(NSCellImagePosition::ImageLeading);
            }
            let t = info.text.text(info.free, info.total);
            button.setTitle(&NSString::from_str(&if t.is_empty() { t } else { format!(" {t}") }));
        }
        let menu = NSMenu::new(mtm);
        if info.total > 0 {
            menu.addItem(&label(&format!("{} free of {}", gb(info.free), gb(info.total)), mtm));
        }
        if info.scanning {
            menu.addItem(&label("Scanning…", mtm));
        } else if info.reclaim > 0 {
            menu.addItem(&label(&format!("{} you can reclaim", gb(info.reclaim)), mtm));
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&item("Open Tidy", OPEN, target, mtm));
        menu.addItem(&item("Space Map", MAP, target, mtm));
        menu.addItem(&item("Suggestions", SUGGESTIONS, target, mtm));
        menu.addItem(&item("Developer", DEVELOPER, target, mtm));
        menu.addItem(&item("Scan Again", RESCAN, target, mtm));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&item("Settings…", SETTINGS, target, mtm));
        menu.addItem(&item("Quit Tidy", QUIT, target, mtm));
        status.setMenu(Some(&menu));
    });
}

/// Takes the item out of the menu bar.
pub fn hide() {
    ITEM.with(|cell| {
        if let Some((status, _, _)) = cell.borrow_mut().take() {
            NSStatusBar::systemStatusBar().removeStatusItem(&status);
        }
    });
}

impl App {
    /// What the menu bar item shows right now.
    pub fn status_info(&self) -> Info {
        let space = self.space.unwrap_or_default();
        let suggested: u64 = self.suggestions.iter().filter(|s| s.risk == tidy_core::Risk::Safe).map(|s| s.size).sum();
        let dev: u64 = self.dev.findings.iter().filter(|f| f.suggested(self.settings.stale_days)).map(|f| f.size).sum();
        Info { free: space.free, total: space.total, reclaim: suggested + dev, scanning: self.job.is_some(), text: self.settings.menu_bar_text }
    }

    /// Keeps the menu bar item current, and warns once a session when space runs low.
    pub fn sync_status_item(&mut self) {
        if headless() {
            return; // snapshots and scripts never touch the real menu bar
        }
        if !self.settings.menu_bar {
            hide();
        } else {
            show(self.status_info());
        }
        let info = self.status_info();
        let low = info.total > 0 && (info.free * 10 < info.total || info.free < 15_000_000_000);
        if low && self.settings.low_space_alert && !self.warned_low && rune_mac::notify::available() && !info.scanning {
            self.warned_low = true;
            let body = if info.reclaim > 0 {
                format!("{} free. Tidy found {} you can reclaim.", tidy_core::format::bytes(info.free), tidy_core::format::bytes(info.reclaim))
            } else {
                format!("{} free.", tidy_core::format::bytes(info.free))
            };
            let _ = rune_mac::notify::post(&rune_mac::notify::Notification::new("low-space", "Your Mac is running low on space").body(body));
        }
    }
}

/// A headless run of Rune's tooling (`--snap`, `--tree`, `--script`…), which has no menu bar.
fn headless() -> bool {
    static H: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *H.get_or_init(|| {
        std::env::args().any(|a| matches!(a.as_str(), "--snap" | "--tree" | "--script" | "--video" | "--strip" | "--trace" | "--list-scenes" | "--list-windows" | "--frames"))
    })
}

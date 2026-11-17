//! The window: a sidebar of places and views, the main pane, and the assistant as an inspector.

mod assistant;
mod demo;
mod changes;
mod developer;
mod duplicates;
mod lists;
mod map;
mod welcome;

use rune::{Align, Cx, El, Tx, col, el, icon, row, text};
use rune_kit::shell::{PaneMotion, SidebarHeader, SidebarRow, Sidebar, SidebarToggle, SplitView, Toolbar};
use rune_kit::{ControlSize, IconButton, IconButtonVariant};

use crate::theme::{Theme, theme};
use crate::{App, Section, icons};

pub use demo::SCENES;
#[cfg(test)]
pub use demo::demo_tree;

/// A scan of the folder already on screen: the map stays, with a progress pill in the toolbar.
fn refreshing(s: &App, job: &crate::ScanJob) -> bool {
    s.tree.as_ref().is_some_and(|t| t.root_path == job.path)
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let t = theme(s.settings.dark(cx.system_dark()), s.settings.accent);
    let main = if let Some(path) = &s.pending {
        welcome::access_needed(path, &t)
    } else if let Some(job) = s.job.as_ref().filter(|j| !refreshing(s, j)) {
        welcome::scanning(s, job, &t, cx)
    } else if s.tree.is_none() {
        welcome::welcome(s, &t)
    } else {
        match s.section {
            Section::Map => map::map_view(s, &t, cx),
            Section::Suggestions => lists::suggestions(s, &t),
            Section::LargeFiles => lists::large_files(s, &t),
            Section::Duplicates => duplicates::duplicates(s, &t),
            Section::Developer => developer::developer(s, &t),
            Section::Changes => changes::changes(s, &t),
        }
    };
    let mut main = col().w_full().h_full().child(main);
    if let Some(toast) = toast(s, &t) {
        main = main.child(toast);
    }
    SplitView::new(main)
        .sidebar(&s.sidebar, |s: &mut App| &mut s.sidebar, sidebar(s, &t))
        .inspector(&s.inspector, |s: &mut App| &mut s.inspector, assistant::panel(s, &t))
        .toolbar(toolbar(s, &t))
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .sidebar_padding(14.0, 10.0)
        .sidebar_toggle(SidebarToggle::new().size(ControlSize::Large).past_lights(14.0).edge(16.0).gap(4.0).in_header())
        .pane_motion(PaneMotion::new().scale_with_width(480.0, 0.15, 0.3).fade_tail(0.15))
        .reduced_motion(cx.is_reduced_motion())
        .build(&t.k)
}

fn sidebar(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let mut sb = Sidebar::new().label("Tidy").inset(0.0).row_gap(2.0).header_padding(0.0, 10.0);
    sb = sb.child(SidebarHeader::new("Views").build(k).key("h-views"));
    let has = s.tree.is_some();
    let n_sugg = s.suggestions.len();
    for (sec, label, ic, key) in [
        (Section::Map, "Space Map", &icons::MAP, "v-map"),
        (Section::Suggestions, "Suggestions", &icons::BROOM, "v-sugg"),
        (Section::LargeFiles, "Large Files", &icons::STACK, "v-large"),
        (Section::Duplicates, "Duplicates", &icons::COPIES, "v-dupes"),
        (Section::Developer, "Developer", &icons::TERMINAL, "v-dev"),
        (Section::Changes, "Changes", &icons::CLOCK, "v-changes"),
    ] {
        let mut r = SidebarRow::new(label).icon(ic).height(30.0).selected(has && s.section == sec && s.job.is_none());
        if sec == Section::Suggestions && n_sugg > 0 {
            r = r.detail(tidy_core::format::count(n_sugg as u64));
        }
        if sec == Section::Duplicates {
            if s.dupes.search.is_some() {
                r = r.trailing(rune_kit::Spinner::new().build(k).key("dupes-spinner"));
            } else {
                let extra: u64 = s.dupes.groups.iter().map(|g| g.extra()).sum();
                if extra > 0 {
                    r = r.detail(tidy_core::format::bytes(extra));
                }
            }
        }
        if sec == Section::Developer {
            let stale: u64 = s.dev.findings.iter().filter(|f| f.suggested(s.settings.stale_days)).map(|f| f.size).sum();
            if s.dev.checking.is_some() {
                // Still asking git and Xcode: the total is about to grow.
                r = r.trailing(rune_kit::Spinner::new().build(k).key("dev-spinner"));
            } else if stale > 0 {
                r = r.detail(tidy_core::format::bytes(stale));
            }
        }
        sb = sb.child(r.on_select(move |s: &mut App, _| s.section = sec).build(k).key(key));
    }
    sb = sb.child(SidebarHeader::new("Locations").build(k).key("h-places"));
    // The one location on screen: what's scanning, waiting for access, or the folder shown.
    let showing = match (&s.pending, &s.job, &s.tree) {
        (Some(p), _, _) => Some(p.clone()),
        (_, Some(j), _) => Some(j.path.clone()),
        (_, _, Some(t)) => Some(t.path(s.focus)),
        _ => None,
    };
    for (i, p) in s.places.iter().enumerate() {
        let current = showing.as_ref() == Some(&p.path);
        // Any scan in memory that covers the place knows its size.
        let size = s.tree.iter().chain(s.shelf.iter()).find_map(|t| t.find(&p.path).map(|id| t.node(id).size));
        let mut r = SidebarRow::new(p.name.clone())
            .icon(if p.drive { &icons::DRIVE } else if i == 0 { &icons::HOME } else { &icons::FOLDER })
            .height(30.0)
            .selected(current);
        if let Some(sz) = size {
            r = r.detail(tidy_core::format::bytes(sz));
        }
        let path = p.path.clone();
        sb = sb.child(r.on_select(move |s: &mut App, _| s.go_to(path.clone())).build(k).key(format!("place-{i}")));
    }
    sb = sb.child(
        SidebarRow::new("Choose Folder…")
            .icon(&icons::FOLDER_PLUS)
            .height(30.0)
            .on_select(|_: &mut App, tx: &mut Tx| pick_folder(tx))
            .build(k)
            .key("place-pick"),
    );
    let footer = disk_meter(s, t);
    sb.footer(footer).build(k)
}

/// Capacity of the scanned volume: a thin bar and "312 GB free of 1 TB". What Tidy moved to the
/// Trash still takes space until the Trash is emptied, so it shows as its own part of the bar.
fn disk_meter(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let Some(sp) = s.space.filter(|sp| sp.total > 0) else { return el() };
    let total = sp.total as f32;
    let trash = (s.in_trash as f32).min(total - sp.free as f32).max(0.0);
    let used = 1.0 - sp.free as f32 / total;
    let ink = if used > 0.9 { k.c.danger.icon } else { t.mint };
    let mut c = col()
        .key("disk")
        .w_full()
        .px(10.0)
        .py(10.0)
        .gap(6.0)
        .child(
            row()
                .w_full()
                .justify_between()
                .child(text("Macintosh HD").font(k.ty.small_medium).color(k.c.text_base))
                .child(text(format!("{:.0}%", used * 100.0)).font(k.ty.small).color(k.c.text_muted)),
        )
        .child(
            row()
                .w_full()
                .h(5.0)
                .radius(2.5)
                .clip_rounded(2.5)
                .fill(t.hairline)
                .child(el().key("disk-used").h(5.0).w_frac(used - trash / total).fill(ink).motion(rune::tok::motion::CALM))
                .child(el().key("disk-trash").h(5.0).w_frac(trash / total).fill(ink.alpha(0.35)).motion(rune::tok::motion::CALM)),
        )
        .child(
            text(format!("{} free of {}", tidy_core::format::bytes(sp.free), tidy_core::format::bytes(sp.total)))
                .key("disk-free")
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
    if s.in_trash > 0 {
        let confirming = s.confirm_empty.is_some();
        c = c.child(
            row()
                .key("trash-row")
                .w_full()
                .pt(2.0)
                .gap(6.0)
                .center_y()
                .child(icon(&icons::TRASH, 12.0).color(k.c.icon_muted))
                .child(text(format!("{} in the Trash", tidy_core::format::bytes(s.in_trash))).font(k.ty.small).color(k.c.text_muted).grow(1.0).min_w(0.0))
                .child(
                    row()
                        .key("empty-trash")
                        .px(7.0)
                        .py(2.0)
                        .radius(k.radius.sm)
                        .fill(if confirming { k.c.danger.icon.alpha(0.18) } else { k.c.hover })
                        .hover(k.c.hover)
                        .role("button")
                        .label(if confirming { "Confirm Empty Trash" } else { "Empty Trash" })
                        .help("Frees the space for good: items in the Trash can't be put back after this.")
                        .on_press_tx(|s: &mut App, tx: &mut Tx| {
                            if s.confirm_empty.is_some() {
                                s.empty_trash(tx);
                            } else {
                                s.confirm_empty = Some(std::time::Instant::now());
                                tx.wake_after(std::time::Duration::from_secs(4));
                            }
                        })
                        .child(
                            text(if confirming { "Empty for good?" } else { "Empty Trash" })
                                .font(k.ty.text_xsmall_medium)
                                .color(if confirming { k.c.danger.fg } else { k.c.text_base }),
                        ),
                ),
        );
    }
    c
}

fn toolbar(s: &App, t: &Theme) -> Toolbar {
    let k = &t.k;
    let refresh = s.job.as_ref().filter(|j| refreshing(s, j));
    let title = match (&s.tree, s.section) {
        (_, _) if s.job.is_some() && refresh.is_none() => "Scanning…".to_owned(),
        (None, _) => "Tidy".to_owned(),
        (Some(_), Section::Map) => "Space Map".to_owned(),
        (Some(_), Section::Suggestions) => "Suggestions".to_owned(),
        (Some(_), Section::LargeFiles) => "Large Files".to_owned(),
        (Some(_), Section::Duplicates) => "Duplicates".to_owned(),
        (Some(_), Section::Developer) => "Developer".to_owned(),
        (Some(_), Section::Changes) => "Changes".to_owned(),
    };
    let mut trailing = row().gap(2.0).center_y();
    // Refreshing the map on screen: say so, quietly, with how far along.
    if let Some(job) = refresh {
        let files = job.progress.files.load(std::sync::atomic::Ordering::Relaxed);
        trailing = trailing.child(
            row()
                .key("refresh-pill")
                .px(10.0)
                .py(4.0)
                .mr(6.0)
                .gap(6.0)
                .center_y()
                .radius(12.0)
                .fill(k.c.hover)
                .help(if s.from_snapshot { "Showing the last scan while a fresh one runs" } else { "Scanning again" })
                .child(rune_kit::Spinner::new().build(k))
                .child(
                    text(format!("{} · {} files", if s.from_snapshot { "Updating" } else { "Rescanning" }, tidy_core::format::count(files)))
                        .font(k.ty.small)
                        .color(k.c.text_muted),
                )
                .transition(rune::Transition::opacity()),
        );
    }
    if s.place.is_some() && s.job.is_none() {
        trailing = trailing.child(
            IconButton::new(&icons::RELOAD)
                .variant(IconButtonVariant::GhostMuted)
                .help("Scan Again")
                .shortcut("⌘R")
                .on_press(|s: &mut App| s.rescan())
                .build(k)
                .key("rescan"),
        );
    }
    trailing = trailing.child(
        IconButton::new(&icons::SPARKLES)
            .variant(if s.inspector.is_open() { IconButtonVariant::Ghost } else { IconButtonVariant::GhostMuted })
            .expanded(s.inspector.is_open())
            .help(if s.inspector.is_open() { "Hide Assistant" } else { "Show Assistant" })
            .shortcut("⌘J")
            .on_press(|s: &mut App| s.inspector.toggle())
            .build(k)
            .key("assistant-toggle"),
    );
    Toolbar::new().title(title).trailing(trailing)
}

fn toast(s: &App, t: &Theme) -> Option<El> {
    let (msg, _) = s.toast.as_ref()?;
    let k = &t.k;
    Some(
        row()
            .key("toast")
            .pin(Some(0.0), None, Some(0.0), Some(16.0))
            .center()
            .child(
                row()
                    .px(14.0)
                    .py(8.0)
                    .gap(8.0)
                    .center_y()
                    .radius(18.0)
                    .fill(k.c.bg_inverse)
                    .shadow(4.0, 16.0, rune::Color::BLACK.alpha(0.2))
                    .child(icon(&icons::TRASH, 13.0).color(k.c.text_inverse))
                    .child(text(msg.clone()).font(k.ty.ui).color(k.c.text_inverse).align(Align::Center)),
            )
            .transition(rune::Transition::opacity()),
    )
}

/// The system's folder picker, as a sheet.
pub fn pick_folder(tx: &mut Tx) {
    let options = rune::FileDialogOptions { title: "Choose a folder to scan".into(), ..Default::default() };
    tx.open_folder(options, |s: &mut App, result, _| {
        if let Ok(Some(path)) = result {
            s.go_to(path);
        }
    });
}

/// The native context menu for an item on the map or in a list.
pub fn item_menu(path: std::path::PathBuf, is_dir: bool, anchor: rune::PointerPress, tx: &mut Tx) {
    use rune::{PopupItem, PopupMenu};
    let mut menu = PopupMenu::new();
    if is_dir {
        menu = menu.item(PopupItem::new("zoom", "Zoom In").symbol("plus.magnifyingglass"));
    }
    menu = menu
        .item(PopupItem::new("open", "Open").symbol("arrow.up.forward.app"))
        .item(PopupItem::new("reveal", "Show in Finder").symbol("folder"))
        .item(PopupItem::new("copy", "Copy Path").symbol("doc.on.doc"))
        .separator()
        .item(PopupItem::new("ask", "Explain with Assistant").symbol("sparkles"))
        .separator()
        .item(PopupItem::new("trash", "Move to Trash").symbol("trash").key_equivalent("cmd+backspace".parse().unwrap()));
    tx.popup_menu(menu, anchor, move |s: &mut App, picked, tx| {
        let p = path.to_string_lossy().into_owned();
        match picked {
            Some("zoom") => {
                if let Some(id) = s.tree.as_ref().and_then(|t| t.find(&path)) {
                    s.open_folder(id);
                }
            }
            Some("open") => {
                let _ = std::process::Command::new("open").arg(&path).spawn();
            }
            Some("reveal") => {
                let _ = std::process::Command::new("open").arg("-R").arg(&path).spawn();
            }
            Some("copy") => tx.copy(p),
            Some("ask") => {
                if !s.inspector.is_open() {
                    s.inspector.toggle();
                }
                s.ask(format!(
                    "Explain {} in two or three sentences: what it is, what fills it, and whether any of it is safe to remove. Point at anything specific with show_in_tidy.",
                    crate::tilde(&p)
                ));
            }
            Some("trash") => {
                s.trash(&[p]);
            }
            _ => {}
        }
    });
}

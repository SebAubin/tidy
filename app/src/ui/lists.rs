//! Suggestions (Tidy's own, offline) and the largest files.

use rune::{Align, El, Tx, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, ControlSize};
use tidy_core::Risk;
use tidy_core::format::{age, bytes};

use crate::theme::{Theme, kind_color};
use crate::{App, icons, plural, tilde};

fn page(key: &str, header: El, body: El, footer: Option<El>, _t: &Theme) -> El {
    let mut c = col()
        .key(key)
        .w_full()
        .h_full()
        .min_h(0.0)
        
        .child(header)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(body));
    if let Some(f) = footer {
        c = c.child(f);
    }
    c
}

fn heading(title: &str, sub: String, t: &Theme) -> El {
    let k = &t.k;
    col()
        .w_full()
        .px(24.0)
        .pt(18.0)
        .pb(12.0)
        .gap(3.0)
        .child(text(title.to_owned()).font(rune::TextStyle { size: 20.0, weight: 700.0, ..k.ty.body_medium }).color(k.c.text_base))
        .child(text(sub).font(k.ty.ui).color(k.c.text_muted).wrap())
}

pub fn suggestions(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let total: u64 = s.suggestions.iter().map(|x| x.size).sum();
    let picked: Vec<&tidy_core::Suggestion> = s.suggestions.iter().filter(|x| s.ticked.contains(&x.path)).collect();
    let picked_size: u64 = picked.iter().map(|x| x.size).sum();
    let header = heading(
        "Suggestions",
        if s.suggestions.is_empty() {
            "Nothing obvious to clean up here.".into()
        } else {
            format!("{} you can likely reclaim, in places that rebuild themselves or that you've left behind.", bytes(total))
        },
        t,
    );
    // Select All / Deselect All, beside the heading.
    let all_on = !s.suggestions.is_empty() && s.suggestions.iter().all(|x| s.ticked.contains(&x.path));
    let header = if s.suggestions.is_empty() {
        header
    } else {
        row().w_full().items_start().child(header.grow(1.0).min_w(0.0)).child(
            row().pt(18.0).pr(20.0).child(
                Button::new(if all_on { "Deselect All" } else { "Select All" })
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut App| {
                        if all_on {
                            s.ticked.clear();
                        } else {
                            s.ticked = s.suggestions.iter().map(|x| x.path.clone()).collect();
                        }
                    })
                    .build(k)
                    .key("sugg-all"),
            ),
        )
    };
    let mut body = col().key("sugg-list").w_full().px(16.0).pb(16.0).gap(6.0);
    for (i, sg) in s.suggestions.iter().enumerate() {
        let on = s.ticked.contains(&sg.path);
        let path = sg.path.clone();
        let p2 = sg.path.clone();
        let safe = sg.risk == Risk::Safe;
        body = body.child(
            row()
                .key(format!("sg{i}"))
                .w_full()
                .px(12.0)
                .py(10.0)
                .gap(12.0)
                .center_y()
                .radius(k.radius.md)
                .fill(if on { t.mint.alpha(0.10) } else { k.c.layer_01 })
                .border(1.0, if on { t.mint.alpha(0.35) } else { t.hairline })
                .on_press(move |s: &mut App| {
                    if !s.ticked.remove(&path) {
                        s.ticked.insert(path.clone());
                    }
                })
                .on_secondary_press(move |_: &mut App, p, tx: &mut Tx| super::item_menu(p2.clone().into(), true, p, tx))
                .label(sg.title).role("checkbox").checked(on)
                .child(Checkbox::new(on).build(k).a11y_hidden().inert())
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(
                            row()
                                .gap(8.0)
                                .center_y()
                                .child(text(sg.title).font(k.ty.ui_medium).color(k.c.text_base))
                                .child(risk_badge(safe, t)),
                        )
                        .child(text(tilde(&sg.path)).font(k.ty.small).color(k.c.text_muted).truncate_middle())
                        .child(text(sg.why.clone()).font(k.ty.small).color(k.c.text_faint).wrap()),
                )
                .child(text(bytes(sg.size)).font(k.ty.ui_medium).color(k.c.text_base).align(Align::End)),
        );
    }
    let footer = (!s.suggestions.is_empty()).then(|| {
        let paths: Vec<String> = picked.iter().map(|x| x.path.clone()).collect();
        row()
            .key("sugg-footer")
            .w_full()
            .px(20.0)
            .py(12.0)
            .gap(12.0)
            .center_y()
            .border_sides([1.0, 0.0, 0.0, 0.0], t.hairline)
            .child(icon(&icons::SHIELD, 15.0).color(k.c.icon_muted))
            .child(
                text(format!("Items go to the {T}, so you can put them back from {}.", crate::os::FILE_MANAGER, T = crate::os::TRASH))
                    .font(k.ty.small)
                    .color(k.c.text_muted)
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(
                Button::new(format!("Move {} to {} · {}", plural(paths.len(), "item"), crate::os::TRASH, bytes(picked_size)))
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Large)
                    .disabled(paths.is_empty())
                    .on_press(move |s: &mut App| {
                        s.trash(&paths);
                    })
                    .build(k)
                    .key("trash-picked"),
            )
    });
    page("suggestions", header, body, footer, t)
}

pub fn risk_badge(safe: bool, t: &Theme) -> El {
    let k = &t.k;
    let (label, ink) = if safe { ("Safe", t.mint) } else { ("Review", k.c.warning.icon) };
    row().px(6.0).py(1.0).radius(4.0).fill(ink.alpha(0.16)).child(text(label).font(k.ty.text_xsmall_medium).color(ink))
}

pub fn large_files(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let Some(tree) = &s.tree else { return el() };
    let now = tidy_core::scan::now();
    let total: u64 = s.largest.iter().map(|&i| tree.node(i).size).sum();
    let header = heading(
        "Large Files",
        format!("The {} largest files over 10 MB, {} together. ⌘-click or ⇧-click to pick several, ⌘A for all.", s.largest.len(), bytes(total)),
        t,
    );
    let mut body = col().key("large-list").w_full().px(16.0).pb(16.0);
    for (i, &id) in s.largest.iter().enumerate().take(500) {
        let n = tree.node(id);
        let path = tree.path(id);
        let parent = path.parent().map(|p| tilde(&p.to_string_lossy())).unwrap_or_default();
        let picked = s.picked.contains(&id);
        let p2 = path.clone();
        body = body.child(
            row()
                .key(format!("lf{id}"))
                .w_full()
                .h(48.0)
                .px(10.0)
                .gap(12.0)
                .center_y()
                .radius(k.radius.sm)
                .role("row")
                .label(n.name.to_string())
                .checked(picked)
                .fill(if picked {
                    t.mint.alpha(0.20)
                } else if i % 2 == 1 {
                    k.c.text_base.alpha(0.025)
                } else {
                    rune::Color::TRANSPARENT
                })
                .on_press_tx(move |s: &mut App, tx: &mut Tx| pick(s, i, id, tx.mods()))
                .on_double_press(move |_: &mut App| {
                    crate::os::reveal(&[&p2]);
                })
                .on_secondary_press(move |s: &mut App, p, tx: &mut Tx| {
                    if !s.picked.contains(&id) {
                        s.picked = [id].into();
                        s.pick_anchor = Some(i);
                    }
                    if s.picked.len() > 1 {
                        picked_menu(p, tx);
                    } else {
                        super::item_menu(path.clone(), false, p, tx);
                    }
                })
                .child(el().size(10.0, 10.0).radius(3.0).fill(kind_color(n.kind, t.dark)))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(1.0)
                        .child(text(n.name.to_string()).font(k.ty.ui).color(k.c.text_base).truncate_middle())
                        .child(text(parent).font(k.ty.small).color(k.c.text_faint).truncate_middle()),
                )
                .child(text(format!("opened {}", age(now - n.accessed))).font(k.ty.small).color(k.c.text_muted).w(140.0).align(Align::End))
                .child(text(bytes(n.size)).font(k.ty.ui_medium).color(k.c.text_base).w(80.0).align(Align::End)),
        );
    }
    let footer = (!s.picked.is_empty()).then(|| {
        let size: u64 = s.picked.iter().map(|&id| tree.node(id).size).sum();
        row()
            .key("large-footer")
            .w_full()
            .px(20.0)
            .py(12.0)
            .gap(10.0)
            .center_y()
            .border_sides([1.0, 0.0, 0.0, 0.0], t.hairline)
            .child(text(format!("{} selected · {}", plural(s.picked.len(), "file"), bytes(size))).font(k.ty.ui_medium).color(k.c.text_base).grow(1.0))
            .child(Button::new("Deselect").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.picked.clear()).build(k).key("deselect"))
            .child(
                Button::new(crate::os::reveal_label())
                    .variant(ButtonVariant::Neutral)
                    .on_press(|s: &mut App| reveal_picked(s))
                    .build(k)
                    .key("reveal-picked"),
            )
            .child(
                Button::new(format!("Move to {} · {}", crate::os::TRASH, bytes(size)))
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Large)
                    .on_press(|s: &mut App| trash_picked(s))
                    .build(k)
                    .key("trash-large"),
            )
    });
    page("large", header, body, footer, t)
}

/// Finder-style picking: a click picks one, ⌘-click toggles, ⇧-click extends from the anchor.
fn pick(s: &mut App, i: usize, id: tidy_core::NodeId, mods: rune::Mods) {
    if mods.shift
        && let Some(a) = s.pick_anchor
    {
        let (lo, hi) = (a.min(i), a.max(i));
        if !mods.cmd {
            s.picked.clear();
        }
        s.picked.extend(s.largest[lo..=hi.min(s.largest.len() - 1)].iter().copied());
    } else if mods.cmd {
        if !s.picked.remove(&id) {
            s.picked.insert(id);
        }
        s.pick_anchor = Some(i);
    } else {
        s.picked = [id].into();
        s.pick_anchor = Some(i);
    }
    s.selected = Some(id);
}

fn picked_paths(s: &App) -> Vec<String> {
    s.picked.iter().filter_map(|&id| s.path_of(id)).map(|p| p.to_string_lossy().into_owned()).collect()
}

fn trash_picked(s: &mut App) {
    let paths = picked_paths(s);
    s.trash(&paths);
    s.picked.clear();
}

fn reveal_picked(s: &App) {
    crate::os::reveal(&picked_paths(s));
}

fn picked_menu(anchor: rune::PointerPress, tx: &mut Tx) {
    use rune::{PopupItem, PopupMenu};
    let menu = PopupMenu::new()
        .item(PopupItem::new("reveal", crate::os::reveal_label()).symbol("folder"))
        .item(PopupItem::new("copy", "Copy Paths").symbol("doc.on.doc"))
        .separator()
        .item(PopupItem::new("trash", format!("Move to {}", crate::os::TRASH)).symbol("trash"));
    tx.popup_menu(menu, anchor, |s: &mut App, picked, tx| match picked {
        Some("reveal") => reveal_picked(s),
        Some("copy") => tx.copy(picked_paths(s).join("\n")),
        Some("trash") => trash_picked(s),
        _ => {}
    });
}

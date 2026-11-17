//! Duplicates: files stored more than once, each group with the copy to keep and the extras.

use std::sync::atomic::Ordering;

use rune::{Align, El, Tx, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, ControlSize};
use tidy_core::format::{age, bytes};

use crate::theme::Theme;
use crate::{App, icons, plural, tilde};

pub fn duplicates(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let d = &s.dupes;
    let stale = d.for_root.as_ref() != s.tree.as_ref().map(|t| &t.root_path);
    let title = rune::TextStyle { size: 20.0, weight: 700.0, ..k.ty.body_medium };

    // Not searched yet (or for another scan): say what it does, then let the person start it.
    if (d.groups.is_empty() || stale) && d.search.is_none() {
        let ran = d.for_root.is_some() && !stale;
        return col()
            .key("dupes-start")
            .w_full()
            .h_full()
            .center()
            .gap(14.0)
            .px(40.0)
            .child(icon(&icons::COPIES, 40.0).color(t.mint))
            .child(text(if ran { "No duplicates found" } else { "Find duplicate files" }).font(title).color(k.c.text_base))
            .child(
                text(if ran {
                    "Your own files (outside app libraries and build folders) are each stored once."
                } else {
                    "Tidy compares files of the same size by their contents, reading only what it must. Nothing leaves this computer, and one copy of each is always kept."
                })
                .font(k.ty.body)
                .color(k.c.text_muted)
                .align(Align::Center)
                .wrap()
                .max_w(460.0),
            )
            .child(
                Button::new(if ran { "Search Again" } else { "Find Duplicates" })
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Large)
                    .on_press(|s: &mut App| s.find_dupes())
                    .build(k)
                    .key("dupes-find"),
            );
    }

    if let Some((p, _)) = &d.search {
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
        return col()
            .key("dupes-searching")
            .w_full()
            .h_full()
            .center()
            .gap(12.0)
            .child(text("Comparing files…").font(title).color(k.c.text_base))
            .child(rune_kit::Progress::new(Some((done as f32 / total as f32).min(1.0))).label("Comparing files").build(k).w(360.0))
            .child(text(format!("{} of at most {} read", bytes(done), bytes(total))).font(k.ty.small).color(k.c.text_muted))
            .child(Button::new("Stop").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.stop_dupes()).build(k).key("dupes-stop"));
    }

    let extra: u64 = d.groups.iter().map(|g| g.extra()).sum();
    let now = tidy_core::scan::now();
    let header = row()
        .w_full()
        .px(24.0)
        .pt(18.0)
        .pb(12.0)
        .items_start()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(3.0)
                .child(text("Duplicates").font(title).color(k.c.text_base))
                .child(
                    text(format!("{} stored more than once, {} in extra copies. The copy to keep is the original: outside Downloads and Desktop, then the oldest.", plural(d.groups.len(), "file"), bytes(extra)))
                        .font(k.ty.ui)
                        .color(k.c.text_muted)
                        .wrap(),
                ),
        )
        .child(
            row().pt(4.0).child(
                Button::new("Search Again").variant(ButtonVariant::Ghost).control_size(ControlSize::Small).on_press(|s: &mut App| s.find_dupes()).build(k).key("dupes-again"),
            ),
        );
    let mut body = col().key("dupes-list").w_full().px(16.0).pb(16.0).gap(10.0);
    for (gi, g) in d.groups.iter().enumerate().take(200) {
        let name = g.copies[0].path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut card = col()
            .key(format!("dg{gi}"))
            .w_full()
            .radius(k.radius.lg)
            .border(1.0, t.hairline)
            .fill(k.c.layer_01)
            .clip_rounded(k.radius.lg)
            .child(
                row()
                    .w_full()
                    .px(12.0)
                    .pt(10.0)
                    .pb(6.0)
                    .gap(8.0)
                    .center_y()
                    .child(text(name).font(k.ty.ui_medium).color(k.c.text_base).truncate_middle().grow(1.0).min_w(0.0))
                    .child(text(format!("{} copies · {} each", g.copies.len(), bytes(g.size))).font(k.ty.small).color(k.c.text_muted)),
            );
        for c in &g.copies {
            let on = d.picked.contains(&c.path);
            let path = c.path.clone();
            let reveal = c.path.clone();
            card = card.child(
                row()
                    .key(format!("dc-{}", c.path.display()))
                    .w_full()
                    .px(12.0)
                    .py(7.0)
                    .gap(10.0)
                    .center_y()
                    .role("checkbox")
                    .label(c.path.to_string_lossy().into_owned())
                    .checked(on)
                    .hover(k.c.hover)
                    .on_press(move |s: &mut App| s.toggle_copy(gi, &path))
                    .on_secondary_press(move |_: &mut App, p, tx: &mut Tx| super::item_menu(reveal.clone(), false, p, tx))
                    .child(Checkbox::new(on).build(k).a11y_hidden().inert())
                    .child(text(tilde(&c.path.to_string_lossy())).font(k.ty.small).color(if on { k.c.text_muted } else { k.c.text_base }).truncate_middle().grow(1.0).min_w(0.0))
                    .child(if on {
                        el()
                    } else {
                        row().key("keep").px(6.0).py(1.0).radius(4.0).fill(t.mint.alpha(0.16)).child(text("Keep").font(k.ty.text_xsmall_medium).color(t.mint))
                    })
                    .child(text(format!("modified {}", age(now - c.modified))).font(k.ty.small).color(k.c.text_faint).w(150.0).align(Align::End)),
            );
        }
        body = body.child(card);
    }
    let size: u64 = d.groups.iter().map(|g| g.size * g.copies.iter().filter(|c| d.picked.contains(&c.path)).count() as u64).sum();
    let footer = row()
        .key("dupes-footer")
        .w_full()
        .px(20.0)
        .py(12.0)
        .gap(10.0)
        .center_y()
        .border_sides([1.0, 0.0, 0.0, 0.0], t.hairline)
        .child(text(format!("Extra copies go to the {}; one of each always stays.", crate::os::TRASH)).font(k.ty.small).color(k.c.text_muted).grow(1.0).min_w(0.0).truncate())
        .child(
            Button::new(format!("Move {} to {} · {}", if d.picked.len() == 1 { "1 copy".to_owned() } else { format!("{} copies", d.picked.len()) }, crate::os::TRASH, bytes(size)))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Large)
                .disabled(d.picked.is_empty())
                .on_press(|s: &mut App| s.trash_dupes())
                .build(k)
                .key("dupes-trash"),
        );
    col()
        .key("duplicates")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(header)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(body))
        .child(footer)
}

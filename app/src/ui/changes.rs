//! Changes: where space went (or came back) since an earlier scan of the same folder.

use rune::{Align, El, Tx, col, el, icon, row, text};
use rune_kit::SegmentedControl;
use tidy_core::format::{age, bytes};
use tidy_core::history::{Summary, baseline, compare};

use crate::theme::Theme;
use crate::{App, icons, tilde};

const AGES: [(&str, i64); 3] = [("Last scan", 0), ("A week ago", 7 * 86_400), ("A month ago", 30 * 86_400)];

fn signed(d: i64) -> String {
    if d >= 0 { format!("+{}", bytes(d as u64)) } else { format!("−{}", bytes(d.unsigned_abs())) }
}

pub fn changes(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let Some(tree) = &s.tree else { return el() };
    let title = rune::TextStyle { size: 20.0, weight: 700.0, ..k.ty.body_medium };
    let now = Summary::of(tree);
    let base = baseline(&s.history, now.scanned_at, s.changes_age);
    let picker = SegmentedControl::new(&AGES.map(|a| a.0), AGES.iter().position(|a| a.1 == s.changes_age))
        .label("Compare with")
        .fit()
        .on_pick(|s: &mut App, i, _| s.changes_age = AGES[i].1)
        .build(k);
    let Some(base) = base else {
        return col()
            .key("changes-empty")
            .w_full()
            .h_full()
            .center()
            .gap(12.0)
            .px(40.0)
            .child(icon(&icons::CLOCK, 40.0).color(t.mint))
            .child(text("Nothing to compare yet").font(title).color(k.c.text_base))
            .child(
                text(if s.history.is_empty() {
                    "Tidy remembers each scan. Scan this folder again another day and this shows which folders grew and which shrank."
                } else {
                    "No scan of this folder is that old yet. Pick a closer point, or come back later."
                })
                .font(k.ty.body)
                .color(k.c.text_muted)
                .align(Align::Center)
                .wrap()
                .max_w(440.0),
            )
            .child(picker);
    };
    let list = compare(base, &now);
    // The root's own entry is the folder's size at each scan.
    let size = |x: &Summary| x.folders.get("").copied().unwrap_or(x.total);
    let (then, now_total) = (size(base), size(&now));
    let net = now_total as i64 - then as i64;
    let biggest = list.iter().map(|c| c.delta().unsigned_abs()).max().unwrap_or(1).max(1) as f32;
    let since = age(now.scanned_at - base.scanned_at);
    let header = row()
        .w_full()
        .px(24.0)
        .pt(18.0)
        .pb(12.0)
        .gap(12.0)
        .items_start()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(3.0)
                .child(text("Changes").font(title).color(k.c.text_base))
                .child(
                    text(format!(
                        "{} since the scan {} ({} then, {} now).",
                        signed(net),
                        if since == "today" { "earlier today".to_owned() } else { since },
                        bytes(then),
                        bytes(now_total)
                    ))
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
                ),
        )
        .child(picker);
    let mut body = col().key("changes-list").w_full().px(16.0).pb(16.0).gap(2.0);
    if list.is_empty() {
        body = body.child(text("No folder moved by more than 100 MB.").font(k.ty.ui).color(k.c.text_muted).px(8.0).py(12.0));
    }
    for (i, c) in list.iter().enumerate().take(60) {
        let d = c.delta();
        let grew = d > 0;
        let ink = if grew { k.c.warning.icon } else { t.mint };
        let path = if c.rel.is_empty() { tree.root_path.clone() } else { tree.root_path.join(&c.rel) };
        let label = tilde(&path.to_string_lossy());
        let target = path.clone();
        body = body.child(
            row()
                .key(format!("ch{i}"))
                .w_full()
                .px(10.0)
                .py(8.0)
                .gap(12.0)
                .center_y()
                .radius(k.radius.sm)
                .hover(k.c.hover)
                .help("Show on the map")
                .on_press_tx(move |s: &mut App, _: &mut Tx| {
                    s.go_to(target.clone());
                    s.section = crate::Section::Map;
                })
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(4.0)
                        .child(text(label).font(k.ty.ui).color(k.c.text_base).truncate_middle())
                        .child(el().w_full().h(4.0).radius(2.0).fill(t.hairline).child(el().h(4.0).w_frac((d.unsigned_abs() as f32 / biggest).max(0.01)).radius(2.0).fill(ink))),
                )
                .child(text(format!("{} → {}", bytes(c.before), bytes(c.after))).font(k.ty.small).color(k.c.text_faint).w(170.0).align(Align::End))
                .child(text(signed(d)).font(k.ty.ui_medium).color(ink).w(90.0).align(Align::End)),
        );
    }
    col()
        .key("changes")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(header)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(body))
}

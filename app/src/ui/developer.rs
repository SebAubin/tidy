//! The Developer view: what a developer's Mac piles up, grouped by kind, each group with a tip,
//! each item with how Tidy would clean it. Stale things come ticked; blocked ones can't be.

use rune::{Align, El, Tx, col, el, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, ControlSize};
use tidy_core::dev::{Action, Category, Finding};
use tidy_core::format::{age, bytes};

use crate::theme::Theme;
use crate::{App, plural};


pub fn developer(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let d = &s.dev;
    let days = s.settings.stale_days;
    let stale_total: u64 = d.findings.iter().filter(|f| f.suggested(days)).map(|f| f.size).sum();
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
                .child(text("Developer").font(rune::TextStyle { size: 20.0, weight: 700.0, ..k.ty.body_medium }).color(k.c.text_base))
                .child(
                    text(if d.findings.is_empty() && d.checking.is_none() {
                        "Nothing developer-shaped found in this scan.".to_owned()
                    } else {
                        format!(
                            "{} in things unused for over {}. Stale items come ticked; nothing runs until you press Clean.",
                            bytes(stale_total),
                            stale_label(days)
                        )
                    })
                    .font(k.ty.ui)
                    .color(k.c.text_muted)
                    .wrap(),
                ),
        )
        .child(if d.checking.is_some() {
            let (waiting, all) = d.pending();
            let what = if waiting > 0 {
                format!("Checking {} of {} worktrees with git…", all - waiting + 1, all)
            } else {
                "Asking Xcode and Docker…".to_owned()
            };
            row().key("dev-checking").gap(8.0).center_y().pt(6.0).child(rune_kit::Spinner::new().build(k)).child(text(what).font(k.ty.small).color(k.c.text_muted))
        } else {
            Button::new("Check Again")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(|s: &mut App| {
                    if let Some(root) = s.tree.as_ref().map(|t| t.root_path.clone()) {
                        s.start_dev(&root);
                    }
                })
                .build(k)
                .key("dev-refresh")
        });

    let mut body = col().key("dev-groups").w_full().px(16.0).pb(16.0).gap(14.0);
    for cat in Category::ALL {
        let items: Vec<&Finding> = d.findings.iter().filter(|f| f.category == cat && f.size > 0).collect();
        if !items.is_empty() {
            body = body.child(group(s, cat, &items, t));
        }
    }
    if !d.failures.is_empty() {
        let mut errs = col().w_full().px(12.0).py(10.0).gap(4.0).radius(k.radius.md).fill(k.c.danger.icon.alpha(0.10));
        for (what, why) in &d.failures {
            errs = errs.child(text(format!("{what}: {why}")).font(k.ty.small).color(k.c.danger.fg).wrap_anywhere());
        }
        body = body.child(errs.key("dev-failures"));
    }

    let ids = d.effective();
    let picked: Vec<&Finding> = d.findings.iter().filter(|f| ids.contains(&f.id)).collect();
    let size: u64 = picked.iter().map(|f| f.size).sum();
    let tools = picked.iter().filter(|f| matches!(f.action, Action::Run { .. })).count();
    let losing = picked.iter().filter(|f| f.warning.is_some()).count();
    let footer = row()
        .key("dev-footer")
        .w_full()
        .px(20.0)
        .py(12.0)
        .gap(10.0)
        .center_y()
        .border_sides([1.0, 0.0, 0.0, 0.0], t.hairline)
        .child(
            text(match &d.cleaning {
                Some(c) => format!("Cleaning {} of {}…", (c.done + 1).min(c.total), c.total),
                None if losing > 0 => format!("{} ticked will lose uncommitted work", plural(losing, "item")),
                None if tools > 0 => format!("{} to the Trash, {} through their own tool", plural(picked.len() - tools, "item"), plural(tools, "item")),
                None => "Items go to the Trash, so you can put them back.".to_owned(),
            })
            .font(k.ty.small)
            .color(k.c.text_muted)
            .grow(1.0)
            .min_w(0.0)
            .truncate(),
        )
        .child(
            Button::new(format!("Clean {} · {}", plural(picked.len(), "item"), bytes(size)))
                .variant(if losing > 0 { ButtonVariant::Danger } else { ButtonVariant::Contrast })
                .control_size(ControlSize::Large)
                .disabled(picked.is_empty() || d.cleaning.is_some())
                .on_press(|s: &mut App| s.clean_dev())
                .build(k)
                .key("dev-clean"),
        );

    col()
        .key("developer")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(header)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).child(body))
        .child(footer)
}

fn stale_label(days: u32) -> String {
    match days {
        14 => "2 weeks".into(),
        30 => "a month".into(),
        90 => "3 months".into(),
        180 => "6 months".into(),
        d => format!("{d} days"),
    }
}

fn group(s: &App, cat: Category, items: &[&Finding], t: &Theme) -> El {
    let k = &t.k;
    let total: u64 = items.iter().map(|f| f.size).sum();
    // "All" means the safe ones: anything that loses work is ticked one by one.
    let ids: Vec<String> = items.iter().filter(|f| f.blocked.is_none() && f.warning.is_none()).map(|f| f.id.clone()).collect();
    let all_on = !ids.is_empty() && ids.iter().all(|i| s.dev.picked.contains(i));
    let mut card = col()
        .key(format!("cat-{cat:?}"))
        .w_full()
        .radius(k.radius.lg)
        .border(1.0, t.hairline)
        .fill(k.c.layer_01)
        .clip_rounded(k.radius.lg)
        .child(
            // The whole header (title, tip, totals) ticks or unticks the group, like its box.
            col()
                .key("all")
                .w_full()
                .role("checkbox")
                .label(format!("All {}", cat.title()))
                .checked(all_on)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .on_press(move |s: &mut App| {
                    if all_on {
                        for i in &ids {
                            s.dev.picked.remove(i);
                        }
                    } else {
                        s.dev.picked.extend(ids.iter().cloned());
                    }
                })
                .child(
                    row()
                        .w_full()
                        .px(12.0)
                        .pt(10.0)
                        .pb(4.0)
                        .gap(10.0)
                        .center_y()
                        .child(Checkbox::new(all_on).build(k).a11y_hidden().inert())
                        .child(text(cat.title()).font(k.ty.ui_medium).color(k.c.text_base).grow(1.0))
                        .child(if items.iter().any(|f| crate::dev::checking(f)) {
                            row().gap(6.0).center_y().child(rune_kit::Spinner::new().build(k)).child(text(format!("{} · {}", plural(items.len(), "item"), bytes(total))).font(k.ty.small).color(k.c.text_muted))
                        } else {
                            row().child(text(format!("{} · {}", plural(items.len(), "item"), bytes(total))).font(k.ty.small).color(k.c.text_muted))
                        }),
                )
                .child(text(cat.tip()).font(k.ty.small).color(k.c.text_faint).wrap().px(12.0).pl(42.0).pb(8.0)),
        );
    for f in items.iter().take(60) {
        card = card.child(row().w_full().pl(42.0).child(el().h(1.0).grow(1.0).fill(t.hairline))).child(item(s, f, t));
    }
    if items.len() > 60 {
        card = card.child(text(format!("and {} more", items.len() - 60)).font(k.ty.small).color(k.c.text_faint).px(42.0).py(8.0));
    }
    card
}

fn item(s: &App, f: &Finding, t: &Theme) -> El {
    let k = &t.k;
    let days = s.settings.stale_days;
    let on = s.dev.picked.contains(&f.id);
    let blocked = f.blocked.is_some();
    let id = f.id.clone();
    let path = f.path.clone();
    let mut badges = row().gap(5.0).center_y();
    let within = s.dev.covered_by(f).map(|o| o.title.clone());
    if let Some(outer) = &within {
        badges = badges.child(badge(&format!("Goes with {outer}"), k.c.text_muted, t));
    } else if let Some(why) = &f.blocked {
        badges = badges.child(badge(why, k.c.text_muted, t));
    } else if let Some(risk) = &f.warning {
        badges = badges.child(badge(risk, k.c.warning.icon, t));
    } else if f.keep {
        badges = badges.child(badge("Keep", k.c.text_muted, t));
    } else if f.stale(days) {
        badges = badges.child(badge("Stale", t.mint, t));
    }
    let used = match f.last_used {
        Some(when) => format!("used {}", age(tidy_core::scan::now() - when)),
        None if f.category == Category::Docker => String::new(),
        None => "never used".into(),
    };
    let mut r = row()
        .key(format!("f-{}", f.id))
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(10.0)
        .center_y()
        .role("checkbox")
        .label(f.title.clone())
        .checked(on)
        .opacity(if blocked { 0.6 } else { 1.0 })
        .child(Checkbox::new(on).disabled(blocked).build(k).a11y_hidden().inert())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(row().w_full().gap(8.0).center_y().child(text(f.title.clone()).font(k.ty.ui).color(k.c.text_base).truncate().shrink().min_w(0.0)).child(badges))
                .child(text(f.detail.clone()).font(k.ty.small).color(k.c.text_muted).truncate_middle())
                .child(text(f.action.label()).font(k.ty.text_xsmall).color(k.c.text_faint)),
        )
        .child(text(used).font(k.ty.small).color(k.c.text_muted).w(120.0).align(Align::End))
        .child(text(bytes(f.size)).font(k.ty.ui_medium).color(k.c.text_base).w(76.0).align(Align::End));
    if !blocked {
        r = r.hover(k.c.hover).on_press(move |s: &mut App| {
            if !s.dev.picked.remove(&id) {
                s.dev.picked.insert(id.clone());
            }
        });
    }
    if let Some(p) = path {
        r = r.on_secondary_press(move |_: &mut App, press, tx: &mut Tx| super::item_menu(p.clone(), true, press, tx));
    }
    r
}

fn badge(label: &str, ink: rune::Color, t: &Theme) -> El {
    let k = &t.k;
    row().px(6.0).py(1.0).radius(4.0).fill(ink.alpha(0.16)).child(text(label.to_owned()).font(k.ty.text_xsmall_medium).color(ink))
}


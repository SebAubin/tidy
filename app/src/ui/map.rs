//! The space map: a treemap of the folder in focus, two levels deep, beside a ranked list.

use rune::{Align, Cx, El, Tx, col, el, icon, row, text};
use rune_kit::{IconButton, IconButtonVariant};
use tidy_core::format::bytes;
use tidy_core::{Kind, NodeId, Tree};

use crate::theme::{Theme, folder_color, kind_color};
use crate::treemap::squarify;
use crate::{App, icons};

/// The map's size, so the layout is computed in the view without a measuring pass.
const PAD: f32 = 1.0;
const HEADER: f32 = 18.0;
/// Below this share of the folder, items merge into one "smaller items" tile.
const MIN_SHARE: f64 = 0.004;

pub fn map_view(s: &App, t: &Theme, cx: &mut Cx<App>) -> El {
    let tree = s.tree.as_ref().unwrap();
    let focus = s.focus;
    // The ranked list only when the pane has room for both it and a useful map.
    let wide = cx.layout_size("map-body").is_none_or(|z| z[0] >= 820.0);
    let mut body = row()
        .key("map-body")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(14.0)
        .pb(10.0)
        .gap(14.0)
        .child(map(s, tree, focus, t, cx));
    if wide {
        body = body.child(side_list(s, tree, focus, t));
    }
    col()
        .key("map-page")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(breadcrumb(s, tree, t))
        .child(body)
        .child(legend(tree, t))
}

fn breadcrumb(s: &App, tree: &Tree, t: &Theme) -> El {
    let k = &t.k;
    let chain = tree.ancestry(s.focus);
    let mut r = row().key("crumbs").w_full().h(40.0).px(14.0).gap(2.0).center_y();
    r = r.child(
        IconButton::new(&icons::BACK)
            .variant(IconButtonVariant::GhostMuted)
            .help("Enclosing Folder")
            .shortcut("⎋")
            .disabled(chain.len() <= 1)
            .on_press(|s: &mut App| s.go_up())
            .build(k)
            .key("up"),
    );
    for (i, &id) in chain.iter().enumerate() {
        let last = i + 1 == chain.len();
        let name = if id == Tree::ROOT { crate::tilde(&tree.root_path.to_string_lossy()) } else { tree.node(id).name.to_string() };
        if i > 0 {
            r = r.child(icon(&icons::CHEVRON, 10.0).color(k.c.icon_faint));
        }
        r = r.child(
            row()
                .key(format!("crumb-{id}"))
                .px(6.0)
                .py(3.0)
                .radius(k.radius.sm)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .on_press(move |s: &mut App| {
                    s.focus = id;
                    s.selected = None;
                })
                .child(text(name).font(if last { k.ty.ui_medium } else { k.ty.ui }).color(if last { k.c.text_base } else { k.c.text_muted })),
        );
    }
    let n = tree.node(s.focus);
    r.child(el().grow(1.0)).child(
        text(format!(
            "{} · {} · scanned {}",
            bytes(n.size),
            crate::plural(n.files as usize, "file"),
            ago(tidy_core::scan::now() - tree.scanned_at)
        ))
            .font(k.ty.ui)
            .color(k.c.text_muted)
            .key("focus-size"),
    )
}

/// Children worth a tile, and what the rest add up to.
fn tiles(tree: &Tree, id: NodeId) -> (Vec<NodeId>, u64) {
    let n = tree.node(id);
    let min = (n.size as f64 * MIN_SHARE) as u64;
    let shown: Vec<NodeId> = n.children.iter().copied().filter(|&c| tree.node(c).size > 0 && tree.node(c).size >= min).take(160).collect();
    let rest = n.size.saturating_sub(shown.iter().map(|&c| tree.node(c).size).sum());
    (shown, rest)
}

fn map(s: &App, tree: &Tree, focus: NodeId, t: &Theme, cx: &mut Cx<App>) -> El {
    let k = &t.k;
    // Squarify into the space the map had last layout; a resize re-runs the view.
    let [w, h] = cx.layout_size("map-canvas").filter(|z| z[0] > 10.0 && z[1] > 10.0).unwrap_or([760.0, 520.0]);
    let (ids, rest) = tiles(tree, focus);
    let mut sizes: Vec<f64> = ids.iter().map(|&c| tree.node(c).size as f64).collect();
    if rest > 0 {
        sizes.push(rest as f64);
    }
    let rects = squarify(&sizes, [0.0, 0.0, w, h]);
    let mut canvas = el().key("map-canvas").grow(1.0).h_full().min_w(0.0).radius(k.radius.md).clip_rounded(k.radius.md).fill(t.hairline);
    for (i, r) in rects.iter().enumerate() {
        let tile = if let Some(&id) = ids.get(i) { tile(s, tree, id, i, *r, t) } else { rest_tile(rest, *r, t) };
        canvas = canvas.child(tile);
    }
    canvas
}

fn placed(e: El, r: [f32; 4]) -> El {
    e.absolute(r[0], r[1]).size(r[2], r[3]).padding(PAD)
}

fn tile(s: &App, tree: &Tree, id: NodeId, i: usize, r: [f32; 4], t: &Theme) -> El {
    let k = &t.k;
    let n = tree.node(id);
    let base = if n.is_dir && n.kind == Kind::Folder { folder_color(i, t.dark) } else { kind_color(n.kind, t.dark) };
    let selected = s.selected == Some(id);
    let path = tree.path(id);
    let is_dir = n.is_dir;
    let help = format!("{}\n{} · {}", crate::tilde(&path.to_string_lossy()), bytes(n.size), crate::plural(n.files as usize, "file"));
    let mut body = col()
        .w_full()
        .h_full()
        .radius(5.0)
        .clip_rounded(5.0)
        .fill(base)
        .border(if selected { 2.0 } else { 0.0 }, k.c.text_base)
        .hover_group()
        .on_press(move |s: &mut App| s.selected = Some(id))
        .on_double_press(move |s: &mut App| s.open_folder(id))
        .on_secondary_press({
            let path = path.clone();
            move |s: &mut App, p, tx: &mut Tx| {
                s.selected = Some(id);
                super::item_menu(path.clone(), is_dir, p, tx);
            }
        })
        .help(help);
    // Room for a label: name and size along the top.
    let big = r[2] > 70.0 && r[3] > 34.0;
    let ink = if t.dark { rune::Color::WHITE.alpha(0.92) } else { rune::Color::BLACK.alpha(0.78) };
    if big {
        body = body.child(
            row()
                .w_full()
                .h(HEADER)
                .px(6.0)
                .gap(6.0)
                .center_y()
                .child(text(n.name.to_string()).font(k.ty.small_medium).color(ink).truncate().grow(1.0).min_w(0.0))
                .child(text(bytes(n.size)).font(k.ty.small).color(ink.alpha(0.7))),
        );
    }
    // A second level inside big folders, drawn lighter, so the eye sees what each is made of.
    if is_dir && r[2] > 140.0 && r[3] > 90.0 {
        let (kids, rest) = tiles(tree, id);
        let mut sizes: Vec<f64> = kids.iter().map(|&c| tree.node(c).size as f64).collect();
        if rest > 0 {
            sizes.push(rest as f64);
        }
        let (iw, ih) = (r[2] - 2.0 * PAD - 6.0, r[3] - 2.0 * PAD - HEADER - 3.0);
        let inner = squarify(&sizes, [0.0, 0.0, iw, ih]);
        let mut area = el().w_full().grow(1.0).px(3.0).pb(3.0);
        let mut boxed = el().w_full().h_full();
        for (j, ir) in inner.iter().enumerate() {
            let fill = match kids.get(j) {
                Some(&c) if !tree.node(c).is_dir || tree.node(c).kind != Kind::Folder => kind_color(tree.node(c).kind, t.dark),
                Some(_) => base.mix(rune::Color::WHITE, if t.dark { 0.12 } else { 0.28 }),
                None => base.mix(rune::Color::WHITE, 0.45),
            };
            let mut cell = el().w_full().h_full().radius(3.0).fill(fill).group_opacity(1.0, 0.92);
            if let Some(&c) = kids.get(j) {
                let cn = tree.node(c);
                if ir[2] > 64.0 && ir[3] > 22.0 {
                    cell = cell.child(text(cn.name.to_string()).font(k.ty.text_xsmall).color(ink.alpha(0.85)).truncate().px(4.0).pt(3.0));
                }
                let cpath = tree.path(c);
                let cdir = cn.is_dir;
                cell = cell
                    .help(format!("{}\n{}", crate::tilde(&cpath.to_string_lossy()), bytes(cn.size)))
                    .on_press(move |s: &mut App| s.selected = Some(c))
                    .on_double_press(move |s: &mut App| s.open_folder(c))
                    .on_secondary_press(move |s: &mut App, p, tx: &mut Tx| {
                        s.selected = Some(c);
                        super::item_menu(cpath.clone(), cdir, p, tx);
                    });
                if s.selected == Some(c) {
                    cell = cell.border(2.0, k.c.text_base);
                }
            }
            boxed = boxed.child(placed(el().child(cell), *ir).padding(0.75));
        }
        area = area.child(boxed);
        body = body.child(area);
    }
    placed(el().key(format!("t{id}")).motion(rune::tok::motion::DEFAULT).child(body), r)
}

fn rest_tile(rest: u64, r: [f32; 4], t: &Theme) -> El {
    let k = &t.k;
    placed(
        el().key("rest").child(
            el().w_full()
                .h_full()
                .radius(5.0)
                .fill(t.hairline)
                .help(format!("Smaller items · {}", bytes(rest)))
                .child(text("Smaller items").font(k.ty.text_xsmall).color(k.c.text_faint).truncate().px(5.0).pt(4.0)),
        ),
        r,
    )
}

fn side_list(s: &App, tree: &Tree, focus: NodeId, t: &Theme) -> El {
    let k = &t.k;
    let n = tree.node(focus);
    let total = n.size.max(1) as f32;
    let mut list = col().key("side-rows").w_full().gap(1.0);
    for (i, &c) in n.children.iter().take(60).enumerate() {
        let cn = tree.node(c);
        if cn.size == 0 {
            break;
        }
        let color = if cn.is_dir && cn.kind == Kind::Folder { folder_color(i, t.dark) } else { kind_color(cn.kind, t.dark) };
        let share = cn.size as f32 / total;
        let selected = s.selected == Some(c);
        let path = tree.path(c);
        let is_dir = cn.is_dir;
        list = list.child(
            col()
                .key(format!("r{c}"))
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(4.0)
                .radius(k.radius.sm)
                .fill(if selected { t.mint.alpha(0.18) } else { rune::Color::TRANSPARENT })
                .hover(k.c.hover)
                .on_press(move |s: &mut App| s.selected = Some(c))
                .on_double_press(move |s: &mut App| s.open_folder(c))
                .on_secondary_press(move |s: &mut App, p, tx: &mut Tx| {
                    s.selected = Some(c);
                    super::item_menu(path.clone(), is_dir, p, tx);
                })
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(el().size(9.0, 9.0).radius(2.5).fill(color))
                        .child(text(cn.name.to_string()).font(k.ty.ui).color(k.c.text_base).truncate().grow(1.0).min_w(0.0))
                        .child(text(bytes(cn.size)).font(k.ty.ui).color(k.c.text_muted).align(Align::End)),
                )
                .child(el().w_full().h(3.0).radius(1.5).fill(t.hairline).child(el().h(3.0).w_frac(share.max(0.005)).radius(1.5).fill(color))),
        );
    }
    col()
        .key("side")
        .w(280.0)
        .h_full()
        .min_h(0.0)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .scroll_edge_fade(12.0)
                .child(list),
        )
}

fn legend(tree: &Tree, t: &Theme) -> El {
    let k = &t.k;
    let mut r = row().key("legend").w_full().px(16.0).pb(12.0).gap(14.0).center_y().flex_wrap();
    for (kind, b, _) in tree.by_kind().into_iter().filter(|(kd, _, _)| *kd != Kind::Folder).take(7) {
        r = r.child(
            row()
                .gap(5.0)
                .center_y()
                .child(el().size(8.0, 8.0).radius(2.0).fill(kind_color(kind, t.dark)))
                .child(text(kind.label()).font(k.ty.small).color(k.c.text_muted))
                .child(text(bytes(b)).font(k.ty.small).color(k.c.text_faint)),
        );
    }
    r
}

/// "just now", "4 min ago", "2 h ago", then days.
fn ago(secs: i64) -> String {
    match secs {
        i64::MIN..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => tidy_core::format::age(secs),
    }
}

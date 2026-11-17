//! The first screen, and the scan in progress.

use std::sync::atomic::Ordering;

use rune::{Align, El, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize};
use tidy_core::format::{bytes, count};

use crate::theme::Theme;
use crate::{App, ScanJob, icons, tilde};

/// Tidy's mark, as on the app icon: a disk map whose big tile is mint, and one tile left as a
/// dashed outline with a spark, the space Tidy freed.
pub fn mark(size: f32, t: &Theme) -> El {
    let u = size / 10.0;
    let g = u * 0.5;
    let r = u * 1.1;
    let frost = if t.dark { rune::Color::WHITE.alpha(0.22) } else { t.mint.mix(rune::Color::WHITE, 0.55).alpha(0.7) };
    let left = 5.7 * u - g / 2.0;
    let rx = 5.7 * u + g / 2.0;
    let rw = size - rx;
    let top_h = 5.0 * u - g / 2.0;
    el().size(size, size)
        .child(el().absolute(0.0, 0.0).size(left, size).radius(r).continuous_corners().gradient(t.mint.mix(rune::Color::WHITE, 0.25), t.mint))
        .child(el().absolute(rx, 0.0).size(rw, top_h).radius(r).continuous_corners().fill(frost))
        .child(
            el().absolute(rx, top_h + g)
                .size(rw, size - top_h - g)
                .radius(r)
                .border((u * 0.28).max(1.0), t.mint)
                .border_dash(u * 0.9, u * 0.6)
                .center()
                .child(icon(&icons::SPARKLES, rw * 0.62).color(t.mint)),
        )
}

pub fn welcome(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let assistant = s.chat.detected.as_ref().and_then(|d| d.providers.first().map(|p| p.kind.name()));
    col()
        .key("welcome")
        .w_full()
        .h_full()
        .center()
        .gap(18.0)
        .child(mark(64.0, t))
        .child(
            col()
                .items_center()
                .gap(6.0)
                .child(text(format!("See what fills your {}", crate::os::COMPUTER)).font(rune::TextStyle { size: 24.0, weight: 700.0, ..k.ty.body_medium }).color(k.c.text_base))
                .child(
                    text(format!("Tidy maps every folder by the space it takes, finds caches and leftovers that are safe to remove, and moves what you choose to the {}.", crate::os::TRASH))
                        .font(k.ty.body)
                        .color(k.c.text_muted)
                        .align(Align::Center)
                        .wrap()
                        .max_w(440.0),
                ),
        )
        .child(
            row()
                .gap(10.0)
                .child(
                    Button::new("Scan Home Folder")
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::Large)
                        .on_press(|s: &mut App| {
                            let p = s.places[0].path.clone();
                            s.go_to(p);
                        })
                        .build(k)
                        .key("scan-home"),
                )
                .child(
                    Button::new("Choose Folder…")
                        .variant(ButtonVariant::Neutral)
                        .control_size(ControlSize::Large)
                        .on_press_tx(|_: &mut App, tx| super::pick_folder(tx))
                        .build(k)
                        .key("scan-pick"),
                ),
        )
        .child(
            row()
                .gap(6.0)
                .center_y()
                .pt(8.0)
                .child(icon(&icons::SPARKLES, 13.0).color(t.mint))
                .child(
                    text(match assistant {
                        Some(name) => format!("{name} found on this {}: ask it to help you clean up.", crate::os::COMPUTER),
                        None => format!("Looking for an AI assistant on this {}…", crate::os::COMPUTER),
                    })
                    .font(k.ty.small)
                    .color(k.c.text_muted),
                ),
        )
}

/// The scan in progress: a calm count, and the map filling in as each top-level folder is
/// measured. Tiles keep their colour and slide into place on springs as sizes change.
pub fn scanning(s: &App, job: &ScanJob, t: &Theme, cx: &mut rune::Cx<App>) -> El {
    let k = &t.k;
    let p = &job.progress;
    let files = p.files.load(Ordering::Relaxed);
    let b = p.bytes.load(Ordering::Relaxed);
    let secs = job.started.elapsed().as_secs();
    let place = s.places.iter().find(|pl| pl.path == job.path).map(|pl| pl.name.clone()).unwrap_or_else(|| tilde(&job.path.to_string_lossy()));
    let mut top: Vec<tidy_core::scan::TopEntry> = p.top.lock().map(|v| v.clone()).unwrap_or_default();
    top.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));

    let [w, h] = cx.layout_size("scan-map").filter(|z| z[0] > 10.0).unwrap_or([560.0, 260.0]);
    let total: u64 = top.iter().map(|e| e.size).sum();
    let shown: Vec<&tidy_core::scan::TopEntry> = top.iter().filter(|e| e.size > 0 && e.size as f64 >= total as f64 * 0.004).take(48).collect();
    let rects = crate::treemap::squarify(&shown.iter().map(|e| e.size as f64).collect::<Vec<_>>(), [0.0, 0.0, w, h]);
    let ink = if t.dark { rune::Color::WHITE.alpha(0.92) } else { rune::Color::BLACK.alpha(0.75) };
    let mut map = el().key("scan-map").w_full().max_w(620.0).h(260.0).radius(k.radius.lg).clip_rounded(k.radius.lg).fill(t.hairline);
    for (e, r) in shown.iter().zip(&rects) {
        let color = crate::theme::folder_color(stable_index(&e.name), t.dark);
        let mut tile = el()
            .w_full()
            .h_full()
            .radius(5.0)
            .fill(if e.done { color } else { color.alpha(0.45) })
            .motion(rune::tok::motion::CALM);
        if r[2] > 70.0 && r[3] > 30.0 {
            tile = tile.child(
                col()
                    .px(7.0)
                    .pt(5.0)
                    .gap(1.0)
                    .child(text(e.name.to_string()).font(k.ty.small_medium).color(ink).truncate())
                    .child(text(bytes(e.size)).font(k.ty.text_xsmall).color(ink.alpha(0.7))),
            );
        }
        map = map.child(
            el().key(format!("scan-{}", e.name))
                .absolute(r[0], r[1])
                .size(r[2], r[3])
                .padding(1.0)
                .motion(rune::tok::motion::CALM)
                .transition(rune::Transition::opacity())
                .child(tile),
        );
    }

    col()
        .key("scanning")
        .w_full()
        .h_full()
        .center()
        .px(32.0)
        .gap(6.0)
        .child(text(format!("Scanning {place}")).font(k.ty.ui_medium).color(k.c.text_muted))
        .child(
            text(bytes(b))
                .key("scan-bytes")
                .font(rune::TextStyle { size: 40.0, weight: 700.0, ..k.ty.body_medium })
                .color(k.c.text_base)
                .py(4.0),
        )
        .child(
            rune_kit::RollingText::new("scan-files", format!("{} files · {}", count(files), elapsed(secs)))
                .value(files as f64)
                .font(k.ty.ui)
                .color(k.c.text_faint)
                .build(k),
        )
        .child(el().h(18.0))
        .child(map)
        .child(el().h(18.0))
        .child(
            Button::new("Stop")
                .variant(ButtonVariant::Ghost)
                .on_press(|s: &mut App| s.cancel_scan())
                .build(k)
                .key("scan-stop"),
        )
}

fn elapsed(secs: u64) -> String {
    if secs < 60 { format!("{secs} s") } else { format!("{}:{:02}", secs / 60, secs % 60) }
}

/// The same colour for a folder every time, whatever order it lands in.
fn stable_index(name: &str) -> usize {
    name.bytes().fold(5381usize, |h, c| h.wrapping_mul(33) ^ c as usize)
}

/// Asks once for Full Disk Access instead of letting macOS ask about each protected folder.
pub fn access_needed(path: &std::path::Path, t: &Theme) -> El {
    let k = &t.k;
    let guarded = tidy_core::access::guarded_under(path);
    let names: Vec<&str> = guarded.iter().map(|g| g.label).collect();
    let step = |n: &str, line: &str| {
        row()
            .gap(10.0)
            .center_y()
            .child(row().size(22.0, 22.0).radius(11.0).center().fill(t.mint.alpha(0.18)).child(text(n.to_owned()).font(k.ty.small_medium).color(t.mint)))
            .child(text(line.to_owned()).font(k.ty.ui).color(k.c.text_base))
    };
    col()
        .key("access")
        .w_full()
        .h_full()
        .center()
        .gap(18.0)
        .child(icon(&icons::SHIELD, 44.0).color(t.mint))
        .child(
            col()
                .items_center()
                .gap(6.0)
                .max_w(470.0)
                .child(text("Let Tidy see the whole disk").font(rune::TextStyle { size: 22.0, weight: 700.0, ..k.ty.body_medium }).color(k.c.text_base))
                .child(
                    text(format!(
                        "macOS guards {} with a separate permission each. Turn on Full Disk Access once and Tidy can measure them all, without a pile of dialogs.",
                        list(&names)
                    ))
                    .font(k.ty.body)
                    .color(k.c.text_muted)
                    .align(Align::Center)
                    .wrap(),
                ),
        )
        .child(
            col()
                .gap(10.0)
                .px(18.0)
                .py(14.0)
                .radius(k.radius.lg)
                .fill(k.c.layer_01)
                .border(1.0, t.hairline)
                .child(step("1", "Open Privacy & Security › Full Disk Access"))
                .child(step("2", "Turn on Tidy (use + if it isn't listed)"))
                .child(step("3", "Come back: Tidy starts on its own")),
        )
        .child(
            row()
                .gap(10.0)
                .child(
                    Button::new("Open System Settings")
                        .variant(ButtonVariant::Contrast)
                        .control_size(ControlSize::Large)
                        .on_press(|_: &mut App| tidy_core::access::open_full_disk_access_settings())
                        .build(k)
                        .key("open-settings"),
                )
                .child(
                    Button::new("Scan Without Them")
                        .variant(ButtonVariant::Neutral)
                        .control_size(ControlSize::Large)
                        .on_press(|s: &mut App| {
                            s.without_access = true;
                            if let Some(p) = s.pending.take() {
                                s.start_scan(p);
                            }
                        })
                        .build(k)
                        .key("scan-without"),
                ),
        )
        .child(
            text("Already on? Some Macs need Tidy to quit and reopen before it takes effect.")
                .font(k.ty.small)
                .color(k.c.text_faint),
        )
        .child(
            Button::new("Quit & Reopen")
                .variant(ButtonVariant::Ghost)
                .on_press_tx(|_: &mut App, tx| {
                    relaunch();
                    tx.quit();
                })
                .build(k)
                .key("relaunch"),
        )
}

/// "A, B and 3 other places".
fn list(names: &[&str]) -> String {
    match names {
        [] => "some folders".into(),
        [a] => (*a).into(),
        [a, b] => format!("{a} and {b}"),
        [a, b, c] => format!("{a}, {b} and {c}"),
        [a, b, c, rest @ ..] => format!("{a}, {b}, {c} and {} other places", rest.len()),
    }
}

/// Opens this app again once this process has really exited, so two copies never run side by
/// side (which shows the app twice in the Dock).
fn relaunch() {
    let Ok(exe) = std::env::current_exe() else { return };
    let target = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(|p| p.to_path_buf()).unwrap_or(exe);
    let pid = std::process::id();
    let script = format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open \"{}\"", target.display());
    let _ = std::process::Command::new("/bin/sh").arg("-c").arg(script).spawn();
}

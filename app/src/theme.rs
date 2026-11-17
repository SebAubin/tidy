//! Tidy's look: macOS's own neutrals and type (SF Pro through the system font), with Tidy's
//! mint as the accent, and the palette the map paints with.

use rune::{Color, oklch};

/// `#rrggbb` to a colour.
fn hex(s: &str) -> Option<Color> {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).ok()?;
    let c = |sh: u32| ((v >> sh) & 0xff) as f32 / 255.0;
    let dec = |x: f32| if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    Some(Color { r: dec(c(16)), g: dec(c(8)), b: dec(c(0)), a: 1.0 })
}
use rune_kit::{Kit, PaletteSeed};
use tidy_core::Kind;

/// Accent choices: name and OKLCH hue/chroma. Mint, first, is Tidy's own: "a clean slate".
pub const ACCENTS: [(&str, (f32, f32)); 6] =
    [("Mint", (168.0, 0.13)), ("Blue", (250.0, 0.15)), ("Purple", (300.0, 0.15)), ("Pink", (350.0, 0.15)), ("Orange", (55.0, 0.15)), ("Graphite", (260.0, 0.0))];

pub fn accent(i: usize, dark: bool) -> Color {
    let (h, c) = ACCENTS[i.min(ACCENTS.len() - 1)].1;
    if dark { oklch(0.78, c, h) } else { oklch(0.62, c, h) }
}

pub struct Theme {
    pub k: Kit,
    pub dark: bool,
    /// The accent (Tidy's mint unless changed in Settings).
    /// The window behind the sidebar.
    pub sidebar: Color,
    /// The content card.
    pub content: Color,
    pub hairline: Color,
    pub mint: Color,
}

pub fn theme(dark: bool, accent_index: usize) -> Theme {
    let (bg, fg) = if dark { (hex("#1e1e20").unwrap(), hex("#f2f2f4").unwrap()) } else { (hex("#ffffff").unwrap(), hex("#1d1d1f").unwrap()) };
    let mut k = Kit::from_palette(PaletteSeed { bg, fg, accent: accent(accent_index, dark) });
    k.radius.sm = 5.0;
    k.radius.md = 7.0;
    k.radius.lg = 10.0;
    let sidebar = if dark { hex("#2a2a2d").unwrap() } else { hex("#ececee").unwrap() };
    k.c.page = sidebar;
    // The window behind the cards: the sidebar's tone, so the main pane and the assistant read
    // as two cards on it.
    k.c.bg_deep = sidebar;
    Theme { hairline: fg.alpha(if dark { 0.12 } else { 0.10 }), content: bg, sidebar, mint: accent(accent_index, dark), dark, k }
}

/// A file kind's colour on the map.
pub fn kind_color(kind: Kind, dark: bool) -> Color {
    let l = if dark { 0.66 } else { 0.74 };
    let (c, h) = match kind {
        Kind::Video => (0.14, 300.0),
        Kind::Image => (0.14, 350.0),
        Kind::Audio => (0.14, 25.0),
        Kind::Document => (0.12, 250.0),
        Kind::Archive => (0.13, 75.0),
        Kind::DiskImage => (0.14, 45.0),
        Kind::Code => (0.12, 150.0),
        Kind::App => (0.12, 195.0),
        Kind::Library => (0.03, 260.0),
        Kind::Other => (0.015, 260.0),
        Kind::Folder => (0.10, 230.0),
    };
    oklch(l, c, h)
}

/// Hues for folders, by position among their siblings, so neighbours stay apart.
pub fn folder_color(i: usize, dark: bool) -> Color {
    const H: [f32; 8] = [238.0, 168.0, 290.0, 60.0, 205.0, 330.0, 130.0, 20.0];
    let l = if dark { 0.60 } else { 0.76 };
    oklch(l, 0.09, H[i % H.len()])
}

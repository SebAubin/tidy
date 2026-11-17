//! Preferences: what they hold, where they're saved, and the Settings window (⌘,).

use std::sync::OnceLock;

use rune::{AppHandle, Cx, El, Tx, col, el, icon, row, text};
use rune_kit::SegmentedControl;
use serde::{Deserialize, Serialize};

use crate::theme::{ACCENTS, Theme, theme};
use crate::{App, icons};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// What the menu bar item shows beside its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MenuBarText {
    #[default]
    FreeSpace,
    PercentFree,
    PercentUsed,
    IconOnly,
}

impl MenuBarText {
    pub const ALL: [MenuBarText; 4] = [MenuBarText::FreeSpace, MenuBarText::PercentFree, MenuBarText::PercentUsed, MenuBarText::IconOnly];

    /// The text for `free` bytes of `total`.
    pub fn text(self, free: u64, total: u64) -> String {
        if total == 0 {
            return String::new();
        }
        let pct = |x: u64| (x as f64 / total as f64 * 100.0).round() as u64;
        match self {
            MenuBarText::FreeSpace => tidy_core::format::bytes(free),
            MenuBarText::PercentFree => format!("{}% free", pct(free)),
            MenuBarText::PercentUsed => format!("{}% used", pct(total - free)),
            MenuBarText::IconOnly => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    /// Index into `theme::ACCENTS`.
    pub accent: usize,
    /// Large Files lists files at least this big.
    pub large_min_mb: u64,
    /// A project untouched this long counts as stale in the Developer view.
    pub stale_days: u32,
    /// Tell the assistant which folder is on screen with each message.
    pub share_context: bool,
    /// Free space in the menu bar.
    pub menu_bar: bool,
    pub menu_bar_text: MenuBarText,
    /// A notification when the disk runs low.
    pub low_space_alert: bool,
    /// The assistant to use (a `ProviderKind::id`): the one chosen last, or set here.
    pub assistant: Option<String>,
    /// A model per assistant, by id; absent or empty uses the CLI's own default.
    pub models: std::collections::BTreeMap<String, String>,
    /// Assistants the person turned off, by id: never listed or started.
    pub disabled: Vec<String>,
    /// Anonymous usage statistics and crash reports (see the README, "Telemetry").
    pub telemetry: bool,
    /// The first-launch note about telemetry was dismissed.
    pub telemetry_notice_seen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { appearance: Appearance::System, accent: 0, large_min_mb: 10, stale_days: 30, share_context: true, menu_bar: true, menu_bar_text: MenuBarText::FreeSpace, low_space_alert: true, assistant: None, models: Default::default(), disabled: Vec::new(), telemetry: true, telemetry_notice_seen: false }
    }
}

impl Settings {
    pub fn enabled(&self, kind: crate::assistant::ProviderKind) -> bool {
        !self.disabled.iter().any(|d| d == kind.id())
    }

    pub fn model(&self, kind: crate::assistant::ProviderKind) -> Option<String> {
        self.models.get(kind.id()).filter(|m| !m.trim().is_empty()).cloned()
    }

    pub fn dark(&self, system_dark: bool) -> bool {
        match self.appearance {
            Appearance::System => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }

    fn path() -> std::path::PathBuf {
        crate::assistant::support_dir().join("settings.json")
    }

    /// Read once at launch, before the first frame.
    pub fn load() -> Settings {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// Written off the UI thread.
    pub fn save(&self) {
        let json = serde_json::to_vec_pretty(self).unwrap_or_default();
        std::thread::spawn(move || {
            let p = Settings::path();
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, json);
        });
    }
}

/// The main window's handle, so the Settings window can apply what changes there.
pub static MAIN: OnceLock<AppHandle<App>> = OnceLock::new();
/// The Settings window's handle, so a menu item or ⌘, can open it.
pub static WINDOW: OnceLock<AppHandle<SettingsUi>> = OnceLock::new();

pub fn open() {
    if let Some(w) = WINDOW.get() {
        w.open();
    }
}

/// The Settings window's own state: a copy of the settings, sent to the main window on change.
pub struct SettingsUi {
    pub s: Settings,
    pub full_disk: bool,
    /// The model fields, one per assistant (`ProviderKind::ALL` order).
    pub models: Vec<rune::TextEdit>,
    /// How many installed assistants the view last showed.
    seen: usize,
}

/// Redraws once the main window has found the installed assistants.
pub fn poll(ui: &mut SettingsUi, _: f64, tx: &mut rune::Tx) -> bool {
    let n = INSTALLED.lock().map(|v| v.len()).unwrap_or(0);
    if n != ui.seen {
        ui.seen = n;
        return true;
    }
    if n == 0 {
        tx.wake_after(std::time::Duration::from_secs(1));
    }
    false
}

/// The assistants found on this Mac, filled by the main window once it has looked.
pub static INSTALLED: std::sync::Mutex<Vec<crate::assistant::ProviderKind>> = std::sync::Mutex::new(Vec::new());

pub fn init() -> SettingsUi {
    let s = Settings::load();
    let models = crate::assistant::ProviderKind::ALL.iter().map(|k| rune::TextEdit::new(s.models.get(k.id()).cloned().unwrap_or_default())).collect();
    SettingsUi { s, full_disk: tidy_core::access::has_full_disk_access(), models, seen: 0 }
}

fn change(ui: &mut SettingsUi, f: impl FnOnce(&mut Settings)) {
    f(&mut ui.s);
    ui.s.save();
    let next = ui.s.clone();
    if let Some(m) = MAIN.get() {
        m.update(move |app: &mut App, tx: &mut Tx| app.apply_settings(next, tx));
    }
}

pub fn view(ui: &SettingsUi, cx: &mut Cx<SettingsUi>) -> El {
    let t = theme(ui.s.dark(cx.system_dark()), ui.s.accent);
    let k = &t.k;
    let st = &ui.s;
    let appearance = SegmentedControl::new(&["System", "Light", "Dark"], Some(st.appearance as usize))
        .label("Appearance")
        .fit()
        .on_pick(|ui: &mut SettingsUi, i, tx: &mut Tx| {
            tx.color_motion(rune::tok::motion::CALM);
            change(ui, |s| s.appearance = [Appearance::System, Appearance::Light, Appearance::Dark][i]);
        })
        .build(k);
    let mut swatches = row().gap(10.0).role("radiogroup").label("Accent colour");
    for (i, (name, _)) in ACCENTS.iter().enumerate() {
        let c = crate::theme::accent(i, t.dark);
        let on = st.accent == i;
        swatches = swatches.child(
            el().key(format!("acc{i}"))
                .size(20.0, 20.0)
                .radius(10.0)
                .fill(c)
                .border(if on { 2.0 } else { 0.0 }, k.c.text_base)
                .role("radio")
                .label(*name)
                .checked(on)
                .help(*name)
                .focusable(true)
                .on_press(move |ui: &mut SettingsUi| change(ui, |s| s.accent = i)),
        );
    }
    const SIZES: [u64; 4] = [10, 100, 500, 1000];
    let large = SegmentedControl::new(&["10 MB", "100 MB", "500 MB", "1 GB"], SIZES.iter().position(|&m| m == st.large_min_mb))
        .label("Large files over")
        .fit()
        .on_pick(|ui: &mut SettingsUi, i, _| change(ui, |s| s.large_min_mb = SIZES[i]))
        .build(k);
    const DAYS: [u32; 4] = [14, 30, 90, 180];
    let stale = SegmentedControl::new(&["2 weeks", "1 month", "3 months", "6 months"], DAYS.iter().position(|&d| d == st.stale_days))
        .label("Stale after")
        .fit()
        .on_pick(|ui: &mut SettingsUi, i, _| change(ui, |s| s.stale_days = DAYS[i]))
        .build(k);
    let access = if ui.full_disk {
        row().gap(6.0).center_y().child(icon(&icons::SHIELD, 14.0).color(t.mint)).child(text("Granted").font(k.ty.ui).color(k.c.text_muted))
    } else {
        rune_kit::Button::new("Open Privacy Settings")
            .variant(rune_kit::ButtonVariant::Neutral)
            .on_press(|_: &mut SettingsUi| tidy_core::access::open_full_disk_access_settings())
            .build(k)
    };
    let menu_bar = Switch::new(st.menu_bar)
        .label("Show Tidy in the menu bar")
        .hide_label()
        .on_toggle(|ui: &mut SettingsUi| change(ui, |s| s.menu_bar = !s.menu_bar))
        .build(k);
    // The choices show this Mac's own numbers.
    let space = tidy_core::volume::space(&tidy_core::system_root()).unwrap_or_default();
    let labels: Vec<String> = MenuBarText::ALL
        .iter()
        .map(|m| match m {
            MenuBarText::IconOnly => "Icon only".to_owned(),
            _ => m.text(space.free, space.total.max(1)),
        })
        .collect();
    let bar_text = SegmentedControl::new(&labels, MenuBarText::ALL.iter().position(|m| *m == st.menu_bar_text))
        .label("Menu bar shows")
        .fit()
        .on_pick(|ui: &mut SettingsUi, i, _| change(ui, |s| s.menu_bar_text = MenuBarText::ALL[i]))
        .build(k)
        .opacity(if st.menu_bar { 1.0 } else { 0.4 });
    let low = Switch::new(st.low_space_alert)
        .label("Warn when space runs low")
        .hide_label()
        .on_toggle(|ui: &mut SettingsUi| change(ui, |s| s.low_space_alert = !s.low_space_alert))
        .build(k);
    let share = Switch::new(st.share_context)
        .label("Share the folder on screen")
        .hide_label()
        .on_toggle(|ui: &mut SettingsUi| change(ui, |s| s.share_context = !s.share_context))
        .build(k);

    let inner = col()
        .key("settings")
        .w_full()
        .px(20.0)
        .pt(16.0)
        .pb(20.0)
        .gap(18.0)
        .child(group(&t, "Appearance", vec![line(&t, "Theme", None, appearance), line(&t, "Accent colour", None, swatches)]))
        .children(cfg!(target_os = "macos").then(|| {
            group(
                &t,
                "Menu Bar",
                vec![
                    line(&t, "Show Tidy in the menu bar", None, menu_bar),
                    line(&t, "Beside the icon", None, bar_text),
                    line(&t, "Warn when space runs low", Some("A notification under 10% free, with what Tidy found to reclaim."), low),
                ],
            )
        }))
        .child(group(&t, "Scanning", {
            let mut rows = vec![line(&t, "Large Files lists files over", None, large)];
            // Full Disk Access is macOS's; Windows has no per-folder privacy prompts to avoid.
            if cfg!(target_os = "macos") {
                rows.push(line(&t, "Full Disk Access", Some("Lets Tidy measure protected folders without asking for each one."), access));
            }
            rows
        }))
        .child(group(&t, "Assistant", assistant_rows(ui, &t, share)))
        .child(group(&t, "Privacy", vec![privacy_row(ui, &t)]))
        .child(group(
            &t,
            "Developer",
            vec![line(&t, "Projects are stale after", Some("Untouched build folders and worktrees older than this are flagged."), stale)],
        ))
        .child(group(&t, "About", vec![about_row(&t)]));
    col()
        .w_full()
        .h_full()
        .fill(t.sidebar)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(rune::ScrollSpec { stick_bottom: false }).scroll_edge_fade(12.0).child(inner))
}

fn about_row(t: &Theme) -> El {
    let k = &t.k;
    let link = rune_kit::Button::new("GitHub")
        .variant(rune_kit::ButtonVariant::Neutral)
        .on_press(|_: &mut SettingsUi| crate::os::open_url("https://github.com/SebAubin/tidy"))
        .build(k);
    line(t, &format!("Tidy {}", env!("CARGO_PKG_VERSION")), Some("Open source under the MIT License. Issues and ideas welcome on GitHub."), link)
}

fn privacy_row(ui: &SettingsUi, t: &Theme) -> El {
    let k = &t.k;
    let on = ui.s.telemetry;
    let toggle = Switch::new(on)
        .label("Share anonymous usage statistics")
        .hide_label()
        .on_toggle(|ui: &mut SettingsUi| {
            change(ui, |s| s.telemetry = !s.telemetry);
            crate::telemetry::set_enabled(ui.s.telemetry);
        })
        .build(k);
    let hint = if crate::telemetry::available() {
        "Which views are used, scan and cleanup sizes in broad ranges, and crash reports. Never paths, file names or what you ask the assistant. The full list is in Tidy's README."
    } else {
        "This build was made without a telemetry key: it sends nothing either way."
    };
    line(t, "Share anonymous usage statistics", Some(hint), toggle)
}

/// The default assistant, then each one: installed or not, its model, on or off.
fn assistant_rows(ui: &SettingsUi, t: &Theme, share: El) -> Vec<El> {
    use crate::assistant::ProviderKind;
    let k = &t.k;
    let installed = INSTALLED.lock().map(|v| v.clone()).unwrap_or_default();
    let usable: Vec<ProviderKind> = ProviderKind::ALL.into_iter().filter(|p| installed.contains(p) && ui.s.enabled(*p)).collect();
    let mut rows = Vec::new();
    if !usable.is_empty() {
        let names: Vec<&str> = usable.iter().map(|p| p.name()).collect();
        let current = ui.s.assistant.as_deref().and_then(|id| usable.iter().position(|p| p.id() == id)).or(Some(0));
        let picker = SegmentedControl::new(&names, current)
            .label("Default assistant")
            .fit()
            .on_pick(move |ui: &mut SettingsUi, i, _| {
                let id = usable[i].id().to_owned();
                change(ui, |s| s.assistant = Some(id));
            })
            .build(k);
        rows.push(line(t, "Use", Some("Tidy also remembers the one you pick in the assistant panel."), picker));
    }
    for (i, kind) in ProviderKind::ALL.into_iter().enumerate() {
        let here = installed.contains(&kind);
        let on = ui.s.enabled(kind);
        let model = rune_kit::TextInput::new(&ui.models[i])
            .placeholder("Default model")
            .label(format!("{} model", kind.name()))
            .on_edit(move |ui: &mut SettingsUi| &mut ui.models[i])
            .on_change(move |ui: &mut SettingsUi, _| {
                let text = ui.models[i].text.trim().to_owned();
                change(ui, |s| {
                    if text.is_empty() {
                        s.models.remove(kind.id());
                    } else {
                        s.models.insert(kind.id().to_owned(), text);
                    }
                });
            })
            .build(k)
            .w(150.0);
        let toggle = Switch::new(on && here)
            .label(format!("Use {}", kind.name()))
            .hide_label()
            .disabled(!here)
            .on_toggle(move |ui: &mut SettingsUi| {
                change(ui, |s| {
                    if s.enabled(kind) {
                        s.disabled.push(kind.id().to_owned());
                    } else {
                        s.disabled.retain(|d| d != kind.id());
                    }
                })
            })
            .build(k);
        let hint = if here { format!("Model: blank for its default ({})", kind.model_hint()) } else { format!("Not installed on this {}", crate::os::COMPUTER) };
        let controls = row().gap(10.0).center_y().opacity(if here { 1.0 } else { 0.45 }).child(if here { model } else { el() }).child(toggle);
        rows.push(line(t, kind.name(), Some(&hint), controls));
    }
    rows.push(line(t, "Tell the assistant what's on screen", Some("Adds the folder you're looking at to each message."), share));
    rows
}

fn group(t: &Theme, title: &str, rows: Vec<El>) -> El {
    let k = &t.k;
    let n = rows.len();
    let mut card = col().w_full().radius(k.radius.lg).fill(t.content).border(1.0, t.hairline);
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(r);
        if i + 1 < n {
            card = card.child(row().w_full().pl(14.0).child(el().h(1.0).grow(1.0).fill(t.hairline)));
        }
    }
    col().w_full().gap(6.0).child(text(title.to_owned()).font(k.ty.small_medium).color(k.c.text_muted).pl(4.0)).child(card)
}

fn line(t: &Theme, label: &str, hint: Option<&str>, control: El) -> El {
    let k = &t.k;
    let mut left = col().grow(1.0).min_w(0.0).gap(2.0).child(text(label.to_owned()).font(k.ty.ui).color(k.c.text_base));
    if let Some(h) = hint {
        left = left.child(text(h.to_owned()).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    row().w_full().min_h(44.0).px(14.0).py(8.0).gap(16.0).center_y().child(left).child(control)
}

/// An on/off switch drawn the platform's way: `NSSwitch` on macOS, the Windows 11 toggle
/// elsewhere. Same calls as `rune_kit::Switch`, so the rows don't care which.
struct Switch {
    on: bool,
    label: String,
    disabled: bool,
    press: Option<Box<dyn Fn(&mut SettingsUi)>>,
}

impl Switch {
    fn new(on: bool) -> Switch {
        Switch { on, label: String::new(), disabled: false, press: None }
    }
    fn label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    /// The label is only for accessibility; the row shows its own.
    fn hide_label(self) -> Self {
        self
    }
    fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    fn on_toggle(mut self, f: impl Fn(&mut SettingsUi) + 'static) -> Self {
        self.press = Some(Box::new(f));
        self
    }

    #[cfg(target_os = "macos")]
    fn build(self, k: &rune_kit::Kit) -> El {
        let mut s = rune_kit::Switch::new(self.on).label(self.label).hide_label().disabled(self.disabled);
        if let Some(f) = self.press {
            s = s.on_toggle(move |ui: &mut SettingsUi| f(ui));
        }
        s.build(k)
    }

    /// WinUI's ToggleSwitch: a 40 × 20 pill. Off, an outline with a 12 px knob in the text
    /// colour; on, filled with the accent and a knob in the colour of text on the accent.
    #[cfg(not(target_os = "macos"))]
    fn build(self, k: &rune_kit::Kit) -> El {
        let c = &k.c;
        let (w, h, knob) = (40.0, 20.0, 12.0);
        let inset = (h - knob) / 2.0;
        let mut track = el().key("control").size(w, h).radius(h / 2.0).motion(k.motion.control);
        let thumb_color;
        if self.on {
            track = track.fill(c.bg_accent).hover(over_accent(c.bg_accent));
            // Windows draws the knob in the text colour on the accent: white on a dark accent.
            thumb_color = if c.bg_accent.luminance() > 0.5 { rune::Color::BLACK } else { rune::Color::WHITE };
        } else {
            track = track.fill(c.text_muted.alpha(0.0)).border(1.0, c.text_muted).hover(c.hover);
            thumb_color = c.text_muted;
        }
        let x = if self.on { w - inset - knob } else { inset };
        // Absolute children sit inside the border, so the outline's pixel is taken back.
        let (x, y) = if self.on { (x, inset) } else { (x - 1.0, inset - 1.0) };
        track = track.child(el().key("thumb").size(knob, knob).radius(knob / 2.0).fill(thumb_color).absolute(x, y).motion(k.motion.control));
        let mut root = row()
            .role("switch")
            .checked(self.on)
            .label(self.label)
            .center_y()
            .child(track)
            .focus_ring(2.0, 1.0, c.border_focus)
            .focusable(!self.disabled)
            .disabled(self.disabled);
        if self.disabled {
            root = root.opacity(0.4);
        } else if let Some(f) = self.press {
            root = root.on_press(move |ui: &mut SettingsUi| f(ui));
        }
        root
    }
}

/// The accent a shade lighter, as WinUI's hovered accent.
#[cfg(not(target_os = "macos"))]
fn over_accent(c: rune::Color) -> rune::Color {
    c.mix(rune::Color::WHITE, 0.1)
}

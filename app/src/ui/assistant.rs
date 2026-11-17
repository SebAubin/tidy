//! The assistant panel: a conversation with the AI installed on this Mac, and its proposals as
//! checklists the person confirms.

use rune::{Align, El, Tx, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, ControlSize, IconButton, IconButtonVariant, Markdown, TextArea};
use tidy_core::format::bytes;

use crate::theme::Theme;
use crate::{App, Msg, icons, plural, tilde};

const STARTERS: [&str; 3] = ["Where did my space go?", "Find large files I haven't opened in a year", "What can I safely clean up?"];

pub fn panel(s: &App, t: &Theme) -> El {
    let k = &t.k;
    // A card like the main pane's, beside it on the window background: the outer column's
    // padding keeps the window's edge around it.
    let card = col()
        .key("assistant")
        .w_full()
        .h_full()
        .min_h(0.0)
        .radius(k.radius.lg)
        .border(1.0, k.c.border_muted)
        .clip_rounded(k.radius.lg)
        .fill(k.c.bg_base)
        .child(header(s, t))
        .child(
            col()
                .key("chat-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .scroll(rune::ScrollSpec { stick_bottom: true })
                .scroll_edge_fade(14.0)
                .child(conversation(s, t)),
        )
        .child(composer(s, t));
    col().key("assistant-frame").w_full().h_full().min_h(0.0).pl(8.0).pr(8.0).pb(8.0).child(card)
}

fn header(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let d = s.chat.detected.as_ref();
    let name = d.and_then(|d| d.providers.get(s.chat.provider)).map(|p| p.kind.name()).unwrap_or("No assistant");
    let model = if s.chat.model.is_empty() { String::new() } else { pretty_model(&s.chat.model) };
    let current = s.chat.provider.to_string();
    // Only the assistants the person hasn't turned off in Settings.
    let others: Vec<(usize, &'static str, bool)> =
        d.map(|d| d.providers.iter().enumerate().filter(|(_, p)| s.settings.enabled(p.kind)).map(|(i, p)| (i, p.kind.name(), true)).collect()).unwrap_or_default();
    let picker = row()
        .key("provider")
        .gap(6.0)
        .px(6.0)
        .py(3.0)
        .center_y()
        .radius(k.radius.sm)
        .hover(k.c.hover)
        .child(icon(&icons::SPARKLES, 14.0).color(t.mint))
        .child(
            col()
                .child(
                    row()
                        .gap(5.0)
                        .center_y()
                        .child(text(name).font(k.ty.ui_medium).color(k.c.text_base))
                        // There's a choice to make here: say so.
                        .child(icon(&icons::CHEVRON, 9.0).color(k.c.icon_muted).rotate(std::f32::consts::FRAC_PI_2)),
                )
                .child(text(if model.is_empty() { if d.is_some_and(|d| d.providers.is_empty()) { "None installed".to_owned() } else { format!("Installed on this {}", crate::os::COMPUTER) } } else { model }).font(k.ty.text_xsmall).color(k.c.text_faint)),
        )
        .help("Choose the assistant")
        .on_press_tx(move |_: &mut App, tx: &mut Tx| {
            use rune::{PopupItem, PopupMenu};
            let mut menu = PopupMenu::new();
            for (i, n, ok) in &others {
                menu = menu.item(PopupItem::new(i.to_string(), *n).enabled(*ok).checked(i.to_string() == current));
            }
            tx.popup_menu(menu, "provider", |s: &mut App, picked, _| {
                if let Some(i) = picked.and_then(|p| p.parse::<usize>().ok()) {
                    s.choose_provider(i);
                }
            });
        });
    row()
        .key("assistant-head")
        .w_full()
        .h(48.0)
        .px(10.0)
        .center_y()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], t.hairline)
        .child(picker)
        .child(
            IconButton::new(&icons::NEW_CHAT)
                .variant(IconButtonVariant::GhostMuted)
                .help("New Conversation")
                .disabled(s.chat.messages.is_empty())
                .on_press(|s: &mut App| s.new_chat())
                .build(k)
                .key("new-chat"),
        )
}

fn pretty_model(id: &str) -> String {
    // claude-opus-5-5 → Opus 5.5
    let parts: Vec<&str> = id.trim_start_matches("claude-").split('-').collect();
    match parts.as_slice() {
        [fam, maj, min, ..] if min.len() <= 2 => format!("{}{} {maj}.{min}", fam[..1].to_uppercase(), &fam[1..]),
        [fam, maj] => format!("{}{} {maj}", fam[..1].to_uppercase(), &fam[1..]),
        _ => id.to_owned(),
    }
}

fn conversation(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let c = &s.chat;
    let mut list = col().key("messages").w_full().min_w(0.0).px(14.0).py(14.0).gap(12.0);
    if c.messages.is_empty() {
        let no_ai = c.detected.as_ref().is_some_and(|d| !d.providers.iter().any(|p| p.kind.supported()));
        list = list.child(empty(s, no_ai, t));
    }
    for (i, m) in c.messages.iter().enumerate() {
        let key = format!("m{i}");
        let e = match m {
            Msg::User(text_) => row().w_full().min_w(0.0).pl(36.0).justify_end().child(
                el().min_w(0.0)
                    .shrink()
                    .px(12.0)
                    .py(8.0)
                    .radius(14.0)
                    .fill(t.mint.alpha(if t.dark { 0.28 } else { 0.18 }))
                    .child(text(text_.clone()).font(k.ty.body).color(k.c.text_base).wrap_anywhere()),
            ),
            Msg::Assistant(md) => col().w_full().min_w(0.0).child(Markdown::new(md).selectable(true).build(k)),
            Msg::Activity(line) => row()
                .gap(6.0)
                .center_y()
                .child(el().size(5.0, 5.0).radius(2.5).fill(k.c.icon_faint))
                .child(text(line.clone()).font(k.ty.small).color(k.c.text_faint).truncate_middle()),
            Msg::Error(e) => row()
                .w_full()
                .px(10.0)
                .py(8.0)
                .radius(k.radius.md)
                .fill(k.c.danger.icon.alpha(0.12))
                .child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap_anywhere().min_w(0.0)),
            Msg::Proposal(p) => proposal(s, *p, t),
        };
        list = list.child(e.key(key).transition(rune::Transition::opacity()));
    }
    if !c.streaming.is_empty() {
        list = list.child(col().key("streaming").w_full().child(Markdown::new(&c.streaming).streaming(true).build(k)));
    } else if c.busy {
        list = list.child(row().key("thinking").gap(8.0).center_y().child(rune_kit::Spinner::new().build(k)).child(text("Thinking…").font(k.ty.small).color(k.c.text_faint)));
    }
    list
}

fn empty(s: &App, no_ai: bool, t: &Theme) -> El {
    let k = &t.k;
    let mut c = col().key("chat-empty").w_full().pt(24.0).gap(10.0).items_center();
    c = c.child(super::welcome::mark(36.0, t));
    if no_ai {
        return c.child(
            text("Tidy works with the AI you already use. Install Claude Code, Codex or Gemini CLI and sign in once; Tidy finds it on its own.")
                .font(k.ty.ui)
                .color(k.c.text_muted)
                .align(Align::Center)
                .wrap(),
        );
    }
    c = c.child(text("Ask about your disk").font(k.ty.body_medium).color(k.c.text_base)).child(
        text("The assistant sees names, sizes and dates from Tidy's scan, never what's inside your files. It proposes; you decide.")
            .font(k.ty.small)
            .color(k.c.text_muted)
            .align(Align::Center)
            .wrap()
            .pb(6.0),
    );
    for (i, q) in STARTERS.iter().enumerate() {
        let enabled = s.tree.is_some();
        c = c.child(
            row()
                .key(format!("starter{i}"))
                .w_full()
                .px(12.0)
                .py(9.0)
                .radius(k.radius.md)
                .border(1.0, t.hairline)
                .hover(k.c.hover)
                .press(k.c.pressed)
                .opacity(if enabled { 1.0 } else { 0.5 })
                .on_press(move |s: &mut App| s.ask((*q).to_owned()))
                .child(text(*q).font(k.ty.ui).color(k.c.text_base)),
        );
    }
    c
}

fn proposal(s: &App, idx: usize, t: &Theme) -> El {
    let k = &t.k;
    let p = &s.chat.proposals[idx];
    let picked: Vec<String> = p.items.iter().filter(|i| i.checked).map(|i| i.path.clone()).collect();
    let size: u64 = p.items.iter().filter(|i| i.checked).map(|i| i.size).sum();
    let mut card = col()
        .w_full()
        .radius(k.radius.lg)
        .border(1.0, t.mint.alpha(0.4))
        .fill(t.mint.alpha(0.06))
        .clip_rounded(k.radius.lg)
        .child(
            row()
                .w_full()
                .px(12.0)
                .pt(10.0)
                .pb(6.0)
                .gap(6.0)
                .center_y()
                .child(icon(&icons::BROOM, 14.0).color(t.mint))
                .child(text(p.title.clone()).font(k.ty.ui_medium).color(k.c.text_base).grow(1.0).min_w(0.0).truncate()),
        );
    for (j, it) in p.items.iter().enumerate() {
        let done = p.done.is_some();
        let name = it.path.rsplit('/').next().unwrap_or(&it.path).to_owned();
        card = card.child(
            row()
                .key(format!("pi{j}"))
                .w_full()
                .px(12.0)
                .py(6.0)
                .gap(10.0)
                .items_start()
                .hover(if done { rune::Color::TRANSPARENT } else { k.c.hover })
                .on_press(move |s: &mut App| {
                    let pr = &mut s.chat.proposals[idx];
                    if pr.done.is_none() {
                        pr.items[j].checked ^= true;
                    }
                })
                .label(name.clone()).role("checkbox").checked(it.checked)
                .child(Checkbox::new(it.checked).disabled(done).build(k).a11y_hidden().inert())
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(
                            row()
                                .w_full()
                                .gap(6.0)
                                .center_y()
                                .child(text(name).font(k.ty.ui).color(k.c.text_base).truncate().grow(1.0).min_w(0.0))
                                .child(super::lists::risk_badge(it.safe, t))
                                .child(text(bytes(it.size)).font(k.ty.small_medium).color(k.c.text_muted)),
                        )
                        .child(text(tilde(&it.path)).font(k.ty.text_xsmall).color(k.c.text_faint).truncate_middle())
                        .child(text(it.reason.clone()).font(k.ty.small).color(k.c.text_muted).wrap()),
                ),
        );
    }
    let foot = match p.done {
        Some(freed) => row().w_full().px(12.0).py(10.0).gap(6.0).center_y().child(icon(&icons::TRASH, 13.0).color(t.mint)).child(
            text(format!("Moved to the {} · {} freed", crate::os::TRASH, bytes(freed))).font(k.ty.small_medium).color(k.c.text_base),
        ),
        None => row().w_full().px(12.0).py(10.0).justify_end().child(
            Button::new(format!("Move {} to {} · {}", plural(picked.len(), "item"), crate::os::TRASH, bytes(size)))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Regular)
                .disabled(picked.is_empty())
                .on_press(move |s: &mut App| {
                    let freed = s.trash(&picked);
                    s.chat.proposals[idx].done = Some(freed);
                })
                .build(k),
        ),
    };
    card.child(el().w_full().h(1.0).fill(t.hairline)).child(foot)
}

fn composer(s: &App, t: &Theme) -> El {
    let k = &t.k;
    let busy = s.chat.busy;
    let can = s.tree.is_some() && !busy;
    col()
        .key("composer")
        .w_full()
        .px(12.0)
        .pt(8.0)
        .pb(12.0)
        .gap(6.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_end()
                .child(
                    TextArea::new(&s.chat.input)
                        .placeholder(if s.tree.is_some() { "Ask about your disk…" } else { "Scan a folder to start" })
                        .label("Message the assistant")
                        .rows(1)
                        .on_edit(|s: &mut App| &mut s.chat.input)
                        .on_submit(|s: &mut App, _| {
                            let q = s.chat.input.text.clone();
                            s.ask(q);
                        })
                        .build(k)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    IconButton::new(&icons::ARROW_UP)
                        .variant(IconButtonVariant::Contrast)
                        .help("Send")
                        .disabled(!can || s.chat.input.text.trim().is_empty())
                        .on_press(|s: &mut App| {
                            let q = s.chat.input.text.clone();
                            s.ask(q);
                        })
                        .build(k)
                        .key("send"),
                ),
        )
        .child({
            // One quiet line: who answers, and that the person stays in charge (or what it cost).
            let via = s.chat.detected.as_ref().and_then(|d| d.providers.get(s.chat.provider)).map_or("your assistant", |p| p.kind.name());
            text(if s.chat.cost > 0.0 { format!("Via {via} · ${:.2} so far", s.chat.cost) } else { format!("Via {via} · you approve every deletion") })
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .align(Align::Center)
                .truncate()
                .w_full()
        })
}

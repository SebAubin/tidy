//! Line icons drawn on a 16-point grid in the spirit of SF Symbols (1.3 pt strokes, round joins).

use rune::Icon;

macro_rules! icon {
    ($id:ident, $name:literal, $body:literal) => {
        pub static $id: Icon = Icon { name: concat!("tidy-", $name), view: 16.0, body: $body };
    };
}

icon!(HOME, "home", r#"<path d="M2.5 7.2 8 2.8l5.5 4.4M4 6v7.2h3v-3.7h2v3.7h3V6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(DRIVE, "drive", r#"<rect x="1.8" y="4.2" width="12.4" height="7.6" rx="2" stroke="currentColor" stroke-width="1.3" fill="none"/><circle cx="11.6" cy="8" r=".9" fill="currentColor"/><path d="M4.2 8h4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#);
icon!(FOLDER, "folder", r#"<path d="M1.9 4.4c0-.7.5-1.2 1.2-1.2h3l1.4 1.5h5.4c.7 0 1.2.5 1.2 1.2v6.5c0 .7-.5 1.2-1.2 1.2H3.1c-.7 0-1.2-.5-1.2-1.2z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/>"#);
icon!(FOLDER_PLUS, "folder-plus", r#"<path d="M1.9 4.4c0-.7.5-1.2 1.2-1.2h3l1.4 1.5h5.4c.7 0 1.2.5 1.2 1.2v6.5c0 .7-.5 1.2-1.2 1.2H3.1c-.7 0-1.2-.5-1.2-1.2z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M8 7.2v3.6M6.2 9h3.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#);
icon!(MAP, "map", r#"<rect x="2" y="2.5" width="12" height="11" rx="1.6" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8.6 2.5v11M8.6 8.4H14M2 9.6h6.6" stroke="currentColor" stroke-width="1.3"/>"#);
icon!(BROOM, "broom", r#"<path d="M13.2 2.3 8.6 6.9" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/><path d="M7.2 5.9l2.9 2.9-1 1c-1.6 1.6-3.3 2.6-6.4 3.4.8-3.1 1.8-4.8 3.4-6.4z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" fill="none"/><path d="M4.3 11.3 6 9.6" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>"#);
icon!(STACK, "stack", r#"<path d="M3 4h10M3 8h10M3 12h6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>"#);
icon!(SPARKLES, "sparkles", r#"<path d="M7 2.2 8.2 5.8 11.8 7 8.2 8.2 7 11.8 5.8 8.2 2.2 7 5.8 5.8z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/><path d="M12.4 10.2l.5 1.4 1.4.5-1.4.5-.5 1.4-.5-1.4-1.4-.5 1.4-.5z" fill="currentColor"/>"#);
icon!(RELOAD, "reload", r#"<path d="M13 8a5 5 0 1 1-1.5-3.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/><path d="M11.9 1.9v2.8H9.1" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(TRASH, "trash", r#"<path d="M2.8 4.2h10.4M6.2 4.2V2.8h3.6v1.4M4 4.2l.7 8.6c.1.6.5 1 1.1 1h4.4c.6 0 1-.4 1.1-1l.7-8.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(ARROW_UP, "arrow-up", r#"<path d="M8 13V3.4M3.8 7.4 8 3.2l4.2 4.2" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(CHEVRON, "chevron", r#"<path d="M6 3.5 10.5 8 6 12.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(BACK, "back", r#"<path d="M10 3.5 5.5 8l4.5 4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(SHIELD, "shield", r#"<path d="M8 1.9 13 3.8v3.8c0 3-2.1 5.3-5 6.5-2.9-1.2-5-3.5-5-6.5V3.8z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/><path d="M5.8 8l1.6 1.6 2.9-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(NEW_CHAT, "new-chat", r#"<path d="M13.2 8.6V12c0 .7-.5 1.2-1.2 1.2H4c-.7 0-1.2-.5-1.2-1.2V4c0-.7.5-1.2 1.2-1.2h3.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" fill="none"/><path d="M11.8 2.2l2 2-5.3 5.3-2.6.6.6-2.6z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" fill="none"/>"#);
icon!(TERMINAL, "terminal", r#"<rect x="1.9" y="2.8" width="12.2" height="10.4" rx="2" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M4.6 6.3 6.6 8l-2 1.7M8.2 10h3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
icon!(COPIES, "copies", r#"<rect x="5.2" y="5.2" width="8.6" height="8.6" rx="1.8" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M10.8 5.2V3.9c0-.9-.7-1.7-1.7-1.7H3.9c-.9 0-1.7.8-1.7 1.7v5.2c0 1 .8 1.7 1.7 1.7h1.3" stroke="currentColor" stroke-width="1.3" fill="none"/>"#);
icon!(CLOCK, "clock", r#"<circle cx="8" cy="8" r="5.8" stroke="currentColor" stroke-width="1.3" fill="none"/><path d="M8 4.8V8l2.2 1.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);

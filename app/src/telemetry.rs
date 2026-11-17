//! Anonymous usage statistics and crash reports, sent to Aptabase (aptabase.com), an open-source,
//! privacy-first analytics service for desktop apps.
//!
//! What's sent is listed in the README ("Telemetry") and nothing else: event names with a few
//! coarse properties (a view's name, sizes in buckets like "100 GB-1 TB"), the app and macOS
//! versions, and a random session id. Never paths, file or folder names, exact sizes, or what's
//! said to the assistant. Crash reports have every path taken out.
//!
//! It's on unless the person turns it off in Settings, or sets `DO_NOT_TRACK=1`. Builds without
//! an App Key (`TIDY_APTABASE_KEY` at compile time: release builds only) send nothing at all.
//! Events are batched and sent from a background thread (the first batch a few seconds after
//! launch, then once a minute, and what's left at quit); nothing is written to disk.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

/// The App Key, baked in by the release workflow. Absent: telemetry is compiled in but inert.
const APP_KEY: Option<&str> = option_env!("TIDY_APTABASE_KEY");
const FLUSH: Duration = Duration::from_secs(60);

static ENABLED: AtomicBool = AtomicBool::new(false);
static QUEUE: OnceLock<Mutex<Sender<Msg>>> = OnceLock::new();

enum Msg {
    Event(Value),
    /// Send what's queued now, then answer.
    Flush(Sender<()>),
}

/// The first batch goes soon after launch; then once a minute.
const FIRST: Duration = Duration::from_secs(5);
static SESSION: OnceLock<String> = OnceLock::new();
static SYSTEM: OnceLock<System> = OnceLock::new();

struct System {
    os_version: String,
    locale: String,
}

fn base_url(key: &str) -> Option<&'static str> {
    match key.split('-').nth(1)? {
        "EU" => Some("https://eu.aptabase.com"),
        "US" => Some("https://us.aptabase.com"),
        _ => None,
    }
}

fn key() -> Option<&'static str> {
    APP_KEY.filter(|k| base_url(k).is_some())
}

/// Whether this build can send at all (an App Key is compiled in).
pub fn available() -> bool {
    key().is_some()
}

fn do_not_track() -> bool {
    std::env::var("DO_NOT_TRACK").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Starts the sender. Call once at launch, with the person's setting.
pub fn init(enabled: bool) {
    let Some(key) = key() else { return };
    SESSION.get_or_init(|| {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let secs = (nanos / 1_000_000_000) as u64;
        let random = (nanos as u64 ^ (std::process::id() as u64).rotate_left(17)).wrapping_mul(0x9E37_79B9_7F4A_7C15) % 100_000_000;
        (secs * 100_000_000 + random).to_string()
    });
    SYSTEM.get_or_init(|| System { os_version: os_version(), locale: locale() });
    let (tx, rx) = channel();
    if QUEUE.set(Mutex::new(tx)).is_err() {
        return;
    }
    set_enabled(enabled);
    std::thread::Builder::new().name("telemetry".into()).spawn(move || sender(key, rx)).ok();
    install_panic_hook(key);
}

/// Turns sending on or off at once (from Settings). Off drops what's queued, too.
pub fn set_enabled(on: bool) {
    ENABLED.store(on && !do_not_track(), Ordering::Relaxed);
}

fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed) && key().is_some()
}

fn system_props() -> Value {
    let sys = SYSTEM.get();
    json!({
        "isDebug": cfg!(debug_assertions),
        "osName": "macOS",
        "osVersion": sys.map_or("", |s| s.os_version.as_str()),
        "locale": sys.map_or("", |s| s.locale.as_str()),
        "engineName": "Rune",
        "engineVersion": "",
        "appVersion": env!("CARGO_PKG_VERSION"),
        "sdkVersion": concat!("tidy@", env!("CARGO_PKG_VERSION")),
    })
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // Civil date from days (Howard Hinnant's algorithm).
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Records an event. `props` holds only coarse, non-identifying values (see the README).
pub fn track(name: &str, props: Value) {
    if !enabled() {
        return;
    }
    let ev = json!({
        "timestamp": now_rfc3339(),
        "sessionId": SESSION.get().cloned().unwrap_or_default(),
        "eventName": name,
        "systemProps": system_props(),
        "props": props,
    });
    if let Some(q) = QUEUE.get()
        && let Ok(tx) = q.lock()
    {
        let _ = tx.send(Msg::Event(ev));
    }
}

/// Sends what's queued, waiting at most `limit` (at quit, so nothing tracked is lost).
pub fn flush(limit: Duration) {
    if !enabled() {
        return;
    }
    let Some(q) = QUEUE.get() else { return };
    let (done, wait) = channel();
    if q.lock().map(|tx| tx.send(Msg::Flush(done)).is_ok()).unwrap_or(false) {
        let _ = wait.recv_timeout(limit);
    }
}

fn log(what: impl std::fmt::Display) {
    if std::env::var_os("TIDY_TELEMETRY_LOG").is_some() {
        eprintln!("tidy telemetry: {what}");
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).user_agent(concat!("Tidy/", env!("CARGO_PKG_VERSION"), " (macOS)")).build()
}

fn sender(key: &'static str, rx: Receiver<Msg>) {
    let url = format!("{}/api/v0/events", base_url(key).unwrap_or_default());
    let http = agent();
    let mut batch: Vec<Value> = Vec::new();
    let mut due = Instant::now() + FIRST;
    let send = |batch: &mut Vec<Value>| {
        if batch.is_empty() || !enabled() {
            batch.clear();
            return;
        }
        let n = batch.len();
        // Best effort: a failed batch is dropped, never retried from disk.
        match http.post(&url).set("App-Key", key).send_json(Value::Array(std::mem::take(batch))) {
            Ok(r) => log(format!("sent {n} event(s): HTTP {}", r.status())),
            Err(e) => log(format!("couldn't send {n} event(s): {e}")),
        }
    };
    loop {
        let wait = due.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(Msg::Event(ev)) => batch.push(ev),
            Ok(Msg::Flush(done)) => {
                send(&mut batch);
                let _ = done.send(());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return,
        }
        if Instant::now() >= due || batch.len() >= 25 {
            send(&mut batch);
            due = Instant::now() + FLUSH;
        }
    }
}

/// Every path, user name and home folder taken out of a message or stack trace. On each line,
/// everything from the start of a personal path (`/Users/…`, `/Volumes/…`, `~/…`) to the end of
/// the line goes, since a path can hold spaces; other paths become `<path>`, except Rust source
/// locations (`src/x.rs:12`), which name code, not the person's files.
pub fn scrub(text: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        let mut line = if home.len() > 1 { line.replace(&home, "~") } else { line.to_owned() };
        let cut = ["/Users/", "/Volumes/", "/private/", "~/"].iter().filter_map(|m| line.find(m)).min();
        if let Some(at) = cut {
            line.truncate(at);
            line.push_str("<path>");
        }
        let words: Vec<String> = line
            .split(' ')
            .map(|w| if w.contains('/') && !w.contains(".rs:") && w != "<path>" { "<path>".to_owned() } else { w.to_owned() })
            .collect();
        out.push(words.join(" "));
    }
    let out = out.join("\n");
    if user.len() > 2 { out.replace(&user, "<user>") } else { out }
}

fn install_panic_hook(key: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if enabled() {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".into());
            let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            let trace = format!("panicked at {at}\n{}", std::backtrace::Backtrace::force_capture());
            let body = json!({
                "errorMessage": truncate(&format!("{} (at {})", scrub(&message), scrub(&at)), 5000),
                "errorType": "panic",
                "stackTrace": truncate(&scrub(&trace), 10_000),
                "timestamp": now_rfc3339(),
                "sessionId": SESSION.get().cloned().unwrap_or_default(),
                "platform": "Rune",
                "osName": "macOS",
                "osVersion": SYSTEM.get().map_or(String::new(), |s| s.os_version.clone()),
                "appVersion": env!("CARGO_PKG_VERSION"),
                "sdkVersion": concat!("tidy@", env!("CARGO_PKG_VERSION")),
                "severity": "fatal",
                "kind": "crash",
                "isDebug": cfg!(debug_assertions),
            });
            // A crash is sent right away, on its own thread, waiting three seconds at most.
            let url = format!("{}/api/v0/error", base_url(key).unwrap_or_default());
            let (done, wait) = channel();
            std::thread::spawn(move || {
                let _ = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build().post(&url).set("App-Key", key).send_json(body);
                let _ = done.send(());
            });
            let _ = wait.recv_timeout(Duration::from_secs(3));
        }
        previous(info);
    }));
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn os_version() -> String {
    // kern.osproductversion, e.g. "27.2.1".
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    let name = c"kern.osproductversion";
    let ok = unsafe { libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) } == 0;
    if ok { String::from_utf8_lossy(&buf[..len.saturating_sub(1)]).into_owned() } else { String::new() }
}

fn locale() -> String {
    std::env::var("LANG").ok().and_then(|l| l.split('.').next().map(|s| s.replace('_', "-"))).filter(|l| !l.is_empty() && l != "C").unwrap_or_default()
}

/// Sizes in coarse buckets, never exact.
pub fn size_bucket(bytes: u64) -> &'static str {
    const G: u64 = 1_000_000_000;
    match bytes {
        0 => "0",
        b if b < G => "<1 GB",
        b if b < 10 * G => "1-10 GB",
        b if b < 100 * G => "10-100 GB",
        b if b < 1000 * G => "100 GB-1 TB",
        _ => ">1 TB",
    }
}

/// Counts in coarse buckets.
pub fn count_bucket(n: u64) -> &'static str {
    match n {
        0 => "0",
        1 => "1",
        2..=9 => "2-9",
        10..=99 => "10-99",
        100..=999 => "100-999",
        1000..=99_999 => "1k-100k",
        100_000..=999_999 => "100k-1M",
        _ => ">1M",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_reports_carry_no_paths() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/someone".into());
        let s = scrub(&format!("No such file: {home}/Documents/secret plan.pdf, from /Volumes/Backup/x"));
        assert_eq!(s, "No such file: <path>");
        let s = scrub("called `Option::unwrap()` on a `None` value at app/src/dupes.rs:12:5");
        assert!(s.contains("app/src/dupes.rs:12:5"), "a source location stays: {s}");
        assert_eq!(scrub("thread main at /private/var/folders/x y.rs"), "thread main at <path>");
    }

    #[test]
    fn buckets_and_dates() {
        assert_eq!(size_bucket(250_000_000_000), "100 GB-1 TB");
        assert_eq!(count_bucket(4_000_000), ">1M");
        let t = now_rfc3339();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
        assert_eq!(base_url("A-EU-123"), Some("https://eu.aptabase.com"));
        assert_eq!(base_url("nope"), None);
    }
}

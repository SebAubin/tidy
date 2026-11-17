//! Numbers as people read them.

/// Bytes in decimal units, as Finder shows them (1 KB = 1,000 bytes).
pub fn bytes(b: u64) -> String {
    const U: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if b < 1000 {
        return format!("{b} bytes");
    }
    let mut v = b as f64 / 1000.0;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if v >= 100.0 { format!("{v:.0} {}", U[i]) } else if v >= 10.0 { format!("{v:.1} {}", U[i]) } else { format!("{v:.2} {}", U[i]) }
}

pub fn count(n: u64) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// "3 days ago", "2 years ago".
pub fn age(secs_ago: i64) -> String {
    let d = secs_ago / 86_400;
    match d {
        i64::MIN..=0 => "today".into(),
        1 => "yesterday".into(),
        2..=59 => format!("{d} days ago"),
        60..=729 => format!("{} months ago", d / 30),
        _ => format!("{} years ago", d / 365),
    }
}

#[test]
fn formats() {
    assert_eq!(bytes(999), "999 bytes");
    assert_eq!(bytes(1_500_000), "1.50 MB");
    assert_eq!(bytes(250_000_000_000), "250 GB");
    assert_eq!(count(1234567), "1,234,567");
}

//! Squarified treemap layout (Bruls, Huizing, van Wijk): rectangles as close to square as the
//! sizes allow, largest first, so the eye compares areas easily.

pub type Rect = [f32; 4];

/// Lays `sizes` (largest first) into `rect`. Returns one rect per size, in the same order.
pub fn squarify(sizes: &[f64], rect: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().sum();
    let mut out = vec![[0.0; 4]; sizes.len()];
    if total <= 0.0 || rect[2] <= 0.0 || rect[3] <= 0.0 {
        return out;
    }
    let scale = (rect[2] as f64 * rect[3] as f64) / total;
    let areas: Vec<f64> = sizes.iter().map(|s| s * scale).collect();
    let mut free = rect.map(|v| v as f64);
    let mut i = 0;
    while i < areas.len() {
        let short = free[2].min(free[3]);
        // Grow the row while it makes the worst aspect ratio better.
        let mut j = i + 1;
        let mut best = worst(&areas[i..j], short);
        while j < areas.len() {
            let w = worst(&areas[i..=j], short);
            if w > best {
                break;
            }
            best = w;
            j += 1;
        }
        let row: f64 = areas[i..j].iter().sum();
        let thick = row / short;
        let mut at = 0.0;
        for k in i..j {
            let len = areas[k] / thick;
            out[k] = if free[2] >= free[3] {
                // Row runs down the left edge.
                [free[0], free[1] + at, thick, len]
            } else {
                [free[0] + at, free[1], len, thick]
            }
            .map(|v| v as f32);
            at += len;
        }
        if free[2] >= free[3] {
            free[0] += thick;
            free[2] -= thick;
        } else {
            free[1] += thick;
            free[3] -= thick;
        }
        i = j;
    }
    out
}

fn worst(row: &[f64], short: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    let (mx, mn) = row.iter().fold((0f64, f64::MAX), |(a, b), &v| (a.max(v), b.min(v)));
    let s2 = sum * sum;
    let w2 = short * short;
    (w2 * mx / s2).max(s2 / (w2 * mn))
}

#[test]
fn fills_the_rect_without_overlap() {
    let r = squarify(&[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0], [0.0, 0.0, 6.0, 4.0]);
    let area: f32 = r.iter().map(|r| r[2] * r[3]).sum();
    assert!((area - 24.0).abs() < 1e-3);
    for a in &r {
        assert!(a[0] >= -1e-3 && a[1] >= -1e-3 && a[0] + a[2] <= 6.001 && a[1] + a[3] <= 4.001);
    }
}

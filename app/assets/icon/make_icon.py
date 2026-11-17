"""Draws Tidy's app icon and packs it as Tidy.icns.

The mark: a disk map (treemap) on a deep teal squircle. The large tile is Tidy's mint; one
small tile is an empty dashed outline with a spark in it: space that's been freed.
Run: python3 make_icon.py  (needs Pillow; iconutil ships with macOS)
"""
import math, os, shutil, subprocess
from PIL import Image, ImageDraw, ImageFilter

S = 4  # supersampling
N = 1024 * S
HERE = os.path.dirname(os.path.abspath(__file__))


def squircle(size, inset, n=4.2):
    """Points of a superellipse, the shape of a macOS icon."""
    c = size / 2
    r = size / 2 - inset
    pts = []
    for i in range(720):
        t = 2 * math.pi * i / 720
        ct, st = math.cos(t), math.sin(t)
        x = c + r * math.copysign(abs(ct) ** (2 / n), ct)
        y = c + r * math.copysign(abs(st) ** (2 / n), st)
        pts.append((x, y))
    return pts


def vgradient(w, h, top, bottom):
    g = Image.new("RGBA", (w, h))
    d = ImageDraw.Draw(g)
    for y in range(h):
        t = y / (h - 1)
        d.line([(0, y), (w, y)], fill=tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(4)))
    return g


def rrect_mask(w, h, box, radius):
    m = Image.new("L", (w, h), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=radius, fill=255)
    return m


def main():
    img = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    body_inset = 100 * S
    body = Image.new("L", (N, N), 0)
    ImageDraw.Draw(body).polygon(squircle(N, body_inset), fill=255)

    # Shadow under the body, as macOS draws app icons on the Dock.
    sh = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    sh.putalpha(body.point(lambda v: int(v * 0.45)))
    sh = sh.filter(ImageFilter.GaussianBlur(14 * S))
    img.alpha_composite(sh, (0, 10 * S))

    bg = vgradient(N, N, (22, 74, 70, 255), (8, 34, 36, 255))
    img.paste(bg, (0, 0), body)

    # A soft light from the top.
    glow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((N * 0.05, -N * 0.55, N * 0.95, N * 0.45), fill=(120, 255, 220, 46))
    glow = glow.filter(ImageFilter.GaussianBlur(60 * S))
    masked = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    masked.paste(glow, (0, 0), body)
    img.alpha_composite(masked)

    # The treemap.
    x0, y0, x1, y1 = 232 * S, 232 * S, 792 * S, 792 * S
    gap = 30 * S
    rad = 46 * S
    w = x1 - x0
    split = x0 + int(w * 0.57)
    right_split = y0 + int((y1 - y0) * 0.50)
    small_split = split + gap + int((x1 - split - gap) * 0.5)

    def tile(box, top, bottom):
        bw, bh = box[2] - box[0], box[3] - box[1]
        g = vgradient(bw, bh, top, bottom)
        m = rrect_mask(bw, bh, (0, 0, bw - 1, bh - 1), rad)
        layer = Image.new("RGBA", (N, N), (0, 0, 0, 0))
        layer.paste(g, (box[0], box[1]), m)
        img.alpha_composite(layer)

    # Big mint tile, with a little depth.
    big = (x0, y0, split - gap // 2, y1)
    shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((big[0], big[1] + 10 * S, big[2], big[3] + 10 * S), radius=rad, fill=(0, 0, 0, 90))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(16 * S)))
    tile(big, (120, 240, 200, 255), (34, 196, 160, 255))
    # Frosted tiles.
    tile((split + gap // 2, y0, x1, right_split - gap // 2), (255, 255, 255, 92), (255, 255, 255, 64))

    # The freed tile: a dashed outline and a spark.
    fx0, fy0, fx1, fy1 = split + gap // 2, right_split + gap // 2, x1, y1
    d = ImageDraw.Draw(img)
    lw = 9 * S
    dash, space = 30 * S, 22 * S
    r = rad - lw
    perim_pts = []
    # Walk the rounded rect outline and draw dashes along it.
    def outline_points(step=2):
        pts = []
        L, T, R, B = fx0 + lw / 2, fy0 + lw / 2, fx1 - lw / 2, fy1 - lw / 2
        rr = rad - lw / 2
        segs = [
            ((L + rr, T), (R - rr, T)), ("arc", R - rr, T + rr, -90, 0),
            ((R, T + rr), (R, B - rr)), ("arc", R - rr, B - rr, 0, 90),
            ((R - rr, B), (L + rr, B)), ("arc", L + rr, B - rr, 90, 180),
            ((L, B - rr), (L, T + rr)), ("arc", L + rr, T + rr, 180, 270),
        ]
        for s in segs:
            if s[0] == "arc":
                _, cx, cy, a0, a1 = s
                for i in range(0, 60):
                    a = math.radians(a0 + (a1 - a0) * i / 60)
                    pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
            else:
                (ax, ay), (bx, by) = s
                n = int(max(abs(bx - ax), abs(by - ay)) / step)
                for i in range(n):
                    pts.append((ax + (bx - ax) * i / n, ay + (by - ay) * i / n))
        return pts
    pts = outline_points()
    acc, on, cur = 0.0, True, []
    for a, b in zip(pts, pts[1:] + pts[:1]):
        seg = math.dist(a, b)
        acc += seg
        if on:
            cur.append(a)
        if acc >= (dash if on else space):
            if on and len(cur) > 1:
                d.line(cur + [b], fill=(170, 255, 225, 210), width=lw, joint="curve")
            cur, acc, on = [], 0.0, not on
    cx, cy = (fx0 + fx1) / 2, (fy0 + fy1) / 2
    R, r2 = 62 * S, 19 * S
    star = []
    for i in range(8):
        a = math.radians(-90 + i * 45)
        rad_i = R if i % 2 == 0 else r2
        star.append((cx + rad_i * math.cos(a), cy + rad_i * math.sin(a)))
    d.polygon(star, fill=(190, 255, 232, 255))

    out = img.resize((1024, 1024), Image.LANCZOS)
    out.save(os.path.join(HERE, "icon_1024.png"))

    iconset = os.path.join(HERE, "Tidy.iconset")
    shutil.rmtree(iconset, ignore_errors=True)
    os.makedirs(iconset)
    for s in [16, 32, 128, 256, 512]:
        out.resize((s, s), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}.png"))
        out.resize((s * 2, s * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}@2x.png"))
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(HERE, "Tidy.icns")], check=True)
    shutil.rmtree(iconset)


if __name__ == "__main__":
    main()

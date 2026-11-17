"""Draws the DMG window background: Tidy on the left, Applications on the right, an arrow
between them. 660x400 points, written at 1x and 2x (combined into a TIFF by the packaging
script so Retina screens get the sharp one). Run: python3 make_background.py (needs Pillow)."""
import os
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
W, H = 660, 400
MINT = (46, 196, 160)


def font(size, bold=False):
    for path in ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]:
        try:
            return ImageFont.truetype(path, size, index=0)
        except OSError:
            continue
    return ImageFont.load_default()


def draw(scale):
    w, h = W * scale, H * scale
    img = Image.new("RGB", (w, h), (247, 248, 248))
    d = ImageDraw.Draw(img)
    # A soft mint wash at the top.
    for y in range(h):
        t = max(0.0, 1 - y / (h * 0.55))
        c = tuple(int(247 + (m - 247) * 0.10 * t) for m in (200, 245, 230))
        d.line([(0, y), (w, y)], fill=c)
    # Title and hint.
    f1, f2 = font(22 * scale), font(13 * scale)
    title = "Install Tidy"
    tw = d.textlength(title, font=f1)
    d.text(((w - tw) / 2, 34 * scale), title, font=f1, fill=(29, 29, 31))
    hint = "Drag Tidy into your Applications folder"
    hw = d.textlength(hint, font=f2)
    d.text(((w - hw) / 2, 66 * scale), hint, font=f2, fill=(110, 110, 115))
    # The arrow between the two icons (icons sit at x=180 and x=480, y=215).
    y = 215 * scale
    x0, x1 = 262 * scale, 398 * scale
    d.line([(x0, y), (x1 - 14 * scale, y)], fill=MINT, width=5 * scale)
    d.polygon([(x1, y), (x1 - 20 * scale, y - 12 * scale), (x1 - 20 * scale, y + 12 * scale)], fill=MINT)
    # Footer.
    foot = "Open it from Applications. It asks once for Full Disk Access."
    fw = d.textlength(foot, font=f2)
    d.text(((w - fw) / 2, 345 * scale), foot, font=f2, fill=(140, 140, 145))
    return img


draw(1).save(os.path.join(HERE, "background.png"))
draw(2).save(os.path.join(HERE, "background@2x.png"))

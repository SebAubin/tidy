"""Draws Tidy.ico for Windows: small sizes (16-48 px) drawn on the pixel grid, so the title bar,
taskbar and Alt-Tab show a crisp mark instead of a shrunken copy of the big icon; larger sizes
from icon_1024.png without its drop shadow. Run after make_icon.py: python3 make_ico.py"""
import os
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
SS = 8  # supersampling, with every edge on a whole pixel


def small(s):
    S = s * SS
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    px = lambda v: int(round(v)) * SS
    d.rounded_rectangle((0, 0, S - 1, S - 1), radius=px(s * 0.22), fill=(18, 62, 60, 255))
    pad = max(2, round(s * 0.18))
    gap = max(1, round(s * 0.07))
    inner = s - 2 * pad
    left_w = round(inner * 0.56)
    top_h = round((inner - gap) * 0.5)
    r = max(1, round(s * 0.07))
    # The big tile: Tidy's mint.
    d.rounded_rectangle((px(pad), px(pad), px(pad + left_w) - 1, px(s - pad) - 1), radius=px(r), fill=(72, 220, 176, 255))
    rx = pad + left_w + gap
    # Top right: frosted.
    d.rounded_rectangle((px(rx), px(pad), px(s - pad) - 1, px(pad + top_h) - 1), radius=px(r), fill=(150, 196, 188, 255))
    # Bottom right: the freed tile, an outline (a lighter fill when too small for a line).
    box = (px(rx), px(pad + top_h + gap), px(s - pad) - 1, px(s - pad) - 1)
    if s >= 24:
        d.rounded_rectangle(box, radius=px(r), outline=(150, 240, 210, 255), width=SS * max(1, s // 24))
    else:
        d.rounded_rectangle(box, radius=px(r), fill=(40, 110, 100, 255))
    return img.resize((s, s), Image.LANCZOS)


def large(s):
    big = Image.open(os.path.join(HERE, "icon_1024.png")).convert("RGBA")
    # The squircle itself, without the shadow around it: where it's nearly opaque.
    alpha = big.getchannel("A").point(lambda a: 255 if a > 230 else 0)
    body = big.crop(alpha.getbbox())
    return body.resize((s, s), Image.LANCZOS)


frames = [small(s) for s in (16, 20, 24, 32, 40, 48)] + [large(s) for s in (64, 128, 256)]
frames[-1].save(os.path.join(HERE, "Tidy.ico"), format="ICO", sizes=[f.size for f in frames], append_images=frames[:-1])
preview = Image.new("RGBA", (sum(f.size[0] for f in frames) + 10 * len(frames), 256), (240, 240, 240, 255))
x = 5
for f in frames:
    preview.paste(f, (x, 256 - f.size[1]), f)
    x += f.size[0] + 10
preview.save("/tmp/ico-preview.png") if os.access("/tmp", os.W_OK) else None
print("ok", [f.size for f in frames])

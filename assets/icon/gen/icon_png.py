# Rasterizes the ShapeCraft icon (same geometry and variants as icon_svg.py) to transparent PNGs, supersampled.
# usage: icon_png.py OUTDIR
import sys, os
import numpy as np
from PIL import Image, ImageDraw, ImageFilter
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from icon_svg import squircle, FACES, PANEL, TINT, TILE_STOPS, RIM, GLINTS, VARIANTS


def hexrgb(h): return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def vgrad(n, s, a, b, y0=12, y1=88):
    """n x n RGB image of a vertical gradient from a (at y0) to b (at y1), in icon units scaled by s."""
    y = (np.arange(n) / s - y0) / (y1 - y0)
    t = np.clip(y, 0, 1)[:, None, None]
    A, B = np.array(hexrgb(a), float), np.array(hexrgb(b), float)
    row = A + (B - A) * t
    return Image.fromarray(np.broadcast_to(row, (n, n, 3)).astype(np.uint8), "RGB")


def tile_grad(n, s, t, w):
    """the tile's vertical three-stop gradient, over the tile square."""
    v = np.clip((np.arange(n) / s - t) / w, 0, 1)
    offs = [o for o, _ in TILE_STOPS]
    cols = np.array([hexrgb(c) for _, c in TILE_STOPS], float)
    row = np.stack([np.interp(v, offs, cols[:, i]) for i in range(3)], -1)[:, None, :]
    return Image.fromarray(np.broadcast_to(row, (n, n, 3)).astype(np.uint8), "RGB")


def tile_finish(n, s, t, w, r, tm):
    """the rim glow and the two corner glints, as a white layer clipped to the tile."""
    q = w / 96
    rim = Image.new("L", (n, n), 0)
    sq = [(x * s, y * s) for x, y in squircle(t, w)]
    # the stroke straddles the edge; only its inner half survives the tile mask, like the clipped SVG stroke
    ImageDraw.Draw(rim).line(sq + sq[:1], fill=255, width=max(1, int(2 * RIM["width"] * q * s)), joint="curve")
    ys, xs = np.mgrid[0:n, 0:n] / s
    # the rim is lit from the top-left: strongest there, fading toward the bottom-right
    ramp = np.clip(((xs - t) + (ys - t)) / (2 * w), 0, 1)
    op = RIM["opacity"][0] + (RIM["opacity"][1] - RIM["opacity"][0]) * ramp
    # ...and it fades out toward the top-right and bottom-left corners
    across = np.clip(((t + w - xs) + (ys - t)) / (2 * w), 0, 1)
    op = op * (RIM["corners"] + (1 - RIM["corners"]) * (1 - np.abs(2 * across - 1)))
    a = np.array(rim.filter(ImageFilter.GaussianBlur(RIM["blur"] * q * s)), float) / 255 * op
    for corner, rad, op in GLINTS:
        c = t + corner * w
        d = np.hypot(xs - c, ys - c) / (rad * w)
        g = np.clip(1 - d, 0, 1) * op
        a = 1 - (1 - a) * (1 - g)
    a = a * (np.array(tm, float) / 255)
    layer = Image.new("RGBA", (n, n), (255, 255, 255, 0))
    layer.putalpha(Image.fromarray((a * 255).astype(np.uint8), "L"))
    return layer


def pts(p, s, k):
    out = []
    for xy in p.split():
        x, y = map(float, xy.split(","))
        x, y = 50 + (x - 50) * k, 50 + (y - 50) * k
        out.append((x * s, y * s))
    return out


def poly_mask(n, P):
    m = Image.new("L", (n, n), 0)
    ImageDraw.Draw(m).polygon(P, fill=255)
    return m


def stroke_mask(n, P, width):
    m = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(m)
    w = max(1, int(round(width)))
    for i in range(len(P)):
        d.line([P[i], P[(i + 1) % len(P)]], fill=255, width=w)
    r = width / 2
    for x, y in P:
        d.ellipse([x - r, y - r, x + r, y + r], fill=255)
    return m


def render(size, stroke=3.85, soft=0.15, hi=0.55, tint=True, mac=False):
    ss = 8 if size <= 256 else 4
    n = size * ss
    s = n / 100
    t, r, k = (9.8, 18.0, 80.4 / 96) if mac else (2.0, 22.0, 1.0)
    w = 100 - 2 * t
    out = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    tm = Image.new("L", (n, n), 0)
    ImageDraw.Draw(tm).polygon([(x * s, y * s) for x, y in squircle(t, w)], fill=255)
    if mac:  # soft drop shadow under the tile
        sh = tm.filter(ImageFilter.GaussianBlur(1.1 * s))
        sh = Image.fromarray((np.array(sh, float) * 0.32).astype(np.uint8), "L")
        shadow = Image.new("RGBA", (n, n), hexrgb("#001a66") + (0,))
        shadow.putalpha(sh)
        out.alpha_composite(shadow, (0, int(0.9 * s)))
    tile = tile_grad(n, s, t, w).convert("RGBA")
    tile.putalpha(tm)
    out.alpha_composite(tile)
    out.alpha_composite(tile_finish(n, s, t, w, r, tm))

    g = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    faces = [(kind, solid, pts(p, s, k)) for kind, solid, p in FACES]
    sw = stroke * s * k
    for kind, solid, P in faces:
        if solid:
            layer = vgrad(n, s, *PANEL[kind]).convert("RGBA")
            layer.putalpha(poly_mask(n, P))
            g.alpha_composite(layer)
    white = Image.new("RGBA", (n, n), (255, 255, 255, 255))
    for _, _, P in faces:
        layer = white.copy(); layer.putalpha(stroke_mask(n, P, sw)); g.alpha_composite(layer)
    if tint:
        for kind, _, P in faces:
            m = np.minimum(np.array(stroke_mask(n, P, sw)), np.array(poly_mask(n, P)))
            layer = vgrad(n, s, *TINT[kind]).convert("RGBA")
            layer.putalpha(Image.fromarray(m, "L"))
            g.alpha_composite(layer)
    if hi:
        for _, _, P in faces:
            layer = white.copy(); layer.putalpha(stroke_mask(n, P, hi * s * k)); g.alpha_composite(layer)
    if soft:
        g = g.filter(ImageFilter.GaussianBlur(soft * s))
    out.alpha_composite(g)
    return out.resize((size, size), Image.LANCZOS)


def variant(size):
    return "tiny" if size <= 16 else "small" if size <= 32 else "medium" if size <= 64 else "large"


if __name__ == "__main__":
    o = sys.argv[1]
    os.makedirs(o, exist_ok=True)
    for size in (16, 32, 48, 64, 128, 256, 512, 1024):
        kw = VARIANTS[variant(size)]
        render(size, **kw).save(os.path.join(o, f"shapecraft-{size}.png"))
        render(size, mac=True, **kw).save(os.path.join(o, f"mac-{size}.png"))

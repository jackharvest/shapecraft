# ShapeCraft app icon (K4, "step"): one flat net on a blue tile. Writes SVG variants tuned per size.
# usage: icon_svg.py OUTDIR
import sys, os

# measured on the reference icon: a vertical three-stop tile, an inner rim glow, and glints at top-left and bottom-right
TILE_STOPS = [(0.0, "#3f7bff"), (0.5, "#0d5cff"), (1.0, "#004fff")]
RIM = dict(width=2.6, blur=0.9, opacity=(0.6, 0.18), corners=0.15)   # icon units at a 96-unit tile; opacity top-left -> bottom-right
GLINTS = [(0, 0.16, 0.8), (1, 0.14, 0.6)]      # (corner 0=top-left 1=bottom-right, radius / tile, opacity)
# K4 faces: (kind, solid?, points). kind picks the panel and stroke-tint gradients.
FACES = [
    ("t", False, "50.00,67.76 34.62,58.88 65.38,41.12 80.76,50.00"),
    ("r", True, "80.76,50.00 80.76,67.76 50.00,85.52 50.00,67.76"),
    ("l", False, "50.00,85.52 50.00,67.76 34.62,58.88 34.62,41.12 19.24,32.24 19.24,67.76"),
    ("t", False, "34.62,41.12 19.24,32.24 50.00,14.48 65.38,23.36"),
    ("r", True, "65.38,23.36 65.38,41.12 34.62,58.88 34.62,41.12"),
]
PANEL = {"t": ("#ffffff", "#eef2fa"), "l": ("#f7f9fd", "#e2e8f5"), "r": ("#eef2fa", "#d3dcef")}
TINT = {"t": ("#ffffff", "#f1f5fc"), "l": ("#eef3fc", "#dde6f7"), "r": ("#e2e9f8", "#cfdbf3")}


def squircle(t, w, n=5.0, steps=192):
    """Apple's continuous-corner tile: the superellipse |x|^n + |y|^n = 1 over the tile square (n=5 matches the
    reference's measured corner profile)."""
    import math
    c, h = t + w / 2, w / 2
    pts = []
    for i in range(steps):
        a = 2 * math.pi * i / steps
        ca, sa = math.cos(a), math.sin(a)
        pts.append((c + h * math.copysign(abs(ca) ** (2 / n), ca), c + h * math.copysign(abs(sa) ** (2 / n), sa)))
    return pts


def grad(id_, a, b, y0=12, y1=88):
    return (f'<linearGradient id="{id_}" gradientUnits="userSpaceOnUse" x1="0" y1="{y0}" x2="0" y2="{y1}">'
            f'<stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient>')


def svg(stroke=3.85, soft=0.15, hi=0.55, tint=True, mac=False):
    # mac: Apple's icon grid (tile ~80% of the canvas, continuous-corner radius, soft drop shadow)
    if mac:
        t, r, k = 9.8, 18.0, 80.4 / 96
    else:
        t, r, k = 2.0, 22.0, 1.0
    w = 100 - 2 * t
    q = w / 96
    tile_d = "M" + " L".join(f"{x:.3f},{y:.3f}" for x, y in squircle(t, w)) + " Z"
    defs = ['<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">'
            + ''.join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in TILE_STOPS) + '</linearGradient>',
            f'<clipPath id="tclip"><path d="{tile_d}"/></clipPath>',
            f'<filter id="rimblur" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation="{RIM["blur"] * q}"/></filter>',
            '<linearGradient id="rim" x1="0" y1="0" x2="1" y2="1">'
            f'<stop offset="0" stop-color="#ffffff" stop-opacity="{RIM["opacity"][0]}"/>'
            f'<stop offset="1" stop-color="#ffffff" stop-opacity="{RIM["opacity"][1]}"/></linearGradient>',
            '<linearGradient id="across" x1="1" y1="0" x2="0" y2="1">'
            f'<stop offset="0" stop-color="#ffffff" stop-opacity="{RIM["corners"]}"/><stop offset="0.5" stop-color="#ffffff"/>'
            f'<stop offset="1" stop-color="#ffffff" stop-opacity="{RIM["corners"]}"/></linearGradient>',
            f'<mask id="rimfade" maskContentUnits="userSpaceOnUse"><rect x="{t}" y="{t}" width="{w}" height="{w}" fill="url(#across)"/></mask>']
    for i, (corner, rad, op) in enumerate(GLINTS):
        c = t + corner * w
        defs.append(f'<radialGradient id="glint{i}" gradientUnits="userSpaceOnUse" cx="{c}" cy="{c}" r="{rad * w}">'
                    f'<stop offset="0" stop-color="#ffffff" stop-opacity="{op}"/><stop offset="1" stop-color="#ffffff" stop-opacity="0"/></radialGradient>')
    defs += [grad("p" + n, *c) for n, c in PANEL.items()] + [grad("i" + n, *c) for n, c in TINT.items()]
    defs += [f'<clipPath id="c{i}"><polygon points="{p}"/></clipPath>' for i, (_, _, p) in enumerate(FACES)]
    if soft:
        defs.append(f'<filter id="soft" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation="{soft}"/></filter>')
    if mac:
        defs.append('<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">'
                    '<feDropShadow dx="0" dy="0.9" stdDeviation="1.1" flood-color="#001a66" flood-opacity="0.32"/></filter>')
    body = [f'<path d="{tile_d}" fill="url(#tile)"'
            + (' filter="url(#shadow)"' if mac else '') + '/>',
            '<g clip-path="url(#tclip)">'
            f'<path d="{tile_d}" fill="none" stroke="url(#rim)" mask="url(#rimfade)" '
            f'stroke-width="{2 * RIM["width"] * q}" filter="url(#rimblur)"/>'
            + ''.join(f'<rect x="{t}" y="{t}" width="{w}" height="{w}" fill="url(#glint{i})"/>' for i in range(len(GLINTS)))
            + '</g>']
    g = [f'<polygon points="{p}" fill="url(#p{kind})"/>' for kind, solid, p in FACES if solid]
    g += [f'<polygon points="{p}" fill="none" stroke="#ffffff" stroke-width="{stroke}" stroke-linejoin="round"/>' for _, _, p in FACES]
    if tint:
        g += [f'<polygon points="{p}" fill="none" stroke="url(#i{kind})" stroke-width="{stroke}" stroke-linejoin="round" clip-path="url(#c{i})"/>'
              for i, (kind, _, p) in enumerate(FACES)]
    if hi:
        g += [f'<polygon points="{p}" fill="none" stroke="#ffffff" stroke-width="{hi}" stroke-linejoin="round"/>' for _, _, p in FACES]
    tf = f' transform="translate(50 50) scale({k:.4f}) translate(-50 -50)"' if k != 1 else ''
    flt = ' filter="url(#soft)"' if soft else ''
    body.append(f'<g{tf}><g{flt}>' + ''.join(g) + '</g></g>')
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="1024" height="1024">'
            '<title>ShapeCraft</title><defs>' + ''.join(defs) + '</defs>' + ''.join(body) + '</svg>\n')


# per-size tuning: thin strokes and blur vanish at small sizes, so small variants go heavier and crisper
VARIANTS = {
    "large": dict(stroke=3.85, soft=0.15, hi=0.55),      # 128 px and up (the master)
    "medium": dict(stroke=4.4, soft=0.08, hi=0.5),       # 48-64 px
    "small": dict(stroke=5.4, soft=0.0, hi=0.0),         # 32 px
    "tiny": dict(stroke=6.6, soft=0.0, hi=0.0, tint=False),  # 16 px
}

if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for name, kw in VARIANTS.items():
        open(os.path.join(out, f"shapecraft-{name}.svg"), "w").write(svg(**kw))
        open(os.path.join(out, f"shapecraft-mac-{name}.svg"), "w").write(svg(mac=True, **kw))

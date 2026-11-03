#!/usr/bin/env python3
"""Writes snap scenarios that record the site's GIFs from our own app (offscreen, no window).

  python3 website/gen/gifs.py OUT_DIR      # writes OUT_DIR/<name>.jsonl, then run make_gifs.sh

Every frame is a `shot` of the real UI driven by `shapecraft-snap`. Coordinates are the 1840x1324 window
the other scenarios use; `hold`/`hover` sequences give the in-between frames.
"""
import json
import sys
from pathlib import Path

SIZE = {"size": [1840, 1324]}


def ex(command, **params):
    return {"control": "engine.execute", "params": {"command": command, "params": params}}


class Scene:
    def __init__(self, name):
        self.name, self.steps, self.n = name, [SIZE, {"control": "project.new"}, {"wait": 0.3}], 0

    def add(self, *steps):
        self.steps.extend(steps)

    def shot(self, hold=1):
        # `hold` = how many GIF frames this picture stays up (written as copies later).
        for _ in range(hold):
            self.steps.append({"shot": f"f{self.n:04d}"})
            self.n += 1

    def glide(self, a, b, frames, wait=0.02):
        for i in range(1, frames + 1):
            t = i / frames
            t = t * t * (3 - 2 * t)  # ease in-out
            p = [round(a[0] + (b[0] - a[0]) * t), round(a[1] + (b[1] - a[1]) * t)]
            self.add({"hover": p}, {"wait": wait})
            self.shot()

    def write(self, out):
        (out / f"{self.name}.jsonl").write_text("".join(json.dumps(s) + "\n" for s in self.steps))


def W(x, y, z):
    """A model point (mm): snap puts it on screen through the camera of the moment."""
    return [round(x, 3), round(y, 3), round(z, 3)]


def lerp3(a, b, t):
    return [a[i] + (b[i] - a[i]) * t for i in range(3)]


class Feature(Scene):
    """A feature clip: the ground grid on, the part already there, the camera fitted to it."""

    def __init__(self, name, setup):
        super().__init__(name)
        self.add({"control": "ui.toggle", "params": {"panel": "grid"}}, *setup, ex("deselect_all"),
                 {"control": "ui.view", "params": {"view": "default"}}, {"wait": 0.9})

    def wglide(self, a, b, frames, wait=0.02):
        # The pointer travelling between two world points (eased), a frame at each stop.
        for i in range(1, frames + 1):
            t = i / frames
            t = t * t * (3 - 2 * t)
            self.add({"hover": W(*lerp3(a, b, t))}, {"wait": wait})
            self.shot()


def plate(w=72, h=46, t=8, r=7, hole=3.2):
    """A mounting plate with rounded corners and four holes."""
    return [ex("sketch", plane="xy"), ex("rectangle", center=[0, 0], size=[w, h]), ex("sketch_fillet", radius=r),
            ex("exit_sketching"), ex("extrude", sketch=1, region=0, distance=t),
            ex("sketch", origin=[0, 0, t], normal=[0, 0, 1]),
            *[ex("circle", center=[-y, x], radius=hole) for x in (-w / 2 + 8, w / 2 - 8) for y in (-h / 2 + 8, h / 2 - 8)],
            ex("exit_sketching"), ex("extrude", sketch=3, regions=[0, 1, 2, 3], distance=-t, result="subtract")]


def sketch_rect():
    # Sketching straight onto the top of a part: a rectangle, typed to size, then a circle beside it.
    s = Feature("sketch", plate() + [ex("sketch", body=2, face="@face:2:0,0,8")])
    s.shot(6)
    a, b = [-24, -11, 8], [6, 11, 8]
    s.add({"hover": W(-30, -16, 8)}, {"key": "R"}, {"wait": 0.3})
    s.wglide([-30, -16, 8], a, 6)
    s.add({"click": W(*a)})
    s.shot(2)
    s.wglide(a, b, 12)
    s.shot(3)
    for ch in "28":
        s.add({"text": ch}, {"wait": 0.1})
        s.shot(2)
    s.add({"key": "Tab"}, {"wait": 0.1})
    s.shot(2)
    for ch in "20":
        s.add({"text": ch}, {"wait": 0.1})
        s.shot(2)
    s.add({"key": "Enter"}, {"wait": 0.3}, ex("deselect_all"))
    s.shot(5)
    c = [17, 0, 8]
    s.add({"key": "C"}, {"wait": 0.2})
    s.wglide([4, 9, 8], c, 6)
    s.add({"click": W(*c)})
    s.shot(2)
    s.wglide(c, [23, 2, 8], 8)
    s.add({"text": "7"}, {"wait": 0.1})
    s.shot(3)
    s.add({"key": "Enter"}, {"wait": 0.3}, ex("deselect_all"))
    s.shot(16)
    return s


def push_pull():
    # A boss pulled up out of the plate; then a hexagon pushed right through it, cutting a hole of its shape, until
    # the Result badge is switched to New Body: the hole fills back in and the hexagon is a part of its own.
    setup = plate() + [ex("sketch", body=2, face="@face:2:0,0,8"),
                       ex("circle", center=[-16, 0], radius=10), ex("exit_sketching"),
                       ex("sketch", body=2, face="@face:2:0,0,8"),
                       ex("polygon", center=[15, 0], radius=9, sides=6, angle=90),
                       ex("exit_sketching")]
    s = Feature("extrude", setup)
    s.shot(5)
    # The arrow comes up where the region is clicked: off the circle's centre point, which would pick the point.
    boss, hexa = [-12, 5, 8], [13, 4, 8]
    s.wglide([10, -30, 8], boss, 7)
    s.add({"click": W(*boss)}, {"wait": 0.4})
    s.shot(4)
    s.add({"hold": [W(*boss), W(-12, 5, 8.5)]}, {"wait": 0.05})
    s.wglide([-12, 5, 8.5], [-12, 5, 26], 16)
    s.shot(4)
    s.add({"release": True}, {"wait": 0.4}, {"key": "Enter"}, {"wait": 0.2}, {"key": "Escape"}, ex("deselect_all"), {"hover": W(-60, -70, 0)}, {"wait": 0.3})
    s.shot(4)
    s.wglide([-12, 5, 26], hexa, 7)
    s.add({"click": W(*hexa)}, {"wait": 0.4})
    s.shot(4)
    s.add({"hold": [W(*hexa), W(13, 4, 7.6)]}, {"wait": 0.05})
    s.wglide([13, 4, 7.6], [13, 4, -4], 16)
    s.shot(6)
    s.add({"release": True}, {"wait": 0.4})
    s.shot(6)
    # Its Result badge (above the distance pill; screen points, measured for this camera) → New Body.
    badge, row = [1060, 687], [1145, 702]
    s.glide([1060, 735], badge, 6)
    s.add({"click": badge}, {"wait": 0.3})
    s.shot(5)
    s.glide(badge, row, 6)
    s.add({"click": row}, {"wait": 0.4})
    s.shot(8)
    s.add({"key": "Enter"}, {"wait": 0.2}, {"key": "Escape"}, ex("deselect_all"), {"hover": W(-60, -70, 0)}, {"wait": 0.3})
    s.shot(6)
    # Ta-da: the hexagon (body 6) lifts out of the whole plate. A Translate step, its offset eased up frame by frame.
    s.add(ex("translate", by=[0, 0, 0], items=[6]), ex("deselect_all"))
    for i in range(1, 15):
        t = i / 14
        t = t * t * (3 - 2 * t)
        s.add({"control": "engine.execute", "params": {"command": "edit_step", "params": {"step": 8, "params": {"by": [0, 0, round(24 * t, 2)]}}}},
              {"wait": 0.03})
        s.shot()
    s.shot(16)
    return s


def bracket():
    """An angle bracket: a holed base, an upright with a slot, and a gusset between them."""
    return [ex("sketch", plane="xy"), ex("rectangle", center=[0, 0], size=[64, 40]), ex("sketch_fillet", radius=5),
            ex("exit_sketching"), ex("extrude", sketch=1, region=0, distance=6),
            ex("sketch", origin=[0, 0, 6], normal=[0, 0, 1]),
            ex("rectangle", corner=[-20, -32], opposite=[-14, 32]),
            ex("exit_sketching"), ex("extrude", sketch=3, region=0, distance=34, result="union"),
            ex("sketch", origin=[0, 0, 6], normal=[0, 0, 1]),
            *[ex("circle", center=[-y, x], radius=3.4) for x in (-20, 20) for y in (4,)],
            ex("exit_sketching"), ex("extrude", sketch=4, regions=[0, 1], distance=-6, result="subtract")]


def fillet():
    # Rounding the inside corner of a bracket, then its top edge.
    s = Feature("fillet", bracket())
    s.shot(5)
    # The inside corner: click the edge, then pull its arrow out along the corner's bisector.
    inner = [6, 14, 6]
    s.wglide([20, 2, 6], inner, 8)
    s.add({"click": W(*inner)}, {"wait": 0.5})
    s.shot(4)
    grab = [4.5, 13.2, 6.8]
    s.add({"hold": [W(*grab), W(4.5, 12.8, 7.2)]}, {"wait": 0.05})
    s.wglide([4.5, 12.8, 7.2], [4.5, 7, 13], 16)
    s.shot(4)
    s.add({"release": True}, {"wait": 0.4}, {"key": "Enter"}, {"wait": 0.2}, {"key": "Escape"}, ex("deselect_all"), {"hover": W(-60, -70, 0)}, {"wait": 0.3})
    s.shot(4)
    # Then the upright's top front edge.
    top = [8, 14, 40]
    s.wglide([4.5, 7, 13], top, 8)
    s.add({"click": W(*top)}, {"wait": 0.5})
    s.shot(4)
    # Its arrow sits a little along the edge from the click and points down the front face.
    s.add({"hold": [W(4.7, 14, 40.2), W(4.5, 14, 39.6)]}, {"wait": 0.05})
    s.wglide([4.5, 14, 39.6], [1.5, 14, 33], 14)
    s.shot(4)
    s.add({"release": True}, {"wait": 0.4}, {"key": "Enter"}, {"wait": 0.2}, {"key": "Escape"}, ex("deselect_all"), {"hover": W(-60, -70, 0)}, {"wait": 0.3})
    s.shot(16)
    return s


def rotate():
    # Arrange: one arm on a hub, a circular pattern that grows to eight, then the arm's own Rotate step turned, and
    # every linked copy swings with it. The camera drifts round the whole time.
    setup = [ex("sketch", plane="xy"), ex("circle", center=[0, 0], radius=30),
             ex("exit_sketching"), ex("extrude", sketch=1, region=0, distance=8),
             ex("sketch", origin=[0, 0, 8], normal=[0, 0, 1]), ex("circle", center=[0, 0], radius=8),
             ex("exit_sketching"), ex("extrude", sketch=3, region=0, distance=8, result="new"),
             ex("sketch", origin=[0, 0, 16], normal=[0, 0, 1]),
             ex("rectangle", corner=[-5, -6], opposite=[5, 46]), ex("sketch_fillet", radius=5),
             ex("exit_sketching"), ex("extrude", sketch=5, region=0, distance=5, result="new"),
             ex("sketch", origin=[0, 0, 21], normal=[0, 0, 1]), ex("circle", center=[0, 40], radius=2.6),
             ex("exit_sketching"), ex("extrude", sketch=7, region=0, distance=-5, result="subtract")]
    s = Feature("rotate", setup)
    arm, rot_step, pat_step = 6, 8, 9
    k = [0]
    total = 96

    def cam():
        t = k[0] / total
        k[0] += 1
        return {"control": "ui.camera", "params": {"target": [0, 0, 12], "yaw": -0.95 + 0.9 * t, "pitch": 0.62 - 0.08 * t,
                                                    "distance": 250, "ortho": 0}}

    def frame(*steps, n=1):
        for _ in range(n):
            s.add(*steps, cam(), {"wait": 0.03})
            steps = ()
            s.shot()

    frame(n=6)
    frame(ex("select", picks=[{"kind": "body", "body": arm}]), n=5)
    # The arm's turn about the hub (0° for now) and the pattern after it, so editing the turn moves every copy.
    frame(ex("rotate", origin=[0, 0, 0], axis=[0, 0, 1], angle=0, items=[arm]),
          ex("pattern_circular", origin=[0, 0, 0], axis=[0, 0, 1], count=2, angle=360, items=[arm]),
          ex("select", picks=[{"kind": "body", "body": arm}]), n=4)
    for count in range(3, 9):
        frame({"control": "engine.execute", "params": {"command": "edit_step", "params": {"step": pat_step, "params": {"count": count}}}},
              n=4)
    frame(ex("deselect_all"), n=6)
    for i in range(1, 25):
        t = i / 24
        t = t * t * (3 - 2 * t)
        frame({"control": "engine.execute", "params": {"command": "edit_step", "params": {"step": rot_step, "params": {"angle": round(40 * t, 2)}}}})
    frame(n=total - k[0])
    return s


def handheld():
    """Speed-lapse: a clamshell handheld built from nothing, the camera drifting round it, the lid snapping shut
    and opening again at the end. Every step is a real command; see handheld.py."""
    import math
    import handheld as hh

    s = Scene("handheld")
    steps = hh.steps()
    target = [0, 70, 12]
    # Every frame is taken in the same state so nothing on screen flickers: the ground grid stays on, nothing is
    # left selected (a selection filters History and tints the model), and a sketch goes in whole (sketching swaps
    # the toolbar and moves the grid), so frames land after its Exit, after each solid step and each colour.
    weight = {"exit_sketching": 1, "extrude": 3, "fillet": 3, "set_appearance": 2}
    plan = [weight.get(c, 0) for c, _ in steps]
    total = sum(plan) + 70
    s.add({"control": "ui.toggle", "params": {"panel": "grid"}})
    k = [0]

    start = {"yaw": -1.15, "pitch": 0.62}
    last = dict(start)

    def at(yaw, pitch):
        last.update(yaw=yaw, pitch=pitch)
        return {"control": "ui.camera", "params": {"target": target, "yaw": yaw, "pitch": pitch, "distance": 330, "ortho": 0}}

    def cam(extra=0.0):
        t = k[0] / total
        k[0] += 1
        return at(start["yaw"] + 0.85 * t + extra, start["pitch"] - 0.12 * t)

    s.add(cam(), {"wait": 0.2})
    s.shot(4)
    for (c, p), n in zip(steps, plan):
        if c == "rotate":
            break
        s.add(ex(c, **p))
        if c in ("extrude", "fillet"):
            s.add(ex("deselect_all"))
        for _ in range(n):
            s.add(cam(), {"wait": 0.05})
            s.shot()
    # The lid: flat, shut, then open, by editing the Rotate step's angle frame by frame.
    rot = [p for c, p in steps if c == "rotate"][0]
    s.add(ex("rotate", **dict(rot, angle=0)), ex("deselect_all"))
    step_index = len([1 for c, _ in steps if c not in hh.NOT_IN_HISTORY]) - 1

    def ease(a, b, frames):
        for i in range(1, frames + 1):
            t = i / frames
            t = t * t * (3 - 2 * t)
            s.add({"control": "engine.execute", "params": {"command": "edit_step", "params": {
                "step": step_index, "params": {"angle": a + (b - a) * t}}}}, cam(), {"wait": 0.05})
            s.shot()

    s.shot(3)
    ease(0, 178, 16)
    s.shot(4)
    ease(178, hh.LID_OPEN, 18)
    # A last slow turn to show it off.
    for i in range(24):
        s.add(cam(0.02 * i), {"wait": 0.05})
        s.shot()
    s.shot(6)
    # The loop: the parts fly off in turn while the camera swings back to where it started, so the last frame is
    # the empty grid the video opens on. Each group gets a Translate step whose offset grows frame by frame.
    groups = [([12, 14, 16], [0, -60, 520]), ([6, 9], [0, 420, 380]), ([4, 19], [-520, 120, 160]), ([2], [480, -300, 120])]
    first = step_index + 1
    for items, _ in groups:
        s.add(ex("translate", by=[0, 0, 0], items=items))
    s.add(ex("deselect_all"))
    frames, lag, y0, p0 = 30, 4, last["yaw"], last["pitch"]
    # Swing back the short way round.
    dy = (start["yaw"] - y0 + math.pi) % (2 * math.pi) - math.pi
    for f in range(1, frames + 1):
        for g, (items, far) in enumerate(groups):
            u = min(1.0, max(0.0, (f - g * lag) / (frames - 3 * lag)))
            e = u * u * u  # slow to leave, then gone
            s.add({"control": "engine.execute", "params": {"command": "edit_step", "params": {
                "step": first + g, "params": {"by": [round(c * e, 2) for c in far]}}}})
        t = f / frames
        t = t * t * (3 - 2 * t)
        s.add(at(y0 + dy * t, p0 + (start["pitch"] - p0) * t), {"wait": 0.05})
        s.shot()
    s.shot(2)
    return s


def stills():
    s = Scene("stills")
    s.steps = [SIZE, {"control": "ui.landing", "params": {"section": "whats_new"}}, {"wait": 0.6},
               {"shot": "home-whats-new"},
               {"control": "ui.landing", "params": {"section": "get_started"}}, {"wait": 0.6},
               {"shot": "home-get-started"},
               {"control": "ui.landing", "params": {"section": "tutorials"}}, {"wait": 0.6},
               {"shot": "home-tutorials"},
               {"control": "project.new"}, {"wait": 0.3},
               ex("sketch", plane="xy"), ex("rectangle", center=[0, 0], size=[40, 24]),
               ex("circle", center=[10, 0], radius=5),
               ex("extrude", sketch=1, region=0, distance=12), ex("deselect_all"),
               {"control": "ui.view", "params": {"view": "default"}}, {"wait": 0.8},
               {"shot": "workspace"}]
    return s


if __name__ == "__main__":
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    sys.path.insert(0, str(Path(__file__).parent))
    for scene in (sketch_rect(), push_pull(), fillet(), rotate(), handheld(), stills()):
        scene.write(out)
        print(out / f"{scene.name}.jsonl")

#!/usr/bin/env python3
"""Writes bench/tasks/*.jsonl, the task benchmark (see bench/README.md).

Each task is a workflow a user of the reference does, driven through the real UI offscreen where the input matters (keys,
clicks on model points, typed values) and through commands where only the result does, with checks on the model.
Expected values are worked out here from geometry, never read back from our own app. A task for something we don't
have yet stays in and scores 0: the benchmark measures parity, not what we built.

Run: python3 bench/make_tasks.py && shapecraft-snap --bench bench/tasks
"""
import json
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "tasks")
PI = math.pi
TASKS = {}


def E(command, **params):
    return {"control": "engine.execute", "params": {"command": command, "params": params}}


def C(method, **params):
    return {"control": method, "params": params}


def expect(**kw):
    return {"expect": kw}


def task(name, category, about, steps, allow_errors=False):
    meta = {"category": category, "about": about}
    if allow_errors:
        meta["allow_errors"] = True
    TASKS[name] = [{"task": meta}, {"size": [1840, 1324]}, C("project.new")] + steps


def box(w=20, d=10, h=10, sketch=1, at=(0, 0), plane="xy"):
    """A box by commands: sketch `sketch` (its id), body id sketch + 1."""
    return [
        E("sketch", plane=plane),
        E("rectangle", corner=list(at), opposite=[at[0] + w, at[1] + d]),
        E("extrude", sketch=sketch, region=0, distance=h),
        E("deselect_all"),
    ]


def ui_rect(w, d, corner=(0, 0)):
    """R, a click on the grid (top view), the size typed: what a user does."""
    return [
        C("ui.view", view="top"), {"wait": 0.8},
        {"key": "R"}, {"click": [corner[0], corner[1], 0]}, {"hover": [corner[0] + 3, corner[1] + 3, 0]},
        {"text": str(w)}, {"key": "Tab"}, {"text": str(d)}, {"key": "Enter"}, {"wait": 0.3},
    ]


def ui_extrude_region(at, h):
    """Escape the sketch tool, click the region, E, type the distance."""
    return [{"key": "Escape"}, {"click": at}, {"wait": 0.3}, {"key": "E"}, {"text": str(h)}, {"key": "Enter"}, {"wait": 0.3}]


def default_view():
    return [E("deselect_all"), C("ui.view", view="default"), {"wait": 1.0}]


def fillet_loss(r, length):
    return (1 - PI / 4) * r * r * length


def rounded_box(a, b, c, r):
    ia, ib, ic = a - 2 * r, b - 2 * r, c - 2 * r
    return ia * ib * ic + 2 * r * (ia * ib + ib * ic + ic * ia) + PI * r * r * (ia + ib + ic) + 4 / 3 * PI * r ** 3


def frustum(a1, a2, h):
    return h / 3 * (a1 + a2 + math.sqrt(a1 * a2))


# ---------------------------------------------------------------- Sketch drawing tools
task("sketch-01-rect-typed", "sketch", "R, click a corner, type 12 Tab 8 Enter; click the region, E 10",
     ui_rect(12, 8) + [expect(sketches=1, regions=1)] + ui_extrude_region([3, 3, 0], 10) + [expect(bodies=1, volume=960)])
task("sketch-02-circle-typed", "sketch", "C, click the center, type the diameter 10; extrude 10",
     [C("ui.view", view="top"), {"wait": 0.8}, {"key": "C"}, {"click": [0, 0, 0]}, {"hover": [3, 3, 0]},
      {"text": "10"}, {"key": "Enter"}, {"wait": 0.3}, expect(regions=1)]
     + ui_extrude_region([1, 1, 0], 10) + [expect(volume=PI * 25 * 10)])
task("sketch-03-line-loop", "sketch", "L, click three corners and back to the first: a closed triangle region",
     [C("ui.view", view="top"), {"wait": 0.8}, {"key": "L"},
      {"click": [0, 0, 0]}, {"click": [10, 0, 0]}, {"click": [0, 8, 0]}, {"click": [0, 0, 0]}, {"wait": 0.3},
      expect(regions=1)] + ui_extrude_region([2, 2, 0], 5) + [expect(volume=200)])
task("sketch-04-drag-rect", "sketch", "R, press-drag-release draws a rectangle",
     [C("ui.view", view="top"), {"wait": 0.8}, {"key": "R"}, {"drag": [[0, 0, 0], [10, 8, 0]]}, {"wait": 0.3},
      expect(sketches=1, regions=1)])
task("sketch-05-polygon", "sketch", "Hexagon of radius 10, extruded 1",
     [E("sketch", plane="xy"), E("polygon", center=[0, 0], radius=10, sides=6), E("extrude", sketch=1, region=0, distance=1),
      expect(volume=3 * math.sqrt(3) / 2 * 100)])
task("sketch-06-circle-in-rect", "sketch", "A circle inside a rectangle splits into a ring and a disc",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 20]), E("circle", center=[10, 10], radius=5),
      expect(regions=2)])
task("sketch-07-trim", "sketch", "A line across a rectangle, then Trim the piece outside: two regions",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]), E("line", a=[10, -5], b=[10, 15]),
      expect(regions=2), E("trim", entity=4, at=[10, -2]), E("trim", entity=4, at=[10, 12]), expect(regions=2)])
task("sketch-08-sketch-fillet", "sketch", "Round every corner of a 20 × 10 rectangle by 2; extrude 1",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]), E("sketch_fillet", radius=2),
      E("extrude", sketch=1, region=0, distance=1), expect(volume=200 - (4 - PI) * 4)])
task("sketch-09-offset-region", "sketch", "Offset a rectangle's outline 2 outwards: a ring region (corner style unobserved: round or sharp both pass)",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]), E("offset_edge", distance=2, region=0),
      expect(regions=2), E("extrude", sketch=1, regions=[0, 1], distance=1),
      expect(volume_between=[24 * 14 - (4 - PI) * 4 - 0.01, 24 * 14 + 0.01])])
task("sketch-10-ellipse", "sketch", "Ellipse 10 × 5, extruded 2",
     [E("sketch", plane="xy"), E("ellipse", center=[0, 0], rx=10, ry=5), E("extrude", sketch=1, region=0, distance=2),
      expect(volume=PI * 50 * 2)])
task("sketch-11-arc-slot", "sketch", "Two lines and two 3-point arcs make a slot region",
     [E("sketch", plane="xy"), E("line", a=[0, 0], b=[20, 0]), E("line", a=[0, 10], b=[20, 10]),
      E("arc", a=[20, 0], mid=[25, 5], b=[20, 10]), E("arc", a=[0, 10], mid=[-5, 5], b=[0, 0]),
      E("extrude", sketch=1, region=0, distance=1), expect(regions=1, volume=200 + PI * 25)])
task("sketch-12-text", "sketch", "Text makes regions that extrude",
     [E("sketch", plane="xy"), E("text", at=[0, 0], text="HI", size=10), E("extrude", sketch=1, regions=[0, 1], distance=1),
      expect(bodies=1)])
task("sketch-13-sketch-on-face", "sketch", "Sketch on a box's top face (click it with Sketch armed), circle, cut",
     box() + default_view() + [{"key": "S", "mods": "shift"}, {"wait": 0.2}, E("sketch", body=2, face="@face:2:10,5,10"),
                               E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-10),
                               expect(bodies=1, volume=2000 - PI * 4 * 10)])
task("sketch-14-spline", "sketch", "A closed fit-point spline makes a region",
     [E("sketch", plane="xy"), E("spline", points=[[0, 0], [10, -5], [20, 0], [10, 10], [0, 0]]), expect(regions=1)])
task("sketch-15-project-edges", "sketch", "Project a box's top face outline into a sketch above it",
     box() + [E("offset_plane", **{"from": "xy"}, distance=20), E("sketch", construction=3),
              E("project", faces=[{"body": 2, "face": 5}]), expect(regions=1)])

# ---------------------------------------------------------------- Constraints, dimensions, snapping
task("cons-01-dimension-width", "constraints", "Dimension a rectangle's bottom side to 30: the region follows",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]),
      E("constrain", kind="distance", entities=[0], value=30), E("extrude", sketch=1, region=0, distance=1),
      expect(volume=300)])
task("cons-02-horizontal", "constraints", "Horizontal on a slanted line levels it",
     [E("sketch", plane="xy"), E("line", a=[0, 0], b=[10, 1]), E("constrain", kind="horizontal_vertical", entities=[0]),
      C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/0/b/1", "same": "/sketches/0/entities/0/a/1"}}])
task("cons-03-equal-circles", "constraints", "Equal makes the second circle's radius the first's",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=5), E("circle", center=[20, 0], radius=3),
      E("constrain", kind="equal", entities=[0, 1]), E("extrude", sketch=1, regions=[0, 1], distance=1),
      expect(volume=2 * PI * 25)])
task("cons-04-perpendicular", "constraints", "Perpendicular turns the second line square to the first",
     [E("sketch", plane="xy"), E("line", a=[0, 0], b=[10, 0]), E("line", a=[0, 0], b=[3, 10]),
      E("constrain", kind="perpendicular", entities=[0, 1]),
      C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/1/b/0", "same": "/sketches/0/entities/1/a/0"}}])
task("cons-05-radius", "constraints", "Radius dimension 8 on a circle",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=5), E("constrain", kind="radius", entities=[0], value=8),
      E("extrude", sketch=1, region=0, distance=1), expect(volume=PI * 64)])
task("cons-06-dim-keeps-rect", "constraints", "Changing one side keeps the rectangle a rectangle (perpendicular kept)",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]),
      E("constrain", kind="distance", entities=[1], value=15), E("extrude", sketch=1, region=0, distance=1),
      expect(volume=300)])
task("cons-07-angle", "constraints", "Angle 45° between two lines",
     [E("sketch", plane="xy"), E("line", a=[0, 0], b=[10, 0]), E("line", a=[0, 0], b=[10, 3]),
      E("constrain", kind="angle", entities=[0, 1], value=45),
      C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/1/b/1", "same": "/sketches/0/entities/1/b/0"}}])
task("cons-08-ui-type-dimension", "constraints", "Click a rectangle's dimension in the sketch and type a new value",
     ui_rect(20, 10) + [{"key": "Escape"}, {"click": [10, 12.5, 0]}, {"wait": 0.2}, {"text": "30"}, {"key": "Enter"},
                        {"wait": 0.3}, E("exit_sketching"), E("extrude", sketch=1, region=0, distance=1), expect(volume=300)])
task("cons-09-concentric", "constraints", "Concentric moves the second circle onto the first's center",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=10), E("circle", center=[1, 1], radius=3),
      E("constrain", kind="concentric", entities=[0, 1]), expect(regions=2),
      C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/1/center/0", "near": 0, "tol": 1e-4}}])
task("cons-10-symmetry", "constraints", "Symmetry of two points about a line",
     [E("sketch", plane="xy"), E("line", a=[0, -10], b=[0, 10]), E("circle", center=[-5, 0], radius=1),
      E("circle", center=[4, 1], radius=1), E("constrain", kind="symmetry", entities=[1, 2, 0]),
      C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/2/center/0", "neg": "/sketches/0/entities/1/center/0"}}])

# ---------------------------------------------------------------- Solid modeling
task("model-01-box-ui", "modeling", "Rectangle then E: a 20 × 10 × 12 box, all by input",
     ui_rect(20, 10) + ui_extrude_region([5, 5, 0], 12) + [expect(bodies=1, volume=2400, bounds=[[0, 0, 0], [20, 10, 12]])])
task("model-02-pushpull-face", "modeling", "Click the top face, type 15 (Offset Face measures the Total thickness, as observed): the box is 15 tall",
     box() + default_view() + [{"click": [10, 5, 10]}, {"wait": 0.3}, {"text": "15"}, {"key": "Enter"}, {"wait": 0.3},
                               expect(bodies=1, volume=3000, bounds=[[0, 0, 0], [20, 10, 15]])])
task("model-03-cut-hole", "modeling", "Circle on the top face extruded -10 cuts a through hole",
     box() + [E("sketch", body=2, face="@face:2:10,5,10"), E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-10),
              expect(bodies=1, volume=2000 - PI * 40)])
task("model-04-fillet-edge-ui", "modeling", "Click the top front edge, type 2: a 2 mm fillet",
     box() + default_view() + [{"click": [10, 0, 10]}, {"wait": 0.3}, {"text": "2"}, {"key": "Enter"}, {"wait": 0.3},
                               expect(bodies=1, volume=2000 - fillet_loss(2, 20))])
task("model-05-fillet-all", "modeling", "Fillet every edge of a 20 × 10 × 10 box by 1",
     box() + [E("select", picks=[{"kind": "body", "body": 2}]), E("fillet", radius=1), expect(volume=rounded_box(20, 10, 10, 1))])
task("model-06-chamfer-ui", "modeling", "Edge, then Chamfer from the toolbar, type 2",
     box() + default_view() + [{"click": [10, 0, 10]}, {"wait": 0.3}, C("ui.tool", tool="none"),
                               E("chamfer", distance=2), expect(volume=2000 - 2 * 20)])
task("model-07-two-distance-chamfer", "modeling", "Two-distance chamfer 2 and 4 on one edge",
     box() + [E("chamfer", body=2, edges=[11], distance=2, distance2=4), expect(volume=2000 - 0.5 * 2 * 4 * 20)])
task("model-08-shell", "modeling", "Shell the top face open with 1 mm walls",
     box() + default_view() + [{"click": [10, 5, 10]}, C("ui.tool", tool="none"), E("shell", thickness=1),
                               expect(bodies=1, volume=2000 - 18 * 8 * 9)])
task("model-09-revolve", "modeling", "Revolve a 5 × 10 rectangle 5 mm off the axis: a tube",
     [E("sketch", plane="xz"), E("rectangle", corner=[5, 0], opposite=[10, 10]),
      E("revolve", sketch=1, region=0, axis={"origin": [0, 0, 0], "dir": [0, 0, 1]}, angle=360),
      expect(bodies=1, volume=PI * (100 - 25) * 10)])
task("model-10-loft", "modeling", "Loft a 20 square at z=0 to a 10 square at z=10",
     [E("sketch", plane="xy"), E("rectangle", center=[0, 0], size=[20, 20]), E("offset_plane", **{"from": "xy"}, distance=10),
      E("sketch", construction=2), E("rectangle", center=[0, 0], size=[10, 10]),
      E("loft", profiles=[{"sketch": 1, "region": 0}, {"sketch": 3, "region": 0}]),
      expect(bodies=1, volume=frustum(400, 100, 10))])
task("model-11-sweep", "modeling", "Sweep a circle along a straight path: a cylinder",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=2), E("sketch", plane="xz"), E("line", a=[0, 0], b=[0, 30]),
      E("sweep", profile={"sketch": 1, "region": 0}, path={"sketch": 2}), expect(bodies=1, volume=PI * 4 * 30)])
task("model-12-draft", "modeling", "Extrude with a 10° draft",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 20]), E("extrude", sketch=1, region=0, distance=5, draft=10),
      expect(volume=frustum(400, (20 - 10 * math.tan(math.radians(10))) ** 2, 5))])
task("model-13-symmetric", "modeling", "Symmetric extrude 10 puts 10 on each side (observed s16-009)",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[10, 10]), E("extrude", sketch=1, region=0, distance=10, sides="symmetric"),
      expect(bounds=[[0, 0, -10], [10, 10, 10]])])
task("model-14-through-all", "modeling", "Through All cut from a sketch above the part",
     box() + [E("offset_plane", **{"from": "xy"}, distance=30), E("sketch", construction=3), E("circle", center=[10, 5], radius=2),
              E("extrude", sketch=4, region=0, distance=-1, extent="through_all", result="subtract"), expect(bodies=1, volume=2000 - PI * 40)])
task("model-15-booleans", "modeling", "Union, then Subtract a third box",
     box() + box(10, 10, 10, sketch=3, at=(10, 0)) + box(4, 4, 30, sketch=5, at=(2, 2)) +
     [E("union", target=2, tools=[4]), expect(bodies=2), E("subtract", target=2, tools=[6]),
      expect(bodies=1, volume=2000 + 1000 - 1000 - 16 * 10)])
task("model-16-intersect", "modeling", "Intersect two overlapping boxes",
     box() + box(20, 10, 10, sketch=3, at=(10, 5)) + [E("intersect", target=2, tools=[4]), expect(bodies=1, volume=10 * 5 * 10)])
task("model-17-offset-face", "modeling", "Offset Face moves a side face out by 3",
     box() + [E("offset_face", body=2, faces=[2], distance=3), expect(volume=23 * 10 * 10)])
task("model-18-split", "modeling", "Split a box with the YZ plane at x=10: two bodies",
     box() + [E("split_body", origin=[10, 0, 0], normal=[1, 0, 0], items=[2]), expect(bodies=2, volumes=[1000, 1000])])
task("model-19-g2-fillet", "modeling", "G2 fillet on a box's top edges",
     box() + [E("fillet", body=2, edges=[3, 6, 9, 11], radius=2, continuity="g2"), expect(bodies=1, volume_between=[1900, 1999])])
task("model-20-wrap-emboss", "modeling", "Wrap & Emboss a sketch onto a cylinder",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=10), E("extrude", sketch=1, region=0, distance=20),
      E("sketch", plane="xz"), E("rectangle", corner=[-2, 5], opposite=[2, 15]),
      E("wrap_emboss", sketch=3, region=0, body=2, depth=1), expect(bodies=1, volume_between=[PI * 2000 + 1, PI * 2000 + 100])])
task("model-25-fillet-curvature", "modeling", "Fillet the top edges with the card's Curvature -1: a flat bevel",
     box() + [E("fillet", body=2, edges=[3, 6, 9, 11], radius=2, curvature=-1), expect(bodies=1, volume_between=[1880, 1899])])
task("model-22-extrude-to-object", "modeling", "Extrude a region up to a face of another body",
     box() + [E("sketch", plane="xy"), E("circle", center=[40, 5], radius=2),
              E("offset_plane", **{"from": "xy"}, distance=25), E("extrude", sketch=3, region=0, distance=1, extent="to_object", to={"plane": 4}),
              expect(bodies=2, volume=2000 + PI * 4 * 25)])
task("model-23-variable-fillet", "modeling", "Variable-radius fillet (2 to 4 along an edge)",
     box() + [E("fillet", body=2, edges=[11], radius=2, radius2=4), expect(volume_between=[2000 - fillet_loss(4, 20), 2000 - fillet_loss(2, 20)])])
task("model-24-fillet-chain-corner", "modeling", "Fillet three edges meeting at a corner (corner blend)",
     box() + default_view() + [{"click": [10, 0, 10]}, {"click": [20, 5, 10], "mods": "shift"}, {"click": [20, 0, 5], "mods": "shift"},
                               {"wait": 0.3}, {"text": "2"}, {"key": "Enter"}, {"wait": 0.3}, expect(selection=0),
                               expect(volume_between=[2000 - fillet_loss(2, 40) - 1, 2000 - fillet_loss(2, 20) - 1])])

# ---------------------------------------------------------------- Transform, pattern, align
task("xf-01-translate", "transform", "Translate a body by (5, 0, 0)",
     box() + [E("translate", by=[5, 0, 0], items=[2]), expect(bounds=[[5, 0, 0], [25, 10, 10]])])
task("xf-02-rotate", "transform", "Rotate 90° about Z",
     box() + [E("rotate", origin=[0, 0, 0], axis=[0, 0, 1], angle=90, items=[2]), expect(bounds=[[-10, 0, 0], [0, 20, 10]])])
task("xf-03-mirror-copy", "transform", "Mirror a copy across x=0",
     box() + [E("mirror", origin=[0, 0, 0], normal=[1, 0, 0], items=[2], copy=True), expect(bodies=2, bounds=[[-20, 0, 0], [20, 10, 10]])])
task("xf-04-linear-pattern", "transform", "Linear pattern of 4, spacing 30",
     box() + [E("pattern", direction=[1, 0, 0], spacing=30, count=4, items=[2]), expect(bodies=4, volume=8000)])
task("xf-05-circular-pattern", "transform", "Circular pattern of 6 around Z",
     box(4, 4, 4, at=(20, 0)) + [E("pattern_circular", origin=[0, 0, 0], axis=[0, 0, 1], count=6, angle=360, items=[2]),
                                 expect(bodies=6, volume=6 * 64)])
task("xf-06-scale", "transform", "Scale 2× about the origin",
     box() + [E("scale", factor=2, center=[0, 0, 0], items=[2]), expect(volume=16000, bounds=[[0, 0, 0], [40, 20, 20]])])
task("xf-07-scale-nonuniform", "transform", "Scale only along X by 3",
     box() + [E("scale", factors=[3, 1, 1], center=[0, 0, 0], items=[2]), expect(volume=6000)])
task("xf-08-align-face", "transform", "Align a box's bottom onto another box's top",
     box() + box(5, 5, 5, sketch=3, at=(40, 0)) +
     [E("align", **{"from": {"body": 4, "face": 4}, "to": {"body": 2, "face": 5}}), expect(bounds=[[0, 0, 0], [20, 10, 15]])])
task("xf-12-unlinked-copy", "transform", "Move with Copy and the Link badge off: an Unlinked Copy that keeps its shape",
     box() + [E("translate", by=[25, 0, 0], items=[2], copy=True, linked=False), E("edit_step", step=1, params={"distance": 4}),
              expect(bodies=2, volume=2000 + 800)])
task("xf-10-move-copies", "transform", "Translate with 3 copies",
     box() + [E("translate", by=[25, 0, 0], items=[2], copy=True, copies=3), expect(bodies=4)])

# ---------------------------------------------------------------- Construction geometry
task("con-01-offset-plane-sketch", "construction", "Offset plane 15 up, sketch on it, extrude",
     [E("offset_plane", **{"from": "xy"}, distance=15), E("sketch", construction=1), E("rectangle", corner=[0, 0], opposite=[10, 10]),
      E("extrude", sketch=2, region=0, distance=5), expect(bounds=[[0, 0, 15], [10, 10, 20]])])
task("con-02-midplane-mirror", "construction", "Midplane between two faces",
     box() + [E("midplane", a={"body": 2, "face": 0}, b={"body": 2, "face": 2}), expect(bodies=1)])
task("con-03-plane-3points", "construction", "Plane through 3 points, sketch on it",
     [E("plane_3points", points=[[0, 0, 5], [10, 0, 5], [0, 10, 5]]), E("sketch", construction=1), E("circle", center=[0, 0], radius=1),
      E("extrude", sketch=2, region=0, distance=1), expect(bodies=1, volume_between=[PI - 0.01, PI + 0.01])])
task("con-04-axis-revolve", "construction", "Axis along an edge; revolve a profile around that edge",
     box() + [E("axis_along_edge", body=2, edge=1), E("sketch", plane="xz"), E("rectangle", corner=[30, 0], opposite=[32, 10]),
              E("revolve", sketch=4, region=0, axis={"body": 2, "edge": 1}, angle=360, result="new"),
              expect(bodies=2, volume=2000 + PI * (32 ** 2 - 30 ** 2) * 10)])
# con-05 (construction point) retired 2026-11-02: the reference (26.200) has no construction points (its menus, s24 dump).

# ---------------------------------------------------------------- Selection & inspection
task("sel-01-select-all", "selection", "⌘A selects every body",
     box() + box(5, 5, 5, sketch=3, at=(30, 0)) + [{"key": "A", "mods": "cmd"}, {"wait": 0.2}, expect(selection=2)])
task("sel-02-measure-faces", "selection", "Measure two parallel faces: 20 apart",
     box() + [C("engine.execute", command="measure", params={"picks": [{"kind": "face", "body": 2, "face": 0}, {"kind": "face", "body": 2, "face": 2}]})
              | {"returns": {"at": "/distance/mm", "near": 20}}])
task("sel-03-measure-volume", "selection", "Measure a body: its volume",
     box() + [C("engine.execute", command="measure", params={"picks": [{"kind": "body", "body": 2}]})
              | {"returns": {"at": "/items/0/volume_mm3", "near": 2000}}])
task("sel-04-click-deselect", "selection", "Click a face, then empty space: nothing stays selected",
     box() + default_view() + [{"click": [10, 5, 10]}, {"wait": 0.2}, {"key": "Escape"}, {"click": [1300, 1150]}, {"wait": 0.2},
                               expect(selection=0)])
task("sel-05-selection-filter", "selection", "Selection filter Bodies only: clicking a face picks its body",
     box() + default_view() + [C("ui.selection_filter", only="bodies"), {"click": [10, 5, 10]}, {"wait": 0.2},
                               C("ui.state") | {"returns": {"at": "/selection/0/kind", "is": "body"}}])
task("sel-06-interference", "selection", "Interference check finds overlapping bodies",
     box() + box(20, 10, 10, sketch=3, at=(10, 0)) + [C("engine.execute", command="measure", params={"picks": [{"kind": "body", "body": 2}, {"kind": "body", "body": 4}]})
                                                       | {"returns": {"at": "/interference", "min": 1}}])

# ---------------------------------------------------------------- History & parametrics (the torture section)
FL = fillet_loss(2, 20)
task("hist-01-edit-extrude-keeps-fillet", "history", "Fillet the top front edge, then make the box taller: the fillet stays on that edge",
     box() + default_view() + [{"click": [10, 0, 10]}, {"wait": 0.3}, {"text": "2"}, {"key": "Enter"}, {"wait": 0.3},
                               expect(volume=2000 - FL), E("edit_step", step=1, params={"distance": 15}),
                               expect(volume=3000 - FL, bounds=[[0, 0, 0], [20, 10, 15]], failed_steps=0)])
task("hist-02-edit-under-hole", "history", "Hole sketched on the top face; the box gets taller: the hole follows the top",
     box() + [E("sketch", body=2, face="@face:2:10,5,10"), E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-5),
              expect(volume=2000 - PI * 20), E("edit_step", step=1, params={"distance": 15}),
              expect(volume=3000 - PI * 20, failed_steps=0)])
task("hist-03-pushpull-after-renumber", "history", "Cut a side hole, push the top face up, then edit the box: the push stays on the top",
     box() + [E("sketch", body=2, face="@face:2:10,0,5"), E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-3),
              E("extrude", body=2, face="@face:2:10,5,10", distance=5)] +
     [C("engine.execute", command="inspect") | {"returns": {"at": "/bodies/0/bounds/max/2", "near": 15, "tol": 0.01}},
      E("edit_step", step=1, params={"distance": 12}), expect(bounds=[[0, 0, 0], [20, 10, 17]], failed_steps=0)])
task("hist-04-suppress", "history", "Suppress a fillet and bring it back",
     box() + [E("fillet", body=2, edges=[11], radius=2), E("suppress_step", step=2, suppressed=True), expect(volume=2000),
              E("suppress_step", step=2, suppressed=False), expect(volume=2000 - FL, failed_steps=0)])
task("hist-05-delete-middle", "history", "Push the top up twice, delete the first push: the second still pushes the top",
     box() + [E("extrude", body=2, face="@face:2:10,5,10", distance=5), E("extrude", body=2, face="@face:2:10,5,15", distance=5),
              E("delete_step", step=2), expect(volume=3000, bounds=[[0, 0, 0], [20, 10, 15]], failed_steps=0)])
task("hist-06-variable", "history", "Extrude by a variable, then change it",
     [E("set_variable", name="h", value="10 mm"), E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]),
      E("extrude", sketch=1, region=0, distance="h"), expect(volume=2000), E("set_variable", name="h", value="25 mm"),
      expect(volume=5000)])
task("hist-07-breakpoint", "history", "A breakpoint before the fillet rolls it back; removing it restores",
     box() + [E("fillet", body=2, edges=[11], radius=2), E("set_breakpoint", before=2), expect(volume=2000),
              E("set_breakpoint"), expect(volume=2000 - FL)])
task("hist-08-undo-redo-chain", "history", "Five steps, undo three, redo two",
     box() + [E("extrude", body=2, face="@face:2:10,5,10", distance=1), E("extrude", body=2, face="@face:2:10,5,11", distance=1),
              E("extrude", body=2, face="@face:2:10,5,12", distance=1),
              {"key": "Z", "mods": "cmd"}, {"key": "Z", "mods": "cmd"}, {"key": "Z", "mods": "cmd"}, expect(volume=2000),
              {"key": "Z", "mods": "cmd+shift"}, {"key": "Z", "mods": "cmd+shift"}, expect(volume=2400)])
task("hist-09-edit-sketch-past", "history", "Edit the first sketch after a fillet: widen the rectangle, the fillet survives",
     box() + [E("fillet", body=2, edges=[11], radius=2), E("edit_sketch_step", step=0),
              E("set_entity", sketch=1, entities=[[0, {"type": "line", "a": [0, 0], "b": [30, 0]}], [1, {"type": "line", "a": [30, 0], "b": [30, 10]}],
                                                  [2, {"type": "line", "a": [30, 10], "b": [0, 10]}]]),
              E("exit_sketching"), expect(bodies=1, failed_steps=0, volume=3000 - fillet_loss(2, 30))])
task("hist-10-pattern-count", "history", "Edit a pattern's count from 3 to 5",
     box() + [E("pattern", direction=[1, 0, 0], spacing=30, count=3, items=[2]), expect(bodies=3),
              E("edit_step", step=2, params={"count": 5}), expect(bodies=5)])
task("hist-11-edit-breaks-downstream", "history", "Make the box too short for its 4 mm fillet: the edit applies, the fillet is marked",
     box() + [E("fillet", body=2, edges=[11], radius=4), E("edit_step", step=1, params={"distance": 3}),
              expect(bounds=[[0, 0, 0], [20, 10, 3]], failed_steps=1)])
task("hist-12-long-chain", "history", "Eight steps; editing the first replays them all",
     box() + [E("sketch", body=2, face="@face:2:10,5,10"), E("circle", center=[0, 5], radius=1), E("extrude", sketch=3, region=0, distance=-4),
              E("sketch", body=2, face="@face:2:10,5,10"), E("circle", center=[0, -5], radius=1), E("extrude", sketch=4, region=0, distance=-4),
              E("fillet", body=2, edges=["@edge:2:0,0,5"], radius=1), E("shell", body=2, faces=["@face:2:10,5,0"], thickness=0.5),
              E("edit_step", step=1, params={"distance": 14}), expect(bodies=1, failed_steps=0, bounds=[[0, 0, 0], [20, 10, 14]])])
task("hist-13-rename-and-merge", "history", "Rename a step, then Merge Modeling Steps",
     box() + [E("rename_step", step=1, name="Base"), expect(history=["Sketch", "Base"]), E("merge_steps", keep_sketches=False, keep_variables=True),
              expect(history=1, volume=2000)])
task("hist-14-step-card-ui", "history", "Open the extrude's step card from History, click Distance, type a new value",
     box() + [C("ui.history_step", step=1), {"wait": 0.5}, {"double_click": [242, 1162]}, {"key": "A", "mods": "cmd"}, {"text": "30"},
              {"key": "Enter"}, {"wait": 0.5}, expect(volume=6000)])
task("hist-15-edit-fillet-radius-after-shell", "history", "Fillet, shell, then change the fillet radius",
     box() + [E("fillet", body=2, edges=[11], radius=1), E("shell", body=2, faces=[4], thickness=1),
              E("edit_step", step=2, params={"radius": 2}), expect(bodies=1, failed_steps=0)])
task("hist-16-delete-sketch-used", "history", "Delete the step a later feature needs: refused or the later step marked",
     box() + [E("fillet", body=2, edges=[11], radius=1), E("delete_step", step=1), expect(failed_steps=1)], allow_errors=True)
task("hist-17-chamfer-after-edit-width", "history", "Chamfer the top right edge, then widen the box via its sketch: chamfer moves with the edge",
     box() + [E("chamfer", body=2, edges=[9], distance=1), E("edit_sketch_step", step=0),
              E("set_entity", sketch=1, entities=[[0, {"type": "line", "a": [0, 0], "b": [30, 0]}], [1, {"type": "line", "a": [30, 0], "b": [30, 10]}],
                                                  [2, {"type": "line", "a": [30, 10], "b": [0, 10]}]]),
              E("exit_sketching"), expect(failed_steps=0, volume=3000 - 0.5 * 10)])

# ---------------------------------------------------------------- Items & organization
task("items-01-rename", "items", "Rename a body", box() + [E("rename", item=2, name="Bracket"),
                                                         C("engine.execute", command="inspect") | {"returns": {"at": "/bodies/0/name", "is": "Bracket"}}])
task("items-02-hide-show", "items", "Hide a body, then Show All",
     box() + [E("show_hide", items=[2], visible=False), expect(visible_bodies=0), E("show_all"), expect(visible_bodies=1)])
task("items-03-folder", "items", "Put two bodies in a folder",
     box() + box(5, 5, 5, sketch=3, at=(30, 0)) + [E("new_folder", name="Parts", items=[2, 4]), expect(bodies=2)])
task("items-04-delete", "items", "Delete a body", box() + [E("select", picks=[{"kind": "body", "body": 2}]), {"key": "Backspace"}, expect(bodies=0)])
task("items-05-nested-folder", "items", "A folder inside a folder",
     box() + [E("new_folder", name="A", items=[2]), E("new_folder", name="B", parent=4), expect(bodies=1)])

# ---------------------------------------------------------------- Viewport & display
task("view-01-standard-views", "viewport", "Top view looks straight down",
     box() + [C("ui.view", view="top"), {"wait": 1.0}, C("ui.state") | {"returns": {"at": "/camera/pitch", "near": PI / 2, "tol": 0.01}}])
task("view-02-front", "viewport", "Front view: level camera",
     box() + [C("ui.view", view="front"), {"wait": 1.0}, C("ui.state") | {"returns": {"at": "/camera/pitch", "near": 0, "tol": 0.01}}])
task("view-03-section", "viewport", "Preferences values can be changed (placeholder until Section View has a control)", box() + [C("ui.setting", key="theme", value="Dark"), {"wait": 0.3}, expect(bodies=1)])

# ---------------------------------------------------------------- Visualization
task("vis-01-appearance", "visualization", "Aluminum appearance on a body",
     box() + [E("set_appearance", items=[2], appearance="Aluminum"), C("ui.toggle", panel="visualization"), {"wait": 0.3}, expect(bodies=1)])
task("vis-02-face-color", "visualization", "Colour one face", box() + [E("set_face_color", body=2, faces=[5], color=[1, 0, 0]), expect(bodies=1)])
task("vis-03-decal", "visualization", "A picture as a decal on the top face",
     box() + [C("ui.capture", path="$TMP/decal.png"), E("import_decal", path="$TMP/decal.png") | {"save": {"name": "img", "at": "/image"}},
              E("place_decal", image="$img", body=2, face=5, point=[10, 5, 10]), expect(bodies=1)])

# ---------------------------------------------------------------- Drawings
task("draw-01-create-export", "drawings", "Create a drawing and export it as PDF",
     box() + [E("create_drawing", bodies=[2]), E("export_drawing", path="$TMP/d.pdf"), expect(file="$TMP/d.pdf")])
task("draw-02-dimension", "drawings", "Put a distance dimension on the front view",
     box() + [E("create_drawing", bodies=[2]), E("add_drawing_view", drawing=3, kind="base", orientation="front", at=[150, 150]) | {"save": {"name": "v", "at": "/view"}},
              E("annotate", drawing=3, annotation={"kind": "distance", "view": "$v", "a": [0, 0], "b": [20, 0], "offset": 5}),
              E("export_drawing", drawing=3, path="$TMP/d.dxf"), expect(file="$TMP/d.dxf")])
task("draw-03-section-view", "drawings", "A section view from a cut line",
     box() + [E("create_drawing", bodies=[2]) | {"save": {"name": "dr", "at": "/drawing"}}, E("add_drawing_view", drawing="$dr", kind="base", orientation="front", at=[150, 150]) | {"save": {"name": "v", "at": "/view"}},
              E("add_drawing_view", drawing="$dr", kind="section", parent="$v", a=[-15, 5], b=[15, 5], at=[300, 150]), expect(bodies=1)])
task("draw-04-dwg", "drawings", "Export DWG", box() + [E("create_drawing", bodies=[2]), E("export_drawing", drawing=3, path="$TMP/d.dwg"), expect(file="$TMP/d.dwg")])

# ---------------------------------------------------------------- Import & export
for fmt in ["step", "iges", "stl"]:
    task(f"io-{fmt}-roundtrip", "io", f"Export {fmt.upper()}, import it into a new project: same volume",
         box() + [E(f"export_{fmt}", path=f"$TMP/b.{fmt}"), expect(file=f"$TMP/b.{fmt}"), C("project.new"),
                  E("import_file", path=f"$TMP/b.{fmt}"), expect(bodies=1, volume=[2000, 0.01])])
task("io-obj-3mf-glb", "io", "Export OBJ, 3MF, GLB and USDZ",
     box() + [E("export_obj", path="$TMP/b.obj"), E("export_3mf", path="$TMP/b.3mf"), E("export_glb", path="$TMP/b.glb"),
              E("export_usdz", path="$TMP/b.usdz"), expect(file="$TMP/b.3mf"), expect(file="$TMP/b.glb"), expect(file="$TMP/b.usdz")])
task("io-dxf-sketch", "io", "Export a sketch as DXF and import it back",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 10]), E("circle", center=[30, 0], radius=3),
      E("export_sketch", path="$TMP/s.dxf"), C("project.new"), E("import_file", path="$TMP/s.dxf"), expect(sketches=1, regions=2)])
task("io-shapr-roundtrip", "io", "Export .shapr, import it into a new project: same body, and its Fillet comes back as an editable step",
     box() + [E("fillet", body=2, edges=[11], radius=2), E("export_shapr", path="$TMP/b.shapr"), C("project.new"),
              E("import_file", path="$TMP/b.shapr"), {"wait": 0.5}, expect(bodies=1, volume=2000 - FL), expect(history_has=["Fillet"])])
task("io-parasolid", "io", "Export Parasolid", box() + [E("export_parasolid", path="$TMP/b.x_b"), expect(file="$TMP/b.x_b")])

# ---------------------------------------------------------------- Projects, preferences, shortcuts
task("proj-01-save-reopen", "projects", "Save, go home, open again",
     box() + [E("save", path="$TMP/p1"), C("project.home"), C("project.open", path="$TMP/p1"), {"wait": 0.5}, expect(bodies=1, volume=2000)])
task("proj-02-rename", "projects", "Rename the project", box() + [E("rename_project", name="Widget"),
                                                                  C("engine.execute", command="inspect") | {"returns": {"at": "/name", "is": "Widget"}}])
task("proj-03-units", "projects", "Switch units to inches", box() + [E("set_units", units="Inch"),
                                                                     C("engine.execute", command="inspect") | {"returns": {"at": "/units", "is": "Inch"}}])
task("proj-04-search", "projects", "⌘F opens command search; typing 'fil' and Return runs Fillet",
     box() + [E("select", picks=[{"kind": "edge", "body": 2, "edge": "@edge:2:10,0,10"}]), {"key": "F", "mods": "cmd"}, {"text": "Fillet"}, {"key": "Enter"},
              {"wait": 0.3}, {"text": "1"}, {"key": "Enter"}, {"wait": 0.3}, expect(volume=2000 - fillet_loss(1, 20))])
task("proj-05-theme", "projects", "Light theme", [C("ui.theme", theme="Light"), C("ui.state") | {"returns": {"at": "/screen", "is": "workspace"}}])

# ---------------------------------------------------------------- Second wave (2026-11-01): harder, and what we lack
TOPF = "@face:2:10,5,10"
HOLE = PI * 4
# History torture: picks recorded by number must survive the model being renumbered by edits upstream.
task("hist-18-edit-fillet-radius-after-hole", "history", "Fillet the top front edge, cut a hole in the top, then change the fillet to 3",
     box() + [E("fillet", body=2, edges=["@edge:2:10,0,10"], radius=2), E("sketch", body=2, face=TOPF), E("circle", center=[0, 0], radius=2),
              E("extrude", sketch=3, region=0, distance=-10), E("edit_step", step=2, params={"radius": 3}),
              expect(failed_steps=0, volume=2000 - fillet_loss(3, 20) - HOLE * 10)])
task("hist-19-edit-hole-radius", "history", "Edit the hole's sketch (circle 2 → 3) under a later fillet",
     box() + [E("sketch", body=2, face=TOPF), E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-10),
              E("fillet", body=2, edges=["@edge:2:10,0,10"], radius=1), E("edit_sketch_step", step=2),
              E("set_entity", sketch=3, entity=0, value={"type": "circle", "center": [0, 0], "radius": 3}), E("exit_sketching"),
              expect(failed_steps=0, volume=2000 - PI * 9 * 10 - fillet_loss(1, 20))])
task("hist-20-suppress-hole", "history", "Suppress the hole: the fillet after it stays on its edge",
     box() + [E("sketch", body=2, face=TOPF), E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-10),
              E("fillet", body=2, edges=["@edge:2:10,0,10"], radius=2), E("suppress_step", step=3, suppressed=True),
              expect(failed_steps=0, volume=2000 - FL)])
task("hist-21-two-bodies", "history", "Editing one body's extrude leaves the other body's fillet alone",
     box() + box(10, 10, 10, sketch=3, at=(40, 0)) + [E("fillet", body=4, edges=["@edge:4:45,0,10"], radius=2),
                                                     E("edit_step", step=1, params={"distance": 20}),
                                                     expect(failed_steps=0, volumes=[4000, 1000 - fillet_loss(2, 10)])])
# (Top-face sketches read like the Top view since 2026-11-01: u along +X.)
task("hist-22-delete-first-hole", "history", "Two holes in the top; delete the first: the second stays where it was",
     box() + [E("sketch", body=2, face=TOPF), E("circle", center=[5, 0], radius=1), E("extrude", sketch=3, region=0, distance=-10),
              E("sketch", body=2, face=TOPF), E("circle", center=[-5, 0], radius=1), E("extrude", sketch=4, region=0, distance=-10),
              E("delete_step", step=3), expect(failed_steps=0, volume=2000 - PI * 10)])
task("hist-23-undo-edit", "history", "Undo an edited step: the model goes back", box() + [E("edit_step", step=1, params={"distance": 30}),
                                                                                       expect(volume=6000), {"key": "Z", "mods": "cmd"}, expect(volume=2000)])
task("hist-24-region-after-sketch-edit", "history", "Extrude the ring around a circle, then move the circle: still the ring",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[20, 20]), E("circle", center=[10, 10], radius=3),
      E("extrude", sketch=1, region=0, distance=5), expect(volume=(400 - PI * 9) * 5), E("edit_sketch_step", step=0),
      E("set_entity", sketch=1, entity=4, value={"type": "circle", "center": [6, 6], "radius": 3}), E("exit_sketching"),
      expect(failed_steps=0, volume=(400 - PI * 9) * 5)])
task("hist-25-reopen-then-edit", "history", "Save, reopen, then edit the first extrude: the fillet replays",
     box() + [E("fillet", body=2, edges=["@edge:2:10,0,10"], radius=2), E("save", path="$TMP/h25"), C("project.home"),
              C("project.open", path="$TMP/h25"), {"wait": 0.3}, E("edit_step", step=1, params={"distance": 15}),
              expect(failed_steps=0, volume=3000 - FL)])
task("hist-26-breakpoint-insert", "history", "Breakpoint before the fillet, add a hole there, remove it: the fillet is still on its edge",
     box() + [E("fillet", body=2, edges=["@edge:2:10,0,10"], radius=2), E("set_breakpoint", before=2), E("sketch", body=2, face=TOPF),
              E("circle", center=[0, 0], radius=2), E("extrude", sketch=3, region=0, distance=-10), E("set_breakpoint"),
              expect(failed_steps=0, volume=2000 - FL - HOLE * 10)])
task("hist-27-offset-face-after-widen", "history", "Offset the right face out 3, then widen the sketch: the offset stays on the right face",
     box() + [E("offset_face", body=2, faces=["@face:2:20,5,5"], distance=3), E("edit_sketch_step", step=0),
              E("set_entity", sketch=1, entities=[[0, {"type": "line", "a": [0, 0], "b": [30, 0]}], [1, {"type": "line", "a": [30, 0], "b": [30, 10]}],
                                                  [2, {"type": "line", "a": [30, 10], "b": [0, 10]}]]), E("exit_sketching"),
              expect(failed_steps=0, bounds=[[0, 0, 0], [33, 10, 10]])])
task("hist-28-chamfer-then-taller", "history", "Chamfer the top front edge by clicking it, then make the box taller",
     box() + default_view() + [{"click": [10, 0, 10]}, {"wait": 0.3}, C("ui.tool", tool="none"), E("chamfer", distance=2),
                               E("edit_step", step=1, params={"distance": 14}), expect(failed_steps=0, volume=2800 - 40)])
task("hist-29-shell-after-push", "history", "Shell the top open, then push the box taller: the open face is still the top",
     box() + [E("shell", body=2, faces=[TOPF], thickness=1), E("edit_step", step=1, params={"distance": 15}),
              expect(failed_steps=0, volume=3000 - 18 * 8 * 14)])
task("hist-30-variable-chain", "history", "Width and depth variables drive two steps; change both",
     [E("set_variable", name="h", value="10 mm"), E("set_variable", name="r", value="h / 5")] + box() +
     [E("edit_step", step=1, params={"distance": "h"}), E("fillet", body=2, edges=["@edge:2:10,0,10"], radius="r"),
      E("set_variable", name="h", value="20 mm"), expect(failed_steps=0, volume=4000 - fillet_loss(4, 20))])

# Sketching by input
task("sketch-16-circle-two-clicks", "sketch", "C, click the center, click a rim point 5 away: a Ø10 circle",
     [C("ui.view", view="top"), {"wait": 0.8}, {"key": "C"}, {"click": [0, 0, 0]}, {"click": [5, 0, 0]}, {"wait": 0.3}, {"key": "Escape"},
      E("exit_sketching"), E("extrude", sketch=1, region=0, distance=1), expect(volume=PI * 25)])
task("sketch-17-line-typed-length", "sketch", "L, click, type 8 Enter: an 8 mm line",
     [C("ui.view", view="top"), {"wait": 0.8}, {"key": "L"}, {"click": [0, 0, 0]}, {"hover": [5, 0.2, 0]}, {"text": "8"}, {"key": "Enter"},
      {"wait": 0.3}, C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/entities/0/b/0", "near": 8, "tol": 0.3}}])
task("sketch-18-undo-in-sketch", "sketch", "Draw two rectangles, ⌘Z removes the second",
     [E("sketch", plane="xy"), E("rectangle", corner=[0, 0], opposite=[5, 5]), E("rectangle", corner=[10, 0], opposite=[15, 5]),
      {"key": "Z", "mods": "cmd"}, expect(regions=1)])
task("sketch-19-change-sketch-plane", "sketch", "Move a sketch to another plane (Change Sketch Plane)",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=2), E("exit_sketching"), E("change_sketch_plane", sketch=1, plane="xz"),
      expect(sketches=1)])
task("sketch-20-spline-break", "sketch", "Break a spline at a point", [E("sketch", plane="xy"), E("spline", points=[[0, 0], [5, 5], [10, 0]]),
                                                                     E("break_curve", entity=0, at=[5, 5])])
task("sketch-21-text-font", "sketch", "Text in a named font", [E("sketch", plane="xy"), E("text", at=[0, 0], text="A", size=10, font="Helvetica"),
                                                              expect(regions=2)])
task("sketch-22-face-sketch-axes", "sketch", "A sketch on a top face lines up with the Top view (u along +X, v along +Y)",
     box() + [E("sketch", body=2, face=TOPF), C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/1/plane/x/0", "near": 1, "tol": 1e-6}}])

# Constraints
task("cons-11-linked-pattern", "constraints", "A sketch pattern stays linked: making the source circle bigger makes the copies bigger",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=1), E("select", picks=[{"kind": "sketch_entity", "sketch": 1, "entity": 0}]),
      E("pattern", direction=[1, 0, 0], spacing=5, count=3), expect(regions=3),
      E("set_entity", sketch=1, entity=0, value={"type": "circle", "center": [0, 0], "radius": 2}),
      E("extrude", sketch=1, regions=[0, 1, 2], distance=1), expect(volume=3 * PI * 4)])
task("cons-12-fully-defined", "constraints", "R from the origin with both sizes typed: the rectangle is fully defined (green)",
     ui_rect(20, 10) + [C("engine.execute", command="inspect") | {"returns": {"at": "/sketches/0/defined", "is": True}}])

# Modeling breadth from the manual
task("model-25-fillet-setback", "modeling", "Fillet with Setback corners", box() + [E("fillet", body=2, edges=[3, 6, 9, 11], radius=2, setback=True), expect(bodies=1)])
task("model-26-offset-face-radius", "modeling", "Offset Face 'Radius' distance type on a cylinder",
     [E("sketch", plane="xy"), E("circle", center=[0, 0], radius=5), E("extrude", sketch=1, region=0, distance=10),
      E("offset_face", body=2, faces=["@face:2:0,0,5"], distance=6, distance_type="radius"), expect(volume=PI * 36 * 10)])
task("model-27-split-by-sketch", "modeling", "Split a body with a sketch line",
     box() + [E("sketch", plane="xy"), E("line", a=[10, -5], b=[10, 15]), E("split_body", **{"with": [{"sketch": 3, "entity": 0}]}, items=[2]),
              expect(bodies=2, volumes=[1000, 1000])])
task("model-28-sweep-twist", "modeling", "Sweep a square along a line with 90° twist",
     [E("sketch", plane="xy"), E("rectangle", center=[0, 0], size=[4, 4]), E("sketch", plane="xz"), E("line", a=[0, 0], b=[0, 30]),
      E("sweep", profile={"sketch": 1, "region": 0}, path={"sketch": 2}, twist=90), expect(bodies=1, volume=[480, 0.01])])
task("model-29-revolve-ui", "modeling", "Select a region, V, pick the sketch's axis line, type 180",
     [E("sketch", plane="xz"), E("rectangle", corner=[2, 0], opposite=[6, 10]), E("line", a=[0, 0], b=[0, 10]), E("exit_sketching"),
      E("select", picks=[{"kind": "region", "sketch": 1, "region": 0}]), {"key": "V"}, {"wait": 0.3},
      E("revolve", sketch=1, region=0, axis={"line": 4}, angle=180), expect(bodies=1, volume=PI / 2 * (36 - 4) * 10)])
task("model-30-chordal-fillet", "modeling", "Chordal fillet (constant width)", box() + [E("fillet", body=2, edges=[11], radius=2, mode="chordal"), expect(bodies=1)])

# Transform / construction / selection breadth
task("xf-11-move-copy-count", "transform", "Copy count in the Move gizmo (Copy ×3)",
     box() + [E("select", picks=[{"kind": "body", "body": 2}]), {"key": "M"}, E("translate", by=[25, 0, 0], items=[2], copy=True, copies=3),
              expect(bodies=4)])
task("con-06-plane-resize", "construction", "Resize a construction plane", [E("offset_plane", **{"from": "xy"}, distance=5), E("plane_size", plane=1, size=80)])
task("sel-07-tab-cycle", "selection", "Tab cycles what's under the cursor (face → edge → body)",
     box() + default_view() + [{"hover": [10, 5, 10]}, {"key": "Tab"}, {"key": "Tab"}, {"click": [10, 5, 10]}, expect(selection=1)])

# Items, viewport, visualization, drawings, io, projects
task("items-06-reorder", "items", "Drag a body to reorder the Items list", box() + box(5, 5, 5, sketch=3, at=(30, 0)) + [E("place_item", item=4, folder=None, before=2)])
task("vis-04-roughness", "visualization", "Set a material's roughness to 0.8",
     box() + [E("set_appearance", items=[2], appearance={"name": "Matte", "color": [0.5, 0.5, 0.5], "metal": 0, "roughness": 0.8}),
              C("engine.execute", command="inspect") | {"returns": {"at": "/bodies/0/appearance/gloss", "near": 0.2, "tol": 1e-6}}])
task("vis-05-panorama", "visualization", "Import a panorama environment", box() + [C("ui.capture", path="$TMP/env.png"), E("import_environment", path="$TMP/env.png")])
task("draw-05-update", "drawings", "Drawing follows a model edit (Update)", box() + [E("create_drawing", bodies=[2]) | {"save": {"name": "dr", "at": "/drawing"}},
                                                                                 E("edit_step", step=1, params={"distance": 20}), E("update_drawing", drawing="$dr"),
                                                                                 E("export_drawing", drawing="$dr", path="$TMP/u.svg"), expect(file="$TMP/u.svg")])
task("io-x_t-import", "io", "Import Parasolid text (x_t)", box() + [E("export_parasolid", path="$TMP/b.x_t"), C("project.new"), E("import_file", path="$TMP/b.x_t"),
                                                                    expect(bodies=1, volume=[2000, 0.01])])
task("io-import-options", "io", "Import an STL whose numbers are inches (import options: Unit)", box() + [E("export_stl", path="$TMP/o.stl"), C("project.new"),
                                                                             E("import_file", path="$TMP/o.stl", units="in"), expect(bodies=1, volume=[2000 * 25.4 ** 3, 0.01])])
task("proj-06-language", "projects", "Switch the language", [C("ui.setting", key="language", value="Deutsch")])
task("proj-07-recents", "projects", "Projects screen lists a saved project", box() + [C("project.save"), C("project.home"), C("project.list") | {"returns": {"at": "/projects", "min": 1}}])


# Coverage for the smaller categories (5–6 tasks each made one gap swing them by 17%): reference features from the
# manual, the ones we lack included.
PL = lambda name, **kw: {"save": {"name": name, "at": "/plane"}}
task("con-07-plane-through-edge", "construction", "Plane through an edge at 45°, sketch on it",
     box() + [E("plane_through_edge", body=2, edge="@edge:2:10,0,10", angle=45) | PL("p"), E("sketch", construction="$p") | {"save": {"name": "sk", "at": "/sketch"}},
              E("circle", center=[0, 0], radius=1), E("extrude", sketch="$sk", region=0, distance=1, result="new"), expect(bodies=2)])
task("con-08-plane-on-curve", "construction", "Plane on a curve at its middle, perpendicular to it",
     box() + [E("plane_on_curve", body=2, edge="@edge:2:10,0,10", at=0.5) | PL("p"), E("sketch", construction="$p"),
              E("circle", center=[0, 0], radius=1), expect(sketches=2)])
task("con-09-axis-2planes-rotate", "construction", "Axis where XY and XZ meet (the X axis), rotate a body 90° around it",
     box() + [E("axis_2planes", a="xy", b="xz") | {"save": {"name": "ax", "at": "/axis"}},
              E("rotate", about={"axis": "$ax"}, angle=90, items=[2]), expect(bounds=[[0, -10, 0], [20, 0, 10]])])
task("con-10-plane-perp-edge", "construction", "Plane perpendicular to an edge at a point",
     box() + [E("plane_perp_edge", body=2, edge="@edge:2:10,0,10", point=[5, 0, 10]) | PL("p"), E("sketch", construction="$p"), expect(sketches=2)])
task("con-11-plane-parallel-face", "construction", "Plane parallel to the top face through a point 5 above it",
     box() + [E("plane_parallel_face", **{"from": {"body": 2, "face": "@face:2:10,5,10"}}, point=[0, 0, 15]) | PL("p"),
              E("sketch", construction="$p") | {"save": {"name": "sk", "at": "/sketch"}}, E("rectangle", corner=[0, 0], opposite=[4, 4]),
              E("extrude", sketch="$sk", region=0, distance=1, result="new"),
              expect(bodies=2, bounds=[[0, 0, 0], [20, 10, 16]])])
task("con-12-axis-perp-face", "construction", "Axis perpendicular to a face at a point, circular pattern around it",
     box(4, 4, 4, at=(10, 0)) + [E("axis_perp_face", body=2, face="@face:2:12,2,0", at=[0, 0, 0]) | {"save": {"name": "ax", "at": "/axis"}},
                                 E("pattern_circular", about={"axis": "$ax"}, count=4, angle=360, items=[2]), expect(bodies=4)])

DR = {"save": {"name": "dr", "at": "/drawing"}}
BV = lambda name: {"save": {"name": name, "at": "/view"}}
def drawing_with_front():
    return box() + [E("create_drawing", bodies=[2]) | DR, E("add_drawing_view", drawing="$dr", kind="base", orientation="front", at=[120, 180]) | BV("v")]
task("draw-06-projected", "drawings", "A projected view to the right of the front view",
     drawing_with_front() + [E("add_drawing_view", drawing="$dr", kind="projected", parent="$v", dx=1, dy=0, at=[260, 180]),
                             E("export_drawing", drawing="$dr", path="$TMP/p.svg"), expect(file="$TMP/p.svg")])
task("draw-07-detail", "drawings", "A detail view of a corner at 2×",
     drawing_with_front() + [E("add_drawing_view", drawing="$dr", kind="detail", parent="$v", center=[20, 10], radius=4, scale=2, at=[300, 80]),
                             E("export_drawing", drawing="$dr", path="$TMP/dt.pdf"), expect(file="$TMP/dt.pdf")])
task("draw-08-tolerance", "drawings", "A dimension with a ± tolerance and a prefix",
     drawing_with_front() + [E("annotate", drawing="$dr", annotation={"kind": "distance", "view": "$v", "a": [0, 0], "b": [20, 0], "offset": 5}),
                             E("update_annotation", drawing="$dr", index=0, text={"prefix": "2×", "tolerance": {"kind": "symmetrical", "upper": 0.1, "lower": 0.1}}),
                             E("export_drawing", drawing="$dr", path="$TMP/t.svg"), expect(file="$TMP/t.svg")])
task("draw-09-view-bodies", "drawings", "A base view showing only some bodies (the view's Bodies… pill)",
     box() + box(5, 5, 5, sketch=3, at=(30, 0)) + [E("create_drawing", bodies=[2, 4]) | DR,
                                                   E("add_drawing_view", drawing="$dr", kind="base", orientation="front", at=[120, 180], bodies=[2])])
task("draw-10-four-views", "drawings", "Four standard views on an A4 sheet, first-angle",
     box() + [E("create_drawing", bodies=[2], sheet="A4", four_views=True, first_angle=True) | DR, E("export_drawing", drawing="$dr", path="$TMP/f.pdf"),
              expect(file="$TMP/f.pdf")])

task("vis-06-environment", "visualization", "Pick another studio environment and turn the light",
     box() + [C("ui.toggle", panel="visualization"), E("visual_settings", environment=3, light_rotation=90, sun_elevation=40), expect(bodies=1)])
task("vis-07-custom-material", "visualization", "A custom material from a picture, on a body",
     box() + [C("ui.capture", path="$TMP/wood.png"), E("import_material", path="$TMP/wood.png") | {"save": {"name": "m", "at": "/image"}},
              E("set_appearance", items=[2], custom="$m"), C("engine.execute", command="inspect") | {"returns": {"at": "/bodies/0/appearance/texture", "is": "$m"}}])
task("vis-08-depth-of-field", "visualization", "Depth of field on a point of the model",
     box() + [E("visual_settings", aperture=2.8, blur=1.0, focus=[10, 5, 10]), expect(bodies=1)])


if __name__ == "__main__":
    # Only touch files that changed (the repo may sit on a slow network share).
    os.makedirs(OUT, exist_ok=True)
    wanted = {name + ".jsonl": "".join(json.dumps(s) + "\n" for s in steps) for name, steps in TASKS.items()}
    for old in os.listdir(OUT):
        if old.endswith(".jsonl") and old not in wanted:
            os.remove(os.path.join(OUT, old))
    changed = 0
    for file, text in wanted.items():
        path = os.path.join(OUT, file)
        if not os.path.exists(path) or open(path).read() != text:
            with open(path, "w") as f:
                f.write(text)
            changed += 1
    print(f"{len(TASKS)} tasks, {changed} written → {OUT}")

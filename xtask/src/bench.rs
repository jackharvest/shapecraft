//! Writes bench/tasks/*.jsonl, the task benchmark (see bench/README.md).
//!
//! Each task is a workflow a user of a direct modeler does, driven through the real UI offscreen where the input
//! matters (keys, clicks on model points, typed values) and through commands where only the result does, with checks
//! on the model. Expected values are worked out here from geometry, never read back from our own app. A task for
//! something we don't have yet stays in and scores 0: the benchmark measures parity, not what we built.
//!
//! Run: cargo xtask bench-tasks && shapecraft-snap --bench bench/tasks

use std::collections::BTreeMap;
use std::f64::consts::PI;

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::util::root;

type Steps = Vec<Value>;

fn e(command: &str, params: Value) -> Value {
    json!({ "control": "engine.execute", "params": { "command": command, "params": params } })
}

fn e0(command: &str) -> Value {
    e(command, json!({}))
}

fn c(method: &str, params: Value) -> Value {
    json!({ "control": method, "params": params })
}

fn c0(method: &str) -> Value {
    c(method, json!({}))
}

fn expect(v: Value) -> Value {
    json!({ "expect": v })
}

/// `a | b` for two JSON objects: b's keys over a's.
fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            a.extend(b);
            Value::Object(a)
        }
        (a, _) => a,
    }
}

fn returns(step: Value, r: Value) -> Value {
    merge(step, json!({ "returns": r }))
}

fn save(step: Value, name: &str, at: &str) -> Value {
    merge(step, json!({ "save": { "name": name, "at": at } }))
}

fn inspect() -> Value {
    c("engine.execute", json!({ "command": "inspect" }))
}

/// Steps joined: `cat![a, b, c]` for lists of steps.
macro_rules! cat {
    ($($v:expr),* $(,)?) => {{
        let mut all: Steps = Vec::new();
        $( all.extend($v); )*
        all
    }};
}

struct Tasks(BTreeMap<String, Steps>);

impl Tasks {
    fn task(&mut self, name: &str, category: &str, about: &str, steps: Steps) {
        self.task_with(name, category, about, steps, false);
    }

    fn task_with(&mut self, name: &str, category: &str, about: &str, steps: Steps, allow_errors: bool) {
        let mut meta = Map::new();
        meta.insert("category".into(), json!(category));
        meta.insert("about".into(), json!(about));
        if allow_errors {
            meta.insert("allow_errors".into(), json!(true));
        }
        let mut all = vec![json!({ "task": meta }), json!({ "size": [1840, 1324] }), c0("project.new")];
        all.extend(steps);
        self.0.insert(name.into(), all);
    }
}

/// A box by commands: sketch `sketch` (its id), body id sketch + 1.
fn box_(w: f64, d: f64, h: f64, sketch: u32, at: (f64, f64), plane: &str) -> Steps {
    vec![
        e("sketch", json!({ "plane": plane })),
        e("rectangle", json!({ "corner": [at.0, at.1], "opposite": [at.0 + w, at.1 + d] })),
        e("extrude", json!({ "sketch": sketch, "region": 0, "distance": h })),
        e0("deselect_all"),
    ]
}

fn bx() -> Steps {
    box_(20.0, 10.0, 10.0, 1, (0.0, 0.0), "xy")
}

/// R, a click on the grid (top view), the size typed: what a user does.
fn ui_rect(w: u32, d: u32) -> Steps {
    vec![
        c("ui.view", json!({ "view": "top" })),
        json!({ "wait": 0.8 }),
        json!({ "key": "R" }),
        json!({ "click": [0, 0, 0] }),
        json!({ "hover": [3, 3, 0] }),
        json!({ "text": w.to_string() }),
        json!({ "key": "Tab" }),
        json!({ "text": d.to_string() }),
        json!({ "key": "Enter" }),
        json!({ "wait": 0.3 }),
    ]
}

/// Escape the sketch tool, click the region, E, type the distance.
fn ui_extrude_region(at: Value, h: u32) -> Steps {
    vec![
        json!({ "key": "Escape" }),
        json!({ "click": at }),
        json!({ "wait": 0.3 }),
        json!({ "key": "E" }),
        json!({ "text": h.to_string() }),
        json!({ "key": "Enter" }),
        json!({ "wait": 0.3 }),
    ]
}

fn default_view() -> Steps {
    vec![e0("deselect_all"), c("ui.view", json!({ "view": "default" })), json!({ "wait": 1.0 })]
}

fn fillet_loss(r: f64, length: f64) -> f64 {
    (1.0 - PI / 4.0) * r * r * length
}

fn rounded_box(a: f64, b: f64, c: f64, r: f64) -> f64 {
    let (ia, ib, ic) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
    ia * ib * ic + 2.0 * r * (ia * ib + ib * ic + ic * ia) + PI * r * r * (ia + ib + ic) + 4.0 / 3.0 * PI * r.powi(3)
}

fn frustum(a1: f64, a2: f64, h: f64) -> f64 {
    h / 3.0 * (a1 + a2 + (a1 * a2).sqrt())
}

fn wide_sketch() -> Value {
    json!([
        [0, { "type": "line", "a": [0, 0], "b": [30, 0] }],
        [1, { "type": "line", "a": [30, 0], "b": [30, 10] }],
        [2, { "type": "line", "a": [30, 10], "b": [0, 10] }]
    ])
}

const TOPF: &str = "@face:2:10,5,10";

fn sketch_tasks(t: &mut Tasks) {
    t.task(
        "sketch-01-rect-typed",
        "sketch",
        "R, click a corner, type 12 Tab 8 Enter; click the region, E 10",
        cat![
            ui_rect(12, 8),
            [expect(json!({ "sketches": 1, "regions": 1 }))],
            ui_extrude_region(json!([3, 3, 0]), 10),
            [expect(json!({ "bodies": 1, "volume": 960 }))]
        ],
    );
    t.task(
        "sketch-02-circle-typed",
        "sketch",
        "C, click the center, type the diameter 10; extrude 10",
        cat![
            [
                c("ui.view", json!({ "view": "top" })),
                json!({ "wait": 0.8 }),
                json!({ "key": "C" }),
                json!({ "click": [0, 0, 0] }),
                json!({ "hover": [3, 3, 0] }),
                json!({ "text": "10" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                expect(json!({ "regions": 1 })),
            ],
            ui_extrude_region(json!([1, 1, 0]), 10),
            [expect(json!({ "volume": PI * 25.0 * 10.0 }))]
        ],
    );
    t.task(
        "sketch-03-line-loop",
        "sketch",
        "L, click three corners and back to the first: a closed triangle region",
        cat![
            [
                c("ui.view", json!({ "view": "top" })),
                json!({ "wait": 0.8 }),
                json!({ "key": "L" }),
                json!({ "click": [0, 0, 0] }),
                json!({ "click": [10, 0, 0] }),
                json!({ "click": [0, 8, 0] }),
                json!({ "click": [0, 0, 0] }),
                json!({ "wait": 0.3 }),
                expect(json!({ "regions": 1 })),
            ],
            ui_extrude_region(json!([2, 2, 0]), 5),
            [expect(json!({ "volume": 200 }))]
        ],
    );
    t.task(
        "sketch-04-drag-rect",
        "sketch",
        "R, press-drag-release draws a rectangle",
        vec![
            c("ui.view", json!({ "view": "top" })),
            json!({ "wait": 0.8 }),
            json!({ "key": "R" }),
            json!({ "drag": [[0, 0, 0], [10, 8, 0]] }),
            json!({ "wait": 0.3 }),
            expect(json!({ "sketches": 1, "regions": 1 })),
        ],
    );
    t.task(
        "sketch-05-polygon",
        "sketch",
        "Hexagon of radius 10, extruded 1",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("polygon", json!({ "center": [0, 0], "radius": 10, "sides": 6 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": 3.0 * 3f64.sqrt() / 2.0 * 100.0 })),
        ],
    );
    t.task(
        "sketch-06-circle-in-rect",
        "sketch",
        "A circle inside a rectangle splits into a ring and a disc",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 20] })),
            e("circle", json!({ "center": [10, 10], "radius": 5 })),
            expect(json!({ "regions": 2 })),
        ],
    );
    t.task(
        "sketch-07-trim",
        "sketch",
        "A line across a rectangle, then Trim the piece outside: two regions",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("line", json!({ "a": [10, -5], "b": [10, 15] })),
            expect(json!({ "regions": 2 })),
            e("trim", json!({ "entity": 4, "at": [10, -2] })),
            e("trim", json!({ "entity": 4, "at": [10, 12] })),
            expect(json!({ "regions": 2 })),
        ],
    );
    t.task(
        "sketch-08-sketch-fillet",
        "sketch",
        "Round every corner of a 20 × 10 rectangle by 2; extrude 1",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("sketch_fillet", json!({ "radius": 2 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": 200.0 - (4.0 - PI) * 4.0 })),
        ],
    );
    t.task(
        "sketch-09-offset-region",
        "sketch",
        "Offset a rectangle's outline 2 outwards: a ring region (corner style unobserved: round or sharp both pass)",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("offset_edge", json!({ "distance": 2, "region": 0 })),
            expect(json!({ "regions": 2 })),
            e("extrude", json!({ "sketch": 1, "regions": [0, 1], "distance": 1 })),
            expect(json!({ "volume_between": [24.0 * 14.0 - (4.0 - PI) * 4.0 - 0.01, 24.0 * 14.0 + 0.01] })),
        ],
    );
    t.task(
        "sketch-10-ellipse",
        "sketch",
        "Ellipse 10 × 5, extruded 2",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("ellipse", json!({ "center": [0, 0], "rx": 10, "ry": 5 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 2 })),
            expect(json!({ "volume": PI * 50.0 * 2.0 })),
        ],
    );
    t.task(
        "sketch-11-arc-slot",
        "sketch",
        "Two lines and two 3-point arcs make a slot region",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("line", json!({ "a": [0, 0], "b": [20, 0] })),
            e("line", json!({ "a": [0, 10], "b": [20, 10] })),
            e("arc", json!({ "a": [20, 0], "mid": [25, 5], "b": [20, 10] })),
            e("arc", json!({ "a": [0, 10], "mid": [-5, 5], "b": [0, 0] })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "regions": 1, "volume": 200.0 + PI * 25.0 })),
        ],
    );
    t.task(
        "sketch-12-text",
        "sketch",
        "Text makes regions that extrude",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("text", json!({ "at": [0, 0], "text": "HI", "size": 10 })),
            e("extrude", json!({ "sketch": 1, "regions": [0, 1], "distance": 1 })),
            expect(json!({ "bodies": 1 })),
        ],
    );
    t.task(
        "sketch-13-sketch-on-face",
        "sketch",
        "Sketch on a box's top face (click it with Sketch armed), circle, cut",
        cat![
            bx(),
            default_view(),
            [
                json!({ "key": "S", "mods": "shift" }),
                json!({ "wait": 0.2 }),
                e("sketch", json!({ "body": 2, "face": "@face:2:10,5,10" })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                expect(json!({ "bodies": 1, "volume": 2000.0 - PI * 4.0 * 10.0 })),
            ]
        ],
    );
    t.task(
        "sketch-14-spline",
        "sketch",
        "A closed fit-point spline makes a region",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("spline", json!({ "points": [[0, 0], [10, -5], [20, 0], [10, 10], [0, 0]] })),
            expect(json!({ "regions": 1 })),
        ],
    );
    t.task(
        "sketch-15-project-edges",
        "sketch",
        "Project a box's top face outline into a sketch above it",
        cat![
            bx(),
            [
                e("offset_plane", json!({ "from": "xy", "distance": 20 })),
                e("sketch", json!({ "construction": 3 })),
                e("project", json!({ "faces": [{ "body": 2, "face": 5 }] })),
                expect(json!({ "regions": 1 })),
            ]
        ],
    );
}

fn constraint_tasks(t: &mut Tasks) {
    t.task(
        "cons-01-dimension-width",
        "constraints",
        "Dimension a rectangle's bottom side to 30: the region follows",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("constrain", json!({ "kind": "distance", "entities": [0], "value": 30 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": 300 })),
        ],
    );
    t.task(
        "cons-02-horizontal",
        "constraints",
        "Horizontal on a slanted line levels it",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("line", json!({ "a": [0, 0], "b": [10, 1] })),
            e("constrain", json!({ "kind": "horizontal_vertical", "entities": [0] })),
            returns(inspect(), json!({ "at": "/sketches/0/entities/0/b/1", "same": "/sketches/0/entities/0/a/1" })),
        ],
    );
    t.task(
        "cons-03-equal-circles",
        "constraints",
        "Equal makes the second circle's radius the first's",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 5 })),
            e("circle", json!({ "center": [20, 0], "radius": 3 })),
            e("constrain", json!({ "kind": "equal", "entities": [0, 1] })),
            e("extrude", json!({ "sketch": 1, "regions": [0, 1], "distance": 1 })),
            expect(json!({ "volume": 2.0 * PI * 25.0 })),
        ],
    );
    t.task(
        "cons-04-perpendicular",
        "constraints",
        "Perpendicular turns the second line square to the first",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("line", json!({ "a": [0, 0], "b": [10, 0] })),
            e("line", json!({ "a": [0, 0], "b": [3, 10] })),
            e("constrain", json!({ "kind": "perpendicular", "entities": [0, 1] })),
            returns(inspect(), json!({ "at": "/sketches/0/entities/1/b/0", "same": "/sketches/0/entities/1/a/0" })),
        ],
    );
    t.task(
        "cons-05-radius",
        "constraints",
        "Radius dimension 8 on a circle",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 5 })),
            e("constrain", json!({ "kind": "radius", "entities": [0], "value": 8 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": PI * 64.0 })),
        ],
    );
    t.task(
        "cons-06-dim-keeps-rect",
        "constraints",
        "Changing one side keeps the rectangle a rectangle (perpendicular kept)",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("constrain", json!({ "kind": "distance", "entities": [1], "value": 15 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": 300 })),
        ],
    );
    t.task(
        "cons-07-angle",
        "constraints",
        "Angle 45° between two lines",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("line", json!({ "a": [0, 0], "b": [10, 0] })),
            e("line", json!({ "a": [0, 0], "b": [10, 3] })),
            e("constrain", json!({ "kind": "angle", "entities": [0, 1], "value": 45 })),
            returns(inspect(), json!({ "at": "/sketches/0/entities/1/b/1", "same": "/sketches/0/entities/1/b/0" })),
        ],
    );
    t.task(
        "cons-08-ui-type-dimension",
        "constraints",
        "Click a rectangle's dimension in the sketch and type a new value",
        cat![
            ui_rect(20, 10),
            [
                json!({ "key": "Escape" }),
                json!({ "click": [10, 12.5, 0] }),
                json!({ "wait": 0.2 }),
                json!({ "text": "30" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                e0("exit_sketching"),
                e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
                expect(json!({ "volume": 300 })),
            ]
        ],
    );
    t.task(
        "cons-09-concentric",
        "constraints",
        "Concentric moves the second circle onto the first's center",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 10 })),
            e("circle", json!({ "center": [1, 1], "radius": 3 })),
            e("constrain", json!({ "kind": "concentric", "entities": [0, 1] })),
            expect(json!({ "regions": 2 })),
            returns(inspect(), json!({ "at": "/sketches/0/entities/1/center/0", "near": 0, "tol": 1e-4 })),
        ],
    );
    t.task(
        "cons-10-symmetry",
        "constraints",
        "Symmetry of two points about a line",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("line", json!({ "a": [0, -10], "b": [0, 10] })),
            e("circle", json!({ "center": [-5, 0], "radius": 1 })),
            e("circle", json!({ "center": [4, 1], "radius": 1 })),
            e("constrain", json!({ "kind": "symmetry", "entities": [1, 2, 0] })),
            returns(inspect(), json!({ "at": "/sketches/0/entities/2/center/0", "neg": "/sketches/0/entities/1/center/0" })),
        ],
    );
}

fn modeling_tasks(t: &mut Tasks) {
    t.task(
        "model-01-box-ui",
        "modeling",
        "Rectangle then E: a 20 × 10 × 12 box, all by input",
        cat![
            ui_rect(20, 10),
            ui_extrude_region(json!([5, 5, 0]), 12),
            [expect(json!({ "bodies": 1, "volume": 2400, "bounds": [[0, 0, 0], [20, 10, 12]] }))]
        ],
    );
    t.task(
        "model-02-pushpull-face",
        "modeling",
        "Click the top face, type 15 (Offset Face measures the Total thickness, as observed): the box is 15 tall",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 5, 10] }),
                json!({ "wait": 0.3 }),
                json!({ "text": "15" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                expect(json!({ "bodies": 1, "volume": 3000, "bounds": [[0, 0, 0], [20, 10, 15]] })),
            ]
        ],
    );
    t.task(
        "model-03-cut-hole",
        "modeling",
        "Circle on the top face extruded -10 cuts a through hole",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": "@face:2:10,5,10" })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                expect(json!({ "bodies": 1, "volume": 2000.0 - PI * 40.0 })),
            ]
        ],
    );
    t.task(
        "model-04-fillet-edge-ui",
        "modeling",
        "Click the top front edge, type 2: a 2 mm fillet",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 0, 10] }),
                json!({ "wait": 0.3 }),
                json!({ "text": "2" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                expect(json!({ "bodies": 1, "volume": 2000.0 - fillet_loss(2.0, 20.0) })),
            ]
        ],
    );
    t.task(
        "model-05-fillet-all",
        "modeling",
        "Fillet every edge of a 20 × 10 × 10 box by 1",
        cat![
            bx(),
            [
                e("select", json!({ "picks": [{ "kind": "body", "body": 2 }] })),
                e("fillet", json!({ "radius": 1 })),
                expect(json!({ "volume": rounded_box(20.0, 10.0, 10.0, 1.0) })),
            ]
        ],
    );
    t.task(
        "model-06-chamfer-ui",
        "modeling",
        "Edge, then Chamfer from the toolbar, type 2",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 0, 10] }),
                json!({ "wait": 0.3 }),
                c("ui.tool", json!({ "tool": "none" })),
                e("chamfer", json!({ "distance": 2 })),
                expect(json!({ "volume": 2000 - 2 * 20 })),
            ]
        ],
    );
    t.task(
        "model-07-two-distance-chamfer",
        "modeling",
        "Two-distance chamfer 2 and 4 on one edge",
        cat![
            bx(),
            [
                e("chamfer", json!({ "body": 2, "edges": [11], "distance": 2, "distance2": 4 })),
                expect(json!({ "volume": 2000.0 - 0.5 * 2.0 * 4.0 * 20.0 }))
            ]
        ],
    );
    t.task(
        "model-08-shell",
        "modeling",
        "Shell the top face open with 1 mm walls",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 5, 10] }),
                c("ui.tool", json!({ "tool": "none" })),
                e("shell", json!({ "thickness": 1 })),
                expect(json!({ "bodies": 1, "volume": 2000 - 18 * 8 * 9 })),
            ]
        ],
    );
    t.task(
        "model-09-revolve",
        "modeling",
        "Revolve a 5 × 10 rectangle 5 mm off the axis: a tube",
        vec![
            e("sketch", json!({ "plane": "xz" })),
            e("rectangle", json!({ "corner": [5, 0], "opposite": [10, 10] })),
            e("revolve", json!({ "sketch": 1, "region": 0, "axis": { "origin": [0, 0, 0], "dir": [0, 0, 1] }, "angle": 360 })),
            expect(json!({ "bodies": 1, "volume": PI * (100.0 - 25.0) * 10.0 })),
        ],
    );
    t.task(
        "model-10-loft",
        "modeling",
        "Loft a 20 square at z=0 to a 10 square at z=10",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "center": [0, 0], "size": [20, 20] })),
            e("offset_plane", json!({ "from": "xy", "distance": 10 })),
            e("sketch", json!({ "construction": 2 })),
            e("rectangle", json!({ "center": [0, 0], "size": [10, 10] })),
            e("loft", json!({ "profiles": [{ "sketch": 1, "region": 0 }, { "sketch": 3, "region": 0 }] })),
            expect(json!({ "bodies": 1, "volume": frustum(400.0, 100.0, 10.0) })),
        ],
    );
    t.task(
        "model-11-sweep",
        "modeling",
        "Sweep a circle along a straight path: a cylinder",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 2 })),
            e("sketch", json!({ "plane": "xz" })),
            e("line", json!({ "a": [0, 0], "b": [0, 30] })),
            e("sweep", json!({ "profile": { "sketch": 1, "region": 0 }, "path": { "sketch": 2 } })),
            expect(json!({ "bodies": 1, "volume": PI * 4.0 * 30.0 })),
        ],
    );
    let top = 20.0 - 10.0 * 10f64.to_radians().tan();
    t.task(
        "model-12-draft",
        "modeling",
        "Extrude with a 10° draft",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 20] })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 5, "draft": 10 })),
            expect(json!({ "volume": frustum(400.0, top * top, 5.0) })),
        ],
    );
    t.task(
        "model-13-symmetric",
        "modeling",
        "Symmetric extrude 10 puts 10 on each side (observed s16-009)",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [10, 10] })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 10, "sides": "symmetric" })),
            expect(json!({ "bounds": [[0, 0, -10], [10, 10, 10]] })),
        ],
    );
    t.task(
        "model-14-through-all",
        "modeling",
        "Through All cut from a sketch above the part",
        cat![
            bx(),
            [
                e("offset_plane", json!({ "from": "xy", "distance": 30 })),
                e("sketch", json!({ "construction": 3 })),
                e("circle", json!({ "center": [10, 5], "radius": 2 })),
                e("extrude", json!({ "sketch": 4, "region": 0, "distance": -1, "extent": "through_all", "result": "subtract" })),
                expect(json!({ "bodies": 1, "volume": 2000.0 - PI * 40.0 })),
            ]
        ],
    );
    t.task(
        "model-15-booleans",
        "modeling",
        "Union, then Subtract a third box",
        cat![
            bx(),
            box_(10.0, 10.0, 10.0, 3, (10.0, 0.0), "xy"),
            box_(4.0, 4.0, 30.0, 5, (2.0, 2.0), "xy"),
            [
                e("union", json!({ "target": 2, "tools": [4] })),
                expect(json!({ "bodies": 2 })),
                e("subtract", json!({ "target": 2, "tools": [6] })),
                expect(json!({ "bodies": 1, "volume": 2000 + 1000 - 1000 - 16 * 10 })),
            ]
        ],
    );
    t.task(
        "model-16-intersect",
        "modeling",
        "Intersect two overlapping boxes",
        cat![
            bx(),
            box_(20.0, 10.0, 10.0, 3, (10.0, 5.0), "xy"),
            [e("intersect", json!({ "target": 2, "tools": [4] })), expect(json!({ "bodies": 1, "volume": 10 * 5 * 10 }))]
        ],
    );
    t.task(
        "model-17-offset-face",
        "modeling",
        "Offset Face moves a side face out by 3",
        cat![bx(), [e("offset_face", json!({ "body": 2, "faces": [2], "distance": 3 })), expect(json!({ "volume": 23 * 10 * 10 }))]],
    );
    t.task(
        "model-18-split",
        "modeling",
        "Split a box with the YZ plane at x=10: two bodies",
        cat![
            bx(),
            [
                e("split_body", json!({ "origin": [10, 0, 0], "normal": [1, 0, 0], "items": [2] })),
                expect(json!({ "bodies": 2, "volumes": [1000, 1000] }))
            ]
        ],
    );
    t.task(
        "model-19-g2-fillet",
        "modeling",
        "G2 fillet on a box's top edges",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [3, 6, 9, 11], "radius": 2, "continuity": "g2" })),
                expect(json!({ "bodies": 1, "volume_between": [1900, 1999] })),
            ]
        ],
    );
    t.task(
        "model-20-wrap-emboss",
        "modeling",
        "Wrap & Emboss a sketch onto a cylinder",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 10 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 20 })),
            e("sketch", json!({ "plane": "xz" })),
            e("rectangle", json!({ "corner": [-2, 5], "opposite": [2, 15] })),
            e("wrap_emboss", json!({ "sketch": 3, "region": 0, "body": 2, "depth": 1 })),
            expect(json!({ "bodies": 1, "volume_between": [PI * 2000.0 + 1.0, PI * 2000.0 + 100.0] })),
        ],
    );
    t.task(
        "model-25-fillet-curvature",
        "modeling",
        "Fillet the top edges with the card's Curvature -1: a flat bevel",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [3, 6, 9, 11], "radius": 2, "curvature": -1 })),
                expect(json!({ "bodies": 1, "volume_between": [1880, 1899] })),
            ]
        ],
    );
    t.task(
        "model-22-extrude-to-object",
        "modeling",
        "Extrude a region up to a face of another body",
        cat![
            bx(),
            [
                e("sketch", json!({ "plane": "xy" })),
                e("circle", json!({ "center": [40, 5], "radius": 2 })),
                e("offset_plane", json!({ "from": "xy", "distance": 25 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": 1, "extent": "to_object", "to": { "plane": 4 } })),
                expect(json!({ "bodies": 2, "volume": 2000.0 + PI * 4.0 * 25.0 })),
            ]
        ],
    );
    t.task(
        "model-23-variable-fillet",
        "modeling",
        "Variable-radius fillet (2 to 4 along an edge)",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 2, "radius2": 4 })),
                expect(json!({ "volume_between": [2000.0 - fillet_loss(4.0, 20.0), 2000.0 - fillet_loss(2.0, 20.0)] })),
            ]
        ],
    );
    t.task(
        "model-24-fillet-chain-corner",
        "modeling",
        "Fillet three edges meeting at a corner (corner blend)",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 0, 10] }),
                json!({ "click": [20, 5, 10], "mods": "shift" }),
                json!({ "click": [20, 0, 5], "mods": "shift" }),
                json!({ "wait": 0.3 }),
                json!({ "text": "2" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                expect(json!({ "selection": 0 })),
                expect(json!({ "volume_between": [2000.0 - fillet_loss(2.0, 40.0) - 1.0, 2000.0 - fillet_loss(2.0, 20.0) - 1.0] })),
            ]
        ],
    );
}

fn transform_tasks(t: &mut Tasks) {
    t.task(
        "xf-01-translate",
        "transform",
        "Translate a body by (5, 0, 0)",
        cat![bx(), [e("translate", json!({ "by": [5, 0, 0], "items": [2] })), expect(json!({ "bounds": [[5, 0, 0], [25, 10, 10]] }))]],
    );
    t.task(
        "xf-02-rotate",
        "transform",
        "Rotate 90° about Z",
        cat![
            bx(),
            [
                e("rotate", json!({ "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 90, "items": [2] })),
                expect(json!({ "bounds": [[-10, 0, 0], [0, 20, 10]] }))
            ]
        ],
    );
    t.task(
        "xf-03-mirror-copy",
        "transform",
        "Mirror a copy across x=0",
        cat![
            bx(),
            [
                e("mirror", json!({ "origin": [0, 0, 0], "normal": [1, 0, 0], "items": [2], "copy": true })),
                expect(json!({ "bodies": 2, "bounds": [[-20, 0, 0], [20, 10, 10]] })),
            ]
        ],
    );
    t.task(
        "xf-04-linear-pattern",
        "transform",
        "Linear pattern of 4, spacing 30",
        cat![
            bx(),
            [
                e("pattern", json!({ "direction": [1, 0, 0], "spacing": 30, "count": 4, "items": [2] })),
                expect(json!({ "bodies": 4, "volume": 8000 }))
            ]
        ],
    );
    t.task(
        "xf-05-circular-pattern",
        "transform",
        "Circular pattern of 6 around Z",
        cat![
            box_(4.0, 4.0, 4.0, 1, (20.0, 0.0), "xy"),
            [
                e("pattern_circular", json!({ "origin": [0, 0, 0], "axis": [0, 0, 1], "count": 6, "angle": 360, "items": [2] })),
                expect(json!({ "bodies": 6, "volume": 6 * 64 })),
            ]
        ],
    );
    t.task(
        "xf-06-scale",
        "transform",
        "Scale 2× about the origin",
        cat![
            bx(),
            [
                e("scale", json!({ "factor": 2, "center": [0, 0, 0], "items": [2] })),
                expect(json!({ "volume": 16000, "bounds": [[0, 0, 0], [40, 20, 20]] }))
            ]
        ],
    );
    t.task(
        "xf-07-scale-nonuniform",
        "transform",
        "Scale only along X by 3",
        cat![bx(), [e("scale", json!({ "factors": [3, 1, 1], "center": [0, 0, 0], "items": [2] })), expect(json!({ "volume": 6000 }))]],
    );
    t.task(
        "xf-08-align-face",
        "transform",
        "Align a box's bottom onto another box's top",
        cat![
            bx(),
            box_(5.0, 5.0, 5.0, 3, (40.0, 0.0), "xy"),
            [
                e("align", json!({ "from": { "body": 4, "face": 4 }, "to": { "body": 2, "face": 5 } })),
                expect(json!({ "bounds": [[0, 0, 0], [20, 10, 15]] })),
            ]
        ],
    );
    t.task(
        "xf-12-unlinked-copy",
        "transform",
        "Move with Copy and the Link badge off: an Unlinked Copy that keeps its shape",
        cat![
            bx(),
            [
                e("translate", json!({ "by": [25, 0, 0], "items": [2], "copy": true, "linked": false })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 4 } })),
                expect(json!({ "bodies": 2, "volume": 2000 + 800 })),
            ]
        ],
    );
    t.task(
        "xf-10-move-copies",
        "transform",
        "Translate with 3 copies",
        cat![bx(), [e("translate", json!({ "by": [25, 0, 0], "items": [2], "copy": true, "copies": 3 })), expect(json!({ "bodies": 4 }))]],
    );
}

fn construction_and_selection_tasks(t: &mut Tasks) {
    t.task(
        "con-01-offset-plane-sketch",
        "construction",
        "Offset plane 15 up, sketch on it, extrude",
        vec![
            e("offset_plane", json!({ "from": "xy", "distance": 15 })),
            e("sketch", json!({ "construction": 1 })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [10, 10] })),
            e("extrude", json!({ "sketch": 2, "region": 0, "distance": 5 })),
            expect(json!({ "bounds": [[0, 0, 15], [10, 10, 20]] })),
        ],
    );
    t.task(
        "con-02-midplane-mirror",
        "construction",
        "Midplane between two faces",
        cat![
            bx(),
            [e("midplane", json!({ "a": { "body": 2, "face": 0 }, "b": { "body": 2, "face": 2 } })), expect(json!({ "bodies": 1 }))]
        ],
    );
    t.task(
        "con-03-plane-3points",
        "construction",
        "Plane through 3 points, sketch on it",
        vec![
            e("plane_3points", json!({ "points": [[0, 0, 5], [10, 0, 5], [0, 10, 5]] })),
            e("sketch", json!({ "construction": 1 })),
            e("circle", json!({ "center": [0, 0], "radius": 1 })),
            e("extrude", json!({ "sketch": 2, "region": 0, "distance": 1 })),
            expect(json!({ "bodies": 1, "volume_between": [PI - 0.01, PI + 0.01] })),
        ],
    );
    t.task(
        "con-04-axis-revolve",
        "construction",
        "Axis along an edge; revolve a profile around that edge",
        cat![
            bx(),
            [
                e("axis_along_edge", json!({ "body": 2, "edge": 1 })),
                e("sketch", json!({ "plane": "xz" })),
                e("rectangle", json!({ "corner": [30, 0], "opposite": [32, 10] })),
                e("revolve", json!({ "sketch": 4, "region": 0, "axis": { "body": 2, "edge": 1 }, "angle": 360, "result": "new" })),
                expect(json!({ "bodies": 2, "volume": 2000.0 + PI * (32.0f64.powi(2) - 30.0f64.powi(2)) * 10.0 })),
            ]
        ],
    );
    // con-05 (construction point) retired 2026-11-02: no construction points in the menus.
    t.task(
        "sel-01-select-all",
        "selection",
        "⌘A selects every body",
        cat![
            bx(),
            box_(5.0, 5.0, 5.0, 3, (30.0, 0.0), "xy"),
            [json!({ "key": "A", "mods": "cmd" }), json!({ "wait": 0.2 }), expect(json!({ "selection": 2 }))]
        ],
    );
    t.task(
        "sel-02-measure-faces",
        "selection",
        "Measure two parallel faces: 20 apart",
        cat![
            bx(),
            [returns(
                c("engine.execute", json!({ "command": "measure", "params": { "picks": [{ "kind": "face", "body": 2, "face": 0 }, { "kind": "face", "body": 2, "face": 2 }] } })),
                json!({ "at": "/distance/mm", "near": 20 })
            )]
        ],
    );
    t.task(
        "sel-03-measure-volume",
        "selection",
        "Measure a body: its volume",
        cat![
            bx(),
            [returns(
                c("engine.execute", json!({ "command": "measure", "params": { "picks": [{ "kind": "body", "body": 2 }] } })),
                json!({ "at": "/items/0/volume_mm3", "near": 2000 })
            )]
        ],
    );
    t.task(
        "sel-04-click-deselect",
        "selection",
        "Click a face, then empty space: nothing stays selected",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 5, 10] }),
                json!({ "wait": 0.2 }),
                json!({ "key": "Escape" }),
                json!({ "click": [1300, 1150] }),
                json!({ "wait": 0.2 }),
                expect(json!({ "selection": 0 })),
            ]
        ],
    );
    t.task(
        "sel-05-selection-filter",
        "selection",
        "Selection filter Bodies only: clicking a face picks its body",
        cat![
            bx(),
            default_view(),
            [
                c("ui.selection_filter", json!({ "only": "bodies" })),
                json!({ "click": [10, 5, 10] }),
                json!({ "wait": 0.2 }),
                returns(c0("ui.state"), json!({ "at": "/selection/0/kind", "is": "body" })),
            ]
        ],
    );
    t.task(
        "sel-06-interference",
        "selection",
        "Interference check finds overlapping bodies",
        cat![
            bx(),
            box_(20.0, 10.0, 10.0, 3, (10.0, 0.0), "xy"),
            [returns(
                c(
                    "engine.execute",
                    json!({ "command": "measure", "params": { "picks": [{ "kind": "body", "body": 2 }, { "kind": "body", "body": 4 }] } })
                ),
                json!({ "at": "/interference", "min": 1 })
            )]
        ],
    );
}

fn history_tasks(t: &mut Tasks) {
    let fl = fillet_loss(2.0, 20.0);
    let hole = PI * 4.0;
    let fillet_click = || {
        vec![
            json!({ "click": [10, 0, 10] }),
            json!({ "wait": 0.3 }),
            json!({ "text": "2" }),
            json!({ "key": "Enter" }),
            json!({ "wait": 0.3 }),
        ]
    };
    t.task(
        "hist-01-edit-extrude-keeps-fillet",
        "history",
        "Fillet the top front edge, then make the box taller: the fillet stays on that edge",
        cat![
            bx(),
            default_view(),
            fillet_click(),
            [
                expect(json!({ "volume": 2000.0 - fl })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 15 } })),
                expect(json!({ "volume": 3000.0 - fl, "bounds": [[0, 0, 0], [20, 10, 15]], "failed_steps": 0 })),
            ]
        ],
    );
    t.task(
        "hist-02-edit-under-hole",
        "history",
        "Hole sketched on the top face; the box gets taller: the hole follows the top",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": "@face:2:10,5,10" })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -5 })),
                expect(json!({ "volume": 2000.0 - PI * 20.0 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 15 } })),
                expect(json!({ "volume": 3000.0 - PI * 20.0, "failed_steps": 0 })),
            ]
        ],
    );
    t.task(
        "hist-03-pushpull-after-renumber",
        "history",
        "Cut a side hole, push the top face up, then edit the box: the push stays on the top",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": "@face:2:10,0,5" })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -3 })),
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,10", "distance": 5 })),
            ],
            [
                returns(inspect(), json!({ "at": "/bodies/0/bounds/max/2", "near": 15, "tol": 0.01 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 12 } })),
                expect(json!({ "bounds": [[0, 0, 0], [20, 10, 17]], "failed_steps": 0 })),
            ]
        ],
    );
    t.task(
        "hist-04-suppress",
        "history",
        "Suppress a fillet and bring it back",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 2 })),
                e("suppress_step", json!({ "step": 2, "suppressed": true })),
                expect(json!({ "volume": 2000 })),
                e("suppress_step", json!({ "step": 2, "suppressed": false })),
                expect(json!({ "volume": 2000.0 - fl, "failed_steps": 0 })),
            ]
        ],
    );
    t.task(
        "hist-05-delete-middle",
        "history",
        "Push the top up twice, delete the first push: the second still pushes the top",
        cat![
            bx(),
            [
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,10", "distance": 5 })),
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,15", "distance": 5 })),
                e("delete_step", json!({ "step": 2 })),
                expect(json!({ "volume": 3000, "bounds": [[0, 0, 0], [20, 10, 15]], "failed_steps": 0 })),
            ]
        ],
    );
    t.task(
        "hist-06-variable",
        "history",
        "Extrude by a variable, then change it",
        vec![
            e("set_variable", json!({ "name": "h", "value": "10 mm" })),
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": "h" })),
            expect(json!({ "volume": 2000 })),
            e("set_variable", json!({ "name": "h", "value": "25 mm" })),
            expect(json!({ "volume": 5000 })),
        ],
    );
    t.task(
        "hist-07-breakpoint",
        "history",
        "A breakpoint before the fillet rolls it back; removing it restores",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 2 })),
                e("set_breakpoint", json!({ "before": 2 })),
                expect(json!({ "volume": 2000 })),
                e0("set_breakpoint"),
                expect(json!({ "volume": 2000.0 - fl })),
            ]
        ],
    );
    let undo = || json!({ "key": "Z", "mods": "cmd" });
    let redo = || json!({ "key": "Z", "mods": "cmd+shift" });
    t.task(
        "hist-08-undo-redo-chain",
        "history",
        "Five steps, undo three, redo two",
        cat![
            bx(),
            [
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,10", "distance": 1 })),
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,11", "distance": 1 })),
                e("extrude", json!({ "body": 2, "face": "@face:2:10,5,12", "distance": 1 })),
                undo(),
                undo(),
                undo(),
                expect(json!({ "volume": 2000 })),
                redo(),
                redo(),
                expect(json!({ "volume": 2400 })),
            ]
        ],
    );
    t.task(
        "hist-09-edit-sketch-past",
        "history",
        "Edit the first sketch after a fillet: widen the rectangle, the fillet survives",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 2 })),
                e("edit_sketch_step", json!({ "step": 0 })),
                e("set_entity", json!({ "sketch": 1, "entities": wide_sketch() })),
                e0("exit_sketching"),
                expect(json!({ "bodies": 1, "failed_steps": 0, "volume": 3000.0 - fillet_loss(2.0, 30.0) })),
            ]
        ],
    );
    t.task(
        "hist-10-pattern-count",
        "history",
        "Edit a pattern's count from 3 to 5",
        cat![
            bx(),
            [
                e("pattern", json!({ "direction": [1, 0, 0], "spacing": 30, "count": 3, "items": [2] })),
                expect(json!({ "bodies": 3 })),
                e("edit_step", json!({ "step": 2, "params": { "count": 5 } })),
                expect(json!({ "bodies": 5 })),
            ]
        ],
    );
    t.task(
        "hist-11-edit-breaks-downstream",
        "history",
        "Make the box too short for its 4 mm fillet: the edit applies, the fillet is marked",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 4 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 3 } })),
                expect(json!({ "bounds": [[0, 0, 0], [20, 10, 3]], "failed_steps": 1 })),
            ]
        ],
    );
    t.task(
        "hist-12-long-chain",
        "history",
        "Eight steps; editing the first replays them all",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": "@face:2:10,5,10" })),
                e("circle", json!({ "center": [0, 5], "radius": 1 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -4 })),
                e("sketch", json!({ "body": 2, "face": "@face:2:10,5,10" })),
                e("circle", json!({ "center": [0, -5], "radius": 1 })),
                e("extrude", json!({ "sketch": 4, "region": 0, "distance": -4 })),
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:0,0,5"], "radius": 1 })),
                e("shell", json!({ "body": 2, "faces": ["@face:2:10,5,0"], "thickness": 0.5 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 14 } })),
                expect(json!({ "bodies": 1, "failed_steps": 0, "bounds": [[0, 0, 0], [20, 10, 14]] })),
            ]
        ],
    );
    t.task(
        "hist-13-rename-and-merge",
        "history",
        "Rename a step, then Merge Modeling Steps",
        cat![
            bx(),
            [
                e("rename_step", json!({ "step": 1, "name": "Base" })),
                expect(json!({ "history": ["Sketch", "Base"] })),
                e("merge_steps", json!({ "keep_sketches": false, "keep_variables": true })),
                expect(json!({ "history": 1, "volume": 2000 })),
            ]
        ],
    );
    t.task(
        "hist-14-step-card-ui",
        "history",
        "Open the extrude's step card from History, click Distance, type a new value",
        cat![
            bx(),
            [
                c("ui.history_step", json!({ "step": 1 })),
                json!({ "wait": 0.5 }),
                json!({ "double_click": [242, 1162] }),
                json!({ "key": "A", "mods": "cmd" }),
                json!({ "text": "30" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.5 }),
                expect(json!({ "volume": 6000 })),
            ]
        ],
    );
    t.task(
        "hist-15-edit-fillet-radius-after-shell",
        "history",
        "Fillet, shell, then change the fillet radius",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 1 })),
                e("shell", json!({ "body": 2, "faces": [4], "thickness": 1 })),
                e("edit_step", json!({ "step": 2, "params": { "radius": 2 } })),
                expect(json!({ "bodies": 1, "failed_steps": 0 })),
            ]
        ],
    );
    t.task_with(
        "hist-16-delete-sketch-used",
        "history",
        "Delete the step a later feature needs: refused or the later step marked",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 1 })),
                e("delete_step", json!({ "step": 1 })),
                expect(json!({ "failed_steps": 1 }))
            ]
        ],
        true,
    );
    t.task(
        "hist-17-chamfer-after-edit-width",
        "history",
        "Chamfer the top right edge, then widen the box via its sketch: chamfer moves with the edge",
        cat![
            bx(),
            [
                e("chamfer", json!({ "body": 2, "edges": [9], "distance": 1 })),
                e("edit_sketch_step", json!({ "step": 0 })),
                e("set_entity", json!({ "sketch": 1, "entities": wide_sketch() })),
                e0("exit_sketching"),
                expect(json!({ "failed_steps": 0, "volume": 3000.0 - 0.5 * 10.0 })),
            ]
        ],
    );
    // Second wave (2026-11-01): picks recorded by number must survive the model being renumbered by edits upstream.
    t.task(
        "hist-18-edit-fillet-radius-after-hole",
        "history",
        "Fillet the top front edge, cut a hole in the top, then change the fillet to 3",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": 2 })),
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                e("edit_step", json!({ "step": 2, "params": { "radius": 3 } })),
                expect(json!({ "failed_steps": 0, "volume": 2000.0 - fillet_loss(3.0, 20.0) - hole * 10.0 })),
            ]
        ],
    );
    t.task(
        "hist-19-edit-hole-radius",
        "history",
        "Edit the hole's sketch (circle 2 → 3) under a later fillet",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": 1 })),
                e("edit_sketch_step", json!({ "step": 2 })),
                e("set_entity", json!({ "sketch": 3, "entity": 0, "value": { "type": "circle", "center": [0, 0], "radius": 3 } })),
                e0("exit_sketching"),
                expect(json!({ "failed_steps": 0, "volume": 2000.0 - PI * 9.0 * 10.0 - fillet_loss(1.0, 20.0) })),
            ]
        ],
    );
    t.task(
        "hist-20-suppress-hole",
        "history",
        "Suppress the hole: the fillet after it stays on its edge",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": 2 })),
                e("suppress_step", json!({ "step": 3, "suppressed": true })),
                expect(json!({ "failed_steps": 0, "volume": 2000.0 - fl })),
            ]
        ],
    );
    t.task(
        "hist-21-two-bodies",
        "history",
        "Editing one body's extrude leaves the other body's fillet alone",
        cat![
            bx(),
            box_(10.0, 10.0, 10.0, 3, (40.0, 0.0), "xy"),
            [
                e("fillet", json!({ "body": 4, "edges": ["@edge:4:45,0,10"], "radius": 2 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 20 } })),
                expect(json!({ "failed_steps": 0, "volumes": [4000.0, 1000.0 - fillet_loss(2.0, 10.0)] })),
            ]
        ],
    );
    // (Top-face sketches read like the Top view since 2026-11-01: u along +X.)
    t.task(
        "hist-22-delete-first-hole",
        "history",
        "Two holes in the top; delete the first: the second stays where it was",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [5, 0], "radius": 1 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [-5, 0], "radius": 1 })),
                e("extrude", json!({ "sketch": 4, "region": 0, "distance": -10 })),
                e("delete_step", json!({ "step": 3 })),
                expect(json!({ "failed_steps": 0, "volume": 2000.0 - PI * 10.0 })),
            ]
        ],
    );
    t.task(
        "hist-23-undo-edit",
        "history",
        "Undo an edited step: the model goes back",
        cat![
            bx(),
            [
                e("edit_step", json!({ "step": 1, "params": { "distance": 30 } })),
                expect(json!({ "volume": 6000 })),
                undo(),
                expect(json!({ "volume": 2000 }))
            ]
        ],
    );
    t.task(
        "hist-24-region-after-sketch-edit",
        "history",
        "Extrude the ring around a circle, then move the circle: still the ring",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 20] })),
            e("circle", json!({ "center": [10, 10], "radius": 3 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 5 })),
            expect(json!({ "volume": (400.0 - PI * 9.0) * 5.0 })),
            e("edit_sketch_step", json!({ "step": 0 })),
            e("set_entity", json!({ "sketch": 1, "entity": 4, "value": { "type": "circle", "center": [6, 6], "radius": 3 } })),
            e0("exit_sketching"),
            expect(json!({ "failed_steps": 0, "volume": (400.0 - PI * 9.0) * 5.0 })),
        ],
    );
    t.task(
        "hist-25-reopen-then-edit",
        "history",
        "Save, reopen, then edit the first extrude: the fillet replays",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": 2 })),
                e("save", json!({ "path": "$TMP/h25" })),
                c0("project.home"),
                c("project.open", json!({ "path": "$TMP/h25" })),
                json!({ "wait": 0.3 }),
                e("edit_step", json!({ "step": 1, "params": { "distance": 15 } })),
                expect(json!({ "failed_steps": 0, "volume": 3000.0 - fl })),
            ]
        ],
    );
    t.task(
        "hist-26-breakpoint-insert",
        "history",
        "Breakpoint before the fillet, add a hole there, remove it: the fillet is still on its edge",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": 2 })),
                e("set_breakpoint", json!({ "before": 2 })),
                e("sketch", json!({ "body": 2, "face": TOPF })),
                e("circle", json!({ "center": [0, 0], "radius": 2 })),
                e("extrude", json!({ "sketch": 3, "region": 0, "distance": -10 })),
                e0("set_breakpoint"),
                expect(json!({ "failed_steps": 0, "volume": 2000.0 - fl - hole * 10.0 })),
            ]
        ],
    );
    t.task(
        "hist-27-offset-face-after-widen",
        "history",
        "Offset the right face out 3, then widen the sketch: the offset stays on the right face",
        cat![
            bx(),
            [
                e("offset_face", json!({ "body": 2, "faces": ["@face:2:20,5,5"], "distance": 3 })),
                e("edit_sketch_step", json!({ "step": 0 })),
                e("set_entity", json!({ "sketch": 1, "entities": wide_sketch() })),
                e0("exit_sketching"),
                expect(json!({ "failed_steps": 0, "bounds": [[0, 0, 0], [33, 10, 10]] })),
            ]
        ],
    );
    t.task(
        "hist-28-chamfer-then-taller",
        "history",
        "Chamfer the top front edge by clicking it, then make the box taller",
        cat![
            bx(),
            default_view(),
            [
                json!({ "click": [10, 0, 10] }),
                json!({ "wait": 0.3 }),
                c("ui.tool", json!({ "tool": "none" })),
                e("chamfer", json!({ "distance": 2 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 14 } })),
                expect(json!({ "failed_steps": 0, "volume": 2800 - 40 })),
            ]
        ],
    );
    t.task(
        "hist-29-shell-after-push",
        "history",
        "Shell the top open, then push the box taller: the open face is still the top",
        cat![
            bx(),
            [
                e("shell", json!({ "body": 2, "faces": [TOPF], "thickness": 1 })),
                e("edit_step", json!({ "step": 1, "params": { "distance": 15 } })),
                expect(json!({ "failed_steps": 0, "volume": 3000 - 18 * 8 * 14 })),
            ]
        ],
    );
    t.task(
        "hist-30-variable-chain",
        "history",
        "Width and depth variables drive two steps; change both",
        cat![
            [e("set_variable", json!({ "name": "h", "value": "10 mm" })), e("set_variable", json!({ "name": "r", "value": "h / 5" }))],
            bx(),
            [
                e("edit_step", json!({ "step": 1, "params": { "distance": "h" } })),
                e("fillet", json!({ "body": 2, "edges": ["@edge:2:10,0,10"], "radius": "r" })),
                e("set_variable", json!({ "name": "h", "value": "20 mm" })),
                expect(json!({ "failed_steps": 0, "volume": 4000.0 - fillet_loss(4.0, 20.0) })),
            ]
        ],
    );
}

fn items_view_vis_draw_io_tasks(t: &mut Tasks) {
    t.task(
        "items-01-rename",
        "items",
        "Rename a body",
        cat![
            bx(),
            [e("rename", json!({ "item": 2, "name": "Bracket" })), returns(inspect(), json!({ "at": "/bodies/0/name", "is": "Bracket" }))]
        ],
    );
    t.task(
        "items-02-hide-show",
        "items",
        "Hide a body, then Show All",
        cat![
            bx(),
            [
                e("show_hide", json!({ "items": [2], "visible": false })),
                expect(json!({ "visible_bodies": 0 })),
                e0("show_all"),
                expect(json!({ "visible_bodies": 1 })),
            ]
        ],
    );
    t.task(
        "items-03-folder",
        "items",
        "Put two bodies in a folder",
        cat![
            bx(),
            box_(5.0, 5.0, 5.0, 3, (30.0, 0.0), "xy"),
            [e("new_folder", json!({ "name": "Parts", "items": [2, 4] })), expect(json!({ "bodies": 2 }))]
        ],
    );
    t.task(
        "items-04-delete",
        "items",
        "Delete a body",
        cat![
            bx(),
            [
                e("select", json!({ "picks": [{ "kind": "body", "body": 2 }] })),
                json!({ "key": "Backspace" }),
                expect(json!({ "bodies": 0 }))
            ]
        ],
    );
    t.task(
        "items-05-nested-folder",
        "items",
        "A folder inside a folder",
        cat![
            bx(),
            [
                e("new_folder", json!({ "name": "A", "items": [2] })),
                e("new_folder", json!({ "name": "B", "parent": 4 })),
                expect(json!({ "bodies": 1 })),
            ]
        ],
    );
    t.task(
        "view-01-standard-views",
        "viewport",
        "Top view looks straight down",
        cat![
            bx(),
            [
                c("ui.view", json!({ "view": "top" })),
                json!({ "wait": 1.0 }),
                returns(c0("ui.state"), json!({ "at": "/camera/pitch", "near": PI / 2.0, "tol": 0.01 })),
            ]
        ],
    );
    t.task(
        "view-02-front",
        "viewport",
        "Front view: level camera",
        cat![
            bx(),
            [
                c("ui.view", json!({ "view": "front" })),
                json!({ "wait": 1.0 }),
                returns(c0("ui.state"), json!({ "at": "/camera/pitch", "near": 0, "tol": 0.01 }))
            ]
        ],
    );
    t.task(
        "view-03-section",
        "viewport",
        "Preferences values can be changed (placeholder until Section View has a control)",
        cat![bx(), [c("ui.setting", json!({ "key": "theme", "value": "Dark" })), json!({ "wait": 0.3 }), expect(json!({ "bodies": 1 }))]],
    );
    t.task(
        "vis-01-appearance",
        "visualization",
        "Aluminum appearance on a body",
        cat![
            bx(),
            [
                e("set_appearance", json!({ "items": [2], "appearance": "Aluminum" })),
                c("ui.toggle", json!({ "panel": "visualization" })),
                json!({ "wait": 0.3 }),
                expect(json!({ "bodies": 1 })),
            ]
        ],
    );
    t.task(
        "vis-02-face-color",
        "visualization",
        "Colour one face",
        cat![bx(), [e("set_face_color", json!({ "body": 2, "faces": [5], "color": [1, 0, 0] })), expect(json!({ "bodies": 1 }))]],
    );
    t.task(
        "vis-03-decal",
        "visualization",
        "A picture as a decal on the top face",
        cat![
            bx(),
            [
                c("ui.capture", json!({ "path": "$TMP/decal.png" })),
                save(e("import_decal", json!({ "path": "$TMP/decal.png" })), "img", "/image"),
                e("place_decal", json!({ "image": "$img", "body": 2, "face": 5, "point": [10, 5, 10] })),
                expect(json!({ "bodies": 1 })),
            ]
        ],
    );
    t.task(
        "draw-01-create-export",
        "drawings",
        "Create a drawing and export it as PDF",
        cat![
            bx(),
            [
                e("create_drawing", json!({ "bodies": [2] })),
                e("export_drawing", json!({ "path": "$TMP/d.pdf" })),
                expect(json!({ "file": "$TMP/d.pdf" })),
            ]
        ],
    );
    t.task(
        "draw-02-dimension",
        "drawings",
        "Put a distance dimension on the front view",
        cat![
            bx(),
            [
                e("create_drawing", json!({ "bodies": [2] })),
                save(
                    e("add_drawing_view", json!({ "drawing": 3, "kind": "base", "orientation": "front", "at": [150, 150] })),
                    "v",
                    "/view"
                ),
                e(
                    "annotate",
                    json!({ "drawing": 3, "annotation": { "kind": "distance", "view": "$v", "a": [0, 0], "b": [20, 0], "offset": 5 } })
                ),
                e("export_drawing", json!({ "drawing": 3, "path": "$TMP/d.dxf" })),
                expect(json!({ "file": "$TMP/d.dxf" })),
            ]
        ],
    );
    t.task(
        "draw-03-section-view",
        "drawings",
        "A section view from a cut line",
        cat![
            bx(),
            [
                save(e("create_drawing", json!({ "bodies": [2] })), "dr", "/drawing"),
                save(
                    e("add_drawing_view", json!({ "drawing": "$dr", "kind": "base", "orientation": "front", "at": [150, 150] })),
                    "v",
                    "/view"
                ),
                e(
                    "add_drawing_view",
                    json!({ "drawing": "$dr", "kind": "section", "parent": "$v", "a": [-15, 5], "b": [15, 5], "at": [300, 150] })
                ),
                expect(json!({ "bodies": 1 })),
            ]
        ],
    );
    t.task(
        "draw-04-dwg",
        "drawings",
        "Export DWG",
        cat![
            bx(),
            [
                e("create_drawing", json!({ "bodies": [2] })),
                e("export_drawing", json!({ "drawing": 3, "path": "$TMP/d.dwg" })),
                expect(json!({ "file": "$TMP/d.dwg" })),
            ]
        ],
    );
    for fmt in ["step", "iges", "stl"] {
        let file = format!("$TMP/b.{fmt}");
        t.task(
            &format!("io-{fmt}-roundtrip"),
            "io",
            &format!("Export {}, import it into a new project: same volume", fmt.to_uppercase()),
            cat![
                bx(),
                [
                    e(&format!("export_{fmt}"), json!({ "path": file })),
                    expect(json!({ "file": file })),
                    c0("project.new"),
                    e("import_file", json!({ "path": file })),
                    expect(json!({ "bodies": 1, "volume": [2000, 0.01] })),
                ]
            ],
        );
    }
    t.task(
        "io-obj-3mf-glb",
        "io",
        "Export OBJ, 3MF, GLB and USDZ",
        cat![
            bx(),
            [
                e("export_obj", json!({ "path": "$TMP/b.obj" })),
                e("export_3mf", json!({ "path": "$TMP/b.3mf" })),
                e("export_glb", json!({ "path": "$TMP/b.glb" })),
                e("export_usdz", json!({ "path": "$TMP/b.usdz" })),
                expect(json!({ "file": "$TMP/b.3mf" })),
                expect(json!({ "file": "$TMP/b.glb" })),
                expect(json!({ "file": "$TMP/b.usdz" })),
            ]
        ],
    );
    t.task(
        "io-dxf-sketch",
        "io",
        "Export a sketch as DXF and import it back",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [20, 10] })),
            e("circle", json!({ "center": [30, 0], "radius": 3 })),
            e("export_sketch", json!({ "path": "$TMP/s.dxf" })),
            c0("project.new"),
            e("import_file", json!({ "path": "$TMP/s.dxf" })),
            expect(json!({ "sketches": 1, "regions": 2 })),
        ],
    );
    t.task(
        "io-shapr-roundtrip",
        "io",
        "Export .shapr, import it into a new project: same body, and its Fillet comes back as an editable step",
        cat![
            bx(),
            [
                e("fillet", json!({ "body": 2, "edges": [11], "radius": 2 })),
                e("export_shapr", json!({ "path": "$TMP/b.shapr" })),
                c0("project.new"),
                e("import_file", json!({ "path": "$TMP/b.shapr" })),
                json!({ "wait": 0.5 }),
                expect(json!({ "bodies": 1, "volume": 2000.0 - fillet_loss(2.0, 20.0) })),
                expect(json!({ "history_has": ["Fillet"] })),
            ]
        ],
    );
    t.task(
        "io-parasolid",
        "io",
        "Export Parasolid",
        cat![bx(), [e("export_parasolid", json!({ "path": "$TMP/b.x_b" })), expect(json!({ "file": "$TMP/b.x_b" }))]],
    );
}

fn project_and_breadth_tasks(t: &mut Tasks) {
    t.task(
        "proj-01-save-reopen",
        "projects",
        "Save, go home, open again",
        cat![
            bx(),
            [
                e("save", json!({ "path": "$TMP/p1" })),
                c0("project.home"),
                c("project.open", json!({ "path": "$TMP/p1" })),
                json!({ "wait": 0.5 }),
                expect(json!({ "bodies": 1, "volume": 2000 })),
            ]
        ],
    );
    t.task(
        "proj-02-rename",
        "projects",
        "Rename the project",
        cat![bx(), [e("rename_project", json!({ "name": "Widget" })), returns(inspect(), json!({ "at": "/name", "is": "Widget" }))]],
    );
    t.task(
        "proj-03-units",
        "projects",
        "Switch units to inches",
        cat![bx(), [e("set_units", json!({ "units": "Inch" })), returns(inspect(), json!({ "at": "/units", "is": "Inch" }))]],
    );
    t.task(
        "proj-04-search",
        "projects",
        "⌘F opens command search; typing 'fil' and Return runs Fillet",
        cat![
            bx(),
            [
                e("select", json!({ "picks": [{ "kind": "edge", "body": 2, "edge": "@edge:2:10,0,10" }] })),
                json!({ "key": "F", "mods": "cmd" }),
                json!({ "text": "Fillet" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                json!({ "text": "1" }),
                json!({ "key": "Enter" }),
                json!({ "wait": 0.3 }),
                expect(json!({ "volume": 2000.0 - fillet_loss(1.0, 20.0) })),
            ]
        ],
    );
    t.task(
        "proj-05-theme",
        "projects",
        "Light theme",
        vec![c("ui.theme", json!({ "theme": "Light" })), returns(c0("ui.state"), json!({ "at": "/screen", "is": "workspace" }))],
    );

    // Sketching by input
    t.task(
        "sketch-16-circle-two-clicks",
        "sketch",
        "C, click the center, click a rim point 5 away: a Ø10 circle",
        vec![
            c("ui.view", json!({ "view": "top" })),
            json!({ "wait": 0.8 }),
            json!({ "key": "C" }),
            json!({ "click": [0, 0, 0] }),
            json!({ "click": [5, 0, 0] }),
            json!({ "wait": 0.3 }),
            json!({ "key": "Escape" }),
            e0("exit_sketching"),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 1 })),
            expect(json!({ "volume": PI * 25.0 })),
        ],
    );
    t.task(
        "sketch-17-line-typed-length",
        "sketch",
        "L, click, type 8 Enter: an 8 mm line",
        vec![
            c("ui.view", json!({ "view": "top" })),
            json!({ "wait": 0.8 }),
            json!({ "key": "L" }),
            json!({ "click": [0, 0, 0] }),
            json!({ "hover": [5, 0.2, 0] }),
            json!({ "text": "8" }),
            json!({ "key": "Enter" }),
            json!({ "wait": 0.3 }),
            returns(inspect(), json!({ "at": "/sketches/0/entities/0/b/0", "near": 8, "tol": 0.3 })),
        ],
    );
    t.task(
        "sketch-18-undo-in-sketch",
        "sketch",
        "Draw two rectangles, ⌘Z removes the second",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "corner": [0, 0], "opposite": [5, 5] })),
            e("rectangle", json!({ "corner": [10, 0], "opposite": [15, 5] })),
            json!({ "key": "Z", "mods": "cmd" }),
            expect(json!({ "regions": 1 })),
        ],
    );
    t.task(
        "sketch-19-change-sketch-plane",
        "sketch",
        "Move a sketch to another plane (Change Sketch Plane)",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 2 })),
            e0("exit_sketching"),
            e("change_sketch_plane", json!({ "sketch": 1, "plane": "xz" })),
            expect(json!({ "sketches": 1 })),
        ],
    );
    t.task(
        "sketch-20-spline-break",
        "sketch",
        "Break a spline at a point",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("spline", json!({ "points": [[0, 0], [5, 5], [10, 0]] })),
            e("break_curve", json!({ "entity": 0, "at": [5, 5] })),
        ],
    );
    t.task(
        "sketch-21-text-font",
        "sketch",
        "Text in a named font",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("text", json!({ "at": [0, 0], "text": "A", "size": 10, "font": "Helvetica" })),
            expect(json!({ "regions": 2 })),
        ],
    );
    t.task(
        "sketch-22-face-sketch-axes",
        "sketch",
        "A sketch on a top face lines up with the Top view (u along +X, v along +Y)",
        cat![
            bx(),
            [
                e("sketch", json!({ "body": 2, "face": TOPF })),
                returns(inspect(), json!({ "at": "/sketches/1/plane/x/0", "near": 1, "tol": 1e-6 }))
            ]
        ],
    );

    // Constraints
    t.task(
        "cons-11-linked-pattern",
        "constraints",
        "A sketch pattern stays linked: making the source circle bigger makes the copies bigger",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 1 })),
            e("select", json!({ "picks": [{ "kind": "sketch_entity", "sketch": 1, "entity": 0 }] })),
            e("pattern", json!({ "direction": [1, 0, 0], "spacing": 5, "count": 3 })),
            expect(json!({ "regions": 3 })),
            e("set_entity", json!({ "sketch": 1, "entity": 0, "value": { "type": "circle", "center": [0, 0], "radius": 2 } })),
            e("extrude", json!({ "sketch": 1, "regions": [0, 1, 2], "distance": 1 })),
            expect(json!({ "volume": 3.0 * PI * 4.0 })),
        ],
    );
    t.task(
        "cons-12-fully-defined",
        "constraints",
        "R from the origin with both sizes typed: the rectangle is fully defined (green)",
        cat![ui_rect(20, 10), [returns(inspect(), json!({ "at": "/sketches/0/defined", "is": true }))]],
    );

    // Modeling breadth from the manual
    t.task(
        "model-25-fillet-setback",
        "modeling",
        "Fillet with Setback corners",
        cat![
            bx(),
            [e("fillet", json!({ "body": 2, "edges": [3, 6, 9, 11], "radius": 2, "setback": true })), expect(json!({ "bodies": 1 }))]
        ],
    );
    t.task(
        "model-26-offset-face-radius",
        "modeling",
        "Offset Face 'Radius' distance type on a cylinder",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("circle", json!({ "center": [0, 0], "radius": 5 })),
            e("extrude", json!({ "sketch": 1, "region": 0, "distance": 10 })),
            e("offset_face", json!({ "body": 2, "faces": ["@face:2:0,0,5"], "distance": 6, "distance_type": "radius" })),
            expect(json!({ "volume": PI * 36.0 * 10.0 })),
        ],
    );
    t.task(
        "model-27-split-by-sketch",
        "modeling",
        "Split a body with a sketch line",
        cat![
            bx(),
            [
                e("sketch", json!({ "plane": "xy" })),
                e("line", json!({ "a": [10, -5], "b": [10, 15] })),
                e("split_body", json!({ "with": [{ "sketch": 3, "entity": 0 }], "items": [2] })),
                expect(json!({ "bodies": 2, "volumes": [1000, 1000] })),
            ]
        ],
    );
    t.task(
        "model-28-sweep-twist",
        "modeling",
        "Sweep a square along a line with 90° twist",
        vec![
            e("sketch", json!({ "plane": "xy" })),
            e("rectangle", json!({ "center": [0, 0], "size": [4, 4] })),
            e("sketch", json!({ "plane": "xz" })),
            e("line", json!({ "a": [0, 0], "b": [0, 30] })),
            e("sweep", json!({ "profile": { "sketch": 1, "region": 0 }, "path": { "sketch": 2 }, "twist": 90 })),
            expect(json!({ "bodies": 1, "volume": [480, 0.01] })),
        ],
    );
    t.task(
        "model-29-revolve-ui",
        "modeling",
        "Select a region, V, pick the sketch's axis line, type 180",
        vec![
            e("sketch", json!({ "plane": "xz" })),
            e("rectangle", json!({ "corner": [2, 0], "opposite": [6, 10] })),
            e("line", json!({ "a": [0, 0], "b": [0, 10] })),
            e0("exit_sketching"),
            e("select", json!({ "picks": [{ "kind": "region", "sketch": 1, "region": 0 }] })),
            json!({ "key": "V" }),
            json!({ "wait": 0.3 }),
            e("revolve", json!({ "sketch": 1, "region": 0, "axis": { "line": 4 }, "angle": 180 })),
            expect(json!({ "bodies": 1, "volume": PI / 2.0 * (36.0 - 4.0) * 10.0 })),
        ],
    );
    t.task(
        "model-30-chordal-fillet",
        "modeling",
        "Chordal fillet (constant width)",
        cat![bx(), [e("fillet", json!({ "body": 2, "edges": [11], "radius": 2, "mode": "chordal" })), expect(json!({ "bodies": 1 }))]],
    );

    // Transform / construction / selection breadth
    t.task(
        "xf-11-move-copy-count",
        "transform",
        "Copy count in the Move gizmo (Copy ×3)",
        cat![
            bx(),
            [
                e("select", json!({ "picks": [{ "kind": "body", "body": 2 }] })),
                json!({ "key": "M" }),
                e("translate", json!({ "by": [25, 0, 0], "items": [2], "copy": true, "copies": 3 })),
                expect(json!({ "bodies": 4 })),
            ]
        ],
    );
    t.task(
        "con-06-plane-resize",
        "construction",
        "Resize a construction plane",
        vec![e("offset_plane", json!({ "from": "xy", "distance": 5 })), e("plane_size", json!({ "plane": 1, "size": 80 }))],
    );
    t.task(
        "sel-07-tab-cycle",
        "selection",
        "Tab cycles what's under the cursor (face → edge → body)",
        cat![
            bx(),
            default_view(),
            [
                json!({ "hover": [10, 5, 10] }),
                json!({ "key": "Tab" }),
                json!({ "key": "Tab" }),
                json!({ "click": [10, 5, 10] }),
                expect(json!({ "selection": 1 })),
            ]
        ],
    );

    // Items, viewport, visualization, drawings, io, projects
    t.task(
        "items-06-reorder",
        "items",
        "Drag a body to reorder the Items list",
        cat![bx(), box_(5.0, 5.0, 5.0, 3, (30.0, 0.0), "xy"), [e("place_item", json!({ "item": 4, "folder": null, "before": 2 }))]],
    );
    t.task(
        "vis-04-roughness",
        "visualization",
        "Set a material's roughness to 0.8",
        cat![
            bx(),
            [
                e(
                    "set_appearance",
                    json!({ "items": [2], "appearance": { "name": "Matte", "color": [0.5, 0.5, 0.5], "metal": 0, "roughness": 0.8 } })
                ),
                returns(inspect(), json!({ "at": "/bodies/0/appearance/gloss", "near": 0.2, "tol": 1e-6 })),
            ]
        ],
    );
    t.task(
        "vis-05-panorama",
        "visualization",
        "Import a panorama environment",
        cat![bx(), [c("ui.capture", json!({ "path": "$TMP/env.png" })), e("import_environment", json!({ "path": "$TMP/env.png" }))]],
    );
    t.task(
        "draw-05-update",
        "drawings",
        "Drawing follows a model edit (Update)",
        cat![
            bx(),
            [
                save(e("create_drawing", json!({ "bodies": [2] })), "dr", "/drawing"),
                e("edit_step", json!({ "step": 1, "params": { "distance": 20 } })),
                e("update_drawing", json!({ "drawing": "$dr" })),
                e("export_drawing", json!({ "drawing": "$dr", "path": "$TMP/u.svg" })),
                expect(json!({ "file": "$TMP/u.svg" })),
            ]
        ],
    );
    t.task(
        "io-x_t-import",
        "io",
        "Import Parasolid text (x_t)",
        cat![
            bx(),
            [
                e("export_parasolid", json!({ "path": "$TMP/b.x_t" })),
                c0("project.new"),
                e("import_file", json!({ "path": "$TMP/b.x_t" })),
                expect(json!({ "bodies": 1, "volume": [2000, 0.01] })),
            ]
        ],
    );
    t.task(
        "io-import-options",
        "io",
        "Import an STL whose numbers are inches (import options: Unit)",
        cat![
            bx(),
            [
                e("export_stl", json!({ "path": "$TMP/o.stl" })),
                c0("project.new"),
                e("import_file", json!({ "path": "$TMP/o.stl", "units": "in" })),
                expect(json!({ "bodies": 1, "volume": [2000.0 * 25.4f64.powi(3), 0.01] })),
            ]
        ],
    );
    t.task("proj-06-language", "projects", "Switch the language", vec![c("ui.setting", json!({ "key": "language", "value": "Deutsch" }))]);
    t.task(
        "proj-07-recents",
        "projects",
        "Projects screen lists a saved project",
        cat![bx(), [c0("project.save"), c0("project.home"), returns(c0("project.list"), json!({ "at": "/projects", "min": 1 }))]],
    );
}

/// Coverage for the smaller categories (5–6 tasks each made one gap swing them by 17%).
fn coverage_tasks(t: &mut Tasks) {
    let pl = |step: Value, name: &str| save(step, name, "/plane");
    t.task(
        "con-07-plane-through-edge",
        "construction",
        "Plane through an edge at 45°, sketch on it",
        cat![
            bx(),
            [
                pl(e("plane_through_edge", json!({ "body": 2, "edge": "@edge:2:10,0,10", "angle": 45 })), "p"),
                save(e("sketch", json!({ "construction": "$p" })), "sk", "/sketch"),
                e("circle", json!({ "center": [0, 0], "radius": 1 })),
                e("extrude", json!({ "sketch": "$sk", "region": 0, "distance": 1, "result": "new" })),
                expect(json!({ "bodies": 2 })),
            ]
        ],
    );
    t.task(
        "con-08-plane-on-curve",
        "construction",
        "Plane on a curve at its middle, perpendicular to it",
        cat![
            bx(),
            [
                pl(e("plane_on_curve", json!({ "body": 2, "edge": "@edge:2:10,0,10", "at": 0.5 })), "p"),
                e("sketch", json!({ "construction": "$p" })),
                e("circle", json!({ "center": [0, 0], "radius": 1 })),
                expect(json!({ "sketches": 2 })),
            ]
        ],
    );
    t.task(
        "con-09-axis-2planes-rotate",
        "construction",
        "Axis where XY and XZ meet (the X axis), rotate a body 90° around it",
        cat![
            bx(),
            [
                save(e("axis_2planes", json!({ "a": "xy", "b": "xz" })), "ax", "/axis"),
                e("rotate", json!({ "about": { "axis": "$ax" }, "angle": 90, "items": [2] })),
                expect(json!({ "bounds": [[0, -10, 0], [20, 0, 10]] })),
            ]
        ],
    );
    t.task(
        "con-10-plane-perp-edge",
        "construction",
        "Plane perpendicular to an edge at a point",
        cat![
            bx(),
            [
                pl(e("plane_perp_edge", json!({ "body": 2, "edge": "@edge:2:10,0,10", "point": [5, 0, 10] })), "p"),
                e("sketch", json!({ "construction": "$p" })),
                expect(json!({ "sketches": 2 })),
            ]
        ],
    );
    t.task(
        "con-11-plane-parallel-face",
        "construction",
        "Plane parallel to the top face through a point 5 above it",
        cat![
            bx(),
            [
                pl(e("plane_parallel_face", json!({ "from": { "body": 2, "face": "@face:2:10,5,10" }, "point": [0, 0, 15] })), "p"),
                save(e("sketch", json!({ "construction": "$p" })), "sk", "/sketch"),
                e("rectangle", json!({ "corner": [0, 0], "opposite": [4, 4] })),
                e("extrude", json!({ "sketch": "$sk", "region": 0, "distance": 1, "result": "new" })),
                expect(json!({ "bodies": 2, "bounds": [[0, 0, 0], [20, 10, 16]] })),
            ]
        ],
    );
    t.task(
        "con-12-axis-perp-face",
        "construction",
        "Axis perpendicular to a face at a point, circular pattern around it",
        cat![
            box_(4.0, 4.0, 4.0, 1, (10.0, 0.0), "xy"),
            [
                save(e("axis_perp_face", json!({ "body": 2, "face": "@face:2:12,2,0", "at": [0, 0, 0] })), "ax", "/axis"),
                e("pattern_circular", json!({ "about": { "axis": "$ax" }, "count": 4, "angle": 360, "items": [2] })),
                expect(json!({ "bodies": 4 })),
            ]
        ],
    );

    let drawing_with_front = || {
        cat![
            bx(),
            [
                save(e("create_drawing", json!({ "bodies": [2] })), "dr", "/drawing"),
                save(
                    e("add_drawing_view", json!({ "drawing": "$dr", "kind": "base", "orientation": "front", "at": [120, 180] })),
                    "v",
                    "/view"
                ),
            ]
        ]
    };
    t.task(
        "draw-06-projected",
        "drawings",
        "A projected view to the right of the front view",
        cat![
            drawing_with_front(),
            [
                e("add_drawing_view", json!({ "drawing": "$dr", "kind": "projected", "parent": "$v", "dx": 1, "dy": 0, "at": [260, 180] })),
                e("export_drawing", json!({ "drawing": "$dr", "path": "$TMP/p.svg" })),
                expect(json!({ "file": "$TMP/p.svg" })),
            ]
        ],
    );
    t.task(
        "draw-07-detail",
        "drawings",
        "A detail view of a corner at 2×",
        cat![
            drawing_with_front(),
            [
                e("add_drawing_view", json!({ "drawing": "$dr", "kind": "detail", "parent": "$v", "center": [20, 10], "radius": 4, "scale": 2, "at": [300, 80] })),
                e("export_drawing", json!({ "drawing": "$dr", "path": "$TMP/dt.pdf" })),
                expect(json!({ "file": "$TMP/dt.pdf" })),
            ]
        ],
    );
    t.task(
        "draw-08-tolerance",
        "drawings",
        "A dimension with a ± tolerance and a prefix",
        cat![
            drawing_with_front(),
            [
                e("annotate", json!({ "drawing": "$dr", "annotation": { "kind": "distance", "view": "$v", "a": [0, 0], "b": [20, 0], "offset": 5 } })),
                e(
                    "update_annotation",
                    json!({ "drawing": "$dr", "index": 0, "text": { "prefix": "2×", "tolerance": { "kind": "symmetrical", "upper": 0.1, "lower": 0.1 } } })
                ),
                e("export_drawing", json!({ "drawing": "$dr", "path": "$TMP/t.svg" })),
                expect(json!({ "file": "$TMP/t.svg" })),
            ]
        ],
    );
    t.task(
        "draw-09-view-bodies",
        "drawings",
        "A base view showing only some bodies (the view's Bodies… pill)",
        cat![
            bx(),
            box_(5.0, 5.0, 5.0, 3, (30.0, 0.0), "xy"),
            [
                save(e("create_drawing", json!({ "bodies": [2, 4] })), "dr", "/drawing"),
                e("add_drawing_view", json!({ "drawing": "$dr", "kind": "base", "orientation": "front", "at": [120, 180], "bodies": [2] })),
            ]
        ],
    );
    t.task(
        "draw-10-four-views",
        "drawings",
        "Four standard views on an A4 sheet, first-angle",
        cat![
            bx(),
            [
                save(
                    e("create_drawing", json!({ "bodies": [2], "sheet": "A4", "four_views": true, "first_angle": true })),
                    "dr",
                    "/drawing"
                ),
                e("export_drawing", json!({ "drawing": "$dr", "path": "$TMP/f.pdf" })),
                expect(json!({ "file": "$TMP/f.pdf" })),
            ]
        ],
    );
    t.task(
        "vis-06-environment",
        "visualization",
        "Pick another studio environment and turn the light",
        cat![
            bx(),
            [
                c("ui.toggle", json!({ "panel": "visualization" })),
                e("visual_settings", json!({ "environment": 3, "light_rotation": 90, "sun_elevation": 40 })),
                expect(json!({ "bodies": 1 })),
            ]
        ],
    );
    t.task(
        "vis-07-custom-material",
        "visualization",
        "A custom material from a picture, on a body",
        cat![
            bx(),
            [
                c("ui.capture", json!({ "path": "$TMP/wood.png" })),
                save(e("import_material", json!({ "path": "$TMP/wood.png" })), "m", "/image"),
                e("set_appearance", json!({ "items": [2], "custom": "$m" })),
                returns(inspect(), json!({ "at": "/bodies/0/appearance/texture", "is": "$m" })),
            ]
        ],
    );
    t.task(
        "vis-08-depth-of-field",
        "visualization",
        "Depth of field on a point of the model",
        cat![bx(), [e("visual_settings", json!({ "aperture": 2.8, "blur": 1.0, "focus": [10, 5, 10] })), expect(json!({ "bodies": 1 }))]],
    );
}

fn all() -> Tasks {
    let mut t = Tasks(BTreeMap::new());
    sketch_tasks(&mut t);
    constraint_tasks(&mut t);
    modeling_tasks(&mut t);
    transform_tasks(&mut t);
    construction_and_selection_tasks(&mut t);
    history_tasks(&mut t);
    items_view_vis_draw_io_tasks(&mut t);
    project_and_breadth_tasks(&mut t);
    coverage_tasks(&mut t);
    t
}

/// `cargo xtask bench-tasks [OUT_DIR]` (default bench/tasks). Only files that changed are written.
pub fn write(args: &[String]) -> Result<()> {
    let out = args.first().map_or_else(|| root().join("bench/tasks"), std::path::PathBuf::from);
    std::fs::create_dir_all(&out)?;
    let tasks = all();
    let wanted: BTreeMap<String, String> =
        tasks.0.iter().map(|(name, steps)| (format!("{name}.jsonl"), steps.iter().map(|s| format!("{s}\n")).collect::<String>())).collect();
    for entry in std::fs::read_dir(&out)? {
        let p = entry?.path();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.ends_with(".jsonl") && !wanted.contains_key(&name) {
            std::fs::remove_file(&p)?;
        }
    }
    let mut changed = 0;
    for (file, text) in &wanted {
        let p = out.join(file);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&p, text)?;
            changed += 1;
        }
    }
    println!("{} tasks, {changed} written → {}", tasks.0.len(), out.display());
    Ok(())
}

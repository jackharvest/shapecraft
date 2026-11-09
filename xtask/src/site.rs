//! The website's and home screen's pictures, all from our own app:
//!
//!   cargo xtask site-scenes OUT_DIR   snap scenarios (OUT_DIR/<name>.jsonl) recording the site's clips
//!   cargo xtask site-media SCRATCH    runs them with shapecraft-snap and encodes website/media (gifski, ffmpeg)
//!   cargo xtask home-art SCRATCH [tut] [new]   the home screen's pictures (assets/home/*.png) via shapecraft-cli
//!   cargo xtask site-shot URL OUT_DIR W H STEPS.json   drives headless Chromium (Playwright's cached shell) to
//!       look at a page: steps {"shot":name} {"fullshot":name,"scale":s} {"scroll":y} {"wait":ms} {"eval":js}
//!       {"click":[x,y]} {"drag":[[x0,y0],[x1,y1]],"steps":n,"hold":true} {"up":[x,y]}; THEME=dark|light
//!
//! Every frame is a `shot` of the real UI driven offscreen; coordinates are the 1840 × 1324 window the other
//! scenarios use, or model points (mm) that snap puts on screen through the camera of the moment.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::util::{root, run, s, target};

const SNAP_SIZE: [u32; 2] = [1840, 1324];

fn ex(command: &str, params: Value) -> Value {
    json!({ "control": "engine.execute", "params": { "command": command, "params": params } })
}

fn ex0(command: &str) -> Value {
    ex(command, json!({}))
}

fn edit_step(step: usize, params: Value) -> Value {
    json!({ "control": "engine.execute", "params": { "command": "edit_step", "params": { "step": step, "params": params } } })
}

/// Python's round(x, n): the nearest decimal, ties to even on the exact value.
fn round_to(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse().unwrap_or(x)
}

/// A model point (mm): snap puts it on screen through the camera of the moment.
fn w(x: f64, y: f64, z: f64) -> Value {
    json!([round_to(x, 3), round_to(y, 3), round_to(z, 3)])
}

fn wv(p: [f64; 3]) -> Value {
    w(p[0], p[1], p[2])
}

fn ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

struct Scene {
    name: &'static str,
    steps: Vec<Value>,
    n: usize,
}

impl Scene {
    fn new(name: &'static str) -> Self {
        Self { name, steps: vec![json!({ "size": SNAP_SIZE }), json!({ "control": "project.new" }), json!({ "wait": 0.3 })], n: 0 }
    }

    /// A feature clip: the ground grid on, the part already there, the camera fitted to it.
    fn feature(name: &'static str, setup: Vec<Value>) -> Self {
        let mut s = Self::new(name);
        s.add([json!({ "control": "ui.toggle", "params": { "panel": "grid" } })]);
        s.add(setup);
        s.add([ex0("deselect_all"), json!({ "control": "ui.view", "params": { "view": "default" } }), json!({ "wait": 0.9 })]);
        s
    }

    fn add(&mut self, steps: impl IntoIterator<Item = Value>) {
        self.steps.extend(steps);
    }

    /// `hold` = how many frames this picture stays up.
    fn shot(&mut self, hold: usize) {
        for _ in 0..hold {
            self.steps.push(json!({ "shot": format!("f{:04}", self.n) }));
            self.n += 1;
        }
    }

    /// The pointer travelling between two screen points (eased), a frame at each stop.
    fn glide(&mut self, a: [f64; 2], b: [f64; 2], frames: usize) {
        for i in 1..=frames {
            let t = ease(i as f64 / frames as f64);
            let p = [(a[0] + (b[0] - a[0]) * t).round_ties_even() as i64, (a[1] + (b[1] - a[1]) * t).round_ties_even() as i64];
            self.add([json!({ "hover": p }), json!({ "wait": 0.02 })]);
            self.shot(1);
        }
    }

    /// The same between two model points.
    fn wglide(&mut self, a: [f64; 3], b: [f64; 3], frames: usize) {
        for i in 1..=frames {
            let t = ease(i as f64 / frames as f64);
            self.add([json!({ "hover": wv([0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t)) }), json!({ "wait": 0.02 })]);
            self.shot(1);
        }
    }

    fn text(&self) -> String {
        self.steps.iter().map(|s| format!("{s}\n")).collect()
    }
}

/// A mounting plate with rounded corners and four holes.
fn plate() -> Vec<Value> {
    let (w_, h, t, hole) = (72.0, 46.0, 8.0, 3.2);
    let mut v = vec![
        ex("sketch", json!({ "plane": "xy" })),
        ex("rectangle", json!({ "center": [0, 0], "size": [w_, h] })),
        ex("sketch_fillet", json!({ "radius": 7 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 1, "region": 0, "distance": t })),
        ex("sketch", json!({ "origin": [0, 0, t], "normal": [0, 0, 1] })),
    ];
    for x in [-w_ / 2.0 + 8.0, w_ / 2.0 - 8.0] {
        for y in [-h / 2.0 + 8.0, h / 2.0 - 8.0] {
            v.push(ex("circle", json!({ "center": [-y, x], "radius": hole })));
        }
    }
    v.push(ex0("exit_sketching"));
    v.push(ex("extrude", json!({ "sketch": 3, "regions": [0, 1, 2, 3], "distance": -t, "result": "subtract" })));
    v
}

fn finish() -> [Value; 7] {
    [
        json!({ "release": true }),
        json!({ "wait": 0.4 }),
        json!({ "key": "Enter" }),
        json!({ "wait": 0.2 }),
        json!({ "key": "Escape" }),
        ex0("deselect_all"),
        json!({ "hover": w(-60.0, -70.0, 0.0) }),
    ]
}

/// Sketching straight onto the top of a part: a rectangle, typed to size, then a circle beside it.
fn sketch_rect() -> Scene {
    let mut setup = plate();
    setup.push(ex("sketch", json!({ "body": 2, "face": "@face:2:0,0,8" })));
    let mut s = Scene::feature("sketch", setup);
    s.shot(6);
    let (a, b) = ([-24.0, -11.0, 8.0], [6.0, 11.0, 8.0]);
    s.add([json!({ "hover": w(-30.0, -16.0, 8.0) }), json!({ "key": "R" }), json!({ "wait": 0.3 })]);
    s.wglide([-30.0, -16.0, 8.0], a, 6);
    s.add([json!({ "click": wv(a) })]);
    s.shot(2);
    s.wglide(a, b, 12);
    s.shot(3);
    for ch in ["2", "8"] {
        s.add([json!({ "text": ch }), json!({ "wait": 0.1 })]);
        s.shot(2);
    }
    s.add([json!({ "key": "Tab" }), json!({ "wait": 0.1 })]);
    s.shot(2);
    for ch in ["2", "0"] {
        s.add([json!({ "text": ch }), json!({ "wait": 0.1 })]);
        s.shot(2);
    }
    s.add([json!({ "key": "Enter" }), json!({ "wait": 0.3 }), ex0("deselect_all")]);
    s.shot(5);
    let c = [17.0, 0.0, 8.0];
    s.add([json!({ "key": "C" }), json!({ "wait": 0.2 })]);
    s.wglide([4.0, 9.0, 8.0], c, 6);
    s.add([json!({ "click": wv(c) })]);
    s.shot(2);
    s.wglide(c, [23.0, 2.0, 8.0], 8);
    s.add([json!({ "text": "7" }), json!({ "wait": 0.1 })]);
    s.shot(3);
    s.add([json!({ "key": "Enter" }), json!({ "wait": 0.3 }), ex0("deselect_all")]);
    s.shot(16);
    s
}

/// A boss pulled up out of the plate; then a hexagon pushed right through it, cutting a hole of its shape, until the
/// Result badge is switched to New Body: the hole fills back in and the hexagon is a part of its own.
fn push_pull() -> Scene {
    let mut setup = plate();
    setup.extend([
        ex("sketch", json!({ "body": 2, "face": "@face:2:0,0,8" })),
        ex("circle", json!({ "center": [-16, 0], "radius": 10 })),
        ex0("exit_sketching"),
        ex("sketch", json!({ "body": 2, "face": "@face:2:0,0,8" })),
        ex("polygon", json!({ "center": [15, 0], "radius": 9, "sides": 6, "angle": 90 })),
        ex0("exit_sketching"),
    ]);
    let mut s = Scene::feature("extrude", setup);
    s.shot(5);
    // The arrow comes up where the region is clicked: off the circle's centre point, which would pick the point.
    let (boss, hexa) = ([-12.0, 5.0, 8.0], [13.0, 4.0, 8.0]);
    s.wglide([10.0, -30.0, 8.0], boss, 7);
    s.add([json!({ "click": wv(boss) }), json!({ "wait": 0.4 })]);
    s.shot(4);
    s.add([json!({ "hold": [wv(boss), w(-12.0, 5.0, 8.5)] }), json!({ "wait": 0.05 })]);
    s.wglide([-12.0, 5.0, 8.5], [-12.0, 5.0, 26.0], 16);
    s.shot(4);
    s.add(finish());
    s.add([json!({ "wait": 0.3 })]);
    s.shot(4);
    s.wglide([-12.0, 5.0, 26.0], hexa, 7);
    s.add([json!({ "click": wv(hexa) }), json!({ "wait": 0.4 })]);
    s.shot(4);
    s.add([json!({ "hold": [wv(hexa), w(13.0, 4.0, 7.6)] }), json!({ "wait": 0.05 })]);
    s.wglide([13.0, 4.0, 7.6], [13.0, 4.0, -4.0], 16);
    s.shot(6);
    s.add([json!({ "release": true }), json!({ "wait": 0.4 })]);
    s.shot(6);
    // Its Result badge (above the distance pill; screen points, measured for this camera) → New Body.
    let (badge, row) = ([1060.0, 687.0], [1145.0, 702.0]);
    s.glide([1060.0, 735.0], badge, 6);
    s.add([json!({ "click": [1060, 687] }), json!({ "wait": 0.3 })]);
    s.shot(5);
    s.glide(badge, row, 6);
    s.add([json!({ "click": [1145, 702] }), json!({ "wait": 0.4 })]);
    s.shot(8);
    s.add([
        json!({ "key": "Enter" }),
        json!({ "wait": 0.2 }),
        json!({ "key": "Escape" }),
        ex0("deselect_all"),
        json!({ "hover": w(-60.0, -70.0, 0.0) }),
        json!({ "wait": 0.3 }),
    ]);
    s.shot(6);
    // The hexagon (body 6) lifts out of the whole plate: a Translate step, its offset eased up frame by frame.
    s.add([ex("translate", json!({ "by": [0, 0, 0], "items": [6] })), ex0("deselect_all")]);
    for i in 1..=14 {
        let t = ease(f64::from(i) / 14.0);
        s.add([edit_step(8, json!({ "by": [0, 0, round_to(24.0 * t, 2)] })), json!({ "wait": 0.03 })]);
        s.shot(1);
    }
    s.shot(16);
    s
}

/// An angle bracket: a holed base, an upright with a slot, and a gusset between them.
fn bracket() -> Vec<Value> {
    vec![
        ex("sketch", json!({ "plane": "xy" })),
        ex("rectangle", json!({ "center": [0, 0], "size": [64, 40] })),
        ex("sketch_fillet", json!({ "radius": 5 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 1, "region": 0, "distance": 6 })),
        ex("sketch", json!({ "origin": [0, 0, 6], "normal": [0, 0, 1] })),
        ex("rectangle", json!({ "corner": [-20, -32], "opposite": [-14, 32] })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 3, "region": 0, "distance": 34, "result": "union" })),
        ex("sketch", json!({ "origin": [0, 0, 6], "normal": [0, 0, 1] })),
        ex("circle", json!({ "center": [-4, -20], "radius": 3.4 })),
        ex("circle", json!({ "center": [-4, 20], "radius": 3.4 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 4, "regions": [0, 1], "distance": -6, "result": "subtract" })),
    ]
}

/// Rounding the inside corner of a bracket, then its top edge.
fn fillet() -> Scene {
    let mut s = Scene::feature("fillet", bracket());
    s.shot(5);
    // The inside corner: click the edge, then pull its arrow out along the corner's bisector.
    let inner = [6.0, 14.0, 6.0];
    s.wglide([20.0, 2.0, 6.0], inner, 8);
    s.add([json!({ "click": wv(inner) }), json!({ "wait": 0.5 })]);
    s.shot(4);
    s.add([json!({ "hold": [w(4.5, 13.2, 6.8), w(4.5, 12.8, 7.2)] }), json!({ "wait": 0.05 })]);
    s.wglide([4.5, 12.8, 7.2], [4.5, 7.0, 13.0], 16);
    s.shot(4);
    s.add(finish());
    s.add([json!({ "wait": 0.3 })]);
    s.shot(4);
    // Then the upright's top front edge; its arrow sits a little along the edge and points down the front face.
    let top = [8.0, 14.0, 40.0];
    s.wglide([4.5, 7.0, 13.0], top, 8);
    s.add([json!({ "click": wv(top) }), json!({ "wait": 0.5 })]);
    s.shot(4);
    s.add([json!({ "hold": [w(4.7, 14.0, 40.2), w(4.5, 14.0, 39.6)] }), json!({ "wait": 0.05 })]);
    s.wglide([4.5, 14.0, 39.6], [1.5, 14.0, 33.0], 14);
    s.shot(4);
    s.add(finish());
    s.add([json!({ "wait": 0.3 })]);
    s.shot(16);
    s
}

/// Arrange: one arm on a hub, a circular pattern that grows to eight, then the arm's own Rotate step turned, and
/// every linked copy swings with it. The camera drifts round the whole time.
fn rotate() -> Scene {
    let setup = vec![
        ex("sketch", json!({ "plane": "xy" })),
        ex("circle", json!({ "center": [0, 0], "radius": 30 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 1, "region": 0, "distance": 8 })),
        ex("sketch", json!({ "origin": [0, 0, 8], "normal": [0, 0, 1] })),
        ex("circle", json!({ "center": [0, 0], "radius": 8 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 3, "region": 0, "distance": 8, "result": "new" })),
        ex("sketch", json!({ "origin": [0, 0, 16], "normal": [0, 0, 1] })),
        ex("rectangle", json!({ "corner": [-5, -6], "opposite": [5, 46] })),
        ex("sketch_fillet", json!({ "radius": 5 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 5, "region": 0, "distance": 5, "result": "new" })),
        ex("sketch", json!({ "origin": [0, 0, 21], "normal": [0, 0, 1] })),
        ex("circle", json!({ "center": [0, 40], "radius": 2.6 })),
        ex0("exit_sketching"),
        ex("extrude", json!({ "sketch": 7, "region": 0, "distance": -5, "result": "subtract" })),
    ];
    let mut s = Scene::feature("rotate", setup);
    let (arm, rot_step, pat_step, total) = (6, 8, 9, 96usize);
    let mut k = 0usize;
    let mut frame = |s: &mut Scene, steps: Vec<Value>, n: usize| {
        let mut steps = steps;
        for _ in 0..n {
            let t = k as f64 / total as f64;
            k += 1;
            let cam = json!({ "control": "ui.camera", "params": { "target": [0, 0, 12], "yaw": -0.95 + 0.9 * t, "pitch": 0.62 - 0.08 * t, "distance": 250, "ortho": 0 } });
            s.add(std::mem::take(&mut steps));
            s.add([cam, json!({ "wait": 0.03 })]);
            s.shot(1);
        }
    };
    let pick_arm = || ex("select", json!({ "picks": [{ "kind": "body", "body": arm }] }));
    frame(&mut s, vec![], 6);
    frame(&mut s, vec![pick_arm()], 5);
    // The arm's turn about the hub (0° for now) and the pattern after it, so editing the turn moves every copy.
    frame(
        &mut s,
        vec![
            ex("rotate", json!({ "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 0, "items": [arm] })),
            ex("pattern_circular", json!({ "origin": [0, 0, 0], "axis": [0, 0, 1], "count": 2, "angle": 360, "items": [arm] })),
            pick_arm(),
        ],
        4,
    );
    for count in 3..=8 {
        frame(&mut s, vec![edit_step(pat_step, json!({ "count": count }))], 4);
    }
    frame(&mut s, vec![ex0("deselect_all")], 6);
    for i in 1..=24 {
        let t = ease(f64::from(i) / 24.0);
        frame(&mut s, vec![edit_step(rot_step, json!({ "angle": round_to(40.0 * t, 2) }))], 1);
    }
    let left = total.saturating_sub(6 + 5 + 4 + 6 * 4 + 6 + 24);
    frame(&mut s, vec![], left);
    s
}

// ------------------------------------------------------------------ the handheld (speed-lapse model)

const LID_OPEN: f64 = 32.0;
const NOT_IN_HISTORY: [&str; 7] = ["select", "deselect_all", "exit_sketching", "rectangle", "circle", "sketch_fillet", "text"];

/// A sketch on a z plane has u along -Y and v along +X, so world (x, y) is (-y, x) there.
fn p(x: f64, y: f64) -> Value {
    json!([-y, x])
}

fn sz(wd: f64, h: f64) -> Value {
    json!([h, wd])
}

fn plane_at(z: f64) -> Value {
    json!({ "origin": [0, 0, z], "normal": [0, 0, 1] })
}

/// A clamshell handheld console, modeled step by step with ShapeCraft's own commands. Millimetres; the lower half
/// lies on XY with its hinge along X at the back (y = 80). Item ids are the ones the commands hand out in order.
fn handheld_steps() -> Vec<(&'static str, Value)> {
    let cobalt = json!({ "name": "Cobalt", "color": [0.10, 0.23, 0.78], "metal": 0.15, "gloss": 0.55 });
    let graphite = json!({ "name": "Graphite", "color": [0.13, 0.13, 0.15], "metal": 0.0, "gloss": 0.35 });
    let glass = json!({ "name": "Screen", "color": [0.02, 0.02, 0.03], "metal": 0.0, "gloss": 0.95 });
    let holes: Vec<[f64; 2]> = (0..3).flat_map(|j| (0..4).map(move |i| [21.0 + 4.2 * f64::from(i), 9.0 + 4.2 * f64::from(j)])).collect();
    let mut s: Vec<(&'static str, Value)> = vec![
        // 1 · lower half
        ("sketch", json!({ "plane": "xy" })),
        ("rectangle", json!({ "center": [0, 40], "size": [84, 80] })),
        ("sketch_fillet", json!({ "radius": 8 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 1, "region": 0, "distance": 11 })),
        ("select", json!({ "picks": [{ "kind": "body", "body": 2 }] })),
        ("fillet", json!({ "radius": 2 })),
        // 2 · hinge
        ("sketch", json!({ "origin": [-31, 0, 0], "normal": [1, 0, 0] })),
        ("circle", json!({ "center": [81, 11], "radius": 6 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 3, "region": 0, "distance": 62, "result": "new" })),
        // 3 · lid, lying flat for now
        ("sketch", json!({ "plane": "xy" })),
        ("rectangle", json!({ "center": [0, 122], "size": [84, 80] })),
        ("sketch_fillet", json!({ "radius": 8 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 5, "region": 0, "distance": 10, "result": "new" })),
        ("select", json!({ "picks": [{ "kind": "body", "body": 6 }] })),
        ("fillet", json!({ "radius": 2 })),
        // 4 · screen bezel and glass
        ("sketch", plane_at(10.0)),
        ("rectangle", json!({ "center": p(0.0, 127.0), "size": sz(68.0, 50.0) })),
        ("sketch_fillet", json!({ "radius": 4 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 7, "region": 0, "distance": -1.2, "result": "subtract" })),
        ("sketch", plane_at(8.8)),
        ("rectangle", json!({ "center": p(0.0, 128.0), "size": sz(58.0, 39.0) })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 8, "region": 0, "distance": 0.6, "result": "new" })),
        // 5 · d-pad well and cross
        ("sketch", plane_at(11.0)),
        ("circle", json!({ "center": p(-24.0, 40.0), "radius": 11.5 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 10, "region": 0, "distance": -1.2, "result": "subtract" })),
        ("sketch", plane_at(9.8)),
        ("rectangle", json!({ "center": p(-24.0, 40.0), "size": sz(19.0, 6.4) })),
        ("rectangle", json!({ "center": p(-24.0, 40.0), "size": sz(6.4, 19.0) })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 11, "regions": [0, 1, 2, 3, 4], "distance": 3.2, "result": "new" })),
        // 6 · A and B
        ("sketch", plane_at(10.0)),
        ("circle", json!({ "center": p(19.0, 34.0), "radius": 4.8 })),
        ("circle", json!({ "center": p(31.0, 43.0), "radius": 4.8 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 13, "regions": [0, 1], "distance": 3.2, "result": "new" })),
        // 7 · start, select, speaker
        ("sketch", plane_at(10.2)),
        ("circle", json!({ "center": p(-29.0, 15.0), "radius": 2.2 })),
        ("circle", json!({ "center": p(-19.0, 15.0), "radius": 2.2 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 15, "regions": [0, 1], "distance": 1.6, "result": "new" })),
        ("sketch", plane_at(11.0)),
    ];
    for h in &holes {
        s.push(("circle", json!({ "center": p(h[0], h[1]), "radius": 0.9 })));
    }
    s.extend([
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 17, "regions": (0..holes.len()).collect::<Vec<_>>(), "distance": -1.5, "result": "subtract" })),
        // 8 · shoulder buttons at the back corners
        ("sketch", plane_at(3.0)),
        ("rectangle", json!({ "corner": p(32.5, 70.0), "opposite": p(41.4, 81.5) })),
        ("rectangle", json!({ "corner": p(-41.4, 70.0), "opposite": p(-32.5, 81.5) })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 18, "regions": [0, 1], "distance": 6, "result": "new" })),
        // 9 · lettering under the screen
        ("sketch", plane_at(10.0)),
        ("text", json!({ "at": p(-14.0, 90.5), "text": "SHAPECRAFT", "size": 4.2, "angle": 90 })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 20, "regions": (0..10).collect::<Vec<_>>(), "distance": -0.5, "result": "subtract" })),
        // 10 · colour
        ("set_appearance", json!({ "items": [2, 4, 6], "appearance": cobalt })),
        ("set_appearance", json!({ "items": [12, 14, 16, 19], "appearance": graphite })),
        ("set_appearance", json!({ "items": [9], "appearance": glass })),
        ("deselect_all", json!({})),
        // 11 · open the lid (the speed-lapse eases this angle in with edit_step)
        ("rotate", json!({ "origin": [0, 81, 11], "axis": [1, 0, 0], "angle": LID_OPEN, "items": [6, 9] })),
    ]);
    s
}

/// Speed-lapse: the handheld built from nothing, the camera drifting round it, the lid snapping shut and opening
/// again, then the parts flying off so the loop ends on the empty grid it starts on.
fn handheld() -> Scene {
    let mut s = Scene::new("handheld");
    let steps = handheld_steps();
    let target = json!([0, 70, 12]);
    // Frames land after a sketch's Exit, each solid step and each colour, so nothing on screen flickers.
    let weight = |c: &str| match c {
        "exit_sketching" => 1,
        "extrude" | "fillet" => 3,
        "set_appearance" => 2,
        _ => 0,
    };
    let plan: Vec<usize> = steps.iter().map(|(c, _)| weight(c)).collect();
    let total = plan.iter().sum::<usize>() + 70;
    s.add([json!({ "control": "ui.toggle", "params": { "panel": "grid" } })]);
    let (start_yaw, start_pitch) = (-1.15, 0.62);
    let mut last = (start_yaw, start_pitch);
    let mut k = 0usize;
    let at = |last: &mut (f64, f64), yaw: f64, pitch: f64| {
        *last = (yaw, pitch);
        json!({ "control": "ui.camera", "params": { "target": target, "yaw": yaw, "pitch": pitch, "distance": 330, "ortho": 0 } })
    };
    let cam = |k: &mut usize, last: &mut (f64, f64), extra: f64| {
        let t = *k as f64 / total as f64;
        *k += 1;
        at(last, start_yaw + 0.85 * t + extra, start_pitch - 0.12 * t)
    };
    let c0 = cam(&mut k, &mut last, 0.0);
    s.add([c0, json!({ "wait": 0.2 })]);
    s.shot(4);
    for ((c, p), n) in steps.iter().zip(&plan) {
        if *c == "rotate" {
            break;
        }
        s.add([ex(c, p.clone())]);
        if matches!(*c, "extrude" | "fillet") {
            s.add([ex0("deselect_all")]);
        }
        for _ in 0..*n {
            let cm = cam(&mut k, &mut last, 0.0);
            s.add([cm, json!({ "wait": 0.05 })]);
            s.shot(1);
        }
    }
    // The lid: flat, shut, then open, by editing the Rotate step's angle frame by frame.
    let Some((_, rot)) = steps.iter().find(|(c, _)| *c == "rotate") else { return s };
    let mut flat = rot.clone();
    flat["angle"] = json!(0);
    s.add([ex("rotate", flat), ex0("deselect_all")]);
    let step_index = steps.iter().filter(|(c, _)| !NOT_IN_HISTORY.contains(c)).count() - 1;
    s.shot(3);
    let lid = |s: &mut Scene, k: &mut usize, last: &mut (f64, f64), a: f64, b: f64, frames: usize| {
        for i in 1..=frames {
            let t = ease(i as f64 / frames as f64);
            let cm = cam(k, last, 0.0);
            s.add([edit_step(step_index, json!({ "angle": a + (b - a) * t })), cm, json!({ "wait": 0.05 })]);
            s.shot(1);
        }
    };
    lid(&mut s, &mut k, &mut last, 0.0, 178.0, 16);
    s.shot(4);
    lid(&mut s, &mut k, &mut last, 178.0, LID_OPEN, 18);
    // A last slow turn to show it off.
    for i in 0..24 {
        let cm = cam(&mut k, &mut last, 0.02 * f64::from(i));
        s.add([cm, json!({ "wait": 0.05 })]);
        s.shot(1);
    }
    s.shot(6);
    // The parts fly off in turn while the camera swings back the short way round to where it started.
    let groups: [(Vec<u32>, [f64; 3]); 4] = [
        (vec![12, 14, 16], [0.0, -60.0, 520.0]),
        (vec![6, 9], [0.0, 420.0, 380.0]),
        (vec![4, 19], [-520.0, 120.0, 160.0]),
        (vec![2], [480.0, -300.0, 120.0]),
    ];
    let first = step_index + 1;
    for (items, _) in &groups {
        s.add([ex("translate", json!({ "by": [0, 0, 0], "items": items }))]);
    }
    s.add([ex0("deselect_all")]);
    let (frames, lag, (y0, p0)) = (30usize, 4usize, last);
    let dy = (start_yaw - y0 + PI).rem_euclid(2.0 * PI) - PI;
    for f in 1..=frames {
        for (g, (_, far)) in groups.iter().enumerate() {
            let u = ((f as f64 - (g * lag) as f64) / (frames - 3 * lag) as f64).clamp(0.0, 1.0);
            let e = u * u * u;
            s.add([edit_step(first + g, json!({ "by": far.map(|c| round_to(c * e, 2)) }))]);
        }
        let t = ease(f as f64 / frames as f64);
        let a = at(&mut last, y0 + dy * t, p0 + (start_pitch - p0) * t);
        s.add([a, json!({ "wait": 0.05 })]);
        s.shot(1);
    }
    s.shot(2);
    s
}

fn stills() -> Scene {
    let mut s = Scene::new("stills");
    s.steps = vec![
        json!({ "size": SNAP_SIZE }),
        json!({ "control": "ui.landing", "params": { "section": "whats_new" } }),
        json!({ "wait": 0.6 }),
        json!({ "shot": "home-whats-new" }),
        json!({ "control": "ui.landing", "params": { "section": "get_started" } }),
        json!({ "wait": 0.6 }),
        json!({ "shot": "home-get-started" }),
        json!({ "control": "ui.landing", "params": { "section": "tutorials" } }),
        json!({ "wait": 0.6 }),
        json!({ "shot": "home-tutorials" }),
        json!({ "control": "project.new" }),
        json!({ "wait": 0.3 }),
        ex("sketch", json!({ "plane": "xy" })),
        ex("rectangle", json!({ "center": [0, 0], "size": [40, 24] })),
        ex("circle", json!({ "center": [10, 0], "radius": 5 })),
        ex("extrude", json!({ "sketch": 1, "region": 0, "distance": 12 })),
        ex0("deselect_all"),
        json!({ "control": "ui.view", "params": { "view": "default" } }),
        json!({ "wait": 0.8 }),
        json!({ "shot": "workspace" }),
    ];
    s
}

fn all_scenes() -> Vec<Scene> {
    vec![sketch_rect(), push_pull(), fillet(), rotate(), handheld(), stills()]
}

pub fn scenes(args: &[String]) -> Result<()> {
    let Some(out) = args.first() else { bail!("usage: cargo xtask site-scenes OUT_DIR") };
    let out = Path::new(out);
    std::fs::create_dir_all(out)?;
    for scene in all_scenes() {
        let path = out.join(format!("{}.jsonl", scene.name));
        std::fs::write(&path, scene.text())?;
        println!("{}", path.display());
    }
    // The handheld on its own, as shapecraft-cli runs it (`shapecraft-cli run handheld-model.jsonl --png out.png`).
    let model: String = handheld_steps().iter().map(|(c, p)| format!("{}\n", json!({ "command": c, "params": p }))).collect();
    std::fs::write(out.join("handheld-model.jsonl"), model)?;
    Ok(())
}

/// Records the site's GIFs, videos and stills from our own app, offscreen (no window opens). Needs a release build
/// of shapecraft-snap (SNAP=path to override), gifski and ffmpeg. Takes about 3 minutes.
pub fn media(args: &[String]) -> Result<()> {
    let Some(work) = args.first() else { bail!("usage: cargo xtask site-media SCRATCH_DIR") };
    let work = PathBuf::from(work);
    let snap = std::env::var("SNAP").map_or_else(|_| target().join("release/shapecraft-snap"), PathBuf::from);
    let media = root().join("website/media");
    let scn = work.join("scn");
    std::fs::create_dir_all(&scn)?;
    std::fs::create_dir_all(&media)?;
    scenes(&[s(&scn).to_string()])?;
    let record = |name: &str| -> Result<PathBuf> {
        let dir = work.join(name);
        crate::util::clear(&dir)?;
        std::fs::create_dir_all(&dir)?;
        let out = std::process::Command::new(&snap)
            .arg(scn.join(format!("{name}.jsonl")))
            .arg(&dir)
            .output()
            .context("couldn't start shapecraft-snap")?;
        if !out.status.success() {
            bail!("shapecraft-snap {name} failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(dir)
    };
    let frames = |dir: &Path| -> Result<Vec<String>> {
        let mut v: Vec<String> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('f')) && p.extension().is_some_and(|x| x == "png"))
            .map(|p| s(&p).to_string())
            .collect();
        v.sort();
        Ok(v)
    };
    let ffmpeg = |args: &[&str]| run("ffmpeg", &[&["-loglevel", "error", "-y"][..], args].concat());
    for name in ["sketch", "extrude", "fillet", "rotate"] {
        let dir = record(name)?;
        if name == "rotate" {
            // The camera moves in every frame of this one, so a video is a fifth of the size of the GIF.
            let pattern = dir.join("f%04d.png");
            let mp4 = media.join("rotate.mp4");
            ffmpeg(&[
                "-framerate",
                "15",
                "-i",
                s(&pattern),
                "-vf",
                "scale=920:-2,format=yuv420p",
                "-c:v",
                "libx264",
                "-crf",
                "24",
                "-preset",
                "slow",
                "-movflags",
                "+faststart",
                "-an",
                s(&mp4),
            ])?;
            ffmpeg(&["-i", s(&dir.join("f0040.png")), "-vf", "scale=920:-1", "-q:v", "4", s(&media.join("rotate.jpg"))])?;
            println!("{}", mp4.display());
            continue;
        }
        let gif = media.join(format!("{name}.gif"));
        let mut args = vec!["--quiet", "--fps", "15", "--width", "920", "--quality", "90", "-o", s(&gif)];
        let f = frames(&dir)?;
        args.extend(f.iter().map(String::as_str));
        run("gifski", &args)?;
        println!("{}", gif.display());
    }
    // The speed-lapse: a video for the page, a GIF for the README, a poster.
    let dir = record("handheld")?;
    let mp4 = media.join("handheld.mp4");
    ffmpeg(&[
        "-framerate",
        "15",
        "-i",
        s(&dir.join("f%04d.png")),
        "-vf",
        "scale=1380:-2,format=yuv420p",
        "-c:v",
        "libx264",
        "-crf",
        "22",
        "-preset",
        "slow",
        "-movflags",
        "+faststart",
        "-an",
        s(&mp4),
    ])?;
    let f = frames(&dir)?;
    let gif = media.join("handheld.gif");
    let mut args = vec!["--quiet", "--fps", "15", "--width", "720", "--quality", "80", "-o", s(&gif)];
    args.extend(f.iter().map(String::as_str));
    run("gifski", &args)?;
    if let Some(last) = f.last() {
        ffmpeg(&["-i", last, "-vf", "scale=1380:-1", "-q:v", "3", s(&media.join("handheld.jpg"))])?;
    }
    println!("{}", mp4.display());
    let dir = record("stills")?;
    let mut stills: Vec<PathBuf> =
        std::fs::read_dir(&dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
    stills.sort();
    for f in stills {
        let jpg = media.join(format!("{}.jpg", f.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        ffmpeg(&["-i", s(&f), "-vf", "scale=1380:-1", "-q:v", "3", s(&jpg)])?;
        println!("{}", jpg.display());
    }
    Ok(())
}

// ------------------------------------------------------------------ home screen art

/// Runs commands through shapecraft-cli; with `png`, renders the result (see-through, 680 × 876 = 2× the card art).
fn cli_run(cli: &Path, scratch: &Path, name: &str, cmds: &[(&str, Value)], png: bool) -> Result<String> {
    let path = scratch.join(format!("{name}.jsonl"));
    std::fs::write(&path, cmds.iter().map(|(c, p)| format!("{}\n", json!({ "command": c, "params": p }))).collect::<String>())?;
    let mut cmd = std::process::Command::new(cli);
    cmd.arg("run").arg(&path);
    if png {
        cmd.arg("--png").arg(root().join(format!("assets/home/{name}.png"))).args(["--size", "680x876", "--clear"]);
    }
    let out = cmd.output().with_context(|| format!("couldn't start {}", cli.display()))?;
    if !out.status.success() {
        bail!("{name}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The home screen's pictures (assets/home/*.png) from our own models: `tut` (the tutorials' cards), `new` (What's
/// new), or both. SHAPECRAFT_CLI picks the CLI (default: this build's release one); SHAPR_SAMPLE renders a .shapr we
/// made ourselves as new-shapr.png. new-cards.png is a crop of our own Fillet card rendered by shapecraft-snap.
pub fn home_art(args: &[String]) -> Result<()> {
    let Some(scratch) = args.first() else { bail!("usage: cargo xtask home-art SCRATCH_DIR [tut] [new]") };
    let scratch = PathBuf::from(scratch);
    std::fs::create_dir_all(&scratch)?;
    let cli = std::env::var("SHAPECRAFT_CLI").map_or_else(|_| target().join("release/shapecraft-cli"), PathBuf::from);
    let which: Vec<&str> = if args.len() > 1 { args[1..].iter().map(String::as_str).collect() } else { vec!["all"] };
    let want = |n: &str| which.contains(&"all") || which.contains(&n);
    let body2 = || json!({ "picks": [{ "kind": "body", "body": 2 }] });
    let stand: Vec<(&str, Value)> = vec![
        ("sketch", json!({ "plane": "xy" })),
        ("rectangle", json!({ "center": [0, 0], "size": [90, 70] })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 1, "region": 0, "distance": 8 })),
        ("sketch", json!({ "origin": [0, 0, 8], "normal": [0, 0, 1] })),
        ("rectangle", json!({ "center": [-28, 0], "size": [14, 90] })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 3, "region": 0, "distance": 70, "result": "union" })),
        ("sketch", json!({ "origin": [0, 0, 8], "normal": [0, 0, 1] })),
        ("rectangle", json!({ "center": [12, 0], "size": [6, 90] })),
        ("exit_sketching", json!({})),
        ("extrude", json!({ "sketch": 4, "region": 0, "distance": -4, "result": "subtract" })),
        ("select", body2()),
        ("fillet", json!({ "radius": 1.5 })),
        ("deselect_all", json!({})),
    ];
    if want("new") {
        let threads = vec![
            ("sketch", json!({"plane":"xy"})),
            ("circle", json!({"center":[0,0],"radius":4})),
            ("extrude", json!({"sketch":1,"region":0,"distance":15})),
            ("thread", json!({"body":2,"face":0,"preset":"M8x1.25","length":12})),
            ("sketch", json!({"plane":"xy"})),
            ("polygon", json!({"center":[0,0],"radius":6.5,"sides":6})),
            ("extrude", json!({"sketch":3,"region":0,"distance":-4,"result":"union"})),
            ("deselect_all", json!({})),
        ];
        cli_run(&cli, &scratch, "new-threads", &threads, true)?;
    }
    if want("tut") {
        let cup: Vec<(&str, Value)> = vec![
            ("sketch", json!({ "plane": "xy" })),
            ("circle", json!({ "center": [0, 0], "radius": 35 })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 1, "region": 0, "distance": 95 })),
            ("shell", json!({ "body": 2, "faces": [2], "thickness": 3 })),
            ("select", body2()),
            ("fillet", json!({ "radius": 1 })),
            ("deselect_all", json!({})),
        ];
        let tag: Vec<(&str, Value)> = vec![
            ("sketch", json!({ "plane": "xy" })),
            ("rectangle", json!({ "center": [0, 0], "size": [60, 30] })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 1, "region": 0, "distance": 4 })),
            ("sketch", json!({ "origin": [0, 0, 4], "normal": [0, 0, 1] })),
            ("circle", json!({ "center": [0, -22], "radius": 4 })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 3, "region": 0, "distance": -5, "result": "subtract" })),
            ("select", body2()),
            ("fillet", json!({ "radius": 1 })),
            ("deselect_all", json!({})),
        ];
        cli_run(&cli, &scratch, "tut-phone-stand", &stand, true)?;
        cli_run(&cli, &scratch, "tut-pencil-cup", &cup, true)?;
        cli_run(&cli, &scratch, "tut-key-tag", &tag, true)?;
    }
    if want("new") {
        let mut tutorials = stand.clone();
        tutorials.push((
            "set_appearance",
            json!({ "items": [2], "appearance": { "name": "Teal", "color": [0.16, 0.55, 0.62], "metal": 0, "gloss": 0.5 } }),
        ));
        cli_run(&cli, &scratch, "new-tutorials", &tutorials, true)?;
        let cube: Vec<(&str, Value)> = vec![
            ("sketch", json!({ "plane": "xy" })),
            ("rectangle", json!({ "center": [0, 0], "size": [40, 40] })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 1, "region": 0, "distance": 40 })),
            ("select", body2()),
            ("chamfer", json!({ "distance": 4 })),
            ("deselect_all", json!({})),
        ];
        cli_run(&cli, &scratch, "new-view-cube", &cube, true)?;
        let copies: Vec<(&str, Value)> = vec![
            ("sketch", json!({ "plane": "xy" })),
            ("rectangle", json!({ "center": [0, 0], "size": [24, 24] })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 1, "region": 0, "distance": 6 })),
            ("sketch", json!({ "origin": [0, 0, 6], "normal": [0, 0, 1] })),
            ("circle", json!({ "center": [0, 0], "radius": 6 })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 3, "region": 0, "distance": 14, "result": "union" })),
            ("select", body2()),
            ("fillet", json!({ "radius": 1 })),
            ("translate", json!({ "by": [0, 34, 0], "items": [2], "copy": true, "copies": 1 })),
            ("select_all", json!({})),
            ("translate", json!({ "by": [34, 0, 0], "copy": true, "copies": 1 })),
            ("deselect_all", json!({})),
        ];
        cli_run(&cli, &scratch, "new-copies", &copies, true)?;
        let mut plate: Vec<(&str, Value)> = vec![
            ("sketch", json!({ "plane": "xy" })),
            ("rectangle", json!({ "center": [0, 0], "size": [40, 84] })),
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 1, "region": 0, "distance": 4 })),
            ("sketch", json!({ "body": 2, "face": 5 })),
            ("text", json!({ "at": [8, -26], "text": "Hola", "size": 22, "angle": 90 })),
        ];
        // The lettering's regions, read back from the sketch.
        let mut probe = plate.clone();
        probe.push(("inspect", json!({})));
        let out = cli_run(&cli, &scratch, "_probe", &probe, false)?;
        let line = out.lines().rev().find_map(|l| l.strip_prefix("inspect: ")).context("no inspect output")?;
        let v: Value = serde_json::from_str(line)?;
        let regions: Vec<Value> = v["sketches"]
            .as_array()
            .and_then(|a| a.last())
            .and_then(|s| s["regions"].as_array())
            .map(|r| r.iter().map(|x| x["region"].clone()).collect())
            .unwrap_or_default();
        plate.extend([
            ("exit_sketching", json!({})),
            ("extrude", json!({ "sketch": 3, "regions": regions, "distance": 2.5, "result": "new" })),
            ("select", body2()),
            ("fillet", json!({ "radius": 1.2 })),
            ("deselect_all", json!({})),
            (
                "set_appearance",
                json!({ "items": [2], "appearance": { "name": "Plate", "color": [0.85, 0.85, 0.86], "metal": 0, "gloss": 0.4 } }),
            ),
        ]);
        cli_run(&cli, &scratch, "new-languages", &plate, true)?;
        if let Ok(shapr) = std::env::var("SHAPR_SAMPLE")
            && !shapr.is_empty()
        {
            cli_run(&cli, &scratch, "new-shapr", &[("import_file", json!({ "path": shapr }))], true)?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ a headless look at a page

/// Playwright's cached headless Chromium (CHROME overrides).
fn chrome() -> Result<PathBuf> {
    if let Ok(c) = std::env::var("CHROME") {
        return Ok(PathBuf::from(c));
    }
    let home = std::env::var("HOME").context("no HOME")?;
    let cache = PathBuf::from(home).join("Library/Caches/ms-playwright");
    for entry in std::fs::read_dir(&cache).with_context(|| format!("no Playwright cache at {}", cache.display()))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("chromium_headless_shell") {
            for arch in ["chrome-headless-shell-mac-arm64", "chrome-headless-shell-mac-x64"] {
                let p = entry.path().join(arch).join("chrome-headless-shell");
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }
    bail!("no headless Chromium found (set CHROME=path)")
}

/// A plain HTTP GET on localhost (the DevTools target list): the body, read by its Content-Length, never waiting
/// more than two seconds.
fn http_get(port: u16, path: &str) -> Result<String> {
    use std::io::{Read, Write};
    let mut sock = std::net::TcpStream::connect(("127.0.0.1", port))?;
    sock.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
    write!(sock, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = match sock.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let len = head
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()));
            if len.is_some_and(|len| body.len() >= len) {
                return Ok(body.to_string());
            }
        }
    }
    let text = String::from_utf8_lossy(&buf);
    Ok(text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

struct Cdp {
    ws: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    id: u64,
}

impl Cdp {
    fn send(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        self.ws.send(tungstenite::Message::text(json!({ "id": id, "method": method, "params": params }).to_string()))?;
        loop {
            if let tungstenite::Message::Text(t) = self.ws.read()? {
                let v: Value = serde_json::from_str(t.as_str())?;
                if v["id"].as_u64() == Some(id) {
                    return Ok(v);
                }
            }
        }
    }

    fn mouse(&mut self, kind: &str, x: f64, y: f64, buttons: u8) -> Result<()> {
        self.send(
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 }),
        )?;
        Ok(())
    }
}

fn pause(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub fn shot(args: &[String]) -> Result<()> {
    let [url, out, wd, ht, steps_path] = args else { bail!("usage: cargo xtask site-shot URL OUT_DIR W H STEPS.json") };
    let (wd, ht): (u32, u32) = (wd.parse()?, ht.parse()?);
    let steps: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(steps_path)?)?;
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let port = 9300 + (std::process::id() % 500) as u16;
    let mut proc = std::process::Command::new(chrome()?)
        .args([
            format!("--remote-debugging-port={port}"),
            "--hide-scrollbars".into(),
            "--autoplay-policy=no-user-gesture-required".into(),
            format!("--window-size={wd},{ht}"),
            "about:blank".into(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("couldn't start Chromium")?;
    let result = (|| -> Result<()> {
        let mut ws_url = None;
        for _ in 0..50 {
            if let Ok(body) = http_get(port, "/json/list")
                && let Ok(tabs) = serde_json::from_str::<Vec<Value>>(&body)
                && let Some(t) = tabs.iter().find(|t| t["type"] == "page")
            {
                ws_url = t["webSocketDebuggerUrl"].as_str().map(str::to_string);
                break;
            }
            pause(200);
        }
        let ws_url = ws_url.context("Chromium's DevTools didn't answer")?;
        let (ws, _) = tungstenite::connect(ws_url.as_str())?;
        if let tungstenite::stream::MaybeTlsStream::Plain(t) = ws.get_ref() {
            t.set_read_timeout(Some(std::time::Duration::from_secs(60)))?;
        }
        let mut cdp = Cdp { ws, id: 0 };
        cdp.send("Page.enable", json!({}))?;
        cdp.send("Emulation.setDeviceMetricsOverride", json!({ "width": wd, "height": ht, "deviceScaleFactor": 1, "mobile": false }))?;
        if let Ok(theme) = std::env::var("THEME") {
            cdp.send("Emulation.setEmulatedMedia", json!({ "features": [{ "name": "prefers-color-scheme", "value": theme }] }))?;
        }
        cdp.send("Page.navigate", json!({ "url": url }))?;
        pause(2500);
        for s in &steps {
            if let Some(ms) = s["wait"].as_u64() {
                pause(ms);
            }
            if let Some(y) = s.get("scroll").filter(|v| !v.is_null()) {
                cdp.send(
                    "Runtime.evaluate",
                    json!({ "expression": format!("document.documentElement.style.scrollBehavior='auto';scrollTo(0,{y})") }),
                )?;
                pause(900);
            }
            if let Some(js) = s["eval"].as_str() {
                let r = cdp.send("Runtime.evaluate", json!({ "expression": js, "returnByValue": true }))?;
                println!("{} {}", s["tag"].as_str().unwrap_or("eval"), r["result"]["result"]["value"]);
            }
            let xy = |v: &Value| (v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0));
            if let Some(d) = s["drag"].as_array().filter(|d| d.len() == 2) {
                let ((x0, y0), (x1, y1)) = (xy(&d[0]), xy(&d[1]));
                let n = s["steps"].as_u64().unwrap_or(12).max(1);
                cdp.mouse("mouseMoved", x0, y0, 0)?;
                cdp.mouse("mousePressed", x0, y0, 1)?;
                for i in 1..=n {
                    let f = i as f64 / n as f64;
                    cdp.mouse("mouseMoved", x0 + (x1 - x0) * f, y0 + (y1 - y0) * f, 1)?;
                    pause(16);
                }
                if s["hold"].as_bool() != Some(true) {
                    cdp.mouse("mouseReleased", x1, y1, 0)?;
                }
            }
            if s["up"].is_array() {
                let (x, y) = xy(&s["up"]);
                cdp.mouse("mouseReleased", x, y, 0)?;
            }
            if s["click"].is_array() {
                let (x, y) = xy(&s["click"]);
                cdp.mouse("mouseMoved", x, y, 0)?;
                cdp.mouse("mousePressed", x, y, 1)?;
                cdp.mouse("mouseReleased", x, y, 0)?;
            }
            let name = s["shot"].as_str().or_else(|| s["fullshot"].as_str());
            if let Some(name) = name {
                let mut p = json!({ "format": "png" });
                if s["fullshot"].is_string() {
                    let m = cdp.send("Page.getLayoutMetrics", json!({}))?;
                    let h = m["result"]["cssContentSize"]["height"].as_f64().unwrap_or(f64::from(ht));
                    p["captureBeyondViewport"] = json!(true);
                    p["clip"] = json!({ "x": 0, "y": 0, "width": wd, "height": h, "scale": s["scale"].as_f64().unwrap_or(0.5) });
                }
                let r = cdp.send("Page.captureScreenshot", p)?;
                let data = r["result"]["data"].as_str().context("no picture came back")?;
                use base64::Engine as _;
                std::fs::write(out.join(format!("{name}.png")), base64::engine::general_purpose::STANDARD.decode(data)?)?;
            }
        }
        let _ = cdp.ws.close(None);
        Ok(())
    })();
    let _ = proc.kill();
    let _ = proc.wait();
    result
}

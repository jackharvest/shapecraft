//! In-app tutorials, after the reference's "Redo the First Modeling Steps" (observed s30-020..095): a few short
//! steps, each starting from its own prepared scene so a mistake never carries on, with only the tools the step
//! needs, an instruction that moves on as you go, and Continue once the step's goal is met.
//!
//! The lessons are ours (a phone stand, a pencil cup, a key tag); none of the reference's models or text.
//! Every step also carries a scripted solution: the tests run it to prove the step can be finished, and the
//! workspace renders it as the "where you're headed" picture in the corner.

use std::collections::HashMap;

use glam::DVec3;
use serde_json::Value;
use shapecraft_engine::{SelectionKind, Session};

/// A sketch made from an origin and a normal (+Z) runs u along -Y and v along +X, so its points are given that
/// way; sketches opened on a top face read like the Top view instead (u = +X).
///
/// One engine command: its id and its JSON params. String values `"$sketch"`, `"$body"`, `"$face"` (the last
/// ones made) and `"$ref"` (the first body made: the finished part shown beside yours) are filled in as it runs.
pub type Cmd = (&'static str, &'static str);

pub struct Lesson {
    pub id: &'static str,
    /// The card's and the window's title.
    pub title: &'static str,
    pub minutes: u32,
    pub steps: &'static [Step],
    /// Builds the finished part for the card picture and the last step.
    pub finished: &'static [Cmd],
}

pub struct Step {
    /// What the step is about, shown large while it starts (the reference plays a captioned video here).
    pub intro: &'static str,
    pub setup: &'static [Cmd],
    pub view: View,
    /// The toolbar while not sketching, and while sketching.
    pub tools: &'static [Tool],
    pub sketch_tools: &'static [Tool],
    /// The instruction pill, in order; each moves on once its goal is met. The last one's goal finishes the step.
    pub hints: &'static [Hint],
    pub solution: &'static [Cmd],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    /// The default view, fitted to the scene.
    Default,
    /// Face-on to the open sketch.
    Sketch,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Sketch,
    Line,
    Rectangle,
    Circle,
    Arc,
    Extrude,
    Shell,
    Fillet,
    Visualization,
}

pub struct Hint {
    pub text: &'static str,
    /// A green tip by the pointer, as the reference shows them.
    pub tip: Option<&'static str>,
    pub goal: Goal,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    /// Choosing a plane, or a sketch is open.
    Sketching,
    SketchOpen,
    /// A sketch is open on a body's face (Space over a face).
    SketchOnFace,
    Lines(usize),
    Circles(usize),
    Picked(Pick),
    NewBody,
    /// The bodies grew by at least this many mm³.
    Grew(u32),
    /// The bodies lost at least this many mm³.
    Cut(u32),
    /// The bodies gained faces (a fillet or chamfer).
    Rounded,
    Orbited,
    /// A body moved at least 5 mm.
    Moved,
    Visualizing,
    /// A body that had no material has one.
    Finished,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pick {
    Region,
    Face,
    Edge,
    Body,
}

/// What the document looks like, to compare with how the step started.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub bodies: usize,
    pub volume: f64,
    pub faces: usize,
    pub lines: usize,
    pub circles: usize,
    pub finished: usize,
    pub centers: Vec<(u64, DVec3)>,
}

/// What the workspace is doing right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Live {
    pub picking_plane: bool,
    pub sketch_open: bool,
    pub sketch_on_face: bool,
    pub selection: SelectionKind,
    /// How far the camera has turned since the step started, in degrees.
    pub turned: f64,
    pub visualizing: bool,
}

pub fn snapshot(session: &Session) -> Snapshot {
    let doc = &session.doc;
    let mut s = Snapshot { bodies: doc.bodies.len(), ..Snapshot::default() };
    for b in &doc.bodies {
        s.volume += session.kernel.mass_props(&b.shape).map_or(0.0, |m| m.volume);
        s.faces += b.mesh.triangle_faces.iter().max().map_or(0, |m| *m as usize + 1);
        s.finished += usize::from(b.appearance.is_some());
        if let Some((lo, hi)) = b.mesh.bounds() {
            let c = (DVec3::from(lo.map(f64::from)) + DVec3::from(hi.map(f64::from))) / 2.0;
            s.centers.push((b.id.0, c));
        }
    }
    for sk in &doc.sketches {
        for e in &sk.entities {
            match e {
                shapecraft_doc::SketchEntity::Line { .. } => s.lines += 1,
                shapecraft_doc::SketchEntity::Rectangle { .. } => s.lines += 4,
                shapecraft_doc::SketchEntity::Circle { .. } => s.circles += 1,
                _ => {}
            }
        }
    }
    s
}

/// Whether a goal is met, from the step's starting snapshot, the current one and the workspace.
pub fn met(goal: Goal, base: &Snapshot, now: &Snapshot, live: &Live) -> bool {
    match goal {
        Goal::Sketching => live.picking_plane || live.sketch_open,
        Goal::SketchOpen => live.sketch_open,
        Goal::SketchOnFace => live.sketch_on_face,
        Goal::Lines(n) => now.lines >= base.lines + n,
        Goal::Circles(n) => now.circles >= base.circles + n,
        Goal::Picked(p) => {
            live.selection
                == match p {
                    Pick::Region => SelectionKind::Regions,
                    Pick::Face => SelectionKind::Faces,
                    Pick::Edge => SelectionKind::Edges,
                    Pick::Body => SelectionKind::Bodies,
                }
        }
        Goal::NewBody => now.bodies > base.bodies,
        Goal::Grew(v) => now.volume >= base.volume + v as f64,
        Goal::Cut(v) => now.volume <= base.volume - v as f64,
        Goal::Rounded => now.faces > base.faces,
        Goal::Orbited => live.turned >= 20.0,
        Goal::Moved => now.centers.iter().any(|(id, c)| base.centers.iter().any(|(b, d)| b == id && c.distance(*d) >= 5.0)),
        Goal::Visualizing => live.visualizing,
        Goal::Finished => now.finished > base.finished,
    }
}

/// Whether a goal can be checked from the document alone (the tests run those against the solutions).
pub fn on_document(goal: Goal) -> bool {
    matches!(
        goal,
        Goal::Lines(_) | Goal::Circles(_) | Goal::NewBody | Goal::Grew(_) | Goal::Cut(_) | Goal::Rounded | Goal::Moved | Goal::Finished
    )
}

/// Runs commands, filling in the `$` names from what the earlier ones made.
pub fn run(session: &mut Session, cmds: &[Cmd], vars: &mut HashMap<&'static str, Value>) -> Result<(), String> {
    for (id, params) in cmds {
        let mut p: Value = serde_json::from_str(params).map_err(|e| format!("{id}: {e}"))?;
        fill(&mut p, vars);
        let out = session.execute(id, &p).map_err(|e| format!("{id}: {e}"))?;
        for key in ["sketch", "body", "face"] {
            if let Some(v) = out.value.get(key).filter(|v| v.is_u64()) {
                vars.insert(key, v.clone());
                if key == "body" {
                    vars.entry("ref").or_insert_with(|| v.clone());
                }
            }
        }
    }
    Ok(())
}

fn fill(v: &mut Value, vars: &HashMap<&'static str, Value>) {
    match v {
        Value::String(s) if s.starts_with('$') => {
            if let Some(x) = vars.get(&s[1..]) {
                *v = x.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| fill(x, vars)),
        Value::Object(o) => o.values_mut().for_each(|x| fill(x, vars)),
        _ => {}
    }
}

pub fn lesson(id: &str) -> Option<&'static Lesson> {
    LESSONS.iter().find(|l| l.id == id)
}

pub const LESSONS: &[Lesson] = &[PHONE_STAND, PENCIL_CUP, KEY_TAG];

// ---- The phone stand -------------------------------------------------------------------------------------------

/// The finished stand, 140 mm to the left, so yours is built beside it (as the reference shows its own part).
const STAND_REF: &[Cmd] = &[
    ("sketch", r#"{"plane": "xy"}"#),
    ("rectangle", r#"{"center": [-140, 0], "size": [90, 70]}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 8}"#),
    ("sketch", r#"{"origin": [0, 0, 8], "normal": [0, 0, 1]}"#),
    ("rectangle", r#"{"center": [-28, -140], "size": [14, 90]}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 70, "result": "union"}"#),
    ("sketch", r#"{"origin": [0, 0, 8], "normal": [0, 0, 1]}"#),
    ("rectangle", r#"{"center": [12, -140], "size": [6, 90]}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": -4, "result": "subtract"}"#),
    ("select", r#"{"picks": [{"kind": "body", "body": "$ref"}]}"#),
    ("fillet", r#"{"radius": 1.5}"#),
    ("deselect_all", "{}"),
];

const STAND_SKETCH: &[Cmd] =
    &[("sketch", r#"{"plane": "xy"}"#), ("rectangle", r#"{"center": [0, 0], "size": [90, 70]}"#), ("exit_sketching", "{}")];
const STAND_BASE: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 8}"#)];
const STAND_BACK_SKETCH: &[Cmd] = &[
    ("sketch", r#"{"origin": [0, 0, 8], "normal": [0, 0, 1]}"#),
    ("rectangle", r#"{"center": [-28, 0], "size": [14, 90]}"#),
    ("exit_sketching", "{}"),
];
const STAND_BACK: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 70, "result": "union"}"#)];
const STAND_SLOT_SKETCH: &[Cmd] = &[
    ("sketch", r#"{"origin": [0, 0, 8], "normal": [0, 0, 1]}"#),
    ("rectangle", r#"{"center": [12, 0], "size": [6, 90]}"#),
    ("exit_sketching", "{}"),
];
const STAND_SLOT: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": -4, "result": "subtract"}"#)];

/// Joins command lists at compile time.
macro_rules! cmds {
    ($($part:expr),* $(,)?) => {{
        const LEN: usize = 0 $(+ $part.len())*;
        const ALL: [Cmd; LEN] = {
            let mut out: [Cmd; LEN] = [("", ""); LEN];
            let mut i = 0;
            $(
                let mut k = 0;
                while k < $part.len() {
                    out[i] = $part[k];
                    i += 1;
                    k += 1;
                }
            )*
            let _ = i;
            out
        };
        &ALL
    }};
}

const SKETCH_TOOLS: &[Tool] = &[Tool::Line, Tool::Arc, Tool::Rectangle, Tool::Circle];

const PHONE_STAND: Lesson = Lesson {
    id: "phone-stand",
    title: "Model a Phone Stand",
    minutes: 7,
    finished: cmds![STAND_REF],
    steps: &[
        Step {
            intro: "To draw the base, select Sketch.",
            setup: &[],
            view: View::Default,
            tools: &[Tool::Sketch],
            sketch_tools: SKETCH_TOOLS,
            hints: &[
                Hint { text: "Select Sketch", tip: None, goal: Goal::Sketching },
                Hint { text: "Select the ground plane", tip: None, goal: Goal::SketchOpen },
                Hint {
                    text: "Select Rectangle and draw the base",
                    tip: Some("Click one corner, then the opposite one"),
                    goal: Goal::Lines(4),
                },
            ],
            solution: &[("sketch", r#"{"plane": "xy"}"#), ("rectangle", r#"{"center": [0, 0], "size": [90, 70]}"#)],
        },
        Step {
            intro: "Pull the sketch up into a solid base.",
            setup: cmds![STAND_REF, STAND_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the rectangle's profile", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Pull up the arrow to make the base", tip: Some("Or type 8 and press Return"), goal: Goal::NewBody },
            ],
            solution: STAND_BASE,
        },
        Step {
            intro: "Turn the view, then look straight at the top of the base.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint {
                    text: "Rotate the camera",
                    tip: Some("Drag the view cube, or drag with two fingers on a trackpad"),
                    goal: Goal::Orbited,
                },
                Hint { text: "Hover over the top face and press the Space bar", tip: None, goal: Goal::SketchOnFace },
            ],
            solution: &[("sketch", r#"{"body": "$body", "face": "$face"}"#)],
        },
        Step {
            intro: "Draw the back of the stand along the far edge.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE, [("sketch", r#"{"body": "$body", "face": "$face"}"#)]],
            view: View::Sketch,
            tools: &[],
            sketch_tools: SKETCH_TOOLS,
            hints: &[Hint {
                text: "Select Rectangle and draw a strip along the back edge",
                tip: Some("Start on a back corner of the base"),
                goal: Goal::Lines(4),
            }],
            solution: &[("rectangle", r#"{"center": [0, 28], "size": [90, 14]}"#)],
        },
        Step {
            intro: "Pull the strip up into the back of the stand.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE, STAND_BACK_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the strip", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Pull the arrow up", tip: Some("Or type 70 and press Return"), goal: Goal::Grew(5000) },
            ],
            solution: STAND_BACK,
        },
        Step {
            intro: "Cut a slot for the phone to rest in.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE, STAND_BACK_SKETCH, STAND_BACK, STAND_SLOT_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the slot's profile", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Push the arrow down to cut the slot", tip: Some("Or type -4 and press Return"), goal: Goal::Cut(200) },
            ],
            solution: STAND_SLOT,
        },
        Step {
            intro: "Select things, then move the stand.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE, STAND_BACK_SKETCH, STAND_BACK, STAND_SLOT_SKETCH, STAND_SLOT],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint { text: "To select a face, click it once", tip: None, goal: Goal::Picked(Pick::Face) },
                Hint { text: "To select the whole body, double-click it", tip: None, goal: Goal::Picked(Pick::Body) },
                Hint { text: "Drag an arrow to move the body", tip: None, goal: Goal::Moved },
            ],
            solution: &[("translate", r#"{"by": [0, -20, 0], "items": ["$body"]}"#)],
        },
        Step {
            intro: "Round off a sharp edge.",
            setup: cmds![STAND_REF, STAND_SKETCH, STAND_BASE, STAND_BACK_SKETCH, STAND_BACK, STAND_SLOT_SKETCH, STAND_SLOT],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select an edge", tip: None, goal: Goal::Picked(Pick::Edge) },
                Hint { text: "Drag the arrow to round it", tip: Some("Or type 2 and press Return"), goal: Goal::Rounded },
            ],
            solution: &[("select", r#"{"picks": [{"kind": "body", "body": "$body"}]}"#), ("fillet", r#"{"radius": 1.5}"#)],
        },
        Step {
            intro: "Give the stand a finish.",
            setup: cmds![
                STAND_REF,
                STAND_SKETCH,
                STAND_BASE,
                STAND_BACK_SKETCH,
                STAND_BACK,
                STAND_SLOT_SKETCH,
                STAND_SLOT,
                [("select", r#"{"picks": [{"kind": "body", "body": "$body"}]}"#), ("fillet", r#"{"radius": 1.5}"#), ("deselect_all", "{}")]
            ],
            view: View::Default,
            tools: &[Tool::Visualization],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select Visualization", tip: None, goal: Goal::Visualizing },
                Hint { text: "Drag a material onto your stand", tip: None, goal: Goal::Finished },
            ],
            solution: &[("set_appearance", r#"{"items": ["$body"], "appearance": "Aluminum"}"#)],
        },
    ],
};

// ---- The pencil cup --------------------------------------------------------------------------------------------

const CUP_REF: &[Cmd] = &[
    ("sketch", r#"{"plane": "xy"}"#),
    ("circle", r#"{"center": [-150, 0], "radius": 35}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 95}"#),
    ("shell", r#"{"body": "$ref", "faces": ["$face"], "thickness": 3}"#),
    ("select", r#"{"picks": [{"kind": "body", "body": "$ref"}]}"#),
    ("fillet", r#"{"radius": 1}"#),
    ("deselect_all", "{}"),
];
const CUP_SKETCH: &[Cmd] = &[("sketch", r#"{"plane": "xy"}"#), ("circle", r#"{"center": [0, 0], "radius": 35}"#), ("exit_sketching", "{}")];
const CUP_BODY: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 95}"#)];
const CUP_SHELL: &[Cmd] = &[("shell", r#"{"body": "$body", "faces": ["$face"], "thickness": 3}"#)];

const PENCIL_CUP: Lesson = Lesson {
    id: "pencil-cup",
    title: "Model a Pencil Cup",
    minutes: 7,
    finished: CUP_REF,
    steps: &[
        Step {
            intro: "To draw the bottom of the cup, select Sketch.",
            setup: &[],
            view: View::Default,
            tools: &[Tool::Sketch],
            sketch_tools: SKETCH_TOOLS,
            hints: &[
                Hint { text: "Select Sketch", tip: None, goal: Goal::Sketching },
                Hint { text: "Select the ground plane", tip: None, goal: Goal::SketchOpen },
                Hint { text: "Select Circle and draw a circle", tip: Some("Click the centre, then the edge"), goal: Goal::Circles(1) },
            ],
            solution: &[("sketch", r#"{"plane": "xy"}"#), ("circle", r#"{"center": [0, 0], "radius": 35}"#)],
        },
        Step {
            intro: "Pull the circle up into a cylinder. You can type a size at any time.",
            setup: cmds![CUP_REF, CUP_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the circle's profile", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Type 95 and press Return", tip: Some("Typing a number sets the size exactly"), goal: Goal::NewBody },
            ],
            solution: CUP_BODY,
        },
        Step {
            intro: "Turn the view to see the top of the cylinder.",
            setup: cmds![CUP_REF, CUP_SKETCH, CUP_BODY],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint {
                    text: "Rotate the camera",
                    tip: Some("Drag the view cube, or drag with two fingers on a trackpad"),
                    goal: Goal::Orbited,
                },
                Hint { text: "Click the top face once to select it", tip: None, goal: Goal::Picked(Pick::Face) },
            ],
            solution: &[],
        },
        Step {
            intro: "Hollow the cylinder out with Shell.",
            setup: cmds![CUP_REF, CUP_SKETCH, CUP_BODY],
            view: View::Default,
            tools: &[Tool::Shell],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the top face", tip: None, goal: Goal::Picked(Pick::Face) },
                Hint {
                    text: "Select Shell, then type 3 and press Return",
                    tip: Some("The top face opens; the walls are 3 mm"),
                    goal: Goal::Cut(20000),
                },
            ],
            solution: CUP_SHELL,
        },
        Step {
            intro: "Round the rim so it's soft to the touch.",
            setup: cmds![CUP_REF, CUP_SKETCH, CUP_BODY, CUP_SHELL],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select an edge of the rim", tip: None, goal: Goal::Picked(Pick::Edge) },
                Hint { text: "Drag the arrow to round it", tip: Some("Or type 1 and press Return"), goal: Goal::Rounded },
            ],
            solution: &[("select", r#"{"picks": [{"kind": "body", "body": "$body"}]}"#), ("fillet", r#"{"radius": 1}"#)],
        },
        Step {
            intro: "Move the cup to where you want it.",
            setup: cmds![CUP_REF, CUP_SKETCH, CUP_BODY, CUP_SHELL],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Double-click the cup to select all of it", tip: None, goal: Goal::Picked(Pick::Body) },
                Hint { text: "Drag an arrow to move it", tip: None, goal: Goal::Moved },
            ],
            solution: &[("translate", r#"{"by": [30, 0, 0], "items": ["$body"]}"#)],
        },
        Step {
            intro: "Give the cup a colour.",
            setup: cmds![CUP_REF, CUP_SKETCH, CUP_BODY, CUP_SHELL],
            view: View::Default,
            tools: &[Tool::Visualization],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select Visualization", tip: None, goal: Goal::Visualizing },
                Hint { text: "Drag a material onto your cup", tip: None, goal: Goal::Finished },
            ],
            solution: &[("set_appearance", r#"{"items": ["$body"], "appearance": "Aluminum"}"#)],
        },
    ],
};

// ---- The key tag -----------------------------------------------------------------------------------------------

const TAG_REF: &[Cmd] = &[
    ("sketch", r#"{"plane": "xy"}"#),
    ("rectangle", r#"{"center": [-100, 0], "size": [60, 30]}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 4}"#),
    ("sketch", r#"{"origin": [0, 0, 4], "normal": [0, 0, 1]}"#),
    ("circle", r#"{"center": [0, -122], "radius": 4}"#),
    ("exit_sketching", "{}"),
    ("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": -5, "result": "subtract"}"#),
    ("select", r#"{"picks": [{"kind": "body", "body": "$ref"}]}"#),
    ("fillet", r#"{"radius": 1}"#),
    ("deselect_all", "{}"),
];
const TAG_SKETCH: &[Cmd] =
    &[("sketch", r#"{"plane": "xy"}"#), ("rectangle", r#"{"center": [0, 0], "size": [60, 30]}"#), ("exit_sketching", "{}")];
const TAG_BODY: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": 4}"#)];
const TAG_HOLE_SKETCH: &[Cmd] = &[
    ("sketch", r#"{"origin": [0, 0, 4], "normal": [0, 0, 1]}"#),
    ("circle", r#"{"center": [0, -22], "radius": 4}"#),
    ("exit_sketching", "{}"),
];
const TAG_HOLE: &[Cmd] = &[("extrude", r#"{"sketch": "$sketch", "region": 0, "distance": -5, "result": "subtract"}"#)];

const KEY_TAG: Lesson = Lesson {
    id: "key-tag",
    title: "Model a Key Tag",
    minutes: 7,
    finished: TAG_REF,
    steps: &[
        Step {
            intro: "To draw the tag, select Sketch.",
            setup: &[],
            view: View::Default,
            tools: &[Tool::Sketch],
            sketch_tools: SKETCH_TOOLS,
            hints: &[
                Hint { text: "Select Sketch", tip: None, goal: Goal::Sketching },
                Hint { text: "Select the ground plane", tip: None, goal: Goal::SketchOpen },
                Hint {
                    text: "Select Rectangle and draw the tag",
                    tip: Some("Click one corner, then the opposite one"),
                    goal: Goal::Lines(4),
                },
            ],
            solution: &[("sketch", r#"{"plane": "xy"}"#), ("rectangle", r#"{"center": [0, 0], "size": [60, 30]}"#)],
        },
        Step {
            intro: "Make the tag 4 mm thick.",
            setup: cmds![TAG_REF, TAG_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the tag's profile", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Type 4 and press Return", tip: Some("Typing a number sets the size exactly"), goal: Goal::NewBody },
            ],
            solution: TAG_BODY,
        },
        Step {
            intro: "Look straight down at the tag.",
            setup: cmds![TAG_REF, TAG_SKETCH, TAG_BODY],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint {
                    text: "Rotate the camera",
                    tip: Some("Drag the view cube, or drag with two fingers on a trackpad"),
                    goal: Goal::Orbited,
                },
                Hint { text: "Hover over the top face and press the Space bar", tip: None, goal: Goal::SketchOnFace },
            ],
            solution: &[("sketch", r#"{"body": "$body", "face": "$face"}"#)],
        },
        Step {
            intro: "Draw the hole for the key ring.",
            setup: cmds![TAG_REF, TAG_SKETCH, TAG_BODY, [("sketch", r#"{"body": "$body", "face": "$face"}"#)]],
            view: View::Sketch,
            tools: &[],
            sketch_tools: SKETCH_TOOLS,
            hints: &[Hint {
                text: "Select Circle and draw a small circle near one end",
                tip: Some("About 8 mm across is plenty"),
                goal: Goal::Circles(1),
            }],
            solution: &[("circle", r#"{"center": [-22, 0], "radius": 4}"#)],
        },
        Step {
            intro: "Push the circle through to make the hole.",
            setup: cmds![TAG_REF, TAG_SKETCH, TAG_BODY, TAG_HOLE_SKETCH],
            view: View::Default,
            tools: &[Tool::Extrude],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select the circle's profile", tip: None, goal: Goal::Picked(Pick::Region) },
                Hint { text: "Push the arrow down through the tag", tip: Some("Or type -5 and press Return"), goal: Goal::Cut(100) },
            ],
            solution: TAG_HOLE,
        },
        Step {
            intro: "Round the corners of the tag.",
            setup: cmds![TAG_REF, TAG_SKETCH, TAG_BODY, TAG_HOLE_SKETCH, TAG_HOLE],
            view: View::Default,
            tools: &[],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select one of the tag's edges", tip: None, goal: Goal::Picked(Pick::Edge) },
                Hint { text: "Drag the arrow to round it", tip: Some("Or type 1 and press Return"), goal: Goal::Rounded },
            ],
            solution: &[("select", r#"{"picks": [{"kind": "body", "body": "$body"}]}"#), ("fillet", r#"{"radius": 1}"#)],
        },
        Step {
            intro: "Pick a colour for your tag.",
            setup: cmds![
                TAG_REF,
                TAG_SKETCH,
                TAG_BODY,
                TAG_HOLE_SKETCH,
                TAG_HOLE,
                [("select", r#"{"picks": [{"kind": "body", "body": "$body"}]}"#), ("fillet", r#"{"radius": 1}"#), ("deselect_all", "{}")]
            ],
            view: View::Default,
            tools: &[Tool::Visualization],
            sketch_tools: &[],
            hints: &[
                Hint { text: "Select Visualization", tip: None, goal: Goal::Visualizing },
                Hint { text: "Drag a material onto your tag", tip: None, goal: Goal::Finished },
            ],
            solution: &[("set_appearance", r#"{"items": ["$body"], "appearance": "Aluminum"}"#)],
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn session() -> Session {
        Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Tutorial")
    }

    /// Every step's scene builds, and its scripted solution meets every goal that the document can show.
    #[test]
    fn every_step_can_be_finished() {
        for lesson in LESSONS {
            let mut s = session();
            run(&mut s, lesson.finished, &mut HashMap::new()).unwrap_or_else(|e| panic!("{} finished: {e}", lesson.id));
            assert!(!s.doc.bodies.is_empty(), "{} finished has no body", lesson.id);
            for (i, step) in lesson.steps.iter().enumerate() {
                let mut s = session();
                let mut vars = HashMap::new();
                run(&mut s, step.setup, &mut vars).unwrap_or_else(|e| panic!("{} step {}: setup: {e}", lesson.id, i + 1));
                let base = snapshot(&s);
                run(&mut s, step.solution, &mut vars).unwrap_or_else(|e| panic!("{} step {}: solution: {e}", lesson.id, i + 1));
                let now = snapshot(&s);
                let live = Live {
                    picking_plane: false,
                    sketch_open: s.active_sketch.is_some(),
                    sketch_on_face: false,
                    selection: s.selection_kind(),
                    turned: 0.0,
                    visualizing: false,
                };
                for h in step.hints.iter().filter(|h| on_document(h.goal)) {
                    assert!(met(h.goal, &base, &now, &live), "{} step {}: {:?} not met by the solution", lesson.id, i + 1, h.goal);
                }
                // A goal mustn't be met before the step is done (it would skip the step).
                for h in step.hints.iter().filter(|h| on_document(h.goal)) {
                    assert!(!met(h.goal, &base, &base, &live), "{} step {}: {:?} already met at the start", lesson.id, i + 1, h.goal);
                }
            }
        }
    }

    #[test]
    fn placeholders_fill_in() {
        let mut vars = HashMap::new();
        vars.insert("body", Value::from(7));
        let mut v: Value = serde_json::from_str(r#"{"items": ["$body"], "x": "$none"}"#).unwrap_or_default();
        fill(&mut v, &vars);
        assert_eq!(v["items"][0], 7);
        assert_eq!(v["x"], "$none");
    }
}

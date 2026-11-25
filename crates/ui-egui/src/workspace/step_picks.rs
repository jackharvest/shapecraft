//! What a History step was made from, picked again from its card: each row's Edit... rolls the model back to just
//! before the step with its picks lit, clicks in the view add or let go, and Done rebuilds the step with them.

use serde_json::{Map, Value, json};
use shapecraft_doc::{HistoryStep, ItemId, Pick};
use shapecraft_engine::history::BREAKPOINT;

use super::Workspace;
use super::tools::Tool;

/// How a step keeps a setting's picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Held {
    /// `"body"` and `"faces"` of one body.
    Faces,
    /// One face of one body: `"body"` and `"face"`.
    Face,
    /// One edge of one body: `"body"` and `"edge"`.
    Edge,
    /// A list of regions and flat faces.
    Profiles,
    /// One region or flat face, under the key.
    Profile,
    /// A list of sketch curves and edges.
    Path,
    /// A face, plane or (with `bodies`) whole body, under the key.
    Target { bodies: bool },
    /// A straight or round thing to turn about: an edge, a face or an axis.
    Axis,
    /// A list of body ids.
    Bodies,
    /// One body id.
    Body,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Slot {
    pub label: &'static str,
    /// What one of them is called ("1 face").
    pub noun: &'static str,
    pub key: &'static str,
    pub held: Held,
    /// Nothing picked is fine (guides).
    pub optional: bool,
}

const fn slot(label: &'static str, noun: &'static str, key: &'static str, held: Held) -> Slot {
    Slot { label, noun, key, held, optional: false }
}

const TO: Slot = slot("To Object", "item", "to", Held::Target { bodies: true });
const START: Slot = slot("Start Plane", "item", "start_plane", Held::Target { bodies: false });
const ITEMS: Slot = slot("Items", "body", "items", Held::Bodies);

/// The settings a step's card can pick again, in order.
pub(super) fn slots(kind: &str) -> &'static [Slot] {
    const EXTRUDE: &[Slot] = &[TO, START];
    const LOFT: &[Slot] = &[
        slot("Profiles", "profile", "profiles", Held::Profiles),
        Slot { label: "Guides", noun: "curve", key: "guides", held: Held::Path, optional: true },
    ];
    const SWEEP: &[Slot] = &[slot("Profile", "face", "profile", Held::Profile), slot("Path", "curve", "path", Held::Path)];
    const REVOLVE: &[Slot] = &[slot("Axis", "axis", "axis", Held::Axis)];
    const SHELL: &[Slot] = &[Slot { label: "Open Faces", noun: "face", key: "faces", held: Held::Faces, optional: true }];
    const OFFSET: &[Slot] = &[slot("Faces", "face", "faces", Held::Faces)];
    const REPLACE: &[Slot] = &[
        slot("Faces to Replace", "face", "faces", Held::Faces),
        slot("Replacing Face", "face", "to", Held::Target { bodies: false }),
    ];
    const THREAD: &[Slot] = &[slot("Faces to Thread", "face", "faces", Held::Faces)];
    const BOOLEAN: &[Slot] = &[slot("Target", "body", "target", Held::Body), slot("Tools", "body", "tools", Held::Bodies)];
    const MIRROR: &[Slot] = &[ITEMS, slot("Mirror Plane", "item", "plane", Held::Target { bodies: false })];
    const ABOUT: &[Slot] = &[ITEMS, slot("Axis", "axis", "about", Held::Axis)];
    const MOVED: &[Slot] = &[ITEMS];
    const ALIGN: &[Slot] = &[
        slot("Face to Align", "item", "from", Held::Target { bodies: false }),
        slot("Align To", "item", "to", Held::Target { bodies: false }),
    ];
    const PROJECT: &[Slot] = &[slot("Project Onto", "face", "to", Held::Target { bodies: false })];
    const WRAP: &[Slot] = &[slot("Face", "face", "face", Held::Face)];
    const FILL: &[Slot] = &[
        Slot { label: "Boundary Profiles", noun: "profile", key: "profiles", held: Held::Profiles, optional: true },
        Slot { label: "Guide Curves", noun: "curve", key: "guides", held: Held::Path, optional: true },
    ];
    const FROM: &[Slot] = &[slot("Reference", "item", "from", Held::Target { bodies: false })];
    const TWO: &[Slot] = &[
        slot("Plane #1", "item", "a", Held::Target { bodies: false }),
        slot("Plane #2", "item", "b", Held::Target { bodies: false }),
    ];
    const EDGE: &[Slot] = &[slot("Edge", "edge", "edge", Held::Edge)];
    const FACE: &[Slot] = &[slot("Face", "face", "face", Held::Face)];
    match kind {
        "extrude" => EXTRUDE,
        "loft" => LOFT,
        "sweep" => SWEEP,
        "fill" => FILL,
        "revolve" => REVOLVE,
        "shell" => SHELL,
        "offset_face" | "move_faces" => OFFSET,
        "replace_face" => REPLACE,
        "thread" => THREAD,
        "union" | "subtract" | "intersect" => BOOLEAN,
        "mirror" => MIRROR,
        "rotate" | "pattern_circular" => ABOUT,
        "translate" | "scale" | "pattern" | "split_body" => MOVED,
        "align" => ALIGN,
        "project_edge" => PROJECT,
        "wrap_emboss" => WRAP,
        "offset_plane" | "plane_parallel_face" => FROM,
        "midplane" | "axis_2planes" => TWO,
        "plane_through_edge" | "plane_on_curve" | "plane_perp_edge" | "axis_along_edge" => EDGE,
        "axis_perp_face" | "axis_cylinder" => FACE,
        _ => &[],
    }
}

fn ids(v: Option<&Value>) -> Vec<ItemId> {
    v.and_then(Value::as_array).map(|l| l.iter().filter_map(|n| n.as_u64().map(ItemId)).collect()).unwrap_or_default()
}

fn num(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok())
}

/// The picks one entry of a parameter stands for.
fn entry(v: &Value) -> Vec<Pick> {
    let id = |k: &str| v.get(k).and_then(Value::as_u64).map(ItemId);
    let many = |one: &str, all: &str| -> Vec<u32> {
        match v.get(all).and_then(Value::as_array) {
            Some(l) => l.iter().filter_map(|n| num(Some(n))).collect(),
            None => num(v.get(one)).into_iter().collect(),
        }
    };
    if let Some(plane) = id("plane").or_else(|| id("construction")) {
        return vec![Pick::Plane { plane }];
    }
    if let Some(axis) = id("axis") {
        return vec![Pick::Axis { axis }];
    }
    if let Some(body) = id("body") {
        let (faces, edges) = (many("face", "faces"), many("edge", "edges"));
        return if faces.is_empty() && edges.is_empty() {
            vec![Pick::Body { body }]
        } else {
            faces.into_iter().map(|face| Pick::Face { body, face }).chain(edges.into_iter().map(|edge| Pick::Edge { body, edge })).collect()
        };
    }
    if let Some(sketch) = id("sketch") {
        let entities = many("entity", "entities");
        if !entities.is_empty() {
            return entities.into_iter().map(|entity| Pick::SketchEntity { sketch, entity }).collect();
        }
        if let Some(region) = num(v.get("region")) {
            return vec![Pick::Region { sketch, region }];
        }
        return vec![Pick::Sketch { sketch }];
    }
    Vec::new()
}

/// What a setting is made of, from the step's parameters, else from what was selected when it ran.
pub(super) fn picks(step: &HistoryStep, slot: &Slot) -> Vec<Pick> {
    let Some(e) = step.entries.first() else { return Vec::new() };
    let p = &e.params;
    let chosen = |f: &dyn Fn(&Pick) -> bool| e.selection.iter().copied().filter(|x| f(x)).collect::<Vec<_>>();
    let v = p.get(slot.key).filter(|v| !v.is_null());
    match slot.held {
        Held::Faces if p.get("body").is_some() => {
            let Some(body) = p.get("body").and_then(Value::as_u64).map(ItemId) else { return Vec::new() };
            let faces = p.get("faces").and_then(Value::as_array);
            faces.map(|l| l.iter().filter_map(|n| num(Some(n))).map(|face| Pick::Face { body, face }).collect()).unwrap_or_default()
        }
        Held::Faces => chosen(&|x| matches!(x, Pick::Face { .. })),
        Held::Face | Held::Edge => {
            let Some(body) = p.get("body").and_then(Value::as_u64).map(ItemId) else {
                return chosen(&|x| if slot.held == Held::Face { matches!(x, Pick::Face { .. }) } else { matches!(x, Pick::Edge { .. }) });
            };
            let n = num(v);
            match (slot.held, n) {
                (Held::Face, Some(face)) => vec![Pick::Face { body, face }],
                (_, Some(edge)) => vec![Pick::Edge { body, edge }],
                _ => Vec::new(),
            }
        }
        Held::Profiles | Held::Path | Held::Profile => {
            let given: Vec<Pick> = match v {
                Some(Value::Array(l)) => l.iter().flat_map(entry).collect(),
                Some(one) => entry(one),
                None => Vec::new(),
            };
            if v.is_some() || slot.optional {
                return given;
            }
            let found = if slot.held == Held::Path {
                chosen(&|x| matches!(x, Pick::SketchEntity { .. } | Pick::Edge { .. }))
            } else {
                chosen(&|x| matches!(x, Pick::Region { .. } | Pick::Face { .. }))
            };
            if slot.held == Held::Profile { found.into_iter().take(1).collect() } else { found }
        }
        Held::Target { .. } | Held::Axis => v.map(entry).unwrap_or_default(),
        Held::Bodies => match v {
            Some(v) => ids(Some(v)).into_iter().map(|body| Pick::Body { body }).collect(),
            None => {
                let mut seen: Vec<ItemId> = Vec::new();
                for b in e.selection.iter().filter_map(Pick::body) {
                    if !seen.contains(&b) {
                        seen.push(b);
                    }
                }
                // Union's tools are what's left after its target.
                let skip = usize::from(slot.key == "tools" && p.get("target").is_none());
                seen.into_iter().skip(skip).map(|body| Pick::Body { body }).collect()
            }
        },
        Held::Body => match v.and_then(Value::as_u64) {
            Some(id) => vec![Pick::Body { body: ItemId(id) }],
            None => e.selection.iter().filter_map(Pick::body).take(1).map(|body| Pick::Body { body }).collect(),
        },
    }
}

/// Whether a click on `p` belongs in the setting, as the pick it stands for.
fn takes(held: Held, p: Pick) -> Option<Pick> {
    match (held, p) {
        (Held::Faces | Held::Face, Pick::Face { .. })
        | (Held::Edge, Pick::Edge { .. })
        | (Held::Profiles | Held::Profile, Pick::Region { .. } | Pick::Face { .. })
        | (Held::Path, Pick::SketchEntity { .. } | Pick::Edge { .. })
        | (Held::Target { .. }, Pick::Face { .. } | Pick::Plane { .. })
        | (Held::Target { bodies: true }, Pick::Body { .. })
        | (Held::Axis, Pick::Edge { .. } | Pick::Face { .. } | Pick::Axis { .. })
        | (Held::Bodies | Held::Body, Pick::Body { .. }) => Some(p),
        (Held::Bodies | Held::Body, _) => p.body().map(|body| Pick::Body { body }),
        _ => None,
    }
}

impl Held {
    /// One pick takes the place of the last.
    fn single(self) -> bool {
        matches!(self, Held::Face | Held::Edge | Held::Profile | Held::Target { .. } | Held::Axis | Held::Body)
    }
}

pub(super) fn pick_json(p: &Pick) -> Value {
    match *p {
        Pick::Body { body } => json!({ "body": body.0 }),
        Pick::Face { body, face } => json!({ "body": body.0, "face": face }),
        Pick::Edge { body, edge } => json!({ "body": body.0, "edge": edge }),
        Pick::Region { sketch, region } => json!({ "sketch": sketch.0, "region": region }),
        Pick::SketchEntity { sketch, entity } => json!({ "sketch": sketch.0, "entities": [entity] }),
        Pick::Plane { plane } => json!({ "plane": plane.0 }),
        Pick::Axis { axis } => json!({ "axis": axis.0 }),
        Pick::Sketch { sketch } | Pick::SketchPoint { sketch, .. } => json!({ "sketch": sketch.0 }),
        Pick::Image { .. } => Value::Null,
    }
}

/// The parameters that make the setting these picks.
fn params(slot: &Slot, picks: &[Pick]) -> Map<String, Value> {
    let mut m = Map::new();
    let list = || Value::Array(picks.iter().map(pick_json).collect());
    match slot.held {
        Held::Faces => {
            if let Some(body) = picks.iter().find_map(Pick::body) {
                m.insert("body".into(), json!(body.0));
            }
            let faces: Vec<u32> = picks.iter().filter_map(|p| if let Pick::Face { face, .. } = p { Some(*face) } else { None }).collect();
            m.insert("faces".into(), json!(faces));
        }
        Held::Face | Held::Edge => match picks.first() {
            Some(Pick::Face { body, face }) => {
                m.insert("body".into(), json!(body.0));
                m.insert("face".into(), json!(face));
            }
            Some(Pick::Edge { body, edge }) => {
                m.insert("body".into(), json!(body.0));
                m.insert("edge".into(), json!(edge));
            }
            _ => {}
        },
        Held::Profiles | Held::Path => {
            m.insert(slot.key.into(), if picks.is_empty() { Value::Null } else { list() });
        }
        Held::Profile | Held::Target { .. } | Held::Axis => {
            m.insert(slot.key.into(), picks.first().map(pick_json).unwrap_or(Value::Null));
        }
        Held::Bodies => {
            let ids: Vec<u64> = picks.iter().filter_map(Pick::body).map(|b| b.0).collect();
            m.insert(slot.key.into(), json!(ids));
        }
        Held::Body => {
            if let Some(b) = picks.iter().find_map(Pick::body) {
                m.insert(slot.key.into(), json!(b.0));
            }
        }
    }
    m
}

/// A setting being picked again.
#[derive(Clone, Debug)]
pub(crate) struct PickEdit {
    /// The step's place in the list while rolled back (the open card's).
    pub(crate) step: usize,
    orig: usize,
    depth: usize,
    held: Option<usize>,
    pub(super) slot: Slot,
    pub(crate) picks: Vec<Pick>,
    /// Set along with the picks (Extent becoming To Object).
    extra: Map<String, Value>,
}

/// Starts picking one of a step's settings: the model rolls back to just before the step, its picks lit.
pub(super) fn begin(ws: &mut Workspace, step: usize, slot: Slot, extra: Map<String, Value>) {
    let all = ws.session.all_steps();
    let Some(h) = all.get(step) else { return };
    let picks = picks(h, &slot);
    let held = all.iter().position(|h| h.kind == BREAKPOINT);
    let plain = step - usize::from(held.is_some_and(|b| b < step));
    if ws.run("set_breakpoint", json!({ "before": plain })).is_none() {
        return;
    }
    let rolled = plain + 1;
    ws.history.open = Some(rolled);
    ws.history.editing = None;
    ws.profile_edit = None;
    ws.cut_edit = None;
    // Sketches a feature hid show again by themselves here: the rollback shows the model as the step found it.
    ws.session.selection = picks.clone();
    ws.pick_edit = Some(PickEdit { step: rolled, orig: step, depth: ws.session.undo_depth(), held, slot, picks, extra });
    ws.toast(if slot.held.single() { "Click what to use instead, then Done" } else { "Click to add or remove, then Done" });
}

/// A click while picking: adds, lets go, or (a single pick) takes its place.
pub(super) fn click(ws: &mut Workspace, hit: Option<Pick>) {
    let Some(ed) = ws.pick_edit.as_mut() else { return };
    let Some(p) = hit.and_then(|p| takes(ed.slot.held, p)) else { return };
    if let Some(i) = ed.picks.iter().position(|x| *x == p) {
        ed.picks.remove(i);
    } else if ed.slot.held.single() {
        ed.picks = vec![p];
    } else {
        // The faces are one body's: a face of another starts over there.
        if ed.slot.held == Held::Faces && ed.picks.first().and_then(Pick::body) != p.body() {
            ed.picks.clear();
        }
        ed.picks.push(p);
    }
    let picks = ed.picks.clone();
    ws.session.selection = picks;
}

/// Ends the picking: with `apply`, the step is rebuilt with what's picked (nothing picked keeps picking).
pub(super) fn end(ws: &mut Workspace, apply: bool) {
    let Some(ed) = ws.pick_edit.take() else { return };
    if apply && ed.picks.is_empty() && !ed.slot.optional {
        ws.pick_edit = Some(ed);
        ws.toast("Pick at least one");
        return;
    }
    // Back to the model as it was: the rollback taken back exactly, or the breakpoint put back where it was.
    if !(ws.session.undo_depth() == ed.depth && ws.session.drop_last_step()) {
        let _ = ws.run("set_breakpoint", ed.held.map_or(json!({}), |b| json!({ "before": b })));
    }
    ws.session.selection.clear();
    ws.history.open = Some(ed.orig);
    if apply {
        let mut m = params(&ed.slot, &ed.picks);
        m.extend(ed.extra);
        let _ = ws.run("edit_step", json!({ "step": ed.orig, "params": m }));
    }
}

/// The card closed (or a tool started) while picking: put everything back.
pub(super) fn tend(ws: &mut Workspace) {
    if ws.pick_edit.as_ref().is_some_and(|e| Some(e.step) != ws.history.open || !matches!(ws.tool, Tool::Idle)) {
        end(ws, false);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A sweep's path and a shell's faces are picked again from their cards; Escape changes nothing.
    #[test]
    fn steps_take_new_picks_from_their_cards() {
        let mut s = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Picks");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 20, "result": "new"})).unwrap();
        let body = s.doc.bodies[0].id;
        s.execute("shell", &json!({"body": body.0, "faces": [0], "thickness": 1})).unwrap();
        let step = s.all_steps().len() - 1;
        let mut ws = Workspace::new(s, crate::settings::Settings::default());
        let volume = |ws: &Workspace| ws.session.kernel.mass_props(&ws.session.doc.bodies[0].shape).unwrap().volume;
        let before = volume(&ws);
        ws.open_history_step(step);
        let faces = slots("shell")[0];
        begin(&mut ws, step, faces, Map::new());
        assert_eq!(ws.session.selection, vec![Pick::Face { body, face: 0 }]);
        assert!((volume(&ws) - 8000.0).abs() < 1e-6, "rolled back");
        ws.escape();
        assert!(ws.pick_edit.is_none() && (volume(&ws) - before).abs() < 1e-9);
        begin(&mut ws, step, faces, Map::new());
        click(&mut ws, Some(Pick::Edge { body, edge: 1 }));
        assert_eq!(ws.pick_edit.as_ref().unwrap().picks.len(), 1, "an edge isn't a face");
        click(&mut ws, Some(Pick::Face { body, face: 1 }));
        end(&mut ws, true);
        assert_eq!(ws.session.all_steps()[step].entries[0].params["faces"], json!([0, 1]));
        assert!(volume(&ws) < before - 1.0, "one more side open");
        let _ = ws.run("undo", json!({}));
        assert!((volume(&ws) - before).abs() < 1e-9);
    }
}

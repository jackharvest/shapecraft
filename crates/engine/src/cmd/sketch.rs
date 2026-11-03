use glam::{DVec2, DVec3};
use serde_json::{Value, json};
use shapecraft_doc::{Constraint, ItemId, Pick, Sketch, SketchEntity, SketchPlane};
use shapecraft_kernel::EdgeCurve;

use super::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session};

fn in_sketch(s: &Session) -> bool {
    s.active_sketch.is_some()
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "sketch",
            label: "Sketch",
            menu: "Tools/Sketch",
            shortcut: None,
            help: "Choose a plane or a flat face to sketch on",
            params: r#"{"plane": "xy"|"xz"|"yz"} | {"body": id, "face": n} | {"construction": id} | {"origin":[x,y,z],"normal":[x,y,z],"x_dir":[x,y,z]?}, "look": false? (the open app keeps its camera)  (returns the frame: origin, x (u), y (v), normal)"#,
            modifies: true,
            enabled: always,
            run: new_sketch,
        },
        CommandSpec {
            id: "change_sketch_plane",
            label: "Change Sketch Plane",
            menu: "",
            shortcut: None,
            help: "Move a sketch onto another plane or flat face; its curves keep their shape",
            params: r#"{"sketch": id, "plane": "xy"|"xz"|"yz"} | {"sketch": id, "body": id, "face": n} | {"sketch": id, "construction": id} | {"sketch": id, "origin":[x,y,z],"normal":[x,y,z]}"#,
            modifies: true,
            enabled: |s| !s.doc.sketches.is_empty(),
            run: change_sketch_plane,
        },
        CommandSpec {
            id: "break_curve",
            label: "Break",
            menu: "",
            shortcut: None,
            help: "Split a sketch curve in two at a point",
            params: r#"{"entity": n, "at": [u,v], "sketch": id?}  (a line, arc or spline; the point is moved onto the curve)"#,
            modifies: true,
            enabled: in_sketch,
            run: break_curve,
        },
        CommandSpec {
            id: "exit_sketching",
            label: "Exit Sketching",
            menu: "",
            shortcut: None,
            help: "Leave the sketch",
            params: "{}",
            modifies: true,
            enabled: in_sketch,
            run: exit_sketching,
        },
        CommandSpec {
            id: "line",
            label: "Line",
            menu: "Sketch/Line",
            shortcut: Some("L"),
            help: "Click point after point; Return ends the chain",
            params: r#"{"a":[u,v], "b":[u,v]} in the active sketch's plane, millimeters"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let (a, b) = (p.vec2("a")?, p.vec2("b")?);
                if a.distance(b) < 1e-9 {
                    return Err(EngineError::BadParams("line has zero length".into()));
                }
                add_entity(s, p, SketchEntity::Line { a, b })
            },
        },
        CommandSpec {
            id: "rectangle",
            label: "Rectangle",
            menu: "Sketch/Rectangle",
            shortcut: Some("R"),
            help: "Click one corner, then the opposite one",
            params: r#"{"corner":[u,v], "opposite":[u,v]} | {"center":[u,v], "size":[w,h]} | {"points":[[u,v],[u,v],[u,v]]} (3 Points: one side, then the width; a turned rectangle is four lines)"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                if p.has("points") {
                    let pts: [[f64; 2]; 3] = p.parse("points")?;
                    let [a, b, c] = pts.map(glam::DVec2::from_array);
                    let side = b - a;
                    let n = side.perp().try_normalize().ok_or_else(|| EngineError::BadParams("the first side has no length".into()))?;
                    let w = (c - b).dot(n);
                    if !w.is_finite() || w.abs() < 1e-9 || !a.is_finite() || !side.is_finite() {
                        return Err(EngineError::BadParams("rectangle needs a width and a height".into()));
                    }
                    let corners = [a, b, b + n * w, a + n * w];
                    let out = add_lines(s, p, [0, 1, 2, 3].map(|i| SketchEntity::Line { a: corners[i], b: corners[(i + 1) % 4] }))?;
                    return hold_rectangle(s, out, false);
                }
                let (a, b) = if p.has("center") {
                    let (c, size) = (p.vec2("center")?, p.vec2("size")?);
                    (c - size.abs() / 2.0, c + size.abs() / 2.0)
                } else {
                    (p.vec2("corner")?, p.vec2("opposite")?)
                };
                let size = (b - a).abs();
                if size.x < 1e-9 || size.y < 1e-9 {
                    return Err(EngineError::BadParams("rectangle needs a width and a height".into()));
                }
                // Four lines, as the reference draws it: each side can be picked on its own.
                let out = add_lines(s, p, SketchEntity::rectangle_lines(a, b))?;
                hold_rectangle(s, out, true)
            },
        },
        CommandSpec {
            id: "polygon",
            label: "Polygon",
            menu: "Sketch/Polygon",
            shortcut: Some("G"),
            help: "Click the center, then a corner. Type to set the size",
            params: r#"{"center":[u,v], "radius": r, "sides": 5, "angle": degrees (90: a corner points up)}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let center = p.vec2("center")?;
                let radius = p.f64("radius")?;
                let sides = p.f64_or("sides", 5.0)?;
                if radius <= 1e-9 || !(3.0..=64.0).contains(&sides) {
                    return Err(EngineError::BadParams("a polygon needs a positive radius and 3–64 sides".into()));
                }
                let angle = p.f64_or("angle", 90.0)?.to_radians();
                add_entity(s, p, SketchEntity::Polygon { center, radius, sides: sides as u32, angle })
            },
        },
        CommandSpec {
            id: "arc",
            label: "Arc",
            menu: "Sketch/Arc",
            shortcut: Some("A"),
            help: "Click the start, the end, then a point the arc passes through",
            params: r#"{"a":[u,v], "mid":[u,v], "b":[u,v]}  (start, a point on the arc, end)"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let (a, mid, b) = (p.vec2("a")?, p.vec2("mid")?, p.vec2("b")?);
                if shapecraft_doc::circle_through(a, mid, b).is_none() || a.distance(b) < 1e-9 {
                    return Err(EngineError::BadParams("the three points of an arc can't be in a line".into()));
                }
                add_entity(s, p, SketchEntity::Arc { a, mid, b })
            },
        },
        CommandSpec {
            id: "ellipse",
            label: "Ellipse",
            menu: "Sketch/Ellipse",
            shortcut: None,
            help: "Click the center, then the end of one axis, then pull out the other",
            params: r#"{"center":[u,v], "rx": mm, "ry": mm, "angle": degrees (0)}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let center = p.vec2("center")?;
                let (rx, ry) = (p.f64("rx")?, p.f64("ry")?);
                if rx <= 1e-9 || ry <= 1e-9 {
                    return Err(EngineError::BadParams("both radii must be positive".into()));
                }
                let angle = p.f64_or("angle", 0.0)?.to_radians();
                add_entity(s, p, SketchEntity::Ellipse { center, rx, ry, angle })
            },
        },
        CommandSpec {
            id: "constrain",
            label: "Constrain",
            menu: "",
            shortcut: None,
            help: "Tie the selected sketch elements together; the sketch keeps it from then on",
            params: r#"{"kind": "parallel"|"perpendicular"|"tangent"|"coincident"|"midpoint"|"concentric"|"horizontal_vertical"|"equal"|"symmetry"|"lock"|"distance"|"radius"|"angle"|"point_distance", "sketch": id?, "entities": [n]? (default: the selected elements, in selection order; symmetry takes two elements and then the axis line), "value": n? (dimensions), "along": "absolute"|"horizontal"|"vertical"? (distance), "elements": [{"kind": "point", "entity", "index"}, {"kind": "point"|"segment", …}]? (point_distance; default: the picked points, then a picked line)}"#,
            modifies: true,
            enabled: in_sketch,
            run: constrain,
        },
        CommandSpec {
            id: "delete_constraint",
            label: "Delete Constraint",
            menu: "",
            shortcut: None,
            help: "Remove one of the sketch's constraints",
            params: r#"{"sketch": id?, "index": n}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let id = target_sketch(s, p)?;
                let index = p.f64("index")?;
                let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
                if !(index >= 0.0 && (index as usize) < sketch.constraints.len()) {
                    return Err(EngineError::BadParams(format!("no constraint {index}")));
                }
                sketch.constraints.remove(index as usize);
                Ok(json!({ "constraints": sketch.constraints.len() }))
            },
        },
        CommandSpec {
            id: "disconnect",
            label: "Disconnect",
            menu: "",
            shortcut: None,
            help: "Free the selected elements from their constraints",
            params: r#"{"sketch": id?, "entities": [n]? (default: the selected elements)}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let (id, ents) = picked_entities(s, p)?;
                let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
                let before = sketch.constraints.len();
                sketch.constraints.retain(|c| c.entities().iter().all(|e| !ents.contains(e)));
                Ok(json!({ "removed": before - sketch.constraints.len() }))
            },
        },
        CommandSpec {
            id: "make_construction",
            label: "Make Construction",
            menu: "",
            shortcut: None,
            help: "Turn the selected elements into construction geometry (dashed, not part of profiles), or back",
            params: r#"{"sketch": id?, "entities": [n]? (default: the selected elements)}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let (id, ents) = picked_entities(s, p)?;
                let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
                // All construction already: back to normal; otherwise all become construction.
                let all = ents.iter().all(|e| sketch.construction.contains(e));
                for e in &ents {
                    if all {
                        sketch.construction.remove(e);
                    } else {
                        sketch.construction.insert(*e);
                    }
                }
                Ok(json!({ "construction": !all }))
            },
        },
        CommandSpec {
            id: "trim",
            label: "Trim",
            menu: "Sketch/Trim",
            shortcut: Some("T"),
            help: "Click a piece of a sketch curve to cut it back to the nearest crossings",
            params: r#"{"entity": n, "at": [u,v], "sketch": id?}  (removes the piece of curve n around the point)"#,
            modifies: true,
            enabled: in_sketch,
            run: trim,
        },
        CommandSpec {
            id: "sketch_fillet",
            label: "Fillet",
            menu: "Sketch/Fillet",
            shortcut: None,
            help: "Round corners where two sketch lines meet with tangent arcs",
            params: r#"{"corners": [[u,v]...]?, "radius": mm, "entities": [n]?, "sketch": id?}  (default corners: those of the picked lines, else every corner)"#,
            modifies: true,
            enabled: in_sketch,
            run: sketch_fillet,
        },
        CommandSpec {
            id: "offset_edge",
            label: "Offset Edge",
            menu: "Sketch/Offset Edge/Single",
            shortcut: Some("O"),
            help: "Draw a parallel copy of the selected sketch curves",
            params: r#"{"distance": mm, "entities": [n]? | "region": n, "sketch": id? | "body": id, "face": n}  (default: the selected curves or region; connected curves offset as one chain; positive is outwards, or left of a line; a flat body face's outline goes into a new sketch on it)"#,
            modifies: true,
            enabled: |s| {
                s.active_sketch.is_some()
                    || s.selection.iter().any(|p| matches!(p, Pick::Region { .. } | Pick::SketchEntity { .. } | Pick::Face { .. }))
            },
            run: offset_edge,
        },
        CommandSpec {
            id: "offset_body_edge",
            label: "Offset Edge",
            menu: "",
            shortcut: None,
            help: "Draw a body edge's parallel copy on one of its flat faces, in a new sketch on that face",
            params: r#"{"body": id, "edge": n, "face": n, "distance": mm}  (positive is into the face)"#,
            modifies: true,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: offset_body_edge,
        },
        CommandSpec {
            id: "project_sketch",
            label: "Project as Sketch",
            menu: "",
            shortcut: None,
            help: "Project body edges and faces onto a face or plane, into a new sketch there (Project from outside a sketch)",
            params: r#"{"items": [{"body": id, "edge": n} | {"body": id, "face": n}], "to": {"body": id, "face": n} | {"plane": id}}"#,
            modifies: true,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: project_sketch,
        },
        CommandSpec {
            id: "text",
            label: "Text",
            menu: "Sketch/Text…",
            shortcut: None,
            help: "Write letters into the sketch; they enclose regions you can extrude",
            params: r#"{"at": [u,v], "text": "…", "size": mm (10), "angle": degrees (0), "font": "family"?}  (an installed font family; default a plain sans-serif)"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let at = p.vec2("at")?;
                let text = p.str("text")?.to_string();
                let size = p.f64_or("size", 10.0)?;
                let angle = p.f64_or("angle", 0.0)?;
                if text.trim().is_empty() || text.chars().count() > shapecraft_doc::text::MAX_CHARS {
                    return Err(EngineError::BadParams(format!("text must be 1–{} characters", shapecraft_doc::text::MAX_CHARS)));
                }
                if !(size.is_finite() && size > 1e-3 && size < 1e5 && angle.is_finite() && at.is_finite()) {
                    return Err(EngineError::BadParams("size, angle and position must be finite (size > 0)".into()));
                }
                if !shapecraft_doc::text::available() {
                    return Err(EngineError::NotAvailable("no system font found to draw text with".into()));
                }
                let font = p.opt_str("font").map(str::trim).filter(|f| !f.is_empty() && f.len() <= 200).map(str::to_string);
                add_entity(s, p, SketchEntity::Text { origin: at, text, size, angle: angle.to_radians(), font })
            },
        },
        CommandSpec {
            id: "project",
            label: "Project",
            menu: "Sketch/Project",
            shortcut: Some("P"),
            help: "Copy body edges (or a face's outline) into the sketch, flattened onto its plane",
            params: r#"{"edges": [{"body": id, "edge": n}]?, "faces": [{"body": id, "face": n}]?, "sketch": id?}  (default: the selected edges and faces)"#,
            modifies: true,
            enabled: in_sketch,
            run: project,
        },
        CommandSpec {
            id: "spline",
            label: "Spline",
            menu: "Sketch/Spline/Fit",
            shortcut: Some("I"),
            help: "Click the points it should pass through (or, with control, the points that pull it); Return ends it",
            params: r#"{"points": [[u,v], …], "control": false}  (2–1000 points; control: they're control points)"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let raw: Vec<[f64; 2]> = p.parse("points")?;
                if raw.len() < 2 || raw.len() > 1000 || raw.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(EngineError::BadParams("a spline needs 2–1000 finite points".into()));
                }
                let points: Vec<glam::DVec2> = raw.iter().map(|q| glam::DVec2::new(q[0], q[1])).collect();
                if points.windows(2).any(|w| w[0].distance(w[1]) < 1e-9) {
                    return Err(EngineError::BadParams("spline points must be distinct".into()));
                }
                if p.bool_or("control", false) {
                    return add_entity(s, p, SketchEntity::ControlSpline { poles: points });
                }
                add_entity(s, p, SketchEntity::Spline { points })
            },
        },
        CommandSpec {
            id: "set_entity",
            label: "Edit Sketch Element",
            menu: "",
            shortcut: None,
            help: "Replace a sketch element, e.g. after editing one of its dimensions (several at once with \"entities\")",
            params: r#"{"sketch": id, "entity": n, "value": {"type": "line", "a": [u,v], "b": [u,v]} | …} | {"sketch": id, "entities": [[n, value], …]}"#,
            modifies: true,
            enabled: |_| true,
            run: |s, p| {
                let id = p.id("sketch")?;
                let edits: Vec<(f64, SketchEntity)> =
                    if p.has("entities") { p.parse("entities")? } else { vec![(p.f64("entity")?, p.parse("value")?)] };
                if edits.is_empty() || edits.len() > 10_000 {
                    return Err(EngineError::BadParams("give 1–10000 elements".into()));
                }
                for (_, value) in &edits {
                    let ok = match value {
                        SketchEntity::Line { a, b } => a.distance(*b) > 1e-9,
                        SketchEntity::Rectangle { min, max } => max.x - min.x > 1e-9 && max.y - min.y > 1e-9,
                        SketchEntity::Circle { radius, .. } | SketchEntity::Polygon { radius, .. } => *radius > 1e-9,
                        SketchEntity::Ellipse { rx, ry, .. } => *rx > 1e-9 && *ry > 1e-9,
                        SketchEntity::Arc { a, mid, b } => shapecraft_doc::circle_through(*a, *mid, *b).is_some(),
                        SketchEntity::Spline { points } | SketchEntity::ControlSpline { poles: points } => {
                            (2..=1000).contains(&points.len())
                        }
                        SketchEntity::Text { text, size, .. } => {
                            !text.is_empty() && text.chars().count() <= shapecraft_doc::text::MAX_CHARS && *size > 1e-6
                        }
                    };
                    if !ok {
                        return Err(EngineError::BadParams("that size isn't possible".into()));
                    }
                }
                let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
                for (index, value) in &edits {
                    if !index.is_finite() || *index < 0.0 {
                        return Err(EngineError::BadParams("bad element index".into()));
                    }
                    let slot =
                        sketch.entities.get_mut(*index as usize).ok_or_else(|| EngineError::BadParams(format!("no element {index}")))?;
                    *slot = value.clone();
                }
                // An element edited outright keeps its driving dimension in step (its new length or radius).
                let entities = sketch.entities.clone();
                for c in &mut sketch.constraints {
                    match c {
                        Constraint::Distance { a, value, along } if edits.iter().any(|(i, _)| *i as u32 == a.entity()) => {
                            if let Some(SketchEntity::Line { a: p, b: q }) = entities.get(a.entity() as usize) {
                                *value = along.measure(*p, *q);
                            }
                        }
                        Constraint::Radius { a, value } if edits.iter().any(|(i, _)| *i as u32 == a.entity()) => {
                            match entities.get(a.entity() as usize) {
                                Some(SketchEntity::Circle { radius, .. }) => *value = *radius,
                                Some(SketchEntity::Arc { a: p, mid, b: q }) => {
                                    if let Some((_, r)) = shapecraft_doc::circle_through(*p, *mid, *q) {
                                        *value = r;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Constraint::Angle { a, b, value } if edits.iter().any(|(i, _)| [a.entity(), b.entity()].contains(&(*i as u32))) => {
                            if let (Some(SketchEntity::Line { a: p, b: q }), Some(SketchEntity::Line { a: r, b: t })) =
                                (entities.get(a.entity() as usize), entities.get(b.entity() as usize))
                            {
                                let (_, d1, d2) = shapecraft_doc::constraints::corner_rays((*p, *q), (*r, *t));
                                *value = shapecraft_doc::constraints::ray_angle(d1, d2).to_degrees();
                            }
                        }
                        _ => {}
                    }
                }
                Ok(json!({ "sketch": id.0, "entity": edits[0].0 as u64 }))
            },
        },
        CommandSpec {
            id: "circle",
            label: "Circle",
            menu: "Sketch/Circle",
            shortcut: Some("C"),
            help: "Place the center, then drag out the radius.",
            params: r#"{"center":[u,v], "radius": r}"#,
            modifies: true,
            enabled: in_sketch,
            run: |s, p| {
                let center = p.vec2("center")?;
                let radius = p.f64("radius")?;
                if radius <= 1e-9 {
                    return Err(EngineError::BadParams("radius must be positive".into()));
                }
                add_entity(s, p, SketchEntity::Circle { center, radius })
            },
        },
    ]
}

fn new_sketch(s: &mut Session, p: &Params) -> Result<Value> {
    // "look": false is for the open app (don't turn the camera to the new plane); headless it changes nothing.
    let _ = p.bool_or("look", true);
    let (plane, on_body) =
        plane_from(s, p)?.ok_or_else(|| EngineError::BadParams("give a plane, a face or an origin and normal".into()))?;
    let id = s.doc.alloc_id();
    let name = s.doc.next_sketch_name();
    s.doc.sketches.push(Sketch::new(id, name.clone(), plane, on_body));
    s.active_sketch = Some(id);
    s.selection.clear();
    // The frame, so a script knows where (u, v) land in the world without probing.
    let v = |d: glam::DVec3| [d.x, d.y, d.z];
    Ok(json!({
        "sketch": id.0, "name": name,
        "origin": v(plane.origin), "x": v(plane.x), "y": v(plane.y), "normal": v(plane.normal()),
    }))
}

/// The plane a command names: an origin plane, a planar face (and its body), a construction plane, or an origin
/// and normal. `None` when none is given.
pub(super) fn plane_from(s: &Session, p: &Params) -> Result<Option<(SketchPlane, Option<ItemId>)>> {
    Ok(Some(if let Some(name) = p.opt_str("plane") {
        let plane = match name.to_ascii_lowercase().as_str() {
            "xy" | "top" => SketchPlane::XY,
            "xz" | "front" => SketchPlane::XZ,
            "yz" | "right" => SketchPlane::YZ,
            other => return Err(EngineError::BadParams(format!("unknown plane '{other}'"))),
        };
        (plane, None)
    } else if p.has("body") {
        let body_id = p.id("body")?;
        let face = p.f64("face")? as u32;
        let body = s.doc.body(body_id).ok_or_else(|| EngineError::BadParams(format!("no body {}", body_id.0)))?;
        let fp = s
            .kernel
            .face_plane(&body.shape, face)?
            .ok_or_else(|| EngineError::NotAvailable("sketching on curved faces isn't supported yet".into()))?;
        let plane = SketchPlane::on_face(fp.origin, fp.normal, s.doc.legacy_face_axes)
            .ok_or_else(|| EngineError::BadParams("face has no usable normal".into()))?;
        (plane, Some(body_id))
    } else if p.has("construction") {
        let id = p.id("construction")?;
        let pl = s.doc.plane(id).ok_or_else(|| EngineError::BadParams(format!("no plane {}", id.0)))?;
        (pl.plane, None)
    } else if p.has("origin") {
        let mut plane = SketchPlane::from_normal(p.vec3("origin")?, p.vec3("normal")?)
            .ok_or_else(|| EngineError::BadParams("normal must not be zero".into()))?;
        // x_dir: where the sketch's u axis points (projected into the plane); v follows by the right-hand rule.
        if p.has("x_dir") {
            let n = plane.normal();
            let d = p.vec3("x_dir")?;
            let x =
                (d - n * d.dot(n)).try_normalize().ok_or_else(|| EngineError::BadParams("x_dir must not be along the normal".into()))?;
            plane = SketchPlane { origin: plane.origin, x, y: n.cross(x) };
        }
        (plane, None)
    } else {
        return Ok(None);
    }))
}

fn change_sketch_plane(s: &mut Session, p: &Params) -> Result<Value> {
    let id = p.id("sketch")?;
    let (plane, on_body) =
        plane_from(s, p)?.ok_or_else(|| EngineError::BadParams("give the new plane, face or construction plane".into()))?;
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    sketch.plane = plane;
    sketch.on_body = on_body;
    Ok(json!({ "sketch": id.0 }))
}

fn break_curve(s: &mut Session, p: &Params) -> Result<Value> {
    let id = target_sketch(s, p)?;
    let index = p.f64("entity")?;
    let at = p.vec2("at")?;
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    let i = (index >= 0.0)
        .then_some(index as usize)
        .filter(|i| *i < sketch.entities.len())
        .ok_or_else(|| EngineError::BadParams("no such curve".into()))?;
    let near_end = |p: DVec2, a: DVec2, b: DVec2| p.distance(a) < 1e-6 || p.distance(b) < 1e-6;
    let halves = match sketch.entities[i].clone() {
        SketchEntity::Line { a, b } => {
            let t = ((at - a).dot(b - a) / (b - a).length_squared().max(1e-18)).clamp(0.0, 1.0);
            let m = a + (b - a) * t;
            if near_end(m, a, b) {
                return Err(EngineError::NotAvailable("pick a point inside the line".into()));
            }
            [SketchEntity::Line { a, b: m }, SketchEntity::Line { a: m, b }]
        }
        SketchEntity::Arc { a, mid, b } => {
            let (c, r) =
                shapecraft_doc::circle_through(a, mid, b).ok_or_else(|| EngineError::NotAvailable("that arc is straight".into()))?;
            let ang = |q: DVec2| (q - c).y.atan2((q - c).x);
            // Walk from a towards b through mid; keep angles increasing in that sense.
            let ccw = (mid - a).perp_dot(b - a) < 0.0;
            let rel = |q: DVec2| {
                let d = ang(q) - ang(a);
                let d = if ccw { d } else { -d };
                d.rem_euclid(std::f64::consts::TAU)
            };
            let (tb, tm) = (rel(b), rel(at));
            if tm <= 1e-6 || tm >= tb - 1e-6 {
                return Err(EngineError::NotAvailable("pick a point inside the arc".into()));
            }
            let at_angle = |t: f64| {
                let base = ang(a) + if ccw { t } else { -t };
                c + DVec2::new(base.cos(), base.sin()) * r
            };
            let m = at_angle(tm);
            [SketchEntity::Arc { a, mid: at_angle(tm / 2.0), b: m }, SketchEntity::Arc { a: m, mid: at_angle((tm + tb) / 2.0), b }]
        }
        SketchEntity::Spline { points } if points.len() >= 2 => {
            // Into the span nearest the point, then split there: the two halves share it.
            let (k, m) = points
                .windows(2)
                .enumerate()
                .map(|(k, w)| {
                    let t = ((at - w[0]).dot(w[1] - w[0]) / (w[1] - w[0]).length_squared().max(1e-18)).clamp(0.0, 1.0);
                    (k, w[0] + (w[1] - w[0]) * t)
                })
                .min_by(|x, y| x.1.distance(at).total_cmp(&y.1.distance(at)))
                .ok_or_else(|| EngineError::NotAvailable("that spline is too short".into()))?;
            let mut first: Vec<DVec2> = points[..=k].to_vec();
            let mut second: Vec<DVec2> = points[k + 1..].to_vec();
            if first.last().is_none_or(|l| l.distance(m) > 1e-9) {
                first.push(m);
            }
            if second.first().is_none_or(|f| f.distance(m) > 1e-9) {
                second.insert(0, m);
            }
            if first.len() < 2 || second.len() < 2 {
                return Err(EngineError::NotAvailable("pick a point inside the spline".into()));
            }
            [SketchEntity::Spline { points: first }, SketchEntity::Spline { points: second }]
        }
        _ => return Err(EngineError::NotAvailable("only lines, arcs and splines can be broken".into())),
    };
    sketch.entities.splice(i..=i, halves);
    s.selection.retain(|pk| !matches!(pk, Pick::SketchEntity { sketch, .. } if *sketch == id));
    Ok(json!({ "sketch": id.0, "entities": [i, i + 1] }))
}

fn exit_sketching(s: &mut Session, _: &Params) -> Result<Value> {
    if let Some(id) = s.active_sketch.take()
        && s.doc.sketch(id).is_some_and(|sk| sk.entities.is_empty())
    {
        s.doc.remove_item(id);
    }
    s.selection.retain(|p| !matches!(p, Pick::SketchEntity { .. }));
    Ok(Value::Null)
}

/// The sketch elements a constraint command works on: `entities`, else the selected ones in selection order.
fn picked_entities(s: &Session, p: &Params) -> Result<(ItemId, Vec<u32>)> {
    let id = target_sketch(s, p)?;
    let ents: Vec<u32> = if p.has("entities") {
        p.u32s("entities")?
    } else {
        s.selection
            .iter()
            .filter_map(|pk| if let Pick::SketchEntity { sketch, entity } = *pk { (sketch == id).then_some(entity) } else { None })
            .collect()
    };
    let count = s.doc.sketch(id).map_or(0, |k| k.entities.len());
    if ents.is_empty() || ents.iter().any(|&e| e as usize >= count) {
        return Err(EngineError::NotAvailable("select sketch elements first".into()));
    }
    Ok((id, ents))
}

fn constrain(s: &mut Session, p: &Params) -> Result<Value> {
    let kind = p.str("kind")?;
    if kind == "point_distance" {
        return point_distance(s, p);
    }
    let (id, ents) = picked_entities(s, p)?;
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    // Lock takes every selected element (as observed s14-145: all of them turn green).
    if kind == "lock" && ents.len() > 1 {
        let before = sketch.constraints.len();
        for &e in &ents {
            let c = Constraint::Lock { entity: e };
            if (e as usize) < sketch.entities.len() && !sketch.constraints.contains(&c) {
                sketch.constraints.push(c);
            }
        }
        if sketch.constraints.len() == before {
            return Err(EngineError::NotAvailable("already locked".into()));
        }
        return Ok(json!({ "constraint": sketch.constraints.len() - 1, "kind": "Lock" }));
    }
    let c = match kind {
        // Driving dimensions: a line's length (or a side's: "index"), a circle's radius.
        "distance" | "radius" => {
            let value = p.f64("value")?;
            if !(value.is_finite() && value > 1e-9 && value < 1e9) {
                return Err(EngineError::BadParams("a dimension must be a positive number".into()));
            }
            let entity = *ents.first().ok_or_else(|| EngineError::NotAvailable("select an element first".into()))?;
            if kind == "distance" {
                let index = p.f64_or("index", 0.0)?.clamp(0.0, 1e6) as u32;
                // "along": a Distance Type of horizontal or vertical measures across that sketch axis only.
                let along = match p.opt_str("along") {
                    None | Some("absolute") => shapecraft_doc::Along::Absolute,
                    Some("horizontal") => shapecraft_doc::Along::Horizontal,
                    Some("vertical") => shapecraft_doc::Along::Vertical,
                    Some(other) => return Err(EngineError::BadParams(format!("no distance type '{other}'"))),
                };
                Constraint::Distance { a: shapecraft_doc::constraints::Element::Segment { entity, index }, value, along }
            } else {
                Constraint::Radius { a: shapecraft_doc::constraints::Element::Curve { entity }, value }
            }
        }
        // The angle between two lines (degrees): the later-picked line turns about the corner they share, keeping
        // its length (s15-009), before the solver checks the rest.
        "angle" => {
            let value = p.f64("value")?;
            if !(value.is_finite() && value > 1e-6 && value < 180.0 - 1e-6) {
                return Err(EngineError::BadParams("an angle must be between 0° and 180°".into()));
            }
            let (Some(&ea), Some(&eb)) = (ents.first(), ents.get(1)) else {
                return Err(EngineError::NotAvailable("select two lines first".into()));
            };
            let (Some(SketchEntity::Line { a: p0, b: p1 }), Some(SketchEntity::Line { a: q0, b: q1 })) =
                (sketch.entities.get(ea as usize).cloned(), sketch.entities.get(eb as usize).cloned())
            else {
                return Err(EngineError::NotAvailable("pick two straight lines".into()));
            };
            let (corner, d1, d2) = shapecraft_doc::constraints::corner_rays((p0, p1), (q0, q1));
            if d1.length() < 1e-9 || d2.length() < 1e-9 {
                return Err(EngineError::NotAvailable("pick two straight lines".into()));
            }
            // Keep the side the second line is on; turn it about the end nearest the corner.
            let side = if d1.perp_dot(d2) < 0.0 { -1.0 } else { 1.0 };
            let dir = DVec2::from_angle(d1.to_angle() + side * value.to_radians()) * d2.length();
            let near = if q0.distance(corner) <= q1.distance(corner) { q0 } else { q1 };
            let turned = if near == q0 { SketchEntity::Line { a: q0, b: q0 + dir } } else { SketchEntity::Line { a: q1 + dir, b: q1 } };
            if let Some(slot) = sketch.entities.get_mut(eb as usize) {
                *slot = turned;
            }
            Constraint::Angle {
                a: shapecraft_doc::constraints::Element::Segment { entity: ea, index: 0 },
                b: shapecraft_doc::constraints::Element::Segment { entity: eb, index: 0 },
                value,
            }
        }
        _ => shapecraft_doc::constraints::constraint_for(kind, &sketch.entities, &ents).map_err(|m| EngineError::NotAvailable(m.into()))?,
    };
    // A new angle between the same two lines replaces the old one.
    if let Constraint::Angle { a, b, .. } = c {
        sketch.constraints.retain(|o| !matches!(o, Constraint::Angle { a: x, b: y, .. } if (*x == a && *y == b) || (*x == b && *y == a)));
    }
    // A new value for a dimension that's already there replaces it.
    if let Constraint::Distance { a, .. } | Constraint::Radius { a, .. } = c {
        sketch.constraints.retain(|o| !matches!(o, Constraint::Distance { a: b, .. } | Constraint::Radius { a: b, .. } if *b == a));
    }
    if sketch.constraints.contains(&c) {
        return Err(EngineError::NotAvailable(format!("already {}", c.label().to_lowercase())));
    }
    let mut all = sketch.constraints.clone();
    all.push(c.clone());
    // The first element picked holds still and the others move to meet it, as in the reference app.
    let mut pinned: std::collections::BTreeSet<u32> = ents.first().copied().into_iter().filter(|_| ents.len() > 1).collect();
    // Symmetry mirrors the second element across the line: the line holds still too.
    if let Constraint::Symmetric { b, axis, .. } = c
        && axis.entity() != b.entity()
    {
        pinned.insert(axis.entity());
    }
    let solved = shapecraft_doc::constraints::solve(&sketch.entities, &all, &pinned)
        .or_else(|| shapecraft_doc::constraints::solve(&sketch.entities, &all, &Default::default()))
        .ok_or_else(|| {
            EngineError::NotAvailable(format!("{} can't be added: it conflicts with the sketch's other constraints", c.label()))
        })?;
    sketch.entities = solved;
    sketch.constraints = all;
    Ok(json!({ "constraint": sketch.constraints.len() - 1, "kind": c.label() }))
}

/// A driving distance from a point (s15-012, s15-014): to another point (aligned) or square to a straight line. The
/// elements come from `elements` or the selection (picked points, then a picked line); the second one moves to meet
/// the value, the first holds still.
fn point_distance(s: &mut Session, p: &Params) -> Result<Value> {
    use shapecraft_doc::constraints::{Element, points};
    let id = target_sketch(s, p)?;
    let value = p.f64("value")?;
    if !(value.is_finite() && value > 1e-9 && value < 1e9) {
        return Err(EngineError::BadParams("a dimension must be a positive number".into()));
    }
    let els: Vec<Element> = if p.has("elements") {
        p.parse("elements")?
    } else {
        let pts = s.selection.iter().filter_map(|pk| match *pk {
            Pick::SketchPoint { sketch, entity, index } if sketch == id => Some(Element::Point { entity, index }),
            _ => None,
        });
        let lines = s.selection.iter().filter_map(|pk| match *pk {
            Pick::SketchEntity { sketch, entity } if sketch == id => Some(Element::Segment { entity, index: 0 }),
            _ => None,
        });
        pts.chain(lines).collect()
    };
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    let ok_point = |e: &Element| matches!(*e, Element::Point { entity, index } if sketch.entities.get(entity as usize).is_some_and(|x| (index as usize) < points(x).len()));
    let ok_line = |e: &Element| matches!(*e, Element::Segment { entity, index } if index == 0 && matches!(sketch.entities.get(entity as usize), Some(SketchEntity::Line { .. })));
    let (a, b) = match els[..] {
        [a, b] if ok_point(&a) && (ok_point(&b) || ok_line(&b)) && a != b => (a, b),
        _ => return Err(EngineError::NotAvailable("pick two points, or a point and a line".into())),
    };
    let c = Constraint::PointDistance { a, b, value };
    sketch
        .constraints
        .retain(|o| !matches!(o, Constraint::PointDistance { a: x, b: y, .. } if (*x == a && *y == b) || (*x == b && *y == a)));
    let mut all = sketch.constraints.clone();
    all.push(c);
    let pinned: std::collections::BTreeSet<u32> = [a.entity()].into_iter().filter(|e| *e != b.entity()).collect();
    let solved = shapecraft_doc::constraints::solve(&sketch.entities, &all, &pinned)
        .or_else(|| shapecraft_doc::constraints::solve(&sketch.entities, &all, &Default::default()))
        .ok_or_else(|| EngineError::NotAvailable("Distance can't be added: it conflicts with the sketch's other constraints".into()))?;
    sketch.entities = solved;
    sketch.constraints = all;
    Ok(json!({ "constraint": sketch.constraints.len() - 1, "kind": "Distance" }))
}

fn target_sketch(s: &Session, p: &Params) -> Result<ItemId> {
    p.opt_id("sketch").or(s.active_sketch).ok_or_else(|| EngineError::NotAvailable("start a sketch first".into()))
}

fn add_entity(s: &mut Session, p: &Params, e: SketchEntity) -> Result<Value> {
    let id = target_sketch(s, p)?;
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    if sketch.entities.len() >= 100_000 {
        return Err(EngineError::BadParams("sketch is full".into()));
    }
    // A point drawn onto the origin (where its snap puts it) stays there, as in the reference: a shape drawn from
    // the origin with its sizes typed is fully defined.
    let origin = sketch.plane.to_local(glam::DVec3::ZERO);
    let index = sketch.entities.len();
    let anchor = shapecraft_doc::constraints::points(&e).iter().position(|p| p.distance(origin) < 1e-9);
    if let (Some(at), Some(o)) = (anchor, shapecraft_doc::Element::origin_at(origin)) {
        let a = shapecraft_doc::Element::Point { entity: index as u32, index: at as u32 };
        sketch.constraints.push(Constraint::Coincident { a, b: o });
    }
    sketch.entities.push(e);
    sketch.visible = true;
    Ok(json!({ "sketch": id.0, "entity": index }))
}

/// Keeps four new lines a rectangle when one of them is dimensioned or dragged: corners joined, sides level and
/// upright (`level`), or square to each other for a turned one.
fn hold_rectangle(s: &mut Session, out: Value, level: bool) -> Result<Value> {
    use shapecraft_doc::{Constraint as C, Element as El};
    let (Some(id), Some(ents)) = (out["sketch"].as_u64(), out["entities"].as_array()) else { return Ok(out) };
    let e: Vec<u32> = ents.iter().filter_map(|v| v.as_u64().and_then(|n| u32::try_from(n).ok())).collect();
    let [e0, e1, e2, e3] = e[..] else { return Ok(out) };
    let Some(sketch) = s.doc.sketch_mut(ItemId(id)) else { return Ok(out) };
    let side = |entity| El::Segment { entity, index: 0 };
    for (x, y) in [(e0, e1), (e1, e2), (e2, e3), (e3, e0)] {
        sketch.constraints.push(C::Coincident { a: El::Point { entity: x, index: 1 }, b: El::Point { entity: y, index: 0 } });
    }
    if level {
        sketch.constraints.extend([
            C::Horizontal { a: side(e0) },
            C::Vertical { a: side(e1) },
            C::Horizontal { a: side(e2) },
            C::Vertical { a: side(e3) },
        ]);
    } else {
        sketch.constraints.extend([
            C::Perpendicular { a: side(e0), b: side(e1) },
            C::Parallel { a: side(e0), b: side(e2) },
            C::Parallel { a: side(e1), b: side(e3) },
        ]);
    }
    Ok(out)
}

/// Adds several elements as one shape; reports the first and all of them.
fn add_lines(s: &mut Session, p: &Params, lines: [SketchEntity; 4]) -> Result<Value> {
    let mut entities = Vec::new();
    let mut sketch = Value::Null;
    for e in lines {
        let v = add_entity(s, p, e)?;
        sketch = v["sketch"].clone();
        entities.push(v["entity"].clone());
    }
    Ok(json!({ "sketch": sketch, "entity": entities[0], "entities": entities }))
}

fn trim(s: &mut Session, p: &Params) -> Result<Value> {
    let id = target_sketch(s, p)?;
    let entity = p.f64("entity")?;
    let at: [f64; 2] = p.parse("at")?;
    if !entity.is_finite() || entity < 0.0 || !at.iter().all(|v| v.is_finite()) {
        return Err(EngineError::BadParams("trim needs a curve index and a finite point".into()));
    }
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    let (list, _) = shapecraft_doc::sketch_ops::trim(&sketch.entities, entity as usize, glam::DVec2::from_array(at))
        .map_err(|m| EngineError::NotAvailable(m.into()))?;
    sketch.entities = list;
    let count = sketch.entities.len();
    s.selection.retain(|pk| !matches!(pk, Pick::SketchEntity { sketch, .. } if *sketch == id));
    Ok(json!({ "sketch": id.0, "entities": count }))
}

fn sketch_fillet(s: &mut Session, p: &Params) -> Result<Value> {
    let id = target_sketch(s, p)?;
    let r = p.f64("radius")?;
    if !(r.is_finite() && r > 1e-6 && r < 1e6) {
        return Err(EngineError::BadParams("the fillet radius must be a positive number".into()));
    }
    let picked: Vec<usize> = if p.has("entities") {
        p.parse::<Vec<f64>>("entities")?.into_iter().filter(|e| e.is_finite() && *e >= 0.0).map(|e| e as usize).collect()
    } else {
        s.selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::SketchEntity { sketch, entity } if sketch == id => Some(entity as usize),
                _ => None,
            })
            .collect()
    };
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    let corners: Vec<glam::DVec2> = if p.has("corners") {
        let raw: Vec<[f64; 2]> = p.parse("corners")?;
        if raw.len() > 10_000 || !raw.iter().flatten().all(|v| v.is_finite()) {
            return Err(EngineError::BadParams("corners must be finite points".into()));
        }
        raw.into_iter().map(glam::DVec2::from_array).collect()
    } else {
        shapecraft_doc::sketch_ops::line_corners(&sketch.entities, (!picked.is_empty()).then_some(picked.as_slice()))
    };
    let list =
        shapecraft_doc::sketch_ops::fillet_corners(&sketch.entities, &corners, r).map_err(|m| EngineError::NotAvailable(m.into()))?;
    let added = list.len() - sketch.entities.len();
    sketch.entities = list;
    Ok(json!({ "sketch": id.0, "arcs": added }))
}

fn offset_edge(s: &mut Session, p: &Params) -> Result<Value> {
    let distance = p.f64("distance")?;
    if !distance.is_finite() || distance.abs() < 1e-9 || distance.abs() > 1e6 {
        return Err(EngineError::BadParams("offset distance must be a non-zero number".into()));
    }
    // A flat body face: its outline offset into a new sketch on it (s24-008..010).
    if p.has("face") {
        let (body, face) = (p.id("body")?, p.f64("face")? as u32);
        let (plane, on_body, added) = face_offset(s, body, face, distance)?;
        let id = s.doc.alloc_id();
        let name = s.doc.next_sketch_name();
        let mut sketch = Sketch::new(id, name.clone(), plane, on_body);
        sketch.entities = added;
        s.doc.sketches.push(sketch);
        s.selection.clear();
        return Ok(json!({ "sketch": id.0, "name": name }));
    }
    let (id, entities, region): (ItemId, Vec<u32>, Option<u32>) = if p.has("region") {
        (target_sketch(s, p)?, Vec::new(), Some(p.f64("region")? as u32))
    } else if p.has("entities") {
        (
            target_sketch(s, p)?,
            p.parse::<Vec<f64>>("entities")?.into_iter().filter(|e| e.is_finite() && *e >= 0.0).map(|e| e as u32).collect(),
            None,
        )
    } else if let Some((sketch, region)) = s.selection.iter().find_map(|pk| match *pk {
        Pick::Region { sketch, region } => Some((sketch, region)),
        _ => None,
    }) {
        (sketch, Vec::new(), Some(region))
    } else {
        let mut sketch = None;
        let mut ents = Vec::new();
        for pk in &s.selection {
            if let Pick::SketchEntity { sketch: sk, entity } = *pk
                && sketch.is_none_or(|x| x == sk)
            {
                sketch = Some(sk);
                ents.push(entity);
            }
        }
        (sketch.ok_or_else(|| EngineError::NotAvailable("select sketch curves or a region to offset".into()))?, ents, None)
    };
    if entities.len() > 10_000 {
        return Err(EngineError::BadParams("too many curves".into()));
    }
    let added = offset_entities(s, id, &entities, region, distance)?;
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    if sketch.entities.len() + added.len() > 100_000 {
        return Err(EngineError::BadParams("too many curves".into()));
    }
    let first = sketch.entities.len() as u32;
    let n = added.len() as u32;
    sketch.entities.extend(added);
    sketch.visible = true;
    link_offsets(sketch, &entities, first, distance.abs());
    s.selection = (first..first + n).map(|entity| Pick::SketchEntity { sketch: id, entity }).collect();
    Ok(json!({ "sketch": id.0, "entities": (first..first + n).collect::<Vec<_>>() }))
}

/// Offset copies stay linked to their curves: a line keeps parallel at the offset (a driving distance), a circle or
/// arc concentric. Each new curve is matched to the source it was offset from by its geometry.
fn link_offsets(sketch: &mut Sketch, sources: &[u32], first: u32, distance: f64) {
    use shapecraft_doc::constraints::{Element, curve};
    let size = sketch.entities.iter().flat_map(shapecraft_doc::constraints::points).fold(1.0f64, |m, p| m.max(p.length()));
    let tol = size * 1e-6;
    let mut added = Vec::new();
    for j in first as usize..sketch.entities.len() {
        let made = &sketch.entities[j];
        for &i in sources {
            let Some(src) = sketch.entities.get(i as usize) else { continue };
            match (src, made) {
                (SketchEntity::Line { a, b }, SketchEntity::Line { a: c, b: d }) => {
                    let u = (*b - *a).normalize_or_zero();
                    let parallel = u.perp_dot((*d - *c).normalize_or_zero()).abs() < 1e-9;
                    if parallel && ((*c - *a).perp_dot(u).abs() - distance).abs() < tol {
                        added.push(Constraint::Parallel {
                            a: Element::Segment { entity: i, index: 0 },
                            b: Element::Segment { entity: j as u32, index: 0 },
                        });
                        added.push(Constraint::PointDistance {
                            a: Element::Point { entity: j as u32, index: 0 },
                            b: Element::Segment { entity: i, index: 0 },
                            value: distance,
                        });
                        break;
                    }
                }
                (SketchEntity::Circle { .. } | SketchEntity::Arc { .. }, SketchEntity::Circle { .. } | SketchEntity::Arc { .. }) => {
                    if let (Some((c0, r0)), Some((c1, r1))) = (curve(src), curve(made))
                        && c0.distance(c1) < tol
                        && ((r0 - r1).abs() - distance).abs() < tol
                    {
                        added.push(Constraint::Concentric { a: Element::Curve { entity: i }, b: Element::Curve { entity: j as u32 } });
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let mut all = sketch.constraints.clone();
    all.extend(added);
    if shapecraft_doc::constraints::error(&sketch.entities, &all) < tol * 100.0 {
        sketch.constraints = all;
    }
}

/// Offset Edge on a body edge, as observed (347–350): the copy lands in a new sketch on the face, nothing else
/// changes.
fn offset_body_edge(s: &mut Session, p: &Params) -> Result<Value> {
    let (body, edge, face) = (p.id("body")?, p.f64("edge")? as u32, p.f64("face")? as u32);
    let distance = p.f64("distance")?;
    if !distance.is_finite() || distance.abs() < 1e-9 || distance.abs() > 1e6 {
        return Err(EngineError::BadParams("offset distance must be a non-zero number".into()));
    }
    let on_face = json!({ "body": body.0, "face": face });
    let (plane, on_body) =
        plane_from(s, &Params::new(&on_face))?.ok_or_else(|| EngineError::NotAvailable("the edge's face isn't flat".into()))?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let fp = s.kernel.face_plane(&b.shape, face)?.ok_or_else(|| EngineError::NotAvailable("the edge's face isn't flat".into()))?;
    let curve = edge_entity(&plane, s.kernel.edge_curve(&b.shape, edge)?)
        .ok_or_else(|| EngineError::NotAvailable("that edge can't be offset".into()))?;
    // Into the face: whichever side brings the curve's middle nearer the face's middle.
    let center = plane.to_local(fp.origin);
    let middle = |e: &SketchEntity| {
        let o = e.outline();
        o.get(o.len() / 2).copied().unwrap_or(DVec2::ZERO)
    };
    let pick = |d: f64| shapecraft_doc::sketch_ops::offset(&curve, d);
    let (a, b) = (pick(distance), pick(-distance));
    let inward = match (a, b) {
        (Some(a), Some(b)) => {
            let (da, db) = (middle(&a).distance(center), middle(&b).distance(center));
            if (da < db) == (distance > 0.0) { a } else { b }
        }
        (Some(e), None) | (None, Some(e)) => e,
        (None, None) => return Err(EngineError::NotAvailable("that offset would collapse the curve".into())),
    };
    let id = s.doc.alloc_id();
    let name = s.doc.next_sketch_name();
    let mut sketch = Sketch::new(id, name.clone(), plane, on_body);
    sketch.entities.push(inward);
    s.doc.sketches.push(sketch);
    s.selection.clear();
    Ok(json!({ "sketch": id.0, "name": name }))
}

/// An edge flattened onto a sketch plane: exact when it lies in the plane, else a line or spline.
fn edge_entity(plane: &SketchPlane, curve: EdgeCurve) -> Option<SketchEntity> {
    let n = plane.normal();
    let flat = |v: DVec3| v.cross(n).length() < 1e-6 * v.length().max(1e-12);
    let local = |q: DVec3| plane.to_local(q);
    match curve {
        EdgeCurve::Line { a, b } => shapecraft_doc::sketch_ops::fit_polyline(&[local(a), local(b)]),
        EdgeCurve::Circle { center, normal, radius } if flat(normal) => Some(SketchEntity::Circle { center: local(center), radius }),
        EdgeCurve::Circle { center, normal, radius } => {
            shapecraft_doc::sketch_ops::fit_polyline(&circle3_points(center, normal, radius, 48).into_iter().map(local).collect::<Vec<_>>())
        }
        EdgeCurve::Arc { a, mid, b } if flat((mid - a).cross(b - a)) => {
            Some(SketchEntity::Arc { a: local(a), mid: local(mid), b: local(b) })
        }
        EdgeCurve::Arc { a, mid, b } => {
            shapecraft_doc::sketch_ops::fit_polyline(&arc3_points(a, mid, b, 32).into_iter().map(local).collect::<Vec<_>>())
        }
        EdgeCurve::Other { points } => shapecraft_doc::sketch_ops::fit_polyline(&points.into_iter().map(local).collect::<Vec<_>>()),
    }
}

/// A flat body face's outline offset as a whole (outwards when positive), flattened onto the face's sketch plane.
pub fn face_offset(s: &Session, body: ItemId, face: u32, distance: f64) -> Result<(SketchPlane, Option<ItemId>, Vec<SketchEntity>)> {
    let on_face = json!({ "body": body.0, "face": face });
    let (plane, on_body) = plane_from(s, &Params::new(&on_face))?.ok_or_else(|| EngineError::NotAvailable("the face isn't flat".into()))?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let shape = s.kernel.offset_face_outline(&s.kernel.face(&b.shape, face)?, distance)?;
    let n = s.kernel.edge_count(&shape)?;
    let out: Vec<SketchEntity> =
        (0..n.min(10_000)).filter_map(|i| s.kernel.edge_curve(&shape, i).ok().and_then(|c| edge_entity(&plane, c))).collect();
    if out.is_empty() {
        return Err(EngineError::NotAvailable("that offset would collapse the face's outline".into()));
    }
    Ok((plane, on_body, out))
}

/// What Offset Edge adds to a sketch: a region's outline offset as a whole, a connected chain of curves with
/// joined corners, or each curve on its own. Positive is outwards (or to the left of a lone line).
pub fn offset_entities(s: &Session, sketch: ItemId, entities: &[u32], region: Option<u32>, distance: f64) -> Result<Vec<SketchEntity>> {
    let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
    let from_shape = |shape: &shapecraft_kernel::Shape| -> Result<Vec<SketchEntity>> {
        let n = s.kernel.edge_count(shape)?;
        Ok((0..n.min(10_000)).filter_map(|i| s.kernel.edge_curve(shape, i).ok().and_then(|c| edge_entity(&sk.plane, c))).collect())
    };
    if let Some(r) = region {
        let face = &sk.region(r).ok_or_else(|| EngineError::BadParams(format!("no region {r}")))?.face;
        return from_shape(&s.kernel.offset_face_outline(face, distance)?);
    }
    let picked: Vec<&SketchEntity> = entities
        .iter()
        .map(|&i| sk.entities.get(i as usize).ok_or_else(|| EngineError::BadParams(format!("no sketch curve {i}"))))
        .collect::<Result<_>>()?;
    if picked.len() >= 2 {
        let mut one = sk.clone();
        one.entities = picked.iter().map(|e| (*e).clone()).collect();
        if let Ok(shape) = s.kernel.offset_curves(sk.frame(), &one.curves(), distance) {
            let out = from_shape(&shape)?;
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }
    picked
        .iter()
        .map(|e| {
            shapecraft_doc::sketch_ops::offset(e, distance)
                .ok_or_else(|| EngineError::NotAvailable("that offset would collapse the curve (or it's an ellipse)".into()))
        })
        .collect()
}

/// Points along the 3D arc a → mid → b.
fn arc3_points(a: DVec3, mid: DVec3, b: DVec3, n: usize) -> Vec<DVec3> {
    let Some(plane) = SketchPlane::from_normal(a, (mid - a).cross(b - a)) else { return vec![a, mid, b] };
    let l = |p: DVec3| plane.to_local(p);
    shapecraft_doc::arc_points(l(a), l(mid), l(b), n).into_iter().map(|p| plane.to_world(p)).collect()
}

fn circle3_points(center: DVec3, normal: DVec3, radius: f64, n: usize) -> Vec<DVec3> {
    let Some(plane) = SketchPlane::from_normal(center, normal) else { return Vec::new() };
    (0..=n).map(|i| plane.to_world(DVec2::from_angle(i as f64 / n as f64 * std::f64::consts::TAU) * radius)).collect()
}

fn project_sketch(s: &mut Session, p: &Params) -> Result<Value> {
    let to: Value = p.parse("to")?;
    // A construction plane target is {"plane": id}; the sketch command calls it "construction".
    let on = match to.get("plane").and_then(Value::as_u64) {
        Some(id) => json!({ "construction": id }),
        None => to.clone(),
    };
    let (plane, on_body) =
        plane_from(s, &Params::new(&on))?.ok_or_else(|| EngineError::NotAvailable("project onto a flat face or a plane".into()))?;
    let items: Vec<Value> = p.parse("items")?;
    let (mut edges, mut faces) = (Vec::new(), Vec::new());
    for v in items.iter().take(10_000) {
        if v.get("edge").is_some() {
            edges.push(v.clone());
        } else if v.get("face").is_some() {
            faces.push(v.clone());
        }
    }
    if edges.is_empty() && faces.is_empty() {
        return Err(EngineError::NotAvailable("pick edges or faces to project".into()));
    }
    let id = s.doc.alloc_id();
    let name = s.doc.next_sketch_name();
    s.doc.sketches.push(Sketch::new(id, name.clone(), plane, on_body));
    project(s, &Params::new(&json!({ "sketch": id.0, "edges": edges, "faces": faces })))?;
    s.selection.clear();
    Ok(json!({ "sketch": id.0, "name": name }))
}

fn project(s: &mut Session, p: &Params) -> Result<Value> {
    let id = target_sketch(s, p)?;
    let mut edges: Vec<(ItemId, u32)> = Vec::new();
    let mut faces: Vec<(ItemId, u32)> = Vec::new();
    if p.has("edges") || p.has("faces") {
        for v in p.parse::<Vec<Value>>("edges").unwrap_or_default() {
            let e = Params::new(&v);
            edges.push((e.id("body")?, e.f64("edge")? as u32));
        }
        for v in p.parse::<Vec<Value>>("faces").unwrap_or_default() {
            let f = Params::new(&v);
            faces.push((f.id("body")?, f.f64("face")? as u32));
        }
    } else {
        for pk in &s.selection {
            match *pk {
                Pick::Edge { body, edge } => edges.push((body, edge)),
                Pick::Face { body, face } => faces.push((body, face)),
                _ => {}
            }
        }
    }
    if edges.len() + faces.len() > 10_000 {
        return Err(EngineError::BadParams("too many edges".into()));
    }
    for (body, face) in faces {
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        for e in s.kernel.face_edges(&b.shape, face)? {
            if !edges.contains(&(body, e)) {
                edges.push((body, e));
            }
        }
    }
    if edges.is_empty() {
        return Err(EngineError::NotAvailable("select edges or faces to project".into()));
    }
    let plane = s.doc.sketch(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?.plane;
    let mut made = Vec::new();
    for (body, edge) in edges {
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        made.extend(edge_entity(&plane, s.kernel.edge_curve(&b.shape, edge)?));
    }
    let sketch = s.doc.sketch_mut(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
    if sketch.entities.len() + made.len() > 100_000 {
        return Err(EngineError::BadParams("sketch is full".into()));
    }
    let first = sketch.entities.len() as u32;
    let count = made.len() as u32;
    sketch.entities.extend(made);
    s.selection = (first..first + count).map(|entity| Pick::SketchEntity { sketch: id, entity }).collect();
    Ok(json!({ "sketch": id.0, "entities": (first..first + count).collect::<Vec<_>>() }))
}

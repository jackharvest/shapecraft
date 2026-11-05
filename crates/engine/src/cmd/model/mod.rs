//! Modeling commands. The registry table is here, with the helpers every feature shares; the features live in
//! submodules by family.

use std::sync::Arc;

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{Body, ItemId, Pick};
use shapecraft_kernel::{BoolOp, Shape};

use crate::cmd::CommandSpec;
use crate::{EngineError, Params, Result, Session};
use boolean::{boolean_spec, split_body};
use extrude::extrude;
pub use extrude::extrude_auto_body;
use faces::{chamfer, edge_target, fillet, move_edge, move_faces, offset_face, project_edge, replace_face, shell};
pub use sweep::revolve_auto;
use sweep::{loft, revolve, sweep};
use transform::{align, mirror, pattern, pattern_circular, rotate, scale, translate};
use wrap::wrap_emboss;

mod boolean;
mod extrude;
mod faces;
mod sweep;
mod transform;
mod wrap;

pub(crate) use extrude::extrude_block;

fn has_bodies(s: &Session) -> bool {
    !s.doc.bodies.is_empty()
}

/// Transforms also work on selected sketch curves.
fn has_bodies_or_curves(s: &Session) -> bool {
    has_bodies(s) || !s.doc.planes.is_empty() || s.selection.iter().any(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Region { .. }))
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "extrude",
            label: "Extrude - One-Sided",
            menu: "Tools/Extrude/One-Sided",
            shortcut: Some("E"),
            help: "Pull the arrow or type a distance; negative cuts",
            params: r#"{"distance": mm, "sketch": id, "region": n | "regions": [n, …]} | {"distance": mm, "body": id, "face": n}, "draft": degrees (0; positive leans the walls in), "sides": "one"|"symmetric", "extent": "distance"|"to_object"|"through_all" (distance then only gives the side, default -1), "to": {"body": id, "face": n}|{"plane": id} (to_object), "start_offset": mm, "start_plane": {"body", "face"}|{"plane"}? (From Plane), "result": "auto"|"new"|"union"|"subtract"|"intersect"  (default: the selected region or face; negative cuts)"#,
            modifies: true,
            enabled: |_| true,
            run: extrude,
        },
        boolean_spec("union", "Union", "Tools/Union", "⌘U", "Merge the selected bodies into the first one", BoolOp::Union),
        boolean_spec("subtract", "Subtract", "Tools/Subtract", "⌘B", "Cut the other bodies out of the first one", BoolOp::Subtract),
        boolean_spec("intersect", "Intersect", "Tools/Intersect", "⌘I", "Keep only what the selected bodies share", BoolOp::Intersect),
        CommandSpec {
            id: "fillet",
            label: "Chamfer/Fillet",
            menu: "Tools/Chamfer/Fillet/Auto",
            shortcut: Some("F"),
            help: "Drag the arrow to round the selected edges",
            params: r#"{"radius": mm, "radius2": mm? (variable: radius at each edge's start, radius2 at its end), "continuity": "g1"|"g2"? (tangent, the default, or curvature-continuous: straight outside edges between flat faces), "curvature": -1..1? (profile: -1 flat, 0 round, 1 sharp; straight edges between flat faces), "mode": "radius"|"chordal"?, "corner": "rolling_ball"|"setback"?, "body": id?, "edges": [n]?}  (default: the selected edges; all edges if a body is selected)"#,
            modifies: true,
            enabled: has_bodies,
            run: fillet,
        },
        CommandSpec {
            id: "chamfer",
            label: "Chamfer",
            menu: "Tools/Chamfer/Fillet/2-Distance Chamfer",
            shortcut: None,
            help: "Bevel the selected edges by a distance, or by two (2-Distance Chamfer)",
            params: r#"{"distance": mm, "distance2": mm? (the other face's; default the same), "face": n? (the face distance runs along), "body": id?, "edges": [n]?}  (default: the selected edges)"#,
            modifies: true,
            enabled: has_bodies,
            run: chamfer,
        },
        CommandSpec {
            id: "fill",
            label: "Fill (Beta)",
            menu: "Tools/Fill (Beta)",
            shortcut: None,
            help: "Close an opening: a smooth face through the selected edges (a loop), joined to the body",
            params: r#"{"body": id?, "edges": [n]?}  (default: the selected edges)"#,
            modifies: true,
            enabled: has_bodies,
            run: |s, p| {
                let (body, edges) = edge_target(s, p)?;
                if edges.is_empty() {
                    return Err(EngineError::NotAvailable("select the edges around the opening".into()));
                }
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                let shape = s.kernel.fill(&b.shape, &edges)?;
                set_body_shape(s, body, shape)?;
                s.selection.clear();
                Ok(json!({ "body": body.0 }))
            },
        },
        CommandSpec {
            id: "shell",
            label: "Shell",
            menu: "Tools/Shell",
            shortcut: Some("H"),
            help: "Hollow the body, leaving the selected faces open",
            params: r#"{"thickness": mm, "body": id?, "faces": [n]?}  (default: the selected faces; walls grow inwards)"#,
            modifies: true,
            enabled: has_bodies,
            run: shell,
        },
        CommandSpec {
            id: "revolve",
            label: "Revolve",
            menu: "Tools/Revolve",
            shortcut: Some("V"),
            help: "Spin a sketch region or a flat face around an axis",
            params: r#"{"sketch": id, "region": n | "body": id, "face": n, "axis": "u"|"v" | {"line": entity} | {"body": id, "edge": n} | {"body": id, "face": n} | {"origin": [x,y,z], "dir": [x,y,z]}, "angle": degrees (360), "result": "auto"|"new"|"union"|"subtract"|"intersect"}"#,
            modifies: true,
            enabled: |_| true,
            run: revolve,
        },
        CommandSpec {
            id: "align",
            label: "Align",
            menu: "Transform/Align",
            shortcut: None,
            help: "Turn and move bodies so one of their faces meets another face or plane, center to center",
            params: r#"{"from": {"body": id, "face": n} | {"body": id, "edge": n} | {"point": [x,y,z]}, "to": {"body": id, "face": n} | {"plane": id} | {"body": id, "edge": n} | {"point": [x,y,z]}, "items": [ids]? (default: the "from" body), "flip": bool?}"#,
            modifies: true,
            enabled: has_bodies,
            run: align,
        },
        CommandSpec {
            id: "loft",
            label: "Loft",
            menu: "Tools/Loft",
            shortcut: None,
            help: "Blend a solid through the selected profiles, in order",
            params: r#"{"profiles": [{"sketch": id, "region": n} | {"body": id, "face": n}, …], "ruled": false (straight walls between profiles), "result": "new"|"union"|"subtract"|"intersect"}  (default: the selected regions and faces, in selection order)"#,
            modifies: true,
            enabled: |_| true,
            run: loft,
        },
        CommandSpec {
            id: "sweep",
            label: "Sweep",
            menu: "Tools/Sweep",
            shortcut: Some("W"),
            help: "Sweep the selected profile along the selected sketch curves",
            params: r#"{"profile": {"sketch": id, "region": n} | {"body": id, "face": n}, "path": {"sketch": id, "entities": [n]?}, "result": "new"|"union"|"subtract"|"intersect", "twist": degrees (0), "scale": factor at the end (1), "orientation": "normal"|"parallel", "corners": "mitre"|"round"}  (default: the selected region or face, along the selected sketch curves)"#,
            modifies: true,
            enabled: |_| true,
            run: sweep,
        },
        CommandSpec {
            id: "rotate",
            label: "Rotate Around Axis",
            menu: "Transform/Rotate Around Axis",
            shortcut: None,
            help: "Turn the selected bodies around an axis",
            params: r#"{"origin": [x,y,z], "axis": [x,y,z] | "about": {"body": id, "edge": n} | {"body": id, "face": n}, "angle": degrees, "items": [ids]?, "copy": false, "linked": true? (false with copy: an Unlinked Copy step that keeps the copies' geometry)}"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: rotate,
        },
        CommandSpec {
            id: "mirror",
            label: "Mirror",
            menu: "Transform/Mirror",
            shortcut: None,
            help: "Mirror the selected bodies across a plane (as copies by default)",
            params: r#"{"origin": [x,y,z], "normal": [x,y,z], "items": [ids]?, "copy": true}"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: mirror,
        },
        CommandSpec {
            id: "pattern",
            label: "Pattern - Linear",
            menu: "Transform/Pattern/Linear",
            shortcut: None,
            help: "Repeat the selected bodies along a direction",
            params: r#"{"direction": [x,y,z], "spacing": mm, "count": n | "expression" (total, including the original), "items": [ids]?, "result": "new"|"union"|"subtract"|"intersect" ("merge": true = union)}"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: pattern,
        },
        CommandSpec {
            id: "pattern_circular",
            label: "Pattern - Circular",
            menu: "Transform/Pattern/Circular",
            shortcut: None,
            help: "Repeat the selected bodies around an axis",
            params: r#"{"origin": [x,y,z], "axis": [x,y,z] | "about": {"body": id, "edge": n} | {"body": id, "face": n}, "count": n | "expression" (total), "angle": degrees (360 = full circle), "orientation": "rotated"|"uniform", "items": [ids]?, "result": "new"|"union"|"subtract"|"intersect" ("merge": true = union)}"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: pattern_circular,
        },
        CommandSpec {
            id: "scale",
            label: "Scale",
            menu: "Transform/Scale",
            shortcut: None,
            help: "Resize the selected bodies (or sketch curves) around a point",
            params: r#"{"factor": f | "factors": [x,y,z], "center": [x,y,z]? (default: the middle of the selection), "items": [ids]?, "copy": false}  (with no bodies picked: the selected sketch curves, in their plane)"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: scale,
        },
        CommandSpec {
            id: "offset_face",
            label: "Offset Face",
            menu: "Tools/Offset Face",
            shortcut: None,
            help: "Move the selected faces along their normals; neighbours stretch to follow",
            params: r#"{"distance": mm, "distance_type": "offset"|"radius"? ("radius": round faces get that radius), "body": id?, "faces": [n]?}  (default: the selected faces; negative moves inwards)"#,
            modifies: true,
            enabled: has_bodies,
            run: offset_face,
        },
        CommandSpec {
            id: "move_edge",
            label: "Move Edge",
            menu: "",
            shortcut: None,
            help: "Move a straight edge between flat faces (Move/Rotate with an edge picked); the faces beside it tilt to follow",
            params: r#"{"by": [dx,dy,dz], "body": id?, "edge": n?}  (default: the selected edge)"#,
            modifies: true,
            enabled: has_bodies,
            run: move_edge,
        },
        CommandSpec {
            id: "move_faces",
            label: "Move Faces",
            menu: "",
            shortcut: None,
            help: "Move or turn the selected faces of a body (Move/Rotate with faces picked); the rest of the body follows",
            params: r#"{"by": [dx,dy,dz]? | "origin": [x,y,z], "axis": [x,y,z], "angle": degrees, "body": id?, "faces": [n]?}  (default: the selected faces)"#,
            modifies: true,
            enabled: has_bodies,
            run: move_faces,
        },
        CommandSpec {
            id: "wrap_emboss",
            label: "Wrap & Emboss",
            menu: "Tools/Wrap & Emboss",
            shortcut: None,
            help: "Lay sketch profiles onto a cylindrical or conical face, keeping their size, and raise or engrave them",
            params: r#"{"depth": mm (negative engraves), "sketch": id?, "region": n? | "regions": [n]?, "body": id?, "face": n? (default: the round face nearest the profiles), "rotation": degrees?, "center": [x,y,z]? (the point on the face the profiles' centre goes to)}  (default: the selected regions and face)"#,
            modifies: true,
            enabled: has_bodies,
            run: wrap_emboss,
        },
        CommandSpec {
            id: "replace_face",
            label: "Replace Face",
            menu: "Tools/Replace Face",
            shortcut: None,
            help: "Move faces onto another face or plane; the body grows or shrinks to meet it",
            params: r#"{"to": {"body": id, "face": n} | {"plane": id}, "body": id?, "faces": [n]?, "flip": bool?}  (default: the selected faces)"#,
            modifies: true,
            enabled: has_bodies,
            run: replace_face,
        },
        CommandSpec {
            id: "project_edge",
            label: "Project as Edge",
            menu: "Tools/Project as Edge",
            shortcut: None,
            help: "Project edges, faces or sketch regions onto a face; their outlines become edges on it",
            params: r#"{"to": {"body": id, "face": n}, "items": [{"body": id, "edge": n} | {"body": id, "face": n} | {"sketch": id, "region": n}]?}  (default: the selection)"#,
            modifies: true,
            enabled: has_bodies,
            run: project_edge,
        },
        CommandSpec {
            id: "split_body",
            label: "Split Body",
            menu: "Tools/Split Body",
            shortcut: None,
            help: "Cut the selected bodies with planes, flat faces or sketch lines",
            params: r#"{"with": [{"origin","normal"} | {"plane"} | {"body","face"} | {"sketch","entity"}] | "origin": [x,y,z], "normal": [x,y,z], "items": [ids]?, "keep": bool?}"#,
            modifies: true,
            enabled: has_bodies,
            run: split_body,
        },
        CommandSpec {
            id: "translate",
            label: "Translate",
            menu: "Transform/Translate",
            shortcut: Some("N"),
            help: "Move the selected bodies by a distance",
            params: r#"{"by": [dx,dy,dz], "items": [ids]?, "copy": false, "copies": n? (with copy: that many, each a step further; may be an expression), "linked": true? (false with copy: an Unlinked Copy step that keeps the copies' geometry)}"#,
            modifies: true,
            enabled: has_bodies_or_curves,
            run: translate,
        },
    ]
}

/// Replaces a body's shape and re-meshes it for display.
pub fn set_body_shape(s: &mut Session, id: ItemId, shape: Shape) -> Result<()> {
    let mesh = Arc::new(s.kernel.tessellate(&shape, shapecraft_doc::project::deflection())?);
    let body = s.doc.body_mut(id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
    body.shape = shape;
    body.mesh = mesh;
    Ok(())
}

/// A new body copied from `of`: named "<of> (1)" and with its looks.
fn add_copy(s: &mut Session, of: ItemId, shape: Shape) -> Result<ItemId> {
    let id = add_body(s, shape)?;
    let (name, appearance, colors) = match s.doc.body(of) {
        Some(b) => (s.doc.copy_name(&b.name), b.appearance.clone(), b.face_colors.clone()),
        None => return Ok(id),
    };
    if let Some(b) = s.doc.bodies.iter_mut().find(|b| b.id == id) {
        b.name = name;
        b.appearance = appearance;
        b.face_colors = colors;
    }
    Ok(id)
}

fn add_body(s: &mut Session, shape: Shape) -> Result<ItemId> {
    let mesh = Arc::new(s.kernel.tessellate(&shape, shapecraft_doc::project::deflection())?);
    let id = s.doc.alloc_id();
    let name = s.doc.next_body_name();
    s.doc.bodies.push(Body { id, name, shape, mesh, visible: true, appearance: None, face_colors: Vec::new() });
    Ok(id)
}

fn selected_bodies(s: &Session) -> Vec<ItemId> {
    let mut out = Vec::new();
    for b in s.selection.iter().filter_map(Pick::body) {
        if !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

/// Bodies from params or the selection, failing with `missing` when there are none.
fn target_bodies(s: &Session, p: &Params, missing: &str) -> Result<Vec<ItemId>> {
    let ids = if p.has("items") { p.ids("items")? } else { selected_bodies(s) };
    if ids.is_empty() {
        return Err(EngineError::NotAvailable(missing.into()));
    }
    Ok(ids)
}

/// A flat face's center and outward normal, or a construction plane's.
fn face_frame(s: &Session, v: &Value) -> Result<(DVec3, DVec3)> {
    let p = Params::new(v);
    if p.has("plane") {
        let id = p.id("plane")?;
        let pl = s.doc.plane(id).ok_or_else(|| EngineError::BadParams(format!("no plane {}", id.0)))?;
        return Ok((pl.plane.origin, pl.plane.normal()));
    }
    let body = p.id("body")?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let fp = s
        .kernel
        .face_plane(&b.shape, p.f64("face")? as u32)?
        .ok_or_else(|| EngineError::NotAvailable("align works with flat faces".into()))?;
    Ok((fp.origin, fp.normal))
}

//! Union, Subtract and Intersect, and Split Body.

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_kernel::BoolOp;

use super::{add_mesh_body, add_result, face_frame, facets_of, put_result, selected_bodies, set_body_facets, target_bodies, working_shape};
use crate::cmd::CommandSpec;
use crate::meshops;
use crate::{EngineError, Params, Result, Session};
use shapecraft_doc::{Facets, ItemId};

pub(super) fn boolean_spec(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    shortcut: &'static str,
    help: &'static str,
    op: BoolOp,
) -> CommandSpec {
    let run: crate::cmd::RunFn = match op {
        BoolOp::Union => |s, p| boolean(s, p, BoolOp::Union),
        BoolOp::Subtract => |s, p| boolean(s, p, BoolOp::Subtract),
        BoolOp::Intersect => |s, p| boolean(s, p, BoolOp::Intersect),
    };
    CommandSpec {
        id,
        label,
        menu,
        shortcut: Some(shortcut),
        help,
        params: r#"{"target": id, "tools": [ids], "keep": "none"|"target"|"tools"|"all"?}  (default: first selected body is the target)"#,
        modifies: true,
        enabled: |s| s.doc.bodies.len() >= 2,
        run,
    }
}

fn boolean(s: &mut Session, p: &Params, op: BoolOp) -> Result<Value> {
    let (target, tools) = if p.has("target") {
        (p.id("target")?, p.ids("tools")?)
    } else {
        let sel = selected_bodies(s);
        match sel.split_first() {
            Some((t, rest)) if !rest.is_empty() => (*t, rest.to_vec()),
            _ => return Err(EngineError::NotAvailable("select at least two bodies".into())),
        }
    };
    // With a mesh body in it, the boolean runs on triangles and the result stays a mesh body, whichever boolean it
    // is; an exact body taking part is tessellated.
    let mesh = std::iter::once(&target).chain(&tools).any(|id| s.doc.body(*id).is_some_and(|b| b.is_mesh()));
    if mesh {
        return mesh_boolean(s, p, op, target, &tools);
    }
    let mut shape = working_shape(s, target, true)?;
    for t in &tools {
        if *t == target {
            continue;
        }
        let tool = working_shape(s, *t, true)?;
        shape = s.kernel.boolean(op, &shape, &tool)?;
    }
    // Keep Originals (observed 380, 383): "target" and/or "tools" stay as they were and the result is a new body.
    let keep = p.opt_str("keep").unwrap_or(if p.bool_or("keep", false) { "all" } else { "none" }).to_string();
    let (keep_target, keep_tools) = match keep.as_str() {
        "target" => (true, false),
        "tools" => (false, true),
        "all" | "true" => (true, true),
        _ => (false, false),
    };
    // Pieces that came apart become bodies of their own, named after the target (observed 385).
    let mut pieces = s.kernel.solids(&shape)?;
    if pieces.is_empty() {
        pieces.push(shape);
    }
    let target_name = s.doc.body(target).map(|b| b.name.clone()).unwrap_or_default();
    let mut made = Vec::new();
    let mut pieces = pieces.into_iter();
    if !keep_target && let Some(first) = pieces.next() {
        put_result(s, target, first, mesh)?;
        made.push(target);
    }
    for piece in pieces {
        let id = add_result(s, piece, mesh)?;
        if let Some(b) = s.doc.bodies.iter_mut().find(|b| b.id == id) {
            b.name = target_name.clone();
        }
        made.push(id);
    }
    if !keep_tools {
        for t in tools.iter().filter(|t| **t != target) {
            s.doc.remove_item(*t);
        }
    }
    // Nothing stays selected (observed 382, 385).
    s.selection.clear();
    Ok(json!({ "body": made.first().map_or(target.0, |b| b.0), "bodies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

/// A boolean among mesh bodies (and exact bodies with them), on triangles. Pieces that came apart become mesh bodies
/// named after the target; tools go unless kept, as with exact bodies.
fn mesh_boolean(s: &mut Session, p: &Params, op: BoolOp, target: ItemId, tools: &[ItemId]) -> Result<Value> {
    let facets = |s: &Session, id: ItemId| -> Result<Facets> {
        let b = s.doc.body(id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        match &b.facets {
            Some(f) => Ok((**f).clone()),
            None => facets_of(s, &b.shape),
        }
    };
    let target_facets = facets(s, target)?;
    let tool_facets = tools.iter().filter(|t| **t != target).map(|t| facets(s, *t)).collect::<Result<Vec<_>>>()?;
    let mop = match op {
        BoolOp::Union => meshops::Op::Union,
        BoolOp::Subtract => meshops::Op::Subtract,
        BoolOp::Intersect => meshops::Op::Intersect,
    };
    let pieces = meshops::boolean(mop, &target_facets, &tool_facets)?;
    if pieces.is_empty() {
        return Err(EngineError::NotAvailable("nothing of the body would be left".into()));
    }
    let keep = p.opt_str("keep").unwrap_or(if p.bool_or("keep", false) { "all" } else { "none" }).to_string();
    let (keep_target, keep_tools) = match keep.as_str() {
        "target" => (true, false),
        "tools" => (false, true),
        "all" | "true" => (true, true),
        _ => (false, false),
    };
    let name = s.doc.body(target).map(|b| b.name.clone()).unwrap_or_default();
    let mut made = Vec::new();
    let mut pieces = pieces.into_iter();
    if !keep_target && let Some(first) = pieces.next() {
        let empty = s.kernel.empty()?;
        if let Some(b) = s.doc.body_mut(target) {
            b.shape = empty;
        }
        set_body_facets(s, target, first)?;
        made.push(target);
    }
    for piece in pieces {
        let id = add_mesh_body(s, piece, name.clone())?;
        made.push(id);
    }
    if !keep_tools {
        for t in tools.iter().filter(|t| **t != target) {
            s.doc.remove_item(*t);
        }
    }
    s.selection.clear();
    Ok(json!({ "body": made.first().map_or(target.0, |b| b.0), "bodies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

/// Split Body (s12-002..020): the bodies are cut by each tool in "with" (an origin plane given as origin + normal,
/// a construction plane, a flat face, or a sketch line, which cuts along its sketch's normal and runs on past its
/// ends). A tool that misses every piece fails the split, as observed. The pieces all keep the body's name.
pub(super) fn split_body(s: &mut Session, p: &Params) -> Result<Value> {
    const MISSED: &str = "Split failed. The splitting profile should intersect the target body.";
    let cuts = if p.has("with") {
        let with: Vec<Value> = p.parse("with")?;
        if with.is_empty() || with.len() > 64 {
            return Err(EngineError::BadParams("with takes 1–64 splitting tools".into()));
        }
        with.iter().map(|w| split_plane(s, w)).collect::<Result<Vec<_>>>()?
    } else {
        vec![(p.vec3("origin")?, p.vec3("normal")?)]
    };
    let ids = target_bodies(s, p, "select bodies to split")?;
    // Keep Originals: the body stays whole and every piece is a new body.
    let keep = p.bool_or("keep", false);
    let mut made = Vec::new();
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let name = b.name.clone();
        // A mesh body splits on its triangles (a closed one into capped pieces) and its pieces stay mesh bodies.
        if let Some(f) = b.facets.clone() {
            let mut pieces = vec![(*f).clone()];
            for &(origin, normal) in &cuts {
                let mut next = Vec::new();
                for piece in &pieces {
                    match meshops::split(piece, origin, normal)? {
                        Some(parts) => next.extend(parts),
                        None => next.push(piece.clone()),
                    }
                }
                if next.len() <= pieces.len() {
                    return Err(EngineError::NotAvailable(MISSED.into()));
                }
                pieces = next;
            }
            let mut parts = pieces.into_iter();
            if !keep {
                let Some(first) = parts.next() else { continue };
                set_body_facets(s, *id, first)?;
            }
            for part in parts {
                made.push(add_mesh_body(s, part, name.clone())?);
            }
            continue;
        }
        let mesh = false;
        let mut pieces = vec![working_shape(s, *id, false)?];
        for &(origin, normal) in &cuts {
            let mut next = Vec::new();
            for piece in &pieces {
                // A piece the plane misses stays whole.
                match s.kernel.split_by_plane(piece, origin, normal) {
                    Ok(parts) if !parts.is_empty() => next.extend(parts),
                    _ => next.push(piece.clone()),
                }
            }
            if next.len() <= pieces.len() {
                return Err(EngineError::NotAvailable(MISSED.into()));
            }
            pieces = next;
        }
        let mut parts = pieces.into_iter();
        if !keep {
            let Some(first) = parts.next() else { continue };
            put_result(s, *id, first, mesh)?;
        }
        for part in parts {
            let made_id = add_result(s, part, mesh)?;
            if let Some(nb) = s.doc.bodies.iter_mut().find(|b| b.id == made_id) {
                nb.name = name.clone();
            }
            made.push(made_id);
        }
    }
    s.selection.clear();
    Ok(json!({ "bodies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

/// A splitting tool's plane: {"origin", "normal"}, {"plane"}, {"body", "face"} (flat) or {"sketch", "entity"} (a line).
fn split_plane(s: &Session, v: &Value) -> Result<(DVec3, DVec3)> {
    let p = Params::new(v);
    if p.has("sketch") {
        let id = p.id("sketch")?;
        let sk = s.doc.sketch(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
        let i = p.f64("entity")? as usize;
        return match sk.entities.get(i) {
            Some(shapecraft_doc::SketchEntity::Line { a, b }) if a.distance(*b) > 1e-9 => {
                let (wa, wb) = (sk.plane.to_world(*a), sk.plane.to_world(*b));
                Ok((wa, (wb - wa).cross(sk.plane.normal()).normalize()))
            }
            Some(_) => Err(EngineError::NotAvailable("Split Body cuts along sketch lines".into())),
            None => Err(EngineError::BadParams(format!("no sketch curve {i}"))),
        };
    }
    if p.has("body") {
        let body = p.id("body")?;
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        let fp = s
            .kernel
            .face_plane(&b.shape, p.f64("face")? as u32)?
            .ok_or_else(|| EngineError::NotAvailable("Split Body cuts along flat faces".into()))?;
        return Ok((fp.origin, fp.normal));
    }
    if p.has("plane") {
        return face_frame(s, v);
    }
    Ok((p.vec3("origin")?, p.vec3("normal")?))
}

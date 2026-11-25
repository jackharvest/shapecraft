//! Extrude: its targets and options, the solid it adds or cuts, and where a feature's result goes (shared with
//! revolve, loft and sweep).

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};
use shapecraft_kernel::{BoolOp, Shape};

use super::{add_body, set_body_shape};
use crate::{EngineError, Params, Result, Session};

/// The face of `body` whose plane passes through `point` with normal `normal`: the cap after an extrude.
fn find_cap(s: &Session, body: ItemId, point: DVec3, normal: DVec3) -> Option<u32> {
    let b = s.doc.body(body)?;
    let n = s.kernel.face_count(&b.shape).ok()?;
    (0..n).find(|&f| {
        s.kernel
            .face_plane(&b.shape, f)
            .ok()
            .flatten()
            .is_some_and(|fp| fp.normal.dot(normal) > 0.999 && (fp.origin - point).dot(normal).abs() < 1e-6)
    })
}

enum ExtrudeTarget {
    /// One or more regions of one sketch, extruded together as one solid (no wall where they meet).
    Regions {
        sketch: ItemId,
        regions: Vec<u32>,
    },
    Face {
        body: ItemId,
        face: u32,
    },
}

fn extrude_target(s: &Session, p: &Params) -> Result<ExtrudeTarget> {
    if p.has("sketch") {
        let regions: Vec<u32> = if p.has("regions") { p.parse("regions")? } else { vec![p.f64_or("region", 0.0)? as u32] };
        if regions.is_empty() || regions.len() > 10_000 {
            return Err(EngineError::BadParams("give 1–10000 regions".into()));
        }
        return Ok(ExtrudeTarget::Regions { sketch: p.id("sketch")?, regions });
    }
    if p.has("body") {
        return Ok(ExtrudeTarget::Face { body: p.id("body")?, face: p.f64("face")? as u32 });
    }
    for pick in &s.selection {
        match *pick {
            // Every selected region of that sketch goes up together, as the reference does.
            Pick::Region { sketch, .. } => {
                let regions = s
                    .selection
                    .iter()
                    .filter_map(|p| match *p {
                        Pick::Region { sketch: k, region } if k == sketch => Some(region),
                        _ => None,
                    })
                    .collect();
                return Ok(ExtrudeTarget::Regions { sketch, regions });
            }
            Pick::Face { body, face } => return Ok(ExtrudeTarget::Face { body, face }),
            _ => {}
        }
    }
    Err(EngineError::NotAvailable("select a sketch region or a flat face to extrude".into()))
}

/// The Extrude card's choices (observed s16-005..011): Sides, Extent, Start offset and Result.
struct ExtrudeOptions {
    symmetric: bool,
    through_all: bool,
    /// Extent ▸ To Object: up to the face, plane or body in "to".
    to_object: bool,
    start_offset: f64,
    /// "auto" (join the body the profile is on, cutting when pushed in), "new", "union", "subtract" or "intersect".
    result: String,
}

fn extrude_options(p: &Params) -> Result<ExtrudeOptions> {
    let symmetric = match p.opt_str("sides").unwrap_or("one") {
        "one" | "one_sided" => false,
        "symmetric" => true,
        other => return Err(EngineError::BadParams(format!("sides is \"one\" or \"symmetric\", not {other:?}"))),
    };
    let (through_all, to_object) = match p.opt_str("extent").unwrap_or("distance") {
        "distance" => (false, false),
        "through_all" => (true, false),
        "to_object" => (false, true),
        other => return Err(EngineError::BadParams(format!("extent is \"distance\", \"to_object\" or \"through_all\", not {other:?}"))),
    };
    let start_offset = p.f64_or("start_offset", 0.0)?;
    if !start_offset.is_finite() || start_offset.abs() > 1e6 {
        return Err(EngineError::BadParams("start_offset must be a finite length".into()));
    }
    let result = p.opt_str("result").unwrap_or("auto");
    if !matches!(result, "auto" | "new" | "union" | "subtract" | "intersect") {
        return Err(EngineError::BadParams(format!("result is auto, new, union, subtract or intersect, not {result:?}")));
    }
    Ok(ExtrudeOptions { symmetric, through_all, to_object, start_offset, result: result.to_string() })
}

/// A flat face or construction plane named by `{"body", "face"}` or `{"plane"}`: a point on it and its normal.
fn flat_target(s: &Session, v: &Value) -> Result<(DVec3, DVec3)> {
    let t = Params::new(v);
    if t.has("plane") {
        let id = t.id("plane")?;
        let pl = s.doc.plane(id).ok_or_else(|| EngineError::BadParams(format!("no plane {}", id.0)))?;
        return Ok((pl.plane.origin, pl.plane.normal()));
    }
    let body = t.id("body")?;
    let face = t.f64("face")? as u32;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let fp = s.kernel.face_plane(&b.shape, face)?.ok_or_else(|| EngineError::NotAvailable("pick a flat face".into()))?;
    Ok((fp.origin, fp.normal))
}

/// What Extent ▸ To Object stops at.
enum Until {
    /// A face or plane parallel to the profile, this far along its normal.
    Level(f64),
    /// A tilted plane: a point on it and its normal.
    Plane(DVec3, DVec3),
    /// A round, free-form or tilted face, carried on past its edges.
    Face(Shape, u32),
    /// A whole body.
    Body(Shape),
}

fn until_target(s: &Session, to: &Value, start: DVec3, normal: DVec3) -> Result<Until> {
    let t = Params::new(to);
    if t.has("body") {
        let id = t.id("body")?;
        let b = s.doc.body(id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        if b.is_mesh() {
            return Err(EngineError::NotAvailable("a mesh body can't be extruded to".into()));
        }
        if !t.has("face") {
            return Ok(Until::Body(b.shape.clone()));
        }
        let face = t.f64("face")? as u32;
        if s.kernel.face_plane(&b.shape, face)?.is_none() {
            return Ok(Until::Face(b.shape.clone(), face));
        }
    }
    let (o, n) = flat_target(s, to)?;
    if n.cross(normal).length() > 1e-6 * n.length().max(1e-12) {
        return if n.normalize_or_zero().dot(normal).abs() < 1e-3 {
            Err(EngineError::NotAvailable("that face runs along the extrusion, so it never meets it".into()))
        } else {
            Ok(Until::Plane(o, n))
        };
    }
    let d = (o - start).dot(normal);
    if d.abs() < 1e-9 {
        return Err(EngineError::NotAvailable("that face is level with the profile".into()));
    }
    Ok(Until::Level(d))
}

/// How far to pull before cutting back to the target: through all of it, on the side it's on (the way `distance` goes
/// when it's on both).
fn until_reach(s: &Session, until: &Until, faces: &[Shape], start: DVec3, normal: DVec3, distance: f64) -> Result<f64> {
    use shapecraft_kernel::SubShape;
    let mut reach: f64 = 1.0;
    let mut side = distance.signum();
    let mut through = |shape: &Shape| -> Result<()> {
        let bb = s.kernel.bounds(shape)?;
        reach = reach.max(bb.min.distance(start) + bb.min.distance(bb.max));
        Ok(())
    };
    for f in faces {
        through(f)?;
    }
    match until {
        Until::Plane(o, n) => {
            let Some(f) = faces.first() else { return Ok(reach * side) };
            let (_, c) = s.kernel.measure(f, SubShape::Face(0))?;
            let mut far: f64 = 0.0;
            for f in faces {
                let bb = s.kernel.bounds(f)?;
                for i in 0..8 {
                    let pick = |bit: u32, lo: f64, hi: f64| if i >> bit & 1 == 0 { lo } else { hi };
                    let q = DVec3::new(pick(0, bb.min.x, bb.max.x), pick(1, bb.min.y, bb.max.y), pick(2, bb.min.z, bb.max.z));
                    far = far.max(((o - q).dot(*n) / normal.dot(*n)).abs());
                }
            }
            reach += far * 1.01;
            let d = (o - c).dot(*n) / normal.dot(*n);
            if d.abs() > 1e-9 {
                side = d.signum();
            }
        }
        Until::Face(shape, face) => {
            through(shape)?;
            let Some(f) = faces.first() else { return Ok(reach * side) };
            // On which side the face is, seen from the profile's middle.
            let (_, c) = s.kernel.measure(f, SubShape::Face(0))?;
            let (_, m) = s.kernel.measure(shape, SubShape::Face(*face))?;
            let near = s.kernel.distance(f, SubShape::Whole, shape, SubShape::Face(*face))?;
            let along = if near.distance > 1e-6 { (near.b - near.a).dot(normal) } else { (m - c).dot(normal) };
            if along.abs() > 1e-6 {
                side = along.signum();
            }
        }
        Until::Body(shape) => {
            through(shape)?;
            let Some(f) = faces.first() else { return Ok(reach * side) };
            let near = s.kernel.distance(f, SubShape::Whole, shape, SubShape::Whole)?;
            let along = (near.b - near.a).dot(normal);
            if along.abs() > 1e-6 {
                side = along.signum();
            }
        }
        Until::Level(d) => return Ok(*d),
    }
    Ok((reach * 1.1 + 1.0) * side)
}

/// The part of a long pulled block between its profile and the target.
fn cut_back(s: &Session, until: &Until, faces: &[Shape], dir: DVec3, block: Shape) -> Result<Shape> {
    use shapecraft_kernel::SubShape;
    const SHORT: &str = "the profile doesn't all meet that when pulled straight";
    let pieces = match until {
        Until::Level(_) => return Ok(block),
        Until::Plane(o, n) => s.kernel.split_by_plane(&block, *o, *n).map_err(|_| EngineError::NotAvailable(SHORT.into()))?,
        Until::Face(shape, face) => s.kernel.split_by_face(&block, shape, *face)?,
        Until::Body(shape) => {
            let left = s.kernel.boolean(BoolOp::Subtract, &block, shape)?;
            s.kernel.solids(&left)?
        }
    };
    if pieces.is_empty() || (pieces.len() < 2 && !matches!(until, Until::Body(_))) {
        return Err(EngineError::NotAvailable(SHORT.into()));
    }
    // A thin skin on the profile tells the pieces that stand on it; the far end must not be one of them.
    let skin = extrude_block(s, faces, dir.normalize_or_zero() * 0.01, 0.0, false)?;
    let ends = faces.iter().map(|f| s.kernel.translate(f, dir)).collect::<std::result::Result<Vec<_>, _>>()?;
    let mut kept: Option<Shape> = None;
    for piece in pieces {
        let shared = s.kernel.boolean(BoolOp::Intersect, &piece, &skin).and_then(|c| s.kernel.mass_props(&c)).map_or(0.0, |m| m.volume);
        if !(shared.is_finite() && shared > 1e-9) {
            continue;
        }
        for end in &ends {
            if s.kernel.distance(&piece, SubShape::Whole, end, SubShape::Whole)?.distance < 1e-6 {
                return Err(EngineError::NotAvailable(SHORT.into()));
            }
        }
        kept = Some(match kept {
            Some(k) => s.kernel.boolean(BoolOp::Union, &k, &piece)?,
            None => piece,
        });
    }
    kept.ok_or_else(|| EngineError::NotAvailable(SHORT.into()))
}

/// How far past `origin` along `normal` (both ways) every body reaches: what Through All pulls to.
fn reach_through_all(s: &Session, origin: DVec3, normal: DVec3) -> Result<f64> {
    let mut reach: f64 = 0.0;
    for b in &s.doc.bodies {
        let bb = crate::cmd::model::body_box(s, b)?;
        for i in 0..8 {
            let c = DVec3::new(
                if i & 1 == 0 { bb.min.x } else { bb.max.x },
                if i & 2 == 0 { bb.min.y } else { bb.max.y },
                if i & 4 == 0 { bb.min.z } else { bb.max.z },
            );
            reach = reach.max((c - origin).dot(normal).abs());
        }
    }
    if reach < 1e-9 {
        return Err(EngineError::NotAvailable("there is nothing to extrude through".into()));
    }
    Ok(reach + 1.0)
}

/// The solid an extrude adds or cuts: each face pulled along `dir` (and back along it too when symmetric).
pub(crate) fn extrude_block(s: &Session, faces: &[Shape], dir: DVec3, draft: f64, symmetric: bool) -> Result<Shape> {
    let mut block: Option<Shape> = None;
    for face in &s.kernel.merge_faces(faces)? {
        let sides: &[f64] = if symmetric { &[1.0, -1.0] } else { &[1.0] };
        for side in sides {
            let d = dir * *side;
            let part = if draft == 0.0 { s.kernel.prism(face, d)? } else { s.kernel.prism_draft(face, d, draft)? };
            block = Some(match block {
                Some(acc) => s.kernel.boolean(BoolOp::Union, &acc, &part)?,
                None => part,
            });
        }
    }
    block.ok_or_else(|| EngineError::BadParams("no profile to extrude".into()))
}

/// A feature's Result choice (the badges of the reference's feature cards): `None` = auto.
pub(super) fn result_param(p: &Params) -> Result<Option<Option<BoolOp>>> {
    Ok(match p.opt_str("result").unwrap_or("auto") {
        "auto" => None,
        "new" => Some(None),
        "union" => Some(Some(BoolOp::Union)),
        "subtract" => Some(Some(BoolOp::Subtract)),
        "intersect" => Some(Some(BoolOp::Intersect)),
        other => return Err(EngineError::BadParams(format!("result is auto, new, union, subtract or intersect, not {other:?}"))),
    })
}

/// The body a profile from a free plane rests on, and which side of the plane it is on (1 along the normal, -1
/// against it): a sketch on the ground drawn over a block's top face. The reference treats that profile like one
/// sketched on the face (pushed in it cuts, pulled out it joins) only when all of it lies on the face: a ground
/// rectangle half under a block, pulled up into it, is a new body there (both observed in 26.202, s36). A body on
/// both sides (the plane runs through it) or on neither doesn't count.
pub(crate) fn resting_body(s: &Session, faces: &[Shape], normal: DVec3) -> Result<Option<(ItemId, f64)>> {
    const SKIN: f64 = 0.01;
    let mut slabs = Vec::new();
    for side in [1.0, -1.0] {
        let slab = extrude_block(s, faces, normal * (SKIN * side), 0.0, false)?;
        let volume = s.kernel.mass_props(&slab)?.volume;
        if !(volume.is_finite() && volume > 0.0) {
            return Ok(None);
        }
        slabs.push((side, slab, volume));
    }
    let mut best: Option<(ItemId, f64, f64)> = None;
    for b in &s.doc.bodies {
        let mut share = [0.0; 2];
        // A body without extent (an empty one from an older project) can't hold the profile (#50).
        let Ok(o) = crate::cmd::model::body_box(s, b) else { continue };
        for (i, (_, slab, volume)) in slabs.iter().enumerate() {
            let bb = s.kernel.bounds(slab)?;
            if !(o.min.cmple(bb.max).all() && bb.min.cmple(o.max).all()) {
                continue;
            }
            let common = s.kernel.boolean(BoolOp::Intersect, &b.shape, slab).and_then(|c| s.kernel.mass_props(&c));
            share[i] = common.map_or(0.0, |m| m.volume / volume);
        }
        // All of the profile on the face: the body fills the skin on one side and none of the other.
        let (side, inside) = match (share[0] > 0.999, share[1] > 0.999) {
            (true, false) if share[1] < 1e-3 => (1.0, share[0]),
            (false, true) if share[0] < 1e-3 => (-1.0, share[1]),
            _ => continue,
        };
        if best.is_none_or(|(_, _, most)| inside > most) {
            best = Some((b.id, side, inside));
        }
    }
    Ok(best.map(|(id, side, _)| (id, side)))
}

/// The body an extrude of these regions works on when its Result is left on auto, and which side of the sketch
/// plane that body is on: the body the sketch was drawn on (behind its plane), or else the one a free-plane
/// profile rests on ([`resting_body`]). The UI asks this to show the same Result the engine will use.
pub fn extrude_auto_body(s: &Session, sketch: ItemId, regions: &[u32]) -> Option<(ItemId, f64)> {
    let sk = s.doc.sketch(sketch)?;
    if let Some(b) = sk.on_body.filter(|b| s.doc.body(*b).is_some()) {
        return Some((b, -1.0));
    }
    let faces: Vec<Shape> = regions.iter().filter_map(|r| sk.region(*r).map(|g| g.face.clone())).collect();
    if faces.is_empty() {
        return None;
    }
    resting_body(s, &faces, sk.plane.normal()).ok().flatten()
}

/// Puts a feature's solid in the model: a new body, or combined with the body it was made on (`auto` there) or
/// the bodies it overlaps (see [`boolean_targets`]). Returns the first body changed.
pub(super) fn place_result(
    s: &mut Session,
    shape: Shape,
    result: Option<Option<BoolOp>>,
    on_body: Option<ItemId>,
    auto: BoolOp,
) -> Result<ItemId> {
    let op = match result {
        None => on_body.map(|_| auto),
        Some(op) => op,
    };
    let Some(op) = op else { return add_body(s, shape) };
    let targets = match on_body {
        Some(b) => vec![b],
        None => boolean_targets(s, &shape, op)?,
    };
    combine(s, op, &targets, &shape)
}

/// Applies `op` with `tool` to each target body; returns the first.
fn combine(s: &mut Session, op: BoolOp, targets: &[ItemId], tool: &Shape) -> Result<ItemId> {
    for target in targets {
        let b = s.doc.body(*target).ok_or_else(|| EngineError::BadParams("body vanished".into()))?;
        let merged = s.kernel.boolean(op, &b.shape, tool)?;
        set_body_shape(s, *target, merged)?;
    }
    targets.first().copied().ok_or_else(|| EngineError::NotAvailable("the new solid doesn't touch a body".into()))
}

/// The bodies an explicit Union / Subtract / Intersect works on when the profile isn't on a body. Subtract cuts
/// every body the new solid really runs into; Union and Intersect take the one it overlaps most. Bounding boxes
/// only narrow the search: a shell's box holds everything inside it, and taking the first box hit cut the oldest
/// body instead of the panel the profile sat on. With no real overlap (a solid that only touches a face), the
/// first body whose box it touches.
pub(super) fn boolean_targets(s: &Session, block: &Shape, op: BoolOp) -> Result<Vec<ItemId>> {
    let bb = s.kernel.bounds(block)?;
    let mut touching = Vec::new();
    // Mesh bodies aren't merged into or cut by a feature: they take part only through the boolean tools.
    for b in s.doc.bodies.iter().filter(|b| !b.is_mesh()) {
        // Nor can an empty body be a target (#50).
        let Ok(o) = crate::cmd::model::body_box(s, b) else { continue };
        if o.min.cmple(bb.max).all() && bb.min.cmple(o.max).all() {
            touching.push(b.id);
        }
    }
    let mut overlap: Vec<(ItemId, f64)> = Vec::new();
    if touching.len() > 1 || op == BoolOp::Subtract {
        for id in &touching {
            let Some(b) = s.doc.body(*id) else { continue };
            let volume = s
                .kernel
                .boolean(BoolOp::Intersect, &b.shape, block)
                .and_then(|common| s.kernel.mass_props(&common))
                .map_or(0.0, |m| m.volume);
            if volume.is_finite() && volume > 1e-6 {
                overlap.push((*id, volume));
            }
        }
    }
    let ids = if overlap.is_empty() {
        touching.into_iter().take(1).collect()
    } else if op == BoolOp::Subtract {
        overlap.into_iter().map(|(id, _)| id).collect()
    } else {
        overlap.sort_by(|a, b| b.1.total_cmp(&a.1));
        overlap.into_iter().take(1).map(|(id, _)| id).collect()
    };
    Ok(ids)
}

/// The bodies a command's "from" names, to be joined, cut or intersected: at least one, each an exact body.
fn explicit_targets(s: &Session, p: &Params) -> Result<Vec<ItemId>> {
    let list = p.raw("from").and_then(Value::as_array).ok_or_else(|| EngineError::BadParams("from is a list of body ids".into()))?;
    let mut out: Vec<ItemId> = Vec::new();
    for v in list.iter().take(10_001) {
        let id = ItemId(v.as_u64().ok_or_else(|| EngineError::BadParams("from is a list of body ids".into()))?);
        let body = s.doc.body(id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        if body.is_mesh() {
            return Err(EngineError::NotAvailable("a mesh body can't be cut by a feature".into()));
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.is_empty() || out.len() > 10_000 {
        return Err(EngineError::BadParams("pick at least one body".into()));
    }
    Ok(out)
}

pub(super) fn extrude(s: &mut Session, p: &Params) -> Result<Value> {
    let opts = extrude_options(p)?;
    let distance = if opts.through_all || opts.to_object { p.f64_or("distance", -1.0)? } else { p.f64("distance")? };
    if !distance.is_finite() || distance.abs() < 1e-9 {
        return Err(EngineError::BadParams("extrude distance is zero".into()));
    }
    let draft = p.f64_or("draft", 0.0)?;
    if !draft.is_finite() || draft.abs() >= 85.0 {
        return Err(EngineError::BadParams("draft must be between -85 and 85 degrees".into()));
    }
    let draft = draft.to_radians();
    // The sketch regions this extrusion uses (none for a face).
    let mut used: Vec<u32> = Vec::new();
    let (faces, plane_origin, plane_normal, on_body, sketch) = match extrude_target(s, p)? {
        ExtrudeTarget::Face { body, face } => {
            let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
            let plane = s
                .kernel
                .face_plane(&b.shape, face)?
                .ok_or_else(|| EngineError::NotAvailable("only flat faces can be extruded yet".into()))?;
            let plain = draft == 0.0
                && !opts.symmetric
                && !opts.through_all
                && !opts.to_object
                && opts.start_offset == 0.0
                && opts.result == "auto"
                && !p.has("start_plane");
            if plain {
                // Push/pull of the face itself: the body keeps its faces.
                let shape = s.kernel.extrude_face(&b.shape, face, distance)?;
                set_body_shape(s, body, shape)?;
                let cap = find_cap(s, body, plane.origin + plane.normal * distance, plane.normal);
                s.selection = cap.map(|face| vec![Pick::Face { body, face }]).unwrap_or_default();
                return Ok(json!({ "body": body.0, "face": cap }));
            }
            (vec![s.kernel.face(&b.shape, face)?], plane.origin, plane.normal, Some(body), None)
        }
        ExtrudeTarget::Regions { sketch, regions } => {
            let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
            let mut faces: Vec<(u32, Shape)> = Vec::new();
            for region in &regions {
                if !faces.iter().any(|(r, _)| r == region) {
                    let face = sk.region(*region).ok_or_else(|| EngineError::BadParams(format!("no region {region}")))?.face.clone();
                    faces.push((*region, face));
                }
            }
            let plane = sk.plane;
            let on_body = sk.on_body.filter(|b| s.doc.body(*b).is_some());
            used = faces.iter().map(|(r, _)| *r).collect();
            (faces.into_iter().map(|(_, f)| f).collect(), plane.origin, plane.normal(), on_body, Some(sketch))
        }
    };
    // Start ▸ From Plane: the profile starts where a parallel face or plane is (its offset along the normal).
    let start_offset = if p.has("start_plane") {
        let (o, n) = flat_target(s, &p.parse::<Value>("start_plane")?)?;
        if n.cross(plane_normal).length() > 1e-6 {
            return Err(EngineError::NotAvailable("the start plane must be parallel to the profile".into()));
        }
        (o - plane_origin).dot(plane_normal)
    } else {
        opts.start_offset
    };
    let opts = ExtrudeOptions { start_offset, ..opts };
    // Start ▸ Offset: the profile moves along its normal before it is pulled.
    let start = plane_origin + plane_normal * opts.start_offset;
    let faces = if opts.start_offset == 0.0 {
        faces
    } else {
        faces.iter().map(|f| s.kernel.translate(f, plane_normal * opts.start_offset)).collect::<std::result::Result<Vec<_>, _>>()?
    };
    let until = if opts.to_object {
        let to = p.parse::<Value>("to").map_err(|_| EngineError::NotAvailable("pick the face or plane to extrude to".into()))?;
        until_target(s, &to, start, plane_normal)?
    } else {
        Until::Level(0.0)
    };
    let length = if opts.through_all {
        reach_through_all(s, start, plane_normal)? * distance.signum()
    } else if opts.to_object {
        match &until {
            // Up to a parallel face or plane: as far along the normal as it is (either way).
            Until::Level(d) => *d,
            // Long enough to go through; cut back below.
            Until::Plane(..) | Until::Face(..) | Until::Body(_) => until_reach(s, &until, &faces, start, plane_normal, distance)?,
        }
    } else {
        distance
    };
    let dir = plane_normal * length;
    // Regions split by a line (a line drawn across a circle) fuse back into one solid: the line stays in the
    // sketch, the union leaves no wall or seam where the regions touched.
    // A face pushed into its body cuts a block whose walls lean the other way.
    let draft = if sketch.is_none() && length < 0.0 { -draft } else { draft };
    // Auto on a free plane: a profile resting on a body's face works on that body like one sketched on it. The
    // step keeps which side that body was on (`auto_side`, worked out again on every rebuild) for its card.
    let _ = p.f64_or("auto_side", 0.0)?;
    let (on_body, body_side) = match on_body {
        Some(b) => (Some(b), -1.0),
        None if opts.result == "auto" && sketch.is_some() => match resting_body(s, &faces, plane_normal)? {
            Some((b, side)) => {
                let mut logged = p.all().clone();
                logged["auto_side"] = json!(side);
                s.log_params = Some(logged);
                (Some(b), side)
            }
            None => (None, -1.0),
        },
        None => (None, -1.0),
    };
    let block = match until {
        Until::Level(_) => extrude_block(s, &faces, dir, draft, opts.symmetric)?,
        _ if opts.symmetric => {
            return Err(EngineError::NotAvailable("a symmetric extrude needs a flat face parallel to the profile".into()));
        }
        _ => cut_back(s, &until, &faces, dir, extrude_block(s, &faces, dir, draft, false)?)?,
    };
    let op = match opts.result.as_str() {
        "new" => None,
        "union" => Some(BoolOp::Union),
        "subtract" => Some(BoolOp::Subtract),
        "intersect" => Some(BoolOp::Intersect),
        // Toward the body's side it cuts, away from it it joins.
        _ => on_body.map(|_| if length * body_side > 0.0 { BoolOp::Subtract } else { BoolOp::Union }),
    };
    let targets = match (op, on_body) {
        (None, _) => Vec::new(),
        // The bodies named (Subtract From's list) work in place of the ones it would have found.
        (Some(_), _) if p.has("from") => explicit_targets(s, p)?,
        (Some(_), Some(b)) => vec![b],
        (Some(op), None) => {
            let t = boolean_targets(s, &block, op)?;
            if t.is_empty() {
                return Err(EngineError::NotAvailable("the extrusion doesn't touch a body".into()));
            }
            t
        }
    };
    let body = match op {
        Some(op) => combine(s, op, &targets, &block)?,
        None => add_body(s, block)?,
    };
    if let Some(sketch) = sketch {
        // A sketch hides once all of it has been used; one with something left stays shown (#53).
        let every_region = s.keep_sketches;
        if let Some(sk) = s.doc.sketch_mut(sketch)
            && sk.use_regions(&used, every_region)
        {
            sk.visible = false;
        }
        if s.active_sketch == Some(sketch) {
            s.active_sketch = None;
        }
    }
    let normal = plane_normal * length.signum();
    let cap = find_cap(s, body, start + dir, normal);
    s.selection = cap.map(|face| vec![Pick::Face { body, face }]).unwrap_or_default();
    let changed: Vec<u64> = if targets.is_empty() { vec![body.0] } else { targets.iter().map(|b| b.0).collect() };
    Ok(json!({ "body": body.0, "bodies": changed, "face": cap }))
}

//! Moving and copying: move, rotate, scale, align, mirror and patterns, for bodies, construction planes and sketch
//! curves.

use glam::{DAffine2, DVec2, DVec3};
use serde_json::{Value, json};
use shapecraft_doc::{Constraint, ItemId, Pick, SketchEntity, SketchPlane};
use shapecraft_kernel::{Axis, BoolOp, SubShape};

use super::sweep::topo_axis;
use super::{add_body, add_copy, face_frame, selected_bodies, set_body_shape, target_bodies};
use crate::{EngineError, Params, Result, Session};

/// Selected bodies and construction planes: what Move/Rotate moves.
fn selected_movables(s: &Session) -> Vec<ItemId> {
    let mut out = selected_bodies(s);
    for p in &s.selection {
        if let Pick::Plane { plane } = *p
            && !out.contains(&plane)
        {
            out.push(plane);
        }
    }
    out
}

/// Moves a construction plane by `f`, or adds a moved copy (returned).
fn xform_plane(s: &mut Session, id: ItemId, copy: bool, f: impl Fn(SketchPlane) -> SketchPlane) -> Result<Option<ItemId>> {
    let (plane, size) =
        s.doc.plane(id).map(|p| (p.plane, p.size)).ok_or_else(|| EngineError::BadParams(format!("no body or plane {}", id.0)))?;
    let moved = f(plane);
    if !(moved.origin.is_finite() && moved.x.is_finite() && moved.y.is_finite()) {
        return Err(EngineError::BadParams("that move isn't finite".into()));
    }
    if copy {
        return crate::cmd::construct::add_plane(s, moved, size).map(Some);
    }
    if let Some(p) = s.doc.plane_mut(id) {
        p.plane = moved;
    }
    Ok(None)
}

/// How an item is selected as a whole.
fn item_pick(s: &Session, id: ItemId) -> Pick {
    if s.doc.plane(id).is_some() { Pick::Plane { plane: id } } else { Pick::Body { body: id } }
}

/// The selected curves of one sketch, when no bodies are selected (sketch-mode Move, Mirror, Pattern).
fn selected_entities(s: &Session, p: &Params) -> Option<(ItemId, Vec<u32>)> {
    if p.has("items") || s.selection.iter().any(|pk| pk.body().is_some()) {
        return None;
    }
    let mut sketch = None;
    let mut ents = Vec::new();
    for pk in &s.selection {
        match *pk {
            Pick::SketchEntity { sketch: sk, entity } if sketch.is_none_or(|x| x == sk) => {
                sketch = Some(sk);
                ents.push(entity);
            }
            // A whole sketch (picked in Items) stands for all its curves.
            Pick::Sketch { sketch: sk } if sketch.is_none_or(|x| x == sk) => {
                sketch = Some(sk);
                for e in 0..s.doc.sketch(sk).map_or(0, |k| k.entities.len() as u32) {
                    if !ents.contains(&e) {
                        ents.push(e);
                    }
                }
            }
            // A selected region stands for the curves around it.
            Pick::Region { sketch: sk, region } if sketch.is_none_or(|x| x == sk) => {
                sketch = Some(sk);
                for e in s.doc.sketch(sk).map(|k| k.region_entities(region)).unwrap_or_default() {
                    if !ents.contains(&e) {
                        ents.push(e);
                    }
                }
            }
            _ => {}
        }
    }
    Some((sketch?, ents)).filter(|(_, e)| !e.is_empty())
}

/// A turn about a world axis, as a map in the sketch's coordinates (the axis must be along its normal).
fn sketch_rotation(plane: &SketchPlane, axis: Axis, degrees: f64) -> Result<DAffine2> {
    let n = plane.normal();
    let dir = axis.dir.try_normalize().ok_or_else(|| EngineError::BadParams("axis must not be zero".into()))?;
    if dir.cross(n).length() > 1e-6 {
        return Err(EngineError::NotAvailable("sketch curves turn only around the sketch's normal".into()));
    }
    let a = degrees.to_radians() * dir.dot(n).signum();
    let c = plane.to_local(axis.origin);
    Ok(DAffine2::from_translation(c) * DAffine2::from_angle(a) * DAffine2::from_translation(-c))
}

/// Moves the curves (`maps` has one entry, `copy` false) or adds a moved copy per map.
/// `linked` (patterns): the copies stay tied to the source curves and follow their changes.
fn sketch_xform(s: &mut Session, (sketch, ents): (ItemId, Vec<u32>), maps: &[DAffine2], copy: bool, linked: bool) -> Result<Value> {
    let sk = s.doc.sketch_mut(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
    if ents.iter().any(|i| *i as usize >= sk.entities.len()) {
        return Err(EngineError::BadParams("no such sketch curve".into()));
    }
    if sk.entities.len() + ents.len() * maps.len() * 4 > 100_000 {
        return Err(EngineError::BadParams("sketch is full".into()));
    }
    let mut picked = Vec::new();
    if copy {
        let mut copies = Vec::new();
        for m in maps {
            for &i in &ents {
                let made = shapecraft_doc::sketch_ops::transform(&sk.entities[i as usize], *m);
                copies.push((sk.entities.len() as u32..(sk.entities.len() + made.len()) as u32).collect());
                sk.entities.extend(made);
            }
        }
        if linked {
            sk.patterns.push(shapecraft_doc::SketchPattern {
                source: ents.clone(),
                maps: maps.iter().map(|m| m.to_cols_array()).collect(),
                copies,
            });
        }
        picked.extend(ents.iter().copied());
    } else {
        // Curves moved on purpose let go of the origin they were drawn from.
        sk.constraints.retain(
            |c| !matches!(c, Constraint::Coincident { a, b: shapecraft_doc::Element::Origin { .. } } if ents.contains(&a.entity())),
        );
        let m = maps.first().copied().unwrap_or(DAffine2::IDENTITY);
        let mut list = Vec::with_capacity(sk.entities.len());
        for (i, e) in sk.entities.iter().enumerate() {
            if ents.contains(&(i as u32)) {
                for piece in shapecraft_doc::sketch_ops::transform(e, m) {
                    picked.push(list.len() as u32);
                    list.push(piece);
                }
            } else {
                list.push(e.clone());
            }
        }
        sk.entities = list;
    }
    s.selection = picked.iter().map(|&entity| Pick::SketchEntity { sketch, entity }).collect();
    Ok(json!({ "sketch": sketch.0, "entities": picked }))
}

/// Curves carried off their sketch's plane (the gizmo's normal arrow, or a turn about an axis across the sketch).
/// With every curve picked the sketch's plane goes with them; otherwise they move into a new sketch on the moved
/// plane, which opens in place of the old one when that was open.
fn lift_curves(s: &mut Session, (sketch, ents): (ItemId, Vec<u32>), copy: bool, f: impl Fn(SketchPlane) -> SketchPlane) -> Result<Value> {
    let old = s.doc.sketch(sketch).cloned().ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
    let moved = f(old.plane);
    if !(moved.origin.is_finite() && moved.x.is_finite() && moved.y.is_finite()) {
        return Err(EngineError::BadParams("that move isn't finite".into()));
    }
    let n = old.entities.len();
    if ents.iter().any(|i| *i as usize >= n) {
        return Err(EngineError::BadParams("no such sketch curve".into()));
    }
    if !copy && (0..n as u32).all(|i| ents.contains(&i)) {
        if let Some(sk) = s.doc.sketch_mut(sketch) {
            sk.plane = moved;
            sk.on_body = None;
        }
        s.selection = ents.iter().map(|&entity| Pick::SketchEntity { sketch, entity }).collect();
        return Ok(json!({ "sketch": sketch.0, "entities": ents }));
    }
    // New numbers on each side of the split.
    let (mut lifted, mut kept) = (vec![None; n], vec![None; n]);
    let (mut a, mut b) = (0u32, 0u32);
    for i in 0..n {
        if ents.contains(&(i as u32)) {
            lifted[i] = Some(a);
            a += 1;
        } else {
            kept[i] = Some(b);
            b += 1;
        }
    }
    let side = |map: &[Option<u32>]| {
        let entities: Vec<SketchEntity> = (0..n).filter(|&i| map[i].is_some()).map(|i| old.entities[i].clone()).collect();
        let at = |e: u32| map.get(e as usize).copied().flatten();
        let constraints: Vec<Constraint> = old.constraints.iter().filter_map(|c| c.remapped(&at)).collect();
        let construction = old.construction.iter().filter_map(|&e| at(e)).collect();
        (entities, constraints, construction)
    };
    let id = s.doc.alloc_id();
    let name = s.doc.next_sketch_name();
    let mut new = shapecraft_doc::Sketch::new(id, name.clone(), moved, None);
    let (entities, constraints, construction) = side(&lifted);
    new.entities = entities;
    // The origin stays behind on the old plane.
    new.constraints =
        constraints.into_iter().filter(|c| !c.elements().iter().any(|e| matches!(e, shapecraft_doc::Element::Origin { .. }))).collect();
    new.construction = construction;
    // Copy: the curves stay where they were too; only the new sketch is added.
    if copy {
        s.doc.sketches.push(new);
        if s.active_sketch == Some(sketch) {
            s.active_sketch = Some(id);
        }
        s.selection = (0..a).map(|entity| Pick::SketchEntity { sketch: id, entity }).collect();
        return Ok(json!({ "sketch": id.0, "name": name, "entities": (0..a).collect::<Vec<_>>() }));
    }
    let (entities, constraints, construction) = side(&kept);
    let at = |e: &u32| kept.get(*e as usize).copied().flatten();
    let patterns: Vec<shapecraft_doc::SketchPattern> = old
        .patterns
        .iter()
        .filter_map(|pt| {
            Some(shapecraft_doc::SketchPattern {
                source: pt.source.iter().map(at).collect::<Option<_>>()?,
                maps: pt.maps.clone(),
                copies: pt.copies.iter().map(|c| c.iter().map(at).collect::<Option<_>>()).collect::<Option<_>>()?,
            })
        })
        .collect();
    if let Some(sk) = s.doc.sketch_mut(sketch) {
        sk.entities = entities;
        sk.constraints = constraints;
        sk.construction = construction;
        sk.patterns = patterns;
    }
    s.doc.sketches.push(new);
    if s.active_sketch == Some(sketch) {
        s.active_sketch = Some(id);
    }
    s.selection = (0..a).map(|entity| Pick::SketchEntity { sketch: id, entity }).collect();
    Ok(json!({ "sketch": id.0, "name": name, "entities": (0..a).collect::<Vec<_>>() }))
}

fn sketch_plane_of(s: &Session, sketch: ItemId) -> Result<SketchPlane> {
    Ok(s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?.plane)
}

/// An Unlinked Copy replayed: the copies as they were made, whatever the bodies they came from look like now.
fn thaw_copies(s: &mut Session, p: &Params) -> Result<Option<Value>> {
    if !p.has("frozen") {
        return Ok(None);
    }
    let frozen: Vec<(u64, String)> = p.parse("frozen")?;
    // How they were made no longer matters.
    for k in p.unread() {
        p.raw(&k);
    }
    let mut made = Vec::new();
    for (of, text) in frozen {
        let shape = s.kernel.shape_from_text(&text)?;
        made.push(add_copy(s, ItemId(of), shape)?);
    }
    s.selection = made.iter().map(|&id| item_pick(s, id)).collect();
    Ok(Some(json!({ "copies": made.iter().map(|b| b.0).collect::<Vec<_>>() })))
}

/// Copies made with "linked": false: the History keeps their geometry instead of how they were made.
fn freeze_copies(s: &mut Session, p: &Params, made: &[(ItemId, ItemId)]) -> Result<()> {
    if !made.is_empty() && !p.bool_or("linked", true) {
        let mut frozen = Vec::new();
        for (of, id) in made {
            let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
            frozen.push(json!([of.0, s.kernel.shape_to_text(&b.shape)?]));
        }
        let mut logged = p.all().clone();
        if let Some(o) = logged.as_object_mut() {
            o.insert("frozen".into(), Value::Array(frozen));
        }
        s.log_params = Some(logged);
    }
    Ok(())
}

pub(super) fn translate(s: &mut Session, p: &Params) -> Result<Value> {
    if let Some(v) = thaw_copies(s, p)? {
        return Ok(v);
    }
    let by = p.vec3("by")?;
    if let Some(sel) = selected_entities(s, p) {
        let pl = sketch_plane_of(s, sel.0)?;
        // Off the sketch's plane: the curves take a plane of their own along with them.
        let copy = p.bool_or("copy", false);
        if by.dot(pl.normal()).abs() > 1e-9 {
            return lift_curves(s, sel, copy, |pl| SketchPlane { origin: pl.origin + by, ..pl });
        }
        let d = DVec2::new(by.dot(pl.x), by.dot(pl.y));
        return sketch_xform(s, sel, &[DAffine2::from_translation(d)], copy, false);
    }
    let ids = if p.has("items") { p.ids("items")? } else { selected_movables(s) };
    if ids.is_empty() {
        return Err(EngineError::NotAvailable("select bodies to move".into()));
    }
    let copy = p.bool_or("copy", false);
    // Several copies, each one step further along (Move with Copy and a count).
    let copies = if copy { count_param(s, p, "copies", 1.0)?.clamp(1.0, 500.0) as usize } else { 1 };
    let mut made = Vec::new();
    let mut bodies = Vec::new();
    for id in &ids {
        if s.doc.body(*id).is_none() {
            for k in 1..=copies {
                let step = by * k as f64;
                made.extend(xform_plane(s, *id, copy, |pl| SketchPlane { origin: pl.origin + step, ..pl })?);
            }
            continue;
        }
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let original = b.shape.clone();
        if copy {
            for k in 1..=copies {
                let moved = s.kernel.translate(&original, by * k as f64)?;
                let c = add_copy(s, *id, moved)?;
                made.push(c);
                bodies.push((*id, c));
            }
        } else {
            let moved = s.kernel.translate(&original, by)?;
            set_body_shape(s, *id, moved)?;
        }
    }
    if copy {
        s.selection = made.iter().map(|&id| item_pick(s, id)).collect();
    }
    freeze_copies(s, p, &bodies)?;
    Ok(json!({ "moved": ids.len(), "copies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

/// A count that may be typed as an expression of variables ("n_holes", "2 * rows").
fn count_param(s: &Session, p: &Params, key: &str, default: f64) -> Result<f64> {
    let v = match p.opt_str(key) {
        Some(text) => shapecraft_doc::expr::eval(text, &s.doc.variables).map_err(EngineError::BadParams)?,
        None => p.f64_or(key, default)?,
    };
    if !v.is_finite() {
        return Err(EngineError::BadParams(format!("{key} must be a number")));
    }
    Ok(v.round())
}

/// The middle of the bodies' combined bounding box.
pub fn bodies_center(s: &Session, ids: &[ItemId]) -> Result<DVec3> {
    let (mut min, mut max) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for id in ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let bb = s.kernel.bounds(&b.shape)?;
        min = min.min(bb.min);
        max = max.max(bb.max);
    }
    Ok((min + max) / 2.0)
}

pub(super) fn scale(s: &mut Session, p: &Params) -> Result<Value> {
    let factors = if p.has("factors") { p.vec3("factors")? } else { DVec3::splat(p.f64("factor")?) };
    if factors.min_element() <= 0.0 || factors.max_element() > 1e4 || factors.min_element() < 1e-4 {
        return Err(EngineError::BadParams("scale factors must be between 0.0001 and 10000".into()));
    }
    if let Some(sel) = selected_entities(s, p) {
        // Sketch curves scale in their own plane, about the center (default: the middle of the curves).
        let pl = sketch_plane_of(s, sel.0)?;
        let along = |axis: DVec3| {
            let w = axis.abs();
            (factors.x * w.x + factors.y * w.y + factors.z * w.z) / (w.x + w.y + w.z).max(1e-12)
        };
        let center = if p.has("center") {
            pl.to_local(p.vec3("center")?)
        } else {
            let sk = s.doc.sketch(sel.0).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sel.0.0)))?;
            let pts: Vec<DVec2> = sel.1.iter().filter_map(|i| sk.entities.get(*i as usize)).flat_map(|e| e.outline()).collect();
            let min = pts.iter().fold(DVec2::splat(f64::INFINITY), |m, q| m.min(*q));
            let max = pts.iter().fold(DVec2::splat(f64::NEG_INFINITY), |m, q| m.max(*q));
            if !min.is_finite() || !max.is_finite() {
                return Err(EngineError::BadParams("nothing to scale".into()));
            }
            (min + max) / 2.0
        };
        let m = DAffine2::from_translation(center)
            * DAffine2::from_scale(DVec2::new(along(pl.x), along(pl.y)))
            * DAffine2::from_translation(-center);
        return sketch_xform(s, sel, &[m], p.bool_or("copy", false), false);
    }
    // One closed edge (a rim): it scales on its own; its flat face goes with it and the face beside it follows
    // Picked, or named with "body" + "edge".
    let edge = if p.has("edge") {
        Some((p.id("body")?, p.f64("edge")?.clamp(0.0, u32::MAX as f64) as u32))
    } else if p.has("items") {
        None
    } else {
        match s.selection.as_slice() {
            [Pick::Edge { body, edge }] => Some((*body, *edge)),
            _ => None,
        }
    };
    if let Some((body, edge)) = edge {
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        let center = if p.has("center") { p.vec3("center")? } else { s.kernel.measure(&b.shape, SubShape::Edge(edge))?.1 };
        let scaled = s.kernel.scale_edge(&b.shape, edge, center, factors)?;
        set_body_shape(s, body, scaled)?;
        s.selection.clear();
        return Ok(json!({ "scaled": 1, "body": body.0, "copies": [] }));
    }
    let ids = target_bodies(s, p, "select bodies to scale")?;
    let center = if p.has("center") { p.vec3("center")? } else { bodies_center(s, &ids)? };
    let copy = p.bool_or("copy", false);
    let mut made = Vec::new();
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let scaled = s.kernel.scale(&b.shape, center, factors)?;
        if copy {
            made.push(add_copy(s, *id, scaled)?);
        } else {
            set_body_shape(s, *id, scaled)?;
        }
    }
    Ok(json!({ "scaled": ids.len(), "copies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

pub(super) fn pattern_circular(s: &mut Session, p: &Params) -> Result<Value> {
    let axis = if p.has("about") {
        let a: Value = p.parse("about")?;
        topo_axis(s, &Params::new(&a))?
    } else {
        Axis::new(p.vec3("origin")?, p.vec3("axis")?)
    };
    if axis.dir.length_squared() == 0.0 {
        return Err(EngineError::BadParams("axis must not be zero".into()));
    }
    let count = count_param(s, p, "count", 0.0)?;
    if !(2.0..=500.0).contains(&count) {
        return Err(EngineError::BadParams("count must be 2–500".into()));
    }
    // Rotated (the default) turns each copy with the circle; Uniform keeps them facing the same way.
    let uniform = match p.opt_str("orientation").unwrap_or("rotated") {
        "rotated" => false,
        "uniform" => true,
        other => return Err(EngineError::BadParams(format!("orientation is rotated or uniform, not {other:?}"))),
    };
    let angle = p.f64_or("angle", 360.0)?;
    if angle.abs() < 1e-6 || angle.abs() > 360.0 {
        return Err(EngineError::BadParams("angle must be between 0 and 360 degrees".into()));
    }
    let n = count as usize;
    // A full circle spreads the copies evenly without doubling up the first; a partial arc ends on the last copy.
    let step = if (angle.abs() - 360.0).abs() < 1e-9 { angle / n as f64 } else { angle / (n - 1) as f64 };
    if let Some(sel) = selected_entities(s, p) {
        let pl = sketch_plane_of(s, sel.0)?;
        let maps = (1..n).map(|i| sketch_rotation(&pl, axis, step * i as f64)).collect::<Result<Vec<_>>>()?;
        return sketch_xform(s, sel, &maps, true, true);
    }
    let ids = target_bodies(s, p, "select bodies to pattern")?;
    let op = pattern_op(p)?;
    let mut made = Vec::new();
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let original = b.shape.clone();
        let mut merged = original.clone();
        let bb = s.kernel.bounds(&original)?;
        let center = (bb.min + bb.max) / 2.0;
        for i in 1..n {
            let turn = (step * i as f64).to_radians();
            let copy = if uniform {
                // Only the position goes round: the centre's path around the axis.
                let q = glam::DQuat::from_axis_angle(axis.dir.normalize(), turn);
                let moved = axis.origin + q * (center - axis.origin);
                s.kernel.translate(&original, moved - center)?
            } else {
                s.kernel.rotate(&original, axis, turn)?
            };
            match op {
                Some(op) => merged = s.kernel.boolean(op, &merged, &copy)?,
                None => made.push(add_body(s, copy)?.0),
            }
        }
        if op.is_some() {
            set_body_shape(s, *id, merged)?;
        }
    }
    Ok(json!({ "bodies": made }))
}

/// How a pattern's copies end up (the Result badge): new bodies (None), or combined with the original. "merge": true
/// is the older spelling of "union".
fn pattern_op(p: &Params) -> Result<Option<BoolOp>> {
    Ok(match p.opt_str("result") {
        None if p.bool_or("merge", false) => Some(BoolOp::Union),
        None | Some("new") => None,
        Some("union") => Some(BoolOp::Union),
        Some("subtract") => Some(BoolOp::Subtract),
        Some("intersect") => Some(BoolOp::Intersect),
        Some(other) => return Err(EngineError::BadParams(format!("result is new, union, subtract or intersect, not {other:?}"))),
    })
}

/// What Align matches: a point, a straight edge (its middle and direction) or a flat face / plane.
enum AlignRef {
    Point(DVec3),
    Edge(DVec3, DVec3),
    Face,
}

fn align_ref(s: &Session, v: &Value) -> Result<AlignRef> {
    let t = Params::new(v);
    if t.has("point") {
        let q = t.vec3("point")?;
        if !q.is_finite() {
            return Err(EngineError::BadParams("point must be finite".into()));
        }
        return Ok(AlignRef::Point(q));
    }
    if t.has("edge") {
        let body = t.id("body")?;
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        return match s.kernel.edge_curve(&b.shape, t.f64("edge")? as u32)? {
            shapecraft_kernel::EdgeCurve::Line { a, b } => Ok(AlignRef::Edge((a + b) / 2.0, (b - a).normalize_or_zero())),
            _ => Err(EngineError::NotAvailable("pick a straight edge".into())),
        };
    }
    Ok(AlignRef::Face)
}

pub(super) fn align(s: &mut Session, p: &Params) -> Result<Value> {
    let from: Value = p.parse("from")?;
    let to: Value = p.parse("to")?;
    // Points and edges: a point goes onto a point (or an edge's middle); an edge turns parallel to the other and
    // their middles meet.
    let (fa, ta) = (align_ref(s, &from)?, align_ref(s, &to)?);
    if !matches!((&fa, &ta), (AlignRef::Face, AlignRef::Face)) {
        let ids = if p.has("items") { p.ids("items")? } else { vec![Params::new(&from).id("body")?] };
        let (o_s, o_t) = match (&fa, &ta) {
            (AlignRef::Point(a) | AlignRef::Edge(a, _), AlignRef::Point(b) | AlignRef::Edge(b, _)) => (*a, *b),
            _ => return Err(EngineError::NotAvailable("align a point or edge with a point or edge, or a face with a face".into())),
        };
        let rotation = match (&fa, &ta) {
            (AlignRef::Edge(_, d1), AlignRef::Edge(_, d2)) => {
                let want = if (d1.dot(*d2) < 0.0) != p.bool_or("flip", false) { -*d2 } else { *d2 };
                let cos = d1.dot(want).clamp(-1.0, 1.0);
                let cross = d1.cross(want);
                (cross.length() > 1e-9).then(|| (Axis::new(o_s, cross), cos.acos()))
            }
            _ => None,
        };
        for id in &ids {
            let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
            let mut shape = b.shape.clone();
            if let Some((axis, angle)) = rotation {
                shape = s.kernel.rotate(&shape, axis, angle)?;
            }
            shape = s.kernel.translate(&shape, o_t - o_s)?;
            set_body_shape(s, *id, shape)?;
        }
        s.selection = ids.iter().map(|&body| Pick::Body { body }).collect();
        return Ok(json!({ "aligned": ids.len() }));
    }
    let (o_s, n_s) = face_frame(s, &from)?;
    let (o_t, n_t) = face_frame(s, &to)?;
    let ids = if p.has("items") { p.ids("items")? } else { vec![Params::new(&from).id("body")?] };
    // Turn the source normal onto the reversed target normal (the faces end up touching), around the source
    // face's center, then slide the centers together. Flipped, it goes through to the target's other side.
    let want = if p.bool_or("flip", false) { n_t.normalize() } else { -n_t.normalize() };
    let have = n_s.normalize();
    let cos = have.dot(want).clamp(-1.0, 1.0);
    let rotation = if cos > 1.0 - 1e-12 {
        None
    } else if cos < -1.0 + 1e-12 {
        let perp = if have.x.abs() < 0.9 { have.cross(DVec3::X) } else { have.cross(DVec3::Y) };
        Some((Axis::new(o_s, perp), std::f64::consts::PI))
    } else {
        Some((Axis::new(o_s, have.cross(want)), cos.acos()))
    };
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let mut shape = b.shape.clone();
        if let Some((axis, angle)) = rotation {
            shape = s.kernel.rotate(&shape, axis, angle)?;
        }
        shape = s.kernel.translate(&shape, o_t - o_s)?;
        set_body_shape(s, *id, shape)?;
    }
    s.selection = ids.iter().map(|&body| Pick::Body { body }).collect();
    Ok(json!({ "aligned": ids.len() }))
}

pub(super) fn rotate(s: &mut Session, p: &Params) -> Result<Value> {
    if let Some(v) = thaw_copies(s, p)? {
        return Ok(v);
    }
    let axis = if p.has("about") {
        let a: Value = p.parse("about")?;
        topo_axis(s, &Params::new(&a))?
    } else {
        Axis::new(p.vec3("origin")?, p.vec3("axis")?)
    };
    if let Some(sel) = selected_entities(s, p) {
        let pl = sketch_plane_of(s, sel.0)?;
        let angle = p.f64("angle")?;
        let dir = axis.dir.try_normalize().ok_or_else(|| EngineError::BadParams("axis must not be zero".into()))?;
        // Turned about an axis across the sketch, the curves leave its plane.
        let copy = p.bool_or("copy", false);
        if dir.cross(pl.normal()).length() > 1e-6 && angle.abs() > 1e-12 {
            let q = glam::DQuat::from_axis_angle(dir, angle.to_radians());
            return lift_curves(s, sel, copy, |pl| SketchPlane {
                origin: axis.origin + q * (pl.origin - axis.origin),
                x: q * pl.x,
                y: q * pl.y,
            });
        }
        let m = sketch_rotation(&pl, axis, angle)?;
        return sketch_xform(s, sel, &[m], copy, false);
    }
    let angle = p.f64("angle")?.to_radians();
    let ids = if p.has("items") { p.ids("items")? } else { selected_movables(s) };
    if ids.is_empty() {
        return Err(EngineError::NotAvailable("select bodies to rotate".into()));
    }
    let copy = p.bool_or("copy", false);
    let mut made = Vec::new();
    let mut bodies = Vec::new();
    let q = glam::DQuat::from_axis_angle(axis.dir.normalize_or(DVec3::Z), angle);
    for id in &ids {
        if s.doc.body(*id).is_none() {
            let turn = |pl: SketchPlane| SketchPlane { origin: axis.origin + q * (pl.origin - axis.origin), x: q * pl.x, y: q * pl.y };
            made.extend(xform_plane(s, *id, copy, turn)?);
            continue;
        }
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let turned = s.kernel.rotate(&b.shape, axis, angle)?;
        if copy {
            let c = add_copy(s, *id, turned)?;
            made.push(c);
            bodies.push((*id, c));
        } else {
            set_body_shape(s, *id, turned)?;
        }
    }
    if copy {
        s.selection = made.iter().map(|&id| item_pick(s, id)).collect();
    }
    freeze_copies(s, p, &bodies)?;
    Ok(json!({ "rotated": ids.len(), "copies": made.iter().map(|b| b.0).collect::<Vec<_>>() }))
}

/// A mirrored copy stays linked to what it mirrors (as the reference's mirror keeps its result): when a line of the
/// sketch lies on the mirror line, each mirrored point gets a Symmetric constraint about it, and circles and arcs an
/// Equal one. Copies of curves that came apart (a rectangle into sides) aren't linked.
fn link_mirror(s: &mut Session, sketch: ItemId, ents: &[u32], before: usize, o: DVec2, n: DVec2) {
    use shapecraft_doc::constraints::{Element, points};
    let Some(sk) = s.doc.sketch_mut(sketch) else { return };
    let size = sk.entities.iter().flat_map(points).fold(1.0f64, |m, p| m.max(p.length()));
    let tol = size * 1e-7;
    let on_axis = |p: DVec2| (p - o).dot(n).abs() < tol;
    let Some(axis) =
        sk.entities.iter().position(|e| matches!(e, SketchEntity::Line { a, b } if on_axis(*a) && on_axis(*b) && a.distance(*b) > tol))
    else {
        return;
    };
    if sk.entities.len() != before + ents.len() {
        return;
    }
    let mirror = |p: DVec2| p - n * (2.0 * (p - o).dot(n));
    let mut added = Vec::new();
    for (k, &i) in ents.iter().enumerate() {
        let j = before + k;
        if i as usize == axis {
            continue;
        }
        let (Some(src), Some(dst)) = (sk.entities.get(i as usize), sk.entities.get(j)) else { continue };
        let (ps, pd) = (points(src), points(dst));
        for (ki, p) in ps.iter().enumerate() {
            if on_axis(*p) {
                continue;
            }
            if let Some(kj) = pd.iter().position(|q| q.distance(mirror(*p)) < tol * 10.0) {
                added.push(Constraint::Symmetric {
                    a: Element::Point { entity: i, index: ki as u32 },
                    b: Element::Point { entity: j as u32, index: kj as u32 },
                    axis: Element::Segment { entity: axis as u32, index: 0 },
                });
            }
        }
        if matches!(src, SketchEntity::Circle { .. } | SketchEntity::Arc { .. }) {
            added.push(Constraint::Equal { a: Element::Curve { entity: i }, b: Element::Curve { entity: j as u32 } });
        }
    }
    let mut all = sk.constraints.clone();
    all.extend(added);
    // Only kept when the sketch still holds together with them.
    if shapecraft_doc::constraints::error(&sk.entities, &all) < tol.max(1e-9) * 100.0 {
        sk.constraints = all;
    }
}

pub(super) fn mirror(s: &mut Session, p: &Params) -> Result<Value> {
    let origin = p.vec3("origin")?;
    let normal = p.vec3("normal")?;
    let copy = p.bool_or("copy", true);
    if let Some(sel) = selected_entities(s, p) {
        let pl = sketch_plane_of(s, sel.0)?;
        if normal.dot(pl.normal()).abs() > 1e-6 * normal.length().max(1e-12) {
            return Err(EngineError::NotAvailable("pick a mirror line or plane across the sketch".into()));
        }
        let n = DVec2::new(normal.dot(pl.x), normal.dot(pl.y))
            .try_normalize()
            .ok_or_else(|| EngineError::BadParams("normal must not be zero".into()))?;
        let o = pl.to_local(origin);
        // Reflection across the line through o perpendicular to n.
        let lin = glam::DMat2::from_cols(DVec2::X - n * (2.0 * n.x), DVec2::Y - n * (2.0 * n.y));
        let m = DAffine2::from_mat2_translation(lin, o - lin * o);
        let (sketch, ents) = (sel.0, sel.1.clone());
        let before = s.doc.sketch(sketch).map_or(0, |k| k.entities.len());
        let out = sketch_xform(s, sel, &[m], copy, false)?;
        if copy {
            link_mirror(s, sketch, &ents, before, o, n);
        }
        return Ok(out);
    }
    let ids = if p.has("items") { p.ids("items")? } else { selected_bodies(s) };
    if ids.is_empty() {
        return Err(EngineError::NotAvailable("select bodies to mirror".into()));
    }
    let mut made = Vec::new();
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let m = s.kernel.mirror(&b.shape, origin, normal)?;
        if copy {
            made.push(add_body(s, m)?.0);
        } else {
            set_body_shape(s, *id, m)?;
        }
    }
    Ok(json!({ "bodies": made }))
}

pub(super) fn pattern(s: &mut Session, p: &Params) -> Result<Value> {
    let dir = p.vec3("direction")?.try_normalize().ok_or_else(|| EngineError::BadParams("direction must not be zero".into()))?;
    let spacing = p.f64("spacing")?;
    let count = count_param(s, p, "count", 0.0)?;
    if !(2.0..=500.0).contains(&count) {
        return Err(EngineError::BadParams("count must be 2–500".into()));
    }
    if let Some(sel) = selected_entities(s, p) {
        let pl = sketch_plane_of(s, sel.0)?;
        let d = DVec2::new(dir.dot(pl.x), dir.dot(pl.y)) * spacing;
        let maps: Vec<DAffine2> = (1..count as usize).map(|i| DAffine2::from_translation(d * i as f64)).collect();
        return sketch_xform(s, sel, &maps, true, true);
    }
    let ids = if p.has("items") { p.ids("items")? } else { selected_bodies(s) };
    if ids.is_empty() {
        return Err(EngineError::NotAvailable("select bodies to pattern".into()));
    }
    let op = pattern_op(p)?;
    let mut made = Vec::new();
    for id in &ids {
        let b = s.doc.body(*id).ok_or_else(|| EngineError::BadParams(format!("no body {}", id.0)))?;
        let original = b.shape.clone();
        let mut merged = original.clone();
        for i in 1..count as usize {
            let copy = s.kernel.translate(&original, dir * spacing * i as f64)?;
            match op {
                Some(op) => merged = s.kernel.boolean(op, &merged, &copy)?,
                None => made.push(add_body(s, copy)?.0),
            }
        }
        if op.is_some() {
            set_body_shape(s, *id, merged)?;
        }
    }
    Ok(json!({ "bodies": made }))
}

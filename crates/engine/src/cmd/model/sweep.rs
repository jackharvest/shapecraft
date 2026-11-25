//! Revolve, loft and sweep, and the profiles (sketch regions or flat faces) they take.

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};
use shapecraft_kernel::{Axis, BoolOp, Shape};

use super::extrude::{place_result, result_param};
use crate::{EngineError, Params, Result, Session};

/// An axis from a body's edge or face (`{"body": id, "edge": n}` / `{"body": id, "face": n}`) or a
/// construction axis (`{"axis": id}`).
pub(super) fn topo_axis(s: &Session, ap: &Params) -> Result<Axis> {
    if ap.has("axis") {
        let id = ap.id("axis")?;
        let a = s.doc.axis(id).ok_or_else(|| EngineError::BadParams(format!("no axis {}", id.0)))?;
        return Ok(Axis::new(a.origin, a.dir));
    }
    let body = ap.id("body")?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let sub = if ap.has("edge") {
        shapecraft_kernel::SubShape::Edge(ap.f64("edge")? as u32)
    } else {
        shapecraft_kernel::SubShape::Face(ap.f64("face")? as u32)
    };
    s.kernel
        .axis_of(&b.shape, sub)?
        .ok_or_else(|| EngineError::NotAvailable("that edge or face has no axis (pick a straight or round one)".into()))
}

pub(super) fn revolve(s: &mut Session, p: &Params) -> Result<Value> {
    let (shape, profile, result) = revolve_shape(s, p)?;
    let (result, on_body) = match result {
        Some(r) => (Some(r), profile_body(s, profile)),
        None => {
            let (op, target) = auto_result(s, &shape, profile_body(s, profile))?;
            (Some(op), target)
        }
    };
    let body = place_result(s, shape, result, on_body, BoolOp::Union)?;
    consume_sketches(s, &profile_sketch(profile).into_iter().collect::<Vec<_>>());
    s.selection = vec![Pick::Body { body }];
    Ok(json!({ "body": body.0 }))
}

/// What Revolve's Result does when left on auto, for these params: "new", "union" or "subtract".
pub fn revolve_auto(s: &Session, params: &Value) -> Result<&'static str> {
    let p = Params::with_variables(params, &s.doc.variables);
    let (shape, profile, _) = revolve_shape(s, &p)?;
    Ok(match auto_result(s, &shape, profile_body(s, profile))?.0 {
        Some(BoolOp::Union) => "union",
        Some(BoolOp::Subtract) => "subtract",
        _ => "new",
    })
}

/// The body a profile was made on: a face's own, or the body a region's sketch sits on.
fn profile_body(s: &Session, profile: Profile) -> Option<ItemId> {
    match profile {
        Profile::Face { body, .. } => Some(body),
        Profile::Region { sketch, .. } => s.doc.sketch(sketch).and_then(|sk| sk.on_body),
    }
}

/// The manual's automatic Boolean for a new solid, and the body it acts on: a new body when it meets none; a union
/// when it only touches one (grown away from the face or attached sketch it started on); a subtract when it lies
/// wholly inside one. Partly through a body (a full turn of a face about its own edge) it is a new body, as
/// observed in the reference (s25 r2), which the manual's table leaves open.
fn auto_result(s: &Session, shape: &Shape, on_body: Option<ItemId>) -> Result<(Option<BoolOp>, Option<ItemId>)> {
    use shapecraft_kernel::SubShape;
    let own = s.kernel.mass_props(shape)?.volume.abs();
    let bb = s.kernel.bounds(shape)?;
    let candidates: Vec<ItemId> =
        on_body.into_iter().chain(s.doc.bodies.iter().filter(|b| s.doc.item_shown(b.id) && Some(b.id) != on_body).map(|b| b.id)).collect();
    for id in candidates {
        let Some(b) = s.doc.body(id) else { continue };
        let o = crate::cmd::model::body_box(s, b)?;
        let near = (bb.max - bb.min).length().max(1.0) * 1e-6;
        if !((o.min - near).cmple(bb.max).all() && (bb.min - near).cmple(o.max).all()) {
            continue;
        }
        if s.kernel.distance(&b.shape, SubShape::Whole, shape, SubShape::Whole)?.distance > near {
            continue;
        }
        let shared =
            s.kernel.boolean(BoolOp::Intersect, &b.shape, shape).and_then(|x| s.kernel.mass_props(&x)).map_or(0.0, |m| m.volume.abs());
        return Ok(if shared <= own * 1e-6 {
            (Some(BoolOp::Union), Some(id))
        } else if shared >= own * (1.0 - 1e-6) {
            (Some(BoolOp::Subtract), Some(id))
        } else {
            (None, None)
        });
    }
    Ok((None, None))
}

/// Revolve's solid, its profile and the Result asked for (`None` = auto).
fn revolve_shape(s: &Session, p: &Params) -> Result<(Shape, Profile, Option<Option<BoolOp>>)> {
    let profile = if p.has("sketch") {
        Profile::Region { sketch: p.id("sketch")?, region: p.f64_or("region", 0.0)? as u32 }
    } else if p.has("body") {
        Profile::Face { body: p.id("body")?, face: p.f64("face")? as u32 }
    } else {
        s.selection
            .iter()
            .find_map(|pk| match *pk {
                Pick::Region { sketch, region } => Some(Profile::Region { sketch, region }),
                Pick::Face { body, face } => Some(Profile::Face { body, face }),
                _ => None,
            })
            .ok_or_else(|| EngineError::NotAvailable("select a sketch region or flat face to revolve".into()))?
    };
    let angle = p.f64_or("angle", 360.0)?;
    // A height makes it helical: then the angle may run past one turn (360° per turn).
    let height = p.f64_or("height", 0.0)?;
    if !height.is_finite() || height.abs() > 1e6 {
        return Err(EngineError::BadParams("height must be a length".into()));
    }
    let max_angle = if height.abs() > 1e-9 { 360.0 * 200.0 } else { 360.0 };
    if !angle.is_finite() || angle.abs() < 1e-6 || angle.abs() > max_angle {
        return Err(EngineError::BadParams(if height.abs() > 1e-9 {
            "angle must be between 0 and 72000 degrees (200 turns)".into()
        } else {
            "angle must be between 0 and 360 degrees".into()
        }));
    }
    let face = profile_face(s, profile)?;
    let sketch = profile_sketch(profile).and_then(|id| s.doc.sketch(id));
    let axis = if let Some(name) = p.opt_str("axis") {
        let sk = sketch.ok_or_else(|| EngineError::BadParams("\"u\" and \"v\" axes need a sketch region".into()))?;
        match name {
            "u" => Axis::new(sk.plane.origin, sk.plane.x),
            "v" => Axis::new(sk.plane.origin, sk.plane.y),
            other => return Err(EngineError::BadParams(format!("unknown axis '{other}'"))),
        }
    } else if !p.has("axis") {
        let sk = sketch.ok_or_else(|| EngineError::NotAvailable("pick an edge to revolve around".into()))?;
        Axis::new(sk.plane.origin, sk.plane.y)
    } else {
        let a: Value = p.parse("axis")?;
        let ap = Params::new(&a);
        if ap.has("line") {
            let sk = sketch.ok_or_else(|| EngineError::BadParams("a sketch line axis needs a sketch region".into()))?;
            let i = ap.f64("line")? as usize;
            match sk.entities.get(i) {
                Some(shapecraft_doc::SketchEntity::Line { a, b }) => {
                    let (wa, wb) = (sk.plane.to_world(*a), sk.plane.to_world(*b));
                    Axis::new(wa, wb - wa)
                }
                _ => return Err(EngineError::BadParams(format!("entity {i} isn't a line"))),
            }
        } else if ap.has("body") || ap.has("axis") {
            topo_axis(s, &ap)?
        } else {
            Axis::new(ap.vec3("origin")?, ap.vec3("dir")?)
        }
    };
    let result = result_param(p)?;
    let shape = if height.abs() > 1e-9 {
        s.kernel.revolve_helix(&face, axis, angle.to_radians(), height)?
    } else {
        s.kernel.revolve(&face, axis, angle.to_radians())?
    };
    Ok((shape, profile, result))
}

/// A profile for loft or sweep: a sketch region or a flat body face.
#[derive(Clone, Copy)]
pub(super) enum Profile {
    Region { sketch: ItemId, region: u32 },
    Face { body: ItemId, face: u32 },
}

pub(super) fn profile_param(v: &Value) -> Result<Profile> {
    let p = Params::new(v);
    if p.has("sketch") {
        Ok(Profile::Region { sketch: p.id("sketch")?, region: p.f64_or("region", 0.0)? as u32 })
    } else {
        Ok(Profile::Face { body: p.id("body")?, face: p.f64("face")? as u32 })
    }
}

fn selected_profiles(s: &Session) -> Vec<Profile> {
    s.selection
        .iter()
        .filter_map(|pk| match *pk {
            Pick::Region { sketch, region } => Some(Profile::Region { sketch, region }),
            Pick::Face { body, face } => Some(Profile::Face { body, face }),
            _ => None,
        })
        .collect()
}

pub(super) fn profile_face(s: &Session, pr: Profile) -> Result<Shape> {
    match pr {
        Profile::Region { sketch, region } => {
            let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
            Ok(sk.region(region).ok_or_else(|| EngineError::BadParams(format!("no region {region}")))?.face.clone())
        }
        Profile::Face { body, face } => {
            let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
            if s.kernel.face_plane(&b.shape, face)?.is_none() {
                return Err(EngineError::NotAvailable("only flat faces can be profiles".into()));
            }
            Ok(s.kernel.face(&b.shape, face)?)
        }
    }
}

/// Hides the sketches a new body was made from, like the reference does after Extrude.
fn consume_sketches(s: &mut Session, sketches: &[ItemId]) {
    for id in sketches {
        if let Some(sk) = s.doc.sketch_mut(*id) {
            sk.visible = false;
        }
        if s.active_sketch == Some(*id) {
            s.active_sketch = None;
        }
    }
}

fn profile_sketch(pr: Profile) -> Option<ItemId> {
    match pr {
        Profile::Region { sketch, .. } => Some(sketch),
        Profile::Face { .. } => None,
    }
}

pub(super) fn loft(s: &mut Session, p: &Params) -> Result<Value> {
    let profiles = if p.has("profiles") {
        let list: Vec<Value> = p.parse("profiles")?;
        list.iter().map(profile_param).collect::<Result<Vec<_>>>()?
    } else {
        selected_profiles(s)
    };
    if profiles.len() < 2 {
        return Err(EngineError::NotAvailable("select two or more regions or flat faces to loft".into()));
    }
    if profiles.len() > 64 {
        return Err(EngineError::BadParams("too many loft profiles".into()));
    }
    let faces = profiles.iter().map(|pr| profile_face(s, *pr)).collect::<Result<Vec<_>>>()?;
    let result = result_param(p)?;
    let guides = if p.has("guides") {
        match p.parse::<Value>("guides")? {
            Value::Array(list) => list.iter().map(|v| path_part(s, v)).collect::<Result<Vec<_>>>()?.concat(),
            v => path_part(s, &v)?,
        }
    } else {
        Vec::new()
    };
    let shape = match skin_options(p, !guides.is_empty())? {
        Some(how) => super::skin::loft(s, &profiles, &faces, &guides, &how)?,
        None => s.kernel.loft_ruled(&faces, p.bool_or("ruled", false))?,
    };
    let body = place_result(s, shape, result, None, BoolOp::Union)?;
    let used: Vec<ItemId> = profiles.iter().filter_map(|pr| profile_sketch(*pr)).collect();
    consume_sketches(s, &used);
    // Nothing stays picked.
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

/// Loft's card settings past the plain loft: `None` when there are none.
fn skin_options(p: &Params, guided: bool) -> Result<Option<super::skin::LoftOptions>> {
    use super::skin::{LoftEnd, LoftOptions};
    let end = |which: &str| -> Result<Option<LoftEnd>> {
        let magnitude = p.f64_or(&format!("{which}_magnitude"), 1.0)?;
        if !(magnitude.is_finite() && magnitude.abs() <= 100.0) {
            return Err(EngineError::BadParams("a tangent magnitude is between -100 and 100".into()));
        }
        Ok(match p.opt_str(&format!("{which}_continuity")).unwrap_or("none") {
            "none" => None,
            "g1" => Some(LoftEnd { curvature: false, magnitude }),
            "g2" => Some(LoftEnd { curvature: true, magnitude }),
            o => return Err(EngineError::BadParams(format!("continuity is none, g1 or g2, not '{o}'"))),
        })
    };
    let how = LoftOptions { periodic: p.bool_or("periodic", false), start: end("start")?, end: end("end")? };
    if how.periodic || how.start.is_some() || how.end.is_some() || guided {
        if p.bool_or("ruled", false) {
            return Err(EngineError::BadParams("a ruled loft has no guide curves, tangents or closing".into()));
        }
        return Ok(Some(how));
    }
    Ok(None)
}

pub(super) fn sweep(s: &mut Session, p: &Params) -> Result<Value> {
    let profile = if p.has("profile") {
        profile_param(&p.parse::<Value>("profile")?)?
    } else {
        *selected_profiles(s).first().ok_or_else(|| EngineError::NotAvailable("select a region or flat face to sweep".into()))?
    };
    let path = if p.has("path") {
        match p.parse::<Value>("path")? {
            Value::Array(list) => list.iter().map(|v| path_part(s, v)).collect::<Result<Vec<_>>>()?.concat(),
            v => path_part(s, &v)?,
        }
    } else {
        s.selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::SketchEntity { sketch, entity } => Some(PathPart::Curve { sketch, entity }),
                Pick::Edge { body, edge } => Some(PathPart::Edge { body, edge }),
                _ => None,
            })
            .collect()
    };
    if path.is_empty() {
        return Err(EngineError::NotAvailable("select the sketch curves or edges to sweep along".into()));
    }
    if path.len() > 10_000 {
        return Err(EngineError::BadParams("too many path curves".into()));
    }
    let face = profile_face(s, profile)?;
    let result = result_param(p)?;
    // The Sweep card (26.0): Scale, Orientation, Twist and Corner type.
    let opts = shapecraft_kernel::SweepOptions {
        twist: p.f64_or("twist", 0.0)?,
        scale: p.f64_or("scale", 1.0)?,
        parallel: match p.opt_str("orientation").unwrap_or("normal") {
            "normal" => false,
            "parallel" => true,
            o => return Err(EngineError::BadParams(format!("orientation is normal or parallel, not '{o}'"))),
        },
        round: match p.opt_str("corners").unwrap_or("mitre") {
            "mitre" => false,
            "round" => true,
            c => return Err(EngineError::BadParams(format!("corners are mitre or round, not '{c}'"))),
        },
    };
    let position = match p.opt_str("position").unwrap_or("auto") {
        "auto" => Position::Auto,
        "intersection" => Position::Intersection,
        "point" => Position::Point,
        "endpoint" => Position::Endpoint,
        o => return Err(EngineError::BadParams(format!("position is auto, intersection, point or endpoint, not '{o}'"))),
    };
    let shape = sweep_shape(s, &face, &path, position, &opts)?;
    let body = place_result(s, shape, result, None, BoolOp::Union)?;
    let mut used: Vec<ItemId> = path.iter().filter_map(PathPart::sketch).chain(profile_sketch(profile)).collect();
    used.sort_unstable();
    used.dedup();
    consume_sketches(s, &used);
    // Nothing stays picked.
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

/// One piece of a sweep's path: a sketch curve or a body's edge.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PathPart {
    Curve { sketch: ItemId, entity: u32 },
    Edge { body: ItemId, edge: u32 },
}

impl PathPart {
    pub(super) fn sketch(&self) -> Option<ItemId> {
        match *self {
            PathPart::Curve { sketch, .. } => Some(sketch),
            PathPart::Edge { .. } => None,
        }
    }
}

/// `{"sketch": id, "entities": [n]?}` (a sketch's curves, all of them by default) or `{"body": id, "edge": n | "edges":
/// [n]}`.
pub(super) fn path_part(s: &Session, v: &Value) -> Result<Vec<PathPart>> {
    let pp = Params::new(v);
    if pp.has("body") {
        let body = pp.id("body")?;
        let edges: Vec<f64> = if pp.has("edges") { pp.parse("edges")? } else { vec![pp.f64("edge")?] };
        return Ok(edges.into_iter().map(|e| PathPart::Edge { body, edge: e as u32 }).collect());
    }
    let sketch = pp.id("sketch")?;
    let entities: Vec<f64> = if pp.has("entities") {
        pp.parse("entities")?
    } else {
        (0..s.doc.sketch(sketch).map_or(0, |k| k.entities.len())).map(|i| i as f64).collect()
    };
    Ok(entities.into_iter().map(|e| PathPart::Curve { sketch, entity: e as u32 }).collect())
}

/// Sweep's Profile Position: which point of the path the profile belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Auto,
    Intersection,
    Point,
    Endpoint,
}

pub(super) fn entity_curves(
    s: &Session,
    sketch: ItemId,
    entity: u32,
) -> Result<(shapecraft_kernel::PlaneFrame, Vec<shapecraft_kernel::PlanarCurve>)> {
    let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
    let e = sk.entities.get(entity as usize).ok_or_else(|| EngineError::BadParams(format!("no sketch curve {entity}")))?;
    let mut one = sk.clone();
    one.entities = vec![e.clone()];
    Ok((sk.frame(), one.curves()))
}

fn sweep_shape(s: &Session, face: &Shape, path: &[PathPart], position: Position, opts: &shapecraft_kernel::SweepOptions) -> Result<Shape> {
    // Curves of one sketch go to the kernel as they are.
    let one_sketch = path.first().and_then(PathPart::sketch).filter(|k| path.iter().all(|p| p.sketch() == Some(*k)));
    let planar = match one_sketch {
        Some(_) => {
            let mut all = Vec::new();
            let mut frame = None;
            for part in path {
                if let PathPart::Curve { sketch, entity } = *part {
                    let (f, curves) = entity_curves(s, sketch, entity)?;
                    frame = Some(f);
                    all.extend(curves);
                }
            }
            frame.map(|f| (f, all))
        }
        None => None,
    };
    if let Some((_, curves)) = &planar
        && curves.is_empty()
    {
        return Err(EngineError::NotAvailable("the sweep path is empty".into()));
    }
    let mut shapes = Vec::new();
    for part in path {
        shapes.push(match *part {
            PathPart::Curve { sketch, entity } => {
                let (frame, curves) = entity_curves(s, sketch, entity)?;
                if curves.is_empty() {
                    continue;
                }
                match s.kernel.curves_shape(frame, &curves) {
                    Ok(shape) => shape,
                    // A kernel without loose curves still sweeps along one sketch.
                    Err(e) => match &planar {
                        Some((f, all)) if matches!(position, Position::Auto) => return Ok(s.kernel.sweep_with(face, *f, all, opts)?),
                        _ => return Err(e.into()),
                    },
                }
            }
            PathPart::Edge { body, edge } => {
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                s.kernel.edge(&b.shape, edge)?
            }
        });
    }
    let at = attach_point(s, face, &shapes, position)?;
    match planar {
        Some((frame, curves)) if at.is_none() => Ok(s.kernel.sweep_with(face, frame, &curves, opts)?),
        _ => Ok(s.kernel.sweep_path(face, &shapes, at, opts)?),
    }
}

/// Points along an edge's curve, close enough to find crossings on.
pub(super) fn along(c: &shapecraft_kernel::EdgeCurve) -> Vec<DVec3> {
    along_n(c, 64)
}

/// [`along`] with `n` steps on round curves.
pub(super) fn along_n(c: &shapecraft_kernel::EdgeCurve, n: usize) -> Vec<DVec3> {
    use shapecraft_kernel::EdgeCurve;
    let steps = n;
    match c {
        EdgeCurve::Line { a, b } => vec![*a, *b],
        EdgeCurve::Other { points } => points.clone(),
        EdgeCurve::Circle { center, normal, radius } => {
            let n = normal.normalize_or(DVec3::Z);
            let x = n.any_orthonormal_vector() * *radius;
            let y = n.cross(x);
            (0..=steps).map(|i| std::f64::consts::TAU * i as f64 / steps as f64).map(|t| *center + x * t.cos() + y * t.sin()).collect()
        }
        EdgeCurve::Arc { a, mid, b } => match arc_frame(*a, *mid, *b) {
            Some((c, x, y, sweep)) => (0..=steps).map(|i| sweep * i as f64 / steps as f64).map(|t| c + x * t.cos() + y * t.sin()).collect(),
            None => vec![*a, *mid, *b],
        },
    }
}

/// The circle through three points: its center, the radius vector to `a`, the one a quarter turn on towards `b`, and
/// the angle to `b`.
fn arc_frame(a: DVec3, mid: DVec3, b: DVec3) -> Option<(DVec3, DVec3, DVec3, f64)> {
    let (u, v) = (mid - a, b - a);
    let n = u.cross(v);
    let nn = n.length_squared();
    if nn < 1e-24 {
        return None;
    }
    let c = a + (u.length_squared() * v.cross(n) + v.length_squared() * n.cross(u)) / (2.0 * nn);
    let x = a - c;
    let y = n.normalize().cross(x);
    let r = b - c;
    let mut sweep = r.dot(y).atan2(r.dot(x));
    if sweep <= 0.0 {
        sweep += std::f64::consts::TAU;
    }
    Some((c, x, y, sweep))
}

/// The point of the path the profile belongs to. `None` leaves it to the kernel: the profile where it is, at the end
/// of the path it stands on (what Auto finds first) or the point nearest to it.
fn attach_point(s: &Session, face: &Shape, path: &[Shape], position: Position) -> Result<Option<DVec3>> {
    use shapecraft_kernel::SubShape;
    let Some(plane) = s.kernel.face_plane(face, 0)? else { return Ok(None) };
    let n = plane.normal.normalize_or(DVec3::Z);
    let (_, center) = s.kernel.measure(face, SubShape::Face(0))?;
    let mut runs: Vec<Vec<DVec3>> = Vec::new();
    for shape in path {
        for e in 0..s.kernel.edge_count(shape)? {
            runs.push(along(&s.kernel.edge_curve(shape, e)?));
        }
    }
    let size = runs.iter().flatten().map(|q| q.distance(center)).fold(1.0, f64::max);
    let tol = size * 1e-6;
    // The path's loose ends: end points no other piece shares.
    let tips: Vec<DVec3> = runs.iter().flat_map(|r| [r.first().copied(), r.last().copied()]).flatten().collect();
    let ends =
        tips.iter().enumerate().filter(|(i, a)| !tips.iter().enumerate().any(|(j, b)| j != *i && a.distance(*b) < 1e-3)).map(|(_, a)| *a);
    let nearest = |pts: Vec<DVec3>| pts.into_iter().min_by(|a, b| a.distance(center).total_cmp(&b.distance(center)));
    let crossings = || -> Vec<DVec3> {
        let side = |q: DVec3| (q - plane.origin).dot(n);
        runs.iter()
            .flat_map(|r| r.windows(2))
            .filter_map(|w| {
                let (da, db) = (side(w[0]), side(w[1]));
                if da.abs() <= tol {
                    return Some(w[0]);
                }
                if db.abs() <= tol {
                    return Some(w[1]);
                }
                (da * db < 0.0).then(|| w[0] + (w[1] - w[0]) * (da / (da - db)))
            })
            .collect()
    };
    Ok(match position {
        Position::Endpoint => {
            let ends: Vec<DVec3> = ends.collect();
            // A closed path has none: its nearest joint.
            nearest(if ends.is_empty() { tips.clone() } else { ends })
        }
        Position::Point => {
            let mut best: Option<(f64, DVec3)> = None;
            for shape in path {
                let d = s.kernel.distance(face, SubShape::Whole, shape, SubShape::Whole)?;
                if best.is_none_or(|(b, _)| d.distance < b) {
                    best = Some((d.distance, d.b));
                }
            }
            best.map(|(_, q)| q)
        }
        Position::Intersection => {
            Some(nearest(crossings()).ok_or_else(|| EngineError::NotAvailable("the path doesn't cross the profile's plane".into()))?)
        }
        Position::Auto => {
            let ends: Vec<DVec3> = ends.collect();
            let start = if ends.is_empty() { tips.first().copied() } else { None };
            if ends.into_iter().chain(start).any(|q| (q - plane.origin).dot(n).abs() <= tol) { None } else { nearest(crossings()) }
        }
    })
}

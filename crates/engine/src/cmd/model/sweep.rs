//! Revolve, loft and sweep, and the profiles (sketch regions or flat faces) they take.

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
        on_body.into_iter().chain(s.doc.bodies.iter().filter(|b| b.visible && Some(b.id) != on_body).map(|b| b.id)).collect();
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

fn profile_param(v: &Value) -> Result<Profile> {
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
    let shape = s.kernel.loft_ruled(&faces, p.bool_or("ruled", false))?;
    let body = place_result(s, shape, result, None, BoolOp::Union)?;
    let used: Vec<ItemId> = profiles.iter().filter_map(|pr| profile_sketch(*pr)).collect();
    consume_sketches(s, &used);
    s.selection = vec![Pick::Body { body }];
    Ok(json!({ "body": body.0 }))
}

pub(super) fn sweep(s: &mut Session, p: &Params) -> Result<Value> {
    let profile = if p.has("profile") {
        profile_param(&p.parse::<Value>("profile")?)?
    } else {
        *selected_profiles(s).first().ok_or_else(|| EngineError::NotAvailable("select a region or flat face to sweep".into()))?
    };
    let (path_sketch, entities) = if p.has("path") {
        let v: Value = p.parse("path")?;
        let pp = Params::new(&v);
        let sk = pp.id("sketch")?;
        let ents = if pp.has("entities") {
            pp.parse::<Vec<f64>>("entities")?.into_iter().map(|e| e as u32).collect()
        } else {
            let n = s.doc.sketch(sk).map_or(0, |k| k.entities.len());
            (0..n as u32).collect()
        };
        (sk, ents)
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
        (sketch.ok_or_else(|| EngineError::NotAvailable("select the sketch curves to sweep along".into()))?, ents)
    };
    if entities.len() > 10_000 {
        return Err(EngineError::BadParams("too many path curves".into()));
    }
    let sk = s.doc.sketch(path_sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", path_sketch.0)))?;
    // Each entity becomes one or more curves; keep only the chosen ones.
    let mut path = Vec::new();
    for &i in &entities {
        let e = sk.entities.get(i as usize).ok_or_else(|| EngineError::BadParams(format!("no sketch curve {i}")))?;
        let mut one = sk.clone();
        one.entities = vec![e.clone()];
        path.extend(one.curves());
    }
    if path.is_empty() {
        return Err(EngineError::NotAvailable("the sweep path is empty".into()));
    }
    let frame = sk.frame();
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
    let shape = s.kernel.sweep_with(&face, frame, &path, &opts)?;
    let body = place_result(s, shape, result, None, BoolOp::Union)?;
    let mut used = vec![path_sketch];
    used.extend(profile_sketch(profile));
    consume_sketches(s, &used);
    s.selection = vec![Pick::Body { body }];
    Ok(json!({ "body": body.0 }))
}

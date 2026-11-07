//! Edge and face features: fillet, chamfer, shell, offset and replace face, and project as edge.

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};
use shapecraft_kernel::{BoolOp, Shape};

use super::sweep::{Profile, profile_face};
use super::{face_frame, set_body_shape};
use crate::{EngineError, Params, Result, Session};

pub(super) fn fillet(s: &mut Session, p: &Params) -> Result<Value> {
    let radius = p.f64("radius")?;
    let (body, edges) = if p.has("body") {
        let body = p.id("body")?;
        let edges = if p.has("edges") { p.u32s("edges")? } else { Vec::new() };
        (body, edges)
    } else {
        let mut body = None;
        let mut edges = Vec::new();
        for pick in &s.selection {
            match *pick {
                Pick::Edge { body: b, edge } if body.is_none_or(|x| x == b) => {
                    body = Some(b);
                    edges.push(edge);
                }
                Pick::Body { body: b } if body.is_none() => body = Some(b),
                _ => {}
            }
        }
        (body.ok_or_else(|| EngineError::NotAvailable("select edges to fillet".into()))?, edges)
    };
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    // Variable radius: `radius` at each edge's start, `radius2` at its end.
    let radius2 = if p.has("radius2") { Some(p.f64("radius2")?) } else { None }.filter(|r2| (r2 - radius).abs() > 1e-12);
    // Size Type (Radius or Chord Width) and Corner (Rolling Ball or Setback), from the manual's Fillet card.
    let chordal = match p.opt_str("mode").or(p.opt_str("size_type")).unwrap_or("radius") {
        "radius" => false,
        "chordal" | "chord_width" => true,
        other => return Err(EngineError::BadParams(format!("a fillet's size is a radius or chordal, not {other:?}"))),
    };
    let setback = match (p.opt_str("corner"), p.has("setback") && p.bool_or("setback", false)) {
        (Some("setback"), _) | (None, true) => true,
        (Some("rolling_ball") | None, false) => false,
        (Some("rolling_ball"), true) => return Err(EngineError::BadParams("pick one corner type".into())),
        (Some(other), _) => return Err(EngineError::BadParams(format!("corners are rolling_ball or setback, not {other:?}"))),
    };
    // Curvature (the card's profile slider, the manual's Magnitude): -1 flat (a bevel), 0 the usual round, 1 a sharp
    // corner; anything but 0 loses the true radius ("radius" then sets where the round meets the faces).
    let curvature = if p.has("curvature") { p.f64("curvature")? } else { 0.0 };
    if !(curvature.is_finite() && (-1.0..=1.0).contains(&curvature)) {
        return Err(EngineError::BadParams("curvature runs from -1 (flat) to 1 (sharp)".into()));
    }
    let shaped = curvature.abs() > 1e-9;
    if chordal || setback {
        if shaped {
            return Err(EngineError::NotAvailable("a fillet's curvature works with a radius and rolling-ball corners".into()));
        }
        if radius2.is_some() || p.opt_str("continuity").is_some_and(|c| c != "g1") {
            return Err(EngineError::NotAvailable("chordal fillets and setback corners are G1 with one size".into()));
        }
        if !(radius.is_finite() && radius > 0.0) {
            return Err(EngineError::BadParams("the fillet size must be positive".into()));
        }
        let mut list = if edges.is_empty() { b.mesh.edge_ids.clone() } else { edges.clone() };
        list.sort_unstable();
        list.dedup();
        // Shapr3D's setback distance isn't on its card; twice the radius reads like its pictures.
        let shape = s.kernel.fillet_edges_ex(&b.shape, &list, radius, chordal, if setback { 2.0 * radius } else { 0.0 })?;
        set_body_shape(s, body, shape)?;
        s.selection.clear();
        return Ok(json!({ "body": body.0 }));
    }
    let shape = match (p.opt_str("continuity").unwrap_or("g1"), radius2) {
        (_, Some(_)) if shaped => return Err(EngineError::NotAvailable("a variable fillet keeps the curvature at 0".into())),
        (c @ ("g1" | "g2"), None) if shaped => fillet_profile(s, body, &edges, radius, c == "g2", curvature)?,
        ("g1", Some(_)) if edges.is_empty() => return Err(EngineError::NotAvailable("pick the edges for a variable fillet".into())),
        ("g1", Some(r2)) => s.kernel.fillet_edges_variable(&b.shape, &edges, radius, r2)?,
        ("g1", None) if edges.is_empty() => s.kernel.fillet_all_edges(&b.shape, radius)?,
        ("g1", None) => s.kernel.fillet_edges(&b.shape, &edges, radius)?,
        ("g2", None) => fillet_profile(s, body, &edges, radius, true, 0.0)?,
        ("g2", Some(_)) => return Err(EngineError::NotAvailable("G2 fillets have one radius".into())),
        (other, _) => return Err(EngineError::BadParams(format!("continuity is g1 or g2, not {other:?}"))),
    };
    set_body_shape(s, body, shape)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

/// A shaped round cut along each straight edge between two flat faces: the corner is cut away under a clamped
/// B-spline whose end poles lie where a circular fillet of `radius` would meet the faces.
/// G2 (curvature-continuous; the kernel's fillets are circular, tangent only): three poles on each face (zero
/// curvature where it meets them); with the inner poles at 0.8 and 0.6 of the setback it takes about as much as the
/// circle (on a square edge 0.211 r² against 0.215 r², its middle 0.306 r from the corner against 0.293 r).
/// G1 with a curvature: the cubic that best matches the circular arc, its inner poles then pulled toward the corner
/// (curvature > 0, sharper; 1 leaves the corner) or onto the chord (< 0, flatter; -1 is a bevel).
/// Other edges (curved, or between curved faces) and inside corners are refused.
fn fillet_profile(s: &Session, body: ItemId, edges: &[u32], radius: f64, g2: bool, curvature: f64) -> Result<Shape> {
    use shapecraft_kernel::{EdgeCurve, PlanarCurve, PlaneFrame};
    if !(radius.is_finite() && radius > 0.0) {
        return Err(EngineError::BadParams("the radius must be positive".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    if curvature >= 1.0 - 1e-9 {
        // Fully sharp: the corner stays.
        return Ok(b.shape.clone());
    }
    let mut picked: Vec<u32> = if edges.is_empty() { b.mesh.edge_ids.clone() } else { edges.to_vec() };
    picked.sort_unstable();
    picked.dedup();
    let what = if g2 { "G2 rounds" } else { "A fillet's curvature rounds" };
    let refuse = || EngineError::NotAvailable(format!("{what} straight outside edges between flat faces"));
    let mut out = b.shape.clone();
    for e in picked {
        let k = b.mesh.edge_ids.iter().position(|x| *x == e).ok_or_else(|| EngineError::BadParams(format!("no edge {e}")))?;
        let [fa, fb] = *b.mesh.edge_faces.get(k).ok_or_else(refuse)?;
        if fa == u32::MAX || fb == u32::MAX || fa == fb {
            return Err(refuse());
        }
        let EdgeCurve::Line { a: p, b: q } = s.kernel.edge_curve(&b.shape, e)? else { return Err(refuse()) };
        let (Some(pa), Some(pb)) = (s.kernel.face_plane(&b.shape, fa)?, s.kernel.face_plane(&b.shape, fb)?) else {
            return Err(refuse());
        };
        let len = p.distance(q);
        let t = (q - p) / len.max(1e-12);
        // Along each face, square to the edge, toward the face's middle.
        let along = |n: DVec3, mid: DVec3| {
            let d = n.cross(t).normalize_or_zero();
            if (mid - p).dot(d) < 0.0 { -d } else { d }
        };
        let (da, db) = (along(pa.normal, pa.origin), along(pb.normal, pb.origin));
        // Outside corners only: going along one face passes under the other.
        let theta = da.angle_between(db);
        if len < 1e-9 || da.dot(pb.normal) >= -1e-9 || !(1e-3..std::f64::consts::PI - 1e-3).contains(&theta) {
            return Err(refuse());
        }
        let setback = radius / (theta / 2.0).tan();
        let ext = len * 1e-3 + 1e-4;
        let o = p - t * ext;
        let fractions: Vec<(DVec3, f64)> = if g2 {
            vec![(da, 1.0), (da, 0.8), (da, 0.6), (db, 0.6), (db, 0.8), (db, 1.0)]
        } else {
            // The arc turns through π − θ; its cubic's handles are 4/3 tan(sweep/4) r long.
            let k0 = 4.0 / 3.0 * ((std::f64::consts::PI - theta) / 4.0).tan() * radius / setback;
            vec![(da, 1.0), (da, 1.0 - k0), (db, 1.0 - k0), (db, 1.0)]
        };
        let (ta, tb) = (o + da * setback, o + db * setback);
        let n = fractions.len();
        let poles: Vec<DVec3> = fractions
            .iter()
            .enumerate()
            .map(|(i, (d, f))| {
                let at = o + *d * setback * f;
                if i == 0 || i + 1 == n {
                    return at;
                }
                if curvature > 0.0 {
                    at.lerp(o, curvature)
                } else {
                    // Onto the chord between where the round meets the faces.
                    let c = tb - ta;
                    let flat = ta + c * ((at - ta).dot(c) / c.length_squared().max(1e-24)).clamp(0.0, 1.0);
                    at.lerp(flat, -curvature)
                }
            })
            .collect();
        let frame = PlaneFrame { origin: o, x: da, y: (db - da * db.dot(da)).normalize_or_zero() };
        // Closed through a point outside the corner: the curve leaves each face so gently that lines along the
        // faces would hug it and split nothing.
        let out_at = o - (da + db) * setback;
        let profile = if curvature <= -1.0 + 1e-9 { PlanarCurve::Line { a: ta, b: tb } } else { PlanarCurve::BSpline { poles } };
        let curves = [PlanarCurve::Line { a: out_at, b: ta }, profile, PlanarCurve::Line { a: tb, b: out_at }];
        let face = s.kernel.planar_regions(frame, &curves)?.into_iter().next().ok_or_else(refuse)?;
        let tool = s.kernel.prism(&face, t * (len + 2.0 * ext))?;
        out = s.kernel.boolean(BoolOp::Subtract, &out, &tool)?;
    }
    Ok(out)
}

/// Edges (or a whole body) to work on, from params or the selection.
pub(super) fn edge_target(s: &Session, p: &Params) -> Result<(ItemId, Vec<u32>)> {
    if p.has("body") {
        let body = p.id("body")?;
        let edges = if p.has("edges") { p.u32s("edges")? } else { Vec::new() };
        return Ok((body, edges));
    }
    let mut body = None;
    let mut edges = Vec::new();
    for pick in &s.selection {
        match *pick {
            Pick::Edge { body: b, edge } if body.is_none_or(|x| x == b) => {
                body = Some(b);
                edges.push(edge);
            }
            Pick::Body { body: b } if body.is_none() => body = Some(b),
            _ => {}
        }
    }
    Ok((body.ok_or_else(|| EngineError::NotAvailable("select edges first".into()))?, edges))
}

pub(super) fn chamfer(s: &mut Session, p: &Params) -> Result<Value> {
    let distance = p.f64("distance")?;
    let (body, mut edges) = edge_target(s, p)?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    if edges.is_empty() {
        edges = b.mesh.edge_ids.clone();
    }
    let shape = if p.has("distance2") {
        let d2 = p.f64("distance2")?;
        if !(d2.is_finite() && d2 > 0.0) {
            return Err(EngineError::BadParams("the second distance must be positive".into()));
        }
        // `face`: the face the first distance runs along (Two-Distance's first handle).
        let face = if p.has("face") { Some(p.parse::<u32>("face")?) } else { None };
        s.kernel.chamfer_edges_two(&b.shape, &edges, distance, d2, face)?
    } else {
        s.kernel.chamfer_edges(&b.shape, &edges, distance)?
    };
    set_body_shape(s, body, shape)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

pub(super) fn shell(s: &mut Session, p: &Params) -> Result<Value> {
    let thickness = p.f64("thickness")?;
    let (body, mut faces) = face_target(s, p, "select the faces to open")?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    // Every face picked (Shell on a whole body selects them all, as observed): nothing can open, so it's hollowed.
    faces.sort_unstable();
    faces.dedup();
    if faces.len() as u32 >= s.kernel.face_count(&b.shape)? {
        faces.clear();
    }
    let shape = s.kernel.shell(&b.shape, &faces, thickness)?;
    set_body_shape(s, body, shape)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

/// Faces of one body (possibly none, when a whole body is picked), from params or the selection.
fn face_target(s: &Session, p: &Params, missing: &str) -> Result<(ItemId, Vec<u32>)> {
    if p.has("body") {
        return Ok((p.id("body")?, if p.has("faces") { p.u32s("faces")? } else { Vec::new() }));
    }
    let mut body = None;
    let mut faces = Vec::new();
    for pick in &s.selection {
        match *pick {
            Pick::Face { body: b, face } if body.is_none_or(|x| x == b) => {
                body = Some(b);
                faces.push(face);
            }
            Pick::Body { body: b } if body.is_none() => body = Some(b),
            _ => {}
        }
    }
    Ok((body.ok_or_else(|| EngineError::NotAvailable(missing.into()))?, faces))
}

pub(super) fn offset_face(s: &mut Session, p: &Params) -> Result<Value> {
    let distance = p.f64("distance")?;
    let (body, faces) = face_target(s, p, "select the faces to offset")?;
    if faces.is_empty() {
        return Err(EngineError::NotAvailable("select the faces to offset".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    // Distance Type: "offset" moves the faces by the distance; "radius" makes round faces that radius.
    let distance = match p.opt_str("distance_type").unwrap_or("offset") {
        "offset" => distance,
        "radius" => {
            if distance <= 0.0 {
                return Err(EngineError::BadParams("the radius must be positive".into()));
            }
            let mut by: Option<f64> = None;
            for &f in &faces {
                let (r, outside) = round_face(s, b, f).ok_or_else(|| EngineError::NotAvailable("Radius works on round faces".into()))?;
                let d = if outside { distance - r } else { r - distance };
                if by.is_some_and(|x| (x - d).abs() > 1e-9) {
                    return Err(EngineError::NotAvailable("the round faces have different radii".into()));
                }
                by = Some(d);
            }
            by.unwrap_or(0.0)
        }
        other => return Err(EngineError::BadParams(format!("distance_type is offset or radius, not {other:?}"))),
    };
    if distance.abs() < 1e-12 {
        return Ok(json!({ "body": body.0 }));
    }
    let marks: Vec<Mark> = face_marks(b, &faces).into_iter().map(|(p, n)| (p + n * distance, n)).collect();
    let shape = s.kernel.offset_faces(&b.shape, &faces, distance)?;
    set_body_shape(s, body, shape)?;
    repick_faces(s, body, &marks);
    Ok(json!({ "body": body.0 }))
}

/// A point inside a face and its outward normal there.
type Mark = (DVec3, DVec3);

/// For each face, the middle of its biggest triangle (well inside the face) and the outward normal there.
fn face_marks(b: &shapecraft_doc::Body, faces: &[u32]) -> Vec<Mark> {
    let m = &b.mesh;
    let at = |i: u32| m.positions.get(i as usize).map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
    faces
        .iter()
        .filter_map(|&face| {
            let (_, mid, n) = m
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .zip(&m.triangle_faces)
                .filter(|(_, f)| **f == face)
                .filter_map(|(t, _)| {
                    let (a, b, c) = (at(t[0])?, at(t[1])?, at(t[2])?);
                    let cross = (b - a).cross(c - a);
                    Some((cross.length(), (a + b + c) / 3.0, cross.normalize_or_zero()))
                })
                .max_by(|x, y| x.0.total_cmp(&y.0))?;
            Some((mid, n))
        })
        .collect()
}

/// Picks the faces of `body` lying at `marks` (each moved to where its face went), so a face that was pushed,
/// pulled or turned stays picked where it is now, whatever number the kernel gave it. Nothing stays picked when a
/// face can't be found (it was swallowed, or merged away).
pub(super) fn repick_faces(s: &mut Session, body: ItemId, marks: &[Mark]) {
    let found = s.doc.body(body).and_then(|b| {
        let m = &b.mesh;
        let (lo, hi) = m.bounds()?;
        let size = DVec3::from(hi.map(f64::from)).distance(DVec3::from(lo.map(f64::from)));
        let tol = (2e-3 * size).max(0.02);
        let at = |i: u32| m.positions.get(i as usize).map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
        let mut faces: Vec<u32> = Vec::new();
        for &(p, n) in marks {
            let best = m
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .zip(&m.triangle_faces)
                .filter_map(|(t, &f)| {
                    let (a, b, c) = (at(t[0])?, at(t[1])?, at(t[2])?);
                    ((b - a).cross(c - a).normalize_or_zero().dot(n) > 0.7).then(|| (closest_on_triangle(p, a, b, c).distance(p), f))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .filter(|(d, _)| *d < tol)?;
            if !faces.contains(&best.1) {
                faces.push(best.1);
            }
        }
        Some(faces)
    });
    s.selection = found.unwrap_or_default().into_iter().map(|face| Pick::Face { body, face }).collect();
}

/// The point of triangle abc nearest to p (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-300 {
        return a;
    }
    a + ab * (vb / denom) + ac * (vc / denom)
}

/// A cylinder-like face's radius (from the circles and arcs around it) and whether it bulges outwards (a boss,
/// not a hole).
fn round_face(s: &Session, b: &shapecraft_doc::Body, face: u32) -> Option<(f64, bool)> {
    let axis = s.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Face(face)).ok()??;
    let dir = axis.dir.normalize_or_zero();
    let radius = b.mesh.edge_ids.iter().zip(&b.mesh.edge_faces).filter(|(_, fs)| fs.contains(&face)).find_map(|(&e, _)| {
        match s.kernel.edge_curve(&b.shape, e).ok()? {
            shapecraft_kernel::EdgeCurve::Circle { radius, .. } => Some(radius),
            shapecraft_kernel::EdgeCurve::Arc { a, mid, b } => {
                let (ab, am) = (b - a, mid - a);
                let n = ab.cross(am);
                (n.length_squared() > 1e-18).then(|| ab.length() * am.length() * (b - mid).length() / (2.0 * n.length()))
            }
            _ => None,
        }
    })?;
    // A triangle of the face: its normal points away from the axis on a boss, towards it in a hole.
    let t = b.mesh.triangle_faces.iter().position(|&f| f == face)?;
    let i = b.mesh.indices[t * 3] as usize;
    let (pos, nrm) = (glam::DVec3::from(b.mesh.positions[i].map(f64::from)), glam::DVec3::from(b.mesh.normals[i].map(f64::from)));
    let off = pos - axis.origin;
    let radial = off - dir * off.dot(dir);
    Some((radius, radial.dot(nrm) >= 0.0))
}

/// Replace Face: each face ends up on the target plane. A parallel target is a plain face offset (the
/// neighbours stretch to follow); a tilted one adds the face's prism up to the plane, or cuts away what lies
/// past it. A target across the face's own direction can't be met.
pub(super) fn replace_face(s: &mut Session, p: &Params) -> Result<Value> {
    const CANT: &str = "The selected face can't be replaced.";
    let to: Value = p.parse("to")?;
    let (o_t, n_t) = face_frame(s, &to)?;
    let (body, faces) = face_target(s, p, "select the faces to replace")?;
    if faces.is_empty() {
        return Err(EngineError::NotAvailable("select the faces to replace".into()));
    }
    if Params::new(&to).has("body") && Params::new(&to).id("body")? == body && faces.contains(&(Params::new(&to).f64("face")? as u32)) {
        return Err(EngineError::NotAvailable(CANT.into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let t = n_t.normalize_or_zero();
    let marks: Vec<Mark> =
        face_marks(b, &faces).into_iter().map(|(p, n)| (p - t * (p - o_t).dot(t), if n.dot(t) >= 0.0 { t } else { -t })).collect();
    let shape = fit_faces_to_plane(s, &b.shape.clone(), &faces, o_t, n_t)?.ok_or_else(|| EngineError::NotAvailable(CANT.into()))?;
    set_body_shape(s, body, shape)?;
    repick_faces(s, body, &marks);
    Ok(json!({ "body": body.0 }))
}

/// The body with its flat `faces` brought onto the plane through `o_t` with normal `n_t`, neighbours stretching or
/// trimming to meet it (Replace Face, and faces turned about an axis in their own plane). `None` when a face isn't
/// flat or stands square to the plane.
fn fit_faces_to_plane(s: &Session, shape: &Shape, faces: &[u32], o_t: DVec3, n_t: DVec3) -> Result<Option<Shape>> {
    let t = n_t.normalize_or_zero();
    let mut planes = Vec::new();
    for &f in faces {
        let Some(fp) = s.kernel.face_plane(shape, f)? else { return Ok(None) };
        let n = fp.normal.normalize_or_zero();
        if n.dot(t).abs() < 0.05 {
            return Ok(None);
        }
        planes.push((f, fp.origin, n));
    }
    let Some(first) = planes.first().copied() else { return Ok(None) };
    // All parallel and the same distance away: one offset keeps fillets and stretches neighbours.
    let dist = |o: DVec3, n: DVec3| (o_t - o).dot(n);
    let parallel = planes.iter().all(|&(_, _, n)| n.dot(t).abs() > 1.0 - 1e-9);
    let d0 = dist(first.1, first.2);
    if parallel && planes.iter().all(|&(_, o, n)| (dist(o, n) - d0).abs() < 1e-9) {
        return Ok(Some(if d0.abs() < 1e-9 { shape.clone() } else { s.kernel.offset_faces(shape, faces, d0)? }));
    }
    let bb = s.kernel.bounds(shape)?;
    let reach = (bb.max - bb.min).length() + (o_t - bb.min).length() + (o_t - bb.max).length() + 1.0;
    let profiles = faces.iter().map(|&f| s.kernel.face(shape, f)).collect::<std::result::Result<Vec<_>, _>>()?;
    let mut out = shape.clone();
    for (profile, &(_, _, n)) in profiles.iter().zip(&planes) {
        // The target's normal turned toward the face's outside: "below" the plane is the body's side.
        let up = if n.dot(t) > 0.0 { t } else { -t };
        let below = |c: DVec3| (c - o_t).dot(up) < 0.0;
        // A prism the plane misses stays whole.
        let halves = |prism: Shape| s.kernel.split_by_plane(&prism, o_t, up).unwrap_or_else(|_| vec![prism]);
        for piece in halves(s.kernel.prism(profile, n * reach)?) {
            if below(s.kernel.mass_props(&piece)?.centroid) {
                out = s.kernel.boolean(BoolOp::Union, &out, &piece)?;
            }
        }
        for piece in halves(s.kernel.prism(profile, -n * reach)?) {
            if !below(s.kernel.mass_props(&piece)?.centroid) {
                out = s.kernel.boolean(BoolOp::Subtract, &out, &piece)?;
            }
        }
    }
    Ok(Some(out))
}

/// Project as Edge: imprints the items' outlines on a face, projected along its normal (26.200, s11-134..138).
pub(super) fn project_edge(s: &mut Session, p: &Params) -> Result<Value> {
    let to: Value = p.parse("to")?;
    let to = Params::new(&to);
    let (body, face) = (to.id("body")?, to.f64("face")? as u32);
    let items: Vec<Value> = if p.has("items") {
        p.parse("items")?
    } else {
        s.selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::Edge { body, edge } => Some(json!({ "body": body.0, "edge": edge })),
                Pick::Face { body: b, face: f } if (b, f) != (body, face) => Some(json!({ "body": b.0, "face": f })),
                Pick::Region { sketch, region } => Some(json!({ "sketch": sketch.0, "region": region })),
                _ => None,
            })
            .collect()
    };
    if items.is_empty() {
        return Err(EngineError::NotAvailable("select edges, faces or sketch regions to project".into()));
    }
    if items.len() > 10_000 {
        return Err(EngineError::BadParams("too many items".into()));
    }
    let mut tools = Vec::new();
    for v in &items {
        let it = Params::new(v);
        if it.has("sketch") {
            tools.push((profile_face(s, Profile::Region { sketch: it.id("sketch")?, region: it.f64("region")? as u32 })?, None));
            continue;
        }
        let b = it.id("body")?;
        let shape = s.doc.body(b).ok_or_else(|| EngineError::BadParams(format!("no body {}", b.0)))?.shape.clone();
        if it.has("edge") {
            tools.push((shape, Some(it.f64("edge")? as u32)));
        } else {
            tools.push((s.kernel.face(&shape, it.f64("face")? as u32)?, None));
        }
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let shape = s.kernel.imprint(&b.shape, face, &tools)?;
    set_body_shape(s, body, shape)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

/// Moves a straight edge of a body: each flat face beside it turns about its far side to meet the moved edge (a face
/// the move stays within just stretches). Refused for round edges or faces that aren't flat.
pub(super) fn move_edge(s: &mut Session, p: &Params) -> Result<Value> {
    const CANT: &str = "this edge can't be moved";
    let (body, edge) = if p.has("edge") {
        (p.id("body")?, p.f64("edge")? as u32)
    } else {
        match s.selection.as_slice() {
            [Pick::Edge { body, edge }] => (*body, *edge),
            _ => return Err(EngineError::NotAvailable("select one edge to move".into())),
        }
    };
    let by = p.vec3("by")?;
    if !by.is_finite() {
        return Err(EngineError::BadParams("by must be finite".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    if by.length() < 1e-12 {
        return Ok(json!({ "body": body.0 }));
    }
    let m = b.mesh.clone();
    let k =
        m.edge_ids.iter().position(|e| *e == edge).ok_or_else(|| EngineError::BadParams(format!("body {} has no edge {edge}", body.0)))?;
    let pts: Vec<DVec3> =
        m.edges.get(k).map(|l| l.iter().map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)).collect()).unwrap_or_default();
    let (Some(&a), Some(&e)) = (pts.first(), pts.last()) else { return Err(EngineError::NotAvailable(CANT.into())) };
    // A closed edge (a rim): it moves with the flat face it bounds, and the face beside it follows.
    if pts.len() > 2 && a.distance(e) < 1e-6 * (1.0 + pts.iter().map(|q| q.distance(a)).fold(0.0, f64::max)) {
        let moved = s.kernel.map_edge(&b.shape, edge, glam::DMat3::IDENTITY, by)?;
        set_body_shape(s, body, moved)?;
        s.selection.clear();
        return Ok(json!({ "body": body.0 }));
    }
    let dir = (e - a).normalize_or_zero();
    // Straight: every point on the line through its ends.
    if dir.length() < 0.5 || pts.iter().any(|q| (*q - a).cross(dir).length() > 1e-6 * (1.0 + (e - a).length())) {
        return Err(EngineError::NotAvailable(CANT.into()));
    }
    let faces: Vec<u32> = m.edge_faces.get(k).map(|f| f.iter().copied().filter(|f| *f != u32::MAX).collect()).unwrap_or_default();
    let mut shape = b.shape.clone();
    let count = s.kernel.face_count(&shape)?;
    let moved = a + by;
    for f in faces {
        let fp = s.kernel.face_plane(&shape, f)?.ok_or_else(|| EngineError::NotAvailable(CANT.into()))?;
        let n = fp.normal.normalize_or_zero();
        if by.dot(n).abs() < 1e-9 {
            continue;
        }
        // The face's far side: its mesh point farthest from the edge's line stays where it is.
        let far = m
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .filter(|(t, _)| m.triangle_faces.get(*t) == Some(&f))
            .flat_map(|(_, tri)| {
                tri.iter().filter_map(|i| m.positions.get(*i as usize)).map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64))
            })
            .max_by(|p, q| (*p - a).cross(dir).length().total_cmp(&(*q - a).cross(dir).length()))
            .ok_or_else(|| EngineError::NotAvailable(CANT.into()))?;
        let mut normal = dir.cross(far - moved).normalize_or_zero();
        if normal.length() < 0.5 {
            return Err(EngineError::NotAvailable(CANT.into()));
        }
        if normal.dot(n) < 0.0 {
            normal = -normal;
        }
        shape = fit_faces_to_plane(s, &shape, &[f], moved, normal)?.ok_or_else(|| EngineError::NotAvailable(CANT.into()))?;
    }
    let same = s.kernel.face_count(&shape).is_ok_and(|n| n == count);
    set_body_shape(s, body, shape)?;
    // The moved edge keeps its pick: the new edge whose ends sit where the old ones went (ids can change).
    let (a2, e2) = (a + by, e + by);
    let found = s.doc.body(body).and_then(|b| {
        let near = |q: &[f32; 3], p: DVec3| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64).distance(p) < 1e-4 * (1.0 + (e - a).length());
        b.mesh.edges.iter().zip(&b.mesh.edge_ids).find_map(|(l, id)| {
            let (f, z) = (l.first()?, l.last()?);
            ((near(f, a2) && near(z, e2)) || (near(f, e2) && near(z, a2))).then_some(*id)
        })
    });
    s.selection = match found {
        Some(edge) if same => vec![Pick::Edge { body, edge }],
        _ => Vec::new(),
    };
    Ok(json!({ "body": body.0 }))
}

/// Moves or turns some of a body's faces, the way Move/Rotate does with faces picked: the rest of the body
/// follows. Flat faces slid along their own normal are offset; anything else is cut out of the body as a feature
/// (the body with those faces removed, against the body as it is) and put back moved.
pub(super) fn move_faces(s: &mut Session, p: &Params) -> Result<Value> {
    let (body, faces) = face_target(s, p, "select the faces to move")?;
    if faces.is_empty() {
        return Err(EngineError::NotAvailable("select the faces to move".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let count = s.kernel.face_count(&b.shape)?;
    if let Some(f) = faces.iter().find(|f| **f >= count) {
        return Err(EngineError::BadParams(format!("body {} has no face {f}", body.0)));
    }
    let shape = b.shape.clone();
    let by = if p.has("by") { p.vec3("by")? } else { DVec3::ZERO };
    let turn = if p.has("angle") {
        let (origin, dir, angle) = (p.vec3("origin")?, p.vec3("axis")?, p.f64("angle")?);
        if !angle.is_finite() || dir.length() < 1e-12 {
            return Err(EngineError::BadParams("rotate needs an axis and a finite angle".into()));
        }
        (angle.abs() > 1e-12).then(|| (shapecraft_kernel::Axis::new(origin, dir.normalize()), angle.to_radians()))
    } else {
        None
    };
    if !by.is_finite() {
        return Err(EngineError::BadParams("by must be finite".into()));
    }
    if by.length() < 1e-12 && turn.is_none() {
        return Ok(json!({ "body": body.0 }));
    }
    // Where each face goes: turned, then slid.
    let spin = |v: DVec3, (axis, angle): (shapecraft_kernel::Axis, f64)| {
        let d = axis.dir.normalize_or_zero();
        let (sin, cos) = angle.sin_cos();
        v * cos + d.cross(v) * sin + d * d.dot(v) * (1.0 - cos)
    };
    let marks: Vec<Mark> = face_marks(b, &faces)
        .into_iter()
        .map(|(p, n)| match turn {
            Some(t) => (t.0.origin + spin(p - t.0.origin, t) + by, spin(n, t)),
            None => (p + by, n),
        })
        .collect();
    // Flat faces on one plane turned about an axis lying in that plane (a face about one of its own edges): the
    // faces tilt onto the turned plane, neighbours following.
    if let Some((axis, angle)) = turn
        && by.length() < 1e-12
    {
        let mut plane = None;
        let mut coplanar = true;
        for &f in &faces {
            match (s.kernel.face_plane(&shape, f)?, plane) {
                (Some(fp), None) => plane = Some((fp.origin, fp.normal.normalize_or_zero())),
                (Some(fp), Some((o, n))) if fp.normal.normalize_or_zero().dot(n) > 1.0 - 1e-9 && (fp.origin - o).dot(n).abs() < 1e-6 => {}
                _ => coplanar = false,
            }
        }
        let d = axis.dir.normalize_or_zero();
        if let Some((o, n)) = plane.filter(|_| coplanar)
            && d.dot(n).abs() < 1e-6
            && (axis.origin - o).dot(n).abs() < 1e-6
        {
            // Rodrigues: n turned by `angle` about d.
            let (sin, cos) = angle.sin_cos();
            let turned = n * cos + d.cross(n) * sin + d * d.dot(n) * (1.0 - cos);
            if let Ok(Some(moved)) = fit_faces_to_plane(s, &shape, &faces, axis.origin, turned) {
                return moved_faces(s, body, &marks, moved);
            }
        }
    }
    // Flat faces sharing one normal, slid along it: an offset keeps every neighbour attached.
    if turn.is_none() {
        let mut normal = None;
        let mut flat = true;
        for &f in &faces {
            match s.kernel.face_plane(&shape, f)? {
                Some(fp) if normal.is_none_or(|n: DVec3| n.dot(fp.normal) > 1.0 - 1e-9) => normal = Some(fp.normal),
                _ => flat = false,
            }
        }
        if let Some(n) = normal.filter(|_| flat)
            && by.normalize().dot(n).abs() > 1.0 - 1e-9
            && let Ok(moved) = s.kernel.offset_faces(&shape, &faces, by.dot(n))
        {
            return moved_faces(s, body, &marks, moved);
        }
    }
    let place = |s: &Session, piece: &Shape| -> Result<Shape> {
        let mut out = piece.clone();
        if let Some((axis, angle)) = turn {
            out = s.kernel.rotate(&out, axis, angle)?;
        }
        if by.length() > 1e-12 {
            out = s.kernel.translate(&out, by)?;
        }
        Ok(out)
    };
    let without = s
        .kernel
        .remove_faces(&shape, &faces)
        .map_err(|_| EngineError::NotAvailable("these faces can't be moved on their own; pick the whole feature".into()))?;
    let volume = |s: &Session, sh: &Shape| s.kernel.mass_props(sh).map(|m| m.volume).unwrap_or(0.0);
    let tiny = 1e-9 * volume(s, &shape).abs().max(1.0);
    let cut = s.kernel.boolean(BoolOp::Subtract, &without, &shape).ok().filter(|c| volume(s, c) > tiny);
    let added = s.kernel.boolean(BoolOp::Subtract, &shape, &without).ok().filter(|a| volume(s, a) > tiny);
    if cut.is_none() && added.is_none() {
        return Err(EngineError::NotAvailable("these faces can't be moved on their own; pick the whole feature".into()));
    }
    let mut out = without;
    if let Some(c) = &cut {
        out = s.kernel.boolean(BoolOp::Subtract, &out, &place(s, c)?)?;
    }
    if let Some(a) = &added {
        out = s.kernel.boolean(BoolOp::Union, &out, &place(s, a)?)?;
    }
    if volume(s, &out) <= tiny {
        return Err(EngineError::NotAvailable("moving these faces leaves nothing of the body".into()));
    }
    moved_faces(s, body, &marks, out)
}

/// Puts the moved body in and keeps the moved faces picked where they went (so the gizmo's next drag works on
/// them, and the highlight follows the faces).
fn moved_faces(s: &mut Session, body: ItemId, marks: &[Mark], shape: Shape) -> Result<Value> {
    set_body_shape(s, body, shape)?;
    repick_faces(s, body, marks);
    Ok(json!({ "body": body.0 }))
}

//! Lofts with guide curves, tangent ends or a closed ring: the profiles' edges and what lies beside them, read from the
//! kernel for [`shapecraft_kernel::skin`].

use glam::DVec3;
use shapecraft_kernel::skin::{self, End, Options};
use shapecraft_kernel::{EdgeCurve, PlanarCurve, PlaneFrame, Shape};

use super::sweep::{PathPart, Profile, along, along_n, entity_curves};
use crate::{EngineError, Result, Session};

/// Tangent Continuity at an end of a loft.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoftEnd {
    pub curvature: bool,
    pub magnitude: f64,
}

pub(super) struct LoftOptions {
    pub periodic: bool,
    pub start: Option<LoftEnd>,
    pub end: Option<LoftEnd>,
}

/// Path pieces as point runs, joined end to end into chains.
pub(super) fn chains(s: &Session, parts: &[PathPart]) -> Result<Vec<Vec<DVec3>>> {
    let mut runs: Vec<Vec<DVec3>> = Vec::new();
    for part in parts {
        match *part {
            PathPart::Curve { sketch, entity } => {
                let (frame, curves) = entity_curves(s, sketch, entity)?;
                runs.extend(curves.iter().map(|c| drawn(frame, c)));
            }
            PathPart::Edge { body, edge } => {
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                runs.push(fine(&s.kernel.edge_curve(&b.shape, edge)?));
            }
        }
    }
    let mut out: Vec<Vec<DVec3>> = Vec::new();
    while let Some(mut chain) = runs.pop() {
        loop {
            let (Some(a), Some(z)) = (chain.first().copied(), chain.last().copied()) else { break };
            let hit = runs.iter().enumerate().find_map(|(i, r)| {
                let (p, q) = (*r.first()?, *r.last()?);
                [(p.distance(z), 0), (q.distance(z), 1), (q.distance(a), 2), (p.distance(a), 3)]
                    .into_iter()
                    .find(|x| x.0 < 1e-4)
                    .map(|x| (i, x.1))
            });
            let Some((i, how)) = hit else { break };
            let mut r = runs.remove(i);
            match how {
                0 => chain.extend(r.drain(1..)),
                1 => chain.extend(r.into_iter().rev().skip(1)),
                2 => {
                    r.extend(chain.drain(1..));
                    chain = r;
                }
                _ => {
                    r.reverse();
                    r.extend(chain.drain(1..));
                    chain = r;
                }
            }
        }
        out.push(chain);
    }
    Ok(out)
}

/// Close points along a curve (a fit through a spline's samples).
pub(super) fn fine(c: &EdgeCurve) -> Vec<DVec3> {
    match c {
        EdgeCurve::Other { points } => {
            let n = points.len();
            let params = (0..n).map(|i| i as f64 / (n.max(2) - 1) as f64).collect();
            match shapecraft_kernel::spline::fit_with(points, params) {
                Some(f) => (0..=512).map(|i| f.point(f64::from(i) / 512.0)).collect(),
                None => points.clone(),
            }
        }
        _ => along_n(c, 512),
    }
}

/// Close points along a sketch curve.
fn drawn(frame: PlaneFrame, c: &PlanarCurve) -> Vec<DVec3> {
    const N: u32 = 512;
    let n = frame.x.cross(frame.y).normalize_or(DVec3::Z);
    let round = |center: DVec3, x: DVec3, y: DVec3| -> Vec<DVec3> {
        (0..=N).map(|i| std::f64::consts::TAU * f64::from(i) / f64::from(N)).map(|t| center + x * t.cos() + y * t.sin()).collect()
    };
    match c {
        PlanarCurve::Line { a, b } => vec![*a, *b],
        PlanarCurve::Arc { a, mid, b } => along_n(&EdgeCurve::Arc { a: *a, mid: *mid, b: *b }, N as usize),
        PlanarCurve::Circle { center, radius } => {
            round(*center, frame.x.normalize_or(DVec3::X) * *radius, n.cross(frame.x.normalize_or(DVec3::X)) * *radius)
        }
        PlanarCurve::Ellipse { center, major, rx, ry } => round(*center, *major * *rx, n.cross(*major) * *ry),
        PlanarCurve::Spline { points } => match shapecraft_kernel::spline::fit(points) {
            Some(f) => (0..=N).map(|i| f.point(f64::from(i) / f64::from(N))).collect(),
            None => points.clone(),
        },
        PlanarCurve::BSpline { poles } => {
            let p = (poles.len().max(2) - 1).min(3);
            let spans = poles.len().saturating_sub(p).max(1);
            let mut full = vec![0.0; p];
            full.extend((0..=spans).map(|k| k as f64));
            full.extend(vec![spans as f64; p]);
            (0..=N)
                .map(|i| spans as f64 * f64::from(i) / f64::from(N))
                .map(|t| shapecraft_kernel::nurbs::basis_all(&full, p, t).iter().zip(poles).map(|(b, q)| *q * *b).sum())
                .collect()
        }
    }
}

/// How walls leave a face profile: on from the faces beside it. Their shape is asked for at the points wanted.
struct Beside {
    normal: DVec3,
    /// Samples of each edge with the face across it: (points, the face's number).
    edges: Vec<(Vec<DVec3>, u32)>,
    shape: Shape,
}

pub(super) fn loft(s: &Session, profiles: &[Profile], faces: &[Shape], guides: &[PathPart], how: &LoftOptions) -> Result<Shape> {
    let mut sections = Vec::new();
    for f in faces {
        let edges = s.kernel.face_edges(f, 0)?;
        let curves = edges.iter().map(|&e| s.kernel.edge_curve(f, e)).collect::<std::result::Result<Vec<_>, _>>()?;
        sections
            .push(skin::outer_loop(&curves).ok_or_else(|| EngineError::NotAvailable("a loft profile isn't one closed outline".into()))?);
    }
    let mid = |f: &Shape| -> Result<DVec3> { Ok(s.kernel.measure(f, shapecraft_kernel::SubShape::Face(0))?.1) };
    let n = faces.len();
    // Which way each end faces the loft.
    let inward = |i: usize, next: usize| -> Result<DVec3> {
        let plane =
            s.kernel.face_plane(&faces[i], 0)?.ok_or_else(|| EngineError::NotAvailable("only flat faces can be profiles".into()))?;
        let to = mid(&faces[next])? - mid(&faces[i])?;
        let normal = plane.normal.normalize_or(DVec3::Z);
        Ok(if normal.dot(to) < 0.0 { -normal } else { normal })
    };
    let beside = |i: usize, next: usize| -> Result<Beside> {
        let normal = inward(i, next)?;
        let mut edges = Vec::new();
        let mut shape = faces[i].clone();
        if let Profile::Face { body, face } = profiles[i]
            && let Some(b) = s.doc.body(body)
        {
            shape = b.shape.clone();
            for e in s.kernel.face_edges(&b.shape, face)? {
                let others = b.mesh.edge_ids.iter().position(|x| *x == e).and_then(|k| b.mesh.edge_faces.get(k));
                let other = others.and_then(|fs| fs.iter().copied().find(|f| *f != face));
                if let Some(other) = other {
                    edges.push((along(&s.kernel.edge_curve(&b.shape, e)?), other));
                }
            }
        }
        Ok(Beside { normal, edges, shape })
    };
    let ends = [(how.start, 0, 1), (how.end, n - 1, n.saturating_sub(2))];
    let mut sides: Vec<Option<Beside>> = Vec::new();
    for (e, i, next) in ends {
        sides.push(if e.is_some() && !how.periodic { Some(beside(i, next)?) } else { None });
    }
    let leave_of = |b: &Beside, p: DVec3, scale_curve: bool| -> (DVec3, DVec3) {
        let mut best: Option<(f64, DVec3, u32)> = None;
        for (pts, f) in &b.edges {
            for w in pts.windows(2) {
                let d = w[1] - w[0];
                let t = if d.length_squared() > 0.0 { ((p - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                let off = w[0].lerp(w[1], t).distance(p);
                if best.is_none_or(|x| off < x.0) {
                    best = Some((off, d.normalize_or_zero(), *f));
                }
            }
        }
        let Some((_, tangent, f)) = best else { return (b.normal, DVec3::ZERO) };
        let Ok(at) = s.kernel.face_at(&b.shape, f, &[p], &[b.normal]) else { return (b.normal, DVec3::ZERO) };
        let face_normal = at[0].0;
        let mut dir = tangent.cross(face_normal).normalize_or_zero();
        if dir.dot(b.normal) < 0.0 {
            dir = -dir;
        }
        if dir == DVec3::ZERO {
            return (b.normal, DVec3::ZERO);
        }
        let curve = if scale_curve { s.kernel.face_at(&b.shape, f, &[p], &[dir]).map_or(0.0, |c| c[0].1) } else { 0.0 };
        (dir, face_normal * curve)
    };
    let start = |p: DVec3| sides[0].as_ref().map_or((DVec3::Z, DVec3::ZERO), |b| leave_of(b, p, true));
    let finish = |p: DVec3| sides[1].as_ref().map_or((DVec3::Z, DVec3::ZERO), |b| leave_of(b, p, true));
    let end =
        |e: Option<LoftEnd>, leave| e.filter(|_| !how.periodic).map(|e| End { leave, curvature: e.curvature, magnitude: e.magnitude });
    let opts = Options { guides: chains(s, guides)?, periodic: how.periodic, start: end(how.start, &start), end: end(how.end, &finish) };
    let brep = skin::solid(sections, &opts).map_err(|e| EngineError::NotAvailable(e.into()))?;
    let shape = s.kernel.from_brep(&brep)?;
    let m = s.kernel.mass_props(&shape)?;
    if !(m.volume.is_finite() && m.volume > 1e-12) {
        return Err(EngineError::NotAvailable("the loft crosses itself with these settings".into()));
    }
    // A wall that folds over itself measures right but doesn't draw right: the triangles must agree.
    let drawn = s.kernel.mass_props_mesh(&shape).map_or(m.volume, |x| x.volume);
    if (drawn - m.volume).abs() > m.volume * 0.05 {
        return Err(EngineError::NotAvailable("the loft crosses itself with these settings".into()));
    }
    Ok(shape)
}

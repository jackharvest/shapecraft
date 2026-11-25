//! Fill: a solid spanning boundary profiles and guide curves. The profiles' outlines and the curves, cut where they meet,
//! make a network; every loop of it that isn't a profile gets a smooth face, and each edge says how the faces at it meet
//! what's beside it (G0 at the edge, G1 tangent, G2 with its curvature).

use std::collections::HashMap;

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::Pick;
use shapecraft_kernel::patch::{self, Bend, Rim};
use shapecraft_kernel::skin::Run;
use shapecraft_kernel::{BoolOp, Shape};

use super::extrude::{place_result, result_param};
use super::faces::edge_target;
use super::set_body_shape;
use super::skin::fine;
use super::sweep::{PathPart, Profile, entity_curves, path_part, profile_param};
use crate::{EngineError, Params, Result, Session};

const LOOSE: &str = "the profiles and curves don't make a closed network yet";

/// One stretch of the network between meeting points: the edges it's made of, its middle, and how faces meet there.
#[derive(Clone, Debug)]
pub struct Side {
    edges: Vec<u32>,
    pub mid: DVec3,
    pub order: u8,
    /// Between two profiles (nothing to choose).
    pub fixed: bool,
}

struct Net {
    shape: Shape,
    faces: u32,
    /// Each edge: its points, its two ends as vertex numbers, the profiles it borders.
    edges: Vec<(Vec<DVec3>, usize, usize, Vec<u32>)>,
    vertices: Vec<DVec3>,
    /// Each opening: its edges.
    cells: Vec<Vec<u32>>,
    sides: Vec<Side>,
}

fn order_of(name: &str) -> Result<u8> {
    match name {
        "g0" => Ok(0),
        "g1" => Ok(1),
        "g2" => Ok(2),
        o => Err(EngineError::BadParams(format!("continuity is g0, g1 or g2, not '{o}'"))),
    }
}

fn picks(s: &Session, p: &Params) -> Result<(Vec<Profile>, Vec<PathPart>)> {
    let profiles = if p.has("profiles") {
        p.parse::<Vec<Value>>("profiles")?.iter().map(profile_param).collect::<Result<Vec<_>>>()?
    } else if p.has("guides") {
        Vec::new()
    } else {
        s.selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::Region { sketch, region } => Some(Profile::Region { sketch, region }),
                Pick::Face { body, face } => Some(Profile::Face { body, face }),
                _ => None,
            })
            .collect()
    };
    let guides = if p.has("guides") {
        match p.parse::<Value>("guides")? {
            Value::Array(list) => list.iter().map(|v| path_part(s, v)).collect::<Result<Vec<_>>>()?.concat(),
            v => path_part(s, &v)?,
        }
    } else if p.has("profiles") {
        Vec::new()
    } else {
        s.selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::SketchEntity { sketch, entity } => Some(PathPart::Curve { sketch, entity }),
                _ => None,
            })
            .collect()
    };
    if profiles.len() + guides.len() > 200 {
        return Err(EngineError::BadParams("too many profiles and curves".into()));
    }
    Ok((profiles, guides))
}

/// A profile's face: unlike Loft's, it may be curved.
fn face_shape(s: &Session, pr: Profile) -> Result<Shape> {
    match pr {
        Profile::Region { sketch, region } => {
            let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
            Ok(sk.region(region).ok_or_else(|| EngineError::BadParams(format!("no region {region}")))?.face.clone())
        }
        Profile::Face { body, face } => {
            let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
            Ok(s.kernel.face(&b.shape, face)?)
        }
    }
}

fn network(s: &Session, p: &Params) -> Result<(Net, Vec<Profile>, Vec<PathPart>)> {
    let (profiles, guides) = picks(s, p)?;
    if profiles.is_empty() && guides.is_empty() {
        return Err(EngineError::NotAvailable("select profiles and curves around what to fill".into()));
    }
    let faces = profiles.iter().map(|pr| face_shape(s, *pr)).collect::<Result<Vec<_>>>()?;
    let mut curves = Vec::new();
    for g in &guides {
        match *g {
            PathPart::Curve { sketch, entity } => {
                let (frame, list) = entity_curves(s, sketch, entity)?;
                if !list.is_empty() {
                    curves.push(s.kernel.curves_shape(frame, &list)?);
                }
            }
            PathPart::Edge { body, edge } => {
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                curves.push(s.kernel.edge(&b.shape, edge)?);
            }
        }
    }
    let shape = s.kernel.network(&faces, &curves)?;
    let nf = s.kernel.face_count(&shape)?;
    if nf as usize != profiles.len() {
        return Err(EngineError::NotAvailable("a curve runs across one of the profiles".into()));
    }
    let mut at: Vec<Vec<u32>> = vec![Vec::new(); s.kernel.edge_count(&shape)? as usize];
    for f in 0..nf {
        for e in s.kernel.face_edges(&shape, f)? {
            if let Some(list) = at.get_mut(e as usize)
                && !list.contains(&f)
            {
                list.push(f);
            }
        }
    }
    let mut vertices: Vec<DVec3> = Vec::new();
    let mut vertex = |q: DVec3| -> usize {
        if let Some(i) = vertices.iter().position(|v| v.distance(q) < 1e-3) {
            return i;
        }
        vertices.push(q);
        vertices.len() - 1
    };
    let mut edges = Vec::new();
    for (e, borders) in at.into_iter().enumerate() {
        let pts = fine(&s.kernel.edge_curve(&shape, e as u32)?);
        let (a, z) = (pts.first().copied().unwrap_or_default(), pts.last().copied().unwrap_or_default());
        edges.push((pts, vertex(a), vertex(z), borders));
    }
    let cells = openings(&edges)?;
    let mut sides = stretches(&edges, &vertices);
    let all = order_of(p.opt_str("continuity").unwrap_or("g0"))?;
    for side in &mut sides {
        side.order = if side.fixed { 0 } else { all };
    }
    if p.has("sides") {
        for v in p.parse::<Vec<Value>>("sides")? {
            let sp = Params::new(&v);
            let (q, order) = (sp.vec3("at")?, order_of(sp.opt_str("continuity").unwrap_or("g0"))?);
            if let Some(side) = sides.iter_mut().filter(|x| !x.fixed).min_by(|a, b| a.mid.distance(q).total_cmp(&b.mid.distance(q))) {
                side.order = order;
            }
        }
    }
    Ok((Net { shape, faces: nf, edges, vertices, cells, sides }, profiles, guides))
}

/// Where Fill's continuity badges go, for these params.
pub fn fill_sides(s: &Session, params: &Value) -> Result<Vec<Side>> {
    let p = Params::with_variables(params, &s.doc.variables);
    Ok(network(s, &p)?.0.sides)
}

type NetEdge = (Vec<DVec3>, usize, usize, Vec<u32>);

/// Every loop of the network that isn't a profile, so each edge ends up with a face on both sides: shortest loops first.
fn openings(edges: &[NetEdge]) -> Result<Vec<Vec<u32>>> {
    let loose = || EngineError::NotAvailable(LOOSE.into());
    let mut need: Vec<i32> = edges.iter().map(|e| 2 - e.3.len() as i32).collect();
    if need.iter().any(|n| *n < 0) {
        return Err(EngineError::NotAvailable("three profiles meet at one edge".into()));
    }
    let length = |e: usize| -> f64 { edges[e].0.windows(2).map(|w| w[0].distance(w[1])).sum() };
    let lens: Vec<f64> = (0..edges.len()).map(length).collect();
    // The profiles' own outlines aren't openings.
    let mut outlines: HashMap<u32, Vec<u32>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        for f in &e.3 {
            outlines.entry(*f).or_default().push(i as u32);
        }
    }
    let outlines: Vec<Vec<u32>> = outlines.into_values().collect();
    let mut loops: Vec<(Vec<u32>, f64)> = Vec::new();
    let mut steps = 0usize;
    for start in 0..edges.len() {
        if need[start] == 0 {
            continue;
        }
        // Loops whose lowest edge is this one, walked from its one end back to the other.
        let (home, first) = (edges[start].1, edges[start].2);
        let mut stack: Vec<(usize, Vec<u32>, Vec<usize>)> = vec![(first, vec![start as u32], vec![first])];
        while let Some((at, path, seen)) = stack.pop() {
            steps += 1;
            if steps > 400_000 {
                return Err(EngineError::NotAvailable("the network has too many ways round".into()));
            }
            if at == home {
                let mut key = path.clone();
                key.sort_unstable();
                if !outlines.iter().any(|o| {
                    let mut o = o.clone();
                    o.sort_unstable();
                    o == key
                }) && !loops.iter().any(|(l, _)| {
                    let mut l = l.clone();
                    l.sort_unstable();
                    l == key
                }) {
                    let len = path.iter().map(|e| lens[*e as usize]).sum();
                    loops.push((path, len));
                }
                continue;
            }
            if path.len() >= 12 {
                continue;
            }
            for (i, e) in edges.iter().enumerate() {
                if i <= start || need[i] == 0 || path.contains(&(i as u32)) {
                    continue;
                }
                let next = if e.1 == at {
                    e.2
                } else if e.2 == at {
                    e.1
                } else {
                    continue;
                };
                if next != home && seen.contains(&next) {
                    continue;
                }
                let (mut p, mut v) = (path.clone(), seen.clone());
                p.push(i as u32);
                v.push(next);
                stack.push((next, p, v));
            }
        }
    }
    // Few-sided loops first, then short ones.
    loops.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then(a.1.total_cmp(&b.1)));
    fn cover(loops: &[(Vec<u32>, f64)], need: &mut [i32], chosen: &mut Vec<usize>, tries: &mut usize) -> bool {
        let Some(e) = need.iter().position(|n| *n > 0) else { return true };
        *tries += 1;
        if *tries > 20_000 {
            return false;
        }
        for (i, (l, _)) in loops.iter().enumerate() {
            if chosen.contains(&i) || !l.contains(&(e as u32)) || l.iter().any(|x| need[*x as usize] < 1) {
                continue;
            }
            l.iter().for_each(|x| need[*x as usize] -= 1);
            chosen.push(i);
            if cover(loops, need, chosen, tries) {
                return true;
            }
            chosen.pop();
            l.iter().for_each(|x| need[*x as usize] += 1);
        }
        false
    }
    let (mut chosen, mut tries) = (Vec::new(), 0);
    if !cover(&loops, &mut need, &mut chosen, &mut tries) || chosen.is_empty() {
        return Err(loose());
    }
    Ok(chosen.into_iter().map(|i| loops[i].0.clone()).collect())
}

/// The network's edges joined through points where only two meet smoothly (a circle's start) into whole sides.
fn stretches(edges: &[NetEdge], vertices: &[DVec3]) -> Vec<Side> {
    let mut at: Vec<Vec<usize>> = vec![Vec::new(); vertices.len()];
    for (i, e) in edges.iter().enumerate() {
        at[e.1].push(i);
        if e.2 != e.1 {
            at[e.2].push(i);
        }
    }
    let heading = |i: usize, v: usize| -> DVec3 {
        let pts = &edges[i].0;
        let n = pts.len();
        if edges[i].1 == v { (pts[1] - pts[0]).normalize_or_zero() } else { (pts[n - 2] - pts[n - 1]).normalize_or_zero() }
    };
    let smooth = |v: usize| -> Option<(usize, usize)> {
        match at[v].as_slice() {
            [a, b] if edges[*a].3 == edges[*b].3 && heading(*a, v).dot(heading(*b, v)) < -0.999 => Some((*a, *b)),
            _ => None,
        }
    };
    let mut used = vec![false; edges.len()];
    let mut out = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut chain = vec![start];
        let mut pts = edges[start].0.clone();
        for forward in [true, false] {
            let mut v = if forward { edges[start].2 } else { edges[start].1 };
            while let Some((a, b)) = smooth(v) {
                let last = if forward { chain[chain.len() - 1] } else { chain[0] };
                let next = if a == last { b } else { a };
                if used[next] {
                    break;
                }
                used[next] = true;
                let mut more = edges[next].0.clone();
                if edges[next].1 != v {
                    more.reverse();
                }
                v = if edges[next].1 == v { edges[next].2 } else { edges[next].1 };
                if forward {
                    chain.push(next);
                    pts.extend(more.into_iter().skip(1));
                } else {
                    chain.insert(0, next);
                    more.reverse();
                    more.pop();
                    more.extend(pts);
                    pts = more;
                }
            }
        }
        let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut left = total / 2.0;
        let mut mid = pts[0];
        for w in pts.windows(2) {
            let l = w[0].distance(w[1]);
            if left <= l {
                mid = w[0].lerp(w[1], if l > 0.0 { left / l } else { 0.0 });
                break;
            }
            left -= l;
        }
        out.push(Side { fixed: edges[start].3.len() >= 2, edges: chain.into_iter().map(|e| e as u32).collect(), mid, order: 0 });
    }
    out
}

/// The points of a rim that what lies beside it is sampled at.
const SAMPLES: usize = 65;

/// What lies beside a side of the network, for faces to be tangent to: the body's face beside a profile's edge, a wall
/// square to a sketch profile, or along a loose curve a strip that leaves it towards what else meets its ends.
fn beside(s: &Session, net: &Net, profiles: &[Profile], side: &Side, rim: &Rim) -> Result<Option<Bend>> {
    let e = side.edges[0];
    let (_, _, _, borders) = &net.edges[e as usize];
    let pts: Vec<DVec3> = (0..SAMPLES).map(|i| rim.at(i as f64 / (SAMPLES - 1) as f64)).collect();
    let n = pts.len();
    let tangent = |i: usize| (pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)]).normalize_or_zero();
    // How the side itself bends at a sample.
    let bend = |i: usize| -> DVec3 {
        let j = i.clamp(1, n - 2);
        let (a, b) = (pts[j] - pts[j - 1], pts[j + 1] - pts[j]);
        (b.normalize_or_zero() - a.normalize_or_zero()) / ((a.length() + b.length()) / 2.0).max(1e-12)
    };
    // A surface that is straight across the side, in the directions given.
    let ruled = |out: &dyn Fn(usize) -> DVec3| -> Option<Bend> {
        Bend::new(
            (0..n)
                .map(|i| {
                    let normal = tangent(i).cross(out(i)).normalize_or_zero();
                    (normal, [bend(i).dot(normal), 0.0, 0.0])
                })
                .collect(),
        )
    };
    if let Some(f) = borders.first() {
        match profiles.get(*f as usize) {
            Some(Profile::Region { sketch, .. }) => {
                let Some(sk) = s.doc.sketch(*sketch) else { return Ok(None) };
                let up = sk.plane.normal();
                return Ok(ruled(&|_| up));
            }
            Some(Profile::Face { body, face }) => {
                let Some(b) = s.doc.body(*body) else { return Ok(None) };
                let mid = pts[n / 2];
                let mut best: Option<(f64, u32)> = None;
                for be in s.kernel.face_edges(&b.shape, *face)? {
                    let d = fine(&s.kernel.edge_curve(&b.shape, be)?)
                        .windows(2)
                        .map(|w| {
                            let d = w[1] - w[0];
                            let t = if d.length_squared() > 0.0 { ((mid - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                            w[0].lerp(w[1], t).distance(mid)
                        })
                        .fold(f64::MAX, f64::min);
                    if best.is_none_or(|x| d < x.0) {
                        best = Some((d, be));
                    }
                }
                let Some((_, be)) = best else { return Ok(None) };
                let others = b.mesh.edge_ids.iter().position(|x| *x == be).and_then(|k| b.mesh.edge_faces.get(k));
                let Some(other) = others.and_then(|fs| fs.iter().copied().find(|f| f != face)) else { return Ok(None) };
                let along: Vec<DVec3> = (0..n).map(tangent).collect();
                let at = s.kernel.face_at(&b.shape, other, &pts, &along)?;
                let over: Vec<DVec3> = at.iter().zip(&along).map(|(a, t)| a.0.cross(*t)).collect();
                let mixed: Vec<DVec3> = over.iter().zip(&along).map(|(c, t)| *c + *t).collect();
                let (kc, km) = (s.kernel.face_at(&b.shape, other, &pts, &over)?, s.kernel.face_at(&b.shape, other, &pts, &mixed)?);
                return Ok(Bend::new(
                    (0..n)
                        .map(|i| {
                            let (kt, c, m) = (at[i].1, kc[i].1, km[i].1);
                            (at[i].0, [kt, c, m - (kt + c) / 2.0])
                        })
                        .collect(),
                ));
            }
            None => {}
        }
    }
    // A loose curve: the way out at each end is along another edge there (or, with a profile's edge, along it).
    let leave = |at: DVec3, own: DVec3| -> Option<DVec3> {
        let mut best: Option<(f64, DVec3)> = None;
        for (i, (q, a, z, _)) in net.edges.iter().enumerate() {
            if side.edges.contains(&(i as u32)) || q.len() < 2 {
                continue;
            }
            for (v, d) in [(*a, q[1] - q[0]), (*z, q[q.len() - 2] - q[q.len() - 1])] {
                if net.vertices[v].distance(at) > 2e-3 {
                    continue;
                }
                let d = d.normalize_or_zero();
                let square = d - own * d.dot(own);
                if square.length() > 0.1 && best.is_none_or(|b| square.length() > b.0) {
                    best = Some((square.length(), square.normalize()));
                }
            }
        }
        best.map(|b| b.1)
    };
    let (a, z) = (leave(pts[0], tangent(0)), leave(pts[n - 1], tangent(n - 1)));
    let (Some(a), Some(mut z)) = (a.or(z), z.or(a)) else { return Ok(None) };
    // Carried along the curve without twisting, the first direction says which way round the second belongs.
    let mut carried = vec![a];
    for i in 1..n {
        let (t0, t1) = (tangent(i - 1), tangent(i));
        let q = glam::DQuat::from_rotation_arc(t0.normalize_or(DVec3::X), t1.normalize_or(DVec3::X));
        carried.push(q * carried[i - 1]);
    }
    if carried[n - 1].dot(z) < 0.0 {
        z = -z;
    }
    let turn = carried[n - 1].cross(z).dot(tangent(n - 1)).atan2(carried[n - 1].dot(z));
    let out: Vec<DVec3> = (0..n)
        .map(|i| glam::DQuat::from_axis_angle(tangent(i).normalize_or(DVec3::X), turn * i as f64 / (n - 1) as f64) * carried[i])
        .collect();
    Ok(ruled(&|i| out[i]))
}

/// An opening's sides in order round it: edges of one stretch joined.
fn rims_of(s: &Session, net: &Net, profiles: &[Profile], cell: &[u32]) -> Result<Vec<Rim>> {
    let loose = || EngineError::NotAvailable(LOOSE.into());
    // Each edge the way the loop runs along it.
    let mut runs: Vec<(u32, bool)> = Vec::new();
    let first = &net.edges[cell[0] as usize];
    let mut at = if cell.len() == 1
        || cell.len() > 1 && {
            let next = &net.edges[cell[1] as usize];
            next.1 == first.2 || next.2 == first.2
        } {
        first.1
    } else {
        first.2
    };
    for &e in cell {
        let edge = &net.edges[e as usize];
        let forward = edge.1 == at;
        if !forward && edge.2 != at {
            return Err(loose());
        }
        at = if forward { edge.2 } else { edge.1 };
        runs.push((e, forward));
    }
    let side_of = |e: u32| net.sides.iter().position(|x| x.edges.contains(&e));
    // Start at a corner, so no side is split over the end.
    if let Some(k) = (0..runs.len()).find(|&k| side_of(runs[k].0) != side_of(runs[(k + runs.len() - 1) % runs.len()].0)) {
        runs.rotate_left(k);
    }
    let mut out = Vec::new();
    let mut k = 0;
    while k < runs.len() {
        let side = side_of(runs[k].0).ok_or_else(loose)?;
        let mut list = Vec::new();
        while k < runs.len() && side_of(runs[k].0) == Some(side) {
            let (e, forward) = runs[k];
            let mut run = Run::new(&s.kernel.edge_curve(&net.shape, e)?).ok_or_else(loose)?;
            // The kernel's own start of the edge, as the network has it.
            if run.at(0.0).distance(net.vertices[net.edges[e as usize].1]) > run.at(1.0).distance(net.vertices[net.edges[e as usize].1]) {
                run = run.flipped();
            }
            list.push(if forward { run } else { run.flipped() });
            k += 1;
        }
        let side = &net.sides[side];
        let mut rim = Rim::new(list, side.order, None).ok_or_else(loose)?;
        if side.order > 0 {
            rim.bend = beside(s, net, profiles, side, &rim)?;
        }
        out.push(rim);
    }
    Ok(out)
}

/// The older form, still what edges picked on a body mean: a face through a loop of them, joined to the body.
fn close_opening(s: &mut Session, p: &Params) -> Result<Value> {
    let (body, edges) = edge_target(s, p)?;
    if edges.is_empty() {
        return Err(EngineError::NotAvailable("select the edges around the opening".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let shape = s.kernel.fill(&b.shape, &edges)?;
    set_body_shape(s, body, shape)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

pub(super) fn fill(s: &mut Session, p: &Params) -> Result<Value> {
    let loose_edges = !s.selection.is_empty() && s.selection.iter().all(|x| matches!(x, Pick::Edge { .. }));
    if p.has("body") || !p.has("profiles") && !p.has("guides") && loose_edges {
        return close_opening(s, p);
    }
    let shape = solid(s, p)?;
    let result = result_param(p)?;
    let body = place_result(s, shape, result, None, BoolOp::Union)?;
    s.selection.clear();
    Ok(json!({ "body": body.0 }))
}

fn solid(s: &Session, p: &Params) -> Result<Shape> {
    let (net, profiles, _) = network(s, p)?;
    let mut faces = (0..net.faces).map(|f| s.kernel.face(&net.shape, f)).collect::<std::result::Result<Vec<_>, _>>()?;
    for cell in &net.cells {
        let rims = rims_of(s, &net, &profiles, cell)?;
        for sheet in patch::fill(&rims).map_err(|e| EngineError::NotAvailable(e.into()))? {
            faces.push(s.kernel.from_brep(&sheet)?);
        }
    }
    let size = net.vertices.iter().flat_map(|a| net.vertices.iter().map(move |b| a.distance(*b))).fold(0.0, f64::max).max(1.0);
    let mut last = None;
    for tolerance in [1e-5 * size, 1e-4 * size, 1e-3 * size] {
        match s.kernel.sew_solid(&faces, tolerance) {
            Ok(shape) => {
                let m = s.kernel.mass_props(&shape)?;
                let drawn = s.kernel.mass_props_mesh(&shape)?;
                // Faces that run through each other measure right but don't draw right.
                if m.volume.is_finite() && m.volume.abs() > 1e-9 && (m.volume - drawn.volume).abs() < m.volume.abs() * 0.05 {
                    return Ok(shape);
                }
                last = Some(EngineError::NotAvailable("the faces run into each other with these settings".into()));
            }
            Err(e) => last = Some(e.into()),
        }
    }
    Err(last.unwrap_or_else(|| EngineError::NotAvailable(LOOSE.into())))
}

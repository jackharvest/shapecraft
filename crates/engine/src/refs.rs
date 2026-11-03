//! Faces and edges that history steps point at. The kernel numbers a body's faces and edges afresh after every
//! change, so a step that says "edge 11" can find a different edge once an earlier step is edited, deleted or
//! suppressed. Each step therefore also keeps what its faces and edges looked like when it ran (kind, direction,
//! where they sit in the body's bounding box, size), and a replay points it at the ones that look most alike.

use std::collections::HashMap;

use glam::DVec3;
use serde_json::Value;
use shapecraft_doc::{ItemId, LogEntry, Pick, TopoRef};
use shapecraft_kernel::{EdgeCurve, SubShape};

use crate::Session;

/// Calls `f` with every face or edge number in `v`: `"face"`, `"faces"`, `"edge"` or `"edges"` next to a `"body"`.
fn walk(v: &mut Value, f: &mut impl FnMut(ItemId, bool, &mut Value)) {
    match v {
        Value::Object(m) => {
            if let Some(body) = m.get("body").and_then(Value::as_u64).map(ItemId) {
                for (key, edge) in [("face", false), ("faces", false), ("edge", true), ("edges", true)] {
                    match m.get_mut(key) {
                        Some(Value::Array(list)) => list.iter_mut().filter(|n| n.is_u64()).for_each(|n| f(body, edge, n)),
                        Some(n) if n.is_u64() => f(body, edge, n),
                        _ => {}
                    }
                }
            }
            for (_, child) in m.iter_mut() {
                walk(child, f);
            }
        }
        Value::Array(list) => list.iter_mut().for_each(|c| walk(c, f)),
        _ => {}
    }
}

/// The faces and edges a command is about to use (its parameters and the selection), as they look now.
pub(crate) fn collect(s: &Session, params: &Value, selection: &[Pick]) -> Vec<TopoRef> {
    let mut picked: Vec<(ItemId, bool, u32)> = Vec::new();
    let mut add = |body: ItemId, edge: bool, n: u32| {
        if !picked.contains(&(body, edge, n)) {
            picked.push((body, edge, n));
        }
    };
    walk(&mut params.clone(), &mut |body, edge, n| {
        if let Some(n) = n.as_u64().and_then(|n| u32::try_from(n).ok()) {
            add(body, edge, n);
        }
    });
    for p in selection {
        match *p {
            Pick::Face { body, face } => add(body, false, face),
            Pick::Edge { body, edge } => add(body, true, edge),
            _ => {}
        }
    }
    // Hundreds of edges ("fillet every edge") would make each replay slow for little gain.
    if picked.len() > 64 {
        return Vec::new();
    }
    picked.into_iter().filter_map(|(body, edge, n)| look(s, body, edge, n)).collect()
}

/// What face or edge `index` of `body` looks like.
fn look(s: &Session, body: ItemId, edge: bool, index: u32) -> Option<TopoRef> {
    let shape = &s.doc.body(body)?.shape;
    let bb = s.kernel.bounds(shape).ok()?;
    let sub = if edge { SubShape::Edge(index) } else { SubShape::Face(index) };
    let (size, center) = s.kernel.measure(shape, sub).ok()?;
    let ext = (bb.max - bb.min).max(DVec3::splat(1e-9));
    let rel = (center - bb.min) / ext;
    let (kind, dir) = if edge {
        match s.kernel.edge_curve(shape, index).ok()? {
            EdgeCurve::Line { a, b } => ("line", Some((b - a).normalize_or_zero())),
            EdgeCurve::Circle { normal, .. } => ("circle", Some(normal.normalize_or_zero())),
            EdgeCurve::Arc { a, mid, b } => ("arc", Some((mid - a).cross(b - a).normalize_or_zero())),
            EdgeCurve::Other { .. } => ("curve", None),
        }
    } else {
        match s.kernel.face_plane(shape, index).ok().flatten() {
            Some(fp) => ("plane", Some(fp.normal.normalize_or_zero())),
            None => match s.kernel.axis_of(shape, SubShape::Face(index)).ok().flatten() {
                Some(ax) => ("round", Some(ax.dir.normalize_or_zero())),
                None => ("curved", None),
            },
        }
    };
    Some(TopoRef {
        body,
        edge,
        index,
        kind: kind.into(),
        center: center.to_array(),
        rel: rel.to_array(),
        dir: dir.map(|d| d.to_array()),
        size,
    })
}

/// How unlike two faces or edges are: 0 for the same, about 1 for "the same kind and direction, but elsewhere",
/// 10 or more for a different kind or direction.
fn unlike(a: &TopoRef, b: &TopoRef) -> f64 {
    let mut d = 0.0;
    if a.kind != b.kind {
        d += 10.0;
    }
    match (a.dir, b.dir) {
        (Some(x), Some(y)) => {
            let dot = DVec3::from(x).dot(DVec3::from(y));
            // A flat face's normal says which side it faces; lines and axes run either way.
            let dot = if a.kind == "plane" { dot } else { dot.abs() };
            d += (1.0 - dot) * 20.0;
        }
        (None, None) => {}
        _ => d += 5.0,
    }
    d += DVec3::from(a.rel).distance(DVec3::from(b.rel));
    d += DVec3::from(a.center).distance(DVec3::from(b.center)) * 0.01;
    if a.size > 0.0 && b.size > 0.0 {
        d += (a.size / b.size).ln().abs() * 0.1;
    }
    d
}

/// The face or edge of the body as it is now that looks most like `r`.
fn find(s: &Session, r: &TopoRef) -> Option<TopoRef> {
    let shape = &s.doc.body(r.body)?.shape;
    let count = if r.edge { s.kernel.edge_count(shape).ok()? } else { s.kernel.face_count(shape).ok()? };
    if r.index < count
        && let Some(same) = look(s, r.body, r.edge, r.index)
        && unlike(r, &same) < 1e-3
    {
        return Some(same);
    }
    (0..count.min(5000))
        .filter_map(|i| look(s, r.body, r.edge, i))
        .map(|c| (unlike(r, &c) + if c.index == r.index { 0.0 } else { 1e-6 }, c))
        .filter(|(d, _)| *d < 10.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

/// Points a logged command at the faces and edges that look like the ones it used, in the model as it is now (a
/// replay, just before the command runs again), and remembers them as they look now.
pub(crate) fn remap(s: &Session, e: &mut LogEntry) {
    if e.refs.is_empty() {
        return;
    }
    let mut moved: HashMap<(ItemId, bool, u32), u32> = HashMap::new();
    let mut now = Vec::with_capacity(e.refs.len());
    for r in &e.refs {
        match find(s, r) {
            Some(found) => {
                if found.index != r.index {
                    moved.insert((r.body, r.edge, r.index), found.index);
                }
                now.push(found);
            }
            None => now.push(r.clone()),
        }
    }
    e.refs = now;
    if moved.is_empty() {
        return;
    }
    walk(&mut e.params, &mut |body, edge, n| {
        if let Some(to) = n.as_u64().and_then(|i| u32::try_from(i).ok()).and_then(|i| moved.get(&(body, edge, i))) {
            *n = Value::from(*to);
        }
    });
    for p in &mut e.selection {
        match p {
            Pick::Face { body, face } => *face = moved.get(&(*body, false, *face)).copied().unwrap_or(*face),
            Pick::Edge { body, edge } => *edge = moved.get(&(*body, true, *edge)).copied().unwrap_or(*edge),
            _ => {}
        }
    }
}

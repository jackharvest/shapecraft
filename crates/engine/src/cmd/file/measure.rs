//! Measure and Inspect: sizes, distances and angles of picks, and a description of the whole project.

use serde_json::{Value, json};
use shapecraft_doc::Pick;

use crate::cmd::{CommandSpec, always};
use crate::{Params, Result, Session};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "measure",
            label: "Measure",
            menu: "Tools/Measure/Object Measurements",
            shortcut: None,
            help: "Sizes of what's selected; with two things selected, the gap between them",
            params: r#"{"picks": [Pick]?}  (default: the selection)"#,
            modifies: false,
            enabled: always,
            run: measure,
        },
        CommandSpec {
            id: "inspect",
            label: "Inspect",
            menu: "",
            shortcut: None,
            help: "Describe the project: bodies with volumes and bounds, sketches with regions, the selection",
            params: r#"{"body": id?, "sketch": id?, "faces": bool (true; false leaves out each body's face list)}  (body or sketch: only that one, and nothing of the other kind)"#,
            modifies: false,
            enabled: always,
            run: inspect,
        },
    ]
}

fn round3(v: glam::DVec3) -> [f64; 3] {
    let r = |x: f64| {
        let y = (x * 1e4).round() / 1e4;
        if y == 0.0 { 0.0 } else { y }
    };
    [r(v.x), r(v.y), r(v.z)]
}

fn inspect(s: &mut Session, p: &Params) -> Result<Value> {
    let only_body = p.opt_id("body");
    let only_sketch = p.opt_id("sketch");
    let with_faces = p.bool_or("faces", true);
    if let Some(id) = only_body
        && s.doc.body(id).is_none()
    {
        return Err(crate::EngineError::BadParams(format!("no body {}", id.0)));
    }
    if let Some(id) = only_sketch
        && s.doc.sketch(id).is_none()
    {
        return Err(crate::EngineError::BadParams(format!("no sketch {}", id.0)));
    }
    let bodies: Vec<Value> = s
        .doc
        .bodies
        .iter()
        .filter(|b| only_sketch.is_none() && only_body.is_none_or(|id| id == b.id))
        .map(|b| {
            let mp = s.kernel.mass_props(&b.shape).ok();
            let bb = s.kernel.bounds(&b.shape).ok();
            let face_count = s.kernel.face_count(&b.shape).unwrap_or(0);
            // Planar faces with their outward normals, so agents can say which face to work on.
            let faces: Vec<Value> = (0..if with_faces { face_count.min(500) } else { 0 })
                .map(|f| match s.kernel.face_plane(&b.shape, f).ok().flatten() {
                    Some(fp) => json!({ "face": f, "planar": true, "center": round3(fp.origin), "normal": round3(fp.normal) }),
                    None => json!({ "face": f, "planar": false }),
                })
                .collect();
            json!({
                "id": b.id.0,
                "name": b.name,
                "visible": b.visible,
                "volume_mm3": mp.map(|m| m.volume),
                "area_mm2": mp.map(|m| m.area),
                "bounds": bb.map(|bb| json!({ "min": bb.min.to_array(), "max": bb.max.to_array() })),
                "face_count": face_count,
                "faces": faces,
                "edges": b.mesh.edge_ids.len(),
                "appearance": b.appearance,
            })
        })
        .collect();
    let sketches: Vec<Value> = s
        .doc
        .sketches
        .iter()
        .filter(|sk| only_body.is_none() && only_sketch.is_none_or(|id| id == sk.id))
        .map(|sk| {
            json!({
                "id": sk.id.0,
                "name": sk.name,
                "visible": sk.visible,
                "plane": { "origin": sk.plane.origin.to_array(), "x": sk.plane.x.to_array(), "y": sk.plane.y.to_array() },
                "on_body": sk.on_body.map(|b| b.0),
                "entities": sk.entities,
                // holes: how many holes the region has; inside: the region it fills a hole of (the inside of an
                // "o", the middle of two nested rectangles), so solid letters are the regions with no "inside".
                "regions": sk.regions().iter().zip(shapecraft_doc::region_nesting(sk.regions())).map(|(r, (holes, inside))| json!({
                    "region": r.index, "area_mm2": r.area, "center": [r.centroid.x, r.centroid.y], "holes": holes, "inside": inside,
                })).collect::<Vec<_>>(),
                // Every element fully defined (drawn green), as the reference shows a finished sketch.
                "defined": !sk.entities.is_empty() && shapecraft_doc::constraints::fully_defined(&sk.entities, &sk.constraints).iter().all(|d| *d),
            })
        })
        .collect();
    Ok(json!({
        "name": s.doc.name,
        "units": s.doc.units,
        "bodies": bodies,
        "sketches": sketches,
        "active_sketch": s.active_sketch.map(|i| i.0),
        "selection": s.selection,
        "history": s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.clone()).collect::<Vec<_>>(),
        "failed_steps": s.doc.history.iter().filter(|h| h.error.is_some()).map(|h| h.label.clone()).collect::<Vec<_>>(),
        "can_undo": s.can_undo(),
        "can_redo": s.can_redo(),
    }))
}

fn sub_of(p: &Pick) -> Option<(shapecraft_doc::ItemId, shapecraft_kernel::SubShape)> {
    use shapecraft_kernel::SubShape;
    match *p {
        Pick::Body { body } => Some((body, SubShape::Whole)),
        Pick::Face { body, face } => Some((body, SubShape::Face(face))),
        Pick::Edge { body, edge } => Some((body, SubShape::Edge(edge))),
        _ => None,
    }
}

fn measure(s: &mut Session, p: &Params) -> Result<Value> {
    use shapecraft_kernel::SubShape;
    let picks: Vec<Pick> = if p.has("picks") { p.parse("picks")? } else { s.selection.clone() };
    let mut items = Vec::new();
    for pick in &picks {
        let Some((body, sub)) = sub_of(pick) else { continue };
        let Some(b) = s.doc.body(body) else { continue };
        let (value, center) = s.kernel.measure(&b.shape, sub)?;
        let mut v = json!({ "pick": pick, "center": center.to_array() });
        match sub {
            SubShape::Whole => {
                let bb = s.kernel.bounds(&b.shape)?;
                let mp = s.kernel.mass_props(&b.shape)?;
                v["volume_mm3"] = json!(value);
                v["area_mm2"] = json!(mp.area);
                v["size"] = json!((bb.max - bb.min).to_array());
            }
            SubShape::Face(f) => {
                v["area_mm2"] = json!(value);
                if let Some(fp) = s.kernel.face_plane(&b.shape, f)? {
                    v["normal"] = json!(fp.normal.to_array());
                }
            }
            SubShape::Edge(_) => v["length_mm"] = json!(value),
        }
        items.push(v);
    }
    let mut out = json!({ "items": items });
    // Ours: two or more whole bodies picked → where each pair overlaps (interference), by the volume they share.
    let bodies: Vec<shapecraft_doc::ItemId> =
        picks.iter().filter_map(|p| if let Pick::Body { body } = p { Some(*body) } else { None }).collect();
    if bodies.len() >= 2 && bodies.len() <= 8 {
        let mut clashes = Vec::new();
        for (i, a) in bodies.iter().enumerate() {
            for b in &bodies[i + 1..] {
                let (Some(x), Some(y)) = (s.doc.body(*a), s.doc.body(*b)) else { continue };
                if !s.kernel.bounds(&x.shape)?.overlaps(&s.kernel.bounds(&y.shape)?) {
                    continue;
                }
                let Ok(common) = s.kernel.boolean(shapecraft_kernel::BoolOp::Intersect, &x.shape, &y.shape) else { continue };
                let Ok(mp) = s.kernel.mass_props(&common) else { continue };
                if mp.volume > 1e-9 {
                    clashes.push(json!({ "a": a.0, "b": b.0, "volume_mm3": mp.volume, "center": mp.centroid.to_array() }));
                }
            }
        }
        out["interference"] = json!(clashes);
    }
    if let [a, b] = picks.as_slice()
        && let (Some((ba, sa)), Some((bb, sb))) = (sub_of(a), sub_of(b))
        && let (Some(x), Some(y)) = (s.doc.body(ba), s.doc.body(bb))
    {
        let d = s.kernel.distance(&x.shape, sa, &y.shape, sb)?;
        out["distance"] = json!({ "mm": d.distance, "from": d.a.to_array(), "to": d.b.to_array() });
        // The farthest two points of the pair (from their tessellation).
        let (pa, pb) = (pick_points(x, sa), pick_points(y, sb));
        let mut far: Option<(f64, glam::DVec3, glam::DVec3)> = None;
        for p in &pa {
            for q in &pb {
                let dd = p.distance(*q);
                if far.is_none_or(|(m, ..)| dd > m) {
                    far = Some((dd, *p, *q));
                }
            }
        }
        if let Some((mm, from, to)) = far {
            out["max_distance"] = json!({ "mm": mm, "from": from.to_array(), "to": to.to_array() });
        }
        // Two flat faces or two straight edges: the angle between them.
        let dir_of = |b: &shapecraft_doc::Body, sub: SubShape| -> Option<glam::DVec3> {
            match sub {
                SubShape::Face(f) => s.kernel.face_plane(&b.shape, f).ok().flatten().map(|fp| fp.normal),
                SubShape::Edge(e) => match s.kernel.edge_curve(&b.shape, e).ok()? {
                    shapecraft_kernel::EdgeCurve::Line { a, b } => (b - a).try_normalize(),
                    _ => None,
                },
                SubShape::Whole => None,
            }
        };
        if let (Some(u), Some(v)) = (dir_of(x, sa), dir_of(y, sb))
            && matches!((sa, sb), (SubShape::Face(_), SubShape::Face(_)) | (SubShape::Edge(_), SubShape::Edge(_)))
        {
            let mut deg = u.dot(v).clamp(-1.0, 1.0).acos().to_degrees();
            // Edges have no direction of their own: the smaller angle between their lines.
            if matches!(sa, SubShape::Edge(_)) && deg > 90.0 {
                deg = 180.0 - deg;
            }
            out["angle_deg"] = json!(deg);
        }
        // Central distance: from a center point (a circle's or arc's, a body's centre of mass) or a central axis (a
        // cylinder's or cone's) to the other pick's center, axis, line or plane.
        let (fa, fb) = (feature(s, x, sa), feature(s, y, sb));
        if let (Some(fa), Some(fb)) = (fa, fb)
            && (fa.1 || fb.1)
            && let Some((mm, from, to)) = gap(fa.0, fb.0, false)
        {
            out["central_distance"] = json!({ "mm": mm, "from": from.to_array(), "to": to.to_array() });
        }
        // Parallel distance: two parallel flat faces or straight edges (or an edge and a face parallel to it), or two
        // parallel axes.
        if let (Some(fa), Some(fb)) = (fa, fb)
            && let Some((mm, from, to)) = gap(fa.0, fb.0, true)
        {
            out["parallel_distance"] = json!({ "mm": mm, "from": from.to_array(), "to": to.to_array() });
        }
    }
    // One arc picked: the angle it sweeps.
    if let [pick] = picks.as_slice()
        && let Some((body, SubShape::Edge(e))) = sub_of(pick)
        && let Some(b) = s.doc.body(body)
        && let Ok(shapecraft_kernel::EdgeCurve::Arc { a, mid, b: end }) = s.kernel.edge_curve(&b.shape, e)
        && let Some(deg) = arc_sweep(a, mid, end)
    {
        out["arc_angle_deg"] = json!(deg);
    }
    Ok(out)
}

/// What a pick measures from: a point, a line (an axis or a straight edge) or a plane, and whether it is a center
/// (a center point or a central axis) for Central Distance.
#[derive(Clone, Copy)]
enum Feat {
    Point(glam::DVec3),
    Line(glam::DVec3, glam::DVec3),
    Plane(glam::DVec3, glam::DVec3),
}

fn feature(s: &Session, b: &shapecraft_doc::Body, sub: shapecraft_kernel::SubShape) -> Option<(Feat, bool)> {
    use shapecraft_kernel::{EdgeCurve, SubShape};
    match sub {
        SubShape::Whole => s.kernel.mass_props(&b.shape).ok().map(|m| (Feat::Point(m.centroid), true)),
        SubShape::Edge(e) => match s.kernel.edge_curve(&b.shape, e).ok()? {
            EdgeCurve::Line { a, b } => Some((Feat::Line(a, (b - a).try_normalize()?), false)),
            EdgeCurve::Circle { center, .. } => Some((Feat::Point(center), true)),
            EdgeCurve::Arc { a, mid, b } => arc_center(a, mid, b).map(|c| (Feat::Point(c), true)),
            EdgeCurve::Other { .. } => None,
        },
        SubShape::Face(f) => {
            if let Some(fp) = s.kernel.face_plane(&b.shape, f).ok().flatten() {
                return Some((Feat::Plane(fp.origin, fp.normal.try_normalize()?), false));
            }
            let ax = s.kernel.axis_of(&b.shape, sub).ok().flatten()?;
            Some((Feat::Line(ax.origin, ax.dir.try_normalize()?), true))
        }
    }
}

/// The center of the circle through three points.
fn arc_center(a: glam::DVec3, m: glam::DVec3, b: glam::DVec3) -> Option<glam::DVec3> {
    let (u, v) = (m - a, b - a);
    let n = u.cross(v);
    let nn = n.length_squared();
    if nn < 1e-18 {
        return None;
    }
    Some(a + (v.length_squared() * n.cross(u) + u.length_squared() * v.cross(n)) / (2.0 * nn))
}

/// The angle an arc from `a` through `m` to `b` sweeps, in degrees.
fn arc_sweep(a: glam::DVec3, m: glam::DVec3, b: glam::DVec3) -> Option<f64> {
    let c = arc_center(a, m, b)?;
    let (ua, um, ub) = (a - c, m - c, b - c);
    let n = ua.cross(ub);
    let half = |p: glam::DVec3, q: glam::DVec3| p.angle_between(q);
    let direct = half(ua, ub);
    // The middle point tells the short way round from the long one.
    let short = (ua.cross(um).dot(n) >= 0.0) == (um.cross(ub).dot(n) >= 0.0) && n.length_squared() > 1e-18;
    Some(if short { direct } else { std::f64::consts::TAU - direct }.to_degrees())
}

/// The gap between two features, with the points it runs between. `parallel`: only for features parallel to each
/// other (a plane, line or axis with another), the distance square across.
fn gap(a: Feat, b: Feat, parallel: bool) -> Option<(f64, glam::DVec3, glam::DVec3)> {
    use Feat::*;
    let on_line = |p: glam::DVec3, o: glam::DVec3, d: glam::DVec3| o + d * (p - o).dot(d);
    let on_plane = |p: glam::DVec3, o: glam::DVec3, n: glam::DVec3| p - n * (p - o).dot(n);
    let par = |u: glam::DVec3, v: glam::DVec3| u.cross(v).length() < 1e-6;
    let res = |from: glam::DVec3, to: glam::DVec3| Some((from.distance(to), from, to));
    match (a, b, parallel) {
        (Point(p), Point(q), false) => res(p, q),
        (Point(p), Line(o, d), false) => res(p, on_line(p, o, d)),
        (Line(o, d), Point(p), false) => res(on_line(p, o, d), p),
        (Point(p), Plane(o, n), false) => res(p, on_plane(p, o, n)),
        (Plane(o, n), Point(p), false) => res(on_plane(p, o, n), p),
        (Line(o1, d1), Line(o2, d2), _) if par(d1, d2) => res(o1, on_line(o1, o2, d2)),
        (Line(o1, d1), Line(o2, d2), false) => {
            // Skew lines: the closest points.
            let w = o1 - o2;
            let (a, b, c, d, e) = (d1.dot(d1), d1.dot(d2), d2.dot(d2), d1.dot(w), d2.dot(w));
            let den = a * c - b * b;
            (den.abs() > 1e-12).then_some(())?;
            res(o1 + d1 * ((b * e - c * d) / den), o2 + d2 * ((a * e - b * d) / den))
        }
        (Line(o, d), Plane(po, n), _) if d.dot(n).abs() < 1e-6 => res(o, on_plane(o, po, n)),
        (Plane(po, n), Line(o, d), _) if d.dot(n).abs() < 1e-6 => res(on_plane(o, po, n), o),
        (Plane(o1, n1), Plane(o2, n2), true) if par(n1, n2) => res(o1, on_plane(o1, o2, n2)),
        _ => None,
    }
}

/// Points of a body, face or edge from its tessellation (at most ~1500, spread evenly), for the farthest distance.
fn pick_points(b: &shapecraft_doc::Body, sub: shapecraft_kernel::SubShape) -> Vec<glam::DVec3> {
    use shapecraft_kernel::SubShape;
    let m = &b.mesh;
    let v = |p: &[f32; 3]| glam::DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
    let pts: Vec<glam::DVec3> = match sub {
        SubShape::Whole => m.positions.iter().map(v).collect(),
        SubShape::Face(f) => m
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&m.triangle_faces)
            .filter(|(_, tf)| **tf == f)
            .flat_map(|(t, _)| t.iter().filter_map(|i| m.positions.get(*i as usize)).map(v).collect::<Vec<_>>())
            .collect(),
        SubShape::Edge(e) => {
            m.edge_ids.iter().position(|x| *x == e).and_then(|k| m.edges.get(k)).map(|l| l.iter().map(v).collect()).unwrap_or_default()
        }
    };
    let step = pts.len().div_ceil(1500).max(1);
    pts.into_iter().step_by(step).collect()
}

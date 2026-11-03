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
            params: "{}",
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

fn inspect(s: &mut Session, _: &Params) -> Result<Value> {
    let bodies: Vec<Value> = s
        .doc
        .bodies
        .iter()
        .map(|b| {
            let mp = s.kernel.mass_props(&b.shape).ok();
            let bb = s.kernel.bounds(&b.shape).ok();
            let face_count = s.kernel.face_count(&b.shape).unwrap_or(0);
            // Planar faces with their outward normals, so agents can say which face to work on.
            let faces: Vec<Value> = (0..face_count.min(500))
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
        .map(|sk| {
            json!({
                "id": sk.id.0,
                "name": sk.name,
                "visible": sk.visible,
                "plane": { "origin": sk.plane.origin.to_array(), "x": sk.plane.x.to_array(), "y": sk.plane.y.to_array() },
                "on_body": sk.on_body.map(|b| b.0),
                "entities": sk.entities,
                "regions": sk.regions().iter().map(|r| json!({ "region": r.index, "area_mm2": r.area, "center": [r.centroid.x, r.centroid.y] })).collect::<Vec<_>>(),
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
    }
    Ok(out)
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

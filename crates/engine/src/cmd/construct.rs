use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{ConstructionAxis, ConstructionPlane, ItemId, Pick, SketchPlane};

use super::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "offset_plane",
            label: "Plane - Offset",
            menu: "Construct/Plane/Offset Plane",
            shortcut: None,
            help: "A construction plane parallel to a face, plane or sketch, at a distance",
            params: r#"{"from": "xy"|"xz"|"yz" | {"body": id, "face": n} | {"plane": id} | {"sketch": id}, "distance": mm}  (default: the selected face, plane or region)"#,
            modifies: true,
            enabled: always,
            run: offset_plane,
        },
        CommandSpec {
            id: "midplane",
            label: "Midplane",
            menu: "Construct/Plane/Midplane",
            shortcut: None,
            help: "A construction plane halfway between two faces or planes (parallel: midway; meeting: halving the angle)",
            params: r#"{"a": {"body": id, "face": n} | {"plane": id}, "b": …}  (default: the two selected faces or planes)"#,
            modifies: true,
            enabled: always,
            run: midplane,
        },
        CommandSpec {
            id: "plane_3points",
            label: "Plane Through 3 Points",
            menu: "Construct/Plane/Plane Through 3 Points",
            shortcut: None,
            help: "A construction plane through three points",
            params: r#"{"points": [[x,y,z], [x,y,z], [x,y,z]]}"#,
            modifies: true,
            enabled: always,
            run: plane_3points,
        },
        CommandSpec {
            id: "plane_through_edge",
            label: "Plane - Through Edge",
            menu: "Construct/Plane/Plane Through Edge at Angle",
            shortcut: None,
            help: "A construction plane through a straight edge, turned from one of its faces by an angle",
            params: r#"{"body": id, "edge": n, "angle": degrees?}  (default: the selected edge)"#,
            modifies: true,
            enabled: always,
            run: plane_through_edge,
        },
        CommandSpec {
            id: "plane_on_curve",
            label: "Plane - On Curve at Point",
            menu: "Construct/Plane/Plane on Curve at Point",
            shortcut: None,
            help: "A construction plane square to an edge at a point along it (0 = its start, 1 = its end)",
            params: r#"{"body": id, "edge": n, "at": 0..1?}  (default: the selected edge, at its start)"#,
            modifies: true,
            enabled: always,
            run: plane_on_curve,
        },
        CommandSpec {
            id: "plane_perp_edge",
            label: "Plane Perpendicular to Edge at Point",
            menu: "Construct/Plane/Plane Perpendicular to Edge at Point",
            shortcut: None,
            help: "A construction plane through a point, square to an edge where the edge passes nearest it",
            params: r#"{"body": id, "edge": n, "point": [x,y,z]}  (default: the selected edge)"#,
            modifies: true,
            enabled: always,
            run: plane_perp_edge,
        },
        CommandSpec {
            id: "plane_parallel_face",
            label: "Plane Parallel to Face at Point",
            menu: "Construct/Plane/Plane Parallel to Face at Point",
            shortcut: None,
            help: "A construction plane through a point, parallel to a flat face or plane",
            params: r#"{"from": {"body": id, "face": n} | {"plane": id} | "xy"…, "point": [x,y,z]}  (default: the selected face or plane)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let from = if p.has("from") {
                    p.parse::<Value>("from")?
                } else {
                    selected_sources(s).into_iter().next().ok_or_else(|| EngineError::NotAvailable("select a flat face or plane".into()))?
                };
                let point = p.vec3("point")?;
                if !point.is_finite() {
                    return Err(EngineError::BadParams("point must be finite".into()));
                }
                let (mut plane, size) = source_plane(s, &from)?;
                plane.origin = point;
                let id = add_plane(s, plane, size)?;
                Ok(json!({ "plane": id.0 }))
            },
        },
        CommandSpec {
            id: "axis_2planes",
            label: "Axis Through 2 Planes",
            menu: "Construct/Axis/Axis Through 2 Planes",
            shortcut: None,
            help: "A construction axis where two flat faces or planes meet",
            params: r#"{"a": {"body": id, "face": n} | {"plane": id} | "xy"…, "b": …}  (default: the two selected faces or planes)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let (a, b) = if p.has("a") {
                    (p.parse::<Value>("a")?, p.parse::<Value>("b")?)
                } else {
                    let mut v = selected_sources(s).into_iter();
                    match (v.next(), v.next()) {
                        (Some(a), Some(b)) => (a, b),
                        _ => return Err(EngineError::NotAvailable("select two flat faces or planes".into())),
                    }
                };
                let (pa, sa) = source_plane(s, &a)?;
                let (pb, sb) = source_plane(s, &b)?;
                let (na, nb) = (pa.normal(), pb.normal());
                let dir = na.cross(nb);
                if dir.length() < 1e-6 {
                    return Err(EngineError::NotAvailable("the two are parallel: they never meet".into()));
                }
                // The point of the line nearest both planes' middles.
                let (da, db) = (na.dot(pa.origin), nb.dot(pb.origin));
                let on = (nb.cross(dir) * da + dir.cross(na) * db) / dir.length_squared();
                let mid = (pa.origin + pb.origin) / 2.0;
                let d = dir.normalize();
                let origin = on + d * (mid - on).dot(d);
                add_axis(s, origin, d, sa.max(sb))
            },
        },
        CommandSpec {
            id: "plane_size",
            label: "Plane Size",
            menu: "",
            shortcut: None,
            help: "How big a construction plane is drawn",
            params: r#"{"plane": id, "size": mm}  (default: the selected planes)"#,
            modifies: true,
            enabled: |s| !s.doc.planes.is_empty(),
            run: |s, p| {
                let size = p.f64("size")?;
                if !(size.is_finite() && (1.0..=1e6).contains(&size)) {
                    return Err(EngineError::BadParams("size must be 1 mm to 1 km".into()));
                }
                let ids: Vec<ItemId> = if p.has("plane") {
                    vec![p.id("plane")?]
                } else {
                    s.selection.iter().filter_map(|k| if let Pick::Plane { plane } = *k { Some(plane) } else { None }).collect()
                };
                if ids.is_empty() {
                    return Err(EngineError::NotAvailable("select a plane".into()));
                }
                for id in &ids {
                    s.doc
                        .planes
                        .iter_mut()
                        .find(|q| q.id == *id)
                        .ok_or_else(|| EngineError::BadParams(format!("no plane {}", id.0)))?
                        .size = size;
                }
                Ok(json!({ "changed": ids.len() }))
            },
        },
        CommandSpec {
            id: "axis_perp_face",
            label: "Axis - Perp. to Face",
            menu: "Construct/Axis/Axis Perpendicular to Face",
            shortcut: None,
            help: "A construction axis through a flat face's center (or a point on it), along its normal",
            params: r#"{"body": id, "face": n, "at": [x,y,z]?} | {"sketch": id, "region": n}  (default: the selected face or region)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let region = if p.has("sketch") {
                    Some((p.id("sketch")?, p.f64_or("region", 0.0)? as u32))
                } else if p.has("body") {
                    None
                } else {
                    s.selection.iter().find_map(|pk| match *pk {
                        Pick::Region { sketch, region } => Some((sketch, region)),
                        _ => None,
                    })
                };
                if let Some((sketch, region)) = region {
                    let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
                    let r = sk.region(region).ok_or_else(|| EngineError::BadParams(format!("no region {region}")))?;
                    let (origin, dir) = (sk.plane.to_world(r.centroid), sk.plane.normal());
                    let len = (r.area.sqrt() * 2.0).max(20.0);
                    return add_axis(s, origin, dir, len);
                }
                let (body, face) = face_param(s, p)?;
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                let fp = s.kernel.face_plane(&b.shape, face)?.ok_or_else(|| EngineError::NotAvailable("pick a flat face".into()))?;
                let origin = if p.has("at") { p.vec3("at")? } else { fp.origin };
                let len = size_of_body(s, body)?;
                add_axis(s, origin, fp.normal, len)
            },
        },
        CommandSpec {
            id: "axis_along_edge",
            label: "Axis Along Edge",
            menu: "Construct/Axis/Axis Along Edge",
            shortcut: None,
            help: "A construction axis along a straight edge (or through a round edge's center)",
            params: r#"{"body": id, "edge": n}  (default: the selected edge)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let (body, edge) = if p.has("body") {
                    (p.id("body")?, p.f64("edge")? as u32)
                } else {
                    s.selection
                        .iter()
                        .find_map(|pk| match *pk {
                            Pick::Edge { body, edge } => Some((body, edge)),
                            _ => None,
                        })
                        .ok_or_else(|| EngineError::NotAvailable("select an edge".into()))?
                };
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                let ax = s
                    .kernel
                    .axis_of(&b.shape, shapecraft_kernel::SubShape::Edge(edge))?
                    .ok_or_else(|| EngineError::NotAvailable("pick a straight or round edge".into()))?;
                let len = size_of_body(s, body)?;
                add_axis(s, ax.origin, ax.dir, len)
            },
        },
        CommandSpec {
            id: "axis_cylinder",
            label: "Axis of Cylinder/Cone",
            menu: "Construct/Axis/Axis of Cylinder/Cone",
            shortcut: None,
            help: "A construction axis along a round face's axis",
            params: r#"{"body": id, "face": n}  (default: the selected face)"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let (body, face) = face_param(s, p)?;
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                if s.kernel.face_plane(&b.shape, face)?.is_some() {
                    return Err(EngineError::NotAvailable("pick a cylinder or cone face".into()));
                }
                let ax = s
                    .kernel
                    .axis_of(&b.shape, shapecraft_kernel::SubShape::Face(face))?
                    .ok_or_else(|| EngineError::NotAvailable("pick a cylinder or cone face".into()))?;
                let len = size_of_body(s, body)?;
                add_axis(s, ax.origin, ax.dir, len)
            },
        },
        CommandSpec {
            id: "axis_2points",
            label: "Axis Through 2 Points",
            menu: "Construct/Axis/Axis Through 2 Points",
            shortcut: None,
            help: "A construction axis through two points",
            params: r#"{"points": [[x,y,z], [x,y,z]]}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let pts: Vec<[f64; 3]> = p.parse("points")?;
                let [a, b] = pts.as_slice() else { return Err(EngineError::BadParams("give exactly two points".into())) };
                let (a, b) = (DVec3::from_array(*a), DVec3::from_array(*b));
                if !(a.is_finite() && b.is_finite()) || a.distance(b) < 1e-9 {
                    return Err(EngineError::BadParams("two different, finite points".into()));
                }
                add_axis(s, (a + b) / 2.0, b - a, a.distance(b) * 1.4)
            },
        },
    ]
}

fn face_param(s: &Session, p: &Params) -> Result<(ItemId, u32)> {
    if p.has("body") {
        return Ok((p.id("body")?, p.f64("face")? as u32));
    }
    s.selection
        .iter()
        .find_map(|pk| match *pk {
            Pick::Face { body, face } => Some((body, face)),
            _ => None,
        })
        .ok_or_else(|| EngineError::NotAvailable("select a face".into()))
}

fn size_of_body(s: &Session, body: ItemId) -> Result<f64> {
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let bb = s.kernel.bounds(&b.shape)?;
    Ok(((bb.max - bb.min).length() * 1.3).max(20.0))
}

fn add_axis(s: &mut Session, origin: DVec3, dir: DVec3, length: f64) -> Result<Value> {
    let dir = dir.try_normalize().ok_or_else(|| EngineError::BadParams("axis needs a direction".into()))?;
    if !(origin.is_finite() && length.is_finite()) || s.doc.axes.len() >= 10_000 {
        return Err(EngineError::BadParams("axis must be finite".into()));
    }
    let id = s.doc.alloc_id();
    let name = s.doc.next_axis_name();
    s.doc.axes.push(ConstructionAxis { id, name, origin, dir, length: length.clamp(1.0, 1e6), visible: true });
    s.selection = vec![Pick::Axis { axis: id }];
    Ok(json!({ "axis": id.0 }))
}

/// A plane to build from: its frame (normal pointing out of a face) and a size hint.
fn source_plane(s: &Session, v: &Value) -> Result<(SketchPlane, f64)> {
    if let Some(name) = v.as_str() {
        let plane = match name.to_ascii_lowercase().as_str() {
            "xy" | "top" => SketchPlane::XY,
            "xz" | "front" => SketchPlane::XZ,
            "yz" | "right" => SketchPlane::YZ,
            other => return Err(EngineError::BadParams(format!("unknown plane '{other}'"))),
        };
        return Ok((plane, scene_size(s)));
    }
    let p = Params::new(v);
    if p.has("plane") {
        let id = p.id("plane")?;
        let pl = s.doc.plane(id).ok_or_else(|| EngineError::BadParams(format!("no plane {}", id.0)))?;
        Ok((pl.plane, pl.size))
    } else if p.has("sketch") {
        let id = p.id("sketch")?;
        let sk = s.doc.sketch(id).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", id.0)))?;
        Ok((sk.plane, scene_size(s)))
    } else {
        let body = p.id("body")?;
        let face = p.f64("face")? as u32;
        let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
        let fp = s.kernel.face_plane(&b.shape, face)?.ok_or_else(|| EngineError::NotAvailable("pick a flat face".into()))?;
        let plane = SketchPlane::on_face(fp.origin, fp.normal, s.doc.legacy_face_axes)
            .ok_or_else(|| EngineError::BadParams("face has no usable normal".into()))?;
        let bb = s.kernel.bounds(&b.shape)?;
        Ok((plane, ((bb.max - bb.min).max_element() * 1.3).max(20.0)))
    }
}

/// A square that covers what's in the scene, so a new plane reads at a glance.
fn scene_size(s: &Session) -> f64 {
    s.doc.bounds().map_or(100.0, |(min, max)| ((max - min).max_element() * 1.3).max(40.0))
}

fn selected_sources(s: &Session) -> Vec<Value> {
    s.selection
        .iter()
        .filter_map(|p| match *p {
            Pick::Face { body, face } => Some(json!({ "body": body.0, "face": face })),
            Pick::Plane { plane } => Some(json!({ "plane": plane.0 })),
            Pick::Region { sketch, .. } | Pick::Sketch { sketch } => Some(json!({ "sketch": sketch.0 })),
            _ => None,
        })
        .collect()
}

pub(super) fn add_plane(s: &mut Session, plane: SketchPlane, size: f64) -> Result<ItemId> {
    if s.doc.planes.len() >= 10_000 {
        return Err(EngineError::BadParams("too many planes".into()));
    }
    if !(plane.origin.is_finite() && plane.x.is_finite() && plane.y.is_finite() && size.is_finite()) {
        return Err(EngineError::BadParams("plane must be finite".into()));
    }
    let id = s.doc.alloc_id();
    let name = s.doc.next_plane_name();
    s.doc.planes.push(ConstructionPlane { id, name, plane, size: size.clamp(1.0, 1e6), visible: true });
    s.selection = vec![Pick::Plane { plane: id }];
    Ok(id)
}

fn offset_plane(s: &mut Session, p: &Params) -> Result<Value> {
    let from = if p.has("from") {
        p.parse::<Value>("from")?
    } else {
        selected_sources(s)
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::NotAvailable("select a flat face, plane or sketch to offset from".into()))?
    };
    let distance = p.f64_or("distance", 0.0)?;
    if !distance.is_finite() || distance.abs() > 1e6 {
        return Err(EngineError::BadParams("distance must be a finite number".into()));
    }
    let (mut plane, size) = source_plane(s, &from)?;
    plane.origin += plane.normal() * distance;
    let id = add_plane(s, plane, size)?;
    Ok(json!({ "plane": id.0 }))
}

fn midplane(s: &mut Session, p: &Params) -> Result<Value> {
    let (a, b) = if p.has("a") {
        (p.parse::<Value>("a")?, p.parse::<Value>("b")?)
    } else {
        let mut v = selected_sources(s).into_iter();
        match (v.next(), v.next()) {
            (Some(a), Some(b)) => (a, b),
            _ => return Err(EngineError::NotAvailable("select two parallel flat faces or planes".into())),
        }
    };
    let (pa, sa) = source_plane(s, &a)?;
    let (pb, sb) = source_plane(s, &b)?;
    let (na, nb) = (pa.normal(), pb.normal());
    // Not parallel: the plane through the line where they meet, halving the angle on the inside of the part (the
    // reference does this for two faces at a corner, s52r-018).
    let line = na.cross(nb);
    if line.length() > 1e-6 {
        let dir = line.normalize();
        // A point on both planes: the one nearest the middle of their origins.
        let mid = (pa.origin + pb.origin) / 2.0;
        let (da, db) = (na.dot(pa.origin), nb.dot(pb.origin));
        let m = glam::DMat3::from_cols(na, nb, dir).transpose();
        let Some(inv) = (m.determinant().abs() > 1e-12).then(|| m.inverse()) else {
            return Err(EngineError::NotAvailable("the two faces don't meet".into()));
        };
        let on_line = inv * DVec3::new(da, db, dir.dot(mid));
        let normal = (na - nb).normalize();
        let x = dir;
        let y = normal.cross(x);
        let plane = shapecraft_doc::SketchPlane { origin: on_line, x, y };
        let id = add_plane(s, plane, sa.max(sb))?;
        return Ok(json!({ "plane": id.0 }));
    }
    // Halfway along the normal; keep the first plane's frame, centered between the two.
    let gap = (pb.origin - pa.origin).dot(na);
    let mut plane = pa;
    let mid_in_plane = (pa.origin + pb.origin) / 2.0;
    plane.origin = mid_in_plane - na * (mid_in_plane - pa.origin).dot(na) + na * gap / 2.0;
    let id = add_plane(s, plane, sa.max(sb))?;
    Ok(json!({ "plane": id.0 }))
}

fn plane_3points(s: &mut Session, p: &Params) -> Result<Value> {
    let pts: Vec<[f64; 3]> = p.parse("points")?;
    let [a, b, c] = pts.as_slice() else { return Err(EngineError::BadParams("give exactly three points".into())) };
    let (a, b, c) = (DVec3::from_array(*a), DVec3::from_array(*b), DVec3::from_array(*c));
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return Err(EngineError::BadParams("points must be finite".into()));
    }
    let n = (b - a).cross(c - a);
    let size = a.distance(b).max(a.distance(c)).max(b.distance(c));
    if n.length() < 1e-9 * size.max(1.0).powi(2) {
        return Err(EngineError::NotAvailable("the three points are in a line".into()));
    }
    let x = (b - a).normalize();
    let y = n.normalize().cross(x);
    let plane = SketchPlane { origin: (a + b + c) / 3.0, x, y };
    let id = add_plane(s, plane, (size * 1.5).max(20.0))?;
    Ok(json!({ "plane": id.0 }))
}

/// The edge named in the params (or the selected one) and its points, start to end.
fn edge_line(s: &Session, p: &Params) -> Result<(ItemId, u32, Vec<DVec3>)> {
    let (body, edge) = if p.has("body") {
        (p.id("body")?, p.f64("edge")? as u32)
    } else {
        s.selection
            .iter()
            .find_map(|pk| match *pk {
                Pick::Edge { body, edge } => Some((body, edge)),
                _ => None,
            })
            .ok_or_else(|| EngineError::NotAvailable("select an edge".into()))?
    };
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let k = b.mesh.edge_ids.iter().position(|e| *e == edge).ok_or_else(|| EngineError::BadParams(format!("no edge {edge}")))?;
    let pts: Vec<DVec3> =
        b.mesh.edges.get(k).map(|l| l.iter().map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)).collect()).unwrap_or_default();
    if pts.len() < 2 {
        return Err(EngineError::NotAvailable("that edge has no length".into()));
    }
    Ok((body, edge, pts))
}

fn plane_through_edge(s: &mut Session, p: &Params) -> Result<Value> {
    let (body, edge, pts) = edge_line(s, p)?;
    let angle = p.f64_or("angle", 0.0)?;
    if !angle.is_finite() {
        return Err(EngineError::BadParams("angle must be a finite number".into()));
    }
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let axis = s
        .kernel
        .axis_of(&b.shape, shapecraft_kernel::SubShape::Edge(edge))?
        .filter(|_| pts.len() == 2 || pts.windows(3).all(|w| (w[1] - w[0]).cross(w[2] - w[1]).length() < 1e-6))
        .ok_or_else(|| EngineError::NotAvailable("pick a straight edge".into()))?;
    let x = axis.dir.normalize();
    let (a, e) = (pts[0], pts[pts.len() - 1]);
    let (mid, len) = ((a + e) / 2.0, a.distance(e));
    // At 0° the plane lies on the edge's first face, centered on the edge (as observed, 345).
    let k = b.mesh.edge_ids.iter().position(|v| *v == edge).unwrap_or(0);
    let face = b.mesh.edge_faces.get(k).map(|f| f[0]).filter(|f| *f != u32::MAX);
    let fp = face.and_then(|f| s.kernel.face_plane(&b.shape, f).ok().flatten());
    let into = match fp {
        Some(fp) => {
            let v = fp.origin - mid;
            (v - x * v.dot(x)).normalize_or(fp.normal.cross(x))
        }
        None => x.any_orthonormal_vector(),
    };
    let y = glam::DQuat::from_axis_angle(x, angle.to_radians()) * into;
    let size = (len * 1.3).max(10.0);
    let plane = SketchPlane { origin: mid, x, y };
    let id = add_plane(s, plane, size)?;
    Ok(json!({ "plane": id.0 }))
}

fn plane_perp_edge(s: &mut Session, p: &Params) -> Result<Value> {
    let (_, _, pts) = edge_line(s, p)?;
    let point = p.vec3("point")?;
    if !point.is_finite() {
        return Err(EngineError::BadParams("point must be finite".into()));
    }
    // The edge's direction where it passes nearest the point.
    let mut best = (f64::INFINITY, pts[1] - pts[0]);
    for w in pts.windows(2) {
        let d = w[1] - w[0];
        let t = if d.length_squared() > 0.0 { ((point - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let gap = point.distance(w[0] + d * t);
        if gap < best.0 && d.length_squared() > 0.0 {
            best = (gap, d);
        }
    }
    let n = best.1.try_normalize().ok_or_else(|| EngineError::NotAvailable("that edge has no length".into()))?;
    let x = n.any_orthonormal_vector();
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let plane = SketchPlane { origin: point, x, y: n.cross(x) };
    let id = add_plane(s, plane, (total * 0.6).max(10.0))?;
    Ok(json!({ "plane": id.0 }))
}

fn plane_on_curve(s: &mut Session, p: &Params) -> Result<Value> {
    let (_, _, pts) = edge_line(s, p)?;
    let at = p.f64_or("at", 0.0)?;
    if !at.is_finite() {
        return Err(EngineError::BadParams("at must be a number from 0 to 1".into()));
    }
    let at = at.clamp(0.0, 1.0);
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * at;
    let (mut point, mut tangent) = (pts[0], pts[1] - pts[0]);
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        tangent = w[1] - w[0];
        if l >= left && l > 0.0 {
            point = w[0].lerp(w[1], left / l);
            break;
        }
        left -= l;
        point = w[1];
    }
    let n = tangent.try_normalize().ok_or_else(|| EngineError::NotAvailable("that edge has no length".into()))?;
    let x = n.any_orthonormal_vector();
    let plane = SketchPlane { origin: point, x, y: n.cross(x) };
    let id = add_plane(s, plane, (total * 0.4).max(5.0))?;
    Ok(json!({ "plane": id.0 }))
}

//! The Shapr3D round trip: importing a `.shapr` project (its extrudes and finishes redone as our steps) and
//! exporting back into its history or as a fresh project.

use serde_json::{Value, json};
use shapecraft_doc::{Pick, Sketch, SketchEntity, SketchPlane};

use super::export::write_file;
use super::import::{add_bodies, read_capped};
use super::path_param;
use crate::{EngineError, Params, Result, Session};

/// A Shapr3D project's current solids as bodies, named and foldered like its Items list.
pub(super) fn import_shapr(s: &mut Session, path: &std::path::Path) -> Result<Value> {
    let bad = |e: String| EngineError::BadParams(e);
    let bytes = read_capped(path)?;
    let file = shapecraft_shapr::ShaprFile::parse(&bytes).map_err(|e| bad(e.to_string()))?;
    let project = shapecraft_shapr::Project::read(&file).map_err(|e| bad(e.to_string()))?;
    if project.bodies.len() > 10_000 {
        return Err(bad("that project has too many bodies to import".into()));
    }
    let mut ids = Vec::new();
    // Bodies whose geometry the kernel couldn't rebuild: named in the result (the app says how many) rather than
    // losing the whole project to one of them.
    let mut failed: Vec<String> = project.unreadable.clone();
    let mut folders: Vec<(String, Vec<shapecraft_doc::ItemId>)> = Vec::new();
    let mut links = Vec::new();
    // A body Shapr3D extruded from one of its sketches can come back as an Extrude step of ours (after this
    // import), so its distance and draft stay editable and go back into Shapr3D's step. Only for the project the
    // exports go back into, and only bodies outside folders (for now).
    let linking = s.doc.shapr.is_none();
    let made_by = |node: i64| project.bodies.iter().filter(|b| b.step == Some(node)).count();
    let mut deferred = Vec::new();
    let mut sketch_ids = Vec::new();
    let mut sketch_links = Vec::new();
    // Shapr3D curve id → (our sketch, entity index), to find an extrude's profile among our regions.
    let mut curve_entity: std::collections::HashMap<i64, (shapecraft_doc::ItemId, u32)> = std::collections::HashMap::new();
    // Items come in the order their steps made them, so the Items list (newest first) reads as Shapr3D's does.
    let at = |node: Option<i64>| node.and_then(|n| project.steps.iter().position(|st| st.node == n)).unwrap_or(usize::MAX);
    let mut order: Vec<(usize, std::result::Result<&shapecraft_shapr::Body, &shapecraft_shapr::Sketch>)> =
        project.bodies.iter().map(|b| (at(b.step), Ok(b))).collect();
    order.extend(project.sketches.iter().take(10_000).map(|k| (at(project.sketch_step(k.id)), Err(k))));
    order.sort_by_key(|(i, _)| *i);
    for (_, item) in order {
        match item {
            Ok(body) => {
                let extrude = body
                    .step
                    .filter(|_| linking && body.folder.is_empty())
                    .and_then(|n| project.extrudes.iter().find(|e| e.node == n && made_by(n) == 1));
                match extrude {
                    Some(e) => deferred.push((body, e)),
                    None => match add_shapr_body(s, body, &mut folders, &mut links) {
                        Ok(made) => ids.extend(made),
                        Err(e) => failed.push(format!("{}: {e}", body.name.as_deref().unwrap_or("a body"))),
                    },
                }
            }
            Err(sk) => {
                let v = |a: [f64; 3]| glam::DVec3::from_array(a);
                let (Some(n), Some(x)) = (v(sk.normal).try_normalize(), v(sk.u).try_normalize()) else { continue };
                let plane = SketchPlane { origin: v(sk.origin), x, y: n.cross(x) };
                let id = s.doc.alloc_id();
                let mut sketch = Sketch::new(id, sk.name.chars().take(200).collect(), plane, None);
                sketch.visible = !sk.hidden;
                for curve in &sk.curves {
                    if let Some((entity, construction)) = sketch_entity(curve) {
                        if construction {
                            sketch.construction.insert(sketch.entities.len() as u32);
                        }
                        curve_entity.insert(curve_id(curve), (id, sketch.entities.len() as u32));
                        sketch.entities.push(entity);
                    }
                }
                sketch_links.push(shapecraft_doc::ShaprSketch { sketch: id, id: sk.id, circles: circles_of(&sketch) });
                s.doc.sketches.push(sketch);
                sketch_ids.push(id.0);
            }
        }
    }
    let mut made_folders = Vec::new();
    for (name, items) in folders {
        let id = s.doc.alloc_id();
        s.doc.folders.push(shapecraft_doc::Folder { id, name: name.chars().take(200).collect(), items });
        made_folders.push(id.0);
    }
    s.refresh_regions();
    let mut redone = 0;
    for (body, e) in deferred {
        let sketch = sketch_links.iter().find(|l| l.id == e.sketch).map(|l| l.sketch);
        match sketch.and_then(|k| matching_region(s, k, body, e).map(|r| (k, r))) {
            Some((sketch, (region, subs, size))) => {
                redone += 1;
                if !s.replaying {
                    s.followups.push(Followup {
                        sketch,
                        region,
                        distance: e.distance,
                        draft: e.draft,
                        symmetric: e.symmetric,
                        name: body.name.clone(),
                        color: body.color,
                        hidden: body.hidden,
                        node: e.node,
                        fillet: e.fillet.as_ref().map(|f| Round {
                            node: f.node,
                            size,
                            kind: f.kind,
                            subs,
                            total: (f.kind == shapecraft_shapr::Finish::OffsetTotal).then_some(f.radius - size),
                        }),
                    });
                }
            }
            None => match add_shapr_body(s, body, &mut Vec::new(), &mut links) {
                Ok(made) => ids.extend(made),
                Err(e) => failed.push(format!("{}: {e}", body.name.as_deref().unwrap_or("a body"))),
            },
        }
    }
    // A file Shapr3D hasn't opened since we wrote it keeps no solids for its own steps (Shapr3D rebuilds them): its
    // simple extrudes are redone from their profiles instead, or those bodies would be missing.
    if linking && !project.bodies.iter().any(|b| b.partition.is_some()) {
        for e in project.extrudes.iter().filter(|e| !e.holes && !e.used_later && !project.bodies.iter().any(|b| b.step == Some(e.node))) {
            let Some((sketch, region)) = profile_region(s, e, &curve_entity) else { continue };
            redone += 1;
            if !s.replaying {
                let item = e.item.as_ref();
                s.followups.push(Followup {
                    sketch,
                    region,
                    distance: e.distance,
                    draft: e.draft,
                    symmetric: e.symmetric,
                    name: item.map(|i| i.name.clone()),
                    color: item.and_then(|i| i.color),
                    hidden: item.is_some_and(|i| i.hidden),
                    node: e.node,
                    fillet: None,
                });
            }
        }
    }
    // The first Shapr3D project brought into this one is remembered, so exports go back into its history.
    let linked = linking;
    if linked {
        let mut refs = Vec::new();
        for (id, step) in links {
            let Some(b) = s.doc.body(id) else { continue };
            let (color, hidden) = (b.appearance.as_ref().map(|a| a.color), !b.visible);
            refs.push(shapecraft_doc::ShaprBody {
                body: id,
                step,
                name: b.name.clone(),
                fingerprint: fingerprint(s, &b.shape.clone())?,
                color,
                hidden,
                extrude: None,
            });
        }
        s.doc.shapr = Some(shapecraft_doc::ShaprLink { file: std::sync::Arc::new(bytes), bodies: refs, sketches: sketch_links });
    }
    s.selection = ids.iter().map(|&body| Pick::Body { body }).collect();
    // A file we wrote brings its own steps back, when it opens into an empty project (as Shapr3D's "New Project").
    let empty = s.doc.history.is_empty() && s.doc.sketches.len() == sketch_ids.len() && s.doc.planes.is_empty() && s.doc.axes.is_empty();
    let adopting = !s.replaying && empty && failed.is_empty();
    if adopting && let Some(h) = project.own_history.as_deref().and_then(|j| serde_json::from_str::<OwnHistory>(j).ok()) {
        s.adopt = Some(h);
    }
    // Shapr3D's steps go under the Import step (bookkeeping steps without a name are left out, as Shapr3D lists them).
    s.shapr_steps =
        project.steps.iter().filter(|st| !st.name.is_empty()).take(100_000).map(|st| [st.name.clone(), st.op.clone()]).collect();
    Ok(
        json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folders": made_folders, "sketches": sketch_ids, "steps": project.steps.len(), "linked": linked, "extrudes": redone, "unreadable": failed }),
    )
}

/// One of a Shapr3D project's bodies as it is, named, coloured, shown or hidden, and noted for its folder and link.
fn add_shapr_body(
    s: &mut Session,
    body: &shapecraft_shapr::Body,
    folders: &mut Vec<(String, Vec<shapecraft_doc::ItemId>)>,
    links: &mut Vec<(shapecraft_doc::ItemId, i64)>,
) -> Result<Vec<shapecraft_doc::ItemId>> {
    let made = match &body.mesh {
        // A facet body (an imported STL kept as a mesh in the file): it stays a mesh body here too.
        Some(m) if body.brep.faces.is_empty() => {
            let (pts, idx) = crate::formats::weld(&m.points, &m.triangles);
            match shapecraft_doc::Facets::new(pts, &idx) {
                Some(f) => {
                    let name = body.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "Mesh body 01".into());
                    vec![crate::cmd::model::add_mesh_body(s, f, name)?]
                }
                None => Vec::new(),
            }
        }
        _ => add_bodies(s, vec![s.kernel.from_brep(&body.brep)?], body.name.as_deref())?,
    };
    if let (Some(step), [id]) = (body.step, made.as_slice()) {
        links.push((*id, step));
    }
    let folder = body.folder.join(" / ");
    for &id in &made {
        if let (Some(c), Some(b)) = (body.color, s.doc.body_mut(id)) {
            b.appearance = Some(imported_colour(c));
        }
        if let (true, Some(b)) = (body.hidden, s.doc.body_mut(id)) {
            b.visible = false;
        }
        if !folder.is_empty() {
            match folders.iter_mut().find(|f| f.0 == folder) {
                Some(f) => f.1.push(id),
                None => folders.push((folder.clone(), vec![id])),
            }
        }
    }
    Ok(made)
}

fn imported_colour(c: [f32; 3]) -> shapecraft_doc::Appearance {
    shapecraft_doc::Appearance { name: "Imported Color".into(), color: c, ..Default::default() }
}

/// The region of our sketch whose extrude, as Shapr3D's step says, makes exactly the body Shapr3D made; for a
/// rounded extrude the edges our fillet rounds (empty: all of them) and its size, for an offset one the face pushed
/// and by how much.
fn matching_region(
    s: &Session,
    sketch: shapecraft_doc::ItemId,
    body: &shapecraft_shapr::Body,
    e: &shapecraft_shapr::ExtrudeStep,
) -> Option<(u32, Vec<u32>, f64)> {
    use shapecraft_shapr::Finish;
    let sk = s.doc.sketch(sketch)?;
    let target = s.kernel.from_brep(&body.brep).ok()?;
    let want = fingerprint(s, &target).ok()?;
    let dir = sk.plane.normal() * e.distance;
    // Shapr3D's drafted curved walls come out a little leaner than an exact taper (n4: 394.212 mm³ against
    // 394.237), so close within 0.1% counts.
    // A rounded extrude is compared without its face count: corners where three fillets meet may be patched
    // with other faces than Shapr3D's.
    let skip = if e.fillet.as_ref().is_some_and(|f| matches!(f.kind, Finish::Fillet | Finish::Chamfer)) { Some(2) } else { None };
    let same = |a: &[f64], b: &[f64]| {
        a.len() == b.len()
            && a.iter().zip(b).enumerate().all(|(i, (x, y))| Some(i) == skip || (x - y).abs() <= 1e-3 * x.abs().max(y.abs()).max(1.0))
    };
    sk.regions().iter().take(64).find_map(|r| {
        let block = crate::cmd::model::extrude_block(s, std::slice::from_ref(&r.face), dir, e.draft.to_radians(), e.symmetric).ok()?;
        let Some(f) = &e.fillet else {
            return fingerprint(s, &block).is_ok_and(|got| same(&got, &want)).then_some((r.index, Vec::new(), 0.0));
        };
        let matches = |shape: shapecraft_kernel::Result<shapecraft_kernel::Shape>| {
            shape.ok().and_then(|x| fingerprint(s, &x).ok()).is_some_and(|got| same(&got, &want))
        };
        if matches!(f.kind, Finish::Offset | Finish::OffsetTotal) {
            // One flat face pushed: by the step's distance, or (a total thickness) by what the volume gained over
            // the face's area says.
            let grown = want.first()? - s.kernel.mass_props(&block).ok()?.volume;
            return (0..s.kernel.face_count(&block).ok()?.min(256)).find_map(|i| {
                s.kernel.face_plane(&block, i).ok()??;
                let d = match f.kind {
                    Finish::Offset => f.radius,
                    _ => grown / s.kernel.measure(&block, shapecraft_kernel::SubShape::Face(i)).ok()?.0,
                };
                (d.is_finite() && d != 0.0 && matches(s.kernel.offset_faces(&block, &[i], d))).then_some((r.index, vec![i], d))
            });
        }
        let count = s.kernel.edge_count(&block).ok()?.min(4096);
        let chamfer = f.kind == Finish::Chamfer;
        let round = |edges: &[u32]| match (chamfer, edges.is_empty()) {
            (true, true) => s.kernel.chamfer_edges(&block, &(0..count).collect::<Vec<_>>(), f.radius),
            (true, false) => s.kernel.chamfer_edges(&block, edges, f.radius),
            (false, true) => s.kernel.fillet_all_edges(&block, f.radius),
            (false, false) => s.kernel.fillet_edges(&block, edges, f.radius),
        };
        if matches(round(&[])) {
            return Some((r.index, Vec::new(), f.radius));
        }
        // Chosen edges: a rounded or bevelled edge of ours lies off Shapr3D's body (by about 0.4 r, or 0.7 of a
        // chamfer's distance, on a square corner).
        let edges: Vec<u32> = (0..count)
            .filter(|&i| {
                s.kernel
                    .distance(&block, shapecraft_kernel::SubShape::Edge(i), &target, shapecraft_kernel::SubShape::Whole)
                    .is_ok_and(|d| d.distance > 0.1 * f.radius)
            })
            .collect();
        (!edges.is_empty() && matches(round(&edges))).then_some((r.index, edges, f.radius))
    })
}

/// The region of our sketch that is an extrude's profile: the one cell its outline's curves alone enclose, found
/// among the sketch's regions by area and centre.
fn profile_region(
    s: &Session,
    e: &shapecraft_shapr::ExtrudeStep,
    curve_entity: &std::collections::HashMap<i64, (shapecraft_doc::ItemId, u32)>,
) -> Option<(shapecraft_doc::ItemId, u32)> {
    let picked: Vec<(shapecraft_doc::ItemId, u32)> = e.outline.iter().map(|c| curve_entity.get(c).copied()).collect::<Option<_>>()?;
    let sketch = picked.first()?.0;
    let sk = s.doc.sketch(sketch)?;
    let mut alone = Sketch::new(sk.id, String::new(), sk.plane, None);
    alone.entities = picked.iter().filter_map(|(_, i)| sk.entities.get(*i as usize).cloned()).collect();
    let cells = crate::compute_regions(s.kernel.as_ref(), &alone);
    let [cell] = cells.as_slice() else { return None };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0);
    let region = sk
        .regions()
        .iter()
        .find(|r| close(r.area, cell.area) && close(r.centroid.x, cell.centroid.x) && close(r.centroid.y, cell.centroid.y))?;
    Some((sketch, region.index))
}

/// A Shapr3D sketch curve's id.
fn curve_id(c: &shapecraft_shapr::SketchCurve) -> i64 {
    use shapecraft_shapr::SketchCurve as C;
    match c {
        C::Circle { id, .. } | C::Line { id, .. } | C::Arc { id, .. } | C::Spline { id, .. } | C::Other { id, .. } => *id,
    }
}

/// A Shapr3D Extrude to redo as our own step once the import is logged.
pub(crate) struct Followup {
    sketch: shapecraft_doc::ItemId,
    region: u32,
    distance: f64,
    draft: f64,
    symmetric: bool,
    name: Option<String>,
    color: Option<[f32; 3]>,
    hidden: bool,
    node: i64,
    /// A Shapr3D Fillet, Chamfer or Offset Face on the extruded body.
    fillet: Option<Round>,
}

/// A Shapr3D Fillet (radius) or Chamfer (distance) on these edges of an extruded body (empty: every edge), or an
/// Offset Face (our distance) of these faces.
pub(crate) struct Round {
    node: i64,
    size: f64,
    kind: shapecraft_shapr::Finish,
    subs: Vec<u32>,
    /// A total-thickness offset: Shapr3D's total less our distance.
    total: Option<f64>,
}

/// Redoes a Shapr3D import's extrudes as steps of ours (each its own History step, named and coloured as in
/// Shapr3D) and links each body to the Extrude it stands for.
pub(crate) fn run_followups(s: &mut Session, todo: Vec<Followup>) {
    for f in todo {
        let sides = if f.symmetric { "symmetric" } else { "one" };
        let params =
            json!({ "sketch": f.sketch.0, "region": f.region, "distance": f.distance, "draft": f.draft, "sides": sides, "result": "new" });
        let Some(body) = s.execute("extrude", &params).ok().and_then(|o| o.value["body"].as_u64()).map(shapecraft_doc::ItemId) else {
            continue;
        };
        let step = s.doc.history.last().map(|h| h.label.clone()).unwrap_or_default();
        let fillet = match &f.fillet {
            Some(r) => match match r.kind {
                shapecraft_shapr::Finish::Chamfer => s.execute("chamfer", &json!({ "body": body.0, "distance": r.size, "edges": r.subs })),
                shapecraft_shapr::Finish::Fillet => s.execute("fillet", &json!({ "body": body.0, "radius": r.size, "edges": r.subs })),
                _ => s.execute("offset_face", &json!({ "body": body.0, "faces": r.subs, "distance": r.size })),
            } {
                Ok(_) => Some((r.node, s.doc.history.last().map(|h| h.label.clone()).unwrap_or_default())),
                // Not rounded here: the body can't stand for Shapr3D's, so it goes back as a plain one.
                Err(_) => None,
            },
            None => None,
        };
        if let Some(name) = &f.name {
            let _ = s.execute("rename", &json!({ "item": body.0, "name": name }));
        }
        if let Some(c) = f.color {
            let a = imported_colour(c);
            let _ = s.execute(
                "set_appearance",
                &json!({ "items": [body.0], "appearance": { "name": a.name, "color": a.color, "metal": a.metal, "gloss": a.gloss } }),
            );
        }
        if f.hidden {
            let _ = s.execute("show_hide", &json!({ "items": [body.0], "visible": false }));
        }
        let Some(b) = s.doc.body(body).map(|b| (b.name.clone(), b.appearance.as_ref().map(|a| a.color), !b.visible)) else {
            continue;
        };
        let Ok(shape) = crate::cmd::model::working_shape(s, body, false) else { continue };
        let b = (b.0, shape, b.1, b.2);
        let Ok(fp) = fingerprint(s, &b.1) else { continue };
        if let Some(link) = s.doc.shapr.as_mut() {
            let extrude = (f.fillet.is_none() || fillet.is_some()).then_some(shapecraft_doc::ShaprExtrude {
                node: f.node,
                step,
                fillet,
                total: f.fillet.as_ref().and_then(|r| r.total),
            });
            link.bodies.push(shapecraft_doc::ShaprBody {
                body,
                step: f.node,
                name: b.0,
                fingerprint: fp,
                color: b.2,
                hidden: b.3,
                extrude,
            });
        }
    }
}

/// The distance and draft of our step standing for a Shapr3D Extrude, and the size of our fillet, chamfer or face
/// offset standing for the Shapr3D step after it (as Shapr3D measures it), when those steps alone shape the body (so
/// the change can go into Shapr3D's steps).
fn extrude_edit(s: &Session, r: &shapecraft_doc::ShaprBody) -> Option<(f64, f64, Option<f64>)> {
    let x = r.extrude.as_ref()?;
    let live = |label: &str, kind: &str| {
        s.doc.history.iter().position(|h| h.label == label && h.kind == kind && !h.suppressed && h.error.is_none())
    };
    let at = live(&x.step, "extrude")?;
    let fillet_at = match &x.fillet {
        Some((_, label)) => Some(live(label, "fillet").or_else(|| live(label, "chamfer")).or_else(|| live(label, "offset_face"))?),
        None => None,
    };
    let related = s.related_steps(&[r.body], &[])?;
    if related.iter().enumerate().any(|(i, &on)| on && i != at && Some(i) != fillet_at && !s.doc.history[i].quiet) {
        return None;
    }
    let field = |at: usize, command: &str, key: &str| {
        let feature = crate::history::feature(command)?;
        feature.fields.iter().find(|f| f.key == key).map(|f| crate::history::value(&s.doc.history[at], f, &s.doc.variables))
    };
    let radius = match fillet_at {
        Some(i) if s.doc.history[i].kind == "chamfer" => Some(field(i, "chamfer", "distance")?),
        Some(i) if s.doc.history[i].kind == "offset_face" => Some(field(i, "offset_face", "distance")? + x.total.unwrap_or(0.0)),
        Some(i) => Some(field(i, "fillet", "radius")?),
        None => None,
    };
    Some((field(at, "extrude", "distance")?, field(at, "extrude", "draft")?, radius))
}

/// A sketch's circles as (center, radius), in entity order.
fn circles_of(sk: &Sketch) -> Vec<([f64; 2], f64)> {
    sk.entities
        .iter()
        .filter_map(|e| match e {
            SketchEntity::Circle { center, radius } => Some((center.to_array(), *radius)),
            _ => None,
        })
        .collect()
}

/// One of Shapr3D's sketch curves as our entity, with its construction flag.
fn sketch_entity(curve: &shapecraft_shapr::SketchCurve) -> Option<(SketchEntity, bool)> {
    use shapecraft_shapr::SketchCurve as C;
    let v = glam::DVec2::from_array;
    match curve {
        C::Circle { center, radius, construction, .. } => {
            Some((SketchEntity::Circle { center: v(*center), radius: *radius }, *construction))
        }
        C::Line { a, b, construction, .. } => Some((SketchEntity::Line { a: v(*a), b: v(*b) }, *construction)),
        C::Arc { center, start, end, construction, .. } => {
            let (c, s, e) = (v(*center), v(*start), v(*end));
            let r = (s - c).length();
            let a0 = (s - c).y.atan2((s - c).x);
            let mut a1 = (e - c).y.atan2((e - c).x);
            if a1 <= a0 + 1e-12 {
                a1 += std::f64::consts::TAU;
            }
            let m = (a0 + a1) / 2.0;
            (r > 1e-9).then(|| (SketchEntity::Arc { a: s, mid: c + glam::DVec2::new(m.cos(), m.sin()) * r, b: e }, *construction))
        }
        C::Spline { points, poles, construction, .. } => {
            let e = if points.len() >= 2 {
                SketchEntity::Spline { points: points.iter().map(|p| v(*p)).collect() }
            } else {
                SketchEntity::ControlSpline { poles: poles.iter().map(|p| v(*p)).collect() }
            };
            Some((e, *construction))
        }
        C::Other { .. } => None,
    }
}

/// A sketch as Shapr3D's sketch rows describe it: lines, arcs, circles and fit splines (rectangles and polygons as
/// their sides).
fn shapr_sketch(sk: &Sketch) -> shapecraft_shapr::Sketch {
    use shapecraft_shapr::SketchCurve as C;
    let normal = sk.plane.x.cross(sk.plane.y).normalize_or_zero();
    let mut curves = Vec::new();
    for (i, e) in sk.entities.iter().enumerate() {
        let construction = sk.construction.contains(&(i as u32));
        let mut sides = |corners: &[glam::DVec2]| {
            for k in 0..corners.len() {
                curves.push(C::Line { id: 0, a: corners[k].to_array(), b: corners[(k + 1) % corners.len()].to_array(), construction });
            }
        };
        match e {
            SketchEntity::Circle { center, radius } => {
                curves.push(C::Circle { id: 0, center: center.to_array(), radius: *radius, construction })
            }
            SketchEntity::Line { a, b } => curves.push(C::Line { id: 0, a: a.to_array(), b: b.to_array(), construction }),
            SketchEntity::Rectangle { min, max } => {
                sides(&[*min, glam::DVec2::new(max.x, min.y), *max, glam::DVec2::new(min.x, max.y)]);
            }
            SketchEntity::Polygon { center, radius, sides: n, angle } => {
                let n = (*n).clamp(3, 1000);
                let corners: Vec<glam::DVec2> = (0..n)
                    .map(|k| *center + glam::DVec2::from_angle(angle + std::f64::consts::TAU * k as f64 / n as f64) * *radius)
                    .collect();
                sides(&corners);
            }
            SketchEntity::Arc { a, mid, b } => {
                let Some((c, _)) = shapecraft_doc::circle_through(*a, *mid, *b) else { continue };
                // Shapr3D's arcs run counter-clockwise from start to end.
                let ccw = (*mid - *a).perp_dot(*b - *mid) > 0.0;
                let (start, end) = if ccw { (a, b) } else { (b, a) };
                curves.push(C::Arc { id: 0, center: c.to_array(), start: start.to_array(), end: end.to_array(), construction });
            }
            SketchEntity::Spline { points } => {
                curves.push(C::Spline { id: 0, points: points.iter().map(|p| p.to_array()).collect(), poles: Vec::new(), construction })
            }
            _ => {}
        }
    }
    shapecraft_shapr::Sketch {
        id: 0,
        name: sk.name.clone(),
        hidden: !sk.visible,
        origin: sk.plane.origin.to_array(),
        normal: normal.to_array(),
        u: sk.plane.x.to_array(),
        curves,
    }
}

/// A body's colour and visibility as Shapr3D keeps them.
fn look(b: &shapecraft_doc::Body) -> shapecraft_shapr::Look {
    shapecraft_shapr::Look { color: b.appearance.as_ref().map(|a| a.color), hidden: !b.visible }
}

/// Volume, area, face count, centroid and bounding box: enough to tell whether a body changed or moved.
fn fingerprint(s: &Session, shape: &shapecraft_kernel::Shape) -> Result<Vec<f64>> {
    let m = s.kernel.mass_props(shape)?;
    let faces = s.kernel.face_count(shape)? as f64;
    let b = s.kernel.bounds(shape)?;
    Ok([m.volume, m.area, faces].into_iter().chain(m.centroid.to_array()).chain(b.min.to_array()).chain(b.max.to_array()).collect())
}

/// The Items folder path a body sits in ("A / B" folders from imports become nested folders).
fn folder_path(s: &Session, body: shapecraft_doc::ItemId) -> Vec<String> {
    s.doc.folders.iter().find(|f| f.items.contains(&body)).map(|f| f.name.split(" / ").map(str::to_string).collect()).unwrap_or_default()
}

/// A body's solids as Parasolid parts (neutral binary).
pub(super) fn parasolid_parts(s: &Session, shape: &shapecraft_kernel::Shape) -> Result<Vec<Vec<u8>>> {
    s.kernel
        .to_brep(shape)?
        .iter()
        .map(|b| {
            shapecraft_xt::write_body(b, shapecraft_xt::Format::Binary)
                .map_err(|e| EngineError::BadParams(format!("can't write this body as Parasolid: {e}")))
        })
        .collect()
}

/// A Shapr3D project. A project that came from one goes back into that file's own history: bodies changed
/// here replace theirs (a Delete step, then an Import) unless they only moved (a Move/Rotate step, or the import
/// placed further), renamed ones are renamed, new ones are imported, and
/// everything Shapr3D made stays parametric. Other projects become a fresh Shapr3D project.
pub(super) fn export_shapr(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let bad = |e: shapecraft_shapr::Error| EngineError::BadParams(e.to_string());
    let link = s.doc.shapr.clone().filter(|l| !l.file.is_empty());
    let mut w = match &link {
        Some(l) => shapecraft_shapr::Writer::patch(&shapecraft_shapr::ShaprFile::parse(&l.file).map_err(bad)?).map_err(bad)?,
        None => shapecraft_shapr::Writer::fresh().map_err(bad)?,
    };
    let mut kept = std::collections::HashSet::new();
    let (mut deleted, mut renamed, mut moved, mut restyled) = (0, 0, 0, 0);
    for r in link.iter().flat_map(|l| &l.bodies) {
        match s.doc.body(r.body).map(|b| (b.name.clone(), b.shape.clone(), look(b))) {
            None => {
                w.delete_body(r.step).map_err(bad)?;
                deleted += 1;
            }
            Some((name, shape, now_look)) => {
                let now = fingerprint(s, &shape)?;
                let moved_by = if r.unchanged(&now) { None } else { r.moved_by(&now) };
                let edited = |w: &mut shapecraft_shapr::Writer| {
                    extrude_edit(s, r).zip(r.extrude.as_ref()).is_some_and(|((d, draft, radius), x)| {
                        let rounded = match (radius, &x.fillet) {
                            (Some(radius), Some((node, _))) => w.set_fillet(*node, radius).unwrap_or(false),
                            _ => true,
                        };
                        rounded && w.set_extrude(x.node, d, draft).unwrap_or(false)
                    })
                };
                if r.unchanged(&now)
                    || moved_by.is_some_and(|t| w.translate_import(r.step, t).unwrap_or(false) || w.move_body(r.step, t).unwrap_or(false))
                    || edited(&mut w)
                {
                    kept.insert(r.body);
                    moved += usize::from(moved_by.is_some());
                    if name != r.name && w.rename_item(r.step, &name).map_err(bad)? {
                        renamed += 1;
                    }
                    // Compared as Shapr3D stores colours (a byte a channel), so a colour read back counts as unchanged.
                    let byte = |c: Option<[f32; 3]>| c.map(shapecraft_shapr::write::rgba);
                    let recoloured = byte(now_look.color) != byte(r.color) && w.set_color(r.step, now_look.color).map_err(bad)?;
                    let shown = now_look.hidden != r.hidden && w.set_hidden(r.step, now_look.hidden).map_err(bad)?;
                    restyled += usize::from(recoloured || shown);
                } else {
                    w.delete_body(r.step).map_err(bad)?;
                    deleted += 1;
                }
            }
        }
    }
    // Copies of the Shapr3D-made bodies kept as they are, for reading the file here again before Shapr3D rebuilds it.
    for r in link.iter().flat_map(|l| &l.bodies).filter(|r| kept.contains(&r.body)) {
        if s.doc.body(r.body).is_none() {
            continue;
        }
        let shape = crate::cmd::model::working_shape(s, r.body, false)?;
        if let [xt] = parasolid_parts(s, &shape)?.as_slice() {
            w.stash_body(r.step, xt).map_err(bad)?;
        }
    }
    // Sketches: Shapr3D's own get their circles moved or resized when they changed here; new ones are added (lines,
    // arcs, circles; splines and text not yet).
    let (mut sketches_added, mut sketches_updated) = (0, 0);
    let sketches: Vec<Sketch> = s.doc.sketches.iter().filter(|k| !k.entities.is_empty()).cloned().collect();
    for sk in &sketches {
        match link.iter().flat_map(|l| &l.sketches).find(|l| l.sketch == sk.id) {
            Some(l) => {
                let now = circles_of(sk);
                if now != l.circles && now.len() == l.circles.len() && w.update_circles(l.id, &now).map_err(bad)? {
                    sketches_updated += 1;
                }
            }
            None => {
                let out = shapr_sketch(sk);
                if !out.curves.is_empty() {
                    w.add_sketch(&out).map_err(bad)?;
                    sketches_added += 1;
                }
            }
        }
    }
    let mut imported = 0;
    let bodies: Vec<_> = s
        .doc
        .bodies
        .iter()
        .filter(|b| !kept.contains(&b.id))
        .map(|b| Ok((b.id, b.name.clone(), crate::cmd::model::working_shape(s, b.id, false)?, look(b))))
        .collect::<Result<Vec<_>>>()?;
    for (id, name, shape, look) in bodies {
        let folder = folder_path(s, id);
        let parts = parasolid_parts(s, &shape)?;
        let many = parts.len() > 1;
        for (i, xt) in parts.iter().enumerate() {
            let label = if many { format!("{name} ({})", i + 1) } else { name.clone() };
            w.import_body(&label, xt, &folder, look).map_err(bad)?;
            imported += 1;
        }
    }
    // A folder of ours whose bodies are all hidden goes hidden too, as it shows here.
    for f in &s.doc.folders {
        let bodies: Vec<_> = f.items.iter().filter_map(|&i| s.doc.body(i)).collect();
        if !bodies.is_empty() && bodies.iter().all(|b| !b.visible) {
            let path: Vec<String> = f.name.split(" / ").map(str::to_string).collect();
            w.hide_folder(&path).map_err(bad)?;
        }
    }
    // Our steps go along when they rebuild the model alone (a Shapr3D project brought in here has an Import step
    // first, so its file keeps only Shapr3D's history).
    let kept_history = match own_history(s)? {
        Some(h) => {
            w.set_own_history(&serde_json::to_string(&h).map_err(|e| EngineError::BadParams(e.to_string()))?).map_err(bad)?;
            true
        }
        _ => false,
    };
    let bytes = w.finish().map_err(bad)?;
    write_file(&path, &bytes)?;
    Ok(
        json!({ "path": path, "imported": imported, "deleted": deleted, "renamed": renamed, "kept": kept.len(), "moved": moved, "restyled": restyled, "sketches_added": sketches_added, "sketches_updated": sketches_updated, "linked": link.is_some(), "history": kept_history }),
    )
}

/// Our own parametric history, kept inside the `.shapr` files we write, so one opened here again comes back with its
/// steps (Shapr3D, opening such a file as a new project, likewise shows that project's whole history).
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct OwnHistory {
    units: shapecraft_doc::Units,
    #[serde(default)]
    legacy_face_axes: bool,
    #[serde(default)]
    variables: Vec<shapecraft_doc::Variable>,
    steps: Vec<shapecraft_doc::HistoryStep>,
    /// The bodies' volumes, smallest first: the steps are only taken when they make the same bodies again.
    volumes: Vec<f64>,
}

/// The history to keep with an export, when it rebuilds the model on its own: none while a sketch is edited in the
/// past, from before the history was parametric, or with steps that read other files (they may not be there).
fn own_history(s: &Session) -> Result<Option<OwnHistory>> {
    let h = &s.doc.history;
    if h.is_empty()
        || s.editing_past()
        || h.iter().any(|st| st.entries.is_empty() || st.entries.iter().any(|e| e.command.starts_with("import_")))
    {
        return Ok(None);
    }
    // Only steps that make this very model again (a script may have changed it behind the history's back).
    let mut fresh = Session::new(s.kernel.clone(), &s.doc.name);
    fresh.doc.units = s.doc.units;
    fresh.doc.legacy_face_axes = s.doc.legacy_face_axes;
    fresh.doc.variables = s.doc.variables.clone();
    let curves = |d: &shapecraft_doc::Document| d.sketches.iter().map(|k| k.entities.clone()).collect::<Vec<_>>();
    if fresh.replay(h.clone()).is_err()
        || fresh.doc.history.iter().any(|st| st.error.is_some())
        || curves(&fresh.doc) != curves(&s.doc)
        || volumes(&fresh)? != volumes(s)?
    {
        return Ok(None);
    }
    Ok(Some(OwnHistory {
        units: s.doc.units,
        legacy_face_axes: s.doc.legacy_face_axes,
        variables: s.doc.variables.clone(),
        steps: h.clone(),
        volumes: volumes(s)?,
    }))
}

fn volumes(s: &Session) -> Result<Vec<f64>> {
    let mut v = s.doc.bodies.iter().map(|b| s.kernel.mass_props(&b.shape).map(|m| m.volume)).collect::<std::result::Result<Vec<_>, _>>()?;
    v.sort_by(f64::total_cmp);
    Ok(v)
}

impl Session {
    /// Takes a `.shapr` file's own history in place of the plain import that just ran: the document is rebuilt from
    /// its steps, and kept only when that makes the bodies the file holds (else the import stays as it was).
    pub(crate) fn adopt_history(&mut self, h: OwnHistory) {
        let saved = (self.doc.clone(), self.active_sketch, self.selection.clone());
        let want = match volumes(self) {
            Ok(v) => v,
            Err(_) => return,
        };
        self.doc.units = h.units;
        self.doc.legacy_face_axes = h.legacy_face_axes;
        self.doc.variables = h.variables;
        let same = |a: &[f64], b: &[f64]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-6 * x.abs().max(1.0));
        let ok = self.replay(h.steps).is_ok()
            && self.doc.history.iter().all(|st| st.error.is_none())
            && same(&want, &h.volumes)
            && volumes(self).is_ok_and(|now| same(&now, &want));
        if ok {
            self.active_sketch = None;
            self.selection.clear();
            self.doc.shapr = relink(self, &saved.0);
        } else {
            (self.doc, self.active_sketch, self.selection) = saved;
        }
    }
}

/// The import's link to its file, moved onto the bodies and sketches the steps rebuilt (each body by its fingerprint,
/// each sketch by its name and curves), so exports still go back into that file. What finds no match is left out.
fn relink(s: &Session, imported: &shapecraft_doc::Document) -> Option<shapecraft_doc::ShaprLink> {
    let mut link = imported.shapr.clone()?;
    let prints: Vec<(shapecraft_doc::ItemId, Vec<f64>)> =
        s.doc.bodies.iter().filter_map(|b| fingerprint(s, &b.shape).ok().map(|f| (b.id, f))).collect();
    let mut taken = std::collections::HashSet::new();
    link.bodies.retain_mut(|r| {
        let same = |f: &[f64]| {
            f.len() == r.fingerprint.len() && f.iter().zip(&r.fingerprint).all(|(a, b)| (a - b).abs() <= 1e-6 * a.abs().max(1.0))
        };
        match prints.iter().find(|(id, f)| !taken.contains(id) && same(f)) {
            Some((id, _)) => {
                taken.insert(*id);
                r.body = *id;
                true
            }
            None => false,
        }
    });
    link.sketches.retain_mut(|l| {
        let Some(old) = imported.sketch(l.sketch) else { return false };
        match s.doc.sketches.iter().find(|k| k.name == old.name && circles_of(k) == l.circles && k.entities.len() == old.entities.len()) {
            Some(k) => {
                l.sketch = k.id;
                true
            }
            None => false,
        }
    });
    Some(link)
}

//! Solid features from profiles: extrude (draft, extents, results), loft, sweep and fill.

use std::sync::Arc;

use serde_json::json;
use shapecraft_doc::Pick;

use super::{session, volume};

#[test]
fn sketch_rectangle_extrude_then_push_pull() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sketch.0, "region": 0}]})).unwrap();
    let out = s.execute("extrude", &json!({"distance": 5})).unwrap();
    assert!(out.changed);
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    assert_eq!(s.doc.bodies[0].name, "Body 01");
    assert!(!s.doc.sketches[0].visible, "used sketch is hidden");
    assert!(s.active_sketch.is_none());
    // The cap stays selected, so a second extrude pulls it further (direct modeling).
    assert!(matches!(s.selection[..], [Pick::Face { .. }]));
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    assert!((volume(&mut s) - 2000.0).abs() < 1e-6);
    s.execute("extrude", &json!({"distance": -8})).unwrap();
    assert!((volume(&mut s) - 400.0).abs() < 1e-6);
}

#[test]
fn sketch_on_face_merges_and_cuts() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    let top = out.value["face"].as_u64().unwrap();
    s.execute("sketch", &json!({"body": body, "face": top})).unwrap();
    // Sketch coordinates on the face are centred on it.
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": -10})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    let expected = 1000.0 - std::f64::consts::PI * 4.0 * 10.0;
    assert!((volume(&mut s) - expected).abs() < 1e-3);
}

#[test]
fn loft_two_sketches_and_sweep_along_a_path() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    let a = s.active_sketch.unwrap();
    s.execute("sketch", &json!({"origin": [0, 0, 10], "normal": [0, 0, 1]})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [5, 5]})).unwrap();
    let b = s.active_sketch.unwrap();
    s.execute(
        "select",
        &json!({"picks": [{"kind": "region", "sketch": a.0, "region": 0}, {"kind": "region", "sketch": b.0, "region": 0}]}),
    )
    .unwrap();
    s.execute("loft", &json!({})).unwrap();
    let frustum = 10.0 / 3.0 * (100.0 + 25.0 + 50.0);
    assert!((volume(&mut s) - frustum).abs() < 1e-3);
    assert!(s.doc.sketches.iter().all(|k| !k.visible));
    assert!(s.execute("loft", &json!({"profiles": [{"sketch": a.0, "region": 0}]})).is_err());

    // A disc swept along a line in the xz plane.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [30, 0], "radius": 1})).unwrap();
    let disc = s.active_sketch.unwrap();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("line", &json!({"a": [30, 0], "b": [30, 20]})).unwrap();
    let path = s.active_sketch.unwrap();
    s.execute(
        "select",
        &json!({"picks": [{"kind": "region", "sketch": disc.0, "region": 0}, {"kind": "sketch_entity", "sketch": path.0, "entity": 0}]}),
    )
    .unwrap();
    s.execute("sweep", &json!({})).unwrap();
    let rod = std::f64::consts::PI * 20.0;
    assert!((volume(&mut s) - frustum - rod).abs() < 1e-3);
}

#[test]
fn extrude_with_draft() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [20, 20]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 10, "draft": 10})).unwrap();
    let v = volume(&mut s);
    assert!(v < 4000.0 && v > 2000.0, "{v}");
    // Pull the cap further, tapered too.
    s.execute("extrude", &json!({"distance": 3, "draft": 5})).unwrap();
    assert!(volume(&mut s) > v);
    assert!(s.execute("extrude", &json!({"distance": 3, "draft": 90})).is_err());
}

#[test]
fn a_cut_runs_along_its_sketch_planes_normal() {
    use shapecraft_kernel::SubShape;
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 20})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    // A circle on the side face at y = 0, pushed through the whole block.
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let front = v["bodies"][0]["faces"].as_array().unwrap().iter().find(|f| f["normal"][1].as_f64() == Some(-1.0)).unwrap()["face"]
        .as_u64()
        .unwrap();
    s.execute("sketch", &json!({"body": body, "face": front})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 3})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": -20})).unwrap();
    let b = &s.doc.bodies[0];
    let n = s.kernel.face_count(&b.shape).unwrap();
    let walls: Vec<_> = (0..n).filter(|f| s.kernel.face_plane(&b.shape, *f).unwrap().is_none()).collect();
    assert!(!walls.is_empty(), "the hole has a round wall");
    for f in walls {
        let axis = s.kernel.axis_of(&b.shape, SubShape::Face(f)).unwrap().unwrap();
        assert!(axis.dir.normalize().cross(glam::DVec3::Y).length() < 1e-9, "hole axis {:?} isn't the face normal", axis.dir);
        assert!((axis.origin.x - 10.0).abs() < 1e-6 && (axis.origin.z - 10.0).abs() < 1e-6, "hole centred on the face: {:?}", axis.origin);
    }
}

#[test]
fn extrude_symmetric_through_all_offset_and_result() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    // Symmetric: the distance goes both ways from the sketch.
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 5, "sides": "symmetric"})).unwrap();
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.min.z + 5.0).abs() < 1e-6 && (bb.max.z - 5.0).abs() < 1e-6, "{bb:?}");
    // A small square through all of it, from a sketch on the XY plane that isn't on the body.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [4, 4], "opposite": [6, 6]})).unwrap();
    let hole = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": hole.0, "region": 0, "extent": "through_all", "sides": "symmetric", "result": "subtract"}))
        .unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - 960.0).abs() < 1e-6);
    // Start offset: a new body floating 20 mm above the sketch.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [2, 2]})).unwrap();
    let up = s.active_sketch.unwrap();
    let out = s.execute("extrude", &json!({"sketch": up.0, "region": 0, "distance": 3, "start_offset": 20, "result": "new"})).unwrap();
    let id = out.value["body"].as_u64().unwrap();
    let b = s.doc.bodies.iter().find(|b| b.id.0 == id).unwrap();
    let bb = s.kernel.bounds(&b.shape).unwrap();
    assert!((bb.min.z - 20.0).abs() < 1e-6 && (bb.max.z - 23.0).abs() < 1e-6, "{bb:?}");
    assert!(s.execute("extrude", &json!({"distance": 1, "sides": "both"})).is_err());
    assert!(s.execute("extrude", &json!({"distance": 1, "result": "merge"})).is_err());
}

#[test]
fn extrude_to_object_from_plane_and_feature_results() {
    let mut s = session();
    // A slab, and a plane 12 mm up.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [40, 40]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 4})).unwrap();
    let slab = s.doc.bodies[0].id.0;
    s.execute("offset_plane", &json!({"from": "xy", "distance": 12})).unwrap();
    let plane = s.doc.planes[0].id.0;
    s.execute("offset_plane", &json!({"from": "xy", "distance": 6})).unwrap();
    let low = s.doc.planes[1].id.0;
    // A post from the 6 mm plane up to the 12 mm plane, as a new body.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [30, 30], "radius": 3})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "extent": "to_object", "to": {"plane": plane}, "start_plane": {"plane": low}, "result": "new"})).unwrap();
    let post = s.doc.bodies.last().unwrap();
    let bb = s.kernel.bounds(&post.shape).unwrap();
    assert!((bb.min.z - 6.0).abs() < 1e-6 && (bb.max.z - 12.0).abs() < 1e-6, "{bb:?}");
    assert!(s.execute("extrude", &json!({"sketch": 1, "region": 0, "extent": "to_object"})).is_err(), "needs a target");
    // A revolve cutting the slab (Result Subtract).
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("rectangle", &json!({"corner": [5, -1], "opposite": [8, 2]})).unwrap();
    let before = s.kernel.mass_props(&s.doc.body(shapecraft_doc::ItemId(slab)).unwrap().shape).unwrap().volume;
    s.execute(
        "revolve",
        &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "axis": {"origin": [0, 0, 0], "dir": [0, 0, 1]}, "result": "subtract"}),
    )
    .unwrap();
    let after = s.kernel.mass_props(&s.doc.body(shapecraft_doc::ItemId(slab)).unwrap().shape).unwrap().volume;
    assert!(after < before - 1.0, "the groove is cut: {before} → {after}");
    // A 2-distance chamfer on the post's top edge, and a ruled loft.
    let post = s.doc.bodies.iter().find(|b| b.id.0 != slab).unwrap().id.0;
    s.execute("chamfer", &json!({"body": post, "distance": 1, "distance2": 0.5})).unwrap();
    assert!(s.execute("chamfer", &json!({"body": post, "distance": 1, "distance2": -1})).is_err());
}

#[test]
fn fill_closes_an_open_box() {
    let dir = std::env::temp_dir().join(format!("shapecraft-fill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A 10 mm box without its top.
    let obj = "v 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nv 0 0 10\nv 10 0 10\nv 10 10 10\nv 0 10 10\n\
f 1 3 2\nf 1 4 3\nf 1 2 6\nf 1 6 5\nf 2 3 7\nf 2 7 6\nf 3 4 8\nf 3 8 7\nf 4 1 5\nf 4 5 8\n";
    let path = dir.join("cup.obj");
    std::fs::write(&path, obj).unwrap();
    let mut s = session();
    // An OBJ comes in as a mesh body, which Fill doesn't take : the cup as an exact shell.
    let (pts, tris) = crate::formats::read_obj(obj).unwrap();
    let (pts, idx) = crate::formats::weld(&pts, &tris);
    let shape = s.kernel.shape_from_mesh(&pts, &idx).unwrap();
    let mesh = Arc::new(s.kernel.tessellate(&shape, shapecraft_doc::project::deflection()).unwrap());
    let id = s.doc.alloc_id();
    s.doc.bodies.push(shapecraft_doc::Body {
        id,
        name: "Cup".into(),
        shape,
        mesh,
        visible: true,
        appearance: None,
        face_colors: Vec::new(),
        facets: None,
    });
    let _ = &path;
    let b = &s.doc.bodies[0];
    let top: Vec<u32> = b
        .mesh
        .edges
        .iter()
        .zip(&b.mesh.edge_ids)
        .filter(|(l, _)| l.iter().all(|q| (q[2] - 10.0).abs() < 1e-4))
        .map(|(_, id)| *id)
        .collect();
    assert!(!top.is_empty(), "the rim edges");
    s.execute("fill", &json!({"body": b.id.0, "edges": top})).unwrap();
    let v = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().volume;
    assert!((v - 1000.0).abs() < 1.0, "a closed 10 mm cube: {v}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_ruled_loft_has_flat_walls() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [20, 20]})).unwrap();
    let a = s.active_sketch.unwrap().0;
    s.execute("exit_sketching", &json!({})).unwrap();
    s.execute("offset_plane", &json!({"from": "xy", "distance": 10})).unwrap();
    let pl = s.doc.planes[0].id.0;
    s.execute("sketch", &json!({"construction": pl})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    let b = s.active_sketch.unwrap().0;
    s.execute("exit_sketching", &json!({})).unwrap();
    s.execute("loft", &json!({"profiles": [{"sketch": a, "region": 0}, {"sketch": b, "region": 0}], "ruled": true})).unwrap();
    // A frustum of a square pyramid: h/3 (A1 + A2 + √(A1·A2)).
    let v = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().volume;
    assert!((v - 10.0 / 3.0 * (400.0 + 100.0 + 200.0)).abs() < 1e-3, "{v}");
}

/// Wrap & Emboss keeps the profile's size on the round face: a 4 × 10 rectangle raised 1 on an r10 cylinder spans
/// 0.4 rad, so it adds 0.2 · (11² − 10²) · 10 = 42; engraved 1 it removes 0.2 · (10² − 9²) · 10 = 38. It sits on the
/// side facing the sketch (the XZ plane faces −Y), and History can change its depth.
#[test]
fn wrap_emboss_on_cylinder_and_cone() {
    use std::f64::consts::PI;
    let cylinder = |s: &mut crate::Session| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 10})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
        s.execute("sketch", &json!({"plane": "xz"})).unwrap();
        s.execute("rectangle", &json!({"corner": [-2, 5], "opposite": [2, 15]})).unwrap();
    };
    let mut s = session();
    cylinder(&mut s);
    s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": 1})).unwrap();
    assert!((volume(&mut s) - (2000.0 * PI + 42.0)).abs() < 0.05, "{}", volume(&mut s));
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.min.y + 11.0).abs() < 1e-3 && (bb.max.y - 10.0).abs() < 1e-3, "{bb:?}");
    let step = s.doc.history.iter().position(|h| h.kind == "wrap_emboss").unwrap();
    assert_eq!(s.doc.history[step].label, "Wrap & Emboss 01");
    s.execute("edit_step", &json!({"step": step, "params": {"depth": -1}})).unwrap();
    assert!((volume(&mut s) - (2000.0 * PI - 38.0)).abs() < 0.05, "{}", volume(&mut s));

    // Turned a quarter turn, the 10-tall strip lies round the cylinder: 1 rad, 0.5 · 21 · 4 = 42 again.
    let mut s = session();
    cylinder(&mut s);
    s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": 1, "rotation": 90})).unwrap();
    assert!((volume(&mut s) - (2000.0 * PI + 42.0)).abs() < 0.05, "{}", volume(&mut s));

    // A disc of radius 2 with a hole of radius 1 (a ring, area 3π): ≈ 3π · (1 + 1/20) more.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 10})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("circle", &json!({"center": [0, 10], "radius": 2})).unwrap();
    s.execute("circle", &json!({"center": [0, 10], "radius": 1})).unwrap();
    let ring = s.doc.sketches.last().unwrap().regions().iter().max_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
    s.execute("wrap_emboss", &json!({"sketch": 3, "region": ring, "body": 2, "depth": 1})).unwrap();
    assert!((volume(&mut s) - (2000.0 * PI + 3.0 * PI * 1.05)).abs() < 0.05, "{}", volume(&mut s));

    // A cone (r 10 at the bottom, 5 at the top, 20 tall): an engraving 0.5 deep of a 2 × 2 square on its side.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [10, 0]})).unwrap();
    s.execute("line", &json!({"a": [10, 0], "b": [5, 20]})).unwrap();
    s.execute("line", &json!({"a": [5, 20], "b": [0, 20]})).unwrap();
    s.execute("line", &json!({"a": [0, 20], "b": [0, 0]})).unwrap();
    s.execute("revolve", &json!({"sketch": 1, "region": 0, "axis": "v"})).unwrap();
    let before = volume(&mut s);
    assert!((before - PI * 20.0 / 3.0 * (100.0 + 50.0 + 25.0)).abs() < 0.05, "{before}");
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let sk = s.active_sketch.unwrap().0;
    s.execute("rectangle", &json!({"corner": [-1, 9], "opposite": [1, 11]})).unwrap();
    let body = s.doc.bodies[0].id.0;
    s.execute("wrap_emboss", &json!({"sketch": sk, "region": 0, "body": body, "depth": -0.5})).unwrap();
    let lost = before - volume(&mut s);
    // The face at height 10 has radius 7.5: 4 · 0.5 · (1 − 0.25 / 7.5), give or take the slant.
    assert!((lost - 4.0 * 0.5 * (1.0 - 0.25 / 7.5)).abs() < 0.1, "{lost}");

    // Hostile and wrong: no depth, a flat face only, a sketch facing along the axis.
    let mut s = session();
    cylinder(&mut s);
    assert!(s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": 0})).is_err());
    assert!(s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": f64::NAN})).is_err());
    assert!(s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "face": 1, "depth": 1})).is_err());
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [12, 0], "opposite": [14, 2]})).unwrap();
    assert!(s.execute("wrap_emboss", &json!({"sketch": 4, "region": 0, "body": 2, "depth": 1})).is_err());
}

/// A cut from a free plane goes into the body it really runs into, not the first body whose bounding box it
/// touches (a shell's box holds the inset panel, so the oldest body was cut and the panel left alone).
#[test]
fn subtract_cuts_the_body_it_runs_into() {
    let mut s = session();
    let square = |s: &mut crate::Session, origin: [f64; 3], half: f64| {
        s.execute("sketch", &json!({"origin": origin, "normal": [0, 0, 1]})).unwrap();
        s.execute("rectangle", &json!({"corner": [-half, -half], "opposite": [half, half]})).unwrap();
        s.active_sketch.unwrap().0
    };
    // The shell, with a pocket on top, and a panel sitting in the pocket as its own body.
    let shell = square(&mut s, [50.0, 50.0, 0.0], 50.0);
    s.execute("extrude", &json!({"sketch": shell, "region": 0, "distance": 20})).unwrap();
    let pocket = square(&mut s, [50.0, 50.0, 20.0], 10.0);
    s.execute("extrude", &json!({"sketch": pocket, "region": 0, "distance": -2, "result": "subtract"})).unwrap();
    let panel = square(&mut s, [50.0, 50.0, 18.0], 10.0);
    let out = s.execute("extrude", &json!({"sketch": panel, "region": 0, "distance": 2, "result": "new"})).unwrap();
    let panel_id = out.value["body"].as_u64().unwrap();
    let vol = |s: &crate::Session, id: u64| {
        let b = s.doc.bodies.iter().find(|b| b.id.0 == id).unwrap();
        s.kernel.mass_props(&b.shape).unwrap().volume
    };
    let shell_id = s.doc.bodies[0].id.0;
    let shell_before = vol(&s, shell_id);
    // A logo pushed 1 mm into the panel's top.
    let logo = square(&mut s, [50.0, 50.0, 20.0], 5.0);
    let out = s.execute("extrude", &json!({"sketch": logo, "region": 0, "distance": -1, "result": "subtract"})).unwrap();
    assert_eq!(out.value["bodies"], json!([panel_id]));
    assert!((vol(&s, panel_id) - (400.0 * 2.0 - 100.0)).abs() < 1e-6);
    assert!((vol(&s, shell_id) - shell_before).abs() < 1e-6);
    // Deeper than the panel: both are cut.
    let deep = square(&mut s, [50.0, 50.0, 20.0], 2.0);
    let out = s.execute("extrude", &json!({"sketch": deep, "region": 0, "distance": -5, "result": "subtract"})).unwrap();
    assert_eq!(out.value["bodies"].as_array().map(Vec::len), Some(2));
}

/// A free-plane profile lying on a body's face works on that body on auto, as if sketched on the face: pushed in
/// it cuts, pulled out it joins (reference 26.202, s36: ground sketch over a block's top, disc pulled down =
/// Subtract). Only part of the profile on the face (a ground rectangle half under the block), a plane through the
/// middle of a body, or a profile off every body still makes a new body.
#[test]
fn free_plane_profile_on_a_face_cuts_and_joins() {
    let mut s = session();
    let disc = |s: &mut crate::Session, z: f64| {
        s.execute("sketch", &json!({"origin": [0, 0, z], "normal": [0, 0, 1]})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
        s.active_sketch.unwrap().0
    };
    s.execute("sketch", &json!({"origin": [0, 0, 0], "normal": [0, 0, 1]})).unwrap();
    s.execute("rectangle", &json!({"corner": [-5, -5], "opposite": [5, 5]})).unwrap();
    let block = s.active_sketch.unwrap().0;
    s.execute("extrude", &json!({"sketch": block, "region": 0, "distance": -10})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    let disc_area = std::f64::consts::PI * 4.0;

    // Into the block: a 4 mm hole, and the step remembers the block was behind the plane.
    let k = disc(&mut s, 0.0);
    assert_eq!(
        crate::cmd::extrude_auto_body(&s, crate::ItemId(k), &[0]).map(|(_, side)| side),
        Some(-1.0),
        "the block is behind the ground plane"
    );
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": -4})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - (1000.0 - disc_area * 4.0)).abs() < 1e-6, "{}", volume(&mut s));
    let step = s.all_steps().last().cloned().unwrap();
    assert_eq!(step.entries[0].params["auto_side"], json!(-1.0));

    // Out of the bottom face (the block is on the plane's front side there): a boss joined on.
    let k = disc(&mut s, -10.0);
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": -3})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - (1000.0 - disc_area)).abs() < 1e-6, "{}", volume(&mut s));

    // Half on the block's bottom face, pulled up into it: a new body (reference 26.202).
    s.execute("sketch", &json!({"origin": [0, 0, -10], "normal": [0, 0, 1]})).unwrap();
    s.execute("rectangle", &json!({"corner": [3, 3], "opposite": [8, 8]})).unwrap();
    let k = s.active_sketch.unwrap().0;
    assert_eq!(crate::cmd::extrude_auto_body(&s, crate::ItemId(k), &[0]), None);
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": 4})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);

    // A plane through the block's middle, and one off to the side: new bodies, as before.
    let k = disc(&mut s, -5.0);
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": 1})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    s.execute("sketch", &json!({"origin": [30, 0, 0], "normal": [0, 0, 1]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    let k = s.active_sketch.unwrap().0;
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": -1})).unwrap();
    assert_eq!(s.doc.bodies.len(), 3);

    // Explicit New Body still means a new body.
    let k = disc(&mut s, 0.0);
    s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": 2, "result": "new"})).unwrap();
    assert_eq!(s.doc.bodies.len(), 4);
}

/// The selection pill's radius: a cylinder's wall gives its radius, a flat cap and a cone's wall give none.
#[test]
fn selection_radius_of_round_faces() {
    let mut s = session();
    let tube = |s: &mut crate::Session, draft: f64| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 3})).unwrap();
        let k = s.active_sketch.unwrap().0;
        let out = s.execute("extrude", &json!({"sketch": k, "region": 0, "distance": 10, "draft": draft, "result": "new"})).unwrap();
        out.value["body"].as_u64().unwrap()
    };
    let check = |s: &mut crate::Session, body: u64| -> (Option<f64>, usize) {
        let b = s.doc.bodies.iter().find(|b| b.id.0 == body).unwrap();
        let faces = b.mesh.triangle_faces.iter().max().copied().unwrap() + 1;
        let mut found = None;
        let mut flat = 0;
        for face in 0..faces {
            s.selection = vec![Pick::Face { body: shapecraft_doc::ItemId(body), face }];
            match s.selection_radius() {
                Some(r) => found = Some(r),
                None => flat += 1,
            }
        }
        (found, flat)
    };
    let cyl = tube(&mut s, 0.0);
    let (r, others) = check(&mut s, cyl);
    assert!((r.expect("the wall has a radius") - 3.0).abs() < 1e-4, "{r:?}");
    assert_eq!(others, 2, "the two caps have none");
    // The wall also reads how tall it is (#35); a cap reads no height.
    let faces = s.doc.bodies.iter().find(|b| b.id.0 == cyl).unwrap().mesh.triangle_faces.iter().max().copied().unwrap() + 1;
    let heights: Vec<f64> = (0..faces)
        .filter_map(|face| {
            s.selection = vec![Pick::Face { body: shapecraft_doc::ItemId(cyl), face }];
            s.selection_height()
        })
        .collect();
    assert_eq!(heights.len(), 1, "only the wall: {heights:?}");
    assert!((heights[0] - 10.0).abs() < 1e-3, "{heights:?}");
    let cone = tube(&mut s, 10.0);
    assert_eq!(check(&mut s, cone).0, None, "a drafted wall is a cone");
    s.selection.clear();
    assert_eq!(s.selection_radius(), None);
}

/// #53: a sketch stays shown while it still has profiles nobody used, and hides once all of them are.
#[test]
fn sketch_hides_only_once_every_profile_is_used() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    for x in [0, 30, 60] {
        s.execute("circle", &json!({"center": [x, 0], "radius": 5})).unwrap();
    }
    let sketch = s.active_sketch.unwrap();
    assert_eq!(s.doc.sketches[0].regions().len(), 3);
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    assert!(s.doc.sketches[0].visible, "two holes left to use: the sketch stays");
    s.execute("extrude", &json!({"sketch": sketch.0, "regions": [1, 2], "distance": 5})).unwrap();
    assert!(!s.doc.sketches[0].visible, "every profile used: the sketch hides");
    // Undo brings back the sketch as it was, and what had been used.
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.sketches[0].visible);
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 1, "distance": 5})).unwrap();
    assert!(s.doc.sketches[0].visible, "region 2 still unused");
}

/// A sketch is used up when all of its curves are (a ring takes its hole's circle along, a lens both circles);
/// `keep_sketches` waits for every region instead.
#[test]
fn a_sketch_hides_once_its_curves_are_used() {
    for (second, regions) in [(json!({"center": [0, 0], "radius": 3}), 2), (json!({"center": [7, 0], "radius": 5}), 3)] {
        for keep in [false, true] {
            let mut s = session();
            s.keep_sketches = keep;
            s.execute("sketch", &json!({"plane": "xy"})).unwrap();
            s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
            s.execute("circle", &second).unwrap();
            let sketch = s.active_sketch.unwrap();
            assert_eq!(s.doc.sketches[0].regions().len(), regions);
            // The region both circles bound: the ring, the lens.
            let both = s.doc.sketches[0].regions().iter().map(|r| r.index).find(|&i| s.doc.sketches[0].region_entities(i).len() == 2);
            let lens = s.doc.sketches[0].regions().iter().min_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
            let pick = if regions == 3 { lens } else { both.unwrap() };
            s.execute("extrude", &json!({"sketch": sketch.0, "region": pick, "distance": 5})).unwrap();
            assert_eq!(s.doc.sketches[0].visible, keep, "{regions} regions, keep {keep}");
        }
    }
}

/// Every letter extruded: the sketch hides, though the holes of A and B are regions nobody used.
#[test]
fn a_text_sketch_hides_once_its_letters_are_used() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("text", &json!({"text": "AIB", "at": [0, 0], "size": 10})).unwrap();
    let sketch = s.active_sketch.unwrap();
    let first = s.doc.sketches[0].regions()[0].index;
    let letters = s.doc.sketches[0].text_regions(first).unwrap();
    assert!(letters.len() == 3 && s.doc.sketches[0].regions().len() == 6, "{letters:?}");
    s.execute("extrude", &json!({"sketch": sketch.0, "regions": &letters[..2], "distance": 2})).unwrap();
    assert!(s.doc.sketches[0].visible, "a letter left");
    s.execute("extrude", &json!({"sketch": sketch.0, "region": letters[2], "distance": 2})).unwrap();
    assert!(!s.doc.sketches[0].visible, "every letter used");
}

/// #50: extruding text, letter by letter or all at once, never leaves a body without extent behind, and a
/// circle extruded after it still works ("bounds failed: shape has no extent").
#[test]
fn text_extrusions_leave_no_empty_body() {
    for (text, all) in [("Hi 8%@&", true), ("ABO", false), ("B&@", false)] {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("text", &json!({"at": [0, 0], "text": text, "size": 10})).unwrap();
        let sketch = s.active_sketch.unwrap();
        let n = s.doc.sketches[0].regions().len() as u32;
        assert!(n > 0);
        if all {
            let regions: Vec<u32> = (0..n).collect();
            let _ = s.execute("extrude", &json!({"sketch": sketch.0, "regions": regions, "distance": 2}));
        } else {
            for r in 0..n {
                let _ = s.execute("extrude", &json!({"sketch": sketch.0, "region": r, "distance": 2}));
            }
        }
        for b in &s.doc.bodies {
            assert!(s.kernel.bounds(&b.shape).is_ok(), "{text}: {} has no extent", b.name);
        }
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, -40], "radius": 5})).unwrap();
        let k = s.active_sketch.unwrap();
        s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": 3})).unwrap();
    }
}

/// #50 as reported: text on a cylinder's top face, its letters running over the face's edge, each letter pulled
/// up or cut down. No body may be left without extent, and later extrusions keep working.
#[test]
fn text_over_a_face_edge_extrudes_and_cuts() {
    for dist in [2.0, -2.0, 12.0, -12.0] {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 20})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 8})).unwrap();
        let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
        let body = out.value["body"].as_u64().unwrap();
        let top = out.value["face"].as_u64().unwrap();
        s.execute("sketch", &json!({"body": body, "face": top})).unwrap();
        s.execute("text", &json!({"at": [-30, 10], "text": "0000 B", "size": 12, "angle": -20})).unwrap();
        let sketch = s.active_sketch.unwrap();
        let n = s.doc.sketch(sketch).unwrap().regions().len() as u32;
        let mut errors = Vec::new();
        for r in 0..n {
            if let Err(e) = s.execute("extrude", &json!({"sketch": sketch.0, "region": r, "distance": dist})) {
                errors.push(format!("region {r}: {e}"));
            }
        }
        for b in &s.doc.bodies {
            assert!(s.kernel.bounds(&b.shape).is_ok(), "d={dist}: {} has no extent", b.name);
        }
        assert!(!errors.iter().any(|e| e.contains("extent")), "d={dist}: {errors:?}");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, -60], "radius": 5})).unwrap();
        let k = s.active_sketch.unwrap();
        s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": 3})).unwrap();
    }
}

/// #50: a cut that takes away all of a body leaves no empty body behind, and one empty body (from an older file
/// or a kernel slip) never blocks extruding elsewhere.
#[test]
fn a_cut_through_everything_leaves_no_empty_body() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    let top = out.value["face"].as_u64().unwrap();
    s.execute("sketch", &json!({"body": body, "face": top})).unwrap();
    s.execute("rectangle", &json!({"corner": [-20, -20], "opposite": [20, 20]})).unwrap();
    let k = s.active_sketch.unwrap();
    let n = s.doc.sketch(k).unwrap().regions().len() as u32;
    let before = volume(&mut s);
    for r in 0..n {
        let _ = s.execute("extrude", &json!({"sketch": k.0, "region": r, "distance": -5}));
    }
    for b in &s.doc.bodies {
        assert!(s.kernel.bounds(&b.shape).is_ok(), "{} has no extent", b.name);
    }
    assert!((volume(&mut s) - before).abs() < 1e-6, "the cut that would leave nothing is refused");
    // An empty body in the project anyway: a new sketch's extrusion still works.
    let mut empty = s.doc.bodies.first().cloned().unwrap_or_else(|| panic!("a body is left"));
    let cube = s.kernel.make_box(glam::DVec3::new(100.0, 0.0, 0.0), glam::DVec3::splat(5.0)).unwrap();
    empty.shape = s.kernel.boolean(shapecraft_kernel::BoolOp::Subtract, &cube, &cube).unwrap();
    assert!(s.kernel.bounds(&empty.shape).is_err(), "the stand-in body is empty");
    empty.id = s.doc.alloc_id();
    s.doc.bodies.push(empty);
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [40, 40], "radius": 5})).unwrap();
    let k = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": 3})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [80, 40], "radius": 5})).unwrap();
    let k = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": 3, "result": "union"})).unwrap_err();
}

/// #47: a counterbore. A cylinder with a hole through it; a bigger circle drawn on the top face around the hole
/// and pushed 1 mm in cuts the step, it doesn't add material (the profile sits partly over the hole).
#[test]
fn bigger_circle_over_a_hole_cuts_a_counterbore() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 20})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    let top = out.value["face"].as_u64().unwrap();
    s.execute("sketch", &json!({"body": body, "face": top})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": -10})).unwrap();
    let pi = std::f64::consts::PI;
    let holed = pi * (400.0 - 25.0) * 10.0;
    assert!((volume(&mut s) - holed).abs() < 1e-3, "the hole: {}", volume(&mut s));
    let top = super::top_face(&mut s);
    s.execute("sketch", &json!({"body": body, "face": top})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 8})).unwrap();
    let k = s.active_sketch.unwrap();

    s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": -1})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1, "no new body");
    let want = holed - pi * (64.0 - 25.0) * 1.0;
    assert!((volume(&mut s) - want).abs() < 1e-3, "counterbore: {} want {want}", volume(&mut s));
}

/// #60: a plate whose straight side meets a concave arc at a sharp corner; the short vertical edge there fillets.
#[test]
fn fillet_where_a_side_meets_a_concave_arc() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    for (a, b) in [([0, 0], [60, 0]), ([60, 0], [60, 20]), ([30, 40], [0, 40]), ([0, 40], [0, 0])] {
        s.execute("line", &json!({"a": a, "b": b})).unwrap();
    }
    // Concave arc from (60,20) to (30,40), bulging into the plate.
    s.execute("arc", &json!({"a": [60, 20], "mid": [42, 25], "b": [30, 40]})).unwrap();
    let k = s.active_sketch.unwrap();
    let out = s.execute("extrude", &json!({"sketch": k.0, "region": 0, "distance": 2})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    // The vertical edge at (60, 20): every point of it there.
    let found = super::edges_where(&s, 0, |p| (p[0] - 60.0).abs() < 1e-3 && (p[1] - 20.0).abs() < 1e-3);
    assert_eq!(found.len(), 1, "the corner edge: {found:?}");
    let edge = found[0];
    let before = volume(&mut s);
    for r in [0.5, 1.0, 3.0] {
        let res = s.execute("fillet", &json!({"body": body, "edges": [edge], "radius": r}));
        if res.is_ok() {
            assert!(volume(&mut s) < before);
            s.execute("undo", &json!({})).unwrap();
        }
    }
}

/// An extrude's "from" names the bodies a cut works on: without it the cut finds every body it runs into, with it
/// only those listed, and a list that names nothing usable is refused.
#[test]
fn an_extrude_cut_works_on_the_bodies_named_in_from() {
    let mut s = session();
    for (a, b) in [(0, 10), (20, 30)] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [a, 0], "opposite": [b, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10, "result": "new"})).unwrap();
    }
    let ids: Vec<u64> = s.doc.bodies.iter().map(|b| b.id.0).collect();
    let volume_of =
        |s: &crate::Session, id: u64| s.kernel.mass_props(&s.doc.body(shapecraft_doc::ItemId(id)).unwrap().shape).unwrap().volume;
    let cut = |s: &mut crate::Session, extra: serde_json::Value| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [-5, -5], "opposite": [35, 15]})).unwrap();
        let mut p = json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5, "result": "subtract"});
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        s.execute("extrude", &p)
    };
    let mut named = s.fork();
    cut(&mut named, json!({"from": [ids[1]]})).unwrap();
    assert!((volume_of(&named, ids[0]) - 1000.0).abs() < 1e-6, "the first body is left alone");
    assert!((volume_of(&named, ids[1]) - 500.0).abs() < 1e-6, "the second is cut");
    cut(&mut s, json!({})).unwrap();
    assert!((volume_of(&s, ids[0]) - 500.0).abs() < 1e-6 && (volume_of(&s, ids[1]) - 500.0).abs() < 1e-6, "found both without it");
    assert!(cut(&mut named, json!({"from": []})).is_err());
    assert!(cut(&mut named, json!({"from": [9999]})).is_err());
}

/// Wrap & Emboss beyond one plain profile: a strip running past the face's rim (raised off the skin all along, so
/// 0.2 · (11² − 9.75²) · 5 more above the top), two profiles at once, an engraving into an earlier emboss, profiles
/// a ring with the disc in its hole, profiles reaching most of the way round (264° and 172° on an r10 cylinder: 2.3 · ½ · (11² − 10²) · 10 = 483 raised,
/// 2.3 · ½ · (10² − 9²) · 10 = 437 engraved), and two engravings on a cone. Both kernels give these.
#[test]
fn wrap_emboss_wide_past_the_rim_and_again() {
    use std::f64::consts::PI;
    let cylinder = |s: &mut crate::Session| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 10})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
        s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    };
    let added = |s: &mut crate::Session| volume(s) - 2000.0 * PI;

    let mut s = session();
    cylinder(&mut s);
    s.execute("rectangle", &json!({"corner": [-2, 10], "opposite": [2, 25]})).unwrap();
    s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": 1})).unwrap();
    assert!((added(&mut s) - (42.0 + 25.9375)).abs() < 0.01, "{}", added(&mut s));

    let mut s = session();
    cylinder(&mut s);
    s.execute("rectangle", &json!({"corner": [-6, 5], "opposite": [-2, 15]})).unwrap();
    s.execute("rectangle", &json!({"corner": [2, 5], "opposite": [6, 15]})).unwrap();
    s.execute("wrap_emboss", &json!({"sketch": 3, "regions": [0, 1], "body": 2, "depth": 1})).unwrap();
    assert!((added(&mut s) - 84.0).abs() < 0.01, "{}", added(&mut s));
    // Into the first raised one (its face at r11 is the nearest), then across that one's edge.
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let sk = s.active_sketch.unwrap().0;
    s.execute("rectangle", &json!({"corner": [-1, 8], "opposite": [1, 12]})).unwrap();
    s.execute("wrap_emboss", &json!({"sketch": sk, "region": 0, "body": 2, "depth": -0.5})).unwrap();
    assert!((added(&mut s) - 80.1).abs() < 0.01, "{}", added(&mut s));
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let sk = s.active_sketch.unwrap().0;
    s.execute("rectangle", &json!({"corner": [-3, 14], "opposite": [3, 17]})).unwrap();
    s.execute("wrap_emboss", &json!({"sketch": sk, "region": 0, "body": 2, "depth": -0.5})).unwrap();
    assert!((added(&mut s) - 71.0734).abs() < 0.01, "{}", added(&mut s));

    // A ring and the disc in its hole, picked together: one disc of radius 2, ≈ 4π · (1 + 1/20).
    let mut s = session();
    cylinder(&mut s);
    s.execute("circle", &json!({"center": [0, 10], "radius": 2})).unwrap();
    s.execute("circle", &json!({"center": [0, 10], "radius": 1})).unwrap();
    s.execute("wrap_emboss", &json!({"sketch": 3, "regions": [0, 1], "body": 2, "depth": 1})).unwrap();
    assert!((added(&mut s) - 4.0 * PI * 1.05).abs() < 0.01, "{}", added(&mut s));

    for (width, depth, want) in [(46.0, 1.0, 483.0), (46.0, -1.0, -437.0), (30.0, 1.0, 315.0)] {
        let mut s = session();
        cylinder(&mut s);
        s.execute("rectangle", &json!({"corner": [-width / 2.0, 5], "opposite": [width / 2.0, 15]})).unwrap();
        s.execute("wrap_emboss", &json!({"sketch": 3, "region": 0, "body": 2, "depth": depth})).unwrap();
        assert!((added(&mut s) - want).abs() < 0.01, "{width} {depth}: {}", added(&mut s));
    }

    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [10, 0]})).unwrap();
    s.execute("line", &json!({"a": [10, 0], "b": [5, 20]})).unwrap();
    s.execute("line", &json!({"a": [5, 20], "b": [0, 20]})).unwrap();
    s.execute("line", &json!({"a": [0, 20], "b": [0, 0]})).unwrap();
    s.execute("revolve", &json!({"sketch": 1, "region": 0, "axis": "v"})).unwrap();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let sk = s.active_sketch.unwrap().0;
    s.execute("rectangle", &json!({"corner": [-3, 5], "opposite": [-1, 8]})).unwrap();
    s.execute("rectangle", &json!({"corner": [1, 5], "opposite": [3, 8]})).unwrap();
    let before = volume(&mut s);
    let body = s.doc.bodies[0].id.0;
    s.execute("wrap_emboss", &json!({"sketch": sk, "regions": [0, 1], "body": body, "depth": -0.5})).unwrap();
    assert!((before - volume(&mut s) - 5.8264).abs() < 0.01, "{}", before - volume(&mut s));
}

/// #91: offsetting the side of a bent swept tube walked the kernel into a missing edge and ended the app. Now it's a
/// distance that doesn't work, and the body is as it was.
#[test]
fn offsetting_a_bent_tube_too_far_is_refused() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [0, 40]})).unwrap();
    s.execute("line", &json!({"a": [0, 40], "b": [25, 60]})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 4})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    s.execute("sweep", &json!({"profile": {"sketch": 2, "region": 0}, "path": {"sketch": 1}, "result": "new", "orientation": "parallel"}))
        .unwrap();
    let before = volume(&mut s);
    for face in 0..4 {
        for distance in [8.0, -8.0, 30.0] {
            let offset = json!({"distance": distance, "body": 3, "faces": [face]});
            let _ = s.preview("offset_face", &offset);
        }
    }
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - before).abs() < 1e-9);
    // The kernel still works afterwards.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [100, 0], "opposite": [110, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10, "result": "new"})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
}

/// Found beside #91: the same for Shell, on a tube swept round a corner.
#[test]
fn shelling_a_round_cornered_tube_never_ends_the_app() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [0, 40]})).unwrap();
    s.execute("line", &json!({"a": [0, 40], "b": [40, 75]})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2.5})).unwrap();
    s.execute("sweep", &json!({"profile": {"sketch": 2, "region": 0}, "path": {"sketch": 1}, "result": "new", "corners": "round"}))
        .unwrap();
    let before = volume(&mut s);
    let _ = s.preview("shell", &json!({"thickness": 0.8, "body": 3, "faces": [2]}));
    assert!((volume(&mut s) - before).abs() < 1e-9);
}

/// Sweep along a body's edges, along curves of two sketches, and with the profile in the middle of its path.
#[test]
fn sweep_follows_edges_and_several_sketches() {
    let mut s = session();
    // A box; a bead swept round its top rim from a circle standing on one corner's edge.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [40, 30]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = s.doc.bodies[0].id;
    let top: Vec<u32> = (0..s.kernel.edge_count(&s.doc.bodies[0].shape).unwrap())
        .filter(|&e| match s.kernel.edge_curve(&s.doc.bodies[0].shape, e).unwrap() {
            shapecraft_kernel::EdgeCurve::Line { a, b } => a.z > 9.9 && b.z > 9.9,
            _ => false,
        })
        .collect();
    assert_eq!(top.len(), 4);
    s.execute("sketch", &json!({"origin": [20, 0, 10], "normal": [1, 0, 0]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap();
    let c = s.active_sketch.unwrap();
    s.execute(
        "sweep",
        &json!({"profile": {"sketch": c.0, "region": 0}, "path": [{"body": body.0, "edges": top}], "result": "new"}),
    )
    .unwrap();
    let bead = s.doc.bodies.last().unwrap();
    let v = s.kernel.mass_props(&bead.shape).unwrap().volume.abs();
    // Mitred corners: exactly the circle's area times the rim's length.
    let want = std::f64::consts::PI * 140.0;
    assert!((v - want).abs() < want * 1e-3, "{v} vs {want}");

    // An L-shaped path: one leg in each of two sketches, the profile at its start.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [30, 0]})).unwrap();
    let a = s.active_sketch.unwrap();
    s.execute("sketch", &json!({"origin": [30, 0, 0], "normal": [0, -1, 0]})).unwrap();
    let sk = s.active_sketch.unwrap();
    let up = s.doc.sketch(sk).unwrap().plane;
    let to2 = |p: glam::DVec3| { let d = p - up.origin; json!([d.dot(up.x), d.dot(up.y)]) };
    s.execute("line", &json!({"a": to2(glam::dvec3(30.0, 0.0, 0.0)), "b": to2(glam::dvec3(30.0, 0.0, 20.0))})).unwrap();
    s.execute("sketch", &json!({"origin": [0, 0, 0], "normal": [1, 0, 0]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    let c = s.active_sketch.unwrap();
    s.execute("sweep", &json!({"profile": {"sketch": c.0, "region": 0}, "path": [{"sketch": a.0}, {"sketch": sk.0}], "result": "new"})).unwrap();
    let want = std::f64::consts::PI * 4.0 * 50.0;
    assert!((volume(&mut s) - want).abs() < want * 1e-3, "{} vs {want}", volume(&mut s));

    // The profile half way along its line: the whole line is swept, both ways.
    for position in ["auto", "intersection", "point"] {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("line", &json!({"a": [-30, 0], "b": [10, 0]})).unwrap();
        let a = s.active_sketch.unwrap();
        s.execute("sketch", &json!({"origin": [0, 0, 0], "normal": [1, 0, 0]})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
        let c = s.active_sketch.unwrap();
        s.execute("sweep", &json!({"profile": {"sketch": c.0, "region": 0}, "path": {"sketch": a.0}, "result": "new", "position": position})).unwrap();
        let b = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
        assert!((b.min.x + 30.0).abs() < 1e-3 && (b.max.x - 10.0).abs() < 1e-3, "{position}: {b:?}");
    }
    // A profile away from the path: from its nearest end, or refused where they don't cross.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("line", &json!({"a": [0, 0], "b": [40, 0]})).unwrap();
    let a = s.active_sketch.unwrap();
    s.execute("sketch", &json!({"origin": [-5, 0, 0], "normal": [1, 0, 0]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 2})).unwrap();
    let c = s.active_sketch.unwrap();
    let mut sweep = json!({"profile": {"sketch": c.0, "region": 0}, "path": {"sketch": a.0}, "result": "new", "position": "intersection"});
    assert!(s.execute("sweep", &sweep).is_err());
    sweep["position"] = json!("endpoint");
    s.execute("sweep", &sweep).unwrap();
    let want = std::f64::consts::PI * 4.0 * 40.0;
    assert!((volume(&mut s) - want).abs() < want * 1e-3, "{}", volume(&mut s));
}

/// Extent ▸ To Object stops at round faces, tilted planes and whole bodies.
#[test]
fn extrude_reaches_round_faces_tilted_planes_and_bodies() {
    // A cylinder lying along y, radius 10, its axis at z = 30; a 4×4 post from the ground up to it.
    let scene = || {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xz"})).unwrap();
        let sk = s.active_sketch.unwrap();
        let up = s.doc.sketch(sk).unwrap().plane;
        let d = glam::dvec3(0.0, 0.0, 30.0) - up.origin;
        s.execute("circle", &json!({"center": [d.dot(up.x), d.dot(up.y)], "radius": 10})).unwrap();
        s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 20, "sides": "symmetric"})).unwrap();
        let cyl = s.doc.bodies[0].id;
        let shape = s.doc.bodies[0].shape.clone();
        let side = (0..s.kernel.face_count(&shape).unwrap()).find(|&f| s.kernel.face_plane(&shape, f).unwrap().is_none()).unwrap();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [-2, -2], "opposite": [2, 2]})).unwrap();
        (s, cyl, side)
    };
    // ∫ over the 4×4 square of 30 − √(100 − x²).
    let under = 4.0 * (120.0 - 2.0 * (2.0 * 96f64.sqrt() / 2.0 + 50.0 * 0.2f64.asin()));
    for to in [0, 1] {
        let (mut s, cyl, side) = scene();
        let to = if to == 0 { json!({"body": cyl.0, "face": side}) } else { json!({"body": cyl.0}) };
        let sk = s.active_sketch.unwrap();
        s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "extent": "to_object", "to": to, "result": "new"})).unwrap();
        let v = s.kernel.mass_props(&s.doc.bodies[1].shape).unwrap().volume;
        assert!((v - under).abs() < 1e-3, "{v} vs {under}");
        // Editable: it follows when the cylinder moves.
        assert!(s.doc.history.iter().all(|h| h.error.is_none()));
    }
    // A profile wider than the cylinder can't all meet it.
    let (mut s, cyl, side) = scene();
    s.execute("rectangle", &json!({"corner": [-15, -5], "opposite": [15, 5]})).unwrap();
    let sk = s.active_sketch.unwrap();
    let n = s.doc.sketch(sk).unwrap().regions.len();
    let regions: Vec<usize> = (0..n).collect();
    let e = s.execute("extrude", &json!({"sketch": sk.0, "regions": regions, "extent": "to_object", "to": {"body": cyl.0, "face": side}})).unwrap_err();
    assert!(e.to_string().contains("doesn't all meet"), "{e}");
    // A tilted plane: a wedge top.
    let mut s = session();
    s.execute("plane_3points", &json!({"points": [[0, 0, 10], [10, 0, 20], [0, 10, 10]]})).unwrap();
    let pl = s.doc.planes.last().unwrap().id;
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "extent": "to_object", "to": {"plane": pl.0}})).unwrap();
    assert!((volume(&mut s) - 1500.0).abs() < 1e-3, "{}", volume(&mut s));
}

/// Replace Face onto a round face: the block's top takes the cylinder's shape, grown and cut as needed.
#[test]
fn replace_face_takes_a_round_faces_shape() {
    let mut s = session();
    // A tunnel mold: a cylinder along y, axis at z = 0, radius 30.
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 30})).unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 5})).unwrap();
    let cyl = s.doc.bodies[0].id;
    let shape = s.doc.bodies[0].shape.clone();
    let side = (0..s.kernel.face_count(&shape).unwrap()).find(|&f| s.kernel.face_plane(&shape, f).unwrap().is_none()).unwrap();
    for (height, what) in [(20.0, "below"), (28.0, "across"), (40.0, "above")] {
        let mut s2 = session();
        s2.doc = s.doc.clone();
        s2.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s2.execute("rectangle", &json!({"corner": [-15, 20], "opposite": [15, 40]})).unwrap();
        let out = s2.execute("extrude", &json!({"sketch": s2.active_sketch.unwrap().0, "region": 0, "distance": height, "result": "new"})).unwrap();
        let (body, top) = (out.value["body"].as_u64().unwrap(), out.value["face"].as_u64().unwrap());
        s2.execute("replace_face", &json!({"body": body, "faces": [top], "to": {"body": cyl.0, "face": side}})).unwrap_or_else(|e| panic!("{what}: {e}"));
        let b = s2.doc.bodies.iter().find(|b| b.id.0 == body).unwrap();
        let v = s2.kernel.mass_props(&b.shape).unwrap().volume;
        // 20 × ∫ √(900 − x²) over −15…15.
        let want = 20.0 * (15.0 * 675f64.sqrt() + 900.0 * 0.5f64.asin());
        assert!((v - want).abs() < want * 1e-5, "{what}: {v} vs {want}");
    }
}

/// A loft's settings past the plain one: guides, tangent and curvature ends, and the ring.
#[test]
fn lofts_follow_guides_leave_ends_square_and_close_into_rings() {
    use std::f64::consts::PI;
    // Two circles of radius 10, 40 apart along z.
    let circles = |radii: &[(f64, f64)]| {
        let mut s = session();
        let mut ids = Vec::new();
        for (z, r) in radii {
            s.execute("sketch", &json!({"origin": [0, 0, z], "normal": [0, 0, 1], "x_dir": [1, 0, 0]})).unwrap();
            s.execute("circle", &json!({"center": [0, 0], "radius": r})).unwrap();
            ids.push(json!({"sketch": s.active_sketch.unwrap().0, "region": 0}));
        }
        (s, ids)
    };
    // Straight up with G1 at both ends is the plain cylinder.
    let (mut s, p) = circles(&[(0.0, 10.0), (40.0, 10.0)]);
    s.execute("loft", &json!({"profiles": p, "start_continuity": "g1", "end_continuity": "g1"})).unwrap();
    assert!((volume(&mut s) - PI * 4000.0).abs() < 0.05, "{} vs {}", volume(&mut s), PI * 4000.0);
    // Radius 10 to 4, G1 both ends: r(t) = 10 − 6(3t² − 2t³).
    for (how, want) in [
        (json!({"start_continuity": "g1", "end_continuity": "g1"}), {
            let r = |t: f64| 10.0 - 6.0 * t * t * (3.0 - 2.0 * t);
            (0..10_000).map(|i| (f64::from(i) + 0.5) / 10_000.0).map(|t| PI * r(t) * r(t) * 40.0 / 10_000.0).sum::<f64>()
        }),
        (json!({"start_continuity": "g2", "end_continuity": "g2"}), {
            let r = |t: f64| 10.0 - 6.0 * t.powi(3) * (10.0 - 15.0 * t + 6.0 * t * t);
            (0..10_000).map(|i| (f64::from(i) + 0.5) / 10_000.0).map(|t| PI * r(t) * r(t) * 40.0 / 10_000.0).sum::<f64>()
        }),
    ] {
        let (mut s, p) = circles(&[(0.0, 10.0), (40.0, 4.0)]);
        let mut params = how.clone();
        params["profiles"] = json!(p);
        s.execute("loft", &params).unwrap();
        // The slope's length is the mean gap (a little over 40), so the height wobbles a hair: 1 %.
        let v = volume(&mut s);
        assert!((v - want).abs() < want * 0.01, "{how}: {v} vs {want}");
    }
    // A guide on one side of two circles, bulging out to x = 16 half way; mirrored by a second: an ellipse-like waist.
    let (mut s, p) = circles(&[(0.0, 10.0), (40.0, 10.0)]);
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    let g = s.active_sketch.unwrap();
    let pl = s.doc.sketch(g).unwrap().plane;
    let uv = |x: f64, z: f64| { let d = glam::dvec3(x, 0.0, z) - pl.origin; json!([d.dot(pl.x), d.dot(pl.y)]) };
    for x in [1.0, -1.0] {
        s.execute("arc", &json!({"a": uv(10.0 * x, 0.0), "mid": uv(16.0 * x, 20.0), "b": uv(10.0 * x, 40.0)})).unwrap();
    }
    let before = s.doc.bodies.len();
    s.execute("loft", &json!({"profiles": p, "guides": [{"sketch": g.0, "entities": [0, 1]}]})).unwrap();
    assert_eq!(s.doc.bodies.len(), before + 1);
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.x - 16.0).abs() < 0.02 && (bb.min.x + 16.0).abs() < 0.02, "{bb:?}");
    assert!((bb.max.y - 10.0).abs() < 0.05, "the sides between the guides stay: {bb:?}");
    assert!(volume(&mut s) > PI * 4000.0 * 1.2);
    // Squares with four guides at the corners.
    let mut s = session();
    let mut p = Vec::new();
    for z in [0, 30] {
        s.execute("sketch", &json!({"origin": [0, 0, z], "normal": [0, 0, 1], "x_dir": [1, 0, 0]})).unwrap();
        s.execute("rectangle", &json!({"center": [0, 0], "size": [20, 20]})).unwrap();
        p.push(json!({"sketch": s.active_sketch.unwrap().0, "region": 0}));
    }
    let mut guides = Vec::new();
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0)] {
        // The diagonal plane through two opposite corners.
        s.execute("sketch", &json!({"origin": [0, 0, 0], "normal": [-sy, sx, 0], "x_dir": [sx, sy, 0]})).unwrap();
        let g = s.active_sketch.unwrap();
        let pl = s.doc.sketch(g).unwrap().plane;
        let uv = |q: glam::DVec3| { let d = q - pl.origin; json!([d.dot(pl.x), d.dot(pl.y)]) };
        for k in [1.0, -1.0] {
            let at = |r: f64, z: f64| uv(glam::dvec3(sx * r * k, sy * r * k, z));
            s.execute("arc", &json!({"a": at(10.0, 0.0), "mid": at(6.0, 15.0), "b": at(10.0, 30.0)})).unwrap();
        }
        guides.push(json!({"sketch": g.0}));
    }
    s.execute("loft", &json!({"profiles": p, "guides": guides})).unwrap();
    let v = volume(&mut s);
    assert!(v < 12000.0 * 0.75 && v > 12000.0 * 0.4, "waisted: {v}");
    // The ring: four circles round the z axis make a torus, near enough.
    let mut s = session();
    let mut p = Vec::new();
    for k in 0..8 {
        let a = f64::from(k) * PI / 4.0;
        s.execute("sketch", &json!({"origin": [30.0 * a.cos(), 30.0 * a.sin(), 0], "normal": [-a.sin(), a.cos(), 0], "x_dir": [a.cos(), a.sin(), 0]})).unwrap();
        s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
        p.push(json!({"sketch": s.active_sketch.unwrap().0, "region": 0}));
    }
    s.execute("loft", &json!({"profiles": p, "periodic": true})).unwrap();
    let want = 2.0 * PI * 30.0 * PI * 25.0;
    let v = volume(&mut s);
    assert!((v - want).abs() < want * 0.01, "{v} vs {want}");
    // A face's neighbours carry on: a tapered frustum's walls go straight on (G1 from its top).
    let (mut s, _) = circles(&[(0.0, 10.0), (20.0, 5.0)]);
    let ids: Vec<u64> = s.doc.sketches.iter().map(|k| k.id.0).collect();
    s.execute("loft", &json!({"profiles": ids.iter().map(|i| json!({"sketch": i, "region": 0})).collect::<Vec<_>>()})).unwrap();
    let body = s.doc.bodies[0].id;
    let shape = s.doc.bodies[0].shape.clone();
    let top = (0..s.kernel.face_count(&shape).unwrap())
        .find(|&f| s.kernel.face_plane(&shape, f).unwrap().is_some_and(|p| p.normal.z > 0.9))
        .unwrap();
    s.execute("sketch", &json!({"origin": [0, 0, 40], "normal": [0, 0, 1], "x_dir": [1, 0, 0]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    let cap = s.active_sketch.unwrap();
    s.execute(
        "loft",
        &json!({"profiles": [{"body": body.0, "face": top}, {"sketch": cap.0, "region": 0}], "start_continuity": "g1", "result": "new"}),
    )
    .unwrap();
    // It narrows first, on from the cone, before coming back to 5.
    let bb = s.kernel.mass_props(&s.doc.bodies[1].shape).unwrap();
    assert!(bb.volume < PI * 25.0 * 20.0 * 0.95, "{}", bb.volume);
}

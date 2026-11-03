//! Solid features from profiles: extrude (draft, extents, results), loft, sweep and fill.

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
    s.execute("import_file", &json!({"path": path})).unwrap();
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

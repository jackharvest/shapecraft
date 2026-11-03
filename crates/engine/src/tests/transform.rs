//! Booleans, split body, and moving, rotating, scaling, aligning and patterning bodies.

use serde_json::json;
use shapecraft_doc::Pick;

use super::{holed_block, session, volume};
use crate::Session;

#[test]
fn booleans_and_fillet() {
    let mut s = session();
    for x in [0.0, 5.0] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [x, 0], "opposite": [x + 10.0, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    }
    let (a, b) = (s.doc.bodies[0].id, s.doc.bodies[1].id);
    s.execute("union", &json!({"target": a.0, "tools": [b.0]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - 1500.0).abs() < 1e-6);
    s.execute("fillet", &json!({"body": a.0, "radius": 1})).unwrap();
    assert!(volume(&mut s) < 1500.0);
}

#[test]
fn linear_pattern_copies_and_merges() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [2, 2]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 2})).unwrap();
    s.execute("pattern", &json!({"items": [2], "direction": [1, 0, 0], "spacing": 5, "count": 4})).unwrap();
    assert_eq!(s.doc.bodies.len(), 4);
    s.execute("undo", &json!({})).unwrap();
    s.execute("pattern", &json!({"items": [2], "direction": [1, 0, 0], "spacing": 1, "count": 3, "merge": true})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - 4.0 * 2.0 * 2.0).abs() < 1e-6, "three overlapping cubes make a 4×2×2 bar");
}

#[test]
fn a_patterns_result_badge_subtracts_or_intersects_the_copies() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [2, 2]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 2})).unwrap();
    let pattern = |result: &str| json!({"items": [2], "direction": [1, 0, 0], "spacing": 1, "count": 2, "result": result});
    s.execute("pattern", &pattern("subtract")).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - 4.0).abs() < 1e-6, "the shifted copy takes half the cube");
    s.execute("undo", &json!({})).unwrap();
    s.execute("pattern", &pattern("intersect")).unwrap();
    assert!((volume(&mut s) - 4.0).abs() < 1e-6, "what the cube and its copy share");
    s.execute("undo", &json!({})).unwrap();
    s.execute("pattern", &pattern("new")).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!(s.execute("pattern", &pattern("sideways")).is_err());
}

#[test]
fn scale_offset_split_and_circular_pattern() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    // The cap is selected after the extrude: offset it.
    s.execute("offset_face", &json!({"distance": 2})).unwrap();
    assert!((volume(&mut s) - 1200.0).abs() < 1e-3);
    s.execute("scale", &json!({"items": [2], "factor": 0.5})).unwrap();
    assert!((volume(&mut s) - 150.0).abs() < 1e-3);
    s.execute("undo", &json!({})).unwrap();
    s.execute("split_body", &json!({"items": [2], "origin": [5, 0, 0], "normal": [1, 0, 0]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!((volume(&mut s) - 1200.0).abs() < 1e-3);
    s.execute("undo", &json!({})).unwrap();
    // Keep Originals: the body stays whole and both pieces are added.
    s.execute("split_body", &json!({"items": [2], "origin": [5, 0, 0], "normal": [1, 0, 0], "keep": true})).unwrap();
    assert_eq!(s.doc.bodies.len(), 3);
    assert!((volume(&mut s) - 2400.0).abs() < 1e-3);
    s.execute("undo", &json!({})).unwrap();
    // A scaled copy leaves the original as it was; non-uniform scaling keeps flat faces flat.
    s.execute("scale", &json!({"items": [2], "factors": [1, 1, 2], "copy": true})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!((volume(&mut s) - 3600.0).abs() < 1e-3);
    let copy = s.doc.bodies[1].shape.clone();
    assert!(s.kernel.face_plane(&copy, 0).unwrap().is_some());
    s.execute("undo", &json!({})).unwrap();
    s.execute("pattern_circular", &json!({"items": [2], "origin": [-20, 0, 0], "axis": [0, 0, 1], "count": 6})).unwrap();
    assert_eq!(s.doc.bodies.len(), 6);
    assert!(s.execute("pattern_circular", &json!({"items": [2], "origin": [0, 0, 0], "axis": [0, 0, 0], "count": 3})).is_err());
}

#[test]
fn revolve_a_face_around_an_edge_and_rotate_about_an_edge() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let body = &v["bodies"][0];
    let id = body["id"].as_u64().unwrap();
    // The face at x = 10, spun a quarter turn around a line parallel to y above it. Away from the body it only
    // touches it there, so it grows the body (the manual's automatic Union).
    let side = body["faces"].as_array().unwrap().iter().find(|f| f["normal"][0].as_f64() == Some(1.0)).unwrap()["face"].as_u64().unwrap();
    let away = json!({"body": id, "face": side, "axis": {"origin": [10, 0, 12], "dir": [0, 1, 0]}, "angle": -90});
    assert_eq!(crate::cmd::revolve_auto(&s, &away).unwrap(), "union");
    s.execute("revolve", &away).unwrap();
    assert_eq!(s.doc.bodies.len(), 1, "merged into its body");
    assert!(volume(&mut s) > 200.0);
    s.execute("undo", &json!({})).unwrap();
    // The other way it sweeps back through the body: a new body, as the reference did (s25 r2).
    let through = json!({"body": id, "face": side, "axis": {"origin": [10, 0, 12], "dir": [0, 1, 0]}, "angle": 90});
    assert_eq!(crate::cmd::revolve_auto(&s, &through).unwrap(), "new");
    s.execute("revolve", &through).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.execute("revolve", &json!({"body": id, "face": side})).is_err(), "a face needs an axis");
    s.execute("rotate", &json!({"items": [id], "about": {"body": id, "edge": 0}, "angle": 90})).unwrap();
}

#[test]
fn align_puts_one_face_on_another() {
    let mut s = session();
    let block = |s: &mut Session, origin: [f64; 3], normal: [f64; 3]| {
        s.execute("sketch", &json!({"origin": origin, "normal": normal})).unwrap();
        s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 4})).unwrap();
        s.doc.bodies.last().unwrap().id.0
    };
    let a = block(&mut s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let b = block(&mut s, [50.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let face = |body: u64, n: [f64; 3]| {
        v["bodies"].as_array().unwrap().iter().find(|x| x["id"].as_u64() == Some(body)).unwrap()["faces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| (0..3).all(|i| (f["normal"][i].as_f64().unwrap_or(9.0) - n[i]).abs() < 1e-9))
            .unwrap()["face"]
            .as_u64()
            .unwrap()
    };
    // Put b's far end (normal +x) onto a's top (normal +z).
    s.execute(
        "align",
        &json!({"from": {"body": b, "face": face(b, [1.0, 0.0, 0.0])}, "to": {"body": a, "face": face(a, [0.0, 0.0, 1.0])}}),
    )
    .unwrap();
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let bb = &v["bodies"].as_array().unwrap().iter().find(|x| x["id"].as_u64() == Some(b)).unwrap()["bounds"];
    assert!((bb["min"][2].as_f64().unwrap() - 4.0).abs() < 1e-6, "{bb}");
    assert!((bb["max"][2].as_f64().unwrap() - 8.0).abs() < 1e-6, "{bb}");
}

#[test]
fn move_and_rotate_copies() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    let id = s.doc.bodies[0].id.0;
    s.execute("translate", &json!({"items": [id], "by": [20, 0, 0], "copy": true})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!(matches!(s.selection[..], [Pick::Body { .. }]));
    s.execute("rotate", &json!({"items": [id], "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 45, "copy": true})).unwrap();
    assert_eq!(s.doc.bodies.len(), 3);
}

#[test]
fn copies_and_booleans_with_two_bodies() {
    let mut s = session();
    let body = holed_block(&mut s);
    // A copy 5.8 mm along Y (observed 379): "Body 01 (1)".
    let v = s.execute("translate", &json!({"items": [body.0], "by": [0, 5.8, 0], "copy": true})).unwrap().value;
    let copy = v["copies"][0].as_u64().unwrap();
    assert_eq!(s.doc.bodies[1].name, "Body 01 (1)");
    let one = volume(&mut s) / 2.0;
    // Union: one body; Keep Originals keeps both and adds the union.
    s.execute("union", &json!({"target": copy, "tools": [body.0]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!(s.selection.is_empty());
    s.execute("undo", &json!({})).unwrap();
    s.execute("union", &json!({"target": copy, "tools": [body.0], "keep": "all"})).unwrap();
    assert_eq!(s.doc.bodies.len(), 3);
    s.execute("undo", &json!({})).unwrap();
    // Subtract the original from the copy: what's left keeps the copy's name, and comes apart into pieces.
    s.execute("subtract", &json!({"target": copy, "tools": [body.0]})).unwrap();
    assert!(s.doc.bodies.len() >= 2, "{}", s.doc.bodies.len());
    assert!(s.doc.bodies.iter().all(|b| b.name == "Body 01 (1)"));
    assert!(volume(&mut s) < one);
}

#[test]
fn split_body_with_sketch_lines_planes_and_misses() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    let body = s.doc.bodies[0].id;
    // A short line on the top runs on past its ends: a diagonal cut across the whole body.
    s.execute("sketch", &json!({"origin": [0, 0, 10], "normal": [0, 0, 1]})).unwrap();
    let plane = s.doc.sketches.last().unwrap().plane;
    let (a, b) = (plane.to_local(glam::DVec3::new(4.0, 4.0, 10.0)), plane.to_local(glam::DVec3::new(6.0, 6.0, 10.0)));
    s.execute("line", &json!({"a": [a.x, a.y], "b": [b.x, b.y]})).unwrap();
    let sk = s.doc.sketches.last().unwrap().id;
    s.execute("exit_sketching", &json!({})).unwrap();
    s.execute("split_body", &json!({"items": [body.0], "with": [{"sketch": sk.0, "entity": 0}]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!(s.doc.bodies.iter().all(|b| b.name == s.doc.bodies[0].name), "the pieces keep the body's name");
    assert!((volume(&mut s) - 1000.0).abs() < 1e-3);
    s.execute("undo", &json!({})).unwrap();
    // Two tools at once: four pieces.
    s.execute(
        "split_body",
        &json!({"items": [body.0], "with": [{"origin": [5, 0, 0], "normal": [1, 0, 0]}, {"origin": [0, 5, 0], "normal": [0, 1, 0]}]}),
    )
    .unwrap();
    assert_eq!(s.doc.bodies.len(), 4);
    s.execute("undo", &json!({})).unwrap();
    // A plane that misses the body fails the split.
    let e = s.execute("split_body", &json!({"items": [body.0], "with": [{"origin": [0, 0, 20], "normal": [0, 0, 1]}]})).unwrap_err();
    assert!(e.to_string().contains("should intersect"));
    assert_eq!(s.doc.bodies.len(), 1);
}

#[test]
fn move_copies_uniform_patterns_and_point_edge_align() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [10, 0], "opposite": [14, 2]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    let a = s.doc.bodies[0].id.0;
    // Three copies, 10 mm apart.
    let out = s.execute("translate", &json!({"items": [a], "by": [0, 10, 0], "copy": true, "copies": 3})).unwrap().value;
    assert_eq!(out["copies"].as_array().unwrap().len(), 3);
    let far = s.doc.bodies.last().unwrap();
    assert!((s.kernel.bounds(&far.shape).unwrap().min.y - 30.0).abs() < 1e-6);
    for id in out["copies"].as_array().unwrap() {
        s.execute("delete", &json!({"items": [id]})).unwrap();
    }
    // A uniform circular pattern keeps each copy square to the axes; the count can be a variable.
    s.execute("set_variable", &json!({"name": "n", "value": "4", "kind": "number"})).unwrap();
    s.execute("pattern_circular", &json!({"items": [a], "origin": [0, 0, 0], "axis": [0, 0, 1], "count": "n", "orientation": "uniform"}))
        .unwrap();
    assert_eq!(s.doc.bodies.len(), 4);
    let quarter = s.kernel.bounds(&s.doc.bodies[1].shape).unwrap();
    let size = quarter.max - quarter.min;
    assert!((size.x - 4.0).abs() < 1e-6 && (size.y - 2.0).abs() < 1e-6, "not turned: {size}");
    // Align: a point onto a point, and an edge parallel to an edge.
    let b = s.doc.bodies[1].id.0;
    s.execute("align", &json!({"from": {"point": [0, 0, 0]}, "to": {"point": [0, 0, 50]}, "items": [b]})).unwrap();
    assert!((s.kernel.bounds(&s.doc.body(shapecraft_doc::ItemId(b)).unwrap().shape).unwrap().min.z - 50.0).abs() < 1e-6);
    let edge_along = |s: &crate::Session, body: u64, dir: glam::DVec3| {
        let bd = s.doc.body(shapecraft_doc::ItemId(body)).unwrap();
        let k = bd
            .mesh
            .edges
            .iter()
            .position(|l| {
                let (p, q) = (l[0], l[l.len() - 1]);
                let d = glam::DVec3::new((q[0] - p[0]) as f64, (q[1] - p[1]) as f64, (q[2] - p[2]) as f64);
                d.length() > 1.0 && d.normalize().cross(dir).length() < 1e-6
            })
            .unwrap();
        bd.mesh.edge_ids[k]
    };
    let (ea, eb) = (edge_along(&s, a, glam::DVec3::Z), edge_along(&s, b, glam::DVec3::X));
    s.execute("align", &json!({"from": {"body": a, "edge": ea}, "to": {"body": b, "edge": eb}})).unwrap();
    let bb = s.kernel.bounds(&s.doc.body(shapecraft_doc::ItemId(a)).unwrap().shape).unwrap();
    assert!((bb.max.z - bb.min.z - 2.0).abs() > 0.5, "the upright edge now lies along X: {bb:?}");
    assert!(
        s.execute(
            "pattern_circular",
            &json!({"items": [a], "origin": [0, 0, 0], "axis": [0, 0, 1], "count": 3, "orientation": "sideways"})
        )
        .is_err()
    );
}

/// Move with Copy and the Link badge off (26.200): History gets an "Unlinked Copy 01" step that keeps the copy's
/// geometry, so changing the extrude it was copied from later leaves the copy as it was; a linked copy follows.
#[test]
fn an_unlinked_copy_keeps_its_geometry_when_earlier_steps_change() {
    let run = |linked: bool| {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
        let body = s.doc.bodies[0].id.0;
        s.execute("translate", &json!({"items": [body], "by": [20, 0, 0], "copy": true, "linked": linked})).unwrap();
        let last = s.doc.history.last().unwrap();
        assert_eq!(last.label, if linked { "Movement/Rotation 01" } else { "Unlinked Copy 01" });
        assert_eq!(s.doc.bodies.len(), 2);
        let ex = s.doc.history.iter().position(|h| h.kind == "extrude").unwrap();
        s.execute("edit_step", &json!({"step": ex, "params": {"distance": 4}})).unwrap();
        assert_eq!(s.doc.bodies.len(), 2);
        let tops: Vec<f64> = s.doc.bodies.iter().map(|b| s.kernel.bounds(&b.shape).unwrap().max.z).collect();
        tops
    };
    let unlinked = run(false);
    assert!((unlinked[0] - 4.0).abs() < 1e-6 && (unlinked[1] - 10.0).abs() < 1e-6, "{unlinked:?}");
    let linked = run(true);
    assert!((linked[0] - 4.0).abs() < 1e-6 && (linked[1] - 4.0).abs() < 1e-6, "{linked:?}");
    // Rotated copies freeze the same way; bad frozen text is refused, not a crash.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    let body = s.doc.bodies[0].id.0;
    s.execute("rotate", &json!({"items": [body], "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 90, "copy": true, "linked": false}))
        .unwrap();
    assert_eq!(s.doc.history.last().unwrap().label, "Unlinked Copy 01");
    assert!(s.execute("translate", &json!({"by": [0, 0, 0], "frozen": [[body, "garbage"]]})).is_err());
}

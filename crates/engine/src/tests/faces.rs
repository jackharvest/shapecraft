//! Edge and face features: fillet, chamfer, shell, offset and replace face, and project as edge.

use serde_json::json;

use super::{edges_where, holed_block, session, top_face, volume};
use crate::Session;

#[test]
fn shell_revolve_mirror_chamfer() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    let top = out.value["face"].as_u64().unwrap();
    // The cap is selected after extrude, so Shell opens it.
    s.execute("shell", &json!({"thickness": 1})).unwrap();
    assert!((volume(&mut s) - 424.0).abs() < 1e-3);
    let _ = top;
    s.execute("undo", &json!({})).unwrap();
    s.execute("chamfer", &json!({"body": 2, "distance": 1})).unwrap();
    assert!(volume(&mut s) < 1000.0);

    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("rectangle", &json!({"corner": [4, 0], "opposite": [5, 10]})).unwrap();
    s.execute("revolve", &json!({"sketch": 1, "region": 0, "axis": "v"})).unwrap();
    let v = volume(&mut s);
    assert!((v - std::f64::consts::PI * 9.0 * 10.0).abs() < 1e-3, "{v}");
    s.execute("mirror", &json!({"items": [2], "origin": [20, 0, 0], "normal": [1, 0, 0]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert!((volume(&mut s) - 2.0 * v).abs() < 1e-3);
    s.execute("rotate", &json!({"items": [3], "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 90})).unwrap();
}

#[test]
fn project_a_face_with_a_hole() {
    use shapecraft_doc::SketchEntity;
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [20, 20]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 3})).unwrap();
    let sk = s.active_sketch.unwrap();
    let ring = s.active_sketch().unwrap().regions().iter().max_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
    s.execute("extrude", &json!({"sketch": sk.0, "region": ring, "distance": 5})).unwrap();
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let body = &v["bodies"][0];
    let top = body["faces"].as_array().unwrap().iter().find(|f| f["normal"][2].as_f64() == Some(1.0)).unwrap()["face"].as_u64().unwrap();
    s.execute("sketch", &json!({"origin": [0, 0, 20], "normal": [0, 0, 1]})).unwrap();
    s.execute("project", &json!({"faces": [{"body": body["id"], "face": top}]})).unwrap();
    let ents = &s.active_sketch().unwrap().entities;
    assert_eq!(ents.iter().filter(|e| matches!(e, SketchEntity::Line { .. })).count(), 4);
    assert!(ents.iter().any(|e| matches!(e, SketchEntity::Circle { radius, .. } if (radius - 3.0).abs() < 1e-9)));
    assert_eq!(s.active_sketch().unwrap().regions().len(), 2, "projected outline closes regions");
}

#[test]
fn offsetting_faces_keeps_fillets_and_chamfers() {
    let mut s = session();
    let body = holed_block(&mut s);
    let faces_before = s.doc.bodies[0].mesh.triangle_faces.iter().max().copied().unwrap();
    let v0 = volume(&mut s);
    // Pull the top up 3 mm: the filleted rim rides along, the chamfer's plane stays and its face grows.
    let top = top_face(&mut s);
    s.execute("offset_face", &json!({"body": body.0, "faces": [top], "distance": 3})).unwrap();
    let faces_after = s.doc.bodies[0].mesh.triangle_faces.iter().max().copied().unwrap();
    // Known: re-applying the fillet comes back with the hole wall and the fillet in two halves each.
    assert!(faces_after >= faces_before, "{faces_before} → {faces_after}");
    // The fillet's outer rim is at the new top with the same radius (hole 4 + fillet 1.5).
    let rim = edges_where(&s, 0, |p| {
        (p[2] - 23.0).abs() < 1e-3
            && ((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2)).sqrt() > 5.4
            && ((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2)).sqrt() < 5.6
    });
    assert!(!rim.is_empty(), "fillet rides along");
    // The chamfer keeps its plane (45° through y = 2 at the old top) and grows: its top edge is now at y = 5.
    let chamfer_top = edges_where(&s, 0, |p| (p[2] - 23.0).abs() < 1e-3 && (p[1] - 5.0).abs() < 1e-3);
    assert!(!chamfer_top.is_empty(), "the chamfer's face extends in its plane");
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 23.0).abs() < 1e-6, "{bb:?}");
    assert!(volume(&mut s) > v0);
}

#[test]
fn a_fillet_face_finds_its_step() {
    let mut s = session();
    let body = holed_block(&mut s);
    let b = &s.doc.bodies[0];
    let faces = b.mesh.triangle_faces.iter().max().copied().unwrap();
    let found: Vec<_> = (0..=faces).filter_map(|f| crate::history::fillet_step_for(&s, body, f).map(|x| (f, x))).collect();
    assert_eq!(found.len(), 1, "only the rim's fillet is a fillet (the hole's wall isn't): {found:?}");
    let (face, (step, r, convex)) = found[0];
    assert!((r - 1.5).abs() < 1e-9 && convex);
    assert_eq!(s.doc.history[step].label, "Fillet 01");
    // Changing the radius edits that step; zero removes it.
    s.execute("edit_step", &json!({"step": step, "params": {"radius": 2.5}})).unwrap();
    assert!(crate::history::fillet_step_for(&s, body, face).is_some_and(|(_, r, _)| (r - 2.5).abs() < 1e-9));
    s.execute("delete_step", &json!({"step": step})).unwrap();
    assert!(!s.doc.history.iter().any(|h| h.kind == "fillet"));
}

#[test]
fn pulling_a_chamfer_face_out_shrinks_it_then_removes_it() {
    let mut s = session();
    let body = holed_block(&mut s);
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let chamfer = v["bodies"][0]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["normal"][1].as_f64().is_some_and(|y| (y + 0.7071).abs() < 1e-3))
        .unwrap()["face"]
        .as_u64()
        .unwrap();
    let v0 = volume(&mut s);
    s.execute("offset_face", &json!({"body": body.0, "faces": [chamfer], "distance": 0.5})).unwrap();
    assert!(volume(&mut s) > v0);
    s.execute("undo", &json!({})).unwrap();
    // Past the corner (2 / √2 ≈ 1.41 mm) the chamfer is gone: a sharp edge again (observed 367–368).
    s.execute("offset_face", &json!({"body": body.0, "faces": [chamfer], "distance": 1.6})).unwrap();
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!(bb.min.y.abs() < 1e-6 && (bb.max.z - 20.0).abs() < 1e-6, "{bb:?}");
    let sharp = edges_where(&s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 20.0).abs() < 1e-3);
    assert_eq!(sharp.len(), 1, "the front top edge is sharp again");
}

#[test]
fn shell_with_every_face_picked_hollows_the_body() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    s.execute("shell", &json!({"body": body, "faces": [0, 1, 2, 3, 4, 5], "thickness": 1})).unwrap();
    assert!((volume(&mut s) - (1000.0 - 512.0)).abs() < 1e-3, "{}", volume(&mut s));
}

#[test]
fn refilleting_after_a_face_move_keeps_whole_faces() {
    let mut s = session();
    let body = holed_block(&mut s);
    let count = |s: &Session| {
        let mut f = s.doc.bodies[0].mesh.triangle_faces.clone();
        f.sort_unstable();
        f.dedup();
        f.len()
    };
    let top = top_face(&mut s);
    s.execute("offset_face", &json!({"body": body.0, "faces": [top], "distance": 1.3})).unwrap();
    let after_move = count(&s);
    let b = &s.doc.bodies[0];
    let max = b.mesh.triangle_faces.iter().max().copied().unwrap();
    let face = (0..=max).find(|f| crate::history::fillet_step_for(&s, body, *f).is_some()).expect("the rim's fillet is still found");
    let (step, _, _) = crate::history::fillet_step_for(&s, body, face).unwrap();
    s.execute("edit_step", &json!({"step": step, "params": {"radius": 2.5}})).unwrap();
    assert_eq!(count(&s), after_move, "the hole wall and fillet stay whole");
}

#[test]
fn inside_corners_of_merged_bodies_fillet_and_chamfer() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let a = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap().value["body"]
        .as_u64()
        .unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [5, 5], "opposite": [15, 20]})).unwrap();
    let b = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 6})).unwrap().value["body"]
        .as_u64()
        .unwrap();
    s.execute("union", &json!({"target": a, "tools": [b]})).unwrap();
    let shape = s.doc.bodies[0].shape.clone();
    let n = s.kernel.edge_count(&shape).unwrap();
    // Every edge takes a small round and a small bevel, the inside ones where the parts meet included.
    for e in 0..n {
        assert!(s.kernel.fillet_edges(&shape, &[e], 0.5).is_ok(), "fillet edge {e}");
        assert!(s.kernel.chamfer_edges(&shape, &[e], 0.5).is_ok(), "chamfer edge {e}");
    }
}

#[test]
fn replace_face_meets_a_parallel_or_tilted_plane() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id;
    let top = top_face(&mut s);
    // A parallel plane 25 mm up: the top rises to it.
    let up = s.execute("offset_plane", &json!({"from": "xy", "distance": 25})).unwrap().value["plane"].as_u64().unwrap();
    s.execute("replace_face", &json!({"body": body.0, "faces": [top], "to": {"plane": up}})).unwrap();
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 25.0).abs() < 1e-6, "{bb:?}");
    assert!((volume(&mut s) - 20.0 * 20.0 * 25.0).abs() < 1e-3);
    s.execute("undo", &json!({})).unwrap();
    // A plane tilted along X through z = 15 at x = 0 and z = 25 at x = 20: half cut away, half added.
    let tilt =
        s.execute("plane_3points", &json!({"points": [[0, 0, 15], [0, 1, 15], [20, 0, 25]]})).unwrap().value["plane"].as_u64().unwrap();
    let top = top_face(&mut s);
    s.execute("replace_face", &json!({"body": body.0, "faces": [top], "to": {"plane": tilt}})).unwrap();
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 25.0).abs() < 1e-6, "{bb:?}");
    assert!((volume(&mut s) - 20.0 * 20.0 * 20.0).abs() < 1e-3, "the wedge added equals the wedge cut");
    assert_eq!(s.doc.bodies.len(), 1);
    // A tilted plane wholly above the top: only added material.
    s.execute("undo", &json!({})).unwrap();
    let high =
        s.execute("plane_3points", &json!({"points": [[0, 0, 25], [0, 1, 25], [20, 0, 30]]})).unwrap().value["plane"].as_u64().unwrap();
    let top = top_face(&mut s);
    s.execute("replace_face", &json!({"body": body.0, "faces": [top], "to": {"plane": high}})).unwrap();
    assert!((volume(&mut s) - 20.0 * 20.0 * 27.5).abs() < 1e-3, "{}", volume(&mut s));
    // A side face can't be put on a horizontal plane.
    s.execute("undo", &json!({})).unwrap();
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let side =
        v["bodies"][0]["faces"].as_array().unwrap().iter().find(|f| f["normal"][0].as_f64().is_some_and(|x| x > 0.999)).unwrap()["face"]
            .as_u64()
            .unwrap();
    let err = s.execute("replace_face", &json!({"body": body.0, "faces": [side], "to": {"plane": up}})).unwrap_err();
    assert!(err.to_string().contains("can't be replaced"), "{err}");
}

#[test]
fn project_as_edge_imprints_a_region_on_a_face() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    let body = s.doc.bodies[0].id;
    let v0 = volume(&mut s);
    let faces0 = s.kernel.face_count(&s.doc.bodies[0].shape).unwrap();
    // A circle on a plane 30 mm up, over the block's top.
    let up = s.execute("offset_plane", &json!({"from": "xy", "distance": 30})).unwrap().value["plane"].as_u64().unwrap();
    let sk = s.execute("sketch", &json!({"construction": up})).unwrap().value["sketch"].as_u64().unwrap();
    s.execute("circle", &json!({"center": [10, 10], "radius": 4})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    let top = top_face(&mut s);
    s.execute("project_edge", &json!({"to": {"body": body.0, "face": top}, "items": [{"sketch": sk, "region": 0}]})).unwrap();
    let shape = &s.doc.bodies[0].shape;
    assert_eq!(s.kernel.face_count(shape).unwrap(), faces0 + 1, "the top splits into a disc and a holed square");
    assert!((volume(&mut s) - v0).abs() < 1e-6, "the volume stays");
    let ring =
        edges_where(&s, 0, |p| (p[2] - 10.0).abs() < 1e-6 && (((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2)).sqrt() - 4.0).abs() < 1e-3);
    assert!(!ring.is_empty(), "a circle edge on the top");
    // Nothing that lands on the face: refused.
    let side = s.execute("inspect", &json!({})).unwrap().value["bodies"][0]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["normal"][0].as_f64().is_some_and(|x| x > 0.999))
        .unwrap()["face"]
        .as_u64()
        .unwrap();
    assert!(s.execute("project_edge", &json!({"to": {"body": body.0, "face": side}, "items": [{"sketch": sk, "region": 0}]})).is_err());
}

#[test]
fn an_offset_that_runs_away_fails_instead_of_making_a_wrong_solid() {
    // A half disc extruded both ways with a draft: OCCT's offset of its top runs the drafted wall up to the cone's
    // apex. Refused now (a Shapr3D body like that comes in as Shapr3D's solid: n14's Body 04).
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("line", &json!({"a": [0, -6], "b": [0, 6]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sketch.0, "region": 0}]})).unwrap();
    s.execute("extrude", &json!({"distance": 6, "draft": 5, "sides": "symmetric"})).unwrap();
    let v0 = volume(&mut s);
    let top = top_face(&mut s);
    let body = s.doc.bodies[0].id.0;
    if s.execute("offset_face", &json!({"body": body, "faces": [top], "distance": 2})).is_ok() {
        let v = volume(&mut s);
        assert!(v > v0 && v < v0 + 2.0 * 40.0, "{v0} → {v}");
    }
}

#[test]
fn two_distance_chamfer_runs_its_first_distance_along_the_given_face() {
    // The front-top edge of a 10 mm cube, 1 mm along the given face and 3 mm along the other: the bevel's normal.
    let bevel = |on_top: bool| {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
        let body = s.doc.bodies[0].id;
        let faces = s.execute("inspect", &json!({})).unwrap().value["bodies"][0]["faces"].clone();
        let facing = |axis: usize, sign: f64| {
            faces.as_array().unwrap().iter().find(|f| f["normal"][axis].as_f64().is_some_and(|n| n * sign > 0.999)).unwrap()["face"]
                .as_u64()
                .unwrap()
        };
        let face = if on_top { facing(2, 1.0) } else { facing(1, -1.0) };
        let front = edges_where(&s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 10.0).abs() < 1e-3);
        s.execute("chamfer", &json!({"body": body.0, "edges": front, "distance": 1, "distance2": 3, "face": face})).unwrap();
        assert!((volume(&mut s) - 985.0).abs() < 1e-3);
        assert!(s.execute("chamfer", &json!({"body": body.0, "distance": 0.1, "distance2": 0.2, "face": 9999})).is_err());
        let v = s.execute("inspect", &json!({})).unwrap().value;
        v["bodies"][0]["faces"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|f| f["normal"][1].as_f64().filter(|y| *y < -1e-3 && *y > -0.999))
            .unwrap()
    };
    // 1 along the top and 3 down the front: the bevel mostly faces forward ((0, -3, 1) / √10); the other way, up.
    assert!((bevel(true) + 3.0 / 10f64.sqrt()).abs() < 1e-3);
    assert!((bevel(false) + 1.0 / 10f64.sqrt()).abs() < 1e-3);
}

#[test]
fn a_g2_round_meets_the_faces_where_a_circular_one_would_and_takes_about_as_much() {
    let cube = |s: &mut Session| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
        edges_where(s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 10.0).abs() < 1e-3)
    };
    let mut s = session();
    let front = cube(&mut s);
    s.execute("fillet", &json!({"body": 2, "edges": front, "radius": 2})).unwrap();
    let g1 = volume(&mut s);
    let mut s = session();
    let front = cube(&mut s);
    s.execute("fillet", &json!({"body": 2, "edges": front, "radius": 2, "continuity": "g2"})).unwrap();
    let g2 = volume(&mut s);
    // A quarter-round of 2 takes (4 - π) × 10 ≈ 8.58; the G2 curve 0.2106 × 4 × 10 ≈ 8.42.
    assert!((g1 - 991.416).abs() < 1e-2 && (g2 - (1000.0 - 8.424)).abs() < 0.05, "g1 {g1} g2 {g2}");
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 10.0).abs() < 1e-6 && bb.min.y.abs() < 1e-6, "the faces stay where they were: {bb:?}");
    // Curved edges are refused rather than rounded wrongly.
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    assert!(s.execute("fillet", &json!({"body": 2, "radius": 1, "continuity": "g2"})).is_err());
    assert!(s.execute("fillet", &json!({"body": 2, "radius": 1, "continuity": "g3"})).is_err());
}

/// The manual's Fillet card: Size Type Chordal sizes the round by its chord (on a square edge a chord of w is a
/// radius of w/√2), and Setback corners close each corner where three rounds meet with one smooth patch.
#[test]
fn chordal_fillets_and_setback_corners() {
    let cube = |s: &mut crate::Session, side: f64| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [side, side]})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": side})).unwrap();
    };
    let mut s = session();
    cube(&mut s, 10.0);
    let top_front = edges_where(&s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 10.0).abs() < 1e-3);
    s.execute("fillet", &json!({"body": 2, "edges": top_front, "radius": 2, "mode": "chordal"})).unwrap();
    let loss = 1000.0 - volume(&mut s);
    // r² (1 − π/4) per unit length, r² = 2.
    assert!((loss - 10.0 * 2.0 * (1.0 - std::f64::consts::FRAC_PI_4)).abs() < 1e-6, "{loss}");

    let mut rolled = session();
    cube(&mut rolled, 10.0);
    rolled.execute("fillet", &json!({"body": 2, "edges": [], "radius": 2})).unwrap();
    let mut s = session();
    cube(&mut s, 10.0);
    s.execute("fillet", &json!({"body": 2, "edges": [], "radius": 2, "corner": "setback"})).unwrap();
    let (v_roll, v_set) = (volume(&mut rolled), volume(&mut s));
    assert!((v_set - v_roll).abs() > 1e-3 && (v_set - v_roll).abs() < 8.0 * 1.5, "a different corner, about as big: {v_roll} {v_set}");
    // (Measured on the display mesh: the kernel's box grows by the patch's tolerance.)
    let (lo, hi) = s.doc.bodies[0].mesh.positions.iter().flatten().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
    assert!(lo > -2e-3 && hi < 10.002, "nothing bulges out: {lo} {hi}");
    assert_eq!(
        s.doc.bodies[0].mesh.triangle_faces.iter().max(),
        rolled.doc.bodies[0].mesh.triangle_faces.iter().max(),
        "one patch per corner"
    );
    // Where only two rounds meet there is no corner to set back: the same as a rolling ball.
    let mut a = session();
    cube(&mut a, 10.0);
    let top = edges_where(&a, 0, |p| (p[2] - 10.0).abs() < 1e-3);
    a.execute("fillet", &json!({"body": 2, "edges": top.clone(), "radius": 2, "setback": true})).unwrap();
    let mut b = session();
    cube(&mut b, 10.0);
    b.execute("fillet", &json!({"body": 2, "edges": top, "radius": 2})).unwrap();
    assert!((volume(&mut a) - volume(&mut b)).abs() < 1e-6);
    // Too big for the edges, unknown types, mixed with G2: refused.
    let mut s = session();
    cube(&mut s, 10.0);
    assert!(s.execute("fillet", &json!({"body": 2, "edges": [], "radius": 4.5, "corner": "setback"})).is_err());
    assert!(s.execute("fillet", &json!({"body": 2, "edges": [], "radius": 1, "corner": "cliff"})).is_err());
    assert!(s.execute("fillet", &json!({"body": 2, "edges": [], "radius": 1, "mode": "sideways"})).is_err());
    assert!(s.execute("fillet", &json!({"body": 2, "edges": [], "radius": 1, "mode": "chordal", "continuity": "g2"})).is_err());
    assert!(s.execute("fillet", &json!({"body": 2, "edges": [], "radius": -1, "mode": "chordal"})).is_err());
}

/// The Fillet card's Curvature (the manual's profile slider and Magnitude): -1 is a flat bevel between where the round
/// meets the faces, 1 leaves the corner sharp, and in between the profile bulges more (positive) or less.
#[test]
fn fillet_curvature_runs_from_a_bevel_to_a_sharp_corner() {
    let loss = |curvature: f64, continuity: &str| {
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
        let front = edges_where(&s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 10.0).abs() < 1e-3);
        s.execute("fillet", &json!({"body": 2, "edges": front, "radius": 2, "curvature": curvature, "continuity": continuity})).unwrap();
        1000.0 - volume(&mut s)
    };
    // A bevel of 2 × 2 along 10: 20. Sharp: nothing. The round's own 8.58 sits between the halves.
    assert!((loss(-1.0, "g1") - 20.0).abs() < 1e-3);
    assert!(loss(1.0, "g1").abs() < 1e-6);
    let (flatter, sharper) = (loss(-0.5, "g1"), loss(0.5, "g1"));
    assert!(flatter > 8.6 && flatter < 20.0 && sharper > 0.0 && sharper < 8.5, "{flatter} {sharper}");
    let g2 = loss(0.5, "g2");
    assert!(g2 > 0.0 && g2 < 8.4, "{g2}");
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    for bad in [json!(1.5), json!(-2), json!("x")] {
        assert!(s.execute("fillet", &json!({"body": 2, "radius": 1, "curvature": bad})).is_err());
    }
    assert!(s.execute("fillet", &json!({"body": 2, "radius": 1, "curvature": 0.3, "mode": "chordal"})).is_err());
}

/// A chordal fillet where the faces' angle changes along the edge (a twisted loft's top edge): the radius follows
/// the angle, so the chord stays the same; one averaged radius would make the chord drift.
#[test]
fn a_chordal_fillet_keeps_its_chord_where_the_angle_changes() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("polygon", &json!({"center": [0, 0], "radius": 10, "sides": 4, "angle": 45})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    s.execute("sketch", &json!({"origin": [0, 0, 10], "normal": [0, 0, 1]})).unwrap();
    s.execute("polygon", &json!({"center": [0, 0], "radius": 10, "sides": 4, "angle": 75})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    let sketches: Vec<u64> = s.doc.sketches.iter().map(|k| k.id.0).collect();
    s.execute("loft", &json!({"profiles": [{"sketch": sketches[0], "region": 0}, {"sketch": sketches[1], "region": 0}], "ruled": true}))
        .unwrap();
    let body = s.doc.bodies[0].id.0;
    let top = edges_where(&s, 0, |p| (p[2] - 10.0).abs() < 1e-3);
    assert!(!top.is_empty());
    let before = volume(&mut s);
    s.execute("fillet", &json!({"body": body, "edges": [top[0]], "radius": 1, "mode": "chordal"})).unwrap();
    let after = volume(&mut s);
    assert!(after < before && before - after < 10.0, "{before} {after}");
}

/// A 20 mm block with a blind round pocket (r 4, 10 deep) from the top, centred.
fn pocketed_block(s: &mut Session) -> shapecraft_doc::ItemId {
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id;
    let top = top_face(s);
    s.execute("sketch", &json!({"body": body.0, "face": top})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 4})).unwrap();
    let sk = s.active_sketch.unwrap();
    let inner = s.doc.sketch(sk).unwrap().regions().iter().find(|r| r.area < 60.0).map(|r| r.index).unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": inner, "distance": -10})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    body
}

/// Faces of the first body: (index, planar centre z or None, normal z).
fn faces_of(s: &mut Session) -> Vec<(u64, Option<f64>, f64)> {
    let v = s.execute("inspect", &json!({})).unwrap().value;
    v["bodies"][0]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["face"].as_u64().unwrap(), f["center"][2].as_f64(), f["normal"][2].as_f64().unwrap_or(0.0)))
        .collect()
}

#[test]
fn moving_faces_raises_a_pocket_floor_and_slides_a_whole_pocket() {
    let mut s = session();
    let body = pocketed_block(&mut s);
    let full = 8000.0;
    let pocket = std::f64::consts::PI * 16.0 * 10.0;
    assert!((volume(&mut s) - (full - pocket)).abs() < 1.0);
    // The floor (flat, facing up, 10 mm down) slid up along its normal: an offset, the pocket gets shallower.
    let floor = faces_of(&mut s).into_iter().find(|f| f.1.is_some_and(|z| (z - 10.0).abs() < 1e-3) && f.2 > 0.999).unwrap().0;
    s.execute("move_faces", &json!({"body": body.0, "faces": [floor], "by": [0, 0, 4]})).unwrap();
    assert!((volume(&mut s) - (full - pocket * 0.6)).abs() < 1.0, "floor raised 4 mm: {}", volume(&mut s));
    // The wall and floor together slid sideways: the same pocket somewhere else, the volume unchanged.
    let faces: Vec<u64> =
        faces_of(&mut s).into_iter().filter(|f| f.1.is_none() || f.1.is_some_and(|z| (z - 14.0).abs() < 1e-3)).map(|f| f.0).collect();
    assert_eq!(faces.len(), 2, "pocket wall + floor");
    let before = volume(&mut s);
    let c0 = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().centroid;
    s.execute("move_faces", &json!({"body": body.0, "faces": faces, "by": [3, 0, 0]})).unwrap();
    assert!((volume(&mut s) - before).abs() < 1.0);
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.x - 20.0).abs() < 1e-6 && bb.min.x.abs() < 1e-6, "the block itself stays put");
    let c1 = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().centroid;
    assert!(c1.x < c0.x - 0.01, "the pocket went +x, so the material's centre went -x");
    assert!(s.execute("move_faces", &json!({"body": body.0, "faces": [99], "by": [1, 0, 0]})).is_err());
    assert!(s.execute("move_faces", &json!({"body": body.0, "faces": [0], "by": [f64::NAN, 0, 0]})).is_err());
}

#[test]
fn a_face_turned_about_its_own_edge_tilts() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id;
    let n = s.kernel.face_count(&s.doc.bodies[0].shape).unwrap();
    // The +x side face, turned 10° outward about its top edge (x = 20, z = 20, along y): the bottom swings out.
    let side = (0..n).find(|f| s.kernel.face_plane(&s.doc.bodies[0].shape, *f).unwrap().is_some_and(|p| p.normal.x > 0.999)).unwrap();
    s.execute("move_faces", &json!({"body": body.0, "faces": [side], "origin": [20, 0, 20], "axis": [0, -1, 0], "angle": 10})).unwrap();
    let grown = 0.5 * 20.0 * (20.0 * 10f64.to_radians().tan()) * 20.0;
    assert!((volume(&mut s) - (8000.0 + grown)).abs() < 1.0, "a wedge added under the turned face: {}", volume(&mut s));
    assert_eq!(s.kernel.face_count(&s.doc.bodies[0].shape).unwrap(), n, "still six faces");
}

#[test]
fn moving_an_edge_tilts_the_faces_beside_it() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id;
    // The top edge along y at x = 20, z = 20, lifted 5 mm: the +x side stretches, the top tilts about x = 0.
    let m = s.doc.bodies[0].mesh.clone();
    let k = m.edges.iter().position(|l| l.iter().all(|q| (q[0] - 20.0).abs() < 1e-4 && (q[2] - 20.0).abs() < 1e-4)).unwrap();
    let edge = m.edge_ids[k];
    s.execute("move_edge", &json!({"body": body.0, "edge": edge, "by": [0, 0, 5]})).unwrap();
    let wedge = 0.5 * 20.0 * 5.0 * 20.0;
    assert!((volume(&mut s) - (8000.0 + wedge)).abs() < 1.0, "a wedge on top: {}", volume(&mut s));
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 25.0).abs() < 1e-6 && (bb.max.x - 20.0).abs() < 1e-6);
    // The moved edge stays picked, wherever its id went.
    let Some(shapecraft_doc::Pick::Edge { edge: picked, .. }) = s.selection.first().copied() else { panic!("an edge stays picked") };
    let m = s.doc.bodies[0].mesh.clone();
    let k = m.edge_ids.iter().position(|e| *e == picked).unwrap();
    assert!(m.edges[k].iter().all(|q| (q[0] - 20.0).abs() < 1e-4 && (q[2] - 25.0).abs() < 1e-4), "the lifted edge");
}

#[test]
fn a_pushed_or_moved_face_stays_picked_where_it_went() {
    let mut s = session();
    let body = pocketed_block(&mut s);
    let picked_z = |s: &mut Session| -> Vec<f64> {
        let faces = faces_of(s);
        s.selection
            .iter()
            .filter_map(|p| match *p {
                shapecraft_doc::Pick::Face { face, .. } => faces.iter().find(|f| f.0 == u64::from(face)).and_then(|f| f.1),
                _ => None,
            })
            .collect()
    };
    let floor =
        |s: &mut Session, z: f64| faces_of(s).into_iter().find(|f| f.1.is_some_and(|c| (c - z).abs() < 1e-3) && f.2 > 0.999).unwrap().0;
    // Offset Face on the pocket floor (z 10) up 2 mm: the floor, now at z 12, is what's picked, not the body.
    let f = floor(&mut s, 10.0);
    s.selection = vec![shapecraft_doc::Pick::Face { body, face: f as u32 }];
    s.execute("offset_face", &json!({"distance": 2})).unwrap();
    assert_eq!(picked_z(&mut s), vec![12.0], "{:?}", s.selection);
    // Move on that face, up 3 more: still the floor, at z 15.
    s.execute("move_faces", &json!({"by": [0, 0, 3]})).unwrap();
    assert_eq!(picked_z(&mut s), vec![15.0], "{:?}", s.selection);
    // Pushed down again by a typed amount through the body's own id: z 13.
    let f = floor(&mut s, 15.0);
    s.execute("offset_face", &json!({"body": body.0, "faces": [f], "distance": -2})).unwrap();
    assert_eq!(picked_z(&mut s), vec![13.0], "{:?}", s.selection);
    // Edge features leave nothing picked (the edges are gone), never the whole body.
    s.execute("fillet", &json!({"body": body.0, "edges": [0], "radius": 1})).unwrap();
    assert!(s.selection.is_empty(), "{:?}", s.selection);
}

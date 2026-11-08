//! Construction planes and axes, and measuring.

use serde_json::json;
use shapecraft_doc::Pick;
use shapecraft_kernel_occt::OcctKernel;

use super::{face_facing, session, volume};

#[test]
fn construction_planes() {
    let mut s = session();
    let p = s.execute("offset_plane", &json!({"from": "xy", "distance": 10})).unwrap().value["plane"].as_u64().unwrap();
    assert!((s.doc.planes[0].plane.origin.z - 10.0).abs() < 1e-12);
    assert!(matches!(s.selection[..], [Pick::Plane { .. }]));
    assert_eq!(s.doc.history.last().unwrap().label, "Plane - Offset 01");
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.planes.is_empty());
    s.execute("redo", &json!({})).unwrap();
    s.execute("sketch", &json!({"construction": p})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 4})).unwrap();
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let body = &v["bodies"][0];
    assert_eq!(body["bounds"]["min"][2].as_f64(), Some(10.0));
    let id = body["id"].as_u64().unwrap();
    let faces = body["faces"].as_array().unwrap();
    let face = |z: f64| faces.iter().find(|f| f["normal"][2].as_f64() == Some(z)).unwrap()["face"].as_u64().unwrap();
    s.execute("midplane", &json!({"a": {"body": id, "face": face(1.0)}, "b": {"body": id, "face": face(-1.0)}})).unwrap();
    assert!((s.doc.planes[1].plane.origin.z - 12.0).abs() < 1e-9, "{:?}", s.doc.planes[1].plane.origin);
    // Two faces at a corner (top, +x side): the plane through their edge halving the angle inside the part.
    let side = faces.iter().find(|f| f["normal"][0].as_f64() == Some(1.0)).unwrap()["face"].as_u64().unwrap();
    s.execute("midplane", &json!({"a": {"body": id, "face": face(1.0)}, "b": {"body": id, "face": side}})).unwrap();
    let bis = s.doc.planes.last().unwrap().plane;
    let n = bis.normal();
    assert!((n.x.abs() - n.z.abs()).abs() < 1e-9 && n.y.abs() < 1e-9 && n.x * n.z < 0.0, "{n:?}");
    assert!(n.dot(bis.origin - glam::DVec3::new(5.0, 0.0, 14.0)).abs() < 1e-6, "through the edge: {bis:?}");
    s.execute("undo", &json!({})).unwrap();
    s.execute("plane_3points", &json!({"points": [[0, 0, 0], [1, 0, 0], [0, 0, 1]]})).unwrap();
    assert!(s.doc.planes[2].plane.normal().y.abs() > 0.999);
    assert!(s.execute("plane_3points", &json!({"points": [[0, 0, 0], [1, 0, 0], [2, 0, 0]]})).is_err());
    // Through an edge: on the plane of one of its faces at 0°, turned about the edge otherwise; on a curve: square to it.
    let b = &s.doc.bodies[0];
    let k = b.mesh.edges.iter().position(|l| l.len() == 2 && (l[0][2] - l[1][2]).abs() > 1.0).unwrap();
    let edge = b.mesh.edge_ids[k];
    s.execute("plane_through_edge", &json!({"body": id, "edge": edge})).unwrap();
    let n0 = s.doc.planes[3].plane.normal();
    assert!(n0.z.abs() < 1e-9 && (n0.x.abs() > 0.999 || n0.y.abs() > 0.999), "{n0:?}");
    s.execute("plane_through_edge", &json!({"body": id, "edge": edge, "angle": 90})).unwrap();
    assert!(s.doc.planes[4].plane.normal().dot(n0).abs() < 1e-9);
    s.execute("plane_on_curve", &json!({"body": id, "edge": edge, "at": 0.5})).unwrap();
    let pl = s.doc.planes[5].plane;
    assert!(pl.normal().z.abs() > 0.999 && (pl.origin.z - 12.0).abs() < 1e-6, "{pl:?}");
    for _ in 0..3 {
        s.execute("undo", &json!({})).unwrap();
    }
    // Offset Edge on a body edge: a new sketch on the face, the line 2 mm in from the edge.
    let f0 = s.doc.bodies[0].mesh.edge_faces[k][0];
    let before = s.doc.sketches.len();
    s.execute("offset_body_edge", &json!({"body": id, "edge": edge, "face": f0, "distance": 2})).unwrap();
    assert_eq!(s.doc.sketches.len(), before + 1);
    let sk = s.doc.sketches.last().unwrap();
    let mid = sk.plane.to_world(sk.entities[0].outline()[0]);
    let e = &s.doc.bodies[0].mesh.edges[k];
    let edge_xy = glam::DVec2::new(e[0][0] as f64, e[0][1] as f64);
    assert!((glam::DVec2::new(mid.x, mid.y).distance(edge_xy) - 2.0).abs() < 1e-6, "{mid:?} {edge_xy:?}");
    assert!(mid.x.abs() <= 5.0 + 1e-9 && mid.y.abs() <= 5.0 + 1e-9, "inside the face: {mid:?}");
    s.execute("undo", &json!({})).unwrap();
    // Offset Edge on a picked face: its whole outline, 1 mm in, in a new sketch on it (10 × 4 → 8 × 2).
    s.execute("select", &json!({"picks": [{"kind": "face", "body": id, "face": f0}]})).unwrap();
    s.execute("offset_edge", &json!({"body": id, "face": f0, "distance": -1})).unwrap();
    let sk = s.doc.sketches.last().unwrap();
    assert_eq!(sk.entities.len(), 4);
    let mut lens: Vec<f64> = sk.entities.iter().map(|e| e.outline()[0].distance(*e.outline().last().unwrap())).collect();
    lens.sort_by(f64::total_cmp);
    assert!((lens[0] - 2.0).abs() < 1e-6 && (lens[3] - 8.0).abs() < 1e-6, "{lens:?}");
    s.execute("undo", &json!({})).unwrap();
    // Hide, rename, save and load.
    s.execute("rename", &json!({"item": p, "name": "Lid"})).unwrap();
    s.execute("show_hide", &json!({"items": [p]})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-planes-{}", std::process::id()));
    s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &dir).unwrap();
    assert_eq!(doc.planes.len(), 3);
    assert_eq!(doc.planes[0].name, "Lid");
    assert!(!doc.planes[0].visible);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn construction_axes_drive_revolve_and_rotate() {
    let mut s = session();
    let a = s.execute("axis_2points", &json!({"points": [[20, 0, 0], [20, 1, 0]]})).unwrap().value["axis"].as_u64().unwrap();
    assert!(matches!(s.selection[..], [Pick::Axis { .. }]));
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [4, 4]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("revolve", &json!({"sketch": sk.0, "region": 0, "axis": {"axis": a}, "angle": 360})).unwrap();
    // A square ring: area 16 swept around a circle of radius 18 (distance from the axis to the centroid).
    let expected = 16.0 * std::f64::consts::TAU * 18.0;
    assert!((volume(&mut s) - expected).abs() < 1e-3);
    let body = s.doc.bodies[0].id.0;
    s.execute("rotate", &json!({"items": [body], "about": {"axis": a}, "angle": 45})).unwrap();
    let face = s.execute("inspect", &json!({})).unwrap().value["bodies"][0]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["planar"] == json!(false))
        .unwrap()["face"]
        .as_u64()
        .unwrap();
    s.execute("axis_cylinder", &json!({"body": body, "face": face})).unwrap();
    assert_eq!(s.doc.axes.len(), 2);
    s.execute("delete", &json!({"items": [a]})).unwrap();
    assert_eq!(s.doc.axes.len(), 1);
}

#[test]
fn construction_planes_move_rotate_and_copy() {
    let mut s = session();
    s.execute("offset_plane", &json!({"from": "xy", "distance": 5})).unwrap();
    let id = s.doc.planes[0].id;
    s.execute("translate", &json!({"items": [id.0], "by": [1, 2, 3]})).unwrap();
    assert!((s.doc.planes[0].plane.origin - glam::DVec3::new(1.0, 2.0, 8.0)).length() < 1e-9);
    s.execute("rotate", &json!({"items": [id.0], "origin": [1, 2, 8], "axis": [1, 0, 0], "angle": 90})).unwrap();
    assert!(s.doc.planes[0].plane.normal().dot(glam::DVec3::Y).abs() > 1.0 - 1e-9, "turned upright");
    s.execute("translate", &json!({"items": [id.0], "by": [0, 0, 1], "copy": true})).unwrap();
    assert_eq!(s.doc.planes.len(), 2);
    assert!(matches!(s.selection.as_slice(), [shapecraft_doc::Pick::Plane { .. }]));
    assert!(s.execute("translate", &json!({"items": [99999], "by": [1, 0, 0]})).is_err());
}

#[test]
fn planes_at_points_and_an_axis_where_two_planes_meet() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    let body = s.doc.bodies[0].id.0;
    let top = face_facing(&s, glam::DVec3::Z);
    // Parallel to the top, through a point below it.
    s.execute("plane_parallel_face", &json!({"from": {"body": body, "face": top}, "point": [3, 3, 1]})).unwrap();
    let pl = s.doc.planes.last().unwrap().plane;
    assert!((pl.normal() - glam::DVec3::Z).length() < 1e-9 && (pl.origin.z - 1.0).abs() < 1e-9);
    // Square to an edge along Y, through a point beside it.
    let b = &s.doc.bodies[0];
    let k = b
        .mesh
        .edges
        .iter()
        .position(|l| {
            let (a, e) = (l[0], l[l.len() - 1]);
            (a[0] - e[0]).abs() < 1e-4 && (a[2] - e[2]).abs() < 1e-4 && (a[1] - e[1]).abs() > 10.0
        })
        .unwrap();
    let edge = b.mesh.edge_ids[k];
    s.execute("plane_perp_edge", &json!({"body": body, "edge": edge, "point": [4, 7, 2]})).unwrap();
    let pl = s.doc.planes.last().unwrap().plane;
    assert!(pl.normal().cross(glam::DVec3::Y).length() < 1e-6 && (pl.origin - glam::DVec3::new(4.0, 7.0, 2.0)).length() < 1e-9);
    // The top plane and the front plane meet along X at the origin.
    s.execute("axis_2planes", &json!({"a": "xy", "b": "xz"})).unwrap();
    let ax = s.doc.axes.last().unwrap();
    assert!(ax.dir.cross(glam::DVec3::X).length() < 1e-9 && ax.origin.y.abs() < 1e-9 && ax.origin.z.abs() < 1e-9);
    assert!(s.execute("axis_2planes", &json!({"a": "xy", "b": {"body": body, "face": top}})).is_err(), "parallel");
    // Plane size.
    let id = s.doc.planes[0].id.0;
    s.execute("plane_size", &json!({"plane": id, "size": 77})).unwrap();
    assert_eq!(s.doc.planes[0].size, 77.0);
    assert!(s.execute("plane_size", &json!({"plane": id, "size": -1})).is_err());
}

#[test]
fn project_as_sketch_from_outside_a_sketch() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 5})).unwrap();
    let body = s.doc.bodies[0].id.0;
    let (top, bottom) = (face_facing(&s, glam::DVec3::Z), face_facing(&s, -glam::DVec3::Z));
    let before = s.doc.sketches.len();
    let out =
        s.execute("project_sketch", &json!({"items": [{"body": body, "face": top}], "to": {"body": body, "face": bottom}})).unwrap().value;
    assert_eq!(s.doc.sketches.len(), before + 1);
    let sk = s.doc.sketch(shapecraft_doc::ItemId(out["sketch"].as_u64().unwrap())).unwrap();
    assert_eq!(sk.entities.len(), 4, "the top face's four edges, flattened onto the bottom");
    assert!(s.execute("project_sketch", &json!({"items": [], "to": {"body": body, "face": bottom}})).is_err());
}

#[test]
fn measure_two_bodies_reports_their_interference() {
    let mut s = session();
    for (x, sketch) in [(0, 1), (6, 3)] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [x, 0], "opposite": [x + 10, 10]})).unwrap();
        s.execute("extrude", &json!({"sketch": sketch, "region": 0, "distance": 10, "result": "new"})).unwrap();
    }
    let (a, b) = (s.doc.bodies[0].id.0, s.doc.bodies[1].id.0);
    let picks = json!([{"kind": "body", "body": a}, {"kind": "body", "body": b}]);
    let out = s.execute("measure", &json!({ "picks": picks })).unwrap().value;
    let v = out["interference"][0]["volume_mm3"].as_f64().unwrap();
    assert!((v - 400.0).abs() < 1e-6, "4 × 10 × 10 overlap: {v}");
}

#[test]
fn measure_two_faces_gives_min_max_and_angle() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    let body = s.doc.bodies[0].id.0;
    let (top, bottom, side) = (face_facing(&s, glam::DVec3::Z), face_facing(&s, -glam::DVec3::Z), face_facing(&s, glam::DVec3::X));
    let out = s
        .execute(
            "measure",
            &json!({"picks": [{"kind": "face", "body": body, "face": top}, {"kind": "face", "body": body, "face": bottom}]}),
        )
        .unwrap()
        .value;
    assert!((out["distance"]["mm"].as_f64().unwrap() - 5.0).abs() < 1e-6);
    let far = out["max_distance"]["mm"].as_f64().unwrap();
    assert!((far - (100.0f64 + 400.0 + 25.0).sqrt()).abs() < 1e-3, "corner to corner: {far}");
    assert!((out["angle_deg"].as_f64().unwrap() - 180.0).abs() < 1e-6);
    let out = s
        .execute("measure", &json!({"picks": [{"kind": "face", "body": body, "face": top}, {"kind": "face", "body": body, "face": side}]}))
        .unwrap()
        .value;
    assert!((out["angle_deg"].as_f64().unwrap() - 90.0).abs() < 1e-6);
}

#[test]
fn measure_central_and_parallel_distances() {
    let mut s = session();
    let body = super::holed_block(&mut s);
    let v = s.execute("inspect", &json!({})).unwrap().value;
    let faces = v["bodies"][0]["faces"].as_array().unwrap().clone();
    let by_normal = |n: [f64; 3]| {
        faces.iter().find(|f| f["normal"].as_array().is_some_and(|a| (0..3).all(|i| a[i].as_f64() == Some(n[i])))).unwrap()["face"]
            .as_u64()
            .unwrap()
    };
    let (left, right) = (by_normal([-1.0, 0.0, 0.0]), by_normal([1.0, 0.0, 0.0]));
    let pick = |f: u64| json!({"kind": "face", "body": body.0, "face": f});
    // The hole's wall: the biggest round face (the others are the fillet round its rim).
    let area = |s: &mut crate::Session, f: u64| {
        s.execute("measure", &json!({"picks": [pick(f)]})).unwrap().value["items"][0]["area_mm2"].as_f64().unwrap()
    };
    let round: Vec<u64> = faces.iter().filter(|f| f["planar"] == json!(false)).map(|f| f["face"].as_u64().unwrap()).collect();
    let hole = *round.iter().max_by(|a, b| area(&mut s, **a).total_cmp(&area(&mut s, **b))).unwrap();
    let out = s.execute("measure", &json!({"picks": [pick(hole), pick(left)]})).unwrap().value;
    assert!((out["central_distance"]["mm"].as_f64().unwrap() - 10.0).abs() < 1e-6, "{out}");
    assert!((out["parallel_distance"]["mm"].as_f64().unwrap() - 10.0).abs() < 1e-6, "{out}");
    let out = s.execute("measure", &json!({"picks": [pick(left), pick(right)]})).unwrap().value;
    assert!((out["parallel_distance"]["mm"].as_f64().unwrap() - 20.0).abs() < 1e-6, "{out}");
    assert!(out.get("central_distance").is_none(), "two flat faces have no center: {out}");
}

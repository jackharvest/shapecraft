//! Engine tests, run through `Session::execute` on the OCCT kernel. Shared helpers are here; the tests are grouped
//! by area in the submodules.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use serde_json::json;
use shapecraft_kernel_occt::OcctKernel;

use crate::Session;

mod construction;
mod drawing;
mod environment;
mod faces;
mod features;
mod files;
mod history;
mod meshes;
mod project;
mod shapr;
mod sketch;
mod transform;

fn session() -> Session {
    Session::new(Arc::new(OcctKernel::new()), "Test")
}

fn volume(s: &mut Session) -> f64 {
    let v = s.execute("inspect", &json!({})).unwrap().value;
    v["bodies"].as_array().unwrap().iter().map(|b| b["volume_mm3"].as_f64().unwrap()).sum()
}

/// Edges of body `b` whose points all satisfy `f`.
fn edges_where(s: &Session, b: usize, f: impl Fn([f32; 3]) -> bool) -> Vec<u32> {
    let m = &s.doc.bodies[b].mesh;
    m.edges.iter().zip(&m.edge_ids).filter(|(l, _)| l.iter().all(|p| f(*p))).map(|(_, e)| *e).collect()
}

/// The user's test part (observed 362–371): a block with a hole whose rim is filleted and a chamfered top edge.
fn holed_block(s: &mut Session) -> shapecraft_doc::ItemId {
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id;
    let top = top_face(s);
    s.execute("sketch", &json!({"body": body.0, "face": top})).unwrap();
    // A face's sketch is centered on the face.
    s.execute("circle", &json!({"center": [0, 0], "radius": 4})).unwrap();
    let sk = s.active_sketch.unwrap();
    let inner = s.doc.sketch(sk).unwrap().regions().iter().find(|r| r.area < 60.0).map(|r| r.index).unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": inner, "distance": -20})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1, "the hole cuts the block");
    let rim = edges_where(s, 0, |p| (p[2] - 20.0).abs() < 1e-3 && ((p[0] - 10.0).powi(2) + (p[1] - 10.0).powi(2) - 16.0).abs() < 1e-2);
    assert!(!rim.is_empty());
    s.execute("fillet", &json!({"body": body.0, "edges": rim, "radius": 1.5})).unwrap();
    let front = edges_where(s, 0, |p| p[1].abs() < 1e-3 && (p[2] - 20.0).abs() < 1e-3);
    assert_eq!(front.len(), 1);
    s.execute("chamfer", &json!({"body": body.0, "edges": front, "distance": 2})).unwrap();
    body
}

/// The first body's face pointing straight up.
fn top_face(s: &mut Session) -> u64 {
    let v = s.execute("inspect", &json!({})).unwrap().value;
    v["bodies"][0]["faces"].as_array().unwrap().iter().find(|f| f["normal"][2].as_f64().is_some_and(|z| z > 0.999)).unwrap()["face"]
        .as_u64()
        .unwrap()
}

/// The first face of the first body whose triangles face `dir`.
fn face_facing(s: &Session, dir: glam::DVec3) -> u32 {
    let m = &s.doc.bodies[0].mesh;
    let p = |i: u32| {
        let v = m.positions[i as usize];
        glam::DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64)
    };
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .zip(&m.triangle_faces)
        .find(|(t, _)| (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).normalize().dot(dir) > 0.99)
        .map(|(_, f)| *f)
        .unwrap()
}

//! Mesh bodies (STL, OBJ, .shapr mesh bodies), kept as triangles:
//! no exact faces, picked whole, moved and combined with booleans while staying meshes.

use serde_json::{Value, json};

use super::session;
use crate::Session;

/// A 10 mm cube STL (12 triangles, binary) at `at`.
fn cube_stl(dir: &std::path::Path, name: &str, at: [f32; 3]) -> String {
    let v: Vec<[f32; 3]> = (0..8)
        .map(|i| [at[0] + 10.0 * (i & 1) as f32, at[1] + 10.0 * ((i >> 1) & 1) as f32, at[2] + 10.0 * ((i >> 2) & 1) as f32])
        .collect();
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut bytes = vec![0u8; 80];
    bytes.extend_from_slice(&12u32.to_le_bytes());
    for q in quads {
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            bytes.extend_from_slice(&[0u8; 12]);
            for i in t {
                for c in v[i] {
                    bytes.extend_from_slice(&c.to_le_bytes());
                }
            }
            bytes.extend_from_slice(&[0, 0]);
        }
    }
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("shapecraft-mesh-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn info(s: &mut Session, id: u64) -> Value {
    s.execute("inspect", &json!({"body": id, "faces": false})).unwrap().value["bodies"][0].clone()
}

fn vol(s: &mut Session, id: u64) -> f64 {
    info(s, id)["volume_mm3"].as_f64().unwrap_or(f64::NAN)
}

fn import(s: &mut Session, path: &str) -> u64 {
    s.execute("import_file", &json!({"path": path})).unwrap().value["bodies"][0].as_u64().unwrap()
}

#[test]
fn an_stl_comes_in_as_a_mesh_body_in_a_folder() {
    let d = dir("import");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "cube.stl", [0.0; 3]));
    let b = s.doc.bodies.last().unwrap();
    assert!(b.is_mesh() && b.name == "Mesh body 01");
    assert!(b.mesh.edges.is_empty(), "no edge lines");
    assert_eq!(s.doc.folders.last().unwrap().name, "cube.stl");
    let i = info(&mut s, id);
    assert_eq!(i["triangles"], 12);
    assert!(i["closed"].as_bool().unwrap());
    assert!((i["volume_mm3"].as_f64().unwrap() - 1000.0).abs() < 1e-6);
    assert_eq!(s.doc.history.last().unwrap().label, "Import \"cube.stl\"");
}

#[test]
fn moves_turns_mirrors_and_patterns_keep_it_a_mesh() {
    let d = dir("move");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "c.stl", [0.0; 3]));
    s.execute("translate", &json!({"items": [id], "by": [5, 0, 0]})).unwrap();
    assert!((info(&mut s, id)["bounds"]["min"][0].as_f64().unwrap() - 5.0).abs() < 1e-9);
    s.execute("rotate", &json!({"items": [id], "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 90})).unwrap();
    s.execute("scale", &json!({"items": [id], "factor": 2})).unwrap();
    assert!((vol(&mut s, id) - 8000.0).abs() < 1e-6);
    s.execute("mirror", &json!({"items": [id], "origin": [0, 0, 0], "normal": [1, 0, 0], "copy": true})).unwrap();
    s.execute("pattern", &json!({"items": [id], "direction": [0, 0, 1], "spacing": 30, "count": 3})).unwrap();
    // The original, its mirror and two pattern copies: all mesh bodies of the same volume.
    assert_eq!(s.doc.bodies.len(), 4);
    let ids: Vec<u64> = s.doc.bodies.iter().map(|b| b.id.0).collect();
    for b in ids {
        assert!(s.doc.body(shapecraft_doc::ItemId(b)).unwrap().is_mesh());
        assert!((vol(&mut s, b) - 8000.0).abs() < 1e-6, "body {b}");
        assert!(info(&mut s, b)["closed"].as_bool().unwrap(), "a mirror keeps it closed and facing out");
    }
}

#[test]
fn booleans_with_meshes_stay_meshes() {
    let d = dir("bool");
    let mut s = session();
    let a = import(&mut s, &cube_stl(&d, "a.stl", [0.0; 3]));
    let b = import(&mut s, &cube_stl(&d, "b.stl", [5.0, 0.0, 0.0]));
    s.execute("subtract", &json!({"target": a, "tools": [b]})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert!(s.doc.bodies[0].is_mesh());
    let v = vol(&mut s, a);
    assert!((v - 500.0).abs() < 1e-3, "{v} {}", info(&mut s, a));
    // A mesh and an exact body: the result is a mesh body too.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [5, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    let block = s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 4, "result": "new"})).unwrap().value["body"]
        .as_u64()
        .unwrap();
    s.execute("union", &json!({"target": a, "tools": [block]})).unwrap();
    assert!(s.doc.body(shapecraft_doc::ItemId(a)).unwrap().is_mesh());
    assert!((vol(&mut s, a) - 500.0).abs() < 1e-3, "the block lies inside the half that's left");
}

#[test]
fn an_open_mesh_refuses_booleans_but_splits() {
    let d = dir("open");
    // The cube without its top: an open (sheet) mesh.
    let obj = "v 0 0 0\nv 10 0 0\nv 10 10 0\nv 0 10 0\nv 0 0 10\nv 10 0 10\nv 10 10 10\nv 0 10 10\n\
f 1 3 2\nf 1 4 3\nf 1 2 6\nf 1 6 5\nf 2 3 7\nf 2 7 6\nf 3 4 8\nf 3 8 7\nf 4 1 5\nf 4 5 8\n";
    std::fs::write(d.join("cup.obj"), obj).unwrap();
    let mut s = session();
    let cup = import(&mut s, &d.join("cup.obj").to_string_lossy());
    assert!(!info(&mut s, cup)["closed"].as_bool().unwrap());
    let other = import(&mut s, &cube_stl(&d, "c.stl", [20.0, 0.0, 0.0]));
    assert!(s.execute("union", &json!({"target": cup, "tools": [other]})).is_err());
    s.execute("split_body", &json!({"items": [cup], "origin": [0, 0, 5], "normal": [0, 0, 1]})).unwrap();
    assert_eq!(s.doc.bodies.iter().filter(|b| b.is_mesh()).count(), 3);
}

#[test]
fn tools_for_exact_faces_refuse_a_mesh() {
    let d = dir("refuse");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "c.stl", [0.0; 3]));
    assert!(s.execute("fillet", &json!({"body": id, "radius": 1})).is_err());
    assert!(s.execute("shell", &json!({"body": id, "thickness": 1})).is_err());
    assert!(s.doc.body(shapecraft_doc::ItemId(id)).unwrap().is_mesh());
}

#[test]
fn sketches_and_features_next_to_a_mesh() {
    let d = dir("sketch");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "c.stl", [0.0; 3]));
    // Sketch on Face on a mesh: the plane of the triangle under the pointer (its point and normal).
    let f = s.doc.body(shapecraft_doc::ItemId(id)).unwrap().facets.clone().unwrap();
    let (_, at, normal) = f.hit(glam::DVec3::new(5.0, 5.0, 50.0), glam::DVec3::NEG_Z).unwrap();
    s.execute("sketch", &json!({"origin": at.to_array(), "normal": normal.to_array()})).unwrap();
    s.execute("rectangle", &json!({"corner": [-2, -2], "opposite": [2, 2]})).unwrap();
    let sk = s.active_sketch.unwrap();
    // An extrude into it makes its own body and leaves the mesh alone (the usual way: extrude, then
    // subtracts with the boolean tool).
    let out = s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": -4})).unwrap().value;
    assert_ne!(out["body"].as_u64(), Some(id));
    assert!((vol(&mut s, id) - 1000.0).abs() < 1e-6);
    let pin = out["body"].as_u64().unwrap();
    s.execute("subtract", &json!({"target": id, "tools": [pin]})).unwrap();
    assert!((vol(&mut s, id) - (1000.0 - 64.0)).abs() < 1e-3);
    assert!(s.doc.body(shapecraft_doc::ItemId(id)).unwrap().is_mesh());
}

#[test]
fn measuring_a_mesh_body() {
    let d = dir("measure");
    let mut s = session();
    let a = import(&mut s, &cube_stl(&d, "a.stl", [0.0; 3]));
    let b = import(&mut s, &cube_stl(&d, "b.stl", [30.0, 0.0, 0.0]));
    s.selection = vec![
        shapecraft_doc::Pick::Body { body: shapecraft_doc::ItemId(a) },
        shapecraft_doc::Pick::Body { body: shapecraft_doc::ItemId(b) },
    ];
    let m = s.execute("measure", &json!({})).unwrap().value;
    assert!(m.to_string().contains("20"), "the 20 mm gap: {m}");
}

#[test]
fn a_project_with_meshes_saves_small_and_reads_back() {
    let d = dir("save");
    let mut s = session();
    let a = import(&mut s, &cube_stl(&d, "a.stl", [0.0; 3]));
    s.execute("translate", &json!({"items": [a], "by": [1, 2, 3]})).unwrap();
    let proj = d.join("proj");
    s.execute("save", &json!({"path": proj})).unwrap();
    assert!(!proj.join("bodies.step").exists() && !proj.join("bodies-next.step").exists(), "no exact bodies, no STEP");
    assert_eq!(std::fs::read_dir(proj.join("meshes")).unwrap().count(), 1);
    // Unchanged, the mesh file isn't written again; moved, the old file goes.
    s.execute("save", &json!({"path": proj})).unwrap();
    s.execute("translate", &json!({"items": [a], "by": [1, 0, 0]})).unwrap();
    s.execute("save", &json!({"path": proj})).unwrap();
    assert_eq!(std::fs::read_dir(proj.join("meshes")).unwrap().count(), 1);
    let doc = shapecraft_doc::project::load(s.kernel.as_ref(), &proj).unwrap();
    let back = doc.body(shapecraft_doc::ItemId(a)).unwrap();
    assert!(back.is_mesh());
    let (lo, _) = back.facets.as_ref().unwrap().bounds().unwrap();
    assert!((lo - glam::DVec3::new(2.0, 2.0, 3.0)).length() < 1e-12, "exact, no rounding: {lo}");
    // Mixed with an exact body: the STEP holds only that one, in order.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [50, 0], "opposite": [60, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 5, "result": "new"})).unwrap();
    s.execute("save", &json!({"path": proj})).unwrap();
    let doc = shapecraft_doc::project::load(s.kernel.as_ref(), &proj).unwrap();
    assert_eq!(doc.bodies.len(), 2);
    assert!(doc.bodies[0].is_mesh() && !doc.bodies[1].is_mesh());
}

#[test]
fn exports_include_mesh_bodies() {
    let d = dir("export");
    let mut s = session();
    import(&mut s, &cube_stl(&d, "a.stl", [0.0; 3]));
    let p = |n: &str| d.join(n).to_string_lossy().into_owned();
    s.execute("export_step", &json!({"path": p("out.step")})).unwrap();
    s.execute("export_stl", &json!({"path": p("out.stl")})).unwrap();
    let back = s.execute("import_file", &json!({"path": p("out.step")})).unwrap().value;
    let id = back["bodies"][0].as_u64().unwrap();
    assert!((vol(&mut s, id) - 1000.0).abs() < 1e-3);
    assert_eq!(std::fs::metadata(d.join("out.stl")).unwrap().len(), 84 + 12 * 50);
}

#[test]
fn a_pasted_mesh_body_survives_a_history_rebuild() {
    let d = dir("paste");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "c.stl", [0.0; 3]));
    // Paste: an unlinked copy in place.
    s.execute("translate", &json!({"items": [id], "by": [0, 0, 0], "copy": true, "linked": false})).unwrap();
    s.execute("translate", &json!({"items": [id], "by": [20, 0, 0]})).unwrap();
    assert_eq!(s.doc.bodies.iter().filter(|b| b.is_mesh()).count(), 2);
    // Rebuild from the History (editing a step replays every step): both mesh bodies come back where they were.
    assert_eq!(s.doc.history.len(), 3, "import, unlinked copy, move");
    s.execute("edit_step", &json!({"step": 0, "params": {}})).unwrap();
    assert_eq!(s.doc.bodies.iter().filter(|b| b.is_mesh()).count(), 2);
    let i = info(&mut s, id);
    assert!((i["bounds"]["min"][0].as_f64().unwrap() - 20.0).abs() < 1e-9, "{i}");
}

#[test]
fn moving_a_mesh_body_is_a_step_and_undoes() {
    let d = dir("undo");
    let mut s = session();
    let id = import(&mut s, &cube_stl(&d, "c.stl", [0.0; 3]));
    s.execute("translate", &json!({"items": [id], "by": [7, 0, 0]})).unwrap();
    assert_eq!(s.doc.history.last().unwrap().label, "Movement/Rotation 01");
    s.execute("undo", &json!({})).unwrap();
    assert!(info(&mut s, id)["bounds"]["min"][0].as_f64().unwrap().abs() < 1e-9);
}

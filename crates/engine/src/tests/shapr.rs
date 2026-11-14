//! The Shapr3D round trip: bodies, sketches, folders and looks out and back, and Shapr3D steps coming back as
//! ours.

use serde_json::json;

use super::{session, volume};

#[test]
fn imported_nested_folders_remain_a_tree_when_exported_again() {
    let dir = std::env::temp_dir().join(format!("shapecraft-nested-shapr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("nested.shapr");
    let mut s = session();
    let shape = s.kernel.make_box(glam::DVec3::ZERO, glam::DVec3::ONE).unwrap();
    let brep = s.kernel.to_brep(&shape).unwrap().remove(0);
    let xt = shapecraft_xt::write_body(&brep, shapecraft_xt::Format::Binary).unwrap();
    let mut w = shapecraft_shapr::Writer::fresh().unwrap();
    for name in ["Left", "Right"] {
        w.import_body(name, &xt, &["Assembly".into(), name.into()], Default::default()).unwrap();
    }
    std::fs::write(&path, w.finish().unwrap()).unwrap();
    s.execute("import_file", &json!({"path":path})).unwrap();
    assert_eq!(s.doc.bodies.len(), 2);
    assert_eq!(s.doc.folders.len(), 3);
    let root = s.doc.folders.iter().find(|f| f.name == "Assembly").unwrap();
    assert_eq!(root.items.len(), 2);
    for child in ["Left", "Right"] {
        let folder = s.doc.folders.iter().find(|f| f.name == child).unwrap();
        assert!(root.items.contains(&folder.id));
        assert_eq!(folder.items.len(), 1);
        assert_eq!(s.doc.body(folder.items[0]).unwrap().name, child);
    }
    // Force a fresh export: passing the original workspace through would hide a broken folder traversal.
    s.doc.shapr = None;
    let out = dir.join("out.shapr");
    s.execute("export_shapr", &json!({"path":out})).unwrap();
    let back = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
    for b in back.bodies {
        assert_eq!(b.folder, vec!["Assembly".to_string(), b.name.unwrap()]);
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// A project written as a Shapr3D file reads back with the same solids and remembers the file; exporting the
/// linked project again keeps unchanged bodies, and a changed body becomes a Delete step plus a new Import.
#[test]
fn shapr_export_import_and_linked_round_trip() {
    let dir = std::env::temp_dir().join(format!("shapecraft-shapr-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("a.shapr");
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sk.0, "region": 0}]})).unwrap();
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    let body = s.doc.bodies[0].id;
    s.execute("fillet", &json!({"body": body.0, "radius": 1})).unwrap();
    let v0 = volume(&mut s);
    let out = s.execute("export_shapr", &json!({"path": first})).unwrap().value;
    assert_eq!(out["imported"], 1);
    assert_eq!(out["linked"], false);

    // Without a cached partition the bodies come from the imported prototypes; nothing is linked to a step
    // until Shapr3D has replayed the file, so this checks geometry only.
    let mut t = session();
    let got = t.execute("import_file", &json!({"path": first})).unwrap().value;
    assert_eq!(got["bodies"].as_array().unwrap().len(), 1);
    assert!((volume(&mut t) - v0).abs() < 1e-3 * v0);
    assert!(t.doc.shapr.is_some());

    // Linked export: pretend the body came from that file's import step; untouched it is kept, changed it is replaced.
    let read = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&first).unwrap()).unwrap();
    let step = read.steps.iter().find(|s| s.op == "MaterializeImportedBodies").unwrap().node;
    let id = t.doc.bodies[0].id;
    let shape = t.doc.bodies[0].shape.clone();
    let m = t.kernel.mass_props(&shape).unwrap();
    let b = t.kernel.bounds(&shape).unwrap();
    let fp: Vec<f64> = [m.volume, m.area, t.kernel.face_count(&shape).unwrap() as f64]
        .into_iter()
        .chain(m.centroid.to_array())
        .chain(b.min.to_array())
        .chain(b.max.to_array())
        .collect();
    let name = t.doc.bodies[0].name.clone();
    t.doc.shapr.as_mut().unwrap().bodies = vec![shapecraft_doc::ShaprBody {
        body: id,
        step,
        item_name_id: None,
        name,
        fingerprint: fp,
        mesh_fingerprint: false,
        color: None,
        hidden: false,
        extrude: None,
    }];
    let kept = t.execute("export_shapr", &json!({"path": dir.join("b.shapr")})).unwrap().value;
    assert_eq!((kept["kept"].as_u64(), kept["imported"].as_u64(), kept["deleted"].as_u64()), (Some(1), Some(0), Some(0)));
    // Moved only: the import's placement moves it, nothing is replaced.
    t.execute("translate", &json!({"items": [id.0], "by": [5, 0, 0]})).unwrap();
    let moved = t.execute("export_shapr", &json!({"path": dir.join("m.shapr")})).unwrap().value;
    assert_eq!((moved["moved"].as_u64(), moved["deleted"].as_u64(), moved["imported"].as_u64()), (Some(1), Some(0), Some(0)));
    let file = shapecraft_shapr::ShaprFile::read(&dir.join("m.shapr")).unwrap();
    let db = file.db().unwrap();
    let tr: Vec<u8> = db.conn.query_row("SELECT Transform FROM HistoryImportedBodies", [], |r| r.get(0)).unwrap();
    let m: Vec<f64> = shapecraft_shapr::unpack(&tr).unwrap().as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((m[12] - 0.005).abs() < 1e-12 && m[13] == 0.0 && m[0] == 1.0, "{m:?}");
    // Read back before Shapr3D replays it (no cached partitions), the body sits where the placement puts it.
    let mut u = session();
    u.execute("import_file", &json!({"path": dir.join("m.shapr")})).unwrap();
    let b = u.kernel.bounds(&u.doc.bodies[0].shape.clone()).unwrap();
    assert!((b.min.x - 5.0).abs() < 1e-6, "{b:?}");
    // Turned: replaced by a Delete and an Import.
    t.execute("rotate", &json!({"items": [id.0], "origin": [0, 0, 0], "axis": [0, 0, 1], "angle": 30})).unwrap();
    let changed = t.execute("export_shapr", &json!({"path": dir.join("c.shapr")})).unwrap().value;
    assert_eq!((changed["deleted"].as_u64(), changed["imported"].as_u64()), (Some(1), Some(1)));
    let file = shapecraft_shapr::ShaprFile::read(&dir.join("c.shapr")).unwrap();
    let project = shapecraft_shapr::Project::read(&file).unwrap();
    let ops: Vec<&str> = project.steps.iter().map(|s| s.op.as_str()).filter(|op| *op != "MaterializeSketchPlane").collect();
    assert_eq!(ops, ["MaterializeImportedBodies", "Delete", "MaterializeImportedBodies"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shapr_sketches_go_out_and_come_back() {
    let dir = std::env::temp_dir().join(format!("shapecraft-shapr-sketch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [3, 4], "radius": 5})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    let out = s.execute("export_shapr", &json!({"path": dir.join("a.shapr")})).unwrap().value;
    assert_eq!(out["sketches_added"], 1);

    let mut t = session();
    let got = t.execute("import_file", &json!({"path": dir.join("a.shapr")})).unwrap().value;
    assert_eq!(got["sketches"].as_array().unwrap().len(), 1);
    let sk = t.doc.sketches.last().unwrap().clone();
    assert_eq!(sk.name, "Sketch 01");
    match sk.entities.as_slice() {
        [shapecraft_doc::SketchEntity::Circle { center, radius }] => {
            assert!((center.x - 3.0).abs() < 1e-9 && (center.y - 4.0).abs() < 1e-9 && (radius - 5.0).abs() < 1e-9)
        }
        e => panic!("{e:?}"),
    }
    // Linked: resizing the circle here resizes it in the file.
    let i = t.doc.sketches.len() - 1;
    t.doc.sketches[i].entities[0] = shapecraft_doc::SketchEntity::Circle { center: glam::DVec2::new(3.0, 4.0), radius: 7.0 };
    let back = t.execute("export_shapr", &json!({"path": dir.join("b.shapr")})).unwrap().value;
    assert_eq!((back["sketches_updated"].as_u64(), back["sketches_added"].as_u64()), (Some(1), Some(0)));
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&dir.join("b.shapr")).unwrap()).unwrap();
    assert!(matches!(p.sketches[0].curves[0], shapecraft_shapr::SketchCurve::Circle { radius, .. } if (radius - 7.0).abs() < 1e-9));
    assert_eq!(p.steps.iter().filter(|s| s.op == "MaterializeSketchPlane").count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shapr_sketch_lines_and_arcs_go_out_and_come_back() {
    use shapecraft_doc::SketchEntity as E;
    let path = std::env::temp_dir().join(format!("shapecraft-shapr-lines-{}.shapr", std::process::id()));
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 5]})).unwrap();
    s.execute("line", &json!({"a": [20, 0], "b": [30, 0]})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    let i = s.doc.sketches.len() - 1;
    // Clockwise through the top: written counter-clockwise from the other end.
    let v = glam::DVec2::new;
    s.doc.sketches[i].entities.push(E::Arc { a: v(-5.0, 0.0), mid: v(0.0, 5.0), b: v(5.0, 0.0) });
    s.execute("export_shapr", &json!({"path": path})).unwrap();

    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&path).unwrap()).unwrap();
    let lines = p.sketches[0].curves.iter().filter(|c| matches!(c, shapecraft_shapr::SketchCurve::Line { .. })).count();
    assert!(lines >= 5, "{:?}", p.sketches[0].curves);
    match p.sketches[0].curves.last().unwrap() {
        shapecraft_shapr::SketchCurve::Arc { center, start, end, .. } => {
            assert!(center[0].abs() < 1e-9 && center[1].abs() < 1e-9);
            assert!((start[0] - 5.0).abs() < 1e-9 && (end[0] + 5.0).abs() < 1e-9, "{start:?} {end:?}");
        }
        c => panic!("{c:?}"),
    }
    let mut t = session();
    t.execute("import_file", &json!({"path": path})).unwrap();
    let sk = t.doc.sketches.last().unwrap();
    assert!((sk.plane.y - glam::DVec3::Z).length() < 1e-9, "XZ plane comes back as XZ: {:?}", sk.plane);
    match sk.entities.last().unwrap() {
        E::Arc { a, mid, b } => {
            assert!((a.x - 5.0).abs() < 1e-9 && (b.x + 5.0).abs() < 1e-9 && (mid.y - 5.0).abs() < 1e-9, "{a} {mid} {b}")
        }
        e => panic!("{e:?}"),
    }
    assert!(sk.entities.iter().filter(|e| matches!(e, E::Line { .. })).count() >= 5);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn shapr_hidden_folders_go_out() {
    let path = std::env::temp_dir().join(format!("shapecraft-shapr-hidden-{}.shapr", std::process::id()));
    let mut s = session();
    for x in [0, 30] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [x, 0], "opposite": [x + 10, 10]})).unwrap();
        let sk = s.active_sketch.unwrap();
        s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sk.0, "region": 0}]})).unwrap();
        s.execute("extrude", &json!({"distance": 5})).unwrap();
    }
    let spare = s.doc.bodies[1].id;
    s.execute("new_folder", &json!({"name": "Spare", "items": [spare.0]})).unwrap();
    s.execute("show_hide", &json!({"items": [spare.0], "visible": false})).unwrap();
    s.execute("export_shapr", &json!({"path": path})).unwrap();
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&path).unwrap()).unwrap();
    let hidden: Vec<(&str, bool)> = p.items.iter().map(|i| (i.folder.first().map_or("", String::as_str), i.hidden)).collect();
    assert!(hidden.contains(&("Spare", true)) && hidden.contains(&("", false)), "{hidden:?}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn shapr_colours_and_hidden_bodies_go_out_and_come_back() {
    let dir = std::env::temp_dir().join(format!("shapecraft-shapr-look-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    for x in [0, 30] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [x, 0], "opposite": [x + 10, 10]})).unwrap();
        let sk = s.active_sketch.unwrap();
        s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sk.0, "region": 0}]})).unwrap();
        s.execute("extrude", &json!({"distance": 5})).unwrap();
    }
    let (a, b) = (s.doc.bodies[0].id, s.doc.bodies[1].id);
    s.execute(
        "set_appearance",
        &json!({"items": [a.0], "appearance": {"name": "Red", "color": [0.8706, 0.2157, 0.2157], "metal": 0, "gloss": 0.5}}),
    )
    .unwrap();
    s.execute("show_hide", &json!({"items": [b.0], "visible": false})).unwrap();
    s.execute("export_shapr", &json!({"path": dir.join("first.shapr")})).unwrap();
    let looks = |name: &str| -> Vec<(Option<u32>, bool)> {
        let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&dir.join(name)).unwrap()).unwrap();
        p.items.iter().map(|i| (i.color.map(shapecraft_shapr::write::rgba), i.hidden)).collect()
    };
    assert_eq!(looks("first.shapr"), vec![(Some(0xDE37_37FF), false), (None, true)]);
    // Back in: the colour and the hidden body come over; recolour one, show the other, and only the looks change.
    let mut t = session();
    t.execute("import_file", &json!({"path": dir.join("first.shapr")})).unwrap();
    assert_eq!(
        t.doc.bodies.iter().map(|b| (b.appearance.as_ref().map(|a| shapecraft_shapr::write::rgba(a.color)), b.visible)).collect::<Vec<_>>(),
        vec![(Some(0xDE37_37FF), true), (None, false)]
    );
    let (a, b) = (t.doc.bodies[0].id, t.doc.bodies[1].id);
    t.execute(
        "set_appearance",
        &json!({"items": [a.0], "appearance": {"name": "Blue", "color": [0.0, 0.0, 1.0], "metal": 0, "gloss": 0.5}}),
    )
    .unwrap();
    t.execute("show_hide", &json!({"items": [b.0], "visible": true})).unwrap();
    let got = t.execute("export_shapr", &json!({"path": dir.join("second.shapr")})).unwrap().value;
    assert_eq!((got["imported"].as_u64(), got["deleted"].as_u64(), got["restyled"].as_u64()), (Some(0), Some(0), Some(2)), "{got}");
    assert_eq!(looks("second.shapr"), vec![(Some(0x0000_FFFF), false), (None, false)]);
    // Opened again and left alone, nothing is restyled.
    let mut u = session();
    u.execute("import_file", &json!({"path": dir.join("second.shapr")})).unwrap();
    let got = u.execute("export_shapr", &json!({"path": dir.join("third.shapr")})).unwrap().value;
    assert_eq!(got["restyled"].as_u64(), Some(0), "{got}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shapr3d_extrudes_come_back_as_our_steps_and_edits_go_back_into_them() {
    // A project saved by Shapr3D (kept out of the repository; the check passes quietly without it).
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files/n4-extrudes");
    if !sample.is_file() {
        return;
    }
    let out = std::env::temp_dir().join(format!("shapecraft-shapr-extrudes-{}.shapr", std::process::id()));
    let mut s = session();
    let got = s.execute("import_file", &json!({"path": sample})).unwrap().value;
    assert_eq!(got["extrudes"].as_u64(), Some(2), "{got}");
    let labels: Vec<&str> = s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.as_str()).collect();
    assert_eq!(labels[1..], ["Extrude 01", "Extrude 02"]);
    assert_eq!(s.doc.bodies.len(), 4);
    // Editing a step rebuilds everything: no body twice, and the links to Shapr3D's extrudes stay.
    let at = s.doc.history.iter().position(|h| h.label == "Extrude 01").unwrap();
    s.execute("edit_step", &json!({"step": at, "params": {"distance": 12}})).unwrap();
    assert_eq!(s.doc.bodies.len(), 4);
    assert_eq!(s.doc.shapr.as_ref().unwrap().bodies.iter().filter(|b| b.extrude.is_some()).count(), 2);
    let got = s.execute("export_shapr", &json!({"path": out})).unwrap().value;
    assert_eq!((got["deleted"].as_u64(), got["imported"].as_u64()), (Some(0), Some(0)), "{got}");
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
    assert!((p.extrudes[0].distance - 12.0).abs() < 1e-9 && (p.extrudes[1].distance - 6.0).abs() < 1e-9);
    assert!(!p.steps.iter().any(|st| st.op == "Delete"));
    // Opened here again before Shapr3D has rebuilt it, the extrudes are redone from their profiles.
    let mut u = session();
    let got = u.execute("import_file", &json!({"path": out})).unwrap().value;
    assert_eq!(got["extrudes"].as_u64(), Some(2), "{got}");
    let body = u.doc.bodies.iter().find(|b| b.name == "Body 03").unwrap().shape.clone();
    assert!((u.kernel.mass_props(&body).unwrap().volume - 20.0 * 10.0 * 12.0).abs() < 1e-6);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_shapr3d_fillet_on_an_extrude_comes_back_as_our_fillet_step_and_its_radius_goes_back() {
    // A project saved by Shapr3D (kept out of the repository): the drafted half-disc rounded 1 mm on every edge.
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files/n6-curved-fillet");
    if !sample.is_file() {
        return;
    }
    let out = std::env::temp_dir().join(format!("shapecraft-shapr-fillet-{}.shapr", std::process::id()));
    let mut s = session();
    let got = s.execute("import_file", &json!({"path": sample})).unwrap().value;
    assert_eq!(got["extrudes"].as_u64(), Some(2), "{got}");
    let labels: Vec<&str> = s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.as_str()).collect();
    assert_eq!(labels[1..], ["Extrude 01", "Extrude 02", "Fillet 01"]);
    let body = s.doc.bodies.iter().find(|b| b.name == "Body 04").unwrap().shape.clone();
    assert!((s.kernel.mass_props(&body).unwrap().volume - 378.82).abs() < 0.01);
    let at = s.doc.history.iter().position(|h| h.label == "Fillet 01").unwrap();
    s.execute("edit_step", &json!({"step": at, "params": {"radius": 0.5}})).unwrap();
    let got = s.execute("export_shapr", &json!({"path": out})).unwrap().value;
    assert_eq!((got["deleted"].as_u64(), got["imported"].as_u64()), (Some(0), Some(0)), "{got}");
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
    let e = p.extrudes.iter().find(|e| e.symmetric).unwrap();
    assert!((e.fillet.as_ref().unwrap().radius - 0.5).abs() < 1e-9);
    assert!(!p.steps.iter().any(|st| st.op == "Delete"));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_shapr3d_fillet_on_chosen_edges_of_an_extrude_comes_back_on_the_same_edges() {
    // Saved by Shapr3D (kept out of the repository): n6's file through our export, then Fillet 02 = one top edge of
    // the 20×10×8 box, 2 mm.
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files/n11-edge-fillet");
    if !sample.is_file() {
        return;
    }
    let file = shapecraft_shapr::ShaprFile::read(&sample).unwrap();
    let theirs = shapecraft_shapr::Project::read(&file).unwrap();
    let mut s = session();
    let got = s.execute("import_file", &json!({"path": sample})).unwrap().value;
    assert_eq!(got["extrudes"].as_u64(), Some(2), "{got}");
    let steps: Vec<&shapecraft_doc::HistoryStep> = s.doc.history.iter().filter(|h| h.kind == "fillet").collect();
    assert_eq!(steps.len(), 2);
    let want = theirs
        .bodies
        .iter()
        .find(|b| b.name.as_deref() == Some("Body 03"))
        .map(|b| s.kernel.mass_props(&s.kernel.from_brep(&b.brep).unwrap()).unwrap().volume)
        .unwrap();
    let body = s.doc.bodies.iter().find(|b| b.name == "Body 03").unwrap().shape.clone();
    assert!((s.kernel.mass_props(&body).unwrap().volume - want).abs() < 1e-3 * want);
    assert!(want < 1600.0 - 1.0, "one edge rounded: {want}");
}

#[test]
fn a_shapr3d_chamfer_on_an_extrude_comes_back_as_our_chamfer_and_its_distance_goes_back() {
    // Saved by Shapr3D (kept out of the repository): n11 with Fillet 02 replaced by a 2 mm Chamfer on the same edge.
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files/n12-edge-chamfer");
    if !sample.is_file() {
        return;
    }
    let out = std::env::temp_dir().join(format!("shapecraft-shapr-chamfer-{}.shapr", std::process::id()));
    let theirs = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&sample).unwrap()).unwrap();
    let mut s = session();
    let got = s.execute("import_file", &json!({"path": sample})).unwrap().value;
    assert_eq!(got["extrudes"].as_u64(), Some(2), "{got}");
    let at = s.doc.history.iter().position(|h| h.kind == "chamfer").expect("our chamfer step");
    let want = theirs
        .bodies
        .iter()
        .find(|b| b.name.as_deref() == Some("Body 03"))
        .map(|b| s.kernel.mass_props(&s.kernel.from_brep(&b.brep).unwrap()).unwrap().volume)
        .unwrap();
    let body = s.doc.bodies.iter().find(|b| b.name == "Body 03").unwrap().shape.clone();
    assert!((s.kernel.mass_props(&body).unwrap().volume - want).abs() < 1e-3 * want, "{want}");
    s.execute("edit_step", &json!({"step": at, "params": {"distance": 1}})).unwrap();
    let got = s.execute("export_shapr", &json!({"path": out})).unwrap().value;
    assert_eq!((got["deleted"].as_u64(), got["imported"].as_u64()), (Some(0), Some(0)), "{got}");
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
    let c = p.extrudes.iter().find_map(|e| e.fillet.clone().filter(|f| f.kind == shapecraft_shapr::Finish::Chamfer)).unwrap();
    assert!((c.radius - 1.0).abs() < 1e-9);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn shapr3d_offset_faces_on_extrudes_come_back_as_our_offsets_and_go_back() {
    // Saved by Shapr3D (kept out of the repository), from n9: Body 03's top offset by 2 mm (n15) or to a 3 mm total
    // (n13).
    use shapecraft_shapr::Finish;
    for (file, kind, size) in [("n15-offset-box", Finish::Offset, 3.0), ("n13-offset-total", Finish::OffsetTotal, 4.0)] {
        let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files").join(file);
        if !sample.is_file() {
            return;
        }
        let out = std::env::temp_dir().join(format!("shapecraft-shapr-offset-{}-{file}.shapr", std::process::id()));
        let theirs = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&sample).unwrap()).unwrap();
        let mut s = session();
        let got = s.execute("import_file", &json!({"path": sample})).unwrap().value;
        assert_eq!(got["extrudes"].as_u64(), Some(2), "{file}: {got}");
        let at = s.doc.history.iter().position(|h| h.kind == "offset_face").expect("our offset step");
        let want = theirs
            .bodies
            .iter()
            .find(|b| b.name.as_deref() == Some("Body 03"))
            .map(|b| s.kernel.mass_props(&s.kernel.from_brep(&b.brep).unwrap()).unwrap().volume)
            .unwrap();
        let body = s.doc.bodies.iter().find(|b| b.name == "Body 03").unwrap().shape.clone();
        assert!((s.kernel.mass_props(&body).unwrap().volume - want).abs() < 1e-3 * want, "{file}: {want}");
        // One mm more: an offset of 3, or a total of 4.
        let d = s.doc.history[at].entries[0].params["distance"].as_f64().unwrap();
        s.execute("edit_step", &json!({"step": at, "params": {"distance": d + 1.0}})).unwrap();
        let got = s.execute("export_shapr", &json!({"path": out})).unwrap().value;
        assert_eq!((got["deleted"].as_u64(), got["imported"].as_u64()), (Some(0), Some(0)), "{file}: {got}");
        let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
        let f = p.extrudes.iter().find_map(|e| e.fillet.clone()).unwrap();
        assert_eq!((f.kind, (f.radius * 1e6).round() / 1e6), (kind, size), "{file}");
        let _ = std::fs::remove_file(&out);
    }
}

#[test]
fn fit_splines_go_to_shapr3d_as_shapr3d_fits_them() {
    // Shapr3D's own spline through these six points (n10, meters): poles 1 and 4.
    let pts = [
        [2.5, 3.0],
        [4.8706503584980968, 6.6141304560005669],
        [7.345651276409626, 1.664130948483944],
        [9.820651262998581, 7.439130451530218],
        [12.295651249587536, 2.4891304783523084],
        [14.77065123617649, 6.201631389558315],
    ];
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("spline", &json!({"points": pts})).unwrap();
    let out = std::env::temp_dir().join(format!("shapecraft-spline-{}.shapr", std::process::id()));
    s.execute("export_shapr", &json!({"path": out})).unwrap();
    let p = shapecraft_shapr::Project::read(&shapecraft_shapr::ShaprFile::read(&out).unwrap()).unwrap();
    let _ = std::fs::remove_file(&out);
    let poles = p.sketches.iter().flat_map(|k| &k.curves).find_map(|c| match c {
        shapecraft_shapr::SketchCurve::Spline { poles, .. } => Some(poles.clone()),
        _ => None,
    });
    let poles = poles.expect("the spline went out");
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9;
    assert!(near(poles[1], [5.891249220690948, 11.952877437305934]), "{poles:?}");
    assert!(near(poles[4], [11.242576621613703, -2.929485200868861]), "{poles:?}");
}

#[test]
fn shapr3d_bodies_survive_our_export_until_shapr3d_rebuilds_them() {
    // A project saved by Shapr3D (kept out of the repository): Body 01 extruded, moved and filleted there.
    let sample = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files/n8-move-then-fillet");
    if !sample.is_file() {
        return;
    }
    let out = std::env::temp_dir().join(format!("shapecraft-shapr-stash-{}.shapr", std::process::id()));
    let mut s = session();
    s.execute("import_file", &json!({"path": sample})).unwrap();
    s.execute("export_shapr", &json!({"path": out})).unwrap();
    // Our file keeps no Shapr3D solids (so Shapr3D replays it); opened here, its bodies still come back.
    let mut t = session();
    t.execute("import_file", &json!({"path": out})).unwrap();
    let names: Vec<&str> = t.doc.bodies.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Body 01", "Body 02"]);
    let filleted = t.kernel.mass_props(&t.doc.bodies[0].shape).unwrap().volume;
    assert!((filleted - 284.306).abs() < 0.01, "{filleted}");
    let _ = std::fs::remove_file(&out);
}

/// A `.shapr` file we wrote opens into an empty project with its own steps (editable), as Shapr3D shows a project's
/// whole history; into a project with something in it, or when the steps don't make the file's bodies, it is a
/// plain import.
#[test]
fn our_shapr_files_bring_their_history_back() {
    let dir = std::env::temp_dir().join(format!("shapecraft-own-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("b.shapr");
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 10})).unwrap();
    s.execute("set_variable", &json!({"name": "r", "value": "2"})).unwrap();
    s.execute("fillet", &json!({"body": 2, "edges": [11], "radius": "r"})).unwrap();
    s.execute("rename", &json!({"item": 2, "name": "Block"})).unwrap();
    let v0 = volume(&mut s);
    let labels: Vec<String> = s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.clone()).collect();
    let out = s.execute("export_shapr", &json!({"path": file})).unwrap().value;
    assert_eq!(out["history"], true);

    let mut t = session();
    t.execute("import_file", &json!({"path": file})).unwrap();
    let got: Vec<String> = t.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.clone()).collect();
    assert_eq!(got, labels);
    assert!((volume(&mut t) - v0).abs() < 1e-6);
    assert_eq!(t.doc.bodies[0].name, "Block");
    // Editable: the variable and the fillet's step drive the model.
    t.execute("set_variable", &json!({"name": "r", "value": "3"})).unwrap();
    let fillet = t.doc.history.iter().position(|h| h.kind == "fillet").unwrap();
    t.execute("edit_step", &json!({"step": fillet, "params": {"radius": 1}})).unwrap();
    assert!(volume(&mut t) > v0, "a smaller round leaves more");
    // Undoing the two edits and then the import leaves the project empty again.
    t.execute("undo", &json!({})).unwrap();
    t.execute("undo", &json!({})).unwrap();
    t.execute("undo", &json!({})).unwrap();
    assert!(t.doc.bodies.is_empty() && t.doc.history.is_empty());

    // Into a project that has something: a plain import (one Import step).
    let mut u = session();
    u.execute("sketch", &json!({"plane": "xy"})).unwrap();
    u.execute("circle", &json!({"center": [50, 0], "radius": 2})).unwrap();
    u.execute("exit_sketching", &json!({})).unwrap();
    u.execute("import_file", &json!({"path": file})).unwrap();
    assert!(u.doc.history.iter().any(|h| h.kind == "import_file") && !u.doc.history.iter().any(|h| h.kind == "fillet"));
    assert_eq!(u.doc.bodies.len(), 1);
    // The file's own Shapr3D steps are listed under that Import step.
    let import = u.doc.history.iter().find(|h| h.kind == "import_file").unwrap();
    assert!(
        import.shapr_steps.iter().any(|[name, op]| name.contains("Block") && op == "MaterializeImportedBodies"),
        "{:?}",
        import.shapr_steps
    );

    // Steps that don't make the file's bodies (tampered with) are left out.
    let bytes = std::fs::read(&file).unwrap();
    let shapr = shapecraft_shapr::ShaprFile::parse(&bytes).unwrap();
    let db = shapr.db().unwrap();
    let json: Vec<u8> = db.conn.query_row("SELECT Data FROM ShapeCraftHistory", [], |r| r.get(0)).unwrap();
    let tampered = String::from_utf8(json).unwrap().replace("\"distance\":10", "\"distance\":12");
    db.conn.execute("UPDATE ShapeCraftHistory SET Data = ?1", [tampered.as_bytes()]).unwrap();
    let bad = shapecraft_shapr::ShaprFile { metadata: shapr.metadata.clone(), workspace: db.bytes().unwrap() };
    let file2 = dir.join("c.shapr");
    std::fs::write(&file2, bad.to_bytes()).unwrap();
    let mut w = session();
    w.execute("import_file", &json!({"path": file2})).unwrap();
    assert!((volume(&mut w) - v0).abs() < 1e-6);
    assert!(!w.doc.history.iter().any(|h| h.kind == "fillet"));
    let _ = std::fs::remove_dir_all(&dir);
}

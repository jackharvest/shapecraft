//! Import and export of neutral formats: meshes, IGES, STEP, USDZ, other projects, and telling files apart by
//! their contents.

use serde_json::json;

use super::{session, volume};

#[test]
fn mesh_and_iges_exports_round_trip() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).to_string_lossy().into_owned();
    s.execute("export_obj", &json!({"path": path("p.obj")})).unwrap();
    assert!(std::fs::read_to_string(dir.join("p.obj")).unwrap().contains("mtllib p.mtl"));
    assert!(std::fs::read_to_string(dir.join("p.mtl")).unwrap().contains("Kd "));
    s.execute("export_3mf", &json!({"path": path("p.3mf")})).unwrap();
    let z = std::fs::read(dir.join("p.3mf")).unwrap();
    assert!(z.starts_with(b"PK\x03\x04") && z.windows(16).any(|w| w == b"3D/3dmodel.model"));
    s.execute("export_glb", &json!({"path": path("p.glb")})).unwrap();
    let g = std::fs::read(dir.join("p.glb")).unwrap();
    assert!(g.starts_with(b"glTF") && u32::from_le_bytes([g[8], g[9], g[10], g[11]]) as usize == g.len());
    s.execute("export_iges", &json!({"path": path("p.igs")})).unwrap();
    s.execute("export_step", &json!({"path": path("p242.step"), "protocol": "AP242"})).unwrap();
    assert!(std::fs::read_to_string(dir.join("p242.step")).unwrap().contains("AP242"));
    assert!(s.execute("export_step", &json!({"path": path("x.step"), "protocol": "AP999"})).is_err());
    s.execute("export_stl", &json!({"path": path("p.stl")})).unwrap();
    // Each comes back as a closed body of the same volume.
    for (n, file) in ["p.igs", "p.stl", "p.obj"].iter().enumerate() {
        let out = s.execute("import_file", &json!({"path": path(file)})).unwrap().value;
        assert_eq!(out["bodies"].as_array().unwrap().len(), 1, "{file}");
        assert_eq!(s.doc.bodies.len(), n + 2);
        let body = s.doc.bodies.last().unwrap();
        let v = s.kernel.mass_props(&body.shape).unwrap().volume;
        assert!((v - 1000.0).abs() < 1e-3, "{file}: {v}");
    }
    assert_eq!(s.doc.bodies.last().unwrap().name, "p");
    // Drawings come in as a sketch named after the file, with regions.
    std::fs::write(dir.join("plate.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="20mm" viewBox="0 0 40 20"><rect width="40" height="20"/><circle cx="20" cy="10" r="5"/></svg>"#).unwrap();
    let out = s.execute("import_file", &json!({"path": path("plate.svg")})).unwrap().value;
    assert_eq!(out["elements"], 5);
    let sk = s.doc.sketches.last().unwrap();
    assert_eq!(sk.name, "plate");
    // The sketch goes back out as a DXF and reads in again the same.
    s.execute("export_sketch", &json!({"path": path("plate.dxf"), "sketch": out["sketch"]})).unwrap();
    let again = s.execute("import_file", &json!({"path": path("plate.dxf")})).unwrap().value;
    assert_eq!(again["elements"], 5);
    // Options: ASCII STL in inches, low resolution, hidden bodies.
    s.execute("export_stl", &json!({"path": path("a.stl"), "ascii": true, "units": "in", "resolution": "low", "hidden": true})).unwrap();
    let text = std::fs::read_to_string(dir.join("a.stl")).unwrap();
    assert!(text.starts_with("solid") && text.contains("vertex 0.787"), "20 mm = 0.787 in");
    assert!(s.execute("export_stl", &json!({"path": path("b.stl"), "units": "furlong"})).is_err());
    std::fs::write(dir.join("bad.stl"), b"solid x\nfacet normal 0 0 1\nouter loop\nvertex nan 0 0\n").unwrap();
    assert!(s.execute("import_file", &json!({"path": path("bad.stl")})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_project_brings_items_in_a_folder() {
    let mut a = session();
    a.execute("sketch", &json!({"plane": "xy"})).unwrap();
    a.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let sk = a.active_sketch.unwrap();
    a.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 10})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-import-{}", std::process::id()));
    a.doc.name = "Bracket".into();
    shapecraft_doc::project::save(&a.doc, a.kernel.as_ref(), &dir).unwrap();

    let mut b = session();
    b.execute("sketch", &json!({"plane": "xy"})).unwrap();
    b.execute("circle", &json!({"center": [50, 0], "radius": 5})).unwrap();
    let out = b.execute("import_project", &json!({"path": dir.to_string_lossy()})).unwrap();
    assert_eq!(b.doc.bodies.len(), 1);
    assert_eq!(b.doc.sketches.len(), 2);
    let folder = b.doc.folders.last().unwrap();
    assert_eq!(folder.name, "Bracket (Imported)");
    assert_eq!(folder.items.len(), 2);
    assert_eq!(out.value["bodies"].as_array().unwrap().len(), 1);
    assert!((volume(&mut b) - 1000.0).abs() < 1e-6);
    assert_eq!(b.doc.history.last().unwrap().label, "Import Project 01");
    assert!(b.execute("import_project", &json!({"path": "/nonexistent/project"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn usdz_export_is_an_aligned_package_of_a_usda_layer() {
    let dir = std::env::temp_dir().join(format!("shapecraft-usdz-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    s.execute("set_appearance", &json!({"items": [s.doc.bodies[0].id.0], "appearance": "Polished Gold"})).unwrap();
    let path = dir.join("part.usdz");
    s.execute("export_usdz", &json!({"path": path})).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    let extra = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
    let start = 30 + name_len + extra;
    assert_eq!(start % 64, 0, "the layer starts on a 64-byte boundary");
    let text = String::from_utf8_lossy(&bytes[start..start + 400]);
    assert!(text.starts_with("#usda 1.0") && text.contains("metersPerUnit = 0.001"));
    let all = String::from_utf8_lossy(&bytes);
    assert!(all.contains("def Mesh \"Body_01\"") && all.contains("inputs:metallic = 1"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn step_keeps_part_names_and_colors_both_ways() {
    let dir = std::env::temp_dir().join(format!("shapecraft-step-names-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [30, 0], "radius": 4})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 8})).unwrap();
    let (a, b) = (s.doc.bodies[0].id.0, s.doc.bodies[1].id.0);
    s.execute("rename", &json!({"item": a, "name": "Bracket Plate"})).unwrap();
    s.execute("rename", &json!({"item": b, "name": "Pin"})).unwrap();
    s.execute("set_appearance", &json!({"items": [b], "appearance": {"name": "Red", "color": [1.0, 0.0, 0.0], "metal": 0, "gloss": 0.3}}))
        .unwrap();
    for ap in ["AP214", "AP242"] {
        let path = dir.join(format!("parts-{ap}.step"));
        s.execute("export_step", &json!({"path": path, "protocol": ap})).unwrap();
        let mut t = session();
        let out = t.execute("import_file", &json!({"path": path})).unwrap();
        assert_eq!(out.value["bodies"].as_array().unwrap().len(), 2);
        let names: Vec<&str> = t.doc.bodies.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"Bracket Plate") && names.contains(&"Pin"), "{ap}: {names:?}");
        let pin = t.doc.bodies.iter().find(|b| b.name == "Pin").unwrap();
        let c = pin.appearance.as_ref().map(|a| a.color).unwrap_or([0.0; 3]);
        assert!((c[0] - 1.0).abs() < 0.01 && c[1] < 0.01, "{ap}: {c:?}");
        assert!(t.doc.bodies.iter().find(|b| b.name == "Bracket Plate").unwrap().appearance.is_none());
    }
    // Items folders go out as assemblies and come back as folders.
    s.execute("new_folder", &json!({"name": "Hinge", "items": [b]})).unwrap();
    let path = dir.join("assembly.step");
    s.execute("export_step", &json!({"path": path})).unwrap();
    let mut t = session();
    t.execute("import_file", &json!({"path": path})).unwrap();
    // Everything lands in a folder named after the file (as Shapr3D's Import File does), assemblies below it.
    let names: Vec<&str> = t.doc.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["assembly.step", "assembly.step / Hinge"]);
    let pin = t.doc.bodies.iter().find(|b| b.name == "Pin").unwrap();
    assert_eq!(t.doc.folders[1].items, vec![pin.id]);
    let c = pin.appearance.as_ref().map(|a| a.color).unwrap_or([0.0; 3]);
    assert!((c[0] - 1.0).abs() < 0.01, "the color survives the assembly: {c:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_without_an_extension_are_told_apart_by_their_contents() {
    let dir = std::env::temp_dir().join(format!("shapecraft-sniff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sk.0, "region": 0}]})).unwrap();
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    // Shapr3D saves under the bare name it is given.
    s.execute("export_shapr", &json!({"path": dir.join("project")})).unwrap();
    s.execute("export_parasolid", &json!({"path": dir.join("part")})).unwrap();
    for name in ["project", "part"] {
        let mut t = session();
        let got = t.execute("import_file", &json!({"path": dir.join(name)})).unwrap().value;
        assert_eq!(got["bodies"].as_array().map(Vec::len), Some(1), "{name}");
        assert!((volume(&mut t) - 500.0).abs() < 1e-6, "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

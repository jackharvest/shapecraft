//! Import and export of neutral formats: meshes, IGES, STEP, USDZ, other projects, and telling files apart by
//! their contents.

use serde_json::json;

use super::{session, volume};

/// A SolidWorks part: the first configuration's model partition comes in as one body named after the file;
/// hidden faceless bodies and other configurations stay out, and `config` picks another one.
#[test]
fn solidworks_part_imports_its_model_partition() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-sw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let xb = dir.join("box.x_b");
    s.execute("export_parasolid", &json!({"path": xb.to_string_lossy()})).unwrap();
    let ps = std::fs::read(&xb).unwrap();
    // Config 1 is twice as tall: export it from a second session.
    let mut t = session();
    t.execute("sketch", &json!({"plane": "xy"})).unwrap();
    t.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sk = t.active_sketch.unwrap();
    t.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 10})).unwrap();
    let tall = dir.join("tall.x_b");
    t.execute("export_parasolid", &json!({"path": tall.to_string_lossy()})).unwrap();
    let tall = std::fs::read(&tall).unwrap();
    let part = dir.join("Bracket.SLDPRT");
    std::fs::write(
        &part,
        shapecraft_xt::sldprt::container(&[("Contents/Config-1-Partition", &tall), ("Contents/Config-0-Partition", &ps)]),
    )
    .unwrap();
    let before = s.doc.bodies.len();
    let out = s.execute("import_file", &json!({"path": part.to_string_lossy()})).unwrap().value;
    assert_eq!(out["bodies"].as_array().unwrap().len(), 1);
    assert_eq!(out["configurations"], json!([0, 1]));
    assert_eq!(s.doc.bodies.len(), before + 1);
    assert_eq!(s.doc.bodies.last().unwrap().name, "Bracket");
    let last = |s: &mut crate::Session| {
        let id = s.doc.bodies.last().unwrap().id.0;
        s.execute("inspect", &json!({"body": id, "faces": false})).unwrap().value["bodies"][0]["volume_mm3"].as_f64().unwrap()
    };
    assert!((last(&mut s) - 1000.0).abs() < 1e-3);
    s.execute("import_file", &json!({"path": part.to_string_lossy(), "config": 1})).unwrap();
    assert!((last(&mut s) - 2000.0).abs() < 1e-3);
    assert!(s.execute("import_file", &json!({"path": part.to_string_lossy(), "config": 7})).is_err());
    // Without the extension it's still told apart by its contents.
    let bare = dir.join("Bracket");
    std::fs::copy(&part, &bare).unwrap();
    s.execute("import_file", &json!({"path": bare.to_string_lossy()})).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// NX, Solid Edge and JT files built around our own Parasolid export import as that body, named after the file;
/// a JT with only meshes, or a Creo `.prt`, says why it can't.
#[test]
fn nx_solid_edge_and_jt_parts_import() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let xb = dir.join("box.x_b");
    s.execute("export_parasolid", &json!({"path": xb.to_string_lossy()})).unwrap();
    let ps = std::fs::read(&xb).unwrap();
    use shapecraft_xt::native::{jt_container, nx_container, solid_edge_container};
    for (name, bytes) in
        [("Bracket.prt", nx_container(&ps)), ("Bracket.par", solid_edge_container(&ps).unwrap()), ("Bracket.jt", jt_container(&ps))]
    {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut t = session();
        let out = t.execute("import_file", &json!({"path": path.to_string_lossy()})).unwrap_or_else(|e| panic!("{name}: {e}")).value;
        assert_eq!(out["bodies"].as_array().unwrap().len(), 1, "{name}");
        assert_eq!(t.doc.bodies[0].name, "Bracket", "{name}");
        let id = t.doc.bodies[0].id.0;
        let v = t.execute("inspect", &json!({"body": id, "faces": false})).unwrap().value["bodies"][0]["volume_mm3"].as_f64().unwrap();
        assert!((v - 1000.0).abs() < 1e-3, "{name}: {v}");
        // Without the extension it's still told apart by its contents.
        let bare = dir.join(format!("bare-{}", name.replace('.', "-")));
        std::fs::copy(&path, &bare).unwrap();
        t.execute("import_file", &json!({"path": bare.to_string_lossy()})).unwrap_or_else(|e| panic!("bare {name}: {e}"));
    }
    // A JT with a mesh segment only.
    let mut mesh_only = jt_container(&ps);
    let toc = u32::from_le_bytes(mesh_only[85..89].try_into().unwrap()) as usize;
    mesh_only[toc + 4 + 27] = 7;
    let path = dir.join("Viewer.jt");
    std::fs::write(&path, mesh_only).unwrap();
    let e = s.execute("import_file", &json!({"path": path.to_string_lossy()})).unwrap_err().to_string();
    assert!(e.contains("only display meshes"), "{e}");
    let creo = dir.join("Creo.prt");
    std::fs::write(&creo, b"#UGC:2 PART 1234 650 0 2 1 0 0 0 0").unwrap();
    let e = s.execute("import_file", &json!({"path": creo.to_string_lossy()})).unwrap_err().to_string();
    assert!(e.contains("Creo"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

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
    s.execute("export_fbx", &json!({"path": path("p.fbx")})).unwrap();
    let f = std::fs::read(dir.join("p.fbx")).unwrap();
    assert!(f.starts_with(b"Kaydara FBX Binary  \0\x1a\0") && u32::from_le_bytes([f[23], f[24], f[25], f[26]]) == 7400);
    assert!(fbx_nodes_end_where_they_say(&f));
    s.execute("export_vrml", &json!({"path": path("p.wrl")})).unwrap();
    let w = std::fs::read_to_string(dir.join("p.wrl")).unwrap();
    assert!(w.starts_with("#VRML V2.0 utf8") && w.matches(", -1,").count() >= 12 && w.contains("scale 0.001"));
    // Each comes back as a closed body of the same volume: IGES exact, STL and OBJ as mesh bodies, each in a folder named after its file.
    for (n, file) in ["p.igs", "p.stl", "p.obj"].iter().enumerate() {
        let out = s.execute("import_file", &json!({"path": path(file)})).unwrap().value;
        assert_eq!(out["bodies"].as_array().unwrap().len(), 1, "{file}");
        assert_eq!(s.doc.bodies.len(), n + 2);
        let id = s.doc.bodies.last().unwrap().id.0;
        let info = s.execute("inspect", &json!({"body": id, "faces": false})).unwrap().value;
        let v = info["bodies"][0]["volume_mm3"].as_f64().unwrap();
        assert!((v - 1000.0).abs() < 1e-3, "{file}: {v}");
        assert_eq!(info["bodies"][0]["mesh"].as_bool().unwrap_or(false), *file != "p.igs", "{file}");
    }
    assert_eq!(s.doc.bodies.last().unwrap().name, "Mesh body 01");
    assert_eq!(s.doc.folders.last().unwrap().name, "p.obj");
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
    // And as a DWG: every CRC good, one entity per element.
    s.execute("export_sketch", &json!({"path": path("plate.dwg"), "sketch": out["sketch"]})).unwrap();
    let f = shapecraft_dwg::inspect(&std::fs::read(dir.join("plate.dwg")).unwrap()).unwrap();
    let drawn = f.entities().filter(|o| !matches!(o.kind.as_str(), "BLOCK" | "ENDBLK")).count();
    assert_eq!(drawn, 5);
    // And as an SVG: one sketch is a plain SVG, sized in millimetres too, which reads back in.
    let svg = s.execute("export_sketch", &json!({"path": path("plate.svg"), "sketch": out["sketch"], "join": false})).unwrap().value;
    assert_eq!(svg["files"], json!(["plate.svg"]));
    let text = std::fs::read_to_string(dir.join("plate.svg")).unwrap();
    assert!(text.contains("width=\"44.") && text.contains("mm\""), "{text}");
    let back = crate::formats::read_svg(&text).unwrap();
    assert!(!back.is_empty());
    // Several sketches, one on a side plane: one SVG each, face on, packed in a zip named like the file.
    s.execute("sketch", &json!({"plane": "yz"})).unwrap();
    s.execute("circle", &json!({"center": [5, 5], "radius": 3})).unwrap();
    let side = s.doc.sketches.last().unwrap().name.clone();
    let all = s.execute("export_sketch", &json!({"path": path("all.svg")})).unwrap().value;
    assert!(all["path"].as_str().unwrap().ends_with("all.zip"), "{all}");
    let names: Vec<&str> = all["files"].as_array().unwrap().iter().filter_map(|n| n.as_str()).collect();
    assert!(names.contains(&"plate.svg") && names.contains(&format!("{side}.svg").as_str()), "{names:?}");
    let z = std::fs::read(dir.join("all.zip")).unwrap();
    assert!(z.starts_with(b"PK\x03\x04") && z.windows(9).any(|w| w == b"plate.svg"));
    // A circle on the side plane is a circle, not an edge-on line.
    let one = s.execute("export_sketch", &json!({"path": path("side.svg"), "sketch": s.doc.sketches.last().unwrap().id.0})).unwrap().value;
    assert_eq!(one["files"], json!(["side.svg"]));
    assert!(std::fs::read_to_string(dir.join("side.svg")).unwrap().contains(" A11.3386,11.3386 0 0,0 "));
    // The selection: just the picked sketch; nothing usable picked means every sketch.
    s.selection = vec![shapecraft_doc::Pick::Sketch { sketch: shapecraft_doc::ItemId(out["sketch"].as_u64().unwrap()) }];
    let picked = s.execute("export_sketch", &json!({"path": path("picked.svg"), "selected": true})).unwrap().value;
    assert_eq!((picked["files"].clone(), picked["sketches"].clone()), (json!(["picked.svg"]), json!(1)));
    // A flat face of the box: its outline alone, closed.
    let body = s.doc.bodies[0].id.0;
    let face = s.execute("export_sketch", &json!({"path": path("face.svg"), "faces": [{"body": body, "face": 0}]})).unwrap().value;
    assert_eq!(face["files"].as_array().unwrap().len(), 1, "{face}");
    let text = std::fs::read_to_string(dir.join("face.svg")).unwrap();
    assert_eq!(text.matches("<polygon").count(), 1, "{text}");
    assert!(s.execute("export_sketch", &json!({"path": path("bad.svg"), "resolution": 101})).is_err());
    assert!(s.execute("export_sketch", &json!({"path": path("bad.svg"), "dpi": 0})).is_err());
    // Options: ASCII STL in inches, low resolution, hidden bodies.
    s.execute("export_stl", &json!({"path": path("a.stl"), "ascii": true, "units": "in", "resolution": "low", "hidden": true})).unwrap();
    // Low is coarser than High, a typed tolerance finer than either, and a later High isn't what Low left behind.
    let mut round = session();
    round.execute("sketch", &json!({"plane": "xy"})).unwrap();
    round.execute("circle", &json!({"center": [0, 0], "radius": 30})).unwrap();
    round.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 20})).unwrap();
    let mut triangles = |p: serde_json::Value| {
        let mut p = p;
        p["path"] = json!(path("r.stl"));
        round.execute("export_stl", &p).map(|o| o.value["triangles"].as_u64().unwrap())
    };
    let high = triangles(json!({"resolution": "high"})).unwrap();
    let low = triangles(json!({"resolution": "low"})).unwrap();
    let fine = triangles(json!({"tolerance": 0.002})).unwrap();
    assert!(low < high && high < fine, "{low} {high} {fine}");
    assert_eq!(triangles(json!({"resolution": "high"})).unwrap(), high);
    assert!(triangles(json!({"tolerance": 0})).is_err());
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
        // The names go out in the file; coming in, parts are numbered per file.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("'Bracket Plate'") && text.contains("'Pin'"), "{ap}");
        let names: Vec<&str> = t.doc.bodies.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Imported 01", "Imported 02"], "{ap}");
        let pin = &t.doc.bodies[1];
        let c = pin.appearance.as_ref().map(|a| a.color).unwrap_or([0.0; 3]);
        assert!((c[0] - 1.0).abs() < 0.01 && c[1] < 0.01, "{ap}: {c:?}");
        assert!(t.doc.bodies[0].appearance.is_none());
    }
    // Items folders go out as assemblies and come back as folders.
    s.execute("new_folder", &json!({"name": "Hinge", "items": [b]})).unwrap();
    let path = dir.join("assembly.step");
    s.execute("export_step", &json!({"path": path})).unwrap();
    let mut t = session();
    t.execute("import_file", &json!({"path": path})).unwrap();
    // Everything lands in a folder named after the file (as Import File does), assemblies as folders
    // below it, nested like the assembly.
    let names: Vec<&str> = t.doc.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["assembly.step", "Hinge"]);
    let pin = t.doc.bodies.iter().find(|b| b.appearance.is_some()).unwrap();
    assert_eq!(t.doc.folders[1].items, vec![pin.id]);
    assert!(t.doc.folders[0].items.contains(&t.doc.folders[1].id), "Hinge sits inside the file's folder");
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

/// A STEP file's parts come in as "Imported NN" whatever the file names them, nothing selected,
/// and every Import Preferences clean-up keeps a sound solid.
#[test]
fn step_parts_are_numbered_imported_and_left_unselected() {
    let mut s = session();
    for (corner, opposite) in [([0, 0], [20, 10]), ([40, 0], [50, 10])] {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": corner, "opposite": opposite})).unwrap();
        let sketch = s.active_sketch.unwrap();
        s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("shapecraft-heal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two.step").to_string_lossy().into_owned();
    s.execute("export_step", &json!({"path": path})).unwrap();
    for heal in
        [json!(null), json!("quality"), json!("speed"), json!({"repair": true, "sew": true}), json!({"simplify": true, "edges": true})]
    {
        let mut t = session();
        let out = t.execute("import_file", &json!({"path": path, "heal": heal})).unwrap().value;
        assert_eq!(out["bodies"].as_array().unwrap().len(), 2, "{heal}");
        let names: Vec<_> = t.doc.bodies.iter().map(|b| b.name.clone()).collect();
        assert_eq!(names, ["Imported 01", "Imported 02"], "{heal}");
        assert!(t.selection.is_empty(), "{heal}");
        let v: f64 = t.doc.bodies.iter().map(|b| t.kernel.mass_props(&b.shape).unwrap().volume).sum();
        assert!((v - 1500.0).abs() < 1e-3, "{heal}: {v}");
    }
    // A second file starts at 01 again.
    s.execute("import_file", &json!({"path": path})).unwrap();
    s.execute("import_file", &json!({"path": path})).unwrap();
    assert_eq!(s.doc.bodies.iter().filter(|b| b.name == "Imported 01").count(), 2);
    assert!(s.execute("import_file", &json!({"path": path, "heal": 3})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A long import says what it is doing: the stage it is in and how many of its steps are done, for the screen
/// that waits for it.
#[test]
fn an_import_reports_its_stages() {
    let dir = std::env::temp_dir().join(format!("sc-progress-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("two.step");
    let mut a = session();
    for (x0, x1) in [(0, 10), (20, 30)] {
        a.execute("sketch", &json!({"plane": "xy"})).unwrap();
        a.execute("rectangle", &json!({"corner": [x0, 0], "opposite": [x1, 10]})).unwrap();
        a.execute("extrude", &json!({"sketch": a.active_sketch.unwrap().0, "region": 0, "distance": 4, "result": "new"})).unwrap();
    }
    a.execute("export_step", &json!({"path": file})).unwrap();
    let progress = std::sync::Arc::new(crate::progress::Progress::default());
    let mut s = session();
    s.progress = Some(progress.clone());
    s.execute("import_file", &json!({"path": file})).unwrap();
    let (text, done, total) = progress.snapshot();
    assert_eq!(text, "Repairing and adding the parts…");
    assert_eq!((done, total), (2, 2), "both parts counted");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Real parts from other CAD systems (not in the repo: their licences vary): SolidWorks, NX, Solid Edge, JT and
/// Parasolid. `SHAPECRAFT_PARTS_DIR=folder cargo test -p shapecraft-engine real_cad_parts -- --ignored --nocapture`
/// imports each and prints what came in (SHAPECRAFT_PARTS_OUT=folder also writes each as STEP).
#[test]
#[ignore]
fn real_cad_parts() {
    let Ok(dir) = std::env::var("SHAPECRAFT_PARTS_DIR") else { return };
    let mut failed = 0;
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|f| f.path()).collect();
    files.sort();
    for path in files {
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !["sldprt", "prt", "par", "psm", "pwd", "jt", "x_t", "x_b"].contains(&ext.as_str()) {
            continue;
        }
        let mut s = session();
        match s.execute("import_file", &json!({"path": path.to_string_lossy()})) {
            Ok(out) => {
                let info = s.execute("inspect", &json!({"faces": false})).unwrap().value;
                // SHAPECRAFT_PARTS_OUT: each import also written as STEP, to check with another system.
                if let Ok(out_dir) = std::env::var("SHAPECRAFT_PARTS_OUT") {
                    let step = std::path::Path::new(&out_dir).join(path.with_extension("step").file_name().unwrap());
                    s.execute("export_step", &json!({"path": step.to_string_lossy()})).unwrap();
                }
                let v: Vec<String> = info["bodies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| format!("{:.4} mm³ ({} faces)", b["volume_mm3"].as_f64().unwrap_or(0.0), b["face_count"]))
                    .collect();
                let name = path.file_name().unwrap().to_string_lossy();
                println!("{name}: {} bodies: {v:?} {}", out.value["bodies"].as_array().unwrap().len(), out.value["note"]);
            }
            Err(e) => {
                failed += 1;
                println!("{}: FAILED {e}", path.file_name().unwrap().to_string_lossy());
            }
        }
    }
    println!("{failed} failed");
}

/// Walks a binary FBX's top-level records: each end offset lands on the next record, ending at the null record.
fn fbx_nodes_end_where_they_say(f: &[u8]) -> bool {
    let u = |at: usize| u32::from_le_bytes([f[at], f[at + 1], f[at + 2], f[at + 3]]) as usize;
    let mut at = 27;
    let mut names = Vec::new();
    loop {
        let end = u(at);
        if end == 0 {
            return names == ["FBXHeaderExtension", "Creator", "GlobalSettings", "Definitions", "Objects", "Connections"];
        }
        if end <= at || end > f.len() {
            return false;
        }
        let len = f[at + 12] as usize;
        names.push(String::from_utf8_lossy(&f[at + 13..at + 13 + len]).into_owned());
        at = end;
    }
}

/// Writes a two-body, two-colour model as FBX and VRML for checking in other apps:
/// `SHAPECRAFT_EXPORT_DIR=folder cargo test -p shapecraft-engine sample_mesh_exports -- --ignored`.
#[test]
#[ignore]
fn sample_mesh_exports() {
    let Ok(dir) = std::env::var("SHAPECRAFT_EXPORT_DIR") else { return };
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [40, 20]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 10})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [80, 10], "radius": 10})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 30})).unwrap();
    let ids: Vec<u64> = s.doc.bodies.iter().map(|b| b.id.0).collect();
    for (id, c) in [(ids[0], [0.8, 0.2, 0.1]), (ids[1], [0.1, 0.4, 0.8])] {
        s.execute("set_appearance", &json!({"items": [id], "appearance": {"name": "Paint", "color": c, "metal": 0, "gloss": 0.5}}))
            .unwrap();
    }
    for (cmd, f) in [("export_fbx", "sample.fbx"), ("export_vrml", "sample.wrl"), ("export_glb", "sample.glb")] {
        s.execute(cmd, &json!({"path": format!("{dir}/{f}")})).unwrap();
    }
}

/// 3MF (#93): what we export comes back as mesh bodies in a folder named after the file, with names and colors.
#[test]
fn a_3mf_comes_back_with_names_and_colors() {
    let mut s = session();
    let id = super::holed_block(&mut s).0;
    s.execute("rename", &json!({"item": id, "name": "Lid & hinge"})).unwrap();
    s.execute(
        "set_appearance",
        &json!({"items": [id], "appearance": {"name": "Red", "color": [1.0, 0.0, 0.0], "metal": 0.0, "gloss": 0.5}}),
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-3mf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("part.3mf");
    s.execute("export_3mf", &json!({"path": path})).unwrap();
    let mut t = session();
    let v = t.execute("import_file", &json!({"path": path})).unwrap().value;
    assert_eq!(v["mesh"], json!(true));
    let b = &t.doc.bodies[0];
    assert_eq!(b.name, "Lid & hinge");
    assert!(b.is_mesh() && b.facets.as_ref().unwrap().is_closed());
    let c = b.appearance.as_ref().unwrap().color;
    assert!(c[0] > 0.99 && c[1] < 0.01 && c[2] < 0.01, "{c:?}");
    assert_eq!(t.doc.folders[0].name, "part.3mf");
    // Told by its contents when the name doesn't say, and taken back in one undo.
    let bare = dir.join("part");
    std::fs::copy(&path, &bare).unwrap();
    t.execute("undo", &json!({})).unwrap();
    assert!(t.doc.bodies.is_empty());
    t.execute("import_file", &json!({"path": bare})).unwrap();
    assert_eq!(t.doc.bodies.len(), 1);
}

/// A material's opacity, glass and glow go into a GLB file, and a GLB file's materials come back as custom ones.
#[test]
fn glb_materials_go_out_and_come_back() {
    let mut s = super::session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10, "result": "new"})).unwrap();
    let body = s.doc.bodies[0].id;
    s.execute("rename", &json!({"item": body.0, "name": "Lamp"})).unwrap();
    let look = json!({"name": "Lamp", "color": [0.2, 0.5, 0.8], "metal": 0.25, "gloss": 0.75, "opacity": 0.5, "transmission": 0.6, "ior": 1.33, "emission": 0.4});
    s.execute("set_appearance", &json!({"items": [body.0], "appearance": look})).unwrap();
    let dir = std::env::temp_dir().join(format!("sc-glb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lamp.glb");
    s.execute("export_glb", &json!({"path": path})).unwrap();
    let out = s.execute("import_material", &json!({"path": path})).unwrap();
    let id = out.value["image"].as_u64().unwrap();
    s.execute("set_appearance", &json!({"items": [body.0], "custom": id})).unwrap();
    let a = s.doc.bodies[0].appearance.clone().unwrap();
    assert_eq!(a.name, "Lamp");
    assert!(a.texture.is_none(), "a plain color has no picture to tile");
    for (got, want) in [(a.metal, 0.25), (a.gloss, 0.75), (a.opacity, 0.5), (a.transmission, 0.6), (a.ior, 1.33), (a.color[1], 0.5)] {
        assert!((got - want).abs() < 0.01, "{got} vs {want}");
    }
    assert!(s.execute("import_material", &json!({"path": dir.join("nothing.glb")})).is_err());
    std::fs::write(dir.join("bad.glb"), b"glTF\x02\0\0\0\xff\xff\xff\xff\xff\xff\xff\x7fJSON").unwrap();
    assert!(s.execute("import_material", &json!({"path": dir.join("bad.glb")})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// DXF from every sketch plane goes into one file each, zipped; painted faces go into OBJ as vertex colors.
#[test]
fn sketch_planes_separately_and_painted_vertices() {
    let mut s = super::session();
    for plane in ["xy", "xz"] {
        s.execute("sketch", &json!({"plane": plane})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        s.execute("exit_sketching", &json!({})).unwrap();
    }
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [30, 0], "radius": 4})).unwrap();
    s.execute("exit_sketching", &json!({})).unwrap();
    let dir = std::env::temp_dir().join(format!("sc-planes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = s.execute("export_sketch", &json!({"path": dir.join("all.dxf"), "separately": true})).unwrap();
    assert_eq!(out.value["planes"], 2);
    let zip = std::fs::read(dir.join("all.zip")).unwrap();
    assert!(zip.windows(6).any(|w| w == b"CIRCLE"));
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 5, "result": "new"})).unwrap();
    let body = s.doc.bodies[0].id;
    s.execute("set_face_color", &json!({"color": [1.0, 0.0, 0.0], "faces": [{"body": body.0, "face": 0}]})).unwrap();
    s.execute("export_obj", &json!({"path": dir.join("a.obj"), "vertex_colors": true})).unwrap();
    let obj = std::fs::read_to_string(dir.join("a.obj")).unwrap();
    assert!(obj.lines().any(|l| l.starts_with("v ") && l.ends_with("1.0000 0.0000 0.0000")), "{}", &obj[..200]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A STEP file's loose planar curves come in as sketches with their regions, unless told not to.
#[test]
fn step_curves_come_in_as_sketches() {
    use glam::dvec3;
    use shapecraft_kernel::{PlanarCurve as C, PlaneFrame};
    let mut s = super::session();
    let frame = PlaneFrame { origin: dvec3(0.0, 0.0, 7.0), x: glam::DVec3::X, y: glam::DVec3::Y };
    let p = |x: f64, y: f64| dvec3(x, y, 7.0);
    let curves = [
        C::Line { a: p(0.0, 0.0), b: p(20.0, 0.0) },
        C::Line { a: p(20.0, 0.0), b: p(20.0, 10.0) },
        C::Arc { a: p(20.0, 10.0), mid: p(10.0, 14.0), b: p(0.0, 10.0) },
        C::Line { a: p(0.0, 10.0), b: p(0.0, 0.0) },
        C::Circle { center: p(10.0, 5.0), radius: 2.0 },
    ];
    let shape = s.kernel.curves_shape(frame, &curves).unwrap();
    let dir = std::env::temp_dir().join(format!("sc-wires-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("wires.step");
    s.kernel.write_step(&[shape], &path).unwrap();
    let out = s.execute("import_file", &json!({"path": path})).unwrap();
    let ids = out.value["sketches"].as_array().unwrap().clone();
    assert!(!ids.is_empty(), "{:?}", out.value);
    let sk = &s.doc.sketches[0];
    assert!((sk.plane.origin.z - 7.0).abs() < 1e-9 && sk.plane.normal().z > 0.99);
    let total: usize = s.doc.sketches.iter().map(|k| k.entities.len()).sum();
    assert_eq!(total, 5);
    let mut t = super::session();
    let _ = t.execute("import_file", &json!({"path": path, "heal": {"curves": false}}));
    assert!(t.doc.sketches.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

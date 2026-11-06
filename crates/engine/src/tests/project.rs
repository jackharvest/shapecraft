//! The project as a document: save and reload, appearances, materials, decals, reference images, folders and
//! saved views.

use std::sync::Arc;

use serde_json::json;
use shapecraft_doc::Pick;
use shapecraft_kernel_occt::OcctKernel;

use super::{face_facing, session, volume};
use crate::Session;

#[test]
fn save_and_reload_round_trip() {
    let dir = std::env::temp_dir().join(format!("shapecraft-proj-{}", std::process::id()));
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xz"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [4, 6]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 3})).unwrap();
    s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
    let kernel = Arc::new(OcctKernel::new());
    let doc = shapecraft_doc::project::load(kernel.as_ref(), &dir).unwrap();
    assert_eq!(doc.bodies.len(), 1);
    assert_eq!(doc.sketches.len(), 1);
    assert_eq!(doc.bodies[0].name, "Body 01");
    let mut s2 = Session::with_document(kernel, doc, Some(dir.clone()));
    assert!((volume(&mut s2) - 72.0).abs() < 1e-6);
    assert_eq!(s2.doc.next_body_name(), "Body 02");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_save_cut_short_leaves_the_last_project_openable() {
    let dir = std::env::temp_dir().join(format!("shapecraft-cut-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = session();
    let body = |s: &mut Session, x: f64| {
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"center": [x, 0], "size": [4, 4]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    };
    let save = json!({"path": dir.to_string_lossy()});
    body(&mut s, 0.0);
    s.execute("save", &save).unwrap();
    let kernel = OcctKernel::new();

    // What a save dying partway leaves: new bodies under the other name, project.json not yet swapped.
    std::fs::write(dir.join("bodies-next.step"), b"half a file").unwrap();
    std::fs::write(dir.join(".~project.json.tmp"), b"{").unwrap();
    assert_eq!(shapecraft_doc::project::load(&kernel, &dir).unwrap().bodies.len(), 1);

    // The next saves take turns and clean up after themselves.
    body(&mut s, 10.0);
    for name in ["bodies-next.step", "bodies.step"] {
        s.execute("save", &save).unwrap();
        assert!(dir.join(name).exists(), "{name}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "{name}");
        assert_eq!(shapecraft_doc::project::load(&kernel, &dir).unwrap().bodies.len(), 2);
    }

    // The name comes from a file, so it can't point anywhere else.
    let json = std::fs::read_to_string(dir.join("project.json")).unwrap();
    std::fs::write(dir.join("project.json"), json.replace("\"bodies_file\": \"bodies.step\"", "\"bodies_file\": \"../secret.step\""))
        .unwrap();
    assert!(shapecraft_doc::project::load(&kernel, &dir).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn appearances_apply_undo_and_save() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 3})).unwrap();
    let id = s.doc.bodies[0].id.0;
    s.execute("set_appearance", &json!({"items": [id], "appearance": "brass"})).unwrap();
    assert_eq!(s.doc.bodies[0].appearance.as_ref().unwrap().name, "Brass");
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.bodies[0].appearance.is_none());
    s.execute(
        "set_appearance",
        &json!({"items": [id], "appearance": {"name": "Mine", "color": [2.0, 0.5, -1.0], "metal": 0.5, "gloss": 0.5}}),
    )
    .unwrap();
    assert_eq!(s.doc.bodies[0].appearance.as_ref().unwrap().color, [1.0, 0.5, 0.0], "clamped");
    assert!(s.execute("set_appearance", &json!({"items": [id], "appearance": "unobtainium"})).is_err());
    let dir = std::env::temp_dir().join(format!("shapecraft-look-{}", std::process::id()));
    s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &dir).unwrap();
    assert_eq!(doc.bodies[0].appearance.as_ref().unwrap().name, "Mine");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reference_images_insert_resize_save_and_undo() {
    let dir = std::env::temp_dir().join(format!("shapecraft-img-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A tiny 4×2 PNG.
    let pic = dir.join("pic.png");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&pic, &png).unwrap();
    let mut s = session();
    let out = s.execute("insert_image", &json!({"path": pic, "plane": "xz", "width": 40})).unwrap();
    let id = out.value["image"].as_u64().unwrap();
    assert_eq!(s.doc.images[0].size, glam::DVec2::new(40.0, 20.0));
    assert_eq!(s.selection, vec![Pick::Image { image: shapecraft_doc::ItemId(id) }]);
    s.execute("image_settings", &json!({"image": id, "width": 10, "opacity": 0.5})).unwrap();
    assert_eq!(s.doc.images[0].size, glam::DVec2::new(10.0, 5.0));
    assert!(s.execute("insert_image", &json!({"path": dir.join("missing.png")})).is_err());
    assert!(s.execute("image_settings", &json!({"image": id, "width": -1})).is_err());
    let proj = dir.join("proj");
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert_eq!(doc.images.len(), 1);
    assert_eq!(doc.images[0].pixels.0, 4);
    assert_eq!(doc.images[0].opacity, 0.5);
    s.execute("delete", &json!({})).unwrap();
    assert!(s.doc.images.is_empty());
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.images.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_views_are_kept_with_the_project() {
    let dir = std::env::temp_dir().join(format!("shapecraft-views-{}", std::process::id()));
    let mut s = session();
    let view = json!({"target": [1, 2, 3], "yaw": 0.5, "pitch": 0.25, "distance": 80, "ortho": 1, "roll": 0});
    s.execute("save_view", &json!({"slot": 3, "view": view})).unwrap();
    assert!(s.doc.saved_views[2].is_some());
    assert!(s.execute("save_view", &json!({"slot": 9, "view": view})).is_err());
    assert!(s.execute("save_view", &json!({"slot": 1, "view": {"target": [0, 0, 0], "yaw": 0, "pitch": 0, "distance": -1}})).is_err());
    s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &dir).unwrap();
    assert_eq!(doc.saved_views[2].map(|v| v.distance), Some(80.0));
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.saved_views[2].is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn folders_hold_items_hide_them_and_go_with_them() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    let folder = s.execute("new_folder", &json!({})).unwrap().value["folder"].as_u64().unwrap();
    assert_eq!(s.doc.folders[0].name, "Folder 01");
    s.execute("move_to_folder", &json!({"items": [body], "folder": folder})).unwrap();
    assert_eq!(s.doc.folder_of(shapecraft_doc::ItemId(body)), Some(shapecraft_doc::ItemId(folder)));
    s.execute("show_hide", &json!({"items": [folder]})).unwrap();
    assert!(!s.doc.bodies[0].visible, "hiding the folder hides what's in it");
    s.execute("rename", &json!({"item": folder, "name": "Parts"})).unwrap();
    assert_eq!(s.doc.folders[0].name, "Parts");
    assert!(s.doc.history.iter().filter(|h| h.kind.contains("folder")).all(|h| h.quiet), "folders are bookkeeping");
    // Nested: a folder inside the first, never the first inside its own child.
    let inner = s.execute("new_folder", &json!({"parent": folder})).unwrap().value["folder"].as_u64().unwrap();
    assert_eq!(s.doc.folder_of(shapecraft_doc::ItemId(inner)), Some(shapecraft_doc::ItemId(folder)));
    assert!(s.execute("move_to_folder", &json!({"items": [folder], "folder": inner})).is_err());
    assert!(s.execute("place_item", &json!({"item": folder, "folder": inner})).is_err());
    s.execute("place_item", &json!({"item": body, "folder": inner})).unwrap();
    s.execute("show_hide", &json!({"items": [folder]})).unwrap();
    assert!(s.doc.bodies[0].visible, "showing the outer folder reaches inside the inner one");
    // Placed between two rows: its key falls between theirs.
    s.execute("place_item", &json!({"item": inner, "folder": null, "after": body, "before": folder})).unwrap();
    let k = |i: u64| s.doc.item_key(shapecraft_doc::ItemId(i));
    assert!(k(inner) > k(body).min(k(folder)) && k(inner) < k(body).max(k(folder)));
    s.execute("delete", &json!({"items": [folder]})).unwrap();
    s.execute("delete", &json!({"items": [inner]})).unwrap();
    assert!(s.doc.bodies.is_empty() && s.doc.folders.is_empty(), "a folder goes with what's in it");
}

#[test]
fn decals_place_move_save_and_stay_out_of_history() {
    let dir = std::env::temp_dir().join(format!("shapecraft-decal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("logo.png");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 20, 30, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&pic, &png).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 3})).unwrap();
    let steps = s.doc.history.len();
    let body = s.doc.bodies[0].id.0;
    let top = face_facing(&s, glam::DVec3::Z);
    let image = s.execute("import_decal", &json!({"path": pic})).unwrap().value["image"].as_u64().unwrap();
    assert_eq!(s.doc.decal_images[0].name, "logo");
    let out = s.execute("place_decal", &json!({"image": image, "body": body, "face": top, "point": [1, 2, 3]})).unwrap();
    let decal = out.value["decal"].as_u64().unwrap();
    let d = &s.doc.decals[0];
    assert!((d.normal - glam::DVec3::Z).length() < 1e-9, "the face's normal");
    assert!((d.up - glam::DVec3::Y).length() < 1e-9, "world Y on a face that looks up");
    assert!((d.size - 20.0).abs() < 1e-9, "1.0x is the body's size");
    assert_eq!(d.extent(s.doc.decal_images[0].aspect()), (20.0, 10.0));
    // Hostile values are clamped or refused.
    s.execute(
        "update_decal",
        &json!({"decal": decal, "scale": 50, "rotate": 30, "opacity": 0.5, "projection": "cylindrical", "extend": true}),
    )
    .unwrap();
    let d = &s.doc.decals[0];
    assert_eq!((d.scale, d.rotate, d.opacity, d.projection, d.extend), (10.0, 30.0, 0.5, shapecraft_doc::Projection::Cylindrical, true));
    assert!(s.execute("update_decal", &json!({"decal": decal, "projection": "spherical"})).is_err());
    assert!(s.execute("place_decal", &json!({"image": image, "body": body, "face": 999, "point": [0, 0, 3]})).is_err());
    assert!(s.execute("place_decal", &json!({"image": 12345, "body": body, "face": top, "point": [0, 0, 3]})).is_err());
    assert!(s.execute("place_decal", &json!({"image": image, "body": body, "face": top, "point": [0, 0, f64::NAN]})).is_err());
    // Moving to the side keeps up as near as it can.
    let side = face_facing(&s, glam::DVec3::X);
    s.execute("update_decal", &json!({"decal": decal, "body": body, "face": side, "point": [5, 0, 1.5]})).unwrap();
    let d = &s.doc.decals[0];
    assert_eq!(d.face, side);
    assert!((d.normal - glam::DVec3::X).length() < 1e-9 && (d.up - glam::DVec3::Z).length() < 1e-9);
    // Environment and camera.
    s.execute(
        "visual_settings",
        &json!({"environment": 2, "light_rotation": 400, "ground": "xy", "background": [1, 0.5, 0], "aperture": 2, "focus": [0, 0, 3]}),
    )
    .unwrap();
    let v = &s.doc.visual;
    assert_eq!(
        (v.environment, v.light_rotation, v.ground, v.background(2), v.aperture),
        (2, 180.0, shapecraft_doc::Ground::Xy, Some([1.0, 0.5, 0.0]), 2.0)
    );
    assert!(s.execute("visual_settings", &json!({"ground": "moon"})).is_err());
    s.execute("visual_settings", &json!({"focus": null})).unwrap();
    assert!(s.doc.visual.focus.is_none());
    assert_eq!(s.doc.history.len(), steps, "Visualization changes aren't history steps");
    // Saved and loaded.
    let proj = dir.join("proj");
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert_eq!((doc.decal_images.len(), doc.decals.len()), (1, 1));
    assert_eq!(doc.decal_images[0].pixels.0, 4);
    assert_eq!(doc.decals[0], s.doc.decals[0]);
    assert_eq!(doc.visual, s.doc.visual);
    // They survive the model being rebuilt from its history.
    let d = s.doc.history.len() - 1;
    s.execute("edit_step", &json!({"step": d, "params": {"distance": 4}})).unwrap();
    assert_eq!(s.doc.decals.len(), 1);
    assert_eq!(s.doc.visual.environment, 2);
    // Undo, delete, and the library picture taking its decals with it.
    s.execute("delete_decal", &json!({"decal": decal})).unwrap();
    assert!(s.doc.decals.is_empty());
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.decals.len(), 1);
    s.execute("delete_decal_image", &json!({"image": image})).unwrap();
    assert!(s.doc.decals.is_empty() && s.doc.decal_images.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_decal_on_a_cylinder_knows_its_axis() {
    let dir = std::env::temp_dir().join(format!("shapecraft-decal-cyl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("tag.png");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&pic, &png).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 20})).unwrap();
    let body = s.doc.bodies[0].id.0;
    let image = s.execute("import_decal", &json!({"path": pic})).unwrap().value["image"].as_u64().unwrap();
    let wall = s.doc.bodies[0]
        .mesh
        .triangle_faces
        .iter()
        .copied()
        .find(|f| s.kernel.face_plane(&s.doc.bodies[0].shape, *f).ok().flatten().is_none())
        .unwrap();
    s.execute("place_decal", &json!({"image": image, "body": body, "face": wall, "point": [5, 0, 10]})).unwrap();
    let d = &s.doc.decals[0];
    let (foot, r) = d.axis.unwrap();
    assert!((r - 5.0).abs() < 1e-6 && foot.truncate().length() < 1e-6 && (foot.z - 10.0).abs() < 1e-6);
    assert!((d.normal - glam::DVec3::X).length() < 1e-6, "out from the axis");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn custom_materials_from_a_picture() {
    let dir = std::env::temp_dir().join(format!("shapecraft-material-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("walnut.png");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([100, 60, 20, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&pic, &png).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [10, 20]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 3})).unwrap();
    let body = s.doc.bodies[0].id.0;
    let image = s.execute("import_material", &json!({"path": pic})).unwrap().value["image"].as_u64().unwrap();
    assert!(s.doc.decal_images[0].material && s.doc.decal_images[0].name == "walnut");
    // A material picture isn't a decal.
    let top = face_facing(&s, glam::DVec3::Z);
    assert!(s.execute("place_decal", &json!({"image": image, "body": body, "face": top, "point": [0, 0, 3]})).is_err());
    s.execute("set_appearance", &json!({"items": [body], "custom": image})).unwrap();
    let look = s.doc.bodies[0].appearance.clone().unwrap();
    assert_eq!((look.texture, look.name.as_str(), look.finish.as_str()), (Some(shapecraft_doc::ItemId(image)), "walnut", "Standard"));
    assert!((look.color[0] - 100.0 / 255.0).abs() < 1e-3, "the picture's average color");
    assert!(s.execute("set_appearance", &json!({"items": [body], "custom": 999_999})).is_err());
    assert!(
        s.execute(
            "set_appearance",
            &json!({"items": [body], "appearance": {"name": "x", "color": [1, 1, 1], "metal": 0, "gloss": 0, "texture": 999_999}})
        )
        .is_err()
    );
    s.execute("rename_decal_image", &json!({"image": image, "name": "Walnut Board"})).unwrap();
    // Saved and loaded with the flag and the picture.
    let proj = dir.join("proj");
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert!(doc.decal_images[0].material && doc.decal_images[0].pixels.0 == 4);
    assert_eq!(doc.bodies[0].appearance.as_ref().and_then(|a| a.texture), Some(shapecraft_doc::ItemId(image)));
    // Deleting the material puts its bodies back to grey.
    s.execute("delete_decal_image", &json!({"image": image})).unwrap();
    assert!(s.doc.bodies[0].appearance.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A project whose bodies file is unreadable still opens: the rest loads, the History makes the bodies again,
/// and the broken file is kept beside it.
#[test]
fn an_unreadable_bodies_file_is_rebuilt_from_the_history() {
    let dir = std::env::temp_dir().join(format!("sc-recover-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = session();
    s.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &serde_json::json!({"corner": [0, 0], "opposite": [4, 5]})).unwrap();
    s.execute("extrude", &serde_json::json!({"sketch": 1, "region": 0, "distance": 3})).unwrap();
    let id = s.doc.bodies[0].id;
    shapecraft_doc::project::save(&s.doc, s.kernel.as_ref(), &dir).unwrap();
    let step = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "step")).unwrap();
    std::fs::write(&step, b"ISO-10303-21;\nthis is not a step file\n").unwrap();
    assert!(shapecraft_doc::project::load(s.kernel.as_ref(), &dir).is_err());

    let doc = shapecraft_doc::project::load_without_bodies(s.kernel.as_ref(), &dir).unwrap();
    let mut back = crate::Session::with_document(s.kernel.clone(), doc, Some(dir.clone()));
    assert_eq!(back.rebuild_bodies().unwrap(), 0, "no step failed");
    assert_eq!(back.doc.bodies.len(), 1);
    assert_eq!(back.doc.bodies[0].id, id, "the body keeps its id");
    let v = back.kernel.mass_props(&back.doc.bodies[0].shape).unwrap().volume;
    assert!((v - 60.0).abs() < 1e-6, "{v}");
    assert!(dir.join(shapecraft_doc::project::UNREADABLE_BODIES_FILE).exists());
    // Saved again, it opens normally.
    shapecraft_doc::project::save(&back.doc, back.kernel.as_ref(), &dir).unwrap();
    assert_eq!(shapecraft_doc::project::load(back.kernel.as_ref(), &dir).unwrap().bodies.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copies_of_a_body_save_and_reopen() {
    for (n, by, linked) in
        [(0, [0.0, 0.0, 0.0], false), (1, [6.0, 0.0, 0.0], false), (2, [6.0, 0.0, 0.0], true), (3, [0.0, 0.0, 0.0], true)]
    {
        let dir = std::env::temp_dir().join(format!("shapecraft-copies-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = session();
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"center": [0, 0], "size": [4, 4]})).unwrap();
        let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
        let body = out.value["body"].as_u64().unwrap();
        s.execute("translate", &json!({"items": [body], "by": by, "copy": true, "linked": linked})).unwrap();
        assert_eq!(s.doc.bodies.len(), 2, "case {n}");
        s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
        let doc = shapecraft_doc::project::load(&OcctKernel::new(), &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(doc.map(|d| d.bodies.len()).map_err(|e| e.to_string()), Ok(2), "case {n}");
    }
}

#[test]
fn a_missing_bodies_file_says_so_and_the_history_can_rebuild_it() {
    let dir = std::env::temp_dir().join(format!("sc-missing-bodies-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [4, 5]})).unwrap();
    s.execute("extrude", &json!({"sketch": 1, "region": 0, "distance": 3})).unwrap();
    shapecraft_doc::project::save(&s.doc, s.kernel.as_ref(), &dir).unwrap();
    // A copy that brought project.json without its bodies file, long ago (a sync that just did is waited for).
    for e in std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "step")) {
        std::fs::remove_file(e.path()).unwrap();
    }
    assert!(matches!(shapecraft_doc::project::load(s.kernel.as_ref(), &dir), Err(shapecraft_doc::project::ProjectError::StillSyncing)));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    std::fs::File::options().write(true).open(dir.join("project.json")).unwrap().set_modified(old).unwrap();
    match shapecraft_doc::project::load(s.kernel.as_ref(), &dir) {
        Err(shapecraft_doc::project::ProjectError::Bodies(m)) => assert!(m.contains("missing"), "{m}"),
        other => panic!("{:?}", other.map(|d| d.bodies.len())),
    }
    let doc = shapecraft_doc::project::load_without_bodies(s.kernel.as_ref(), &dir).unwrap();
    let mut back = crate::Session::with_document(s.kernel.clone(), doc, Some(dir.clone()));
    assert_eq!(back.rebuild_bodies().unwrap(), 0);
    assert_eq!(back.doc.bodies.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

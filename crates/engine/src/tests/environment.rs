//! Custom environments: panoramas imported into Visualization, selected, saved, undone and deleted, and hostile files.

use serde_json::json;
use shapecraft_doc::{BUILTIN_ENVIRONMENTS, HdrPixels};
use shapecraft_kernel_occt::OcctKernel;

use super::session;
use crate::Session;

/// A Radiance .hdr file (flat RGBE scanlines) from a pixel function.
pub(crate) fn hdr_file(w: u32, h: u32, px: impl Fn(u32, u32) -> [f32; 3]) -> Vec<u8> {
    let mut out = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {h} +X {w}\n").into_bytes();
    for y in 0..h {
        for x in 0..w {
            let c = px(x, y);
            let m = c[0].max(c[1]).max(c[2]);
            if m < 1e-32 {
                out.extend([0, 0, 0, 0]);
                continue;
            }
            let e = m.log2().floor() as i32 + 1;
            let scale = 256.0 / 2f32.powi(e);
            out.extend([(c[0] * scale) as u8, (c[1] * scale) as u8, (c[2] * scale) as u8, (e + 128) as u8]);
        }
    }
    out
}

fn png_file(w: u32, h: u32) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 90, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

fn block(s: &mut Session) {
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
}

#[test]
fn panoramas_import_select_save_undo_and_delete() {
    let dir = std::env::temp_dir().join(format!("shapecraft-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // A sky with a hot sun (beyond what half floats hold: clamped) over a dark floor.
    let hdr = dir.join("courtyard.hdr");
    std::fs::write(
        &hdr,
        hdr_file(64, 32, |x, y| match (x, y) {
            (10, 5) => [1e30, 1e30, 1e30],
            (_, y) if y < 16 => [0.4, 0.6, 1.2],
            _ => [0.05, 0.04, 0.03],
        }),
    )
    .unwrap();
    let png = dir.join("street.png");
    std::fs::write(&png, png_file(40, 20)).unwrap();
    let odd = dir.join("square.png");
    std::fs::write(&odd, png_file(16, 16)).unwrap();

    let mut s = session();
    block(&mut s);
    let steps = s.doc.history.len();
    let out = s.execute("import_environment", &json!({"path": hdr})).unwrap().value;
    assert_eq!(out["environment"], json!(BUILTIN_ENVIRONMENTS));
    assert!(out["note"].is_null(), "2:1 needs no note");
    let v = &s.doc.visual;
    assert_eq!(v.environment, BUILTIN_ENVIRONMENTS, "the imported panorama is selected");
    let p = v.panorama().unwrap();
    assert_eq!((p.name.as_str(), p.ext.as_str(), p.pixels.width, p.pixels.height, p.pixels.hdr), ("courtyard", "hdr", 64, 32, true));
    assert!(p.pixels.rgb.iter().flatten().all(|c| c.is_finite() && (0.0..=HdrPixels::MAX).contains(c)));
    assert!(p.pixels.rgb.iter().flatten().any(|c| *c == HdrPixels::MAX), "the sun is clamped, not lost");
    assert!((p.pixels.rgb[64 * 2][2] - 1.2).abs() < 0.02, "linear values as written");

    let out = s.execute("import_environment", &json!({"path": png})).unwrap().value;
    assert_eq!(out["environment"], json!(BUILTIN_ENVIRONMENTS + 1));
    let pixels = &s.doc.visual.panorama().unwrap().pixels;
    assert!(!pixels.hdr && pixels.rgb.iter().flatten().all(|c| (0.0..=1.0).contains(c)), "LDR is linearized 0..1");
    let out = s.execute("import_environment", &json!({"path": odd})).unwrap().value;
    assert!(out["note"].as_str().unwrap().contains("2:1"), "a square panorama is taken, with a note");
    assert_eq!(s.doc.history.len(), steps, "Visualization changes aren't history steps");

    // Selecting by its place in the list; past the end is refused.
    s.execute("visual_settings", &json!({"environment": BUILTIN_ENVIRONMENTS})).unwrap();
    assert_eq!(s.doc.visual.panorama().unwrap().name, "courtyard");
    assert!(s.execute("visual_settings", &json!({"environment": BUILTIN_ENVIRONMENTS + 3})).is_err());
    assert!(s.execute("visual_settings", &json!({"environment": 9.5})).is_err());
    s.execute("rename_environment", &json!({"environment": BUILTIN_ENVIRONMENTS + 2, "name": "Square"})).unwrap();

    // Saved beside the project and read back.
    let proj = dir.join("proj");
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let id = s.doc.visual.panoramas[0].id.0;
    assert!(proj.join("images").join(format!("{id}.hdr")).is_file());
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert_eq!(doc.visual, s.doc.visual);
    assert_eq!(doc.visual.panoramas[0].pixels, s.doc.visual.panoramas[0].pixels);
    assert_eq!(doc.visual.panoramas[2].name, "Square");
    // A lost file drops that environment; the selected one stays selected.
    std::fs::remove_file(proj.join("images").join(format!("{}.png", s.doc.visual.panoramas[1].id.0))).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert_eq!(doc.visual.panoramas.len(), 2);
    assert_eq!(doc.visual.panorama().unwrap().name, "courtyard");

    // They survive the model being rebuilt from its history.
    let d = s.doc.history.len() - 1;
    s.execute("edit_step", &json!({"step": d, "params": {"distance": 4}})).unwrap();
    assert_eq!((s.doc.visual.panoramas.len(), s.doc.visual.environment), (3, BUILTIN_ENVIRONMENTS));

    // Deleting the selected one goes back to Default; one after the selection moves the selection with it.
    s.execute("visual_settings", &json!({"environment": BUILTIN_ENVIRONMENTS + 2})).unwrap();
    s.execute("delete_environment", &json!({"environment": BUILTIN_ENVIRONMENTS})).unwrap();
    assert_eq!(s.doc.visual.panorama().unwrap().name, "Square");
    s.execute("delete_environment", &json!({"id": s.doc.visual.panorama().unwrap().id.0})).unwrap();
    assert_eq!((s.doc.visual.environment, s.doc.visual.panoramas.len()), (0, 1));
    assert!(s.execute("delete_environment", &json!({"environment": 3})).is_err(), "built-ins stay");
    // Undo brings them back, selection and all.
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.visual.panorama().unwrap().name, "Square");
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.visual.panoramas.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undo_takes_an_import_back() {
    let dir = std::env::temp_dir().join(format!("shapecraft-env-undo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let hdr = dir.join("sky.hdr");
    std::fs::write(&hdr, hdr_file(8, 4, |_, y| if y < 2 { [2.0, 2.0, 2.0] } else { [0.1, 0.1, 0.1] })).unwrap();
    let mut s = session();
    s.execute("visual_settings", &json!({"environment": 3})).unwrap();
    s.execute("import_environment", &json!({"path": hdr})).unwrap();
    assert_eq!(s.doc.visual.environment, BUILTIN_ENVIRONMENTS);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.visual.panoramas.is_empty());
    assert_eq!(s.doc.visual.environment, 3);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.doc.visual.panoramas.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hostile_panoramas_are_refused() {
    let dir = std::env::temp_dir().join(format!("shapecraft-env-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = session();
    let cases: [(&str, Vec<u8>); 7] = [
        ("garbage.hdr", b"this is not a picture at all".to_vec()),
        ("empty.hdr", Vec::new()),
        ("huge.hdr", b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 50000 +X 100000\n".to_vec()),
        ("wide.hdr", b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 10 +X 9000\n".to_vec()),
        ("zero.hdr", b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 0 +X 0\n".to_vec()),
        ("cut.hdr", hdr_file(64, 32, |_, _| [1.0, 1.0, 1.0])[..300].to_vec()),
        ("cut.png", png_file(64, 32)[..60].to_vec()),
    ];
    for (name, bytes) in cases {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        assert!(s.execute("import_environment", &json!({"path": path})).is_err(), "{name} is refused");
    }
    assert!(s.execute("import_environment", &json!({"path": dir.join("missing.hdr")})).is_err());
    assert!(s.execute("import_environment", &json!({})).is_err());
    assert!(s.doc.visual.panoramas.is_empty());
    // A project file naming a panorama whose file is garbage loads without it.
    let proj = dir.join("proj");
    let hdr = dir.join("ok.hdr");
    std::fs::write(&hdr, hdr_file(8, 4, |_, _| [0.5, 0.5, 0.5])).unwrap();
    s.execute("import_environment", &json!({"path": hdr})).unwrap();
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let id = s.doc.visual.panoramas[0].id.0;
    std::fs::write(proj.join("images").join(format!("{id}.hdr")), b"garbage").unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert!(doc.visual.panoramas.is_empty());
    assert_eq!(doc.visual.environment, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

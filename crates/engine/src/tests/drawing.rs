//! Drawings: sheets, views, annotations, pictures and drawing export.

use serde_json::json;
use shapecraft_kernel_occt::OcctKernel;

use super::session;
use crate::Session;

#[test]
fn export_a_drawing() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [80, 40]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 8})).unwrap();
    let sk = s.active_sketch.unwrap();
    let ring = s.active_sketch().unwrap().regions().iter().max_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
    s.execute("extrude", &json!({"sketch": sk.0, "region": ring, "distance": 10})).unwrap();
    let dir = std::env::temp_dir().join(format!("shapecraft-dwg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("part.svg");
    let out = s.execute("export_drawing", &json!({"path": svg.to_string_lossy()})).unwrap().value;
    assert_eq!(out["scale"], "1:1");
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains("stroke-dasharray") && text.contains(">TITLE<") && text.contains(">1:1<"));
    // A saved drawing: one front view, A4 portrait at 2:1, then exported as PDF and DXF.
    let body = s.doc.bodies[0].id.0;
    let d =
        s.execute("create_drawing", &json!({"bodies": [body], "sheet": "A4", "landscape": false, "scale": 2.0})).unwrap().value["drawing"]
            .as_u64()
            .unwrap();
    assert_eq!(s.doc.drawings[0].name, "Body 01");
    s.execute("update_drawing", &json!({"drawing": d, "four_views": true, "first_angle": false})).unwrap();
    assert!(s.doc.drawings[0].four_views && !s.doc.drawings[0].first_angle);
    let pdf = dir.join("part.pdf");
    s.execute("export_drawing", &json!({"path": pdf.to_string_lossy(), "drawing": d})).unwrap();
    let bytes = std::fs::read(&pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4") && bytes.ends_with(b"%%EOF\n"));
    let dxf = dir.join("part.dxf");
    s.execute("export_drawing", &json!({"path": dxf.to_string_lossy(), "drawing": d})).unwrap();
    assert!(std::fs::read_to_string(&dxf).unwrap().contains("HIDDEN"));
    assert!(s.execute("export_drawing", &json!({"path": dir.join("x.dwf").to_string_lossy()})).is_err());
    // DWG (R2000): read back structurally – every CRC, the layers, and one entity per view line, frame line and text.
    let dwg = dir.join("part.dwg");
    s.execute("export_drawing", &json!({"path": dwg.to_string_lossy(), "drawing": d})).unwrap();
    let bytes = std::fs::read(&dwg).unwrap();
    if let Ok(keep) = std::env::var("SHAPECRAFT_DWG_OUT") {
        let _ = std::fs::write(std::path::Path::new(&keep).join("engine-part.dwg"), &bytes);
        let _ = std::fs::copy(&dxf, std::path::Path::new(&keep).join("engine-part.dxf"));
    }
    let f = shapecraft_dwg::inspect(&bytes).unwrap();
    let layers: Vec<_> = f.objects.iter().filter(|o| o.kind == "LAYER").filter_map(|o| o.name.clone()).collect();
    assert_eq!(layers, ["0", "VISIBLE", "HIDDEN", "FRAME"]);
    let spec = s.doc.drawing(shapecraft_doc::ItemId(d)).unwrap().clone();
    let sheet = crate::drawing::make(&s, &spec).unwrap();
    let view_lines = sheet.views.iter().flat_map(|v| &v.lines).filter(|l| l.points.len() >= 2).count();
    let hidden = sheet.views.iter().flat_map(|v| &v.lines).filter(|l| l.points.len() >= 2 && l.hidden).count();
    assert!(hidden > 0, "four views of a ring show hidden lines");
    let on = |layer: &str| f.entities().filter(|o| o.layer.as_deref() == Some(layer) && o.kind != "BLOCK" && o.kind != "ENDBLK").count();
    assert_eq!(on("VISIBLE") + on("HIDDEN"), view_lines);
    assert_eq!(on("HIDDEN"), hidden);
    let texts: Vec<_> = f.objects.iter().filter(|o| o.kind == "TEXT").filter_map(|o| o.text.clone()).collect();
    assert!(texts.iter().any(|t| t == "TITLE") && texts.iter().any(|t| t == "2:1"), "{texts:?}");
    let frame_lines = sheet.marks().iter().filter(|m| matches!(m, crate::drawing::Mark::Line { .. })).count()
        - sheet.views.iter().flat_map(|v| &v.lines).map(|l| l.points.len().saturating_sub(1)).sum::<usize>();
    assert_eq!(f.entities().filter(|o| o.kind == "LINE" && o.layer.as_deref() == Some("FRAME")).count(), frame_lines);
    assert!(s.execute("update_drawing", &json!({"drawing": d, "sheet": "B7"})).is_err());
    // Line widths from the Properties sidebar reach the sheet.
    let mut style = shapecraft_doc::DrawingStyle { visible: 0.7, ..Default::default() };
    s.execute("update_drawing", &json!({"drawing": d, "style": style})).unwrap();
    let spec = s.doc.drawing(shapecraft_doc::ItemId(d)).unwrap().clone();
    let sheet = crate::drawing::make(&s, &spec).unwrap();
    assert!(
        sheet.marks().iter().any(|m| matches!(m, crate::drawing::Mark::Line { width, dashed: false, .. } if (*width - 0.7).abs() < 1e-9))
    );
    style.hidden = -1.0;
    assert!(s.execute("update_drawing", &json!({"drawing": d, "style": style})).is_err());
    s.execute("delete_drawing", &json!({"drawing": d})).unwrap();
    assert!(s.doc.drawings.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn drawing_annotations_measure_the_model() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [40, 20]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 10})).unwrap();
    let d = s.execute("create_drawing", &json!({"four_views": true})).unwrap().value["drawing"].as_u64().unwrap();
    let id = shapecraft_doc::ItemId(d);
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    // The front view's bottom corners, picked on the sheet as the UI does.
    let front = sheet.views.iter().find(|v| v.name == "Front").unwrap();
    let pts: Vec<glam::DVec2> = front.lines.iter().flat_map(|l| l.points.clone()).collect();
    let lo = pts.iter().fold(glam::DVec2::splat(f64::INFINITY), |m, p| m.min(*p));
    let hi = pts.iter().fold(glam::DVec2::splat(f64::NEG_INFINITY), |m, p| m.max(*p));
    let (_, a) = sheet.sheet_to_view(lo).unwrap();
    let (_, b) = sheet.sheet_to_view(glam::DVec2::new(hi.x, lo.y)).unwrap();
    s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "distance", "view": "Front", "a": a, "b": b, "offset": -8.0}}))
        .unwrap();
    s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "note", "at": [30, 30], "text": "Deburr all edges"}})).unwrap();
    // Measured in model units whatever the sheet scale.
    s.execute("update_drawing", &json!({"drawing": d, "scale": 0.5})).unwrap();
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    let texts: Vec<String> = sheet
        .annotation_marks()
        .into_iter()
        .filter_map(|m| if let crate::drawing::Mark::Text { text, .. } = m { Some(text) } else { None })
        .collect();
    assert_eq!(texts, ["40.00", "Deburr all edges"]);
    assert!(sheet.to_svg().contains("40.00"));
    s.execute("delete_annotation", &json!({"drawing": d, "index": 1})).unwrap();
    assert_eq!(s.doc.drawing(id).unwrap().annotations.len(), 1);
    assert!(s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "note", "at": [0, 0], "text": "  "}})).is_err());
    // Angles: the smaller way round, decimal or degrees-minutes-seconds.
    s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "angle", "view": "Front", "vertex": [0, 0], "a": [10, 0], "b": [0, 10], "radius": 12.0}})).unwrap();
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    let last = sheet.annotations.last().unwrap();
    let text = sheet
        .marks_for(last)
        .into_iter()
        .find_map(|m| if let crate::drawing::Mark::Text { text, .. } = m { Some(text) } else { None })
        .unwrap();
    assert_eq!(text, "90.00°");
    assert_eq!(sheet.format_angle(30.5), "30.50°");
    let style = shapecraft_doc::DrawingStyle { angle_format: "Degrees, Minutes, Seconds".into(), ..Default::default() };
    s.execute("update_drawing", &json!({"drawing": d, "style": style})).unwrap();
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    assert_eq!(sheet.format_angle(30.5), "30°30′00″");
}

#[test]
fn drawing_views_base_projected_section_detail() {
    use crate::drawing::Mark;
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [80, 40]})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 8})).unwrap();
    let sk = s.active_sketch.unwrap();
    let ring = s.active_sketch().unwrap().regions().iter().max_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
    s.execute("extrude", &json!({"sketch": sk.0, "region": ring, "distance": 10})).unwrap();
    let d = s.execute("create_drawing", &json!({"scale": 1.0})).unwrap().value["drawing"].as_u64().unwrap();
    let id = shapecraft_doc::ItemId(d);
    assert_eq!(s.doc.drawing(id).unwrap().views.as_ref().map(Vec::len), Some(0), "an empty sheet waits for its base view");
    assert!(crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap().views.is_empty());
    let add =
        |s: &mut Session, v: serde_json::Value| s.execute("add_drawing_view", &v).map(|o| o.value["view"].as_str().unwrap().to_string());
    assert_eq!(add(&mut s, json!({"drawing": d, "kind": "base", "orientation": "front", "at": [100, 200]})).unwrap(), "Front");
    // First angle: the view to the right of the front shows its left side, level with it.
    assert_eq!(
        add(&mut s, json!({"drawing": d, "kind": "projected", "parent": "Front", "dx": 1, "dy": 0, "at": [250, 150]})).unwrap(),
        "Left"
    );
    assert!(add(&mut s, json!({"drawing": d, "kind": "projected", "parent": "Front", "dx": 1, "dy": 0})).is_err());
    assert!(add(&mut s, json!({"drawing": d, "kind": "projected", "parent": "Nope", "dx": 1, "dy": 0})).is_err());
    assert!(add(&mut s, json!({"drawing": d, "kind": "projected", "parent": "Front", "dx": 0, "dy": 0})).is_err());
    assert_eq!(add(&mut s, json!({"drawing": d, "kind": "base", "orientation": "top", "at": [100, 80]})).unwrap(), "Top");
    // Cut lengthwise through the hole, seen from the front.
    let sec =
        add(&mut s, json!({"drawing": d, "kind": "section", "parent": "Top", "a": [-60, 0], "b": [60, 0], "flip": true, "at": [300, 60]}))
            .unwrap();
    assert_eq!(sec, "Section A-A");
    let det =
        add(&mut s, json!({"drawing": d, "kind": "detail", "parent": "Front", "center": [40, 5], "radius": 6, "at": [330, 220]})).unwrap();
    assert_eq!(det, "Detail B");
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    let view = |n: &str| sheet.views.iter().find(|v| v.name == n).unwrap();
    assert_eq!(view("Left").at.y, view("Front").at.y, "projected views stay level with their parent");
    assert!((view("Left").hi.x - view("Left").lo.x - 40.0).abs() < 0.5, "the side view is 40 wide");
    let s_view = view(&sec);
    assert!(!s_view.lines.is_empty() && s_view.lines.iter().all(|l| !l.hidden), "sections show no hidden lines");
    let hatching: Vec<(glam::DVec2, glam::DVec2)> = s_view
        .extra
        .iter()
        .filter_map(|m| {
            if let Mark::Line { a, b, width, dashed: false } = m { (*width == sheet.style.section).then_some((*a, *b)) } else { None }
        })
        .collect();
    assert!(hatching.len() > 10, "{}", hatching.len());
    // Two cut faces either side of the hole: no hatch line crosses it (80 wide, 10 high, the hole 16 across).
    assert!((s_view.hi.x - s_view.lo.x - 80.0).abs() < 0.5 && (s_view.hi.y - s_view.lo.y - 10.0).abs() < 0.5);
    let hole = s_view.at;
    assert!(hatching.iter().all(|(a, b)| a.x.max(b.x) < hole.x - 7.9 || a.x.min(b.x) > hole.x + 7.9), "hatching skips the hole");
    assert!(s_view.extra.iter().any(|m| matches!(m, Mark::Text { text, .. } if text == "A-A")));
    let dv = view(&det);
    assert_eq!(dv.scale, 2.0, "a detail is twice its parent");
    assert!(dv.lines.iter().flat_map(|l| l.points.iter()).all(|p| p.distance(dv.at) <= 12.0 + 1e-6));
    assert!(dv.extra.iter().any(|m| matches!(m, Mark::Text { text, .. } if text == "SCALE 2:1")));
    // Hidden lines off, a scale of its own, then moved.
    s.execute("update_drawing_view", &json!({"drawing": d, "view": "Front", "hidden_lines": false, "scale": 0.5, "at": [120, 210]}))
        .unwrap();
    let sheet = crate::drawing::make(&s, s.doc.drawing(id).unwrap()).unwrap();
    let front = sheet.views.iter().find(|v| v.name == "Front").unwrap();
    assert!(front.scale == 0.5 && front.lines.iter().all(|l| !l.hidden) && front.at == glam::DVec2::new(120.0, 210.0));
    assert_eq!(sheet.views.iter().find(|v| v.name == "Left").unwrap().scale, 0.5, "projections follow their parent's scale");
    assert!(sheet.to_svg().contains(">A-A<") && sheet.to_dxf().contains("A-A"));
    // Deleting the front takes its projection, its detail and its dimensions with it.
    s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "centerline", "view": "Front", "a": [0, -5], "b": [0, 5]}}))
        .unwrap();
    s.execute("annotate", &json!({"drawing": d, "annotation": {"kind": "note", "at": [30, 30], "text": "Keep"}})).unwrap();
    let gone = s.execute("delete_drawing_view", &json!({"drawing": d, "view": "Front"})).unwrap().value["removed"].clone();
    assert_eq!(gone, json!(["Front", "Left", "Detail B"]));
    let spec = s.doc.drawing(id).unwrap();
    assert_eq!(spec.views.as_ref().unwrap().iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Top", "Section A-A"]);
    assert_eq!(spec.annotations.len(), 1);
    assert!(s.execute("update_drawing_view", &json!({"drawing": d, "view": "Front", "at": [0, 0]})).is_err());
    assert!(s.execute("update_drawing_view", &json!({"drawing": d, "view": "Top", "at": [1e9, 0]})).is_err());
    assert!(add(&mut s, json!({"drawing": d, "kind": "detail", "parent": "Top", "center": [0, 0], "radius": -1})).is_err());
    assert!(add(&mut s, json!({"drawing": d, "kind": "base", "orientation": "sideways"})).is_err());
    // The next letter skips the one in use.
    assert_eq!(add(&mut s, json!({"drawing": d, "kind": "detail", "parent": "Top", "center": [0, 0], "radius": 10})).unwrap(), "Detail B");
    // Undo brings the front back.
    s.execute("undo", &json!({})).unwrap();
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.drawing(id).unwrap().views.as_ref().unwrap().iter().any(|v| v.name == "Front"));
}

#[test]
fn pictures_on_a_drawing_sheet() {
    use crate::drawing::Mark;
    let dir = std::env::temp_dir().join(format!("shapecraft-sheet-img-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("logo.png");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 200, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    std::fs::write(&pic, &png).unwrap();
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [20, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    let d = s.execute("create_drawing", &json!({"scale": 1.0, "four_views": true})).unwrap().value["drawing"].as_u64().unwrap();
    let out = s.execute("insert_sheet_image", &json!({"drawing": d, "path": pic, "at": [100, 100], "width": 40})).unwrap();
    let index = out.value["annotation"].as_u64().unwrap();
    assert!(s.doc.decal_images[0].sheet && !s.doc.decal_images[0].is_decal());
    let made = crate::drawing::make(&s, s.doc.drawing(shapecraft_doc::ItemId(d)).unwrap()).unwrap();
    let marks: Vec<(glam::DVec2, glam::DVec2)> =
        made.marks().into_iter().filter_map(|m| if let Mark::Image { min, max, .. } = m { Some((min, max)) } else { None }).collect();
    assert_eq!(marks, vec![(glam::DVec2::new(80.0, 90.0), glam::DVec2::new(120.0, 110.0))], "40 wide, its shape kept");
    assert!(made.to_svg().contains("data:image/png;base64,"));
    let pdf = made.to_pdf();
    assert!(pdf.windows(9).any(|w| w == b"/Im0 Do Q") && pdf.windows(6).any(|w| w == b"/SMask"));
    // Moved and resized; nonsense refused.
    s.execute("insert_sheet_image", &json!({"drawing": d, "index": index, "at": [50, 60], "width": 20})).unwrap();
    assert!(s.execute("insert_sheet_image", &json!({"drawing": d, "index": index, "at": [50, 60], "width": -5})).is_err());
    assert!(s.execute("insert_sheet_image", &json!({"drawing": d, "index": 99, "at": [50, 60]})).is_err());
    // Saved with the project.
    let proj = dir.join("proj");
    s.execute("save", &json!({"path": proj.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(&OcctKernel::new(), &proj).unwrap();
    assert!(doc.decal_images[0].sheet && doc.decal_images[0].pixels.0 == 4);
    let _ = std::fs::remove_dir_all(&dir);
}

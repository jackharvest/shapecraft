//! Drawings: sheets, their views, annotations and pictures, and drawing export.

use serde_json::{Value, json};
use shapecraft_doc::Pick;

use super::path_param;
use crate::cmd::CommandSpec;
use crate::{EngineError, Params, Result, Session};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "create_drawing",
            label: "Create Drawing",
            menu: "File/Create Drawing",
            shortcut: None,
            help: "A drawing sheet of the chosen bodies",
            params: r#"{"name": "…"?, "bodies": [ids]? (default: the selected bodies, else all visible), "sheet": "A4"|"A3"|"A2"|"A1"|"A0"|"Letter"|"Tabloid" (A3), "landscape": true, "scale": n|null (auto), "first_angle": true, "four_views": false}"#,
            modifies: true,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: create_drawing,
        },
        CommandSpec {
            id: "update_drawing",
            label: "Drawing Properties",
            menu: "",
            shortcut: None,
            help: "Change a drawing's sheet, scale, projection or views",
            params: r#"{"drawing": id, "name"?, "sheet"?, "landscape"?, "scale"? (null: auto), "first_angle"?, "four_views"?, "style"?: {"angle_format", "length_precision", "angle_precision", "decimal_separator", "visible", "hidden", "dimension", "centerline", "section", "detail" (mm), "title_block": "Simple"|"Horizontal"|…, "date_format": "MM/DD/YY"|…}}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: update_drawing,
        },
        CommandSpec {
            id: "annotate",
            label: "Annotate Drawing",
            menu: "",
            shortcut: None,
            help: "Add a dimension, centerline, center mark or note to a drawing",
            params: r#"{"drawing": id, "annotation": {"kind": "distance", "view": "Front", "a": [x,y], "b": [x,y], "offset": mm} | {"kind": "radius", "view", "center", "radius", "angle", "diameter"} | {"kind": "centerline", "view", "a", "b"} | {"kind": "center_mark", "view", "center", "radius"} | {"kind": "note", "at": [x,y] (sheet mm), "text"}}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: |s, p| {
                let id = p.id("drawing")?;
                let a: shapecraft_doc::Annotation = p.parse("annotation")?;
                let ok = match &a {
                    shapecraft_doc::Annotation::Distance { a, b, offset, .. } => a.distance(*b) > 1e-9 && offset.is_finite(),
                    shapecraft_doc::Annotation::Radius { radius, angle, .. } => *radius > 1e-9 && angle.is_finite(),
                    shapecraft_doc::Annotation::CenterMark { radius, .. }
                    | shapecraft_doc::Annotation::CircularCenterline { radius, .. } => *radius > 1e-9 && radius.is_finite(),
                    shapecraft_doc::Annotation::IntersectionMark { at, .. } => at.is_finite(),
                    shapecraft_doc::Annotation::Centerline { a, b, .. } => a.distance(*b) > 1e-9,
                    shapecraft_doc::Annotation::Angle { vertex, a, b, radius, .. } => {
                        vertex.distance(*a) > 1e-9 && vertex.distance(*b) > 1e-9 && *radius > 0.0 && radius.is_finite()
                    }
                    shapecraft_doc::Annotation::Note { text, .. } => !text.trim().is_empty() && text.chars().count() <= 2000,
                    shapecraft_doc::Annotation::Image { at, width, image } => {
                        at.is_finite()
                            && width.is_finite()
                            && *width > 0.5
                            && *width < 5000.0
                            && s.doc.decal_images.iter().any(|i| i.id == *image && i.sheet)
                    }
                };
                if !ok {
                    return Err(EngineError::BadParams("that annotation has no size".into()));
                }
                let d = s.doc.drawing_mut(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                d.annotations.push(a);
                Ok(json!({ "drawing": id.0, "annotation": d.annotations.len() - 1 }))
            },
        },
        CommandSpec {
            id: "update_annotation",
            label: "Dimension Text",
            menu: "",
            shortcut: None,
            help: "Set a dimension's prefix, suffix and tolerance",
            params: r#"{"drawing": id, "index": n, "text": {"prefix"?: "text", "suffix"?: "text", "tolerance"?: {"kind": "symmetrical"|"deviation"|"limits"|"basic", "upper": n, "lower": n}}}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: |s, p| {
                let id = p.id("drawing")?;
                let index = p.f64("index")?;
                let text: shapecraft_doc::DimText = p.parse("text")?;
                let long = |t: &Option<String>| t.as_ref().is_some_and(|t| t.chars().count() > 200);
                if long(&text.prefix) || long(&text.suffix) {
                    return Err(EngineError::BadParams("a prefix or suffix can have up to 200 characters".into()));
                }
                if let Some(t) = &text.tolerance
                    && !(t.upper.is_finite() && t.lower.is_finite() && t.upper.abs() < 1e6 && t.lower.abs() < 1e6)
                {
                    return Err(EngineError::BadParams("a tolerance needs finite limits".into()));
                }
                let d = s.doc.drawing_mut(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                let a = (index >= 0.0)
                    .then(|| d.annotations.get_mut(index as usize))
                    .flatten()
                    .ok_or_else(|| EngineError::BadParams(format!("no annotation {index}")))?;
                use shapecraft_doc::Annotation as A;
                match a {
                    A::Distance { text: t, .. } | A::Radius { text: t, .. } | A::Angle { text: t, .. } => *t = text,
                    _ => return Err(EngineError::NotAvailable("only dimensions take a prefix, suffix or tolerance".into())),
                }
                Ok(json!({ "drawing": id.0 }))
            },
        },
        CommandSpec {
            id: "insert_sheet_image",
            label: "Image",
            menu: "",
            shortcut: None,
            help: "Put a PNG or JPEG picture on a drawing sheet (Drawings ▸ Image), or move or resize one placed before",
            params: r#"{"drawing": id, "path": "/logo.png" (a new picture) | "index": n (one on the sheet), "at": [x,y]? (sheet mm, default the sheet's middle), "width": mm? (default a quarter of the sheet)}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: |s, p| {
                let id = p.id("drawing")?;
                let sheet = {
                    let d = s.doc.drawing(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                    let (w, h) = shapecraft_doc::DrawingSheet::paper(&d.sheet, d.landscape).unwrap_or((420.0, 297.0));
                    glam::DVec2::new(w, h)
                };
                let at = if p.has("at") { p.vec2("at")? } else { sheet / 2.0 };
                let width = p.f64_or("width", sheet.x / 4.0)?;
                if !(at.is_finite() && width.is_finite() && width > 0.5 && width < 5000.0) {
                    return Err(EngineError::BadParams("an image needs a finite spot and a width of 0.5–5000 mm".into()));
                }
                if p.has("index") {
                    let index = p.f64("index")?;
                    let d = s.doc.drawing_mut(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                    match (index >= 0.0).then(|| d.annotations.get_mut(index as usize)).flatten() {
                        Some(shapecraft_doc::Annotation::Image { at: a, width: w, .. }) => {
                            *a = at;
                            *w = width;
                        }
                        _ => return Err(EngineError::BadParams(format!("no image {index} on that sheet"))),
                    }
                    return Ok(json!({ "drawing": id.0, "annotation": index as usize }));
                }
                let made = crate::cmd::visual::import_picture(s, p.str("path")?, false)?;
                let image = shapecraft_doc::ItemId(made["image"].as_u64().unwrap_or(0));
                if let Some(pic) = s.doc.decal_images.iter_mut().find(|i| i.id == image) {
                    pic.sheet = true;
                }
                let d = s.doc.drawing_mut(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                d.annotations.push(shapecraft_doc::Annotation::Image { at, width, image });
                Ok(json!({ "drawing": id.0, "annotation": d.annotations.len() - 1, "image": image.0 }))
            },
        },
        CommandSpec {
            id: "delete_annotation",
            label: "Delete Annotation",
            menu: "",
            shortcut: None,
            help: "Remove an annotation from a drawing",
            params: r#"{"drawing": id, "index": n}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: |s, p| {
                let id = p.id("drawing")?;
                let index = p.f64("index")?;
                let d = s.doc.drawing_mut(id).ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
                if !(index >= 0.0 && (index as usize) < d.annotations.len()) {
                    return Err(EngineError::BadParams(format!("no annotation {index}")));
                }
                d.annotations.remove(index as usize);
                Ok(json!({ "drawing": id.0 }))
            },
        },
        CommandSpec {
            id: "add_drawing_view",
            label: "Add View",
            menu: "",
            shortcut: None,
            help: "Place a base, projected, section or detail view on a drawing",
            params: r#"{"drawing": id, "kind": "base"|"projected"|"section"|"detail", "at": [x,y]? (sheet mm), "scale": n|null?, "hidden_lines": bool?; base: "orientation": "front"|"back"|"left"|"right"|"top"|"bottom"|"iso_tl"|"iso_tr"|"iso_bl"|"iso_br" or "toward": [x,y,z], "x": [x,y,z]; projected: "parent": name, "dx", "dy" (-1..1); section: "parent", "a", "b" (parent mm), "flip"?; detail: "parent", "center", "radius" (parent mm)}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: add_drawing_view,
        },
        CommandSpec {
            id: "update_drawing_view",
            label: "Change View",
            menu: "",
            shortcut: None,
            help: "Move a drawing view, change its scale or show its hidden lines",
            params: r#"{"drawing": id, "view": name, "at": [x,y]?, "scale": n|null?, "hidden_lines": bool?, "flip": bool? (section)}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: update_drawing_view,
        },
        CommandSpec {
            id: "delete_drawing_view",
            label: "Delete View",
            menu: "",
            shortcut: None,
            help: "Remove a drawing view, the views made from it and their annotations",
            params: r#"{"drawing": id, "view": name}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: delete_drawing_view,
        },
        CommandSpec {
            id: "delete_drawing",
            label: "Delete Drawing",
            menu: "",
            shortcut: None,
            help: "Remove a drawing",
            params: r#"{"drawing": id}"#,
            modifies: true,
            enabled: |s| !s.doc.drawings.is_empty(),
            run: |s, p| {
                let id = p.id("drawing")?;
                let before = s.doc.drawings.len();
                s.doc.drawings.retain(|d| d.id != id);
                if before == s.doc.drawings.len() {
                    return Err(EngineError::BadParams(format!("no drawing {}", id.0)));
                }
                Ok(json!({ "deleted": id.0 }))
            },
        },
        CommandSpec {
            id: "export_drawing",
            label: "Drawing…",
            menu: "File/Export to/Drawing…",
            shortcut: None,
            help: "Export a drawing sheet as PDF, DWG, DXF or SVG",
            params: r#"{"path": "/out.pdf" | "/out.dwg" | "/out.dxf" | "/out.svg", "drawing": id? (default: four views of the visible bodies on A3)}"#,
            modifies: false,
            enabled: |s| s.doc.bodies.iter().any(|b| b.visible),
            run: |s, p| {
                let path = path_param(p, "path")?;
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let spec = match p.opt_id("drawing") {
                    Some(id) => s.doc.drawing(id).cloned().ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?,
                    None => crate::drawing::default_sheet(&s.doc.name),
                };
                let d = crate::drawing::make(s, &spec)?;
                let bytes = match ext.as_str() {
                    "svg" => d.to_svg().into_bytes(),
                    "dxf" => d.to_dxf().into_bytes(),
                    "pdf" => d.to_pdf(),
                    "dwg" => d.to_dwg()?,
                    other => return Err(EngineError::BadParams(format!("drawings export to .pdf, .dwg, .dxf or .svg, not .{other}"))),
                };
                std::fs::write(&path, bytes).map_err(|e| EngineError::BadParams(format!("{}: {e}", path.display())))?;
                Ok(json!({ "path": path, "scale": crate::drawing::scale_label(d.scale) }))
            },
        },
    ]
}

const SHEETS: [&str; 7] = ["A4", "A3", "A2", "A1", "A0", "Letter", "Tabloid"];

/// Applies the optional sheet settings in `p` to a drawing (shared by create and update).
fn drawing_settings(d: &mut shapecraft_doc::DrawingSheet, p: &Params) -> Result<()> {
    if p.has("name") {
        let n = p.str("name")?.trim();
        if n.is_empty() || n.len() > 200 {
            return Err(EngineError::BadParams("name must be 1–200 characters".into()));
        }
        d.name = n.to_string();
    }
    if p.has("sheet") {
        let sheet = p.str("sheet")?;
        if !SHEETS.contains(&sheet) {
            return Err(EngineError::BadParams(format!("sheet must be one of {}", SHEETS.join(", "))));
        }
        d.sheet = sheet.to_string();
    }
    if p.has("landscape") {
        d.landscape = p.bool_or("landscape", true);
    }
    if p.has("scale") {
        d.scale = match p.parse::<Option<f64>>("scale")? {
            Some(v) if v.is_finite() && v > 0.0 && v < 1e4 => Some(v),
            Some(_) => return Err(EngineError::BadParams("scale must be a positive number".into())),
            None => None,
        };
    }
    if p.has("first_angle") {
        d.first_angle = p.bool_or("first_angle", true);
    }
    if p.has("four_views") {
        d.four_views = p.bool_or("four_views", false);
        if d.views.is_some() {
            d.views = Some(crate::drawing::standard_views(d, d.four_views));
        }
    }
    if p.has("style") {
        let style: shapecraft_doc::DrawingStyle = p.parse("style")?;
        let widths = [style.visible, style.hidden, style.dimension, style.centerline, style.section, style.detail];
        if widths.iter().any(|w| !w.is_finite() || *w <= 0.0 || *w > 5.0) || style.length_precision > 6 || style.angle_precision > 6 {
            return Err(EngineError::BadParams("line widths must be 0–5 mm and precisions 0–6 digits".into()));
        }
        if !shapecraft_doc::TITLE_BLOCKS.contains(&style.title_block.as_str())
            || !shapecraft_doc::DATE_FORMATS.contains(&style.date_format.as_str())
        {
            return Err(EngineError::BadParams(format!(
                "title_block is one of {:?} and date_format one of {:?}",
                shapecraft_doc::TITLE_BLOCKS,
                shapecraft_doc::DATE_FORMATS
            )));
        }
        d.style = style;
    }
    Ok(())
}

fn create_drawing(s: &mut Session, p: &Params) -> Result<Value> {
    let bodies = if p.has("bodies") {
        p.ids("bodies")?
    } else {
        let mut v = Vec::new();
        for b in s.selection.iter().filter_map(Pick::body) {
            if !v.contains(&b) {
                v.push(b);
            }
        }
        v
    };
    if let Some(bad) = bodies.iter().find(|b| s.doc.body(**b).is_none()) {
        return Err(EngineError::BadParams(format!("no body {}", bad.0)));
    }
    if s.doc.drawings.len() >= 1000 {
        return Err(EngineError::BadParams("too many drawings".into()));
    }
    // Named after its (first) body, as the reference does.
    let default_name = bodies.first().and_then(|b| s.doc.body(*b)).map_or_else(|| s.doc.name.clone(), |b| b.name.clone());
    let mut d = crate::drawing::default_sheet(&default_name);
    d.four_views = false;
    d.bodies = bodies;
    drawing_settings(&mut d, p)?;
    // Four views to start with, else an empty sheet for the base view.
    d.views = Some(if d.four_views { crate::drawing::standard_views(&d, true) } else { Vec::new() });
    d.id = s.doc.alloc_id();
    let id = d.id;
    s.doc.drawings.push(d);
    Ok(json!({ "drawing": id.0 }))
}

fn update_drawing(s: &mut Session, p: &Params) -> Result<Value> {
    let id = p.id("drawing")?;
    let mut d = s.doc.drawing(id).cloned().ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))?;
    drawing_settings(&mut d, p)?;
    if let Some(slot) = s.doc.drawing_mut(id) {
        *slot = d;
    }
    Ok(json!({ "drawing": id.0 }))
}

fn drawing_of(s: &Session, p: &Params) -> Result<shapecraft_doc::DrawingSheet> {
    let id = p.id("drawing")?;
    s.doc.drawing(id).cloned().ok_or_else(|| EngineError::BadParams(format!("no drawing {}", id.0)))
}

fn put_drawing(s: &mut Session, d: shapecraft_doc::DrawingSheet) {
    if let Some(slot) = s.doc.drawing_mut(d.id) {
        *slot = d;
    }
}

/// A point on (or near) the sheet.
fn sheet_pos(d: &shapecraft_doc::DrawingSheet, p: &Params, key: &str) -> Result<glam::DVec2> {
    let at = p.vec2(key)?;
    let (w, h) = shapecraft_doc::DrawingSheet::paper(&d.sheet, d.landscape).unwrap_or((420.0, 297.0));
    if !at.is_finite() || at.x < -w || at.y < -h || at.x > 2.0 * w || at.y > 2.0 * h {
        return Err(EngineError::BadParams(format!("{key} is off the sheet")));
    }
    Ok(at)
}

fn view_scale_param(p: &Params) -> Result<Option<f64>> {
    match p.parse::<Option<f64>>("scale")? {
        Some(v) if v.is_finite() && v > 0.0 && v < 1e4 => Ok(Some(v)),
        Some(_) => Err(EngineError::BadParams("scale must be a positive number".into())),
        None => Ok(None),
    }
}

/// "Front", else "Front 2", "Front 3"…
fn unique_view_name(views: &[shapecraft_doc::SheetView], base: &str) -> String {
    let taken = |n: &str| views.iter().any(|v| v.name == n);
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).unwrap_or_else(|| base.to_string())
}

/// The next letter for a section or detail (I, O and Q are skipped, as drafting does).
fn next_letter(views: &[shapecraft_doc::SheetView]) -> String {
    use shapecraft_doc::SheetViewKind as K;
    let used: Vec<&str> = views
        .iter()
        .filter_map(|v| match &v.kind {
            K::Section { letter, .. } | K::Detail { letter, .. } => Some(letter.as_str()),
            _ => None,
        })
        .collect();
    let letters = "ABCDEFGHJKLMNPRSTUVWXYZ";
    letters.chars().map(String::from).find(|l| !used.contains(&l.as_str())).unwrap_or_else(|| format!("Z{}", used.len()))
}

fn add_drawing_view(s: &mut Session, p: &Params) -> Result<Value> {
    use shapecraft_doc::{SheetView, SheetViewKind as K};
    let mut d = drawing_of(s, p)?;
    // A sheet that laid its views out by itself keeps them, now placed.
    let mut views = d.views.take().unwrap_or_else(|| crate::drawing::standard_views(&d, d.four_views));
    if views.len() >= 100 {
        return Err(EngineError::BadParams("a sheet holds at most 100 views".into()));
    }
    let kind = p.str("kind")?;
    let parent = |views: &[SheetView]| -> Result<SheetView> {
        let name = p.str("parent")?;
        views.iter().find(|v| v.name == name).cloned().ok_or_else(|| EngineError::BadParams(format!("no view '{name}'")))
    };
    let middle = crate::drawing::sheet_point(&d, 0.5, 0.5);
    let (name, kind, at) = match kind {
        "base" => {
            let (toward, x) = if p.has("toward") {
                let (t, x) = (p.vec3("toward")?, p.vec3("x")?);
                if !t.is_finite() || !x.is_finite() || t.length() < 1e-9 || t.normalize().cross(x).length() < 1e-6 {
                    return Err(EngineError::BadParams("toward and x must be two directions that aren't parallel".into()));
                }
                (t.normalize(), x)
            } else {
                let key = p.opt_str("orientation").unwrap_or("front");
                crate::drawing::orientation(key).ok_or_else(|| EngineError::BadParams(format!("no orientation '{key}'")))?
            };
            let name = unique_view_name(&views, crate::drawing::orientation_name(toward));
            (name, K::Base { toward, x }, middle)
        }
        "projected" => {
            let par = parent(&views)?;
            let (dx, dy) = (p.f64("dx")?, p.f64("dy")?);
            if ![-1.0, 0.0, 1.0].contains(&dx) || ![-1.0, 0.0, 1.0].contains(&dy) || (dx == 0.0 && dy == 0.0) {
                return Err(EngineError::BadParams("dx and dy must be -1, 0 or 1, not both 0".into()));
            }
            let (dx, dy) = (dx as i8, dy as i8);
            if views.iter().any(|v| matches!(&v.kind, K::Projected { parent, dx: x, dy: y } if *parent == par.name && *x == dx && *y == dy))
            {
                return Err(EngineError::BadParams("that projection is already on the sheet".into()));
            }
            // Named for what it shows, following the projection angle.
            let base_frame = |v: &SheetView| -> Option<shapecraft_kernel::ViewFrame> {
                let mut chain = vec![v.clone()];
                while let Some(pn) = chain.last().and_then(|c| c.kind.parent().map(str::to_string)) {
                    let next = views.iter().find(|w| w.name == pn)?.clone();
                    if chain.len() > views.len() {
                        return None;
                    }
                    chain.push(next);
                }
                let root = chain.pop()?;
                let K::Base { toward, x } = root.kind else { return None };
                let mut f = shapecraft_kernel::ViewFrame { origin: glam::DVec3::ZERO, toward: toward.normalize(), x: x.normalize() };
                while let Some(c) = chain.pop() {
                    if let K::Projected { dx, dy, .. } = c.kind {
                        f = crate::drawing::projected_frame(f, dx, dy, d.first_angle)?;
                    }
                }
                Some(f)
            };
            let label = base_frame(&par)
                .and_then(|f| crate::drawing::projected_frame(f, dx, dy, d.first_angle))
                .map_or("View", |f| crate::drawing::orientation_name(f.toward));
            let gap = crate::drawing::sheet_point(&d, 0.25, 0.25) - crate::drawing::sheet_point(&d, 0.0, 0.0);
            (
                unique_view_name(&views, label),
                K::Projected { parent: par.name.clone(), dx, dy },
                par.at + glam::DVec2::new(f64::from(dx) * gap.x * 2.0, f64::from(dy) * gap.y * 2.0),
            )
        }
        "section" => {
            let par = parent(&views)?;
            let (a, b) = (p.vec2("a")?, p.vec2("b")?);
            if !a.is_finite() || !b.is_finite() || a.distance(b) < 1e-6 {
                return Err(EngineError::BadParams("a section line needs two different points".into()));
            }
            let letter = next_letter(&views);
            let flip = p.bool_or("flip", false);
            // The arrows (the way the section looks) point toward the view in first angle, away in third.
            let sight = if flip { -(b - a).normalize().perp() } else { (b - a).normalize().perp() };
            let place = if d.first_angle { sight } else { -sight };
            (format!("Section {letter}-{letter}"), K::Section { parent: par.name.clone(), a, b, flip, letter }, par.at + place * 90.0)
        }
        "detail" => {
            let par = parent(&views)?;
            let (center, radius) = (p.vec2("center")?, p.f64("radius")?);
            if !center.is_finite() || !(radius.is_finite() && radius > 1e-6 && radius < 1e6) {
                return Err(EngineError::BadParams("a detail needs a center and a positive radius".into()));
            }
            let letter = next_letter(&views);
            (
                format!("Detail {letter}"),
                K::Detail { parent: par.name.clone(), center, radius, letter },
                par.at + glam::DVec2::new(110.0, 0.0),
            )
        }
        other => return Err(EngineError::BadParams(format!("no kind of view '{other}'"))),
    };
    let at = if p.has("at") { sheet_pos(&d, p, "at")? } else { at };
    let section = matches!(kind, K::Section { .. });
    let scale = if p.has("scale") { view_scale_param(p)? } else { None };
    // A base view's Bodies…: which of the drawing's bodies it shows.
    let bodies = if p.has("bodies") { p.ids("bodies")? } else { Vec::new() };
    if !bodies.is_empty() && !matches!(kind, K::Base { .. }) {
        return Err(EngineError::NotAvailable("only a base view picks its bodies; the views made from it follow".into()));
    }
    if let Some(b) = bodies.iter().find(|b| s.doc.body(**b).is_none()) {
        return Err(EngineError::BadParams(format!("no body {}", b.0)));
    }
    views.push(SheetView { name: name.clone(), kind, at, scale, hidden_lines: p.bool_or("hidden_lines", !section), bodies });
    d.views = Some(views);
    let id = d.id;
    put_drawing(s, d);
    Ok(json!({ "drawing": id.0, "view": name }))
}

fn update_drawing_view(s: &mut Session, p: &Params) -> Result<Value> {
    let mut d = drawing_of(s, p)?;
    let name = p.str("view")?.to_string();
    let at = if p.has("at") { Some(sheet_pos(&d, p, "at")?) } else { None };
    let scale = if p.has("scale") { Some(view_scale_param(p)?) } else { None };
    let mut views = d.views.take().unwrap_or_else(|| crate::drawing::standard_views(&d, d.four_views));
    let v = views.iter_mut().find(|v| v.name == name).ok_or_else(|| EngineError::BadParams(format!("no view '{name}'")))?;
    if let Some(at) = at {
        v.at = at;
    }
    if let Some(scale) = scale {
        v.scale = scale;
    }
    if p.has("hidden_lines") {
        v.hidden_lines = p.bool_or("hidden_lines", true);
    }
    if p.has("flip")
        && let shapecraft_doc::SheetViewKind::Section { flip, .. } = &mut v.kind
    {
        *flip = p.bool_or("flip", false);
    }
    d.views = Some(views);
    let id = d.id;
    put_drawing(s, d);
    Ok(json!({ "drawing": id.0, "view": name }))
}

fn delete_drawing_view(s: &mut Session, p: &Params) -> Result<Value> {
    let mut d = drawing_of(s, p)?;
    let name = p.str("view")?.to_string();
    let mut views = d.views.take().unwrap_or_else(|| crate::drawing::standard_views(&d, d.four_views));
    if !views.iter().any(|v| v.name == name) {
        return Err(EngineError::BadParams(format!("no view '{name}'")));
    }
    // The view and every view made from it, then their annotations.
    let mut gone = vec![name];
    loop {
        let more: Vec<String> = views
            .iter()
            .filter(|v| !gone.contains(&v.name) && v.kind.parent().is_some_and(|p| gone.iter().any(|g| g == p)))
            .map(|v| v.name.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        gone.extend(more);
    }
    views.retain(|v| !gone.contains(&v.name));
    d.annotations.retain(|a| annotation_view(a).is_none_or(|v| !gone.iter().any(|g| g == v)));
    d.views = Some(views);
    let id = d.id;
    put_drawing(s, d);
    Ok(json!({ "drawing": id.0, "removed": gone }))
}

fn annotation_view(a: &shapecraft_doc::Annotation) -> Option<&str> {
    use shapecraft_doc::Annotation as A;
    match a {
        A::Distance { view, .. }
        | A::Radius { view, .. }
        | A::Angle { view, .. }
        | A::Centerline { view, .. }
        | A::CircularCenterline { view, .. }
        | A::IntersectionMark { view, .. }
        | A::CenterMark { view, .. } => Some(view),
        A::Note { .. } | A::Image { .. } => None,
    }
}

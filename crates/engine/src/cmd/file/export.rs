//! Export to STEP, Parasolid, IGES, STL, OBJ, 3MF, glTF, USDZ and DXF (Shapr3D export lives in `shapr`).

use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;

use serde_json::{Value, json};
use shapecraft_doc::{Body, ItemId, Pick, SketchEntity, project};
use shapecraft_kernel::Mesh;

use super::path_param;
use super::shapr::{export_shapr, parasolid_parts};
use crate::cmd::CommandSpec;
use crate::{EngineError, Params, Result, Session, formats};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "export_shapr",
            label: ".shapr…",
            menu: "File/Export to/.shapr…",
            shortcut: None,
            help: "Export as a .shapr project; a project that came from a .shapr file keeps its history and gets the changes as new steps",
            params: r#"{"path": "/out.shapr"}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty() || s.doc.sketches.iter().any(|k| !k.entities.is_empty()) || s.doc.shapr.is_some(),
            run: export_shapr,
        },
        CommandSpec {
            id: "export_parasolid",
            label: "Parasolid file…",
            menu: "File/Export to/Parasolid file…",
            shortcut: None,
            help: "Export bodies as Parasolid binary parts (.x_b); several bodies give one file each",
            params: r#"{"path": "/out.x_b", "hidden": false}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_parasolid,
        },
        CommandSpec {
            id: "export_step",
            label: "STEP…",
            menu: "File/Export to/STEP…",
            shortcut: None,
            help: "Export bodies as a STEP file",
            params: r#"{"path": "/out.step", "items": [ids]?, "hidden": false, "protocol": "AP242"|"AP214"|"AP203"?}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_step,
        },
        CommandSpec {
            id: "export_stl",
            label: "STL…",
            menu: "File/Export to/STL…",
            shortcut: None,
            help: "Export bodies as a binary STL mesh",
            params: r#"{"path": "/out.stl", "items": [ids]?, "hidden": false, "resolution": "high"|"low", "ascii": false, "units": "mm"|"cm"|"m"|"in"}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_stl,
        },
        CommandSpec {
            id: "export_obj",
            label: "OBJ…",
            menu: "File/Export to/OBJ…",
            shortcut: None,
            help: "Export bodies as an OBJ mesh with an MTL of their colors",
            params: r#"{"path": "/out.obj", "items": [ids]?, "hidden": false, "resolution": "high"|"low", "colors": true}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_obj,
        },
        CommandSpec {
            id: "export_3mf",
            label: "3MF…",
            menu: "File/Export to/3MF…",
            shortcut: None,
            help: "Export bodies as a 3MF package for 3D printing, with their colors",
            params: r#"{"path": "/out.3mf", "items": [ids]?, "hidden": false, "resolution": "high"|"low", "colors": true}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_3mf,
        },
        CommandSpec {
            id: "export_glb",
            label: "GLB…",
            menu: "File/Export to/GLB…",
            shortcut: None,
            help: "Export bodies as binary glTF with their materials",
            params: r#"{"path": "/out.glb", "items": [ids]?, "hidden": false, "resolution": "high"|"low", "colors": true}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_glb,
        },
        CommandSpec {
            id: "export_usdz",
            label: "USDZ…",
            menu: "File/Export to/USDZ…",
            shortcut: None,
            help: "Export bodies as a USDZ package (AR Quick Look, USD viewers) with their materials",
            params: r#"{"path": "/out.usdz", "items": [ids]?, "hidden": false, "resolution": "high"|"low"}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: |s, p| {
                let path = path_param(p, "path")?;
                let bodies = export_bodies(s, p)?;
                let bytes =
                    formats::write_usdz(&bodies).ok_or_else(|| EngineError::BadParams("the model is too large for a USDZ file".into()))?;
                write_file(&path, &bytes)?;
                Ok(json!({ "path": path, "bodies": bodies.len() }))
            },
        },
        CommandSpec {
            id: "export_iges",
            label: "IGES…",
            menu: "File/Export to/IGES…",
            shortcut: None,
            help: "Export bodies as an IGES file",
            params: r#"{"path": "/out.iges", "items": [ids]?, "hidden": false}"#,
            modifies: false,
            enabled: |s| !s.doc.bodies.is_empty(),
            run: export_iges,
        },
        CommandSpec {
            id: "export_sketch",
            label: "DXF from Sketch…",
            menu: "File/Export to/DXF from Sketch…",
            shortcut: None,
            help: "Export a sketch's elements as a DXF (or, for a .dwg path, a DWG) in its plane; a .svg path writes each visible sketch as an SVG image in its own plane (several: one SVG each in a .zip of the same name)",
            params: r#"{"path": "/out.dxf" | "/out.dwg" | "/out.svg", "sketch": id? (DXF/DWG default: the active, selected or first visible sketch; SVG default: every visible sketch), "faces": [{"body": id, "face": n}]? (SVG: flat faces' outlines), "selected": false (SVG: the selected sketches, curves, regions and flat faces when there are any), "hidden": false (SVG: hidden sketches too), "join": true (SVG: connected curves as one shape; false: one line, arc or circle each), "curves": true (SVG: arcs, circles, ellipses and splines as true curves; false: short lines at the resolution), "dpi": 96 (SVG pixels per inch), "resolution": 100 (SVG smoothness of flattened curves, 0..100)}"#,
            modifies: false,
            enabled: |s| s.doc.sketches.iter().any(|k| !k.entities.is_empty()),
            run: export_sketch,
        },
    ]
}

fn chosen_bodies<'a>(s: &'a Session, p: &Params) -> Result<Vec<&'a Body>> {
    let bodies: Vec<&Body> = if p.has("items") {
        let ids = p.ids("items")?;
        s.doc.bodies.iter().filter(|b| ids.contains(&b.id)).collect()
    } else {
        s.doc.bodies.iter().filter(|b| b.visible || p.bool_or("hidden", false)).collect()
    };
    if bodies.is_empty() {
        return Err(EngineError::BadParams("no bodies to export".into()));
    }
    Ok(bodies)
}

fn export_step(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    // Each body a part named after it, in its color when it has a look of its own.
    let parts: Vec<(shapecraft_kernel::Shape, shapecraft_kernel::PartInfo)> = chosen_bodies(s, p)?
        .iter()
        .map(|b| {
            // A body in an Items folder sits in an assembly of that name ("A / B" folders from an import nest again).
            let path = s
                .doc
                .folder_of(b.id)
                .and_then(|f| s.doc.folder(f))
                .map(|f| f.name.split(" / ").map(str::to_string).collect())
                .unwrap_or_default();
            Ok((
                crate::cmd::model::working_shape(s, b.id, false)?,
                shapecraft_kernel::PartInfo { name: Some(b.name.clone()), color: b.appearance.as_ref().map(|a| a.color), path },
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let n = match p.opt_str("protocol") {
        None => 214,
        Some(ap) => ap
            .trim_start_matches("AP")
            .trim_start_matches("ap")
            .parse()
            .map_err(|_| EngineError::BadParams(format!("unknown STEP protocol '{ap}'")))?,
    };
    s.kernel.write_step_parts(&parts, &path, n)?;
    Ok(json!({ "path": path, "bodies": parts.len() }))
}

fn export_stl(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    // STL has no units: the numbers are written in the asked unit (millimetres by default).
    let per_mm = match p.opt_str("units").unwrap_or("mm") {
        "cm" => 0.1,
        "m" => 0.001,
        "in" => 1.0 / 25.4,
        "mm" => 1.0,
        other => return Err(EngineError::BadParams(format!("unknown STL unit '{other}' (mm, cm, m, in)"))),
    };
    let mut all = Mesh::default();
    for b in export_bodies(s, p)? {
        let base = all.positions.len() as u32;
        all.positions.extend(b.mesh.positions.iter().map(|q| q.map(|v| v * per_mm)));
        all.indices.extend(b.mesh.indices.iter().map(|i| i + base));
    }
    if p.bool_or("ascii", false) {
        write_file(&path, formats::write_stl_ascii(&all).as_bytes())?;
    } else {
        let file = File::create(&path).map_err(|e| EngineError::BadParams(format!("{}: {e}", path.display())))?;
        all.write_stl(BufWriter::new(file))?;
    }
    Ok(json!({ "path": path, "triangles": all.triangle_count() }))
}

/// The chosen sketch (`sketch`, else the active one, else the one a selection is in, else the first visible) as a
/// DXF of its elements in the sketch plane.
fn export_sketch(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        return export_sketch_svg(s, p, &path);
    }
    let picked = s.selection.iter().find_map(|pk| match *pk {
        Pick::SketchEntity { sketch, .. } | Pick::Region { sketch, .. } => Some(sketch),
        _ => None,
    });
    let id = p
        .opt_id("sketch")
        .or(s.active_sketch)
        .or(picked)
        .or_else(|| s.doc.sketches.iter().find(|k| k.visible && !k.entities.is_empty()).map(|k| k.id));
    let sk = id.and_then(|i| s.doc.sketch(i)).ok_or_else(|| EngineError::NotAvailable("there's no sketch to export".into()))?;
    let ents: Vec<_> = sk.entities.iter().enumerate().map(|(i, e)| (e.clone(), sk.construction.contains(&(i as u32)))).collect();
    // A .dwg path writes an AutoCAD drawing; anything else the DXF.
    let dwg = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dwg"));
    if dwg {
        write_file(&path, &formats::write_dwg(&ents).map_err(EngineError::BadParams)?)?;
    } else {
        write_file(&path, formats::write_dxf(&ents).as_bytes())?;
    }
    Ok(json!({ "path": path, "sketch": sk.id.0, "elements": ents.len() }))
}

/// SVG from sketches: the given sketch, else every visible one (and the hidden ones with `hidden`), drawn in the
/// first one's plane.
fn export_sketch_svg(s: &mut Session, p: &Params, path: &std::path::Path) -> Result<Value> {
    let opts = formats::SvgOptions {
        dpi: p.f64_or("dpi", 96.0)?,
        resolution: p.f64_or("resolution", 100.0)?,
        join: p.bool_or("join", true),
        curves: p.bool_or("curves", true),
    };
    if !(opts.dpi.is_finite() && opts.dpi > 0.0 && opts.dpi <= 10_000.0) {
        return Err(EngineError::BadParams("dpi must be between 0 and 10000".into()));
    }
    if !(opts.resolution.is_finite() && (0.0..=100.0).contains(&opts.resolution)) {
        return Err(EngineError::BadParams("resolution must be between 0 and 100".into()));
    }
    let drawings = svg_drawings(s, p)?;
    // Each drawing in its own plane, so a sketch on any plane comes out face on.
    let mut files: Vec<(String, String)> = Vec::new();
    for (name, ents) in &drawings {
        if let Some(svg) = formats::write_svg(ents, &opts) {
            let base = name.replace(['/', '\\', ':'], "-");
            let mut file = format!("{base}.svg");
            let mut n = 2;
            while files.iter().any(|(f, _)| f.eq_ignore_ascii_case(&file)) {
                file = format!("{base} ({n}).svg");
                n += 1;
            }
            files.push((file, svg));
        }
    }
    let path = match files.len() {
        0 => return Err(EngineError::NotAvailable("there's nothing to draw".into())),
        1 => {
            write_file(path, files[0].1.as_bytes())?;
            path.to_path_buf()
        }
        // Several drawings: one SVG each, packed in a zip beside the asked name.
        _ => {
            let zip_path = path.with_extension("zip");
            let entries: Vec<(&str, Vec<u8>)> = files.iter().map(|(f, svg)| (f.as_str(), svg.as_bytes().to_vec())).collect();
            let bytes = formats::zip_store(&entries).ok_or_else(|| EngineError::BadParams("too much to fit in a zip".into()))?;
            write_file(&zip_path, &bytes)?;
            zip_path
        }
    };
    // One drawing is the file itself; several are the zip's entries.
    let names: Vec<String> = match files.len() {
        1 => vec![path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()],
        _ => files.iter().map(|(f, _)| f.clone()).collect(),
    };
    let elements: usize = drawings.iter().map(|(_, e)| e.iter().filter(|(_, c)| !c).count()).sum();
    Ok(json!({ "path": path, "files": names, "sketches": drawings.len(), "elements": elements }))
}

/// A named drawing: its curves, each with whether it's construction geometry.
type Drawing = (String, Vec<(SketchEntity, bool)>);

/// What an SVG export draws, each a name and its curves (with whether each is construction) in its own plane:
/// the given sketch or faces, else the selected sketches, curves, regions and flat faces when `selected`, else every
/// visible sketch (hidden ones too with `hidden`).
fn svg_drawings(s: &Session, p: &Params) -> Result<Vec<Drawing>> {
    let of_sketch = |k: &shapecraft_doc::Sketch| {
        (k.name.clone(), k.entities.iter().enumerate().map(|(i, e)| (e.clone(), k.construction.contains(&(i as u32)))).collect())
    };
    if let Some(id) = p.opt_id("sketch") {
        return Ok(vec![of_sketch(s.doc.sketch(id).ok_or_else(|| EngineError::BadParams("no such sketch".into()))?)]);
    }
    let mut picks: Vec<Pick> = Vec::new();
    if let Some(faces) = p.raw("faces").and_then(Value::as_array) {
        for f in faces {
            let (Some(body), Some(face)) = (f.get("body").and_then(Value::as_u64), f.get("face").and_then(Value::as_u64)) else {
                return Err(EngineError::BadParams("faces: [{\"body\": id, \"face\": n}]".into()));
            };
            picks.push(Pick::Face { body: ItemId(body), face: face as u32 });
        }
    } else if p.bool_or("selected", false) {
        picks = s.selection.clone();
    }
    let mut out = Vec::new();
    let (mut whole, mut curves): (Vec<ItemId>, Vec<(ItemId, Vec<u32>)>) = (Vec::new(), Vec::new());
    for pick in picks {
        match pick {
            Pick::Sketch { sketch } => {
                if !whole.contains(&sketch)
                    && let Some(k) = s.doc.sketch(sketch)
                {
                    whole.push(sketch);
                    out.push(of_sketch(k));
                }
            }
            // Picked curves: just those, one drawing per sketch.
            Pick::SketchEntity { sketch, entity } => match curves.iter_mut().find(|(k, _)| *k == sketch) {
                Some((_, list)) => list.push(entity),
                None => curves.push((sketch, vec![entity])),
            },
            Pick::Face { body, face } => {
                let on_face = json!({ "body": body.0, "face": face });
                let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
                let Some((plane, _)) = super::super::sketch::plane_from(s, &Params::new(&on_face))? else {
                    return Err(EngineError::NotAvailable(format!("face {} of {} isn't flat", face + 1, b.name)));
                };
                let shape = s.kernel.face(&b.shape, face)?;
                let n = s.kernel.edge_count(&shape)?;
                let ents = (0..n.min(10_000))
                    .filter_map(|i| s.kernel.edge_curve(&shape, i).ok().and_then(|c| super::super::sketch::edge_entity(&plane, c)))
                    .map(|e| (e, false))
                    .collect();
                out.push((format!("{} Face {}", b.name, face + 1), ents));
            }
            Pick::Region { sketch, region } => {
                let Some(k) = s.doc.sketch(sketch) else { continue };
                let Some(r) = k.region(region) else { continue };
                let n = s.kernel.edge_count(&r.face)?;
                let ents = (0..n.min(10_000))
                    .filter_map(|i| s.kernel.edge_curve(&r.face, i).ok().and_then(|c| super::super::sketch::edge_entity(&k.plane, c)))
                    .map(|e| (e, false))
                    .collect();
                out.push((format!("{} Region {}", k.name, region + 1), ents));
            }
            _ => {}
        }
    }
    for (sketch, list) in curves {
        let Some(k) = s.doc.sketch(sketch).filter(|_| !whole.contains(&sketch)) else { continue };
        let ents = list.iter().filter_map(|&i| k.entities.get(i as usize).map(|e| (e.clone(), k.construction.contains(&i)))).collect();
        out.push((k.name.clone(), ents));
    }
    if out.is_empty() {
        let hidden = p.bool_or("hidden", false);
        out = s.doc.sketches.iter().filter(|k| (k.visible || hidden) && !k.entities.is_empty()).map(of_sketch).collect();
    }
    if out.is_empty() {
        return Err(EngineError::NotAvailable("there's no sketch to export".into()));
    }
    Ok(out)
}

fn export_iges(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let ids: Vec<_> = chosen_bodies(s, p)?.iter().map(|b| b.id).collect();
    let shapes = ids.iter().map(|id| crate::cmd::model::working_shape(s, *id, false)).collect::<Result<Vec<_>>>()?;
    s.kernel.write_iges(&shapes, &path)?;
    Ok(json!({ "path": path, "bodies": shapes.len() }))
}

/// The bodies a mesh export writes, at the asked resolution ("low" re-meshes coarser; "high" is the view's mesh),
/// with their appearance colors unless "colors" is false.
fn export_bodies<'a>(s: &'a Session, p: &Params) -> Result<Vec<formats::ExportBody<'a>>> {
    let low = p.opt_str("resolution") == Some("low");
    let colors = p.bool_or("colors", true);
    chosen_bodies(s, p)?
        .into_iter()
        .map(|b| {
            let (color, metal, gloss) = match (&b.appearance, colors) {
                (Some(a), true) => (a.color, a.metal, a.gloss),
                _ => ([0.7, 0.7, 0.72], 0.0, 0.3),
            };
            let mesh =
                if low && !b.is_mesh() { Arc::new(s.kernel.tessellate(&b.shape, project::deflection() * 10.0)?) } else { b.mesh.clone() };
            Ok(formats::ExportBody { name: &b.name, mesh, color, metal, gloss })
        })
        .collect()
}

pub(super) fn write_file(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| EngineError::BadParams(format!("{}: {e}", path.display())))
}

fn export_obj(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let mtl_path = path.with_extension("mtl");
    let mtl_name = mtl_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "materials.mtl".into());
    let bodies = export_bodies(s, p)?;
    let (obj, mtl) = formats::write_obj(&bodies, &mtl_name);
    write_file(&path, obj.as_bytes())?;
    write_file(&mtl_path, mtl.as_bytes())?;
    Ok(json!({ "path": path, "mtl": mtl_path, "bodies": bodies.len() }))
}

fn export_3mf(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let bodies = export_bodies(s, p)?;
    let bytes = formats::write_3mf(&bodies).ok_or_else(|| EngineError::BadParams("the model is too large for a 3MF file".into()))?;
    write_file(&path, &bytes)?;
    Ok(json!({ "path": path, "bodies": bodies.len() }))
}

fn export_glb(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let bodies = export_bodies(s, p)?;
    let bytes = formats::write_glb(&bodies).ok_or_else(|| EngineError::BadParams("the model is too large for a GLB file".into()))?;
    write_file(&path, &bytes)?;
    Ok(json!({ "path": path, "bodies": bodies.len() }))
}

/// Bodies as Parasolid parts (.x_b); several bodies go to one file each, named after the body.
fn export_parasolid(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let bodies: Vec<_> = s
        .doc
        .bodies
        .iter()
        .filter(|b| b.visible || p.bool_or("hidden", false))
        .map(|b| Ok((b.name.clone(), crate::cmd::model::working_shape(s, b.id, false)?)))
        .collect::<Result<Vec<_>>>()?;
    let mut files = Vec::new();
    let single = bodies.len() == 1;
    for (name, shape) in bodies {
        for (i, xt) in parasolid_parts(s, &shape)?.into_iter().enumerate() {
            let out = if single && i == 0 {
                path.clone()
            } else {
                let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
                path.with_file_name(format!("{stem} - {safe}{}.x_b", if i > 0 { format!(" ({})", i + 1) } else { String::new() }))
            };
            write_file(&out, &xt)?;
            files.push(out);
        }
    }
    Ok(json!({ "files": files }))
}

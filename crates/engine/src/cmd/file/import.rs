//! Bringing files in: reference images, Import File (STEP, IGES, meshes, drawings, by extension or contents) and Import Project.

use std::fs::File;
use std::sync::Arc;

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{Body, ItemId, Pick, Sketch, SketchPlane, project};

use super::path_param;
use super::shapr::import_shapr;
use crate::cmd::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session, formats};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "insert_image",
            label: "Insert Image",
            menu: "Insert/Image…",
            shortcut: None,
            help: "Place a picture on a plane as a reference",
            params: r#"{"path": "/pic.png", "plane": "xy"|"xz"|"yz" | "body": id, "face": n | "construction": id | "origin","normal" (default: the top plane), "width": mm (100)}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let path = p.str("path")?;
                let bytes = std::fs::read(path).map_err(|e| EngineError::BadParams(format!("couldn't read {path}: {e}")))?;
                if bytes.len() > 64 << 20 {
                    return Err(EngineError::BadParams("that picture is too big (64 MB at most)".into()));
                }
                let bytes = shapecraft_doc::RefImage::normalize(bytes).map_err(EngineError::BadParams)?;
                let (ext, pixels) = shapecraft_doc::RefImage::decode(&bytes).map_err(EngineError::BadParams)?;
                let (plane, _) = crate::cmd::sketch::plane_from(s, p)?.unwrap_or((shapecraft_doc::SketchPlane::XY, None));
                let width = p.f64_or("width", 100.0)?;
                if !(width > 0.0 && width < 1e6) {
                    return Err(EngineError::BadParams("width must be positive".into()));
                }
                let size = glam::DVec2::new(width, width * pixels.1 as f64 / pixels.0.max(1) as f64);
                let id = s.doc.alloc_id();
                let n = s.doc.images.len() + 1;
                let name =
                    std::path::Path::new(path).file_stem().map_or_else(|| format!("Image {n:02}"), |f| f.to_string_lossy().into_owned());
                s.doc.images.push(shapecraft_doc::RefImage {
                    id,
                    name,
                    plane,
                    size,
                    opacity: 1.0,
                    visible: true,
                    ext,
                    file: std::sync::Arc::new(bytes),
                    pixels: std::sync::Arc::new(pixels),
                });
                s.selection = vec![Pick::Image { image: id }];
                Ok(json!({ "image": id.0 }))
            },
        },
        CommandSpec {
            id: "image_settings",
            label: "Image Settings",
            menu: "",
            shortcut: None,
            help: "Resize a reference picture or change how see-through it is",
            params: r#"{"image": id, "width": mm?, "opacity": 0–1?}"#,
            modifies: true,
            enabled: |s| !s.doc.images.is_empty(),
            run: |s, p| {
                let id = p.id("image")?;
                let width = if p.has("width") { Some(p.f64("width")?) } else { None };
                let opacity = if p.has("opacity") { Some(p.f64("opacity")?) } else { None };
                let img = s.doc.image_mut(id).ok_or_else(|| EngineError::BadParams(format!("no image {}", id.0)))?;
                if let Some(w) = width {
                    if !(w > 0.0 && w < 1e6) {
                        return Err(EngineError::BadParams("width must be positive".into()));
                    }
                    img.size = img.size * (w / img.size.x);
                }
                if let Some(o) = opacity {
                    img.opacity = o.clamp(0.05, 1.0) as f32;
                }
                Ok(json!({ "image": id.0 }))
            },
        },
        CommandSpec {
            id: "import_file",
            label: "Import File…",
            menu: "File/Import File…",
            shortcut: Some("⇧⌘I"),
            help: "Import STEP, IGES, Parasolid, SolidWorks, NX, Solid Edge or JT bodies, STL, OBJ or 3MF meshes as bodies, or DXF or SVG drawings as a sketch",
            params: r#"{"path": "/in.step|.stp|.iges|.igs|.x_t|.x_b|.sldprt|.prt (NX)|.par|.psm|.jt|.stl|.obj|.3mf|.dxf|.svg", "config": n? (SolidWorks: which configuration, default the first), "plane": "top"|"front"|"right"? (DXF/SVG: the sketch plane, default top), "units": "mm"|"cm"|"m"|"in"|"ft"? (STL/OBJ: what the file's numbers are in, default mm), "heal": "quality"|"speed"|{"repair": bool, "edges": bool, "sew": bool, "simplify": bool, "curves": bool}? (STEP/IGES/Parasolid clean-up, default speed = none; curves off leaves loose planar curves out of a STEP import)}"#,
            modifies: true,
            enabled: always,
            run: import_file,
        },
        CommandSpec {
            id: "import_project",
            label: "Import Project…",
            menu: "File/Import Project…",
            shortcut: Some("⇧⌘P"),
            help: "Bring another project's bodies, sketches, planes and axes into this one, in a folder named after it",
            params: r#"{"path": "/a/project/folder"}"#,
            modifies: true,
            enabled: always,
            run: import_project,
        },
    ]
}

/// Most bytes an imported mesh or drawing file may have.
// Real Shapr3D projects reach several hundred MB (a keyboard case with its PCB: 338 MB).
const MAX_IMPORT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(super) fn read_capped(path: &std::path::Path) -> Result<Vec<u8>> {
    let len = std::fs::metadata(path).map_err(|e| EngineError::BadParams(format!("{}: {e}", path.display())))?.len();
    if len > MAX_IMPORT_BYTES {
        return Err(EngineError::BadParams("the file is too large to import".into()));
    }
    std::fs::read(path).map_err(|e| EngineError::BadParams(format!("{}: {e}", path.display())))
}

/// Adds imported shapes as bodies; a mesh file's one body is named after the file.
pub(super) fn add_bodies(
    s: &mut Session,
    shapes: Vec<shapecraft_kernel::Shape>,
    mesh_name: Option<&str>,
) -> Result<Vec<shapecraft_doc::ItemId>> {
    let mut ids = Vec::new();
    for shape in shapes {
        let mesh = s.kernel.tessellate(&shape, project::deflection())?;
        ids.push(add_meshed_body(s, shape, mesh, mesh_name));
    }
    Ok(ids)
}

/// A body whose display triangles are already made.
pub(super) fn add_meshed_body(
    s: &mut Session,
    shape: shapecraft_kernel::Shape,
    mesh: shapecraft_kernel::Mesh,
    name: Option<&str>,
) -> shapecraft_doc::ItemId {
    let id = s.doc.alloc_id();
    let name = name.filter(|n| !n.is_empty()).map_or_else(|| s.doc.next_body_name(), str::to_string);
    s.doc.bodies.push(Body {
        id,
        name,
        shape,
        mesh: Arc::new(mesh),
        visible: true,
        appearance: None,
        face_colors: Vec::new(),
        facets: None,
    });
    id
}

/// Import Project (observed s14-150..152): the other project's items come in with new ids, in an Items folder
/// "<name> (Imported)", and the selection goes to its bodies.
fn import_project(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let other = shapecraft_doc::project::load(s.kernel.as_ref(), &path)
        .map_err(|e| EngineError::BadParams(format!("that project can't be opened: {e}")))?;
    if other.bodies.len() + other.sketches.len() > 10_000 {
        return Err(EngineError::BadParams("that project is too big to import".into()));
    }
    let mut map: std::collections::HashMap<shapecraft_doc::ItemId, shapecraft_doc::ItemId> = std::collections::HashMap::new();
    let mut items = Vec::new();
    let mut bodies = Vec::new();
    for mut b in other.bodies {
        let id = s.doc.alloc_id();
        map.insert(b.id, id);
        b.id = id;
        bodies.push(id);
        items.push(id);
        s.doc.bodies.push(b);
    }
    for mut sk in other.sketches {
        sk.id = s.doc.alloc_id();
        // A sketch drawn on an imported body follows it.
        sk.on_body = sk.on_body.and_then(|b| map.get(&b).copied());
        items.push(sk.id);
        s.doc.sketches.push(sk);
    }
    for mut pl in other.planes {
        pl.id = s.doc.alloc_id();
        items.push(pl.id);
        s.doc.planes.push(pl);
    }
    for mut ax in other.axes {
        ax.id = s.doc.alloc_id();
        items.push(ax.id);
        s.doc.axes.push(ax);
    }
    let folder = s.doc.alloc_id();
    let name = format!("{} (Imported)", other.name);
    s.doc.folders.push(shapecraft_doc::Folder { id: folder, name, items, visible: true });
    s.selection = bodies.iter().map(|&body| Pick::Body { body }).collect();
    Ok(json!({ "folder": folder.0, "bodies": bodies.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn import_file(s: &mut Session, p: &Params) -> Result<Value> {
    // Left over only from an import that failed after reading a Shapr3D project.
    s.shapr_steps.clear();
    // Meshes carry no units: the import options say what their numbers are in, and the bodies are scaled to mm.
    let factor = match p.opt_str("units").unwrap_or("mm") {
        "mm" => 1.0,
        "cm" => 10.0,
        "m" => 1000.0,
        "in" => 25.4,
        "ft" => 304.8,
        other => return Err(EngineError::BadParams(format!("units is mm, cm, m, in or ft, not {other:?}"))),
    };
    let before: Vec<ItemId> = s.doc.bodies.iter().map(|b| b.id).collect();
    let out = import_any(s, p)?;
    if factor != 1.0 {
        let ext = path_param(p, "path")?.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "stl" | "obj") {
            return Err(EngineError::NotAvailable("only STL and OBJ files take import units (the others carry their own)".into()));
        }
        let new: Vec<ItemId> = s.doc.bodies.iter().map(|b| b.id).filter(|id| !before.contains(id)).collect();
        for id in new {
            let m = glam::DMat4::from_scale(DVec3::splat(factor));
            crate::cmd::model::transform_body(s, id, &m, |s, shape| Ok(s.kernel.scale(shape, DVec3::ZERO, DVec3::splat(factor))?))?;
        }
    }
    Ok(out)
}

fn import_any(s: &mut Session, p: &Params) -> Result<Value> {
    let path = path_param(p, "path")?;
    let mut ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bad = |e: String| EngineError::BadParams(e);
    if !KNOWN_IMPORTS.contains(&ext.as_str()) {
        // Shapr3D saves exports under the bare name it was given: tell the kind from the contents.
        ext = sniff(&path).unwrap_or(ext);
    }
    if !path.is_file() {
        return Err(bad(format!("no file at {}", path.display())));
    }
    match ext.as_str() {
        "step" | "stp" => import_step(s, &path, heal_options(p)?, planar_curves(p)),
        "shapr" => import_shapr(s, &path),
        "x_t" | "x_b" | "xmt_txt" | "xmt_bin" => {
            s.stage("Reading the file…", 0);
            let bytes = read_capped(&path)?;
            import_parasolid(s, p, &path, &bytes, false, true, false)
        }
        "prt" | "par" | "psm" | "pwd" | "jt" => import_native(s, p, &path),
        "sldprt" => {
            s.stage("Reading the SolidWorks part…", 0);
            let bytes = read_capped(&path)?;
            use shapecraft_xt::sldprt::{PartitionKind, is_solidworks, partitions};
            if !is_solidworks(&bytes) {
                return Err(bad(format!("{stem} isn't a SolidWorks part")));
            }
            let all = partitions(&bytes);
            let mut configs: Vec<u32> = all.iter().filter(|m| m.kind == PartitionKind::Model).filter_map(|m| m.config).collect();
            configs.dedup();
            let config = if p.has("config") { Some(p.f64("config")? as u32) } else { configs.first().copied() };
            if let Some(c) = config
                && !configs.contains(&c)
            {
                return Err(bad(format!("{stem} has no configuration {c} (it has {configs:?})")));
            }
            // The configuration's model, or when that's empty (bodies from an imported file), its local bodies (older
            // versions: the bodies packed in its feature data).
            let of =
                |k: PartitionKind| all.iter().filter(|m| m.kind == k && m.config == config).map(|m| m.bytes.as_slice()).collect::<Vec<_>>();
            let model = of(PartitionKind::Model);
            let local = match of(PartitionKind::Local) {
                l if l.is_empty() => of(PartitionKind::Resolved),
                l => l,
            };
            // Only a model that reads and is empty falls back: one we can't read reports why instead.
            let empty = |ps: &[u8]| {
                shapecraft_xt::parse(ps).and_then(|f| shapecraft_xt::bodies(&f)).is_ok_and(|b| b.iter().all(|b| b.brep.faces.is_empty()))
            };
            let fallback = !local.is_empty() && model.iter().all(|m| empty(m));
            let sources = if fallback { local } else { model };
            if sources.is_empty() {
                return Err(bad(format!(
                    "{stem} has no solid model saved in it (a SolidWorks assembly or drawing, or a part saved without its geometry)"
                )));
            }
            let mut out = json!({"bodies": [], "folder": null, "repair_cut": 0});
            let mut ids = Vec::new();
            for ps in sources {
                let got = import_parasolid(s, p, &path, ps, config.is_some(), false, fallback)?;
                ids.extend(got["bodies"].as_array().into_iter().flatten().filter_map(Value::as_u64));
                out["repair_cut"] = json!(out["repair_cut"].as_u64().unwrap_or(0) + got["repair_cut"].as_u64().unwrap_or(0));
            }
            if ids.is_empty() {
                return Err(bad(format!("{stem} has no solid bodies in it")));
            }
            let items: Vec<_> = ids.iter().map(|&i| shapecraft_doc::ItemId(i)).collect();
            out["folder"] = json!(file_folder(s, &path, &items));
            out["bodies"] = json!(ids);
            out["configurations"] = json!(configs);
            s.selection.clear();
            Ok(out)
        }
        "iges" | "igs" => {
            s.stage("Reading the IGES file…", 0);
            let shapes = s.kernel.read_iges(&path)?;
            let mut ids = Vec::new();
            let mut cut = 0;
            let heal = heal_options(p)?;
            s.stage("Repairing and adding the shapes…", shapes.len() as u64);
            for (k, shape) in shapes.into_iter().enumerate() {
                s.progress_step();
                let shape = healed(s, &shape, heal, &mut cut)?;
                let name = format!("Imported {:02}", k + 1);
                ids.extend(add_bodies(s, vec![shape], Some(&name))?);
            }
            let folder = file_folder(s, &path, &ids);
            s.selection.clear();
            Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folder": folder, "repair_cut": cut }))
        }
        "stl" | "obj" => {
            let bytes = read_capped(&path)?;
            let (pts, tris) = if ext == "stl" {
                formats::read_stl(&bytes).map_err(bad)?
            } else {
                formats::read_obj(&String::from_utf8_lossy(&bytes)).map_err(bad)?
            };
            // Kept as a mesh body, in a folder named after the file, "Mesh body 01".
            let (pts, idx) = formats::weld(&pts, &tris);
            let facets = shapecraft_doc::Facets::new(pts, &idx)
                .ok_or_else(|| EngineError::BadParams(format!("{} has no usable triangles", path.display())))?;
            let id = crate::cmd::model::add_mesh_body(s, facets, "Mesh body 01".into())?;
            let folder = file_folder(s, &path, &[id]);
            s.selection.clear();
            Ok(json!({ "bodies": [id.0], "folder": folder, "mesh": true }))
        }
        "3mf" => {
            s.stage("Reading the 3MF file…", 0);
            let bytes = read_capped(&path)?;
            let parts = formats::read_3mf(&bytes).map_err(bad)?;
            s.stage("Adding the parts…", parts.len() as u64);
            let mut ids = Vec::new();
            for part in parts {
                s.progress_step();
                let (pts, idx) = formats::weld(&part.points, &part.triangles);
                // A part with nothing but collapsed triangles adds nothing.
                let Some(facets) = shapecraft_doc::Facets::new(pts, &idx) else { continue };
                let name = part.name.unwrap_or_else(|| format!("Mesh body {:02}", ids.len() + 1));
                let id = crate::cmd::model::add_mesh_body(s, facets, name)?;
                if let (Some(color), Some(b)) = (part.color, s.doc.body_mut(id)) {
                    b.appearance = Some(shapecraft_doc::Appearance { name: "Imported Color".into(), color, ..Default::default() });
                }
                ids.push(id);
            }
            if ids.is_empty() {
                return Err(EngineError::BadParams(format!("{} has no usable triangles", path.display())));
            }
            let folder = file_folder(s, &path, &ids);
            s.selection.clear();
            Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folder": folder, "mesh": true }))
        }
        "dxf" | "svg" => {
            let text = String::from_utf8_lossy(&read_capped(&path)?).into_owned();
            let entities = if ext == "dxf" { formats::read_dxf(&text) } else { formats::read_svg(&text) }.map_err(bad)?;
            let plane = match p.opt_str("plane").unwrap_or("top").to_ascii_lowercase().as_str() {
                "front" | "xz" => SketchPlane::XZ,
                "right" | "yz" => SketchPlane::YZ,
                _ => SketchPlane::XY,
            };
            let id = s.doc.alloc_id();
            let mut sketch = Sketch::new(id, stem.clone(), plane, None);
            let count = entities.len();
            sketch.entities = entities;
            s.doc.sketches.push(sketch);
            s.selection.clear();
            Ok(json!({ "sketch": id.0, "elements": count }))
        }
        "" => Err(EngineError::NotAvailable(format!("can't tell what kind of file {} is", path.display()))),
        _ => Err(EngineError::NotAvailable(format!("importing .{ext} files isn't supported"))),
    }
}

/// An NX part, a Solid Edge part or a JT file: the Parasolid model each keeps, in a folder named after the file.
fn import_native(s: &mut Session, p: &Params, path: &std::path::Path) -> Result<Value> {
    use shapecraft_xt::native::{Native, jt, kind, nx, solid_edge};
    let bad = |e: String| EngineError::BadParams(e);
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    s.stage("Reading the file…", 0);
    let bytes = read_capped(path)?;
    let mut note = None;
    let files = match kind(&bytes) {
        Some(Native::Nx) => nx(&bytes),
        Some(Native::SolidEdge) => solid_edge(&bytes).map_err(bad)?,
        Some(Native::Jt) => {
            let j = jt(&bytes).map_err(bad)?;
            if j.parts.is_empty() {
                return Err(bad(if j.meshes > 0 || j.jt_breps > 0 {
                    format!(
                        "{stem} has only display meshes in it, not exact geometry. Save it again with exact (XT B-Rep) geometry, or export STEP"
                    )
                } else {
                    format!("{stem} has no geometry in it")
                }));
            }
            if j.parts.len() > 1 {
                note = Some("Each part is placed where its own file puts it: assembly positions aren't read yet".to_string());
            }
            j.parts
        }
        None if bytes.starts_with(b"#UGC") => {
            return Err(bad(format!("{stem} is a Creo part, which can't be read. Export it from Creo as STEP")));
        }
        None => return Err(bad(format!("{stem} isn't an NX part, a Solid Edge part or a JT file"))),
    };
    if files.is_empty() {
        return Err(bad(format!("{stem} has no solid model saved in it (an assembly or drawing, or a part saved without its geometry)")));
    }
    let solo = files.len() == 1;
    let (mut ids, mut cut, mut failed) = (Vec::new(), 0, Vec::new());
    for ps in &files {
        match import_parasolid(s, p, path, ps, solo, false, false) {
            Ok(got) => {
                ids.extend(got["bodies"].as_array().into_iter().flatten().filter_map(Value::as_u64));
                cut += got["repair_cut"].as_u64().unwrap_or(0);
            }
            // One part of an assembly we can't read doesn't lose the rest.
            Err(e) if !solo => failed.push(e.to_string()),
            Err(e) => return Err(e),
        }
    }
    if ids.is_empty() {
        return Err(bad(match failed.first() {
            Some(e) => e.clone(),
            None => format!("{stem} has no bodies in it"),
        }));
    }
    let items: Vec<_> = ids.iter().map(|&i| shapecraft_doc::ItemId(i)).collect();
    let folder = file_folder(s, path, &items);
    s.selection.clear();
    let mut out = json!({ "bodies": ids, "folder": folder, "repair_cut": cut });
    if let Some(n) = note {
        out["note"] = json!(n);
    }
    if !failed.is_empty() {
        out["unreadable"] = json!(failed);
    }
    Ok(out)
}

/// Bodies from a Parasolid transmit file (a `.x_t`/`.x_b`, or the model inside another system's part), in a
/// folder named after the file. Bodies without faces (a part's hidden working bodies) are left out.
/// `solids_only` also leaves out sheets and wires (a part's feature data carries them beside its solids).
/// `name_solo`: a file with a single body names it after the file.
fn import_parasolid(
    s: &mut Session,
    p: &Params,
    path: &std::path::Path,
    bytes: &[u8],
    name_solo: bool,
    folder: bool,
    solids_only: bool,
) -> Result<Value> {
    let bad = |e: String| EngineError::BadParams(e);
    let file = shapecraft_xt::parse(bytes).map_err(|e| bad(e.to_string()))?;
    let bodies: Vec<_> = shapecraft_xt::bodies(&file)
        .map_err(|e| bad(e.to_string()))?
        .into_iter()
        .filter(|b| !b.brep.faces.is_empty() && (!solids_only || b.brep.kind == shapecraft_kernel::brep::BodyKind::Solid))
        .collect();
    let mut ids = Vec::new();
    let mut cut = 0;
    s.stage("Building the bodies…", bodies.len() as u64);
    let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let solo = bodies.len() == 1;
    for (k, b) in bodies.into_iter().enumerate() {
        s.progress_step();
        let shape = healed(s, &s.kernel.from_brep(&b.brep)?, heal_options(p)?, &mut cut)?;
        let name = b.name.unwrap_or_else(|| if name_solo && solo { stem.clone() } else { format!("Imported {:02}", k + 1) });
        ids.extend(add_bodies(s, vec![shape], Some(&name))?);
    }
    let folder = if folder { file_folder(s, path, &ids) } else { None };
    s.selection.clear();
    Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folder": folder, "repair_cut": cut }))
}

/// Import Preferences: "heal" is "quality" (every clean-up), "speed" (none, the default for
/// scripts) or an object picking them: {"repair", "edges", "sew", "simplify"}.
fn heal_options(p: &Params) -> Result<shapecraft_kernel::HealOptions> {
    use shapecraft_kernel::HealOptions;
    Ok(match p.raw("heal") {
        None | Some(Value::Null) => HealOptions::default(),
        Some(Value::String(m)) if m == "quality" => HealOptions::ALL,
        Some(Value::String(m)) if m == "speed" => HealOptions::default(),
        Some(Value::Object(o)) => {
            let on = |k: &str| o.get(k).and_then(Value::as_bool).unwrap_or(false);
            HealOptions { repair: on("repair"), edges: on("edges"), sew: on("sew"), simplify: on("simplify") }
        }
        Some(other) => {
            return Err(EngineError::BadParams(format!("heal is \"quality\", \"speed\" or an object of switches, not {other}")));
        }
    })
}

/// Import Planar Curves as Sketches: on unless "heal" turns it off.
fn planar_curves(p: &Params) -> bool {
    p.raw("heal").and_then(|h| h.get("curves")).and_then(Value::as_bool).unwrap_or(true)
}

/// Loose curves of a file as sketches: every set of them lying in one plane.
fn curves_as_sketches(s: &mut Session, name: &str, curves: &[shapecraft_kernel::EdgeCurve]) -> Vec<shapecraft_doc::ItemId> {
    use shapecraft_doc::SketchEntity;
    use shapecraft_kernel::EdgeCurve as E;
    let points = |c: &E| -> Vec<DVec3> {
        match c {
            E::Line { a, b } => vec![*a, *b],
            E::Arc { a, mid, b } => vec![*a, *mid, *b],
            E::Circle { center, normal, radius } => {
                let x = normal.any_orthonormal_vector() * *radius;
                let y = normal.normalize_or(DVec3::Z).cross(x);
                vec![*center + x, *center + y, *center - x]
            }
            E::Other { points } => points.clone(),
        }
    };
    let all: Vec<Vec<DVec3>> = curves.iter().map(points).collect();
    let size = all.iter().flatten().map(|q| q.distance(all[0][0])).fold(1e-6, f64::max);
    let tol = size * 1e-5;
    let normal_of = |pts: &[DVec3]| -> Option<DVec3> {
        let (a, far) = (pts[0], pts.iter().copied().max_by(|p, q| p.distance(pts[0]).total_cmp(&q.distance(pts[0])))?);
        let off = pts.iter().map(|p| (far - a).cross(*p - a)).max_by(|p, q| p.length().total_cmp(&q.length()))?;
        (off.length() > tol * (far - a).length()).then(|| off.normalize())
    };
    let within = |pts: &[DVec3], o: DVec3, n: DVec3| pts.iter().all(|p| (*p - o).dot(n).abs() <= tol);
    let mut left: Vec<usize> = (0..curves.len()).collect();
    let mut out = Vec::new();
    while !left.is_empty() {
        // The plane of a curve that has one, else of two lines that meet, else (a lone line) an upright or level one.
        let bent = left.iter().find_map(|&i| normal_of(&all[i]).map(|n| (all[i][0], n)));
        let crossed = || {
            left.iter().find_map(|&i| {
                left.iter().find_map(|&j| {
                    let four: Vec<DVec3> = all[i].iter().chain(&all[j]).copied().collect();
                    let n = if i == j { None } else { normal_of(&four) }?;
                    within(&four, four[0], n).then_some((four[0], n))
                })
            })
        };
        let Some((o, n)) = bent.or_else(crossed).or_else(|| {
            let line = &all[*left.first()?];
            let d = (line[line.len() - 1] - line[0]).normalize_or(DVec3::X);
            let n = if d.z.abs() < 1e-6 { DVec3::Z } else { d.cross(if d.x.abs() < 0.9 { DVec3::X } else { DVec3::Y }) };
            Some((line[0], n.normalize_or(DVec3::Z)))
        }) else {
            break;
        };
        let (mine, rest): (Vec<usize>, Vec<usize>) = left.iter().partition(|&&i| within(&all[i], o, n));
        if mine.is_empty() {
            break;
        }
        left = rest;
        // The frame the app's own planes have, so the file's coordinates read the same.
        let n = if n.z.abs() > 1.0 - 1e-9 {
            DVec3::Z
        } else if n.y.abs() > 1.0 - 1e-9 {
            DVec3::Y * -1.0
        } else {
            n
        };
        let n = if n.x < -1.0 + 1e-9 { DVec3::X } else { n };
        let Some(plane) = SketchPlane::on_face(n * o.dot(n), n, false) else { continue };
        let at = |q: DVec3| plane.to_local(q);
        let entities: Vec<SketchEntity> = mine
            .iter()
            .map(|&i| match &curves[i] {
                E::Line { a, b } => SketchEntity::Line { a: at(*a), b: at(*b) },
                E::Arc { a, mid, b } => SketchEntity::Arc { a: at(*a), mid: at(*mid), b: at(*b) },
                E::Circle { center, radius, .. } => SketchEntity::Circle { center: at(*center), radius: *radius },
                E::Other { points } => SketchEntity::Spline { points: points.iter().step_by(2).map(|p| at(*p)).collect() },
            })
            .collect();
        let id = s.doc.alloc_id();
        let label = if out.is_empty() { name.to_string() } else { format!("{name} ({})", out.len() + 1) };
        let mut sketch = Sketch::new(id, label, plane, None);
        sketch.entities = entities;
        s.doc.sketches.push(sketch);
        out.push(id);
    }
    out
}

/// Imported bodies go into an Items folder named after the file, as Shapr3D's Import File does ("box.step").
fn file_folder(s: &mut Session, path: &std::path::Path, items: &[shapecraft_doc::ItemId]) -> Option<u64> {
    if items.is_empty() {
        return None;
    }
    let name: String = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default().chars().take(200).collect();
    let id = s.doc.alloc_id();
    s.doc.folders.push(shapecraft_doc::Folder { id, name, items: items.to_vec(), visible: true });
    Some(id.0)
}

const KNOWN_IMPORTS: &[&str] = &[
    "step", "stp", "shapr", "x_t", "x_b", "xmt_txt", "xmt_bin", "sldprt", "prt", "par", "psm", "pwd", "jt", "iges", "igs", "stl", "obj",
    "3mf", "dxf", "svg",
];

/// The import kind of a file from its first bytes: a Shapr3D project (a ZIP holding `workspace`), Parasolid
/// (binary "PS", or the text header), STEP or IGES.
fn sniff(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut head = vec![0u8; 4096];
    let n = File::open(path).and_then(|mut f| f.read(&mut head)).ok()?;
    head.truncate(n);
    // A compound file's directory can be anywhere in it: SolidWorks and Solid Edge are told apart by their streams.
    if head.starts_with(b"\xD0\xCF\x11\xE0") {
        head = read_capped(path).ok()?;
    }
    let head = &head[..];
    let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    let kind = if head.starts_with(b"PK\x03\x04") && (has(b"3D/3dmodel.model") || has(b"[Content_Types].xml") && has(b"3D/")) {
        "3mf"
    } else if head.starts_with(b"PK\x03\x04") && (has(b"workspace") || has(b".metadata")) {
        "shapr"
    } else if shapecraft_xt::sldprt::is_solidworks(head) {
        "sldprt"
    } else if let Some(k) = shapecraft_xt::native::kind(head) {
        match k {
            shapecraft_xt::native::Native::Nx => "prt",
            shapecraft_xt::native::Native::SolidEdge => "par",
            shapecraft_xt::native::Native::Jt => "jt",
        }
    } else if head.starts_with(b"PS\0\0") {
        "x_b"
    } else if head.starts_with(b"**ABCDEFGHIJKLMNOPQRSTUVWXYZ") || has(b"**PARASOLID") {
        "x_t"
    } else if head.starts_with(b"ISO-10303-21") {
        "step"
    } else if head.len() > 73 && head[72] == b'S' && head.get(..80).is_some_and(|l| l.iter().all(|c| c.is_ascii())) {
        "iges"
    } else {
        return None;
    };
    Some(kind.to_string())
}

/// A STEP file's parts as bodies with the names and colors the file gives them; parts inside assemblies go into an
/// Items folder per assembly ("Engine / Piston" for nested ones).
fn import_step(s: &mut Session, path: &std::path::Path, heal: shapecraft_kernel::HealOptions, curves: bool) -> Result<Value> {
    s.stage("Reading the STEP file…", 0);
    let parts = s.kernel.read_step_parts(path)?;
    if parts.len() > 10_000 {
        return Err(EngineError::BadParams("that file has too many parts to import".into()));
    }
    let mut ids = Vec::new();
    let mut sketches: Vec<shapecraft_doc::ItemId> = Vec::new();
    let mut cut = 0;
    let mut folders: Vec<(Vec<String>, Vec<shapecraft_doc::ItemId>)> = Vec::new();
    // Everything lands in a folder named after the file, parts of assemblies in folders below it.
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    // Parts are "Imported 01", "02"… in each file, whatever the file calls them.
    s.stage("Repairing and adding the parts…", parts.len() as u64);
    for (k, (read, mut info)) in parts.into_iter().enumerate() {
        s.progress_step();
        // Loose curves: sketches, or left out.
        if s.kernel.face_count(&read)? == 0 {
            let count = s.kernel.edge_count(&read)?;
            let list: Vec<_> = (0..count.min(20_000)).filter_map(|e| s.kernel.edge_curve(&read, e).ok()).collect();
            if curves && !list.is_empty() {
                let made = curves_as_sketches(s, &format!("Imported {:02}", k + 1), &list);
                info.path.insert(0, file.clone());
                match folders.iter_mut().find(|f| f.0 == info.path) {
                    Some(f) => f.1.extend(&made),
                    None => folders.push((info.path.clone(), made.clone())),
                }
                sketches.extend(made);
                continue;
            }
            if !list.is_empty() {
                continue;
            }
        }
        let shape = healed(s, &read, heal, &mut cut)?;
        let face_colors = repaired_face_colors(s, &read, &shape, &info.face_colors);
        let name = format!("Imported {:02}", k + 1);
        let made = add_bodies(s, vec![shape], Some(&name))?;
        info.path.insert(0, file.clone());
        for &id in &made {
            if let (Some(c), Some(b)) = (info.color, s.doc.body_mut(id)) {
                b.appearance = Some(shapecraft_doc::Appearance { name: "Imported Color".into(), color: c, ..Default::default() });
            }
            if let (false, Some(b)) = (face_colors.is_empty(), s.doc.body_mut(id)) {
                b.face_colors = face_colors.clone();
            }
            match folders.iter_mut().find(|f| f.0 == info.path) {
                Some(f) => f.1.push(id),
                None => folders.push((info.path.clone(), vec![id])),
            }
        }
        ids.extend(made);
    }
    // One nested folder per assembly step in the path (as an Items list shows them), not one flat folder
    // wearing the whole path as its name.
    let made_folders = super::shapr::import_folder_tree(s, folders);
    // Nothing is selected after an import.
    s.selection.clear();
    Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folders": made_folders, "repair_cut": cut,
        "sketches": sketches.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// The faces of a part the file colours one by one, on the faces of its repaired shape: the repair may renumber, split
/// or merge faces, so each repaired face takes the colour of the face it came from.
fn repaired_face_colors(
    s: &Session,
    read: &shapecraft_kernel::Shape,
    repaired: &shapecraft_kernel::Shape,
    colors: &[(u32, [f32; 3])],
) -> Vec<(u32, [f32; 3])> {
    if colors.is_empty() {
        return Vec::new();
    }
    if read.ptr_eq(repaired) {
        return colors.iter().copied().take(10_000).collect();
    }
    let by_face: std::collections::HashMap<u32, [f32; 3]> = colors.iter().copied().collect();
    let from = s.kernel.match_faces(read, repaired).unwrap_or_default();
    from.iter().enumerate().filter_map(|(face, was)| Some((face as u32, *by_face.get(&(*was)?)?))).take(10_000).collect()
}

/// Seconds the repair of one imported shape may take before it is cut short (#21).
const REPAIR_SECONDS: f64 = 20.0;

/// Heals one imported shape within [`REPAIR_SECONDS`], counting the shapes whose repair was cut short.
fn healed(
    s: &Session,
    shape: &shapecraft_kernel::Shape,
    heal: shapecraft_kernel::HealOptions,
    cut: &mut usize,
) -> Result<shapecraft_kernel::Shape> {
    let (shape, how) = s.kernel.heal_within(shape, heal, REPAIR_SECONDS)?;
    if how > 0 {
        *cut += 1;
    }
    Ok(shape)
}

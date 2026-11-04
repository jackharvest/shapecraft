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
            help: "Import STEP or IGES bodies, STL or OBJ meshes as bodies, or DXF or SVG drawings as a sketch",
            params: r#"{"path": "/in.step|.stp|.iges|.igs|.stl|.obj|.dxf|.svg", "plane": "top"|"front"|"right"? (DXF/SVG: the sketch plane, default top), "units": "mm"|"cm"|"m"|"in"|"ft"? (STL/OBJ: what the file's numbers are in, default mm), "heal": "quality"|"speed"|{"repair": bool, "edges": bool, "sew": bool, "simplify": bool}? (STEP/IGES/Parasolid clean-up, default speed = none)}"#,
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
        let mesh = Arc::new(s.kernel.tessellate(&shape, project::deflection())?);
        let id = s.doc.alloc_id();
        let name = mesh_name.filter(|n| !n.is_empty()).map_or_else(|| s.doc.next_body_name(), str::to_string);
        s.doc.bodies.push(Body { id, name, shape, mesh, visible: true, appearance: None, face_colors: Vec::new() });
        ids.push(id);
    }
    Ok(ids)
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
    s.doc.folders.push(shapecraft_doc::Folder { id: folder, name, items });
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
            let Some(b) = s.doc.body(id) else { continue };
            let scaled = s.kernel.scale(&b.shape, DVec3::ZERO, DVec3::splat(factor))?;
            crate::cmd::model::set_body_shape(s, id, scaled)?;
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
        "step" | "stp" => import_step(s, &path, heal_options(p)?),
        "shapr" => import_shapr(s, &path),
        "x_t" | "x_b" | "xmt_txt" | "xmt_bin" => {
            let bytes = read_capped(&path)?;
            let file = shapecraft_xt::parse(&bytes).map_err(|e| bad(e.to_string()))?;
            let bodies = shapecraft_xt::bodies(&file).map_err(|e| bad(e.to_string()))?;
            let mut ids = Vec::new();
            for (k, b) in bodies.into_iter().enumerate() {
                let shape = s.kernel.heal(&s.kernel.from_brep(&b.brep)?, heal_options(p)?)?;
                let name = b.name.unwrap_or_else(|| format!("Imported {:02}", k + 1));
                ids.extend(add_bodies(s, vec![shape], Some(&name))?);
            }
            let folder = file_folder(s, &path, &ids);
            s.selection.clear();
            Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folder": folder }))
        }
        "iges" | "igs" => {
            let shapes = s.kernel.read_iges(&path)?;
            let mut ids = Vec::new();
            let heal = heal_options(p)?;
            for (k, shape) in shapes.into_iter().enumerate() {
                let shape = s.kernel.heal(&shape, heal)?;
                let name = format!("Imported {:02}", k + 1);
                ids.extend(add_bodies(s, vec![shape], Some(&name))?);
            }
            let folder = file_folder(s, &path, &ids);
            s.selection.clear();
            Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folder": folder }))
        }
        "stl" | "obj" => {
            let bytes = read_capped(&path)?;
            let (pts, tris) = if ext == "stl" {
                formats::read_stl(&bytes).map_err(bad)?
            } else {
                formats::read_obj(&String::from_utf8_lossy(&bytes)).map_err(bad)?
            };
            let (pts, idx) = formats::weld(&pts, &tris);
            let shape = s.kernel.shape_from_mesh(&pts, &idx)?;
            let ids = add_bodies(s, vec![shape], Some(&stem))?;
            s.selection.clear();
            Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
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

/// Imported bodies go into an Items folder named after the file, as Shapr3D's Import File does ("box.step").
fn file_folder(s: &mut Session, path: &std::path::Path, items: &[shapecraft_doc::ItemId]) -> Option<u64> {
    if items.is_empty() {
        return None;
    }
    let name: String = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default().chars().take(200).collect();
    let id = s.doc.alloc_id();
    s.doc.folders.push(shapecraft_doc::Folder { id, name, items: items.to_vec() });
    Some(id.0)
}

const KNOWN_IMPORTS: &[&str] = &["step", "stp", "shapr", "x_t", "x_b", "xmt_txt", "xmt_bin", "iges", "igs", "stl", "obj", "dxf", "svg"];

/// The import kind of a file from its first bytes: a Shapr3D project (a ZIP holding `workspace`), Parasolid
/// (binary "PS", or the text header), STEP or IGES.
fn sniff(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut head = vec![0u8; 4096];
    let n = File::open(path).and_then(|mut f| f.read(&mut head)).ok()?;
    let head = &head[..n];
    let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
    let kind = if head.starts_with(b"PK\x03\x04") && (has(b"workspace") || has(b".metadata")) {
        "shapr"
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
fn import_step(s: &mut Session, path: &std::path::Path, heal: shapecraft_kernel::HealOptions) -> Result<Value> {
    let parts = s.kernel.read_step_parts(path)?;
    if parts.len() > 10_000 {
        return Err(EngineError::BadParams("that file has too many parts to import".into()));
    }
    let mut ids = Vec::new();
    let mut folders: Vec<(String, Vec<shapecraft_doc::ItemId>)> = Vec::new();
    // Everything lands in a folder named after the file, parts of assemblies in folders below it.
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    // Parts are "Imported 01", "02"… in each file, whatever the file calls them.
    for (k, (shape, mut info)) in parts.into_iter().enumerate() {
        let shape = s.kernel.heal(&shape, heal)?;
        let name = format!("Imported {:02}", k + 1);
        let made = add_bodies(s, vec![shape], Some(&name))?;
        info.path.insert(0, file.clone());
        for &id in &made {
            if let (Some(c), Some(b)) = (info.color, s.doc.body_mut(id)) {
                b.appearance = Some(shapecraft_doc::Appearance { name: "Imported Color".into(), color: c, ..Default::default() });
            }
            {
                let key = info.path.join(" / ");
                match folders.iter_mut().find(|f| f.0 == key) {
                    Some(f) => f.1.push(id),
                    None => folders.push((key, vec![id])),
                }
            }
        }
        ids.extend(made);
    }
    let mut made_folders = Vec::new();
    for (name, items) in folders {
        let id = s.doc.alloc_id();
        s.doc.folders.push(shapecraft_doc::Folder { id, name: name.chars().take(200).collect(), items });
        made_folders.push(id.0);
    }
    // Nothing is selected after an import.
    s.selection.clear();
    Ok(json!({ "bodies": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "folders": made_folders }))
}

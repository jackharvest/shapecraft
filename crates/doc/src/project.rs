//! On-disk projects: a folder holding `project.json` (readable, diffable) and a STEP file (all bodies,
//! in document order). Nothing proprietary, nothing in the cloud.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use shapecraft_kernel::Kernel;

use crate::{
    Appearance, Body, ConstructionAxis, ConstructionPlane, Document, DrawingSheet, HistoryStep, ItemId, Sketch, SketchEntity, SketchPlane,
    Units,
};

/// 2: sketches on faces line up with the standard views (older files keep their frame: `legacy_face_axes`).
pub const FILE_VERSION: u32 = 2;
pub const PROJECT_FILE: &str = "project.json";
pub const BODIES_FILE: &str = "bodies.step";
/// The other name saves take turns with, so a save never touches the file the project currently points at.
const BODIES_FILE_NEXT: &str = "bodies-next.step";
/// The folder of reference pictures (Insert ▸ Image).
pub const IMAGES_DIR: &str = "images";
pub const THUMBNAIL_FILE: &str = "thumbnail.png";

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("project.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Kernel(#[from] shapecraft_kernel::KernelError),
    #[error("{0}")]
    Invalid(String),
    /// The bodies file is missing or doesn't hold the bodies project.json lists (the rest can still open).
    #[error("{0}")]
    Bodies(String),
    /// project.json says its bodies file is one that isn't here yet: a sync app (Dropbox, OneDrive…) has brought the
    /// new project.json but not the shapes saved with it. Opening now would show old shapes, so it waits.
    #[error("it's still syncing from another computer (its shapes haven't arrived yet). Try again in a moment.")]
    StillSyncing,
}

fn default_bodies_file() -> String {
    BODIES_FILE.to_string()
}

/// Just which file holds a saved project's bodies.
#[derive(Deserialize)]
struct BodiesRef {
    #[serde(default = "default_bodies_file")]
    bodies_file: String,
}

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    version: u32,
    /// Sketches on faces keep the frame of version 1 (files from before version 2, saved again).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    legacy_face_axes: bool,
    name: String,
    units: Units,
    next_id: u64,
    body_counter: u32,
    sketch_counter: u32,
    /// Which of the two names holds the bodies.
    #[serde(default = "default_bodies_file")]
    bodies_file: String,
    /// That file's size and FNV-1a hash as saved, so a stale copy of it (the same name two saves ago) is told apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bodies_check: Option<(u64, u64)>,
    bodies: Vec<BodyRecord>,
    sketches: Vec<SketchRecord>,
    #[serde(default)]
    plane_counter: u32,
    #[serde(default)]
    planes: Vec<ConstructionPlane>,
    #[serde(default)]
    axis_counter: u32,
    #[serde(default)]
    axes: Vec<ConstructionAxis>,
    #[serde(default)]
    drawings: Vec<DrawingSheet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    variables: Vec<crate::Variable>,
    /// Reference pictures; each file is saved as images/<id>.<ext>.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    images: Vec<crate::RefImage>,
    #[serde(default, skip_serializing_if = "no_views")]
    saved_views: [Option<crate::SavedView>; 8],
    history: Vec<HistoryStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    future: Vec<HistoryStep>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<crate::Folder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    item_ranks: Vec<(ItemId, f64)>,
    /// Visualization; decal pictures are saved beside the reference pictures, as images/<id>.<ext>.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    decal_images: Vec<crate::DecalImage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    decals: Vec<crate::Decal>,
    #[serde(default)]
    visual: crate::VisualSettings,
    /// The Shapr3D project this one came from; its file is saved beside it as source.shapr.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shapr: Option<crate::ShaprLink>,
}

/// The Shapr3D file a project came from.
pub const SHAPR_SOURCE: &str = "source.shapr";

#[derive(Serialize, Deserialize)]
struct BodyRecord {
    id: ItemId,
    name: String,
    visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    appearance: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    face_colors: Vec<(u32, [f32; 3])>,
}

#[derive(Serialize, Deserialize)]
struct SketchRecord {
    id: ItemId,
    name: String,
    plane: SketchPlane,
    entities: Vec<SketchEntity>,
    visible: bool,
    on_body: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    constraints: Vec<crate::Constraint>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    construction: std::collections::BTreeSet<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    patterns: Vec<crate::SketchPattern>,
}

/// Display tolerance used when meshing loaded bodies.
pub const DEFLECTION: f64 = 0.02;

/// The display meshes' deflection in use (Preferences ▸ Tessellation Quality; read when the app starts).
static DEFLECTION_NOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How far a display mesh may stray from the exact surface, in mm.
pub fn deflection() -> f64 {
    let bits = DEFLECTION_NOW.load(std::sync::atomic::Ordering::Relaxed);
    if bits == 0 { DEFLECTION } else { f64::from_bits(bits) }
}

/// Sets it (clamped to 0.005–1 mm).
pub fn set_deflection(mm: f64) {
    let v = if mm.is_finite() { mm.clamp(0.005, 1.0) } else { DEFLECTION };
    DEFLECTION_NOW.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

pub fn save(doc: &Document, kernel: &dyn Kernel, dir: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(dir)?;
    let file = file_of(doc);
    save_file(doc, file, kernel, dir)
}

/// The document as `project.json` holds it (the bodies' shapes go to STEP separately).
fn file_of(doc: &Document) -> ProjectFile {
    let (next_id, body_counter, sketch_counter, plane_counter, axis_counter) = doc.counters();
    ProjectFile {
        version: FILE_VERSION,
        legacy_face_axes: doc.legacy_face_axes,
        name: doc.name.clone(),
        units: doc.units,
        next_id,
        body_counter,
        sketch_counter,
        bodies_file: default_bodies_file(),
        bodies_check: None,
        bodies: doc
            .bodies
            .iter()
            .map(|b| BodyRecord {
                id: b.id,
                name: b.name.clone(),
                visible: b.visible,
                appearance: b.appearance.clone(),
                face_colors: b.face_colors.clone(),
            })
            .collect(),
        sketches: doc
            .sketches
            .iter()
            .map(|s| SketchRecord {
                id: s.id,
                name: s.name.clone(),
                plane: s.plane,
                entities: s.entities.clone(),
                visible: s.visible,
                on_body: s.on_body,
                constraints: s.constraints.clone(),
                construction: s.construction.clone(),
                patterns: s.patterns.clone(),
            })
            .collect(),
        plane_counter,
        planes: doc.planes.clone(),
        axis_counter,
        axes: doc.axes.clone(),
        drawings: doc.drawings.clone(),
        variables: doc.variables.clone(),
        saved_views: doc.saved_views,
        images: doc.images.clone(),
        history: doc.history.clone(),
        future: doc.future.clone(),
        folders: doc.folders.clone(),
        item_ranks: doc.item_ranks.clone(),
        decal_images: doc.decal_images.clone(),
        decals: doc.decals.clone(),
        visual: doc.visual.clone(),
        shapr: doc.shapr.clone(),
    }
}

/// Forces a written file to the disk, so a power cut can't leave a renamed file empty.
fn sync(path: &Path) -> Result<(), ProjectError> {
    patiently(|| fs::OpenOptions::new().write(true).open(path)?.sync_all())?;
    Ok(())
}

/// Waits out what briefly holds a file on Windows (virus scanners, search indexing, cloud sync), which makes an operation
/// fail where it would have worked a moment later.
fn patiently<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut wait = 10;
    loop {
        match op() {
            // Access denied, sharing violation, lock violation.
            Err(e) if cfg!(windows) && wait <= 640 && matches!(e.raw_os_error(), Some(5 | 32 | 33)) => {
                std::thread::sleep(std::time::Duration::from_millis(wait));
                wait *= 2;
            }
            result => return result,
        }
    }
}

/// Puts a file or folder at a new path, replacing a file already there in one step.
pub fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    patiently(|| fs::rename(from, to))
}

/// Writes a whole file so that a reader, or a crash, sees the old contents or the new, never part of them.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let tmp = tmp_name(path);
    fs::write(&tmp, bytes)?;
    sync(&tmp)?;
    replace(&tmp, path)?;
    Ok(())
}

/// The temporary a file is written under: hidden, and a name the sync apps leave alone (Dropbox skips `.~` names), so
/// a half-written file is never uploaded.
fn tmp_name(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".~{name}.tmp"))
}

/// Size and FNV-1a hash of a file's bytes.
pub fn file_check(path: &Path) -> std::io::Result<(u64, u64)> {
    let bytes = fs::read(path)?;
    let mut h: u64 = 0xcbf29ce484222325;
    for b in &bytes {
        h = (h ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    Ok((bytes.len() as u64, h))
}

/// A failed or interrupted save must leave the last good project. The bodies go to the name the project isn't using,
/// and renaming `project.json` into place, last, is the one moment the save takes effect.
fn save_file(doc: &Document, mut file: ProjectFile, kernel: &dyn Kernel, dir: &Path) -> Result<(), ProjectError> {
    let current = fs::read(dir.join(PROJECT_FILE)).ok().and_then(|b| serde_json::from_slice::<BodiesRef>(&b).ok());
    let using_first = current.is_some_and(|c| c.bodies_file == BODIES_FILE);
    file.bodies_file = if using_first { BODIES_FILE_NEXT } else { BODIES_FILE }.to_string();
    if !doc.bodies.is_empty() {
        let step_path = dir.join(&file.bodies_file);
        let step_tmp = tmp_name(&step_path);
        let shapes: Vec<_> = doc.bodies.iter().map(|b| b.shape.clone()).collect();
        kernel.write_step(&shapes, &step_tmp)?;
        sync(&step_tmp)?;
        file.bodies_check = Some(file_check(&step_tmp)?);
        replace(&step_tmp, &step_path)?;
    }
    // Pictures beside the project, named by item id; files of removed pictures go.
    let images_dir = dir.join(IMAGES_DIR);
    let files: Vec<(String, &[u8])> = doc
        .images
        .iter()
        .map(|i| (format!("{}.{}", i.id.0, i.ext), i.file.as_slice()))
        .chain(doc.decal_images.iter().map(|i| (format!("{}.{}", i.id.0, i.ext), i.file.as_slice())))
        .chain(doc.visual.panoramas.iter().map(|i| (format!("{}.{}", i.id.0, i.ext), i.file.as_slice())))
        .collect();
    if !files.is_empty() {
        fs::create_dir_all(&images_dir)?;
    }
    let keep: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
    for (name, bytes) in &files {
        let path = images_dir.join(name);
        if fs::metadata(&path).map(|m| m.len()).ok() != Some(bytes.len() as u64) {
            fs::write(&path, bytes)?;
        }
    }
    if let Ok(entries) = fs::read_dir(&images_dir) {
        for e in entries.flatten() {
            if !keep.iter().any(|k| *k == e.file_name().to_string_lossy()) {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    // The Shapr3D file the project came from, beside it (written when it changed).
    let source = dir.join(SHAPR_SOURCE);
    match &doc.shapr {
        Some(link) if !link.file.is_empty() => {
            if fs::metadata(&source).map(|m| m.len()).ok() != Some(link.file.len() as u64) {
                fs::write(&source, link.file.as_slice())?;
            }
        }
        _ => {
            let _ = fs::remove_file(&source);
        }
    }
    write_atomic(&dir.join(PROJECT_FILE), &serde_json::to_vec_pretty(&file)?)?;
    // A save cut short by an older version left its temporary under the plain name.
    let _ = fs::remove_file(dir.join("project.json.tmp"));
    // Saved. The old bodies go, and with no bodies neither name is in use.
    for name in [BODIES_FILE, BODIES_FILE_NEXT] {
        if name != file.bodies_file || doc.bodies.is_empty() {
            let _ = fs::remove_file(dir.join(name));
        }
    }
    Ok(())
}

pub fn load(kernel: &dyn Kernel, dir: &Path) -> Result<Document, ProjectError> {
    let bytes = fs::read(dir.join(PROJECT_FILE))?;
    let file: ProjectFile = serde_json::from_slice(&bytes)?;
    if file.version > FILE_VERSION {
        return Err(ProjectError::Invalid(format!("made by a newer ShapeCraft (file version {})", file.version)));
    }
    // Only ever one of our two names: this comes from a file and picks a path to read.
    if ![BODIES_FILE, BODIES_FILE_NEXT].contains(&file.bodies_file.as_str()) {
        return Err(ProjectError::Invalid(format!("unknown bodies file {:?}", file.bodies_file)));
    }
    if let Some(want) = file.bodies_check
        && !file.bodies.is_empty()
    {
        let path = dir.join(&file.bodies_file);
        // There but not the one saved with it: an older file of the same name, the new one still on its way.
        // Missing: on its way if project.json just arrived; long missing, it's a copy that lost it (rebuilt then).
        let recent = fs::metadata(dir.join(PROJECT_FILE))
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|e| e < std::time::Duration::from_secs(600)));
        match file_check(&path) {
            Ok(got) if got != want => return Err(ProjectError::StillSyncing),
            Err(_) if recent => return Err(ProjectError::StillSyncing),
            _ => {}
        }
    }
    let shapes = if file.bodies.is_empty() { Vec::new() } else { read_bodies(kernel, &dir.join(&file.bodies_file), file.bodies.len())? };
    build(file, shapes, kernel, Some(dir))
}

/// The bodies, one shape per body as they were written; a missing file says so (a project copied or synced
/// without it reads "STEP import failed" otherwise). Files read the import way, solid by solid, are the fallback.
fn read_bodies(kernel: &dyn Kernel, path: &Path, want: usize) -> Result<Vec<shapecraft_kernel::Shape>, ProjectError> {
    if !path.exists() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(ProjectError::Bodies(format!("its bodies file {name} is missing")));
    }
    let roots = kernel.read_step_roots(path)?;
    if roots.len() == want {
        return Ok(roots);
    }
    match kernel.read_step(path) {
        Ok(solids) if solids.len() == want => Ok(solids),
        _ => Err(ProjectError::Bodies(format!("the bodies file has {} bodies, project.json lists {want}", roots.len()))),
    }
}

/// The project without its bodies (sketches, planes, History and the rest), for when its bodies file can't be read:
/// the History can make the bodies again. The unreadable file is kept beside it as `bodies-unreadable.step`.
pub fn load_without_bodies(kernel: &dyn Kernel, dir: &Path) -> Result<Document, ProjectError> {
    let bytes = fs::read(dir.join(PROJECT_FILE))?;
    let mut file: ProjectFile = serde_json::from_slice(&bytes)?;
    if file.version > FILE_VERSION {
        return Err(ProjectError::Invalid(format!("made by a newer ShapeCraft (file version {})", file.version)));
    }
    if [BODIES_FILE, BODIES_FILE_NEXT].contains(&file.bodies_file.as_str()) {
        let bad = dir.join(&file.bodies_file);
        let keep = dir.join(UNREADABLE_BODIES_FILE);
        if bad.exists() && !keep.exists() {
            let _ = fs::copy(&bad, &keep);
        }
    }
    file.bodies.clear();
    for s in &mut file.sketches {
        s.on_body = None;
    }
    build(file, Vec::new(), kernel, Some(dir))
}

/// Where `load_without_bodies` keeps a bodies file it couldn't read.
pub const UNREADABLE_BODIES_FILE: &str = "bodies-unreadable.step";

/// A document from its `project.json` contents and its bodies' shapes; pictures are read from `dir` when given.
fn build(
    file: ProjectFile,
    shapes: Vec<shapecraft_kernel::Shape>,
    kernel: &dyn Kernel,
    dir: Option<&Path>,
) -> Result<Document, ProjectError> {
    if shapes.len() != file.bodies.len() {
        return Err(ProjectError::Bodies(format!("the bodies file has {} bodies, project.json lists {}", shapes.len(), file.bodies.len())));
    }
    let mut doc = Document::new(file.name);
    doc.units = file.units;
    doc.legacy_face_axes = file.legacy_face_axes || file.version < 2;
    doc.set_counters(file.next_id, file.body_counter, file.sketch_counter, file.plane_counter, file.axis_counter);
    // Pictures whose files are missing or unreadable are left out rather than failing the whole project.
    doc.images = file
        .images
        .into_iter()
        .filter(|i| matches!(i.ext.as_str(), "png" | "jpg") && i.size.is_finite() && i.size.min_element() > 0.0)
        .take(1000)
        .filter_map(|mut i| {
            let bytes = fs::read(dir?.join(IMAGES_DIR).join(format!("{}.{}", i.id.0, i.ext))).ok()?;
            let (_, pixels) = crate::RefImage::decode(&bytes).ok()?;
            i.file = std::sync::Arc::new(bytes);
            i.pixels = std::sync::Arc::new(pixels);
            Some(i)
        })
        .collect();
    doc.decal_images = file
        .decal_images
        .into_iter()
        .filter(|i| matches!(i.ext.as_str(), "png" | "jpg"))
        .take(1000)
        .filter_map(|mut i| {
            let bytes = fs::read(dir?.join(IMAGES_DIR).join(format!("{}.{}", i.id.0, i.ext))).ok()?;
            let (_, pixels) = crate::RefImage::decode(&bytes).ok()?;
            i.file = std::sync::Arc::new(bytes);
            i.pixels = std::sync::Arc::new(pixels);
            Some(i)
        })
        .collect();
    // Custom environments whose files are missing or unreadable are left out (sanitized() then lets go of one that
    // was selected).
    let mut visual = file.visual;
    let selected = visual.panorama().map(|p| p.id);
    visual.panoramas = std::mem::take(&mut visual.panoramas)
        .into_iter()
        .filter(|p| matches!(p.ext.as_str(), "hdr" | "exr" | "png" | "jpg"))
        .take(crate::MAX_PANORAMAS)
        .filter_map(|mut p| {
            let bytes = fs::read(dir?.join(IMAGES_DIR).join(format!("{}.{}", p.id.0, p.ext))).ok()?;
            let (_, pixels) = crate::Panorama::decode(&bytes).ok()?;
            p.file = Arc::new(bytes);
            p.pixels = Arc::new(pixels);
            Some(p)
        })
        .collect();
    if let Some(id) = selected {
        visual.environment = visual.panoramas.iter().position(|p| p.id == id).map_or(0, |k| crate::BUILTIN_ENVIRONMENTS + k);
    }
    doc.visual = visual.sanitized();
    // A link whose file is gone is dropped: exports then write a fresh Shapr3D project.
    doc.shapr = file.shapr.and_then(|mut link| {
        let bytes = fs::read(dir?.join(SHAPR_SOURCE)).ok()?;
        link.file = Arc::new(bytes);
        link.bodies.truncate(100_000);
        Some(link)
    });
    doc.saved_views = file.saved_views.map(|v| {
        v.filter(|v| [v.yaw, v.pitch, v.distance, v.ortho, v.roll].iter().chain(&v.target).all(|x| x.is_finite()) && v.distance > 0.0)
    });
    doc.variables = file.variables.into_iter().filter(|v| crate::expr::valid_name(&v.name)).take(1000).collect();
    doc.drawings = file
        .drawings
        .into_iter()
        .filter(|d| DrawingSheet::paper(&d.sheet, d.landscape).is_some() && d.scale.is_none_or(|s| s.is_finite() && s > 0.0 && s < 1e4))
        .take(1000)
        .collect();
    doc.axes = file
        .axes
        .into_iter()
        .filter(|a| {
            a.origin.is_finite() && a.dir.is_finite() && a.dir.length() > 1e-6 && a.length.is_finite() && a.length > 0.0 && a.length < 1e7
        })
        .collect();
    // Hostile input: keep only planes with a finite size and a finite, non-degenerate frame.
    doc.planes = file
        .planes
        .into_iter()
        .filter(|p| {
            let f = &p.plane;
            p.size.is_finite()
                && p.size > 0.0
                && p.size < 1e7
                && f.origin.is_finite()
                && f.x.is_finite()
                && f.y.is_finite()
                && f.normal().length() > 1e-6
        })
        .collect();
    for (rec, shape) in file.bodies.into_iter().zip(shapes) {
        let mesh = Arc::new(kernel.tessellate(&shape, deflection())?);
        let appearance = rec.appearance.and_then(Appearance::sanitized);
        let face_colors = rec
            .face_colors
            .into_iter()
            .filter(|(_, c)| c.iter().all(|v| v.is_finite()))
            .map(|(f, c)| (f, c.map(|v| v.clamp(0.0, 1.0))))
            .take(10_000)
            .collect();
        doc.bodies.push(Body { id: rec.id, name: rec.name, shape, mesh, visible: rec.visible, appearance, face_colors });
    }
    let (bodies, pictures) = (&doc.bodies, &doc.decal_images);
    doc.decals = file
        .decals
        .into_iter()
        .filter(|d| d.valid() && bodies.iter().any(|b| b.id == d.body) && pictures.iter().any(|i| i.id == d.image))
        .map(|mut d| {
            d.scale = d.scale.clamp(crate::DECAL_SCALE.0, crate::DECAL_SCALE.1);
            d.rotate = d.rotate.clamp(-180.0, 180.0);
            d.opacity = d.opacity.clamp(0.0, 1.0);
            d
        })
        .take(10_000)
        .collect();
    doc.sketches = file
        .sketches
        .into_iter()
        .map(|s| {
            let mut sk = Sketch::new(s.id, s.name, s.plane, s.on_body);
            sk.entities = s.entities;
            sk.visible = s.visible;
            sk.constraints = s.constraints;
            sk.construction = s.construction;
            sk.patterns = s.patterns;
            sk
        })
        .collect();
    doc.history = file.history;
    doc.future = file.future;
    // Folders keep only items (and folders) that exist, each in one folder, none inside itself.
    let mut seen = std::collections::HashSet::new();
    let folder_ids: std::collections::HashSet<ItemId> = file.folders.iter().map(|f| f.id).collect();
    doc.folders = file
        .folders
        .into_iter()
        .take(10_000)
        .map(|mut f| {
            let own = f.id;
            f.items.retain(|i| *i != own && (doc.has_item(*i) || folder_ids.contains(i)) && seen.insert(*i));
            f
        })
        .collect();
    for k in 0..doc.folders.len() {
        let id = doc.folders[k].id;
        if let Some(parent) = doc.folder_of(id)
            && doc.is_within(parent, id)
            && let Some(p) = doc.folder_mut(parent)
        {
            p.items.retain(|i| *i != id);
        }
    }
    doc.item_ranks = file.item_ranks.into_iter().filter(|r| r.1.is_finite()).take(100_000).collect();
    Ok(doc)
}

/// The whole document but its history and pictures, as JSON with the bodies' STEP text inside: History ▸ Merge
/// Modeling Steps keeps the merged model in its one step this way, so it replays and saves like any other step.
pub fn snapshot(doc: &Document, kernel: &dyn Kernel) -> Result<serde_json::Value, ProjectError> {
    let mut file = file_of(doc);
    file.history.clear();
    file.future.clear();
    file.images.clear();
    file.decal_images.clear();
    file.decals.clear();
    let step = if doc.bodies.is_empty() {
        String::new()
    } else {
        let tmp = scratch_step();
        let shapes: Vec<_> = doc.bodies.iter().map(|b| b.shape.clone()).collect();
        let written = kernel
            .write_step(&shapes, &tmp)
            .map_err(ProjectError::from)
            .and_then(|()| fs::read_to_string(&tmp).map_err(ProjectError::from));
        let _ = fs::remove_file(&tmp);
        written?
    };
    Ok(serde_json::json!({ "file": serde_json::to_value(&file)?, "step": step }))
}

/// The document a [`snapshot`] holds (no history).
pub fn from_snapshot(value: &serde_json::Value, kernel: &dyn Kernel) -> Result<Document, ProjectError> {
    let file: ProjectFile = serde_json::from_value(value.get("file").cloned().unwrap_or_default())?;
    let step = value.get("step").and_then(serde_json::Value::as_str).unwrap_or("");
    let shapes = if file.bodies.is_empty() {
        Vec::new()
    } else {
        let tmp = scratch_step();
        let read = fs::write(&tmp, step).map_err(ProjectError::from).and_then(|()| kernel.read_step(&tmp).map_err(ProjectError::from));
        let _ = fs::remove_file(&tmp);
        read?
    };
    build(file, shapes, kernel, None)
}

/// A temporary STEP file name of our own (the kernel reads and writes STEP through files).
fn scratch_step() -> PathBuf {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("shapecraft-snapshot-{}-{n}.step", std::process::id()))
}

/// What the landing screen needs to show a project without loading its geometry.
#[derive(Clone, Debug)]
pub struct ProjectSummary {
    pub dir: PathBuf,
    pub name: String,
    pub modified: std::time::SystemTime,
    pub thumbnail: Option<PathBuf>,
    /// Other versions a sync app kept beside it after two computers saved it at once (`other_versions`).
    pub versions: usize,
}

#[derive(Deserialize)]
struct NameOnly {
    name: String,
}

/// Lists project folders under `root`, newest first.
pub fn list(root: &Path) -> Vec<ProjectSummary> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<ProjectSummary> = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            // Hidden: a copy still being put together, Recently Deleted.
            if hidden(&dir) {
                return None;
            }
            let json = dir.join(PROJECT_FILE);
            let meta = fs::metadata(&json).ok()?;
            let name = fs::read(&json)
                .ok()
                .and_then(|b| serde_json::from_slice::<NameOnly>(&b).ok())
                .map(|n| n.name)
                .unwrap_or_else(|| e.file_name().to_string_lossy().into_owned());
            let thumb = dir.join(THUMBNAIL_FILE);
            Some(ProjectSummary {
                modified: meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                thumbnail: thumb.exists().then_some(thumb),
                versions: other_versions(&dir).len(),
                name,
                dir,
            })
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.modified));
    out
}

/// The other project.json files a sync app left in a project folder when two computers saved it at once:
/// Dropbox "project (Mike's conflicted copy 2026-11-06).json", OneDrive "project-DESKTOP-4F2.json", Google Drive
/// "project (1).json", iCloud "project 2.json".
pub fn other_versions(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n != PROJECT_FILE && n.starts_with("project") && n.ends_with(".json") && n.len() > PROJECT_FILE.len()
        })
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

/// Makes one of those other versions a project of its own beside this one ("<name> - Other Version"), with the
/// bodies file that was saved with it when one of the files here is it (otherwise its bodies are rebuilt from its
/// History when it opens). The version file leaves the original folder.
pub fn split_version(dir: &Path, other: &Path) -> Result<PathBuf, ProjectError> {
    let bytes = fs::read(other)?;
    let mut file: serde_json::Value = serde_json::from_slice(&bytes)?;
    let name = format!("{} - Other Version", file.get("name").and_then(|n| n.as_str()).unwrap_or("Project"));
    let bodies_file = file.get("bodies_file").and_then(|n| n.as_str()).unwrap_or(BODIES_FILE).to_string();
    if ![BODIES_FILE, BODIES_FILE_NEXT].contains(&bodies_file.as_str()) {
        return Err(ProjectError::Invalid(format!("unknown bodies file {bodies_file:?}")));
    }
    let want = file.get("bodies_check").and_then(|c| Some((c.get(0)?.as_u64()?, c.get(1)?.as_u64()?)));
    let parent = dir.parent().ok_or_else(|| ProjectError::Invalid("no parent folder".into()))?;
    let to = unique_dir(parent, &name);
    let staging = parent.join(format!(".~copying {}", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let mut bodies_from = None;
    for e in fs::read_dir(dir)?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        if n.starts_with('.') || n.ends_with(".tmp") || (n.starts_with("project") && n.ends_with(".json")) {
            continue;
        }
        if n.ends_with(".step") {
            // Whichever .step here (the sync app may have renamed it too) was saved with this version.
            if want.is_some() && file_check(&path).ok() == want {
                bodies_from = Some(path);
            }
            continue;
        }
        if path.is_dir() {
            copy_tree(&path, &staging.join(&n))?;
        } else {
            fs::copy(&path, staging.join(&n))?;
        }
    }
    if let Some(b) = bodies_from {
        fs::copy(b, staging.join(&bodies_file))?;
    }
    file["name"] = serde_json::Value::String(name);
    fs::write(staging.join(PROJECT_FILE), serde_json::to_vec_pretty(&file)?)?;
    replace(&staging, &to)?;
    fs::remove_file(other)?;
    Ok(to)
}

/// Copies a folder's files and folders (pictures, under a project).
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let dst = to.join(e.file_name());
        if e.path().is_dir() { copy_tree(&e.path(), &dst)? } else { fs::copy(e.path(), dst).map(|_| ())? }
    }
    Ok(())
}

/// Says a project is open on this computer: a small hidden file the sync app carries to the others, renewed every
/// minute while it's open (`mark_open`) and removed when it closes.
fn open_mark(dir: &Path, computer: &str) -> PathBuf {
    dir.join(format!(".open-{computer}.json"))
}

pub fn mark_open(dir: &Path, computer: &str) {
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = write_atomic(&open_mark(dir, computer), serde_json::json!({ "computer": computer, "at": at }).to_string().as_bytes());
}

pub fn mark_closed(dir: &Path, computer: &str) {
    let _ = fs::remove_file(open_mark(dir, computer));
}

/// Another computer that has this project open (seen within `within`): its name and how long ago.
pub fn open_elsewhere(dir: &Path, computer: &str, within: std::time::Duration) -> Option<(String, std::time::Duration)> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let mine = open_mark(dir, computer);
    fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        if !(n.starts_with(".open-") && n.ends_with(".json")) || e.path() == mine {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&fs::read(e.path()).ok()?).ok()?;
        let ago = std::time::Duration::from_secs(now.saturating_sub(v.get("at")?.as_u64()?));
        let who = v.get("computer")?.as_str()?.to_string();
        (ago <= within).then_some((who, ago))
    })
}

/// A folder name for a new project that doesn't collide with existing ones.
pub fn unique_dir(root: &Path, name: &str) -> PathBuf {
    let clean = folder_name(name);
    let mut dir = root.join(&clean);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{clean} {n}"));
        n += 1;
    }
    dir
}

/// A project's folder name, any project name being allowed: safe on all three systems, since projects move between them,
/// and short enough that the longest file inside still fits Windows' 260-character paths under an ordinary home folder.
fn folder_name(name: &str) -> String {
    let clean: String = name.chars().take(100).map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let clean = clean.trim();
    let upper = clean.to_uppercase();
    let numbered = upper.strip_prefix("COM").or(upper.strip_prefix("LPT")).is_some_and(|n| {
        let mut digits = n.chars();
        digits.next().is_some_and(|d| d.is_ascii_digit() || "¹²³".contains(d)) && digits.next().is_none()
    });
    match clean {
        "" => "Untitled Project".to_string(),
        // Names Windows keeps for devices, in any case.
        _ if numbered || ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str()) => format!("{clean}_"),
        _ => clean.to_string(),
    }
}

/// Where Delete puts projects and folders (inside the projects root), so nothing is lost by a slip.
pub const TRASH_DIR: &str = ".Recently Deleted";

fn hidden(p: &Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'))
}

/// Folders (not projects) directly inside `dir`, by name.
pub fn folders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !hidden(p) && !p.join(PROJECT_FILE).exists()).collect();
    out.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    out
}

/// Projects anywhere under `root` (a few folders deep), newest first: the Recents list.
pub fn list_all(root: &Path) -> Vec<ProjectSummary> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<ProjectSummary>) {
        out.extend(list(dir));
        if depth > 0 {
            for f in folders(dir) {
                walk(&f, depth - 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 4, &mut out);
    out.sort_by_key(|p| std::cmp::Reverse(p.modified));
    out
}

/// A new, empty folder inside `parent`: "Untitled Folder", then "Untitled Folder 1", "Untitled Folder 2" … (as the
/// reference names them).
pub fn new_folder(parent: &Path) -> Result<PathBuf, ProjectError> {
    let mut dir = parent.join("Untitled Folder");
    let mut n = 1;
    while dir.exists() {
        dir = parent.join(format!("Untitled Folder {n}"));
        n += 1;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// File ▸ New Folder with Selection: a new folder beside `item` (a project or a folder), with `item` moved into it.
/// Returns the new folder.
pub fn new_folder_with(item: &Path) -> Result<PathBuf, ProjectError> {
    let parent = item.parent().ok_or_else(|| ProjectError::Invalid("no parent folder".into()))?;
    let name = item.file_name().ok_or_else(|| ProjectError::Invalid("no name".into()))?.to_owned();
    let dir = new_folder(parent)?;
    if let Err(e) = replace(item, &dir.join(name)) {
        // Leave nothing half done: an empty folder isn't what was asked for.
        let _ = fs::remove_dir(&dir);
        return Err(e.into());
    }
    Ok(dir)
}

/// Renames a project (its name and its folder) or a plain folder. Returns the new path.
pub fn rename(dir: &Path, name: &str) -> Result<PathBuf, ProjectError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(ProjectError::Invalid("name must be 1–200 characters".into()));
    }
    let parent = dir.parent().ok_or_else(|| ProjectError::Invalid("no parent folder".into()))?;
    // The folder first: if it can't move, the project keeps its name instead of ending up with a name its folder doesn't have.
    let to = if dir.file_name().is_some_and(|n| n.to_string_lossy() == folder_name(name)) {
        dir.to_path_buf()
    } else {
        let to = unique_dir(parent, name);
        replace(dir, &to)?;
        to
    };
    let json = to.join(PROJECT_FILE);
    if json.exists() {
        let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&json)?)?;
        v["name"] = serde_json::Value::String(name.to_string());
        write_atomic(&json, &serde_json::to_vec_pretty(&v)?)?;
    }
    Ok(to)
}

/// Copies a project next to itself as "<name> copy", with its pictures.
pub fn duplicate(dir: &Path) -> Result<PathBuf, ProjectError> {
    let json = dir.join(PROJECT_FILE);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&json)?)?;
    let name = format!("{} copy", v["name"].as_str().unwrap_or("Untitled Project"));
    let parent = dir.parent().ok_or_else(|| ProjectError::Invalid("no parent folder".into()))?;
    let to = unique_dir(parent, &name);
    fs::create_dir_all(&to)?;
    for sub in [Path::new(""), Path::new(IMAGES_DIR)] {
        let Ok(entries) = fs::read_dir(dir.join(sub)) else { continue };
        for e in entries.flatten().filter(|e| e.path().is_file()) {
            fs::create_dir_all(to.join(sub))?;
            fs::copy(e.path(), to.join(sub).join(e.file_name()))?;
        }
    }
    v["name"] = serde_json::Value::String(name);
    write_atomic(&to.join(PROJECT_FILE), &serde_json::to_vec_pretty(&v)?)?;
    Ok(to)
}

/// Copies a project or a folder of them into `parent` (another location: a cloud folder, say), under the same name
/// or a free one beside it. It's put together under a hidden name first and renamed into place once complete, so a
/// sync app never uploads half a project. Unfinished saves (`*.tmp`) aren't copied.
pub fn copy_to(item: &Path, parent: &Path) -> Result<PathBuf, ProjectError> {
    fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for e in fs::read_dir(from)?.flatten() {
            let (src, dst) = (e.path(), to.join(e.file_name()));
            if src.is_dir() {
                copy_tree(&src, &dst)?;
            } else if src.extension().is_none_or(|x| x != "tmp") {
                fs::copy(&src, &dst)?;
            }
        }
        Ok(())
    }
    if parent.starts_with(item) {
        return Err(ProjectError::Invalid("can't copy a folder into itself".into()));
    }
    let name = item.file_name().ok_or_else(|| ProjectError::Invalid("no name".into()))?.to_string_lossy().into_owned();
    fs::create_dir_all(parent)?;
    let to = unique_dir(parent, &name);
    let staging = parent.join(format!(".~copying {}", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let _ = fs::remove_dir_all(&staging);
    let done = copy_tree(item, &staging).and_then(|()| replace(&staging, &to));
    if let Err(e) = done {
        let _ = fs::remove_dir_all(&staging);
        return Err(e.into());
    }
    Ok(to)
}

/// Every file under `dir` (but unfinished saves) with its size, relative paths sorted: two folders with the same list
/// hold the same project as far as a copy can tell.
fn manifest(dir: &Path) -> std::io::Result<Vec<(PathBuf, u64)>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, u64)>) -> std::io::Result<()> {
        for e in fs::read_dir(dir)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out)?;
            } else if p.extension().is_none_or(|x| x != "tmp") {
                out.push((p.strip_prefix(base).unwrap_or(&p).to_path_buf(), e.metadata()?.len()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

/// Moves a project or folder into `parent`, which may be on another disk or in a cloud folder. A plain rename when it
/// can be; otherwise a copy, checked file by file, and only then is the original removed.
pub fn move_to(item: &Path, parent: &Path) -> Result<PathBuf, ProjectError> {
    if item.parent() == Some(parent) {
        return Ok(item.to_path_buf());
    }
    if parent.starts_with(item) {
        return Err(ProjectError::Invalid("can't move a folder into itself".into()));
    }
    let name = item.file_name().ok_or_else(|| ProjectError::Invalid("no name".into()))?.to_string_lossy().into_owned();
    fs::create_dir_all(parent)?;
    let to = unique_dir(parent, &name);
    if replace(item, &to).is_ok() {
        return Ok(to);
    }
    let to = copy_to(item, parent)?;
    if manifest(item)? != manifest(&to)? {
        let _ = fs::remove_dir_all(&to);
        return Err(ProjectError::Invalid("the copy didn't match, so the project was left where it was".into()));
    }
    patiently(|| fs::remove_dir_all(item))?;
    Ok(to)
}

/// Moves a project or folder into the root's Recently Deleted folder.
pub fn trash(dir: &Path, root: &Path) -> Result<PathBuf, ProjectError> {
    if dir == root || !dir.starts_with(root) {
        return Err(ProjectError::Invalid("only items inside the projects folder can be deleted".into()));
    }
    let bin = root.join(TRASH_DIR);
    fs::create_dir_all(&bin)?;
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let to = unique_dir(&bin, &name);
    replace(dir, &to)?;
    Ok(to)
}

fn no_views(v: &[Option<crate::SavedView>; 8]) -> bool {
    v.iter().all(Option::is_none)
}

#[cfg(test)]
mod file_ops_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn new_folder_names_and_new_folder_with_selection() {
        let root = std::env::temp_dir().join(format!("sc-newfolder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Part")).unwrap();
        fs::write(root.join("Part").join(PROJECT_FILE), b"{}").unwrap();
        assert_eq!(new_folder(&root).unwrap(), root.join("Untitled Folder"));
        let wrapped = new_folder_with(&root.join("Part")).unwrap();
        assert_eq!(wrapped, root.join("Untitled Folder 1"));
        assert!(wrapped.join("Part").join(PROJECT_FILE).exists());
        assert!(!root.join("Part").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn folders_rename_duplicate_and_trash() {
        let root = std::env::temp_dir().join(format!("shapecraft-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let folder = new_folder(&root).unwrap();
        assert_eq!(folders(&root), vec![folder.clone()]);
        let proj = folder.join("Bracket");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join(PROJECT_FILE), br#"{"name": "Bracket", "other": 1}"#).unwrap();
        assert!(list(&root).is_empty(), "projects in folders aren't at the top");
        assert_eq!(list_all(&root).len(), 1, "but Recents finds them");
        let copy = duplicate(&proj).unwrap();
        assert_eq!(list(&folder).len(), 2);
        let renamed = rename(&copy, "Bracket v2").unwrap();
        assert!(renamed.ends_with("Bracket v2"));
        let v: serde_json::Value = serde_json::from_slice(&fs::read(renamed.join(PROJECT_FILE)).unwrap()).unwrap();
        assert_eq!(v["name"], "Bracket v2");
        assert_eq!(v["other"], 1, "other fields survive");
        let gone = trash(&renamed, &root).unwrap();
        assert!(gone.starts_with(root.join(TRASH_DIR)));
        assert_eq!(list_all(&root).len(), 1, "Recently Deleted is hidden");
        assert!(trash(&root, &root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_and_move_between_locations() {
        let base = std::env::temp_dir().join(format!("sc-locations-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (here, cloud) = (base.join("Local"), base.join("Cloud/ShapeCraft"));
        let proj = here.join("Bracket");
        fs::create_dir_all(proj.join(IMAGES_DIR)).unwrap();
        fs::write(proj.join(PROJECT_FILE), br#"{"name": "Bracket"}"#).unwrap();
        fs::write(proj.join(BODIES_FILE), b"ISO-10303-21;").unwrap();
        fs::write(proj.join(IMAGES_DIR).join("1.png"), b"png").unwrap();
        fs::write(proj.join("project.json.tmp"), b"half a save").unwrap();
        let copy = copy_to(&proj, &cloud).unwrap();
        assert_eq!(copy, cloud.join("Bracket"));
        assert!(copy.join(IMAGES_DIR).join("1.png").exists());
        assert!(!copy.join("project.json.tmp").exists(), "unfinished saves stay behind");
        assert!(proj.exists(), "a copy leaves the original");
        assert_eq!(fs::read_dir(&cloud).unwrap().count(), 1, "no staging folder left over");
        // A second copy gets a free name beside the first.
        assert_eq!(copy_to(&proj, &cloud).unwrap(), cloud.join("Bracket 2"));
        let moved = move_to(&proj, &cloud).unwrap();
        assert_eq!(moved, cloud.join("Bracket 3"));
        assert!(!proj.exists());
        assert_eq!(manifest(&moved).unwrap(), manifest(&copy).unwrap());
        assert_eq!(list(&cloud).len(), 3);
        assert!(copy_to(&cloud, &cloud.join("Bracket")).is_err(), "not into itself");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn any_name_makes_a_folder_every_system_can_hold() {
        for (name, folder) in [
            ("a/b\\c:d*e?f\"g<h>i|j", "a_b_c_d_e_f_g_h_i_j"),
            ("../../etc", "______etc"),
            ("trailing dot.", "trailing dot_"),
            ("  spaces  ", "spaces"),
            ("", "Untitled Project"),
            ("CON", "CON_"),
            ("nul", "nul_"),
            ("Lpt1", "Lpt1_"),
            ("COM¹", "COM¹_"),
            ("COMPUTER", "COMPUTER"),
            ("部品 café", "部品 café"),
        ] {
            assert_eq!(folder_name(name), folder, "{name:?}");
        }
        assert_eq!(folder_name(&"長".repeat(300)).chars().count(), 100);
    }

    #[test]
    fn renaming_keeps_a_project_consistent_and_a_name_its_folder_cant_show() {
        let root = std::env::temp_dir().join(format!("shapecraft-rename-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let proj = root.join("Old");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join(PROJECT_FILE), br#"{"name": "Old"}"#).unwrap();
        let renamed = rename(&proj, "Part: v1.2?").unwrap();
        assert!(renamed.ends_with("Part_ v1_2_"));
        let name =
            |dir: &Path| serde_json::from_slice::<serde_json::Value>(&fs::read(dir.join(PROJECT_FILE)).unwrap()).unwrap()["name"].clone();
        assert_eq!(name(&renamed), "Part: v1.2?");
        assert_eq!(rename(&renamed, "Part: v1.2?").unwrap(), renamed, "the same name doesn't move it to a numbered folder");
        assert!(rename(&renamed, &"x".repeat(201)).is_err());
        assert_eq!(name(&renamed), "Part: v1.2?", "a refused name changes nothing");
        assert!(rename(&root.join("Not there"), "New").is_err());
        assert!(fs::read_dir(&renamed).unwrap().flatten().all(|e| !e.path().to_string_lossy().ends_with(".tmp")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_duplicate_keeps_its_pictures() {
        let root = std::env::temp_dir().join(format!("shapecraft-dup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let proj = root.join("Plate");
        fs::create_dir_all(proj.join(IMAGES_DIR)).unwrap();
        fs::write(proj.join(PROJECT_FILE), br#"{"name": "Plate"}"#).unwrap();
        fs::write(proj.join(IMAGES_DIR).join("7.png"), b"picture").unwrap();
        let copy = duplicate(&proj).unwrap();
        assert_eq!(fs::read(copy.join(IMAGES_DIR).join("7.png")).unwrap(), b"picture");
        let _ = fs::remove_dir_all(&root);
    }
}

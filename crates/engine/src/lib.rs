//! The editing session: a document, the kernel, undo/redo, the selection, and every command.
//!
//! Every user-visible action is a [`CommandSpec`] in [`registry`]. The UI, command search, the CLI, the
//! control channel and MCP all run commands through [`Session::execute`]. Commands never open dialogs.

#![forbid(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use shapecraft_doc::{Document, ItemId, Pick, Sketch};
use shapecraft_kernel::Kernel;

pub mod cmd;
pub mod drawing;
pub mod formats;
pub mod history;
mod keep;
pub mod meshops;
mod params;
mod refs;
pub use cmd::{CommandSpec, face_offset, offset_entities, registry};
pub use params::Params;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("unknown command '{0}'")]
    UnknownCommand(String),
    #[error("{0}")]
    BadParams(String),
    #[error("{0}")]
    NotAvailable(String),
    #[error("{0}")]
    Kernel(#[from] shapecraft_kernel::KernelError),
    #[error("{0}")]
    Project(#[from] shapecraft_doc::project::ProjectError),
    #[error("internal error in '{0}' (the project was restored)")]
    Panicked(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

const UNDO_LIMIT: usize = 200;

pub struct Session {
    pub doc: Document,
    pub kernel: Arc<dyn Kernel>,
    pub selection: Vec<Pick>,
    /// The sketch being edited, if in sketch mode.
    pub active_sketch: Option<ItemId>,
    /// Where the project lives on disk, once saved.
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// Goes up whenever the document changes (commands, undo, redo), for caches and autosave.
    pub revision: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Steps a command asks for after it is logged (a Shapr3D import's extrudes, redone as our own steps).
    pub(crate) followups: Vec<cmd::file::Followup>,
    /// A `.shapr` file's own history, taken once its import has been recorded.
    pub(crate) adopt: Option<cmd::file::OwnHistory>,
    /// The Shapr3D steps of the project an import is bringing in, for its History step.
    pub(crate) shapr_steps: Vec<[String; 2]>,
    /// A rebuild from the history: commands don't ask for follow-ups (they are steps of their own already).
    pub(crate) replaying: bool,
    /// What the History keeps for the command that just ran, when that isn't its params (an Unlinked Copy keeps
    /// the copies' geometry, so later edits of what it copied leave it alone).
    pub(crate) log_params: Option<Value>,
}

#[derive(Clone)]
struct Snapshot {
    doc: Document,
    active_sketch: Option<ItemId>,
    /// What was selected at that point: undo and redo bring the selection back too, as observed.
    selection: Vec<Pick>,
    label: String,
}

/// What a command reports back.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub value: Value,
    /// Set when the command changed the document (it then went into undo history).
    pub changed: bool,
}

impl Session {
    pub fn new(kernel: Arc<dyn Kernel>, name: &str) -> Self {
        Self {
            doc: Document::new(name),
            kernel,
            selection: Vec::new(),
            active_sketch: None,
            path: None,
            dirty: false,
            revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            followups: Vec::new(),
            adopt: None,
            shapr_steps: Vec::new(),
            replaying: false,
            log_params: None,
        }
    }

    pub fn with_document(kernel: Arc<dyn Kernel>, doc: Document, path: Option<PathBuf>) -> Self {
        let mut s = Self::new(kernel, "");
        s.doc = doc;
        s.path = path;
        s.refresh_regions();
        s
    }

    /// Recomputes the regions of sketches whose curves changed. Sketches the kernel can't split keep no
    /// regions rather than failing the command.
    pub fn refresh_regions(&mut self) {
        let kernel = self.kernel.clone();
        for sk in &mut self.doc.sketches {
            let key = sk.regions_key();
            if sk.regions_for == key {
                continue;
            }
            sk.regions = Arc::new(compute_regions(kernel.as_ref(), sk));
            sk.regions_for = key;
        }
    }

    /// Runs a command by id with JSON parameters.
    ///
    /// With `SHAPECRAFT_STRICT_PARAMS=1` (the task benchmark sets it) a parameter the command never reads is an
    /// error rather than quietly ignored, so a missing option can't pass for a working one.
    pub fn execute(&mut self, id: &str, params: &Value) -> Result<Outcome> {
        let spec = cmd::find(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        if !(spec.enabled)(self) {
            return Err(EngineError::NotAvailable(format!("{} isn't available right now", spec.label)));
        }
        let before = Snapshot {
            doc: self.doc.clone(),
            active_sketch: self.active_sketch,
            selection: self.selection.clone(),
            label: spec.label.into(),
        };
        let selection_before = self.selection.clone();
        // The faces and edges the command points at, as they look before it runs (a replay finds them again by this).
        let refs = if self.replaying || !spec.modifies { Vec::new() } else { refs::collect(self, params, &selection_before) };
        let vars = self.doc.variables.clone();
        let p = Params::with_variables(params, &vars);
        self.log_params = None;
        let mut result = catch_unwind(AssertUnwindSafe(|| (spec.run)(self, &p)));
        if strict_params()
            && matches!(result, Ok(Ok(_)))
            && let Some(k) = p.unread().into_iter().find(|k| !matches!(k.as_str(), "construction"))
        {
            result = Ok(Err(EngineError::BadParams(format!("unknown parameter '{k}' for {id}"))));
        }
        if matches!(result, Ok(Ok(_))) && params.get("construction").and_then(Value::as_bool) == Some(true) {
            self.mark_construction(&before.doc);
        }
        if matches!(result, Ok(Ok(_))) {
            match self.keep_constraints(&before.doc) {
                Ok(()) => self.refresh_regions(),
                Err(e) => result = Ok(Err(e)),
            }
        }
        match result {
            Ok(Ok(value)) => {
                let mut changed = spec.modifies && self.doc_changed(&before.doc);
                if changed && !history::UNLOGGED.contains(&spec.id) {
                    let logged = self.log_params.take().unwrap_or_else(|| params.clone());
                    self.record(spec.id, spec.label, &logged, selection_before.clone(), before.active_sketch, refs);
                }
                // Leaving a sketch edited in the past rebuilds the steps after it.
                if self.editing_past() && self.active_sketch.is_none() {
                    if let Err(e) = self.resume_history() {
                        self.restore(before, selection_before);
                        return Err(e);
                    }
                    self.prune_selection();
                    changed = true;
                }
                if changed {
                    self.revision += 1;
                    self.undo.push(before);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                    self.dirty = true;
                }
                self.prune_selection();
                if !self.followups.is_empty() {
                    let todo = std::mem::take(&mut self.followups);
                    if !self.replaying {
                        cmd::file::run_followups(self, todo);
                    }
                }
                if let Some(h) = self.adopt.take()
                    && !self.replaying
                {
                    self.adopt_history(h);
                }
                Ok(Outcome { value, changed })
            }
            Ok(Err(e)) => {
                self.restore(before, selection_before);
                Err(e)
            }
            Err(_) => {
                self.restore(before, selection_before);
                Err(EngineError::Panicked(id.to_string()))
            }
        }
    }

    /// Runs a command in place of the last step: a tool pulled again after it applied (the reference's live handles)
    /// replaces its own step, so one undo still takes the whole pull back. On failure nothing changes.
    pub fn amend(&mut self, id: &str, params: &Value) -> Result<Outcome> {
        let snap = self.undo.pop().ok_or_else(|| EngineError::NotAvailable("nothing to amend".into()))?;
        let now = (self.doc.clone(), self.active_sketch, self.selection.clone(), self.revision);
        self.doc = snap.doc.clone();
        self.active_sketch = snap.active_sketch;
        match self.execute(id, params) {
            Ok(o) if o.changed => Ok(o),
            other => {
                (self.doc, self.active_sketch, self.selection, self.revision) = now;
                self.undo.push(snap);
                other.and(Err(EngineError::NotAvailable("that changes nothing".into())))
            }
        }
    }

    /// Takes the last step back without keeping it for redo (a live pull dragged back to nothing).
    pub fn drop_last_step(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else { return false };
        let selection = self.selection.clone();
        self.restore(snap, selection);
        self.prune_selection();
        self.revision += 1;
        true
    }

    /// Takes the last step back (Cancel on a live result) along with what was picked before it, so the picks fit the
    /// model as it was again (a face the step moved may have had another number). Not kept for redo.
    pub fn cancel_last_step(&mut self) -> bool {
        let Some(snap) = self.undo.pop() else { return false };
        let selection = snap.selection.clone();
        self.restore(snap, selection);
        self.prune_selection();
        self.revision += 1;
        true
    }

    /// The document before the last step: what a live tool started from (to draw the edges a live fillet rounds).
    pub fn before_last_step(&self) -> Option<&Document> {
        self.undo.last().map(|s| &s.doc)
    }

    /// [`Session::preview`] as if the last step hadn't happened: the live preview of an [`Session::amend`].
    pub fn preview_amend(&mut self, id: &str, params: &Value) -> Result<Document> {
        let Some(snap) = self.undo.last() else { return self.preview(id, params) };
        let (doc, sketch) = (snap.doc.clone(), snap.active_sketch);
        let now = (std::mem::replace(&mut self.doc, doc), std::mem::replace(&mut self.active_sketch, sketch));
        let out = self.preview(id, params);
        (self.doc, self.active_sketch) = now;
        out
    }

    /// Runs a command on a throwaway copy and returns the document it would produce. Nothing changes:
    /// not the document, history, selection or the active sketch. Used for live previews while dragging.
    pub fn preview(&mut self, id: &str, params: &Value) -> Result<Document> {
        let spec = cmd::find(id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        let saved = (self.doc.clone(), self.selection.clone(), self.active_sketch);
        let vars = self.doc.variables.clone();
        let p = Params::with_variables(params, &vars);
        let mut result = catch_unwind(AssertUnwindSafe(|| (spec.run)(self, &p)));
        if matches!(result, Ok(Ok(_))) {
            match self.keep_constraints(&saved.0) {
                Ok(()) => self.refresh_regions(),
                Err(e) => result = Ok(Err(e)),
            }
        }
        let produced = std::mem::replace(&mut self.doc, saved.0);
        self.selection = saved.1;
        self.active_sketch = saved.2;
        match result {
            Ok(Ok(_)) => Ok(produced),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(EngineError::Panicked(id.to_string())),
        }
    }

    fn restore(&mut self, snap: Snapshot, selection: Vec<Pick>) {
        self.doc = snap.doc;
        self.active_sketch = snap.active_sketch;
        self.selection = selection;
    }

    fn doc_changed(&self, before: &Document) -> bool {
        let shapes_same = before.bodies.len() == self.doc.bodies.len()
            && before.bodies.iter().zip(&self.doc.bodies).all(|(a, b)| {
                a.id == b.id
                    && a.shape.ptr_eq(&b.shape)
                    // A mesh body changes its triangles, not its (empty) shape.
                    && match (&a.facets, &b.facets) {
                        (None, None) => true,
                        (Some(x), Some(y)) => std::sync::Arc::ptr_eq(x, y),
                        _ => false,
                    }
                    && a.name == b.name
                    && a.visible == b.visible
                    && a.appearance == b.appearance
                    && a.face_colors == b.face_colors
            });
        let sketches_same = before.sketches == self.doc.sketches
            && before.planes == self.doc.planes
            && before.axes == self.doc.axes
            && before.drawings == self.doc.drawings
            && before.future == self.doc.future
            && before.variables == self.doc.variables
            && before.images == self.doc.images
            && before.saved_views == self.doc.saved_views
            && before.folders == self.doc.folders;
        let visual_same =
            before.decals == self.doc.decals && before.decal_images == self.doc.decal_images && before.visual == self.doc.visual;
        // Merging steps may change nothing but the history.
        let steps_same = before.history.iter().map(|h| (&h.label, &h.kind)).eq(self.doc.history.iter().map(|h| (&h.label, &h.kind)));
        !(shapes_same && sketches_same && visual_same && steps_same && before.name == self.doc.name && before.units == self.doc.units)
    }

    /// Records a change of selection or of the sketch being edited, alone, as an undo step (both step back with
    /// ⌘Z, as observed: undo after leaving a sketch goes back into it). `before` is the selection and active sketch
    /// until now. Bumps the revision, so a live tool step can't be amended past it.
    pub fn record_selection(&mut self, before: Vec<Pick>, before_sketch: Option<ItemId>) {
        if before == self.selection && before_sketch == self.active_sketch {
            return;
        }
        self.undo.push(Snapshot { doc: self.doc.clone(), active_sketch: before_sketch, selection: before, label: "Selection".into() });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// How many steps Undo can take back.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub(crate) fn undo_step(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        let current = Snapshot {
            doc: self.doc.clone(),
            active_sketch: self.active_sketch,
            selection: self.selection.clone(),
            label: prev.label.clone(),
        };
        self.redo.push(current);
        self.doc = prev.doc;
        self.active_sketch = prev.active_sketch;
        self.selection = prev.selection;
        self.dirty = true;
        self.revision += 1;
        self.prune_selection();
        true
    }

    pub(crate) fn redo_step(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        let current = Snapshot {
            doc: self.doc.clone(),
            active_sketch: self.active_sketch,
            selection: self.selection.clone(),
            label: next.label.clone(),
        };
        self.undo.push(current);
        self.doc = next.doc;
        self.active_sketch = next.active_sketch;
        self.selection = next.selection;
        self.dirty = true;
        self.revision += 1;
        self.prune_selection();
        true
    }

    /// Drops selected things that no longer exist (after undo, delete, or a body being rebuilt).
    fn prune_selection(&mut self) {
        let doc = &self.doc;
        self.selection.retain(|p| match *p {
            Pick::Body { body } => doc.body(body).is_some(),
            Pick::Face { body, face } => doc.body(body).is_some_and(|b| b.mesh.triangle_faces.contains(&face)),
            Pick::Edge { body, edge } => doc.body(body).is_some_and(|b| b.mesh.edge_ids.contains(&edge)),
            Pick::Sketch { sketch } => doc.sketch(sketch).is_some(),
            Pick::Region { sketch, region } => doc.sketch(sketch).is_some_and(|s| s.region(region).is_some()),
            Pick::SketchEntity { sketch, entity } => doc.sketch(sketch).is_some_and(|s| (entity as usize) < s.entities.len()),
            Pick::SketchPoint { sketch, entity, index } => doc.sketch(sketch).is_some_and(|s| {
                s.entities.get(entity as usize).is_some_and(|e| (index as usize) < shapecraft_doc::constraints::points(e).len())
            }),
            Pick::Plane { plane } => doc.plane(plane).is_some(),
            Pick::Axis { axis } => doc.axis(axis).is_some(),
            Pick::Image { image } => doc.image(image).is_some(),
        });
        if let Some(id) = self.active_sketch
            && self.doc.sketch(id).is_none()
        {
            self.active_sketch = None;
        }
    }

    pub fn active_sketch(&self) -> Option<&Sketch> {
        self.active_sketch.and_then(|id| self.doc.sketch(id))
    }

    /// What kind of thing is selected, which drives the contextual toolbar.
    pub fn selection_kind(&self) -> SelectionKind {
        if self.selection.is_empty() {
            return SelectionKind::Nothing;
        }
        let all = |f: fn(&Pick) -> bool| self.selection.iter().all(f);
        if all(|p| matches!(p, Pick::Region { .. })) {
            SelectionKind::Regions
        } else if all(|p| matches!(p, Pick::Face { .. })) {
            SelectionKind::Faces
        } else if all(|p| matches!(p, Pick::Edge { .. })) {
            SelectionKind::Edges
        } else if all(|p| matches!(p, Pick::Body { .. })) {
            SelectionKind::Bodies
        } else if all(|p| matches!(p, Pick::SketchEntity { .. })) {
            SelectionKind::SketchEntities
        } else if all(|p| matches!(p, Pick::Plane { .. })) {
            SelectionKind::Planes
        } else if all(|p| matches!(p, Pick::Axis { .. })) {
            SelectionKind::Axes
        } else if all(|p| matches!(p, Pick::Image { .. })) {
            SelectionKind::Images
        } else {
            SelectionKind::Mixed
        }
    }

    /// The total length of the selected edges or sketch curves (the selection pill's "∿ 7.4 mm", as observed).
    pub fn selection_length(&self) -> Option<f64> {
        let mut total = 0.0;
        let mut any = false;
        for pick in &self.selection {
            let len = match *pick {
                Pick::Edge { body, edge } => {
                    let b = self.doc.body(body)?;
                    self.kernel.measure(&b.shape, shapecraft_kernel::SubShape::Edge(edge)).ok()?.0
                }
                Pick::SketchEntity { sketch, entity } => {
                    let e = self.doc.sketch(sketch)?.entities.get(entity as usize)?;
                    entity_length(e)
                }
                _ => return None,
            };
            if !len.is_finite() {
                return None;
            }
            total += len;
            any = true;
        }
        any.then_some(total)
    }

    /// The radius of the one selected face when it is round with the same radius all over (a cylinder's wall or a
    /// hole), for the selection pill ("1 face | ⊝ 5.6569 mm | ‹" in the reference, 26.202 s36). Cones, spheres and
    /// anything else answer `None`.
    pub fn selection_radius(&self) -> Option<f64> {
        match self.selection[..] {
            [Pick::Face { body, face }] => self.face_radius(body, face),
            // A one-face body picked whole: a sphere reads its diameter too (26.202, s40-022: "⌀ 16 mm").
            [Pick::Body { body }] => self.sphere_radius(body),
            _ => None,
        }
    }

    /// A body that is a single sphere: its radius, from its display mesh's points around their middle.
    fn sphere_radius(&self, body: ItemId) -> Option<f64> {
        let b = self.doc.body(body)?;
        if self.kernel.face_count(&b.shape).ok()? != 1 {
            return None;
        }
        let pts: Vec<glam::DVec3> = b.mesh.positions.iter().map(|p| glam::DVec3::from(p.map(f64::from))).collect();
        let (lo, hi) = pts.iter().fold((glam::DVec3::splat(f64::MAX), glam::DVec3::splat(f64::MIN)), |(l, h), p| (l.min(*p), h.max(*p)));
        let c = (lo + hi) / 2.0;
        let (mut rlo, mut rhi) = (f64::MAX, f64::MIN);
        for p in &pts {
            let r = p.distance(c);
            rlo = rlo.min(r);
            rhi = rhi.max(r);
        }
        (rhi >= rlo && rlo > 1e-6 && rhi - rlo <= 1e-4 * rhi.max(1.0)).then_some((rlo + rhi) / 2.0)
    }

    /// A face's radius when it's the same all over about its axis (a cylinder's wall, a hole); `None` for flat faces,
    /// cones, spheres and free-form ones.
    pub fn face_radius(&self, body: ItemId, face: u32) -> Option<f64> {
        let b = self.doc.body(body)?;
        if self.kernel.face_plane(&b.shape, face).ok().flatten().is_some() {
            return None;
        }
        let axis = self.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Face(face)).ok().flatten()?;
        let dir = axis.dir.try_normalize()?;
        let m = &b.mesh;
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for (t, f) in m.triangle_faces.iter().enumerate() {
            if *f != face {
                continue;
            }
            for k in 0..3 {
                let p = glam::DVec3::from(m.positions[*m.indices.get(t * 3 + k)? as usize].map(f64::from));
                let r = (p - axis.origin).reject_from_normalized(dir).length();
                lo = lo.min(r);
                hi = hi.max(r);
            }
        }
        (hi >= lo && lo > 1e-6 && hi - lo <= 1e-4 * hi.max(1.0)).then_some((lo + hi) / 2.0)
    }

    /// "1 face", "2 edges", "3 items" for the selection pill.
    pub fn selection_summary(&self) -> Option<String> {
        let n = self.selection.len();
        // One whole item is named, as in the reference ("Body 01").
        if let [
            Pick::Body { body: id }
            | Pick::Sketch { sketch: id }
            | Pick::Plane { plane: id }
            | Pick::Axis { axis: id }
            | Pick::Image { image: id },
        ] = self.selection[..]
            && let Some(name) = self.doc.item_name(id)
        {
            return Some(name.to_string());
        }
        let noun = match self.selection_kind() {
            SelectionKind::Nothing => return None,
            SelectionKind::Regions | SelectionKind::Faces => "face",
            SelectionKind::Edges => "edge",
            SelectionKind::Bodies => "body",
            // The reference calls sketch curves edges too ("4 edges" for a rectangle, 414).
            SelectionKind::SketchEntities => "edge",
            SelectionKind::Planes => "plane",
            SelectionKind::Axes => "axis",
            SelectionKind::Images => "image",
            SelectionKind::Mixed => "item",
        };
        Some(match (n, noun) {
            (1, _) => format!("1 {noun}"),
            (_, "body") => format!("{n} bodies"),
            _ => format!("{n} {noun}s"),
        })
    }
}

/// A sketch element's length: exact for lines, circles and arcs, along its drawn outline otherwise.
fn entity_length(e: &shapecraft_doc::SketchEntity) -> f64 {
    use shapecraft_doc::SketchEntity as E;
    match *e {
        E::Line { a, b } => a.distance(b),
        E::Circle { radius, .. } => std::f64::consts::TAU * radius,
        E::Arc { a, mid, b } => match shapecraft_doc::circle_through(a, mid, b) {
            Some((c, r)) => {
                let ang = |p: glam::DVec2| (p - c).to_angle();
                let tau = std::f64::consts::TAU;
                let norm = |x: f64| x.rem_euclid(tau);
                // The sweep from a to b that passes through mid.
                let ccw = norm(ang(b) - ang(a));
                let span = if norm(ang(mid) - ang(a)) < ccw { ccw } else { tau - ccw };
                r * span
            }
            None => a.distance(b),
        },
        _ => e.outlines().iter().map(|o| o.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()).sum(),
    }
}

fn compute_regions(kernel: &dyn Kernel, sk: &shapecraft_doc::Sketch) -> Vec<shapecraft_doc::Region> {
    let curves = sk.curves();
    if curves.is_empty() {
        return Vec::new();
    }
    let Ok(faces) = kernel.planar_regions(sk.frame(), &curves) else { return Vec::new() };
    let mut regions: Vec<shapecraft_doc::Region> = faces
        .into_iter()
        .filter_map(|face| {
            let mesh = kernel.tessellate(&face, shapecraft_doc::project::deflection()).ok()?;
            let mp = kernel.mass_props(&face).ok()?;
            let local = |i: u32| {
                let p = mesh.positions.get(i as usize).copied().unwrap_or_default();
                sk.plane.to_local(glam::DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
            };
            let triangles = mesh.indices.as_chunks::<3>().0.iter().map(|t| [local(t[0]), local(t[1]), local(t[2])]).collect();
            Some(shapecraft_doc::Region {
                index: 0,
                face,
                mesh: Arc::new(mesh),
                area: mp.area,
                centroid: sk.plane.to_local(mp.centroid),
                triangles,
            })
        })
        .collect();
    // Largest first, so a lone shape is region 0 and a plate with holes comes before its holes. Equal areas keep
    // the kernel's order (saved steps name regions by index, so this order must not change).
    regions.sort_by(|a, b| b.area.total_cmp(&a.area));
    for (i, r) in regions.iter_mut().enumerate() {
        r.index = i as u32;
    }
    regions
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    Nothing,
    Regions,
    Faces,
    Edges,
    Bodies,
    SketchEntities,
    Mixed,
    Planes,
    Axes,
    Images,
}

#[cfg(test)]
mod tests;

fn strict_params() -> bool {
    static STRICT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STRICT.get_or_init(|| std::env::var("SHAPECRAFT_STRICT_PARAMS").is_ok_and(|v| v == "1"))
}

//! The parametric history. Every command that changes the document is logged with its parameters and the
//! selection and active sketch it saw. Edits to one sketch group into a "Sketch 01" step, features get their own
//! numbered step ("Extrude 01", 26.200), and changing a step's parameters rebuilds the model by replaying every step
//! from an empty document.

use serde_json::{Map, Value};
use shapecraft_doc::{HistoryStep, ItemId, LogEntry, Pick};

use crate::{EngineError, Result, Session};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Length,
    Angle,
    Count,
    Factor,
}

/// An editable parameter of a feature.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    /// What the command uses when the parameter is left out.
    pub default: f64,
}

/// A command that makes a feature step, its step name and its editable parameters.
#[derive(Clone, Copy, Debug)]
pub struct Feature {
    pub command: &'static str,
    pub name: &'static str,
    pub fields: &'static [Field],
}

const fn f(key: &'static str, label: &'static str, kind: FieldKind, default: f64) -> Field {
    Field { key, label, kind, default }
}

use FieldKind::{Angle, Count, Factor, Length};

pub const FEATURES: &[Feature] = &[
    Feature {
        command: "thread",
        name: "Thread",
        fields: &[f("length", "Length", Length, 10.0), f("offset", "Offset", Length, 0.0), f("clearance", "Radial Clearance", Length, 0.0)],
    },
    Feature {
        command: "extrude",
        name: "Extrude",
        fields: &[f("distance", "Distance", Length, 0.0), f("draft", "Draft Angle", Angle, 0.0)],
    },
    Feature { command: "revolve", name: "Revolve", fields: &[f("angle", "Angle", Angle, 360.0), f("height", "Height", Length, 0.0)] },
    Feature { command: "loft", name: "Loft", fields: &[] },
    Feature { command: "sweep", name: "Sweep", fields: &[f("scale", "Scale", Factor, 1.0), f("twist", "Twist", Angle, 0.0)] },
    Feature { command: "fillet", name: "Fillet", fields: &[f("radius", "Radius", Length, 1.0)] },
    Feature { command: "chamfer", name: "Chamfer", fields: &[f("distance", "Distance", Length, 1.0)] },
    Feature { command: "shell", name: "Shell", fields: &[f("thickness", "Thickness", Length, 1.0)] },
    Feature { command: "union", name: "Union", fields: &[] },
    Feature { command: "subtract", name: "Subtract", fields: &[] },
    Feature { command: "intersect", name: "Intersect", fields: &[] },
    Feature { command: "offset_face", name: "Offset Face", fields: &[f("distance", "Distance", Length, 0.0)] },
    Feature { command: "replace_face", name: "Replace Face", fields: &[] },
    Feature { command: "project_edge", name: "Project as Edge", fields: &[] },
    Feature { command: "scale", name: "Scale", fields: &[f("factor", "Factor", Factor, 1.0)] },
    Feature { command: "translate", name: "Movement/Rotation", fields: &[] },
    Feature { command: "rotate", name: "Rotate", fields: &[f("angle", "Angle", Angle, 0.0)] },
    Feature { command: "move_faces", name: "Movement/Rotation", fields: &[] },
    Feature { command: "mirror", name: "Mirror", fields: &[] },
    Feature { command: "unlinked_copy", name: "Unlinked Copy", fields: &[] },
    Feature { command: "align", name: "Alignment", fields: &[] },
    Feature {
        command: "pattern",
        name: "Linear Pattern",
        fields: &[f("count", "Count", Count, 2.0), f("spacing", "Spacing", Length, 10.0)],
    },
    Feature {
        command: "pattern_circular",
        name: "Circular Pattern",
        fields: &[f("count", "Count", Count, 2.0), f("angle", "Angle", Angle, 360.0)],
    },
    Feature { command: "split_body", name: "Split Body", fields: &[] },
    Feature {
        command: "wrap_emboss",
        name: "Wrap & Emboss",
        fields: &[f("depth", "Emboss", Length, 1.0), f("rotation", "Rotation", Angle, 0.0)],
    },
    Feature { command: "offset_plane", name: "Plane - Offset", fields: &[f("distance", "Distance", Length, 0.0)] },
    Feature { command: "midplane", name: "Midplane", fields: &[] },
    Feature { command: "plane_3points", name: "Plane", fields: &[] },
    Feature { command: "plane_through_edge", name: "Plane", fields: &[f("angle", "Angle", Angle, 0.0)] },
    Feature { command: "plane_on_curve", name: "Plane", fields: &[] },
    Feature { command: "axis_perp_face", name: "Axis", fields: &[] },
    Feature { command: "axis_along_edge", name: "Axis", fields: &[] },
    Feature { command: "axis_cylinder", name: "Axis", fields: &[] },
    Feature { command: "axis_2points", name: "Axis", fields: &[] },
    Feature { command: "import_file", name: "Import", fields: &[] },
    Feature { command: "import_project", name: "Import Project", fields: &[] },
];

/// Which steps that fail after a rebuild refuse it (nothing changes) instead of being marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Strict {
    No,
    All,
    /// Only the step at this index (the one being edited).
    Step(usize),
}

/// The kind of the marker step a breakpoint is while it holds back the steps after it.
pub const BREAKPOINT: &str = "breakpoint";

/// Commands that change the document but never go into the history: they edit the history itself, or change
/// Visualization (decals and the environment ride along when the history is rebuilt).
pub(crate) const UNLOGGED: &[&str] = &[
    "edit_step",
    "edit_sketch_step",
    "delete_step",
    "suppress_step",
    "rename_step",
    "set_breakpoint",
    "set_variable",
    "delete_variable",
    "merge_steps",
    "import_decal",
    "rename_decal_image",
    "delete_decal_image",
    "place_decal",
    "update_decal",
    "delete_decal",
    "visual_settings",
    "import_environment",
    "rename_environment",
    "delete_environment",
];

/// Commands that change the document without being modeling steps.
const QUIET: &[&str] = &[
    "rename",
    "rename_project",
    "show_hide",
    "show_all",
    "set_appearance",
    "set_face_color",
    "set_units",
    "save_view",
    "create_drawing",
    "update_drawing",
    "delete_drawing",
    "annotate",
    "delete_annotation",
    "update_annotation",
    "add_drawing_view",
    "update_drawing_view",
    "delete_drawing_view",
    "new_folder",
    "move_to_folder",
    "place_item",
];

pub fn feature(command: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.command == command)
}

/// A feature step's current value for a field.
pub fn value(step: &HistoryStep, field: &Field, vars: &[shapecraft_doc::Variable]) -> f64 {
    match step.entries.first().and_then(|e| e.params.get(field.key)) {
        Some(Value::String(expr)) => shapecraft_doc::expr::eval(expr, vars).unwrap_or(field.default),
        other => other.and_then(Value::as_f64).unwrap_or(field.default),
    }
}

/// The expression a field was given ("depth / 2"), if it wasn't a plain number.
pub fn expression<'a>(step: &'a HistoryStep, field: &Field) -> Option<&'a str> {
    step.entries.first().and_then(|e| e.params.get(field.key)).and_then(Value::as_str)
}

/// How many faces or regions a feature step's profile is ("1 face"), when it has one.
pub fn profile_count(step: &HistoryStep) -> Option<usize> {
    let e = step.entries.first()?;
    if !matches!(step.kind.as_str(), "extrude" | "revolve") {
        return None;
    }
    if let Some(list) = e.params.get("regions").and_then(Value::as_array) {
        return Some(list.len());
    }
    if e.params.get("region").is_some() || e.params.get("face").is_some() {
        return Some(1);
    }
    Some(e.selection.iter().filter(|p| matches!(p, Pick::Region { .. } | Pick::Face { .. })).count())
}

impl Session {
    /// Logs a command that changed the document, as its own step or as part of the sketch step it edits.
    pub(crate) fn record(
        &mut self,
        id: &str,
        label: &str,
        params: &Value,
        selection: Vec<Pick>,
        active_sketch: Option<ItemId>,
        refs: Vec<shapecraft_doc::TopoRef>,
    ) {
        let entry = LogEntry { command: id.to_string(), params: params.clone(), selection, active_sketch, refs };
        let quiet = QUIET.contains(&id);
        let feat = feature(id);
        let open = self.active_sketch.or(active_sketch);
        // Move, Rotate, Scale, Mirror and the Patterns on curves inside a sketch are edits of that sketch: no step of
        // their own.
        let sketch_edit = matches!(id, "translate" | "rotate" | "scale" | "mirror" | "pattern" | "pattern_circular")
            && open.is_some()
            && params.get("items").is_none()
            && !entry.selection.is_empty()
            && entry
                .selection
                .iter()
                .all(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Region { .. } | Pick::Sketch { .. } | Pick::SketchPoint { .. }));
        let sketch = if (feat.is_none() || sketch_edit) && !quiet { open } else { None };
        if let Some(sk) = sketch {
            // Only the latest step grows: replaying in a different order could hand out different ids.
            let next = self.doc.next_id();
            if let Some(last) = self.doc.history.last_mut()
                && last.sketch == Some(sk)
            {
                last.entries.push(entry);
                last.ids = next;
                return;
            }
            let name = self.doc.sketch(sk).map_or_else(|| label.to_string(), |s| s.name.clone());
            let ids = self.doc.next_id();
            self.doc.history.push(HistoryStep {
                label: name,
                kind: "sketch".into(),
                sketch: Some(sk),
                entries: vec![entry],
                ids,
                ..Default::default()
            });
            return;
        }
        // A copy that isn't linked keeps its own geometry: its own kind of step (26.200's "Unlinked Copy").
        let (id, feat) = if params.get("frozen").is_some() { ("unlinked_copy", feature("unlinked_copy")) } else { (id, feat) };
        let name = feat.map_or(label, |f| f.name);
        let n = self.doc.history.iter().filter(|h| !h.quiet && feature(&h.kind).map_or(h.kind == id, |f| f.name == name)).count() + 1;
        // Imports are named after the file, as Shapr3D names them (Import "box.step").
        let file = (id == "import_file")
            .then(|| params.get("path").and_then(Value::as_str))
            .flatten()
            .and_then(|p| std::path::Path::new(p).file_name())
            .map(|f| f.to_string_lossy().into_owned());
        let label = match file {
            Some(f) => format!("Import \"{f}\""),
            None if quiet => label.to_string(),
            None => format!("{name} {n:02}"),
        };
        let ids = self.doc.next_id();
        let shapr_steps = if id == "import_file" { std::mem::take(&mut self.shapr_steps) } else { Vec::new() };
        self.doc.history.push(HistoryStep {
            label,
            kind: id.to_string(),
            quiet,
            entries: vec![entry],
            ids,
            shapr_steps,
            ..Default::default()
        });
    }

    /// Every step: the ones that ran, then the ones held back (by a breakpoint, or while a sketch is edited in the
    /// past). Step indices of the history commands count in this list.
    pub fn all_steps(&self) -> Vec<HistoryStep> {
        self.doc.history.iter().chain(&self.doc.future).cloned().collect()
    }

    /// A breakpoint holds back the steps after it (it's then the first of `future`).
    pub fn at_breakpoint(&self) -> bool {
        self.doc.future.first().is_some_and(|h| h.kind == BREAKPOINT)
    }

    /// A sketch is being edited in the past: the steps after it wait until it's left.
    pub fn editing_past(&self) -> bool {
        !self.doc.future.is_empty() && !self.at_breakpoint()
    }

    fn check_not_editing_past(&self) -> Result<()> {
        if self.editing_past() {
            return Err(EngineError::NotAvailable("finish editing the sketch first".into()));
        }
        Ok(())
    }

    /// Rebuilds the model from all the steps; a breakpoint among them holds back the ones after it. Steps that fail
    /// are skipped and marked. `strict` says which newly failing steps refuse the result instead.
    pub(crate) fn rebuild(&mut self, mut all: Vec<HistoryStep>, strict: Strict) -> Result<()> {
        let old = self.all_steps();
        let held = match all.iter().position(|h| h.kind == BREAKPOINT) {
            Some(i) => all.split_off(i),
            None => Vec::new(),
        };
        self.replay(all)?;
        self.doc.future = held;
        let newly_broken =
            |h: &&HistoryStep| h.error.is_some() && old.iter().find(|o| o.label == h.label).is_none_or(|o| o.error.is_none());
        let broken = match strict {
            Strict::No => None,
            Strict::All => self.doc.history.iter().find(newly_broken),
            Strict::Step(i) => self.doc.history.get(i).filter(newly_broken),
        };
        if let Some(broken) = broken {
            return Err(EngineError::BadParams(format!("{} fails: {}", broken.label, broken.error.clone().unwrap_or_default())));
        }
        Ok(())
    }

    fn step_at(&self, index: usize) -> Result<HistoryStep> {
        self.all_steps().get(index).cloned().ok_or_else(|| EngineError::BadParams(format!("no step {index}")))
    }

    /// Suppresses a step (or brings it back): the model is rebuilt without it, and steps that needed it fail.
    pub(crate) fn suppress_step(&mut self, index: usize, on: Option<bool>) -> Result<bool> {
        self.check_not_editing_past()?;
        let mut all = self.all_steps();
        let step = all.get_mut(index).ok_or_else(|| EngineError::BadParams(format!("no step {index}")))?;
        if step.kind == BREAKPOINT || step.entries.is_empty() {
            return Err(EngineError::NotAvailable(format!("{} can't be suppressed", step.label)));
        }
        step.suppressed = on.unwrap_or(!step.suppressed);
        let now = step.suppressed;
        self.rebuild(all, Strict::No)?;
        Ok(now)
    }

    pub(crate) fn rename_step(&mut self, index: usize, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 200 {
            return Err(EngineError::BadParams("a step's name needs 1 to 200 characters".into()));
        }
        let step = self.step_at(index)?;
        if step.kind == BREAKPOINT {
            return Err(EngineError::NotAvailable("the breakpoint has no name".into()));
        }
        let past = self.doc.history.len();
        let target = if index < past { &mut self.doc.history[index] } else { &mut self.doc.future[index - past] };
        target.label = name.to_string();
        Ok(())
    }

    /// Puts the breakpoint before the step at `before` (counting steps without the breakpoint): the model rolls
    /// back to it and new steps go in there. `None`, or a place past the last step, removes it.
    pub(crate) fn set_breakpoint(&mut self, before: Option<usize>) -> Result<()> {
        self.check_not_editing_past()?;
        let mut all: Vec<HistoryStep> = self.all_steps().into_iter().filter(|h| h.kind != BREAKPOINT).collect();
        if let Some(n) = before.filter(|n| *n < all.len()) {
            all.insert(n, HistoryStep { label: "Breakpoint".into(), kind: BREAKPOINT.into(), ..Default::default() });
        }
        self.rebuild(all, Strict::No)
    }

    /// Changes a step's parameters and rebuilds the model from its history. Nothing changes when the step itself
    /// fails; steps after it that no longer work are marked, as the reference marks them, and can be fixed or undone.
    pub(crate) fn edit_step(&mut self, index: usize, changes: &Map<String, Value>) -> Result<()> {
        self.check_not_editing_past()?;
        let mut steps = self.all_steps();
        let step = steps.get_mut(index).ok_or_else(|| EngineError::BadParams(format!("no step {index}")))?;
        let entry = step
            .entries
            .first_mut()
            .ok_or_else(|| EngineError::NotAvailable(format!("{} can't be edited (it was made by an older version)", step.label)))?;
        let Value::Object(params) = &mut entry.params else {
            return Err(EngineError::BadParams(format!("{} has no parameters", step.label)));
        };
        // A null takes the parameter out (back to the command's default).
        for (k, v) in changes {
            if v.is_null() {
                params.remove(k);
            } else {
                params.insert(k.clone(), v.clone());
            }
        }
        self.rebuild(steps, Strict::Step(index))
    }

    /// Sets a variable (adding it if new, renaming it when `rename` is given) and rebuilds the model, since steps
    /// may use it. Nothing changes when the value can't be worked out or a step can't be rebuilt with it.
    pub(crate) fn set_variable(&mut self, name: &str, expr: &str, rename: Option<&str>) -> Result<()> {
        let new_name = rename.unwrap_or(name);
        if !shapecraft_doc::expr::valid_name(new_name) {
            return Err(EngineError::BadParams(format!("'{new_name}' can't be a variable name: use letters, digits and _")));
        }
        let mut vars = self.doc.variables.clone();
        if new_name != name && vars.iter().any(|v| v.name == new_name) {
            return Err(EngineError::BadParams(format!("there's already a variable '{new_name}'")));
        }
        match vars.iter_mut().find(|v| v.name == name) {
            Some(v) => {
                v.name = new_name.to_string();
                v.expr = expr.to_string();
            }
            None => vars.push(shapecraft_doc::Variable { name: new_name.to_string(), expr: expr.to_string(), kind: Default::default() }),
        }
        for v in &vars {
            shapecraft_doc::expr::eval(&v.expr, &vars).map_err(|e| EngineError::BadParams(format!("{}: {e}", v.name)))?;
        }
        self.apply_variables(vars)
    }

    pub(crate) fn delete_variable(&mut self, name: &str) -> Result<()> {
        let mut vars = self.doc.variables.clone();
        let before = vars.len();
        vars.retain(|v| v.name != name);
        if vars.len() == before {
            return Err(EngineError::BadParams(format!("there's no variable '{name}'")));
        }
        for v in &vars {
            shapecraft_doc::expr::eval(&v.expr, &vars).map_err(|e| EngineError::BadParams(format!("{} uses it: {e}", v.name)))?;
        }
        self.apply_variables(vars)
    }

    /// Takes new variables, rebuilding the model when any step used an expression.
    fn apply_variables(&mut self, vars: Vec<shapecraft_doc::Variable>) -> Result<()> {
        let old = std::mem::replace(&mut self.doc.variables, vars);
        let uses = self
            .doc
            .history
            .iter()
            .flat_map(|h| &h.entries)
            .any(|e| e.params.as_object().is_some_and(|m| m.values().any(serde_json::Value::is_string)));
        if uses
            && !self.editing_past()
            && self.doc.history.iter().all(|h| !h.entries.is_empty())
            && let Err(e) = self.rebuild(self.all_steps(), Strict::All)
        {
            self.doc.variables = old;
            return Err(e);
        }
        Ok(())
    }

    /// Removes a step and rebuilds the model without it; later steps that needed it are marked as failing.
    pub(crate) fn delete_step(&mut self, index: usize) -> Result<()> {
        self.check_not_editing_past()?;
        let mut steps = self.all_steps();
        if index >= steps.len() {
            return Err(EngineError::BadParams(format!("no step {index}")));
        }
        steps.remove(index);
        self.rebuild(steps, Strict::No)?;
        self.selection.clear();
        Ok(())
    }

    /// Edits a sketch step in the past: the model goes back to just after that step and the sketch opens; the
    /// steps after it come back when the sketch is left.
    pub(crate) fn edit_sketch_step(&mut self, index: usize) -> Result<ItemId> {
        if self.editing_past() {
            return Err(EngineError::NotAvailable("finish editing the other sketch first".into()));
        }
        let steps = self.doc.history.clone();
        let step = steps.get(index).ok_or_else(|| {
            if index < self.all_steps().len() {
                EngineError::NotAvailable("that step is after the breakpoint".into())
            } else {
                EngineError::BadParams(format!("no step {index}"))
            }
        })?;
        let sketch = step
            .sketch
            .filter(|_| step.kind == "sketch")
            .ok_or_else(|| EngineError::BadParams(format!("{} isn't a sketch", step.label)))?;
        if step.suppressed || step.error.is_some() {
            return Err(EngineError::NotAvailable(format!("{} isn't in the model (it's suppressed or failed)", step.label)));
        }
        if index + 1 < steps.len() {
            let mut held = steps[index + 1..].to_vec();
            held.append(&mut self.doc.future);
            self.replay(steps[..=index].to_vec())?;
            self.doc.future = held;
        }
        if let Some(sk) = self.doc.sketch_mut(sketch) {
            sk.visible = true;
        }
        self.active_sketch = Some(sketch);
        self.selection.clear();
        Ok(sketch)
    }

    /// Merge Modeling Steps, as observed (L17): the history becomes one "Merged Steps" step holding the model as it
    /// is; the sketches go unless kept (they no longer drive anything), and so do the variables unless kept.
    pub(crate) fn merge_steps(&mut self, keep_sketches: bool, keep_variables: bool) -> Result<()> {
        if !keep_sketches {
            self.doc.sketches.clear();
            self.active_sketch = None;
        }
        if !keep_variables {
            self.doc.variables.clear();
        }
        let snapshot = shapecraft_doc::project::snapshot(&self.doc, self.kernel.as_ref())
            .map_err(|e| EngineError::NotAvailable(format!("the model can't be merged: {e}")))?;
        let entry = LogEntry {
            command: "restore_merged".into(),
            params: serde_json::json!({ "snapshot": snapshot }),
            selection: Vec::new(),
            active_sketch: None,
            refs: Vec::new(),
        };
        self.doc.history =
            vec![HistoryStep { label: "Merged Steps".into(), kind: "merged".into(), entries: vec![entry], ..Default::default() }];
        self.doc.future.clear();
        Ok(())
    }

    /// Puts back the model a merged step holds, keeping the document's name and history.
    pub(crate) fn restore_merged(&mut self, snapshot: &Value) -> Result<()> {
        let mut doc = shapecraft_doc::project::from_snapshot(snapshot, self.kernel.as_ref())
            .map_err(|e| EngineError::BadParams(format!("the merged model can't be read: {e}")))?;
        doc.name = std::mem::take(&mut self.doc.name);
        doc.history = std::mem::take(&mut self.doc.history);
        doc.future = std::mem::take(&mut self.doc.future);
        doc.images = std::mem::take(&mut self.doc.images);
        carry_visual(&mut doc, &mut self.doc);
        self.doc = doc;
        Ok(())
    }

    /// Replays the steps that were set aside while a sketch was edited in the past.
    /// Makes the bodies again from the History, for a project whose bodies file couldn't be read (opened with
    /// `project::load_without_bodies`). Returns how many steps failed (they're marked, as on any rebuild).
    pub fn rebuild_bodies(&mut self) -> Result<usize> {
        if self.doc.history.is_empty() {
            return Err(EngineError::NotAvailable("there's no History to rebuild the bodies from".into()));
        }
        self.rebuild(self.all_steps(), Strict::No)?;
        self.refresh_regions();
        self.revision += 1;
        Ok(self.doc.history.iter().filter(|h| h.error.is_some()).count())
    }

    pub(crate) fn resume_history(&mut self) -> Result<()> {
        self.rebuild(self.all_steps(), Strict::No)
    }

    /// Rebuilds the document by running every step's commands from an empty document. A suppressed step is
    /// skipped; a step that fails is undone, marked with its error and skipped too.
    pub(crate) fn replay(&mut self, mut steps: Vec<HistoryStep>) -> Result<()> {
        if let Some(old) = steps.iter().find(|h| h.entries.is_empty()) {
            return Err(EngineError::NotAvailable(format!("the history can't be rebuilt: {} was made by an older version", old.label)));
        }
        let mut fresh = Session::new(self.kernel.clone(), &self.doc.name);
        fresh.replaying = true;
        fresh.doc.units = self.doc.units;
        fresh.doc.legacy_face_axes = self.doc.legacy_face_axes;
        fresh.doc.variables = self.doc.variables.clone();
        for step in &mut steps {
            step.error = None;
            if !step.suppressed {
                let saved = (fresh.doc.clone(), fresh.active_sketch);
                for e in &mut step.entries {
                    // Faces and edges are numbered afresh by every change upstream: point the step at the ones that
                    // look like what it picked, and remember them as they are now.
                    crate::refs::remap(&fresh, e);
                    fresh.selection = e.selection.clone();
                    fresh.active_sketch = e.active_sketch;
                    if let Err(err) = fresh.execute(&e.command, &e.params) {
                        step.error = Some(err.to_string());
                        (fresh.doc, fresh.active_sketch) = saved;
                        break;
                    }
                }
            }
            if step.error.is_none() && !step.suppressed {
                step.ids = step.ids.max(fresh.doc.next_id());
            }
            fresh.doc.reserve_ids(step.ids);
        }
        let mut doc = fresh.doc;
        doc.history = steps;
        doc.name = self.doc.name.clone();
        carry_visual(&mut doc, &mut self.doc);
        self.doc = doc;
        if self.active_sketch.is_some_and(|id| self.doc.sketch(id).is_none()) {
            self.active_sketch = None;
        }
        Ok(())
    }
}

/// Decals and the Visualization settings aren't history steps: they move to the rebuilt document, decals only while
/// their body is still there. So do the Shapr3D links of bodies our own steps remade from Shapr3D's extrudes (the
/// import leaves those to the steps after it; ids replay the same).
fn carry_visual(to: &mut shapecraft_doc::Document, from: &mut shapecraft_doc::Document) {
    if let (Some(new), Some(old)) = (to.shapr.as_mut(), from.shapr.as_ref()) {
        for r in old.bodies.iter().filter(|r| r.extrude.is_some()) {
            if !new.bodies.iter().any(|n| n.body == r.body) {
                new.bodies.push(r.clone());
            }
        }
    }
    to.decal_images = std::mem::take(&mut from.decal_images);
    to.visual = std::mem::take(&mut from.visual);
    let bodies = &to.bodies;
    to.decals = std::mem::take(&mut from.decals).into_iter().filter(|d| bodies.iter().any(|b| b.id == d.body)).collect();
}

impl Session {
    /// Which history steps made the given bodies and sketches (History's "Related to Selection", as observed):
    /// a step is related when it creates or changes one of the bodies, or edits one of the sketches; a sketch step
    /// is related when a related step used its sketch. Found by replaying the history on the side; `None` when it
    /// can't be (old steps, or a sketch being edited in the past).
    pub fn related_steps(&self, bodies: &[ItemId], sketches: &[ItemId]) -> Option<Vec<bool>> {
        let steps = &self.doc.history;
        if self.editing_past() || steps.iter().any(|h| h.entries.is_empty()) {
            return None;
        }
        let mut fresh = Session::new(self.kernel.clone(), &self.doc.name);
        fresh.replaying = true;
        fresh.doc.units = self.doc.units;
        fresh.doc.legacy_face_axes = self.doc.legacy_face_axes;
        fresh.doc.variables = self.doc.variables.clone();
        let mut related = vec![false; steps.len()];
        for (i, step) in steps.iter().enumerate() {
            let before: Vec<(ItemId, shapecraft_kernel::Shape)> =
                fresh.doc.bodies.iter().filter(|b| bodies.contains(&b.id)).map(|b| (b.id, b.shape.clone())).collect();
            if !step.suppressed && step.error.is_none() {
                for e in &step.entries {
                    fresh.selection = e.selection.clone();
                    fresh.active_sketch = e.active_sketch;
                    fresh.execute(&e.command, &e.params).ok()?;
                }
            }
            fresh.doc.reserve_ids(step.ids);
            let touched = fresh
                .doc
                .bodies
                .iter()
                .any(|b| bodies.contains(&b.id) && !before.iter().any(|(id, sh)| *id == b.id && sh.ptr_eq(&b.shape)));
            related[i] = !step.quiet && (touched || step.sketch.is_some_and(|s| sketches.contains(&s)));
        }
        // The sketches the related steps were made from.
        for i in (0..steps.len()).rev() {
            if !related[i] || steps[i].sketch.is_some() {
                continue;
            }
            let used: Vec<ItemId> = steps[i]
                .entries
                .iter()
                .flat_map(|e| {
                    let param = e.params.get("sketch").and_then(Value::as_u64).map(ItemId);
                    let picked = e.selection.iter().filter_map(|p| match *p {
                        Pick::Sketch { sketch } | Pick::Region { sketch, .. } | Pick::SketchEntity { sketch, .. } => Some(sketch),
                        _ => None,
                    });
                    param.into_iter().chain(picked).collect::<Vec<_>>()
                })
                .collect();
            for j in 0..i {
                if steps[j].sketch.is_some_and(|s| used.contains(&s)) {
                    related[j] = true;
                }
            }
        }
        Some(related)
    }
}

/// A fillet face's own step, as the reference finds it (observed 362: "Edit Fillet 01"): the latest Fillet step on
/// the body with the face's radius. Returns the step's index, its radius and whether the face is convex.
pub fn fillet_step_for(s: &Session, body: ItemId, face: u32) -> Option<(usize, f64, bool)> {
    let b = s.doc.body(body)?;
    let (radius, convex) = s.kernel.blend_of(&b.shape, face).ok().flatten()?;
    s.doc.history.iter().enumerate().rev().find_map(|(i, step)| {
        let e = step.entries.last()?;
        if step.kind != "fillet" || e.command != "fillet" {
            return None;
        }
        let r = e
            .params
            .get("radius")
            .and_then(Value::as_f64)
            .or_else(|| e.params.get("radius").and_then(Value::as_str).and_then(|t| t.trim().parse().ok()))?;
        let on = e
            .params
            .get("body")
            .and_then(Value::as_u64)
            .map(|id| id == body.0)
            .unwrap_or_else(|| e.selection.iter().any(|p| p.body() == Some(body)));
        (on && (r - radius).abs() <= 1e-6 * (1.0 + radius)).then_some((i, radius, convex))
    })
}

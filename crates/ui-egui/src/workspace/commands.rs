//! Running commands from the UI (failures become toasts), live steps a pulled tool amends, and the running
//! tool's live preview.

use serde_json::{Value, json};
use shapecraft_engine::EngineError;

use super::tools::{self, Tool};
use super::{Preview, Workspace};

impl Workspace {
    /// Whether the running tool's last pull is still the latest step (so the next one amends it).
    pub(crate) fn is_live(&self) -> bool {
        self.live_step == Some(self.session.revision)
    }

    /// Applies a pulled tool's step: the first pull adds it, later pulls replace it (one undo takes all of it back),
    /// and a pull back to nothing removes it.
    pub(crate) fn run_live(&mut self, id: &'static str, params: Value, nothing: bool) -> bool {
        if nothing {
            if self.is_live() {
                self.session.drop_last_step();
            }
            self.live_step = None;
            return true;
        }
        // Heavy steps (Offset Face) work on another thread; a long one finishes in the background (#59).
        if super::working::HEAVY.contains(&id) {
            if self.importing.is_some() || self.working.is_some() {
                return false;
            }
            let params_copy = params.clone();
            return match self.run_heavy(id, params, self.is_live()) {
                Some(Ok(_)) => {
                    self.remember(id);
                    self.live_step = Some(self.session.revision);
                    true
                }
                Some(Err(e)) => {
                    self.fail_toast(
                        id,
                        &params_copy,
                        match e {
                            EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                            other => other.to_string(),
                        },
                    );
                    false
                }
                None => true,
            };
        }
        let ok = if self.is_live() {
            match self.session.amend(id, &params) {
                Ok(_) => true,
                Err(e) => {
                    self.fail_toast(
                        id,
                        &params,
                        match e {
                            EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                            other => other.to_string(),
                        },
                    );
                    false
                }
            }
        } else {
            self.run(id, params).is_some()
        };
        if ok {
            self.live_step = Some(self.session.revision);
        }
        ok
    }

    /// Runs a command and reports failures as a toast. Returns the command's value on success.
    pub fn run(&mut self, id: &'static str, mut params: Value) -> Option<Value> {
        // Construction mode: what the sketch tools draw becomes construction geometry.
        const DRAWS: &[&str] = &["line", "rectangle", "circle", "polygon", "arc", "ellipse", "spline", "text", "offset_edge", "project"];
        if self.construction_mode
            && DRAWS.contains(&id)
            && let Value::Object(map) = &mut params
        {
            map.insert("construction".into(), Value::Bool(true));
        }
        // While an import runs on its own thread, its result replaces the project: nothing else may change it.
        if self.importing.is_some() || self.working.is_some() {
            return None;
        }
        // A History edit rebuilds from its step on: off the window's thread when it takes a while.
        if super::working::HISTORY_EDITS.contains(&id) {
            return self.run_off_thread(id, params);
        }
        // A view-only phone changes nothing (Undo and Redo included: they'd step through what came with the file).
        if crate::tablet::view_only() && (matches!(id, "undo" | "redo") || shapecraft_engine::cmd::find(id).is_some_and(|c| c.modifies)) {
            self.toast(crate::tablet::VIEW_ONLY_NOTE);
            return None;
        }
        // Undo and Redo flash what they changed (crate::motion's fancy animations).
        // Each body's mesh by address: Undo puts back the very mesh an unchanged body had.
        let mesh_at = |b: &shapecraft_doc::Body| std::sync::Arc::as_ptr(&b.mesh).cast::<()>() as usize;
        let flash_before: Option<Vec<(shapecraft_doc::ItemId, usize)>> = (matches!(id, "undo" | "redo") && crate::motion::enabled())
            .then(|| self.session.doc.bodies.iter().map(|b| (b.id, mesh_at(b))).collect());
        match self.session.execute(id, &params) {
            Ok(out) => {
                if let Some(before) = flash_before {
                    let changed: Vec<shapecraft_doc::ItemId> = self
                        .session
                        .doc
                        .bodies
                        .iter()
                        .filter(|b| !before.iter().any(|(id, m)| *id == b.id && *m == mesh_at(b)))
                        .map(|b| b.id)
                        .collect();
                    self.undo_flash = (!changed.is_empty()).then_some((changed, self.now));
                }
                self.remember(id);
                if matches!(id, "select" | "select_all") {
                    self.repredict();
                }
                Some(out.value)
            }
            Err(e) => {
                self.fail_toast(
                    id,
                    &params,
                    match e {
                        EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    },
                );
                None
            }
        }
    }

    /// Runs a whole command (a History edit, a boolean) on a fork off the window's thread when it takes a while, as
    /// the heavy tools do (#59): Some(value) when it landed at once; None when it failed (said in a toast), when
    /// nothing may change the project now, or while it goes on behind the working card (`self.working`).
    pub(crate) fn run_off_thread(&mut self, id: &'static str, params: Value) -> Option<Value> {
        if self.importing.is_some() || self.working.is_some() {
            return None;
        }
        if crate::tablet::view_only() && shapecraft_engine::cmd::find(id).is_some_and(|c| c.modifies) {
            self.toast(crate::tablet::VIEW_ONLY_NOTE);
            return None;
        }
        let params_copy = params.clone();
        match self.run_heavy(id, params, false) {
            Some(Ok(out)) => {
                self.remember(id);
                self.live_step = None;
                self.say_note(&out.value);
                Some(out.value)
            }
            Some(Err(e)) => {
                self.fail_toast(
                    id,
                    &params_copy,
                    match e {
                        EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    },
                );
                None
            }
            None => None,
        }
    }

    /// What a step that worked has to say about it (a Union that joined nothing).
    pub(super) fn say_note(&mut self, value: &Value) {
        if let Some(note) = value["note"].as_str() {
            self.toast(note);
        }
    }

    pub(super) fn remember(&mut self, id: &'static str) {
        if matches!(id, "select" | "inspect" | "deselect_all") {
            return;
        }
        self.recent_commands.retain(|c| *c != id);
        self.recent_commands.insert(0, id);
        self.recent_commands.truncate(8);
    }

    /// Recomputes the live result of the running tool when its value changed (at most every 30 ms).
    pub(super) fn update_preview(&mut self) {
        match &self.tool {
            Tool::Extrude(_) if self.cut_base.is_none() && !self.is_live() => self.cut_base = Some(self.session.doc.clone()),
            Tool::Extrude(_) => {}
            _ => self.cut_base = None,
        }
        // A live step at rest is already what the preview would show; leave the selection highlighted (the new cap
        // stays blue, as observed). A typed value shows only once Return applies it (26.202: Offset Face s39-of-type15 and a cut s39-cut-typed
        // keep the part as it was while the field is open).
        // A pattern keeps showing its copies while a value is typed.
        let patterning = matches!(self.tool, Tool::Pattern(_) | Tool::CircularPattern(_));
        if (self.is_live() || self.numeric.is_some() && !patterning) && !self.dragging_handle() {
            self.preview = None;
            return;
        }
        let request = match &self.tool {
            Tool::Extrude(t) => {
                let d = t.distance;
                (d.abs() > 1e-9).then(|| t.request(d))
            }
            Tool::Fillet(t) => {
                let r = t.radius;
                (r > 1e-9).then(|| tools::fillet_request(t, r))
            }
            Tool::Shell(t) => {
                let th = t.thickness;
                (th > 1e-9).then(|| ("shell", json!({ "body": t.body.0, "faces": t.faces, "thickness": th })))
            }
            Tool::Move(t) => t.moves(t.value).then(|| t.request(t.value)),
            Tool::OffsetFace(t) => (t.distance.abs() > 1e-9).then(|| t.request(t.distance)),
            Tool::Scale(t) => {
                let f = t.factor;
                t.changed(f).then(|| t.request(f))
            }
            Tool::Pattern(t) => {
                let sp = t.spacing;
                (sp.abs() > 1e-9).then(|| t.request(sp))
            }
            Tool::OffsetEdge(t) => {
                let d = t.distance;
                (d.abs() > 1e-9).then(|| t.request(d))
            }
            Tool::OffsetPlane(t) => Some(t.request(t.distance)),
            Tool::Loft(t) => t.request(),
            Tool::Sweep(t) => t.request(),
            Tool::Fill(t) => t.request(),
            Tool::Revolve(t) => {
                let a = t.angle;
                t.params(a).map(|p| ("revolve", p))
            }
            Tool::CircularPattern(t) => {
                let a = t.angle;
                t.request(a)
            }
            Tool::RotateAxis(t) => {
                let a = t.angle;
                if a.abs() < 1e-9 { None } else { t.request(a) }
            }
            Tool::BodyEdgeOffset(_) => None,
            Tool::EdgePlane(t) => Some(t.request(t.value)),
            _ => None,
        };
        let Some((id, params)) = request else {
            self.preview = None;
            self.preview_job = None;
            return;
        };
        let key = format!("{id}{params}");
        if super::working::HEAVY.contains(&id) {
            if self.working.is_none() {
                self.heavy_preview(id, &params, key.clone());
                if let Tool::Fillet(t) = &self.tool
                    && t.dragging.is_some()
                    && t.two.is_none()
                    && let Some(p) = self.preview.as_mut().filter(|p| p.key == key)
                {
                    self.pull_limit.learn(t.radius, p);
                }
            }
            return;
        }
        if let Some(p) = &self.preview
            && (p.key == key || self.now - p.computed_at < 0.03)
        {
            return;
        }
        let made = if self.is_live() { self.session.preview_amend(id, &params) } else { self.session.preview(id, &params) };
        self.preview = Some(Preview::new(key, self.now, made));
    }

    /// A body face pushed out (Extrude on a face, not a cut) once it has a result, live or previewed: it shows the
    /// part as it was with the face still picked, and the grown body see-through over it.
    pub(crate) fn growing_face(&self) -> Option<(shapecraft_doc::ItemId, u32)> {
        let Tool::Extrude(t) = &self.tool else { return None };
        let crate::workspace::tools::Target::Face { body, face } = t.target else { return None };
        let applied = self.is_live() || self.has_live_preview();
        let cut = self.badge_current(crate::workspace::badges::Badge::Result) == Some(3);
        (applied && !cut && t.distance.abs() > 1e-9 && self.cut_base.as_ref().is_some_and(|d| d.body(body).is_some()))
            .then_some((body, face))
    }

    /// The stand-in id of the part as it was, drawn under a growing face's see-through result.
    pub(crate) fn before_id(body: shapecraft_doc::ItemId) -> shapecraft_doc::ItemId {
        shapecraft_doc::ItemId(body.0 | (1 << 62))
    }

    pub(crate) fn has_live_preview(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.doc.is_some())
    }

    /// The document as shown: with Isolate on, everything else is hidden.
    pub(crate) fn shown_doc(&self) -> std::borrow::Cow<'_, shapecraft_doc::Document> {
        self.doc_as_shown(true)
    }

    /// What picking sees: as shown, but a cut's part with its live hole (face numbers follow the real shape).
    pub(crate) fn pick_doc(&self) -> std::borrow::Cow<'_, shapecraft_doc::Document> {
        self.doc_as_shown(false)
    }

    fn doc_as_shown(&self, show_uncut: bool) -> std::borrow::Cow<'_, shapecraft_doc::Document> {
        // Sketch Fillet shows the rounded sketch in place of the original (s11-095).
        if let (Tool::SketchFillet { corners, radius, .. }, Some(id)) = (&self.tool, self.session.active_sketch) {
            let r = self.numeric.as_deref().and_then(|t| self.eval_typed(t)).map(f64::abs).unwrap_or(*radius);
            let mut d = self.session.doc.clone();
            if let Some(sk) = d.sketch_mut(id)
                && let Ok(list) = shapecraft_doc::sketch_ops::fillet_corners(&sk.entities, corners, r)
            {
                sk.entities = list;
            }
            return std::borrow::Cow::Owned(d);
        }
        let base = self.preview.as_ref().and_then(|p| p.doc.as_ref()).unwrap_or(&self.session.doc);
        // A cut shows the part still whole, the red volume it takes drawn over it (26.202, s36-disc-pull).
        let base: std::borrow::Cow<'_, shapecraft_doc::Document> = match &self.cut_base {
            Some(before)
                if show_uncut
                    && matches!(self.tool, Tool::Extrude(_))
                    && self.badge_current(crate::workspace::badges::Badge::Result) == Some(3) =>
            {
                let mut d = base.clone();
                for b in &mut d.bodies {
                    if let Some(old) = before.body(b.id) {
                        b.shape = old.shape.clone();
                        b.mesh = old.mesh.clone();
                    }
                }
                std::borrow::Cow::Owned(d)
            }
            _ => std::borrow::Cow::Borrowed(base),
        };
        // A face growing out: the part as it was goes in under the grown body (drawn see-through, see split_tints).
        let base = match (show_uncut, self.growing_face(), &self.cut_base) {
            (true, Some((body, _)), Some(before)) if let Some(old) = before.body(body) => {
                let mut d = base.into_owned();
                let mut was = old.clone();
                was.id = Self::before_id(body);
                d.bodies.push(was);
                std::borrow::Cow::Owned(d)
            }
            _ => base,
        };
        // A profile pulled into a new body keeps its sketch drawn until Done (the outline shows through the
        // see-through body); the step hides it once it's applied.
        let base = match (&self.tool, &self.cut_base) {
            (Tool::Extrude(t), Some(before))
                if let tools::Target::Region { sketch, .. } = t.target
                    && before.sketch(sketch).is_some_and(|s| s.visible)
                    && base.sketch(sketch).is_some_and(|s| !s.visible) =>
            {
                let mut d = base.into_owned();
                if let Some(sk) = d.sketch_mut(sketch) {
                    sk.visible = true;
                }
                std::borrow::Cow::Owned(d)
            }
            _ => base,
        };
        if !self.view_opts.decals && base.images.iter().any(|i| i.visible) {
            let mut d = base.into_owned();
            for i in &mut d.images {
                i.visible = false;
            }
            if let Some(ids) = &self.isolated {
                for b in &mut d.bodies {
                    b.visible &= ids.contains(&b.id);
                }
            }
            return std::borrow::Cow::Owned(d);
        }
        // View ▸ Show Sketches off: in the Visualized look only the sketch being edited stays.
        let base = if self.view_opts.sketches || !self.visualized_look() {
            base
        } else {
            let mut d = base.into_owned();
            for s in &mut d.sketches {
                if Some(s.id) != self.session.active_sketch {
                    s.visible = false;
                }
            }
            std::borrow::Cow::Owned(d)
        };
        // Section Only / 2D Section View: the bodies give way to their cut faces (drawn as overlays).
        if self.section.is_some_and(|s| s.caps_only()) {
            let mut d = base.into_owned();
            for b in &mut d.bodies {
                b.visible = false;
            }
            for s in &mut d.sketches {
                s.visible = false;
            }
            return std::borrow::Cow::Owned(d);
        }
        match &self.isolated {
            None => base,
            Some(ids) => {
                let mut d = base.into_owned();
                for b in &mut d.bodies {
                    b.visible &= ids.contains(&b.id);
                }
                for s in &mut d.sketches {
                    if Some(s.id) != self.session.active_sketch {
                        s.visible = false;
                    }
                }
                std::borrow::Cow::Owned(d)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod undo_flash {
    use super::*;
    use std::sync::Arc;

    /// Redo flashes the body it brought back and not the one it left alone (an unchanged body keeps its very mesh).
    #[test]
    fn undo_and_redo_flash_only_what_they_changed() {
        if crate::motion::enabled() {
            let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Flash");
            let mut ws = Workspace::new(session, crate::settings::Settings::default());
            for x in [0, 30] {
                ws.session.execute("sketch", &serde_json::json!({"plane": "xy"})).unwrap();
                ws.session.execute("circle", &serde_json::json!({"center": [x, 0], "radius": 5})).unwrap();
                let k = ws.session.active_sketch.unwrap().0;
                ws.session.execute("extrude", &serde_json::json!({"sketch": k, "region": 0, "distance": 10})).unwrap();
            }
            assert_eq!(ws.session.doc.bodies.len(), 2);
            ws.run("undo", serde_json::json!({}));
            assert_eq!(ws.session.doc.bodies.len(), 1);
            assert!(ws.undo_flash.is_none(), "nothing left changed: {:?}", ws.undo_flash);
            ws.run("redo", serde_json::json!({}));
            assert_eq!(ws.session.doc.bodies.len(), 2);
            let flashed = ws.undo_flash.as_ref().map(|f| f.0.len());
            assert_eq!(flashed, Some(1), "only the body that came back");
        }
    }
}

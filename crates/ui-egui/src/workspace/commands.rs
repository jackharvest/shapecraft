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
        let ok = if self.is_live() {
            match self.session.amend(id, &params) {
                Ok(_) => true,
                Err(e) => {
                    self.toast(match e {
                        EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                        other => other.to_string(),
                    });
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
        match self.session.execute(id, &params) {
            Ok(out) => {
                self.remember(id);
                if matches!(id, "select" | "select_all") {
                    self.repredict();
                }
                Some(out.value)
            }
            Err(e) => {
                self.toast(match e {
                    EngineError::NotAvailable(m) | EngineError::BadParams(m) => m,
                    other => other.to_string(),
                });
                None
            }
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
        if (self.is_live() || self.numeric.is_some()) && !self.dragging_handle() {
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
            return;
        };
        let key = format!("{id}{params}");
        if let Some(p) = &self.preview
            && (p.key == key || self.now - p.computed_at < 0.03)
        {
            return;
        }
        let doc = if self.is_live() { self.session.preview_amend(id, &params).ok() } else { self.session.preview(id, &params).ok() };
        self.preview = Some(Preview { key, computed_at: self.now, doc });
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
            let r = self.numeric.as_deref().and_then(tools::parse_length).map(f64::abs).unwrap_or(*radius);
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

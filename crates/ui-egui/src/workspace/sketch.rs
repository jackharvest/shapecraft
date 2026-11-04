//! Sketching: entering and editing sketches, the sketch tools as their picked variants draw, and the geometry
//! helpers they share.

use glam::DVec2;
use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};

use super::Workspace;
use super::toolbar::ToolKind;
use super::tools::Tool;

impl Workspace {
    /// Offset Edge, Chain: grows the picked sketch curves to every curve joined to them end to end.
    pub(super) fn select_chains(&mut self) {
        let mut picked: Vec<(ItemId, u32)> = self
            .session
            .selection
            .iter()
            .filter_map(|p| match *p {
                Pick::SketchEntity { sketch, entity } => Some((sketch, entity)),
                _ => None,
            })
            .collect();
        let Some(&(sketch, _)) = picked.first() else { return };
        let Some(sk) = self.session.doc.sketch(sketch) else { return };
        let ends: Vec<Option<(DVec2, DVec2)>> = sk
            .entities
            .iter()
            .map(|e| {
                let o = e.outline();
                match (o.first(), o.last()) {
                    (Some(a), Some(b)) if a.distance(*b) > 1e-9 => Some((*a, *b)),
                    _ => None,
                }
            })
            .collect();
        let touches = |i: usize, j: usize| match (ends.get(i).copied().flatten(), ends.get(j).copied().flatten()) {
            (Some((a, b)), Some((c, d))) => [a, b].iter().any(|p| p.distance(c) < 1e-6 || p.distance(d) < 1e-6),
            _ => false,
        };
        let mut k = 0;
        while k < picked.len() {
            let i = picked[k].1 as usize;
            for j in 0..ends.len() {
                if picked.iter().all(|p| p.1 as usize != j) && touches(i, j) {
                    picked.push((sketch, j as u32));
                }
            }
            k += 1;
        }
        self.session.selection = picked.into_iter().map(|(sketch, entity)| Pick::SketchEntity { sketch, entity }).collect();
    }

    pub(crate) fn polygon_sides(&self) -> u32 {
        [3, 5, 6, 8][self.variant("Polygon").min(3)]
    }

    /// A sketch tool as its picked variant draws it.
    pub(crate) fn sketch_tool(&self, kind: ToolKind) -> Tool {
        match kind {
            ToolKind::Rectangle if self.variant("Rectangle") == 2 => Tool::Rect3 { a: None, b: None },
            ToolKind::Spline => Tool::Spline { points: Vec::new(), control: self.variant("Spline") == 0 },
            _ => Tool::for_sketch_kind(kind),
        }
    }

    /// The rectangle's first corner: where the first click was, or with Center its mirror through it.
    /// A transform running on picked sketch curves (Move/Rotate, Mirror, Scale, the Patterns, Rotate Around Axis):
    /// 26.200 keeps the sketch column, hides the picked curves' dimensions and the selection pill, and puts Cancel
    /// (and Done once something changed) at the bottom (s11-088..093).
    pub(super) fn sketch_transform(&self) -> Option<bool> {
        self.session.active_sketch.as_ref()?;
        match &self.tool {
            Tool::Move(t) if t.items.is_empty() => Some(t.value.abs() > 1e-9 || t.planar.length() > 1e-9),
            Tool::Mirror(_) => Some(false),
            Tool::Pattern(t) => Some(t.spacing.abs() > 1e-9),
            Tool::CircularPattern(t) => Some(t.about.is_some()),
            Tool::RotateAxis(t) => Some(t.angle.abs() > 1e-9),
            Tool::Scale(_) => Some(false),
            _ => None,
        }
    }

    /// Offset Edge running on a sketch region or a body face from outside a sketch.
    pub(super) fn offsetting_region(&self) -> bool {
        self.session.active_sketch.is_none() && matches!(&self.tool, Tool::OffsetEdge(t) if t.region.is_some() || t.face.is_some())
    }

    /// The sketch column with no plane yet: Offset Edge outside a sketch, or a sketch tool (Offset Edge, Trim, Project)
    /// waiting for its pick from outside one (s24 matrix).
    /// The column is the idle set with the running tool joining it: Move waiting for items, a card tool with its card
    /// open (s24 matrix), or Extrude picked by its key or once pulled (s24-007; side by side 2026-11-02).
    pub(super) fn idle_column(&self) -> bool {
        matches!(self.tool, Tool::Await { kind: super::toolbar::ToolKind::Move, .. })
            || self.card_tool() && self.options_menu.is_some()
            // A body face started with E keeps the face set, Extrude lit, until it's pulled.
            || matches!(&self.tool, Tool::Extrude(t) if !matches!(t.target, super::tools::Target::Face { .. }))
                && (!self.predicted || self.is_live())
            || matches!(self.tool, Tool::Extrude(_)) && self.is_live()
            // A pulled fillet waits for Done under the idle set (26.202, s40-010, s40-012).
            || matches!(&self.tool, Tool::Fillet(t) if !t.chamfer) && self.is_live()
    }

    /// A running tool whose ≡ opens a Parameter Card (step_card.rs) rather than an options menu.
    pub(super) fn card_tool(&self) -> bool {
        matches!(&self.tool, Tool::Extrude(_))
            || matches!(&self.tool, Tool::Fillet(t) if !t.chamfer)
            || matches!(&self.tool, Tool::Mirror(t) if t.split)
    }

    pub(super) fn sketch_column_no_plane(&self) -> bool {
        self.offsetting_region()
            || self.session.active_sketch.is_none()
                && (matches!(self.tool, Tool::Await { kind: ToolKind::OffsetEdge | ToolKind::Trim, .. } | Tool::BodyEdgeOffset(_))
                    || matches!(&self.tool, Tool::Onto(t) if t.sketch))
    }

    /// Where the segment being drawn from `last` ends: the cursor, or at the length fixed with Tab toward it.
    pub(crate) fn line_end(&self, last: DVec2, cursor: DVec2) -> DVec2 {
        match self.line_len {
            Some(len) => last + (cursor - last).normalize_or(DVec2::X) * len,
            None => cursor,
        }
    }

    pub(crate) fn rect_corner(&self, first: DVec2, cursor: DVec2) -> DVec2 {
        if self.variant("Rectangle") == 0 { first * 2.0 - cursor } else { first }
    }

    /// Where the rectangle's moving corner is: the cursor, or with a width typed (and Tab), the cursor's height at
    /// that width on the cursor's side.
    pub(crate) fn rect_cursor(&self, first: DVec2, cursor: DVec2) -> DVec2 {
        let Some(w) = self.rect_width else { return cursor };
        let sx = if cursor.x < first.x { -1.0 } else { 1.0 };
        let half = if self.variant("Rectangle") == 0 { 0.5 } else { 1.0 };
        DVec2::new(first.x + sx * w * half, cursor.y)
    }

    /// The sketch being edited: which of its elements the constraints pin down (found once per change).
    pub(crate) fn defined_elements(&mut self) -> Vec<bool> {
        let Some(id) = self.session.active_sketch else { return Vec::new() };
        let rev = self.session.revision;
        if let Some((r, s, v)) = &self.defined
            && *r == rev
            && *s == id
        {
            return v.clone();
        }
        let v = self
            .session
            .doc
            .sketch(id)
            .map(|s| shapecraft_doc::constraints::fully_defined(&s.entities, &s.constraints))
            .unwrap_or_default();
        self.defined = Some((rev, id, v.clone()));
        v
    }

    /// Opens a sketch as it was at its history step (the steps after it wait until the sketch is left).
    pub(crate) fn edit_sketch_step(&mut self, step: usize) {
        if self.run("edit_sketch_step", json!({ "step": step })).is_none() {
            return;
        }
        self.history.open = None;
        self.history.editing = None;
        if let Some(plane) = self.session.active_sketch.and_then(|id| self.session.doc.sketch(id)).map(|s| s.plane) {
            self.focus_plane(plane);
        }
    }

    /// A shown sketch with curves lying on the plane through `origin` square to `normal`: Space over that plane opens
    /// it instead of starting another (s24 matrix).
    pub(super) fn shown_sketch_on(&self, origin: glam::DVec3, normal: glam::DVec3) -> Option<ItemId> {
        let n = normal.try_normalize()?;
        self.session
            .doc
            .sketches
            .iter()
            .filter(|s| s.visible && !s.entities.is_empty())
            .find(|s| s.plane.normal().dot(n).abs() > 1.0 - 1e-9 && (s.plane.origin - origin).dot(n).abs() < 1e-6)
            .map(|s| s.id)
    }

    /// Opens a finished sketch for editing where it is in the history, without turning the camera (a double-click
    /// on its curves, as observed: the "Normal to Sketch" pill offers the turn).
    pub(crate) fn open_sketch_in_place(&mut self, sketch: ItemId) -> bool {
        let Some(step) = self.session.doc.history.iter().position(|h| h.kind == "sketch" && h.sketch == Some(sketch)) else { return false };
        if self.run("edit_sketch_step", json!({ "step": step })).is_none() {
            return false;
        }
        self.history.open = None;
        self.history.editing = None;
        true
    }

    /// Starts a sketch where the user clicked, without turning the camera or closing History (26.200).
    pub(super) fn start_sketch_in_place(&mut self, params: Value) -> bool {
        if self.run("sketch", params).is_none() {
            return false;
        }
        let kind = self.pending_tool.take().unwrap_or(ToolKind::Line);
        self.tool = self.sketch_tool(kind);
        true
    }

    pub fn start_sketch_on(&mut self, params: Value) -> bool {
        let face = params.get("body").and_then(Value::as_u64).zip(params.get("face").and_then(Value::as_u64));
        if self.run("sketch", params).is_none() {
            return false;
        }
        self.show_history = false;
        if let Some(sk) = self.session.active_sketch() {
            let plane = sk.plane;
            self.focus_plane(plane);
            if let Some((body, face)) = face {
                self.frame_face(plane, ItemId(body), face as u32);
            }
        }
        // As observed, a new sketch starts with Line armed unless another sketch tool was waiting for the plane.
        let kind = self.pending_tool.take().unwrap_or(ToolKind::Line);
        self.tool = self.sketch_tool(kind);
        true
    }
}

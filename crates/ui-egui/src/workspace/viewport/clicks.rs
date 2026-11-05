//! Clicks in the view: the running tool takes them or they select; double-clicks; and the press-drag-release
//! drawing of sketch tools and the sketch line's sideways handle.

use egui::{Pos2, Rect, Ui, pos2};
use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{Pick, SketchEntity, SketchPlane};

use super::dimensions::{DimEdit, DimKind};
use super::handles::SECTION_FLIP;
use super::snap::ellipse_minor;
use crate::workspace::ToolKind;
use crate::workspace::Workspace;
use crate::workspace::tools::{AXES, Tool};

impl Workspace {
    pub(super) fn handle_clicks(&mut self, ui: &Ui, resp: &egui::Response) {
        if resp.secondary_clicked()
            && let Some(c) = self.cursor
        {
            let under = self.pick_at(c).map(|(p, _)| p);
            self.context_menu = Some((c, under, self.now));
            return;
        }
        if resp.double_clicked()
            && let Some(c) = self.cursor
            && self.double_click(c)
        {
            return;
        }
        if !resp.clicked() {
            return;
        }
        let Some(c) = self.cursor else { return };
        // A running tool's badges open their menus.
        if self.badge_click(ui, c) {
            return;
        }
        // Visualization: a click on a decal picks it (not the body under it); anywhere else lets it go.
        if self.visualizing {
            self.env_settings = false;
            if let Some(d) = self.decal_at(c) {
                self.decal_sel = Some(d);
                self.session.selection.clear();
                return;
            }
            self.decal_sel = None;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        // Section View's reference: an origin plane or a flat face (s11-117).
        if self.section_pick.is_some() {
            let (o, n) = if let Some(i) = self.tile_under(c) {
                (DVec3::ZERO, [DVec3::Z, DVec3::Y, DVec3::X].get(i).copied().unwrap_or(DVec3::Z))
            } else if let Some((Pick::Face { body, face }, _)) = self.pick_at(c)
                && let Some(fp) = self.session.doc.body(body).and_then(|b| self.session.kernel.face_plane(&b.shape, face).ok().flatten())
            {
                (fp.origin, fp.normal)
            } else {
                return;
            };
            // Keep the half away from the camera: the normal points at the side cut away (s16-021: a top face
            // keeps the whole body, its arrow pointing up).
            let n = if n.dot(self.camera.back()) < 0.0 { -n } else { n };
            self.section_pick = None;
            self.section = Some(crate::workspace::Section::new(o, n));
            return;
        }
        // Editing a section: its flip handle turns it round; anywhere else ends the editing (s16-025).
        if let Some(s) = self.section.filter(|s| s.editing && matches!(self.tool, Tool::Idle)) {
            let flip = self.handle_frame(s.origin(), s.normal).map(|(s0, sdir, _)| s0 + sdir * SECTION_FLIP);
            if let Some(sec) = &mut self.section {
                if flip.is_some_and(|f| f.distance(c) < 11.0) {
                    let at = sec.origin();
                    sec.normal = -sec.normal;
                    sec.center = at;
                    sec.offset = 0.0;
                } else {
                    sec.editing = false;
                }
            }
            return;
        }
        // Measure's point modes pick points on the model (two for a distance, three for an angle), starting over after.
        if self.measuring && matches!(self.tool, Tool::Idle) && self.variant("Measure mode") > 0 {
            if let Some((_, at)) = self.pick_at(c) {
                let need = if self.variant("Measure mode") == 1 { 2 } else { 3 };
                if self.measure_points.len() >= need {
                    self.measure_points.clear();
                }
                self.measure_points.push(at);
            }
            return;
        }
        // Offset Face's "Total ⌄" / "Offset ⌄" pill opens the Distance Type menu.
        // On a fillet's face the pill is "Edit Fillet NN": it opens that step in History.
        if let Tool::OffsetFace(t) = &self.tool
            && let Some(f) = &t.fillet
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("offset-kind-pill")))
            && pr.contains(c)
        {
            let step = f.step;
            self.open_history_step(step);
            return;
        }
        if matches!(self.tool, Tool::OffsetFace(_))
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("offset-kind-pill")))
            && pr.contains(c)
        {
            self.distance_menu = if self.distance_menu.is_some() { None } else { Some(pos2(pr.right() + 10.0, pr.center().y)) };
            self.popup_opened_at = self.now;
            return;
        }
        // Linear Pattern's "Spacing ⌄" / "Total ⌄" pill opens its Distance Type menu.
        if matches!(self.tool, Tool::Pattern(_))
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("pattern-kind-pill")))
            && pr.expand(2.0).contains(c)
        {
            self.distance_menu = if self.distance_menu.is_some() { None } else { Some(pos2(pr.right() + 10.0, pr.center().y)) };
            self.popup_opened_at = self.now;
            return;
        }
        // The Link badge (Copy lit): unlinked copies keep their own geometry in the History.
        if let Tool::Move(t) = &self.tool
            && t.copy
            && let Some(lr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("move-link-pill")))
            && lr.expand(2.0).contains(c)
        {
            if let Tool::Move(t) = &mut self.tool {
                t.unlinked = !t.unlinked;
            }
            if self.is_live() {
                self.commit_tool();
            }
            return;
        }
        // The Move gizmo's Copy pill toggles copying.
        if let Tool::Move(t) = &self.tool
            && !t.items.is_empty()
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("move-copy-pill")))
            && pr.expand(3.0).contains(c)
        {
            if let Tool::Move(t) = &mut self.tool {
                t.copy = !t.copy;
            }
            // A live move takes the change at once.
            if self.is_live() {
                self.commit_tool();
            }
            return;
        }
        if let Tool::Align { items, from: Some((fb, ff)), to: Some(to), flip, edge } = &self.tool
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("align-flip")))
            && pr.expand(3.0).contains(c)
        {
            let flip = !*flip;
            let edge = *edge;
            let ids: Vec<u64> = items.iter().map(|i| i.0).collect();
            let from_json = if edge { json!({ "body": fb.0, "edge": ff }) } else { json!({ "body": fb.0, "face": ff }) };
            let params = json!({ "from": from_json, "to": to, "items": ids, "flip": flip });
            let (items, from, to) = (items.clone(), Some((*fb, *ff)), Some(to.clone()));
            if self.run_live("align", params, false) {
                self.tool = Tool::Align { items, from, to, flip, edge };
            }
            return;
        }
        if let Tool::RotateAxis(t) = &self.tool
            && t.axis.is_some()
            && t.copy
            && let Some(lr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("rotaxis-link-pill")))
            && lr.expand(2.0).contains(c)
        {
            if let Tool::RotateAxis(t) = &mut self.tool {
                t.unlinked = !t.unlinked;
            }
            if self.is_live() {
                self.commit_tool();
            }
            return;
        }
        if let Tool::RotateAxis(t) = &self.tool
            && t.axis.is_some()
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("rotaxis-copy-pill")))
            && pr.expand(3.0).contains(c)
        {
            if let Tool::RotateAxis(t) = &mut self.tool {
                t.copy = !t.copy;
            }
            if self.is_live() {
                self.commit_tool();
            }
            return;
        }
        if let Tool::Scale(_) = &self.tool
            && let Some(pr) = ui.ctx().data(|d| d.get_temp::<Rect>(ui.scope_id().with("scale-copy-pill")))
            && pr.expand(3.0).contains(c)
        {
            if let Tool::Scale(t) = &mut self.tool {
                t.copy = !t.copy;
            }
            if self.is_live() {
                self.commit_tool();
            }
            return;
        }
        // A constraint badge: select it (Delete removes it).
        if let Some((_, sketch, i, _)) = self.constraint_badges().into_iter().find(|b| b.0.contains(c)) {
            self.selected_constraint = Some((sketch, i));
            return;
        }
        self.selected_constraint = None;
        // A driving dimension's value: type a new one (s11-037).
        if let Some((sketch, entity, kind, ..)) =
            self.driving_dims().0.into_iter().find(|d| (Self::driving_label(d.2, d.5) - c).length() < 30.0)
        {
            self.dim_edit = Some(DimEdit { sketch, entity, kind, other: None, pair: None });
            self.numeric = Some(String::new());
            return;
        }
        // A distance from a point, kept or shown for what's picked (s15-012).
        if let Some(d) = self.point_dims().into_iter().find(|d| (d.label - c).length() < 34.0 && (d.label.y - c.y).abs() < 14.0) {
            self.dim_edit =
                Some(DimEdit { sketch: d.sketch, entity: u32::MAX, kind: DimKind::PointDistance, other: None, pair: Some((d.a, d.b)) });
            self.numeric = Some(String::new());
            return;
        }
        // An angle between two lines, kept or shown for the two picked ones (s15-007).
        if let Some(a) = self.angle_dims().into_iter().find(|a| (a.label - c).length() < 30.0) {
            self.dim_edit = Some(DimEdit { sketch: a.sketch, entity: a.first, kind: DimKind::Angle, other: Some(a.second), pair: None });
            self.numeric = Some(String::new());
            return;
        }
        if let Some(d) = self
            .entity_dims()
            .into_iter()
            .filter(|_| self.picked_corner().is_none())
            .find(|d| (d.label - c).length() < 40.0 && (d.label.y - c.y).abs() < 12.0)
        {
            self.dim_edit = Some(DimEdit { sketch: d.sketch, entity: d.entity, kind: d.kind, other: None, pair: None });
            self.numeric = Some(String::new());
            return;
        }
        if self.dim_edit.take().is_some() {
            self.numeric = None;
        }
        self.click_at(c, shift);
    }

    /// The ↕ on a selected sketch line, as observed (430–432): at the line's middle, pointing out of its rectangle.
    /// Returns (sketch, line, handle center, the screen direction it pushes, the sketch direction, px per unit).
    pub(in crate::workspace) fn side_handle(&self) -> Option<(shapecraft_doc::ItemId, u32, Pos2, egui::Vec2, DVec2, f64)> {
        if !matches!(self.tool, Tool::Idle) {
            return None;
        }
        let sk = self.session.active_sketch()?;
        let [Pick::SketchEntity { sketch, entity }] = self.session.selection[..] else { return None };
        if sketch != sk.id {
            return None;
        }
        let Some(&SketchEntity::Line { a, b }) = sk.entities.get(entity as usize) else { return None };
        let mid = (a + b) / 2.0;
        let mut n = (b - a).perp().try_normalize()?;
        let world = |p: DVec2| self.to_screen(sk.plane.to_world(p));
        let (m, e) = (world(mid)?, world(mid + n)?);
        let mut ns = e - m;
        let px = ns.length() as f64;
        if px < 1e-6 {
            return None;
        }
        ns /= px as f32;
        // Out of the rectangle; a loose line's arrow goes down the screen.
        let outward =
            match shapecraft_doc::rectangle_loop(&sk.entities, entity).and_then(|lp| shapecraft_doc::rectangle_frame(&sk.entities, lp)) {
                Some(f) => (mid - (f.origin + f.u * f.width / 2.0 + f.v * f.height / 2.0)).dot(n) >= 0.0,
                None => ns.y >= 0.0,
            };
        if !outward {
            n = -n;
            ns = -ns;
        }
        Some((sketch, entity, m + ns * 20.0, ns, n, px))
    }

    /// Dragging a selected line's ↕ moves it sideways; lines meeting its ends stretch with it (a rectangle stays a
    /// rectangle). One step, live while dragging.
    pub(super) fn side_drags(&mut self, ui: &Ui, resp: &egui::Response) -> bool {
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            && let Some((sketch, entity, c, ..)) = self.side_handle()
            && origin.distance(c) < 14.0
            && let Some(sk) = self.session.doc.sketch(sketch)
        {
            self.live_step = None;
            self.side_drag = Some((sketch, entity, sk.entities.clone(), origin));
            return true;
        }
        let Some((sketch, entity, base, start)) = self.side_drag.clone() else { return false };
        if let (Some((.., ns, n, px)), Some(p)) = (self.side_handle(), ui.input(|i| i.pointer.interact_pos())) {
            let d = (p - start).dot(ns) as f64 / px;
            let edits = moved_side(&base, entity, n * d);
            let nothing = d.abs() < 1e-9 || edits.is_empty();
            self.run_live("set_entity", json!({ "sketch": sketch.0, "entities": edits }), nothing);
            self.session.selection = vec![Pick::SketchEntity { sketch, entity }];
        }
        if resp.drag_stopped() {
            self.side_drag = None;
        }
        true
    }

    /// Press, drag, release draws with the two-point sketch tools, as observed: the press places the first point
    /// and the release the second (a short drag is just a click).
    pub(super) fn sketch_drags(&mut self, ui: &Ui, resp: &egui::Response) -> bool {
        let two_point = matches!(
            self.tool,
            Tool::Line { last: None, .. }
                | Tool::Rectangle { first: None }
                | Tool::Circle { center: None }
                | Tool::Polygon { center: None }
                | Tool::Ellipse { center: None, .. }
        ) || (matches!(self.tool, Tool::PickPlane { .. })
            && matches!(
                self.pending_tool,
                Some(ToolKind::Line | ToolKind::Rectangle | ToolKind::Circle | ToolKind::Polygon | ToolKind::Ellipse)
            ));
        // A drawing tool still waiting for its plane: the press picks the plane and the first point, as a click does.
        if resp.drag_started_by(egui::PointerButton::Primary)
            && two_point
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        {
            let now = self.cursor;
            self.cursor = Some(origin);
            self.click_at(origin, false);
            self.cursor = now;
            self.sketch_drag = Some(origin);
            return true;
        }
        let Some(origin) = self.sketch_drag else { return false };
        if resp.drag_stopped() {
            self.sketch_drag = None;
            if let Some(end) = ui.input(|i| i.pointer.interact_pos())
                && (end - origin).length() > 6.0
            {
                let now = self.cursor;
                self.cursor = Some(end);
                self.click_at(end, false);
                self.cursor = now;
                // A dragged line is one segment, not the start of a chain.
                if let Tool::Line { .. } = self.tool {
                    self.tool = Tool::Line { first: None, last: None };
                }
            }
        }
        true
    }

    /// A click in the view at `c`: the running tool takes it, or it selects.
    /// Double-clicks, as observed (413–419): on a body's face or edge the whole body is selected (Move/Rotate comes
    /// up); on a finished sketch's curve or region that sketch opens for editing where it is in the history, the
    /// camera staying put, with the curve's chain or the region's outline selected. Elsewhere nothing happens.
    fn double_click(&mut self, c: Pos2) -> bool {
        if self.session.active_sketch.is_some() || self.visualizing || self.drawings.is_some() {
            return false;
        }
        // Only over what the first click started (a predicted tool waiting for a drag), never a tool in use.
        let fresh = matches!(self.tool, Tool::Idle | Tool::OffsetFace(_) | Tool::Fillet(_) | Tool::Move(_) | Tool::Extrude(_));
        if !fresh || self.live_step == Some(self.session.revision) {
            return false;
        }
        match self.pick_at(c).map(|(p, _)| p) {
            Some(Pick::Face { body, .. } | Pick::Edge { body, .. }) => {
                self.tool = Tool::Idle;
                self.session.selection = vec![Pick::Body { body }];
                self.predict_tool();
                true
            }
            Some(Pick::SketchEntity { sketch, entity }) => {
                self.tool = Tool::Idle;
                if !self.open_sketch_in_place(sketch) {
                    return false;
                }
                self.session.selection = vec![Pick::SketchEntity { sketch, entity }];
                self.select_chains();
                true
            }
            Some(Pick::Region { sketch, region }) => {
                self.tool = Tool::Idle;
                let entities = self.session.doc.sketch(sketch).map(|s| s.region_entities(region)).unwrap_or_default();
                if !self.open_sketch_in_place(sketch) {
                    return false;
                }
                self.session.selection = entities.into_iter().map(|entity| Pick::SketchEntity { sketch, entity }).collect();
                true
            }
            _ => false,
        }
    }

    fn click_at(&mut self, c: Pos2, shift: bool) {
        self.items_focus = false;
        // A step's Profile ▸ Edit...: a click on one of its sketch's regions picks it or lets it go.
        if let Some((_, sketch, _)) = self.profile_edit.clone() {
            let (o, d) = self.camera.ray(self.local(c), self.size());
            let hit = self.session.doc.sketch(sketch).and_then(|sk| {
                let at = sk.plane.intersect_ray(o, d)?;
                let local = sk.plane.to_local(at);
                sk.regions().iter().find(|r| r.contains(local)).map(|r| r.index)
            });
            if let (Some(r), Some((_, _, regions))) = (hit, self.profile_edit.as_mut()) {
                if let Some(k) = regions.iter().position(|x| *x == r) {
                    regions.remove(k);
                } else {
                    regions.push(r);
                }
            }
            return;
        }
        // Extrude's To Object / From Plane: the click picks that face or plane.
        if let Tool::Extrude(t) = &self.tool
            && let Some(which) = t.picking
        {
            let target = match self.pick_at(c).map(|(p, _)| p) {
                Some(Pick::Face { body, face }) => Some(json!({ "body": body.0, "face": face })),
                Some(Pick::Plane { plane }) => Some(json!({ "plane": plane.0 })),
                _ => None,
            };
            if let (Some(target), Tool::Extrude(t)) = (target, &mut self.tool) {
                t.picking = None;
                if which == "to" {
                    t.to = Some(target);
                    if t.distance.abs() < 1e-9 {
                        t.distance = 1.0;
                    }
                } else {
                    t.start_plane = Some(target);
                }
                let t = t.clone();
                if t.distance.abs() > 1e-9 {
                    self.apply_extrude(&t);
                }
            }
            return;
        }
        match self.tool.clone() {
            Tool::Trim { target: Some((entity, at)) } => {
                let _ = self.run("trim", json!({ "entity": entity, "at": [at.x, at.y] }));
                self.tool = Tool::Trim { target: None };
            }
            Tool::Trim { target: None } | Tool::OffsetEdge(_) | Tool::OffsetPlane(_) => {}
            Tool::Text { .. } => {}
            Tool::SketchFillet { .. } => {
                let near = match (&self.tool, self.sketch_plane()) {
                    (Tool::SketchFillet { all, .. }, Some(pl)) => {
                        all.iter().copied().find(|q| self.to_screen(pl.to_world(*q)).is_some_and(|sp| sp.distance(c) < 14.0))
                    }
                    _ => None,
                };
                if let (Some(q), Tool::SketchFillet { corners, .. }) = (near, &mut self.tool) {
                    if let Some(k) = corners.iter().position(|x| x.distance(q) < 1e-7) {
                        corners.remove(k);
                    } else {
                        corners.push(q);
                    }
                }
            }
            Tool::Project => match self.pick_at(c) {
                Some((Pick::Edge { body, edge }, _)) => {
                    let _ = self.run("project", json!({ "edges": [{ "body": body.0, "edge": edge }] }));
                }
                Some((Pick::Face { body, face }, _)) => {
                    let _ = self.run("project", json!({ "faces": [{ "body": body.0, "face": face }] }));
                }
                _ => {}
            },
            Tool::Align { items, from, to, flip, edge } => match (from, self.pick_at(c)) {
                (None, Some((Pick::Face { body, face }, _))) if items.contains(&body) => {
                    self.session.selection = vec![Pick::Face { body, face }];
                    self.tool = Tool::Align { items, from: Some((body, face)), to, flip, edge: false };
                }
                // A straight edge of the items: it'll turn parallel to the edge picked next.
                (None, Some((Pick::Edge { body, edge: e }, _))) if items.contains(&body) => {
                    self.session.selection = vec![Pick::Edge { body, edge: e }];
                    self.tool = Tool::Align { items, from: Some((body, e)), to, flip, edge: true };
                }
                (Some((fb, ff)), Some((target, _))) => {
                    let target = match target {
                        Pick::Face { body, face } if !items.contains(&body) && !edge => Some(json!({ "body": body.0, "face": face })),
                        Pick::Plane { plane } if !edge => Some(json!({ "plane": plane.0 })),
                        Pick::Edge { body, edge: e } if !items.contains(&body) && edge => Some(json!({ "body": body.0, "edge": e })),
                        _ => None,
                    };
                    match target {
                        // Applied at once, as observed (387); another target replaces it.
                        Some(t) => {
                            let ids: Vec<u64> = items.iter().map(|i| i.0).collect();
                            let from_json = if edge { json!({ "body": fb.0, "edge": ff }) } else { json!({ "body": fb.0, "face": ff }) };
                            let params = json!({ "from": from_json, "to": t, "items": ids, "flip": flip });
                            if self.run_live("align", params, false) {
                                self.session.selection = items.iter().map(|&body| Pick::Body { body }).collect();
                                self.tool = Tool::Align { items, from: Some((fb, ff)), to: Some(t), flip, edge };
                            }
                        }
                        None => self.tool = Tool::Align { items, from: Some((fb, ff)), to, flip, edge },
                    }
                }
                _ => {}
            },
            Tool::EdgePlane(_) | Tool::BodyEdgeOffset(_) => {}
            Tool::Onto(mut t) => {
                // Every pick works on the model as it was: the live step comes off first.
                if self.is_live() {
                    self.session.drop_last_step();
                    self.live_step = None;
                }
                t.error = None;
                match self.pick_at(c) {
                    // An item: clicked again it leaves.
                    Some((p, at)) if t.takes_item(&p) => match t.items.iter().position(|(i, _)| *i == p) {
                        Some(k) => {
                            t.items.remove(k);
                        }
                        None => t.items.push((p, at)),
                    },
                    Some((Pick::Face { body, face }, at)) => t.to = Some((json!({ "body": body.0, "face": face }), at)),
                    // Replace Face also takes a plane (Project as Edge needs a face to imprint).
                    Some((Pick::Plane { plane }, at)) if !t.project || t.sketch => t.to = Some((json!({ "plane": plane.0 }), at)),
                    _ => {}
                }
                self.apply_onto(t);
            }
            Tool::Boolean(mut t) => {
                // Subtract's role badges sit at each body's middle: a click swaps the role.
                let badge = (t.op == "subtract")
                    .then(|| {
                        t.targets.iter().chain(&t.tools).copied().find(|id| {
                            self.session
                                .doc
                                .body(*id)
                                .and_then(|b| self.session.kernel.bounds(&b.shape).ok())
                                .and_then(|bb| self.to_screen((bb.min + bb.max) / 2.0))
                                .is_some_and(|m| (m.x - c.x).abs() < 16.0 && (m.y - c.y).abs() < 12.0)
                        })
                    })
                    .flatten();
                if let Some(body) = badge {
                    t.swap_role(body);
                    self.session.selection = t.targets.iter().chain(&t.tools).map(|&body| Pick::Body { body }).collect();
                } else if let Some(body) = self.pick_at(c).and_then(|(p, _)| p.body()) {
                    t.toggle(body);
                    self.session.selection = t.targets.iter().chain(&t.tools).map(|&body| Pick::Body { body }).collect();
                }
                self.tool = Tool::Boolean(t);
            }
            Tool::Translate { items, from, copy, .. } => {
                // Off the model, the end point lands on the ground grid.
                let at = self.snap_point(c).or_else(|| {
                    let (o, d) = self.camera.ray(self.local(c), self.size());
                    SketchPlane::XY.intersect_ray(o, d)
                });
                if let Some(at) = at {
                    self.tool = match from {
                        None => Tool::Translate { items, from: Some(at), to: None, copy },
                        Some(f) => Tool::Translate { items, from: Some(f), to: Some(at), copy },
                    };
                }
            }
            Tool::PointPlane { perp, base } => {
                let hit = self.pick_at(c);
                match (base, hit) {
                    (None, Some((p @ Pick::Edge { .. }, _))) if perp => self.tool = Tool::PointPlane { perp, base: Some(p) },
                    (None, Some((p @ (Pick::Face { .. } | Pick::Plane { .. }), _))) if !perp => {
                        self.tool = Tool::PointPlane { perp, base: Some(p) }
                    }
                    (Some(b), Some((_, at))) => {
                        let point = [at.x, at.y, at.z];
                        let ok = match b {
                            Pick::Edge { body, edge } => {
                                self.run("plane_perp_edge", json!({ "body": body.0, "edge": edge, "point": point }))
                            }
                            Pick::Face { body, face } => {
                                self.run("plane_parallel_face", json!({ "from": { "body": body.0, "face": face }, "point": point }))
                            }
                            Pick::Plane { plane } => {
                                self.run("plane_parallel_face", json!({ "from": { "plane": plane.0 }, "point": point }))
                            }
                            _ => None,
                        };
                        if ok.is_some() {
                            self.tool = Tool::Idle;
                        }
                    }
                    _ => {}
                }
            }
            Tool::Axis2 { mut points } => {
                if let Some((_, hit)) = self.pick_at(c) {
                    points.push(hit);
                }
                if points.len() == 2 {
                    let pts: Vec<[f64; 3]> = points.iter().map(|p| p.to_array()).collect();
                    let ok = self.run("axis_2points", json!({ "points": pts })).is_some();
                    self.tool = if ok { Tool::Idle } else { Tool::Axis2 { points: Vec::new() } };
                } else {
                    self.tool = Tool::Axis2 { points };
                }
            }
            Tool::Plane3 { mut points } => {
                if let Some((_, hit)) = self.pick_at(c) {
                    points.push(hit);
                }
                if points.len() == 3 {
                    let pts: Vec<[f64; 3]> = points.iter().map(|p| p.to_array()).collect();
                    let ok = self.run("plane_3points", json!({ "points": pts })).is_some();
                    self.tool = if ok { Tool::Idle } else { Tool::Plane3 { points: Vec::new() } };
                } else {
                    self.tool = Tool::Plane3 { points };
                }
            }
            // 26.200: the click that picks the plane also places the waiting tool's first point, and the camera
            // stays where it is ("Normal to Sketch" offers the turn). Empty floor counts as the XY plane (s11-008..022).
            Tool::PickPlane { .. } => {
                let params = if let Some(i) = self.tile_under(c) {
                    let plane = ["xy", "xz", "yz"].get(i).copied().unwrap_or("xy");
                    Some(json!({ "plane": plane }))
                } else {
                    match self.pick_at(c) {
                        Some((Pick::Face { body, face }, _)) => Some(json!({ "body": body.0, "face": face })),
                        Some((Pick::Plane { plane }, _)) => Some(json!({ "construction": plane.0 })),
                        _ => {
                            let (o, d) = self.camera.ray(self.local(c), self.size());
                            SketchPlane::XY.intersect_ray(o, d).is_some().then(|| json!({ "plane": "xy" }))
                        }
                    }
                };
                if let Some(params) = params
                    && self.start_sketch_in_place(params)
                {
                    self.click_at(c, shift);
                    // A tutorial turns to the plane for the beginner (its scenes draw face-on, s30-036).
                    if self.tutorial.is_some()
                        && let Some(plane) = self.session.active_sketch().map(|s| s.plane)
                    {
                        self.focus_plane(plane);
                    }
                }
            }
            Tool::Rectangle { first } => {
                if let Some(p) = self.sketch_cursor() {
                    match first {
                        None => {
                            self.rect_width = None;
                            self.tool = Tool::Rectangle { first: Some(p) };
                        }
                        Some(a) => {
                            let p = self.rect_cursor(a, p);
                            self.add_rectangle(self.rect_corner(a, p), p);
                        }
                    }
                }
            }
            Tool::Rect3 { a, b } => {
                if let Some(p) = self.sketch_cursor() {
                    match (a, b) {
                        (None, _) => self.tool = Tool::Rect3 { a: Some(p), b: None },
                        (Some(a), None) if a.distance(p) > 1e-6 => self.tool = Tool::Rect3 { a: Some(a), b: Some(p) },
                        (Some(a), Some(b)) => self.add_rect3(a, b, p),
                        _ => {}
                    }
                }
            }
            Tool::Circle { center } => {
                if let Some(p) = self.sketch_cursor() {
                    match center {
                        None => self.tool = Tool::Circle { center: Some(p) },
                        Some(ctr) => {
                            let r = ctr.distance(p);
                            if r > 1e-6 {
                                self.add_circle(ctr, r);
                            }
                        }
                    }
                }
            }
            Tool::Ellipse { center, major } => {
                if let Some(p) = self.sketch_cursor() {
                    match (center, major) {
                        (None, _) => self.tool = Tool::Ellipse { center: Some(p), major: None },
                        (Some(c), None) if c.distance(p) > 1e-6 => self.tool = Tool::Ellipse { center: Some(c), major: Some(p) },
                        (Some(c), Some(m)) => self.add_ellipse(c, m, ellipse_minor(c, m, p)),
                        _ => {}
                    }
                }
            }
            Tool::Spline { mut points, control } => {
                if let Some(p) = self.sketch_cursor() {
                    // Clicking the last point again ends the spline.
                    if points.last().is_some_and(|l| l.distance(p) < 1e-6) {
                        self.finish_spline(points, control);
                    } else {
                        points.push(p);
                        self.tool = Tool::Spline { points, control };
                    }
                }
            }
            Tool::Arc { a, b } => {
                if let Some(p) = self.sketch_cursor() {
                    match (a, b) {
                        (None, _) => self.tool = Tool::Arc { a: Some(p), b: None },
                        (Some(a), None) if a.distance(p) > 1e-6 => self.tool = Tool::Arc { a: Some(a), b: Some(p) },
                        (Some(a), Some(b)) => {
                            if let Some((a, mid, b)) = self.arc_preview(a, Some(b), p) {
                                self.add_arc(a, mid, b);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Tool::Polygon { center } => {
                if let Some(p) = self.sketch_cursor() {
                    match center {
                        None => self.tool = Tool::Polygon { center: Some(p) },
                        Some(ctr) if ctr.distance(p) > 1e-6 => self.add_polygon(ctr, ctr.distance(p), (p - ctr).to_angle()),
                        Some(_) => {}
                    }
                }
            }
            Tool::Line { first, last } => {
                if let Some(p) = self.sketch_cursor() {
                    let p = last.map_or(p, |l| self.line_end(l, p));
                    let fixed = self.line_len.take();
                    match last {
                        None => self.tool = Tool::Line { first: Some(p), last: Some(p) },
                        Some(l) if l.distance(p) > 1e-9 => {
                            if self.add_line(l, p) {
                                if let Some(len) = fixed {
                                    self.drive("distance", len, |_| 0.0);
                                }
                                let closed = first.is_some_and(|f| f.distance(p) < 1e-9);
                                self.tool =
                                    if closed { Tool::Line { first: None, last: None } } else { Tool::Line { first, last: Some(p) } };
                            }
                        }
                        Some(_) => {}
                    }
                }
            }
            Tool::Mirror(mut t) if t.split => {
                // Every pick works on the model as it was: the live split comes off first.
                if self.is_live() {
                    self.session.drop_last_step();
                    self.live_step = None;
                }
                let target = if let Some(i) = self.mirror_tile_under(c) {
                    let (o, d) = self.camera.ray(self.local(c), self.size());
                    let plane = [SketchPlane::YZ, SketchPlane::XZ, SketchPlane::XY][i.min(2)];
                    plane.intersect_ray(o, d).map(|at| (json!({ "origin": [0.0, 0.0, 0.0], "normal": AXES[i.min(2)].to_array() }), at))
                } else {
                    match self.pick_at(c) {
                        Some((Pick::Face { body, face }, at)) if !t.items.contains(&body) => {
                            Some((json!({ "body": body.0, "face": face }), at))
                        }
                        Some((Pick::Plane { plane }, at)) => Some((json!({ "plane": plane.0 }), at)),
                        Some((Pick::SketchEntity { sketch, entity }, at)) => Some((json!({ "sketch": sketch.0, "entity": entity }), at)),
                        _ => None,
                    }
                };
                // Each splitting tool clicked joins (s12-014); clicked again it leaves.
                if let Some((w, at)) = target {
                    match t.with.iter().position(|(x, _)| *x == w) {
                        Some(k) => {
                            t.with.remove(k);
                        }
                        None => t.with.push((w, at)),
                    }
                }
                self.apply_split(t);
            }
            Tool::Mirror(t) => {
                let normal_origin = if let Some(i) = self.mirror_tile_under(c) {
                    Some((AXES[i.min(2)], DVec3::ZERO))
                } else {
                    match self.pick_at(c) {
                        Some((Pick::Face { body, face }, _)) => self
                            .session
                            .doc
                            .body(body)
                            .and_then(|b| self.session.kernel.face_plane(&b.shape, face).ok().flatten())
                            .map(|fp| (fp.normal, fp.origin)),
                        Some((Pick::Plane { plane }, _)) => self.session.doc.plane(plane).map(|p| (p.plane.normal(), p.plane.origin)),
                        // A sketch line mirrors across itself (in its sketch's plane).
                        Some((Pick::SketchEntity { sketch, entity }, _)) => {
                            self.session.doc.sketch(sketch).and_then(|sk| match sk.entities.get(entity as usize) {
                                Some(SketchEntity::Line { a, b }) => {
                                    let (wa, wb) = (sk.plane.to_world(*a), sk.plane.to_world(*b));
                                    sk.plane.normal().cross(wb - wa).try_normalize().map(|n| (n, wa))
                                }
                                _ => None,
                            })
                        }
                        _ => None,
                    }
                };
                if let Some((n, o)) = normal_origin {
                    let id = if t.split { "split_body" } else { "mirror" };
                    let mut params = json!({ "origin": [o.x, o.y, o.z], "normal": [n.x, n.y, n.z] });
                    params[if t.split { "keep" } else { "copy" }] = json!(t.keep);
                    if !t.items.is_empty() {
                        params["items"] = json!(t.items.iter().map(|i| i.0).collect::<Vec<_>>());
                    }
                    if self.run(id, params).is_some() {
                        self.tool = Tool::Idle;
                    }
                }
            }
            Tool::Revolve(mut t) => {
                // A line of the same sketch, or a straight or round edge, becomes the axis.
                if let Some((p, _)) = self.pick_at(c)
                    && let Some(a) = crate::workspace::tools::AxisPick::from_click(&self.session, p, t.sketch())
                {
                    t.axis = Some(a);
                    self.tool = Tool::Revolve(t);
                    self.preview = None;
                }
            }
            Tool::CircularPattern(mut t) => {
                if let Some((p, _)) = self.pick_at(c)
                    && let Some(a) = crate::workspace::tools::CircularPatternTool::axis_from_click(&self.session, p)
                {
                    t.about = Some(a);
                    self.tool = Tool::CircularPattern(t);
                    self.preview = None;
                }
            }
            Tool::RotateAxis(mut t) => {
                // Turning sketch curves: a sketch point under the pointer can be the axis too.
                let point = if t.items.is_empty() { self.sketch_point_at(c) } else { None };
                if t.about.is_none()
                    && let Some(p) = point.or_else(|| self.pick_at(c).map(|(p, _)| p))
                    && t.set_axis(&self.session, p)
                {
                    self.tool = Tool::RotateAxis(t);
                    self.preview = None;
                }
            }
            Tool::Pattern(_) => {}
            Tool::Loft(mut t) => {
                if let Some((p, _)) = self.pick_at(c)
                    && t.click(p)
                {
                    self.session.selection = t.profiles.clone();
                    self.preview = None;
                }
                self.tool = Tool::Loft(t);
            }
            Tool::Sweep(mut t) => {
                if let Some((p, _)) = self.pick_at(c)
                    && t.click(p)
                {
                    self.session.selection = t.picks();
                    self.preview = None;
                }
                self.tool = Tool::Sweep(t);
            }
            Tool::Idle
            | Tool::Await { .. }
            | Tool::Extrude(_)
            | Tool::Fillet(_)
            | Tool::Shell(_)
            | Tool::Move(_)
            | Tool::OffsetFace(_)
            | Tool::Scale(_) => {
                let awaiting = if let Tool::Await { kind, .. } = self.tool { Some(kind) } else { None };
                // In a sketch, a key point under the pointer is picked itself: a plain click replaces the selection,
                // ⇧ adds (s15-011).
                if matches!(self.tool, Tool::Idle)
                    && let Some(pt) = self.sketch_point_at(c)
                {
                    let _ = self.run("select", json!({ "picks": [pt], "add": shift }));
                    return;
                }
                // Outside a sketch, a shown sketch's key point opens that sketch with the point picked.
                if matches!(self.tool, Tool::Idle | Tool::Extrude(_) | Tool::OffsetFace(_) | Tool::Fillet(_) | Tool::Move(_))
                    && (matches!(self.tool, Tool::Idle) || self.predicted)
                    && let Some(pt @ Pick::SketchPoint { sketch, .. }) = self.shown_sketch_point_at(c)
                {
                    self.tool = Tool::Idle;
                    self.predicted = false;
                    self.preview = None;
                    if self.open_sketch_in_place(sketch) {
                        let _ = self.run("select", json!({ "picks": [pt] }));
                    }
                    return;
                }
                let hit = self.pick_at(c);
                self.last_hit = hit.map(|(_, at)| at);
                // A body that is one face all round (a sphere) is picked whole by a plain click (26.202, s40-022).
                let whole = awaiting.is_none() && (matches!(self.tool, Tool::Idle) || self.predicted);
                let hit_pick = hit.map(|(p, _)| match p {
                    Pick::Face { body, .. } if whole && self.one_face_body(body) => Pick::Body { body },
                    p => p,
                });
                match hit_pick {
                    Some(p) => {
                        // With "Require holding shift" off in Preferences, every click adds to the selection; a waiting
                        // tool gathers what's clicked.
                        let extend = shift || !self.settings.extend_needs_shift;
                        let add = extend || awaiting.is_some();
                        // Extending onto something already picked takes it back out.
                        let _ = self.run("select", json!({ "picks": [p], "add": add, "toggle": extend }));
                    }
                    None => self.session.selection.clear(),
                }
                // Selecting something else while a tool runs moves the tool to it, if it applies (a new step).
                self.live_step = None;
                // A tool the selection only predicted is predicted again from the new selection, as a first click
                // would: a face and an edge together predict nothing, so no stale Offset Face arrow stays up.
                let repredict = self.predicted && awaiting.is_none();
                if repredict {
                    self.predicted = false;
                }
                self.tool = match &self.tool {
                    _ if repredict => Tool::Idle,
                    Tool::Extrude(_) => crate::workspace::tools::ExtrudeTool::from_selection(&self.session, self.last_hit)
                        .map(Tool::Extrude)
                        .unwrap_or(Tool::Idle),
                    Tool::Fillet(f) => {
                        let chamfer = f.chamfer;
                        let d2 = f.two.as_ref().map(|t| t.d2);
                        let g2 = f.g2;
                        crate::workspace::tools::FilletTool::from_selection(&self.session, self.last_hit)
                            .map(|mut n| {
                                n.chamfer = chamfer;
                                n.two = d2.and_then(|d2| n.two_distance(d2));
                                n.g2 = g2;
                                Tool::Fillet(n)
                            })
                            .unwrap_or(Tool::Idle)
                    }
                    Tool::Shell(_) => {
                        crate::workspace::tools::ShellTool::from_selection(&self.session).map(Tool::Shell).unwrap_or(Tool::Idle)
                    }
                    Tool::Move(_) => crate::workspace::tools::MoveTool::from_selection(&self.session).map(Tool::Move).unwrap_or(Tool::Idle),
                    Tool::OffsetFace(_) => crate::workspace::tools::OffsetFaceTool::from_selection(&self.session, self.last_hit)
                        .map(Tool::OffsetFace)
                        .unwrap_or(Tool::Idle),
                    Tool::Scale(t) => {
                        crate::workspace::tools::ScaleTool::from_selection(&self.session, t.uniform).map(Tool::Scale).unwrap_or(Tool::Idle)
                    }
                    _ => Tool::Idle,
                };
                if let Some(kind) = awaiting {
                    // Starts once the selection fits; otherwise it keeps waiting.
                    self.start_tool(kind);
                } else if matches!(self.tool, Tool::Idle) {
                    self.predict_tool();
                }
            }
        }
    }
}

/// The elements a line moved by `by` changes: the line, and the ends of other lines that met its ends.
fn moved_side(base: &[SketchEntity], line: u32, by: DVec2) -> Vec<serde_json::Value> {
    let Some(SketchEntity::Line { a, b }) = base.get(line as usize) else { return Vec::new() };
    let (a, b) = (*a, *b);
    let near = |p: DVec2, q: DVec2| p.distance(q) < 1e-7 * (1.0 + q.length());
    let shift = |p: DVec2| if near(p, a) || near(p, b) { p + by } else { p };
    base.iter()
        .enumerate()
        .filter_map(|(k, e)| match e {
            SketchEntity::Line { a: p, b: q } if k as u32 == line || near(*p, a) || near(*p, b) || near(*q, a) || near(*q, b) => {
                Some(json!([k, SketchEntity::Line { a: shift(*p), b: shift(*q) }]))
            }
            _ => None,
        })
        .collect()
}

//! The other tools' drag handles: the running tool's arrow, Extrude's draft, Section View, patterns, Plane
//! Through Edge and Offset Edge on a body edge.

use egui::{Pos2, Ui, vec2};
use glam::DVec3;

use super::snap::{seg_dist, to_d};
use crate::workspace::Workspace;
use crate::workspace::tools::{Handle, Tool};

/// How far along Section View's arrow its flip button sits, in points.
pub(super) const SECTION_FLIP: f32 = 112.0;

impl Workspace {
    /// Screen position and direction (unit, in points) of a tool's arrow handle, and points per mm.
    pub(super) fn handle_frame(&self, base: DVec3, dir: DVec3) -> Option<(Pos2, egui::Vec2, f64)> {
        let s0 = self.to_screen(base)?;
        let k = self.camera.world_per_pixel(self.size().y) * 60.0;
        let s1 = self.to_screen(base + dir * k)?;
        let v = s1 - s0;
        let len = v.length();
        if len < 1e-3 {
            return None;
        }
        Some((s0, v / len, len as f64 / k))
    }

    /// Plane Through Edge: the handle's spot and the frame it turns in (edge middle, edge direction, the way the
    /// plane reaches out from the edge). Taken from the plane the live step made.
    pub(super) fn edge_plane_frame(&self, t: &crate::workspace::tools::EdgePlaneTool) -> Option<(DVec3, DVec3, DVec3)> {
        // The previewed plane while dragging, so the handle follows it.
        let doc = self.shown_doc();
        let pl = doc.planes.last()?;
        t.through.then_some((pl.plane.origin, pl.plane.x, pl.plane.y))
    }

    pub(super) fn edge_plane_handle(&self, t: &crate::workspace::tools::EdgePlaneTool) -> Option<Pos2> {
        if t.through {
            let (mid, _, out) = self.edge_plane_frame(t)?;
            self.to_screen(mid + out * self.camera.world_per_pixel(self.size().y) * 46.0)
        } else {
            self.to_screen(t.point_at(&self.session, t.value)?)
        }
    }

    fn edge_plane_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::EdgePlane(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
            && let Some(h) = self.edge_plane_handle(t)
            && (p - h).length() < 14.0
            && let Tool::EdgePlane(t) = &mut self.tool
        {
            t.dragging = Some((p, t.value));
            self.numeric = None;
        }
        let Tool::EdgePlane(t) = &self.tool else { return };
        let (Some((p0, v0)), Some(p)) = (t.dragging, pointer) else { return };
        let v = if t.through {
            let Some((mid, x, _)) = self.edge_plane_frame(t) else { return };
            let (Some(q0), Some(q1)) = (self.on_plane(p0, mid, x), self.on_plane(p, mid, x)) else { return };
            let (a, b) = (q0 - mid, q1 - mid);
            (v0 + x.dot(a.cross(b)).atan2(a.dot(b)).to_degrees()).round()
        } else {
            // Slide along the edge: the nearest point to the pointer on screen.
            let pts = t.line(&self.session);
            let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>().max(1e-9);
            let mut best: Option<(f64, f64)> = None;
            let mut run = 0.0;
            for w in pts.windows(2) {
                let l = w[0].distance(w[1]);
                if let (Some(a), Some(b)) = (self.to_screen(w[0]), self.to_screen(w[1])) {
                    let (d, u) = seg_dist(to_d(p), to_d(a), to_d(b));
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, (run + u * l) / total));
                    }
                }
                run += l;
            }
            let Some((_, at)) = best else { return };
            (at * 100.0).round() / 100.0
        };
        if let Tool::EdgePlane(t) = &mut self.tool {
            t.value = v;
        }
        if resp.drag_stopped() {
            self.commit_tool();
        }
    }

    /// Offset Edge on a body edge: where each face's arrow sits on screen (its middle and the way it points).
    pub(super) fn body_edge_arrows(&self, t: &crate::workspace::tools::BodyEdgeOffsetTool) -> Vec<(Pos2, egui::Vec2)> {
        let base = t.anchor + t.dir() * t.distance;
        t.faces
            .iter()
            .enumerate()
            .filter_map(|(i, (_, d, _))| {
                let from = if i == t.active { base } else { t.anchor };
                let (s0, sdir, _) = self.handle_frame(from, *d)?;
                Some((s0 + sdir * 18.0, sdir))
            })
            .collect()
    }

    fn body_edge_offset_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::BodyEdgeOffset(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
            && let Some(i) = self.body_edge_arrows(t).iter().position(|(c, _)| (p - *c).length() < 14.0)
            && let Tool::BodyEdgeOffset(t) = &mut self.tool
        {
            if t.active != i {
                t.active = i;
                t.distance = 0.0;
            }
            t.dragging = Some((p, t.distance));
            self.numeric = None;
        }
        let Tool::BodyEdgeOffset(t) = &self.tool else { return };
        let (Some((p0, v0)), Some(p)) = (t.dragging, pointer) else { return };
        let Some((_, sdir, px_per_mm)) = self.handle_frame(t.anchor, t.dir()) else { return };
        let v = ((v0 + (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9)) * 10.0).round() / 10.0;
        if let Tool::BodyEdgeOffset(t) = &mut self.tool {
            t.distance = v;
            if resp.drag_stopped() {
                t.dragging = None;
            }
        }
    }

    fn pattern_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::Pattern(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let (pivot, frame) = (t.pivot, t.frame);
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
            && let Some(Handle::Arrow(a)) = self.gizmo_hit(pivot, frame, p)
            && let Tool::Pattern(t) = &mut self.tool
        {
            self.numeric = None;
            if t.axis != a {
                t.spacing = 0.0;
            }
            t.axis = a;
            t.dragging = Some((p, t.spacing));
        }
        let Tool::Pattern(t) = &self.tool else { return };
        let (Some((p0, v0)), Some(p)) = (t.dragging, pointer) else { return };
        let axis = t.axis;
        let sign = self.toward_viewer(frame.axes[axis]);
        let Some((_, sdir, px_per_mm)) = self.handle_frame(pivot, frame.axes[axis] * sign) else { return };
        let v = ((v0 + (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9) * sign) * 10.0).round() / 10.0;
        if let Tool::Pattern(t) = &mut self.tool {
            t.spacing = v;
            if resp.drag_stopped() {
                t.dragging = None;
            }
        }
    }

    /// Dragging the Section View arrow slides the cut.
    fn section_drags(&mut self, ui: &Ui, resp: &egui::Response) -> bool {
        let Some(sec) = self.section.filter(|s| s.editing) else { return false };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let Some((s0, sdir, px_per_mm)) = self.handle_frame(sec.origin(), sec.normal) else { return false };
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
            && seg_dist(to_d(p), to_d(s0), to_d(s0 + sdir * 46.0)).0 < 14.0
            && let Some(s) = &mut self.section
        {
            s.dragging = Some((p, s.offset));
        }
        // The rotate handles (s16-021): ~90 px out along the plane, each tilting the cut about the other's line.
        let reach = 90.0 / px_per_mm.max(1e-9);
        let (u, v) = sec.axes();
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
            && let Some(k) = [u, v].iter().position(|a| self.to_screen(sec.origin() + *a * reach).is_some_and(|h| h.distance(p) < 13.0))
            && let Some(s) = &mut self.section
        {
            s.center = s.origin();
            s.offset = 0.0;
            s.turning = Some((k, p, s.normal));
        }
        if let Some((k, p0, n0)) = sec.turning
            && let Some(p) = pointer
        {
            let (along, about) = if k == 0 { (u, v) } else { (v, u) };
            let (Some(h), Some(hn)) =
                (self.to_screen(sec.origin() + along * reach), self.to_screen(sec.origin() + along * reach + n0 * reach))
            else {
                return true;
            };
            let dir = (hn - h).normalized();
            let angle = ((p - p0).dot(dir) as f64 / 90.0).atan();
            if let Some(s) = &mut self.section {
                s.normal = (glam::DQuat::from_axis_angle(about.normalize(), -angle * if k == 0 { 1.0 } else { -1.0 }) * n0).normalize();
                if resp.drag_stopped() {
                    s.turning = None;
                }
            }
            return true;
        }
        let Some(s) = &mut self.section else { return false };
        let (Some((p0, v0)), Some(p)) = (s.dragging, pointer) else { return false };
        // The arrow points at the side cut away (s16-021), and the cut follows the pointer along it.
        s.offset = v0 + (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9);
        if resp.drag_stopped() {
            s.dragging = None;
        }
        true
    }

    pub(super) fn handle_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        if self.visualizing && self.decal_drags(ui, resp) {
            return;
        }
        if matches!(self.tool, Tool::Idle) && self.section_drags(ui, resp) {
            return;
        }
        if self.side_drags(ui, resp) {
            return;
        }
        if self.sketch_drags(ui, resp) {
            return;
        }
        if matches!(self.tool, Tool::Move(_)) {
            self.move_drags(ui, resp);
            return;
        }
        if matches!(self.tool, Tool::Pattern(_)) {
            self.pattern_drags(ui, resp);
            return;
        }
        if matches!(self.tool, Tool::Scale(_)) {
            self.scale_drags(ui, resp);
            return;
        }
        if matches!(&self.tool, Tool::RotateAxis(t) if t.axis.is_some()) {
            self.rotate_axis_drags(ui, resp);
            return;
        }
        if matches!(self.tool, Tool::EdgePlane(_)) {
            self.edge_plane_drags(ui, resp);
            return;
        }
        if matches!(self.tool, Tool::BodyEdgeOffset(_)) {
            self.body_edge_offset_drags(ui, resp);
            return;
        }
        let pressed = resp.drag_started_by(egui::PointerButton::Primary);
        let released = resp.drag_stopped();
        // A grab is tested where the button went down (the pointer has moved a little by the time a drag starts).
        let pointer = ui.input(|i| if pressed { i.pointer.press_origin().or(i.pointer.interact_pos()) } else { i.pointer.interact_pos() });
        if self.draft_drags(pressed, released, pointer)
            || self.chamfer2_drags(pressed, released, pointer)
            || self.fillet_slider_drags(pressed, released, pointer)
        {
            return;
        }
        let Some((base, dir, value, per_mm)) = self.arrow_handle() else { return };
        let Some((s0, sdir, px_per_mm)) = self.handle_frame(base, dir) else { return };
        if pressed && let Some(p) = pointer {
            // The fillet's ↔ sits just outside its point (it points into the part); the others reach forward.
            let (from, tip) = match self.tool {
                Tool::Fillet(_) => (s0 - sdir * 40.0, s0),
                // Revolve's ↕ sits on the profile, centred on it.
                Tool::Revolve(_) => (s0 - sdir * 16.0, s0 + sdir * 16.0),
                _ => (s0, s0 + sdir * 46.0),
            };
            let (d, _) = seg_dist(to_d(p), to_d(from), to_d(tip));
            if d < 14.0 {
                self.numeric = None;
                match &mut self.tool {
                    Tool::Extrude(t) => t.dragging = Some((p, value)),
                    Tool::Fillet(t) => t.dragging = Some((p, value)),
                    Tool::Shell(t) => t.dragging = Some((p, value)),
                    Tool::OffsetFace(t) => t.dragging = Some((p, value)),
                    Tool::OffsetEdge(t) => t.dragging = Some((p, value)),
                    Tool::OffsetPlane(t) => t.dragging = Some((p, value)),
                    Tool::Revolve(t) => t.dragging = Some((p, value)),
                    _ => {}
                }
            }
        }
        let start = match &self.tool {
            Tool::Extrude(t) => t.dragging,
            Tool::Fillet(t) => t.dragging,
            Tool::Shell(t) => t.dragging,
            Tool::OffsetFace(t) => t.dragging,
            Tool::OffsetEdge(t) => t.dragging,
            Tool::OffsetPlane(t) => t.dragging,
            Tool::Revolve(t) => t.dragging,
            _ => None,
        };
        if let (Some((p0, v0)), Some(p)) = (start, pointer) {
            let along = (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9) * per_mm;
            let onto = self.extrude_onto(p, s0, sdir);
            let onto_offset = match &self.tool {
                Tool::OffsetFace(t) if t.fillet.is_none() && t.faces.len() == 1 => {
                    self.onto_face(p, s0, sdir, t.anchor, t.dir, Some((t.body, t.faces[0]))).map(|(.., d)| d)
                }
                _ => None,
            };
            // Off the axis over nothing, letting go cancels the pull (back to where it began).
            let lost = Self::off_axis(p, s0, sdir) && onto.is_none() && onto_offset.is_none();
            let snap = |v: f64| {
                // Snap to 0.1 mm while dragging, which is what typed values usually are.
                (v * 10.0).round() / 10.0
            };
            match &mut self.tool {
                // 26.200: pulled off its axis onto a flat face, the arrow reaches to it (To Object); back on the axis,
                // the distance takes over again.
                Tool::Extrude(t) => match onto {
                    Some((target, d)) => {
                        t.to = Some(target);
                        t.distance = d;
                    }
                    // Off the axis over nothing it waits where it began (letting go there cancels).
                    None => {
                        t.to = None;
                        t.distance = if lost { v0 } else { snap(v0 + along) };
                    }
                },
                // One arrow for both, as observed: pulled into the part it rounds, pushed out it bevels.
                Tool::Fillet(t) if t.two.is_some() => t.radius = snap(v0 + along).max(0.0),
                Tool::Fillet(t) => {
                    let v = snap(v0 + along);
                    t.chamfer = v < 0.0;
                    t.radius = v.abs();
                }
                Tool::Shell(t) => t.thickness = snap((v0 + along).max(0.0)),
                // Pulled off its axis onto a flat face, the face's offset reaches it; back on the axis, the drag rules.
                Tool::OffsetFace(t) => t.distance = onto_offset.unwrap_or_else(|| snap(v0 + along)),
                Tool::OffsetEdge(t) => t.distance = snap(v0 + along),
                Tool::OffsetPlane(t) => t.distance = snap(v0 + along),
                // Whole degrees, within a turn either way.
                Tool::Revolve(t) => {
                    let a = (v0 + along).round().clamp(-360.0, 360.0);
                    t.angle = if a == 0.0 { if along < 0.0 { -1.0 } else { 1.0 } } else { a };
                }
                _ => {}
            }
            if released {
                match self.tool.clone() {
                    Tool::Extrude(mut t) => {
                        t.dragging = None;
                        if lost && matches!(self.tool, Tool::Extrude(_)) {
                            t.distance = v0;
                            t.to = None;
                            self.tool = Tool::Extrude(t);
                        } else {
                            self.tool = Tool::Extrude(t.clone());
                            self.apply_extrude(&t);
                        }
                    }
                    Tool::Fillet(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::Fillet(t.clone());
                        self.apply_fillet(&t);
                    }
                    Tool::Shell(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::Shell(t);
                    }
                    // Applied on release, as observed (364, 368); the handle stays to pull the same step again.
                    Tool::OffsetFace(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::OffsetFace(t);
                        self.commit_tool();
                    }
                    Tool::OffsetEdge(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::OffsetEdge(t);
                    }
                    Tool::OffsetPlane(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::OffsetPlane(t);
                    }
                    Tool::Revolve(mut t) => {
                        t.dragging = None;
                        self.tool = Tool::Revolve(t);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The fillet's profile slider (s29-016/017, manual "Profile slider"): the dashed dimension from the edge to the
    /// arrow carries a knob, in the middle at curvature 0, toward the edge sharper, toward the arrow flatter. Returns
    /// the edge's and the arrow's screen points and the knob.
    pub(super) fn fillet_slider(&self) -> Option<(Pos2, Pos2, Pos2)> {
        let Tool::Fillet(t) = &self.tool else { return None };
        if t.chamfer || t.radius.abs() < 1e-9 || t.chordal || t.setback {
            return None;
        }
        let (arrow, ..) = self.arrow_handle()?;
        let (s0, e) = (self.to_screen(arrow)?, self.to_screen(t.anchor)?);
        ((e - s0).length() > 30.0).then_some(())?;
        Some((e, s0, e + (s0 - e) * (0.5 - 0.4 * t.curvature as f32)))
    }

    /// Dragging the profile slider's knob sets the curvature; released, the fillet is applied with it.
    fn fillet_slider_drags(&mut self, pressed: bool, released: bool, pointer: Option<Pos2>) -> bool {
        let Some((e, s0, knob)) = self.fillet_slider() else { return false };
        if pressed
            && let Some(p) = pointer
            && p.distance(knob) < 9.0
            && let Tool::Fillet(t) = &mut self.tool
        {
            t.slider = Some((p, t.curvature));
            self.numeric = None;
        }
        let Tool::Fillet(t) = &mut self.tool else { return false };
        let Some((p0, c0)) = t.slider else { return false };
        if let Some(p) = pointer {
            let span = (s0 - e).length().max(1.0);
            let along = (p - p0).dot((s0 - e) / span) / span;
            t.curvature = ((c0 - along as f64 / 0.4) * 100.0).round().clamp(-100.0, 100.0) / 100.0;
        }
        if released {
            t.slider = None;
            let t = t.clone();
            self.preview = None;
            self.apply_fillet(&t);
        }
        true
    }

    /// Two-Distance chamfer's second arrow: where it sits on screen and the screen direction it pulls along.
    pub(super) fn chamfer2_handle(&self) -> Option<(Pos2, egui::Vec2, f64)> {
        let Tool::Fillet(t) = &self.tool else { return None };
        let two = t.two.as_ref()?;
        let (s0, sdir, px_per_mm) = self.handle_frame(t.anchor + two.dirs[1] * two.d2, two.dirs[1])?;
        Some((s0, sdir, px_per_mm))
    }

    /// Pulling Two-Distance's second arrow sets the other face's distance; released, the chamfer is applied.
    fn chamfer2_drags(&mut self, pressed: bool, released: bool, pointer: Option<Pos2>) -> bool {
        let Some((s0, sdir, px_per_mm)) = self.chamfer2_handle() else { return false };
        if pressed
            && let Some(p) = pointer
            && seg_dist(to_d(p), to_d(s0 - sdir * 40.0), to_d(s0)).0 < 14.0
            && let Tool::Fillet(t) = &mut self.tool
            && let Some(two) = &mut t.two
        {
            two.dragging = Some((p, two.d2));
            self.numeric = None;
        }
        let Tool::Fillet(t) = &mut self.tool else { return false };
        let Some(two) = &mut t.two else { return false };
        let Some((p0, v0)) = two.dragging else { return false };
        if let Some(p) = pointer {
            two.d2 = (((v0 + (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9)) * 10.0).round() / 10.0).max(0.0);
        }
        if released {
            two.dragging = None;
            let t = t.clone();
            self.apply_fillet(&t);
        }
        true
    }

    /// Where Extrude's ↔ draft handle sits on screen, and the screen direction it slides along.
    /// A body face pushed out has none: just the badges, the value and the ↕.
    pub(super) fn draft_handle(&self) -> Option<(Pos2, egui::Vec2)> {
        let Tool::Extrude(t) = &self.tool else { return None };
        if matches!(t.target, crate::workspace::tools::Target::Face { .. }) && t.draft == 0.0 {
            return None;
        }
        let (s0, sdir, _) = self.handle_frame(t.origin + t.normal * t.distance, t.normal)?;
        Some((s0 + sdir * 24.0, vec2(-sdir.y, sdir.x)))
    }

    /// Dragging the ↔ handle leans Extrude's walls (a quarter degree per point, in half-degree steps).
    fn draft_drags(&mut self, pressed: bool, released: bool, pointer: Option<Pos2>) -> bool {
        let Some((at, along)) = self.draft_handle() else { return false };
        // The ↔ sits just past the ↕ (as observed): it takes the press only when that's nearer to it.
        let arrow = at - vec2(along.y, -along.x) * 24.0;
        if pressed
            && let Some(p) = pointer
            && (p - at).length() < 12.0
            && (p - at).length() < (p - arrow).length()
            && let Tool::Extrude(t) = &mut self.tool
        {
            t.drafting = Some((p, t.draft));
            self.numeric = None;
        }
        let Tool::Extrude(t) = &mut self.tool else { return false };
        let Some((p0, a0)) = t.drafting else { return false };
        if let Some(p) = pointer {
            let a = a0 + (p - p0).dot(along) as f64 * 0.25;
            t.draft = ((a * 2.0).round() / 2.0).clamp(-80.0, 80.0);
        }
        if released {
            t.drafting = None;
        }
        true
    }

    /// The running tool's arrow: where it sits, which way it points, its value, and how much one mm of drag adds.
    pub(super) fn arrow_handle(&self) -> Option<(DVec3, DVec3, f64, f64)> {
        Some(match &self.tool {
            Tool::Extrude(t) => (t.origin + t.normal * t.distance, t.normal, t.distance, 1.0),
            // Two-Distance: the first arrow lies on its face.
            Tool::Fillet(t) if let Some(two) = &t.two => (t.anchor + two.dirs[0] * t.radius, two.dirs[0], t.radius, 1.0),
            Tool::Fillet(t) => {
                // A chamfer is the arrow pushed the other way: a negative value along it.
                let v = if t.chamfer { -t.radius } else { t.radius };
                (t.anchor + t.dir * v, t.dir, v, 1.0)
            }
            Tool::Shell(t) => (t.anchor + t.dir * t.thickness, t.dir, t.thickness, 1.0),
            Tool::OffsetFace(t) => (t.anchor + t.dir * t.distance, t.dir, t.distance, 1.0),
            Tool::OffsetEdge(t) => (t.anchor + t.dir * t.distance, t.dir, t.distance, 1.0),
            Tool::OffsetPlane(t) => (t.anchor + t.dir * t.distance, t.dir, t.distance, 1.0),
            // The ↕ on the profile slides along the circle it travels: degrees per mm of that circle.
            Tool::Revolve(t) => {
                let (c, foot, axis) = self.revolve_frame(t)?;
                let r = c.distance(foot);
                (r > 1e-9).then_some(())?;
                (c, axis.cross((c - foot) / r), t.angle, 180.0 / (std::f64::consts::PI * r))
            }
            _ => return None,
        })
    }

    /// Extrude's arrow dragged well off its axis over a flat face (not its own, not side-on): that face as a To Object
    /// target, and how far along the profile's normal it is.
    fn extrude_onto(&self, p: Pos2, s0: Pos2, sdir: egui::Vec2) -> Option<(serde_json::Value, f64)> {
        let Tool::Extrude(t) = &self.tool else { return None };
        let own = match t.target {
            crate::workspace::tools::Target::Face { body, face } => Some((body, face)),
            _ => None,
        };
        let (body, face, d) = self.onto_face(p, s0, sdir, t.origin, t.normal, own)?;
        Some((serde_json::json!({ "body": body.0, "face": face }), d))
    }

    /// A handle pulled well off its axis (40 points) over another flat face that isn't side-on to it: that face and
    /// how far it is from `origin` along `normal` (the To Object reach of Extrude and Offset Face).
    fn onto_face(
        &self,
        p: Pos2,
        s0: Pos2,
        sdir: egui::Vec2,
        origin: DVec3,
        normal: DVec3,
        own: Option<(shapecraft_doc::ItemId, u32)>,
    ) -> Option<(shapecraft_doc::ItemId, u32, f64)> {
        if !Self::off_axis(p, s0, sdir) {
            return None;
        }
        let Some((shapecraft_doc::Pick::Face { body, face }, _)) = self.pick_at(p) else { return None };
        if own == Some((body, face)) {
            return None;
        }
        let fp = self.session.kernel.face_plane(&self.session.doc.body(body)?.shape, face).ok()??;
        if fp.normal.dot(normal).abs() < 0.1 {
            return None;
        }
        let d = (fp.origin - origin).dot(normal);
        (d.abs() > 1e-6).then_some((body, face, d))
    }

    /// The pointer is well off the handle's axis on screen.
    fn off_axis(p: Pos2, s0: Pos2, sdir: egui::Vec2) -> bool {
        let off = p - s0;
        (off.x * sdir.y - off.y * sdir.x).abs() >= 40.0
    }

    /// Revolve's handle (the profile's middle), the nearest point of the axis to it, and the axis direction.
    pub(super) fn revolve_frame(&self, t: &crate::workspace::tools::RevolveTool) -> Option<(DVec3, DVec3, DVec3)> {
        let c = crate::workspace::tools::ExtrudeTool::profile_middle(&self.session, t.profile)?;
        let (o, d) = t.axis_line(&self.session)?;
        Some((c, o + d * (c - o).dot(d), d))
    }
}

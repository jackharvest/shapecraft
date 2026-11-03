//! The transform gizmos: Move's arrows, arcs and plane tabs, Scale's hammers, and Rotate Around Axis, with
//! their hit testing and drags.

use egui::{Pos2, Ui};
use glam::DVec3;
use shapecraft_doc::Pick;

use super::snap::{seg_dist, to_d};
use crate::workspace::Workspace;
use crate::workspace::tools::{AXES, GizmoFrame, Handle, Tool};

impl Workspace {
    /// Screen polylines of the gizmo's handles, for drawing and hit testing. Proportions follow the
    /// reference: short arrows out from the middle, little rotation arcs between them, and a square tab per
    /// plane near the middle (a closed polyline).
    pub(super) fn gizmo_shapes(&self, pivot: DVec3, frame: GizmoFrame) -> Vec<(Handle, Vec<Pos2>)> {
        let wpp = self.camera.world_per_pixel(self.size().y);
        let axes = frame.axes.map(|ax| ax * self.toward_viewer(ax));
        let mut out = Vec::new();
        for (a, ax) in axes.iter().enumerate().filter(|(a, _)| frame.has(Handle::Arrow(*a))) {
            if let (Some(p0), Some(p1)) = (self.to_screen(pivot + *ax * wpp * 24.0), self.to_screen(pivot + *ax * wpp * 54.0)) {
                out.push((Handle::Arrow(a), vec![p0, p1]));
            }
        }
        for a in (0..3).filter(|a| frame.has(Handle::Ring(*a))) {
            let (u, v) = (axes[(a + 1) % 3], axes[(a + 2) % 3]);
            let r = wpp * 58.0;
            let pts: Vec<Pos2> = (0..=6)
                .filter_map(|i| {
                    let t = (0.38 + 0.24 * i as f64 / 6.0) * std::f64::consts::FRAC_PI_2;
                    self.to_screen(pivot + (u * t.cos() + v * t.sin()) * r)
                })
                .collect();
            out.push((Handle::Ring(a), pts));
        }
        for a in (0..3).filter(|a| frame.has(Handle::Plane(*a))) {
            let (u, v) = (axes[(a + 1) % 3], axes[(a + 2) % 3]);
            let c = pivot + (u + v) * wpp * 19.0;
            let h = wpp * 7.5;
            let pts: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)]
                .iter()
                .filter_map(|(x, y)| self.to_screen(c + u * h * x + v * h * y))
                .collect();
            if pts.len() == 5 {
                out.push((Handle::Plane(a), pts));
            }
        }
        out
    }

    /// +1 or −1: which way along `ax` faces the camera (the reference's gizmo arrows always face you).
    pub(super) fn toward_viewer(&self, ax: DVec3) -> f64 {
        let d = ax.dot(self.camera.back());
        if d.abs() > 1e-3 {
            return d.signum();
        }
        // Lying in the screen (a face-on sketch): right or up, as the reference draws them (s11-088).
        let (right, up) = self.camera.basis();
        let (r, u) = (ax.dot(right), ax.dot(up));
        if if r.abs() >= u.abs() { r } else { u } >= 0.0 { 1.0 } else { -1.0 }
    }

    /// Where the pointer's ray meets the plane through `pivot` normal to `n`.
    pub(super) fn on_plane(&self, p: Pos2, pivot: DVec3, n: DVec3) -> Option<DVec3> {
        let (o, d) = self.camera.ray(self.local(p), self.size());
        let den = d.dot(n);
        (den.abs() > 1e-9).then(|| o + d * ((pivot - o).dot(n) / den))
    }

    pub(super) fn gizmo_hit(&self, pivot: DVec3, frame: GizmoFrame, p: Pos2) -> Option<Handle> {
        let mut best: Option<(f64, Handle)> = None;
        for (h, pts) in self.gizmo_shapes(pivot, frame) {
            for w in pts.windows(2) {
                let (d, _) = seg_dist(to_d(p), to_d(w[0]), to_d(w[1]));
                if d < 10.0 && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, h));
                }
            }
        }
        best.map(|(_, h)| h)
    }

    pub(super) fn move_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::Move(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let (shown, frame) = (t.shown_pivot(), t.frame);
        let live = self.is_live();
        if self.relocate_gizmo(ui, resp, shown, live) {
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = pointer
            && let Some(h) = self.gizmo_hit(shown, frame, p)
        {
            let mut settled = false;
            if let Tool::Move(t) = &mut self.tool {
                // The same handle carries on with the live step; another one starts a new step from here.
                if t.handle != h || !live {
                    if live {
                        t.settle();
                        settled = true;
                    } else {
                        t.value = 0.0;
                        t.planar = DVec3::ZERO;
                    }
                }
                t.handle = h;
                t.planar_start = t.planar;
                t.dragging = Some((h, p, t.value));
            }
            if settled {
                self.live_step = None;
                // Copies are selected after a copying move; the next step moves those.
                let copies: Vec<_> = self.session.selection.iter().filter_map(Pick::body).collect();
                if let Tool::Move(t) = &mut self.tool
                    && t.copy
                    && !copies.is_empty()
                {
                    t.items = copies;
                }
            }
            self.numeric = None;
        }
        let Tool::Move(t) = &self.tool else { return };
        let (pivot, planar_start) = (t.pivot, t.planar_start);
        let (Some((h, p0, v0)), Some(p)) = (t.dragging, pointer) else { return };
        let value = match h {
            Handle::Arrow(a) => {
                let sign = self.toward_viewer(frame.axes[a]);
                let Some((_, sdir, px_per_mm)) = self.handle_frame(pivot, frame.axes[a] * sign) else { return };
                let along = (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9) * sign;
                ((v0 + along) * 10.0).round() / 10.0
            }
            Handle::Plane(a) => {
                let n = frame.axes[a];
                let (Some(h0), Some(h1)) = (self.on_plane(p0, pivot, n), self.on_plane(p, pivot, n)) else { return };
                let d = planar_start + ((h1 - h0) * 10.0).round() / 10.0;
                if let Tool::Move(t) = &mut self.tool {
                    t.planar = d;
                }
                d.length()
            }
            Handle::Ring(a) => {
                let Some(c) = self.to_screen(pivot) else { return };
                let ang = |q: Pos2| ((q.y - c.y) as f64).atan2((q.x - c.x) as f64);
                let mut d = ang(p) - ang(p0);
                if d > std::f64::consts::PI {
                    d -= std::f64::consts::TAU;
                } else if d < -std::f64::consts::PI {
                    d += std::f64::consts::TAU;
                }
                // Screen y points down, so a turn that looks counter-clockwise is negative here.
                let facing = if frame.axes[a].dot(self.camera.back()) >= 0.0 { -1.0 } else { 1.0 };
                (v0 + d.to_degrees() * facing).round()
            }
        };
        if let Tool::Move(t) = &mut self.tool {
            t.value = value;
        }
        if resp.drag_stopped() {
            if let Tool::Move(t) = &mut self.tool {
                t.dragging = None;
            }
            self.commit_tool();
        }
    }

    /// Dragging the gizmo's center slides it over the model (snapping to face centers); the items stay put.
    fn relocate_gizmo(&mut self, ui: &Ui, resp: &egui::Response, shown: DVec3, live: bool) -> bool {
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let (Some(p), Some(c)) = (ui.input(|i| i.pointer.press_origin()), self.to_screen(shown))
            && (p - c).length() < 9.0
        {
            if let Tool::Move(t) = &mut self.tool {
                if live {
                    t.settle();
                } else {
                    t.value = 0.0;
                    t.planar = DVec3::ZERO;
                }
                t.relocating = true;
            }
            self.live_step = None;
            self.numeric = None;
        }
        if !matches!(&self.tool, Tool::Move(t) if t.relocating) {
            return false;
        }
        // Auto-orientation: over a flat face the gizmo turns to it (its Z along the face's normal).
        let turned = pointer
            .filter(|_| self.auto_orient() && matches!(&self.tool, Tool::Move(t) if !t.items.is_empty()))
            .and_then(|p| self.face_frame_at(p));
        if let Some(p) = pointer
            && let Some((at, snapped)) = self.relocation_target(p)
            && let Tool::Move(t) = &mut self.tool
        {
            t.pivot = at;
            t.snapped = snapped;
            if let Some(f) = turned {
                t.frame = f;
            }
        }
        if resp.drag_stopped()
            && let Tool::Move(t) = &mut self.tool
        {
            t.relocating = false;
        }
        true
    }

    /// The Scale gizmo's handles: hammers where Move has arrows and the same plane tabs (Non-uniform), or one
    /// hammer pointing up (Uniform), as observed.
    pub(in crate::workspace) fn scale_shapes(&self, t: &crate::workspace::tools::ScaleTool) -> Vec<(Handle, Vec<Pos2>)> {
        let all = self.gizmo_shapes(t.pivot, GizmoFrame::WORLD);
        let c = self.to_screen(t.pivot);
        all.into_iter()
            .filter(|(h, _)| match h {
                Handle::Ring(_) => false,
                Handle::Arrow(a) => !t.uniform || *a == 2,
                Handle::Plane(_) => !t.uniform,
            })
            .map(|(h, pts)| match (h, c) {
                // The tabs sit a little further out than Move's (measured, 313).
                (Handle::Plane(_), Some(c)) => {
                    let mid = pts.iter().take(4).fold(egui::Vec2::ZERO, |acc, q| acc + q.to_vec2() / 4.0).to_pos2();
                    let push = (mid - c) * 0.35;
                    (h, pts.into_iter().map(|q| q + push).collect())
                }
                _ => (h, pts),
            })
            .collect()
    }

    pub(super) fn scale_hit(&self, t: &crate::workspace::tools::ScaleTool, p: Pos2) -> Option<Handle> {
        let mut best: Option<(f64, Handle)> = None;
        for (h, pts) in self.scale_shapes(t) {
            for w in pts.windows(2) {
                let (d, _) = seg_dist(to_d(p), to_d(w[0]), to_d(w[1]));
                if d < 10.0 && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, h));
                }
            }
        }
        best.map(|(_, h)| h)
    }

    pub(super) fn scale_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::Scale(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let pivot = t.pivot;
        let live = self.is_live();
        let pressed = resp.drag_started_by(egui::PointerButton::Primary);
        let press = ui.input(|i| i.pointer.press_origin());
        // The center drags elsewhere: the point it scales from (no step of its own, as observed).
        if pressed
            && let (Some(p), Some(c)) = (press, self.to_screen(pivot))
            && (p - c).length() < 9.0
            && let Tool::Scale(t) = &mut self.tool
        {
            t.relocating = true;
            t.factor = 1.0;
            self.live_step = None;
            self.numeric = None;
        }
        if let Tool::Scale(t) = &self.tool
            && t.relocating
        {
            if let Some(p) = pointer
                && let Some((at, snapped)) = self.relocation_target(p)
                && let Tool::Scale(t) = &mut self.tool
            {
                t.pivot = at;
                t.snapped = snapped;
            }
            if resp.drag_stopped()
                && let Tool::Scale(t) = &mut self.tool
            {
                t.relocating = false;
            }
            return;
        }
        if pressed
            && let Some(p) = press
            && let Tool::Scale(t) = &self.tool
            && let Some(h) = self.scale_hit(t, p)
            && let Tool::Scale(t) = &mut self.tool
        {
            // The same handle carries on with the live step; another one starts a new step.
            if t.handle != h || !live {
                if live {
                    self.live_step = None;
                }
                t.factor = 1.0;
            }
            t.handle = h;
            t.dragging = Some((p, t.factor));
            self.numeric = None;
        }
        let Tool::Scale(t) = &self.tool else { return };
        let (Some((p0, f0)), Some(p)) = (t.dragging, pointer) else { return };
        let mm = match t.handle {
            Handle::Plane(a) => {
                let (u, v) = (AXES[(a + 1) % 3], AXES[(a + 2) % 3]);
                let out = (u * self.toward_viewer(u) + v * self.toward_viewer(v)).normalize_or_zero();
                let (Some(h0), Some(h1)) = (self.on_plane(p0, pivot, AXES[a]), self.on_plane(p, pivot, AXES[a])) else { return };
                (h1 - h0).dot(out)
            }
            Handle::Arrow(a) | Handle::Ring(a) => {
                let ax = AXES[a.min(2)] * self.toward_viewer(AXES[a.min(2)]);
                let Some((_, sdir, px_per_mm)) = self.handle_frame(pivot, ax) else { return };
                (p - p0).dot(sdir) as f64 / px_per_mm.max(1e-9)
            }
        };
        // Two decimals for a factor; never down to nothing.
        let f = ((f0 + mm / t.mm_per_unit()) * 100.0).round().max(1.0) / 100.0;
        if let Tool::Scale(t) = &mut self.tool {
            t.factor = f;
            if resp.drag_stopped() {
                t.dragging = None;
                self.commit_tool();
            }
        }
    }

    /// Rotate Around Axis: the handle at the items' middle swings around the axis; the angle is how far it went,
    /// in whole degrees.
    pub(super) fn rotate_axis_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::RotateAxis(t) = &self.tool else { return };
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
            && let Some(h) = t.handle_at(t.angle).and_then(|h| self.to_screen(h))
            && (p - h).length() < 14.0
            && let Tool::RotateAxis(t) = &mut self.tool
        {
            t.dragging = Some((p, t.angle));
            self.numeric = None;
        }
        let Tool::RotateAxis(t) = &self.tool else { return };
        let (Some((p0, a0)), Some(p), Some((_, dir)), Some(f)) = (t.dragging, pointer, t.axis, t.foot()) else { return };
        let (Some(q0), Some(q1)) = (self.on_plane(p0, f, dir), self.on_plane(p, f, dir)) else { return };
        let (v0, v1) = (q0 - f, q1 - f);
        let d = dir.dot(v0.cross(v1)).atan2(v0.dot(v1)).to_degrees();
        let mut a = (a0 + d).round();
        if a > 180.0 {
            a -= 360.0;
        } else if a < -180.0 {
            a += 360.0;
        }
        if let Tool::RotateAxis(t) = &mut self.tool {
            t.angle = a;
        }
        if resp.drag_stopped() {
            self.commit_tool();
        }
    }
}

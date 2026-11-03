//! The camera: animated moves, standard and saved views, rotating, zooming to fit, and facing a sketch plane.

use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{ItemId, SketchPlane};
use shapecraft_render::{Camera, StandardView};

use super::{Anim, Workspace};

const STANDARD_VIEWS: [StandardView; 7] = [
    StandardView::Default,
    StandardView::Front,
    StandardView::Back,
    StandardView::Top,
    StandardView::Bottom,
    StandardView::Right,
    StandardView::Left,
];

impl Workspace {
    pub fn animate_to(&mut self, mut to: Camera) {
        self.settings.apply_view(&mut to);
        if !self.settings.animate_camera {
            self.anim = None;
            self.camera = to;
            return;
        }
        // Measured: the reference's view moves take ~0.33 s (⌘1 timed off the view cube, s30 animtime).
        self.anim = Some(Anim { from: self.camera, to, start: self.now, duration: 0.33 });
    }

    /// The standard view the camera is in right now, if any (the View popover checks it).
    pub fn current_view(&self) -> Option<StandardView> {
        if self.anim.is_some() {
            return None;
        }
        let back = self.camera.back();
        STANDARD_VIEWS.into_iter().find(|v| {
            let c = Camera::standard(*v, DVec3::ZERO, 1.0);
            (c.back() - back).length() < 1e-6
                && (*v == StandardView::Default || self.camera.is_ortho())
                && (c.basis().0 - self.camera.basis().0).length() < 1e-6
        })
    }

    /// View ▸ Saved Views: keeps the camera in slot 1–8 of the project.
    pub fn save_view(&mut self, slot: usize) {
        let c = self.camera;
        let view = json!({ "target": c.target.to_array(), "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance, "ortho": c.ortho_amount(), "roll": c.roll });
        let _ = self.run("save_view", json!({ "slot": slot, "view": view }));
    }

    pub fn go_to_saved_view(&mut self, slot: usize) {
        let Some(Some(v)) = slot.checked_sub(1).and_then(|k| self.session.doc.saved_views.get(k)).copied() else { return };
        let mut to = self.camera;
        to.target = DVec3::from_array(v.target);
        (to.yaw, to.pitch, to.distance, to.ortho, to.roll) = (v.yaw, v.pitch, v.distance, v.ortho, v.roll);
        self.animate_to(to);
    }

    /// Turns to the standard view nearest the current direction (View ▸ Nearest Ortho View).
    pub fn nearest_ortho_view(&mut self) {
        let back = self.camera.back();
        let best = STANDARD_VIEWS[1..].iter().copied().max_by(|a, b| {
            let da = Camera::standard(*a, DVec3::ZERO, 1.0).back().dot(back);
            let db = Camera::standard(*b, DVec3::ZERO, 1.0).back().dot(back);
            da.total_cmp(&db)
        });
        if let Some(v) = best {
            self.standard_view(v);
        }
    }

    pub fn standard_view(&mut self, view: StandardView) {
        let mut target = self.camera.target;
        let mut distance = self.camera.distance;
        if view == StandardView::Default
            && let Some((min, max)) = self.session.doc.bounds()
        {
            let mut c = self.camera;
            c.fit(min, max);
            // Fitted to the model either way, as opening a project and ⌘1 both do in the reference (s30-100/101).
            target = c.target;
            distance = c.distance;
        }
        self.animate_to(Camera::standard(view, target, distance));
    }

    /// Frames one body (or everything), keeping the view direction.
    pub fn zoom_to(&mut self, body: Option<ItemId>) {
        let bounds = match body.and_then(|b| self.session.doc.body(b)) {
            Some(b) => self.session.kernel.bounds(&b.shape).ok().map(|bb| (bb.min, bb.max)),
            None => self.session.doc.bounds(),
        };
        if let Some((min, max)) = bounds {
            let mut c = self.camera;
            c.fit(min, max);
            self.animate_to(c);
        }
    }

    /// The selected bodies' (and their faces' and edges' bodies') box, from their display meshes.
    pub(super) fn selection_bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for pick in &self.session.selection {
            let Some(b) = pick.body().and_then(|id| self.session.doc.body(id)) else { continue };
            for p in &b.mesh.positions {
                let q = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        (lo.x.is_finite() && hi.x.is_finite()).then_some((lo, hi))
    }

    /// Whether a box shows entirely in the free part of the view (between the sidebars).
    pub(super) fn box_in_view(&self, lo: DVec3, hi: DVec3) -> bool {
        let area = self.viewport_rect;
        (0..8).all(|i| {
            let c = DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            self.to_screen(c).is_some_and(|s| area.contains(s))
        })
    }

    /// Zoom to Selection: frames the selection's box.
    pub(super) fn zoom_to_selection(&mut self) {
        if let Some((min, max)) = self.selection_bounds() {
            let mut c = self.camera;
            c.fit(min, max);
            self.animate_to(c);
        }
    }

    pub(super) fn step_animation(&mut self) -> bool {
        let Some(a) = self.anim else { return false };
        let t = ((self.now - a.start) / a.duration).clamp(0.0, 1.0);
        self.camera = Camera::lerp(&a.from, &a.to, t);
        if t >= 1.0 {
            self.anim = None;
        }
        true
    }

    /// Zooms the view turning to a face's sketch so the face fills most of it, as observed (408).
    pub(super) fn frame_face(&mut self, plane: SketchPlane, body: ItemId, face: u32) {
        let Some(b) = self.session.doc.body(body) else { return };
        let m = &b.mesh;
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for (t, f) in m.triangle_faces.iter().enumerate() {
            if *f != face {
                continue;
            }
            for k in 0..3 {
                let Some(p) = m.indices.get(t * 3 + k).and_then(|i| m.positions.get(*i as usize)) else { continue };
                let q = plane.to_local(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        if !(lo.x.is_finite() && hi.x.is_finite()) {
            return;
        }
        let size = hi - lo;
        let vp = self.view_rect;
        let aspect = (vp.width() / vp.height().max(1.0)) as f64;
        let half_h = (size.y / 2.0).max(size.x / 2.0 / aspect.max(1e-3)) * 1.27;
        let mid = (lo + hi) / 2.0;
        let cam = match self.anim.as_mut() {
            Some(a) => &mut a.to,
            None => &mut self.camera,
        };
        cam.target = plane.origin + plane.x * mid.x + plane.y * mid.y;
        cam.distance = (half_h / (cam.fov_y / 2.0).tan()).max(1e-3);
    }

    /// Looks straight at a sketch plane, the way picking a plane does in the reference app.
    pub(super) fn focus_plane(&mut self, plane: SketchPlane) {
        let target = plane.origin + plane.x * plane.to_local(self.camera.target).x + plane.y * plane.to_local(self.camera.target).y;
        let target = if (target - plane.origin).length() > self.camera.distance { plane.origin } else { target };
        self.animate_to(Camera::facing(plane.normal(), plane.y, target, self.camera.distance));
    }

    /// The camera is on its way somewhere (a view change or a plane being faced).
    pub(crate) fn animating(&self) -> bool {
        self.anim.is_some()
    }

    pub(crate) fn focus_plane_public(&mut self, plane: SketchPlane) {
        self.show_history = false;
        self.focus_plane(plane);
    }

    /// View ▸ Rotate View: left, right, up or down (0–3) by 15°, or 45° when `big`.
    pub fn rotate_view(&mut self, dir: u8, big: bool) {
        let step = if big { 45f64 } else { 15f64 }.to_radians();
        let mut to = self.camera;
        match dir {
            0 => to.yaw -= step,
            1 => to.yaw += step,
            2 => to.pitch = (to.pitch + step).min(std::f64::consts::FRAC_PI_2),
            _ => to.pitch = (to.pitch - step).max(-std::f64::consts::FRAC_PI_2),
        }
        to.ortho = 0.0;
        to.roll = 0.0;
        self.animate_to(to);
    }

    /// Where a new project's thumbnail comes from: the default view, framed.
    pub fn thumbnail_camera(&self) -> Camera {
        let mut c = Camera::default();
        if let Some((min, max)) = self.session.doc.bounds() {
            c.fit(min, max);
        }
        Camera::standard(StandardView::Default, c.target, c.distance * 0.9)
    }

    pub fn camera_eye(&self) -> DVec3 {
        self.camera.eye()
    }
}

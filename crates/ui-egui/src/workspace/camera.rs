//! The camera: animated moves, standard and saved views, rotating, zooming to fit, and facing a sketch plane.

use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{ItemId, Pick, SketchPlane};
use shapecraft_render::{Camera, StandardView};

use super::{Anim, Workspace};

/// A selection's box and a key for the picked bodies and meshes it was worked out from.
pub(crate) type KeptBox = Option<(u64, Option<(DVec3, DVec3)>)>;

/// A screen with fewer points per millimetre than this is a mistake (about 25 points per inch).
const MIN_POINTS_PER_MM: f64 = 1.0;
const MAX_POINTS_PER_MM: f64 = 20.0;
/// Where calibration starts: a Mac's points run about 110 to the inch, other systems' 96 (their "100%").
const GUESS_POINTS_PER_MM: f64 = if cfg!(target_os = "macos") { 110.0 / 25.4 } else { 96.0 / 25.4 };
/// The card to match: a bank or ID card (ISO/IEC 7810 ID-1), 85.60 × 53.98 mm, which nearly everyone has at hand.
pub(crate) const CARD_MM: (f64, f64) = (85.60, 53.98);

/// The orthographic camera distance at which a viewport `height` points tall shows `per_mm` points per model mm.
pub(crate) fn actual_size_distance(camera: &Camera, height: f64, per_mm: f64) -> f64 {
    let half = (camera.fov_y / 2.0).tan().max(1e-6);
    // world_per_pixel = 2 · distance · tan(fov/2) / height = 1 / per_mm.
    height.max(1.0) / per_mm / (2.0 * half)
}

/// Keeps a tried calibration within what any screen could be.
pub(crate) fn clamp_points_per_mm(v: f64) -> f64 {
    if v.is_finite() { v.clamp(MIN_POINTS_PER_MM, MAX_POINTS_PER_MM) } else { GUESS_POINTS_PER_MM }
}

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
        let settle = crate::motion::enabled();
        self.anim = Some(Anim {
            from: self.camera,
            to,
            start: self.now,
            duration: if settle { crate::motion::SETTLE_CAMERA_S } else { 0.33 },
            settle,
        });
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

    /// The straight-on view nearest the current direction.
    fn nearest_standard(&self) -> Option<StandardView> {
        let back = self.camera.back();
        STANDARD_VIEWS[1..].iter().copied().max_by(|a, b| {
            let da = Camera::standard(*a, DVec3::ZERO, 1.0).back().dot(back);
            let db = Camera::standard(*b, DVec3::ZERO, 1.0).back().dot(back);
            da.total_cmp(&db)
        })
    }

    /// Turns to the standard view nearest the current direction (View ▸ Nearest Ortho View).
    pub fn nearest_ortho_view(&mut self) {
        if let Some(v) = self.nearest_standard() {
            self.standard_view(v);
        }
    }

    /// View ▸ Actual Size (#62): zooms so 1 mm of the model is 1 mm on the screen, to hold a part or a ruler against.
    /// Only a flat (orthographic) view has one scale, so a perspective view turns straight-on first, as Nearest Ortho
    /// View does. The first time it asks how big this screen's millimetres are.
    pub fn actual_size(&mut self) {
        let per_mm = self.settings.screen_points_per_mm;
        if !(per_mm.is_finite() && per_mm >= MIN_POINTS_PER_MM) {
            self.calibrate_actual_size(true);
            return;
        }
        let mut to = match (self.camera.is_ortho() && self.anim.is_none(), self.nearest_standard()) {
            (false, Some(v)) => Camera::standard(v, self.camera.target, self.camera.distance),
            _ => self.camera,
        };
        self.settings.apply_view(&mut to);
        to.distance = actual_size_distance(&to, self.size().y, per_mm);
        self.animate_to(to);
        self.toast("Actual size: 1 mm on the model is 1 mm on the screen");
    }

    /// Opens the calibration card, starting from the last calibration or a guess for this kind of screen.
    pub fn calibrate_actual_size(&mut self, then_apply: bool) {
        let s = self.settings.screen_points_per_mm;
        let start = if s.is_finite() && s >= MIN_POINTS_PER_MM { s } else { GUESS_POINTS_PER_MM };
        self.calibrating = Some((start, then_apply));
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
            Some(b) => shapecraft_engine::cmd::body_box(&self.session, b).ok().map(|bb| (bb.min, bb.max)),
            None => self.session.doc.bounds(),
        };
        if let Some((min, max)) = bounds {
            let mut c = self.camera;
            c.fit(min, max);
            self.animate_to(c);
        }
    }

    /// The selected bodies' (and their faces' and edges' bodies') box, from their display meshes.
    ///
    /// Asked every frame (for the "Zoom to Selection" pill), so the box is kept for as long as the picked bodies and
    /// their meshes stay the same: going over every vertex of a couple of thousand picked bodies costs a frame.
    pub(super) fn selection_bounds(&self) -> Option<(DVec3, DVec3)> {
        use std::hash::{Hash, Hasher};
        let picked: std::collections::HashSet<_> = self.session.selection.iter().filter_map(|p| p.body()).collect();
        if picked.is_empty() {
            return None;
        }
        let bodies: Vec<_> = self.session.doc.bodies.iter().filter(|b| picked.contains(&b.id)).collect();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for b in &bodies {
            (b.id.0, std::sync::Arc::as_ptr(&b.mesh) as usize).hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some((was, found)) = self.selection_box.get()
            && was == key
        {
            return found;
        }
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        for b in bodies {
            for p in &b.mesh.positions {
                let q = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        let found = (lo.x.is_finite() && hi.x.is_finite()).then_some((lo, hi));
        self.selection_box.set(Some((key, found)));
        found
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
        self.camera = if a.settle {
            Camera::blend(&a.from, &a.to, crate::motion::settle((self.now - a.start) as f32).into())
        } else {
            Camera::lerp(&a.from, &a.to, t)
        };
        if t >= 1.0 {
            self.camera = a.to;
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

    /// A finger's double tap on a touch screen: turns to look straight at the face under the
    /// point and frames it. A flat face is exact; a curved one is faced along its average normal.
    pub(crate) fn zoom_to_face(&mut self, at: egui::Pos2) -> bool {
        let Some((Pick::Face { body, face }, _)) = self.pick_at(at) else { return false };
        let Some(b) = self.session.doc.body(body) else { return false };
        let m = &b.mesh;
        let (mut weighted, mut sum, mut area) = (DVec3::ZERO, DVec3::ZERO, 0.0);
        for (t, f) in m.triangle_faces.iter().enumerate() {
            if *f != face {
                continue;
            }
            let corner = |k: usize| {
                m.indices
                    .get(t * 3 + k)
                    .and_then(|i| m.positions.get(*i as usize))
                    .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
            };
            let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else { continue };
            let n = (b - a).cross(c - a);
            let w = n.length() / 2.0;
            weighted += n;
            sum += (a + b + c) / 3.0 * w;
            area += w;
        }
        if area.is_nan() || area <= 0.0 || !weighted.is_finite() {
            return false;
        }
        let Some(plane) = SketchPlane::from_normal(sum / area, weighted.normalize()) else { return false };
        self.animate_to(Camera::facing(plane.normal(), plane.y, sum / area, self.camera.distance));
        self.frame_face(plane, body, face);
        true
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
        if self.settings.free_orbit {
            let px = step / 0.008;
            let (dx, dy) = [(-px, 0.0), (px, 0.0), (0.0, px), (0.0, -px)][usize::from(dir.min(3))];
            to.orbit_free(dx, dy);
            self.animate_to(to);
            return;
        }
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

#[cfg(test)]
mod actual_size_tests {
    use super::*;

    #[test]
    fn actual_size_puts_one_model_mm_on_one_screen_mm() {
        for (fov, height, per_mm) in [(30.0f64, 800.0, 4.33), (60.0, 1324.0, 3.78), (10.0, 500.0, 6.1)] {
            let mut c = Camera::standard(StandardView::Top, DVec3::ZERO, 50.0);
            c.fov_y = fov.to_radians();
            c.distance = actual_size_distance(&c, height, per_mm);
            let mm_per_point = c.world_per_pixel(height);
            assert!((mm_per_point * per_mm - 1.0).abs() < 1e-9, "{fov} {height} {per_mm}: {mm_per_point}");
        }
        assert_eq!(clamp_points_per_mm(f64::NAN), GUESS_POINTS_PER_MM);
        assert_eq!(clamp_points_per_mm(0.01), MIN_POINTS_PER_MM);
    }
}

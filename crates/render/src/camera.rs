use glam::{DMat4, DVec2, DVec3, Mat4};

/// An orbit camera around `target`. Z is up, like the reference app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: DVec3,
    /// Rotation about Z, radians. 0 looks along +Y (the Front view).
    pub yaw: f64,
    /// Elevation, radians. Positive looks down.
    pub pitch: f64,
    pub distance: f64,
    pub fov_y: f64,
    /// 0 = perspective, 1 = orthographic; in between while a view animation blends them. Standard views and
    /// views square to a sketch plane are orthographic, orbiting goes back to perspective (as observed).
    pub ortho: f64,
    /// Orthographic whatever the view (the View popover's field of view set to 0°).
    pub always_ortho: bool,
    /// Turn about the view direction, radians (the orientation cube's 90° buttons). Positive turns the picture
    /// counter-clockwise.
    pub roll: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardView {
    Default,
    Front,
    Back,
    Top,
    Bottom,
    Right,
    Left,
}

const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;

impl Default for Camera {
    fn default() -> Self {
        // 26.202 at 1600×1000: a 13 mm square at the origin spans 306 px across (s36); 89.4 (matched to 26.200)
        // drew it 1.19× bigger.
        Self::standard(StandardView::Default, DVec3::ZERO, 106.1)
    }
}

impl Camera {
    pub fn standard(view: StandardView, target: DVec3, distance: f64) -> Self {
        let (yaw, pitch) = match view {
            // Observed: X runs to the lower right, Y to the upper right, Z up, both at the same slope (26.202:
            // a square at the origin is a symmetric diamond, s36).
            StandardView::Default => (-std::f64::consts::FRAC_PI_4, 0.52),
            StandardView::Front => (0.0, 0.0),
            StandardView::Back => (std::f64::consts::PI, 0.0),
            StandardView::Right => (HALF_PI, 0.0),
            StandardView::Left => (-HALF_PI, 0.0),
            StandardView::Top => (0.0, HALF_PI),
            StandardView::Bottom => (0.0, -HALF_PI),
        };
        let ortho = if view == StandardView::Default { 0.0 } else { 1.0 };
        Self { target, yaw, pitch, distance, fov_y: 30f64.to_radians(), ortho, always_ortho: false, roll: 0.0 }
    }

    /// How orthographic the projection is right now (0..1).
    pub fn ortho_amount(&self) -> f64 {
        if self.always_ortho { 1.0 } else { self.ortho.clamp(0.0, 1.0) }
    }

    pub fn is_ortho(&self) -> bool {
        self.ortho_amount() > 1.0 - 1e-6
    }

    /// Half the visible height at the target's depth; the same whatever the projection, so switching keeps the scale.
    fn half_height(&self) -> f64 {
        self.distance * (self.fov_y / 2.0).tan()
    }

    /// The field of view and eye distance actually used: blending towards orthographic narrows the view and
    /// backs the eye off (a dolly zoom), so the target plane keeps its size.
    fn effective(&self) -> (f64, f64) {
        let fov = (self.fov_y * (1.0 - self.ortho_amount())).max(self.fov_y * 0.01);
        (fov, self.half_height() / (fov / 2.0).tan())
    }

    /// Unit vector from the target towards the eye.
    pub fn back(&self) -> DVec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        DVec3::new(-sy * cp, -cy * cp, sp)
    }

    /// The eye position. For an orthographic view it's only a reference point at `distance` (rays are parallel).
    pub fn eye(&self) -> DVec3 {
        let d = if self.is_ortho() { self.distance } else { self.effective().1 };
        self.target + self.back() * d
    }

    /// Screen-right and screen-up directions in world space.
    pub fn basis(&self) -> (DVec3, DVec3) {
        let (sy, cy) = self.yaw.sin_cos();
        let right = DVec3::new(cy, -sy, 0.0);
        let up = self.back().cross(right).normalize_or(DVec3::Z);
        if self.roll == 0.0 {
            return (right, up);
        }
        let (sr, cr) = self.roll.sin_cos();
        (right * cr - up * sr, up * cr + right * sr)
    }

    pub fn view(&self) -> DMat4 {
        let (_, up) = self.basis();
        DMat4::look_at_rh(self.eye(), self.target, up)
    }

    pub fn proj(&self, aspect: f64) -> DMat4 {
        let aspect = aspect.max(1e-6);
        if self.is_ortho() {
            let (h, depth) = (self.half_height(), self.distance * 100.0);
            return DMat4::orthographic_rh(-h * aspect, h * aspect, -h, h, -depth, depth);
        }
        let (fov, d) = self.effective();
        let near = (d - self.distance * 50.0).max(d * 0.01).max(1e-4);
        let far = d + self.distance * 100.0;
        DMat4::perspective_rh(fov, aspect, near, far)
    }

    pub fn view_proj(&self, size: DVec2) -> Mat4 {
        (self.proj(size.x / size.y.max(1.0)) * self.view()).as_mat4()
    }

    /// World point → pixel position (top-left origin) and depth; `None` behind the camera.
    pub fn project(&self, p: DVec3, size: DVec2) -> Option<(DVec2, f64)> {
        let clip = self.proj(size.x / size.y.max(1.0)) * self.view() * p.extend(1.0);
        if clip.w <= 1e-9 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        let depth = (p - self.eye()).dot(-self.back());
        Some((DVec2::new((ndc.x + 1.0) * 0.5 * size.x, (1.0 - ndc.y) * 0.5 * size.y), depth))
    }

    /// The ray through a pixel: origin and unit direction.
    pub fn ray(&self, px: DVec2, size: DVec2) -> (DVec3, DVec3) {
        let ndc = DVec2::new(px.x / size.x * 2.0 - 1.0, 1.0 - px.y / size.y * 2.0);
        let inv = (self.proj(size.x / size.y.max(1.0)) * self.view()).inverse();
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        if self.is_ortho() {
            // Parallel rays, starting behind everything in view.
            return (near, -self.back());
        }
        (self.eye(), (far - near).normalize_or(-self.back()))
    }

    /// World units per pixel at the target's depth.
    pub fn world_per_pixel(&self, viewport_height: f64) -> f64 {
        2.0 * self.distance * (self.fov_y / 2.0).tan() / viewport_height.max(1.0)
    }

    pub fn orbit(&mut self, dx_px: f64, dy_px: f64) {
        self.ortho = 0.0;
        self.roll = 0.0;
        self.yaw += dx_px * 0.008;
        self.pitch = (self.pitch + dy_px * 0.008).clamp(-HALF_PI, HALF_PI);
    }

    pub fn pan(&mut self, dx_px: f64, dy_px: f64, viewport_height: f64) {
        let (right, up) = self.basis();
        let k = self.world_per_pixel(viewport_height);
        self.target += (-right * dx_px + up * dy_px) * k;
    }

    /// Zooms by `factor` (<1 moves in), keeping the world point under the cursor fixed.
    pub fn zoom_at(&mut self, factor: f64, cursor: DVec2, size: DVec2) {
        let factor = factor.clamp(0.2, 5.0);
        let (o, d) = self.ray(cursor, size);
        // Point under the cursor on the plane through the target facing the camera.
        let n = self.back();
        let denom = d.dot(n);
        let anchor = if denom.abs() > 1e-9 { o + d * ((self.target - o).dot(n) / denom) } else { self.target };
        self.target = anchor + (self.target - anchor) * factor;
        self.distance = (self.distance * factor).clamp(1e-3, 1e7);
    }

    /// Frames a bounding box.
    pub fn fit(&mut self, min: DVec3, max: DVec3) {
        let center = (min + max) / 2.0;
        let radius = ((max - min).length() / 2.0).max(1.0);
        self.target = center;
        self.distance = radius / (self.fov_y / 2.0).sin() * 1.15;
    }

    /// Smooth step between two cameras for view animations, taking the short way round in yaw.
    pub fn lerp(a: &Camera, b: &Camera, t: f64) -> Camera {
        let t = t.clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        let dyaw = short_turn(b.yaw - a.yaw);
        Camera {
            target: a.target.lerp(b.target, s),
            yaw: a.yaw + dyaw * s,
            pitch: a.pitch + (b.pitch - a.pitch) * s,
            distance: a.distance * (b.distance / a.distance).powf(s),
            fov_y: a.fov_y + (b.fov_y - a.fov_y) * s,
            ortho: a.ortho_amount() + (b.ortho_amount() - a.ortho_amount()) * s,
            always_ortho: b.always_ortho,
            roll: a.roll + short_turn(b.roll - a.roll) * s,
        }
    }

    /// A camera looking straight at a plane (the sketch-plane animation target).
    pub fn facing(normal: DVec3, up_hint: DVec3, target: DVec3, distance: f64) -> Camera {
        let n = normal.normalize_or(DVec3::Z);
        let pitch = n.z.clamp(-1.0, 1.0).asin();
        let yaw = if n.z.abs() > 0.999 {
            // Looking straight down/up: keep the plane's y axis pointing up the screen.
            if n.z > 0.0 { up_hint.x.atan2(up_hint.y) } else { (-up_hint.x).atan2(-up_hint.y) }
        } else {
            (-n.x).atan2(-n.y)
        };
        Camera { target, yaw, pitch, distance, fov_y: 30f64.to_radians(), ortho: 1.0, always_ortho: false, roll: 0.0 }
    }
}

/// An angle difference taken the short way round (−π..π).
fn short_turn(d: f64) -> f64 {
    let mut d = d % std::f64::consts::TAU;
    if d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    } else if d < -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_the_target_through_the_centre() {
        let c = Camera::default();
        let size = DVec2::new(800.0, 600.0);
        let (o, d) = c.ray(size / 2.0, size);
        let to_target = (c.target - o).normalize();
        assert!(d.dot(to_target) > 0.9999);
        let (px, _) = c.project(c.target, size).unwrap_or_default();
        assert!((px - size / 2.0).length() < 1e-6);
    }

    #[test]
    fn top_view_looks_down() {
        let c = Camera::standard(StandardView::Top, DVec3::ZERO, 10.0);
        assert!((c.eye() - DVec3::new(0.0, 0.0, 10.0)).length() < 1e-6);
        let size = DVec2::new(100.0, 100.0);
        let (x, _) = c.project(DVec3::X, size).unwrap_or_default();
        let (y, _) = c.project(DVec3::Y, size).unwrap_or_default();
        assert!(x.x > 50.0 && (x.y - 50.0).abs() < 1e-6, "X goes right");
        assert!(y.y < 50.0 && (y.x - 50.0).abs() < 1e-6, "Y goes up");
    }

    #[test]
    fn front_view_has_z_up() {
        let c = Camera::standard(StandardView::Front, DVec3::ZERO, 10.0);
        let size = DVec2::new(100.0, 100.0);
        let (z, _) = c.project(DVec3::Z, size).unwrap_or_default();
        let (x, _) = c.project(DVec3::X, size).unwrap_or_default();
        assert!(z.y < 50.0 && x.x > 50.0);
    }

    #[test]
    fn standard_views_are_orthographic() {
        let size = DVec2::new(100.0, 100.0);
        let mut c = Camera::standard(StandardView::Front, DVec3::ZERO, 10.0);
        assert!(c.is_ortho());
        // A point in front of the target and one behind it land on the same pixel: no perspective.
        let (a, _) = c.project(DVec3::new(1.0, -3.0, 1.0), size).unwrap_or_default();
        let (b, _) = c.project(DVec3::new(1.0, 3.0, 1.0), size).unwrap_or_default();
        assert!((a - b).length() < 1e-6);
        // Rays are parallel to the view direction and start in front of the scene.
        let (o, d) = c.ray(DVec2::new(20.0, 70.0), size);
        assert!((d - DVec3::Y).length() < 1e-9 && o.y < -10.0);
        // The scale at the target is what the perspective view had.
        let p = Camera { ortho: 0.0, ..c };
        let (x_o, _) = c.project(DVec3::X, size).unwrap_or_default();
        let (x_p, _) = p.project(DVec3::X, size).unwrap_or_default();
        assert!((x_o - x_p).length() < 1e-6);
        c.orbit(5.0, 0.0);
        assert!(!c.is_ortho());
        assert!(!Camera::default().is_ortho());
    }

    #[test]
    fn roll_turns_the_picture() {
        let size = DVec2::new(100.0, 100.0);
        let mut c = Camera::standard(StandardView::Front, DVec3::ZERO, 10.0);
        c.roll = std::f64::consts::FRAC_PI_2;
        // A quarter turn counter-clockwise: X, which pointed right, now points up the screen.
        let (x, _) = c.project(DVec3::X, size).unwrap_or_default();
        assert!(x.y < 50.0 - 1.0 && (x.x - 50.0).abs() < 1e-6, "{x:?}");
    }

    #[test]
    fn facing_matches_standard_views() {
        let f = Camera::facing(DVec3::new(0.0, -1.0, 0.0), DVec3::Z, DVec3::ZERO, 10.0);
        assert!((f.eye() - DVec3::new(0.0, -10.0, 0.0)).length() < 1e-6);
        let t = Camera::facing(DVec3::Z, DVec3::Y, DVec3::ZERO, 10.0);
        let s = Camera::standard(StandardView::Top, DVec3::ZERO, 10.0);
        assert!((t.yaw - s.yaw).abs() < 1e-9);
    }
}

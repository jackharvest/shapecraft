//! Where the pointer lands: on the active sketch plane (snapped to key points, lined up with them, or to the
//! grid) or on the model, plus the arc preview and the small geometry helpers snapping needs.

use egui::{Color32, Pos2, Rect, Stroke, vec2};
use glam::{DVec2, DVec3};
use shapecraft_doc::{Pick, SketchEntity, SketchPlane};

use crate::theme;
use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

const SNAP_PX: f64 = 10.0;
const ALIGN_PX: f64 = 7.0;

pub(super) struct Snap {
    pub(super) p: DVec2,
    /// Snapped onto a point (not just lined up with one).
    pub(super) point: bool,
    /// Inference lines: from the point we line up with, to the cursor.
    pub(super) guides: Vec<(DVec2, DVec2)>,
    /// What the point is ("Endpoint", "Midpoint", "Center"…), shown in a label while hovering it, as observed.
    name: Option<&'static str>,
}

impl Workspace {
    pub(super) fn sketch_plane(&self) -> Option<SketchPlane> {
        self.session.active_sketch().map(|s| s.plane)
    }

    /// The cursor on the active sketch plane, snapped to nearby sketch points and the origin.
    pub(in crate::workspace) fn sketch_cursor(&self) -> Option<DVec2> {
        self.sketch_snap().map(|s| s.p)
    }

    /// Corners, edge middles and face centers of the bodies that lie on a sketch plane (they snap and show purple
    /// while sketching, as observed), in the plane's coordinates; `true` marks corners.
    pub(in crate::workspace) fn body_points_on(&self, plane: &SketchPlane) -> Vec<(DVec2, &'static str, bool)> {
        let n = plane.normal();
        let on = |p: DVec3| (p - plane.origin).dot(n).abs() < 1e-4 * (1.0 + p.length());
        let mut out: Vec<(DVec2, &'static str, bool)> = Vec::new();
        let mut push = |p: DVec3, name: &'static str, corner: bool| {
            let q = plane.to_local(p);
            if !out.iter().any(|(o, ..)| o.distance(q) < 1e-6) {
                out.push((q, name, corner));
            }
        };
        for body in self.session.doc.bodies.iter().filter(|b| b.visible) {
            let m = &body.mesh;
            for line in &m.edges {
                let pts: Vec<DVec3> = line.iter().map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();
                let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else { continue };
                if !pts.iter().all(|p| on(*p)) || a.distance(b) < 1e-9 {
                    continue;
                }
                push(a, "Endpoint", true);
                push(b, "Endpoint", true);
                let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
                let mut left = total / 2.0;
                for w in pts.windows(2) {
                    let l = w[0].distance(w[1]);
                    if l >= left && l > 0.0 {
                        push(w[0].lerp(w[1], left / l), "Midpoint", false);
                        break;
                    }
                    left -= l;
                }
            }
            // Faces flat on the plane: their area-weighted middle.
            let mut faces: std::collections::BTreeMap<u32, (f64, DVec3, bool)> = std::collections::BTreeMap::new();
            for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
                let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
                let face = m.triangle_faces.get(t).copied().unwrap_or(u32::MAX);
                let area = (b - a).cross(c - a).length() / 2.0;
                let e = faces.entry(face).or_insert((0.0, DVec3::ZERO, true));
                e.0 += area;
                e.1 += (a + b + c) / 3.0 * area;
                e.2 &= on(a) && on(b) && on(c);
            }
            for (area, sum, flat) in faces.into_values() {
                if flat && area > 1e-12 {
                    push(sum / area, "Center", false);
                }
            }
        }
        out
    }

    /// Snapping like the reference: onto key points, else into line with them horizontally or vertically
    /// (with a purple guide line back to the point it lines up with).
    /// Where the screen point `c` meets the sketch's plane, unsnapped.
    pub(super) fn sketch_cursor_raw(&self, c: Pos2) -> Option<DVec3> {
        let plane = self.sketch_plane()?;
        let (o, d) = self.camera.ray(self.local(c), self.size());
        plane.intersect_ray(o, d)
    }

    pub(super) fn sketch_snap(&self) -> Option<Snap> {
        let cursor = self.cursor?;
        let plane = self.sketch_plane()?;
        let (o, d) = self.camera.ray(self.local(cursor), self.size());
        let hit = plane.intersect_ray(o, d)?;
        let local = plane.to_local(hit);
        let mut keys = vec![plane.to_local(DVec3::ZERO)];
        let mut names = vec!["Origin"];
        if let Some(sk) = self.session.active_sketch() {
            for e in &sk.entities {
                for (p, n) in named_key_points(e) {
                    keys.push(p);
                    names.push(n);
                }
            }
            // Where curves cross (small sketches only: every pair is tried).
            if sk.entities.len() <= 200 {
                for p in shapecraft_doc::sketch_ops::crossings(&sk.entities, 500) {
                    keys.push(p);
                    names.push("Intersection");
                }
            }
        }
        for (p, n, _) in self.body_points_on(&plane) {
            keys.push(p);
            names.push(n);
        }
        let named = keys.len();
        match self.tool {
            Tool::Line { first: Some(f), last } => {
                keys.push(f);
                keys.extend(last);
            }
            Tool::Rectangle { first: Some(a) }
            | Tool::Circle { center: Some(a) }
            | Tool::Polygon { center: Some(a) }
            | Tool::Rect3 { a: Some(a), .. } => keys.push(a),
            Tool::Arc { a, b } => keys.extend(a.into_iter().chain(b)),
            Tool::Ellipse { center: Some(c), major } => keys.extend(std::iter::once(c).chain(major)),
            Tool::Spline { ref points, .. } => keys.extend(points.iter().copied()),
            _ => {}
        }
        let screen_dist = |p: DVec2| self.to_screen(plane.to_world(p)).map(|s| to_d(s).distance(to_d(cursor)));
        // A pencil or a finger reaches a little farther for points and guide lines than a mouse.
        let slop = self.pick_slop.max(1.0) as f64 - 1.0;
        let (snap_px, align_px) = (SNAP_PX * (1.0 + slop * 0.6), ALIGN_PX * (1.0 + slop * 0.3));
        let mut best: Option<(f64, DVec2, usize)> = None;
        for (i, k) in keys.iter().enumerate() {
            if let Some(dpx) = screen_dist(*k)
                && dpx < snap_px
                && best.is_none_or(|(b, ..)| dpx < b)
            {
                best = Some((dpx, *k, i));
            }
        }
        if let Some((_, p, i)) = best
            && self.snap_opts.guide_points
        {
            let name = names.get(i).copied().filter(|_| i < named).or(Some("Point"));
            return Some(Snap { p, point: true, guides: Vec::new(), name });
        }
        if !self.snap_opts.guide_lines {
            return Some(Snap { p: self.grid_snapped(local), point: false, guides: Vec::new(), name: None });
        }
        // Line up with a key point: same u (vertical guide) and/or same v (horizontal guide).
        let mut vertical: Option<(f64, DVec2)> = None;
        let mut horizontal: Option<(f64, DVec2)> = None;
        for k in &keys {
            if let Some(dpx) = screen_dist(DVec2::new(k.x, local.y))
                && dpx < align_px
                && vertical.is_none_or(|(b, _)| dpx < b)
            {
                vertical = Some((dpx, *k));
            }
            if let Some(dpx) = screen_dist(DVec2::new(local.x, k.y))
                && dpx < align_px
                && horizontal.is_none_or(|(b, _)| dpx < b)
            {
                horizontal = Some((dpx, *k));
            }
        }
        let mut p = self.grid_snapped(local);
        if let Some((_, k)) = vertical {
            p.x = k.x;
        }
        if let Some((_, k)) = horizontal {
            p.y = k.y;
        }
        let guides = vertical.into_iter().chain(horizontal).map(|(_, k)| (k, p)).collect();
        Some(Snap { p, point: false, guides, name: None })
    }

    /// The arc being drawn as (start, a point on it, end). With only the start placed, it runs to the pointer with
    /// the typed sweep (45° until one is typed), bulging so its center is on the left of start→pointer on screen;
    /// with both ends placed it passes through the pointer, or takes the typed sweep on the pointer's side.
    pub(in crate::workspace) fn arc_preview(&self, a: DVec2, b: Option<DVec2>, cursor: DVec2) -> Option<(DVec2, DVec2, DVec2)> {
        let typed = self
            .numeric
            .as_deref()
            .and_then(|t| t.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && v.abs() > 1e-6 && v.abs() < 360.0);
        let (end, bulge_sign) = match b {
            None => {
                let pl = self.sketch_plane()?;
                let n = (cursor - a).perp().normalize_or_zero();
                let m = (a + cursor) * 0.5;
                // Which in-plane normal points left of start→pointer on screen.
                let left = match (self.to_screen(pl.to_world(a)), self.to_screen(pl.to_world(cursor)), self.to_screen(pl.to_world(m + n))) {
                    (Some(sa), Some(sb), Some(sn)) => {
                        let d = sb - sa;
                        (sn - (sa + d * 0.5)).dot(vec2(d.y, -d.x)) >= 0.0
                    }
                    _ => true,
                };
                // The bulge is opposite the center.
                (cursor, if left { -1.0 } else { 1.0 })
            }
            Some(b) => {
                if typed.is_none() {
                    return shapecraft_doc::circle_through(a, cursor, b).map(|_| (a, cursor, b));
                }
                let n = (b - a).perp().normalize_or_zero();
                (b, if (cursor - a).dot(n) >= 0.0 { 1.0 } else { -1.0 })
            }
        };
        let sweep = typed.unwrap_or(if b.is_none() { 45.0 } else { 0.0 }).abs().to_radians();
        let chord = a.distance(end);
        if chord < 1e-9 || sweep < 1e-9 {
            return None;
        }
        let n = (end - a).perp().normalize_or_zero() * bulge_sign;
        let r = chord / (2.0 * (sweep / 2.0).sin());
        let d = r * (sweep / 2.0).cos();
        // Center on -n at distance d (behind the chord past 180°); the curve's middle on +n.
        let mid = (a + end) * 0.5 + n * (r - d);
        Some((a, mid, end))
    }

    /// The hovered key point's name in a small dark label just above it.
    pub(super) fn paint_point_label(&self, painter: &egui::Painter) {
        // 26.200: only without a tool running; a drawing tool shows its purple snap mark instead (s11-033/034).
        if !(matches!(self.tool, Tool::Idle) && self.session.active_sketch.is_some()) || self.sketch_drag.is_some() {
            return;
        }
        let Some(snap) = self.sketch_snap() else { return };
        let (Some(name), Some(pl)) = (snap.name, self.sketch_plane()) else { return };
        let Some(at) = self.to_screen(pl.to_world(snap.p)) else { return };
        let g = painter.layout_no_wrap(name.to_string(), theme::strong(12.5), Color32::WHITE);
        let r = Rect::from_center_size(at - vec2(0.0, 31.0), g.size() + vec2(16.0, 11.0));
        painter.rect_filled(r, egui::CornerRadius::same(5), theme::c(0x2c2c2e));
        painter.rect_stroke(r, egui::CornerRadius::same(5), Stroke::new(1.0, theme::c(0x48484a)), egui::StrokeKind::Inside);
        painter.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    }

    /// With Snap ▸ Grid on, the point rounded to the grid (when it's within a few pixels of a grid line).
    fn grid_snapped(&self, p: DVec2) -> DVec2 {
        if !self.snap_opts.grid {
            return p;
        }
        let wpp = self.camera.world_per_pixel(self.size().y);
        let step = self.grid_locked.unwrap_or_else(|| shapecraft_render::grid_step(wpp));
        let near = |v: f64| {
            let r = (v / step).round() * step;
            if (r - v).abs() / wpp < 6.0 { r } else { v }
        };
        DVec2::new(near(p.x), near(p.y))
    }

    /// A point picked in 3D: a body corner or edge middle near the pointer, else where the pointer meets the model.
    pub(in crate::workspace) fn snap_point(&self, c: Pos2) -> Option<DVec3> {
        let mut best: Option<(f32, DVec3)> = None;
        for body in self.session.doc.bodies.iter().filter(|b| b.visible) {
            for line in &body.mesh.edges {
                let (Some(a), Some(b)) = (line.first(), line.last()) else { continue };
                let (a, b) = (DVec3::from(a.map(f64::from)), DVec3::from(b.map(f64::from)));
                let mid = line.get(line.len() / 2).map(|m| DVec3::from(m.map(f64::from)));
                for p in [Some(a), Some(b), mid.filter(|_| line.len() > 2).or(Some((a + b) / 2.0))].into_iter().flatten() {
                    if let Some(s) = self.to_screen(p) {
                        let d = (s - c).length();
                        if d < 12.0 && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, p));
                        }
                    }
                }
            }
        }
        best.map(|(_, p)| p).or_else(|| self.pick_at(c).map(|(_, hit)| hit))
    }

    /// Where a dragged gizmo center lands: on the model under the pointer, snapping to a face's center when near
    /// it (`true`).
    pub(super) fn relocation_target(&self, p: Pos2) -> Option<(DVec3, bool)> {
        let (pick, hit) = self.pick_at(p)?;
        let center = match pick {
            Pick::Face { body, face } => self
                .session
                .doc
                .body(body)
                .and_then(|b| self.session.kernel.measure(&b.shape, shapecraft_kernel::SubShape::Face(face)).ok())
                .map(|(_, c)| c),
            _ => None,
        };
        let near = center.filter(|c| self.to_screen(*c).is_some_and(|s| (s - p).length() < 16.0));
        Some((near.unwrap_or(hit), near.is_some()))
    }
}

impl Workspace {
    /// A gizmo frame lying on the flat face under `p`: Z out of the face, X the world axis that lies flattest in it.
    pub(super) fn face_frame_at(&self, p: Pos2) -> Option<crate::workspace::tools::GizmoFrame> {
        let (Pick::Face { body, face }, _) = self.pick_at(p)? else { return None };
        let b = self.session.doc.body(body)?;
        let n = self.session.kernel.face_plane(&b.shape, face).ok()??.normal.normalize_or_zero();
        let x = [DVec3::X, DVec3::Y, DVec3::Z]
            .into_iter()
            .map(|a| a - n * a.dot(n))
            .max_by(|a, b| a.length().total_cmp(&b.length()))?
            .normalize_or_zero();
        (n.length() > 0.5 && x.length() > 0.5).then(|| crate::workspace::tools::GizmoFrame {
            axes: [x, n.cross(x), n],
            planar: false,
            turns: true,
        })
    }
}

pub(super) fn to_d(p: Pos2) -> DVec2 {
    DVec2::new(p.x as f64, p.y as f64)
}

/// A curve's key points with what each is called.
fn named_key_points(e: &SketchEntity) -> Vec<(DVec2, &'static str)> {
    let pts = key_points(e);
    let n = pts.len();
    pts.into_iter()
        .enumerate()
        .map(|(i, p)| {
            let name = match e {
                SketchEntity::Line { .. } => ["Endpoint", "Endpoint", "Midpoint"][i.min(2)],
                SketchEntity::Rectangle { .. } => {
                    ["Endpoint", "Endpoint", "Endpoint", "Endpoint", "Midpoint", "Midpoint", "Midpoint", "Midpoint", "Center"][i.min(8)]
                }
                SketchEntity::Polygon { .. } if i + 1 == n => "Center",
                SketchEntity::Polygon { .. } => "Endpoint",
                SketchEntity::Arc { .. } if i == 1 => "Midpoint",
                SketchEntity::Arc { .. } => "Endpoint",
                SketchEntity::Circle { .. } | SketchEntity::Ellipse { .. } => "Center",
                SketchEntity::Spline { .. } if i == 0 || i + 1 == n => "Endpoint",
                SketchEntity::ControlSpline { .. } => "Endpoint",
                _ => "Point",
            };
            (p, name)
        })
        .collect()
}

fn key_points(e: &SketchEntity) -> Vec<DVec2> {
    match *e {
        SketchEntity::Line { a, b } => vec![a, b, (a + b) / 2.0],
        SketchEntity::Rectangle { min, max } => {
            let c = (min + max) / 2.0;
            vec![
                min,
                max,
                DVec2::new(min.x, max.y),
                DVec2::new(max.x, min.y),
                DVec2::new(c.x, min.y),
                DVec2::new(c.x, max.y),
                DVec2::new(min.x, c.y),
                DVec2::new(max.x, c.y),
                c,
            ]
        }
        SketchEntity::Circle { center, .. } => vec![center],
        SketchEntity::Polygon { center, radius, sides, angle } => {
            let mut v = SketchEntity::polygon_points(center, radius, sides, angle);
            v.push(center);
            v
        }
        SketchEntity::Arc { a, mid, b } => vec![a, mid, b],
        SketchEntity::Ellipse { center, .. } => vec![center],
        SketchEntity::Spline { ref points } => points.clone(),
        SketchEntity::ControlSpline { ref poles } => poles.first().into_iter().chain(poles.last()).copied().collect(),
        SketchEntity::Text { origin, .. } => vec![origin],
    }
}

pub(super) fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> (f64, f64) {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((a + ab * t).distance(p), t)
}

/// The other half-axis of an ellipse with center `c` and first axis end `m`: the cursor's distance from that axis.
pub(super) fn ellipse_minor(c: DVec2, m: DVec2, cursor: DVec2) -> f64 {
    (cursor - c).dot((m - c).normalize_or_zero().perp()).abs()
}

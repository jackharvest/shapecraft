//! Circular Pattern in the view: the turning arrows around a dot, pulling one, and the pills beside it (#96).

use egui::{Color32, Pos2, Stroke, Ui, vec2};
use glam::DVec3;
use shapecraft_doc::{ItemId, Pick};

use super::paint::{axis_pill_ex, keep_row_in, pill_half};
use super::snap::{seg_dist, to_d};
use crate::theme::{self, Tokens};
use crate::workspace::Workspace;
use crate::workspace::badges::Badge;
use crate::workspace::tools::{CircularPatternTool, Handle, PatternTool, Tool};

/// A pull goes in steps of this many degrees (any angle can be typed).
const STEP: f64 = 5.0;
/// How far from the dot the handle stands, in pixels: where the Move gizmo's turning arrows are.
const RADIUS: f64 = 58.0;

impl Workspace {
    pub(super) fn near_dot(&self, pivot: DVec3, c: Pos2) -> bool {
        self.to_screen(pivot).is_some_and(|s| (s - c).length() < 9.0)
    }

    /// The turning arrows on screen: all three before the pull (around the sketch's normal only, for curves), then
    /// the pulled one where it got to.
    fn circular_shapes(&self, t: &CircularPatternTool) -> Vec<(usize, Vec<Pos2>)> {
        let Some((d, _)) = t.turn else {
            let sketch = t.items.is_empty();
            let rings = self.gizmo_shapes(t.pivot, t.frame).into_iter();
            return rings
                .filter_map(|(h, pts)| match h {
                    Handle::Ring(a) if !sketch || a == 2 => Some((a, pts)),
                    _ => None,
                })
                .collect();
        };
        let r = self.camera.world_per_pixel(self.size().y) * RADIUS;
        let Some(arm) = t.arm_at(t.angle) else { return Vec::new() };
        let pts = (-3..=3).filter_map(|i| {
            let q = glam::DQuat::from_axis_angle(d, f64::from(i) * 0.062);
            self.to_screen(t.pivot + q * arm * r)
        });
        vec![(0, pts.collect())]
    }

    /// The turning arrow under the pointer.
    pub(super) fn circular_hit(&self, t: &CircularPatternTool, p: Pos2) -> Option<usize> {
        let mut best: Option<(f64, usize)> = None;
        for (a, pts) in self.circular_shapes(t) {
            for w in pts.windows(2) {
                let (d, _) = seg_dist(to_d(p), to_d(w[0]), to_d(w[1]));
                if d < 10.0 && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, a));
                }
            }
        }
        best.map(|(_, a)| a)
    }

    /// The pointer's angle around the axis in degrees; seen nearly edge-on, its angle on screen around the dot.
    fn angle_about(&self, p: Pos2, pivot: DVec3, (d, arm): (DVec3, DVec3)) -> Option<f64> {
        let back = self.camera.back();
        if d.dot(back).abs() > 0.15
            && let Some(q) = self.on_plane(p, pivot, d)
            && (q - pivot).dot(back).is_finite()
        {
            let v = q - pivot;
            return Some(d.dot(arm.cross(v)).atan2(arm.dot(v)).to_degrees());
        }
        let c = self.to_screen(pivot)?;
        let facing = if d.dot(back) >= 0.0 { -1.0 } else { 1.0 };
        Some(f64::from((p.y - c.y).atan2(p.x - c.x)).to_degrees() * facing)
    }

    pub(super) fn circular_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        let Tool::CircularPattern(t) = &self.tool else { return };
        let (pivot, frame, turn) = (t.pivot, t.frame, t.turn);
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            let hit = self.circular_hit(t, p);
            if turn.is_none() && self.near_dot(pivot, p) {
                self.take_pattern_value();
                if let Tool::CircularPattern(t) = &mut self.tool {
                    t.relocating = true;
                }
            } else if let Some(a) = hit {
                self.pull_turning_arrow(a, p);
            }
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if matches!(&self.tool, Tool::CircularPattern(t) if t.relocating) {
            // Over a flat face the arrows turn to it, as Move's do.
            let onto = pointer.filter(|_| self.auto_orient()).and_then(|p| self.face_frame_at(p));
            let target = pointer.and_then(|p| self.center_target(p));
            if let Tool::CircularPattern(t) = &mut self.tool {
                if let Some((at, snapped)) = target {
                    (t.pivot, t.snapped) = (at, snapped);
                    t.frame = onto.filter(|_| !t.items.is_empty()).unwrap_or(frame);
                }
                if resp.drag_stopped() {
                    t.relocating = false;
                }
            }
            return;
        }
        let Tool::CircularPattern(t) = &self.tool else { return };
        let (Some((last, pulled)), Some(turn), Some(p)) = (t.dragging, t.turn, pointer) else { return };
        let now = self.angle_about(p, pivot, turn).unwrap_or(last);
        let mut step = now - last;
        if step > 180.0 {
            step -= 360.0;
        } else if step < -180.0 {
            step += 360.0;
        }
        let pulled = (pulled + step).clamp(-360.0, 360.0);
        if let Tool::CircularPattern(t) = &mut self.tool {
            t.dragging = Some((now, pulled));
            t.angle = (pulled / STEP).round() * STEP;
            if resp.drag_stopped() {
                t.dragging = None;
                // Let go where it started: nothing to repeat, all three arrows are back.
                if !t.ready() && matches!(t.about, Some(crate::workspace::tools::CircularAxis::Free(..))) {
                    t.about = None;
                    t.turn = None;
                }
            }
        }
    }

    /// Takes hold of a turning arrow: before the pull it picks the axis. A value being typed is kept.
    fn pull_turning_arrow(&mut self, a: usize, at: Pos2) {
        self.take_pattern_value();
        let Tool::CircularPattern(t) = &self.tool else { return };
        let mut t = t.clone();
        if t.turn.is_none() {
            let axes = t.frame.axes.map(|ax| ax * self.toward_viewer(ax));
            t.set_axis(t.frame.axes[a], axes[(a + 1) % 3] + axes[(a + 2) % 3]);
            t.angle = 0.0;
        }
        let Some(turn) = t.turn else { return };
        let Some(angle) = self.angle_about(at, t.pivot, turn) else { return };
        t.dragging = Some((angle, t.angle));
        self.tool = Tool::CircularPattern(t);
    }

    /// Where the dragged dot lands: a face's center or another notable point near the pointer (a hole's center, a
    /// corner, an edge's middle), else on the model.
    fn center_target(&self, p: Pos2) -> Option<(DVec3, bool)> {
        // Curves: anywhere on their sketch, snapped as drawing is.
        if let Tool::CircularPattern(t) = &self.tool
            && t.items.is_empty()
        {
            return match (self.sketch_plane(), self.sketch_cursor()) {
                (Some(plane), Some(c)) => Some((plane.to_world(c), false)),
                _ => self.on_plane(p, t.pivot, t.frame.axes[2]).map(|at| (at, false)),
            };
        }
        let (on_model, snapped) = self.relocation_target(p).map_or((None, false), |(at, s)| (Some(at), s));
        if snapped {
            return on_model.map(|at| (at, true));
        }
        let near = self.snap_point(p).filter(|q| on_model.is_none_or(|m| m.distance(*q) > 1e-9));
        match (near, on_model) {
            (Some(q), _) => Some((q, true)),
            (None, Some(m)) => Some((m, false)),
            (None, None) => None,
        }
    }

    /// A click on a body before the pull takes it into the pattern, or out of it (the last one stays).
    pub(super) fn toggle_patterned(&mut self, body: ItemId) {
        let mut items = match &self.tool {
            Tool::Pattern(t) => t.items.clone(),
            Tool::CircularPattern(t) => t.items.clone(),
            _ => return,
        };
        match items.iter().position(|i| *i == body) {
            Some(_) if items.len() == 1 => return,
            Some(k) => {
                items.remove(k);
            }
            None => items.push(body),
        }
        self.session.selection = items.into_iter().map(|body| Pick::Body { body }).collect();
        match &mut self.tool {
            Tool::Pattern(t) => {
                if let Some(n) = PatternTool::from_selection(&self.session) {
                    *t = PatternTool { count: t.count, total: t.total, result: t.result, ..n };
                }
            }
            Tool::CircularPattern(t) => {
                if let Some(n) = CircularPatternTool::from_selection(&self.session) {
                    let (count, uniform, spacing, result) = (t.count, t.uniform, t.spacing, t.result);
                    *t = CircularPatternTool { count, uniform, spacing, result, ..n };
                }
            }
            _ => {}
        }
        self.preview = None;
    }

    pub(super) fn draw_circular(&self, ui: &Ui, painter: &egui::Painter, t: &CircularPatternTool, typed: Option<&str>) {
        let rim = theme::c(0x6e6e73);
        let Some(c) = self.to_screen(t.pivot) else { return };
        if let Some((d, arm)) = t.turn {
            // The circle the copies go round, with a dashed radius to where they start and where they end.
            let wpp = self.camera.world_per_pixel(self.size().y);
            let start = t.reach().filter(|v| v.length() > wpp * RADIUS * 0.5).unwrap_or(arm * wpp * RADIUS);
            let at = |deg: f64| self.to_screen(t.pivot + glam::DQuat::from_axis_angle(d, deg.to_radians()) * start);
            let ink = Color32::from_white_alpha(225);
            let dash = |pts: &[Pos2]| {
                painter.add(egui::Shape::dashed_line(pts, Stroke::new(1.3, ink), 5.0, 5.0));
            };
            let mut run: Vec<Pos2> = Vec::new();
            for i in 0..=120 {
                match at(f64::from(i) * 3.0) {
                    Some(q) => run.push(q),
                    None => {
                        dash(&run);
                        run.clear();
                    }
                }
            }
            dash(&run);
            let end = t.total_angle(t.angle);
            for deg in [0.0, end] {
                let (Some(q), Some(before)) = (at(deg), at(deg + if deg == 0.0 { 2.0 } else { -2.0 * end.signum() })) else { continue };
                dash(&[c, q]);
                let dir = (q - before).normalized();
                let perp = vec2(-dir.y, dir.x);
                let head = vec![q + dir * 4.0, q - dir * 6.0 + perp * 4.0, q - dir * 6.0 - perp * 4.0];
                painter.add(egui::Shape::convex_polygon(head, Color32::WHITE, Stroke::NONE));
            }
        }
        let hover = self.cursor.and_then(|p| self.circular_hit(t, p));
        let pulled = t.turn.is_some();
        for (a, pts) in self.circular_shapes(t) {
            if pts.len() < 2 {
                continue;
            }
            let fill = if pulled || hover == Some(a) { Tokens::accent() } else { theme::c(0xf4f4f6) };
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(5.5, rim)));
            painter.add(egui::Shape::line(pts.clone(), Stroke::new(3.2, fill)));
            for (tip, from) in [(pts[0], pts[1]), (pts[pts.len() - 1], pts[pts.len() - 2])] {
                let dir = (tip - from).normalized();
                let perp = vec2(-dir.y, dir.x);
                let head = vec![tip + dir * 6.0, tip + perp * 4.5, tip - perp * 4.5];
                painter.add(egui::Shape::convex_polygon(head, fill, Stroke::new(1.0, rim)));
            }
        }
        // The dot (purple on a snap point); it can be moved until an arrow is pulled.
        if !pulled {
            let ring = if t.snapped { theme::c(0x8b5cf6) } else { rim };
            painter.circle_filled(c, 7.0, theme::c(0xf4f4f6));
            painter.circle_stroke(c, 7.0, Stroke::new(if t.snapped { 2.0 } else { 1.0 }, ring));
            painter.circle_filled(c, 2.5, if t.snapped { ring } else { Tokens::accent() });
            return;
        }
        let Some(h) = self.circular_shapes(t).first().and_then(|(_, pts)| pts.get(pts.len() / 2).copied()) else { return };
        // The pills run on outward from the handle; nearly end-on they run to the right.
        let dir = if (h - c).length() > 12.0 { (h - c).normalized() } else { vec2(1.0, 0.0) };
        let (value_typed, count_typed) = if t.count_field { (None, typed) } else { (typed, None) };
        let kind = format!("{} ⌄", crate::i18n::tr(if t.spacing { "Spacing" } else { "Total" }));
        let value = value_typed.map_or_else(|| format!("{}°", (t.angle * 1e6).round() / 1e6), str::to_string);
        let many = crate::i18n::tr("Quantity");
        let quantity = count_typed.map_or_else(|| format!("{many}  {}×", t.count), |n| format!("{many}  {n}"));
        let way = format!("{} ⌄", crate::i18n::tr(if t.uniform { "Uniform" } else { "Rotated" }));
        let badges = self.badges();
        let has = |b: Badge| badges.iter().any(|(x, _)| *x == b);
        let mut row = vec![(0, kind), (1, value), (2, quantity)];
        if has(Badge::Orientation) {
            row.push((3, way));
        }
        let length: f32 = row.iter().map(|(_, s)| pill_half(s) * 2.0 + 6.0).sum();
        let mut at = h + dir * 34.0;
        at += keep_row_in(self.reachable(), at, at + dir * (length + if has(Badge::Result) { 30.0 } else { 0.0 }), h);
        let mut drawn = Vec::new();
        for (i, text) in &row {
            let half = pill_half(text);
            at += dir * half;
            let blue = match i {
                1 => !t.count_field,
                2 => t.count_field,
                _ => false,
            };
            let r = axis_pill_ex(painter, at, dir, text, blue, *i == 1);
            match i {
                0 => drawn.push((Badge::Definition, r)),
                2 => {
                    ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("pattern-count-pill"), r));
                }
                3 => drawn.push((Badge::Orientation, r)),
                _ => {}
            }
            at += dir * (half + 6.0);
        }
        crate::workspace::badges::paint_row_after(self, ui, painter, at + dir * 8.0, dir, drawn);
    }
}

//! Area Selection, as observed (299, 308–309): a left-drag that grabs nothing draws a box. Dragged right it takes
//! what lies fully inside it (cyan), dragged left whatever it crosses (purple); Tab cycles what it takes.

use egui::text::LayoutJob;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, TextFormat, Ui, pos2, vec2};
use glam::DVec3;
use serde_json::json;
use shapecraft_doc::Pick;

use super::Workspace;
use super::tools::Tool;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

/// The box being dragged: where it started and where the pointer is.
#[derive(Clone, Copy, Debug)]
pub struct AreaSel {
    pub start: Pos2,
    pub end: Pos2,
}

impl AreaSel {
    pub fn rect(&self) -> Rect {
        Rect::from_two_pos(self.start, self.end)
    }

    /// Dragged leftwards: take whatever the box crosses.
    pub fn crossing(&self) -> bool {
        self.end.x < self.start.x
    }
}

/// The filter row: label and its key.
pub const FILTERS: [(&str, &str); 4] = [("All Items", ""), ("Bodies", "B"), ("Faces", "F"), ("Edges", "E")];

/// The filter row while a sketch is open: its curves, their points, or its constraints; no faces, so a box
/// in a sketch never takes its regions (they're taken from outside it, with Faces).
pub const SKETCH_FILTERS: [(&str, &str); 4] = [("All Items", ""), ("Edges", "E"), ("Points", "P"), ("Constraints", "C")];

fn cyan() -> Color32 {
    theme::c(0x0dabd3)
}
fn purple() -> Color32 {
    theme::c(0xa855f7)
}

impl Workspace {
    /// Starts, grows and finishes an area selection. Runs after the tools had their chance at the drag.
    /// Whether a left-drag that grabs nothing draws a box now (else it's the tool's, or navigation's).
    pub(super) fn area_open(&self) -> bool {
        matches!(
            self.tool,
            Tool::Idle | Tool::Move(_) | Tool::Extrude(_) | Tool::Fillet(_) | Tool::Shell(_) | Tool::OffsetFace(_) | Tool::Scale(_)
        ) && self.settings.preset().action(crate::settings::Button::Left, false, false, false).is_none()
    }

    pub(super) fn area_drags(&mut self, ui: &Ui, resp: &egui::Response) {
        if self.area.is_none()
            && self.area_open()
            && !self.touch_drag
            && (!self.pen_input || self.pen_held.is_some())
            && resp.drag_started_by(egui::PointerButton::Primary)
            && !self.dragging_handle()
            && let Some(p) = self.pen_held.filter(|_| self.pen_input).or_else(|| ui.input(|i| i.pointer.press_origin()))
        {
            self.area = Some(AreaSel { start: p, end: p });
            // A filter picked in a sketch means something else outside it (and the other way round).
            let in_sketch = self.session.active_sketch.is_some();
            if self.area_filter_in_sketch != in_sketch {
                self.area_filter = 0;
                self.area_filter_in_sketch = in_sketch;
            }
        }
        let Some(mut a) = self.area else { return };
        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
            a.end = p;
            self.area = Some(a);
        }
        if resp.drag_stopped() {
            self.area = None;
            let picks = self.area_picks(&a);
            let shift = ui.input(|i| i.modifiers.shift);
            if a.rect().width() > 3.0 || a.rect().height() > 3.0 {
                // What was running gives way to what the new selection suggests.
                self.tool = Tool::Idle;
                self.live_step = None;
                let picks: Vec<serde_json::Value> = picks.iter().filter_map(|p| serde_json::to_value(p).ok()).collect();
                let _ = self.run("select", json!({ "picks": picks, "add": shift }));
                self.predict_tool();
            }
        }
    }

    /// The filter row for where the box is: the sketch being edited has its own.
    pub(super) fn area_filters(&self) -> &'static [(&'static str, &'static str); 4] {
        if self.session.active_sketch.is_some() { &SKETCH_FILTERS } else { &FILTERS }
    }

    /// A filter's key pressed while a box is dragged (B / F / E outside a sketch, E / P / C in one).
    pub(super) fn area_filter_key(&mut self, key: &str) {
        if let Some(i) = self.area_filters().iter().position(|(_, k)| *k == key) {
            self.area_filter = i;
            self.area_filter_in_sketch = self.session.active_sketch.is_some();
        }
    }

    /// What the box takes, with the current filter.
    pub(super) fn area_picks(&self, a: &AreaSel) -> Vec<Pick> {
        if let Some(sk) = self.session.active_sketch() {
            return self.area_sketch_picks(a, sk);
        }
        let r = a.rect();
        let crossing = a.crossing();
        let filter = self.area_filter;
        let doc = &self.session.doc;
        let screen = |p: DVec3| self.to_screen(p);
        // A polyline (screen points; a point that can't be shown counts as outside) against the box.
        let takes = |pts: &[Option<Pos2>]| -> bool {
            if pts.is_empty() {
                return false;
            }
            if !crossing {
                return pts.iter().all(|p| p.is_some_and(|p| r.contains(p)));
            }
            pts.iter().any(|p| p.is_some_and(|p| r.contains(p)))
                || pts.windows(2).any(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if segment_hits_rect(a, b, r)))
        };
        // The box's middle over a triangle: a crossing box inside one big face still takes it.
        let under_center = |tri: &[Option<Pos2>; 3]| matches!(tri, [Some(a), Some(b), Some(c)] if in_triangle(r.center(), *a, *b, *c));
        let v3 = |p: &[f32; 3]| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        let mut out = Vec::new();
        for body in doc.bodies.iter().filter(|b| b.visible) {
            let m = &body.mesh;
            let edges: Vec<Vec<Option<Pos2>>> = m.edges.iter().map(|l| l.iter().map(|p| screen(v3(p))).collect()).collect();
            let tris: Vec<[Option<Pos2>; 3]> = m
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| [0, 1, 2].map(|k| m.positions.get(t[k] as usize).and_then(|p| screen(v3(p)))))
                .collect();
            match filter {
                0 | 1 => {
                    // A mesh body has no edge lines: its triangles' corners stand in for them.
                    let all: Vec<Option<Pos2>> =
                        if body.is_mesh() { tris.iter().flatten().copied().collect() } else { edges.iter().flatten().copied().collect() };
                    let hit = if crossing { edges.iter().any(|e| takes(e)) || tris.iter().any(under_center) } else { takes(&all) };
                    if hit {
                        out.push(Pick::Body { body: body.id });
                    }
                }
                // A mesh body has no faces to pick.
                2 if body.is_mesh() => {}
                2 => {
                    let mut faces: std::collections::BTreeMap<u32, (bool, bool)> = std::collections::BTreeMap::new();
                    for (t, tri) in tris.iter().enumerate() {
                        let face = m.triangle_faces.get(t).copied().unwrap_or(0);
                        let inside = tri.iter().all(|p| p.is_some_and(|p| r.contains(p)));
                        let touched = takes(&[tri[0], tri[1], tri[2], tri[0]]) || under_center(tri);
                        let e = faces.entry(face).or_insert((true, false));
                        e.0 &= inside;
                        e.1 |= touched;
                    }
                    for (face, (all_in, any)) in faces {
                        if if crossing { any } else { all_in } {
                            out.push(Pick::Face { body: body.id, face });
                        }
                    }
                }
                _ => {
                    for (e, &edge) in edges.iter().zip(&m.edge_ids) {
                        if takes(e) {
                            out.push(Pick::Edge { body: body.id, edge });
                        }
                    }
                }
            }
        }
        // Sketch curves count as edges and their regions as faces (the reference reported "4 edges, 2 faces &
        // 1 body" for a box over its sketch).
        for sk in doc.sketches.iter().filter(|s| s.visible || self.session.active_sketch == Some(s.id)) {
            if matches!(filter, 0 | 3) {
                for (i, e) in sk.entities.iter().enumerate() {
                    let pts: Vec<Option<Pos2>> = e.outline().iter().map(|p| screen(sk.plane.to_world(*p))).collect();
                    if takes(&pts) {
                        out.push(Pick::SketchEntity { sketch: sk.id, entity: i as u32 });
                    }
                }
            }
            if matches!(filter, 0 | 2) {
                for region in sk.regions() {
                    let pts: Vec<Option<Pos2>> = region.triangles.iter().flatten().map(|p| screen(sk.plane.to_world(*p))).collect();
                    let hit = if crossing {
                        takes(&pts)
                            || region.triangles.iter().any(|t| {
                                let s = t.map(|p| screen(sk.plane.to_world(p)));
                                under_center(&s)
                            })
                    } else {
                        takes(&pts)
                    };
                    if hit {
                        out.push(Pick::Region { sketch: sk.id, region: region.index });
                    }
                }
            }
        }
        if filter == 0 {
            for pl in doc.planes.iter().filter(|p| p.visible) {
                let h = pl.size / 2.0;
                let corners: Vec<Option<Pos2>> = [(-h, -h), (h, -h), (h, h), (-h, h), (-h, -h)]
                    .iter()
                    .map(|&(x, y)| screen(pl.plane.origin + pl.plane.x * x + pl.plane.y * y))
                    .collect();
                if takes(&corners) {
                    out.push(Pick::Plane { plane: pl.id });
                }
            }
        }
        out
    }

    /// In the sketch being edited: its curves (All Items, Edges) or their key points (Points);
    /// Constraints takes nothing yet (constraint badges can't be picked).
    fn area_sketch_picks(&self, a: &AreaSel, sk: &shapecraft_doc::Sketch) -> Vec<Pick> {
        let r = a.rect();
        let crossing = a.crossing();
        let screen = |p: glam::DVec2| self.to_screen(sk.plane.to_world(p));
        let mut out = Vec::new();
        match self.area_filter {
            0 | 1 => {
                for (i, e) in sk.entities.iter().enumerate() {
                    let pts: Vec<Option<Pos2>> = e.outline().iter().map(|p| screen(*p)).collect();
                    let hit = if crossing {
                        pts.iter().any(|p| p.is_some_and(|p| r.contains(p)))
                            || pts.windows(2).any(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if segment_hits_rect(a, b, r)))
                    } else {
                        !pts.is_empty() && pts.iter().all(|p| p.is_some_and(|p| r.contains(p)))
                    };
                    if hit {
                        out.push(Pick::SketchEntity { sketch: sk.id, entity: i as u32 });
                    }
                }
            }
            2 => {
                for (entity, e) in sk.entities.iter().enumerate() {
                    if matches!(e, shapecraft_doc::SketchEntity::Text { .. }) {
                        continue;
                    }
                    for (index, q) in shapecraft_doc::constraints::points(e).into_iter().enumerate() {
                        if screen(q).is_some_and(|p| r.contains(p)) {
                            out.push(Pick::SketchPoint { sketch: sk.id, entity: entity as u32, index: index as u32 });
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// The box itself, over the view.
    pub(super) fn paint_area(&self, ui: &Ui) {
        let Some(a) = self.area else { return };
        let painter = ui.painter_at(self.view_rect);
        let r = a.rect();
        if a.crossing() {
            painter.rect_filled(r, 0.0, purple().gamma_multiply(0.1));
            let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.add(egui::Shape::dashed_line(&c, Stroke::new(1.2, purple()), 6.0, 4.0));
        } else {
            painter.rect_filled(r, 0.0, cyan().gamma_multiply(0.08));
            painter.rect_stroke(r, 0.0, Stroke::new(1.2, cyan()), StrokeKind::Inside);
        }
    }

    /// On a touch screen (owner's iPad screenshot): the banner shows from the pencil's long press, two lines with a
    /// small icon each and no Tab line, and a dot half purple, half cyan marks the press until the drag says which.
    fn paint_area_banner_touch(&self, ui: &Ui) {
        let held = self.pen_input && self.area_open() && self.pen_held.is_some();
        if self.area.is_none() && !held {
            return;
        }
        let vp = self.viewport_rect;
        let p = ui.painter();
        let r = Rect::from_min_size(pos2(vp.center().x - 155.0, self.banner_y()), vec2(310.0, 92.0));
        p.rect_filled(r.translate(vec2(0.0, 4.0)), radius(14), Color32::from_black_alpha(60));
        p.rect_filled(r, radius(14), Tokens::banner());
        p.text(r.left_top() + vec2(12.0, 21.0), Align2::LEFT_CENTER, "Area Selection", theme::strong(16.0), Tokens::text_strong());
        let fmt = |font: FontId, color: Color32| TextFormat { font_id: font, color, ..Default::default() };
        for (i, (word, col)) in [("fully included", cyan()), ("crossing", purple())].into_iter().enumerate() {
            let y = r.top() + 45.0 + i as f32 * 24.0;
            let icon = Rect::from_center_size(pos2(r.left() + 20.0, y), vec2(14.0, 14.0));
            if i == 0 {
                p.rect_stroke(icon, 1.0, Stroke::new(1.2, col), StrokeKind::Inside);
            } else {
                let c = [icon.left_top(), icon.right_top(), icon.right_bottom(), icon.left_bottom(), icon.left_top()];
                p.add(egui::Shape::dashed_line(&c, Stroke::new(1.2, col), 3.0, 2.0));
            }
            let mut job = LayoutJob::default();
            let (lead, _) = if i == 0 { ("Drag right to select ", ()) } else { ("Drag left to select ", ()) };
            job.append(lead, 0.0, fmt(theme::body(14.0), Tokens::text()));
            job.append(word, 0.0, fmt(theme::strong(14.0), col));
            job.append(" items", 0.0, fmt(theme::body(14.0), Tokens::text()));
            let g = ui.fonts_mut(|f| f.layout_job(job));
            p.galley(pos2(r.left() + 34.0, y - g.size().y / 2.0), g, Tokens::text());
        }
        if self.area.is_none()
            && let Some(at) = self.pen_held
        {
            let painter = ui.painter_at(self.view_rect);
            let rad = 13.0;
            let half = |sign: f32| -> Vec<Pos2> {
                let mut pts = vec![at];
                for k in 0..=16 {
                    let a = std::f32::consts::FRAC_PI_2 + sign * std::f32::consts::PI * k as f32 / 16.0;
                    pts.push(at + vec2(a.cos(), -a.sin()) * rad);
                }
                pts
            };
            painter.add(egui::Shape::convex_polygon(half(1.0), purple(), Stroke::NONE));
            painter.add(egui::Shape::convex_polygon(half(-1.0), cyan().gamma_multiply(0.6), Stroke::NONE));
        }
    }

    /// The banner while a box is being dragged, in place of the tool's.
    pub(super) fn paint_area_banner(&self, ui: &Ui) {
        if crate::tablet::touch_ui() {
            self.paint_area_banner_touch(ui);
            return;
        }
        if self.area.is_none() {
            return;
        }
        let vp = self.viewport_rect;
        let p = ui.painter();
        let r = Rect::from_min_size(pos2(vp.center().x - 250.0, vp.top() + 8.0), vec2(500.0, 102.0));
        p.rect_filled(r, radius(8), Tokens::banner());
        p.rect_stroke(r, radius(8), Stroke::new(1.0, Tokens::chip_border()), StrokeKind::Inside);
        p.text(r.left_top() + vec2(12.0, 14.0), Align2::LEFT_CENTER, "Area Selection", theme::strong(13.0), Tokens::text_strong());
        let body = theme::body(12.5);
        let mut job = LayoutJob::default();
        let fmt = |font: FontId, color: Color32| TextFormat { font_id: font, color, ..Default::default() };
        job.append("Drag right to select ", 0.0, fmt(body.clone(), Tokens::text()));
        job.append("fully included", 0.0, fmt(theme::strong(12.5), cyan()));
        job.append(" items, or drag left to select ", 0.0, fmt(body.clone(), Tokens::text()));
        job.append("crossing", 0.0, fmt(theme::strong(12.5), purple()));
        job.append(".\nPress ", 0.0, fmt(body.clone(), Tokens::text()));
        job.append("⇥ (Tab)", 0.0, fmt(theme::strong(12.5), Tokens::text_strong()));
        job.append(" to filter the selection.", 0.0, fmt(body, Tokens::text()));
        let g = ui.fonts_mut(|f| f.layout_job(job));
        p.galley(r.left_top() + vec2(12.0, 26.0), g, Tokens::text());
        // The filter row: a dark strip with the chosen filter raised.
        let row = Rect::from_min_size(r.left_bottom() + vec2(8.0, -30.0), vec2(r.width() - 16.0, 24.0));
        p.rect_filled(row, radius(6), theme::c(0x1f1f21));
        let filters = self.area_filters();
        let w = row.width() / filters.len() as f32;
        let icons = if self.session.active_sketch.is_some() {
            [Icon::DeselectAll, Icon::OffsetEdge, Icon::CCoincident, Icon::CParallel]
        } else {
            [Icon::DeselectAll, Icon::Body, Icon::OffsetFace, Icon::Fillet]
        };
        for (i, ((label, key), icon)) in filters.iter().zip(icons).enumerate() {
            let cell = Rect::from_min_size(row.min + vec2(w * i as f32, 0.0), vec2(w, row.height())).shrink(2.0);
            if i == self.area_filter {
                p.rect_filled(cell, radius(5), theme::c(0x3a3a3c));
            }
            icons::paint(p, Rect::from_center_size(pos2(cell.left() + 14.0, cell.center().y), vec2(12.0, 12.0)), icon, Tokens::icon());
            let font = if i == self.area_filter { theme::strong(11.5) } else { theme::body(11.5) };
            p.text(pos2(cell.left() + 28.0, cell.center().y), Align2::LEFT_CENTER, *label, font, Tokens::text_strong());
            if !key.is_empty() {
                p.text(pos2(cell.right() - 10.0, cell.center().y), Align2::RIGHT_CENTER, *key, theme::body(10.0), Tokens::text_dim());
            }
        }
    }
}

/// Whether the segment a–b crosses the rectangle's outline or lies inside it.
fn segment_hits_rect(a: Pos2, b: Pos2, r: Rect) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    (0..4).any(|i| segments_cross(a, b, corners[i], corners[(i + 1) % 4]))
}

fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let cross = |o: Pos2, p: Pos2, q: Pos2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

fn in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let s = |o: Pos2, q: Pos2| (q.x - o.x) * (p.y - o.y) - (q.y - o.y) * (p.x - o.x);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// #19: in the sketch being edited the filters are All Items / Edges /
    /// Points / Constraints and no box takes its regions; out of the sketch, Faces (F) takes them.
    #[test]
    fn a_box_takes_regions_only_outside_the_sketch() {
        let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Area");
        let mut ws = Workspace::new(session, crate::settings::Settings::default());
        ws.session.execute("sketch", &json!({"plane": "xy"})).unwrap();
        ws.session.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
        ws.session.execute("line", &json!({"a": [5, 0], "b": [5, 10]})).unwrap();
        assert!(ws.session.active_sketch.is_some());
        ws.view_rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        ws.camera.fit(DVec3::splat(-1.0), DVec3::splat(11.0));
        let all = AreaSel { start: pos2(1.0, 1.0), end: pos2(799.0, 599.0) };
        let regions = |p: &[Pick]| p.iter().filter(|p| matches!(p, Pick::Region { .. })).count();
        let count = |p: &[Pick], f: fn(&Pick) -> bool| p.iter().filter(|p| f(p)).count();
        ws.area_filter_key("F");
        assert_eq!(ws.area_filter, 0, "F isn't a filter in a sketch");
        assert_eq!(regions(&ws.area_picks(&all)), 0);
        assert_eq!(count(&ws.area_picks(&all), |p| matches!(p, Pick::SketchEntity { .. })), 5);
        ws.area_filter_key("P");
        assert_eq!(ws.area_filter, 2);
        let points = ws.area_picks(&all);
        assert!(!points.is_empty() && points.iter().all(|p| matches!(p, Pick::SketchPoint { .. })));
        ws.area_filter_key("C");
        assert!(ws.area_picks(&all).is_empty());
        ws.session.execute("exit_sketching", &json!({})).unwrap();
        ws.area_filter_key("F");
        assert_eq!(ws.area_filter, 2);
        assert_eq!(regions(&ws.area_picks(&all)), 2, "both halves of the split square");
    }
}

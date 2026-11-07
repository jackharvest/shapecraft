//! Dimension labels, handles and the value box, drawn in screen space over the 3D view.

use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::{DVec2, DVec3};
use shapecraft_doc::{Pick, SketchEntity, SketchPlane, format_length};

use super::handles::SECTION_FLIP;
use super::paint::{
    DASHED_RADIUS, DIAMETER, PILL_INK, RADIAL, axis_pill, copy_pill, dashed_radius, diameter_dim, double_arrow, face_arrow, lead_drop,
    leader_dim, link_pill, padlock, pill, radius_dim, screen_dim, sketch_dimension, target_badge, value_box, value_field,
};
use super::snap::ellipse_minor;
use crate::theme::{self, Tokens, radius};
use crate::workspace::Workspace;
use crate::workspace::tools::{Handle, Tool};

impl Workspace {
    /// Dimension labels, handles and the value box, drawn in screen space over the 3D view.
    pub(super) fn draw_annotations(&self, ui: &Ui) {
        let painter = ui.painter_at(self.view_rect);
        super::paint::clear_value_pills(ui.ctx(), self.numeric_fresh && self.numeric.is_some());
        let units = self.session.doc.units;
        let typed = self.numeric.as_deref();
        let plane = self.sketch_plane();
        let cursor = self.sketch_cursor();
        match (&self.tool, plane, cursor) {
            (Tool::Rectangle { first: Some(f) }, Some(pl), Some(c)) => {
                let c = self.rect_cursor(*f, c);
                let a = self.rect_corner(*f, c);
                let min = a.min(c);
                let max = a.max(c);
                let (Some(tl), Some(tr), Some(br)) = (
                    self.to_screen(pl.to_world(DVec2::new(min.x, max.y))),
                    self.to_screen(pl.to_world(max)),
                    self.to_screen(pl.to_world(DVec2::new(max.x, min.y))),
                ) else {
                    return;
                };
                // The width is the live field (it takes typed values); after Tab it's fixed and the height is live.
                let (w, h) = (format_length(max.x - min.x, units), format_length(max.y - min.y, units));
                // 26.200 (s11-053): ~70 px out, the live one boxed (typing opens the field), the other plain.
                let width_live = self.rect_width.is_none();
                screen_dim(&painter, tl, tr, 70.0, &w, typed.filter(|_| width_live), width_live);
                screen_dim(&painter, tr, br, 70.0, &h, typed.filter(|_| !width_live), !width_live);
            }
            // As observed: a circle being drawn is measured across, "Ø 20 mm" on a line through the center, and a
            // typed value is that diameter (unless Preferences ask for radius everywhere).
            (Tool::Circle { center: Some(ctr) }, Some(pl), Some(c)) if self.settings.circular_annotations != "Always Radius" => {
                let opposite = *ctr * 2.0 - c;
                if let (Some(a), Some(b)) = (self.to_screen(pl.to_world(opposite)), self.to_screen(pl.to_world(c))) {
                    let text = format!("Ø {}", format_length(ctr.distance(c) * 2.0, units));
                    diameter_dim(&painter, a, b, &text, typed, true);
                }
            }
            // A polygon (or a circle with radius labels) is measured from the center to the cursor: "R 20 mm".
            // A polygon's R is a leader pointing in at the corner under the pointer, its value boxed out past it
            // (s11-065).
            (Tool::Polygon { center: Some(ctr) }, Some(pl), Some(c)) => {
                if let (Some(a), Some(b)) = (self.to_screen(pl.to_world(*ctr)), self.to_screen(pl.to_world(c))) {
                    let text = format!("R {}", format_length(ctr.distance(c), units));
                    leader_dim(&painter, a, b, &text, typed);
                }
            }
            (Tool::Circle { center: Some(ctr) }, Some(pl), Some(c)) => {
                if let (Some(a), Some(b)) = (self.to_screen(pl.to_world(*ctr)), self.to_screen(pl.to_world(c))) {
                    let text = format!("R {}", format_length(ctr.distance(c), units));
                    radius_dim(&painter, a, b, &text, typed, true);
                }
            }
            (Tool::Ellipse { center: Some(ctr), major }, Some(pl), Some(c)) => {
                let s = |p: DVec2| self.to_screen(pl.to_world(p));
                match major {
                    // The first axis: a dimension beside the line, the live field.
                    None => {
                        if let (Some(a), Some(b)) = (s(*ctr), s(c)) {
                            // Like a line's: on the left of center→pointer, boxed while live (s11-062).
                            let text = format_length(ctr.distance(c), units);
                            let d = b - a;
                            if d.length() > 2.0 {
                                let n = vec2(d.y, -d.x).normalized() * 45.0;
                                sketch_dimension(&painter, [a, b], [a + n, b + n], &text, typed, true);
                            }
                        }
                    }
                    // Then both half-axes as radii; the second one is live.
                    Some(m) => {
                        let ry = ellipse_minor(*ctr, *m, c);
                        let v = (*m - *ctr).normalize_or_zero().perp();
                        let rim = *ctr + v * ry * if (c - *ctr).dot(v) < 0.0 { -1.0 } else { 1.0 };
                        if let (Some(a), Some(b), Some(r)) = (s(*ctr), s(*m), s(rim)) {
                            radius_dim(&painter, a, b, &format!("R {}", format_length(ctr.distance(*m), units)), None, false);
                            let text = format!("R {}", format_length(ry, units));
                            radius_dim(&painter, a, r, &text, typed, true);
                        }
                    }
                }
            }
            // 26.200 (s11-041/042): the sweep angle is the live value, on a dimension arc outside the curve; the
            // radius shows too (dashed spokes to both ends while the end is still moving).
            (Tool::Arc { a: Some(a), b }, Some(pl), Some(c)) => {
                if let Some((p, mid, q)) = self.arc_preview(*a, *b, c) {
                    self.paint_arc_dims(&painter, &pl, p, mid, q, b.is_none(), typed, true);
                }
            }
            // 26.200: an offset dimension lying in the sketch plane, its label in a black box on the dimension line
            // turned along it; typing swaps the box for the value field (s11-009, s11-014).
            (Tool::Line { last: Some(l), .. }, Some(pl), Some(c)) if l.distance(c) > 1e-9 => {
                let c = self.line_end(*l, c);
                let side = (c - *l).normalize().perp();
                let k = self.camera.world_per_pixel(self.size().y) * 45.0;
                // On the left of the drawing direction as seen on screen (s11-009, s11-030).
                let toward = |s: f64| match (
                    self.to_screen(pl.to_world(*l)),
                    self.to_screen(pl.to_world(c)),
                    self.to_screen(pl.to_world((*l + c) * 0.5 + side * k * s)),
                ) {
                    (Some(a), Some(b), Some(m)) => {
                        let d = b - a;
                        (m - (a + d * 0.5)).dot(vec2(d.y, -d.x))
                    }
                    _ => 0.0,
                };
                let off = side * k * if toward(1.0) >= toward(-1.0) { 1.0 } else { -1.0 };
                let s = |p: DVec2| self.to_screen(pl.to_world(p));
                if let (Some(a), Some(b), Some(a2), Some(b2)) = (s(*l), s(c), s(*l + off), s(c + off)) {
                    sketch_dimension(&painter, [a, b], [a2, b2], &format_length(l.distance(c), units), typed, true);
                    // A fixed length wears a padlock beside its label (s11-035).
                    if self.line_len.is_some() && typed.is_none() {
                        let m = a2 + (b2 - a2) * 0.5;
                        let u = (b2 - a2).normalized();
                        let u = if u.x < 0.0 { -u } else { u };
                        let w = format_length(l.distance(c), units).len() as f32 * 3.6 + 16.0;
                        padlock(&painter, m + u * w);
                    }
                }
            }
            _ => {}
        }
        let handle = self.arrow_handle();
        if let Tool::Move(t) = &self.tool {
            // As observed: white handles with a grey rim, blue under the pointer or while dragged.
            let pivot = t.shown_pivot();
            let hover = self.cursor.and_then(|c| self.gizmo_hit(pivot, t.frame, c));
            // The handle that moved the items stays blue while its step is live, as observed.
            let moved = t.dragging.is_some() || t.value.abs() > 1e-9 || t.planar.length() > 1e-9;
            let rim = theme::c(0x6e6e73);
            for (h, pts) in self.gizmo_shapes(pivot, t.frame) {
                let hot = hover == Some(h) || t.handle == h && moved;
                let fill = if hot { Tokens::accent() } else { theme::c(0xf4f4f6) };
                match (h, pts.as_slice()) {
                    (Handle::Arrow(_), [a, b]) => {
                        let dir = (*b - *a).normalized();
                        let perp = vec2(-dir.y, dir.x);
                        let neck = *b - dir * 10.0;
                        painter.line_segment([*a, neck], Stroke::new(7.0, rim));
                        painter.line_segment([*a, neck], Stroke::new(4.8, fill));
                        let head = vec![*b + dir * 2.0, neck + perp * 8.0, neck - perp * 8.0];
                        painter.add(egui::Shape::convex_polygon(head, fill, Stroke::new(1.2, rim)));
                    }
                    (Handle::Ring(_), _) if pts.len() >= 2 => {
                        painter.add(egui::Shape::line(pts.clone(), Stroke::new(5.5, rim)));
                        painter.add(egui::Shape::line(pts.clone(), Stroke::new(3.2, fill)));
                        for (tip, from) in [(pts[0], pts[1]), (pts[pts.len() - 1], pts[pts.len() - 2])] {
                            let dir = (tip - from).normalized();
                            let perp = vec2(-dir.y, dir.x);
                            painter.add(egui::Shape::convex_polygon(
                                vec![tip + dir * 6.0, tip + perp * 4.5, tip - perp * 4.5],
                                fill,
                                Stroke::new(1.0, rim),
                            ));
                        }
                    }
                    (Handle::Plane(_), _) if pts.len() == 5 => {
                        painter.add(egui::Shape::convex_polygon(pts[..4].to_vec(), fill, Stroke::new(1.2, rim)));
                    }
                    _ => {}
                }
            }
            // As observed: once moved, a dashed line runs back to where the move started, marked by a small
            // axis triad there.
            if let (Some(c), Some(s0)) = (self.to_screen(pivot), self.to_screen(t.pivot))
                && (c - s0).length() > 2.0
            {
                painter.add(egui::Shape::dashed_line(&[s0, c], Stroke::new(1.3, Color32::from_white_alpha(210)), 5.0, 4.0));
                let colors = [theme::c(0xe5534b), theme::c(0x57ab5a), theme::c(0x4c8dff)];
                for (axis, col) in t.frame.axes.iter().zip(colors) {
                    if let Some(e) = self.to_screen(t.pivot + *axis * self.camera.world_per_pixel(self.size().y))
                        && (e - s0).length() > 1e-3
                    {
                        painter.line_segment([s0, s0 + (e - s0).normalized() * 40.0], Stroke::new(1.2, col));
                    }
                }
            }
            let active = t.value.abs() > 1e-9 || t.dragging.is_some() || typed.is_some();
            let center = self.to_screen(pivot);
            let arrow_tip = match t.handle {
                Handle::Arrow(a) if active => {
                    self.gizmo_shapes(pivot, t.frame).into_iter().find_map(|(h, pts)| (h == Handle::Arrow(a)).then_some(pts)).and_then(
                        |pts| match pts.as_slice() {
                            [a, b] => Some((*b, (*b - *a).normalized())),
                            _ => None,
                        },
                    )
                }
                // Turning, as observed (302): the pills run outward from the center through the dragged handle.
                Handle::Ring(a) if active => self
                    .gizmo_shapes(pivot, t.frame)
                    .into_iter()
                    .find_map(|(h, pts)| (h == Handle::Ring(a)).then_some(pts))
                    .and_then(|pts| Some((*pts.get(pts.len() / 2)?, center?)))
                    .map(|(mid, c)| (mid + (mid - c).normalized() * 38.0, (mid - c).normalized())),
                _ => None,
            };
            // Turning also shows the whole ring, large and dashed, in the plane it turns in.
            if let (Handle::Ring(a), true) = (t.handle, active) {
                let n = t.frame.axes[a.min(2)];
                let u = n.any_orthonormal_vector();
                let v = n.cross(u);
                let r = self.camera.world_per_pixel(self.size().y) * 270.0;
                let pts: Vec<Pos2> = (0..=96)
                    .filter_map(|i| {
                        let t = i as f64 / 96.0 * std::f64::consts::TAU;
                        self.to_screen(pivot + (u * t.cos() + v * t.sin()) * r)
                    })
                    .collect();
                for w in pts.chunks(2).filter(|w| w.len() == 2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(1.6, Color32::from_white_alpha(200)));
                }
            }
            if let Some(c) = center {
                painter.circle_filled(c, 7.0, theme::c(0xf4f4f6));
                painter.circle_stroke(c, 7.0, Stroke::new(1.0, rim));
                painter.circle_filled(c, 2.5, if t.snapped { theme::c(0x8b5cf6) } else { Tokens::accent() });
                if let Some((tip, dir)) = arrow_tip {
                    // Moving along an arrow: the value capsule past its tip, then Copy, both along the axis.
                    let text = typed.map(str::to_string).unwrap_or_else(|| match t.handle {
                        Handle::Ring(_) => format!("{}°", t.value),
                        _ => format_length(t.value, units),
                    });
                    let r = axis_pill(&painter, tip + dir * 30.0, dir, &text, true);
                    if !t.items.is_empty() {
                        let step = r.width().max(r.height()) / 2.0 + 26.0;
                        let pr = axis_pill(&painter, tip + dir * (30.0 + step), dir, "Copy", t.copy);
                        ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("move-copy-pill"), pr));
                        // The Link badge rides along the arrow after Copy (s29-023).
                        if t.copy {
                            let lr = link_pill(&painter, tip + dir * (30.0 + step + pr.width().max(pr.height()) / 2.0 + 14.0), !t.unlinked);
                            ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("move-link-pill"), lr));
                        }
                    }
                } else if !t.items.is_empty() {
                    let at = c + vec2(0.0, 64.0);
                    copy_pill(&painter, at, t.copy);
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(ui.scope_id().with("move-copy-pill"), Rect::from_center_size(at, vec2(46.0, 20.0))));
                    // Copy lit: the Link badge right of it (s29-019).
                    if t.copy {
                        let lr = link_pill(&painter, at + vec2(34.0, 0.0), !t.unlinked);
                        ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("move-link-pill"), lr));
                    }
                }
                if arrow_tip.is_none() && (t.dragging.is_some() || typed.is_some()) {
                    let text = match t.handle {
                        Handle::Plane(_) => format_length(t.planar.length(), units),
                        Handle::Ring(_) => format!("{}°", t.value),
                        Handle::Arrow(_) => format_length(t.value, units),
                    };
                    value_box(&painter, c + vec2(80.0, -70.0), &text, typed);
                }
            }
        }
        if let Tool::RotateAxis(t) = &self.tool
            && let (Some((_, dir)), Some(f)) = (t.axis, t.foot())
        {
            // As observed (340–342): the axis as a long purple line, a dashed radius from it to the handle, and
            // once turned a dashed arc from the start (white dot) round to the handle; the angle capsule and Copy
            // run outward from the axis through the handle.
            let shown = typed.and_then(|n| n.trim().parse::<f64>().ok()).filter(|a| a.is_finite()).unwrap_or(t.angle);
            // Long, but not so long an end falls behind the camera.
            let wpp = self.camera.world_per_pixel(self.size().y);
            let ends = |k: f64| self.to_screen(f + dir * wpp * k);
            let up = [4000.0, 2000.0, 1000.0, 400.0].into_iter().find_map(&ends);
            let down = [4000.0, 2000.0, 1000.0, 400.0].into_iter().find_map(|k| ends(-k));
            if let (Some(a), Some(b)) = (down, up) {
                painter.line_segment([a, b], Stroke::new(1.4, theme::c(0x8b5cf6)));
            }
            let ink = Color32::from_white_alpha(225);
            if let (Some(fs), Some(start)) = (self.to_screen(f), self.to_screen(t.center)) {
                let handle = t.handle_at(shown).and_then(|h| self.to_screen(h)).unwrap_or(start);
                painter.add(egui::Shape::dashed_line(&[fs, handle], Stroke::new(1.3, ink), 6.0, 5.0));
                if shown.abs() > 1e-9 {
                    painter.add(egui::Shape::dashed_line(&[fs, start], Stroke::new(1.3, ink), 6.0, 5.0));
                    let arc: Vec<Pos2> =
                        (0..=32).filter_map(|i| t.handle_at(shown * i as f64 / 32.0).and_then(|h| self.to_screen(h))).collect();
                    painter.add(egui::Shape::dashed_line(&arc, Stroke::new(1.3, ink), 4.0, 4.0));
                    painter.circle_filled(start, 4.0, Color32::WHITE);
                }
                // The handle: a short blue double arrow across the radius.
                let out = (handle - fs).normalized();
                let across = vec2(-out.y, out.x);
                double_arrow(&painter, handle, across, 9.0, Tokens::accent(), theme::c(0x6e6e73));
                // At rest the angle sits along the axis, above the handle; once turned, outward past it (340–341).
                let along = match (self.to_screen(t.center), self.to_screen(t.center + dir)) {
                    (Some(a), Some(b)) if (b - a).length() > 1e-3 => {
                        let v = (b - a).normalized();
                        if v.y > 0.0 { -v } else { v }
                    }
                    _ => vec2(0.0, -1.0),
                };
                let cap = if shown.abs() > 1e-9 {
                    axis_pill(&painter, handle + out * 34.0, out, &format!("{shown}°"), true)
                } else {
                    axis_pill(&painter, handle + along * 34.0, along, &format!("{shown}°"), true)
                };
                let step = cap.width().max(cap.height()) / 2.0 + 30.0;
                let pr = if shown.abs() > 1e-9 {
                    axis_pill(&painter, handle + out * (34.0 + step), out, "Copy", t.copy)
                } else {
                    // Before turning, Copy waits beside the handle (340).
                    copy_pill(&painter, handle + vec2(-38.0, 0.0), t.copy)
                };
                ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("rotaxis-copy-pill"), pr));
                // The Link badge after a lit Copy, as on Move's gizmo (s29-019).
                if t.copy {
                    let at = if shown.abs() > 1e-9 {
                        pr.center() + out * (pr.width().max(pr.height()) / 2.0 + 14.0)
                    } else {
                        pr.center() + vec2(pr.width() / 2.0 + 14.0, 0.0)
                    };
                    let lr = link_pill(&painter, at, !t.unlinked);
                    ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("rotaxis-link-pill"), lr));
                }
            }
        }
        if let Tool::BodyEdgeOffset(t) = &self.tool {
            // As observed (347–349): the edge orange, a ↔ into each face beside it (the active one blue), and once
            // pulled the copy as a blue line on the face, a dashed line back to the edge and the distance capsule.
            let d = typed.and_then(crate::workspace::tools::parse_length).unwrap_or(t.distance);
            let orange = theme::c(0xf0a030);
            let line: Vec<Pos2> = t.line.iter().filter_map(|p| self.to_screen(*p)).collect();
            painter.add(egui::Shape::line(line, Stroke::new(2.0, orange)));
            if d.abs() > 1e-9 {
                let moved: Vec<Pos2> = t.line.iter().filter_map(|p| self.to_screen(*p + t.dir() * d)).collect();
                painter.add(egui::Shape::line(moved, Stroke::new(2.5, Tokens::sketch_blue())));
            }
            let arrows = self.body_edge_arrows(t);
            for (i, (c, dir)) in arrows.iter().enumerate() {
                let fill = if i == t.active { Tokens::accent() } else { theme::c(0xf4f4f6) };
                double_arrow(&painter, *c, *dir, 10.0, fill, theme::c(0x6e6e73));
            }
            if let (Some(a), Some(b)) = (self.to_screen(t.anchor), self.to_screen(t.anchor + t.dir() * d)) {
                let text = typed.map(str::to_string).unwrap_or_else(|| format_length(d, units));
                if (b - a).length() > 40.0 {
                    painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.3, Color32::from_white_alpha(225)), 5.0, 4.0));
                    let dir = (b - a).normalized();
                    axis_pill(&painter, a + (b - a) * 0.5, dir, &text, true);
                } else if let Some((c, dir)) = arrows.get(t.active) {
                    // Before a pull the capsule waits out past the edge, away from the face.
                    axis_pill(&painter, *c - *dir * 70.0, -*dir, &text, true);
                }
            }
        }
        if let Tool::EdgePlane(t) = &self.tool
            && let Some(h) = self.edge_plane_handle(t)
        {
            if t.through {
                // As observed (345): a dashed line out from the edge to a small blue double arrow across it, and
                // the angle in a capsule past it.
                if let Some((mid, ..)) = self.edge_plane_frame(t)
                    && let Some(m) = self.to_screen(mid)
                {
                    let dir = (h - m).normalized();
                    painter.add(egui::Shape::dashed_line(&[m, h - dir * 10.0], Stroke::new(1.3, Color32::from_white_alpha(225)), 5.0, 4.0));
                    double_arrow(&painter, h, vec2(-dir.y, dir.x), 9.0, Tokens::accent(), theme::c(0x6e6e73));
                    let shown = typed.map(|n| format!("{n}°")).unwrap_or_else(|| format!("{}°", t.value));
                    axis_pill(&painter, h + dir * 30.0 + vec2(dir.y, -dir.x) * 10.0, dir, &shown, true);
                }
            } else {
                // The point (white dot in a dark ring with a soft halo), as observed (346).
                painter.circle_filled(h, 13.0, Color32::from_white_alpha(28));
                painter.circle_filled(h, 7.5, theme::c(0x2a2a2c));
                painter.circle_filled(h, 5.0, Color32::WHITE);
            }
        }
        if let Tool::Align { to: Some(to), .. } = &self.tool {
            // The Flip badge at the target face's middle (387).
            let p = shapecraft_engine::Params::new(to);
            let at = if let Ok(pl) = p.id("plane") {
                self.session.doc.plane(pl).map(|pl| pl.plane.origin)
            } else {
                p.id("body").ok().and_then(|b| self.session.doc.body(b)).and_then(|b| {
                    // An edge target: its middle.
                    if let Ok(e) = p.f64("edge") {
                        return match self.session.kernel.edge_curve(&b.shape, e as u32).ok()? {
                            shapecraft_kernel::EdgeCurve::Line { a, b } => Some((a + b) / 2.0),
                            _ => None,
                        };
                    }
                    let f = p.f64("face").ok()? as u32;
                    self.session.kernel.face_plane(&b.shape, f).ok().flatten().map(|fp| fp.origin)
                })
            };
            if let Some(c) = at.and_then(|a| self.to_screen(a)) {
                let c = c + vec2(0.0, 40.0);
                painter.circle_filled(c, 11.0, Color32::WHITE);
                crate::icons::paint_on(
                    &painter,
                    Rect::from_center_size(c, vec2(14.0, 14.0)),
                    crate::icons::Icon::Mirror,
                    PILL_INK,
                    Color32::WHITE,
                );
                ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("align-flip"), Rect::from_center_size(c, vec2(22.0, 22.0))));
            }
        }
        if let Tool::Onto(t) = &self.tool {
            // White badges at the click points: the tool's glyph in a blue ring on each face to replace, a target
            // in a purple ring on the replacing face (s11-123, s11-130).
            let badge = |at: DVec3, ring: Color32| {
                let c = self.to_screen(at)?;
                painter.circle_filled(c, 18.0, Color32::WHITE);
                painter.circle_stroke(c, 15.0, Stroke::new(1.6, ring));
                Some(c)
            };
            let glyph = if t.project { crate::icons::Icon::ProjectEdge } else { crate::icons::Icon::ReplaceFace };
            for (_, at) in &t.items {
                if let Some(c) = badge(*at, theme::c(0x03a7ff)) {
                    crate::icons::paint_on(&painter, Rect::from_center_size(c, vec2(16.0, 16.0)), glyph, PILL_INK, Color32::WHITE);
                }
            }
            if let Some((_, at)) = &t.to
                && let Some(c) = self.to_screen(*at)
            {
                target_badge(&painter, c);
            }
        }
        // Split Body's splitting tools: a target badge where each was clicked (s12-004, s12-014).
        if let Tool::Mirror(t) = &self.tool {
            for (_, at) in &t.with {
                if let Some(c) = self.to_screen(*at) {
                    target_badge(&painter, c);
                }
            }
        }
        if let Tool::Align { from: Some((fb, ff)), to: None, edge: false, .. } = &self.tool
            && let Some(b) = self.session.doc.body(*fb)
            && let Some(fp) = self.session.kernel.face_plane(&b.shape, *ff).ok().flatten()
        {
            // The picked face's snap points, purple (386): its corners and edge middles and its middle.
            let plane = SketchPlane::from_normal(fp.origin, fp.normal);
            if let Some(plane) = plane {
                for (q, ..) in self.body_points_on(&plane) {
                    if let Some(s) = self.to_screen(plane.to_world(q)) {
                        painter.circle_filled(s, 3.5, theme::c(0x8b5cf6));
                        painter.circle_stroke(s, 3.5, Stroke::new(1.0, Color32::WHITE));
                    }
                }
            }
        }
        if let Tool::Boolean(t) = &self.tool
            && t.op == "subtract"
        {
            // A "+" badge on what's cut from and a "−" on what's cut away, at their middles (383–384).
            for (ids, plus) in [(&t.targets, true), (&t.tools, false)] {
                for id in ids.iter() {
                    let Some(b) = self.session.doc.body(*id) else { continue };
                    let Ok(bb) = self.session.kernel.bounds(&b.shape) else { continue };
                    let Some(c) = self.to_screen((bb.min + bb.max) / 2.0) else { continue };
                    let r = Rect::from_center_size(c, vec2(30.0, 22.0));
                    painter.rect_filled(r, radius(11), Color32::WHITE);
                    let ring = if plus { theme::c(0x8b5cf6) } else { theme::c(0x1f9bff) };
                    painter.rect_stroke(r.shrink(2.0), radius(9), Stroke::new(1.6, ring), StrokeKind::Inside);
                    let ink = PILL_INK;
                    painter.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.6, ink));
                    if plus {
                        painter.line_segment([c - vec2(0.0, 5.0), c + vec2(0.0, 5.0)], Stroke::new(1.6, ink));
                    }
                }
            }
        }
        if let Tool::Mirror(t) = &self.tool
            && let Some(c) = self.to_screen(t.center)
        {
            // A badge at the items' middle (330, 332): white capsule, blue ring, the tool's glyph.
            let r = Rect::from_center_size(c, vec2(34.0, 24.0));
            painter.rect_filled(r, radius(12), Color32::WHITE);
            painter.rect_stroke(r.shrink(2.5), radius(10), Stroke::new(1.6, theme::c(0x1f9bff)), StrokeKind::Inside);
            let icon = if t.split { crate::icons::Icon::SplitBody } else { crate::icons::Icon::Mirror };
            crate::icons::paint_on(&painter, Rect::from_center_size(c, vec2(16.0, 16.0)), icon, PILL_INK, Color32::WHITE);
        }
        if let Tool::Translate { from, to, .. } = &self.tool {
            // As observed: a white dot in a dark ring with a soft halo on each picked point, and once both are in,
            // a dashed arrow between them with the distance in a capsule halfway.
            let marker = |p: Pos2| {
                painter.circle_filled(p, 13.0, Color32::from_white_alpha(28));
                painter.circle_filled(p, 7.5, theme::c(0x2a2a2c));
                painter.circle_filled(p, 5.0, Color32::WHITE);
            };
            let (a, b) = (from.and_then(|p| self.to_screen(p)), to.and_then(|p| self.to_screen(p)));
            if let (Some(a), Some(b), Some(f), Some(t)) = (a, b, from, to)
                && (b - a).length() > 20.0
            {
                let dir = (b - a).normalized();
                let lp = vec2(-dir.y, dir.x);
                let (s0, s1) = (a + dir * 9.0, b - dir * 9.0);
                let ink = Color32::from_white_alpha(235);
                painter.add(egui::Shape::dashed_line(&[s0, s1 - dir * 10.0], Stroke::new(1.3, ink), 5.0, 4.0));
                painter.add(egui::Shape::convex_polygon(
                    vec![s1, s1 - dir * 13.0 + lp * 5.0, s1 - dir * 13.0 - lp * 5.0],
                    ink,
                    Stroke::NONE,
                ));
                axis_pill(&painter, a + (b - a) * 0.5, dir, &format_length(f.distance(*t), units), true);
            }
            for p in [a, b].into_iter().flatten() {
                marker(p);
            }
        }
        if let Tool::Scale(t) = &self.tool {
            let hover = self.cursor.and_then(|c| self.scale_hit(t, c));
            let typed_f = typed.and_then(|n| n.trim().parse::<f64>().ok());
            // Uniform shows its value from the start; the others once pulled (as observed).
            let active = t.uniform || t.dragging.is_some() || (t.factor - 1.0).abs() > 1e-9 || typed.is_some();
            let rim = theme::c(0x6e6e73);
            let shapes = self.scale_shapes(t);
            for (h, pts) in &shapes {
                let hot = hover == Some(*h) || t.handle == *h && active;
                let fill = if hot { Tokens::accent() } else { theme::c(0xf4f4f6) };
                match pts.as_slice() {
                    [a, b] => {
                        // A hammer: a stem and a square head.
                        let dir = (*b - *a).normalized();
                        let perp = vec2(-dir.y, dir.x);
                        let neck = *b - dir * 9.0;
                        painter.line_segment([*a, neck], Stroke::new(6.5, rim));
                        painter.line_segment([*a, neck], Stroke::new(4.3, fill));
                        let head = vec![neck + perp * 7.0, *b + perp * 7.0, *b - perp * 7.0, neck - perp * 7.0];
                        painter.add(egui::Shape::convex_polygon(head, fill, Stroke::new(1.2, rim)));
                    }
                    _ if pts.len() == 5 => {
                        painter.add(egui::Shape::convex_polygon(pts[..4].to_vec(), fill, Stroke::new(1.2, rim)));
                    }
                    _ => {}
                }
            }
            if let Some(c) = self.to_screen(t.pivot) {
                painter.circle_filled(c, 7.0, theme::c(0xf4f4f6));
                painter.circle_stroke(c, 7.0, Stroke::new(1.0, rim));
                painter.circle_filled(c, 2.5, if t.snapped { theme::c(0x8b5cf6) } else { Tokens::accent() });
                // The factor and Copy run outward through the handle; before that, Copy waits under the center.
                let through = shapes.iter().find(|(h, _)| *h == t.handle).and_then(|(_, pts)| {
                    let far = match pts.as_slice() {
                        [_, b] => *b,
                        _ => pts.iter().take(4).fold(Pos2::ZERO, |acc, q| acc + q.to_vec2() / 4.0),
                    };
                    let dir = (far - c).normalized();
                    (dir.length() > 0.5).then_some((far, dir))
                });
                match through {
                    Some((far, dir)) if active => {
                        let text = typed.map(|n| format!("{n}×")).unwrap_or_else(|| format!("{:.2}×", typed_f.unwrap_or(t.factor)));
                        let r = axis_pill(&painter, far + dir * 30.0, dir, &text, true);
                        let step = r.width().max(r.height()) / 2.0 + 26.0;
                        let pr = axis_pill(&painter, far + dir * (30.0 + step), dir, "Copy", t.copy);
                        ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("scale-copy-pill"), pr));
                        // Scale Mode's badge follows Copy (26.200).
                        let after = pr.width().max(pr.height()) / 2.0 + 17.0;
                        crate::workspace::badges::paint_row(self, ui, &painter, far + dir * (30.0 + step + after), dir);
                    }
                    _ => {
                        let at = c + vec2(0.0, 57.0);
                        copy_pill(&painter, at, t.copy);
                        crate::workspace::badges::paint_row(self, ui, &painter, at + vec2(36.0, 0.0), vec2(1.0, 0.0));
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(ui.scope_id().with("scale-copy-pill"), Rect::from_center_size(at, vec2(46.0, 20.0)))
                        });
                    }
                }
            }
        }
        // Section View's handles show while it's being edited (s16-021/022): the arrow, its value, and a round flip
        // button beyond them.
        if let Some(sec) = self.section.filter(|s| s.editing)
            && let Some((s0, sdir, _)) = self.handle_frame(sec.origin(), sec.normal)
        {
            let col = theme::c(0xf2c94c);
            let tip = s0 + sdir * 46.0;
            painter.line_segment([s0, tip], Stroke::new(3.0, col));
            let perp = vec2(-sdir.y, sdir.x);
            painter.add(egui::Shape::convex_polygon(vec![tip + sdir * 12.0, tip + perp * 7.0, tip - perp * 7.0], col, Stroke::NONE));
            painter.circle_filled(s0, 4.0, col);
            if sec.dragging.is_some() {
                value_box(&painter, tip + sdir * 24.0 + perp * 30.0, &format_length(sec.offset, units), None);
            } else {
                // A white pill with the offset (s16-021).
                let g = painter.layout_no_wrap(format_length(sec.offset, units), theme::strong(11.0), PILL_INK);
                let pill = Rect::from_center_size(s0 + sdir * 80.0, g.size() + vec2(12.0, 6.0));
                painter.rect_filled(pill, radius(8), Color32::WHITE);
                painter.rect_stroke(pill, radius(8), Stroke::new(1.5, Tokens::accent()), StrokeKind::Outside);
                painter.galley(pill.center() - g.size() / 2.0, g, PILL_INK);
            }
            // The two rotate handles: white discs with a curved arrow, out along the plane.
            if let Some((_, _, ppm)) = self.handle_frame(sec.origin(), sec.normal) {
                let reach = 90.0 / ppm.max(1e-9);
                let (u, v) = sec.axes();
                for a in [u, v] {
                    if let Some(h) = self.to_screen(sec.origin() + a * reach) {
                        painter.circle_filled(h, 9.0, Color32::WHITE);
                        painter.circle_stroke(h, 9.0, Stroke::new(1.0, theme::c(0x3a3a3c)));
                        let arc: Vec<Pos2> = (0..=12)
                            .map(|i| {
                                let t = std::f32::consts::PI * (0.15 + 1.2 * i as f32 / 12.0);
                                h + vec2(t.cos(), -t.sin()) * 4.5
                            })
                            .collect();
                        let end = arc[12];
                        painter.add(egui::Shape::line(arc, Stroke::new(1.4, PILL_INK)));
                        painter.add(egui::Shape::convex_polygon(
                            vec![end + vec2(-2.5, -1.0), end + vec2(2.5, -1.0), end + vec2(0.0, 2.8)],
                            PILL_INK,
                            Stroke::NONE,
                        ));
                    }
                }
            }
            let flip = s0 + sdir * SECTION_FLIP;
            painter.circle_filled(flip, 8.0, Color32::WHITE);
            painter.circle_stroke(flip, 8.0, Stroke::new(1.0, theme::c(0x3a3a3c)));
            for k in [-1.0f32, 1.0] {
                let a = flip + vec2(2.5 * k, 3.5 * k);
                painter.line_segment([a, flip + vec2(2.5 * k, -3.5 * k)], Stroke::new(1.3, PILL_INK));
                painter.add(egui::Shape::convex_polygon(
                    vec![flip + vec2(2.5 * k, -5.0 * k), flip + vec2(2.5 * k - 2.2, -2.5 * k), flip + vec2(2.5 * k + 2.2, -2.5 * k)],
                    PILL_INK,
                    Stroke::NONE,
                ));
            }
        }
        if let Tool::Pattern(t) = &self.tool {
            // As observed (324–326): plain white arrows (no rings or tabs). Pulling one moves it out to the spacing,
            // blue, with a dashed dimension back to the start, and the pills run on along the axis.
            let rim = theme::c(0x6e6e73);
            let spacing = typed.and_then(crate::workspace::tools::parse_length).unwrap_or(t.spacing);
            let pulled = spacing.abs() > 1e-9 || t.dragging.is_some();
            let hover = self.cursor.and_then(|c| self.gizmo_hit(t.pivot, t.frame, c));
            let shift = |a: usize| -> egui::Vec2 {
                match (self.to_screen(t.pivot), self.to_screen(t.pivot + t.frame.axes[a] * spacing)) {
                    (Some(c0), Some(c1)) if a == t.axis && pulled => c1 - c0,
                    _ => egui::Vec2::ZERO,
                }
            };
            let mut tip_dir = None;
            for (h, pts) in self.gizmo_shapes(t.pivot, t.frame) {
                let Handle::Arrow(a) = h else { continue };
                let [p0, p1] = pts.as_slice() else { continue };
                let (p0, p1) = (*p0 + shift(a), *p1 + shift(a));
                let hot = (a == t.axis && pulled) || hover == Some(h);
                let fill = if hot { Tokens::accent() } else { theme::c(0xf4f4f6) };
                let dir = (p1 - p0).normalized();
                let perp = vec2(-dir.y, dir.x);
                let neck = p1 - dir * 10.0;
                painter.line_segment([p0, neck], Stroke::new(7.0, rim));
                painter.line_segment([p0, neck], Stroke::new(4.8, fill));
                painter.add(egui::Shape::convex_polygon(
                    vec![p1 + dir * 2.0, neck + perp * 8.0, neck - perp * 8.0],
                    fill,
                    Stroke::new(1.2, rim),
                ));
                if a == t.axis && pulled {
                    tip_dir = Some((p1, dir, p0));
                }
            }
            if let Some(c) = self.to_screen(t.pivot) {
                painter.circle_filled(c, 7.0, theme::c(0xf4f4f6));
                painter.circle_stroke(c, 7.0, Stroke::new(1.0, rim));
                painter.circle_filled(c, 2.5, Tokens::accent());
                let quantity = format!("Quantity  {}×", t.count);
                let kind = if t.total { "Total ⌄" } else { "Spacing ⌄" };
                let value = typed.map(str::to_string).unwrap_or_else(|| format_length(spacing.abs(), units));
                match tip_dir {
                    Some((tip, dir, base)) => {
                        let ink = Color32::from_white_alpha(235);
                        let lp = vec2(-dir.y, dir.x);
                        let (a, b) = (c + dir * 10.0, base - dir * 6.0);
                        if (b - a).dot(dir) > 12.0 {
                            painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.3, ink), 5.0, 4.0));
                            for (q, d) in [(a, -dir), (b, dir)] {
                                painter.add(egui::Shape::convex_polygon(
                                    vec![q, q - d * 10.0 + lp * 4.0, q - d * 10.0 - lp * 4.0],
                                    ink,
                                    Stroke::NONE,
                                ));
                            }
                        }
                        let mut at = tip + dir * 34.0;
                        for (i, (text, blue)) in [(kind, false), (value.as_str(), true), (quantity.as_str(), false)].into_iter().enumerate()
                        {
                            let half = text.chars().count() as f32 * 3.0 + 8.0;
                            at += dir * half;
                            let r = axis_pill(&painter, at, dir, text, blue);
                            if i == 0 {
                                ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("pattern-kind-pill"), r));
                            }
                            at += dir * (half + 6.0);
                        }
                        crate::workspace::badges::paint_row(self, ui, &painter, at + dir * 6.0, dir);
                    }
                    None => {
                        let y = c.y + 70.0;
                        let r = axis_pill(&painter, pos2(c.x - 40.0, y), vec2(1.0, 0.0), kind, false);
                        ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("pattern-kind-pill"), r));
                        axis_pill(&painter, pos2(c.x + 40.0, y), vec2(1.0, 0.0), &quantity, true);
                    }
                }
            }
        }
        // Loft and Sweep's Result badge (26.190). Where it sits wasn't observed: here, between the profiles (Loft)
        // or beside the profile (Sweep).
        let profiles: Vec<shapecraft_doc::Pick> = match &self.tool {
            Tool::Loft(t) => t.profiles.clone(),
            Tool::Sweep(t) => vec![t.profile],
            _ => Vec::new(),
        };
        let middles: Vec<DVec3> =
            profiles.iter().filter_map(|p| crate::workspace::tools::ExtrudeTool::profile_middle(&self.session, *p)).collect();
        if !middles.is_empty()
            && let Some(at) = self.to_screen(middles.iter().sum::<DVec3>() / middles.len() as f64)
        {
            crate::workspace::badges::paint_row(self, ui, &painter, at + vec2(0.0, -34.0), vec2(1.0, 0.0));
        }
        let angle_box = match &self.tool {
            // 26.200 shows the angle beside the handle on the profile (below); the box is for when there's none.
            Tool::Revolve(t) if t.params(t.angle).is_some() && self.revolve_frame(t).is_none() => Some(t.angle),
            Tool::CircularPattern(t) if t.about.is_some() => Some(t.angle),
            _ => None,
        };
        if let Some(angle) = angle_box {
            let c = self.viewport_rect.center();
            let mut shown = typed.map(str::to_string).unwrap_or_else(|| format!("{angle}°"));
            if let Tool::CircularPattern(t) = &self.tool {
                shown = format!("{} × {shown}", t.count);
            }
            value_box(&painter, pos2(c.x, self.viewport_rect.top() + 90.0), &shown, typed);
            if matches!(self.tool, Tool::CircularPattern(_)) {
                // The pattern control badges follow the value (wide enough for "6 × 360°" or a typed field).
                let half = shown.chars().count() as f32 * 4.0 + 30.0;
                crate::workspace::badges::paint_row(
                    self,
                    ui,
                    &painter,
                    pos2(c.x + half.max(70.0), self.viewport_rect.top() + 90.0),
                    vec2(1.0, 0.0),
                );
            }
        }
        if let Tool::Extrude(t) = &self.tool
            && let Some((s0, sdir, _)) = self.handle_frame(t.origin + t.normal * t.distance, t.normal)
        {
            // The reference's look: a ↕ glyph on the cap, the length in a pill on a dashed line back to the
            // base, and the ↔ draft handle with its angle pill.
            // The ↕ is blue for a region and for a body face, before and after the pull. A face shows no ↔ draft handle.
            let blue = Tokens::accent();
            let fresh = matches!(t.target, crate::workspace::tools::Target::Region { .. });
            let shown_len = t.distance;
            // The value field (26.202, s39-cut-typed) opens just past the handle, clear of it whichever way the arrow
            // points, or where the value pill was on the dashed line.
            let field_reach = 30.0 + sdir.x.abs() * 107.0 + sdir.y.abs() * 20.0;
            let mut field_at = s0 + sdir * field_reach;
            double_arrow(&painter, s0, sdir, 14.0, blue, theme::c(0x5a5d66));
            if let Some(b0) = self.to_screen(t.origin) {
                let len = (s0 - b0).length();
                if len > 30.0 {
                    let d = (s0 - b0) / len;
                    let mut x = 18.0;
                    while x < len - 18.0 {
                        painter.line_segment(
                            [b0 + d * x, b0 + d * (x + 6.0).min(len - 18.0)],
                            Stroke::new(1.2, Color32::from_white_alpha(200)),
                        );
                        x += 11.0;
                    }
                }
                // Along the arrow with a blue edge: in the dashed line's middle once there's room, else past the
                // handle ("0 mm" before any pull), as observed.
                // The badges follow the value outward (Sides, then Result), as observed (s25-017); when the line
                // can't hold them all, the value and badges wait past the handle instead (s25-005).
                let text = format_length(shown_len, units);
                let half = text.chars().count() as f32 * 3.0 + 6.0;
                let stack = half + 17.0 + 23.0 * self.badges().len() as f32;
                let pill_at = if len > 2.0 * stack + 40.0 { b0 + (s0 - b0) * 0.5 } else { s0 + sdir * (42.0 + half) };
                if typed.is_none() {
                    axis_pill(&painter, pill_at, sdir, &text, true);
                } else if len > 2.0 * stack + 40.0 {
                    field_at = pill_at;
                }
                // Typing swaps the value for the field: the badges go on past it.
                let start = if typed.is_some() { field_at + sdir * (field_reach - 2.0) } else { pill_at + sdir * (half + 17.0) };
                crate::workspace::badges::paint_row(self, ui, &painter, start, sdir);
            }
            if let Some(t) = typed {
                value_field(&painter, field_at - vec2(13.0, 0.0), t);
            }
            if let Some((at, along)) = self.draft_handle() {
                let hot = t.drafting.is_some() || self.cursor.is_some_and(|c| (c - at).length() < 16.0);
                double_arrow(
                    &painter,
                    at,
                    along,
                    11.0,
                    if hot || !fresh || t.draft.abs() > 1e-9 { blue } else { Color32::WHITE },
                    theme::c(0x5a5d66),
                );
                if !fresh || t.draft.abs() > 1e-9 {
                    pill(&painter, at - along * 40.0, &format!("{}°", t.draft), true);
                }
            }
        } else if let Tool::Revolve(t) = &self.tool
            && t.params(t.angle).is_some()
            && let Some((c, foot, axis)) = self.revolve_frame(t)
            && let (Some(s0), Some(f)) = (self.to_screen(c), self.to_screen(foot))
        {
            // As observed (26.200): a blue ↕ on the profile, a dashed radius to the axis and the dashed circle the
            // handle travels; away from the axis, the Result badge and then the angle.
            let ink = Color32::from_white_alpha(215);
            let r = c.distance(foot);
            if r > 1e-9 {
                let u = (c - foot) / r;
                let v = axis.cross(u);
                let ring: Vec<Pos2> = (0..=96)
                    .filter_map(|i| {
                        let a = i as f64 / 96.0 * std::f64::consts::TAU;
                        self.to_screen(foot + (u * a.cos() + v * a.sin()) * r)
                    })
                    .collect();
                for w in ring.chunks(2).filter(|w| w.len() == 2) {
                    painter.line_segment([w[0], w[1]], Stroke::new(1.3, ink));
                }
                if (f - s0).length() > 20.0 {
                    let d = (f - s0).normalized();
                    painter.add(egui::Shape::dashed_line(&[s0 + d * 14.0, f], Stroke::new(1.3, ink), 5.0, 4.0));
                }
                let tangent = self.to_screen(c + v * r * 0.05).map_or(vec2(0.0, -1.0), |p| (p - s0).normalized());
                let hot = t.dragging.is_some() || self.cursor.is_some_and(|p| (p - s0).length() < 16.0);
                double_arrow(&painter, s0, tangent, 14.0, if hot { Tokens::accent() } else { Tokens::sketch_blue() }, theme::c(0x5a5d66));
                let out = if (s0 - f).length() > 1.0 { (s0 - f).normalized() } else { vec2(-1.0, 0.0) };
                let n = self.badges().len() as f32;
                crate::workspace::badges::paint_row(self, ui, &painter, s0 + out * 30.0, out);
                let shown = typed.map(|n| format!("{n}°")).unwrap_or_else(|| format!("{}°", t.angle));
                let half = shown.chars().count() as f32 * 3.3 + 6.0;
                let at = s0 + out * (30.0 + 23.0 * n + half - 4.0);
                if typed.is_some() {
                    value_box(&painter, at + out * 40.0, "", typed);
                } else {
                    axis_pill(&painter, at, out, &shown, true);
                }
            }
        } else if let Some((base, dir, value, _)) = handle
            && !matches!(self.tool, Tool::Revolve(_))
            && let Some((s0, sdir, _)) = self.handle_frame(base, dir)
        {
            // As observed: a blue glyph on the face/edge (↕, or a bracket for fillets) with the value in a pill
            // that runs along the handle's axis, and a dashed guide back to where it started.
            let blue = Tokens::accent();
            if let Some((anchor, d0)) = match &self.tool {
                Tool::OffsetFace(t) => Some((t.anchor, t.distance)),
                Tool::Shell(t) => Some((t.anchor, t.thickness)),
                Tool::OffsetPlane(t) => Some((t.anchor, t.distance)),
                Tool::OffsetEdge(t) => Some((t.anchor, t.distance)),
                _ => None,
            } && d0.abs() > 1e-9
                && let Some(b0) = self.to_screen(anchor)
            {
                painter.add(egui::Shape::dashed_line(&[b0, s0], Stroke::new(1.3, Color32::from_white_alpha(210)), 5.0, 4.0));
                // Offset Face's lead starts in a drop where the face was (26.202, s37).
                if matches!(self.tool, Tool::OffsetFace(_)) && (s0 - b0).length() > 14.0 {
                    lead_drop(&painter, b0, (b0 - s0).normalized(), Color32::from_white_alpha(235));
                }
            }
            if matches!(self.tool, Tool::Fillet(_)) {
                // As observed (280): a bold blue ↔ starting at the edge and pointing out of the part.
                double_arrow(&painter, s0 - sdir * 20.0, sdir, 16.0, blue, theme::c(0x5a5d66));
            } else if let Tool::OffsetFace(t) = &self.tool {
                face_arrow(&painter, s0, sdir, t.dragging.is_some());
            } else {
                double_arrow(&painter, s0, sdir, 14.0, blue, theme::c(0x5a5d66));
            }
            let mut label_at = s0 + sdir * 40.0;
            let mut label_dir = sdir;
            // Offset Edge (26.200, s11-075): the value inward of the arrow, then a small offset-edge badge.
            if matches!(self.tool, Tool::OffsetEdge(_)) {
                label_at = s0 - sdir * 36.0;
                // Its Type badge (Single / Chain), as in the manual.
                crate::workspace::badges::paint_row(self, ui, &painter, s0 - sdir * 66.0, -sdir);
            }
            // Offset Face's distance type pill, and where it goes (nearer the handle than the value).
            let mut kind_pill: Option<(Pos2, &str)> = None;
            if let Tool::OffsetFace(t) = &self.tool
                && t.fillet.is_some()
            {
                // As observed (362): "R0.9 mm" then "Edit Fillet 01" past the handle.
                label_at = s0 + sdir * 44.0;
                kind_pill = Some((s0 + sdir * 112.0, ""));
            } else if let Tool::OffsetFace(t) = &self.tool {
                use crate::workspace::tools::OffsetKind as K;
                let mode = self.offset_kind(t);
                let kind = match mode {
                    K::Offset => "Offset ⌄",
                    K::Total => "Total ⌄",
                    K::Radius => "Radius ⌄",
                    K::Diameter => "Diameter ⌄",
                };
                // Offset, as observed: only the arrow until it's pulled.
                if t.kinds && (mode != K::Offset || t.distance.abs() > 1e-9 || typed.is_some()) {
                    kind_pill = Some((s0 + sdir * 40.0, kind));
                }
                label_at = s0 + sdir * 86.0;
                let ink = Color32::from_white_alpha(235);
                // Radius: a dashed lead from the handle in to the axis. Diameter: on through it to the far wall
                // (26.202, s37). The pills stay outside the part.
                let far = match (mode, t.radius) {
                    (K::Radius, Some((_, foot))) => Some(foot),
                    (K::Diameter, Some((_, foot))) => Some(foot * 2.0 - t.anchor),
                    _ => None,
                };
                if let Some(e) = far.and_then(|p| self.to_screen(p))
                    && (e - s0).length() > 20.0
                {
                    let ldir = (e - s0).normalized();
                    painter.add(egui::Shape::dashed_line(&[s0 + ldir * 13.0, e], Stroke::new(1.3, ink), 5.0, 4.0));
                    lead_drop(&painter, e, ldir, ink);
                }
                // Total, as observed: a dashed dimension through the part to the opposite wall, arrowheads at
                // both ends; the pills sit in its middle when there's room, else beyond the handle.
                if mode == K::Total
                    && let Some(th) = t.thickness
                    && let Some(e) = self.to_screen(t.anchor - t.dir * th)
                    && let Some(ldir) = ((e - s0).length() > 1.0).then(|| (e - s0).normalized())
                {
                    let start = s0 + ldir * 18.0;
                    if (e - start).dot(ldir) > 8.0 {
                        painter.add(egui::Shape::dashed_line(&[start, e], Stroke::new(1.3, ink), 5.0, 4.0));
                        let lp = vec2(-ldir.y, ldir.x);
                        for (tip, d) in [(start, -ldir), (e, ldir)] {
                            painter.add(egui::Shape::convex_polygon(
                                vec![tip, tip - d * 11.0 + lp * 4.0, tip - d * 11.0 - lp * 4.0],
                                ink,
                                Stroke::NONE,
                            ));
                        }
                    }
                    let len = (e - s0).length();
                    if len > 170.0 {
                        let mid = s0 + ldir * (len / 2.0);
                        kind_pill = Some((mid - ldir * 26.0, kind));
                        label_at = mid + ldir * 26.0;
                        label_dir = ldir;
                    }
                }
            }
            let fillet_name = match &self.tool {
                Tool::OffsetFace(t) => t.fillet.as_ref().map(|f| format!("Edit {}", f.label)),
                _ => None,
            };
            // Only a fillet's face trades the distance type pill for its name (taking it unconditionally hid "Total").
            if let Some(name) = &fillet_name
                && let Some((at, _)) = kind_pill.take()
            {
                let pr = axis_pill(&painter, at, label_dir, name, false);
                ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("offset-kind-pill"), pr));
            }
            let shown = match &self.tool {
                Tool::OffsetFace(t) if t.fillet.is_some() => {
                    let k = self.offset_kind(t);
                    let d = typed.and_then(crate::workspace::tools::parse_length).map(|v| {
                        let mut c = t.clone();
                        c.set_shown(k, v);
                        c.shown(k)
                    });
                    format!("R{}", format_length(d.unwrap_or(t.shown(k)), units))
                }
                // A round face reads "R7.5 mm" or "⌀10 mm" (26.202, s37).
                Tool::OffsetFace(t) => {
                    let k = self.offset_kind(t);
                    let prefix = match k {
                        crate::workspace::tools::OffsetKind::Radius => "R",
                        crate::workspace::tools::OffsetKind::Diameter => "⌀",
                        _ => "",
                    };
                    format!("{prefix}{}", format_length(t.shown(k), units))
                }
                // As observed: a fillet reads "R5 mm", a chamfer just its length.
                Tool::Fillet(t) if t.chamfer => format_length(t.radius, units),
                // Before a pull it reads just "0 mm"; pulled, "R2 mm" (26.202, s40-008 / s40-010).
                Tool::Fillet(t) if t.radius.abs() < 1e-9 => format_length(0.0, units),
                Tool::Fillet(t) => format!("R{}", format_length(t.radius, units)),
                _ => format_length(value, units),
            };
            // Offset, Radius and Diameter (26.202, s37): the value comes first along the line, the type pill after
            // it. Total keeps the type first.
            // Typing opens the value field where the value pill was (26.202, s39-of-type15, s39-cut-typed), so the
            // pills keep their places while it's open. Fillet too (26.202, s40-009: the field over its "0 mm").
            let value_first = matches!(&self.tool, Tool::OffsetFace(t)
                if t.fillet.is_none() && self.offset_kind(t) != crate::workspace::tools::OffsetKind::Total);
            if let Some((at, kind)) = kind_pill {
                let half = shown.chars().count() as f32 * 2.9 + 6.0;
                let kind_at = if value_first {
                    label_at = at + label_dir * (half - 12.0);
                    let kind_half = kind.chars().count() as f32 * 2.9 + 6.0;
                    label_at + label_dir * (half + 5.0 + kind_half)
                } else {
                    at
                };
                let pr = axis_pill(&painter, kind_at, label_dir, kind, false);
                ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with("offset-kind-pill"), pr));
                // The value follows the type pill along the same line, a small gap after it however long it reads.
                if !value_first && matches!(&self.tool, Tool::OffsetFace(t) if t.fillet.is_none()) {
                    let along = (pr.width().max(pr.height()) / 2.0).max(10.0);
                    label_at = at + label_dir * (along + 5.0 + half);
                }
            }
            // Fillet, as observed: before a pull, "Fillet ⌄" out past the handle and the value capsule after it;
            // once pulled, a dashed dimension from the edge to the handle (arrowheads both ends) carries the value.
            if let Tool::Fillet(t) = &self.tool {
                let out = -sdir;
                if t.radius.abs() < 1e-9 && typed.is_none() {
                    // 26.200: the value waits past the handle, its badge (G1, or the chamfer Type) beyond it.
                    label_at = s0 + out * (26.0 + shown.chars().count() as f32 * 3.3 + 6.0);
                    label_dir = out;
                } else if let Some(e) = self.to_screen(t.anchor) {
                    let ink = Color32::from_white_alpha(235);
                    painter.add(egui::Shape::dashed_line(&[s0, e], Stroke::new(1.3, ink), 5.0, 4.0));
                    if (e - s0).length() > 2.0 {
                        let ldir = (e - s0).normalized();
                        let lp = vec2(-ldir.y, ldir.x);
                        for (tip, d) in [(s0, -ldir), (e, ldir)] {
                            painter.add(egui::Shape::convex_polygon(
                                vec![tip, tip - d * 11.0 + lp * 4.0, tip - d * 11.0 - lp * 4.0],
                                ink,
                                Stroke::NONE,
                            ));
                        }
                    }
                    label_at = s0 + (e - s0) * 0.5 - vec2(-sdir.y, sdir.x) * 14.0;
                    label_dir = sdir;
                }
                // The profile slider's knob on that dimension (black, as observed s29-016).
                if let Some((_, _, knob)) = self.fillet_slider() {
                    painter.circle_filled(knob, 5.0, theme::c(0x101010));
                    painter.circle_stroke(knob, 5.0, Stroke::new(1.2, Color32::WHITE));
                }
            }
            // Offset Face's plain Offset shows no value until it's pulled (as observed, s37).
            let unpulled_offset = matches!(&self.tool, Tool::OffsetFace(t)
                if t.kinds && t.fillet.is_none() && t.distance.abs() < 1e-9 && self.offset_kind(t) == crate::workspace::tools::OffsetKind::Offset);
            // Before anything is typed the value is the blue-rimmed pill on the line (26.202, s39-of-side: "13 mm"
            // after "Total ⌄"); a key opens the field over it.
            if let Some(t) = typed {
                value_field(&painter, label_at - vec2(13.0, 0.0), t);
            } else if !unpulled_offset {
                axis_pill(&painter, label_at, label_dir, &shown, true);
            }
            // Its badge (G1, or the chamfer's Type) once there's a fillet to change, and from the start when Fillet
            // was chosen (F, the column) rather than brought up by picking an edge; never while the field is open.
            if let Tool::Fillet(t) = &self.tool
                && typed.is_none()
                && (t.chamfer || t.radius.abs() > 1e-9 || !self.predicted)
            {
                let half = shown.chars().count() as f32 * 3.3 + 6.0;
                crate::workspace::badges::paint_row(self, ui, &painter, label_at + label_dir * (half + 17.0), label_dir);
            }
        }
        // Two-Distance chamfer's second arrow, on the other face, with its own dashed dimension back to the edge.
        if let Some((s1, sdir1, _)) = self.chamfer2_handle()
            && let Tool::Fillet(t) = &self.tool
            && let Some(two) = &t.two
        {
            double_arrow(&painter, s1 - sdir1 * 20.0, sdir1, 16.0, Tokens::accent(), theme::c(0x5a5d66));
            let label = format_length(if two.d2 > 1e-9 { two.d2 } else { t.radius }, units);
            match self.to_screen(t.anchor) {
                Some(e) if (e - s1).length() > 2.0 && two.d2 > 1e-9 => {
                    painter.add(egui::Shape::dashed_line(&[s1, e], Stroke::new(1.3, Color32::from_white_alpha(235)), 5.0, 4.0));
                    axis_pill(&painter, s1 + (e - s1) * 0.5 + vec2(-sdir1.y, sdir1.x) * 14.0, sdir1, &label, true);
                }
                _ => {
                    axis_pill(&painter, s1 - sdir1 * (26.0 + label.chars().count() as f32 * 3.3 + 6.0), -sdir1, &label, true);
                }
            }
        }
        // Selected sketch elements keep their dimensions up, like the reference; click one to type a new value.
        let editing = self.dim_edit;
        // Drawing the next segment of a chain hides the last one's dimension (it shows until the pointer moves).
        let chaining = match (&self.tool, self.sketch_cursor()) {
            (Tool::Line { last: Some(l), .. }, Some(c)) => l.distance(c) > 1e-9,
            // Transforming (or rounding) picked curves hides their dimensions (s11-088, s11-093, s11-095).
            _ => self.sketch_transform().is_some() || matches!(self.tool, Tool::SketchFillet { .. }),
        };
        // Driving dimensions (typed values kept, s11-037/056/061) stay up on every shown sketch: plain values on
        // their lines (left of start→end), circles as a dashed diameter.
        let driven = self.paint_driving_dims(&painter, chaining || matches!(self.tool, Tool::Text { .. }));
        // Two lines picked at a corner show only their angle (s15-006).
        let cornered = self.picked_corner().is_some();
        for d in self.entity_dims().into_iter().filter(|_| !chaining && !cornered).filter(|d| !driven.contains(&(d.sketch, d.entity))) {
            let is_edit = editing.is_some_and(|e| e.sketch == d.sketch && e.entity == d.entity && e.kind == d.kind);
            let text = format_length(d.value, units);
            let typed_here = if is_edit { Some(typed.unwrap_or("")) } else { None };
            match d.line {
                Some((a, b, RADIAL)) => radius_dim(&painter, a, b, &format!("{}{}", d.prefix, text), typed_here, false),
                Some((a, b, DASHED_RADIUS)) => dashed_radius(&painter, a, b, &format!("{}{}", d.prefix, text), typed_here),
                Some((a, b, DIAMETER)) => diameter_dim(&painter, a, b, &format!("{}{}", d.prefix, text), typed_here, false),
                Some((a, b, off)) if (b - a).length() > 2.0 => {
                    // Same placement rule as before (outside: up or right), drawn the 26.200 way.
                    let n = vec2(-(b - a).y, (b - a).x).normalized();
                    let n = if n.y > 0.0 || (n.y == 0.0 && n.x < 0.0) { -n } else { n };
                    sketch_dimension(&painter, [a, b], [a + n * off, b + n * off], &text, typed_here, false);
                }
                Some(_) => {}
                None => value_box(&painter, d.label, &format!("{}{}", d.prefix, text), typed_here),
            }
        }
        // An opened line dimension's Distance Type badge sits just left of its value field (the panel reaches 94 px left of the value).
        if let Some((sketch, entity, ..)) = self.opened_line() {
            let at = self
                .driving_dims()
                .0
                .into_iter()
                .find(|d| d.0 == sketch && d.1 == entity)
                .map(|d| Self::driving_label(d.2, d.5))
                .or_else(|| self.entity_dims().into_iter().find(|d| d.sketch == sketch && d.entity == entity).map(|d| d.label));
            if let Some(at) = at {
                crate::workspace::badges::paint_row(self, ui, &painter, at - vec2(110.0, 0.0), vec2(-1.0, 0.0));
            }
        }
        // A picked arc keeps its angle and radius up, as it showed while drawn (s11-043).
        if let (Some(sk), false) = (self.session.active_sketch(), chaining) {
            for pick in &self.session.selection {
                if let Pick::SketchEntity { sketch, entity } = *pick
                    && sketch == sk.id
                    && let Some(SketchEntity::Arc { a, mid, b }) = sk.entities.get(entity as usize)
                {
                    self.paint_arc_dims(&painter, &sk.plane, *a, *mid, *b, false, None, false);
                }
            }
        }
        if let Some((.., c, ns, _, _)) = self.side_handle() {
            let fill = if self.side_drag.is_some() { Tokens::accent() } else { Color32::WHITE };
            double_arrow(&painter, c, ns, 11.0, fill, theme::c(0x1c1c1e));
        }
        self.paint_constraint_badges(&painter, self.cursor);
        self.paint_point_label(&painter);
        // Picked key points: orange dots with a white ring.
        if let Some(pl) = self.sketch_plane() {
            for q in self.picked_points() {
                if let Some(sp) = self.to_screen(pl.to_world(q)) {
                    painter.circle_filled(sp, 5.0, theme::c(0xfaa84b));
                    painter.circle_stroke(sp, 5.0, Stroke::new(1.5, Color32::WHITE));
                }
            }
        }
    }
}

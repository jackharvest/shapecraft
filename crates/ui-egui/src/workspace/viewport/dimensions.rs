//! Sketch dimensions: the driving ones, angles, point distances and the selected elements' labels, where each
//! sits on screen, and applying a typed value.

use egui::{Color32, Pos2, Stroke, vec2};
use glam::DVec2;
use serde_json::json;
use shapecraft_doc::constraints::Element;
use shapecraft_doc::{ItemId, Pick, SketchEntity, SketchPlane, format_length};

use super::paint::{
    DASHED_RADIUS, DIAMETER, RADIAL, angle_dim, diameter_dim, dim_label_pos, sketch_dimension, sketch_dimension_label, value_field,
};
use crate::theme;
use crate::workspace::Workspace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimKind {
    Width,
    Height,
    Length,
    Radius,
    Diameter,
    EllipseX,
    EllipseY,
    /// Between two lines meeting at a corner (`DimEdit::other` is the second line).
    Angle,
    /// From a point to another point or to a line (`DimEdit::pair`).
    PointDistance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DimEdit {
    pub sketch: shapecraft_doc::ItemId,
    pub entity: u32,
    pub kind: DimKind,
    pub other: Option<u32>,
    /// A point distance's two elements.
    pub pair: Option<(Element, Element)>,
}

pub struct Dim {
    pub sketch: shapecraft_doc::ItemId,
    pub entity: u32,
    pub kind: DimKind,
    pub value: f64,
    pub label: Pos2,
    /// Drawn as a dimension line between two screen points with an offset, else as a floating label.
    pub line: Option<(Pos2, Pos2, f32)>,
    pub prefix: &'static str,
}

/// A distance from a point on screen: its two ends, where the dimension line runs, and its value's spot.
pub(in crate::workspace) struct PointDim {
    pub sketch: ItemId,
    pub a: Element,
    pub b: Element,
    pub value: f64,
    pub ends: [Pos2; 2],
    pub dim: [Pos2; 2],
    pub label: Pos2,
    /// Shown for the picked elements, not kept yet.
    pub live: bool,
}

/// The dimension arc's radius around the corner, as measured (s15-006: about 50 px).
/// A driving dimension on screen: the entity, which of its values, the kind, the value, the dimension line and
/// the extension lines' far ends.
pub(super) type DrivingDim = (ItemId, u32, DimKind, f64, [Pos2; 2], [Pos2; 2]);

pub(super) const ANGLE_RADIUS: f32 = 50.0;

/// An angle dimension between two lines on screen: the corner, the unit directions of the two lines from it, and
/// where its value sits.
pub(in crate::workspace) struct AngleDim {
    pub sketch: ItemId,
    pub first: u32,
    pub second: u32,
    pub degrees: f64,
    pub corner: Pos2,
    pub u1: egui::Vec2,
    pub u2: egui::Vec2,
    pub label: Pos2,
}

impl AngleDim {
    /// The point `r` px out from the corner, `t` of the way from the first line to the second.
    pub(super) fn at(&self, r: f32, t: f32) -> Pos2 {
        let a1 = self.u1.y.atan2(self.u1.x);
        let mut sweep = self.u2.y.atan2(self.u2.x) - a1;
        while sweep > std::f32::consts::PI {
            sweep -= std::f32::consts::TAU;
        }
        while sweep < -std::f32::consts::PI {
            sweep += std::f32::consts::TAU;
        }
        let a = a1 + sweep * t;
        self.corner + vec2(a.cos(), a.sin()) * r
    }
}

impl Workspace {
    /// The arc's sweep on a dimension arc ~45 px outside it (boxed while live, typing opens the field) and its
    /// radius: dashed spokes with "R …" while the end still moves, else an "R …" leader to the curve.
    pub(super) fn paint_arc_dims(
        &self,
        painter: &egui::Painter,
        pl: &SketchPlane,
        a: DVec2,
        mid: DVec2,
        b: DVec2,
        spokes: bool,
        typed: Option<&str>,
        live: bool,
    ) {
        let Some((c, r)) = shapecraft_doc::circle_through(a, mid, b) else { return };
        let units = self.session.doc.units;
        let pts = shapecraft_doc::arc_points(a, mid, b, 48);
        let t0 = (a - c).to_angle();
        let span = {
            let p1 = pts.get(1).copied().unwrap_or(b);
            let dir = ((p1 - c).to_angle() - t0).sin().signum();
            let tau = std::f64::consts::TAU;
            let ccw = ((b - c).to_angle() - t0).rem_euclid(tau);
            if dir >= 0.0 { ccw } else { -(tau - ccw) }
        };
        let off = self.camera.world_per_pixel(self.size().y) * 45.0;
        let s = |p: DVec2| self.to_screen(pl.to_world(p));
        let at = |t: f64, rr: f64| c + DVec2::from_angle(t) * rr;
        let white = Color32::WHITE;
        // Extension lines from the ends out past the dimension arc.
        for t in [t0, t0 + span] {
            if let (Some(p), Some(q)) = (s(at(t, r)), s(at(t, r + off * 1.12))) {
                painter.line_segment([p, q], Stroke::new(1.0, white));
            }
        }
        let arc: Vec<Pos2> = (0..=48).filter_map(|i| s(at(t0 + span * i as f64 / 48.0, r + off))).collect();
        if arc.len() > 2 {
            painter.add(egui::Shape::line(arc.clone(), Stroke::new(1.0, white)));
            for (tip, prev) in [(arc[0], arc[1]), (arc[arc.len() - 1], arc[arc.len() - 2])] {
                let u = (prev - tip).normalized();
                let pp = vec2(-u.y, u.x);
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, tip + u * 9.0 + pp * 3.5, tip + u * 9.0 - pp * 3.5],
                    white,
                    Stroke::NONE,
                ));
            }
            let k = arc.len() / 2;
            let mut text = format!("{:.4}", span.abs().to_degrees());
            while text.contains('.') && (text.ends_with('0') || text.ends_with('.')) {
                text.pop();
            }
            text.push('°');
            if let Some(t) = typed {
                value_field(painter, arc[k], t);
            } else {
                let u = (arc[(k + 1).min(arc.len() - 1)] - arc[k.saturating_sub(1)]).normalized();
                // A placed arc's angle sits just outside its dimension arc.
                let lift = if live { vec2(0.0, 0.0) } else { vec2(-u.y, u.x) * if vec2(-u.y, u.x).y > 0.0 { -9.0 } else { 9.0 } };
                sketch_dimension_label(painter, arc[k] + lift, u, &text, live);
            }
        }
        let rtext = format!("R {}", format_length(r, units));
        if spokes {
            for p in [a, b] {
                if let (Some(sc), Some(sp)) = (s(c), s(p)) {
                    painter.add(egui::Shape::dashed_line(&[sc, sp], Stroke::new(1.0, white.gamma_multiply(0.8)), 4.0, 3.0));
                }
            }
            if let (Some(sc), Some(sb)) = (s(c), s(b)) {
                let u = (sb - sc).normalized();
                let pp = vec2(-u.y, u.x);
                painter.add(egui::Shape::convex_polygon(vec![sb, sb - u * 9.0 + pp * 3.5, sb - u * 9.0 - pp * 3.5], white, Stroke::NONE));
                sketch_dimension_label(painter, sc + (sb - sc) * 0.5 - pp * 10.0, u, &rtext, false);
            }
        } else {
            // A leader from outside the start of the curve, pointing in at it.
            let t = t0 + span * 0.08;
            if let (Some(tip), Some(out)) = (s(at(t, r)), s(at(t, r + off * 1.6))) {
                let u = (tip - out).normalized();
                let pp = vec2(-u.y, u.x);
                painter.line_segment([out, tip], Stroke::new(1.0, white));
                painter.add(egui::Shape::convex_polygon(
                    vec![tip, tip - u * 9.0 + pp * 3.5, tip - u * 9.0 - pp * 3.5],
                    white,
                    Stroke::NONE,
                ));
                let g = painter.layout_no_wrap(rtext, theme::mono(12.0), white);
                let anchor = if u.x > 0.0 { out - vec2(g.size().x + 4.0, g.size().y / 2.0) } else { out + vec2(4.0, -g.size().y / 2.0) };
                painter.galley(anchor, g, white);
            }
        }
    }

    /// The sketches' driving dimensions as drawn: (sketch, element, kind, value, ends, dimension-line ends) — a
    /// line's on its left of start→end 45 px out, a rectangle side's 70 px outside it, a circle's as a diameter
    /// across (ends = the rim points). Also every element that has one (a rectangle counts all four sides).
    pub(super) fn driving_dims(&self) -> (Vec<DrivingDim>, Vec<(ItemId, u32)>) {
        let (mut dims, mut has) = (Vec::new(), Vec::new());
        for sk in self
            .session
            .doc
            .sketches
            .iter()
            .filter(|s| s.visible && (self.session.active_sketch.is_none() || self.session.active_sketch == Some(s.id)))
        {
            let s = |p: DVec2| self.to_screen(sk.plane.to_world(p));
            for c in &sk.constraints {
                match *c {
                    shapecraft_doc::Constraint::Distance { a, value, along } => {
                        let entity = a.entity();
                        has.push((sk.id, entity));
                        let rect = shapecraft_doc::rectangle_loop(&sk.entities, entity).and_then(|lp| {
                            has.extend(lp.iter().map(|e| (sk.id, *e)));
                            shapecraft_doc::rectangle_frame(&sk.entities, lp)
                        });
                        let Some(SketchEntity::Line { a: p, b: q }) = sk.entities.get(entity as usize) else { continue };
                        if let (Some(sa), Some(sb)) = (s(*p), s(*q))
                            && (sb - sa).length() > 2.0
                        {
                            let d = sb - sa;
                            let mut n = vec2(d.y, -d.x).normalized() * 45.0;
                            if let Some(f) = rect
                                && let Some(mid) = s(f.origin + (f.u * f.width + f.v * f.height) / 2.0)
                            {
                                n = n.normalized() * 70.0;
                                if (sa - mid).dot(n) < 0.0 {
                                    n = -n;
                                }
                            }
                            // Horizontal / Vertical: a level (upright) dimension line past the higher (righter) end,
                            // its extension lines square to it.
                            let px_mm = s(*p + DVec2::X).map_or(1.0, |x| (x - sa).length().max(1e-6)) as f64;
                            let off = 45.0 / px_mm;
                            let level = match along {
                                shapecraft_doc::Along::Absolute => None,
                                shapecraft_doc::Along::Horizontal => {
                                    let y = p.y.max(q.y) + off;
                                    Some((DVec2::new(p.x, y), DVec2::new(q.x, y)))
                                }
                                shapecraft_doc::Along::Vertical => {
                                    let x = p.x.max(q.x) + off;
                                    Some((DVec2::new(x, p.y), DVec2::new(x, q.y)))
                                }
                            };
                            if let Some((da, db)) = level
                                && let (Some(da), Some(db)) = (s(da), s(db))
                            {
                                dims.push((sk.id, entity, DimKind::Length, value, [sa, sb], [da, db]));
                                continue;
                            }
                            dims.push((sk.id, entity, DimKind::Length, value, [sa, sb], [sa + n, sb + n]));
                        }
                    }
                    shapecraft_doc::Constraint::Radius { a, value } => {
                        let entity = a.entity();
                        has.push((sk.id, entity));
                        let center = match sk.entities.get(entity as usize) {
                            Some(SketchEntity::Circle { center, .. }) => *center,
                            Some(SketchEntity::Arc { a, mid, b }) => shapecraft_doc::circle_through(*a, *mid, *b).map_or(*a, |(c, _)| c),
                            _ => continue,
                        };
                        if let (Some(sc), Some(rim)) = (s(center), s(center + DVec2::new(value, 0.0))) {
                            let r = (rim - sc).length();
                            let dir = vec2(0.82, 0.57);
                            let ends = [sc - dir * r, sc + dir * r];
                            dims.push((sk.id, entity, DimKind::Diameter, value * 2.0, ends, ends));
                        }
                    }
                    _ => {}
                }
            }
        }
        (dims, has)
    }

    /// Where a driving dimension's value sits (what a click on it edits).
    pub(super) fn driving_label(kind: DimKind, dim: [Pos2; 2]) -> Pos2 {
        if kind == DimKind::Diameter { dim[0] + (dim[1] - dim[0]) * 0.775 } else { dim[0] + (dim[1] - dim[0]) * 0.5 }
    }

    /// Paints the driving dimensions (unless `hidden`; the one being edited shows the value field) and returns the
    /// elements that have one.
    pub(super) fn paint_driving_dims(&self, painter: &egui::Painter, hidden: bool) -> Vec<(ItemId, u32)> {
        let (dims, has) = self.driving_dims();
        if hidden {
            return has;
        }
        let units = self.session.doc.units;
        for (sketch, entity, kind, value, ends, dim) in dims {
            let typed = self.dim_edit.filter(|e| e.sketch == sketch && e.entity == entity).map(|_| self.numeric.as_deref().unwrap_or(""));
            if kind == DimKind::Diameter {
                diameter_dim(painter, ends[0], ends[1], &format!("Ø {}", format_length(value, units)), typed, false);
            } else {
                sketch_dimension(painter, ends, dim, &format_length(value, units), typed, false);
            }
        }
        for d in self.point_dims() {
            let typed = self
                .dim_edit
                .filter(|e| e.kind == DimKind::PointDistance && e.sketch == d.sketch && e.pair == Some((d.a, d.b)))
                .map(|_| self.numeric.as_deref().unwrap_or(""));
            if d.live && matches!(d.b, Element::Point { .. }) {
                // A faint purple line joins the two picked points (s15-012).
                painter.line_segment([d.ends[0], d.ends[1]], Stroke::new(1.0, Color32::from_rgba_unmultiplied(161, 98, 248, 150)));
            }
            sketch_dimension(painter, d.ends, d.dim, &format_length(d.value, units), typed, false);
        }
        for a in self.angle_dims() {
            let typed = self
                .dim_edit
                .filter(|e| e.kind == DimKind::Angle && e.sketch == a.sketch && e.entity == a.first && e.other == Some(a.second))
                .map(|_| self.numeric.as_deref().unwrap_or(""));
            angle_dim(painter, &a, typed);
        }
        has
    }

    /// Angle dimensions as drawn: the sketches' driving angles, and the live one between two picked lines.
    pub(super) fn angle_dims(&self) -> Vec<AngleDim> {
        let mut out = Vec::new();
        let picked = self.picked_corner();
        for sk in self
            .session
            .doc
            .sketches
            .iter()
            .filter(|s| s.visible && (self.session.active_sketch.is_none() || self.session.active_sketch == Some(s.id)))
        {
            let mut pairs: Vec<(u32, u32)> = sk
                .constraints
                .iter()
                .filter_map(|c| if let shapecraft_doc::Constraint::Angle { a, b, .. } = c { Some((a.entity(), b.entity())) } else { None })
                .collect();
            if let Some((s, a, b)) = picked
                && s == sk.id
                && !pairs.iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
            {
                pairs.push((a, b));
            }
            for (a, b) in pairs {
                let (Some(SketchEntity::Line { a: p, b: q }), Some(SketchEntity::Line { a: r, b: t })) =
                    (sk.entities.get(a as usize), sk.entities.get(b as usize))
                else {
                    continue;
                };
                let (c, d1, d2) = shapecraft_doc::constraints::corner_rays((*p, *q), (*r, *t));
                let s = |v: DVec2| self.to_screen(sk.plane.to_world(v));
                let (Some(sc), Some(s1), Some(s2)) = (s(c), s(c + d1), s(c + d2)) else { continue };
                if (s1 - sc).length() < 2.0 || (s2 - sc).length() < 2.0 {
                    continue;
                }
                let (u1, u2) = ((s1 - sc).normalized(), (s2 - sc).normalized());
                let degrees = shapecraft_doc::constraints::ray_angle(d1, d2).to_degrees();
                let mut dim = AngleDim { sketch: sk.id, first: a, second: b, degrees, corner: sc, u1, u2, label: sc };
                dim.label = dim.at(ANGLE_RADIUS + 14.0, 0.5);
                out.push(dim);
            }
        }
        out
    }

    /// Distances from points as drawn: the sketches' driving ones, and the live one for two picked points or a picked
    /// point and line (s15-012, s15-014). Aligned between points, ~45 px to one side; square to a line, beside them.
    pub(super) fn point_dims(&self) -> Vec<PointDim> {
        let mut out = Vec::new();
        let live = self.session.active_sketch().and_then(|sk| {
            let pts: Vec<Element> = self
                .session
                .selection
                .iter()
                .filter_map(|p| match *p {
                    Pick::SketchPoint { sketch, entity, index } if sketch == sk.id => Some(Element::Point { entity, index }),
                    _ => None,
                })
                .collect();
            let lines: Vec<Element> = self
                .session
                .selection
                .iter()
                .filter_map(|p| match *p {
                    Pick::SketchEntity { sketch, entity }
                        if sketch == sk.id && matches!(sk.entities.get(entity as usize), Some(SketchEntity::Line { .. })) =>
                    {
                        Some(Element::Segment { entity, index: 0 })
                    }
                    _ => None,
                })
                .collect();
            match (&pts[..], &lines[..], self.session.selection.len()) {
                ([a, b], [], 2) => Some((sk.id, *a, *b)),
                ([a], [b], 2) => Some((sk.id, *a, *b)),
                _ => None,
            }
        });
        for sk in self
            .session
            .doc
            .sketches
            .iter()
            .filter(|s| s.visible && (self.session.active_sketch.is_none() || self.session.active_sketch == Some(s.id)))
        {
            let mut pairs: Vec<(Element, Element, bool)> = sk
                .constraints
                .iter()
                .filter_map(|c| if let shapecraft_doc::Constraint::PointDistance { a, b, .. } = c { Some((*a, *b, false)) } else { None })
                .collect();
            if let Some((s, a, b)) = live
                && s == sk.id
                && !pairs.iter().any(|&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a))
            {
                pairs.push((a, b, true));
            }
            let point = |el: Element| match el {
                Element::Point { entity, index } => {
                    sk.entities.get(entity as usize).and_then(|e| shapecraft_doc::constraints::points(e).get(index as usize).copied())
                }
                _ => None,
            };
            for (a, b, is_live) in pairs {
                let Some(p) = point(a) else { continue };
                // The far end: the other point, or the foot of the perpendicular on the line.
                let q = match b {
                    Element::Segment { entity, .. } => match sk.entities.get(entity as usize) {
                        Some(SketchEntity::Line { a: l0, b: l1 }) => {
                            let d = (*l1 - *l0).normalize_or_zero();
                            *l0 + d * (p - *l0).dot(d)
                        }
                        _ => continue,
                    },
                    _ => match point(b) {
                        Some(q) => q,
                        None => continue,
                    },
                };
                let s = |v: DVec2| self.to_screen(sk.plane.to_world(v));
                let (Some(sp), Some(sq)) = (s(p), s(q)) else { continue };
                if sp.distance(sq) < 2.0 {
                    continue;
                }
                let u = (sq - sp).normalized();
                let mut n = vec2(-u.y, u.x);
                // Offset to the side away from the screen's centre of the two, upwards when level.
                if n.y > 0.0 || (n.y == 0.0 && n.x < 0.0) {
                    n = -n;
                }
                let dim = [sp + n * 45.0, sq + n * 45.0];
                out.push(PointDim {
                    sketch: sk.id,
                    a,
                    b,
                    value: p.distance(q),
                    ends: [sp, sq],
                    dim,
                    label: dim[0] + (dim[1] - dim[0]) * 0.5,
                    live: is_live,
                });
            }
        }
        out
    }

    /// The dimension labels of the selected sketch elements, with where they sit on screen.
    pub(in crate::workspace) fn entity_dims(&self) -> Vec<Dim> {
        let mut out = Vec::new();
        let Some(sk) = self.session.active_sketch() else { return out };
        // A side of a rectangle shows the rectangle's width and height (once per rectangle), as observed (426).
        let mut rects_done: Vec<u32> = Vec::new();
        for pick in &self.session.selection {
            let Pick::SketchEntity { sketch, entity } = *pick else { continue };
            if sketch != sk.id {
                continue;
            }
            let world = |p: DVec2| self.to_screen(sk.plane.to_world(p));
            if let Some(lp) = shapecraft_doc::rectangle_loop(&sk.entities, entity)
                && let Some(f) = shapecraft_doc::rectangle_frame(&sk.entities, lp)
            {
                let key = lp.iter().copied().min().unwrap_or(entity);
                if rects_done.contains(&key) {
                    continue;
                }
                rects_done.push(key);
                let [bl, br, tr, tl] = f.corners().map(world);
                // Whole: width over the top, height down the right. One side picked: its own dimension beside it,
                // the other on the far left or top, as observed (426).
                // (ends, offset): a positive offset puts it up or right on screen, a negative one down or left.
                // Measured: about 70 px off the sides (425, 426).
                const OFF: f32 = 70.0;
                let mut width = (tl, tr, OFF);
                let mut height = (tr, br, OFF);
                if let [_] = self.session.selection[..]
                    && let Some(SketchEntity::Line { a, b }) = sk.entities.get(entity as usize)
                {
                    let mid = (*a + *b) / 2.0 - f.origin;
                    let (along_u, along_v) = (mid.dot(f.u), mid.dot(f.v));
                    let horizontal = (*b - *a).dot(f.u).abs() > (*b - *a).dot(f.v).abs();
                    if horizontal {
                        width = if along_v < f.height / 2.0 { (bl, br, -OFF) } else { (tl, tr, OFF) };
                        height = (tl, bl, -OFF);
                    } else {
                        height = if along_u < f.width / 2.0 { (tl, bl, -OFF) } else { (tr, br, OFF) };
                        width = (tl, tr, OFF);
                    }
                }
                for (kind, value, (a, b, off)) in [(DimKind::Width, f.width, width), (DimKind::Height, f.height, height)] {
                    if let (Some(a), Some(b)) = (a, b) {
                        out.push(Dim {
                            sketch,
                            entity: key,
                            kind,
                            value,
                            label: dim_label_pos(a, b, off),
                            line: Some((a, b, off)),
                            prefix: "",
                        });
                    }
                }
                continue;
            }
            let mut line_dim = |kind, value, a: Option<Pos2>, b: Option<Pos2>, off: f32| {
                if let (Some(a), Some(b)) = (a, b) {
                    out.push(Dim { sketch, entity, kind, value, label: dim_label_pos(a, b, off), line: Some((a, b, off)), prefix: "" });
                }
            };
            match sk.entities.get(entity as usize) {
                Some(SketchEntity::Rectangle { min, max }) => {
                    let (tl, tr, br) = (world(DVec2::new(min.x, max.y)), world(*max), world(DVec2::new(max.x, min.y)));
                    line_dim(DimKind::Width, max.x - min.x, tl, tr, 26.0);
                    line_dim(DimKind::Height, max.y - min.y, tr, br, 26.0);
                }
                Some(SketchEntity::Line { a, b }) => {
                    // On the left of the line's start→end on screen, where it showed while being drawn (s11-031).
                    let (sa, sb) = (world(*a), world(*b));
                    let off = match (sa, sb) {
                        (Some(p), Some(q)) => {
                            let d = q - p;
                            let n = vec2(-d.y, d.x);
                            let out = if n.y > 0.0 || (n.y == 0.0 && n.x < 0.0) { -n } else { n };
                            if out.dot(vec2(d.y, -d.x)) >= 0.0 { 45.0 } else { -45.0 }
                        }
                        _ => 45.0,
                    };
                    line_dim(DimKind::Length, a.distance(*b), sa, sb, off)
                }
                Some(e @ (SketchEntity::Circle { center, radius } | SketchEntity::Polygon { center, radius, .. })) => {
                    // Circles are labelled by diameter unless Preferences ask for radius everywhere.
                    let diameter = matches!(e, SketchEntity::Circle { .. }) && self.settings.circular_annotations != "Always Radius";
                    let (kind, value, prefix) =
                        if diameter { (DimKind::Diameter, radius * 2.0, "Ø ") } else { (DimKind::Radius, *radius, "R ") };
                    if let (Some(c), Some(rim)) = (world(*center), world(*center + DVec2::new(*radius, 0.0))) {
                        // Across the circle, down to the right on screen (as it was drawn in s11-061).
                        let r = (rim - c).length();
                        let dir = vec2(0.82, 0.57);
                        let (a, b) = (c - dir * r, c + dir * r);
                        let line = match e {
                            _ if diameter => Some((a, b, DIAMETER)),
                            &SketchEntity::Polygon { center, radius, sides, angle } => {
                                // To a corner, the one the pointer placed (its first).
                                let corner = SketchEntity::polygon_points(center, radius, sides, angle).first().copied();
                                corner.and_then(world).map(|p| (c, p, DASHED_RADIUS))
                            }
                            _ => None,
                        };
                        out.push(Dim { sketch, entity, kind, value, label: c + (b - c) * 0.55, line, prefix });
                    }
                }
                Some(SketchEntity::Ellipse { center, rx, ry, angle }) => {
                    let (u, v) = (DVec2::from_angle(*angle), DVec2::from_angle(*angle).perp());
                    // As observed: each half-axis is a radius drawn from the center to the rim.
                    for (kind, value, dir) in [(DimKind::EllipseX, *rx, u), (DimKind::EllipseY, *ry, v)] {
                        if let (Some(a), Some(b)) = (world(*center), world(*center + dir * value)) {
                            out.push(Dim {
                                sketch,
                                entity,
                                kind,
                                value,
                                label: a + (b - a) * 0.5,
                                line: Some((a, b, RADIAL)),
                                prefix: "R ",
                            });
                        }
                    }
                }
                Some(
                    SketchEntity::Arc { .. } | SketchEntity::Spline { .. } | SketchEntity::ControlSpline { .. } | SketchEntity::Text { .. },
                )
                | None => {}
            }
        }
        out
    }

    /// Applies a typed dimension to its sketch element.
    pub(in crate::workspace) fn apply_dim(&mut self, mut edit: DimEdit, value: f64) {
        // A point distance: the second element moves to meet it, and it's kept (s15-012).
        if edit.kind == DimKind::PointDistance {
            if let Some((a, b)) = edit.pair
                && self
                    .run(
                        "constrain",
                        json!({ "kind": "point_distance", "value": value.abs(), "elements": [a, b], "sketch": edit.sketch.0 }),
                    )
                    .is_some()
            {
                self.session.selection.clear();
            }
            return;
        }
        // An angle: the second line turns about the corner and the angle is kept (s15-009).
        if edit.kind == DimKind::Angle {
            if let Some(other) = edit.other {
                let _ = self.run(
                    "constrain",
                    json!({ "kind": "angle", "value": value.abs(), "entities": [edit.entity, other], "sketch": edit.sketch.0 }),
                );
            }
            return;
        }
        let Some(e) = self.session.doc.sketch(edit.sketch).and_then(|s| s.entities.get(edit.entity as usize)).cloned() else { return };
        // A rectangle side's driving dimension resizes the whole rectangle (its width or height).
        if edit.kind == DimKind::Length
            && let Some(sk) = self.session.doc.sketch(edit.sketch)
            && let Some(lp) = shapecraft_doc::rectangle_loop(&sk.entities, edit.entity)
            && let Some(f) = shapecraft_doc::rectangle_frame(&sk.entities, lp)
            && let SketchEntity::Line { a, b } = e
        {
            edit.kind = if (b - a).normalize_or_zero().dot(f.u).abs() > 0.5 { DimKind::Width } else { DimKind::Height };
        }
        let v = value.abs();
        // A driving dimension takes the new value and the sketch is solved to it.
        let driven = self.session.doc.sketch(edit.sketch).is_some_and(|sk| {
            sk.constraints.iter().any(|c| matches!(c, shapecraft_doc::Constraint::Distance { a, .. } | shapecraft_doc::Constraint::Radius { a, .. } if a.entity() == edit.entity))
        });
        if driven && v > 1e-9 && !matches!(edit.kind, DimKind::Width | DimKind::Height) {
            let (kind, value) = if edit.kind == DimKind::Diameter {
                ("radius", v / 2.0)
            } else if matches!(e, SketchEntity::Line { .. }) {
                ("distance", v)
            } else {
                ("radius", v)
            };
            // A line's dimension keeps its Distance Type.
            let along = self.session.doc.sketch(edit.sketch).and_then(|sk| {
                sk.constraints.iter().find_map(|c| match c {
                    shapecraft_doc::Constraint::Distance { a, along, .. } if a.entity() == edit.entity => Some(*along),
                    _ => None,
                })
            });
            let _ = self.run(
                "constrain",
                json!({ "kind": kind, "value": value, "along": along.unwrap_or_default(), "entities": [edit.entity], "sketch": edit.sketch.0 }),
            );
            return;
        }
        // A rectangle of four lines: its far side moves, all four sides change in one step.
        if matches!(edit.kind, DimKind::Width | DimKind::Height)
            && let Some(sk) = self.session.doc.sketch(edit.sketch)
            && let Some(lp) = shapecraft_doc::rectangle_loop(&sk.entities, edit.entity)
            && let Some(f) = shapecraft_doc::rectangle_frame(&sk.entities, lp)
        {
            if v < 1e-9 {
                return;
            }
            let mut g = f;
            if edit.kind == DimKind::Width {
                g.width = v
            } else {
                g.height = v
            }
            let (old, new) = (f.corners(), g.corners());
            let moved = |p: DVec2| old.iter().position(|c| c.distance(p) < 1e-6 * (1.0 + c.length())).map_or(p, |k| new[k]);
            let edits: Vec<serde_json::Value> = lp
                .iter()
                .filter_map(|&k| match sk.entities.get(k as usize) {
                    Some(SketchEntity::Line { a, b }) => Some(json!([k, SketchEntity::Line { a: moved(*a), b: moved(*b) }])),
                    _ => None,
                })
                .collect();
            let _ = self.run("set_entity", json!({ "sketch": edit.sketch.0, "entities": edits }));
            return;
        }
        let new = match (e, edit.kind) {
            (SketchEntity::Rectangle { min, max }, DimKind::Width) => SketchEntity::Rectangle { min, max: DVec2::new(min.x + v, max.y) },
            (SketchEntity::Rectangle { min, max }, DimKind::Height) => SketchEntity::Rectangle { min, max: DVec2::new(max.x, min.y + v) },
            (SketchEntity::Line { a, b }, DimKind::Length) => SketchEntity::Line { a, b: a + (b - a).normalize_or(DVec2::X) * v },
            (SketchEntity::Circle { center, .. }, DimKind::Radius) => SketchEntity::Circle { center, radius: v },
            (SketchEntity::Circle { center, .. }, DimKind::Diameter) => SketchEntity::Circle { center, radius: v / 2.0 },
            (SketchEntity::Polygon { center, sides, angle, .. }, DimKind::Radius) => {
                SketchEntity::Polygon { center, radius: v, sides, angle }
            }
            (SketchEntity::Ellipse { center, ry, angle, .. }, DimKind::EllipseX) => SketchEntity::Ellipse { center, rx: v, ry, angle },
            (SketchEntity::Ellipse { center, rx, angle, .. }, DimKind::EllipseY) => SketchEntity::Ellipse { center, rx, ry: v, angle },
            _ => return,
        };
        let value = serde_json::to_value(&new).unwrap_or_default();
        let _ = self.run("set_entity", json!({ "sketch": edit.sketch.0, "entity": edit.entity, "value": value }));
    }
}

/// Degrees with up to four decimals ("120.9638°", "90°").
pub(in crate::workspace) fn format_degrees(d: f64) -> String {
    shapecraft_doc::format_angle(d)
}

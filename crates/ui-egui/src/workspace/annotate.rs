//! Annotating a drawing sheet: the Dimensions, Geometries and Note tools snap to the views' corners, straight
//! edges and circles; without a tool, clicking an annotation selects it (Delete removes it).

use egui::{Align2, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use glam::DVec2;
use serde_json::json;
use shapecraft_engine::drawing::{Drawing, Mark};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawTool {
    LineLength,
    PointToPoint,
    Radius,
    Diameter,
    Centerline,
    CenterMark,
    Note,
    PointToLine,
    LineToLine,
    LineAngle,
    ThreePointAngle,
    ArcAngle,
    LineCenterline,
    CircleCenterline2,
    CircleCenterline3,
    IntersectionMark,
    MinMax,
}

impl DrawTool {
    pub fn from_index(i: u8) -> Option<Self> {
        [
            Self::LineLength,
            Self::PointToPoint,
            Self::Radius,
            Self::Diameter,
            Self::Centerline,
            Self::CenterMark,
            Self::Note,
            Self::PointToLine,
            Self::LineToLine,
            Self::LineAngle,
            Self::ThreePointAngle,
            Self::ArcAngle,
            Self::LineCenterline,
            Self::CircleCenterline2,
            Self::CircleCenterline3,
            Self::IntersectionMark,
            Self::MinMax,
        ]
        .get(i as usize)
        .copied()
    }

    fn hint(self) -> (&'static str, &'static str) {
        match self {
            Self::LineLength => ("Line Length", "Click a straight edge of a view"),
            Self::PointToPoint => ("Point-to-Point Distance", "Click two corners or ends"),
            Self::Radius => ("Radius", "Click a circle or arc"),
            Self::Diameter => ("Diameter", "Click a circle"),
            Self::Centerline => ("2-Point Centerline", "Click two points"),
            Self::CenterMark => ("Center Mark", "Click a circle"),
            Self::Note => ("Note", "Click where the note goes, type, then press Return"),
            Self::PointToLine => ("Point-to-Line Distance", "Click a point, then a straight edge"),
            Self::LineToLine => ("Line-to-Line Distance", "Click two parallel edges"),
            Self::LineAngle => ("Line-to-Line Angle", "Click two straight edges"),
            Self::ThreePointAngle => ("3-Point Angle", "Click a point, the corner, then another point"),
            Self::ArcAngle => ("Arc Angle", "Click an arc"),
            Self::LineCenterline => ("2-Line Centerline", "Click two edges; the centerline runs between them"),
            Self::CircleCenterline2 => ("2-Point Circular Centerline", "Click the center, then a point on the circle"),
            Self::CircleCenterline3 => ("3-Point Circular Centerline", "Click three points on the circle"),
            Self::IntersectionMark => ("Intersection Mark", "Click two straight edges"),
            // Paraphrased from s16-036.
            Self::MinMax => ("Min-Max Distance", "Click a circle or arc, then a circle, arc or line"),
        }
    }
}

/// What the views offer to snap to, in sheet millimeters.
struct Snaps {
    points: Vec<DVec2>,
    segments: Vec<(DVec2, DVec2)>,
    circles: Vec<(DVec2, f64)>,
    /// Open round curves: center, radius, ends.
    arcs: Vec<(DVec2, f64, DVec2, DVec2)>,
}

fn circle_through(a: DVec2, b: DVec2, c: DVec2) -> Option<(DVec2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    let center = DVec2::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    Some((center, center.distance(a)))
}

fn snaps(d: &Drawing) -> Snaps {
    let mut s = Snaps { points: Vec::new(), segments: Vec::new(), circles: Vec::new(), arcs: Vec::new() };
    for l in d.views.iter().flat_map(|v| v.lines.iter()).filter(|l| !l.hidden) {
        let pts = &l.points;
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else { continue };
        let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        if len < 1e-6 {
            continue;
        }
        // Straight: every point near the chord.
        let chord = last - first;
        let straight = chord.length() > 1e-6 && pts.iter().all(|p| (chord.perp_dot(*p - first) / chord.length()).abs() < len * 1e-3 + 1e-6);
        if straight {
            s.segments.push((first, last));
            s.points.extend([first, last]);
            continue;
        }
        s.points.extend([first, last]);
        // Round: points on one circle.
        if pts.len() >= 6
            && let Some((c, r)) = circle_through(first, pts[pts.len() / 3], pts[2 * pts.len() / 3])
            && pts.iter().all(|p| (p.distance(c) - r).abs() < r * 0.02 + 1e-6)
        {
            s.circles.push((c, r));
            s.points.push(c);
            if first.distance(last) > r * 0.05 {
                s.arcs.push((c, r, first, last));
            }
        }
    }
    s
}

/// Where the lines through two segments cross.
fn line_crossing(s: (DVec2, DVec2), t: (DVec2, DVec2)) -> Option<DVec2> {
    let (d1, d2) = (s.1 - s.0, t.1 - t.0);
    let den = d1.perp_dot(d2);
    if den.abs() < 1e-9 * d1.length() * d2.length() {
        return None;
    }
    Some(s.0 + d1 * ((t.0 - s.0).perp_dot(d2) / den))
}

/// The foot of the perpendicular from `p` to the line through a segment.
fn foot(p: DVec2, s: (DVec2, DVec2)) -> DVec2 {
    let d = s.1 - s.0;
    s.0 + d * ((p - s.0).dot(d) / d.length_squared().max(1e-12))
}

fn seg_distance(q: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = if d.length_squared() > 0.0 { ((q - a).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + d * t).distance(q)
}

/// The dimension line goes outside the view: away from the view's middle.
fn outward(d: &Drawing, view: &str, a: DVec2, b: DVec2) -> f64 {
    let mid = d.views.iter().find(|v| v.name == view).map_or(a, |v| v.at);
    let n = (b - a).normalize_or_zero().perp();
    if n.dot((a + b) / 2.0 - mid) >= 0.0 { 8.0 } else { -8.0 }
}

impl Workspace {
    pub(super) fn start_draw_tool(&mut self, tool: DrawTool) {
        if let Some(d) = &mut self.drawings {
            d.tool = Some(tool);
            d.picks.clear();
            d.note = None;
            d.selected = None;
        }
        self.flyout = None;
    }

    fn annotate(&mut self, annotation: serde_json::Value) {
        let Some(id) = self.drawings.as_ref().map(|d| d.id) else { return };
        let _ = self.run("annotate", json!({ "drawing": id.0, "annotation": annotation }));
        if let Some(d) = &mut self.drawings {
            d.picks.clear();
        }
    }
}

/// Hover markers, clicks, the note field and the hint banner, over the drawn sheet (`to`/`from` map sheet mm to
/// the screen and back, `k` is pixels per mm).
pub(super) fn interact(ws: &mut Workspace, ui: &mut Ui, d: &Drawing, sheet: Rect, k: f32) {
    let to = |q: DVec2| pos2(sheet.left() + q.x as f32 * k, sheet.bottom() - q.y as f32 * k);
    let from = |p: Pos2| DVec2::new(((p.x - sheet.left()) / k) as f64, ((sheet.bottom() - p.y) / k) as f64);
    let resp = ui.interact(sheet, ui.scope_id().with("sheet"), Sense::click());
    let cursor = ui.input(|i| i.pointer.hover_pos()).filter(|p| sheet.contains(*p));
    let q = cursor.map(from);
    let reach = (12.0 / k) as f64;
    let p = ui.painter_at(ws.view_rect);
    let accent = theme::c(0xff9f2e);
    let Some(state) = ws.drawings.as_ref() else { return };
    let tool = state.tool;
    let picks = state.picks.clone();
    let snaps = snaps(d);
    let nearest_point =
        |q: DVec2| snaps.points.iter().copied().filter(|p| p.distance(q) < reach).min_by(|a, b| a.distance(q).total_cmp(&b.distance(q)));
    let nearest_segment = |q: DVec2| {
        snaps
            .segments
            .iter()
            .copied()
            .filter(|s| seg_distance(q, s.0, s.1) < reach)
            .min_by(|a, b| seg_distance(q, a.0, a.1).total_cmp(&seg_distance(q, b.0, b.1)))
    };
    let nearest_circle = |q: DVec2| {
        snaps
            .circles
            .iter()
            .copied()
            .filter(|c| (q.distance(c.0) - c.1).abs() < reach)
            .min_by(|a, b| (q.distance(a.0) - a.1).abs().total_cmp(&(q.distance(b.0) - b.1).abs()))
    };

    match tool {
        Some(t) => {
            let (title, body) = t.hint();
            let banner = Rect::from_center_size(pos2(ws.viewport_rect.center().x, ws.viewport_rect.top() + 34.0), vec2(330.0, 44.0));
            p.rect_filled(banner, radius(8), Tokens::banner());
            p.text(
                pos2(banner.left() + 14.0, banner.top() + 14.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(title),
                theme::strong(12.0),
                Tokens::text_strong(),
            );
            p.text(
                pos2(banner.left() + 14.0, banner.top() + 31.0),
                Align2::LEFT_CENTER,
                crate::i18n::tr_s(body),
                theme::body(11.0),
                Tokens::text_dim(),
            );
            // Min-Max keeps its first circle in the picks (drawn as the circle itself).
            for pk in picks.iter().filter(|_| t != DrawTool::MinMax) {
                p.circle_filled(to(*pk), 4.0, accent);
            }
            let Some(q) = q else { return };
            match t {
                DrawTool::LineLength => {
                    if let Some((a, b)) = nearest_segment(q) {
                        p.line_segment([to(a), to(b)], Stroke::new(3.0, accent));
                        if resp.clicked()
                            && let Some((view, _)) = d.sheet_to_view((a + b) / 2.0)
                            && let (Some(la), Some(lb)) = (d.sheet_to_named_view(view, a), d.sheet_to_named_view(view, b))
                        {
                            ws.annotate(json!({ "kind": "distance", "view": view, "a": la, "b": lb, "offset": outward(d, view, a, b) }));
                        }
                    }
                }
                DrawTool::PointToPoint | DrawTool::Centerline => {
                    if let Some(pt) = nearest_point(q) {
                        p.circle_stroke(to(pt), 6.0, Stroke::new(2.0, accent));
                        if let Some(first) = picks.first() {
                            p.line_segment([to(*first), to(pt)], Stroke::new(1.0, accent));
                        }
                        if resp.clicked() {
                            match picks.first().copied() {
                                None => {
                                    if let Some(st) = &mut ws.drawings {
                                        st.picks.push(pt);
                                    }
                                }
                                Some(a) if a.distance(pt) > 1e-6 => {
                                    if let Some((view, _)) = d.sheet_to_view(a)
                                        && let (Some(la), Some(lb)) = (d.sheet_to_named_view(view, a), d.sheet_to_named_view(view, pt))
                                    {
                                        if t == DrawTool::Centerline {
                                            ws.annotate(json!({ "kind": "centerline", "view": view, "a": la, "b": lb }));
                                        } else {
                                            ws.annotate(json!({ "kind": "distance", "view": view, "a": la, "b": lb, "offset": outward(d, view, a, pt) }));
                                        }
                                    }
                                }
                                Some(_) => {}
                            }
                        }
                    }
                }
                DrawTool::Radius | DrawTool::Diameter | DrawTool::CenterMark => {
                    if let Some((c, r)) = nearest_circle(q) {
                        p.circle_stroke(to(c), r as f32 * k, Stroke::new(3.0, accent));
                        if resp.clicked()
                            && let Some((view, lc)) = d.sheet_to_view(c)
                        {
                            let radius = r / d.scale;
                            if t == DrawTool::CenterMark {
                                ws.annotate(json!({ "kind": "center_mark", "view": view, "center": lc, "radius": radius }));
                            } else {
                                let angle = (q - c).to_angle();
                                ws.annotate(json!({ "kind": "radius", "view": view, "center": lc, "radius": radius, "angle": angle, "diameter": t == DrawTool::Diameter }));
                            }
                        }
                    }
                }
                DrawTool::PointToLine | DrawTool::LineToLine | DrawTool::LineAngle => {
                    // Point-to-Line takes a point first; the others a straight edge first. All end on an edge.
                    let first_is_point = t == DrawTool::PointToLine;
                    let have_first = if first_is_point { !picks.is_empty() } else { picks.len() >= 2 };
                    if !have_first && first_is_point {
                        if let Some(pt) = nearest_point(q) {
                            p.circle_stroke(to(pt), 6.0, Stroke::new(2.0, accent));
                            if resp.clicked()
                                && let Some(st) = &mut ws.drawings
                            {
                                st.picks.push(pt);
                            }
                        }
                    } else if let Some(seg) = nearest_segment(q) {
                        p.line_segment([to(seg.0), to(seg.1)], Stroke::new(3.0, accent));
                        if resp.clicked() {
                            if !have_first {
                                if let Some(st) = &mut ws.drawings {
                                    st.picks.extend([seg.0, seg.1]);
                                }
                            } else {
                                let (from_pt, first_seg) = if first_is_point {
                                    (picks[0], None)
                                } else {
                                    ((picks[0] + picks[1]) / 2.0, Some((picks[0], picks[1])))
                                };
                                let Some((view, _)) = d.sheet_to_view(from_pt) else { return };
                                let local = |q: DVec2| d.sheet_to_named_view(view, q);
                                match (t, first_seg) {
                                    (DrawTool::LineAngle, Some(s1)) => match line_crossing(s1, seg) {
                                        Some(v) => {
                                            let far = |s: (DVec2, DVec2)| if s.0.distance(v) > s.1.distance(v) { s.0 } else { s.1 };
                                            if let (Some(lv), Some(la), Some(lb)) = (local(v), local(far(s1)), local(far(seg))) {
                                                ws.annotate(json!({ "kind": "angle", "view": view, "vertex": lv, "a": la, "b": lb, "radius": 14.0 }));
                                            }
                                        }
                                        None => {
                                            ws.toast("Those edges are parallel");
                                            if let Some(st) = &mut ws.drawings {
                                                st.picks.clear();
                                            }
                                        }
                                    },
                                    _ => {
                                        let f = foot(from_pt, seg);
                                        if from_pt.distance(f) < 1e-6 {
                                            ws.toast("That point is on the line");
                                        } else if let (Some(la), Some(lb)) = (local(from_pt), local(f)) {
                                            ws.annotate(json!({ "kind": "distance", "view": view, "a": la, "b": lb, "offset": 0.0 }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                DrawTool::LineCenterline | DrawTool::IntersectionMark => {
                    if let Some(seg) = nearest_segment(q) {
                        p.line_segment([to(seg.0), to(seg.1)], Stroke::new(3.0, accent));
                        if resp.clicked() {
                            if picks.len() < 2 {
                                if let Some(st) = &mut ws.drawings {
                                    st.picks.extend([seg.0, seg.1]);
                                }
                            } else if let Some((view, _)) = d.sheet_to_view((picks[0] + picks[1]) / 2.0) {
                                let s1 = (picks[0], picks[1]);
                                let local = |q: DVec2| d.sheet_to_named_view(view, q);
                                if t == DrawTool::IntersectionMark {
                                    match line_crossing(s1, seg).and_then(local) {
                                        Some(at) => ws.annotate(json!({ "kind": "intersection_mark", "view": view, "at": at })),
                                        None => ws.toast("Those edges are parallel"),
                                    }
                                } else {
                                    // Pair the ends that face each other, then run between the pairs' middles.
                                    let s2 = if s1.0.distance(seg.0) + s1.1.distance(seg.1) <= s1.0.distance(seg.1) + s1.1.distance(seg.0) {
                                        seg
                                    } else {
                                        (seg.1, seg.0)
                                    };
                                    if let (Some(a), Some(b)) = (local((s1.0 + s2.0) / 2.0), local((s1.1 + s2.1) / 2.0)) {
                                        ws.annotate(json!({ "kind": "centerline", "view": view, "a": a, "b": b }));
                                    }
                                }
                                if let Some(st) = &mut ws.drawings {
                                    st.picks.clear();
                                }
                            }
                        }
                    }
                }
                DrawTool::CircleCenterline2 | DrawTool::CircleCenterline3 => {
                    let need = if t == DrawTool::CircleCenterline2 { 2 } else { 3 };
                    if let Some(pt) = nearest_point(q) {
                        p.circle_stroke(to(pt), 6.0, Stroke::new(2.0, accent));
                        if resp.clicked() {
                            if picks.len() + 1 < need {
                                if let Some(st) = &mut ws.drawings {
                                    st.picks.push(pt);
                                }
                            } else {
                                let circle = if need == 2 {
                                    Some((picks[0], picks[0].distance(pt)))
                                } else {
                                    circle_through(picks[0], picks[1], pt)
                                };
                                match circle {
                                    Some((c, r)) if r > 1e-6 => {
                                        if let Some((view, lc)) = d.sheet_to_view(c) {
                                            ws.annotate(
                                                json!({ "kind": "circular_centerline", "view": view, "center": lc, "radius": r / d.scale }),
                                            );
                                        }
                                    }
                                    _ => {
                                        ws.toast("Those points don't make a circle");
                                        if let Some(st) = &mut ws.drawings {
                                            st.picks.clear();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                DrawTool::ThreePointAngle => {
                    if let Some(pt) = nearest_point(q) {
                        p.circle_stroke(to(pt), 6.0, Stroke::new(2.0, accent));
                        if resp.clicked() {
                            if picks.len() < 2 {
                                if let Some(st) = &mut ws.drawings {
                                    st.picks.push(pt);
                                }
                            } else if let Some((view, _)) = d.sheet_to_view(picks[1])
                                && let (Some(la), Some(lv), Some(lb)) = (
                                    d.sheet_to_named_view(view, picks[0]),
                                    d.sheet_to_named_view(view, picks[1]),
                                    d.sheet_to_named_view(view, pt),
                                )
                            {
                                ws.annotate(json!({ "kind": "angle", "view": view, "vertex": lv, "a": la, "b": lb, "radius": 14.0 }));
                            }
                        }
                    }
                }
                DrawTool::ArcAngle => {
                    let near_arc = snaps
                        .arcs
                        .iter()
                        .copied()
                        .filter(|a| (q.distance(a.0) - a.1).abs() < reach)
                        .min_by(|a, b| (q.distance(a.0) - a.1).abs().total_cmp(&(q.distance(b.0) - b.1).abs()));
                    if let Some((c, r, a, b)) = near_arc {
                        p.circle_stroke(to(c), r as f32 * k, Stroke::new(1.0, accent));
                        p.circle_filled(to(a), 4.0, accent);
                        p.circle_filled(to(b), 4.0, accent);
                        if resp.clicked()
                            && let Some((view, _)) = d.sheet_to_view(c)
                            && let (Some(lc), Some(la), Some(lb)) =
                                (d.sheet_to_named_view(view, c), d.sheet_to_named_view(view, a), d.sheet_to_named_view(view, b))
                        {
                            ws.annotate(json!({ "kind": "angle", "view": view, "vertex": lc, "a": la, "b": lb, "radius": r + 6.0 }));
                        }
                    }
                }
                // Min-Max Distance (s16-036): a circle or arc, then a circle, arc or line. Which side of the first
                // circle was clicked decides: the near side gives the least distance, the far side the greatest
                // (unobserved: the reference's way of choosing wasn't seen).
                DrawTool::MinMax => {
                    let first = (picks.len() >= 3).then(|| (picks[0], picks[0].distance(picks[1]), picks[2]));
                    let second_circle = nearest_circle(q);
                    let second_line = if first.is_some() && second_circle.is_none() { nearest_segment(q) } else { None };
                    match (first, second_circle, second_line) {
                        (None, Some((c, r)), _) => {
                            p.circle_stroke(to(c), r as f32 * k, Stroke::new(3.0, accent));
                            if resp.clicked()
                                && let Some(st) = &mut ws.drawings
                            {
                                st.picks = vec![c, c + DVec2::new(r, 0.0), q];
                            }
                        }
                        (Some((c1, r1, q1)), circle, line) if circle.is_some() || line.is_some() => {
                            p.circle_stroke(to(c1), r1 as f32 * k, Stroke::new(2.0, accent));
                            // The two ends of the dimension, nearest first.
                            let ends = match (circle, line) {
                                (Some((c2, r2)), _) => {
                                    p.circle_stroke(to(c2), r2 as f32 * k, Stroke::new(3.0, accent));
                                    let u = (c2 - c1).normalize_or_zero();
                                    let far = (q1 - c1).dot(u) < 0.0;
                                    (u != DVec2::ZERO).then(|| if far { (c1 - u * r1, c2 + u * r2) } else { (c1 + u * r1, c2 - u * r2) })
                                }
                                (None, Some(seg)) => {
                                    p.line_segment([to(seg.0), to(seg.1)], Stroke::new(3.0, accent));
                                    let f = foot(c1, seg);
                                    let u = (f - c1).normalize_or_zero();
                                    let far = (q1 - c1).dot(u) < 0.0;
                                    (u != DVec2::ZERO).then(|| (if far { c1 - u * r1 } else { c1 + u * r1 }, f))
                                }
                                _ => None,
                            };
                            if let Some((a, b)) = ends {
                                p.line_segment([to(a), to(b)], Stroke::new(1.0, accent));
                                if resp.clicked() {
                                    match d.sheet_to_view(c1) {
                                        Some((view, _)) if a.distance(b) > 1e-6 => {
                                            if let (Some(la), Some(lb)) = (d.sheet_to_named_view(view, a), d.sheet_to_named_view(view, b)) {
                                                ws.annotate(json!({ "kind": "distance", "view": view, "a": la, "b": lb, "offset": 0.0 }));
                                            }
                                        }
                                        _ => ws.toast("Those touch"),
                                    }
                                    if let Some(st) = &mut ws.drawings {
                                        st.picks.clear();
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                DrawTool::Note => {
                    if resp.clicked()
                        && let Some(st) = &mut ws.drawings
                        && st.note.is_none()
                    {
                        st.note = Some((q, String::new(), true));
                    }
                }
            }
        }
        None => {
            // Select an annotation by its marks.
            let hit = |q: DVec2| {
                d.annotations.iter().position(|a| {
                    d.marks_for(a).iter().any(|m| match m {
                        Mark::Line { a, b, .. } => seg_distance(q, *a, *b) < reach * 0.6,
                        Mark::Text { at, text, size, .. } => {
                            let w = text.chars().count() as f64 * size * 0.6;
                            q.x > at.x - w && q.x < at.x + w && (q.y - at.y).abs() < size * 1.2
                        }
                        Mark::Image { min, max, .. } => q.cmpge(*min).all() && q.cmple(*max).all(),
                    })
                })
            };
            let hovered = q.and_then(hit);
            let selected = ws.drawings.as_ref().and_then(|s| s.selected);
            for (i, a) in d.annotations.iter().enumerate() {
                if Some(i) == hovered || Some(i) == selected {
                    let col = if Some(i) == selected { theme::c(0x3f8cff) } else { accent };
                    for m in d.marks_for(a) {
                        match m {
                            Mark::Line { a, b, .. } => {
                                p.line_segment([to(a), to(b)], Stroke::new(2.0, col));
                            }
                            Mark::Text { at, text, size, center, .. } => {
                                let align = if center { Align2::CENTER_CENTER } else { Align2::LEFT_BOTTOM };
                                p.text(to(at), align, text, theme::body((size as f32 * k * 1.25).max(6.0)), col);
                            }
                            Mark::Image { min, max, .. } => {
                                p.rect_stroke(Rect::from_two_pos(to(min), to(max)), 0.0, Stroke::new(2.0, col), egui::StrokeKind::Outside);
                            }
                        }
                    }
                }
            }
            // A picked picture follows a drag.
            if let Some(i) = selected
                && let Some(shapecraft_doc::Annotation::Image { at, width, .. }) = d.annotations.get(i)
                && resp.dragged()
                && hovered == Some(i)
            {
                let delta = resp.drag_delta();
                let moved = *at + DVec2::new(delta.x as f64 / k as f64, -delta.y as f64 / k as f64);
                let id = ws.drawings.as_ref().map(|s| s.id.0);
                if let Some(id) = id {
                    ws.live_run("insert_sheet_image", json!({ "drawing": id, "index": i, "at": [moved.x, moved.y], "width": width }));
                }
            }
            if resp.drag_stopped() {
                ws.live_step = None;
            }
            // Double-clicking a dimension opens its card (s14-127).
            if resp.double_clicked()
                && let Some(i) = hovered
                && let Some(text) = d.annotations.get(i).and_then(dim_text)
                && let Some(st) = &mut ws.drawings
            {
                let at = cursor.unwrap_or(sheet.center());
                st.dim_card = Some(DimCard::new(i, text, at, ws.now));
            }
            if resp.clicked()
                && let Some(st) = &mut ws.drawings
            {
                if st.dim_card.as_ref().is_some_and(|c| Some(c.index) != hovered) {
                    st.dim_card = None;
                }
                st.selected = hovered;
                // Clicking a view (not one of its annotations) selects the view.
                st.view_sel = if hovered.is_none() { q.and_then(|q| d.view_at(q)).map(|v| v.name.clone()) } else { None };
            }
        }
    }
    // The note being typed.
    let note = ws.drawings.as_mut().and_then(|s| s.note.take());
    if let Some((at, mut text, focus)) = note {
        let field = Rect::from_min_size(to(at) - vec2(0.0, 22.0), vec2(220.0, 22.0));
        let id = ui.scope_id().with("note-field");
        let r = ui.put(field, egui::TextEdit::singleline(&mut text).id(id).hint_text(crate::i18n::tr("Note")).font(theme::body(13.0)));
        if focus {
            r.request_focus();
        }
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if enter && !text.trim().is_empty() {
            ws.annotate(json!({ "kind": "note", "at": at, "text": text }));
        } else if !esc
            && !(r.lost_focus() && text.trim().is_empty())
            && let Some(st) = &mut ws.drawings
        {
            st.note = Some((at, text, false));
        }
    }
}

/// A dimension's prefix, suffix and tolerance (None for annotations that aren't dimensions).
fn dim_text(a: &shapecraft_doc::Annotation) -> Option<&shapecraft_doc::DimText> {
    use shapecraft_doc::Annotation as A;
    match a {
        A::Distance { text, .. } | A::Radius { text, .. } | A::Angle { text, .. } => Some(text),
        _ => None,
    }
}

/// The card a double-clicked dimension opens (s14-127..131): a grab bar, then Prefix Text, Tolerances and Suffix
/// Text, each a checkbox that opens its fields. The fields are typed here and saved on Return or leaving them.
pub struct DimCard {
    pub index: usize,
    pub at: Pos2,
    pub opened: f64,
    text: shapecraft_doc::DimText,
    prefix: String,
    suffix: String,
    upper: String,
    lower: String,
}

/// The symbols the prefix field offers (s14-128): minus, plus-minus, times, diameter, degree, arc, countersink,
/// counterbore, depth, slope, square.
const SYMBOLS: [&str; 11] = ["−", "±", "×", "⌀", "°", "⌒", "⌵", "⌴", "↧", "◺", "□"];

impl DimCard {
    fn new(index: usize, text: &shapecraft_doc::DimText, at: Pos2, now: f64) -> Self {
        let t = text.tolerance.as_ref();
        Self {
            index,
            at,
            opened: now,
            text: text.clone(),
            prefix: text.prefix.clone().unwrap_or_default(),
            suffix: text.suffix.clone().unwrap_or_default(),
            upper: format!("{:+.2}", t.map_or(0.0, |t| t.upper)),
            lower: format!("{:+.2}", -t.map_or(0.0, |t| t.lower.abs())),
        }
    }
}

fn checkbox(p: &egui::Painter, at: Pos2, on: bool) {
    let r = Rect::from_center_size(at, vec2(14.0, 14.0));
    if on {
        p.rect_filled(r, radius(3), theme::c(0x3b5bdb));
        p.line_segment([at + vec2(-3.5, 0.0), at + vec2(-1.0, 3.0)], Stroke::new(1.8, egui::Color32::WHITE));
        p.line_segment([at + vec2(-1.0, 3.0), at + vec2(4.0, -3.5)], Stroke::new(1.8, egui::Color32::WHITE));
    } else {
        p.rect_filled(r, radius(3), theme::c(0x4a4a4d));
    }
}

/// Draws the open dimension card and applies its changes.
pub(super) fn dim_card(ws: &mut Workspace, ui: &mut Ui) {
    use shapecraft_doc::{Tolerance, ToleranceKind as K};
    let Some(st) = ws.drawings.as_mut() else { return };
    let drawing = st.id;
    let Some(card) = st.dim_card.as_mut() else { return };
    const W: f32 = 300.0;
    let parse = |t: &str| t.trim().replace(',', ".").replace('−', "-").parse::<f64>().ok().filter(|v| v.is_finite());
    let mut changed = false;
    let mut close = false;
    let bounds = ws.view_rect;
    let origin = pos2((card.at.x + 30.0).min(bounds.right() - W - 8.0), (card.at.y + 12.0).min(bounds.bottom() - 330.0));
    let shown = egui::Area::new(ui.scope_id().with("dim-card")).fixed_pos(origin).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        ui.set_width(W);
        let frame = egui::Frame::new().fill(theme::c(0x232325)).corner_radius(radius(10)).inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 8,
            bottom: 12,
        });
        frame.show(ui, |ui| {
            ui.set_width(W - 32.0);
            let (bar, _) = ui.allocate_exact_size(vec2(W - 32.0, 12.0), Sense::hover());
            ui.painter().rect_filled(Rect::from_center_size(bar.center(), vec2(36.0, 5.0)), radius(3), theme::c(0x5a5a5e));
            let section = |ui: &mut Ui, label: &str, on: bool| -> bool {
                let (row, resp) = ui.allocate_exact_size(vec2(W - 32.0, 28.0), Sense::click());
                checkbox(ui.painter(), pos2(row.left() + 7.0, row.center().y), on);
                ui.painter().text(
                    pos2(row.left() + 22.0, row.center().y),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr_s(label),
                    theme::strong(12.5),
                    if on { Tokens::text_strong() } else { Tokens::text_dim() },
                );
                resp.clicked()
            };
            let field = |ui: &mut Ui, text: &mut String, id: &str| -> bool {
                let r = ui.add(egui::TextEdit::singleline(text).id(ui.scope_id().with(id)).desired_width(W - 32.0).font(theme::body(12.5)));
                r.lost_focus() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            };
            let line = |ui: &mut Ui| {
                let (r, _) = ui.allocate_exact_size(vec2(W - 32.0, 9.0), Sense::hover());
                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(1.0, theme::c(0x3a3a3d)));
            };
            // Prefix Text.
            if section(ui, "Prefix Text", card.text.prefix.is_some()) {
                card.text.prefix = if card.text.prefix.is_some() { None } else { Some(card.prefix.clone()) };
                changed = true;
            }
            if card.text.prefix.is_some() {
                if field(ui, &mut card.prefix, "dim-prefix") {
                    card.text.prefix = Some(card.prefix.clone());
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for sym in SYMBOLS {
                        let (r, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, radius(4), theme::c(0x3a3a3d));
                        }
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, sym, theme::body(12.0), Tokens::text_strong());
                        if resp.clicked() {
                            card.prefix.push_str(sym);
                            card.text.prefix = Some(card.prefix.clone());
                            changed = true;
                        }
                    }
                });
            }
            line(ui);
            // Tolerances.
            if section(ui, "Tolerances", card.text.tolerance.is_some()) {
                card.text.tolerance = match card.text.tolerance {
                    Some(_) => None,
                    None => Some(Tolerance {
                        kind: K::Symmetrical,
                        upper: parse(&card.upper).unwrap_or(0.0),
                        lower: parse(&card.lower).unwrap_or(0.0),
                    }),
                };
                changed = true;
            }
            if let Some(t) = card.text.tolerance.as_mut() {
                let names = ["Symmetrical", "Deviation", "Limits", "Basic"];
                let kinds = [K::Symmetrical, K::Deviation, K::Limits, K::Basic];
                let current = kinds.iter().position(|k| *k == t.kind);
                let (row, _) = ui.allocate_exact_size(vec2(W - 32.0, 26.0), Sense::hover());
                let name = names[current.unwrap_or(0)];
                let width = ui.painter().layout_no_wrap(name.to_string(), theme::body(13.0), Tokens::text()).size().x;
                if let Some(i) = super::popover::pill_select(
                    ui,
                    "dim-tol-kind",
                    pos2(row.left() + width + 30.0, row.center().y),
                    0.0,
                    &names.map(String::from),
                    current,
                ) {
                    t.kind = kinds[i];
                    changed = true;
                }
                let labelled = |ui: &mut Ui, label: &str, text: &mut String, id: &str| -> Option<f64> {
                    ui.label(egui::RichText::new(crate::i18n::tr(label)).size(11.0).color(Tokens::text_dim()));
                    if field(ui, text, id) { parse(text) } else { None }
                };
                match t.kind {
                    K::Symmetrical => {
                        if let Some(v) = labelled(ui, "Limits", &mut card.upper, "dim-sym") {
                            t.upper = v.abs();
                            t.lower = v.abs();
                            changed = true;
                        }
                    }
                    K::Deviation | K::Limits => {
                        if let Some(v) = labelled(ui, "Upper Limit", &mut card.upper, "dim-up") {
                            t.upper = v;
                            changed = true;
                        }
                        if let Some(v) = labelled(ui, "Lower Limit", &mut card.lower, "dim-low") {
                            t.lower = v.abs();
                            changed = true;
                        }
                    }
                    K::Basic => {}
                }
            }
            line(ui);
            // Suffix Text.
            if section(ui, "Suffix Text", card.text.suffix.is_some()) {
                card.text.suffix = if card.text.suffix.is_some() { None } else { Some(card.suffix.clone()) };
                changed = true;
            }
            if card.text.suffix.is_some() && field(ui, &mut card.suffix, "dim-suffix") {
                card.text.suffix = Some(card.suffix.clone());
                changed = true;
            }
        });
    });
    let rect = shown.response.rect;
    if ws.now - card.opened > 0.3 && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    // A click on the sheet closes it (not one in its own pop-up list).
    let ctx = ui.ctx().clone();
    let outside = ws.now - card.opened > 0.3
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|q| !rect.contains(q) && ctx.layer_id_at(q).is_none_or(|l| l.order < egui::Order::Foreground))
        });
    let index = card.index;
    let text = card.text.clone();
    if changed {
        let _ = ws.run("update_annotation", json!({ "drawing": drawing.0, "index": index, "text": text }));
    }
    if (close || outside)
        && let Some(st) = ws.drawings.as_mut()
    {
        st.dim_card = None;
    }
}

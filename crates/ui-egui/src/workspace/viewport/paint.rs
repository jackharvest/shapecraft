//! Painter helpers for what is drawn over the view: dimension lines and labels, value fields, pills, arrows and
//! badges.

use egui::{Align2, Color32, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

use super::dimensions::{ANGLE_RADIUS, AngleDim, format_degrees};
use crate::theme::{self, Tokens, radius};

/// Where `dimension` puts its label for the edge a→b.
pub(super) fn dim_label_pos(a: Pos2, b: Pos2, offset: f32) -> Pos2 {
    let d = b - a;
    if d.length() < 1e-3 {
        return a;
    }
    let n = vec2(-d.y, d.x).normalized();
    let n = if n.y > 0.0 || (n.y == 0.0 && n.x < 0.0) { -n } else { n };
    a + d * 0.5 + n * offset
}

fn hex_text() -> Color32 {
    theme::c(0xd7d7d7)
}

/// `Dim::line` offset meaning "a radius from the first point to the second".
pub(super) const RADIAL: f32 = -1.0;
/// A value on a leader from outside: the arrow at `tip` (a corner), the line running on away from `from` (the
/// center) with the boxed value at its end, turned along it.
pub(super) fn leader_dim(painter: &egui::Painter, from: Pos2, tip: Pos2, text: &str, typed: Option<&str>) {
    let d = tip - from;
    if d.length() < 2.0 {
        return;
    }
    let u = d.normalized();
    let pp = vec2(-u.y, u.x);
    let white = Color32::WHITE;
    let tail = tip + u * 48.0;
    painter.line_segment([tip, tail], Stroke::new(1.0, white));
    painter.add(egui::Shape::convex_polygon(vec![tip, tip + u * 9.0 + pp * 3.5, tip + u * 9.0 - pp * 3.5], white, Stroke::NONE));
    let half = text.chars().count() as f32 * 3.6 + 5.0;
    match typed {
        Some(t) => value_field(painter, tail + u * 40.0, t),
        None => sketch_dimension_label(painter, tail + u * half, u, text, true),
    }
}

/// A radius drawn dashed from the center to `b`, its value beside the middle (a placed polygon, s11-066).
pub(super) fn dashed_radius(painter: &egui::Painter, a: Pos2, b: Pos2, text: &str, typed: Option<&str>) {
    let d = b - a;
    if d.length() < 4.0 {
        return;
    }
    painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, Color32::WHITE), 4.0, 3.0));
    let u = d.normalized();
    let pp = vec2(-u.y, u.x);
    let side = if pp.y > 0.0 { -pp } else { pp };
    match typed {
        Some(t) => value_field(painter, a + d * 0.5, t),
        None => sketch_dimension_label(painter, a + d * 0.5 + side * 9.0, u, text, false),
    }
}

/// `Dim::line` offset meaning "a dashed radius from the center (first) to a corner (second)".
pub(super) const DASHED_RADIUS: f32 = -3.0;

/// `Dim::line` offset meaning "a diameter across from the first point to the second".
pub(super) const DIAMETER: f32 = -2.0;

/// A circle's diameter as 26.200 shows it (s11-060/061): a dashed line across, arrowheads at the rim, the value
/// turned along it a little past the middle toward `b` (boxed while live).
pub(super) fn diameter_dim(painter: &egui::Painter, a: Pos2, b: Pos2, text: &str, typed: Option<&str>, live: bool) {
    let d = b - a;
    if d.length() < 4.0 {
        return;
    }
    let u = d.normalized();
    let pp = vec2(-u.y, u.x);
    let white = Color32::WHITE;
    painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, white), 4.0, 3.0));
    for (tip, s) in [(a, 1.0f32), (b, -1.0)] {
        painter.add(egui::Shape::convex_polygon(
            vec![tip, tip + u * 9.0 * s + pp * 3.5, tip + u * 9.0 * s - pp * 3.5],
            white,
            Stroke::NONE,
        ));
    }
    let at = a + d * 0.775;
    match typed {
        Some(t) => value_field(painter, at, t),
        None => sketch_dimension_label(painter, at, u, text, live),
    }
}

/// A radius as observed on ellipses: a thin line from the center `a` to the rim `b`, an arrowhead at the rim, and
/// the label running along the line beside its middle (the live one in the value box).
pub(super) fn radius_dim(painter: &egui::Painter, a: Pos2, b: Pos2, text: &str, typed: Option<&str>, live: bool) {
    let d = b - a;
    if d.length() < 2.0 {
        return;
    }
    // 26.200 (s11-063): a white line center→rim with an arrowhead at the rim; the live value boxed on the line, a
    // settled one just beside it, both turned along it.
    let col = Color32::WHITE;
    let u = d.normalized();
    let perp = vec2(-u.y, u.x);
    painter.line_segment([a, b - u * 6.0], Stroke::new(1.0, col));
    painter.add(egui::Shape::convex_polygon(vec![b, b - u * 9.0 + perp * 3.5, b - u * 9.0 - perp * 3.5], col, Stroke::NONE));
    let mid = a + d * 0.5;
    if let Some(t) = typed {
        value_field(painter, mid, t);
        return;
    }
    let side = if perp.y > 0.0 { -perp } else { perp };
    sketch_dimension_label(painter, if live { mid } else { mid + side * 9.0 }, u, text, live);
}

/// A sketch dimension as 26.200 draws it: extension lines from the ends `ends` out to `dim`, the dimension line
/// with arrowheads, and the value along it, upright. `live` puts the value in a black box with a blue rim; typing
/// shows the value field instead.
pub(super) fn sketch_dimension(painter: &egui::Painter, ends: [Pos2; 2], dim: [Pos2; 2], text: &str, typed: Option<&str>, live: bool) {
    let d = dim[1] - dim[0];
    if d.length() < 2.0 {
        return;
    }
    let col = Color32::WHITE;
    let u = d.normalized();
    for i in 0..2 {
        let out = (dim[i] - ends[i]).normalized();
        painter.line_segment([ends[i] + out * 3.0, dim[i] + out * 5.0], Stroke::new(1.0, col));
    }
    painter.line_segment([dim[0], dim[1]], Stroke::new(1.0, col));
    let perp = vec2(-u.y, u.x);
    for (p, s) in [(dim[0], 1.0f32), (dim[1], -1.0)] {
        painter.add(egui::Shape::convex_polygon(vec![p, p + u * 9.0 * s + perp * 3.5, p + u * 9.0 * s - perp * 3.5], col, Stroke::NONE));
    }
    let mid = dim[0] + d * 0.5;
    if let Some(t) = typed {
        value_field(painter, mid, t);
        return;
    }
    let mut angle = u.y.atan2(u.x);
    let q = std::f32::consts::FRAC_PI_2;
    if angle > q {
        angle -= std::f32::consts::PI;
    } else if angle < -q + 0.01 {
        // Vertical values read top to bottom (s11-053).
        angle += std::f32::consts::PI;
    }
    let (ax, ay) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
    let galley = painter.layout_no_wrap(text.to_string(), theme::mono(12.0), col);
    let size = galley.size();
    if live {
        let (hw, hh) = (size.x / 2.0 + 5.0, size.y / 2.0 + 2.0);
        let corners = vec![mid - ax * hw - ay * hh, mid + ax * hw - ay * hh, mid + ax * hw + ay * hh, mid - ax * hw + ay * hh];
        painter.add(egui::Shape::convex_polygon(corners, Color32::BLACK, Stroke::new(1.5, theme::c(0x3d55e0))));
    }
    // Plain (a placed line's) labels sit just above the dimension line; the live one covers it.
    let lift = if live { 0.0 } else { size.y / 2.0 + 2.0 };
    let pos = mid - ax * (size.x / 2.0) - ay * (size.y / 2.0 + lift);
    painter.add(egui::epaint::TextShape::new(pos, galley, col).with_angle(angle));
}

/// A small white padlock in a dark disc (a fixed value, s11-035).
pub(super) fn padlock(painter: &egui::Painter, at: Pos2) {
    painter.circle_filled(at, 8.0, Color32::from_black_alpha(200));
    let body = Rect::from_center_size(at + vec2(0.0, 1.5), vec2(7.0, 5.5));
    painter.rect_filled(body, radius(1), Color32::WHITE);
    painter.add(egui::Shape::Path(egui::epaint::PathShape::line(
        (0..=8)
            .map(|i| {
                let t = std::f32::consts::PI * i as f32 / 8.0;
                at + vec2(-2.3 * t.cos(), -1.0 - 2.6 * t.sin())
            })
            .collect(),
        Stroke::new(1.3, Color32::WHITE),
    )));
}

/// [`sketch_dimension`] for an edge a→b on screen, `offset` px out on the outside (up or right; negative: down or
/// left), like the old screen-space dimensions.
pub(super) fn screen_dim(painter: &egui::Painter, a: Pos2, b: Pos2, offset: f32, text: &str, typed: Option<&str>, live: bool) {
    if (b - a).length() < 2.0 {
        return;
    }
    let n = vec2(-(b - a).y, (b - a).x).normalized();
    let n = if n.y > 0.0 || (n.y == 0.0 && n.x < 0.0) { -n } else { n };
    sketch_dimension(painter, [a, b], [a + n * offset, b + n * offset], text, typed, live);
}

/// A dimension's value at `mid`, turned along `u` and kept upright; `live` boxes it (black, blue rim).
pub(super) fn sketch_dimension_label(painter: &egui::Painter, mid: Pos2, u: egui::Vec2, text: &str, live: bool) {
    let mut angle = u.y.atan2(u.x);
    let q = std::f32::consts::FRAC_PI_2;
    if angle > q {
        angle -= std::f32::consts::PI;
    } else if angle < -q + 0.01 {
        // Vertical values read top to bottom (s11-053).
        angle += std::f32::consts::PI;
    }
    let (ax, ay) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
    let galley = painter.layout_no_wrap(text.to_string(), theme::mono(12.0), Color32::WHITE);
    let size = galley.size();
    if live {
        let (hw, hh) = (size.x / 2.0 + 5.0, size.y / 2.0 + 2.0);
        let corners = vec![mid - ax * hw - ay * hh, mid + ax * hw - ay * hh, mid + ax * hw + ay * hh, mid - ax * hw + ay * hh];
        painter.add(egui::Shape::convex_polygon(corners, Color32::BLACK, Stroke::new(1.5, theme::c(0x3d55e0))));
    }
    let pos = mid - ax * (size.x / 2.0) - ay * (size.y / 2.0);
    painter.add(egui::epaint::TextShape::new(pos, galley, Color32::WHITE).with_angle(angle));
}

/// The value field opened by typing (26.200, s11-014): a dark rounded panel holding a blue-rimmed field with the
/// text, an "x+" variable glyph at its right end, and a "123" keypad key beside it.
pub(super) fn value_field(painter: &egui::Painter, center: Pos2, typed: &str) {
    // See-through, as in 26.202 (s52r-044: the panel reads #282931 over the dark view and the field takes on the
    // colour of a face behind it): the panel #30323d at 64 %, the field a light veil over it.
    let alpha = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    let panel = Rect::from_center_size(center + vec2(13.0, 0.0), vec2(214.0, 34.0));
    painter.rect_filled(panel, radius(9), alpha(theme::c(0x30323d), 165));
    let field = Rect::from_min_size(panel.min + vec2(4.0, 4.0), vec2(176.0, 26.0));
    painter.rect_filled(field, radius(7), alpha(theme::c(0x5f6a86), 51));
    painter.rect_stroke(field, radius(7), Stroke::new(2.0, theme::c(0x2f4793)), StrokeKind::Inside);
    let g = painter.layout_no_wrap(typed.to_string(), theme::body(14.0), Tokens::text_strong());
    let tw = g.size().x;
    let text_at = pos2(field.left() + 8.0, field.center().y - g.size().y / 2.0);
    // Filled from a clicked pill: the text shows selected (the next key replaces it), no caret.
    let fresh = painter.ctx().data(|d| d.get_temp::<bool>(egui::Id::unique("sc-numeric-fresh"))).unwrap_or(false);
    if fresh && tw > 0.0 {
        let sel = Rect::from_min_size(text_at - vec2(1.0, 1.0), g.size() + vec2(2.0, 2.0));
        painter.rect_filled(sel, radius(2), Tokens::accent().gamma_multiply(0.55));
    }
    painter.galley(text_at, g, Tokens::text_strong());
    // The caret right after the text.
    let cx = field.left() + 9.0 + tw;
    if !fresh {
        painter.line_segment([pos2(cx, field.top() + 6.0), pos2(cx, field.bottom() - 6.0)], Stroke::new(1.5, Tokens::accent()));
    }
    let ink = theme::c(0xd8d8d8);
    let x = pos2(field.right() - 15.0, field.center().y);
    painter.text(x, Align2::CENTER_CENTER, "x", egui::FontId::new(15.0, egui::FontFamily::Proportional), ink);
    painter.circle_filled(x + vec2(5.0, 4.0), 4.0, ink);
    painter.line_segment([x + vec2(3.0, 4.0), x + vec2(7.0, 4.0)], Stroke::new(1.2, theme::c(0x3a3c48)));
    painter.line_segment([x + vec2(5.0, 2.0), x + vec2(5.0, 6.0)], Stroke::new(1.2, theme::c(0x3a3c48)));
    let key = Rect::from_center_size(pos2(panel.right() - 16.0, panel.center().y), vec2(18.0, 13.0));
    painter.rect_stroke(key, radius(2), Stroke::new(1.0, ink), StrokeKind::Inside);
    painter.text(key.center(), Align2::CENTER_CENTER, "123", egui::FontId::new(6.5, egui::FontFamily::Proportional), ink);
}

/// Where this frame's value pills were drawn, with their text: a click on one opens the value field with that
/// value in it (some people click rather than type).
pub(in crate::workspace) fn value_pills(ctx: &egui::Context) -> Vec<(Rect, String)> {
    ctx.data(|d| d.get_temp::<Vec<(Rect, String)>>(egui::Id::unique("sc-value-pills"))).unwrap_or_default()
}

pub(super) fn clear_value_pills(ctx: &egui::Context, fresh: bool) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique("sc-value-pills"), Vec::<(Rect, String)>::new());
        d.insert_temp(egui::Id::unique("sc-numeric-fresh"), fresh);
    });
}

/// A blue-rimmed pill holding a number is a value the running tool takes.
fn note_value_pill(painter: &egui::Painter, r: Rect, text: &str) {
    if text.chars().any(|c| c.is_ascii_digit()) {
        painter
            .ctx()
            .data_mut(|d| d.get_temp_mut_or_default::<Vec<(Rect, String)>>(egui::Id::unique("sc-value-pills")).push((r, text.to_string())));
    }
}

/// The number in a pill's text ("R 2mm" → "2", "⌀10 mm" → "10", "-3.5 mm" → "-3.5").
pub(in crate::workspace) fn pill_number(text: &str) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit() || c == '-' || c == '.')?;
    let n: String = text[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.').collect();
    n.parse::<f64>().ok().filter(|v| v.is_finite()).map(|_| n)
}

/// The value label. While typing it becomes the editable box with a blue border.
/// A white value pill whose text runs along `dir` (screen space), kept upright. Returns its bounds.
pub(super) fn axis_pill(painter: &egui::Painter, center: Pos2, dir: egui::Vec2, text: &str, blue_edge: bool) -> Rect {
    // Keep text upright; near-vertical text reads upwards, as in the reference.
    let mut angle = dir.y.atan2(dir.x);
    let q = std::f32::consts::FRAC_PI_2;
    if angle > q - 0.05 {
        angle -= std::f32::consts::PI;
    } else if angle < -q - 0.05 {
        angle += std::f32::consts::PI;
    }
    let ink = PILL_INK;
    let galley = painter.layout_no_wrap(text.to_string(), theme::strong(10.0), ink);
    let (w, h) = (galley.size().x + 12.0, 17.0);
    let (ax, ay) = (vec2(angle.cos(), angle.sin()), vec2(-angle.sin(), angle.cos()));
    let corners: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, y)| center + ax * (w / 2.0 * x) + ay * (h / 2.0 * y))
        .collect();
    // A capsule (round ends), with the blue edge drawn as a slightly bigger capsule behind it.
    let capsule = |r: f32, fill: Color32| {
        let half = (w / 2.0 - h / 2.0).max(0.0);
        let (a, b) = (center - ax * half, center + ax * half);
        painter.circle_filled(a, r, fill);
        painter.circle_filled(b, r, fill);
        painter.add(egui::Shape::convex_polygon(vec![a + ay * r, a - ay * r, b - ay * r, b + ay * r], fill, Stroke::NONE));
    };
    if blue_edge {
        capsule(h / 2.0 + 1.8, Tokens::accent());
    }
    capsule(h / 2.0, Color32::WHITE);
    let size = galley.size();
    let top_left = center - ax * (size.x / 2.0) - ay * (size.y / 2.0);
    painter.add(egui::epaint::TextShape::new(top_left, galley, ink).with_angle(angle));
    let r = Rect::from_points(&corners);
    if blue_edge {
        note_value_pill(painter, r, text);
    }
    r
}

/// The "Copy" pill under the Move gizmo: white, or blue while on.
pub(super) fn copy_pill(painter: &egui::Painter, center: Pos2, on: bool) -> Rect {
    let r = Rect::from_center_size(center, vec2(40.0, 17.0));
    painter.rect_filled(r, radius(5), if on { Tokens::accent() } else { Color32::WHITE });
    painter.text(r.center(), Align2::CENTER_CENTER, "Copy", theme::strong(10.0), if on { Color32::WHITE } else { PILL_INK });
    r
}

/// The Link badge right of a lit Copy pill (s29-019/022): a blue tile with a white chain while copies stay linked
/// (the default), white with a dark chain once unlinked.
pub(super) fn link_pill(painter: &egui::Painter, center: Pos2, linked: bool) -> Rect {
    let r = Rect::from_center_size(center, vec2(19.0, 17.0));
    painter.rect_filled(r, radius(5), if linked { Tokens::accent() } else { Color32::WHITE });
    let ink = if linked { Color32::WHITE } else { PILL_INK };
    // Two links on a diagonal, each a capsule outline.
    let d = vec2(1.0, -1.0).normalized();
    for (a, b) in [(-5.0f32, -0.6f32), (0.6, 5.0)] {
        let (p, q) = (center + d * a, center + d * b);
        let n = vec2(-d.y, d.x);
        let mut pts = Vec::new();
        for i in 0..=8 {
            let t = std::f32::consts::PI * i as f32 / 8.0;
            pts.push(q + (n * t.cos() + d * t.sin()) * 2.2);
        }
        for i in 0..=8 {
            let t = std::f32::consts::PI * i as f32 / 8.0;
            pts.push(p - (n * t.cos() + d * t.sin()) * 2.2);
        }
        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.3, ink)));
    }
    r
}

/// A thick double-headed arrow centered on `c` along `dir` (unit, screen space), with a thin outline.
pub(super) fn double_arrow(painter: &egui::Painter, c: Pos2, dir: egui::Vec2, half: f32, fill: Color32, outline: Color32) {
    let perp = vec2(-dir.y, dir.x);
    let head = 7.0;
    let shaft = 2.2;
    let pts = vec![
        c + dir * half,
        c + dir * (half - head) + perp * head,
        c + dir * (half - head) + perp * shaft,
        c - dir * (half - head) + perp * shaft,
        c - dir * (half - head) + perp * head,
        c - dir * half,
        c - dir * (half - head) - perp * head,
        c - dir * (half - head) - perp * shaft,
        c + dir * (half - head) - perp * shaft,
        c + dir * (half - head) - perp * head,
    ];
    painter.add(egui::Shape::closed_line(pts.clone(), Stroke::new(2.0, outline)));
    // Not convex: fill as a shaft and two heads.
    painter.add(egui::Shape::convex_polygon(vec![pts[2], pts[3], pts[7], pts[8]], fill, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![pts[0], pts[1], pts[9]], fill, Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(vec![pts[5], pts[6], pts[4]], fill, Stroke::NONE));
}

/// Offset Face's handle (26.202, s37): a short, stubby double arrow with swept-back barbs, light blue with a grey
/// rim (deeper blue while it's pulled).
pub(super) fn face_arrow(painter: &egui::Painter, c: Pos2, dir: egui::Vec2, hot: bool) {
    let perp = vec2(-dir.y, dir.x);
    let (half, head, wing, barb, shaft) = (11.5, 6.5, 6.5, 1.6, 2.4);
    let fill = if hot { theme::c(0x416aff) } else { theme::c(0x6397ff) };
    let outline = theme::c(0x626265);
    // One head: tip, a wing tip swept back, the notch where it meets the shaft; the same on the other side.
    let head_pts = |s: f32| {
        let d = dir * s;
        let tip = c + d * half;
        let base = c + d * (half - head);
        [tip, base + perp * wing - d * barb, base + perp * shaft, base - perp * shaft, base - perp * wing - d * barb]
    };
    let (a, b) = (head_pts(1.0), head_pts(-1.0));
    // Round the outline: one side of both heads (+perp), then the other.
    let ring = vec![a[0], a[1], a[2], b[2], b[1], b[0], b[4], b[3], a[3], a[4]];
    painter.add(egui::Shape::closed_line(ring, Stroke::new(1.6, outline)));
    painter.add(egui::Shape::convex_polygon(vec![a[2], b[2], b[3], a[3]], fill, Stroke::NONE));
    for h in [a, b] {
        // The barbs make a head concave: three triangles from its tip.
        for tri in [[h[0], h[1], h[2]], [h[0], h[2], h[3]], [h[0], h[3], h[4]]] {
            painter.add(egui::Shape::convex_polygon(tri.to_vec(), fill, Stroke::NONE));
        }
    }
}

/// The rounded drop at the far end of a dashed lead (26.202, s37): its point runs on along the line.
pub(super) fn lead_drop(painter: &egui::Painter, tip: Pos2, dir: egui::Vec2, ink: Color32) {
    let perp = vec2(-dir.y, dir.x);
    let c = tip - dir * 6.0;
    painter.circle_filled(c, 3.6, ink);
    painter.add(egui::Shape::convex_polygon(vec![tip, c + perp * 3.4 - dir * 0.6, c - perp * 3.4 - dir * 0.6], ink, Stroke::NONE));
}

/// The ink on the white pills and badges over the view. They're white in both themes (s25 light, s11-075 dark), so
/// their ink isn't themed either.
pub(in crate::workspace) const PILL_INK: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1e);

/// A small white rounded label (blue-edged for angles), like the reference's handle values.
pub(super) fn pill(painter: &egui::Painter, center: Pos2, text: &str, blue_edge: bool) {
    let galley = painter.layout_no_wrap(text.to_string(), theme::strong(10.5), PILL_INK);
    let r = Rect::from_center_size(center, vec2(galley.size().x + 12.0, 18.0));
    painter.rect_filled(r, radius(5), Color32::WHITE);
    if blue_edge {
        painter.rect_stroke(r, radius(5), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
        note_value_pill(painter, r, text);
    }
    painter.galley(r.center() - galley.size() / 2.0, galley, PILL_INK);
}

pub(super) fn value_box(painter: &egui::Painter, center: Pos2, text: &str, typed: Option<&str>) {
    let (shown, editing) = match typed {
        Some(t) => (t.to_string(), true),
        None => (text.to_string(), false),
    };
    let galley = painter.layout_no_wrap(shown.clone(), theme::mono(11.5), Tokens::text_strong());
    let w = galley.size().x.max(if editing { 84.0 } else { 0.0 }) + 12.0;
    let r = Rect::from_center_size(center, vec2(w, 20.0));
    // Plain labels float on the view (no box), the live field gets the blue-bordered box.
    if !editing {
        painter.text(r.center(), Align2::CENTER_CENTER, shown, theme::mono(11.5), hex_text());
        return;
    }
    let r = Rect::from_min_size(r.min, vec2(r.width() + 18.0, r.height()));
    painter.rect_filled(r, radius(4), theme::c(0x1b1f2b));
    let border = Stroke::new(1.5, Tokens::accent());
    // Calculator glyph at the right end of the field.
    let calc = Rect::from_center_size(pos2(r.right() - 11.0, r.center().y), vec2(10.0, 12.0));
    painter.rect_stroke(calc, radius(2), Stroke::new(1.0, Tokens::text_dim()), StrokeKind::Inside);
    for (dx, dy) in [(-2.0, 1.0), (2.0, 1.0), (-2.0, 4.0), (2.0, 4.0)] {
        painter.circle_filled(calc.center() + vec2(dx, dy), 0.8, Tokens::text_dim());
    }
    painter.line_segment([calc.center() + vec2(-3.0, -3.0), calc.center() + vec2(3.0, -3.0)], Stroke::new(1.0, Tokens::text_dim()));
    painter.rect_stroke(r, radius(4), border, StrokeKind::Inside);
    let anchor = if editing { Align2::LEFT_CENTER } else { Align2::CENTER_CENTER };
    let pos = if editing { pos2(r.left() + 6.0, r.center().y) } else { r.center() };
    painter.text(pos, anchor, shown, theme::mono(11.5), Tokens::text_strong());
}

/// A tool's target badge (Replace Face, Project as Edge, Split Body): white, a purple ring, a crosshair.
pub(super) fn target_badge(painter: &egui::Painter, c: Pos2) {
    painter.circle_filled(c, 18.0, Color32::WHITE);
    painter.circle_stroke(c, 15.0, Stroke::new(1.6, theme::c(0xa162f8)));
    let ink = Stroke::new(1.6, PILL_INK);
    painter.circle_stroke(c, 6.5, ink);
    painter.circle_filled(c, 2.0, PILL_INK);
    for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
        painter.line_segment([c + d * 6.5, c + d * 10.0], ink);
    }
}

/// An angle dimension as observed (s15-006): a thin white arc between the two lines with arrowheads pointing into
/// them, the value outside the arc's middle, turned along it (or the value field while it's typed).
pub(super) fn angle_dim(painter: &egui::Painter, a: &AngleDim, typed: Option<&str>) {
    let white = Color32::WHITE;
    let arc: Vec<Pos2> = (0..=32).map(|i| a.at(ANGLE_RADIUS, i as f32 / 32.0)).collect();
    painter.add(egui::Shape::line(arc.clone(), Stroke::new(1.0, white)));
    for (tip, prev) in [(arc[0], arc[1]), (arc[32], arc[31])] {
        let u = (prev - tip).normalized();
        let pp = vec2(-u.y, u.x);
        painter.add(egui::Shape::convex_polygon(vec![tip, tip + u * 9.0 + pp * 3.5, tip + u * 9.0 - pp * 3.5], white, Stroke::NONE));
    }
    if let Some(t) = typed {
        value_field(painter, a.label, t);
    } else {
        let u = (a.at(ANGLE_RADIUS, 0.55) - a.at(ANGLE_RADIUS, 0.45)).normalized();
        sketch_dimension_label(painter, a.label, u, &format_degrees(a.degrees), false);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pill_gives_its_number() {
        use super::pill_number;
        assert_eq!(pill_number("19 mm").as_deref(), Some("19"));
        assert_eq!(pill_number("R 2mm").as_deref(), Some("2"));
        assert_eq!(pill_number("⌀10 mm").as_deref(), Some("10"));
        assert_eq!(pill_number("-3.5 mm").as_deref(), Some("-3.5"));
        assert_eq!(pill_number("1.50×").as_deref(), Some("1.50"));
        assert_eq!(pill_number("45°").as_deref(), Some("45"));
        assert_eq!(pill_number("Copy"), None);
    }
}

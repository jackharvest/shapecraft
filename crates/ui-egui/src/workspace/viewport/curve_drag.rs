//! Dragging a sketch's curves straight in the view, with no tool running, as observed (#87): a line's side slides
//! (a rectangle stays a rectangle), a rectangle's corner moves with the opposite one held, a circle's rim sets its
//! radius, and a polygon or ellipse grows or shrinks about its middle. One step, live while dragging.

use egui::{Pos2, Ui};
use glam::DVec2;
use serde_json::json;
use shapecraft_doc::{ItemId, Pick, SketchEntity};

use super::clicks::moved_side;
use super::snap::to_d;
use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

/// How near (screen points) a press has to be to a rectangle's corner to take the corner rather than a side.
const CORNER_PX: f64 = 9.0;

#[derive(Clone, Debug)]
pub(crate) struct CurveDrag {
    sketch: ItemId,
    entity: u32,
    /// The sketch's curves when the drag started: every frame works from these, so nothing drifts.
    base: Vec<SketchEntity>,
    /// Where it was grabbed, on the sketch plane.
    start: DVec2,
    grab: Grab,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grab {
    /// A line, pushed sideways; lines meeting its ends stretch with it.
    Side,
    /// A rectangle's corner: the rectangle's four lines (in order) and which of its frame's corners.
    Corner([u32; 4], usize),
    /// A circle's rim.
    Rim,
    /// A polygon or an ellipse, scaled about its middle.
    Scale,
}

impl CurveDrag {
    /// The changed curves for the pointer at `at` (on the sketch plane, already snapped), as `set_entity` takes them.
    fn edits(&self, at: DVec2) -> Vec<serde_json::Value> {
        let Some(e) = self.base.get(self.entity as usize) else { return Vec::new() };
        match (self.grab, e) {
            (Grab::Side, SketchEntity::Line { a, b }) => {
                let n = (*b - *a).normalize_or_zero().perp();
                let by = (at - self.start).dot(n);
                if by.abs() < 1e-9 || n == DVec2::ZERO {
                    return Vec::new();
                }
                moved_side(&self.base, self.entity, n * by)
            }
            (Grab::Corner(lines, k), _) => corner_moved(&self.base, lines, k, at),
            (Grab::Rim, SketchEntity::Circle { center, .. }) => {
                let radius = at.distance(*center);
                if radius < 1e-6 {
                    return Vec::new();
                }
                vec![json!([self.entity, SketchEntity::Circle { center: *center, radius }])]
            }
            (Grab::Scale, SketchEntity::Polygon { center, radius, sides, angle }) => {
                let Some(s) = self.scale(*center, at) else { return Vec::new() };
                vec![json!([self.entity, SketchEntity::Polygon { center: *center, radius: radius * s, sides: *sides, angle: *angle }])]
            }
            (Grab::Scale, SketchEntity::Ellipse { center, rx, ry, angle }) => {
                let Some(s) = self.scale(*center, at) else { return Vec::new() };
                vec![json!([self.entity, SketchEntity::Ellipse { center: *center, rx: rx * s, ry: ry * s, angle: *angle }])]
            }
            _ => Vec::new(),
        }
    }

    /// How much bigger the pointer's distance from `center` is than the grab's.
    fn scale(&self, center: DVec2, at: DVec2) -> Option<f64> {
        let from = self.start.distance(center);
        let s = at.distance(center) / from;
        (from > 1e-9 && s.is_finite() && s > 1e-6 && (s - 1.0).abs() > 1e-9).then_some(s)
    }
}

/// A rectangle with corner `k` (of its frame's corners) moved to `at`, the opposite corner held and the sides
/// kept square to each other.
fn corner_moved(base: &[SketchEntity], lines: [u32; 4], k: usize, at: DVec2) -> Vec<serde_json::Value> {
    let Some(f) = shapecraft_doc::rectangle_frame(base, lines) else { return Vec::new() };
    let old = f.corners();
    let uv = |p: DVec2| DVec2::new((p - f.origin).dot(f.u), (p - f.origin).dot(f.v));
    let back = |q: DVec2| f.origin + f.u * q.x + f.v * q.y;
    let (o, q) = (uv(old[(k + 2) % 4]), uv(at));
    if (q.x - o.x).abs() < 1e-6 || (q.y - o.y).abs() < 1e-6 {
        return Vec::new();
    }
    let mut new = old;
    new[k] = back(q);
    for j in [(k + 1) % 4, (k + 3) % 4] {
        // The corner beside it on the same side keeps the grabbed corner's place along that side's direction.
        new[j] = if (uv(old[j]).x - uv(old[k]).x).abs() < 1e-9 { back(DVec2::new(q.x, o.y)) } else { back(DVec2::new(o.x, q.y)) };
    }
    let moved = |p: DVec2| {
        let i = (0..4).min_by(|&i, &j| old[i].distance(p).total_cmp(&old[j].distance(p))).unwrap_or(0);
        new[i]
    };
    lines
        .iter()
        .filter_map(|&l| match base.get(l as usize) {
            Some(SketchEntity::Line { a, b }) => Some(json!([l, SketchEntity::Line { a: moved(*a), b: moved(*b) }])),
            _ => None,
        })
        .collect()
}

impl Workspace {
    /// Where a screen point lands on the sketch being edited.
    fn on_sketch(&self, p: Pos2) -> Option<DVec2> {
        let plane = self.sketch_plane()?;
        Some(plane.to_local(self.sketch_cursor_raw(p)?))
    }

    /// What a press at `p` would drag: a rectangle's corner, else the curve under it.
    fn grab_curve(&self, p: Pos2) -> Option<CurveDrag> {
        let sk = self.session.active_sketch()?;
        let start = self.on_sketch(p)?;
        let screen = |q: DVec2| self.to_screen(sk.plane.to_world(q)).map(|s| to_d(s).distance(to_d(p)));
        let drag = |entity, grab| CurveDrag { sketch: sk.id, entity, base: sk.entities.clone(), start, grab };
        for (i, e) in sk.entities.iter().enumerate() {
            if !matches!(e, SketchEntity::Line { .. }) {
                continue;
            }
            if let Some(lp) = shapecraft_doc::rectangle_loop(&sk.entities, i as u32)
                && lp.iter().min() == Some(&(i as u32))
                && let Some(f) = shapecraft_doc::rectangle_frame(&sk.entities, lp)
                && let Some(k) = (0..4).find(|&k| screen(f.corners()[k]).is_some_and(|d| d < CORNER_PX))
            {
                return Some(drag(i as u32, Grab::Corner(lp, k)));
            }
        }
        let (Pick::SketchEntity { sketch, entity }, _) = self.pick_at(p)? else { return None };
        if sketch != sk.id {
            return None;
        }
        let grab = match sk.entities.get(entity as usize)? {
            // A loose line's end is a point, not the line (points don't drag yet): leave it to the box.
            SketchEntity::Line { a, b } if [a, b].iter().any(|q| screen(**q).is_some_and(|d| d < CORNER_PX)) => return None,
            SketchEntity::Line { .. } => Grab::Side,
            SketchEntity::Circle { .. } => Grab::Rim,
            SketchEntity::Polygon { .. } | SketchEntity::Ellipse { .. } => Grab::Scale,
            _ => return None,
        };
        Some(drag(entity, grab))
    }

    /// A press and drag on a curve of the sketch being edited reshapes it (see the module notes). Returns whether
    /// the drag was this one's.
    pub(super) fn curve_drags(&mut self, ui: &Ui, resp: &egui::Response) -> bool {
        if self.curve_drag.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
            && matches!(self.tool, Tool::Idle)
            && !self.visualizing
            && self.drawings.is_none()
            && !self.touch_drag
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
            && let Some(mut drag) = self.grab_curve(origin)
        {
            drag.start = self.grid_snapped(drag.start);
            self.live_step = None;
            self.dim_edit = None;
            self.numeric = None;
            self.session.selection = vec![Pick::SketchEntity { sketch: drag.sketch, entity: drag.entity }];
            self.curve_drag = Some(drag);
        }
        let Some(drag) = self.curve_drag.clone() else { return false };
        if let Some(at) = ui.input(|i| i.pointer.interact_pos()).and_then(|p| self.on_sketch(p)) {
            let edits = drag.edits(self.grid_snapped(at));
            let nothing = edits.is_empty();
            self.run_live("set_entity", json!({ "sketch": drag.sketch.0, "entities": edits }), nothing);
            self.session.selection = vec![Pick::SketchEntity { sketch: drag.sketch, entity: drag.entity }];
        }
        if resp.drag_stopped() {
            self.curve_drag = None;
        }
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn rect() -> Vec<SketchEntity> {
        SketchEntity::rectangle_lines(DVec2::ZERO, DVec2::new(10.0, 6.0)).to_vec()
    }

    fn apply(base: &[SketchEntity], edits: &[serde_json::Value]) -> Vec<SketchEntity> {
        let mut out = base.to_vec();
        for e in edits {
            let k = e[0].as_u64().unwrap() as usize;
            out[k] = serde_json::from_value(e[1].clone()).unwrap();
        }
        out
    }

    fn frame(es: &[SketchEntity]) -> shapecraft_doc::RectFrame {
        shapecraft_doc::rectangle_frame(es, shapecraft_doc::rectangle_loop(es, 0).unwrap()).unwrap()
    }

    /// A rectangle's side pulled out 3: the rectangle is 3 wider and still a rectangle.
    #[test]
    fn a_side_slides_and_the_rectangle_stays_square() {
        let base = rect();
        // Line 1 is the right side (10,0)→(10,6).
        let d = CurveDrag { sketch: ItemId(1), entity: 1, base: base.clone(), start: DVec2::new(10.0, 3.0), grab: Grab::Side };
        let out = apply(&base, &d.edits(DVec2::new(13.0, 4.0)));
        let f = frame(&out);
        assert!((f.width - 13.0).abs() < 1e-9 && (f.height - 6.0).abs() < 1e-9, "{f:?}");
        assert!(d.edits(DVec2::new(10.0, 5.0)).is_empty(), "along the side: nothing moves");
    }

    /// The top-right corner to (14, 9): the bottom-left one stays put and the sides stay square.
    #[test]
    fn a_corner_moves_with_the_opposite_one_held() {
        let base = rect();
        let lp = shapecraft_doc::rectangle_loop(&base, 0).unwrap();
        let k = frame(&base).corners().iter().position(|c| c.distance(DVec2::new(10.0, 6.0)) < 1e-9).unwrap();
        let d = CurveDrag { sketch: ItemId(1), entity: 0, base: base.clone(), start: DVec2::new(10.0, 6.0), grab: Grab::Corner(lp, k) };
        let out = apply(&base, &d.edits(DVec2::new(14.0, 9.0)));
        let f = frame(&out);
        assert!(f.origin.distance(DVec2::ZERO) < 1e-9 && (f.width - 14.0).abs() < 1e-9 && (f.height - 9.0).abs() < 1e-9, "{f:?}");
        assert!(d.edits(DVec2::new(0.0, 4.0)).is_empty(), "flat onto the held corner: refused");
    }

    /// A circle's rim pulled to 7 from its middle: radius 7. A pentagon pulled from 4 to 6 out: 1.5 times as big.
    #[test]
    fn a_rim_sets_the_radius_and_a_polygon_scales() {
        let base = vec![
            SketchEntity::Circle { center: DVec2::new(1.0, 1.0), radius: 3.0 },
            SketchEntity::Polygon { center: DVec2::ZERO, radius: 5.0, sides: 5, angle: 0.0 },
        ];
        let d = CurveDrag { sketch: ItemId(1), entity: 0, base: base.clone(), start: DVec2::new(4.0, 1.0), grab: Grab::Rim };
        assert!(
            matches!(apply(&base, &d.edits(DVec2::new(8.0, 1.0)))[0], SketchEntity::Circle { radius, .. } if (radius - 7.0).abs() < 1e-9)
        );
        let d = CurveDrag { sketch: ItemId(1), entity: 1, base: base.clone(), start: DVec2::new(-4.0, 0.0), grab: Grab::Scale };
        assert!(
            matches!(apply(&base, &d.edits(DVec2::new(-6.0, 0.0)))[1], SketchEntity::Polygon { radius, sides: 5, .. } if (radius - 7.5).abs() < 1e-9)
        );
    }

    mod in_the_app {
        use std::sync::Arc;

        use egui::{Event, Pos2, Rect, pos2, vec2};
        use glam::DVec2;
        use serde_json::json;
        use shapecraft_doc::SketchEntity;

        use crate::workspace::Workspace;

        fn frame(ctx: &egui::Context, ws: &mut Workspace, events: Vec<Event>) {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| ws.ui(ui, None));
            out.textures_delta.clear();
        }

        fn button(at: Pos2, pressed: bool) -> Event {
            Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
        }

        fn drag(ctx: &egui::Context, ws: &mut Workspace, from: Pos2, to: Pos2) {
            frame(ctx, ws, vec![Event::PointerMoved(from)]);
            frame(ctx, ws, vec![button(from, true)]);
            for k in 1..=8 {
                frame(ctx, ws, vec![Event::PointerMoved(from + (to - from) * (k as f32 / 8.0))]);
            }
            frame(ctx, ws, vec![button(to, false)]);
            frame(ctx, ws, Vec::new());
        }

        fn tap(ctx: &egui::Context, ws: &mut Workspace, at: Pos2) {
            frame(ctx, ws, vec![Event::PointerMoved(at)]);
            frame(ctx, ws, vec![button(at, true)]);
            frame(ctx, ws, vec![button(at, false)]);
            frame(ctx, ws, Vec::new());
        }

        fn sketch_with(shapes: &[serde_json::Value]) -> (Workspace, egui::Context) {
            let session = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Drag");
            let mut ws = Workspace::new(session, crate::settings::Settings::default());
            ws.session.execute("sketch", &json!({"plane": "xy"})).unwrap();
            for s in shapes {
                let (id, p) = (s[0].as_str().unwrap(), &s[1]);
                ws.session.execute(id, p).unwrap();
            }
            ws.session.selection.clear();
            let ctx = egui::Context::default();
            for _ in 0..3 {
                frame(&ctx, &mut ws, Vec::new());
            }
            (ws, ctx)
        }

        fn screen(ws: &Workspace, p: DVec2) -> Pos2 {
            let plane = ws.sketch_plane().unwrap();
            ws.to_screen(plane.to_world(p)).unwrap()
        }

        /// #87: a press on a circle's rim and a drag outward makes it bigger, in one step, with no selection box.
        #[test]
        fn dragging_a_circles_rim_resizes_it() {
            let (mut ws, ctx) = sketch_with(&[json!(["circle", {"center": [0, 0], "radius": 10}])]);
            let (rim, out) = (screen(&ws, DVec2::new(10.0, 0.0)), screen(&ws, DVec2::new(15.0, 0.0)));
            drag(&ctx, &mut ws, rim, out);
            let sk = ws.session.active_sketch().unwrap();
            let Some(SketchEntity::Circle { radius, .. }) = sk.entities.first() else { panic!("{:?}", sk.entities) };
            assert!((radius - 15.0).abs() < 1.0, "radius {radius}");
            assert!(ws.area.is_none() && ws.curve_drag.is_none(), "no selection box, and the drag is over");
            // One step: a single undo puts it back.
            let _ = ws.run("undo", json!({}));
            let Some(SketchEntity::Circle { radius, .. }) = ws.session.active_sketch().unwrap().entities.first() else { panic!() };
            assert!((radius - 10.0).abs() < 1e-9, "undone in one: {radius}");
        }

        /// #87: a rectangle's side dragged out moves that side; its free corner dragged moves the corner.
        #[test]
        fn dragging_a_rectangles_side_and_corner_reshapes_it() {
            let (mut ws, ctx) = sketch_with(&[json!(["rectangle", {"corner": [0, 0], "opposite": [20, 10]}])]);
            let (a, b) = (screen(&ws, DVec2::new(20.0, 5.0)), screen(&ws, DVec2::new(25.0, 6.0)));
            drag(&ctx, &mut ws, a, b);
            let es = ws.session.active_sketch().unwrap().entities.clone();
            let f = shapecraft_doc::rectangle_frame(&es, shapecraft_doc::rectangle_loop(&es, 0).unwrap()).unwrap();
            assert!((f.width - 25.0).abs() < 1.0 && (f.height - 10.0).abs() < 1e-6, "{f:?}");
            let (a, b) = (screen(&ws, DVec2::new(25.0, 10.0)), screen(&ws, DVec2::new(30.0, 15.0)));
            drag(&ctx, &mut ws, a, b);
            let es = ws.session.active_sketch().unwrap().entities.clone();
            let f = shapecraft_doc::rectangle_frame(&es, shapecraft_doc::rectangle_loop(&es, 0).unwrap()).unwrap();
            assert!((f.width - 30.0).abs() < 1.0 && (f.height - 15.0).abs() < 1.0, "{f:?}");
        }

        /// #87: a drag that starts on nothing still draws the selection box.
        #[test]
        fn a_drag_on_nothing_still_boxes() {
            let (mut ws, ctx) = sketch_with(&[json!(["circle", {"center": [0, 0], "radius": 5}])]);
            let (a, b) = (screen(&ws, DVec2::new(-15.0, 15.0)), screen(&ws, DVec2::new(-20.0, 20.0)));
            frame(&ctx, &mut ws, vec![Event::PointerMoved(a)]);
            frame(&ctx, &mut ws, vec![button(a, true)]);
            for k in 1..=4 {
                frame(&ctx, &mut ws, vec![Event::PointerMoved(a + (b - a) * (k as f32 / 4.0))]);
            }
            assert!(ws.area.is_some() && ws.curve_drag.is_none());
            frame(&ctx, &mut ws, vec![button(b, false)]);
        }

        /// #88: a value typed into a dimension is applied by a click elsewhere, not thrown away.
        #[test]
        fn a_typed_dimension_is_applied_by_clicking_away() {
            let (mut ws, ctx) = sketch_with(&[json!(["circle", {"center": [0, 0], "radius": 10}])]);
            let sk = ws.session.active_sketch.unwrap().0;
            ws.session.execute("select", &json!({"picks": [{"kind": "sketch_entity", "sketch": sk, "entity": 0}]})).unwrap();
            frame(&ctx, &mut ws, Vec::new());
            let label = ws.entity_dims().first().map(|d| d.label).expect("the circle's diameter");
            tap(&ctx, &mut ws, label);
            assert!(ws.dim_edit.is_some());
            frame(&ctx, &mut ws, vec![Event::Text("50".into())]);
            let away = screen(&ws, DVec2::new(-10.0, 25.0));
            tap(&ctx, &mut ws, away);
            assert!(ws.dim_edit.is_none());
            let Some(SketchEntity::Circle { radius, .. }) = ws.session.active_sketch().unwrap().entities.first() else { panic!() };
            assert!((radius - 25.0).abs() < 1e-6, "diameter 50 applied: radius {radius}");
        }

        /// #86: a rectangle's middle snaps, named Center.
        #[test]
        fn a_rectangles_middle_snaps() {
            let (mut ws, ctx) = sketch_with(&[json!(["rectangle", {"corner": [0, 0], "opposite": [20, 10]}])]);
            let near = screen(&ws, DVec2::new(10.2, 5.1));
            frame(&ctx, &mut ws, vec![Event::PointerMoved(near)]);
            let snap = ws.sketch_snap().unwrap();
            assert!(snap.point && snap.p.distance(DVec2::new(10.0, 5.0)) < 1e-9, "{:?}", snap.p);
        }

        /// #102: near a curve but away from its key points, the point lands on the curve itself (a side, a circle);
        /// a key point close by still wins.
        #[test]
        fn points_snap_onto_curves_anywhere_along_them() {
            let (mut ws, ctx) = sketch_with(&[
                json!(["rectangle", {"corner": [0, 0], "opposite": [20, 10]}]),
                json!(["circle", {"center": [-12, 5], "radius": 4}]),
            ]);
            let mut at = |p: DVec2| {
                let s = screen(&ws, p);
                frame(&ctx, &mut ws, vec![Event::PointerMoved(s)]);
                ws.sketch_snap().unwrap()
            };
            let side = at(DVec2::new(6.3, 0.06));
            assert!(side.point && side.curve.is_some() && side.p.distance(DVec2::new(6.3, 0.0)) < 1e-3, "{:?}", side.p);
            let rim = at(DVec2::new(-12.0 + 2.9, 5.0 + 2.8));
            let r = rim.p.distance(DVec2::new(-12.0, 5.0));
            assert!(rim.curve.is_some() && (r - 4.0).abs() < 1e-9, "on the circle: {:?}", rim.p);
            let mid = at(DVec2::new(10.1, 0.05));
            assert!(mid.curve.is_none() && mid.p.distance(DVec2::new(10.0, 0.0)) < 1e-9, "the midpoint wins: {:?}", mid.p);
        }
    }
}

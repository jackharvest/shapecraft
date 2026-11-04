//! Badges: the small white tiles beside a running tool's handle that change how its result behaves after the pull,
//! as observed in 26.200 (s25-005..017). Extrude stacks them along its dashed line, from the value outward: Sides,
//! then Result once the solid meets a body. Clicking one opens a menu pointing at it, with a dim title and a check on
//! the current choice; the badge then shows the chosen option's icon (or short text, like Fillet's "G1").

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::Workspace;
use super::tools::{Target, Tool};
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    /// New Body / Union / Intersect / Subtract.
    Result,
    /// Extrude's One-Sided / Symmetric.
    Sides,
    /// Scale's Uniform / Non-uniform, after its Copy pill.
    ScaleMode,
    /// Fillet's G1 - Tangent / G2 - Curvature.
    Continuity,
    /// Fillet's Size Type: Radius / Chord Width.
    SizeType,
    /// Fillet's Corners: Rolling Ball / Setback.
    Corner,
    /// Chamfer's Equal Distance / Two-Distance.
    ChamferType,
    /// Offset Edge's Single / Chain (whole connected chains of the picked curves).
    OffsetType,
    /// An opened line dimension's Absolute / Horizontal / Vertical (left of its value).
    DistanceType,
    /// Circular Pattern's Definition: Total angle / Spacing angle.
    Definition,
    /// Circular Pattern's Orientation: Uniform / Rotated.
    Orientation,
}

/// What a badge shows: the chosen option's icon, or a short text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    Icon(Icon),
    Text(&'static str),
}

/// A row of a badge's menu: label, icon, and whether it can be picked (rows we can't build yet stay greyed).
type Row = (&'static str, Option<Icon>, bool);

/// The Result menu in the observed order: (label, request value, icon).
pub(crate) const RESULTS: [(&str, &str, Icon); 4] = [
    ("New Body", "new", Icon::NewBody),
    ("Union", "union", Icon::Union),
    ("Intersect", "intersect", Icon::Intersect),
    ("Subtract", "subtract", Icon::Subtract),
];
/// The Distance Type menu, in the manual's order.
const DISTANCE_TYPES: [(&str, Icon, shapecraft_doc::Along); 3] = [
    ("Absolute", Icon::DistAbsolute, shapecraft_doc::Along::Absolute),
    ("Horizontal", Icon::DistHorizontal, shapecraft_doc::Along::Horizontal),
    ("Vertical", Icon::DistVertical, shapecraft_doc::Along::Vertical),
];
const SIDES: [(&str, Icon); 2] = [("One-Sided", Icon::Extrude), ("Symmetric", Icon::ExtrudeSymmetric)];

/// Where this frame's badges were painted, for clicks.
const RECTS: &str = "tool-badges";

impl Workspace {
    /// The running tool's Result choice ("auto" when untouched) and the option "auto" stands for.
    fn result_choice(&self) -> Option<(&'static str, usize)> {
        match &self.tool {
            Tool::Extrude(t) => {
                // As the engine decides it: on a body, pulling out adds and pushing in cuts; elsewhere a new body.
                let side = match self.live_extrude_side() {
                    Some(side) => side,
                    None => self.extrude_body_side(t),
                };
                let auto = match side {
                    None => 0,
                    Some(side) if t.distance * side > 0.0 => 3,
                    Some(_) => 1,
                };
                Some((t.result, auto))
            }
            // As the engine decides it (the manual's automatic Boolean: new, union, or subtract).
            Tool::Revolve(t) => Some((t.result, self.revolve_auto_row(t))),
            // A pattern's copies are new bodies unless picked otherwise (s25 p4).
            Tool::Pattern(t) if !t.items.is_empty() => Some((t.result, 0)),
            Tool::Loft(t) => Some((t.result, 0)),
            Tool::CircularPattern(t) if !t.items.is_empty() => Some((t.result, 0)),
            Tool::Sweep(t) => Some((t.result, 0)),
            _ => None,
        }
    }

    /// The line whose length dimension is open for typing (not a rectangle's side, which is a width or height):
    /// its sketch, element and ends.
    pub(super) fn opened_line(&self) -> Option<(shapecraft_doc::ItemId, u32, glam::DVec2, glam::DVec2)> {
        let e = self.dim_edit.filter(|e| e.kind == super::viewport::DimKind::Length)?;
        let sk = self.session.doc.sketch(e.sketch)?;
        let Some(shapecraft_doc::SketchEntity::Line { a, b }) = sk.entities.get(e.entity as usize) else { return None };
        shapecraft_doc::rectangle_loop(&sk.entities, e.entity).is_none().then_some((e.sketch, e.entity, *a, *b))
    }

    /// The open line dimension's Distance Type (its kept dimension's, else Absolute).
    fn distance_type(&self) -> Option<usize> {
        let (sketch, entity, ..) = self.opened_line()?;
        let along = self.session.doc.sketch(sketch)?.constraints.iter().find_map(|c| match c {
            shapecraft_doc::Constraint::Distance { a, along, .. } if a.entity() == entity => Some(*along),
            _ => None,
        });
        Some(DISTANCE_TYPES.iter().position(|d| Some(d.2) == along).unwrap_or(0))
    }

    /// The Result row Revolve's auto stands for, asked of the engine once per model revision and settings.
    /// The body a running Extrude works on when its Result is auto, as the engine decides it: which side of the
    /// profile's plane it is on (-1 behind it, like the face a sketch was drawn on), or `None` for a new body.
    pub(super) fn extrude_body_side(&self, t: &super::tools::ExtrudeTool) -> Option<f64> {
        let Target::Region { sketch, region } = t.target else { return Some(-1.0) };
        let mut regions = vec![region];
        regions.extend(t.more.iter().copied());
        self.extrude_side_of(sketch, regions)
    }

    /// While the extrude is applied live the document already holds its result (a cut leaves the profile over a
    /// hole, resting on nothing), so the side it worked on is read off its step rather than worked out again.
    fn live_extrude_side(&self) -> Option<Option<f64>> {
        if !self.is_live() {
            return None;
        }
        let step = self.session.doc.history.last().filter(|h| h.kind == "extrude")?;
        let e = step.entries.last()?;
        let on_body = e.params.get("face").is_some()
            || e.params
                .get("sketch")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| self.session.doc.sketch(shapecraft_doc::ItemId(id)))
                .is_some_and(|s| s.on_body.is_some());
        Some(if on_body { Some(-1.0) } else { e.params.get("auto_side").and_then(serde_json::Value::as_f64) })
    }

    /// [`Self::extrude_body_side`] for a sketch's regions, cached per document revision.
    pub(super) fn extrude_side_of(&self, sketch: shapecraft_doc::ItemId, regions: Vec<u32>) -> Option<f64> {
        let rev = self.session.revision;
        if let Some((r, k, g, side)) = self.extrude_auto.borrow().as_ref()
            && *r == rev
            && *k == sketch
            && *g == regions
        {
            return *side;
        }
        let side = shapecraft_engine::cmd::extrude_auto_body(&self.session, sketch, &regions).map(|(_, side)| side);
        *self.extrude_auto.borrow_mut() = Some((rev, sketch, regions, side));
        side
    }

    fn revolve_auto_row(&self, t: &super::tools::RevolveTool) -> usize {
        let Some(mut params) = t.params(t.angle) else { return 0 };
        params["result"] = serde_json::json!("auto");
        let key = params.to_string();
        let rev = self.session.revision;
        if let Some((r, k, row)) = self.revolve_auto.borrow().as_ref()
            && *r == rev
            && *k == key
        {
            return *row;
        }
        let row = match shapecraft_engine::cmd::revolve_auto(&self.session, &params) {
            Ok("union") => 1,
            Ok("subtract") => 3,
            _ => 0,
        };
        *self.revolve_auto.borrow_mut() = Some((rev, key, row));
        row
    }

    /// The badges the running tool shows, outward from its value.
    pub(super) fn badges(&self) -> Vec<(Badge, Face)> {
        let mut out = Vec::new();
        if let Some(i) = self.distance_type() {
            out.push((Badge::DistanceType, Face::Icon(DISTANCE_TYPES[i].1)));
        }
        if let Tool::Extrude(t) = &self.tool {
            out.push((Badge::Sides, Face::Icon(SIDES[usize::from(t.symmetric)].1)));
            if self.extrude_meets_body()
                && let Some(i) = self.badge_current(Badge::Result)
            {
                out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
            }
        }
        match &self.tool {
            // After the pull, the pattern's Result badge follows its quantity (26.200).
            Tool::Pattern(t) if t.spacing.abs() > 1e-9 || self.numeric.is_some() => {
                if let Some(i) = self.badge_current(Badge::Result) {
                    out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
                }
            }
            // Loft and Sweep (26.190): once there's a solid to make and a body it could meet.
            Tool::Loft(t) if t.request().is_some() && self.session.doc.bodies.iter().any(|b| b.visible) => {
                if let Some(i) = self.badge_current(Badge::Result) {
                    out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
                }
            }
            Tool::Sweep(t) if t.request().is_some() && self.session.doc.bodies.iter().any(|b| b.visible) => {
                if let Some(i) = self.badge_current(Badge::Result) {
                    out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
                }
            }
            // The manual's pattern control badges: Definition, then (bodies) Orientation and Result.
            Tool::CircularPattern(t) if t.about.is_some() => {
                out.push((Badge::Definition, Face::Text(if t.spacing { "Spacing" } else { "Total" })));
                if !t.items.is_empty() {
                    out.push((Badge::Orientation, Face::Icon(if t.uniform { Icon::Translate } else { Icon::RotateAxis })));
                    if let Some(i) = self.badge_current(Badge::Result) {
                        out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
                    }
                }
            }
            Tool::Scale(t) => out.push((Badge::ScaleMode, Face::Icon(if t.uniform { Icon::Scale } else { Icon::ScaleFree }))),
            Tool::Fillet(t) if t.chamfer => out.push((Badge::ChamferType, Face::Text(if t.two.is_some() { "2" } else { "=" }))),
            Tool::Fillet(t) => {
                out.push((Badge::Continuity, Face::Text(if t.g2 { "G2" } else { "G1" })));
                out.push((Badge::SizeType, Face::Text(if t.chordal { "C" } else { "R" })));
                out.push((Badge::Corner, Face::Text(if t.setback { "SB" } else { "RB" })));
            }
            // Shown on a whole region too, as observed (s11-075).
            Tool::OffsetEdge(t) if t.face.is_none() => out
                .push((Badge::OffsetType, Face::Icon(if self.variant("Offset Edge") == 1 { Icon::OffsetChain } else { Icon::OffsetEdge }))),
            _ => {}
        }
        // Revolve's Result badge is up as soon as there's a body to combine with (26.100).
        if let Tool::Revolve(t) = &self.tool
            && t.params(t.angle).is_some()
            && self.session.doc.bodies.iter().any(|b| b.visible)
            && let Some(i) = self.badge_current(Badge::Result)
        {
            out.push((Badge::Result, Face::Icon(RESULTS[i].2)));
        }
        out
    }

    /// Extrude's solid would touch a body: started on one, or reaching into one (by bounding boxes). Before any
    /// pull there is nothing to combine yet, so the Result badge waits, as observed (s25-014 → s25-015).
    fn extrude_meets_body(&self) -> bool {
        let Tool::Extrude(t) = &self.tool else { return false };
        let d = self.numeric_len().unwrap_or(t.distance);
        if d.abs() < 1e-9 {
            return false;
        }
        if matches!(t.target, Target::Face { .. }) || self.result_choice().is_some_and(|(_, auto)| auto != 0) {
            return true;
        }
        let (mut lo, mut hi) = (glam::DVec3::splat(f64::MAX), glam::DVec3::splat(f64::MIN));
        let back = if t.symmetric { -d } else { 0.0 };
        for p in t.tris.iter().flatten() {
            for q in [*p + t.normal * d, *p + t.normal * back] {
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
        self.session.doc.bodies.iter().filter(|b| b.visible).any(|b| {
            b.mesh.bounds().is_some_and(|(a, b)| {
                let (a, b) = (glam::DVec3::from(a.map(f64::from)), glam::DVec3::from(b.map(f64::from)));
                a.cmple(hi).all() && b.cmpge(lo).all()
            })
        })
    }

    /// What's being typed, as a length.
    fn numeric_len(&self) -> Option<f64> {
        self.numeric.as_deref().and_then(crate::workspace::tools::parse_length)
    }

    /// Which row of a badge's menu is checked.
    pub(in crate::workspace) fn badge_current(&self, b: Badge) -> Option<usize> {
        match b {
            Badge::Result => {
                let (result, auto) = self.result_choice()?;
                Some(RESULTS.iter().position(|r| r.1 == result).unwrap_or(auto))
            }
            Badge::Sides => match &self.tool {
                Tool::Extrude(t) => Some(usize::from(t.symmetric)),
                _ => None,
            },
            Badge::ScaleMode => match &self.tool {
                Tool::Scale(t) => Some(usize::from(!t.uniform)),
                _ => None,
            },
            Badge::Continuity => match &self.tool {
                Tool::Fillet(t) => Some(usize::from(t.g2)),
                _ => None,
            },
            Badge::SizeType => match &self.tool {
                Tool::Fillet(t) => Some(usize::from(t.chordal)),
                _ => None,
            },
            Badge::Corner => match &self.tool {
                Tool::Fillet(t) => Some(usize::from(t.setback)),
                _ => None,
            },
            Badge::ChamferType => match &self.tool {
                Tool::Fillet(t) => Some(usize::from(t.two.is_some())),
                _ => None,
            },
            Badge::OffsetType => matches!(self.tool, Tool::OffsetEdge(_)).then(|| self.variant("Offset Edge")),
            Badge::DistanceType => self.distance_type(),
            Badge::Definition => match &self.tool {
                Tool::CircularPattern(t) => Some(usize::from(t.spacing)),
                _ => None,
            },
            Badge::Orientation => match &self.tool {
                Tool::CircularPattern(t) => Some(usize::from(!t.uniform)),
                _ => None,
            },
        }
    }

    fn badge_menu_rows(b: Badge) -> (&'static str, Vec<Row>) {
        match b {
            Badge::Result => ("Result", RESULTS.iter().map(|r| (r.0, Some(r.2), true)).collect()),
            Badge::Sides => ("Sides", SIDES.iter().map(|r| (r.0, Some(r.1), true)).collect()),
            Badge::ScaleMode => ("Scale Mode", vec![("Uniform", Some(Icon::Scale), true), ("Non-uniform", Some(Icon::ScaleFree), true)]),
            // G2 is built for straight outside edges between flat faces (engine fillet_g2); others refuse with a toast.
            Badge::Continuity => ("Continuity", vec![("G1 - Tangent", None, true), ("G2 - Curvature", None, true)]),
            Badge::SizeType => ("Size Type", vec![("Radius", None, true), ("Chord Width", None, true)]),
            Badge::Corner => ("Corners", vec![("Rolling Ball", None, true), ("Setback", None, true)]),
            Badge::ChamferType => ("Type", vec![("Equal Distance", None, true), ("Two-Distance", None, true)]),
            Badge::OffsetType => ("Type", vec![("Single", Some(Icon::OffsetEdge), true), ("Chain", Some(Icon::OffsetChain), true)]),
            Badge::Definition => ("Definition", vec![("Total Angle", None, true), ("Spacing Angle", None, true)]),
            Badge::Orientation => {
                ("Orientation", vec![("Uniform", Some(Icon::Translate), true), ("Rotated", Some(Icon::RotateAxis), true)])
            }
            Badge::DistanceType => ("Distance Type", DISTANCE_TYPES.iter().map(|d| (d.0, Some(d.1), true)).collect()),
        }
    }

    pub(super) fn badge_pick(&mut self, b: Badge, i: usize) {
        // A Distance Type keeps the line where it is: the dimension takes the line's size measured the new way, and
        // typing then sets that.
        if b == Badge::DistanceType {
            let (Some((sketch, entity, p, q)), Some(d)) = (self.opened_line(), DISTANCE_TYPES.get(i)) else { return };
            let kept = self.distance_type().is_some_and(|k| k != 0)
                || self.session.doc.sketch(sketch).is_some_and(|sk| {
                    sk.constraints.iter().any(|c| matches!(c, shapecraft_doc::Constraint::Distance { a, .. } if a.entity() == entity))
                });
            let value = d.2.measure(p, q);
            if (i != 0 || kept) && value > 1e-9 {
                let along = serde_json::to_value(d.2).unwrap_or_default();
                let _ = self.run(
                    "constrain",
                    serde_json::json!({ "kind": "distance", "value": value, "along": along, "entities": [entity], "sketch": sketch.0 }),
                );
            } else if value <= 1e-9 {
                self.toast(format!("The line has no {} size", d.0.to_lowercase()));
            }
            self.numeric = Some(String::new());
            return;
        }
        // Two-Distance: a second arrow on the edge's other face, starting at the first distance (an edge without two
        // faces stays Equal Distance).
        // Continuity re-applies the live round the new way (remembered for the next fillet).
        if let (Badge::Continuity, Tool::Fillet(t)) = (b, &self.tool) {
            let mut t = t.clone();
            t.g2 = i == 1;
            self.variants.insert("Fillet Continuity".into(), i);
            self.tool = Tool::Fillet(t.clone());
            self.preview = None;
            if t.radius > 1e-9 {
                self.apply_fillet(&t);
            }
            return;
        }
        if let (Badge::SizeType | Badge::Corner, Tool::Fillet(t)) = (b, &self.tool) {
            let mut t = t.clone();
            if b == Badge::SizeType {
                t.chordal = i == 1;
            } else {
                t.setback = i == 1;
            }
            self.tool = Tool::Fillet(t.clone());
            self.preview = None;
            if t.radius > 1e-9 {
                self.apply_fillet(&t);
            }
            return;
        }
        if let (Badge::ChamferType, Tool::Fillet(t)) = (b, &self.tool) {
            let mut t = t.clone();
            t.two = if i == 1 { t.two_distance(t.radius) } else { None };
            t.chamfer = true;
            let changed = t.radius > 1e-9;
            self.tool = Tool::Fillet(t.clone());
            self.preview = None;
            if changed {
                self.apply_fillet(&t);
            }
            return;
        }
        match (b, &mut self.tool) {
            (Badge::Result, Tool::Extrude(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            (Badge::Result, Tool::Pattern(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            (Badge::Result, Tool::Loft(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            (Badge::Result, Tool::Sweep(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            (Badge::Result, Tool::Revolve(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            (Badge::Result, Tool::CircularPattern(t)) => {
                if let Some(r) = RESULTS.get(i) {
                    t.result = r.1;
                }
            }
            // The same spread either way: switching keeps the copies where they are, the value shown changes.
            (Badge::Definition, Tool::CircularPattern(t)) => {
                let total = t.total_angle(t.angle);
                t.spacing = i == 1;
                let n = f64::from(t.count.max(2));
                t.angle = if !t.spacing {
                    total
                } else if (total.abs() - 360.0).abs() < 1e-9 {
                    total / n
                } else {
                    total / (n - 1.0)
                };
            }
            (Badge::Orientation, Tool::CircularPattern(t)) => t.uniform = i == 0,
            (Badge::Sides, Tool::Extrude(t)) => t.symmetric = i == 1,
            // Chain takes in every curve connected to the picked ones; Single goes back to what was picked. Remembered.
            (Badge::OffsetType, Tool::OffsetEdge(t)) => {
                let (sketch, distance, picked) = (t.sketch, t.distance, t.picked.clone());
                self.variants.insert("Offset Edge".into(), i);
                if picked.is_empty() {
                    return;
                }
                self.session.selection = picked.iter().map(|&entity| shapecraft_doc::Pick::SketchEntity { sketch, entity }).collect();
                if i == 1 {
                    self.select_chains();
                }
                if let Some(mut n) = crate::workspace::tools::OffsetEdgeTool::from_selection(&self.session) {
                    n.distance = distance;
                    n.picked = picked;
                    self.tool = Tool::OffsetEdge(n);
                }
            }
            // The choice is remembered for the next Scale, as observed.
            (Badge::ScaleMode, Tool::Scale(t)) => {
                t.uniform = i == 0;
                self.variants.insert("Scale".into(), i);
            }
            _ => return,
        }
        self.preview = None;
        // An extrusion already pulled is a live step: redo it the new way now, not only at Return.
        if let Tool::Extrude(t) = &self.tool
            && self.is_live()
        {
            let t = t.clone();
            self.apply_extrude(&t);
        }
    }

    /// A click on one of this frame's badges opens (or closes) its menu. True when it hit one.
    pub(super) fn badge_click(&mut self, ui: &Ui, c: Pos2) -> bool {
        let rects: Vec<(Badge, Rect)> = ui.ctx().data(|d| d.get_temp(ui.scope_id().with(RECTS))).unwrap_or_default();
        // Only badges the running tool still shows (the stored rects outlive the tool that painted them).
        let shown = self.badges();
        let Some((b, r)) = rects.into_iter().find(|(b, r)| r.expand(2.0).contains(c) && shown.iter().any(|(s, _)| s == b)) else {
            return false;
        };
        self.badge_menu =
            if self.badge_menu.is_some_and(|(open, _)| open == b) { None } else { Some((b, pos2(r.right() + 6.0, r.center().y))) };
        self.popup_opened_at = self.now;
        true
    }
}

/// Paints the tool's badges in a row from `start` along `dir` (screen space) and remembers where they went.
pub(super) fn paint_row(ws: &Workspace, ui: &Ui, painter: &egui::Painter, start: Pos2, dir: egui::Vec2) {
    let mut rects: Vec<(Badge, Rect)> = Vec::new();
    // 23 px apart for the square ones; a wider text badge keeps the same 4 px gap.
    let mut at = start;
    for (b, face) in ws.badges() {
        let w = width(painter, face);
        if let Some((_, last)) = rects.last() {
            at += dir * (last.width() / 2.0 + 4.0 + w / 2.0);
        }
        let open = ws.badge_menu.is_some_and(|(o, _)| o == b);
        rects.push((b, paint(painter, at, face, open)));
    }
    ui.ctx().data_mut(|d| d.insert_temp(ui.scope_id().with(RECTS), rects));
}

/// How wide a badge is: a square tile for an icon or a short text, wider for longer text.
fn width(painter: &egui::Painter, face: Face) -> f32 {
    match face {
        Face::Text(t) => (painter.layout_no_wrap(t.to_string(), theme::strong(9.5), Color32::BLACK).size().x + 9.0).max(19.0),
        Face::Icon(_) => 19.0,
    }
}

/// One badge: a white rounded tile with the option's icon (or text) in near-black, blue-edged while its menu is open.
pub(super) fn paint(painter: &egui::Painter, center: Pos2, face: Face, open: bool) -> Rect {
    let ink = super::viewport::PILL_INK;
    let text = match face {
        Face::Text(t) => Some(painter.layout_no_wrap(t.to_string(), theme::strong(9.5), ink)),
        Face::Icon(_) => None,
    };
    let w = text.as_ref().map_or(19.0, |g| (g.size().x + 9.0).max(19.0));
    let r = Rect::from_center_size(center, vec2(w, 18.0));
    painter.rect_filled(r.translate(vec2(0.0, 1.0)), radius(5), Color32::from_black_alpha(40));
    painter.rect_filled(r, radius(5), Color32::WHITE);
    if open {
        painter.rect_stroke(r, radius(5), Stroke::new(1.8, Tokens::accent()), StrokeKind::Outside);
    }
    match (face, text) {
        (_, Some(g)) => painter.galley(center - g.size() / 2.0, g, ink),
        (Face::Icon(icon), None) => icons::paint_on(painter, Rect::from_center_size(center, vec2(14.0, 14.0)), icon, ink, Color32::WHITE),
        _ => {}
    }
    r
}

/// The open badge's menu: a popover pointing left at the badge, a dim title, then the options with their icons and
/// a check on the current one.
pub(super) fn menu(ws: &mut Workspace, ui: &mut Ui, b: Badge, at: Pos2) {
    let Some(current) = ws.badge_current(b) else {
        ws.badge_menu = None;
        return;
    };
    let (title, rows) = Workspace::badge_menu_rows(b);
    let row_h = 22.0;
    let width = if rows.iter().any(|r| r.1.is_none()) { 150.0 } else { 128.0 };
    let rect = Rect::from_min_size(pos2(at.x + 8.0, at.y - 22.0), vec2(width, 26.0 + row_h * rows.len() as f32 + 6.0));
    let mut chosen = None;
    let shown = egui::Area::new(ui.scope_id().with("badge-menu")).fixed_pos(rect.min - vec2(8.0, 0.0)).order(egui::Order::Foreground).show(
        ui.ctx(),
        |ui| {
            let (r, _) = ui.allocate_exact_size(rect.size() + vec2(8.0, 0.0), Sense::hover());
            let rect = Rect::from_min_max(pos2(r.left() + 8.0, r.top()), r.max);
            let p = ui.painter();
            p.rect_filled(rect.translate(vec2(0.0, 4.0)), radius(9), Color32::from_black_alpha(70));
            let fill = theme::c(0x2e2e30);
            p.rect_filled(rect, radius(9), fill);
            p.rect_stroke(rect, radius(9), Stroke::new(1.0, theme::c(0x48484a)), StrokeKind::Inside);
            let tip = pos2(rect.left() - 7.0, at.y);
            p.add(egui::Shape::convex_polygon(
                vec![tip, pos2(rect.left() + 1.0, tip.y - 7.0), pos2(rect.left() + 1.0, tip.y + 7.0)],
                fill,
                Stroke::NONE,
            ));
            p.text(pos2(rect.left() + 12.0, rect.top() + 14.0), Align2::LEFT_CENTER, title, theme::body(11.5), Tokens::text_dim());
            for (i, (label, icon, enabled)) in rows.iter().enumerate() {
                let row = Rect::from_min_size(pos2(rect.left() + 5.0, rect.top() + 26.0 + row_h * i as f32), vec2(width - 10.0, row_h));
                let sense = if *enabled { Sense::click() } else { Sense::hover() };
                let resp = ui.interact(row, ui.scope_id().with(("badge-row", i)), sense);
                let ink = if *enabled { Tokens::text_strong() } else { Tokens::text_faint() };
                if i > 0 {
                    // Hairlines between the options, as observed (s25-016).
                    let y = row.top();
                    p.line_segment([pos2(row.left() + 24.0, y), pos2(row.right() - 4.0, y)], Stroke::new(1.0, theme::c(0x3c3c3e)));
                }
                let lit = resp.hovered() && *enabled;
                let bg = if lit { theme::c(0x2f6df5) } else { fill };
                if lit {
                    p.rect_filled(row, radius(4), bg);
                }
                if i == current {
                    p.text(pos2(row.left() + 6.0, row.center().y), Align2::LEFT_CENTER, "✓", theme::body(12.5), Tokens::text_strong());
                }
                let text_x = match icon {
                    Some(icon) => {
                        icons::paint_on(
                            p,
                            Rect::from_center_size(pos2(row.left() + 28.0, row.center().y), vec2(16.0, 16.0)),
                            *icon,
                            ink,
                            bg,
                        );
                        40.0
                    }
                    None => 20.0,
                };
                p.text(pos2(row.left() + text_x, row.center().y), Align2::LEFT_CENTER, *label, theme::body(12.5), ink);
                if resp.clicked() && *enabled {
                    chosen = Some(i);
                }
            }
        },
    );
    let outside = ws.now - ws.popup_opened_at > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !shown.response.rect.contains(q)));
    if let Some(i) = chosen {
        ws.badge_pick(b, i);
    }
    if chosen.is_some() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.badge_menu = None;
    }
}

/// How long a cycled option's bubble stays up, in seconds (the banner comes back after about this, as observed).
const BUBBLE_SECS: f64 = 2.5;

/// The bubble a tool's key leaves when it cycles an option (26.200, s25 e2): the option's name, the choice under it,
/// and the choices as a segmented row of icons that can be clicked too. True while it shows.
pub(super) fn cycle_bubble(ws: &mut Workspace, ui: &mut Ui) -> bool {
    let Some((b, since)) = ws.cycled else { return false };
    let (Some(current), true) = (ws.badge_current(b), ws.now - since < BUBBLE_SECS) else {
        ws.cycled = None;
        return false;
    };
    let (title, rows) = Workspace::badge_menu_rows(b);
    let p = ui.painter().clone();
    let vp = ws.viewport_rect;
    let cell = vec2(34.0, 24.0);
    let seg_w = cell.x * rows.len() as f32 + 4.0;
    let name = rows.get(current).map_or("", |r| r.0);
    let text_w = p.layout_no_wrap(name.to_string(), theme::body(11.5), Tokens::text_dim()).size().x.max(40.0);
    let r = Rect::from_min_size(pos2(vp.center().x - (text_w + seg_w + 40.0) / 2.0, vp.top() + 8.0), vec2(text_w + seg_w + 40.0, 46.0));
    p.rect_filled(r, radius(10), Tokens::banner());
    p.text(pos2(r.left() + 14.0, r.top() + 15.0), Align2::LEFT_CENTER, title, theme::strong(13.0), Tokens::text_strong());
    p.text(pos2(r.left() + 14.0, r.top() + 31.0), Align2::LEFT_CENTER, name, theme::body(11.5), Tokens::text_dim());
    let seg = Rect::from_min_size(pos2(r.right() - 12.0 - seg_w, r.center().y - cell.y / 2.0 - 2.0), vec2(seg_w, cell.y + 4.0));
    p.rect_filled(seg, radius(7), Tokens::button());
    for (i, (_, icon, enabled)) in rows.iter().enumerate() {
        let c = Rect::from_min_size(pos2(seg.left() + 2.0 + cell.x * i as f32, seg.top() + 2.0), cell);
        let bg = if i == current { theme::c(0x5a5a5e) } else { Tokens::button() };
        if i == current {
            p.rect_filled(c, radius(6), bg);
        }
        if let Some(icon) = icon {
            icons::paint_on(&p, Rect::from_center_size(c.center(), vec2(16.0, 16.0)), *icon, Tokens::text_strong(), bg);
        }
        if *enabled && ui.interact(c, ui.scope_id().with(("cycle-cell", i)), Sense::click()).clicked() {
            ws.badge_pick(b, i);
            ws.cycled = Some((b, ws.now));
        }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    true
}

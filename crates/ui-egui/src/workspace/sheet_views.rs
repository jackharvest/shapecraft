//! Placing views on a drawing sheet, as observed in the Views flyout: Base View (an orientation from an
//! unfolded-cube popover, placed with a click), Projection (the eight neighbours of a view as ghosts, each turned on
//! with its ⊕), Section View (a line across a view, then the cut dragged out) and Detail View (a circle, then the
//! magnified view placed). A selected view shows in cyan with its scale and hidden-line toggle, and drags to move.

use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use glam::DVec2;
use serde_json::json;
use shapecraft_doc::{SheetView, SheetViewKind};
use shapecraft_engine::drawing::{self, Drawing, Mark};

use super::Workspace;
use crate::theme::{self, Tokens, radius};

/// A Views tool running on the sheet.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewTool {
    /// Placing a base view looking from `key` (see `drawing::orientation`) at `at` (sheet mm; the sheet's middle
    /// until the sheet is clicked elsewhere); `popover` shows the orientations.
    Base { key: &'static str, popover: bool, at: Option<DVec2> },
    /// Turning on the projections of a view (none picked yet: click one).
    Projection { parent: Option<String> },
    /// The section line's ends (sheet mm) and the view they were drawn on.
    Section { parent: Option<String>, picks: Vec<DVec2> },
    /// The detail circle's centre and a point on it (sheet mm), and the view.
    Detail { parent: Option<String>, picks: Vec<DVec2> },
}

impl ViewTool {
    pub fn from_index(i: u8) -> Option<Self> {
        match i {
            0 => Some(Self::Base { key: "front", popover: false, at: None }),
            1 => Some(Self::Projection { parent: None }),
            2 => Some(Self::Section { parent: None, picks: Vec::new() }),
            3 => Some(Self::Detail { parent: None, picks: Vec::new() }),
            _ => None,
        }
    }

    /// Only Projection has a Done; the others are cancelled.
    pub fn has_done(&self) -> bool {
        matches!(self, Self::Projection { parent: Some(_) })
    }
}

/// The Base View popover's grid: the cube unfolded around the front, isometrics in the corners (as observed).
const CUBE: [&[(&str, &str)]; 3] = [
    &[("iso_tl", "Isometric"), ("bottom", "Bottom"), ("iso_tr", "Isometric")],
    &[("right", "Right"), ("front", "Front"), ("left", "Left"), ("back", "Back")],
    &[("iso_bl", "Isometric"), ("top", "Top"), ("iso_br", "Isometric")],
];

fn cyan() -> Color32 {
    theme::c(0x3cd0f2)
}

/// Standard view scales offered in the view's scale pop-up, largest first.
fn scale_choices() -> Vec<f64> {
    let mut v: Vec<f64> = [50.0, 25.0, 20.0, 16.0, 12.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0].to_vec();
    v.extend(
        [2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 12.0, 16.0, 20.0, 25.0, 32.0, 40.0, 50.0, 75.0, 100.0, 200.0, 500.0, 1000.0]
            .map(|n| 1.0 / n),
    );
    v
}

impl Workspace {
    pub(super) fn start_view_tool(&mut self, tool: ViewTool) {
        if let Some(d) = &mut self.drawings {
            d.tool = None;
            d.picks.clear();
            d.selected = None;
            // Projection starts from the selected view, if there is one.
            d.view_tool = Some(match tool {
                ViewTool::Projection { parent: None } => ViewTool::Projection { parent: d.view_sel.clone() },
                t => t,
            });
        }
        self.flyout = None;
    }

    pub(super) fn end_view_tool(&mut self) {
        if let Some(d) = &mut self.drawings {
            d.view_tool = None;
            d.preview = None;
        }
    }

    /// The open drawing's views as placed (a sheet that laid them out by itself gets them placed).
    fn placed(&self) -> Option<shapecraft_doc::DrawingSheet> {
        let id = self.drawings.as_ref()?.id;
        let mut spec = self.session.doc.drawing(id)?.clone();
        if spec.views.is_none() {
            spec.views = Some(drawing::standard_views(&spec, spec.four_views));
        }
        Some(spec)
    }

    /// The sheet with some views added for show (ghosts, thumbnails), at the sheet's present scale; kept until the
    /// model, the drawing or `tag` changes.
    fn preview(&mut self, tag: &str, scale: f64, extra: Vec<SheetView>) -> Option<Drawing> {
        use std::hash::{Hash, Hasher};
        let mut spec = self.placed()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.session.revision.hash(&mut h);
        serde_json::to_string(&spec).unwrap_or_default().hash(&mut h);
        tag.hash(&mut h);
        let key = h.finish();
        if let Some((k, d)) = self.drawings.as_ref().and_then(|d| d.preview.as_ref())
            && *k == key
        {
            return Some(d.clone());
        }
        spec.scale = Some(scale);
        if let Some(v) = &mut spec.views {
            v.extend(extra);
        }
        let made = drawing::make(&self.session, &spec).ok()?;
        if let Some(d) = &mut self.drawings {
            d.preview = Some((key, made.clone()));
        }
        Some(made)
    }

    fn add_view(&mut self, params: serde_json::Value) -> Option<String> {
        let id = self.drawings.as_ref()?.id;
        let mut params = params;
        params["drawing"] = json!(id.0);
        let out = self.run("add_drawing_view", params)?;
        out["view"].as_str().map(str::to_string)
    }
}

fn banner(p: &egui::Painter, ws: &Workspace, title: &str, body: &str) {
    let galley_w = p.layout_no_wrap(crate::i18n::tr_s(body), theme::body(11.0), Tokens::text_dim()).size().x;
    let banner =
        Rect::from_center_size(pos2(ws.viewport_rect.center().x, ws.viewport_rect.top() + 34.0), vec2((galley_w + 28.0).max(330.0), 44.0));
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
}

/// A white ⊕ like the reference's add buttons; true when clicked.
fn plus(ui: &mut Ui, at: Pos2, id: (i8, i8)) -> bool {
    let r = Rect::from_center_size(at, vec2(26.0, 26.0));
    let resp = ui.interact(r, ui.scope_id().with(("view-plus", id)), Sense::click());
    let p = ui.painter();
    p.circle_filled(at, 12.0, if resp.hovered() { Color32::WHITE } else { theme::c(0xe8e8e8) });
    p.line_segment([at - vec2(6.0, 0.0), at + vec2(6.0, 0.0)], Stroke::new(2.0, theme::c(0x222222)));
    p.line_segment([at - vec2(0.0, 6.0), at + vec2(0.0, 6.0)], Stroke::new(2.0, theme::c(0x222222)));
    resp.clicked()
}

/// A small white pill with dark text; true when clicked.
fn white_pill(ui: &mut Ui, center: Pos2, text: &str, id: &str) -> (Rect, bool) {
    let galley = ui.painter().layout_no_wrap(crate::i18n::tr_s(text), theme::strong(10.5), theme::c(0x1e1e1e));
    let r = Rect::from_center_size(center, vec2(galley.size().x + 14.0, 18.0));
    let resp = ui.interact(r, ui.scope_id().with(("view-pill", id)), Sense::click());
    ui.painter().rect_filled(r, radius(4), if resp.hovered() { Color32::WHITE } else { theme::c(0xeeeeee) });
    ui.painter().galley(pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, theme::c(0x1e1e1e));
    (r, resp.clicked())
}

/// Draws a view's lines (and what belongs to it) in one colour, moved by `offset` sheet mm.
fn paint_view(
    p: &egui::Painter,
    v: &shapecraft_engine::drawing::DrawingView,
    to: &dyn Fn(DVec2) -> Pos2,
    offset: DVec2,
    color: Color32,
    width: f32,
) {
    for l in &v.lines {
        let pts: Vec<Pos2> = l.points.iter().map(|q| to(*q + offset)).collect();
        let stroke = Stroke::new(if l.hidden { width * 0.6 } else { width }, if l.hidden { color.gamma_multiply(0.6) } else { color });
        if l.hidden {
            p.add(egui::Shape::dashed_line(&pts, stroke, 4.0, 3.0));
        } else {
            p.add(egui::Shape::line(pts, stroke));
        }
    }
    for m in &v.extra {
        if let Mark::Line { a, b, dashed, .. } = m {
            let stroke = Stroke::new(width * 0.7, color);
            if *dashed {
                p.add(egui::Shape::dashed_line(&[to(*a + offset), to(*b + offset)], stroke, 8.0, 3.0));
            } else {
                p.line_segment([to(*a + offset), to(*b + offset)], stroke);
            }
        }
    }
}

/// Snap points of the views: line ends (white) and the middles of straight lines (purple), as observed.
fn snap_points(d: &Drawing) -> Vec<(DVec2, bool)> {
    let mut out = Vec::new();
    for l in d.views.iter().flat_map(|v| v.lines.iter()).filter(|l| !l.hidden) {
        let (Some(a), Some(b)) = (l.points.first().copied(), l.points.last().copied()) else { continue };
        out.push((a, false));
        out.push((b, false));
        if l.points.len() == 2 {
            out.push(((a + b) / 2.0, true));
        }
    }
    out
}

/// The Views tools and view selection over the drawn sheet. Returns true while a Views tool runs (the annotation
/// tools then stay out of the way).
pub(super) fn interact(ws: &mut Workspace, ui: &mut Ui, d: &Drawing, sheet: Rect, k: f32) -> bool {
    let to = |q: DVec2| pos2(sheet.left() + q.x as f32 * k, sheet.bottom() - q.y as f32 * k);
    let from = |p: Pos2| DVec2::new(((p.x - sheet.left()) / k) as f64, ((sheet.bottom() - p.y) / k) as f64);
    let p = ui.painter_at(ws.view_rect);
    let Some(state) = ws.drawings.as_ref() else { return false };
    let tool = state.view_tool.clone();
    let cursor = ui.input(|i| i.pointer.hover_pos()).filter(|p| sheet.contains(*p));
    let q = cursor.map(from);
    let Some(tool) = tool else {
        selected_view(ws, ui, d, sheet, k);
        return false;
    };
    let resp = ui.interact(sheet, ui.scope_id().with("sheet-views"), Sense::click());
    let clicked = resp.clicked();
    let reach = (10.0 / k) as f64;
    match tool {
        ViewTool::Base { key, popover, at } => {
            banner(&p, ws, "Base View", "Choose base view orientation, then add it to the sheet and create projections.");
            let keys: Vec<&str> = CUBE.iter().flat_map(|r| r.iter().map(|(k, _)| *k)).collect();
            let extra: Vec<SheetView> = keys
                .iter()
                .filter_map(|k| {
                    let (toward, x) = drawing::orientation(k)?;
                    Some(SheetView {
                        name: format!("__{k}"),
                        kind: SheetViewKind::Base { toward, x },
                        at: DVec2::ZERO,
                        scale: None,
                        hidden_lines: true,
                        bodies: Vec::new(),
                    })
                })
                .collect();
            let Some(pre) = ws.preview("base", d.scale, extra) else { return true };
            let ghost = pre.views.iter().find(|v| v.name == format!("__{key}"));
            // The ghost waits in the sheet's middle (a click elsewhere moves it), ⊕ in it, its pill under it.
            let at = at.unwrap_or_else(|| ws.placed().map_or(d.sheet / 2.0, |s| drawing::sheet_point(&s, 0.5, 0.5)));
            let mut frame = Rect::from_center_size(to(at), vec2(120.0, 120.0));
            if let Some(g) = ghost {
                paint_view(&p, g, &to, at, Color32::from_white_alpha(150), 1.0);
                frame = Rect::from_two_pos(to(at + g.lo), to(at + g.hi)).expand(12.0);
            }
            p.rect_stroke(frame, 0.0, Stroke::new(1.0, theme::c(0x546d7a)), StrokeKind::Outside);
            // Two small pills under the box's right corner: the orientation, then the bodies.
            let label = CUBE.iter().flat_map(|r| r.iter()).find(|(k2, _)| *k2 == key).map_or("Front", |(_, n)| *n);
            let (pill, open) = white_pill(ui, frame.right_bottom() + vec2(-24.0, 22.0), label, "base-orient");
            let (bodies, _) = white_pill(ui, frame.right_bottom() + vec2(-24.0, 46.0), "Bodies…", "base-bodies");
            let mut over_ui = pill.union(bodies).contains(cursor.unwrap_or(Pos2::ZERO));
            let mut new_key = key;
            let mut pop = popover ^ open;
            if pop {
                // To the right of the pills, as observed (s12-125): a grid of the cube unfolded around the front.
                let (cell, gap) = (80.0, 4.0);
                let box_size = vec2(cell * 4.0 + gap * 3.0 + 30.0, 46.0 + (cell + 22.0) * 3.0 + 44.0);
                let r = Rect::from_min_size(pos2(pill.right() + 10.0, pill.center().y - box_size.y / 2.0), box_size);
                let r = r.translate(vec2(0.0, (ws.viewport_rect.top() + 60.0 - r.top()).max(0.0)));
                over_ui |= r.contains(cursor.unwrap_or(Pos2::ZERO));
                let pp = ui.painter().clone();
                pp.rect_filled(r.translate(vec2(0.0, 4.0)), radius(10), Color32::from_black_alpha(80));
                pp.rect_filled(r, radius(10), theme::c(0x1d2b3b));
                pp.rect_stroke(r, radius(10), Stroke::new(1.0, theme::c(0x3a4656)), StrokeKind::Inside);
                pp.text(
                    pos2(r.left() + 15.0, r.top() + 24.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr("Base View"),
                    theme::strong(12.5),
                    Tokens::text_strong(),
                );
                for (row, cells) in CUBE.iter().enumerate() {
                    // Rows of three sit over the first three of the middle row's four, as the cube unfolds.
                    for (i, (k2, name)) in cells.iter().enumerate() {
                        let c = Rect::from_min_size(
                            pos2(r.left() + 15.0 + i as f32 * (cell + gap), r.top() + 44.0 + row as f32 * (cell + 22.0)),
                            vec2(cell, cell),
                        );
                        let resp = ui.interact(c.expand2(vec2(0.0, 10.0)), ui.scope_id().with(("cube", row, i)), Sense::click());
                        pp.rect_filled(c, radius(4), if resp.hovered() { theme::c(0x2a2a2a) } else { theme::c(0x1e1e1e) });
                        if *k2 == key {
                            pp.rect_stroke(c, radius(4), Stroke::new(2.0, Tokens::accent()), StrokeKind::Inside);
                        }
                        if let Some(v) = pre.views.iter().find(|v| v.name == format!("__{k2}")) {
                            let size = (v.hi - v.lo).max(DVec2::splat(1e-6));
                            let s = ((c.width() - 14.0) / size.x.max(size.y) as f32).min(8.0);
                            let mid = (v.lo + v.hi) / 2.0;
                            let tc = |q: DVec2| c.center() + vec2(((q - mid).x as f32) * s, -((q - mid).y as f32) * s);
                            for l in v.lines.iter().filter(|l| !l.hidden) {
                                pp.add(egui::Shape::line(l.points.iter().map(|q| tc(*q)).collect(), Stroke::new(1.0, Tokens::text())));
                            }
                        }
                        pp.text(pos2(c.center().x, c.bottom() + 11.0), Align2::CENTER_CENTER, *name, theme::body(11.0), Tokens::text());
                        if resp.clicked() {
                            new_key = k2;
                            pop = false;
                        }
                    }
                }
                let custom = Rect::from_min_size(pos2(r.left() + 15.0, r.bottom() - 34.0), vec2(r.width() - 30.0, 20.0));
                let cr = ui.interact(custom, ui.scope_id().with("custom-base"), Sense::click());
                pp.rect_filled(custom, radius(4), if cr.hovered() { theme::c(0x6d7680) } else { theme::c(0x5d6670) });
                pp.text(
                    custom.center(),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr("Custom Base View…"),
                    theme::body(12.0),
                    Tokens::text_strong(),
                );
                // The model as it's seen in Modeling: the view looks the way the camera does, its right to the right.
                if cr.clicked() {
                    let toward = ws.camera.back();
                    let x = glam::DVec3::Z.cross(toward).try_normalize().unwrap_or(glam::DVec3::X);
                    if let Some(name) =
                        ws.add_view(json!({ "kind": "base", "toward": [toward.x, toward.y, toward.z], "x": [x.x, x.y, x.z], "at": at }))
                        && let Some(st) = &mut ws.drawings
                    {
                        st.view_tool = Some(ViewTool::Projection { parent: Some(name.clone()) });
                        st.view_sel = Some(name);
                        st.preview = None;
                    }
                    return true;
                }
            }
            if plus(ui, to(at), (9, 9)) {
                if let Some(name) = ws.add_view(json!({ "kind": "base", "orientation": key, "at": at }))
                    && let Some(st) = &mut ws.drawings
                {
                    st.view_tool = Some(ViewTool::Projection { parent: Some(name.clone()) });
                    st.view_sel = Some(name);
                    st.preview = None;
                }
                return true;
            }
            let mut at = Some(at);
            if clicked && !over_ui && !open {
                if pop {
                    pop = false;
                } else {
                    at = q.or(at);
                }
            }
            if let Some(st) = &mut ws.drawings {
                st.view_tool = Some(ViewTool::Base { key: new_key, popover: pop, at });
            }
        }
        ViewTool::Projection { parent: None } => {
            banner(&p, ws, "Projection", "Select a view, then turn on all necessary projections.");
            let hover = q.and_then(|q| d.view_at(q));
            if let Some(v) = hover {
                paint_view(&p, v, &to, DVec2::ZERO, cyan().gamma_multiply(0.7), 1.2);
            }
            if clicked
                && let Some(v) = hover
                && let Some(st) = &mut ws.drawings
            {
                st.view_tool = Some(ViewTool::Projection { parent: Some(v.name.clone()) });
                st.view_sel = Some(v.name.clone());
                st.preview = None;
            }
        }
        ViewTool::Projection { parent: Some(parent) } => {
            banner(&p, ws, "Projection", "Select a view, then turn on all necessary projections.");
            let Some(pv) = d.views.iter().find(|v| v.name == parent) else {
                ws.end_view_tool();
                return true;
            };
            paint_view(&p, pv, &to, DVec2::ZERO, cyan(), 1.4);
            let spec = ws.placed();
            let taken = |dx: i8, dy: i8| {
                spec.as_ref().and_then(|s| s.views.as_ref()).is_some_and(|vs| {
                    vs.iter().any(|v| matches!(&v.kind, SheetViewKind::Projected { parent: pn, dx: x, dy: y } if *pn == parent && *x == dx && *y == dy))
                })
            };
            let gap = pv.hi - pv.lo + DVec2::splat(25.0);
            let cells: Vec<(i8, i8)> = [(-1, 1), (0, 1), (1, 1), (-1, 0), (1, 0), (-1, -1), (0, -1), (1, -1)]
                .into_iter()
                .filter(|(dx, dy)| !taken(*dx, *dy))
                .collect();
            let extra: Vec<SheetView> = cells
                .iter()
                .map(|(dx, dy)| SheetView {
                    name: format!("__{dx}_{dy}"),
                    kind: SheetViewKind::Projected { parent: parent.clone(), dx: *dx, dy: *dy },
                    at: pv.at + DVec2::new(f64::from(*dx) * gap.x, f64::from(*dy) * gap.y),
                    scale: None,
                    hidden_lines: true,
                    bodies: Vec::new(),
                })
                .collect();
            let tag = format!("proj {parent}");
            let Some(pre) = ws.preview(&tag, d.scale, extra) else { return true };
            for (dx, dy) in cells {
                let Some(g) = pre.views.iter().find(|v| v.name == format!("__{dx}_{dy}")) else { continue };
                paint_view(&p, g, &to, DVec2::ZERO, Color32::from_white_alpha(70), 1.0);
                if plus(ui, to(g.at), (dx, dy)) {
                    let at = g.at;
                    let _ = ws.add_view(json!({ "kind": "projected", "parent": parent, "dx": dx, "dy": dy, "at": at }));
                    if let Some(st) = &mut ws.drawings {
                        st.preview = None;
                    }
                }
            }
        }
        ViewTool::Section { parent, picks } => {
            let snaps = snap_points(d);
            let snapped = q.map(|q| {
                snaps
                    .iter()
                    .filter(|(s, _)| s.distance(q) < reach)
                    .min_by(|a, b| a.0.distance(q).total_cmp(&b.0.distance(q)))
                    .map_or(q, |s| s.0)
            });
            if picks.len() < 2 {
                banner(&p, ws, "Section View", "Draw a line to create a section line; reference points help alignment");
                let focus = parent.as_deref().map(str::to_string).or_else(|| q.and_then(|q| d.view_at(q)).map(|v| v.name.clone()));
                if let Some(v) = focus.as_deref().and_then(|n| d.views.iter().find(|v| v.name == n)) {
                    for (s, mid) in snaps.iter().filter(|(s, _)| s.cmpge(v.lo - DVec2::ONE).all() && s.cmple(v.hi + DVec2::ONE).all()) {
                        p.circle_filled(to(*s), 2.5, if *mid { theme::c(0xb07cff) } else { Color32::WHITE });
                    }
                }
                if let (Some(a), Some(b)) = (picks.first(), snapped) {
                    p.add(egui::Shape::dashed_line(&[to(*a), to(b)], Stroke::new(1.5, cyan()), 8.0, 3.0));
                }
                if clicked
                    && let Some(s) = snapped
                    && let Some(st) = &mut ws.drawings
                    && picks.first().is_none_or(|a| a.distance(s) > 1e-6)
                {
                    // The line may start off the view: it cuts the view its middle is in, else the nearest.
                    let mut picks = picks;
                    picks.push(s);
                    let parent = match picks.as_slice() {
                        [a, b] => parent.or_else(|| nearest_view(d, (*a + *b) / 2.0)),
                        _ => parent,
                    };
                    st.view_tool = Some(ViewTool::Section { parent, picks });
                }
            } else if let (Some(pn), Some(pv)) = (parent.as_deref(), parent.as_deref().and_then(|n| d.views.iter().find(|v| v.name == n))) {
                banner(&p, ws, "Section View", "Move the pointer to place the section view, then click");
                let (a, b) = (picks[0], picks[1]);
                let dir = (b - a).normalize_or_zero();
                let n = dir.perp();
                p.add(egui::Shape::dashed_line(&[to(a - dir * 4.0), to(b + dir * 4.0)], Stroke::new(1.5, cyan()), 8.0, 3.0));
                let mid = (a + b) / 2.0;
                for s in [1.0, -1.0] {
                    let tip = mid + n * 7.0 * s;
                    p.arrow(to(mid), to(tip) - to(mid), Stroke::new(1.5, cyan()));
                }
                let Some(q) = q else { return true };
                // The view slides out along the cut's normal, level with its parent.
                let along = n * (q - pv.at).dot(n);
                let at = pv.at + along;
                let size = pv.hi - pv.lo;
                let ghost = Rect::from_center_size(to(at), vec2(size.x as f32 * k, size.y as f32 * k));
                p.rect_stroke(ghost, radius(4), Stroke::new(1.0, Color32::from_white_alpha(120)), StrokeKind::Outside);
                p.text(
                    ghost.center_bottom() + vec2(0.0, 14.0),
                    Align2::CENTER_CENTER,
                    crate::i18n::tr("Section"),
                    theme::body(11.0),
                    Color32::from_white_alpha(170),
                );
                if clicked && let (Some(la), Some(lb)) = (d.sheet_to_named_view(pn, a), d.sheet_to_named_view(pn, b)) {
                    // The arrows look across the cut: toward where the view goes in first angle, away in third.
                    let toward_view = (q - mid).dot(n) > 0.0;
                    let flip = toward_view != d.first_angle;
                    let name = ws.add_view(json!({ "kind": "section", "parent": pn, "a": la, "b": lb, "flip": flip, "at": at }));
                    ws.end_view_tool();
                    if let Some(st) = &mut ws.drawings {
                        st.view_sel = name;
                    }
                }
            } else {
                ws.end_view_tool();
            }
        }
        ViewTool::Detail { parent, picks } => {
            let snaps = snap_points(d);
            let snapped = q.map(|q| {
                snaps
                    .iter()
                    .filter(|(s, _)| s.distance(q) < reach)
                    .min_by(|a, b| a.0.distance(q).total_cmp(&b.0.distance(q)))
                    .map_or(q, |s| s.0)
            });
            if picks.len() < 2 {
                banner(&p, ws, "Detail View", "Draw a circle from its centre to define the magnified area");
                if let (Some(c), Some(e)) = (picks.first(), q) {
                    let r = c.distance(e) as f32 * k;
                    p.add(egui::Shape::dashed_line(&circle_pts(to(*c), r), Stroke::new(1.5, cyan()), 8.0, 3.0));
                }
                if clicked && let Some(s) = if picks.is_empty() { snapped } else { q } {
                    let parent = parent.or_else(|| nearest_view(d, s));
                    if parent.is_some()
                        && let Some(st) = &mut ws.drawings
                    {
                        let mut picks = picks;
                        if picks.first().is_none_or(|a| a.distance(s) > 1e-6) {
                            picks.push(s);
                        }
                        st.view_tool = Some(ViewTool::Detail { parent, picks });
                    }
                }
            } else if let (Some(pn), Some(pv)) = (parent.as_deref(), parent.as_deref().and_then(|n| d.views.iter().find(|v| v.name == n))) {
                banner(&p, ws, "Detail View", "Move the pointer to place the magnified view, then click");
                let (c, e) = (picks[0], picks[1]);
                let r = c.distance(e);
                p.add(egui::Shape::dashed_line(&circle_pts(to(c), r as f32 * k), Stroke::new(1.5, cyan()), 8.0, 3.0));
                let Some(q) = q else { return true };
                // Twice the parent's scale, as the reference starts it.
                let big = r * 2.0 * k as f64;
                p.add(egui::Shape::closed_line(circle_pts(to(q), big as f32), Stroke::new(1.2, Color32::from_white_alpha(140))));
                let mag = 2.0;
                if let Some(mid) = d.sheet_to_named_view(pn, c) {
                    // The parent's lines inside the circle, magnified about its centre.
                    let visible: Vec<_> = pv.lines.iter().filter(|l| !l.hidden).cloned().collect();
                    for l in drawing::clip_to_circle(&visible, c, r) {
                        p.add(egui::Shape::line(
                            l.points.iter().map(|s| to(q + (*s - c) * mag)).collect(),
                            Stroke::new(1.0, Color32::from_white_alpha(150)),
                        ));
                    }
                    if clicked && let Some(edge) = d.sheet_to_named_view(pn, e) {
                        let name =
                            ws.add_view(json!({ "kind": "detail", "parent": pn, "center": mid, "radius": mid.distance(edge), "at": q }));
                        ws.end_view_tool();
                        if let Some(st) = &mut ws.drawings {
                            st.view_sel = name;
                        }
                    }
                }
            } else {
                ws.end_view_tool();
            }
        }
    }
    true
}

/// The view holding a sheet point, else the one whose extent comes nearest it.
fn nearest_view(d: &Drawing, q: DVec2) -> Option<String> {
    d.view_at(q)
        .or_else(|| d.views.iter().min_by(|a, b| (q.clamp(a.lo, a.hi).distance(q)).total_cmp(&q.clamp(b.lo, b.hi).distance(q))))
        .map(|v| v.name.clone())
}

fn circle_pts(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..=72).map(|i| c + egui::Vec2::angled(std::f32::consts::TAU * i as f32 / 72.0) * r).collect()
}

/// The selected view in cyan with its scale pill and hidden-line toggle under it; any view drags to move.
fn selected_view(ws: &mut Workspace, ui: &mut Ui, d: &Drawing, sheet: Rect, k: f32) {
    let to = |q: DVec2| pos2(sheet.left() + q.x as f32 * k, sheet.bottom() - q.y as f32 * k);
    let from = |p: Pos2| DVec2::new(((p.x - sheet.left()) / k) as f64, ((sheet.bottom() - p.y) / k) as f64);
    let p = ui.painter_at(ws.view_rect);
    let resp = ui.interact(sheet, ui.scope_id().with("sheet-drag"), Sense::drag());
    let Some(state) = ws.drawings.as_ref() else { return };
    let (id, sel, drag) = (state.id, state.view_sel.clone(), state.drag.clone());
    let spec = ws.placed();
    let kind_of = |name: &str| spec.as_ref().and_then(|s| s.views.as_ref()).and_then(|vs| vs.iter().find(|v| v.name == name)).cloned();
    if resp.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        && let Some(v) = d.view_at(from(origin))
        && let Some(st) = &mut ws.drawings
    {
        st.drag = Some((v.name.clone(), from(origin)));
        st.view_sel = Some(v.name.clone());
        st.selected = None;
    }
    // The move, kept level with the parent for a side or top projection.
    let delta = |name: &str, start: DVec2| {
        let now = ui.input(|i| i.pointer.interact_pos()).map(from).unwrap_or(start);
        let mut dlt = now - start;
        if let Some(SheetView { kind: SheetViewKind::Projected { dx, dy, .. }, .. }) = kind_of(name) {
            if dy == 0 {
                dlt.y = 0.0;
            } else if dx == 0 {
                dlt.x = 0.0;
            }
        }
        dlt
    };
    if let Some((name, start)) = &drag {
        let dlt = delta(name, *start);
        if let Some(v) = d.views.iter().find(|v| v.name == *name) {
            paint_view(&p, v, &to, dlt, cyan(), 1.4);
            if resp.drag_stopped() {
                if dlt.length() > 1e-3 {
                    let at = v.at + dlt;
                    let _ = ws.run("update_drawing_view", json!({ "drawing": id.0, "view": name, "at": at }));
                }
                if let Some(st) = &mut ws.drawings {
                    st.drag = None;
                }
            }
        }
        return;
    }
    let Some(name) = sel else { return };
    let Some(v) = d.views.iter().find(|v| v.name == name) else {
        if let Some(st) = &mut ws.drawings {
            st.view_sel = None;
        }
        return;
    };
    paint_view(&p, v, &to, DVec2::ZERO, cyan(), 1.4);
    let Some(sv) = kind_of(&name) else { return };
    // In the view's middle, as observed (s12-138): move arrows, the scale pill under them, the round toggle under it.
    let mid = to((v.lo + v.hi) / 2.0);
    let col = Color32::WHITE;
    p.circle_filled(mid, 3.5, col);
    p.arrow(mid, vec2(0.0, -22.0), Stroke::new(2.0, col));
    p.arrow(mid, vec2(22.0, 0.0), Stroke::new(2.0, col));
    let under = mid + vec2(0.0, 34.0);
    let mut update = serde_json::Map::new();
    if !matches!(sv.kind, SheetViewKind::Projected { .. }) {
        let choices = scale_choices();
        let names: Vec<String> = choices.iter().map(|s| drawing::scale_label(*s)).collect();
        let at = choices.iter().position(|s| (s - v.scale).abs() < 1e-9 * s.max(1.0));
        let mut names = names;
        let current = at.or_else(|| {
            names.push(drawing::scale_label(v.scale));
            Some(names.len() - 1)
        });
        if let Some(i) = super::popover::pill_select(ui, &format!("view-scale-{name}"), under + vec2(22.0, 0.0), 0.0, &names, current)
            && let Some(s) = choices.get(i)
        {
            update.insert("scale".into(), json!(s));
        }
    }
    // The round toggle: hidden lines.
    let c = under + vec2(0.0, 28.0);
    let r = Rect::from_center_size(c, vec2(22.0, 22.0));
    let t = ui.interact(r, ui.scope_id().with(("view-hidden", &name)), Sense::click());
    ui.painter().circle_filled(c, 10.0, if sv.hidden_lines { Tokens::accent() } else { theme::c(0x555559) });
    ui.painter().add(egui::Shape::dashed_line(&[c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], Stroke::new(1.5, Color32::WHITE), 2.5, 1.5));
    if t.hovered() {
        p.text(c + vec2(16.0, 0.0), Align2::LEFT_CENTER, crate::i18n::tr("Hidden Lines"), theme::body(11.0), Color32::WHITE);
    }
    if t.clicked() {
        update.insert("hidden_lines".into(), json!(!sv.hidden_lines));
    }
    if !update.is_empty() {
        update.insert("drawing".into(), json!(id.0));
        update.insert("view".into(), json!(name));
        let _ = ws.run("update_drawing_view", serde_json::Value::Object(update));
    }
}

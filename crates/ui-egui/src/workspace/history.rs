//! The History sidebar as observed: a parametric timeline. Each step is a row (feature icon, bold name, chevron);
//! hovering shows a ⋯ button (Insert Breakpoint, Suppress, Rename, Delete), the chevron opens the step's parameters
//! inline, and changing one rebuilds the model. A breakpoint is a grey row the steps after it wait behind; failing
//! steps carry a red ⚠ (observed s14).

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;
use shapecraft_doc::{HistoryStep, format_length};
use shapecraft_engine::history::{self, BREAKPOINT, FieldKind};

use super::Workspace;
use super::tools::parse_length;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

/// The step whose parameters are open, and the field being typed into.
#[derive(Default)]
pub struct HistoryUi {
    pub open: Option<usize>,
    /// The step whose ⋯ menu is open, and where.
    pub menu: Option<(usize, egui::Pos2)>,
    pub editing: Option<(usize, &'static str, String, bool)>,
    /// "Related to Selection": the steps found for a selection at a revision (cached, it replays the history).
    related: Option<(u64, Vec<shapecraft_doc::Pick>, Option<Vec<bool>>)>,
    /// The selection whose filter was cleared.
    cleared: Option<Vec<shapecraft_doc::Pick>>,
    /// Merge Modeling Steps' popover under the header: keep sketches, keep variables, and when it opened.
    pub merge: Option<(bool, bool, f64)>,
    /// The step being renamed in place: its index, the text, and whether it still has to take the focus.
    pub renaming: Option<(usize, String, bool)>,
    /// The breakpoint is being dragged.
    bp_drag: bool,
    /// The Import step whose Shapr3D steps are listed out.
    shapr_open: Option<usize>,
}

/// Measured (s14-005): the breakpoint's grey row and the dimmed steps after it.
fn bp_row() -> Color32 {
    theme::c(0x3a3939)
}
fn bp_grip() -> Color32 {
    theme::c(0xa6a6a6)
}
const BP_H: f32 = 28.0;
/// The red panel of a failing step (s14-010).
fn error_panel() -> Color32 {
    theme::c(0x54302f)
}
const ERROR_H: f32 = 46.0;

/// With something selected, History lists only the steps that made it (as observed); `None` shows everything.
fn related_filter(ws: &mut Workspace, busy: bool) -> Option<Vec<bool>> {
    use shapecraft_doc::Pick;
    // While a result waits for Done the filter stays as it was when the tool began, dimmed (26.202, s40-010): the
    // live step and whatever the tool did to the selection don't change it.
    if busy {
        return ws.history.related.as_ref().and_then(|(_, _, r)| r.clone()).filter(|r| r.iter().any(|x| *x));
    }
    let sel = &ws.session.selection;
    if sel.is_empty() || ws.history.cleared.as_ref() == Some(sel) {
        return None;
    }
    let rev = ws.session.revision;
    let fresh = matches!(&ws.history.related, Some((r, s, _)) if *r == rev && s == sel);
    if !fresh {
        let mut bodies: Vec<_> = sel.iter().filter_map(Pick::body).collect();
        bodies.dedup();
        let sketches: Vec<_> = sel
            .iter()
            .filter_map(|p| match *p {
                Pick::Sketch { sketch } | Pick::Region { sketch, .. } | Pick::SketchEntity { sketch, .. } => Some(sketch),
                _ => None,
            })
            .collect();
        let rel = if bodies.is_empty() && sketches.is_empty() { None } else { ws.session.related_steps(&bodies, &sketches) };
        ws.history.related = Some((rev, sel.clone(), rel));
    }
    ws.history.related.as_ref().and_then(|(_, _, r)| r.clone()).filter(|r| r.iter().any(|x| *x))
}

const ROW_H: f32 = 34.0;
const ROW_PITCH: f32 = 36.0;
const FIELD_PITCH: f32 = 32.0;
/// The gray of the reference's pop-up buttons.
fn pill() -> Color32 {
    theme::c(0x555559)
}

/// Our kind of step for a Shapr3D operation, for its icon.
fn shapr_kind(op: &str) -> &'static str {
    match op {
        "Extrude" | "Extrusion" => "extrude",
        "OffsetFace" => "offset_face",
        "Fillet" => "fillet",
        "Chamfer" => "chamfer",
        "Boolean" => "union",
        "Delete" => "delete",
        "Transform" | "Movement" => "translate",
        "Scale" => "scale",
        "Revolve" => "revolve",
        "Loft" => "loft",
        "Sweep" => "sweep",
        "Shell" => "shell",
        "Mirror" => "mirror",
        "Pattern" | "CircularPattern" | "LinearPattern" => "pattern",
        "SplitBody" | "Split" => "split_body",
        "MaterializeImportedBodies" => "import_file",
        o if o.contains("Sketch") => "sketch",
        o if o.contains("Plane") => "offset_plane",
        o if o.contains("Axis") => "axis_2points",
        _ => "",
    }
}

fn icon_for(step: &HistoryStep) -> Icon {
    match step.kind.as_str() {
        "sketch" => Icon::Sketch,
        "merged" => Icon::Merged,
        "extrude" => Icon::Extrude,
        "revolve" => Icon::Revolve,
        "loft" => Icon::Loft,
        "sweep" => Icon::Sweep,
        "fillet" | "chamfer" => Icon::Fillet,
        "shell" => Icon::Shell,
        "union" => Icon::Union,
        "subtract" => Icon::Subtract,
        "intersect" => Icon::Intersect,
        "offset_face" => Icon::OffsetFace,
        "replace_face" => Icon::ReplaceFace,
        "project_edge" => Icon::ProjectEdge,
        "scale" => Icon::Scale,
        "translate" => Icon::Translate,
        "unlinked_copy" => Icon::Body,
        "rotate" => Icon::RotateAxis,
        "mirror" => Icon::Mirror,
        "align" => Icon::Align,
        "pattern" | "pattern_circular" => Icon::Pattern,
        "split_body" => Icon::SplitBody,
        "offset_plane" | "midplane" | "plane_3points" | "plane_through_edge" | "plane_on_curve" => Icon::Plane,
        "axis_perp_face" | "axis_along_edge" | "axis_cylinder" | "axis_2points" => Icon::Axis,
        "import_file" => Icon::Import,
        "insert_image" | "image_settings" => Icon::Insert,
        "delete" => Icon::Delete,
        _ => Icon::Body,
    }
}

fn show_value(kind: FieldKind, v: f64, units: shapecraft_doc::Units) -> String {
    match kind {
        FieldKind::Length => format_length(v, units),
        FieldKind::Angle => {
            let s = format_length(v, shapecraft_doc::Units::Millimeter);
            format!("{}°", s.trim_end_matches(" mm"))
        }
        FieldKind::Count => format!("{}", v.round() as i64),
        FieldKind::Factor => format_length(v, shapecraft_doc::Units::Millimeter).trim_end_matches(" mm").to_string(),
    }
}

fn parse_value(kind: FieldKind, text: &str, units: shapecraft_doc::Units, vars: &[shapecraft_doc::Variable]) -> Option<serde_json::Value> {
    let t = text.trim().trim_end_matches('°').trim_start_matches('×');
    // Anything using a variable stays an expression, so the step follows the variable.
    if t.chars().any(|c| c.is_ascii_alphabetic() && !matches!(c, 'm' | 'c' | 'i' | 'n')) || vars.iter().any(|v| t.contains(v.name.as_str()))
    {
        return shapecraft_doc::expr::eval(t, vars).ok().map(|_| json!(t));
    }
    match kind {
        FieldKind::Length => {
            let has_suffix = t.ends_with("mm") || t.ends_with("cm") || t.ends_with("in");
            let v = parse_length(t)?;
            Some(json!(if has_suffix { v } else { v * units.scale() }))
        }
        FieldKind::Angle | FieldKind::Factor => t.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| json!(v)),
        FieldKind::Count => t.parse::<f64>().ok().filter(|v| *v >= 1.0).map(|v| json!(v.round() as i64)),
    }
}

/// A read-only choice, drawn like the reference's pop-up buttons (text on a gray pill, blue stepper square).
fn choice(p: &egui::Painter, right: f32, y: f32, text: &str) {
    let galley = p.layout_no_wrap(text.to_string(), theme::body(12.0), Tokens::text_strong());
    let w = galley.size().x + 32.0;
    let r = Rect::from_min_max(pos2(right - w, y - 10.0), pos2(right, y + 10.0));
    p.rect_filled(r, radius(5), pill());
    p.galley(pos2(r.left() + 9.0, y - galley.size().y / 2.0), galley, Tokens::text_strong());
    let knob = Rect::from_center_size(pos2(r.right() - 9.5, y), vec2(14.0, 15.0));
    p.rect_filled(knob, radius(4), Tokens::accent());
    icons::paint(p, Rect::from_center_size(knob.center() - vec2(0.0, 2.5), vec2(7.0, 7.0)), Icon::ChevronUp, Color32::WHITE);
    icons::paint(p, Rect::from_center_size(knob.center() + vec2(0.0, 2.5), vec2(7.0, 7.0)), Icon::ChevronDown, Color32::WHITE);
}

/// Extrusion's fixed settings as the reference lists them (only Distance and Draft Angle are editable here).
fn extrude_result(step: &HistoryStep) -> &'static str {
    let Some(e) = step.entries.first() else { return "New Body" };
    let on_face = e.params.get("face").is_some() || e.selection.iter().any(|p| matches!(p, shapecraft_doc::Pick::Face { .. }));
    let d = e.params.get("distance").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    match (on_face, d < 0.0) {
        (false, _) => "New Body",
        (true, false) => "Union",
        (true, true) => "Subtract",
    }
}

pub(super) fn sidebar(ws: &mut Workspace, ui: &mut Ui, r: Rect) {
    super::panels::panel(ui, r, ws.glass());
    let p = ui.painter().clone();
    // Measured (L02): a lighter 35 px header band, the title grey 17 px in.
    let band = Rect::from_min_max(r.min + vec2(1.0, 1.0), pos2(r.right() - 1.0, r.top() + 36.0));
    p.rect_filled(
        band,
        egui::CornerRadius { nw: Tokens::PANEL_RADIUS - 1, ne: Tokens::PANEL_RADIUS - 1, sw: 0, se: 0 },
        theme::c(0x2f2d2e),
    );
    let title = p.text(
        pos2(r.left() + 17.0, r.top() + 18.5),
        Align2::LEFT_CENTER,
        crate::i18n::tr("History"),
        theme::strong(12.5),
        theme::c(0xa1a0a0),
    );
    let chevron = pos2(title.right() + 9.0, r.top() + 18.5);
    icons::paint(&p, Rect::from_center_size(chevron, vec2(10.0, 10.0)), Icon::ChevronDown, theme::c(0xa1a0a0));
    // The title opens Merge Modeling Steps (as observed, L16).
    let head = Rect::from_min_max(pos2(title.left() - 4.0, r.top() + 6.0), pos2(chevron.x + 8.0, r.top() + 31.0));
    if ui.interact(head, ui.scope_id().with("history-head"), Sense::click()).clicked() {
        ws.history.merge = if ws.history.merge.is_some() { None } else { Some((false, true, ws.now)) };
    }
    if ws.history.merge.is_some() {
        merge_popover(ws, ui, pos2(chevron.x, r.top() + 31.0));
    }
    icons::paint(&p, Rect::from_center_size(pos2(r.right() - 22.0, r.top() + 18.0), vec2(15.0, 15.0)), Icon::Filter, Tokens::text_dim());
    // Steps set aside while a sketch is edited in the past come after a marker, greyed out; steps held back by a
    // breakpoint come after its row, dimmed.
    let past = ws.session.doc.history.len();
    let at_bp = ws.session.at_breakpoint();
    let all_count = past + ws.session.doc.future.len();
    let bp_index = at_bp.then_some(past);
    // How many steps (without the breakpoint) come before the one at `i`.
    let plain = |i: usize| i - usize::from(bp_index.is_some_and(|b| b < i));
    let mut gaps: Vec<(f32, usize)> = Vec::new();
    // In a sketch (26.202, s37): its step isn't listed until something is drawn in it; then every step is dimmed
    // and the sketch's own row is lit grey.
    let sketching = ws.session.active_sketch;
    let empty_sketch = sketching.filter(|&k| ws.session.doc.sketch(k).is_some_and(|sk| sk.entities.is_empty()));
    // A tool holding a result not yet applied (Cancel / Done showing) dims the filter and its steps (26.202, s37),
    // and so does a modeling tool the moment it's started, before any pull (s41-011: Extrude with only Cancel); a
    // tool merely predicted for the selection doesn't (s41-010).
    let held = ws.is_live() && !matches!(ws.tool, super::tools::Tool::Idle);
    let started = !matches!(ws.tool, super::tools::Tool::Idle | super::tools::Tool::PickPlane { .. })
        && !ws.tool.is_sketch_tool()
        && !ws.predicted
        && sketching.is_none();
    let busy = held || started;
    let live = held && past > 0;
    let steps: Vec<(usize, HistoryStep)> = ws
        .session
        .doc
        .history
        .iter()
        .chain(&ws.session.doc.future)
        .cloned()
        .enumerate()
        .filter(|(_, h)| !h.quiet && !(h.kind == "sketch" && empty_sketch.is_some() && h.sketch == empty_sketch))
        // A result waiting for Done isn't a step yet (26.202, s40-010, s40-014: Extrude, Fillet).
        .filter(|(i, _)| !(live && *i + 1 == past))
        .collect();
    let mut edit_sketch: Option<usize> = None;
    let mut marked = false;
    if steps.is_empty() {
        let c = r.center();
        p.text(c, Align2::CENTER_CENTER, crate::i18n::tr("Workspace is empty"), theme::strong(12.0), Tokens::text_strong());
        p.text(c + vec2(0.0, 19.0), Align2::CENTER_CENTER, "Your steps show up here as you model", theme::body(10.5), Tokens::text_dim());
        return;
    }
    let units = ws.session.doc.units;
    let mut y = r.top() + 62.0 - ROW_H / 2.0;
    let left = r.left() + 9.0;
    let right = r.right() - 9.0;
    let filter = related_filter(ws, busy);
    if filter.is_some() {
        // The filter's row: dark blue, a blue filter badge, the title, and Clear on the right.
        let bar = Rect::from_min_size(pos2(left, r.top() + 34.0), vec2(right - left, 26.0));
        p.rect_filled(bar, radius(6), theme::c(0x2e3957));
        let dot = pos2(bar.left() + 12.0, bar.center().y);
        let (badge, ink, link) = if busy {
            (theme::c(0x2e4276), theme::c(0x747c90), theme::c(0x3a4a78))
        } else {
            (theme::c(0x2f6cf6), Color32::WHITE, theme::c(0x3a7bf7))
        };
        p.circle_filled(dot, 6.5, badge);
        for (dy, half) in [(-2.2, 3.4), (0.0, 2.4), (2.2, 1.3)] {
            p.line_segment([dot + vec2(-half, dy), dot + vec2(half, dy)], Stroke::new(1.0, ink));
        }
        p.text(pos2(bar.left() + 25.0, bar.center().y), Align2::LEFT_CENTER, "Related to Selection", theme::strong(12.5), ink);
        let clear = p.text(pos2(bar.right() - 6.0, bar.center().y), Align2::RIGHT_CENTER, "Clear", theme::body(12.5), link);
        if !busy && ui.interact(clear.expand(4.0), ui.scope_id().with("history-clear"), Sense::click()).clicked() {
            ws.history.cleared = Some(ws.session.selection.clone());
        }
        y += 40.0;
    }
    let mut hidden = 0usize;
    // A run of steps the filter hides shows as its count, small and dim, where it was.
    let hidden_run = |n: &mut usize, y: &mut f32| {
        if *n > 0 {
            p.text(pos2(left + 5.0, *y + 6.0), Align2::LEFT_CENTER, format!("({n})"), theme::body(10.0), Tokens::text_faint());
            *y += 14.0;
            *n = 0;
        }
    };
    let mut commit: Option<(usize, &'static str, serde_json::Value)> = None;
    let mut fix: Option<usize> = None;
    let mut rename: Option<(usize, String)> = None;
    let mut drawn = 0usize;
    for (i, step) in &steps {
        let i = *i;
        if filter.as_ref().is_some_and(|f| !f.get(i).copied().unwrap_or(false)) {
            hidden += 1;
            continue;
        }
        hidden_run(&mut hidden, &mut y);
        if y > r.bottom() - ROW_H {
            break;
        }
        let future = i >= past;
        if step.kind == BREAKPOINT {
            let row = Rect::from_min_size(pos2(left, y + 2.0), vec2(right - left, BP_H));
            breakpoint_row(ws, ui, &p, row, i);
            drawn = 0;
            y += BP_H + 6.0;
            continue;
        }
        gaps.push((y - 1.0, plain(i)));
        // While a sketch is edited in the past the steps after it can't be touched.
        let locked = future && !at_bp;
        if locked && !marked {
            marked = true;
            p.line_segment([pos2(left, y + 1.0), pos2(right, y + 1.0)], Stroke::new(2.0, Tokens::accent()));
            y += 6.0;
        }
        let row = Rect::from_min_size(pos2(left, y), vec2(right - left, ROW_H));
        // A thin line between rows (measured: #3e3d3e, 16 px in from the left, 14 from the right).
        if drawn > 0 {
            p.line_segment(
                [pos2(r.left() + 16.0, row.top() - 1.5), pos2(r.right() - 14.0, row.top() - 1.5)],
                Stroke::new(1.0, theme::c(0x3e3d3e)),
            );
        }
        drawn += 1;
        let open = ws.history.open == Some(i) && !future;
        let lit = sketching.is_some() && step.kind == "sketch" && step.sketch == sketching;
        let dim = future || step.suppressed || busy && filter.is_some() || sketching.is_some();
        let text_color = if lit {
            theme::c(0x919094)
        } else if dim {
            Tokens::text_faint()
        } else {
            Tokens::text_strong()
        };
        let err_h = if open && step.error.is_some() { ERROR_H + 8.0 } else { 0.0 };
        let feature = history::feature(&step.kind);
        let fields = feature.map_or(&[][..], |f| f.fields);
        // 26.200 opens a feature step's settings in the card at the bottom left (step_card.rs), not inline.
        let carded = feature.is_some();
        let extra = if step.kind == "extrude" { 5 } else { 0 };
        let body_rows = if open && !carded { fields.len() + extra + usize::from(step.kind == "sketch") } else { 0 };
        let block = Rect::from_min_size(
            row.min,
            vec2(row.width(), ROW_H + err_h + if body_rows > 0 { body_rows as f32 * FIELD_PITCH + 18.0 } else { 0.0 }),
        );
        let resp = ui.interact(row, ui.scope_id().with(("history-row", i)), Sense::click());
        let hovered = (resp.hovered() || ui.rect_contains_pointer(row)) && !locked && !ws.history.bp_drag;
        if open && carded {
            // The step whose card is up is lit blue (s16-010).
            p.rect_filled(row, radius(6), Tokens::accent());
        } else if open {
            p.rect_filled(block, radius(6), theme::c(0x2b2b2d));
            p.rect_filled(row, radius(6), theme::c(0x37373a));
        } else if lit {
            p.rect_filled(row, radius(6), theme::c(0x58575d));
        } else if hovered {
            p.rect_filled(row, radius(6), Tokens::row_hover());
        }
        icons::paint(
            &p,
            Rect::from_center_size(pos2(r.left() + 27.5, row.center().y), vec2(19.0, 19.0)),
            icon_for(step),
            if dim { Tokens::text_faint() } else { Tokens::text() },
        );
        let renaming = matches!(&ws.history.renaming, Some((s, ..)) if *s == i);
        if renaming {
            // Renamed in place (s14-012): a text field with the name selected; Return keeps it, Escape doesn't.
            if let Some((_, text, focus)) = &mut ws.history.renaming {
                let field = Rect::from_min_max(pos2(r.left() + 42.0, row.center().y - 11.0), pos2(r.right() - 34.0, row.center().y + 11.0));
                let id = ui.scope_id().with(("history-rename", i));
                let resp = ui.put(field, egui::TextEdit::singleline(text).id(id).font(theme::strong(13.0)).margin(vec2(6.0, 2.0)));
                p.rect_stroke(field.expand(1.5), radius(6), Stroke::new(2.0, Tokens::accent()), StrokeKind::Outside);
                if *focus {
                    resp.request_focus();
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(text.chars().count()),
                        )));
                        state.store(ui.ctx(), id);
                    }
                    *focus = false;
                }
                let (enter, esc) = ui.input(|inp| (inp.key_pressed(egui::Key::Enter), inp.key_pressed(egui::Key::Escape)));
                if esc {
                    ws.history.renaming = None;
                } else if enter || resp.lost_focus() {
                    rename = Some((i, text.clone()));
                }
            }
        } else {
            // Room for the error badge and the row's ⋯ button; longer names end in "…".
            let x = r.left() + 47.0;
            let room = row.right() - 40.0 - if step.error.is_some() { 22.0 } else { 0.0 } - x;
            let shown = theme::elide(&p, &step.label, theme::strong(13.0), room);
            let name = p.text(pos2(x, row.center().y), Align2::LEFT_CENTER, shown, theme::strong(13.0), text_color);
            if step.error.is_some() {
                icons::paint(
                    &p,
                    Rect::from_center_size(pos2(name.right() + 11.0, row.center().y - 0.5), vec2(13.0, 13.0)),
                    Icon::Warning,
                    Color32::WHITE,
                );
            }
        }
        let chevron = Rect::from_center_size(pos2(r.right() - 23.0, row.center().y), vec2(10.0, 10.0));
        // A merged step has nothing to open (as observed, L17).
        // 26.200's rows have no chevrons (s16-009); only a sketch's inline Curves count is left behind it.
        if step.kind == "sketch" && open {
            icons::paint(&p, chevron, if open { Icon::ChevronDown } else { Icon::ChevronRight }, Tokens::text_dim());
        }
        let dots = pos2(r.right() - 46.0, row.center().y);
        let menu_open = ws.history.menu.is_some_and(|(s, _)| s == i);
        if hovered || open || menu_open {
            p.circle_filled(dots, 6.5, if menu_open { Tokens::text() } else { Tokens::text_dim() });
            for dx in [-3.0, 0.0, 3.0] {
                p.circle_filled(dots + vec2(dx, 0.0), 1.0, Tokens::panel());
            }
        }
        let editable = !step.entries.is_empty() && !future && step.kind != "merged";
        let on_dots = ui.input(|inp| inp.pointer.interact_pos()).is_some_and(|p| p.distance(dots) < 10.0);
        let on_chevron = ui.input(|inp| inp.pointer.interact_pos()).is_some_and(|p| p.x > chevron.left() - 8.0);
        if !renaming && resp.clicked() && on_dots && !locked {
            ws.history.menu = if menu_open { None } else { Some((i, dots + vec2(-7.0, 10.0))) };
            ws.popup_opened_at = ws.now;
        } else if resp.clicked() && editable {
            if step.kind == "sketch" && !on_chevron {
                edit_sketch = Some(i);
            } else {
                ws.history.open = if open { None } else { Some(i) };
                ws.history.editing = None;
            }
        }
        y += ROW_H;
        // An imported Shapr3D project's own steps, under its Import step: listed in order, dim (they came in as the
        // model they made; only steps redone as ours, after it, can be edited).
        if !step.shapr_steps.is_empty() && y <= r.bottom() - ROW_H {
            let listed = ws.history.shapr_open == Some(i);
            let sub = Rect::from_min_size(pos2(left + 22.0, y), vec2(right - left - 22.0, 24.0));
            let sr = ui.interact(sub, ui.scope_id().with(("history-shapr", i)), Sense::click());
            if sr.hovered() {
                p.rect_filled(sub, radius(5), Tokens::row_hover());
            }
            icons::paint(
                &p,
                Rect::from_center_size(pos2(sub.left() + 10.0, sub.center().y), vec2(9.0, 9.0)),
                if listed { Icon::ChevronDown } else { Icon::ChevronRight },
                Tokens::text_dim(),
            );
            let n = step.shapr_steps.len();
            let caption = format!("{n} step{} from the .shapr file", if n == 1 { "" } else { "s" });
            p.text(pos2(sub.left() + 22.0, sub.center().y), Align2::LEFT_CENTER, caption, theme::body(12.0), Tokens::text_dim());
            if sr.clicked() {
                ws.history.shapr_open = if listed { None } else { Some(i) };
            }
            y += 26.0;
            if listed {
                for [name, op] in &step.shapr_steps {
                    if y > r.bottom() - 22.0 {
                        break;
                    }
                    let kind = shapr_kind(op);
                    let probe = HistoryStep { kind: kind.to_string(), ..Default::default() };
                    icons::paint(
                        &p,
                        Rect::from_center_size(pos2(left + 44.0, y + 11.0), vec2(14.0, 14.0)),
                        icon_for(&probe),
                        Tokens::text_faint(),
                    );
                    let shown = theme::elide(&p, &name.to_string(), theme::body(12.0), r.right() - 12.0 - (left + 58.0));
                    p.text(pos2(left + 58.0, y + 11.0), Align2::LEFT_CENTER, shown, theme::body(12.0), Tokens::text_dim());
                    y += 22.0;
                }
            }
        }
        if open {
            let lx = r.left() + 25.0;
            let rx = r.right() - 27.0;
            if let Some(err) = &step.error {
                // As observed (s14-010): a red panel over the parameters; the reason shows when hovered.
                let panel = Rect::from_min_size(pos2(left + 4.0, y + 6.0), vec2(right - left - 8.0, ERROR_H));
                p.rect_filled(panel, radius(6), error_panel());
                icons::paint(
                    &p,
                    Rect::from_center_size(pos2(panel.left() + 15.0, panel.center().y), vec2(14.0, 14.0)),
                    Icon::Warning,
                    Color32::WHITE,
                );
                // "Fix…" (26.200, the manual's broken-dependency steps): pick the step's references again.
                let fix_at = Rect::from_min_size(pos2(panel.right() - 50.0, panel.center().y - 10.0), vec2(42.0, 20.0));
                let text = p.layout(
                    "Something went wrong. Try editing or recreating this step.".to_string(),
                    theme::body(12.0),
                    Tokens::text_strong(),
                    panel.width() - 40.0 - fix_at.width() - 8.0,
                );
                p.galley(pos2(panel.left() + 30.0, panel.center().y - text.size().y / 2.0), text, Tokens::text_strong());
                ui.interact(panel, ui.scope_id().with(("history-error", i)), Sense::hover()).on_hover_text(err);
                let fr = ui.interact(fix_at, ui.scope_id().with(("history-fix", i)), Sense::click());
                p.rect_filled(fix_at, radius(5), if fr.hovered() { Color32::from_white_alpha(70) } else { Color32::from_white_alpha(45) });
                p.text(fix_at.center(), Align2::CENTER_CENTER, "Fix…", theme::body(12.0), Color32::WHITE);
                if fr.clicked() {
                    fix = Some(i);
                }
                y += err_h;
            }
            if carded {
                y = block.bottom();
                y += ROW_PITCH - ROW_H;
                continue;
            }
            let mut fy = y + 12.0 + FIELD_PITCH / 2.0;
            let label = |text: &str, fy: f32| {
                p.text(pos2(lx, fy), Align2::LEFT_CENTER, text, theme::body(12.5), Tokens::text());
            };
            if step.kind == "sketch" {
                let curves = step.sketch.and_then(|id| ws.session.doc.sketch(id)).map_or(0, |s| s.entities.len());
                label("Curves", fy);
                p.text(pos2(rx, fy), Align2::RIGHT_CENTER, curves.to_string(), theme::body(11.5), Tokens::text_dim());
                fy += FIELD_PITCH;
            }
            if step.kind == "extrude" {
                p.text(pos2(lx, fy - 6.0), Align2::LEFT_CENTER, "Profile", theme::body(12.5), Tokens::text());
                let n = history::profile_count(step).unwrap_or(1);
                p.text(
                    pos2(lx, fy + 8.0),
                    Align2::LEFT_CENTER,
                    format!("{n} {}", if n == 1 { "face" } else { "faces" }),
                    theme::body(9.5),
                    Tokens::text_dim(),
                );
                let b = Rect::from_min_max(pos2(rx - 54.0, fy - 10.0), pos2(rx, fy + 10.0));
                p.rect_filled(b, radius(5), pill());
                p.text(b.center(), Align2::CENTER_CENTER, "Edit…", theme::body(12.0), Tokens::text_strong());
                fy += FIELD_PITCH;
                for (text, value) in [("Sides", "One-Sided"), ("Extent", "Distance")] {
                    label(text, fy);
                    choice(&p, rx, fy, value);
                    fy += FIELD_PITCH;
                }
            }
            for field in fields {
                label(field.label, fy);
                let value = history::value(step, field, &ws.session.doc.variables);
                let expr = history::expression(step, field).map(str::to_string);
                let f = Rect::from_min_max(pos2(rx - 93.0, fy - 9.5), pos2(rx, fy + 9.5));
                let edit_here = matches!(&ws.history.editing, Some((s, k, ..)) if *s == i && *k == field.key);
                if edit_here {
                    if let Some((_, _, text, focus)) = &mut ws.history.editing {
                        let edit_id = ui.scope_id().with(("history-field", i, field.key));
                        let resp = ui.put(f, egui::TextEdit::singleline(text).id(edit_id).font(theme::body(11.5)).margin(vec2(6.0, 2.0)));
                        if *focus {
                            resp.request_focus();
                            *focus = false;
                        }
                        let (enter, esc) = ui.input(|inp| (inp.key_pressed(egui::Key::Enter), inp.key_pressed(egui::Key::Escape)));
                        if esc {
                            ws.history.editing = None;
                        } else if enter || resp.lost_focus() {
                            match parse_value(field.kind, text, units, &ws.session.doc.variables) {
                                Some(v) => commit = Some((i, field.key, v)),
                                None => ws.history.editing = None,
                            }
                        }
                    }
                } else {
                    let fr = ui.interact(f, ui.scope_id().with(("history-field-btn", i, field.key)), Sense::click());
                    p.rect_filled(f, radius(4), Tokens::field());
                    p.rect_stroke(
                        f,
                        radius(4),
                        Stroke::new(1.0, if fr.hovered() { Tokens::text_faint() } else { Tokens::chip_border() }),
                        StrokeKind::Inside,
                    );
                    // A field following a variable shows its expression in blue.
                    let (shown, color) = match &expr {
                        Some(e) => (e.clone(), Tokens::icon_blue()),
                        None => (show_value(field.kind, value, units), Tokens::text_strong()),
                    };
                    p.text(pos2(f.left() + 7.0, fy), Align2::LEFT_CENTER, &shown, theme::body(12.0), color);
                    if fr.hovered() && expr.is_some() {
                        p.text(
                            pos2(f.left() - 6.0, fy),
                            Align2::RIGHT_CENTER,
                            show_value(field.kind, value, units),
                            theme::body(10.5),
                            Tokens::text_dim(),
                        );
                    }
                    if fr.clicked() {
                        ws.history.editing = Some((i, field.key, shown, true));
                    }
                }
                fy += FIELD_PITCH;
            }
            if step.kind == "extrude" {
                for (text, value) in [("Start", "From Profile"), ("Result", extrude_result(step))] {
                    label(text, fy);
                    choice(&p, rx, fy, value);
                    fy += FIELD_PITCH;
                }
            }
            y = block.bottom();
        }
        y += ROW_PITCH - ROW_H;
    }
    gaps.push((y - 1.0, plain(all_count)));
    if ws.history.bp_drag {
        drag_breakpoint(ws, ui, &p, &gaps, left, right, plain(all_count));
    }
    // The ⋯ menu, as observed (s14-004).
    if let Some((step, at)) = ws.history.menu {
        let all = ws.session.all_steps();
        let suppressed = all.get(step).is_some_and(|h| h.suppressed);
        let label = all.get(step).map(|h| h.label.clone()).unwrap_or_default();
        let rows = [
            ("Insert Breakpoint", Some(Icon::Breakpoint)),
            ("", None),
            (if suppressed { "Unsuppress" } else { "Suppress" }, Some(if suppressed { Icon::Unsuppress } else { Icon::Suppress })),
            ("Rename", Some(Icon::Pencil)),
            ("Duplicate", Some(Icon::Pattern)),
            ("Zoom To", Some(Icon::Search)),
            ("", None),
            ("Delete", Some(Icon::Delete)),
        ];
        let (picked, rect) = super::popover::native_menu(ui, "history-step-menu", at, 180.0, ui.ctx().content_rect(), &rows);
        let outside =
            !ws.just_opened() && ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
        if picked.is_some() || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ws.history.menu = None;
        }
        match picked {
            Some(0) => {
                let _ = ws.run("set_breakpoint", json!({ "before": plain(step) + 1 }));
            }
            Some(2) => {
                let _ = ws.run("suppress_step", json!({ "step": step }));
            }
            Some(3) => ws.history.renaming = Some((step, label, true)),
            Some(4) => duplicate_step(ws, step),
            Some(5) => {
                if let Some((min, max)) = step_bounds(ws, step) {
                    let mut c = ws.camera;
                    c.fit(min, max);
                    ws.animate_to(c);
                }
            }
            Some(7) => {
                ws.history.open = None;
                let _ = ws.run("delete_step", json!({ "step": step }));
            }
            _ => {}
        }
    }
    if let Some((step, name)) = rename {
        ws.history.renaming = None;
        let _ = ws.run("rename_step", json!({ "step": step, "name": name }));
    }
    if let Some(step) = edit_sketch {
        ws.edit_sketch_step(step);
    }
    if let Some(step) = fix {
        super::step_card::fix(ws, step);
    }
    if let Some((step, key, value)) = commit {
        ws.history.editing = None;
        let mut params = serde_json::Map::new();
        params.insert(key.to_string(), value);
        let _ = ws.run("edit_step", json!({ "step": step, "params": params }));
    }
}

/// The breakpoint's row (s14-005): grey, a grip in the middle to drag it by, ⓧ on the right to take it away.
fn breakpoint_row(ws: &mut Workspace, ui: &mut Ui, p: &egui::Painter, row: Rect, index: usize) {
    p.rect_filled(row, radius(6), bp_row());
    let c = row.center();
    p.line_segment([c - vec2(15.0, 0.0), c + vec2(15.0, 0.0)], Stroke::new(1.5, bp_grip()));
    let x = pos2(row.right() - 14.0, c.y);
    p.circle_filled(x, 5.5, bp_grip());
    for d in [vec2(2.2, 2.2), vec2(2.2, -2.2)] {
        p.line_segment([x - d, x + d], Stroke::new(1.3, bp_row()));
    }
    let resp = ui.interact(row, ui.scope_id().with(("history-bp", index)), Sense::click_and_drag());
    let on_x = ui.input(|i| i.pointer.interact_pos()).is_some_and(|q| q.distance(x) < 9.0);
    if resp.clicked() && on_x {
        let _ = ws.run("delete_step", json!({ "step": index }));
    } else if resp.drag_started() {
        ws.history.bp_drag = true;
        ws.history.menu = None;
    }
}

/// Dragging the breakpoint (s14-006): a see-through copy follows the pointer and a blue line shows where it lands;
/// dropping it after the last step takes it away (s14-007).
fn drag_breakpoint(ws: &mut Workspace, ui: &mut Ui, p: &egui::Painter, gaps: &[(f32, usize)], left: f32, right: f32, total: usize) {
    let Some(at) = ui.input(|i| i.pointer.interact_pos()) else { return };
    let Some(&(gy, before)) = gaps.iter().min_by(|a, b| (a.0 - at.y).abs().total_cmp(&(b.0 - at.y).abs())) else { return };
    p.line_segment([pos2(left, gy), pos2(right, gy)], Stroke::new(2.0, Tokens::accent()));
    let ghost = Rect::from_center_size(pos2((left + right) / 2.0, at.y), vec2(right - left, BP_H));
    p.rect_filled(ghost, radius(6), bp_row().gamma_multiply(0.7));
    p.line_segment([ghost.center() - vec2(15.0, 0.0), ghost.center() + vec2(15.0, 0.0)], Stroke::new(1.5, bp_grip()));
    ui.ctx().request_repaint();
    if !ui.input(|i| i.pointer.primary_down()) {
        ws.history.bp_drag = false;
        let params = if before >= total { json!({}) } else { json!({ "before": before }) };
        let _ = ws.run("set_breakpoint", params);
    }
}

/// Merge Modeling Steps (observed L16–L17): what it does, Delete/Keep Sketches, Delete/Keep Variables, Merge now.
fn merge_popover(ws: &mut Workspace, ui: &mut Ui, below: egui::Pos2) {
    use super::popover::{Place, Row};
    let Some((keep_sketches, keep_vars, opened)) = ws.history.merge else { return };
    let can = ws.session.doc.history.iter().any(|h| !h.quiet) && ws.session.doc.future.is_empty() && ws.session.active_sketch.is_none();
    let move_tip = theme::key_text("⌥⇧M");
    let rows = [
        Row::Title("Merge Modeling Steps"),
        Row::Paragraph(
            "Bakes every step so far into one. Sketches stop driving bodies, and the steps can't be edited again once the project is closed.",
            3,
        ),
        Row::Choice { label: "Delete Sketches", checked: !keep_sketches, enabled: true },
        Row::Choice { label: "Keep Sketches", checked: keep_sketches, enabled: true },
        Row::Separator,
        Row::Choice { label: "Delete Variables", checked: !keep_vars, enabled: true },
        Row::Choice { label: "Keep Variables", checked: keep_vars, enabled: true },
        Row::Button(if can { "Merge now" } else { "Nothing to merge" }),
        Row::Tip(&move_tip),
    ];
    let bounds = ui.ctx().content_rect();
    let (clicked, rect) = super::popover::show_at(ui, "merge-steps", Place::Below(below), bounds, &rows);
    match clicked.map(|c| c.0) {
        Some(2) => ws.history.merge = Some((false, keep_vars, opened)),
        Some(3) => ws.history.merge = Some((true, keep_vars, opened)),
        Some(5) => ws.history.merge = Some((keep_sketches, false, opened)),
        Some(6) => ws.history.merge = Some((keep_sketches, true, opened)),
        Some(7) if can => {
            ws.history.merge = None;
            let _ = ws.run("merge_steps", serde_json::json!({ "keep_sketches": keep_sketches, "keep_variables": keep_vars }));
        }
        _ => {}
    }
    let outside = ws.now - opened > 0.15
        && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !rect.expand(4.0).contains(q)));
    if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.history.merge = None;
    }
}

/// Duplicate: the step's commands run again at the end of the history, as they were (a sketch's edits aren't
/// copied: they'd draw over themselves).
fn duplicate_step(ws: &mut Workspace, step: usize) {
    let Some(h) = ws.session.all_steps().get(step).cloned() else { return };
    if h.kind == "sketch" || h.entries.is_empty() {
        ws.toast("Sketches can't be duplicated: copy their curves instead");
        return;
    }
    for e in h.entries {
        ws.session.selection = e.selection.clone();
        ws.session.active_sketch = e.active_sketch;
        if let Err(err) = ws.session.execute(&e.command, &e.params) {
            ws.toast(err.to_string());
            break;
        }
    }
}

/// What a step made or changed: the bodies it created (their ids fall between the step before's and its own), the
/// body or sketch its parameters name, and the sketch a sketch step edits.
fn step_bounds(ws: &Workspace, step: usize) -> Option<(glam::DVec3, glam::DVec3)> {
    let all = ws.session.all_steps();
    let h = all.get(step)?;
    let from = all[..step].iter().rev().map(|s| s.ids).find(|i| *i > 0).unwrap_or(0);
    let doc = &ws.session.doc;
    let mut lo = glam::DVec3::splat(f64::INFINITY);
    let mut hi = glam::DVec3::splat(f64::NEG_INFINITY);
    let mut add = |q: glam::DVec3| {
        lo = lo.min(q);
        hi = hi.max(q);
    };
    let named: Vec<u64> = h
        .entries
        .iter()
        .flat_map(|e| {
            ["body", "target", "sketch"].iter().filter_map(|k| e.params.get(*k).and_then(serde_json::Value::as_u64)).collect::<Vec<_>>()
        })
        .collect();
    for b in &doc.bodies {
        if (b.id.0 >= from && b.id.0 < h.ids) || named.contains(&b.id.0) {
            for p in &b.mesh.positions {
                add(glam::DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            }
        }
    }
    for sk in &doc.sketches {
        if h.sketch == Some(sk.id) || named.contains(&sk.id.0) || (sk.id.0 >= from && sk.id.0 < h.ids) {
            for e in &sk.entities {
                for q in e.outline() {
                    add(sk.plane.to_world(q));
                }
            }
        }
    }
    lo.x.is_finite().then_some((lo, hi))
}

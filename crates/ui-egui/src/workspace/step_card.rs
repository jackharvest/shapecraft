//! The Parameter Card of 26.200 (observed s16-005..011, s29-011..027): a frosted card over the foot of the Items
//! sidebar with the feature's settings. The running Extrude or Fillet opens it from its ≡; clicking a History step
//! opens the same card for that step ("Extrude 01"), and a change there rebuilds the model. Its ⋯ menu says when it
//! opens by itself (Show on Tool Start / Show on History Edit); dragging its title moves it, and the spot is kept.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};
use shapecraft_doc::format_length;
use shapecraft_engine::history::{self, FieldKind};

use super::Workspace;
use super::tools::{ExtrudeTool, FilletTool, Tool, parse_length};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};

/// Which pop-up is open and which number is being typed.
#[derive(Default)]
pub struct CardUi {
    popup: Option<&'static str>,
    editing: Option<(&'static str, String, bool)>,
    /// A number being scrubbed: its key, the value when the drag began, and how far it has gone (points).
    scrub: Option<(&'static str, f64, f32)>,
    /// The ⋯ menu is open.
    more: bool,
    /// The card's title is being dragged: where the card's corner was and how far the pointer has gone.
    drag: Option<(egui::Pos2, egui::Vec2)>,
    /// The tool the card last saw, so Show on Tool Start opens it once per tool.
    seen_tool: Option<&'static str>,
}

/// Measured on s16-005 (full-size points).
const CARD_W: f32 = 320.0;
const ROW: f32 = 42.0;
// Frosted: the model shows through a little (s29-014).
fn card_bg() -> Color32 {
    theme::c(0x2a2727).gamma_multiply(0.94)
}
fn inner_bg() -> Color32 {
    theme::c(0x2d2a2a).gamma_multiply(0.94)
}
fn rule() -> Color32 {
    theme::c(0x413f3f)
}
fn pill_grey() -> Color32 {
    theme::c(0x555559)
}
fn seg_bg() -> Color32 {
    theme::c(0x3a3838)
}
fn seg_on() -> Color32 {
    theme::c(0x6a6868)
}
fn link_blue() -> Color32 {
    theme::c(0x5b7cf9)
}

/// What the card shows: the running tool's settings, or a History step's.
enum Source {
    Tool(Box<ExtrudeTool>),
    Fillet(Box<FilletTool>),
    /// Split Body (s29-027): what's split, what it's split with, Keep Originals.
    Split {
        items: usize,
        with: usize,
        keep: bool,
    },
    Step(usize),
}

/// One setting of the card.
enum Row {
    /// What the feature was made from: label, what one of them is called, how many.
    Picks(&'static str, &'static str, usize),
    /// Label, key, on.
    Switch(&'static str, &'static str, bool),
    /// Label, sub-label, icons, the lit one, the parameter values they stand for.
    Segments(&'static str, &'static str, &'static [Icon], usize, &'static str, &'static [&'static str]),
    /// Label, key, current value, (shown name, value, enabled).
    Choice(&'static str, &'static str, &'static str, &'static [(&'static str, &'static str, bool)]),
    Number(&'static str, &'static str, FieldKind, f64, Option<String>),
}

const EXTENTS: &[(&str, &str, bool)] =
    &[("Distance", "distance", true), ("To Object", "to_object", true), ("Through All", "through_all", true)];
const STARTS: &[(&str, &str, bool)] = &[("From Profile", "profile", true), ("Offset", "offset", true), ("From Plane", "plane", true)];
const SIDES: &[&str] = &["one", "symmetric"];
const RESULTS: &[&str] = &["new", "union", "subtract", "intersect"];
const ORIENTATIONS: &[(&str, &str, bool)] = &[("Normal to Path", "normal", true), ("Parallel to Profile", "parallel", true)];
const CORNERS: &[(&str, &str, bool)] = &[("Mitre", "mitre", true), ("Round", "round", true)];
const SIZE_TYPES: &[&str] = &["radius", "chordal"];
const CORNER_TYPES: &[&str] = &["rolling_ball", "setback"];
const CONTINUITIES: &[(&str, &str, bool)] = &[("G1 - Tangent", "g1", true), ("G2 - Curvature", "g2", true)];
// Only Auto is built; the others are listed as the reference lists them (s29-012).
const OVERFLOWS: &[(&str, &str, bool)] =
    &[("Auto", "auto", true), ("Cliff", "cliff", false), ("Smooth", "smooth", false), ("Notch", "notch", false)];

/// The fillet settings, whichever way they're held.
struct Fillet {
    edges: usize,
    chordal: bool,
    radius: f64,
    expr: Option<String>,
    g2: bool,
    setback: bool,
    curvature: f64,
}

/// The Fillet card (s29-011): Edge, Type, Radius, Continuity, Corner, Overflow, Curvature, Include Tangent Edges,
/// Y-Shaped Blend.
fn fillet_rows(f: &Fillet) -> Vec<Row> {
    vec![
        Row::Picks("Edge", "edge", f.edges),
        Row::Segments(
            "Type",
            if f.chordal { "Chordal" } else { "Radius" },
            &[Icon::FilletRadius, Icon::FilletChord],
            usize::from(f.chordal),
            "mode",
            SIZE_TYPES,
        ),
        Row::Number("Radius", "radius", FieldKind::Length, f.radius, f.expr.clone()),
        Row::Choice("Continuity", "continuity", if f.g2 { "G2 - Curvature" } else { "G1 - Tangent" }, CONTINUITIES),
        Row::Segments(
            "Corner",
            if f.setback { "Setback" } else { "Rolling Ball" },
            &[Icon::CornerRolling, Icon::CornerSetback],
            usize::from(f.setback),
            "corner",
            CORNER_TYPES,
        ),
        Row::Choice("Overflow", "overflow", "Auto", OVERFLOWS),
        Row::Number("Curvature", "curvature", FieldKind::Factor, f.curvature, None),
        // OCCT's fillets always run on along tangent edges; no Y-shaped blends.
        Row::Switch("Include Tangent Edges", "tangent_edges", true),
        Row::Switch("Y-Shaped Blend", "y_blend", false),
    ]
}

fn tool_fillet(t: &FilletTool) -> Fillet {
    Fillet {
        edges: t.edges.len().max(1),
        chordal: t.chordal,
        radius: t.radius,
        expr: None,
        g2: t.g2,
        setback: t.setback,
        curvature: t.curvature,
    }
}

fn step_fillet(ws: &Workspace, i: usize) -> Option<Fillet> {
    let all = ws.session.all_steps();
    let step = all.get(i)?;
    let e = step.entries.first()?;
    let get = |k: &str| e.params.get(k).and_then(Value::as_str);
    let fields = history::feature("fillet")?.fields;
    Some(Fillet {
        edges: e.params.get("edges").and_then(Value::as_array).map_or(0, Vec::len),
        chordal: matches!(get("mode").or(get("size_type")), Some("chordal" | "chord_width")),
        radius: history::value(step, &fields[0], &ws.session.doc.variables),
        expr: history::expression(step, &fields[0]).map(str::to_string),
        g2: get("continuity") == Some("g2"),
        setback: get("corner") == Some("setback") || e.params.get("setback").and_then(Value::as_bool) == Some(true),
        curvature: e.params.get("curvature").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

fn result_index(result: &str, auto_on_body: bool, cuts: bool) -> (usize, bool) {
    match result {
        "new" => (0, false),
        "union" => (1, false),
        "subtract" => (2, false),
        "intersect" => (3, false),
        _ if !auto_on_body => (0, true),
        _ => (if cuts { 2 } else { 1 }, true),
    }
}

/// The extrude settings, whichever way they're held.
struct Extrude {
    profile: usize,
    symmetric: bool,
    through_all: bool,
    to_object: bool,
    from_plane: bool,
    offset: Option<f64>,
    distance: f64,
    draft: f64,
    result: String,
    on_body: bool,
    exprs: [Option<String>; 2],
}

fn extrude_rows(e: &Extrude) -> (Vec<Row>, String) {
    let (lit, auto) = result_index(&e.result, e.on_body, e.distance < 0.0);
    let result_sub = format!("{}{}", ["New Body", "Union", "Subtract", "Intersect"][lit], if auto { " - Auto" } else { "" });
    let mut rows = vec![
        Row::Picks("Profile", "face", e.profile),
        Row::Segments(
            "Sides",
            if e.symmetric { "Symmetric" } else { "One-Sided" },
            &[Icon::Extrude, Icon::ExtrudeSymmetric],
            usize::from(e.symmetric),
            "sides",
            SIDES,
        ),
        Row::Choice(
            "Extent",
            "extent",
            if e.through_all {
                "Through All"
            } else if e.to_object {
                "To Object"
            } else {
                "Distance"
            },
            EXTENTS,
        ),
    ];
    if !e.through_all && !e.to_object {
        rows.push(Row::Number("Distance", "distance", FieldKind::Length, e.distance, e.exprs[0].clone()));
    }
    rows.push(Row::Number("Draft Angle", "draft", FieldKind::Angle, e.draft, e.exprs[1].clone()));
    rows.push(Row::Choice(
        "Start",
        "start",
        if e.from_plane {
            "From Plane"
        } else if e.offset.is_some() {
            "Offset"
        } else {
            "From Profile"
        },
        STARTS,
    ));
    if let Some(o) = e.offset {
        rows.push(Row::Number("Start Offset", "start_offset", FieldKind::Length, o, None));
    }
    rows.push(Row::Segments("Result", "", &[Icon::Body, Icon::Union, Icon::Subtract, Icon::Intersect], lit, "result", RESULTS));
    (rows, result_sub)
}

fn tool_extrude(ws: &Workspace, t: &ExtrudeTool) -> Extrude {
    let on_body = match t.target {
        super::tools::Target::Region { sketch, .. } => ws.session.doc.sketch(sketch).is_some_and(|s| s.on_body.is_some()),
        super::tools::Target::Face { .. } => true,
    };
    Extrude {
        profile: 1 + t.more.len(),
        symmetric: t.symmetric,
        through_all: t.through_all,
        to_object: t.to.is_some() || t.picking == Some("to"),
        from_plane: t.start_plane.is_some() || t.picking == Some("start"),
        offset: t.start_offset,
        distance: t.distance,
        draft: t.draft,
        result: t.result.to_string(),
        on_body,
        exprs: [None, None],
    }
}

fn step_extrude(ws: &Workspace, i: usize) -> Option<Extrude> {
    let all = ws.session.all_steps();
    let step = all.get(i)?;
    let e = step.entries.first()?;
    let get = |k: &str| e.params.get(k);
    let fields = history::feature("extrude")?.fields;
    let vars = &ws.session.doc.variables;
    let on_body = e.params.get("face").is_some()
        || e.params
            .get("sketch")
            .and_then(Value::as_u64)
            .and_then(|id| ws.session.doc.sketch(shapecraft_doc::ItemId(id)))
            .is_some_and(|s| s.on_body.is_some());
    Some(Extrude {
        profile: history::profile_count(step).unwrap_or(1),
        symmetric: get("sides").and_then(Value::as_str) == Some("symmetric"),
        through_all: get("extent").and_then(Value::as_str) == Some("through_all"),
        to_object: get("extent").and_then(Value::as_str) == Some("to_object"),
        from_plane: get("start_plane").is_some_and(|v| !v.is_null()),
        offset: get("start_offset").and_then(Value::as_f64),
        distance: history::value(step, &fields[0], vars),
        draft: history::value(step, &fields[1], vars),
        result: get("result").and_then(Value::as_str).unwrap_or("auto").to_string(),
        on_body,
        exprs: [history::expression(step, &fields[0]).map(str::to_string), history::expression(step, &fields[1]).map(str::to_string)],
    })
}

/// A plain feature step: its number fields.
fn step_rows(ws: &Workspace, i: usize) -> Option<(String, Vec<Row>, String)> {
    let all = ws.session.all_steps();
    let step = all.get(i)?;
    if step.kind == "extrude" {
        let (rows, sub) = extrude_rows(&step_extrude(ws, i)?);
        return Some((step.label.clone(), rows, sub));
    }
    if step.kind == "fillet" {
        return Some((step.label.clone(), fillet_rows(&step_fillet(ws, i)?), String::new()));
    }
    // An Unlinked Copy has nothing to set (s29-024).
    if step.kind == "unlinked_copy" {
        return None;
    }
    let feature = history::feature(&step.kind)?;
    let vars = &ws.session.doc.variables;
    let mut rows = Vec::new();
    if let Some(n) = history::profile_count(step) {
        rows.push(Row::Picks("Profile", "face", n));
    }
    for f in feature.fields {
        rows.push(Row::Number(f.label, f.key, f.kind, history::value(step, f, vars), history::expression(step, f).map(str::to_string)));
    }
    // Sweep's card (26.0): Orientation and Corner type beside Scale and Twist.
    if step.kind == "sweep" {
        let param = |key: &str, choices: &'static [(&'static str, &'static str, bool)]| {
            let set = step.entries.first().and_then(|e| e.params.get(key)).and_then(Value::as_str);
            choices.iter().find(|c| Some(c.1) == set).unwrap_or(&choices[0]).0
        };
        rows.push(Row::Choice("Orientation", "orientation", param("orientation", ORIENTATIONS), ORIENTATIONS));
        rows.push(Row::Choice("Corner Type", "corners", param("corners", CORNERS), CORNERS));
    }
    Some((step.label.clone(), rows, String::new()))
}

fn show_value(kind: FieldKind, v: f64, units: shapecraft_doc::Units) -> String {
    match kind {
        FieldKind::Length => format_length(v, units),
        FieldKind::Angle => format!("{}°", format_length(v, shapecraft_doc::Units::Millimeter).trim_end_matches(" mm")),
        FieldKind::Count => format!("{}", v.round() as i64),
        FieldKind::Factor => format_length(v, shapecraft_doc::Units::Millimeter).trim_end_matches(" mm").to_string(),
    }
}

fn parse(kind: FieldKind, text: &str, units: shapecraft_doc::Units) -> Option<f64> {
    let t = text.trim().trim_end_matches('°').trim_start_matches('×');
    match kind {
        FieldKind::Length => {
            let suffix = t.ends_with("mm") || t.ends_with("cm") || t.ends_with("in");
            parse_length(t).map(|v| if suffix { v } else { v * units.scale() })
        }
        FieldKind::Count => t.parse::<f64>().ok().filter(|v| *v >= 1.0).map(f64::round),
        _ => t.parse::<f64>().ok().filter(|v| v.is_finite()),
    }
}

/// Draws the card when there's one to show, and applies what was changed.
pub(super) fn show(ws: &mut Workspace, ui: &mut Ui) {
    // Picking a profile ends with its card.
    if ws.profile_edit.as_ref().is_some_and(|(s, ..)| Some(*s) != ws.history.open || !matches!(ws.tool, Tool::Idle)) {
        ws.profile_edit = None;
    }
    // Show on Tool Start: a card tool that has just started opens its card once.
    // (A tool a pick started counts: the reference lights it as running, s29-010.)
    let tool_name = match &ws.tool {
        Tool::Extrude(_) => Some("extrude"),
        Tool::Fillet(t) if !t.chamfer => Some("fillet"),
        _ => None,
    };
    if tool_name != ws.card.seen_tool {
        ws.card.seen_tool = tool_name;
        if tool_name.is_some() && ws.settings.card_on_tool_start && ws.options_menu.is_none() {
            ws.options_menu = Some(ws.viewport_rect.left_top());
        }
    }
    let source = match &ws.tool {
        Tool::Extrude(t) if ws.options_menu.is_some() => Source::Tool(Box::new(t.clone())),
        Tool::Fillet(t) if ws.options_menu.is_some() && !t.chamfer => Source::Fillet(Box::new(t.clone())),
        Tool::Mirror(t) if ws.options_menu.is_some() && t.split => Source::Split { items: t.items.len(), with: t.with.len(), keep: t.keep },
        Tool::Idle if ws.show_history && ws.drawings.is_none() && !ws.visualizing && ws.settings.card_on_history_edit => {
            match ws.history.open {
                Some(i) if step_rows(ws, i).is_some() => Source::Step(i),
                _ => return,
            }
        }
        _ => return,
    };
    let (title, rows, result_sub) = match &source {
        Source::Tool(t) => {
            let (rows, sub) = extrude_rows(&tool_extrude(ws, t));
            ("Extrude".to_string(), rows, sub)
        }
        Source::Fillet(t) => ("Fillet".to_string(), fillet_rows(&tool_fillet(t)), String::new()),
        Source::Split { items, with, keep } => (
            "Split Body".to_string(),
            vec![
                Row::Picks("To Split", "body", *items),
                Row::Picks("Split With", "item", *with),
                Row::Switch("Keep Originals", "keep", *keep),
            ],
            String::new(),
        ),
        Source::Step(i) => match step_rows(ws, *i) {
            Some(r) => r,
            None => return,
        },
    };
    let units = ws.session.doc.units;
    let height = 42.0 + 10.0 + rows.iter().map(|r| if matches!(r, Row::Picks(..)) { 62.0 } else { ROW }).sum::<f32>();
    let bounds = ui.ctx().content_rect();
    // Where it was dragged last (kept on screen), else over the foot of the Items sidebar.
    let home = pos2(bounds.left() + 4.0, bounds.bottom() - 5.0 - height);
    let mut corner = ws.settings.card_at.map_or(home, |[x, y]| pos2(x, y));
    if let Some((from, moved)) = ws.card.drag {
        corner = from + moved;
    }
    let corner = pos2(
        corner.x.clamp(bounds.left(), (bounds.right() - CARD_W).max(bounds.left())),
        corner.y.clamp(bounds.top(), (bounds.bottom() - height).max(bounds.top())),
    );
    let card = Rect::from_min_size(corner, vec2(CARD_W, height));
    let mut change: Option<(&'static str, Value)> = None;
    let mut pick_hint: Option<&'static str> = None;
    let mut open_popup: Option<(&'static str, egui::Pos2)> = None;
    let mut profile_click: Option<usize> = None;
    egui::Area::new(ui.scope_id().with("step-card")).fixed_pos(card.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(card.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(rect, radius(10), card_bg());
        p.rect_stroke(rect, radius(10), Stroke::new(1.0, theme::c(0x6f6d6d)), StrokeKind::Inside);
        let head = pos2(rect.left() + 16.0, rect.top() + 19.0);
        p.text(head, Align2::LEFT_CENTER, tr(&title), theme::strong(13.0), Tokens::text_strong());
        let more = pos2(rect.right() - 22.0, head.y);
        // The title row drags the card; the ⋯ ring opens when it opens by itself.
        let title_bar = Rect::from_min_max(rect.min, pos2(more.x - 12.0, rect.top() + 37.0));
        let grip = ui.interact(title_bar, ui.scope_id().with("card-title"), Sense::drag());
        if grip.drag_started() {
            ws.card.drag = Some((corner, vec2(0.0, 0.0)));
        }
        if let Some((_, moved)) = &mut ws.card.drag {
            *moved += grip.drag_delta();
            if grip.drag_stopped() {
                ws.settings.card_at = Some([corner.x, corner.y]);
                ws.card.drag = None;
            }
        }
        if grip.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if ui.interact(Rect::from_center_size(more, vec2(20.0, 20.0)), ui.scope_id().with("card-more"), Sense::click()).clicked() {
            ws.card.more = !ws.card.more;
            ws.popup_opened_at = ws.now;
        }
        p.circle_stroke(more, 6.0, Stroke::new(1.2, link_blue()));
        for dx in [-2.6, 0.0, 2.6] {
            p.circle_filled(more + vec2(dx, 0.0), 0.9, link_blue());
        }
        p.hline(rect.x_range(), rect.top() + 37.0, Stroke::new(1.0, theme::c(0x4d4a4a)));
        let inner = Rect::from_min_max(pos2(rect.left() + 5.0, rect.top() + 42.0), pos2(rect.right() - 5.0, rect.bottom() - 5.0));
        p.rect_filled(inner, radius(7), inner_bg());
        let lx = inner.left() + 11.0;
        let rx = inner.right() - 11.0;
        let mut y = inner.top();
        for (n, row) in rows.iter().enumerate() {
            let h = if matches!(row, Row::Picks(..)) { 62.0 } else { ROW };
            let cy = y + ROW / 2.0;
            if n > 0 {
                p.hline(lx..=rx, y, Stroke::new(1.0, rule()));
            }
            match row {
                Row::Picks(label, noun, count) => {
                    p.text(pos2(lx, y + 18.0), Align2::LEFT_CENTER, tr(label), theme::body(13.0), Tokens::text());
                    // Edit... picks the step's regions again in the view; Done applies them.
                    let editing = matches!((&source, ws.profile_edit.as_ref()), (Source::Step(i), Some((s, ..))) if s == i);
                    let link = Rect::from_min_max(pos2(rx - 50.0, y + 8.0), pos2(rx, y + 28.0));
                    // Split Body's rows are only their chips (s29-027).
                    let editable = !matches!(source, Source::Split { .. });
                    if editable {
                        p.text(
                            pos2(rx, y + 18.0),
                            Align2::RIGHT_CENTER,
                            tr(if editing { "Done" } else { "Edit..." }),
                            theme::body(12.5),
                            link_blue(),
                        );
                    }
                    if editable && ui.interact(link, ui.scope_id().with(("profile-edit", n)), Sense::click()).clicked() {
                        match (&source, *label) {
                            (Source::Step(i), "Profile") => profile_click = Some(*i),
                            (Source::Step(_), _) => pick_hint = Some("Pick the edges again: open the step's body and fillet them anew"),
                            (Source::Split { .. }, _) => {
                                pick_hint = Some("Click bodies, faces, planes or sketches in the view to change them")
                            }
                            _ => pick_hint = Some("Click edges in the view to add or remove them"),
                        }
                    }
                    let count = if editing { ws.profile_edit.as_ref().map_or(*count, |(_, _, r)| r.len()) } else { *count };
                    let g = p.layout_no_wrap(
                        format!("{count} {noun}{}", if count == 1 { "" } else { "s" }),
                        theme::strong(10.5),
                        Tokens::text_strong(),
                    );
                    let chip = Rect::from_min_size(pos2(lx, y + 39.0), vec2(g.size().x + 27.0, 18.0));
                    p.rect_filled(chip, radius(9), theme::c(0x4a4848));
                    p.galley(pos2(chip.left() + 6.0, chip.center().y - g.size().y / 2.0), g, Tokens::text_strong());
                    let xc = pos2(chip.right() - 9.0, chip.center().y);
                    p.circle_filled(xc, 5.0, Color32::from_white_alpha(170));
                    p.text(xc, Align2::CENTER_CENTER, "×", theme::strong(8.0), theme::c(0x4a4848));
                }
                Row::Segments(label, sub, icons_, lit, key, values) => {
                    let label_sub = if *key == "result" { result_sub.as_str() } else { sub };
                    p.text(pos2(lx, cy - 6.0), Align2::LEFT_CENTER, tr(label), theme::body(13.0), Tokens::text());
                    p.text(pos2(lx, cy + 8.0), Align2::LEFT_CENTER, tr(label_sub), theme::body(10.0), Tokens::text_dim());
                    let cell = 34.0;
                    let seg = Rect::from_min_max(pos2(rx - cell * icons_.len() as f32, cy - 11.0), pos2(rx, cy + 11.0));
                    p.rect_filled(seg, radius(6), seg_bg());
                    for (k, icon) in icons_.iter().enumerate() {
                        let c = Rect::from_min_size(pos2(seg.left() + cell * k as f32, seg.top()), vec2(cell, seg.height()));
                        if k == *lit {
                            p.rect_filled(c.shrink(2.0), radius(5), seg_on());
                        }
                        icons::paint(&p, Rect::from_center_size(c.center(), vec2(15.0, 15.0)), *icon, Tokens::text_strong());
                        if ui.interact(c, ui.scope_id().with(("card-seg", *key, k)), Sense::click()).clicked() && k != *lit {
                            change = Some((key, json!(values[k])));
                        }
                    }
                }
                Row::Switch(label, key, on) => {
                    p.text(pos2(lx, cy), Align2::LEFT_CENTER, tr(label), theme::body(13.0), Tokens::text());
                    let track = Rect::from_min_max(pos2(rx - 34.0, cy - 10.0), pos2(rx, cy + 10.0));
                    p.rect_filled(track, radius(10), if *on { Tokens::accent() } else { seg_bg() });
                    let knob = if *on { track.right() - 10.0 } else { track.left() + 10.0 };
                    p.circle_filled(pos2(knob, cy), 8.0, Color32::WHITE);
                    if ui.interact(track, ui.scope_id().with(("card-switch", *key)), Sense::click()).clicked() {
                        if *key == "keep" {
                            change = Some((key, json!(!*on)));
                        } else {
                            pick_hint = Some(if *key == "tangent_edges" {
                                "Fillets always run on along tangent edges here"
                            } else {
                                "Y-shaped blends aren't built yet"
                            });
                        }
                    }
                }
                Row::Choice(label, key, value, _) => {
                    p.text(pos2(lx, cy), Align2::LEFT_CENTER, tr(label), theme::body(13.0), Tokens::text());
                    let pill = Rect::from_min_max(pos2(rx - 108.0, cy - 10.0), pos2(rx, cy + 10.0));
                    p.rect_filled(pill, radius(5), pill_grey());
                    p.text(pos2(pill.center().x - 7.0, cy), Align2::CENTER_CENTER, tr(value), theme::body(12.0), Tokens::text_strong());
                    let knob = Rect::from_center_size(pos2(pill.right() - 9.5, cy), vec2(14.0, 15.0));
                    p.rect_filled(knob, radius(4), Tokens::accent());
                    icons::paint(
                        &p,
                        Rect::from_center_size(knob.center() - vec2(0.0, 2.5), vec2(7.0, 7.0)),
                        Icon::ChevronUp,
                        Color32::WHITE,
                    );
                    icons::paint(
                        &p,
                        Rect::from_center_size(knob.center() + vec2(0.0, 2.5), vec2(7.0, 7.0)),
                        Icon::ChevronDown,
                        Color32::WHITE,
                    );
                    if ui.interact(pill, ui.scope_id().with(("card-choice", *key)), Sense::click()).clicked() {
                        open_popup = Some((key, pos2(pill.left(), pill.top() - 2.0)));
                    }
                }
                Row::Number(label, key, kind, value, expr) => {
                    p.text(pos2(lx, cy), Align2::LEFT_CENTER, tr(label), theme::body(13.0), Tokens::text());
                    let field = Rect::from_min_max(pos2(rx - 132.0, cy - 10.0), pos2(rx, cy + 10.0));
                    p.rect_stroke(field, radius(5), Stroke::new(1.0, theme::c(0x4d4a4a)), StrokeKind::Inside);
                    let minus = Rect::from_min_size(field.min, vec2(20.0, field.height()));
                    let plus = Rect::from_min_max(pos2(field.right() - 20.0, field.top()), field.max);
                    p.vline(minus.right(), field.y_range(), Stroke::new(1.0, theme::c(0x4d4a4a)));
                    p.vline(plus.left(), field.y_range(), Stroke::new(1.0, theme::c(0x4d4a4a)));
                    p.text(minus.center(), Align2::CENTER_CENTER, "−", theme::body(13.0), Tokens::text());
                    p.text(plus.center(), Align2::CENTER_CENTER, "+", theme::body(13.0), Tokens::text());
                    let mid = Rect::from_min_max(pos2(minus.right(), field.top()), pos2(plus.left(), field.bottom()));
                    let step = match kind {
                        FieldKind::Length => units.scale(),
                        _ if *key == "curvature" => 0.1,
                        _ => 1.0,
                    };
                    if ui.interact(minus, ui.scope_id().with(("card-minus", *key)), Sense::click()).clicked() {
                        change = Some((key, json!(value - step)));
                    }
                    if ui.interact(plus, ui.scope_id().with(("card-plus", *key)), Sense::click()).clicked() {
                        change = Some((key, json!(value + step)));
                    }
                    let typing = matches!(&ws.card.editing, Some((k, ..)) if k == key);
                    if typing {
                        if let Some((_, text, focus)) = &mut ws.card.editing {
                            let id = ui.scope_id().with(("card-edit", *key));
                            let resp = ui.put(mid, egui::TextEdit::singleline(text).id(id).font(theme::body(12.0)).margin(vec2(2.0, 2.0)));
                            if *focus {
                                resp.request_focus();
                                *focus = false;
                            }
                            let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                            if esc {
                                ws.card.editing = None;
                            } else if enter || resp.lost_focus() {
                                if let Some(v) = parse(*kind, text, units) {
                                    change = Some((key, json!(v)));
                                }
                                ws.card.editing = None;
                            }
                        }
                    } else {
                        // Dragging across the value scrubs it a step per 6 points; let go to apply. A click types.
                        let resp = ui.interact(mid, ui.scope_id().with(("card-value", *key)), Sense::click_and_drag());
                        if resp.drag_started() {
                            ws.card.scrub = Some((key, *value, 0.0));
                        }
                        let mut scrubbed = None;
                        if let Some((k, base, dx)) = &mut ws.card.scrub
                            && k == key
                        {
                            *dx += resp.drag_delta().x;
                            let v = *base + (*dx / 6.0).round() as f64 * step;
                            let v = if matches!(kind, FieldKind::Count) { v.max(1.0) } else { v };
                            scrubbed = Some(v);
                            if resp.drag_stopped() {
                                if (v - *base).abs() > 1e-12 {
                                    change = Some((key, json!(v)));
                                }
                                ws.card.scrub = None;
                            }
                        }
                        if resp.hovered() || scrubbed.is_some() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                        }
                        let (shown, color) = match (scrubbed, expr) {
                            (Some(v), _) => (show_value(*kind, v, units), Tokens::accent()),
                            (None, Some(e)) => (e.clone(), Tokens::icon_blue()),
                            (None, None) => (show_value(*kind, *value, units), Tokens::text_strong()),
                        };
                        p.text(mid.center(), Align2::CENTER_CENTER, &shown, theme::body(12.0), color);
                        if resp.clicked() {
                            ws.card.editing = Some((key, show_value(*kind, *value, units).trim_end_matches(" mm").to_string(), true));
                        }
                    }
                }
            }
            y += h;
        }
    });
    if let Some(text) = pick_hint {
        ws.toast(text);
    }
    if ws.card.more {
        let rows = [
            ("Show on Tool Start", None, ws.settings.card_on_tool_start),
            ("Show on History Edit", None, ws.settings.card_on_history_edit),
        ];
        let at = pos2(card.right() - 150.0, card.top() + 30.0);
        let (hit, rect) = super::popover::pull_down(ui, "step-card-more", at, 170.0, &rows);
        match hit {
            Some(0) => ws.settings.card_on_tool_start = !ws.settings.card_on_tool_start,
            Some(_) => ws.settings.card_on_history_edit = !ws.settings.card_on_history_edit,
            None => {}
        }
        if hit.is_some()
            || ws.now - ws.popup_opened_at > 0.15
                && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !rect.contains(q)))
        {
            ws.card.more = false;
        }
    }
    if let Some((key, at)) = open_popup {
        ws.card.popup = if ws.card.popup == Some(key) { None } else { Some(key) };
        ws.popup_opened_at = ws.now;
        ws.card_popup_at = at;
    }
    if let Some(key) = ws.card.popup {
        let (current, options) = rows
            .iter()
            .find_map(|r| match r {
                Row::Choice(_, k, v, o) if *k == key => Some((*v, *o)),
                _ => None,
            })
            .unwrap_or(("", &[]));
        let rows: Vec<(&str, Option<Icon>, bool)> = options.iter().map(|(name, _, _)| (*name, None, *name == current)).collect();
        let top = ws.card_popup_at - vec2(0.0, 24.0 * options.len() as f32 / 2.0);
        let (hit, rect) = super::popover::pull_down(ui, "step-card-popup", top, 120.0, &rows);
        if let Some(k) = hit {
            ws.card.popup = None;
            if let Some((_, value, true)) = options.get(k) {
                change = Some((key, json!(value)));
            }
        } else if ws.now - ws.popup_opened_at > 0.15
            && ui.input(|i| i.pointer.primary_clicked() && i.pointer.interact_pos().is_some_and(|q| !rect.contains(q)))
        {
            ws.card.popup = None;
        }
    }
    if let Some(i) = profile_click {
        profile_edit(ws, i);
    }
    if let Some((key, value)) = change {
        apply(ws, source, key, value);
    }
}

fn apply(ws: &mut Workspace, source: Source, key: &'static str, value: Value) {
    match source {
        Source::Split { keep, .. } => {
            if key == "keep" && value.as_bool() != Some(keep) {
                ws.ui_action(super::toolbar::UiAction::ToolOption);
            }
        }
        Source::Fillet(t) => {
            let mut t = *t;
            match key {
                "mode" => {
                    t.chordal = value.as_str() == Some("chordal");
                    ws.variants.insert("Fillet Size Type", usize::from(t.chordal));
                }
                "corner" => {
                    t.setback = value.as_str() == Some("setback");
                    ws.variants.insert("Fillet Corner", usize::from(t.setback));
                }
                "continuity" => {
                    t.g2 = value.as_str() == Some("g2");
                    ws.variants.insert("Fillet Continuity", usize::from(t.g2));
                }
                "radius" => t.radius = value.as_f64().unwrap_or(t.radius).max(0.0),
                "curvature" => t.curvature = value.as_f64().unwrap_or(t.curvature).clamp(-1.0, 1.0),
                _ => return,
            }
            ws.preview = None;
            ws.tool = Tool::Fillet(t.clone());
            if t.radius > 1e-9 {
                ws.apply_fillet(&t);
            }
        }
        Source::Tool(t) => {
            let mut t = *t;
            match key {
                "sides" => t.symmetric = value.as_str() == Some("symmetric"),
                "extent" => {
                    t.through_all = value.as_str() == Some("through_all");
                    // To Object waits for the face or plane to be clicked.
                    t.picking = (value.as_str() == Some("to_object")).then_some("to");
                    t.to = None;
                }
                "start" => {
                    t.start_offset = (value.as_str() == Some("offset")).then_some(t.start_offset.unwrap_or(0.0));
                    t.start_plane = None;
                    if value.as_str() == Some("plane") {
                        t.picking = Some("start");
                    }
                }
                "start_offset" => t.start_offset = value.as_f64(),
                "result" => t.result = RESULTS.iter().find(|r| Some(**r) == value.as_str()).copied().unwrap_or("auto"),
                "distance" => t.distance = value.as_f64().unwrap_or(t.distance),
                "draft" => t.draft = value.as_f64().unwrap_or(t.draft).clamp(-84.0, 84.0),
                _ => {}
            }
            if t.through_all && t.distance.abs() < 1e-9 {
                t.distance = -1.0;
            }
            ws.tool = Tool::Extrude(t.clone());
            if t.distance.abs() > 1e-9 && t.picking.is_none() {
                ws.apply_extrude(&t);
            }
        }
        Source::Step(_) if matches!(value.as_str(), Some("to_object" | "plane")) => {
            ws.toast("Pick the face or plane while extruding: open the step's sketch and extrude again");
        }
        Source::Step(i) => {
            let mut params = serde_json::Map::new();
            // Start ▸ From Profile takes the offset out; Offset starts it at 0.
            let value = match key {
                "start" if value.as_str() == Some("offset") => json!(0.0),
                "start" => Value::Null,
                "curvature" => json!(value.as_f64().unwrap_or(0.0).clamp(-1.0, 1.0)),
                _ => value,
            };
            params.insert(if key == "start" { "start_offset" } else { key }.into(), value);
            let _ = ws.run("edit_step", json!({ "step": i, "params": params }));
        }
    }
}

/// Profile ▸ Edit... on a step: start picking its sketch's regions (the ones it uses picked), or with Done, rebuild
/// the step from the regions picked.
/// A failing step's "Fix…": a step built from sketch regions picks them again in the view (the ones still there
/// stay picked; Done in its card applies), anything else says what to do.
pub(super) fn fix(ws: &mut Workspace, step: usize) {
    ws.history.open = Some(step);
    ws.profile_edit = None;
    let all = ws.session.all_steps();
    let sketch = all.get(step).and_then(|h| h.entries.first()).and_then(|e| e.params.get("sketch")).and_then(Value::as_u64);
    match sketch.map(shapecraft_doc::ItemId) {
        Some(id) if ws.session.doc.sketch(id).is_some() => {
            profile_edit(ws, step);
            ws.toast("Pick the profile again, then Done");
        }
        Some(_) => ws.toast("The sketch this step was built on is gone: recreate it or edit the step"),
        None => ws.toast("Edit this step's settings, or recreate what it was built on"),
    }
}

fn profile_edit(ws: &mut Workspace, step: usize) {
    if let Some((s, sketch, regions)) = ws.profile_edit.take() {
        if s == step && !regions.is_empty() {
            let _ =
                ws.run("edit_step", json!({ "step": step, "params": { "sketch": sketch.0, "region": regions[0], "regions": regions } }));
        }
        return;
    }
    let all = ws.session.all_steps();
    let Some(e) = all.get(step).and_then(|h| h.entries.first()) else { return };
    let Some(sketch) = e.params.get("sketch").and_then(Value::as_u64).map(shapecraft_doc::ItemId) else {
        ws.toast("This step's profile is a face: pick it again by extruding the face");
        return;
    };
    let regions: Vec<u32> = match e.params.get("regions").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(|v| v.as_u64().map(|r| r as u32)).collect(),
        None => vec![e.params.get("region").and_then(Value::as_u64).unwrap_or(0) as u32],
    };
    ws.profile_edit = Some((step, sketch, regions));
}

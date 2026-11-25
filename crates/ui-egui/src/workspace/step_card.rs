//! The Parameter Card of 26.200 (observed s16-005..011, s29-011..027): a frosted card over the foot of the Items
//! sidebar with the feature's settings. The running Extrude or Fillet opens it from its ≡; clicking a History step
//! opens the same card for that step ("Extrude 01"), and a change there rebuilds the model. Its ⋯ menu says when it
//! opens by itself (Show on Tool Start / Show on History Edit); dragging its title moves it, and the spot is kept.

use egui::{Align2, Color32, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Value, json};
use shapecraft_doc::format_length;
use shapecraft_engine::history::{self, BREAKPOINT, FieldKind};

use super::Workspace;
use super::step_picks::{self, Slot};
use super::tools::{ExtrudeTool, FilletTool, ThreadTool, Tool};
use crate::i18n::tr;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens, radius};
use shapecraft_engine::thread::Standard;

/// Which pop-up is open and which number is being typed.
#[derive(Default)]
pub struct CardUi {
    popup: Option<&'static str>,
    editing: Option<(&'static str, String, bool)>,
    /// A number being scrubbed: its key, the value when the drag began, and how far it has gone (points).
    scrub: Option<(&'static str, f64, f32)>,
    /// The ⋯ menu is open.
    more: bool,
    /// How far a long pop-up list (a thread's sizes) is scrolled, in rows.
    list_scroll: f32,
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
    /// Thread (Faces to Thread, Extent, Length, Flip Start Side, Standard, Size, Pitch, Direction; then
    /// ours: Print Fit and its Clearance).
    Thread(Box<ThreadTool>),
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
    /// A choice whose options are worked out (a thread standard's sizes): label, key, shown value, (shown, value).
    List(&'static str, &'static str, String, Vec<(String, String)>),
}

const STANDARDS: &[(&str, &str, bool)] = &[("ISO", "iso", true), ("ANSI", "ansi", true), ("ACME", "acme", true)];
const HANDS: &[(&str, &str, bool)] = &[("Right Hand", "right", true), ("Left Hand", "left", true)];
const EXTENT_KINDS: &[&str] = &["full", "custom"];
/// Print Fit presets: radial clearance (mm) a printed thread gets on each part.
pub(super) const FITS: &[(&str, f64)] = &[("Off", 0.0), ("Snug", 0.1), ("Normal", 0.2), ("Loose", 0.3)];

/// A thread's settings, whichever way they're held.
struct Thread {
    faces: usize,
    custom: bool,
    length: f64,
    flip: bool,
    standard: Standard,
    size: String,
    pitch: f64,
    left: bool,
    clearance: f64,
}

fn fit_name(clearance: f64) -> &'static str {
    FITS.iter().find(|f| (f.1 - clearance).abs() < 1e-6).map_or("Custom", |f| f.0)
}

fn thread_rows(t: &Thread) -> Vec<Row> {
    let mut rows = vec![
        Row::Picks("Faces to Thread", "face", t.faces),
        Row::Segments(
            "Extent",
            if t.custom { "Custom Length" } else { "Full Length" },
            &[Icon::ThreadFull, Icon::ThreadCustom],
            usize::from(t.custom),
            "extent",
            EXTENT_KINDS,
        ),
    ];
    if t.custom {
        rows.push(Row::Number("Length", "length", FieldKind::Length, t.length, None));
        rows.push(Row::Switch("Flip Start Side", "flip", t.flip));
    }
    rows.push(Row::Choice("Standard", "standard", t.standard.label(), STANDARDS));
    let sizes = t.standard.sizes();
    let pitches = sizes.iter().find(|s| s.name == t.size).map(|s| s.pitches.clone()).unwrap_or_else(|| vec![t.pitch]);
    rows.push(Row::List("Size", "size", t.size.clone(), sizes.into_iter().map(|s| (s.name.clone(), s.name)).collect()));
    rows.push(Row::List(
        if t.standard.per_inch() { "Threads per Inch" } else { "Pitch" },
        "pitch",
        t.standard.pitch_label(t.pitch),
        pitches.iter().map(|&p| (t.standard.pitch_label(p), format!("{p}"))).collect(),
    ));
    rows.push(Row::Choice("Direction", "hand", if t.left { "Left Hand" } else { "Right Hand" }, HANDS));
    // Ours: room for a 3D-printed thread to turn (both parts get it).
    rows.push(Row::List(
        "Print Fit",
        "fit",
        fit_name(t.clearance).to_string(),
        FITS.iter().map(|(n, c)| (if *c > 0.0 { format!("{n} ({c} mm)") } else { (*n).to_string() }, format!("{c}"))).collect(),
    ));
    if t.clearance > 0.0 {
        rows.push(Row::Number("Clearance", "clearance", FieldKind::Length, t.clearance, None));
    }
    rows
}

fn tool_thread(t: &ThreadTool) -> Thread {
    Thread {
        faces: t.faces.len(),
        custom: t.custom,
        length: t.length,
        flip: t.flip,
        standard: t.standard,
        size: t.size.clone(),
        pitch: t.pitch,
        left: t.left,
        clearance: t.clearance,
    }
}

fn step_thread(ws: &Workspace, i: usize) -> Option<Thread> {
    let all = ws.session.all_steps();
    let e = all.get(i)?.entries.first()?;
    let p = &e.params;
    let standard = p.get("standard").and_then(Value::as_str).and_then(Standard::from_key).unwrap_or(Standard::Iso);
    let size = p.get("size").and_then(Value::as_str).map(str::to_string).unwrap_or_default();
    let pitch = p.get("pitch").and_then(Value::as_f64).or_else(|| standard.size(&size).map(|s| s.default_pitch())).unwrap_or(1.0);
    Some(Thread {
        faces: p.get("faces").and_then(Value::as_array).map_or(1, Vec::len),
        custom: p.get("extent").and_then(Value::as_str) == Some("custom"),
        length: p.get("length").and_then(Value::as_f64).unwrap_or(0.0),
        flip: p.get("flip").and_then(Value::as_bool).unwrap_or(false),
        standard,
        size,
        pitch,
        left: p.get("hand").and_then(Value::as_str) == Some("left"),
        clearance: p.get("clearance").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

const EXTENTS: &[(&str, &str, bool)] =
    &[("Distance", "distance", true), ("To Object", "to_object", true), ("Through All", "through_all", true)];
const STARTS: &[(&str, &str, bool)] = &[("From Profile", "profile", true), ("Offset", "offset", true), ("From Plane", "plane", true)];
const SIDES: &[&str] = &["one", "symmetric"];
const RESULTS: &[&str] = &["new", "union", "subtract", "intersect"];
const POSITIONS: &[(&str, &str, bool)] = &[
    ("Auto", "auto", true),
    ("Path Intersection", "intersection", true),
    ("Closest Point", "point", true),
    ("Closest Endpoint", "endpoint", true),
];
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

/// The Result row lit, and whether that is auto's pick. `side`: which side of the profile's plane the body auto
/// works on is (-1 behind it), `None` when auto makes a new body.
fn result_index(result: &str, side: Option<f64>, distance: f64) -> (usize, bool) {
    match result {
        "new" => (0, false),
        "union" => (1, false),
        "subtract" => (2, false),
        "intersect" => (3, false),
        _ => match side {
            None => (0, true),
            Some(side) => (if distance * side > 0.0 { 2 } else { 1 }, true),
        },
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
    /// Which side of the profile's plane auto's body is on, `None` for a new body (see [`result_index`]).
    side: Option<f64>,
    /// A depth being typed, which the Result row already counts (as the Result badge does).
    typed: Option<f64>,
    exprs: [Option<String>; 2],
    /// A step's To Object and Start Plane picks (how many), which its card can pick again.
    to_picks: Option<usize>,
    start_picks: Option<usize>,
}

fn extrude_rows(e: &Extrude) -> (Vec<Row>, String) {
    let (lit, auto) = result_index(&e.result, e.side, e.typed.unwrap_or(e.distance));
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
    if e.to_object
        && let Some(n) = e.to_picks
    {
        rows.push(Row::Picks("To Object", "item", n));
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
    if e.from_plane
        && let Some(n) = e.start_picks
    {
        rows.push(Row::Picks("Start Plane", "item", n));
    }
    if let Some(o) = e.offset {
        rows.push(Row::Number("Start Offset", "start_offset", FieldKind::Length, o, None));
    }
    rows.push(Row::Segments("Result", "", &[Icon::Body, Icon::Union, Icon::Subtract, Icon::Intersect], lit, "result", RESULTS));
    // A cut names what it cuts (26.202, s39-cut-card: "Subtract From  1 body  Edit...").
    if lit == 2 {
        rows.push(Row::Picks("Subtract From", "body", 1));
    }
    (rows, result_sub)
}

fn tool_extrude(ws: &Workspace, t: &ExtrudeTool) -> Extrude {
    // Live, the cut is already in the part: read the side off the step, as the Result badge does.
    let side = ws.extrude_auto_side(t);
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
        side,
        typed: ws.numeric.as_deref().and_then(|t| ws.eval_typed(t)),
        exprs: [None, None],
        to_picks: None,
        start_picks: None,
    }
}

fn step_extrude(ws: &Workspace, i: usize) -> Option<Extrude> {
    let all = ws.session.all_steps();
    let step = all.get(i)?;
    let e = step.entries.first()?;
    let get = |k: &str| e.params.get(k);
    let fields = history::feature("extrude")?.fields;
    let vars = &ws.session.doc.variables;
    // A free-plane profile resting on a body: the side that body was on when the step ran.
    let on_body = e.params.get("face").is_some()
        || e.params
            .get("sketch")
            .and_then(Value::as_u64)
            .and_then(|id| ws.session.doc.sketch(shapecraft_doc::ItemId(id)))
            .is_some_and(|s| s.on_body.is_some());
    let side = if on_body { Some(-1.0) } else { e.params.get("auto_side").and_then(Value::as_f64) };
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
        side,
        typed: None,
        exprs: [history::expression(step, &fields[0]).map(str::to_string), history::expression(step, &fields[1]).map(str::to_string)],
        to_picks: Some(usize::from(get("to").is_some_and(|v| !v.is_null()))),
        start_picks: Some(usize::from(get("start_plane").is_some_and(|v| !v.is_null()))),
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
    if step.kind == "thread" {
        return Some((step.label.clone(), thread_rows(&step_thread(ws, i)?), String::new()));
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
    for slot in step_picks::slots(&step.kind) {
        rows.push(Row::Picks(slot.label, slot.noun, step_picks::picks(step, slot).len()));
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
        rows.insert(2, Row::Choice("Profile Position", "position", param("position", POSITIONS), POSITIONS));
        rows.insert(3, Row::Choice("Orientation", "orientation", param("orientation", ORIENTATIONS), ORIENTATIONS));
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

/// Draws the card when there's one to show, and applies what was changed.
pub(super) fn show(ws: &mut Workspace, ui: &mut Ui) {
    // Picking a profile ends with its card.
    if ws.profile_edit.as_ref().is_some_and(|(s, ..)| Some(*s) != ws.history.open || !matches!(ws.tool, Tool::Idle)) {
        ws.profile_edit = None;
    }
    if ws.cut_edit.as_ref().is_some_and(|(s, _)| Some(*s) != ws.history.open || !matches!(ws.tool, Tool::Idle)) {
        ws.cut_edit = None;
    }
    if ws.edge_edit.as_ref().is_some_and(|e| Some(e.step) != ws.history.open || !matches!(ws.tool, Tool::Idle)) {
        edge_end(ws, false);
    }
    step_picks::tend(ws);
    // Show on Tool Start: a card tool that has just started opens its card once.
    // (A tool a pick started counts: the reference lights it as running, s29-010.)
    let tool_name = match &ws.tool {
        Tool::Extrude(_) => Some("extrude"),
        Tool::Fillet(t) if !t.chamfer => Some("fillet"),
        Tool::Thread(_) => Some("thread"),
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
        Tool::Thread(t) if ws.options_menu.is_some() => Source::Thread(Box::new(t.clone())),
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
        Source::Thread(t) => ("Thread".to_string(), thread_rows(&tool_thread(t)), String::new()),
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
    let mut cut_click: Option<usize> = None;
    let mut edge_click: Option<usize> = None;
    let mut cut_clear: Option<usize> = None;
    let mut slot_click: Option<(usize, Slot)> = None;
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
                    let cut_step = match (&source, *label) {
                        (Source::Step(i), "Subtract From") => Some(*i),
                        _ => None,
                    };
                    // A setting with picks of its own (a path, a target, the bodies moved).
                    let slot = match &source {
                        Source::Step(i) => {
                            let all = ws.session.all_steps();
                            let kind = all.get(*i).map_or("", |h| h.kind.as_str());
                            step_picks::slots(kind).iter().find(|s| s.label == *label).map(|s| (*i, *s))
                        }
                        _ => None,
                    };
                    let slot_editing = slot.is_some_and(|(i, s)| ws.pick_edit.as_ref().is_some_and(|e| e.step == i && e.slot == s));
                    let edge_editing = slot.is_none()
                        && *label == "Edge" && matches!((&source, ws.edge_edit.as_ref()), (Source::Step(i), Some(e)) if e.step == *i);
                    let editing = slot_editing
                        || edge_editing
                        || match cut_step {
                            Some(i) => ws.cut_edit.as_ref().is_some_and(|(s, _)| *s == i),
                            None if slot.is_some() => false,
                            None => matches!((&source, ws.profile_edit.as_ref()), (Source::Step(i), Some((s, ..))) if s == i),
                        };
                    let link = Rect::from_min_max(pos2(rx - 50.0, y + 8.0), pos2(rx, y + 28.0));
                    // Split Body's rows are only their chips (s29-027).
                    let editable = !matches!(source, Source::Split { .. });
                    if editable {
                        p.text(
                            pos2(rx, y + 18.0),
                            Align2::RIGHT_CENTER,
                            tr(if editing {
                                "Done"
                            } else if slot.is_some() && *count == 0 {
                                "Select..."
                            } else {
                                "Edit..."
                            }),
                            theme::body(12.5),
                            link_blue(),
                        );
                    }
                    if editable && ui.interact(link, ui.scope_id().with(("profile-edit", n)), Sense::click()).clicked() {
                        match (&source, *label) {
                            _ if slot.is_some() => slot_click = slot,
                            (Source::Step(i), "Profile") => profile_click = Some(*i),
                            (Source::Step(i), "Subtract From") => cut_click = Some(*i),
                            (_, "Subtract From") => pick_hint = Some("The cut goes into the body the profile rests on"),
                            (Source::Thread(_), _) => pick_hint = Some("Click cylindrical faces in the view to add or remove them"),
                            (Source::Step(_), "Faces to Thread") => {
                                pick_hint = Some("Pick the faces again: undo the thread and thread them anew")
                            }
                            (Source::Step(i), "Edge") => edge_click = Some(*i),
                            (Source::Step(_), _) => pick_hint = Some("Pick the edges again: open the step's body and fillet them anew"),
                            (Source::Split { .. }, _) => {
                                pick_hint = Some("Click bodies, faces, planes or sketches in the view to change them")
                            }
                            _ => pick_hint = Some("Click edges in the view to add or remove them"),
                        }
                    }
                    let count = match (cut_step, editing) {
                        _ if slot_editing => ws.pick_edit.as_ref().map_or(*count, |e| e.picks.len()),
                        _ if slot.is_some() => *count,
                        _ if edge_editing => ws.edge_edit.as_ref().map_or(*count, |e| e.edges.len()),
                        (Some(_), true) => ws.cut_edit.as_ref().map_or(*count, |(_, l)| l.len()),
                        (Some(i), false) => cut_list(ws, i).map_or(*count, |l| l.len()),
                        (None, true) => ws.profile_edit.as_ref().map_or(*count, |(_, _, r)| r.len()),
                        (None, false) => *count,
                    };
                    let g = p.layout_no_wrap(
                        crate::i18n::tr_s(format!("{count} {noun}{}", if count == 1 { "" } else { "s" })),
                        theme::strong(10.5),
                        Tokens::text_strong(),
                    );
                    let chip = Rect::from_min_size(pos2(lx, y + 39.0), vec2(g.size().x + 27.0, 18.0));
                    p.rect_filled(chip, radius(9), theme::c(0x4a4848));
                    p.galley(pos2(chip.left() + 6.0, chip.center().y - g.size().y / 2.0), g, Tokens::text_strong());
                    let xc = pos2(chip.right() - 9.0, chip.center().y);
                    p.circle_filled(xc, 5.0, Color32::from_white_alpha(170));
                    p.text(xc, Align2::CENTER_CENTER, "×", theme::strong(8.0), theme::c(0x4a4848));
                    // The × lets go of what the chip counts: for a cut, the bodies, picked again with Edit...
                    if let Some(i) = cut_step
                        && ui
                            .interact(Rect::from_center_size(xc, vec2(16.0, 16.0)), ui.scope_id().with(("cut-clear", n)), Sense::click())
                            .clicked()
                    {
                        cut_clear = Some(i);
                    }
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
                        // An option the kernel in use can't do is ghosted and says why.
                        let lacking = super::toolbar::option_feature(key, values[k]).is_some_and(super::toolbar::core_lacks);
                        let ink = if lacking { Tokens::text_ghost() } else { Tokens::text_strong() };
                        icons::paint(&p, Rect::from_center_size(c.center(), vec2(15.0, 15.0)), *icon, ink);
                        let resp = ui.interact(c, ui.scope_id().with(("card-seg", *key, k)), Sense::click());
                        let resp = if lacking { resp.on_hover_text(super::toolbar::NEEDS_OCCT) } else { resp };
                        if resp.clicked() && k != *lit && !lacking {
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
                        if *key == "keep" || *key == "flip" {
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
                Row::Choice(label, key, _, _) | Row::List(label, key, ..) => {
                    let value: &str = match row {
                        Row::Choice(_, _, v, _) => v,
                        Row::List(_, _, v, _) => v.as_str(),
                        _ => "",
                    };
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
                        ws.card.list_scroll = -1.0;
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
                                if let Some(v) = super::history::parse_value(*kind, text, units, &ws.session.doc.variables) {
                                    change = Some((key, v));
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
        let (current, options): (String, Vec<(String, String, bool)>) = rows
            .iter()
            .find_map(|r| match r {
                Row::Choice(_, k, v, o) if *k == key => {
                    Some(((*v).to_string(), o.iter().map(|(n, v, on)| ((*n).to_string(), (*v).to_string(), *on)).collect()))
                }
                Row::List(_, k, v, o) if *k == key => Some((v.clone(), o.iter().map(|(n, v)| (n.clone(), v.clone(), true)).collect())),
                _ => None,
            })
            .unwrap_or_default();
        let rows: Vec<(&str, Option<Icon>, bool)> = options
            .iter()
            .map(|(name, _, _)| (name.as_str(), None, name == &current || name.split(" (").next() == Some(current.as_str())))
            .collect();
        // A long list (a standard's sizes) shows 14 rows around the current one and scrolls.
        const SHOWN: usize = 14;
        let (hit, rect) = if rows.len() > SHOWN {
            if ws.card.list_scroll < 0.0 {
                let at = rows.iter().position(|r| r.2).unwrap_or(0);
                ws.card.list_scroll = at.saturating_sub(SHOWN / 2).min(rows.len() - SHOWN) as f32;
            }
            let top = ws.card_popup_at - vec2(0.0, 24.0 * SHOWN as f32 / 2.0);
            super::popover::pull_down_scrolled(ui, "step-card-popup", top, 120.0, &rows, SHOWN, &mut ws.card.list_scroll)
        } else {
            let top = ws.card_popup_at - vec2(0.0, 24.0 * options.len() as f32 / 2.0);
            super::popover::pull_down(ui, "step-card-popup", top, 120.0, &rows)
        };
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
    if let Some(i) = cut_click {
        if ws.cut_edit.as_ref().is_some_and(|(s, _)| *s == i) {
            cut_done(ws, i);
        } else {
            cut_begin(ws, i, false);
        }
    }
    if let Some(i) = edge_click {
        if ws.edge_edit.as_ref().is_some_and(|e| e.step == i) {
            edge_end(ws, true);
        } else {
            edge_begin(ws, i);
        }
    }
    if let Some(i) = cut_clear {
        cut_begin(ws, i, true);
    }
    if let Some((i, slot)) = slot_click {
        if ws.pick_edit.as_ref().is_some_and(|e| e.step == i && e.slot == slot) {
            step_picks::end(ws, true);
        } else if ws.pick_edit.is_none() {
            step_picks::begin(ws, i, slot, serde_json::Map::new());
        }
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
                    ws.variants.insert("Fillet Size Type".into(), usize::from(t.chordal));
                }
                "corner" => {
                    t.setback = value.as_str() == Some("setback");
                    ws.variants.insert("Fillet Corner".into(), usize::from(t.setback));
                }
                "continuity" => {
                    t.g2 = value.as_str() == Some("g2");
                    ws.variants.insert("Fillet Continuity".into(), usize::from(t.g2));
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
        Source::Thread(t) => {
            let mut t = *t;
            match key {
                "extent" => t.custom = value.as_str() == Some("custom"),
                "length" => t.length = value.as_f64().unwrap_or(t.length).clamp(0.01, (t.face.hi - t.face.lo).max(0.01)),
                "flip" => t.flip = value.as_bool().unwrap_or(!t.flip),
                "standard" => {
                    if let Some(st) = value.as_str().and_then(Standard::from_key) {
                        t.set_standard(st);
                        ws.variants.insert("Thread Standard".into(), Standard::ALL.iter().position(|s| *s == st).unwrap_or(0));
                    }
                }
                "size" => t.set_size(value.as_str().unwrap_or_default()),
                "pitch" => t.pitch = value.as_str().and_then(|v| v.parse().ok()).unwrap_or(t.pitch),
                "hand" => t.left = value.as_str() == Some("left"),
                "fit" | "clearance" => {
                    let c = value.as_f64().or_else(|| value.as_str().and_then(|v| v.parse().ok())).unwrap_or(t.clearance);
                    t.clearance = (c.max(0.0) * 1000.0).round() / 1000.0;
                    ws.variants.insert("Thread Clearance".into(), (t.clearance * 1000.0).round() as usize);
                }
                _ => return,
            }
            ws.apply_thread(t);
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
        // To Object and From Plane wait for what to be picked, then set both.
        Source::Step(i) if matches!(value.as_str(), Some("to_object" | "plane")) => {
            let want = if value.as_str() == Some("plane") { "start_plane" } else { "to" };
            let mut extra = serde_json::Map::new();
            if want == "to" {
                extra.insert("extent".into(), value);
            }
            if let Some(slot) = step_picks::slots("extrude").iter().find(|s| s.key == want) {
                step_picks::begin(ws, i, *slot, extra);
            }
        }
        Source::Step(i) if ws.session.all_steps().get(i).is_some_and(|s| s.kind == "thread") => {
            let Some(t) = step_thread(ws, i) else { return };
            let mut params = serde_json::Map::new();
            match key {
                "standard" => {
                    if let Some(st) = value.as_str().and_then(Standard::from_key) {
                        let major = t.standard.size(&t.size).map_or(10.0, |s| s.major);
                        let size = st.nearest(major);
                        params.insert("standard".into(), json!(st.key()));
                        params.insert("pitch".into(), json!(size.default_pitch()));
                        params.insert("size".into(), json!(size.name));
                    }
                }
                "size" => {
                    if let Some(size) = value.as_str().and_then(|n| t.standard.size(n)) {
                        params.insert("pitch".into(), json!(size.default_pitch()));
                        params.insert("size".into(), json!(size.name));
                    }
                }
                "pitch" => {
                    params.insert("pitch".into(), json!(value.as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(t.pitch)));
                }
                "fit" | "clearance" => {
                    let c = value.as_f64().or_else(|| value.as_str().and_then(|v| v.parse().ok())).unwrap_or(t.clearance);
                    params.insert("clearance".into(), json!((c.max(0.0) * 1000.0).round() / 1000.0));
                }
                "extent" if value.as_str() == Some("custom") && t.length <= 0.0 => {
                    params.insert("extent".into(), value);
                }
                _ => {
                    params.insert(key.into(), value);
                }
            }
            let _ = ws.run("edit_step", json!({ "step": i, "params": params }));
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
            if key == "start" {
                params.insert("start_plane".into(), Value::Null);
            }
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

/// The bodies a step's cut works on: the ones it names, else the ones the step changed (read off the models kept
/// after each step, so nothing is rebuilt to ask).
fn cut_list(ws: &Workspace, step: usize) -> Option<Vec<shapecraft_doc::ItemId>> {
    let named = ws.session.doc.history.get(step)?.entries.first()?.params.get("from").and_then(Value::as_array);
    if let Some(named) = named {
        return Some(named.iter().filter_map(|v| v.as_u64().map(shapecraft_doc::ItemId)).collect());
    }
    let revision = ws.session.revision;
    if let Some((r, s, list)) = ws.cut_found.borrow().as_ref()
        && *r == revision
        && *s == step
    {
        return Some(list.clone());
    }
    let list = ws.session.touches_kept()?.get(step)?.clone();
    *ws.cut_found.borrow_mut() = Some((revision, step, list.clone()));
    Some(list)
}

/// Subtract From ▸ Edit... (or its ×, which starts from nothing): the view picks the bodies, a click on one
/// toggles it, and Done applies them to the step.
fn cut_begin(ws: &mut Workspace, step: usize, empty: bool) {
    let list = if empty { Vec::new() } else { cut_list(ws, step).unwrap_or_default() };
    ws.session.selection = list.iter().map(|&body| shapecraft_doc::Pick::Body { body }).collect();
    ws.cut_edit = Some((step, list));
    ws.toast("Click the bodies to cut in the view, then Done");
}

fn cut_done(ws: &mut Workspace, step: usize) {
    let Some((s, list)) = ws.cut_edit.take() else { return };
    if s != step {
        return;
    }
    if list.is_empty() {
        ws.cut_edit = Some((s, list));
        ws.toast("Pick at least one body to cut");
        return;
    }
    ws.session.selection.clear();
    let ids: Vec<u64> = list.iter().map(|b| b.0).collect();
    let _ = ws.run("edit_step", json!({ "step": step, "params": { "from": ids } }));
}

/// A fillet's or chamfer's Edge ▸ Edit... (#89): the model rolls back to just before the step, showing the body as
/// the step found it, with the step's edges picked. A click on an edge of that body adds it or lets it go, and Done
/// rebuilds the step with them; Escape (or closing the card) puts everything back as it was.
#[derive(Clone, Debug)]
pub(crate) struct EdgeEdit {
    /// The step's place in the list while rolled back (the open card's).
    pub(crate) step: usize,
    /// Its place before, and the undo depth the rollback left (so it's taken back exactly).
    orig: usize,
    depth: usize,
    /// Where a breakpoint was before (counting steps without it), to put it back if the rollback can't be undone.
    held: Option<usize>,
    pub(crate) body: shapecraft_doc::ItemId,
    pub(crate) edges: Vec<u32>,
}

pub(super) fn edge_begin(ws: &mut Workspace, step: usize) {
    let all = ws.session.all_steps();
    let Some(e) = all.get(step).and_then(|h| h.entries.first()) else { return };
    let Some(body) = e.params.get("body").and_then(Value::as_u64).map(shapecraft_doc::ItemId) else {
        ws.toast("This step rounds a whole body: pick its edges again by filleting anew");
        return;
    };
    let edges: Vec<u32> = e
        .params
        .get("edges")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(|v| v.as_u64().map(|n| n as u32)).collect())
        .unwrap_or_default();
    let held = all.iter().position(|h| h.kind == BREAKPOINT);
    let plain = step - usize::from(held.is_some_and(|b| b < step));
    if ws.run("set_breakpoint", json!({ "before": plain })).is_none() {
        return;
    }
    let rolled = plain + 1;
    ws.history.open = Some(rolled);
    ws.history.editing = None;
    ws.profile_edit = None;
    ws.cut_edit = None;
    ws.session.selection = edges.iter().map(|&edge| shapecraft_doc::Pick::Edge { body, edge }).collect();
    ws.edge_edit = Some(EdgeEdit { step: rolled, orig: step, depth: ws.session.undo_depth(), held, body, edges });
    ws.toast("Click edges to add or remove them, then Done");
}

/// Ends picking a step's edges: with `apply`, the step is rebuilt with the picked ones (none picked keeps picking).
pub(super) fn edge_end(ws: &mut Workspace, apply: bool) {
    let Some(ed) = ws.edge_edit.take() else { return };
    if apply && ed.edges.is_empty() {
        ws.edge_edit = Some(ed);
        ws.toast("Pick at least one edge");
        return;
    }
    // Back to the model as it was: the rollback taken back exactly, or the breakpoint put back where it was.
    if !(ws.session.undo_depth() == ed.depth && ws.session.drop_last_step()) {
        let _ = ws.run("set_breakpoint", ed.held.map_or(json!({}), |b| json!({ "before": b })));
    }
    ws.session.selection.clear();
    ws.history.open = Some(ed.orig);
    if apply {
        let _ = ws.run("edit_step", json!({ "step": ed.orig, "params": { "body": ed.body.0, "edges": ed.edges } }));
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod cut_tests {
    use super::*;
    use shapecraft_doc::ItemId;
    use std::sync::Arc;

    /// #89: a fillet's Edge ▸ Edit... rolls the model back to before the fillet with its edge picked; another edge
    /// added and Done rebuild the fillet on both, as one step that a single undo takes back. Escape changes nothing.
    #[test]
    fn a_fillets_edges_are_picked_again() {
        let mut s = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Fillet edges");
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 20]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 20, "result": "new"})).unwrap();
        let body = s.doc.bodies[0].id;
        s.execute("fillet", &json!({"body": body.0, "edges": [0], "radius": 2})).unwrap();
        let step = s.all_steps().len() - 1;
        let mut ws = Workspace::new(s, crate::settings::Settings::default());
        let faces = |ws: &Workspace| ws.session.doc.bodies[0].mesh.triangle_faces.iter().max().map_or(0, |m| m + 1);
        let filleted = faces(&ws);
        ws.open_history_step(step);
        let depth = ws.session.undo_depth();

        // Escape: nothing changes.
        edge_begin(&mut ws, step);
        assert!(ws.edge_edit.is_some());
        assert!(faces(&ws) < filleted, "rolled back to the plain box: {} faces", faces(&ws));
        assert_eq!(ws.session.selection, vec![shapecraft_doc::Pick::Edge { body, edge: 0 }]);
        ws.escape();
        assert!(ws.edge_edit.is_none() && faces(&ws) == filleted && ws.session.undo_depth() == depth);
        assert_eq!(ws.history.open, Some(step));

        // Another edge, then Done.
        edge_begin(&mut ws, step);
        ws.edge_edit.as_mut().unwrap().edges.push(5);
        edge_end(&mut ws, true);
        assert!(ws.edge_edit.is_none() && !ws.session.at_breakpoint());
        let edges = ws.session.all_steps()[step].entries[0].params["edges"].clone();
        assert_eq!(edges, json!([0, 5]));
        assert!(faces(&ws) > filleted, "two edges rounded: {} faces", faces(&ws));
        assert_eq!(ws.session.undo_depth(), depth + 1, "one step");
        let _ = ws.run("undo", json!({}));
        assert_eq!(faces(&ws), filleted);
    }

    /// A cut step's Subtract From lists the bodies it cut; × starts over from none, Done refuses an empty list and
    /// otherwise names the picked bodies in the step; Escape ends the picking, then closes the card.
    #[test]
    fn subtract_from_picks_the_bodies_a_cut_works_on() {
        let mut s = shapecraft_engine::Session::new(Arc::new(shapecraft_kernel_occt::OcctKernel::new()), "Cut");
        for (a, b) in [(0, 10), (20, 30)] {
            s.execute("sketch", &json!({"plane": "xy"})).unwrap();
            s.execute("rectangle", &json!({"corner": [a, 0], "opposite": [b, 10]})).unwrap();
            s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10, "result": "new"})).unwrap();
        }
        s.execute("sketch", &json!({"plane": "xy"})).unwrap();
        s.execute("rectangle", &json!({"corner": [-5, -5], "opposite": [35, 15]})).unwrap();
        s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5, "result": "subtract"})).unwrap();
        let step = s.doc.history.len() - 1;
        let (first, second) = (s.doc.bodies[0].id, s.doc.bodies[1].id);
        let mut ws = Workspace::new(s, crate::settings::Settings::default());
        ws.open_history_step(step);
        // The cut found both bodies on its own.
        assert_eq!(cut_list(&ws, step).map(|l| l.len()), Some(2));
        // × starts from nothing: Done refuses until a body is picked.
        cut_begin(&mut ws, step, true);
        cut_done(&mut ws, step);
        assert!(ws.cut_edit.is_some(), "an empty list is refused");
        ws.cut_edit.as_mut().unwrap().1.push(second);
        cut_done(&mut ws, step);
        assert!(ws.cut_edit.is_none());
        assert_eq!(cut_list(&ws, step), Some(vec![second]));
        let volume = |ws: &Workspace, id: ItemId| ws.session.kernel.mass_props(&ws.session.doc.body(id).unwrap().shape).unwrap().volume;
        assert!((volume(&ws, first) - 1000.0).abs() < 1e-6, "the first body is no longer cut");
        assert!((volume(&ws, second) - 500.0).abs() < 1e-6);
        // Escape ends the picking, then closes the step's card.
        cut_begin(&mut ws, step, false);
        ws.escape();
        assert!(ws.cut_edit.is_none() && ws.history.open == Some(step));
        ws.escape();
        assert!(ws.history.open.is_none());
    }
}

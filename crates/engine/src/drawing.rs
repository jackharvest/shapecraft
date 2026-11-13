//! Drawings: sheets of views of bodies (exact hidden-line removal), laid out as the reference does – first
//! angle by default: front top left, the side view beside it, the top view below and an isometric in the
//! remaining corner – at one standard scale, with zone markers and a title block. Written out as SVG, DXF or PDF.

use glam::{DVec2, DVec3};
use shapecraft_doc::{DrawingSheet, SheetView, SheetViewKind};
use shapecraft_kernel::{ViewFrame, ViewLine};

use crate::{EngineError, Result, Session};

/// The border's inset from the paper edge, and the zone strip inside it.
pub const MARGIN: f64 = 10.0;
pub const ZONE: f64 = 8.0;
/// Title block size (bottom right, inside the frame).
pub const TITLE: DVec2 = DVec2::new(120.0, 36.0);

#[derive(Clone, Debug)]
pub struct DrawingView {
    pub name: String,
    /// Lines in sheet millimeters, y up from the sheet's bottom-left corner.
    pub lines: Vec<ViewLine>,
    /// Where the view's own middle (`mid`, in its model millimeters) sits on the sheet.
    pub at: DVec2,
    pub mid: DVec2,
    /// Sheet mm per model mm.
    pub scale: f64,
    /// What belongs to the view besides its lines: a section's hatching and cut line, a detail's frame and circle,
    /// their labels (sheet mm).
    pub extra: Vec<Mark>,
    /// The view's extent on the sheet.
    pub lo: DVec2,
    pub hi: DVec2,
    /// The view it was made from.
    pub parent: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Drawing {
    pub title: String,
    /// Paper size in mm.
    pub sheet: DVec2,
    /// Sheet mm per model mm.
    pub scale: f64,
    pub first_angle: bool,
    pub units: String,
    /// Today as (year, month, day).
    pub date: (i64, u32, u32),
    /// The sheet's size name ("A3").
    pub size: String,
    pub views: Vec<DrawingView>,
    pub style: shapecraft_doc::DrawingStyle,
    pub annotations: Vec<shapecraft_doc::Annotation>,
    /// Millimeters per display unit, for dimension values.
    pub unit_scale: f64,
    /// The pictures placed on the sheet (Image annotations refer to them by id).
    pub pictures: Vec<shapecraft_doc::DecalImage>,
}

/// View directions (toward the viewer) and the screen-right axis.
const FRONT: ([f64; 3], [f64; 3]) = ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0]);
const TOP: ([f64; 3], [f64; 3]) = ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
const LEFT: ([f64; 3], [f64; 3]) = ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]);
const RIGHT: ([f64; 3], [f64; 3]) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
const ISO: ([f64; 3], [f64; 3]) = ([1.0, -1.0, 1.0], [1.0, 1.0, 0.0]);

/// The base view orientations offered, by key, as (toward the viewer, screen right).
pub fn orientation(key: &str) -> Option<(DVec3, DVec3)> {
    let (t, x) = match key {
        "front" => FRONT,
        "back" => ([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]),
        "left" => LEFT,
        "right" => RIGHT,
        "top" => TOP,
        "bottom" => ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),
        "iso" | "iso_tr" => ISO,
        "iso_tl" => ([-1.0, -1.0, 1.0], [1.0, -1.0, 0.0]),
        "iso_br" => ([1.0, -1.0, -1.0], [1.0, 1.0, 0.0]),
        "iso_bl" => ([-1.0, -1.0, -1.0], [1.0, -1.0, 0.0]),
        _ => return None,
    };
    Some((DVec3::from_array(t).normalize(), DVec3::from_array(x).normalize()))
}

/// The name a view looking from `toward` goes by.
pub fn orientation_name(toward: DVec3) -> &'static str {
    let t = toward.normalize_or_zero();
    for (key, name) in [("front", "Front"), ("back", "Back"), ("left", "Left"), ("right", "Right"), ("top", "Top"), ("bottom", "Bottom")] {
        if orientation(key).is_some_and(|(d, _)| d.dot(t) > 0.999) {
            return name;
        }
    }
    if t.x.abs() > 0.3 && t.y.abs() > 0.3 && t.z.abs() > 0.3 { "Isometric" } else { "View" }
}

/// The frame of a view one step (`dx`, `dy`) from `parent`: orthographic beside it, isometric on the diagonals,
/// following the projection angle.
pub fn projected_frame(parent: ViewFrame, dx: i8, dy: i8, first_angle: bool) -> Option<ViewFrame> {
    let (t, x) = (parent.toward, parent.x);
    let u = t.cross(x);
    // First angle shows, beside a view, the side facing away from where it's placed.
    let side = (x * f64::from(dx) + u * f64::from(dy)) * if first_angle { -1.0 } else { 1.0 };
    let (toward, right) = if dx != 0 && dy != 0 {
        let toward = (t + side).normalize_or_zero();
        (toward, u.cross(toward).normalize_or_zero())
    } else if dx != 0 {
        (side, u.cross(side).normalize_or_zero())
    } else if dy != 0 {
        (side, x)
    } else {
        return None;
    };
    (toward != DVec3::ZERO && right != DVec3::ZERO).then_some(ViewFrame { origin: parent.origin, toward, x: right })
}

fn bounds(lines: &[ViewLine]) -> Option<(DVec2, DVec2)> {
    let mut it = lines.iter().flat_map(|l| l.points.iter()).peekable();
    it.peek()?;
    Some(it.fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
}

/// Standard scales, largest first.
pub const SCALES: [f64; 17] = [50.0, 20.0, 12.0, 10.0, 5.0, 4.0, 2.0, 1.0, 0.5, 0.4, 0.25, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005];

/// The largest standard scale at which `size` fits into `room`.
fn standard_scale(size: DVec2, room: DVec2) -> f64 {
    SCALES.into_iter().find(|s| size.x * s <= room.x && size.y * s <= room.y).unwrap_or(0.001)
}

/// "1:2", "2:1", "1:1".
pub fn scale_label(scale: f64) -> String {
    if scale >= 1.0 { format!("{}:1", trim(scale)) } else { format!("1:{}", trim(1.0 / scale)) }
}

fn trim(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A drawing of every visible body on an A3 landscape sheet with four views (for quick exports).
pub fn default_sheet(name: &str) -> DrawingSheet {
    DrawingSheet {
        id: shapecraft_doc::ItemId(0),
        name: name.to_string(),
        bodies: Vec::new(),
        sheet: "A3".into(),
        landscape: true,
        scale: None,
        first_angle: true,
        four_views: true,
        style: Default::default(),
        annotations: Vec::new(),
        views: None,
    }
}

/// A point of a sheet as fractions across and up the drawing area above the title block.
pub fn sheet_point(spec: &DrawingSheet, px: f64, py: f64) -> DVec2 {
    let (w, h) = DrawingSheet::paper(&spec.sheet, spec.landscape).unwrap_or((420.0, 297.0));
    let inner_min = DVec2::splat(MARGIN + ZONE);
    let inner = DVec2::new(w, h) - inner_min * 2.0;
    DVec2::new(inner_min.x + inner.x * px, inner_min.y + TITLE.y + (inner.y - TITLE.y) * py)
}

/// The views a new sheet starts with: the front alone in the middle, or the front with its side and top
/// projections and an isometric, laid out for the sheet's projection angle.
pub fn standard_views(spec: &DrawingSheet, four: bool) -> Vec<SheetView> {
    let base = |name: &str, key: &str, at: DVec2| {
        let (toward, x) = orientation(key).unwrap_or((DVec3::NEG_Y, DVec3::X));
        SheetView { name: name.into(), kind: SheetViewKind::Base { toward, x }, at, scale: None, hidden_lines: true, bodies: Vec::new() }
    };
    let proj = |name: &str, dx: i8, dy: i8, at: DVec2| SheetView {
        name: name.into(),
        kind: SheetViewKind::Projected { parent: "Front".into(), dx, dy },
        at,
        scale: None,
        hidden_lines: true,
        bodies: Vec::new(),
    };
    let p = |x: f64, y: f64| sheet_point(spec, x, y);
    if !four {
        return vec![base("Front", "front", p(0.5, 0.5))];
    }
    if spec.first_angle {
        vec![
            base("Front", "front", p(0.25, 0.75)),
            proj("Left", 1, 0, p(0.75, 0.75)),
            proj("Top", 0, -1, p(0.25, 0.25)),
            base("Isometric", "iso", p(0.75, 0.25)),
        ]
    } else {
        vec![
            base("Front", "front", p(0.25, 0.25)),
            proj("Right", 1, 0, p(0.75, 0.25)),
            proj("Top", 0, 1, p(0.25, 0.75)),
            base("Isometric", "iso", p(0.75, 0.75)),
        ]
    }
}

fn today() -> (i64, u32, u32) {
    shapecraft_doc::date::local_date(std::time::SystemTime::now())
}

/// A date written in one of the title block's formats ("MM/DD/YY", "DD-MMM-YY", "YYYYMMDD"…).
pub fn format_date((y, m, d): (i64, u32, u32), format: &str) -> String {
    const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let mut out = String::new();
    let mut rest = format;
    while !rest.is_empty() {
        let (piece, n) = if rest.starts_with("YYYY") {
            (format!("{y:04}"), 4)
        } else if rest.starts_with("YY") {
            (format!("{:02}", y.rem_euclid(100)), 2)
        } else if rest.starts_with("MMM") {
            (MONTHS.get((m as usize).saturating_sub(1)).copied().unwrap_or("").to_string(), 3)
        } else if rest.starts_with("MM") {
            (format!("{m:02}"), 2)
        } else if rest.starts_with("DD") {
            (format!("{d:02}"), 2)
        } else {
            let c = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            (rest[..c].to_string(), c)
        };
        out.push_str(&piece);
        rest = &rest[n..];
    }
    out
}

/// A revision table: REVISION | NAME | DATE over `rows` empty rows.
fn revision_table(cells: &mut Vec<(f64, f64, f64, f64, &str, String)>, x0: f64, y0: f64, x1: f64, rows: usize) {
    let w = x1 - x0;
    let cols = [(0.0, 1.0 / 3.0, "REVISION"), (1.0 / 3.0, 2.0 / 3.0, "NAME"), (2.0 / 3.0, 1.0, "DATE")];
    let header = y0 + 7.0 * rows as f64;
    for (a, b, label) in cols {
        cells.push((x0 + w * a, header, x0 + w * b, header + 6.0, label, String::new()));
        for k in 0..rows {
            let y = y0 + 7.0 * k as f64;
            cells.push((x0 + w * a, y, x0 + w * b, y + 7.0, "", String::new()));
        }
    }
}

/// The projection-angle symbol: a truncated cone's side view and its end view (two circles); first angle has the
/// circles on the right of the side view, third angle on the left.
fn proj_symbol(out: &mut Vec<Mark>, at: DVec2, size: f64, first_angle: bool) {
    let line = |a: DVec2, b: DVec2| Mark::Line { a, b, width: 0.25, dashed: false };
    let (side, end) = if first_angle { (at, at + DVec2::new(size * 2.2, 0.0)) } else { (at + DVec2::new(size * 2.2, 0.0), at) };
    let (hw, big, small) = (size * 0.8, size * 0.9, size * 0.45);
    let pts = [side + DVec2::new(-hw, -small), side + DVec2::new(hw, -big), side + DVec2::new(hw, big), side + DVec2::new(-hw, small)];
    for k in 0..4 {
        out.push(line(pts[k], pts[(k + 1) % 4]));
    }
    for r in [big, small] {
        for i in 0..24 {
            let (t0, t1) = (std::f64::consts::TAU * i as f64 / 24.0, std::f64::consts::TAU * (i + 1) as f64 / 24.0);
            out.push(line(end + DVec2::from_angle(t0) * r, end + DVec2::from_angle(t1) * r));
        }
    }
}

/// A view direction and its up vector.
type ViewDir = ([f64; 3], [f64; 3]);

/// Lays out a sheet's views.
pub fn make(s: &Session, spec: &DrawingSheet) -> Result<Drawing> {
    let bodies: Vec<_> =
        s.doc.bodies.iter().filter(|b| if spec.bodies.is_empty() { b.visible } else { spec.bodies.contains(&b.id) }).collect();
    if bodies.is_empty() {
        return Err(EngineError::NotAvailable("there are no bodies to draw".into()));
    }
    let shapes: Vec<_> = bodies.iter().map(|b| b.shape.clone()).collect();
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for b in &bodies {
        let bb = crate::cmd::model::body_box(s, b)?;
        min = min.min(bb.min);
        max = max.max(bb.max);
    }
    let (w, h) = DrawingSheet::paper(&spec.sheet, spec.landscape)
        .ok_or_else(|| EngineError::BadParams(format!("unknown sheet '{}'", spec.sheet)))?;
    let sheet = DVec2::new(w, h);
    let center = (min + max) / 2.0;
    let deflection = ((max - min).max_element() * 1e-3).max(1e-3);
    let inner_min = DVec2::splat(MARGIN + ZONE);
    let inner = sheet - inner_min * 2.0;
    let (views, scale) = match &spec.views {
        None => auto_views(s, spec, &shapes, center, deflection, inner_min, inner)?,
        Some(placed) => placed_views(s, spec, placed, &shapes, center, max - min, deflection, inner)?,
    };
    Ok(Drawing {
        title: spec.name.clone(),
        sheet,
        scale,
        first_angle: spec.first_angle,
        units: s.doc.units.suffix().to_string(),
        date: today(),
        size: spec.sheet.clone(),
        views,
        style: spec.style.clone(),
        annotations: spec.annotations.clone(),
        unit_scale: s.doc.units.scale(),
        pictures: s.doc.decal_images.iter().filter(|i| i.sheet).cloned().collect(),
    })
}

/// A view's lines moved onto the sheet, and their extent.
fn place(name: String, lines: Vec<ViewLine>, at: DVec2, mid: DVec2, scale: f64, parent: Option<String>) -> DrawingView {
    let lines: Vec<ViewLine> = lines
        .into_iter()
        .map(|l| ViewLine { points: l.points.iter().map(|p| at + (*p - mid) * scale).collect(), hidden: l.hidden })
        .collect();
    let (lo, hi) = bounds(&lines).unwrap_or((at - DVec2::splat(10.0), at + DVec2::splat(10.0)));
    DrawingView { name, lines, at, mid, scale, extra: Vec::new(), lo, hi, parent }
}

/// The front view, or the four views, laid out by themselves (drawings saved before views could be placed).
fn auto_views(
    s: &Session,
    spec: &DrawingSheet,
    shapes: &[shapecraft_kernel::Shape],
    center: DVec3,
    deflection: f64,
    inner_min: DVec2,
    inner: DVec2,
) -> Result<(Vec<DrawingView>, f64)> {
    // First angle puts the left view to the right of the front and the top view below it; third angle the right
    // view to the right and the top view above.
    let set: Vec<(&'static str, ViewDir, (f64, f64))> = if !spec.four_views {
        vec![("Front", FRONT, (0.5, 0.5))]
    } else if spec.first_angle {
        vec![("Front", FRONT, (0.25, 0.75)), ("Left", LEFT, (0.75, 0.75)), ("Top", TOP, (0.25, 0.25)), ("Isometric", ISO, (0.75, 0.25))]
    } else {
        vec![("Top", TOP, (0.25, 0.75)), ("Isometric", ISO, (0.75, 0.75)), ("Front", FRONT, (0.25, 0.25)), ("Right", RIGHT, (0.75, 0.25))]
    };
    let mut raw = Vec::new();
    for (name, (toward, x), place) in &set {
        let view = ViewFrame { origin: center, toward: DVec3::from_array(*toward).normalize(), x: DVec3::from_array(*x).normalize() };
        raw.push((*name, s.kernel.hidden_lines(shapes, view, deflection)?, *place));
    }
    let per = if spec.four_views { 0.5 } else { 1.0 };
    let cell = DVec2::new(inner.x * per, (inner.y - TITLE.y) * per) - DVec2::splat(14.0);
    let scale = spec.scale.unwrap_or_else(|| {
        raw.iter().map(|(_, l, _)| standard_scale(bounds(l).map_or(DVec2::ONE, |(lo, hi)| hi - lo), cell)).fold(f64::INFINITY, f64::min)
    });
    let views = raw
        .into_iter()
        .map(|(name, lines, (px, py))| {
            let at = DVec2::new(inner_min.x + inner.x * px, inner_min.y + TITLE.y + (inner.y - TITLE.y) * py);
            let mid = bounds(&lines).map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0);
            place(name.to_string(), lines, at, mid, scale, None)
        })
        .collect();
    Ok((views, scale))
}

/// A placed view before it goes on the sheet: its frame, its lines in its own model mm, and a section's cut.
struct Raw<'a> {
    spec: &'a SheetView,
    frame: ViewFrame,
    lines: Vec<ViewLine>,
    mid: DVec2,
    cut: Vec<(DVec2, DVec2)>,
    /// The bodies it shows (empty: the drawing's).
    bodies: Vec<shapecraft_doc::ItemId>,
}

/// The views the drawing lists, each from the one it was made from.
#[allow(clippy::too_many_arguments)]
fn placed_views(
    s: &Session,
    spec: &DrawingSheet,
    placed: &[SheetView],
    shapes: &[shapecraft_kernel::Shape],
    center: DVec3,
    size: DVec3,
    deflection: f64,
    inner: DVec2,
) -> Result<(Vec<DrawingView>, f64)> {
    let mut raws: Vec<Raw> = Vec::new();
    for v in placed {
        let parent = v.kind.parent().and_then(|p| raws.iter().find(|r| r.spec.name == p));
        // A base view may show only some bodies; the views made from it show the same ones.
        let picked = match (&v.kind, parent) {
            (SheetViewKind::Base { .. }, _) => v.bodies.clone(),
            (_, Some(p)) => p.bodies.clone(),
            _ => Vec::new(),
        };
        let own: Vec<shapecraft_kernel::Shape> = s.doc.bodies.iter().filter(|b| picked.contains(&b.id)).map(|b| b.shape.clone()).collect();
        let shapes = if picked.is_empty() { shapes } else { &own[..] };
        let mut raw = match (&v.kind, parent) {
            (SheetViewKind::Base { toward, x }, _) => {
                let t = toward.normalize_or_zero();
                let x = (*x - t * x.dot(t)).normalize_or_zero();
                if t == DVec3::ZERO || x == DVec3::ZERO {
                    continue;
                }
                let frame = ViewFrame { origin: center, toward: t, x };
                Raw {
                    spec: v,
                    frame,
                    lines: s.kernel.hidden_lines(shapes, frame, deflection)?,
                    mid: DVec2::ZERO,
                    cut: Vec::new(),
                    bodies: Vec::new(),
                }
            }
            (SheetViewKind::Projected { dx, dy, .. }, Some(p)) => {
                let Some(frame) = projected_frame(p.frame, *dx, *dy, spec.first_angle) else { continue };
                Raw {
                    spec: v,
                    frame,
                    lines: s.kernel.hidden_lines(shapes, frame, deflection)?,
                    mid: DVec2::ZERO,
                    cut: Vec::new(),
                    bodies: Vec::new(),
                }
            }
            (SheetViewKind::Section { a, b, flip, .. }, Some(p)) => {
                let Some((frame, pieces, plane)) = section_frame(s, p.frame, *a, *b, *flip, shapes)? else { continue };
                let lines = if pieces.is_empty() { Vec::new() } else { s.kernel.hidden_lines(&pieces, frame, deflection)? };
                let cut = cut_outline(s, &pieces, plane, frame, size.max_element())?;
                Raw { spec: v, frame, lines, mid: DVec2::ZERO, cut, bodies: Vec::new() }
            }
            (SheetViewKind::Detail { center: c, radius, .. }, Some(p)) => {
                if !(radius.is_finite() && *radius > 0.0) {
                    continue;
                }
                Raw { spec: v, frame: p.frame, lines: clip_to_circle(&p.lines, *c, *radius), mid: *c, cut: Vec::new(), bodies: Vec::new() }
            }
            _ => continue,
        };
        raw.bodies = picked;
        raws.push(raw);
    }
    // The drawing's scale fits the front, side and top of the model each into a quarter of the sheet, as four
    // views would (whichever views are placed, so adding one doesn't rescale the rest).
    let cell = DVec2::new(inner.x * 0.5, (inner.y - TITLE.y) * 0.5) - DVec2::splat(14.0);
    let scale = spec.scale.unwrap_or_else(|| {
        [DVec2::new(size.x, size.z), DVec2::new(size.y, size.z), DVec2::new(size.x, size.y)]
            .into_iter()
            .map(|v| standard_scale(v, cell))
            .fold(f64::INFINITY, f64::min)
    });
    let style = &spec.style;
    let mut out: Vec<DrawingView> = Vec::new();
    for r in raws {
        let v = r.spec;
        let parent = v.kind.parent().and_then(|p| out.iter().position(|o| o.name == p));
        let parent_scale = parent.and_then(|i| out.get(i)).map_or(scale, |p| p.scale);
        let view_scale = match &v.kind {
            SheetViewKind::Projected { .. } => parent_scale,
            SheetViewKind::Detail { .. } => v.scale.unwrap_or(parent_scale * 2.0),
            _ => v.scale.unwrap_or(scale),
        };
        let mut at = v.at;
        if let (SheetViewKind::Projected { dx, dy, .. }, Some(p)) = (&v.kind, parent.and_then(|i| out.get(i))) {
            if *dy == 0 {
                at.y = p.at.y;
            } else if *dx == 0 {
                at.x = p.at.x;
            }
        }
        let lines = r.lines.into_iter().filter(|l| v.hidden_lines || !l.hidden).collect();
        let mut view = place(v.name.clone(), lines, at, r.mid, view_scale, v.kind.parent().map(str::to_string));
        let to_sheet = |q: DVec2| at + (q - r.mid) * view_scale;
        match &v.kind {
            SheetViewKind::Section { a, b, flip, letter, .. } => {
                view.extra.extend(hatch(&r.cut, 2.5 / view_scale).into_iter().map(|(p, q)| Mark::Line {
                    a: to_sheet(p),
                    b: to_sheet(q),
                    width: style.section,
                    dashed: false,
                }));
                view.extra.push(Mark::Text {
                    at: DVec2::new(at.x, view.lo.y - 6.0),
                    text: format!("{letter}-{letter}"),
                    size: 4.0,
                    mono: false,
                    center: true,
                });
                if let Some(p) = parent.and_then(|i| out.get(i)) {
                    let (pa, pb) = (p.at + (*a - p.mid) * p.scale, p.at + (*b - p.mid) * p.scale);
                    let dir = (pb - pa).normalize_or_zero();
                    let sight = if *flip { -dir.perp() } else { dir.perp() };
                    let (ea, eb) = (pa - dir * 4.0, pb + dir * 4.0);
                    view.extra.push(Mark::Line { a: ea, b: eb, width: style.centerline, dashed: true });
                    for e in [ea, eb] {
                        // A heavy tick at each end, an arrow pointing the way the section looks, and the letter.
                        view.extra.push(Mark::Line {
                            a: e,
                            b: e + (pb - pa).normalize_or_zero() * if e == ea { 5.0 } else { -5.0 },
                            width: style.visible,
                            dashed: false,
                        });
                        let tail = e - sight * 7.0;
                        view.extra.push(Mark::Line { a: tail, b: e, width: style.dimension, dashed: false });
                        let n = sight.perp();
                        view.extra.push(Mark::Line { a: e, b: e - sight * 2.5 + n * 0.9, width: style.dimension, dashed: false });
                        view.extra.push(Mark::Line { a: e, b: e - sight * 2.5 - n * 0.9, width: style.dimension, dashed: false });
                        view.extra.push(Mark::Text { at: tail - sight * 2.5, text: letter.clone(), size: 4.0, mono: false, center: true });
                    }
                }
            }
            SheetViewKind::Detail { center: c, radius, letter, .. } => {
                let rs = radius * view_scale;
                view.extra.extend(circle_marks(at, rs, style.detail, false));
                view.lo = view.lo.min(at - DVec2::splat(rs));
                view.hi = view.hi.max(at + DVec2::splat(rs));
                view.extra.push(Mark::Text {
                    at: DVec2::new(at.x, at.y - rs - 5.0),
                    text: letter.clone(),
                    size: 4.0,
                    mono: false,
                    center: true,
                });
                view.extra.push(Mark::Text {
                    at: DVec2::new(at.x, at.y - rs - 10.0),
                    text: format!("SCALE {}", scale_label(view_scale)),
                    size: 3.0,
                    mono: false,
                    center: true,
                });
                if let Some(p) = parent.and_then(|i| out.get(i)) {
                    let pc = p.at + (*c - p.mid) * p.scale;
                    let pr = radius * p.scale;
                    view.extra.extend(circle_marks(pc, pr, style.detail, true));
                    let d = DVec2::new(1.0, 1.0).normalize();
                    let (rim, end) = (pc + d * pr, pc + d * (pr + 8.0));
                    view.extra.push(Mark::Line { a: rim, b: end, width: style.dimension, dashed: false });
                    view.extra.push(Mark::Text { at: end + d * 3.0, text: letter.clone(), size: 4.0, mono: false, center: true });
                }
            }
            _ => {}
        }
        out.push(view);
    }
    Ok((out, scale))
}

/// A circle as short lines; dashed leaves gaps (about 4 mm drawn, 1.5 mm gap) rather than marking the lines.
fn circle_marks(c: DVec2, r: f64, width: f64, dashed: bool) -> Vec<Mark> {
    let n = 96;
    let seg = std::f64::consts::TAU * r / n as f64;
    let (on, period) = if dashed { ((4.0 / seg).round().max(1.0) as usize, ((5.5 / seg).round() as usize).max(2)) } else { (1, 1) };
    (0..n)
        .filter(|i| i % period < on)
        .map(|i| {
            let (t0, t1) = (std::f64::consts::TAU * i as f64 / n as f64, std::f64::consts::TAU * (i + 1) as f64 / n as f64);
            Mark::Line { a: c + DVec2::from_angle(t0) * r, b: c + DVec2::from_angle(t1) * r, width, dashed: false }
        })
        .collect()
}

/// The section of the model along a parent view's line a→b: the view's frame, the pieces beyond the cut, and the
/// cut plane (a point and its normal toward the viewer).
#[allow(clippy::type_complexity)]
fn section_frame(
    s: &Session,
    parent: ViewFrame,
    a: DVec2,
    b: DVec2,
    flip: bool,
    shapes: &[shapecraft_kernel::Shape],
) -> Result<Option<(ViewFrame, Vec<shapecraft_kernel::Shape>, (DVec3, DVec3))>> {
    let dir = (b - a).normalize_or_zero();
    if dir == DVec2::ZERO || !a.is_finite() || !b.is_finite() {
        return Ok(None);
    }
    let up = parent.toward.cross(parent.x);
    let world = |q: DVec2| parent.origin + parent.x * q.x + up * q.y;
    let d = parent.x * dir.x + up * dir.y;
    let sight2 = if flip { -dir.perp() } else { dir.perp() };
    let sight = parent.x * sight2.x + up * sight2.y;
    let toward = -sight;
    let x = if dir.x.abs() >= dir.y.abs() { d * dir.x.signum() } else { (d * dir.y.signum()).cross(toward) };
    let frame = ViewFrame { origin: parent.origin, toward, x: x.normalize_or_zero() };
    if frame.x == DVec3::ZERO {
        return Ok(None);
    }
    let p0 = world(a);
    let mut pieces = Vec::new();
    for shape in shapes {
        let parts = s.kernel.split_by_plane(shape, p0, sight).unwrap_or_else(|_| vec![shape.clone()]);
        for part in parts {
            let bb = s.kernel.bounds(&part)?;
            if ((bb.min + bb.max) / 2.0 - p0).dot(sight) > 0.0 {
                pieces.push(part);
            }
        }
    }
    Ok(Some((frame, pieces, (p0, toward))))
}

/// The outlines of the pieces' faces lying on the cut plane, in the view's model mm.
fn cut_outline(
    s: &Session,
    pieces: &[shapecraft_kernel::Shape],
    plane: (DVec3, DVec3),
    frame: ViewFrame,
    size: f64,
) -> Result<Vec<(DVec2, DVec2)>> {
    use shapecraft_kernel::EdgeCurve;
    let (p0, n) = plane;
    let up = frame.toward.cross(frame.x);
    let flat = |w: DVec3| DVec2::new((w - frame.origin).dot(frame.x), (w - frame.origin).dot(up));
    let tol = size * 1e-6 + 1e-6;
    let mut segs = Vec::new();
    for piece in pieces {
        for f in 0..s.kernel.face_count(piece)? {
            let Some(fp) = s.kernel.face_plane(piece, f)? else { continue };
            if fp.normal.dot(n).abs() < 0.999 || (fp.origin - p0).dot(n).abs() > tol {
                continue;
            }
            for e in s.kernel.face_edges(piece, f)? {
                let pts: Vec<DVec2> = match s.kernel.edge_curve(piece, e)? {
                    EdgeCurve::Line { a, b } => vec![flat(a), flat(b)],
                    EdgeCurve::Circle { center, radius, .. } => {
                        let c = flat(center);
                        (0..=64).map(|i| c + DVec2::from_angle(std::f64::consts::TAU * i as f64 / 64.0) * radius).collect()
                    }
                    EdgeCurve::Arc { a, mid, b } => arc_points(flat(a), flat(mid), flat(b)),
                    EdgeCurve::Other { points } => points.into_iter().map(flat).collect(),
                };
                segs.extend(pts.windows(2).map(|w| (w[0], w[1])));
            }
        }
    }
    Ok(segs)
}

/// Points along the circle from `a` through `m` to `b` (straight if they're in line).
fn arc_points(a: DVec2, m: DVec2, b: DVec2) -> Vec<DVec2> {
    let d = 2.0 * (a.x * (m.y - b.y) + m.x * (b.y - a.y) + b.x * (a.y - m.y));
    if d.abs() < 1e-12 {
        return vec![a, b];
    }
    let (a2, m2, b2) = (a.length_squared(), m.length_squared(), b.length_squared());
    let c = DVec2::new(
        (a2 * (m.y - b.y) + m2 * (b.y - a.y) + b2 * (a.y - m.y)) / d,
        (a2 * (b.x - m.x) + m2 * (a.x - b.x) + b2 * (m.x - a.x)) / d,
    );
    let ang = |p: DVec2| (p - c).to_angle();
    let (ta, tm, tb) = (ang(a), ang(m), ang(b));
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(std::f64::consts::TAU);
    let sweep = if ccw(ta, tm) <= ccw(ta, tb) { ccw(ta, tb) } else { ccw(ta, tb) - std::f64::consts::TAU };
    let r = c.distance(a);
    (0..=32).map(|i| c + DVec2::from_angle(ta + sweep * i as f64 / 32.0) * r).collect()
}

/// 45° hatching inside closed outlines (even-odd), `gap` apart.
pub fn hatch(segs: &[(DVec2, DVec2)], gap: f64) -> Vec<(DVec2, DVec2)> {
    if segs.is_empty() || !(gap.is_finite() && gap > 0.0) {
        return Vec::new();
    }
    // Rotate so the hatch runs along x, scan in y.
    let (c, s) = (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
    let rot = |p: DVec2| DVec2::new(p.x * c + p.y * s, -p.x * s + p.y * c);
    let back = |p: DVec2| DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c);
    let r: Vec<(DVec2, DVec2)> = segs.iter().map(|(a, b)| (rot(*a), rot(*b))).collect();
    let (lo, hi) = r.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (a, b)| (lo.min(a.y).min(b.y), hi.max(a.y).max(b.y)));
    let mut out = Vec::new();
    let mut y = (lo / gap).floor() * gap + gap * 0.5;
    let mut rows = 0;
    while y < hi && rows < 2000 {
        let mut xs: Vec<f64> =
            r.iter().filter(|(a, b)| (a.y <= y) != (b.y <= y)).map(|(a, b)| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x)).collect();
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            out.push((back(DVec2::new(pair[0], y)), back(DVec2::new(pair[1], y))));
        }
        y += gap;
        rows += 1;
    }
    out
}

/// The parts of some lines inside a circle.
pub fn clip_to_circle(lines: &[ViewLine], c: DVec2, r: f64) -> Vec<ViewLine> {
    let mut out = Vec::new();
    for l in lines {
        let mut run: Vec<DVec2> = Vec::new();
        for w in l.points.windows(2) {
            let (p, q) = (w[0], w[1]);
            let d = q - p;
            let (aa, bb, cc) = (d.length_squared(), 2.0 * d.dot(p - c), (p - c).length_squared() - r * r);
            let disc = bb * bb - 4.0 * aa * cc;
            if aa < 1e-18 || disc <= 0.0 {
                if !run.is_empty() {
                    out.push(ViewLine { points: std::mem::take(&mut run), hidden: l.hidden });
                }
                continue;
            }
            let sq = disc.sqrt();
            let (t0, t1) = (((-bb - sq) / (2.0 * aa)).max(0.0), ((-bb + sq) / (2.0 * aa)).min(1.0));
            if t0 >= t1 {
                if !run.is_empty() {
                    out.push(ViewLine { points: std::mem::take(&mut run), hidden: l.hidden });
                }
                continue;
            }
            if run.is_empty() || t0 > 0.0 {
                if !run.is_empty() {
                    out.push(ViewLine { points: std::mem::take(&mut run), hidden: l.hidden });
                }
                run.push(p + d * t0);
            }
            run.push(p + d * t1);
            if t1 < 1.0 {
                out.push(ViewLine { points: std::mem::take(&mut run), hidden: l.hidden });
            }
        }
        if run.len() >= 2 {
            out.push(ViewLine { points: run, hidden: l.hidden });
        }
    }
    out.retain(|l| l.points.len() >= 2);
    out
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A line or text in sheet millimeters, for the writers and the on-screen sheet alike.
#[derive(Clone, Debug)]
pub enum Mark {
    Line {
        a: DVec2,
        b: DVec2,
        width: f64,
        dashed: bool,
    },
    Text {
        at: DVec2,
        text: String,
        size: f64,
        mono: bool,
        center: bool,
    },
    /// A picture filling `min`..`max` (sheet mm).
    Image {
        min: DVec2,
        max: DVec2,
        image: u64,
    },
}

impl Drawing {
    /// Zone count across and down (about 50 mm each, as on standard frames).
    pub fn zones(&self) -> (usize, usize) {
        let inner = self.sheet - DVec2::splat(2.0 * MARGIN);
        (((inner.x / 50.0).round() as usize).max(2), ((inner.y / 50.0).round() as usize).max(2))
    }

    /// The frame, zone markers and title block (everything but the views).
    pub fn frame_marks(&self) -> Vec<Mark> {
        let mut out = Vec::new();
        let line = |a: DVec2, b: DVec2, width: f64| Mark::Line { a, b, width, dashed: false };
        let o = DVec2::splat(MARGIN);
        let s = self.sheet - DVec2::splat(MARGIN);
        let i0 = DVec2::splat(MARGIN + ZONE);
        let i1 = self.sheet - DVec2::splat(MARGIN + ZONE);
        if self.style.title_block == "Empty Sheet" {
            return out;
        }
        // No outer border: the zones are marked by ticks outside the frame, and centering marks cross it.
        let c = self.sheet / 2.0;
        for (a, b) in [
            ((c.x, o.y), (c.x, i0.y + 3.0)),
            ((c.x, s.y), (c.x, i1.y - 3.0)),
            ((o.x, c.y), (i0.x + 3.0, c.y)),
            ((s.x, c.y), (i1.x - 3.0, c.y)),
        ] {
            out.push(line(DVec2::new(a.0, a.1), DVec2::new(b.0, b.1), 0.35));
        }
        for (a, b) in
            [((i0.x, i0.y), (i1.x, i0.y)), ((i1.x, i0.y), (i1.x, i1.y)), ((i1.x, i1.y), (i0.x, i1.y)), ((i0.x, i1.y), (i0.x, i0.y))]
        {
            out.push(line(DVec2::new(a.0, a.1), DVec2::new(b.0, b.1), 0.35));
        }
        let (nx, ny) = self.zones();
        let zw = (s.x - o.x) / nx as f64;
        let zh = (s.y - o.y) / ny as f64;
        for k in 0..nx {
            let x = o.x + zw * (k as f64 + 0.5);
            for y in [o.y + ZONE / 2.0, s.y - ZONE / 2.0] {
                out.push(Mark::Text { at: DVec2::new(x, y), text: (k + 1).to_string(), size: 3.5, mono: false, center: true });
            }
            if k > 0 {
                let xe = o.x + zw * k as f64;
                out.push(line(DVec2::new(xe, o.y), DVec2::new(xe, o.y + ZONE), 0.25));
                out.push(line(DVec2::new(xe, s.y), DVec2::new(xe, s.y - ZONE), 0.25));
            }
        }
        for k in 0..ny {
            // Letters run from the top down.
            let y = s.y - zh * (k as f64 + 0.5);
            let letter = char::from(b'A' + (k as u8).min(25)).to_string();
            for x in [o.x + ZONE / 2.0, s.x - ZONE / 2.0] {
                out.push(Mark::Text { at: DVec2::new(x, y), text: letter.clone(), size: 3.5, mono: false, center: true });
            }
            if k > 0 {
                let ye = s.y - zh * k as f64;
                out.push(line(DVec2::new(o.x, ye), DVec2::new(o.x + ZONE, ye), 0.25));
                out.push(line(DVec2::new(s.x, ye), DVec2::new(s.x - ZONE, ye), 0.25));
            }
        }
        self.title_block(&mut out, i0, i1);
        out
    }

    /// The title block in the layout the drawing asks for (observed s14-120..tb-*), anchored in the frame's
    /// bottom-right corner. Empty fields show a "---" placeholder.
    fn title_block(&self, out: &mut Vec<Mark>, i0: DVec2, i1: DVec2) {
        let w = i1.x - i0.x;
        let date = format_date(self.date, &self.style.date_format);
        let proj = if self.first_angle { "\u{1}first" } else { "\u{1}third" };
        let sheet = "1/1".to_string();
        // (x0, y0, x1, y1, label, value); a label of "" is an empty field.
        let mut cells: Vec<(f64, f64, f64, f64, &str, String)> = Vec::new();
        let x1 = i1.x;
        let y0 = i0.y;
        match self.style.title_block.as_str() {
            "Empty Sheet" | "Border Only" => {}
            "Horizontal" => {
                let r = 12.0;
                let mut x = i0.x;
                let cols: [(f64, &str, String); 8] = [
                    (0.184, "TITLE", self.title.clone()),
                    (0.184, "", String::new()),
                    (0.344, "", String::new()),
                    (0.05, "UNITS", self.units.clone()),
                    (0.051, "PROJ. ANG.", proj.into()),
                    (0.051, "SIZE", self.size.clone()),
                    (0.051, "SCALE", scale_label(self.scale)),
                    (0.085, "LAST UPDATED", date.clone()),
                ];
                for (f, label, value) in cols {
                    let nx = (x + w * f).min(x1);
                    cells.push((x, y0, nx, y0 + r, label, value));
                    x = nx;
                }
            }
            "Vertical" => {
                let cw = (w * 0.158).max(50.0);
                let x0 = x1 - cw;
                let xm = x0 + cw * 0.6;
                let r = 12.0;
                let rows = [
                    (0.0, "LAST UPDATED", date.clone(), "SHEET", sheet.clone()),
                    (r, "PROJ. ANG.", proj.into(), "SIZE", self.size.clone()),
                    (2.0 * r, "UNITS", self.units.clone(), "SCALE", scale_label(self.scale)),
                ];
                for (dy, l1, v1, l2, v2) in rows {
                    cells.push((x0, y0 + dy, xm, y0 + dy + r, l1, v1));
                    cells.push((xm, y0 + dy, x1, y0 + dy + r, l2, v2));
                }
                let mut y = y0 + 3.0 * r;
                cells.push((x0, y, x1, y + 9.0, "", String::new()));
                y += 9.0;
                revision_table(&mut cells, x0, y, x1, 5);
                y += 6.0 + 5.0 * 7.0;
                let top = i1.y - 2.0 * r;
                let mid = (y + top) / 2.0;
                cells.push((x0, y, x1, mid, "", String::new()));
                cells.push((x0, mid, x1, top, "", String::new()));
                cells.push((x0, top, x1, top + r, "", String::new()));
                cells.push((x0, top + r, x1, i1.y, "TITLE", self.title.clone()));
            }
            "Block" | "Block with Table" => {
                let r = 11.5;
                let cw = (w * 0.184).max(60.0);
                let x0 = x1 - cw;
                let third = cw / 3.0;
                cells.push((x0, y0, x0 + third, y0 + r, "SCALE", scale_label(self.scale)));
                cells.push((x0 + third, y0, x1 - third * 0.6, y0 + r, "LAST UPDATED", date.clone()));
                cells.push((x1 - third * 0.6, y0, x1, y0 + r, "SHEET", sheet.clone()));
                cells.push((x0, y0 + r, x0 + third, y0 + 2.0 * r, "UNITS", self.units.clone()));
                cells.push((x0 + third, y0 + r, x1 - third * 0.6, y0 + 2.0 * r, "PROJ. ANG.", proj.into()));
                cells.push((x1 - third * 0.6, y0 + r, x1, y0 + 2.0 * r, "SIZE", self.size.clone()));
                cells.push((x0, y0 + 2.0 * r, x1, y0 + 3.0 * r, "", String::new()));
                cells.push((x0, y0 + 3.0 * r, x1, y0 + 4.0 * r, "TITLE", self.title.clone()));
                cells.push((x0, y0 + 4.0 * r, x1, y0 + 5.0 * r, "", String::new()));
                let tw = (w * 0.138).max(45.0);
                let t0 = x0 - tw;
                cells.push((t0, y0, x0, y0 + r, "", String::new()));
                revision_table(&mut cells, t0, y0 + r, x0, 5);
                if self.style.title_block == "Block with Table" {
                    let nw = w * 0.165;
                    let n0 = t0 - nw;
                    cells.push((n0, y0, t0, y0 + 2.5 * r, "", String::new()));
                    cells.push((n0, y0 + 2.5 * r, t0, y0 + 5.0 * r, "", String::new()));
                    // A parts table across the rest: an empty band on top, then six rows of five columns.
                    let px0 = i0.x;
                    cells.push((px0, y0 + 4.0 * r, n0, y0 + 5.0 * r, "", String::new()));
                    let rh = 4.0 * r / 6.0;
                    let fr = [0.0, 0.1, 0.2, 0.76, 0.88, 1.0];
                    for k in 0..6 {
                        let y = y0 + rh * k as f64;
                        for c in 0..5 {
                            cells.push((px0 + (n0 - px0) * fr[c], y, px0 + (n0 - px0) * fr[c + 1], y + rh, "", String::new()));
                        }
                    }
                }
            }
            // Simple (the default): TITLE across the top; UNITS | PROJ. ANG. and SCALE | LAST UPDATED below.
            _ => {
                let x0 = x1 - TITLE.x;
                let mid = x0 + TITLE.x / 2.0;
                let r = TITLE.y / 3.0;
                cells.push((x0, y0 + 2.0 * r, x1, y0 + 3.0 * r, "TITLE", self.title.clone()));
                cells.push((x0, y0 + r, mid, y0 + 2.0 * r, "UNITS", self.units.clone()));
                cells.push((mid, y0 + r, x1, y0 + 2.0 * r, "PROJ. ANG.", proj.into()));
                cells.push((x0, y0, mid, y0 + r, "SCALE", scale_label(self.scale)));
                cells.push((mid, y0, x1, y0 + r, "LAST UPDATED", date.clone()));
            }
        }
        let line = |a: DVec2, b: DVec2, width: f64| Mark::Line { a, b, width, dashed: false };
        for (x0, y0, x1, y1, label, value) in cells {
            let (a, b) = (DVec2::new(x0, y0), DVec2::new(x1, y1));
            for (p, q) in [((a.x, a.y), (b.x, a.y)), ((b.x, a.y), (b.x, b.y)), ((b.x, b.y), (a.x, b.y)), ((a.x, b.y), (a.x, a.y))] {
                out.push(line(DVec2::new(p.0, p.1), DVec2::new(q.0, q.1), 0.25));
            }
            let tall = (y1 - y0) >= 10.0;
            let label_size = if tall { 2.0 } else { 1.6 };
            let shown = if label.is_empty() { "---" } else { label };
            out.push(Mark::Text {
                at: DVec2::new(x0 + 2.0, y1 - 2.5),
                text: shown.to_string(),
                size: label_size,
                mono: true,
                center: false,
            });
            if let Some(kind) = value.strip_prefix('\u{1}') {
                proj_symbol(out, DVec2::new(x0 + 9.0, y0 + (y1 - y0) * 0.35), (y1 - y0) * 0.32, kind == "first");
            } else if !value.is_empty() {
                out.push(Mark::Text {
                    at: DVec2::new(x0 + 2.0, y1 - 8.0),
                    text: value,
                    size: if tall { 4.5 } else { 3.0 },
                    mono: true,
                    center: false,
                });
            } else if label.is_empty() && x1 - x0 < 80.0 && tall {
                out.push(Mark::Text { at: DVec2::new(x0 + 2.0, y1 - 7.0), text: "----".into(), size: 2.6, mono: true, center: false });
            }
        }
    }

    /// A point of a view (its model millimeters) on the sheet.
    pub fn view_to_sheet(&self, view: &str, p: DVec2) -> Option<DVec2> {
        let v = self.views.iter().find(|v| v.name == view)?;
        Some(v.at + (p - v.mid) * v.scale)
    }

    /// A view's scale (the sheet's if there's no such view).
    pub fn view_scale(&self, view: &str) -> f64 {
        self.views.iter().find(|v| v.name == view).map_or(self.scale, |v| v.scale)
    }

    /// The view whose extent holds a sheet point (the smallest, so a detail inside another wins).
    pub fn view_at(&self, q: DVec2) -> Option<&DrawingView> {
        self.views
            .iter()
            .filter(|v| q.cmpge(v.lo - DVec2::splat(3.0)).all() && q.cmple(v.hi + DVec2::splat(3.0)).all())
            .min_by(|a, b| ((a.hi - a.lo).x * (a.hi - a.lo).y).total_cmp(&((b.hi - b.lo).x * (b.hi - b.lo).y)))
    }

    /// The view whose lines come nearest a sheet point, and the point in that view's model millimeters.
    pub fn sheet_to_view(&self, q: DVec2) -> Option<(&str, DVec2)> {
        let v = self.views.iter().min_by(|a, b| {
            let d =
                |v: &DrawingView| v.lines.iter().flat_map(|l| l.points.iter()).map(|p| p.distance_squared(q)).fold(f64::INFINITY, f64::min);
            d(a).total_cmp(&d(b))
        })?;
        Some((&v.name, v.mid + (q - v.at) / v.scale))
    }

    /// A dimension value in display units, written with the drawing's precision and decimal separator.
    pub fn format_value(&self, mm: f64) -> String {
        let s = format!("{:.*}", self.style.length_precision as usize, mm / self.unit_scale);
        if self.style.decimal_separator == "Comma" { s.replace('.', ",") } else { s }
    }

    /// An angle in degrees, decimal or degrees-minutes-seconds as the drawing asks.
    pub fn format_angle(&self, deg: f64) -> String {
        let sep = |s: String| if self.style.decimal_separator == "Comma" { s.replace('.', ",") } else { s };
        if self.style.angle_format.starts_with("Degrees") {
            let total = (deg * 3600.0).round() as i64;
            return format!("{}°{:02}′{:02}″", total / 3600, (total / 60) % 60, total % 60);
        }
        sep(format!("{:.*}°", self.style.angle_precision as usize, deg))
    }

    /// A tolerance amount: the drawing's precision with trailing zeros dropped ("0,1", "0").
    fn format_tolerance(&self, v: f64) -> String {
        let s = format!("{:.*}", self.style.length_precision.max(1) as usize + 1, v.abs());
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if self.style.decimal_separator == "Comma" { s.replace('.', ",") } else { s }
    }

    /// A dimension's value with its prefix, suffix and tolerance (as observed s14-132: tolerances small after the
    /// value, a deviation stacked +upper over −lower). `number` is the value in display units, `shown` the value as
    /// written; `center` centers the whole label on `at`, otherwise it starts there. A centered label beside a
    /// steep dimension line moves `away` from it by half its width, so the text stays clear of the line.
    fn value_marks(
        &self,
        out: &mut Vec<Mark>,
        at: DVec2,
        number: f64,
        shown: String,
        text: &shapecraft_doc::DimText,
        center: bool,
        away: DVec2,
    ) {
        use shapecraft_doc::ToleranceKind as K;
        const SIZE: f64 = 3.0;
        const SMALL: f64 = 1.9;
        let width = |t: &str, size: f64| t.chars().count() as f64 * size * 0.62;
        let prefix = text.prefix.clone().unwrap_or_default();
        let suffix = text.suffix.clone().unwrap_or_default();
        let tol = text.tolerance.as_ref();
        // The main text, and what goes after it (small, stacked or not).
        let (main, after): (String, Vec<String>) = match tol.map(|t| (t.kind, t.upper, t.lower)) {
            Some((K::Symmetrical, u, _)) => (format!("{prefix}{shown}"), vec![format!("±{}", self.format_tolerance(u))]),
            Some((K::Deviation, u, l)) => {
                (format!("{prefix}{shown}"), vec![format!("+{}", self.format_tolerance(u)), format!("−{}", self.format_tolerance(l))])
            }
            Some((K::Limits, u, l)) => {
                let hi = self.format_value((number + u.abs()) * self.unit_scale);
                let lo = self.format_value((number - l.abs()) * self.unit_scale);
                (String::new(), vec![format!("{prefix}{hi}{suffix}"), format!("{prefix}{lo}{suffix}")])
            }
            _ => (format!("{prefix}{shown}"), Vec::new()),
        };
        // Text that isn't centred hangs from its bottom-left corner: `base` is the main text's bottom.
        let base = if center { at.y - SIZE * 0.6 } else { at.y };
        let limits = matches!(tol.map(|t| t.kind), Some(K::Limits));
        if limits {
            let w = after.iter().map(|t| width(t, 2.4)).fold(0.0, f64::max);
            let at = at + DVec2::new(away.x * (w / 2.0), 0.0);
            let x = if center { at.x - w / 2.0 } else { at.x };
            out.push(Mark::Text { at: DVec2::new(x, base + 1.9), text: after[0].clone(), size: 2.4, mono: false, center: false });
            out.push(Mark::Text { at: DVec2::new(x, base - 1.3), text: after[1].clone(), size: 2.4, mono: false, center: false });
            return;
        }
        let main_w = width(&main, SIZE);
        let tail_w = after.iter().map(|t| width(t, SMALL)).fold(0.0, f64::max);
        let suffix_w = width(&suffix, SIZE);
        let total = main_w + tail_w + suffix_w;
        let at = at + DVec2::new(away.x * (total / 2.0), 0.0);
        let x0 = if center { at.x - total / 2.0 } else { at.x };
        out.push(Mark::Text { at: DVec2::new(x0, base), text: main.clone(), size: SIZE, mono: false, center: false });
        let tx = x0 + main_w;
        match after.len() {
            1 => out.push(Mark::Text { at: DVec2::new(tx, base + 1.4), text: after[0].clone(), size: SMALL, mono: false, center: false }),
            2 => {
                out.push(Mark::Text { at: DVec2::new(tx, base + 2.0), text: after[0].clone(), size: SMALL, mono: false, center: false });
                out.push(Mark::Text { at: DVec2::new(tx, base - 0.3), text: after[1].clone(), size: SMALL, mono: false, center: false });
            }
            _ => {}
        }
        if !suffix.is_empty() {
            out.push(Mark::Text { at: DVec2::new(tx + tail_w, base), text: suffix, size: SIZE, mono: false, center: false });
        }
        // Basic: the value boxed.
        if matches!(tol.map(|t| t.kind), Some(K::Basic)) {
            let (a, b) = (DVec2::new(x0 - 0.8, base - 0.7), DVec2::new(x0 + main_w + 0.8, base + SIZE * 1.3));
            let w = self.style.dimension;
            for (p, q) in [((a.x, a.y), (b.x, a.y)), ((b.x, a.y), (b.x, b.y)), ((b.x, b.y), (a.x, b.y)), ((a.x, b.y), (a.x, a.y))] {
                out.push(Mark::Line { a: DVec2::new(p.0, p.1), b: DVec2::new(q.0, q.1), width: w, dashed: false });
            }
        }
    }

    /// The marks of the annotations: dimension lines with arrows and values, leaders, centerlines, notes.
    pub fn annotation_marks(&self) -> Vec<Mark> {
        self.annotations.iter().flat_map(|a| self.marks_for(a)).collect()
    }

    /// A sheet point in a named view's model millimeters.
    pub fn sheet_to_named_view(&self, view: &str, q: DVec2) -> Option<DVec2> {
        let v = self.views.iter().find(|v| v.name == view)?;
        Some(v.mid + (q - v.at) / v.scale)
    }

    /// One annotation's marks.
    pub fn marks_for(&self, a: &shapecraft_doc::Annotation) -> Vec<Mark> {
        use shapecraft_doc::Annotation as A;
        let mut out = Vec::new();
        let w = self.style.dimension;
        let line = |a: DVec2, b: DVec2, width: f64, dashed: bool| Mark::Line { a, b, width, dashed };
        let arrow = |out: &mut Vec<Mark>, tip: DVec2, dir: DVec2| {
            let n = dir.perp();
            out.push(line(tip, tip - dir * 2.5 + n * 0.9, w, false));
            out.push(line(tip, tip - dir * 2.5 - n * 0.9, w, false));
        };
        {
            match a {
                A::Distance { view, a, b, offset, text } => {
                    let (Some(pa), Some(pb)) = (self.view_to_sheet(view, *a), self.view_to_sheet(view, *b)) else { return out };
                    let dir = (pb - pa).normalize_or_zero();
                    if dir == DVec2::ZERO {
                        return out;
                    }
                    let n = dir.perp();
                    let (da, db) = (pa + n * *offset, pb + n * *offset);
                    let ext = n * offset.signum() * 1.5;
                    out.push(line(pa + n * offset.signum() * 1.0, da + ext, w, false));
                    out.push(line(pb + n * offset.signum() * 1.0, db + ext, w, false));
                    out.push(line(da, db, w, false));
                    arrow(&mut out, da, -dir);
                    arrow(&mut out, db, dir);
                    let up = if n.y < 0.0 || (n.y == 0.0 && n.x > 0.0) { -n } else { n };
                    let v = a.distance(*b);
                    // Beside a steep line the value goes outward, away from the part.
                    let side = if up.x.abs() > 0.7 { n * offset.signum() } else { up };
                    self.value_marks(&mut out, (da + db) / 2.0 + side * 2.2, v / self.unit_scale, self.format_value(v), text, true, side);
                }
                A::Radius { view, center, radius, angle, diameter, text } => {
                    let Some(c) = self.view_to_sheet(view, *center) else { return out };
                    let dir = DVec2::from_angle(*angle);
                    let r = radius * self.view_scale(view);
                    let rim = c + dir * r;
                    let start = if *diameter { c - dir * r } else { c };
                    let end = rim + dir * 8.0;
                    out.push(line(start, end, w, false));
                    arrow(&mut out, rim, dir);
                    if *diameter {
                        arrow(&mut out, c - dir * r, -dir);
                    }
                    let value = if *diameter { radius * 2.0 } else { *radius };
                    let label = format!("{}{}", if *diameter { "Ø" } else { "R" }, self.format_value(value));
                    self.value_marks(
                        &mut out,
                        end + dir * 1.5 + DVec2::new(0.0, -1.0),
                        value / self.unit_scale,
                        label,
                        text,
                        false,
                        DVec2::ZERO,
                    );
                }
                A::Angle { view, vertex, a, b, radius, text } => {
                    let (Some(v), Some(pa), Some(pb)) =
                        (self.view_to_sheet(view, *vertex), self.view_to_sheet(view, *a), self.view_to_sheet(view, *b))
                    else {
                        return out;
                    };
                    let (ta, tb) = ((pa - v).to_angle(), (pb - v).to_angle());
                    // Sweep the smaller way round, from a to b.
                    let mut sweep = tb - ta;
                    while sweep > std::f64::consts::PI {
                        sweep -= std::f64::consts::TAU;
                    }
                    while sweep < -std::f64::consts::PI {
                        sweep += std::f64::consts::TAU;
                    }
                    let n = 24;
                    let at = |t: f64| v + DVec2::from_angle(ta + sweep * t) * *radius;
                    for i in 0..n {
                        out.push(line(at(i as f64 / n as f64), at((i + 1) as f64 / n as f64), w, false));
                    }
                    let tangent = |t: f64| DVec2::from_angle(ta + sweep * t).perp() * sweep.signum();
                    arrow(&mut out, at(0.0), -tangent(0.0));
                    arrow(&mut out, at(1.0), tangent(1.0));
                    // Extension lines out to the arc when the picked points sit inside it.
                    for (p, t) in [(pa, ta), (pb, tb)] {
                        if p.distance(v) < *radius {
                            out.push(line(p, v + DVec2::from_angle(t) * (*radius + 1.5), w, false));
                        }
                    }
                    let mid = v + DVec2::from_angle(ta + sweep / 2.0) * (*radius + 3.5);
                    let deg = sweep.abs().to_degrees();
                    self.value_marks(&mut out, mid, deg, self.format_angle(deg), text, true, DVec2::ZERO);
                }
                A::Centerline { view, a, b } => {
                    let (Some(pa), Some(pb)) = (self.view_to_sheet(view, *a), self.view_to_sheet(view, *b)) else { return out };
                    let dir = (pb - pa).normalize_or_zero();
                    out.push(line(pa - dir * 3.0, pb + dir * 3.0, self.style.centerline, true));
                }
                A::CircularCenterline { view, center, radius } => {
                    let Some(c) = self.view_to_sheet(view, *center) else { return out };
                    let r = radius * self.view_scale(view);
                    let n = 72;
                    for i in 0..n {
                        let (t0, t1) = (std::f64::consts::TAU * i as f64 / n as f64, std::f64::consts::TAU * (i + 1) as f64 / n as f64);
                        out.push(line(c + DVec2::from_angle(t0) * r, c + DVec2::from_angle(t1) * r, self.style.centerline, i % 3 == 2));
                    }
                }
                A::IntersectionMark { view, at } => {
                    let Some(c) = self.view_to_sheet(view, *at) else { return out };
                    let s = 1.5;
                    out.push(line(c + DVec2::new(-s, -s), c + DVec2::new(s, s), w, false));
                    out.push(line(c + DVec2::new(-s, s), c + DVec2::new(s, -s), w, false));
                }
                A::CenterMark { view, center, radius } => {
                    let Some(c) = self.view_to_sheet(view, *center) else { return out };
                    let r = radius * self.view_scale(view) + 2.5;
                    out.push(line(c - DVec2::X * r, c + DVec2::X * r, self.style.centerline, true));
                    out.push(line(c - DVec2::Y * r, c + DVec2::Y * r, self.style.centerline, true));
                }
                A::Note { at, text } => {
                    for (i, row) in text.lines().enumerate() {
                        out.push(Mark::Text {
                            at: *at - DVec2::new(0.0, 5.0 * i as f64),
                            text: row.to_string(),
                            size: 3.5,
                            mono: false,
                            center: false,
                        });
                    }
                }
                A::Image { at, width, image } => {
                    if let Some(pic) = self.pictures.iter().find(|p| p.id == *image) {
                        let size = DVec2::new(*width, width / pic.aspect().max(1e-3));
                        out.push(Mark::Image { min: *at - size / 2.0, max: *at + size / 2.0, image: image.0 });
                    }
                }
            }
        }
        out
    }

    /// Every mark on the sheet: the views (hidden lines first, so visible ones sit on top) and the frame.
    pub fn marks(&self) -> Vec<Mark> {
        let mut out = Vec::new();
        for v in &self.views {
            for hidden in [true, false] {
                for l in v.lines.iter().filter(|l| l.hidden == hidden) {
                    for w in l.points.windows(2) {
                        out.push(Mark::Line {
                            a: w[0],
                            b: w[1],
                            width: if hidden { self.style.hidden } else { self.style.visible },
                            dashed: hidden,
                        });
                    }
                }
            }
            out.extend(v.extra.iter().cloned());
        }
        out.extend(self.frame_marks());
        out.extend(self.annotation_marks());
        out
    }

    pub fn to_svg(&self) -> String {
        let (w, h) = (self.sheet.x, self.sheet.y);
        let y = |v: f64| h - v;
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\n"
        );
        for m in self.marks() {
            match m {
                Mark::Line { a, b, width, dashed } => {
                    let dash = if dashed { " stroke-dasharray=\"1.5 1\"" } else { "" };
                    out += &format!(
                        "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"black\" stroke-width=\"{width}\"{dash} stroke-linecap=\"round\"/>\n",
                        a.x,
                        y(a.y),
                        b.x,
                        y(b.y)
                    );
                }
                Mark::Text { at, text, size, mono, center } => {
                    let family = if mono { "Menlo, Consolas, monospace" } else { "Helvetica, Arial, sans-serif" };
                    let anchor = if center { " text-anchor=\"middle\" dominant-baseline=\"middle\"" } else { "" };
                    out += &format!(
                        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{family}\" font-size=\"{size}\"{anchor}>{}</text>\n",
                        at.x,
                        y(at.y),
                        esc(&text)
                    );
                }
                Mark::Image { min, max, image } => {
                    if let Some(pic) = self.pictures.iter().find(|p| p.id.0 == image) {
                        let mime = if pic.ext == "png" { "image/png" } else { "image/jpeg" };
                        out += &format!(
                            "<image x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" preserveAspectRatio=\"none\" href=\"data:{mime};base64,{}\"/>\n",
                            min.x,
                            y(max.y),
                            max.x - min.x,
                            max.y - min.y,
                            base64(&pic.file)
                        );
                    }
                }
            }
        }
        out += "</svg>\n";
        out
    }

    /// An ASCII DXF (R12 entities) with VISIBLE, HIDDEN and FRAME layers, in sheet millimeters.
    pub fn to_dxf(&self) -> String {
        let mut out =
            String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLTYPE\n70\n2\n");
        out += "0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid\n72\n65\n73\n0\n40\n0.0\n";
        out += "0\nLTYPE\n2\nDASHED\n70\n0\n3\n__ __ __\n72\n65\n73\n2\n40\n2.5\n49\n1.5\n49\n-1.0\n0\nENDTAB\n";
        out += "0\nTABLE\n2\nLAYER\n70\n3\n0\nLAYER\n2\nVISIBLE\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nLAYER\n2\nHIDDEN\n70\n0\n62\n8\n6\nDASHED\n0\nLAYER\n2\nFRAME\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n";
        out += "0\nSECTION\n2\nENTITIES\n";
        let views = self.views.iter().flat_map(|v| v.lines.iter()).flat_map(|l| {
            let layer = if l.hidden { "HIDDEN" } else { "VISIBLE" };
            l.points.windows(2).map(move |w| (layer, w[0], w[1]))
        });
        let frame = self
            .frame_marks()
            .into_iter()
            .chain(self.views.iter().flat_map(|v| v.extra.iter().cloned()))
            .chain(self.annotation_marks())
            .filter_map(|m| match m {
                Mark::Line { a, b, .. } => Some(("FRAME", a, b)),
                Mark::Text { .. } | Mark::Image { .. } => None,
            });
        for (layer, a, b) in views.chain(frame) {
            out += &format!("0\nLINE\n8\n{layer}\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n", a.x, a.y, b.x, b.y);
        }
        for m in
            self.frame_marks().into_iter().chain(self.views.iter().flat_map(|v| v.extra.iter().cloned())).chain(self.annotation_marks())
        {
            if let Mark::Text { at, text, size, center, .. } = m {
                let clean: String = text.chars().filter(|c| c.is_ascii() && *c != '\n').collect();
                let (h_align, v_align) = if center { (1, 2) } else { (0, 0) };
                out += &format!(
                    "0\nTEXT\n8\nFRAME\n10\n{:.4}\n20\n{:.4}\n30\n0.0\n40\n{size}\n1\n{clean}\n72\n{h_align}\n11\n{:.4}\n21\n{:.4}\n31\n0.0\n73\n{v_align}\n",
                    at.x, at.y, at.x, at.y
                );
            }
        }
        out += "0\nENDSEC\n0\nEOF\n";
        out
    }

    /// The sheet as an AutoCAD R2000 DWG (crate `shapecraft-dwg`), in sheet millimetres like the DXF: view
    /// lines as polylines on VISIBLE and HIDDEN (dashed), the frame, title block, view extras and annotations
    /// as lines and text on FRAME, each line with its drawn width as its lineweight. Pictures are left out.
    pub fn to_dwg(&self) -> Result<Vec<u8>> {
        use shapecraft_dwg as dwg;
        let ok = |p: DVec2| p.is_finite();
        let mut d = dwg::Drawing {
            layers: vec![
                dwg::Layer { name: "VISIBLE".into(), color: 7, lineweight: dwg::LineWeight::Mm(self.style.visible), linetype: None },
                dwg::Layer {
                    name: "HIDDEN".into(),
                    color: 8,
                    lineweight: dwg::LineWeight::Mm(self.style.hidden),
                    linetype: Some("DASHED".into()),
                },
                dwg::Layer::new("FRAME", 7),
            ],
            linetypes: vec![dwg::Linetype { name: "DASHED".into(), description: "__ __ __".into(), dashes: vec![1.5, -1.0] }],
            entities: Vec::new(),
            limits: Some(([0.0, 0.0], [self.sheet.x, self.sheet.y])),
            date: Some(self.date),
        };
        for l in self.views.iter().flat_map(|v| v.lines.iter()) {
            let pts: Vec<[f64; 2]> = l.points.iter().filter(|p| ok(**p)).map(|p| [p.x, p.y]).collect();
            let layer = if l.hidden { "HIDDEN" } else { "VISIBLE" };
            let shape = match pts.len() {
                0 | 1 => continue,
                2 => dwg::Shape::Line { a: [pts[0][0], pts[0][1], 0.0], b: [pts[1][0], pts[1][1], 0.0] },
                _ => dwg::Shape::Polyline { points: pts, bulges: Vec::new(), closed: false },
            };
            d.entities.push(dwg::Entity::new(layer, shape));
        }
        let marks =
            self.frame_marks().into_iter().chain(self.views.iter().flat_map(|v| v.extra.iter().cloned())).chain(self.annotation_marks());
        for m in marks {
            match m {
                Mark::Line { a, b, width, dashed } if ok(a) && ok(b) => {
                    let mut e = dwg::Entity::new("FRAME", dwg::Shape::Line { a: [a.x, a.y, 0.0], b: [b.x, b.y, 0.0] });
                    if width.is_finite() && width >= 0.0 {
                        e.lineweight = dwg::LineWeight::Mm(width);
                    }
                    if dashed {
                        e.linetype = Some("DASHED".into());
                    }
                    d.entities.push(e);
                }
                Mark::Text { at, text, size, center, .. } if ok(at) && size.is_finite() && size > 0.0 => {
                    let (h_align, v_align) =
                        if center { (dwg::HAlign::Center, dwg::VAlign::Middle) } else { (dwg::HAlign::Left, dwg::VAlign::Baseline) };
                    let value: String = text.chars().filter(|c| *c != '\n').take(1000).collect();
                    d.entities.push(dwg::Entity::new(
                        "FRAME",
                        dwg::Shape::Text { at: [at.x, at.y, 0.0], height: size, rotation: 0.0, value, h_align, v_align },
                    ));
                }
                _ => {}
            }
        }
        dwg::write(&d).map_err(|e| EngineError::BadParams(e.to_string()))
    }

    /// A one-page vector PDF (1.4, uncompressed), lines in points, text in Helvetica/Courier.
    pub fn to_pdf(&self) -> Vec<u8> {
        let pt = 72.0 / 25.4;
        let mut content = String::new();
        content += "1 J 1 j 0 0 0 RG 0 0 0 rg\n";
        // Pictures: each one an RGB image with its alpha as a soft mask, objects 7 and on.
        // (object number, RGB bytes, alpha bytes, width, height)
        type PdfImage = (u64, Vec<u8>, Vec<u8>, u32, u32);
        let mut images: Vec<PdfImage> = Vec::new();
        for m in self.marks() {
            match m {
                Mark::Line { a, b, width, dashed } => {
                    content += &format!(
                        "{:.3} w {} {:.3} {:.3} m {:.3} {:.3} l S\n",
                        width * pt,
                        if dashed { "[4 3] 0 d" } else { "[] 0 d" },
                        a.x * pt,
                        a.y * pt,
                        b.x * pt,
                        b.y * pt
                    );
                }
                Mark::Text { at, text, size, mono, center } => {
                    let font = if mono { "/F2" } else { "/F1" };
                    let clean: String = text
                        .chars()
                        .filter(|c| c.is_ascii())
                        .collect::<String>()
                        .replace('\\', "\\\\")
                        .replace('(', "\\(")
                        .replace(')', "\\)");
                    // Helvetica's average glyph is about half an em wide, Courier's 0.6.
                    let width = clean.len() as f64 * size * if mono { 0.6 } else { 0.5 };
                    let (x, y) = if center { (at.x - width / 2.0, at.y - size * 0.35) } else { (at.x, at.y) };
                    content += &format!("BT {font} {:.2} Tf {:.3} {:.3} Td ({clean}) Tj ET\n", size * pt, x * pt, y * pt);
                }
                Mark::Image { min, max, image } => {
                    let Some(pic) = self.pictures.iter().find(|p| p.id.0 == image) else { continue };
                    let (w, h, ref px) = *pic.pixels;
                    if w == 0 || h == 0 || px.len() != (w * h * 4) as usize {
                        continue;
                    }
                    let k = match images.iter().position(|i| i.0 == image) {
                        Some(k) => k,
                        None => {
                            let rgb: Vec<u8> = px.as_chunks::<4>().0.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
                            let alpha: Vec<u8> = px.as_chunks::<4>().0.iter().map(|c| c[3]).collect();
                            images.push((image, rgb, alpha, w, h));
                            images.len() - 1
                        }
                    };
                    let d = max - min;
                    content += &format!("q {:.3} 0 0 {:.3} {:.3} {:.3} cm /Im{k} Do Q\n", d.x * pt, d.y * pt, min.x * pt, min.y * pt);
                }
            }
        }
        let (w, h) = (self.sheet.x * pt, self.sheet.y * pt);
        let xobjects: String = (0..images.len()).map(|k| format!("/Im{k} {} 0 R ", 7 + 2 * k)).collect();
        let mut objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.2} {h:.2}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> /XObject << {xobjects}>> >> >>").into_bytes(),
            format!("<< /Length {} >>\nstream\n{content}endstream", content.len()).into_bytes(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".to_vec(),
        ];
        for (k, (_, rgb, alpha, iw, ih)) in images.iter().enumerate() {
            let mut img = format!("<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {} 0 R /Length {} >>\nstream\n", 8 + 2 * k, rgb.len()).into_bytes();
            img.extend_from_slice(rgb);
            img.extend_from_slice(b"\nendstream");
            let mut mask = format!("<< /Type /XObject /Subtype /Image /Width {iw} /Height {ih} /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n", alpha.len()).into_bytes();
            mask.extend_from_slice(alpha);
            mask.extend_from_slice(b"\nendstream");
            objects.push(img);
            objects.push(mask);
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(o);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }
}

/// Standard base64 (for pictures inside SVG).
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn standard_scales() {
        assert_eq!(standard_scale(DVec2::new(100.0, 50.0), DVec2::new(180.0, 110.0)), 1.0);
        assert_eq!(standard_scale(DVec2::new(300.0, 50.0), DVec2::new(180.0, 110.0)), 0.5);
        assert_eq!(standard_scale(DVec2::new(10.0, 5.0), DVec2::new(180.0, 110.0)), 12.0);
        assert_eq!(scale_label(0.5), "1:2");
        assert_eq!(scale_label(2.0), "2:1");
        assert_eq!(scale_label(0.25), "1:4");
        assert_eq!(format_date((2026, 10, 28), "MM/DD/YY"), "10/28/26");
        assert_eq!(format_date((2026, 3, 5), "DD-MMM-YY"), "05-MAR-26");
        assert_eq!(format_date((2026, 3, 5), "YYYYMMDD"), "20260305");
    }
}

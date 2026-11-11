//! Every icon is drawn in code on a 24×24 grid. No image assets.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::theme::{self, Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Preferences ▸ Tablet: a tablet with a pencil beside it.
    Tablet,
    Modeling,
    Visualization,
    Drawings,
    Items,
    /// The Items filter's All Items: a hexagon with stacked layers inside.
    AllItems,
    Search,
    Sketch,
    Insert,
    Construct,
    Transform,
    Tools,
    SectionView,
    Measure,
    Isolate,
    Construction,
    Close,
    /// Done (a tool's confirm button).
    Check,
    /// A tool option (Keep Originals, Copy).
    Gear,
    DeselectAll,
    Line,
    Arc,
    Spline,
    Rectangle,
    Circle,
    Ellipse,
    Polygon,
    /// Tool variants (the cascades beside Rectangle, Polygon, Spline, Offset Edge, Pattern).
    RectCenter,
    Rect3,
    Triangle,
    Hexagon,
    Octagon,
    SplineControl,
    OffsetChain,
    PatternCircular,
    OffsetEdge,
    MoveRotate,
    Mirror,
    Pattern,
    Project,
    Text,
    Trim,
    Delete,
    Extrude,
    /// Extrude's Symmetric side: a slab with arrows both ways.
    ExtrudeSymmetric,
    /// Section View's edit column (s16-021): a frame with a filled panel, and two bars.
    Section2D,
    SectionOnly,
    Fillet,
    /// The Fillet card's Type: Radius (a quarter round with its radius) and Chordal (the round with its chord).
    FilletRadius,
    FilletChord,
    /// The Fillet card's Corner: Rolling Ball (three rounds blending into one ball) and Setback (cut back corners).
    CornerRolling,
    CornerSetback,
    Shell,
    Scale,
    ScaleFree,
    Align,
    OffsetFace,
    Axis,
    Plane,
    Translate,
    RotateAxis,
    Loft,
    SplitBody,
    Revolve,
    Sweep,
    WrapEmboss,
    Visualize,
    More,
    Union,
    Subtract,
    Intersect,
    Body,
    NewBody,
    SketchItem,
    Eye,
    EyeOff,
    Home,
    Help,
    Snap,
    Origin,
    /// The sketch toolset's own Move/Rotate, Mirror and Pattern (as observed, L18).
    SketchMove,
    SketchMirror,
    SketchPattern,
    /// The Drawings toolbar (as observed, 115).
    DViews,
    /// The Measure card and modes (as observed, L24): area (hatched square), volume (cylinder), length, distance,
    /// angle, and the pin.
    MArea,
    MVolume,
    MLength,
    MDistance,
    MAngle,
    Pin,
    /// Insert's Variable (an italic x) and File (a filled folder), as observed (50).
    Variable,
    FolderFilled,
    /// Construct's Plane and Axis (as observed, 51): a flat plate, a dashed line.
    PlaneFlat,
    AxisDashed,
    /// Replace Face: a plate over an open box.
    ReplaceFace,
    /// Fill: a grey slab and a grey ball bridged by a light, curved patch.
    Fill,
    /// Project as Edge (26.200): a line over a curved strip, like Project's. Project as Sketch: an outlined cube with
    /// a sheet beside it (s11-133).
    ProjectEdge,
    ProjectSketch,
    /// Create Drawing: a sheet with a block on it and a plus badge.
    CreateDrawing,
    DDimensions,
    DGeometries,
    DNote,
    DImage,
    /// Visualization's sidebar toggle and Generative Render buttons.
    SidebarToggle,
    CameraSparkle,
    DisplayModes,
    /// Display Modes while Zebra / Curvature Map is on (as observed: a striped cube, a coloured cube).
    DisplayZebra,
    /// History's Merged Steps: a page with a folded corner and a cube on it.
    Merged,
    /// The sketch constraint column (thin line drawings, as observed L18).
    CParallel,
    CPerpendicular,
    CTangent,
    CCoincident,
    CMidpoint,
    CConcentric,
    CHorizVert,
    CEqual,
    CSymmetry,
    Disconnect,
    /// A length dimension's Distance Type: straight between the ends, across only, or up only.
    DistAbsolute,
    DistHorizontal,
    DistVertical,
    Lock,
    MakeConstruction,
    DisplayCurvature,
    Screenshot,
    /// Screenshot mode: a dotted grid with an eye, a checkerboard, a box with "1x".
    ShotGrid,
    ShotChecker,
    ShotResolution,
    History,
    Folder,
    NewFolder,
    Plus,
    Import,
    Export,
    Sort,
    Recents,
    Projects,
    /// A cloud storage location on the projects screen.
    Cloud,
    GetStarted,
    Tutorials,
    ChevronLeft,
    ChevronRight,
    /// A turning arrow (a tutorial's Retry / Redo).
    Retry,
    /// Touch screens' on-screen Undo / Redo: a U-turn arrow in a thin circle.
    Undo,
    Redo,
    ChevronDown,
    ChevronUp,
    Filter,
    Ellipsis,
    Agent,
    /// History's step menu (observed s14-004).
    Breakpoint,
    Suppress,
    Unsuppress,
    Pencil,
    /// A red triangle with "!" (a failing step); drawn in its own colors.
    Warning,
}

/// Drawing helper: coordinates are on a 24×24 grid mapped into `rect`.
struct Pen<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    stroke: Stroke,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.o.x + x * self.s, self.o.y + y * self.s)
    }

    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::line(pts, self.stroke));
    }

    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::closed_line(pts, self.stroke));
    }

    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
    }

    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.at(x, y), r * self.s, self.stroke);
    }

    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.at(x, y), r * self.s, self.stroke.color);
    }

    /// The reference's sketch-shape style: a light filled shape with small dark dots on its key points.
    fn solid(&self, pts: &[(f32, f32)], dots: &[(f32, f32)]) {
        let c = self.stroke.color;
        let q: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::convex_polygon(q.clone(), c.gamma_multiply(0.82), Stroke::NONE));
        self.p.add(Shape::closed_line(q, Stroke::new(self.stroke.width * 0.8, c)));
        for &(x, y) in dots {
            self.p.circle_filled(self.at(x, y), 1.9 * self.s, theme::c(0x2a2a2c));
        }
    }

    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = 18;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.line(&pts);
    }

    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.closed(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    }

    fn with(&self, color: Color32) -> Pen<'_> {
        Pen { p: self.p, o: self.o, s: self.s, stroke: Stroke::new(self.stroke.width, color) }
    }
    /// A hairline pen (about one pixel at list size), for the reference's thin outline icons.
    fn thin(&self) -> Pen<'_> {
        Pen { p: self.p, o: self.o, s: self.s, stroke: Stroke::new((self.s * 1.25).max(0.9), self.stroke.color) }
    }
}

/// A pointy-top hexagon: an isometric block's outline, the shape most of the reference's toolbar icons are built
/// from (as observed, L05).
fn hexagon(cx: f32, cy: f32, r: f32) -> [(f32, f32); 6] {
    [270.0f32, 330.0, 30.0, 90.0, 150.0, 210.0].map(|a| (cx + r * a.to_radians().cos(), cy + r * a.to_radians().sin()))
}

/// A sketch icon's points: small grey dots (as observed).
fn sketch_dots(pen: &Pen, color: Color32, pts: &[(f32, f32)]) {
    let (_, grey) = two_tone(color);
    for &(x, y) in pts {
        pen.p.circle_filled(pen.at(x, y), 1.8 * pen.s, grey);
    }
}

/// The icon colour and its grey (the reference's two tones), so dimmed icons dim both.
fn two_tone(color: Color32) -> (Color32, Color32) {
    let k = |v: u8| (v as f32 * 0.62) as u8;
    (color, Color32::from_rgba_unmultiplied(k(color.r()), k(color.g()), k(color.b()), color.a()))
}

/// A colour wheel, white towards the middle (Visualize, Visualization).
/// The hues reach most of the way in, so the pale middle stays small (as observed, D-idle, s30-100).
fn color_wheel(painter: &Painter, c: Pos2, r: f32) {
    let n = 36u32;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(c, Color32::from_rgb(232, 232, 232));
    let pale = |h: Color32| {
        Color32::from_rgb(((h.r() as u32 + 232) / 2) as u8, ((h.g() as u32 + 232) / 2) as u8, ((h.b() as u32 + 232) / 2) as u8)
    };
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let h = hue(i as f32 / n as f32);
        mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * r * 0.3, pale(h));
        mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * r, h);
    }
    for i in 0..n {
        let (inner, outer, inner2, outer2) = (1 + 2 * i, 2 + 2 * i, 3 + 2 * i, 4 + 2 * i);
        mesh.add_triangle(0, inner, inner2);
        mesh.add_triangle(inner, outer, outer2);
        mesh.add_triangle(inner, outer2, inner2);
    }
    painter.add(Shape::mesh(mesh));
}

/// A bright hue around the colour wheel, `t` from 0 to 1.
fn hue(t: f32) -> Color32 {
    let h = (t.rem_euclid(1.0)) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    let c = |v: f32| (90.0 + v * 165.0) as u8;
    Color32::from_rgb(c(r), c(g), c(b))
}

/// A unit isometric cube outline, used by several solid-modeling icons.
fn cube(pen: &Pen, cx: f32, cy: f32, r: f32) {
    let h = r * 0.5;
    let top = [(cx, cy - r), (cx + r * 0.87, cy - h), (cx, cy), (cx - r * 0.87, cy - h)];
    pen.closed(&top);
    pen.line(&[(cx - r * 0.87, cy - h), (cx - r * 0.87, cy + h), (cx, cy + r), (cx + r * 0.87, cy + h), (cx + r * 0.87, cy - h)]);
    pen.line(&[(cx, cy), (cx, cy + r)]);
}

/// Three tones for solid, lit icons (top, left, right faces), scaled by the icon color's brightness so
/// dimmed icons dim too.
fn tones(color: Color32) -> [Color32; 3] {
    let k = color.r().max(color.g()).max(color.b()) as f32 / 255.0;
    let t = |v: u8| {
        let v = (v as f32 * k) as u8;
        Color32::from_rgb(v, v, (v as f32 * 1.01).min(255.0) as u8)
    };
    [t(0xf4), t(0xc2), t(0x8c)]
}

/// A solid isometric box centred at (cx, cy) with half-width r and height h (in grid units).
fn solid_box(pen: &Pen, cx: f32, cy: f32, r: f32, h: f32, tone: [Color32; 3]) {
    let w = r * 0.87;
    let q = r * 0.5;
    let top = [(cx, cy - h / 2.0 - q), (cx + w, cy - h / 2.0), (cx, cy - h / 2.0 + q), (cx - w, cy - h / 2.0)];
    let left = [(cx - w, cy - h / 2.0), (cx, cy - h / 2.0 + q), (cx, cy + h / 2.0 + q), (cx - w, cy + h / 2.0)];
    let right = [(cx, cy - h / 2.0 + q), (cx + w, cy - h / 2.0), (cx + w, cy + h / 2.0), (cx, cy + h / 2.0 + q)];
    pen.fill(&left, tone[1]);
    pen.fill(&right, tone[2]);
    pen.fill(&top, tone[0]);
}

/// A short white arrow from a to b (grid units), drawn over solid icons.
fn arrow(pen: &Pen, a: (f32, f32), b: (f32, f32), color: Color32) {
    let p = pen.with(color);
    let w = Pen { stroke: Stroke::new(pen.stroke.width * 1.2, color), ..p };
    w.line(&[a, b]);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    pen.fill(&[(b.0 + ux * 2.2, b.1 + uy * 2.2), (b.0 + px * 2.2, b.1 + py * 2.2), (b.0 - px * 2.2, b.1 - py * 2.2)], color);
}

thread_local! {
    /// What the boolean icons cut their gap with: the toolbar button unless the icon sits on something else.
    static GAP: std::cell::Cell<Option<Color32>> = const { std::cell::Cell::new(None) };
}

fn gap() -> Color32 {
    GAP.with(|g| g.get()).unwrap_or_else(Tokens::button)
}

/// Paints an icon exactly in `color`, in either theme, on a background of `bg` (badges and their menus).
pub fn paint_on(painter: &Painter, rect: Rect, icon: Icon, color: Color32, bg: Color32) {
    GAP.with(|g| g.set(Some(bg)));
    paint_dark(painter, rect, icon, color);
    GAP.with(|g| g.set(None));
}

/// Paints an icon in `color`. Icons are drawn in the dark design's shades; the Light theme shows their pairs.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    // The colour wheel is the same in both themes.
    if matches!(icon, Icon::Visualize) {
        return paint_dark(painter, rect, icon, color);
    }
    let ink = theme::dark_of(color);
    theme::paint_themed(painter, || paint_dark(painter, rect, icon, ink));
}

fn paint_dark(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height());
    let o = rect.center() - vec2(size, size) / 2.0;
    let pen = Pen { p: painter, o, s: size / 24.0, stroke: Stroke::new((size / 24.0 * 1.8).max(1.0), color) };
    let blue = pen.with(Tokens::icon_blue());
    use Icon::*;
    match icon {
        // The colour block in a grey hexagon (as observed).
        Modeling => {
            let (_, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.0, 11.0), grey);
            let (cx, cy, r) = (12.0, 12.0, 6.0);
            let h = r * 0.5;
            pen.fill(&[(cx, cy - r), (cx + r * 0.87, cy - h), (cx, cy), (cx - r * 0.87, cy - h)], theme::c(0x2f6cf6));
            pen.fill(&[(cx - r * 0.87, cy - h), (cx, cy), (cx, cy + r), (cx - r * 0.87, cy + h)], theme::c(0xe5453a));
            pen.fill(&[(cx, cy), (cx + r * 0.87, cy - h), (cx + r * 0.87, cy + h), (cx, cy + r)], theme::c(0x3cc45a));
        }
        // A colour wheel in a grey hexagon (as observed).
        Visualization => {
            let (_, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.0, 11.0), grey);
            color_wheel(painter, pen.at(12.0, 12.0), 6.5 * pen.s);
        }
        // A framed sheet with a white block and a title block (as observed).
        Drawings => {
            painter.rect_stroke(
                Rect::from_min_max(pen.at(2.5, 4.5), pen.at(21.5, 19.5)),
                3.0 * pen.s,
                Stroke::new(1.6 * pen.s, color),
                egui::StrokeKind::Middle,
            );
            pen.fill(&hexagon(10.5, 12.0, 5.5), color);
            let (_, grey) = two_tone(color);
            let g = pen.with(grey);
            for y in [14.5, 16.5] {
                g.line(&[(13.5, y), (19.0, y)]);
            }
            g.line(&[(16.25, 13.5), (16.25, 17.5)]);
        }
        Items => {
            for (i, y) in [7.0f32, 11.5, 16.0].iter().enumerate() {
                let pts = [(12.0, y - 3.5), (20.0, *y), (12.0, y + 3.5), (4.0, *y)];
                if i == 0 {
                    pen.fill(&pts, color);
                } else {
                    pen.line(&[(4.0, *y), (12.0, y + 3.5), (20.0, *y)]);
                }
            }
        }
        // A hexagon with a second one's lower edges inside it, like stacked layers (s22-002).
        AllItems => {
            pen.closed(&hexagon(12.0, 12.0, 11.0));
            pen.line(&[(2.5, 11.5), (12.0, 17.0), (21.5, 11.5)]);
        }
        Search => {
            pen.circle(10.5, 10.5, 6.0);
            pen.line(&[(15.0, 15.0), (20.0, 20.0)]);
        }
        // A thin solid arc between two end points over a faint grid of dots, and a cross (as observed, D-idle,
        // s30-100).
        Sketch => {
            let (_, grey) = two_tone(color);
            for gx in [5.0, 10.0, 15.0, 20.0] {
                for gy in [5.0, 10.0, 15.0, 20.0] {
                    pen.p.circle_filled(pen.at(gx, gy), 0.6 * pen.s, grey);
                }
            }
            let arc = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.3).max(1.0), color) };
            arc.arc(20.0, 20.0, 15.0, 180.0, 270.0);
            pen.dot(5.0, 20.0, 1.7);
            pen.dot(20.0, 5.0, 1.7);
            let cross = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.7).max(1.0), color) };
            cross.line(&[(14.5, 10.8), (14.5, 18.2)]);
            cross.line(&[(10.8, 14.5), (18.2, 14.5)]);
        }
        // A light grey square with the plus cut out of it in the button's colour (as observed: #A2A2A2 with a
        // #323232 plus in Dark, D-idle).
        Insert => {
            let r = Rect::from_min_max(pen.at(4.0, 4.0), pen.at(20.0, 20.0));
            painter.rect_filled(r, egui::CornerRadius::same((3.5 * pen.s) as u8), theme::c(0xa2a2a2));
            let w = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.0 * pen.s, theme::c(0x323232)) };
            w.line(&[(12.0, 7.5), (12.0, 16.5)]);
            w.line(&[(7.5, 12.0), (16.5, 12.0)]);
        }
        // A grey plane with an axis through it (as observed).
        Construct => {
            let (light, grey) = two_tone(color);
            let w = pen.with(light);
            w.line(&[(3.0, 16.0), (9.0, 13.5)]);
            pen.fill(&[(9.0, 5.0), (15.5, 8.0), (15.5, 19.0), (9.0, 16.0)], grey);
            for k in 0..2 {
                let x = 15.5 + k as f32 * 3.2;
                w.line(&[(x, 9.8 - k as f32 * 1.35), (x + 1.8, 9.0 - k as f32 * 1.35)]);
            }
            w.dot(12.3, 12.0, 1.3);
        }
        // A small grey block with arrows leaving it three ways (as observed).
        Transform => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.5, 4.8), grey);
            let w = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.7 * pen.s, light) };
            for (ax, ay) in [(0.0f32, -1.0f32), (-0.87, 0.5), (0.87, 0.5)] {
                let (a, b) = ((12.0 + ax * 5.5, 12.5 + ay * 5.5), (12.0 + ax * 10.5, 12.5 + ay * 10.5));
                w.line(&[a, b]);
                let (px, py) = (-ay, ax);
                w.line(&[
                    (b.0 - ax * 2.5 + px * 2.3, b.1 - ay * 2.5 + py * 2.3),
                    b,
                    (b.0 - ax * 2.5 - px * 2.3, b.1 - ay * 2.5 - py * 2.3),
                ]);
            }
        }
        // A wrench (ring at the lower left, open jaw at the upper right) crossed with a screwdriver (thin tip at the
        // upper left, fat handle at the lower right), as observed (D-idle, s30-100).
        Tools => {
            let thick = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.5 * pen.s, color) };
            thick.line(&[(7.0, 17.0), (15.2, 8.8)]);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.6 * pen.s, color) }.circle(5.8, 18.2, 1.7);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.2 * pen.s, color) }.arc(17.2, 6.8, 2.6, 0.0, 270.0);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.5 * pen.s, color) }.line(&[(4.5, 4.5), (11.0, 11.0)]);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(3.6 * pen.s, color) }.line(&[(13.6, 13.6), (19.0, 19.0)]);
        }
        // A cylinder cut open, its cut face hatched (as observed).
        SectionView => {
            let (light, grey) = two_tone(color);
            let top: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let a = (i as f32 / 24.0 * 360.0f32).to_radians();
                    (12.0 + 7.5 * a.cos(), 6.5 + 2.6 * a.sin())
                })
                .collect();
            let body = Rect::from_min_max(pen.at(4.5, 6.5), pen.at(19.5, 19.5));
            painter.add(Shape::rect_filled(
                body,
                egui::CornerRadius { nw: 0, ne: 0, sw: (3.0 * pen.s) as u8, se: (3.0 * pen.s) as u8 },
                light,
            ));
            let hatch = painter.with_clip_rect(body);
            for k in 0..8 {
                let x = 1.0 + k as f32 * 3.4;
                hatch.line_segment([pen.at(x, 20.0), pen.at(x + 9.0, 6.0)], Stroke::new(1.5 * pen.s, Tokens::button()));
            }
            pen.fill(&top, grey);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(10.0, 6.5), pen.at(14.0, 19.5)), 0.0, Tokens::button()));
        }
        // A tape measure, its tape run out to the right (as observed).
        Measure => {
            let (light, grey) = two_tone(color);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(8.0, 13.5), pen.at(21.5, 18.5)), 1.0 * pen.s, light));
            for k in 0..5 {
                let x = 11.0 + k as f32 * 2.3;
                pen.with(Tokens::button()).line(&[(x, 13.5), (x, 15.3)]);
            }
            painter.circle_filled(pen.at(8.5, 11.5), 6.5 * pen.s, grey);
            painter.circle_filled(pen.at(8.5, 11.5), 2.0 * pen.s, light);
        }
        // A block ringed in white, a small grey one set aside (as observed).
        Isolate => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(6.0, 6.5, 3.6), grey);
            let big = hexagon(14.0, 13.5, 8.0);
            pen.fill(&big, grey);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.2 * pen.s, light) }.closed(&big);
        }
        Construction => {
            let dash = |a: (f32, f32), b: (f32, f32)| pen.line(&[a, b]);
            dash((4.0, 5.0), (8.0, 5.0));
            dash((11.0, 5.0), (15.0, 5.0));
            dash((4.0, 8.0), (4.0, 12.0));
            dash((4.0, 15.0), (4.0, 19.0));
            dash((7.0, 19.0), (11.0, 19.0));
            blue.circle(16.5, 15.5, 4.0);
        }
        Close => {
            pen.line(&[(6.0, 6.0), (18.0, 18.0)]);
            pen.line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Check => pen.line(&[(5.5, 12.5), (10.0, 17.5), (19.0, 5.5)]),
        Gear => {
            // Eight teeth around a ring with a hole.
            let c = pen.at(12.0, 12.0);
            for i in 0..8 {
                let a = i as f32 / 8.0 * std::f32::consts::TAU;
                let d = vec2(a.cos(), a.sin());
                let n = vec2(-d.y, d.x) * 1.6 * pen.s;
                let (r0, r1) = (5.5 * pen.s, 8.5 * pen.s);
                painter.add(Shape::convex_polygon(
                    vec![c + d * r0 - n, c + d * r1 - n, c + d * r1 + n, c + d * r0 + n],
                    color,
                    Stroke::NONE,
                ));
            }
            painter.circle_filled(c, 6.5 * pen.s, color);
            painter.circle_filled(c, 2.6 * pen.s, Tokens::button());
        }
        // A dashed square with an X (as observed).
        DeselectAll => {
            let r = Rect::from_min_max(pen.at(4.0, 4.0), pen.at(20.0, 20.0));
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.add(Shape::dashed_line(&pts, pen.stroke, 3.2 * pen.s, 2.2 * pen.s));
            pen.line(&[(9.0, 9.0), (15.0, 15.0)]);
            pen.line(&[(15.0, 9.0), (9.0, 15.0)]);
        }
        // Sketch curves with their points as grey dots (as observed, L18).
        Line => {
            let thin = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.4).max(1.0), color) };
            thin.line(&[(5.5, 18.5), (18.5, 5.5)]);
            sketch_dots(&pen, color, &[(5.5, 18.5), (18.5, 5.5)]);
        }
        Arc => {
            let thin = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.4).max(1.0), color) };
            thin.arc(18.5, 18.5, 12.5, 180.0, 270.0);
            sketch_dots(&pen, color, &[(6.0, 18.5), (18.5, 6.0), (9.66, 9.66)]);
        }
        Spline => {
            let pts: Vec<(f32, f32)> = (0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    // An S from bottom left to top right, like the reference.
                    let u = 1.0 - t;
                    let b = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    let c = [(5.0, 19.0), (19.0, 19.0), (5.0, 5.0), (19.0, 5.0)];
                    (c.iter().zip(b).map(|(c, w)| c.0 * w).sum(), c.iter().zip(b).map(|(c, w)| c.1 * w).sum())
                })
                .collect();
            pen.line(&pts);
            sketch_dots(&pen, color, &[(4.0, 12.0), (20.0, 12.0)]);
        }
        Rectangle => {
            pen.solid(&[(4.5, 7.0), (19.5, 7.0), (19.5, 17.5), (4.5, 17.5)], &[]);
            sketch_dots(&pen, color, &[(4.5, 7.0), (19.5, 17.5)]);
        }
        Circle => pen.solid(&ring(12.0, 12.0, 8.0, 8.0, 32, 0.0), &[(12.0, 12.0), (20.0, 12.0)]),
        Ellipse => pen.solid(&ring(12.0, 12.0, 9.0, 5.5, 32, 0.0), &[(12.0, 12.0)]),
        Polygon => {
            pen.solid(&ring(12.0, 12.5, 8.5, 8.5, 5, -90.0), &[(12.0, 12.5)]);
            sketch_dots(&pen, color, &[(17.0, 19.4)]);
        }
        // Sketch Move/Rotate: a dashed square with arrows up and right out of it.
        SketchMove => {
            let r = Rect::from_min_max(pen.at(5.0, 10.0), pen.at(13.5, 18.5));
            painter.add(Shape::dashed_line(
                &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()],
                Stroke::new(1.3 * pen.s, color),
                2.0 * pen.s,
                1.6 * pen.s,
            ));
            arrow(&pen, (9.25, 13.0), (9.25, 4.5), color);
            arrow(&pen, (11.0, 14.25), (19.5, 14.25), color);
        }
        // Sketch Mirror: two brackets facing each other across a dashed axis.
        SketchMirror => {
            let thin = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.5).max(1.0), color) };
            thin.line(&[(9.0, 8.0), (5.0, 8.0), (5.0, 16.0), (9.0, 16.0)]);
            thin.line(&[(15.0, 8.0), (19.0, 8.0), (19.0, 16.0), (15.0, 16.0)]);
            for k in 0..5 {
                let y = 3.5 + k as f32 * 3.8;
                thin.line(&[(12.0, y), (12.0, y + 1.8)]);
            }
        }
        // Sketch Pattern: four rounded squares with a point in each.
        SketchPattern => {
            let (_, grey) = two_tone(color);
            for (x, y) in [(4.5, 4.5), (13.0, 4.5), (4.5, 13.0), (13.0, 13.0)] {
                painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(x, y), pen.at(x + 7.0, y + 7.0)), 1.6 * pen.s, grey));
                painter.circle_filled(pen.at(x + 3.5, y + 3.5), 1.5 * pen.s, color);
            }
        }
        RectCenter => pen.solid(&[(4.0, 7.0), (20.0, 7.0), (20.0, 17.0), (4.0, 17.0)], &[(12.0, 12.0), (20.0, 17.0)]),
        Rect3 => pen.solid(&[(4.0, 9.5), (16.5, 5.5), (20.0, 14.5), (7.5, 18.5)], &[(4.0, 9.5), (16.5, 5.5), (20.0, 14.5)]),
        Triangle | Hexagon | Octagon => {
            let n = match icon {
                Triangle => 3,
                Hexagon => 6,
                _ => 8,
            };
            pen.solid(&ring(12.0, 12.5, 8.5, 8.5, n, -90.0), &[(12.0, 12.5)]);
        }
        SplineControl => {
            let ctrl = [(4.0, 18.0), (8.0, 6.0), (16.0, 18.0), (20.0, 6.0)];
            blue.line(&ctrl);
            for (x, y) in ctrl {
                blue.dot(x, y, 1.4);
            }
            let pts: Vec<(f32, f32)> = (0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    let u = 1.0 - t;
                    let b = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    (ctrl.iter().zip(b).map(|(c, w)| c.0 * w).sum(), ctrl.iter().zip(b).map(|(c, w)| c.1 * w).sum())
                })
                .collect();
            pen.line(&pts);
        }
        OffsetChain => {
            pen.arc(12.0, 14.0, 8.0, 180.0, 360.0);
            pen.line(&[(4.0, 14.0), (4.0, 19.0), (20.0, 19.0), (20.0, 14.0)]);
            blue.arc(12.0, 14.0, 4.5, 180.0, 360.0);
            blue.line(&[(7.5, 14.0), (7.5, 16.0), (16.5, 16.0), (16.5, 14.0)]);
        }
        PatternCircular => {
            let t = tones(color);
            for k in 0..4 {
                let a = (k as f32 * 90.0 - 45.0).to_radians();
                solid_box(&pen, 12.0 + 6.5 * a.cos(), 12.5 + 6.5 * a.sin(), 3.2, 3.0, t);
            }
        }
        // An arch and a smaller one inside it (as observed).
        OffsetEdge => {
            pen.arc(12.0, 11.0, 8.0, 180.0, 360.0);
            pen.line(&[(4.0, 11.0), (4.0, 20.0), (20.0, 20.0), (20.0, 11.0)]);
            pen.arc(12.0, 12.0, 4.5, 180.0, 360.0);
            pen.line(&[(7.5, 12.0), (7.5, 16.5)]);
            pen.line(&[(16.5, 12.0), (16.5, 16.5)]);
        }
        // A grey block with three white arrows out of its middle (as observed, L05).
        MoveRotate => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.0, 10.8), grey);
            // Open V heads, inside the block.
            let w = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.7 * pen.s, light) };
            for (ax, ay) in [(0.0f32, -1.0f32), (-0.87, 0.5), (0.87, 0.5)] {
                let (a, b) = ((12.0 + ax * 3.0, 12.0 + ay * 3.0), (12.0 + ax * 8.0, 12.0 + ay * 8.0));
                w.line(&[a, b]);
                let (px, py) = (-ay, ax);
                w.line(&[
                    (b.0 - ax * 2.6 + px * 2.4, b.1 - ay * 2.6 + py * 2.4),
                    b,
                    (b.0 - ax * 2.6 - px * 2.4, b.1 - ay * 2.6 - py * 2.4),
                ]);
            }
            pen.with(light).dot(12.0, 12.0, 1.3);
        }
        // A small grey block and its white mirror image, an arrow between them (as observed).
        Mirror => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(7.0, 9.0, 4.5), grey);
            pen.fill(&hexagon(15.0, 14.0, 7.0), light);
            arrow(&pen, (7.0, 9.5), (12.0, 13.0), light);
        }
        // Two blocks, one behind the other (as observed).
        Pattern => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(15.0, 9.5, 6.5), grey);
            pen.fill(&hexagon(10.0, 14.5, 6.5), light);
            pen.with(Tokens::button()).dot(10.0, 11.5, 1.2);
        }
        // A line over a curved sheet it lands on (as observed).
        ProjectSketch => {
            let h = hexagon(9.5, 11.0, 6.0);
            pen.closed(&h);
            pen.line(&[h[4], (9.5, 11.0), h[2]]);
            pen.line(&[(9.5, 11.0), (9.5, 17.0)]);
            pen.closed(&[(15.0, 13.0), (19.5, 10.5), (19.5, 17.5), (15.0, 20.0)]);
        }
        Project | ProjectEdge => {
            let (light, _) = two_tone(color);
            let w = pen.with(light);
            w.line(&[(8.0, 3.5), (17.0, 8.0)]);
            let top: Vec<(f32, f32)> = (0..=10)
                .map(|i| {
                    let t = i as f32 / 10.0;
                    (4.0 + 15.0 * t, 13.0 - 3.0 * t + 2.5 * (t * std::f32::consts::PI).sin())
                })
                .collect();
            let mut outline = top.clone();
            outline.extend(top.iter().rev().map(|(x, y)| (x + 2.5, y + 7.0)));
            w.closed(&outline);
            for k in 1..4 {
                let t = k as f32 / 4.0;
                let (x, y) = top[(t * 10.0) as usize];
                w.line(&[(x, y), (x + 2.5, y + 7.0)]);
            }
        }
        Text => {
            painter.text(pen.at(12.0, 12.5), egui::Align2::CENTER_CENTER, "Aa", crate::theme::strong(15.0 * pen.s), color);
        }
        Trim => {
            pen.circle(7.0, 17.0, 2.8);
            pen.circle(7.0, 7.0, 2.8);
            pen.line(&[(9.2, 15.2), (20.0, 5.0)]);
            pen.line(&[(9.2, 8.8), (20.0, 19.0)]);
        }
        Delete => {
            pen.fill(&[(4.5, 5.5), (19.5, 5.5), (19.5, 8.0), (4.5, 8.0)], color);
            pen.fill(&[(9.5, 3.5), (14.5, 3.5), (14.5, 5.5), (9.5, 5.5)], color);
            pen.fill(&[(6.0, 9.0), (18.0, 9.0), (17.0, 21.0), (7.0, 21.0)], color);
            let dark = pen.with(theme::c(0x2b2b2d));
            for x in [9.5f32, 12.0, 14.5] {
                dark.line(&[(x, 11.5), (x, 18.5)]);
            }
        }
        // A white block with an arrow up its face (as observed).
        Extrude => {
            let (light, _) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.5, 10.0), light);
            arrow(&pen, (12.0, 17.0), (12.0, 7.5), Tokens::button());
        }
        // A thin slab with an arrow up and one down (s16-005, Sides ▸ Symmetric).
        ExtrudeSymmetric => {
            let (light, _) = two_tone(color);
            pen.fill(&[(4.0, 12.0), (12.0, 8.0), (20.0, 12.0), (12.0, 16.0)], light);
            arrow(&pen, (12.0, 10.5), (12.0, 2.0), light);
            arrow(&pen, (12.0, 13.5), (12.0, 22.0), light);
        }
        Section2D => {
            let (light, _) = two_tone(color);
            pen.with(light).line(&[(5.0, 5.0), (19.0, 5.0), (19.0, 19.0), (5.0, 19.0), (5.0, 5.0)]);
            pen.fill(&[(8.0, 7.5), (13.0, 7.5), (13.0, 16.5), (8.0, 16.5)], light);
        }
        SectionOnly => {
            let (light, _) = two_tone(color);
            pen.fill(&[(6.0, 6.0), (10.5, 6.0), (10.5, 18.0), (6.0, 18.0)], light);
            pen.fill(&[(13.5, 6.0), (18.0, 6.0), (18.0, 18.0), (13.5, 18.0)], light);
        }
        // A grey block with a light rounded-off edge across its top (as observed).
        Fillet => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.0, 10.8), grey);
            pen.fill(&[(7.0, 6.5), (12.8, 3.4), (19.6, 7.6), (13.6, 11.6)], light);
            pen.fill(&[(7.0, 6.5), (13.6, 11.6), (13.0, 15.0), (6.0, 10.0)], light);
        }
        // A block with its top edge rounded; a white radius from the round's centre (Radius) or across it (Chordal).
        FilletRadius | FilletChord => {
            let (light, grey) = two_tone(color);
            let mut body = vec![(4.0, 20.0), (4.0, 5.0), (11.0, 5.0)];
            for k in 0..=8 {
                let a = (-90.0 + 90.0 * k as f32 / 8.0).to_radians();
                body.push((11.0 + 8.0 * a.cos(), 13.0 + 8.0 * a.sin()));
            }
            body.push((19.0, 20.0));
            pen.fill(&body, grey);
            let w = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.6 * pen.s, light) };
            if icon == FilletRadius {
                w.line(&[(11.0, 13.0), (16.7, 7.3)]);
                w.dot(11.0, 13.0, 1.2);
            } else {
                w.line(&[(11.0, 5.0), (19.0, 13.0)]);
            }
        }
        // Three rounds meeting: in one ball (Rolling Ball), or in a corner patch set back along them (Setback).
        CornerRolling | CornerSetback => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.5, 9.5), grey);
            let w = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.4 * pen.s, light) };
            for (dx, dy) in [(0.0f32, -1.0f32), (-0.87, 0.5), (0.87, 0.5)] {
                let from = if icon == CornerSetback { 4.5 } else { 2.5 };
                w.line(&[(12.0 + dx * from, 12.5 + dy * from), (12.0 + dx * 8.5, 12.5 + dy * 8.5)]);
            }
            if icon == CornerRolling {
                pen.with(light).dot(12.0, 12.5, 2.6);
            } else {
                pen.fill(&hexagon(12.0, 12.5, 4.0), light);
            }
        }
        // A grey block with a thin white wall bent round inside it (as observed).
        Shell => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 12.0, 10.8), grey);
            let wall = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(2.8 * pen.s, light) };
            wall.line(&[(7.0, 8.0), (12.5, 5.2), (17.0, 8.0), (17.0, 18.0)]);
        }
        // A cube inside a cube's outline, corners joined (as observed).
        Scale => {
            let (light, _) = two_tone(color);
            let w = pen.with(light);
            let outer = hexagon(12.0, 12.0, 10.8);
            let inner = hexagon(12.0, 12.5, 4.6);
            w.closed(&outer);
            cube(&w, 12.0, 12.5, 4.6);
            for k in [0usize, 2, 4] {
                w.line(&[outer[k], inner[k]]);
            }
        }
        // Scale Mode's Non-uniform: the same frame around a block stretched wide.
        ScaleFree => {
            let (light, _) = two_tone(color);
            let w = pen.with(light);
            w.closed(&hexagon(12.0, 12.0, 10.8));
            w.closed(&[(6.5, 10.0), (17.5, 10.0), (17.5, 15.0), (6.5, 15.0)]);
        }
        // A white block brought up against a grey one, an arrow on it (as observed).
        Align => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(9.5, 10.0, 7.0), grey);
            pen.fill(&hexagon(14.0, 14.5, 7.0), light);
            arrow(&pen, (14.0, 19.0), (14.0, 11.5), Tokens::button());
        }
        // A flat slab, white on top, with an arrow up off it (as observed).
        OffsetFace => {
            let (light, grey) = two_tone(color);
            pen.fill(&[(3.5, 14.0), (12.0, 18.5), (20.5, 14.0), (20.5, 16.5), (12.0, 21.0), (3.5, 16.5)], grey);
            pen.fill(&[(12.0, 9.5), (20.5, 14.0), (12.0, 18.5), (3.5, 14.0)], light);
            arrow(&pen, (12.0, 12.5), (12.0, 4.0), light);
        }
        MArea => {
            let r = Rect::from_min_max(pen.at(5.0, 5.0), pen.at(19.0, 19.0));
            let clip = painter.with_clip_rect(r);
            for k in 0..7 {
                let x = 1.0 + k as f32 * 4.0;
                clip.line_segment([pen.at(x, 19.0), pen.at(x + 14.0, 5.0)], Stroke::new(1.4 * pen.s, color));
            }
        }
        MVolume => {
            let (light, grey) = two_tone(color);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(6.0, 8.0), pen.at(18.0, 17.5)), 0.0, grey));
            painter.add(Shape::ellipse_filled(pen.at(12.0, 17.5), vec2(6.0, 2.4) * pen.s, grey));
            painter.add(Shape::ellipse_filled(pen.at(12.0, 8.0), vec2(6.0, 2.4) * pen.s, light));
        }
        MLength => {
            pen.line(&[(4.0, 16.0), (20.0, 8.0)]);
            for (x, y) in [(4.0, 16.0), (20.0, 8.0)] {
                pen.dot(x, y, 1.6);
            }
        }
        MDistance => {
            pen.line(&[(6.5, 12.0), (17.5, 12.0)]);
            pen.line(&[(9.0, 10.0), (6.5, 12.0), (9.0, 14.0)]);
            pen.line(&[(15.0, 10.0), (17.5, 12.0), (15.0, 14.0)]);
            pen.dot(3.5, 12.0, 1.4);
            pen.dot(20.5, 12.0, 1.4);
        }
        MAngle => {
            for k in 0..5 {
                let t0 = k as f32 / 5.0;
                let t1 = t0 + 0.11;
                pen.line(&[(4.0 + 8.0 * t0, 19.0 - 14.0 * t0), (4.0 + 8.0 * t1, 19.0 - 14.0 * t1)]);
            }
            pen.line(&[(4.0, 19.0), (20.0, 19.0)]);
            pen.arc(4.0, 19.0, 9.0, 300.0, 360.0);
            pen.dot(12.0, 5.0, 1.4);
            pen.dot(20.0, 19.0, 1.4);
        }
        Pin => {
            pen.line(&[(8.5, 5.0), (15.5, 5.0)]);
            pen.line(&[(10.0, 5.0), (10.0, 11.0), (7.5, 14.0), (16.5, 14.0), (14.0, 11.0), (14.0, 5.0)]);
            pen.line(&[(12.0, 14.0), (12.0, 20.0)]);
        }
        // An italic x drawn in strokes (no font of our own to lean on).
        Variable => {
            let thin = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.6 * pen.s, color) };
            thin.arc(9.0, 11.0, 3.2, 200.0, 330.0);
            thin.line(&[(11.6, 9.4), (13.0, 16.0), (15.5, 17.2)]);
            thin.line(&[(8.0, 17.0), (12.5, 12.5), (16.5, 8.5), (18.5, 8.2)]);
        }
        FolderFilled => {
            pen.fill(&[(3.0, 6.5), (9.0, 6.5), (11.0, 8.5), (21.0, 8.5), (21.0, 19.0), (3.0, 19.0)], color);
        }
        PlaneFlat => {
            let (light, grey) = two_tone(color);
            pen.fill(&[(3.0, 12.5), (12.0, 17.0), (21.0, 12.5), (21.0, 14.0), (12.0, 18.5), (3.0, 14.0)], grey);
            pen.fill(&[(12.0, 8.0), (21.0, 12.5), (12.0, 17.0), (3.0, 12.5)], light);
        }
        AxisDashed => {
            for k in 0..6 {
                let t0 = k as f32 / 6.0;
                let t1 = t0 + 0.09;
                pen.line(&[(3.5 + 17.0 * t0, 17.0 - 10.0 * t0), (3.5 + 17.0 * t1, 17.0 - 10.0 * t1)]);
            }
        }
        // A white plate tilted above an open grey box (as observed).
        ReplaceFace => {
            let (light, grey) = two_tone(color);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.8 * pen.s, grey) }.line(&[
                (7.5, 12.0),
                (7.5, 18.0),
                (12.0, 20.5),
                (16.5, 18.0),
                (16.5, 12.0),
            ]);
            pen.with(grey).line(&[(12.0, 14.5), (12.0, 20.5)]);
            pen.fill(&[(5.0, 9.5), (13.0, 3.0), (21.0, 7.0), (13.5, 13.5)], light);
        }
        // Two grey pieces with the light surface that fills the gap between them.
        Fill => {
            let (light, grey) = two_tone(color);
            let slab = Rect::from_min_max(pen.at(3.5, 3.5), pen.at(10.0, 20.5));
            painter.rect_filled(slab, egui::CornerRadius::same((3.0 * pen.s) as u8), grey);
            painter.circle_filled(pen.at(16.5, 8.5), 4.8 * pen.s, grey);
            // The patch: a crescent from the slab's edge round to the ball.
            let outer: Vec<(f32, f32)> = (0..=8)
                .map(|i| {
                    let a = (200.0f32 - 110.0 * i as f32 / 8.0).to_radians();
                    (17.0 + 6.5 * a.cos(), 8.0 - 6.5 * a.sin())
                })
                .collect();
            for w in outer.windows(2) {
                pen.fill(&[w[0], w[1], (13.0, 11.0)], light);
            }
            pen.fill(&[(10.5, 9.5), (13.0, 11.0), (13.5, 16.5), (10.5, 15.0)], light);
        }
        // A grey plane with a white axis through it (as observed).
        Axis => {
            let (light, grey) = two_tone(color);
            let w = pen.with(light);
            w.line(&[(3.0, 16.0), (9.0, 13.5)]);
            pen.fill(&[(9.0, 5.0), (15.5, 8.0), (15.5, 19.0), (9.0, 16.0)], grey);
            w.line(&[(13.0, 11.6), (21.0, 8.2)]);
            w.dot(12.3, 12.0, 1.3);
        }
        // A white plate over a grey block (as observed).
        Plane => {
            let (light, grey) = two_tone(color);
            pen.fill(&[(6.0, 11.0), (12.0, 14.0), (18.0, 11.0), (18.0, 17.5), (12.0, 20.5), (6.0, 17.5)], grey);
            pen.with(light).line(&[(12.0, 14.5), (12.0, 20.0)]);
            pen.fill(&[(12.0, 3.0), (21.5, 7.8), (12.0, 12.6), (2.5, 7.8)], light);
        }
        // An upright grey cylinder with a white arrow leaving it (as observed).
        Translate => {
            let (light, grey) = two_tone(color);
            let n = 24;
            let ell = |yc: f32, from: f32, to: f32| -> Vec<(f32, f32)> {
                (0..=n)
                    .map(|i| {
                        let a = (from + (to - from) * i as f32 / n as f32).to_radians();
                        (9.5 + 6.5 * a.cos(), yc + 2.8 * a.sin())
                    })
                    .collect()
            };
            let mut body = ell(8.0, 180.0, 360.0);
            body.extend(ell(17.5, 0.0, 180.0));
            pen.fill(&body, grey);
            pen.fill(&ell(8.0, 0.0, 360.0), light);
            arrow(&pen, (13.5, 14.0), (21.5, 14.0), light);
        }
        // A grey block and a white arrow turning round a point (as observed).
        RotateAxis => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(9.0, 9.0, 6.0), grey);
            let w = pen.with(light);
            w.arc(13.0, 14.5, 5.5, 300.0, 520.0);
            arrow(&pen, (9.5, 19.0), (8.0, 17.5), light);
            w.dot(13.0, 14.5, 1.2);
        }
        // A white frustum on a grey slab (as observed).
        Loft => {
            let (light, grey) = two_tone(color);
            pen.fill(&[(4.0, 14.5), (12.0, 18.5), (20.0, 14.5), (20.0, 17.0), (12.0, 21.0), (4.0, 17.0)], grey);
            let top: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let a = (180.0 + 180.0 * i as f32 / 12.0f32).to_radians();
                    (12.0 + 4.5 * a.cos(), 6.0 + 1.8 * a.sin())
                })
                .collect();
            let mut body = top;
            body.extend([(19.5, 13.0), (12.0, 16.5), (4.5, 13.0)]);
            pen.fill(&body, light);
        }
        // A grey block cut through by a white slab (as observed).
        SplitBody => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(12.0, 9.5, 8.8), grey);
            pen.fill(&[(4.4, 12.5), (12.0, 16.9), (19.6, 12.5), (19.6, 15.2), (12.0, 19.6), (4.4, 15.2)], light);
            pen.fill(&[(4.4, 17.0), (12.0, 21.4), (19.6, 17.0), (19.6, 18.6), (12.0, 23.0), (4.4, 18.6)], grey);
        }
        // A white half disc and a grey disc about a dashed axis (as observed).
        Revolve => {
            let (light, grey) = two_tone(color);
            let half: Vec<(f32, f32)> = (0..=16)
                .map(|i| {
                    let a = (-90.0 + 180.0 * i as f32 / 16.0f32).to_radians();
                    (12.5 + 8.0 * a.cos(), 12.0 + 8.0 * a.sin())
                })
                .collect();
            pen.fill(&half, light);
            painter.circle_filled(pen.at(7.0, 14.5), 3.8 * pen.s, grey);
            for k in 0..5 {
                let y = 2.5 + k as f32 * 4.0;
                pen.with(light).line(&[(11.0, y), (11.0, y + 2.0)]);
            }
        }
        // A white pipe bending through the icon (as observed).
        Sweep => {
            let (light, grey) = two_tone(color);
            let pipe: Vec<(f32, f32)> = (0..=20)
                .map(|i| {
                    let t = i as f32 / 20.0;
                    let x = 5.0 + 14.0 * t;
                    (x, 19.0 - 12.0 * (3.0 * t * t - 2.0 * t * t * t))
                })
                .collect();
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(5.0 * pen.s, light) }.line(&pipe);
            painter.add(Shape::ellipse_filled(pen.at(19.0, 7.0), vec2(2.5, 1.2) * pen.s, grey));
        }
        // A curled white sheet (as observed).
        WrapEmboss => {
            let (light, grey) = two_tone(color);
            let curl = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(3.2 * pen.s, light) };
            curl.arc(9.5, 13.0, 5.0, 120.0, 400.0);
            pen.fill(&[(9.0, 18.0), (20.0, 15.5), (20.5, 19.0), (9.5, 21.0)], grey);
        }
        // A colour wheel, white towards the middle (as observed).
        Visualize => color_wheel(painter, pen.at(12.0, 12.0), 10.0 * pen.s),
        More => {
            pen.dot(6.0, 12.0, 1.6);
            pen.dot(12.0, 12.0, 1.6);
            pen.dot(18.0, 12.0, 1.6);
        }
        Ellipsis => {
            pen.circle(12.0, 12.0, 8.0);
            pen.dot(8.5, 12.0, 1.1);
            pen.dot(12.0, 12.0, 1.1);
            pen.dot(15.5, 12.0, 1.1);
        }
        // A block and a disc, both white (as observed).
        Union => {
            let (light, _) = two_tone(color);
            pen.fill(&hexagon(10.0, 11.0, 7.0), light);
            let c = pen.at(15.0, 15.0);
            painter.circle_filled(c, 6.6 * pen.s, gap());
            painter.circle_filled(c, 5.8 * pen.s, light);
        }
        // A white block with a grey disc taken out of it (as observed).
        Subtract => {
            let (light, grey) = two_tone(color);
            pen.fill(&hexagon(10.0, 11.0, 7.0), light);
            let c = pen.at(15.0, 15.0);
            painter.circle_filled(c, 6.6 * pen.s, gap());
            painter.circle_filled(c, 5.8 * pen.s, grey);
        }
        // A block and a disc in grey outline, what they share in white (as observed).
        Intersect => {
            let (light, grey) = two_tone(color);
            let hx = hexagon(10.0, 11.0, 7.0);
            let (cx, cy, cr) = (15.0, 15.0, 5.8);
            let inside_hex = |q: (f32, f32)| {
                (0..6).all(|k| {
                    let (a, b) = (hx[k], hx[(k + 1) % 6]);
                    (b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0) >= 0.0
                })
            };
            let mut lens: Vec<(f32, f32)> = (0..48)
                .map(|i| {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    (cx + cr * a.cos(), cy + cr * a.sin())
                })
                .filter(|q| inside_hex(*q))
                .collect();
            lens.extend(hx.iter().copied().filter(|q| (q.0 - cx).hypot(q.1 - cy) <= cr));
            if lens.len() >= 3 {
                let (mx, my) = lens.iter().fold((0.0, 0.0), |acc, q| (acc.0 + q.0, acc.1 + q.1));
                let (mx, my) = (mx / lens.len() as f32, my / lens.len() as f32);
                lens.sort_by(|a, b| (a.1 - my).atan2(a.0 - mx).total_cmp(&(b.1 - my).atan2(b.0 - mx)));
                pen.fill(&lens, light);
            }
            let g = pen.with(grey);
            g.closed(&hx);
            g.circle(cx, cy, cr);
        }
        // A hairline cube, as in the reference's Items list (s22-002).
        Body => cube(&pen.thin(), 12.0, 12.0, 7.8),
        // The Result badge's New Body: a solid hexagon (s25-017).
        NewBody => pen.fill(&hexagon(12.0, 12.0, 8.5), color),
        // A hairline triangle pointing left with a small ring on each corner: a sketch in the Items list (s22-002).
        SketchItem => {
            let pen = pen.thin();
            let (r, corners) = (1.6, [(5.8f32, 12.0f32), (18.4, 6.2), (18.4, 17.8)]);
            for (k, &(x, y)) in corners.iter().enumerate() {
                pen.circle(x, y, r);
                let (nx, ny) = corners[(k + 1) % 3];
                let len = ((nx - x).powi(2) + (ny - y).powi(2)).sqrt();
                let (ux, uy) = ((nx - x) / len, (ny - y) / len);
                pen.line(&[(x + ux * r, y + uy * r), (nx - ux * r, ny - uy * r)]);
            }
        }
        // A solid almond with a dark pupil, as in the reference's Items list (s20-030); hidden ones are struck through
        // with the stroke cut out of the eye.
        Eye | EyeOff => {
            let lens = |cy: f32, a0: f32| {
                (0..=12).map(move |i| {
                    let a = (a0 + 132.2 * i as f32 / 12.0).to_radians();
                    (12.0 + 10.94 * a.cos(), cy + 10.94 * a.sin())
                })
            };
            let almond: Vec<(f32, f32)> = lens(16.44, 203.9).chain(lens(7.56, 23.9)).collect();
            pen.fill(&almond, pen.stroke.color);
            let cut = theme::c(0x232325);
            pen.p.circle_filled(pen.at(12.0, 12.0), 3.0 * pen.s, cut);
            if icon == EyeOff {
                let slash = |w: f32, c: Color32| pen.p.line_segment([pen.at(5.0, 4.5), pen.at(19.0, 19.5)], Stroke::new(w * pen.s, c));
                slash(3.0, cut);
                slash(1.0, pen.stroke.color);
            }
        }
        Tablet => {
            pen.closed(&[(3.5, 4.5), (16.5, 4.5), (16.5, 20.0), (3.5, 20.0)]);
            pen.dot(10.0, 17.5, 0.9);
            pen.line(&[(20.5, 4.0), (20.5, 17.0), (19.5, 19.5)]);
            pen.line(&[(20.5, 17.0), (21.5, 19.5), (19.5, 19.5)]);
        }
        Home => {
            pen.closed(&[(4.0, 11.0), (12.0, 4.0), (20.0, 11.0), (20.0, 20.0), (4.0, 20.0)]);
            pen.rect(10.0, 14.0, 14.0, 20.0);
        }
        Help => {
            pen.circle(12.0, 12.0, 8.5);
            pen.arc(12.0, 9.5, 3.0, 180.0, 400.0);
            pen.line(&[(13.9, 11.8), (12.0, 13.2), (12.0, 14.5)]);
            pen.dot(12.0, 17.0, 1.0);
        }
        // A solid grey horseshoe magnet with white tips and a spark above it (as observed).
        Snap => {
            let grey = Color32::from_rgb((color.r() as f32 * 0.74) as u8, (color.g() as f32 * 0.74) as u8, (color.b() as f32 * 0.74) as u8);
            let thick = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(4.2 * pen.s, grey) };
            thick.arc(12.0, 13.0, 5.4, 0.0, 180.0);
            thick.line(&[(6.6, 13.0), (6.6, 9.0)]);
            thick.line(&[(17.4, 13.0), (17.4, 9.0)]);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(4.5, 6.5), pen.at(8.7, 9.0)), 0.5 * pen.s, color));
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(15.3, 6.5), pen.at(19.5, 9.0)), 0.5 * pen.s, color));
            pen.fill(&[(13.2, 1.5), (9.8, 6.8), (12.0, 6.8), (10.8, 10.5), (14.2, 5.2), (12.0, 5.2)], color);
        }
        // The view button: a small cube with the X/Y/Z axes leaving its lower corners (as observed, L22).
        Origin => {
            cube(&Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.4).max(1.0), color) }, 12.0, 13.5, 5.2);
            pen.with(theme::c(0x3d7be0)).line(&[(12.0, 8.3), (12.0, 3.5)]);
            pen.with(theme::c(0xd04a3c)).line(&[(7.5, 16.1), (3.5, 18.6)]);
            pen.with(theme::c(0x45b24a)).line(&[(16.5, 16.1), (20.5, 18.6)]);
        }
        DViews | DImage | CreateDrawing => {
            if icon == DViews {
                cube(&pen, 10.5, 10.5, 7.5);
            } else if icon == CreateDrawing {
                painter.rect_stroke(
                    Rect::from_min_max(pen.at(2.5, 5.0), pen.at(20.0, 18.5)),
                    2.5 * pen.s,
                    pen.stroke,
                    egui::StrokeKind::Middle,
                );
                cube(&pen, 11.0, 11.5, 4.2);
            } else {
                painter.rect_stroke(
                    Rect::from_min_max(pen.at(3.0, 5.0), pen.at(19.0, 17.0)),
                    2.5 * pen.s,
                    pen.stroke,
                    egui::StrokeKind::Middle,
                );
                pen.fill(&[(4.5, 15.5), (9.0, 10.0), (13.5, 15.5)], color);
                pen.dot(14.5, 9.0, 1.6);
            }
            // A white badge with a dark plus at the lower right.
            let c = pen.at(17.5, 17.5);
            painter.circle_filled(c, 5.0 * pen.s, color);
            let ink = Stroke::new(1.6 * pen.s, Tokens::button());
            painter.line_segment([c - vec2(2.6 * pen.s, 0.0), c + vec2(2.6 * pen.s, 0.0)], ink);
            painter.line_segment([c - vec2(0.0, 2.6 * pen.s), c + vec2(0.0, 2.6 * pen.s)], ink);
        }
        DDimensions => {
            pen.line(&[(4.0, 3.5), (4.0, 9.5)]);
            pen.line(&[(20.0, 3.5), (20.0, 9.5)]);
            pen.line(&[(5.5, 6.5), (18.5, 6.5)]);
            pen.line(&[(8.0, 4.5), (5.5, 6.5), (8.0, 8.5)]);
            pen.line(&[(16.0, 4.5), (18.5, 6.5), (16.0, 8.5)]);
            pen.line(&[(4.0, 19.5), (4.0, 15.0), (6.0, 13.0), (18.0, 13.0), (20.0, 15.0), (20.0, 19.5)]);
        }
        DGeometries => {
            pen.line(&[(6.0, 4.0), (6.0, 20.0)]);
            pen.line(&[(18.0, 4.0), (18.0, 20.0)]);
            for k in 0..4 {
                let y = 4.0 + k as f32 * 4.3;
                pen.line(&[(12.0, y), (12.0, y + 2.2)]);
            }
        }
        DNote => {
            painter.rect_stroke(
                Rect::from_min_max(pen.at(4.0, 4.0), pen.at(20.0, 20.0)),
                3.0 * pen.s,
                pen.stroke,
                egui::StrokeKind::Middle,
            );
            painter.text(pen.at(12.0, 12.5), egui::Align2::CENTER_CENTER, "A", crate::theme::strong(11.0 * pen.s), color);
        }
        SidebarToggle => {
            painter.rect_stroke(
                Rect::from_min_max(pen.at(3.0, 5.0), pen.at(21.0, 19.0)),
                3.0 * pen.s,
                pen.stroke,
                egui::StrokeKind::Middle,
            );
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(14.0, 7.0), pen.at(19.0, 17.0)), 1.5 * pen.s, color));
        }
        CameraSparkle => {
            pen.closed(&[(3.0, 9.0), (7.0, 9.0), (8.5, 6.5), (13.0, 6.5), (14.5, 9.0), (17.0, 9.0), (17.0, 19.0), (3.0, 19.0)]);
            pen.circle(10.0, 14.0, 3.0);
            // A four-point star at the corner.
            let c = pen.at(18.5, 17.5);
            let (a, b) = (3.5 * pen.s, 1.0 * pen.s);
            painter.add(Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -a),
                    c + vec2(b, -b),
                    c + vec2(a, 0.0),
                    c + vec2(b, b),
                    c + vec2(0.0, a),
                    c + vec2(-b, b),
                    c + vec2(-a, 0.0),
                    c + vec2(-b, -b),
                ],
                color,
                Stroke::NONE,
            ));
        }
        CParallel | CPerpendicular | CTangent | CCoincident | CMidpoint | CConcentric | CHorizVert | CEqual | CSymmetry | Disconnect
        | DistAbsolute | DistHorizontal | DistVertical | MakeConstruction => {
            let thin = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 1.1).max(1.0), color) };
            match icon {
                CParallel => {
                    thin.line(&[(4.0, 18.0), (14.0, 6.0)]);
                    thin.line(&[(10.0, 18.0), (20.0, 6.0)]);
                }
                CPerpendicular => {
                    thin.line(&[(6.0, 5.0), (6.0, 18.0), (19.0, 18.0)]);
                    thin.arc(6.0, 18.0, 9.0, 270.0, 360.0);
                }
                CTangent => {
                    // A line with two arcs leaving it either side of the touching point.
                    thin.line(&[(4.0, 9.0), (20.0, 9.0)]);
                    thin.arc(4.0, 17.0, 8.0, 270.0, 330.0);
                    thin.arc(20.0, 17.0, 8.0, 210.0, 270.0);
                    thin.dot(12.0, 9.0, 1.3);
                }
                CCoincident => {
                    thin.line(&[(4.0, 14.0), (11.0, 6.0)]);
                    thin.line(&[(9.0, 8.0), (19.0, 18.0)]);
                    thin.dot(10.0, 7.0, 1.3);
                }
                CMidpoint => {
                    thin.line(&[(5.0, 19.0), (19.0, 5.0)]);
                    thin.line(&[(6.0, 6.0), (18.0, 18.0)]);
                    thin.dot(5.0, 19.0, 1.3);
                    thin.dot(19.0, 5.0, 1.3);
                    thin.dot(12.0, 12.0, 1.3);
                }
                CConcentric => {
                    thin.circle(12.0, 12.0, 8.0);
                    thin.circle(12.0, 12.0, 5.0);
                    thin.dot(12.0, 12.0, 1.0);
                }
                CHorizVert => {
                    for k in 0..5 {
                        let x = 4.0 + k as f32 * 3.4;
                        thin.line(&[(x, 10.5), (x + 2.0, 10.5)]);
                    }
                    thin.line(&[(4.0, 13.5), (20.0, 13.5)]);
                }
                CEqual => {
                    thin.line(&[(5.0, 10.0), (19.0, 10.0)]);
                    thin.line(&[(5.0, 14.0), (19.0, 14.0)]);
                }
                CSymmetry => {
                    thin.line(&[(4.0, 4.0), (9.5, 9.0), (9.5, 15.0), (4.0, 20.0)]);
                    thin.line(&[(20.0, 4.0), (14.5, 9.0), (14.5, 15.0), (20.0, 20.0)]);
                    for k in 0..4 {
                        let y = 3.0 + k as f32 * 5.0;
                        thin.line(&[(12.0, y), (12.0, y + 2.5)]);
                    }
                }
                Disconnect => {
                    let bold = Pen { p: painter, o, s: pen.s, stroke: Stroke::new((pen.s * 2.0).max(1.0), color) };
                    bold.arc(8.0, 12.0, 4.0, 90.0, 270.0);
                    bold.line(&[(8.0, 8.0), (10.5, 8.0)]);
                    bold.line(&[(8.0, 16.0), (10.5, 16.0)]);
                    bold.arc(16.0, 12.0, 4.0, -90.0, 90.0);
                    bold.line(&[(13.5, 8.0), (16.0, 8.0)]);
                    bold.line(&[(13.5, 16.0), (16.0, 16.0)]);
                    bold.line(&[(10.0, 12.0), (14.0, 12.0)]);
                }
                // A dimension line with end ticks, slanted, level or upright.
                DistAbsolute | DistHorizontal | DistVertical => {
                    let (a, b) = match icon {
                        DistAbsolute => ((6.0, 18.0), (18.0, 6.0)),
                        DistHorizontal => ((4.0, 12.0), (20.0, 12.0)),
                        _ => ((12.0, 4.0), (12.0, 20.0)),
                    };
                    thin.line(&[a, b]);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let len = (dx * dx + dy * dy).sqrt();
                    let (px, py) = (-dy / len * 4.0, dx / len * 4.0);
                    for e in [a, b] {
                        thin.line(&[(e.0 - px, e.1 - py), (e.0 + px, e.1 + py)]);
                    }
                }
                _ => {
                    // Make Construction: a dashed square with its corners marked.
                    for (a, b) in
                        [((5.0, 6.0), (19.0, 6.0)), ((19.0, 6.0), (19.0, 18.0)), ((19.0, 18.0), (5.0, 18.0)), ((5.0, 18.0), (5.0, 6.0))]
                    {
                        for k in 1..4 {
                            let t0 = k as f32 / 4.0 - 0.08;
                            let t1 = k as f32 / 4.0 + 0.08;
                            thin.line(&[
                                (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0),
                                (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1),
                            ]);
                        }
                    }
                    for (x, y) in [(5.0, 6.0), (19.0, 6.0), (19.0, 18.0), (5.0, 18.0)] {
                        thin.dot(x, y, 1.3);
                    }
                }
            }
        }
        Lock => {
            pen.arc(12.0, 10.0, 4.0, 180.0, 360.0);
            pen.line(&[(8.0, 10.0), (8.0, 12.0)]);
            pen.line(&[(16.0, 10.0), (16.0, 12.0)]);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(6.0, 11.5), pen.at(18.0, 20.5)), 2.0 * pen.s, color));
        }
        Merged => {
            pen.closed(&[(5.5, 3.0), (14.0, 3.0), (18.5, 7.5), (18.5, 21.0), (5.5, 21.0)]);
            pen.line(&[(14.0, 3.0), (14.0, 7.5), (18.5, 7.5)]);
            pen.closed(&[(12.0, 10.0), (15.5, 12.0), (15.5, 16.0), (12.0, 18.0), (8.5, 16.0), (8.5, 12.0)]);
            pen.line(&[(8.5, 12.0), (12.0, 14.0), (15.5, 12.0)]);
            pen.line(&[(12.0, 14.0), (12.0, 18.0)]);
        }
        // Shaded: a soft rounded lump with a rounded step cut out of its lower right, lit from above, darkening
        // downwards, with a dark rim (as observed, s30-100).
        DisplayModes => {
            let arc = |out: &mut Vec<Pos2>, cx: f32, cy: f32, r: f32, a0: f32, a1: f32| {
                // Drawn on a 4..20 grid, then grown 1.15× about the middle and turned 12° clockwise like the reference.
                let (tc, ts) = (12f32.to_radians().cos(), 12f32.to_radians().sin());
                for k in 0..=8 {
                    let a = (a0 + (a1 - a0) * k as f32 / 8.0).to_radians();
                    let (x, y) = ((cx + r * a.cos() - 12.0) * 1.15, (cy + r * a.sin() - 11.5) * 1.15);
                    out.push(pen.at(11.2 + x * tc - y * ts, 12.6 + x * ts + y * tc));
                }
            };
            let mut rim = Vec::new();
            arc(&mut rim, 9.0, 9.0, 5.0, 180.0, 270.0);
            arc(&mut rim, 15.0, 9.5, 5.0, 270.0, 360.0);
            arc(&mut rim, 20.0, 15.0, 5.0, 270.0, 180.0);
            arc(&mut rim, 12.5, 16.0, 2.5, 0.0, 90.0);
            arc(&mut rim, 9.0, 14.0, 4.5, 90.0, 180.0);
            let (top, bottom) = (pen.at(0.0, 2.0).y, pen.at(0.0, 21.0).y);
            let shade = |q: Pos2| {
                let t = ((q.y - top) / (bottom - top)).clamp(0.0, 1.0).powf(0.8);
                let v = (0xae as f32 - (0xae - 0x50) as f32 * t) as u32;
                theme::fixed(v * 0x010101)
            };
            let c = pen.at(10.0, 11.5);
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(c, shade(c));
            for q in &rim {
                mesh.colored_vertex(*q, shade(*q));
            }
            let n = rim.len() as u32;
            for i in 0..n {
                mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
            }
            painter.add(mesh);
            rim.push(rim[0]);
            painter.add(Shape::line(rim, Stroke::new(1.1 * pen.s, theme::fixed(0x2c2c2c))));
        }
        DisplayZebra | DisplayCurvature => {
            let c = pen.at(12.0, 12.0);
            let r = 8.0 * pen.s;
            let (top, left, right) = match icon {
                DisplayZebra => (theme::c(0xf0f0f0), theme::c(0x1a1a1a), theme::c(0xf0f0f0)),
                _ => (theme::c(0x2ad62a), theme::c(0xe0402c), theme::c(0xe8cf2a)),
            };
            painter.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -r), c + vec2(r * 0.87, -r * 0.5), c, c + vec2(-r * 0.87, -r * 0.5)],
                top,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![c + vec2(-r * 0.87, -r * 0.5), c, c + vec2(0.0, r), c + vec2(-r * 0.87, r * 0.5)],
                left,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![c, c + vec2(r * 0.87, -r * 0.5), c + vec2(r * 0.87, r * 0.5), c + vec2(0.0, r)],
                right,
                Stroke::NONE,
            ));
            if icon == DisplayZebra {
                // Black bands across the light faces.
                let ink = Stroke::new(1.6 * pen.s, theme::c(0x1a1a1a));
                for k in [0.35, 0.7] {
                    // Across the right face, parallel to its top edge.
                    painter.line_segment([c + vec2(0.0, r * k), c + vec2(r * 0.87, r * (k - 0.5))], ink);
                }
                // Across the top face, from its left corner's side to its right corner's side.
                painter.line_segment([c + vec2(-r * 0.43, -r * 0.75), c + vec2(r * 0.43, -r * 0.25)], ink);
            }
        }
        // A solid camera, in the magnet's softer grey, with a thin light ring for the lens and a small flash dot
        // (as observed, s30-100).
        Screenshot => {
            let grey = Color32::from_rgb((color.r() as f32 * 0.8) as u8, (color.g() as f32 * 0.8) as u8, (color.b() as f32 * 0.8) as u8);
            painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(2.5, 7.5), pen.at(21.5, 19.5)), 2.2 * pen.s, grey));
            pen.fill(&[(8.0, 8.0), (9.5, 5.2), (14.5, 5.2), (16.0, 8.0)], grey);
            painter.circle_stroke(pen.at(12.0, 13.4), 3.9 * pen.s, Stroke::new(1.3 * pen.s, Tokens::button()));
            painter.circle_filled(pen.at(18.3, 10.2), 0.9 * pen.s, Tokens::button());
        }
        // A dotted grid with a small eye at its lower right (as observed).
        ShotGrid => {
            for gx in 0..4 {
                for gy in 0..4 {
                    if gx >= 2 && gy >= 2 {
                        continue;
                    }
                    painter.circle_filled(pen.at(4.0 + gx as f32 * 4.5, 4.0 + gy as f32 * 4.5), 0.9 * pen.s, color);
                }
            }
            pen.line(&[
                (12.0, 17.5),
                (15.0, 15.0),
                (18.0, 14.2),
                (21.0, 15.0),
                (23.0, 17.5),
                (21.0, 20.0),
                (18.0, 20.8),
                (15.0, 20.0),
                (12.0, 17.5),
            ]);
            painter.circle_filled(pen.at(17.5, 17.5), 1.4 * pen.s, color);
        }
        // A checkerboard: see-through.
        ShotChecker => {
            let grey = Color32::from_rgb((color.r() as f32 * 0.55) as u8, (color.g() as f32 * 0.55) as u8, (color.b() as f32 * 0.55) as u8);
            for cx in 0..4 {
                for cy in 0..4 {
                    let fill = if (cx + cy) % 2 == 0 { color } else { grey };
                    let x = 4.0 + cx as f32 * 4.0;
                    let y = 4.0 + cy as f32 * 4.0;
                    painter.add(Shape::rect_filled(Rect::from_min_max(pen.at(x, y), pen.at(x + 4.0, y + 4.0)), 0.0, fill));
                }
            }
        }
        // A rounded box with "1x" in it.
        ShotResolution => {
            pen.closed(&[(3.0, 6.0), (21.0, 6.0), (21.0, 18.0), (3.0, 18.0)]);
            painter.text(pen.at(12.0, 12.2), egui::Align2::CENTER_CENTER, "1x", egui::FontId::proportional(7.5 * pen.s), color);
        }
        // A clock with firm hands at twelve and nine (as observed, s30-100).
        History => {
            let (_, grey) = two_tone(color);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.5 * pen.s, grey) }.circle(12.0, 12.0, 8.5);
            Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.9 * pen.s, color) }.line(&[(12.0, 6.8), (12.0, 12.0), (7.6, 12.0)]);
        }
        // An open circle with an arrowhead at its top end (as observed in the tutorial, s30-035).
        Retry => {
            pen.arc(12.0, 12.5, 6.5, -60.0, 230.0);
            pen.line(&[(4.6, 5.6), (7.9, 7.6), (9.3, 4.0)]);
        }
        Undo | Redo => {
            // Thin and grey.
            let (_, grey) = two_tone(color);
            let pen = Pen { p: painter, o, s: pen.s, stroke: Stroke::new(1.4 * pen.s, grey) };
            pen.circle(12.0, 12.0, 9.5);
            // Drawn for Undo (head at the left); Redo is its mirror image.
            let m = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> {
                pts.iter().map(|&(x, y)| if matches!(icon, Redo) { (24.0 - x, y) } else { (x, y) }).collect()
            };
            pen.line(&m(&[(10.2, 7.4), (7.6, 10.0), (10.2, 12.6)]));
            pen.line(&m(&[(7.9, 10.0), (12.6, 10.0)]));
            if matches!(icon, Redo) {
                pen.arc(11.4, 12.9, 2.9, 90.0, 270.0);
            } else {
                pen.arc(12.6, 12.9, 2.9, -90.0, 90.0);
            }
            pen.line(&m(&[(12.6, 15.8), (10.6, 15.8)]));
        }
        Folder => pen.closed(&[(3.0, 7.0), (9.0, 7.0), (11.0, 9.0), (21.0, 9.0), (21.0, 19.0), (3.0, 19.0)]),
        NewFolder => {
            pen.closed(&[(3.0, 7.0), (9.0, 7.0), (11.0, 9.0), (21.0, 9.0), (21.0, 19.0), (3.0, 19.0)]);
            pen.line(&[(12.0, 11.5), (12.0, 16.5)]);
            pen.line(&[(9.5, 14.0), (14.5, 14.0)]);
        }
        Plus => {
            pen.line(&[(12.0, 5.0), (12.0, 19.0)]);
            pen.line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Import => {
            pen.line(&[(4.0, 13.0), (4.0, 20.0), (20.0, 20.0), (20.0, 13.0)]);
            pen.line(&[(12.0, 4.0), (12.0, 15.0)]);
            pen.line(&[(8.0, 11.0), (12.0, 15.0), (16.0, 11.0)]);
        }
        Export => {
            pen.line(&[(4.0, 13.0), (4.0, 20.0), (20.0, 20.0), (20.0, 13.0)]);
            pen.line(&[(12.0, 15.0), (12.0, 4.0)]);
            pen.line(&[(8.0, 8.0), (12.0, 4.0), (16.0, 8.0)]);
        }
        Sort => {
            pen.line(&[(8.0, 4.0), (8.0, 20.0)]);
            pen.line(&[(4.5, 7.5), (8.0, 4.0), (11.5, 7.5)]);
            pen.line(&[(16.0, 4.0), (16.0, 20.0)]);
            pen.line(&[(12.5, 16.5), (16.0, 20.0), (19.5, 16.5)]);
        }
        Recents => {
            blue.circle(12.0, 12.0, 8.0);
            blue.line(&[(12.0, 7.5), (12.0, 12.0), (15.0, 14.0)]);
        }
        Projects => {
            blue.closed(&[(5.0, 3.5), (14.0, 3.5), (19.0, 8.5), (19.0, 20.5), (5.0, 20.5)]);
            blue.line(&[(14.0, 3.5), (14.0, 8.5), (19.0, 8.5)]);
        }
        // Three bumps on a flat base; each arc starts where the last one meets it.
        Cloud => {
            blue.arc(8.0, 14.5, 3.5, 90.0, 270.0);
            blue.arc(12.5, 11.0, 4.5, 180.0, 360.5);
            blue.arc(16.5, 14.5, 3.5, 278.0, 450.0);
            blue.line(&[(8.0, 18.0), (16.5, 18.0)]);
        }
        GetStarted => {
            blue.line(&[
                (12.0, 3.0),
                (13.5, 10.5),
                (21.0, 12.0),
                (13.5, 13.5),
                (12.0, 21.0),
                (10.5, 13.5),
                (3.0, 12.0),
                (10.5, 10.5),
                (12.0, 3.0),
            ]);
        }
        Tutorials => {
            blue.closed(&[(2.5, 9.0), (12.0, 4.5), (21.5, 9.0), (12.0, 13.5)]);
            blue.line(&[(6.5, 11.0), (6.5, 16.0), (12.0, 18.5), (17.5, 16.0), (17.5, 11.0)]);
        }
        ChevronLeft => pen.line(&[(15.0, 5.0), (8.0, 12.0), (15.0, 19.0)]),
        ChevronRight => pen.line(&[(9.0, 5.0), (16.0, 12.0), (9.0, 19.0)]),
        ChevronDown => pen.line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        ChevronUp => pen.line(&[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]),
        Filter => {
            pen.circle(12.0, 12.0, 8.5);
            pen.line(&[(8.0, 9.5), (16.0, 9.5)]);
            pen.line(&[(9.5, 12.5), (14.5, 12.5)]);
            pen.line(&[(11.0, 15.5), (13.0, 15.5)]);
        }
        // A small stack of bars on a stand.
        Breakpoint => {
            pen.fill(&[(7.0, 4.0), (17.0, 4.0), (17.0, 9.0), (7.0, 9.0)], color);
            pen.fill(&[(7.0, 10.5), (17.0, 10.5), (17.0, 15.5), (7.0, 15.5)], color);
            pen.line(&[(9.0, 16.0), (9.0, 20.5)]);
            pen.line(&[(15.0, 16.0), (15.0, 20.5)]);
        }
        Suppress => {
            pen.circle(12.0, 12.0, 8.0);
            pen.line(&[(8.0, 12.0), (16.0, 12.0)]);
        }
        Unsuppress => {
            pen.circle(12.0, 12.0, 8.0);
            pen.line(&[(8.0, 12.0), (16.0, 12.0)]);
            pen.line(&[(12.0, 8.0), (12.0, 16.0)]);
        }
        Pencil => {
            pen.line(&[(5.0, 19.0), (6.0, 15.0), (16.0, 5.0), (19.0, 8.0), (9.0, 18.0), (5.0, 19.0)]);
        }
        Warning => {
            pen.fill(&[(12.0, 2.5), (22.5, 20.5), (1.5, 20.5)], theme::c(0xff3c30));
            let white = pen.with(Color32::WHITE);
            white.line(&[(12.0, 8.5), (12.0, 14.0)]);
            white.dot(12.0, 17.3, 1.2);
        }
        Agent => {
            pen.rect(5.0, 8.0, 19.0, 19.0);
            pen.line(&[(12.0, 8.0), (12.0, 4.5)]);
            pen.dot(12.0, 4.0, 1.4);
            blue.dot(9.0, 13.0, 1.5);
            blue.dot(15.0, 13.0, 1.5);
            pen.line(&[(9.5, 16.0), (14.5, 16.0)]);
        }
    }
}

/// Points around an ellipse (a regular polygon when `n` is small), starting at angle `a0` degrees.
fn ring(cx: f32, cy: f32, rx: f32, ry: f32, n: usize, a0: f32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = (a0 + i as f32 / n as f32 * 360.0).to_radians();
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

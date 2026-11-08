use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::{DVec2, DVec3};
use shapecraft_doc::{Document, ItemId, Pick, Sketch, SketchPlane};

use crate::{Camera, Rgba, SceneColors};

/// A screen-space-width line segment.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: DVec3,
    pub b: DVec3,
    pub color: Rgba,
    pub width: f32,
    /// Drawn on top of everything (sketch lines, previews), else depth-tested.
    pub overlay: bool,
    /// Cut away by the section plane (body edges).
    pub clip: bool,
}

/// One of the translucent plane tiles shown when choosing a sketch plane.
#[derive(Clone, Copy, Debug)]
pub struct PlaneTile {
    pub corners: [DVec3; 4],
    pub hovered: bool,
    /// Picked as a tool's target (Split Body): purple.
    pub picked: bool,
}

/// Sketch elements the constraints fully define (measured s14-145).
const DEFINED: Rgba = Rgba::hex(0x2ecd05);

/// Transient geometry drawn by an active tool (rubber bands, previews, handles).
#[derive(Clone, Debug)]
pub enum Overlay {
    Line { a: DVec3, b: DVec3, color: Rgba, width: f32 },
    Dot { p: DVec3, color: Rgba, size: f32 },
    Fill { tris: Vec<[DVec3; 3]>, color: Rgba },
}

/// How bodies are drawn (View ▸ Wireframe / X-Ray / Shaded).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayMode {
    #[default]
    Shaded,
    Wireframe,
    XRay,
    /// Surface Analysis ▸ Zebra: black and white reflection stripes, horizontal or vertical, `scale` in percent.
    Zebra {
        vertical: bool,
        scale: u16,
    },
    /// Surface Analysis ▸ Curvature Map: Gaussian curvature, blue (saddles) – green (flat, cylinders) – red (domes).
    Curvature {
        scale: u16,
    },
    /// Shaders ▸ Visualized (View ▸ Visualized ⌥V): the Visualize look (no edges, grid or axes, the backdrop
    /// and floor shadow) while modeling goes on as usual.
    Visualized,
}

impl DisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Shaded => "Shaded",
            DisplayMode::Wireframe => "Wireframe",
            DisplayMode::XRay => "X-Ray",
            DisplayMode::Zebra { .. } => "Zebra",
            DisplayMode::Curvature { .. } => "Curvature Map",
            DisplayMode::Visualized => "Visualized",
        }
    }

    /// Solid bodies, drawn like Shaded (the surface analyses only change the colour).
    pub fn solid(self) -> bool {
        matches!(self, DisplayMode::Shaded | DisplayMode::Zebra { .. } | DisplayMode::Curvature { .. } | DisplayMode::Visualized)
    }

    /// For the shader: mode (0 none, 1 zebra, 2 curvature), vertical, scale.
    pub fn analysis(self) -> [f32; 4] {
        match self {
            DisplayMode::Zebra { vertical, scale } => [1.0, if vertical { 1.0 } else { 0.0 }, scale as f32 / 100.0, 0.0],
            DisplayMode::Curvature { scale } => [2.0, 0.0, scale as f32 / 100.0, 0.0],
            _ => [0.0; 4],
        }
    }
}

pub struct SceneInput<'a> {
    pub doc: &'a Document,
    pub selection: &'a [Pick],
    pub hover: Option<Pick>,
    pub active_sketch: Option<ItemId>,
    pub plane_tiles: &'a [PlaneTile],
    pub overlays: &'a [Overlay],
    pub colors: SceneColors,
    pub show_axes: bool,
    /// View options from the viewport's right-click menu.
    pub show_edges: bool,
    pub show_hidden_edges: bool,
    /// View ▸ Show Decals.
    pub show_decals: bool,
    /// Draw the grid on this plane (the active sketch's, or the floor while picking a plane).
    pub grid: Option<SketchPlane>,
    pub display: DisplayMode,
    /// Section View: keep what's behind the plane through `.0` with normal `.1` (the normal points at the
    /// part that is cut away).
    pub section: Option<(DVec3, DVec3)>,
    /// Visualization: no edges, grid or axes, and a studio backdrop.
    pub presentation: bool,
    /// Visualization's backdrop color and how dark the contact shadow under the bodies is.
    pub backdrop: Rgba,
    pub shadow: f32,
    /// Bodies drawn see-through in a color, their hidden edges showing (a boolean's target and tools, observed 383).
    pub ghosts: &'a [(ItemId, Rgba)],
    /// Bodies shaded in another color, solid, edges as usual (a split's pieces before Done, observed s12-018).
    pub tints: &'a [(ItemId, Rgba)],
    /// Selected sketch elements that were just drawn: they stay blue (picked ones turn orange), as observed.
    pub drawn: &'a [Pick],
    /// For the sketch being edited: which of its elements the constraints pin down (drawn green, s14-145).
    pub defined: &'a [bool],
}

#[derive(Default)]
pub(crate) struct Built {
    pub segments: Vec<Segment>,
    /// How many of `segments` (the grid and the axes) go under the bodies' edges.
    pub under_edges: usize,
    pub dots: Vec<(DVec3, Rgba, f32)>,
    /// (triangles, color, overlay)
    pub fills: Vec<([DVec3; 3], Rgba, bool)>,
}

/// What the bodies' edges depend on. They don't depend on the camera, and a big model has far more of them than
/// everything else in the view put together, so they're only rebuilt when this changes.
pub(crate) fn edges_key(scene: &SceneInput) -> u64 {
    let mut h = DefaultHasher::new();
    for body in scene.doc.bodies.iter().filter(|b| b.visible) {
        body.id.hash(&mut h);
        Arc::as_ptr(&body.mesh).hash(&mut h);
    }
    scene.selection.hash(&mut h);
    scene.hover.hash(&mut h);
    scene.ghosts.hash(&mut h);
    (scene.active_sketch.is_some(), scene.presentation, scene.show_edges, scene.show_hidden_edges, scene.display).hash(&mut h);
    (scene.colors.edge, scene.colors.selected, scene.colors.hover).hash(&mut h);
    h.finish()
}

/// Every visible body's edges.
pub(crate) fn body_edges(scene: &SceneInput) -> Vec<Segment> {
    let c = &scene.colors;
    let mut out = Built::default();
    for body in scene.doc.bodies.iter().filter(|b| b.visible) {
        // The Visualize look draws no edges, only the outline of what's picked (a picked face isn't filled there).
        if scene.presentation {
            for (i, (line, &edge)) in body.mesh.edges.iter().zip(&body.mesh.edge_ids).enumerate() {
                let picked = scene.selection.contains(&Pick::Edge { body: body.id, edge })
                    || body.mesh.edge_faces.get(i).is_some_and(|ab| {
                        ab.iter().any(|f| *f != u32::MAX && scene.selection.contains(&Pick::Face { body: body.id, face: *f }))
                    });
                if picked {
                    for w in line.windows(2) {
                        let a = DVec3::new(w[0][0] as f64, w[0][1] as f64, w[0][2] as f64);
                        let b = DVec3::new(w[1][0] as f64, w[1][1] as f64, w[1][2] as f64);
                        out.segments.push(Segment { a, b, color: c.selected, width: 2.4, overlay: false, clip: true });
                    }
                }
            }
            continue;
        }
        let selected_body = fully_selected(body, scene.selection);
        let hovered_body = scene.hover == Some(Pick::Body { body: body.id });
        let hovered_face = match scene.hover {
            Some(Pick::Face { body: b, face }) if b == body.id => Some(face),
            _ => None,
        };
        let solid_view = scene.display.solid() && scene.active_sketch.is_none();
        for (i, (line, &edge)) in body.mesh.edges.iter().zip(&body.mesh.edge_ids).enumerate() {
            let pick = Pick::Edge { body: body.id, edge };
            let around_hovered_face = hovered_face.is_some_and(|f| body.mesh.edge_faces.get(i).is_some_and(|ab| ab.contains(&f)));
            // Picked edges, the outline of picked faces and of a body hovered in the Items list also show faintly
            // where faces hide them (a pocket's floor, a body inside another), as the faces do in the renderer.
            let around_picked_face =
                body.mesh.edge_faces.get(i).is_some_and(|ab| {
                    ab.iter().any(|f| *f != u32::MAX && scene.selection.contains(&Pick::Face { body: body.id, face: *f }))
                });
            let through = if scene.selection.contains(&pick) {
                Some((c.selected.with_alpha(150), 2.0))
            } else if around_picked_face {
                Some((c.selected.with_alpha(130), 1.4))
            } else if hovered_body {
                Some((c.hover.with_alpha(110), 1.2))
            } else {
                None
            };
            if let Some((color, width)) = through.filter(|_| solid_view) {
                for w in line.windows(2) {
                    let a = DVec3::new(w[0][0] as f64, w[0][1] as f64, w[0][2] as f64);
                    let b = DVec3::new(w[1][0] as f64, w[1][1] as f64, w[1][2] as f64);
                    out.segments.push(Segment { a, b, color, width, overlay: true, clip: true });
                }
            }
            let (color, width) = if scene.selection.contains(&pick) {
                (c.selected, 3.0)
            } else if scene.hover == Some(pick) {
                (c.hover, 3.0)
            } else if around_hovered_face || hovered_body {
                // The reference outlines what's under the pointer in cyan.
                (c.hover, 2.4)
            } else if let Some((_, g)) = scene.ghosts.iter().find(|(id, _)| *id == body.id) {
                // Its edges show through it (and through what's in front).
                for w in line.windows(2) {
                    let a = DVec3::new(w[0][0] as f64, w[0][1] as f64, w[0][2] as f64);
                    let b = DVec3::new(w[1][0] as f64, w[1][1] as f64, w[1][2] as f64);
                    out.segments.push(Segment { a, b, color: g.with_alpha(200), width: 1.4, overlay: true, clip: true });
                }
                continue;
            } else if selected_body {
                // A selected body's own hidden edges stay hidden: it reads as one solid tinted body.
                (c.selected, 1.4)
            } else if scene.display.solid() {
                (c.edge, 1.5)
            } else {
                (Rgba::hex(0xbdbdc0), 1.2)
            };
            let highlighted = color != c.edge || !scene.display.solid();
            if !scene.show_edges && !highlighted {
                continue;
            }
            // Without opaque faces every edge shows, so they go on top.
            let overlay = !scene.display.solid();
            if scene.show_hidden_edges && !overlay {
                // A faint copy on top shows the edges behind faces.
                for w in line.windows(2) {
                    let a = DVec3::new(w[0][0] as f64, w[0][1] as f64, w[0][2] as f64);
                    let b = DVec3::new(w[1][0] as f64, w[1][1] as f64, w[1][2] as f64);
                    out.segments.push(Segment { a, b, color: color.with_alpha(70), width: 1.0, overlay: true, clip: true });
                }
            }
            for w in line.windows(2) {
                let a = DVec3::new(w[0][0] as f64, w[0][1] as f64, w[0][2] as f64);
                let b = DVec3::new(w[1][0] as f64, w[1][1] as f64, w[1][2] as f64);
                out.segments.push(Segment { a, b, color, width, overlay, clip: true });
            }
        }
    }
    out.segments
}

pub(crate) fn build(scene: &SceneInput, camera: &Camera, size: DVec2) -> Built {
    let c = &scene.colors;
    let mut out = Built::default();
    let wpp = camera.world_per_pixel(size.y);

    if let Some(plane) = scene.grid.filter(|_| !scene.presentation) {
        let first = out.segments.len();
        grid(&mut out, plane, camera, wpp, c);
        // While sketching, the plane's grid shows through the faded bodies, as observed.
        if scene.active_sketch.is_some() {
            for s in &mut out.segments[first..] {
                s.overlay = true;
            }
        }
    }
    if scene.show_axes && !scene.presentation {
        let len = camera.distance * 50.0;
        // Bodies hide the axes (s22-002: no trace of the Y axis across two boxes it runs under); they only show
        // through a selected body, which turns see-through.
        for (dir, col) in [(DVec3::X, c.axis_x), (DVec3::Y, c.axis_y), (DVec3::Z, c.axis_z)] {
            out.segments.push(Segment { a: DVec3::ZERO, b: dir * len, color: col, width: 1.0, overlay: false, clip: false });
        }
    }
    out.under_edges = out.segments.len();

    for sketch in &scene.doc.sketches {
        let active = scene.active_sketch == Some(sketch.id);
        if !sketch.visible && !active {
            continue;
        }
        sketch_geometry(&mut out, scene, sketch, active, c);
    }

    for pl in scene.doc.planes.iter().filter(|p| p.visible) {
        let pick = Pick::Plane { plane: pl.id };
        let (fill, edge) = if scene.selection.contains(&pick) {
            (c.selected.with_alpha(60), c.selected)
        } else if scene.hover == Some(pick) {
            (c.plane_tile_hover, c.hover)
        } else {
            (c.plane_tile.with_alpha(40), c.plane_tile.with_alpha(170))
        };
        let [a, b, cc, d] = pl.corners();
        out.fills.push(([a, b, cc], fill, false));
        out.fills.push(([a, cc, d], fill, false));
        for (p, q) in [(a, b), (b, cc), (cc, d), (d, a)] {
            out.segments.push(Segment { a: p, b: q, color: edge, width: 1.2, overlay: false, clip: false });
        }
    }

    // Reference pictures are drawn by the renderer; selected or hovered ones get an outline.
    for im in scene.doc.images.iter().filter(|i| i.visible) {
        let pick = Pick::Image { image: im.id };
        let edge = if scene.selection.contains(&pick) {
            Some(c.selected)
        } else if scene.hover == Some(pick) {
            Some(c.hover)
        } else {
            None
        };
        if let Some(edge) = edge {
            let [a, b, cc, d] = im.corners();
            for (p, q) in [(a, b), (b, cc), (cc, d), (d, a)] {
                out.segments.push(Segment { a: p, b: q, color: edge, width: 2.0, overlay: false, clip: false });
            }
        }
    }

    for ax in scene.doc.axes.iter().filter(|a| a.visible) {
        let pick = Pick::Axis { axis: ax.id };
        let (color, width) = if scene.selection.contains(&pick) {
            (c.selected, 3.0)
        } else if scene.hover == Some(pick) {
            (c.hover, 3.0)
        } else {
            (c.plane_tile.with_alpha(230), 1.6)
        };
        let (a, b) = ax.ends();
        out.segments.push(Segment { a, b, color, width, overlay: false, clip: false });
        out.dots.push((ax.origin, color, 6.0));
    }

    let v = &scene.doc.visual;
    if scene.presentation && scene.shadow > 0.0 && v.ground != shapecraft_doc::Ground::Off && v.object_shadow {
        cast_shadow(&mut out, scene);
        contact_shadow(&mut out, scene);
    }

    for tile in scene.plane_tiles {
        let [a, b, cc, d] = tile.corners;
        let col = if tile.picked {
            Rgba::hex(0x8a7cf0).with_alpha(200)
        } else if tile.hovered {
            c.plane_tile_hover
        } else {
            c.plane_tile
        };
        // Tiles are pick targets, so they stay visible even inside or behind bodies.
        out.fills.push(([a, b, cc], col, true));
        out.fills.push(([a, cc, d], col, true));
        let edge = if tile.picked {
            Rgba::hex(0x8a7cf0)
        } else if tile.hovered {
            c.plane_tile_hover.with_alpha(220)
        } else {
            c.plane_tile.with_alpha(150)
        };
        for (p, q) in [(a, b), (b, cc), (cc, d), (d, a)] {
            out.segments.push(Segment { a: p, b: q, color: edge, width: 1.0, overlay: true, clip: false });
        }
    }

    for o in scene.overlays {
        match o {
            Overlay::Line { a, b, color, width } => {
                out.segments.push(Segment { a: *a, b: *b, color: *color, width: *width, overlay: true, clip: false })
            }
            Overlay::Dot { p, color, size } => out.dots.push((*p, *color, *size)),
            Overlay::Fill { tris, color } => out.fills.extend(tris.iter().map(|t| (*t, *color, true))),
        }
    }
    out
}

fn sketch_geometry(out: &mut Built, scene: &SceneInput, sketch: &Sketch, active: bool, c: &SceneColors) {
    let plane = sketch.plane;
    for region in sketch.regions() {
        let pick = Pick::Region { sketch: sketch.id, region: region.index };
        let color = if scene.selection.contains(&pick) {
            Some(c.selected_pre.with_alpha(235))
        } else if scene.hover == Some(pick) {
            Some(c.region_hover)
        } else {
            None
        };
        // Every closed region is tinted faint blue, in the sketch being edited too (26.202: a finished rectangle or
        // circle reads as a profile at once; 26.200 left them plain while editing).
        let color = color.or((active || sketch.visible).then_some(c.region_tint));
        if let Some(color) = color {
            for t in &region.triangles {
                out.fills.push(([plane.to_world(t[0]), plane.to_world(t[1]), plane.to_world(t[2])], color, true));
            }
        }
    }
    for (i, e) in sketch.entities.iter().enumerate() {
        let pick = Pick::SketchEntity { sketch: sketch.id, entity: i as u32 };
        let selected = scene.selection.contains(&pick);
        // Picked sketch elements are orange; just drawn ones stay blue (with an orange center point), as observed.
        let color = if selected && !scene.drawn.contains(&pick) {
            c.sketch_selected
        } else if scene.hover == Some(pick) || scene.hover == Some(Pick::Sketch { sketch: sketch.id }) {
            c.sketch_hover
        } else if active && scene.defined.get(i).copied().unwrap_or(false) {
            DEFINED
        } else {
            c.sketch_line
        };
        let point_color = if active && scene.defined.get(i).copied().unwrap_or(false) { DEFINED } else { c.sketch_point };
        let width = if active { 1.7 } else { 1.5 };
        // Construction geometry is dashed (dashes sized to the sketch, so they read at any zoom of it).
        let construction = sketch.construction.contains(&(i as u32));
        let dash = sketch.entities.iter().flat_map(|e| e.outline()).fold(0.0f64, |m, p| m.max(p.length())).max(1.0) * 0.015;
        for w in e.outlines().iter().flat_map(|o| o.windows(2)) {
            let pieces = if construction { dashes(w[0], w[1], dash) } else { vec![(w[0], w[1])] };
            for (a, b) in pieces {
                out.segments.push(Segment {
                    a: plane.to_world(a),
                    b: plane.to_world(b),
                    color,
                    width,
                    // Sketch curves show through bodies, as observed (a sketch under a box shows through its sides).
                    overlay: true,
                    clip: false,
                });
            }
        }
        if active {
            // As observed: a control spline keeps its control polygon up, dashed purple, with a dot on every pole.
            if let shapecraft_doc::SketchEntity::ControlSpline { poles } = e {
                for w in poles.windows(2) {
                    for (a, b) in dashes(w[0], w[1], dash * 2.5) {
                        out.segments.push(Segment {
                            a: plane.to_world(a),
                            b: plane.to_world(b),
                            color: Rgba::hex(0x8b5cf6),
                            width: 1.0,
                            overlay: true,
                            clip: false,
                        });
                    }
                }
                for p in poles.iter().skip(1).take(poles.len().saturating_sub(2)) {
                    out.dots.push((plane.to_world(*p), c.sketch_point, 5.0));
                }
            }
            // 26.200 (s11-030/031): a line's free end is a dot, an end joined to another curve a hollow ring, and
            // its midpoint a small purple dot.
            if let shapecraft_doc::SketchEntity::Line { a, b } = *e {
                for p in [a, b] {
                    let joined = sketch.entities.iter().enumerate().any(|(j, o)| j != i && ends_of(o).iter().any(|q| q.distance(p) < 1e-7));
                    out.dots.push((plane.to_world(p), point_color, if joined { -9.5 } else { 5.0 }));
                }
                out.dots.push((plane.to_world((a + b) / 2.0), Rgba::hex(0xb266ff), 4.0));
            } else if let shapecraft_doc::SketchEntity::Polygon { center, radius, sides, angle } = *e {
                // Corners join two sides: rings, like joined line ends; the middle a dot.
                for p in shapecraft_doc::SketchEntity::polygon_points(center, radius, sides, angle).into_iter().take(sides as usize) {
                    out.dots.push((plane.to_world(p), point_color, -9.5));
                }
                out.dots.push((plane.to_world(center), point_color, 5.0));
            } else {
                for p in key_points(e) {
                    out.dots.push((plane.to_world(p), point_color, 5.0));
                }
            }
            // A picked rectangle shows one orange point at its middle, with a soft ring (s11-056); other picked
            // curves their own center point.
            let rect = shapecraft_doc::rectangle_loop(&sketch.entities, i as u32);
            if selected && let Some(lp) = rect {
                if lp.iter().min() == Some(&(i as u32))
                    && let Some(f) = shapecraft_doc::rectangle_frame(&sketch.entities, lp)
                {
                    let [p0, _, p2, _] = f.corners();
                    let m = plane.to_world((p0 + p2) / 2.0);
                    out.dots.push((m, c.selected_pre.with_alpha(110), 16.0));
                    out.dots.push((m, c.selected_pre, 8.0));
                }
            } else if selected && let Some(center) = center_point(e) {
                out.dots.push((plane.to_world(center), c.selected_pre, 9.0));
            }
        }
    }
}

/// The dashes of a dashed segment: `dash` long with gaps of 0.7 dash, the phase running from `a`.
fn dashes(a: DVec2, b: DVec2, dash: f64) -> Vec<(DVec2, DVec2)> {
    let len = a.distance(b);
    if len < 1e-12 || dash <= 0.0 {
        return Vec::new();
    }
    let dir = (b - a) / len;
    let mut out = Vec::new();
    let mut t = 0.0;
    while t < len {
        out.push((a + dir * t, a + dir * (t + dash).min(len)));
        t += dash * 1.7;
    }
    out
}

/// Corners, ends, centres and midpoints: the dots the reference app draws on sketch elements.
/// Where a curve ends (none for closed ones).
fn ends_of(e: &shapecraft_doc::SketchEntity) -> Vec<DVec2> {
    use shapecraft_doc::SketchEntity::*;
    match e {
        Line { a, b } | Arc { a, b, .. } => vec![*a, *b],
        Spline { points } => points.first().into_iter().chain(points.last()).copied().collect(),
        ControlSpline { poles } => poles.first().into_iter().chain(poles.last()).copied().collect(),
        _ => Vec::new(),
    }
}

fn key_points(e: &shapecraft_doc::SketchEntity) -> Vec<DVec2> {
    use shapecraft_doc::SketchEntity::*;
    match *e {
        Line { a, b } => vec![a, b, (a + b) / 2.0],
        Rectangle { min, max } => {
            let c = (min + max) / 2.0;
            vec![
                min,
                max,
                DVec2::new(min.x, max.y),
                DVec2::new(max.x, min.y),
                DVec2::new(c.x, min.y),
                DVec2::new(c.x, max.y),
                DVec2::new(min.x, c.y),
                DVec2::new(max.x, c.y),
                c,
            ]
        }
        Circle { center, .. } => vec![center],
        Polygon { center, radius, sides, angle } => {
            let mut v = shapecraft_doc::SketchEntity::polygon_points(center, radius, sides, angle);
            v.push(center);
            v
        }
        Arc { a, mid, b } => vec![a, mid, b],
        Ellipse { center, .. } => vec![center],
        Spline { ref points } => points.clone(),
        // The curve passes through its first and last control points only.
        ControlSpline { ref poles } => poles.first().into_iter().chain(poles.last()).copied().collect(),
        Text { origin, .. } => vec![origin],
    }
}

fn center_point(e: &shapecraft_doc::SketchEntity) -> Option<DVec2> {
    use shapecraft_doc::SketchEntity::*;
    match *e {
        Rectangle { min, max } => Some((min + max) / 2.0),
        Circle { center, .. } => Some(center),
        Line { a, b } => Some((a + b) / 2.0),
        Polygon { center, .. } => Some(center),
        Arc { a, mid, b } => shapecraft_doc::circle_through(a, mid, b).map(|(c, _)| c),
        Ellipse { center, .. } => Some(center),
        Spline { .. } | ControlSpline { .. } | Text { .. } => None,
    }
}

/// A grid on the plane: minor lines at a step that keeps ~12 px spacing, major every 10.
/// The sketch grid's spacing at this zoom (world units per pixel): 1, 2·5 or 5 steps of ten.
pub fn grid_step(wpp: f64) -> f64 {
    let raw = (wpp * 14.0).max(1e-6);
    let mut step = 10f64.powf(raw.log10().ceil());
    if step / 2.0 >= raw {
        step /= 2.0;
    }
    step
}

fn grid(out: &mut Built, plane: SketchPlane, camera: &Camera, wpp: f64, c: &SceneColors) {
    let step = grid_step(wpp);
    let center = plane.to_local(camera.target);
    let half = (camera.distance * 1.5).max(step * 10.0);
    let n = ((half / step).ceil() as i64).min(400);
    let cx = (center.x / step).round() as i64;
    let cy = (center.y / step).round() as i64;
    for i in -n..=n {
        for (axis, k) in [(0, cx + i), (1, cy + i)] {
            let v = k as f64 * step;
            let major = k.rem_euclid(10) == 0;
            let (a, b) = if axis == 0 {
                (DVec2::new(v, (cy - n) as f64 * step), DVec2::new(v, (cy + n) as f64 * step))
            } else {
                (DVec2::new((cx - n) as f64 * step, v), DVec2::new((cx + n) as f64 * step, v))
            };
            out.segments.push(Segment {
                a: plane.to_world(a),
                b: plane.to_world(b),
                color: if major { c.grid_major } else { c.grid_minor },
                width: 1.0,
                overlay: false,
                clip: false,
            });
        }
    }
}

/// The floor the shadow falls on: under the lowest body, or the XY plane.
fn floor(scene: &SceneInput) -> Option<f64> {
    let low = scene
        .doc
        .bodies
        .iter()
        .filter(|b| b.visible)
        .flat_map(|b| b.mesh.positions.iter())
        .map(|p| p[2] as f64)
        .fold(f64::INFINITY, f64::min);
    if !low.is_finite() {
        return None;
    }
    match scene.doc.visual.ground {
        shapecraft_doc::Ground::Xy => Some(0.0),
        _ => Some(low),
    }
}

/// The bodies' shadow cast by the environment's light onto the floor (as observed it turns with Light Rotation and
/// shortens as the sun climbs). Drawn in the backdrop's color darkened, opaque, so overlapping triangles don't add up.
fn cast_shadow(out: &mut Built, scene: &SceneInput) {
    let Some(z) = floor(scene) else { return };
    let l = scene.doc.visual.light_dir();
    if l.z < 1e-3 {
        return;
    }
    let z = z - 1e-4 * (1.0 + z.abs());
    let k = 1.0 - (scene.shadow * 0.9).clamp(0.0, 1.0);
    let Rgba(r, g, b, _) = scene.backdrop;
    let dark = |c: u8| (c as f32 * k) as u8;
    // Over a panorama there's no one floor color: the shadow darkens what's behind it instead.
    let color = if scene.doc.visual.panorama().is_some() {
        Rgba(0, 0, 0, ((1.0 - k) * 0.75 * 255.0) as u8)
    } else {
        Rgba(dark(r), dark(g), dark(b), 255)
    };
    let drop = |p: [f32; 3]| {
        let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        p - l * ((p.z - z) / l.z)
    };
    for body in scene.doc.bodies.iter().filter(|b| b.visible) {
        let m = &body.mesh;
        for t in m.indices.as_chunks::<3>().0 {
            let (Some(a), Some(b), Some(c)) =
                (m.positions.get(t[0] as usize), m.positions.get(t[1] as usize), m.positions.get(t[2] as usize))
            else {
                continue;
            };
            // Only the triangles that face the light: together they cover the shadow.
            let (pa, pb, pc) = (DVec3::from(a.map(f64::from)), DVec3::from(b.map(f64::from)), DVec3::from(c.map(f64::from)));
            if (pb - pa).cross(pc - pa).dot(l) <= 0.0 {
                continue;
            }
            out.fills.push(([drop(*a), drop(*b), drop(*c)], color, false));
        }
    }
}

/// A soft shadow on the floor under the visible bodies, rings fading out from the middle (Visualization).
fn contact_shadow(out: &mut Built, scene: &SceneInput) {
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for b in scene.doc.bodies.iter().filter(|b| b.visible) {
        for p in &b.mesh.positions {
            let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
            min = min.min(p);
            max = max.max(p);
        }
    }
    if !min.is_finite() {
        return;
    }
    let z = floor(scene).unwrap_or(min.z);
    let c = DVec3::new((min.x + max.x) / 2.0, (min.y + max.y) / 2.0, z - (max - min).max_element() * 1e-3);
    let half = DVec2::new((max.x - min.x) / 2.0, (max.y - min.y) / 2.0);
    let spread = half.max_element() * 0.35 + 1e-6;
    let rings = 10;
    let segs = 48;
    let point = |k: f64, t: f64| c + DVec3::new((half.x + spread * k) * t.cos(), (half.y + spread * k) * t.sin(), 0.0);
    for ring in 0..rings {
        let (k0, k1) = (ring as f64 / rings as f64, (ring + 1) as f64 / rings as f64);
        // Alpha falls off towards the rim; each ring is its own band.
        let a = (scene.shadow * 0.5 * (1.0 - k1 as f32).powf(1.6) * 255.0).clamp(0.0, 255.0) as u8;
        let col = Rgba(0, 0, 0, a);
        for s in 0..segs {
            let (t0, t1) = (s as f64 / segs as f64 * std::f64::consts::TAU, (s + 1) as f64 / segs as f64 * std::f64::consts::TAU);
            if ring == 0 {
                let inner = Rgba(0, 0, 0, (scene.shadow * 0.5 * 255.0).clamp(0.0, 255.0) as u8);
                out.fills.push(([c, point(0.0, t0), point(0.0, t1)], inner, false));
            }
            out.fills.push(([point(k0, t0), point(k1, t0), point(k1, t1)], col, false));
            out.fills.push(([point(k0, t0), point(k1, t1), point(k0, t1)], col, false));
        }
    }
}

/// The whole body is selected: the body itself, or every one of its faces (Shell and the like select them all).
pub(crate) fn fully_selected(body: &shapecraft_doc::Body, selection: &[Pick]) -> bool {
    if selection.contains(&Pick::Body { body: body.id }) {
        return true;
    }
    let picked = selection.iter().filter(|p| matches!(p, Pick::Face { body: b, .. } if *b == body.id)).count();
    if picked == 0 {
        return false;
    }
    let mut faces = body.mesh.triangle_faces.clone();
    faces.sort_unstable();
    faces.dedup();
    picked >= faces.len() && faces.iter().all(|f| selection.contains(&Pick::Face { body: body.id, face: *f }))
}

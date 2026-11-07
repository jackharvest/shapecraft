//! Interactive tools: what happens between choosing a tool and the command that commits it.
//!
//! This file holds the `Tool` enum, its hints, and each tool's state. `setup` builds a tool from the selection
//! and the command its value makes; `commit` applies a tool's result and adds the sketch entities the drawing
//! tools make.

use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{ItemId, Pick};
use shapecraft_engine::Session;

use super::Workspace;
use super::toolbar::ToolKind;

mod commit;
mod setup;

pub(crate) use setup::picked_faces;
use setup::{selected_items_bounds, v3, with_items};

#[derive(Clone, Debug)]
pub enum Tool {
    Idle,
    PickPlane {
        hovered: Option<usize>,
    },
    Line {
        first: Option<DVec2>,
        last: Option<DVec2>,
    },
    Rectangle {
        first: Option<DVec2>,
    },
    Circle {
        center: Option<DVec2>,
    },
    Polygon {
        center: Option<DVec2>,
    },
    /// Start, then end; the cursor bends it until the third click.
    Arc {
        a: Option<DVec2>,
        b: Option<DVec2>,
    },
    /// 3-Point Ellipse: center, the end of one axis, then the cursor sets the other axis.
    Ellipse {
        center: Option<DVec2>,
        major: Option<DVec2>,
    },
    /// A tool waiting for something to be selected before it can start.
    Await {
        kind: ToolKind,
        title: &'static str,
        hint: &'static str,
    },
    /// Rectangle, 3 Points: one side's two ends, then the cursor sets the width.
    Rect3 {
        a: Option<DVec2>,
        b: Option<DVec2>,
    },
    /// Fit (through the points) or Control (the points pull the curve).
    Spline {
        points: Vec<DVec2>,
        control: bool,
    },
    Extrude(ExtrudeTool),
    Fillet(FilletTool),
    Shell(ShellTool),
    Revolve(RevolveTool),
    Move(MoveTool),
    Mirror(MirrorTool),
    Pattern(PatternTool),
    OffsetFace(OffsetFaceTool),
    Scale(ScaleTool),
    Loft(LoftTool),
    Sweep(SweepTool),
    /// The sketch curve under the cursor and where on it (sketch coordinates).
    Trim {
        target: Option<(u32, DVec2)>,
    },
    OffsetEdge(OffsetEdgeTool),
    RotateAxis(RotateAxisTool),
    OffsetPlane(OffsetPlaneTool),
    CircularPattern(CircularPatternTool),
    /// Project: click body edges or faces to copy them into the sketch.
    Project,
    /// Text: where it starts (after the first click) and what's typed so far.
    /// Text (26.200): `at` None while the Add Text dialog is up; then the block sits centered on `at`, turned by
    /// `angle` (radians), while it's moved and turned with its handles. `align`: 0 left, 1 center, 2 right.
    Text {
        at: Option<DVec2>,
        text: String,
        size: f64,
        align: u8,
        angle: f64,
    },
    /// Plane Through 3 Points: the points clicked so far.
    Plane3 {
        points: Vec<DVec3>,
    },
    /// Align: the bodies to move and the face of theirs that should meet the next face clicked.
    /// `edge`: `from` is a straight edge (it turns parallel to the target edge), not a face.
    Align {
        items: Vec<ItemId>,
        from: Option<(ItemId, u32)>,
        to: Option<serde_json::Value>,
        flip: bool,
        edge: bool,
    },
    /// Axis Through 2 Points: the points clicked so far.
    Axis2 {
        points: Vec<DVec3>,
    },
    /// Plane Perpendicular to Edge at Point (`perp`) or Parallel to Face at Point: the edge, face or plane picked
    /// first; the next click places the plane through that point.
    PointPlane {
        perp: bool,
        base: Option<Pick>,
    },
    /// Translate (N), as observed (335–339): click a start point, then an end point (clicking again moves it);
    /// Done moves (or copies) the items by the difference.
    Translate {
        items: Vec<ItemId>,
        from: Option<DVec3>,
        to: Option<DVec3>,
        copy: bool,
    },
    /// Sketch Fillet (26.200, s11-095): the corners that can be rounded, those picked, and the radius.
    SketchFillet {
        all: Vec<DVec2>,
        corners: Vec<DVec2>,
        radius: f64,
    },
    /// Plane - Through Edge / On Curve at Point (345–346): the plane is made at once; a handle turns it about the
    /// edge or slides it along.
    EdgePlane(EdgePlaneTool),
    /// Offset Edge on a body edge (347–350): an arrow into each flat face beside the edge; the copy goes into a
    /// new sketch on the face pulled into, on Done.
    BodyEdgeOffset(BodyEdgeOffsetTool),
    /// Union / Subtract / Intersect as observed (380–385): the selected bodies, then the bodies clicked; Done applies.
    Boolean(BooleanTool),
    Onto(OntoTool),
}

/// Tools that take items and then a face they go onto (26.200): Replace Face (s11-123..130; faces of one body onto a
/// face or plane, Flip Alignment) and Project as Edge (s11-134..138; regions, faces or edges imprinted on a face).
/// Applied live once both are picked; Done keeps it. `error` is the last refusal, shown in the tool's card.
#[derive(Clone, Debug, Default)]
pub struct OntoTool {
    pub project: bool,
    /// Project as Sketch from outside a sketch (with `project`): the items go into a new sketch on the target.
    pub sketch: bool,
    /// Each item and where its badge sits.
    pub items: Vec<(Pick, DVec3)>,
    /// The target ({"body", "face"} or {"plane"}) and where its badge sits.
    pub to: Option<(serde_json::Value, DVec3)>,
    pub flip: bool,
    pub error: Option<String>,
}

impl OntoTool {
    pub fn title(&self) -> &'static str {
        match (self.project, self.sketch) {
            (true, true) => "Project as Sketch",
            (true, false) => "Project as Edge",
            _ => "Replace Face",
        }
    }

    pub fn command(&self) -> &'static str {
        match (self.project, self.sketch) {
            (true, true) => "project_sketch",
            (true, false) => "project_edge",
            _ => "replace_face",
        }
    }

    /// Replace Face: the body whose faces move.
    pub fn body(&self) -> Option<ItemId> {
        self.items.iter().find_map(|(p, _)| match p {
            Pick::Face { body, .. } => Some(*body),
            _ => None,
        })
    }

    /// Whether a click on `p` adds (or takes back) an item rather than setting the target.
    pub fn takes_item(&self, p: &Pick) -> bool {
        match p {
            Pick::Face { .. } if self.project => self.items.is_empty() || self.to.is_some(),
            Pick::Face { body, .. } => self.items.is_empty() || self.body() == Some(*body) && self.items.iter().any(|(i, _)| i == p),
            Pick::Edge { .. } | Pick::Region { .. } => self.project,
            _ => false,
        }
    }

    /// The item's chip in the card ("1 body" for a body face in Project as Edge, as observed).
    pub fn chip(&self, p: &Pick) -> &'static str {
        match p {
            Pick::Edge { .. } => "1 edge",
            Pick::Face { .. } if self.project => "1 body",
            _ => "1 face",
        }
    }

    pub fn params(&self) -> Option<serde_json::Value> {
        let (to, _) = self.to.as_ref()?;
        if self.items.is_empty() {
            return None;
        }
        if self.project {
            let items: Vec<serde_json::Value> = self
                .items
                .iter()
                .filter_map(|(p, _)| match *p {
                    Pick::Face { body, face } => Some(json!({ "body": body.0, "face": face })),
                    Pick::Edge { body, edge } => Some(json!({ "body": body.0, "edge": edge })),
                    Pick::Region { sketch, region } => Some(json!({ "sketch": sketch.0, "region": region })),
                    _ => None,
                })
                .collect();
            return Some(json!({ "to": to, "items": items }));
        }
        let body = self.body()?;
        let faces: Vec<u32> = self
            .items
            .iter()
            .filter_map(|(p, _)| match *p {
                Pick::Face { body: b, face } if b == body => Some(face),
                _ => None,
            })
            .collect();
        Some(json!({ "body": body.0, "faces": faces, "to": to, "flip": self.flip }))
    }
}

#[derive(Clone, Debug)]
pub struct BooleanTool {
    /// "union", "subtract" or "intersect".
    pub op: &'static str,
    /// What's kept (Subtract: what's cut from).
    pub targets: Vec<ItemId>,
    /// Subtract: what's cut away.
    pub tools: Vec<ItemId>,
    /// Keep Originals: 0 none, 1 targets, 2 tools, 3 all (Union/Intersect: 0 or 3).
    pub keep: u8,
}

impl BooleanTool {
    pub fn title(&self) -> &'static str {
        match (self.op, cfg!(target_os = "macos")) {
            ("subtract", true) => "Subtract (⌘ B)",
            ("subtract", false) => "Subtract (Ctrl+B)",
            ("intersect", true) => "Intersect (⌘ I)",
            ("intersect", false) => "Intersect (Ctrl+I)",
            (_, true) => "Union (⌘ U)",
            (_, false) => "Union (Ctrl+U)",
        }
    }

    /// Enough bodies to apply.
    pub fn ready(&self) -> bool {
        if self.op == "subtract" { !self.targets.is_empty() && !self.tools.is_empty() } else { self.targets.len() >= 2 }
    }

    /// A clicked body joins (or leaves) the operation.
    pub fn toggle(&mut self, body: ItemId) {
        if self.op == "subtract" && self.targets.contains(&body) {
            return;
        }
        let list = if self.op == "subtract" { &mut self.tools } else { &mut self.targets };
        match list.iter().position(|b| *b == body) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(body),
        }
    }

    /// Subtract: a role badge clicked swaps that body between what's cut from (+) and what cuts (−), as the manual
    /// describes; the last body left on a side stays (nothing to swap with).
    pub fn swap_role(&mut self, body: ItemId) {
        if let Some(i) = self.targets.iter().position(|b| *b == body) {
            self.targets.remove(i);
            self.tools.push(body);
        } else if let Some(i) = self.tools.iter().position(|b| *b == body) {
            self.tools.remove(i);
            self.targets.push(body);
        }
    }

    pub fn keep_label(&self) -> &'static str {
        match self.keep {
            0 if self.op == "subtract" => "None",
            1 => "Targets",
            2 => "Tools",
            3 if self.op == "subtract" => "All",
            3 => "On",
            _ => "Off",
        }
    }

    /// Cycles Keep Originals: Off/On, or for Subtract None → Targets → Tools → All (only None was observed).
    pub fn next_keep(&mut self) {
        self.keep = if self.op == "subtract" {
            (self.keep + 1) % 4
        } else if self.keep == 0 {
            3
        } else {
            0
        };
    }

    /// The commands that apply it: one per Subtract target.
    pub fn requests(&self) -> Vec<(&'static str, serde_json::Value)> {
        let keep = match self.keep {
            1 => "target",
            2 => "tools",
            3 => "all",
            _ => "none",
        };
        let ids = |v: &[ItemId]| v.iter().map(|i| i.0).collect::<Vec<_>>();
        if self.op == "subtract" {
            self.targets.iter().map(|t| ("subtract", json!({ "target": t.0, "tools": ids(&self.tools), "keep": keep }))).collect()
        } else {
            match self.targets.split_first() {
                Some((t, rest)) => vec![(self.op, json!({ "target": t.0, "tools": ids(rest), "keep": keep }))],
                None => Vec::new(),
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct BodyEdgeOffsetTool {
    pub body: ItemId,
    pub edge: u32,
    /// The flat faces beside the edge, each with the way into it from the edge (in its plane, square to the edge) and
    /// its outward normal.
    pub faces: Vec<(u32, DVec3, DVec3)>,
    pub active: usize,
    pub anchor: DVec3,
    pub distance: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// The edge's points, for drawing the copy.
    pub line: Vec<DVec3>,
}

impl BodyEdgeOffsetTool {
    pub fn from_selection(s: &Session, at: Option<DVec3>) -> Option<Self> {
        let [Pick::Edge { body, edge }] = s.selection.as_slice() else { return None };
        let b = s.doc.body(*body)?;
        let k = b.mesh.edge_ids.iter().position(|e| e == edge)?;
        let line: Vec<DVec3> = b.mesh.edges.get(k)?.iter().map(|p| v3(*p)).collect();
        let (first, last) = (*line.first()?, *line.last()?);
        // The click point when it's on the edge, else the edge's middle.
        let anchor = at
            .filter(|p| line.windows(2).any(|w| seg_point_dist(*p, w[0], w[1]) < 1e-3 * (1.0 + first.distance(last))))
            .or_else(|| polyline_middle(line.clone()))?;
        let tangent = line
            .windows(2)
            .min_by(|a, c| seg_point_dist(anchor, a[0], a[1]).total_cmp(&seg_point_dist(anchor, c[0], c[1])))
            .map(|w| w[1] - w[0])?;
        let mut faces = Vec::new();
        for f in b.mesh.edge_faces.get(k)?.iter().filter(|f| **f != u32::MAX) {
            let Some(fp) = s.kernel.face_plane(&b.shape, *f).ok().flatten() else { continue };
            let mut d = fp.normal.cross(tangent).normalize_or_zero();
            if d.dot(fp.origin - anchor) < 0.0 {
                d = -d;
            }
            if d.length() > 0.5 {
                faces.push((*f, d, fp.normal));
            }
        }
        (!faces.is_empty()).then_some(Self { body: *body, edge: *edge, faces, active: 0, anchor, distance: 0.0, dragging: None, line })
    }

    pub fn dir(&self) -> DVec3 {
        self.faces.get(self.active).map_or(DVec3::X, |f| f.1)
    }

    /// Starts on the face that faces the viewer most, as observed (347).
    pub fn face_viewer(&mut self, back: DVec3) {
        if let Some((i, _)) = self.faces.iter().enumerate().max_by(|a, b| a.1.2.dot(back).total_cmp(&b.1.2.dot(back))) {
            self.active = i;
        }
    }

    pub fn request(&self, d: f64) -> Option<(&'static str, serde_json::Value)> {
        let face = self.faces.get(self.active)?.0;
        (d.abs() > 1e-9).then(|| ("offset_body_edge", json!({ "body": self.body.0, "edge": self.edge, "face": face, "distance": d })))
    }
}

/// A curved face's outward normal where it was clicked (from the display mesh's nearest triangle on it), if the
/// point is on that face.
fn curved_normal_at(m: &shapecraft_kernel::Mesh, face: u32, p: DVec3) -> Option<DVec3> {
    let v = |i: u32| m.positions.get(i as usize).map(|q| v3(*q));
    let nrm = |i: u32| m.normals.get(i as usize).map(|q| v3(*q));
    let mut best: Option<(f64, DVec3)> = None;
    for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
        if m.triangle_faces.get(t) != Some(&face) {
            continue;
        }
        let (Some(a), Some(b), Some(c)) = (v(tri[0]), v(tri[1]), v(tri[2])) else { continue };
        let d = ((a + b + c) / 3.0).distance(p);
        if best.is_none_or(|(bd, _)| d < bd) {
            let n = nrm(tri[0]).unwrap_or_default() + nrm(tri[1]).unwrap_or_default() + nrm(tri[2]).unwrap_or_default();
            let size = a.distance(b).max(b.distance(c)).max(c.distance(a));
            if d < size * 2.0 + 1e-3 {
                best = Some((d, n));
            }
        }
    }
    best.and_then(|(_, n)| n.try_normalize())
}

fn seg_point_dist(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = b - a;
    let t = if d.length_squared() > 0.0 { ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + d * t).distance(p)
}

#[derive(Clone, Debug)]
pub struct EdgePlaneTool {
    pub body: ItemId,
    pub edge: u32,
    /// Through the edge (turned by `value` degrees), or square to it at `value` (0..1 along it).
    pub through: bool,
    pub value: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
}

impl EdgePlaneTool {
    pub fn request(&self, v: f64) -> (&'static str, serde_json::Value) {
        if self.through {
            ("plane_through_edge", json!({ "body": self.body.0, "edge": self.edge, "angle": v }))
        } else {
            ("plane_on_curve", json!({ "body": self.body.0, "edge": self.edge, "at": v }))
        }
    }

    /// The edge's points, start to end.
    pub fn line(&self, s: &Session) -> Vec<DVec3> {
        let Some(b) = s.doc.body(self.body) else { return Vec::new() };
        b.mesh
            .edge_ids
            .iter()
            .position(|e| *e == self.edge)
            .and_then(|k| b.mesh.edges.get(k))
            .map(|l| l.iter().map(|p| v3(*p)).collect())
            .unwrap_or_default()
    }

    /// The point `at` (0..1) of the way along the edge.
    pub fn point_at(&self, s: &Session, at: f64) -> Option<DVec3> {
        let pts = self.line(s);
        let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mut left = total * at.clamp(0.0, 1.0);
        for w in pts.windows(2) {
            let l = w[0].distance(w[1]);
            if l >= left && l > 0.0 {
                return Some(w[0].lerp(w[1], left / l));
            }
            left -= l;
        }
        pts.last().copied()
    }
}

#[derive(Clone, Debug)]
pub struct OffsetPlaneTool {
    /// What it's parallel to, as the command's "from" parameter.
    pub from: serde_json::Value,
    pub anchor: DVec3,
    pub dir: DVec3,
    pub distance: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
}

impl OffsetPlaneTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        s.selection.iter().find_map(|p| {
            let (from, anchor, dir) = match *p {
                Pick::Face { body, face } => {
                    let fp = s.kernel.face_plane(&s.doc.body(body)?.shape, face).ok().flatten()?;
                    (json!({ "body": body.0, "face": face }), fp.origin, fp.normal)
                }
                Pick::Plane { plane } => {
                    let pl = s.doc.plane(plane)?;
                    (json!({ "plane": plane.0 }), pl.plane.origin, pl.plane.normal())
                }
                Pick::Region { sketch, region } => {
                    let sk = s.doc.sketch(sketch)?;
                    (json!({ "sketch": sketch.0 }), sk.plane.to_world(sk.region(region)?.centroid), sk.plane.normal())
                }
                _ => return None,
            };
            Some(Self { from, anchor, dir, distance: 0.0, dragging: None })
        })
    }

    pub fn request(&self, distance: f64) -> (&'static str, serde_json::Value) {
        ("offset_plane", json!({ "from": self.from, "distance": distance }))
    }
}

#[derive(Clone, Debug)]
pub struct OffsetEdgeTool {
    pub sketch: ItemId,
    pub entities: Vec<u32>,
    /// A region whose whole outline is offset, instead of picked curves.
    pub region: Option<u32>,
    /// A flat body face whose outline is offset into a new sketch on it (no sketch yet).
    pub face: Option<(ItemId, u32)>,
    pub distance: f64,
    /// A point on the first curve and the way a positive offset moves it, in world space.
    pub anchor: DVec3,
    pub dir: DVec3,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// The curves picked before Chain grew them, for going back to Single.
    pub picked: Vec<u32>,
}

/// Loft: regions and flat faces, in the order they were picked.
#[derive(Clone, Debug, Default)]
pub struct LoftTool {
    pub profiles: Vec<Pick>,
    /// Straight walls from profile to profile (the ≡ menu's Ruled).
    pub ruled: bool,
    /// The Result badge's choice; empty or "new" makes a new body.
    pub result: &'static str,
}

/// Sweep: one region or flat face, then the sketch curves it follows.
#[derive(Clone, Debug)]
pub struct SweepTool {
    pub profile: Pick,
    pub path: Vec<Pick>,
    /// The Result badge's choice: "new", "union", "subtract" or "intersect".
    pub result: &'static str,
}

#[derive(Clone, Debug)]
pub struct MirrorTool {
    pub items: Vec<ItemId>,
    pub center: DVec3,
    /// Size of the selection (bounding-box diagonal), so the planes cover it.
    pub size: f64,
    /// Hovered plane tile: 0 = YZ (mirror in X), 1 = XZ (in Y), 2 = XY (in Z).
    pub hovered: Option<usize>,
    /// Split Body picks its cutting plane the same way: cut instead of mirroring.
    pub split: bool,
    /// Keep Originals: Mirror makes a mirrored copy (on by default), Split Body leaves the body whole and adds the
    /// pieces (off by default), as observed.
    pub keep: bool,
    /// Split Body (26.200, s12-002..020): the splitting tools clicked (split_body's "with" entries) and where their
    /// badges sit. Applied live as they're picked; Done keeps it.
    pub with: Vec<(serde_json::Value, DVec3)>,
    /// The pieces the live split added (tinted with the bodies split until Done).
    pub pieces: Vec<ItemId>,
}

/// Offset Face's Distance Type (26.202, s37): a flat face offers Offset / Total, a round one Offset / Radius /
/// Diameter (Diameter first picked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetKind {
    Offset,
    Total,
    Radius,
    Diameter,
}

impl OffsetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Offset => "Offset",
            Self::Total => "Total",
            Self::Radius => "Radius",
            Self::Diameter => "Diameter",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OffsetFaceTool {
    pub body: ItemId,
    pub faces: Vec<u32>,
    pub distance: f64,
    pub anchor: DVec3,
    /// Out of the part, along the first face's normal.
    pub dir: DVec3,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// How far the part reaches behind the face to an opposite parallel face (the "Total" distance type).
    pub thickness: Option<f64>,
    /// A face turned about an axis (cylinder, cone): the radius at the anchor and the axis point level with it.
    /// Its distance type is Radius (dashed line to the axis) or Diameter (dashed line across to the far wall).
    pub radius: Option<(f64, DVec3)>,
    /// A fillet's face (observed 362–364): pulling it changes that fillet's radius ("R0.9 mm", "Edit Fillet 01");
    /// down to nothing removes the fillet.
    pub fillet: Option<FilletFace>,
    /// Whether the face offers distance types: a flat face (Offset / Total) or a cylinder's wall (Offset / Radius /
    /// Diameter). Cones, spheres and free-form faces only offset, with the value always on show (26.202, s40-023/024).
    pub kinds: bool,
}

#[derive(Clone, Debug)]
pub struct FilletFace {
    /// The fillet's history step and its name there.
    pub step: usize,
    pub label: String,
    pub radius: f64,
    pub convex: bool,
}

impl FilletFace {
    /// The radius after pulling the face `d` along its outward normal.
    pub fn radius_after(&self, d: f64) -> f64 {
        (if self.convex { self.radius - d } else { self.radius + d }).max(0.0)
    }
}

/// Scale, as observed (313–323): a gizmo at the selection's center. Non-uniform has a "hammer" per axis
/// (stretches along it) and a tab per plane (stretches the two axes in it); Uniform has one hammer pointing up.
/// The center can be dragged elsewhere: that's the point it scales from.
#[derive(Clone, Debug)]
pub struct ScaleTool {
    pub items: Vec<ItemId>,
    /// One picked rim (closed edge) scaled on its own, about its center.
    pub edge: Option<(ItemId, u32)>,
    pub pivot: DVec3,
    /// The selection's half size per axis: pulling a hammer this far (in mm) doubles that axis.
    pub half: [f64; 3],
    pub uniform: bool,
    /// The handle being (or last) dragged: Arrow(axis) or Plane(normal axis). Uniform uses Arrow(2).
    pub handle: Handle,
    pub factor: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
    pub relocating: bool,
    /// The center sits on a face center it snapped to (drawn purple).
    pub snapped: bool,
    pub copy: bool,
}

#[derive(Clone, Debug)]
pub struct PatternTool {
    pub items: Vec<ItemId>,
    pub frame: GizmoFrame,
    pub pivot: DVec3,
    pub axis: usize,
    /// How far the arrow was pulled: between neighbours (Spacing) or first to last (Total).
    pub spacing: f64,
    pub count: u32,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// Distance Type "Total" (357): the pulled distance spans all the copies.
    pub total: bool,
    /// The Result badge's choice for bodies: "new", "union", "subtract" or "intersect".
    pub result: &'static str,
}

/// A gizmo handle: an axis arrow (move) or a quarter ring (rotate), for axis 0 = X, 1 = Y, 2 = Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Arrow(usize),
    Ring(usize),
    /// Moves within the plane normal to this axis (the reference's small square tabs).
    Plane(usize),
}

#[derive(Clone, Debug)]
pub struct MoveTool {
    /// The live offset while dragging a plane tab (world space).
    pub planar: DVec3,
    /// `planar` when the current drag started (a new drag on the same tab carries on from it).
    pub planar_start: DVec3,
    /// The center is being dragged over the model to put the gizmo somewhere else (no step), as observed.
    pub relocating: bool,
    /// The center sits on a snap point (a face's center): drawn purple.
    pub snapped: bool,
    /// The "Copy" pill: apply to copies, keep the originals.
    pub copy: bool,
    /// The Link badge turned off: copies keep their own geometry (an "Unlinked Copy" step).
    pub unlinked: bool,
    /// Bodies; empty when moving the selected sketch curves or faces.
    pub items: Vec<ItemId>,
    /// Picked faces of one body, moved on their own (the rest of the body follows); empty otherwise.
    pub faces: Option<(ItemId, Vec<u32>)>,
    /// One picked straight edge, moved on its own (the faces beside it tilt).
    pub edge: Option<(ItemId, u32)>,
    pub frame: GizmoFrame,
    pub pivot: DVec3,
    /// The handle being dragged: where the drag started and the value then.
    pub dragging: Option<(Handle, egui::Pos2, f64)>,
    /// The live amount on the active handle (mm or degrees).
    pub value: f64,
    pub handle: Handle,
}

pub const AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

/// The directions a gizmo's handles follow: the world axes, or a sketch's u, v and normal (then only the two
/// in-plane arrows and the ring around the normal show).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GizmoFrame {
    pub axes: [DVec3; 3],
    pub planar: bool,
    /// Turning rings shown (not for an edge, which only moves).
    pub turns: bool,
}

impl GizmoFrame {
    pub const WORLD: GizmoFrame = GizmoFrame { axes: AXES, planar: false, turns: true };

    /// Sketch curves get the whole gizmo on the sketch's axes: the normal arrow and the turns about the in-plane axes
    /// carry them off the plane.
    pub fn sketch(plane: &shapecraft_doc::SketchPlane) -> Self {
        Self { axes: [plane.x, plane.y, plane.normal()], planar: false, turns: true }
    }

    pub fn has(&self, h: Handle) -> bool {
        match h {
            Handle::Arrow(a) => !self.planar || a < 2,
            Handle::Ring(a) => self.turns && (!self.planar || a == 2),
            Handle::Plane(a) => !self.planar || a == 2,
        }
    }
}

/// The selected curves of one sketch: the sketch's frame and their middle, in world space.
fn selected_curves_center(s: &Session) -> Option<(GizmoFrame, DVec3, f64)> {
    if s.selection.iter().any(|p| p.body().is_some()) {
        return None;
    }
    let mut sketch = None;
    let (mut min, mut max) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in &s.selection {
        let (sk, ents) = match *p {
            Pick::SketchEntity { sketch: sk, entity } => (sk, vec![entity]),
            Pick::Region { sketch: sk, region } => (sk, s.doc.sketch(sk)?.region_entities(region)),
            _ => continue,
        };
        if sketch.is_some_and(|x| x != sk) {
            continue;
        }
        sketch = Some(sk);
        for entity in ents {
            for q in s.doc.sketch(sk)?.entities.get(entity as usize)?.outline() {
                min = min.min(q);
                max = max.max(q);
            }
        }
    }
    if !min.is_finite() {
        return None;
    }
    let sk = s.doc.sketch(sketch?)?;
    Some((GizmoFrame::sketch(&sk.plane), sk.plane.to_world((min + max) / 2.0), (max - min).length()))
}

#[derive(Clone, Debug)]
pub struct ShellTool {
    pub body: ItemId,
    pub faces: Vec<u32>,
    pub thickness: f64,
    pub anchor: DVec3,
    /// Into the part: the arrow points the way the wall grows.
    pub dir: DVec3,
    pub dragging: Option<(egui::Pos2, f64)>,
}

#[derive(Clone, Debug)]
pub struct RevolveTool {
    /// A sketch region or a flat face.
    pub profile: Pick,
    /// What it spins around; nothing until a line, edge, round face or axis is clicked.
    pub axis: Option<AxisPick>,
    pub angle: f64,
    /// The Result badge's choice: "auto", "new", "union", "subtract" or "intersect".
    pub result: &'static str,
    /// Dragging the ↕ handle on the profile: where it started and the angle then.
    pub dragging: Option<(egui::Pos2, f64)>,
}

/// An axis chosen by clicking: a line of the profile's sketch, or a body's straight/round edge or round face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisPick {
    Line(u32),
    Topo(Pick),
}

impl AxisPick {
    /// What a click on `p` gives as an axis, if anything (sketch lines only from `sketch`).
    pub fn from_click(s: &Session, p: Pick, sketch: Option<ItemId>) -> Option<Self> {
        match p {
            Pick::SketchEntity { sketch: sk, entity }
                if Some(sk) == sketch
                    && matches!(s.doc.sketch(sk)?.entities.get(entity as usize), Some(shapecraft_doc::SketchEntity::Line { .. })) =>
            {
                Some(AxisPick::Line(entity))
            }
            Pick::Axis { axis } => s.doc.axis(axis).map(|_| AxisPick::Topo(p)),
            Pick::Edge { body, edge } => {
                let b = s.doc.body(body)?;
                s.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Edge(edge)).ok().flatten().map(|_| AxisPick::Topo(p))
            }
            Pick::Face { body, face } => {
                let b = s.doc.body(body)?;
                // Round faces only: a flat face's normal is rarely what's meant.
                if s.kernel.face_plane(&b.shape, face).ok().flatten().is_some() {
                    return None;
                }
                s.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Face(face)).ok().flatten().map(|_| AxisPick::Topo(p))
            }
            _ => None,
        }
    }

    fn json(&self) -> serde_json::Value {
        match *self {
            AxisPick::Line(l) => json!({ "line": l }),
            AxisPick::Topo(Pick::Edge { body, edge }) => json!({ "body": body.0, "edge": edge }),
            AxisPick::Topo(Pick::Face { body, face }) => json!({ "body": body.0, "face": face }),
            AxisPick::Topo(Pick::Axis { axis }) => json!({ "axis": axis.0 }),
            AxisPick::Topo(_) => serde_json::Value::Null,
        }
    }
}

/// Circular pattern: bodies (or the selected sketch curves), the axis, how many in all, over what angle.
#[derive(Clone, Debug)]
pub struct CircularPatternTool {
    pub items: Vec<ItemId>,
    /// A body's edge or round face; for sketch curves, the center of a picked circle or arc.
    pub about: Option<CircularAxis>,
    pub count: u32,
    pub angle: f64,
    /// Uniform: the copies keep facing the same way (the ≡ menu's Orientation), else they turn with the circle.
    pub uniform: bool,
    /// Definition "Spacing": the angle is between neighbours, not the whole spread (Total).
    pub spacing: bool,
    /// The Result badge's choice for bodies: "new", "union", "subtract" or "intersect".
    pub result: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CircularAxis {
    Topo(Pick),
    /// A point and direction in world space.
    Free(DVec3, DVec3),
}

impl CircularPatternTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let mut items = Vec::new();
        for b in s.selection.iter().filter_map(Pick::body) {
            if !items.contains(&b) {
                items.push(b);
            }
        }
        let curves = s.selection.iter().any(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Region { .. }));
        (!items.is_empty() || curves).then_some(Self {
            items,
            about: None,
            count: 6,
            angle: 360.0,
            uniform: false,
            spacing: false,
            result: "new",
        })
    }

    /// What a click on `p` gives as the axis: a straight or round edge or a round face, or, for sketch curves,
    /// the center of a circle or arc (around the sketch's normal).
    pub fn axis_from_click(s: &Session, p: Pick) -> Option<CircularAxis> {
        if let Pick::SketchEntity { sketch, entity } = p {
            let sk = s.doc.sketch(sketch)?;
            let center = match *sk.entities.get(entity as usize)? {
                shapecraft_doc::SketchEntity::Circle { center, .. } | shapecraft_doc::SketchEntity::Polygon { center, .. } => center,
                shapecraft_doc::SketchEntity::Arc { a, mid, b } => shapecraft_doc::circle_through(a, mid, b)?.0,
                _ => return None,
            };
            return Some(CircularAxis::Free(sk.plane.to_world(center), sk.plane.normal()));
        }
        match AxisPick::from_click(s, p, None)? {
            AxisPick::Topo(p) => Some(CircularAxis::Topo(p)),
            AxisPick::Line(_) => None,
        }
    }

    /// The whole spread for a typed angle: as is for Total; for Spacing, the step times the gaps (a step that goes
    /// exactly round the circle is a full circle, its copies spread evenly), at most a turn.
    pub fn total_angle(&self, angle: f64) -> f64 {
        if !self.spacing {
            return angle;
        }
        let n = f64::from(self.count.max(2));
        if ((angle * n).abs() - 360.0).abs() < 1e-6 { 360f64.copysign(angle) } else { (angle * (n - 1.0)).clamp(-360.0, 360.0) }
    }

    pub fn request(&self, angle: f64) -> Option<(&'static str, serde_json::Value)> {
        let mut v = match self.about? {
            CircularAxis::Topo(p) => json!({ "about": AxisPick::Topo(p).json() }),
            CircularAxis::Free(o, d) => json!({ "origin": [o.x, o.y, o.z], "axis": [d.x, d.y, d.z] }),
        };
        v["count"] = json!(self.count);
        v["angle"] = json!(self.total_angle(angle));
        if self.uniform && !self.items.is_empty() {
            v["orientation"] = json!("uniform");
        }
        if !self.items.is_empty() && self.result != "new" {
            v["result"] = json!(self.result);
        }
        with_items(&mut v, &self.items);
        Some(("pattern_circular", v))
    }
}

/// Rotate Around Axis: the bodies, the edge or round face they turn around, and the angle.
#[derive(Clone, Debug)]
pub struct RotateAxisTool {
    pub items: Vec<ItemId>,
    pub about: Option<Pick>,
    pub angle: f64,
    /// The items' middle: the handle sits there, as observed (340).
    pub center: DVec3,
    /// The picked axis in world space (a point on it and its direction).
    pub axis: Option<(DVec3, DVec3)>,
    pub dragging: Option<(egui::Pos2, f64)>,
    pub copy: bool,
    /// The Link badge turned off: an Unlinked Copy step.
    pub unlinked: bool,
    /// Faces of one body turning on their own (picked faces plus the edge they turn around).
    pub faces: Option<(ItemId, Vec<u32>)>,
}

impl RotateAxisTool {
    /// Faces of one body, or bodies, picked together with one straight or round edge: they turn around that edge
    /// straight away (the pick brings Rotate Around Axis up, the edge drawn as the axis).
    pub fn items_and_axis(s: &Session) -> Option<Self> {
        let edges: Vec<Pick> = s.selection.iter().copied().filter(|p| matches!(p, Pick::Edge { .. })).collect();
        let [edge] = edges.as_slice() else { return None };
        let rest: Vec<Pick> = s.selection.iter().copied().filter(|p| !matches!(p, Pick::Edge { .. })).collect();
        let mut t = if rest.iter().all(|p| matches!(p, Pick::Body { .. })) && !rest.is_empty() {
            let items: Vec<ItemId> = rest.iter().filter_map(Pick::body).collect();
            let (mut min, mut max) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for id in &items {
                let bb = s.doc.body(*id).and_then(|b| s.kernel.bounds(&b.shape).ok())?;
                min = min.min(bb.min);
                max = max.max(bb.max);
            }
            Self {
                items,
                about: None,
                angle: 0.0,
                center: (min + max) / 2.0,
                axis: None,
                dragging: None,
                copy: false,
                unlinked: false,
                faces: None,
            }
        } else {
            let mut body = None;
            let mut faces = Vec::new();
            for p in &rest {
                match *p {
                    Pick::Face { body: b, face } if body.is_none_or(|x| x == b) => {
                        body = Some(b);
                        faces.push(face);
                    }
                    _ => return None,
                }
            }
            let body = body?;
            let b = s.doc.body(body)?;
            let (mut min, mut max) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for f in &faces {
                let bb = s.kernel.face(&b.shape, *f).and_then(|face| s.kernel.bounds(&face)).ok()?;
                min = min.min(bb.min);
                max = max.max(bb.max);
            }
            Self {
                items: Vec::new(),
                about: None,
                angle: 0.0,
                center: (min + max) / 2.0,
                axis: None,
                dragging: None,
                copy: false,
                unlinked: false,
                faces: Some((body, faces)),
            }
        };
        t.set_axis(s, *edge).then_some(t)
    }

    /// Picked bodies, or (with none) picked sketch curves, which then turn in their sketch's plane.
    pub fn from_selection(s: &Session) -> Option<Self> {
        let (items, center) = match selected_items_bounds(s) {
            Some((items, min, max)) => (items, (min + max) / 2.0),
            None => (Vec::new(), selected_curves_center(s)?.1),
        };
        Some(Self { items, about: None, angle: 0.0, center, axis: None, dragging: None, copy: false, unlinked: false, faces: None })
    }

    /// Takes a straight or round edge (or a round face) as the axis. Sketch curves turn in their plane, so for them
    /// the axis is square to the sketch, through a clicked sketch point (a clicked curve joins what turns instead).
    pub fn set_axis(&mut self, s: &Session, p: Pick) -> bool {
        if self.items.is_empty() && self.faces.is_none() {
            let at = match p {
                Pick::SketchPoint { sketch, entity, index } => s.doc.sketch(sketch).and_then(|sk| {
                    let q = *shapecraft_doc::constraints::points(sk.entities.get(entity as usize)?).get(index as usize)?;
                    Some((sk.plane.to_world(q), sk.plane.normal()))
                }),
                _ => None,
            };
            let Some((o, d)) = at else { return false };
            self.about = Some(p);
            self.axis = Some((o, d.normalize_or(DVec3::Z)));
            self.angle = 0.0;
            return true;
        }
        let sub = match p {
            Pick::Edge { edge, .. } => shapecraft_kernel::SubShape::Edge(edge),
            Pick::Face { face, .. } => shapecraft_kernel::SubShape::Face(face),
            _ => return false,
        };
        let Some(b) = p.body().and_then(|b| s.doc.body(b)) else { return false };
        // A flat face has an axis (its normal) but isn't one to turn around.
        if let Pick::Face { face, .. } = p
            && s.kernel.face_plane(&b.shape, face).ok().flatten().is_some()
        {
            return false;
        }
        let Some(a) = s.kernel.axis_of(&b.shape, sub).ok().flatten() else { return false };
        self.about = Some(p);
        self.axis = Some((a.origin, a.dir.normalize_or(DVec3::Z)));
        self.angle = 0.0;
        true
    }

    /// The point on the axis nearest the handle (where the dashed radius starts).
    pub fn foot(&self) -> Option<DVec3> {
        let (o, d) = self.axis?;
        Some(o + d * (self.center - o).dot(d))
    }

    /// Where the handle is after turning by `angle` degrees.
    pub fn handle_at(&self, angle: f64) -> Option<DVec3> {
        let (_, d) = self.axis?;
        let f = self.foot()?;
        Some(f + glam::DQuat::from_axis_angle(d, angle.to_radians()) * (self.center - f))
    }

    pub fn request(&self, angle: f64) -> Option<(&'static str, serde_json::Value)> {
        if let Some((body, faces)) = &self.faces {
            let (o, d) = self.axis?;
            return Some((
                "move_faces",
                json!({ "body": body.0, "faces": faces, "origin": [o.x, o.y, o.z], "axis": [d.x, d.y, d.z], "angle": angle }),
            ));
        }
        if self.items.is_empty() {
            // Sketch curves: about the axis square to their sketch.
            let (o, d) = self.axis?;
            return Some(("rotate", json!({ "origin": [o.x, o.y, o.z], "axis": [d.x, d.y, d.z], "angle": angle, "copy": self.copy })));
        }
        let about = AxisPick::Topo(self.about?).json();
        let items: Vec<u64> = self.items.iter().map(|i| i.0).collect();
        let mut v = json!({ "items": items, "about": about, "angle": angle, "copy": self.copy });
        if self.copy && self.unlinked {
            v["linked"] = json!(false);
        }
        Some(("rotate", v))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Region { sketch: ItemId, region: u32 },
    Face { body: ItemId, face: u32 },
}

#[derive(Clone, Debug)]
pub struct ExtrudeTool {
    /// Draft angle in degrees (the walls lean in when positive), set with the ↔ handle.
    pub draft: f64,
    pub drafting: Option<(egui::Pos2, f64)>,
    pub target: Target,
    /// Other selected regions of the target's sketch: they go up with it as one solid.
    pub more: Vec<u32>,
    pub origin: DVec3,
    pub normal: DVec3,
    pub distance: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// The face or region as triangles, and its outline, for the ghost preview.
    pub tris: Vec<[DVec3; 3]>,
    pub outline: Vec<(DVec3, DVec3)>,
    /// The Extrude card's settings (s16-005): Sides ▸ Symmetric, Extent ▸ Through All, Start ▸ Offset, Result.
    pub symmetric: bool,
    pub through_all: bool,
    pub start_offset: Option<f64>,
    pub result: &'static str,
    /// Extent ▸ To Object: the flat face or plane it extrudes up to ({"body","face"} or {"plane"}).
    pub to: Option<serde_json::Value>,
    /// Start ▸ From Plane: the parallel face or plane the profile starts at.
    pub start_plane: Option<serde_json::Value>,
    /// The next click picks the To Object target ("to") or the From Plane one ("start").
    pub picking: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct FilletTool {
    pub body: ItemId,
    pub edges: Vec<u32>,
    pub radius: f64,
    pub anchor: DVec3,
    pub dir: DVec3,
    pub dragging: Option<(egui::Pos2, f64)>,
    /// Bevel instead of round (the tool's Fillet / Chamfer switch).
    pub chamfer: bool,
    /// A Two-Distance chamfer (its Type badge): `radius` runs along the first face, this along the other.
    pub two: Option<TwoDistance>,
    /// The first edge's two faces and the way along each from the anchor, for Two-Distance.
    pub sides: Option<([u32; 2], [DVec3; 2])>,
    /// Continuity G2 (curvature-continuous) instead of G1 (tangent).
    pub g2: bool,
    /// Size Type Chord Width: the size is the round's chord, not its radius (the manual's Fillet card).
    pub chordal: bool,
    /// Setback corners instead of rolling-ball ones where three rounds meet.
    pub setback: bool,
    /// The card's Curvature (profile slider): -1 flat, 0 a true round, 1 sharp.
    pub curvature: f64,
    /// The profile slider's knob being dragged: where the drag began and the curvature then.
    pub slider: Option<(egui::Pos2, f64)>,
}

/// The second side of a Two-Distance chamfer: each distance has its own arrow lying on its face.
#[derive(Clone, Debug)]
pub struct TwoDistance {
    /// The face the first distance runs along, then the other (the first edge's two faces).
    pub faces: [u32; 2],
    /// Along each face, away from the edge, at the anchor.
    pub dirs: [DVec3; 2],
    pub d2: f64,
    pub dragging: Option<(egui::Pos2, f64)>,
}

impl Tool {
    pub fn for_sketch_kind(kind: ToolKind) -> Tool {
        match kind {
            ToolKind::Line => Tool::Line { first: None, last: None },
            ToolKind::Rectangle => Tool::Rectangle { first: None },
            ToolKind::Circle => Tool::Circle { center: None },
            ToolKind::Polygon => Tool::Polygon { center: None },
            ToolKind::Arc => Tool::Arc { a: None, b: None },
            ToolKind::Ellipse => Tool::Ellipse { center: None, major: None },
            ToolKind::Spline => Tool::Spline { points: Vec::new(), control: false },
            _ => Tool::Idle,
        }
    }

    pub fn is_sketch_tool(&self) -> bool {
        matches!(
            self,
            Tool::Line { .. }
                | Tool::Rectangle { .. }
                | Tool::Rect3 { .. }
                | Tool::Circle { .. }
                | Tool::Polygon { .. }
                | Tool::Arc { .. }
                | Tool::Ellipse { .. }
                | Tool::Spline { .. }
        )
    }

    pub fn takes_numbers(&self) -> bool {
        match self {
            Tool::Line { last, .. } => last.is_some(),
            Tool::Rectangle { first } => first.is_some(),
            Tool::Rect3 { b, .. } => b.is_some(),
            Tool::Circle { center } | Tool::Polygon { center } => center.is_some(),
            Tool::Ellipse { center, .. } => center.is_some(),
            Tool::Arc { .. } | Tool::Spline { .. } => false,
            Tool::Extrude(_) | Tool::Fillet(_) | Tool::Shell(_) | Tool::Revolve(_) | Tool::Move(_) | Tool::Pattern(_) => true,
            Tool::RotateAxis(_) | Tool::CircularPattern(_) => true,
            Tool::EdgePlane(t) => t.through,
            Tool::BodyEdgeOffset(_) => true,
            Tool::Boolean(_) => false,
            Tool::OffsetFace(_) | Tool::Scale(_) | Tool::OffsetEdge(_) | Tool::OffsetPlane(_) => true,
            Tool::Mirror(_)
            | Tool::Loft(_)
            | Tool::Sweep(_)
            | Tool::Trim { .. }
            | Tool::Plane3 { .. }
            | Tool::Axis2 { .. }
            | Tool::PointPlane { .. }
            | Tool::Project
            | Tool::Text { .. }
            | Tool::Align { .. }
            | Tool::Translate { .. }
            | Tool::SketchFillet { .. }
            | Tool::Onto(_) => false,
            Tool::Idle | Tool::PickPlane { .. } | Tool::Await { .. } => false,
        }
    }

    /// Title and one-line hint for the banner at the top of the viewport (our own words).
    pub fn hint(&self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Tool::Idle => return None,
            // While choosing a plane the banner explains the tool waiting for it (the caller asks that tool).
            Tool::PickPlane { .. } => return None,
            Tool::Line { .. } => {
                ("Line (L)", "Click to place the ends of the line. Return finishes it; Escape takes back the last temporary segment.")
            }
            Tool::Rectangle { .. } => ("Rectangle (R)", "Click one corner, then the opposite one. Type to set the size"),
            Tool::Await { title, hint, .. } => (title, hint),
            Tool::Rect3 { .. } => ("Rectangle (R)", "Click both ends of one side, then pull out the width. Type to set it"),
            Tool::Circle { .. } => ("Circle (C)", "Click the center, then the rim. Type to set the radius"),
            Tool::Polygon { .. } => ("Polygon (G)", "Click the center, then a corner. Type to set the size"),
            Tool::Arc { .. } => {
                ("3-Point Arc (A)", "Click the two ends of the arc, then a third point to shape the curve. Return finishes it.")
            }
            Tool::Ellipse { .. } => {
                ("3-Point Ellipse", "Click the center, then the end of one axis, then pull out the other. Type to set it")
            }
            Tool::Spline { control: true, .. } => {
                ("Control Point Spline (I)", "Click the points of a guiding polyline. Escape or Return ends it")
            }
            Tool::Spline { .. } => (
                "Fit Point Spline (I)",
                "Click the points the curve goes through, adding as many segments as you like. Return finishes it.",
            ),
            Tool::Extrude(t) if t.picking == Some("to") => ("Extrude - To Object", "Select the flat face or plane to extrude to"),
            Tool::Extrude(t) if t.picking.is_some() => ("Extrude - From Plane", "Select the face or plane the extrusion starts from"),
            // These say what to pick, as the reference does at that moment (26.202, s37), not how to drag.
            Tool::Extrude(_) => {
                ("Extrude - One-Sided (E)", "Pick a sketch profile or a flat face, then pull it out to the depth you want.")
            }
            Tool::Fillet(t) if t.chamfer => ("Chamfer/Fillet (F)", "Pick edges or faces to cut them back at an angle."),
            Tool::Fillet(_) => ("Chamfer/Fillet (F)", "Pick edges or faces to round them off."),
            Tool::Shell(_) => {
                ("Shell (H)", "Pick the faces to take away and hollow the body out; pull the arrow or type the wall thickness.")
            }
            Tool::Revolve(t) if t.axis.is_none() => {
                ("Revolve (V)", "Pick a profile or a face, and an axis to spin it around into a solid.")
            }
            Tool::Revolve(_) => ("Revolve (V)", "Type an angle, or click a line or edge to spin around it. Return applies"),
            Tool::RotateAxis(t) if t.about.is_none() && t.items.is_empty() => {
                ("Rotate Around Axis", "Click more curves to turn, then a sketch point to turn them around")
            }
            Tool::RotateAxis(t) if t.about.is_none() => {
                ("Rotate Around Axis", "Click a straight or round edge, or a round face, to turn around")
            }
            Tool::RotateAxis(_) => ("Rotate Around Axis", "Drag the handle around the axis, or type an angle"),
            Tool::CircularPattern(t) if t.about.is_none() => {
                ("Pattern - Circular", "Click an edge or round face (or a sketch circle) to repeat around")
            }
            Tool::CircularPattern(_) => ("Pattern - Circular", "− / + for the count, type an angle. Return applies"),
            Tool::Move(_) => ("Move/Rotate (M)", "Pick what to move or turn, then use the gizmo."),
            Tool::Mirror(t) if t.split => ("Split Body", "Select a body to split, then a face, plane or sketch line to split it with."),
            Tool::Mirror(_) => ("Mirror", "Pick what to mirror, then a line, plane or flat face to mirror it across."),
            Tool::OffsetFace(_) => ("Offset Face", "Pick faces to push them in or pull them out."),
            Tool::Scale(_) => ("Scale (S)", "Pull a handle or type a factor. Drag the center to scale from somewhere else"),
            Tool::Pattern(t) if t.spacing.abs() > 1e-9 => {
                ("Linear Pattern", "Set the spacing and how many; − / + change the count. Done applies")
            }
            Tool::Pattern(_) => ("Linear Pattern", "Pull an arrow to repeat along it"),
            Tool::Loft(_) => ("Loft", "Pick two or more profiles or faces, in order, to blend a solid through them."),
            Tool::Sweep(_) => ("Sweep (W)", "Pick a profile, then the path to sweep it along."),
            Tool::SketchFillet { .. } => ("Fillet", "Drag the arrow to set the radius; click corners to add or take them away."),
            Tool::Trim { .. } => ("Trim (T)", "Click any piece of a curve between crossings to cut it away."),
            Tool::Project => ("Project (P)", "Click body edges or faces to copy them into the sketch"),
            Tool::Text { at: None, .. } => return None,
            Tool::Text { .. } => ("Drag the handles to move or turn the text", ""),
            Tool::OffsetPlane(_) => ("Plane - Offset", "Pick a face or plane to start from, then set how far away."),
            Tool::Plane3 { .. } => ("Plane Through 3 Points", "Click three points on faces"),
            Tool::Axis2 { .. } => ("Axis Through 2 Points", "Click two points on faces or edges"),
            Tool::PointPlane { perp: true, base: None } => ("Plane Perpendicular to Edge", "Select an edge"),
            Tool::PointPlane { perp: true, base: Some(_) } => ("Plane Perpendicular to Edge", "Click the point the plane goes through"),
            Tool::PointPlane { perp: false, base: None } => ("Plane Parallel to Face", "Select a flat face or plane"),
            Tool::PointPlane { perp: false, base: Some(_) } => ("Plane Parallel to Face", "Click the point the plane goes through"),
            Tool::Align { from: None, .. } => ("Align", "Select a face or a straight edge to align, then its target"),
            Tool::Align { to: None, edge: true, .. } => ("Align", "Select the straight edge to align it with"),
            Tool::Align { to: None, .. } => ("Align", "Select a target face or plane to align to"),
            Tool::Align { .. } => ("Align", "Flip it with the badge, then Done"),
            Tool::Onto(t) if t.sketch => {
                ("Project as Sketch", "Pick edges or faces to project, then the face or plane to project them onto.")
            }
            Tool::Onto(t) if t.project => ("Project as Edge", "Pick edges or sketches to project, then the face to project them onto."),
            Tool::Onto(_) => ("Replace Face", "Pick a face to change, then the face it should be replaced with."),
            // Outside a sketch the banner keeps asking for edges or faces, arrow and all (s24-008).
            Tool::OffsetEdge(t) if t.region.is_some() || t.face.is_some() => {
                ("Offset Edge (O)", "Select edges or faces to create an offset sketch")
            }
            Tool::OffsetEdge(_) => ("Offset Edge (O)", "Pull the arrow or type a distance. Return applies"),
            Tool::Boolean(t) if t.op == "subtract" => (t.title(), "Select bodies to remove from, then select bodies to subtract"),
            Tool::Boolean(t) if t.op == "intersect" => (t.title(), "Select intersecting bodies to keep what they share"),
            Tool::Boolean(t) => (t.title(), "Pick bodies that overlap to merge them into one."),
            // 26.200 keeps the pick hint while the arrows show (s24 matrix), as for a face.
            Tool::BodyEdgeOffset(_) => ("Offset Edge (O)", "Select edges or faces to create an offset sketch"),
            Tool::EdgePlane(t) if t.through => {
                ("Plane Through Edge at Angle", "Drag the handle to turn the plane about the edge, or type an angle")
            }
            Tool::EdgePlane(_) => ("Plane on Curve at Point", "Drag the point along the edge"),
            Tool::Translate { from: None, .. } => ("Translate (N)", "Click the point to move from"),
            Tool::Translate { to: None, .. } => ("Translate (N)", "Click the point to move it to"),
            Tool::Translate { .. } => ("Translate (N)", "Click elsewhere to change the end point. Done applies"),
        })
    }
}

impl Workspace {
    /// A typed value: a length ("12", "3 cm") or an expression using the project's variables.
    pub fn eval_typed(&self, text: &str) -> Option<f64> {
        parse_length(text).or_else(|| shapecraft_doc::expr::eval(text, &self.session.doc.variables).ok())
    }
}

/// Parses what the user typed. Units are millimeters unless a suffix says otherwise.
pub fn parse_length(text: &str) -> Option<f64> {
    let t = text.trim();
    let (num, scale) = if let Some(n) = t.strip_suffix("mm") {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("cm") {
        (n, 10.0)
    } else if let Some(n) = t.strip_suffix("in") {
        (n, 25.4)
    } else {
        (t, 1.0)
    };
    let v: f64 = num.trim().parse().ok()?;
    (v.is_finite()).then_some(v * scale)
}

/// The command a Chamfer/Fillet tool runs for a given size.
pub fn fillet_request(t: &FilletTool, size: f64) -> (&'static str, serde_json::Value) {
    let key = if t.chamfer { "distance" } else { "radius" };
    let mut p = serde_json::Map::new();
    p.insert("body".into(), json!(t.body.0));
    p.insert(key.into(), json!(size));
    if !t.edges.is_empty() {
        p.insert("edges".into(), json!(t.edges));
    }
    // Before the second arrow is pulled, both sides take the first distance.
    if let Some(two) = t.two.as_ref().filter(|_| t.chamfer) {
        p.insert("distance2".into(), json!(if two.d2 > 1e-9 { two.d2 } else { size }));
        p.insert("face".into(), json!(two.faces[0]));
    }
    if t.g2 && !t.chamfer {
        p.insert("continuity".into(), json!("g2"));
    }
    if t.chordal && !t.chamfer {
        p.insert("mode".into(), json!("chordal"));
    }
    if t.setback && !t.chamfer {
        p.insert("corner".into(), json!("setback"));
    }
    if t.curvature.abs() > 1e-9 && !t.chamfer {
        p.insert("curvature".into(), json!(t.curvature));
    }
    (if t.chamfer { "chamfer" } else { "fillet" }, serde_json::Value::Object(p))
}

/// Casts a ray from a point on `face` into the part (against `out`) and returns how far it is to the first face it
/// meets, if that face is parallel to the start (an opposite wall). Uses the display mesh.
fn opposite_face_distance(m: &shapecraft_kernel::Mesh, from: DVec3, out: DVec3, face: u32) -> Option<f64> {
    let d = -out;
    let mut best: Option<(f64, DVec3)> = None;
    for (t, f) in m.triangle_faces.iter().enumerate() {
        if *f == face {
            continue;
        }
        let v = |k: usize| m.indices.get(t * 3 + k).and_then(|i| m.positions.get(*i as usize)).map(|p| v3(*p));
        let (Some(a), Some(b), Some(c)) = (v(0), v(1), v(2)) else { continue };
        // Möller–Trumbore.
        let (e1, e2) = (b - a, c - a);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-12 {
            continue;
        }
        let sv = from - a;
        let u = sv.dot(h) / det;
        let q = sv.cross(e1);
        let w = d.dot(q) / det;
        let dist = e2.dot(q) / det;
        if u < -1e-9 || w < -1e-9 || u + w > 1.0 + 1e-9 || dist <= 1e-6 {
            continue;
        }
        if best.is_none_or(|(bd, _)| dist < bd) {
            best = Some((dist, e1.cross(e2).normalize_or_zero()));
        }
    }
    let (dist, n) = best?;
    (n.dot(out).abs() > 0.999).then_some(dist)
}

/// The point halfway along a polyline (by length).
fn polyline_middle(pts: Vec<DVec3>) -> Option<DVec3> {
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total / 2.0;
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if l >= left && l > 0.0 {
            return Some(w[0].lerp(w[1], left / l));
        }
        left -= l;
    }
    pts.first().copied()
}

/// Where each line of a text block starts so the whole block is centered on `anchor` and turned by `angle`; lines
/// are 1.4 heights apart and lined up left, centered or right (`align` 0, 1, 2) within the block.
pub(super) fn text_lines(text: &str, size: f64, align: u8, anchor: DVec2, angle: f64, font: Option<&str>) -> Vec<(DVec2, String)> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let bounds = |line: &str| {
        let pts: Vec<DVec2> = shapecraft_doc::text::outlines_in(font, line, DVec2::ZERO, size, 0.0).into_iter().flatten().collect();
        let min = pts.iter().fold(DVec2::splat(f64::INFINITY), |m, p| m.min(*p));
        let max = pts.iter().fold(DVec2::splat(f64::NEG_INFINITY), |m, p| m.max(*p));
        if min.is_finite() && max.is_finite() { (min, max) } else { (DVec2::ZERO, DVec2::new(size * 0.6 * line.len() as f64, size)) }
    };
    let boxes: Vec<(DVec2, DVec2)> = lines.iter().map(|l| bounds(l)).collect();
    let widest = boxes.iter().map(|(a, b)| b.x - a.x).fold(0.0f64, f64::max);
    let pitch = size * 1.4;
    // Unturned layout with the first line's baseline at y 0, then centered on the block's middle.
    let mut placed: Vec<(DVec2, DVec2, DVec2)> = Vec::new();
    for (i, (min, max)) in boxes.iter().enumerate() {
        let w = max.x - min.x;
        let x = match align {
            1 => (widest - w) / 2.0,
            2 => widest - w,
            _ => 0.0,
        } - min.x;
        let origin = DVec2::new(x, -(i as f64) * pitch);
        placed.push((origin, *min + origin, *max + origin));
    }
    let lo = placed.iter().fold(DVec2::splat(f64::INFINITY), |m, p| m.min(p.1));
    let hi = placed.iter().fold(DVec2::splat(f64::NEG_INFINITY), |m, p| m.max(p.2));
    let mid = if lo.is_finite() && hi.is_finite() { (lo + hi) / 2.0 } else { DVec2::ZERO };
    let rot = DVec2::from_angle(angle);
    lines.iter().zip(placed).map(|(l, (o, ..))| (anchor + rot.rotate(o - mid), l.to_string())).collect()
}

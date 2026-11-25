//! How each tool sets itself up from the selection, and the command it builds from its value.

use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{ItemId, Pick};
use shapecraft_engine::Session;
use shapecraft_render::{Overlay, Rgba};

use super::*;

pub(super) fn v3(p: [f32; 3]) -> DVec3 {
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
}

/// The area-weighted middle of some triangles.
fn middle_of(tris: &[[DVec3; 3]]) -> Option<DVec3> {
    let (mut sum, mut area) = (DVec3::ZERO, 0.0);
    for [a, b, c] in tris {
        let w = (*b - *a).cross(*c - *a).length();
        sum += (*a + *b + *c) / 3.0 * w;
        area += w;
    }
    (area > 1e-12).then(|| sum / area)
}

impl ExtrudeTool {
    /// `at`: where the region or face was clicked; the arrow sits there when it's on the profile, as observed.
    pub fn from_selection(s: &Session, at: Option<DVec3>) -> Option<Self> {
        let mut t = None;
        for pick in &s.selection {
            match *pick {
                Pick::Region { sketch, region } => {
                    t = Self::for_region(s, sketch, region);
                    if let Some(t) = t.as_mut() {
                        t.add_regions(s, sketch, region);
                    }
                    break;
                }
                Pick::Face { body, face } => {
                    t = Self::for_face(s, body, face);
                    if t.is_some() {
                        break;
                    }
                }
                _ => {}
            }
        }
        let mut t = t?;
        if let Some(p) = at
            && (p - t.origin).dot(t.normal).abs() < 1e-3
        {
            t.origin = p;
        }
        Some(t)
    }

    /// The middle of a region or flat face, where tools put their handle on it.
    pub(crate) fn profile_middle(s: &Session, p: Pick) -> Option<DVec3> {
        match p {
            Pick::Region { sketch, region } => Self::for_region(s, sketch, region).map(|t| t.origin),
            Pick::Face { body, face } => Self::for_face(s, body, face).map(|t| t.origin),
            _ => None,
        }
    }

    /// Takes in the other selected regions of `sketch`. The ghost's outline drops the edges they share, so a line
    /// that split them doesn't show on the preview either.
    fn add_regions(&mut self, s: &Session, sketch: ItemId, first: u32) {
        let Some(sk) = s.doc.sketch(sketch) else { return };
        for pick in &s.selection {
            let Pick::Region { sketch: k, region } = *pick else { continue };
            if k != sketch || region == first || self.more.contains(&region) {
                continue;
            }
            let Some(reg) = sk.region(region) else { continue };
            let plane = sk.plane;
            self.tris.extend(reg.triangles.iter().map(|t| [plane.to_world(t[0]), plane.to_world(t[1]), plane.to_world(t[2])]));
            for (a, b) in reg.mesh.edges.iter().flat_map(|line| line.windows(2).map(|w| (v3(w[0]), v3(w[1]))).collect::<Vec<_>>()) {
                let same = |p: DVec3, q: DVec3| p.distance_squared(q) < 1e-10;
                match self.outline.iter().position(|&(c, d)| (same(a, c) && same(b, d)) || (same(a, d) && same(b, c))) {
                    Some(i) => {
                        self.outline.swap_remove(i);
                    }
                    None => self.outline.push((a, b)),
                }
            }
            self.more.push(region);
        }
    }

    fn for_region(s: &Session, sketch: ItemId, region: u32) -> Option<Self> {
        let sk = s.doc.sketch(sketch)?;
        let reg = sk.region(region)?;
        let plane = sk.plane;
        let tris: Vec<[DVec3; 3]> =
            reg.triangles.iter().map(|t| [plane.to_world(t[0]), plane.to_world(t[1]), plane.to_world(t[2])]).collect();
        let outline = reg.mesh.edges.iter().flat_map(|line| line.windows(2).map(|w| (v3(w[0]), v3(w[1]))).collect::<Vec<_>>()).collect();
        Some(Self {
            draft: 0.0,
            drafting: None,
            target: Target::Region { sketch, region },
            more: Vec::new(),
            origin: middle_of(&tris).unwrap_or_else(|| plane.to_world(reg.centroid)),
            normal: plane.normal(),
            distance: 0.0,
            dragging: None,
            tris,
            outline,
            symmetric: false,
            through_all: false,
            start_offset: None,
            result: "auto",
            to: None,
            start_plane: None,
            picking: None,
        })
    }

    fn for_face(s: &Session, body: ItemId, face: u32) -> Option<Self> {
        let b = s.doc.body(body)?;
        let fp = s.kernel.face_plane(&b.shape, face).ok()??;
        let m = &b.mesh;
        let mut tris = Vec::new();
        let mut edge_count: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
        for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
            if m.triangle_faces.get(t) != Some(&face) {
                continue;
            }
            let p = |i: u32| m.positions.get(i as usize).copied().map(v3).unwrap_or_default();
            tris.push([p(tri[0]), p(tri[1]), p(tri[2])]);
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let outline = edge_count
            .into_iter()
            .filter(|(_, n)| *n == 1)
            .map(|((a, b), _)| {
                let p = |i: u32| m.positions.get(i as usize).copied().map(v3).unwrap_or_default();
                (p(a), p(b))
            })
            .collect();
        // The arrow starts in the face's middle, as on a region; a click moves it (from_selection).
        let middle = middle_of(&tris).unwrap_or(fp.origin);
        Some(Self {
            draft: 0.0,
            drafting: None,
            target: Target::Face { body, face },
            more: Vec::new(),
            origin: middle - fp.normal * (middle - fp.origin).dot(fp.normal),
            normal: fp.normal,
            distance: 0.0,
            dragging: None,
            tris,
            outline,
            symmetric: false,
            through_all: false,
            start_offset: None,
            result: "auto",
            to: None,
            start_plane: None,
            picking: None,
        })
    }

    pub fn request(&self, distance: f64) -> (&'static str, serde_json::Value) {
        let mut v = match self.target {
            Target::Region { sketch, region } if !self.more.is_empty() => {
                let regions: Vec<u32> = std::iter::once(region).chain(self.more.iter().copied()).collect();
                json!({ "sketch": sketch.0, "regions": regions, "distance": distance })
            }
            Target::Region { sketch, region } => json!({ "sketch": sketch.0, "region": region, "distance": distance }),
            Target::Face { body, face } => json!({ "body": body.0, "face": face, "distance": distance }),
        };
        if self.draft.abs() > 1e-9 {
            v["draft"] = json!(self.draft);
        }
        if self.symmetric {
            v["sides"] = json!("symmetric");
        }
        if self.through_all {
            v["extent"] = json!("through_all");
        }
        if let Some(o) = self.start_offset {
            v["start_offset"] = json!(o);
        }
        if let Some(to) = &self.to {
            v["extent"] = json!("to_object");
            v["to"] = to.clone();
        }
        if let Some(sp) = &self.start_plane {
            v["start_plane"] = sp.clone();
        }
        if self.result != "auto" {
            v["result"] = json!(self.result);
        }
        ("extrude", v)
    }

    pub fn overlays(&self, out: &mut Vec<Overlay>) {
        self.overlays_in(out, Rgba::hex(0x1db3dc));
    }

    /// The pulled volume in one colour: caps, faint walls, lines up the sharp corners. A cut shows it red over
    /// the live result (26.202, s36-disc-pull: the volume that goes reads pinkish red through the part).
    pub fn overlays_in(&self, out: &mut Vec<Overlay>, ghost: Rgba) {
        // Start ▸ Offset lifts the profile; Symmetric pulls it both ways.
        let base = self.normal * self.start_offset.unwrap_or(0.0);
        let off = self.normal * self.distance;
        if self.distance.abs() > 1e-9 {
            let ends: &[f64] = if self.symmetric { &[1.0, -1.0] } else { &[1.0] };
            let from = if self.symmetric { base - off } else { base };
            let corners = self.outline_corners();
            for side in ends {
                let end = base + off * *side;
                out.push(Overlay::Fill {
                    tris: self.tris.iter().map(|t| [t[0] + end, t[1] + end, t[2] + end]).collect(),
                    color: ghost.with_alpha(70),
                });
                for (a, b) in &self.outline {
                    out.push(Overlay::Line { a: *a + end, b: *b + end, color: ghost, width: 1.5 });
                }
            }
            // The walls as a faint tint, and lines only up the sharp corners: a round profile reads as one smooth
            // side, not a fence of facet lines.
            let top = base + off;
            let walls =
                self.outline.iter().flat_map(|(a, b)| [[*a + from, *b + from, *b + top], [*a + from, *b + top, *a + top]]).collect();
            out.push(Overlay::Fill { tris: walls, color: ghost.with_alpha(35) });
            for a in corners {
                out.push(Overlay::Line { a: a + from, b: a + top, color: ghost.with_alpha(160), width: 1.0 });
            }
        }
    }

    /// A cut, as the reference draws it once the depth is set (26.202, s39-cut-applied): the profile orange with
    /// its outline, the volume a faint red with a thin dark rim where it ends, and the part left whole.
    pub fn cut_overlays(&self, out: &mut Vec<Overlay>) {
        let base = self.normal * self.start_offset.unwrap_or(0.0);
        out.push(Overlay::Fill {
            tris: self.tris.iter().map(|t| [t[0] + base, t[1] + base, t[2] + base]).collect(),
            color: Rgba::hex(0xda8a2e).with_alpha(235),
        });
        for (a, b) in &self.outline {
            out.push(Overlay::Line { a: *a + base, b: *b + base, color: Rgba::hex(0x1d4eff), width: 1.5 });
        }
        if self.distance.abs() <= 1e-9 {
            return;
        }
        let off = self.normal * self.distance;
        let red = Rgba::hex(0xe8506e);
        let ends: &[f64] = if self.symmetric { &[1.0, -1.0] } else { &[1.0] };
        for side in ends {
            let end = base + off * *side;
            for (a, b) in &self.outline {
                out.push(Overlay::Line { a: *a + end, b: *b + end, color: Rgba::hex(0x2a2224).with_alpha(200), width: 1.0 });
            }
        }
        let (from, top) = if self.symmetric { (base - off, base + off) } else { (base, base + off) };
        let walls = self.outline.iter().flat_map(|(a, b)| [[*a + from, *b + from, *b + top], [*a + from, *b + top, *a + top]]).collect();
        out.push(Overlay::Fill { tris: walls, color: red.with_alpha(40) });
        let caps = ends.iter().flat_map(|side| {
            let end = base + off * *side;
            self.tris.iter().map(move |t| [t[0] + end, t[1] + end, t[2] + end])
        });
        out.push(Overlay::Fill { tris: caps.collect(), color: red.with_alpha(22) });
    }

    /// Outline points where the profile turns sharply (more than ~25°); a tessellated curve has none.
    fn outline_corners(&self) -> Vec<DVec3> {
        let tol = 1e-6 * (1.0 + self.outline.iter().map(|(a, b)| (*b - *a).length()).fold(0.0, f64::max));
        let mut out = Vec::new();
        for (a, b) in &self.outline {
            let dir = (*b - *a).normalize_or_zero();
            let before = self.outline.iter().find(|(_, pb)| pb.distance(*a) <= tol).map(|(pa, pb)| (*pb - *pa).normalize_or_zero());
            // An open end (no segment arrives here) is a corner too.
            if before.is_none_or(|d| d.dot(dir) < 0.9) {
                out.push(*a);
            }
            if !self.outline.iter().any(|(na, _)| na.distance(*b) <= tol) {
                out.push(*b);
            }
        }
        out
    }
}

impl FilletTool {
    /// `at`: where the edge was clicked; the handle goes there when it's on the edge, as observed (305).
    pub fn from_selection(s: &Session, at: Option<DVec3>) -> Option<Self> {
        let mut body = None;
        let mut edges = Vec::new();
        for pick in &s.selection {
            match *pick {
                Pick::Edge { body: b, edge } if body.is_none_or(|x| x == b) => {
                    body = Some(b);
                    edges.push(edge);
                }
                Pick::Body { body: b } if body.is_none() => body = Some(b),
                _ => {}
            }
        }
        let body = body?;
        let b = s.doc.body(body)?;
        let bb = s.kernel.bounds(&b.shape).ok()?;
        let center = (bb.min + bb.max) / 2.0;
        // The handle sits on the newest pick (the edge just added, or a face's inner loop, which comes last).
        let line: Vec<DVec3> = edges
            .last()
            .and_then(|e| b.mesh.edge_ids.iter().position(|x| x == e))
            .and_then(|i| b.mesh.edges.get(i))
            .map(|line| line.iter().map(|p| v3(*p)).collect())
            .unwrap_or_default();
        let near = (bb.max - bb.min).length() * 0.01;
        let on_edge = |p: DVec3| {
            line.windows(2)
                .map(|w| {
                    let d = w[1] - w[0];
                    let t = if d.length_squared() > 0.0 { ((p - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                    w[0] + d * t
                })
                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                .filter(|q| q.distance(p) < near)
        };
        // At the click when it was on the edge, else the edge's middle.
        let anchor = at.and_then(on_edge).or_else(|| polyline_middle(line.clone())).unwrap_or(bb.max);
        // Into the part: pulling the handle inward rounds more, as observed (306).
        let dir = (center - anchor).normalize_or(DVec3::Z);
        let mut t = Self {
            body,
            edges,
            radius: 0.0,
            anchor,
            dir,
            dragging: None,
            chamfer: false,
            two: None,
            sides: None,
            g2: false,
            chordal: false,
            setback: false,
            curvature: 0.0,
            slider: None,
        };
        // Worked out now: once a live chamfer is applied the body's edges are numbered anew.
        t.sides = t.sides(s);
        Some(t)
    }

    /// A Two-Distance chamfer whose second distance starts at `d2`; `None` for an edge without two faces.
    pub fn two_distance(&self, d2: f64) -> Option<TwoDistance> {
        let (faces, dirs) = self.sides?;
        Some(TwoDistance { faces, dirs, d2, dragging: None })
    }

    /// Two-Distance's frame: the handle's (newest) edge's two faces and, for each, the way along it from the anchor away from
    /// the edge (perpendicular to the edge, on the face).
    fn sides(&self, s: &Session) -> Option<([u32; 2], [DVec3; 2])> {
        let m = &s.doc.body(self.body)?.mesh;
        let k = m.edge_ids.iter().position(|e| Some(e) == self.edges.last())?;
        let faces = *m.edge_faces.get(k)?;
        if faces.contains(&u32::MAX) || faces[0] == faces[1] {
            return None;
        }
        let line: Vec<DVec3> = m.edges.get(k)?.iter().map(|p| v3(*p)).collect();
        // The edge's direction at the anchor: its nearest segment.
        let tangent = line
            .windows(2)
            .min_by(|a, b| {
                let d = |w: &[DVec3]| (w[0] + w[1]).distance(self.anchor * 2.0);
                d(a).total_cmp(&d(b))
            })
            .map(|w| (w[1] - w[0]).normalize_or_zero())?;
        // Each face's normal at its mesh vertex nearest the anchor.
        let normal = |face: u32| {
            m.indices
                .as_chunks::<3>()
                .0
                .iter()
                .zip(&m.triangle_faces)
                .filter(|(_, f)| **f == face)
                .flat_map(|(t, _)| t.iter().copied())
                .filter_map(|i| Some((v3(*m.positions.get(i as usize)?), v3(*m.normals.get(i as usize)?))))
                .min_by(|a, b| a.0.distance(self.anchor).total_cmp(&b.0.distance(self.anchor)))
                .map(|(_, n)| n.normalize_or_zero())
        };
        let (na, nb) = (normal(faces[0])?, normal(faces[1])?);
        // On a face, away from the edge, is below the other face (for the convex edges a chamfer cuts).
        let along = |n: DVec3, other: DVec3| {
            let d = n.cross(tangent).normalize_or_zero();
            if d.dot(other) > 0.0 { -d } else { d }
        };
        let dirs = [along(na, nb), along(nb, na)];
        (dirs[0].length() > 0.5 && dirs[1].length() > 0.5).then_some((faces, dirs))
    }
}

impl MoveTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let mut items: Vec<ItemId> = Vec::new();
        for b in s.selection.iter().filter_map(|p| match *p {
            Pick::Plane { plane } => Some(plane),
            _ => p.body(),
        }) {
            if !items.contains(&b) {
                items.push(b);
            }
        }
        // One sketch point: the gizmo on it, in its sketch's plane; it moves and what meets there follows.
        if let [Pick::SketchPoint { sketch, entity, index }] = s.selection.as_slice()
            && let Some(sk) = s.doc.sketch(*sketch)
            && let Some(at) =
                sk.entities.get(*entity as usize).and_then(|e| shapecraft_doc::constraints::points(e).get(*index as usize).copied())
        {
            return Some(Self {
                items: Vec::new(),
                faces: None,
                edge: None,
                point: Some(Box::new(super::PointMove {
                    sketch: *sketch,
                    at,
                    plane: sk.plane,
                    base: sk.entities.clone(),
                    constraints: sk.constraints.clone(),
                    picked: (*entity, *index),
                })),
                frame: GizmoFrame { axes: [sk.plane.x, sk.plane.y, sk.plane.normal()], planar: true, turns: false },
                pivot: sk.plane.to_world(at),
                dragging: None,
                value: 0.0,
                handle: Handle::Arrow(0),
                planar: DVec3::ZERO,
                planar_start: DVec3::ZERO,
                relocating: false,
                snapped: false,
                copy: false,
                unlinked: false,
            });
        }
        // One straight edge (nothing else picked): it moves or turns on its own, the gizmo at its middle.
        if let [Pick::Edge { body, edge }] = s.selection.as_slice()
            && let Some(b) = s.doc.body(*body)
            && let Some(k) = b.mesh.edge_ids.iter().position(|e| e == edge)
            && let Some(line) = b.mesh.edges.get(k)
            && let (Some(a), Some(z)) = (line.first().map(|q| v3(*q)), line.last().map(|q| v3(*q)))
            && line.iter().all(|q| (v3(*q) - a).cross((z - a).normalize_or_zero()).length() < 1e-6 * (1.0 + (z - a).length()))
            && (z - a).length() > 1e-9
        {
            return Some(Self {
                items: Vec::new(),
                faces: None,
                edge: Some((*body, *edge)),
                frame: GizmoFrame::WORLD,
                pivot: (a + z) / 2.0,
                dragging: None,
                value: 0.0,
                handle: Handle::Arrow(2),
                planar: DVec3::ZERO,
                planar_start: DVec3::ZERO,
                relocating: false,
                snapped: false,
                copy: false,
                unlinked: false,
                point: None,
            });
        }
        // One rim (a closed edge): it moves or turns with its flat face, the gizmo at its middle.
        if let Some((edge, pivot, _)) = picked_rim(s) {
            return Some(Self {
                items: Vec::new(),
                faces: None,
                edge: Some(edge),
                frame: GizmoFrame::WORLD,
                pivot,
                dragging: None,
                value: 0.0,
                handle: Handle::Arrow(2),
                planar: DVec3::ZERO,
                planar_start: DVec3::ZERO,
                relocating: false,
                snapped: false,
                copy: false,
                unlinked: false,
                point: None,
            });
        }
        // Faces of one body (nothing else picked): they move on their own, the gizmo at their middle.
        if let Some((body, faces)) = picked_faces(s) {
            let b = s.doc.body(body)?;
            let mut min = DVec3::splat(f64::INFINITY);
            let mut max = DVec3::splat(f64::NEG_INFINITY);
            for &f in &faces {
                let bb = s.kernel.face(&b.shape, f).and_then(|face| s.kernel.bounds(&face)).ok()?;
                min = min.min(bb.min);
                max = max.max(bb.max);
            }
            return Some(Self {
                items: Vec::new(),
                faces: Some((body, faces)),
                edge: None,
                frame: GizmoFrame::WORLD,
                pivot: (min + max) / 2.0,
                dragging: None,
                value: 0.0,
                handle: Handle::Arrow(2),
                planar: DVec3::ZERO,
                planar_start: DVec3::ZERO,
                relocating: false,
                snapped: false,
                copy: false,
                unlinked: false,
                point: None,
            });
        }
        if items.is_empty() {
            let (frame, pivot, _) = selected_curves_center(s)?;
            return Some(Self {
                items,
                faces: None,
                edge: None,
                frame,
                pivot,
                dragging: None,
                value: 0.0,
                handle: Handle::Arrow(0),
                planar: DVec3::ZERO,
                planar_start: DVec3::ZERO,
                relocating: false,
                snapped: false,
                copy: false,
                unlinked: false,
                point: None,
            });
        }
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        for id in &items {
            // A construction plane counts as its center.
            if let Some(pl) = s.doc.plane(*id) {
                min = min.min(pl.plane.origin);
                max = max.max(pl.plane.origin);
                continue;
            }
            let bb = s.doc.body(*id).and_then(|b| s.kernel.bounds(&b.shape).ok())?;
            min = min.min(bb.min);
            max = max.max(bb.max);
        }
        Some(Self {
            items,
            faces: None,
            edge: None,
            frame: GizmoFrame::WORLD,
            pivot: (min + max) / 2.0,
            dragging: None,
            value: 0.0,
            handle: Handle::Arrow(2),
            planar: DVec3::ZERO,
            planar_start: DVec3::ZERO,
            relocating: false,
            snapped: false,
            copy: false,
            unlinked: false,
            point: None,
        })
    }

    /// The command for the active handle and amount.
    pub fn request(&self, value: f64) -> (&'static str, serde_json::Value) {
        let mut v = match self.handle {
            Handle::Arrow(a) => {
                let by = self.frame.axes[a.min(2)] * value;
                json!({ "by": [by.x, by.y, by.z] })
            }
            Handle::Plane(_) => json!({ "by": [self.planar.x, self.planar.y, self.planar.z] }),
            Handle::Ring(a) => {
                let ax = self.frame.axes[a.min(2)];
                json!({ "origin": [self.pivot.x, self.pivot.y, self.pivot.z], "axis": [ax.x, ax.y, ax.z], "angle": value })
            }
        };
        if let Some((body, edge)) = self.edge {
            v["body"] = json!(body.0);
            v["edge"] = json!(edge);
            return ("move_edge", v);
        }
        if let Some(pm) = &self.point {
            let by = match self.handle {
                Handle::Arrow(a) => self.frame.axes[a.min(2)] * value,
                Handle::Plane(_) => self.planar,
                Handle::Ring(_) => DVec3::ZERO,
            };
            return ("set_entity", json!({ "sketch": pm.sketch.0, "entities": pm.edits(by) }));
        }
        if let Some((body, faces)) = &self.faces {
            v["body"] = json!(body.0);
            v["faces"] = json!(faces);
            return ("move_faces", v);
        }
        with_items(&mut v, &self.items);
        if self.copy {
            v["copy"] = json!(true);
            if self.unlinked && !self.items.is_empty() {
                v["linked"] = json!(false);
            }
        }
        (if matches!(self.handle, Handle::Ring(_)) { "rotate" } else { "translate" }, v)
    }

    /// Where the gizmo sits: it moves with the items while the step is live (`pivot` is where it started).
    pub fn shown_pivot(&self) -> DVec3 {
        match self.handle {
            Handle::Arrow(a) => self.pivot + self.frame.axes[a.min(2)] * self.value,
            Handle::Plane(_) => self.pivot + self.planar,
            Handle::Ring(_) => self.pivot,
        }
    }

    /// Starts over from where the items are now (the live step is done).
    pub fn settle(&mut self) {
        self.pivot = self.shown_pivot();
        self.value = 0.0;
        self.planar = DVec3::ZERO;
        self.planar_start = DVec3::ZERO;
    }

    /// Whether there's anything to apply for `value` on the active handle.
    pub fn moves(&self, value: f64) -> bool {
        match self.handle {
            Handle::Plane(_) => self.planar.length() > 1e-9,
            _ => value.abs() > 1e-9,
        }
    }
}

/// The picked faces when the pick is only faces, all of one body (outside a sketch).
pub(crate) fn picked_faces(s: &Session) -> Option<(ItemId, Vec<u32>)> {
    if s.active_sketch.is_some() {
        return None;
    }
    let mut body = None;
    let mut faces = Vec::new();
    for p in &s.selection {
        match *p {
            Pick::Face { body: b, face } if body.is_none_or(|x| x == b) => {
                body = Some(b);
                faces.push(face);
            }
            _ => return None,
        }
    }
    Some((body?, faces))
}

fn selected_items_and_center(s: &Session) -> Option<(Vec<ItemId>, DVec3)> {
    selected_items_bounds(s).map(|(i, min, max)| (i, (min + max) / 2.0))
}

pub(super) fn selected_items_bounds(s: &Session) -> Option<(Vec<ItemId>, DVec3, DVec3)> {
    let mut items: Vec<ItemId> = Vec::new();
    for b in s.selection.iter().filter_map(Pick::body) {
        if !items.contains(&b) {
            items.push(b);
        }
    }
    if items.is_empty() {
        return None;
    }
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for id in &items {
        let bb = s.doc.body(*id).and_then(|b| s.kernel.bounds(&b.shape).ok())?;
        min = min.min(bb.min);
        max = max.max(bb.max);
    }
    Some((items, min, max))
}

/// The picked construction planes and the corners of the space they take up on screen.
pub(super) fn selected_planes_bounds(s: &Session) -> Option<(Vec<ItemId>, DVec3, DVec3)> {
    let items: Vec<ItemId> =
        s.selection.iter().filter_map(|p| if let Pick::Plane { plane } = p { Some(*plane) } else { None }).collect();
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for id in &items {
        let p = s.doc.plane(*id)?;
        for (u, v) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            let q = p.plane.origin + p.plane.x * (u * p.size) + p.plane.y * (v * p.size);
            min = min.min(q);
            max = max.max(q);
        }
    }
    (!items.is_empty()).then_some((items, min, max))
}

/// One picked edge a tool can work on by itself: a rim (closed) or a straight edge; with its middle and half size
/// per axis.
fn picked_edge(s: &Session) -> Option<((ItemId, u32), DVec3, [f64; 3])> {
    picked_rim(s).or_else(|| {
        let [Pick::Edge { body, edge }] = s.selection.as_slice() else { return None };
        let b = s.doc.body(*body)?;
        let k = b.mesh.edge_ids.iter().position(|e| e == edge)?;
        let line = b.mesh.edges.get(k)?;
        let (a, z) = (v3(*line.first()?), v3(*line.last()?));
        let dir = (z - a).normalize_or_zero();
        let straight = line.iter().all(|q| (v3(*q) - a).cross(dir).length() < 1e-6 * (1.0 + (z - a).length()));
        (straight && (z - a).length() > 1e-9)
            .then(|| ((*body, *edge), (a + z) / 2.0, ((z - a).abs() / 2.0).max(DVec3::splat(1e-3)).to_array()))
    })
}

/// One picked closed edge (a rim): it, the middle of its drawn points and their half size per axis.
fn picked_rim(s: &Session) -> Option<((ItemId, u32), DVec3, [f64; 3])> {
    let [Pick::Edge { body, edge }] = s.selection.as_slice() else { return None };
    let b = s.doc.body(*body)?;
    let k = b.mesh.edge_ids.iter().position(|e| e == edge)?;
    let pts: Vec<DVec3> = b.mesh.edges.get(k)?.iter().map(|q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64)).collect();
    let (first, last) = (*pts.first()?, *pts.last()?);
    let min = pts.iter().fold(DVec3::splat(f64::INFINITY), |m, q| m.min(*q));
    let max = pts.iter().fold(DVec3::splat(f64::NEG_INFINITY), |m, q| m.max(*q));
    let size = (max - min).length();
    if pts.len() < 3 || size <= 1e-9 || first.distance(last) > size * 1e-4 {
        return None;
    }
    let center = s.kernel.measure(&b.shape, shapecraft_kernel::SubShape::Edge(*edge)).ok().map(|m| m.1).unwrap_or((min + max) / 2.0);
    Some(((*body, *edge), center, ((max - min) / 2.0).max(DVec3::splat(1e-3)).to_array()))
}

/// Bodies go in "items"; with none, the command works on the selected sketch curves.
pub(super) fn with_items(v: &mut serde_json::Value, items: &[ItemId]) {
    if !items.is_empty() {
        v["items"] = json!(items.iter().map(|i| i.0).collect::<Vec<_>>());
    }
}

impl MirrorTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        // A construction plane on its own is what to mirror across; the items are picked then.
        if let [Pick::Plane { plane }] = s.selection.as_slice()
            && let Some(p) = s.doc.plane(*plane)
        {
            return Some(Self {
                items: Vec::new(),
                center: p.plane.origin,
                size: p.size,
                hovered: None,
                split: false,
                keep: true,
                with: Vec::new(),
                pieces: Vec::new(),
                across: Some(*plane),
            });
        }
        match selected_items_bounds(s) {
            Some((items, min, max)) => Some(Self {
                across: None,
                items,
                center: (min + max) / 2.0,
                size: (max - min).length(),
                hovered: None,
                split: false,
                keep: true,
                with: Vec::new(),
                pieces: Vec::new(),
            }),
            None => {
                let (_, center, size) = selected_curves_center(s)?;
                Some(Self {
                    across: None,
                    items: Vec::new(),
                    center,
                    size: size.max(10.0),
                    hovered: None,
                    split: false,
                    keep: true,
                    with: Vec::new(),
                    pieces: Vec::new(),
                })
            }
        }
    }
}

impl MirrorTool {
    /// Mirroring across the plane picked first, once there are items.
    pub fn across_request(&self) -> Option<(&'static str, serde_json::Value)> {
        let plane = self.across.filter(|_| !self.items.is_empty() && !self.split)?;
        let items: Vec<u64> = self.items.iter().map(|i| i.0).collect();
        Some(("mirror", json!({ "plane": { "plane": plane.0 }, "items": items, "copy": self.keep })))
    }
}

impl OffsetFaceTool {
    /// `at` is where the face was clicked: the arrow sits there, as in the reference (else on the face's middle).
    pub fn from_selection(s: &Session, at: Option<DVec3>) -> Option<Self> {
        let mut body = None;
        let mut faces = Vec::new();
        for pick in &s.selection {
            if let Pick::Face { body: b, face } = *pick
                && body.is_none_or(|x| x == b)
            {
                body = Some(b);
                faces.push(face);
            }
        }
        let body = body?;
        let b = s.doc.body(body)?;
        // The arrow sits on the face picked last (where the last click was).
        let first = *faces.last()?;
        let (anchor, dir) = match s.kernel.face_plane(&b.shape, first).ok().flatten() {
            Some(fp) => match at {
                Some(p) if (p - fp.origin).dot(fp.normal).abs() < 1e-3 => (p, fp.normal),
                _ => (fp.origin, fp.normal),
            },
            None if let Some(hit) = at.and_then(|p| curved_normal_at(&b.mesh, first, p).map(|n| (p, n))) => hit,
            None => {
                // Curved: the face's center and its average normal from the display mesh.
                let (_, center) = s.kernel.measure(&b.shape, shapecraft_kernel::SubShape::Face(first)).ok()?;
                let m = &b.mesh;
                let mut n = DVec3::ZERO;
                for (t, f) in m.triangle_faces.iter().enumerate() {
                    if *f == first {
                        for k in 0..3 {
                            n += m.indices.get(t * 3 + k).and_then(|i| m.normals.get(*i as usize)).map(|v| v3(*v)).unwrap_or(DVec3::ZERO);
                        }
                    }
                }
                // A full cylinder's normals cancel out; point at the face from the body's middle instead.
                let n = n.try_normalize().or_else(|| {
                    let bb = s.kernel.bounds(&b.shape).ok()?;
                    (center - (bb.min + bb.max) / 2.0).try_normalize()
                })?;
                (center, n)
            }
        };
        let flat = s.kernel.face_plane(&b.shape, first).ok().flatten().is_some();
        // Only a cylinder's wall reads as a radius; a cone or a sphere just offsets (26.202, s40-023/024).
        let round = !flat && s.face_radius(body, first).is_some();
        let radius = if !round {
            None
        } else {
            s.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Face(first)).ok().flatten().and_then(|axis| {
                let dir = axis.dir.try_normalize()?;
                let foot = axis.origin + dir * (anchor - axis.origin).dot(dir);
                // The click lands on a flat mesh facet, a little inside the true surface: read the radius off the
                // face's mesh points instead (they lie on it), so a 5 mm cylinder reads 5, not 4.9907.
                let r = surface_radius(&b.mesh, first, axis.origin, dir, anchor).unwrap_or_else(|| anchor.distance(foot));
                (r > 1e-6).then_some((r, foot))
            })
        };
        // The handle sits on the true surface too.
        let anchor = match radius {
            Some((r, foot)) => (anchor - foot).try_normalize().map_or(anchor, |u| foot + u * r),
            None => anchor,
        };
        let thickness = if flat { opposite_face_distance(&b.mesh, anchor, dir, first) } else { None };
        let fillet = match faces.as_slice() {
            [f] => shapecraft_engine::history::fillet_step_for(s, body, *f).and_then(|(step, radius, convex)| {
                Some(FilletFace { step, label: s.doc.history.get(step)?.label.clone(), radius, convex })
            }),
            _ => None,
        };
        // Several faces just offset: a plain "0 mm" pill, no Total / Radius choice.
        let kinds = (flat || radius.is_some()) && faces.len() == 1;
        Some(Self { body, faces, distance: 0.0, anchor, dir, dragging: None, thickness, radius, fillet, kinds, whole: false })
    }

    /// +1 when pulling out of the part grows the radius (a shaft), -1 when it shrinks it (a hole).
    fn radius_sign(&self, foot: DVec3) -> f64 {
        if self.dir.dot(self.anchor - foot) >= 0.0 { 1.0 } else { -1.0 }
    }

    /// The value shown and typed for a distance type: the offset itself, the part's whole thickness after it
    /// (Total), or a turned face's radius or diameter after it.
    pub fn shown(&self, kind: OffsetKind) -> f64 {
        if let Some(f) = &self.fillet {
            return f.radius_after(self.distance);
        }
        if let Some((r, foot)) = self.radius {
            let after = r + self.radius_sign(foot) * self.distance;
            return match kind {
                OffsetKind::Radius => after,
                OffsetKind::Diameter => 2.0 * after,
                _ => self.distance,
            };
        }
        match self.thickness {
            Some(t) if kind == OffsetKind::Total => t + self.distance,
            _ => self.distance,
        }
    }

    pub fn set_shown(&mut self, kind: OffsetKind, v: f64) {
        // A typed radius for a fillet's face.
        if let Some(f) = &self.fillet {
            self.distance = if f.convex { f.radius - v } else { v - f.radius };
            return;
        }
        if let Some((r, foot)) = self.radius {
            match kind {
                OffsetKind::Radius => return self.distance = (v - r) * self.radius_sign(foot),
                OffsetKind::Diameter => return self.distance = (v / 2.0 - r) * self.radius_sign(foot),
                _ => {}
            }
        }
        self.distance = match self.thickness {
            Some(t) if kind == OffsetKind::Total => v - t,
            _ => v,
        };
    }

    pub fn request(&self, distance: f64) -> (&'static str, serde_json::Value) {
        match &self.fillet {
            Some(f) if f.radius_after(distance) <= 1e-6 => ("delete_step", json!({ "step": f.step })),
            Some(f) => ("edit_step", json!({ "step": f.step, "params": { "radius": f.radius_after(distance) } })),
            None => ("offset_face", json!({ "body": self.body.0, "faces": self.faces, "distance": distance })),
        }
    }
}

/// The handle a new Scale starts on: the one hammer for uniform, X for non-uniform (lit blue at once).
pub(crate) fn first_scale_handle(uniform: bool) -> Handle {
    if uniform { Handle::Arrow(2) } else { Handle::Arrow(0) }
}

impl ScaleTool {
    /// Picked bodies, or (with none) picked sketch curves: the gizmo sits in the middle of what it scales.
    pub fn from_selection(s: &Session, uniform: bool) -> Option<Self> {
        if let Some((edge, pivot, half)) = picked_edge(s) {
            return Some(Self {
                items: Vec::new(),
                edge: Some(edge),
                pivot,
                half,
                uniform,
                handle: first_scale_handle(uniform),
                factor: 1.0,
                dragging: None,
                relocating: false,
                snapped: false,
                copy: false,
            });
        }
        let (items, pivot, half) = match selected_items_bounds(s).or_else(|| selected_planes_bounds(s)) {
            Some((items, min, max)) => (items, (min + max) / 2.0, ((max - min) / 2.0).max(DVec3::splat(1e-3)).to_array()),
            None => {
                let (_, center, size) = selected_curves_center(s)?;
                (Vec::new(), center, [(size / 2.0).max(1e-3); 3])
            }
        };
        Some(Self {
            items,
            edge: None,
            pivot,
            half,
            uniform,
            handle: first_scale_handle(uniform),
            factor: 1.0,
            dragging: None,
            relocating: false,
            snapped: false,
            copy: false,
        })
    }

    /// The factor per axis for `f` on the active handle.
    pub fn factors(&self, f: f64) -> [f64; 3] {
        let mut out = [1.0; 3];
        match self.handle {
            _ if self.uniform => out = [f; 3],
            Handle::Arrow(a) => out[a.min(2)] = f,
            Handle::Plane(a) | Handle::Ring(a) => {
                out[(a + 1) % 3] = f;
                out[(a + 2) % 3] = f;
            }
        }
        out
    }

    /// How many mm of drag add 1× to the factor: the size the handle stretches.
    pub fn mm_per_unit(&self) -> f64 {
        let h = self.half;
        match self.handle {
            _ if self.uniform => (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt(),
            Handle::Arrow(a) => h[a.min(2)],
            Handle::Plane(a) | Handle::Ring(a) => (h[(a + 1) % 3] + h[(a + 2) % 3]) / 2.0,
        }
        .max(1e-6)
    }

    pub fn request(&self, f: f64) -> (&'static str, serde_json::Value) {
        let c = self.pivot;
        let mut v = json!({ "factors": self.factors(f), "center": [c.x, c.y, c.z], "copy": self.copy });
        if let Some((body, edge)) = self.edge {
            v["body"] = json!(body.0);
            v["edge"] = json!(edge);
        }
        with_items(&mut v, &self.items);
        ("scale", v)
    }

    pub fn changed(&self, f: f64) -> bool {
        f > 0.0 && (f - 1.0).abs() > 1e-9
    }
}

impl PatternTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        match selected_items_and_center(s) {
            Some((items, pivot)) => Some(Self {
                items,
                frame: GizmoFrame::WORLD,
                pivot,
                axis: 0,
                spacing: 0.0,
                count: 3,
                dragging: None,
                total: false,
                result: "new",
                count_field: false,
            }),
            None => {
                let (frame, pivot, _) = selected_curves_center(s)?;
                Some(Self {
                    items: Vec::new(),
                    frame,
                    pivot,
                    axis: 0,
                    spacing: 0.0,
                    count: 3,
                    dragging: None,
                    total: false,
                    result: "new",
                    count_field: false,
                })
            }
        }
    }

    pub fn request(&self, pulled: f64) -> (&'static str, serde_json::Value) {
        let spacing = if self.total { pulled / f64::from(self.count.max(2) - 1) } else { pulled };
        let d = self.frame.axes[self.axis.min(2)];
        let mut v = json!({ "direction": [d.x, d.y, d.z], "spacing": spacing, "count": self.count });
        if self.result != "new" && !self.items.is_empty() {
            v["result"] = json!(self.result);
        }
        with_items(&mut v, &self.items);
        ("pattern", v)
    }
}

impl ShellTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let mut body = None;
        let mut faces = Vec::new();
        for pick in &s.selection {
            match *pick {
                Pick::Face { body: b, face } if body.is_none_or(|x| x == b) => {
                    body = Some(b);
                    faces.push(face);
                }
                Pick::Body { body: b } if body.is_none() => body = Some(b),
                _ => {}
            }
        }
        let body = body?;
        let b = s.doc.body(body)?;
        let fp = faces.first().and_then(|f| s.kernel.face_plane(&b.shape, *f).ok().flatten());
        let (anchor, dir) = match fp {
            Some(fp) => (fp.origin, -fp.normal),
            None => {
                let bb = s.kernel.bounds(&b.shape).ok()?;
                (DVec3::new((bb.min.x + bb.max.x) / 2.0, (bb.min.y + bb.max.y) / 2.0, bb.max.z), -DVec3::Z)
            }
        };
        Some(Self { body, faces, thickness: 0.0, anchor, dir, dragging: None })
    }
}

impl RevolveTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let profile = s.selection.iter().copied().find(|p| match *p {
            Pick::Region { .. } => true,
            Pick::Face { body, face } => s.doc.body(body).is_some_and(|b| s.kernel.face_plane(&b.shape, face).ok().flatten().is_some()),
            _ => false,
        })?;
        Some(Self { profile, axis: None, angle: 360.0, result: "auto", dragging: None, height: 0.0, lifting: None, height_typed: false })
    }

    pub fn sketch(&self) -> Option<ItemId> {
        match self.profile {
            Pick::Region { sketch, .. } => Some(sketch),
            _ => None,
        }
    }

    /// The axis it spins around in world space (origin, unit direction), as the engine reads `params`.
    pub fn axis_line(&self, s: &Session) -> Option<(DVec3, DVec3)> {
        let sketch = self.sketch().and_then(|id| s.doc.sketch(id));
        let (o, d) = match self.axis {
            None => return None,
            Some(AxisPick::Line(i)) => match sketch?.entities.get(i as usize)? {
                shapecraft_doc::SketchEntity::Line { a, b } => {
                    let sk = sketch?;
                    let (wa, wb) = (sk.plane.to_world(*a), sk.plane.to_world(*b));
                    (wa, wb - wa)
                }
                _ => return None,
            },
            Some(AxisPick::Topo(Pick::Axis { axis })) => {
                let a = s.doc.axis(axis)?;
                (a.origin, a.dir)
            }
            Some(AxisPick::Topo(p)) => {
                let (body, sub) = match p {
                    Pick::Edge { body, edge } => (body, shapecraft_kernel::SubShape::Edge(edge)),
                    Pick::Face { body, face } => (body, shapecraft_kernel::SubShape::Face(face)),
                    _ => return None,
                };
                let a = s.kernel.axis_of(&s.doc.body(body)?.shape, sub).ok()??;
                (a.origin, a.dir)
            }
        };
        (d.length() > 1e-12).then(|| (o, d.normalize()))
    }

    /// The widest angle allowed: one turn, or 200 turns once it rises (a helix).
    pub fn max_angle(&self) -> f64 {
        if self.height.abs() > 1e-9 { 360.0 * 200.0 } else { 360.0 }
    }

    /// The command parameters, or `None` while the profile still waits for its axis: nothing spins (and nothing
    /// previews) until a line, edge, round face or axis is clicked, for a region as for a face.
    pub fn params(&self, angle: f64) -> Option<serde_json::Value> {
        let mut v = profile_json(&self.profile)?;
        v["axis"] = self.axis.as_ref()?.json();
        v["angle"] = json!(angle);
        if self.height.abs() > 1e-9 {
            v["height"] = json!(self.height);
        }
        if self.result != "auto" {
            v["result"] = json!(self.result);
        }
        Some(v)
    }
}

fn is_profile(p: &Pick) -> bool {
    matches!(p, Pick::Region { .. } | Pick::Face { .. })
}

fn profile_json(p: &Pick) -> Option<serde_json::Value> {
    match *p {
        Pick::Region { sketch, region } => Some(json!({ "sketch": sketch.0, "region": region })),
        Pick::Face { body, face } => Some(json!({ "body": body.0, "face": face })),
        _ => None,
    }
}

/// Adds the pick to the list, or takes it out if it's there already.
fn toggle(list: &mut Vec<Pick>, p: Pick) {
    if let Some(i) = list.iter().position(|x| *x == p) {
        list.remove(i);
    } else {
        list.push(p);
    }
}

/// A path or guide pick list as the commands take it: each sketch's curves together, then each body's edges.
pub(crate) fn curves_json(picks: &[Pick]) -> Vec<serde_json::Value> {
    let mut out: Vec<(ItemId, bool, Vec<u32>)> = Vec::new();
    for p in picks {
        let (id, edge, n) = match *p {
            Pick::SketchEntity { sketch, entity } => (sketch, false, entity),
            Pick::Edge { body, edge } => (body, true, edge),
            _ => continue,
        };
        match out.iter_mut().find(|(i, e, _)| *i == id && *e == edge) {
            Some((_, _, list)) => list.push(n),
            None => out.push((id, edge, vec![n])),
        }
    }
    out.into_iter()
        .map(|(id, edge, list)| if edge { json!({ "body": id.0, "edges": list }) } else { json!({ "sketch": id.0, "entities": list }) })
        .collect()
}

impl LoftTool {
    pub fn from_selection(s: &Session) -> Self {
        Self {
            profiles: s.selection.iter().copied().filter(is_profile).collect(),
            guides: s.selection.iter().copied().filter(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Edge { .. })).collect(),
            set: serde_json::Map::new(),
            result: "new",
        }
    }

    /// A click on a region or face adds it as the next profile (or drops it); one on a curve or edge, as a guide.
    pub fn click(&mut self, p: Pick) -> bool {
        if matches!(p, Pick::SketchEntity { .. } | Pick::Edge { .. }) {
            toggle(&mut self.guides, p);
            return true;
        }
        if !is_profile(&p) {
            return false;
        }
        toggle(&mut self.profiles, p);
        true
    }

    pub fn picks(&self) -> Vec<Pick> {
        self.profiles.iter().chain(&self.guides).copied().collect()
    }

    pub fn request(&self) -> Option<(&'static str, serde_json::Value)> {
        let profiles: Vec<_> = self.profiles.iter().filter_map(profile_json).collect();
        (profiles.len() >= 2).then(|| {
            let mut v = json!({ "profiles": profiles });
            if !self.guides.is_empty() {
                v["guides"] = json!(curves_json(&self.guides));
            }
            for (key, value) in &self.set {
                v[key] = value.clone();
            }
            if !matches!(self.result, "" | "new") {
                v["result"] = json!(self.result);
            }
            ("loft", v)
        })
    }
}

impl FillTool {
    pub fn from_selection(s: &Session) -> Self {
        let mut t = Self {
            profiles: s.selection.iter().copied().filter(is_profile).collect(),
            guides: s.selection.iter().copied().filter(|p| matches!(p, Pick::SketchEntity { .. })).collect(),
            continuity: "g0",
            result: "new",
            ..Self::default()
        };
        t.refresh(s);
        t
    }

    /// Profiles and sketch curves toggle (body edges can't be guides there either).
    pub fn click(&mut self, s: &Session, p: Pick) -> bool {
        match p {
            Pick::SketchEntity { .. } => toggle(&mut self.guides, p),
            _ if is_profile(&p) => toggle(&mut self.profiles, p),
            _ => return false,
        }
        self.refresh(s);
        true
    }

    pub fn picks(&self) -> Vec<Pick> {
        self.profiles.iter().chain(&self.guides).copied().collect()
    }

    /// Works out where the badges go (when the picks make a closed network).
    pub fn refresh(&mut self, s: &Session) {
        self.marks.clear();
        let Some(sides) = self.params().and_then(|p| shapecraft_engine::cmd::fill_sides(s, &p).ok()) else { return };
        let choice = |side: &shapecraft_engine::cmd::FillSide| ["g0", "g1", "g2"][usize::from(side.order.min(2))];
        self.marks = sides.iter().filter(|x| !x.fixed).map(|x| (x.mid, choice(x))).collect();
    }

    pub fn set_side(&mut self, s: &Session, i: usize, to: &'static str) {
        let Some((at, _)) = self.marks.get(i).copied() else { return };
        self.sides.retain(|(q, _)| q.distance(at) > 1e-6);
        self.sides.push((at, to));
        self.refresh(s);
    }

    pub fn set_all(&mut self, s: &Session, to: &'static str) {
        self.continuity = to;
        self.sides.clear();
        self.refresh(s);
    }

    pub fn params(&self) -> Option<serde_json::Value> {
        if self.profiles.is_empty() && self.guides.is_empty() {
            return None;
        }
        let profiles: Vec<_> = self.profiles.iter().filter_map(profile_json).collect();
        let mut v = json!({ "profiles": profiles, "guides": curves_json(&self.guides) });
        if self.continuity != "g0" {
            v["continuity"] = json!(self.continuity);
        }
        if !self.sides.is_empty() {
            let sides: Vec<_> = self.sides.iter().map(|(q, c)| json!({ "at": [q.x, q.y, q.z], "continuity": c })).collect();
            v["sides"] = json!(sides);
        }
        if !matches!(self.result, "" | "new") {
            v["result"] = json!(self.result);
        }
        Some(v)
    }

    /// Only a closed network can be filled.
    pub fn request(&self) -> Option<(&'static str, serde_json::Value)> {
        if self.marks.is_empty() { None } else { self.params().map(|p| ("fill", p)) }
    }
}

impl SweepTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let profile = s.selection.iter().copied().find(is_profile)?;
        let path = s.selection.iter().copied().filter(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Edge { .. })).collect();
        Some(Self { profile, path, result: "new", set: serde_json::Map::new() })
    }

    /// Sketch curves and edges toggle in and out of the path; a region or face becomes the profile.
    pub fn click(&mut self, p: Pick) -> bool {
        match p {
            Pick::SketchEntity { .. } | Pick::Edge { .. } => {
                toggle(&mut self.path, p);
                true
            }
            _ if is_profile(&p) => {
                self.profile = p;
                true
            }
            _ => false,
        }
    }

    pub fn request(&self) -> Option<(&'static str, serde_json::Value)> {
        let mut path = curves_json(&self.path);
        if path.is_empty() {
            return None;
        }
        let one = path.len() == 1 && path[0].get("sketch").is_some();
        let path = if one { path.remove(0) } else { json!(path) };
        let mut v = json!({ "profile": profile_json(&self.profile)?, "path": path });
        if self.result != "new" {
            v["result"] = json!(self.result);
        }
        for (key, value) in &self.set {
            v[key] = value.clone();
        }
        Some(("sweep", v))
    }

    pub fn picks(&self) -> Vec<Pick> {
        std::iter::once(self.profile).chain(self.path.iter().copied()).collect()
    }
}

impl OffsetEdgeTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let mut sketch = None;
        let mut entities = Vec::new();
        let mut region = None;
        // In a sketch, picked edges of bodies on its plane, when no curve of it is picked.
        let edges: Vec<(ItemId, u32)> = s
            .selection
            .iter()
            .filter_map(|p| match *p {
                Pick::Edge { body, edge } => Some((body, edge)),
                _ => None,
            })
            .collect();
        if let (Some(open), false, false) =
            (s.active_sketch, edges.is_empty(), s.selection.iter().any(|p| matches!(p, Pick::SketchEntity { .. } | Pick::Region { .. })))
        {
            return Self::from_edges(s, open, edges);
        }
        for pick in &s.selection {
            match *pick {
                Pick::SketchEntity { sketch: sk, entity } if sketch.is_none_or(|x| x == sk) && region.is_none() => {
                    sketch = Some(sk);
                    entities.push(entity);
                }
                Pick::Region { sketch: sk, region: r } if sketch.is_none() => {
                    sketch = Some(sk);
                    region = Some(r);
                }
                _ => {}
            }
        }
        let sketch = sketch?;
        let sk = s.doc.sketch(sketch)?;
        // Where a 1 mm offset lands next to the curve: that's the arrow's direction.
        let probe = shapecraft_engine::offset_entities(s, sketch, &entities, region, 1.0).ok()?;
        let base: Vec<DVec2> = match region {
            Some(r) => sk.region(r)?.mesh.edges.first()?.iter().map(|p| sk.plane.to_local(v3(*p))).collect(),
            None => sk.entities.get(*entities.first()? as usize)?.outline(),
        };
        let p0 = (*base.first()? + *base.get(1)?) / 2.0;
        let nearest = nearest_on(&probe, p0)?;
        let w0 = sk.plane.to_world(p0);
        let dir = (sk.plane.to_world(nearest) - w0).try_normalize()?;
        let picked = entities.clone();
        Some(Self { sketch, entities, region, face: None, edges: Vec::new(), distance: 0.0, anchor: w0, dir, dragging: None, picked })
    }

    /// Edges of bodies on the open sketch's plane: the arrow on the first one, where a 1 mm offset lands.
    fn from_edges(s: &Session, sketch: ItemId, edges: Vec<(ItemId, u32)>) -> Option<Self> {
        let sk = s.doc.sketch(sketch)?;
        let probe = shapecraft_engine::offset_body_edges(s, sketch, &edges, 1.0).ok()?;
        let first = shapecraft_engine::body_edge_entities(s, sketch, &edges[..1]).ok()?;
        let base = first.first()?.outline();
        let p0 = (*base.first()? + *base.get(1)?) / 2.0;
        let nearest = nearest_on(&probe, p0)?;
        let w0 = sk.plane.to_world(p0);
        let dir = (sk.plane.to_world(nearest) - w0).try_normalize()?;
        Some(Self {
            sketch,
            entities: Vec::new(),
            region: None,
            face: None,
            edges,
            distance: 0.0,
            anchor: w0,
            dir,
            dragging: None,
            picked: Vec::new(),
        })
    }

    /// On a picked flat body face: its whole outline, the arrow on its first edge (s24-008).
    pub fn from_face(s: &Session) -> Option<Self> {
        let [Pick::Face { body, face }] = s.selection.as_slice() else { return None };
        let (plane, _, probe) = shapecraft_engine::face_offset(s, *body, *face, 1.0).ok()?;
        let m = &s.doc.body(*body)?.mesh;
        let k = m.edge_faces.iter().position(|f| f.contains(face))?;
        let edge = m.edges.get(k)?;
        let p0 = plane.to_local((v3(*edge.first()?) + v3(*edge.get(1)?)) / 2.0);
        let nearest = nearest_on(&probe, p0)?;
        let w0 = plane.to_world(p0);
        let dir = (plane.to_world(nearest) - w0).try_normalize()?;
        Some(Self {
            sketch: ItemId(0),
            entities: Vec::new(),
            region: None,
            face: Some((*body, *face)),
            edges: Vec::new(),
            distance: 0.0,
            anchor: w0,
            dir,
            dragging: None,
            picked: Vec::new(),
        })
    }

    pub fn request(&self, distance: f64) -> (&'static str, serde_json::Value) {
        if let Some((body, face)) = self.face {
            return ("offset_edge", json!({ "body": body.0, "face": face, "distance": distance }));
        }
        if !self.edges.is_empty() {
            let edges: Vec<serde_json::Value> = self.edges.iter().map(|(b, e)| json!({ "body": b.0, "edge": e })).collect();
            return ("offset_edge", json!({ "sketch": self.sketch.0, "edges": edges, "distance": distance }));
        }
        match self.region {
            Some(r) => ("offset_edge", json!({ "sketch": self.sketch.0, "region": r, "distance": distance })),
            None => ("offset_edge", json!({ "sketch": self.sketch.0, "entities": self.entities, "distance": distance })),
        }
    }
}

/// The nearest point to `p` on the curves (not just their corners).
fn nearest_on(curves: &[shapecraft_doc::SketchEntity], p: DVec2) -> Option<DVec2> {
    curves
        .iter()
        .flat_map(|e| {
            e.outline()
                .windows(2)
                .map(|w| {
                    let d = w[1] - w[0];
                    let t = if d.length_squared() > 0.0 { ((p - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                    w[0] + d * t
                })
                .collect::<Vec<_>>()
        })
        .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
}

/// A turned face's radius level with `at`, read off its mesh points (which lie on the true surface): the two rings
/// of points nearest `at` along the axis, interpolated (exact for cylinders and cones).
fn surface_radius(m: &shapecraft_kernel::Mesh, face: u32, origin: DVec3, dir: DVec3, at: DVec3) -> Option<f64> {
    let h = (at - origin).dot(dir);
    // (height along the axis, radius) of the nearest mesh point at or below `at`, and at or above it.
    let mut below: Option<(f64, f64)> = None;
    let mut above: Option<(f64, f64)> = None;
    for (t, f) in m.triangle_faces.iter().enumerate() {
        if *f != face {
            continue;
        }
        for k in 0..3 {
            let Some(p) = m.indices.get(t * 3 + k).and_then(|i| m.positions.get(*i as usize)) else { continue };
            let p = DVec3::from(p.map(f64::from)) - origin;
            let (ph, pr) = (p.dot(dir), p.reject_from_normalized(dir).length());
            if ph <= h && below.is_none_or(|(bh, _)| ph > bh) {
                below = Some((ph, pr));
            }
            if ph >= h && above.is_none_or(|(ah, _)| ph < ah) {
                above = Some((ph, pr));
            }
        }
    }
    match (below, above) {
        (Some((h0, r0)), Some((h1, r1))) if h1 - h0 > 1e-9 => Some(r0 + (r1 - r0) * (h - h0) / (h1 - h0)),
        (Some((_, r)), _) | (_, Some((_, r))) => Some(r),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turned(anchor: DVec3, dir: DVec3) -> OffsetFaceTool {
        OffsetFaceTool {
            body: ItemId(1),
            faces: vec![0],
            distance: 0.0,
            anchor,
            dir,
            dragging: None,
            thickness: None,
            radius: Some((5.0, DVec3::ZERO)),
            fillet: None,
            kinds: true,
            whole: false,
        }
    }

    #[test]
    fn a_turned_face_shows_its_radius_after_the_offset() {
        // A shaft: out of the part points away from the axis, so pulling out grows the radius.
        let mut shaft = turned(DVec3::X * 5.0, DVec3::X);
        shaft.distance = 2.0;
        assert!((shaft.shown(OffsetKind::Radius) - 7.0).abs() < 1e-9);
        assert!((shaft.shown(OffsetKind::Diameter) - 14.0).abs() < 1e-9);
        assert!((shaft.shown(OffsetKind::Offset) - 2.0).abs() < 1e-9);
        shaft.set_shown(OffsetKind::Radius, 4.0);
        assert!((shaft.distance + 1.0).abs() < 1e-9);
        shaft.set_shown(OffsetKind::Diameter, 12.0);
        assert!((shaft.distance - 1.0).abs() < 1e-9);
        // A hole: out of the part points at the axis, so pulling out shrinks it.
        let mut hole = turned(DVec3::X * 5.0, -DVec3::X);
        hole.distance = 2.0;
        assert!((hole.shown(OffsetKind::Radius) - 3.0).abs() < 1e-9);
        hole.set_shown(OffsetKind::Radius, 6.0);
        assert!((hole.distance + 1.0).abs() < 1e-9);
    }
}

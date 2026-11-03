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
        let ghost = Rgba::hex(0x1db3dc);
        // Start ▸ Offset lifts the profile; Symmetric pulls it both ways.
        let base = self.normal * self.start_offset.unwrap_or(0.0);
        let off = self.normal * self.distance;
        if self.distance.abs() > 1e-9 {
            let ends: &[f64] = if self.symmetric { &[1.0, -1.0] } else { &[1.0] };
            for side in ends {
                let end = base + off * *side;
                let from = if self.symmetric { base - off } else { base };
                out.push(Overlay::Fill {
                    tris: self.tris.iter().map(|t| [t[0] + end, t[1] + end, t[2] + end]).collect(),
                    color: ghost.with_alpha(70),
                });
                for (a, b) in &self.outline {
                    out.push(Overlay::Line { a: *a + end, b: *b + end, color: ghost, width: 1.5 });
                    if *side > 0.0 {
                        out.push(Overlay::Line { a: *a + from, b: *a + end, color: ghost.with_alpha(160), width: 1.0 });
                    }
                }
            }
        }
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
        let line: Vec<DVec3> = edges
            .first()
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

    /// Two-Distance's frame: the first picked edge's two faces and, for each, the way along it from the anchor away from
    /// the edge (perpendicular to the edge, on the face).
    fn sides(&self, s: &Session) -> Option<([u32; 2], [DVec3; 2])> {
        let m = &s.doc.body(self.body)?.mesh;
        let k = m.edge_ids.iter().position(|e| Some(e) == self.edges.first())?;
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
        if items.is_empty() {
            let (frame, pivot, _) = selected_curves_center(s)?;
            return Some(Self {
                items,
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
        with_items(&mut v, &self.items);
        if self.copy && !self.items.is_empty() {
            v["copy"] = json!(true);
            if self.unlinked {
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

/// Bodies go in "items"; with none, the command works on the selected sketch curves.
pub(super) fn with_items(v: &mut serde_json::Value, items: &[ItemId]) {
    if !items.is_empty() {
        v["items"] = json!(items.iter().map(|i| i.0).collect::<Vec<_>>());
    }
}

impl MirrorTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        match selected_items_bounds(s) {
            Some((items, min, max)) => Some(Self {
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
        let first = *faces.first()?;
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
        let radius = if flat {
            None
        } else {
            s.kernel.axis_of(&b.shape, shapecraft_kernel::SubShape::Face(first)).ok().flatten().and_then(|axis| {
                let dir = axis.dir.try_normalize()?;
                let foot = axis.origin + dir * (anchor - axis.origin).dot(dir);
                let r = anchor.distance(foot);
                (r > 1e-6).then_some((r, foot))
            })
        };
        let thickness = if radius.is_some() { None } else { opposite_face_distance(&b.mesh, anchor, dir, first) };
        let fillet = match faces.as_slice() {
            [f] => shapecraft_engine::history::fillet_step_for(s, body, *f).and_then(|(step, radius, convex)| {
                Some(FilletFace { step, label: s.doc.history.get(step)?.label.clone(), radius, convex })
            }),
            _ => None,
        };
        Some(Self { body, faces, distance: 0.0, anchor, dir, dragging: None, thickness, radius, fillet })
    }

    /// +1 when pulling out of the part grows the radius (a shaft), -1 when it shrinks it (a hole).
    fn radius_sign(&self, foot: DVec3) -> f64 {
        if self.dir.dot(self.anchor - foot) >= 0.0 { 1.0 } else { -1.0 }
    }

    /// The value shown and typed: the offset, or with "Total" the part's whole thickness after it (a turned face:
    /// its radius after it).
    pub fn shown(&self, total: bool) -> f64 {
        if let Some(f) = &self.fillet {
            return f.radius_after(self.distance);
        }
        if let Some((r, foot)) = self.radius
            && total
        {
            return r + self.radius_sign(foot) * self.distance;
        }
        match self.thickness {
            Some(t) if total => t + self.distance,
            _ => self.distance,
        }
    }

    pub fn set_shown(&mut self, total: bool, v: f64) {
        // A typed radius for a fillet's face.
        if let Some(f) = &self.fillet {
            self.distance = if f.convex { f.radius - v } else { v - f.radius };
            return;
        }
        if let Some((r, foot)) = self.radius
            && total
        {
            self.distance = (v - r) * self.radius_sign(foot);
            return;
        }
        self.distance = match self.thickness {
            Some(t) if total => v - t,
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

impl ScaleTool {
    pub fn from_selection(s: &Session, uniform: bool) -> Option<Self> {
        let (items, min, max) = selected_items_bounds(s)?;
        let half = ((max - min) / 2.0).max(DVec3::splat(1e-3)).to_array();
        Some(Self {
            items,
            pivot: (min + max) / 2.0,
            half,
            uniform,
            handle: Handle::Arrow(2),
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
        let ids: Vec<u64> = self.items.iter().map(|i| i.0).collect();
        let c = self.pivot;
        ("scale", json!({ "items": ids, "factors": self.factors(f), "center": [c.x, c.y, c.z], "copy": self.copy }))
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
            }),
            None => {
                let (frame, pivot, _) = selected_curves_center(s)?;
                Some(Self { items: Vec::new(), frame, pivot, axis: 0, spacing: 0.0, count: 3, dragging: None, total: false, result: "new" })
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
        Some(Self { profile, axis: None, angle: 360.0, result: "auto", dragging: None })
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
            None => {
                let sk = sketch?;
                (sk.plane.origin, sk.plane.y)
            }
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

    /// The command parameters, or `None` while a face still waits for its axis.
    pub fn params(&self, angle: f64) -> Option<serde_json::Value> {
        let mut v = profile_json(&self.profile)?;
        let axis = match (&self.axis, self.profile) {
            (Some(a), _) => a.json(),
            (None, Pick::Region { .. }) => json!("v"),
            (None, _) => return None,
        };
        v["axis"] = axis;
        v["angle"] = json!(angle);
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

impl LoftTool {
    pub fn from_selection(s: &Session) -> Self {
        Self { profiles: s.selection.iter().copied().filter(is_profile).collect(), ruled: false, result: "new" }
    }

    /// A click on a region or face adds it as the next profile (or drops it).
    pub fn click(&mut self, p: Pick) -> bool {
        if !is_profile(&p) {
            return false;
        }
        toggle(&mut self.profiles, p);
        true
    }

    pub fn request(&self) -> Option<(&'static str, serde_json::Value)> {
        let profiles: Vec<_> = self.profiles.iter().filter_map(profile_json).collect();
        (profiles.len() >= 2).then(|| {
            let mut v = json!({ "profiles": profiles });
            if self.ruled {
                v["ruled"] = json!(true);
            }
            if !matches!(self.result, "" | "new") {
                v["result"] = json!(self.result);
            }
            ("loft", v)
        })
    }
}

impl SweepTool {
    pub fn from_selection(s: &Session) -> Option<Self> {
        let profile = s.selection.iter().copied().find(is_profile)?;
        let path = s.selection.iter().copied().filter(|p| matches!(p, Pick::SketchEntity { .. })).collect();
        Some(Self { profile, path, result: "new" })
    }

    /// Sketch curves toggle in and out of the path (one sketch at a time); a region or face becomes the profile.
    pub fn click(&mut self, p: Pick) -> bool {
        match p {
            Pick::SketchEntity { sketch, .. } => {
                self.path.retain(|x| x.sketch() == Some(sketch));
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
        let sketch = self.path.first()?.sketch()?;
        let entities: Vec<u32> = self
            .path
            .iter()
            .filter_map(|p| match *p {
                Pick::SketchEntity { entity, .. } => Some(entity),
                _ => None,
            })
            .collect();
        let mut v = json!({ "profile": profile_json(&self.profile)?, "path": { "sketch": sketch.0, "entities": entities } });
        if self.result != "new" {
            v["result"] = json!(self.result);
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
        Some(Self { sketch, entities, region, face: None, distance: 0.0, anchor: w0, dir, dragging: None, picked })
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
        }
    }

    #[test]
    fn a_turned_face_shows_its_radius_after_the_offset() {
        // A shaft: out of the part points away from the axis, so pulling out grows the radius.
        let mut shaft = turned(DVec3::X * 5.0, DVec3::X);
        shaft.distance = 2.0;
        assert!((shaft.shown(true) - 7.0).abs() < 1e-9);
        assert!((shaft.shown(false) - 2.0).abs() < 1e-9);
        shaft.set_shown(true, 4.0);
        assert!((shaft.distance + 1.0).abs() < 1e-9);
        // A hole: out of the part points at the axis, so pulling out shrinks it.
        let mut hole = turned(DVec3::X * 5.0, -DVec3::X);
        hole.distance = 2.0;
        assert!((hole.shown(true) - 3.0).abs() < 1e-9);
        hole.set_shown(true, 6.0);
        assert!((hole.distance + 1.0).abs() < 1e-9);
    }
}

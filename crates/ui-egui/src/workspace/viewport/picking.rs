//! What is under the pointer: the open sketch's points and picked lines, the origin plane tiles, and model picks.

use egui::Pos2;
use glam::{DVec2, DVec3};
use serde_json::json;
use shapecraft_doc::{ItemId, Pick, SketchEntity, SketchPlane};
use shapecraft_render::{PickContext, PlaneTile, pick};

use crate::workspace::Workspace;
use crate::workspace::tools::{AXES, Tool};

impl Workspace {
    /// Two picked lines of the open sketch that meet at a corner, in the order picked (s15-006).
    pub(in crate::workspace) fn picked_corner(&self) -> Option<(ItemId, u32, u32)> {
        let sk = self.session.active_sketch()?;
        let [Pick::SketchEntity { sketch: s1, entity: e1 }, Pick::SketchEntity { sketch: s2, entity: e2 }] = self.session.selection[..]
        else {
            return None;
        };
        if s1 != sk.id || s2 != sk.id || e1 == e2 {
            return None;
        }
        let (Some(SketchEntity::Line { a: p, b: q }), Some(SketchEntity::Line { a: r, b: t })) =
            (sk.entities.get(e1 as usize), sk.entities.get(e2 as usize))
        else {
            return None;
        };
        let (c, _, _) = shapecraft_doc::constraints::corner_rays((*p, *q), (*r, *t));
        let other = if r.distance(c) <= t.distance(c) { *r } else { *t };
        let size = 1.0 + p.length().max(q.length()).max(r.length()).max(t.length());
        (c.distance(other) < 1e-6 * size).then_some((sk.id, e1, e2))
    }

    /// The angle between the two picked lines (degrees), for the pill's chip.
    pub(in crate::workspace) fn picked_angle(&self) -> Option<f64> {
        // Two flat faces: the angle between them ("2 faces | ∠ 90°").
        if let [Pick::Face { body: b1, face: f1 }, Pick::Face { body: b2, face: f2 }] = self.session.selection[..] {
            let normal = |body, face| {
                let b = self.session.doc.body(body)?;
                self.session.kernel.face_plane(&b.shape, face).ok().flatten().map(|p| p.normal.normalize_or_zero())
            };
            let (n1, n2) = (normal(b1, f1)?, normal(b2, f2)?);
            return Some(n1.dot(n2).abs().clamp(0.0, 1.0).acos().to_degrees());
        }
        let (sketch, a, b) = self.picked_corner()?;
        let sk = self.session.doc.sketch(sketch)?;
        let (Some(SketchEntity::Line { a: p, b: q }), Some(SketchEntity::Line { a: r, b: t })) =
            (sk.entities.get(a as usize), sk.entities.get(b as usize))
        else {
            return None;
        };
        let (_, d1, d2) = shapecraft_doc::constraints::corner_rays((*p, *q), (*r, *t));
        Some(shapecraft_doc::constraints::ray_angle(d1, d2).to_degrees())
    }

    /// The open sketch's key point under the cursor (within 8 px), as a pick.
    pub(in crate::workspace) fn sketch_point_at(&self, c: Pos2) -> Option<Pick> {
        self.point_in_sketch(self.session.active_sketch()?, c)
    }

    /// Outside a sketch: a key point (a circle's centre, a line's end) of a shown sketch under the pointer; clicking it
    /// opens that sketch with the point picked (s24-uw-region).
    pub(in crate::workspace) fn shown_sketch_point_at(&self, c: Pos2) -> Option<Pick> {
        if self.session.active_sketch.is_some() {
            return None;
        }
        self.session.doc.sketches.iter().filter(|s| s.visible && !s.entities.is_empty()).find_map(|sk| self.point_in_sketch(sk, c))
    }

    fn point_in_sketch(&self, sk: &shapecraft_doc::Sketch, c: Pos2) -> Option<Pick> {
        let mut best: Option<(f32, Pick)> = None;
        for (entity, e) in sk.entities.iter().enumerate() {
            if matches!(e, SketchEntity::Text { .. }) {
                continue;
            }
            for (index, q) in shapecraft_doc::constraints::points(e).into_iter().enumerate() {
                let Some(sp) = self.to_screen(sk.plane.to_world(q)) else { continue };
                let d = sp.distance(c);
                if d < 8.0 && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, Pick::SketchPoint { sketch: sk.id, entity: entity as u32, index: index as u32 }));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// The picked points of the open sketch (in sketch coordinates), for painting them.
    pub(super) fn picked_points(&self) -> Vec<DVec2> {
        let Some(sk) = self.session.active_sketch() else { return Vec::new() };
        self.session
            .selection
            .iter()
            .filter_map(|p| match *p {
                Pick::SketchPoint { sketch, entity, index } if sketch == sk.id => {
                    sk.entities.get(entity as usize).and_then(|e| shapecraft_doc::constraints::points(e).get(index as usize).copied())
                }
                _ => None,
            })
            .collect()
    }

    /// Mirror's three planes through the selection's center (YZ, XZ, XY).
    /// Mirror and Split Body offer the origin planes, as observed (330, 332): the same small tiles as when
    /// picking a sketch plane, indexed by the axis they're normal to (0 = YZ, 1 = XZ, 2 = XY).
    fn mirror_tiles(&self) -> Vec<(SketchPlane, PlaneTile)> {
        let Tool::Mirror(t) = &self.tool else { return Vec::new() };
        // In a sketch the mirror is a line of it: no plane tiles (s11-092).
        if self.session.active_sketch.is_some() {
            return Vec::new();
        }
        let [xy, xz, yz] = self.origin_tiles();
        // Split Body: a tile picked as a splitting tool turns purple (s12-004).
        let picked = |i: usize| t.with.iter().any(|(w, _)| w["normal"] == json!(AXES[i].to_array()) && w.get("body").is_none());
        [yz, xz, xy]
            .into_iter()
            .enumerate()
            .map(|(i, (plane, corners))| (plane, PlaneTile { corners, hovered: t.hovered == Some(i), picked: picked(i) }))
            .collect()
    }

    /// The origin planes' tiles: XY floor in front, XZ to the left, YZ to the right, about 75 px on screen.
    fn origin_tiles(&self) -> [(SketchPlane, [DVec3; 4]); 3] {
        let s = self.camera.world_per_pixel(self.size().y) * 92.0;
        let g = s * 0.3;
        let tile = |plane: SketchPlane, u0: f64, v0: f64| {
            let c = [
                plane.to_world(DVec2::new(u0, v0)),
                plane.to_world(DVec2::new(u0 + s, v0)),
                plane.to_world(DVec2::new(u0 + s, v0 + s)),
                plane.to_world(DVec2::new(u0, v0 + s)),
            ];
            (plane, c)
        };
        [tile(SketchPlane::XY, g, -g - s), tile(SketchPlane::XZ, -g - s, g), tile(SketchPlane::YZ, g, g)]
    }

    pub(super) fn mirror_tile_under(&self, cursor: Pos2) -> Option<usize> {
        let (o, d) = self.camera.ray(self.local(cursor), self.size());
        let mut best: Option<(f64, usize)> = None;
        for (i, (plane, tile)) in self.mirror_tiles().iter().enumerate() {
            let Some(hit) = plane.intersect_ray(o, d) else { continue };
            let a = plane.to_local(tile.corners[0]);
            let b = plane.to_local(tile.corners[2]);
            let l = plane.to_local(hit);
            let inside = l.x >= a.x.min(b.x) && l.x <= a.x.max(b.x) && l.y >= a.y.min(b.y) && l.y <= a.y.max(b.y);
            let dist = (hit - o).length();
            if inside && best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, i));
            }
        }
        best.map(|(_, i)| i)
    }

    pub(super) fn plane_tiles(&self) -> Vec<(SketchPlane, PlaneTile)> {
        if matches!(self.tool, Tool::Mirror(_)) {
            return self.mirror_tiles();
        }
        let hovered = match (self.tool.clone(), self.section_pick) {
            (_, Some(h)) => h,
            (Tool::PickPlane { hovered }, _) => hovered,
            _ => return Vec::new(),
        };
        self.origin_tiles()
            .into_iter()
            .enumerate()
            .map(|(i, (plane, corners))| (plane, PlaneTile { corners, hovered: hovered == Some(i), picked: false }))
            .collect()
    }

    pub(super) fn tile_under(&self, cursor: Pos2) -> Option<usize> {
        let (o, d) = self.camera.ray(self.local(cursor), self.size());
        let mut best: Option<(f64, usize)> = None;
        for (i, (plane, tile)) in self.plane_tiles().iter().enumerate() {
            let Some(hit) = plane.intersect_ray(o, d) else { continue };
            let a = plane.to_local(tile.corners[0]);
            let b = plane.to_local(tile.corners[2]);
            let l = plane.to_local(hit);
            let inside = l.x >= a.x.min(b.x) && l.x <= a.x.max(b.x) && l.y >= a.y.min(b.y) && l.y <= a.y.max(b.y);
            let dist = (hit - o).length();
            if inside && best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, i));
            }
        }
        best.map(|(_, i)| i)
    }

    pub(in crate::workspace) fn pick_at(&self, cursor: Pos2) -> Option<(Pick, DVec3)> {
        let doc = self.pick_doc();
        let ctx = PickContext {
            doc: &doc,
            active_sketch: self.session.active_sketch,
            camera: &self.camera,
            size: self.size(),
            section: self.section.map(|s| (s.origin(), s.normal)),
            slop: self.pick_slop as f64,
            prefer: self.hover.filter(|_| self.pick_slop > 1.0),
        };
        let hit = pick(&ctx, self.local(cursor))?;
        // Selection filter: a body for anything on it, or the nearest face or edge under the point.
        let wanted = |p: &Pick| match self.pick_filter {
            "faces" => matches!(p, Pick::Face { .. }),
            "edges" => matches!(p, Pick::Edge { .. }),
            _ => true,
        };
        match (self.pick_filter, hit.0) {
            ("bodies", Pick::Face { body, .. } | Pick::Edge { body, .. }) => Some((Pick::Body { body }, hit.1)),
            (_, p) if wanted(&p) => Some(hit),
            (_, Pick::Face { .. } | Pick::Edge { .. }) => {
                shapecraft_render::pick_through(&ctx, self.local(cursor)).into_iter().map(|(p, _)| p).find(wanted).map(|p| (p, hit.1))
            }
            _ => Some(hit),
        }
    }

    /// Everything under a screen point, nearest first (for "Select Through This Point…").
    pub(in crate::workspace) fn picks_through(&self, cursor: Pos2) -> Vec<Pick> {
        let doc = self.pick_doc();
        let ctx = PickContext {
            doc: &doc,
            active_sketch: self.session.active_sketch,
            camera: &self.camera,
            size: self.size(),
            section: self.section.map(|s| (s.origin(), s.normal)),
            slop: self.pick_slop as f64,
            prefer: self.hover.filter(|_| self.pick_slop > 1.0),
        };
        shapecraft_render::pick_through(&ctx, self.local(cursor)).into_iter().map(|(p, _)| p).collect()
    }
}

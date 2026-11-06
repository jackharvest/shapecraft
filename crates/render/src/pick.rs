use glam::{DVec2, DVec3};
use shapecraft_doc::{Body, Document, ItemId, Pick, Sketch};

use crate::Camera;

pub struct PickContext<'a> {
    pub doc: &'a Document,
    pub active_sketch: Option<ItemId>,
    pub camera: &'a Camera,
    pub size: DVec2,
    /// Section View plane (origin, normal toward the removed side): hits there don't count.
    pub section: Option<(DVec3, DVec3)>,
    /// How much wider than a mouse's the reach for edges and curves is: 1 for a mouse, more for a pencil or a finger.
    pub slop: f64,
    /// What is lit up now (the hover). With `slop` > 1 it holds on until the pointer is well away from it, so a
    /// pencil tap picks what it showed rather than its neighbour, as on a tablet app.
    pub prefer: Option<Pick>,
}

impl<'a> PickContext<'a> {
    pub fn new(doc: &'a Document, camera: &'a Camera, size: DVec2) -> Self {
        Self { doc, active_sketch: None, camera, size, section: None, slop: 1.0, prefer: None }
    }

    /// The reach for `pick` (base pixels): wider for the lit-up one, which also wins ties against what's a little nearer.
    fn reach(&self, base: f64, pick: Pick) -> (f64, f64) {
        if self.slop > 1.0 && self.prefer == Some(pick) { (base * self.slop * STICK, 0.45) } else { (base * self.slop, 1.0) }
    }
}

const EDGE_PX: f64 = 6.0;
const SKETCH_PX: f64 = 7.0;
/// How much farther than its reach the lit-up edge or curve holds on (pencil/finger only).
const STICK: f64 = 1.8;

fn v3(p: [f32; 3]) -> DVec3 {
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
}

/// Möller–Trumbore; returns the distance along the ray.
fn ray_triangle(o: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> (f64, f64) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((a + ab * t).distance(p), t)
}

fn sketch_entity_under(ctx: &PickContext, sketch: &Sketch, cursor: DVec2) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    for (i, e) in sketch.entities.iter().enumerate() {
        let (reach, weight) = ctx.reach(SKETCH_PX, Pick::SketchEntity { sketch: sketch.id, entity: i as u32 });
        for outline in e.outlines() {
            let pts: Vec<DVec2> =
                outline.iter().filter_map(|p| ctx.camera.project(sketch.plane.to_world(*p), ctx.size).map(|(s, _)| s)).collect();
            for w in pts.windows(2) {
                let (d, _) = segment_distance(cursor, w[0], w[1]);
                if d < reach && best.is_none_or(|(bd, _)| d * weight < bd) {
                    best = Some((d * weight, i as u32));
                }
            }
        }
    }
    best.map(|(_, i)| i)
}

/// Whether a ray can reach a body at all: picking tests every triangle otherwise, and this runs each frame for what's under
/// the cursor, so a model of many bodies would spend its frame there.
fn ray_meets(bounds: Option<([f32; 3], [f32; 3])>, o: DVec3, d: DVec3) -> bool {
    let Some((lo, hi)) = bounds else { return false };
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for i in 0..3 {
        let (a, b) = (lo[i] as f64 - o[i], hi[i] as f64 - o[i]);
        if d[i] == 0.0 {
            if a > 0.0 || b < 0.0 {
                return false;
            }
        } else {
            let (t1, t2) = (a / d[i], b / d[i]);
            near = near.max(t1.min(t2));
            far = far.min(t1.max(t2));
        }
    }
    near <= far && far >= 0.0
}

/// Whether the cursor can be within `px` of a body's outline on screen. Unsure (part of it behind the viewer) counts as yes.
fn near_on_screen(ctx: &PickContext, body: &Body, cursor: DVec2, px: f64) -> bool {
    let Some((lo, hi)) = body.mesh.bounds() else { return false };
    let (mut min, mut max) = (DVec2::INFINITY, DVec2::NEG_INFINITY);
    for corner in 0..8 {
        let pick = |i: usize| if corner >> i & 1 == 0 { lo[i] } else { hi[i] };
        let Some((p, _)) = ctx.camera.project(v3([pick(0), pick(1), pick(2)]), ctx.size) else { return true };
        min = min.min(p);
        max = max.max(p);
    }
    cursor.x >= min.x - px && cursor.x <= max.x + px && cursor.y >= min.y - px && cursor.y <= max.y + px
}

/// The nearest visible face along a ray: distance, body, face.
fn nearest_face(ctx: &PickContext, o: DVec3, d: DVec3) -> Option<(f64, ItemId, u32)> {
    let mut face_hit: Option<(f64, ItemId, u32)> = None;
    for body in ctx.doc.bodies.iter().filter(|b| b.visible && ray_meets(b.mesh.bounds(), o, d)) {
        let m = &body.mesh;
        for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
            let (Some(a), Some(b), Some(c)) =
                (m.positions.get(tri[0] as usize), m.positions.get(tri[1] as usize), m.positions.get(tri[2] as usize))
            else {
                continue;
            };
            if let Some(dist) = ray_triangle(o, d, v3(*a), v3(*b), v3(*c))
                && face_hit.is_none_or(|(bd, _, _)| dist < bd)
                && !cut(ctx, o + d * dist)
            {
                face_hit = Some((dist, body.id, m.triangle_faces.get(t).copied().unwrap_or(0)));
            }
        }
    }
    face_hit
}

fn cut(ctx: &PickContext, p: DVec3) -> bool {
    ctx.section.is_some_and(|(o, n)| (p - o).dot(n) > 0.0)
}

/// What's under the cursor, and the world point there.
pub fn pick(ctx: &PickContext, cursor: DVec2) -> Option<(Pick, DVec3)> {
    let (o, d) = ctx.camera.ray(cursor, ctx.size);

    // In sketch mode the sketch wins: its elements, then its regions.
    if let Some(sketch) = ctx.active_sketch.and_then(|id| ctx.doc.sketch(id)) {
        let hit = sketch.plane.intersect_ray(o, d);
        if let Some(i) = sketch_entity_under(ctx, sketch, cursor) {
            return Some((Pick::SketchEntity { sketch: sketch.id, entity: i }, hit.unwrap_or(o)));
        }
        if let Some(hit) = hit {
            let local = sketch.plane.to_local(hit);
            // Regions don't overlap (the kernel splits them), so at most one contains the point.
            if let Some(r) = sketch.regions().iter().find(|r| r.contains(local)) {
                return Some((Pick::Region { sketch: sketch.id, region: r.index }, hit));
            }
        }
    }

    // Construction axes, by screen distance like edges.
    for ax in ctx.doc.axes.iter().filter(|a| a.visible) {
        let (a, b) = ax.ends();
        if let (Some((sa, _)), Some((sb, _))) = (ctx.camera.project(a, ctx.size), ctx.camera.project(b, ctx.size)) {
            let (d, t) = segment_distance(cursor, sa, sb);
            if d < ctx.reach(SKETCH_PX, Pick::Axis { axis: ax.id }).0 {
                return Some((Pick::Axis { axis: ax.id }, a.lerp(b, t.clamp(0.0, 1.0))));
            }
        }
    }

    // Curves of other visible sketches stay pickable (a sweep path, a revolve axis), like edges are.
    for sketch in ctx.doc.sketches.iter().filter(|s| s.visible && Some(s.id) != ctx.active_sketch) {
        if let Some(i) = sketch_entity_under(ctx, sketch, cursor) {
            let hit = sketch.plane.intersect_ray(o, d).unwrap_or(o);
            return Some((Pick::SketchEntity { sketch: sketch.id, entity: i }, hit));
        }
    }

    let face_hit = nearest_face(ctx, o, d);

    // Nearest visible edge within a few pixels.
    let mut edge_hit: Option<(f64, ItemId, u32, DVec3)> = None;
    let widest = EDGE_PX * ctx.slop * if ctx.slop > 1.0 { STICK } else { 1.0 };
    for body in ctx.doc.bodies.iter().filter(|b| b.visible && near_on_screen(ctx, b, cursor, widest)) {
        for (k, (line, &edge)) in body.mesh.edges.iter().zip(&body.mesh.edge_ids).enumerate() {
            let own = body.mesh.edge_faces.get(k).copied().unwrap_or([u32::MAX; 2]);
            let (reach, weight) = ctx.reach(EDGE_PX, Pick::Edge { body: body.id, edge });
            for w in line.windows(2) {
                let (a3, b3) = (v3(w[0]), v3(w[1]));
                let (Some((a, _)), Some((b, _))) = (ctx.camera.project(a3, ctx.size), ctx.camera.project(b3, ctx.size)) else {
                    continue;
                };
                let (px, t) = segment_distance(cursor, a, b);
                if px >= reach || edge_hit.is_some_and(|(bp, ..)| px * weight >= bp) {
                    continue;
                }
                let world = a3.lerp(b3, t);
                if cut(ctx, world) {
                    continue;
                }
                // Hidden or not is judged where the edge itself is on screen: next to a slanted face the cursor's
                // own face point can be well in front of the edge it bounds.
                let at = a + (b - a) * t;
                let (eo, ed) = ctx.camera.ray(at, ctx.size);
                let depth = (world - eo).dot(ed);
                // A face seen at a grazing angle can be hit a little in front of its own edge: the edge still shows
                // when what's in front is one of the two faces it bounds, close by. A round wall wraps around, so its
                // front half is "one of its faces" too for the far half of its rim: that stays hidden (s36).
                let near = ctx.camera.world_per_pixel(ctx.size.y) * 12.0;
                let visible = nearest_face(ctx, eo, ed).is_none_or(|(fd, fb, ff)| {
                    depth <= fd + ctx.camera.distance * 0.002 || fb == body.id && own.contains(&ff) && depth - fd <= near
                });
                if visible {
                    edge_hit = Some((px * weight, body.id, edge, world));
                }
            }
        }
    }
    if let Some((_, body, edge, world)) = edge_hit {
        return Some((Pick::Edge { body, edge }, world));
    }

    // Regions of other visible sketches, if nearer than any face.
    let mut region_hit: Option<(f64, Pick, DVec3)> = None;
    for sketch in ctx.doc.sketches.iter().filter(|s| s.visible && Some(s.id) != ctx.active_sketch) {
        let Some(hit) = sketch.plane.intersect_ray(o, d) else { continue };
        let dist = (hit - o).length();
        let local = sketch.plane.to_local(hit);
        if let Some(r) = sketch.regions().iter().find(|r| r.contains(local))
            && region_hit.as_ref().is_none_or(|(bd, ..)| dist < *bd)
        {
            region_hit = Some((dist, Pick::Region { sketch: sketch.id, region: r.index }, hit));
        }
    }
    // Construction planes: their square, if nearer than any face or region.
    for pl in ctx.doc.planes.iter().filter(|p| p.visible) {
        let Some(hit) = pl.plane.intersect_ray(o, d) else { continue };
        let local = pl.plane.to_local(hit);
        let dist = (hit - o).length();
        if local.abs().max_element() <= pl.size / 2.0 && region_hit.as_ref().is_none_or(|(bd, ..)| dist < *bd) {
            region_hit = Some((dist, Pick::Plane { plane: pl.id }, hit));
        }
    }
    // Reference pictures, like planes: inside their rectangle, if nearer than any face or region.
    for im in ctx.doc.images.iter().filter(|i| i.visible) {
        let Some(hit) = im.plane.intersect_ray(o, d) else { continue };
        let local = im.plane.to_local(hit);
        let dist = (hit - o).length();
        if local.x.abs() <= im.size.x / 2.0 && local.y.abs() <= im.size.y / 2.0 && region_hit.as_ref().is_none_or(|(bd, ..)| dist < *bd) {
            region_hit = Some((dist, Pick::Image { image: im.id }, hit));
        }
    }
    match (face_hit, region_hit) {
        (Some((fd, body, face)), Some((rd, pick, hit))) => {
            if rd <= fd + ctx.camera.distance * 0.001 {
                Some((pick, hit))
            } else {
                Some((Pick::Face { body, face }, o + d * fd))
            }
        }
        (Some((fd, body, face)), None) => Some((Pick::Face { body, face }, o + d * fd)),
        (None, Some((_, pick, hit))) => Some((pick, hit)),
        (None, None) => None,
    }
}

/// Everything along the ray under the cursor, nearest first: each face it passes through, edges within a few
/// pixels, and sketch regions and construction planes it crosses ("Select Through This Point…").
pub fn pick_through(ctx: &PickContext, cursor: DVec2) -> Vec<(Pick, f64)> {
    let (o, d) = ctx.camera.ray(cursor, ctx.size);
    let mut hits: Vec<(Pick, f64)> = Vec::new();
    let add = |pick: Pick, dist: f64, hits: &mut Vec<(Pick, f64)>| match hits.iter_mut().find(|(p, _)| *p == pick) {
        Some(h) => h.1 = h.1.min(dist),
        None => hits.push((pick, dist)),
    };
    for body in ctx.doc.bodies.iter().filter(|b| b.visible) {
        let m = &body.mesh;
        for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
            let (Some(a), Some(b), Some(c)) =
                (m.positions.get(tri[0] as usize), m.positions.get(tri[1] as usize), m.positions.get(tri[2] as usize))
            else {
                continue;
            };
            if let Some(dist) = ray_triangle(o, d, v3(*a), v3(*b), v3(*c))
                && !cut(ctx, o + d * dist)
            {
                add(Pick::Face { body: body.id, face: m.triangle_faces.get(t).copied().unwrap_or(0) }, dist, &mut hits);
            }
        }
        for (line, &edge) in body.mesh.edges.iter().zip(&body.mesh.edge_ids) {
            for w in line.windows(2) {
                let (a3, b3) = (v3(w[0]), v3(w[1]));
                let (Some((a, _)), Some((b, _))) = (ctx.camera.project(a3, ctx.size), ctx.camera.project(b3, ctx.size)) else { continue };
                let (px, t) = segment_distance(cursor, a, b);
                let world = a3.lerp(b3, t);
                if px < EDGE_PX * ctx.slop && !cut(ctx, world) {
                    add(Pick::Edge { body: body.id, edge }, (world - o).dot(d), &mut hits);
                }
            }
        }
    }
    for sketch in ctx.doc.sketches.iter().filter(|s| s.visible || Some(s.id) == ctx.active_sketch) {
        let Some(hit) = sketch.plane.intersect_ray(o, d) else { continue };
        let local = sketch.plane.to_local(hit);
        if let Some(r) = sketch.regions().iter().find(|r| r.contains(local)) {
            add(Pick::Region { sketch: sketch.id, region: r.index }, (hit - o).length(), &mut hits);
        }
    }
    for pl in ctx.doc.planes.iter().filter(|p| p.visible) {
        let Some(hit) = pl.plane.intersect_ray(o, d) else { continue };
        if pl.plane.to_local(hit).abs().max_element() <= pl.size / 2.0 {
            add(Pick::Plane { plane: pl.id }, (hit - o).length(), &mut hits);
        }
    }
    for im in ctx.doc.images.iter().filter(|i| i.visible) {
        let Some(hit) = im.plane.intersect_ray(o, d) else { continue };
        let local = im.plane.to_local(hit);
        if local.x.abs() <= im.size.x / 2.0 && local.y.abs() <= im.size.y / 2.0 {
            add(Pick::Image { image: im.id }, (hit - o).length(), &mut hits);
        }
    }
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_meets_a_box_only_where_it_is() {
        let unit = Some(([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
        let down = DVec3::NEG_Z;
        assert!(ray_meets(unit, DVec3::new(0.5, 0.5, 5.0), down), "straight through");
        assert!(ray_meets(unit, DVec3::new(0.5, 0.5, 0.5), down), "from inside");
        assert!(ray_meets(unit, DVec3::new(1.0, 1.0, 5.0), down), "along a corner");
        assert!(!ray_meets(unit, DVec3::new(1.5, 0.5, 5.0), down), "beside it");
        assert!(!ray_meets(unit, DVec3::new(0.5, 0.5, -5.0), down), "behind the ray");
        assert!(ray_meets(unit, DVec3::new(-3.0, -3.0, -3.0), DVec3::ONE.normalize()), "diagonally");
        assert!(ray_meets(Some(([0.0, 0.0, 0.0], [1.0, 1.0, 0.0])), DVec3::new(0.5, 0.5, 5.0), down), "flat, as a plate's face");
        assert!(!ray_meets(None, DVec3::ZERO, down), "an empty mesh");
    }

    struct NoShape;
    impl shapecraft_kernel::ShapeData for NoShape {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// A closed cylinder (radius 5, height 10) as a mesh: face 0 the wall, 1 the bottom, 2 the top; edge 0 the
    /// bottom rim, edge 1 the top rim.
    fn cylinder() -> Document {
        let n = 48;
        let ring = |z: f32| -> Vec<[f32; 3]> {
            (0..=n)
                .map(|k| {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    [5.0 * a.cos(), 5.0 * a.sin(), z]
                })
                .collect()
        };
        let (lo, hi) = (ring(0.0), ring(10.0));
        let mut m = shapecraft_kernel::Mesh::default();
        let mut tri = |m: &mut shapecraft_kernel::Mesh, a: [f32; 3], b: [f32; 3], c: [f32; 3], face: u32| {
            let i = m.positions.len() as u32;
            m.positions.extend([a, b, c]);
            m.normals.extend([[0.0, 0.0, 1.0]; 3]);
            m.indices.extend([i, i + 1, i + 2]);
            m.triangle_faces.push(face);
        };
        for k in 0..n {
            tri(&mut m, lo[k], lo[k + 1], hi[k + 1], 0);
            tri(&mut m, lo[k], hi[k + 1], hi[k], 0);
            tri(&mut m, [0.0, 0.0, 0.0], lo[k + 1], lo[k], 1);
            tri(&mut m, [0.0, 0.0, 10.0], hi[k], hi[k + 1], 2);
        }
        m.edges = vec![lo, hi];
        m.edge_ids = vec![0, 1];
        m.edge_faces = vec![[0, 1], [0, 2]];
        let mut doc = Document::default();
        doc.bodies.push(shapecraft_doc::Body {
            id: ItemId(1),
            name: "Body 01".into(),
            shape: shapecraft_kernel::Shape::new(NoShape),
            mesh: std::sync::Arc::new(m),
            visible: true,
            appearance: None,
            face_colors: Vec::new(),
        });
        doc
    }

    /// The far half of a cylinder's bottom rim is behind its own wall: a click there takes the wall (s36), while the
    /// near half of the rim still picks.
    #[test]
    fn a_rim_behind_its_own_round_wall_is_hidden() {
        let doc = cylinder();
        let camera = crate::Camera::standard(crate::StandardView::Default, DVec3::new(0.0, 0.0, 5.0), 60.0);
        let ctx = PickContext::new(&doc, &camera, DVec2::new(1200.0, 900.0));
        // Where the far side of the bottom rim (the point away from the camera) lands on screen.
        let back = camera.eye().with_z(0.0).normalize() * -5.0;
        let (far, _) = camera.project(back, ctx.size).unwrap();
        let hit = pick(&ctx, far).map(|(p, _)| p);
        assert_eq!(hit, Some(Pick::Face { body: ItemId(1), face: 0 }), "the wall, not the hidden rim");
        let front = camera.eye().with_z(0.0).normalize() * 5.0;
        let (near, _) = camera.project(front, ctx.size).unwrap();
        assert_eq!(pick(&ctx, near).map(|(p, _)| p), Some(Pick::Edge { body: ItemId(1), edge: 0 }), "the near rim still picks");
    }

    /// A pencil reaches an edge from twice a mouse's distance, and the lit-up edge holds on farther still, so a tap
    /// picks what the hover showed.
    #[test]
    fn a_pencil_reaches_farther_and_the_lit_edge_holds_on() {
        let doc = cylinder();
        let camera = crate::Camera::standard(crate::StandardView::Default, DVec3::new(0.0, 0.0, 5.0), 60.0);
        let mut ctx = PickContext::new(&doc, &camera, DVec2::new(1200.0, 900.0));
        let front = camera.eye().with_z(0.0).normalize() * 5.0;
        let (rim, _) = camera.project(front, ctx.size).unwrap();
        let edge = Some(Pick::Edge { body: ItemId(1), edge: 0 });
        // Straight up from the rim, onto the wall.
        let at = |ctx: &PickContext, px: f64| pick(ctx, rim - DVec2::new(0.0, px)).map(|(p, _)| p);
        assert_ne!(at(&ctx, 9.0), edge, "a mouse 9 px off is on the wall");
        ctx.slop = 2.0;
        assert_eq!(at(&ctx, 9.0), edge, "a pencil 9 px off still gets the rim");
        assert_ne!(at(&ctx, 16.0), edge, "16 px is past a pencil's reach");
        ctx.prefer = edge;
        assert_eq!(at(&ctx, 16.0), edge, "but the lit-up rim holds on that far");
        assert_ne!(at(&ctx, 30.0), edge, "and lets go farther out");
    }
}

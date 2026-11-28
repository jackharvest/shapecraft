//! What the renderer draws besides the model: tool and selection overlays, and the bodies a tool tints.

use std::sync::Arc;

use crate::theme;
use glam::{DVec2, DVec3};
use shapecraft_doc::{Pick, SketchEntity};
use shapecraft_render::{Overlay, Rgba, SceneColors};

use super::snap::{ellipse_minor, slot_end, slot_ends, slot_radius};
use crate::workspace::Workspace;
use crate::workspace::tools::Tool;

/// Undo / Redo's flash: how long, how far toward its colour at the brightest, and the colour.
pub(in crate::workspace) const UNDO_FLASH_S: f64 = 0.7;
const UNDO_FLASH_PEAK: f32 = 0.55;
const UNDO_FLASH_COLOR: Rgba = Rgba(0x9c, 0xc4, 0xff, 255);

impl Workspace {
    pub(super) fn overlays(&self) -> Vec<Overlay> {
        let mut out = Vec::new();
        // Profile ▸ Edit...: the sketch's regions, the picked ones lit.
        if let Some((_, sketch, regions)) = &self.profile_edit
            && let Some(sk) = self.session.doc.sketch(*sketch)
        {
            for r in sk.regions() {
                let picked = regions.contains(&r.index);
                let tris =
                    r.triangles.iter().map(|t| [sk.plane.to_world(t[0]), sk.plane.to_world(t[1]), sk.plane.to_world(t[2])]).collect();
                out.push(Overlay::Fill {
                    tris,
                    color: if picked { Rgba::hex(0xc37b14).with_alpha(150) } else { Rgba::hex(0x0034ff).with_alpha(40) },
                });
            }
        }
        // Align: the face picked to align fills purple (386).
        if let Tool::Align { from: Some((body, face)), to: None, edge: false, .. } = &self.tool
            && let Some(b) = self.session.doc.body(*body)
        {
            let m = &b.mesh;
            let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            let tris: Vec<[DVec3; 3]> = m
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .enumerate()
                .filter(|(t, _)| m.triangle_faces.get(*t) == Some(face))
                .filter_map(|(_, tri)| Some([v(tri[0])?, v(tri[1])?, v(tri[2])?]))
                .collect();
            out.push(Overlay::Fill { tris, color: Rgba::hex(0x8a7cf0).with_alpha(235) });
        }
        // Loft's, Sweep's and Fill's picks show through the solid they make: regions orange, faces blue.
        let (profiles, curves): (Vec<Pick>, Vec<Pick>) = match &self.tool {
            Tool::Loft(t) => (t.profiles.clone(), t.guides.clone()),
            Tool::Sweep(t) => (vec![t.profile], t.path.clone()),
            Tool::Fill(t) => (t.profiles.clone(), t.guides.clone()),
            _ => (Vec::new(), Vec::new()),
        };
        for p in &profiles {
            match *p {
                Pick::Region { sketch, region } => {
                    let Some(sk) = self.session.doc.sketch(sketch) else { continue };
                    let Some(r) = sk.regions().iter().find(|r| r.index == region) else { continue };
                    let tris =
                        r.triangles.iter().map(|t| [sk.plane.to_world(t[0]), sk.plane.to_world(t[1]), sk.plane.to_world(t[2])]).collect();
                    out.push(Overlay::Fill { tris, color: Rgba::hex(0xc98a2b).with_alpha(150) });
                }
                Pick::Face { body, face } => {
                    let Some(b) = self.session.doc.body(body) else { continue };
                    let m = &b.mesh;
                    let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                    let tris = m
                        .indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .enumerate()
                        .filter(|(t, _)| m.triangle_faces.get(*t) == Some(&face))
                        .filter_map(|(_, tri)| Some([v(tri[0])?, v(tri[1])?, v(tri[2])?]))
                        .collect();
                    out.push(Overlay::Fill { tris, color: Rgba::hex(0x24a6cf).with_alpha(150) });
                }
                _ => {}
            }
        }
        // Loft's and Fill's curves are orange; a Sweep's path takes the selected-edge blue.
        let orange = if matches!(self.tool, Tool::Sweep(_)) { Rgba::hex(0x0dabd3) } else { Rgba::hex(0xf2a341) };
        for p in &curves {
            match *p {
                Pick::SketchEntity { sketch, entity } => {
                    let Some(sk) = self.session.doc.sketch(sketch) else { continue };
                    let Some(e) = sk.entities.get(entity as usize) else { continue };
                    for line in e.outlines() {
                        for w in line.windows(2) {
                            out.push(Overlay::Line { a: sk.plane.to_world(w[0]), b: sk.plane.to_world(w[1]), color: orange, width: 2.2 });
                        }
                    }
                }
                Pick::Edge { body, edge } => {
                    let Some(b) = self.session.doc.body(body) else { continue };
                    let Some(k) = b.mesh.edge_ids.iter().position(|e| *e == edge) else { continue };
                    for w in b.mesh.edges.get(k).map(Vec::as_slice).unwrap_or_default().windows(2) {
                        let at = |q: [f32; 3]| DVec3::from(q.map(f64::from));
                        out.push(Overlay::Line { a: at(w[0]), b: at(w[1]), color: orange, width: 2.2 });
                    }
                }
                _ => {}
            }
        }
        // Project as Edge's items are teal throughout (s11-134/135); regions are meshed here, they're small.
        if let Tool::Onto(t) = &self.tool
            && t.project
        {
            let teal = Rgba::hex(0x3ba3a6).with_alpha(240);
            let changed = t.to.as_ref().and_then(|(to, _)| to["body"].as_u64());
            for (p, _) in &t.items {
                let mesh = match *p {
                    Pick::Region { sketch, region } => self
                        .session
                        .doc
                        .sketch(sketch)
                        .and_then(|s| s.region(region))
                        .and_then(|r| self.session.kernel.tessellate(&r.face, 0.05).ok())
                        .map(|m| (Arc::new(m), None)),
                    // A face of the body being imprinted renumbers once applied.
                    Pick::Face { body, .. } if self.is_live() && changed == Some(body.0) => None,
                    Pick::Face { body, face } => self.session.doc.body(body).map(|b| (b.mesh.clone(), Some(face))),
                    _ => None,
                };
                let Some((m, only)) = mesh else { continue };
                let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                let tris: Vec<[DVec3; 3]> = m
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(t, _)| only.is_none_or(|f| m.triangle_faces.get(*t) == Some(&f)))
                    .filter_map(|(_, tri)| Some([v(tri[0])?, v(tri[1])?, v(tri[2])?]))
                    .collect();
                out.push(Overlay::Fill { tris, color: teal });
            }
        }
        // Replace Face applied (s11-130): the replacing face lavender, the faces now on it pale teal. Project as Edge
        // (s11-135): the face projected onto purple, split pieces and all.
        if let Tool::Onto(t) = &self.tool
            && self.is_live()
            && let Some((to, _)) = &t.to
            && let Some(body) = if t.project { shapecraft_engine::Params::new(to).id("body").ok() } else { t.body() }
        {
            let face_tris = |b: &shapecraft_doc::Body, keep: &dyn Fn(u32) -> bool| -> Vec<[DVec3; 3]> {
                let m = &b.mesh;
                let v = |i: u32| m.positions.get(i as usize).map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                m.indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(t, _)| m.triangle_faces.get(*t).is_some_and(|f| keep(*f)))
                    .filter_map(|(_, tri)| Some([v(tri[0])?, v(tri[1])?, v(tri[2])?]))
                    .collect()
            };
            let p = shapecraft_engine::Params::new(to);
            let target = if let Ok(pl) = p.id("plane") {
                self.session.doc.plane(pl).map(|pl| (pl.plane.origin, pl.plane.normal()))
            } else {
                p.id("body").ok().and_then(|tb| {
                    let b = self.session.doc.body(tb)?;
                    let f = p.f64("face").ok()? as u32;
                    let fp = self.session.kernel.face_plane(&b.shape, f).ok().flatten()?;
                    if tb != body && !t.project {
                        out.push(Overlay::Fill { tris: face_tris(b, &|x| x == f), color: Rgba::hex(0x89a4c1).with_alpha(235) });
                    }
                    Some((fp.origin, fp.normal))
                })
            };
            if let (Some((o, n)), Some(b)) = (target, self.session.doc.body(body)) {
                let k = &self.session.kernel;
                let on = |f: u32| {
                    k.face_plane(&b.shape, f)
                        .ok()
                        .flatten()
                        .is_some_and(|fp| fp.normal.dot(n).abs() > 1.0 - 1e-6 && (fp.origin - o).dot(n).abs() < 1e-6)
                };
                let faces: Vec<u32> = (0..k.face_count(&b.shape).unwrap_or(0)).filter(|f| on(*f)).collect();
                let color = if t.project { Rgba::hex(0x7b70dd) } else { Rgba::hex(0x6ca9bb) };
                out.push(Overlay::Fill { tris: face_tris(b, &|x| faces.contains(&x)), color: color.with_alpha(235) });
            }
        }
        let preview = Rgba::hex(0xe8a23a);
        let plane = self.sketch_plane();
        let snap = self.sketch_snap();
        let snapped = snap.as_ref().map(|s| (s.p, s.point));
        match (&self.tool, plane, snapped) {
            (Tool::Rectangle { first: Some(f) }, Some(pl), Some((c, _))) => {
                let c = self.rect_cursor(*f, c);
                let a = &self.rect_corner(*f, c);
                let e = SketchEntity::rectangle(*a, c);
                for w in e.outline().windows(2) {
                    out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                }
                let (min, max) = (a.min(c), a.max(c));
                let blue = Rgba::hex(0x2e68ff);
                for p in [min, max, DVec2::new(min.x, max.y), DVec2::new(max.x, min.y), (min + max) / 2.0] {
                    out.push(Overlay::Dot { p: pl.to_world(p), color: blue, size: 5.0 });
                }
            }
            (Tool::Circle { center: Some(ctr) }, Some(pl), Some((c, _))) => {
                let r = ctr.distance(c);
                if r > 0.0 {
                    let e = SketchEntity::Circle { center: *ctr, radius: r };
                    for w in e.outline().windows(2) {
                        out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                    }
                    // 26.200: the center shows as a dot; the diameter is drawn with the dimension (s11-060).
                    out.push(Overlay::Dot { p: pl.to_world(*ctr), color: Rgba::hex(0x2e68ff), size: 5.0 });
                }
            }
            (Tool::Line { last: Some(l), .. }, Some(pl), Some((c, _))) => {
                out.push(Overlay::Line { a: pl.to_world(*l), b: pl.to_world(self.line_end(*l, c)), color: preview, width: 2.0 });
            }
            (Tool::Arc { a: Some(a), b }, Some(pl), Some((c, _))) => {
                for p in std::iter::once(*a).chain(*b) {
                    out.push(Overlay::Dot { p: pl.to_world(p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                }
                let pts = match self.arc_preview(*a, *b, c) {
                    Some((p, q, r)) => shapecraft_doc::arc_points(p, q, r, 64),
                    None => vec![*a, c],
                };
                for w in pts.windows(2) {
                    out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                }
            }
            (Tool::Ellipse { center: Some(ctr), major }, Some(pl), Some((c, _))) => {
                // As observed: first just the axis line from the center to the cursor, then the whole ellipse
                // with the cursor setting the other axis.
                let (m, ry) = match major {
                    Some(m) => (*m, ellipse_minor(*ctr, *m, c)),
                    None => (c, 0.0),
                };
                let rx = ctr.distance(m);
                if major.is_none() {
                    out.push(Overlay::Line { a: pl.to_world(*ctr), b: pl.to_world(c), color: preview, width: 2.0 });
                    for p in [*ctr, c] {
                        out.push(Overlay::Dot { p: pl.to_world(p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                    }
                } else if rx > 1e-9 && ry > 1e-9 {
                    let angle = (m - *ctr).to_angle();
                    let e = SketchEntity::Ellipse { center: *ctr, rx, ry, angle };
                    for w in e.outline().windows(2) {
                        out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                    }
                    for p in [*ctr, m] {
                        out.push(Overlay::Dot { p: pl.to_world(p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                    }
                }
            }
            (Tool::Spline { points, control }, Some(pl), Some((c, _))) if !points.is_empty() => {
                let mut pts = points.clone();
                pts.push(c);
                let curve = if *control { shapecraft_doc::bspline_points(&pts, 16) } else { shapecraft_doc::spline_points(&pts, 16) };
                if *control {
                    // The control polygon, dashed purple, as observed.
                    let dash = self.camera.world_per_pixel(self.size().y) * 5.0;
                    for w in pts.windows(2) {
                        let (len, dir) = (w[0].distance(w[1]), (w[1] - w[0]).normalize_or_zero());
                        let mut t = 0.0;
                        while t < len {
                            let (a, b) = (w[0] + dir * t, w[0] + dir * (t + dash).min(len));
                            out.push(Overlay::Line { a: pl.to_world(a), b: pl.to_world(b), color: Rgba::hex(0x8b5cf6), width: 1.0 });
                            t += dash * 1.8;
                        }
                    }
                }
                for w in curve.windows(2) {
                    out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                }
                for p in points {
                    out.push(Overlay::Dot { p: pl.to_world(*p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                }
            }
            (Tool::Rect3 { a: Some(a), b }, Some(pl), Some((c, _))) => {
                let pts: Vec<DVec2> = match b {
                    None => vec![*a, c],
                    Some(b) => {
                        let n = (*b - *a).perp().normalize_or_zero();
                        let w = (c - *b).dot(n);
                        vec![*a, *b, *b + n * w, *a + n * w, *a]
                    }
                };
                for w in pts.windows(2) {
                    out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                }
                for p in &pts {
                    out.push(Overlay::Dot { p: pl.to_world(*p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                }
            }
            (Tool::Slot { middle, a: Some(a), b, length, radius }, Some(pl), Some((c, _))) => {
                let blue = Rgba::hex(0x2e68ff);
                let mut line = |p: DVec2, q: DVec2| {
                    out.push(Overlay::Line { a: pl.to_world(p), b: pl.to_world(q), color: preview, width: 2.0 });
                };
                let (p, q) = slot_ends(*middle, *a, b.unwrap_or_else(|| slot_end(*middle, *a, c, *length)));
                let mut dots = vec![p, q];
                match b {
                    // First just the center line, solid; then the slot round it, the line dash-dotted.
                    None => line(p, q),
                    Some(b) => {
                        let (d, r) = ((q - p).normalize_or_zero(), radius.unwrap_or_else(|| slot_radius(*a, *b, c)));
                        let n = d.perp() * r;
                        for arc in [(q + n, q + d * r, q - n), (p - n, p - d * r, p + n)] {
                            for w in shapecraft_doc::arc_points(arc.0, arc.1, arc.2, 32).windows(2) {
                                line(w[0], w[1]);
                            }
                        }
                        line(p + n, q + n);
                        line(q - n, p - n);
                        dots.extend([p + n, q + n, q - n, p - n]);
                        let len = p.distance(q);
                        let dash = self.camera.world_per_pixel(self.size().y) * 14.0;
                        let (mut at, mut k) = (0.0, 0);
                        while at < len && k < 400 {
                            let step = if k % 2 == 0 { dash } else { dash * 0.15 };
                            line(p + d * at, p + d * (at + step).min(len));
                            at += step + dash * 0.45;
                            k += 1;
                        }
                    }
                }
                for p in dots {
                    out.push(Overlay::Dot { p: pl.to_world(p), color: blue, size: 5.0 });
                }
            }
            (Tool::Polygon { center: Some(ctr) }, Some(pl), Some((c, _))) => {
                let r = ctr.distance(c);
                if r > 0.0 {
                    let e = SketchEntity::Polygon { center: *ctr, radius: r, sides: self.polygon_sides(), angle: (c - *ctr).to_angle() };
                    for w in e.outline().windows(2) {
                        out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                    }
                    // Corners and the middle show as dots while it's drawn (s11-065).
                    for p in SketchEntity::polygon_points(*ctr, r, self.polygon_sides(), (c - *ctr).to_angle()).into_iter().chain([*ctr]) {
                        out.push(Overlay::Dot { p: pl.to_world(p), color: Rgba::hex(0x2e68ff), size: 5.0 });
                    }
                }
            }
            _ => {}
        }
        if let (Some(pl), Some((c, on_point)), true) = (plane, snapped, self.tool.is_sketch_tool()) {
            if on_point {
                // A purple disc with a dark gap round a bright purple center (s11-030).
                out.push(Overlay::Dot { p: pl.to_world(c), color: Rgba::hex(0x7d4caf), size: 18.0 });
                out.push(Overlay::Dot { p: pl.to_world(c), color: Rgba::hex(0x1a1a1a), size: 9.0 });
                out.push(Overlay::Dot { p: pl.to_world(c), color: Rgba::hex(0xb266ff), size: 5.0 });
            } else {
                out.push(Overlay::Dot { p: pl.to_world(c), color: Rgba::hex(0x2e68ff), size: 6.0 });
            }
            let purple = Rgba::hex(0x8b5cf6).with_alpha(210);
            for (from, to) in snap.as_ref().map(|s| s.guides.as_slice()).unwrap_or(&[]) {
                // Run the guide a little past both ends, like the reference's long inference lines.
                let d = (*to - *from).normalize_or_zero() * self.camera.world_per_pixel(self.size().y) * 60.0;
                out.push(Overlay::Line { a: pl.to_world(*from - d), b: pl.to_world(*to + d), color: purple, width: 1.0 });
            }
        }
        if let Some(pl) = plane {
            for &(p, _, corner) in self.body_points_on(&pl).iter() {
                out.push(Overlay::Dot { p: pl.to_world(p), color: Rgba::hex(0x8b5cf6), size: if corner { 6.0 } else { 4.0 } });
            }
        }
        if let Tool::Plane3 { points } | Tool::Axis2 { points } = &self.tool {
            for p in points {
                out.push(Overlay::Dot { p: *p, color: Rgba::hex(0x2e68ff), size: 9.0 });
            }
        }
        // Text: the letters as they're typed.
        // Offset Edge on a region: the region outlined in orange instead of filled (s11-075).
        if let Tool::OffsetEdge(t) = &self.tool
            && let (Some(r), Some(sk)) = (t.region, self.session.doc.sketch(t.sketch))
            && let Some(region) = sk.regions().iter().find(|g| g.index == r)
        {
            let key = |p: DVec2| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
            // Each triangle side under its rounded end points; a side used once is on the outline.
            type Key = (i64, i64);
            let mut edges: std::collections::HashMap<(Key, Key), (DVec2, DVec2, u32)> = std::collections::HashMap::new();
            for t in &region.triangles {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let k = if key(a) < key(b) { (key(a), key(b)) } else { (key(b), key(a)) };
                    edges.entry(k).or_insert((a, b, 0)).2 += 1;
                }
            }
            for (a, b, n) in edges.into_values() {
                if n == 1 {
                    out.push(Overlay::Line { a: sk.plane.to_world(a), b: sk.plane.to_world(b), color: Rgba::hex(0xf5a540), width: 1.5 });
                }
            }
        }
        // Offset Edge on a body face: the face's edges in orange (s24-008).
        if let Tool::OffsetEdge(t) = &self.tool
            && let Some((body, face)) = t.face
            && let Some(b) = self.session.doc.body(body)
        {
            for (k, line) in b.mesh.edges.iter().enumerate() {
                if b.mesh.edge_faces.get(k).is_some_and(|f| f.contains(&face)) {
                    for w in line.windows(2) {
                        out.push(Overlay::Line {
                            a: DVec3::from(w[0].map(f64::from)),
                            b: DVec3::from(w[1].map(f64::from)),
                            color: Rgba::hex(0xf5a540),
                            width: 1.5,
                        });
                    }
                }
            }
        }
        // Text: the block in orange where it will go (behind the dialog at first, at the sketch's origin).
        if let (Tool::Text { at, text, size, align, angle }, Some(pl)) = (&self.tool, plane) {
            let font = self.text_font.as_deref();
            for (origin, line) in crate::workspace::tools::text_lines(text, *size, *align, at.unwrap_or(DVec2::ZERO), *angle, font) {
                for o in shapecraft_doc::text::outlines_in(font, &line, origin, *size, *angle) {
                    for w in o.windows(2) {
                        out.push(Overlay::Line { a: pl.to_world(w[0]), b: pl.to_world(w[1]), color: preview, width: 2.0 });
                    }
                }
            }
        }
        // Sketch Mirror: the hovered line light blue and the picked curves mirrored across it (s11-092).
        if let (Tool::Mirror(_), Some(Pick::SketchEntity { sketch, entity }), Some(sk)) =
            (&self.tool, self.hover, self.session.active_sketch())
            && sketch == sk.id
            && let Some(SketchEntity::Line { a, b }) = sk.entities.get(entity as usize)
            && let Some(u) = (*b - *a).try_normalize()
        {
            let flip = |p: DVec2| {
                let d = p - *a;
                *a + u * d.dot(u) * 2.0 - d
            };
            out.push(Overlay::Line { a: sk.plane.to_world(*a), b: sk.plane.to_world(*b), color: Rgba::hex(0x4fb3ff), width: 2.5 });
            for pick in &self.session.selection {
                if let Pick::SketchEntity { sketch: s2, entity: e2 } = *pick
                    && s2 == sk.id
                    && e2 != entity
                    && let Some(e) = sk.entities.get(e2 as usize)
                {
                    for o in e.outlines() {
                        for w in o.windows(2) {
                            out.push(Overlay::Line {
                                a: sk.plane.to_world(flip(w[0])),
                                b: sk.plane.to_world(flip(w[1])),
                                color: Rgba::hex(0x1d4eff),
                                width: 1.7,
                            });
                        }
                    }
                }
            }
        }
        // Sketch Mirror over an origin axis (#104): the axis light blue and the picked curves mirrored across the plane
        // through it that stands square to their sketch.
        if let Tool::Mirror(t) = &self.tool
            && let Some(h) = t.hovered.filter(|h| (3..6).contains(h))
            && let Some(sk) = self.session.selection.iter().find_map(Pick::sketch).and_then(|id| self.session.doc.sketch(id))
            && let Some(n) = sk.plane.normal().cross(crate::workspace::tools::AXES[h - 3]).try_normalize()
        {
            let axis = crate::workspace::tools::AXES[h - 3] * self.camera.distance * 4.0;
            out.push(Overlay::Line { a: -axis, b: axis, color: Rgba::hex(0x4fb3ff), width: 2.5 });
            let flip = |p: DVec3| p - n * (2.0 * p.dot(n));
            for pick in &self.session.selection {
                let ents: Vec<u32> = match *pick {
                    Pick::SketchEntity { sketch, entity } if sketch == sk.id => vec![entity],
                    Pick::Sketch { sketch } if sketch == sk.id => (0..sk.entities.len() as u32).collect(),
                    _ => continue,
                };
                for e in ents.iter().filter_map(|e| sk.entities.get(*e as usize)) {
                    for o in e.outlines() {
                        for w in o.windows(2) {
                            out.push(Overlay::Line {
                                a: flip(sk.plane.to_world(w[0])),
                                b: flip(sk.plane.to_world(w[1])),
                                color: Rgba::hex(0x1d4eff),
                                width: 1.7,
                            });
                        }
                    }
                }
            }
        }
        // Sketch Fillet: the rounded result (changed lines green, new arcs blue) and the corners as rings, orange
        // when picked (s11-095).
        if let (Tool::SketchFillet { all, corners, radius }, Some(sk)) = (&self.tool, self.session.active_sketch()) {
            let r = self.numeric.as_deref().and_then(|t| self.eval_typed(t)).map(f64::abs).unwrap_or(*radius);
            if let Ok(list) = shapecraft_doc::sketch_ops::fillet_corners(&sk.entities, corners, r) {
                for (i, e) in list.iter().enumerate() {
                    if sk.entities.get(i) == Some(e) {
                        continue;
                    }
                    let color = if i >= sk.entities.len() { Rgba::hex(0x2e68ff) } else { Rgba::hex(0x7ed957) };
                    for w in e.outline().windows(2) {
                        out.push(Overlay::Line { a: sk.plane.to_world(w[0]), b: sk.plane.to_world(w[1]), color, width: 2.0 });
                    }
                }
            }
            for c in all {
                let on = corners.iter().any(|q| q.distance(*c) < 1e-7);
                out.push(Overlay::Dot {
                    p: sk.plane.to_world(*c),
                    color: if on { Rgba::hex(0xf5a540) } else { Rgba::hex(0x8e8e93) },
                    size: -10.0,
                });
            }
        }
        // Trim: the piece that would go, in the orange of a hovered curve (26.200, s11-084).
        if let (Tool::Trim { target: Some((i, at)) }, Some(sk)) = (&self.tool, self.session.active_sketch())
            && let Ok(shapecraft_doc::sketch_ops::Trimmed { removed: gone, .. }) =
                shapecraft_doc::sketch_ops::trim(&sk.entities, *i as usize, *at)
        {
            for w in gone.outline().windows(2) {
                out.push(Overlay::Line { a: sk.plane.to_world(w[0]), b: sk.plane.to_world(w[1]), color: Rgba::hex(0xd0861a), width: 2.0 });
            }
        }
        // Extend: the curve it would grow, and a ring where it would end.
        if let (Tool::Extend { target: Some((i, at)), .. }, Some(sk)) = (&self.tool, self.session.active_sketch())
            && let Some(e) = sk.entities.get(*i as usize)
        {
            let orange = Rgba::hex(0xd0861a);
            for w in e.outline().windows(2) {
                out.push(Overlay::Line { a: sk.plane.to_world(w[0]), b: sk.plane.to_world(w[1]), color: orange, width: 2.0 });
            }
            if let Ok(x) = shapecraft_doc::sketch_ops::extend(&sk.entities, *i as usize, *at) {
                out.push(Overlay::Dot { p: sk.plane.to_world(x.to), color: orange.with_alpha(170), size: 18.0 });
                out.push(Overlay::Dot { p: sk.plane.to_world(x.to), color: Rgba::hex(0x2e2e33), size: 8.0 });
            }
        }
        // A live fillet or chamfer keeps its edges drawn picked where they were: they're gone
        // from the rounded body, which isn't picked as a whole.
        if let Tool::Fillet(t) = &self.tool
            && self.is_live()
            && let Some(b) = self.session.before_last_step().and_then(|d| d.body(t.body))
        {
            let c = if theme::is_light() { SceneColors::light() } else { SceneColors::default() }.selected;
            for (line, edge) in b.mesh.edges.iter().zip(&b.mesh.edge_ids) {
                if t.edges.contains(edge) {
                    for w in line.windows(2) {
                        out.push(Overlay::Line {
                            a: DVec3::from(w[0].map(f64::from)),
                            b: DVec3::from(w[1].map(f64::from)),
                            color: c,
                            width: 3.0,
                        });
                    }
                }
            }
        }
        // A cut (Subtract, picked or auto) shows the volume it takes away tinted red over the part, live or not
        // (26.202, s36-disc-pull). Otherwise the ghost only stands in until the live geometry is ready (or if the
        // kernel refuses).
        if let Tool::Extrude(t) = &self.tool {
            // A typed depth shows once Return applies it (26.202, s39-cut-typed: only the orange profile meanwhile).
            if self.badge_current(crate::workspace::badges::Badge::Result) == Some(3) {
                t.cut_overlays(&mut out);
            } else if !self.has_live_preview() && !self.is_live() && self.growing_face().is_none() {
                t.overlays(&mut out);
            }
        }
        self.section_cap_overlays(&mut out, self.camera.world_per_pixel(self.size().y));
        out
    }

    /// Bodies a boolean shows see-through (observed 383–384): Subtract's target blue, what it cuts away cyan; a
    /// Union's or Intersect's bodies cyan once there are two.
    /// Split Body applied, before Done: the pieces grey-teal, the cuts showing as edges (s12-018).
    /// A face growing out (Extrude): the grown body in its own grey, see-through.
    pub(super) fn split_tints(&self) -> Vec<(shapecraft_doc::ItemId, Rgba)> {
        let mut tints = self.split_tints_of_tool();
        // What Undo or Redo just changed lights up blue-white and fades back to its own colour.
        if let Some((ids, since)) = &self.undo_flash
            && tints.is_empty()
        {
            let grey = if theme::is_light() { SceneColors::light() } else { SceneColors::default() }.body;
            let u = ((self.now - since) / UNDO_FLASH_S).clamp(0.0, 1.0) as f32;
            // Up fast, then a long fade.
            let k = UNDO_FLASH_PEAK * if u < 0.15 { u / 0.15 } else { 1.0 - (u - 0.15) / 0.85 };
            let k = k.clamp(0.0, 1.0);
            for id in ids {
                let Some(b) = self.session.doc.body(*id) else { continue };
                let own = b
                    .appearance
                    .as_ref()
                    .map_or(grey, |a| Rgba((a.color[0] * 255.0) as u8, (a.color[1] * 255.0) as u8, (a.color[2] * 255.0) as u8, 255));
                let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
                let hi = UNDO_FLASH_COLOR;
                tints.push((*id, Rgba(mix(own.0, hi.0), mix(own.1, hi.1), mix(own.2, hi.2), 255)));
            }
        }
        tints
    }

    fn split_tints_of_tool(&self) -> Vec<(shapecraft_doc::ItemId, Rgba)> {
        let grey = if theme::is_light() { SceneColors::light() } else { SceneColors::default() }.body;
        if let Some((body, _)) = self.growing_face() {
            return vec![(body, Rgba(grey.0, grey.1, grey.2, 150))];
        }
        // A profile pulled out into a new body: that body in plain grey, a little see-through, so its sketch shows
        // inside it.
        if let Tool::Extrude(t) = &self.tool
            && matches!(t.target, crate::workspace::tools::Target::Region { .. })
            && (self.is_live() || self.has_live_preview())
            && self.badge_current(crate::workspace::badges::Badge::Result) != Some(3)
            && let Some(base) = &self.cut_base
        {
            let shown = self.shown_doc();
            return shown.bodies.iter().filter(|b| base.body(b.id).is_none()).map(|b| (b.id, Rgba(grey.0, grey.1, grey.2, 205))).collect();
        }
        match &self.tool {
            Tool::Mirror(t) if t.split && self.is_live() => t.items.iter().chain(&t.pieces).map(|b| (*b, Rgba::hex(0x7da6b4))).collect(),
            _ => Vec::new(),
        }
    }

    pub(in crate::workspace) fn boolean_ghosts(&self) -> Vec<(shapecraft_doc::ItemId, Rgba)> {
        let Tool::Boolean(t) = &self.tool else { return Vec::new() };
        if t.op == "subtract" {
            t.targets.iter().map(|b| (*b, Rgba::hex(0x3d7bff))).chain(t.tools.iter().map(|b| (*b, Rgba::hex(0x1fb8e8)))).collect()
        } else if t.targets.len() >= 2 {
            t.targets.iter().map(|b| (*b, Rgba::hex(0x1fb8e8))).collect()
        } else {
            Vec::new()
        }
    }
}

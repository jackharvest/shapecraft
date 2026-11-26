//! Section View's cut faces for Section Only and 2D Section View (observed s16-023/024): the bodies are cut by the
//! kernel, the faces lying on the cut are filled a dusky red with darker 45° hatching, outlined white.

use glam::{DVec2, DVec3};
use shapecraft_render::{Overlay, Rgba};

use super::Workspace;

/// The cut faces for a section at a revision: triangles and boundary.
#[derive(Default)]
pub struct Caps {
    key: Option<(u64, [u64; 6])>,
    tris: Vec<[DVec3; 3]>,
    outline: Vec<(DVec3, DVec3)>,
    /// The plain Section View: where the plane cuts each body's surface, to outline the cut faces.
    cut: Vec<(DVec3, DVec3)>,
    /// Those cut faces filled, body by body.
    faces: Vec<[DVec3; 3]>,
    /// Made for Section Only or the 2D Section View.
    only: bool,
}

/// Measured on s16-024.
const FILL: u32 = 0x5c4c4c;
const HATCH: u32 = 0x4e4141;
/// The plain Section View's cut faces: a brick red with darker stripes about 8 px apart (measured 2026-11-25).
const CAP: u32 = 0x67413c;
const CAP_HATCH: u32 = 0x583833;

fn basis(n: DVec3) -> (DVec3, DVec3) {
    let u = if n.z.abs() > 0.9 { DVec3::X } else { DVec3::Z.cross(n).normalize() };
    (u, n.cross(u))
}

impl Caps {
    fn same(&self, key: (u64, [u64; 6]), only: bool) -> bool {
        self.key == Some(key) && self.only == only
    }
}

/// Where a plane cuts a mesh: a segment for each triangle it runs through.
fn slice(m: &shapecraft_kernel::Mesh, o: DVec3, n: DVec3) -> Vec<(DVec3, DVec3)> {
    let at = |i: u32| m.positions.get(i as usize).map_or(DVec3::ZERO, |q| DVec3::from(q.map(f64::from)));
    let side: Vec<f64> = m.positions.iter().map(|q| (DVec3::from(q.map(f64::from)) - o).dot(n)).collect();
    let mut out = Vec::new();
    for t in m.indices.as_chunks::<3>().0 {
        let Some(s) = t.iter().map(|i| side.get(*i as usize).copied()).collect::<Option<Vec<f64>>>() else { continue };
        if s.iter().all(|x| *x > 0.0) || s.iter().all(|x| *x < 0.0) {
            continue;
        }
        let mut pts = Vec::new();
        for k in 0..3 {
            let (a, b) = (s[k], s[(k + 1) % 3]);
            if a * b < 0.0 {
                pts.push(at(t[k]).lerp(at(t[(k + 1) % 3]), a / (a - b)));
            } else if a == 0.0 {
                pts.push(at(t[k]));
            }
        }
        if let [a, b] = pts[..] {
            out.push((a, b));
        }
    }
    out
}

/// More cut segments than this and the cut faces go unfilled (they show as the bodies' insides).
const FILL_LIMIT: usize = 20_000;

/// The area inside closed outlines on the plane (inside an odd number of them), as triangles: a strip between every two
/// heights at which an outline turns.
fn fill(lines: &[(DVec3, DVec3)], o: DVec3, n: DVec3) -> Vec<[DVec3; 3]> {
    if lines.is_empty() || lines.len() > FILL_LIMIT {
        return Vec::new();
    }
    let (u, v) = basis(n);
    let flat = |q: DVec3| DVec2::new((q - o).dot(u), (q - o).dot(v));
    let mut segs: Vec<(DVec2, DVec2)> = lines
        .iter()
        .map(|(a, b)| (flat(*a), flat(*b)))
        .map(|(a, b)| if a.y <= b.y { (a, b) } else { (b, a) })
        .filter(|(a, b)| b.y - a.y > 1e-12)
        .collect();
    segs.sort_by(|p, q| p.0.y.total_cmp(&q.0.y));
    let mut levels: Vec<f64> = segs.iter().flat_map(|s| [s.0.y, s.1.y]).collect();
    levels.sort_by(f64::total_cmp);
    let span = levels.last().zip(levels.first()).map_or(0.0, |(a, b)| a - b);
    levels.dedup_by(|a, b| (*a - *b).abs() <= span * 1e-7);
    let mut out = Vec::new();
    let mut active: Vec<(DVec2, DVec2)> = Vec::new();
    let mut next = 0;
    for w in levels.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let mid = (lo + hi) / 2.0;
        while next < segs.len() && segs[next].0.y <= mid {
            active.push(segs[next]);
            next += 1;
        }
        active.retain(|s| s.1.y > mid);
        let at = |s: &(DVec2, DVec2), y: f64| s.0.x + (y - s.0.y) / (s.1.y - s.0.y) * (s.1.x - s.0.x);
        let mut across: Vec<(f64, f64, f64)> = active.iter().map(|s| (at(s, mid), at(s, lo), at(s, hi))).collect();
        across.sort_by(|a, b| a.0.total_cmp(&b.0));
        for p in across.as_chunks::<2>().0 {
            let q = |x: f64, y: f64| o + u * x + v * y;
            let (a, b, c, d) = (q(p[0].1, lo), q(p[1].1, lo), q(p[1].2, hi), q(p[0].2, hi));
            out.push([a, b, c]);
            out.push([a, c, d]);
        }
    }
    out
}

impl Workspace {
    /// A section being edited takes a typed offset.
    pub(super) fn section_typing(&self) -> bool {
        self.section.is_some_and(|s| s.editing) && matches!(self.tool, super::tools::Tool::Idle) && self.dim_edit.is_none()
    }

    /// Whether typed digits are a value here: the running tool's, or the section's offset.
    pub(super) fn takes_values(&self) -> bool {
        self.tool.takes_numbers() || self.section_typing()
    }

    pub(super) fn commit_section_value(&mut self) {
        let value = self.numeric.take().and_then(|t| self.eval_typed(&t));
        if let (Some(v), Some(s)) = (value.filter(|v| v.is_finite() && v.abs() < 1e7), &mut self.section) {
            s.offset = v;
        }
    }

    /// Recomputes the cut faces when the model or the section moved.
    pub(super) fn refresh_section_caps(&mut self) {
        let Some(sec) = self.section else {
            self.section_caps = Caps::default();
            return;
        };
        let (o, n) = (sec.origin(), sec.normal);
        let key = (self.session.revision, [o.x.to_bits(), o.y.to_bits(), o.z.to_bits(), n.x.to_bits(), n.y.to_bits(), n.z.to_bits()]);
        if self.section_caps.same(key, sec.caps_only()) {
            return;
        }
        if !sec.caps_only() {
            let (mut cut, mut faces) = (Vec::new(), Vec::new());
            for b in self.session.doc.bodies.iter().filter(|b| self.session.doc.item_shown(b.id)) {
                let lines = slice(&b.mesh, o, n);
                faces.extend(fill(&lines, o, n));
                cut.extend(lines);
            }
            self.section_caps = Caps { key: Some(key), cut, faces, only: false, ..Caps::default() };
            return;
        }
        let kernel = &self.session.kernel;
        let mut caps = Caps { key: Some(key), only: true, ..Caps::default() };
        for body in self.session.doc.bodies.iter().filter(|b| b.visible) {
            let Ok(pieces) = kernel.split_by_plane(&body.shape, o, n) else { continue };
            for piece in pieces {
                // The kept side: behind the plane (the normal points at what's cut away).
                let Ok(bb) = kernel.bounds(&piece) else { continue };
                if ((bb.min + bb.max) / 2.0 - o).dot(n) > 0.0 {
                    continue;
                }
                let Ok(count) = kernel.face_count(&piece) else { continue };
                let on_cut: Vec<u32> = (0..count)
                    .filter(|&f| {
                        kernel
                            .face_plane(&piece, f)
                            .ok()
                            .flatten()
                            .is_some_and(|fp| fp.normal.dot(n).abs() > 0.9999 && (fp.origin - o).dot(n).abs() < 1e-6)
                    })
                    .collect();
                if on_cut.is_empty() {
                    continue;
                }
                let Ok(mesh) = kernel.tessellate(&piece, shapecraft_doc::project::deflection()) else { continue };
                let p = |i: u32| mesh.positions.get(i as usize).map_or(DVec3::ZERO, |q| DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
                let mut edges: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
                for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
                    if !mesh.triangle_faces.get(t).is_some_and(|f| on_cut.contains(f)) {
                        continue;
                    }
                    caps.tris.push([p(tri[0]), p(tri[1]), p(tri[2])]);
                    for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                        *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                    }
                }
                caps.outline.extend(edges.into_iter().filter(|(_, k)| *k == 1).map(|((a, b), _)| (p(a), p(b))));
            }
        }
        self.section_caps = caps;
    }

    /// The cut faces as overlays, hatched every few pixels.
    pub(super) fn section_cap_overlays(&self, out: &mut Vec<Overlay>, world_per_px: f64) {
        let Some(sec) = self.section else { return };
        if !sec.caps_only() {
            let edge = Rgba::hex(0x17181a);
            out.push(Overlay::Cap { tris: self.section_caps.faces.clone(), color: Rgba::hex(CAP) });
            let (o, n) = (sec.origin(), sec.normal);
            let (u, v) = basis(n);
            let flat = |q: DVec3| DVec2::new((q - o).dot(u), (q - o).dot(v));
            let lines: Vec<(DVec2, DVec2)> = self.section_caps.cut.iter().map(|(a, b)| (flat(*a), flat(*b))).collect();
            let span = self.section_caps.cut.iter().map(|(a, _)| (*a - o).length()).fold(0.0, f64::max);
            for (a, b) in shapecraft_engine::drawing::hatch(&lines, (world_per_px * 8.0).max(span / 400.0)) {
                // Just short of the outline, so their ends don't stick out past it.
                let inset = (b - a).normalize_or_zero() * world_per_px * 1.5;
                if (b - a).length() < world_per_px * 4.0 {
                    continue;
                }
                let (a, b) = (a + inset, b - inset);
                let at = |q: DVec2| o + u * q.x + v * q.y;
                out.push(Overlay::Edge { a: at(a), b: at(b), color: Rgba::hex(CAP_HATCH), width: 2.0 });
            }
            out.extend(self.section_caps.cut.iter().map(|(a, b)| Overlay::Edge { a: *a, b: *b, color: edge, width: 1.6 }));
            return;
        }
        if self.section_caps.tris.is_empty() {
            return;
        }
        let (o, n) = (sec.origin(), sec.normal);
        let (u, v) = basis(n);
        // Lifted a hair toward the viewer so the fill and its lines don't fight.
        let lift = -n * world_per_px * 0.5;
        out.push(Overlay::Fill {
            tris: self.section_caps.tris.iter().map(|t| [t[0] + lift, t[1] + lift, t[2] + lift]).collect(),
            color: Rgba::hex(FILL),
        });
        let flat: Vec<(DVec2, DVec2)> = self
            .section_caps
            .outline
            .iter()
            .map(|(a, b)| (DVec2::new((*a - o).dot(u), (*a - o).dot(v)), DVec2::new((*b - o).dot(u), (*b - o).dot(v))))
            .collect();
        let back = |q: DVec2| o + u * q.x + v * q.y + lift * 2.0;
        // Never more than a couple of thousand lines, however far in the view is zoomed.
        let span = self.section_caps.outline.iter().flat_map(|(a, b)| [(*a - o).length(), (*b - o).length()]).fold(0.0, f64::max);
        for (a, b) in shapecraft_engine::drawing::hatch(&flat, (world_per_px * 7.0).max(span / 1000.0)) {
            out.push(Overlay::Line { a: back(a), b: back(b), color: Rgba::hex(HATCH), width: 1.5 });
        }
        let edge = if sec.flat.is_some() { Rgba::hex(0xf2f2f2) } else { Rgba::hex(0x8a7676) };
        for (a, b) in &self.section_caps.outline {
            out.push(Overlay::Line { a: *a + lift * 3.0, b: *b + lift * 3.0, color: edge, width: 1.2 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square with a round hole, as a section cuts it: the fill covers the square less the hole, drawn the right size.
    #[test]
    fn a_cut_face_fills_round_its_hole() {
        let ring = |n: usize, r: f64, turn: f64| -> Vec<(DVec3, DVec3)> {
            let at = |k: usize| {
                let a = turn + std::f64::consts::TAU * k as f64 / n as f64;
                DVec3::new(r * a.cos(), r * a.sin(), 5.0)
            };
            (0..n).map(|k| (at(k), at(k + 1))).collect()
        };
        let mut lines = ring(4, 10.0 * 2f64.sqrt(), std::f64::consts::FRAC_PI_4);
        lines.extend(ring(90, 4.0, 0.1));
        let tris = fill(&lines, DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
        let area: f64 = tris.iter().map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() / 2.0).sum();
        let want = 400.0 - 0.5 * 90.0 * 16.0 * (std::f64::consts::TAU / 90.0).sin();
        assert!((area - want).abs() < 1e-6 * want, "{area} vs {want}");
    }
}

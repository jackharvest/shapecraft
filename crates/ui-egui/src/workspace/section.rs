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
}

/// Measured on s16-024.
const FILL: u32 = 0x5c4c4c;
const HATCH: u32 = 0x4e4141;

fn basis(n: DVec3) -> (DVec3, DVec3) {
    let u = if n.z.abs() > 0.9 { DVec3::X } else { DVec3::Z.cross(n).normalize() };
    (u, n.cross(u))
}

impl Workspace {
    /// Recomputes the cut faces when the model or the section moved.
    pub(super) fn refresh_section_caps(&mut self) {
        let Some(sec) = self.section.filter(|s| s.caps_only()) else {
            self.section_caps = Caps::default();
            return;
        };
        let (o, n) = (sec.origin(), sec.normal);
        let key = (self.session.revision, [o.x.to_bits(), o.y.to_bits(), o.z.to_bits(), n.x.to_bits(), n.y.to_bits(), n.z.to_bits()]);
        if self.section_caps.key == Some(key) {
            return;
        }
        let kernel = &self.session.kernel;
        let mut caps = Caps { key: Some(key), ..Caps::default() };
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
        let Some(sec) = self.section.filter(|s| s.caps_only()) else { return };
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

//! Mesh bodies: the triangles of an STL, an OBJ or a .shapr mesh body, kept as they came instead of being turned into
//! exact faces. A mesh body is used to work from, moved, patterned and mirrored, and combined
//! with booleans (staying a mesh), but it has no faces or edges to pick or round.

use std::collections::HashMap;

use glam::{DMat4, DVec3};
use shapecraft_kernel::Mesh;

/// Creases sharper than this keep their own normals; gentler ones shade smooth (a tessellated sphere looks round,
/// a cube keeps its corners).
const CREASE_DEGREES: f64 = 40.0;

/// The largest mesh body kept (triangles); bigger files are refused rather than freezing the app.
pub const MAX_TRIANGLES: usize = 20_000_000;

const TEXT_PREFIX: &str = "SCMESH64:";

/// A mesh body's triangles, welded (shared corners use one point), in millimeters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facets {
    pub points: Vec<DVec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Worked out once (`is_closed` walks every edge; the tool column asks every frame).
    closed: bool,
}

impl Facets {
    /// From welded points and indices (3 per triangle); `None` when the indices don't fit.
    pub fn new(points: Vec<DVec3>, indices: &[u32]) -> Option<Self> {
        if !indices.len().is_multiple_of(3) || indices.len() / 3 > MAX_TRIANGLES {
            return None;
        }
        let n = points.len() as u32;
        let triangles: Vec<[u32; 3]> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .copied()
            .filter(|t| t.iter().all(|&i| i < n) && t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        if triangles.is_empty() {
            return None;
        }
        let closed = closed(&triangles);
        Some(Self { points, triangles, closed })
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Every edge is shared by exactly two triangles: a closed (solid) mesh, which booleans accept. An open (sheet)
    /// mesh is only for looking at and measuring.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let first = *self.points.first()?;
        Some(self.points.iter().fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
    }

    /// Surface area (mm²) and, for a closed mesh, the enclosed volume (mm³; divergence theorem).
    pub fn area_volume(&self) -> (f64, Option<f64>) {
        let (mut area, mut vol) = (0.0, 0.0);
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.points[i as usize]);
            area += (b - a).cross(c - a).length() / 2.0;
            vol += a.dot(b.cross(c)) / 6.0;
        }
        (area, self.closed.then_some(vol.abs()))
    }

    /// The same triangles moved by a transform (a mirror flips them, so they keep facing outwards).
    pub fn transformed(&self, m: &DMat4) -> Self {
        let points = self.points.iter().map(|p| m.transform_point3(*p)).collect();
        let flip = m.determinant() < 0.0;
        let triangles = if flip { self.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect() } else { self.triangles.clone() };
        Self { points, triangles, closed: self.closed }
    }

    /// Welded positions and flat indices, as the kernel takes them.
    pub fn flat_indices(&self) -> Vec<u32> {
        self.triangles.iter().flatten().copied().collect()
    }

    /// The display mesh: three corners per triangle, each with the normal of the triangles around that point that
    /// meet it at a gentle angle. No edge polylines: a mesh body shows no edges, and every triangle belongs to
    /// "face 0" (it is picked whole).
    pub fn render_mesh(&self) -> Mesh {
        let normals: Vec<DVec3> = self
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| self.points[i as usize]);
                (b - a).cross(c - a) // area-weighted
            })
            .collect();
        let mut around: Vec<Vec<u32>> = vec![Vec::new(); self.points.len()];
        for (i, t) in self.triangles.iter().enumerate() {
            for &v in t {
                around[v as usize].push(i as u32);
            }
        }
        let cos = CREASE_DEGREES.to_radians().cos();
        let n = self.triangles.len();
        let mut positions = Vec::with_capacity(n * 3);
        let mut out_normals = Vec::with_capacity(n * 3);
        for (i, t) in self.triangles.iter().enumerate() {
            let own = normals[i].normalize_or_zero();
            for &v in t {
                let sum = around[v as usize]
                    .iter()
                    .map(|&j| normals[j as usize])
                    .filter(|m| m.normalize_or_zero().dot(own) >= cos)
                    .fold(DVec3::ZERO, |acc, m| acc + m);
                let nrm = if sum.length_squared() > 0.0 { sum.normalize() } else { own };
                let p = self.points[v as usize];
                positions.push([p.x as f32, p.y as f32, p.z as f32]);
                out_normals.push([nrm.x as f32, nrm.y as f32, nrm.z as f32]);
            }
        }
        Mesh {
            indices: (0..(n * 3) as u32).collect(),
            positions,
            normals: out_normals,
            triangle_faces: vec![0; n],
            edges: Vec::new(),
            edge_ids: Vec::new(),
            edge_faces: Vec::new(),
            bounds_cache: Default::default(),
        }
    }

    /// The triangle under a ray (origin, direction), nearest first: its distance along the ray and its plane
    /// (a point and the outward normal), for Sketch on Face and measuring on a mesh body.
    pub fn hit(&self, origin: DVec3, dir: DVec3) -> Option<(f64, DVec3, DVec3)> {
        let mut best: Option<(f64, DVec3, DVec3)> = None;
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.points[i as usize]);
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-12 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - a;
            let u = s.dot(p) * inv;
            let q = s.cross(e1);
            let v = dir.dot(q) * inv;
            if !(0.0..=1.0).contains(&u) || v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = e2.dot(q) * inv;
            if d > 0.0 && best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, origin + dir * d, e1.cross(e2).normalize_or_zero()));
            }
        }
        best
    }

    /// A compact file of its own: "SCMESH1", the point and triangle counts, the points (f64 x, y, z) and the
    /// triangles (u32 × 3), little-endian. Exact (no rounding), and quick to read and write.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.points.len() * 24 + self.triangles.len() * 12);
        out.extend_from_slice(b"SCMESH1\0");
        out.extend_from_slice(&(self.points.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.triangles.len() as u32).to_le_bytes());
        for p in &self.points {
            for v in [p.x, p.y, p.z] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        for t in &self.triangles {
            for v in t {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out
    }

    /// The file format as text ("SCMESH64:" and base64), for the History's unlinked copies.
    pub fn to_text(&self) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = self.to_bytes();
        let mut out = String::with_capacity(9 + bytes.len().div_ceil(3) * 4);
        out.push_str(TEXT_PREFIX);
        for c in bytes.chunks(3) {
            let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= c.len() {
                    out.push(T[((n >> (18 - 6 * k)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn from_text(text: &str) -> Option<Self> {
        let body = text.strip_prefix(TEXT_PREFIX)?.trim_end_matches('=');
        let val = |c: u8| -> Option<u32> {
            Some(match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            } as u32)
        };
        let mut out = Vec::with_capacity(body.len() * 3 / 4);
        for c in body.as_bytes().chunks(4) {
            let mut n = 0u32;
            for (k, &ch) in c.iter().enumerate() {
                n |= val(ch)? << (18 - 6 * k);
            }
            for k in 0..c.len().saturating_sub(1) {
                out.push((n >> (16 - 8 * k)) as u8);
            }
        }
        Self::from_bytes(&out)
    }

    /// Whether History text holds a mesh (`to_text`) rather than an exact shape.
    pub fn is_text(text: &str) -> bool {
        text.starts_with(TEXT_PREFIX)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(b"SCMESH1\0")?;
        let word = |at: usize| rest.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let (np, nt) = (word(0)? as usize, word(4)? as usize);
        if nt > MAX_TRIANGLES || rest.len() != 8 + np * 24 + nt * 12 {
            return None;
        }
        let f = |at: usize| f64::from_le_bytes(rest[at..at + 8].try_into().unwrap_or([0; 8]));
        let points = (0..np).map(|i| DVec3::new(f(8 + i * 24), f(16 + i * 24), f(24 + i * 24))).collect();
        let base = 8 + np * 24;
        let mut idx = Vec::with_capacity(nt * 3);
        for i in 0..nt * 3 {
            idx.push(word(base + i * 4)?);
        }
        Self::new(points, &idx)
    }
}

fn closed(triangles: &[[u32; 3]]) -> bool {
    let mut uses: HashMap<(u32, u32), u32> = HashMap::with_capacity(triangles.len() * 2);
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    !uses.is_empty() && uses.values().all(|&n| n == 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Facets {
        let p: Vec<DVec3> = (0..8).map(|i| DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 10.0).collect();
        let q = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let idx: Vec<u32> = q.iter().flat_map(|f| [f[0], f[1], f[2], f[0], f[2], f[3]]).collect();
        Facets::new(p, &idx).unwrap_or_default()
    }

    #[test]
    fn a_cube_is_closed_and_keeps_its_corners() {
        let c = cube();
        assert!(c.is_closed());
        let m = c.render_mesh();
        assert_eq!(m.triangle_faces.len(), 12);
        assert!(m.edges.is_empty());
        // Hard 90° creases: every corner normal is an axis (no smoothing across the cube's edges).
        assert!(m.normals.iter().all(|n| n.iter().filter(|v| v.abs() > 0.99).count() == 1));
    }

    #[test]
    fn an_open_mesh_is_not_closed() {
        let c = cube();
        let open = Facets::new(c.points.clone(), &c.flat_indices()[..30]).unwrap_or_default();
        assert!(!open.is_closed());
    }

    #[test]
    fn the_file_round_trips_exactly() {
        let c = cube().transformed(&DMat4::from_translation(DVec3::new(0.1, 0.2, 0.3)));
        assert_eq!(Facets::from_bytes(&c.to_bytes()), Some(c.clone()));
        assert_eq!(Facets::from_text(&c.to_text()), Some(c));
        assert_eq!(Facets::from_bytes(b"nope"), None);
    }

    #[test]
    fn a_mirror_keeps_the_triangles_facing_out() {
        let c = cube().transformed(&DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)));
        assert!(c.is_closed());
        // The first triangle (bottom, z = 0) still faces down after the flip.
        let [a, b, d] = c.triangles[0].map(|i| c.points[i as usize]);
        assert!((b - a).cross(d - a).z < 0.0);
    }

    #[test]
    fn a_ray_finds_the_nearest_triangle() {
        let hit = cube().hit(DVec3::new(5.0, 5.0, 50.0), DVec3::NEG_Z);
        let (d, p, n) = hit.unwrap_or_default();
        assert!((d - 40.0).abs() < 1e-9 && (p.z - 10.0).abs() < 1e-9 && n.z > 0.99);
    }
}

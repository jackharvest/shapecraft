use std::io::Write;

use crate::{KernelError, Result};

/// Display triangles plus feature edges, with the B-rep face each triangle came from.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// One entry per triangle: index of the source face in the shape's face order.
    pub triangle_faces: Vec<u32>,
    /// Polylines along the shape's edges, for drawing outlines.
    pub edges: Vec<Vec<[f32; 3]>>,
    /// The shape's edge index for each polyline in `edges`.
    pub edge_ids: Vec<u32>,
    /// The (up to two) faces each polyline in `edges` borders; `u32::MAX` where there is none.
    pub edge_faces: Vec<[u32; 2]>,
    /// What `bounds` found. A mesh doesn't change once it's made and shared, so it's worked out once.
    pub bounds_cache: std::sync::OnceLock<Option<([f32; 3], [f32; 3])>>,
}

impl Mesh {
    /// The smallest box around the triangles and edges (corners low, high); `None` when there's nothing or it isn't finite.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        *self.bounds_cache.get_or_init(|| {
            let mut points = self.positions.iter().chain(self.edges.iter().flatten());
            let first = *points.next()?;
            let (lo, hi) = points.fold((first, first), |(mut lo, mut hi), p| {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
                (lo, hi)
            });
            lo.iter().chain(&hi).all(|v| v.is_finite()).then_some((lo, hi))
        })
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Writes binary STL.
    pub fn write_stl(&self, mut out: impl Write) -> Result<()> {
        let io = |e: std::io::Error| KernelError::Io(e.to_string());
        let count = u32::try_from(self.triangle_count()).map_err(|_| KernelError::InvalidInput("too many triangles for STL".into()))?;
        let mut header = [0u8; 80];
        let tag = b"ShapeCraft binary STL";
        header[..tag.len()].copy_from_slice(tag);
        out.write_all(&header).map_err(io)?;
        out.write_all(&count.to_le_bytes()).map_err(io)?;
        for tri in self.indices.as_chunks::<3>().0 {
            let p = |i: u32| self.positions.get(i as usize).copied().unwrap_or_default();
            let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let n = face_normal(a, b, c);
            for v in [n, a, b, c] {
                for f in v {
                    out.write_all(&f.to_le_bytes()).map_err(io)?;
                }
            }
            out.write_all(&[0, 0]).map_err(io)?;
        }
        Ok(())
    }
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 && len.is_finite() { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0; 3] }
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    #[test]
    fn bounds_cover_triangles_and_edges() {
        assert_eq!(Mesh::default().bounds(), None);
        let mesh = Mesh { positions: vec![[1.0, 2.0, 3.0], [0.0, 5.0, 1.0]], edges: vec![vec![[-1.0, 0.0, 9.0]]], ..Default::default() };
        assert_eq!(mesh.bounds(), Some(([-1.0, 0.0, 1.0], [1.0, 5.0, 9.0])));
        let broken = Mesh { positions: vec![[f32::NAN, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0]], ..Default::default() };
        assert_eq!(broken.bounds(), None);
    }
}

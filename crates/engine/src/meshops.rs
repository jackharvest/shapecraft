//! Booleans and splits of mesh bodies on their triangles (Manifold's robust mesh booleans, ported to Rust), so a
//! mesh body stays a mesh. Sewing triangles into exact faces and using the B-rep booleans gets
//! coplanar faces wrong (two STL cubes sharing a top face come out with a face missing).

use glam::DVec3;
use manifold_rust::linalg::Vec3;
use manifold_rust::manifold::Manifold;
use manifold_rust::types::MeshGL64;
use shapecraft_doc::Facets;

use crate::EngineError;

type Result<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Union,
    Subtract,
    Intersect,
}

fn to_manifold(f: &Facets) -> Result<Manifold> {
    if !f.is_closed() {
        return Err(EngineError::NotAvailable("booleans need a closed mesh body (this one is open)".into()));
    }
    let mesh = MeshGL64 {
        num_prop: 3,
        vert_properties: f.points.iter().flat_map(|p| [p.x, p.y, p.z]).collect(),
        tri_verts: f.triangles.iter().flat_map(|t| t.map(u64::from)).collect(),
        ..Default::default()
    };
    let m = Manifold::from_mesh_gl64_robust(&mesh);
    if m.is_empty() && f.triangle_count() > 0 {
        return Err(EngineError::NotAvailable("that mesh body isn't a closed, consistent surface, so it can't be combined".into()));
    }
    Ok(m)
}

fn from_manifold(m: &Manifold) -> Option<Facets> {
    let g = m.get_mesh_gl64(-1);
    let n = g.num_prop as usize;
    if n < 3 {
        return None;
    }
    let points: Vec<DVec3> = g.vert_properties.chunks_exact(n).map(|v| DVec3::new(v[0], v[1], v[2])).collect();
    let idx: Vec<u32> = g.tri_verts.iter().map(|&i| i as u32).collect();
    Facets::new(points, &idx)
}

/// The pieces that a manifold falls into, each a mesh body.
fn pieces(m: &Manifold) -> Vec<Facets> {
    let parts = m.decompose();
    let parts = if parts.is_empty() { vec![m.clone()] } else { parts };
    parts.iter().filter_map(from_manifold).collect()
}

/// `target` combined with every tool in turn; the result in pieces (empty when nothing is left).
pub fn boolean(op: Op, target: &Facets, tools: &[Facets]) -> Result<Vec<Facets>> {
    let mut m = to_manifold(target)?;
    for t in tools {
        let t = to_manifold(t)?;
        m = match op {
            Op::Union => m.union(&t),
            Op::Subtract => m.difference(&t),
            Op::Intersect => m.intersection(&t),
        };
    }
    Ok(pieces(&m))
}

/// A mesh cut by a plane: a closed mesh into closed (capped) pieces, an open one by clipping its triangles. `None`
/// when the plane misses it.
pub fn split(f: &Facets, origin: DVec3, normal: DVec3) -> Result<Option<Vec<Facets>>> {
    let n = normal.normalize_or_zero();
    if n == DVec3::ZERO {
        return Err(EngineError::BadParams("the splitting plane has no normal".into()));
    }
    let side = |p: DVec3| (p - origin).dot(n);
    let (lo, hi) = f.points.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(side(*p)), hi.max(side(*p))));
    let eps = (hi - lo).abs().max(1.0) * 1e-9;
    if lo >= -eps || hi <= eps {
        return Ok(None);
    }
    if f.is_closed() {
        let m = to_manifold(f)?;
        let (a, b) = m.split_by_plane(Vec3::new(n.x, n.y, n.z), origin.dot(n));
        let mut out = pieces(&a);
        out.extend(pieces(&b));
        return Ok((out.len() > 1).then_some(out));
    }
    Ok(clip_open(f, origin, n, eps))
}

/// An open mesh's triangles on each side of a plane, the ones it crosses cut in two.
fn clip_open(f: &Facets, origin: DVec3, n: DVec3, eps: f64) -> Option<Vec<Facets>> {
    let mut sides: [(Vec<DVec3>, Vec<u32>); 2] = [(Vec::new(), Vec::new()), (Vec::new(), Vec::new())];
    let mut push = |s: usize, poly: &[DVec3]| {
        let (pts, idx) = &mut sides[s];
        for k in 1..poly.len().saturating_sub(1) {
            for p in [poly[0], poly[k], poly[k + 1]] {
                idx.push(pts.len() as u32);
                pts.push(p);
            }
        }
    };
    for t in &f.triangles {
        let tri = t.map(|i| f.points[i as usize]);
        let d = tri.map(|p| (p - origin).dot(n));
        if d.iter().all(|&v| v >= -eps) {
            push(0, &tri);
            continue;
        }
        if d.iter().all(|&v| v <= eps) {
            push(1, &tri);
            continue;
        }
        let (mut above, mut below) = (Vec::new(), Vec::new());
        for k in 0..3 {
            let (p, q, dp, dq) = (tri[k], tri[(k + 1) % 3], d[k], d[(k + 1) % 3]);
            if dp >= 0.0 {
                above.push(p);
            }
            if dp <= 0.0 {
                below.push(p);
            }
            if (dp > 0.0 && dq < 0.0) || (dp < 0.0 && dq > 0.0) {
                let x = p + (q - p) * (dp / (dp - dq));
                above.push(x);
                below.push(x);
            }
        }
        push(0, &above);
        push(1, &below);
    }
    let out: Vec<Facets> = sides
        .into_iter()
        .filter_map(|(pts, tris)| {
            let tris: Vec<[u32; 3]> = tris.as_chunks::<3>().0.to_vec();
            let (pts, idx) = crate::formats::weld(&pts, &tris);
            Facets::new(pts, &idx)
        })
        .collect();
    (out.len() == 2).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(at: DVec3) -> Facets {
        let p: Vec<DVec3> = (0..8).map(|i| at + DVec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64) * 10.0).collect();
        let q = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let idx: Vec<u32> = q.iter().flat_map(|f| [f[0], f[1], f[2], f[0], f[2], f[3]]).collect();
        Facets::new(p, &idx).unwrap_or_default()
    }

    fn vol(f: &Facets) -> f64 {
        f.area_volume().1.unwrap_or(f64::NAN)
    }

    #[test]
    fn cubes_sharing_faces_combine_exactly() {
        let (a, b) = (cube(DVec3::ZERO), cube(DVec3::new(5.0, 0.0, 0.0)));
        let sub = boolean(Op::Subtract, &a, std::slice::from_ref(&b)).unwrap();
        assert_eq!(sub.len(), 1);
        assert!((vol(&sub[0]) - 500.0).abs() < 1e-6 && sub[0].is_closed());
        assert!((vol(&boolean(Op::Union, &a, std::slice::from_ref(&b)).unwrap()[0]) - 1500.0).abs() < 1e-6);
        assert!((vol(&boolean(Op::Intersect, &a, &[b]).unwrap()[0]) - 500.0).abs() < 1e-6);
    }

    #[test]
    fn a_cut_through_the_middle_makes_two_pieces() {
        let a = cube(DVec3::ZERO);
        let b = Facets::new(
            (0..8)
                .map(|i| DVec3::new(-1.0 + 12.0 * (i & 1) as f64, 4.0 + 2.0 * ((i >> 1) & 1) as f64, -1.0 + 12.0 * ((i >> 2) & 1) as f64))
                .collect(),
            &a.flat_indices(),
        )
        .unwrap_or_default();
        let parts = boolean(Op::Subtract, &a, &[b]).unwrap();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn splits_cap_a_closed_mesh_and_clip_an_open_one() {
        let parts = split(&cube(DVec3::ZERO), DVec3::new(0.0, 0.0, 3.0), DVec3::Z).unwrap().unwrap();
        assert_eq!(parts.len(), 2);
        let mut v: Vec<f64> = parts.iter().map(vol).collect();
        v.sort_by(f64::total_cmp);
        assert!((v[0] - 300.0).abs() < 1e-6 && (v[1] - 700.0).abs() < 1e-6, "{v:?}");
        let c = cube(DVec3::ZERO);
        let open = Facets::new(c.points.clone(), &c.flat_indices()[..30]).unwrap_or_default();
        let parts = split(&open, DVec3::new(0.0, 0.0, 5.0), DVec3::Z).unwrap().unwrap();
        assert_eq!(parts.len(), 2);
        assert!(split(&open, DVec3::new(0.0, 0.0, 50.0), DVec3::Z).unwrap().is_none(), "a plane that misses");
    }
}

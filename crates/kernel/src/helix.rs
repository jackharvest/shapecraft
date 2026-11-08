//! A helical revolve's path, worked out here so every kernel shares it: the profile is carried along a helix by the
//! helix's Frenet frame, which for a helix is exactly a screw motion (turn about the axis while rising along it), so
//! any radius gives the same solid. The helix goes through the profile's middle, which keeps the sweep accurate.

use glam::DVec3;

use crate::{Axis, KernelError, Mesh, Result};

fn refuse(message: &str) -> KernelError {
    KernelError::Operation { op: "revolve", message: message.into() }
}

/// Points along the helix and the way it runs at each, for a profile (`mesh`, a flat face) turned `angle` radians
/// around `axis` while it rises `height` along it. Refuses a profile touching the axis and turns that would run into
/// each other.
pub fn path(mesh: &Mesh, axis: Axis, angle: f64, height: f64) -> Result<Vec<(DVec3, DVec3)>> {
    if !angle.is_finite() || !height.is_finite() || angle.abs() < 1e-9 {
        return Err(KernelError::InvalidInput("a helical revolve needs an angle and a height".into()));
    }
    let d = axis.dir.try_normalize().ok_or_else(|| KernelError::InvalidInput("the axis has no direction".into()))?;
    let o = axis.origin;
    let v = |p: &[f32; 3]| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
    // The outline (its edges' polylines; the triangles' corners if it has none) for its reach along and off the axis.
    let outline: Vec<DVec3> =
        if mesh.edges.is_empty() { mesh.positions.iter().map(v).collect() } else { mesh.edges.iter().flatten().map(v).collect() };
    if outline.is_empty() {
        return Err(KernelError::InvalidInput("the profile is empty".into()));
    }
    let along = |q: DVec3| (q - o).dot(d);
    let (mut lo, mut hi, mut rmin) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY);
    for q in &outline {
        let h = along(*q);
        lo = lo.min(h);
        hi = hi.max(h);
        rmin = rmin.min(q.distance(o + d * h));
    }
    if rmin < 1e-6 {
        return Err(refuse("a helical revolve needs the profile clear of the axis"));
    }
    let turns = angle.abs() / std::f64::consts::TAU;
    if turns > 1.0 && height.abs() / turns < (hi - lo) - 1e-6 {
        return Err(refuse("the turns overlap: make the height larger, or the angle smaller"));
    }
    // The profile's middle, from its triangles weighted by area.
    let (mut area, mut sum) = (0.0, DVec3::ZERO);
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.positions.get(i as usize).map_or(DVec3::ZERO, v));
        let w = (b - a).cross(c - a).length() / 2.0;
        area += w;
        sum += (a + b + c) / 3.0 * w;
    }
    let middle = if area > 1e-12 { sum / area } else { outline.iter().sum::<DVec3>() / outline.len() as f64 };
    let foot = o + d * along(middle);
    let radial = middle - foot;
    let (x, r) = match radial.try_normalize() {
        Some(x) if radial.length() > 1e-6 => (x, radial.length()),
        _ => (d.any_orthonormal_vector(), 1.0),
    };
    let y = d.cross(x);
    // 24 points a turn, the tangent being how the helix runs with the angle (flipped when it turns backwards).
    let n = ((turns * 24.0).ceil() as usize + 1).clamp(9, 100_000);
    let way = angle.signum();
    Ok((0..n)
        .map(|k| {
            let th = angle * k as f64 / (n - 1) as f64;
            let p = foot + (x * th.cos() + y * th.sin()) * r + d * (height * th / angle);
            let t = ((y * th.cos() - x * th.sin()) * r + d * (height / angle)) * way;
            (p, t)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(at: f64) -> Mesh {
        // A 2 × 2 square in the XZ plane, centred `at` along X.
        let p = |x: f64, z: f64| [(at + x) as f32, 0.0, z as f32];
        let corners = [p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)];
        Mesh {
            positions: corners.to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            edges: vec![vec![corners[0], corners[1], corners[2], corners[3], corners[0]]],
            ..Mesh::default()
        }
    }

    #[test]
    fn the_path_rises_and_turns_round_the_middle() {
        let axis = Axis::new(DVec3::ZERO, DVec3::Z);
        let path = path(&square(5.0), axis, std::f64::consts::TAU * 2.0, 8.0).unwrap();
        let (first, _) = path[0];
        let (last, _) = *path.last().unwrap();
        assert!(first.distance(DVec3::new(5.0, 0.0, 0.0)) < 1e-9);
        assert!(last.distance(DVec3::new(5.0, 0.0, 8.0)) < 1e-9, "{last}");
        assert!(path.iter().all(|(p, _)| ((p.x * p.x + p.y * p.y).sqrt() - 5.0).abs() < 1e-9));
    }

    #[test]
    fn overlapping_turns_and_a_profile_on_the_axis_are_refused() {
        let axis = Axis::new(DVec3::ZERO, DVec3::Z);
        assert!(path(&square(5.0), axis, std::f64::consts::TAU * 3.0, 3.0).is_err(), "2 mm tall, 1 mm a turn");
        assert!(path(&square(1.0), axis, std::f64::consts::TAU, 3.0).is_err(), "touches the axis");
    }
}

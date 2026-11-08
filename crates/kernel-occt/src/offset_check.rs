//! Whether an Offset Face result is believable. OCCT can come back with a valid but runaway solid (where two
//! drafted halves meet at a ridge, say), so every attempt is checked: pushing faces out only adds material, never
//! reaches back behind the body along a pushed flat face's normal nor past the face's new place; pulling them in
//! only takes material away and stays inside the old extent.

use shapecraft_kernel::{Aabb, DVec3};

/// `before` and `after`: (volume, tight bounds). `levels`: each moved face's outward normal and level when it lies
/// on an analytic plane.
pub(crate) fn plausible(before: (f64, Aabb), levels: &[Option<(DVec3, f64)>], distance: f64, after: (f64, Aabb)) -> bool {
    let ((v0, a), (v1, b)) = (before, after);
    // Out adds material, in takes it away (a NaN volume is neither).
    let right_way = if distance > 0.0 { v1 > v0 } else { v1 < v0 };
    if !right_way {
        return false;
    }
    let tol = 1e-6 * (a.max - a.min).length() + 1e-7;
    if distance < 0.0 {
        return (b.min - a.min).min_element() >= -tol && (a.max - b.max).min_element() >= -tol;
    }
    // The lowest any corner of a box reaches along `n`.
    let lowest = |bb: &Aabb, n: DVec3| {
        (0..8)
            .map(|i| {
                let c = DVec3::new(
                    if i & 1 != 0 { bb.max.x } else { bb.min.x },
                    if i & 2 != 0 { bb.max.y } else { bb.min.y },
                    if i & 4 != 0 { bb.max.z } else { bb.min.z },
                );
                c.dot(n)
            })
            .fold(f64::INFINITY, f64::min)
    };
    levels.iter().flatten().all(|&(n, h)| {
        // Nothing behind the body, and nothing past the face's new place unless the body already reached further.
        !(lowest(&b, n) < lowest(&a, n) - tol || -lowest(&b, -n) > (-lowest(&a, -n)).max(h + distance) + tol)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(min: [f64; 3], max: [f64; 3]) -> Aabb {
        Aabb { min: DVec3::from_array(min), max: DVec3::from_array(max) }
    }

    #[test]
    fn pushing_a_top_face_up_is_believable_and_a_runaway_is_not() {
        let before = (1000.0, bx([0.0; 3], [10.0; 3]));
        let top = [Some((DVec3::Z, 10.0))];
        assert!(plausible(before, &top, 5.0, (1500.0, bx([0.0; 3], [10.0, 10.0, 15.0]))));
        // Reached below the body's bottom: not believable.
        assert!(!plausible(before, &top, 5.0, (1600.0, bx([0.0, 0.0, -1.0], [10.0, 10.0, 15.0]))));
        // Past the face's new place.
        assert!(!plausible(before, &top, 5.0, (1700.0, bx([0.0; 3], [10.0, 10.0, 17.0]))));
        // Pushed out but lost material.
        assert!(!plausible(before, &top, 5.0, (900.0, bx([0.0; 3], [10.0, 10.0, 15.0]))));
        // Pulled in: less material, inside the old box.
        assert!(plausible(before, &top, -2.0, (800.0, bx([0.0; 3], [10.0, 10.0, 8.0]))));
        assert!(!plausible(before, &top, -2.0, (800.0, bx([0.0; 3], [10.0, 10.0, 11.0]))));
    }
}

//! Single-start, 60-degree modeled threads. Dimensions are basic profiles with flat roots;
//! manufacturing tolerance classes and rounded root forms are not implied.

use crate::{Axis, BoolOp, CylinderFace, Kernel, KernelError, PlanarCurve, PlaneFrame, Result, Shape};

#[derive(Clone, Copy, Debug)]
pub struct ThreadSpec {
    pub diameter: f64,
    pub pitch: f64,
    pub length: f64,
    pub offset: f64,
    /// Radial allowance: shrinks an external thread, enlarges an internal one.
    pub clearance: f64,
    pub internal: bool,
    pub left_hand: bool,
    pub reverse: bool,
}

fn invalid(message: &str) -> KernelError {
    KernelError::InvalidInput(message.into())
}

impl ThreadSpec {
    pub fn validate(self, wall: CylinderFace) -> Result<()> {
        if ![
            self.diameter,
            self.pitch,
            self.length,
            self.offset,
            self.clearance,
            wall.radius,
            wall.length,
            wall.axis.origin.x,
            wall.axis.origin.y,
            wall.axis.origin.z,
            wall.axis.dir.x,
            wall.axis.dir.y,
            wall.axis.dir.z,
        ]
        .iter()
        .all(|x| x.is_finite())
        {
            return Err(invalid("thread dimensions must be finite"));
        }
        if !(0.1..=1000.0).contains(&self.diameter)
            || !(0.05..=20.0).contains(&self.pitch)
            || self.pitch >= self.diameter / 2.0
            || self.length <= 1e-5
            || self.length > 10000.0
            || self.offset < 0.0
            || self.offset + self.length > wall.length + 1e-6
            || self.length / self.pitch > 100.0
            || self.clearance < 0.0
            || self.clearance > self.pitch / 4.0
            || wall.radius <= 0.0
            || wall.axis.dir.length_squared() < 1e-12
        {
            return Err(invalid(
                "thread needs a positive length within the face, at most 100 turns, and radial clearance from 0 to pitch/4",
            ));
        }
        if self.internal != wall.internal {
            return Err(invalid("choose Internal for a hole wall or External for a shaft wall"));
        }
        let actual = wall.radius * 2.0;
        let tolerance = (self.pitch * 0.1).max(0.02);
        if self.internal {
            let minor = self.diameter - 1.0825317547305482 * self.pitch;
            // Accommodate the usual D - P tapping hole, but never a clearance hole.
            if actual < minor - tolerance || actual > minor + self.pitch * 0.25 {
                return Err(invalid("hole diameter does not match this preset; use its tapping-hole diameter"));
            }
        } else if (actual - self.diameter).abs() > tolerance {
            return Err(invalid("shaft diameter does not match this preset's nominal diameter"));
        }
        Ok(())
    }
}

/// Cuts a thread into the selected cylinder, confined to its radius and the requested axial span.
/// All geometry goes through the replaceable kernel interface.
pub fn cut(kernel: &dyn Kernel, body: &Shape, wall: CylinderFace, spec: ThreadSpec) -> Result<Shape> {
    spec.validate(wall)?;
    let mut axis =
        Axis::new(wall.axis.origin, wall.axis.dir.try_normalize().ok_or_else(|| invalid("thread axis has no finite direction"))?);
    if spec.reverse {
        axis.origin += axis.dir * wall.length;
        axis.dir = -axis.dir;
    }
    axis.origin += axis.dir * spec.offset;
    let x = axis.dir.any_orthonormal_vector();
    let p = spec.pitch;
    let h = 3.0_f64.sqrt() * p / 2.0;
    let major = spec.diameter / 2.0 + if spec.internal { spec.clearance } else { -spec.clearance };
    let epsilon = p * 0.01;
    // The extra turn at each end gives complete threads at the clipping planes.
    let (inner, outer, inner_half, outer_half) = if spec.internal {
        let minor = major - 5.0 * h / 8.0;
        (minor - epsilon, major, 3.0 * p / 8.0 + epsilon / 3.0_f64.sqrt(), p / 16.0)
    } else {
        // Flat root at the ISO external minor diameter (17 H / 24 radial depth).
        let root = major - 17.0 * h / 24.0;
        (root, major + epsilon, p / 12.0, 7.0 * p / 16.0 + epsilon / 3.0_f64.sqrt())
    };
    let point = |r: f64, z: f64| axis.origin + x * r + axis.dir * (z - p);
    let points = [point(inner, -inner_half), point(outer, -outer_half), point(outer, outer_half), point(inner, inner_half)];
    let curves: Vec<_> = (0..4).map(|i| PlanarCurve::Line { a: points[i], b: points[(i + 1) % 4] }).collect();
    let frame = PlaneFrame { origin: axis.origin, x, y: axis.dir };
    let profile = kernel.planar_regions(frame, &curves)?.into_iter().next().ok_or_else(|| invalid("thread profile could not be built"))?;
    let travel = spec.length + 2.0 * p;
    let angle = std::f64::consts::TAU * travel / p * if spec.left_hand { -1.0 } else { 1.0 };
    let helix = kernel.revolve_helix(&profile, axis, angle, travel)?;
    let span = axis.dir * spec.length;
    let envelope = kernel.extrude_circle(axis.origin, outer.max(wall.radius) + epsilon, span)?;
    let groove = kernel.boolean(BoolOp::Intersect, &helix, &envelope)?;
    let mut result = kernel.boolean(BoolOp::Subtract, body, &groove)?;
    if spec.internal {
        // Bring a slightly undersize pilot bore up to the basic minor diameter and apply clearance to its crests.
        let bore = kernel.extrude_circle(axis.origin, major - 5.0 * h / 8.0, span)?;
        result = kernel.boolean(BoolOp::Subtract, &result, &bore)?;
    } else if major < wall.radius - 1e-7 {
        // Clearance applies to the crests as well as the flanks, within this axial span only.
        let core = kernel.extrude_circle(axis.origin, major, span)?;
        let sleeve = kernel.boolean(BoolOp::Subtract, &envelope, &core)?;
        result = kernel.boolean(BoolOp::Subtract, &result, &sleeve)?;
    }
    if kernel.solids(&result)?.len() != 1 {
        return Err(invalid("thread would split the body; reduce its depth or choose another face"));
    }
    let before = kernel.mass_props(body)?.volume.abs();
    let after = kernel.mass_props(&result)?.volume.abs();
    if !after.is_finite() || after <= 1e-9 || after >= before - 1e-8 {
        return Err(invalid("thread did not remove material from the selected face"));
    }
    Ok(result)
}

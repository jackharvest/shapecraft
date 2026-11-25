//! Solids skinned over closed sections: a wall patch per profile edge, carried smoothly from section to section, with
//! guide curves to follow, a direction (and curvature) to leave the ends in, or closed into a ring. Pure geometry; the
//! result is a neutral B-rep any kernel can read.

use glam::DVec3;

use crate::EdgeCurve;
use crate::brep::{BodyKind, Brep, Curve, Edge, Face, Fin, Nurbs, NurbsSurface, Surface};

/// One edge of a section, evaluated 0 to 1 the way the section runs.
#[derive(Clone, Debug)]
pub struct Run {
    shape: RunShape,
    reversed: bool,
}

#[derive(Clone, Debug)]
enum RunShape {
    Line(DVec3, DVec3),
    /// Center, two vectors a quarter turn apart, the angle run through.
    Arc(DVec3, DVec3, DVec3, f64),
    Fit(Box<crate::spline::Fit>),
}

impl Run {
    pub fn new(c: &EdgeCurve) -> Option<Run> {
        let shape = match c {
            EdgeCurve::Line { a, b } => RunShape::Line(*a, *b),
            EdgeCurve::Circle { center, normal, radius } => {
                let n = normal.try_normalize()?;
                let x = n.any_orthonormal_vector() * *radius;
                RunShape::Arc(*center, x, n.cross(x), std::f64::consts::TAU)
            }
            EdgeCurve::Arc { a, mid, b } => {
                let (u, v) = (*mid - *a, *b - *a);
                let n = u.cross(v);
                let nn = n.length_squared();
                if nn < 1e-24 {
                    RunShape::Line(*a, *b)
                } else {
                    let c = *a + (u.length_squared() * v.cross(n) + v.length_squared() * n.cross(u)) / (2.0 * nn);
                    let x = *a - c;
                    let y = n.normalize().cross(x);
                    let r = *b - c;
                    let mut sweep = r.dot(y).atan2(r.dot(x));
                    if sweep <= 0.0 {
                        sweep += std::f64::consts::TAU;
                    }
                    RunShape::Arc(c, x, y, sweep)
                }
            }
            EdgeCurve::Other { points } => {
                let n = points.len();
                let params = (0..n).map(|i| i as f64 / (n.max(2) - 1) as f64).collect();
                RunShape::Fit(Box::new(crate::spline::fit_with(points, params)?))
            }
        };
        Some(Run { shape, reversed: false })
    }

    pub fn at(&self, t: f64) -> DVec3 {
        let t = if self.reversed { 1.0 - t } else { t };
        match &self.shape {
            RunShape::Line(a, b) => a.lerp(*b, t),
            RunShape::Arc(c, x, y, s) => *c + *x * (s * t).cos() + *y * (s * t).sin(),
            RunShape::Fit(f) => f.point(t),
        }
    }

    pub fn flipped(&self) -> Run {
        Run { reversed: !self.reversed, ..self.clone() }
    }

    fn closed(&self) -> bool {
        self.at(0.0).distance(self.at(1.0)) < 1e-7
    }

    fn length(&self) -> f64 {
        const N: usize = 64;
        (0..N).map(|i| self.at(i as f64 / N as f64).distance(self.at((i + 1) as f64 / N as f64))).sum()
    }
}

/// A closed chain of edges in order: the largest loop among `curves` (inner ones are ignored).
pub fn outer_loop(curves: &[EdgeCurve]) -> Option<Vec<Run>> {
    let mut left: Vec<Run> = curves.iter().filter_map(Run::new).collect();
    let mut best: Option<(f64, Vec<Run>)> = None;
    while let Some(first) = left.pop() {
        let mut chain = vec![first];
        if !chain[0].closed() {
            loop {
                let end = chain[chain.len() - 1].at(1.0);
                if end.distance(chain[0].at(0.0)) < 1e-4 && chain.len() > 1 {
                    break;
                }
                let next = left
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (i, r.at(0.0).distance(end), r.at(1.0).distance(end)))
                    .min_by(|a, b| a.1.min(a.2).total_cmp(&b.1.min(b.2)))
                    .filter(|(_, a, b)| a.min(*b) < 1e-4)?;
                let run = left.remove(next.0);
                chain.push(if next.1 <= next.2 { run } else { run.flipped() });
            }
        }
        let pts: Vec<DVec3> = chain.iter().flat_map(|r| (0..32).map(move |i| r.at(f64::from(i) / 32.0))).collect();
        let size = area(&pts).length();
        if best.as_ref().is_none_or(|(a, _)| size > *a) {
            best = Some((size, chain));
        }
    }
    best.map(|(_, c)| c)
}

/// The vector area of a closed polygon (its normal, its length the area).
fn area(pts: &[DVec3]) -> DVec3 {
    let Some(&o) = pts.first() else { return DVec3::ZERO };
    pts.windows(2).map(|w| (w[0] - o).cross(w[1] - o)).sum::<DVec3>() / 2.0
}

/// A section: a closed chain of edges, where its corners are kept.
struct Ring {
    runs: Vec<Run>,
    /// Where each run ends, as a fraction of the way round (the last is 1).
    ends: Vec<f64>,
    center: DVec3,
    normal: DVec3,
    /// Added to a fraction before it's looked up: where the section "starts" for the others.
    start: f64,
}

impl Ring {
    fn new(runs: Vec<Run>) -> Ring {
        let lens: Vec<f64> = runs.iter().map(Run::length).collect();
        let total: f64 = lens.iter().sum::<f64>().max(1e-12);
        let mut at = 0.0;
        let mut ends: Vec<f64> = lens
            .iter()
            .map(|l| {
                at += l / total;
                at
            })
            .collect();
        if let Some(e) = ends.last_mut() {
            *e = 1.0;
        }
        let pts: Vec<DVec3> = runs.iter().flat_map(|r| (0..32).map(move |i| r.at(f64::from(i) / 32.0))).collect();
        let center = pts.iter().sum::<DVec3>() / pts.len().max(1) as f64;
        let normal = area(&pts).normalize_or(DVec3::Z);
        Ring { runs, ends, center, normal, start: 0.0 }
    }

    /// Which run a fraction of the way round falls in, and where in it. `after`: at a corner, the run that follows.
    fn place(&self, f: f64, after: bool) -> (usize, f64) {
        let f = f - f.floor();
        let n = self.runs.len();
        for i in 0..n {
            let lo = if i == 0 { 0.0 } else { self.ends[i - 1] };
            let hi = self.ends[i];
            let inside = if after { f < hi - 1e-12 } else { f <= hi + 1e-12 };
            if inside || i + 1 == n {
                return (i, ((f - lo) / (hi - lo).max(1e-15)).clamp(0.0, 1.0));
            }
        }
        (0, 0.0)
    }

    fn at(&self, f: f64) -> DVec3 {
        let (i, t) = self.place(f + self.start, true);
        self.runs[i].at(t)
    }

    fn reverse(&mut self) {
        self.runs = self.runs.iter().rev().map(Run::flipped).collect();
        *self = Ring { start: 0.0, ..Ring::new(std::mem::take(&mut self.runs)) };
    }

    /// Its corners, as fractions counted from the start, rising from 0 (none for one closed smooth edge).
    fn corners(&self) -> Vec<f64> {
        if self.runs.len() == 1 {
            return Vec::new();
        }
        let mut c: Vec<f64> = self.ends.iter().map(|e| wrap(e - self.start)).collect();
        c.sort_by(f64::total_cmp);
        c
    }

    /// The fraction (from the start) nearest a point, and how far off it is.
    fn nearest(&self, p: DVec3) -> (f64, f64) {
        const N: usize = 720;
        let d = |f: f64| self.at(f).distance(p);
        let mut best = (0..N).map(|i| i as f64 / N as f64).min_by(|a, b| d(*a).total_cmp(&d(*b))).unwrap_or(0.0);
        let mut h = 1.0 / N as f64;
        for _ in 0..40 {
            for c in [best - h, best + h] {
                if d(c) < d(best) {
                    best = c;
                }
            }
            h *= 0.6;
        }
        // A corner as near as makes no difference is the corner itself.
        for c in self.corners() {
            if d(c) < d(best) + 1e-7 {
                best = c;
            }
        }
        (wrap(best), d(best))
    }
}

/// Into 0..1 (a hair under 1 counts as 0).
fn wrap(f: f64) -> f64 {
    let f = f - f.floor();
    if f > 1.0 - 1e-9 { 0.0 } else { f }
}

/// A guide curve: points along it, close enough to straight between them.
pub type Guide = Vec<DVec3>;

/// How the walls leave an end: the direction at points round the section (with how they curve there, for G2), asked
/// for by position.
pub struct End<'a> {
    /// The wall's direction at a point of the section (any length) and its curvature vector there (zero when flat).
    pub leave: &'a dyn Fn(DVec3) -> (DVec3, DVec3),
    /// Matches the curvature too.
    pub curvature: bool,
    /// How strongly the direction holds (1 = as long as the gap to the next section).
    pub magnitude: f64,
}

#[derive(Default)]
pub struct Options<'a> {
    pub guides: Vec<Guide>,
    /// The last section runs on smoothly into the first.
    pub periodic: bool,
    pub start: Option<End<'a>>,
    pub end: Option<End<'a>>,
}

/// Dense LU for the small systems here.
struct Lu {
    n: usize,
    a: Vec<f64>,
    perm: Vec<usize>,
}

impl Lu {
    fn new(n: usize, mut a: Vec<f64>) -> Option<Lu> {
        let mut perm: Vec<usize> = (0..n).collect();
        for k in 0..n {
            let p = (k..n).max_by(|&i, &j| a[i * n + k].abs().total_cmp(&a[j * n + k].abs()))?;
            if a[p * n + k].abs() < 1e-14 {
                return None;
            }
            if p != k {
                for c in 0..n {
                    a.swap(p * n + c, k * n + c);
                }
                perm.swap(p, k);
            }
            for i in k + 1..n {
                let f = a[i * n + k] / a[k * n + k];
                a[i * n + k] = f;
                if f != 0.0 {
                    for c in k + 1..n {
                        a[i * n + c] -= f * a[k * n + c];
                    }
                }
            }
        }
        Some(Lu { n, a, perm })
    }

    fn solve(&self, b: &[DVec3]) -> Vec<DVec3> {
        let n = self.n;
        let mut x: Vec<DVec3> = self.perm.iter().map(|&i| b[i]).collect();
        for i in 0..n {
            for k in 0..i {
                let v = x[k];
                x[i] -= v * self.a[i * n + k];
            }
        }
        for i in (0..n).rev() {
            for k in i + 1..n {
                let v = x[k];
                x[i] -= v * self.a[i * n + k];
            }
            x[i] /= self.a[i * n + i];
        }
        x
    }
}

/// What pins a cubic spline's two ends besides its points.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Pin {
    /// A first derivative given.
    Slope,
    /// No bending.
    Free,
}

/// A C2 cubic spline through values at rising parameters: the solver for its B-spline poles (n + 2 of them).
pub(crate) struct Cubic {
    params: Vec<f64>,
    full: Vec<f64>,
    lu: Lu,
    pins: [Pin; 2],
}

impl Cubic {
    pub(crate) fn new(params: Vec<f64>, pins: [Pin; 2]) -> Option<Cubic> {
        let n = params.len();
        let (lo, hi) = (*params.first()?, *params.last()?);
        let mut full = vec![lo; 4];
        full.extend(&params[1..n - 1]);
        full.extend([hi; 4]);
        let m = n + 2;
        let mut a = vec![0.0; m * m];
        // Rows: the first value, the first pin, the inner values, the last pin, the last value.
        a[0] = 1.0;
        a[m * m - 1] = 1.0;
        for (i, &t) in params.iter().enumerate().take(n - 1).skip(1) {
            let b = crate::nurbs::basis_all(&full, 3, t);
            a[(i + 1) * m..(i + 2) * m].copy_from_slice(&b[..m]);
        }
        for (pin, row, at_end) in [(pins[0], 1, false), (pins[1], m - 2, true)] {
            let d = derivative_row(&full, m, if at_end { hi } else { lo }, if pin == Pin::Slope { 1 } else { 2 }, at_end);
            a[row * m..(row + 1) * m].copy_from_slice(&d);
        }
        Some(Cubic { lu: Lu::new(m, a)?, params, full, pins })
    }

    /// The poles through `values`, with the end slopes where they're asked for.
    pub(crate) fn poles(&self, values: &[DVec3], slopes: [DVec3; 2]) -> Vec<DVec3> {
        let n = values.len();
        let mut b = Vec::with_capacity(n + 2);
        b.push(values[0]);
        b.push(if self.pins[0] == Pin::Slope { slopes[0] } else { DVec3::ZERO });
        b.extend(&values[1..n - 1]);
        b.push(if self.pins[1] == Pin::Slope { slopes[1] } else { DVec3::ZERO });
        b.push(values[n - 1]);
        self.lu.solve(&b)
    }

    fn point(&self, poles: &[DVec3], t: f64) -> DVec3 {
        crate::nurbs::basis_all(&self.full, 3, t).iter().zip(poles).map(|(b, p)| *p * *b).sum()
    }

    /// First and second derivatives at `t`.
    fn slopes(&self, poles: &[DVec3], t: f64) -> (DVec3, DVec3) {
        let m = poles.len();
        let at_end = t >= self.params[self.params.len() - 1] - 1e-15;
        let d = |k| derivative_row(&self.full, m, t, k, at_end).iter().zip(poles).map(|(b, p)| *p * *b).sum();
        (d(1), d(2))
    }
}

/// The `order`th derivatives of every cubic basis function at `t`.
fn derivative_row(full: &[f64], m: usize, t: f64, order: usize, _at_end: bool) -> Vec<f64> {
    // Derivatives by lowering the degree: N'(i,p) = p·(N(i,p−1)/(k(i+p)−k(i)) − N(i+1,p−1)/(k(i+p+1)−k(i+1))).
    let p = 3;
    let low = p - order;
    let hi = full[full.len() - 1];
    let mut b: Vec<f64> = (0..full.len() - 1)
        .map(|i| {
            let inside = full[i] <= t && t < full[i + 1];
            let last = t >= hi && full[i] < full[i + 1] && full[i + 1] >= hi;
            if inside || last { 1.0 } else { 0.0 }
        })
        .collect();
    let ratio = |n: f64, d: f64| if d.abs() < 1e-15 { 0.0 } else { n / d };
    for q in 1..=p {
        let len = full.len() - 1 - q;
        b = (0..len)
            .map(|i| {
                if q <= low {
                    ratio(t - full[i], full[i + q] - full[i]) * b[i] + ratio(full[i + q + 1] - t, full[i + q + 1] - full[i + 1]) * b[i + 1]
                } else {
                    q as f64 * (ratio(b[i], full[i + q] - full[i]) - ratio(b[i + 1], full[i + q + 1] - full[i + 1]))
                }
            })
            .collect();
    }
    b.truncate(m);
    b
}

/// How much each of two or more guides moves the wall at a fraction of the way round: between neighbours the one's share
/// falls as the other's rises, level at both.
fn spread(at: &[f64], f: f64) -> Vec<f64> {
    let n = at.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| at[i].total_cmp(&at[j]));
    let pos = |k: usize| at[order[k % n]] + (k / n) as f64;
    let g = {
        let f = wrap(f);
        if f < pos(0) { f + 1.0 } else { f }
    };
    let k = (0..n).rfind(|&k| pos(k) <= g + 1e-12).unwrap_or(0);
    let t = ((g - pos(k)) / (pos(k + 1) - pos(k)).max(1e-12)).clamp(0.0, 1.0);
    let mut w = vec![0.0; n];
    w[order[k % n]] += 1.0 - t * t * (3.0 - 2.0 * t);
    w[order[(k + 1) % n]] += t * t * (3.0 - 2.0 * t);
    w
}

/// A rising map of the way round: `from[i]` to `to[i]` (both rising, starting at 0 and wrapping at 1), smooth at them.
struct Remap {
    from: Vec<f64>,
    to: Vec<f64>,
    slope: Vec<f64>,
}

impl Remap {
    fn new(from: &[f64], to: &[f64]) -> Remap {
        let mut from = from.to_vec();
        let mut to = to.to_vec();
        from.push(from[0] + 1.0);
        to.push(to[0] + 1.0);
        let n = from.len() - 1;
        let chord: Vec<f64> = (0..n).map(|i| (to[i + 1] - to[i]) / (from[i + 1] - from[i]).max(1e-12)).collect();
        let slope = (0..=n)
            .map(|i| {
                let (a, b) = (chord[(i + n - 1) % n], chord[i % n]);
                if a * b <= 0.0 { 0.0 } else { 2.0 * a * b / (a + b) }
            })
            .collect();
        Remap { from, to, slope }
    }

    fn at(&self, f: f64) -> f64 {
        let n = self.from.len() - 1;
        let mut f = f;
        while f < self.from[0] {
            f += 1.0;
        }
        while f > self.from[n] {
            f -= 1.0;
        }
        let i = (0..n).find(|&i| f <= self.from[i + 1]).unwrap_or(n - 1);
        let h = (self.from[i + 1] - self.from[i]).max(1e-12);
        let t = (f - self.from[i]) / h;
        let (u, v) = (1.0 - t, t);
        (1.0 + 2.0 * t) * u * u * self.to[i]
            + t * u * u * h * self.slope[i]
            + v * v * (3.0 - 2.0 * t) * self.to[i + 1]
            + v * v * (t - 1.0) * h * self.slope[i + 1]
    }
}

/// The turn that takes direction `a` to `b` the short way.
fn turn(a: DVec3, b: DVec3) -> glam::DMat3 {
    if a.dot(b) < -1.0 + 1e-9 {
        return glam::DMat3::from_axis_angle(a.any_orthonormal_vector(), std::f64::consts::PI);
    }
    glam::DMat3::from_quat(glam::DQuat::from_rotation_arc(a, b))
}

/// Guide points that touch each section, in order along the guide: where (fraction of the section, parameter on it).
fn touches(rings: &[Ring], guide: &Guide, tol: f64) -> Result<Vec<(f64, f64)>, &'static str> {
    const OFF: &str = "a guide curve doesn't touch every profile";
    let lens: Vec<f64> = guide.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f64 = lens.iter().sum();
    if total < 1e-9 {
        return Err(OFF);
    }
    rings
        .iter()
        .map(|r| {
            // Where the guide crosses the section's plane, nearest the section.
            let side = |p: DVec3| (p - r.center).dot(r.normal);
            let mut run = 0.0;
            let mut best: Option<(f64, f64, f64)> = None;
            for (w, l) in guide.windows(2).zip(&lens) {
                let (a, b) = (side(w[0]), side(w[1]));
                let mut hits = Vec::new();
                if a.abs() <= tol {
                    hits.push(0.0);
                }
                if b.abs() <= tol {
                    hits.push(1.0);
                }
                if a * b < 0.0 {
                    hits.push(a / (a - b));
                }
                for t in hits {
                    let p = w[0].lerp(w[1], t);
                    let (f, d) = r.nearest(p);
                    if best.is_none_or(|(x, ..)| d < x) {
                        best = Some((d, f, (run + l * t) / total));
                    }
                }
                run += l;
            }
            best.filter(|b| b.0 <= tol).map(|(_, f, t)| (f, t)).ok_or(OFF)
        })
        .collect()
}

fn along(guide: &Guide, t: f64) -> DVec3 {
    let total: f64 = guide.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = t.clamp(0.0, 1.0) * total;
    for w in guide.windows(2) {
        let l = w[0].distance(w[1]);
        if left <= l || l < 1e-15 && left <= 0.0 {
            return w[0].lerp(w[1], if l < 1e-15 { 0.0 } else { left / l });
        }
        left -= l;
    }
    guide.last().copied().unwrap_or(DVec3::ZERO)
}

/// Lines every section up with the first: the same way round, starting at matching places, corners on corners.
/// Returns, for each section, how to look up a common fraction, and the common fractions of the corners.
fn line_up(rings: &mut [Ring], hits: &[Vec<(f64, f64)>]) -> Result<(Vec<Remap>, Vec<f64>), &'static str> {
    // All of them the same way round, as seen along the loft.
    for i in 0..rings.len() {
        let ahead = if i + 1 < rings.len() { rings[i + 1].center - rings[i].center } else { rings[i].center - rings[i - 1].center };
        let facing = rings[i].normal.dot(ahead.normalize_or_zero());
        let flip = if facing.abs() > 0.05 {
            facing < 0.0
        } else if i > 0 {
            rings[i].normal.dot(rings[i - 1].normal) < 0.0
        } else {
            false
        };
        if flip {
            rings[i].reverse();
        }
    }
    let most = (0..rings.len()).max_by_key(|&i| (rings[i].corners().len(), rings.len() - i)).unwrap_or(0);
    let guided = !hits.is_empty();
    // Re-find guide touches after the reversals, as raw fractions.
    let hits: Vec<Vec<f64>> = hits.iter().map(|h| h.iter().zip(rings.iter()).map(|(&(f, _), r)| f + r.start).collect()).collect();
    // Where each starts: the first section's first corner (or guide), the others turned to match.
    rings[most].start = if guided {
        hits[0][most]
    } else if rings[most].runs.len() > 1 {
        0.0
    } else {
        rings[most].start
    };
    let base: Vec<f64> = rings[most].corners();
    let samples: Vec<f64> = if base.len() >= 3 { base.clone() } else { (0..24).map(|i| f64::from(i) / 24.0).collect() };
    let place = |r: &Ring, f: f64| r.at(f) - r.center;
    for step in [1i64, -1] {
        let mut i = most as i64 + step;
        while (0..rings.len() as i64).contains(&i) {
            let (me, before) = (i as usize, (i - step) as usize);
            if guided {
                rings[me].start = hits[0][me];
            } else {
                let rot = turn(rings[before].normal, rings[me].normal);
                let wanted: Vec<DVec3> = samples.iter().map(|&f| rot * place(&rings[before], f)).collect();
                let size = |v: &[DVec3]| (v.iter().map(|p| p.length_squared()).sum::<f64>() / v.len() as f64).sqrt().max(1e-9);
                let ws = size(&wanted);
                let corners: Vec<f64> = rings[me].corners();
                let tries: Vec<f64> = if corners.len() == base.len() && corners.len() > 1 {
                    corners.clone()
                } else {
                    (0..360).map(|k| f64::from(k) / 360.0).collect()
                };
                let same = corners.len() == base.len() && corners.len() > 1;
                let cost = |r: &Ring, f: f64| -> f64 {
                    let pts: Vec<DVec3> = if same {
                        let k = corners.iter().position(|c| (*c - f).abs() < 1e-9).unwrap_or(0);
                        (0..corners.len()).map(|j| place(r, corners[(j + k) % corners.len()])).collect()
                    } else {
                        samples.iter().map(|s| place(r, s + f)).collect()
                    };
                    let want: Vec<DVec3> =
                        if same { base.iter().map(|&f| rot * place(&rings[before], f)).collect() } else { wanted.clone() };
                    let ps = size(&pts);
                    pts.iter().zip(&want).map(|(p, w)| (*p / ps - *w / ws).length_squared()).sum()
                };
                let costs: Vec<(f64, f64)> = tries.iter().map(|&f| (f, cost(&rings[me], f))).collect();
                let best = costs.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map_or(0.0, |c| c.0);
                // Sections with corners of their own start at one of them, when there's one close by.
                let start = if same || corners.is_empty() {
                    best
                } else {
                    corners
                        .iter()
                        .copied()
                        .min_by(|a, b| {
                            let d = |c: f64| wrap(c - best).min(wrap(best - c));
                            d(*a).total_cmp(&d(*b))
                        })
                        .unwrap_or(best)
                };
                rings[me].start = wrap(rings[me].start + start);
            }
            i += step;
        }
    }
    // Pins: guide touches and corners, common fractions taken from the section with the most corners.
    let n = rings.len();
    let local = |r: &Ring, raw: f64| wrap(raw - r.start);
    let mut pins: Vec<Vec<f64>> = vec![Vec::new(); n];
    if guided {
        for h in &hits {
            for i in 0..n {
                pins[i].push(local(&rings[i], h[i]));
            }
        }
        // Guides meet the sections in the same order round each.
        let mut order: Vec<usize> = (0..hits.len()).collect();
        order.sort_by(|&a, &b| pins[most][a].total_cmp(&pins[most][b]));
        for p in &mut pins {
            let sorted: Vec<f64> = order.iter().map(|&k| p[k]).collect();
            if sorted.windows(2).any(|w| w[1] <= w[0] + 1e-9) {
                return Err("the guide curves cross each other");
            }
            *p = sorted;
        }
    } else {
        pins.iter_mut().for_each(|p| p.push(0.0));
    }
    // Corners with the same count between the same pins meet one to one; the rest fall where the lengths put them.
    let common = pins[most].clone();
    let corners_of: Vec<Vec<f64>> = rings.iter().map(Ring::corners).collect();
    let between = |cs: &[f64], lo: f64, hi: f64| -> Vec<f64> { cs.iter().copied().filter(|c| *c > lo + 1e-7 && *c < hi - 1e-7).collect() };
    let mut maps_from: Vec<Vec<f64>> = vec![Vec::new(); n];
    let mut maps_to: Vec<Vec<f64>> = vec![Vec::new(); n];
    let mut hard: Vec<f64> = Vec::new();
    for k in 0..common.len() {
        let (lo, hi) = (common[k], common.get(k + 1).copied().unwrap_or(common[0] + 1.0));
        let spans: Vec<(f64, f64)> = pins.iter().map(|p| (p[k], p.get(k + 1).copied().unwrap_or(p[0] + 1.0))).collect();
        let inner: Vec<Vec<f64>> = (0..n).map(|i| between(&corners_of[i], spans[i].0, spans[i].1)).collect();
        for i in 0..n {
            maps_from[i].push(lo);
            maps_to[i].push(spans[i].0);
        }
        if corners_of.iter().zip(&spans).any(|(cs, s)| cs.iter().any(|c| (c - s.0).abs() < 1e-7)) {
            hard.push(lo);
        }
        let most_here = (0..n).max_by_key(|&i| (inner[i].len(), usize::from(i == most))).unwrap_or(most);
        let scale = |i: usize, c: f64| (c - spans[i].0) / (spans[i].1 - spans[i].0).max(1e-12);
        let count = inner[most_here].len();
        for c in 0..count {
            let f = lo + scale(most_here, inner[most_here][c]) * (hi - lo);
            hard.push(f);
            for i in 0..n {
                let to = if inner[i].len() == count {
                    inner[i][c]
                } else {
                    spans[i].0 + scale(most_here, inner[most_here][c]) * (spans[i].1 - spans[i].0)
                };
                maps_from[i].push(f);
                maps_to[i].push(to);
            }
        }
        // Corners of a section with fewer: a crease where they fall.
        for (own, span) in inner.iter().zip(&spans) {
            if own.len() != count {
                hard.extend(own.iter().map(|c| lo + (c - span.0) / (span.1 - span.0).max(1e-12) * (hi - lo)));
            }
        }
    }
    hard.iter_mut().for_each(|h| *h = wrap(*h));
    hard.sort_by(f64::total_cmp);
    hard.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let maps = (0..n).map(|i| Remap::new(&maps_from[i], &maps_to[i])).collect();
    Ok((maps, hard))
}

/// The skin's parts at one fraction of the way round.
struct Skin<'a> {
    rings: &'a [Ring],
    maps: &'a [Remap],
    /// Each section's parameter along the loft.
    v: &'a [f64],
    opts: &'a Options<'a>,
    /// Each guide: its fraction round (common), and its parameter at each section.
    guides: Vec<(f64, Vec<f64>)>,
    /// Mean gaps between sections, for the end slopes.
    gaps: Vec<f64>,
}

impl Skin<'_> {
    fn section_points(&self, f: f64) -> Vec<DVec3> {
        self.rings.iter().zip(self.maps).map(|(r, m)| r.at(m.at(f))).collect()
    }

    /// Slope (and, asked for, curvature) at an end, in units of the loft's parameter.
    fn end(&self, e: &End, p: DVec3, last: bool) -> (DVec3, DVec3) {
        let (dir, curve) = (e.leave)(p);
        let n = self.v.len();
        let between = |a: usize, b: usize| self.rings[a].center.distance(self.rings[b].center);
        let (gap, dv) = if last { (between(n - 1, n - 2), self.v[n - 1] - self.v[n - 2]) } else { (between(0, 1), self.v[1] - self.v[0]) };
        let speed = e.magnitude * gap / dv;
        (dir.normalize_or_zero() * speed * if last { -1.0 } else { 1.0 }, curve * speed * speed)
    }

    /// The skin without guides, along the loft at one fraction: at any parameter.
    fn base(&self, f: f64) -> impl Fn(f64) -> DVec3 + use<'_> {
        let pts = self.section_points(f);
        let n = pts.len();
        let s = self.opts.start.as_ref().map(|e| self.end(e, pts[0], false));
        let e = self.opts.end.as_ref().map(|e| self.end(e, pts[n - 1], true));
        let g2 = [self.opts.start.as_ref().is_some_and(|e| e.curvature), self.opts.end.as_ref().is_some_and(|e| e.curvature)];
        let pin = |o: &Option<(DVec3, DVec3)>| if o.is_some() { Pin::Slope } else { Pin::Free };
        let spline = if self.opts.periodic {
            None
        } else {
            Cubic::new(self.v.to_vec(), [pin(&s), pin(&e)]).map(|c| {
                let poles = c.poles(&pts, [s.map_or(DVec3::ZERO, |x| x.0), e.map_or(DVec3::ZERO, |x| x.0)]);
                (c, poles)
            })
        };
        // A ring: the sections repeated each side, so the middle turn is smooth all round.
        let ring = self.opts.periodic.then(|| {
            let turn = self.v[n - 1] + self.gap_back();
            let mut params = Vec::new();
            let mut vals = Vec::new();
            for k in -2i32..=2 {
                for (v, p) in self.v.iter().zip(&pts) {
                    params.push(v + turn * f64::from(k));
                    vals.push(*p);
                }
            }
            Cubic::new(params, [Pin::Free, Pin::Free]).map(|c| {
                let poles = c.poles(&vals, [DVec3::ZERO; 2]);
                (c, poles)
            })
        });
        let v = self.v.to_vec();
        move |t: f64| {
            if let Some(Some((c, poles))) = &ring {
                return c.point(poles, t);
            }
            let Some((c, poles)) = &spline else { return pts[0] };
            // Curvature asked for at an end: that span becomes the quintic with the same far end.
            let first = t <= v[1] && g2[0] && s.is_some();
            let last = t >= v[n - 2] && g2[1] && e.is_some();
            if first || last {
                let (lo, hi) = if first || n == 2 { (v[0], v[1]) } else { (v[n - 2], v[n - 1]) };
                let h = hi - lo;
                let far = |t: f64| c.slopes(poles, t);
                let (d0, c0) = if (lo - v[0]).abs() < 1e-15 && g2[0] { s.unwrap_or_default() } else { far(lo) };
                let (d1, c1) = if (hi - v[n - 1]).abs() < 1e-15 && g2[1] { e.unwrap_or_default() } else { far(hi) };
                let (p0, p1) = (c.point(poles, lo), c.point(poles, hi));
                return quintic(p0, d0 * h, c0 * h * h, p1, d1 * h, c1 * h * h, (t - lo) / h);
            }
            c.point(poles, t)
        }
    }

    /// A ring's closing gap, from the last section back to the first.
    fn gap_back(&self) -> f64 {
        let n = self.v.len();
        let mean = self.v[n - 1] / self.gaps.iter().sum::<f64>().max(1e-12);
        self.rings[n - 1].center.distance(self.rings[0].center).max(self.gaps.iter().sum::<f64>() / self.gaps.len() as f64 * 0.1) * mean
    }

    /// The skin at `v`s, at one fraction: through the sections and, between them, along the guides.
    fn column(&self, f: f64, vs: &[f64]) -> Vec<DVec3> {
        let base = self.base(f);
        let mut out: Vec<DVec3> = vs.iter().map(|&t| base(t)).collect();
        if self.guides.is_empty() {
            return out;
        }
        let target = |k: usize, t: f64| -> DVec3 {
            let marks = &self.guides[k].1;
            let i = (0..self.v.len() - 1).rfind(|&i| self.v[i] <= t + 1e-12).unwrap_or(0);
            let s = ((t - self.v[i]) / (self.v[i + 1] - self.v[i]).max(1e-12)).clamp(0.0, 1.0);
            along(&self.opts.guides[k], marks[i] + (marks[i + 1] - marks[i]) * s)
        };
        // One guide: each section in between grows (and turns) about its middle until it touches.
        if let [(gf, _)] = self.guides.as_slice() {
            let own = self.base(*gf);
            let round: Vec<_> = (0..24).map(|k| self.base(f64::from(k) / 24.0)).collect();
            for (p, &t) in out.iter_mut().zip(vs) {
                let c = round.iter().map(|b| b(t)).sum::<DVec3>() / 24.0;
                let (from, to) = (own(t) - c, target(0, t) - c);
                let (a, b) = (from.length(), to.length());
                if a > 1e-9 && b > 1e-9 {
                    *p = c + turn(from / a, to / b) * (*p - c) * (b / a);
                }
            }
            return out;
        }
        let at: Vec<f64> = self.guides.iter().map(|g| g.0).collect();
        for (k, w) in spread(&at, f).into_iter().enumerate() {
            if w.abs() < 1e-12 {
                continue;
            }
            let own = self.base(self.guides[k].0);
            for (p, &t) in out.iter_mut().zip(vs) {
                *p += (target(k, t) - own(t)) * w;
            }
        }
        out
    }
}

fn quintic(p0: DVec3, d0: DVec3, c0: DVec3, p1: DVec3, d1: DVec3, c1: DVec3, t: f64) -> DVec3 {
    let (t2, t3, t4, t5) = (t * t, t.powi(3), t.powi(4), t.powi(5));
    p0 * (1.0 - 10.0 * t3 + 15.0 * t4 - 6.0 * t5)
        + d0 * (t - 6.0 * t3 + 8.0 * t4 - 3.0 * t5)
        + c0 * (0.5 * t2 - 1.5 * t3 + 1.5 * t4 - 0.5 * t5)
        + p1 * (10.0 * t3 - 15.0 * t4 + 6.0 * t5)
        + d1 * (-4.0 * t3 + 7.0 * t4 - 3.0 * t5)
        + c1 * (0.5 * t3 - t4 + 0.5 * t5)
}

/// The skinned solid. `sections` are closed chains of edges ([`outer_loop`]) in order.
pub fn solid(sections: Vec<Vec<Run>>, opts: &Options) -> Result<Brep, &'static str> {
    let n = sections.len();
    if n < 2 || opts.periodic && n < 3 {
        return Err(if opts.periodic { "a periodic loft needs at least three profiles" } else { "loft needs at least two profiles" });
    }
    if opts.periodic && (!opts.guides.is_empty() || opts.start.is_some() || opts.end.is_some()) {
        return Err("a periodic loft has no ends to set and takes no guide curves");
    }
    let mut rings: Vec<Ring> = sections.into_iter().map(Ring::new).collect();
    let size = rings
        .iter()
        .flat_map(|a| rings.iter().map(move |b| a.center.distance(b.center)))
        .fold(0.0, f64::max)
        .max(rings.iter().flat_map(|r| (0..8).map(move |i| r.at(f64::from(i) / 8.0).distance(r.center))).fold(0.0, f64::max));
    if !(size.is_finite() && size > 1e-6) {
        return Err("the profiles have no size");
    }
    // Points round each section, whole turns apart in a fixed place, before any reversal.
    let tol = (size * 2e-4).max(0.01);
    // The way round is settled first, so a guide's touches stay where they were found.
    line_up(&mut rings, &[])?;
    for r in &mut rings {
        r.start = 0.0;
    }
    let hits = opts.guides.iter().map(|g| touches(&rings, g, tol)).collect::<Result<Vec<_>, _>>()?;
    let (maps, hard) = line_up(&mut rings, &hits)?;
    // The loft's parameter: the mean gap between sections.
    let gap = |a: usize, b: usize| -> f64 {
        (0..24).map(|k| f64::from(k) / 24.0).map(|f| rings[a].at(maps[a].at(f)).distance(rings[b].at(maps[b].at(f)))).sum::<f64>() / 24.0
    };
    let gaps: Vec<f64> = (0..n - 1).map(|i| gap(i, i + 1)).collect();
    if gaps.iter().any(|g| *g < size * 1e-7) {
        return Err("two profiles lie in the same place");
    }
    let total: f64 = gaps.iter().sum();
    let mut v = vec![0.0];
    let mut at = 0.0;
    for g in &gaps {
        at += g / total;
        v.push(at);
    }
    v[n - 1] = 1.0;
    let guides: Vec<(f64, Vec<f64>)> = {
        // Guides sit at their common fraction: where the section with the most corners has them.
        let mut g = Vec::new();
        for h in &hits {
            let marks: Vec<f64> = h.iter().map(|x| x.1).collect();
            // The common fraction whose place on the first section is the touch (the maps are rising).
            let local = wrap(h[0].0 - rings[0].start);
            let mut best = (0.0, f64::MAX);
            for k in 0..2000 {
                let f = f64::from(k) / 2000.0;
                let d = wrap(maps[0].at(f) - local).min(wrap(local - maps[0].at(f)));
                if d < best.1 {
                    best = (f, d);
                }
            }
            let mut step = 1.0 / 2000.0;
            for _ in 0..40 {
                for f in [best.0 - step, best.0 + step] {
                    let d = wrap(maps[0].at(f) - local).min(wrap(local - maps[0].at(f)));
                    if d < best.1 {
                        best = (f, d);
                    }
                }
                step *= 0.6;
            }
            g.push((wrap(best.0), marks));
        }
        g
    };
    let skin = Skin { rings: &rings, maps: &maps, v: &v, opts, guides, gaps };
    build(&skin, &hard, size)
}

fn build(skin: &Skin, hard: &[f64], size: f64) -> Result<Brep, &'static str> {
    const WAY: &str = "the loft's walls couldn't be made";
    let n = skin.v.len();
    let opts = skin.opts;
    let plain =
        skin.guides.is_empty() && !opts.periodic && ![&opts.start, &opts.end].iter().any(|e| e.as_ref().is_some_and(|e| e.curvature));
    // Rows along the loft: the sections alone where a cubic spline through them is the whole story, else close rows.
    let turn = if opts.periodic { skin.v[n - 1] + skin.gap_back() } else { 1.0 };
    let mut stops: Vec<f64> = skin.v.to_vec();
    if opts.periodic {
        stops.push(turn);
    }
    let rows: Vec<f64> = if plain {
        stops.clone()
    } else {
        let per = if skin.guides.is_empty() { 8 } else { (96 / (stops.len() - 1)).clamp(12, 48) };
        let mut r = Vec::new();
        for w in stops.windows(2) {
            r.extend((0..per).map(|k| w[0] + (w[1] - w[0]) * k as f64 / per as f64));
        }
        r.push(turn);
        r
    };
    let slope_at = |f: f64, last: bool| -> Option<DVec3> {
        if plain {
            let e = if last { opts.end.as_ref() } else { opts.start.as_ref() }?;
            let pts = skin.section_points(f);
            return Some(skin.end(e, if last { pts[n - 1] } else { pts[0] }, last).0);
        }
        // Close rows: the slope the skin itself has there.
        let h = (rows[1] - rows[0]) * 0.05;
        let t = |k: f64| if last { turn - h * k } else { h * k };
        let c = skin.column(f, &[t(0.0), t(1.0), t(2.0), t(3.0), t(4.0)]);
        let d = (c[0] * -25.0 + c[1] * 48.0 - c[2] * 36.0 + c[3] * 16.0 - c[4] * 3.0) / (12.0 * h);
        Some(if last { -d } else { d })
    };
    let pins =
        if plain { [&opts.start, &opts.end].map(|e| if e.is_some() { Pin::Slope } else { Pin::Free }) } else { [Pin::Slope, Pin::Slope] };
    let vs = Cubic::new(rows.clone(), pins).ok_or(WAY)?;
    // Patches between corners, or one all the way round.
    let spans: Vec<(f64, f64)> = if hard.is_empty() {
        vec![(0.0, 1.0)]
    } else {
        hard.iter().enumerate().map(|(i, &a)| (a, hard.get(i + 1).copied().unwrap_or(hard[0] + 1.0))).collect()
    };
    let around = hard.is_empty();
    // A ring of faces all the way round its own hole is cut into two at sections, so each face has an outline.
    let cut = if opts.periodic { n / 2 * ((rows.len() - 1) / n) } else { 0 };
    let mut b = Brep { kind: BodyKind::Solid, ..Brep::default() };
    let levels: Vec<usize> = if opts.periodic { vec![0, cut] } else { vec![0, rows.len() - 1] };
    // Vertices at the corners of each level.
    let mut vertex = vec![vec![usize::MAX; spans.len()]; levels.len()];
    if !around {
        for (k, s) in spans.iter().enumerate() {
            let col = skin.column(s.0 + 1e-13, &rows);
            for (l, &row) in levels.iter().enumerate() {
                vertex[l][k] = b.vertices.len();
                b.vertices.push(col[row]);
            }
        }
    }
    let mut level_edges = vec![Vec::new(); levels.len()];
    // Per patch: its grid of poles.
    struct Patch {
        poles: Vec<DVec3>,
        nu: usize,
        u_knots: Vec<f64>,
    }
    let mut patches = Vec::new();
    for &(a, z) in &spans {
        let straight = !around
            && skin.guides.is_empty()
            && skin.rings.iter().zip(skin.maps).all(|(r, m)| {
                // Both ends and the middle in line, evenly.
                let p = |t: f64| r.at(m.at(a + (z - a) * t.clamp(1e-12, 1.0 - 1e-12)));
                (1..8).all(|k| p(f64::from(k) / 8.0).distance(p(0.0).lerp(p(1.0), f64::from(k) / 8.0)) < size * 1e-9)
            });
        let nu = if straight {
            2
        } else if around {
            73
        } else {
            25
        };
        let us: Vec<f64> = (0..nu).map(|k| k as f64 / (nu - 1) as f64).collect();
        // Never quite at the corner itself: the run on this side of it.
        let inside = |t: f64| if around { a + (z - a) * t } else { a + (z - a) * t.clamp(1e-12, 1.0 - 1e-12) };
        let column_poles = |f: f64| -> Vec<DVec3> {
            let col = skin.column(f, &rows);
            vs.poles(&col, [slope_at(f, false).unwrap_or_default(), slope_at(f, true).unwrap_or_default()])
        };
        let cols: Vec<Vec<DVec3>> = us.iter().map(|&t| column_poles(inside(t))).collect();
        let nv = cols[0].len();
        if straight {
            let mut poles = cols[0].clone();
            poles.extend(&cols[1]);
            patches.push(Patch { poles, nu: 2, u_knots: vec![0.0, 1.0] });
            continue;
        }
        let uc = Cubic::new(us.clone(), [Pin::Slope, Pin::Slope]).ok_or(WAY)?;
        let h = 2e-3 / (z - a).max(1e-9) * (z - a);
        let slope = |last: bool| -> Vec<DVec3> {
            let c: Vec<Vec<DVec3>> = (0..5)
                .map(|k| {
                    let t = if last { 1.0 - h * f64::from(k) } else { h * f64::from(k) };
                    column_poles(inside(t))
                })
                .collect();
            let one = (0..nv)
                .map(|j| (c[0][j] * -25.0 + c[1][j] * 48.0 - c[2][j] * 36.0 + c[3][j] * 16.0 - c[4][j] * 3.0) / (12.0 * h))
                .map(|d| if last { -d } else { d })
                .collect::<Vec<_>>();
            if !around {
                return one;
            }
            // Round a closed loop: the same slope both sides of the seam.
            let (p, m) = (column_poles(inside(h)), column_poles(inside(1.0 - h)));
            (0..nv).map(|j| (p[j] - m[j]) / (2.0 * h)).collect()
        };
        let (s0, s1) = (slope(false), slope(true));
        let mut grid = vec![DVec3::ZERO; (nu + 2) * nv];
        for j in 0..nv {
            let vals: Vec<DVec3> = cols.iter().map(|c| c[j]).collect();
            for (i, p) in uc.poles(&vals, [s0[j], s1[j]]).into_iter().enumerate() {
                grid[i * nv + j] = p;
            }
        }
        if around {
            for j in 0..nv {
                grid[(nu + 1) * nv + j] = grid[j];
            }
        }
        patches.push(Patch { poles: grid, nu: nu + 2, u_knots: us });
    }
    let nv = rows.len() + 2;
    if patches.iter().any(|p| p.poles.iter().any(|q| !q.is_finite() || q.length() > 1e7 + size * 1e3)) {
        return Err(WAY);
    }
    let v_knots = rows.clone();
    let mult = |n: usize| -> Vec<u32> { (0..n).map(|i| if i == 0 || i + 1 == n { 4 } else { 1 }).collect() };
    // The edges round each level: the profile's own curve where it is one, else the wall's.
    for (l, &row) in levels.iter().enumerate() {
        for (k, p) in patches.iter().enumerate() {
            let ud = if p.nu == 2 { 1 } else { 3 };
            let basis = crate::nurbs::basis_all(&vs.full, 3, rows[row]);
            let poles: Vec<DVec3> = (0..p.nu).map(|i| (0..nv).map(|j| p.poles[i * nv + j] * basis[j]).sum()).collect();
            let curve = Curve::BSpline(Nurbs {
                degree: ud,
                mults: if ud == 1 { vec![2, 2] } else { mult(p.u_knots.len()) },
                knots: p.u_knots.clone(),
                weights: None,
                periodic: false,
                poles: poles.clone(),
            });
            let exact = exact_curve(&poles, &p.u_knots, ud, size);
            let id = b.curves.len();
            b.curves.push(exact.unwrap_or(curve));
            level_edges[l].push(b.edges.len());
            let (start, end) = if around { (None, None) } else { (Some(vertex[l][k]), Some(vertex[l][(k + 1) % spans.len()])) };
            b.edges.push(Edge { curve: Some(id), reversed: false, start, end, tolerance: Some(1e-5 * size.max(1.0)) });
        }
    }
    // The edges up the corners, one per stretch of rows.
    let stretches: Vec<(usize, usize, usize, usize)> =
        if opts.periodic { vec![(0, cut, 0, 1), (cut, rows.len() - 1, 1, 0)] } else { vec![(0, rows.len() - 1, 0, 1)] };
    let sub = |poles: &[DVec3], lo: usize, hi: usize| -> (Vec<DVec3>, Vec<f64>) { stretch(poles, &rows, lo, hi) };
    let mut rails = vec![vec![usize::MAX; spans.len()]; stretches.len()];
    if !around {
        for (s, &(lo, hi, la, lb)) in stretches.iter().enumerate() {
            for k in 0..spans.len() {
                let (poles, knots) = sub(&patches[k].poles[..nv], lo, hi);
                let id = b.curves.len();
                let line = poles.iter().all(|p| off_line(*p, poles[0], poles[poles.len() - 1]) < size * 1e-10);
                b.curves.push(if line {
                    Curve::Line { origin: poles[0], dir: (poles[poles.len() - 1] - poles[0]).normalize_or(DVec3::Z) }
                } else {
                    Curve::BSpline(Nurbs { degree: 3, mults: mult(knots.len()), knots, weights: None, periodic: false, poles })
                });
                rails[s][k] = b.edges.len();
                b.edges.push(Edge {
                    curve: Some(id),
                    reversed: false,
                    start: Some(vertex[la][k]),
                    end: Some(vertex[lb][k]),
                    tolerance: None,
                });
            }
        }
    }
    let fin = |edge: usize, forward: bool| Fin { edge, forward, pcurve: None };
    for (s, &(lo, hi, la, lb)) in stretches.iter().enumerate() {
        for (k, p) in patches.iter().enumerate() {
            let mut poles = Vec::new();
            let mut knots = v_knots.clone();
            for i in 0..p.nu {
                let (q, kn) = sub(&p.poles[i * nv..(i + 1) * nv], lo, hi);
                poles.extend(q);
                knots = kn;
            }
            let pv = knots.len() + 2;
            let ud = if p.nu == 2 { 1 } else { 3 };
            let surf = NurbsSurface {
                u_degree: ud,
                v_degree: 3,
                nu: p.nu,
                nv: pv,
                poles,
                weights: None,
                u_mults: if ud == 1 { vec![2, 2] } else { mult(p.u_knots.len()) },
                u_knots: p.u_knots.clone(),
                v_mults: mult(knots.len()),
                v_knots: knots,
                // Closed but stored clamped: the reader gives it its seam.
                u_periodic: around,
                v_periodic: false,
            };
            let id = b.surfaces.len();
            b.surfaces.push(Surface::BSpline(surf));
            let loops = if around {
                vec![vec![fin(level_edges[la][k], true)], vec![fin(level_edges[lb][k], false)]]
            } else {
                let next = (k + 1) % spans.len();
                vec![vec![
                    fin(level_edges[la][k], true),
                    fin(rails[s][next], true),
                    fin(level_edges[lb][k], false),
                    fin(rails[s][k], false),
                ]]
            };
            b.faces.push(Face { surface: id, flip: false, loops, color: None });
        }
    }
    if !opts.periodic {
        for (l, ring, last) in [(0, 0, false), (1, n - 1, true)] {
            let normal = if last { skin.rings[ring].normal } else { -skin.rings[ring].normal };
            let id = b.surfaces.len();
            b.surfaces.push(Surface::Plane { origin: skin.rings[ring].center, normal, x: normal.any_orthonormal_vector() });
            let mut round: Vec<Fin> = level_edges[l].iter().map(|&e| fin(e, last)).collect();
            if !last {
                round.reverse();
            }
            b.faces.push(Face { surface: id, flip: false, loops: vec![round], color: None });
        }
    }
    b.shells = vec![(0..b.faces.len()).collect()];
    Ok(b)
}

fn off_line(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let d = (b - a).normalize_or_zero();
    ((p - a) - d * (p - a).dot(d)).length()
}

/// The line or circle a boundary really is, if it is one.
fn exact_curve(poles: &[DVec3], knots: &[f64], degree: u32, size: f64) -> Option<Curve> {
    let (a, z) = (poles[0], poles[poles.len() - 1]);
    if a.distance(z) > size * 1e-6 && poles.iter().all(|p| off_line(*p, a, z) < size * 1e-9) {
        return Some(Curve::Line { origin: a, dir: (z - a).normalize() });
    }
    if degree != 3 {
        return None;
    }
    let mut full = vec![knots[0]; 3];
    full.extend(knots);
    full.extend([knots[knots.len() - 1]; 3]);
    let at = |t: f64| -> DVec3 { crate::nurbs::basis_all(&full, 3, t).iter().zip(poles).map(|(b, p)| *p * *b).sum() };
    let pts: Vec<DVec3> = (0..=60).map(|k| at(f64::from(k) / 60.0)).collect();
    let (p, q, r) = (pts[0], pts[20], pts[40]);
    let (u, v) = (q - p, r - p);
    let n = u.cross(v);
    let nn = n.length_squared();
    if nn < (size * 1e-6).powi(4) {
        return None;
    }
    let c = p + (u.length_squared() * v.cross(n) + v.length_squared() * n.cross(u)) / (2.0 * nn);
    let radius = c.distance(p);
    let normal = n.normalize();
    let fits =
        radius < size * 1e3 && pts.iter().all(|x| (x.distance(c) - radius).abs() < size * 2e-6 && (*x - c).dot(normal).abs() < size * 2e-6);
    // The same way round as the boundary runs.
    fits.then(|| Curve::Circle { center: c, normal, x: (p - c).normalize(), radius })
}

/// The stretch of a clamped cubic spline (knots at `rows`) from row `lo` to row `hi`, as a clamped spline of its own.
fn stretch(poles: &[DVec3], rows: &[f64], lo: usize, hi: usize) -> (Vec<DVec3>, Vec<f64>) {
    if lo == 0 && hi == rows.len() - 1 {
        return (poles.to_vec(), rows.to_vec());
    }
    // Knots at the cut raised to full multiplicity, then the poles between.
    let mut full = vec![rows[0]; 3];
    full.extend(rows);
    full.extend([rows[rows.len() - 1]; 3]);
    let mut poles = poles.to_vec();
    let insert = |poles: &mut Vec<DVec3>, full: &mut Vec<f64>, t: f64| {
        let Some(k) = (0..full.len() - 1).rfind(|&i| full[i] <= t && t < full[i + 1]) else { return };
        let mut q = Vec::with_capacity(poles.len() + 1);
        for i in 0..=poles.len() {
            q.push(if i + 3 <= k {
                poles[i]
            } else if i > k {
                poles[i - 1]
            } else {
                let d = full[i + 3] - full[i];
                let a = if d.abs() < 1e-15 { 0.0 } else { (t - full[i]) / d };
                let left = if i > 0 { poles[i - 1] } else { poles[0] };
                let right = poles.get(i).copied().unwrap_or(left);
                left * (1.0 - a) + right * a
            });
        }
        full.insert(k + 1, t);
        *poles = q;
    };
    for row in [lo, hi] {
        if row != 0 && row != rows.len() - 1 {
            for _ in 0..2 {
                insert(&mut poles, &mut full, rows[row]);
            }
        }
    }
    let first = if lo == 0 { 0 } else { full.iter().position(|k| *k >= rows[lo]).unwrap_or(0).saturating_sub(1) };
    let count = hi - lo + 3;
    (poles[first..(first + count).min(poles.len())].to_vec(), rows[lo..=hi].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::dvec3;

    fn square(z: f64, h: f64) -> Vec<Run> {
        let p = [dvec3(-h, -h, z), dvec3(h, -h, z), dvec3(h, h, z), dvec3(-h, h, z)];
        let edges: Vec<EdgeCurve> = (0..4).map(|i| EdgeCurve::Line { a: p[i], b: p[(i + 1) % 4] }).collect();
        outer_loop(&edges).unwrap()
    }

    fn circle(z: f64, r: f64) -> Vec<Run> {
        outer_loop(&[EdgeCurve::Circle { center: dvec3(0.0, 0.0, z), normal: DVec3::Z, radius: r }]).unwrap()
    }

    #[test]
    fn a_cubic_passes_its_points_with_its_slopes() {
        let c = Cubic::new(vec![0.0, 0.3, 1.0], [Pin::Slope, Pin::Free]).unwrap();
        let vals = [dvec3(0.0, 0.0, 0.0), dvec3(1.0, 2.0, 0.0), dvec3(3.0, 1.0, 5.0)];
        let poles = c.poles(&vals, [dvec3(0.0, 4.0, 0.0), DVec3::ZERO]);
        for (t, v) in [0.0, 0.3, 1.0].into_iter().zip(vals) {
            assert!(c.point(&poles, t).distance(v) < 1e-12);
        }
        assert!(c.slopes(&poles, 0.0).0.distance(dvec3(0.0, 4.0, 0.0)) < 1e-9);
        assert!(c.slopes(&poles, 1.0).1.length() < 1e-8);
    }

    #[test]
    fn sections_line_up_corner_to_corner() {
        let b = solid(vec![square(0.0, 5.0), square(10.0, 3.0)], &Options::default()).unwrap();
        assert_eq!(b.faces.len(), 6);
        assert_eq!(b.vertices.len(), 8);
        // Round to square: four walls, the circle split where the corners go.
        let b = solid(vec![circle(0.0, 5.0), square(10.0, 3.0)], &Options::default()).unwrap();
        assert_eq!(b.faces.len(), 6);
        let b = solid(vec![circle(0.0, 5.0), circle(10.0, 3.0)], &Options::default()).unwrap();
        assert_eq!(b.faces.len(), 3);
        assert!(b.curves.iter().all(|c| matches!(c, Curve::Circle { .. })), "round rims stay circles");
    }

    #[test]
    fn a_guide_must_touch_every_profile() {
        let off = Options { guides: vec![vec![dvec3(9.0, 0.0, 0.0), dvec3(9.0, 0.0, 10.0)]], ..Options::default() };
        assert!(solid(vec![circle(0.0, 5.0), circle(10.0, 5.0)], &off).is_err());
    }
}

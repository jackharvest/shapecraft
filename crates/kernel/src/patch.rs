//! Smooth faces through a closed loop of sides (two or more), each met at its edge (G0), tangent to what's beside it (G1) or
//! curving with it too (G2). Points are blended from straight ways across the opening, each a polynomial that leaves both
//! sides as they ask; openings of five sides or more are cut into four-sided ones round a point in their middle.

use glam::DVec3;

use crate::brep::{BodyKind, Brep, Curve, Edge, Face, Fin, Nurbs, NurbsSurface, Surface};
use crate::skin::{Cubic, Pin, Run};

/// What lies beside a side: its surface's normal along the side, and how it curves.
#[derive(Clone, Debug)]
pub struct Bend {
    /// At even fractions along the side: the normal (either way), and the curvature along the side, across it, and mixed
    /// (towards the normal).
    samples: Vec<(DVec3, [f64; 3])>,
}

impl Bend {
    /// From samples at even fractions along the side (two or more).
    pub fn new(mut samples: Vec<(DVec3, [f64; 3])>) -> Option<Bend> {
        if samples.len() < 2 {
            return None;
        }
        // One way all along.
        for i in 1..samples.len() {
            if samples[i].0.dot(samples[i - 1].0) < 0.0 {
                samples[i].0 = -samples[i].0;
                samples[i].1 = samples[i].1.map(|k| -k);
            }
        }
        Some(Bend { samples })
    }

    fn at(&self, f: f64) -> (DVec3, [f64; 3]) {
        let n = self.samples.len() - 1;
        let x = f.clamp(0.0, 1.0) * n as f64;
        let i = (x as usize).min(n - 1);
        let t = x - i as f64;
        // Catmull-Rom between samples keeps the normals smooth along the side.
        let get = |k: i64| self.samples[k.clamp(0, n as i64) as usize];
        let (a, b, c, d) = (get(i as i64 - 1), get(i as i64), get(i as i64 + 1), get(i as i64 + 2));
        let (t2, t3) = (t * t, t * t * t);
        let w = [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2];
        let normal = a.0 * w[0] + b.0 * w[1] + c.0 * w[2] + d.0 * w[3];
        let k = std::array::from_fn(|j| b.1[j] + (c.1[j] - b.1[j]) * t);
        (normal.normalize_or(b.0), k)
    }
}

/// One side of an opening, run along evenly from 0 to 1.
#[derive(Clone, Debug)]
pub struct Rim {
    runs: Vec<(Run, Vec<f64>)>,
    /// Where each run ends, as a fraction of the side.
    ends: Vec<f64>,
    reversed: bool,
    /// A part of the side: its ends.
    part: (f64, f64),
    pub order: u8,
    pub bend: Option<Bend>,
    /// Which way from the side the opening is, on the surface beside it: along the normal crossed with the side (1) or
    /// against it (-1).
    inward: f64,
}

const TABLE: usize = 256;

impl Rim {
    pub fn new(runs: Vec<Run>, order: u8, bend: Option<Bend>) -> Option<Rim> {
        let mut total = 0.0;
        let mut out = Vec::new();
        let mut lens = Vec::new();
        for r in runs {
            let mut table = vec![0.0];
            let mut run = 0.0;
            let mut last = r.at(0.0);
            for k in 1..=TABLE {
                let p = r.at(k as f64 / TABLE as f64);
                run += p.distance(last);
                last = p;
                table.push(run);
            }
            if run < 1e-12 {
                continue;
            }
            table.iter_mut().for_each(|x| *x /= run);
            total += run;
            lens.push(total);
            out.push((r, table));
        }
        if out.is_empty() {
            return None;
        }
        Some(Rim {
            runs: out,
            ends: lens.into_iter().map(|l| l / total).collect(),
            reversed: false,
            part: (0.0, 1.0),
            order,
            bend,
            inward: 0.0,
        })
    }

    pub fn at(&self, f: f64) -> DVec3 {
        let f = self.whole(f);
        let i = self.ends.iter().position(|e| f <= *e).unwrap_or(self.ends.len() - 1);
        let lo = if i == 0 { 0.0 } else { self.ends[i - 1] };
        let x = ((f - lo) / (self.ends[i] - lo).max(1e-15)).clamp(0.0, 1.0);
        let (run, table) = &self.runs[i];
        let k = table.partition_point(|t| *t < x).clamp(1, TABLE);
        let (a, b) = (table[k - 1], table[k]);
        run.at((k as f64 - 1.0 + if b > a { (x - a) / (b - a) } else { 0.0 }) / TABLE as f64)
    }

    /// The fraction of the whole side a fraction of this part is.
    fn whole(&self, f: f64) -> f64 {
        let f = self.part.0 + (self.part.1 - self.part.0) * f.clamp(0.0, 1.0);
        if self.reversed { 1.0 - f } else { f }
    }

    pub fn flipped(&self) -> Rim {
        Rim { reversed: !self.reversed, part: (1.0 - self.part.1, 1.0 - self.part.0), ..self.clone() }
    }

    fn portion(&self, a: f64, z: f64) -> Rim {
        let (p, q) = self.part;
        Rim { part: (p + (q - p) * a, p + (q - p) * z), ..self.clone() }
    }

    fn tangent(&self, f: f64) -> DVec3 {
        let h = 1e-4;
        (self.at((f + h).min(1.0)) - self.at((f - h).max(0.0))).normalize_or_zero()
    }

    fn bend_at(&self, f: f64) -> Option<(DVec3, [f64; 3])> {
        self.bend.as_ref().filter(|_| self.order > 0).map(|b| b.at(self.whole(f)))
    }

    /// How a way across leaves (or reaches) the side in the direction `chord`: its slope there, and its second derivative.
    fn leave(&self, f: f64, chord: DVec3) -> (Option<DVec3>, Option<DVec3>) {
        let Some((n, [kt, kc, kx])) = self.bend_at(f) else { return (None, None) };
        let len = chord.length();
        if len < 1e-12 {
            return (Some(DVec3::ZERO), None);
        }
        // The way there, flattened onto the surface beside the side; what would leave straight off it (or the wrong way)
        // turns into the opening instead.
        let along = self.tangent(f);
        let mut over = n.cross(along);
        // (The side may be run either way; the sign was found the way it runs whole.)
        if self.reversed {
            over = -over;
        }
        if if self.inward == 0.0 { over.dot(chord) < 0.0 } else { self.inward < 0.0 } {
            over = -over;
        }
        let part = chord.dot(along);
        let (square, straight) = ((len * len - part * part).max(0.0).sqrt(), chord.dot(over));
        let dir = (along * part + over * (0.5 * (straight + straight.hypot(0.5 * square)))).normalize_or_zero();
        // As long as a circular arc through both ends would ask.
        let cos = dir.dot(chord / len).clamp(-0.5, 1.0);
        let slope = dir * len * 2.0 / (1.0 + cos);
        let second = (self.order >= 2).then(|| {
            let t = self.tangent(f);
            let (a, b) = (slope.dot(t), slope.dot(n.cross(t)));
            n * (kt * a * a + 2.0 * kx * a * b + kc * b * b)
        });
        (Some(slope), second)
    }
}

/// The polynomial from `p` to `q` with whatever slopes and second derivatives its ends ask for, at `t`.
fn across(p: DVec3, q: DVec3, start: (Option<DVec3>, Option<DVec3>), end: (Option<DVec3>, Option<DVec3>), t: f64) -> DVec3 {
    // Worked out as corrections to the line: each condition adds a term that leaves the earlier ones alone.
    let line = p.lerp(q, t);
    let run = q - p;
    let (d0, d1) = (start.0.map(|d| d - run), end.0.map(|d| d - run));
    let (c0, c1) = (start.1, end.1);
    let s = 1.0 - t;
    // Powers: at the start the term for the slope is t·s^k, where k leaves the far end's conditions alone.
    let far = |slope: bool, second: bool| {
        if second {
            3
        } else if slope {
            2
        } else {
            1
        }
    };
    let (near_start, near_end) = (far(d1.is_some(), c1.is_some()), far(d0.is_some(), c0.is_some()));
    let mut out = line;
    if let Some(d) = d0 {
        let k = near_start;
        out += d * (t * s.powi(k));
        // Its own second derivative, −2k·d, is made up to what's asked.
        if let Some(c) = c0 {
            out += (c + d * (2.0 * f64::from(k))) * (0.5 * t * t * s.powi(k));
        }
    }
    if let Some(d) = d1 {
        let k = near_end;
        out -= d * (s * t.powi(k));
        if let Some(c) = c1 {
            out += (c - d * (2.0 * f64::from(k))) * (0.5 * s * s * t.powi(k));
        }
    }
    out
}

fn way(a: &Rim, fa: f64, b: &Rim, fb: f64, t: f64) -> DVec3 {
    let (p, q) = (a.at(fa), b.at(fb));
    let chord = q - p;
    // At the far end the way comes out of the opening: what it would leave by, backwards.
    let (slope, second) = b.leave(fb, -chord);
    across(p, q, a.leave(fa, chord), (slope.map(|d| -d), second), t)
}

fn plain(rims: &[Rim]) -> bool {
    rims.iter().all(|r| r.order == 0 || r.bend.is_none())
}

/// Two sides from corner to corner (both run the same way): across from one to the other.
fn two(r: &[Rim; 2], s: f64, t: f64) -> DVec3 {
    way(&r[0], s, &r[1], s, t)
}

/// Three sides round: `r[0]` from A to B, `r[1]` from B to C, `r[2]` from C to A, at barycentric weights.
fn three(r: &[Rim; 3], w: [f64; 3], flat: bool) -> DVec3 {
    let corner = [r[0].at(0.0), r[1].at(0.0), r[2].at(0.0)];
    // Along lines parallel to the side facing corner `i`, between the two sides at that corner.
    let f = |i: usize| -> DVec3 {
        let k = (i + 2) % 3;
        if w[i] > 1.0 - 1e-12 {
            return corner[i];
        }
        let (side_ij, side_ki) = (&r[i], &r[k]);
        let x = 1.0 - w[i];
        // On side i→j the point is `x` along; on side k→i it's `x` back from its end.
        way(side_ij, x, side_ki, 1.0 - x, w[k] / x)
    };
    if flat {
        return (f(0) + f(1) + f(2) - corner[0] * w[0] - corner[1] * w[1] - corner[2] * w[2]) * 0.5;
    }
    // Each way meets two sides as asked; its weight dies, with its slope, on the third (facing corner i).
    let k = |i: usize| w[i] * w[i] * (3.0 - 2.0 * w[i] + 6.0 * w[(i + 1) % 3] * w[(i + 2) % 3]);
    let total = k(0) + k(1) + k(2);
    (f(0) * k(0) + f(1) * k(1) + f(2) * k(2)) / total
}

/// Four sides: bottom (u), right (v), top (u, same way as the bottom), left (v).
fn four(bottom: &Rim, right: &Rim, top: &Rim, left: &Rim, u: f64, v: f64, flat: bool) -> DVec3 {
    let p = way(left, v, right, v, u);
    let q = way(bottom, u, top, u, v);
    if flat {
        let (a, b) = (bottom.at(0.0).lerp(bottom.at(1.0), u), top.at(0.0).lerp(top.at(1.0), u));
        return p + q - a.lerp(b, v);
    }
    let (a, b) = ((v * (1.0 - v)).powi(2), (u * (1.0 - u)).powi(2));
    if a + b < 1e-24 { (p + q) / 2.0 } else { (p * a + q * b) / (a + b) }
}

const FAIL: &str = "the opening couldn't be filled";

/// A sheet through a grid of points (`at(u, v)`, both 0 to 1), its edges exactly the grid's. Sides that shrink to a point
/// are left out of the outline.
fn sheet(at: &dyn Fn(f64, f64) -> DVec3, n: usize, size: f64) -> Result<Brep, &'static str> {
    let us: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
    let c = Cubic::new(us.clone(), [Pin::Slope, Pin::Slope]).ok_or(FAIL)?;
    let h = 1e-3;
    let slope = |f: &dyn Fn(f64) -> DVec3, last: bool| -> DVec3 {
        let x = |k: f64| if last { 1.0 - h * k } else { h * k };
        let d = (f(x(0.0)) * -25.0 + f(x(1.0)) * 48.0 - f(x(2.0)) * 36.0 + f(x(3.0)) * 16.0 - f(x(4.0)) * 3.0) / (12.0 * h);
        if last { -d } else { d }
    };
    let m = n + 2;
    // Poles along v of each u column, for any u.
    let column = |u: f64| -> Vec<DVec3> {
        let vals: Vec<DVec3> = us.iter().map(|&v| at(u, v)).collect();
        c.poles(&vals, [slope(&|v| at(u, v), false), slope(&|v| at(u, v), true)])
    };
    let cols: Vec<Vec<DVec3>> = us.iter().map(|&u| column(u)).collect();
    let end = |last: bool| -> Vec<DVec3> {
        let k: Vec<Vec<DVec3>> = (0..5).map(|k| column(if last { 1.0 - h * f64::from(k) } else { h * f64::from(k) })).collect();
        (0..m)
            .map(|j| (k[0][j] * -25.0 + k[1][j] * 48.0 - k[2][j] * 36.0 + k[3][j] * 16.0 - k[4][j] * 3.0) / (12.0 * h))
            .map(|d| if last { -d } else { d })
            .collect()
    };
    let (s0, s1) = (end(false), end(true));
    let mut poles = vec![DVec3::ZERO; m * m];
    for j in 0..m {
        let vals: Vec<DVec3> = cols.iter().map(|c| c[j]).collect();
        for (i, p) in c.poles(&vals, [s0[j], s1[j]]).into_iter().enumerate() {
            poles[i * m + j] = p;
        }
    }
    if poles.iter().any(|p| !p.is_finite() || p.length() > 1e8) {
        return Err(FAIL);
    }
    let mults: Vec<u32> = (0..n).map(|i| if i == 0 || i + 1 == n { 4 } else { 1 }).collect();
    let side = |k: usize| -> Vec<DVec3> {
        match k {
            0 => (0..m).map(|i| poles[i * m]).collect(),
            1 => (0..m).map(|j| poles[(m - 1) * m + j]).collect(),
            2 => (0..m).map(|i| poles[i * m + m - 1]).collect(),
            _ => poles[..m].to_vec(),
        }
    };
    let mut b = Brep { kind: BodyKind::Sheet, ..Brep::default() };
    let vertex = |b: &mut Brep, p: DVec3| -> usize {
        if let Some(i) = b.vertices.iter().position(|v| v.distance(p) < size * 1e-7) {
            return i;
        }
        b.vertices.push(p);
        b.vertices.len() - 1
    };
    let mut ring = Vec::new();
    for (k, forward) in [(0, true), (1, true), (2, false), (3, false)] {
        let pts = side(k);
        // A side that's one point has no edge.
        if pts.iter().all(|p| p.distance(pts[0]) < size * 1e-7) {
            continue;
        }
        let (a, z) = (vertex(&mut b, pts[0]), vertex(&mut b, pts[m - 1]));
        b.curves.push(Curve::BSpline(Nurbs {
            degree: 3,
            poles: pts,
            weights: None,
            knots: us.clone(),
            mults: mults.clone(),
            periodic: false,
        }));
        b.edges.push(Edge { curve: Some(b.curves.len() - 1), reversed: false, start: Some(a), end: Some(z), tolerance: None });
        ring.push(Fin { edge: b.edges.len() - 1, forward, pcurve: None });
    }
    b.surfaces.push(Surface::BSpline(NurbsSurface {
        u_degree: 3,
        v_degree: 3,
        nu: m,
        nv: m,
        poles,
        weights: None,
        u_knots: us.clone(),
        u_mults: mults.clone(),
        v_knots: us,
        v_mults: mults,
        u_periodic: false,
        v_periodic: false,
    }));
    b.faces.push(Face { surface: 0, flip: false, loops: vec![ring], color: None });
    b.shells = vec![vec![0]];
    Ok(b)
}

/// Sheets that together close the loop of `rims` (in order, each run from where the one before ends).
pub fn fill(rims: &[Rim]) -> Result<Vec<Brep>, &'static str> {
    let pts: Vec<DVec3> = rims.iter().flat_map(|r| (0..16).map(move |k| r.at(f64::from(k) / 16.0))).collect();
    let size = pts.iter().map(|p| p.distance(pts[0])).fold(0.0, f64::max);
    if !(size.is_finite() && size > 1e-9) {
        return Err(FAIL);
    }
    let n = 33;
    // Which way the opening is from each side: towards its middle.
    let middle = pts.iter().sum::<DVec3>() / pts.len() as f64;
    let rims: Vec<Rim> = rims
        .iter()
        .map(|r| {
            let lean: f64 = (0..=16)
                .filter_map(|k| {
                    let f = f64::from(k) / 16.0;
                    let (normal, _) = r.bend_at(f)?;
                    let over = normal.cross(r.tangent(f)) * if r.reversed { -1.0 } else { 1.0 };
                    Some(over.dot((middle - r.at(f)).normalize_or_zero()))
                })
                .sum();
            Rim { inward: if lean.abs() < 0.5 { 0.0 } else { lean.signum() }, ..r.clone() }
        })
        .collect();
    match rims.as_slice() {
        [] => Err(FAIL),
        // A closed side without corners: two halves.
        [r] => fill(&[r.portion(0.0, 0.5), r.portion(0.5, 1.0)]),
        [a, b] => {
            let r = [a.clone(), b.flipped()];
            Ok(vec![sheet(&|s, t| two(&r, s, t), n, size)?])
        }
        [a, b, c] => {
            let r = [a.clone(), b.clone(), c.clone()];
            let flat = plain(&r);
            // The whole line v = 1 is the corner C.
            let at = |u: f64, v: f64| three(&r, [(1.0 - u) * (1.0 - v), u * (1.0 - v), v], flat);
            Ok(vec![sheet(&at, n, size)?])
        }
        [a, b, c, d] => {
            let (top, left) = (c.flipped(), d.flipped());
            let flat = plain(&[a.clone(), b.clone(), c.clone(), d.clone()]);
            Ok(vec![sheet(&|u, v| four(a, b, &top, &left, u, v, flat), n, size)?])
        }
        many => around(many, size, n),
    }
}

/// Five sides or more: a point in the middle, spokes to the middle of each side, and a four-sided patch at each corner.
fn around(rims: &[Rim], size: f64, n: usize) -> Result<Vec<Brep>, &'static str> {
    let k = rims.len();
    let mids: Vec<DVec3> = rims.iter().map(|r| r.at(0.5)).collect();
    // The middle: halfway along the ways across from each side to the one facing it.
    let mut center = DVec3::ZERO;
    for i in 0..k {
        let facing = (i + k / 2) % k;
        center += way(&rims[i], 0.5, &rims[facing], 0.5, 0.5);
    }
    center /= k as f64;
    let area: DVec3 = (0..k).map(|i| (mids[i] - center).cross(mids[(i + 1) % k] - center)).sum();
    let normal = area.try_normalize().ok_or(FAIL)?;
    let spokes: Vec<Rim> = (0..k)
        .map(|i| {
            let chord = center - mids[i];
            let start = rims[i].leave(0.5, chord);
            let flat = chord - normal * chord.dot(normal);
            let len = chord.length();
            let dir = flat.normalize_or_zero();
            let cos = if len > 0.0 { dir.dot(chord / len).clamp(-0.5, 1.0) } else { 1.0 };
            let end = (Some(dir * len * 2.0 / (1.0 + cos)), None);
            let pts: Vec<DVec3> = (0..=48).map(|j| across(mids[i], center, (start.0, None), end, f64::from(j) / 48.0)).collect();
            let run = Run::new(&crate::EdgeCurve::Other { points: pts.clone() })?;
            // The surface's normal along the spoke: from the side's own to the middle's.
            let t0 = (pts[1] - pts[0]).normalize_or_zero();
            let first = rims[i].bend_at(0.5).map_or_else(|| t0.cross(rims[i].tangent(0.5)).normalize_or_zero(), |b| b.0);
            let first = if first.dot(normal) < 0.0 { -first } else { first };
            let samples = (0..=32)
                .map(|j| {
                    let f = f64::from(j) / 32.0;
                    let at = ((f * 48.0) as usize).clamp(1, 47);
                    let t = (pts[at + 1] - pts[at - 1]).normalize_or_zero();
                    let m = first.lerp(normal, f);
                    ((m - t * m.dot(t)).normalize_or(normal), [0.0; 3])
                })
                .collect();
            Rim::new(vec![run], 1, Bend::new(samples))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(FAIL)?;
    (0..k)
        .map(|i| {
            let j = (i + 1) % k;
            // Corner between side i and side j: half of each, and the two spokes.
            let (bottom, right) = (rims[i].portion(0.5, 1.0), rims[j].portion(0.0, 0.5));
            let (top, left) = (&spokes[j], &spokes[i]);
            // bottom: mid i → corner; right: corner → mid j; top (same way as bottom): center → mid j; left: mid i → center.
            let top = top.flipped();
            sheet(&|u, v| four(&bottom, &right, &top, left, u, v, false), n, size)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EdgeCurve;
    use glam::dvec3;

    #[test]
    fn a_way_across_keeps_every_end_condition() {
        let (p, q) = (dvec3(0.0, 0.0, 0.0), dvec3(4.0, 1.0, 0.0));
        let start = (Some(dvec3(1.0, 5.0, 0.0)), Some(dvec3(0.0, 0.0, 3.0)));
        let end = (Some(dvec3(2.0, 0.0, -1.0)), Some(dvec3(1.0, 1.0, 1.0)));
        for (s, e) in [(start, end), (start, (end.0, None)), ((start.0, None), end), ((start.0, None), (None, None)), ((None, None), end)] {
            let f = |t: f64| across(p, q, s, e, t);
            assert!(f(0.0).distance(p) < 1e-12 && f(1.0).distance(q) < 1e-12);
            let h = 1e-4;
            let d0 = (f(h) - f(-h)) / (2.0 * h);
            let d1 = (f(1.0 + h) - f(1.0 - h)) / (2.0 * h);
            let c0 = (f(h) + f(-h) - f(0.0) * 2.0) / (h * h);
            let c1 = (f(1.0 + h) + f(1.0 - h) - f(1.0) * 2.0) / (h * h);
            assert!(s.0.is_none_or(|d| d.distance(d0) < 1e-5), "{d0}");
            assert!(e.0.is_none_or(|d| d.distance(d1) < 1e-5), "{d1}");
            assert!(s.1.is_none_or(|c| c.distance(c0) < 1e-3), "{c0}");
            assert!(e.1.is_none_or(|c| c.distance(c1) < 1e-3), "{c1}");
        }
    }

    #[test]
    fn sides_meet_the_sheets_edges() {
        let p = [dvec3(0.0, 0.0, 0.0), dvec3(10.0, 0.0, 1.0), dvec3(12.0, 8.0, 0.0), dvec3(5.0, 12.0, 2.0), dvec3(-2.0, 7.0, 0.0)];
        for k in 3..=5 {
            let rims: Vec<Rim> = (0..k)
                .map(|i| Rim::new(vec![Run::new(&EdgeCurve::Line { a: p[i], b: p[(i + 1) % k] }).unwrap()], 0, None).unwrap())
                .collect();
            let sheets = fill(&rims).unwrap();
            assert_eq!(sheets.len(), if k == 5 { 5 } else { 1 });
            for s in &sheets {
                assert_eq!(s.faces[0].loops[0].len(), if k == 3 { 3 } else { 4 });
            }
        }
    }
}

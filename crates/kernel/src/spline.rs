//! The fit-point spline as the reference makes it, so a spline drawn here is the same curve in Shapr3D: a clamped
//! B-spline (cubic, lower with fewer points) through the points, each point's parameter set by the square root of
//! the chord before it with the first and last chords counting half, and the inner knots averaged from those
//! parameters. Measured on Shapr3D's own splines (4 and 6 points: poles to 1e-17, knots to 1e-16).

use glam::DVec3;

#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub degree: usize,
    pub poles: Vec<DVec3>,
    /// Every knot, ends repeated `degree + 1` times, 0 to 1.
    pub knots: Vec<f64>,
    /// Where the curve passes each point.
    pub params: Vec<f64>,
}

/// The spline through `points`; `None` for fewer than two, more than 1000, or two in the same place.
pub fn fit(points: &[DVec3]) -> Option<Fit> {
    let n = points.len();
    if !(2..=1000).contains(&n) || points.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let p = (n - 1).min(3);
    let mut w: Vec<f64> = points.windows(2).map(|s| s[0].distance(s[1]).sqrt()).collect();
    if w.iter().any(|&x| x < 1e-9) {
        return None;
    }
    if w.len() > 2 {
        w[0] *= 0.5;
        let last = w.len() - 1;
        w[last] *= 0.5;
    }
    let total: f64 = w.iter().sum();
    let mut params = vec![0.0];
    let mut at = 0.0;
    for x in &w {
        at += x / total;
        params.push(at);
    }
    params[n - 1] = 1.0;
    let mut knots = vec![0.0; p + 1];
    knots.extend((1..n - p).map(|j| params[j..j + p].iter().sum::<f64>() / p as f64));
    knots.extend(std::iter::repeat_n(1.0, p + 1));
    // Solve basis(params) · poles = points.
    let mut a = vec![vec![0.0; n]; n];
    for (i, &u) in params.iter().enumerate() {
        let (span, b) = basis(&knots, p, u);
        for (k, v) in b.iter().enumerate() {
            a[i][span - p + k] = *v;
        }
    }
    let poles = solve(a, points.to_vec())?;
    Some(Fit { degree: p, poles, knots, params })
}

impl Fit {
    pub fn point(&self, u: f64) -> DVec3 {
        let (span, b) = basis(&self.knots, self.degree, u.clamp(0.0, 1.0));
        b.iter().enumerate().map(|(k, v)| self.poles[span - self.degree + k] * *v).sum()
    }

    /// The first derivative over the 0–1 parameter.
    pub fn derivative(&self, u: f64) -> DVec3 {
        let p = self.degree;
        if p == 0 {
            return DVec3::ZERO;
        }
        let poles: Vec<DVec3> = (0..self.poles.len() - 1)
            .map(|i| {
                let gap = self.knots[i + p + 1] - self.knots[i + 1];
                if gap > 0.0 { (self.poles[i + 1] - self.poles[i]) * (p as f64 / gap) } else { DVec3::ZERO }
            })
            .collect();
        let d = Fit { degree: p - 1, poles, knots: self.knots[1..self.knots.len() - 1].to_vec(), params: Vec::new() };
        d.point(u)
    }

    /// Points along the curve, `per_span` between each pair of points it passes.
    pub fn sample(&self, per_span: usize) -> Vec<DVec3> {
        let mut out = Vec::new();
        for s in self.params.windows(2) {
            for k in 0..per_span.max(1) {
                out.push(self.point(s[0] + (s[1] - s[0]) * k as f64 / per_span.max(1) as f64));
            }
        }
        out.push(self.point(1.0));
        out
    }

    /// The distinct knots and how often each repeats.
    pub fn knot_runs(&self) -> (Vec<f64>, Vec<usize>) {
        let (mut ks, mut ms) = (Vec::new(), Vec::<usize>::new());
        for &k in &self.knots {
            if ks.last() == Some(&k) {
                if let Some(m) = ms.last_mut() {
                    *m += 1;
                }
            } else {
                ks.push(k);
                ms.push(1);
            }
        }
        (ks, ms)
    }
}

/// The knot span holding `u` and the `p + 1` basis functions that don't vanish there (Piegl & Tiller A2.1/A2.2).
fn basis(knots: &[f64], p: usize, u: f64) -> (usize, Vec<f64>) {
    let n = knots.len() - p - 2;
    let span = if u >= knots[n + 1] { n } else { (p..=n).rev().find(|&i| knots[i] <= u).unwrap_or(p) };
    let mut b = vec![0.0; p + 1];
    let (mut left, mut right) = (vec![0.0; p + 1], vec![0.0; p + 1]);
    b[0] = 1.0;
    for j in 1..=p {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let gap = right[r + 1] + left[j - r];
            let t = if gap != 0.0 { b[r] / gap } else { 0.0 };
            b[r] = saved + right[r + 1] * t;
            saved = left[j - r] * t;
        }
        b[j] = saved;
    }
    (span, b)
}

/// Gaussian elimination with partial pivoting.
// Row operations read best with explicit indices.
#[allow(clippy::needless_range_loop)]
fn solve(mut a: Vec<Vec<f64>>, mut x: Vec<DVec3>) -> Option<Vec<DVec3>> {
    let n = x.len();
    for c in 0..n {
        let pivot = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[pivot][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, pivot);
        x.swap(c, pivot);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f != 0.0 {
                for k in c..n {
                    a[r][k] -= f * a[c][k];
                }
                x[r] = x[r] - x[c] * f;
            }
        }
    }
    for c in (0..n).rev() {
        let mut v = x[c];
        for k in c + 1..n {
            v -= x[k] * a[c][k];
        }
        x[c] = v / a[c][c];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_points_make_shapr3ds_knots_and_poles() {
        // A spline Shapr3D made through six points (meters): its knots and first inner pole.
        let q = [
            (0.0025, 0.003),
            (0.0048706503584980968, 0.0066141304560005669),
            (0.007345651276409626, 0.001664130948483944),
            (0.009820651262998581, 0.007439130451530218),
            (0.012295651249587536, 0.0024891304783523084),
            (0.01477065123617649, 0.006201631389558315),
        ];
        let f = fit(&q.map(|(x, y)| DVec3::new(x, y, 0.0))).unwrap();
        assert_eq!(f.knot_runs(), (vec![0.0, 0.3699659378017636, 0.6282446147862379, 1.0], vec![4, 1, 1, 4]));
        assert!((f.poles[1] - DVec3::new(0.005891249220690948, 0.011952877437305934, 0.0)).length() < 1e-15);
        assert!((f.derivative(0.0) - DVec3::new(0.027499146874229747, 0.07259758147332278, 0.0)).length() < 1e-12);
        for (u, p) in f.params.iter().zip(&q) {
            assert!((f.point(*u) - DVec3::new(p.0, p.1, 0.0)).length() < 1e-15);
        }
    }
}

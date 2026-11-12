//! Sketch exports: the elements of a sketch, in its own plane's x/y (millimetres), as an ASCII DXF, a DWG or an SVG.

use std::fmt::Write as _;

use glam::DVec2;
use shapecraft_doc::{SketchEntity, bspline_points, circle_through, spline_points};

fn line(out: &mut String, a: DVec2, b: DVec2) {
    let _ = write!(out, "0\nLINE\n8\n0\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a.x, a.y, b.x, b.y);
}

fn polyline(out: &mut String, pts: &[DVec2], closed: bool) {
    for w in pts.windows(2) {
        line(out, w[0], w[1]);
    }
    if closed && pts.len() > 2 {
        line(out, pts[pts.len() - 1], pts[0]);
    }
}

/// An R12 DXF (millimetres) of the sketch elements; text and construction elements are left out.
pub fn write_dxf(entities: &[(SketchEntity, bool)]) -> String {
    let mut out = String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (e, construction) in entities {
        if *construction {
            continue;
        }
        match e {
            SketchEntity::Line { a, b } => line(&mut out, *a, *b),
            SketchEntity::Rectangle { min, max } => {
                polyline(&mut out, &[*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)], true)
            }
            SketchEntity::Circle { center, radius } => {
                let _ = write!(out, "0\nCIRCLE\n8\n0\n10\n{}\n20\n{}\n30\n0\n40\n{}\n", center.x, center.y, radius);
            }
            SketchEntity::Polygon { center, radius, sides, angle } => {
                let n = (*sides).clamp(3, 1000);
                let pts: Vec<DVec2> = (0..n)
                    .map(|k| *center + DVec2::from_angle(angle + std::f64::consts::TAU * f64::from(k) / f64::from(n)) * *radius)
                    .collect();
                polyline(&mut out, &pts, true);
            }
            SketchEntity::Arc { a, mid, b } => {
                let Some((c, r)) = circle_through(*a, *mid, *b) else {
                    line(&mut out, *a, *b);
                    continue;
                };
                // DXF arcs run counter-clockwise from start to end.
                let ccw = (*mid - *a).perp_dot(*b - *mid) > 0.0;
                let (s, e) = if ccw { (*a, *b) } else { (*b, *a) };
                let ang = |p: DVec2| (p - c).y.atan2((p - c).x).to_degrees();
                let _ = write!(out, "0\nARC\n8\n0\n10\n{}\n20\n{}\n30\n0\n40\n{}\n50\n{}\n51\n{}\n", c.x, c.y, r, ang(s), ang(e));
            }
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let (major, ratio, turn) =
                    if rx >= ry { (*rx, ry / rx, *angle) } else { (*ry, rx / ry, angle + std::f64::consts::FRAC_PI_2) };
                let m = DVec2::from_angle(turn) * major;
                let _ = write!(
                    out,
                    "0\nELLIPSE\n8\n0\n100\nAcDbEntity\n100\nAcDbEllipse\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n40\n{}\n41\n0\n42\n{}\n",
                    center.x,
                    center.y,
                    m.x,
                    m.y,
                    ratio,
                    std::f64::consts::TAU
                );
            }
            SketchEntity::Spline { points } => polyline(&mut out, &spline_points(points, 16), false),
            SketchEntity::ControlSpline { poles } => polyline(&mut out, &bspline_points(poles, 16), false),
            SketchEntity::Text { .. } => {}
        }
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

/// A sketch's curves as an AutoCAD R2000 DWG, in millimetres in the sketch's plane: lines, circles and arcs as
/// themselves, rectangles and polygons as closed polylines, ellipses and splines as fine polylines. Construction
/// curves and text are left out, as in the DXF.
pub fn write_dwg(entities: &[(SketchEntity, bool)]) -> std::result::Result<Vec<u8>, String> {
    use shapecraft_dwg as dwg;
    let mut d = dwg::Drawing { layers: Vec::new(), linetypes: Vec::new(), entities: Vec::new(), limits: None, date: None };
    let p3 = |p: DVec2| [p.x, p.y, 0.0];
    let poly =
        |pts: &[DVec2], closed: bool| dwg::Shape::Polyline { points: pts.iter().map(|p| [p.x, p.y]).collect(), bulges: Vec::new(), closed };
    for (e, construction) in entities {
        if *construction {
            continue;
        }
        let shape = match e {
            SketchEntity::Line { a, b } => dwg::Shape::Line { a: p3(*a), b: p3(*b) },
            SketchEntity::Rectangle { min, max } => poly(&[*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)], true),
            SketchEntity::Circle { center, radius } => dwg::Shape::Circle { center: p3(*center), radius: *radius },
            SketchEntity::Polygon { center, radius, sides, angle } => {
                let n = (*sides).clamp(3, 1000);
                let pts: Vec<DVec2> = (0..n)
                    .map(|k| *center + DVec2::from_angle(angle + std::f64::consts::TAU * f64::from(k) / f64::from(n)) * *radius)
                    .collect();
                poly(&pts, true)
            }
            SketchEntity::Arc { a, mid, b } => match circle_through(*a, *mid, *b) {
                Some((c, r)) => {
                    // Counter-clockwise from start to end.
                    let ccw = (*mid - *a).perp_dot(*b - *mid) > 0.0;
                    let (s, e) = if ccw { (*a, *b) } else { (*b, *a) };
                    let ang = |p: DVec2| (p - c).y.atan2((p - c).x);
                    dwg::Shape::Arc { center: p3(c), radius: r, start: ang(s), end: ang(e) }
                }
                None => dwg::Shape::Line { a: p3(*a), b: p3(*b) },
            },
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let (u, v) = (DVec2::from_angle(*angle), DVec2::from_angle(angle + std::f64::consts::FRAC_PI_2));
                let pts: Vec<DVec2> = (0..96)
                    .map(|k| {
                        let t = std::f64::consts::TAU * f64::from(k) / 96.0;
                        *center + u * (rx * t.cos()) + v * (ry * t.sin())
                    })
                    .collect();
                poly(&pts, true)
            }
            SketchEntity::Spline { points } => poly(&spline_points(points, 16), false),
            SketchEntity::ControlSpline { poles } => poly(&bspline_points(poles, 16), false),
            SketchEntity::Text { .. } => continue,
        };
        d.entities.push(dwg::Entity::new("0", shape));
    }
    dwg::write(&d).map_err(|e| e.to_string())
}

/// SVG sketch export settings: pixels per inch, curve resolution 0 (coarsest) to 100 (finest), whether connected
/// curves are joined into one shape (on) or written one shape each (off), and whether arcs, circles, ellipses and
/// splines stay true curves (on) or become short straight lines at the resolution (off).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgOptions {
    pub dpi: f64,
    pub resolution: f64,
    pub join: bool,
    pub curves: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions { dpi: 96.0, resolution: 100.0, join: true, curves: true }
    }
}

impl SvgOptions {
    /// The turning angle between points on a flattened curve: 0.1 rad at resolution 0, 0.01 rad at 100.
    fn step(&self) -> f64 {
        let r = if self.resolution.is_finite() { self.resolution.clamp(0.0, 100.0) } else { 100.0 };
        0.1 - 0.09 * r / 100.0
    }
}

/// One curve flattened: its points, and whether the last joins back to the first.
struct Flat {
    pts: Vec<DVec2>,
    closed: bool,
}

/// Points every `step` radians from `t0` over `sweep` (either sign), the last step shorter; the end itself is
/// added for an open curve.
fn steps(t0: f64, sweep: f64, step: f64, closed: bool, at: impl Fn(f64) -> DVec2) -> Vec<DVec2> {
    let n = ((sweep.abs() / step) - 1e-9).ceil().clamp(1.0, 100_000.0) as usize;
    let dir = sweep.signum();
    let mut pts: Vec<DVec2> = (0..n).map(|k| at(t0 + dir * step * k as f64)).collect();
    if !closed {
        pts.push(at(t0 + sweep));
    }
    pts
}

/// A densely sampled curve thinned so that the tangent turns about `step` radians between kept points.
fn thin(dense: &[DVec2], step: f64) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = dense.first().copied().into_iter().collect();
    let mut turned = 0.0;
    for w in dense.windows(3) {
        let (u, v) = (w[1] - w[0], w[2] - w[1]);
        if u.length() > 1e-12 && v.length() > 1e-12 {
            turned += u.angle_to(v).abs();
        }
        if turned >= step {
            out.push(w[1]);
            turned = 0.0;
        }
    }
    if let Some(last) = dense.last().copied().filter(|l| out.last() != Some(l)) {
        out.push(last);
    }
    out
}

fn arc_parts(a: DVec2, mid: DVec2, b: DVec2) -> Option<(DVec2, f64, f64, f64)> {
    let (c, r) = circle_through(a, mid, b)?;
    let ang = |p: DVec2| (p - c).y.atan2((p - c).x);
    let ccw = (mid - a).perp_dot(b - mid) > 0.0;
    let tau = std::f64::consts::TAU;
    let mut sweep = (ang(b) - ang(a)).rem_euclid(tau);
    if !ccw {
        sweep -= tau;
    }
    Some((c, r, ang(a), sweep))
}

fn flatten(e: &SketchEntity, step: f64) -> Vec<Flat> {
    let open = |pts: Vec<DVec2>| vec![Flat { pts, closed: false }];
    match e {
        SketchEntity::Line { a, b } => open(vec![*a, *b]),
        SketchEntity::Rectangle { min, max } => {
            vec![Flat { pts: vec![*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)], closed: true }]
        }
        SketchEntity::Polygon { center, radius, sides, angle } => {
            let n = (*sides).clamp(3, 1000);
            let pts = (0..n).map(|k| *center + DVec2::from_angle(angle + std::f64::consts::TAU * f64::from(k) / f64::from(n)) * *radius);
            vec![Flat { pts: pts.collect(), closed: true }]
        }
        SketchEntity::Circle { center, radius } => {
            vec![Flat { pts: steps(0.0, std::f64::consts::TAU, step, true, |t| *center + DVec2::from_angle(t) * *radius), closed: true }]
        }
        SketchEntity::Arc { a, mid, b } => match arc_parts(*a, *mid, *b) {
            Some((c, r, t0, sweep)) => open(steps(t0, sweep, step, false, |t| c + DVec2::from_angle(t) * r)),
            None => open(vec![*a, *b]),
        },
        SketchEntity::Ellipse { center, rx, ry, angle } => {
            let rot = DVec2::from_angle(*angle);
            let at = |t: f64| *center + rot.rotate(DVec2::new(rx * t.cos(), ry * t.sin()));
            vec![Flat { pts: steps(0.0, std::f64::consts::TAU, step, true, at), closed: true }]
        }
        SketchEntity::Spline { points } => open(thin(&spline_points(points, 256), step)),
        SketchEntity::ControlSpline { poles } => open(thin(&bspline_points(poles, 256), step)),
        // Text as its letters' outlines, one closed shape per contour (cut or engraved like any other shape).
        SketchEntity::Text { .. } => e
            .outlines()
            .into_iter()
            .filter_map(|mut pts| {
                if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) < 1e-9 {
                    pts.pop();
                }
                (pts.len() > 2).then_some(Flat { pts, closed: true })
            })
            .collect(),
    }
}

/// One stretch of a shape, ending at its last point (millimetres, y up).
#[derive(Clone, Copy, Debug)]
enum Seg {
    Line(DVec2),
    /// An elliptical arc: radii, the x radius' turn (radians, counter-clockwise), the long way round, and which way.
    Arc {
        rx: f64,
        ry: f64,
        turn: f64,
        large: bool,
        ccw: bool,
        to: DVec2,
    },
    Quad(DVec2, DVec2),
    Cubic(DVec2, DVec2, DVec2),
}

impl Seg {
    fn to(&self) -> DVec2 {
        match *self {
            Seg::Line(p) | Seg::Arc { to: p, .. } | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        }
    }

    fn set_to(&mut self, q: DVec2) {
        match self {
            Seg::Line(p) | Seg::Arc { to: p, .. } | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => *p = q,
        }
    }
}

/// Connected segments from `start`; a closed piece's last segment comes back to `start`. `order` is the first
/// sketch element it came from, so shapes are written in drawing order.
#[derive(Clone, Debug)]
struct Piece {
    start: DVec2,
    segs: Vec<Seg>,
    closed: bool,
    order: usize,
}

impl Piece {
    fn from_flat(f: Flat, order: usize) -> Piece {
        let mut segs: Vec<Seg> = f.pts.iter().skip(1).map(|p| Seg::Line(*p)).collect();
        if f.closed {
            segs.push(Seg::Line(f.pts[0]));
        }
        Piece { start: f.pts[0], segs, closed: f.closed, order }
    }

    fn end(&self) -> DVec2 {
        self.segs.last().map_or(self.start, Seg::to)
    }

    fn reversed(&self) -> Piece {
        let mut pts = vec![self.start];
        pts.extend(self.segs.iter().map(Seg::to));
        let segs = self
            .segs
            .iter()
            .enumerate()
            .rev()
            .map(|(i, s)| {
                let to = pts[i];
                match *s {
                    Seg::Line(_) => Seg::Line(to),
                    Seg::Arc { rx, ry, turn, large, ccw, .. } => Seg::Arc { rx, ry, turn, large, ccw: !ccw, to },
                    Seg::Quad(c, _) => Seg::Quad(c, to),
                    Seg::Cubic(a, b, _) => Seg::Cubic(b, a, to),
                }
            })
            .collect();
        Piece { start: self.end(), segs, closed: self.closed, order: self.order }
    }
}

/// Inserts knot `u` once into a clamped B-spline (Boehm).
fn insert_knot(deg: usize, poles: &mut Vec<DVec2>, knots: &mut Vec<f64>, u: f64) {
    let Some(s) = knots.iter().rposition(|&k| k <= u) else { return };
    let mut out = Vec::with_capacity(poles.len() + 1);
    for i in 0..=poles.len() {
        if i + deg <= s {
            out.push(poles[i]);
        } else if i > s {
            out.push(poles[i - 1]);
        } else {
            let span = knots[i + deg] - knots[i];
            let a = if span > 1e-15 { (u - knots[i]) / span } else { 0.0 };
            out.push(poles[i - 1] * (1.0 - a) + poles[i] * a);
        }
    }
    *poles = out;
    knots.insert(s + 1, u);
}

/// A clamped B-spline (its end knots repeated `deg + 1` times) as segments from its first pole: lines, quadratic or
/// cubic Béziers, exactly the same curve.
fn bspline_segs(deg: usize, poles: &[DVec2], knots: &[f64]) -> Option<Vec<Seg>> {
    if !(1..=3).contains(&deg) || poles.len() <= deg || knots.len() != poles.len() + deg + 1 {
        return None;
    }
    let (mut p, mut k) = (poles.to_vec(), knots.to_vec());
    let (lo, hi) = (k[deg], k[k.len() - deg - 1]);
    let mut inner: Vec<f64> = k.iter().copied().filter(|&u| u > lo + 1e-12 && u < hi - 1e-12).collect();
    inner.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    for u in inner {
        let have = k.iter().filter(|&&x| (x - u).abs() < 1e-12).count();
        for _ in have..deg {
            insert_knot(deg, &mut p, &mut k, u);
        }
    }
    if (p.len() - 1) % deg != 0 {
        return None;
    }
    Some(
        p[1..]
            .chunks(deg)
            .map(|c| match deg {
                1 => Seg::Line(c[0]),
                2 => Seg::Quad(c[0], c[1]),
                _ => Seg::Cubic(c[0], c[1], c[2]),
            })
            .collect(),
    )
}

/// A spline element as exact Bézier segments; `None` when it can't be built.
fn spline_piece(e: &SketchEntity, order: usize) -> Option<Piece> {
    let (deg, poles, knots) = match e {
        SketchEntity::Spline { points } => {
            let flat: Vec<glam::DVec3> = points.iter().map(|p| p.extend(0.0)).collect();
            let f = shapecraft_kernel::spline::fit(&flat)?;
            (f.degree, f.poles.iter().map(|p| p.truncate()).collect::<Vec<_>>(), f.knots)
        }
        SketchEntity::ControlSpline { poles } => {
            let n = poles.len();
            if n < 2 {
                return None;
            }
            let deg = (n - 1).min(3);
            let spans = n - deg;
            let knots = (0..n + deg + 1).map(|i| (i as f64 - deg as f64).clamp(0.0, spans as f64)).collect();
            (deg, poles.clone(), knots)
        }
        _ => return None,
    };
    let segs = bspline_segs(deg, &poles, &knots)?;
    Some(Piece { start: poles[0], segs, closed: false, order })
}

/// An element as pieces: with `curves`, arcs, circles and ellipses as elliptical arcs and splines as Béziers;
/// otherwise (and for everything else) short straight lines at `step`.
fn pieces(e: &SketchEntity, step: f64, curves: bool, order: usize) -> Vec<Piece> {
    if curves {
        let arc = |rx: f64, ry: f64, turn: f64, large: bool, ccw: bool, to: DVec2| Seg::Arc { rx, ry, turn, large, ccw, to };
        let ring = |c: DVec2, rx: f64, ry: f64, turn: f64| {
            let u = DVec2::from_angle(turn) * rx;
            let (a, b) = (c + u, c - u);
            Piece { start: a, segs: vec![arc(rx, ry, turn, false, true, b), arc(rx, ry, turn, false, true, a)], closed: true, order }
        };
        match e {
            SketchEntity::Circle { center, radius } => return vec![ring(*center, *radius, *radius, 0.0)],
            SketchEntity::Ellipse { center, rx, ry, angle } => return vec![ring(*center, *rx, *ry, *angle)],
            SketchEntity::Arc { a, mid, b } => {
                if let Some((_, r, _, sweep)) = arc_parts(*a, *mid, *b) {
                    let large = sweep.abs() > std::f64::consts::PI + 1e-9;
                    return vec![Piece { start: *a, segs: vec![arc(r, r, 0.0, large, sweep > 0.0, *b)], closed: false, order }];
                }
            }
            SketchEntity::Spline { .. } | SketchEntity::ControlSpline { .. } => {
                if let Some(p) = spline_piece(e, order) {
                    return vec![p];
                }
            }
            _ => {}
        }
    }
    flatten(e, step).into_iter().filter(|f| !f.pts.is_empty()).map(|f| Piece::from_flat(f, order)).collect()
}

/// Joins open pieces end to end (either way round) into chains; a chain that comes back to its start is closed.
/// The result is in drawing order.
fn join(all: Vec<Piece>) -> Vec<Piece> {
    const TOL: f64 = 1e-6;
    let (mut done, mut open): (Vec<Piece>, Vec<Piece>) = all.into_iter().partition(|p| p.closed);
    while let Some(mut chain) = open.pop() {
        loop {
            let (s, e) = (chain.start, chain.end());
            if chain.segs.len() > 1 && s.distance(e) < TOL {
                break;
            }
            let Some(i) = open.iter().position(|p| {
                let (ps, pe) = (p.start, p.end());
                ps.distance(e) < TOL || pe.distance(e) < TOL || ps.distance(s) < TOL || pe.distance(s) < TOL
            }) else {
                break;
            };
            let p = open.swap_remove(i);
            let order = chain.order.min(p.order);
            if p.start.distance(e) < TOL {
                chain.segs.extend(p.segs);
            } else if p.end().distance(e) < TOL {
                chain.segs.extend(p.reversed().segs);
            } else {
                let mut q = if p.end().distance(s) < TOL { p } else { p.reversed() };
                q.segs.append(&mut chain.segs);
                chain = q;
            }
            chain.order = order;
        }
        chain.closed = chain.segs.len() > 1 && chain.start.distance(chain.end()) < TOL;
        if chain.closed {
            let start = chain.start;
            if let Some(last) = chain.segs.last_mut() {
                last.set_to(start);
            }
        }
        done.push(chain);
    }
    done.sort_by_key(|p| p.order);
    done
}

/// A number with six significant digits and no trailing zeros ("17.6063", "119.303", "0.37").
fn num(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-9 {
        return "0".into();
    }
    let decimals = (5 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Sketch curves (in one plane's x/y, millimetres) as an SVG image: user units are pixels at `dpi`, y points down,
/// the drawing centred in a view box 10% larger than it, a thin black line (1/800 of the view box) and no fill. The
/// width and height are also given in millimetres so other apps open it at its true size whatever DPI they assume.
/// Construction curves are left out; text is its letters' outlines. `None` when there's nothing to draw.
pub fn write_svg(entities: &[(SketchEntity, bool)], o: &SvgOptions) -> Option<String> {
    let step = o.step();
    let dpi = if o.dpi.is_finite() && o.dpi > 0.0 { o.dpi.min(10_000.0) } else { 96.0 };
    let k = dpi / 25.4;
    let curves: Vec<(usize, &SketchEntity)> = entities.iter().enumerate().filter(|(_, (_, c))| !c).map(|(i, (e, _))| (i, e)).collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for (_, e) in &curves {
        for p in flatten(e, step).iter().flat_map(|f| f.pts.iter()) {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return None;
    }
    let size = (hi - lo) * k;
    let (w, h) = ((size.x * 1.1).ceil().max(1.0), (size.y * 1.1).ceil().max(1.0));
    let (mx, my) = ((w - size.x) / 2.0, (h - size.y) / 2.0);
    let px = |p: DVec2| DVec2::new((p.x - lo.x) * k + mx, (hi.y - p.y) * k + my);
    let pt = |p: DVec2| {
        let q = px(p);
        format!("{},{}", num(q.x), num(q.y))
    };
    let mm = |v: f64| num(v / dpi * 25.4);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\"?>\n<svg viewBox=\"0 0 {} {}\" width=\"{}mm\" height=\"{}mm\" \
         version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\" fill-rule=\"evenodd\" xml:space=\"preserve\" >\n<g fill=\"none\" stroke=\"rgb(0,0,0)\" stroke-width=\"{}\" >\n",
        num(w),
        num(h),
        mm(w),
        mm(h),
        num(0.00125 * w.max(h))
    );
    let write = |out: &mut String, p: &Piece| {
        if p.segs.iter().all(|s| matches!(s, Seg::Line(_))) {
            let tag = if p.closed { "polygon" } else { "polyline" };
            let ends = if p.closed { p.segs.len().saturating_sub(1) } else { p.segs.len() };
            let pts: String = std::iter::once(p.start).chain(p.segs[..ends].iter().map(Seg::to)).map(|q| pt(q) + " ").collect();
            let _ = writeln!(out, "<{tag} points=\"{pts}\" />");
            return;
        }
        let mut d = format!("M{}", pt(p.start));
        for s in &p.segs {
            let _ = match *s {
                Seg::Line(q) => write!(d, " L{}", pt(q)),
                Seg::Arc { rx, ry, turn, large, ccw, to } => write!(
                    d,
                    " A{},{} {} {},{} {}",
                    num(rx * k),
                    num(ry * k),
                    // Turned and swept the other way once y points down.
                    num(-turn.to_degrees()),
                    u8::from(large),
                    u8::from(!ccw),
                    pt(to)
                ),
                Seg::Quad(c, q) => write!(d, " Q{} {}", pt(c), pt(q)),
                Seg::Cubic(a, b, q) => write!(d, " C{} {} {}", pt(a), pt(b), pt(q)),
            };
        }
        if p.closed {
            d.push_str(" Z");
        }
        let _ = writeln!(out, "<path d=\"{d}\" />");
    };
    if o.join {
        for p in &join(curves.iter().flat_map(|(i, e)| pieces(e, step, o.curves, *i)).collect()) {
            write(&mut out, p);
        }
    } else {
        // One shape per curve: lines as two-point polylines, arcs as paths, circles and ellipses as themselves.
        for (i, e) in curves {
            match e {
                SketchEntity::Circle { center, radius } => {
                    let c = px(*center);
                    let _ = writeln!(out, "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" />", num(c.x), num(c.y), num(radius * k));
                }
                SketchEntity::Ellipse { center, rx, ry, angle } => {
                    let c = px(*center);
                    let (x, y) = (num(c.x), num(c.y));
                    // Turned the other way once y points down.
                    let turn = num(-angle.to_degrees());
                    let rotate = if turn == "0" { String::new() } else { format!(" transform=\"rotate({turn} {x} {y})\"") };
                    let _ = writeln!(out, "<ellipse cx=\"{x}\" cy=\"{y}\" rx=\"{}\" ry=\"{}\"{rotate} />", num(rx * k), num(ry * k));
                }
                SketchEntity::Arc { a, mid, b } if arc_parts(*a, *mid, *b).is_some() => {
                    let Some((_, r, _, sweep)) = arc_parts(*a, *mid, *b) else { continue };
                    let large = u8::from(sweep.abs() > std::f64::consts::PI + 1e-9);
                    // Counter-clockwise with y up turns the other way once y points down.
                    let flag = u8::from(sweep < 0.0);
                    let rr = num(r * k);
                    let _ = writeln!(out, "<path d=\"M{}  A{rr},{rr} 0 {large},{flag} {} \" />", pt(*a), pt(*b));
                }
                SketchEntity::Rectangle { .. } | SketchEntity::Polygon { .. } => {
                    for f in flatten(e, step) {
                        let n = f.pts.len();
                        for j in 0..n {
                            write(&mut out, &Piece::from_flat(Flat { pts: vec![f.pts[j], f.pts[(j + 1) % n]], closed: false }, i));
                        }
                    }
                }
                _ => {
                    for p in pieces(e, step, o.curves, i) {
                        write(&mut out, &p);
                    }
                }
            }
        }
    }
    out.push_str("</g>\n</svg>\n");
    Some(out)
}

/// The map from one sketch plane's (u, v) to another's, by projecting along the second plane's normal.
pub fn plane_to_plane(from: &shapecraft_doc::SketchPlane, to: &shapecraft_doc::SketchPlane) -> glam::DAffine2 {
    let d = from.origin - to.origin;
    glam::DAffine2::from_cols(
        DVec2::new(from.x.dot(to.x), from.x.dot(to.y)),
        DVec2::new(from.y.dot(to.x), from.y.dot(to.y)),
        DVec2::new(d.dot(to.x), d.dot(to.y)),
    )
}

/// A sketch element moved by `m`. A rigid move keeps lines, arcs, circles, ellipses and splines as they are
/// (rectangles and polygons become their sides); any other map turns the element into short lines.
pub fn entity_in_frame(e: &SketchEntity, m: &glam::DAffine2) -> Vec<SketchEntity> {
    let (x, y) = (m.matrix2.x_axis, m.matrix2.y_axis);
    let rigid = (x.length() - 1.0).abs() < 1e-9 && (y.length() - 1.0).abs() < 1e-9 && x.dot(y).abs() < 1e-9;
    let p = |v: DVec2| m.transform_point2(v);
    if rigid && *m == glam::DAffine2::IDENTITY {
        return vec![e.clone()];
    }
    if rigid {
        match e {
            SketchEntity::Line { a, b } => return vec![SketchEntity::Line { a: p(*a), b: p(*b) }],
            SketchEntity::Circle { center, radius } => return vec![SketchEntity::Circle { center: p(*center), radius: *radius }],
            SketchEntity::Arc { a, mid, b } => return vec![SketchEntity::Arc { a: p(*a), mid: p(*mid), b: p(*b) }],
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let ax = m.transform_vector2(DVec2::from_angle(*angle));
                return vec![SketchEntity::Ellipse { center: p(*center), rx: *rx, ry: *ry, angle: ax.y.atan2(ax.x) }];
            }
            SketchEntity::Spline { points } => return vec![SketchEntity::Spline { points: points.iter().map(|v| p(*v)).collect() }],
            SketchEntity::ControlSpline { poles } => {
                return vec![SketchEntity::ControlSpline { poles: poles.iter().map(|v| p(*v)).collect() }];
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for f in flatten(e, SvgOptions::default().step()) {
        let n = f.pts.len();
        let segs = if f.closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            out.push(SketchEntity::Line { a: p(f.pts[i]), b: p(f.pts[(i + 1) % n]) });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A 40 × 30 rectangle of four lines, a circle, a half arc, a small circle at the origin and a construction line
    /// (left out): the expected view box, stroke, point counts and shapes.
    fn sample() -> Vec<(SketchEntity, bool)> {
        let v = DVec2::new;
        let mut e: Vec<SketchEntity> =
            [(v(0.0, 0.0), v(40.0, 0.0)), (v(40.0, 0.0), v(40.0, 30.0)), (v(40.0, 30.0), v(0.0, 30.0)), (v(0.0, 30.0), v(0.0, 0.0))]
                .into_iter()
                .map(|(a, b)| SketchEntity::Line { a, b })
                .collect();
        e.push(SketchEntity::Circle { center: v(20.0, 15.0), radius: 8.0 });
        e.push(SketchEntity::Arc { a: v(70.0, 15.0), mid: v(60.0, 25.0), b: v(50.0, 15.0) });
        e.push(SketchEntity::Circle { center: v(0.0, 0.0), radius: 1.0 });
        e.push(SketchEntity::Line { a: v(-50.0, 0.0), b: v(-60.0, 0.0) });
        let mut out: Vec<_> = e.into_iter().map(|e| (e, false)).collect();
        out.last_mut().unwrap().1 = true; // construction: left out
        out
    }

    fn counts(svg: &str) -> Vec<(String, usize)> {
        svg.split('<')
            .filter_map(|t| {
                let tag = t.split_whitespace().next()?;
                let pts = t.split("points=\"").nth(1)?.split('"').next()?;
                Some((tag.to_string(), pts.split_whitespace().count()))
            })
            .collect()
    }

    #[test]
    fn joined_polylines_at_96_dpi() {
        let svg = write_svg(&sample(), &SvgOptions { curves: false, ..SvgOptions::default() }).unwrap();
        assert!(svg.contains("viewBox=\"0 0 296 129\" width=\"78.3167mm\" height=\"34.1313mm\""), "{svg}");
        assert!(svg.contains("stroke-width=\"0.37\""));
        // In drawing order: the rectangle, the circle, the arc, the small circle.
        let c = counts(&svg);
        let want = [("polygon", 4), ("polygon", 629), ("polyline", 316), ("polygon", 629)];
        assert_eq!(c, want.iter().map(|(t, n)| (t.to_string(), *n)).collect::<Vec<_>>());
        // The rectangle's corners, in pixels from the view box's top left (the last digit can differ by one).
        for corner in ["17.6063,5.9173", "17.6063,119.303", "168.787,119.303", "168.787,5.9173"] {
            assert!(svg.contains(corner), "{corner} in {svg}");
        }
    }

    #[test]
    fn dpi_and_resolution() {
        let svg = write_svg(&sample(), &SvgOptions { dpi: 72.0, resolution: 10.0, join: true, curves: false }).unwrap();
        assert!(svg.contains("viewBox=\"0 0 222 97\""));
        assert!(svg.contains("stroke-width=\"0.2775\""));
        let mut n: Vec<usize> = counts(&svg).into_iter().map(|c| c.1).collect();
        n.sort();
        assert_eq!(n, [4, 36, 70, 70]);
    }

    #[test]
    fn unjoined_shapes() {
        let svg = write_svg(&sample(), &SvgOptions { join: false, curves: false, ..SvgOptions::default() }).unwrap();
        assert!(svg.contains("<polyline points=\"17.6063,119.303 168.787,119.303 \" />"), "{svg}");
        assert!(svg.contains("<path d=\"M282.173,62.6102  A37.7953,37.7953 0 0,0 206.583,62.6102 \" />"));
        assert!(svg.contains("<circle cx=\"93.196") && svg.contains("cy=\"62.6102\" r=\"30.2362\" />"), "{svg}");
        assert!(svg.contains("<circle cx=\"17.6063\" cy=\"119.303\" r=\"3.77953\" />"));
    }

    #[test]
    fn stroke_follows_the_longer_side() {
        let tall = [(SketchEntity::Line { a: DVec2::ZERO, b: DVec2::new(0.0, 100.0) }, false)];
        let svg = write_svg(&tall, &SvgOptions::default()).unwrap();
        // 100 mm = 377.953 px, 10% more = 416 px tall; a hairline 1/800 of that.
        assert!(svg.contains("viewBox=\"0 0 1 416\"") && svg.contains("stroke-width=\"0.52\""), "{svg}");
    }

    #[test]
    fn joined_curves_stay_curves() {
        let svg = write_svg(&sample(), &SvgOptions::default()).unwrap();
        // The same view box as the flattened file; circles as two half arcs, the arc as one, the rectangle unchanged.
        assert!(svg.contains("viewBox=\"0 0 296 129\""));
        assert_eq!(counts(&svg), [("polygon".to_string(), 4)]);
        assert!(
            svg.contains("<path d=\"M123.433,62.6102 A30.2362,30.2362 0 0,0 62.9606,62.6102 A30.2362,30.2362 0 0,0 123.433,62.6102 Z\" />"),
            "{svg}"
        );
        assert!(svg.contains("<path d=\"M282.173,62.6102 A37.7953,37.7953 0 0,0 206.583,62.6102\" />"), "{svg}");
        // An arc and a line meeting end to end become one path.
        let v = DVec2::new;
        let hook = [
            (SketchEntity::Line { a: v(0.0, 0.0), b: v(10.0, 0.0) }, false),
            (SketchEntity::Arc { a: v(10.0, 0.0), mid: v(15.0, 5.0), b: v(10.0, 10.0) }, false),
        ];
        let svg = write_svg(&hook, &SvgOptions::default()).unwrap();
        assert_eq!(svg.matches("<path").count(), 1, "{svg}");
        assert!(svg.contains(" L") && svg.contains(" A18.8976,18.8976 0 0,0 "), "{svg}");
    }

    #[test]
    fn splines_become_exact_beziers() {
        let v = DVec2::new;
        let points = vec![v(0.0, 0.0), v(5.0, 10.0), v(10.0, 2.0), v(15.0, 12.0), v(20.0, 4.0)];
        // The fit spline: each Bézier matches the curve at its ends, a third and two thirds of its knot span.
        let flat: Vec<glam::DVec3> = points.iter().map(|p| p.extend(0.0)).collect();
        let fit = shapecraft_kernel::spline::fit(&flat).unwrap();
        let mut spans: Vec<f64> = fit.knots.clone();
        spans.dedup();
        let p = spline_piece(&SketchEntity::Spline { points: points.clone() }, 0).unwrap();
        assert_eq!(p.segs.len(), spans.len() - 1);
        let mut from = p.start;
        for (s, w) in p.segs.iter().zip(spans.windows(2)) {
            let Seg::Cubic(a, b, to) = *s else { panic!("{s:?}") };
            for t in [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0] {
                let mt = 1.0 - t;
                let bez = from * mt * mt * mt + a * 3.0 * mt * mt * t + b * 3.0 * mt * t * t + to * t * t * t;
                let on = fit.point(w[0] + (w[1] - w[0]) * t).truncate();
                assert!(bez.distance(on) < 1e-9, "{t}: {bez} vs {on}");
            }
            from = to;
        }
        // The control-point spline: its ends are the first and last poles, and it passes the sketch's own samples.
        let p = spline_piece(&SketchEntity::ControlSpline { poles: points.clone() }, 0).unwrap();
        let dense = bspline_points(&points, 1);
        let mut from = p.start;
        for (s, want) in p.segs.iter().zip(dense.iter().skip(1)) {
            let Seg::Cubic(_, _, to) = *s else { panic!("{s:?}") };
            assert!(to.distance(*want) < 1e-9, "{to} vs {want}");
            from = to;
        }
        assert!(from.distance(points[4]) < 1e-9);
        let svg = write_svg(&[(SketchEntity::Spline { points }, false)], &SvgOptions::default()).unwrap();
        assert!(svg.contains("<path d=\"M") && svg.contains(" C"), "{svg}");
    }

    #[test]
    fn nothing_to_draw() {
        assert!(write_svg(&[(SketchEntity::Line { a: DVec2::ZERO, b: DVec2::X }, true)], &SvgOptions::default()).is_none());
    }
}

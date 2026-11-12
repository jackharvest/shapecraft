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

/// SVG sketch export settings: pixels per inch, curve resolution 0 (coarsest) to 100 (finest), and whether
/// connected curves are joined into polylines (on) or written one shape each as lines, arcs and circles (off).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgOptions {
    pub dpi: f64,
    pub resolution: f64,
    pub join: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions { dpi: 96.0, resolution: 100.0, join: true }
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
        SketchEntity::Text { .. } => Vec::new(),
    }
}

/// Joins open pieces end to end (either way round) into chains; a chain that comes back to its start is closed.
fn join(pieces: Vec<Flat>) -> Vec<Flat> {
    const TOL: f64 = 1e-6;
    let (mut done, mut open): (Vec<Flat>, Vec<Vec<DVec2>>) = (Vec::new(), Vec::new());
    for f in pieces {
        if f.closed { done.push(f) } else { open.push(f.pts) }
    }
    while let Some(mut chain) = open.pop() {
        while let (Some(&s), Some(&e)) = (chain.first(), chain.last()) {
            if chain.len() > 2 && s.distance(e) < TOL {
                break;
            }
            let Some(i) = open.iter().position(|p| {
                let (ps, pe) = (p[0], p[p.len() - 1]);
                ps.distance(e) < TOL || pe.distance(e) < TOL || ps.distance(s) < TOL || pe.distance(s) < TOL
            }) else {
                break;
            };
            let mut p = open.swap_remove(i);
            let (ps, pe) = (p[0], p[p.len() - 1]);
            if ps.distance(e) < TOL {
                chain.extend(p.drain(1..));
            } else if pe.distance(e) < TOL {
                p.reverse();
                chain.extend(p.drain(1..));
            } else {
                if ps.distance(s) < TOL {
                    p.reverse();
                }
                p.pop();
                p.append(&mut chain);
                chain = p;
            }
        }
        let closed = chain.len() > 2 && chain[0].distance(chain[chain.len() - 1]) < TOL;
        if closed {
            chain.pop();
        }
        done.push(Flat { pts: chain, closed });
    }
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

/// Sketch curves (already in one shared plane's x/y, millimetres) as an SVG image: user units are pixels at
/// `dpi`, y points down, a black hairline with no fill, and the drawing centred in a view box 10% larger than
/// it. Construction curves and text are left out. `None` when there's nothing to draw.
pub fn write_svg(entities: &[(SketchEntity, bool)], o: &SvgOptions) -> Option<String> {
    let step = o.step();
    let dpi = if o.dpi.is_finite() && o.dpi > 0.0 { o.dpi.min(10_000.0) } else { 96.0 };
    let k = dpi / 25.4;
    let curves: Vec<&SketchEntity> =
        entities.iter().filter(|(e, c)| !c && !matches!(e, SketchEntity::Text { .. })).map(|(e, _)| e).collect();
    let flats: Vec<Flat> = curves.iter().flat_map(|e| flatten(e, step)).collect();
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in flats.iter().flat_map(|f| f.pts.iter()) {
        lo = lo.min(*p);
        hi = hi.max(*p);
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
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\"?>\n<svg viewBox=\"0 0 {} {}\" version=\"1.1\" \
         xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\" fill-rule=\"evenodd\" xml:space=\"preserve\" >\n<g fill=\"none\" stroke=\"rgb(0,0,0)\" stroke-width=\"{}\" >\n",
        num(w),
        num(h),
        num(0.37 * dpi / 96.0)
    );
    let poly = |out: &mut String, f: &Flat| {
        let tag = if f.closed { "polygon" } else { "polyline" };
        let pts: String = f.pts.iter().map(|p| pt(*p) + " ").collect();
        let _ = writeln!(out, "<{tag} points=\"{pts}\" />");
    };
    if o.join {
        for f in &join(flats) {
            poly(&mut out, f);
        }
    } else {
        // One shape per curve: lines as two-point polylines, arcs as paths, circles and ellipses as themselves.
        for e in curves {
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
                        for i in 0..n {
                            poly(&mut out, &Flat { pts: vec![f.pts[i], f.pts[(i + 1) % n]], closed: false });
                        }
                    }
                }
                _ => {
                    for f in flatten(e, step) {
                        poly(&mut out, &f);
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
        let svg = write_svg(&sample(), &SvgOptions::default()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 296 129\""), "{svg}");
        assert!(svg.contains("stroke-width=\"0.37\""));
        let mut c = counts(&svg);
        c.sort();
        let want = [("polygon", 4), ("polygon", 629), ("polygon", 629), ("polyline", 316)];
        assert_eq!(c, want.iter().map(|(t, n)| (t.to_string(), *n)).collect::<Vec<_>>());
        // The rectangle's corners, in pixels from the view box's top left (the last digit can differ by one).
        for corner in ["17.6063,5.9173", "17.6063,119.303", "168.787,119.303", "168.787,5.9173"] {
            assert!(svg.contains(corner), "{corner} in {svg}");
        }
    }

    #[test]
    fn dpi_and_resolution() {
        let svg = write_svg(&sample(), &SvgOptions { dpi: 72.0, resolution: 10.0, join: true }).unwrap();
        assert!(svg.contains("viewBox=\"0 0 222 97\""));
        assert!(svg.contains("stroke-width=\"0.2775\""));
        let mut n: Vec<usize> = counts(&svg).into_iter().map(|c| c.1).collect();
        n.sort();
        assert_eq!(n, [4, 36, 70, 70]);
    }

    #[test]
    fn unjoined_shapes() {
        let svg = write_svg(&sample(), &SvgOptions { join: false, ..SvgOptions::default() }).unwrap();
        assert!(svg.contains("<polyline points=\"17.6063,119.303 168.787,119.303 \" />"), "{svg}");
        assert!(svg.contains("<path d=\"M282.173,62.6102  A37.7953,37.7953 0 0,0 206.583,62.6102 \" />"));
        assert!(svg.contains("<circle cx=\"93.196") && svg.contains("cy=\"62.6102\" r=\"30.2362\" />"), "{svg}");
        assert!(svg.contains("<circle cx=\"17.6063\" cy=\"119.303\" r=\"3.77953\" />"));
    }

    #[test]
    fn nothing_to_draw() {
        assert!(write_svg(&[(SketchEntity::Line { a: DVec2::ZERO, b: DVec2::X }, true)], &SvgOptions::default()).is_none());
    }
}

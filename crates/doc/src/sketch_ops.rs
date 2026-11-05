//! Trim and Offset Edge: editing sketch curves in sketch coordinates.

use std::f64::consts::{PI, TAU};

use glam::{DAffine2, DVec2};

use crate::sketch::{SketchEntity, circle_through};

const EPS: f64 = 1e-9;

/// A piece of a curve, for intersection tests.
#[derive(Clone, Copy, Debug)]
enum Prim {
    Seg(DVec2, DVec2),
    /// Center, radius, start angle, signed sweep (±TAU for a full circle).
    Arc(DVec2, f64, f64, f64),
}

/// Center, radius, start angle and signed sweep of the arc a → mid → b.
fn arc_frame(a: DVec2, mid: DVec2, b: DVec2) -> Option<(DVec2, f64, f64, f64)> {
    let (c, r) = circle_through(a, mid, b)?;
    let ang = |p: DVec2| (p - c).to_angle();
    let (t0, tm, t1) = (ang(a), ang(mid), ang(b));
    let ccw_span = (t1 - t0).rem_euclid(TAU);
    let ccw = (tm - t0).rem_euclid(TAU) < ccw_span;
    let sweep = if ccw { ccw_span } else { -(TAU - ccw_span) };
    Some((c, r, t0, sweep))
}

fn segs(points: &[DVec2]) -> Vec<Prim> {
    points.windows(2).map(|w| Prim::Seg(w[0], w[1])).collect()
}

fn prims(e: &SketchEntity) -> Vec<Prim> {
    match *e {
        SketchEntity::Line { a, b } => vec![Prim::Seg(a, b)],
        SketchEntity::Circle { center, radius } => vec![Prim::Arc(center, radius, 0.0, TAU)],
        SketchEntity::Arc { a, mid, b } => match arc_frame(a, mid, b) {
            Some((c, r, t0, sw)) => vec![Prim::Arc(c, r, t0, sw)],
            None => vec![Prim::Seg(a, b)],
        },
        // Exact for the straight ones; a fine polyline for ellipses, splines and letters.
        _ => e.outlines().iter().flat_map(|o| segs(o)).collect(),
    }
}

/// Where along the arc `p` (on its circle) lies, from 0 at the start to 1 at the end; `None` if off the arc.
fn arc_param(c: DVec2, start: f64, sweep: f64, p: DVec2) -> Option<f64> {
    let a = (p - c).to_angle();
    let mut d = if sweep >= 0.0 { (a - start).rem_euclid(TAU) } else { (start - a).rem_euclid(TAU) };
    if d > TAU - 1e-9 {
        d = 0.0;
    }
    (d <= sweep.abs() + 1e-9).then(|| (d / sweep.abs()).min(1.0))
}

fn seg_param(a: DVec2, b: DVec2, p: DVec2) -> f64 {
    let d = b - a;
    let l2 = d.length_squared();
    if l2 < EPS * EPS { 0.0 } else { (p - a).dot(d) / l2 }
}

fn line_circle(a: DVec2, b: DVec2, c: DVec2, r: f64) -> Vec<DVec2> {
    let d = b - a;
    let f = a - c;
    let (qa, qb, qc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - r * r);
    if qa < EPS * EPS {
        return Vec::new();
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < -1e-9 * qa.max(1.0) {
        return Vec::new();
    }
    let s = disc.max(0.0).sqrt();
    let mut out = vec![a + d * ((-qb - s) / (2.0 * qa))];
    if s > 1e-12 {
        out.push(a + d * ((-qb + s) / (2.0 * qa)));
    }
    out
}

fn circle_circle(c0: DVec2, r0: f64, c1: DVec2, r1: f64) -> Vec<DVec2> {
    let d = c0.distance(c1);
    if d < EPS || d > r0 + r1 + 1e-9 || d < (r0 - r1).abs() - 1e-9 {
        return Vec::new();
    }
    let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let dir = (c1 - c0) / d;
    let m = c0 + dir * a;
    let perp = dir.perp() * h;
    if h < 1e-12 { vec![m] } else { vec![m + perp, m - perp] }
}

fn intersections(p: Prim, q: Prim) -> Vec<DVec2> {
    let on = |pr: Prim, x: DVec2| match pr {
        Prim::Seg(a, b) => (-1e-9..=1.0 + 1e-9).contains(&seg_param(a, b, x)),
        Prim::Arc(c, _, s, sw) => arc_param(c, s, sw, x).is_some(),
    };
    let raw = match (p, q) {
        (Prim::Seg(a, b), Prim::Seg(c, d)) => {
            let (r, s) = (b - a, d - c);
            let den = r.perp_dot(s);
            if den.abs() < EPS * r.length().max(1.0) * s.length().max(1.0) {
                // Parallel: overlapping ends still count as meeting points.
                [c, d].into_iter().filter(|x| (x - a).perp_dot(r).abs() < 1e-9 * r.length().max(1.0)).chain([a, b]).collect()
            } else {
                vec![a + r * ((c - a).perp_dot(s) / den)]
            }
        }
        (Prim::Seg(a, b), Prim::Arc(c, r, ..)) | (Prim::Arc(c, r, ..), Prim::Seg(a, b)) => line_circle(a, b, c, r),
        (Prim::Arc(c0, r0, ..), Prim::Arc(c1, r1, ..)) => circle_circle(c0, r0, c1, r1),
    };
    raw.into_iter().filter(|x| on(p, *x) && on(q, *x)).collect()
}

/// Where the elements' curves cross each other (not where an element meets itself, and not shared ends: those are
/// key points already), for snapping while drawing. At most `cap` points.
pub fn crossings(entities: &[SketchEntity], cap: usize) -> Vec<DVec2> {
    let all: Vec<(usize, Prim)> = entities.iter().enumerate().flat_map(|(i, e)| prims(e).into_iter().map(move |p| (i, p))).collect();
    let ends: Vec<DVec2> = entities.iter().flat_map(crate::constraints::points).collect();
    let mut out: Vec<DVec2> = Vec::new();
    for (k, &(i, p)) in all.iter().enumerate() {
        for &(j, q) in &all[k + 1..] {
            if i == j {
                continue;
            }
            for x in intersections(p, q) {
                let size = x.length().max(1.0) * 1e-7;
                if !ends.iter().any(|e| e.distance(x) < size) && !out.iter().any(|o| o.distance(x) < size) {
                    out.push(x);
                    if out.len() >= cap {
                        return out;
                    }
                }
            }
        }
    }
    out
}

/// Where other curves cross the target: parameter along it (0..1) and the exact point.
fn cuts(target: Prim, cutters: &[Prim]) -> Vec<(f64, DVec2)> {
    let mut ts: Vec<(f64, DVec2)> = cutters
        .iter()
        .flat_map(|q| intersections(target, *q))
        .filter_map(|x| match target {
            Prim::Seg(a, b) => Some((seg_param(a, b, x).clamp(0.0, 1.0), x)),
            Prim::Arc(c, _, s, sw) => arc_param(c, s, sw, x).map(|t| (t, x)),
        })
        .collect();
    ts.sort_by(|a, b| a.0.total_cmp(&b.0));
    ts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-7);
    ts
}

fn cut_params(target: Prim, cutters: &[Prim]) -> Vec<f64> {
    cuts(target, cutters).into_iter().map(|c| c.0).collect()
}

fn arc_point(c: DVec2, r: f64, start: f64, sweep: f64, t: f64) -> DVec2 {
    c + DVec2::from_angle(start + sweep * t) * r
}

fn sub_arc(c: DVec2, r: f64, start: f64, sweep: f64, t0: f64, t1: f64) -> SketchEntity {
    let p = |t| arc_point(c, r, start, sweep, t);
    SketchEntity::Arc { a: p(t0), mid: p((t0 + t1) / 2.0), b: p(t1) }
}

/// Rectangles and polygons become their sides, so one side can be trimmed.
fn explode(e: &SketchEntity) -> Option<Vec<SketchEntity>> {
    match e {
        SketchEntity::Rectangle { .. } | SketchEntity::Polygon { .. } => {
            Some(e.outline().windows(2).map(|w| SketchEntity::Line { a: w[0], b: w[1] }).collect())
        }
        _ => None,
    }
}

fn distance_to(e: &SketchEntity, p: DVec2) -> f64 {
    e.outline()
        .windows(2)
        .map(|w| {
            let t = seg_param(w[0], w[1], p).clamp(0.0, 1.0);
            p.distance(w[0].lerp(w[1], t))
        })
        .fold(f64::INFINITY, f64::min)
}

/// Removes the piece of curve `index` around `at`, between the nearest crossings with the other curves
/// (the whole curve if nothing crosses it). Returns the sketch's new entity list and the piece that goes.
pub fn trim(entities: &[SketchEntity], index: usize, at: DVec2) -> Result<(Vec<SketchEntity>, SketchEntity), &'static str> {
    let target = entities.get(index).ok_or("no such sketch curve")?;
    if let Some(sides) = explode(target) {
        let pick = sides.iter().enumerate().min_by(|a, b| distance_to(a.1, at).total_cmp(&distance_to(b.1, at))).map_or(0, |(i, _)| i);
        let mut list: Vec<SketchEntity> = entities[..index].to_vec();
        list.extend(sides.iter().cloned());
        list.extend(entities[index + 1..].iter().cloned());
        return trim(&list, index + pick, at);
    }
    let cutters: Vec<Prim> = entities.iter().enumerate().filter(|(j, _)| *j != index).flat_map(|(_, e)| prims(e)).collect();
    let (pieces, removed): (Vec<SketchEntity>, SketchEntity) = match *target {
        SketchEntity::Line { a, b } => {
            let mut ts = cuts(Prim::Seg(a, b), &cutters);
            ts.extend([(0.0, a), (1.0, b)]);
            let tc = seg_param(a, b, at).clamp(0.0, 1.0);
            // Exact crossing points, so the pieces still close regions.
            let lo = ts.iter().copied().filter(|t| t.0 <= tc).max_by(|x, y| x.0.total_cmp(&y.0)).unwrap_or((0.0, a));
            let hi = ts.iter().copied().filter(|t| t.0 >= tc).min_by(|x, y| x.0.total_cmp(&y.0)).unwrap_or((1.0, b));
            let mut out = Vec::new();
            if lo.0 > 1e-7 {
                out.push(SketchEntity::Line { a, b: lo.1 });
            }
            if hi.0 < 1.0 - 1e-7 {
                out.push(SketchEntity::Line { a: hi.1, b });
            }
            (out, SketchEntity::Line { a: lo.1, b: hi.1 })
        }
        SketchEntity::Arc { a, mid, b } => {
            let Some((c, r, s, sw)) = arc_frame(a, mid, b) else { return Err("this arc is degenerate") };
            let ts = cut_params(Prim::Arc(c, r, s, sw), &cutters);
            let tc = arc_param(c, s, sw, at).unwrap_or_else(|| if at.distance(a) < at.distance(b) { 0.0 } else { 1.0 });
            let lo = ts.iter().copied().filter(|t| *t <= tc).fold(0.0, f64::max);
            let hi = ts.iter().copied().filter(|t| *t >= tc).fold(1.0, f64::min);
            let mut out = Vec::new();
            if lo > 1e-7 {
                out.push(sub_arc(c, r, s, sw, 0.0, lo));
            }
            if hi < 1.0 - 1e-7 {
                out.push(sub_arc(c, r, s, sw, hi, 1.0));
            }
            (out, sub_arc(c, r, s, sw, lo, hi))
        }
        SketchEntity::Circle { center, radius } => {
            let ts = cut_params(Prim::Arc(center, radius, 0.0, TAU), &cutters);
            let mut ts: Vec<f64> = ts.into_iter().map(|t| t.rem_euclid(1.0)).collect();
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
            if ts.len() < 2 {
                (Vec::new(), target.clone())
            } else {
                let tc = ((at - center).to_angle() / TAU).rem_euclid(1.0);
                // The crossing just before the click and the one just after (wrapping around).
                let before = ts.iter().rev().copied().find(|t| *t <= tc).unwrap_or(ts[ts.len() - 1]);
                let after = ts.iter().copied().find(|t| *t > tc).unwrap_or(ts[0]);
                let keep = (before - after).rem_euclid(1.0);
                let gone = (after - before).rem_euclid(1.0);
                (
                    vec![sub_arc(center, radius, after * TAU, keep * TAU, 0.0, 1.0)],
                    sub_arc(center, radius, before * TAU, gone * TAU, 0.0, 1.0),
                )
            }
        }
        SketchEntity::Ellipse { .. } | SketchEntity::Spline { .. } | SketchEntity::ControlSpline { .. } | SketchEntity::Text { .. } => {
            let crossed = prims(target).iter().any(|p| !cut_params(*p, &cutters).is_empty());
            if crossed {
                return Err("Trim works on lines, arcs, circles, rectangles and polygons so far");
            }
            (Vec::new(), target.clone())
        }
        SketchEntity::Rectangle { .. } | SketchEntity::Polygon { .. } => return Err("could not split the shape"),
    };
    let mut list: Vec<SketchEntity> = entities[..index].to_vec();
    list.extend(pieces);
    list.extend(entities[index + 1..].iter().cloned());
    Ok((list, removed))
}

/// A copy of the curve moved sideways by `d`: outwards for closed shapes and arcs (away from the center),
/// to the left of a → b for lines. `None` when it would collapse.
pub fn offset(e: &SketchEntity, d: f64) -> Option<SketchEntity> {
    if !d.is_finite() {
        return None;
    }
    Some(match *e {
        SketchEntity::Line { a, b } => {
            let n = (b - a).try_normalize()?.perp() * d;
            SketchEntity::Line { a: a + n, b: b + n }
        }
        SketchEntity::Rectangle { min, max } => {
            let (min, max) = (min - DVec2::splat(d), max + DVec2::splat(d));
            if max.x - min.x < EPS || max.y - min.y < EPS {
                return None;
            }
            SketchEntity::Rectangle { min, max }
        }
        SketchEntity::Circle { center, radius } => {
            let r = radius + d;
            (r > EPS).then_some(SketchEntity::Circle { center, radius: r })?
        }
        SketchEntity::Polygon { center, radius, sides, angle } => {
            // Sides move by d, so corners move by d / cos(half the corner angle at the center).
            let r = radius + d / (PI / sides.clamp(3, 64) as f64).cos();
            (r > EPS).then_some(SketchEntity::Polygon { center, radius: r, sides, angle })?
        }
        SketchEntity::Arc { a, mid, b } => {
            let (c, r) = circle_through(a, mid, b)?;
            let k = (r + d) / r;
            if r + d <= EPS {
                return None;
            }
            let s = |p: DVec2| c + (p - c) * k;
            SketchEntity::Arc { a: s(a), mid: s(mid), b: s(b) }
        }
        SketchEntity::Spline { ref points } => {
            let n = points.len();
            if n < 2 {
                return None;
            }
            let moved = (0..n)
                .map(|i| {
                    let t = points[(i + 1).min(n - 1)] - points[i.saturating_sub(1)];
                    t.try_normalize().map(|t| points[i] + t.perp() * d)
                })
                .collect::<Option<Vec<_>>>()?;
            SketchEntity::Spline { points: moved }
        }
        // Poles moved along the control polygon's normals: close to an offset for gentle curves.
        SketchEntity::ControlSpline { ref poles } => match offset(&SketchEntity::Spline { points: poles.clone() }, d)? {
            SketchEntity::Spline { points } => SketchEntity::ControlSpline { poles: points },
            _ => return None,
        },
        // The offset of an ellipse isn't an ellipse; not drawn yet. Text doesn't offset.
        SketchEntity::Ellipse { .. } | SketchEntity::Text { .. } => return None,
    })
}

/// A projected curve as a sketch element: a line when the points lie on one (an edge seen side-on), else a
/// spline through them. `None` if it collapses to a point.
pub fn fit_polyline(points: &[DVec2]) -> Option<SketchEntity> {
    let mut pts: Vec<DVec2> = Vec::with_capacity(points.len());
    for p in points {
        if pts.last().is_none_or(|q| q.distance(*p) > 1e-9) {
            pts.push(*p);
        }
    }
    let first = *pts.first()?;
    let far = pts.iter().copied().max_by(|a, b| a.distance(first).total_cmp(&b.distance(first)))?;
    let size = first.distance(far);
    if size < 1e-9 {
        return None;
    }
    let dir = (far - first) / size;
    if pts.iter().all(|p| (*p - first).perp_dot(dir).abs() < 1e-6 * size.max(1.0)) {
        // Straight: span the extremes along the line.
        let (lo, hi) = pts.iter().fold((0.0f64, 0.0f64), |(lo, hi), p| {
            let t = (*p - first).dot(dir);
            (lo.min(t), hi.max(t))
        });
        return Some(SketchEntity::Line { a: first + dir * lo, b: first + dir * hi });
    }
    if pts.len() > 1000 {
        return None;
    }
    Some(SketchEntity::Spline { points: pts })
}

/// The curve moved by a rigid map (move, turn or mirror) in sketch coordinates. A rectangle that no longer
/// lines up with the sketch axes comes back as its four sides.
pub fn transform(e: &SketchEntity, m: DAffine2) -> Vec<SketchEntity> {
    let p = |q: DVec2| m.transform_point2(q);
    let angle_of = |a: f64| m.transform_vector2(DVec2::from_angle(a)).to_angle();
    // How much a scale grows sizes (1 for moves, turns and mirrors): radii, text heights. A non-uniform scale
    // grows them by the average.
    let k = m.matrix2.determinant().abs().sqrt();
    match *e {
        SketchEntity::Line { a, b } => vec![SketchEntity::Line { a: p(a), b: p(b) }],
        SketchEntity::Circle { center, radius } => vec![SketchEntity::Circle { center: p(center), radius: radius * k }],
        SketchEntity::Arc { a, mid, b } => vec![SketchEntity::Arc { a: p(a), mid: p(mid), b: p(b) }],
        SketchEntity::Spline { ref points } => vec![SketchEntity::Spline { points: points.iter().map(|q| p(*q)).collect() }],
        SketchEntity::ControlSpline { ref poles } => vec![SketchEntity::ControlSpline { poles: poles.iter().map(|q| p(*q)).collect() }],
        SketchEntity::Polygon { center, radius, sides, angle } => {
            vec![SketchEntity::Polygon { center: p(center), radius: radius * k, sides, angle: angle_of(angle) }]
        }
        SketchEntity::Ellipse { center, rx, ry, angle } => {
            vec![SketchEntity::Ellipse { center: p(center), rx: rx * k, ry: ry * k, angle: angle_of(angle) }]
        }
        // Letters move and turn but never read mirrored.
        SketchEntity::Text { origin, ref text, size, angle, ref font } => {
            vec![SketchEntity::Text { origin: p(origin), text: text.clone(), size: size * k, angle: angle_of(angle), font: font.clone() }]
        }
        SketchEntity::Rectangle { min, max } => {
            // Still square to the sketch's axes (moved, scaled, turned by a quarter): still a rectangle.
            let (x, y) = (m.transform_vector2(DVec2::X), m.transform_vector2(DVec2::Y));
            let tiny = |v: f64, of: DVec2| v.abs() <= 1e-9 * of.length().max(1e-300);
            if (tiny(x.y, x) && tiny(y.x, y)) || (tiny(x.x, x) && tiny(y.y, y)) {
                vec![SketchEntity::rectangle(p(min), p(max))]
            } else {
                e.outline().windows(2).map(|w| SketchEntity::Line { a: p(w[0]), b: p(w[1]) }).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    #[test]
    fn scaling_grows_radii_and_keeps_rectangles() {
        let half = DAffine2::from_scale(v(0.5, 0.5));
        assert_eq!(
            transform(&SketchEntity::Circle { center: v(4.0, 0.0), radius: 2.0 }, half),
            vec![SketchEntity::Circle { center: v(2.0, 0.0), radius: 1.0 }]
        );
        let r = transform(&SketchEntity::rectangle(v(0.0, 0.0), v(4.0, 2.0)), half);
        assert_eq!(r, vec![SketchEntity::rectangle(v(0.0, 0.0), v(2.0, 1.0))], "a shrunk rectangle stays one rectangle");
        let turned = transform(&SketchEntity::Circle { center: v(0.0, 0.0), radius: 3.0 }, DAffine2::from_angle(1.0));
        assert!(matches!(turned[0], SketchEntity::Circle { radius, .. } if (radius - 3.0).abs() < 1e-12), "a turn keeps the radius");
    }

    #[test]
    fn trim_the_middle_of_a_crossed_line() {
        let es = vec![
            SketchEntity::Line { a: v(0.0, 0.0), b: v(10.0, 0.0) },
            SketchEntity::Line { a: v(3.0, -1.0), b: v(3.0, 1.0) },
            SketchEntity::Line { a: v(7.0, -1.0), b: v(7.0, 1.0) },
        ];
        let out = trim(&es, 0, v(5.0, 0.1)).unwrap().0;
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], SketchEntity::Line { a: v(0.0, 0.0), b: v(3.0, 0.0) });
        assert_eq!(out[1], SketchEntity::Line { a: v(7.0, 0.0), b: v(10.0, 0.0) });
        // An end piece goes, and an uncrossed line goes entirely.
        assert_eq!(trim(&es, 0, v(1.0, 0.0)).unwrap().0[0], SketchEntity::Line { a: v(3.0, 0.0), b: v(10.0, 0.0) });
        assert_eq!(trim(&es[..1], 0, v(1.0, 0.0)).unwrap().0.len(), 0);
    }

    #[test]
    fn trim_a_circle_across_a_line_leaves_an_arc() {
        let es = vec![SketchEntity::Circle { center: v(0.0, 0.0), radius: 5.0 }, SketchEntity::Line { a: v(-6.0, 0.0), b: v(6.0, 0.0) }];
        let (out, gone) = trim(&es, 0, v(0.0, 5.0)).unwrap();
        let SketchEntity::Arc { mid: gm, .. } = gone else { panic!() };
        assert!(gm.distance(v(0.0, 5.0)) < 1e-9);
        let SketchEntity::Arc { a, mid, b } = out[0] else { panic!("{out:?}") };
        assert!(mid.distance(v(0.0, -5.0)) < 1e-9, "the lower half stays: {mid}");
        assert!((a.y.abs() + b.y.abs()) < 1e-9);
    }

    #[test]
    fn trim_one_side_of_a_rectangle() {
        let es =
            vec![SketchEntity::Rectangle { min: v(0.0, 0.0), max: v(10.0, 4.0) }, SketchEntity::Line { a: v(5.0, -1.0), b: v(5.0, 5.0) }];
        let out = trim(&es, 0, v(2.0, 0.0)).unwrap().0;
        // Four sides, one of them shortened, plus the cutting line.
        assert_eq!(out.len(), 5);
        assert_eq!(out[0], SketchEntity::Line { a: v(5.0, 0.0), b: v(10.0, 0.0) });
    }

    #[test]
    fn trim_an_arc_between_crossings() {
        let es = vec![
            SketchEntity::Arc { a: v(5.0, 0.0), mid: v(0.0, 5.0), b: v(-5.0, 0.0) },
            SketchEntity::Line { a: v(0.0, 0.0), b: v(0.0, 10.0) },
        ];
        let out = trim(&es, 0, v(3.0, 4.0)).unwrap().0;
        assert_eq!(out.len(), 2);
        let SketchEntity::Arc { a, b, .. } = out[0] else { panic!() };
        assert!(a.distance(v(0.0, 5.0)) < 1e-9 && b.distance(v(-5.0, 0.0)) < 1e-9, "{a} {b}");
    }

    #[test]
    fn transforms() {
        let quarter = DAffine2::from_angle(std::f64::consts::FRAC_PI_2);
        let r = transform(&SketchEntity::Rectangle { min: v(0.0, 0.0), max: v(2.0, 1.0) }, quarter);
        let SketchEntity::Rectangle { min, max } = r[0] else { panic!("{r:?}") };
        assert!(min.distance(v(-1.0, 0.0)) < 1e-9 && max.distance(v(0.0, 2.0)) < 1e-9);
        assert_eq!(transform(&SketchEntity::Rectangle { min: v(0.0, 0.0), max: v(2.0, 1.0) }, DAffine2::from_angle(0.3)).len(), 4);
        // Mirror in the y axis: a polygon's first corner flips with it.
        let flip = DAffine2::from_scale(v(-1.0, 1.0));
        let SketchEntity::Polygon { center, angle, .. } =
            transform(&SketchEntity::Polygon { center: v(3.0, 0.0), radius: 1.0, sides: 5, angle: 0.2 }, flip)[0]
        else {
            panic!()
        };
        assert!(center.distance(v(-3.0, 0.0)) < 1e-9 && (angle - (std::f64::consts::PI - 0.2)).abs() < 1e-9);
    }

    #[test]
    fn offsets() {
        let l = offset(&SketchEntity::Line { a: v(0.0, 0.0), b: v(10.0, 0.0) }, 2.0).unwrap();
        assert_eq!(l, SketchEntity::Line { a: v(0.0, 2.0), b: v(10.0, 2.0) });
        let r = offset(&SketchEntity::Rectangle { min: v(0.0, 0.0), max: v(4.0, 4.0) }, -1.0).unwrap();
        assert_eq!(r, SketchEntity::Rectangle { min: v(1.0, 1.0), max: v(3.0, 3.0) });
        assert!(offset(&SketchEntity::Circle { center: v(0.0, 0.0), radius: 1.0 }, -2.0).is_none());
        let SketchEntity::Polygon { radius, .. } =
            offset(&SketchEntity::Polygon { center: v(0.0, 0.0), radius: 2.0, sides: 4, angle: 0.0 }, 1.0).unwrap()
        else {
            panic!()
        };
        assert!((radius - (2.0 + std::f64::consts::SQRT_2)).abs() < 1e-9);
        let SketchEntity::Arc { mid, .. } = offset(&SketchEntity::Arc { a: v(5.0, 0.0), mid: v(0.0, 5.0), b: v(-5.0, 0.0) }, 1.0).unwrap()
        else {
            panic!()
        };
        assert!(mid.distance(v(0.0, 6.0)) < 1e-9);
    }
}

/// Corners where exactly two lines meet end to end (the points a sketch fillet can round), among `only` when
/// given (indices of the lines to consider).
pub fn line_corners(entities: &[SketchEntity], only: Option<&[usize]>) -> Vec<DVec2> {
    const TOL: f64 = 1e-7;
    let use_line = |i: usize| only.is_none_or(|o| o.contains(&i));
    let mut ends: Vec<DVec2> = Vec::new();
    for (i, e) in entities.iter().enumerate() {
        if let SketchEntity::Line { a, b } = e
            && use_line(i)
        {
            ends.push(*a);
            ends.push(*b);
        }
    }
    let mut out: Vec<DVec2> = Vec::new();
    for p in &ends {
        let n = ends.iter().filter(|q| q.distance(*p) < TOL).count();
        if n == 2 && !out.iter().any(|q| q.distance(*p) < TOL) {
            out.push(*p);
        }
    }
    out
}

/// Rounds each corner (where exactly two lines meet) with a tangent arc of radius `r`: both lines are cut back to
/// where the arc touches them and the arc is added. Corners too tight for `r`, or straight, are left alone.
pub fn fillet_corners(entities: &[SketchEntity], corners: &[DVec2], r: f64) -> Result<Vec<SketchEntity>, &'static str> {
    const TOL: f64 = 1e-7;
    if !(r.is_finite() && r > 1e-9) {
        return Err("the fillet radius must be positive");
    }
    let mut list = entities.to_vec();
    let mut done = 0;
    for &p in corners {
        // The two lines ending at the corner, and their far ends.
        let at: Vec<(usize, DVec2)> = list
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match *e {
                SketchEntity::Line { a, b } if a.distance(p) < TOL => Some((i, b)),
                SketchEntity::Line { a, b } if b.distance(p) < TOL => Some((i, a)),
                _ => None,
            })
            .collect();
        let [(i, fa), (j, fb)] = at[..] else { continue };
        let (Some(u), Some(v)) = ((fa - p).try_normalize(), (fb - p).try_normalize()) else { continue };
        let theta = u.dot(v).clamp(-1.0, 1.0).acos();
        if theta < 1e-3 || (std::f64::consts::PI - theta) < 1e-3 {
            continue;
        }
        let t = r / (theta / 2.0).tan();
        if t >= p.distance(fa) - TOL || t >= p.distance(fb) - TOL {
            continue;
        }
        let (pa, pb) = (p + u * t, p + v * t);
        let Some(bis) = (u + v).try_normalize() else { continue };
        let c = p + bis * (r / (theta / 2.0).sin());
        let mid = c - bis * r;
        list[i] = SketchEntity::Line { a: pa, b: fa };
        list[j] = SketchEntity::Line { a: pb, b: fb };
        list.push(SketchEntity::Arc { a: pa, mid, b: pb });
        done += 1;
    }
    if done == 0 {
        return Err("no corner could be rounded with that radius");
    }
    Ok(list)
}

#[cfg(test)]
mod fillet_tests {
    use super::*;

    #[test]
    fn a_square_corner_gets_a_tangent_arc() {
        let l = vec![
            SketchEntity::Line { a: DVec2::ZERO, b: DVec2::new(10.0, 0.0) },
            SketchEntity::Line { a: DVec2::ZERO, b: DVec2::new(0.0, 10.0) },
        ];
        assert_eq!(line_corners(&l, None), vec![DVec2::ZERO]);
        let out = fillet_corners(&l, &[DVec2::ZERO], 2.0).unwrap();
        assert_eq!(out.len(), 3);
        let SketchEntity::Arc { a, mid, b } = out[2] else { panic!("no arc") };
        assert!(a.distance(DVec2::new(2.0, 0.0)) < 1e-9 && b.distance(DVec2::new(0.0, 2.0)) < 1e-9);
        assert!(mid.distance(DVec2::splat(2.0) - DVec2::splat(2.0_f64.sqrt())) < 1e-9);
        assert!(fillet_corners(&l, &[DVec2::ZERO], 20.0).is_err());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod crossing_tests {
    use super::*;

    #[test]
    fn crossings_skip_shared_ends() {
        let l = |a: (f64, f64), b: (f64, f64)| SketchEntity::Line { a: DVec2::new(a.0, a.1), b: DVec2::new(b.0, b.1) };
        // An X and a line meeting the X's end: only the middle of the X counts.
        let es = vec![l((-1.0, -1.0), (1.0, 1.0)), l((-1.0, 1.0), (1.0, -1.0)), l((1.0, 1.0), (3.0, 1.0))];
        let x = crossings(&es, 10);
        assert_eq!(x.len(), 1);
        assert!(x[0].length() < 1e-9);
        // A circle through a line: two points.
        let es = vec![l((-5.0, 0.0), (5.0, 0.0)), SketchEntity::Circle { center: DVec2::ZERO, radius: 2.0 }];
        assert_eq!(crossings(&es, 10).len(), 2);
    }
}

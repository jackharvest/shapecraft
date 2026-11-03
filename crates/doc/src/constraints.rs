//! Sketch constraints and the solver that keeps them. A constraint ties parts of sketch elements together
//! (an endpoint, a side, a circle); the solver nudges the elements' parameters until every constraint holds,
//! moving as little as possible and leaving locked and pinned elements where they are.

use std::collections::BTreeSet;

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::sketch::{SketchEntity, circle_through};

/// A part of a sketch element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Element {
    /// An endpoint, corner or center (see [`point`]).
    Point { entity: u32, index: u32 },
    /// A straight side (see [`segment`]).
    Segment { entity: u32, index: u32 },
    /// A circle or arc.
    Curve { entity: u32 },
    /// The origin as it lies in the sketch, in nanometres (whole numbers, so elements compare exactly): a point
    /// drawn onto it is held there, which is what makes a shape drawn from the origin fully defined.
    Origin { u: i64, v: i64 },
}

/// What [`Element::entity`] gives for the origin, which belongs to no curve.
pub const NO_ENTITY: u32 = u32::MAX;

impl Element {
    pub fn entity(self) -> u32 {
        match self {
            Element::Point { entity, .. } | Element::Segment { entity, .. } | Element::Curve { entity } => entity,
            Element::Origin { .. } => NO_ENTITY,
        }
    }

    /// The origin at sketch point `p`, or `None` when it is out of range.
    pub fn origin_at(p: DVec2) -> Option<Self> {
        let nm = |x: f64| (x.is_finite() && x.abs() < 1e9).then(|| (x * 1e6).round() as i64);
        Some(Element::Origin { u: nm(p.x)?, v: nm(p.y)? })
    }

    fn with_entity(self, e: u32) -> Self {
        match self {
            Element::Point { index, .. } => Element::Point { entity: e, index },
            Element::Segment { index, .. } => Element::Segment { entity: e, index },
            Element::Curve { .. } => Element::Curve { entity: e },
            Element::Origin { .. } => self,
        }
    }
}

/// How a length dimension measures its two ends (its Distance Type badge): straight between them, or only across
/// the sketch's horizontal or vertical axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Along {
    #[default]
    Absolute,
    Horizontal,
    Vertical,
}

impl Along {
    pub fn is_absolute(&self) -> bool {
        *self == Along::Absolute
    }

    /// The distance from `p` to `q` this way.
    pub fn measure(self, p: DVec2, q: DVec2) -> f64 {
        match self {
            Along::Absolute => p.distance(q),
            Along::Horizontal => (q.x - p.x).abs(),
            Along::Vertical => (q.y - p.y).abs(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    Horizontal {
        a: Element,
    },
    Vertical {
        a: Element,
    },
    Parallel {
        a: Element,
        b: Element,
    },
    Perpendicular {
        a: Element,
        b: Element,
    },
    /// Same length (segments) or radius (curves).
    Equal {
        a: Element,
        b: Element,
    },
    /// A point on a point, on a segment's line, or on a curve.
    Coincident {
        a: Element,
        b: Element,
    },
    /// A point at a segment's middle.
    Midpoint {
        a: Element,
        b: Element,
    },
    Concentric {
        a: Element,
        b: Element,
    },
    /// A segment touching a curve, or two curves touching (`inside`: one within the other).
    Tangent {
        a: Element,
        b: Element,
        inside: bool,
    },
    /// Two points mirrored about a segment's line.
    Symmetric {
        a: Element,
        b: Element,
        axis: Element,
    },
    /// The element doesn't move.
    Lock {
        entity: u32,
    },
    /// A driving dimension: a segment's length (a typed line length, a rectangle's width or height, s11-037/056).
    Distance {
        a: Element,
        value: f64,
        /// Measured straight, or across one sketch axis only.
        #[serde(default, skip_serializing_if = "Along::is_absolute")]
        along: Along,
    },
    /// A driving dimension: a circle's or arc's radius.
    Radius {
        a: Element,
        value: f64,
    },
    /// A driving dimension: the angle in degrees between two straight sides, measured between the rays leaving the
    /// corner they share (their nearest ends), as observed s15-006 (an obtuse corner reads 120.9638°).
    Angle {
        a: Element,
        b: Element,
        value: f64,
    },
    /// A driving dimension from a point: to another point (aligned, s15-012), or square to a straight side's line
    /// (s15-014).
    PointDistance {
        a: Element,
        b: Element,
        value: f64,
    },
}

impl Constraint {
    pub fn elements(&self) -> Vec<Element> {
        match *self {
            Constraint::Horizontal { a } | Constraint::Vertical { a } => vec![a],
            Constraint::Parallel { a, b }
            | Constraint::Perpendicular { a, b }
            | Constraint::Equal { a, b }
            | Constraint::Coincident { a, b }
            | Constraint::Midpoint { a, b }
            | Constraint::Concentric { a, b }
            | Constraint::Tangent { a, b, .. }
            | Constraint::Angle { a, b, .. }
            | Constraint::PointDistance { a, b, .. } => vec![a, b],
            Constraint::Symmetric { a, b, axis } => vec![a, b, axis],
            Constraint::Lock { entity } => vec![Element::Curve { entity }],
            Constraint::Distance { a, .. } | Constraint::Radius { a, .. } => vec![a],
        }
    }

    pub fn entities(&self) -> BTreeSet<u32> {
        self.elements().iter().map(|e| e.entity()).filter(|e| *e != NO_ENTITY).collect()
    }

    /// The same constraint on renumbered elements, or `None` when one of them is gone.
    pub fn remapped(&self, map: &dyn Fn(u32) -> Option<u32>) -> Option<Constraint> {
        let m = |e: Element| match e {
            Element::Origin { .. } => Some(e),
            _ => map(e.entity()).map(|n| e.with_entity(n)),
        };
        Some(match *self {
            Constraint::Horizontal { a } => Constraint::Horizontal { a: m(a)? },
            Constraint::Vertical { a } => Constraint::Vertical { a: m(a)? },
            Constraint::Parallel { a, b } => Constraint::Parallel { a: m(a)?, b: m(b)? },
            Constraint::Perpendicular { a, b } => Constraint::Perpendicular { a: m(a)?, b: m(b)? },
            Constraint::Equal { a, b } => Constraint::Equal { a: m(a)?, b: m(b)? },
            Constraint::Coincident { a, b } => Constraint::Coincident { a: m(a)?, b: m(b)? },
            Constraint::Midpoint { a, b } => Constraint::Midpoint { a: m(a)?, b: m(b)? },
            Constraint::Concentric { a, b } => Constraint::Concentric { a: m(a)?, b: m(b)? },
            Constraint::Tangent { a, b, inside } => Constraint::Tangent { a: m(a)?, b: m(b)?, inside },
            Constraint::Symmetric { a, b, axis } => Constraint::Symmetric { a: m(a)?, b: m(b)?, axis: m(axis)? },
            Constraint::Lock { entity } => Constraint::Lock { entity: map(entity)? },
            Constraint::Distance { a, value, along } => Constraint::Distance { a: m(a)?, value, along },
            Constraint::Radius { a, value } => Constraint::Radius { a: m(a)?, value },
            Constraint::Angle { a, b, value } => Constraint::Angle { a: m(a)?, b: m(b)?, value },
            Constraint::PointDistance { a, b, value } => Constraint::PointDistance { a: m(a)?, b: m(b)?, value },
        })
    }

    /// The name shown for it ("Parallel").
    pub fn label(&self) -> &'static str {
        match self {
            Constraint::Horizontal { .. } => "Horizontal",
            Constraint::Vertical { .. } => "Vertical",
            Constraint::Parallel { .. } => "Parallel",
            Constraint::Perpendicular { .. } => "Perpendicular",
            Constraint::Equal { .. } => "Equal",
            Constraint::Coincident { .. } => "Coincident",
            Constraint::Midpoint { .. } => "Midpoint",
            Constraint::Concentric { .. } => "Concentric",
            Constraint::Tangent { .. } => "Tangent",
            Constraint::Symmetric { .. } => "Symmetry",
            Constraint::Lock { .. } => "Lock",
            Constraint::Distance { .. } => "Distance",
            Constraint::Radius { .. } => "Radius",
            Constraint::Angle { .. } => "Angle",
            Constraint::PointDistance { .. } => "Distance",
        }
    }
}

/// The numbers that place an element (what the solver may change).
pub fn params(e: &SketchEntity) -> Vec<f64> {
    match e {
        SketchEntity::Line { a, b } => vec![a.x, a.y, b.x, b.y],
        SketchEntity::Rectangle { min, max } => vec![min.x, min.y, max.x, max.y],
        SketchEntity::Circle { center, radius } => vec![center.x, center.y, *radius],
        SketchEntity::Polygon { center, radius, angle, .. } => vec![center.x, center.y, *radius, *angle],
        SketchEntity::Arc { a, mid, b } => vec![a.x, a.y, mid.x, mid.y, b.x, b.y],
        SketchEntity::Ellipse { center, rx, ry, angle } => vec![center.x, center.y, *rx, *ry, *angle],
        SketchEntity::Spline { points } | SketchEntity::ControlSpline { poles: points } => points.iter().flat_map(|p| [p.x, p.y]).collect(),
        SketchEntity::Text { origin, .. } => vec![origin.x, origin.y],
    }
}

pub fn with_params(e: &SketchEntity, v: &[f64]) -> SketchEntity {
    let p = |i: usize| DVec2::new(v.get(i).copied().unwrap_or(0.0), v.get(i + 1).copied().unwrap_or(0.0));
    let s = |i: usize| v.get(i).copied().unwrap_or(0.0);
    match e {
        SketchEntity::Line { .. } => SketchEntity::Line { a: p(0), b: p(2) },
        SketchEntity::Rectangle { .. } => SketchEntity::Rectangle { min: p(0), max: p(2) },
        SketchEntity::Circle { .. } => SketchEntity::Circle { center: p(0), radius: s(2) },
        SketchEntity::Polygon { sides, .. } => SketchEntity::Polygon { center: p(0), radius: s(2), sides: *sides, angle: s(3) },
        SketchEntity::Arc { .. } => SketchEntity::Arc { a: p(0), mid: p(2), b: p(4) },
        SketchEntity::Ellipse { .. } => SketchEntity::Ellipse { center: p(0), rx: s(2), ry: s(3), angle: s(4) },
        SketchEntity::Spline { points } => SketchEntity::Spline { points: (0..points.len()).map(|i| p(i * 2)).collect() },
        SketchEntity::ControlSpline { poles } => SketchEntity::ControlSpline { poles: (0..poles.len()).map(|i| p(i * 2)).collect() },
        SketchEntity::Text { text, size, angle, font, .. } => {
            SketchEntity::Text { origin: p(0), text: text.clone(), size: *size, angle: *angle, font: font.clone() }
        }
    }
}

fn polygon_corner(center: DVec2, radius: f64, sides: u32, angle: f64, i: u32) -> DVec2 {
    let t = angle + std::f64::consts::TAU * i as f64 / sides.max(3) as f64;
    center + DVec2::new(t.cos(), t.sin()) * radius
}

/// The element's points: line ends; rectangle corners (counter-clockwise from `min`) then center; circle,
/// polygon and ellipse center (polygons then their corners); arc ends then center; spline points; text origin.
pub fn points(e: &SketchEntity) -> Vec<DVec2> {
    match e {
        SketchEntity::Line { a, b } => vec![*a, *b],
        SketchEntity::Rectangle { min, max } => vec![*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y), (*min + *max) / 2.0],
        SketchEntity::Circle { center, .. } | SketchEntity::Ellipse { center, .. } => vec![*center],
        SketchEntity::Polygon { center, radius, sides, angle } => {
            std::iter::once(*center).chain((0..*sides).map(|i| polygon_corner(*center, *radius, *sides, *angle, i))).collect()
        }
        SketchEntity::Arc { a, mid, b } => {
            let c = circle_through(*a, *mid, *b).map_or((*a + *b) / 2.0, |(c, _)| c);
            vec![*a, *b, c]
        }
        SketchEntity::Spline { points } | SketchEntity::ControlSpline { poles: points } => points.clone(),
        SketchEntity::Text { origin, .. } => vec![*origin],
    }
}

/// The element's straight sides: the line itself; rectangle sides (bottom, right, top, left); polygon sides.
pub fn segments(e: &SketchEntity) -> Vec<(DVec2, DVec2)> {
    match e {
        SketchEntity::Line { a, b } => vec![(*a, *b)],
        SketchEntity::Rectangle { .. } => {
            let c = points(e);
            (0..4).map(|i| (c[i], c[(i + 1) % 4])).collect()
        }
        SketchEntity::Polygon { center, radius, sides, angle } => (0..*sides)
            .map(|i| (polygon_corner(*center, *radius, *sides, *angle, i), polygon_corner(*center, *radius, *sides, *angle, i + 1)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Center and radius of a circle or arc (a polygon's circumcircle).
pub fn curve(e: &SketchEntity) -> Option<(DVec2, f64)> {
    match e {
        SketchEntity::Circle { center, radius } | SketchEntity::Polygon { center, radius, .. } => Some((*center, *radius)),
        SketchEntity::Arc { a, mid, b } => circle_through(*a, *mid, *b),
        SketchEntity::Ellipse { center, rx, ry, .. } => Some((*center, (rx + ry) / 2.0)),
        _ => None,
    }
}

fn point_of(es: &[SketchEntity], el: Element) -> Option<DVec2> {
    match el {
        Element::Point { entity, index } => points(es.get(entity as usize)?).get(index as usize).copied(),
        Element::Curve { entity } => curve(es.get(entity as usize)?).map(|c| c.0),
        Element::Segment { entity, index } => segments(es.get(entity as usize)?).get(index as usize).map(|s| (s.0 + s.1) / 2.0),
        Element::Origin { u, v } => Some(DVec2::new(u as f64 * 1e-6, v as f64 * 1e-6)),
    }
}

fn segment_of(es: &[SketchEntity], el: Element) -> Option<(DVec2, DVec2)> {
    match el {
        Element::Segment { entity, index } => segments(es.get(entity as usize)?).get(index as usize).copied(),
        _ => None,
    }
}

fn curve_of(es: &[SketchEntity], el: Element) -> Option<(DVec2, f64)> {
    match el {
        Element::Curve { entity } => curve(es.get(entity as usize)?),
        _ => None,
    }
}

fn cross(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Signed distance from `p` to the line through `s`.
fn line_distance(p: DVec2, s: (DVec2, DVec2)) -> f64 {
    let d = s.1 - s.0;
    let len = d.length().max(1e-12);
    cross(d, p - s.0) / len
}

/// How far from holding a constraint is, as lengths (zero when it holds). `None` if its elements are missing.
pub fn residuals(es: &[SketchEntity], c: &Constraint) -> Option<Vec<f64>> {
    Some(match *c {
        Constraint::Horizontal { a } => {
            let s = segment_of(es, a)?;
            vec![s.1.y - s.0.y]
        }
        Constraint::Vertical { a } => {
            let s = segment_of(es, a)?;
            vec![s.1.x - s.0.x]
        }
        Constraint::Parallel { a, b } | Constraint::Perpendicular { a, b } => {
            let (s, t) = (segment_of(es, a)?, segment_of(es, b)?);
            let (d1, d2) = (s.1 - s.0, t.1 - t.0);
            let (l1, l2) = (d1.length().max(1e-12), d2.length().max(1e-12));
            let scale = (l1 + l2) / 2.0;
            let v = if matches!(c, Constraint::Parallel { .. }) { cross(d1, d2) } else { d1.dot(d2) };
            vec![v / (l1 * l2) * scale]
        }
        Constraint::Equal { a, b } => match (segment_of(es, a), segment_of(es, b)) {
            (Some(s), Some(t)) => vec![s.0.distance(s.1) - t.0.distance(t.1)],
            _ => vec![curve_of(es, a)?.1 - curve_of(es, b)?.1],
        },
        Constraint::Coincident { a, b } => {
            let p = point_of(es, a)?;
            match b {
                Element::Point { .. } | Element::Origin { .. } => {
                    let q = point_of(es, b)?;
                    vec![p.x - q.x, p.y - q.y]
                }
                Element::Segment { .. } => vec![line_distance(p, segment_of(es, b)?)],
                Element::Curve { .. } => {
                    let (c, r) = curve_of(es, b)?;
                    vec![p.distance(c) - r]
                }
            }
        }
        Constraint::Midpoint { a, b } => {
            let p = point_of(es, a)?;
            let s = segment_of(es, b)?;
            let m = (s.0 + s.1) / 2.0;
            vec![p.x - m.x, p.y - m.y]
        }
        Constraint::Concentric { a, b } => {
            let (p, q) = (curve_of(es, a)?.0, curve_of(es, b)?.0);
            vec![p.x - q.x, p.y - q.y]
        }
        Constraint::Tangent { a, b, inside } => match (segment_of(es, a), curve_of(es, b)) {
            (Some(s), Some((c, r))) => vec![line_distance(c, s).abs() - r],
            _ => {
                let (c1, r1) = curve_of(es, a)?;
                let (c2, r2) = curve_of(es, b)?;
                let d = c1.distance(c2);
                vec![if inside { d - (r1 - r2).abs() } else { d - (r1 + r2) }]
            }
        },
        Constraint::Symmetric { a, b, axis } => {
            let (p, q, s) = (point_of(es, a)?, point_of(es, b)?, segment_of(es, axis)?);
            let dir = (s.1 - s.0).normalize_or_zero();
            vec![line_distance((p + q) / 2.0, s), (q - p).dot(dir)]
        }
        Constraint::Lock { entity } => {
            es.get(entity as usize)?;
            Vec::new()
        }
        Constraint::Distance { a, value, along } => {
            let s = segment_of(es, a)?;
            vec![along.measure(s.0, s.1) - value]
        }
        Constraint::Radius { a, value } => vec![curve_of(es, a)?.1 - value],
        Constraint::PointDistance { a, b, value } => {
            let p = point_of(es, a)?;
            match b {
                Element::Segment { .. } => vec![line_distance(p, segment_of(es, b)?).abs() - value],
                _ => vec![p.distance(point_of(es, b)?) - value],
            }
        }
        Constraint::Angle { a, b, value } => {
            let (s, t) = (segment_of(es, a)?, segment_of(es, b)?);
            let (_, d1, d2) = corner_rays(s, t);
            let scale = (s.0.distance(s.1) + t.0.distance(t.1)) / 2.0;
            vec![(ray_angle(d1, d2) - value.to_radians()) * scale]
        }
    })
}

/// Where two straight sides meet and the rays leaving that corner along each: the pair of ends nearest each other
/// is the corner (the first side's end of it is returned), each ray runs from there to the side's far end.
pub fn corner_rays(s: (DVec2, DVec2), t: (DVec2, DVec2)) -> (DVec2, DVec2, DVec2) {
    let pairs = [(s.0, s.1, t.0, t.1), (s.0, s.1, t.1, t.0), (s.1, s.0, t.0, t.1), (s.1, s.0, t.1, t.0)];
    let (c, sf, tc, tf) = pairs.into_iter().min_by(|p, q| p.0.distance(p.2).total_cmp(&q.0.distance(q.2))).unwrap_or(pairs[0]);
    (c, sf - c, tf - tc)
}

/// The unsigned angle between two rays, 0..π.
pub fn ray_angle(d1: DVec2, d2: DVec2) -> f64 {
    cross(d1, d2).abs().atan2(d1.dot(d2))
}

/// Which elements the constraints pin down completely (drawn green, as observed s14-145): an element is fully
/// defined when no motion the constraints allow moves any of its parameters. Found from the null space of the
/// constraints' Jacobian (locked elements contribute their own parameters as fixed). Elements without parameters,
/// and sketches too big to analyse, count as not defined.
pub fn fully_defined(es: &[SketchEntity], cs: &[Constraint]) -> Vec<bool> {
    const MAX_PARAMS: usize = 600;
    let offsets: Vec<usize> = es
        .iter()
        .scan(0usize, |acc, e| {
            let at = *acc;
            *acc += params(e).len();
            Some(at)
        })
        .collect();
    let n = es.iter().map(|e| params(e).len()).sum::<usize>();
    if n == 0 || n > MAX_PARAMS || cs.is_empty() {
        return vec![false; es.len()];
    }
    let size = es.iter().flat_map(points).fold(1.0f64, |m, p| m.max(p.length())).max(1.0);
    let h = size * 1e-6;
    // Jacobian rows by central differences, one constraint at a time (only its own elements' columns).
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for c in cs {
        if let Constraint::Lock { entity } = c {
            let Some(e) = es.get(*entity as usize) else { continue };
            for k in 0..params(e).len() {
                let mut row = vec![0.0; n];
                row[offsets[*entity as usize] + k] = 1.0;
                rows.push(row);
            }
            continue;
        }
        let Some(base) = residuals(es, c) else { continue };
        let mut block = vec![vec![0.0; n]; base.len()];
        for ent in c.entities() {
            let Some(e) = es.get(ent as usize) else { continue };
            let p = params(e);
            for k in 0..p.len() {
                let mut moved = es.to_vec();
                let mut plus = p.clone();
                plus[k] += h;
                moved[ent as usize] = with_params(e, &plus);
                let Some(rp) = residuals(&moved, c) else { continue };
                let mut minus = p.clone();
                minus[k] -= h;
                moved[ent as usize] = with_params(e, &minus);
                let Some(rm) = residuals(&moved, c) else { continue };
                for (r, row) in block.iter_mut().enumerate() {
                    if let (Some(a), Some(b)) = (rp.get(r), rm.get(r)) {
                        row[offsets[ent as usize] + k] = (a - b) / (2.0 * h);
                    }
                }
            }
        }
        rows.extend(block);
    }
    // An arc's middle point can slide along it without changing the arc: hold it halfway between the ends, so an
    // arc whose ends and radius are pinned counts as defined.
    for (i, e) in es.iter().enumerate() {
        if !matches!(e, SketchEntity::Arc { .. }) {
            continue;
        }
        let gauge = |e: &SketchEntity| match e {
            SketchEntity::Arc { a, mid, b } => mid.distance(*a) - mid.distance(*b),
            _ => 0.0,
        };
        let p = params(e);
        let mut row = vec![0.0; n];
        for k in 0..p.len() {
            let (mut plus, mut minus) = (p.clone(), p.clone());
            plus[k] += h;
            minus[k] -= h;
            row[offsets[i] + k] = (gauge(&with_params(e, &plus)) - gauge(&with_params(e, &minus))) / (2.0 * h);
        }
        rows.push(row);
    }
    // Reduced row echelon form; a column is pinned down when it's a pivot whose row has no free columns in it.
    let tol = 1e-7;
    let mut pivots: Vec<(usize, usize)> = Vec::new();
    let mut r = 0;
    for col in 0..n {
        let Some(best) = (r..rows.len()).max_by(|&a, &b| rows[a][col].abs().total_cmp(&rows[b][col].abs())) else { break };
        if rows[best][col].abs() < tol {
            continue;
        }
        rows.swap(r, best);
        let pv = rows[r][col];
        for v in rows[r].iter_mut() {
            *v /= pv;
        }
        for i in 0..rows.len() {
            if i != r && rows[i][col].abs() > 0.0 {
                let f = rows[i][col];
                let (src, dst) = if i < r {
                    let (a, b) = rows.split_at_mut(r);
                    (&b[0], &mut a[i])
                } else {
                    let (a, b) = rows.split_at_mut(i);
                    (&a[r], &mut b[0])
                };
                for (d, s) in dst.iter_mut().zip(src) {
                    *d -= f * s;
                }
            }
        }
        pivots.push((r, col));
        r += 1;
        if r == rows.len() {
            break;
        }
    }
    let pivot_cols: BTreeSet<usize> = pivots.iter().map(|p| p.1).collect();
    let mut defined = vec![false; n];
    for &(row, col) in &pivots {
        defined[col] = (0..n).filter(|c| !pivot_cols.contains(c)).all(|free| rows[row][free].abs() < tol * 10.0);
    }
    es.iter()
        .enumerate()
        .map(|(i, e)| {
            let k = params(e).len();
            k > 0 && (offsets[i]..offsets[i] + k).all(|j| defined[j])
        })
        .collect()
}

/// Total misfit of all constraints (root of the sum of squares).
pub fn error(es: &[SketchEntity], cs: &[Constraint]) -> f64 {
    cs.iter().filter_map(|c| residuals(es, c)).flatten().map(|r| r * r).sum::<f64>().sqrt()
}

/// Moves the elements until every constraint holds. Locked elements and `pinned` ones stay put; the rest move as
/// little as they can. Returns the new elements, or `None` when the constraints can't all be met.
pub fn solve(es: &[SketchEntity], cs: &[Constraint], pinned: &BTreeSet<u32>) -> Option<Vec<SketchEntity>> {
    let size = es.iter().flat_map(points).fold(1.0f64, |m, p| m.max(p.length())).max(1.0);
    let tol = size * 1e-9;
    if error(es, cs) <= tol {
        return Some(es.to_vec());
    }
    let locked: BTreeSet<u32> = cs.iter().filter_map(|c| if let Constraint::Lock { entity } = c { Some(*entity) } else { None }).collect();
    // The variables: parameters of constrained elements that may move.
    let mut movable: BTreeSet<u32> = cs.iter().flat_map(|c| c.entities()).collect();
    movable.retain(|e| !locked.contains(e) && !pinned.contains(e) && (*e as usize) < es.len());
    let slots: Vec<(u32, usize)> = movable.iter().flat_map(|&e| (0..params(&es[e as usize]).len()).map(move |k| (e, k))).collect();
    if slots.is_empty() {
        return None;
    }
    let mut cur = es.to_vec();
    let read = |cur: &[SketchEntity]| -> Vec<f64> { slots.iter().map(|&(e, k)| params(&cur[e as usize])[k]).collect() };
    let write = |cur: &mut Vec<SketchEntity>, x: &[f64]| {
        for &e in &movable {
            let mut p = params(&cur[e as usize]);
            for (i, &(se, k)) in slots.iter().enumerate() {
                if se == e {
                    p[k] = x[i];
                }
            }
            cur[e as usize] = with_params(&cur[e as usize], &p);
        }
    };
    let resid = |cur: &[SketchEntity]| -> Vec<f64> { cs.iter().filter_map(|c| residuals(cur, c)).flatten().collect() };
    let mut x = read(&cur);
    let n = x.len();
    let mut mu = 1e-3;
    for _ in 0..200 {
        let r = resid(&cur);
        let err: f64 = r.iter().map(|v| v * v).sum::<f64>().sqrt();
        if err <= tol {
            return Some(cur);
        }
        // Numeric Jacobian.
        let h = size * 1e-7;
        let m = r.len();
        let mut jac = vec![0.0; m * n];
        for j in 0..n {
            let mut xp = x.clone();
            xp[j] += h;
            let mut probe = cur.clone();
            write(&mut probe, &xp);
            let rp = resid(&probe);
            for i in 0..m.min(rp.len()) {
                jac[i * n + j] = (rp[i] - r[i]) / h;
            }
        }
        // Damped least squares, smallest step first: (JᵀJ + μI) dx = −Jᵀr.
        let mut improved = false;
        for _ in 0..12 {
            let mut a = vec![0.0; n * n];
            let mut g = vec![0.0; n];
            for i in 0..m {
                for j in 0..n {
                    let jij = jac[i * n + j];
                    if jij == 0.0 {
                        continue;
                    }
                    g[j] -= jij * r[i];
                    for k in 0..n {
                        a[j * n + k] += jij * jac[i * n + k];
                    }
                }
            }
            for j in 0..n {
                a[j * n + j] += mu;
            }
            let Some(dx) = linear_solve(a, g, n) else { break };
            let xn: Vec<f64> = x.iter().zip(&dx).map(|(a, b)| a + b).collect();
            let mut trial = cur.clone();
            write(&mut trial, &xn);
            let e2: f64 = resid(&trial).iter().map(|v| v * v).sum::<f64>().sqrt();
            if e2 < err && trial.iter().all(valid) {
                x = xn;
                cur = trial;
                mu = (mu * 0.3).max(1e-12);
                improved = true;
                break;
            }
            mu *= 10.0;
        }
        if !improved {
            break;
        }
    }
    (error(&cur, cs) <= size * 1e-6).then_some(cur)
}

fn valid(e: &SketchEntity) -> bool {
    match e {
        SketchEntity::Circle { radius, .. } | SketchEntity::Polygon { radius, .. } => *radius > 0.0,
        SketchEntity::Rectangle { min, max } => max.x > min.x && max.y > min.y,
        SketchEntity::Ellipse { rx, ry, .. } => *rx > 0.0 && *ry > 0.0,
        SketchEntity::Line { a, b } => a.distance(*b) > 1e-9,
        _ => true,
    }
}

/// Gaussian elimination with partial pivoting.
fn linear_solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let piv = (col..n).max_by(|&i, &j| a[i * n + col].abs().total_cmp(&a[j * n + col].abs()))?;
        if a[piv * n + col].abs() < 1e-300 {
            return None;
        }
        if piv != col {
            for k in 0..n {
                a.swap(piv * n + k, col * n + k);
            }
            b.swap(piv, col);
        }
        for i in col + 1..n {
            let f = a[i * n + col] / a[col * n + col];
            if f == 0.0 {
                continue;
            }
            for k in col..n {
                a[i * n + k] -= f * a[col * n + k];
            }
            b[i] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[i * n + k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i * n + i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Picks the elements a constraint works on from two (or three) selected sketch elements, the way the reference
/// app does it from whole curves: the nearest endpoints, the side that's closest in direction, circle centers.
pub fn constraint_for(kind: &str, es: &[SketchEntity], picked: &[u32]) -> Result<Constraint, &'static str> {
    let get = |i: usize| picked.get(i).and_then(|&e| es.get(e as usize).map(|x| (e, x)));
    let first_segment = |e: u32, x: &SketchEntity| (!segments(x).is_empty()).then_some(Element::Segment { entity: e, index: 0 });
    // The side of `x` closest in direction to `dir`.
    let side_like = |e: u32, x: &SketchEntity, dir: DVec2| {
        let segs = segments(x);
        let best = (0..segs.len()).max_by(|&i, &j| {
            let f = |k: usize| (segs[k].1 - segs[k].0).normalize_or_zero().dot(dir).abs();
            f(i).total_cmp(&f(j))
        })?;
        Some(Element::Segment { entity: e, index: best as u32 })
    };
    let curve_el = |e: u32, x: &SketchEntity| curve(x).map(|_| Element::Curve { entity: e });
    // The pair of points (one per element) closest to each other.
    let nearest_points = |(ea, xa): (u32, &SketchEntity), (eb, xb): (u32, &SketchEntity)| {
        let (pa, pb) = (points(xa), points(xb));
        let mut best = None;
        let mut d = f64::INFINITY;
        for (i, p) in pa.iter().enumerate() {
            for (j, q) in pb.iter().enumerate() {
                if p.distance(*q) < d {
                    d = p.distance(*q);
                    best = Some((Element::Point { entity: ea, index: i as u32 }, Element::Point { entity: eb, index: j as u32 }));
                }
            }
        }
        best
    };
    let need2 = "select two sketch elements first";
    match kind {
        "horizontal_vertical" => {
            let (e, x) = get(0).ok_or("select a line first")?;
            let s = segments(x).first().copied().ok_or("pick a straight line")?;
            let d = s.1 - s.0;
            let a = first_segment(e, x).ok_or("pick a straight line")?;
            Ok(if d.x.abs() >= d.y.abs() { Constraint::Horizontal { a } } else { Constraint::Vertical { a } })
        }
        "parallel" | "perpendicular" => {
            let ((ea, xa), (eb, xb)) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            let a = first_segment(ea, xa).ok_or("pick straight lines")?;
            let da = segments(xa).first().map(|s| (s.1 - s.0).normalize_or_zero()).unwrap_or(DVec2::X);
            let want = if kind == "parallel" { da } else { da.perp() };
            let b = side_like(eb, xb, want).ok_or("pick straight lines")?;
            Ok(if kind == "parallel" { Constraint::Parallel { a, b } } else { Constraint::Perpendicular { a, b } })
        }
        "equal" => {
            let ((ea, xa), (eb, xb)) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            match (curve_el(ea, xa), curve_el(eb, xb)) {
                (Some(a), Some(b)) => Ok(Constraint::Equal { a, b }),
                _ => Ok(Constraint::Equal {
                    a: first_segment(ea, xa).ok_or("pick two lines or two circles")?,
                    b: first_segment(eb, xb).ok_or("pick two lines or two circles")?,
                }),
            }
        }
        "coincident" => {
            let (a, b) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            // A point-like element (a circle's center) onto a curve or line, else the nearest points.
            if matches!(a.1, SketchEntity::Line { .. } | SketchEntity::Arc { .. })
                || matches!(b.1, SketchEntity::Line { .. } | SketchEntity::Arc { .. })
            {
                let (pa, pb) = nearest_points(a, b).ok_or(need2)?;
                return Ok(Constraint::Coincident { a: pa, b: pb });
            }
            let center = Element::Point { entity: a.0, index: 0 };
            match (curve_el(b.0, b.1), first_segment(b.0, b.1)) {
                (Some(c), _) => Ok(Constraint::Coincident { a: center, b: c }),
                (None, Some(s)) => Ok(Constraint::Coincident { a: center, b: s }),
                _ => Err("those can't be made coincident"),
            }
        }
        "midpoint" => {
            let ((ea, xa), (eb, xb)) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            // The first element's point nearest the second's middle.
            let s = segments(xb).first().copied().ok_or("the second element must be a line")?;
            let m = (s.0 + s.1) / 2.0;
            let pts = points(xa);
            let i = (0..pts.len()).min_by(|&i, &j| pts[i].distance(m).total_cmp(&pts[j].distance(m))).ok_or(need2)?;
            Ok(Constraint::Midpoint { a: Element::Point { entity: ea, index: i as u32 }, b: Element::Segment { entity: eb, index: 0 } })
        }
        "concentric" => {
            let ((ea, xa), (eb, xb)) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            Ok(Constraint::Concentric {
                a: curve_el(ea, xa).ok_or("pick circles or arcs")?,
                b: curve_el(eb, xb).ok_or("pick circles or arcs")?,
            })
        }
        "tangent" => {
            let ((ea, xa), (eb, xb)) = (get(0).ok_or(need2)?, get(1).ok_or(need2)?);
            match (curve_el(ea, xa), curve_el(eb, xb)) {
                (Some(a), Some(b)) => {
                    let ((c1, r1), (c2, r2)) = (curve(xa).ok_or(need2)?, curve(xb).ok_or(need2)?);
                    let d = c1.distance(c2);
                    let inside = (d - (r1 - r2).abs()).abs() < (d - (r1 + r2)).abs();
                    Ok(Constraint::Tangent { a, b, inside })
                }
                (None, Some(b)) => {
                    Ok(Constraint::Tangent { a: first_segment(ea, xa).ok_or("pick a line and a circle")?, b, inside: false })
                }
                (Some(a), None) => {
                    Ok(Constraint::Tangent { a: first_segment(eb, xb).ok_or("pick a line and a circle")?, b: a, inside: false })
                }
                _ => Err("pick a line and a circle, or two circles"),
            }
        }
        "symmetry" => {
            let (a, b, axis) = (
                get(0).ok_or("select two elements and a line")?,
                get(1).ok_or("select two elements and a line")?,
                get(2).ok_or("select two elements and a line")?,
            );
            let axis_el = first_segment(axis.0, axis.1).ok_or("the last element must be a line")?;
            let (pa, pb) = match (curve_el(a.0, a.1), curve_el(b.0, b.1)) {
                (Some(_), Some(_)) => (Element::Point { entity: a.0, index: 0 }, Element::Point { entity: b.0, index: 0 }),
                _ => nearest_points(a, b).ok_or("select two elements and a line")?,
            };
            Ok(Constraint::Symmetric { a: pa, b: pb, axis: axis_el })
        }
        "lock" => {
            let (e, _) = get(0).ok_or("select an element first")?;
            Ok(Constraint::Lock { entity: e })
        }
        _ => Err("unknown constraint"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> SketchEntity {
        SketchEntity::Line { a: DVec2::new(a.0, a.1), b: DVec2::new(b.0, b.1) }
    }

    #[test]
    fn an_arc_with_pinned_ends_and_radius_is_defined() {
        // Radius 10 around (0, -5√3): a flatter arc than a half circle (whose radius can't shrink, so it reads as stuck).
        let c = DVec2::new(0.0, -75f64.sqrt());
        let arc = SketchEntity::Arc { a: DVec2::new(-5.0, 0.0), mid: c + DVec2::new(2.0, 96f64.sqrt()), b: DVec2::new(5.0, 0.0) };
        let es = vec![line((-5.0, 0.0), (5.0, 0.0)), arc];
        let pt = |entity, index| Element::Point { entity, index };
        let mut cs = vec![
            Constraint::Lock { entity: 0 },
            Constraint::Coincident { a: pt(1, 0), b: pt(0, 0) },
            Constraint::Coincident { a: pt(1, 1), b: pt(0, 1) },
        ];
        assert_eq!(fully_defined(&es, &cs), vec![true, false], "the radius is still free");
        cs.push(Constraint::Radius { a: Element::Curve { entity: 1 }, value: 10.0 });
        assert_eq!(fully_defined(&es, &cs), vec![true, true]);
    }

    #[test]
    fn a_driving_distance_sets_a_lines_length() {
        let es = vec![line((0.0, 0.0), (3.0, 4.0))];
        let cs = vec![Constraint::Distance { a: Element::Segment { entity: 0, index: 0 }, value: 10.0, along: Along::Absolute }];
        let out = solve(&es, &cs, &Default::default()).unwrap();
        let SketchEntity::Line { a, b } = out[0] else { panic!("not a line") };
        assert!((a.distance(b) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_horizontal_distance_sets_only_the_lines_width() {
        let es = vec![line((0.0, 0.0), (3.0, 4.0))];
        let cs = vec![Constraint::Distance { a: Element::Segment { entity: 0, index: 0 }, value: 6.0, along: Along::Horizontal }];
        let out = solve(&es, &cs, &Default::default()).unwrap();
        let SketchEntity::Line { a, b } = out[0] else { panic!("not a line") };
        assert!(((b.x - a.x).abs() - 6.0).abs() < 1e-6, "{a} {b}");
        // Saved without the field when it's Absolute, so older files read the same.
        let json = serde_json::to_string(&cs[0]).unwrap();
        assert!(json.contains("horizontal"));
        let plain: Constraint =
            serde_json::from_str(r#"{"kind":"distance","a":{"kind":"segment","entity":0,"index":0},"value":2.0}"#).unwrap();
        assert!(matches!(plain, Constraint::Distance { along: Along::Absolute, .. }));
    }

    #[test]
    fn parallel_moves_the_second_line_only() {
        let es = vec![line((0.0, 0.0), (10.0, 0.0)), line((0.0, 5.0), (10.0, 7.0))];
        let c = constraint_for("parallel", &es, &[0, 1]).unwrap();
        let out = solve(&es, std::slice::from_ref(&c), &BTreeSet::from([0])).unwrap();
        assert_eq!(out[0], es[0]);
        assert!(error(&out, &[c]) < 1e-6);
    }

    #[test]
    fn horizontal_coincident_and_tangent() {
        let es = vec![
            line((0.0, 0.0), (10.0, 1.0)),
            line((10.5, 1.2), (12.0, 8.0)),
            SketchEntity::Circle { center: DVec2::new(5.0, 6.0), radius: 3.0 },
        ];
        let cs = vec![
            constraint_for("horizontal_vertical", &es, &[0]).unwrap(),
            constraint_for("coincident", &es, &[0, 1]).unwrap(),
            constraint_for("tangent", &es, &[0, 2]).unwrap(),
        ];
        assert!(matches!(cs[0], Constraint::Horizontal { .. }));
        let out = solve(&es, &cs, &BTreeSet::new()).unwrap();
        assert!(error(&out, &cs) < 1e-6);
    }

    #[test]
    fn impossible_constraints_fail() {
        let es = vec![line((0.0, 0.0), (10.0, 0.0)), line((0.0, 5.0), (10.0, 5.0))];
        let cs = vec![
            Constraint::Lock { entity: 0 },
            Constraint::Lock { entity: 1 },
            Constraint::Perpendicular { a: Element::Segment { entity: 0, index: 0 }, b: Element::Segment { entity: 1, index: 0 } },
        ];
        assert!(solve(&es, &cs, &BTreeSet::new()).is_none());
    }

    #[test]
    fn remapping_drops_constraints_on_removed_elements() {
        let c = Constraint::Parallel { a: Element::Segment { entity: 0, index: 0 }, b: Element::Segment { entity: 2, index: 0 } };
        let shifted = c
            .remapped(&|e| {
                if e == 1 {
                    None
                } else if e > 1 {
                    Some(e - 1)
                } else {
                    Some(e)
                }
            })
            .unwrap();
        assert_eq!(shifted.entities(), BTreeSet::from([0, 1]));
        assert!(c.remapped(&|e| if e == 2 { None } else { Some(e) }).is_none());
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> SketchEntity {
        SketchEntity::Line { a: DVec2::new(a.0, a.1), b: DVec2::new(b.0, b.1) }
    }

    #[test]
    fn locking_one_line_and_dimensioning_the_other_defines_both() {
        let es = vec![line((0.0, 0.0), (10.0, 0.0)), line((10.0, 0.0), (10.0, 5.0))];
        let p = |e: u32, i: u32| Element::Point { entity: e, index: i };
        let seg = |e: u32| Element::Segment { entity: e, index: 0 };
        let mut cs = vec![Constraint::Coincident { a: p(1, 0), b: p(0, 1) }, Constraint::Perpendicular { a: seg(0), b: seg(1) }];
        assert_eq!(fully_defined(&es, &cs), vec![false, false]);
        cs.push(Constraint::Lock { entity: 0 });
        assert_eq!(fully_defined(&es, &cs), vec![true, false], "the second line can still grow");
        cs.push(Constraint::Distance { a: seg(1), value: 5.0, along: Along::Absolute });
        assert_eq!(fully_defined(&es, &cs), vec![true, true]);
    }
}

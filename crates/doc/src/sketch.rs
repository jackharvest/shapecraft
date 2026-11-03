use std::sync::Arc;

use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use shapecraft_kernel::{Mesh, PlanarCurve, PlaneFrame, Shape};

use crate::ItemId;

/// A sketch plane: origin plus an orthonormal frame. Sketch coordinates are (u, v) along `x` and `y`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchPlane {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
}

impl SketchPlane {
    pub const XY: SketchPlane = SketchPlane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y };
    pub const XZ: SketchPlane = SketchPlane { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Z };
    pub const YZ: SketchPlane = SketchPlane { origin: DVec3::ZERO, x: DVec3::Y, y: DVec3::Z };

    /// A plane through `origin` with the given normal, with a stable in-plane x axis.
    pub fn from_normal(origin: DVec3, normal: DVec3) -> Option<Self> {
        let n = normal.try_normalize()?;
        let helper = if n.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let x = helper.cross(n).try_normalize()?;
        let y = n.cross(x);
        Some(Self { origin, x, y })
    }

    /// The plane of a sketch on a flat face, lined up with the standard view that looks at it: a face facing up or
    /// down reads like the Top (or Bottom) view, x along +X; any other face stands upright, y as near +Z as it gets.
    /// `legacy` keeps the frame projects saved before file version 2 used (x = −Y on a top face), so their History
    /// replays the curves where they were drawn.
    pub fn on_face(origin: DVec3, normal: DVec3, legacy: bool) -> Option<Self> {
        let n = normal.try_normalize()?;
        if legacy || n.z.abs() < 0.9 {
            return Self::from_normal(origin, n);
        }
        let x = (DVec3::X - n * n.x).try_normalize()?;
        Some(Self { origin, x, y: n.cross(x) })
    }

    pub fn normal(&self) -> DVec3 {
        self.x.cross(self.y)
    }

    pub fn to_world(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    pub fn to_local(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x), d.dot(self.y))
    }

    /// Where a ray meets the plane, if it isn't parallel to it.
    pub fn intersect_ray(&self, origin: DVec3, dir: DVec3) -> Option<DVec3> {
        let n = self.normal();
        let denom = dir.dot(n);
        if denom.abs() < 1e-12 {
            return None;
        }
        let t = (self.origin - origin).dot(n) / denom;
        (t > 0.0).then(|| origin + dir * t)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SketchEntity {
    Line {
        a: DVec2,
        b: DVec2,
    },
    Rectangle {
        min: DVec2,
        max: DVec2,
    },
    Circle {
        center: DVec2,
        radius: f64,
    },
    /// A regular polygon: `sides` corners on a circle, the first at `angle` (radians).
    Polygon {
        center: DVec2,
        radius: f64,
        sides: u32,
        angle: f64,
    },
    /// A circular arc from `a` through `mid` to `b`.
    Arc {
        a: DVec2,
        mid: DVec2,
        b: DVec2,
    },
    /// Radii along its own axes, turned by `angle` (radians).
    Ellipse {
        center: DVec2,
        rx: f64,
        ry: f64,
        angle: f64,
    },
    /// A smooth curve through the points.
    Spline {
        points: Vec<DVec2>,
    },
    /// A smooth curve pulled toward its control points (a clamped B-spline); it starts and ends on the first and last.
    ControlSpline {
        poles: Vec<DVec2>,
    },
    /// Letters starting at `origin` on the baseline, `size` mm per em, turned by `angle` (radians).
    Text {
        origin: DVec2,
        text: String,
        size: f64,
        angle: f64,
        /// The font family; none = the plain sans-serif.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
    },
}

/// Points along the smooth curve through the points (the kernel and Shapr3D make the same curve), `per_segment`
/// between each pair.
pub fn spline_points(pts: &[DVec2], per_segment: usize) -> Vec<DVec2> {
    let flat: Vec<DVec3> = pts.iter().map(|p| p.extend(0.0)).collect();
    match shapecraft_kernel::spline::fit(&flat) {
        Some(f) => f.sample(per_segment).into_iter().map(|p| p.truncate()).collect(),
        None => pts.to_vec(),
    }
}

/// Points along a clamped uniform B-spline of degree min(3, n-1) over the control points (the kernel builds
/// the same curve).
pub fn bspline_points(poles: &[DVec2], per_span: usize) -> Vec<DVec2> {
    let n = poles.len();
    if n < 3 {
        return poles.to_vec();
    }
    let deg = (n - 1).min(3);
    let spans = n - deg;
    // Knots 0…spans with the ends repeated deg+1 times.
    let knot = |i: usize| (i as f64 - deg as f64).clamp(0.0, spans as f64);
    let at = |t: f64| {
        let s = (t.floor() as usize).min(spans - 1);
        // de Boor on the poles s..=s+deg.
        let mut d: Vec<DVec2> = (0..=deg).map(|j| poles[s + j]).collect();
        for r in 1..=deg {
            for j in (r..=deg).rev() {
                let i = s + j;
                let (k0, k1) = (knot(i), knot(i + deg + 1 - r));
                let a = if k1 - k0 > 1e-12 { (t - k0) / (k1 - k0) } else { 0.0 };
                d[j] = d[j - 1] * (1.0 - a) + d[j] * a;
            }
        }
        d[deg]
    };
    let steps = spans * per_span.max(1);
    (0..=steps).map(|k| at(k as f64 * spans as f64 / steps as f64)).collect()
}

/// The circle through three points: center and radius, or `None` if they are (nearly) collinear.
pub fn circle_through(a: DVec2, b: DVec2, c: DVec2) -> Option<(DVec2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a.length_squared(), b.length_squared(), c.length_squared());
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = DVec2::new(ux, uy);
    Some((center, center.distance(a)))
}

/// Points along the arc a → mid → b.
pub fn arc_points(a: DVec2, mid: DVec2, b: DVec2, segments: usize) -> Vec<DVec2> {
    let Some((c, r)) = circle_through(a, mid, b) else { return vec![a, b] };
    let ang = |p: DVec2| (p - c).to_angle();
    let (t0, tm, t1) = (ang(a), ang(mid), ang(b));
    let tau = std::f64::consts::TAU;
    let norm = |t: f64| t.rem_euclid(tau);
    // Go counter-clockwise if mid lies on the CCW sweep from a to b, else clockwise.
    let ccw_span = norm(t1 - t0);
    let ccw = norm(tm - t0) < ccw_span;
    let span = if ccw { ccw_span } else { -(tau - ccw_span) };
    (0..=segments).map(|i| c + DVec2::from_angle(t0 + span * i as f64 / segments as f64) * r).collect()
}

/// A rectangle made of four lines: its corner with the lowest coordinates along `u` and `v`, the directions of its
/// sides (`u` the one nearest the sketch's x axis) and its size along them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectFrame {
    pub origin: DVec2,
    pub u: DVec2,
    pub v: DVec2,
    pub width: f64,
    pub height: f64,
}

impl RectFrame {
    /// Corners in order: origin, along u, opposite, along v.
    pub fn corners(&self) -> [DVec2; 4] {
        let (o, u, v) = (self.origin, self.u * self.width, self.v * self.height);
        [o, o + u, o + u + v, o + v]
    }
}

/// The four lines of the rectangle line `i` is a side of (as the reference draws rectangles: four lines meeting at
/// right angles), in order around it.
pub fn rectangle_loop(entities: &[SketchEntity], i: u32) -> Option<[u32; 4]> {
    const TOL: f64 = 1e-7;
    let line = |k: usize| match entities.get(k) {
        Some(SketchEntity::Line { a, b }) => Some((*a, *b)),
        _ => None,
    };
    let (a0, b0) = line(i as usize)?;
    let mut out = [i, 0, 0, 0];
    let (mut end, mut dir) = (b0, b0 - a0);
    for step in 1..4 {
        let next = (0..entities.len()).filter(|k| !out[..step].contains(&(*k as u32))).find_map(|k| {
            let (a, b) = line(k)?;
            let far = if a.distance(end) < TOL * (1.0 + end.length()) {
                b
            } else if b.distance(end) < TOL * (1.0 + end.length()) {
                a
            } else {
                return None;
            };
            let d = far - end;
            (d.length() > TOL && dir.normalize_or_zero().dot(d.normalize_or_zero()).abs() < 1e-6).then_some((k, far, d))
        })?;
        out[step] = next.0 as u32;
        (end, dir) = (next.1, next.2);
    }
    (end.distance(a0) < TOL * (1.0 + a0.length())).then_some(out)
}

/// The frame of the rectangle `lines` make (see [`rectangle_loop`]).
pub fn rectangle_frame(entities: &[SketchEntity], lines: [u32; 4]) -> Option<RectFrame> {
    let mut pts = Vec::with_capacity(8);
    for k in lines {
        match entities.get(k as usize) {
            Some(SketchEntity::Line { a, b }) => pts.extend([*a, *b]),
            _ => return None,
        }
    }
    let d = (pts[1] - pts[0]).normalize_or_zero();
    if d == DVec2::ZERO {
        return None;
    }
    // u: whichever side direction is closest to +x.
    let cands = [d, -d, d.perp(), -d.perp()];
    let u = cands.into_iter().max_by(|a, b| a.x.total_cmp(&b.x))?;
    let v = u.perp();
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in &pts {
        let q = DVec2::new(p.dot(u), p.dot(v));
        lo = lo.min(q);
        hi = hi.max(q);
    }
    let size = hi - lo;
    Some(RectFrame { origin: u * lo.x + v * lo.y, u, v, width: size.x, height: size.y })
}

impl SketchEntity {
    /// The four lines of a rectangle, in order around it, from any two opposite corners (as the reference draws it).
    pub fn rectangle_lines(a: DVec2, b: DVec2) -> [SketchEntity; 4] {
        let (lo, hi) = (a.min(b), a.max(b));
        let c = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
        [0, 1, 2, 3].map(|i| SketchEntity::Line { a: c[i], b: c[(i + 1) % 4] })
    }

    /// Corners of a regular polygon, in order.
    pub fn polygon_points(center: DVec2, radius: f64, sides: u32, angle: f64) -> Vec<DVec2> {
        let n = sides.clamp(3, 64);
        (0..n)
            .map(|i| {
                let a = angle + i as f64 / n as f64 * std::f64::consts::TAU;
                center + DVec2::new(a.cos(), a.sin()) * radius
            })
            .collect()
    }

    /// A rectangle from any two opposite corners.
    pub fn rectangle(a: DVec2, b: DVec2) -> Self {
        SketchEntity::Rectangle { min: a.min(b), max: a.max(b) }
    }

    pub fn is_closed(&self) -> bool {
        !matches!(self, SketchEntity::Line { .. })
    }

    /// Every closed or open polyline the element is drawn with (text has one per glyph contour).
    pub fn outlines(&self) -> Vec<Vec<DVec2>> {
        match self {
            SketchEntity::Text { origin, text, size, angle, font } => {
                crate::text::outlines_in(font.as_deref(), text, *origin, *size, *angle)
            }
            _ => vec![self.outline()],
        }
    }

    /// Outline as a polyline in sketch coordinates (closed shapes repeat the first point). Text gives its
    /// bounding box.
    pub fn outline(&self) -> Vec<DVec2> {
        match *self {
            SketchEntity::Text { .. } => {
                let pts: Vec<DVec2> = self.outlines().into_iter().flatten().collect();
                let (min, max) =
                    pts.iter().fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                if pts.is_empty() {
                    return Vec::new();
                }
                vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y), min]
            }
            SketchEntity::Line { a, b } => vec![a, b],
            SketchEntity::Rectangle { min, max } => {
                vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y), min]
            }
            SketchEntity::Polygon { center, radius, sides, angle } => {
                let mut p = Self::polygon_points(center, radius, sides, angle);
                if let Some(first) = p.first().copied() {
                    p.push(first);
                }
                p
            }
            SketchEntity::Arc { a, mid, b } => arc_points(a, mid, b, 48),
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let (c, sn) = (angle.cos(), angle.sin());
                (0..=96)
                    .map(|i| {
                        let t = i as f64 / 96.0 * std::f64::consts::TAU;
                        let (x, y) = (rx * t.cos(), ry * t.sin());
                        center + DVec2::new(x * c - y * sn, x * sn + y * c)
                    })
                    .collect()
            }
            SketchEntity::Spline { ref points } => spline_points(points, 16),
            SketchEntity::ControlSpline { ref poles } => bspline_points(poles, 16),
            SketchEntity::Circle { center, radius } => {
                let n = 96;
                (0..=n)
                    .map(|i| {
                        let t = i as f64 / n as f64 * std::f64::consts::TAU;
                        center + DVec2::new(t.cos(), t.sin()) * radius
                    })
                    .collect()
            }
        }
    }
}

/// A closed cell of a sketch (holes included), found by the kernel from all the sketch's curves, the way
/// overlapping shapes split into separate pickable areas.
#[derive(Clone, Debug)]
pub struct Region {
    pub index: u32,
    pub face: Shape,
    pub mesh: Arc<Mesh>,
    pub area: f64,
    /// Center of the region in sketch coordinates.
    pub centroid: DVec2,
    /// The region as triangles in sketch coordinates (for picking and fills).
    pub triangles: Vec<[DVec2; 3]>,
}

impl Region {
    pub fn contains(&self, p: DVec2) -> bool {
        self.triangles.iter().any(|t| {
            let d = |a: DVec2, b: DVec2| (b - a).perp_dot(p - a);
            let (d0, d1, d2) = (d(t[0], t[1]), d(t[1], t[2]), d(t[2], t[0]));
            let neg = d0 < -1e-12 || d1 < -1e-12 || d2 < -1e-12;
            let pos = d0 > 1e-12 || d1 > 1e-12 || d2 > 1e-12;
            !(neg && pos)
        })
    }

    /// The region's boundary as segments (triangle edges no other triangle shares), grouped into loops; the loop
    /// enclosing the most area first (the outside), then the holes.
    pub fn loops(&self) -> Vec<Vec<[DVec2; 2]>> {
        use std::collections::HashMap;
        let key = |p: DVec2| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64);
        type Key = (i64, i64);
        let mut count: HashMap<(Key, Key), u32> = HashMap::new();
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(a), key(b));
                *count.entry(if ka < kb { (ka, kb) } else { (kb, ka) }).or_default() += 1;
            }
        }
        let mut segs = Vec::new();
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(a), key(b));
                if count.get(&if ka < kb { (ka, kb) } else { (kb, ka) }) == Some(&1) {
                    segs.push([a, b]);
                }
            }
        }
        // Loops are the connected groups of segments (union-find over their end points).
        let mut parent: HashMap<(i64, i64), (i64, i64)> = HashMap::new();
        fn find(parent: &mut HashMap<(i64, i64), (i64, i64)>, k: (i64, i64)) -> (i64, i64) {
            let mut r = k;
            while let Some(&p) = parent.get(&r) {
                if p == r {
                    break;
                }
                r = p;
            }
            parent.insert(k, r);
            r
        }
        for [a, b] in &segs {
            let (ra, rb) = (find(&mut parent, key(*a)), find(&mut parent, key(*b)));
            parent.insert(ra, rb);
            parent.entry(rb).or_insert(rb);
        }
        let mut groups: HashMap<(i64, i64), Vec<[DVec2; 2]>> = HashMap::new();
        for seg in segs {
            let r = find(&mut parent, key(seg[0]));
            groups.entry(r).or_default().push(seg);
        }
        let mut loops: Vec<Vec<[DVec2; 2]>> = groups.into_values().collect();
        let area = |l: &Vec<[DVec2; 2]>| l.iter().map(|[a, b]| a.perp_dot(*b)).sum::<f64>().abs() / 2.0;
        loops.sort_by(|a, b| area(b).total_cmp(&area(a)));
        loops
    }
}

/// Inside a closed outline given as segments (an even-odd ray test; the segments needn't be in order).
fn inside_outline(segs: &[[DVec2; 2]], p: DVec2) -> bool {
    let mut inside = false;
    for [a, b] in segs {
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// How a sketch's regions nest: for each region (in order), how many holes it has and the region it sits in a
/// hole of (the innermost), if any. The inside of an "o" is the region inside the letter; the ring between two
/// rectangles is the one with a hole.
pub fn region_nesting(regions: &[Region]) -> Vec<(usize, Option<u32>)> {
    let loops: Vec<Vec<Vec<[DVec2; 2]>>> = regions.iter().map(Region::loops).collect();
    let outer_area = |i: usize| loops[i].first().map_or(0.0, |l| l.iter().map(|[a, b]| a.perp_dot(*b)).sum::<f64>().abs() / 2.0);
    regions
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let holes = loops[i].len().saturating_sub(1);
            let probe = r.triangles.iter().max_by(|a, b| tri_area(a).total_cmp(&tri_area(b))).map(|t| (t[0] + t[1] + t[2]) / 3.0);
            let inside = probe.and_then(|p| {
                (0..regions.len())
                    .filter(|&j| j != i && !loops[j].is_empty() && inside_outline(&loops[j][0], p) && !regions[j].contains(p))
                    .min_by(|&a, &b| outer_area(a).total_cmp(&outer_area(b)))
                    .map(|j| regions[j].index)
            });
            (holes, inside)
        })
        .collect()
}

fn tri_area(t: &[DVec2; 3]) -> f64 {
    (t[1] - t[0]).perp_dot(t[2] - t[0]).abs()
}

/// A sketch pattern that stays linked: its copies are remade from the source curves whenever those change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchPattern {
    /// The curves patterned.
    pub source: Vec<u32>,
    /// Each copy's placement, as a 2D affine map in sketch coordinates (column-major 2×3).
    pub maps: Vec<[f64; 6]>,
    /// The curves each (map, source curve) pair made, maps outermost.
    pub copies: Vec<Vec<u32>>,
}

#[derive(Clone, Debug)]
pub struct Sketch {
    pub id: ItemId,
    pub name: String,
    pub plane: SketchPlane,
    pub entities: Vec<SketchEntity>,
    pub visible: bool,
    /// The body whose face this sketch sits on, so extrusions merge with it.
    pub on_body: Option<ItemId>,
    /// Constraints between its elements, kept by the solver.
    pub constraints: Vec<crate::Constraint>,
    /// Elements drawn as construction geometry: dashed, and no part of regions or profiles.
    pub construction: std::collections::BTreeSet<u32>,
    /// Linked patterns of its curves.
    pub patterns: Vec<SketchPattern>,
    /// Derived from the entities by the engine (see [`Sketch::regions_key`]); not saved.
    pub regions: Arc<Vec<Region>>,
    pub regions_for: u64,
}

impl PartialEq for Sketch {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
            && self.name == o.name
            && self.plane == o.plane
            && self.entities == o.entities
            && self.visible == o.visible
            && self.on_body == o.on_body
            && self.constraints == o.constraints
            && self.construction == o.construction
            && self.patterns == o.patterns
    }
}

impl Sketch {
    pub fn new(id: ItemId, name: String, plane: SketchPlane, on_body: Option<ItemId>) -> Self {
        Self {
            id,
            name,
            plane,
            entities: Vec::new(),
            visible: true,
            on_body,
            constraints: Vec::new(),
            construction: Default::default(),
            patterns: Vec::new(),
            regions: Arc::default(),
            regions_for: 0,
        }
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn region(&self, index: u32) -> Option<&Region> {
        self.regions.iter().find(|r| r.index == index)
    }

    /// Changes whenever the regions need recomputing.
    pub fn regions_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for v in [self.plane.origin, self.plane.x, self.plane.y] {
            v.to_array().map(f64::to_bits).hash(&mut h);
        }
        for e in &self.entities {
            serde_json::to_string(e).unwrap_or_default().hash(&mut h);
        }
        self.construction.hash(&mut h);
        h.finish() | 1
    }

    pub fn frame(&self) -> PlaneFrame {
        PlaneFrame { origin: self.plane.origin, x: self.plane.x, y: self.plane.y }
    }

    /// The sketch's curves in world space, for the kernel.
    pub fn curves(&self) -> Vec<PlanarCurve> {
        let w = |p: DVec2| self.plane.to_world(p);
        let mut out = Vec::new();
        for (i, e) in self.entities.iter().enumerate() {
            if self.construction.contains(&(i as u32)) {
                continue;
            }
            match *e {
                SketchEntity::Line { a, b } => out.push(PlanarCurve::Line { a: w(a), b: w(b) }),
                SketchEntity::Circle { center, radius } => out.push(PlanarCurve::Circle { center: w(center), radius }),
                SketchEntity::Arc { a, mid, b } => out.push(PlanarCurve::Arc { a: w(a), mid: w(mid), b: w(b) }),
                SketchEntity::Ellipse { center, rx, ry, angle } => {
                    let major = self.plane.x * angle.cos() + self.plane.y * angle.sin();
                    out.push(PlanarCurve::Ellipse { center: w(center), major, rx, ry });
                }
                SketchEntity::Spline { ref points } => out.push(PlanarCurve::Spline { points: points.iter().map(|p| w(*p)).collect() }),
                SketchEntity::ControlSpline { ref poles } => {
                    out.push(PlanarCurve::BSpline { poles: poles.iter().map(|p| w(*p)).collect() })
                }
                SketchEntity::Rectangle { .. } | SketchEntity::Polygon { .. } | SketchEntity::Text { .. } => {
                    for pts in e.outlines() {
                        for seg in pts.windows(2) {
                            out.push(PlanarCurve::Line { a: w(seg[0]), b: w(seg[1]) });
                        }
                    }
                }
            }
        }
        out
    }

    /// The sketch elements that run along a region's boundary (the curves that make it).
    pub fn region_entities(&self, region: u32) -> Vec<u32> {
        let Some(r) = self.region(region) else { return Vec::new() };
        let boundary: Vec<Vec<DVec2>> = r
            .mesh
            .edges
            .iter()
            .map(|line| line.iter().map(|p| self.plane.to_local(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))).collect())
            .collect();
        let size = boundary.iter().flatten().fold(0.0f64, |m, p| m.max(p.length())).max(1.0);
        let tol = size * 1e-4;
        let near = |q: DVec2| {
            boundary.iter().any(|line| {
                line.windows(2).any(|w| {
                    let d = w[1] - w[0];
                    let t = if d.length_squared() > 0.0 { ((q - w[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                    (w[0] + d * t).distance(q) < tol
                })
            })
        };
        (0..self.entities.len() as u32)
            .filter(|&i| self.entities[i as usize].outlines().iter().any(|o| o.windows(2).any(|w| near((w[0] + w[1]) / 2.0))))
            .collect()
    }

    pub fn outline_points(&self) -> impl Iterator<Item = DVec3> + '_ {
        self.entities.iter().flat_map(|e| e.outlines().into_iter().flatten().map(|p| self.plane.to_world(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_of_four_lines_are_found() {
        let mut es: Vec<SketchEntity> = vec![SketchEntity::Line { a: DVec2::new(-5.0, 0.0), b: DVec2::new(-1.0, 3.0) }];
        es.extend(SketchEntity::rectangle_lines(DVec2::new(4.0, 1.0), DVec2::new(0.0, 3.0)));
        let lp = rectangle_loop(&es, 3).expect("a loop");
        assert_eq!(lp.iter().copied().collect::<std::collections::BTreeSet<_>>(), [1, 2, 3, 4].into());
        let f = rectangle_frame(&es, lp).unwrap();
        assert!((f.width - 4.0).abs() < 1e-12 && (f.height - 2.0).abs() < 1e-12 && f.origin.distance(DVec2::new(0.0, 1.0)) < 1e-12);
        assert!(rectangle_loop(&es, 0).is_none());
        // A turned one too.
        let r = glam::DAffine2::from_angle(0.4);
        let turned: Vec<SketchEntity> = SketchEntity::rectangle_lines(DVec2::ZERO, DVec2::new(3.0, 1.0))
            .iter()
            .map(|e| match *e {
                SketchEntity::Line { a, b } => SketchEntity::Line { a: r.transform_point2(a), b: r.transform_point2(b) },
                _ => unreachable!(),
            })
            .collect();
        let f = rectangle_frame(&turned, rectangle_loop(&turned, 2).unwrap()).unwrap();
        assert!((f.width - 3.0).abs() < 1e-9 && (f.height - 1.0).abs() < 1e-9);
    }

    #[test]
    fn plane_frames_are_orthonormal() {
        let p = SketchPlane::from_normal(DVec3::ONE, DVec3::new(0.0, -1.0, 0.0)).unwrap_or(SketchPlane::XY);
        assert!((p.normal() - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9);
        let w = p.to_world(DVec2::new(2.0, 3.0));
        assert!((p.to_local(w) - DVec2::new(2.0, 3.0)).length() < 1e-9);
    }

    #[test]
    fn arcs_go_through_their_middle_point() {
        let pts = arc_points(DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0), DVec2::new(-1.0, 0.0), 32);
        assert!(pts.iter().all(|p| (p.length() - 1.0).abs() < 1e-9));
        assert!(pts.iter().all(|p| p.y >= -1e-9), "upper half only");
        let pts = arc_points(DVec2::new(1.0, 0.0), DVec2::new(0.0, -1.0), DVec2::new(-1.0, 0.0), 32);
        assert!(pts.iter().all(|p| p.y <= 1e-9), "lower half only");
    }
}

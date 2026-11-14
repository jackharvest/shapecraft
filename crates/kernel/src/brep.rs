//! A kernel-neutral boundary representation: exact surfaces and curves plus faces, loops, edges and vertices.
//! File formats that carry exact B-reps (Parasolid XT) convert to and from this; a kernel builds its own
//! shapes from it (`Kernel::from_brep`) and describes its shapes with it (`Kernel::to_brep`).
//!
//! Lengths are millimeters. Senses follow the usual conventions: a face's normal is its surface's natural
//! normal unless `flip`; a loop runs with the face on its left seen from the face's normal side; an edge runs
//! along its curve's natural direction unless `reversed`; a fin runs along its edge unless `!forward`.

use glam::{DVec2, DVec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Nurbs<P> {
    pub degree: u32,
    pub poles: Vec<P>,
    /// One weight per pole when rational.
    pub weights: Option<Vec<f64>>,
    /// Distinct knots and their multiplicities.
    pub knots: Vec<f64>,
    pub mults: Vec<u32>,
    pub periodic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NurbsSurface {
    pub u_degree: u32,
    pub v_degree: u32,
    pub nu: usize,
    pub nv: usize,
    /// `nu * nv` poles, u major: pole (i, j) is at `i * nv + j`.
    pub poles: Vec<DVec3>,
    pub weights: Option<Vec<f64>>,
    pub u_knots: Vec<f64>,
    pub u_mults: Vec<u32>,
    pub v_knots: Vec<f64>,
    pub v_mults: Vec<u32>,
    pub u_periodic: bool,
    pub v_periodic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Plane {
        origin: DVec3,
        normal: DVec3,
        x: DVec3,
    },
    Cylinder {
        origin: DVec3,
        axis: DVec3,
        x: DVec3,
        radius: f64,
    },
    /// `radius` at `origin`; the radius grows along `axis` by `tan(half_angle)` (negative shrinks).
    Cone {
        origin: DVec3,
        axis: DVec3,
        x: DVec3,
        radius: f64,
        half_angle: f64,
    },
    Sphere {
        center: DVec3,
        axis: DVec3,
        x: DVec3,
        radius: f64,
    },
    /// `(major + minor*cos(v)) * (x*cos(u) + y*sin(u)) + minor*axis*sin(v)` about `center`.
    /// A negative major radius describes the inner (lemon) part of a spindle torus; `abs(major) < minor`.
    Torus {
        center: DVec3,
        axis: DVec3,
        x: DVec3,
        major: f64,
        minor: f64,
    },
    BSpline(NurbsSurface),
    /// Offset along the basis surface's natural normal.
    Offset {
        basis: usize,
        distance: f64,
    },
    /// A curve spun about an axis.
    Revolution {
        curve: usize,
        origin: DVec3,
        axis: DVec3,
    },
    /// A curve swept along a direction (the vector's length is irrelevant).
    Extrusion {
        curve: usize,
        dir: DVec3,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line {
        origin: DVec3,
        dir: DVec3,
    },
    Circle {
        center: DVec3,
        normal: DVec3,
        x: DVec3,
        radius: f64,
    },
    Ellipse {
        center: DVec3,
        normal: DVec3,
        x: DVec3,
        major: f64,
        minor: f64,
    },
    BSpline(Nurbs<DVec3>),
    /// A smooth curve through these points (approximated intersection curves).
    Through(Vec<DVec3>),
    /// A 2D B-spline in the parameter space of a surface.
    OnSurface {
        surface: usize,
        uv: Nurbs<DVec2>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub curve: Option<usize>,
    pub reversed: bool,
    /// Both `None` for a closed edge with no vertex (a full circle).
    pub start: Option<usize>,
    pub end: Option<usize>,
    /// Set for tolerant edges.
    pub tolerance: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fin {
    pub edge: usize,
    pub forward: bool,
    /// The fin's own curve in its face's (u, v) (a `Curve::OnSurface` on the face's surface), when the source gives
    /// one; it runs along the edge's direction. A kernel uses it instead of projecting the edge onto the face.
    pub pcurve: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub surface: usize,
    pub flip: bool,
    pub loops: Vec<Vec<Fin>>,
    /// sRGB 0..1, when the face has its own colour.
    pub color: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyKind {
    #[default]
    Solid,
    Sheet,
    Wire,
}

/// One body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Brep {
    pub kind: BodyKind,
    pub surfaces: Vec<Surface>,
    pub curves: Vec<Curve>,
    pub vertices: Vec<DVec3>,
    pub edges: Vec<Edge>,
    pub faces: Vec<Face>,
    /// Faces grouped by shell (a solid with a void has two).
    pub shells: Vec<Vec<usize>>,
}

impl Brep {
    /// Moves the body rigidly: `rot` (a rotation, no scale or mirror) then `shift` (millimeters).
    pub fn transform_rigid(&mut self, rot: glam::DMat3, shift: DVec3) {
        let p = |v: &mut DVec3| *v = rot * *v + shift;
        let d = |v: &mut DVec3| *v = rot * *v;
        for s in &mut self.surfaces {
            match s {
                Surface::Plane { origin, normal, x } => {
                    p(origin);
                    d(normal);
                    d(x);
                }
                Surface::Cylinder { origin, axis, x, .. } | Surface::Cone { origin, axis, x, .. } => {
                    p(origin);
                    d(axis);
                    d(x);
                }
                Surface::Sphere { center, axis, x, .. } | Surface::Torus { center, axis, x, .. } => {
                    p(center);
                    d(axis);
                    d(x);
                }
                Surface::BSpline(n) => {
                    n.poles.iter_mut().for_each(p);
                }
                Surface::Revolution { origin, axis, .. } => {
                    p(origin);
                    d(axis);
                }
                Surface::Extrusion { dir, .. } => {
                    d(dir);
                }
                Surface::Offset { .. } => {}
            }
        }
        for c in &mut self.curves {
            match c {
                Curve::Line { origin, dir } => {
                    p(origin);
                    d(dir);
                }
                Curve::Circle { center, normal, x, .. } | Curve::Ellipse { center, normal, x, .. } => {
                    p(center);
                    d(normal);
                    d(x);
                }
                Curve::BSpline(n) => {
                    n.poles.iter_mut().for_each(p);
                }
                Curve::Through(pts) => {
                    pts.iter_mut().for_each(p);
                }
                Curve::OnSurface { .. } => {}
            }
        }
        self.vertices.iter_mut().for_each(p);
    }
}

/// Flat `f64` stream used to hand a [`Brep`] across a C ABI. Integers are stored exactly as doubles.
pub mod flat {
    use super::*;

    struct W(Vec<f64>);
    impl W {
        fn n(&mut self, v: usize) {
            self.0.push(v as f64);
        }
        fn f(&mut self, v: f64) {
            self.0.push(v);
        }
        fn b(&mut self, v: bool) {
            self.0.push(if v { 1.0 } else { 0.0 });
        }
        fn v(&mut self, v: DVec3) {
            self.0.extend_from_slice(&[v.x, v.y, v.z]);
        }
        fn opt(&mut self, v: Option<usize>) {
            self.0.push(v.map(|i| i as f64).unwrap_or(-1.0));
        }
        fn knots(&mut self, knots: &[f64], mults: &[u32]) {
            self.n(knots.len());
            knots.iter().for_each(|&k| self.f(k));
            mults.iter().for_each(|&m| self.n(m as usize));
        }
    }

    /// `[kind, n_surfaces, surfaces…, n_curves, curves…, n_vertices, xyz…, n_edges, edges…, n_faces, faces…,
    /// n_shells, shells…]`; see `cpp/shim.cpp` `sk_from_brep` for the reader.
    pub fn encode(b: &Brep) -> Vec<f64> {
        let mut w = W(Vec::new());
        w.n(match b.kind {
            BodyKind::Solid => 0,
            BodyKind::Sheet => 1,
            BodyKind::Wire => 2,
        });
        w.n(b.surfaces.len());
        for s in &b.surfaces {
            match s {
                Surface::Plane { origin, normal, x } => {
                    w.n(0);
                    w.v(*origin);
                    w.v(*normal);
                    w.v(*x);
                }
                Surface::Cylinder { origin, axis, x, radius } => {
                    w.n(1);
                    w.v(*origin);
                    w.v(*axis);
                    w.v(*x);
                    w.f(*radius);
                }
                Surface::Cone { origin, axis, x, radius, half_angle } => {
                    w.n(2);
                    w.v(*origin);
                    w.v(*axis);
                    w.v(*x);
                    w.f(*radius);
                    w.f(*half_angle);
                }
                Surface::Sphere { center, axis, x, radius } => {
                    w.n(3);
                    w.v(*center);
                    w.v(*axis);
                    w.v(*x);
                    w.f(*radius);
                }
                Surface::Torus { center, axis, x, major, minor } => {
                    w.n(4);
                    w.v(*center);
                    w.v(*axis);
                    w.v(*x);
                    w.f(*major);
                    w.f(*minor);
                }
                Surface::BSpline(s) => {
                    w.n(5);
                    w.n(s.u_degree as usize);
                    w.n(s.v_degree as usize);
                    w.n(s.nu);
                    w.n(s.nv);
                    w.b(s.u_periodic);
                    w.b(s.v_periodic);
                    w.b(s.weights.is_some());
                    s.poles.iter().for_each(|p| w.v(*p));
                    if let Some(ws) = &s.weights {
                        ws.iter().for_each(|&x| w.f(x));
                    }
                    w.knots(&s.u_knots, &s.u_mults);
                    w.knots(&s.v_knots, &s.v_mults);
                }
                Surface::Offset { basis, distance } => {
                    w.n(6);
                    w.n(*basis);
                    w.f(*distance);
                }
                Surface::Revolution { curve, origin, axis } => {
                    w.n(7);
                    w.n(*curve);
                    w.v(*origin);
                    w.v(*axis);
                }
                Surface::Extrusion { curve, dir } => {
                    w.n(8);
                    w.n(*curve);
                    w.v(*dir);
                }
            }
        }
        w.n(b.curves.len());
        for c in &b.curves {
            match c {
                Curve::Line { origin, dir } => {
                    w.n(0);
                    w.v(*origin);
                    w.v(*dir);
                }
                Curve::Circle { center, normal, x, radius } => {
                    w.n(1);
                    w.v(*center);
                    w.v(*normal);
                    w.v(*x);
                    w.f(*radius);
                }
                Curve::Ellipse { center, normal, x, major, minor } => {
                    w.n(2);
                    w.v(*center);
                    w.v(*normal);
                    w.v(*x);
                    w.f(*major);
                    w.f(*minor);
                }
                Curve::BSpline(n) => {
                    w.n(3);
                    w.n(n.degree as usize);
                    w.n(n.poles.len());
                    w.b(n.periodic);
                    w.b(n.weights.is_some());
                    n.poles.iter().for_each(|p| w.v(*p));
                    if let Some(ws) = &n.weights {
                        ws.iter().for_each(|&x| w.f(x));
                    }
                    w.knots(&n.knots, &n.mults);
                }
                Curve::Through(points) => {
                    w.n(4);
                    w.n(points.len());
                    points.iter().for_each(|p| w.v(*p));
                }
                Curve::OnSurface { surface, uv } => {
                    w.n(5);
                    w.n(*surface);
                    w.n(uv.degree as usize);
                    w.n(uv.poles.len());
                    w.b(uv.periodic);
                    w.b(uv.weights.is_some());
                    uv.poles.iter().for_each(|p| {
                        w.f(p.x);
                        w.f(p.y);
                    });
                    if let Some(ws) = &uv.weights {
                        ws.iter().for_each(|&x| w.f(x));
                    }
                    w.knots(&uv.knots, &uv.mults);
                }
            }
        }
        w.n(b.vertices.len());
        b.vertices.iter().for_each(|p| w.v(*p));
        w.n(b.edges.len());
        for e in &b.edges {
            w.opt(e.curve);
            w.b(e.reversed);
            w.opt(e.start);
            w.opt(e.end);
            w.f(e.tolerance.unwrap_or(-1.0));
        }
        w.n(b.faces.len());
        for f in &b.faces {
            w.n(f.surface);
            w.b(f.flip);
            w.n(f.loops.len());
            for l in &f.loops {
                w.n(l.len());
                for fin in l {
                    w.n(fin.edge);
                    w.b(fin.forward);
                    w.opt(fin.pcurve);
                }
            }
        }
        w.n(b.shells.len());
        for s in &b.shells {
            w.n(s.len());
            s.iter().for_each(|&f| w.n(f));
        }
        w.0
    }

    struct R<'a> {
        d: &'a [f64],
        at: usize,
    }
    impl R<'_> {
        fn f(&mut self) -> Option<f64> {
            let v = *self.d.get(self.at)?;
            self.at += 1;
            Some(v)
        }
        /// A count or index: finite, non-negative and below `cap`.
        fn n(&mut self, cap: usize) -> Option<usize> {
            let v = self.f()?;
            (v.is_finite() && v >= 0.0 && v < cap as f64).then_some(v as usize)
        }
        fn b(&mut self) -> Option<bool> {
            Some(self.f()? != 0.0)
        }
        fn v(&mut self) -> Option<DVec3> {
            Some(DVec3::new(self.f()?, self.f()?, self.f()?))
        }
        fn opt(&mut self) -> Option<Option<usize>> {
            let v = self.f()?;
            Some((v >= 0.0 && v.is_finite()).then_some(v as usize))
        }
        fn knots(&mut self) -> Option<(Vec<f64>, Vec<u32>)> {
            let n = self.n(CAP)?;
            let k = (0..n).map(|_| self.f()).collect::<Option<Vec<_>>>()?;
            let m = (0..n).map(|_| self.n(1 << 16).map(|m| m as u32)).collect::<Option<Vec<_>>>()?;
            Some((k, m))
        }
        fn many<T>(&mut self, n: usize, mut f: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
            (0..n).map(|_| f(self)).collect()
        }
    }
    const CAP: usize = 1 << 24;

    /// The inverse of [`encode`] (the same layout comes back from `sk_to_brep`). `None` on malformed data.
    pub fn decode(d: &[f64]) -> Option<Brep> {
        let mut r = R { d, at: 0 };
        let kind = match r.n(3)? {
            0 => BodyKind::Solid,
            1 => BodyKind::Sheet,
            _ => BodyKind::Wire,
        };
        let ns = r.n(CAP)?;
        let surfaces = r.many(ns, |r| {
            Some(match r.n(9)? {
                0 => Surface::Plane { origin: r.v()?, normal: r.v()?, x: r.v()? },
                1 => Surface::Cylinder { origin: r.v()?, axis: r.v()?, x: r.v()?, radius: r.f()? },
                2 => Surface::Cone { origin: r.v()?, axis: r.v()?, x: r.v()?, radius: r.f()?, half_angle: r.f()? },
                3 => Surface::Sphere { center: r.v()?, axis: r.v()?, x: r.v()?, radius: r.f()? },
                4 => Surface::Torus { center: r.v()?, axis: r.v()?, x: r.v()?, major: r.f()?, minor: r.f()? },
                5 => {
                    let (u_degree, v_degree) = (r.n(64)? as u32, r.n(64)? as u32);
                    let (nu, nv) = (r.n(CAP)?, r.n(CAP)?);
                    let (u_periodic, v_periodic, rational) = (r.b()?, r.b()?, r.b()?);
                    let count = nu.checked_mul(nv).filter(|&c| c < CAP)?;
                    let poles = r.many(count, |r| r.v())?;
                    let weights = if rational { Some(r.many(count, |r| r.f())?) } else { None };
                    let (u_knots, u_mults) = r.knots()?;
                    let (v_knots, v_mults) = r.knots()?;
                    Surface::BSpline(NurbsSurface {
                        u_degree,
                        v_degree,
                        nu,
                        nv,
                        poles,
                        weights,
                        u_knots,
                        u_mults,
                        v_knots,
                        v_mults,
                        u_periodic,
                        v_periodic,
                    })
                }
                6 => Surface::Offset { basis: r.n(CAP)?, distance: r.f()? },
                7 => Surface::Revolution { curve: r.n(CAP)?, origin: r.v()?, axis: r.v()? },
                _ => Surface::Extrusion { curve: r.n(CAP)?, dir: r.v()? },
            })
        })?;
        let nc = r.n(CAP)?;
        let curves = r.many(nc, |r| {
            Some(match r.n(6)? {
                0 => Curve::Line { origin: r.v()?, dir: r.v()? },
                1 => Curve::Circle { center: r.v()?, normal: r.v()?, x: r.v()?, radius: r.f()? },
                2 => Curve::Ellipse { center: r.v()?, normal: r.v()?, x: r.v()?, major: r.f()?, minor: r.f()? },
                3 => {
                    let degree = r.n(64)? as u32;
                    let n = r.n(CAP)?;
                    let (periodic, rational) = (r.b()?, r.b()?);
                    let poles = r.many(n, |r| r.v())?;
                    let weights = if rational { Some(r.many(n, |r| r.f())?) } else { None };
                    let (knots, mults) = r.knots()?;
                    Curve::BSpline(Nurbs { degree, poles, weights, knots, mults, periodic })
                }
                4 => {
                    let n = r.n(CAP)?;
                    Curve::Through(r.many(n, |r| r.v())?)
                }
                _ => {
                    let surface = r.n(CAP)?;
                    let degree = r.n(64)? as u32;
                    let n = r.n(CAP)?;
                    let (periodic, rational) = (r.b()?, r.b()?);
                    let poles = r.many(n, |r| Some(DVec2::new(r.f()?, r.f()?)))?;
                    let weights = if rational { Some(r.many(n, |r| r.f())?) } else { None };
                    let (knots, mults) = r.knots()?;
                    Curve::OnSurface { surface, uv: Nurbs { degree, poles, weights, knots, mults, periodic } }
                }
            })
        })?;
        let nv = r.n(CAP)?;
        let vertices = r.many(nv, |r| r.v())?;
        let ne = r.n(CAP)?;
        let edges = r.many(ne, |r| {
            let (curve, reversed, start, end) = (r.opt()?, r.b()?, r.opt()?, r.opt()?);
            let tol = r.f()?;
            Some(Edge { curve, reversed, start, end, tolerance: (tol >= 0.0).then_some(tol) })
        })?;
        let nf = r.n(CAP)?;
        let faces = r.many(nf, |r| {
            let (surface, flip) = (r.n(CAP)?, r.b()?);
            let nl = r.n(CAP)?;
            let loops = r.many(nl, |r| {
                let n = r.n(CAP)?;
                r.many(n, |r| Some(Fin { edge: r.n(CAP)?, forward: r.b()?, pcurve: r.opt()? }))
            })?;
            Some(Face { surface, flip, loops, color: None })
        })?;
        let nsh = r.n(CAP)?;
        let shells = r.many(nsh, |r| {
            let n = r.n(CAP)?;
            r.many(n, |r| r.n(CAP))
        })?;
        Some(Brep { kind, surfaces, curves, vertices, edges, faces, shells })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_round_trip() {
        let b = Brep {
            kind: BodyKind::Solid,
            surfaces: vec![Surface::Plane { origin: DVec3::ZERO, normal: DVec3::Z, x: DVec3::X }],
            curves: vec![
                Curve::Line { origin: DVec3::ZERO, dir: DVec3::X },
                Curve::BSpline(Nurbs {
                    degree: 2,
                    poles: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
                    weights: Some(vec![1.0, 0.5, 1.0]),
                    knots: vec![0.0, 1.0],
                    mults: vec![3, 3],
                    periodic: false,
                }),
            ],
            vertices: vec![DVec3::ZERO, DVec3::X],
            edges: vec![Edge { curve: Some(0), reversed: false, start: Some(0), end: Some(1), tolerance: None }],
            faces: vec![Face { surface: 0, flip: true, loops: vec![vec![Fin { edge: 0, forward: true, pcurve: None }]], color: None }],
            shells: vec![vec![0]],
        };
        assert_eq!(flat::decode(&flat::encode(&b)), Some(b));
        assert_eq!(flat::decode(&[0.0, 5.0]), None);
    }
}

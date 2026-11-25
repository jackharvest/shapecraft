//! A geometry kernel in pure Rust, behind the same [`Kernel`] trait as OCCT.
//!
//! Exact B-rep modelling is [Arris](https://github.com/arris-labs/arris) (MIT/Apache-2.0, pure Rust, checked against
//! OCCT as its test oracle): planes, cylinders, cones, spheres, tori and NURBS; booleans, fillets, chamfers, shells,
//! offsets, sweeps, lofts, revolves and STEP. This crate adapts it to ShapeCraft's trait: sketch regions (our own
//! planar arrangement, `regions`), face and edge numbering, display meshes, profiles from faces. Mesh bodies (STL and
//! friends) stay polyhedra (`poly`). What neither does yet answers "not in the Rust kernel yet", never a wrong shape.
//!
//! Choose it with `--kernel rust` or `SHAPECRAFT_KERNEL=rust` (see the `shapecraft-kernels` crate).

mod brep_io;
mod brep_step;
mod hlr;
mod iges;
mod offset2d;
pub mod poly;
mod profile;
pub mod regions;
mod remove;
mod step;
mod surgery;
mod unify;
mod varfillet;
mod wrap;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use arris::geom::{Curve, FrameLaw, Path as APath, PathSegment, Profile, Surface};
use arris::math::nalgebra::{Unit, UnitQuaternion};
use arris::math::{Axis as AAxis, Frame, Isometry, Point3, Reflection, Vec3};
use arris::ops::{Control, OpError};
use arris::topo::{Body, Model, Orientation};
use glam::DVec2;
pub use poly::{Kind, Poly, PolyKernel, RsShape};
use profile::{circle_profile, plane_frame, polygon_profile, profile_of, profile_of_shape};
use shapecraft_kernel::{
    Aabb, Axis, BoolOp, DVec3, Distance, EdgeCurve, FacePlane, Feature, Kernel, KernelError, MassProps, Mesh, PartInfo, PlanarCurve,
    PlaneFrame, Result, Shape, ShapeData, SubShape, Support, ViewFrame, ViewLine,
};

pub(crate) type AFace = arris::topo::Face;
type AEdge = arris::topo::Edge;

pub(crate) const C: &Control<'static> = &Control::NONE;

/// The pure-Rust kernel.
#[derive(Debug, Default, Clone, Copy)]
pub struct RustKernel;

impl RustKernel {
    pub fn new() -> Self {
        Self
    }
}

/// An exact shape: Arris bodies in a model of their own (or one face of one, for profiles).
#[derive(Debug, Clone)]
pub struct ArShape {
    pub model: Arc<Model>,
    pub bodies: Vec<Body>,
    /// Set when this shape is one face of `bodies[0]`.
    pub face: Option<AFace>,
    /// The planar profile this face was made from, when it came from a sketch.
    pub profile: Option<Profile>,
    /// What's been measured of it: a shape never changes, and the engine asks again and again.
    memo: Arc<Memo>,
}

#[derive(Debug, Default)]
struct Memo {
    bounds: std::sync::OnceLock<Aabb>,
    mass: std::sync::OnceLock<MassProps>,
}

impl ArShape {
    pub fn new(model: Arc<Model>, bodies: Vec<Body>, face: Option<AFace>) -> Self {
        Self { model, bodies, face, profile: None, memo: Arc::default() }
    }
}

impl ShapeData for ArShape {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Free curves with no faces: what a 2D offset makes. Only their edges can be read back.
#[derive(Debug, Clone)]
pub struct Curves(pub Vec<EdgeCurve>);

impl ShapeData for Curves {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Curves {
    /// Sketch pieces in `frame`'s plane, as world curves.
    fn of(frame: &PlaneFrame, segs: &[regions::Seg]) -> Self {
        let w = |p: DVec2| frame.origin + frame.x * p.x + frame.y * p.y;
        let normal = frame.x.cross(frame.y).normalize_or_zero();
        Curves(
            segs.iter()
                .map(|s| match *s {
                    regions::Seg::Line { a, b } => EdgeCurve::Line { a: w(a), b: w(b) },
                    regions::Seg::Arc { c, r, sweep, .. } if sweep.abs() >= std::f64::consts::TAU - 1e-12 => {
                        EdgeCurve::Circle { center: w(c), normal, radius: r }
                    }
                    regions::Seg::Arc { .. } => EdgeCurve::Arc { a: w(s.at(0.0)), mid: w(s.at(0.5)), b: w(s.at(1.0)) },
                })
                .collect(),
        )
    }

    fn points(c: &EdgeCurve) -> Vec<DVec3> {
        let n = 32;
        match c {
            EdgeCurve::Line { a, b } => vec![*a, *b],
            EdgeCurve::Circle { center, normal, radius } => {
                let x = normal.any_orthonormal_vector();
                let y = normal.cross(x);
                (0..=n)
                    .map(|i| {
                        *center
                            + (x * (i as f64 / n as f64 * std::f64::consts::TAU).cos()
                                + y * (i as f64 / n as f64 * std::f64::consts::TAU).sin())
                                * *radius
                    })
                    .collect()
            }
            EdgeCurve::Arc { a, mid, b } => vec![*a, *mid, *b],
            EdgeCurve::Other { points } => points.clone(),
        }
    }
}

pub(crate) fn not_yet(op: &'static str) -> KernelError {
    KernelError::Operation { op, message: "not in the Rust kernel yet".into() }
}

pub(crate) fn bad(message: impl Into<String>) -> KernelError {
    KernelError::InvalidInput(message.into())
}

fn op(name: &'static str) -> impl Fn(OpError) -> KernelError {
    move |e| KernelError::Operation { op: name, message: e.to_string() }
}

pub(crate) fn p3(v: DVec3) -> Point3 {
    Point3::new(v.x, v.y, v.z)
}

pub(crate) fn v3(v: DVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn dp(p: Point3) -> DVec3 {
    DVec3::new(p.x, p.y, p.z)
}

fn dv(v: &Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

pub(crate) fn sign(o: Orientation) -> f64 {
    if o == Orientation::Reversed { -1.0 } else { 1.0 }
}

enum Data<'a> {
    Exact(&'a ArShape),
    Mesh,
}

fn data(shape: &Shape) -> Result<Data<'_>> {
    if let Ok(a) = shape.downcast::<ArShape>() {
        return Ok(Data::Exact(a));
    }
    if shape.downcast::<RsShape>().is_ok() {
        return Ok(Data::Mesh);
    }
    if shape.downcast::<Curves>().is_ok() {
        return Err(bad("these are only curves"));
    }
    Err(bad("this shape was made by another kernel"))
}

pub(crate) fn exact(shape: &Shape) -> Result<&ArShape> {
    match data(shape)? {
        Data::Exact(a) => Ok(a),
        Data::Mesh => Err(bad("a mesh body has no exact surfaces for this; import it as STEP to model on it")),
    }
}

/// The shape as exact bodies: itself, or a mesh body's polyhedra as flat-faced solids (or sheets) for writing and
/// measuring. Each triangle becomes a face, so this is for files and queries, never for modelling on.
pub(crate) fn exact_or_faceted(shape: &Shape) -> Result<std::borrow::Cow<'_, ArShape>> {
    match data(shape)? {
        Data::Exact(a) => Ok(std::borrow::Cow::Borrowed(a)),
        Data::Mesh => {
            let parts = &shape.downcast::<RsShape>()?.parts;
            let mut m = Model::default();
            let mut bodies = Vec::new();
            for (kind, poly) in parts {
                let made = RustKernel.from_brep(&poly.brep(*kind))?;
                bodies.extend(bring(&mut m, exact(&made)?)?);
            }
            Ok(std::borrow::Cow::Owned(pack(&m, &bodies)?))
        }
    }
}

/// A solid body's separate pieces (a cut that went right through), each with its voids, as bodies of their own.
fn lumps_apart(m: &mut Model, b: Body) -> Result<Vec<Body>> {
    use arris::topo::builder::{Assembly, Builder, FaceSpec};
    let lumps = arris::check::lumps(m, b).map_err(|e| KernelError::Operation { op: "solids", message: format!("{e:?}") })?;
    if lumps.len() < 2 {
        return Ok(vec![b]);
    }
    let fail = |e: &dyn std::fmt::Display| KernelError::Operation { op: "solids", message: e.to_string() };
    let mut out = Vec::with_capacity(lumps.len());
    for lump in lumps {
        let mut shells = Vec::new();
        for shell in std::iter::once(lump.outer).chain(lump.voids) {
            shells.push(m.shell(shell.id).map_err(|e| fail(&e))?.faces().iter().copied().map(FaceSpec::Keep).collect());
        }
        let assembly = Assembly { shells, ..Assembly::default() };
        let (builder, _) = Builder::assemble(m, m.precision().default_tolerance, assembly).map_err(|e| fail(&e))?;
        out.push(builder.finish(m, arris::topo::entity::BodyKind::Solid).map_err(|e| fail(&e))?.body);
    }
    Ok(out)
}

/// Whether every face across the edges of `f` stands square to it (a prism of it would run along them).
fn walls_square_to(s: &ArShape, f: AFace) -> bool {
    let m = &s.model;
    let Ok(face) = m.face(f.id) else { return false };
    let faces = faces_of(s);
    let bordering = edge_faces(s);
    face.loops().iter().flat_map(|l| l.coedges()).all(|c| {
        let Some(n) = normal_at_edge(m, f, c.edge()) else { return false };
        bordering
            .get(&c.edge())
            .into_iter()
            .flatten()
            .filter_map(|&g| faces.get(g as usize))
            .all(|&(_, other)| other.id == f.id || normal_at_edge(m, other, c.edge()).is_some_and(|w| w.dot(n).abs() < 1e-9))
    })
}

/// The face's outward normal at the middle of one of its edges, from the edge's curve in the face's (u, v).
pub(crate) fn normal_at_edge(m: &Model, f: AFace, edge: arris::topo::EdgeId) -> Option<DVec3> {
    normal_along_edge(m, f, edge, 0.5)
}

/// The face's outward normal at the point `at` (0..1) of the way along one of its edges.
fn normal_along_edge(m: &Model, f: AFace, edge: arris::topo::EdgeId, at: f64) -> Option<DVec3> {
    let face = m.face(f.id).ok()?;
    let c = face.loops().iter().flat_map(|l| l.coedges()).find(|c| c.edge() == edge)?;
    let (_, range) = m.edge(edge).ok()?.curve()?;
    let uv = m.curve2(c.pcurve()).ok()?.point(range.lo() + (range.hi() - range.lo()) * at);
    let n = m.surface(face.surface()).ok()?.normal(uv.x, uv.y)?;
    Some(dv(&n.into_inner()) * sign(f.orientation))
}

/// Volume, area and centroid, exactly (Arris integrates over the B-rep): a sheet or a face has area only.
fn mass_of(s: &ArShape) -> Result<MassProps> {
    mass_with(s, C)
}

/// [`mass_of`] under `control`: a poll stops it early (an `Operation` error).
fn mass_with(s: &ArShape, control: &Control<'_>) -> Result<MassProps> {
    if let Some(f) = s.face {
        let p = arris::ops::measure::face_surface_properties(&s.model, f, control).map_err(op("measure"))?;
        return Ok(MassProps { volume: 0.0, area: p.area, centroid: dp(p.centroid) });
    }
    let (mut vol, mut area, mut c_v, mut c_a) = (0.0, 0.0, DVec3::ZERO, DVec3::ZERO);
    for b in &s.bodies {
        match arris::ops::measure::mass_properties(&s.model, *b, control) {
            Ok(p) => {
                vol += p.volume;
                area += p.area;
                c_v += dp(p.centroid) * p.volume;
                c_a += dp(p.centroid) * p.area;
            }
            Err(_) => {
                let p = arris::ops::measure::surface_properties(&s.model, *b, control).map_err(op("measure"))?;
                area += p.area;
                c_a += dp(p.centroid) * p.area;
            }
        }
    }
    let centroid = if vol.abs() > 0.0 {
        c_v / vol
    } else if area > 0.0 {
        c_a / area
    } else {
        DVec3::ZERO
    };
    Ok(MassProps { volume: vol, area, centroid })
}

/// A box grown to hold what's added to it.
#[derive(Clone, Copy)]
struct Bounds {
    lo: DVec3,
    hi: DVec3,
}

impl Bounds {
    const EMPTY: Bounds = Bounds { lo: DVec3::INFINITY, hi: DVec3::NEG_INFINITY };

    fn add(&mut self, bb: arris::math::Aabb) {
        self.lo = self.lo.min(DVec3::from_array(bb.min));
        self.hi = self.hi.max(DVec3::from_array(bb.max));
    }

    /// The edge's curve over its range.
    fn edge(&mut self, m: &Model, e: arris::topo::EdgeId) {
        let Some((cid, range)) = m.edge(e).ok().and_then(|x| x.curve()) else { return };
        if let Some(bb) = m.curve(cid).ok().and_then(|c| c.bounds(range)) {
            self.add(bb);
        }
    }

    /// A curved face's surface over the stretch of (u, v) its loops span (a sphere's bulge between its edges); a
    /// spline face, whose surface only knows its control points' box, from a coarse mesh of it. A flat face adds
    /// nothing its edges don't.
    fn face(&mut self, m: &Model, b: Body, f: AFace) {
        let Ok(face) = m.face(f.id) else { return };
        let Ok(surface) = m.surface(face.surface()) else { return };
        match surface {
            Surface::Plane { .. } => {}
            Surface::Nurbs(_) => {
                let request = arris::mesh::MeshRequest::new(0.05);
                if let Ok(tm) = arris::mesh::tessellate_faces(m, b, &[f.id], &request, C) {
                    tm.positions().iter().for_each(|p| self.add(arris::math::Aabb::of_point(arris::math::Point3::from(*p))));
                }
            }
            _ => {
                let mut uv: Vec<arris::math::Point2> = Vec::new();
                for c in face.loops().iter().flat_map(|l| l.coedges()) {
                    let (Ok(edge), Ok(p)) = (m.edge(c.edge()), m.curve2(c.pcurve())) else { continue };
                    let Some((_, range)) = edge.curve() else { continue };
                    uv.extend((0..=8).map(|i| p.point(range.lo() + (range.hi() - range.lo()) * i as f64 / 8.0)));
                }
                let span = |k: usize| {
                    let (a, b) = uv.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| (a.min(q[k]), b.max(q[k])));
                    arris::math::Interval::new(a, b).ok()
                };
                if let (Some(u), Some(v)) = (span(0), span(1))
                    && let Some(bb) = surface.bounds([u, v])
                {
                    self.add(bb);
                }
            }
        }
    }

    /// One body of a model, whole.
    fn body(m: &Model, b: Body) -> Bounds {
        let mut bb = Bounds::EMPTY;
        m.edges(b).unwrap_or_default().iter().for_each(|e| bb.edge(m, e.id));
        m.faces(b).unwrap_or_default().iter().for_each(|f| bb.face(m, b, *f));
        bb
    }

    fn aabb(self) -> Option<Aabb> {
        (self.lo.is_finite() && self.hi.is_finite()).then_some(Aabb { min: self.lo, max: self.hi })
    }
}

/// The box around a shape, exact (see [`Bounds`]).
fn bounds_of(s: &ArShape) -> Result<Aabb> {
    let mut bb = Bounds::EMPTY;
    edges_of(s).iter().for_each(|(_, e)| bb.edge(&s.model, e.id));
    faces_of(s).iter().for_each(|(b, f)| bb.face(&s.model, *b, *f));
    bb.aabb().ok_or_else(|| bad("an empty shape has no bounds"))
}

/// What `slot` holds, worked out by `make` the first time.
fn remembered<T: Copy>(slot: &std::sync::OnceLock<T>, make: impl FnOnce() -> Result<T>) -> Result<T> {
    if let Some(v) = slot.get() {
        return Ok(*v);
    }
    let v = make()?;
    Ok(*slot.get_or_init(|| v))
}

/// A flat face (a sheet of one face) from a profile, keeping the profile for extruding it exactly.
fn profile_face(profile: Profile) -> Result<Shape> {
    let mut m = Model::default();
    let (b, _) = arris::ops::planar_face(&mut m, &profile, C).map_err(op("face"))?;
    let mut a = pack(&m, &[b])?;
    a.profile = Some(profile);
    Ok(wrap(a))
}

/// Dense copies of `bodies` in a model of their own.
pub(crate) fn pack(m: &Model, bodies: &[Body]) -> Result<ArShape> {
    let mut fresh = Model::default();
    let mut out = Vec::with_capacity(bodies.len());
    for b in bodies {
        out.push(fresh.import(m, *b).map_err(|e| KernelError::Operation { op: "copy", message: e.to_string() })?.0);
    }
    Ok(ArShape::new(Arc::new(fresh), out, None))
}

pub(crate) fn wrap(a: ArShape) -> Shape {
    Shape::new(a)
}

/// The shape's bodies, copied into `m`.
pub(crate) fn bring(m: &mut Model, s: &ArShape) -> Result<Vec<Body>> {
    s.bodies
        .iter()
        .map(|b| m.import(&s.model, *b).map(|r| r.0).map_err(|e| KernelError::Operation { op: "copy", message: e.to_string() }))
        .collect()
}

pub(crate) fn faces_of(s: &ArShape) -> Vec<(Body, AFace)> {
    if let (Some(f), Some(b)) = (s.face, s.bodies.first()) {
        return vec![(*b, f)];
    }
    s.bodies.iter().flat_map(|b| s.model.faces(*b).unwrap_or_default().into_iter().map(move |f| (*b, f))).collect()
}

pub(crate) fn edges_of(s: &ArShape) -> Vec<(Body, AEdge)> {
    if let Some(f) = s.face {
        let Ok(face) = s.model.face(f.id) else { return Vec::new() };
        let mut out: Vec<(Body, AEdge)> = Vec::new();
        for c in face.loops().iter().flat_map(|l| l.coedges()) {
            if !out.iter().any(|(_, e)| e.id == c.edge()) {
                out.push((s.bodies[0], c.edge_use()));
            }
        }
        return out;
    }
    s.bodies.iter().flat_map(|b| s.model.edges(*b).unwrap_or_default().into_iter().map(move |e| (*b, e))).collect()
}

pub(crate) fn face_at(s: &ArShape, i: u32) -> Result<(Body, AFace)> {
    faces_of(s).get(i as usize).copied().ok_or_else(|| bad(format!("no face {i}")))
}

pub(crate) fn edge_at(s: &ArShape, i: u32) -> Result<(Body, AEdge)> {
    edges_of(s).get(i as usize).copied().ok_or_else(|| bad(format!("no edge {i}")))
}

/// The faces each edge borders (edge id → face indices).
pub(crate) fn edge_faces(s: &ArShape) -> HashMap<arris::topo::EdgeId, Vec<u32>> {
    let mut map: HashMap<_, Vec<u32>> = HashMap::new();
    for (i, (_, f)) in faces_of(s).iter().enumerate() {
        if let Ok(face) = s.model.face(f.id) {
            for c in face.loops().iter().flat_map(|l| l.coedges()) {
                let v = map.entry(c.edge()).or_default();
                if !v.contains(&(i as u32)) || v.len() == 1 {
                    v.push(i as u32);
                }
            }
        }
    }
    map
}

pub(crate) fn curve_of(m: &Model, e: AEdge) -> Option<(Curve, f64, f64)> {
    let edge = m.edge(e.id).ok()?;
    let (cid, range) = edge.curve()?;
    let curve = m.curve(cid).ok()?.clone();
    Some((curve, range.lo(), range.hi()))
}

/// Points along an edge: its ends for a straight one, else `n` pieces (`None`: a piece every 0.1 rad round a circle
/// or ellipse, 64 along any other curve, as OCCT's drawings sample them).
pub(crate) fn edge_points(m: &Model, e: AEdge, n: Option<usize>) -> Vec<DVec3> {
    match curve_of(m, e) {
        Some((c, lo, hi)) => {
            let n = match (&c, n) {
                (Curve::Line { .. }, _) => 1,
                (_, Some(n)) => n,
                (Curve::Circle { .. } | Curve::Ellipse { .. }, None) => (((hi - lo).abs() / 0.1).ceil() as usize).clamp(2, 4096),
                (_, None) => 64,
            };
            (0..=n).map(|i| dp(c.point(lo + (hi - lo) * i as f64 / n as f64))).collect()
        }
        None => Vec::new(),
    }
}

fn solid_from(
    name: &'static str,
    f: impl FnOnce(&mut Model) -> std::result::Result<(Body, arris::topo::Provenance), OpError>,
) -> Result<Shape> {
    let mut m = Model::default();
    let (b, _) = f(&mut m).map_err(op(name))?;
    Ok(wrap(pack(&m, &[b])?))
}

fn extrude_profile(p: &Profile, dir: DVec3) -> Result<Shape> {
    let len = dir.length();
    if len.is_nan() || len <= 0.0 {
        return Err(bad("extrude by a non-zero distance"));
    }
    solid_from("extrude", |m| arris::ops::extrude(m, p, v3(dir / len), len, C))
}

/// The axis, turned around for a negative angle (Arris revolves by positive angles only).
fn a_axis(axis: Axis, angle: f64) -> Result<(AAxis, f64)> {
    let dir = if angle < 0.0 { -axis.dir } else { axis.dir };
    Ok((AAxis::new(p3(axis.origin), v3(dir)).map_err(|e| bad(format!("axis: {e}")))?, angle.abs()))
}

/// Runs `f` on each body of the shape with the indexed sub-shapes that belong to it.
fn per_body<T: Copy>(
    s: &ArShape,
    items: &[(Body, T)],
    pick: &[u32],
    mut f: impl FnMut(&mut Model, Body, Vec<T>) -> Result<Body>,
) -> Result<Shape> {
    let mut m = (*s.model).clone();
    let mut out = Vec::new();
    for b in &s.bodies {
        let mine: Vec<T> = pick.iter().filter_map(|i| items.get(*i as usize)).filter(|(ob, _)| ob == b).map(|(_, t)| *t).collect();
        if mine.is_empty() {
            out.push(*b);
        } else {
            out.push(f(&mut m, *b, mine)?);
        }
    }
    if pick.iter().any(|i| *i as usize >= items.len()) {
        return Err(bad("no such edge or face"));
    }
    Ok(wrap(pack(&m, &out)?))
}

fn map_bodies(
    s: &ArShape,
    name: &'static str,
    mut f: impl FnMut(&mut Model, Body) -> std::result::Result<(Body, arris::topo::Provenance), OpError>,
) -> Result<Shape> {
    let mut m = (*s.model).clone();
    let mut out = Vec::new();
    for b in &s.bodies {
        out.push(f(&mut m, *b).map_err(op(name))?.0);
    }
    let mut packed = pack(&m, &out)?;
    if let (Some(_), Some(p)) = (&s.face, &s.profile) {
        packed.profile = Some(p.clone());
    }
    Ok(wrap(packed))
}

fn mesh_of(s: &ArShape, deflection: f64) -> Result<Mesh> {
    let chord = deflection.max(1e-4);
    let faces = faces_of(s);
    let index: HashMap<arris::topo::FaceId, u32> = faces.iter().enumerate().map(|(i, (_, f))| (f.id, i as u32)).collect();
    let edges = edges_of(s);
    let eindex: HashMap<arris::topo::EdgeId, u32> = edges.iter().enumerate().map(|(i, (_, e))| (e.id, i as u32)).collect();
    let bordering = edge_faces(s);
    let mut mesh = Mesh::default();
    for b in &s.bodies {
        let tm = match s.face {
            Some(f) => arris::mesh::tessellate_faces(&s.model, *b, &[f.id], &arris::mesh::MeshRequest::new(chord), C),
            None => arris::mesh::tessellate(&s.model, *b, chord, C),
        }
        .map_err(|e| KernelError::Operation { op: "display mesh", message: e.to_string() })?;
        let pos = tm.positions();
        for fr in tm.faces() {
            let Some(&fi) = index.get(&fr.face) else { continue };
            let mut local: HashMap<u32, u32> = HashMap::new();
            let base = mesh.positions.len() as u32;
            let mut normals: Vec<DVec3> = Vec::new();
            for t in &tm.triangles()[fr.triangles.clone()] {
                let mut ids = [0u32; 3];
                for k in 0..3 {
                    let next = base + local.len() as u32;
                    ids[k] = *local.entry(t[k]).or_insert_with(|| {
                        let p = pos[t[k] as usize];
                        mesh.positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
                        normals.push(DVec3::ZERO);
                        next
                    });
                }
                let q = t.map(|i| DVec3::from_array(pos[i as usize]));
                let n = (q[1] - q[0]).cross(q[2] - q[0]);
                for id in ids {
                    normals[(id - base) as usize] += n;
                }
                mesh.indices.extend(ids);
                mesh.triangle_faces.push(fi);
            }
            mesh.normals.extend(normals.iter().map(|n| n.normalize_or_zero().as_vec3().to_array()));
        }
        for er in tm.edges() {
            let Some(&ei) = eindex.get(&er.edge) else { continue };
            let line: Vec<[f32; 3]> = tm.edge_indices()[er.indices.clone()].iter().map(|i| pos[*i as usize].map(|c| c as f32)).collect();
            let fs = bordering.get(&er.edge).cloned().unwrap_or_default();
            mesh.edges.push(line);
            mesh.edge_ids.push(ei);
            mesh.edge_faces.push([fs.first().copied().unwrap_or(u32::MAX), fs.get(1).copied().unwrap_or(u32::MAX)]);
        }
    }
    Ok(mesh)
}

fn path_of(curves: &[PlanarCurve]) -> Result<APath> {
    let mut start = None;
    let mut segments = Vec::new();
    for c in curves {
        match c {
            PlanarCurve::Line { a, b } => {
                start.get_or_insert(p3(*a));
                segments.push(PathSegment::LineTo(p3(*b)));
            }
            PlanarCurve::Arc { a, mid, b } => {
                start.get_or_insert(p3(*a));
                segments.push(PathSegment::ArcTo { via: p3(*mid), to: p3(*b) });
            }
            PlanarCurve::Spline { points } | PlanarCurve::BSpline { poles: points } => {
                let pts = match (c, shapecraft_kernel::spline::fit(points)) {
                    (PlanarCurve::Spline { .. }, Some(f)) => f.sample(16),
                    _ => points.clone(),
                };
                start.get_or_insert(p3(pts[0]));
                segments.extend(pts[1..].iter().map(|p| PathSegment::LineTo(p3(*p))));
            }
            PlanarCurve::Circle { .. } | PlanarCurve::Ellipse { .. } => return Err(not_yet("sweep along a closed curve")),
        }
    }
    Ok(APath { start: start.ok_or_else(|| bad("an empty path"))?, segments })
}

impl Kernel for RustKernel {
    fn name(&self) -> &'static str {
        "rust"
    }

    /// Kept to what the engine's checks show (`SHAPECRAFT_TEST_KERNEL=rust cargo test -p shapecraft-engine`).
    fn support(&self, feature: Feature) -> Support {
        use Feature as F;
        use Support::{Full, Limited, Missing};
        match feature {
            F::SketchRegions => Limited("splines and cut ellipses are followed as fine polylines"),
            F::OffsetEdge => Limited("chains of ellipses or splines are offset one curve at a time"),
            F::Sweep => Limited("no twist, scale or orientation settings, no closed paths, no body edges as the path"),
            F::Booleans => Limited("bodies that only touch can be refused; each cut slows as a face gathers holes"),
            F::Revolve => Limited("a thread takes a few seconds"),
            F::OffsetFaces => Limited("faces beside fillets or chamfers can be refused"),
            F::DeleteFaces => Limited("holes, pockets, slots and bosses"),
            F::Fillet => Limited("some corners where three rounds meet are refused"),
            F::ChordalFillet => Limited("where the faces meet at one angle along the edge"),
            F::Scale => Limited("uneven scaling of cones, spheres and tori"),
            F::TwoDistanceChamfer => Limited("outside edges: straight ones between flat faces, circles on cylinders and cones"),
            F::MoveEdge => Limited("circles round a flat face, and straight edges between flat faces"),
            F::Fill => Limited("flat openings (any opening on a mesh body)"),
            F::WrapEmboss => Limited("on cones, and across a face's edge, it takes up to a second"),
            F::VariableFillet => Limited("straight edges between flat faces, ending square on flat faces, not meeting at corners"),
            F::SetbackCorners => Missing,
            F::ProjectAsEdge => Limited("closed outlines that land inside one flat face"),
            F::Iges => Limited("solids and shells (B-rep); files of loose trimmed surfaces need Open CASCADE"),
            F::Loft => Limited("no guide curves, tangent ends or periodic lofts"),
            F::Extrude => Limited("To Object takes flat faces and planes"),
            F::DraftAngles | F::PushPull | F::Chamfer | F::Shell | F::SplitBody => Full,
            F::Drawings | F::Measure | F::MeshBodies | F::StepFiles | F::ExactImports => Full,
        }
    }

    fn make_box(&self, min: DVec3, size: DVec3) -> Result<Shape> {
        if size.is_nan() || size.min_element() <= 0.0 {
            return Err(bad("a box needs a positive size"));
        }
        solid_from("box", |m| arris::ops::primitive_box(m, p3(min), p3(min + size), C))
    }

    fn extrude_polygon(&self, points: &[DVec3], dir: DVec3) -> Result<Shape> {
        extrude_profile(&polygon_profile(points)?, dir)
    }

    fn extrude_circle(&self, center: DVec3, radius: f64, dir: DVec3) -> Result<Shape> {
        extrude_profile(&circle_profile(center, dir, radius)?, dir)
    }

    /// Only bodies whose boxes meet are handed to Arris together: a cut takes each body with just the tools that
    /// reach it (all of them in one pass), a union keeps bodies that touch nothing as bodies of their own, and an
    /// intersection skips pairs that can't share anything. Arris's booleans cost with every face in them, so a
    /// plate cut by forty pins is one pass instead of forty.
    fn boolean(&self, bop: BoolOp, a: &Shape, b: &Shape) -> Result<Shape> {
        let (a, b) = (exact(a)?, exact(b)?);
        let mut m = Model::default();
        let (ta, tb) = (bring(&mut m, a)?, bring(&mut m, b)?);
        if ta.is_empty() {
            return Err(bad("nothing to combine"));
        }
        let boxes: HashMap<Body, Option<Aabb>> = ta.iter().chain(&tb).map(|x| (*x, Bounds::body(&m, *x).aabb())).collect();
        let meet = |x: &Body, y: &Body| match (boxes.get(x).copied().flatten(), boxes.get(y).copied().flatten()) {
            (Some(p), Some(q)) => p.overlaps(&q),
            _ => true,
        };
        // OCCT's booleans leave one face where two pieces of one flat side meet; so do these.
        let tidy = |m: &mut Model, r: std::result::Result<(Body, arris::topo::Provenance), OpError>| -> Result<Body> {
            Ok(unify::planar_faces(m, r.map_err(op("boolean"))?.0))
        };
        let mut out = Vec::new();
        match bop {
            BoolOp::Subtract => {
                for t in &ta {
                    let tools: Vec<Body> = tb.iter().filter(|x| meet(t, x)).copied().collect();
                    if tools.is_empty() {
                        out.push(*t);
                    } else {
                        let cut = arris::ops::cut_many(&mut m, *t, &tools, C);
                        out.push(tidy(&mut m, cut)?);
                    }
                }
            }
            BoolOp::Union => {
                // Groups of bodies joined through boxes that meet; each group of two or more is fused.
                let all: Vec<Body> = ta.iter().chain(&tb).copied().collect();
                let mut group: Vec<usize> = (0..all.len()).collect();
                fn root(g: &mut [usize], x: usize) -> usize {
                    let mut r = x;
                    while g[r] != r {
                        r = g[r];
                    }
                    g[x] = r;
                    r
                }
                for i in 0..all.len() {
                    for j in i + 1..all.len() {
                        if meet(&all[i], &all[j]) {
                            let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                            group[ri] = rj;
                        }
                    }
                }
                let mut groups: Vec<Vec<Body>> = Vec::new();
                let mut at: HashMap<usize, usize> = HashMap::new();
                for (i, body) in all.iter().enumerate() {
                    let r = root(&mut group, i);
                    let k = *at.entry(r).or_insert_with(|| {
                        groups.push(Vec::new());
                        groups.len() - 1
                    });
                    groups[k].push(*body);
                }
                for g in groups {
                    match g.as_slice() {
                        [one] => out.push(*one),
                        _ => {
                            let fused = arris::ops::fuse_many(&mut m, &g, C);
                            out.push(tidy(&mut m, fused)?);
                        }
                    }
                }
            }
            BoolOp::Intersect => {
                for x in &ta {
                    for y in tb.iter().filter(|y| meet(x, y)) {
                        let common = arris::ops::common(&mut m, *x, *y, C);
                        out.push(tidy(&mut m, common)?);
                    }
                }
            }
        }
        Ok(wrap(pack(&m, &out)?))
    }

    /// A cut or a union with every tool in one pass (A − t1 − t2 is A − (t1 ∪ t2)); an intersection, where the order
    /// matters, one tool at a time.
    fn boolean_many(&self, op: BoolOp, target: &Shape, tools: &[Shape]) -> Result<Shape> {
        if op == BoolOp::Intersect || tools.len() < 2 {
            return tools.iter().try_fold(target.clone(), |acc, t| self.boolean(op, &acc, t));
        }
        let mut m = Model::default();
        let mut all = Vec::new();
        for t in tools {
            all.extend(bring(&mut m, exact(t)?)?);
        }
        self.boolean(op, target, &wrap(pack(&m, &all)?))
    }

    fn fillet_all_edges(&self, shape: &Shape, radius: f64) -> Result<Shape> {
        let s = exact(shape)?;
        let ef = edge_faces(s);
        let all: Vec<u32> = edges_of(s)
            .iter()
            .enumerate()
            .filter(|(_, (_, e))| ef.get(&e.id).is_some_and(|f| f.len() == 2 && f[0] != f[1]))
            .map(|(i, _)| i as u32)
            .collect();
        self.fillet_edges(shape, &all, radius)
    }

    fn mass_props(&self, shape: &Shape) -> Result<MassProps> {
        match data(shape)? {
            Data::Mesh => PolyKernel.mass_props(shape),
            Data::Exact(s) => remembered(&s.memo.mass, || mass_of(s)),
        }
    }

    /// From the display triangles (a chord of a thousandth of the shape's size): quick on any shape, and the same
    /// every time.
    fn mass_props_mesh(&self, shape: &Shape) -> Result<MassProps> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.mass_props(shape),
            Data::Exact(s) => s,
        };
        let bb = self.bounds(shape)?;
        let chord = ((bb.max - bb.min).length() * 1e-3).clamp(1e-3, 0.1);
        let mut m = mesh_of(s, chord)?.mass_props();
        if s.face.is_some() || s.bodies.iter().all(|b| s.model.body(b.id).is_ok_and(|x| x.kind() != arris::topo::entity::BodyKind::Solid)) {
            m.volume = 0.0;
        }
        Ok(m)
    }

    /// Exact when Arris gets there within `budget` seconds (it stops at the deadline), else from the triangles.
    fn mass_props_within(&self, shape: &Shape, budget: f64) -> Result<(MassProps, bool)> {
        let s = match data(shape)? {
            Data::Mesh => return Ok((PolyKernel.mass_props(shape)?, false)),
            Data::Exact(s) => s,
        };
        if let Some(m) = s.memo.mass.get() {
            return Ok((*m, false));
        }
        if !(budget.is_finite() && budget > 0.0) {
            return Ok((self.mass_props(shape)?, false));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(budget.min(3600.0));
        let late = move || std::time::Instant::now() > deadline;
        match mass_with(s, &Control::poll(&late)) {
            Ok(m) => Ok((*s.memo.mass.get_or_init(|| m), false)),
            Err(_) if std::time::Instant::now() > deadline => Ok((self.mass_props_mesh(shape)?, true)),
            Err(e) => Err(e),
        }
    }

    fn bounds(&self, shape: &Shape) -> Result<Aabb> {
        if let Ok(c) = shape.downcast::<Curves>() {
            let pts: Vec<DVec3> = c.0.iter().flat_map(Curves::points).collect();
            let (lo, hi) = pts.iter().fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(l, h), p| (l.min(*p), h.max(*p)));
            return if lo.is_finite() { Ok(Aabb { min: lo, max: hi }) } else { Err(bad("no curves")) };
        }
        match data(shape)? {
            Data::Mesh => PolyKernel.bounds(shape),
            Data::Exact(s) => remembered(&s.memo.bounds, || bounds_of(s)),
        }
    }

    fn tessellate(&self, shape: &Shape, deflection: f64) -> Result<Mesh> {
        if let Ok(c) = shape.downcast::<Curves>() {
            let mut mesh = Mesh::default();
            for (i, curve) in c.0.iter().enumerate() {
                mesh.edges.push(Curves::points(curve).iter().map(|p| p.as_vec3().to_array()).collect());
                mesh.edge_ids.push(i as u32);
                mesh.edge_faces.push([u32::MAX; 2]);
            }
            return Ok(mesh);
        }
        match data(shape)? {
            Data::Mesh => PolyKernel.tessellate(shape, deflection),
            Data::Exact(s) => mesh_of(s, deflection),
        }
    }

    fn face_count(&self, shape: &Shape) -> Result<u32> {
        match data(shape)? {
            Data::Mesh => PolyKernel.face_count(shape),
            Data::Exact(s) => Ok(faces_of(s).len() as u32),
        }
    }

    fn face_plane(&self, shape: &Shape, face: u32) -> Result<Option<FacePlane>> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.face_plane(shape, face),
            Data::Exact(s) => s,
        };
        let (_, f) = face_at(s, face)?;
        let entity = s.model.face(f.id).map_err(|e| bad(e.to_string()))?;
        let Ok(Surface::Plane { frame }) = s.model.surface(entity.surface()) else { return Ok(None) };
        let normal = dv(&frame.z().into_inner()) * sign(f.orientation);
        let c = arris::ops::measure::face_surface_properties(&s.model, f, C).map_err(op("measure"))?;
        Ok(Some(FacePlane { origin: dp(c.centroid), normal }))
    }

    fn extrude_face(&self, shape: &Shape, face: u32, distance: f64) -> Result<Shape> {
        let s = exact(shape)?;
        let (_, f) = face_at(s, face)?;
        // Every wall square to the face: moving the face makes what a prism fused on would (and avoids fusing two
        // solids along whole coplanar walls).
        if walls_square_to(s, f) {
            let moved = self.offset_faces(shape, &[face], distance);
            if moved.as_ref().is_ok_and(|m| self.mass_props(m).is_ok_and(|p| p.volume > 0.0)) {
                return moved;
            }
        }
        let p = profile_of(&s.model, f)?;
        let n = dv(&p.plane.z().into_inner());
        let tool = extrude_profile(&p, n * distance)?;
        self.boolean(if distance >= 0.0 { BoolOp::Union } else { BoolOp::Subtract }, shape, &tool)
    }

    fn fillet_edges(&self, shape: &Shape, edges: &[u32], radius: f64) -> Result<Shape> {
        let s = exact(shape)?;
        per_body(s, &edges_of(s), edges, |m, b, es| Ok(arris::ops::fillet(m, b, &es, radius, C).map_err(op("fillet"))?.0))
    }

    /// Chordal sizes become radii where the faces meet at one angle all along each edge (chord = 2r·sin(θ/2), θ the
    /// turn between their normals) and all the edges come to one radius. Setback corners aren't in Arris.
    fn fillet_edges_ex(&self, shape: &Shape, edges: &[u32], size: f64, chordal: bool, setback: f64) -> Result<Shape> {
        if setback > 0.0 {
            return Err(not_yet("setback corners"));
        }
        if !chordal {
            return self.fillet_edges(shape, edges, size);
        }
        let s = exact(shape)?;
        let all = edges_of(s);
        let faces = faces_of(s);
        let bordering = edge_faces(s);
        let mut radius: Option<f64> = None;
        for &i in edges {
            let (_, e) = all.get(i as usize).copied().ok_or_else(|| bad(format!("no edge {i}")))?;
            let sides: Vec<AFace> =
                bordering.get(&e.id).into_iter().flatten().filter_map(|&g| faces.get(g as usize)).map(|(_, f)| *f).collect();
            let [a, b] = sides[..] else { return Err(bad("a chordal fillet needs an edge between two faces")) };
            let turn = |at: f64| {
                let (na, nb) = (normal_along_edge(&s.model, a, e.id, at)?, normal_along_edge(&s.model, b, e.id, at)?);
                Some(na.dot(nb).clamp(-1.0, 1.0).acos())
            };
            let angles: Vec<f64> = [0.1, 0.5, 0.9].iter().filter_map(|t| turn(*t)).collect();
            let (lo, hi) = angles.iter().fold((f64::INFINITY, 0.0f64), |(l, h), x| (l.min(*x), h.max(*x)));
            if angles.len() < 3 || hi - lo > 1e-9 || lo < 1e-6 {
                return Err(not_yet("chordal fillets where the angle changes along the edge"));
            }
            let r = size / (2.0 * (lo / 2.0).sin());
            match radius {
                Some(prev) if (prev - r).abs() > 1e-9 * r => return Err(not_yet("chordal fillets of edges at different angles at once")),
                _ => radius = Some(r),
            }
        }
        self.fillet_edges(shape, edges, radius.ok_or_else(|| bad("pick edges to round"))?)
    }

    fn fillet_edges_variable(&self, shape: &Shape, edges: &[u32], r1: f64, r2: f64) -> Result<Shape> {
        if (r1 - r2).abs() <= 1e-12 * r1.abs().max(1.0) {
            return self.fillet_edges(shape, edges, r1);
        }
        varfillet::fillet_variable(exact(shape)?, edges, r1, r2)
    }

    fn translate(&self, shape: &Shape, by: DVec3) -> Result<Shape> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.translate(shape, by),
            Data::Exact(s) => s,
        };
        let motion = Isometry::from_translation(v3(by));
        map_bodies(s, "move", |m, b| arris::ops::transform(m, b, &motion, C))
    }

    fn planar_regions(&self, frame: PlaneFrame, curves: &[PlanarCurve]) -> Result<Vec<Shape>> {
        if curves.len() > 100_000 {
            return Err(bad("too many sketch curves"));
        }
        let plane = plane_frame(&frame)?;
        let ellipses = regions::Ellipse::all(&frame, curves);
        let exact = |c: &regions::Cycle| profile::region_loop(c, &ellipses);
        let mut out = Vec::new();
        for r in regions::regions(&regions::pieces(&frame, curves)) {
            let profile = Profile { plane, outer: exact(&r.outer), holes: r.holes.iter().map(exact).collect() };
            // A region Arris can't make into a face (a sliver, a self-touching loop) is left out, as OCCT does.
            if let Ok(face) = profile_face(profile) {
                out.push(face);
            }
        }
        Ok(out)
    }

    fn measure(&self, shape: &Shape, sub: SubShape) -> Result<(f64, DVec3)> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.measure(shape, sub),
            Data::Exact(s) => s,
        };
        match sub {
            SubShape::Whole => {
                let mp = self.mass_props(shape)?;
                Ok((if mp.volume != 0.0 { mp.volume } else { mp.area }, mp.centroid))
            }
            SubShape::Face(i) => {
                let (_, f) = face_at(s, i)?;
                let p = arris::ops::measure::face_surface_properties(&s.model, f, C).map_err(op("measure"))?;
                Ok((p.area, dp(p.centroid)))
            }
            SubShape::Edge(i) => {
                let (_, e) = edge_at(s, i)?;
                let (c, lo, hi) = curve_of(&s.model, e).ok_or_else(|| bad("a point-like edge"))?;
                let len = match &c {
                    Curve::Line { .. } => dp(c.point(lo)).distance(dp(c.point(hi))),
                    Curve::Circle { radius, .. } => radius * (hi - lo).abs(),
                    _ => edge_points(&s.model, e, Some(256)).windows(2).map(|w| w[0].distance(w[1])).sum(),
                };
                Ok((len, dp(c.point((lo + hi) / 2.0))))
            }
        }
    }

    fn distance(&self, a: &Shape, sa: SubShape, b: &Shape, sb: SubShape) -> Result<Distance> {
        let (xa, xb) = (exact_or_faceted(a)?, exact_or_faceted(b)?);
        let (xa, xb) = (xa.as_ref(), xb.as_ref());
        let mut m = Model::default();
        let operand = |m: &mut Model, s: &ArShape, sub: SubShape| -> Result<arris::ops::query::Operand> {
            let (body, map) =
                m.import(&s.model, s.bodies.first().copied().ok_or_else(|| bad("an empty shape"))?).map_err(|e| bad(e.to_string()))?;
            let shape: arris::topo::Shape = match sub {
                SubShape::Whole => match s.face {
                    Some(f) => arris::topo::Face::new(map.faces[&f.id], f.orientation).into(),
                    None => body.into(),
                },
                SubShape::Face(i) => {
                    let (_, f) = face_at(s, i)?;
                    arris::topo::Face::new(map.faces[&f.id], f.orientation).into()
                }
                SubShape::Edge(i) => {
                    let (_, e) = edge_at(s, i)?;
                    arris::topo::Edge::new(map.edges[&e.id], e.orientation).into()
                }
            };
            Ok(arris::ops::query::Operand::Shape(shape))
        };
        let (oa, ob) = (operand(&mut m, xa, sa)?, operand(&mut m, xb, sb)?);
        let d = arris::ops::query::distance(&m, oa, ob, C).map_err(op("distance"))?;
        Ok(Distance { distance: d.distance, a: dp(d.a.point), b: dp(d.b.point) })
    }

    fn prism(&self, face: &Shape, dir: DVec3) -> Result<Shape> {
        if let Data::Mesh = data(face)? {
            return PolyKernel.prism(face, dir);
        }
        extrude_profile(&profile_of_shape(face)?, dir)
    }

    fn merge_faces(&self, faces: &[Shape]) -> Result<Vec<Shape>> {
        if faces.len() < 2 {
            return Ok(faces.to_vec());
        }
        let profiles = faces.iter().map(profile_of_shape).collect::<Result<Vec<_>>>();
        let Some(merged) = profiles.ok().and_then(|p| profile::merged(&p)) else { return Ok(faces.to_vec()) };
        merged.into_iter().map(profile_face).collect()
    }

    /// A ruled loft from the profile to itself moved `dir` and shrunk by `|dir|·tan(angle)`: straight walls stay
    /// planes and round ones become cones, as OCCT's draft prism makes them.
    fn prism_draft(&self, face: &Shape, dir: DVec3, angle: f64) -> Result<Shape> {
        if !(dir.is_finite() && angle.is_finite()) {
            return Err(bad("extrude by finite numbers"));
        }
        if angle.abs() < 1e-12 {
            return self.prism(face, dir);
        }
        if angle.abs() >= std::f64::consts::FRAC_PI_2 * 0.95 {
            return Err(bad("draft angle must be under 85°"));
        }
        let p = profile_of_shape(face)?;
        let height = dir.length();
        let n = dv(&p.plane.z().into_inner());
        if height <= 0.0 || n.dot(dir).abs() < height * (1.0 - 1e-6) {
            return Err(bad("a drafted extrude goes straight out of the face"));
        }
        let mut end = profile::grown(&p, -height * angle.tan())?;
        let o = p.plane.origin() + v3(dir);
        end.plane = Frame::new(o, p.plane.z().into_inner(), p.plane.x().into_inner()).map_err(|e| bad(e.to_string()))?;
        solid_from("extrude", |m| arris::ops::loft(m, &[p, end], arris::ops::LoftKind::Ruled, false, C))
    }

    fn revolve(&self, face: &Shape, axis: Axis, angle: f64) -> Result<Shape> {
        let p = profile_of_shape(face)?;
        let (ax, angle) = a_axis(axis, angle)?;
        solid_from("revolve", |m| arris::ops::revolve(m, &p, ax, angle, C))
    }

    fn revolve_helix(&self, face: &Shape, axis: Axis, angle: f64, height: f64) -> Result<Shape> {
        if height.abs() < 1e-12 {
            return self.revolve(face, axis, angle);
        }
        // The shared checks first: a profile touching the axis, turns running into each other.
        let mesh = self.tessellate(face, 0.05)?;
        shapecraft_kernel::helix::path(&mesh, axis, angle, height)?;
        let p = profile_of_shape(face)?;
        let start = self.mass_props(face)?.centroid;
        let ax = AAxis::new(p3(axis.origin), v3(axis.dir)).map_err(|e| bad(format!("axis: {e}")))?;
        let pitch = height * std::f64::consts::TAU / angle;
        // A turn a segment, so each face of the coil winds once: what cuts it to length or joins it to a shank then
        // meets only the turns it reaches (box culling), not one face wound round many times.
        let turns = (angle.abs() / std::f64::consts::TAU).ceil().clamp(1.0, 10_000.0);
        let segments = (0..turns as usize).map(|_| PathSegment::Helix { axis: ax, pitch, angle: angle / turns }).collect();
        let path = APath { start: p3(start), segments };
        let law = FrameLaw::FixedBinormal(ax.direction);
        solid_from("helical revolve", |m| arris::ops::sweep(m, &p, &path, law, C))
    }

    fn shell(&self, shape: &Shape, open_faces: &[u32], thickness: f64) -> Result<Shape> {
        let s = exact(shape)?;
        if open_faces.is_empty() {
            let mut m = (*s.model).clone();
            let mut out = Vec::new();
            for b in &s.bodies {
                out.push(arris::ops::shell(&mut m, *b, &[], thickness.abs(), arris::ops::ShellSide::Inward, C).map_err(op("shell"))?.0);
            }
            return Ok(wrap(pack(&m, &out)?));
        }
        per_body(s, &faces_of(s), open_faces, |m, b, fs| {
            Ok(arris::ops::shell(m, b, &fs, thickness.abs(), arris::ops::ShellSide::Inward, C).map_err(op("shell"))?.0)
        })
    }

    fn fill(&self, shape: &Shape, edges: &[u32]) -> Result<Shape> {
        if edges.is_empty() || edges.len() > 10_000 {
            return Err(bad("fill needs the edges around an opening"));
        }
        match data(shape)? {
            Data::Mesh => poly::fill(shape, edges),
            Data::Exact(s) => surgery::fill(s, edges),
        }
    }

    fn chamfer_edges_two(&self, shape: &Shape, edges: &[u32], d1: f64, d2: f64, on_face: Option<u32>) -> Result<Shape> {
        if d1.is_finite() && (d1 - d2).abs() < 1e-12 * d1.abs().max(1.0) {
            return self.chamfer_edges(shape, edges, d1);
        }
        surgery::chamfer_two(shape, exact(shape)?, edges, d1, d2, on_face)
    }

    fn map_edge(&self, shape: &Shape, edge: u32, linear: shapecraft_kernel::DMat3, offset: DVec3) -> Result<Shape> {
        surgery::map_edge(exact(shape)?, edge, linear, offset)
    }

    fn chamfer_edges(&self, shape: &Shape, edges: &[u32], distance: f64) -> Result<Shape> {
        let s = exact(shape)?;
        per_body(s, &edges_of(s), edges, |m, b, es| Ok(arris::ops::chamfer(m, b, &es, distance, C).map_err(op("chamfer"))?.0))
    }

    fn revolve_polygon(&self, points: &[DVec3], axis: Axis, angle: f64) -> Result<Shape> {
        let p = polygon_profile(points)?;
        let (ax, angle) = a_axis(axis, angle)?;
        solid_from("revolve", |m| arris::ops::revolve(m, &p, ax, angle, C))
    }

    fn revolve_circle(&self, center: DVec3, normal: DVec3, radius: f64, axis: Axis, angle: f64) -> Result<Shape> {
        let p = circle_profile(center, normal, radius)?;
        let (ax, angle) = a_axis(axis, angle)?;
        solid_from("revolve", |m| arris::ops::revolve(m, &p, ax, angle, C))
    }

    fn rotate(&self, shape: &Shape, axis: Axis, angle: f64) -> Result<Shape> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.rotate(shape, axis, angle),
            Data::Exact(s) => s,
        };
        let dir = Unit::try_new(v3(axis.dir), 1e-300).ok_or_else(|| bad("a rotation needs an axis"))?;
        let r = UnitQuaternion::from_axis_angle(&dir, angle);
        let o = v3(axis.origin);
        let motion = Isometry::new(r, o - r * o);
        map_bodies(s, "rotate", |m, b| arris::ops::transform(m, b, &motion, C))
    }

    fn mirror(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Shape> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.mirror(shape, origin, normal),
            Data::Exact(s) => s,
        };
        let plane = Reflection::new(p3(origin), v3(normal)).map_err(|e| bad(e.to_string()))?;
        map_bodies(s, "mirror", |m, b| arris::ops::mirror(m, b, &plane, C))
    }

    /// Arris moves bodies only rigidly: a scale goes through the neutral B-rep (exact for a uniform scale; see
    /// [`brep::Brep::scale`](shapecraft_kernel::brep::Brep::scale) for an uneven one).
    fn scale(&self, shape: &Shape, center: DVec3, factors: DVec3) -> Result<Shape> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.scale(shape, center, factors),
            Data::Exact(_) if (factors - DVec3::ONE).abs().max_element() < 1e-12 => return Ok(shape.clone()),
            Data::Exact(s) => s,
        };
        if s.face.is_some() {
            return Err(not_yet("scale a single face"));
        }
        let mut m = Model::default();
        let mut bodies = Vec::new();
        for mut b in self.to_brep(shape)? {
            b.scale(center, factors).map_err(|e| KernelError::Operation { op: "scale", message: e.into() })?;
            bodies.extend(bring(&mut m, exact(&self.from_brep(&b)?)?)?);
        }
        Ok(wrap(pack(&m, &bodies)?))
    }

    fn offset_faces(&self, shape: &Shape, faces: &[u32], distance: f64) -> Result<Shape> {
        let s = exact(shape)?;
        per_body(s, &faces_of(s), faces, |m, b, fs| Ok(arris::ops::offset_faces(m, b, &fs, distance, C).map_err(op("offset faces"))?.0))
    }

    /// Holes, pockets, slots and bosses on a face come out whole (their rim is a hole in the faces kept, which then
    /// closes); faces whose rim cuts across a face need surfaces extended, which this kernel doesn't do yet.
    fn remove_faces(&self, shape: &Shape, faces: &[u32]) -> Result<Shape> {
        if faces.is_empty() || faces.len() > 10_000 {
            return Err(bad("pick 1 to 10000 faces to remove"));
        }
        let s = exact(shape)?;
        let all = faces_of(s);
        let mut m = (*s.model).clone();
        let mut out = Vec::new();
        for b in &s.bodies {
            let gone: std::collections::HashSet<arris::topo::FaceId> =
                faces.iter().filter_map(|i| all.get(*i as usize)).filter(|(ob, _)| ob == b).map(|(_, f)| f.id).collect();
            if gone.is_empty() {
                out.push(*b);
                continue;
            }
            out.push(remove::faces(&mut m, *b, &gone).ok_or_else(|| not_yet("removing faces that cut across a face"))?);
        }
        if faces.iter().any(|i| *i as usize >= all.len()) {
            return Err(bad("no such face"));
        }
        Ok(wrap(pack(&m, &out)?))
    }

    fn split_by_plane(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Vec<Shape>> {
        let s = exact(shape)?;
        let plane = Frame::from_z(p3(origin), v3(normal)).map_err(|e| bad(e.to_string()))?;
        let mut m = (*s.model).clone();
        let mut out = Vec::new();
        for b in &s.bodies {
            match arris::ops::split(&mut m, *b, &plane, C) {
                Ok(sp) => {
                    out.push(wrap(pack(&m, &[sp.positive])?));
                    out.push(wrap(pack(&m, &[sp.negative])?));
                }
                Err(e) => return Err(op("split")(e)),
            }
        }
        Ok(out)
    }

    fn solids(&self, shape: &Shape) -> Result<Vec<Shape>> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.solids(shape),
            Data::Exact(s) => s,
        };
        if s.face.is_some() {
            return Ok(Vec::new());
        }
        let mut m = (*s.model).clone();
        let mut out = Vec::new();
        for b in &s.bodies {
            if arris::ops::measure::mass_properties(&m, *b, C).is_err() {
                continue;
            }
            for lump in lumps_apart(&mut m, *b)? {
                out.push(wrap(pack(&m, &[lump])?));
            }
        }
        Ok(out)
    }

    fn axis_of(&self, shape: &Shape, sub: SubShape) -> Result<Option<Axis>> {
        let Data::Exact(s) = data(shape)? else { return Ok(None) };
        match sub {
            SubShape::Face(i) => {
                let (_, f) = face_at(s, i)?;
                let face = s.model.face(f.id).map_err(|e| bad(e.to_string()))?;
                Ok(match s.model.surface(face.surface()) {
                    Ok(
                        Surface::Cylinder { frame, .. }
                        | Surface::Cone { frame, .. }
                        | Surface::Sphere { frame, .. }
                        | Surface::Torus { frame, .. },
                    ) => Some(Axis::new(dp(frame.origin()), dv(&frame.z().into_inner()))),
                    _ => None,
                })
            }
            SubShape::Edge(i) => {
                let (_, e) = edge_at(s, i)?;
                Ok(match curve_of(&s.model, e) {
                    Some((Curve::Circle { frame, .. }, _, _)) => Some(Axis::new(dp(frame.origin()), dv(&frame.z().into_inner()))),
                    Some((Curve::Line { origin, direction }, _, _)) => Some(Axis::new(dp(origin), dv(&direction.into_inner()))),
                    _ => None,
                })
            }
            SubShape::Whole => Ok(None),
        }
    }

    /// A cylinder or torus face smooth with two neighbours (a hole's wall or a boss isn't), as OCCT's kernel
    /// decides it: convex when its outward normal points away from its centre of curvature.
    fn blend_of(&self, shape: &Shape, face: u32) -> Result<Option<(f64, bool)>> {
        let Data::Exact(s) = data(shape)? else { return Ok(None) };
        let (_, f) = face_at(s, face)?;
        let m = &s.model;
        let entity = m.face(f.id).map_err(|e| bad(e.to_string()))?;
        let (frame, radius, major) = match m.surface(entity.surface()) {
            Ok(Surface::Cylinder { frame, radius }) => (*frame, *radius, None),
            Ok(Surface::Torus { frame, major_radius, minor_radius }) => (*frame, *minor_radius, Some(*major_radius)),
            _ => return Ok(None),
        };
        let faces = faces_of(s);
        let bordering = edge_faces(s);
        let mut smooth = 0;
        let mut probe = None;
        for c in entity.loops().iter().flat_map(|l| l.coedges()) {
            let Some(n) = normal_at_edge(m, f, c.edge()) else { continue };
            probe.get_or_insert((c.edge(), n));
            for &g in bordering.get(&c.edge()).into_iter().flatten() {
                let Some(&(_, other)) = faces.get(g as usize) else { continue };
                if other.id != f.id && normal_at_edge(m, other, c.edge()).is_some_and(|n2| n.dot(n2) > 1.0 - 1e-6) {
                    smooth += 1;
                }
            }
        }
        let Some((edge, n)) = probe.filter(|_| smooth >= 2) else { return Ok(None) };
        let Some((curve, lo, hi)) = curve_of(m, AEdge::new(edge, Orientation::Forward)) else { return Ok(None) };
        let p = dp(curve.point((lo + hi) / 2.0));
        let (origin, axis) = (dp(frame.origin()), dv(&frame.z().into_inner()));
        let on_axis = origin + axis * (p - origin).dot(axis);
        let centre = match major {
            None => on_axis,
            Some(major) => origin + (p - on_axis).normalize_or_zero() * major,
        };
        Ok(Some((radius, n.dot(p - centre) > 0.0)))
    }

    fn edge_count(&self, shape: &Shape) -> Result<u32> {
        if let Ok(c) = shape.downcast::<Curves>() {
            return Ok(c.0.len() as u32);
        }
        match data(shape)? {
            Data::Mesh => PolyKernel.edge_count(shape),
            Data::Exact(s) => Ok(edges_of(s).len() as u32),
        }
    }

    fn hidden_lines(&self, shapes: &[Shape], view: ViewFrame, deflection: f64) -> Result<Vec<ViewLine>> {
        let exact = shapes.iter().map(exact_or_faceted).collect::<Result<Vec<_>>>()?;
        hlr::hidden_lines(&exact.iter().map(|s| s.as_ref()).collect::<Vec<_>>(), &view, deflection)
    }

    fn offset_curves(&self, frame: PlaneFrame, curves: &[PlanarCurve], distance: f64) -> Result<Shape> {
        if !distance.is_finite() {
            return Err(bad("offset by a finite distance"));
        }
        if curves.iter().any(|c| matches!(c, PlanarCurve::Ellipse { .. } | PlanarCurve::Spline { .. } | PlanarCurve::BSpline { .. })) {
            return Err(not_yet("offset ellipses and splines as a chain"));
        }
        let (chain, closed) = offset2d::chain(&regions::pieces(&frame, curves)).map_err(|e| bad(e.message()))?;
        // Positive is outwards round a closed chain, to the left along an open one (as OCCT's offset does).
        let left = if closed && offset2d::area(&chain) > 0.0 { -distance } else { distance };
        let out = offset2d::offset(&chain, left, closed).map_err(|e| bad(e.message()))?;
        Ok(Shape::new(Curves::of(&frame, &out)))
    }

    fn offset_face_outline(&self, face: &Shape, distance: f64) -> Result<Shape> {
        if !distance.is_finite() {
            return Err(bad("offset by a finite distance"));
        }
        let p = profile::grown(&profile_of_shape(face)?, distance)?;
        let frame = PlaneFrame { origin: dp(p.plane.origin()), x: dv(&p.plane.x().into_inner()), y: dv(&p.plane.y().into_inner()) };
        let mut segs = Vec::new();
        for l in std::iter::once(&p.outer).chain(&p.holes) {
            segs.extend(profile::loop_segs(l).ok_or_else(|| not_yet("offset an ellipse"))?);
        }
        Ok(Shape::new(Curves::of(&frame, &segs)))
    }

    fn edge_curve(&self, shape: &Shape, edge: u32) -> Result<EdgeCurve> {
        if let Ok(c) = shape.downcast::<Curves>() {
            return c.0.get(edge as usize).cloned().ok_or_else(|| bad(format!("no edge {edge}")));
        }
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.edge_curve(shape, edge),
            Data::Exact(s) => s,
        };
        let (_, e) = edge_at(s, edge)?;
        let closed = s.model.edge(e.id).is_ok_and(|x| x.is_closed());
        Ok(match curve_of(&s.model, e) {
            Some((c @ Curve::Line { .. }, lo, hi)) => EdgeCurve::Line { a: dp(c.point(lo)), b: dp(c.point(hi)) },
            Some((Curve::Circle { frame, radius }, _, _)) if closed => {
                EdgeCurve::Circle { center: dp(frame.origin()), normal: dv(&frame.z().into_inner()), radius }
            }
            Some((c @ Curve::Circle { .. }, lo, hi)) => {
                EdgeCurve::Arc { a: dp(c.point(lo)), mid: dp(c.point((lo + hi) / 2.0)), b: dp(c.point(hi)) }
            }
            _ => EdgeCurve::Other { points: edge_points(&s.model, e, Some(64)) },
        })
    }

    fn face_edges(&self, shape: &Shape, face: u32) -> Result<Vec<u32>> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.face_edges(shape, face),
            Data::Exact(s) => s,
        };
        let (_, f) = face_at(s, face)?;
        let index: HashMap<arris::topo::EdgeId, u32> = edges_of(s).iter().enumerate().map(|(i, (_, e))| (e.id, i as u32)).collect();
        let entity = s.model.face(f.id).map_err(|e| bad(e.to_string()))?;
        let mut out = Vec::new();
        for c in entity.loops().iter().flat_map(|l| l.coedges()) {
            if let Some(i) = index.get(&c.edge())
                && !out.contains(i)
            {
                out.push(*i);
            }
        }
        Ok(out)
    }

    fn face(&self, shape: &Shape, face: u32) -> Result<Shape> {
        let s = match data(shape)? {
            Data::Mesh => return PolyKernel.face(shape, face),
            Data::Exact(s) => s,
        };
        let (b, f) = face_at(s, face)?;
        let profile = profile_of(&s.model, f).ok();
        Ok(wrap(ArShape { profile, ..ArShape::new(s.model.clone(), vec![b], Some(f)) }))
    }

    fn imprint(&self, shape: &Shape, face: u32, tools: &[(Shape, Option<u32>)]) -> Result<Shape> {
        if tools.is_empty() {
            return Err(bad("nothing to project"));
        }
        surgery::imprint(exact(shape)?, face, tools)
    }

    fn wrap_emboss(
        &self,
        body: &Shape,
        face: Option<u32>,
        regions: &[Shape],
        frame: PlaneFrame,
        depth: f64,
        rotation: f64,
        center: Option<DVec3>,
    ) -> Result<Shape> {
        wrap::wrap_emboss(body, face, regions, frame, depth, rotation, center)
    }

    fn loft(&self, sections: &[Shape]) -> Result<Shape> {
        self.loft_ruled(sections, false)
    }

    fn loft_ruled(&self, sections: &[Shape], ruled: bool) -> Result<Shape> {
        let profiles = profile::aligned(&sections.iter().map(profile_of_shape).collect::<Result<Vec<_>>>()?);
        let kind = if ruled { arris::ops::LoftKind::Ruled } else { arris::ops::LoftKind::Smooth };
        solid_from("loft", |m| arris::ops::loft(m, &profiles, kind, false, C))
    }

    fn sweep(&self, profile: &Shape, _frame: PlaneFrame, path: &[PlanarCurve]) -> Result<Shape> {
        let p = profile_of_shape(profile)?;
        let path = path_of(path)?;
        solid_from("sweep", |m| arris::ops::sweep(m, &p, &path, FrameLaw::RotationMinimizing, C))
    }

    fn write_step(&self, shapes: &[Shape], path: &Path) -> Result<()> {
        let parts: Vec<(Shape, PartInfo)> = shapes.iter().map(|s| (s.clone(), PartInfo::default())).collect();
        step::write_parts(&parts, path, 214)
    }

    fn write_step_ap(&self, shapes: &[Shape], path: &Path, protocol: u32) -> Result<()> {
        let parts: Vec<(Shape, PartInfo)> = shapes.iter().map(|s| (s.clone(), PartInfo::default())).collect();
        step::write_parts(&parts, path, protocol)
    }

    fn write_step_parts(&self, parts: &[(Shape, PartInfo)], path: &Path, protocol: u32) -> Result<()> {
        step::write_parts(parts, path, protocol)
    }

    fn read_step(&self, path: &Path) -> Result<Vec<Shape>> {
        Ok(step::read_parts(path)?.into_iter().map(|(s, _)| s).collect())
    }

    fn read_step_parts(&self, path: &Path) -> Result<Vec<(Shape, PartInfo)>> {
        step::read_parts(path)
    }

    /// B-rep solids (type 186), as Open CASCADE writes them; mesh bodies go as flat-faced solids.
    fn write_iges(&self, shapes: &[Shape], path: &Path) -> Result<()> {
        let mut breps = Vec::new();
        for shape in shapes {
            let s = exact_or_faceted(shape)?;
            for b in &s.bodies {
                let solid = s.model.body(b.id).is_ok_and(|x| x.kind() == arris::topo::entity::BodyKind::Solid);
                breps.push(brep_io::body(&s.model, *b, solid)?);
            }
        }
        if breps.is_empty() {
            return Err(bad("nothing to write"));
        }
        let text = iges::write(&breps)?;
        std::fs::write(path, text).map_err(|e| KernelError::Operation { op: "write IGES", message: e.to_string() })
    }

    /// B-rep solids and shells; files of loose trimmed surfaces say they need Open CASCADE.
    fn read_iges(&self, path: &Path) -> Result<Vec<Shape>> {
        let bytes = std::fs::read(path).map_err(|e| KernelError::Operation { op: "read IGES", message: e.to_string() })?;
        if bytes.len() > 512 * 1024 * 1024 {
            return Err(bad("the IGES file is too big"));
        }
        let text = String::from_utf8_lossy(&bytes);
        iges::read(&text)?.iter().map(|b| self.from_brep(b)).collect()
    }

    fn empty(&self) -> Result<Shape> {
        Ok(wrap(ArShape::new(Arc::new(Model::default()), Vec::new(), None)))
    }

    fn shape_from_mesh(&self, positions: &[DVec3], indices: &[u32]) -> Result<Shape> {
        PolyKernel.shape_from_mesh(positions, indices)
    }

    fn from_brep(&self, brep: &shapecraft_kernel::brep::Brep) -> Result<Shape> {
        Ok(self.from_brep_faces(brep)?.0)
    }

    /// Through STEP text: Arris's reader fits the pcurves, checks the body and heals small gaps, the same road a
    /// STEP import takes. Faces map back through its provenance (each face generated from its `ADVANCED_FACE`).
    fn from_brep_faces(&self, brep: &shapecraft_kernel::brep::Brep) -> Result<(Shape, Vec<Option<u32>>)> {
        let written = brep_step::write(brep)?;
        let mut m = Model::default();
        let read = arris::io::step::read(&mut m, &written.text, &Default::default(), C)
            .map_err(|e| KernelError::Operation { op: "from_brep", message: e.to_string() })?;
        let solid = read.solids.into_iter().next().ok_or_else(|| bad("the body has no faces"))?;
        let body = solid.result.map_err(|e| KernelError::Operation { op: "from_brep", message: e.to_string() })?;
        let shape = ArShape::new(Arc::new(m), vec![body.body], None);
        let index: HashMap<arris::topo::FaceId, u32> = faces_of(&shape).iter().enumerate().map(|(i, (_, f))| (f.id, i as u32)).collect();
        let faces = written
            .faces
            .iter()
            .map(|id| {
                let role = arris::topo::Role::File(arris::topo::provenance::FileEntity { id: *id, instance: 0 });
                body.provenance.generated_from(role).iter().find_map(|s| match s.id {
                    arris::topo::EntityId::Face(f) => index.get(&f).copied(),
                    _ => None,
                })
            })
            .collect();
        Ok((wrap(shape), faces))
    }

    fn to_brep(&self, shape: &Shape) -> Result<Vec<shapecraft_kernel::brep::Brep>> {
        let s = exact(shape)?;
        s.bodies.iter().map(|b| brep_io::body(&s.model, *b, arris::ops::measure::mass_properties(&s.model, *b, C).is_ok())).collect()
    }

    fn shape_to_text(&self, shape: &Shape) -> Result<String> {
        let s = exact(shape)?;
        let parts = s
            .bodies
            .iter()
            .map(|b| {
                arris::io::body::to_json(&s.model, *b, &Default::default())
                    .map_err(|e| KernelError::Operation { op: "save", message: e.to_string() })
            })
            .collect::<Result<Vec<String>>>()?;
        Ok(format!("arris-bodies\n{}", parts.join("\n")))
    }

    fn shape_from_text(&self, text: &str) -> Result<Shape> {
        let rest = text.strip_prefix("arris-bodies\n").ok_or_else(|| bad("not a body this kernel saved"))?;
        let mut m = Model::default();
        let mut bodies = Vec::new();
        for line in rest.lines().filter(|l| !l.is_empty()) {
            bodies.push(
                arris::io::body::from_json(&mut m, line, C)
                    .map_err(|e| KernelError::Operation { op: "open", message: e.to_string() })?
                    .body,
            );
        }
        Ok(wrap(pack(&m, &bodies)?))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

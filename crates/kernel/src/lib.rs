//! The solid-modeling interface the rest of ShapeCraft talks to.
//!
//! No kernel types leak through here: a [`Shape`] is an immutable, cheaply cloned handle that only the kernel
//! that made it can look inside. That keeps undo cheap (snapshots share shapes) and keeps the kernel swappable.

#![forbid(unsafe_code)]

use std::any::Any;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

pub use glam::{DMat3, DVec2, DVec3};

pub mod brep;
mod mesh;
pub mod spline;
pub use mesh::Mesh;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("{}", operation_text(op, message))]
    Operation { op: &'static str, message: String },
    #[error("shape was made by a different kernel")]
    ForeignShape,
    #[error("i/o error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, KernelError>;

/// "offset failed: …", unless the message already says what failed ("offset failed for this distance"), so a
/// toast never reads "offset failed: offset failed for this distance".
fn operation_text(op: &str, message: &str) -> String {
    if message.to_ascii_lowercase().contains("failed") { message.to_string() } else { format!("{op} failed: {message}") }
}

/// Kernel-private shape data. Implemented by each kernel crate.
pub trait ShapeData: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
}

/// An immutable B-rep shape. Cloning shares it.
#[derive(Clone)]
pub struct Shape(Arc<dyn ShapeData>);

impl Shape {
    pub fn new(data: impl ShapeData) -> Self {
        Self(Arc::new(data))
    }

    pub fn downcast<T: ShapeData>(&self) -> Result<&T> {
        self.0.as_any().downcast_ref::<T>().ok_or(KernelError::ForeignShape)
    }

    pub fn ptr_eq(&self, other: &Shape) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shape({:p})", Arc::as_ptr(&self.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MassProps {
    pub volume: f64,
    pub area: f64,
    pub centroid: DVec3,
}

/// A planar face: its centre and outward normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePlane {
    pub origin: DVec3,
    pub normal: DVec3,
}

/// A sketch plane for region finding: origin plus in-plane x and y axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneFrame {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
}

/// A curve lying in a [`PlaneFrame`], in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanarCurve {
    Line {
        a: DVec3,
        b: DVec3,
    },
    Circle {
        center: DVec3,
        radius: f64,
    },
    /// Through `a`, `mid` and `b`, in that order.
    Arc {
        a: DVec3,
        mid: DVec3,
        b: DVec3,
    },
    /// `major` is the unit direction of the long axis; `rx >= ry`.
    Ellipse {
        center: DVec3,
        major: DVec3,
        rx: f64,
        ry: f64,
    },
    /// A smooth curve through the points, in order.
    Spline {
        points: Vec<DVec3>,
    },
    /// A clamped B-spline (cubic, or lower with fewer poles) pulled toward its control points; it starts and
    /// ends on the first and last.
    BSpline {
        poles: Vec<DVec3>,
    },
}

/// What a STEP file says about one of its parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartInfo {
    pub name: Option<String>,
    /// sRGB 0..1.
    pub color: Option<[f32; 3]>,
    /// The assemblies it sits in, outermost first.
    pub path: Vec<String>,
}

/// The exact shape of an edge, in world space.
#[derive(Clone, Debug, PartialEq)]
pub enum EdgeCurve {
    Line {
        a: DVec3,
        b: DVec3,
    },
    Circle {
        center: DVec3,
        normal: DVec3,
        radius: f64,
    },
    /// From `a` through `mid` to `b`.
    Arc {
        a: DVec3,
        mid: DVec3,
        b: DVec3,
    },
    /// Any other curve, sampled from start to end.
    Other {
        points: Vec<DVec3>,
    },
}

/// A drawing view: the view plane through `origin`, looked at from `toward` (pointing at the viewer), with
/// `x` to the right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewFrame {
    pub origin: DVec3,
    pub toward: DVec3,
    pub x: DVec3,
}

/// A projected edge in a drawing view, in view-plane millimeters.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewLine {
    pub points: Vec<DVec2>,
    /// Behind something (drawn dashed).
    pub hidden: bool,
}

/// Which part of a shape a query is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubShape {
    Whole,
    Face(u32),
    Edge(u32),
}

/// The shortest gap between two shapes and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distance {
    pub distance: f64,
    pub a: DVec3,
    pub b: DVec3,
}

/// A line in space: a point and a direction (not necessarily unit length).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub origin: DVec3,
    pub dir: DVec3,
}

impl Axis {
    pub fn new(origin: DVec3, dir: DVec3) -> Self {
        Self { origin, dir }
    }

    pub fn flat(&self) -> [f64; 6] {
        [self.origin.x, self.origin.y, self.origin.z, self.dir.x, self.dir.y, self.dir.z]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    /// Whether the two boxes share any space (touching counts).
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }
}

/// Sweep's settings, as the reference's Sweep card has them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepOptions {
    /// Turn of the profile from the start to the end of the path, degrees.
    pub twist: f64,
    /// The profile's size at the end of the path, as a factor of its size at the start.
    pub scale: f64,
    /// Parallel to Profile: the profile keeps its own orientation instead of turning to stay square to the path.
    pub parallel: bool,
    /// Round corners where the path turns sharply (else mitred).
    pub round: bool,
}

impl Default for SweepOptions {
    fn default() -> Self {
        Self { twist: 0.0, scale: 1.0, parallel: false, round: false }
    }
}

pub trait Kernel: Send + Sync {
    fn name(&self) -> &'static str;

    fn make_box(&self, min: DVec3, size: DVec3) -> Result<Shape>;

    /// Extrudes a closed planar polygon (points in order, not repeated) along `dir`.
    fn extrude_polygon(&self, points: &[DVec3], dir: DVec3) -> Result<Shape>;

    /// Extrudes a circle in the plane through `center` with normal `dir`, by `dir`'s length.
    fn extrude_circle(&self, center: DVec3, radius: f64, dir: DVec3) -> Result<Shape>;

    fn boolean(&self, op: BoolOp, a: &Shape, b: &Shape) -> Result<Shape>;

    /// Rounds every edge of the shape. Per-edge selection comes with topology naming.
    fn fillet_all_edges(&self, shape: &Shape, radius: f64) -> Result<Shape>;

    fn mass_props(&self, shape: &Shape) -> Result<MassProps>;

    fn bounds(&self, shape: &Shape) -> Result<Aabb>;

    /// Triangulates for display. `deflection` is the maximum chord error in model units.
    fn tessellate(&self, shape: &Shape, deflection: f64) -> Result<Mesh>;

    fn face_count(&self, shape: &Shape) -> Result<u32>;

    /// The plane of a planar face, or `None` if the face is curved.
    fn face_plane(&self, shape: &Shape, face: u32) -> Result<Option<FacePlane>>;

    /// Push/pull: moves a planar face along its normal, adding material (positive) or removing it.
    fn extrude_face(&self, shape: &Shape, face: u32, distance: f64) -> Result<Shape>;

    /// Rounds the given edges (indices as in [`Mesh::edge_ids`]).
    fn fillet_edges(&self, shape: &Shape, edges: &[u32], radius: f64) -> Result<Shape>;

    /// Rounds the given edges with a radius running from `r1` at each edge's start to `r2` at its end.
    fn fillet_edges_variable(&self, shape: &Shape, edges: &[u32], r1: f64, r2: f64) -> Result<Shape>;

    fn translate(&self, shape: &Shape, by: DVec3) -> Result<Shape>;

    /// The closed cells a set of curves in one plane encloses, as faces (holes included).
    fn planar_regions(&self, frame: PlaneFrame, curves: &[PlanarCurve]) -> Result<Vec<Shape>>;

    /// Area of a face, length of an edge, or volume of the whole shape, with its center.
    fn measure(&self, shape: &Shape, sub: SubShape) -> Result<(f64, DVec3)>;

    fn distance(&self, a: &Shape, sa: SubShape, b: &Shape, sb: SubShape) -> Result<Distance>;

    /// Extrudes a planar face along `dir`.
    fn prism(&self, face: &Shape, dir: DVec3) -> Result<Shape>;

    /// Extrudes a planar face along `dir` (straight out of it, either side), the walls leaning in by `angle`
    /// radians (out when negative).
    fn prism_draft(&self, face: &Shape, dir: DVec3, angle: f64) -> Result<Shape>;

    /// Spins a planar face around an axis (radians).
    fn revolve(&self, face: &Shape, axis: Axis, angle: f64) -> Result<Shape>;

    /// Spins a planar face around an axis (radians) while it rises `height` along it: a helical body (coil, spring,
    /// thread).
    fn revolve_helix(&self, face: &Shape, axis: Axis, angle: f64, height: f64) -> Result<Shape> {
        let _ = (face, axis, angle, height);
        Err(KernelError::InvalidInput("this kernel can't make helical revolves".into()))
    }

    /// Hollows a solid to `thickness` (inwards), leaving `open_faces` open.
    fn shell(&self, shape: &Shape, open_faces: &[u32], thickness: f64) -> Result<Shape>;

    /// Bevels the given edges by an equal distance.
    fn chamfer_edges(&self, shape: &Shape, edges: &[u32], distance: f64) -> Result<Shape>;

    /// Spins a closed planar polygon around an axis (radians).
    fn revolve_polygon(&self, points: &[DVec3], axis: Axis, angle: f64) -> Result<Shape>;

    /// Spins a circle (center, plane normal, radius) around an axis (radians).
    fn revolve_circle(&self, center: DVec3, normal: DVec3, radius: f64, axis: Axis, angle: f64) -> Result<Shape>;

    fn rotate(&self, shape: &Shape, axis: Axis, angle: f64) -> Result<Shape>;

    /// A mirrored copy across the plane through `origin` with `normal`.
    fn mirror(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Shape>;

    /// Scales around `center` by a factor per world axis (all equal for a uniform scale).
    fn scale(&self, shape: &Shape, center: DVec3, factors: DVec3) -> Result<Shape>;

    /// Moves, turns or scales one closed edge (a rim round a flat face) by x' = linear·x + offset: the face it bounds
    /// goes with it and the face on its other side runs straight from its far edge to the new rim (a cylinder's rim
    /// scaled up makes a taper, moved up makes it taller).
    fn map_edge(&self, shape: &Shape, edge: u32, linear: glam::DMat3, offset: DVec3) -> Result<Shape> {
        let _ = (shape, edge, linear, offset);
        Err(KernelError::InvalidInput("this kernel can't move an edge on its own".into()))
    }

    /// `map_edge` as a scale about `center` by a factor per world axis.
    fn scale_edge(&self, shape: &Shape, edge: u32, center: DVec3, factors: DVec3) -> Result<Shape> {
        if !factors.is_finite() || factors.min_element() < 1e-6 || factors.max_element() > 1e6 || !center.is_finite() {
            return Err(KernelError::InvalidInput("scale factors must be positive".into()));
        }
        self.map_edge(shape, edge, glam::DMat3::from_diagonal(factors), center - center * factors)
    }

    /// Moves faces along their normals (planar or curved); neighbouring faces stretch to follow.
    fn offset_faces(&self, shape: &Shape, faces: &[u32], distance: f64) -> Result<Shape>;

    /// A fillet sized by `size` as the radius, or (`chordal`) as the chord width between where it meets the two faces;
    /// `setback` > 0 gives Setback corners (cut back that far from where three rounded edges meet and closed with
    /// one smooth patch) instead of rolling-ball ones.
    fn fillet_edges_ex(&self, shape: &Shape, edges: &[u32], size: f64, chordal: bool, setback: f64) -> Result<Shape> {
        let _ = (shape, edges, size, chordal, setback);
        Err(KernelError::InvalidInput("this kernel has only rolling-ball radius fillets".into()))
    }

    /// Wrap & Emboss: lays flat sketch regions (faces on `frame`'s plane) onto the body's cylindrical or conical face
    /// with their lengths kept, and raises them by `depth` (negative engraves). `face` `None` takes the round face
    /// nearest the regions; `rotation` (radians) turns them about their centre; `center` is the point on the face
    /// their centre goes to.
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
        let _ = (body, face, regions, frame, depth, rotation, center);
        Err(KernelError::InvalidInput("this kernel can't wrap sketches onto faces".into()))
    }

    /// Takes faces out and closes the body over them (a hole's wall, a fillet, a boss's faces); an error when the
    /// neighbours can't be extended to close it.
    fn remove_faces(&self, shape: &Shape, faces: &[u32]) -> Result<Shape> {
        let _ = (shape, faces);
        Err(KernelError::InvalidInput("this kernel can't remove faces".into()))
    }

    /// Cuts a solid in two (or more) with the plane through `origin` with `normal`.
    fn split_by_plane(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Vec<Shape>>;

    /// The separate solids a shape is made of (one for an ordinary body).
    fn solids(&self, shape: &Shape) -> Result<Vec<Shape>>;

    /// The natural axis of an edge or face: a straight edge's line, a round edge's or cylinder's axis, a flat
    /// face's normal through its center. `None` if it has none.
    fn axis_of(&self, shape: &Shape, sub: SubShape) -> Result<Option<Axis>>;

    /// A round blend (a fillet): a cylinder or torus face that meets two neighbours smoothly. Its radius, and
    /// whether it's convex (a rounded edge) rather than concave (a filled corner). `None` for any other face.
    fn blend_of(&self, shape: &Shape, face: u32) -> Result<Option<(f64, bool)>>;

    fn edge_count(&self, shape: &Shape) -> Result<u32>;

    /// The visible and hidden edges of shapes seen in a drawing view (exact hidden-line removal).
    fn hidden_lines(&self, shapes: &[Shape], view: ViewFrame, deflection: f64) -> Result<Vec<ViewLine>>;

    /// A 2D offset of a connected chain of curves in `frame` (sharp corners); open chains stay open. The
    /// result's edges are read back with [`Kernel::edge_curve`].
    fn offset_curves(&self, frame: PlaneFrame, curves: &[PlanarCurve], distance: f64) -> Result<Shape>;

    /// A 2D offset of a planar face's boundaries, outwards when positive (sharp corners).
    fn offset_face_outline(&self, face: &Shape, distance: f64) -> Result<Shape>;

    /// An edge's exact curve (straight, round) or sample points along it.
    fn edge_curve(&self, shape: &Shape, edge: u32) -> Result<EdgeCurve>;

    /// The edges around a face (indices as in [`Mesh::edge_ids`]).
    fn face_edges(&self, shape: &Shape, face: u32) -> Result<Vec<u32>>;

    /// One face of a shape as its own shape (a profile for loft or sweep).
    fn face(&self, shape: &Shape, face: u32) -> Result<Shape>;

    /// A solid through the outer boundaries of planar faces, in order.
    /// Projects the tools' edges (all of a tool's, or the one numbered) onto `face` along its normal and imprints
    /// them there: the face splits, the volume stays.
    fn imprint(&self, shape: &Shape, face: u32, tools: &[(Shape, Option<u32>)]) -> Result<Shape>;

    fn loft(&self, sections: &[Shape]) -> Result<Shape>;

    /// Fill: a smooth face through a closed loop of the shape's edges, sewn on (a solid when that closes it).
    fn fill(&self, shape: &Shape, edges: &[u32]) -> Result<Shape> {
        let _ = (shape, edges);
        Err(KernelError::InvalidInput("this kernel can't fill openings".into()))
    }

    /// A loft whose walls run straight from profile to profile (Ruled), or smooth.
    fn loft_ruled(&self, sections: &[Shape], ruled: bool) -> Result<Shape> {
        if ruled { Err(KernelError::InvalidInput("this kernel has no ruled lofts".into())) } else { self.loft(sections) }
    }

    /// A chamfer of two distances: `d1` along `on_face` for the edges that border it (else the edge's first face),
    /// `d2` along the other.
    fn chamfer_edges_two(&self, shape: &Shape, edges: &[u32], d1: f64, d2: f64, _on_face: Option<u32>) -> Result<Shape> {
        if (d1 - d2).abs() < 1e-12 {
            self.chamfer_edges(shape, edges, d1)
        } else {
            Err(KernelError::InvalidInput("this kernel has no two-distance chamfers".into()))
        }
    }

    /// Sweeps a planar face along a connected chain of curves lying in `frame`.
    fn sweep(&self, profile: &Shape, frame: PlaneFrame, path: &[PlanarCurve]) -> Result<Shape>;

    /// [`Kernel::sweep`] with Sweep's settings: twist, scale, orientation and corners.
    fn sweep_with(&self, profile: &Shape, frame: PlaneFrame, path: &[PlanarCurve], opts: &SweepOptions) -> Result<Shape> {
        if *opts == SweepOptions::default() {
            self.sweep(profile, frame, path)
        } else {
            Err(KernelError::InvalidInput("this kernel has no sweep settings".into()))
        }
    }

    fn write_step(&self, shapes: &[Shape], path: &Path) -> Result<()>;

    /// STEP in a given application protocol: 203, 214 or 242.
    fn write_step_ap(&self, shapes: &[Shape], path: &Path, protocol: u32) -> Result<()>;

    fn read_step(&self, path: &Path) -> Result<Vec<Shape>>;
    /// Reads back what [`Kernel::write_step`] wrote, one shape per shape written (a body of several solids, or a
    /// surface, stays one). Kernels without it read the file as an import does.
    fn read_step_roots(&self, path: &Path) -> Result<Vec<Shape>> {
        self.read_step(path)
    }

    /// Cleans up a shape read from a file, as the import options ask. Kernels without it leave it as it is.
    fn heal(&self, shape: &Shape, opts: HealOptions) -> Result<Shape> {
        let _ = opts;
        Ok(shape.clone())
    }

    /// STEP with its product structure: each solid placed, with the name, color and assembly path it had in the
    /// file. Kernels without that fall back to the bare shapes.
    fn read_step_parts(&self, path: &Path) -> Result<Vec<(Shape, PartInfo)>> {
        Ok(self.read_step(path)?.into_iter().map(|s| (s, PartInfo::default())).collect())
    }

    /// STEP with each shape a named (and colored) part.
    fn write_step_parts(&self, parts: &[(Shape, PartInfo)], path: &Path, protocol: u32) -> Result<()> {
        let shapes: Vec<Shape> = parts.iter().map(|(s, _)| s.clone()).collect();
        self.write_step_ap(&shapes, path, protocol)
    }

    fn write_iges(&self, shapes: &[Shape], path: &Path) -> Result<()>;

    fn read_iges(&self, path: &Path) -> Result<Vec<Shape>>;

    /// A body from welded triangles (3 indices each): a solid when the mesh is closed, else an open shell.
    fn shape_from_mesh(&self, positions: &[DVec3], indices: &[u32]) -> Result<Shape>;

    /// Builds a shape from an exact boundary representation (Parasolid XT import).
    #[allow(clippy::wrong_self_convention)] // the kernel instance builds it
    fn from_brep(&self, _brep: &brep::Brep) -> Result<Shape> {
        Err(KernelError::Operation { op: "from_brep", message: "not supported by this kernel".into() })
    }

    /// The shape as text the same kernel reads back exactly ([`Kernel::shape_from_text`]): frozen geometry kept in
    /// the history (an Unlinked Copy).
    fn shape_to_text(&self, _shape: &Shape) -> Result<String> {
        Err(KernelError::Operation { op: "shape_to_text", message: "not supported by this kernel".into() })
    }

    /// Reads [`Kernel::shape_to_text`]'s text back (hostile input: errors, never panics).
    fn shape_from_text(&self, _text: &str) -> Result<Shape> {
        Err(KernelError::Operation { op: "shape_from_text", message: "not supported by this kernel".into() })
    }

    /// Describes a shape's solids (or shells) as exact boundary representations (Parasolid XT export).
    fn to_brep(&self, _shape: &Shape) -> Result<Vec<brep::Brep>> {
        Err(KernelError::Operation { op: "to_brep", message: "not supported by this kernel".into() })
    }
}

/// What to clean up on a shape read from a file (Import Preferences ▸ Custom). All off = take it as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HealOptions {
    /// Fix gaps, small edges and wrongly oriented pieces.
    pub repair: bool,
    /// Rebuild edge curves from their faces and make tolerances agree.
    pub edges: bool,
    /// Join touching faces into shells, closed ones into solids.
    pub sew: bool,
    /// Merge faces and edges lying on one surface or curve (fewer, larger faces).
    pub simplify: bool,
}

impl HealOptions {
    /// Everything on: Import Preferences ▸ Quality.
    pub const ALL: Self = Self { repair: true, edges: true, sew: true, simplify: true };
}

#[cfg(test)]
mod error_text_tests {
    use super::KernelError;

    #[test]
    fn a_failure_says_what_failed_once() {
        let doubled = KernelError::Operation { op: "offset", message: "offset failed for this distance".into() };
        assert_eq!(doubled.to_string(), "offset failed for this distance");
        let plain = KernelError::Operation { op: "fillet", message: "radius too large".into() };
        assert_eq!(plain.to_string(), "fillet failed: radius too large");
    }
}

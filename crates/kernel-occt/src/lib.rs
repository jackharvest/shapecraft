//! OpenCASCADE implementation of [`shapecraft_kernel::Kernel`].
//!
//! All `unsafe` in ShapeCraft lives in this crate, at the FFI boundary to `cpp/shim.cpp`.

mod ffi;

use std::any::Any;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

use shapecraft_kernel::{
    Aabb, Axis, BoolOp, DVec3, Distance, EdgeCurve, FacePlane, Kernel, KernelError, MassProps, Mesh, PartInfo, PlanarCurve, PlaneFrame,
    Result, Shape, ShapeData, SubShape, SweepOptions, ViewFrame, ViewLine,
};

struct OcctShape(*mut ffi::SkShape);

// SAFETY: the shim never mutates a shape after creating it, and OCCT shapes are safe to read from several
// threads once built.
unsafe impl Send for OcctShape {}
unsafe impl Sync for OcctShape {}

impl Drop for OcctShape {
    fn drop(&mut self) {
        // SAFETY: we own the pointer, created by the shim, freed once.
        unsafe { ffi::sk_shape_free(self.0) }
    }
}

impl ShapeData for OcctShape {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn last_error() -> String {
    // SAFETY: the shim returns a valid NUL-terminated thread-local string.
    unsafe { CStr::from_ptr(ffi::sk_last_error()) }.to_string_lossy().into_owned()
}

fn check(op: &'static str, rc: i32) -> Result<()> {
    if rc == 0 { Ok(()) } else { Err(KernelError::Operation { op, message: last_error() }) }
}

fn finite(op: &str, values: &[f64]) -> Result<()> {
    if values.iter().all(|v| v.is_finite()) { Ok(()) } else { Err(KernelError::InvalidInput(format!("{op}: non-finite number"))) }
}

/// OCCT's fillet and chamfer builders aren't safe to run on two threads at once (parallel tests failed now and
/// then); they take turns. The app runs commands on one thread, so this never waits there.
static BLEND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// OCCT's STEP and IGES translators share global settings (Interface_Static, the XCAF application): one file at a
/// time.
static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn file_turn() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn blend_out(op: &'static str, f: impl FnOnce(*mut *mut ffi::SkShape) -> i32) -> Result<Shape> {
    let _turn = BLEND_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    shape_out(op, f)
}

fn shape_out(op: &'static str, f: impl FnOnce(*mut *mut ffi::SkShape) -> i32) -> Result<Shape> {
    let mut out = ptr::null_mut();
    check(op, f(&mut out))?;
    if out.is_null() {
        return Err(KernelError::Operation { op, message: "no shape returned".into() });
    }
    Ok(Shape::new(OcctShape(out)))
}

fn raw(s: &Shape) -> Result<*const ffi::SkShape> {
    Ok(s.downcast::<OcctShape>()?.0)
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| KernelError::InvalidInput("path contains NUL".into()))
}

/// Copies `len` items from a shim-owned buffer.
///
/// # Safety
/// `data` must point to at least `len` readable items (or `len` must be 0).
unsafe fn copy_slice<T: Copy>(data: *const T, len: usize) -> Vec<T> {
    if len == 0 || data.is_null() {
        return Vec::new();
    }
    // SAFETY: guaranteed by the caller.
    unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
}

fn triples(v: Vec<f32>) -> Vec<[f32; 3]> {
    v.as_chunks::<3>().0.to_vec()
}

fn sub_code(s: SubShape) -> (i32, u32) {
    match s {
        SubShape::Whole => (0, 0),
        SubShape::Face(f) => (1, f),
        SubShape::Edge(e) => (2, e),
    }
}

#[derive(Default)]
pub struct OcctKernel;

impl OcctKernel {
    pub fn new() -> Self {
        Self
    }
}

impl Kernel for OcctKernel {
    fn name(&self) -> &'static str {
        "OpenCASCADE"
    }

    fn make_box(&self, min: DVec3, size: DVec3) -> Result<Shape> {
        finite("box", &[min.x, min.y, min.z, size.x, size.y, size.z])?;
        if size.min_element() <= 0.0 {
            return Err(KernelError::InvalidInput("box size must be positive".into()));
        }
        // SAFETY: plain values in, out-pointer valid for the call.
        shape_out("box", |out| unsafe { ffi::sk_make_box(min.x, min.y, min.z, size.x, size.y, size.z, out) })
    }

    fn extrude_polygon(&self, points: &[DVec3], dir: DVec3) -> Result<Shape> {
        if points.len() < 3 {
            return Err(KernelError::InvalidInput("a polygon needs at least 3 points".into()));
        }
        let flat: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        finite("extrude", &flat)?;
        finite("extrude", &[dir.x, dir.y, dir.z])?;
        if dir.length_squared() == 0.0 {
            return Err(KernelError::InvalidInput("extrude distance is zero".into()));
        }
        // SAFETY: `flat` holds 3 * points.len() values and outlives the call.
        shape_out("extrude", |out| unsafe { ffi::sk_extrude_polygon(flat.as_ptr(), points.len(), dir.x, dir.y, dir.z, out) })
    }

    fn extrude_circle(&self, center: DVec3, radius: f64, dir: DVec3) -> Result<Shape> {
        finite("extrude", &[center.x, center.y, center.z, radius, dir.x, dir.y, dir.z])?;
        if radius <= 0.0 || dir.length_squared() == 0.0 {
            return Err(KernelError::InvalidInput("radius and distance must be positive".into()));
        }
        // SAFETY: plain values in, out-pointer valid for the call.
        shape_out("extrude", |out| unsafe { ffi::sk_extrude_circle(center.x, center.y, center.z, radius, dir.x, dir.y, dir.z, out) })
    }

    fn boolean(&self, op: BoolOp, a: &Shape, b: &Shape) -> Result<Shape> {
        let (a, b) = (raw(a)?, raw(b)?);
        let code = match op {
            BoolOp::Union => 0,
            BoolOp::Subtract => 1,
            BoolOp::Intersect => 2,
        };
        // SAFETY: both shapes are alive for the call (borrowed).
        shape_out("boolean", |out| unsafe { ffi::sk_boolean(code, a, b, out) })
    }

    fn fillet_all_edges(&self, shape: &Shape, radius: f64) -> Result<Shape> {
        finite("fillet", &[radius])?;
        if radius <= 0.0 {
            return Err(KernelError::InvalidInput("fillet radius must be positive".into()));
        }
        let s = raw(shape)?;
        // SAFETY: shape alive for the call.
        blend_out("fillet", |out| unsafe { ffi::sk_fillet_all_edges(s, radius, out) })
    }

    fn mass_props(&self, shape: &Shape) -> Result<MassProps> {
        let s = raw(shape)?;
        let (mut volume, mut area, mut c) = (0.0, 0.0, [0.0; 3]);
        // SAFETY: out-pointers valid; `c` has 3 slots.
        check("mass properties", unsafe { ffi::sk_mass_props(s, &mut volume, &mut area, c.as_mut_ptr()) })?;
        Ok(MassProps { volume, area, centroid: DVec3::from_array(c) })
    }

    fn bounds(&self, shape: &Shape) -> Result<Aabb> {
        let s = raw(shape)?;
        let (mut min, mut max) = ([0.0; 3], [0.0; 3]);
        // SAFETY: both arrays have 3 slots.
        check("bounds", unsafe { ffi::sk_bounds(s, min.as_mut_ptr(), max.as_mut_ptr()) })?;
        Ok(Aabb { min: DVec3::from_array(min), max: DVec3::from_array(max) })
    }

    fn tessellate(&self, shape: &Shape, deflection: f64) -> Result<Mesh> {
        finite("tessellate", &[deflection])?;
        if deflection <= 0.0 {
            return Err(KernelError::InvalidInput("deflection must be positive".into()));
        }
        let s = raw(shape)?;
        let mut m = ptr::null_mut();
        // SAFETY: shape alive; out-pointer valid.
        check("tessellate", unsafe { ffi::sk_tessellate(s, deflection, &mut m) })?;
        if m.is_null() {
            return Err(KernelError::Operation { op: "tessellate", message: "no mesh returned".into() });
        }
        // SAFETY: `m` is a live mesh; every length comes from the mesh itself; freed exactly once below.
        let mesh = unsafe {
            let np = ffi::sk_mesh_positions_len(m);
            let ni = ffi::sk_mesh_indices_len(m);
            let mut edges = Vec::new();
            let mut edge_ids = Vec::new();
            let mut edge_faces = Vec::new();
            for i in 0..ffi::sk_mesh_edge_count(m) {
                edges.push(triples(copy_slice(ffi::sk_mesh_edge(m, i), ffi::sk_mesh_edge_len(m, i))));
                edge_ids.push(ffi::sk_mesh_edge_id(m, i));
                edge_faces.push([ffi::sk_mesh_edge_face(m, i, 0), ffi::sk_mesh_edge_face(m, i, 1)]);
            }
            let mesh = Mesh {
                bounds_cache: Default::default(),
                positions: triples(copy_slice(ffi::sk_mesh_positions(m), np)),
                normals: triples(copy_slice(ffi::sk_mesh_normals(m), np)),
                indices: copy_slice(ffi::sk_mesh_indices(m), ni),
                triangle_faces: copy_slice(ffi::sk_mesh_tri_faces(m), ni / 3),
                edges,
                edge_ids,
                edge_faces,
            };
            ffi::sk_mesh_free(m);
            mesh
        };
        Ok(mesh)
    }

    fn face_count(&self, shape: &Shape) -> Result<u32> {
        let s = raw(shape)?;
        let mut n = 0;
        // SAFETY: shape alive; out-pointer valid.
        check("face count", unsafe { ffi::sk_face_count(s, &mut n) })?;
        Ok(n)
    }

    fn face_plane(&self, shape: &Shape, face: u32) -> Result<Option<FacePlane>> {
        let s = raw(shape)?;
        let (mut o, mut n) = ([0.0; 3], [0.0; 3]);
        // SAFETY: both arrays have 3 slots.
        let rc = unsafe { ffi::sk_face_plane(s, face, o.as_mut_ptr(), n.as_mut_ptr()) };
        if rc != 0 {
            return Ok(None);
        }
        Ok(Some(FacePlane { origin: DVec3::from_array(o), normal: DVec3::from_array(n) }))
    }

    fn extrude_face(&self, shape: &Shape, face: u32, distance: f64) -> Result<Shape> {
        finite("extrude", &[distance])?;
        let s = raw(shape)?;
        // SAFETY: shape alive for the call.
        shape_out("extrude", |out| unsafe { ffi::sk_extrude_face(s, face, distance, out) })
    }

    fn fillet_edges(&self, shape: &Shape, edges: &[u32], radius: f64) -> Result<Shape> {
        finite("fillet", &[radius])?;
        if radius <= 0.0 || edges.is_empty() {
            return Err(KernelError::InvalidInput("fillet needs edges and a positive radius".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("fillet", |out| unsafe { ffi::sk_fillet_edges(s, edges.as_ptr(), edges.len(), radius, out) })
    }

    fn fillet_edges_variable(&self, shape: &Shape, edges: &[u32], r1: f64, r2: f64) -> Result<Shape> {
        finite("fillet", &[r1, r2])?;
        if r1 <= 0.0 || r2 <= 0.0 || edges.is_empty() {
            return Err(KernelError::InvalidInput("a variable fillet needs edges and two positive radii".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("fillet", |out| unsafe { ffi::sk_fillet_edges_var(s, edges.as_ptr(), edges.len(), r1, r2, out) })
    }

    fn translate(&self, shape: &Shape, by: DVec3) -> Result<Shape> {
        finite("move", &[by.x, by.y, by.z])?;
        let s = raw(shape)?;
        // SAFETY: shape alive for the call.
        shape_out("move", |out| unsafe { ffi::sk_translate(s, by.x, by.y, by.z, out) })
    }

    fn planar_regions(&self, frame: PlaneFrame, curves: &[PlanarCurve]) -> Result<Vec<Shape>> {
        if curves.len() > 100_000 {
            return Err(KernelError::InvalidInput("too many sketch curves".into()));
        }
        let (plane, kinds, data) = encode_curves(frame, curves)?;
        finite("sketch regions", &plane)?;
        finite("sketch regions", &data)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: buffers outlive the call and their sizes match `kinds`.
        check("sketch regions", unsafe {
            ffi::sk_planar_regions(plane.as_ptr(), kinds.as_ptr(), data.as_ptr(), kinds.len(), &mut arr, &mut n)
        })?;
        if arr.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: the shim returned `n` owned shapes in `arr`; each is taken once and the array freed.
        let faces = unsafe {
            let v = (0..n).map(|i| Shape::new(OcctShape(*arr.add(i)))).collect();
            ffi::sk_shape_array_free(arr);
            v
        };
        Ok(faces)
    }

    fn measure(&self, shape: &Shape, sub: SubShape) -> Result<(f64, DVec3)> {
        let s = raw(shape)?;
        let (kind, index) = sub_code(sub);
        let (mut v, mut c) = (0.0, [0.0; 3]);
        // SAFETY: out-pointers valid; `c` has 3 slots.
        check("measure", unsafe { ffi::sk_sub_measure(s, kind, index, &mut v, c.as_mut_ptr()) })?;
        Ok((v, DVec3::from_array(c)))
    }

    fn distance(&self, a: &Shape, sa: SubShape, b: &Shape, sb: SubShape) -> Result<Distance> {
        let (ra, rb) = (raw(a)?, raw(b)?);
        let ((ka, ia), (kb, ib)) = (sub_code(sa), sub_code(sb));
        let (mut d, mut pa, mut pb) = (0.0, [0.0; 3], [0.0; 3]);
        // SAFETY: out-pointers valid; arrays have 3 slots.
        check("measure", unsafe { ffi::sk_distance(ra, ka, ia, rb, kb, ib, &mut d, pa.as_mut_ptr(), pb.as_mut_ptr()) })?;
        Ok(Distance { distance: d, a: DVec3::from_array(pa), b: DVec3::from_array(pb) })
    }

    fn prism(&self, face: &Shape, dir: DVec3) -> Result<Shape> {
        finite("extrude", &[dir.x, dir.y, dir.z])?;
        let f = raw(face)?;
        // SAFETY: shape alive for the call.
        shape_out("extrude", |out| unsafe { ffi::sk_prism(f, dir.x, dir.y, dir.z, out) })
    }

    fn prism_draft(&self, face: &Shape, dir: DVec3, angle: f64) -> Result<Shape> {
        finite("extrude", &[dir.x, dir.y, dir.z, angle])?;
        if angle.abs() >= std::f64::consts::FRAC_PI_2 * 0.95 {
            return Err(KernelError::InvalidInput("draft angle must be under 85°".into()));
        }
        let f = raw(face)?;
        // SAFETY: shape alive for the call.
        shape_out("extrude", |out| unsafe { ffi::sk_prism_draft(f, dir.x, dir.y, dir.z, angle, out) })
    }

    fn revolve(&self, face: &Shape, axis: Axis, angle: f64) -> Result<Shape> {
        let ax = axis.flat();
        finite("revolve", &ax)?;
        finite("revolve", &[angle])?;
        let f = raw(face)?;
        // SAFETY: `ax` outlives the call.
        shape_out("revolve", |out| unsafe { ffi::sk_revolve_shape(f, ax.as_ptr(), angle, out) })
    }

    fn shell(&self, shape: &Shape, open_faces: &[u32], thickness: f64) -> Result<Shape> {
        finite("shell", &[thickness])?;
        if thickness <= 0.0 {
            return Err(KernelError::InvalidInput("wall thickness must be positive".into()));
        }
        // An inward wall can't be thicker than half the part, and it never changes the outer extent; OCCT
        // sometimes returns a valid-looking solid that breaks both.
        let before = self.bounds(shape)?;
        let size = before.max - before.min;
        if thickness * 2.0 >= size.min_element() {
            return Err(KernelError::Operation { op: "shell", message: "the wall is too thick for this part".into() });
        }
        let s = raw(shape)?;
        // SAFETY: `open_faces` outlives the call.
        let result = blend_out("shell", |out| unsafe { ffi::sk_shell(s, open_faces.as_ptr(), open_faces.len(), thickness, out) })?;
        let after = self.bounds(&result)?;
        let tol = size.max_element() * 1e-6 + 1e-6;
        if (after.min - before.min).abs().max_element() > tol || (after.max - before.max).abs().max_element() > tol {
            return Err(KernelError::Operation { op: "shell", message: "shell failed for this wall thickness".into() });
        }
        Ok(result)
    }

    fn chamfer_edges(&self, shape: &Shape, edges: &[u32], distance: f64) -> Result<Shape> {
        finite("chamfer", &[distance])?;
        if distance <= 0.0 || edges.is_empty() {
            return Err(KernelError::InvalidInput("chamfer needs edges and a positive distance".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("chamfer", |out| unsafe { ffi::sk_chamfer_edges(s, edges.as_ptr(), edges.len(), distance, out) })
    }

    fn chamfer_edges_two(&self, shape: &Shape, edges: &[u32], d1: f64, d2: f64, on_face: Option<u32>) -> Result<Shape> {
        finite("chamfer", &[d1, d2])?;
        let face = match on_face {
            Some(f) => i32::try_from(f).map_err(|_| KernelError::InvalidInput("no such face".into()))?,
            None => -1,
        };
        if d1 <= 0.0 || d2 <= 0.0 || edges.is_empty() {
            return Err(KernelError::InvalidInput("chamfer needs edges and positive distances".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("chamfer", |out| unsafe { ffi::sk_chamfer_edges2(s, edges.as_ptr(), edges.len(), d1, d2, face, out) })
    }

    fn revolve_polygon(&self, points: &[DVec3], axis: Axis, angle: f64) -> Result<Shape> {
        let flat: Vec<f64> = points.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        let ax = axis.flat();
        finite("revolve", &flat)?;
        finite("revolve", &ax)?;
        finite("revolve", &[angle])?;
        if points.len() < 3 {
            return Err(KernelError::InvalidInput("a profile needs at least 3 points".into()));
        }
        // SAFETY: buffers outlive the call; sizes match.
        shape_out("revolve", |out| unsafe { ffi::sk_revolve_polygon(flat.as_ptr(), points.len(), ax.as_ptr(), angle, out) })
    }

    fn revolve_circle(&self, center: DVec3, normal: DVec3, radius: f64, axis: Axis, angle: f64) -> Result<Shape> {
        let ax = axis.flat();
        finite("revolve", &[center.x, center.y, center.z, normal.x, normal.y, normal.z, radius, angle])?;
        finite("revolve", &ax)?;
        // SAFETY: `ax` outlives the call.
        shape_out("revolve", |out| unsafe {
            ffi::sk_revolve_circle(center.x, center.y, center.z, normal.x, normal.y, normal.z, radius, ax.as_ptr(), angle, out)
        })
    }

    fn rotate(&self, shape: &Shape, axis: Axis, angle: f64) -> Result<Shape> {
        let ax = axis.flat();
        finite("rotate", &ax)?;
        finite("rotate", &[angle])?;
        let s = raw(shape)?;
        // SAFETY: `ax` outlives the call.
        shape_out("rotate", |out| unsafe { ffi::sk_rotate(s, ax.as_ptr(), angle, out) })
    }

    fn mirror(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Shape> {
        finite("mirror", &[origin.x, origin.y, origin.z, normal.x, normal.y, normal.z])?;
        let s = raw(shape)?;
        // SAFETY: plain values in.
        shape_out("mirror", |out| unsafe { ffi::sk_mirror(s, origin.x, origin.y, origin.z, normal.x, normal.y, normal.z, out) })
    }

    fn heal(&self, shape: &Shape, opts: shapecraft_kernel::HealOptions) -> Result<Shape> {
        let flags = i32::from(opts.repair) | i32::from(opts.edges) << 1 | i32::from(opts.sew) << 2 | i32::from(opts.simplify) << 3;
        if flags == 0 {
            return Ok(shape.clone());
        }
        let s = raw(shape)?;
        // SAFETY: plain values in.
        shape_out("heal", |out| unsafe { ffi::sk_heal(s, flags, out) })
    }

    fn scale(&self, shape: &Shape, center: DVec3, factors: DVec3) -> Result<Shape> {
        finite("scale", &[center.x, center.y, center.z, factors.x, factors.y, factors.z])?;
        if factors.min_element() <= 0.0 || factors.max_element() > 1e6 || factors.min_element() < 1e-6 {
            return Err(KernelError::InvalidInput("scale factors must be positive".into()));
        }
        let s = raw(shape)?;
        // SAFETY: plain values in.
        shape_out("scale", |out| unsafe { ffi::sk_scale(s, center.x, center.y, center.z, factors.x, factors.y, factors.z, out) })
    }

    fn offset_faces(&self, shape: &Shape, faces: &[u32], distance: f64) -> Result<Shape> {
        finite("offset", &[distance])?;
        if faces.is_empty() || distance == 0.0 {
            return Err(KernelError::InvalidInput("offset needs faces and a non-zero distance".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `faces` outlives the call.
        blend_out("offset", |out| unsafe { ffi::sk_offset_faces(s, faces.as_ptr(), faces.len(), distance, out) })
    }

    fn remove_faces(&self, shape: &Shape, faces: &[u32]) -> Result<Shape> {
        if faces.is_empty() || faces.len() > 10_000 {
            return Err(KernelError::InvalidInput("pick 1 to 10000 faces to remove".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `faces` outlives the call.
        blend_out("remove faces", |out| unsafe { ffi::sk_remove_faces(s, faces.as_ptr(), faces.len(), out) })
    }

    fn split_by_plane(&self, shape: &Shape, origin: DVec3, normal: DVec3) -> Result<Vec<Shape>> {
        finite("split", &[origin.x, origin.y, origin.z, normal.x, normal.y, normal.z])?;
        let s = raw(shape)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("split", unsafe { ffi::sk_split_plane(s, origin.x, origin.y, origin.z, normal.x, normal.y, normal.z, &mut arr, &mut n) })?;
        if arr.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: the shim returned `n` owned shapes in `arr`; each is taken once and the array freed.
        let parts = unsafe {
            let v = (0..n).map(|i| Shape::new(OcctShape(*arr.add(i)))).collect();
            ffi::sk_shape_array_free(arr);
            v
        };
        Ok(parts)
    }

    fn solids(&self, shape: &Shape) -> Result<Vec<Shape>> {
        let s = raw(shape)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("solids", unsafe { ffi::sk_solids(s, &mut arr, &mut n) })?;
        if arr.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: the shim returned `n` owned shapes in `arr`; each is taken once and the array freed.
        let parts = unsafe {
            let v = (0..n).map(|i| Shape::new(OcctShape(*arr.add(i)))).collect();
            ffi::sk_shape_array_free(arr);
            v
        };
        Ok(parts)
    }

    fn axis_of(&self, shape: &Shape, sub: SubShape) -> Result<Option<Axis>> {
        let s = raw(shape)?;
        let (kind, index) = sub_code(sub);
        let (mut out, mut found) = ([0.0; 6], 0i32);
        // SAFETY: `out` has 6 slots; out-pointers valid for the call.
        check("axis", unsafe { ffi::sk_axis_of(s, kind, index, out.as_mut_ptr(), &mut found) })?;
        Ok((found != 0).then(|| Axis::new(DVec3::new(out[0], out[1], out[2]), DVec3::new(out[3], out[4], out[5]))))
    }

    fn blend_of(&self, shape: &Shape, face: u32) -> Result<Option<(f64, bool)>> {
        let s = raw(shape)?;
        let (mut radius, mut convex, mut found) = (0.0, 0i32, 0i32);
        // SAFETY: out-pointers valid for the call.
        check("blend", unsafe { ffi::sk_blend_of(s, face, &mut radius, &mut convex, &mut found) })?;
        Ok((found != 0 && radius.is_finite()).then_some((radius, convex != 0)))
    }

    fn hidden_lines(&self, shapes: &[Shape], view: ViewFrame, deflection: f64) -> Result<Vec<ViewLine>> {
        let frame =
            [view.origin.x, view.origin.y, view.origin.z, view.toward.x, view.toward.y, view.toward.z, view.x.x, view.x.y, view.x.z];
        finite("drawing view", &frame)?;
        finite("drawing view", &[deflection])?;
        if deflection.is_nan() || deflection <= 0.0 {
            return Err(KernelError::InvalidInput("deflection must be positive".into()));
        }
        let ptrs = shapes.iter().map(raw).collect::<Result<Vec<_>>>()?;
        let mut lines = ptr::null_mut();
        // SAFETY: `ptrs` and `frame` outlive the call; `lines` is an out-pointer.
        check("drawing view", unsafe { ffi::sk_hlr(ptrs.as_ptr(), ptrs.len(), frame.as_ptr(), deflection, &mut lines) })?;
        if lines.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: the shim owns `lines` until freed; the arrays have the lengths it reports and we copy them
        // before freeing it once.
        let out = unsafe {
            let n = ffi::sk_lines_count(lines);
            let lens = std::slice::from_raw_parts(ffi::sk_lines_lens(lines), n);
            let hidden = std::slice::from_raw_parts(ffi::sk_lines_hidden(lines), n);
            let npts = ffi::sk_lines_points(lines);
            let xy = std::slice::from_raw_parts(ffi::sk_lines_xy(lines), npts * 2);
            let mut out = Vec::with_capacity(n);
            let mut at = 0usize;
            for (len, hid) in lens.iter().zip(hidden) {
                let len = *len as usize;
                let end = (at + len).min(npts);
                let points = (at..end).map(|i| shapecraft_kernel::DVec2::new(xy[2 * i], xy[2 * i + 1])).collect();
                out.push(ViewLine { points, hidden: *hid != 0 });
                at = end;
            }
            ffi::sk_lines_free(lines);
            out
        };
        Ok(out)
    }

    fn edge_count(&self, shape: &Shape) -> Result<u32> {
        let s = raw(shape)?;
        let mut n = 0u32;
        // SAFETY: out-pointer valid for the call.
        check("edges", unsafe { ffi::sk_edge_count(s, &mut n) })?;
        Ok(n)
    }

    fn offset_curves(&self, frame: PlaneFrame, curves: &[PlanarCurve], distance: f64) -> Result<Shape> {
        if curves.is_empty() || curves.len() > 10_000 {
            return Err(KernelError::InvalidInput("offset needs 1 to 10000 curves".into()));
        }
        let (plane, kinds, data) = encode_curves(frame, curves)?;
        finite("offset", &plane)?;
        finite("offset", &data)?;
        finite("offset", &[distance])?;
        // SAFETY: buffers outlive the call and their sizes match `kinds`.
        blend_out("offset", |out| unsafe {
            ffi::sk_offset_curves(plane.as_ptr(), kinds.as_ptr(), data.as_ptr(), kinds.len(), distance, out)
        })
    }

    fn offset_face_outline(&self, face: &Shape, distance: f64) -> Result<Shape> {
        finite("offset", &[distance])?;
        let f = raw(face)?;
        // SAFETY: shape alive for the call.
        blend_out("offset", |out| unsafe { ffi::sk_offset_face_outline(f, distance, out) })
    }

    fn edge_curve(&self, shape: &Shape, edge: u32) -> Result<EdgeCurve> {
        let s = raw(shape)?;
        let (mut kind, mut fixed, mut pts, mut n) = (0i32, [0.0f64; 12], ptr::null_mut(), 0usize);
        // SAFETY: `fixed` has 12 slots (the shim writes at most 9); out-pointers valid for the call.
        check("edge", unsafe { ffi::sk_edge_curve(s, edge, &mut kind, fixed.as_mut_ptr(), &mut pts, &mut n) })?;
        let v = |i: usize| DVec3::new(fixed[i], fixed[i + 1], fixed[i + 2]);
        Ok(match kind {
            0 => EdgeCurve::Line { a: v(0), b: v(3) },
            1 => EdgeCurve::Circle { center: v(0), normal: v(3), radius: fixed[6] },
            2 => EdgeCurve::Arc { a: v(0), mid: v(3), b: v(6) },
            _ => {
                if pts.is_null() {
                    return Ok(EdgeCurve::Other { points: Vec::new() });
                }
                // SAFETY: the shim returned `n` xyz triples in `pts`, copied once and freed.
                let points = unsafe {
                    let raw = std::slice::from_raw_parts(pts, n * 3);
                    let v = raw.as_chunks::<3>().0.iter().map(|c| DVec3::from_array(*c)).collect();
                    ffi::sk_f64_array_free(pts);
                    v
                };
                EdgeCurve::Other { points }
            }
        })
    }

    fn face_edges(&self, shape: &Shape, face: u32) -> Result<Vec<u32>> {
        let s = raw(shape)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("face edges", unsafe { ffi::sk_face_edges(s, face, &mut arr, &mut n) })?;
        if arr.is_null() {
            return Ok(Vec::new());
        }
        // SAFETY: the shim returned `n` ids in `arr`, which we copy once and free.
        let ids = unsafe {
            let v = std::slice::from_raw_parts(arr, n).to_vec();
            ffi::sk_u32_array_free(arr);
            v
        };
        Ok(ids)
    }

    fn face(&self, shape: &Shape, face: u32) -> Result<Shape> {
        let s = raw(shape)?;
        // SAFETY: shape alive for the call.
        shape_out("face", |out| unsafe { ffi::sk_face_shape(s, face, out) })
    }

    fn imprint(&self, shape: &Shape, face: u32, tools: &[(Shape, Option<u32>)]) -> Result<Shape> {
        if tools.is_empty() || tools.len() > 10_000 {
            return Err(KernelError::InvalidInput("project 1 to 10000 items".into()));
        }
        let s = raw(shape)?;
        let ptrs = tools.iter().map(|(t, _)| raw(t)).collect::<Result<Vec<_>>>()?;
        let edges: Vec<i64> = tools.iter().map(|(_, e)| e.map_or(-1, i64::from)).collect();
        // SAFETY: `ptrs` and `edges` outlive the call; the shapes are borrowed through `tools`.
        shape_out("project as edge", |out| unsafe { ffi::sk_imprint(s, face, ptrs.as_ptr(), edges.as_ptr(), ptrs.len(), out) })
    }

    fn loft(&self, sections: &[Shape]) -> Result<Shape> {
        if sections.len() < 2 || sections.len() > 64 {
            return Err(KernelError::InvalidInput("loft needs 2 to 64 profiles".into()));
        }
        let ptrs = sections.iter().map(raw).collect::<Result<Vec<_>>>()?;
        // SAFETY: `ptrs` outlives the call; the shapes are borrowed through `sections`.
        blend_out("loft", |out| unsafe { ffi::sk_loft(ptrs.as_ptr(), ptrs.len(), out) })
    }

    fn fill(&self, shape: &Shape, edges: &[u32]) -> Result<Shape> {
        if edges.is_empty() || edges.len() > 10_000 {
            return Err(KernelError::InvalidInput("fill needs the edges around an opening".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("fill", |out| unsafe { ffi::sk_fill(s, edges.as_ptr(), edges.len(), out) })
    }

    fn fillet_edges_ex(&self, shape: &Shape, edges: &[u32], size: f64, chordal: bool, setback: f64) -> Result<Shape> {
        finite("fillet", &[size, setback])?;
        if size <= 0.0 || edges.is_empty() || setback < 0.0 {
            return Err(KernelError::InvalidInput("fillet needs edges and a positive size".into()));
        }
        let s = raw(shape)?;
        // SAFETY: `edges` outlives the call.
        blend_out("fillet", |out| unsafe {
            ffi::sk_fillet_edges_ex(s, edges.as_ptr(), edges.len(), size, i32::from(chordal), setback, out)
        })
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
        if regions.is_empty() || regions.len() > 10_000 {
            return Err(KernelError::InvalidInput("wrap 1 to 10000 profiles".into()));
        }
        let plane = [frame.origin.to_array(), frame.x.to_array(), frame.y.to_array()].concat();
        let c = center.map(|c| c.to_array());
        finite("wrap", &plane)?;
        finite("wrap", &[depth, rotation])?;
        finite("wrap", c.as_ref().map_or(&[][..], |c| &c[..]))?;
        let s = raw(body)?;
        let ptrs = regions.iter().map(raw).collect::<Result<Vec<_>>>()?;
        let face = face.map_or(-1, i64::from);
        let cp = c.as_ref().map_or(ptr::null(), |c| c.as_ptr());
        // SAFETY: every buffer outlives the call; `cp` is null or points at 3 numbers.
        shape_out("wrap", |out| unsafe {
            ffi::sk_wrap_emboss(s, face, ptrs.as_ptr(), ptrs.len(), plane.as_ptr(), depth, rotation, cp, out)
        })
    }

    fn loft_ruled(&self, sections: &[Shape], ruled: bool) -> Result<Shape> {
        if sections.len() < 2 || sections.len() > 64 {
            return Err(KernelError::InvalidInput("loft needs 2 to 64 profiles".into()));
        }
        let ptrs = sections.iter().map(raw).collect::<Result<Vec<_>>>()?;
        // SAFETY: `ptrs` outlives the call; the shapes are borrowed through `sections`.
        blend_out("loft", |out| unsafe { ffi::sk_loft_ruled(ptrs.as_ptr(), ptrs.len(), i32::from(ruled), out) })
    }

    fn sweep(&self, profile: &Shape, frame: PlaneFrame, path: &[PlanarCurve]) -> Result<Shape> {
        if path.is_empty() || path.len() > 10_000 {
            return Err(KernelError::InvalidInput("the sweep path needs 1 to 10000 curves".into()));
        }
        let (plane, kinds, data) = encode_curves(frame, path)?;
        finite("sweep", &plane)?;
        finite("sweep", &data)?;
        let f = raw(profile)?;
        // SAFETY: buffers outlive the call and their sizes match `kinds`.
        blend_out("sweep", |out| unsafe { ffi::sk_sweep(f, plane.as_ptr(), kinds.as_ptr(), data.as_ptr(), kinds.len(), out) })
    }

    fn sweep_with(&self, profile: &Shape, frame: PlaneFrame, path: &[PlanarCurve], opts: &SweepOptions) -> Result<Shape> {
        if *opts == SweepOptions::default() {
            return self.sweep(profile, frame, path);
        }
        if path.is_empty() || path.len() > 10_000 {
            return Err(KernelError::InvalidInput("the sweep path needs 1 to 10000 curves".into()));
        }
        if !(opts.twist.is_finite() && opts.twist.abs() <= 36_000.0 && opts.scale.is_finite() && opts.scale > 1e-3 && opts.scale < 1e3) {
            return Err(KernelError::InvalidInput("twist must be finite and scale between 0.001 and 1000".into()));
        }
        let (plane, kinds, data) = encode_curves(frame, path)?;
        finite("sweep", &plane)?;
        finite("sweep", &data)?;
        let f = raw(profile)?;
        let (twist, scale) = (opts.twist.to_radians(), opts.scale);
        let (parallel, round) = (i32::from(opts.parallel), i32::from(opts.round));
        // SAFETY: buffers outlive the call and their sizes match `kinds`.
        blend_out("sweep", |out| unsafe {
            ffi::sk_sweep2(f, plane.as_ptr(), kinds.as_ptr(), data.as_ptr(), kinds.len(), twist, scale, parallel, round, out)
        })
    }

    fn write_step(&self, shapes: &[Shape], path: &Path) -> Result<()> {
        let _turn = file_turn();
        let ptrs = shapes.iter().map(raw).collect::<Result<Vec<_>>>()?;
        let p = c_path(path)?;
        // SAFETY: `ptrs` and `p` outlive the call; the shapes are borrowed through `shapes`.
        check("STEP export", unsafe { ffi::sk_write_step(ptrs.as_ptr(), ptrs.len(), p.as_ptr()) })
    }

    fn read_step(&self, path: &Path) -> Result<Vec<Shape>> {
        let _turn = file_turn();
        let p = c_path(path)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("STEP import", unsafe { ffi::sk_read_step(p.as_ptr(), &mut arr, &mut n) })?;
        take_shapes(arr, n)
    }

    fn read_step_roots(&self, path: &Path) -> Result<Vec<Shape>> {
        let _turn = file_turn();
        let p = c_path(path)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("STEP import", unsafe { ffi::sk_read_step_roots(p.as_ptr(), &mut arr, &mut n) })?;
        take_shapes(arr, n)
    }

    fn read_step_parts(&self, path: &Path) -> Result<Vec<(Shape, PartInfo)>> {
        let _turn = file_turn();
        let p = c_path(path)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("STEP import", unsafe { ffi::sk_read_step_doc(p.as_ptr(), &mut arr, &mut n) })?;
        // SAFETY: the shim keeps the metadata in a thread-local string until its next call on this thread.
        let meta = unsafe { std::ffi::CStr::from_ptr(ffi::sk_last_meta()) }.to_string_lossy().into_owned();
        let shapes = take_shapes(arr, n)?;
        let mut lines = meta.lines();
        Ok(shapes
            .into_iter()
            .map(|s| {
                let mut f = lines.next().unwrap_or("").split('\t');
                let name = f.next().map(str::trim).filter(|n| !n.is_empty()).map(|n| n.chars().take(200).collect());
                let color = f.next().and_then(|c| {
                    let v: Vec<f32> = c.split(' ').filter_map(|x| x.parse().ok()).collect();
                    (v.len() == 3 && v.iter().all(|x| x.is_finite()))
                        .then(|| [v[0].clamp(0.0, 1.0), v[1].clamp(0.0, 1.0), v[2].clamp(0.0, 1.0)])
                });
                let path = f
                    .next()
                    .map(|p| {
                        p.split('\x1f').map(str::trim).filter(|x| !x.is_empty()).map(|x| x.chars().take(200).collect()).take(64).collect()
                    })
                    .unwrap_or_default();
                (s, PartInfo { name, color, path })
            })
            .collect())
    }

    fn write_step_parts(&self, parts: &[(Shape, PartInfo)], path: &Path, protocol: u32) -> Result<()> {
        let _turn = file_turn();
        let schema = match protocol {
            203 => 1,
            214 => 2,
            242 => 3,
            _ => return Err(KernelError::InvalidInput(format!("unknown STEP protocol AP{protocol}"))),
        };
        let ptrs = parts.iter().map(|(s, _)| raw(s)).collect::<Result<Vec<_>>>()?;
        let names: Vec<std::ffi::CString> = parts
            .iter()
            .map(|(_, i)| std::ffi::CString::new(i.name.clone().unwrap_or_default().replace('\0', "")).unwrap_or_default())
            .collect();
        let name_ptrs: Vec<*const std::ffi::c_char> = names.iter().map(|c| c.as_ptr()).collect();
        let rgb: Vec<f64> = parts.iter().flat_map(|(_, i)| i.color.map_or([f64::NAN; 3], |c| c.map(f64::from))).collect();
        let paths: Vec<std::ffi::CString> = parts
            .iter()
            .map(|(_, i)| {
                std::ffi::CString::new(i.path.iter().map(|x| x.replace(['\0', '\x1f'], " ")).collect::<Vec<_>>().join("\x1f"))
                    .unwrap_or_default()
            })
            .collect();
        let path_ptrs: Vec<*const std::ffi::c_char> = paths.iter().map(|c| c.as_ptr()).collect();
        let p = c_path(path)?;
        // SAFETY: every array holds `parts.len()` entries (rgb three each) and outlives the call.
        check("STEP export", unsafe {
            ffi::sk_write_step_doc(ptrs.as_ptr(), name_ptrs.as_ptr(), rgb.as_ptr(), path_ptrs.as_ptr(), ptrs.len(), p.as_ptr(), schema)
        })
    }

    fn write_step_ap(&self, shapes: &[Shape], path: &Path, protocol: u32) -> Result<()> {
        let _turn = file_turn();
        let schema = match protocol {
            203 => 1,
            214 => 2,
            242 => 3,
            _ => return Err(KernelError::InvalidInput(format!("unknown STEP protocol AP{protocol}"))),
        };
        let ptrs = shapes.iter().map(raw).collect::<Result<Vec<_>>>()?;
        let p = c_path(path)?;
        // SAFETY: `ptrs` and `p` outlive the call; the shapes are borrowed through `shapes`.
        check("STEP export", unsafe { ffi::sk_write_step_schema(ptrs.as_ptr(), ptrs.len(), p.as_ptr(), schema) })
    }

    fn write_iges(&self, shapes: &[Shape], path: &Path) -> Result<()> {
        let _turn = file_turn();
        let ptrs = shapes.iter().map(raw).collect::<Result<Vec<_>>>()?;
        let p = c_path(path)?;
        // SAFETY: `ptrs` and `p` outlive the call; the shapes are borrowed through `shapes`.
        check("IGES export", unsafe { ffi::sk_write_iges(ptrs.as_ptr(), ptrs.len(), p.as_ptr()) })
    }

    fn read_iges(&self, path: &Path) -> Result<Vec<Shape>> {
        let _turn = file_turn();
        let p = c_path(path)?;
        let (mut arr, mut n) = (ptr::null_mut(), 0usize);
        // SAFETY: out-pointers valid for the call.
        check("IGES import", unsafe { ffi::sk_read_iges(p.as_ptr(), &mut arr, &mut n) })?;
        take_shapes(arr, n)
    }

    fn shape_from_mesh(&self, positions: &[DVec3], indices: &[u32]) -> Result<Shape> {
        if positions.len() > 2_000_000 || indices.len() > 6_000_000 || !indices.len().is_multiple_of(3) {
            return Err(KernelError::InvalidInput("mesh too large or malformed".into()));
        }
        if indices.iter().any(|&i| i as usize >= positions.len()) {
            return Err(KernelError::InvalidInput("mesh index out of range".into()));
        }
        let xyz: Vec<f64> = positions.iter().flat_map(|p| [p.x, p.y, p.z]).collect();
        finite("mesh import", &xyz)?;
        // SAFETY: `xyz` holds 3 doubles per position and `indices` 3 per triangle; both outlive the call.
        shape_out("mesh import", |out| unsafe {
            ffi::sk_from_mesh(xyz.as_ptr(), positions.len(), indices.as_ptr(), indices.len() / 3, out)
        })
    }

    fn from_brep(&self, brep: &shapecraft_kernel::brep::Brep) -> Result<Shape> {
        let data = shapecraft_kernel::brep::flat::encode(brep);
        // SAFETY: `data` outlives the call; the shim bounds-checks every read against `data.len()`.
        shape_out("B-rep import", |out| unsafe { ffi::sk_from_brep(data.as_ptr(), data.len(), out) })
    }

    fn shape_to_text(&self, shape: &Shape) -> Result<String> {
        let s = raw(shape)?;
        // SAFETY: `s` is a live shape; the text stays in the shim's thread-local until its next call on this thread.
        check("shape to text", unsafe { ffi::sk_write_brep_text(s) })?;
        // SAFETY: as above, read right after the call on the same thread.
        Ok(unsafe { CStr::from_ptr(ffi::sk_last_meta()) }.to_string_lossy().into_owned())
    }

    fn shape_from_text(&self, text: &str) -> Result<Shape> {
        if text.len() > 512 << 20 {
            return Err(KernelError::InvalidInput("shape text too large".into()));
        }
        // OCCT's reader complains on stdout about anything that isn't its own text.
        if !text.contains("CASCADE Topology") {
            return Err(KernelError::InvalidInput("not a shape's text".into()));
        }
        let c = CString::new(text).map_err(|_| KernelError::InvalidInput("shape text contains NUL".into()))?;
        // SAFETY: `c` outlives the call.
        shape_out("shape from text", |out| unsafe { ffi::sk_read_brep_text(c.as_ptr(), out) })
    }

    fn to_brep(&self, shape: &Shape) -> Result<Vec<shapecraft_kernel::brep::Brep>> {
        let mut out = Vec::new();
        let parts = self.solids(shape)?;
        let parts = if parts.is_empty() { vec![shape.clone()] } else { parts };
        for part in &parts {
            let s = raw(part)?;
            let mut data: *mut f64 = std::ptr::null_mut();
            let mut n = 0usize;
            // SAFETY: `s` is a live shape; on success the shim hands us `n` doubles that we copy and free.
            let flat = unsafe {
                if ffi::sk_to_brep(s, &mut data, &mut n) != 0 {
                    return Err(KernelError::Operation { op: "B-rep export", message: last_error() });
                }
                let v = std::slice::from_raw_parts(data, n).to_vec();
                ffi::sk_doubles_free(data);
                v
            };
            out.push(
                shapecraft_kernel::brep::flat::decode(&flat)
                    .ok_or_else(|| KernelError::Operation { op: "B-rep export", message: "malformed description".into() })?,
            );
        }
        Ok(out)
    }
}

/// Takes ownership of an array of `n` shapes the shim returned and frees the array.
fn take_shapes(arr: *mut *mut ffi::SkShape, n: usize) -> Result<Vec<Shape>> {
    if arr.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: the shim returned `n` owned shape pointers in `arr`; we take each one and free the array.
    let shapes = unsafe {
        let shapes = (0..n).map(|i| Shape::new(OcctShape(*arr.add(i)))).collect();
        ffi::sk_shape_array_free(arr);
        shapes
    };
    Ok(shapes)
}

#[cfg(test)]
mod tests;

/// Plane frame (9 doubles) plus the curves in the shim's encoding (see `curve_edge` in shim.cpp).
fn encode_curves(frame: PlaneFrame, curves: &[PlanarCurve]) -> Result<([f64; 9], Vec<i32>, Vec<f64>)> {
    let plane = [frame.origin.x, frame.origin.y, frame.origin.z, frame.x.x, frame.x.y, frame.x.z, frame.y.x, frame.y.y, frame.y.z];
    let mut kinds = Vec::with_capacity(curves.len());
    let mut data = Vec::new();
    for c in curves {
        match c {
            PlanarCurve::Line { a, b } => {
                kinds.push(0);
                data.extend([a.x, a.y, a.z, b.x, b.y, b.z]);
            }
            PlanarCurve::Circle { center, radius } => {
                kinds.push(1);
                data.extend([center.x, center.y, center.z, *radius]);
            }
            PlanarCurve::Arc { a, mid, b } => {
                kinds.push(2);
                data.extend([a.x, a.y, a.z, mid.x, mid.y, mid.z, b.x, b.y, b.z]);
            }
            PlanarCurve::Ellipse { center, major, rx, ry } => {
                // OCCT wants the major radius first; swap axes if needed.
                let (dir, a, b) = if rx >= ry { (*major, *rx, *ry) } else { (frame.x.cross(frame.y).cross(*major) * -1.0, *ry, *rx) };
                kinds.push(3);
                data.extend([center.x, center.y, center.z, dir.x, dir.y, dir.z, a, b]);
            }
            PlanarCurve::Spline { points } => {
                if points.len() > 1000 {
                    return Err(KernelError::InvalidInput("spline has too many points".into()));
                }
                // The reference's own fit, so the curve is the same one there.
                if let Some(f) = shapecraft_kernel::spline::fit(points) {
                    kinds.push(6);
                    data.extend([f.poles.len() as f64, f.degree as f64, f.knots.len() as f64]);
                    data.extend(f.poles.iter().flat_map(|p| [p.x, p.y, p.z]));
                    data.extend(&f.knots);
                    continue;
                }
                kinds.push(4);
                data.push(points.len() as f64);
                data.extend(points.iter().flat_map(|p| [p.x, p.y, p.z]));
            }
            PlanarCurve::BSpline { poles } => {
                if poles.len() > 1000 {
                    return Err(KernelError::InvalidInput("spline has too many control points".into()));
                }
                kinds.push(5);
                data.push(poles.len() as f64);
                data.extend(poles.iter().flat_map(|p| [p.x, p.y, p.z]));
            }
        }
    }
    Ok((plane, kinds, data))
}

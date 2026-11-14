//! Raw bindings to `cpp/shim.cpp`.

use std::ffi::c_char;

#[repr(C)]
pub struct SkShape {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkMesh {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkLines {
    _private: [u8; 0],
}

unsafe extern "C" {
    pub fn sk_last_error() -> *const c_char;
    pub fn sk_shape_free(s: *mut SkShape);
    pub fn sk_make_box(x: f64, y: f64, z: f64, dx: f64, dy: f64, dz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_extrude_polygon(xyz: *const f64, n: usize, dx: f64, dy: f64, dz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_extrude_circle(cx: f64, cy: f64, cz: f64, r: f64, dx: f64, dy: f64, dz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_boolean(op: i32, a: *const SkShape, b: *const SkShape, out: *mut *mut SkShape) -> i32;
    pub fn sk_fillet_all_edges(s: *const SkShape, radius: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_mass_props(s: *const SkShape, volume: *mut f64, area: *mut f64, centroid: *mut f64) -> i32;
    pub fn sk_bounds(s: *const SkShape, min: *mut f64, max: *mut f64) -> i32;
    pub fn sk_tessellate(s: *const SkShape, deflection: f64, out: *mut *mut SkMesh) -> i32;
    pub fn sk_mesh_free(m: *mut SkMesh);
    pub fn sk_mesh_positions_len(m: *const SkMesh) -> usize;
    pub fn sk_mesh_positions(m: *const SkMesh) -> *const f32;
    pub fn sk_mesh_normals(m: *const SkMesh) -> *const f32;
    pub fn sk_mesh_indices_len(m: *const SkMesh) -> usize;
    pub fn sk_mesh_indices(m: *const SkMesh) -> *const u32;
    pub fn sk_mesh_tri_faces(m: *const SkMesh) -> *const u32;
    pub fn sk_mesh_edge_count(m: *const SkMesh) -> usize;
    pub fn sk_mesh_edge_len(m: *const SkMesh, i: usize) -> usize;
    pub fn sk_mesh_edge(m: *const SkMesh, i: usize) -> *const f32;
    pub fn sk_write_step(shapes: *const *const SkShape, n: usize, path: *const c_char) -> i32;
    pub fn sk_read_step(path: *const c_char, out: *mut *mut *mut SkShape, n: *mut usize) -> i32;
    pub fn sk_read_step_roots(path: *const c_char, out: *mut *mut *mut SkShape, n: *mut usize) -> i32;
    pub fn sk_write_step_schema(shapes: *const *const SkShape, n: usize, path: *const c_char, schema: i32) -> i32;
    pub fn sk_read_step_doc(path: *const c_char, out: *mut *mut *mut SkShape, n: *mut usize) -> i32;
    pub fn sk_last_meta() -> *const c_char;
    pub fn sk_write_step_doc(
        shapes: *const *const SkShape,
        names: *const *const c_char,
        rgb: *const f64,
        paths: *const *const c_char,
        n: usize,
        path: *const c_char,
        schema: i32,
    ) -> i32;
    pub fn sk_write_iges(shapes: *const *const SkShape, n: usize, path: *const c_char) -> i32;
    pub fn sk_read_iges(path: *const c_char, out: *mut *mut *mut SkShape, n: *mut usize) -> i32;
    pub fn sk_from_mesh(xyz: *const f64, nverts: usize, idx: *const u32, ntris: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_shape_array_free(arr: *mut *mut SkShape);
    pub fn sk_from_brep(data: *const f64, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_to_brep(s: *const SkShape, out: *mut *mut f64, n: *mut usize) -> i32;
    pub fn sk_doubles_free(d: *mut f64);
    pub fn sk_mesh_edge_face(m: *const SkMesh, i: usize, side: usize) -> u32;
    pub fn sk_mesh_edge_id(m: *const SkMesh, i: usize) -> u32;
    pub fn sk_face_plane(s: *const SkShape, face: u32, origin: *mut f64, normal: *mut f64) -> i32;
    pub fn sk_extrude_face(s: *const SkShape, face: u32, distance: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_fillet_edges(s: *const SkShape, ids: *const u32, n: usize, radius: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_fillet_edges_var(s: *const SkShape, ids: *const u32, n: usize, r1: f64, r2: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_write_brep_text(s: *const SkShape) -> i32;
    pub fn sk_read_brep_text(text: *const c_char, out: *mut *mut SkShape) -> i32;
    pub fn sk_empty(out: *mut *mut SkShape) -> i32;
    pub fn sk_translate(s: *const SkShape, dx: f64, dy: f64, dz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_face_count(s: *const SkShape, n: *mut u32) -> i32;
    pub fn sk_planar_regions(
        plane: *const f64,
        kinds: *const i32,
        data: *const f64,
        n: usize,
        out: *mut *mut *mut SkShape,
        count: *mut usize,
    ) -> i32;
    pub fn sk_prism(face: *const SkShape, dx: f64, dy: f64, dz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_revolve_shape(face: *const SkShape, axis: *const f64, angle: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_pipe_through(face: *const SkShape, pts: *const f64, tangents: *const f64, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_sub_measure(s: *const SkShape, kind: i32, index: u32, value: *mut f64, center: *mut f64) -> i32;
    pub fn sk_distance(
        a: *const SkShape,
        ka: i32,
        ia: u32,
        b: *const SkShape,
        kb: i32,
        ib: u32,
        d: *mut f64,
        pa: *mut f64,
        pb: *mut f64,
    ) -> i32;
    pub fn sk_shell(s: *const SkShape, faces: *const u32, n: usize, thickness: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_chamfer_edges(s: *const SkShape, ids: *const u32, n: usize, distance: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_revolve_polygon(xyz: *const f64, n: usize, axis: *const f64, angle: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_revolve_circle(
        cx: f64,
        cy: f64,
        cz: f64,
        nx: f64,
        ny: f64,
        nz: f64,
        r: f64,
        axis: *const f64,
        angle: f64,
        out: *mut *mut SkShape,
    ) -> i32;
    pub fn sk_rotate(s: *const SkShape, axis: *const f64, angle: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_mirror(s: *const SkShape, ox: f64, oy: f64, oz: f64, nx: f64, ny: f64, nz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_heal(s: *const SkShape, flags: i32, budget: f64, cut: *mut i32, out: *mut *mut SkShape) -> i32;
    pub fn sk_cancellable(on: i32);
    pub fn sk_cancel(on: i32);
    pub fn sk_scale(s: *const SkShape, cx: f64, cy: f64, cz: f64, sx: f64, sy: f64, sz: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_map_edge(s: *const SkShape, edge: u32, m: *const f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_offset_faces_way(s: *const SkShape, ids: *const u32, n: usize, distance: f64, way: i32, out: *mut *mut SkShape) -> i32;
    pub fn sk_volume(s: *const SkShape, volume: *mut f64) -> i32;
    pub fn sk_face_level(s: *const SkShape, face: u32, normal: *mut f64, level: *mut f64, found: *mut i32) -> i32;
    pub fn sk_remove_faces(s: *const SkShape, ids: *const u32, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_split_plane(
        s: *const SkShape,
        ox: f64,
        oy: f64,
        oz: f64,
        nx: f64,
        ny: f64,
        nz: f64,
        out: *mut *mut *mut SkShape,
        count: *mut usize,
    ) -> i32;
    pub fn sk_solids(s: *const SkShape, out: *mut *mut *mut SkShape, count: *mut usize) -> i32;
    pub fn sk_face_shape(s: *const SkShape, face: u32, out: *mut *mut SkShape) -> i32;
    pub fn sk_imprint(
        s: *const SkShape,
        face: u32,
        tools: *const *const SkShape,
        edge: *const i64,
        n: usize,
        out: *mut *mut SkShape,
    ) -> i32;
    pub fn sk_fill(s: *const SkShape, ids: *const u32, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_loft(faces: *const *const SkShape, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_loft_ruled(faces: *const *const SkShape, n: usize, ruled: i32, out: *mut *mut SkShape) -> i32;
    pub fn sk_fillet_edges_ex(
        s: *const SkShape,
        ids: *const u32,
        n: usize,
        size: f64,
        chordal: i32,
        setback: f64,
        out: *mut *mut SkShape,
    ) -> i32;
    pub fn sk_wrap_emboss(
        body: *const SkShape,
        face: i64,
        regions: *const *const SkShape,
        n: usize,
        frame: *const f64,
        depth: f64,
        rotation: f64,
        center: *const f64,
        out: *mut *mut SkShape,
    ) -> i32;
    pub fn sk_chamfer_edges2(s: *const SkShape, ids: *const u32, n: usize, d1: f64, d2: f64, face: i32, out: *mut *mut SkShape) -> i32;
    pub fn sk_sweep(face: *const SkShape, plane: *const f64, kinds: *const i32, data: *const f64, n: usize, out: *mut *mut SkShape) -> i32;
    pub fn sk_sweep2(
        face: *const SkShape,
        plane: *const f64,
        kinds: *const i32,
        data: *const f64,
        n: usize,
        twist: f64,
        scale: f64,
        parallel: i32,
        round: i32,
        out: *mut *mut SkShape,
    ) -> i32;
    pub fn sk_axis_of(s: *const SkShape, kind: i32, index: u32, out: *mut f64, found: *mut i32) -> i32;
    pub fn sk_blend_of(s: *const SkShape, face: u32, radius: *mut f64, convex: *mut i32, found: *mut i32) -> i32;
    pub fn sk_face_edges(s: *const SkShape, face: u32, out: *mut *mut u32, n: *mut usize) -> i32;
    pub fn sk_u32_array_free(arr: *mut u32);
    pub fn sk_edge_curve(s: *const SkShape, edge: u32, kind: *mut i32, fixed: *mut f64, pts: *mut *mut f64, npts: *mut usize) -> i32;
    pub fn sk_f64_array_free(arr: *mut f64);
    pub fn sk_prism_draft(face: *const SkShape, dx: f64, dy: f64, dz: f64, angle: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_edge_count(s: *const SkShape, n: *mut u32) -> i32;
    pub fn sk_offset_curves(plane: *const f64, kinds: *const i32, data: *const f64, n: usize, distance: f64, out: *mut *mut SkShape)
    -> i32;
    pub fn sk_offset_face_outline(face: *const SkShape, distance: f64, out: *mut *mut SkShape) -> i32;
    pub fn sk_hlr(shapes: *const *const SkShape, n: usize, frame: *const f64, deflection: f64, out: *mut *mut SkLines) -> i32;
    pub fn sk_lines_free(l: *mut SkLines);
    pub fn sk_lines_count(l: *const SkLines) -> usize;
    pub fn sk_lines_lens(l: *const SkLines) -> *const u32;
    pub fn sk_lines_hidden(l: *const SkLines) -> *const u8;
    pub fn sk_lines_points(l: *const SkLines) -> usize;
    pub fn sk_lines_xy(l: *const SkLines) -> *const f64;
}

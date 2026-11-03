//! File formats written and read in Rust: mesh exports (OBJ, 3MF, GLB), mesh imports (STL, OBJ) and 2D sketch
//! imports (DXF, SVG). B-rep formats (STEP, IGES) go through the kernel.

mod mesh_in;
mod mesh_out;
mod sketch_in;
mod sketch_out;
mod zip;

pub use mesh_in::{read_obj, read_stl};
pub use mesh_out::{ExportBody, write_3mf, write_glb, write_obj, write_stl_ascii, write_usdz};
pub use sketch_in::{read_dxf, read_svg};
pub use sketch_out::write_dxf;

use std::collections::HashMap;

use glam::DVec3;

/// Most triangles a mesh import accepts (sewing every triangle into a face gets slow past this).
pub const MAX_IMPORT_TRIANGLES: usize = 200_000;

/// Merges positions that are equal to within 1e-6 of the mesh size and drops triangles that collapse.
pub fn weld(points: &[DVec3], tris: &[[u32; 3]]) -> (Vec<DVec3>, Vec<u32>) {
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in points {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let size = (hi - lo).max_element();
    let q = if size.is_finite() && size > 0.0 { size * 1e-6 } else { 1e-9 };
    let mut seen: HashMap<[i64; 3], u32> = HashMap::new();
    let mut out = Vec::new();
    let mut map = Vec::with_capacity(points.len());
    for p in points {
        let key = [(p.x / q).round() as i64, (p.y / q).round() as i64, (p.z / q).round() as i64];
        let id = *seen.entry(key).or_insert_with(|| {
            out.push(*p);
            (out.len() - 1) as u32
        });
        map.push(id);
    }
    let mut idx = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        let [a, b, c] = t.map(|i| map.get(i as usize).copied().unwrap_or(u32::MAX));
        if a == u32::MAX || b == u32::MAX || c == u32::MAX || a == b || b == c || a == c {
            continue;
        }
        idx.extend([a, b, c]);
    }
    (out, idx)
}

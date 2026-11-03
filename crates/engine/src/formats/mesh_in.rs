//! Mesh imports: STL (binary or ASCII) and Wavefront OBJ, as raw triangles for `weld`.

use glam::DVec3;

use super::MAX_IMPORT_TRIANGLES;

type Triangles = (Vec<DVec3>, Vec<[u32; 3]>);

fn too_big() -> String {
    format!("the mesh has more than {MAX_IMPORT_TRIANGLES} triangles")
}

fn finite(p: DVec3) -> Result<DVec3, String> {
    if p.is_finite() && p.abs().max_element() < 1e9 { Ok(p) } else { Err("the mesh has invalid coordinates".into()) }
}

/// Reads an STL file's triangles (binary when the size matches the binary layout, else ASCII).
pub fn read_stl(bytes: &[u8]) -> Result<Triangles, String> {
    if bytes.len() >= 84 {
        let count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        if count.checked_mul(50).and_then(|n| n.checked_add(84)) == Some(bytes.len()) {
            if count > MAX_IMPORT_TRIANGLES {
                return Err(too_big());
            }
            let mut pts = Vec::with_capacity(count * 3);
            let mut tris = Vec::with_capacity(count);
            for t in 0..count {
                let base = 84 + t * 50 + 12;
                for v in 0..3 {
                    let o = base + v * 12;
                    let f = |k: usize| {
                        let s = o + k * 4;
                        f64::from(f32::from_le_bytes([bytes[s], bytes[s + 1], bytes[s + 2], bytes[s + 3]]))
                    };
                    pts.push(finite(DVec3::new(f(0), f(1), f(2)))?);
                }
                let i = (t * 3) as u32;
                tris.push([i, i + 1, i + 2]);
            }
            return Ok((pts, tris));
        }
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not a valid STL file".to_string())?;
    let mut pts = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        if it.next() != Some("vertex") {
            continue;
        }
        let v: Vec<f64> = it.filter_map(|w| w.parse().ok()).collect();
        if v.len() != 3 {
            return Err("not a valid STL file".into());
        }
        pts.push(finite(DVec3::new(v[0], v[1], v[2]))?);
        if pts.len() > MAX_IMPORT_TRIANGLES * 3 {
            return Err(too_big());
        }
    }
    if pts.len() < 3 {
        return Err("the STL file has no triangles".into());
    }
    pts.truncate(pts.len() / 3 * 3);
    let tris = (0..pts.len() as u32 / 3).map(|t| [t * 3, t * 3 + 1, t * 3 + 2]).collect();
    Ok((pts, tris))
}

/// Reads an OBJ file's vertices and faces (polygons fanned into triangles; texture/normal indices ignored).
pub fn read_obj(text: &str) -> Result<Triangles, String> {
    let mut pts = Vec::new();
    let mut tris = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let v: Vec<f64> = it.take(3).filter_map(|w| w.parse().ok()).collect();
                if v.len() != 3 {
                    return Err("the OBJ file has a broken vertex".into());
                }
                pts.push(finite(DVec3::new(v[0], v[1], v[2]))?);
            }
            Some("f") => {
                let n = pts.len() as i64;
                let mut face = Vec::new();
                for w in it {
                    let i: i64 = w.split('/').next().and_then(|s| s.parse().ok()).ok_or("the OBJ file has a broken face")?;
                    let i = if i < 0 { n + i } else { i - 1 };
                    if i < 0 || i >= n {
                        return Err("the OBJ file has a face index out of range".into());
                    }
                    face.push(i as u32);
                }
                for k in 1..face.len().saturating_sub(1) {
                    tris.push([face[0], face[k], face[k + 1]]);
                }
                if tris.len() > MAX_IMPORT_TRIANGLES {
                    return Err(too_big());
                }
            }
            _ => {}
        }
    }
    if tris.is_empty() {
        return Err("the OBJ file has no faces".into());
    }
    Ok((pts, tris))
}

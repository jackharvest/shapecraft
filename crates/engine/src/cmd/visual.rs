//! Visualization: the decal library, decals on bodies, and the environment and camera settings.

use std::sync::Arc;

use glam::DVec3;
use serde_json::{Value, json};
use shapecraft_doc::{
    BUILTIN_ENVIRONMENTS, Decal, DecalImage, Document, Ground, ItemId, MAX_PANORAMAS, Panorama, Projection, VisualSettings,
};
use shapecraft_kernel::SubShape;

use super::{CommandSpec, always};
use crate::{EngineError, Params, Result, Session};

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams(msg.into())
}

/// Reads a picture into the project's library: a decal, or a custom material's picture.
pub(super) fn import_picture(s: &mut Session, path: &str, material: bool) -> Result<Value> {
    let bytes = std::fs::read(path).map_err(|e| bad(format!("couldn't read {path}: {e}")))?;
    if bytes.len() > 64 << 20 {
        return Err(bad("that picture is too big (64 MB at most)"));
    }
    if s.doc.decal_images.len() >= 1000 {
        return Err(bad("a project holds 1000 pictures at most"));
    }
    let (ext, pixels) = shapecraft_doc::RefImage::decode(&bytes).map_err(EngineError::BadParams)?;
    let id = s.doc.alloc_id();
    let n = s.doc.decal_images.iter().filter(|i| i.material == material && !i.sheet).count() + 1;
    let fallback = if material { format!("Material {n:02}") } else { format!("Decal {n:02}") };
    let name = std::path::Path::new(path).file_stem().map_or(fallback, |f| f.to_string_lossy().chars().take(100).collect());
    s.doc.decal_images.push(DecalImage { id, name, ext, material, sheet: false, file: Arc::new(bytes), pixels: Arc::new(pixels) });
    Ok(json!({ "image": id.0 }))
}

/// Reads a panorama into the project as a custom environment and selects it. Returns its place in the environment
/// list.
fn import_environment(s: &mut Session, path: &str) -> Result<Value> {
    let meta = std::fs::metadata(path).map_err(|e| bad(format!("couldn't read {path}: {e}")))?;
    if meta.len() > 512 << 20 {
        return Err(bad("that panorama is too big (512 MB at most)"));
    }
    if s.doc.visual.panoramas.len() >= MAX_PANORAMAS {
        return Err(bad(format!("a project holds {MAX_PANORAMAS} custom environments at most")));
    }
    let bytes = std::fs::read(path).map_err(|e| bad(format!("couldn't read {path}: {e}")))?;
    let (ext, pixels) = Panorama::decode(&bytes).map_err(EngineError::BadParams)?;
    let (w, h) = (pixels.width, pixels.height);
    let id = s.doc.alloc_id();
    let n = s.doc.visual.panoramas.len() + 1;
    let name = std::path::Path::new(path)
        .file_stem()
        .map(|f| f.to_string_lossy().chars().take(100).collect::<String>())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("Environment {n:02}"));
    let mut v = s.doc.visual.clone();
    v.panoramas.push(Panorama { id, name, ext, file: Arc::new(bytes), pixels: Arc::new(pixels) });
    let environment = BUILTIN_ENVIRONMENTS + v.panoramas.len() - 1;
    v.environment = environment;
    s.doc.visual = v.sanitized();
    // Equirectangular panoramas are 2:1; other shapes still work, stretched around the sphere.
    let ratio = w as f64 / h.max(1) as f64;
    let note = if (ratio - 2.0).abs() > 0.05 { Some(format!("not 2:1 ({w}×{h}): stretched around the view")) } else { None };
    Ok(json!({ "environment": environment, "id": id.0, "note": note }))
}

/// Which custom environment a command means: "environment" (its place in the list) or "id".
fn custom_index(s: &Session, p: &Params) -> Result<usize> {
    let list = &s.doc.visual.panoramas;
    if p.has("id") {
        let id = p.id("id")?;
        return list.iter().position(|q| q.id == id).ok_or_else(|| bad(format!("no custom environment {}", id.0)));
    }
    let e = p.f64("environment")?;
    let k = e - BUILTIN_ENVIRONMENTS as f64;
    if e.fract() != 0.0 || !(0.0..list.len() as f64).contains(&k) {
        return Err(bad("that isn't a custom environment"));
    }
    Ok(k as usize)
}

/// The look a custom material gives a body: its picture tiled, a matte finish, the picture's average color (shown in
/// cards and wherever the picture isn't drawn).
pub(crate) fn custom_look(doc: &Document, image: ItemId) -> Option<shapecraft_doc::Appearance> {
    let img = doc.decal_images.iter().find(|i| i.id == image && i.material)?;
    let (_, _, px) = &*img.pixels;
    let mut sum = [0f64; 3];
    let mut n = 0f64;
    for c in px.as_chunks::<4>().0.iter().step_by(7) {
        for k in 0..3 {
            sum[k] += c[k] as f64;
        }
        n += 1.0;
    }
    let color = if n > 0.0 { sum.map(|v| (v / n / 255.0) as f32) } else { [0.627, 0.627, 0.643] };
    Some(shapecraft_doc::Appearance {
        name: img.name.clone(),
        finish: "Standard".into(),
        color,
        metal: 0.0,
        gloss: 0.3,
        texture: Some(image),
        scale: 1.0,
    })
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "import_decal",
            label: "Import Decal",
            menu: "",
            shortcut: None,
            help: "Add a PNG or JPEG picture to the project's decals",
            params: r#"{"path": "/logo.png"}"#,
            modifies: true,
            enabled: always,
            run: |s, p| import_picture(s, p.str("path")?, false),
        },
        CommandSpec {
            id: "import_material",
            label: "Import Material",
            menu: "",
            shortcut: None,
            help: "Make a custom material from a PNG or JPEG picture (Material ▸ Custom); set_appearance {\"custom\": id} puts it on bodies",
            params: r#"{"path": "/walnut.jpg"}"#,
            modifies: true,
            enabled: always,
            run: |s, p| import_picture(s, p.str("path")?, true),
        },
        CommandSpec {
            id: "rename_decal_image",
            label: "Rename Decal",
            menu: "",
            shortcut: None,
            help: "Rename a picture in the decal library",
            params: r#"{"image": id, "name": "Logo"}"#,
            modifies: true,
            enabled: |s| !s.doc.decal_images.is_empty(),
            run: |s, p| {
                let id = p.id("image")?;
                let name = p.str("name")?.trim();
                if name.is_empty() || name.len() > 100 {
                    return Err(bad("name must be 1–100 characters"));
                }
                let img = s.doc.decal_images.iter_mut().find(|i| i.id == id).ok_or_else(|| bad(format!("no decal picture {}", id.0)))?;
                img.name = name.to_string();
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "delete_decal_image",
            label: "Delete Decal",
            menu: "",
            shortcut: None,
            help: "Remove a picture from the decal library, with the decals that use it",
            params: r#"{"image": id}"#,
            modifies: true,
            enabled: |s| !s.doc.decal_images.is_empty(),
            run: |s, p| {
                let id = p.id("image")?;
                let before = s.doc.decal_images.len();
                s.doc.decal_images.retain(|i| i.id != id);
                if before == s.doc.decal_images.len() {
                    return Err(bad(format!("no decal picture {}", id.0)));
                }
                s.doc.decals.retain(|d| d.image != id);
                // Bodies wearing it as a custom material go back to grey.
                for b in &mut s.doc.bodies {
                    if b.appearance.as_ref().is_some_and(|a| a.texture == Some(id)) {
                        b.appearance = None;
                    }
                }
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "place_decal",
            label: "Place Decal",
            menu: "",
            shortcut: None,
            help: "Project a decal picture onto a body's face, centred on a point of it",
            params: r#"{"image": id, "body": id, "face": n, "point": [x,y,z], "normal": [x,y,z]? (default: the face's)}"#,
            modifies: true,
            enabled: |s| !s.doc.decal_images.is_empty() && !s.doc.bodies.is_empty(),
            run: |s, p| {
                let image = p.id("image")?;
                if !s.doc.decal_images.iter().any(|i| i.id == image && i.is_decal()) {
                    return Err(bad(format!("no decal picture {}", image.0)));
                }
                if s.doc.decals.len() >= 10_000 {
                    return Err(bad("a project holds 10000 decals at most"));
                }
                let (body, face, center, normal, axis) = spot(s, p)?;
                let size = body_size(&s.doc, body);
                let up = default_up(normal);
                let id = s.doc.alloc_id();
                s.doc.decals.push(Decal {
                    id,
                    image,
                    body,
                    face,
                    center,
                    normal,
                    up,
                    size,
                    scale: 1.0,
                    rotate: 0.0,
                    opacity: 1.0,
                    projection: Projection::Planar,
                    axis,
                    extend: false,
                });
                Ok(json!({ "decal": id.0 }))
            },
        },
        CommandSpec {
            id: "update_decal",
            label: "Decal",
            menu: "",
            shortcut: None,
            help: "Change a decal's scale, rotation, opacity or projection, or move it to another point",
            params: r#"{"decal": id, "scale": 0.3–10?, "rotate": deg?, "opacity": 0–1?, "projection": "planar"|"cylindrical"?, "extend": bool?, "body"+"face"+"point" (+"normal")? (move)}"#,
            modifies: true,
            enabled: |s| !s.doc.decals.is_empty(),
            run: |s, p| {
                let id = p.id("decal")?;
                let at = s.doc.decals.iter().position(|d| d.id == id).ok_or_else(|| bad(format!("no decal {}", id.0)))?;
                let moved = if p.has("point") { Some(spot(s, p)?) } else { None };
                let mut d = s.doc.decals[at].clone();
                if p.has("scale") {
                    d.scale = p.f64("scale")?.clamp(shapecraft_doc::DECAL_SCALE.0, shapecraft_doc::DECAL_SCALE.1);
                }
                if p.has("rotate") {
                    d.rotate = p.f64("rotate")?.clamp(-180.0, 180.0);
                }
                if p.has("opacity") {
                    d.opacity = p.f64("opacity")?.clamp(0.0, 1.0) as f32;
                }
                if p.has("projection") {
                    d.projection = p.parse("projection")?;
                }
                if p.has("extend") {
                    d.extend = p.bool_or("extend", d.extend);
                }
                if let Some((body, face, center, normal, axis)) = moved {
                    // Sliding over its face (or around a cylinder) keeps the picture's up; on another face it stands
                    // upright again.
                    let up = d.up - normal * d.up.dot(normal);
                    d.up = if body == d.body && face == d.face && up.length() > 1e-6 { up.normalize() } else { default_up(normal) };
                    if body != d.body {
                        d.size = body_size(&s.doc, body);
                    }
                    (d.body, d.face, d.center, d.normal, d.axis) = (body, face, center, normal, axis);
                }
                s.doc.decals[at] = d;
                Ok(json!({ "decal": id.0 }))
            },
        },
        CommandSpec {
            id: "delete_decal",
            label: "Delete Decal",
            menu: "",
            shortcut: None,
            help: "Take a decal off its body",
            params: r#"{"decal": id}"#,
            modifies: true,
            enabled: |s| !s.doc.decals.is_empty(),
            run: |s, p| {
                let id = p.id("decal")?;
                let before = s.doc.decals.len();
                s.doc.decals.retain(|d| d.id != id);
                if before == s.doc.decals.len() {
                    return Err(bad(format!("no decal {}", id.0)));
                }
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "import_environment",
            label: "Import Environment",
            menu: "",
            shortcut: None,
            help: "Add a panorama (.hdr, .exr, .png or .jpg, equirectangular) as an environment and select it: it lights and reflects in the materials and shows as the backdrop",
            params: r#"{"path": "/studio.hdr"}"#,
            modifies: true,
            enabled: always,
            run: |s, p| import_environment(s, p.str("path")?),
        },
        CommandSpec {
            id: "rename_environment",
            label: "Rename Environment",
            menu: "",
            shortcut: None,
            help: "Rename a custom environment",
            params: r#"{"environment": n | "id": id, "name": "Courtyard"}"#,
            modifies: true,
            enabled: |s| !s.doc.visual.panoramas.is_empty(),
            run: |s, p| {
                let k = custom_index(s, p)?;
                let name = p.str("name")?.trim();
                if name.is_empty() || name.chars().count() > 100 {
                    return Err(bad("name must be 1–100 characters"));
                }
                let mut v = s.doc.visual.clone();
                if let Some(q) = v.panoramas.get_mut(k) {
                    q.name = name.to_string();
                }
                s.doc.visual = v;
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "delete_environment",
            label: "Delete Environment",
            menu: "",
            shortcut: None,
            help: "Remove a custom environment; when it was selected the view goes back to Default",
            params: r#"{"environment": n | "id": id}"#,
            modifies: true,
            enabled: |s| !s.doc.visual.panoramas.is_empty(),
            run: |s, p| {
                let k = custom_index(s, p)?;
                let mut v = s.doc.visual.clone();
                v.panoramas.remove(k);
                let at = BUILTIN_ENVIRONMENTS + k;
                if v.environment == at {
                    v.environment = 0;
                } else if v.environment > at {
                    v.environment -= 1;
                }
                s.doc.visual = v.sanitized();
                Ok(Value::Null)
            },
        },
        CommandSpec {
            id: "visual_settings",
            label: "Environment",
            menu: "",
            shortcut: None,
            help: "Pick the Visualization environment and set its light, ground, shadow, backdrop and depth of field",
            params: r#"{"environment": n? (0–8 built in, 9… imported panoramas), "light_rotation": -180–180?, "sun_elevation": 5–90?, "light_intensity": 0.5–2?, "ground": "model"|"xy"|"off"?, "object_shadow": bool?, "background": [r,g,b] (0–1, the environment's own)?, "aperture": 1.4–32?, "blur": 0.5–2?, "focus": [x,y,z]|null?}"#,
            modifies: true,
            enabled: always,
            run: |s, p| {
                let mut v: VisualSettings = s.doc.visual.clone();
                if p.has("environment") {
                    let e = p.f64("environment")?;
                    if e.fract() != 0.0 || !(0.0..(BUILTIN_ENVIRONMENTS + v.panoramas.len()) as f64).contains(&e) {
                        return Err(bad("no such environment"));
                    }
                    v.environment = e as usize;
                }
                for (key, slot) in [
                    ("light_rotation", &mut v.light_rotation),
                    ("sun_elevation", &mut v.sun_elevation),
                    ("light_intensity", &mut v.light_intensity),
                    ("aperture", &mut v.aperture),
                    ("blur", &mut v.blur),
                ] {
                    if p.has(key) {
                        *slot = p.f64(key)?;
                    }
                }
                if p.has("ground") {
                    v.ground = p.parse::<Ground>("ground")?;
                }
                if p.has("object_shadow") {
                    v.object_shadow = p.bool_or("object_shadow", v.object_shadow);
                }
                if p.has("background") {
                    let c = p.vec3("background")?;
                    let c = [c.x as f32, c.y as f32, c.z as f32];
                    let env = v.environment;
                    v.backgrounds.retain(|(e, _)| *e != env);
                    v.backgrounds.push((env, c));
                }
                if let Some(f) = p.raw("focus") {
                    v.focus = if f.is_null() { None } else { Some(p.vec3("focus")?) };
                }
                s.doc.visual = v.sanitized();
                Ok(Value::Null)
            },
        },
    ]
}

/// Where a decal goes: the body, the face, the point, the outward normal there, and (on a round face) the axis
/// point and radius a cylindrical projection wraps around.
type Spot = (ItemId, u32, DVec3, DVec3, Option<(DVec3, f64)>);

fn spot(s: &Session, p: &Params) -> Result<Spot> {
    let body_id = p.id("body")?;
    let face = p.f64("face")?;
    if !(0.0..=1e7).contains(&face) || face.fract() != 0.0 {
        return Err(bad("'face' must be a face number"));
    }
    let face = face as u32;
    let point = p.vec3("point")?;
    let body = s.doc.body(body_id).ok_or_else(|| bad(format!("no body {}", body_id.0)))?;
    if !body.mesh.triangle_faces.contains(&face) {
        return Err(bad(format!("{} has no face {face}", body.name)));
    }
    let k = s.kernel.as_ref();
    let flat = k.face_plane(&body.shape, face).ok().flatten();
    let round = if flat.is_none() { k.axis_of(&body.shape, SubShape::Face(face)).ok().flatten() } else { None };
    // On a round face: the axis point level with the picked point, and its distance as the radius.
    let axis = round.and_then(|a| {
        let dir = a.dir.normalize_or_zero();
        let foot = a.origin + dir * (point - a.origin).dot(dir);
        let r = (point - foot).length();
        (dir != DVec3::ZERO && r > 1e-9).then_some((foot, r))
    });
    let normal = if p.has("normal") {
        p.vec3("normal")?
    } else if let Some(f) = flat {
        f.normal
    } else if let Some((foot, _)) = axis {
        point - foot
    } else {
        mesh_normal(body, face).ok_or_else(|| bad("that face has no direction"))?
    };
    let normal = normal.normalize_or_zero();
    if normal == DVec3::ZERO {
        return Err(bad("'normal' must not be zero"));
    }
    Ok((body_id, face, point, normal, axis))
}

/// The average normal of a face's triangles.
fn mesh_normal(body: &shapecraft_doc::Body, face: u32) -> Option<DVec3> {
    let m = &body.mesh;
    let mut sum = DVec3::ZERO;
    for (t, f) in m.indices.as_chunks::<3>().0.iter().zip(&m.triangle_faces) {
        if *f == face {
            let p = |i: u32| m.positions.get(i as usize).map(|v| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
            let (a, b, c) = (p(t[0])?, p(t[1])?, p(t[2])?);
            sum += (b - a).cross(c - a);
        }
    }
    (sum.length() > 1e-12).then(|| sum.normalize())
}

/// The picture's up direction on a face: world up laid onto the face, or world Y on a face that looks up or down.
fn default_up(normal: DVec3) -> DVec3 {
    for up in [DVec3::Z, DVec3::Y, DVec3::X] {
        let u = up - normal * up.dot(normal);
        if u.length() > 0.2 {
            return u.normalize();
        }
    }
    DVec3::Y
}

/// The body's largest extent: a decal at 1.0x is this wide, as observed.
fn body_size(doc: &Document, body: ItemId) -> f64 {
    let Some(b) = doc.body(body) else { return 10.0 };
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in &b.mesh.positions {
        let p = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let size = (hi - lo).max_element();
    if size.is_finite() && size > 1e-6 { size } else { 10.0 }
}

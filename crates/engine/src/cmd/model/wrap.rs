//! Wrap & Emboss: sketch profiles laid onto a cylindrical or conical face, their sizes kept, then raised or engraved.

use serde_json::{Value, json};
use shapecraft_doc::{ItemId, Pick};

use super::set_body_shape;
use crate::{EngineError, Params, Result, Session};

/// The profiles (one sketch's regions), the body and its round face, from the parameters or the selection.
fn targets(s: &Session, p: &Params) -> Result<(ItemId, Vec<u32>, ItemId, Option<u32>)> {
    let (sketch, regions) = if p.has("sketch") {
        let regions: Vec<u32> = if p.has("regions") { p.parse("regions")? } else { vec![p.f64_or("region", 0.0)? as u32] };
        (p.id("sketch")?, regions)
    } else {
        let sketch = s
            .selection
            .iter()
            .find_map(|pk| if let Pick::Region { sketch, .. } = *pk { Some(sketch) } else { None })
            .ok_or_else(|| EngineError::NotAvailable("select the sketch profiles to wrap".into()))?;
        let regions = s
            .selection
            .iter()
            .filter_map(|pk| match *pk {
                Pick::Region { sketch: k, region } if k == sketch => Some(region),
                _ => None,
            })
            .collect();
        (sketch, regions)
    };
    if regions.is_empty() || regions.len() > 10_000 {
        return Err(EngineError::BadParams("give 1–10000 regions".into()));
    }
    let picked_face = s.selection.iter().find_map(|pk| if let Pick::Face { body, face } = *pk { Some((body, face)) } else { None });
    let (body, face) = if p.has("body") {
        let face = if p.has("face") {
            let f = p.f64("face")?;
            if !(0.0..=1e7).contains(&f) || f.fract() != 0.0 {
                return Err(EngineError::BadParams("'face' must be a face number".into()));
            }
            Some(f as u32)
        } else {
            None
        };
        (p.id("body")?, face)
    } else {
        picked_face.map(|(b, f)| (b, Some(f))).ok_or_else(|| EngineError::NotAvailable("select a cylindrical or conical face".into()))?
    };
    Ok((sketch, regions, body, face))
}

pub(super) fn wrap_emboss(s: &mut Session, p: &Params) -> Result<Value> {
    let (sketch, regions, body, face) = targets(s, p)?;
    let depth = p.f64("depth")?;
    let rotation = p.f64_or("rotation", 0.0)?;
    if !(depth.is_finite() && depth.abs() > 1e-6 && depth.abs() < 1e5) {
        return Err(EngineError::BadParams("the emboss depth must be a non-zero length".into()));
    }
    if !(rotation.is_finite() && rotation.abs() <= 3600.0) {
        return Err(EngineError::BadParams("rotation must be an angle in degrees".into()));
    }
    let center = if p.has("center") { Some(p.vec3("center")?) } else { None };
    let sk = s.doc.sketch(sketch).ok_or_else(|| EngineError::BadParams(format!("no sketch {}", sketch.0)))?;
    let mut faces = Vec::new();
    for r in &regions {
        let region = sk.region(*r).ok_or_else(|| EngineError::BadParams(format!("{} has no region {r}", sk.name)))?;
        faces.push(region.face.clone());
    }
    let frame = sk.frame();
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams(format!("no body {}", body.0)))?;
    let shape = s.kernel.wrap_emboss(&b.shape, face, &faces, frame, depth, rotation.to_radians(), center)?;
    set_body_shape(s, body, shape)?;
    s.selection = vec![Pick::Body { body }];
    Ok(json!({ "body": body.0 }))
}

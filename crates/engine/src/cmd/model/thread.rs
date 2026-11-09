use serde_json::{Value, json};
use shapecraft_doc::Pick;
use shapecraft_kernel::threads::ThreadSpec;

use crate::{EngineError, Params, Result, Session};

pub(super) fn thread(s: &mut Session, p: &Params) -> Result<Value> {
    let (body, face) = if p.has("body") || p.has("face") {
        (p.id("body")?, p.parse::<u32>("face")?)
    } else {
        match s.selection.as_slice() {
            [Pick::Face { body, face }] => (*body, *face),
            _ => return Err(EngineError::NotAvailable("select one cylindrical shaft or hole wall".into())),
        }
    };
    let preset_id = p.str("preset")?;
    let preset =
        crate::threads::preset(preset_id).ok_or_else(|| EngineError::BadParams("unknown thread preset; see thread_presets".into()))?;
    let b = s.doc.body(body).ok_or_else(|| EngineError::BadParams("no such body".into()))?;
    let wall = s
        .kernel
        .cylinder_face(&b.shape, face)?
        .ok_or_else(|| EngineError::BadParams("select a complete cylindrical shaft or hole wall".into()))?;
    let internal = match if p.has("kind") { Some(p.str("kind")?) } else { None } {
        None => wall.internal,
        Some("internal") => true,
        Some("external") => false,
        _ => return Err(EngineError::BadParams("kind must be internal or external".into())),
    };
    let left_hand = match if p.has("hand") { Some(p.str("hand")?) } else { None } {
        None | Some("right") => false,
        Some("left") => true,
        _ => return Err(EngineError::BadParams("hand must be right or left".into())),
    };
    let spec = ThreadSpec {
        diameter: preset.diameter,
        pitch: preset.pitch,
        internal,
        left_hand,
        length: p.f64("length")?,
        offset: p.f64_or("offset", 0.0)?,
        clearance: p.f64_or("clearance", 0.0)?,
        reverse: if p.has("reverse") { p.parse::<bool>("reverse")? } else { false },
    };
    let shape = shapecraft_kernel::threads::cut(s.kernel.as_ref(), &b.shape, wall, spec)?;
    super::set_body_shape(s, body, shape)?;
    let mut logged = p.all().clone();
    logged["kind"] = json!(if internal { "internal" } else { "external" });
    logged["preset"] = json!(preset.id);
    s.log_params = Some(logged);
    s.selection = vec![Pick::Body { body }];
    Ok(json!({ "body": body.0, "preset": preset.id, "diameter": preset.diameter, "pitch": preset.pitch }))
}

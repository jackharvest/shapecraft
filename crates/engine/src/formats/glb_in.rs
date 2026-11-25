//! The materials of a binary glTF file (GLB), for Visualization's custom materials. Geometry is left out.

use serde_json::Value;
use shapecraft_doc::Appearance;

pub struct GlbMaterial {
    pub look: Appearance,
    /// The base color's picture, as the file holds it (PNG or JPEG).
    pub picture: Option<Vec<u8>>,
}

const MOST: usize = 200;

pub fn materials(bytes: &[u8]) -> Result<Vec<GlbMaterial>, String> {
    let word =
        |at: usize| -> Option<usize> { bytes.get(at..at + 4).and_then(|b| b.try_into().ok()).map(|b| u32::from_le_bytes(b) as usize) };
    const NOT: &str = "that isn't a GLB file";
    if bytes.get(..4) != Some(b"glTF") || word(4) != Some(2) {
        return Err(NOT.into());
    }
    let (mut json, mut bin): (Option<&[u8]>, &[u8]) = (None, &[]);
    let mut at = 12;
    while let (Some(len), Some(kind)) = (word(at), word(at + 4)) {
        let Some(chunk) = at.checked_add(8).and_then(|a| bytes.get(a..a.checked_add(len)?)) else { break };
        match kind {
            0x4E4F_534A => json = Some(chunk),
            0x004E_4942 => bin = chunk,
            _ => {}
        }
        at += 8 + len;
    }
    let doc: Value = serde_json::from_slice(json.ok_or(NOT)?).map_err(|e| format!("the GLB's contents couldn't be read: {e}"))?;
    let num = |v: &Value, or: f64| v.as_f64().filter(|x| x.is_finite()).unwrap_or(or) as f32;
    let list = doc["materials"].as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(list
        .iter()
        .take(MOST)
        .map(|m| {
            let pbr = &m["pbrMetallicRoughness"];
            let base = |k: usize| num(&pbr["baseColorFactor"][k], 1.0).clamp(0.0, 1.0);
            let glow = (0..3).map(|k| num(&m["emissiveFactor"][k], 0.0)).fold(0.0f32, f32::max);
            let ext = &m["extensions"];
            let mut color = [base(0), base(1), base(2)].map(to_srgb);
            if glow > 0.0 && pbr["baseColorFactor"].is_null() {
                color = [0, 1, 2].map(|k| to_srgb(num(&m["emissiveFactor"][k], 0.0).clamp(0.0, 1.0)));
            }
            let look = Appearance {
                name: m["name"].as_str().unwrap_or_default().chars().take(100).collect(),
                finish: "Standard".into(),
                color,
                metal: num(&pbr["metallicFactor"], 1.0),
                gloss: 1.0 - num(&pbr["roughnessFactor"], 1.0),
                opacity: if m["alphaMode"].as_str() == Some("BLEND") { base(3) } else { 1.0 },
                transmission: num(&ext["KHR_materials_transmission"]["transmissionFactor"], 0.0),
                ior: num(&ext["KHR_materials_ior"]["ior"], 1.5),
                emission: glow.clamp(0.0, 1.0),
                ..Appearance::default()
            };
            GlbMaterial { look, picture: picture(&doc, bin, &pbr["baseColorTexture"]["index"]) }
        })
        .collect())
}

fn picture(doc: &Value, bin: &[u8], texture: &Value) -> Option<Vec<u8>> {
    let at = |list: &str, v: &Value| -> Option<Value> { doc[list].get(usize::try_from(v.as_u64()?).ok()?).cloned() };
    let image = at("images", &at("textures", texture)?["source"])?;
    let view = at("bufferViews", &image["bufferView"])?;
    if view["buffer"].as_u64().unwrap_or(0) != 0 {
        return None;
    }
    let start = usize::try_from(view["byteOffset"].as_u64().unwrap_or(0)).ok()?;
    let len = usize::try_from(view["byteLength"].as_u64()?).ok()?;
    let bytes = bin.get(start..start.checked_add(len)?)?;
    (len <= 64 << 20).then(|| bytes.to_vec())
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

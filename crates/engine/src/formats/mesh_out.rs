//! Mesh exports: OBJ (+ MTL), 3MF and binary glTF. Model units are millimetres.

use std::fmt::Write as _;

use glam::DVec3;
use serde_json::json;
use std::sync::Arc;

use shapecraft_kernel::Mesh;

/// One body as it goes into a mesh file.
pub struct ExportBody<'a> {
    pub name: &'a str,
    pub mesh: Arc<Mesh>,
    /// sRGB 0..1.
    pub color: [f32; 3],
    pub metal: f32,
    pub gloss: f32,
}

fn clean(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    if s.is_empty() { "Body".into() } else { s }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

fn to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Wavefront OBJ with normals, one object per body, and the MTL text it references by `mtl_name`.
pub fn write_obj(bodies: &[ExportBody], mtl_name: &str) -> (String, String) {
    let mut obj = format!("# ShapeCraft\nmtllib {mtl_name}\n");
    let mut mtl = String::from("# ShapeCraft\n");
    let (mut vbase, mut nbase) = (1usize, 1usize);
    for (i, b) in bodies.iter().enumerate() {
        let mat = format!("{}_{}", clean(b.name), i + 1);
        let [r, g, bl] = b.color;
        let _ = write!(
            mtl,
            "newmtl {mat}\nKd {r:.4} {g:.4} {bl:.4}\nKs {s:.3} {s:.3} {s:.3}\nNs {ns:.1}\nd 1\nillum 2\n\n",
            s = b.gloss * 0.5,
            ns = 10.0 + b.gloss * 240.0
        );
        let _ = writeln!(obj, "o {}\nusemtl {mat}", clean(b.name));
        for p in &b.mesh.positions {
            let _ = writeln!(obj, "v {} {} {}", p[0], p[1], p[2]);
        }
        let has_normals = b.mesh.normals.len() == b.mesh.positions.len();
        if has_normals {
            for n in &b.mesh.normals {
                let _ = writeln!(obj, "vn {} {} {}", n[0], n[1], n[2]);
            }
        }
        for t in b.mesh.indices.as_chunks::<3>().0 {
            let [a, bb, c] = t.map(|i| i as usize);
            if has_normals {
                let _ = writeln!(obj, "f {}//{} {}//{} {}//{}", a + vbase, a + nbase, bb + vbase, bb + nbase, c + vbase, c + nbase);
            } else {
                let _ = writeln!(obj, "f {} {} {}", a + vbase, bb + vbase, c + vbase);
            }
        }
        vbase += b.mesh.positions.len();
        if has_normals {
            nbase += b.mesh.normals.len();
        }
    }
    (obj, mtl)
}

/// A 3MF package: one welded mesh object per body, coloured from its appearance, in millimetres.
///
/// Colours go in a Materials-extension colour group (Bambu Studio and OrcaSlicer read those from other apps and put
/// each colour on its own filament), and `Metadata/model_settings.config` names that filament per object (OrcaSlicer
/// only keeps it from there). The core `basematerials` list stays for readers without the extension. No project
/// settings are written: a partial one makes OrcaSlicer crash.
pub fn write_3mf(bodies: &[ExportBody]) -> Option<Vec<u8>> {
    let group = bodies.len() + 2;
    let mut model = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:m=\"http://schemas.microsoft.com/3dmanufacturing/material/2015/02\">\n <metadata name=\"Application\">ShapeCraft</metadata>\n <resources>\n  <basematerials id=\"1\">\n",
    );
    for b in bodies {
        let _ = writeln!(model, "   <base name=\"{}\" displaycolor=\"{}\" />", xml_escape(b.name), hex(b.color));
    }
    model.push_str("  </basematerials>\n");
    let _ = writeln!(model, "  <m:colorgroup id=\"{group}\">");
    for b in bodies {
        let _ = writeln!(model, "   <m:color color=\"{}FF\" />", hex(b.color));
    }
    model.push_str("  </m:colorgroup>\n");
    for (i, b) in bodies.iter().enumerate() {
        let pts: Vec<DVec3> = b.mesh.positions.iter().map(|p| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))).collect();
        let tris: Vec<[u32; 3]> = b.mesh.indices.as_chunks::<3>().0.to_vec();
        let (pts, idx) = super::weld(&pts, &tris);
        let _ = write!(
            model,
            "  <object id=\"{}\" name=\"{}\" type=\"model\" pid=\"{group}\" pindex=\"{i}\">\n   <mesh>\n    <vertices>\n",
            i + 2,
            xml_escape(b.name)
        );
        for p in &pts {
            let _ = writeln!(model, "     <vertex x=\"{}\" y=\"{}\" z=\"{}\" />", p.x as f32, p.y as f32, p.z as f32);
        }
        model.push_str("    </vertices>\n    <triangles>\n");
        for t in idx.as_chunks::<3>().0 {
            let _ = writeln!(model, "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" />", t[0], t[1], t[2]);
        }
        model.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    model.push_str(" </resources>\n <build>\n");
    for i in 0..bodies.len() {
        let _ = writeln!(model, "  <item objectid=\"{}\" />", i + 2);
    }
    model.push_str(" </build>\n</model>\n");
    let types = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\" /><Default Extension=\"model\" ContentType=\"application/vnd.ms-package.3dmanufacturing-3dmodel+xml\" /><Default Extension=\"config\" ContentType=\"text/xml\" /></Types>\n";
    let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\" /></Relationships>\n";
    super::zip::store(&[
        ("[Content_Types].xml", types.as_bytes().to_vec()),
        ("_rels/.rels", rels.as_bytes().to_vec()),
        ("3D/3dmodel.model", model.into_bytes()),
        ("Metadata/model_settings.config", filament_slots(bodies).into_bytes()),
    ])
}

/// The slicer's per-object filament (1-based): one per distinct body colour, in the order they first appear.
fn filament_slots(bodies: &[ExportBody]) -> String {
    let mut colours: Vec<String> = Vec::new();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n");
    for (i, b) in bodies.iter().enumerate() {
        let c = hex(b.color);
        let slot = match colours.iter().position(|x| *x == c) {
            Some(k) => k + 1,
            None => {
                colours.push(c);
                colours.len()
            }
        };
        let _ = write!(
            out,
            "  <object id=\"{}\">\n    <metadata key=\"name\" value=\"{}\"/>\n    <metadata key=\"extruder\" value=\"{slot}\"/>\n  </object>\n",
            i + 2,
            xml_escape(b.name)
        );
    }
    out.push_str("</config>\n");
    out
}

/// Binary glTF 2.0: one node per body under a root that turns Z-up millimetres into Y-up metres, with a PBR
/// material from the body's appearance.
pub fn write_glb(bodies: &[ExportBody]) -> Option<Vec<u8>> {
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut meshes, mut materials, mut nodes) = (vec![], vec![], vec![], vec![], vec![]);
    for b in bodies {
        let m = &b.mesh;
        if m.positions.is_empty() || m.indices.is_empty() {
            continue;
        }
        let normals_ok = m.normals.len() == m.positions.len();
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let mut push_view = |bytes: Vec<u8>, target: u32| -> usize {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            views.push(json!({ "buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len(), "target": target }));
            bin.extend(bytes);
            views.len() - 1
        };
        let pos_view = push_view(m.positions.iter().flat_map(|p| p.iter().flat_map(|f| f.to_le_bytes())).collect(), 34962);
        accessors.push(
            json!({ "bufferView": pos_view, "componentType": 5126, "count": m.positions.len(), "type": "VEC3", "min": lo, "max": hi }),
        );
        let pos_acc = accessors.len() - 1;
        let mut attributes = json!({ "POSITION": pos_acc });
        if normals_ok {
            let nview = push_view(m.normals.iter().flat_map(|p| p.iter().flat_map(|f| f.to_le_bytes())).collect(), 34962);
            accessors.push(json!({ "bufferView": nview, "componentType": 5126, "count": m.normals.len(), "type": "VEC3" }));
            attributes["NORMAL"] = json!(accessors.len() - 1);
        }
        let iview = push_view(m.indices.iter().flat_map(|i| i.to_le_bytes()).collect(), 34963);
        accessors.push(json!({ "bufferView": iview, "componentType": 5125, "count": m.indices.len(), "type": "SCALAR" }));
        let idx_acc = accessors.len() - 1;
        let [r, g, bl] = b.color.map(to_linear);
        materials.push(json!({
            "name": b.name,
            "pbrMetallicRoughness": { "baseColorFactor": [r, g, bl, 1.0], "metallicFactor": b.metal.clamp(0.0, 1.0), "roughnessFactor": (1.0 - b.gloss).clamp(0.04, 1.0) }
        }));
        meshes.push(
            json!({ "name": b.name, "primitives": [{ "attributes": attributes, "indices": idx_acc, "material": materials.len() - 1 }] }),
        );
        nodes.push(json!({ "name": b.name, "mesh": meshes.len() - 1 }));
    }
    let children: Vec<usize> = (1..=nodes.len()).collect();
    let mut all_nodes = vec![
        json!({ "name": "ShapeCraft", "children": children, "rotation": [-std::f64::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f64::consts::FRAC_1_SQRT_2], "scale": [0.001, 0.001, 0.001] }),
    ];
    all_nodes.extend(nodes);
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let doc = json!({
        "asset": { "version": "2.0", "generator": "ShapeCraft" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": all_nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": bin.len() }],
    });
    let mut json_bytes = serde_json::to_vec(&doc).ok()?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let total = u32::try_from(12 + 8 + json_bytes.len() + 8 + bin.len()).ok()?;
    let mut out = Vec::with_capacity(total as usize);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend(total.to_le_bytes());
    out.extend(u32::try_from(json_bytes.len()).ok()?.to_le_bytes());
    out.extend(b"JSON");
    out.extend(json_bytes);
    out.extend(u32::try_from(bin.len()).ok()?.to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    Some(out)
}

/// ASCII STL of one mesh.
pub fn write_stl_ascii(m: &Mesh) -> String {
    let mut out = String::from("solid ShapeCraft\n");
    for t in m.indices.as_chunks::<3>().0 {
        let p = |i: u32| m.positions.get(i as usize).copied().unwrap_or_default();
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 { n.map(|x| x / len) } else { [0.0; 3] };
        let _ = write!(out, "facet normal {} {} {}\n outer loop\n", n[0], n[1], n[2]);
        for q in [a, b, c] {
            let _ = writeln!(out, "  vertex {} {} {}", q[0], q[1], q[2]);
        }
        out.push_str(" endloop\nendfacet\n");
    }
    out.push_str("endsolid ShapeCraft\n");
    out
}

/// A USDZ package (for AR Quick Look and other USD viewers): one USDA layer with a mesh per body and a
/// UsdPreviewSurface material from its appearance, in millimetres, Z up.
pub fn write_usdz(bodies: &[ExportBody]) -> Option<Vec<u8>> {
    let mut usda = String::from(
        "#usda 1.0\n(\n    defaultPrim = \"Root\"\n    metersPerUnit = 0.001\n    upAxis = \"Z\"\n)\n\ndef Xform \"Root\"\n{\n",
    );
    let mut names: Vec<String> = Vec::new();
    for (i, b) in bodies.iter().enumerate() {
        let mut name = clean(b.name).replace(['-', '.'], "_");
        if name.chars().next().is_none_or(|c| c.is_ascii_digit()) {
            name = format!("B_{name}");
        }
        if names.contains(&name) {
            name = format!("{name}_{}", i + 1);
        }
        names.push(name.clone());
        let tris = b.mesh.indices.as_chunks::<3>().0;
        let _ = write!(
            usda,
            "    def Mesh \"{name}\" (\n        prepend apiSchemas = [\"MaterialBindingAPI\"]\n    )\n    {{\n        uniform token subdivisionScheme = \"none\"\n        int[] faceVertexCounts = ["
        );
        usda.push_str(&vec!["3"; tris.len()].join(", "));
        usda.push_str("]\n        int[] faceVertexIndices = [");
        usda.push_str(&tris.iter().flat_map(|t| t.iter().map(|v| v.to_string())).collect::<Vec<_>>().join(", "));
        usda.push_str("]\n        point3f[] points = [");
        usda.push_str(&b.mesh.positions.iter().map(|p| format!("({}, {}, {})", p[0], p[1], p[2])).collect::<Vec<_>>().join(", "));
        usda.push_str("]\n");
        if b.mesh.normals.len() == b.mesh.positions.len() {
            usda.push_str("        normal3f[] normals = [");
            usda.push_str(&b.mesh.normals.iter().map(|n| format!("({}, {}, {})", n[0], n[1], n[2])).collect::<Vec<_>>().join(", "));
            usda.push_str("] (\n            interpolation = \"vertex\"\n        )\n");
        }
        let _ = write!(usda, "        rel material:binding = </Root/Materials/M{i}>\n    }}\n\n");
    }
    usda.push_str("    def Scope \"Materials\"\n    {\n");
    for (i, b) in bodies.iter().enumerate() {
        let [r, g, bl] = b.color.map(to_linear);
        let _ = write!(
            usda,
            "        def Material \"M{i}\"\n        {{\n            token outputs:surface.connect = </Root/Materials/M{i}/Surface.outputs:surface>\n            def Shader \"Surface\"\n            {{\n                uniform token info:id = \"UsdPreviewSurface\"\n                color3f inputs:diffuseColor = ({r}, {g}, {bl})\n                float inputs:metallic = {}\n                float inputs:roughness = {}\n                token outputs:surface\n            }}\n        }}\n",
            b.metal.clamp(0.0, 1.0),
            (1.0 - b.gloss).clamp(0.05, 1.0)
        );
    }
    usda.push_str("    }\n}\n");
    super::zip::store_aligned(&[("model.usda", usda.into_bytes())], 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(name: &str, color: [f32; 3]) -> ExportBody<'_> {
        let mesh =
            Mesh { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], indices: vec![0, 1, 2], ..Default::default() };
        ExportBody { name, mesh: Arc::new(mesh), color, metal: 0.0, gloss: 0.5 }
    }

    #[test]
    fn same_colours_share_a_filament() {
        let red = [0.9, 0.1, 0.1];
        let blue = [0.1, 0.2, 0.9];
        let bodies = [body("A", red), body("B", blue), body("C & D", red)];
        let cfg = filament_slots(&bodies);
        assert!(cfg.contains("<object id=\"2\">\n    <metadata key=\"name\" value=\"A\"/>\n    <metadata key=\"extruder\" value=\"1\"/>"));
        assert!(cfg.contains("<object id=\"3\">\n    <metadata key=\"name\" value=\"B\"/>\n    <metadata key=\"extruder\" value=\"2\"/>"));
        assert!(cfg.contains("value=\"C &amp; D\"/>\n    <metadata key=\"extruder\" value=\"1\"/>"));
        let bytes = write_3mf(&bodies).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("<m:colorgroup id=\"5\">"));
        assert!(text.contains("pid=\"5\" pindex=\"2\""));
        assert!(text.contains("Metadata/model_settings.config"));
    }
}

//! `xt2step IN OUT.step`: read a transmit file, build its bodies with OCCT and write them as STEP (a check).
use shapecraft_kernel::Kernel;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (Some(inp), Some(out)) = (a.get(1), a.get(2)) else { return eprintln!("usage: xt2step IN OUT.step") };
    let bytes = match std::fs::read(inp) {
        Ok(b) => b,
        Err(e) => return eprintln!("{e}"),
    };
    let f = match shapecraft_xt::parse(&bytes) {
        Ok(f) => f,
        Err(e) => return eprintln!("parse: {e}"),
    };
    let bodies = match shapecraft_xt::bodies(&f) {
        Ok(b) => b,
        Err(e) => return eprintln!("model: {e}"),
    };
    let k = shapecraft_kernel_occt::OcctKernel::new();
    let mut shapes = Vec::new();
    for b in &bodies {
        println!(
            "body #{} {:?} name {:?} colour {:?}: {} faces {} edges {} vertices, surfaces {} curves {}",
            b.node,
            b.brep.kind,
            b.name,
            b.color,
            b.brep.faces.len(),
            b.brep.edges.len(),
            b.brep.vertices.len(),
            b.brep.surfaces.len(),
            b.brep.curves.len()
        );
        match k.from_brep(&b.brep) {
            Ok(s) => {
                if let Ok(m) = k.mass_props(&s) {
                    println!("  volume {:.4} area {:.4} centroid {:?}", m.volume, m.area, m.centroid);
                }
                if let Ok(bb) = k.bounds(&s) {
                    println!("  bounds {:?} .. {:?}", bb.min, bb.max);
                    if std::env::var_os("XT2STEP_CHECK").is_some() {
                        match k.tessellate(&s, 0.01) {
                            Ok(m) => println!("  tessellated: {} triangles", m.indices.len() / 3),
                            Err(e) => println!("  tessellate failed: {e}"),
                        }
                        let c = (bb.min + bb.max) / 2.0;
                        let cut = k.make_box(c, bb.max - bb.min).and_then(|b| k.boolean(shapecraft_kernel::BoolOp::Subtract, &s, &b));
                        match cut.and_then(|r| k.mass_props(&r)) {
                            Ok(m) => println!("  minus a corner box: volume {:.4}", m.volume),
                            Err(e) => println!("  cut failed: {e}"),
                        }
                        if let Some(r) = std::env::var("XT2STEP_FILLET").ok().and_then(|r| r.parse::<f64>().ok()) {
                            match k.fillet_all_edges(&s, r).and_then(|f| k.mass_props(&f)) {
                                Ok(m) => println!("  filleted {r}: volume {:.4} area {:.4}", m.volume, m.area),
                                Err(e) => println!("  fillet failed: {e}"),
                            }
                        }
                        match k.split_by_plane(&s, c, shapecraft_kernel::DVec3::X) {
                            Ok(p) => println!(
                                "  split: {} pieces, volumes {:?}",
                                p.len(),
                                p.iter().filter_map(|x| k.mass_props(x).ok().map(|m| m.volume)).collect::<Vec<_>>()
                            ),
                            Err(e) => println!("  split failed: {e}"),
                        }
                    }
                }
                shapes.push(s);
            }
            Err(e) => println!("  build failed: {e}"),
        }
    }
    if let Err(e) = k.write_step(&shapes, std::path::Path::new(out)) {
        eprintln!("write: {e}")
    }
}

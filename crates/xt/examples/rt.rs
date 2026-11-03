//! `rt IN`: XT → B-rep → OCCT → B-rep → OCCT; prints volumes at each step (a kernel round-trip check).
use shapecraft_kernel::Kernel;
fn main() {
    let Some(inp) = std::env::args().nth(1) else { return eprintln!("usage: rt IN") };
    let bytes = std::fs::read(&inp).unwrap_or_default();
    let f = match shapecraft_xt::parse(&bytes) {
        Ok(f) => f,
        Err(e) => return eprintln!("parse: {e}"),
    };
    let k = shapecraft_kernel_occt::OcctKernel::new();
    let bodies = match shapecraft_xt::bodies(&f) {
        Ok(b) => b,
        Err(e) => return println!("bodies: {e}"),
    };
    for b in bodies {
        let s = match k.from_brep(&b.brep) {
            Ok(s) => s,
            Err(e) => {
                println!("build: {e}");
                continue;
            }
        };
        let v0 = k.mass_props(&s).map(|m| m.volume).unwrap_or(f64::NAN);
        match k.to_brep(&s) {
            Ok(bs) => {
                for b2 in bs {
                    println!(
                        "export: {} faces {} edges {} curves {} surfaces",
                        b2.faces.len(),
                        b2.edges.len(),
                        b2.curves.len(),
                        b2.surfaces.len()
                    );
                    match k.from_brep(&b2) {
                        Ok(s2) => println!("volume {v0:.4} -> {:.4}", k.mass_props(&s2).map(|m| m.volume).unwrap_or(f64::NAN)),
                        Err(e) => println!("rebuild: {e}"),
                    }
                }
            }
            Err(e) => println!("export: {e}"),
        }
    }
}

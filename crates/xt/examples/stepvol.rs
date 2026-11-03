//! `stepvol FILE.step…`: volume and area of every solid in STEP files (to compare with what we read).
use shapecraft_kernel::Kernel;
fn main() {
    let k = shapecraft_kernel_occt::OcctKernel::new();
    for f in std::env::args().skip(1) {
        match k.read_step(std::path::Path::new(&f)) {
            Ok(shapes) => {
                let mut v: Vec<(f64, f64)> = shapes
                    .iter()
                    .flat_map(|s| k.solids(s).unwrap_or_default())
                    .filter_map(|s| k.mass_props(&s).ok())
                    .map(|m| (m.volume, m.area))
                    .collect();
                v.sort_by(|a, b| a.0.total_cmp(&b.0));
                println!("{f}: {}", v.iter().map(|(a, b)| format!("{a:.4}/{b:.4}")).collect::<Vec<_>>().join(" "));
            }
            Err(e) => println!("{f}: {e}"),
        }
    }
}

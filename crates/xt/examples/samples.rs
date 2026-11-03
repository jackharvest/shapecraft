//! `samples DIR`: writes test solids as .x_t and .x_b (to try them in other Parasolid-based apps).
use shapecraft_kernel::{Axis, BoolOp, DVec3, Kernel};
use shapecraft_xt::Format;
fn main() {
    let Some(dir) = std::env::args().nth(1) else { return eprintln!("usage: samples DIR") };
    let k = shapecraft_kernel_occt::OcctKernel::new();
    let rect = vec![DVec3::ZERO, DVec3::new(20.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 0.0), DVec3::new(0.0, 10.0, 0.0)];
    let Ok(boxy) = k.extrude_polygon(&rect, DVec3::new(0.0, 0.0, 8.0)) else { return };
    let Ok(hole) = k.extrude_circle(DVec3::new(10.0, 5.0, -1.0), 3.0, DVec3::new(0.0, 0.0, 10.0)) else { return };
    let shapes = [
        ("box", Ok(boxy.clone())),
        ("cylinder", k.extrude_circle(DVec3::ZERO, 5.0, DVec3::new(0.0, 0.0, 12.0))),
        ("rounded", k.fillet_all_edges(&boxy, 2.0)),
        ("holed", k.boolean(BoolOp::Subtract, &boxy, &hole)),
        (
            "cone",
            k.revolve_polygon(
                &[DVec3::ZERO, DVec3::new(6.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 9.0)],
                Axis::new(DVec3::ZERO, DVec3::Z),
                std::f64::consts::TAU,
            ),
        ),
    ];
    for (name, s) in shapes {
        let Ok(s) = s else { continue };
        let Ok(mut b) = k.to_brep(&s) else { continue };
        let b = b.remove(0);
        for (f, ext) in [(Format::Text, "x_t"), (Format::Binary, "x_b")] {
            match shapecraft_xt::write_body(&b, f) {
                Ok(bytes) => {
                    let _ = std::fs::write(format!("{dir}/{name}.{ext}"), bytes);
                }
                Err(e) => eprintln!("{name}: {e}"),
            }
        }
        println!("{name}: volume {:.3}", k.mass_props(&s).map(|m| m.volume).unwrap_or(0.0));
    }
}

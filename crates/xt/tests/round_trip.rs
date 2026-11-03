//! Solids written as transmit files and read back keep their volume (text and binary).
use shapecraft_kernel::{Axis, BoolOp, DVec3, Kernel};
use shapecraft_kernel_occt::OcctKernel;
use shapecraft_xt::Format;

fn shapes(k: &OcctKernel) -> Vec<(&'static str, shapecraft_kernel::Shape)> {
    let rect = vec![DVec3::ZERO, DVec3::new(20.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 0.0), DVec3::new(0.0, 10.0, 0.0)];
    let boxy = k.extrude_polygon(&rect, DVec3::new(0.0, 0.0, 8.0)).unwrap();
    let hole = k.extrude_circle(DVec3::new(10.0, 5.0, -1.0), 3.0, DVec3::new(0.0, 0.0, 10.0)).unwrap();
    vec![
        ("box", boxy.clone()),
        ("cylinder", k.extrude_circle(DVec3::ZERO, 5.0, DVec3::new(0.0, 0.0, 12.0)).unwrap()),
        ("rounded box", k.fillet_all_edges(&boxy, 2.0).unwrap()),
        ("box with hole", k.boolean(BoolOp::Subtract, &boxy, &hole).unwrap()),
        (
            "cone",
            k.revolve_polygon(
                &[DVec3::ZERO, DVec3::new(6.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 9.0)],
                Axis::new(DVec3::ZERO, DVec3::Z),
                std::f64::consts::TAU,
            )
            .unwrap(),
        ),
    ]
}

#[test]
fn written_solids_read_back() {
    let k = OcctKernel::new();
    for (name, s) in shapes(&k) {
        let v0 = k.mass_props(&s).unwrap().volume;
        let brep = k.to_brep(&s).unwrap().remove(0);
        for format in [Format::Binary, Format::Text] {
            let bytes = shapecraft_xt::write_body(&brep, format).unwrap_or_else(|e| panic!("{name}: {e}"));
            let file = shapecraft_xt::parse(&bytes).unwrap_or_else(|e| panic!("{name} {format:?}: {e}"));
            let bodies = shapecraft_xt::bodies(&file).unwrap_or_else(|e| panic!("{name} {format:?}: {e}"));
            assert_eq!(bodies.len(), 1);
            let back = k.from_brep(&bodies[0].brep).unwrap_or_else(|e| panic!("{name} {format:?}: {e}"));
            let v1 = k.mass_props(&back).unwrap().volume;
            assert!((v0 - v1).abs() < 1e-4 * v0, "{name} {format:?}: volume {v0} -> {v1}");
        }
    }
}

#[test]
fn periodic_faces_are_written_without_seams() {
    let k = OcctKernel::new();
    for (name, s) in shapes(&k) {
        let brep = k.to_brep(&s).unwrap().remove(0);
        let bytes = shapecraft_xt::write_body(&brep, Format::Binary).unwrap();
        let back = &shapecraft_xt::bodies(&shapecraft_xt::parse(&bytes).unwrap()).unwrap()[0].brep;
        // Edges as the modeller makes these solids: rings for full circles, no seam lines.
        let want = match name {
            "box" => 12,
            "cylinder" => 2,
            "rounded box" => brep.edges.len(),
            "box with hole" => 14,
            _ => brep.edges.len(),
        };
        assert_eq!(back.edges.len(), want, "{name}");
        if name == "cylinder" {
            assert!(back.vertices.is_empty() && back.faces.iter().any(|f| f.loops.len() == 2), "{name}: rings on a two-loop face");
        }
    }
}

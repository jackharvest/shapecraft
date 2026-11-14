#![allow(clippy::unwrap_used)]

use super::*;
use shapecraft_kernel::SubShape;

fn k() -> OcctKernel {
    OcctKernel::new()
}

fn rect(w: f64, h: f64) -> Vec<DVec3> {
    vec![DVec3::ZERO, DVec3::new(w, 0.0, 0.0), DVec3::new(w, h, 0.0), DVec3::new(0.0, h, 0.0)]
}

#[test]
fn extruded_rectangle_has_exact_volume() {
    let s = k().extrude_polygon(&rect(12.0, 5.0), DVec3::new(0.0, 0.0, 10.0)).unwrap();
    let mp = k().mass_props(&s).unwrap();
    assert!((mp.volume - 600.0).abs() < 1e-6, "{}", mp.volume);
    assert!((mp.centroid - DVec3::new(6.0, 2.5, 5.0)).length() < 1e-6);
}

#[test]
fn subtract_cylinder_from_box() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let c = k().extrude_circle(DVec3::new(5.0, 5.0, -1.0), 2.0, DVec3::new(0.0, 0.0, 12.0)).unwrap();
    let r = k().boolean(BoolOp::Subtract, &b, &c).unwrap();
    let v = k().mass_props(&r).unwrap().volume;
    let expected = 1000.0 - std::f64::consts::PI * 4.0 * 10.0;
    assert!((v - expected).abs() < 1e-3, "{v} vs {expected}");
    // The hole's seam isn't drawn: 12 box edges + the two rims.
    assert_eq!(k().tessellate(&r, 0.05).unwrap().edges.len(), 14);
}

#[test]
fn fillet_removes_material() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let f = k().fillet_all_edges(&b, 1.0).unwrap();
    let v = k().mass_props(&f).unwrap().volume;
    assert!(v < 1000.0 && v > 950.0, "{v}");
}

#[test]
fn oversized_fillet_is_an_error_not_a_crash() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    assert!(k().fillet_all_edges(&b, 50.0).is_err());
}

#[test]
fn bad_input_is_rejected() {
    assert!(k().extrude_polygon(&rect(1.0, 1.0)[..2], DVec3::Z).is_err());
    assert!(k().extrude_polygon(&rect(f64::NAN, 1.0), DVec3::Z).is_err());
    assert!(k().make_box(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0)).is_err());
}

#[test]
fn tessellation_tags_triangles_with_faces() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let m = k().tessellate(&b, 0.05).unwrap();
    assert_eq!(m.triangle_count(), 12);
    assert_eq!(m.triangle_faces.len(), 12);
    assert_eq!(*m.triangle_faces.iter().max().unwrap(), 5);
    assert_eq!(m.edges.len(), 12);
}

#[test]
fn step_round_trip_keeps_volume() {
    let dir = std::env::temp_dir().join(format!("shapecraft-step-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("box.step");
    let b = k().make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0)).unwrap();
    k().write_step(&[b], &path).unwrap();
    let back = k().read_step(&path).unwrap();
    assert_eq!(back.len(), 1);
    let v = k().mass_props(&back[0]).unwrap().volume;
    assert!((v - 6000.0).abs() < 1e-6);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn push_pull_top_face() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let n = k().face_count(&b).unwrap();
    let top = (0..n).find(|&f| k().face_plane(&b, f).unwrap().is_some_and(|p| p.normal.z > 0.99)).unwrap();
    let up = k().extrude_face(&b, top, 5.0).unwrap();
    assert!((k().mass_props(&up).unwrap().volume - 1500.0).abs() < 1e-6);
    let down = k().extrude_face(&b, top, -4.0).unwrap();
    assert!((k().mass_props(&down).unwrap().volume - 600.0).abs() < 1e-6);
}

#[test]
fn fillet_one_edge() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let m = k().tessellate(&b, 0.1).unwrap();
    let f = k().fillet_edges(&b, &m.edge_ids[..1], 2.0).unwrap();
    let removed = 1000.0 - k().mass_props(&f).unwrap().volume;
    let expected = (4.0 - std::f64::consts::PI) * 10.0; // (r² - πr²/4) · length
    assert!((removed - expected).abs() < 1e-3, "{removed}");
    assert!(k().fillet_edges(&b, &[999], 1.0).is_err());
}

#[test]
fn translate_moves_bounds() {
    let b = k().make_box(DVec3::ZERO, DVec3::ONE).unwrap();
    let m = k().translate(&b, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    assert!((k().bounds(&m).unwrap().min.x - 5.0).abs() < 1e-6);
}

#[test]
fn shell_a_box_open_at_the_top() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let n = k().face_count(&b).unwrap();
    let top = (0..n).find(|&f| k().face_plane(&b, f).unwrap().is_some_and(|p| p.normal.z > 0.99)).unwrap();
    let s = k().shell(&b, &[top], 1.0).unwrap();
    // Outer 10³ minus the 8 × 8 × 9 cavity.
    assert!((k().mass_props(&s).unwrap().volume - (1000.0 - 576.0)).abs() < 1e-3);
    assert!(k().shell(&b, &[top], 20.0).is_err());
}

#[test]
fn chamfer_one_edge() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let m = k().tessellate(&b, 0.1).unwrap();
    let c = k().chamfer_edges(&b, &m.edge_ids[..1], 2.0).unwrap();
    assert!((1000.0 - k().mass_props(&c).unwrap().volume - 20.0).abs() < 1e-6);
}

#[test]
fn revolve_rectangle_into_a_tube() {
    // A 1 × 10 strip at radius 4–5 around Z, full turn: π(5² − 4²)·10.
    let pts = [DVec3::new(4.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 10.0), DVec3::new(4.0, 0.0, 10.0)];
    let t = k().revolve_polygon(&pts, Axis::new(DVec3::ZERO, DVec3::Z), std::f64::consts::TAU).unwrap();
    let v = k().mass_props(&t).unwrap().volume;
    assert!((v - std::f64::consts::PI * 9.0 * 10.0).abs() < 1e-3, "{v}");
}

#[test]
fn rotate_and_mirror_keep_volume() {
    let b = k().make_box(DVec3::new(2.0, 0.0, 0.0), DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let r = k().rotate(&b, Axis::new(DVec3::ZERO, DVec3::Z), std::f64::consts::FRAC_PI_2).unwrap();
    let bb = k().bounds(&r).unwrap();
    assert!((bb.max.y - 3.0).abs() < 1e-6 && bb.min.x < -1.9);
    let m = k().mirror(&b, DVec3::ZERO, DVec3::X).unwrap();
    assert!((k().bounds(&m).unwrap().max.x + 2.0).abs() < 1e-6);
    assert!((k().mass_props(&m).unwrap().volume - 6.0).abs() < 1e-9);
}

fn frame() -> PlaneFrame {
    PlaneFrame { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y }
}

fn rect_lines(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<PlanarCurve> {
    let p = |x, y| DVec3::new(x, y, 0.0);
    vec![
        PlanarCurve::Line { a: p(x0, y0), b: p(x1, y0) },
        PlanarCurve::Line { a: p(x1, y0), b: p(x1, y1) },
        PlanarCurve::Line { a: p(x1, y1), b: p(x0, y1) },
        PlanarCurve::Line { a: p(x0, y1), b: p(x0, y0) },
    ]
}

#[test]
fn a_circle_inside_a_rectangle_makes_a_ring_and_a_disc() {
    let mut curves = rect_lines(0.0, 0.0, 20.0, 10.0);
    curves.push(PlanarCurve::Circle { center: DVec3::new(10.0, 5.0, 0.0), radius: 3.0 });
    let regions = k().planar_regions(frame(), &curves).unwrap();
    assert_eq!(regions.len(), 2);
    let mut areas: Vec<f64> = regions.iter().map(|r| k().mass_props(r).unwrap().area).collect();
    areas.sort_by(f64::total_cmp);
    let disc = std::f64::consts::PI * 9.0;
    assert!((areas[0] - disc).abs() < 1e-6);
    assert!((areas[1] - (200.0 - disc)).abs() < 1e-6);
    // The ring extrudes to a plate with a hole.
    let ring = regions.iter().find(|r| k().mass_props(r).unwrap().area > 100.0).unwrap();
    let v = k().mass_props(&k().prism(ring, DVec3::new(0.0, 0.0, 2.0)).unwrap()).unwrap().volume;
    assert!((v - (200.0 - disc) * 2.0).abs() < 1e-6);
}

#[test]
fn a_flat_face_has_its_own_middle_as_centroid() {
    // A face has no volume; its centroid is the surface's, not the origin (region handles sit there).
    let regions = k().planar_regions(frame(), &rect_lines(10.0, 0.0, 20.0, 15.0)).unwrap();
    let mp = k().mass_props(&regions[0]).unwrap();
    assert!((mp.centroid - DVec3::new(15.0, 7.5, 0.0)).length() < 1e-6, "{:?}", mp.centroid);
}

#[test]
fn overlapping_rectangles_split_into_three_cells_and_open_lines_make_none() {
    let mut curves = rect_lines(0.0, 0.0, 10.0, 10.0);
    curves.extend(rect_lines(5.0, 5.0, 15.0, 15.0));
    assert_eq!(k().planar_regions(frame(), &curves).unwrap().len(), 3);
    let open = vec![PlanarCurve::Line { a: DVec3::ZERO, b: DVec3::X }];
    assert!(k().planar_regions(frame(), &open).unwrap().is_empty());
}

#[test]
fn an_arc_and_a_line_close_a_half_disc() {
    let curves = vec![
        PlanarCurve::Line { a: DVec3::new(-5.0, 0.0, 0.0), b: DVec3::new(5.0, 0.0, 0.0) },
        PlanarCurve::Arc { a: DVec3::new(5.0, 0.0, 0.0), mid: DVec3::new(0.0, 5.0, 0.0), b: DVec3::new(-5.0, 0.0, 0.0) },
    ];
    let r = k().planar_regions(frame(), &curves).unwrap();
    assert_eq!(r.len(), 1);
    assert!((k().mass_props(&r[0]).unwrap().area - std::f64::consts::PI * 12.5).abs() < 1e-6);
}

#[test]
fn measure_faces_edges_and_gaps() {
    let a = k().make_box(DVec3::ZERO, DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let m = k().tessellate(&a, 0.1).unwrap();
    let mut lengths: Vec<f64> = m.edge_ids.iter().map(|e| k().measure(&a, SubShape::Edge(*e)).unwrap().0).collect();
    lengths.sort_by(f64::total_cmp);
    assert!((lengths[0] - 10.0).abs() < 1e-9 && (lengths[11] - 30.0).abs() < 1e-9);
    let (area, _) = k().measure(&a, SubShape::Face(0)).unwrap();
    assert!([200.0, 300.0, 600.0].iter().any(|x| (area - x).abs() < 1e-9));
    let b = k().make_box(DVec3::new(15.0, 0.0, 0.0), DVec3::ONE).unwrap();
    let d = k().distance(&a, SubShape::Whole, &b, SubShape::Whole).unwrap();
    assert!((d.distance - 5.0).abs() < 1e-9);
    assert!(k().measure(&a, SubShape::Edge(99)).is_err());
}

#[test]
fn ellipse_and_closed_spline_regions() {
    let e = vec![PlanarCurve::Ellipse { center: DVec3::ZERO, major: DVec3::X, rx: 5.0, ry: 2.0 }];
    let r = k().planar_regions(frame(), &e).unwrap();
    assert_eq!(r.len(), 1);
    assert!((k().mass_props(&r[0]).unwrap().area - std::f64::consts::PI * 10.0).abs() < 1e-6);
    // A spline from one end of a line to the other closes a region.
    let s = vec![
        PlanarCurve::Line { a: DVec3::new(-5.0, 0.0, 0.0), b: DVec3::new(5.0, 0.0, 0.0) },
        PlanarCurve::Spline {
            points: vec![DVec3::new(5.0, 0.0, 0.0), DVec3::new(2.0, 4.0, 0.0), DVec3::new(-2.0, 3.0, 0.0), DVec3::new(-5.0, 0.0, 0.0)],
        },
    ];
    assert_eq!(k().planar_regions(frame(), &s).unwrap().len(), 1);
}

#[test]
fn scale_uniform_and_per_axis() {
    let a = k().make_box(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let b = k().scale(&a, DVec3::ZERO, DVec3::splat(2.0)).unwrap();
    assert!((k().mass_props(&b).unwrap().volume - 8000.0).abs() < 1e-6);
    let c = k().scale(&a, DVec3::splat(5.0), DVec3::new(2.0, 1.0, 0.5)).unwrap();
    let bb = k().bounds(&c).unwrap();
    assert!((bb.min.x + 5.0).abs() < 1e-6 && (bb.max.x - 15.0).abs() < 1e-6 && (bb.max.z - 7.5).abs() < 1e-6);
    assert!(k().scale(&a, DVec3::ZERO, DVec3::new(0.0, 1.0, 1.0)).is_err());
    assert!(k().scale(&a, DVec3::ZERO, DVec3::splat(f64::NAN)).is_err());
}

#[test]
fn offset_a_planar_and_a_curved_face() {
    let a = k().make_box(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let top = (0..k().face_count(&a).unwrap()).find(|f| k().face_plane(&a, *f).unwrap().is_some_and(|p| p.normal.z > 0.9)).unwrap();
    let b = k().offset_faces(&a, &[top], 2.0).unwrap();
    assert!((k().mass_props(&b).unwrap().volume - 1200.0).abs() < 1e-3);
    let c = k().offset_faces(&a, &[top], -3.0).unwrap();
    assert!((k().mass_props(&c).unwrap().volume - 700.0).abs() < 1e-3);
    // The side of a cylinder grows its radius.
    let cyl = k().extrude_circle(DVec3::ZERO, 5.0, DVec3::new(0.0, 0.0, 10.0)).unwrap();
    let side = (0..k().face_count(&cyl).unwrap()).find(|f| k().face_plane(&cyl, *f).unwrap().is_none()).unwrap();
    let fat = k().offset_faces(&cyl, &[side], 1.0).unwrap();
    let v = std::f64::consts::PI * 36.0 * 10.0;
    assert!((k().mass_props(&fat).unwrap().volume - v).abs() < 1e-2);
    assert!(k().offset_faces(&a, &[99], 1.0).is_err());
}

#[test]
fn split_a_box_in_two() {
    let a = k().make_box(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let parts = k().split_by_plane(&a, DVec3::new(3.0, 0.0, 0.0), DVec3::X).unwrap();
    assert_eq!(parts.len(), 2);
    let mut v: Vec<f64> = parts.iter().map(|p| k().mass_props(p).unwrap().volume).collect();
    v.sort_by(f64::total_cmp);
    assert!((v[0] - 300.0).abs() < 1e-6 && (v[1] - 700.0).abs() < 1e-6);
    assert!(k().split_by_plane(&a, DVec3::new(30.0, 0.0, 0.0), DVec3::X).is_err());
}

fn square_face(z: f64, half: f64) -> Shape {
    let f = PlaneFrame { origin: DVec3::new(0.0, 0.0, z), x: DVec3::X, y: DVec3::Y };
    let c = |x: f64, y: f64| DVec3::new(x, y, z);
    let curves = vec![
        PlanarCurve::Line { a: c(-half, -half), b: c(half, -half) },
        PlanarCurve::Line { a: c(half, -half), b: c(half, half) },
        PlanarCurve::Line { a: c(half, half), b: c(-half, half) },
        PlanarCurve::Line { a: c(-half, half), b: c(-half, -half) },
    ];
    k().planar_regions(f, &curves).unwrap().remove(0)
}

#[test]
fn loft_between_two_squares_is_a_frustum() {
    let s = k().loft(&[square_face(0.0, 5.0), square_face(10.0, 2.5)]).unwrap();
    // Frustum of a square pyramid: h/3 (A1 + A2 + sqrt(A1 A2)).
    let expected = 10.0 / 3.0 * (100.0 + 25.0 + 50.0);
    let v = k().mass_props(&s).unwrap().volume;
    assert!((v - expected).abs() < 1e-3, "{v} vs {expected}");
    assert!(k().loft(&[square_face(0.0, 5.0)]).is_err());
}

#[test]
fn loft_square_to_circle_and_body_face() {
    let f = PlaneFrame { origin: DVec3::new(0.0, 0.0, 8.0), x: DVec3::X, y: DVec3::Y };
    let disc = k().planar_regions(f, &[PlanarCurve::Circle { center: DVec3::new(0.0, 0.0, 8.0), radius: 3.0 }]).unwrap().remove(0);
    let s = k().loft(&[square_face(0.0, 5.0), disc]).unwrap();
    let v = k().mass_props(&s).unwrap().volume;
    assert!(v > std::f64::consts::PI * 9.0 * 8.0 && v < 800.0, "{v}");
    // A body's face works as a profile too.
    let b = k().make_box(DVec3::new(-5.0, -5.0, -4.0), DVec3::new(10.0, 10.0, 4.0)).unwrap();
    let top = (0..k().face_count(&b).unwrap()).find(|&i| k().face_plane(&b, i).unwrap().is_some_and(|p| p.normal.z > 0.9)).unwrap();
    let top = k().face(&b, top).unwrap();
    assert!(k().loft(&[top, square_face(6.0, 2.0)]).is_ok());
}

#[test]
fn sweep_a_disc_along_an_l_path() {
    let disc = k().planar_regions(frame(), &[PlanarCurve::Circle { center: DVec3::ZERO, radius: 1.0 }]).unwrap().remove(0);
    let path_frame = PlaneFrame { origin: DVec3::ZERO, x: DVec3::Z, y: DVec3::X };
    let path = vec![
        PlanarCurve::Line { a: DVec3::ZERO, b: DVec3::new(0.0, 0.0, 10.0) },
        PlanarCurve::Arc {
            a: DVec3::new(0.0, 0.0, 10.0),
            mid: DVec3::new(5.0 - 5.0 * std::f64::consts::FRAC_1_SQRT_2, 0.0, 10.0 + 5.0 * std::f64::consts::FRAC_1_SQRT_2),
            b: DVec3::new(5.0, 0.0, 15.0),
        },
    ];
    let s = k().sweep(&disc, path_frame, &path).unwrap();
    let length = 10.0 + std::f64::consts::PI * 5.0 / 2.0;
    let expected = std::f64::consts::PI * length;
    let v = k().mass_props(&s).unwrap().volume;
    assert!((v - expected).abs() / expected < 1e-3, "{v} vs {expected}");
    let gap = vec![
        PlanarCurve::Line { a: DVec3::ZERO, b: DVec3::Z },
        PlanarCurve::Line { a: DVec3::new(0.0, 0.0, 3.0), b: DVec3::new(0.0, 0.0, 4.0) },
    ];
    assert!(k().sweep(&disc, path_frame, &gap).is_err());
}

#[test]
fn delete_face_closes_a_hole_and_refuses_a_cap() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let c = k().extrude_circle(DVec3::new(5.0, 5.0, -1.0), 2.0, DVec3::new(0.0, 0.0, 12.0)).unwrap();
    let holed = k().boolean(BoolOp::Subtract, &b, &c).unwrap();
    // The hole's wall is the one curved face; taking it out fills the hole.
    let wall = (0..k().face_count(&holed).unwrap()).find(|&f| k().face_plane(&holed, f).unwrap().is_none()).unwrap();
    let filled = k().remove_faces(&holed, &[wall]).unwrap();
    assert!((k().mass_props(&filled).unwrap().volume - 1000.0).abs() < 1e-3);
    // A cylinder's cap can't be closed over: refused, not handed back unchanged.
    let cyl = k().extrude_circle(DVec3::ZERO, 2.0, DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let cap = (0..k().face_count(&cyl).unwrap()).find(|&f| k().face_plane(&cyl, f).unwrap().is_some()).unwrap();
    assert!(k().remove_faces(&cyl, &[cap]).is_err());
}

#[test]
fn sweep_with_twist_scale_and_orientation() {
    let path_frame = PlaneFrame { origin: DVec3::ZERO, x: DVec3::Z, y: DVec3::X };
    let path = vec![PlanarCurve::Line { a: DVec3::ZERO, b: DVec3::new(0.0, 0.0, 10.0) }];
    let vol = |o: SweepOptions| k().mass_props(&k().sweep_with(&square_face(0.0, 1.0), path_frame, &path, &o).unwrap()).unwrap().volume;
    // A 2 × 2 square: twisting keeps the volume (40); growing to twice the size makes a frustum (10/3 · (4 + 16 + 8)).
    let twisted = vol(SweepOptions { twist: 90.0, ..Default::default() });
    assert!((twisted - 40.0).abs() < 0.5, "{twisted}");
    let grown = vol(SweepOptions { scale: 2.0, ..Default::default() });
    assert!((grown - 280.0 / 3.0).abs() < 0.5, "{grown}");
    let parallel = vol(SweepOptions { parallel: true, round: true, ..Default::default() });
    assert!((parallel - 40.0).abs() < 1e-3, "{parallel}");
    assert!(k().sweep_with(&square_face(0.0, 1.0), path_frame, &path, &SweepOptions { scale: f64::NAN, ..Default::default() }).is_err());
}

#[test]
fn axes_of_edges_and_faces() {
    let c = k().extrude_circle(DVec3::new(1.0, 2.0, 0.0), 3.0, DVec3::new(0.0, 0.0, 5.0)).unwrap();
    let side = (0..k().face_count(&c).unwrap()).find(|&i| k().face_plane(&c, i).unwrap().is_none()).unwrap();
    let ax = k().axis_of(&c, SubShape::Face(side)).unwrap().unwrap();
    assert!(ax.dir.normalize().cross(DVec3::Z).length() < 1e-9);
    assert!((DVec3::new(ax.origin.x, ax.origin.y, 0.0) - DVec3::new(1.0, 2.0, 0.0)).length() < 1e-9);
    let b = k().make_box(DVec3::ZERO, DVec3::splat(2.0)).unwrap();
    let e = k().axis_of(&b, SubShape::Edge(0)).unwrap().unwrap();
    assert!((e.dir.length() - 1.0).abs() < 1e-9 && e.dir.abs().max_element() > 0.999);
    assert!(k().axis_of(&b, SubShape::Edge(9999)).is_err());
}

#[test]
fn a_box_face_has_four_edges() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(2.0)).unwrap();
    let e = k().face_edges(&b, 0).unwrap();
    assert_eq!(e.len(), 4);
    assert!(e.iter().all(|i| *i < 12));
    assert!(k().face_edges(&b, 99).is_err());
}

#[test]
fn drafted_extrude_tapers() {
    let sq = square_face(0.0, 5.0);
    let straight = k().mass_props(&k().prism_draft(&sq, DVec3::new(0.0, 0.0, 10.0), 0.0).unwrap()).unwrap().volume;
    assert!((straight - 1000.0).abs() < 1e-3, "{straight}");
    let tapered = k().prism_draft(&sq, DVec3::new(0.0, 0.0, 10.0), 10f64.to_radians()).unwrap();
    let v = k().mass_props(&tapered).unwrap().volume;
    assert!(v < 1000.0 && v > 500.0, "{v}");
    let bb = k().bounds(&tapered).unwrap();
    assert!((bb.max.z - 10.0).abs() < 1e-6 && bb.min.z.abs() < 1e-6, "{bb:?}");
    let down = k().bounds(&k().prism_draft(&sq, DVec3::new(0.0, 0.0, -10.0), 0.0).unwrap()).unwrap();
    assert!((down.min.z + 10.0).abs() < 1e-6, "{:?}", down);
    // A ring keeps its hole.
    let f = PlaneFrame { origin: DVec3::ZERO, x: DVec3::X, y: DVec3::Y };
    let c = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let ring = k()
        .planar_regions(
            f,
            &[
                PlanarCurve::Line { a: c(-5.0, -5.0), b: c(5.0, -5.0) },
                PlanarCurve::Line { a: c(5.0, -5.0), b: c(5.0, 5.0) },
                PlanarCurve::Line { a: c(5.0, 5.0), b: c(-5.0, 5.0) },
                PlanarCurve::Line { a: c(-5.0, 5.0), b: c(-5.0, -5.0) },
                PlanarCurve::Circle { center: DVec3::ZERO, radius: 2.0 },
            ],
        )
        .unwrap()
        .into_iter()
        .max_by(|a, b| k().mass_props(a).unwrap().area.total_cmp(&k().mass_props(b).unwrap().area))
        .unwrap();
    let v = k().mass_props(&k().prism_draft(&ring, DVec3::new(0.0, 0.0, 4.0), 5f64.to_radians()).unwrap()).unwrap().volume;
    assert!(v < 400.0 && v > 300.0, "{v}");
    assert!(k().prism_draft(&sq, DVec3::new(1.0, 0.0, 1.0), 0.1).is_err(), "only straight out");
}

#[test]
fn chain_and_outline_offsets() {
    // An L of two lines, offset 1 to the inside corner: still two lines, joined.
    let l = vec![
        PlanarCurve::Line { a: DVec3::new(0.0, 10.0, 0.0), b: DVec3::ZERO },
        PlanarCurve::Line { a: DVec3::ZERO, b: DVec3::new(10.0, 0.0, 0.0) },
    ];
    let o = k().offset_curves(frame(), &l, 1.0).unwrap();
    let n = k().edge_count(&o).unwrap();
    let curves: Vec<_> = (0..n).map(|i| k().edge_curve(&o, i).unwrap()).collect();
    assert_eq!(n, 2, "{curves:?}");
    assert!(curves.iter().any(|c| matches!(c, EdgeCurve::Line { b, .. } if b.distance(DVec3::new(1.0, 1.0, 0.0)) < 1e-9)), "{curves:?}");
    let sq = square_face(0.0, 5.0);
    let grown = k().offset_face_outline(&sq, 1.0).unwrap();
    assert_eq!(k().edge_count(&grown).unwrap(), 4, "sharp corners: still four sides");
    let lens: f64 = (0..4).map(|i| k().measure(&grown, SubShape::Edge(i)).unwrap().0).sum();
    assert!((lens - 48.0).abs() < 1e-6, "{lens}");
}

#[test]
fn non_uniform_scale_remeshes() {
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let _ = k().tessellate(&b, 0.02).unwrap();
    let s = k().scale(&b, DVec3::ZERO, DVec3::new(2.0, 1.0, 1.0)).unwrap();
    let m = k().tessellate(&s, 0.02).unwrap();
    let xmax = m.positions.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    assert!((xmax - 20.0).abs() < 1e-3, "display mesh follows the stretch: {xmax}");
}

#[test]
fn front_view_shows_a_hole_as_hidden_lines() {
    let b = k().make_box(DVec3::ZERO, DVec3::new(20.0, 10.0, 5.0)).unwrap();
    let front = ViewFrame { origin: DVec3::ZERO, toward: DVec3::new(0.0, -1.0, 0.0), x: DVec3::X };
    let lines = k().hidden_lines(std::slice::from_ref(&b), front, 0.05).unwrap();
    assert!(lines.iter().any(|l| !l.hidden));
    let (lo, hi) = lines
        .iter()
        .flat_map(|l| l.points.iter())
        .fold((shapecraft_kernel::DVec2::splat(f64::MAX), shapecraft_kernel::DVec2::splat(f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    assert!((hi - lo - shapecraft_kernel::DVec2::new(20.0, 5.0)).length() < 1e-6, "{lo} {hi}");
    let hole = k().extrude_circle(DVec3::new(10.0, 5.0, -1.0), 2.0, DVec3::new(0.0, 0.0, 7.0)).unwrap();
    let holed = k().boolean(BoolOp::Subtract, &b, &hole).unwrap();
    // The hole's two sides show up dashed, at x = 8 and 12.
    let hidden: Vec<_> = k().hidden_lines(&[holed], front, 0.05).unwrap().into_iter().filter(|l| l.hidden).collect();
    for x in [8.0, 12.0] {
        assert!(hidden.iter().any(|l| l.points.iter().all(|p| (p.x - x).abs() < 1e-6)), "no dashed line at x = {x}: {hidden:?}");
    }
}

/// A shape described as a B-rep and rebuilt keeps its volume and stays one valid solid (curved faces, seams,
/// degenerate apex edges, left-handed frames).
#[test]
fn a_lemon_torus_keeps_its_signed_radius_and_surface_parameters() {
    use shapecraft_kernel::DVec2;
    use shapecraft_kernel::brep::{BodyKind, Brep, Curve, Edge, Face, Fin, Nurbs, Surface};
    // An exact rectangular patch, drawn here from the published torus equation, not from an imported file.
    // Its area integral is minor * (major * dv + minor * (sin(v1)-sin(v0))) * du.
    let (major, minor) = (-2.0, 5.0);
    let du = std::f64::consts::FRAC_PI_2;
    let v: f64 = 0.7;
    let uv = [DVec2::new(0.0, -v), DVec2::new(du, -v), DVec2::new(du, v), DVec2::new(0.0, v)];
    let point = |p: DVec2| DVec3::new((major + minor * p.y.cos()) * p.x.cos(), (major + minor * p.y.cos()) * p.x.sin(), minor * p.y.sin());
    let mut b = Brep {
        kind: BodyKind::Sheet,
        surfaces: vec![Surface::Torus { center: DVec3::ZERO, axis: DVec3::Z, x: DVec3::X, major, minor }],
        vertices: uv.iter().copied().map(point).collect(),
        ..Brep::default()
    };
    for i in 0..4 {
        b.curves.push(Curve::OnSurface {
            surface: 0,
            uv: Nurbs {
                degree: 1,
                poles: vec![uv[i], uv[(i + 1) % 4]],
                weights: None,
                knots: vec![0.0, 1.0],
                mults: vec![2, 2],
                periodic: false,
            },
        });
        b.edges.push(Edge { curve: Some(i), reversed: false, start: Some(i), end: Some((i + 1) % 4), tolerance: None });
    }
    b.faces.push(Face {
        surface: 0,
        flip: false,
        loops: vec![(0..4).map(|i| Fin { edge: i, forward: true, pcurve: Some(i) }).collect()],
        color: None,
    });
    b.shells.push(vec![0]);
    let kernel = k();
    let shape = kernel.from_brep(&b).unwrap();
    let area = minor * (major * (2.0 * v) + minor * (2.0 * v.sin())) * du;
    assert!((kernel.mass_props(&shape).unwrap().area - area).abs() < 1e-5);
    let mesh = kernel.tessellate(&shape, 0.02).unwrap();
    assert!(!mesh.indices.is_empty());
    let back = kernel.to_brep(&shape).unwrap().remove(0);
    assert!(
        back.surfaces
            .iter()
            .any(|s| matches!(s, Surface::Torus { major: r, minor: t, .. } if (*r-major).abs()<1e-9 && (*t-minor).abs()<1e-9))
    );
    let rebuilt = kernel.from_brep(&back).unwrap();
    assert!((kernel.mass_props(&rebuilt).unwrap().area - area).abs() < 1e-5);
    for major in [0.0, -minor, -minor - 1.0] {
        b.surfaces[0] = Surface::Torus { center: DVec3::ZERO, axis: DVec3::Z, x: DVec3::X, major, minor };
        assert!(kernel.from_brep(&b).is_err());
    }
}

#[test]
fn unused_offset_of_a_creased_surface_does_not_block_a_solid() {
    use shapecraft_kernel::brep::{NurbsSurface, Surface};
    let kernel = k();
    let solid = kernel.make_box(DVec3::ZERO, DVec3::splat(4.0)).unwrap();
    let mut brep = kernel.to_brep(&solid).unwrap().remove(0);
    let mut poles = Vec::new();
    // Two quadratic patches meet with a sharp crease at u = 0.5.
    for (x, z) in [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 1.0), (4.0, 2.0)] {
        for y in [0.0, 1.0] {
            poles.push(DVec3::new(x, y, z));
        }
    }
    let basis = brep.surfaces.len();
    brep.surfaces.push(Surface::BSpline(NurbsSurface {
        u_degree: 2,
        v_degree: 1,
        nu: 5,
        nv: 2,
        poles,
        weights: None,
        u_knots: vec![0.0, 0.5, 1.0],
        u_mults: vec![3, 2, 3],
        v_knots: vec![0.0, 1.0],
        v_mults: vec![2, 2],
        u_periodic: false,
        v_periodic: false,
    }));
    brep.surfaces.push(Surface::Offset { basis, distance: 0.25 });
    let rebuilt = kernel.from_brep(&brep).unwrap();
    assert!((kernel.mass_props(&rebuilt).unwrap().volume - 64.0).abs() < 1e-8);
    // An actually used offset of this crease has no unique normal and is rejected.
    brep.faces[0].surface = basis + 1;
    assert!(kernel.from_brep(&brep).is_err());
}

#[test]
fn used_offset_accepts_smooth_rational_joins_but_rejects_creases() {
    use shapecraft_kernel::DVec2;
    use shapecraft_kernel::brep::{BodyKind, Brep, Curve, Edge, Face, Fin, Nurbs, NurbsSurface, Surface};
    // Two rational quarter circles form a smooth half-cylinder, but the
    // double quadratic knot declares only C0 continuity.
    let q = std::f64::consts::FRAC_1_SQRT_2;
    let arc = [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];
    let mut poles = Vec::new();
    for z in [0.0, 2.0] {
        poles.extend(arc.iter().map(|&(x, y)| DVec3::new(x, y, z)));
    }
    let basis = NurbsSurface {
        u_degree: 1,
        v_degree: 2,
        nu: 2,
        nv: 5,
        poles,
        weights: Some(vec![1.0, q, 1.0, q, 1.0, 1.0, q, 1.0, q, 1.0]),
        u_knots: vec![0.0, 1.0],
        u_mults: vec![2, 2],
        v_knots: vec![0.0, 0.5, 1.0],
        v_mults: vec![3, 2, 3],
        u_periodic: false,
        v_periodic: false,
    };
    let uv = [DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(1.0, 1.0), DVec2::new(0.0, 1.0)];
    let points = [DVec3::new(0.75, 0.0, 0.0), DVec3::new(0.75, 0.0, 2.0), DVec3::new(-0.75, 0.0, 2.0), DVec3::new(-0.75, 0.0, 0.0)];
    let mut b = Brep {
        kind: BodyKind::Sheet,
        surfaces: vec![Surface::BSpline(basis), Surface::Offset { basis: 0, distance: 0.25 }],
        vertices: points.to_vec(),
        ..Brep::default()
    };
    for i in 0..4 {
        b.curves.push(Curve::OnSurface {
            surface: 1,
            uv: Nurbs {
                degree: 1,
                poles: vec![uv[i], uv[(i + 1) % 4]],
                weights: None,
                knots: vec![0.0, 1.0],
                mults: vec![2, 2],
                periodic: false,
            },
        });
        b.edges.push(Edge { curve: Some(i), reversed: false, start: Some(i), end: Some((i + 1) % 4), tolerance: None });
    }
    b.faces.push(Face {
        surface: 1,
        flip: false,
        loops: vec![(0..4).map(|i| Fin { edge: i, forward: true, pcurve: Some(i) }).collect()],
        color: None,
    });
    b.shells.push(vec![0]);
    let kernel = k();
    let shape = kernel.from_brep(&b).unwrap();
    let area = kernel.mass_props(&shape).unwrap().area;
    // OCCT mass integration is approximate for this offset surface.
    assert!((area - 1.5 * std::f64::consts::PI).abs() < 0.005, "{area}");
    assert!(!kernel.tessellate(&shape, 0.02).unwrap().indices.is_empty());
    // Keep an independent translated case: the continuity test must not lose
    // its ability to detect the same crease far from the global origin.
    let mut translated = b.clone();
    let shift = DVec3::new(1.0e10, -1.0e10, 1.0e10);
    for p in &mut translated.vertices {
        *p += shift;
    }
    if let Surface::BSpline(ref mut n) = translated.surfaces[0] {
        for p in &mut n.poles {
            *p += shift;
        }
    }
    kernel.from_brep(&translated).unwrap();
    for model in [&mut b, &mut translated] {
        if let Surface::BSpline(ref mut n) = model.surfaces[0] {
            n.poles[3].y += 0.001;
        }
        let err = kernel.from_brep(model).unwrap_err().to_string();
        assert!(err.contains("offset of a C0 surface"), "{err}");
    }
}

#[test]
fn brep_round_trip_keeps_curved_solids() {
    let k = k();
    let cyl = k.extrude_circle(DVec3::ZERO, 5.0, DVec3::new(0.0, 0.0, 12.0)).unwrap();
    let boxy = k.extrude_polygon(&rect(20.0, 10.0), DVec3::new(0.0, 0.0, 8.0)).unwrap();
    let rounded = k.fillet_all_edges(&boxy, 2.0).unwrap();
    let cone = k
        .revolve_polygon(
            &[DVec3::new(0.0, 0.0, 0.0), DVec3::new(6.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 9.0)],
            Axis::new(DVec3::ZERO, DVec3::Z),
            std::f64::consts::TAU,
        )
        .unwrap();
    let holed = k
        .boolean(BoolOp::Subtract, &boxy, &k.extrude_circle(DVec3::new(10.0, 5.0, -1.0), 3.0, DVec3::new(0.0, 0.0, 10.0)).unwrap())
        .unwrap();
    for (name, s) in [("cylinder", cyl), ("rounded box", rounded), ("cone", cone), ("box with hole", holed)] {
        let v0 = k.mass_props(&s).unwrap().volume;
        let breps = k.to_brep(&s).unwrap();
        assert_eq!(breps.len(), 1, "{name}");
        let back = k.from_brep(&breps[0]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let v1 = k.mass_props(&back).unwrap().volume;
        assert!((v0 - v1).abs() < 1e-4 * v0, "{name}: volume {v0} -> {v1}");
        assert_eq!(k.solids(&back).unwrap().len(), 1, "{name}");
    }
}

/// An edge given only as curves in its faces' (u, v) (a tolerant edge as Parasolid files carry them, one curve per
/// fin) rebuilds into the same solid, with its vertex a little off the curve's end.
#[test]
fn brep_edge_from_curves_in_its_faces() {
    use shapecraft_kernel::brep::{Curve, Nurbs, Surface};
    let k = k();
    let boxy = k.extrude_polygon(&rect(20.0, 10.0), DVec3::new(0.0, 0.0, 8.0)).unwrap();
    let v0 = k.mass_props(&boxy).unwrap().volume;
    let mut b = k.to_brep(&boxy).unwrap().remove(0);
    // A straight edge between two planar faces.
    let e = (0..b.edges.len()).find(|&e| matches!(b.edges[e].curve.map(|c| &b.curves[c]), Some(Curve::Line { .. }))).unwrap();
    let (s, t) = (b.vertices[b.edges[e].start.unwrap()], b.vertices[b.edges[e].end.unwrap()]);
    let uv = |surface: usize, p: DVec3| match &b.surfaces[surface] {
        Surface::Plane { origin, normal, x } => {
            let (n, x) = (normal.normalize(), x.normalize());
            let x = (x - n * x.dot(n)).normalize();
            let d = p - *origin;
            Some(shapecraft_kernel::DVec2::new(d.dot(x), d.dot(n.cross(x))))
        }
        _ => None,
    };
    let faces: Vec<(usize, usize, usize)> = b
        .faces
        .iter()
        .enumerate()
        .flat_map(|(f, face)| {
            face.loops.iter().enumerate().flat_map(move |(l, lp)| lp.iter().enumerate().map(move |(i, fin)| (f, l, i, fin.edge)))
        })
        .filter(|x| x.3 == e)
        .map(|(f, l, i, _)| (f, l, i))
        .collect();
    assert_eq!(faces.len(), 2);
    // The parameters run over [0, 3] rather than the edge's length, and the end overshoots a little.
    let line = |surface: usize| Curve::OnSurface {
        surface,
        uv: Nurbs {
            degree: 1,
            poles: vec![uv(surface, s).unwrap(), uv(surface, t + (t - s) * 1e-6).unwrap()],
            weights: None,
            knots: vec![0.0, 3.0],
            mults: vec![2, 2],
            periodic: false,
        },
    };
    let (fa, la, ia) = faces[0];
    let (fb, lb, ib) = faces[1];
    b.curves.push(line(b.faces[fa].surface));
    b.edges[e].curve = Some(b.curves.len() - 1);
    b.edges[e].reversed = false;
    b.edges[e].tolerance = Some(1e-4);
    b.curves.push(line(b.faces[fb].surface));
    b.faces[fb].loops[lb][ib].pcurve = Some(b.curves.len() - 1);
    b.faces[fa].loops[la][ia].pcurve = Some(b.curves.len() - 2);
    let back = k.from_brep(&b).unwrap();
    let v1 = k.mass_props(&back).unwrap().volume;
    assert!((v0 - v1).abs() < 1e-4 * v0, "volume {v0} -> {v1}");
    assert_eq!(k.solids(&back).unwrap().len(), 1);
}

#[test]
fn a_step_file_written_per_body_reads_back_one_shape_per_body() {
    let k = k();
    let a = k.make_box(DVec3::ZERO, DVec3::splat(2.0)).unwrap();
    let b = k.make_box(DVec3::splat(10.0), DVec3::splat(2.0)).unwrap();
    let c = k.make_box(DVec3::splat(-10.0), DVec3::splat(3.0)).unwrap();
    // A surface body, one body of two separate pieces, and a plain solid. Read solid by solid (as an import is),
    // the surface would go and the pair would come back as two.
    let sheet = k.face(&c, 0).unwrap();
    let pair = k.boolean(shapecraft_kernel::BoolOp::Union, &a, &b).unwrap();
    let solo = k.make_box(DVec3::new(20.0, 0.0, 0.0), DVec3::splat(2.0)).unwrap();
    let path = std::env::temp_dir().join(format!("sc-step-roots-{}.step", std::process::id()));
    k.write_step(&[sheet, pair, solo], &path).unwrap();
    let back = k.read_step_roots(&path).unwrap();
    assert_eq!(k.read_step(&path).unwrap().len(), 3);
    let _ = std::fs::remove_file(&path);
    assert_eq!(back.len(), 3);
    assert_eq!(k.face_count(&back[0]).unwrap(), 1);
    assert!((k.mass_props(&back[1]).unwrap().volume - 16.0).abs() < 1e-6);
    assert!((k.mass_props(&back[2]).unwrap().volume - 8.0).abs() < 1e-6);
}

#[test]
fn stretching_again_and_again_keeps_every_part_whole() {
    // A second one-axis stretch of a cylinder used to crash inside OCCT's NURBS conversion (a null curve on the
    // seam, no exception to catch), and filleted or chamfered rims crashed on the first one.
    let kk = k();
    let cyl = kk.extrude_circle(DVec3::ZERO, 9.0, DVec3::new(0.0, 0.0, 19.0)).unwrap();
    let rims: Vec<u32> = (0..kk.edge_count(&cyl).unwrap()).collect();
    let boxed = kk.make_box(DVec3::ZERO, DVec3::new(20.0, 12.0, 8.0)).unwrap();
    let hole = kk.extrude_circle(DVec3::new(10.0, 6.0, -1.0), 3.0, DVec3::new(0.0, 0.0, 10.0)).unwrap();
    let parts = [
        cyl.clone(),
        kk.fillet_edges(&cyl, &rims, 1.6).unwrap(),
        kk.chamfer_edges(&cyl, &rims, 1.2).unwrap(),
        kk.fillet_all_edges(&boxed, 1.0).unwrap(),
        kk.boolean(BoolOp::Subtract, &boxed, &hole).unwrap(),
        kk.shell(&cyl, &[1], 1.0).unwrap(),
    ];
    for (n, part) in parts.iter().enumerate() {
        let v0 = kk.mass_props(part).unwrap().volume;
        let mut s = part.clone();
        let mut grow = 1.0;
        for f in [DVec3::new(1.3, 1.0, 1.0), DVec3::new(1.0, 1.0, 1.25), DVec3::new(1.0, 0.8, 1.0), DVec3::new(1.2, 1.2, 1.0)] {
            s = kk.scale(&s, DVec3::new(1.0, 2.0, 9.0), f).unwrap_or_else(|e| panic!("part {n} {f}: {e}"));
            grow *= f.x * f.y * f.z;
            let v = kk.mass_props(&s).unwrap().volume;
            assert!((v / (v0 * grow) - 1.0).abs() < 1e-4, "part {n} after {f}: {v} vs {}", v0 * grow);
            assert!(!kk.tessellate(&s, 0.05).unwrap().positions.is_empty());
        }
        // A uniform scale after the stretches still works on the B-spline part.
        let u = kk.scale(&s, DVec3::ZERO, DVec3::splat(0.5)).unwrap();
        assert!((kk.mass_props(&u).unwrap().volume / (v0 * grow * 0.125) - 1.0).abs() < 1e-4, "part {n} uniform");
    }
}

#[test]
#[ignore]
// Slow (about 20 s): every face and edge operation on stretched (B-spline) parts, looking for hard crashes.
fn sweep_every_op_on_stretched_parts() {
    let kk = k();
    let cyl = kk.extrude_circle(DVec3::ZERO, 9.0, DVec3::new(0.0, 0.0, 19.0)).unwrap();
    let rims: Vec<u32> = (0..kk.edge_count(&cyl).unwrap()).collect();
    let fil = kk.fillet_edges(&cyl, &rims, 1.6).unwrap();
    let bx = kk.make_box(DVec3::ZERO, DVec3::new(20.0, 12.0, 8.0)).unwrap();
    let st = |s: &Shape| {
        kk.scale(&kk.scale(s, DVec3::ZERO, DVec3::new(1.3, 1.0, 1.0)).unwrap(), DVec3::ZERO, DVec3::new(1.0, 1.0, 1.2)).unwrap()
    };
    let dir = std::env::temp_dir();
    for (name, s) in [("cyl", st(&cyl)), ("fil", st(&fil)), ("box", st(&bx))] {
        let nf = kk.face_count(&s).unwrap();
        let ne = kk.edge_count(&s).unwrap();
        eprintln!("{name}: {nf} faces {ne} edges");
        for f in 0..nf {
            for d in [2.0, -0.5] {
                eprintln!("{name} face {f} extrude {d}");
                let _ = kk.extrude_face(&s, f, d);
                eprintln!("{name} face {f} offset {d}");
                let _ = kk.offset_faces(&s, &[f], d);
            }
            eprintln!("{name} face {f} shell");
            let _ = kk.shell(&s, &[f], 0.8);
            eprintln!("{name} face {f} remove/plane/axis/blend/measure/face");
            let _ = kk.remove_faces(&s, &[f]);
            let _ = kk.face_plane(&s, f);
            let _ = kk.axis_of(&s, SubShape::Face(f));
            let _ = kk.blend_of(&s, f);
            let _ = kk.measure(&s, SubShape::Face(f));
            let _ = kk.face_edges(&s, f);
            let _ = kk.face(&s, f);
        }
        for e in 0..ne {
            eprintln!("{name} edge {e} fillet/chamfer/curve/axis");
            let _ = kk.fillet_edges(&s, &[e], 0.8);
            let _ = kk.chamfer_edges(&s, &[e], 0.8);
            let _ = kk.fillet_edges_variable(&s, &[e], 0.5, 1.0);
            let _ = kk.edge_curve(&s, e);
            let _ = kk.axis_of(&s, SubShape::Edge(e));
            let _ = kk.measure(&s, SubShape::Edge(e));
            let _ = kk.fill(&s, &[e]);
        }
        eprintln!("{name} whole-body ops");
        let _ = kk.fillet_all_edges(&s, 0.5);
        let _ = kk.split_by_plane(&s, DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
        let _ = kk.split_by_plane(&s, DVec3::new(1.0, 0.0, 0.0), DVec3::X);
        for op in [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect] {
            let _ = kk.boolean(op, &s, &bx);
            let _ = kk.boolean(op, &bx, &s);
        }
        let _ = kk.mirror(&s, DVec3::ZERO, DVec3::X);
        let _ = kk.rotate(&s, Axis::new(DVec3::ZERO, DVec3::Z), 0.7);
        let _ = kk.translate(&s, DVec3::ONE);
        let _ = kk.scale(&s, DVec3::ZERO, DVec3::splat(2.0));
        let _ = kk.mass_props(&s);
        let _ = kk.bounds(&s);
        let _ = kk.hidden_lines(
            std::slice::from_ref(&s),
            ViewFrame { origin: DVec3::ZERO, toward: DVec3::new(0.0, -1.0, 0.0), x: DVec3::X },
            0.05,
        );
        let p = dir.join(format!("sc-sweep-{name}.step"));
        kk.write_step(std::slice::from_ref(&s), &p).unwrap();
        let back = kk.read_step(&p).unwrap();
        let _ = kk.scale(&back[0], DVec3::ZERO, DVec3::new(1.0, 1.4, 1.0)).unwrap();
        let _ = std::fs::remove_file(&p);
    }
    eprintln!("sweep done");
}

#[test]
fn scaling_a_rim_tapers_the_cylinder() {
    let kk = k();
    let cyl = kk.extrude_circle(DVec3::ZERO, 4.5, DVec3::new(0.0, 0.0, 19.0)).unwrap();
    let top = (0..kk.edge_count(&cyl).unwrap())
        .find(|e| matches!(kk.edge_curve(&cyl, *e), Ok(EdgeCurve::Circle { center, .. }) if center.z > 18.0))
        .unwrap();
    let c = DVec3::new(0.0, 0.0, 19.0);
    // Uniform ×2: a cone frustum r 4.5 → 9 over 19 mm.
    let up = kk.scale_edge(&cyl, top, c, DVec3::splat(2.0)).unwrap();
    let v = kk.mass_props(&up).unwrap().volume;
    let frustum = std::f64::consts::PI * 19.0 / 3.0 * (4.5f64.powi(2) + 4.5 * 9.0 + 9.0f64.powi(2));
    assert!((v / frustum - 1.0).abs() < 1e-3, "{v} vs {frustum}");
    let bb = kk.bounds(&up).unwrap();
    assert!((bb.max.x - 9.0).abs() < 1e-3 && (bb.max.z - 19.0).abs() < 1e-3, "{bb:?}");
    // One axis: an oval top, the bottom untouched.
    let oval = kk.scale_edge(&cyl, top, c, DVec3::new(1.8, 1.0, 1.0)).unwrap();
    let bb = kk.bounds(&oval).unwrap();
    assert!((bb.max.x - 8.1).abs() < 1e-2 && (bb.max.y - 4.5).abs() < 1e-2, "{bb:?}");
    assert!(kk.mass_props(&oval).unwrap().volume > std::f64::consts::PI * 4.5 * 4.5 * 19.0);
    // And again on the result (a B-spline rim now).
    let top2 = (0..kk.edge_count(&oval).unwrap())
        .max_by(|a, b| kk.measure(&oval, SubShape::Edge(*a)).unwrap().1.z.total_cmp(&kk.measure(&oval, SubShape::Edge(*b)).unwrap().1.z))
        .unwrap();
    assert!(kk.scale_edge(&oval, top2, c, DVec3::new(1.0, 1.5, 1.0)).is_ok());
    // Moved: up makes it taller, sideways slants it (same volume).
    let base = std::f64::consts::PI * 4.5 * 4.5 * 19.0;
    let up = kk.map_edge(&cyl, top, shapecraft_kernel::DMat3::IDENTITY, DVec3::new(0.0, 0.0, 5.0)).unwrap();
    assert!((kk.mass_props(&up).unwrap().volume / (base * 24.0 / 19.0) - 1.0).abs() < 1e-3);
    let slant = kk.map_edge(&cyl, top, shapecraft_kernel::DMat3::IDENTITY, DVec3::new(6.0, 0.0, 0.0)).unwrap();
    assert!((kk.mass_props(&slant).unwrap().volume / base - 1.0).abs() < 1e-3);
    assert!((kk.bounds(&slant).unwrap().max.x - 10.5).abs() < 1e-2);
}

#[test]
fn scaling_a_straight_edge_stretches_it_and_the_ends_follow() {
    let kk = k();
    // 40 × 30 × 12 with a hole; the top edge along y at x = 20.
    let bx = kk.make_box(DVec3::new(-20.0, -15.0, 0.0), DVec3::new(40.0, 30.0, 12.0)).unwrap();
    let pin = kk.extrude_circle(DVec3::new(-10.0, 0.0, -1.0), 4.0, DVec3::new(0.0, 0.0, 14.0)).unwrap();
    let bx = kk.boolean(shapecraft_kernel::BoolOp::Subtract, &bx, &pin).unwrap();
    let edge = (0..kk.edge_count(&bx).unwrap())
        .find(|e| {
            matches!(kk.edge_curve(&bx, *e), Ok(EdgeCurve::Line { a, b })
                if (a.x - 20.0).abs() < 1e-6 && (b.x - 20.0).abs() < 1e-6 && a.z > 11.9 && b.z > 11.9)
        })
        .unwrap();
    let c = DVec3::new(20.0, 0.0, 12.0);
    let k = 26.926 / 15.0;
    let out = kk.scale_edge(&bx, edge, c, DVec3::new(1.0, k, 1.0)).unwrap();
    let bb = kk.bounds(&out).unwrap();
    assert!((bb.max.y - 26.926).abs() < 1e-2 && (bb.min.y + 26.926).abs() < 1e-2, "{bb:?}");
    assert!((bb.max.x - 20.0).abs() < 1e-3 && (bb.max.z - 12.0).abs() < 1e-3, "{bb:?}");
    // The two end faces bend (ruled); the box keeps its 7 faces.
    assert_eq!(kk.face_count(&out).unwrap(), 7);
    assert!(kk.mass_props(&out).unwrap().volume > kk.mass_props(&bx).unwrap().volume);
    // Along a direction the edge doesn't run: nothing to stretch, the part stays as it was.
    let same = kk.scale_edge(&bx, edge, c, DVec3::new(2.0, 1.0, 1.0)).unwrap();
    assert!((kk.mass_props(&same).unwrap().volume / kk.mass_props(&bx).unwrap().volume - 1.0).abs() < 1e-6);
    // A turn about the edge's middle (a quarter turn round z) bends both long sides.
    let turned =
        kk.map_edge(&bx, edge, shapecraft_kernel::DMat3::from_rotation_z(0.3), c - shapecraft_kernel::DMat3::from_rotation_z(0.3) * c);
    assert!(turned.is_ok(), "{:?}", turned.err());
}

#[test]
fn mixed_torus_ring_trims_at_period_seam() {
    use shapecraft_kernel::DVec2;
    use shapecraft_kernel::brep::{BodyKind, Brep, Curve, Edge, Face, Fin, Nurbs, Surface};
    let (r, t) = (10.0_f64, 2.0_f64);
    let lo = -2.0 * std::f64::consts::PI / 3.0;
    let hi = -std::f64::consts::FRAC_PI_2;
    let uv =
        [DVec2::new(0.0, hi), DVec2::new(0.25, hi - 0.2), DVec2::new(1.4, hi), DVec2::new(1.7, hi), DVec2::new(std::f64::consts::TAU, hi)];
    let point = |p: DVec2| DVec3::new((r + t * p.y.cos()) * p.x.cos(), (r + t * p.y.cos()) * p.x.sin(), t * p.y.sin());
    let torus = Surface::Torus { center: DVec3::ZERO, axis: DVec3::Z, x: DVec3::X, major: r, minor: t };
    let mut b = Brep {
        kind: BodyKind::Sheet,
        surfaces: vec![torus.clone(), torus],
        vertices: uv[..4].iter().copied().map(point).collect(),
        ..Brep::default()
    };
    for i in 0..4 {
        if i < 2 {
            b.curves.push(Curve::OnSurface {
                surface: 1,
                uv: Nurbs {
                    degree: 1,
                    poles: vec![uv[i + 1], uv[i]],
                    weights: None,
                    knots: vec![0.0, 1.0],
                    mults: vec![2, 2],
                    periodic: false,
                },
            });
        } else {
            b.curves.push(Curve::Circle {
                center: DVec3::new(0., 0., t * hi.sin()),
                normal: DVec3::Z,
                x: DVec3::X,
                radius: r + t * hi.cos(),
            });
        }
        b.edges.push(Edge {
            curve: Some(i),
            reversed: i >= 2,
            start: Some(if i >= 2 { (i + 1) % 4 } else { i }),
            end: Some(if i >= 2 { i } else { (i + 1) % 4 }),
            tolerance: None,
        });
    }
    b.curves.push(Curve::Circle { center: DVec3::new(0., 0., t * lo.sin()), normal: DVec3::Z, x: DVec3::X, radius: r + t * lo.cos() });
    b.edges.push(Edge { curve: Some(4), reversed: false, start: None, end: None, tolerance: None });
    for i in 0..2 {
        b.curves.push(Curve::OnSurface {
            surface: 0,
            uv: Nurbs {
                degree: 1,
                poles: vec![uv[i], uv[i + 1]],
                weights: None,
                knots: vec![0.0, 1.0],
                mults: vec![2, 2],
                periodic: false,
            },
        });
    }
    b.faces.push(Face {
        surface: 0,
        flip: true,
        loops: vec![
            (0..4).map(|edge| Fin { edge, forward: edge < 2, pcurve: if edge < 2 { Some(edge + 5) } else { None } }).collect(),
            vec![Fin { edge: 4, forward: false, pcurve: None }],
        ],
        color: None,
    });
    b.shells.push(vec![0]);
    let shape = k().from_brep(&b).unwrap();
    let actual = k().mass_props(&shape).unwrap().area;
    let expected: f64 = uv
        .windows(2)
        .map(|p| {
            let du = p[1].x - p[0].x;
            let dv = p[1].y - p[0].y;
            let avg = if dv.abs() < 1e-12 { p[0].y.sin() } else { (p[0].y.cos() - p[1].y.cos()) / dv };
            t * du * (r * ((p[0].y + p[1].y) / 2.0 - lo) + t * (avg - lo.sin()))
        })
        .sum();
    assert!((actual - expected).abs() < 1e-4, "area {actual}, expected {expected}");
}

/// #21: a repair that runs past its time is cut short (here a deadline already gone), and the shape comes back
/// unrepaired instead of the import hanging; with no limit nothing is cut.
#[test]
fn a_repair_past_its_budget_is_cut_short() {
    let opts = shapecraft_kernel::HealOptions { repair: true, edges: false, sew: false, simplify: false };
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let (_, cut) = k().heal_within(&b, opts, 0.0).unwrap();
    assert_eq!(cut, 0);
    let (same, cut) = k().heal_within(&b, opts, 1e-12).unwrap();
    assert_eq!(cut, 2);
    assert!((k().mass_props(&same).unwrap().volume - 1000.0).abs() < 1e-6);
}

/// #22: Cancel stops the repair of an import running on its own thread, and only there.
#[test]
fn cancel_stops_only_a_cancellable_thread() {
    let opts = shapecraft_kernel::HealOptions { repair: true, edges: false, sew: false, simplify: false };
    k().cancel(true);
    let b = k().make_box(DVec3::ZERO, DVec3::splat(10.0)).unwrap();
    let here = k().heal_within(&b, opts, 0.0);
    let there = std::thread::spawn(move || {
        k().cancellable(true);
        k().heal_within(&b, opts, 0.0).map(|(_, c)| c)
    })
    .join()
    .unwrap();
    k().cancel(false);
    assert!(here.is_ok_and(|(_, c)| c == 0), "a thread that isn't importing is never stopped");
    assert!(there.is_err_and(|e| e.to_string().contains("canceled")));
}

/// #20: a body of very dense B-spline surfaces (a loft through many finely drawn sections) gets its volume in
/// seconds from a triangulation, not after minutes of exact integration (237 s before), and still close to the
/// true one.
#[test]
fn dense_bspline_body_mass_props_stay_quick() {
    let section = |z: f64, i: usize| {
        let n = 900;
        let r = 10.0 + 0.2 * (i % 3) as f64;
        let mut poles: Vec<DVec3> = (0..n)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / n as f64;
                let w = r + 0.3 * (a * 40.0).sin();
                DVec3::new(w * a.cos(), w * a.sin(), z)
            })
            .collect();
        poles.push(poles[0]);
        let f = PlaneFrame { origin: DVec3::new(0.0, 0.0, z), x: DVec3::X, y: DVec3::Y };
        k().planar_regions(f, &[PlanarCurve::BSpline { poles }]).unwrap().remove(0)
    };
    let sections: Vec<Shape> = (0..40).map(|i| section(i as f64 * 1.5, i)).collect();
    let started = std::time::Instant::now();
    let body = k().loft(&sections).unwrap();
    let made = started.elapsed();
    let started = std::time::Instant::now();
    let v = k().mass_props(&body).unwrap().volume;
    let took = started.elapsed();
    eprintln!("loft {made:?}, mass props {took:?}, volume {v}");
    // About π r² h with r near 10.2 and h 58.5.
    let expected = std::f64::consts::PI * 10.2f64.powi(2) * 58.5;
    assert!((v - expected).abs() < 0.03 * expected, "{v} vs {expected}");
    assert!(took.as_secs_f64() < 30.0, "{took:?}");
}

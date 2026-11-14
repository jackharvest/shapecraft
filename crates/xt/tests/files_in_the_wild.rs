//! Shapes of data seen in real Parasolid files (written here by hand on top of a box we transmit ourselves):
//! padded knot arrays and tolerant edges that only carry SP-curves.
#![allow(clippy::unwrap_used)]

use shapecraft_kernel::{DVec3, Kernel};
use shapecraft_kernel_occt::OcctKernel;
use shapecraft_xt::{Format, Node, Val, XtFile};

const SP_CURVE: u16 = 137;
const B_CURVE: u16 = 134;
const NURBS_CURVE: u16 = 136;
const BSPLINE_VERTICES: u16 = 45;
const KNOT_MULT: u16 = 127;
const KNOT_SET: u16 = 128;

/// A 20 × 10 × 8 mm box as a parsed transmit file, and its volume.
fn box_file(k: &OcctKernel) -> (XtFile, f64) {
    let rect = vec![DVec3::ZERO, DVec3::new(20.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 0.0), DVec3::new(0.0, 10.0, 0.0)];
    let s = k.extrude_polygon(&rect, DVec3::new(0.0, 0.0, 8.0)).unwrap();
    let brep = k.to_brep(&s).unwrap().remove(0);
    let bytes = shapecraft_xt::write_body(&brep, Format::Text).unwrap();
    (shapecraft_xt::parse(&bytes).unwrap(), k.mass_props(&s).unwrap().volume)
}

/// Adds a node with the given fields (the rest null) and returns its index.
fn add(f: &mut XtFile, ty: u16, set: &[(&str, Val)]) -> u32 {
    // Node types the box doesn't use come from the base schema.
    let def = f.defs.entry(ty).or_insert_with(|| shapecraft_xt::schema::base()[&ty].clone()).clone();
    let fields = def
        .fields
        .iter()
        .map(|fd| {
            set.iter().find(|(n, _)| *n == fd.name).map(|(_, v)| v.clone()).unwrap_or(if fd.kind == b'p' { Val::Ptr(0) } else { Val::Null })
        })
        .collect();
    let index = f.nodes.iter().map(|n| n.index).max().unwrap_or(0) + 1;
    f.by_index.insert(index, f.nodes.len());
    f.nodes.push(Node { ty, index, fields });
    index
}

fn set(f: &mut XtFile, node: u32, field: &str, v: Val) {
    let at = f.by_index[&node];
    let ty = f.nodes[at].ty;
    let i = f.defs[&ty].index_of(field).unwrap();
    f.nodes[at].fields[i] = v;
}

fn get<'a>(f: &'a XtFile, node: u32, field: &str) -> &'a Val {
    let n = f.node(node).unwrap();
    f.field(n, field).unwrap()
}

fn point_of(f: &XtFile, vertex: u32) -> DVec3 {
    DVec3::from_array(get(f, get(f, vertex, "point").ptr(), "pvec").vec3().unwrap())
}

#[test]
fn wire_body_edges_are_read_from_the_shell_without_faces() {
    let k = OcctKernel::new();
    let (mut f, _) = box_file(&k);
    let body = f.nodes.iter().find(|n| f.type_name(n.ty) == "BODY").unwrap().index;
    let edge = f.nodes.iter().find(|n| f.type_name(n.ty) == "EDGE").unwrap().index;
    set(&mut f, body, "body_type", Val::Int(2));
    set(&mut f, edge, "next", Val::Ptr(0));
    set(&mut f, edge, "previous", Val::Ptr(0));
    let shells: Vec<u32> = f.nodes.iter().filter(|n| f.type_name(n.ty) == "SHELL").map(|n| n.index).collect();
    for (i, shell) in shells.into_iter().enumerate() {
        set(&mut f, shell, "face", Val::Ptr(0));
        set(&mut f, shell, "front_face", Val::Ptr(0));
        set(&mut f, shell, "edge", Val::Ptr(if i == 0 { edge } else { 0 }));
    }
    let bodies = shapecraft_xt::bodies(&f).unwrap();
    let b = &bodies[0].brep;
    assert_eq!(b.kind, shapecraft_kernel::brep::BodyKind::Wire);
    assert!(b.faces.is_empty());
    assert_eq!(b.edges.len(), 1);
    assert_eq!(b.vertices.len(), 2);
    let shape = k.from_brep(b).unwrap();
    let bounds = k.bounds(&shape).unwrap();
    assert!((bounds.max - bounds.min).length() > 1.0);
}

/// A straight edge: (edge, its positive fin, its negative fin, start, end) with start/end in meters.
fn an_edge(f: &XtFile) -> (u32, u32, u32, DVec3, DVec3) {
    let edge = f.nodes.iter().find(|n| f.type_name(n.ty) == "EDGE").unwrap().index;
    let fin = get(f, edge, "fin").ptr();
    let other = get(f, fin, "other").ptr();
    let minus = |fin: u32| get(f, fin, "sense").char() == Some(b'-');
    let (pos, neg) = if minus(fin) { (other, fin) } else { (fin, other) };
    // The positive fin's vertex ends the edge, the negative fin's starts it.
    (edge, pos, neg, point_of(f, get(f, neg, "vertex").ptr()), point_of(f, get(f, pos, "vertex").ptr()))
}

/// A degree-1 B-spline from `a` to `b` (each `dim` long) over [0, 1], knot arrays padded like real files pad them.
fn line_nurbs(f: &mut XtFile, a: &[f64], b: &[f64]) -> u32 {
    let verts: Vec<Val> = a.iter().chain(b).map(|x| Val::Real(*x)).collect();
    let v = add(f, BSPLINE_VERTICES, &[("vertices", Val::Arr(verts))]);
    let m = add(f, KNOT_MULT, &[("mult", Val::Arr(vec![Val::Int(2), Val::Int(2), Val::Int(0), Val::Int(0)]))]);
    let k = add(f, KNOT_SET, &[("knots", Val::Arr(vec![Val::Real(0.0), Val::Real(1.0), Val::Null, Val::Null]))]);
    let n = add(
        f,
        NURBS_CURVE,
        &[
            ("degree", Val::Int(1)),
            ("n_vertices", Val::Int(2)),
            ("vertex_dim", Val::Int(a.len() as i64)),
            ("n_knots", Val::Int(2)),
            ("periodic", Val::Bool(false)),
            ("closed", Val::Bool(false)),
            ("rational", Val::Bool(false)),
            ("bspline_vertices", Val::Ptr(v)),
            ("knot_mult", Val::Ptr(m)),
            ("knots", Val::Ptr(k)),
        ],
    );
    add(f, B_CURVE, &[("sense", Val::Char(b'+')), ("nurbs", Val::Ptr(n))])
}

fn rebuilt_volume(k: &OcctKernel, f: &XtFile) -> f64 {
    let bodies = shapecraft_xt::bodies(f).unwrap();
    assert_eq!(bodies.len(), 1);
    let s = k.from_brep(&bodies[0].brep).unwrap();
    assert_eq!(k.solids(&s).unwrap().len(), 1);
    k.mass_props(&s).unwrap().volume
}

#[test]
fn knot_arrays_longer_than_their_count_are_cut_to_it() {
    let k = OcctKernel::new();
    let (mut f, v0) = box_file(&k);
    let (edge, _, _, s, e) = an_edge(&f);
    let c = line_nurbs(&mut f, &s.to_array(), &e.to_array());
    set(&mut f, edge, "curve", Val::Ptr(c));
    let v1 = rebuilt_volume(&k, &f);
    assert!((v0 - v1).abs() < 1e-6 * v0, "volume {v0} -> {v1}");
}

/// The (u, v) of a point on a PLANE node, in the file's units (meters).
fn plane_uv(f: &XtFile, plane: u32, p: DVec3) -> [f64; 2] {
    let o = DVec3::from_array(get(f, plane, "pvec").vec3().unwrap());
    let n = DVec3::from_array(get(f, plane, "normal").vec3().unwrap());
    let x = DVec3::from_array(get(f, plane, "x_axis").vec3().unwrap());
    [(p - o).dot(x), (p - o).dot(n.cross(x))]
}

#[test]
fn a_tolerant_edge_is_built_from_its_fins_sp_curves() {
    let k = OcctKernel::new();
    let (mut f, v0) = box_file(&k);
    let (edge, pos, neg, s, e) = an_edge(&f);
    set(&mut f, edge, "curve", Val::Ptr(0));
    set(&mut f, edge, "tolerance", Val::Real(1e-7));
    for fin in [pos, neg] {
        let face = get(&f, get(&f, fin, "loop").ptr(), "face").ptr();
        let plane = get(&f, face, "surface").ptr();
        assert_eq!(f.type_name(f.node(plane).unwrap().ty), "PLANE");
        // Each SP-curve runs along its fin.
        let (a, b) = if fin == pos { (s, e) } else { (e, s) };
        let (ua, ub) = (plane_uv(&f, plane, a), plane_uv(&f, plane, b));
        let bc = line_nurbs(&mut f, &ua, &ub);
        let sp = add(&mut f, SP_CURVE, &[("sense", Val::Char(b'+')), ("surface", Val::Ptr(plane)), ("b_curve", Val::Ptr(bc))]);
        set(&mut f, fin, "curve", Val::Ptr(sp));
    }
    let v1 = rebuilt_volume(&k, &f);
    assert!((v0 - v1).abs() < 1e-6 * v0, "volume {v0} -> {v1}");
}

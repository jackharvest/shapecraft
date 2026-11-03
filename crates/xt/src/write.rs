//! Writing transmit files: solid bodies from a kernel-neutral B-rep, as text or neutral binary.
//!
//! Files carry an embedded schema against base 13006 with every node type unchanged (`255`), so a reader
//! needs nothing beyond the base schema every embedded file relies on.

use std::collections::HashMap;

use shapecraft_kernel::DVec3;
use shapecraft_kernel::brep::{BodyKind, Brep, Curve, Fin, Surface};

use crate::model::MM;
use crate::read::{Format, NULL_INT, NULL_REAL, Val, XtFile};
use crate::schema::{self, FieldDef, NodeDef};
use crate::{Error, Result};

const MODELLER: &str = ": TRANSMIT FILE created by modeller version 1300000";
const SCHEMA: &str = "SCH_1300000_13006_13006";
const MAX_TYPES: i64 = 249;

/// Nodes being assembled; index 1 is the root.
struct Graph {
    defs: HashMap<u16, NodeDef>,
    /// (type, fields) by index - 1.
    nodes: Vec<(u16, Vec<Val>)>,
    next_id: i64,
}

impl Graph {
    fn new() -> Self {
        Self { defs: schema::base(), nodes: Vec::new(), next_id: 1 }
    }

    /// A node of `ty` with defaults (null pointers, zero numbers, unset reals), then `set` fields.
    fn add(&mut self, ty: u16, set: &[(&str, Val)]) -> Result<u32> {
        let def = self.defs.get(&ty).ok_or_else(|| Error::Bad(format!("no layout for node type {ty}")))?;
        let mut fields: Vec<Val> = def
            .fields
            .iter()
            .map(|f| {
                let one = match f.kind {
                    b'p' => Val::Ptr(0),
                    b'f' | b'v' | b'i' | b'h' | b'b' => Val::Null,
                    b'c' => Val::Char(b'+'),
                    b'l' => Val::Bool(false),
                    _ => Val::Int(0),
                };
                match f.n {
                    0 => one,
                    1 => {
                        if f.kind == b'c' {
                            Val::Bytes(Vec::new())
                        } else {
                            Val::Arr(Vec::new())
                        }
                    }
                    n => Val::Arr(vec![one; n as usize]),
                }
            })
            .collect();
        if def.index_of("node_id").is_some() {
            let id = self.next_id;
            self.next_id += 1;
            if let Some(i) = def.index_of("node_id") {
                fields[i] = Val::Int(id);
            }
        }
        for (name, v) in set {
            let i = def.index_of(name).ok_or_else(|| Error::Bad(format!("{} has no field {name}", def.name)))?;
            fields[i] = v.clone();
        }
        self.nodes.push((ty, fields));
        Ok(self.nodes.len() as u32)
    }

    fn set(&mut self, index: u32, name: &str, v: Val) -> Result<()> {
        let (ty, fields) = self.nodes.get_mut(index.wrapping_sub(1) as usize).ok_or_else(|| Error::Bad(format!("no node {index}")))?;
        let def = self.defs.get(ty).ok_or_else(|| Error::Bad("layout".into()))?;
        let i = def.index_of(name).ok_or_else(|| Error::Bad(format!("{} has no field {name}", def.name)))?;
        fields[i] = v;
        Ok(())
    }

    /// Doubly link `items` through `next`/`previous` (or other names); returns the head.
    fn chain(&mut self, items: &[u32], next: &str, previous: Option<&str>) -> Result<u32> {
        for (k, &n) in items.iter().enumerate() {
            let nx = items.get(k + 1).copied().unwrap_or(0);
            self.set(n, next, Val::Ptr(nx))?;
            if let Some(prev) = previous {
                let pv = if k == 0 { 0 } else { items[k - 1] };
                self.set(n, prev, Val::Ptr(pv))?;
            }
        }
        Ok(items.first().copied().unwrap_or(0))
    }
}

const BODY: u16 = 12;
const SHELL: u16 = 13;
const FACE: u16 = 14;
const LOOP: u16 = 15;
const EDGE: u16 = 16;
const FIN: u16 = 17;
const VERTEX: u16 = 18;
const REGION: u16 = 19;
const POINT: u16 = 29;
const LINE: u16 = 30;
const CIRCLE: u16 = 31;
const ELLIPSE: u16 = 32;
const BSPLINE_VERTICES: u16 = 45;
const PLANE: u16 = 50;
const CYLINDER: u16 = 51;
const CONE: u16 = 52;
const SPHERE: u16 = 53;
const TORUS: u16 = 54;
const B_SURFACE: u16 = 124;
const NURBS_SURF: u16 = 126;
const KNOT_MULT: u16 = 127;
const KNOT_SET: u16 = 128;
const B_CURVE: u16 = 134;
const NURBS_CURVE: u16 = 136;

fn v3(v: DVec3) -> Val {
    Val::Vec3([v.x, v.y, v.z])
}
fn m(v: DVec3) -> Val {
    v3(v / MM)
}
fn unit(v: DVec3) -> Val {
    v3(v.normalize_or_zero())
}
fn ptr(p: u32) -> Val {
    Val::Ptr(p)
}
fn sense(minus: bool) -> Val {
    Val::Char(if minus { b'-' } else { b'+' })
}
/// `x` made perpendicular to `z` (files must hold orthonormal frames).
fn ortho(z: DVec3, x: DVec3) -> DVec3 {
    let z = z.normalize_or_zero();
    let x = x - z * x.dot(z);
    if x.length() < 1e-12 { z.any_orthonormal_vector() } else { x.normalize() }
}

/// Knot nodes for distinct knots and multiplicities: (KNOT_MULT, KNOT_SET).
fn knots(g: &mut Graph, k: &[f64], mults: &[u32]) -> Result<(u32, u32)> {
    let mult = g.add(KNOT_MULT, &[("mult", Val::Arr(mults.iter().map(|&x| Val::Int(x as i64)).collect()))])?;
    let set = g.add(KNOT_SET, &[("knots", Val::Arr(k.iter().map(|&x| Val::Real(x)).collect()))])?;
    Ok((mult, set))
}

/// Homogeneous control vertices in meters (`x*w, y*w, z*w, w` when rational).
fn homogeneous(poles: &[DVec3], weights: Option<&Vec<f64>>) -> Val {
    let mut out = Vec::with_capacity(poles.len() * 4);
    for (i, p) in poles.iter().enumerate() {
        let p = *p / MM;
        match weights {
            Some(w) => {
                let w = w.get(i).copied().unwrap_or(1.0);
                out.extend([p.x * w, p.y * w, p.z * w, w]);
            }
            None => out.extend([p.x, p.y, p.z]),
        }
    }
    Val::Arr(out.into_iter().map(Val::Real).collect())
}

/// The geometry node for a surface (owner set later); `true` when its natural normal is opposite the B-rep's.
fn surface(g: &mut Graph, s: &Surface) -> Result<(u32, bool)> {
    Ok(match s {
        Surface::Plane { origin, normal, x } => {
            (g.add(PLANE, &[("pvec", m(*origin)), ("normal", unit(*normal)), ("x_axis", v3(ortho(*normal, *x)))])?, false)
        }
        Surface::Cylinder { origin, axis, x, radius } => (
            g.add(
                CYLINDER,
                &[("pvec", m(*origin)), ("axis", unit(*axis)), ("radius", Val::Real(radius / MM)), ("x_axis", v3(ortho(*axis, *x)))],
            )?,
            false,
        ),
        Surface::Cone { origin, axis, x, radius, half_angle } => {
            // The radius grows along the axis by tan(a), a > 0 (see model.rs); flip the axis for shrinking cones.
            let (a_axis, a) = if *half_angle > 0.0 { (*axis, *half_angle) } else { (-*axis, -*half_angle) };
            (
                g.add(
                    CONE,
                    &[
                        ("pvec", m(*origin)),
                        ("axis", unit(a_axis)),
                        ("radius", Val::Real(radius / MM)),
                        ("sin_half_angle", Val::Real(a.sin())),
                        ("cos_half_angle", Val::Real(a.cos())),
                        ("x_axis", v3(ortho(a_axis, *x))),
                    ],
                )?,
                false,
            )
        }
        Surface::Sphere { center, axis, x, radius } => (
            g.add(
                SPHERE,
                &[("centre", m(*center)), ("radius", Val::Real(radius / MM)), ("axis", unit(*axis)), ("x_axis", v3(ortho(*axis, *x)))],
            )?,
            false,
        ),
        Surface::Torus { center, axis, x, major, minor } => (
            g.add(
                TORUS,
                &[
                    ("centre", m(*center)),
                    ("axis", unit(*axis)),
                    ("major_radius", Val::Real(major / MM)),
                    ("minor_radius", Val::Real(minor / MM)),
                    ("x_axis", v3(ortho(*axis, *x))),
                ],
            )?,
            false,
        ),
        Surface::BSpline(n) => {
            let verts = g.add(BSPLINE_VERTICES, &[("vertices", homogeneous(&n.poles, n.weights.as_ref()))])?;
            let (um, uk) = knots(g, &n.u_knots, &n.u_mults)?;
            let (vm, vk) = knots(g, &n.v_knots, &n.v_mults)?;
            let rational = n.weights.is_some();
            let nurbs = g.add(
                NURBS_SURF,
                &[
                    ("u_periodic", Val::Bool(false)),
                    ("v_periodic", Val::Bool(false)),
                    ("u_degree", Val::Int(n.u_degree as i64)),
                    ("v_degree", Val::Int(n.v_degree as i64)),
                    ("n_u_vertices", Val::Int(n.nu as i64)),
                    ("n_v_vertices", Val::Int(n.nv as i64)),
                    ("u_knot_type", Val::Int(2)),
                    ("v_knot_type", Val::Int(2)),
                    ("n_u_knots", Val::Int(n.u_knots.len() as i64)),
                    ("n_v_knots", Val::Int(n.v_knots.len() as i64)),
                    ("rational", Val::Bool(rational)),
                    ("u_closed", Val::Bool(false)),
                    ("v_closed", Val::Bool(false)),
                    ("surface_form", Val::Int(1)),
                    ("vertex_dim", Val::Int(if rational { 4 } else { 3 })),
                    ("bspline_vertices", ptr(verts)),
                    ("u_knot_mult", ptr(um)),
                    ("v_knot_mult", ptr(vm)),
                    ("u_knots", ptr(uk)),
                    ("v_knots", ptr(vk)),
                ],
            )?;
            (g.add(B_SURFACE, &[("nurbs", ptr(nurbs))])?, false)
        }
        _ => return Err(Error::Bad("offset, swept and spun surfaces are not written yet".into())),
    })
}

fn curve(g: &mut Graph, c: &Curve) -> Result<u32> {
    Ok(match c {
        Curve::Line { origin, dir } => g.add(LINE, &[("pvec", m(*origin)), ("direction", unit(*dir))])?,
        Curve::Circle { center, normal, x, radius } => g.add(
            CIRCLE,
            &[("centre", m(*center)), ("normal", unit(*normal)), ("x_axis", v3(ortho(*normal, *x))), ("radius", Val::Real(radius / MM))],
        )?,
        Curve::Ellipse { center, normal, x, major, minor } => g.add(
            ELLIPSE,
            &[
                ("centre", m(*center)),
                ("normal", unit(*normal)),
                ("x_axis", v3(ortho(*normal, *x))),
                ("major_radius", Val::Real(major / MM)),
                ("minor_radius", Val::Real(minor / MM)),
            ],
        )?,
        Curve::BSpline(n) => {
            let verts = g.add(BSPLINE_VERTICES, &[("vertices", homogeneous(&n.poles, n.weights.as_ref()))])?;
            let (km, ks) = knots(g, &n.knots, &n.mults)?;
            let rational = n.weights.is_some();
            let closed = n.poles.len() > 2 && n.poles.first().zip(n.poles.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9);
            let nurbs = g.add(
                NURBS_CURVE,
                &[
                    ("degree", Val::Int(n.degree as i64)),
                    ("n_vertices", Val::Int(n.poles.len() as i64)),
                    ("vertex_dim", Val::Int(if rational { 4 } else { 3 })),
                    ("n_knots", Val::Int(n.knots.len() as i64)),
                    ("knot_type", Val::Int(2)),
                    ("periodic", Val::Bool(false)),
                    ("closed", Val::Bool(closed)),
                    ("rational", Val::Bool(rational)),
                    ("curve_form", Val::Int(1)),
                    ("bspline_vertices", ptr(verts)),
                    ("knot_mult", ptr(km)),
                    ("knots", ptr(ks)),
                ],
            )?;
            g.add(B_CURVE, &[("nurbs", ptr(nurbs))])?
        }
        _ => return Err(Error::Bad("only exact curves are written".into())),
    })
}

/// The body without seam edges, as the modeller keeps periodic faces: a seam (an edge both of whose fins lie on
/// the same cylinder, cone, sphere or torus face) goes, its face's loop falls apart into closed loops, and closed
/// edges left alone on their vertex become rings with no vertex. A face whose loop doesn't fall apart cleanly
/// (an apex, a pole) keeps its seam.
fn seamless(b: &Brep) -> Brep {
    let mut out = b.clone();
    let ends = |f: &Fin| -> (Option<usize>, Option<usize>) {
        let e = &b.edges[f.edge];
        if f.forward { (e.start, e.end) } else { (e.end, e.start) }
    };
    for face in &mut out.faces {
        let periodic = matches!(
            b.surfaces.get(face.surface),
            Some(Surface::Cylinder { .. } | Surface::Cone { .. } | Surface::Sphere { .. } | Surface::Torus { .. })
        );
        if !periodic {
            continue;
        }
        let mut uses: HashMap<usize, usize> = HashMap::new();
        for fin in face.loops.iter().flatten() {
            *uses.entry(fin.edge).or_default() += 1;
        }
        if !uses.values().any(|&n| n == 2) {
            continue;
        }
        let mut loops = Vec::new();
        // Every seam end must stay on the face's other edges (an apex or a pole would vanish).
        let kept_ends: Vec<Option<usize>> =
            face.loops.iter().flatten().filter(|f| uses.get(&f.edge) != Some(&2)).flat_map(|f| [ends(f).0, ends(f).1]).collect();
        let mut clean = face
            .loops
            .iter()
            .flatten()
            .filter(|f| uses.get(&f.edge) == Some(&2))
            .all(|f| kept_ends.contains(&ends(f).0) && kept_ends.contains(&ends(f).1));
        for l in &face.loops {
            let kept: Vec<Fin> = l.iter().copied().filter(|f| uses.get(&f.edge) != Some(&2)).collect();
            // Split into closed runs.
            let mut run: Vec<Fin> = Vec::new();
            for fin in kept {
                if b.edges.get(fin.edge).is_none_or(|e| e.curve.is_none() || e.start.is_none()) {
                    clean = false;
                }
                run.push(fin);
                let first = ends(&run[0]).0;
                if first.is_some() && ends(&fin).1 == first {
                    loops.push(std::mem::take(&mut run));
                }
            }
            if !run.is_empty() {
                clean = false;
            }
        }
        if clean && !loops.is_empty() {
            face.loops = loops;
        }
    }
    // Vertices still on more than one edge end, or on an edge that isn't closed, stay.
    let used: Vec<usize> = out.faces.iter().flat_map(|f| f.loops.iter().flatten().map(|fin| fin.edge)).collect();
    let mut live = vec![false; out.edges.len()];
    used.iter().for_each(|&e| live[e] = true);
    let mut touches = vec![0usize; out.vertices.len()];
    let mut needed = vec![false; out.vertices.len()];
    for e in out.edges.iter().enumerate().filter(|(i, _)| live[*i]).map(|(_, e)| e) {
        for v in [e.start, e.end].into_iter().flatten() {
            touches[v] += 1;
        }
        if e.start != e.end {
            for v in [e.start, e.end].into_iter().flatten() {
                needed[v] = true;
            }
        }
    }
    let mut map = vec![None; out.vertices.len()];
    let mut vertices = Vec::new();
    for (v, p) in out.vertices.iter().enumerate() {
        // A closed edge's own vertex counts twice; anything more is another edge.
        if needed[v] || touches[v] > 2 {
            map[v] = Some(vertices.len());
            vertices.push(*p);
        }
    }
    // Drop edges no face uses (the seams) and renumber.
    let mut emap = vec![None; out.edges.len()];
    let mut edges = Vec::new();
    for (i, e) in out.edges.iter().enumerate() {
        if live[i] {
            emap[i] = Some(edges.len());
            let mut e = e.clone();
            e.start = e.start.and_then(|v| map[v]);
            e.end = e.end.and_then(|v| map[v]);
            if e.start.is_none() || e.end.is_none() {
                e.start = None;
                e.end = None;
            }
            edges.push(e);
        }
    }
    for fin in out.faces.iter_mut().flat_map(|f| f.loops.iter_mut().flatten()) {
        fin.edge = emap[fin.edge].unwrap_or(fin.edge);
    }
    out.vertices = vertices;
    out.edges = edges;
    out
}

/// One solid body as a part file's node graph (root = BODY).
fn body_graph(b: &Brep) -> Result<Graph> {
    let seamless = seamless(b);
    let b = &seamless;
    if b.kind != BodyKind::Solid {
        return Err(Error::Bad("only solid bodies are written yet".into()));
    }
    if b.faces.is_empty() {
        return Err(Error::Bad("a solid needs faces".into()));
    }
    let mut g = Graph::new();
    let body = g.add(
        BODY,
        &[
            ("res_size", Val::Real(1e3)),
            ("res_linear", Val::Real(1e-8)),
            ("state", Val::Int(1)),
            ("body_type", Val::Int(1)),
            ("nom_geom_state", Val::Int(1)),
        ],
    )?;

    // Vertices and their points.
    let mut points = Vec::new();
    let mut vertices = Vec::new();
    for p in &b.vertices {
        let v = g.add(VERTEX, &[("owner", ptr(body))])?;
        let pt = g.add(POINT, &[("owner", ptr(v)), ("pvec", m(*p))])?;
        g.set(v, "point", ptr(pt))?;
        points.push(pt);
        vertices.push(v);
    }
    // Edges, each with its own curve.
    let mut edges = Vec::new();
    let mut curves = Vec::new();
    for e in &b.edges {
        let c = e.curve.and_then(|i| b.curves.get(i)).ok_or_else(|| Error::Bad("edge without a curve".into()))?;
        let cn = curve(&mut g, c)?;
        g.set(cn, "sense", sense(e.reversed))?;
        let en = g.add(EDGE, &[("owner", ptr(body)), ("curve", ptr(cn))])?;
        g.set(cn, "owner", ptr(en))?;
        curves.push(cn);
        edges.push(en);
    }
    // Faces, loops and fins.
    let mut faces = Vec::new();
    let mut surfaces = Vec::new();
    let mut fins_of_edge: Vec<Vec<(u32, bool)>> = vec![Vec::new(); b.edges.len()];
    let mut fins_at_vertex: Vec<Vec<u32>> = vec![Vec::new(); b.vertices.len()];
    for f in &b.faces {
        let s = b.surfaces.get(f.surface).ok_or_else(|| Error::Bad("face without a surface".into()))?;
        let (sn, natural_flip) = surface(&mut g, s)?;
        let fnode = g.add(FACE, &[("surface", ptr(sn)), ("sense", sense(f.flip != natural_flip))])?;
        g.set(sn, "owner", ptr(fnode))?;
        let mut loops = Vec::new();
        for l in &f.loops {
            let ln = g.add(LOOP, &[("face", ptr(fnode))])?;
            let mut ring = Vec::new();
            for fin in l {
                let e = b.edges.get(fin.edge).ok_or_else(|| Error::Bad("fin on a missing edge".into()))?;
                // A fin's vertex is its forward end.
                let vx = if fin.forward { e.end } else { e.start };
                let vnode = vx.and_then(|i| vertices.get(i).copied()).unwrap_or(0);
                let fnn = g.add(
                    FIN,
                    &[("loop", ptr(ln)), ("edge", ptr(edges[fin.edge])), ("vertex", ptr(vnode)), ("sense", sense(!fin.forward))],
                )?;
                fins_of_edge[fin.edge].push((fnn, fin.forward));
                if let Some(i) = vx {
                    fins_at_vertex[i].push(fnn);
                }
                ring.push(fnn);
            }
            for (k, &fnn) in ring.iter().enumerate() {
                let fwd = ring[(k + 1) % ring.len()];
                let back = ring[(k + ring.len() - 1) % ring.len()];
                g.set(fnn, "forward", ptr(fwd))?;
                g.set(fnn, "backward", ptr(back))?;
            }
            if let Some(&first) = ring.first() {
                g.set(ln, "fin", ptr(first))?;
                loops.push(ln);
            }
        }
        let head = g.chain(&loops, "next", None)?;
        g.set(fnode, "loop", ptr(head))?;
        faces.push(fnode);
        surfaces.push(sn);
    }
    // Fin rings around edges: the positive fin first, then the rest.
    for (ei, fins) in fins_of_edge.iter_mut().enumerate() {
        if fins.is_empty() {
            return Err(Error::Bad("edge used by no face".into()));
        }
        fins.sort_by_key(|f| !f.1);
        for k in 0..fins.len() {
            let other = fins[(k + 1) % fins.len()].0;
            g.set(fins[k].0, "other", ptr(other))?;
        }
        g.set(edges[ei], "fin", ptr(fins[0].0))?;
    }
    for (vi, fins) in fins_at_vertex.iter().enumerate() {
        let head = g.chain(fins, "next_at_vx", None)?;
        g.set(vertices[vi], "fin", ptr(head))?;
    }
    // Regions: the infinite void first, then the solid; every shell component bounds the solid on its faces'
    // back side and a void on their front (the outer one bounds the infinite region, others are cavities).
    let outside = g.add(REGION, &[("body", ptr(body)), ("type", Val::Char(b'V'))])?;
    let solid = g.add(REGION, &[("body", ptr(body)), ("type", Val::Char(b'S'))])?;
    let mut regions = vec![outside, solid];
    let comps: Vec<Vec<usize>> = if b.shells.is_empty() { vec![(0..b.faces.len()).collect()] } else { b.shells.clone() };
    // The outer component reaches farthest out.
    let reach = |c: &Vec<usize>| -> f64 {
        c.iter()
            .flat_map(|&f| b.faces.get(f).into_iter().flat_map(|f| f.loops.iter().flatten()))
            .filter_map(|fin| b.edges.get(fin.edge).and_then(|e| e.start.or(e.end)).and_then(|v| b.vertices.get(v)))
            .map(|p| p.length())
            .fold(0.0, f64::max)
    };
    let outer = (0..comps.len()).max_by(|&a, &c| reach(&comps[a]).total_cmp(&reach(&comps[c]))).unwrap_or(0);
    let mut solid_shells = Vec::new();
    let mut outside_shells = Vec::new();
    for (ci, comp) in comps.iter().enumerate() {
        let back = g.add(SHELL, &[("body", ptr(body)), ("region", ptr(solid))])?;
        let void_region = if ci == outer {
            outside
        } else {
            let r = g.add(REGION, &[("body", ptr(body)), ("type", Val::Char(b'V'))])?;
            regions.push(r);
            r
        };
        // Void shells of a solid have no body (the reference: "must be set as above").
        let front = g.add(SHELL, &[("region", ptr(void_region))])?;
        let fs: Vec<u32> = comp.iter().filter_map(|&f| faces.get(f).copied()).collect();
        for &f in &fs {
            g.set(f, "shell", ptr(back))?;
            g.set(f, "front_shell", ptr(front))?;
        }
        let head = g.chain(&fs, "next", Some("previous"))?;
        g.set(back, "face", ptr(head))?;
        let head = g.chain(&fs, "next_front", Some("previous_front"))?;
        g.set(front, "front_face", ptr(head))?;
        solid_shells.push(back);
        if void_region == outside {
            outside_shells.push(front);
        } else {
            g.set(void_region, "shell", ptr(front))?;
        }
    }
    let head = g.chain(&solid_shells, "next", None)?;
    g.set(solid, "shell", ptr(head))?;
    let head = g.chain(&outside_shells, "next", None)?;
    g.set(outside, "shell", ptr(head))?;
    let head = g.chain(&regions, "next", Some("previous"))?;
    g.set(body, "region", ptr(head))?;
    g.set(body, "shell", ptr(solid_shells.first().copied().unwrap_or(0)))?;
    let head = g.chain(&edges, "next", Some("previous"))?;
    g.set(body, "edge", ptr(head))?;
    let head = g.chain(&vertices, "next", Some("previous"))?;
    g.set(body, "vertex", ptr(head))?;
    let head = g.chain(&surfaces, "next", Some("previous"))?;
    g.set(body, "boundary_surface", ptr(head))?;
    let head = g.chain(&curves, "next", Some("previous"))?;
    g.set(body, "boundary_curve", ptr(head))?;
    let head = g.chain(&points, "next", Some("previous"))?;
    g.set(body, "boundary_point", ptr(head))?;
    let highest = g.next_id - 1;
    g.set(body, "highest_node_id", Val::Int(highest))?;
    Ok(g)
}

/// A part file holding one solid.
pub fn write_body(b: &Brep, format: Format) -> Result<Vec<u8>> {
    let g = body_graph(b)?;
    let nodes: Vec<(u16, u32, &[Val])> = g.nodes.iter().enumerate().map(|(i, (ty, f))| (*ty, i as u32 + 1, f.as_slice())).collect();
    Ok(encode(format, MODELLER, SCHEMA, Some(MAX_TYPES), &g.defs, &nodes))
}

/// A parsed file written back as it was (same schema declarations, same nodes), in either encoding.
pub fn write_file(f: &XtFile, format: Format) -> Vec<u8> {
    let nodes: Vec<(u16, u32, &[Val])> = f.nodes.iter().map(|n| (n.ty, n.index, n.fields.as_slice())).collect();
    encode(format, &f.modeller, &f.schema_name, f.max_types.map(|m| m as i64), &f.defs, &nodes)
}

// ---- encodings ------------------------------------------------------------------------------------

/// The primitives of the two encodings.
trait Out {
    fn byte(&mut self, v: u8);
    fn char(&mut self, v: u8);
    fn logical(&mut self, v: bool);
    fn short(&mut self, v: i64);
    fn int(&mut self, v: i64);
    fn ptr(&mut self, v: u32);
    fn real(&mut self, v: f64);
    /// Several doubles; the text form writes one `?` when all are unset.
    fn reals(&mut self, v: &[f64]);
    fn chars(&mut self, v: &[u8]);
    fn short_string(&mut self, v: &str);
    fn var_len(&mut self, v: i64);
    fn node_type(&mut self, v: i64);
}

struct Bin(Vec<u8>);
impl Out for Bin {
    fn byte(&mut self, v: u8) {
        self.0.push(v);
    }
    fn char(&mut self, v: u8) {
        self.0.push(v);
    }
    fn logical(&mut self, v: bool) {
        self.0.push(u8::from(v));
    }
    fn short(&mut self, v: i64) {
        self.0.extend_from_slice(&(v as i16).to_be_bytes());
    }
    fn int(&mut self, v: i64) {
        self.0.extend_from_slice(&(v as i32).to_be_bytes());
    }
    fn ptr(&mut self, v: u32) {
        let v = v as i64;
        if v < 32767 {
            self.short(v + 1);
        } else {
            self.short(-(v % 32767 + 1));
            self.short(v / 32767);
        }
    }
    fn real(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn reals(&mut self, v: &[f64]) {
        v.iter().for_each(|&x| self.real(x));
    }
    fn chars(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }
    fn short_string(&mut self, v: &str) {
        self.0.push(v.len().min(255) as u8);
        self.0.extend_from_slice(&v.as_bytes()[..v.len().min(255)]);
    }
    fn var_len(&mut self, v: i64) {
        self.int(v);
    }
    fn node_type(&mut self, v: i64) {
        self.short(v);
    }
}

struct Txt(String);
impl Txt {
    fn num(&mut self, v: impl std::fmt::Display) {
        self.0.push_str(&v.to_string());
        self.0.push(' ');
    }
}
impl Out for Txt {
    fn byte(&mut self, v: u8) {
        self.num(v);
    }
    fn char(&mut self, v: u8) {
        self.0.push(v as char);
    }
    fn logical(&mut self, v: bool) {
        self.0.push(if v { 'T' } else { 'F' });
    }
    fn short(&mut self, v: i64) {
        if v == NULL_INT { self.0.push('?') } else { self.num(v) }
    }
    fn int(&mut self, v: i64) {
        self.short(v);
    }
    fn ptr(&mut self, v: u32) {
        self.num(v);
    }
    fn real(&mut self, x: f64) {
        if x == NULL_REAL {
            self.0.push('?');
        } else {
            self.num(text_real(x));
        }
    }
    fn reals(&mut self, v: &[f64]) {
        if v.iter().all(|&x| x == NULL_REAL) {
            self.0.push('?');
        } else {
            v.iter().for_each(|&x| self.real(x));
        }
    }
    fn chars(&mut self, v: &[u8]) {
        for &c in v {
            match c {
                b'\\' => self.0.push_str("\\\\"),
                b'\n' => self.0.push_str("\\n"),
                b'\r' => self.0.push_str("\\r"),
                0 => self.0.push_str("\\0"),
                c => self.0.push(c as char),
            }
        }
    }
    fn short_string(&mut self, v: &str) {
        self.num(v.len());
        self.chars(v.as_bytes());
    }
    fn var_len(&mut self, v: i64) {
        self.num(v);
    }
    fn node_type(&mut self, v: i64) {
        self.num(v);
    }
}

/// A real as text files hold it: 15 significant digits, the shorter of the plain form (no leading zero)
/// and the exponent form, plain on a tie (`.005`, `20`, `1e3`, `1e-8`).
fn text_real(x: f64) -> String {
    if x == 0.0 || !x.is_finite() {
        return "0".into();
    }
    let s = format!("{:.14e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if x < 0.0 { "-" } else { "" };
    // d.ddd × 10^exp; the exponent form puts the point after the first digit.
    let e_form = {
        let (head, tail) = digits.split_at(1);
        let m = if tail.is_empty() { head.to_string() } else { format!("{head}.{tail}") };
        format!("{sign}{m}e{exp}")
    };
    let n = digits.len() as i32;
    let plain = if exp >= n - 1 {
        format!("{sign}{digits}{}", "0".repeat((exp - (n - 1)) as usize))
    } else if exp >= 0 {
        let (a, b) = digits.split_at(exp as usize + 1);
        format!("{sign}{a}.{b}")
    } else {
        format!("{sign}.{}{digits}", "0".repeat((-exp - 1) as usize))
    };
    if e_form.len() < plain.len() { e_form } else { plain }
}

fn field_def(o: &mut dyn Out, f: &FieldDef) {
    o.short_string(&f.name);
    o.short(f.class as i64);
    o.ptr(f.n);
    if f.class == 0 {
        o.short_string(&(f.kind as char).to_string());
    }
    if f.n == 1 {
        o.logical(f.xmt);
    }
}

/// The layout header of a node type's first node: 255, an edit script against the base, or a full definition.
fn declare(o: &mut dyn Out, def: &NodeDef, base: Option<&NodeDef>) {
    let Some(base) = base else {
        o.byte(def.fields.len().min(255) as u8);
        o.short_string(&def.name);
        o.short_string(&def.description);
        def.fields.iter().for_each(|f| field_def(o, f));
        return;
    };
    if def.fields.len() == base.fields.len() && def.fields.iter().all(|f| f.base) {
        o.byte(255);
        return;
    }
    o.byte(def.fields.len().min(254) as u8);
    let mut b = 0usize;
    for (i, f) in def.fields.iter().enumerate() {
        let at = if f.base { base.fields.iter().position(|x| x.name == f.name) } else { None };
        if at.is_none() {
            // Base fields that no later field copies are deleted first (the modeller's own order).
            let rest = &def.fields[i..];
            while b < base.fields.len() && !rest.iter().any(|r| r.base && r.name == base.fields[b].name) {
                o.char(b'D');
                b += 1;
            }
        }
        match at {
            Some(k) if k >= b => {
                for _ in b..k {
                    o.char(b'D');
                }
                o.char(b'C');
                b = k + 1;
            }
            _ => {
                o.char(if b >= base.fields.len() { b'A' } else { b'I' });
                field_def(o, f);
            }
        }
    }
    o.char(b'Z');
}

fn value(o: &mut dyn Out, kind: u8, v: &Val) {
    match v {
        Val::Arr(a) => a.iter().for_each(|x| value(o, kind, x)),
        Val::Bytes(x) => o.chars(x),
        _ => match kind {
            b'u' => o.byte(v.int().unwrap_or(0) as u8),
            b'c' => o.char(v.char().unwrap_or(b'+')),
            b'l' => o.logical(v.bool().unwrap_or(false)),
            b'n' | b'w' => o.short(v.int().unwrap_or(NULL_INT)),
            b'd' | b't' => o.int(v.int().unwrap_or(NULL_INT)),
            b'p' => o.ptr(v.ptr()),
            b'f' => o.real(v.real().unwrap_or(NULL_REAL)),
            b'v' => o.reals(&v.vec3().unwrap_or([NULL_REAL; 3])),
            b'h' => o.reals(&reals(v, 3)),
            b'i' => o.reals(&reals(v, 2)),
            b'b' => o.reals(&reals(v, 6)),
            _ => {}
        },
    }
}

fn reals(v: &Val, n: usize) -> Vec<f64> {
    match v {
        Val::Reals(r) if r.len() == n => r.clone(),
        Val::Vec3(a) if n == 3 => a.to_vec(),
        _ => vec![NULL_REAL; n],
    }
}

fn encode(
    format: Format,
    modeller: &str,
    schema: &str,
    max_types: Option<i64>,
    defs: &HashMap<u16, NodeDef>,
    nodes: &[(u16, u32, &[Val])],
) -> Vec<u8> {
    let base = schema::base();
    let mut bin = Bin(b"PS\0\0".to_vec());
    let mut txt = Txt(String::from("T"));
    let o: &mut dyn Out = match format {
        Format::Binary => &mut bin,
        Format::Text => &mut txt,
    };
    match format {
        Format::Binary => {
            o.short(modeller.len() as i64);
            o.chars(modeller.as_bytes());
            o.int(schema.len() as i64);
            o.chars(schema.as_bytes());
        }
        Format::Text => {
            o.short_string(modeller);
            o.short_string(schema);
        }
    }
    if let Some(m) = max_types {
        o.short(m);
    }
    o.int(0);
    let mut declared = std::collections::HashSet::new();
    for &(ty, index, fields) in nodes {
        let Some(def) = defs.get(&ty) else { continue };
        o.node_type(ty as i64);
        if declared.insert(ty) && max_types.is_some() {
            declare(o, def, base.get(&ty));
        }
        if def.variable() {
            let n = match fields.last() {
                Some(Val::Arr(a)) => a.len(),
                Some(Val::Bytes(x)) => x.len(),
                _ => 0,
            };
            o.var_len(n as i64);
        }
        o.ptr(index);
        for (f, v) in def.fields.iter().zip(fields) {
            value(o, f.kind, v);
        }
    }
    o.node_type(1);
    o.ptr(0);
    match format {
        Format::Binary => bin.0,
        Format::Text => {
            // A standard human header, then the data in 80-column records (line breaks carry no meaning).
            let mut out = String::from(
                "**ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz**************************\n\
                 **PARASOLID !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789**************************\n\
                 **PART1;\nMC=unknown;\nMC_MODEL=unknown;\nMC_ID=unknown;\nOS=unknown;\nOS_RELEASE=unknown;\n\
                 FRU=ShapeCraft;\nAPPL=ShapeCraft;\nSITE=unknown;\nUSER=unknown;\nFORMAT=text;\nGUISE=transmit;\n\
                 **PART2;\nSCH=SCH_1300000_13006;\nUSFLD_SIZE=0;\n**PART3;\n\
                 **END_OF_HEADER*****************************************************************\n",
            );
            // A full record never ends in a space: the space starts the next one (the modeller rejects
            // files that break otherwise).
            let data = txt.0.as_bytes();
            let mut at = 0;
            while at < data.len() {
                let mut end = (at + 80).min(data.len());
                if end - at == 80 && data[end - 1] == b' ' {
                    end -= 1;
                }
                out.push_str(&String::from_utf8_lossy(&data[at..end]));
                out.push('\n');
                at = end;
            }
            out.into_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::text_real;

    #[test]
    fn reals_are_written_like_the_modeller_writes_them() {
        for (x, s) in [
            (1e3, "1e3"),
            (1e-8, "1e-8"),
            (0.005, ".005"),
            (0.02, ".02"),
            (20.0, "20"),
            (1.0, "1"),
            (-0.5, "-.5"),
            (160.0 / 255.0, ".627450980392157"),
            (0.0, "0"),
            (1.5e-3, ".0015"),
        ] {
            assert_eq!(text_real(x), s, "{x}");
        }
    }
}

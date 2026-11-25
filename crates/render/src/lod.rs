//! Coarser copies of heavy bodies, drawn while the view moves (#25).
//!
//! A copy keeps the body's face and edge numbers, so picks and the pointer colour it like the full mesh, and it never
//! strays from the body by more than its stated error. What's drawn is chosen by how many pixels that error covers.
//! Copies are made on a thread of their own; until one is there the full mesh is drawn.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use glam::DVec3;
use shapecraft_kernel::Mesh;

/// Bodies with less than this to draw (triangles and edge segments) aren't worth a copy.
const WORTH: usize = 400;
/// The copies' errors, as parts of the body's diagonal: about half a pixel, two and eight with the body filling a
/// 1000-pixel view.
const ERRORS: [f64; 3] = [1.0 / 2000.0, 1.0 / 500.0, 1.0 / 125.0];
/// A copy that isn't this much lighter than the one before isn't kept.
const LIGHTER: f64 = 0.75;
/// A triangle may not turn by more than this (the cosine) when a corner moves.
const TURN: f64 = 0.2;
const PASSES: usize = 48;

/// One coarser copy: nowhere farther than `error` (millimetres) from the body.
#[derive(Clone)]
pub struct Level {
    pub error: f64,
    pub mesh: Arc<Mesh>,
}

/// What a mesh costs to draw: triangles and edge segments.
pub fn weight(mesh: &Mesh) -> usize {
    mesh.indices.len() / 3 + mesh.edges.iter().map(|l| l.len().saturating_sub(1)).sum::<usize>()
}

/// The coarser copies worth having, finest first.
pub fn levels(mesh: &Mesh) -> Vec<Level> {
    let mut out: Vec<Level> = Vec::new();
    let mut last = weight(mesh);
    let Some((lo, hi)) = mesh.bounds().filter(|_| last >= WORTH) else { return out };
    let diagonal = (0..3).map(|i| f64::from(hi[i] - lo[i]).powi(2)).sum::<f64>().sqrt();
    if !diagonal.is_finite() || diagonal <= 0.0 {
        return out;
    }
    let mut done = 0.0;
    for part in ERRORS {
        let error = diagonal * part;
        let from = out.last().map_or(mesh, |l| &l.mesh);
        let Some(made) = simplify(from, error - done) else { break };
        let w = weight(&made);
        if (w as f64) <= last as f64 * LIGHTER {
            last = w;
            done = error;
            out.push(Level { error, mesh: Arc::new(made) });
        }
        if last < WORTH / 2 {
            break;
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Inside a face: may go to any neighbour.
    Free,
    /// On one feature line (an edge of the body, a border between faces): may go along it.
    Chain([u32; 2]),
    /// Where feature lines meet or end.
    Fixed,
}

/// The same body with fewer triangles and edge points, nowhere farther than `limit` from this one.
fn simplify(mesh: &Mesh, limit: f64) -> Option<Mesh> {
    // Corners at the very same place are one point: faces are meshed apart and share their borders exactly.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(mesh.positions.len());
    let mut at: Vec<DVec3> = Vec::new();
    let mut bits = |p: &[f32; 3]| -> Option<u32> {
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        let key = p.map(|v| (v + 0.0).to_bits());
        Some(*ids.entry(key).or_insert_with(|| {
            at.push(DVec3::new(p[0].into(), p[1].into(), p[2].into()));
            (at.len() - 1) as u32
        }))
    };
    let point: Vec<Option<u32>> = mesh.positions.iter().map(&mut bits).collect();
    let lines: Vec<Vec<Option<u32>>> = mesh.edges.iter().map(|l| l.iter().map(&mut bits).collect()).collect();

    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.indices.len() / 3);
    let mut faces: Vec<u32> = Vec::with_capacity(tris.capacity());
    // Every point's normals, by the face they belong to.
    let mut normals: Vec<(u32, u32, u32)> = Vec::with_capacity(mesh.indices.len());
    for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let corner = |i: u32| point.get(i as usize).copied().flatten();
        let (Some(a), Some(b), Some(c)) = (corner(tri[0]), corner(tri[1]), corner(tri[2])) else { continue };
        if a == b || b == c || c == a {
            continue;
        }
        let face = mesh.triangle_faces.get(t).copied().unwrap_or(0);
        for (p, i) in [(a, tri[0]), (b, tri[1]), (c, tri[2])] {
            normals.push((p, face, i));
        }
        tris.push([a, b, c]);
        faces.push(face);
    }
    if tris.is_empty() {
        return None;
    }
    normals.sort_unstable();
    normals.dedup();

    let mut kind = kinds(at.len(), &tris, &faces, &lines);
    let mut error = vec![0.0f64; at.len()];
    let mut gone = vec![false; at.len()];
    for _ in 0..PASSES {
        let ring = Ring::of(at.len(), &tris);
        let mut wishes: Vec<(f64, u32, u32)> = (0..at.len() as u32)
            .filter(|&a| !gone[a as usize] && ring.len(a) > 0)
            .filter_map(|a| best_move(a, &at, &tris, &ring, &kind, &error, limit).map(|(cost, b)| (cost, a, b)))
            .collect();
        if wishes.is_empty() {
            break;
        }
        wishes.sort_unstable_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let mut touched = vec![false; at.len()];
        let mut moved = 0usize;
        for (cost, a, b) in wishes {
            if touched[a as usize] || touched[b as usize] {
                continue;
            }
            // What was worked out for anything around here counted on these triangles.
            for &t in ring.at(a) {
                for p in tris[t as usize] {
                    touched[p as usize] = true;
                }
            }
            for &t in ring.at(a) {
                for p in &mut tris[t as usize] {
                    if *p == a {
                        *p = b;
                    }
                }
            }
            if let Kind::Chain(ends) = kind[a as usize] {
                let other = if ends[0] == b { ends[1] } else { ends[0] };
                for (p, new) in [(b, other), (other, b)] {
                    if let Kind::Chain(link) = &mut kind[p as usize] {
                        for l in link.iter_mut().filter(|l| **l == a) {
                            *l = new;
                        }
                    }
                }
                // The neighbour on the far side of a chain isn't in the ring of `a` only when the line runs off the mesh.
                touched[other as usize] = true;
            }
            error[b as usize] = error[b as usize].max(cost);
            gone[a as usize] = true;
            moved += 1;
        }
        let before = tris.len();
        let mut kept = 0;
        for i in 0..tris.len() {
            let [a, b, c] = tris[i];
            if a != b && b != c && c != a {
                tris[kept] = tris[i];
                faces[kept] = faces[i];
                kept += 1;
            }
        }
        tris.truncate(kept);
        faces.truncate(kept);
        if moved * 200 < before {
            break;
        }
    }

    // Each corner takes its face's normal there that suits the new triangle best; a triangle none suits is flat.
    let mut out = Mesh { edge_ids: mesh.edge_ids.clone(), edge_faces: mesh.edge_faces.clone(), ..Default::default() };
    let mut made: HashMap<(u32, [u32; 3]), u32> = HashMap::with_capacity(tris.len());
    for (tri, &face) in tris.iter().zip(&faces) {
        let [a, b, c] = tri.map(|p| at[p as usize]);
        let flat = (b - a).cross(c - a).normalize_or_zero().as_vec3();
        for &p in tri {
            let from = normals.partition_point(|n| (n.0, n.1) < (p, face));
            let best = normals[from..]
                .iter()
                .take_while(|n| (n.0, n.1) == (p, face))
                .filter_map(|n| mesh.normals.get(n.2 as usize).map(|n| glam::Vec3::from_array(*n)))
                .max_by(|x, y| x.dot(flat).total_cmp(&y.dot(flat)))
                .filter(|n| n.dot(flat) > 0.6)
                .unwrap_or(flat)
                .to_array();
            let next = out.positions.len() as u32;
            let index = *made.entry((p, best.map(f32::to_bits))).or_insert(next);
            if index == next {
                out.positions.push(at[p as usize].as_vec3().to_array());
                out.normals.push(best);
            }
            out.indices.push(index);
        }
        out.triangle_faces.push(face);
    }
    out.edges = mesh
        .edges
        .iter()
        .zip(&lines)
        .map(|(line, ids)| {
            // Ends stay where they are, unless it's a ring: that one starts wherever it still can.
            let ring = line.len() > 2 && ids.first() == ids.last();
            let last = line.len().saturating_sub(1);
            let stays = |i: usize| ids.get(i).copied().flatten().is_none_or(|p| !gone[p as usize]);
            let keep = |i: usize| stays(i) || (!ring && (i == 0 || i == last));
            let mut kept: Vec<[f32; 3]> = line.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, p)| *p).collect();
            if ring && !stays(0) {
                kept.extend(kept.first().copied());
            }
            kept
        })
        .collect();
    Some(out)
}

/// The triangles around each point.
struct Ring {
    start: Vec<u32>,
    tris: Vec<u32>,
}

impl Ring {
    fn of(points: usize, tris: &[[u32; 3]]) -> Self {
        let mut start = vec![0u32; points + 1];
        for p in tris.iter().flatten() {
            start[*p as usize + 1] += 1;
        }
        for i in 0..points {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut out = vec![0u32; tris.len() * 3];
        for (t, tri) in tris.iter().enumerate() {
            for p in tri {
                let slot = &mut fill[*p as usize];
                out[*slot as usize] = t as u32;
                *slot += 1;
            }
        }
        Self { start, tris: out }
    }

    fn at(&self, p: u32) -> &[u32] {
        &self.tris[self.start[p as usize] as usize..self.start[p as usize + 1] as usize]
    }

    fn len(&self, p: u32) -> usize {
        self.at(p).len()
    }
}

/// Which points lie on the body's edges and the borders between its faces.
fn kinds(points: usize, tris: &[[u32; 3]], faces: &[u32], lines: &[Vec<Option<u32>>]) -> Vec<Kind> {
    let pair = |a: u32, b: u32| if a < b { (a, b) } else { (b, a) };
    let mut sides: Vec<(u32, u32, u32)> = Vec::with_capacity(tris.len() * 3);
    for (tri, &face) in tris.iter().zip(faces) {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let (a, b) = pair(a, b);
            sides.push((a, b, face));
        }
    }
    sides.sort_unstable();
    let mut features: Vec<(u32, u32)> =
        sides.chunk_by(|x, y| (x.0, x.1) == (y.0, y.1)).filter(|g| !(g.len() == 2 && g[0].2 == g[1].2)).map(|g| (g[0].0, g[0].1)).collect();
    for line in lines {
        features.extend(line.windows(2).filter_map(|w| Some(pair(w[0]?, w[1]?)).filter(|(a, b)| a != b)));
    }
    features.sort_unstable();
    features.dedup();
    // How many feature lines leave each point, and the first two's other ends.
    let mut ends: Vec<(u32, [u32; 2])> = vec![(0, [0; 2]); points];
    for (a, b) in features {
        for (p, q) in [(a, b), (b, a)] {
            let e = &mut ends[p as usize];
            if let Some(slot) = e.1.get_mut(e.0 as usize) {
                *slot = q;
            }
            e.0 += 1;
        }
    }
    let mut kind: Vec<Kind> = ends
        .into_iter()
        .map(|(n, link)| match n {
            0 => Kind::Free,
            2 => Kind::Chain(link),
            _ => Kind::Fixed,
        })
        .collect();
    // Where a drawn edge ends is kept, so the edges still meet there.
    for line in lines {
        let closed = line.first() == line.last();
        for p in [line.first(), line.last()].into_iter().flatten().flatten().filter(|_| !closed) {
            kind[*p as usize] = Kind::Fixed;
        }
    }
    kind
}

fn distance_to_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let l = ab.length_squared();
    let t = if l > 0.0 { ((p - a).dot(ab) / l).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// Where point `a` can go with the least error, and that error (with what it already carried).
fn best_move(a: u32, at: &[DVec3], tris: &[[u32; 3]], ring: &Ring, kind: &[Kind], error: &[f64], limit: f64) -> Option<(f64, u32)> {
    let here = at[a as usize];
    let carried = error[a as usize];
    let around = ring.at(a);
    // Points with hundreds of triangles (a disc's hub) are slow to look at and few.
    if around.len() > 64 {
        return None;
    }
    let mut near: Vec<u32> = around.iter().flat_map(|&t| tris[t as usize]).filter(|&p| p != a).collect();
    near.sort_unstable();
    near.dedup();
    let mut best: Option<(f64, u32)> = None;
    for &b in &near {
        let mut cost = 0.0f64;
        match kind[a as usize] {
            Kind::Free => {}
            Kind::Chain(link) => {
                if !link.contains(&b) {
                    continue;
                }
                let other = if link[0] == b { link[1] } else { link[0] };
                if other == b {
                    continue;
                }
                cost = distance_to_segment(here, at[other as usize], at[b as usize]);
            }
            Kind::Fixed => return None,
        }
        let to = at[b as usize];
        let room = best.map_or(limit, |x| x.0);
        if cost + carried >= room {
            continue;
        }
        let mut shared = 0;
        let ok = around.iter().all(|&t| {
            let tri = tris[t as usize];
            let [p, q, r] = tri.map(|p| at[p as usize]);
            let n = (q - p).cross(r - p);
            let len = n.length();
            if len > 0.0 {
                cost = cost.max(((to - here).dot(n) / len).abs());
                if cost + carried >= room {
                    return false;
                }
            }
            if tri.contains(&b) {
                shared += 1;
                return true;
            }
            let [p, q, r] = tri.map(|x| if x == a { to } else { at[x as usize] });
            let m = (q - p).cross(r - p);
            m.length() > 0.0 && m.dot(n) > TURN * m.length() * len
        });
        if !ok || shared == 0 {
            continue;
        }
        // The surface stays a surface: what `a` and `b` both touch is what's across the triangles they share.
        let common = {
            let mut theirs: Vec<u32> = ring.at(b).iter().flat_map(|&t| tris[t as usize]).filter(|&p| p != b).collect();
            theirs.sort_unstable();
            theirs.dedup();
            near.iter().filter(|p| theirs.binary_search(p).is_ok()).count()
        };
        if common != shared {
            continue;
        }
        best = Some((cost + carried, b));
    }
    best
}

struct Entry {
    mesh: Arc<Mesh>,
    /// None while its copies are being made.
    levels: Option<Arc<[Level]>>,
    /// The frame it was last asked for.
    frame: u64,
    /// Asked for this often: a mesh that's only passing by (a drag's preview) isn't worth the work.
    seen: u32,
    sent: bool,
}

type Made = (usize, Vec<Level>);

/// The copies of the bodies on screen, and the thread that makes them.
pub(crate) struct Lods {
    none: Arc<[Level]>,
    entries: HashMap<usize, Entry>,
    worker: Option<(Sender<Arc<Mesh>>, Receiver<Made>)>,
    frame: u64,
    /// Goes up whenever copies arrive.
    pub arrived: u64,
}

impl Default for Lods {
    fn default() -> Self {
        Self { none: Arc::new([]), entries: HashMap::new(), worker: None, frame: 0, arrived: 0 }
    }
}

impl Lods {
    /// A frame starts: what the thread finished comes in, and what hasn't been asked for in a while goes.
    pub fn begin(&mut self) {
        self.frame += 1;
        let mut lost = false;
        if let Some((_, done)) = &self.worker {
            loop {
                match done.try_recv() {
                    Ok((key, levels)) => {
                        if let Some(e) = self.entries.get_mut(&key) {
                            e.levels = Some(levels.into());
                            self.arrived += 1;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        lost = true;
                        break;
                    }
                }
            }
        }
        if lost {
            self.worker = None;
        }
        let now = self.frame;
        // Undo and a drag's end bring a mesh back soon after it went; the copies of a deleted body go in the end.
        self.entries.retain(|_, e| now - e.frame < 600);
    }

    /// This body's copies, if they're there; else they'll be made.
    pub fn of(&mut self, mesh: &Arc<Mesh>) -> Arc<[Level]> {
        let key = Arc::as_ptr(mesh) as usize;
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            mesh: mesh.clone(),
            levels: (weight(mesh) < WORTH).then(|| self.none.clone()),
            frame: 0,
            seen: 0,
            sent: false,
        });
        entry.frame = self.frame;
        entry.seen = entry.seen.saturating_add(1);
        if entry.levels.is_none() && !entry.sent && entry.seen > 8 {
            if self.worker.is_none() {
                self.worker = start();
            }
            match &self.worker {
                Some((jobs, _)) if jobs.send(entry.mesh.clone()).is_ok() => entry.sent = true,
                // No thread to be had: the full mesh it is.
                _ => entry.levels = Some(self.none.clone()),
            }
        }
        entry.levels.as_ref().unwrap_or(&self.none).clone()
    }

    /// No copies wanted any more.
    pub fn forget(&mut self) {
        self.entries.clear();
        self.worker = None;
    }

    /// Copies are still being made.
    pub fn busy(&self) -> bool {
        self.entries.values().any(|e| e.levels.is_none())
    }
}

fn start() -> Option<(Sender<Arc<Mesh>>, Receiver<Made>)> {
    let (jobs, todo) = channel::<Arc<Mesh>>();
    let (done, made) = channel();
    std::thread::Builder::new()
        .name("lod".into())
        .spawn(move || {
            while let Ok(mesh) = todo.recv() {
                let key = Arc::as_ptr(&mesh) as usize;
                let levels = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| levels(&mesh))).unwrap_or_default();
                if done.send((key, levels)).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some((jobs, made))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid of `n` × `n` squares on a height field.
    fn field(n: u32, size: f64, h: impl Fn(f64, f64) -> f64) -> Mesh {
        let mut m = Mesh::default();
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = (size * f64::from(i) / f64::from(n), size * f64::from(j) / f64::from(n));
                m.positions.push([x as f32, y as f32, h(x, y) as f32]);
                m.normals.push([0.0, 0.0, 1.0]);
            }
        }
        for j in 0..n {
            for i in 0..n {
                let a = j * (n + 1) + i;
                m.indices.extend([a, a + 1, a + n + 2, a, a + n + 2, a + n + 1]);
                m.triangle_faces.extend([0, 0]);
            }
        }
        m
    }

    /// A closed cylinder: two caps (faces 0 and 1), a wall (2), both rims as edges. Every face has its own corners.
    fn cylinder(n: u32, r: f32, h: f32) -> Mesh {
        let mut m = Mesh::default();
        let rim: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                [r * a.cos(), r * a.sin()]
            })
            .collect();
        for (face, z, up) in [(0, 0.0, -1.0), (1, h, 1.0)] {
            let hub = m.positions.len() as u32;
            m.positions.push([0.0, 0.0, z]);
            m.normals.push([0.0, 0.0, up]);
            for p in &rim {
                m.positions.push([p[0], p[1], z]);
                m.normals.push([0.0, 0.0, up]);
            }
            for i in 0..n {
                let (a, b) = (hub + 1 + i, hub + 1 + (i + 1) % n);
                m.indices.extend(if up > 0.0 { [hub, a, b] } else { [hub, b, a] });
                m.triangle_faces.push(face);
            }
            let mut line: Vec<[f32; 3]> = rim.iter().map(|p| [p[0], p[1], z]).collect();
            line.push(line[0]);
            m.edges.push(line);
            m.edge_ids.push(face);
            m.edge_faces.push([face, 2]);
        }
        let base = m.positions.len() as u32;
        for p in &rim {
            for z in [0.0, h] {
                m.positions.push([p[0], p[1], z]);
                m.normals.push([p[0] / r, p[1] / r, 0.0]);
            }
        }
        for i in 0..n {
            let (a, b) = (base + 2 * i, base + 2 * ((i + 1) % n));
            m.indices.extend([a, b, b + 1, a, b + 1, a + 1]);
            m.triangle_faces.extend([2, 2]);
        }
        m
    }

    #[test]
    fn a_flat_field_needs_next_to_nothing() {
        let l = levels(&field(40, 100.0, |_, _| 0.0));
        let last = &l.last().unwrap().mesh;
        assert!(last.indices.len() / 3 < 40, "{}", last.indices.len() / 3);
        // Its outline is still the square.
        let (lo, hi) = last.bounds().unwrap();
        assert_eq!((lo, hi), ([0.0; 3], [100.0, 100.0, 0.0]));
    }

    #[test]
    fn no_point_strays_farther_than_the_error() {
        let m = field(60, 100.0, |x, y| 8.0 * (x / 12.0).sin() * (y / 15.0).cos());
        let ls = levels(&m);
        assert!(!ls.is_empty());
        let mut weights = vec![weight(&m)];
        for l in &ls {
            weights.push(weight(&l.mesh));
            let tris: Vec<[DVec3; 3]> = l
                .mesh
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| DVec3::from_array(l.mesh.positions[i as usize].map(f64::from))))
                .collect();
            let worst = m
                .positions
                .iter()
                .map(|p| {
                    let p = DVec3::from_array(p.map(f64::from));
                    tris.iter().map(|t| to_triangle(p, *t)).fold(f64::MAX, f64::min)
                })
                .fold(0.0, f64::max);
            assert!(worst <= l.error * 1.5, "{worst} past {}", l.error);
        }
        assert!(weights.windows(2).all(|w| w[1] * 4 <= w[0] * 3), "{weights:?}");
    }

    fn to_triangle(p: DVec3, [a, b, c]: [DVec3; 3]) -> f64 {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let q = p - n * (p - a).dot(n);
        let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| (*v - *u).cross(q - *u).dot(n) >= 0.0);
        if inside {
            return p.distance(q);
        }
        [(a, b), (b, c), (c, a)].iter().map(|(u, v)| distance_to_segment(p, *u, *v)).fold(f64::MAX, f64::min)
    }

    #[test]
    fn a_cylinder_stays_closed_and_keeps_its_faces_and_rims() {
        let m = cylinder(400, 50.0, 80.0);
        assert!(crate::closed_outward(&m));
        let ls = levels(&m);
        assert!(ls.len() >= 2, "{}", ls.len());
        for l in &ls {
            assert!(crate::closed_outward(&l.mesh));
            assert_eq!(l.mesh.edge_ids, m.edge_ids);
            let faces: std::collections::BTreeSet<u32> = l.mesh.triangle_faces.iter().copied().collect();
            assert_eq!(faces.into_iter().collect::<Vec<_>>(), [0, 1, 2]);
            // The rims are drawn along the triangles' own corners, all the way around.
            for (line, full) in l.mesh.edges.iter().zip(&m.edges) {
                assert!(line.len() >= 8 && line.len() < full.len(), "{}", line.len());
                assert_eq!(line.first(), line.last());
                assert!(line.iter().all(|p| l.mesh.positions.contains(p)));
            }
        }
        assert!(weight(&ls.last().unwrap().mesh) * 8 < weight(&m), "{}", weight(&ls.last().unwrap().mesh));
    }

    #[test]
    fn junk_is_left_alone() {
        let mut m = cylinder(400, 50.0, 80.0);
        m.positions[5] = [f32::NAN; 3];
        m.indices.extend([1, 2, 900_000, 3, 3, 3]);
        m.edges.push(vec![[f32::INFINITY; 3]]);
        m.edges.push(Vec::new());
        let _ = levels(&m);
        assert!(levels(&Mesh::default()).is_empty());
        assert!(levels(&cylinder(12, 5.0, 5.0)).is_empty(), "a light body has no copies");
    }

    #[test]
    fn copies_arrive_from_the_thread() {
        let mut lods = Lods::default();
        let mesh = Arc::new(cylinder(400, 50.0, 80.0));
        for _ in 0..500 {
            lods.begin();
            if !lods.of(&mesh).is_empty() {
                assert!(!lods.busy());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no copies");
    }
}

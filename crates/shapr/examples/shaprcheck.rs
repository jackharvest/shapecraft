//! `shaprcheck FILE.shapr [--show N] [--xt DIR]`: rebuild every live body of a Shapr3D project with OCCT and group
//! the failures by message (a development aid for the importer). `--xt DIR` writes each failing body's partition
//! there as `<partition>.x_b` for a closer look with xtdump.
use std::collections::BTreeMap;
use std::time::Instant;

use shapecraft_kernel::Kernel;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(path) = a.get(1) else { return eprintln!("usage: shaprcheck FILE [--show N] [--xt DIR]") };
    let show: usize = a.iter().position(|x| x == "--show").and_then(|i| a.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(3);
    let xt_dir = a.iter().position(|x| x == "--xt").and_then(|i| a.get(i + 1)).cloned();
    // `--only N,M`: just these partitions (with --xt they're written out even when they build).
    let only: Vec<i64> = a
        .iter()
        .position(|x| x == "--only")
        .and_then(|i| a.get(i + 1))
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_default();
    let t0 = Instant::now();
    let f = match shapecraft_shapr::ShaprFile::read(std::path::Path::new(path)) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let p = match shapecraft_shapr::Project::read(&f) {
        Ok(p) => p,
        Err(e) => return eprintln!("{e}"),
    };
    let t_read = t0.elapsed().as_secs_f64();
    let k = shapecraft_kernel_occt::OcctKernel::new();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for u in &p.unreadable {
        let msg = u.split(" #").next().unwrap_or(u).to_string();
        groups.entry(format!("xt: {}", squash(&msg))).or_default().push(u.clone());
    }
    let (mut ok, mut meshes, mut bad_volume) = (0, 0, 0);
    let (mut t_build, mut t_volume) = (0.0, 0.0);
    let mut failed_parts = Vec::new();
    for b in &p.bodies {
        if !only.is_empty() && !b.partition.is_some_and(|x| only.contains(&x)) {
            continue;
        }
        if !only.is_empty() {
            failed_parts.extend(b.partition);
        }
        if b.mesh.is_some() && b.brep.faces.is_empty() {
            meshes += 1;
            continue;
        }
        let who = format!("{:?} part {:?}, step {:?} ({:?}, {} faces)", b.name, b.partition, b.step, b.brep.kind, b.brep.faces.len());
        let tb = Instant::now();
        let built = k.from_brep(&b.brep);
        let secs = tb.elapsed().as_secs_f64();
        t_build += secs;
        if secs > 3.0 {
            println!("  slow {secs:.1}s {who} ok={}", built.is_ok());
        }
        match built {
            Ok(s) => {
                let tv = Instant::now();
                let v = k.mass_props(&s).map(|m| m.volume).unwrap_or(f64::NAN);
                t_volume += tv.elapsed().as_secs_f64();
                let solids = k.solids(&s).map(|v| v.len()).unwrap_or(0);
                if !(v > 0.0) || solids == 0 {
                    bad_volume += 1;
                    groups.entry(format!("built but volume {v:.3e} / {solids} solids")).or_default().push(who);
                } else {
                    ok += 1;
                }
            }
            Err(e) => {
                if a.iter().any(|a| a == "--surfaces") {
                    for (i, surface) in b.brep.surfaces.iter().enumerate() {
                        if let shapecraft_kernel::brep::Surface::BSpline(n) = surface {
                            let (lo, hi) =
                                n.weights.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &w| (lo.min(w), hi.max(w)));
                            println!(
                                "  {who} surface {i}: {}x{} poles, degree {}/{}, weights {lo}..{hi}",
                                n.nu, n.nv, n.u_degree, n.v_degree
                            );
                        }
                    }
                }
                groups.entry(format!("occt: {}", squash(&e.to_string()))).or_default().push(who);
                failed_parts.extend(b.partition);
            }
        }
    }
    let live = p.bodies.len() + p.unreadable.len();
    println!(
        "{path}: live {live} = {} read + {} unreadable; built ok {ok}, meshes {meshes}, bad volume {bad_volume}; read {t_read:.1}s build {t_build:.1}s volume {t_volume:.1}s total {:.1}s",
        p.bodies.len(),
        p.unreadable.len(),
        t0.elapsed().as_secs_f64()
    );
    let mut g: Vec<_> = groups.into_iter().collect();
    g.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (m, v) in g {
        println!("  {:5} {m}", v.len());
        for w in v.iter().take(show) {
            println!("          {w}");
        }
    }
    if let (Some(dir), Ok(db)) = (xt_dir, f.db()) {
        let _ = std::fs::create_dir_all(&dir);
        // Partitions whose transmit data we can't read at all.
        let mut all: Vec<(i64, Vec<u8>)> = Vec::new();
        for sql in [
            "SELECT PartitionID, ChunkIndex, Block FROM BodyRevisionBlocks WHERE IsDeleted = 0 AND Phase = 2 ORDER BY PartitionID, ChunkIndex",
            "SELECT PartitionID, 0, Block FROM BodyRevisionPartitions",
        ] {
            if let Ok(mut q) = db.conn.prepare(sql) {
                let rows: Vec<(i64, Vec<u8>)> = q
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(2)?)))
                    .map(|r| r.flatten().collect())
                    .unwrap_or_default();
                for (pid, b) in rows {
                    match all.last_mut() {
                        Some((p, d)) if *p == pid => d.extend(b),
                        _ => all.push((pid, b)),
                    }
                }
            }
        }
        for (pid, data) in all {
            if let Err(e) = shapecraft_xt::parse(&data).and_then(|f| shapecraft_xt::bodies(&f)) {
                println!("  unreadable partition {pid}: {e}");
                let _ = std::fs::write(format!("{dir}/{pid}.x_b"), data);
            }
        }
        for pid in failed_parts {
            let blocks: Vec<Vec<u8>> = db
                .conn
                .prepare("SELECT Block FROM BodyRevisionBlocks WHERE IsDeleted = 0 AND Phase = 2 AND PartitionID = ?1 ORDER BY ChunkIndex")
                .and_then(|mut q| q.query_map([pid], |r| r.get::<_, Vec<u8>>(0))?.collect())
                .or_else(|_| {
                    db.conn
                        .prepare("SELECT Block FROM BodyRevisionPartitions WHERE PartitionID = ?1")
                        .and_then(|mut q| q.query_map([pid], |r| r.get::<_, Vec<u8>>(0))?.collect())
                })
                .unwrap_or_default();
            let _ = std::fs::write(format!("{dir}/{pid}.x_b"), blocks.concat());
        }
    }
}

/// A message with its numbers taken out, so the same failure on different nodes groups together.
fn squash(m: &str) -> String {
    let mut out = String::new();
    let mut digits = false;
    for c in m.chars() {
        if c.is_ascii_digit() {
            if !digits {
                out.push('N');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

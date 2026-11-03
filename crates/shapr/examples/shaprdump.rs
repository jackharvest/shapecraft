//! `shaprdump FILE.shapr`: what we understand of a Shapr3D project.
fn main() {
    let Some(path) = std::env::args().nth(1) else { return eprintln!("usage: shaprdump FILE") };
    let f = match shapecraft_shapr::ShaprFile::read(std::path::Path::new(&path)) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    println!("metadata {}", f.metadata);
    let p = match shapecraft_shapr::Project::read(&f) {
        Ok(p) => p,
        Err(e) => return eprintln!("{e}"),
    };
    for (k, v) in &p.settings {
        println!("  {k} = {v}");
    }
    for s in &p.steps {
        println!("step #{} {:?} {}", s.node, s.name, s.op);
        for (id, v) in &s.params {
            println!("    #{id} {v}");
        }
    }
    for f in &p.folders {
        println!("folder {} {:?} hidden {} {:?}", f.id, f.name, f.hidden, f.children);
    }
    println!("names {:?}", p.item_names);
    for b in &p.bodies {
        println!(
            "body {:?} in {:?} partition {:?}: {} faces {} edges",
            b.name,
            b.folder,
            b.partition,
            b.brep.faces.len(),
            b.brep.edges.len()
        );
    }
}

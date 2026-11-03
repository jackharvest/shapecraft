//! `mkshapr OUT.shapr FILE.x_b…`: a fresh Shapr3D project with each part as a body (named after the file).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(out) = a.get(1) else { return eprintln!("usage: mkshapr OUT FILE…") };
    let mut w = match shapecraft_shapr::Writer::fresh() {
        Ok(w) => w,
        Err(e) => return eprintln!("{e}"),
    };
    for f in &a[2..] {
        let name = std::path::Path::new(f).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = if name.starts_with('r') { vec!["Round things".to_string()] } else { Vec::new() };
        if let Err(e) = w.import_body(&name, &std::fs::read(f).unwrap_or_default(), &folder, Default::default()) {
            return eprintln!("{e}");
        }
    }
    match w.finish() {
        Ok(b) => {
            let _ = std::fs::write(out, b);
        }
        Err(e) => eprintln!("{e}"),
    }
}

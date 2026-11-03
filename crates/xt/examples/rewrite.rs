//! `rewrite IN OUT`: read the bodies of a transmit file and write the first one with our writer (binary).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = match shapecraft_xt::parse(&std::fs::read(&a[1]).unwrap_or_default()) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let Some(b) = shapecraft_xt::bodies(&f).ok().and_then(|mut b| (!b.is_empty()).then(|| b.remove(0))) else {
        return eprintln!("no body");
    };
    let fmt = if a.get(3).map(String::as_str) == Some("text") { shapecraft_xt::Format::Text } else { shapecraft_xt::Format::Binary };
    match shapecraft_xt::write_body(&b.brep, fmt) {
        Ok(bytes) => {
            let _ = std::fs::write(&a[2], bytes);
        }
        Err(e) => eprintln!("{e}"),
    }
}

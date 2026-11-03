//! `recode IN OUT [text|binary]`: parse a transmit file and write it back (checks the encoder).
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (Some(i), Some(o)) = (a.get(1), a.get(2)) else { return eprintln!("usage: recode IN OUT [text|binary]") };
    let f = match shapecraft_xt::parse(&std::fs::read(i).unwrap_or_default()) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let fmt = if a.get(3).map(String::as_str) == Some("binary") { shapecraft_xt::Format::Binary } else { shapecraft_xt::Format::Text };
    let _ = std::fs::write(o, shapecraft_xt::write_file(&f, fmt));
}

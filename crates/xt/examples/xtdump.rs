//! `xtdump FILE [--nodes]`: parse a transmit file and print its schema and a census (or every node).
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else { return eprintln!("usage: xtdump FILE [--nodes]") };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return eprintln!("{e}"),
    };
    let f = match shapecraft_xt::parse(&bytes) {
        Ok(f) => f,
        Err(e) => return eprintln!("error: {e}"),
    };
    println!("{:?} modeller {:?} schema {:?} max_types {:?} nodes {}", f.format, f.modeller, f.schema_name, f.max_types, f.nodes.len());
    let mut census = std::collections::BTreeMap::new();
    for n in &f.nodes {
        *census.entry(n.ty).or_insert(0) += 1;
    }
    for (ty, c) in census {
        println!("  {ty:4} {:20} {c}", f.type_name(ty));
    }
    if args.iter().any(|a| a == "--nodes") {
        for n in &f.nodes {
            let def = f.def(n.ty);
            let parts: Vec<String> = n
                .fields
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let name = def.and_then(|d| d.fields.get(i)).map(|f| f.name.as_str()).unwrap_or("?");
                    format!("{name}={}", short(v))
                })
                .collect();
            println!("#{} {} {}", n.index, f.type_name(n.ty), parts.join(" "));
        }
    }
}
fn short(v: &shapecraft_xt::Val) -> String {
    use shapecraft_xt::Val::*;
    match v {
        Null => "-".into(),
        Int(i) => i.to_string(),
        Real(r) => format!("{r}"),
        Ptr(p) => format!("@{p}"),
        Char(c) => format!("'{}'", *c as char),
        Bool(b) => {
            if *b {
                "T".into()
            } else {
                "F".into()
            }
        }
        Vec3(a) => format!("({},{},{})", a[0], a[1], a[2]),
        Reals(r) => format!("{r:?}"),
        Bytes(b) => format!("{:?}", String::from_utf8_lossy(b)),
        Arr(a) => format!("[{}]", a.iter().map(short).collect::<Vec<_>>().join(",")),
    }
}

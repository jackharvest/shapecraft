//! `xtsig FILE`: one canonical line per node (type, non-pointer values, pointed-to types), sorted.
use shapecraft_xt::Val;
fn main() {
    let Some(path) = std::env::args().nth(1) else { return };
    let f = match shapecraft_xt::parse(&std::fs::read(&path).unwrap_or_default()) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let mut lines: Vec<String> = f
        .nodes
        .iter()
        .map(|n| {
            let d = f.def(n.ty);
            let parts: Vec<String> = n
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    let name = d.and_then(|d| d.fields.get(i)).map(|x| x.name.as_str()).unwrap_or("?");
                    if name == "node_id" || name == "highest_node_id" {
                        return None;
                    }
                    Some(match v {
                        Val::Ptr(0) => format!("{name}=0"),
                        Val::Ptr(p) => format!("{name}->{}", f.node(*p).map(|t| f.type_name(t.ty)).unwrap_or("??")),
                        Val::Real(r) => format!("{name}={:.6}", r),
                        Val::Vec3(a) => format!("{name}=({:.4},{:.4},{:.4})", a[0] + 0.0, a[1] + 0.0, a[2] + 0.0),
                        v => format!("{name}={v:?}"),
                    })
                })
                .collect();
            format!("{} {}", f.type_name(n.ty), parts.join(" "))
        })
        .collect();
    lines.sort();
    for l in lines {
        println!("{l}")
    }
}

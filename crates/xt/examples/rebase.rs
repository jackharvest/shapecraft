//! `rebase IN OUT MODELLER SCHEMA`: rewrite a transmit file with every node in the base 13006 layout (fields
//! the base lacks are dropped) under a given header. Used to find what Parasolid accepts.
use shapecraft_xt::{Format, Val, schema};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 5 {
        return eprintln!("usage: rebase IN OUT MODELLER SCHEMA [text]");
    }
    let mut f = match shapecraft_xt::parse(&std::fs::read(&a[1]).unwrap_or_default()) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let base = schema::base();
    for n in &mut f.nodes {
        let (Some(cur), Some(b)) = (f.defs.get(&n.ty), base.get(&n.ty)) else { continue };
        n.fields = b
            .fields
            .iter()
            .map(|bf| {
                let v = cur.index_of(&bf.name).and_then(|i| n.fields.get(i).cloned()).unwrap_or(Val::Ptr(0));
                match v {
                    Val::Arr(mut a) if bf.n > 1 => {
                        a.resize(bf.n as usize, Val::Bool(false));
                        Val::Arr(a)
                    }
                    v => v,
                }
            })
            .collect();
    }
    f.nodes.retain(|n| base.contains_key(&n.ty));
    if a.iter().any(|x| x == "noattr") {
        let drop = [70u16, 74, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 98, 99];
        f.nodes.retain(|n| !drop.contains(&n.ty));
        for n in &mut f.nodes {
            if let Some(d) = base.get(&n.ty) {
                for name in ["attributes_groups", "attribute_chains"] {
                    if let Some(i) = d.index_of(name) {
                        n.fields[i] = Val::Ptr(0);
                    }
                }
            }
        }
    }
    f.defs = base;
    f.modeller = a[3].clone();
    f.schema_name = a[4].clone();
    let fmt = if a.iter().any(|x| x == "text") { Format::Text } else { Format::Binary };
    let _ = std::fs::write(&a[2], shapecraft_xt::write_file(&f, fmt));
}

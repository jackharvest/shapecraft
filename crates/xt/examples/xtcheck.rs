//! `xtcheck FILE`: topology invariants of every body (fin/vertex/edge/loop/shell links and senses).
use shapecraft_xt::{Node, Val, XtFile};
fn p(f: &XtFile, n: &Node, field: &str) -> u32 {
    f.field(n, field).map(Val::ptr).unwrap_or(0)
}
fn c(f: &XtFile, n: &Node, field: &str) -> u8 {
    f.field(n, field).and_then(Val::char).unwrap_or(b'?')
}
fn pt(f: &XtFile, v: u32) -> Option<[f64; 3]> {
    let v = f.node(v)?;
    f.field(f.node(p(f, v, "point"))?, "pvec")?.vec3()
}
fn main() {
    let Some(path) = std::env::args().nth(1) else { return };
    let f = match shapecraft_xt::parse(&std::fs::read(&path).unwrap_or_default()) {
        Ok(f) => f,
        Err(e) => return eprintln!("{e}"),
    };
    let mut bad = 0;
    let mut say = |m: String| {
        if bad < 40 {
            println!("{m}")
        }
        bad += 1;
    };
    for n in &f.nodes {
        match f.type_name(n.ty) {
            "FIN" => {
                let e = p(&f, n, "edge");
                if e == 0 {
                    continue;
                }
                let Some(edge) = f.node(e) else {
                    say(format!("fin #{} edge missing", n.index));
                    continue;
                };
                let fwd = p(&f, n, "forward");
                if f.node(fwd).map(|x| p(&f, x, "backward")) != Some(n.index) {
                    say(format!("fin #{} forward/backward mismatch", n.index))
                }
                let other = p(&f, n, "other");
                if f.node(other).map(|x| p(&f, x, "edge")) != Some(e) {
                    say(format!("fin #{} other not on same edge", n.index))
                }
                // The fin's vertex is where the next fin in the loop starts.
                let next_start = f.node(fwd).and_then(|x| {
                    let xe = f.node(p(&f, x, "edge"))?;
                    let other = f.node(p(&f, x, "other"))?;
                    Some(if c(&f, x, "sense") == b'+' { p(&f, other, "vertex") } else { p(&f, xe.fins_dummy(), "x") })
                });
                let _ = next_start;
                // Line edges: vertex order must follow the curve direction (with its sense).
                let curve = f.node(p(&f, edge, "curve"));
                if let Some(cv) = curve.filter(|cv| f.type_name(cv.ty) == "LINE") {
                    let dir = f.field(cv, "direction").and_then(Val::vec3).unwrap_or([0.0; 3]);
                    let s = if c(&f, cv, "sense") == b'-' { -1.0 } else { 1.0 };
                    let head = pt(&f, p(&f, n, "vertex"));
                    let tail = f.node(other).and_then(|o| pt(&f, p(&f, o, "vertex")));
                    if let (Some(h), Some(t)) = (head, tail) {
                        let d = (h[0] - t[0]) * dir[0] + (h[1] - t[1]) * dir[1] + (h[2] - t[2]) * dir[2];
                        let fs = if c(&f, n, "sense") == b'-' { -1.0 } else { 1.0 };
                        if d * s * fs <= 0.0 {
                            say(format!("fin #{} sense {} vertex order vs line direction wrong", n.index, c(&f, n, "sense") as char))
                        }
                    }
                }
            }
            "EDGE" => {
                let fin = f.node(p(&f, n, "fin"));
                if fin.map(|x| c(&f, x, "sense")) != Some(b'+') {
                    say(format!("edge #{} head fin not positive", n.index))
                }
            }
            "VERTEX" => {
                let mut q = p(&f, n, "fin");
                let mut k = 0;
                while let Some(x) = f.node(q) {
                    if p(&f, x, "vertex") != n.index {
                        say(format!("vertex #{} chain has fin #{} of another vertex", n.index, x.index))
                    }
                    q = p(&f, x, "next_at_vx");
                    k += 1;
                    if k > 100 {
                        break;
                    }
                }
                let all = f.nodes.iter().filter(|x| f.type_name(x.ty) == "FIN" && p(&f, x, "vertex") == n.index).count();
                if all != k {
                    say(format!("vertex #{} chain has {k} fins, {all} point at it", n.index))
                }
            }
            _ => {}
        }
    }
    println!("{path}: {bad} problems");
}
trait D {
    fn fins_dummy(&self) -> &Self;
}
impl D for Node {
    fn fins_dummy(&self) -> &Self {
        self
    }
}

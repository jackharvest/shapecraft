//! 2D imports as sketch elements: DXF (ASCII) and SVG. Results are in millimetres on the sketch plane's x/y.

use glam::{DAffine2, DVec2};
use shapecraft_doc::SketchEntity;

/// Most elements an import accepts.
const MAX_ELEMENTS: usize = 50_000;

fn ok(p: DVec2) -> bool {
    p.is_finite() && p.abs().max_element() < 1e8
}

fn arc_through(center: DVec2, r: f64, a0: f64, a1: f64) -> SketchEntity {
    let at = |t: f64| center + DVec2::new(t.cos(), t.sin()) * r;
    SketchEntity::Arc { a: at(a0), mid: at((a0 + a1) / 2.0), b: at(a1) }
}

/// A polyline segment with a DXF bulge (tan of a quarter of the included angle, + = counter-clockwise).
fn bulge_segment(a: DVec2, b: DVec2, bulge: f64) -> SketchEntity {
    let d = b - a;
    let len = d.length();
    if bulge.abs() < 1e-9 || len < 1e-12 {
        return SketchEntity::Line { a, b };
    }
    let right = DVec2::new(d.y, -d.x) / len;
    SketchEntity::Arc { a, mid: (a + b) / 2.0 + right * (bulge * len / 2.0), b }
}

struct Out(Vec<SketchEntity>);

impl Out {
    fn push(&mut self, e: SketchEntity) -> Result<(), String> {
        let good = match &e {
            SketchEntity::Line { a, b } => ok(*a) && ok(*b) && a.distance(*b) > 1e-9,
            SketchEntity::Circle { center, radius } => ok(*center) && radius.is_finite() && *radius > 1e-9,
            SketchEntity::Arc { a, mid, b } => ok(*a) && ok(*mid) && ok(*b) && a.distance(*b) > 1e-9,
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                ok(*center) && rx.is_finite() && ry.is_finite() && *rx > 1e-9 && *ry > 1e-9 && angle.is_finite()
            }
            SketchEntity::Spline { points } => points.len() >= 2 && points.iter().all(|p| ok(*p)),
            SketchEntity::ControlSpline { poles } => poles.len() >= 2 && poles.iter().all(|p| ok(*p)),
            _ => true,
        };
        if good {
            if self.0.len() >= MAX_ELEMENTS {
                return Err(format!("the file has more than {MAX_ELEMENTS} elements"));
            }
            self.0.push(e);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------- DXF

/// Reads the model-space LINE, CIRCLE, ARC, LWPOLYLINE, POLYLINE, ELLIPSE and SPLINE entities of an ASCII DXF.
pub fn read_dxf(text: &str) -> Result<Vec<SketchEntity>, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let pairs: Vec<(i32, &str)> = lines.as_chunks::<2>().0.iter().filter_map(|c| Some((c[0].parse().ok()?, c[1]))).collect();
    if pairs.is_empty() {
        return Err("not a DXF file (binary DXF isn't supported)".into());
    }
    // $INSUNITS: 1 inch, 2 feet, 4 mm, 5 cm, 6 m; anything else is taken as millimetres.
    let mut scale = 1.0;
    if let Some(i) = pairs.iter().position(|&(c, v)| c == 9 && v == "$INSUNITS")
        && let Some(&(70, v)) = pairs.get(i + 1)
    {
        scale = match v.trim() {
            "1" => 25.4,
            "2" => 304.8,
            "5" => 10.0,
            "6" => 1000.0,
            _ => 1.0,
        };
    }
    let start = pairs.windows(2).position(|w| w[0] == (0, "SECTION") && w[1] == (2, "ENTITIES")).map_or(0, |i| i + 2);
    // Split into entities: each starts at a group-0 pair.
    let mut ents: Vec<(&str, Vec<(i32, &str)>)> = Vec::new();
    for &(code, value) in &pairs[start..] {
        if code == 0 {
            if value == "ENDSEC" {
                break;
            }
            ents.push((value, Vec::new()));
        } else if let Some(last) = ents.last_mut() {
            last.1.push((code, value));
        }
    }
    let mut out = Out(Vec::new());
    let num = |g: &[(i32, &str)], code: i32| g.iter().find(|p| p.0 == code).and_then(|p| p.1.trim().parse::<f64>().ok());
    let all =
        |g: &[(i32, &str)], code: i32| -> Vec<f64> { g.iter().filter(|p| p.0 == code).filter_map(|p| p.1.trim().parse().ok()).collect() };
    let pt = |g: &[(i32, &str)], cx: i32| -> Option<DVec2> { Some(DVec2::new(num(g, cx)?, num(g, cx + 10)?) * scale) };
    let mut i = 0;
    while i < ents.len() {
        let (kind, g) = (&ents[i].0, &ents[i].1);
        // Paper-space entities (group 67 = 1) aren't part of the drawing's geometry.
        if num(g, 67) == Some(1.0) {
            i += 1;
            continue;
        }
        match *kind {
            "LINE" => {
                if let (Some(a), Some(b)) = (pt(g, 10), pt(g, 11)) {
                    out.push(SketchEntity::Line { a, b })?;
                }
            }
            "CIRCLE" => {
                if let (Some(c), Some(r)) = (pt(g, 10), num(g, 40)) {
                    out.push(SketchEntity::Circle { center: c, radius: r * scale })?;
                }
            }
            "ARC" => {
                if let (Some(c), Some(r), Some(a0), Some(a1)) = (pt(g, 10), num(g, 40), num(g, 50), num(g, 51)) {
                    let (a0, mut a1) = (a0.to_radians(), a1.to_radians());
                    while a1 <= a0 {
                        a1 += std::f64::consts::TAU;
                    }
                    out.push(arc_through(c, r * scale, a0, a1))?;
                }
            }
            "LWPOLYLINE" => {
                let closed = num(g, 70).is_some_and(|f| (f as i64) & 1 != 0);
                // Vertices in order; a bulge (42) belongs to the vertex before it.
                let mut verts: Vec<(DVec2, f64)> = Vec::new();
                let mut x = None;
                for &(c, v) in g {
                    let Ok(f) = v.trim().parse::<f64>() else { continue };
                    match c {
                        10 => x = Some(f),
                        20 => {
                            if let Some(x) = x.take() {
                                verts.push((DVec2::new(x, f) * scale, 0.0));
                            }
                        }
                        42 => {
                            if let Some(last) = verts.last_mut() {
                                last.1 = f;
                            }
                        }
                        _ => {}
                    }
                }
                polyline(&mut out, &verts, closed)?;
            }
            "POLYLINE" => {
                let closed = num(g, 70).is_some_and(|f| (f as i64) & 1 != 0);
                let mut verts = Vec::new();
                while i + 1 < ents.len() && ents[i + 1].0 == "VERTEX" {
                    i += 1;
                    let v = &ents[i].1;
                    if let Some(p) = pt(v, 10) {
                        verts.push((p, num(v, 42).unwrap_or(0.0)));
                    }
                }
                polyline(&mut out, &verts, closed)?;
            }
            "ELLIPSE" => {
                if let (Some(c), Some(major), Some(ratio)) = (pt(g, 10), pt(g, 11), num(g, 40)) {
                    let (t0, t1) = (num(g, 41).unwrap_or(0.0), num(g, 42).unwrap_or(std::f64::consts::TAU));
                    let (rx, ry, angle) = (major.length(), major.length() * ratio, major.y.atan2(major.x));
                    if (t1 - t0 - std::f64::consts::TAU).abs() < 1e-6 || (t1 - t0).abs() < 1e-9 {
                        out.push(SketchEntity::Ellipse { center: c, rx, ry, angle })?;
                    } else {
                        let mut t1 = t1;
                        while t1 <= t0 {
                            t1 += std::f64::consts::TAU;
                        }
                        let rot = DVec2::from_angle(angle);
                        let points = (0..=24)
                            .map(|k| t0 + (t1 - t0) * k as f64 / 24.0)
                            .map(|t| c + rot.rotate(DVec2::new(rx * t.cos(), ry * t.sin())))
                            .collect();
                        out.push(SketchEntity::Spline { points })?;
                    }
                }
            }
            "SPLINE" => {
                let fit: Vec<DVec2> = all(g, 11).into_iter().zip(all(g, 21)).map(|(x, y)| DVec2::new(x, y) * scale).collect();
                let poles: Vec<DVec2> = all(g, 10).into_iter().zip(all(g, 20)).map(|(x, y)| DVec2::new(x, y) * scale).collect();
                if fit.len() >= 2 {
                    out.push(SketchEntity::Spline { points: fit })?;
                } else if poles.len() == 2 {
                    out.push(SketchEntity::Line { a: poles[0], b: poles[1] })?;
                } else if poles.len() >= 3 {
                    out.push(SketchEntity::ControlSpline { poles })?;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if out.0.is_empty() {
        return Err("the DXF file has no lines, arcs or curves to import".into());
    }
    Ok(out.0)
}

fn polyline(out: &mut Out, verts: &[(DVec2, f64)], closed: bool) -> Result<(), String> {
    for w in verts.windows(2) {
        out.push(bulge_segment(w[0].0, w[1].0, w[0].1))?;
    }
    if closed
        && verts.len() > 2
        && let (Some(last), Some(first)) = (verts.last(), verts.first())
    {
        out.push(bulge_segment(last.0, first.0, last.1))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------- SVG

/// One parsed tag: name, attributes, and whether it opens a group that a later `</g>` closes.
struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, String)>,
    closing: bool,
    self_closing: bool,
}

fn tags(text: &str) -> Vec<Tag<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        if rest.starts_with("!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        if rest.starts_with('?') || rest.starts_with('!') {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
            continue;
        }
        // Find the closing '>' outside quotes.
        let (mut end, mut quote) = (None, None);
        for (i, c) in rest.char_indices() {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let body = &rest[..end];
        rest = &rest[end + 1..];
        let closing = body.starts_with('/');
        let body = body.trim_start_matches('/');
        let self_closing = body.ends_with('/');
        let body = body.trim_end_matches('/');
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let name = body[..name_end].rsplit(':').next().unwrap_or("");
        let mut attrs = Vec::new();
        let mut a = &body[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim();
            let after = a[eq + 1..].trim_start();
            let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
            let Some(close) = after[1..].find(q) else { break };
            attrs.push((key.rsplit(':').next().unwrap_or(key), after[1..1 + close].to_string()));
            a = &after[close + 2..];
        }
        out.push(Tag { name, attrs, closing, self_closing });
    }
    out
}

fn numbers(s: &str) -> Vec<f64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'.' {
            let st = i;
            let mut seen_dot = c == b'.';
            let mut seen_e = false;
            i += 1;
            while i < b.len() {
                let d = b[i];
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == b'.' && !seen_dot && !seen_e {
                    seen_dot = true;
                    i += 1;
                } else if (d == b'e' || d == b'E') && !seen_e {
                    seen_e = true;
                    i += 1;
                    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = s[st..i].parse() {
                out.push(v);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn length_mm(v: &str) -> Option<f64> {
    let v = v.trim();
    let n = numbers(v).first().copied()?;
    let per = if v.ends_with("mm") {
        1.0
    } else if v.ends_with("cm") {
        10.0
    } else if v.ends_with("in") {
        25.4
    } else if v.ends_with("pt") {
        25.4 / 72.0
    } else if v.ends_with("pc") {
        25.4 / 6.0
    } else if v.ends_with('%') {
        return None;
    } else {
        25.4 / 96.0
    };
    Some(n * per)
}

fn transform(s: &str) -> DAffine2 {
    let mut m = DAffine2::IDENTITY;
    for part in s.split(')') {
        let Some((name, args)) = part.split_once('(') else { continue };
        let a = numbers(args);
        let g = |i: usize, d: f64| a.get(i).copied().unwrap_or(d);
        let t = match name.trim().trim_start_matches(',').trim() {
            "matrix" if a.len() >= 6 => DAffine2::from_cols_array(&[a[0], a[1], a[2], a[3], a[4], a[5]]),
            "translate" => DAffine2::from_translation(DVec2::new(g(0, 0.0), g(1, 0.0))),
            "scale" => DAffine2::from_scale(DVec2::new(g(0, 1.0), g(1, g(0, 1.0)))),
            "rotate" => {
                let c = DVec2::new(g(1, 0.0), g(2, 0.0));
                DAffine2::from_translation(c) * DAffine2::from_angle(g(0, 0.0).to_radians()) * DAffine2::from_translation(-c)
            }
            "skewX" => DAffine2::from_cols_array(&[1.0, 0.0, g(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0]),
            "skewY" => DAffine2::from_cols_array(&[1.0, g(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => DAffine2::IDENTITY,
        };
        m *= t;
    }
    m
}

/// Uniform scale of a similarity transform (rotation/reflection + one scale), or `None` for anything else.
fn similarity(m: &DAffine2) -> Option<f64> {
    let (x, y) = (m.matrix2.x_axis, m.matrix2.y_axis);
    let (lx, ly) = (x.length(), y.length());
    ((lx - ly).abs() <= 1e-9 * lx.max(1.0) && x.dot(y).abs() <= 1e-9 * lx * ly).then_some(lx)
}

/// Builds sketch elements in the SVG's transformed space.
struct SvgOut {
    out: Out,
    m: DAffine2,
}

impl SvgOut {
    fn p(&self, v: DVec2) -> DVec2 {
        self.m.transform_point2(v)
    }

    fn line(&mut self, a: DVec2, b: DVec2) -> Result<(), String> {
        self.out.push(SketchEntity::Line { a: self.p(a), b: self.p(b) })
    }

    fn cubic(&mut self, p0: DVec2, p1: DVec2, p2: DVec2, p3: DVec2) -> Result<(), String> {
        // A straight Bézier is a line.
        let d = p3 - p0;
        let off = |q: DVec2| if d.length() > 1e-12 { (q - p0).perp_dot(d).abs() / d.length() } else { (q - p0).length() };
        if off(p1) < 1e-9 && off(p2) < 1e-9 {
            return self.line(p0, p3);
        }
        self.out.push(SketchEntity::ControlSpline { poles: vec![self.p(p0), self.p(p1), self.p(p2), self.p(p3)] })
    }

    /// An elliptical arc in center form, as an arc when it's circular under a similarity, else as Béziers.
    fn ellipse_arc(&mut self, c: DVec2, rx: f64, ry: f64, phi: f64, t0: f64, dt: f64) -> Result<(), String> {
        let rot = DVec2::from_angle(phi);
        let at = |t: f64| c + rot.rotate(DVec2::new(rx * t.cos(), ry * t.sin()));
        if (rx - ry).abs() <= 1e-9 * rx.max(1.0) && similarity(&self.m).is_some() {
            let pieces = (dt.abs() / 3.0).ceil().max(1.0) as usize; // under ~172° each
            for k in 0..pieces {
                let (a, b) = (t0 + dt * k as f64 / pieces as f64, t0 + dt * (k + 1) as f64 / pieces as f64);
                self.out.push(SketchEntity::Arc { a: self.p(at(a)), mid: self.p(at((a + b) / 2.0)), b: self.p(at(b)) })?;
            }
            return Ok(());
        }
        let pieces = (dt.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let tan = |t: f64| rot.rotate(DVec2::new(-rx * t.sin(), ry * t.cos()));
        for k in 0..pieces {
            let (a, b) = (t0 + dt * k as f64 / pieces as f64, t0 + dt * (k + 1) as f64 / pieces as f64);
            let h = 4.0 / 3.0 * ((b - a) / 4.0).tan();
            self.cubic(at(a), at(a) + tan(a) * h, at(b) - tan(b) * h, at(b))?;
        }
        Ok(())
    }

    fn full_ellipse(&mut self, c: DVec2, rx: f64, ry: f64) -> Result<(), String> {
        match similarity(&self.m) {
            Some(s) if (rx - ry).abs() <= 1e-9 * rx.max(1.0) => self.out.push(SketchEntity::Circle { center: self.p(c), radius: rx * s }),
            Some(s) => {
                let x = self.m.matrix2.x_axis;
                self.out.push(SketchEntity::Ellipse { center: self.p(c), rx: rx * s, ry: ry * s, angle: x.y.atan2(x.x) })
            }
            None => self.ellipse_arc(c, rx, ry, 0.0, 0.0, std::f64::consts::TAU),
        }
    }

    fn path(&mut self, d: &str) -> Result<(), String> {
        // Tokens: command letters and numbers, in order.
        let mut toks: Vec<Result<char, f64>> = Vec::new();
        let mut num = String::new();
        let flush = |num: &mut String, toks: &mut Vec<Result<char, f64>>| {
            if !num.is_empty() {
                toks.extend(numbers(num).into_iter().map(Err));
                num.clear();
            }
        };
        for c in d.chars() {
            if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
                flush(&mut num, &mut toks);
                toks.push(Ok(c));
            } else {
                num.push(c);
            }
        }
        flush(&mut num, &mut toks);
        let (mut cur, mut start, mut last_ctrl) = (DVec2::ZERO, DVec2::ZERO, None::<(char, DVec2)>);
        let mut i = 0;
        let mut cmd = 'M';
        while i < toks.len() {
            if let Ok(c) = toks[i] {
                cmd = c;
                i += 1;
                if c == 'Z' || c == 'z' {
                    if cur.distance(start) > 1e-9 {
                        self.line(cur, start)?;
                    }
                    cur = start;
                    last_ctrl = None;
                    continue;
                }
            }
            let rel = cmd.is_ascii_lowercase();
            let need = match cmd.to_ascii_uppercase() {
                'M' | 'L' | 'T' => 2,
                'H' | 'V' => 1,
                'C' => 6,
                'S' | 'Q' => 4,
                'A' => 7,
                _ => return Err(format!("unknown SVG path command '{cmd}'")),
            };
            let mut a = Vec::with_capacity(need);
            while a.len() < need {
                match toks.get(i) {
                    Some(Err(v)) => {
                        a.push(*v);
                        i += 1;
                    }
                    _ => break,
                }
            }
            if a.len() < need {
                // A command letter cut short: skip to the next command.
                while matches!(toks.get(i), Some(Err(_))) {
                    i += 1;
                }
                continue;
            }
            let base = if rel { cur } else { DVec2::ZERO };
            let v = |k: usize| base + DVec2::new(a[k], a[k + 1]);
            let up = cmd.to_ascii_uppercase();
            let mut ctrl = None;
            match up {
                'M' => {
                    cur = v(0);
                    start = cur;
                    // Coordinates after a moveto are linetos.
                    cmd = if rel { 'l' } else { 'L' };
                }
                'L' => {
                    let p = v(0);
                    self.line(cur, p)?;
                    cur = p;
                }
                'H' => {
                    let p = DVec2::new(if rel { cur.x + a[0] } else { a[0] }, cur.y);
                    self.line(cur, p)?;
                    cur = p;
                }
                'V' => {
                    let p = DVec2::new(cur.x, if rel { cur.y + a[0] } else { a[0] });
                    self.line(cur, p)?;
                    cur = p;
                }
                'C' | 'S' => {
                    let (c1, c2, p) = if up == 'C' {
                        (v(0), v(2), v(4))
                    } else {
                        let c1 = match last_ctrl {
                            Some(('C', c)) => cur * 2.0 - c,
                            _ => cur,
                        };
                        (c1, v(0), v(2))
                    };
                    self.cubic(cur, c1, c2, p)?;
                    ctrl = Some(('C', c2));
                    cur = p;
                }
                'Q' | 'T' => {
                    let (q, p) = if up == 'Q' {
                        (v(0), v(2))
                    } else {
                        let q = match last_ctrl {
                            Some(('Q', c)) => cur * 2.0 - c,
                            _ => cur,
                        };
                        (q, v(0))
                    };
                    self.cubic(cur, cur + (q - cur) * (2.0 / 3.0), p + (q - p) * (2.0 / 3.0), p)?;
                    ctrl = Some(('Q', q));
                    cur = p;
                }
                'A' => {
                    let p = v(5);
                    self.svg_arc(cur, a[0].abs(), a[1].abs(), a[2].to_radians(), a[3] != 0.0, a[4] != 0.0, p)?;
                    cur = p;
                }
                _ => {}
            }
            last_ctrl = ctrl;
        }
        Ok(())
    }

    /// SVG endpoint arc → center form (SVG 1.1 implementation notes, F.6.5).
    #[allow(clippy::too_many_arguments)]
    fn svg_arc(&mut self, p0: DVec2, mut rx: f64, mut ry: f64, phi: f64, large: bool, sweep: bool, p1: DVec2) -> Result<(), String> {
        if rx < 1e-12 || ry < 1e-12 {
            return self.line(p0, p1);
        }
        if p0.distance(p1) < 1e-12 {
            return Ok(());
        }
        let rot = DVec2::from_angle(-phi);
        let d = rot.rotate((p0 - p1) / 2.0);
        let lam = (d.x * d.x) / (rx * rx) + (d.y * d.y) / (ry * ry);
        if lam > 1.0 {
            rx *= lam.sqrt();
            ry *= lam.sqrt();
        }
        let num = (rx * rx * ry * ry - rx * rx * d.y * d.y - ry * ry * d.x * d.x).max(0.0);
        let den = rx * rx * d.y * d.y + ry * ry * d.x * d.x;
        let mut k = if den > 0.0 { (num / den).sqrt() } else { 0.0 };
        if large == sweep {
            k = -k;
        }
        let cp = DVec2::new(k * rx * d.y / ry, -k * ry * d.x / rx);
        let c = DVec2::from_angle(phi).rotate(cp) + (p0 + p1) / 2.0;
        let ang = |u: DVec2| u.y.atan2(u.x);
        let t0 = ang(DVec2::new((d.x - cp.x) / rx, (d.y - cp.y) / ry));
        let t1 = ang(DVec2::new((-d.x - cp.x) / rx, (-d.y - cp.y) / ry));
        let mut dt = t1 - t0;
        if sweep && dt < 0.0 {
            dt += std::f64::consts::TAU;
        } else if !sweep && dt > 0.0 {
            dt -= std::f64::consts::TAU;
        }
        self.ellipse_arc(c, rx, ry, phi, t0, dt)
    }
}

/// Reads an SVG's path, line, polyline, polygon, rect, circle and ellipse shapes (with transforms) in millimetres,
/// y up. Rounded rectangle corners are left square.
pub fn read_svg(text: &str) -> Result<Vec<SketchEntity>, String> {
    let all = tags(text);
    let root = all.iter().find(|t| t.name == "svg" && !t.closing).ok_or("not an SVG file")?;
    let attr = |t: &Tag, k: &str| t.attrs.iter().find(|a| a.0 == k).map(|a| a.1.clone());
    // User units → mm from width/height against the viewBox; plain numbers are CSS pixels (96 per inch).
    let view = attr(root, "viewBox").map(|v| numbers(&v)).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    let width = attr(root, "width").and_then(|w| length_mm(&w));
    let unit = match (&view, width) {
        (Some(v), Some(w)) => w / v[2],
        _ => 25.4 / 96.0,
    };
    let origin = view.as_ref().map_or(DVec2::ZERO, |v| DVec2::new(v[0], v[1]));
    let base = DAffine2::from_scale(DVec2::new(unit, -unit)) * DAffine2::from_translation(-origin);
    let mut stack = vec![base];
    let mut s = SvgOut { out: Out(Vec::new()), m: base };
    let mut skip_depth = 0usize;
    for t in &all {
        let top = stack.last().copied().unwrap_or(base);
        if t.closing {
            if matches!(t.name, "defs" | "clipPath" | "mask" | "symbol" | "pattern" | "marker") {
                skip_depth = skip_depth.saturating_sub(1);
            } else if t.name == "g" || t.name == "a" || t.name == "svg" {
                stack.pop();
            }
            continue;
        }
        if matches!(t.name, "defs" | "clipPath" | "mask" | "symbol" | "pattern" | "marker") {
            if !t.self_closing {
                skip_depth += 1;
            }
            continue;
        }
        if skip_depth > 0 {
            continue;
        }
        let m = top * attr(t, "transform").map_or(DAffine2::IDENTITY, |v| transform(&v));
        if (t.name == "g" || t.name == "a" || (t.name == "svg" && !std::ptr::eq(t, root))) && !t.self_closing {
            stack.push(m);
            continue;
        }
        s.m = m;
        let n = |k: &str| attr(t, k).and_then(|v| numbers(&v).first().copied()).unwrap_or(0.0);
        let points = || {
            attr(t, "points")
                .map(|v| numbers(&v))
                .unwrap_or_default()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| DVec2::new(c[0], c[1]))
                .collect::<Vec<_>>()
        };
        match t.name {
            "path" => {
                if let Some(d) = attr(t, "d") {
                    s.path(&d)?;
                }
            }
            "line" => s.line(DVec2::new(n("x1"), n("y1")), DVec2::new(n("x2"), n("y2")))?,
            "polyline" | "polygon" => {
                let p = points();
                for w in p.windows(2) {
                    s.line(w[0], w[1])?;
                }
                if t.name == "polygon"
                    && p.len() > 2
                    && let (Some(a), Some(b)) = (p.last(), p.first())
                {
                    s.line(*a, *b)?;
                }
            }
            "rect" => {
                let (x, y, w, h) = (n("x"), n("y"), n("width"), n("height"));
                if w > 0.0 && h > 0.0 {
                    let c = [DVec2::new(x, y), DVec2::new(x + w, y), DVec2::new(x + w, y + h), DVec2::new(x, y + h)];
                    for k in 0..4 {
                        s.line(c[k], c[(k + 1) % 4])?;
                    }
                }
            }
            "circle" => {
                let r = n("r");
                if r > 0.0 {
                    s.full_ellipse(DVec2::new(n("cx"), n("cy")), r, r)?;
                }
            }
            "ellipse" => {
                let (rx, ry) = (n("rx"), n("ry"));
                if rx > 0.0 && ry > 0.0 {
                    s.full_ellipse(DVec2::new(n("cx"), n("cy")), rx, ry)?;
                }
            }
            _ => {}
        }
    }
    if s.out.0.is_empty() {
        return Err("the SVG file has no shapes to import".into());
    }
    Ok(s.out.0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn dxf_lines_arcs_and_bulged_polylines() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n0\nARC\n10\n0\n20\n0\n40\n5\n50\n0\n51\n90\n0\nLWPOLYLINE\n90\n2\n70\n0\n10\n0\n20\n0\n42\n1\n10\n2\n20\n0\n0\nCIRCLE\n10\n1\n20\n1\n40\n2\n0\nENDSEC\n0\nEOF\n";
        let e = read_dxf(dxf).unwrap();
        assert_eq!(e.len(), 4);
        assert!(matches!(e[0], SketchEntity::Line { b, .. } if (b.x - 10.0).abs() < 1e-9));
        let SketchEntity::Arc { mid, .. } = e[1] else { panic!("arc") };
        assert!((mid.length() - 5.0).abs() < 1e-9 && mid.x > 0.0 && mid.y > 0.0);
        // Bulge 1 from (0,0) to (2,0) is a counter-clockwise half circle, below the chord.
        let SketchEntity::Arc { mid, .. } = e[2] else { panic!("bulge arc") };
        assert!((mid - DVec2::new(1.0, -1.0)).length() < 1e-9);
        assert!(matches!(e[3], SketchEntity::Circle { radius, .. } if (radius - 2.0).abs() < 1e-9));
    }

    #[test]
    fn svg_shapes_in_millimetres_with_y_up() {
        let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="50mm" viewBox="0 0 200 100">
            <g transform="translate(10,0)"><rect x="0" y="0" width="20" height="10"/></g>
            <circle cx="100" cy="50" r="10"/>
            <path d="M0 0 C 10 10, 20 10, 30 0 Q 40 -10 50 0 A 5 5 0 0 1 60 0 Z"/>
            <defs><rect width="5" height="5"/></defs></svg>"#;
        let e = read_svg(svg).unwrap();
        // 4 rect sides + circle + cubic + quad + arc pieces + closing line.
        assert!(
            matches!(e[0], SketchEntity::Line { a, b } if (a - DVec2::new(5.0, 0.0)).length() < 1e-9 && (b - DVec2::new(15.0, 0.0)).length() < 1e-9)
        );
        assert!(matches!(e[2], SketchEntity::Line { b, .. } if (b - DVec2::new(5.0, -5.0)).length() < 1e-9));
        assert!(
            matches!(e[4], SketchEntity::Circle { center, radius } if (center - DVec2::new(50.0, -25.0)).length() < 1e-9 && (radius - 5.0).abs() < 1e-9)
        );
        assert!(matches!(e[5], SketchEntity::ControlSpline { ref poles } if poles.len() == 4));
        assert!(e.iter().any(|x| matches!(x, SketchEntity::Arc { .. })));
        assert!(matches!(e.last(), Some(SketchEntity::Line { b, .. }) if b.length() < 1e-9));
    }
}

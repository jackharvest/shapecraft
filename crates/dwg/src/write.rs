//! Assembles an R2000 file (spec chapter 3): file header and section locators, an empty preview, the
//! header variables, classes, objects, object map, the free-space and template sections.

use std::collections::HashMap;

use crate::bits::{BitWriter, crc16, modular_char_signed, modular_char_unsigned, modular_short};
use crate::{Drawing, Entity, Error, HAlign, Layer, LineWeight, Result, Shape, VAlign};

pub(crate) const SN_FILE: [u8; 16] = [0x95, 0xA0, 0x4E, 0x28, 0x99, 0x82, 0x1A, 0xE5, 0x5E, 0x41, 0xE0, 0x5F, 0x9D, 0x3A, 0x4D, 0x00];
pub(crate) const SN_HEADER: [u8; 16] = [0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F];
pub(crate) const SN_HEADER_END: [u8; 16] = [0x30, 0x84, 0xE0, 0xDC, 0x02, 0x21, 0xC7, 0x56, 0xA0, 0x83, 0x97, 0x47, 0xB1, 0x92, 0xCC, 0xA0];
pub(crate) const SN_CLASSES: [u8; 16] = [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];
pub(crate) const SN_CLASSES_END: [u8; 16] =
    [0x72, 0x5E, 0x3B, 0x47, 0x3B, 0x56, 0x07, 0x3A, 0x3F, 0x23, 0x0B, 0xA0, 0x18, 0x30, 0x49, 0x75];
const SN_IMAGE: [u8; 16] = [0x1F, 0x25, 0x6D, 0x07, 0xD4, 0x36, 0x28, 0x28, 0x9D, 0x57, 0xCA, 0x3F, 0x9D, 0x44, 0x10, 0x2B];
const SN_IMAGE_END: [u8; 16] = [0xE0, 0xDA, 0x92, 0xF8, 0x2B, 0xC9, 0xD7, 0xD7, 0x62, 0xA8, 0x35, 0xC0, 0x62, 0xBB, 0xEF, 0xD4];

/// Handle reference codes (spec 2.13).
const SOFT_OWNER: u8 = 2;
const HARD_OWNER: u8 = 3;
const SOFT_PTR: u8 = 4;
const HARD_PTR: u8 = 5;

/// Fixed object type codes (spec 20.3).
pub(crate) mod ty {
    pub const TEXT: u16 = 1;
    pub const BLOCK: u16 = 4;
    pub const ENDBLK: u16 = 5;
    pub const ARC: u16 = 0x11;
    pub const CIRCLE: u16 = 0x12;
    pub const LINE: u16 = 0x13;
    pub const SPLINE: u16 = 0x24;
    pub const DICTIONARY: u16 = 0x2A;
    pub const MTEXT: u16 = 0x2C;
    pub const BLOCK_CONTROL: u16 = 0x30;
    pub const BLOCK_HEADER: u16 = 0x31;
    pub const LAYER_CONTROL: u16 = 0x32;
    pub const LAYER: u16 = 0x33;
    pub const STYLE_CONTROL: u16 = 0x34;
    pub const STYLE: u16 = 0x35;
    pub const LTYPE_CONTROL: u16 = 0x38;
    pub const LTYPE: u16 = 0x39;
    pub const VIEW_CONTROL: u16 = 0x3C;
    pub const UCS_CONTROL: u16 = 0x3E;
    pub const VPORT_CONTROL: u16 = 0x40;
    pub const VPORT: u16 = 0x41;
    pub const APPID_CONTROL: u16 = 0x42;
    pub const APPID: u16 = 0x43;
    pub const DIMSTYLE_CONTROL: u16 = 0x44;
    pub const DIMSTYLE: u16 = 0x45;
    pub const VPENT_CONTROL: u16 = 0x46;
    pub const MLINESTYLE: u16 = 0x49;
    // Classes (the order of CLASSES below, numbered from 500).
    pub const DICTIONARYWDFLT: u16 = 500;
    pub const PLACEHOLDER: u16 = 501;
    pub const LAYOUT: u16 = 502;
    pub const LWPOLYLINE: u16 = 503;
}

/// (dxf name, C++ class, application, item class id) – numbered 500, 501, … in this order.
const CLASSES: [(&str, &str, &str, i16); 4] = [
    ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault", "ObjectDBX Classes", 0x1F3),
    ("ACDBPLACEHOLDER", "AcDbPlaceHolder", "ObjectDBX Classes", 0x1F3),
    ("LAYOUT", "AcDbLayout", "ObjectDBX Classes", 0x1F3),
    ("LWPOLYLINE", "AcDbPolyline", "ObjectDBX Classes", 0x1F2),
];

/// Standard lineweights in hundredths of a millimetre, by their DWG index.
const LINEWEIGHTS: [u16; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

fn lineweight_index(w: LineWeight) -> u8 {
    match w {
        LineWeight::ByLayer => 29,
        LineWeight::ByBlock => 30,
        LineWeight::Default => 31,
        LineWeight::Mm(mm) => {
            let target = mm * 100.0;
            let mut best = 0;
            for (i, &v) in LINEWEIGHTS.iter().enumerate() {
                if (v as f64 - target).abs() < (LINEWEIGHTS[best] as f64 - target).abs() {
                    best = i;
                }
            }
            best as u8
        }
    }
}

/// Text for a TV field in the ANSI_1252 drawing: ASCII as is, everything else as `\U+XXXX`, controls dropped.
fn text_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '..='~' => out.push(c as u8),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            c if (c as u32) <= 0xFFFF => out.extend_from_slice(format!("\\U+{:04X}", c as u32).as_bytes()),
            _ => out.push(b'?'),
        }
    }
    out
}

fn tv(w: &mut BitWriter, s: &str) {
    w.tv(&text_bytes(s));
}

/// One object being written: its data stream and its handle stream.
struct Obj {
    d: BitWriter,
    h: BitWriter,
    size_at: usize,
}

impl Obj {
    fn start(kind: u16, handle: u64) -> Obj {
        let mut d = BitWriter::new();
        d.bs(kind as i16);
        let size_at = d.bit_len();
        d.rl(0); // size of the data in bits, patched in `finish`
        d.h(0, handle);
        d.bs(0); // no extended data
        Obj { d, h: BitWriter::new(), size_at }
    }

    /// A non-entity object owned by `owner` (soft pointer), no reactors, no extension dictionary.
    fn object(kind: u16, handle: u64, owner: u64) -> Obj {
        let mut o = Obj::start(kind, handle);
        o.d.bl(0);
        o.h.h(SOFT_PTR, owner);
        o.h.h(HARD_OWNER, 0);
        o
    }

    /// A symbol table record: common object data, then name and flags; the xref block handle follows.
    fn record(kind: u16, handle: u64, control: u64, name: &str) -> Obj {
        let mut o = Obj::object(kind, handle, control);
        tv(&mut o.d, name);
        o.d.bit(true); // the 64 flag (referenced), as AutoCAD keeps it
        o.d.bs(0); // xref index + 1
        o.d.bit(false); // not xref dependent
        o.h.h(HARD_PTR, 0);
        o
    }

    /// A symbol table control object listing `entries` (soft owner).
    fn control(kind: u16, handle: u64, entries: &[u64]) -> Obj {
        let mut o = Obj::object(kind, handle, 0);
        o.d.bl(entries.len() as i32);
        for &e in entries {
            o.h.h(SOFT_OWNER, e);
        }
        o
    }

    fn entity(kind: u16, handle: u64, c: &Common) -> Obj {
        let mut o = Obj::start(kind, handle);
        o.d.bit(false); // no graphic image
        o.d.bb(c.mode);
        o.d.bl(0); // reactors
        o.d.bit(false); // previous/next links are stored
        o.d.bs(c.color);
        o.d.bd(1.0); // linetype scale
        o.d.bb(c.ltype_flags);
        o.d.bb(0); // plot style ByLayer
        o.d.bs(0); // visible
        o.d.rc(c.lineweight);
        o.h.h(HARD_OWNER, 0); // no extension dictionary
        o.h.h(SOFT_PTR, c.prev);
        o.h.h(SOFT_PTR, c.next);
        o.h.h(HARD_PTR, c.layer);
        if c.ltype_flags == 3 {
            o.h.h(HARD_PTR, c.ltype);
        }
        o
    }

    /// Size, data, handles, padding and CRC.
    fn finish(mut self) -> Vec<u8> {
        let bits = self.d.bit_len() as u32;
        self.d.patch(self.size_at, u32::from_be_bytes(bits.to_le_bytes()) as u64, 32);
        self.d.append(&self.h);
        let body = self.d.into_bytes();
        let mut out = modular_short(body.len() as u32);
        out.extend_from_slice(&body);
        let crc = crc16(0xC0C1, &out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }
}

/// Common entity data that varies.
struct Common {
    /// 0 = owner handle stored, 1 = paper space, 2 = model space.
    mode: u8,
    color: i16,
    ltype_flags: u8,
    ltype: u64,
    lineweight: u8,
    layer: u64,
    prev: u64,
    next: u64,
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite() && x.abs() < 1e15)
}

fn check_name(kind: &str, name: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > 255 || name.chars().any(|c| "<>/\\\":;?*|=`,".contains(c) || c.is_control()) {
        return Err(Error::Bad(format!("{kind} name {name:?} is empty, too long or has a character DWG names can't hold")));
    }
    Ok(())
}

fn check_entity(e: &Entity) -> Result<()> {
    let bad = |what: &str| Err(Error::Bad(format!("{what} on layer {:?}", e.layer)));
    match &e.shape {
        Shape::Line { a, b } => {
            if !finite(a) || !finite(b) {
                return bad("a line with an invalid point");
            }
        }
        Shape::Arc { center, radius, start, end } => {
            if !finite(center) || !finite(&[*radius, *start, *end]) || *radius <= 0.0 {
                return bad("an arc with an invalid center, radius or angle");
            }
        }
        Shape::Circle { center, radius } => {
            if !finite(center) || !radius.is_finite() || *radius <= 0.0 {
                return bad("a circle with an invalid center or radius");
            }
        }
        Shape::Polyline { points, bulges, .. } => {
            if points.len() < 2 || points.iter().any(|p| !finite(p)) || !finite(bulges) {
                return bad("a polyline with fewer than two points or an invalid one");
            }
            if !bulges.is_empty() && bulges.len() != points.len() {
                return bad("a polyline whose bulges don't match its points");
            }
        }
        Shape::Spline { degree, knots, control, weights, .. } => {
            let d = *degree as usize;
            if !(1..=25).contains(&d)
                || control.len() <= d
                || knots.len() != control.len() + d + 1
                || control.iter().any(|p| !finite(p))
                || !finite(knots)
                || knots.windows(2).any(|w| w[1] < w[0])
                || (!weights.is_empty() && (weights.len() != control.len() || weights.iter().any(|w| !w.is_finite() || *w <= 0.0)))
            {
                return bad("an invalid spline (degree, knots, control points or weights)");
            }
        }
        Shape::Text { at, height, rotation, value, .. } => {
            if !finite(at) || !finite(&[*height, *rotation]) || *height <= 0.0 || value.len() > 30000 {
                return bad("text with an invalid point, height or over-long value");
            }
        }
        Shape::MText { at, height, width, attachment, value } => {
            if !finite(at)
                || !finite(&[*height, *width])
                || *height <= 0.0
                || *width < 0.0
                || !(1..=9).contains(attachment)
                || value.len() > 30000
            {
                return bad("paragraph text with an invalid point, height, width, attachment or over-long value");
            }
        }
    }
    Ok(())
}

/// Grows `lo`/`hi` to hold the entity.
fn extend_bounds(e: &Shape, lo: &mut [f64; 3], hi: &mut [f64; 3]) {
    let mut add = |p: [f64; 3]| {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    };
    match e {
        Shape::Line { a, b } => {
            add(*a);
            add(*b);
        }
        Shape::Arc { center: c, radius: r, .. } | Shape::Circle { center: c, radius: r } => {
            add([c[0] - r, c[1] - r, c[2]]);
            add([c[0] + r, c[1] + r, c[2]]);
        }
        Shape::Polyline { points, .. } => points.iter().for_each(|p| add([p[0], p[1], 0.0])),
        Shape::Spline { control, .. } => control.iter().for_each(|p| add(*p)),
        Shape::Text { at, height, value, .. } | Shape::MText { at, height, value, .. } => {
            add(*at);
            let w = value.chars().count() as f64 * height * 0.7;
            add([at[0] + w, at[1] + height, at[2]]);
        }
    }
}

/// Writes the drawing as an R2000 DWG.
pub fn write(dwg: &Drawing) -> Result<Vec<u8>> {
    if dwg.entities.len() > 5_000_000 || dwg.layers.len() > 10_000 || dwg.linetypes.len() > 1_000 {
        return Err(Error::Bad("too many entities, layers or linetypes".into()));
    }
    // ---- names and checks ----
    let mut layers: Vec<Layer> = vec![Layer::new("0", 7)];
    for l in &dwg.layers {
        check_name("layer", &l.name)?;
        if !(1..=255).contains(&l.color) {
            return Err(Error::Bad(format!("layer {:?} color must be 1–255", l.name)));
        }
        if matches!(l.lineweight, LineWeight::Mm(mm) if !mm.is_finite() || mm < 0.0) {
            return Err(Error::Bad(format!("layer {:?} has an invalid lineweight", l.name)));
        }
        if l.name == "0" {
            layers[0] = l.clone();
        } else if layers.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)) {
            return Err(Error::Bad(format!("layer {:?} is defined twice", l.name)));
        } else {
            layers.push(l.clone());
        }
    }
    for t in &dwg.linetypes {
        check_name("linetype", &t.name)?;
        if ["bylayer", "byblock", "continuous"].contains(&t.name.to_ascii_lowercase().as_str())
            || dwg.linetypes.iter().filter(|x| x.name.eq_ignore_ascii_case(&t.name)).count() > 1
        {
            return Err(Error::Bad(format!("linetype {:?} is built in or defined twice", t.name)));
        }
        if t.dashes.len() > 12 || !finite(&t.dashes) {
            return Err(Error::Bad(format!("linetype {:?} needs 0–12 finite dash lengths", t.name)));
        }
    }
    for e in &dwg.entities {
        check_entity(e)?;
        if matches!(e.lineweight, LineWeight::Mm(mm) if !mm.is_finite() || mm < 0.0) {
            return Err(Error::Bad(format!("an entity on layer {:?} has an invalid lineweight", e.layer)));
        }
    }

    // ---- handles ----
    let mut next = 1u64;
    let mut alloc = || {
        let h = next;
        next += 1;
        h
    };
    let h_block_ctrl = alloc();
    let h_layer_ctrl = alloc();
    let h_style_ctrl = alloc();
    let h_ltype_ctrl = alloc();
    let h_view_ctrl = alloc();
    let h_ucs_ctrl = alloc();
    let h_vport_ctrl = alloc();
    let h_appid_ctrl = alloc();
    let h_dimstyle_ctrl = alloc();
    let h_vpent_ctrl = alloc();
    let h_nod = alloc();
    let h_groups = alloc();
    let h_mlstyles = alloc();
    let h_layouts = alloc();
    let h_plotsettings = alloc();
    let h_plotstyles = alloc();
    let h_placeholder = alloc();
    let h_mlstyle = alloc();
    let h_byblock = alloc();
    let h_bylayer = alloc();
    let h_continuous = alloc();
    let h_style = alloc();
    let h_appid = alloc();
    let h_dimstyle = alloc();
    let h_vport = alloc();
    let h_ms = alloc();
    let h_ps = alloc();
    let h_ms_block = alloc();
    let h_ms_endblk = alloc();
    let h_ps_block = alloc();
    let h_ps_endblk = alloc();
    let h_model_layout = alloc();
    let h_paper_layout = alloc();
    let h_ltypes: Vec<u64> = dwg.linetypes.iter().map(|_| alloc()).collect();
    let h_layers: Vec<u64> = layers.iter().map(|_| alloc()).collect();
    let h_ents: Vec<u64> = dwg.entities.iter().map(|_| alloc()).collect();
    let handseed = next;

    let mut ltype_by_name: HashMap<String, u64> = HashMap::new();
    ltype_by_name.insert("continuous".into(), h_continuous);
    for (t, &h) in dwg.linetypes.iter().zip(&h_ltypes) {
        ltype_by_name.insert(t.name.to_ascii_lowercase(), h);
    }
    let layer_ltype = |l: &Layer| -> Result<u64> {
        match &l.linetype {
            None => Ok(h_continuous),
            Some(n) => ltype_by_name
                .get(&n.to_ascii_lowercase())
                .copied()
                .ok_or_else(|| Error::Bad(format!("layer {:?} uses undefined linetype {n:?}", l.name))),
        }
    };
    let mut layer_by_name: HashMap<String, u64> = HashMap::new();
    for (l, &h) in layers.iter().zip(&h_layers) {
        layer_by_name.insert(l.name.to_ascii_lowercase(), h);
    }

    // ---- extents ----
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for e in &dwg.entities {
        extend_bounds(&e.shape, &mut lo, &mut hi);
    }
    if dwg.entities.is_empty() {
        (lo, hi) = ([0.0; 3], [0.0; 3]);
    }
    let (lim_lo, lim_hi) = match dwg.limits {
        Some((a, b)) if finite(&a) && finite(&b) && b[0] > a[0] && b[1] > a[1] => (a, b),
        _ if hi[0] > lo[0] && hi[1] > lo[1] => ([lo[0], lo[1]], [hi[0], hi[1]]),
        _ => ([0.0, 0.0], [420.0, 297.0]),
    };
    let (date_day, date_ms) = {
        let (y, m, d) = dwg.date.unwrap_or((2000, 1, 1));
        let jd =
            if (1..=12).contains(&m) && (1..=31).contains(&d) && (1600..=9999).contains(&y) { crate::julian_day(y, m, d) } else { 2451545 };
        (jd as i32, 43_200_000i32)
    };

    // ---- objects ----
    let mut objects: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut put = |h: u64, o: Obj| objects.push((h, o.finish()));

    put(h_block_ctrl, {
        let mut o = Obj::control(ty::BLOCK_CONTROL, h_block_ctrl, &[]);
        o.h.h(HARD_OWNER, h_ms);
        o.h.h(HARD_OWNER, h_ps);
        o
    });
    put(h_layer_ctrl, Obj::control(ty::LAYER_CONTROL, h_layer_ctrl, &h_layers));
    put(h_style_ctrl, Obj::control(ty::STYLE_CONTROL, h_style_ctrl, &[h_style]));
    put(h_ltype_ctrl, {
        let entries: Vec<u64> = std::iter::once(h_continuous).chain(h_ltypes.iter().copied()).collect();
        let mut o = Obj::control(ty::LTYPE_CONTROL, h_ltype_ctrl, &entries);
        o.h.h(HARD_OWNER, h_byblock);
        o.h.h(HARD_OWNER, h_bylayer);
        o
    });
    put(h_view_ctrl, Obj::control(ty::VIEW_CONTROL, h_view_ctrl, &[]));
    put(h_ucs_ctrl, Obj::control(ty::UCS_CONTROL, h_ucs_ctrl, &[]));
    put(h_vport_ctrl, Obj::control(ty::VPORT_CONTROL, h_vport_ctrl, &[h_vport]));
    put(h_appid_ctrl, Obj::control(ty::APPID_CONTROL, h_appid_ctrl, &[h_appid]));
    put(h_dimstyle_ctrl, {
        let mut o = Obj::control(ty::DIMSTYLE_CONTROL, h_dimstyle_ctrl, &[h_dimstyle]);
        o.d.rc(0); // R2000: count of extra hard-owner handles (DXF 71), none
        o
    });
    put(h_vpent_ctrl, Obj::control(ty::VPENT_CONTROL, h_vpent_ctrl, &[]));

    // Dictionaries.
    let dict = |kind: u16, handle: u64, owner: u64, items: &[(&str, u64)]| {
        let mut o = Obj::object(kind, handle, owner);
        o.d.bl(items.len() as i32);
        o.d.bs(1); // cloning: keep existing
        o.d.rc(0); // not a hard owner
        for (name, h) in items {
            tv(&mut o.d, name);
            o.h.h(SOFT_OWNER, *h);
        }
        o
    };
    put(
        h_nod,
        dict(
            ty::DICTIONARY,
            h_nod,
            0,
            &[
                ("ACAD_GROUP", h_groups),
                ("ACAD_LAYOUT", h_layouts),
                ("ACAD_MLINESTYLE", h_mlstyles),
                ("ACAD_PLOTSETTINGS", h_plotsettings),
                ("ACAD_PLOTSTYLENAME", h_plotstyles),
            ],
        ),
    );
    put(h_groups, dict(ty::DICTIONARY, h_groups, h_nod, &[]));
    put(h_mlstyles, dict(ty::DICTIONARY, h_mlstyles, h_nod, &[("Standard", h_mlstyle)]));
    put(h_layouts, dict(ty::DICTIONARY, h_layouts, h_nod, &[("Model", h_model_layout), ("Layout1", h_paper_layout)]));
    put(h_plotsettings, dict(ty::DICTIONARY, h_plotsettings, h_nod, &[]));
    put(h_plotstyles, {
        let mut o = dict(ty::DICTIONARYWDFLT, h_plotstyles, h_nod, &[("Normal", h_placeholder)]);
        o.h.h(HARD_PTR, h_placeholder);
        o
    });
    put(h_placeholder, Obj::object(ty::PLACEHOLDER, h_placeholder, h_plotstyles));
    put(h_mlstyle, {
        let mut o = Obj::object(ty::MLINESTYLE, h_mlstyle, h_mlstyles);
        tv(&mut o.d, "Standard");
        tv(&mut o.d, "");
        o.d.bs(0);
        o.d.bs(256); // fill colour ByLayer
        o.d.bd(std::f64::consts::FRAC_PI_2);
        o.d.bd(std::f64::consts::FRAC_PI_2);
        o.d.rc(2);
        for off in [0.5, -0.5] {
            o.d.bd(off);
            o.d.bs(256);
            o.d.bs(i16::MAX); // linetype index: ByLayer
        }
        o
    });

    // Linetypes.
    let ltype = |handle: u64, name: &str, description: &str, dashes: &[f64]| {
        let mut o = Obj::record(ty::LTYPE, handle, h_ltype_ctrl, name);
        tv(&mut o.d, description);
        o.d.bd(dashes.iter().map(|d| d.abs()).sum());
        o.d.rc(b'A');
        o.d.rc(dashes.len() as u8);
        for &d in dashes {
            o.d.bd(d);
            o.d.bs(0); // shape number
            o.d.rd(0.0);
            o.d.rd(0.0);
            o.d.bd(1.0);
            o.d.bd(0.0);
            o.d.bs(0); // shape flags: a plain dash
            o.h.h(HARD_PTR, 0); // no shape file
        }
        for _ in 0..256 {
            o.d.rc(0); // text area
        }
        o
    };
    put(h_byblock, ltype(h_byblock, "ByBlock", "", &[]));
    put(h_bylayer, ltype(h_bylayer, "ByLayer", "", &[]));
    put(h_continuous, ltype(h_continuous, "Continuous", "Solid line", &[]));
    for (t, &h) in dwg.linetypes.iter().zip(&h_ltypes) {
        put(h, ltype(h, &t.name, &t.description, &t.dashes));
    }

    // Layers.
    for (l, &h) in layers.iter().zip(&h_layers) {
        let lt = layer_ltype(l)?;
        let mut o = Obj::record(ty::LAYER, h, h_layer_ctrl, &l.name);
        o.d.bs(0x10 | ((lineweight_index(l.lineweight) as i16 & 0x1F) << 5)); // plotted, lineweight
        o.d.bs(l.color as i16);
        o.h.h(HARD_PTR, h_placeholder); // plot style
        o.h.h(HARD_PTR, lt);
        put(h, o);
    }

    put(h_style, {
        let mut o = Obj::record(ty::STYLE, h_style, h_style_ctrl, "Standard");
        o.d.bit(false); // not vertical
        o.d.bit(false); // a font, not a shape file
        o.d.bd(0.0); // no fixed height
        o.d.bd(1.0); // width factor
        o.d.bd(0.0); // oblique
        o.d.rc(0); // generation
        o.d.bd(2.5); // last height
        tv(&mut o.d, "txt");
        tv(&mut o.d, "");
        o
    });
    put(h_appid, {
        let mut o = Obj::record(ty::APPID, h_appid, h_appid_ctrl, "ACAD");
        o.d.rc(0);
        o
    });
    put(h_dimstyle, dimstyle(h_dimstyle, h_dimstyle_ctrl, h_style));
    put(h_vport, {
        let mut o = Obj::record(ty::VPORT, h_vport, h_vport_ctrl, "*Active");
        let (w, ht) = ((hi[0] - lo[0]).max(1.0), (hi[1] - lo[1]).max(1.0));
        let height = (ht * 1.1).max(w * 1.1 / 1.6);
        o.d.bd(height);
        o.d.bd(height * 1.6); // aspect ratio × height
        o.d.rd2([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]);
        o.d.bd3([0.0, 0.0, 0.0]); // target
        o.d.bd3([0.0, 0.0, 1.0]); // direction
        o.d.bd(0.0); // twist
        o.d.bd(50.0); // lens
        o.d.bd(0.0);
        o.d.bd(0.0);
        o.d.bits(0b0001, 4); // view mode
        o.d.rc(0); // render mode
        o.d.rd2([0.0, 0.0]);
        o.d.rd2([1.0, 1.0]);
        o.d.bit(false); // UCSFOLLOW
        o.d.bs(1000); // circle zoom
        o.d.bit(true); // fast zoom
        o.d.bb(3); // UCS icon on, at origin
        o.d.bit(false); // grid off
        o.d.rd2([10.0, 10.0]);
        o.d.bit(false); // snap off
        o.d.bit(false);
        o.d.bs(0);
        o.d.bd(0.0);
        o.d.rd2([0.0, 0.0]);
        o.d.rd2([10.0, 10.0]);
        o.d.bit(false);
        o.d.bit(false); // UCS per viewport
        o.d.bd3([0.0, 0.0, 0.0]);
        o.d.bd3([1.0, 0.0, 0.0]);
        o.d.bd3([0.0, 1.0, 0.0]);
        o.d.bd(0.0);
        o.d.bs(0);
        o.h.h(HARD_PTR, 0); // named UCS
        o.h.h(HARD_PTR, 0); // base UCS
        o
    });

    // Block records, their BLOCK/ENDBLK and layouts.
    let first = h_ents.first().copied().unwrap_or(0);
    let last = h_ents.last().copied().unwrap_or(0);
    let block_record = |handle: u64, name: &str, block: u64, endblk: u64, first: u64, last: u64, layout: u64| {
        let mut o = Obj::record(ty::BLOCK_HEADER, handle, h_block_ctrl, name);
        for _ in 0..5 {
            o.d.bit(false); // anonymous, has attributes, xref, overlaid, (xref) loaded
        }
        o.d.bd3([0.0, 0.0, 0.0]);
        tv(&mut o.d, ""); // xref path
        o.d.rc(0); // no inserts
        tv(&mut o.d, ""); // description
        o.d.bl(0); // no preview
        o.h.h(HARD_OWNER, block);
        o.h.h(SOFT_PTR, first);
        o.h.h(SOFT_PTR, last);
        o.h.h(HARD_OWNER, endblk);
        o.h.h(HARD_PTR, layout);
        o
    };
    put(h_ms, block_record(h_ms, "*Model_Space", h_ms_block, h_ms_endblk, first, last, h_model_layout));
    put(h_ps, block_record(h_ps, "*Paper_Space", h_ps_block, h_ps_endblk, 0, 0, h_paper_layout));
    let layer0 = h_layers.first().copied().unwrap_or(0);
    let plain = |mode: u8| Common { mode, color: 256, ltype_flags: 0, ltype: 0, lineweight: 29, layer: layer0, prev: 0, next: 0 };
    for (h, mode, name) in [(h_ms_block, 2, "*Model_Space"), (h_ps_block, 1, "*Paper_Space")] {
        let mut o = Obj::entity(ty::BLOCK, h, &plain(mode));
        tv(&mut o.d, name);
        put(h, o);
    }
    for (h, mode) in [(h_ms_endblk, 2), (h_ps_endblk, 1)] {
        put(h, Obj::entity(ty::ENDBLK, h, &plain(mode)));
    }
    let paper = [lim_hi[0] - lim_lo[0], lim_hi[1] - lim_lo[1]];
    for (h, name, tab, record, flags) in [(h_model_layout, "Model", 0, h_ms, 0x400), (h_paper_layout, "Layout1", 1, h_ps, 0)] {
        let mut o = Obj::object(ty::LAYOUT, h, h_layouts);
        tv(&mut o.d, ""); // page setup name
        tv(&mut o.d, "none_device");
        o.d.bs(flags);
        for m in [7.5, 20.0, 7.5, 20.0] {
            o.d.bd(m);
        }
        o.d.bd(paper[0]);
        o.d.bd(paper[1]);
        tv(&mut o.d, "");
        o.d.bd2([0.0, 0.0]); // plot origin
        o.d.bs(1); // millimetres
        o.d.bs(0); // no rotation
        o.d.bs(5); // plot the layout
        o.d.bd2([0.0, 0.0]);
        o.d.bd2([0.0, 0.0]);
        o.d.tv(b""); // plot view name
        o.d.bd(1.0);
        o.d.bd(1.0);
        tv(&mut o.d, ""); // style sheet
        o.d.bs(16); // scale 1:1
        o.d.bd(1.0);
        o.d.bd2([0.0, 0.0]);
        tv(&mut o.d, name);
        o.d.bl(tab);
        o.d.bs(1); // layout flags
        o.d.bd3([0.0, 0.0, 0.0]); // UCS origin
        o.d.rd2(lim_lo);
        o.d.rd2(lim_hi);
        o.d.bd3([0.0, 0.0, 0.0]); // insertion base
        o.d.bd3([1.0, 0.0, 0.0]);
        o.d.bd3([0.0, 1.0, 0.0]);
        o.d.bd(0.0);
        o.d.bs(1); // orthographic UCS: top
        o.d.bd3(lo);
        o.d.bd3(hi);
        o.h.h(SOFT_PTR, record);
        o.h.h(SOFT_PTR, 0); // last active viewport
        o.h.h(HARD_PTR, 0); // base UCS
        o.h.h(HARD_PTR, 0); // named UCS
        put(h, o);
    }

    // Entities, chained in model space.
    for (i, e) in dwg.entities.iter().enumerate() {
        let h = h_ents[i];
        let layer = layer_by_name
            .get(&e.layer.to_ascii_lowercase())
            .copied()
            .ok_or_else(|| Error::Bad(format!("an entity uses undefined layer {:?}", e.layer)))?;
        let (ltype_flags, lt) = match e.linetype.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("bylayer") => (0, 0),
            Some("byblock") => (1, 0),
            Some(n) => (3, ltype_by_name.get(n).copied().ok_or_else(|| Error::Bad(format!("an entity uses undefined linetype {n:?}")))?),
        };
        let c = Common {
            mode: 2,
            color: e.color.map_or(256, |c| c as i16),
            ltype_flags,
            ltype: lt,
            lineweight: lineweight_index(e.lineweight),
            layer,
            prev: if i == 0 { 0 } else { h_ents[i - 1] },
            next: h_ents.get(i + 1).copied().unwrap_or(0),
        };
        put(h, entity(h, &c, &e.shape, h_style));
    }

    // ---- sections ----
    let header = header_vars(&HeaderRefs {
        handseed,
        layer0,
        style: h_style,
        bylayer: h_bylayer,
        byblock: h_byblock,
        continuous: h_continuous,
        dimstyle: h_dimstyle,
        mlstyle: h_mlstyle,
        controls: [
            h_block_ctrl,
            h_layer_ctrl,
            h_style_ctrl,
            h_ltype_ctrl,
            h_view_ctrl,
            h_ucs_ctrl,
            h_vport_ctrl,
            h_appid_ctrl,
            h_dimstyle_ctrl,
            h_vpent_ctrl,
        ],
        groups: h_groups,
        mlstyles: h_mlstyles,
        nod: h_nod,
        layouts: h_layouts,
        plotsettings: h_plotsettings,
        plotstyles: h_plotstyles,
        ms: h_ms,
        ps: h_ps,
        ext: (lo, hi),
        limits: (lim_lo, lim_hi),
        date: (date_day, date_ms),
        guid: guid(&objects),
    });
    let classes = classes_section();

    const RECORDS: usize = 5;
    let header_len = 0x19 + RECORDS * 9 + 2 + 16;
    let mut out = vec![0u8; header_len];
    // Empty preview.
    let image_at = out.len();
    out.extend_from_slice(&SN_IMAGE);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.push(0);
    out.extend_from_slice(&SN_IMAGE_END);
    let header_at = out.len();
    out.extend_from_slice(&header);
    let classes_at = out.len();
    out.extend_from_slice(&classes);
    out.extend_from_slice(&[0u8; 0x200]); // padding (R13C3+)
    let objects_at = out.len();
    objects.sort_by_key(|(h, _)| *h);
    let mut map: Vec<(u64, usize)> = Vec::with_capacity(objects.len());
    for (h, bytes) in &objects {
        map.push((*h, out.len()));
        out.extend_from_slice(bytes);
    }
    let map_at = out.len();
    out.extend_from_slice(&object_map(&map));
    let free_at = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(objects.len() as u32).to_le_bytes());
    out.extend_from_slice(&(date_day as u32).to_le_bytes());
    out.extend_from_slice(&(date_ms as u32).to_le_bytes());
    out.extend_from_slice(&(objects_at as u32).to_le_bytes());
    out.push(4);
    for v in [0x32u32, 0, 0x64, 0, 0x200, 0, 0xFFFF_FFFF, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let template_at = out.len();
    out.extend_from_slice(&0u16.to_le_bytes()); // no template description
    out.extend_from_slice(&1u16.to_le_bytes()); // MEASUREMENT: metric
    let end = out.len();
    if end > u32::MAX as usize {
        return Err(Error::Bad("drawing too large for R2000".into()));
    }

    // File header.
    out[..6].copy_from_slice(b"AC1015");
    out[6..13].copy_from_slice(&[0, 0, 0, 0, 0, 0, 1]);
    out[0x0D..0x11].copy_from_slice(&(image_at as u32).to_le_bytes());
    out[0x11] = 0;
    out[0x12] = 0;
    out[0x13..0x15].copy_from_slice(&30u16.to_le_bytes()); // ANSI_1252
    out[0x15..0x19].copy_from_slice(&(RECORDS as u32).to_le_bytes());
    let records: [(u8, usize, usize); RECORDS] = [
        (0, header_at, header.len()),
        (1, classes_at, classes.len()),
        (2, map_at, free_at - map_at),
        (3, free_at, template_at - free_at),
        (4, template_at, end - template_at),
    ];
    for (i, (nr, at, len)) in records.iter().enumerate() {
        let p = 0x19 + i * 9;
        out[p] = *nr;
        out[p + 1..p + 5].copy_from_slice(&(*at as u32).to_le_bytes());
        out[p + 5..p + 9].copy_from_slice(&(*len as u32).to_le_bytes());
    }
    let crc_at = 0x19 + RECORDS * 9;
    let crc = crc16(0, &out[..crc_at]) ^ 0x3CC4; // five records
    out[crc_at..crc_at + 2].copy_from_slice(&crc.to_le_bytes());
    out[crc_at + 2..crc_at + 18].copy_from_slice(&SN_FILE);
    Ok(out)
}

fn entity(h: u64, c: &Common, shape: &Shape, style: u64) -> Obj {
    match shape {
        Shape::Line { a, b } => {
            let mut o = Obj::entity(ty::LINE, h, c);
            let flat = a[2] == 0.0 && b[2] == 0.0;
            o.d.bit(flat);
            o.d.rd(a[0]);
            o.d.dd(b[0], a[0]);
            o.d.rd(a[1]);
            o.d.dd(b[1], a[1]);
            if !flat {
                o.d.rd(a[2]);
                o.d.dd(b[2], a[2]);
            }
            o.d.bt(0.0);
            o.d.be([0.0, 0.0, 1.0]);
            o
        }
        Shape::Arc { center, radius, start, end } => {
            let mut o = Obj::entity(ty::ARC, h, c);
            o.d.bd3(*center);
            o.d.bd(*radius);
            o.d.bt(0.0);
            o.d.be([0.0, 0.0, 1.0]);
            o.d.bd(*start);
            o.d.bd(*end);
            o
        }
        Shape::Circle { center, radius } => {
            let mut o = Obj::entity(ty::CIRCLE, h, c);
            o.d.bd3(*center);
            o.d.bd(*radius);
            o.d.bt(0.0);
            o.d.be([0.0, 0.0, 1.0]);
            o
        }
        Shape::Polyline { points, bulges, closed } => {
            let mut o = Obj::entity(ty::LWPOLYLINE, h, c);
            let has_bulges = bulges.iter().any(|b| *b != 0.0);
            o.d.bs(if has_bulges { 16 } else { 0 } | if *closed { 512 } else { 0 });
            o.d.bl(points.len() as i32);
            if has_bulges {
                o.d.bl(bulges.len() as i32);
            }
            let mut prev = [0.0; 2];
            for (i, p) in points.iter().enumerate() {
                if i == 0 {
                    o.d.rd2(*p);
                } else {
                    o.d.dd(p[0], prev[0]);
                    o.d.dd(p[1], prev[1]);
                }
                prev = *p;
            }
            if has_bulges {
                for b in bulges {
                    o.d.bd(*b);
                }
            }
            o
        }
        Shape::Spline { degree, knots, control, weights, closed } => {
            let mut o = Obj::entity(ty::SPLINE, h, c);
            o.d.bl(1); // control points and knots
            o.d.bl(*degree as i32);
            let rational = !weights.is_empty();
            o.d.bit(rational);
            o.d.bit(*closed);
            o.d.bit(false); // not periodic
            o.d.bd(1e-10);
            o.d.bd(1e-10);
            o.d.bl(knots.len() as i32);
            o.d.bl(control.len() as i32);
            o.d.bit(rational);
            for k in knots {
                o.d.bd(*k);
            }
            for (i, p) in control.iter().enumerate() {
                o.d.bd3(*p);
                if rational {
                    o.d.bd(weights.get(i).copied().unwrap_or(1.0));
                }
            }
            o
        }
        Shape::Text { at, height, rotation, value, h_align, v_align } => {
            let mut o = Obj::entity(ty::TEXT, h, c);
            let ha: i16 = match h_align {
                HAlign::Left => 0,
                HAlign::Center => 1,
                HAlign::Right => 2,
            };
            let va: i16 = match v_align {
                VAlign::Baseline => 0,
                VAlign::Bottom => 1,
                VAlign::Middle => 2,
                VAlign::Top => 3,
            };
            let aligned = ha != 0 || va != 0;
            let mut flags = 0x04 | 0x10 | 0x20; // no oblique, width factor 1, normal generation
            if at[2] == 0.0 {
                flags |= 0x01;
            }
            if !aligned {
                flags |= 0x02;
            }
            if *rotation == 0.0 {
                flags |= 0x08;
            }
            if ha == 0 {
                flags |= 0x40;
            }
            if va == 0 {
                flags |= 0x80;
            }
            o.d.rc(flags);
            if flags & 0x01 == 0 {
                o.d.rd(at[2]);
            }
            o.d.rd2([at[0], at[1]]);
            if aligned {
                o.d.dd(at[0], at[0]);
                o.d.dd(at[1], at[1]);
            }
            o.d.be([0.0, 0.0, 1.0]);
            o.d.bt(0.0);
            if flags & 0x08 == 0 {
                o.d.rd(*rotation);
            }
            o.d.rd(*height);
            tv(&mut o.d, value);
            if ha != 0 {
                o.d.bs(ha);
            }
            if va != 0 {
                o.d.bs(va);
            }
            o.h.h(HARD_PTR, style);
            o
        }
        Shape::MText { at, height, width, attachment, value } => {
            let mut o = Obj::entity(ty::MTEXT, h, c);
            o.d.bd3(*at);
            o.d.bd3([0.0, 0.0, 1.0]);
            o.d.bd3([1.0, 0.0, 0.0]);
            o.d.bd(*width);
            o.d.bd(*height);
            o.d.bs(*attachment as i16);
            o.d.bs(1); // left to right
            let lines = value.split('\n').count().max(1) as f64;
            let longest = value.split('\n').map(|l| l.chars().count()).max().unwrap_or(0) as f64;
            o.d.bd(lines * height * 1.667);
            o.d.bd(longest * height * 0.7);
            let text = value.replace("\r\n", "\n").split('\n').collect::<Vec<_>>().join("\\P");
            tv(&mut o.d, &text);
            o.d.bs(1); // line spacing: at least
            o.d.bd(1.0);
            o.d.bit(false);
            o.h.h(HARD_PTR, style);
            o
        }
    }
}

fn dimstyle(handle: u64, control: u64, style: u64) -> Obj {
    let mut o = Obj::record(ty::DIMSTYLE, handle, control, "Standard");
    let d = &mut o.d;
    tv(d, ""); // DIMPOST
    tv(d, ""); // DIMAPOST
    for v in [1.0, 2.5, 0.625, 3.75, 1.25, 0.0, 0.0, 0.0, 0.0] {
        d.bd(v); // DIMSCALE DIMASZ DIMEXO DIMDLI DIMEXE DIMRND DIMDLE DIMTP DIMTM
    }
    for _ in 0..6 {
        d.bit(false); // DIMTOL DIMLIM DIMTIH DIMTOH DIMSE1 DIMSE2
    }
    d.bs(1); // DIMTAD
    d.bs(8); // DIMZIN
    d.bs(0); // DIMAZIN
    for v in [2.5, 2.5, 0.0, 0.03937007874015748, 1.0, 0.0, 1.0, 0.625, 0.0] {
        d.bd(v); // DIMTXT DIMCEN DIMTSZ DIMALTF DIMLFAC DIMTVP DIMTFAC DIMGAP DIMALTRND
    }
    d.bit(false); // DIMALT
    d.bs(3); // DIMALTD
    d.bit(true); // DIMTOFL
    d.bit(false); // DIMSAH
    d.bit(false); // DIMTIX
    d.bit(false); // DIMSOXD
    for _ in 0..3 {
        d.bs(0); // DIMCLRD DIMCLRE DIMCLRT: ByBlock
    }
    for v in [2, 2, 2, 2, 3, 0, 0, 2, 44, 0, 0] {
        d.bs(v); // DIMADEC DIMDEC DIMTDEC DIMALTU DIMALTTD DIMAUNIT DIMFRAC DIMLUNIT DIMDSEP DIMTMOVE DIMJUST
    }
    d.bit(false); // DIMSD1
    d.bit(false); // DIMSD2
    for v in [0, 8, 0, 0] {
        d.bs(v); // DIMTOLJ DIMTZIN DIMALTZ DIMALTTZ
    }
    d.bit(false); // DIMUPT
    d.bs(3); // DIMFIT
    d.bs(-2); // DIMLWD ByBlock
    d.bs(-2); // DIMLWE ByBlock
    d.bit(false);
    o.h.h(HARD_PTR, style); // DIMTXSTY
    for _ in 0..4 {
        o.h.h(HARD_PTR, 0); // DIMLDRBLK DIMBLK DIMBLK1 DIMBLK2
    }
    o
}

struct HeaderRefs {
    handseed: u64,
    layer0: u64,
    style: u64,
    bylayer: u64,
    byblock: u64,
    continuous: u64,
    dimstyle: u64,
    mlstyle: u64,
    controls: [u64; 10],
    groups: u64,
    mlstyles: u64,
    nod: u64,
    layouts: u64,
    plotsettings: u64,
    plotstyles: u64,
    ms: u64,
    ps: u64,
    ext: ([f64; 3], [f64; 3]),
    limits: ([f64; 2], [f64; 2]),
    date: (i32, i32),
    guid: String,
}

/// A stable GUID-shaped string from the object bytes (FNV-1a), so equal drawings give equal files.
fn guid(objects: &[(u64, Vec<u8>)]) -> String {
    let mut a: u64 = 0xcbf2_9ce4_8422_2325;
    let mut b: u64 = 0x8422_2325_cbf2_9ce4;
    for (h, bytes) in objects {
        for &x in h.to_le_bytes().iter().chain(bytes) {
            a = (a ^ x as u64).wrapping_mul(0x100_0000_01b3);
            b = (b ^ a.rotate_left(17)).wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("{{{:08X}-{:04X}-{:04X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xFFFF, a & 0xFFFF, b >> 48, b & 0xFFFF_FFFF_FFFF)
}

/// The header variables section (spec chapter 9), R2000 fields only.
fn header_vars(r: &HeaderRefs) -> Vec<u8> {
    let mut d = BitWriter::new();
    d.bd(412148564080.0);
    d.bd(1.0);
    d.bd(1.0);
    d.bd(1.0);
    for s in ["m", "", "", ""] {
        tv(&mut d, s);
    }
    d.bl(24);
    d.bl(0);
    d.h(HARD_PTR, 0); // current viewport entity header
    // DIMASO DIMSHO PLINEGEN ORTHOMODE REGENMODE FILLMODE QTEXTMODE PSLTSCALE LIMCHECK USRTIMER SKPOLY ANGDIR
    // SPLFRAME MIRRTEXT WORLDVIEW TILEMODE PLIMCHECK VISRETAIN DISPSILH PELLIPSE
    for b in
        [true, true, false, false, true, true, false, true, false, true, false, false, false, true, true, true, false, true, false, false]
    {
        d.bit(b);
    }
    d.bs(1); // PROXYGRAPHICS
    d.bs(3020); // TREEDEPTH
    d.bs(2); // LUNITS decimal
    d.bs(4); // LUPREC
    d.bs(0); // AUNITS
    d.bs(2); // AUPREC
    d.bs(1); // ATTMODE
    d.bs(0); // PDMODE
    for _ in 0..5 {
        d.bs(0); // USERI1-5
    }
    // SPLINESEGS SURFU SURFV SURFTYPE SURFTAB1 SURFTAB2 SPLINETYPE SHADEDGE SHADEDIF UNITMODE MAXACTVP ISOLINES CMLJUST TEXTQLTY
    for v in [8, 6, 6, 6, 6, 6, 6, 3, 70, 0, 64, 4, 0, 50] {
        d.bs(v);
    }
    // LTSCALE TEXTSIZE TRACEWID SKETCHINC FILLETRAD THICKNESS ANGBASE PDSIZE PLINEWID USERR1-5 CHAMFERA-D FACETRES CMLSCALE CELTSCALE
    for v in [1.0, 2.5, 1.0, 1.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 20.0, 1.0] {
        d.bd(v);
    }
    tv(&mut d, "acad"); // MENUNAME
    let (day, ms) = r.date;
    for v in [day, ms, day, ms, 0, 0, 0, 0] {
        d.bl(v); // TDCREATE TDUPDATE TDINDWG TDUSRTIMER
    }
    d.bs(256); // CECOLOR ByLayer
    d.h(0, r.handseed);
    d.h(HARD_PTR, r.layer0); // CLAYER
    d.h(HARD_PTR, r.style); // TEXTSTYLE
    d.h(HARD_PTR, r.bylayer); // CELTYPE
    d.h(HARD_PTR, r.dimstyle);
    d.h(HARD_PTR, r.mlstyle);
    d.bd(0.0); // PSVPSCALE
    let (lo, hi) = r.ext;
    let (lim_lo, lim_hi) = r.limits;
    for space in 0..2 {
        d.bd3([0.0, 0.0, 0.0]); // INSBASE
        if space == 0 {
            // Paper space is empty.
            d.bd3([1e20, 1e20, 1e20]);
            d.bd3([-1e20, -1e20, -1e20]);
        } else {
            d.bd3(lo);
            d.bd3(hi);
        }
        d.rd2(lim_lo);
        d.rd2(lim_hi);
        d.bd(0.0); // ELEVATION
        d.bd3([0.0, 0.0, 0.0]); // UCSORG
        d.bd3([1.0, 0.0, 0.0]);
        d.bd3([0.0, 1.0, 0.0]);
        d.h(HARD_PTR, 0); // UCSNAME
        d.h(HARD_PTR, 0); // UCSORTHOREF
        d.bs(0); // UCSORTHOVIEW
        d.h(HARD_PTR, 0); // UCSBASE
        for _ in 0..6 {
            d.bd3([0.0, 0.0, 0.0]); // UCSORG TOP BOTTOM LEFT RIGHT FRONT BACK
        }
    }
    tv(&mut d, ""); // DIMPOST
    tv(&mut d, ""); // DIMAPOST
    for v in [1.0, 2.5, 0.625, 3.75, 1.25, 0.0, 0.0, 0.0, 0.0] {
        d.bd(v); // DIMSCALE DIMASZ DIMEXO DIMDLI DIMEXE DIMRND DIMDLE DIMTP DIMTM
    }
    for _ in 0..6 {
        d.bit(false); // DIMTOL DIMLIM DIMTIH DIMTOH DIMSE1 DIMSE2
    }
    d.bs(1); // DIMTAD
    d.bs(8); // DIMZIN
    d.bs(0); // DIMAZIN
    for v in [2.5, 2.5, 0.0, 0.03937007874015748, 1.0, 0.0, 1.0, 0.625] {
        d.bd(v); // DIMTXT DIMCEN DIMTSZ DIMALTF DIMLFAC DIMTVP DIMTFAC DIMGAP
    }
    d.bd(0.0); // DIMALTRND
    d.bit(false); // DIMALT
    d.bs(3); // DIMALTD
    d.bit(true); // DIMTOFL
    d.bit(false); // DIMSAH
    d.bit(false); // DIMTIX
    d.bit(false); // DIMSOXD
    for _ in 0..3 {
        d.bs(0); // DIMCLRD DIMCLRE DIMCLRT
    }
    for v in [0, 2, 2, 2, 3, 0, 0, 2, 44, 0, 0] {
        d.bs(v); // DIMADEC DIMDEC DIMTDEC DIMALTU DIMALTTD DIMAUNIT DIMFRAC DIMLUNIT DIMDSEP DIMTMOVE DIMJUST
    }
    d.bit(false); // DIMSD1
    d.bit(false); // DIMSD2
    for v in [0, 8, 0, 0] {
        d.bs(v); // DIMTOLJ DIMTZIN DIMALTZ DIMALTTZ
    }
    d.bit(false); // DIMUPT
    d.bs(3); // DIMATFIT
    d.h(HARD_PTR, r.style); // DIMTXSTY
    for _ in 0..4 {
        d.h(HARD_PTR, 0); // DIMLDRBLK DIMBLK DIMBLK1 DIMBLK2
    }
    d.bs(-2); // DIMLWD
    d.bs(-2); // DIMLWE
    for &c in &r.controls {
        d.h(HARD_OWNER, c);
    }
    d.h(HARD_PTR, r.groups);
    d.h(HARD_PTR, r.mlstyles);
    d.h(HARD_OWNER, r.nod);
    d.bs(1); // TSTACKALIGN
    d.bs(70); // TSTACKSIZE
    tv(&mut d, ""); // HYPERLINKBASE
    tv(&mut d, ""); // STYLESHEET
    d.h(HARD_PTR, r.layouts);
    d.h(HARD_PTR, r.plotsettings);
    d.h(HARD_PTR, r.plotstyles);
    // CELWEIGHT ByLayer, lineweights displayed, XEDIT on, extended names, colour-dependent plot styles.
    d.bl(0x1D | 0x0800 | 0x2000);
    d.bs(4); // INSUNITS: millimetres
    d.bs(0); // CEPSNTYPE ByLayer
    tv(&mut d, &r.guid); // FINGERPRINTGUID
    tv(&mut d, &r.guid); // VERSIONGUID
    d.h(HARD_PTR, r.ps);
    d.h(HARD_PTR, r.ms);
    d.h(HARD_PTR, r.bylayer);
    d.h(HARD_PTR, r.byblock);
    d.h(HARD_PTR, r.continuous);
    for _ in 0..4 {
        d.bs(-1);
    }
    let data = d.into_bytes();
    section(&SN_HEADER, &SN_HEADER_END, &data)
}

/// Sentinel, size, data, CRC (0xC0C1 over size and data), end sentinel.
fn section(begin: &[u8; 16], end: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let mut out = begin.to_vec();
    let body_at = out.len();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    let crc = crc16(0xC0C1, &out[body_at..]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(end);
    out
}

fn classes_section() -> Vec<u8> {
    let mut d = BitWriter::new();
    for (i, (dxf, cpp, app, item)) in CLASSES.iter().enumerate() {
        d.bs(500 + i as i16);
        d.bs(0); // proxy flags
        tv(&mut d, app);
        tv(&mut d, cpp);
        tv(&mut d, dxf);
        d.bit(false); // not a zombie
        d.bs(*item);
    }
    section(&SN_CLASSES, &SN_CLASSES_END, &d.into_bytes())
}

/// The object map (spec 23.1): sections of at most 2032 bytes of (handle offset, location offset) pairs,
/// each restarting from handle 0 / location 0, a big-endian size and CRC, then an empty closing section.
fn object_map(entries: &[(u64, usize)]) -> Vec<u8> {
    let mut out = Vec::new();
    let flush = |out: &mut Vec<u8>, data: &[u8]| {
        let size = (data.len() + 2) as u16;
        let start = out.len();
        out.extend_from_slice(&size.to_be_bytes());
        out.extend_from_slice(data);
        let crc = crc16(0xC0C1, &out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut data = Vec::new();
    let (mut last_h, mut last_loc) = (0u64, 0i64);
    for &(h, loc) in entries {
        let mut pair = modular_char_unsigned(h - last_h);
        pair.extend(modular_char_signed(loc as i64 - last_loc));
        if data.len() + pair.len() > 2030 {
            flush(&mut out, &data);
            data.clear();
            pair = modular_char_unsigned(h);
            pair.extend(modular_char_signed(loc as i64));
        }
        data.extend(pair);
        (last_h, last_loc) = (h, loc as i64);
    }
    if !data.is_empty() {
        flush(&mut out, &data);
    }
    flush(&mut out, &[]);
    out
}

//! Reads an R2000 file back structurally: section locators, sentinels and CRCs, the class list, the object
//! map, and every object's size, CRC, type and handle – plus the layer, points and text of the entities this
//! crate writes. Used by the tests and as a self-check; it is not a general DWG reader.

use crate::bits::{BitReader, Eof, crc16, read_modular_char, read_modular_short};
use crate::write::{SN_CLASSES, SN_CLASSES_END, SN_FILE, SN_HEADER, SN_HEADER_END, ty};
use crate::{Error, Result};

/// One object as read back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectedObject {
    pub handle: u64,
    pub type_code: u16,
    /// "LINE", "LAYER", … (classes by their DXF name).
    pub kind: String,
    /// The name of a table record or dictionary entry owner (LAYER, LTYPE, BLOCK_HEADER, STYLE …).
    pub name: Option<String>,
    /// An entity's layer name.
    pub layer: Option<String>,
    /// An entity's defining points (LINE ends, LWPOLYLINE vertices, ARC/CIRCLE center, TEXT/MTEXT insertion).
    pub points: Vec<[f64; 3]>,
    pub radius: Option<f64>,
    pub text: Option<String>,
    /// An entity's lineweight index (0–23 standard, 29 ByLayer) or a layer's.
    pub lineweight: Option<u8>,
    /// An entity's or layer's colour number.
    pub color: Option<i16>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inspected {
    pub version: String,
    /// Class DXF names, numbered from 500.
    pub classes: Vec<String>,
    pub objects: Vec<InspectedObject>,
}

impl Inspected {
    pub fn count(&self, kind: &str) -> usize {
        self.objects.iter().filter(|o| o.kind == kind).count()
    }
    pub fn entities(&self) -> impl Iterator<Item = &InspectedObject> {
        self.objects.iter().filter(|o| o.layer.is_some())
    }
}

fn bad(s: impl Into<String>) -> Error {
    Error::Read(s.into())
}

impl From<Eof> for Error {
    fn from(_: Eof) -> Self {
        Error::Read("data ends early".into())
    }
}

fn u32_at(d: &[u8], at: usize) -> Result<u32> {
    let b = d.get(at..at + 4).ok_or_else(|| bad("truncated"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Checks one sentinel-wrapped section and returns its data.
fn section<'a>(d: &'a [u8], at: usize, len: usize, begin: &[u8; 16], end: &[u8; 16], what: &str) -> Result<&'a [u8]> {
    let s = d.get(at..at + len).ok_or_else(|| bad(format!("{what} section outside the file")))?;
    if s.get(..16) != Some(&begin[..]) || s.get(len.saturating_sub(16)..) != Some(&end[..]) {
        return Err(bad(format!("{what} sentinels")));
    }
    let size = u32_at(s, 16)? as usize;
    if 16 + 4 + size + 2 + 16 != len {
        return Err(bad(format!("{what} size {size} disagrees with its locator")));
    }
    let stored = u16::from_le_bytes([s[20 + size], s[21 + size]]);
    if crc16(0xC0C1, &s[16..20 + size]) != stored {
        return Err(bad(format!("{what} CRC")));
    }
    Ok(&s[20..20 + size])
}

fn kind_name(code: u16, classes: &[String]) -> String {
    let fixed = match code {
        ty::TEXT => "TEXT",
        ty::BLOCK => "BLOCK",
        ty::ENDBLK => "ENDBLK",
        ty::ARC => "ARC",
        ty::CIRCLE => "CIRCLE",
        ty::LINE => "LINE",
        ty::SPLINE => "SPLINE",
        ty::DICTIONARY => "DICTIONARY",
        ty::MTEXT => "MTEXT",
        ty::BLOCK_CONTROL => "BLOCK_CONTROL",
        ty::BLOCK_HEADER => "BLOCK_HEADER",
        ty::LAYER_CONTROL => "LAYER_CONTROL",
        ty::LAYER => "LAYER",
        ty::STYLE_CONTROL => "STYLE_CONTROL",
        ty::STYLE => "STYLE",
        ty::LTYPE_CONTROL => "LTYPE_CONTROL",
        ty::LTYPE => "LTYPE",
        ty::VIEW_CONTROL => "VIEW_CONTROL",
        ty::UCS_CONTROL => "UCS_CONTROL",
        ty::VPORT_CONTROL => "VPORT_CONTROL",
        ty::VPORT => "VPORT",
        ty::APPID_CONTROL => "APPID_CONTROL",
        ty::APPID => "APPID",
        ty::DIMSTYLE_CONTROL => "DIMSTYLE_CONTROL",
        ty::DIMSTYLE => "DIMSTYLE",
        ty::VPENT_CONTROL => "VPORT_ENTITY_CONTROL",
        ty::MLINESTYLE => "MLINESTYLE",
        c if c >= 500 => return classes.get(c as usize - 500).cloned().unwrap_or_else(|| format!("CLASS{c}")),
        c => return format!("TYPE{c}"),
    };
    fixed.to_string()
}

fn is_entity(kind: &str) -> bool {
    matches!(kind, "TEXT" | "BLOCK" | "ENDBLK" | "ARC" | "CIRCLE" | "LINE" | "SPLINE" | "MTEXT" | "LWPOLYLINE")
}

/// Reads `bytes` back as an R2000 DWG written by this crate, checking every CRC on the way.
pub fn inspect(bytes: &[u8]) -> Result<Inspected> {
    let version = String::from_utf8_lossy(bytes.get(..6).ok_or_else(|| bad("too short"))?).to_string();
    if version != "AC1015" {
        return Err(bad(format!("version {version}")));
    }
    let n = u32_at(bytes, 0x15)? as usize;
    if !(3..=6).contains(&n) {
        return Err(bad(format!("{n} section locators")));
    }
    let crc_at = 0x19 + n * 9;
    let stored =
        u16::from_le_bytes([*bytes.get(crc_at).ok_or_else(|| bad("truncated"))?, *bytes.get(crc_at + 1).ok_or_else(|| bad("truncated"))?]);
    let magic = [0xA598, 0x8101, 0x3CC4, 0x8461][n - 3];
    if crc16(0, &bytes[..crc_at]) ^ magic != stored {
        return Err(bad("file header CRC"));
    }
    if bytes.get(crc_at + 2..crc_at + 18) != Some(&SN_FILE[..]) {
        return Err(bad("file header sentinel"));
    }
    let mut loc = [(0usize, 0usize); 6];
    for i in 0..n {
        let p = 0x19 + i * 9;
        let nr = bytes[p] as usize;
        if nr < 6 {
            loc[nr] = (u32_at(bytes, p + 1)? as usize, u32_at(bytes, p + 5)? as usize);
        }
    }
    section(bytes, loc[0].0, loc[0].1, &SN_HEADER, &SN_HEADER_END, "header")?;
    let class_data = section(bytes, loc[1].0, loc[1].1, &SN_CLASSES, &SN_CLASSES_END, "classes")?;
    let mut classes = Vec::new();
    let mut r = BitReader::new(class_data);
    while r.pos() + 8 < class_data.len() * 8 {
        let num = r.bs()?;
        if num as usize != 500 + classes.len() {
            break;
        }
        r.bs()?;
        r.tv()?;
        r.tv()?;
        let dxf = r.tv()?;
        r.bit()?;
        r.bs()?;
        classes.push(String::from_utf8_lossy(&dxf).to_string());
    }

    // Object map.
    let map = bytes.get(loc[2].0..loc[2].0 + loc[2].1).ok_or_else(|| bad("object map outside the file"))?;
    let mut entries: Vec<(u64, usize)> = Vec::new();
    let mut p = 0;
    loop {
        let size =
            u16::from_be_bytes([*map.get(p).ok_or_else(|| bad("object map"))?, *map.get(p + 1).ok_or_else(|| bad("object map"))?]) as usize;
        let sec = map.get(p..p + size + 2).ok_or_else(|| bad("object map section"))?;
        if size < 2 || crc16(0xC0C1, &sec[..size]) != u16::from_be_bytes([sec[size], sec[size + 1]]) {
            return Err(bad("object map CRC"));
        }
        if size == 2 {
            break;
        }
        let (mut h, mut l) = (0i64, 0i64);
        let mut q = 2;
        while q < size {
            h += read_modular_char(&sec[..size], &mut q, false)?;
            l += read_modular_char(&sec[..size], &mut q, true)?;
            entries.push((h as u64, l as usize));
        }
        p += size + 2;
    }

    let mut objects = Vec::with_capacity(entries.len());
    let mut layer_handles: Vec<(u64, usize)> = Vec::new();
    for &(handle, at) in &entries {
        let mut q = at;
        let size = read_modular_short(bytes, &mut q)? as usize;
        let body = bytes.get(q..q + size).ok_or_else(|| bad(format!("object {handle:X} outside the file")))?;
        let crc = bytes.get(q + size..q + size + 2).ok_or_else(|| bad("object CRC missing"))?;
        if crc16(0xC0C1, &bytes[at..q + size]) != u16::from_le_bytes([crc[0], crc[1]]) {
            return Err(bad(format!("object {handle:X} CRC")));
        }
        let mut r = BitReader::new(body);
        let code = r.bs()? as u16;
        let bitsize = r.rl()? as usize;
        let (hc, h) = r.h()?;
        if hc != 0 || h != handle {
            return Err(bad(format!("object at {at} has handle {h:X}, the map says {handle:X}")));
        }
        if r.bs()? != 0 {
            return Err(bad(format!("object {handle:X} has extended data")));
        }
        let kind = kind_name(code, &classes);
        let mut o = InspectedObject { handle, type_code: code, kind: kind.clone(), ..Default::default() };
        let mut hr = BitReader::at(body, bitsize);
        if is_entity(&kind) {
            if r.bit()? {
                return Err(bad("unexpected graphic image"));
            }
            let mode = r.bb()?;
            let reactors = r.bl()?.clamp(0, 1000);
            let nolinks = r.bit()?;
            o.color = Some(r.bs()?);
            r.bd()?;
            let lt = r.bb()?;
            let ps = r.bb()?;
            r.bs()?;
            o.lineweight = Some(r.rc()?);
            if mode == 0 {
                hr.h()?;
            }
            for _ in 0..reactors {
                hr.h()?;
            }
            hr.h()?; // xdictionary
            if !nolinks {
                hr.h()?;
                hr.h()?;
            }
            let layer = hr.h()?.1;
            if lt == 3 {
                hr.h()?;
            }
            if ps == 3 {
                hr.h()?;
            }
            o.layer = Some(format!("#{layer}"));
            match kind.as_str() {
                "LINE" => {
                    let flat = r.bit()?;
                    let x1 = r.rd()?;
                    let x2 = r.dd(x1)?;
                    let y1 = r.rd()?;
                    let y2 = r.dd(y1)?;
                    let (z1, z2) = if flat { (0.0, 0.0) } else { (r.rd()?, 0.0) };
                    let z2 = if flat { z2 } else { r.dd(z1)? };
                    o.points = vec![[x1, y1, z1], [x2, y2, z2]];
                }
                "ARC" | "CIRCLE" => {
                    o.points = vec![[r.bd()?, r.bd()?, r.bd()?]];
                    o.radius = Some(r.bd()?);
                }
                "LWPOLYLINE" => {
                    let flags = r.bs()?;
                    if flags & 4 != 0 {
                        r.bd()?;
                    }
                    if flags & 8 != 0 {
                        r.bd()?;
                    }
                    if flags & 2 != 0 {
                        r.bd()?;
                    }
                    if flags & 1 != 0 {
                        r.bd()?;
                        r.bd()?;
                        r.bd()?;
                    }
                    let count = r.bl()?.clamp(0, 1 << 20) as usize;
                    if flags & 16 != 0 {
                        r.bl()?;
                    }
                    let mut prev = [r.rd()?, r.rd()?];
                    o.points.push([prev[0], prev[1], 0.0]);
                    for _ in 1..count {
                        prev = [r.dd(prev[0])?, r.dd(prev[1])?];
                        o.points.push([prev[0], prev[1], 0.0]);
                    }
                }
                "TEXT" => {
                    let f = r.rc()?;
                    let z = if f & 1 == 0 { r.rd()? } else { 0.0 };
                    let at = [r.rd()?, r.rd()?];
                    if f & 2 == 0 {
                        r.dd(at[0])?;
                        r.dd(at[1])?;
                    }
                    r.be()?;
                    r.bt()?;
                    if f & 4 == 0 {
                        r.rd()?;
                    }
                    if f & 8 == 0 {
                        r.rd()?;
                    }
                    r.rd()?;
                    if f & 0x10 == 0 {
                        r.rd()?;
                    }
                    o.points = vec![[at[0], at[1], z]];
                    o.text = Some(String::from_utf8_lossy(&r.tv()?).to_string());
                }
                "MTEXT" => {
                    o.points = vec![[r.bd()?, r.bd()?, r.bd()?]];
                    for _ in 0..6 {
                        r.bd()?;
                    }
                    r.bd()?;
                    r.bd()?;
                    r.bs()?;
                    r.bs()?;
                    r.bd()?;
                    r.bd()?;
                    o.text = Some(String::from_utf8_lossy(&r.tv()?).to_string());
                }
                _ => {}
            }
        } else {
            r.bl()?; // reactors
            if matches!(kind.as_str(), "LAYER" | "LTYPE" | "STYLE" | "BLOCK_HEADER" | "APPID" | "DIMSTYLE" | "VPORT") {
                o.name = Some(String::from_utf8_lossy(&r.tv()?).to_string());
                if kind == "LAYER" {
                    r.bit()?;
                    r.bs()?;
                    r.bit()?;
                    let flags = r.bs()?;
                    o.lineweight = Some(((flags >> 5) & 0x1F) as u8);
                    o.color = Some(r.bs()?);
                    layer_handles.push((handle, objects.len()));
                }
            }
        }
        if r.pos() > bitsize {
            return Err(bad(format!("object {handle:X} ({kind}) data runs past its handle stream")));
        }
        if hr.pos() > body.len() * 8 {
            return Err(bad(format!("object {handle:X} handles run past its end")));
        }
        objects.push(o);
    }
    // Resolve entity layers to names.
    let names: Vec<(u64, String)> = layer_handles.iter().map(|&(h, i)| (h, objects[i].name.clone().unwrap_or_default())).collect();
    for o in &mut objects {
        if let Some(l) = &o.layer {
            let h: u64 = l.trim_start_matches('#').parse().unwrap_or(0);
            if let Some((_, n)) = names.iter().find(|(lh, _)| *lh == h) {
                o.layer = Some(n.clone());
            } else {
                return Err(bad(format!("entity {:X} refers to layer handle {h:X}, which is no layer", o.handle)));
            }
        }
    }
    Ok(Inspected { version, classes, objects })
}

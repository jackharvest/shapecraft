//! Reading transmit files: text (`.x_t`) and neutral binary (`.x_b`, the `PS\0\0` blobs inside `.shapr`).
//!
//! Every node becomes a generic [`Node`] laid out by the file's schema (base 13006 patched by the embedded
//! schema edits the file carries); `model` turns those into bodies.

use std::collections::HashMap;

use crate::schema::{self, FieldDef, NodeDef};
use crate::{Error, Result};

/// The modeller's marker for an unset integer.
pub const NULL_INT: i64 = -32764;
/// The modeller's marker for an unset double.
pub const NULL_REAL: f64 = -3.14158e13;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Null,
    Int(i64),
    Real(f64),
    Ptr(u32),
    Char(u8),
    Bool(bool),
    Vec3([f64; 3]),
    /// `i`, `b`, `h`: 2, 6 and 3 doubles.
    Reals(Vec<f64>),
    /// A `c[]` field (strings) kept as bytes.
    Bytes(Vec<u8>),
    Arr(Vec<Val>),
}

impl Val {
    pub fn ptr(&self) -> u32 {
        match self {
            Val::Ptr(p) => *p,
            _ => 0,
        }
    }
    pub fn int(&self) -> Option<i64> {
        match self {
            Val::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn real(&self) -> Option<f64> {
        match self {
            Val::Real(r) => Some(*r),
            Val::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn vec3(&self) -> Option<[f64; 3]> {
        match self {
            Val::Vec3(v) => Some(*v),
            _ => None,
        }
    }
    pub fn char(&self) -> Option<u8> {
        match self {
            Val::Char(c) => Some(*c),
            _ => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self {
            Val::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub ty: u16,
    pub index: u32,
    pub fields: Vec<Val>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Text,
    Binary,
}

/// A whole transmit file: header strings, the schema the nodes follow and the nodes in file order.
#[derive(Clone, Debug)]
pub struct XtFile {
    pub format: Format,
    /// The human header before `**END_OF_HEADER` (text files and `.x_t` exports only).
    pub header: String,
    pub modeller: String,
    pub schema_name: String,
    /// Present when the file carries an embedded schema (`SCH_x_y_13006`).
    pub max_types: Option<u32>,
    pub defs: HashMap<u16, NodeDef>,
    /// Node types whose layout the file declared (in the order they first appeared).
    pub declared: Vec<u16>,
    pub nodes: Vec<Node>,
    pub by_index: HashMap<u32, usize>,
}

impl XtFile {
    pub fn node(&self, index: u32) -> Option<&Node> {
        self.by_index.get(&index).and_then(|&i| self.nodes.get(i))
    }
    pub fn def(&self, ty: u16) -> Option<&NodeDef> {
        self.defs.get(&ty)
    }
    /// A field of a node by name (`None` when the node type has no such field).
    pub fn field<'a>(&'a self, node: &'a Node, name: &str) -> Option<&'a Val> {
        let i = self.defs.get(&node.ty)?.index_of(name)?;
        node.fields.get(i)
    }
    pub fn type_name(&self, ty: u16) -> &str {
        self.defs.get(&ty).map(|d| d.name.as_str()).unwrap_or("?")
    }
}

/// Primitive reads shared by the two encodings.
trait Src {
    fn u8_num(&mut self) -> Result<u8>;
    fn char(&mut self) -> Result<u8>;
    fn logical(&mut self) -> Result<bool>;
    fn short(&mut self) -> Result<i64>;
    fn int(&mut self) -> Result<i64>;
    fn ptr(&mut self) -> Result<u32>;
    fn real(&mut self) -> Result<f64>;
    /// Several doubles where the text form may write a single `?` for "all unset".
    fn reals(&mut self, n: usize) -> Result<Option<Vec<f64>>>;
    fn chars(&mut self, n: usize) -> Result<Vec<u8>>;
    fn short_string(&mut self) -> Result<String>;
    fn node_type(&mut self) -> Result<i64>;
    fn var_len(&mut self) -> Result<i64>;
    fn pos(&self) -> usize;
}

struct Bin<'a> {
    b: &'a [u8],
    at: usize,
}

impl Bin<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).ok_or(Error::Eof)?;
        let s = self.b.get(self.at..end).ok_or(Error::Eof)?;
        self.at = end;
        Ok(s)
    }
    fn i16(&mut self) -> Result<i16> {
        let s = self.take(2)?;
        Ok(i16::from_be_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Result<i32> {
        let s = self.take(4)?;
        Ok(i32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
}

impl Src for Bin<'_> {
    fn u8_num(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn char(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn logical(&mut self) -> Result<bool> {
        Ok(self.take(1)?[0] != 0)
    }
    fn short(&mut self) -> Result<i64> {
        Ok(self.i16()? as i64)
    }
    fn int(&mut self) -> Result<i64> {
        Ok(self.i32()? as i64)
    }
    fn ptr(&mut self) -> Result<u32> {
        let r = self.i16()? as i64;
        let (q, r) = if r < 0 { (self.i16()? as i64, -r) } else { (0, r) };
        let v = q * 32767 + r - 1;
        u32::try_from(v).map_err(|_| Error::Bad(format!("pointer {v}")))
    }
    fn real(&mut self) -> Result<f64> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(f64::from_be_bytes(a))
    }
    fn reals(&mut self, n: usize) -> Result<Option<Vec<f64>>> {
        let v = (0..n).map(|_| self.real()).collect::<Result<Vec<_>>>()?;
        Ok(if v.iter().all(|&x| x == NULL_REAL) { None } else { Some(v) })
    }
    fn chars(&mut self, n: usize) -> Result<Vec<u8>> {
        Ok(self.take(n)?.to_vec())
    }
    fn short_string(&mut self) -> Result<String> {
        let n = self.u8_num()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn node_type(&mut self) -> Result<i64> {
        self.short()
    }
    fn var_len(&mut self) -> Result<i64> {
        self.int()
    }
    fn pos(&self) -> usize {
        self.at
    }
}

/// Text: newlines are layout only; numbers end with one space, chars and logicals don't.
struct Text {
    b: Vec<u8>,
    at: usize,
}

impl Text {
    fn byte(&mut self) -> Result<u8> {
        let c = *self.b.get(self.at).ok_or(Error::Eof)?;
        self.at += 1;
        Ok(c)
    }
    /// One number token (up to and including its space); `None` for `?`.
    fn token(&mut self) -> Result<Option<String>> {
        if self.b.get(self.at) == Some(&b'?') {
            self.at += 1;
            return Ok(None);
        }
        let start = self.at;
        while let Some(&c) = self.b.get(self.at) {
            if c == b' ' {
                break;
            }
            self.at += 1;
        }
        let tok = String::from_utf8_lossy(self.b.get(start..self.at).ok_or(Error::Eof)?).into_owned();
        if self.at >= self.b.len() && tok.is_empty() {
            return Err(Error::Eof);
        }
        self.at += 1; // the space
        Ok(Some(tok))
    }
    fn num(&mut self) -> Result<i64> {
        match self.token()? {
            None => Ok(NULL_INT),
            Some(t) => t.parse::<i64>().map_err(|_| Error::Bad(format!("integer {t:?} at {}", self.at))),
        }
    }
}

impl Src for Text {
    fn u8_num(&mut self) -> Result<u8> {
        let n = self.num()?;
        u8::try_from(n).map_err(|_| Error::Bad(format!("byte {n}")))
    }
    fn char(&mut self) -> Result<u8> {
        self.byte()
    }
    fn logical(&mut self) -> Result<bool> {
        match self.byte()? {
            b'T' => Ok(true),
            b'F' => Ok(false),
            c => Err(Error::Bad(format!("logical {:?} at {}", c as char, self.at))),
        }
    }
    fn short(&mut self) -> Result<i64> {
        self.num()
    }
    fn int(&mut self) -> Result<i64> {
        self.num()
    }
    fn ptr(&mut self) -> Result<u32> {
        let n = self.num()?;
        u32::try_from(n).map_err(|_| Error::Bad(format!("pointer {n}")))
    }
    fn real(&mut self) -> Result<f64> {
        match self.token()? {
            None => Ok(NULL_REAL),
            Some(t) => t.parse::<f64>().map_err(|_| Error::Bad(format!("real {t:?} at {}", self.at))),
        }
    }
    fn reals(&mut self, n: usize) -> Result<Option<Vec<f64>>> {
        if self.b.get(self.at) == Some(&b'?') {
            self.at += 1;
            return Ok(None);
        }
        Ok(Some((0..n).map(|_| self.real()).collect::<Result<Vec<_>>>()?))
    }
    fn chars(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(n.min(1 << 16));
        while out.len() < n {
            let c = self.byte()?;
            if c == b'\\' {
                match self.byte()? {
                    b'0' => out.push(0),
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b'9' => out.extend_from_slice(b"         "),
                    o => out.push(o),
                }
            } else {
                out.push(c);
            }
        }
        out.truncate(n);
        Ok(out)
    }
    fn short_string(&mut self) -> Result<String> {
        let n = self.num()?;
        let n = usize::try_from(n).map_err(|_| Error::Bad(format!("string length {n}")))?;
        Ok(String::from_utf8_lossy(&self.chars(n)?).into_owned())
    }
    fn node_type(&mut self) -> Result<i64> {
        self.num()
    }
    fn var_len(&mut self) -> Result<i64> {
        self.num()
    }
    fn pos(&self) -> usize {
        self.at
    }
}

/// Caps against hostile lengths.
const MAX_VAR: i64 = 1 << 26;
const MAX_NODES: usize = 1 << 24;

/// Read a transmit file from bytes (text or neutral binary, with or without the text header).
pub fn parse(bytes: &[u8]) -> Result<XtFile> {
    if bytes.starts_with(b"PS\0\0") {
        let mut s = Bin { b: bytes, at: 4 };
        let n = s.i16()?;
        let modeller = String::from_utf8_lossy(s.take(usize::try_from(n).map_err(|_| Error::Bad("version".into()))?)?).into_owned();
        let n = s.i32()?;
        let schema_name = String::from_utf8_lossy(s.take(usize::try_from(n).map_err(|_| Error::Bad("schema".into()))?)?).into_owned();
        return body(&mut s, Format::Binary, String::new(), modeller, schema_name);
    }
    // Text: an optional human header up to **END_OF_HEADER…\n, then 'T'.
    let (header, rest) = match find(bytes, b"**END_OF_HEADER") {
        Some(i) => {
            let nl = bytes[i..].iter().position(|&c| c == b'\n').map(|p| i + p + 1).unwrap_or(bytes.len());
            (String::from_utf8_lossy(&bytes[..nl]).into_owned(), &bytes[nl..])
        }
        None => (String::new(), bytes),
    };
    let b: Vec<u8> = rest.iter().copied().filter(|&c| c != b'\n' && c != b'\r').collect();
    let mut s = Text { b, at: 0 };
    if s.byte()? != b'T' {
        return Err(Error::Bad("not a transmit file".into()));
    }
    let modeller = s.short_string()?;
    let schema_name = s.short_string()?;
    body(&mut s, Format::Text, header, modeller, schema_name)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn body(s: &mut dyn Src, format: Format, header: String, modeller: String, schema_name: String) -> Result<XtFile> {
    let embedded = schema_name.split('_').count() >= 4;
    let max_types = if embedded { Some(s.short()? as u32) } else { None };
    let userfield = if format == Format::Binary { s.int()? } else { s.short()? };
    if userfield != 0 {
        return Err(Error::Bad(format!("user fields ({userfield}) are not supported")));
    }
    let base = schema::base();
    let mut defs: HashMap<u16, NodeDef> = HashMap::new();
    let mut declared = Vec::new();
    let mut nodes = Vec::new();
    let mut by_index = HashMap::new();
    loop {
        let at = s.pos();
        let ty = s.node_type()?;
        if ty == 1 {
            let idx = s.ptr()?;
            if idx == 0 {
                break;
            }
            return Err(Error::Bad(format!("terminator with index {idx}")));
        }
        let ty = u16::try_from(ty).map_err(|_| Error::Bad(format!("node type {ty} at {at}")))?;
        if let std::collections::hash_map::Entry::Vacant(e) = defs.entry(ty) {
            let def = if embedded {
                declare(s, ty, base.get(&ty)).map_err(|e| Error::Bad(format!("schema for node type {ty} at {}: {e}", s.pos())))?
            } else {
                base.get(&ty).cloned().ok_or_else(|| Error::Bad(format!("unknown node type {ty} at {at}")))?
            };
            declared.push(ty);
            e.insert(def);
        }
        let def = defs.get(&ty).ok_or(Error::Eof)?;
        let var = if def.variable() {
            let n = s.var_len()?;
            if !(0..=MAX_VAR).contains(&n) {
                return Err(Error::Bad(format!("variable length {n} at {at}")));
            }
            n as usize
        } else {
            0
        };
        let index = s.ptr().map_err(|e| Error::Bad(format!("{} node index at {}: {e}", def.name, s.pos())))?;
        let mut fields = Vec::with_capacity(def.fields.len());
        for f in &def.fields {
            let n = if f.n == 1 { var } else { f.n as usize };
            fields.push(value(s, f, n).map_err(|e| Error::Bad(format!("{} #{index} .{}: {e}", def.name, f.name)))?);
        }
        by_index.insert(index, nodes.len());
        nodes.push(Node { ty, index, fields });
        if nodes.len() > MAX_NODES {
            return Err(Error::Bad("too many nodes".into()));
        }
    }
    Ok(XtFile { format, header, modeller, schema_name, max_types, defs, declared, nodes, by_index })
}

/// The layout header of the first node of a type in a file with an embedded schema.
fn declare(s: &mut dyn Src, ty: u16, base: Option<&NodeDef>) -> Result<NodeDef> {
    let n = s.u8_num()?;
    let Some(base) = base else {
        // A node type the base schema doesn't have: full definition.
        let name = s.short_string()?;
        let description = s.short_string()?;
        let fields = (0..n).map(|_| field_def(s)).collect::<Result<Vec<_>>>()?;
        return Ok(NodeDef { ty, name, description, fields });
    };
    if n == 255 {
        return Ok(base.clone());
    }
    let mut fields = Vec::new();
    let mut b = 0usize;
    loop {
        match s.char()? {
            b'C' => {
                fields.push(
                    base.fields
                        .get(b)
                        .cloned()
                        .ok_or_else(|| Error::Bad(format!("copy past {} fields of {}", base.fields.len(), base.name)))?,
                );
                b += 1;
            }
            b'D' => b += 1,
            b'I' | b'A' => fields.push(field_def(s)?),
            b'Z' => break,
            c => return Err(Error::Bad(format!("schema edit {:?} for {}", c as char, base.name))),
        }
        if fields.len() > 4096 {
            return Err(Error::Bad("schema edit runaway".into()));
        }
    }
    if fields.len() != n as usize {
        return Err(Error::Bad(format!("{}: {} fields declared, {} built", base.name, n, fields.len())));
    }
    Ok(NodeDef { ty, name: base.name.clone(), description: String::new(), fields })
}

fn field_def(s: &mut dyn Src) -> Result<FieldDef> {
    let name = s.short_string()?;
    let class = s.short()?;
    let n = s.ptr().map_err(|e| Error::Bad(format!("field {name} count at {}: {e}", s.pos())))?;
    let kind = if class != 0 {
        b'p'
    } else {
        *s.short_string()?.as_bytes().first().ok_or_else(|| Error::Bad(format!("field {name} has no type")))?
    };
    let xmt = if n == 1 { s.logical()? } else { true };
    Ok(FieldDef { name, kind, n, class: u16::try_from(class).unwrap_or(0), base: false, xmt })
}

fn value(s: &mut dyn Src, f: &FieldDef, n: usize) -> Result<Val> {
    if f.n == 0 {
        return scalar(s, f.kind);
    }
    if f.kind == b'c' {
        return Ok(Val::Bytes(s.chars(n)?));
    }
    Ok(Val::Arr((0..n).map(|_| scalar(s, f.kind)).collect::<Result<Vec<_>>>()?))
}

fn scalar(s: &mut dyn Src, kind: u8) -> Result<Val> {
    let int = |v: i64| if v == NULL_INT { Val::Null } else { Val::Int(v) };
    Ok(match kind {
        b'u' => Val::Int(s.u8_num()? as i64),
        b'c' => Val::Char(s.char()?),
        b'l' => Val::Bool(s.logical()?),
        b'n' | b'w' => int(s.short()?),
        b'd' | b't' => int(s.int()?),
        b'p' => Val::Ptr(s.ptr()?),
        b'f' => {
            let r = s.real()?;
            if r == NULL_REAL { Val::Null } else { Val::Real(r) }
        }
        b'v' => match s.reals(3)? {
            Some(v) => Val::Vec3([v[0], v[1], v[2]]),
            None => Val::Null,
        },
        b'i' => s.reals(2)?.map(Val::Reals).unwrap_or(Val::Null),
        b'h' => s.reals(3)?.map(Val::Reals).unwrap_or(Val::Null),
        b'b' => s.reals(6)?.map(Val::Reals).unwrap_or(Val::Null),
        k => return Err(Error::Bad(format!("field type {:?}", k as char))),
    })
}

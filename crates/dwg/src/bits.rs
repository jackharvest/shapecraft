//! Bit-coded values (spec chapter 2): a bit writer, a matching reader and the 16-bit CRC.
//!
//! Bits go most significant first within each byte; multi-byte raw values are little-endian.

/// The 8-bit-table CRC of spec 2.14.1 (the reflected 0xA001 polynomial, table built at compile time).
const CRC_TABLE: [u16; 256] = {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// Accumulates the CRC of `data` onto `seed` (0xC0C1 for objects and sections, 0 for the file header).
pub fn crc16(seed: u16, data: &[u8]) -> u16 {
    let mut dx = seed;
    for &b in data {
        let al = b ^ (dx & 0xFF) as u8;
        dx = (dx >> 8) ^ CRC_TABLE[al as usize];
    }
    dx
}

/// A growing stream of bits.
#[derive(Default, Clone, Debug)]
pub struct BitWriter {
    bytes: Vec<u8>,
    /// Bits used in the last byte (0 = the last byte is full or there is none).
    used: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Length in bits.
    pub fn bit_len(&self) -> usize {
        if self.used == 0 { self.bytes.len() * 8 } else { (self.bytes.len() - 1) * 8 + self.used as usize }
    }

    pub fn bit(&mut self, b: bool) {
        if self.used == 0 {
            self.bytes.push(0);
        }
        if let (true, Some(last)) = (b, self.bytes.last_mut()) {
            *last |= 0x80 >> self.used;
        }
        self.used = (self.used + 1) % 8;
    }

    /// The low `n` bits of `v`, most significant first.
    pub fn bits(&mut self, v: u64, n: u32) {
        for i in (0..n).rev() {
            self.bit((v >> i) & 1 != 0);
        }
    }

    /// Overwrites `n` bits starting at bit `pos` (which must already be written).
    pub fn patch(&mut self, pos: usize, v: u64, n: u32) {
        for i in 0..n as usize {
            let bit = (v >> (n as usize - 1 - i)) & 1 != 0;
            let (byte, off) = ((pos + i) / 8, (pos + i) % 8);
            if let Some(b) = self.bytes.get_mut(byte) {
                if bit {
                    *b |= 0x80 >> off;
                } else {
                    *b &= !(0x80 >> off);
                }
            }
        }
    }

    /// Appends every bit of another stream.
    pub fn append(&mut self, other: &BitWriter) {
        if self.used == 0 {
            self.bytes.extend_from_slice(&other.bytes);
            self.used = other.used;
            return;
        }
        let n = other.bit_len();
        for i in 0..n {
            self.bit(other.bytes[i / 8] & (0x80 >> (i % 8)) != 0);
        }
    }

    /// Zero bits up to the next byte boundary.
    pub fn pad(&mut self) {
        self.used = 0;
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    // ---- raw values ----
    pub fn rc(&mut self, v: u8) {
        self.bits(v as u64, 8);
    }
    pub fn rs(&mut self, v: u16) {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
    }
    pub fn rl(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
    }
    pub fn rd(&mut self, v: f64) {
        for b in v.to_le_bytes() {
            self.rc(b);
        }
    }
    pub fn rd2(&mut self, p: [f64; 2]) {
        self.rd(p[0]);
        self.rd(p[1]);
    }

    // ---- compressed values ----
    pub fn bb(&mut self, v: u8) {
        self.bits(v as u64 & 3, 2);
    }
    pub fn bs(&mut self, v: i16) {
        match v {
            0 => self.bb(2),
            256 => self.bb(3),
            1..=255 => {
                self.bb(1);
                self.rc(v as u8);
            }
            _ => {
                self.bb(0);
                self.rs(v as u16);
            }
        }
    }
    pub fn bl(&mut self, v: i32) {
        match v {
            0 => self.bb(2),
            1..=255 => {
                self.bb(1);
                self.rc(v as u8);
            }
            _ => {
                self.bb(0);
                self.rl(v as u32);
            }
        }
    }
    pub fn bd(&mut self, v: f64) {
        if v.to_bits() == 0f64.to_bits() {
            self.bb(2);
        } else if v.to_bits() == 1f64.to_bits() {
            self.bb(1);
        } else {
            self.bb(0);
            self.rd(v);
        }
    }
    pub fn bd2(&mut self, p: [f64; 2]) {
        self.bd(p[0]);
        self.bd(p[1]);
    }
    pub fn bd3(&mut self, p: [f64; 3]) {
        self.bd(p[0]);
        self.bd(p[1]);
        self.bd(p[2]);
    }
    /// BitDouble with default (spec 2.9).
    pub fn dd(&mut self, v: f64, default: f64) {
        let (b, d) = (v.to_le_bytes(), default.to_le_bytes());
        if b == d {
            self.bb(0);
        } else if b[4..] == d[4..] {
            self.bb(1);
            for &x in &b[..4] {
                self.rc(x);
            }
        } else if b[6..] == d[6..] {
            self.bb(2);
            for &x in b[4..6].iter().chain(&b[..4]) {
                self.rc(x);
            }
        } else {
            self.bb(3);
            self.rd(v);
        }
    }
    /// BitThickness (R2000+).
    pub fn bt(&mut self, v: f64) {
        if v.to_bits() == 0f64.to_bits() {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd(v);
        }
    }
    /// BitExtrusion (R2000+).
    pub fn be(&mut self, n: [f64; 3]) {
        if n == [0.0, 0.0, 1.0] {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd3(n);
        }
    }
    /// Text (R2000: bitshort length, then the bytes, no terminator).
    pub fn tv(&mut self, s: &[u8]) {
        let n = s.len().min(i16::MAX as usize);
        self.bs(n as i16);
        for &b in &s[..n] {
            self.rc(b);
        }
    }
    /// A handle reference: 4-bit code, 4-bit byte count, then the handle bytes most significant first.
    pub fn h(&mut self, code: u8, handle: u64) {
        let n = (8 - handle.leading_zeros() / 8) as u8;
        let n = if handle == 0 { 0 } else { n };
        self.rc((code << 4) | n);
        for i in (0..n).rev() {
            self.rc((handle >> (8 * i as u64)) as u8);
        }
    }
}

/// Modular short (object sizes): 15-bit groups, little-endian shorts, high bit = more follows.
pub fn modular_short(mut v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let part = (v & 0x7FFF) as u16;
        v >>= 15;
        let word = if v != 0 { part | 0x8000 } else { part };
        out.extend_from_slice(&word.to_le_bytes());
        if v == 0 {
            return out;
        }
    }
}

/// Unsigned modular char (handle offsets in the object map).
pub fn modular_char_unsigned(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let part = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(part);
            return out;
        }
        out.push(part | 0x80);
    }
}

/// Signed modular char (location offsets): the 0x40 bit of the last byte is the sign.
pub fn modular_char_signed(v: i64) -> Vec<u8> {
    let neg = v < 0;
    let mut m = v.unsigned_abs();
    let mut out = Vec::new();
    loop {
        let part = (m & 0x7F) as u8;
        m >>= 7;
        if m == 0 && part & 0x40 == 0 {
            out.push(if neg { part | 0x40 } else { part });
            return out;
        }
        out.push(part | 0x80);
        if m == 0 {
            // The value filled the sign bit: one more byte carries the sign.
            out.push(if neg { 0x40 } else { 0 });
            return out;
        }
    }
}

/// Reads what [`BitWriter`] writes (for the structural self-check and tests).
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// Running past the end of the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eof;

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn at(data: &'a [u8], bit: usize) -> Self {
        Self { data, pos: bit }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn bit(&mut self) -> Result<bool, Eof> {
        let b = *self.data.get(self.pos / 8).ok_or(Eof)?;
        let v = b & (0x80 >> (self.pos % 8)) != 0;
        self.pos += 1;
        Ok(v)
    }
    pub fn bits(&mut self, n: u32) -> Result<u64, Eof> {
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as u64;
        }
        Ok(v)
    }
    pub fn rc(&mut self) -> Result<u8, Eof> {
        Ok(self.bits(8)? as u8)
    }
    pub fn rs(&mut self) -> Result<u16, Eof> {
        Ok(u16::from_le_bytes([self.rc()?, self.rc()?]))
    }
    pub fn rl(&mut self) -> Result<u32, Eof> {
        Ok(u32::from_le_bytes([self.rc()?, self.rc()?, self.rc()?, self.rc()?]))
    }
    pub fn rd(&mut self) -> Result<f64, Eof> {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.rc()?;
        }
        Ok(f64::from_le_bytes(b))
    }
    pub fn bb(&mut self) -> Result<u8, Eof> {
        Ok(self.bits(2)? as u8)
    }
    pub fn bs(&mut self) -> Result<i16, Eof> {
        Ok(match self.bb()? {
            0 => self.rs()? as i16,
            1 => self.rc()? as i16,
            2 => 0,
            _ => 256,
        })
    }
    pub fn bl(&mut self) -> Result<i32, Eof> {
        Ok(match self.bb()? {
            0 => self.rl()? as i32,
            1 => self.rc()? as i32,
            _ => 0,
        })
    }
    pub fn bd(&mut self) -> Result<f64, Eof> {
        Ok(match self.bb()? {
            0 => self.rd()?,
            1 => 1.0,
            _ => 0.0,
        })
    }
    pub fn dd(&mut self, default: f64) -> Result<f64, Eof> {
        let mut b = default.to_le_bytes();
        match self.bb()? {
            0 => {}
            1 => {
                for x in &mut b[..4] {
                    *x = self.rc()?;
                }
            }
            2 => {
                b[4] = self.rc()?;
                b[5] = self.rc()?;
                for x in &mut b[..4] {
                    *x = self.rc()?;
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(b))
    }
    pub fn bt(&mut self) -> Result<f64, Eof> {
        if self.bit()? { Ok(0.0) } else { self.bd() }
    }
    pub fn be(&mut self) -> Result<[f64; 3], Eof> {
        if self.bit()? { Ok([0.0, 0.0, 1.0]) } else { Ok([self.bd()?, self.bd()?, self.bd()?]) }
    }
    pub fn tv(&mut self) -> Result<Vec<u8>, Eof> {
        let n = self.bs()?.max(0) as usize;
        let mut s = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            s.push(self.rc()?);
        }
        Ok(s)
    }
    /// (code, handle)
    pub fn h(&mut self) -> Result<(u8, u64), Eof> {
        let b = self.rc()?;
        let mut v = 0u64;
        for _ in 0..(b & 0x0F).min(8) {
            v = (v << 8) | self.rc()? as u64;
        }
        Ok((b >> 4, v))
    }
}

/// Reads a modular short at `data[*at..]`, advancing `at`.
pub fn read_modular_short(data: &[u8], at: &mut usize) -> Result<u32, Eof> {
    let mut v = 0u32;
    for shift in [0u32, 15, 30] {
        let w = u16::from_le_bytes([*data.get(*at).ok_or(Eof)?, *data.get(*at + 1).ok_or(Eof)?]);
        *at += 2;
        v |= ((w & 0x7FFF) as u32) << shift;
        if w & 0x8000 == 0 {
            return Ok(v);
        }
    }
    Err(Eof)
}

/// Reads a modular char; `signed` honors the 0x40 sign bit of the last byte.
pub fn read_modular_char(data: &[u8], at: &mut usize, signed: bool) -> Result<i64, Eof> {
    let mut v = 0i64;
    let mut shift = 0;
    loop {
        let b = *data.get(*at).ok_or(Eof)?;
        *at += 1;
        if b & 0x80 != 0 {
            v |= ((b & 0x7F) as i64) << shift;
            shift += 7;
            if shift > 56 {
                return Err(Eof);
            }
        } else {
            if signed && b & 0x40 != 0 {
                v |= ((b & 0x3F) as i64) << shift;
                return Ok(-v);
            }
            v |= ((b & 0x7F) as i64) << shift;
            return Ok(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_table_matches_the_spec() {
        // Spot checks against the table printed in the specification.
        assert_eq!(CRC_TABLE[1], 0xC0C1);
        assert_eq!(CRC_TABLE[2], 0xC181);
        assert_eq!(CRC_TABLE[8], 0xC601);
        assert_eq!(CRC_TABLE[255], 0x4040);
        assert_eq!(crc16(0, &[]), 0);
        // Accumulating in pieces equals one pass.
        let data = b"AC1015 object data";
        assert_eq!(crc16(crc16(0xC0C1, &data[..5]), &data[5..]), crc16(0xC0C1, data));
    }

    #[test]
    fn bitshort_example_from_the_spec() {
        // 257, 0, 256, 15, 0 -> 00 00000001 00000001 | 10 | 11 | 01 00001111 | 10
        let mut w = BitWriter::new();
        for v in [257, 0, 256, 15, 0] {
            w.bs(v);
        }
        assert_eq!(w.bit_len(), 34);
        let mut r = BitReader::new(&w.bytes);
        let s: String = (0..34).map(|_| if r.bit().unwrap_or(false) { '1' } else { '0' }).collect();
        assert_eq!(s, "0000000001000000011011010000111110");
    }

    #[test]
    fn values_round_trip() {
        let mut w = BitWriter::new();
        w.bit(true);
        w.bs(-1);
        w.bl(70000);
        w.bl(5);
        w.bd(0.0);
        w.bd(1.0);
        w.bd(-2.75);
        w.dd(10.5, 10.0);
        w.dd(10.0, 10.0);
        w.dd(f64::from_bits(10.0f64.to_bits() ^ 0x1234), 10.0);
        w.bt(0.0);
        w.be([0.0, 0.0, 1.0]);
        w.be([1.0, 0.0, 0.0]);
        w.tv(b"Layer");
        w.h(5, 0x5E7);
        w.h(3, 0);
        let len = w.bit_len();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.bit(), Ok(true));
        assert_eq!(r.bs(), Ok(-1));
        assert_eq!(r.bl(), Ok(70000));
        assert_eq!(r.bl(), Ok(5));
        assert_eq!(r.bd(), Ok(0.0));
        assert_eq!(r.bd(), Ok(1.0));
        assert_eq!(r.bd(), Ok(-2.75));
        assert_eq!(r.dd(10.0), Ok(10.5));
        assert_eq!(r.dd(10.0), Ok(10.0));
        assert_eq!(r.dd(10.0).map(f64::to_bits), Ok(10.0f64.to_bits() ^ 0x1234));
        assert_eq!(r.bt(), Ok(0.0));
        assert_eq!(r.be(), Ok([0.0, 0.0, 1.0]));
        assert_eq!(r.be(), Ok([1.0, 0.0, 0.0]));
        assert_eq!(r.tv(), Ok(b"Layer".to_vec()));
        assert_eq!(r.h(), Ok((5, 0x5E7)));
        assert_eq!(r.h(), Ok((3, 0)));
        assert_eq!(r.pos(), len);
    }

    #[test]
    fn handle_bytes_follow_the_spec_example() {
        // 5.2.05.E7: 01010010 00000101 11100111
        let mut w = BitWriter::new();
        w.h(5, 0x5E7);
        assert_eq!(w.into_bytes(), vec![0x52, 0x05, 0xE7]);
    }

    #[test]
    fn modular_values_follow_the_spec_examples() {
        // 10000010 00100100 = 4610; 11101001 10010111 11100110 00110101 = 112823273; 10000101 01001011 = -1413
        let mut at = 0;
        assert_eq!(read_modular_char(&[0x82, 0x24], &mut at, true), Ok(4610));
        assert_eq!(modular_char_signed(4610), vec![0x82, 0x24]);
        assert_eq!(modular_char_signed(112823273), vec![0xE9, 0x97, 0xE6, 0x35]);
        assert_eq!(modular_char_signed(-1413), vec![0x85, 0x4B]);
        for v in [0i64, 1, 63, 64, 127, 128, -1, -64, -65, 8191, 8192, -100000, 1 << 40] {
            let b = modular_char_signed(v);
            let mut at = 0;
            assert_eq!(read_modular_char(&b, &mut at, true), Ok(v), "{v}");
            assert_eq!(at, b.len());
        }
        for v in [0u64, 1, 127, 128, 200, 1 << 30] {
            let b = modular_char_unsigned(v);
            let mut at = 0;
            assert_eq!(read_modular_char(&b, &mut at, false), Ok(v as i64));
        }
        // 00110001 11110100 10001101 00000000 = 4650033
        assert_eq!(modular_short(4650033), vec![0x31, 0xF4, 0x8D, 0x00]);
        for v in [0u32, 1, 0x7FFF, 0x8000, 4650033] {
            let b = modular_short(v);
            let mut at = 0;
            assert_eq!(read_modular_short(&b, &mut at), Ok(v));
        }
    }

    #[test]
    fn patch_and_append_keep_bit_positions() {
        let mut a = BitWriter::new();
        a.bits(0b101, 3);
        let at = a.bit_len();
        a.rl(0);
        a.bit(true);
        a.patch(at, 0xDEADBEEF, 32);
        let mut b = BitWriter::new();
        b.bits(0b011, 3);
        a.append(&b);
        assert_eq!(a.bit_len(), 3 + 32 + 1 + 3);
        let bytes = a.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(r.bits(3), Ok(0b101));
        assert_eq!(r.bits(32), Ok(0xDEADBEEF));
        assert_eq!(r.bits(4), Ok(0b1011));
    }
}

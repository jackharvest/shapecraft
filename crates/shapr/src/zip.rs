//! Just enough ZIP for `.shapr`: stored and deflated entries, read through the central directory.

use crate::{Error, Result};

fn u16le(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(Error::Zip("truncated"))
}
fn u32le(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or(Error::Zip("truncated"))
}

/// Entries as (name, contents).
pub fn read(b: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    // End of central directory: the last "PK\5\6" within the final 64 KiB + 22 bytes.
    let from = b.len().saturating_sub(65_557);
    let eocd =
        (from..b.len().saturating_sub(21)).rev().find(|&i| b.get(i..i + 4) == Some(b"PK\x05\x06")).ok_or(Error::Zip("not a zip file"))?;
    let count = u16le(b, eocd + 10)? as usize;
    let mut at = u32le(b, eocd + 16)? as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if b.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(Error::Zip("bad central directory"));
        }
        let method = u16le(b, at + 10)?;
        let csize = u32le(b, at + 20)? as usize;
        let usize_ = u32le(b, at + 24)? as usize;
        let (nlen, xlen, clen) = (u16le(b, at + 28)? as usize, u16le(b, at + 30)? as usize, u16le(b, at + 32)? as usize);
        let local = u32le(b, at + 42)? as usize;
        let name = String::from_utf8_lossy(b.get(at + 46..at + 46 + nlen).ok_or(Error::Zip("truncated"))?).into_owned();
        at += 46 + nlen + xlen + clen;
        if b.get(local..local + 4) != Some(b"PK\x03\x04") {
            return Err(Error::Zip("bad local header"));
        }
        let start = local + 30 + u16le(b, local + 26)? as usize + u16le(b, local + 28)? as usize;
        let data = b.get(start..start.checked_add(csize).ok_or(Error::Zip("size"))?).ok_or(Error::Zip("truncated entry"))?;
        let data = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, usize_.clamp(1, 1 << 31))
                .map_err(|_| Error::Zip("bad deflate data"))?,
            _ => return Err(Error::Zip("unsupported compression")),
        };
        out.push((name, data));
    }
    Ok(out)
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut c = !0u32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// A deflated archive of the entries, in order.
pub fn write(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let crc = crc32(data);
        let offset = out.len() as u32;
        let head = |sig: &[u8], central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(sig);
            if central {
                h.extend_from_slice(&20u16.to_le_bytes()); // made by
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // needed
            h.extend_from_slice(&0u16.to_le_bytes()); // flags
            h.extend_from_slice(&8u16.to_le_bytes()); // deflate
            h.extend_from_slice(&0u16.to_le_bytes()); // time
            h.extend_from_slice(&0x21u16.to_le_bytes()); // date 1980-01-01
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            h.extend_from_slice(&(data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // extra
            if central {
                h.extend_from_slice(&0u16.to_le_bytes()); // comment
                h.extend_from_slice(&0u16.to_le_bytes()); // disk
                h.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
                h.extend_from_slice(&0u32.to_le_bytes()); // external attrs
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(name.as_bytes());
            h
        };
        out.extend_from_slice(&head(b"PK\x03\x04", false));
        out.extend_from_slice(&packed);
        central.extend_from_slice(&head(b"PK\x01\x02", true));
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let a = b"hello hello hello hello".to_vec();
        let z = super::write(&[("workspace", &a), (".metadata", b"{}")]);
        let back = super::read(&z).unwrap_or_default();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], ("workspace".to_string(), a));
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}

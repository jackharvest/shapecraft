//! A minimal zip writer (stored entries, no compression): enough for 3MF packages.

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Packs `(name, bytes)` entries into a zip archive. Fails past 4 GB (no zip64).
pub fn store(entries: &[(&str, Vec<u8>)]) -> Option<Vec<u8>> {
    store_aligned(entries, 1)
}

/// The same, each entry's data starting on a multiple of `align` bytes (USDZ wants 64), padded in the local header's
/// extra field.
pub fn store_aligned(entries: &[(&str, Vec<u8>)], align: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).ok()?;
        let size = u32::try_from(data.len()).ok()?;
        let name_len = u16::try_from(name.len()).ok()?;
        let crc = crc32(data);
        // Local file header.
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes()); // version needed
        out.extend(0u16.to_le_bytes()); // flags
        out.extend(0u16.to_le_bytes()); // stored
        out.extend([0u8; 4]); // time, date
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(name_len.to_le_bytes());
        let start = out.len() + 2 + name.len();
        let pad = (align.max(1) - start % align.max(1)) % align.max(1);
        out.extend(u16::try_from(pad).ok()?.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(std::iter::repeat_n(0u8, pad));
        out.extend(data);
        // Central directory record.
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes()); // made by
        central.extend(20u16.to_le_bytes());
        central.extend(0u16.to_le_bytes());
        central.extend(0u16.to_le_bytes());
        central.extend([0u8; 4]);
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(name_len.to_le_bytes());
        central.extend([0u8; 12]); // extra, comment, disk, internal attrs, external attrs
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).ok()?;
    let cd_size = u32::try_from(central.len()).ok()?;
    let count = u16::try_from(entries.len()).ok()?;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]); // disk numbers
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_matches_the_reference_value() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}

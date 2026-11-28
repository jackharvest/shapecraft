//! On Windows (MSVC) the app icon goes into shapecraft.exe itself, so Explorer, the taskbar, pinned and Start menu
//! entries and the file's Properties show it, not the blank program icon (#106). The .ico is turned into a compiled
//! resource file (.res) that the linker takes as an input: no resource compiler and no extra crate.

use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const LANG_EN_US: u16 = 0x0409;

fn main() {
    let ico = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon/shapecraft.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let res = std::fs::read(&ico).ok().and_then(|bytes| icon_res(&bytes));
    let (Some(res), Ok(out)) = (res, std::env::var("OUT_DIR")) else {
        println!("cargo:warning=the app icon couldn't be read from {}; the exe gets none", ico.display());
        return;
    };
    let path = PathBuf::from(out).join("shapecraft-icon.res");
    if let Err(e) = std::fs::write(&path, res) {
        println!("cargo:warning=couldn't write {}: {e}", path.display());
        return;
    }
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

/// The .ico's images as RT_ICON resources 1..=n plus the RT_GROUP_ICON 1 that lists them (the exe's icon is its
/// first icon group). `None` if the file isn't an .ico.
fn icon_res(ico: &[u8]) -> Option<Vec<u8>> {
    let u16_at = |i: usize| ico.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |i: usize| ico.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return None;
    }
    let count = u16_at(4)?;
    if count == 0 {
        return None;
    }
    let mut res = Vec::new();
    // Every .res starts with an empty entry.
    entry(&mut res, 0, 0, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend(0u16.to_le_bytes());
    group.extend(1u16.to_le_bytes());
    group.extend(count.to_le_bytes());
    for i in 0..count {
        let at = 6 + 16 * i as usize;
        let dir = ico.get(at..at + 12)?;
        let (len, offset) = (u32_at(at + 8)? as usize, u32_at(at + 12)? as usize);
        let image = ico.get(offset..offset.checked_add(len)?)?;
        let id = i + 1;
        entry(&mut res, RT_ICON, id, 0x1010, LANG_EN_US, image);
        // GRPICONDIRENTRY: the .ico's directory entry with the resource id in place of the file offset.
        group.extend(dir);
        group.extend(id.to_le_bytes());
    }
    entry(&mut res, RT_GROUP_ICON, 1, 0x1030, LANG_EN_US, &group);
    Some(res)
}

/// One resource: a 32-byte header (type and name as ordinals), the data, padding to 4 bytes.
fn entry(res: &mut Vec<u8>, kind: u16, name: u16, flags: u16, lang: u16, data: &[u8]) {
    res.extend((data.len() as u32).to_le_bytes());
    res.extend(32u32.to_le_bytes());
    res.extend([0xff, 0xff]);
    res.extend(kind.to_le_bytes());
    res.extend([0xff, 0xff]);
    res.extend(name.to_le_bytes());
    res.extend(0u32.to_le_bytes()); // data version
    res.extend(flags.to_le_bytes());
    res.extend(lang.to_le_bytes());
    res.extend(0u32.to_le_bytes()); // version
    res.extend(0u32.to_le_bytes()); // characteristics
    res.extend(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

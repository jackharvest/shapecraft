//! Fonts: the platform UI font when installed (never bundled), its semibold cut, a monospaced face for
//! dimensions, a symbol font for shortcut glyphs and CJK fallbacks.

use egui::{FontFamily, FontId};

pub const SEMIBOLD: &str = "semibold";

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Uses the platform UI font when it's installed (it is never bundled), else egui's default.
pub fn install_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontData, FontDefinitions, FontTweak, VariationCoords};
    let candidates = [
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/inter/InterVariable.ttf",
        "/usr/share/fonts/inter/InterVariable.ttf",
        "/usr/share/fonts/opentype/inter/InterVariable.ttf",
        "/usr/share/fonts/cantarell/Cantarell-VF.otf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    let mut fonts = FontDefinitions::default();
    let semibold_family = FontFamily::Name(SEMIBOLD.into());
    let mut found = false;
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let weight = |bytes: &[u8], w: f32| {
            let mut d = FontData::from_owned(bytes.to_vec());
            d.tweak = FontTweak { coords: VariationCoords::new([(b"wght", w)]), ..Default::default() };
            d
        };
        // Segoe UI has no weight axis: its semibold is a file of its own.
        let semibold = if path.ends_with("segoeui.ttf") { std::fs::read(path.replace("segoeui", "seguisb")).ok() } else { None };
        fonts.font_data.insert("ui".into(), weight(&bytes, 430.0).into());
        fonts.font_data.insert("ui-semibold".into(), weight(semibold.as_deref().unwrap_or(&bytes), 620.0).into());
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "ui".into());
        }
        let mut semi = vec!["ui-semibold".to_string()];
        if let Some(rest) = fonts.families.get(&FontFamily::Proportional) {
            semi.extend(rest.iter().skip(1).cloned());
        }
        fonts.families.insert(semibold_family.clone(), semi);
        found = true;
        break;
    }
    // Dimension labels use the platform's monospaced UI face when there is one.
    for path in
        ["/System/Library/Fonts/SFNSMono.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", "C:\\Windows\\Fonts\\consola.ttf"]
    {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let mut d = FontData::from_owned(bytes);
        d.tweak = FontTweak { coords: VariationCoords::new([(b"wght", 560.0)]), ..Default::default() };
        fonts.font_data.insert("mono".into(), d.into());
        if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
            list.insert(0, "mono".into());
        }
        break;
    }
    // Shortcut glyphs (⌘ ⇧ ⌥ ⌫) come from a symbol font when the UI font lacks them.
    for path in [
        "/System/Library/Fonts/Apple Symbols.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
    ] {
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts.font_data.insert("symbols".into(), FontData::from_owned(bytes).into());
        for family in [FontFamily::Proportional, semibold_family.clone()] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push("symbols".into());
            }
        }
        break;
    }
    // Japanese, Chinese and Korean names, from the system's own fonts, one per script at most. All of them are in the
    // one Noto file on Linux. Glyphs are only rasterized when text needs them.
    let cjk: [&[&str]; 3] = [
        &[
            "C:\\Windows\\Fonts\\YuGothR.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        ],
        &["C:\\Windows\\Fonts\\msyh.ttc", "/System/Library/Fonts/Hiragino Sans GB.ttc"],
        &["C:\\Windows\\Fonts\\malgun.ttf", "/System/Library/Fonts/AppleSDGothicNeo.ttc"],
    ];
    for (i, paths) in cjk.iter().enumerate() {
        let Some((path, bytes)) = paths.iter().find_map(|p| std::fs::read(p).ok().map(|b| (*p, b))) else { continue };
        let name = format!("cjk{i}");
        let mut data = FontData::from_owned(bytes);
        // Yu Gothic sits high beside Segoe UI (measured: 0.2 em).
        data.tweak.y_offset_factor = if path.ends_with("YuGothR.ttc") { 0.2 } else { 0.0 };
        fonts.font_data.insert(name.clone(), data.into());
        for family in [FontFamily::Proportional, semibold_family.clone()] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push(name.clone());
            }
        }
    }
    if !found {
        let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
        fonts.families.insert(semibold_family, fallback);
    }
    ctx.set_fonts(fonts);
    // A missing glyph (a shortcut symbol on a system without it) must never take the app down.
    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
}

//! Text in sketches: glyph outlines from a font installed with the operating system (nothing is bundled), flattened
//! to closed polylines. Without a chosen family, a plain sans-serif.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::DVec2;
use skrifa::MetadataProvider;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{FontRef, LocationRef, Size};

/// System fonts tried in order (path, index in a collection).
const CANDIDATES: &[(&str, u32)] = &[
    ("/System/Library/Fonts/Helvetica.ttc", 0),
    ("/System/Library/Fonts/Supplemental/Arial.ttf", 0),
    ("/Library/Fonts/Arial.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/TTF/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", 0),
    ("C:\\Windows\\Fonts\\arial.ttf", 0),
    ("C:\\Windows\\Fonts\\segoeui.ttf", 0),
];

pub const MAX_CHARS: usize = 500;

/// The plain sans-serif text is drawn with when no family is chosen.
struct DefaultFont {
    bytes: Vec<u8>,
    index: u32,
    name: String,
}

fn font_data() -> Option<&'static DefaultFont> {
    static FONT: OnceLock<Option<DefaultFont>> = OnceLock::new();
    FONT.get_or_init(|| candidate_font().or_else(installed_sans)).as_ref()
}

fn candidate_font() -> Option<DefaultFont> {
    CANDIDATES.iter().find_map(|(path, index)| {
        let bytes = std::fs::read(path).ok()?;
        FontRef::from_index(&bytes, *index).ok()?;
        Some(DefaultFont { bytes, index: *index, name: name_from_path(path)? })
    })
}

/// The family name of a candidate ("Helvetica", "Arial"…), from its file name.
fn name_from_path(path: &str) -> Option<String> {
    let stem = path.rsplit(['/', '\\']).next()?.split('.').next()?;
    Some(match stem {
        "arial" => "Arial".into(),
        "segoeui" => "Segoe UI".into(),
        s => s.split('-').next().unwrap_or(s).to_string(),
    })
}

/// A sans-serif from the system's font directories, for systems that keep fonts outside the candidate paths
/// (NixOS and Guix have no /usr/share/fonts). The generic family follows fontconfig's sans-serif alias.
fn installed_sans() -> Option<DefaultFont> {
    use fontdb::Family;
    let db = system_fonts();
    let families = [Family::SansSerif, Family::Name("DejaVu Sans"), Family::Name("Liberation Sans"), Family::Name("Noto Sans")];
    let id = db.query(&fontdb::Query { families: &families, ..Default::default() })?;
    let name = db.face(id)?.families.first()?.0.clone();
    let (bytes, index) = db.with_face_data(id, |data, index| (data.to_vec(), index))?;
    FontRef::from_index(&bytes, index).ok()?;
    Some(DefaultFont { bytes, index, name })
}

/// The family name of the font text is drawn with ("Helvetica", "Arial"…).
pub fn font_name() -> Option<String> {
    font_data().map(|f| f.name.clone())
}

fn system_fonts() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// The font families installed on this computer, sorted, for the Text tool's font list.
pub fn families() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<String> =
            system_fonts().faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).filter(|n| !n.starts_with('.')).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    })
}

/// A font file's data and the face's index in it.
type FaceData = Arc<(Vec<u8>, u32)>;

/// A family's regular face, loaded once.
fn family_data(family: &str) -> Option<FaceData> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<FaceData>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(family) {
        return hit.clone();
    }
    let db = system_fonts();
    let query = fontdb::Query { families: &[fontdb::Family::Name(family)], ..Default::default() };
    let found = db.query(&query).and_then(|id| db.with_face_data(id, |data, index| Arc::new((data.to_vec(), index))));
    let found = found.filter(|d| FontRef::from_index(&d.0, d.1).is_ok());
    cache.lock().ok()?.insert(family.to_string(), found.clone());
    found
}

/// Whether a font was found to draw text with.
pub fn available() -> bool {
    font_data().is_some()
}

/// Collects a glyph's contours as polylines, flattening curves.
struct Flatten {
    contours: Vec<Vec<DVec2>>,
    current: Vec<DVec2>,
    offset: DVec2,
}

impl Flatten {
    fn at(&self, x: f32, y: f32) -> DVec2 {
        self.offset + DVec2::new(x as f64, y as f64)
    }

    fn last(&self) -> DVec2 {
        self.current.last().copied().unwrap_or(self.offset)
    }

    fn finish(&mut self) {
        let mut c = std::mem::take(&mut self.current);
        if c.len() >= 3 {
            if let (Some(first), Some(last)) = (c.first().copied(), c.last().copied())
                && first.distance(last) > 1e-9
            {
                c.push(first);
            }
            self.contours.push(c);
        }
    }
}

const STEPS: usize = 8;

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        let p = self.at(x, y);
        self.current.push(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        if self.last().distance(p) > 1e-9 {
            self.current.push(p);
        }
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, c, p1) = (self.last(), self.at(cx0, cy0), self.at(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            self.current.push(p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (p0, c0, c1, p1) = (self.last(), self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            self.current.push(p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p1 * (t * t * t));
        }
    }

    fn close(&mut self) {
        self.finish();
    }
}

/// The closed outlines of `text`, `size` mm tall per em, starting at `origin` on the baseline and turned by
/// `angle` (radians). Empty when no font is installed.
pub fn outlines(text: &str, origin: DVec2, size: f64, angle: f64) -> Vec<Vec<DVec2>> {
    outlines_in(None, text, origin, size, angle)
}

/// [`outlines`] in a font family (the plain sans-serif when it's `None` or not installed).
pub fn outlines_in(family: Option<&str>, text: &str, origin: DVec2, size: f64, angle: f64) -> Vec<Vec<DVec2>> {
    let chosen = family.and_then(family_data);
    let (bytes, index) = match (&chosen, font_data()) {
        (Some(d), _) => (&d.0, d.1),
        (None, Some(f)) => (&f.bytes, f.index),
        (None, None) => return Vec::new(),
    };
    let Ok(font) = FontRef::from_index(bytes, index) else { return Vec::new() };
    if !(size.is_finite() && size > 0.0) {
        return Vec::new();
    }
    let px = Size::new(1000.0);
    let glyphs = font.outline_glyphs();
    let metrics = font.glyph_metrics(px, LocationRef::default());
    let charmap = font.charmap();
    let scale = size / 1000.0;
    let rot = DVec2::from_angle(angle);
    let mut pen = Flatten { contours: Vec::new(), current: Vec::new(), offset: DVec2::ZERO };
    for ch in text.chars().take(MAX_CHARS) {
        if ch == '\n' {
            pen.offset = DVec2::new(0.0, pen.offset.y - 1200.0);
            continue;
        }
        let Some(gid) = charmap.map(ch) else { continue };
        if let Some(g) = glyphs.get(gid) {
            let _ = g.draw(DrawSettings::unhinted(px, LocationRef::default()), &mut pen);
            pen.finish();
        }
        pen.offset.x += metrics.advance_width(gid).unwrap_or(500.0) as f64;
    }
    pen.contours.into_iter().map(|c| c.into_iter().map(|p| origin + rot.rotate(p * scale)).collect()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_have_closed_contours() {
        if !available() {
            return;
        }
        let o = outlines("o", DVec2::ZERO, 10.0, 0.0);
        assert_eq!(o.len(), 2, "an o is a ring");
        assert!(o.iter().all(|c| c.first().zip(c.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9)));
        let wide = outlines("ii", DVec2::ZERO, 10.0, 0.0);
        let xmax = wide.iter().flatten().map(|p| p.x).fold(f64::MIN, f64::max);
        assert!(xmax > 2.0 && xmax < 10.0, "{xmax}");
        assert!(outlines("x", DVec2::ZERO, f64::NAN, 0.0).is_empty());
    }
}

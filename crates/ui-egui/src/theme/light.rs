//! The Light theme: the measured dark → light colour pairs, the fallback for colours never measured, and the
//! mapping of drawings made in dark shades (icons, the view cube).

use egui::{Color32, Stroke};

use super::{is_light, set_light, themed};

/// Dark → Light pairs, read from the same pixels of the reference in both themes (plan/shapr3d/screenshots D-* and
/// L-*, session 23). Sorted by the dark value.
pub(super) const LIGHT: &[(u32, u32)] = &[
    (0x000000, 0xc7c7c7),
    (0x030303, 0xbfbfbf),
    (0x070707, 0xa2a2a2),
    (0x18181b, 0xf6f6f6),
    (0x191919, 0xe8e8e8),
    (0x1a1a1a, 0xededed),
    (0x1d4eff, 0x1d4fff),
    (0x1db3dc, 0x1db3dc),
    (0x1e1e1e, 0xffffff),
    (0x252424, 0xe4e2e2),
    (0x252525, 0xeae9ea),
    (0x262525, 0xe1e0e0),
    (0x2b2b2d, 0xe6e5ea),
    (0x2c2b2d, 0xf0efef),
    (0x2e68ff, 0x1d4fff),
    (0x2f5bd8, 0x1b4af3),
    (0x2f6cf6, 0x1d4fff),
    (0x302f2f, 0xd9d8d8),
    (0x303032, 0xdfdfe0),
    (0x313030, 0xd6d5d5),
    (0x323232, 0xececec),
    (0x363535, 0xf1f0f0),
    (0x373636, 0xf4f3f3),
    (0x3a3939, 0xcecdcd),
    (0x3a3a3a, 0xd3d2d3),
    (0x3a3b44, 0xdcdddc),
    (0x3a70ff, 0x1b4af3),
    (0x3d3d41, 0xd9d8dd),
    (0x3f3e3e, 0xc9c8c8),
    (0x434245, 0xcececf),
    (0x434343, 0xd3d2d3),
    (0x464646, 0xd5d5d5),
    (0x47474b, 0xd0cfd5),
    (0x4b4b4e, 0xd5d5d6),
    (0x555555, 0xecebec),
    (0x575757, 0xc1c1c1),
    (0x5a5a5e, 0xe0e0e0),
    (0x666666, 0xafafaf),
    (0x6e6e72, 0xfdfffd),
    (0x737374, 0x97979a),
    (0x9a9a9a, 0x6c6c6c),
    (0x9b9a9a, 0x6c6b6b),
    (0x9d9c9d, 0x6c6b6b),
    (0x9d9d9f, 0x737375),
    (0x9f9fa0, 0x737375),
    (0xa2a1a1, 0x616161),
    (0xa2a2a2, 0x767676),
    (0xa7a7a9, 0x6d6c6f),
    (0xc6c6c6, 0x424242),
    (0xd8d8da, 0xffffff),
    (0xdfdfdf, 0x232323),
    (0xe0e0e0, 0x242424),
    (0xe5e5e5, 0x1d1d1d),
    (0xebeaeb, 0x484747),
    (0xf2f2f4, 0xffffff),
    (0xffffff, 0x1e1e1e),
];

/// The light pair of a dark colour: measured when known, else greys turn over (dark surfaces become light ones,
/// light inks dark; fitted to the measured pairs) and colours stay.
pub(super) fn light_of(rgb: u32) -> u32 {
    if let Ok(i) = LIGHT.binary_search_by_key(&rgb, |p| p.0) {
        return LIGHT[i].1;
    }
    let (r, g, b) = ((rgb >> 16) as i32 & 255, (rgb >> 8) as i32 & 255, rgb as i32 & 255);
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    if hi - lo > 24 {
        return rgb;
    }
    let map = |v: i32| -> i32 {
        let out = if v < 0x48 {
            0xe6 + (0x2b - v) / 3
        } else if v < 0x70 {
            0xd5 - (v - 0x48)
        } else {
            262 - v
        };
        out.clamp(0x10, 0xf6)
    };
    let mean = (r + g + b) / 3;
    let shift = map(mean) - mean;
    let ch = |v: i32| (v + shift).clamp(0, 255) as u32;
    (ch(r) << 16) | (ch(g) << 8) | ch(b)
}

/// The dark colour a theme colour came from (exact for measured pairs; light greys turn back over).
pub fn dark_of(color: Color32) -> Color32 {
    if !is_light() {
        return color;
    }
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let rgb = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
    let dark =
        LIGHT.iter().find(|p| p.1 == rgb).map_or_else(|| if r.max(g).max(b) - r.min(g).min(b) > 24 { rgb } else { light_of(rgb) }, |p| p.0);
    Color32::from_rgba_unmultiplied((dark >> 16) as u8, (dark >> 8) as u8, dark as u8, a)
}

/// Paints a drawing made in the dark design's shades (icons, the view cube) and shows it in the current theme:
/// `draw` runs with dark colours, then every shape it added takes its light pair.
pub fn paint_themed(painter: &egui::Painter, draw: impl FnOnce()) {
    if !is_light() {
        draw();
        return;
    }
    let layer = painter.layer_id();
    let start = painter.ctx().graphics_mut(|g| g.entry(layer).next_idx().0);
    set_light(false);
    draw();
    set_light(true);
    painter.ctx().graphics_mut(|g| {
        let list = g.entry(layer);
        for i in start..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(i), |cs| relight_shape(&mut cs.shape));
        }
    });
}

fn relight_shape(shape: &mut egui::Shape) {
    use egui::Shape as S;
    use egui::epaint::ColorMode;
    let stroke = |s: &mut Stroke| s.color = themed(s.color);
    let path_stroke = |s: &mut egui::epaint::PathStroke| {
        if let ColorMode::Solid(c) = &mut s.color {
            *c = themed(*c);
        }
    };
    match shape {
        S::Vec(v) => v.iter_mut().for_each(relight_shape),
        S::Circle(c) => {
            c.fill = themed(c.fill);
            stroke(&mut c.stroke);
        }
        S::Ellipse(e) => {
            e.fill = themed(e.fill);
            stroke(&mut e.stroke);
        }
        S::LineSegment { stroke: s, .. } => stroke(s),
        S::Path(p) => {
            p.fill = themed(p.fill);
            path_stroke(&mut p.stroke);
        }
        S::Band(b) => {
            b.fill = themed(b.fill);
            stroke(&mut b.stroke);
        }
        S::Rect(r) if r.brush.is_none() => {
            r.fill = themed(r.fill);
            stroke(&mut r.stroke);
        }
        S::Text(t) => {
            let c = t.override_text_color.unwrap_or_else(|| t.galley.job.sections.first().map_or(t.fallback_color, |s| s.format.color));
            t.override_text_color = Some(themed(c));
        }
        S::Mesh(m) if m.texture_id == egui::TextureId::default() => {
            for v in &mut std::sync::Arc::make_mut(m).vertices {
                v.color = themed(v.color);
            }
        }
        S::QuadraticBezier(b) => {
            b.fill = themed(b.fill);
            path_stroke(&mut b.stroke);
        }
        S::CubicBezier(b) => {
            b.fill = themed(b.fill);
            path_stroke(&mut b.stroke);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_pairs_are_sorted_and_found() {
        assert!(LIGHT.windows(2).all(|w| w[0].0 < w[1].0), "LIGHT must stay sorted by its dark value");
        assert_eq!(light_of(0x252424), 0xe4e2e2);
        // Unmeasured greys still turn over; colours stay.
        assert!(light_of(0x2e2e2e) >> 16 > 0xc0);
        assert!(light_of(0xb0b0b0) >> 16 < 0x60);
        assert_eq!(light_of(0xc8413b), 0xc8413b);
    }
}

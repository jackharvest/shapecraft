//! Design tokens. Colors and sizes are measured from the reference app (plan/shapr3d/01-observed-ui.md);
//! every panel reads from here.

mod fonts;
mod keys;
mod light;

use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, CornerRadius, Stroke};

pub use fonts::{SEMIBOLD, body, install_fonts, mono, strong};
pub use keys::{key_caps, key_text};
use light::light_of;
pub use light::{dark_of, paint_themed};

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

static LIGHT_THEME: AtomicBool = AtomicBool::new(false);

/// Preferences ▸ Theme: Light shows every colour as its light pair.
pub fn set_light(on: bool) {
    LIGHT_THEME.store(on, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT_THEME.load(Ordering::Relaxed)
}

/// A colour of the dark design (measured), as the current theme shows it.
pub fn c(rgb: u32) -> Color32 {
    if is_light() { hex(light_of(rgb)) } else { hex(rgb) }
}

/// A colour both themes show as is: icon parts that the reference keeps the same in Light (the shaded Display
/// Modes lump). Marked by an alpha of 254, which [`themed`] leaves alone.
pub fn fixed(rgb: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 254)
}

/// [`c`] for a colour with alpha (the alpha is kept).
pub fn themed(dark: Color32) -> Color32 {
    if !is_light() {
        return dark;
    }
    let [r, g, b, a] = dark.to_srgba_unmultiplied();
    if a == 254 {
        return dark;
    }
    let l = light_of(((r as u32) << 16) | ((g as u32) << 8) | b as u32);
    Color32::from_rgba_unmultiplied((l >> 16) as u8, (l >> 8) as u8, l as u8, a)
}

pub struct Tokens;

/// The colour tokens: each is its dark-theme value (measured), and the Light theme shows its measured pair (`light.rs`).
macro_rules! tokens {
    ($($(#[$m:meta])* $name:ident = $dark:literal;)*) => {
        impl Tokens {
            $($(#[$m])* pub fn $name() -> Color32 { c($dark) })*
        }
    };
}

tokens! {
    window = 0x191919;
    titlebar = 0x363535;
    viewport = 0x1a1a1a;
    panel = 0x252424;
    panel_border = 0x434245;
    /// The line over the Items footer (measured, L02).
    divider = 0x3a3939;
    field = 0x333233;
    /// An Items row whose body has a face or edge selected (measured: 289).
    row_selected = 0x363539;
    row_hover = 0x302f31;
    button = 0x323232;
    /// Measured: the icon cell lightens to this under the pointer; the label chip to CHIP_HOVER.
    button_hover = 0x464646;
    chip_hover = 0x3d3d41;
    button_active = 0x575757;
    chip = 0x2b2b2d;
    chip_border = 0x3a3a3c;
    /// A toolbar label chip's outline (measured, L05).
    chip_edge = 0x47474b;
    /// Toolbar column containers and the short lines between their groups (measured, L02/L05).
    tool_border = 0x4b4b4e;
    tool_separator = 0x515151;
    keycap = 0x3d3d41;
    banner = 0x2c2b2d;
    pill = 0x3a3a3c;
    landing_sidebar = 0x262525;
    card = 0x1a1a1a;
    card_border = 0x303032;
    /// The reference's text is never pure white (labels, list names, banners and dialog titles all read #DFDFDF);
    /// only text on blue buttons is white.
    text = 0xdfdfdf;
    text_strong = 0xdfdfdf;
    text_dim = 0x9d9d9f;
    text_faint = 0x6e6e70;
    /// Disabled labels (measured, s23 flyout: "No active plane", "Delete").
    text_ghost = 0x737374;
    accent = 0x2f6cf6;
    select_cyan = 0x1db3dc;
    select_orange = 0xc37b14;
    sketch_blue = 0x1d4eff;
    icon = 0xe0e0e0;
    icon_blue = 0x4f8cff;
    danger = 0xe5534b;
}

impl Tokens {
    // Geometry (points)
    pub const TITLEBAR_H: f32 = 52.0;
    pub const GAP: f32 = 8.0;
    pub const SIDEBAR_W: f32 = 300.0;
    pub const HISTORY_W: f32 = 280.0;
    pub const PANEL_RADIUS: u8 = 10;
    pub const TOOL_BUTTON: f32 = 44.0;
    pub const TOOL_PITCH: f32 = 40.0;
    pub const TOOL_RADIUS: u8 = 9;
    pub const CHIP_H: f32 = 36.0;
    pub const CHIP_RADIUS: u8 = 6;
    pub const ROW_H: f32 = 24.0;
}

pub fn radius(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

pub fn hairline(c: Color32) -> Stroke {
    Stroke::new(1.0, c)
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|s| {
        s.visuals = if is_light() { egui::Visuals::light() } else { egui::Visuals::dark() };
        s.visuals.panel_fill = Tokens::window();
        s.visuals.window_fill = Tokens::panel();
        s.visuals.extreme_bg_color = Tokens::field();
        s.visuals.override_text_color = Some(Tokens::text());
        s.visuals.selection.bg_fill = Tokens::accent();
        s.visuals.widgets.inactive.bg_fill = Tokens::button();
        s.visuals.widgets.inactive.weak_bg_fill = Tokens::button();
        s.visuals.widgets.hovered.bg_fill = Tokens::button_hover();
        s.visuals.widgets.hovered.weak_bg_fill = Tokens::button_hover();
        s.visuals.widgets.active.bg_fill = Tokens::button_active();
        s.visuals.window_corner_radius = radius(Tokens::PANEL_RADIUS);
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.text_styles.insert(egui::TextStyle::Body, body(13.0));
        s.text_styles.insert(egui::TextStyle::Button, body(13.0));
        s.text_styles.insert(egui::TextStyle::Small, body(10.5));
        s.text_styles.insert(egui::TextStyle::Heading, strong(17.0));
    });
}

/// `text` cut short with "…" so it fits in `max_width` on one line (names in the Items and History lists, as the
/// reference does, s36). Whole when it fits.
pub fn elide(painter: &egui::Painter, text: &str, font: egui::FontId, max_width: f32) -> String {
    let width = |s: String| painter.layout_no_wrap(s, font.clone(), egui::Color32::WHITE).size().x;
    if max_width <= 0.0 {
        return String::new();
    }
    if width(text.to_string()) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let s: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
        if width(s) <= max_width { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
}

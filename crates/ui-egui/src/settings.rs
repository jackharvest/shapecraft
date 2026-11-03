//! App preferences (the Preferences window and Navigation Presets), saved as JSON in the user's config folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Name of the navigation preset (see [`PRESETS`]).
    pub nav_preset: String,
    pub reverse_scroll: bool,
    /// Adding to the selection needs Shift held (otherwise every click adds).
    pub extend_needs_shift: bool,
    /// Preferences ▸ Theme: "System", "Light" or "Dark".
    pub theme: String,
    pub start_with: String,
    pub menu_labels: String,
    pub circular_annotations: String,
    pub single_key_action: String,
    /// Where a running tool's Done / Cancel show: "Bottom of View" (26.200), "Toolbar" (the classic ✓ / ✕ cells
    /// in the left column, 26.142 and earlier) or "Both".
    pub confirm_buttons: String,
    /// Preferences ▸ ShapeCraft ▸ Graphics: a name from [`crate::graphics::choices`]. Read at startup.
    pub graphics: String,
    /// Preferences ▸ ShapeCraft ▸ GPU: [`crate::graphics::BEST_GPU`] or a GPU's name. Read at startup.
    pub gpu: String,
    /// Preferences ▸ ShapeCraft: flip each direction of orbiting and panning, for every input device.
    pub orbit_invert_x: bool,
    pub orbit_invert_y: bool,
    pub pan_invert_x: bool,
    pub pan_invert_y: bool,
    pub rendering_quality: String,
    pub tessellation_quality: String,
    pub always_advanced_import: bool,
    pub tutorial_clicks: bool,
    pub tutorial_keys: bool,
    /// The View popover's Appearance tab: camera animations, and the field of view (0° = always orthographic).
    pub animate_camera: bool,
    pub field_of_view: f64,
    /// The Items sidebar's width; its right edge drags between 200 and 500 (as observed).
    pub items_width: f32,
    /// Constraint Settings (sketching): auto-constraining, the badges of every constraint (not just the selected
    /// elements'), locked dimensions, and which picked element stays put when constraining (the first or last).
    pub auto_constrain: bool,
    pub always_show_constraints: bool,
    pub always_show_dimensions: bool,
    pub anchor_last: bool,
    /// The Parameter Card's ⋯ menu (s29-013): open it when a tool starts, and when a History step is edited; and
    /// where it was last dragged (its top-left corner, points), kept for every card.
    pub card_on_tool_start: bool,
    pub card_on_history_edit: bool,
    pub card_at: Option<[f32; 2]>,
    /// Preferences ▸ Language: a name from [`crate::i18n::LANGUAGES`].
    pub language: String,
    /// Keyboard Shortcuts' customised keys: (command label, keys as shown, e.g. "⌃⌥D").
    pub custom_keys: Vec<(String, String)>,
    /// File ▸ Export to ▸ Quick Export to Favorite Formats: the formats of the last export (their list names) and
    /// the folder they went to.
    pub favorite_formats: Vec<String>,
    /// The first run's answer (onboarding): "mouse" or "trackpad"; empty until asked.
    pub device: String,
    /// The first run's second question (navigation style) has been answered or skipped.
    pub nav_asked: bool,
    pub last_export_dir: Option<String>,
    /// The in-app tutorials finished (their ids), ticked on their cards.
    pub tutorials_done: Vec<String>,
    /// Preferences ▸ ShapeCraft ▸ Updates: look for a new version at start and once a day, download it in the
    /// background, and what closing ShapeCraft does with a downloaded update ("Ask" or "Always Update").
    pub update_check: bool,
    pub update_download: bool,
    pub update_on_close: String,
    /// A version passed over with Skip This Version (`0.4.1`); a newer one is offered again.
    pub update_skip: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            nav_preset: "Blender".into(),
            reverse_scroll: true,
            extend_needs_shift: true,
            theme: "System".into(),
            start_with: "Recents".into(),
            menu_labels: "Always".into(),
            circular_annotations: "Radius and Diameter".into(),
            single_key_action: "Hotkeys".into(),
            confirm_buttons: "Bottom of View".into(),
            graphics: crate::graphics::AUTOMATIC.into(),
            gpu: crate::graphics::BEST_GPU.into(),
            orbit_invert_x: false,
            orbit_invert_y: false,
            pan_invert_x: false,
            pan_invert_y: false,
            rendering_quality: "High".into(),
            tessellation_quality: "High (Default)".into(),
            always_advanced_import: true,
            tutorial_clicks: false,
            tutorial_keys: false,
            animate_camera: true,
            field_of_view: 45.0,
            items_width: 300.0,
            auto_constrain: true,
            always_show_constraints: false,
            always_show_dimensions: true,
            anchor_last: false,
            card_on_tool_start: false,
            card_on_history_edit: true,
            card_at: None,
            language: "English".into(),
            favorite_formats: Vec::new(),
            device: String::new(),
            nav_asked: false,
            last_export_dir: None,
            custom_keys: Vec::new(),
            tutorials_done: Vec::new(),
            update_check: true,
            update_download: true,
            update_on_close: "Ask".into(),
            update_skip: String::new(),
        }
    }
}

impl Settings {
    /// Multipliers for an orbit's x and y movement.
    pub fn orbit_signs(&self) -> (f64, f64) {
        (if self.orbit_invert_x { -1.0 } else { 1.0 }, if self.orbit_invert_y { -1.0 } else { 1.0 })
    }

    /// Multipliers for a pan's x and y movement.
    pub fn pan_signs(&self) -> (f64, f64) {
        (if self.pan_invert_x { -1.0 } else { 1.0 }, if self.pan_invert_y { -1.0 } else { 1.0 })
    }

    pub fn default_path() -> Option<PathBuf> {
        let dir = dirs::config_dir()?;
        let path = dir.join("ShapeCraft").join("settings.json");
        // Settings saved before the rename (as Shayper3D) are copied over once.
        let old = dir.join("Shayper3D").join("settings.json");
        if !path.exists() && old.exists() && std::fs::create_dir_all(dir.join("ShapeCraft")).is_ok() {
            let _ = std::fs::copy(&old, &path);
        }
        Some(path)
    }

    /// Reads the settings, falling back to the defaults when the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        let mut s: Self = std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // Our own presets were named after the app before the rename ("Shayper3D Default").
        if let Some(rest) = s.nav_preset.strip_prefix("Shayper3D ") {
            s.nav_preset = format!("ShapeCraft {rest}");
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        // Through a second file: being cut short here would otherwise reset every preference on the next start.
        let next = path.with_extension("json.tmp");
        std::fs::write(&next, text)?;
        shapecraft_doc::project::replace(&next, path)
    }

    /// Applies the field of view to a camera. The slider's 45° (the reference's default) is drawn as our tuned
    /// 30° vertical view (a guess: the reference doesn't say which angle the slider measures).
    pub fn apply_view(&self, camera: &mut shapecraft_render::Camera) {
        let fov = if self.field_of_view.is_finite() { self.field_of_view.clamp(0.0, 90.0) } else { 45.0 };
        camera.always_ortho = fov < 0.5;
        if !camera.always_ortho {
            camera.fov_y = (fov * 2.0 / 3.0).to_radians();
        }
    }

    pub fn preset(&self) -> &'static Preset {
        PRESETS.iter().find(|p| p.name == self.nav_preset).unwrap_or(&BLENDER)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavAction {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mods {
    None,
    Shift,
    Ctrl,
    Alt,
    AltShift,
}

/// One mouse gesture: a button dragged with modifiers held.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub button: Button,
    pub mods: Mods,
    pub action: NavAction,
}

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub name: &'static str,
    /// The devices it covers, as listed under the name.
    pub devices: &'static str,
    /// Listed under "ShapeCraft Presets" (else "Popular Presets").
    pub own: bool,
    pub recommended: bool,
    pub mouse: &'static [Binding],
}

const fn b(button: Button, mods: Mods, action: NavAction) -> Binding {
    Binding { button, mods, action }
}

use Button::{Left, Middle, Right};
use Mods::{Alt, AltShift, Ctrl, Shift};
use NavAction::{Orbit, Pan, Zoom};

const BLENDER: Preset = Preset {
    name: "Blender",
    devices: "Standard Mouse, Trackpad",
    own: false,
    recommended: false,
    mouse: &[b(Middle, Mods::None, Orbit), b(Middle, Shift, Pan), b(Middle, Ctrl, Zoom), b(Right, Mods::None, Orbit), b(Right, Shift, Pan)],
};

/// The presets in the reference's order. Mouse bindings follow each source app's own conventions (Blender's was
/// observed in the reference's diagram; our own two are a guess until observed).
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "ShapeCraft Default",
        devices: "Standard Mouse, Magic Mouse, Trackpad",
        own: true,
        recommended: true,
        mouse: &[b(Right, Mods::None, Orbit), b(Middle, Mods::None, Pan), b(Right, Shift, Pan)],
    },
    Preset {
        name: "ShapeCraft Classic",
        devices: "Standard Mouse, Magic Mouse, Trackpad",
        own: true,
        recommended: false,
        mouse: &[b(Right, Mods::None, Orbit), b(Middle, Mods::None, Pan)],
    },
    Preset {
        name: "Alias",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Left, AltShift, Orbit), b(Middle, AltShift, Pan), b(Right, AltShift, Zoom)],
    },
    Preset {
        name: "Catia",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Middle, Mods::None, Pan), b(Middle, Ctrl, Orbit), b(Right, Ctrl, Zoom)],
    },
    Preset {
        name: "NX",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Middle, Mods::None, Orbit), b(Middle, Shift, Pan), b(Middle, Ctrl, Zoom)],
    },
    BLENDER,
    Preset {
        name: "Plasticity",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Right, Mods::None, Orbit), b(Right, Shift, Pan), b(Right, Ctrl, Zoom)],
    },
    Preset {
        name: "Fusion 360",
        devices: "Standard Mouse, Trackpad",
        own: false,
        recommended: false,
        mouse: &[b(Middle, Mods::None, Pan), b(Middle, Shift, Orbit)],
    },
    Preset {
        name: "SolidWorks",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Middle, Mods::None, Orbit), b(Middle, Ctrl, Pan), b(Middle, Shift, Zoom)],
    },
    Preset {
        name: "Onshape",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Right, Mods::None, Orbit), b(Middle, Mods::None, Pan)],
    },
    Preset {
        name: "Rhino",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Right, Mods::None, Orbit), b(Right, Shift, Pan), b(Right, Ctrl, Zoom)],
    },
    Preset {
        name: "SketchUp",
        devices: "Standard Mouse",
        own: false,
        recommended: false,
        mouse: &[b(Middle, Mods::None, Orbit), b(Middle, Shift, Pan)],
    },
];

impl Preset {
    /// What a drag with this button and these modifiers does, if anything.
    pub fn action(&self, button: Button, shift: bool, ctrl: bool, alt: bool) -> Option<NavAction> {
        let mods = match (shift, ctrl, alt) {
            (true, false, true) => AltShift,
            (_, _, true) => Alt,
            (_, true, _) => Ctrl,
            (true, _, _) => Shift,
            _ => Mods::None,
        };
        self.mouse.iter().find(|m| m.button == button && m.mods == mods).map(|m| m.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blender_is_the_default_and_maps_like_the_reference() {
        let s = Settings::default();
        let p = s.preset();
        assert_eq!(p.name, "Blender");
        assert_eq!(p.action(Button::Middle, false, false, false), Some(NavAction::Orbit));
        assert_eq!(p.action(Button::Middle, true, false, false), Some(NavAction::Pan));
        assert_eq!(p.action(Button::Middle, false, true, false), Some(NavAction::Zoom));
        assert_eq!(p.action(Button::Left, false, false, false), None);
    }

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"nav_preset": "Fusion 360"}"#).unwrap_or_default();
        assert_eq!(s.preset().name, "Fusion 360");
        assert!(s.reverse_scroll);
        assert_eq!(s.orbit_signs(), (1.0, 1.0));
        assert_eq!(Settings { orbit_invert_y: true, pan_invert_x: true, ..s.clone() }.orbit_signs(), (1.0, -1.0));
        assert_eq!(Settings { orbit_invert_y: true, pan_invert_x: true, ..s.clone() }.pan_signs(), (-1.0, 1.0));
    }

    #[test]
    fn presets_saved_under_the_old_app_name_still_load() {
        let dir = std::env::temp_dir().join(format!("shapecraft-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, r#"{"nav_preset": "Shayper3D Classic"}"#);
        assert_eq!(Settings::load(&path).preset().name, "ShapeCraft Classic");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

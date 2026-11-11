//! Shortcuts are written the macOS way throughout (⇧⌘F, as the reference writes them). These turn them into what the
//! keyboard says everywhere else: Ctrl, Alt and Shift, in the Windows order.

/// The caps of a shortcut, in the order to show them: "⇧⌘F" is `["⇧", "⌘", "F"]` here on macOS, else `["Ctrl", "Shift", "F"]`.
/// None on a touch screen with no keyboard (`tablet::shows_shortcuts`).
pub fn key_caps(keys: &str) -> Vec<String> {
    caps_if(keys, crate::tablet::shows_shortcuts())
}

fn caps_if(keys: &str, shown: bool) -> Vec<String> {
    if !shown {
        return Vec::new();
    }
    if cfg!(target_os = "macos") { keys.chars().map(String::from).collect() } else { caps_elsewhere(keys) }
}

/// A shortcut as text inside a sentence or a field: "⇧⌘F" here on macOS, else "Ctrl+Shift+F".
pub fn key_text(keys: &str) -> String {
    if cfg!(target_os = "macos") { keys.to_string() } else { caps_elsewhere(keys).join("+") }
}

fn caps_elsewhere(keys: &str) -> Vec<String> {
    let held = |glyphs: &[char]| keys.contains(glyphs);
    let mods = [(held(&['⌘', '⌃']), "Ctrl"), (held(&['⌥']), "Alt"), (held(&['⇧']), "Shift")];
    let rest = keys.replace(['⌘', '⌃', '⌥', '⇧', ' '], "");
    let mut caps: Vec<String> = mods.iter().filter(|m| m.0).map(|m| m.1.to_string()).collect();
    if !rest.is_empty() {
        caps.push(rest);
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_shortcuts_have_no_caps() {
        assert!(caps_if("⌘N", false).is_empty());
        assert_eq!(caps_if("⌘N", true).len(), 2);
    }

    #[test]
    fn modifiers_read_as_on_the_keyboard() {
        assert_eq!(caps_elsewhere("⇧⌘F"), ["Ctrl", "Shift", "F"]);
        assert_eq!(caps_elsewhere("⌥⌘R"), ["Ctrl", "Alt", "R"]);
        assert_eq!(caps_elsewhere("⌃⌥D"), ["Ctrl", "Alt", "D"]);
        assert_eq!(caps_elsewhere("⌘ B"), ["Ctrl", "B"]);
        assert_eq!(caps_elsewhere("⇧A"), ["Shift", "A"]);
        assert_eq!(caps_elsewhere("⌘"), ["Ctrl"]);
        assert_eq!(caps_elsewhere("L"), ["L"]);
        assert!(caps_elsewhere("").is_empty());
    }
}

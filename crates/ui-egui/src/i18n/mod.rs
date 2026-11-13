//! Interface languages (Preferences ▸ Language; the reference is localized into the eleven languages its manual
//! lists). Text is drawn through [`tr`], keyed by the English text, so the English strings stay the identifiers the
//! code compares; anything without a translation shows in English.

use std::sync::atomic::{AtomicUsize, Ordering};

mod tables;

/// The languages, by the names the Language menu shows (each in its own language), English first.
pub const LANGUAGES: &[&str] = &[
    "English",
    "简体中文",
    "繁體中文",
    "Deutsch",
    "Español",
    "Français",
    "Italiano",
    "日本語",
    "한국어",
    "Português (Brasil)",
    "Русский",
    "Slovenčina",
];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn table(i: usize) -> &'static [(&'static str, &'static str)] {
    match i {
        1 => tables::ZH_HANS,
        2 => tables::ZH_HANT,
        3 => tables::DE,
        4 => tables::ES,
        5 => tables::FR,
        6 => tables::IT,
        7 => tables::JA,
        8 => tables::KO,
        9 => tables::PT_BR,
        10 => tables::RU,
        11 => tables::SK,
        _ => &[],
    }
}

/// Switches the interface to a language by its [`LANGUAGES`] name (or its English name, e.g. "German"); unknown
/// names leave it as it was. Returns whether the name was known.
pub fn set(name: &str) -> bool {
    const ENGLISH_NAMES: &[&str] = &[
        "english",
        "chinese (simplified)",
        "chinese (traditional)",
        "german",
        "spanish",
        "french",
        "italian",
        "japanese",
        "korean",
        "portuguese (brazil)",
        "russian",
        "slovak",
    ];
    let lower = name.trim().to_lowercase();
    let found = LANGUAGES.iter().position(|l| l.to_lowercase() == lower).or_else(|| ENGLISH_NAMES.iter().position(|l| *l == lower));
    if let Some(i) = found {
        CURRENT.store(i, Ordering::Relaxed);
    }
    found.is_some()
}

/// The current language's [`LANGUAGES`] name.
pub fn current() -> &'static str {
    LANGUAGES.get(CURRENT.load(Ordering::Relaxed)).copied().unwrap_or("English")
}

/// [`tr`] for text that comes as a `String` or `&String` (a label picked at run time); the result is owned.
pub fn tr_s(english: impl AsRef<str>) -> String {
    tr(english.as_ref()).to_owned()
}

/// `english` in the current language (as it is when there's no translation).
pub fn tr(english: &str) -> &str {
    let t = table(CURRENT.load(Ordering::Relaxed));
    match t.binary_search_by(|(k, _)| (*k).cmp(english)) {
        Ok(i) => t[i].1,
        Err(_) => english,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_is_sorted_and_has_the_same_keys() {
        let de = table(3);
        assert!(!de.is_empty());
        for i in 1..LANGUAGES.len() {
            let t = table(i);
            assert!(t.windows(2).all(|w| w[0].0 < w[1].0), "{} isn't sorted", LANGUAGES[i]);
            assert_eq!(t.iter().map(|p| p.0).collect::<Vec<_>>(), de.iter().map(|p| p.0).collect::<Vec<_>>(), "{}", LANGUAGES[i]);
            assert!(t.iter().all(|p| !p.1.trim().is_empty()), "{} has an empty translation", LANGUAGES[i]);
        }
    }

    #[test]
    fn switching_translates_and_falls_back_to_english() {
        assert!(set("Deutsch"));
        assert_ne!(tr("Sketch"), "Sketch");
        assert_eq!(tr("no such text"), "no such text");
        assert!(set("german") && current() == "Deutsch");
        assert!(!set("Klingon") && current() == "Deutsch");
        assert!(set("English"));
        assert_eq!(tr("Sketch"), "Sketch");
    }
}

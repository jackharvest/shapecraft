//! Export ▸ Slicer: which slicers are installed here, and handing a 3MF to one of them.
//!
//! Only installed slicers are offered (looked up once per run), plus the system's default app for 3MF files, which
//! always works and catches slicers we don't know by name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A slicer and where it lives: macOS app names, Linux commands on PATH, Windows paths under Program Files or
/// %LOCALAPPDATA%\Programs (a `*` at the end of the first folder matches version-numbered folders).
struct Slicer {
    name: &'static str,
    mac: &'static [&'static str],
    linux: &'static [&'static str],
    windows: &'static [&'static str],
}

const KNOWN: &[Slicer] = &[
    Slicer {
        name: "Bambu Studio",
        mac: &["BambuStudio", "Bambu Studio"],
        linux: &["bambu-studio", "BambuStudio"],
        windows: &["Bambu Studio\\bambu-studio.exe"],
    },
    Slicer { name: "OrcaSlicer", mac: &["OrcaSlicer"], linux: &["orca-slicer", "OrcaSlicer"], windows: &["OrcaSlicer\\orca-slicer.exe"] },
    Slicer {
        name: "PrusaSlicer",
        mac: &["PrusaSlicer"],
        linux: &["prusa-slicer", "PrusaSlicer"],
        windows: &["Prusa3D\\PrusaSlicer\\prusa-slicer.exe"],
    },
    Slicer {
        name: "Snapmaker Orca",
        mac: &["Snapmaker Orca"],
        linux: &["snapmaker-orca", "SnapmakerOrca"],
        windows: &["Snapmaker Orca\\snapmaker-orca.exe"],
    },
    Slicer {
        name: "UltiMaker Cura",
        mac: &["UltiMaker Cura", "Ultimaker Cura"],
        linux: &["cura", "UltiMaker-Cura", "ultimaker-cura"],
        windows: &["UltiMaker Cura*\\UltiMaker-Cura.exe", "Ultimaker Cura*\\Ultimaker-Cura.exe"],
    },
    Slicer {
        name: "Creality Print",
        mac: &["Creality Print", "CrealityPrint"],
        linux: &["CrealityPrint", "creality-print"],
        windows: &["Creality\\Creality Print*\\CrealityPrint.exe", "Creality Print*\\CrealityPrint.exe"],
    },
    Slicer {
        name: "ElegooSlicer",
        mac: &["ElegooSlicer"],
        linux: &["elegoo-slicer", "ElegooSlicer"],
        windows: &["ElegooSlicer\\elegoo-slicer.exe"],
    },
];

/// The last choice: the system's default app for .3mf files.
pub const DEFAULT_APP: &str = "Default 3MF App";

/// The installed slicers (name, where), found once per run.
fn installed() -> &'static [(&'static str, PathBuf)] {
    static FOUND: OnceLock<Vec<(&'static str, PathBuf)>> = OnceLock::new();
    FOUND.get_or_init(|| KNOWN.iter().filter_map(|s| locate(s).map(|p| (s.name, p))).collect())
}

/// What the Slicer row offers: the installed slicers, then the default app.
pub fn choices() -> Vec<&'static str> {
    installed().iter().map(|(n, _)| *n).chain(std::iter::once(DEFAULT_APP)).collect()
}

/// The remembered choice when it's still installed, else the first installed slicer.
pub fn pick(remembered: &str) -> String {
    let all = choices();
    all.iter().find(|n| **n == remembered).or(all.first()).map_or(DEFAULT_APP, |n| *n).to_string()
}

/// Opens the file in the named slicer (or the default app). False when nothing could be started.
pub fn open(name: &str, file: &Path) -> bool {
    use std::process::Command;
    match installed().iter().find(|(n, _)| *n == name) {
        Some((_, app)) if cfg!(target_os = "macos") => {
            Command::new("open").arg("-a").arg(app).arg(file).status().map(|s| s.success()).unwrap_or(false)
        }
        Some((_, exe)) => Command::new(exe).arg(file).spawn().is_ok(),
        None => {
            let opener = if cfg!(target_os = "macos") {
                "open"
            } else if cfg!(windows) {
                "explorer"
            } else {
                "xdg-open"
            };
            Command::new(opener).arg(file).spawn().is_ok()
        }
    }
}

fn locate(s: &Slicer) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let roots: Vec<PathBuf> = std::iter::once(PathBuf::from("/Applications")).chain(home.map(|h| h.join("Applications"))).collect();
        s.mac.iter().flat_map(|n| roots.iter().map(move |r| r.join(format!("{n}.app")))).find(|p| p.is_dir())
    } else if cfg!(windows) {
        let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .chain(std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs")))
            .collect();
        s.windows.iter().flat_map(|rel| roots.iter().filter_map(move |r| windows_path(r, rel))).next()
    } else {
        let path = std::env::var_os("PATH")?;
        s.linux.iter().flat_map(|c| std::env::split_paths(&path).map(move |d| d.join(c))).find(|p| p.is_file())
    }
}

/// `root\rel`, where a first folder ending in `*` matches any folder starting with the rest.
fn windows_path(root: &Path, rel: &str) -> Option<PathBuf> {
    let (first, rest) = rel.split_once('\\').unwrap_or((rel, ""));
    let Some(prefix) = first.strip_suffix('*') else {
        let p = root.join(rel.replace('\\', std::path::MAIN_SEPARATOR_STR));
        return p.is_file().then_some(p);
    };
    let mut dirs: Vec<PathBuf> =
        std::fs::read_dir(root).ok()?.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(prefix)).map(|e| e.path()).collect();
    // The newest version first.
    dirs.sort();
    dirs.into_iter().rev().map(|d| d.join(rest.replace('\\', std::path::MAIN_SEPARATOR_STR))).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_is_always_offered_last() {
        let all = choices();
        assert_eq!(all.last(), Some(&DEFAULT_APP));
        assert!(all.contains(&pick("Not A Slicer").as_str()));
    }

    #[test]
    fn version_folders_match() {
        let dir = std::env::temp_dir().join(format!("sc-slicers-{}", std::process::id()));
        let exe = dir.join("UltiMaker Cura 5.9.0").join("UltiMaker-Cura.exe");
        std::fs::create_dir_all(exe.parent().unwrap_or(&dir)).ok();
        std::fs::write(&exe, b"").ok();
        assert_eq!(windows_path(&dir, "UltiMaker Cura*\\UltiMaker-Cura.exe"), Some(exe));
        assert_eq!(windows_path(&dir, "Nope*\\x.exe"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
